// Test doubles: an in-memory database with the interface of `src/db.js`, a
// stand-in for `pg.Pool` that can be switched off and on, and local HTTP
// servers standing in for DeepSeek and ElevenLabs. The real `model.js` and
// `voice.js` clients talk to these servers, so their request building,
// parsing, retry and timeout code is what the tests exercise.

import http from 'node:http';

import { createApp } from '../src/app.js';
import { loadConfig, secrets } from '../src/config.js';
import { DatabaseDown } from '../src/errors.js';
import { createLogger } from '../src/log.js';
import { createModel } from '../src/model.js';
import { createVoice } from '../src/voice.js';

export const GOOD_ANSWER = {
  title: 'Comparing three quotes',
  steps: ['Reading the request', 'Comparing totals', 'Writing the answer'],
  answer: 'Quote B is the cheapest.\n\n- A: $4,440\n- B: $4,180\n- C: $4,690',
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
 * string); with the script empty it answers `GOOD_ANSWER`.
 */
export async function startFakeDeepSeek() {
  const requests = [];
  const script = [];
  const server = http.createServer(async (req, res) => {
    const body = JSON.parse((await readAll(req)).toString('utf8'));
    requests.push({ path: req.url, authorization: req.headers.authorization, body });
    const step = script.shift() ?? {};
    const content = step.content ?? GOOD_ANSWER;
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
  return { url, requests, script, close: () => closeServer(server) };
}

export const FAKE_VOICES = [
  { voice_id: 'v_es', name: 'Lucia', labels: { language: 'es' } },
  { voice_id: 'v_first', name: 'First', labels: { language: 'en', use_case: 'narration' } },
  { voice_id: 'v_chat', name: 'Sarah', labels: { language: 'en', use_case: 'conversational' } },
  { voice_id: 'v_other', name: 'Adam', labels: { language: 'en', use_case: 'social media' } },
];

/**
 * A fake ElevenLabs with the three endpoints the server uses. `state` can
 * be changed by a test: `ttsStatus`, `sttStatus`, `transcript`, `voices`.
 */
export async function startFakeElevenLabs() {
  const requests = [];
  const state = { ttsStatus: 200, sttStatus: 200, voicesStatus: 200, transcript: 'What is the capital of Tanzania?', voices: FAKE_VOICES };
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

/** The storage interface of `src/db.js`, in memory. `state.down` makes it unreachable. */
export function createFakeDb() {
  const profiles = new Map();
  const chats = new Map();
  const messages = new Map();
  const state = { down: false };

  const profile = (deviceId) => {
    if (!profiles.has(deviceId)) profiles.set(deviceId, { device_id: deviceId, model: 'auto', plan: 'free' });
    return profiles.get(deviceId);
  };
  const guard = () => {
    if (state.down) throw new DatabaseDown(new Error('connection to postgres://user:hunter2secret@db.internal/app failed'));
  };

  return {
    state,
    chats,
    messages,
    async migrate() {
      guard();
    },
    async ensure() {
      guard();
    },
    ready: () => !state.down,
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
    async listChats(deviceId, limit) {
      guard();
      return [...chats.values()]
        .filter((c) => c.device_id === deviceId)
        .sort((a, b) => b.updated_at - a.updated_at || (a.id < b.id ? -1 : 1))
        .slice(0, limit)
        .map((c) => ({ id: c.id, title: c.title, updated_at: c.updated_at, messages: messages.get(c.id).filter((m) => m.role === 'user').length }));
    },
    async getChat(id) {
      guard();
      const chat = chats.get(id);
      return chat ? { ...chat } : null;
    },
    async getMessages(chatId, limit) {
      guard();
      return (messages.get(chatId) ?? []).slice(-limit).map((m) => ({ ...m }));
    },
    async createChat({ id, deviceId, title, user, assistant, at }) {
      guard();
      profile(deviceId);
      chats.set(id, { id, device_id: deviceId, title, created_at: at, updated_at: at });
      messages.set(id, [
        { seq: 1, role: 'user', text: user.text, attachments: user.attachments ?? [], steps: null, model_used: null },
        { seq: 2, role: 'assistant', text: assistant.text, attachments: null, steps: assistant.steps, model_used: assistant.modelUsed },
      ]);
    },
    async addExchange(chatId, { seq, user, assistant, at }) {
      guard();
      const list = messages.get(chatId);
      if (seq !== list.length) throw new Error('duplicate key value violates unique constraint');
      list.push({ seq: seq + 1, role: 'user', text: user.text, attachments: user.attachments ?? [], steps: null, model_used: null });
      list.push({ seq: seq + 2, role: 'assistant', text: assistant.text, attachments: null, steps: assistant.steps, model_used: assistant.modelUsed });
      chats.get(chatId).updated_at = at;
    },
    async deleteChat(id, deviceId) {
      guard();
      const chat = chats.get(id);
      if (!chat || chat.device_id !== deviceId) return false;
      chats.delete(id);
      messages.delete(id);
      return true;
    },
  };
}

/**
 * A stand-in for `pg.Pool` for the real `createDb`: while `state.down` every
 * connection fails the way an unresolvable host does; otherwise every query
 * "works" and returns one row that satisfies the migration and the profile
 * queries. `state.connects` counts the attempts.
 */
export function createFakePool() {
  const state = { down: true, connects: 0, queries: [] };
  const fail = () => Object.assign(new Error('getaddrinfo ENOTFOUND db.internal'), { code: 'ENOTFOUND' });
  const run = async (text) => {
    state.queries.push(String(text));
    return { rows: [{ version: 99, device_id: 'test_device_one', model: 'auto', plan: 'free' }], rowCount: 1 };
  };
  class Pool {
    on() {}
    async connect() {
      state.connects += 1;
      if (state.down) throw fail();
      return { query: run, release() {} };
    }
    async query(text) {
      state.connects += 1;
      if (state.down) throw fail();
      return run(text);
    }
    async end() {}
  }
  return { Pool, state };
}

/**
 * The whole server on a random port, over an in-memory database and the
 * fake upstreams. `env` overrides configuration; `.env` is never read.
 * `voice: true` gives it a (fake) ElevenLabs key.
 */
export async function startTestServer({ env = {}, voice = false, db: givenDb } = {}) {
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
  // The real server's redaction list (`secrets`), plus the fake database's password.
  const log = createLogger({ secrets: [...secrets(config), 'hunter2secret'], sink: (line) => lines.push(line) });
  const clock = { offset: 0 };
  const now = () => Date.now() + clock.offset;
  const db = givenDb ? givenDb(config, log, now) : createFakeDb();
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
    config,
    async close() {
      app.close();
      await closeServer(server);
      await deepseek.close();
      await eleven.close();
    },
  };
}
