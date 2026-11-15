// The error shape of the API: a status, a snake_case code and a message
// written for a person. Anything else that is thrown becomes a 500 with a
// fixed message; no stack trace or internal text reaches a response.

export class HttpError extends Error {
  constructor(status, code, message, headers) {
    super(message);
    this.name = 'HttpError';
    this.status = status;
    this.code = code;
    this.headers = headers;
  }
}

export const badRequest = (message) => new HttpError(400, 'bad_request', message);
export const unauthorized = () => new HttpError(401, 'unauthorized', 'This app is not allowed to use the server.');
export const forbidden = (code, message) => new HttpError(403, code, message);
export const notFound = (message = 'Not found.') => new HttpError(404, 'not_found', message);
export const conflict = (code, message) => new HttpError(409, code, message);
export const tooLarge = (message) => new HttpError(413, 'too_large', message);
export const rateLimited = (seconds) =>
  new HttpError(429, 'rate_limited', 'Too many requests. Please wait a moment.', { 'Retry-After': String(seconds) });
export const talkBusy = (seconds) =>
  new HttpError(429, 'talk_busy', 'Barua Talk is busy this minute. Please try again shortly.', {
    'Retry-After': String(seconds),
  });
export const upstreamFailed = (message = 'Barua Talk could not do that just now. Please try again.') =>
  new HttpError(502, 'upstream_failed', message);
export const talkUnavailable = () =>
  new HttpError(503, 'talk_unavailable', 'Calls and new rooms are not set up on this server yet.');
export const phoneUnavailable = () =>
  new HttpError(503, 'phone_unavailable', 'Phone calls are switched off on this server.');
export const databaseUnavailable = () =>
  new HttpError(503, 'database_unavailable', 'The server’s database is not answering just now. Please try again.', {
    'Retry-After': '5',
  });

/**
 * What the Barua Talk client throws. `kind` says what happened:
 *
 * - `disabled`: there is no key; nothing was sent.
 * - `busy`: the minute's allowance is used up; nothing was sent.
 *   `retryAfter` is in seconds.
 * - `http`: upstream answered with an error. `status` and `code` are its
 *   own, `message` its sentence.
 * - `network`: no usable answer (no connection, a timeout, a 5xx without
 *   the error shape, a body that is not JSON). **The request may or may
 *   not have taken effect.**
 */
export class TalkError extends Error {
  constructor(kind, { status = 0, code = '', message = '', retryAfter = 0 } = {}) {
    super(message || kind);
    this.name = 'TalkError';
    this.kind = kind;
    this.status = status;
    this.code = code;
    this.retryAfter = retryAfter;
  }
}

// Upstream's 403s and 409s that the app has a sentence for.
const PASSED_ON = new Set(['not_your_call', 'not_ringing', 'no_caller_number']);

/** The answer the app gets for a failed upstream request. */
export function fromTalk(err) {
  if (!(err instanceof TalkError)) return err;
  if (err.kind === 'disabled') return talkUnavailable();
  if (err.kind === 'busy') return talkBusy(err.retryAfter || 5);
  if (err.kind === 'network') return upstreamFailed();
  const message = err.message || 'Barua Talk refused the request.';
  switch (err.status) {
    case 401:
      return new HttpError(502, 'talk_rejected', 'This server’s Barua key was refused.');
    case 402:
      return new HttpError(402, 'balance_empty', 'The workspace balance is empty. Top up to carry on.');
    case 403:
      if (err.code === 'insufficient_scope') return new HttpError(502, 'talk_rejected', 'This server’s Barua key may not do this.');
      return forbidden(PASSED_ON.has(err.code) ? err.code : 'not_yours', message);
    case 404:
      return notFound(message);
    case 409:
      return conflict(PASSED_ON.has(err.code) ? err.code : 'conflict', message);
    case 422:
      return badRequest(message);
    case 429:
      return talkBusy(err.retryAfter || 5);
    case 503:
      return new HttpError(503, 'not_connected', 'This is not switched on for the workspace yet.');
    default:
      return upstreamFailed();
  }
}

/** The database could not be reached (or is not migrated yet). */
export class DatabaseDown extends Error {
  constructor(cause) {
    super('database unavailable');
    this.name = 'DatabaseDown';
    this.cause = cause;
  }
}
