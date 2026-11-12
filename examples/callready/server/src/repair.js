// Validation and repair of what the model returns.
//
// The model is asked for JSON, but nothing it sends is trusted: every field
// is type-checked, clamped to the contract's limits and given a default
// where one is sensible. A function returns `null` only when the answer has
// nothing usable in it, which the caller treats as an upstream failure.

export const LIMITS = { tip: 28, line: 280, sentence: 90, note: 40 };

export const SKILLS = ['clarity', 'empathy', 'listening', 'problem_solving', 'professional_language'];

const DEFAULT_TIPS = ['Acknowledge the concern', 'Give a clear next step'];

/** Length in characters (code points), as the app counts them. */
export function chars(text) {
  return [...text].length;
}

/** Collapses whitespace and strips control characters. */
export function tidy(value) {
  if (typeof value !== 'string') return '';
  return value.replace(/[\u0000-\u001f\u007f]+/g, ' ').replace(/\s+/g, ' ').trim();
}

/**
 * Cuts `text` to at most `max` characters, at a word boundary when there is
 * one in the second half. `ellipsis` marks the cut and counts in the limit.
 */
export function clip(text, max, { ellipsis = false } = {}) {
  const clean = tidy(text);
  const points = [...clean];
  if (points.length <= max) return clean;
  const room = ellipsis ? max - 1 : max;
  let cut = points.slice(0, room).join('');
  // Only back up to a space when the cut landed inside a word.
  if (!/\s/.test(points[room])) {
    const space = cut.lastIndexOf(' ');
    if (space >= room / 2) cut = cut.slice(0, space);
  }
  cut = cut.replace(/[\s,;:—–-]+$/, '');
  return ellipsis ? `${cut}…` : cut;
}

/** Cuts a spoken line to `max` characters, preferring the end of a sentence. */
export function clipSpoken(text, max = LIMITS.line) {
  const clean = tidy(text);
  if (chars(clean) <= max) return clean;
  const head = [...clean].slice(0, max).join('');
  const end = Math.max(head.lastIndexOf('. '), head.lastIndexOf('! '), head.lastIndexOf('? '));
  if (end >= max / 3) return head.slice(0, end + 1);
  return clip(clean, max, { ellipsis: true });
}

/**
 * Finds a JSON object in the model's text: the text itself, the inside of a
 * code fence, or the outermost braces. Returns `null` when there is none.
 */
export function extractJson(text) {
  if (typeof text !== 'string') return null;
  const candidates = [text.trim()];
  const fence = text.match(/```(?:json)?\s*([\s\S]*?)```/i);
  if (fence) candidates.push(fence[1].trim());
  const first = text.indexOf('{');
  const last = text.lastIndexOf('}');
  if (first !== -1 && last > first) candidates.push(text.slice(first, last + 1));
  for (const candidate of candidates) {
    if (!candidate) continue;
    try {
      const value = JSON.parse(candidate);
      if (value && typeof value === 'object' && !Array.isArray(value)) return value;
    } catch {
      // Try the next shape.
    }
  }
  return null;
}

function truthy(value) {
  if (typeof value === 'boolean') return value;
  if (typeof value === 'string') return /^(true|yes|1)$/i.test(value.trim());
  return value === 1;
}

/** Two tips of at most 28 characters, padded from `fallback`. */
export function repairTips(value, fallback = DEFAULT_TIPS) {
  const list = Array.isArray(value) ? value : typeof value === 'string' ? value.split(/[\n;|]/) : [];
  const tips = [];
  for (const item of [...list, ...fallback, ...DEFAULT_TIPS]) {
    const tip = clip(tidy(item).replace(/^[-*\d.)\s]+/, '').replace(/[.!]+$/, ''), LIMITS.tip);
    if (tip && !tips.some((t) => t.toLowerCase() === tip.toLowerCase())) tips.push(tip);
    if (tips.length === 2) break;
  }
  return tips;
}

/**
 * The caller's turn: `{line, tips, done}`. `callerName` lets a
 * "Dana: ..." prefix be removed. `null` when there is no line to say.
 */
export function repairReply(value, { fallbackTips, callerName } = {}) {
  if (!value || typeof value !== 'object') return null;
  let line = tidy(value.line ?? value.text ?? value.reply ?? value.say);
  if (callerName) line = line.replace(new RegExp(`^${callerName}\\s*:\\s*`, 'i'), '');
  line = line.replace(/^(caller|customer)\s*:\s*/i, '');
  // Stage directions and wrapping quotes do not belong in speech.
  line = line.replace(/\*[^*]*\*/g, ' ').replace(/^\s*["“]+(.*?)["”]+\s*$/, '$1');
  line = clipSpoken(line);
  if (!line) return null;
  return { line, tips: repairTips(value.tips, fallbackTips), done: truthy(value.done) };
}

function score(value) {
  const n = typeof value === 'string' ? Number.parseFloat(value) : value;
  if (typeof n !== 'number' || !Number.isFinite(n)) return null;
  return Math.min(100, Math.max(0, Math.round(n)));
}

function sentence(value, fallback) {
  return clip(tidy(value) || fallback, LIMITS.sentence, { ellipsis: true });
}

/**
 * The scored report. `learnerSeqs` are the transcript positions that may
 * carry a note; notes for anything else are dropped. `null` when no skill
 * score can be read at all.
 */
export function repairReport(value, learnerSeqs = []) {
  if (!value || typeof value !== 'object') return null;
  const raw = value.skills && typeof value.skills === 'object' ? value.skills : value;
  const found = {};
  for (const skill of SKILLS) {
    const n = score(raw[skill] ?? (skill === 'professional_language' ? raw.professional : undefined));
    if (n !== null) found[skill] = n;
  }
  const known = Object.values(found);
  if (known.length === 0) return null;
  const mean = Math.round(known.reduce((a, b) => a + b, 0) / known.length);
  const skills = {};
  for (const skill of SKILLS) skills[skill] = found[skill] ?? mean;

  // The overall score may differ from the mean of the skills, but not wildly:
  // a transcript that talks the model into "score 100" still has to show it
  // in the skills.
  const overall = score(value.score ?? value.overall);
  const bounded = overall === null ? mean : Math.min(mean + 10, Math.max(mean - 10, overall));

  const allowed = new Set(learnerSeqs);
  const notes = {};
  const list = Array.isArray(value.notes) ? value.notes : [];
  for (const item of list) {
    if (!item || typeof item !== 'object') continue;
    const seq = Number(item.n ?? item.index ?? item.turn);
    const note = clip(tidy(item.note ?? item.text), LIMITS.note, { ellipsis: true });
    if (allowed.has(seq) && note && notes[seq] === undefined) notes[seq] = note;
  }

  return {
    score: Math.min(100, Math.max(0, bounded)),
    skills,
    did_well: sentence(value.did_well, 'You stayed on the call and kept working at it.'),
    try_next: sentence(value.try_next, 'Give the caller one clear next step with a time.'),
    notes,
  };
}
