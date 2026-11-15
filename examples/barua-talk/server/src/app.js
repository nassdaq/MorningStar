// The request pipeline around the routes: token, device id, rate limit,
// error shape and the access log. `createApp` takes its collaborators as
// arguments, so the tests can pass an in-memory database and point the
// Barua Talk client at a local fake.

import { createHash, timingSafeEqual } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

import { DatabaseDown, HttpError, badRequest, databaseUnavailable, fromTalk, rateLimited, unauthorized } from './errors.js';
import { send, sendJson } from './http.js';
import { createRoutes } from './routes.js';
import { createRateLimiter } from './stores.js';

const DEVICE_ID = /^[A-Za-z0-9_-]{8,64}$/;
const CONSOLE_PAGE = fileURLToPath(new URL('../web/console.html', import.meta.url));

function sameToken(given, expected) {
  const a = createHash('sha256').update(given).digest();
  const b = createHash('sha256').update(expected).digest();
  return timingSafeEqual(a, b);
}

export function createApp({ config, db, talk, log, now = Date.now, background = true }) {
  const limiter = createRateLimiter({ limit: config.rateLimitPerMinute, now });
  const routes = createRoutes({ config, db, talk, log, now });

  // The housekeeping that no request can be relied on to cause: rings
  // nobody answered, recordings nobody stopped. See `routes.sweep`.
  let sweeper = null;
  let limiterSweeper = null;
  if (background) {
    sweeper = setInterval(() => {
      routes.sweep();
    }, config.sweepMs);
    sweeper.unref();
    limiterSweeper = setInterval(() => limiter.sweep(), 60000);
    limiterSweeper.unref();
  }

  async function respond(req, res, url) {
    if (config.appToken) {
      const header = String(req.headers.authorization ?? '');
      const given = header.startsWith('Bearer ') ? header.slice(7).trim() : '';
      if (!given || !sameToken(given, config.appToken)) throw unauthorized();
    }
    const { handler, params, open } = routes.match(req.method, url.pathname);
    let deviceId = null;
    if (!open) {
      deviceId = String(req.headers['x-device-id'] ?? '');
      if (!DEVICE_ID.test(deviceId)) {
        throw badRequest('The X-Device-Id header must be 8 to 64 characters of A-Z, a-z, 0-9, _ or -.');
      }
      const wait = limiter.take(deviceId);
      if (wait > 0) throw rateLimited(wait);
    }
    const result = await handler({ req, deviceId, params, query: url.searchParams });
    sendJson(req, res, result.status, result.json);
  }

  /** The `http.Server` request listener. Never rejects. */
  async function handle(req, res) {
    const started = performance.now();
    let url = new URL('http://localhost/');
    try {
      url = new URL(req.url ?? '/', 'http://localhost');
    } catch {
      // Leave it as "/": the route lookup below answers 404.
    }
    const path = url.pathname;
    try {
      if (req.method === 'GET' && (path === '/console' || path === '/console/')) {
        // The browser test client (web/console.html): a page with no secret
        // in it, which then speaks the same contract as the app.
        send(req, res, 200, { 'Content-Type': 'text/html; charset=utf-8' }, await readFile(CONSOLE_PAGE));
      } else {
        await respond(req, res, url);
      }
    } catch (thrown) {
      // A database that cannot be reached is a 503 the app can retry, not
      // a crash; a failure of Barua Talk has its own answers.
      const err = thrown instanceof DatabaseDown ? databaseUnavailable() : fromTalk(thrown);
      if (err instanceof HttpError) {
        sendJson(req, res, err.status, { error: { code: err.code, message: err.message } }, err.headers);
      } else {
        log.error(`unhandled error on ${req.method} ${path}`, err);
        sendJson(req, res, 500, { error: { code: 'internal_error', message: 'Something went wrong on the server.' } });
      }
    }
    log.info(`${req.method} ${path} ${res.statusCode} ${Math.round(performance.now() - started)}ms`);
  }

  return {
    handle,
    routes,
    close: () => {
      clearInterval(sweeper);
      clearInterval(limiterSweeper);
    },
  };
}
