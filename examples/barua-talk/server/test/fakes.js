// Test doubles: an in-process Barua Talk API written from
// `docs/barua-talk-api.md`, an in-memory database with the interface of
// `src/db.js`, and a harness that runs the real app between the two.

import { randomBytes } from 'node:crypto';
import http from 'node:http';

import { createApp } from '../src/app.js';
import { loadConfig } from '../src/config.js';
import { pairOf } from '../src/db.js';
import { DatabaseDown } from '../src/errors.js';
import { createLogger } from '../src/log.js';
import { createTalk } from '../src/talk.js';

export const TEST_KEY = 'barua_test_0123456789abcdef';

function listen(server) {
  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => resolve(server.address().port));
  });
}

function closeServer(server) {
  return new Promise((resolve) => {
    server.close(resolve);
    server.closeAllConnections();
  });
}

/**
 * The Barua Talk API as its document describes it, on a local port.
 *
 * Knobs (plain properties, set them in a test): `balanceTzs`,
 * `voiceNumbers`, `phoneConnected`, `recordingConnected`, `limit`
 * (requests a minute), `callsHaveRoom` (whether an in-app call says which
 * room it is), `phoneHangup` (whether `DELETE /calls/{id}` exists).
 * `fail(method, path, how)` makes the next matching request fail: `how` is
 * `{status, body}` to answer with, or `'drop'` to cut the connection after
 * doing the work (the caller cannot tell whether it happened), or
 * `'drop-before'` to cut it before.
 */
export async function createFakeTalk() {
  const fake = {
    balanceTzs: 10000,
    voiceNumbers: ['+255800110011'],
    phoneConnected: true,
    recordingConnected: true,
    limit: 60,
    callsHaveRoom: true,
    phoneHangup: false,
    rooms: new Map(),
    chat: new Map(),
    appCalls: new Map(),
    phoneCalls: [],
    recordings: new Map(),
    sms: [],
    requests: [],
    failures: [],
    minute: [],
  };
  const id = () => randomBytes(8).toString('hex');
  const stamp = () => new Date().toISOString();
  const problem = (status, code, message) => ({ status, body: { error: { code, message } } });

  fake.fail = (method, path, how) => fake.failures.push({ method, path, how });
  /** How many requests matched, e.g. `count('GET', /^\/app-calls\//)`. */
  fake.count = (method, pattern) =>
    fake.requests.filter((r) => r.method === method && (pattern instanceof RegExp ? pattern.test(r.path) : r.path === pattern)).length;
  fake.reset = () => {
    fake.requests.length = 0;
  };

  // What the platform does by itself.
  fake.abandon = (callId) => {
    const call = fake.appCalls.get(callId);
    if (call.status === 'ringing') Object.assign(call, { status: 'missed', endedAt: stamp() });
    else if (call.status === 'active') Object.assign(call, { status: 'ended', endedAt: stamp(), seconds: 42 });
  };
  fake.answerPhone = (callId) => {
    Object.assign(fake.phoneCalls.find((c) => c.id === callId), { status: 'active', startedAt: stamp() });
  };
  fake.endPhone = (callId, seconds) => {
    Object.assign(fake.phoneCalls.find((c) => c.id === callId), {
      status: 'ended',
      endedAt: stamp(),
      seconds,
      priceTzs: Math.ceil(seconds / 60) * 300,
    });
  };
  fake.finishRecording = (recordingId, seconds) => {
    Object.assign(fake.recordings.get(recordingId), {
      status: 'ready',
      seconds,
      sizeBytes: seconds * 20000,
      priceTzs: Math.ceil(seconds / 60) * 30,
      readyAt: stamp(),
    });
  };

  const roomExists = (name) => fake.rooms.has(name) || [...fake.appCalls.values()].some((c) => c.roomName === name);
  const ticket = (identity) => ({ token: `ticket.${identity}.${id()}`, url: 'wss://fake.livekit.invalid' });
  const callOut = (call) => {
    const { roomName, ...rest } = call;
    return fake.callsHaveRoom ? { ...rest, room: roomName } : rest;
  };

  function route(method, path, query, body) {
    let m;
    if (path === '/rooms' && method === 'POST') {
      if (typeof body.name !== 'string' || !/^[a-z0-9-]{2,63}$/.test(body.name)) {
        return problem(422, 'invalid_request', 'lower case letters, digits and dashes');
      }
      if (!fake.rooms.has(body.name)) {
        fake.rooms.set(body.name, { name: body.name, displayName: body.displayName ?? body.name, createdAt: stamp() });
        fake.chat.set(body.name, []);
      }
      return { status: 201, body: { room: fake.rooms.get(body.name), ...(body.identity ? ticket(body.identity) : {}) } };
    }
    if (path === '/rooms' && method === 'GET') return { status: 200, body: { rooms: [...fake.rooms.values()] } };
    if ((m = /^\/rooms\/([^/]+)$/.exec(path)) && method === 'DELETE') {
      if (!fake.rooms.delete(m[1])) return problem(404, 'room_not_found', `No room called ${m[1]}.`);
      fake.chat.delete(m[1]);
      return { status: 200, body: { room: { name: m[1], closed: true } } };
    }
    if ((m = /^\/rooms\/([^/]+)\/tokens$/.exec(path)) && method === 'POST') {
      if (!fake.rooms.has(m[1])) return problem(404, 'room_not_found', `No room called ${m[1]}. Create it first.`);
      if (typeof body.identity !== 'string' || !body.identity) return problem(422, 'invalid_request', 'identity is required');
      return { status: 200, body: ticket(body.identity) };
    }
    if ((m = /^\/rooms\/([^/]+)\/chat$/.exec(path))) {
      if (!fake.rooms.has(m[1])) return problem(404, 'room_not_found', `No room called ${m[1]}. Create it first.`);
      if (method === 'GET') return { status: 200, body: { messages: fake.chat.get(m[1]) } };
      if (method === 'POST') {
        const message = { id: id(), from: body.from, text: body.text, createdAt: stamp() };
        fake.chat.get(m[1]).push(message);
        return { status: 201, body: message };
      }
    }

    if (path === '/app-calls' && method === 'POST') {
      if (typeof body.from !== 'string' || typeof body.to !== 'string') return problem(422, 'invalid_request', 'from and to are required');
      const call = {
        id: id(),
        from: body.from,
        to: body.to,
        video: Boolean(body.video),
        status: 'ringing',
        createdAt: stamp(),
        answeredAt: null,
        endedAt: null,
        seconds: 0,
        roomName: `call-${id()}`,
      };
      fake.appCalls.set(call.id, call);
      return { status: 201, body: { call: callOut(call), ...ticket(body.from) } };
    }
    if (path === '/app-calls' && method === 'GET') {
      const calls = [...fake.appCalls.values()].filter(
        (c) => (!query.get('identity') || c.to === query.get('identity')) && (!query.get('status') || c.status === query.get('status')),
      );
      return { status: 200, body: { calls: calls.map(callOut) } };
    }
    if ((m = /^\/app-calls\/([^/]+)$/.exec(path))) {
      const call = fake.appCalls.get(m[1]);
      if (!call) return problem(404, 'call_not_found', 'No such call.');
      if (method === 'GET') return { status: 200, body: { call: callOut(call) } };
      if (method === 'POST') {
        if (body.identity !== call.to) return problem(403, 'not_your_call', 'This call rings for someone else.');
        if (call.status !== 'ringing') return problem(409, 'not_ringing', 'This call is no longer ringing.');
        if (body.action === 'accept') {
          Object.assign(call, { status: 'active', answeredAt: stamp() });
          return { status: 200, body: { call: callOut(call), ...ticket(call.to) } };
        }
        if (body.action === 'decline') {
          Object.assign(call, { status: 'declined', endedAt: stamp() });
          return { status: 200, body: { call: callOut(call) } };
        }
        return problem(422, 'invalid_request', 'action must be accept or decline');
      }
      if (method === 'DELETE') {
        if (call.status === 'ringing') Object.assign(call, { status: 'missed', endedAt: stamp() });
        else if (call.status === 'active') Object.assign(call, { status: 'ended', endedAt: stamp(), seconds: 7 });
        return { status: 200, body: { call: callOut(call) } };
      }
    }

    if (path === '/recordings' && method === 'POST') {
      if (!fake.recordingConnected) return problem(503, 'not_connected', 'Recording is not provisioned yet.');
      if (fake.balanceTzs <= 0) return problem(402, 'balance_empty', 'Top up to record.');
      if (!roomExists(body.room)) return problem(404, 'room_not_found', `No room called ${body.room}.`);
      const recording = { id: id(), room: body.room, status: 'recording', seconds: 0, sizeBytes: null, priceTzs: null, createdAt: stamp(), readyAt: null };
      fake.recordings.set(recording.id, recording);
      return { status: 201, body: { recording: { id: recording.id, room: recording.room, status: 'recording' } } };
    }
    if (path === '/recordings' && method === 'GET') return { status: 200, body: { recordings: [...fake.recordings.values()] } };
    if ((m = /^\/recordings\/([^/]+)$/.exec(path))) {
      const recording = fake.recordings.get(m[1]);
      if (!recording) return problem(404, 'recording_not_found', 'No such recording.');
      if (method === 'GET') {
        const url = recording.status === 'ready' ? { url: `https://fake.invalid/${recording.id}.mp4?sig=${id()}` } : {};
        return { status: 200, body: { ...recording, ...url } };
      }
      if (method === 'DELETE') {
        if (recording.status !== 'recording') return problem(409, 'not_recording', 'This recording is not running.');
        recording.status = 'processing';
        return { status: 200, body: { recording: { id: recording.id, status: 'processing' } } };
      }
    }

    if (path === '/calls' && method === 'POST') {
      if (!fake.phoneConnected) return problem(503, 'not_connected', 'Phone calls are not provisioned yet.');
      if (typeof body.to !== 'string' || !/^\+[0-9]{8,15}$/.test(body.to)) return problem(422, 'invalid_request', 'to must be E.164');
      if (fake.voiceNumbers.length === 0) return problem(409, 'no_caller_number', 'Add a voice number first.');
      if (fake.balanceTzs <= 0) return problem(402, 'balance_empty', 'Top up to call.');
      const call = {
        id: id(),
        direction: 'outbound',
        status: 'ringing',
        from: body.from ?? fake.voiceNumbers[0],
        to: body.to,
        startedAt: null,
        endedAt: null,
        seconds: 0,
        priceTzs: null,
      };
      fake.phoneCalls.push(call);
      return { status: 201, body: { call: { id: call.id, to: call.to, from: call.from, status: 'ringing' }, ...ticket(body.identity ?? 'caller') } };
    }
    if (path === '/calls' && method === 'GET') return { status: 200, body: { calls: fake.phoneCalls } };
    if ((m = /^\/calls\/([^/]+)$/.exec(path)) && method === 'DELETE' && fake.phoneHangup) {
      const call = fake.phoneCalls.find((c) => c.id === m[1]);
      if (!call) return problem(404, 'call_not_found', 'No such call.');
      Object.assign(call, { status: 'ended', endedAt: stamp(), seconds: 30, priceTzs: 300 });
      return { status: 200, body: { call } };
    }

    if (path === '/messages' && method === 'POST') {
      fake.sms.push(body);
      return { status: 201, body: { id: id(), status: 'queued', priceTzs: 40 } };
    }
    return problem(404, 'not_found', 'No such endpoint.');
  }

  const server = http.createServer((req, res) => {
    const chunks = [];
    req.on('data', (chunk) => chunks.push(chunk));
    req.on('end', () => {
      const url = new URL(req.url, 'http://fake');
      const path = url.pathname.replace(/^\/api\/v1\/talk/, '');
      let body = {};
      try {
        body = chunks.length ? JSON.parse(Buffer.concat(chunks).toString('utf8')) : {};
      } catch {
        body = {};
      }
      fake.requests.push({ method: req.method, path, body });
      const reply = ({ status, body: value }, headers = {}) => {
        const bytes = Buffer.from(typeof value === 'string' ? value : JSON.stringify(value));
        res.writeHead(status, { 'Content-Type': 'application/json', 'Content-Length': bytes.length, ...headers });
        res.end(bytes);
      };

      if (req.headers.authorization !== `Bearer ${TEST_KEY}`) return reply(problem(401, 'invalid_key', 'Missing or unknown API key.'));
      const time = Date.now();
      fake.minute = fake.minute.filter((t) => t > time - 60000);
      if (fake.minute.length >= fake.limit) return reply(problem(429, 'rate_limited', 'Over 60 requests this minute.'));
      fake.minute.push(time);

      const index = fake.failures.findIndex(
        (f) => f.method === req.method && (f.path instanceof RegExp ? f.path.test(path) : f.path === path),
      );
      const failure = index >= 0 ? fake.failures.splice(index, 1)[0].how : null;
      if (failure === 'drop-before') return req.socket.destroy();
      if (failure && failure !== 'drop') return reply(failure);
      const result = route(req.method, path, url.searchParams, body);
      if (failure === 'drop') return req.socket.destroy();
      return reply(result);
    });
  });
  const port = await listen(server);
  fake.base = `http://127.0.0.1:${port}/api/v1/talk`;
  fake.close = () => closeServer(server);
  return fake;
}

/** The storage interface of `src/db.js`, kept in arrays. */
export function createMemoryDb() {
  const people = [];
  const rooms = [];
  const members = [];
  const messages = [];
  const calls = [];
  const phoneCalls = [];
  const recordings = [];
  const directMessages = [];
  const directReads = [];
  let messageId = 0;
  let directId = 0;
  let down = false;

  const copy = (row) => (row ? { ...row } : null);
  const time = (value) => new Date(value).getTime();
  const check = () => {
    if (down) throw new DatabaseDown();
  };
  function patchRow(list, allowed, id, patch, from) {
    const row = list.find((r) => r.id === id);
    if (!row || (from && !from.includes(row.status))) return null;
    for (const name of allowed) if (patch[name] !== undefined) row[name] = patch[name];
    return copy(row);
  }
  const unreadOf = (room, identity) => {
    const me = members.find((m) => m.room === room && m.identity === identity);
    if (!me) return 0;
    return messages.filter((x) => x.room === room && x.id > me.last_read_id && x.from_identity !== identity).length;
  };
  const between = (a, b) => directMessages.filter((x) => (x.low === a && x.high === b) || (x.low === b && x.high === a));
  const directUnread = (identity, peer) => {
    const mark = directReads.find((r) => r.identity === identity && r.peer === peer)?.last_read_id ?? 0;
    return between(identity, peer).filter((x) => x.from_identity !== identity && x.id > mark).length;
  };
  const newest = (list) => [...list].sort((a, b) => time(b.created_at) - time(a.created_at) || (a.id < b.id ? -1 : 1));

  const db = {
    /** Makes every call fail as an unreachable database does. */
    setDown(value) {
      down = value;
    },
    tables: { people, rooms, members, messages, calls, phoneCalls, recordings, directMessages, directReads },

    async migrate() {
      check();
    },
    async ensure() {
      check();
    },
    ready: () => !down,
    async ping() {
      check();
    },
    async close() {},

    async getPerson(deviceId) {
      check();
      return copy(people.find((p) => p.device_id === deviceId));
    },
    async upsertPerson(deviceId, { identity = null, name = null, at }) {
      check();
      let row = people.find((p) => p.device_id === deviceId);
      const wanted = row?.identity ?? identity;
      if (wanted && people.some((p) => p.identity === wanted && p.device_id !== deviceId)) return null;
      if (!row) {
        row = { device_id: deviceId, identity: null, name: null, created_at: at, seen_at: at };
        people.push(row);
      }
      row.identity = wanted;
      row.name = name ?? row.name;
      row.seen_at = at;
      return copy(row);
    },
    async touchPerson(deviceId, at) {
      check();
      const row = people.find((p) => p.device_id === deviceId);
      if (row) row.seen_at = at;
    },
    async listPeople(identity, limit) {
      check();
      return people
        .filter((p) => p.identity && p.identity !== identity)
        .sort((a, b) => time(b.seen_at) - time(a.seen_at) || (a.identity < b.identity ? -1 : 1))
        .slice(0, limit)
        .map(copy);
    },
    async peopleByIdentity(identities) {
      check();
      return people.filter((p) => identities.includes(p.identity)).map(copy);
    },

    async createRoom({ name, displayName, createdBy, at }) {
      check();
      const existing = rooms.find((r) => r.name === name);
      if (existing) return { row: copy(existing), created: false };
      const row = { name, display_name: displayName, created_by: createdBy, created_at: at, active_at: at };
      rooms.push(row);
      return { row: copy(row), created: true };
    },
    async getRoom(name) {
      check();
      return copy(rooms.find((r) => r.name === name));
    },
    async deleteRoom(name) {
      check();
      const index = rooms.findIndex((r) => r.name === name);
      if (index < 0) return false;
      rooms.splice(index, 1);
      for (const list of [members, messages]) {
        for (let i = list.length - 1; i >= 0; i -= 1) if (list[i].room === name) list.splice(i, 1);
      }
      return true;
    },
    async touchRoom(name, at) {
      check();
      const row = rooms.find((r) => r.name === name);
      if (row && time(at) > time(row.active_at)) row.active_at = at;
    },
    async listRooms(identity, { name = null, memberOnly = false, limit = 100 } = {}) {
      check();
      return rooms
        .filter((r) => name === null || r.name === name)
        .map((r) => {
          const member = members.some((m) => m.room === r.name && m.identity === identity);
          const last = messages.filter((x) => x.room === r.name).at(-1);
          return {
            ...r,
            members: members.filter((m) => m.room === r.name).length,
            member,
            unread: unreadOf(r.name, identity),
            last_id: last ? last.id : null,
            last_from: last ? last.from_identity : null,
            last_text: last ? last.text : null,
            last_at: last ? last.created_at : null,
            last_delivered: last ? last.delivered : null,
          };
        })
        .filter((r) => !memberOnly || r.member)
        .sort((a, b) => time(b.active_at) - time(a.active_at) || (a.name < b.name ? -1 : 1))
        .slice(0, limit);
    },
    async joinRoom(room, identity, at) {
      check();
      if (!members.some((m) => m.room === room && m.identity === identity)) {
        members.push({ room, identity, last_read_id: 0, joined_at: at });
      }
    },
    async markRead(room, identity, upTo, at) {
      check();
      await db.joinRoom(room, identity, at);
      const top = messages.filter((x) => x.room === room).reduce((max, x) => Math.max(max, x.id), 0);
      const me = members.find((m) => m.room === room && m.identity === identity);
      me.last_read_id = Math.max(me.last_read_id, Math.min(upTo ?? top, top));
      return unreadOf(room, identity);
    },
    async unread(room, identity) {
      check();
      return unreadOf(room, identity);
    },

    async addMessage({ room, from, text, clientId = null, at }) {
      check();
      if (clientId !== null) {
        const first = messages.find((x) => x.from_identity === from && x.client_id === clientId);
        if (first) return { row: copy(first), created: false };
      }
      messageId += 1;
      const row = { id: messageId, room, from_identity: from, text, client_id: clientId, delivered: false, created_at: at };
      messages.push(row);
      await db.touchRoom(room, at);
      return { row: copy(row), created: true };
    },
    async setDelivered(id, delivered) {
      check();
      messages.find((x) => x.id === id).delivered = delivered;
    },
    async listMessages(room, { after = null, limit }) {
      check();
      const all = messages.filter((x) => x.room === room);
      const lastId = all.length ? all.at(-1).id : 0;
      const rows = (after === null ? all.slice(-limit) : all.filter((x) => x.id > after).slice(0, limit)).map(copy);
      return { rows, lastId, more: after !== null && rows.length > 0 && rows.at(-1).id < lastId };
    },

    async addDirect({ from, to, text, clientId = null, at }) {
      check();
      if (clientId !== null) {
        const first = directMessages.find((x) => x.from_identity === from && x.client_id === clientId);
        if (first) return { row: copy(first), created: false };
      }
      directId += 1;
      const [low, high] = pairOf(from, to);
      const row = { id: directId, low, high, from_identity: from, text, client_id: clientId, created_at: at };
      directMessages.push(row);
      return { row: copy(row), created: true };
    },
    async listDirect(a, b, { after = null, limit }) {
      check();
      const all = between(a, b);
      const lastId = all.length ? all.at(-1).id : 0;
      const rows = (after === null ? all.slice(-limit) : all.filter((x) => x.id > after).slice(0, limit)).map(copy);
      return { rows, lastId, more: after !== null && rows.length > 0 && rows.at(-1).id < lastId };
    },
    async markDirectRead(identity, peer, upTo) {
      check();
      const top = between(identity, peer).reduce((max, x) => Math.max(max, x.id), 0);
      let mark = directReads.find((r) => r.identity === identity && r.peer === peer);
      if (!mark) {
        mark = { identity, peer, last_read_id: 0 };
        directReads.push(mark);
      }
      mark.last_read_id = Math.max(mark.last_read_id, Math.min(upTo ?? top, top));
      return directUnread(identity, peer);
    },
    async listConversations(identity, limit) {
      check();
      const peers = new Set(
        directMessages.filter((x) => x.low === identity || x.high === identity).map((x) => (x.low === identity ? x.high : x.low)),
      );
      return [...peers]
        .map((peer) => {
          const last = between(identity, peer).at(-1);
          return {
            peer,
            last_id: last.id,
            last_from: last.from_identity,
            last_text: last.text,
            last_at: last.created_at,
            unread: directUnread(identity, peer),
          };
        })
        .sort((a, b) => b.last_id - a.last_id)
        .slice(0, limit);
    },

    async insertCall({ id, from, to, video, status, room = null, at }) {
      check();
      const existing = calls.find((c) => c.id === id);
      if (existing) return copy(existing);
      const row = {
        id, from_identity: from, to_identity: to, video, status, room,
        created_at: at, answered_at: null, ended_at: null, seconds: 0, checked_at: at,
      };
      calls.push(row);
      return copy(row);
    },
    async getCall(id) {
      check();
      return copy(calls.find((c) => c.id === id));
    },
    async updateCall(id, patch, from) {
      check();
      return patchRow(calls, ['status', 'room', 'answered_at', 'ended_at', 'seconds', 'checked_at'], id, patch, from);
    },
    async ringingFor(identity) {
      check();
      return newest(calls.filter((c) => c.to_identity === identity && c.status === 'ringing')).reverse().map(copy);
    },
    async listCalls(identity, limit) {
      check();
      return newest(calls.filter((c) => c.from_identity === identity || c.to_identity === identity)).slice(0, limit).map(copy);
    },
    async liveCalls() {
      check();
      return calls.filter((c) => c.status === 'ringing' || c.status === 'active').map(copy);
    },

    async reservePhoneCall({ id, identity, requestId = null, to, at }) {
      check();
      if (requestId !== null) {
        const earlier = phoneCalls.find((c) => c.identity === identity && c.request_id === requestId);
        if (earlier) return { row: copy(earlier), created: false };
      }
      if (phoneCalls.some((c) => c.identity === identity && ['dialing', 'ringing', 'active'].includes(c.status))) {
        return { busy: true };
      }
      const row = {
        id, upstream_id: null, identity, request_id: requestId, to_number: to, caller_id: null, status: 'dialing',
        created_at: at, started_at: null, ended_at: null, seconds: 0, price_tzs: null, left_at: null,
      };
      phoneCalls.push(row);
      return { row: copy(row), created: true };
    },
    async getPhoneCall(id) {
      check();
      return copy(phoneCalls.find((c) => c.id === id));
    },
    async updatePhoneCall(id, patch, from) {
      check();
      return patchRow(
        phoneCalls,
        ['upstream_id', 'caller_id', 'status', 'started_at', 'ended_at', 'seconds', 'price_tzs', 'left_at'],
        id, patch, from,
      );
    },
    async deletePhoneCall(id) {
      check();
      const index = phoneCalls.findIndex((c) => c.id === id);
      if (index >= 0) phoneCalls.splice(index, 1);
    },
    async listPhoneCalls(identity, limit) {
      check();
      return newest(phoneCalls.filter((c) => c.identity === identity)).slice(0, limit).map(copy);
    },
    async livePhoneCalls() {
      check();
      return phoneCalls.filter((c) => ['ringing', 'active'].includes(c.status) && c.upstream_id).map(copy);
    },
    async lastCallerId() {
      check();
      const row = newest(phoneCalls.filter((c) => c.caller_id))[0];
      return row ? row.caller_id : null;
    },

    async reserveRecording({ id, targetRoom, room = null, callId = null, startedBy, at }) {
      check();
      const running = recordings.find((r) => r.target_room === targetRoom && ['starting', 'recording'].includes(r.status));
      if (running) return { row: copy(running), created: false };
      const row = {
        id, upstream_id: null, target_room: targetRoom, room, call_id: callId, started_by: startedBy, status: 'starting',
        created_at: at, ready_at: null, seconds: 0, size_bytes: null, price_tzs: null, url: null, url_at: null, checked_at: at,
      };
      recordings.push(row);
      return { row: copy(row), created: true };
    },
    async getRecording(id) {
      check();
      return copy(recordings.find((r) => r.id === id));
    },
    async updateRecording(id, patch, from) {
      check();
      return patchRow(
        recordings,
        ['upstream_id', 'status', 'ready_at', 'seconds', 'size_bytes', 'price_tzs', 'url', 'url_at', 'checked_at'],
        id, patch, from,
      );
    },
    async deleteRecording(id) {
      check();
      const index = recordings.findIndex((r) => r.id === id);
      if (index >= 0) recordings.splice(index, 1);
    },
    async listRecordings({ startedBy = null, room = null, callId = null }, limit) {
      check();
      return newest(
        recordings.filter(
          (r) => (startedBy !== null && r.started_by === startedBy) || (room !== null && r.room === room) || (callId !== null && r.call_id === callId),
        ),
      ).slice(0, limit).map(copy);
    },
    async runningRecordings() {
      check();
      return recordings.filter((r) => r.status === 'recording').map(copy);
    },
    async failStale(before) {
      check();
      for (const c of phoneCalls) if (c.status === 'dialing' && time(c.created_at) < time(before)) Object.assign(c, { status: 'failed', ended_at: before });
      for (const r of recordings) if (r.status === 'starting' && time(r.created_at) < time(before)) r.status = 'failed';
    },
    async spentTzs() {
      check();
      return [...phoneCalls, ...recordings].reduce((sum, r) => sum + (r.price_tzs ?? 0), 0);
    },
  };
  return db;
}

/** A clock the test moves by hand. */
export function createClock(start = Date.parse('2026-11-15T07:00:00Z')) {
  let time = start;
  const now = () => time;
  now.advance = (ms) => {
    time += ms;
  };
  return now;
}

/**
 * The real app between a database (`db`, default in memory) and a fake
 * Barua Talk. `env` adds to the configuration; `key: false` runs the
 * server without a Barua key. Returns `{api, fake, db, now, app, talk,
 * lines, close}`; `api(method, path, {body, as, headers})` answers
 * `{status, json, headers}`.
 */
export async function createHarness({ env = {}, key = true, db = createMemoryDb(), fake } = {}) {
  const upstream = fake ?? (await createFakeTalk());
  const now = createClock();
  const config = loadConfig({
    DATABASE_URL: 'postgres://unused',
    BARUA_API_BASE: upstream.base,
    BARUA_API_KEY: key ? TEST_KEY : '',
    BARUA_TIMEOUT_MS: '2000',
    ...env,
  });
  const lines = [];
  const log = createLogger({ secrets: [TEST_KEY], sink: (line) => lines.push(line) });
  const talk = createTalk(config, log, { now });
  const app = createApp({ config, db, talk, log, now, background: false });
  const server = http.createServer((req, res) => app.handle(req, res));
  const port = await listen(server);

  async function api(method, path, { body, as = 'device-neema-01', headers = {} } = {}) {
    const response = await fetch(`http://127.0.0.1:${port}${path}`, {
      method,
      headers: {
        ...(as ? { 'X-Device-Id': as } : {}),
        ...(body !== undefined ? { 'Content-Type': 'application/json' } : {}),
        ...headers,
      },
      body: body === undefined ? undefined : typeof body === 'string' ? body : JSON.stringify(body),
    });
    const textBody = await response.text();
    let json = null;
    try {
      json = JSON.parse(textBody);
    } catch {
      json = null;
    }
    return { status: response.status, json, headers: response.headers };
  }

  return {
    api,
    fake: upstream,
    db,
    now,
    app,
    talk,
    config,
    lines,
    async close() {
      app.close();
      await closeServer(server);
      if (!fake) await upstream.close();
    },
  };
}
