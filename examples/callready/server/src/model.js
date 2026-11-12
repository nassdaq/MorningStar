// DeepSeek: plays the caller and scores the call.
//
// One OpenAI-compatible endpoint, JSON output mode, thinking switched off
// (a spoken reply has to come back in about a second). Each request has a
// timeout and is tried at most twice; an answer that cannot be repaired
// counts as a failure, so the route sees either a valid value or an
// `UpstreamError`, never a half-formed one.

import { UpstreamError } from './errors.js';
import { budgetNote, personaMessages, scoringMessages } from './prompts.js';
import { extractJson, repairReply, repairReport } from './repair.js';
import { SCENARIOS } from './scenarios.js';

const RETRYABLE = new Set([408, 409, 425, 429, 500, 502, 503, 504]);

export function createModel(config, log, { fetchImpl = fetch } = {}) {
  const { apiKey, baseUrl, model, timeoutMs } = config.deepseek;
  const enabled = apiKey !== '';

  /**
   * Sends one chat request and returns `repair(parsedJson)`. Retries once on
   * a timeout, a network error, a retryable status or an unusable answer.
   */
  async function chatJson(label, body, repair) {
    if (!enabled) throw new UpstreamError('deepseek', 'no model key configured');
    let failure = 'no attempt made';
    for (let attempt = 1; attempt <= 2; attempt += 1) {
      const started = performance.now();
      let status = 0;
      try {
        const response = await fetchImpl(`${baseUrl}/chat/completions`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${apiKey}` },
          body: JSON.stringify({
            model,
            stream: false,
            response_format: { type: 'json_object' },
            thinking: { type: 'disabled' },
            ...body,
          }),
          signal: AbortSignal.timeout(timeoutMs),
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

    /**
     * The caller's next line. `turns` is the conversation so far, ending with
     * the learner's newest line; `budget` says how the call stands on time.
     */
    reply(call, turns, budget) {
      const scenario = SCENARIOS[call.scenario];
      return chatJson(
        'reply',
        { messages: personaMessages(call, turns, budgetNote(budget)), temperature: 0.8, max_tokens: 220 },
        (value) => repairReply(value, { callerName: scenario.caller }),
      );
    },

    /** The report for a finished call with at least one learner line. */
    score(call, turns) {
      const learnerSeqs = turns.filter((t) => t.speaker === 'learner').map((t) => t.seq);
      return chatJson(
        'score',
        { messages: scoringMessages(call, turns), temperature: 0.2, max_tokens: 900 },
        (value) => repairReport(value, learnerSeqs),
      );
    },
  };
}

function ms(started) {
  return Math.round(performance.now() - started);
}
