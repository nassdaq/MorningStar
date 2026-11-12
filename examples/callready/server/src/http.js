// Small HTTP helpers: reading a body with a size limit and sending a
// response.

import { tooLarge } from './errors.js';

/**
 * Reads the request body, refusing anything over `limit` bytes with a 413.
 * An oversized body is discarded as it arrives rather than buffered; one
 * more than four times the limit is not even waited for.
 */
export function readBody(req, limit, message) {
  return new Promise((resolve, reject) => {
    const declared = Number.parseInt(req.headers['content-length'] ?? '', 10);
    if (Number.isFinite(declared) && declared > limit * 4) {
      reject(tooLarge(message));
      return;
    }
    const chunks = [];
    let size = 0;
    let over = Number.isFinite(declared) && declared > limit;
    let settled = false;
    const fail = (err) => {
      if (settled) return;
      settled = true;
      reject(err);
    };
    req.on('data', (chunk) => {
      size += chunk.length;
      if (size > limit) {
        over = true;
        chunks.length = 0;
        if (size > limit * 4) fail(tooLarge(message));
        return;
      }
      if (!over) chunks.push(chunk);
    });
    req.on('end', () => {
      if (over) fail(tooLarge(message));
      else if (!settled) {
        settled = true;
        resolve(Buffer.concat(chunks));
      }
    });
    req.on('error', fail);
    req.on('aborted', () => fail(new Error('request aborted')));
  });
}

/**
 * Sends a response. If the request body was not read to its end the
 * connection is closed afterwards, so leftover bytes are never parsed as the
 * next request.
 */
export function send(req, res, status, headers, body) {
  if (res.headersSent || res.destroyed) return;
  const all = { 'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff', ...headers, 'Content-Length': body.length };
  if (!req.complete) {
    all.Connection = 'close';
    req.resume();
  }
  res.writeHead(status, all);
  res.end(body);
}

export function sendJson(req, res, status, value, headers = {}) {
  send(req, res, status, { 'Content-Type': 'application/json; charset=utf-8', ...headers }, Buffer.from(JSON.stringify(value)));
}
