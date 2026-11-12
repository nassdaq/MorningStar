// The API of `API.md`, one function per endpoint.
//
// A handler receives `{req, deviceId, params}` and returns `{status, json}`
// or `{status, bytes, type}`; it reports a failure by throwing an
// `HttpError`. Authentication, the device id, the rate limit, logging and
// the error shape are applied around it in `app.js`.

import { randomBytes } from 'node:crypto';

import { HttpError, UpstreamError, badRequest, callEnded, notFound, upstreamFailed, voiceUnavailable } from './errors.js';
import { readBody } from './http.js';
import { SKILLS, chars, clip, tidy } from './repair.js';
import {
  ACCENTS,
  DIFFICULTIES,
  MAX_LEARNER_TURNS,
  MINUTES,
  PLANS,
  SCENARIOS,
  SCENARIO_IDS,
  UNSCORED,
} from './scenarios.js';
import { createKeyedLock } from './stores.js';
import { AUDIO_TYPES } from './voice.js';

const NAME_LIMIT = 24;
const HISTORY_LIMIT = 50;
const DAY_MS = 24 * 60 * 60 * 1000;

/** `2026-11-12T09:14:03Z`: seconds, no fraction, as the contract shows it. */
function isoSeconds(date) {
  return new Date(date).toISOString().replace(/\.\d{3}Z$/, 'Z');
}

/** The calendar day of `time` in `timeZone`, as `YYYY-MM-DD`. */
export function dayIn(timeZone, time) {
  return new Intl.DateTimeFormat('en-CA', { timeZone, year: 'numeric', month: '2-digit', day: '2-digit' }).format(new Date(time));
}

function dayBefore(day) {
  return new Date(Date.parse(`${day}T00:00:00Z`) - DAY_MS).toISOString().slice(0, 10);
}

/**
 * Consecutive days with a scored call, counting back from today. A streak
 * is still alive on a day with no call yet, as long as yesterday had one.
 */
export function streakDays(days, today) {
  const practised = new Set(days);
  let day = practised.has(today) ? today : dayBefore(today);
  let streak = 0;
  while (practised.has(day)) {
    streak += 1;
    day = dayBefore(day);
  }
  return streak;
}

function oneOf(value, allowed, field) {
  if (!allowed.includes(value)) {
    throw badRequest(`"${field}" must be one of: ${allowed.join(', ')}.`);
  }
  return value;
}

async function readJson(req, limit) {
  const bytes = await readBody(req, limit, 'The request body is too large (64 KB at most).');
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

function mediaType(req) {
  return String(req.headers['content-type'] ?? '').split(';')[0].trim().toLowerCase();
}

export function createRoutes({ config, db, model, voice, audio, now }) {
  const withCallLock = createKeyedLock();
  let health = { at: -Infinity, database: false };

  /** The profile as the API shows it; `loading` resolves to the stored row. */
  async function profileJson(loading, deviceId) {
    // The stored row and the figures computed from the calls, in one round trip's time.
    const [row, stats] = await Promise.all([loading, db.stats(deviceId, config.streakTimeZone)]);
    return {
      name: row.name,
      accent: row.accent,
      difficulty: row.difficulty,
      minutes: row.minutes,
      plan: row.plan,
      calls: stats.calls,
      average_score: stats.calls > 0 ? stats.average_score : null,
      streak_days: streakDays(stats.days, dayIn(config.streakTimeZone, now())),
    };
  }

  /** The call, if it exists and belongs to this device; otherwise 404. */
  async function ownCall(callId, deviceId) {
    const call = await db.getCall(callId);
    if (!call || call.device_id !== deviceId) throw notFound('No such call.');
    return call;
  }

  /** Voices a caller line and returns its URL, or null. Never throws. */
  async function speakLine(call, text) {
    if (!voice.enabled) return null;
    const clipBytes = await voice.speak(text, call.accent, SCENARIOS[call.scenario].gender);
    if (!clipBytes) return null;
    const id = audio.put(call.device_id, clipBytes);
    return id ? `/v1/audio/${id}.mp3` : null;
  }

  function reportJson(call, turns, report) {
    const scenario = SCENARIOS[call.scenario];
    return {
      call_id: call.id,
      number: call.number,
      scenario: call.scenario,
      title: scenario.title,
      caller_name: scenario.caller,
      started_at: isoSeconds(call.started_at),
      duration_seconds: call.duration_seconds ?? 0,
      score: report ? report.score : null,
      skills: report ? Object.fromEntries(SKILLS.map((skill) => [skill, report[skill]])) : null,
      did_well: report ? report.did_well : UNSCORED.did_well,
      try_next: report ? report.try_next : UNSCORED.try_next,
      transcript: turns.map((turn) => {
        const line = { speaker: turn.speaker, text: turn.text };
        if (turn.speaker === 'learner' && turn.note) line.note = turn.note;
        return line;
      }),
    };
  }

  async function storedReport(call) {
    const [turns, report] = await Promise.all([db.getTurns(call.id), db.getReport(call.id)]);
    return reportJson(call, turns, report);
  }

  /** The learner's words for a turn: the JSON text, or the transcript of the audio. */
  async function heardFrom(req) {
    const type = mediaType(req);
    if (type === 'application/json') {
      const body = await readJson(req, config.limits.jsonBytes);
      if (typeof body.text !== 'string') throw badRequest('"text" must be a string.');
      const text = tidy(body.text);
      if (!text) throw badRequest('"text" must not be empty.');
      if (chars(text) > config.limits.learnerChars) {
        throw badRequest(`"text" must be at most ${config.limits.learnerChars} characters.`);
      }
      return text;
    }
    if (AUDIO_TYPES.includes(type)) {
      if (!voice.enabled) throw voiceUnavailable();
      const bytes = await readBody(req, config.limits.audioBytes, 'The recording is too large (5 MB at most).');
      if (bytes.length === 0) throw badRequest('The recording is empty.');
      let transcript;
      try {
        transcript = await voice.transcribe(bytes, type);
      } catch (err) {
        if (err instanceof UpstreamError) throw upstreamFailed('The voice service could not transcribe the recording.');
        throw err;
      }
      const text = clip(transcript, config.limits.learnerChars);
      if (!text) throw badRequest('No speech was heard in the recording.');
      return text;
    }
    throw badRequest('Send JSON {"text": ...} or a recording (audio/mp4, audio/mpeg, audio/wav or audio/webm).');
  }

  const handlers = {
    async health() {
      // The check is cheap but not free; one probe every few seconds is enough.
      if (now() - health.at > 5000) {
        let database = true;
        try {
          await db.ping();
        } catch {
          database = false;
        }
        health = { at: now(), database };
      }
      const body = { ok: model.enabled && health.database, model: model.enabled, voice: voice.enabled, database: health.database };
      return { status: 200, json: body };
    },

    async getProfile({ deviceId }) {
      return { status: 200, json: await profileJson(db.getOrCreateProfile(deviceId), deviceId) };
    },

    async putProfile({ req, deviceId }) {
      const body = await readJson(req, config.limits.jsonBytes);
      const patch = {};
      if (body.name !== undefined) {
        if (typeof body.name !== 'string') throw badRequest('"name" must be a string.');
        const name = tidy(body.name);
        if (chars(name) < 1 || chars(name) > NAME_LIMIT) {
          throw badRequest(`"name" must be 1 to ${NAME_LIMIT} characters.`);
        }
        patch.name = name;
      }
      if (body.accent !== undefined) patch.accent = oneOf(body.accent, ACCENTS, 'accent');
      if (body.difficulty !== undefined) patch.difficulty = oneOf(body.difficulty, DIFFICULTIES, 'difficulty');
      if (body.minutes !== undefined) patch.minutes = oneOf(body.minutes, MINUTES, 'minutes');
      if (body.plan !== undefined) patch.plan = oneOf(body.plan, PLANS, 'plan');
      return { status: 200, json: await profileJson(db.updateProfile(deviceId, patch), deviceId) };
    },

    async startCall({ req, deviceId }) {
      const body = await readJson(req, config.limits.jsonBytes);
      const scenarioId = oneOf(body.scenario, SCENARIO_IDS, 'scenario');
      const profile = await db.getOrCreateProfile(deviceId);
      const accent = body.accent === undefined ? profile.accent : oneOf(body.accent, ACCENTS, 'accent');
      const difficulty = body.difficulty === undefined ? profile.difficulty : oneOf(body.difficulty, DIFFICULTIES, 'difficulty');
      const minutes = body.minutes === undefined ? profile.minutes : oneOf(body.minutes, MINUTES, 'minutes');
      const scenario = SCENARIOS[scenarioId];

      const call = await db.createCall({
        id: `c_${randomBytes(16).toString('hex')}`,
        deviceId,
        scenario: scenarioId,
        accent,
        difficulty,
        minutes,
        voice: voice.enabled,
        opening: scenario.opening,
        tips: scenario.tips,
        startedAt: new Date(now()),
      });
      const audioUrl = await speakLine(call, scenario.opening);
      return {
        status: 200,
        json: {
          call_id: call.id,
          number: call.number,
          scenario: scenarioId,
          title: scenario.title,
          caller_name: scenario.caller,
          goal: scenario.goal,
          accent,
          difficulty,
          minutes,
          voice: voice.enabled,
          line: { text: scenario.opening, audio_url: audioUrl },
          tips: scenario.tips,
        },
      };
    },

    async turn({ req, deviceId, params }) {
      // A recording is refused before it is read: a 5 MB upload for a call
      // that is over, or is someone else's, should not be transcribed first.
      // A text turn is small, and is checked once, under the lock.
      if (mediaType(req) !== 'application/json') {
        const before = await ownCall(params.callId, deviceId);
        if (before.status !== 'active' || before.caller_done) throw callEnded();
      }
      const heard = await heardFrom(req);

      return withCallLock(params.callId, async () => {
        const [call, turns] = await Promise.all([ownCall(params.callId, deviceId), db.getTurns(params.callId)]);
        if (call.status !== 'active' || call.caller_done) throw callEnded();
        const learnerTurns = turns.filter((t) => t.speaker === 'learner').length + 1;
        const secondsLeft = call.minutes * 60 - (now() - new Date(call.started_at).getTime()) / 1000;
        const maxTurns = MAX_LEARNER_TURNS[call.minutes] ?? 14;
        const final = secondsLeft <= 0 || learnerTurns >= maxTurns;
        const budget = {
          final,
          closing: !final && (secondsLeft <= 30 || learnerTurns >= maxTurns - 2),
          minutesLeft: Math.max(1, Math.ceil(secondsLeft / 60)),
          learnerTurns,
        };

        let reply;
        try {
          reply = await model.reply(call, [...turns, { speaker: 'learner', text: heard }], budget);
        } catch (err) {
          // Nothing was stored: the app can send the same turn again.
          if (err instanceof UpstreamError) throw upstreamFailed('The caller could not answer just now. Please try again.');
          throw err;
        }
        const done = reply.done || final;
        // Store the exchange and voice the line at the same time.
        const [, audioUrl] = await Promise.all([
          db.addExchange(call.id, { seq: turns.length, learnerText: heard, callerText: reply.line, tips: reply.tips, done }),
          speakLine(call, reply.line),
        ]);
        return {
          status: 200,
          json: {
            heard,
            line: { text: reply.line, audio_url: audioUrl },
            tips: reply.tips,
            turn: learnerTurns + 1,
            done,
          },
        };
      });
    },

    async endCall({ deviceId, params }) {
      return withCallLock(params.callId, async () => {
        const [call, turns] = await Promise.all([ownCall(params.callId, deviceId), db.getTurns(params.callId)]);
        if (call.status === 'ended') return { status: 200, json: await storedReport(call) };

        let report = null;
        if (turns.some((t) => t.speaker === 'learner')) {
          try {
            report = await model.score(call, turns);
          } catch (err) {
            // The call stays open, so `end` can simply be sent again.
            if (err instanceof UpstreamError) throw upstreamFailed('The call could not be scored just now. Please try again.');
            throw err;
          }
        }
        const endedAt = new Date(now());
        const durationSeconds = Math.max(0, Math.round((endedAt.getTime() - new Date(call.started_at).getTime()) / 1000));
        const ended = await db.endCall(call.id, { endedAt, durationSeconds, report });
        // Another server process ended it first: return what that one stored.
        if (!ended) return { status: 200, json: await storedReport(await ownCall(call.id, deviceId)) };
        const noted = turns.map((turn) => ({ ...turn, note: report?.notes[turn.seq] ?? null }));
        const row = report && { score: report.score, ...report.skills, did_well: report.did_well, try_next: report.try_next };
        return { status: 200, json: reportJson({ ...call, duration_seconds: durationSeconds }, noted, row) };
      });
    },

    async getCall({ deviceId, params }) {
      const call = await ownCall(params.callId, deviceId);
      if (call.status !== 'ended') throw notFound('No such call.');
      return { status: 200, json: await storedReport(call) };
    },

    async listCalls({ deviceId }) {
      const rows = await db.listCalls(deviceId, HISTORY_LIMIT);
      return {
        status: 200,
        json: {
          calls: rows.map((row) => ({
            call_id: row.id,
            number: row.number,
            scenario: row.scenario,
            title: SCENARIOS[row.scenario]?.title ?? row.scenario,
            started_at: isoSeconds(row.started_at),
            duration_seconds: row.duration_seconds ?? 0,
            minutes: row.minutes,
            score: row.score,
          })),
        },
      };
    },

    async getAudio({ deviceId, params }) {
      const bytes = audio.get(deviceId, params.audioId);
      if (!bytes) throw notFound('This audio is no longer available.');
      return { status: 200, bytes, type: 'audio/mpeg' };
    },
  };

  const CALL_ID = 'c_[0-9a-f]{32}';
  const table = [
    ['GET', /^\/v1\/health$/, handlers.health, []],
    ['GET', /^\/v1\/profile$/, handlers.getProfile, []],
    ['PUT', /^\/v1\/profile$/, handlers.putProfile, []],
    ['POST', /^\/v1\/calls$/, handlers.startCall, []],
    ['GET', /^\/v1\/calls$/, handlers.listCalls, []],
    ['POST', new RegExp(`^/v1/calls/(${CALL_ID})/turns$`), handlers.turn, ['callId']],
    ['POST', new RegExp(`^/v1/calls/(${CALL_ID})/end$`), handlers.endCall, ['callId']],
    ['GET', new RegExp(`^/v1/calls/(${CALL_ID})$`), handlers.getCall, ['callId']],
    ['GET', /^\/v1\/audio\/(a_[0-9a-f]{24})\.mp3$/, handlers.getAudio, ['audioId']],
  ];

  /** The handler and path parameters for a request, or a 404. */
  return function match(method, path) {
    for (const [verb, pattern, handler, names] of table) {
      if (verb !== method) continue;
      const found = pattern.exec(path);
      if (!found) continue;
      const params = Object.fromEntries(names.map((name, i) => [name, found[i + 1]]));
      return { handler, params, open: handler === handlers.health };
    }
    throw new HttpError(404, 'not_found', 'Not found.');
  };
}
