// What must never stop the server, and what it does while the database
// cannot be reached: it starts, says so, answers 503 where it needs the
// database, and connects by itself when the database is back. No real
// database, model or voice is used.

import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { EventEmitter } from 'node:events';
import http from 'node:http';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

import { loadConfig } from '../src/config.js';
import { createDb, isConnectionError } from '../src/db.js';
import { createLogger } from '../src/log.js';
import { guardProcess, start } from '../src/server.js';

const SERVER = fileURLToPath(new URL('../src/server.js', import.meta.url));
const refused = () => Object.assign(new Error('connect ECONNREFUSED 127.0.0.1:5432'), { code: 'ECONNREFUSED' });

/** A pool that is down until `up` is set, then answers like a migrated database. */
function makePool(state) {
  return class FakePool extends EventEmitter {
    constructor() {
      super();
      state.pool = this;
    }
    async query(text) {
      state.queries += 1;
      if (!state.up) throw refused();
      if (/schema_version/.test(text) && /select/.test(text)) return { rows: [{ version: 9999 }] };
      if (/count\(\*\)/.test(text)) return { rows: [{ calls: 0, average_score: null }] };
      if (/callready_profiles/.test(text)) {
        return { rows: [{ device_id: 'device-0001', name: '', accent: 'us', difficulty: 'standard', minutes: 4, plan: 'free' }], rowCount: 1 };
      }
      return { rows: [], rowCount: 0 };
    }
    async connect() {
      state.queries += 1;
      if (!state.up) throw refused();
      const pool = this;
      return { query: (text, params) => pool.query(text, params), release() {} };
    }
    async end() {}
  };
}

function get(port, path, headers = {}) {
  return new Promise((resolve, reject) => {
    http
      .get({ host: '127.0.0.1', port, path, headers }, (res) => {
        const chunks = [];
        res.on('data', (chunk) => chunks.push(chunk));
        res.on('end', () => {
          let json = null;
          try {
            json = JSON.parse(Buffer.concat(chunks).toString('utf8'));
          } catch {
            // Not JSON.
          }
          resolve({ status: res.statusCode, json, retryAfter: res.headers['retry-after'] });
        });
      })
      .on('error', reject);
  });
}

test('connection errors are told apart from wrong queries', () => {
  assert.ok(isConnectionError(refused()));
  assert.ok(isConnectionError(new Error('Connection terminated unexpectedly')));
  assert.ok(isConnectionError(Object.assign(new Error('x'), { code: '57P01' })));
  assert.ok(!isConnectionError(Object.assign(new Error('syntax error'), { code: '42601' })));
  assert.ok(!isConnectionError(null));
});

test('the server starts without the database, answers 503, and connects when it is back', async () => {
  const state = { up: false, queries: 0, pool: null };
  let clock = 0;
  const config = loadConfig({ DATABASE_URL: 'postgres://unused/db', PORT: '0', HOST: '127.0.0.1', DATABASE_RETRY_MS: '1000' });
  const running = await start(config, {
    quiet: true,
    dbOptions: { Pool: makePool(state), now: () => clock },
    connectAttempts: 2,
    connectPauseMs: 1,
  });
  const device = { 'X-Device-Id': 'device-0001' };
  try {
    const health = await get(running.port, '/v1/health');
    assert.equal(health.status, 200, 'the server is up');
    assert.equal(health.json.database, false, 'and says the database is not');

    clock += 2000;
    const down = await get(running.port, '/v1/profile', device);
    assert.equal(down.status, 503);
    assert.equal(down.json.error.code, 'database_unavailable');
    assert.equal(down.retryAfter, '5');

    // Within the pause after a failure the database is not tried again.
    const before = state.queries;
    const again = await get(running.port, '/v1/profile', device);
    assert.equal(again.status, 503);
    assert.equal(state.queries, before, 'a database that is down is not hammered');

    // The database comes back. Nobody restarts anything.
    state.up = true;
    clock += 2000;
    const back = await get(running.port, '/v1/profile', device);
    assert.equal(back.status, 200, JSON.stringify(back.json));

    // And goes away again in the middle of things: 503, not 500, and no crash.
    state.up = false;
    const lost = await get(running.port, '/v1/profile', device);
    assert.equal(lost.status, 503);
    state.up = true;
    assert.equal((await get(running.port, '/v1/profile', device)).status, 200);
  } finally {
    await running.close();
  }
});

test('a database connection that fails while a query holds it does not throw', () => {
  const state = { up: true, queries: 0, pool: null };
  const lines = [];
  const log = createLogger({ sink: (line) => lines.push(line) });
  createDb(loadConfig({ DATABASE_URL: 'postgres://unused/db' }), log, { Pool: makePool(state) });
  const client = new EventEmitter();
  state.pool.emit('connect', client);
  assert.doesNotThrow(() => client.emit('error', new Error('Connection terminated unexpectedly')));
  assert.doesNotThrow(() => state.pool.emit('error', new Error('idle client lost')));
  assert.equal(lines.length, 2);
});

test('an unhandled rejection and an uncaught exception are logged, and the process lives', async () => {
  const script = `
    import { guardProcess } from ${JSON.stringify(SERVER)};
    const lines = [];
    guardProcess({ error: (message, err) => lines.push(message + ' / ' + err.message) });
    Promise.reject(new Error('nobody awaited this'));
    setTimeout(() => { throw new Error('thrown from a timer'); }, 5);
    setTimeout(() => { console.log(JSON.stringify(lines)); }, 60);
  `;
  const { stdout } = await promisify(execFile)(process.execPath, ['--input-type=module', '-e', script], { timeout: 20000 });
  const lines = JSON.parse(stdout);
  assert.equal(lines.length, 2, stdout);
  assert.match(lines[0], /unhandled rejection; the server carries on \/ nobody awaited this/);
  assert.match(lines[1], /uncaughtException; the server carries on \/ thrown from a timer/);
  const proc = new EventEmitter();
  guardProcess({ error() {} }, proc)();
  assert.equal(proc.listenerCount('unhandledRejection') + proc.listenerCount('uncaughtException'), 0);
});
