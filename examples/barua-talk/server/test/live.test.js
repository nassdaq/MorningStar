// LIVE TEST, OPT-IN: the real Barua Talk API, through this server.
//
// It runs only when asked for, and only with a key:
//
//     BARUA_LIVE=1 node --test test/live.test.js
//
// (`npm test` skips it.) It uses the **free** endpoints only: rooms,
// tickets, room chat and in-app calls. It can never dial a phone, send an
// SMS or start a recording: the `fetch` it gives the client refuses those
// requests before they leave (`guard` below), whatever the code under test
// tries to do. Everything it makes upstream is named `mstest-…` and is
// closed or ended at the end. About 15 upstream requests, far below the
// 60 a minute the workspace is allowed. The database is the in-memory one.

import assert from 'node:assert/strict';
import { randomBytes } from 'node:crypto';
import { readFileSync } from 'node:fs';
import http from 'node:http';
import { after, before, describe, test } from 'node:test';

import { createApp } from '../src/app.js';
import { ENV_PATH, loadConfig, parseEnv } from '../src/config.js';
import { createLogger } from '../src/log.js';
import { createTalk } from '../src/talk.js';
import { createMemoryDb } from './fakes.js';

let fileEnv = {};
try {
  fileEnv = parseEnv(readFileSync(ENV_PATH, 'utf8'));
} catch {
  // No .env: skipped.
}
const apiKey = process.env.BARUA_API_KEY ?? fileEnv.BARUA_API_KEY ?? '';
const skip = process.env.BARUA_LIVE !== '1' ? 'opt-in: set BARUA_LIVE=1' : apiKey ? false : 'needs BARUA_API_KEY in .env';

/** Priced endpoints never leave this process. */
const PRICED = [/\/talk\/calls(\/|\?|$)/, /\/talk\/messages(\/|\?|$)/, /\/talk\/recordings(\/|\?|$)/];
const sent = [];
async function guard(url, init) {
  const method = init?.method ?? 'GET';
  if (method !== 'GET' && PRICED.some((pattern) => pattern.test(String(url)))) {
    throw new Error(`the live test refused a priced request: ${method} ${new URL(url).pathname}`);
  }
  // A name that does not resolve at the first attempt often does at the
  // third: that is this machine's network. Only a request that never left
  // is tried again.
  for (let attempt = 1; ; attempt += 1) {
    try {
      const response = await fetch(url, init);
      sent.push(`${method} ${new URL(url).pathname.replace(/^.*\/talk/, '')} ${response.status}`);
      return response;
    } catch (err) {
      const code = err?.cause?.code ?? '';
      if (attempt === 4 || !['ENOTFOUND', 'EAI_AGAIN'].includes(code)) throw err;
      await new Promise((resolve) => setTimeout(resolve, 2000));
    }
  }
}

describe('LIVE: the real Barua Talk API, free endpoints only', { skip, concurrency: false }, () => {
  const tag = randomBytes(3).toString('hex');
  const A = `mstest-device-a-${tag}`;
  const B = `mstest-device-b-${tag}`;
  const room = `mstest-room-${tag}`;
  let server;
  let app;
  let base;
  let callId = null;

  async function api(method, path, { body, as = A } = {}) {
    const response = await fetch(`${base}${path}`, {
      method,
      headers: { 'X-Device-Id': as, ...(body ? { 'Content-Type': 'application/json' } : {}) },
      body: body ? JSON.stringify(body) : undefined,
    });
    return { status: response.status, json: await response.json() };
  }

  before(async () => {
    const config = loadConfig({ BARUA_API_KEY: apiKey, BARUA_API_BASE: process.env.BARUA_API_BASE ?? fileEnv.BARUA_API_BASE ?? '', DATABASE_URL: 'postgres://unused' });
    const log = createLogger({ secrets: [apiKey], quiet: true });
    const talk = createTalk(config, log, { fetchImpl: guard });
    app = createApp({ config, db: createMemoryDb(), talk, log, background: false });
    server = http.createServer((req, res) => app.handle(req, res));
    await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
    base = `http://127.0.0.1:${server.address().port}`;
    assert.equal((await api('PUT', '/v1/me', { as: A, body: { name: `mstest a ${tag}` } })).json.identity, `mstest-a-${tag}`);
    assert.equal((await api('PUT', '/v1/me', { as: B, body: { name: `mstest b ${tag}` } })).json.identity, `mstest-b-${tag}`);
  });

  after(async () => {
    // Leave nothing behind, whatever failed above.
    if (callId) await api('DELETE', `/v1/calls/${callId}`).catch(() => {});
    await api('DELETE', `/v1/rooms/${room}`).catch(() => {});
    app.close();
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
    console.log(`    upstream requests: ${sent.join(', ') || 'none'}`);
  });

  test('the guard refuses priced requests', async () => {
    for (const path of ['/calls', '/messages', '/recordings']) {
      await assert.rejects(guard(`https://barua.tz/api/v1/talk${path}`, { method: 'POST' }), /refused a priced request/);
    }
    await assert.rejects(guard('https://barua.tz/api/v1/talk/recordings/abc', { method: 'DELETE' }), /refused a priced request/);
  });

  test('a room: made, made again, ticketed, written in, closed', async () => {
    const made = await api('POST', '/v1/rooms', { body: { display_name: `mstest room ${tag}` } });
    assert.equal(made.status, 201, JSON.stringify(made.json));
    assert.equal(made.json.room.name, room);
    assert.equal((await api('POST', '/v1/rooms', { as: B, body: { display_name: `mstest room ${tag}` } })).status, 200);

    const ticket = await api('POST', `/v1/rooms/${room}/ticket`, { body: { video: true } });
    assert.equal(ticket.status, 200, JSON.stringify(ticket.json));
    assert.equal(ticket.json.token.split('.').length, 3, 'a ticket is a JWT');
    assert.match(ticket.json.url, /^wss?:\/\//);

    const posted = await api('POST', `/v1/rooms/${room}/messages`, { body: { text: 'mstest: habari' } });
    assert.equal(posted.status, 201, JSON.stringify(posted.json));
    assert.equal(posted.json.message.delivered, true, 'the forward to the room was accepted');

    assert.deepEqual((await api('DELETE', `/v1/rooms/${room}`)).json, { closed: true });
  });

  test('an in-app call: rung, seen, accepted, watched, hung up; then one declined', async () => {
    const rung = await api('POST', '/v1/calls', { body: { to: `mstest-b-${tag}`, video: false } });
    assert.equal(rung.status, 201, JSON.stringify(rung.json));
    callId = rung.json.call.id;
    assert.equal(rung.json.token.split('.').length, 3);
    assert.deepEqual((await api('GET', '/v1/calls/incoming', { as: B })).json.calls.map((c) => c.id), [callId]);

    const accepted = await api('POST', `/v1/calls/${callId}/accept`, { as: B });
    assert.equal(accepted.status, 200, JSON.stringify(accepted.json));
    assert.equal(accepted.json.call.status, 'active');
    assert.equal(accepted.json.token.split('.').length, 3);

    const over = await api('DELETE', `/v1/calls/${callId}`);
    assert.equal(over.status, 200, JSON.stringify(over.json));
    assert.ok(['ended', 'missed'].includes(over.json.call.status), over.json.call.status);
    callId = null;

    const second = await api('POST', '/v1/calls', { body: { to: `mstest-b-${tag}`, video: true } });
    assert.equal(second.status, 201, JSON.stringify(second.json));
    callId = second.json.call.id;
    assert.equal((await api('POST', `/v1/calls/${callId}/decline`, { as: B })).json.call.status, 'declined');
    callId = null;
  });
});
