// The API of `API.md`, one function per endpoint.
//
// A handler receives `{req, deviceId, params}` and returns `{status, json}`
// or `{status, bytes, type}`; it reports a failure by throwing an
// `HttpError`. Authentication, the device id, the rate limit, logging and
// the error shape are applied around it in `app.js`.

import { randomBytes } from 'node:crypto';

import { HttpError, UpstreamError, badRequest, notFound, upstreamFailed, voiceUnavailable } from './errors.js';
import { readBody } from './http.js';
import { MODELS, PLANS, pickMode } from './prompts.js';
import { chars, speakable, tidy, tidyBlock } from './repair.js';
import { createKeyedLock } from './stores.js';
import { AUDIO_TYPES } from './voice.js';

const CHAT_LIST_LIMIT = 50;
const MESSAGE_LIST_LIMIT = 100;

/** `2026-11-13T09:14:03Z`: seconds, no fraction, as the contract shows it. */
function isoSeconds(date) {
  return new Date(date).toISOString().replace(/\.\d{3}Z$/, 'Z');
}

function oneOf(value, allowed, field) {
  if (!allowed.includes(value)) {
    throw badRequest(`"${field}" must be one of: ${allowed.join(', ')}.`);
  }
  return value;
}

async function readJson(req, limit) {
  const bytes = await readBody(req, limit, 'The request body is too large (64 KB at most).');
  let value;
  try {
    value = JSON.parse(bytes.toString('utf8'));
  } catch {
    throw badRequest('The request body must be JSON.');
  }
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw badRequest('The request body must be a JSON object.');
  }
  return value;
}

function mediaType(req) {
  return String(req.headers['content-type'] ?? '').split(';')[0].trim().toLowerCase();
}

function steps(labels) {
  return (Array.isArray(labels) ? labels : []).map((label) => ({ label: String(label), state: 'done' }));
}

export function createRoutes({ config, db, model, voice, audio, now }) {
  const withChatLock = createKeyedLock();
  const { limits } = config;
  let health = { at: -Infinity, database: false };

  /** The chat, if it exists and belongs to this device; otherwise 404. */
  async function ownChat(chatId, deviceId) {
    const chat = await db.getChat(chatId);
    if (!chat || chat.device_id !== deviceId) throw notFound('No such chat.');
    return chat;
  }

  function attachmentsFrom(value) {
    if (value === undefined || value === null) return [];
    if (!Array.isArray(value)) throw badRequest('"attachments" must be a list.');
    if (value.length > limits.attachments) throw badRequest(`At most ${limits.attachments} attachments.`);
    return value.map((item) => {
      const name = tidy(item?.name);
      const kind = tidy(item?.kind);
      if (!name || chars(name) > limits.attachmentName) {
        throw badRequest(`An attachment’s "name" must be 1 to ${limits.attachmentName} characters.`);
      }
      if (!kind || chars(kind) > limits.attachmentKind) {
        throw badRequest(`An attachment’s "kind" must be 1 to ${limits.attachmentKind} characters.`);
      }
      return { name, kind };
    });
  }

  /**
   * What was sent: `{text, model, attachments}` from the JSON, or from the
   * transcript of a recording (`model` then comes from `X-Model`).
   */
  async function incoming(req) {
    const type = mediaType(req);
    if (type === 'application/json') {
      const body = await readJson(req, limits.jsonBytes);
      if (typeof body.text !== 'string') throw badRequest('"text" must be a string.');
      const text = tidyBlock(body.text).trim();
      if (!text) throw badRequest('"text" must not be empty.');
      if (chars(text) > limits.textChars) throw badRequest(`"text" must be at most ${limits.textChars} characters.`);
      const chosen = body.model === undefined || body.model === null ? null : oneOf(body.model, MODELS, 'model');
      return { text, model: chosen, attachments: attachmentsFrom(body.attachments) };
    }
    if (AUDIO_TYPES.includes(type)) {
      if (!voice.enabled) throw voiceUnavailable();
      const header = String(req.headers['x-model'] ?? '').trim().toLowerCase();
      const chosen = header === '' ? null : oneOf(header, MODELS, 'X-Model');
      const bytes = await readBody(req, limits.audioBytes, 'The recording is too large (5 MB at most).');
      if (bytes.length === 0) throw badRequest('The recording is empty.');
      let transcript;
      try {
        transcript = await voice.transcribe(bytes, type);
      } catch (err) {
        if (err instanceof UpstreamError) throw upstreamFailed('The voice service could not transcribe the recording.');
        throw err;
      }
      const text = [...tidy(transcript)].slice(0, limits.textChars).join('').trim();
      if (!text) throw badRequest('No speech was heard in the recording.');
      return { text, model: chosen, attachments: [] };
    }
    throw badRequest('Send JSON {"text": ...} or a recording (audio/mp4, audio/mpeg, audio/wav or audio/webm).');
  }

  /** Voices an answer and returns its URL, or null. Never throws. */
  async function speakAnswer(deviceId, text) {
    if (!voice.enabled) return null;
    const clip = await voice.speak(speakable(text, limits.spokenChars));
    if (!clip) return null;
    const id = audio.put(deviceId, clip);
    return id ? `/v1/audio/${id}.mp3` : null;
  }

  /**
   * One exchange: asks the model, stores the message and its answer (and
   * the chat, when `chat` is null), voices the answer if asked to.
   */
  async function exchange({ req, deviceId, chat, sent, history }) {
    const spoken = String(req.headers['x-speak'] ?? '').trim() === '1';
    const chosen = sent.model ?? (await db.getOrCreateProfile(deviceId)).model;
    const mode = chosen === 'auto' ? pickMode(sent.text, sent.attachments) : chosen;

    let answer;
    try {
      answer = await model.answer({ mode, history, text: sent.text, attachments: sent.attachments, spoken: spoken && voice.enabled });
    } catch (err) {
      // Nothing was stored: the app can send the same message again.
      if (err instanceof UpstreamError) throw upstreamFailed('Atlas could not answer just now. Please try again.');
      throw err;
    }

    const at = new Date(now());
    const user = { text: sent.text, attachments: sent.attachments };
    const assistant = { text: answer.text, steps: answer.steps, modelUsed: mode };
    const seq = history.length > 0 ? history[history.length - 1].seq : 0;
    const chatId = chat ? chat.id : `h_${randomBytes(16).toString('hex')}`;
    const title = chat ? chat.title : answer.title;
    // Store the exchange and voice the answer at the same time.
    const [, audioUrl] = await Promise.all([
      chat ? db.addExchange(chatId, { seq, user, assistant, at }) : db.createChat({ id: chatId, deviceId, title, user, assistant, at }),
      spoken ? speakAnswer(deviceId, answer.text) : null,
    ]);
    return {
      status: 200,
      json: {
        chat_id: chatId,
        title,
        heard: sent.text,
        steps: steps(answer.steps),
        answer: { text: answer.text, audio_url: audioUrl },
        model_used: mode,
        turn: seq / 2 + 1,
      },
    };
  }

  const handlers = {
    async health() {
      // The check is cheap but not free; one probe every couple of seconds
      // is enough. It is also what finds a database that has come back.
      if (now() - health.at > 2000) {
        let database = true;
        try {
          await db.ping();
        } catch {
          database = false;
        }
        health = { at: now(), database };
      }
      const body = { ok: model.enabled && health.database, model: model.enabled, voice: voice.enabled, database: health.database };
      return { status: 200, json: body };
    },

    async getProfile({ deviceId }) {
      const row = await db.getOrCreateProfile(deviceId);
      return { status: 200, json: { model: row.model, plan: row.plan } };
    },

    async putProfile({ req, deviceId }) {
      const body = await readJson(req, limits.jsonBytes);
      const patch = {};
      if (body.model !== undefined) patch.model = oneOf(body.model, MODELS, 'model');
      if (body.plan !== undefined) patch.plan = oneOf(body.plan, PLANS, 'plan');
      const row = await db.updateProfile(deviceId, patch);
      return { status: 200, json: { model: row.model, plan: row.plan } };
    },

    async listChats({ deviceId }) {
      const rows = await db.listChats(deviceId, CHAT_LIST_LIMIT);
      return {
        status: 200,
        json: {
          chats: rows.map((row) => ({
            chat_id: row.id,
            title: row.title,
            updated_at: isoSeconds(row.updated_at),
            messages: row.messages,
            state: 'answered',
          })),
        },
      };
    },

    async getChat({ deviceId, params }) {
      const chat = await ownChat(params.chatId, deviceId);
      const rows = await db.getMessages(chat.id, MESSAGE_LIST_LIMIT);
      // The window may have cut an exchange in two: start at a user message.
      while (rows.length > 0 && rows[0].role !== 'user') rows.shift();
      return {
        status: 200,
        json: {
          chat_id: chat.id,
          title: chat.title,
          updated_at: isoSeconds(chat.updated_at),
          messages: rows.map((row) =>
            row.role === 'user'
              ? { role: 'user', text: row.text, attachments: Array.isArray(row.attachments) ? row.attachments : [] }
              : { role: 'assistant', text: row.text, steps: steps(row.steps), model_used: row.model_used ?? 'fast' },
          ),
        },
      };
    },

    async deleteChat({ deviceId, params }) {
      const deleted = await db.deleteChat(params.chatId, deviceId);
      if (!deleted) throw notFound('No such chat.');
      return { status: 200, json: { deleted: true } };
    },

    async startChat({ req, deviceId }) {
      const sent = await incoming(req);
      return exchange({ req, deviceId, chat: null, sent, history: [] });
    },

    async message({ req, deviceId, params }) {
      // A recording is refused before it is read: a 5 MB upload for a chat
      // that is someone else's should not be transcribed first. A text
      // message is small, and is checked once, under the lock.
      if (mediaType(req) !== 'application/json') await ownChat(params.chatId, deviceId);
      const sent = await incoming(req);
      return withChatLock(params.chatId, async () => {
        const [chat, history] = await Promise.all([ownChat(params.chatId, deviceId), db.getMessages(params.chatId, limits.historyMessages)]);
        return exchange({ req, deviceId, chat, sent, history });
      });
    },

    async getAudio({ deviceId, params }) {
      const bytes = audio.get(deviceId, params.audioId);
      if (!bytes) throw notFound('This audio is no longer available.');
      return { status: 200, bytes, type: 'audio/mpeg' };
    },
  };

  const CHAT_ID = 'h_[0-9a-f]{32}';
  const table = [
    ['GET', /^\/v1\/health$/, handlers.health, []],
    ['GET', /^\/v1\/profile$/, handlers.getProfile, []],
    ['PUT', /^\/v1\/profile$/, handlers.putProfile, []],
    ['GET', /^\/v1\/chats$/, handlers.listChats, []],
    ['POST', /^\/v1\/chats$/, handlers.startChat, []],
    ['GET', new RegExp(`^/v1/chats/(${CHAT_ID})$`), handlers.getChat, ['chatId']],
    ['DELETE', new RegExp(`^/v1/chats/(${CHAT_ID})$`), handlers.deleteChat, ['chatId']],
    ['POST', new RegExp(`^/v1/chats/(${CHAT_ID})/messages$`), handlers.message, ['chatId']],
    ['GET', /^\/v1\/audio\/(a_[0-9a-f]{24})\.mp3$/, handlers.getAudio, ['audioId']],
  ];

  /** The handler and path parameters for a request, or a 404. */
  return function match(method, path) {
    for (const [verb, pattern, handler, names] of table) {
      if (verb !== method) continue;
      const found = pattern.exec(path);
      if (!found) continue;
      const params = Object.fromEntries(names.map((name, i) => [name, found[i + 1]]));
      return { handler, params, open: handler === handlers.health };
    }
    throw new HttpError(404, 'not_found', 'Not found.');
  };
}
