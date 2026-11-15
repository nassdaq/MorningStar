// The Barua Talk API client (`docs/barua-talk-api.md`): the only code that
// knows the key.
//
// Three rules live here.
//
// 1. Nothing is ever sent twice. A request is tried once; a failure is
//    reported to the caller, who decides. Dialling a phone and starting a
//    recording cost money, and upstream offers no idempotency key for
//    either, so a blind retry could pay twice.
//
// 2. The allowance is counted. Upstream answers 429 beyond 60 requests a
//    minute for the whole workspace. The client spends at most
//    `perMinute` in any 60 seconds and gives each request a priority:
//    `refresh` (a poll wanting fresher state) stops first, `forward` (a
//    chat message copied into a room) next, `action` (something a person
//    pressed) last. What is refused is refused before anything is sent.
//
// 3. After upstream itself says 429, nothing is sent until the minute
//    turns.

import { TalkError } from './errors.js';

export const PRIORITIES = ['action', 'forward', 'refresh'];

export function createTalk(config, log, { fetchImpl = fetch, now = Date.now } = {}) {
  const { apiKey, baseUrl, timeoutMs, perMinute, forwardReserve, refreshReserve } = config.talk;
  const enabled = Boolean(apiKey);
  let sent = []; // times of the requests of the last minute
  let blockedUntil = 0;

  function ceiling(priority) {
    if (priority === 'refresh') return perMinute - refreshReserve;
    if (priority === 'forward') return perMinute - forwardReserve;
    return perMinute;
  }

  /** Seconds until a request of this priority could be sent; 0 if now. */
  function wait(priority = 'action') {
    const time = now();
    sent = sent.filter((t) => t > time - 60000);
    if (time < blockedUntil) return Math.max(1, Math.ceil((blockedUntil - time) / 1000));
    const limit = ceiling(priority);
    if (sent.length < limit) return 0;
    // The request that has to leave the window before there is room.
    const oldest = sent[sent.length - limit];
    return Math.max(1, Math.ceil((oldest + 60000 - time) / 1000));
  }

  /**
   * One request. Returns the parsed JSON of a 2xx answer; throws a
   * `TalkError` otherwise (see `errors.js` for its kinds).
   */
  async function request(method, path, { body, priority = 'action', timeout = timeoutMs } = {}) {
    if (!enabled) throw new TalkError('disabled');
    const seconds = wait(priority);
    if (seconds > 0) throw new TalkError('busy', { retryAfter: seconds });
    sent.push(now());

    // A path is logged without its query and without ids being interpreted.
    const label = `${method} ${path.split('?')[0]}`;
    const started = performance.now();
    const took = () => `${Math.round(performance.now() - started)}ms`;
    let response;
    let text;
    try {
      response = await fetchImpl(`${baseUrl}${path}`, {
        method,
        headers: {
          Authorization: `Bearer ${apiKey}`,
          Accept: 'application/json',
          ...(body === undefined ? {} : { 'Content-Type': 'application/json' }),
        },
        body: body === undefined ? undefined : JSON.stringify(body),
        signal: AbortSignal.timeout(timeout),
      });
      text = await response.text();
    } catch (err) {
      log.warn(`talk ${label} failed after ${took()} (${err?.cause?.code ?? err?.name ?? 'error'})`);
      throw new TalkError('network');
    }

    let json = null;
    try {
      json = text ? JSON.parse(text) : null;
    } catch {
      json = null;
    }
    log.info(`talk ${label} ${response.status} ${took()}`);

    if (response.ok) {
      if (!json || typeof json !== 'object') throw new TalkError('network');
      return json;
    }
    if (response.status === 429) {
      // "Until the minute turns": the header if there is one, else the
      // start of the next minute of the clock.
      const header = Number.parseInt(response.headers.get('retry-after') ?? '', 10);
      const untilMinute = 60000 - (now() % 60000);
      const pause = Number.isFinite(header) && header > 0 ? Math.min(header, 120) * 1000 : untilMinute;
      blockedUntil = now() + pause;
      throw new TalkError('http', { status: 429, code: 'rate_limited', retryAfter: Math.max(1, Math.ceil(pause / 1000)) });
    }
    const error = json && typeof json.error === 'object' && json.error ? json.error : null;
    if (!error || typeof error.code !== 'string') {
      // An error that is not in upstream's own shape (a bare 500, a proxy's
      // page): nothing can be concluded from it.
      throw new TalkError('network', { status: response.status });
    }
    throw new TalkError('http', {
      status: response.status,
      code: error.code,
      message: typeof error.message === 'string' ? error.message.slice(0, 300) : '',
    });
  }

  return {
    enabled,
    request,
    wait,
    /** How many requests went out in the last minute (for the tests). */
    used() {
      const time = now();
      return sent.filter((t) => t > time - 60000).length;
    },
  };
}
