// Validation and repair of what the model returns.
//
// The model is asked for JSON, but nothing it sends is trusted: every field
// is type-checked, cut to the contract's limits and given a default where
// one is sensible, and the answer's text is brought down to the light
// formatting the app renders (API.md). A function returns `null` only when
// the answer has nothing usable in it, which the caller treats as an
// upstream failure.

export const LIMITS = { step: 40, title: 40, answer: 4000 };

const DEFAULT_STEPS = ['Reading the request', 'Writing the answer'];
const FENCE = /^\s*```/;

/** Length in characters (code points), as the app counts them. */
export function chars(text) {
  return [...text].length;
}

/** Collapses whitespace and strips control characters: one line. */
export function tidy(value) {
  if (typeof value !== 'string') return '';
  return value.replace(/[\u0000-\u001f\u007f]+/g, ' ').replace(/\s+/g, ' ').trim();
}

/**
 * Text of several lines, cleaned: line breaks normalised, tabs made spaces,
 * other control characters removed, trailing spaces dropped, at most one
 * blank line in a row, no blank lines at the ends.
 */
export function tidyBlock(value) {
  if (typeof value !== 'string') return '';
  return value
    .replace(/\r\n?/g, '\n')
    .replace(/\t/g, '  ')
    .replace(/[\u0000-\u0009\u000b-\u001f\u007f​-‏‪-‮⁦-⁩﻿]/g, '')
    .split('\n')
    .map((line) => line.replace(/\s+$/, ''))
    .join('\n')
    .replace(/\n{3,}/g, '\n\n')
    .replace(/^\n+|\n+$/g, '');
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

/**
 * Finds a JSON object in the model's text: the text itself, the inside of a
 * code fence, or the outermost braces. Returns `null` when there is none.
 */
export function extractJson(text) {
  if (typeof text !== 'string') return null;
  const candidates = [text.trim()];
  const fence = text.match(/^```(?:json)?\s*\n([\s\S]*?)\n```\s*$/i);
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

/** One line of prose brought down to the formatting the app renders. */
function plainLine(line) {
  let out = line;
  // A heading becomes its text; a quote loses its mark.
  out = out.replace(/^\s{0,3}#{1,6}\s+/, '').replace(/^\s{0,3}>\s?/, '');
  // Bullets of any kind become "- "; "1)" becomes "1.".
  out = out.replace(/^\s*[*•·–—+]\s+/, '- ').replace(/^\s*-\s+/, '- ');
  out = out.replace(/^\s*(\d{1,3})[.)]\s+/, '$1. ');
  // A horizontal rule or a table's separator row says nothing.
  if (/^\s*([-*_=]\s*){3,}$/.test(out) || /^\s*\|?[\s:|-]{3,}\|?\s*$/.test(out)) return '';
  // Emphasis marks go; what they wrapped stays. Inline code is left alone.
  out = out
    .split(/(`[^`\n]*`)/)
    .map((part) => {
      if (part.startsWith('`')) return part;
      return part
        .replace(/!?\[([^\]\n]+)\]\(([^)\s]+)\)/g, '$1 ($2)')
        .replace(/(\*\*|__)(?=\S)(.+?)(?<=\S)\1/g, '$2')
        .replace(/(?<![\w*])\*(?=\S)([^*\n]+?)(?<=\S)\*(?![\w*])/g, '$1')
        .replace(/~~(?=\S)(.+?)(?<=\S)~~/g, '$1');
    })
    .join('');
  return out;
}

/**
 * The answer's text as the contract promises it: paragraphs, "- " and
 * "1. " list items, `inline code` and fenced code blocks, and nothing else;
 * at most `max` characters. Code inside a fence is not touched. Returns ''
 * when nothing is left.
 */
export function repairAnswerText(value, max = LIMITS.answer) {
  const lines = tidyBlock(value).split('\n');
  const out = [];
  let inCode = false;
  for (const line of lines) {
    if (FENCE.test(line)) {
      // "```rust" opens, "```" closes; anything after a closing fence's
      // marks is dropped.
      const language = inCode ? '' : line.replace(/^\s*```+/, '').trim().replace(/[^A-Za-z0-9+#._-]/g, '').slice(0, 20);
      out.push(`\`\`\`${language}`);
      inCode = !inCode;
    } else {
      out.push(inCode ? line : plainLine(line));
    }
  }
  if (inCode) out.push('```');
  let text = out.join('\n').replace(/\n{3,}/g, '\n\n').replace(/^\n+|\n+$/g, '');
  if (chars(text) <= max) return text;

  // Too long: keep whole lines while they fit, close a fence left open and
  // say that something was cut.
  const kept = [];
  let used = 0;
  let open = false;
  const room = max - 8; // "\n```" and "\n…" at the end
  for (const line of text.split('\n')) {
    const cost = chars(line) + 1;
    if (used + cost > room) break;
    kept.push(line);
    used += cost;
    if (FENCE.test(line)) open = !open;
  }
  if (kept.length === 0) return `${[...text].slice(0, max - 1).join('')}…`;
  if (open) kept.push('```');
  text = `${kept.join('\n').replace(/\n+$/, '')}\n…`;
  return text;
}

/** Two to four step labels of at most 40 characters, padded from `fallback`. */
export function repairSteps(value, fallback = DEFAULT_STEPS) {
  let list = [];
  if (Array.isArray(value)) list = value;
  else if (typeof value === 'string') list = value.split(/[\n;|]/);
  const labels = [];
  const add = (item) => {
    const raw = item && typeof item === 'object' ? item.label ?? item.name ?? item.text ?? item.title : item;
    const label = clip(
      tidy(typeof raw === 'string' ? raw : '')
        .replace(/^[-*•\d.)\s]+/, '')
        .replace(/[*_`#]+/g, '')
        .replace(/[.!…:;,]+$/, ''),
      LIMITS.step,
    );
    if (label && !labels.some((l) => l.toLowerCase() === label.toLowerCase())) labels.push(label);
  };
  for (const item of list) {
    if (labels.length === 4) break;
    add(item);
  }
  for (const item of [...fallback, ...DEFAULT_STEPS]) {
    if (labels.length >= 2) break;
    add(item);
  }
  return labels;
}

/** A chat title of at most 40 characters; from the message when the model gave none. */
export function repairTitle(value, message) {
  const strip = (text) =>
    tidy(text)
      .replace(/^["“'‘`*#\s]+|["”'’`*\s]+$/g, '')
      .replace(/^(title|chat)\s*:\s*/i, '')
      .replace(/[.!:;,]+$/, '');
  const title = clip(strip(typeof value === 'string' ? value : ''), LIMITS.title, { ellipsis: true });
  if (title) return title;
  return clip(strip(message ?? '') || 'New chat', LIMITS.title, { ellipsis: true });
}

/**
 * The assistant's turn: `{title, steps, text}`. `message` is what the user
 * sent (the title's fallback). `null` when there is no answer text.
 */
export function repairAnswer(value, { message = '' } = {}) {
  if (!value || typeof value !== 'object') return null;
  let raw = value.answer ?? value.text ?? value.reply ?? value.response ?? value.content;
  if (raw && typeof raw === 'object' && !Array.isArray(raw)) raw = raw.text ?? raw.content;
  if (Array.isArray(raw)) raw = raw.filter((part) => typeof part === 'string').join('\n\n');
  const text = repairAnswerText(typeof raw === 'string' ? raw : '');
  if (!text) return null;
  return {
    title: repairTitle(value.title, message),
    steps: repairSteps(value.steps),
    text,
  };
}

/**
 * What of an answer is said aloud: no code blocks, no list marks, and at
 * most `max` characters, ending at the end of a sentence when it can.
 */
export function speakable(text, max = 600) {
  const parts = [];
  let inCode = false;
  for (const line of String(text).split('\n')) {
    if (FENCE.test(line)) {
      if (!inCode) parts.push('(There is code here, which I will leave for you to read.)');
      inCode = !inCode;
    } else if (!inCode) {
      parts.push(line.replace(/^- /, '').replace(/`/g, ''));
    }
  }
  const clean = tidy(parts.join(' '));
  if (chars(clean) <= max) return clean;
  const head = [...clean].slice(0, max).join('');
  const end = Math.max(head.lastIndexOf('. '), head.lastIndexOf('! '), head.lastIndexOf('? '));
  if (end >= max / 3) return head.slice(0, end + 1);
  return clip(clean, max, { ellipsis: true });
}
