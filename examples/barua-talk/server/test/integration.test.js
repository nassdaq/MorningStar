// Integration test: the same scenarios as the unit tests, on the real
// PostgreSQL database (`src/db.js`, every query), plus the real server
// process path (`start`). Upstream is still the fake Barua Talk: nothing
// here can spend money or touch the real workspace.
//
// It runs only when `.env` holds `DATABASE_URL`, and is skipped otherwise.
// Everything it writes lives in a schema of its own, named
// `barua_talk_test_<random>`, which is dropped at the end; the tables the
// real server uses are never read or written.

import assert from 'node:assert/strict';
import { randomBytes } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { after, before, describe, test } from 'node:test';

import { ENV_PATH, loadConfig, parseEnv } from '../src/config.js';
import { createDb } from '../src/db.js';
import { createLogger } from '../src/log.js';
import { start } from '../src/server.js';
import { TEST_KEY, createFakeTalk, createHarness } from './fakes.js';
import { A, B, defineScenarios } from './scenarios.js';

let fileEnv = {};
try {
  fileEnv = parseEnv(readFileSync(ENV_PATH, 'utf8'));
} catch {
  // No .env: the suite is skipped.
}
const databaseUrl = process.env.DATABASE_URL ?? fileEnv.DATABASE_URL ?? '';
const skip = databaseUrl ? false : 'needs DATABASE_URL in .env';

describe('integration: the real database, the fake Barua Talk', { skip, concurrency: false }, () => {
  const schema = `barua_talk_test_${randomBytes(6).toString('hex')}`;
  const env = { DATABASE_URL: databaseUrl, DATABASE_SCHEMA: schema };
  const log = createLogger({ quiet: true });
  let db;

  before(async () => {
    db = createDb(loadConfig(env), log);
    // A name that does not resolve at the first attempt often does at the
    // third: that is this machine's network, not the code.
    for (let attempt = 1; ; attempt += 1) {
      try {
        await db.migrate();
        break;
      } catch (err) {
        if (attempt === 6) throw err;
        await new Promise((resolve) => setTimeout(resolve, 4000));
      }
    }
  }, { timeout: 120000 });

  after(async () => {
    if (!db) return;
    try {
      await db.dropTestSchema();
    } finally {
      await db.close();
    }
  });

  test('the migration is idempotent and only the test schema is touched', async () => {
    await db.migrate();
    await db.migrate();
    // A schema name that is not a plain identifier is not used at all.
    assert.equal(loadConfig({ DATABASE_SCHEMA: 'public; drop' }).databaseSchema, '');
    const plain = createDb(loadConfig({ DATABASE_URL: databaseUrl }), log);
    try {
      await assert.rejects(plain.dropTestSchema(), /barua_talk_test_/);
      await assert.rejects(plain.truncateTestData(), /barua_talk_test_/);
    } finally {
      await plain.close();
    }
  });

  describe('the API on PostgreSQL', { concurrency: false }, () => {
    // A scenario that met "the database could not be reached" (this
    // machine's network drops, and it sleeps) starts again from an empty
    // schema. Anything else fails at once.
    const unreachable = (err) => err?.name === 'DatabaseDown' || /database_unavailable/.test(String(err?.message));
    defineScenarios(
      async (options) => {
        await db.truncateTestData();
        return createHarness({ ...options, db });
      },
      { again: unreachable },
    );
  });

  test('two requests at once: one identity each, one room, one message per client id', async () => {
    await db.truncateTestData();
    const h = await createHarness({ db });
    try {
      // The same name from five phones at the same moment.
      const devices = Array.from({ length: 5 }, (_, i) => `device-race-000${i}`);
      const made = await Promise.all(devices.map((as) => h.api('PUT', '/v1/me', { as, body: { name: 'Juma' } })));
      assert.deepEqual(made.map((r) => r.status), [200, 200, 200, 200, 200]);
      assert.equal(new Set(made.map((r) => r.json.identity)).size, 5);
      assert.ok(made.every((r) => /^juma(-[0-9a-f]+)?$/.test(r.json.identity)));

      const rooms = await Promise.all(devices.map((as) => h.api('POST', '/v1/rooms', { as, body: { display_name: 'Race' } })));
      assert.deepEqual(rooms.map((r) => r.status).sort(), [200, 200, 200, 200, 201]);
      assert.equal((await h.api('GET', '/v1/rooms', { as: devices[0] })).json.rooms.length, 1);

      const posts = await Promise.all(
        [1, 2, 3, 4].map(() => h.api('POST', '/v1/rooms/race/messages', { as: devices[0], body: { text: 'mara moja', client_id: 'm-race-0001' } })),
      );
      assert.deepEqual(posts.map((r) => r.status).sort(), [200, 200, 200, 201]);
      assert.equal(new Set(posts.map((r) => r.json.message.id)).size, 1);
      assert.equal((await h.api('GET', '/v1/rooms/race/messages', { as: devices[1] })).json.messages.length, 1);
    } finally {
      await h.close();
    }
  });

  test('two dials at once with one request id: one call, one upstream request', async () => {
    await db.truncateTestData();
    const h = await createHarness({ db, env: { PHONE_CALLS: 'on' } });
    try {
      await h.api('PUT', '/v1/me', { body: { name: 'Neema' } });
      const dial = (request_id) => h.api('POST', '/v1/phone-calls', { body: { to: '+255712345678', request_id } });
      const same = await Promise.all([dial('dial-race-01'), dial('dial-race-01'), dial('dial-race-01')]);
      assert.equal(new Set(same.map((r) => r.json.call?.id)).size, 1, JSON.stringify(same.map((r) => r.json)));
      assert.equal(same.filter((r) => r.status === 201).length, 1);
      // Different presses at once: still only one call per person.
      const different = await Promise.all([dial('dial-race-02'), dial('dial-race-03')]);
      assert.deepEqual(different.map((r) => r.json.error?.code), ['call_in_progress', 'call_in_progress']);
      assert.equal(h.fake.count('POST', '/calls'), 1);
      assert.equal(h.fake.phoneCalls.length, 1);

      await h.api('POST', '/v1/rooms', { body: { display_name: 'Race' } });
      const starts = await Promise.all([1, 2, 3].map(() => h.api('POST', '/v1/recordings', { body: { room: 'race' } })));
      assert.deepEqual(starts.map((r) => r.status).sort(), [200, 200, 201]);
      assert.equal(h.fake.count('POST', '/recordings'), 1);
    } finally {
      await h.close();
    }
  });

  test('the server as it is started: listens, reports, stores, closes', async () => {
    await db.truncateTestData();
    const fake = await createFakeTalk();
    const config = loadConfig({ ...env, BARUA_API_KEY: TEST_KEY, BARUA_API_BASE: fake.base, HOST: '127.0.0.1', PORT: '0', APP_TOKEN: '' });
    const running = await start(config, { quiet: true, connectAttempts: 4, connectPauseMs: 4000 });
    try {
      const base = `http://127.0.0.1:${running.port}`;
      const call = async (method, path, as, body) => {
        const response = await fetch(`${base}${path}`, {
          method,
          headers: { ...(as ? { 'X-Device-Id': as } : {}), ...(body ? { 'Content-Type': 'application/json' } : {}) },
          body: body ? JSON.stringify(body) : undefined,
        });
        return { status: response.status, json: await response.json() };
      };
      assert.deepEqual((await call('GET', '/v1/health')).json, { ok: true, talk: true, database: true, phone: false });
      assert.deepEqual((await call('PUT', '/v1/me', A, { name: 'Neema' })).json, { identity: 'neema', name: 'Neema' });
      assert.deepEqual((await call('PUT', '/v1/me', B, { name: 'Baraka' })).json, { identity: 'baraka', name: 'Baraka' });
      assert.equal((await call('POST', '/v1/rooms', A, { display_name: 'Desk' })).status, 201);
      assert.equal((await call('POST', '/v1/rooms/desk/messages', A, { text: 'Habari' })).json.message.delivered, true);
      const rung = await call('POST', '/v1/calls', A, { to: 'baraka', video: true });
      assert.equal(rung.status, 201);
      assert.equal((await call('GET', '/v1/calls/incoming', B)).json.calls[0].id, rung.json.call.id);
      assert.equal((await call('POST', `/v1/calls/${rung.json.call.id}/accept`, B)).json.call.status, 'active');
      assert.equal((await call('DELETE', `/v1/calls/${rung.json.call.id}`, A)).json.call.status, 'ended');
      assert.deepEqual((await call('GET', '/v1/activity', A)).json.items.map((i) => [i.kind, i.status]), [['call', 'ended'], ['room', 'open']]);
      // B was called but never opened the room: it is not on B's Home list.
      assert.deepEqual((await call('GET', '/v1/activity', B)).json.items.map((i) => [i.kind, i.status]), [['call', 'ended']]);
    } finally {
      await running.close();
      await fake.close();
    }
  });

  test('the server starts without a Barua key', async () => {
    const config = loadConfig({ ...env, BARUA_API_KEY: '', HOST: '127.0.0.1', PORT: '0' });
    const running = await start(config, { quiet: true, connectAttempts: 4, connectPauseMs: 4000 });
    try {
      const health = await (await fetch(`http://127.0.0.1:${running.port}/v1/health`)).json();
      assert.deepEqual(health, { ok: false, talk: false, database: true, phone: false });
    } finally {
      await running.close();
    }
  });
});
