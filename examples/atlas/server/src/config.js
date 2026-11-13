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

/** How hard `smart` thinks: DeepSeek's `reasoning_effort`, or `off`. */
function effort(value) {
  const name = str(value).toLowerCase();
  return ['off', 'low', 'high', 'max'].includes(name) ? name : 'low';
}

export function loadConfig(env = process.env) {
  return {
    host: str(env.HOST) || '127.0.0.1',
    port: int(env.PORT, 8788, 0, 65535),
    appToken: str(env.APP_TOKEN),
    databaseUrl: str(env.DATABASE_URL),
    rateLimitPerMinute: int(env.RATE_LIMIT_PER_MIN, 60, 1, 100000),
    // After a failed attempt to reach the database, the next is made no
    // sooner than this.
    databaseRetryMs: int(env.DATABASE_RETRY_MS, 3000, 0, 600000),
    deepseek: {
      apiKey: str(env.DEEPSEEK_API_KEY),
      baseUrl: (str(env.DEEPSEEK_BASE_URL) || 'https://api.deepseek.com').replace(/\/+$/, ''),
      // The ids https://api-docs.deepseek.com lists (and `GET /models`
      // returns) in November 2026. See `model.js` for what each mode sends.
      models: {
        fast: str(env.DEEPSEEK_MODEL_FAST) || 'deepseek-flash',
        smart: str(env.DEEPSEEK_MODEL_SMART) || 'deepseek-v4-pro',
        code: str(env.DEEPSEEK_MODEL_CODE) || 'deepseek-v4-pro',
      },
      smartEffort: effort(env.DEEPSEEK_SMART_EFFORT),
      timeoutMs: int(env.DEEPSEEK_TIMEOUT_MS, 25000, 100, 120000),
      // Thinking takes longer.
      smartTimeoutMs: int(env.DEEPSEEK_SMART_TIMEOUT_MS, 40000, 100, 120000),
    },
    elevenlabs: {
      apiKey: str(env.ELEVENLABS_API_KEY),
      baseUrl: (str(env.ELEVENLABS_BASE_URL) || 'https://api.elevenlabs.io').replace(/\/+$/, ''),
      ttsModel: str(env.ELEVENLABS_TTS_MODEL) || 'eleven_flash_v2_5',
      sttModel: str(env.ELEVENLABS_STT_MODEL) || 'scribe_v2',
      outputFormat: str(env.ELEVENLABS_OUTPUT_FORMAT) || 'mp3_44100_64',
      voice: str(env.ELEVENLABS_VOICE),
      ttsTimeoutMs: int(env.ELEVENLABS_TTS_TIMEOUT_MS, 15000, 100, 120000),
      sttTimeoutMs: int(env.ELEVENLABS_STT_TIMEOUT_MS, 15000, 100, 120000),
    },
    limits: {
      jsonBytes: 64 * 1024,
      audioBytes: 5 * 1024 * 1024,
      textChars: 2000,
      attachments: 5,
      attachmentName: 60,
      attachmentKind: 20,
      // What the model is shown of the chat so far.
      historyMessages: 12,
      historyChars: 12000,
      spokenChars: 600,
      audioTtlMs: 10 * 60 * 1000,
      audioStoreBytes: 48 * 1024 * 1024,
      audioClipBytes: 3 * 1024 * 1024,
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
