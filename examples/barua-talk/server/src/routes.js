// The API of `API.md`, one function per endpoint.
//
// A handler receives `{req, deviceId, params, query}` and returns
// `{status, json}`; it reports a failure by throwing an `HttpError` (or a
// `TalkError`, which `app.js` turns into one). Authentication, the device
// id, the rate limit, logging and the error shape are applied around it in
// `app.js`.
//
// Polls are answered from the database. Upstream is asked for fresher
// state only by `refreshCall`, `syncPhoneCalls` and `refreshRecording`,
// each at most once per time-to-live and once for everyone asking at the
// same moment; see "What polling costs" in `API.md`.

import { createHash, randomBytes } from 'node:crypto';

import {
  HttpError,
  TalkError,
  badRequest,
  conflict,
  forbidden,
  notFound,
  phoneUnavailable,
  talkUnavailable,
  upstreamFailed,
} from './errors.js';
import { readBody } from './http.js';
import { createSharedWork } from './stores.js';

const ROOM_LIST_LIMIT = 100;
const PEOPLE_LIMIT = 200;
const LIST_LIMIT = 50;
const ONLINE_MS = 30000;
const LIVE_ROOM_MS = 10 * 60 * 1000;
const TOUCH_MS = 10000;
// How long a person and a name are remembered without asking the database.
// A poll every two seconds should not cost a query for who is asking.
const PERSON_TTL_MS = 5 * 60 * 1000;
const NAME_TTL_MS = 30000;
const CACHE_ENTRIES = 20000;
// A chat forward must not keep the person who posted waiting.
const FORWARD_TIMEOUT_MS = 5000;
// A row set down for a request that never finished is given up after this.
const STALE_PENDING_MS = 2 * 60 * 1000;
// A call nobody is polling is still looked at this often.
const UNWATCHED_MS = 60000;

const RATES = {
  phone_call: { tzs: 300, per: 'minute', text: 'TSh 300 / minute' },
  recording: { tzs: 30, per: 'minute', text: 'TSh 30 / minute' },
};

const ROOM_NAME = /^[a-z0-9][a-z0-9-]{0,61}[a-z0-9]$/;
const CLIENT_ID = /^[A-Za-z0-9_-]{8,64}$/;
const PHONE_NUMBER = /^\+[0-9]{8,15}$/;

const CALL_STATUSES = ['ringing', 'active', 'declined', 'ended', 'missed'];
const LIVE = new Set(['ringing', 'active']);
const PHONE_LIVE = new Set(['dialing', 'ringing', 'active']);

/** `2026-11-15T07:00:00Z`: seconds, no fraction, as the contract shows it. */
function iso(date) {
  if (date === null || date === undefined) return null;
  return new Date(date).toISOString().replace(/\.\d{3}Z$/, 'Z');
}

/** A date from upstream's text, or null if it is not one. */
function date(value) {
  if (typeof value !== 'string' && typeof value !== 'number') return null;
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? null : parsed;
}

function wholeSeconds(value) {
  const n = Number(value);
  return Number.isFinite(n) && n >= 0 ? Math.round(n) : null;
}

function between(from, to) {
  return Math.max(0, Math.round((new Date(to).getTime() - new Date(from).getTime()) / 1000));
}

/**
 * An address from a name: `"Neema Mushi"` → `neema-mushi`. Letters lose
 * their accents; anything that is not a letter or a digit becomes a dash.
 */
export function slug(text, max) {
  return String(text)
    .normalize('NFKD')
    .replace(/[̀-ͯ]/g, '')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, max)
    .replace(/-+$/g, '');
}

/** `125` → `2 min 5 s`. */
export function duration(seconds) {
  const s = Math.max(0, Math.round(seconds));
  if (s < 60) return `${s} s`;
  const rest = s % 60;
  return rest === 0 ? `${Math.floor(s / 60)} min` : `${Math.floor(s / 60)} min ${rest} s`;
}

/** What upstream calls a phone call's state, in the contract's five words. */
export function phoneStatus(value, ended) {
  const word = String(value ?? '').toLowerCase().replace(/[\s-]+/g, '_');
  if (['ringing', 'dialing', 'queued', 'initiated', 'calling'].includes(word)) return 'ringing';
  if (['active', 'answered', 'in_progress', 'connected', 'ongoing'].includes(word)) return 'active';
  if (['ended', 'completed', 'done', 'finished', 'hangup'].includes(word)) return 'ended';
  if (['missed', 'no_answer', 'busy', 'declined', 'rejected', 'canceled', 'cancelled', 'unanswered'].includes(word)) return 'missed';
  if (['failed', 'error'].includes(word)) return 'failed';
  // A word this server does not know: over if upstream says when it ended.
  return ended ? 'ended' : null;
}

function recordingStatus(value) {
  const word = String(value ?? '').toLowerCase();
  return ['recording', 'processing', 'ready', 'failed'].includes(word) ? word : null;
}

async function readJson(req, limit) {
  const bytes = await readBody(req, limit, 'The request body is too large (16 KB at most).');
  if (bytes.length === 0) return {};
  let value;
  try {
    value = JSON.parse(bytes.toString('utf8'));
  } catch {
    throw badRequest('The request body must be JSON.');
  }
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw badRequest('The request body must be a JSON object.');
  }
  return value;
}

/** A trimmed string of `min..max` characters, or a 400 naming the field. */
function text(value, field, min, max) {
  if (typeof value !== 'string') throw badRequest(`"${field}" must be text.`);
  const clean = value.replace(/\s+/g, ' ').trim();
  const length = [...clean].length;
  if (length < min || length > max) {
    throw badRequest(`"${field}" must be ${min} to ${max} characters.`);
  }
  return clean;
}

function flag(value, field) {
  if (value === undefined || value === null) return false;
  if (typeof value !== 'boolean') throw badRequest(`"${field}" must be true or false.`);
  return value;
}

function clientId(value, field) {
  if (value === undefined || value === null || value === '') return null;
  if (typeof value !== 'string' || !CLIENT_ID.test(value)) {
    throw badRequest(`"${field}" must be 8 to 64 characters of A-Z, a-z, 0-9, _ or -.`);
  }
  return value;
}

function count(query, fallback, max) {
  const raw = query.get('limit');
  if (raw === null) return fallback;
  const n = Number.parseInt(raw, 10);
  if (!Number.isFinite(n) || n < 1 || n > max || String(n) !== raw) {
    throw badRequest(`"limit" must be a number from 1 to ${max}.`);
  }
  return n;
}

const enc = encodeURIComponent;
const newId = (prefix) => `${prefix}_${randomBytes(12).toString('hex')}`;

export function createRoutes({ config, db, talk, log, now }) {
  const shared = createSharedWork();
  const { limits } = config;
  const at = () => new Date(now());

  let health = { at: -Infinity, database: false };
  // What upstream's answers have said about money and phones. In memory:
  // a restart forgets it and learns it again from the next answer.
  let balance = 'unknown';
  let phoneProblem = '';
  let lastPhoneSync = -Infinity;
  const touched = new Map();
  // When each live call and recording was last asked about upstream. The
  // database says so too (`checked_at`), but a request may be holding a
  // row it read just before another request's refresh finished; without
  // this it would ask upstream again.
  const asked = new Map();
  const known = new Map(); // device id → {row, at}
  const named = new Map(); // identity → {name, at}

  function remember(row) {
    if (known.size >= CACHE_ENTRIES) known.clear();
    if (named.size >= CACHE_ENTRIES) named.clear();
    known.set(row.device_id, { row, at: now() });
    named.set(row.identity, { name: row.name || row.identity, at: now() });
  }

  /** One upstream request for an action; notes what the answer says about money. */
  async function ask(method, path, options) {
    try {
      const json = await talk.request(method, path, options);
      if (method === 'POST' && (path === '/calls' || path === '/recordings')) balance = 'ok';
      return json;
    } catch (err) {
      if (err instanceof TalkError && err.kind === 'http' && err.status === 402) balance = 'empty';
      throw err;
    }
  }

  // ---- people -----------------------------------------------------------

  /**
   * The person behind a device, given an identity if they have none yet
   * (`guest-` and six hex digits), and marked as seen.
   */
  async function person(deviceId) {
    const time = now();
    const hit = known.get(deviceId);
    let row = hit && time - hit.at < PERSON_TTL_MS ? hit.row : await db.getPerson(deviceId);
    if (row && row.identity) {
      if (!hit || hit.row !== row) remember(row);
      if (time - (touched.get(deviceId) ?? -Infinity) >= TOUCH_MS) {
        if (touched.size >= CACHE_ENTRIES) touched.clear();
        touched.set(deviceId, time);
        await db.touchPerson(deviceId, new Date(time));
      }
      return row;
    }
    const derived = createHash('sha256').update(deviceId).digest('hex').slice(0, 6);
    for (let attempt = 0; attempt < 6; attempt += 1) {
      const identity = `guest-${attempt === 0 ? derived : randomBytes(3).toString('hex')}`;
      row = await db.upsertPerson(deviceId, { identity, at: new Date(time) });
      if (row) {
        touched.set(deviceId, time);
        remember(row);
        return row;
      }
    }
    throw new Error('could not find a free guest identity');
  }

  /**
   * A lookup from identity to the name to show for it. Names are
   * remembered for half a minute: a new name reaches other people's
   * screens within that.
   */
  async function namesOf(identities) {
    const time = now();
    const wanted = [...new Set(identities.filter(Boolean))];
    const missing = wanted.filter((identity) => !(named.has(identity) && time - named.get(identity).at < NAME_TTL_MS));
    if (missing.length > 0) {
      if (named.size >= CACHE_ENTRIES) named.clear();
      for (const row of await db.peopleByIdentity(missing)) named.set(row.identity, { name: row.name || row.identity, at: time });
    }
    return (identity) => ({ identity, name: named.get(identity)?.name ?? identity });
  }

  // ---- shapes -----------------------------------------------------------

  function messageOut(row, me, who) {
    return {
      id: row.id,
      from: row.from_identity,
      from_name: who(row.from_identity).name,
      mine: row.from_identity === me.identity,
      text: row.text,
      created_at: iso(row.created_at),
      delivered: Boolean(row.delivered),
    };
  }

  function roomOut(row, me, who, recordings) {
    const running = recordings.find((r) => r.target_room === row.name && r.room === row.name);
    return {
      name: row.name,
      display_name: row.display_name,
      created_by: who(row.created_by),
      mine: row.created_by === me.identity,
      member: Boolean(row.member),
      members: row.members,
      unread: row.unread,
      last_message:
        row.last_id === null
          ? null
          : messageOut(
              { id: row.last_id, from_identity: row.last_from, text: row.last_text, created_at: row.last_at, delivered: row.last_delivered },
              me,
              who,
            ),
      active_at: iso(row.active_at),
      live: now() - new Date(row.active_at).getTime() <= LIVE_ROOM_MS,
      recording: running ? { id: running.id, status: 'recording' } : null,
      created_at: iso(row.created_at),
    };
  }

  function callSeconds(row) {
    if (row.status === 'active' && row.answered_at) return between(row.answered_at, now());
    return row.seconds ?? 0;
  }

  function callOut(row, me, who, recordings) {
    const outgoing = row.from_identity === me.identity;
    const running = recordings.find((r) => r.call_id === row.id);
    return {
      id: row.id,
      direction: outgoing ? 'outgoing' : 'incoming',
      video: Boolean(row.video),
      status: row.status,
      peer: who(outgoing ? row.to_identity : row.from_identity),
      created_at: iso(row.created_at),
      answered_at: iso(row.answered_at),
      ended_at: iso(row.ended_at),
      seconds: callSeconds(row),
      recording: running ? { id: running.id, status: 'recording' } : null,
    };
  }

  function phoneSeconds(row) {
    if (row.status === 'active' && row.started_at) return between(row.started_at, now());
    return row.seconds ?? 0;
  }

  function phoneOut(row) {
    const status = row.status === 'dialing' ? 'ringing' : row.status;
    return {
      id: row.id,
      to: row.to_number,
      caller_id: row.caller_id ?? (config.phone.callerId || null),
      status,
      created_at: iso(row.created_at),
      started_at: iso(row.started_at),
      ended_at: iso(row.ended_at),
      seconds: phoneSeconds(row),
      price_tzs: row.price_tzs ?? null,
      rate: RATES.phone_call,
      can_hang_up: config.phone.hangup === 'delete',
    };
  }

  function recordingOut(row, who) {
    const status = row.status === 'starting' ? 'recording' : row.status;
    return {
      id: row.id,
      room: row.room,
      call: row.call_id,
      status,
      started_by: who(row.started_by),
      created_at: iso(row.created_at),
      ready_at: iso(row.ready_at),
      seconds: status === 'recording' ? between(row.created_at, now()) : (row.seconds ?? 0),
      size_bytes: row.size_bytes ?? null,
      price_tzs: row.price_tzs ?? null,
      url: status === 'ready' ? (row.url ?? null) : null,
      rate: RATES.recording,
    };
  }

  // ---- rooms ------------------------------------------------------------

  async function roomOr404(name) {
    const room = await db.getRoom(name);
    if (!room) throw notFound('There is no room with that name.');
    return room;
  }

  /** The room as `me` sees it, in the contract's shape. */
  async function roomView(name, me) {
    const [rows, recordings] = await Promise.all([db.listRooms(me.identity, { name, limit: 1 }), db.runningRecordings()]);
    if (rows.length === 0) throw notFound('There is no room with that name.');
    const who = await namesOf([rows[0].created_by, rows[0].last_from]);
    return roomOut(rows[0], me, who, recordings);
  }

  // ---- in-app calls -----------------------------------------------------

  /** What a call is over as, when this server has to say so itself. */
  const overAs = (row) => (row.status === 'active' ? 'ended' : 'missed');

  /**
   * Writes what upstream says about a call into the database and returns
   * the row. A call that is over is never brought back.
   */
  async function applyCall(row, up) {
    const time = at();
    const status = CALL_STATUSES.includes(up?.status) ? up.status : row.status;
    const patch = { status, checked_at: time };
    if (typeof up?.room === 'string' && up.room) patch.room = up.room;
    if ((status === 'active' || status === 'ended') && !row.answered_at) {
      patch.answered_at = date(up?.answeredAt) ?? (status === 'active' ? time : undefined);
    }
    if (!LIVE.has(status)) {
      patch.ended_at = date(up?.endedAt) ?? time;
      const answered = patch.answered_at ?? row.answered_at;
      patch.seconds = wholeSeconds(up?.seconds) ?? (status === 'ended' && answered ? between(answered, patch.ended_at) : 0);
    }
    const updated = await db.updateCall(row.id, patch, ['ringing', 'active']);
    const fresh = updated ?? (await db.getCall(row.id)) ?? row;
    if (!LIVE.has(fresh.status)) await stopRecordingsOf(fresh.id);
    return fresh;
  }

  /**
   * Ends a call: tells upstream, then records it as over whatever upstream
   * answered. The person who hung up has left; a call upstream could not
   * be told about ends there by itself (the platform notices a departure).
   */
  async function endCall(row) {
    if (!LIVE.has(row.status)) return row;
    let up = null;
    try {
      const json = await talk.request('DELETE', `/app-calls/${enc(row.id)}`);
      up = json.call ?? json;
    } catch (err) {
      if (!(err instanceof TalkError)) throw err;
      log.warn(`hang-up of a call was not confirmed upstream (${err.kind} ${err.status || ''})`);
    }
    const said = CALL_STATUSES.includes(up?.status) && !LIVE.has(up.status) ? up : { ...up, status: overAs(row) };
    return applyCall(row, { ...said, answeredAt: null, endedAt: null });
  }

  /**
   * The call, made fresh if it is live and the server's picture of it is
   * older than the time-to-live. Never fails because of upstream: a poll
   * that cannot be refreshed is answered with what the server has.
   */
  async function refreshCall(row, ttl) {
    if (!LIVE.has(row.status)) return row;
    const time = now();
    if (row.status === 'ringing' && time - new Date(row.created_at).getTime() >= config.calls.ringTimeoutMs) {
      return shared(`hangup:${row.id}`, () => endCall(row));
    }
    const limit = ttl ?? (row.status === 'ringing' ? config.talk.ringingTtlMs : config.talk.activeTtlMs);
    const key = `call:${row.id}`;
    if (time - (asked.get(key) ?? -Infinity) < limit) {
      // Refreshed a moment ago by another request: this row may predate it.
      return (await db.getCall(row.id)) ?? row;
    }
    const age = time - new Date(row.checked_at).getTime();
    if (!talk.enabled || age < limit || talk.wait('refresh') > 0) return row;
    return shared(key, async () => {
      if (asked.size >= CACHE_ENTRIES) asked.clear();
      asked.set(key, now());
      try {
        const json = await talk.request('GET', `/app-calls/${enc(row.id)}`, { priority: 'refresh' });
        return await applyCall(row, json.call ?? json);
      } catch (err) {
        if (!(err instanceof TalkError)) throw err;
        // Upstream no longer knows it: it is over.
        if (err.kind === 'http' && err.status === 404) return applyCall(row, { status: overAs(row) });
        // Not now. Do not ask again on the very next poll.
        return (await db.updateCall(row.id, { checked_at: at() }, ['ringing', 'active'])) ?? row;
      }
    });
  }

  /** The call if `me` is one of its two people; otherwise 404. */
  async function ownCall(id, me) {
    const row = await db.getCall(id);
    if (!row || (row.from_identity !== me.identity && row.to_identity !== me.identity)) {
      throw notFound('There is no such call.');
    }
    return row;
  }

  async function callView(row, me) {
    const [who, recordings] = await Promise.all([namesOf([row.from_identity, row.to_identity]), db.runningRecordings()]);
    return callOut(row, me, who, recordings);
  }

  /** `ownCall` and the running recordings in one round trip. */
  async function ownCallAndRecordings(id, me) {
    const [row, recordings] = await Promise.all([db.getCall(id), db.runningRecordings()]);
    if (!row || (row.from_identity !== me.identity && row.to_identity !== me.identity)) {
      throw notFound('There is no such call.');
    }
    return { row, recordings };
  }

  async function answer(id, me, action) {
    const row = await ownCall(id, me);
    if (row.to_identity !== me.identity) throw forbidden('not_your_call', 'This call is not ringing for you.');
    if (row.status !== 'ringing') throw conflict('not_ringing', 'This call is no longer ringing.');
    let json;
    try {
      json = await ask('POST', `/app-calls/${enc(id)}`, { body: { action, identity: me.identity } });
    } catch (err) {
      if (err instanceof TalkError && err.kind === 'http' && err.status === 409) {
        // The server's picture is out of date: have the next poll ask.
        await db.updateCall(id, { checked_at: new Date(0) }, ['ringing', 'active']);
        asked.delete(`call:${id}`);
      }
      throw err;
    }
    const status = action === 'accept' ? 'active' : 'declined';
    // The moment is this server's own: it is what the call's timer runs from.
    const fresh = await applyCall(row, { ...(json.call ?? {}), status, answeredAt: null, endedAt: null });
    return { fresh, json };
  }

  // ---- phone calls ------------------------------------------------------

  async function applyPhone(row, up) {
    const ended = date(up?.endedAt);
    const status = phoneStatus(up?.status, ended) ?? row.status;
    const over = !PHONE_LIVE.has(status);
    const patch = {
      status,
      caller_id: typeof up?.from === 'string' && up.from ? up.from : undefined,
      started_at: date(up?.startedAt) ?? undefined,
      ended_at: over ? (ended ?? at()) : undefined,
      seconds: wholeSeconds(up?.seconds) ?? undefined,
      price_tzs: wholeSeconds(up?.priceTzs) ?? undefined,
    };
    return (await db.updatePhoneCall(row.id, patch)) ?? row;
  }

  /**
   * Brings every phone call that has not ended up to date with one
   * `GET /calls`, at most once per time-to-live. Never fails because of
   * upstream.
   */
  async function syncPhoneCalls(ttl = config.talk.phoneTtlMs) {
    if (!talk.enabled || now() - lastPhoneSync < ttl || talk.wait('refresh') > 0) return;
    await shared('phone-calls', async () => {
      lastPhoneSync = now();
      const live = await db.livePhoneCalls();
      if (live.length === 0) return;
      let list;
      try {
        const json = await talk.request('GET', '/calls', { priority: 'refresh' });
        list = Array.isArray(json.calls) ? json.calls : [];
      } catch (err) {
        if (!(err instanceof TalkError)) throw err;
        return;
      }
      for (const row of live) {
        const up = list.find((call) => call && call.id === row.upstream_id);
        if (up) await applyPhone(row, up);
      }
    });
  }

  async function ownPhoneCall(id, me) {
    const row = await db.getPhoneCall(id);
    if (!row || row.identity !== me.identity) throw notFound('There is no such phone call.');
    return row;
  }

  // ---- recordings -------------------------------------------------------

  async function applyRecording(row, up, { link = false } = {}) {
    const time = at();
    const status = recordingStatus(up?.status) ?? row.status;
    const patch = {
      status,
      checked_at: time,
      seconds: wholeSeconds(up?.seconds) ?? undefined,
      size_bytes: wholeSeconds(up?.sizeBytes) ?? undefined,
      price_tzs: wholeSeconds(up?.priceTzs) ?? undefined,
      ready_at: date(up?.readyAt) ?? (status === 'ready' && !row.ready_at ? time : undefined),
    };
    if (typeof up?.url === 'string' && up.url) {
      patch.url = up.url;
      patch.url_at = time;
    } else if (link) {
      patch.url_at = time;
    }
    return (await db.updateRecording(row.id, patch)) ?? row;
  }

  /**
   * The recording, made fresh if it is still being made and the server's
   * picture is older than the time-to-live, or (with `link`) if it is
   * ready and its download link is getting old.
   */
  async function refreshRecording(row, { link = false } = {}) {
    if (!row.upstream_id || !talk.enabled) return row;
    const time = now();
    const settling = row.status === 'recording' || row.status === 'processing';
    const stale = settling && time - new Date(row.checked_at).getTime() >= config.talk.recordingTtlMs;
    const oldLink =
      link && row.status === 'ready' && time - new Date(row.url_at ?? 0).getTime() >= config.talk.recordingUrlTtlMs;
    if (!stale && !oldLink) return row;
    const key = `recording:${row.id}`;
    if (time - (asked.get(key) ?? -Infinity) < config.talk.recordingTtlMs) {
      return (await db.getRecording(row.id)) ?? row;
    }
    if (talk.wait('refresh') > 0) return row;
    return shared(key, async () => {
      if (asked.size >= CACHE_ENTRIES) asked.clear();
      asked.set(key, now());
      try {
        const json = await talk.request('GET', `/recordings/${enc(row.upstream_id)}`, { priority: 'refresh' });
        return await applyRecording(row, json.recording ?? json, { link: true });
      } catch (err) {
        if (!(err instanceof TalkError)) throw err;
        if (err.kind === 'http' && err.status === 404 && settling) {
          return (await db.updateRecording(row.id, { status: 'failed', checked_at: at() })) ?? row;
        }
        return (await db.updateRecording(row.id, { checked_at: at(), url_at: oldLink ? at() : undefined })) ?? row;
      }
    });
  }

  /** A recording of `room` that upstream is running, or null. */
  async function findRecording(room) {
    try {
      const json = await talk.request('GET', '/recordings');
      const list = Array.isArray(json.recordings) ? json.recordings : [];
      return list.find((r) => r && r.room === room && r.status === 'recording' && typeof r.id === 'string') ?? null;
    } catch (err) {
      if (!(err instanceof TalkError)) throw err;
      return null;
    }
  }

  /** Stops a running recording upstream. Throws what upstream threw. */
  async function stopRecording(row) {
    if (row.status !== 'recording' || !row.upstream_id) return row;
    let up;
    try {
      const json = await talk.request('DELETE', `/recordings/${enc(row.upstream_id)}`);
      up = json.recording ?? json;
    } catch (err) {
      if (!(err instanceof TalkError) || err.kind !== 'http' || (err.status !== 404 && err.status !== 409)) throw err;
      // It is not running any more as far as upstream is concerned. Find
      // out what became of it at the next look.
      up = { status: err.status === 404 ? 'failed' : 'processing' };
    }
    const status = recordingStatus(up?.status);
    return applyRecording(row, { ...up, status: status && status !== 'recording' ? status : 'processing' });
  }

  /** Stops whatever is still recording a call that is over. Best effort. */
  async function stopRecordingsOf(callId) {
    const running = (await db.runningRecordings()).filter((r) => r.call_id === callId);
    for (const row of running) {
      try {
        await stopRecording(row);
      } catch (err) {
        if (!(err instanceof TalkError)) throw err;
        log.warn(`a recording could not be stopped when its call ended (${err.kind} ${err.status || ''})`);
      }
    }
  }

  /** Whether `me` may see a recording: they started it, or it is of their room or call. */
  async function maySee(row, me) {
    if (row.started_by === me.identity) return true;
    if (row.room) {
      const rooms = await db.listRooms(me.identity, { name: row.room, limit: 1 });
      return rooms.length > 0 && rooms[0].member;
    }
    if (row.call_id) {
      const call = await db.getCall(row.call_id);
      return Boolean(call) && (call.from_identity === me.identity || call.to_identity === me.identity);
    }
    return false;
  }

  async function ownRecording(id, me) {
    const row = await db.getRecording(id);
    if (!row || !(await maySee(row, me))) throw notFound('There is no such recording.');
    return row;
  }

  // ---- housekeeping -----------------------------------------------------

  /**
   * What no request can be relied on to cause, run every `sweepMs`: ends
   * rings nobody answered, looks at calls nobody is watching, stops
   * recordings whose call is over or that have run too long, and gives up
   * on rows left behind by a request that never finished. Costs upstream
   * nothing while nothing is live. Never throws.
   */
  async function sweep() {
    try {
      await db.failStale(new Date(now() - STALE_PENDING_MS));
      for (const call of await db.liveCalls()) {
        await refreshCall(call, UNWATCHED_MS);
      }
      for (const row of await db.runningRecordings()) {
        let over = now() - new Date(row.created_at).getTime() >= config.recordings.maxMs;
        if (!over && row.call_id) {
          const call = await db.getCall(row.call_id);
          over = !call || !LIVE.has(call.status);
        }
        if (!over && row.room) over = !(await db.getRoom(row.room));
        if (over) {
          try {
            await stopRecording(row);
            log.info('a recording was stopped by the server');
          } catch (err) {
            if (!(err instanceof TalkError)) throw err;
          }
        }
      }
      await syncPhoneCalls(UNWATCHED_MS);
    } catch (err) {
      if (err?.name !== 'DatabaseDown') log.error('housekeeping failed', err);
    }
  }

  // ---- activity ---------------------------------------------------------

  function callPreview(call) {
    const kind = call.video ? 'video call' : 'voice call';
    const Kind = call.video ? 'Video call' : 'Voice call';
    switch (call.status) {
      case 'ringing':
        return call.direction === 'incoming' ? `Incoming ${kind}` : 'Ringing…';
      case 'active':
        return `${Kind} in progress`;
      case 'declined':
        return `Declined ${kind}`;
      case 'missed':
        return call.direction === 'incoming' ? `Missed ${kind}` : 'No answer';
      default:
        return `${Kind}, ${duration(call.seconds)}`;
    }
  }

  function phonePreview(call) {
    switch (call.status) {
      case 'ringing':
        return 'Calling…';
      case 'active':
        return 'Phone call in progress';
      case 'missed':
        return 'No answer';
      case 'failed':
        return 'Call failed';
      default:
        return `Phone call, ${duration(call.seconds)}`;
    }
  }

  const handlers = {
    async health() {
      // The check is cheap but not free; one probe every couple of seconds
      // is enough. It is also what finds a database that has come back.
      if (now() - health.at > 2000) {
        let database = true;
        try {
          await db.ping();
        } catch {
          database = false;
        }
        health = { at: now(), database };
      }
      return {
        status: 200,
        json: {
          ok: talk.enabled && health.database,
          talk: talk.enabled,
          database: health.database,
          phone: talk.enabled && config.phone.enabled && !phoneProblem,
        },
      };
    },

    async getMe({ deviceId }) {
      const row = await db.getPerson(deviceId);
      return { status: 200, json: { identity: row?.identity ?? null, name: row?.name ?? null } };
    },

    async putMe({ req, deviceId }) {
      const body = await readJson(req, limits.jsonBytes);
      const name = text(body.name, 'name', 1, limits.nameChars);
      const existing = await db.getPerson(deviceId);
      if (existing?.identity) {
        const row = await db.upsertPerson(deviceId, { name, at: at() });
        remember(row);
        return { status: 200, json: { identity: row.identity, name: row.name } };
      }
      // The first name gives the identity: `neema`, then `neema-2` …
      const base = slug(name, 36);
      const stem = base.length >= 2 ? base : 'person';
      for (let n = 1; n <= 50; n += 1) {
        const identity = n === 1 ? stem : `${stem}-${n < 40 ? n : randomBytes(3).toString('hex')}`;
        const row = await db.upsertPerson(deviceId, { identity, name, at: at() });
        if (row) {
          remember(row);
          return { status: 200, json: { identity: row.identity, name: row.name } };
        }
      }
      throw new Error('could not find a free identity');
    },

    async listPeople({ deviceId }) {
      const me = await person(deviceId);
      const rows = await db.listPeople(me.identity, PEOPLE_LIMIT);
      const time = now();
      return {
        status: 200,
        json: {
          people: rows.map((row) => ({
            identity: row.identity,
            name: row.name || row.identity,
            seen_at: iso(row.seen_at),
            online: time - new Date(row.seen_at).getTime() <= ONLINE_MS,
          })),
          now: iso(time),
        },
      };
    },

    async account({ deviceId }) {
      await person(deviceId);
      const [spent, learned] = await Promise.all([db.spentTzs(), config.phone.callerId ? null : db.lastCallerId()]);
      return {
        status: 200,
        json: {
          phone: {
            enabled: talk.enabled && config.phone.enabled && !phoneProblem,
            caller_id: config.phone.callerId || learned || null,
          },
          rates: RATES,
          balance: { state: balance, tzs: null },
          spent_tzs: spent,
        },
      };
    },

    // ---- rooms ----------------------------------------------------------

    async listRooms({ deviceId }) {
      const me = await person(deviceId);
      const [rows, recordings] = await Promise.all([db.listRooms(me.identity, { limit: ROOM_LIST_LIMIT }), db.runningRecordings()]);
      const who = await namesOf(rows.flatMap((row) => [row.created_by, row.last_from]));
      return { status: 200, json: { rooms: rows.map((row) => roomOut(row, me, who, recordings)), now: iso(now()) } };
    },

    async createRoom({ req, deviceId }) {
      const body = await readJson(req, limits.jsonBytes);
      const displayName = text(body.display_name, 'display_name', 2, limits.roomNameChars);
      let name;
      if (body.name !== undefined && body.name !== null) {
        if (typeof body.name !== 'string' || !ROOM_NAME.test(body.name)) {
          throw badRequest('"name" must be 2 to 63 lower case letters, digits or dashes.');
        }
        name = body.name;
      } else {
        name = slug(displayName, 63);
        if (!ROOM_NAME.test(name)) throw badRequest('"display_name" needs at least two letters or digits.');
      }
      const me = await person(deviceId);
      if (await db.getRoom(name)) {
        return { status: 200, json: { room: await roomView(name, me) } };
      }
      if (!talk.enabled) throw talkUnavailable();
      // Upstream first: a room that exists here but not there could not be
      // joined. Creating the same name twice upstream is not an error.
      await ask('POST', '/rooms', { body: { name, displayName } });
      const { created } = await db.createRoom({ name, displayName, createdBy: me.identity, at: at() });
      await db.joinRoom(name, me.identity, at());
      return { status: created ? 201 : 200, json: { room: await roomView(name, me) } };
    },

    async getRoom({ deviceId, params }) {
      const me = await person(deviceId);
      return { status: 200, json: { room: await roomView(params.room, me), now: iso(now()) } };
    },

    async closeRoom({ deviceId, params }) {
      const me = await person(deviceId);
      const room = await roomOr404(params.room);
      if (room.created_by !== me.identity) throw forbidden('not_yours', 'Only the person who made a room can close it.');
      if (!talk.enabled) throw talkUnavailable();
      for (const row of (await db.runningRecordings()).filter((r) => r.target_room === room.name)) {
        await stopRecording(row);
      }
      try {
        await ask('DELETE', `/rooms/${enc(room.name)}`);
      } catch (err) {
        // Already gone upstream: all that is left is to forget it here.
        if (!(err instanceof TalkError && err.kind === 'http' && err.status === 404)) throw err;
      }
      await db.deleteRoom(room.name);
      return { status: 200, json: { closed: true } };
    },

    async ticket({ req, deviceId, params }) {
      const body = await readJson(req, limits.jsonBytes);
      const video = flag(body.video, 'video');
      const me = await person(deviceId);
      const room = await roomOr404(params.room);
      if (!talk.enabled) throw talkUnavailable();
      const ttlSeconds = config.talk.ticketTtlSeconds;
      let json;
      try {
        json = await ask('POST', `/rooms/${enc(room.name)}/tokens`, {
          body: { identity: me.identity, name: me.name || me.identity, video, ttlSeconds },
        });
      } catch (err) {
        if (!(err instanceof TalkError && err.kind === 'http' && err.status === 404)) throw err;
        // The room is known here but no longer upstream (it was closed
        // from the console, say). Making it again also gives a ticket.
        json = await ask('POST', '/rooms', { body: { name: room.name, displayName: room.display_name, identity: me.identity, video } });
      }
      if (typeof json.token !== 'string' || typeof json.url !== 'string') throw upstreamFailed();
      const time = at();
      await Promise.all([db.joinRoom(room.name, me.identity, time), db.touchRoom(room.name, time)]);
      return {
        status: 200,
        json: {
          token: json.token,
          url: json.url,
          identity: me.identity,
          room: room.name,
          video,
          expires_at: iso(time.getTime() + ttlSeconds * 1000),
        },
      };
    },

    // ---- chat -----------------------------------------------------------

    async listMessages({ deviceId, params, query }) {
      const me = await person(deviceId);
      const limit = count(query, 50, 100);
      let after = null;
      if (query.get('after') !== null) {
        after = Number.parseInt(query.get('after'), 10);
        if (!Number.isSafeInteger(after) || after < 0 || String(after) !== query.get('after')) {
          throw badRequest('"after" must be a message id.');
        }
      }
      const [room, { rows, lastId, more }] = await Promise.all([db.getRoom(params.room), db.listMessages(params.room, { after, limit })]);
      if (!room) throw notFound('There is no room with that name.');
      const time = at();
      // Looking makes a member. `read=1` also moves the mark to the newest
      // message returned; otherwise the mark stays (0: no further than it is).
      const upTo = query.get('read') === '1' ? (rows.length > 0 ? rows[rows.length - 1].id : (after ?? 0)) : 0;
      const [unread, who] = await Promise.all([
        db.markRead(room.name, me.identity, upTo, time),
        namesOf(rows.map((row) => row.from_identity)),
      ]);
      return {
        status: 200,
        json: { messages: rows.map((row) => messageOut(row, me, who)), last_id: lastId, more, unread, now: iso(time) },
      };
    },

    async postMessage({ req, deviceId, params }) {
      const body = await readJson(req, limits.jsonBytes);
      if (typeof body.text !== 'string') throw badRequest('"text" must be text.');
      const words = body.text.trim();
      const length = [...words].length;
      if (length < 1 || length > limits.textChars) throw badRequest(`"text" must be 1 to ${limits.textChars} characters.`);
      const id = clientId(body.client_id, 'client_id');
      const me = await person(deviceId);
      const room = await roomOr404(params.room);
      const time = at();
      const { row, created } = await db.addMessage({ room: room.name, from: me.identity, text: words, clientId: id, at: time });
      const who = await namesOf([me.identity]);
      if (!created) return { status: 200, json: { message: messageOut(row, me, who) } };
      // Stored. Now the copy for LiveKit clients in the room: once, and
      // whatever happens to it the message stays. The author's read mark
      // moves meanwhile.
      const forward = async () => {
        if (!talk.enabled) return false;
        try {
          await talk.request('POST', `/rooms/${enc(room.name)}/chat`, {
            body: { from: me.identity, text: words },
            priority: 'forward',
            timeout: Math.min(FORWARD_TIMEOUT_MS, config.talk.timeoutMs),
          });
        } catch (err) {
          if (!(err instanceof TalkError)) throw err;
          return false;
        }
        await db.setDelivered(row.id, true);
        return true;
      };
      const [delivered] = await Promise.all([forward(), db.markRead(room.name, me.identity, row.id, time)]);
      return { status: 201, json: { message: messageOut({ ...row, delivered }, me, who) } };
    },

    async markRead({ req, deviceId, params }) {
      const body = await readJson(req, limits.jsonBytes);
      let upTo = null;
      if (body.up_to !== undefined && body.up_to !== null) {
        if (!Number.isSafeInteger(body.up_to) || body.up_to < 0) throw badRequest('"up_to" must be a message id.');
        upTo = body.up_to;
      }
      const me = await person(deviceId);
      const room = await roomOr404(params.room);
      return { status: 200, json: { unread: await db.markRead(room.name, me.identity, upTo, at()) } };
    },

    // ---- in-app calls ---------------------------------------------------

    async ring({ req, deviceId }) {
      const body = await readJson(req, limits.jsonBytes);
      if (typeof body.to !== 'string' || !body.to) throw badRequest('"to" must be a person’s identity.');
      const video = flag(body.video, 'video');
      const me = await person(deviceId);
      if (body.to === me.identity) throw badRequest('You cannot call yourself.');
      const [callee] = await db.peopleByIdentity([body.to]);
      if (!callee) throw notFound('There is nobody with that identity.');
      if (!talk.enabled) throw talkUnavailable();
      const json = await ask('POST', '/app-calls', { body: { from: me.identity, to: callee.identity, video } });
      const up = json.call;
      if (!up || typeof up.id !== 'string' || !up.id || typeof json.token !== 'string' || typeof json.url !== 'string') {
        throw upstreamFailed();
      }
      const row = await db.insertCall({
        id: up.id,
        from: me.identity,
        to: callee.identity,
        video,
        status: 'ringing',
        room: typeof up.room === 'string' && up.room ? up.room : null,
        at: at(),
      });
      return { status: 201, json: { call: await callView(row, me), token: json.token, url: json.url } };
    },

    async incoming({ deviceId }) {
      const me = await person(deviceId);
      const rows = await db.ringingFor(me.identity);
      const fresh = (await Promise.all(rows.map((row) => refreshCall(row)))).filter((row) => row.status === 'ringing');
      const [who, recordings] = await Promise.all([namesOf(fresh.map((row) => row.from_identity)), fresh.length ? db.runningRecordings() : []]);
      return { status: 200, json: { calls: fresh.map((row) => callOut(row, me, who, recordings)), now: iso(now()) } };
    },

    async listCalls({ deviceId }) {
      const me = await person(deviceId);
      const [rows, recordings] = await Promise.all([db.listCalls(me.identity, LIST_LIMIT), db.runningRecordings()]);
      const who = await namesOf(rows.flatMap((row) => [row.from_identity, row.to_identity]));
      return { status: 200, json: { calls: rows.map((row) => callOut(row, me, who, recordings)), now: iso(now()) } };
    },

    async getCall({ deviceId, params }) {
      const me = await person(deviceId);
      const found = await ownCallAndRecordings(params.id, me);
      const row = await refreshCall(found.row);
      // A call that has just ended has had its recording stopped.
      const recordings = LIVE.has(row.status) ? found.recordings : [];
      const who = await namesOf([row.from_identity, row.to_identity]);
      return { status: 200, json: { call: callOut(row, me, who, recordings), now: iso(now()) } };
    },

    async accept({ deviceId, params }) {
      const me = await person(deviceId);
      const { fresh, json } = await answer(params.id, me, 'accept');
      if (typeof json.token !== 'string' || typeof json.url !== 'string') throw upstreamFailed();
      return { status: 200, json: { call: await callView(fresh, me), token: json.token, url: json.url } };
    },

    async decline({ deviceId, params }) {
      const me = await person(deviceId);
      const { fresh } = await answer(params.id, me, 'decline');
      return { status: 200, json: { call: await callView(fresh, me) } };
    },

    async hangUp({ deviceId, params }) {
      const me = await person(deviceId);
      const row = await ownCall(params.id, me);
      const fresh = await shared(`hangup:${row.id}`, () => endCall(row));
      return { status: 200, json: { call: await callView(fresh, me) } };
    },

    // ---- phone calls ----------------------------------------------------

    async dial({ req, deviceId }) {
      const body = await readJson(req, limits.jsonBytes);
      if (typeof body.to !== 'string' || !PHONE_NUMBER.test(body.to)) {
        throw badRequest('"to" must be a phone number like +255712345678.');
      }
      const requestId = clientId(body.request_id ?? req.headers['idempotency-key'], 'request_id');
      const me = await person(deviceId);
      if (!talk.enabled) throw talkUnavailable();
      if (!config.phone.enabled) throw phoneUnavailable();

      // Set the call down first: of two requests only one gets to dial.
      const id = newId('p');
      const reserved = await db.reservePhoneCall({ id, identity: me.identity, requestId, to: body.to, at: at() });
      if (reserved.busy) throw conflict('call_in_progress', 'Your previous phone call has not ended yet.');
      if (!reserved.created) {
        return { status: 200, json: { call: phoneOut(reserved.row), token: null, url: null } };
      }

      const unsure = () => upstreamFailed('The call may or may not have been placed. Look at your calls before dialling again.');
      let json;
      try {
        // Once. Never again for this request, whatever comes back.
        json = await ask('POST', '/calls', {
          body: { to: body.to, identity: me.identity, ...(config.phone.callerId ? { from: config.phone.callerId } : {}) },
        });
      } catch (err) {
        if (!(err instanceof TalkError)) throw err;
        if (err.kind === 'network') {
          // No usable answer: it may be ringing. Keep the row as failed, so
          // that the same request cannot dial a second time.
          await db.updatePhoneCall(id, { status: 'failed', ended_at: at() });
          throw unsure();
        }
        // Refused, or never sent: nothing was dialled.
        await db.deletePhoneCall(id);
        if (err.kind === 'http' && (err.code === 'no_caller_number' || err.status === 503)) phoneProblem = err.code || 'not_connected';
        throw err;
      }
      const up = json.call;
      if (!up || typeof up.id !== 'string' || !up.id) {
        await db.updatePhoneCall(id, { status: 'failed', ended_at: at() });
        throw unsure();
      }
      phoneProblem = '';
      const row = await db.updatePhoneCall(id, {
        upstream_id: up.id,
        caller_id: typeof up.from === 'string' && up.from ? up.from : config.phone.callerId || undefined,
        status: phoneStatus(up.status, null) ?? 'ringing',
      });
      return {
        status: 201,
        json: {
          call: phoneOut(row),
          token: typeof json.token === 'string' ? json.token : null,
          url: typeof json.url === 'string' ? json.url : null,
        },
      };
    },

    async listPhoneCalls({ deviceId }) {
      const me = await person(deviceId);
      let rows = await db.listPhoneCalls(me.identity, LIST_LIMIT);
      if (rows.some((row) => PHONE_LIVE.has(row.status))) {
        await syncPhoneCalls();
        rows = await db.listPhoneCalls(me.identity, LIST_LIMIT);
      }
      return { status: 200, json: { calls: rows.map(phoneOut), now: iso(now()) } };
    },

    async getPhoneCall({ deviceId, params }) {
      const me = await person(deviceId);
      let row = await ownPhoneCall(params.id, me);
      if (PHONE_LIVE.has(row.status)) {
        await syncPhoneCalls();
        row = await ownPhoneCall(params.id, me);
      }
      return { status: 200, json: { call: phoneOut(row), now: iso(now()) } };
    },

    async hangUpPhone({ deviceId, params }) {
      const me = await person(deviceId);
      let row = await ownPhoneCall(params.id, me);
      if (!PHONE_LIVE.has(row.status)) {
        return { status: 200, json: { call: phoneOut(row), hung_up: false, message: 'The call is already over.' } };
      }
      if (config.phone.hangup === 'delete' && row.upstream_id) {
        let up = null;
        try {
          const json = await ask('DELETE', `/calls/${enc(row.upstream_id)}`);
          up = json.call ?? json;
        } catch (err) {
          if (!(err instanceof TalkError && err.kind === 'http' && err.status === 404)) throw err;
        }
        const status = phoneStatus(up?.status, date(up?.endedAt));
        row = await applyPhone(row, { ...up, status: status && !PHONE_LIVE.has(status) ? status : 'ended' });
        return { status: 200, json: { call: phoneOut(row), hung_up: true } };
      }
      // Upstream documents no way to end a phone call. Say so, and note
      // that this person has left it.
      row = (await db.updatePhoneCall(row.id, { left_at: at() })) ?? row;
      return {
        status: 200,
        json: { call: phoneOut(row), hung_up: false, message: 'The call ends when the other side hangs up.' },
      };
    },

    // ---- recordings -----------------------------------------------------

    async startRecording({ req, deviceId }) {
      const body = await readJson(req, limits.jsonBytes);
      const hasRoom = typeof body.room === 'string' && body.room !== '';
      const hasCall = typeof body.call === 'string' && body.call !== '';
      if (hasRoom === hasCall) throw badRequest('Give either "room" or "call".');
      const me = await person(deviceId);

      let target;
      if (hasRoom) {
        const room = await roomOr404(body.room);
        await db.joinRoom(room.name, me.identity, at());
        target = { targetRoom: room.name, room: room.name, callId: null };
      } else {
        const call = await ownCall(body.call, me);
        if (call.status !== 'active') throw conflict('conflict', 'Only a call that has been answered can be recorded.');
        if (!call.room) throw conflict('recording_unsupported', 'This call cannot be recorded.');
        target = { targetRoom: call.room, room: null, callId: call.id };
      }
      if (!talk.enabled) throw talkUnavailable();

      // Set it down first: of two requests only one starts (and pays for)
      // a recording of the same room.
      const id = newId('r');
      const reserved = await db.reserveRecording({ id, ...target, startedBy: me.identity, at: at() });
      const who = await namesOf([reserved.row.started_by]);
      if (!reserved.created) return { status: 200, json: { recording: recordingOut(reserved.row, who) } };

      let up;
      try {
        const json = await ask('POST', '/recordings', { body: { room: target.targetRoom } });
        up = json.recording;
      } catch (err) {
        if (!(err instanceof TalkError)) throw err;
        if (err.kind !== 'network') {
          // Refused, or never sent: nothing is recording.
          await db.deleteRecording(id);
          throw err;
        }
        up = null;
      }
      if (!up || typeof up.id !== 'string' || !up.id) {
        // No usable answer: it may be recording, and costing. Look for it
        // once, so that it can at least be stopped.
        up = await findRecording(target.targetRoom);
        if (!up) {
          await db.updateRecording(id, { status: 'failed', checked_at: at() });
          throw upstreamFailed('The recording may or may not have started. Look at the recordings before trying again.');
        }
      }
      const row = await db.updateRecording(id, { upstream_id: up.id, status: 'recording', checked_at: at() });
      return { status: 201, json: { recording: recordingOut(row, who) } };
    },

    async listRecordings({ deviceId, query }) {
      const me = await person(deviceId);
      let rows;
      if (query.get('room')) {
        const rooms = await db.listRooms(me.identity, { name: query.get('room'), limit: 1 });
        if (rooms.length === 0 || !rooms[0].member) throw notFound('There is no room with that name.');
        rows = await db.listRecordings({ room: rooms[0].name }, LIST_LIMIT);
      } else if (query.get('call')) {
        const call = await ownCall(query.get('call'), me);
        rows = await db.listRecordings({ callId: call.id }, LIST_LIMIT);
      } else {
        rows = await db.listRecordings({ startedBy: me.identity }, LIST_LIMIT);
      }
      const who = await namesOf(rows.map((row) => row.started_by));
      return { status: 200, json: { recordings: rows.map((row) => recordingOut(row, who)), now: iso(now()) } };
    },

    async getRecording({ deviceId, params }) {
      const me = await person(deviceId);
      const row = await refreshRecording(await ownRecording(params.id, me), { link: true });
      const who = await namesOf([row.started_by]);
      return { status: 200, json: { recording: recordingOut(row, who), now: iso(now()) } };
    },

    async stopRecording({ deviceId, params }) {
      const me = await person(deviceId);
      const row = await stopRecording(await ownRecording(params.id, me));
      const who = await namesOf([row.started_by]);
      return { status: 200, json: { recording: recordingOut(row, who) } };
    },

    // ---- activity -------------------------------------------------------

    async activity({ deviceId, query }) {
      const me = await person(deviceId);
      const limit = count(query, LIST_LIMIT, 100);
      const [rooms, calls, phones] = await Promise.all([
        db.listRooms(me.identity, { memberOnly: true, limit }),
        db.listCalls(me.identity, limit),
        db.listPhoneCalls(me.identity, limit),
      ]);
      const who = await namesOf([
        ...rooms.flatMap((row) => [row.created_by, row.last_from]),
        ...calls.flatMap((row) => [row.from_identity, row.to_identity]),
      ]);
      const items = [];
      for (const row of rooms) {
        const room = roomOut(row, me, who, []);
        const last = room.last_message;
        items.push({
          sort: new Date(row.active_at).getTime(),
          kind: 'room',
          id: room.name,
          title: room.display_name,
          preview: last ? `${last.mine ? 'You' : last.from_name}: ${last.text.replace(/\s+/g, ' ').slice(0, 120)}` : 'No messages yet',
          at: room.active_at,
          unread: room.unread,
          live: room.live,
          status: 'open',
          direction: null,
          video: null,
          seconds: null,
          peer: null,
        });
      }
      for (const row of calls) {
        const call = callOut(row, me, who, []);
        items.push({
          sort: new Date(row.created_at).getTime(),
          kind: 'call',
          id: call.id,
          title: call.peer.name,
          preview: callPreview(call),
          at: call.created_at,
          unread: 0,
          live: LIVE.has(call.status),
          status: call.status,
          direction: call.direction,
          video: call.video,
          seconds: call.seconds,
          peer: call.peer,
        });
      }
      for (const row of phones) {
        const call = phoneOut(row);
        items.push({
          sort: new Date(row.created_at).getTime(),
          kind: 'phone',
          id: call.id,
          title: call.to,
          preview: phonePreview(call),
          at: call.created_at,
          unread: 0,
          live: call.status === 'ringing' || call.status === 'active',
          status: call.status,
          direction: 'outgoing',
          video: false,
          seconds: call.seconds,
          peer: null,
        });
      }
      items.sort((a, b) => b.sort - a.sort);
      return {
        status: 200,
        json: { items: items.slice(0, limit).map(({ sort, ...item }) => item), now: iso(now()) },
      };
    },
  };

  const ROOM = '[a-z0-9][a-z0-9-]{0,61}[a-z0-9]';
  const ID = '[A-Za-z0-9_.:-]{1,80}';
  const path = (pattern) => new RegExp(`^/v1/${pattern.replaceAll('{room}', `(${ROOM})`).replaceAll('{id}', `(${ID})`)}$`);
  const h = handlers;
  const table = [
    ['GET', 'health', h.health, []],
    ['GET', 'me', h.getMe, []],
    ['PUT', 'me', h.putMe, []],
    ['GET', 'people', h.listPeople, []],
    ['GET', 'account', h.account, []],
    ['GET', 'activity', h.activity, []],

    ['GET', 'rooms', h.listRooms, []],
    ['POST', 'rooms', h.createRoom, []],
    ['GET', 'rooms/{room}', h.getRoom, ['room']],
    ['DELETE', 'rooms/{room}', h.closeRoom, ['room']],
    ['POST', 'rooms/{room}/close', h.closeRoom, ['room']],
    ['POST', 'rooms/{room}/ticket', h.ticket, ['room']],
    ['GET', 'rooms/{room}/messages', h.listMessages, ['room']],
    ['POST', 'rooms/{room}/messages', h.postMessage, ['room']],
    ['POST', 'rooms/{room}/read', h.markRead, ['room']],

    ['GET', 'calls', h.listCalls, []],
    ['POST', 'calls', h.ring, []],
    ['GET', 'calls/incoming', h.incoming, []],
    ['GET', 'calls/{id}', h.getCall, ['id']],
    ['DELETE', 'calls/{id}', h.hangUp, ['id']],
    ['POST', 'calls/{id}/accept', h.accept, ['id']],
    ['POST', 'calls/{id}/decline', h.decline, ['id']],
    ['POST', 'calls/{id}/hangup', h.hangUp, ['id']],

    ['GET', 'phone-calls', h.listPhoneCalls, []],
    ['POST', 'phone-calls', h.dial, []],
    ['GET', 'phone-calls/{id}', h.getPhoneCall, ['id']],
    ['DELETE', 'phone-calls/{id}', h.hangUpPhone, ['id']],
    ['POST', 'phone-calls/{id}/hangup', h.hangUpPhone, ['id']],

    ['GET', 'recordings', h.listRecordings, []],
    ['POST', 'recordings', h.startRecording, []],
    ['GET', 'recordings/{id}', h.getRecording, ['id']],
    ['DELETE', 'recordings/{id}', h.stopRecording, ['id']],
    ['POST', 'recordings/{id}/stop', h.stopRecording, ['id']],
  ].map(([verb, pattern, handler, names]) => [verb, path(pattern), handler, names]);

  /** The handler and path parameters for a request, or a 404. */
  function match(method, pathname) {
    for (const [verb, pattern, handler, names] of table) {
      if (verb !== method) continue;
      const found = pattern.exec(pathname);
      if (!found) continue;
      const params = Object.fromEntries(names.map((name, i) => [name, found[i + 1]]));
      return { handler, params, open: handler === handlers.health };
    }
    throw new HttpError(404, 'not_found', 'Not found.');
  }

  return { match, sweep };
}
