// What must never stop the server: an error nothing caught, and a database
// connection that drops while a query holds it. The server died once, on
// 2026-11-15, leaving nothing in its log but Node's version line; these
// are the ways a process ends like that, each closed and each tested.

import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { EventEmitter } from 'node:events';
import http from 'node:http';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

import { loadConfig } from '../src/config.js';
import { createDb } from '../src/db.js';
import { createLogger } from '../src/log.js';
import { guardProcess, start } from '../src/server.js';

const SERVER = fileURLToPath(new URL('../src/server.js', import.meta.url));

function collector() {
  const lines = [];
  return { lines, log: createLogger({ sink: (line) => lines.push(line) }) };
}

test('an unhandled rejection and an uncaught exception are logged, and the process lives', async () => {
  // In a process of its own: these are the events that end one.
  const script = `
    import { guardProcess } from ${JSON.stringify(SERVER)};
    const lines = [];
    guardProcess({ error: (message, err) => lines.push(message + ' / ' + err.message) });
    Promise.reject(new Error('nobody awaited this'));
    setTimeout(() => { throw new Error('thrown from a timer'); }, 5);
    setTimeout(() => { Promise.reject('not even an Error'); }, 10);
    setTimeout(() => { console.log(JSON.stringify(lines)); }, 60);
  `;
  const { stdout } = await promisify(execFile)(process.execPath, ['--input-type=module', '-e', script], { timeout: 20000 });
  const lines = JSON.parse(stdout);
  assert.equal(lines.length, 3, stdout);
  assert.match(lines[0], /unhandled rejection; the server carries on \/ nobody awaited this/);
  assert.match(lines[1], /uncaughtException; the server carries on \/ thrown from a timer/);
  assert.match(lines[2], /unhandled rejection.* \/ not even an Error/);
});

test('the guard redacts through the logger and can be taken off again', () => {
  const { lines, log } = collector();
  const proc = new EventEmitter();
  const off = guardProcess(log, proc);
  proc.emit('unhandledRejection', new Error('connect to postgres://user:hunter2@db.example/x failed'));
  assert.equal(lines.length, 1);
  assert.ok(!lines[0].includes('hunter2'), lines[0]);
  off();
  assert.equal(proc.listenerCount('unhandledRejection') + proc.listenerCount('uncaughtException'), 0);
});

test('a database connection that fails while a query holds it does not throw', () => {
  const pools = [];
  class FakePool extends EventEmitter {
    constructor() {
      super();
      pools.push(this);
    }
  }
  const { lines, log } = collector();
  createDb(loadConfig({ DATABASE_URL: 'postgres://unused/db' }), log, { Pool: FakePool });
  const [pool] = pools;
  // What pg does when it opens a connection, then what a dropped network
  // does to it. An emitter with no `error` listener throws on this.
  const client = new EventEmitter();
  pool.emit('connect', client);
  assert.doesNotThrow(() => client.emit('error', new Error('Connection terminated unexpectedly')));
  assert.doesNotThrow(() => pool.emit('error', new Error('idle client lost')));
  assert.equal(lines.length, 2);
  assert.match(lines[0], /database connection error .*Connection terminated unexpectedly/);
});

test('a handler that fails outside its own error handling answers 500 and the server keeps serving', async () => {
  // A pool that cannot connect: the server starts without the database.
  class DownPool extends EventEmitter {
    async connect() {
      throw Object.assign(new Error('connect ECONNREFUSED'), { code: 'ECONNREFUSED' });
    }
    async query() {
      throw Object.assign(new Error('connect ECONNREFUSED'), { code: 'ECONNREFUSED' });
    }
    async end() {}
  }
  const config = loadConfig({ DATABASE_URL: 'postgres://unused/db', PORT: '0', HOST: '127.0.0.1' });
  const running = await start(config, { quiet: true, dbOptions: { Pool: DownPool }, connectAttempts: 1, background: false });
  try {
    // The contract says `handle` never rejects. Break it on purpose.
    const handle = running.app.handle;
    let broken = true;
    running.app.handle = async (req, res) => {
      if (broken) throw new Error('outside every try');
      return handle(req, res);
    };
    const get = (path) =>
      new Promise((resolve, reject) => {
        http.get({ host: '127.0.0.1', port: running.port, path }, (res) => {
          res.resume();
          res.on('end', () => resolve(res.statusCode));
        }).on('error', reject);
      });
    assert.equal(await get('/v1/health'), 500);
    broken = false;
    assert.equal(await get('/v1/health'), 200, 'the next request is served');
  } finally {
    await running.close();
  }
});
