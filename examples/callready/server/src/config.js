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

function timeZone(value) {
  const tz = str(value) || 'UTC';
  try {
    new Intl.DateTimeFormat('en-CA', { timeZone: tz });
    return tz;
  } catch {
    return 'UTC';
  }
}

export function loadConfig(env = process.env) {
  return {
    host: str(env.HOST) || '127.0.0.1',
    port: int(env.PORT, 8787, 0, 65535),
    appToken: str(env.APP_TOKEN),
    databaseUrl: str(env.DATABASE_URL),
    // The day boundary used for the practice streak.
    streakTimeZone: timeZone(env.STREAK_TZ),
    rateLimitPerMinute: int(env.RATE_LIMIT_PER_MIN, 60, 1, 100000),
    deepseek: {
      apiKey: str(env.DEEPSEEK_API_KEY),
      baseUrl: (str(env.DEEPSEEK_BASE_URL) || 'https://api.deepseek.com').replace(/\/+$/, ''),
      model: str(env.DEEPSEEK_MODEL) || 'deepseek-chat',
      timeoutMs: int(env.DEEPSEEK_TIMEOUT_MS, 15000, 100, 120000),
    },
    elevenlabs: {
      apiKey: str(env.ELEVENLABS_API_KEY),
      baseUrl: (str(env.ELEVENLABS_BASE_URL) || 'https://api.elevenlabs.io').replace(/\/+$/, ''),
      ttsModel: str(env.ELEVENLABS_TTS_MODEL) || 'eleven_flash_v2_5',
      sttModel: str(env.ELEVENLABS_STT_MODEL) || 'scribe_v2',
      outputFormat: str(env.ELEVENLABS_OUTPUT_FORMAT) || 'mp3_44100_64',
      voices: {
        us: str(env.ELEVENLABS_VOICE_US),
        uk: str(env.ELEVENLABS_VOICE_UK),
        australian: str(env.ELEVENLABS_VOICE_AUSTRALIAN),
        indian: str(env.ELEVENLABS_VOICE_INDIAN),
      },
      ttsTimeoutMs: int(env.ELEVENLABS_TTS_TIMEOUT_MS, 10000, 100, 120000),
      sttTimeoutMs: int(env.ELEVENLABS_STT_TIMEOUT_MS, 15000, 100, 120000),
    },
    limits: {
      jsonBytes: 64 * 1024,
      audioBytes: 5 * 1024 * 1024,
      learnerChars: 600,
      audioTtlMs: 10 * 60 * 1000,
      audioStoreBytes: 48 * 1024 * 1024,
      audioClipBytes: 2 * 1024 * 1024,
    },
  };
}

/** Every value that must never appear in a log line. */
export function secrets(config) {
  const list = [config.appToken, config.databaseUrl, config.deepseek.apiKey, config.elevenlabs.apiKey];
  try {
    const url = new URL(config.databaseUrl);
    if (url.password) list.push(url.password, decodeURIComponent(url.password));
    if (url.hostname) list.push(url.hostname);
  } catch {
    // Not a URL: the whole string is already in the list.
  }
  return list.filter((s) => s && s.length >= 4);
}
