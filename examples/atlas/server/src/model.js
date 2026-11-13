// DeepSeek: answers the user's messages.
//
// One OpenAI-compatible endpoint (`POST /chat/completions`) in JSON output
// mode. What each of the app's models sends, checked against
// https://api-docs.deepseek.com and the account's `GET /models` in November
// 2026 (the ids can be overridden in `.env`):
//
//   fast   deepseek-flash     thinking disabled        a second or two
//   smart  deepseek-v4-pro    thinking enabled, low    a few seconds
//   code   deepseek-v4-pro    thinking disabled        a code-tuned prompt
//   auto   one of the three, chosen from the message (`prompts.pickMode`)
//
// Thinking is on by default on both models, so it is switched off
// explicitly wherever speed matters. In thinking mode `temperature` has no
// effect, and `max_tokens` has to leave room for the reasoning.
//
// Each request has a timeout and is tried at most twice; an answer that
// cannot be repaired counts as a failure, so the route sees either a valid
// value or an `UpstreamError`, never a half-formed one.

import { UpstreamError } from './errors.js';
import { chatMessages, fallbackSteps } from './prompts.js';
import { extractJson, repairAnswer, repairSteps } from './repair.js';

const RETRYABLE = new Set([408, 409, 425, 429, 500, 502, 503, 504]);

export function createModel(config, log, { fetchImpl = fetch } = {}) {
  const { apiKey, baseUrl, models, smartEffort, timeoutMs, smartTimeoutMs } = config.deepseek;
  const enabled = apiKey !== '';

  /** The request parameters of a mode, and how long to wait for it. */
  function settings(mode) {
    if (mode === 'smart') {
      const thinking =
        smartEffort === 'off'
          ? { thinking: { type: 'disabled' }, temperature: 0.5 }
          : { thinking: { type: 'enabled' }, reasoning_effort: smartEffort };
      return { body: { model: models.smart, max_tokens: 4000, ...thinking }, timeout: smartTimeoutMs };
    }
    if (mode === 'code') {
      return { body: { model: models.code, max_tokens: 3000, temperature: 0.2, thinking: { type: 'disabled' } }, timeout: timeoutMs };
    }
    return { body: { model: models.fast, max_tokens: 1200, temperature: 0.6, thinking: { type: 'disabled' } }, timeout: timeoutMs };
  }

  /**
   * Sends one chat request and returns `repair(parsedJson)`. Retries once on
   * a timeout, a network error, a retryable status or an unusable answer.
   */
  async function chatJson(label, body, timeout, repair) {
    if (!enabled) throw new UpstreamError('deepseek', 'no model key configured');
    let failure = 'no attempt made';
    for (let attempt = 1; attempt <= 2; attempt += 1) {
      const started = performance.now();
      let status = 0;
      try {
        const response = await fetchImpl(`${baseUrl}/chat/completions`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${apiKey}` },
          body: JSON.stringify({ stream: false, response_format: { type: 'json_object' }, ...body }),
          signal: AbortSignal.timeout(timeout),
        });
        status = response.status;
        if (!response.ok) {
          await response.arrayBuffer().catch(() => {});
          failure = `status ${status}`;
          log.warn(`upstream deepseek ${label} ${status} ${ms(started)}ms attempt=${attempt}`);
          if (!RETRYABLE.has(status)) break;
          continue;
        }
        const data = await response.json();
        const content = data?.choices?.[0]?.message?.content;
        const value = repair(extractJson(content));
        if (value === null) {
          // JSON mode can return blank content; the documented cure is to retry.
          failure = 'unusable answer';
          log.warn(`upstream deepseek ${label} ${status} ${ms(started)}ms attempt=${attempt} unusable`);
          continue;
        }
        log.info(`upstream deepseek ${label} ${status} ${ms(started)}ms attempt=${attempt}`);
        return value;
      } catch (err) {
        failure = err?.name === 'TimeoutError' ? 'timeout' : `network (${err?.name ?? 'error'})`;
        log.warn(`upstream deepseek ${label} ${failure} ${ms(started)}ms attempt=${attempt}`);
      }
    }
    throw new UpstreamError('deepseek', failure);
  }

  return {
    enabled,

    /** The DeepSeek model id a mode uses (for the log and the README). */
    modelId: (mode) => settings(mode).body.model,

    /**
     * The answer to `text`. `mode` is `fast`, `smart` or `code`; `history`
     * is the chat so far, oldest first. Returns `{title, steps, text}`.
     */
    answer({ mode, history, text, attachments, spoken }) {
      const { body, timeout } = settings(mode);
      const messages = chatMessages({ mode, history, text, attachments, spoken, limits: config.limits });
      return chatJson(mode, { ...body, messages }, timeout, (value) => {
        const answer = repairAnswer(value, { message: text });
        if (!answer) return null;
        // An answer with no usable steps gets ones that are true of any answer.
        const given = Array.isArray(value.steps) || typeof value.steps === 'string';
        if (!given) answer.steps = repairSteps([], fallbackSteps(mode, attachments));
        return answer;
      });
    },
  };
}

function ms(started) {
  return Math.round(performance.now() - started);
}
