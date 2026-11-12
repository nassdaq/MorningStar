# CallReady server

The backend of the CallReady example app (`examples/callready`). A learner
takes a practice phone call from an AI customer and gets a scored report.
The app holds no secrets; this server does the thinking:

- **DeepSeek** plays the caller (one short spoken line per turn, two tips
  for the learner, a `done` flag) and scores the finished call.
- **ElevenLabs**, when a key is present, transcribes the learner's audio and
  gives the caller a voice. Without a key, calls run in text.
- **PostgreSQL** keeps profiles, calls, turns and reports.

The contract with the app is [`API.md`](API.md). Additions to it made by this
implementation are listed under [Beyond API.md](#beyond-apimd).

## Run

```sh
cd examples/callready/server
cp .env.example .env      # then fill in DATABASE_URL and DEEPSEEK_API_KEY
npm install               # one dependency: pg
npm start                 # listens on 127.0.0.1:8787
npm test
```

Node 22 or newer. The server migrates its tables at start-up and logs one
line per request (`GET /v1/profile 200 12ms`) and per upstream call
(`upstream deepseek reply 200 950ms attempt=1`). Keys, the database URL,
request bodies and audio are never logged.

```sh
curl -s localhost:8787/v1/health
H='X-Device-Id: test_readme_01'
curl -s -X POST localhost:8787/v1/calls -H "$H" -H 'Content-Type: application/json' \
     -d '{"scenario":"angry_customer"}'
curl -s -X POST localhost:8787/v1/calls/<call_id>/turns -H "$H" -H 'Content-Type: application/json' \
     -d '{"text":"I am sorry you had to call again. What happens before it logs you out?"}'
curl -s -X POST localhost:8787/v1/calls/<call_id>/end -H "$H"
```

### Reaching it from a phone in development

The server listens on loopback. With the phone connected over USB, forward
the phone's port 8787 to the computer's:

```sh
adb reverse tcp:8787 tcp:8787
```

The app's base URL `http://127.0.0.1:8787` then reaches this server. The
forwarding lasts until the phone is unplugged or adb restarts. To serve a
phone over Wi-Fi instead, start with `HOST=0.0.0.0` and set `APP_TOKEN`
first: the server is then reachable by everything on the network.

## Configuration

Environment variables, read from `.env` in this directory when it exists
(variables already in the environment win). See `.env.example`.

| Variable | Default | |
| --- | --- | --- |
| `DATABASE_URL` | required | PostgreSQL URL. `sslmode=require` encrypts and verifies the certificate. |
| `DEEPSEEK_API_KEY` | required to play calls | Without it the server starts, `health.model` is false and turns return `502`. |
| `ELEVENLABS_API_KEY` | empty | Empty means text-only calls. |
| `PORT` | `8787` | |
| `HOST` | `127.0.0.1` | Set `0.0.0.0` to accept connections from other machines. |
| `APP_TOKEN` | unset | When set, every request needs `Authorization: Bearer <token>`. |
| `DEEPSEEK_MODEL` | `deepseek-chat` | DeepSeek's documentation now lists `deepseek-flash` and `deepseek-v4-pro`; `deepseek-chat` is still accepted and is served by the flash model (checked 2026-11-12). |
| `ELEVENLABS_TTS_MODEL` | `eleven_flash_v2_5` | A low-latency model. |
| `ELEVENLABS_STT_MODEL` | `scribe_v2` | |
| `ELEVENLABS_OUTPUT_FORMAT` | `mp3_44100_64` | Must be an mp3 format: the audio is served as `audio/mpeg`. |
| `ELEVENLABS_VOICE_US`, `_UK`, `_AUSTRALIAN`, `_INDIAN` | unset | A voice id to use for that accent. |
| `RATE_LIMIT_PER_MIN` | `60` | Requests per device per minute. |
| `STREAK_TZ` | `UTC` | The time zone whose midnight separates streak days. |
| `DEEPSEEK_BASE_URL`, `ELEVENLABS_BASE_URL` | the public APIs | Used by the tests to point at local fakes. |

## Enabling voice

Put your ElevenLabs key in `.env` as `ELEVENLABS_API_KEY` and restart.
`GET /v1/health` then reports `"voice": true`, the app may send turns as
audio, and every caller line comes with an `audio_url`.

What the key needs:

- If the key is restricted to certain endpoints, allow **Text to Speech**,
  **Speech to Text** and **Voices (read)**. An unrestricted key works as it is.
- Credit for text-to-speech (every caller line, about 5 to 15 per call, each
  under 280 characters) and speech-to-text (every learner turn sent as audio).
- At least one voice the key can use through the API. At start-up the server
  lists the account's voices once (`GET /v2/voices`) and picks one per accent
  by the voice's accent label (American, British, Australian, Indian),
  preferring one whose gender label suits the caller (Mark and Tom, Dana and
  Priya). An accent with no matching voice uses the account's first voice.
  To choose yourself, set `ELEVENLABS_VOICE_US` and the others to voice ids.
  The start-up log says which accents were matched.

What to expect when something is wrong: if a line cannot be voiced (bad
voice, no credit, timeout) the turn still succeeds with `audio_url: null` and
the log shows `upstream elevenlabs text-to-speech <status>`; if a recording
cannot be transcribed the turn fails with `502 upstream_failed`.

**This path has not been run against the real ElevenLabs service.** It was
written from the API reference (text-to-speech convert, speech-to-text
convert, voices list v2; read 2026-11-12) and is tested against a local fake
of those three endpoints. The first run with a real key is the real test:
start a call, check the log lines above, and play the `audio_url`.

## How a call works

1. `POST /v1/calls` stores the call and returns the scenario's fixed opening
   line (the same line as the app's own script) and two tips. No model
   request is made.
2. Each `POST .../turns` sends the conversation to DeepSeek and returns the
   caller's next line. The learner's words go to the model only as user
   messages; the system prompt says they are speech, never instructions, and
   the response is validated field by field (`src/repair.js`): the line is cut
   to 280 characters, tips to two of at most 28, `done` to a boolean. An
   answer with no usable line is retried once and then becomes
   `502 upstream_failed`; nothing is stored for a failed turn, so the app can
   send it again.
3. The call closes when the caller says goodbye (`done` from the model), when
   its minutes are used up, or at a turn limit (8, 14 or 20 learner turns for
   2, 4 or 6 minutes). On the last turn the caller is told to close and
   `done` is forced to true. Further turns return `409 call_ended`.
4. `POST .../end` scores the call with one model request and stores the
   report. If scoring fails the call stays open and `end` can be sent again.
   A call with no learner turn is ended without a model request and has no
   score.

Requests to DeepSeek and ElevenLabs time out after 15 seconds
(text-to-speech after 10) and are retried once, except text-to-speech,
which is not retried so that a turn is not held up for a voice.

## What is stored

Four tables and a version table, all prefixed `callready_`, created by the
idempotent migration in `src/db.js`. Nothing else in the database is read or
changed.

| Table | Holds |
| --- | --- |
| `callready_schema_version` | Which migrations have run. |
| `callready_profiles` | One row per device id: name, accent, difficulty, minutes, plan. |
| `callready_calls` | One row per call: device, number, scenario, settings, status, start, end, duration. |
| `callready_turns` | Every line of every call: speaker, text, the tips shown with a caller line, the trainer's note on a learner line. |
| `callready_reports` | One row per scored call: overall score, five skill scores, `did_well`, `try_next`. |

The profile's `calls`, `average_score` and `streak_days`, and a call's
`number`, are computed from these rows. Deleting a profile row deletes its
calls, turns and reports.

Not stored: audio. Recordings are passed to ElevenLabs and dropped; spoken
lines are kept in memory for ten minutes (48 MB in total at most) and are
lost on restart. The rate-limit counters are also in memory.

What leaves the server: the transcript of a call goes to DeepSeek; with
voice on, the learner's recordings and the caller's lines go to ElevenLabs.
The learner's name and device id are sent to neither.

## Beyond API.md

- `429 rate_limited`, in the usual error shape and with a `Retry-After`
  header, when a device sends more than `RATE_LIMIT_PER_MIN` requests in a
  minute.
- `500 internal_error` for an unexpected failure (the database being down,
  for example), with a fixed message.
- `GET /v1/health` does not require `X-Device-Id` (it does require the token
  when `APP_TOKEN` is set). `ok` is true when the model key is present and
  the database answers.
- A text turn's `text` must be 1 to 600 characters after trimming, else
  `400`. A recording in which no speech is heard is a `400 bad_request`. A
  body that is neither JSON nor an accepted audio type is a `400`.
- Audio types accepted besides the four in the contract: `audio/m4a`,
  `audio/x-m4a`, `audio/aac`, `audio/mp3`, `audio/x-wav`, `audio/wave`,
  `audio/ogg`, `audio/3gpp`.
- An unscored report (no learner turn) has `score` and `skills` null as the
  contract says; its `did_well` and `try_next` are fixed sentences, not null.
- `number` is one more than the device's highest scored call, so the history
  has no gaps and an unanswered call does not use up a number.
- A spoken line can be fetched only by the device whose call produced it.
- An unknown path, or a known path with another method, is a `404`.

## Before deploying publicly

This is a development server. It is not ready for the internet as it is.

- **Set `APP_TOKEN`.** Without it anyone who can reach the port can spend
  your DeepSeek and ElevenLabs credit. Even with it, the token ships inside
  the app and can be extracted: it keeps strangers out, it does not identify
  users. Real accounts need real authentication; a device id is only a
  random string the client chose, and whoever knows one can read that
  device's history.
- **HTTPS.** The server speaks plain HTTP. Put it behind a TLS-terminating
  proxy, keep `HOST=127.0.0.1` behind that proxy, and make the app refuse
  non-HTTPS base URLs outside development.
- **Rate limits.** The limit is per device id, in memory, per process. A
  client can mint device ids, so add a per-IP limit at the proxy and a daily
  budget per device or per token for model and voice requests. Several
  server processes need a shared store for the counters, the audio and the
  per-call lock (a load balancer must otherwise pin a device to one process).
- **Spending caps.** Set usage limits on the DeepSeek and ElevenLabs keys
  themselves; give the ElevenLabs key only the three permissions above.
- **Key rotation.** Keys live in `.env` or the environment and are read at
  start-up: rotating one means changing it and restarting. Use a secret
  manager and rotate on a schedule, and whenever a key may have been seen.
  Give the server its own database role limited to the `callready_` tables.
- **Data retention.** Transcripts are personal data and are kept for ever;
  nothing deletes old calls, and there is no endpoint for a user to delete
  their data. Decide a retention period, add the deletion job and the
  endpoint, and tell users that their words are sent to DeepSeek and, with
  voice, to ElevenLabs. Check both services' retention terms.
- **Abuse.** The learner's text is passed to a language model. The schema of
  responses is enforced and scores are bounded, but a learner can still try
  to talk the caller out of character; there is no content moderation.
- **Operations.** No metrics, no alerting, no request ids, no backups
  configured here. Abandoned calls stay `active` for ever.
