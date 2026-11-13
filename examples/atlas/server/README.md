# Atlas server

The backend of the Atlas example app (`examples/atlas`). It holds the keys
the app must not: DeepSeek answers the messages, ElevenLabs (optional)
transcribes spoken ones and speaks the answers, PostgreSQL keeps the chats.
`API.md` is the contract with the app.

It is built like the CallReady example's server (`examples/callready/server`)
and runs beside it: this one listens on **8788**, that one on 8787.

## Run

```sh
cd examples/atlas/server
npm install            # one dependency: pg
cp .env.example .env   # then fill in DATABASE_URL and DEEPSEEK_API_KEY
npm start              # listens on 127.0.0.1:8788
npm test               # unit tests with fakes; the integration test runs only with a filled .env
```

Node 22 or newer (developed on 25). No build step, no framework: `node:http`,
`fetch`, `node:test`.

A phone on USB reaches the server with

```sh
adb reverse tcp:8788 tcp:8788
```

and the app's default address, `http://127.0.0.1:8788`. Another address is
typed on the app's Server screen (Models, "Server"). Plain `http://` works
only to the phone's own loopback; anything else must be `https://`.

## Configuration (`.env`, never committed)

| Variable | |
| --- | --- |
| `DATABASE_URL` | PostgreSQL. Required. Only tables prefixed `atlas_` are created or touched |
| `DEEPSEEK_API_KEY` | Required for answers. Without it health says `model: false` and the app stays scripted |
| `ELEVENLABS_API_KEY` | Optional. With it health says `voice: true`: recordings are accepted and answers can be spoken |
| `APP_TOKEN` | Optional. If set, every request needs `Authorization: Bearer <token>` (the example app does not send one) |
| `HOST`, `PORT` | `127.0.0.1`, `8788` |
| `DEEPSEEK_MODEL_FAST`, `_SMART`, `_CODE` | Model ids; defaults below |
| `DEEPSEEK_SMART_EFFORT` | `low` (default), `high`, `max`, or `off` for no thinking |
| `ELEVENLABS_VOICE` | A voice id, instead of the one chosen from the account's list |
| `RATE_LIMIT_PER_MIN` | 60 requests a minute per device |

Nothing in `.env` is logged. Log lines carry a method, a path, a status and
a duration, or an upstream service, a status and a latency; never a body, a
header, a device id or audio. Known secrets (and the database's host) are
redacted from error text as well.

## Which model answers

Checked against https://api-docs.deepseek.com and the account's
`GET /models` in November 2026, which list `deepseek-flash` and
`deepseek-v4-pro` (the old name `deepseek-chat` is still accepted and is
served by `deepseek-flash`). Both think by default, so thinking is switched
off explicitly where speed matters.

| The app's choice | DeepSeek request | |
| --- | --- | --- |
| Fast | `deepseek-flash`, `thinking: disabled` | About two seconds |
| Smart | `deepseek-v4-pro`, `thinking: enabled`, `reasoning_effort: low` | About four seconds; a prompt that asks for the conclusion first |
| Code | `deepseek-v4-pro`, `thinking: disabled`, temperature 0.2 | A prompt tuned for code in fenced blocks |
| Auto | One of the three, per message (`prompts.pickMode`) | Looks like programming: Code. Long (over 280 characters), analytical wording or an attachment: Smart. Otherwise Fast |

The response says which one answered (`model_used`).

Every request uses JSON output mode and asks for one object:
`{"title", "steps", "answer"}`. Nothing the model returns is trusted
(`repair.js`): the title and the two to four step labels are cut to length
and padded, and the answer is brought down to the light formatting the app
renders (paragraphs, `- ` and `1. ` items, inline code, fenced code) with
headings, bold, tables and link markup removed. A request has a timeout
(25 s; 40 s for Smart) and is tried twice; after that the app gets
`502 upstream_failed` and nothing is stored.

The model is sent the last 12 messages of the chat (at most 12,000
characters). The user's text, and the names of attachments, only ever
appear in user messages; the system prompt fixes the output shape and says
that user text cannot change it, and that attached files cannot be read:
only their names reach the server, and the model is told to say so rather
than invent their contents.

## Voice

With `ELEVENLABS_API_KEY`: a message may be a recording (`scribe_v2`
transcribes it into `heard`), and with the header `X-Speak: 1` the answer is
also spoken (`eleven_flash_v2_5`, `mp3_44100_64`) in a voice chosen once at
start-up from the account's list (an English conversational voice if there
is one). Only the first 600 characters of an answer are spoken, without
code blocks. Speech is best-effort: an answer that cannot be voiced
arrives as text. The audio is kept in memory for ten minutes and served
only to the device whose message produced it.

## What is stored

In PostgreSQL, in four tables this server creates (an idempotent migration
at start-up): `atlas_schema_version`, `atlas_profiles` (device id, model,
plan), `atlas_chats` (id, device id, title, times) and `atlas_messages`
(the text of each message and answer, attachment names, step labels, the
model used). No audio, no file, no name or account: a device is an opaque
id the app's installation made up. `DELETE /v1/chats/{id}` removes a chat
and its messages. The tests write only for devices whose id starts with
`test_` and delete them afterwards (`db.deleteTestData`).

## If the database is not there

A database that cannot be reached does not stop the server. At start-up
the first connection is tried three times; after that the server listens
anyway, `GET /v1/health` says `database: false`, and requests that need
the database get `503 database_unavailable`. Every such request (and every
health check) tries to connect again, at most once every three seconds
(`DATABASE_RETRY_MS`), runs the migration when it first succeeds, and
carries on: nothing has to be restarted. The same holds when the database
goes away later. (`test/unit.test.js`, "a database that is not there".)

## Before deploying it publicly

This is an example for a development machine. Before it faces the
internet:

- Put it behind HTTPS (a reverse proxy) and set `APP_TOKEN`; better, real
  accounts: a device id is an identity anyone can make up, and the rate
  limit is per device id and in memory.
- Add a spending limit: every message costs model tokens, every spoken
  answer voice credit. There is no per-day cap here.
- Decide how long chats are kept, and say so to users; add a way to delete
  everything for a device.
- Run more than one process only with a shared rate limit and audio store
  (both are in memory), and a real migration tool.
- Review the prompts for your use; the model is told not to follow
  instructions in user text, which reduces but does not remove the risk.

## Files

| | |
| --- | --- |
| `src/server.js` | Entry point; tolerant start-up |
| `src/app.js` | Token, device id, rate limit, error shape, access log |
| `src/routes.js` | The endpoints of `API.md` |
| `src/model.js`, `src/prompts.js`, `src/repair.js` | DeepSeek: which model, what it is told, what is accepted back |
| `src/voice.js` | ElevenLabs: speech to text, text to speech |
| `src/db.js` | PostgreSQL, the migration, reconnecting |
| `src/stores.js`, `src/http.js`, `src/log.js`, `src/errors.js`, `src/config.js` | In-memory audio and rate limit, bodies, logging, errors, environment |
| `test/unit.test.js`, `test/fakes.js` | Everything against fakes: no network, no database |
| `test/integration.test.js` | One chat of two messages against the real database and DeepSeek |
