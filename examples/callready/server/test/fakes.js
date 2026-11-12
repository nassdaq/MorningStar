// Test doubles: an in-memory database with the interface of `src/db.js`,
// and local HTTP servers standing in for DeepSeek and ElevenLabs. The real
// `model.js` and `voice.js` clients talk to these servers, so their request
// building, parsing, retry and timeout code is what the tests exercise.

import http from 'node:http';

import { createApp } from '../src/app.js';
import { loadConfig } from '../src/config.js';
import { createLogger } from '../src/log.js';
import { createModel } from '../src/model.js';
import { createVoice } from '../src/voice.js';

export const GOOD_REPLY = { line: 'Fine. But I want it fixed today.', tips: ['Give a clear next step', 'Confirm the details'], done: false };
export const GOOD_REPORT = {
  skills: { clarity: 88, empathy: 74, listening: 85, problem_solving: 79, professional_language: 84 },
  score: 82,
  did_well: 'You apologised early and kept a calm tone.',
  try_next: 'Ask for the order number before explaining.',
  notes: [{ n: 2, note: 'Took ownership' }],
};

function listen(server) {
  return new Promise((resolve) => {
    server.listen(0, '127.0.0.1', () => resolve(`http://127.0.0.1:${server.address().port}`));
  });
}

function closeServer(server) {
  return new Promise((resolve) => {
    server.close(resolve);
    server.closeAllConnections();
  });
}

function readAll(req) {
  return new Promise((resolve) => {
    const chunks = [];
    req.on('data', (c) => chunks.push(c));
    req.on('end', () => resolve(Buffer.concat(chunks)));
  });
}

/**
 * A fake DeepSeek. Each request takes the next entry of `script`
 * (`{status, content, delayMs}`; `content` may be an object or a raw
 * string); with the script empty it answers well, as the caller or as the
 * scorer depending on the system prompt.
 */
export async function startFakeDeepSeek() {
  const requests = [];
  const script = [];
  const server = http.createServer(async (req, res) => {
    const body = JSON.parse((await readAll(req)).toString('utf8'));
    requests.push({ path: req.url, authorization: req.headers.authorization, body });
    const scoring = body.messages[0].content.includes('call-centre trainer');
    const step = script.shift() ?? {};
    const content = step.content ?? (scoring ? GOOD_REPORT : GOOD_REPLY);
    if (step.delayMs) await new Promise((r) => setTimeout(r, step.delayMs));
    const status = step.status ?? 200;
    res.writeHead(status, { 'Content-Type': 'application/json' });
    if (status !== 200) {
      res.end(JSON.stringify({ error: { message: 'fake failure' } }));
      return;
    }
    res.end(
      JSON.stringify({
        model: body.model,
        choices: [{ index: 0, finish_reason: 'stop', message: { role: 'assistant', content: typeof content === 'string' ? content : JSON.stringify(content) } }],
      }),
    );
  });
  const url = await listen(server);
  return {
    url,
    requests,
    script,
    count: (kind) => requests.filter((r) => r.body.messages[0].content.includes('call-centre trainer') === (kind === 'score')).length,
    close: () => closeServer(server),
  };
}

export const FAKE_VOICES = [
  { voice_id: 'v_first', name: 'First', labels: {} },
  { voice_id: 'v_us_f', name: 'Sarah', labels: { accent: 'american', gender: 'female' } },
  { voice_id: 'v_us_m', name: 'Adam', labels: { accent: 'american', gender: 'male' } },
  { voice_id: 'v_uk_m', name: 'George', labels: { accent: 'british', gender: 'male' } },
  { voice_id: 'v_au_m', name: 'Charlie', labels: { accent: 'Australian', gender: 'male' } },
  { voice_id: 'v_in_f', name: 'Anika', labels: {}, verified_languages: [{ language: 'en', model_id: 'eleven_flash_v2_5', accent: 'indian', locale: 'en-IN' }] },
];

/**
 * A fake ElevenLabs with the three endpoints the server uses. `state` can
 * be changed by a test: `ttsStatus`, `sttStatus`, `transcript`, `voices`.
 */
export async function startFakeElevenLabs() {
  const requests = [];
  const state = { ttsStatus: 200, sttStatus: 200, voicesStatus: 200, transcript: 'I am sorry about that, let me look into it.', voices: FAKE_VOICES };
  const server = http.createServer(async (req, res) => {
    const url = new URL(req.url, 'http://x');
    const raw = await readAll(req);
    const entry = { method: req.method, path: url.pathname, query: Object.fromEntries(url.searchParams), key: req.headers['xi-api-key'], type: req.headers['content-type'] ?? '', raw };
    requests.push(entry);
    const json = (status, value) => {
      res.writeHead(status, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify(value));
    };
    if (entry.key !== 'el-test-key') return json(401, { detail: { status: 'invalid_api_key' } });

    if (req.method === 'GET' && url.pathname === '/v2/voices') {
      if (state.voicesStatus !== 200) return json(state.voicesStatus, { detail: 'fake failure' });
      // Two pages, to exercise next_page_token.
      const half = Math.ceil(state.voices.length / 2);
      const second = url.searchParams.get('next_page_token') === 'page2';
      return json(200, {
        voices: second ? state.voices.slice(half) : state.voices.slice(0, half),
        has_more: !second && state.voices.length > half,
        next_page_token: second ? null : 'page2',
      });
    }
    const tts = /^\/v1\/text-to-speech\/([^/]+)$/.exec(url.pathname);
    if (req.method === 'POST' && tts) {
      entry.voiceId = tts[1];
      entry.body = JSON.parse(raw.toString('utf8'));
      if (state.ttsStatus !== 200) return json(state.ttsStatus, { detail: 'fake failure' });
      res.writeHead(200, { 'Content-Type': 'audio/mpeg' });
      return res.end(Buffer.from(`ID3 fake mp3 for: ${entry.body.text}`));
    }
    if (req.method === 'POST' && url.pathname === '/v1/speech-to-text') {
      if (state.sttStatus !== 200) return json(state.sttStatus, { detail: 'fake failure' });
      return json(200, { language_code: 'eng', language_probability: 0.99, text: state.transcript, words: [] });
    }
    return json(404, { detail: 'not found' });
  });
  const url = await listen(server);
  return { url, requests, state, of: (path) => requests.filter((r) => r.path.startsWith(path)), close: () => closeServer(server) };
}

/** The storage interface of `src/db.js`, in memory. */
export function createFakeDb() {
  const profiles = new Map();
  const calls = new Map();
  const turns = new Map();
  const reports = new Map();
  const state = { failing: false };

  const profile = (deviceId) => {
    if (!profiles.has(deviceId)) {
      profiles.set(deviceId, { device_id: deviceId, name: '', accent: 'us', difficulty: 'standard', minutes: 4, plan: 'free' });
    }
    return profiles.get(deviceId);
  };
  const scored = (deviceId) => [...calls.values()].filter((c) => c.device_id === deviceId && reports.has(c.id));
  const guard = () => {
    if (state.failing) throw new Error('connection to postgres://user:hunter2secret@db.internal/app failed');
  };

  return {
    state,
    calls,
    turns,
    async migrate() {},
    async ping() {
      guard();
    },
    async close() {},
    async getOrCreateProfile(deviceId) {
      guard();
      return { ...profile(deviceId) };
    },
    async updateProfile(deviceId, patch) {
      guard();
      const row = profile(deviceId);
      for (const [key, value] of Object.entries(patch)) if (value !== undefined && value !== null) row[key] = value;
      return { ...row };
    },
    async stats(deviceId) {
      const mine = scored(deviceId);
      const total = mine.reduce((sum, c) => sum + reports.get(c.id).score, 0);
      const days = [...new Set(mine.map((c) => c.started_at.toISOString().slice(0, 10)))].sort().reverse();
      return { calls: mine.length, average_score: mine.length ? Math.round(total / mine.length) : null, days };
    },
    async createCall({ id, deviceId, scenario, accent, difficulty, minutes, voice, opening, tips, startedAt }) {
      guard();
      profile(deviceId);
      const number = scored(deviceId).reduce((max, c) => Math.max(max, c.number), 0) + 1;
      const call = { id, device_id: deviceId, number, scenario, accent, difficulty, minutes, voice, status: 'active', caller_done: false, started_at: startedAt, ended_at: null, duration_seconds: null };
      calls.set(id, call);
      turns.set(id, [{ seq: 1, speaker: 'caller', text: opening, tips, note: null }]);
      return { ...call };
    },
    async getCall(id) {
      guard();
      const call = calls.get(id);
      return call ? { ...call } : null;
    },
    async getTurns(callId) {
      return (turns.get(callId) ?? []).map((t) => ({ ...t }));
    },
    async addExchange(callId, { seq, learnerText, callerText, tips, done }) {
      const list = turns.get(callId);
      if (seq !== list.length) throw new Error('duplicate key value violates unique constraint');
      list.push({ seq: seq + 1, speaker: 'learner', text: learnerText, tips: null, note: null });
      list.push({ seq: seq + 2, speaker: 'caller', text: callerText, tips, note: null });
      if (done) calls.get(callId).caller_done = true;
    },
    async endCall(callId, { endedAt, durationSeconds, report }) {
      const call = calls.get(callId);
      if (call.status !== 'active') return false;
      Object.assign(call, { status: 'ended', ended_at: endedAt, duration_seconds: durationSeconds });
      if (!report) return true;
      reports.set(callId, { score: report.score, ...report.skills, did_well: report.did_well, try_next: report.try_next });
      for (const [seq, note] of Object.entries(report.notes ?? {})) {
        const turn = turns.get(callId).find((t) => t.seq === Number(seq) && t.speaker === 'learner');
        if (turn) turn.note = note;
      }
      return true;
    },
    async getReport(callId) {
      return reports.get(callId) ?? null;
    },
    async listCalls(deviceId, limit) {
      return scored(deviceId)
        .filter((c) => c.status === 'ended')
        .sort((a, b) => b.started_at - a.started_at)
        .slice(0, limit)
        .map((c) => ({ id: c.id, number: c.number, scenario: c.scenario, started_at: c.started_at, duration_seconds: c.duration_seconds, minutes: c.minutes, score: reports.get(c.id).score }));
    },
  };
}

/**
 * The whole server on a random port, over an in-memory database and the
 * fake upstreams. `env` overrides configuration; `.env` is never read.
 * `voice: true` gives it a (fake) ElevenLabs key.
 */
export async function startTestServer({ env = {}, voice = false } = {}) {
  const deepseek = await startFakeDeepSeek();
  const eleven = await startFakeElevenLabs();
  const config = loadConfig({
    DEEPSEEK_API_KEY: 'ds-test-key',
    DEEPSEEK_BASE_URL: deepseek.url,
    ELEVENLABS_API_KEY: voice ? 'el-test-key' : '',
    ELEVENLABS_BASE_URL: eleven.url,
    ...env,
  });
  const lines = [];
  const log = createLogger({ secrets: ['ds-test-key', 'el-test-key', 'hunter2secret'], sink: (line) => lines.push(line) });
  const clock = { offset: 0 };
  const now = () => Date.now() + clock.offset;
  const db = createFakeDb();
  const model = createModel(config, log);
  const voiceClient = createVoice(config, log, { now });
  await voiceClient.init();
  const app = createApp({ config, db, model, voice: voiceClient, log, now });
  const server = http.createServer((req, res) => app.handle(req, res));
  const url = await listen(server);

  /** One request. `body` objects are sent as JSON; Buffers as they are. */
  async function api(method, path, { device = 'test_device_one', body, headers = {} } = {}) {
    const all = { ...headers };
    if (device) all['X-Device-Id'] = device;
    let payload;
    if (Buffer.isBuffer(body)) payload = body;
    else if (body !== undefined) {
      payload = JSON.stringify(body);
      all['Content-Type'] ??= 'application/json';
    }
    const response = await fetch(`${url}${path}`, { method, headers: all, body: payload });
    const bytes = Buffer.from(await response.arrayBuffer());
    let json = null;
    if ((response.headers.get('content-type') ?? '').includes('json')) json = JSON.parse(bytes.toString('utf8'));
    return { status: response.status, json, bytes, headers: response.headers };
  }

  return {
    url,
    api,
    db,
    deepseek,
    eleven,
    clock,
    lines,
    app,
    async close() {
      app.close();
      await closeServer(server);
      await deepseek.close();
      await eleven.close();
    },
  };
}
