// Everything against fakes: no network beyond the loopback, no database.
// The scenarios of `scenarios.js` run here on the in-memory database; the
// tests below them are the ones that need no database at all or that need
// to break it.

import assert from 'node:assert/strict';
import { describe, test } from 'node:test';

import { loadConfig, parseEnv, secrets } from '../src/config.js';
import { connectionOptions, isConnectionError } from '../src/db.js';
import { HttpError, TalkError, fromTalk } from '../src/errors.js';
import { createLogger } from '../src/log.js';
import { duration, phoneStatus, slug } from '../src/routes.js';
import { createRateLimiter, createSharedWork } from '../src/stores.js';
import { createTalk } from '../src/talk.js';
import { TEST_KEY, createClock, createFakeTalk, createHarness } from './fakes.js';
import { A, B, defineScenarios, introduce } from './scenarios.js';

describe('the API on the in-memory database', () => {
  defineScenarios((options) => createHarness(options));
});

async function withHarness(options, body) {
  const h = await createHarness(options);
  try {
    await body(h);
  } finally {
    await h.close();
  }
}

describe('the request pipeline', () => {
  test('health needs no device id; everything else does', () =>
    withHarness({}, async ({ api }) => {
      const health = await api('GET', '/v1/health', { as: null });
      assert.deepEqual([health.status, health.json], [200, { ok: true, talk: true, database: true, phone: false }]);
      assert.equal(health.headers.get('cache-control'), 'no-store');
      for (const as of [null, 'short', 'has spaces in it', 'x'.repeat(65)]) {
        const response = await api('GET', '/v1/me', { as });
        assert.deepEqual([response.status, response.json.error.code], [400, 'bad_request']);
      }
    }));

  test('unknown paths and methods are 404 in the error shape', () =>
    withHarness({}, async ({ api }) => {
      for (const [method, path] of [['GET', '/'], ['GET', '/v1/nothing'], ['POST', '/v1/me'], ['GET', '/v1/rooms/Bad_Name'], ['PATCH', '/v1/rooms']]) {
        const response = await api(method, path);
        assert.deepEqual([response.status, response.json], [404, { error: { code: 'not_found', message: 'Not found.' } }], `${method} ${path}`);
      }
    }));

  test('bodies: not JSON, not an object, too large', () =>
    withHarness({}, async ({ api }) => {
      for (const body of ['{not json', '[1, 2]', '"text"', 'null']) {
        const response = await api('PUT', '/v1/me', { body });
        assert.deepEqual([response.status, response.json.error.code], [400, 'bad_request'], body);
      }
      const large = await api('PUT', '/v1/me', { body: { name: 'x'.repeat(17 * 1024) } });
      assert.deepEqual([large.status, large.json.error.code], [413, 'too_large']);
    }));

  test('APP_TOKEN: required on every request when set', () =>
    withHarness({ env: { APP_TOKEN: 'a-long-app-token' } }, async ({ api }) => {
      assert.equal((await api('GET', '/v1/health', { as: null })).status, 401);
      assert.deepEqual((await api('GET', '/v1/me')).json, { error: { code: 'unauthorized', message: 'This app is not allowed to use the server.' } });
      assert.equal((await api('GET', '/v1/me', { headers: { Authorization: 'Bearer wrong-token-here' } })).status, 401);
      assert.equal((await api('GET', '/v1/me', { headers: { Authorization: 'Bearer a-long-app-token' } })).status, 200);
    }));

  test('a device is limited per minute, with Retry-After; health is not counted', () =>
    withHarness({ env: { RATE_LIMIT_PER_MIN: '3' } }, async ({ api, now }) => {
      for (let i = 0; i < 3; i += 1) assert.equal((await api('GET', '/v1/me')).status, 200);
      const refused = await api('GET', '/v1/me');
      assert.deepEqual([refused.status, refused.json.error.code, refused.headers.get('retry-after')], [429, 'rate_limited', '60']);
      assert.equal((await api('GET', '/v1/me', { as: B })).status, 200);
      assert.equal((await api('GET', '/v1/health', { as: null })).status, 200);
      now.advance(60001);
      assert.equal((await api('GET', '/v1/me')).status, 200);
    }));

  test('a database that is not there: 503 that can be retried, and it comes back', () =>
    withHarness({}, async ({ api, db, now }) => {
      await introduce(api);
      db.setDown(true);
      now.advance(3000);
      assert.deepEqual((await api('GET', '/v1/health', { as: null })).json, { ok: false, talk: true, database: false, phone: false });
      for (const [method, path, body] of [['GET', '/v1/me'], ['GET', '/v1/rooms'], ['GET', '/v1/calls/incoming'], ['GET', '/v1/activity'], ['POST', '/v1/rooms', { display_name: 'Desk' }]]) {
        const response = await api(method, path, { body });
        assert.deepEqual([response.status, response.json.error.code, response.headers.get('retry-after')], [503, 'database_unavailable', '5'], path);
      }
      db.setDown(false);
      now.advance(3000);
      assert.equal((await api('GET', '/v1/health', { as: null })).json.database, true);
      assert.equal((await api('GET', '/v1/me')).json.identity, 'neema');
    }));

  test('the housekeeping survives a database that is down and an upstream that is down', () =>
    withHarness({}, async ({ api, db, app, fake, now, lines }) => {
      const ids = await introduce(api);
      const { call } = (await api('POST', '/v1/calls', { body: { to: ids.b } })).json;
      db.setDown(true);
      await app.routes.sweep();
      db.setDown(false);
      now.advance(61000);
      fake.fail('DELETE', `/app-calls/${call.id}`, { status: 500, body: '' });
      await app.routes.sweep();
      assert.equal((await api('GET', `/v1/calls/${call.id}`)).json.call.status, 'missed');
      assert.ok(!lines.some((line) => line.includes('housekeeping failed')), lines.join('\n'));
    }));

  test('a row left by a request that never finished stops blocking after two minutes', () =>
    withHarness({ env: { PHONE_CALLS: 'on' } }, async ({ api, db, app, now, fake }) => {
      const ids = await introduce(api);
      await db.reservePhoneCall({ id: 'p_stuck', identity: ids.a, requestId: 'dial-stuck', to: '+255712345678', at: new Date(now()) });
      const dial = () => api('POST', '/v1/phone-calls', { body: { to: '+255712345678', request_id: 'dial-0002' } });
      assert.equal((await dial()).json.error.code, 'call_in_progress');
      now.advance(121000);
      await app.routes.sweep();
      assert.equal((await dial()).status, 201);
      assert.equal(fake.count('POST', '/calls'), 1);
      assert.deepEqual((await api('GET', '/v1/phone-calls')).json.calls.map((c) => c.status), ['ringing', 'failed']);
    }));

  test('nothing secret reaches the log: no key, no ticket, no body, no device id', () =>
    withHarness({ env: { PHONE_CALLS: 'on' } }, async ({ api, lines }) => {
      const ids = await introduce(api);
      await api('POST', '/v1/rooms', { body: { display_name: 'Siri kubwa' } });
      await api('POST', '/v1/rooms/siri-kubwa/ticket', { body: { video: true } });
      await api('POST', '/v1/rooms/siri-kubwa/messages', { body: { text: 'neno la siri' } });
      await api('POST', '/v1/calls', { body: { to: ids.b } });
      await api('POST', '/v1/phone-calls', { body: { to: '+255712345678', request_id: 'dial-0001' } });
      const log = lines.join('\n');
      assert.ok(lines.length >= 12);
      for (const secret of [TEST_KEY, 'ticket.', 'neno la siri', A, 'Bearer', '+255712345678']) {
        assert.ok(!log.includes(secret), `the log contains ${secret}`);
      }
      assert.match(log, /info talk POST \/rooms 201 \d+ms/);
      assert.match(log, /info POST \/v1\/rooms 201 \d+ms/);
    }));
});

describe('upstream errors and the allowance', () => {
  test('a refused key is the server’s problem, said so', () =>
    withHarness({ env: { BARUA_API_KEY: 'barua_wrong_key_000000' } }, async ({ api }) => {
      const response = await api('POST', '/v1/rooms', { body: { display_name: 'Desk' } });
      assert.deepEqual([response.status, response.json.error.code], [502, 'talk_rejected']);
      assert.deepEqual((await api('GET', '/v1/rooms')).json.rooms, []);
    }));

  test('every upstream error has an answer the app can word', () => {
    const answer = (kind, fields) => {
      const err = fromTalk(new TalkError(kind, fields));
      assert.ok(err instanceof HttpError && err.message.length > 0);
      return [err.status, err.code];
    };
    assert.deepEqual(answer('disabled'), [503, 'talk_unavailable']);
    assert.deepEqual(answer('busy', { retryAfter: 7 }), [429, 'talk_busy']);
    assert.deepEqual(answer('network'), [502, 'upstream_failed']);
    assert.deepEqual(answer('http', { status: 401, code: 'invalid_key' }), [502, 'talk_rejected']);
    assert.deepEqual(answer('http', { status: 402, code: 'balance_empty' }), [402, 'balance_empty']);
    assert.deepEqual(answer('http', { status: 403, code: 'insufficient_scope' }), [502, 'talk_rejected']);
    assert.deepEqual(answer('http', { status: 403, code: 'not_your_call' }), [403, 'not_your_call']);
    assert.deepEqual(answer('http', { status: 404, code: 'room_not_found', message: 'No room called desk-4.' }), [404, 'not_found']);
    assert.deepEqual(answer('http', { status: 409, code: 'not_ringing' }), [409, 'not_ringing']);
    assert.deepEqual(answer('http', { status: 409, code: 'no_caller_number' }), [409, 'no_caller_number']);
    assert.deepEqual(answer('http', { status: 409, code: 'something_else' }), [409, 'conflict']);
    assert.deepEqual(answer('http', { status: 422, code: 'invalid_request', message: 'to must be E.164' }), [400, 'bad_request']);
    assert.deepEqual(answer('http', { status: 429, code: 'rate_limited', retryAfter: 30 }), [429, 'talk_busy']);
    assert.deepEqual(answer('http', { status: 503, code: 'not_connected' }), [503, 'not_connected']);
    assert.deepEqual(answer('http', { status: 500, code: 'boom' }), [502, 'upstream_failed']);
    assert.equal(fromTalk(new TalkError('http', { status: 429, retryAfter: 30 })).headers['Retry-After'], '30');
    assert.equal(fromTalk(new TalkError('http', { status: 404, message: 'No room called desk-4.' })).message, 'No room called desk-4.');
    const other = new Error('not a talk error');
    assert.equal(fromTalk(other), other);
  });

  test('after upstream says 429 nothing is sent until the minute turns; polls go on', () =>
    withHarness({}, async ({ api, fake, now }) => {
      const ids = await introduce(api);
      const { call } = (await api('POST', '/v1/calls', { body: { to: ids.b } })).json;
      fake.limit = 0;
      const refused = await api('POST', '/v1/rooms', { body: { display_name: 'Desk' } });
      assert.deepEqual([refused.status, refused.json.error.code], [429, 'talk_busy']);
      const wait = Number(refused.headers.get('retry-after'));
      assert.ok(wait >= 1 && wait <= 60, `Retry-After ${wait}`);
      fake.limit = 60;
      fake.reset();
      // Still inside the minute: refused here, without asking.
      assert.equal((await api('POST', '/v1/rooms', { body: { display_name: 'Desk' } })).json.error.code, 'talk_busy');
      now.advance(6000);
      assert.equal((await api('GET', '/v1/calls/incoming', { as: B })).json.calls.length, 1);
      assert.equal((await api('GET', `/v1/calls/${call.id}`)).json.call.status, 'ringing');
      assert.equal(fake.requests.length, 0);
      now.advance(60000);
      assert.equal((await api('POST', '/v1/rooms', { body: { display_name: 'Desk' } })).status, 201);
    }));

  test('the allowance is spent on actions last: refreshes stop first, then chat forwards', () =>
    withHarness({ env: { BARUA_REQUESTS_PER_MIN: '6', BARUA_REFRESH_RESERVE: '4', BARUA_FORWARD_RESERVE: '2' } }, async ({ api, fake, now, talk }) => {
      const ids = await introduce(api);
      await api('POST', '/v1/rooms', { body: { display_name: 'Desk' } }); // 1
      const { call } = (await api('POST', '/v1/calls', { body: { to: ids.b } })).json; // 2
      now.advance(6000);
      // Two requests used: no room for a refresh (6 - 4). The poll still answers.
      assert.equal((await api('GET', `/v1/calls/${call.id}`)).json.call.status, 'ringing');
      assert.equal(fake.count('GET', /^\/app-calls\//), 0);
      // Forwards go on until four are used (6 - 2)…
      assert.equal((await api('POST', '/v1/rooms/desk/messages', { body: { text: 'moja' } })).json.message.delivered, true); // 3
      assert.equal((await api('POST', '/v1/rooms/desk/messages', { body: { text: 'mbili' } })).json.message.delivered, true); // 4
      const third = await api('POST', '/v1/rooms/desk/messages', { body: { text: 'tatu' } });
      assert.deepEqual([third.status, third.json.message.delivered], [201, false]);
      assert.equal(fake.count('POST', '/rooms/desk/chat'), 2);
      // …and actions until all six are.
      assert.equal((await api('POST', '/v1/rooms/desk/ticket', { body: {} })).status, 200); // 5
      assert.equal((await api('POST', `/v1/calls/${call.id}/accept`, { as: B })).status, 200); // 6
      const refused = await api('POST', '/v1/rooms/desk/ticket', { body: {} });
      assert.deepEqual([refused.status, refused.json.error.code], [429, 'talk_busy']);
      assert.ok(Number(refused.headers.get('retry-after')) >= 1);
      assert.equal(talk.used(), 6);
      assert.equal(fake.requests.length, 6);
      // A hang-up is never refused to the person: the call is over here regardless.
      assert.equal((await api('DELETE', `/v1/calls/${call.id}`)).json.call.status, 'ended');
      assert.equal(fake.requests.length, 6);
      now.advance(61000);
      assert.equal((await api('POST', '/v1/rooms/desk/ticket', { body: {} })).status, 200);
    }));
});

describe('the Barua Talk client', () => {
  const quiet = createLogger({ quiet: true });

  test('one try, the key in the header, JSON both ways; errors by kind', async () => {
    const fake = await createFakeTalk();
    try {
      const config = loadConfig({ BARUA_API_KEY: TEST_KEY, BARUA_API_BASE: `${fake.base}/`, BARUA_TIMEOUT_MS: '500' });
      assert.equal(config.talk.baseUrl, fake.base);
      const talk = createTalk(config, quiet);
      assert.equal(talk.enabled, true);
      assert.deepEqual(await talk.request('GET', '/rooms'), { rooms: [] });
      const made = await talk.request('POST', '/rooms', { body: { name: 'desk', displayName: 'Desk', identity: 'neema' } });
      assert.deepEqual([made.room.name, typeof made.token, made.url], ['desk', 'string', 'wss://fake.livekit.invalid']);

      const kinds = async (promise) => {
        const err = await promise.then(() => assert.fail('should have thrown'), (e) => e);
        assert.ok(err instanceof TalkError);
        return [err.kind, err.status, err.code];
      };
      assert.deepEqual(await kinds(talk.request('POST', '/rooms/nowhere/tokens', { body: { identity: 'x' } })), ['http', 404, 'room_not_found']);
      fake.fail('GET', '/rooms', { status: 500, body: '' });
      assert.deepEqual(await kinds(talk.request('GET', '/rooms')), ['network', 500, '']);
      fake.fail('GET', '/rooms', { status: 502, body: '<html>Bad gateway</html>' });
      assert.deepEqual(await kinds(talk.request('GET', '/rooms')), ['network', 502, '']);
      fake.fail('GET', '/rooms', { status: 200, body: 'not json' });
      assert.deepEqual(await kinds(talk.request('GET', '/rooms')), ['network', 0, '']);
      fake.fail('GET', '/rooms', 'drop-before');
      assert.deepEqual(await kinds(talk.request('GET', '/rooms')), ['network', 0, '']);
      // Each of those was exactly one request.
      assert.equal(fake.count('GET', '/rooms'), 5);

      const none = createTalk(loadConfig({ BARUA_API_BASE: fake.base }), quiet);
      assert.equal(none.enabled, false);
      assert.deepEqual(await kinds(none.request('GET', '/rooms')), ['disabled', 0, '']);
    } finally {
      await fake.close();
    }
  });

  test('a request that takes too long is given up, once', async () => {
    const fake = await createFakeTalk();
    const slow = (await import('node:http')).createServer(() => {});
    await new Promise((resolve) => slow.listen(0, '127.0.0.1', resolve));
    try {
      const talk = createTalk(loadConfig({ BARUA_API_KEY: TEST_KEY, BARUA_API_BASE: `http://127.0.0.1:${slow.address().port}`, BARUA_TIMEOUT_MS: '150' }), quiet);
      const started = Date.now();
      const err = await talk.request('GET', '/rooms').catch((e) => e);
      assert.deepEqual([err.kind, Date.now() - started < 2000], ['network', true]);
      assert.equal(talk.used(), 1);
    } finally {
      slow.closeAllConnections();
      slow.close();
      await fake.close();
    }
  });

  test('the allowance: a sliding minute, three priorities, and how long to wait', async () => {
    const now = createClock();
    let sent = 0;
    const fetchImpl = async () => {
      sent += 1;
      return new Response('{"ok":true}', { status: 200 });
    };
    const config = loadConfig({ BARUA_API_KEY: TEST_KEY, BARUA_REQUESTS_PER_MIN: '10', BARUA_REFRESH_RESERVE: '6', BARUA_FORWARD_RESERVE: '3' });
    const talk = createTalk(config, quiet, { fetchImpl, now });
    const busy = async (priority) => {
      const err = await talk.request('GET', '/rooms', { priority }).then(() => null, (e) => e);
      return err ? [err.kind, err.retryAfter] : null;
    };
    for (let i = 0; i < 4; i += 1) {
      assert.equal(await busy('refresh'), null);
      now.advance(1000);
    }
    assert.deepEqual(await busy('refresh'), ['busy', 56]); // the first leaves the window in 56 s
    for (let i = 0; i < 3; i += 1) assert.equal(await busy('forward'), null);
    assert.deepEqual(await busy('forward'), ['busy', 56]);
    for (let i = 0; i < 3; i += 1) assert.equal(await busy('action'), null);
    assert.deepEqual(await busy('action'), ['busy', 56]);
    assert.deepEqual([sent, talk.used(), talk.wait('action')], [10, 10, 56]);
    now.advance(56000);
    assert.deepEqual([talk.used(), talk.wait('action'), talk.wait('refresh')], [9, 0, 4]);
    assert.equal(await busy('action'), null);
    assert.equal(sent, 11);
  });

  test('upstream’s own 429 blocks until the minute turns, or for Retry-After', async () => {
    const now = createClock(Date.parse('2026-11-15T07:00:40Z'));
    let reply = () => new Response('{"error":{"code":"rate_limited","message":"Over 60."}}', { status: 429 });
    const talk = createTalk(loadConfig({ BARUA_API_KEY: TEST_KEY }), quiet, { fetchImpl: async () => reply(), now });
    const first = await talk.request('GET', '/rooms').catch((e) => e);
    assert.deepEqual([first.kind, first.status, first.retryAfter], ['http', 429, 20]);
    assert.equal(talk.wait('action'), 20);
    assert.deepEqual((await talk.request('GET', '/rooms').catch((e) => e)).kind, 'busy');
    now.advance(20000);
    reply = () => new Response('{"error":{"code":"rate_limited"}}', { status: 429, headers: { 'Retry-After': '7' } });
    assert.equal((await talk.request('GET', '/rooms').catch((e) => e)).retryAfter, 7);
    assert.equal(talk.wait('refresh'), 7);
    now.advance(7000);
    reply = () => new Response('{"rooms":[]}', { status: 200 });
    assert.deepEqual(await talk.request('GET', '/rooms'), { rooms: [] });
  });
});

describe('small things', () => {
  test('slug: an address from a name', () => {
    assert.equal(slug('Neema Mushi', 36), 'neema-mushi');
    assert.equal(slug('  Zoë  Müller-Ñandú ', 36), 'zoe-muller-nandu');
    assert.equal(slug('Support desk #4!', 63), 'support-desk-4');
    assert.equal(slug('★★★', 36), '');
    assert.equal(slug('a'.repeat(30) + ' ' + 'b'.repeat(30), 36), `${'a'.repeat(30)}-bbbbb`);
    assert.equal(slug('abcde fghij', 6), 'abcde');
  });

  test('duration and phone statuses', () => {
    assert.deepEqual([0, 7, 59, 60, 125, 3600].map(duration), ['0 s', '7 s', '59 s', '1 min', '2 min 5 s', '60 min']);
    assert.deepEqual(
      ['ringing', 'Queued', 'in-progress', 'answered', 'completed', 'no_answer', 'busy', 'CANCELED', 'failed'].map((s) => phoneStatus(s, null)),
      ['ringing', 'ringing', 'active', 'active', 'ended', 'missed', 'missed', 'missed', 'failed'],
    );
    assert.equal(phoneStatus('something-new', null), null);
    assert.equal(phoneStatus('something-new', new Date()), 'ended');
    assert.equal(phoneStatus(undefined, null), null);
  });

  test('configuration: defaults, bounds, and what must never be logged', () => {
    const config = loadConfig({});
    assert.deepEqual(
      [config.host, config.port, config.rateLimitPerMinute, config.talk.baseUrl, config.talk.perMinute, config.phone.enabled, config.phone.hangup, config.databaseSchema],
      ['127.0.0.1', 8789, 240, 'https://barua.tz/api/v1/talk', 50, false, 'none', ''],
    );
    assert.equal(config.talk.apiKey, '');
    const set = loadConfig({
      PORT: '99999', BARUA_REQUESTS_PER_MIN: '500', PHONE_CALLS: ' ON ', BARUA_CALLER_ID: '0800', DATABASE_SCHEMA: 'Bad-Schema"; drop',
      BARUA_API_KEY: ' barua_secret_key ', APP_TOKEN: 'app-token-1', DATABASE_URL: 'postgres://user:p%40ss-word@db.example.com/main',
    });
    assert.deepEqual([set.port, set.talk.perMinute, set.phone.enabled, set.phone.callerId, set.databaseSchema], [65535, 60, true, '', '']);
    assert.ok(set.talk.forwardReserve < set.talk.perMinute && set.talk.refreshReserve < set.talk.perMinute);
    assert.deepEqual(secrets(set).sort(), ['app-token-1', 'barua_secret_key', 'db.example.com', 'p%40ss-word', 'p@ss-word', 'postgres://user:p%40ss-word@db.example.com/main'].sort());
    const lines = [];
    const log = createLogger({ secrets: secrets(set), sink: (line) => lines.push(line) });
    log.error('could not connect', new Error('getaddrinfo ENOTFOUND db.example.com with barua_secret_key at postgres://user:p%40ss-word@db.example.com/main'));
    assert.ok(!/db\.example\.com|barua_secret_key|p%40ss|p@ss/.test(lines[0]), lines[0]);
    assert.deepEqual(parseEnv('# c\nA=1\nexport B="two words"\n C = \'x\' \nbad line\n=nokey\n'), { A: '1', B: 'two words', C: 'x' });
  });

  test('database helpers: which errors mean "not reachable", and TLS options', () => {
    assert.ok(isConnectionError({ code: 'ENOTFOUND' }) && isConnectionError({ code: '08006' }) && isConnectionError({ code: '57P01' }));
    assert.ok(isConnectionError({ message: 'Connection terminated unexpectedly' }));
    assert.ok(isConnectionError({ message: 'Query read timeout' }));
    assert.ok(!isConnectionError({ code: '23505' }) && !isConnectionError({ code: '42P01' }) && !isConnectionError(null));
    assert.deepEqual(connectionOptions('postgres://u:p@h/d?sslmode=require'), { connectionString: 'postgres://u:p@h/d', ssl: { rejectUnauthorized: true } });
    assert.deepEqual(connectionOptions('postgres://u:p@h/d'), { connectionString: 'postgres://u:p@h/d' });
    assert.deepEqual(connectionOptions('host=h dbname=d'), { connectionString: 'host=h dbname=d' });
  });

  test('rate limiter and shared work', async () => {
    const now = createClock();
    const limiter = createRateLimiter({ limit: 2, now });
    assert.deepEqual([limiter.take('a'), limiter.take('a'), limiter.take('a'), limiter.take('b')], [0, 0, 60, 0]);
    now.advance(59500);
    assert.equal(limiter.take('a'), 1);
    now.advance(500);
    assert.equal(limiter.take('a'), 0);

    const shared = createSharedWork();
    let runs = 0;
    const work = async () => {
      runs += 1;
      await new Promise((resolve) => setTimeout(resolve, 10));
      return runs;
    };
    assert.deepEqual(await Promise.all([shared('k', work), shared('k', work), shared('other', work)]), [2, 2, 2]);
    assert.equal(await shared('k', work), 3);
    await assert.rejects(shared('k', async () => { throw new Error('boom'); }), /boom/);
    assert.equal(await shared('k', async () => 'again'), 'again');
  });
});
