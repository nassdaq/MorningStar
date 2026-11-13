// The request pipeline around the routes: token, device id, rate limit,
// error shape and the access log. `createApp` takes its collaborators as
// arguments, so the tests can pass an in-memory database and point the
// model and voice clients at local fakes.

import { createHash, timingSafeEqual } from 'node:crypto';

import { DatabaseDown, HttpError, badRequest, databaseUnavailable, rateLimited, unauthorized } from './errors.js';
import { send, sendJson } from './http.js';
import { createRoutes } from './routes.js';
import { createAudioStore, createRateLimiter } from './stores.js';

const DEVICE_ID = /^[A-Za-z0-9_-]{8,64}$/;

function sameToken(given, expected) {
  const a = createHash('sha256').update(given).digest();
  const b = createHash('sha256').update(expected).digest();
  return timingSafeEqual(a, b);
}

export function createApp({ config, db, model, voice, log, now = Date.now }) {
  const audio = createAudioStore({ ttlMs: config.limits.audioTtlMs, maxBytes: config.limits.audioStoreBytes, now });
  const limiter = createRateLimiter({ limit: config.rateLimitPerMinute, now });
  const match = createRoutes({ config, db, model, voice, audio, now });

  const sweeper = setInterval(() => {
    audio.sweep();
    limiter.sweep();
  }, 60000);
  sweeper.unref();

  async function respond(req, res, path) {
    if (config.appToken) {
      const header = String(req.headers.authorization ?? '');
      const given = header.startsWith('Bearer ') ? header.slice(7).trim() : '';
      if (!given || !sameToken(given, config.appToken)) throw unauthorized();
    }
    const { handler, params, open } = match(req.method, path);
    let deviceId = null;
    if (!open) {
      deviceId = String(req.headers['x-device-id'] ?? '');
      if (!DEVICE_ID.test(deviceId)) {
        throw badRequest('The X-Device-Id header must be 8 to 64 characters of A-Z, a-z, 0-9, _ or -.');
      }
      const wait = limiter.take(deviceId);
      if (wait > 0) throw rateLimited(wait);
    }
    const result = await handler({ req, deviceId, params });
    if (result.bytes) send(req, res, result.status, { 'Content-Type': result.type }, result.bytes);
    else sendJson(req, res, result.status, result.json);
  }

  /** The `http.Server` request listener. Never rejects. */
  async function handle(req, res) {
    const started = performance.now();
    let path = '/';
    try {
      path = new URL(req.url ?? '/', 'http://localhost').pathname;
    } catch {
      // Leave it as "/": the route lookup below answers 404.
    }
    try {
      await respond(req, res, path);
    } catch (thrown) {
      // A database that cannot be reached is a 503 the app can retry, not a crash.
      const err = thrown instanceof DatabaseDown ? databaseUnavailable() : thrown;
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
    audio,
    close: () => clearInterval(sweeper),
  };
}
