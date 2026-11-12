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
export const notFound = (message = 'Not found.') => new HttpError(404, 'not_found', message);
export const callEnded = () => new HttpError(409, 'call_ended', 'This call has already ended.');
export const tooLarge = (message) => new HttpError(413, 'too_large', message);
export const rateLimited = (seconds) =>
  new HttpError(429, 'rate_limited', 'Too many requests. Please wait a moment.', { 'Retry-After': String(seconds) });
export const upstreamFailed = (message) => new HttpError(502, 'upstream_failed', message);
export const voiceUnavailable = () =>
  new HttpError(503, 'voice_unavailable', 'Voice is not set up on this server. Send the reply as text.');

/** A failure of the model or the voice service, raised inside their clients. */
export class UpstreamError extends Error {
  constructor(service, message) {
    super(message);
    this.name = 'UpstreamError';
    this.service = service;
  }
}
