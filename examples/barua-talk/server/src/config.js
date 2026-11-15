// Configuration: environment only, optionally seeded from a `.env` file.
//
// Nothing here is ever logged. `secrets()` lists the values the logger must
// redact should one ever turn up inside an error message.

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

export const ENV_PATH = fileURLToPath(new URL('../.env', import.meta.url));

/** Parses `KEY=VALUE` lines. Comments and blank lines are skipped. */
export function parseEnv(text) {
  const out = {};
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || line.startsWith('#')) continue;
    const eq = line.indexOf('=');
    if (eq <= 0) continue;
    const key = line.slice(0, eq).replace(/^export\s+/, '').trim();
    let value = line.slice(eq + 1).trim();
    const quote = value[0];
    if ((quote === '"' || quote === "'") && value.endsWith(quote) && value.length >= 2) {
      value = value.slice(1, -1);
    }
    if (/^[A-Za-z_][A-Za-z0-9_]*$/.test(key)) out[key] = value;
  }
  return out;
}

/**
 * Reads `.env` into `env` without overriding variables that are already set.
 * Returns whether a file was found.
 */
export function loadEnvFile(path = ENV_PATH, env = process.env) {
  let text;
  try {
    text = readFileSync(path, 'utf8');
  } catch {
    return false;
  }
  for (const [key, value] of Object.entries(parseEnv(text))) {
    if (env[key] === undefined) env[key] = value;
  }
  return true;
}

function int(value, fallback, min, max) {
  const n = Number.parseInt(value ?? '', 10);
  if (!Number.isFinite(n)) return fallback;
  return Math.min(max, Math.max(min, n));
}

function str(value) {
  return typeof value === 'string' ? value.trim() : '';
}

export function loadConfig(env = process.env) {
  const perMinute = int(env.BARUA_REQUESTS_PER_MIN, 50, 3, 60);
  const callerId = str(env.BARUA_CALLER_ID);
  const schema = str(env.DATABASE_SCHEMA);
  return {
    host: str(env.HOST) || '127.0.0.1',
    port: int(env.PORT, 8789, 0, 65535),
    appToken: str(env.APP_TOKEN),
    databaseUrl: str(env.DATABASE_URL),
    // Optional: a schema to keep the tables in (the tests use a throwaway
    // one). Empty: the connection's default.
    databaseSchema: /^[a-z_][a-z0-9_]{0,62}$/.test(schema) ? schema : '',
    // A device polls several things every few seconds.
    rateLimitPerMinute: int(env.RATE_LIMIT_PER_MIN, 240, 1, 100000),
    // After a failed attempt to reach the database, the next is made no
    // sooner than this.
    databaseRetryMs: int(env.DATABASE_RETRY_MS, 3000, 0, 600000),
    talk: {
      apiKey: str(env.BARUA_API_KEY),
      baseUrl: (str(env.BARUA_API_BASE) || 'https://barua.tz/api/v1/talk').replace(/\/+$/, ''),
      timeoutMs: int(env.BARUA_TIMEOUT_MS, 15000, 100, 120000),
      // Upstream allows 60 requests a minute per key. The server spends at
      // most `perMinute` of them in any 60 seconds, and keeps the last
      // `forwardReserve` for actions (chat forwards stop there) and the
      // last `refreshReserve` for actions and forwards (refreshes stop
      // there). See `talk.js`.
      perMinute,
      forwardReserve: Math.min(perMinute - 1, int(env.BARUA_FORWARD_RESERVE, 10, 0, 59)),
      refreshReserve: Math.min(perMinute - 1, int(env.BARUA_REFRESH_RESERVE, 20, 0, 59)),
      // How old the server's picture of a live thing may be before a poll
      // asks upstream again.
      ringingTtlMs: int(env.CALL_RINGING_TTL_MS, 5000, 0, 600000),
      activeTtlMs: int(env.CALL_ACTIVE_TTL_MS, 15000, 0, 600000),
      phoneTtlMs: int(env.PHONE_TTL_MS, 5000, 0, 600000),
      recordingTtlMs: int(env.RECORDING_TTL_MS, 10000, 0, 600000),
      // A download link is good for an hour; fetch a new one at 50 minutes.
      recordingUrlTtlMs: 50 * 60 * 1000,
      ticketTtlSeconds: 3600,
    },
    calls: {
      // An unanswered in-app call is ended by the server after this.
      ringTimeoutMs: int(env.RING_TIMEOUT_SECONDS, 45, 5, 600) * 1000,
    },
    phone: {
      // Off unless asked for: the app cannot join a phone call's audio yet
      // and upstream documents no way to hang one up, so a dialled call
      // costs money until the other side ends it.
      enabled: str(env.PHONE_CALLS).toLowerCase() === 'on',
      callerId: /^\+[0-9]{8,15}$/.test(callerId) ? callerId : '',
      // `delete`: hang up with `DELETE /calls/{id}`, for an upstream that
      // has it. Anything else: the server cannot end a phone call.
      hangup: str(env.BARUA_PHONE_HANGUP).toLowerCase() === 'delete' ? 'delete' : 'none',
    },
    recordings: {
      maxMs: int(env.RECORDING_MAX_MINUTES, 60, 1, 24 * 60) * 60000,
    },
    // How often the server looks for rings to time out and recordings to stop.
    sweepMs: int(env.SWEEP_SECONDS, 15, 1, 3600) * 1000,
    limits: {
      jsonBytes: 16 * 1024,
      textChars: 2000,
      nameChars: 40,
      roomNameChars: 60,
    },
  };
}

/** Every value that must never appear in a log line. */
export function secrets(config) {
  const list = [config.appToken, config.databaseUrl, config.talk.apiKey];
  try {
    const url = new URL(config.databaseUrl);
    if (url.password) list.push(url.password, decodeURIComponent(url.password));
    if (url.hostname) list.push(url.hostname);
  } catch {
    // Not a URL: the whole string is already in the list.
  }
  return list.filter((s) => s && s.length >= 4);
}
