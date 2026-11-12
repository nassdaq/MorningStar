// ElevenLabs: speech-to-text for the learner's audio, text-to-speech for the
// caller's lines. Active only when `ELEVENLABS_API_KEY` is set.
//
// Endpoints, as documented at https://elevenlabs.io/docs/api-reference:
//   GET  /v2/voices?page_size=100[&next_page_token=...]      list voices
//   POST /v1/text-to-speech/{voice_id}?output_format=...     JSON in, audio out
//   POST /v1/speech-to-text                                  multipart in, JSON out
// all authenticated with the `xi-api-key` header.
//
// Speech is best-effort: `speak` returns null instead of failing, because a
// call must go on in text when a line cannot be voiced. Transcription is
// not: without the learner's words there is no turn, so it throws.

import { UpstreamError } from './errors.js';

// How a voice's accent label (or an English locale) maps to the app's accents.
const ACCENT_PATTERNS = {
  us: /\bamerican\b|\ben[-_]us\b|\bus\b/,
  uk: /\bbritish\b|\ben[-_]gb\b|\buk\b|\bengland\b/,
  australian: /\baustralian?\b|\ben[-_]au\b/,
  indian: /\bindian?\b|\ben[-_]in\b/,
};

const EXTENSIONS = {
  'audio/mp4': 'm4a',
  'audio/m4a': 'm4a',
  'audio/x-m4a': 'm4a',
  'audio/aac': 'aac',
  'audio/mpeg': 'mp3',
  'audio/mp3': 'mp3',
  'audio/wav': 'wav',
  'audio/x-wav': 'wav',
  'audio/wave': 'wav',
  'audio/webm': 'webm',
  'audio/ogg': 'ogg',
  'audio/3gpp': '3gp',
};

/** The media types a turn may be recorded in. */
export const AUDIO_TYPES = Object.keys(EXTENSIONS);

/** Everything a voice says about its accent, lower-cased, in one string. */
function accentText(voice) {
  const parts = [voice?.labels?.accent];
  for (const language of voice?.verified_languages ?? []) {
    if (typeof language?.language === 'string' && /^en/i.test(language.language)) {
      parts.push(language.accent, language.locale);
    }
  }
  return parts.filter((p) => typeof p === 'string').join(' ').toLowerCase();
}

/**
 * Chooses voices for the four accents from the account's voice list.
 * Returns `{accent: {male, female, any}}` plus `fallback`, all voice ids
 * (or undefined). Exported for the tests.
 */
export function chooseVoices(voices) {
  const usable = voices.filter((v) => v && typeof v.voice_id === 'string' && v.voice_id !== '');
  const chosen = { fallback: usable[0]?.voice_id };
  for (const [accent, pattern] of Object.entries(ACCENT_PATTERNS)) {
    const matches = usable.filter((v) => pattern.test(accentText(v)));
    const gender = (g) => matches.find((v) => String(v.labels?.gender ?? '').toLowerCase() === g)?.voice_id;
    chosen[accent] = { male: gender('male'), female: gender('female'), any: matches[0]?.voice_id };
  }
  return chosen;
}

export function createVoice(config, log, { fetchImpl = fetch, now = Date.now } = {}) {
  const el = config.elevenlabs;
  const enabled = el.apiKey !== '';
  let chosen = null; // set once the voice list has been read
  let loading = null;
  let lastAttempt = -Infinity;

  async function request(label, path, init, timeoutMs) {
    const started = performance.now();
    try {
      const response = await fetchImpl(`${el.baseUrl}${path}`, {
        ...init,
        headers: { 'xi-api-key': el.apiKey, ...init.headers },
        signal: AbortSignal.timeout(timeoutMs),
      });
      const line = `upstream elevenlabs ${label} ${response.status} ${Math.round(performance.now() - started)}ms`;
      if (response.ok) log.info(line);
      else log.warn(line);
      return response;
    } catch (err) {
      const why = err?.name === 'TimeoutError' ? 'timeout' : `network (${err?.name ?? 'error'})`;
      log.warn(`upstream elevenlabs ${label} ${why} ${Math.round(performance.now() - started)}ms`);
      throw new UpstreamError('elevenlabs', why);
    }
  }

  async function listVoices() {
    const voices = [];
    let token = '';
    for (let page = 0; page < 5; page += 1) {
      const query = new URLSearchParams({ page_size: '100', include_total_count: 'false' });
      if (token) query.set('next_page_token', token);
      const response = await request('voices', `/v2/voices?${query}`, { method: 'GET', headers: {} }, 10000);
      if (!response.ok) {
        await response.arrayBuffer().catch(() => {});
        throw new UpstreamError('elevenlabs', `voices status ${response.status}`);
      }
      const data = await response.json();
      if (Array.isArray(data?.voices)) voices.push(...data.voices);
      if (!data?.has_more || !data?.next_page_token) break;
      token = data.next_page_token;
    }
    return voices;
  }

  /**
   * Reads the voice list once and caches the choice. A failed attempt is
   * retried on a later line, at most once a minute.
   */
  async function ensureVoices() {
    if (chosen || !enabled) return chosen;
    if (loading) return loading;
    if (now() - lastAttempt < 60000) return null;
    lastAttempt = now();
    loading = (async () => {
      try {
        const voices = await listVoices();
        chosen = chooseVoices(voices);
        const matched = Object.keys(ACCENT_PATTERNS).filter((a) => el.voices[a] || chosen[a].any);
        log.info(`voice: ${voices.length} voices listed, accents matched: ${matched.join(', ') || 'none'}`);
      } catch (err) {
        log.warn(`voice: could not list voices (${err.message}); overrides only until the next attempt`);
      } finally {
        loading = null;
      }
      return chosen;
    })();
    return loading;
  }

  /** The voice id for an accent and, when there is a choice, a gender. */
  function pick(accent, gender) {
    if (el.voices[accent]) return el.voices[accent];
    const byAccent = chosen?.[accent];
    return byAccent?.[gender] ?? byAccent?.any ?? chosen?.fallback ?? null;
  }

  return {
    enabled,

    /** Called once at start-up; never throws. */
    async init() {
      if (enabled) await ensureVoices();
    },

    voiceFor(accent, gender) {
      return pick(accent, gender);
    },

    /** The words in a recording. Throws `UpstreamError` on any failure. */
    async transcribe(bytes, contentType) {
      if (!enabled) throw new UpstreamError('elevenlabs', 'no voice key configured');
      let failure = 'no attempt made';
      for (let attempt = 1; attempt <= 2; attempt += 1) {
        // A FormData body is consumed by a request, so build it per attempt.
        const form = new FormData();
        form.set('model_id', el.sttModel);
        form.set('language_code', 'en');
        form.set('tag_audio_events', 'false');
        form.set('file', new Blob([bytes], { type: contentType }), `turn.${EXTENSIONS[contentType] ?? 'bin'}`);
        try {
          const response = await request('speech-to-text', '/v1/speech-to-text', { method: 'POST', headers: {}, body: form }, el.sttTimeoutMs);
          if (response.ok) {
            const data = await response.json();
            if (typeof data?.text === 'string') return data.text;
            failure = 'unusable answer';
          } else {
            await response.arrayBuffer().catch(() => {});
            failure = `status ${response.status}`;
            // The request itself is wrong (bad audio, bad key): do not repeat it.
            if (response.status < 500 && response.status !== 429 && response.status !== 408) break;
          }
        } catch (err) {
          failure = err.message;
        }
      }
      throw new UpstreamError('elevenlabs', failure);
    },

    /** The line as mp3 bytes, or null when it cannot be voiced. */
    async speak(text, accent, gender) {
      if (!enabled) return null;
      try {
        await ensureVoices();
        const voiceId = pick(accent, gender);
        if (!voiceId) return null;
        const response = await request(
          'text-to-speech',
          `/v1/text-to-speech/${encodeURIComponent(voiceId)}?output_format=${encodeURIComponent(el.outputFormat)}`,
          {
            method: 'POST',
            headers: { 'Content-Type': 'application/json', Accept: 'audio/mpeg' },
            body: JSON.stringify({ text, model_id: el.ttsModel }),
          },
          el.ttsTimeoutMs,
        );
        if (!response.ok) {
          await response.arrayBuffer().catch(() => {});
          return null;
        }
        const bytes = Buffer.from(await response.arrayBuffer());
        return bytes.length > 0 && bytes.length <= config.limits.audioClipBytes ? bytes : null;
      } catch {
        return null;
      }
    },
  };
}
