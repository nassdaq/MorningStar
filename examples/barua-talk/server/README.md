# Barua Talk server

The backend of the Barua Talk example app (`examples/barua-talk`). It holds
the key the app must not: the Barua Talk API (`docs/barua-talk-api.md`)
makes the rooms, mints the tickets, rings the calls, dials the phones and
records; PostgreSQL keeps the people, the rooms and their chat, and what
the server knows of every call. `API.md` is the contract with the app.

It is built like the Atlas example's server (`examples/atlas/server`) and
runs beside it: this one listens on **8789**, Atlas on 8788, CallReady on
8787.

The app reaches everything by request and poll. Where upstream hands out a
LiveKit ticket the server passes it on (`token`, `url`), and the app joins
the room's audio with it (`../SCREENS.md`, "Sound").

## Run

```sh
cd examples/barua-talk/server
npm ci                 # one dependency: pg
cp .env.example .env   # then fill in DATABASE_URL and BARUA_API_KEY
npm start              # listens on 127.0.0.1:8789
npm test               # see "Tests"
```

Node 22 or newer (developed on 25). No build step, no framework:
`node:http`, `fetch`, `node:test`.

A phone on USB reaches the server with

```sh
adb reverse tcp:8789 tcp:8789
```

and the address `http://127.0.0.1:8789`.

## Configuration (`.env`, never committed)

| Variable | |
| --- | --- |
| `DATABASE_URL` | PostgreSQL. Required. Only tables prefixed `barua_talk_` are created or touched |
| `BARUA_API_KEY` | The Barua Talk key (`talk:read`, `talk:write`). Without it the server starts, health says `talk: false`, and what needs upstream answers `503 talk_unavailable`; people and stored chat still work |
| `PHONE_CALLS` | `on` to allow phone calls. Off by default: see "Money" |
| `BARUA_CALLER_ID` | Optional caller id (E.164) for phone calls |
| `BARUA_PHONE_HANGUP` | `delete` to hang up phone calls with `DELETE /calls/{id}`, for an upstream that has it |
| `APP_TOKEN` | Optional. If set, every request needs `Authorization: Bearer <token>` |
| `HOST`, `PORT` | `127.0.0.1`, `8789` |
| `BARUA_API_BASE` | `https://barua.tz/api/v1/talk`; the tests point it at a fake |
| `BARUA_REQUESTS_PER_MIN` | 50: how much of upstream's 60 a minute the server lets itself use |
| `BARUA_FORWARD_RESERVE`, `BARUA_REFRESH_RESERVE` | 10 and 20: how much of that is kept back from chat forwards, and from refreshes |
| `RING_TIMEOUT_SECONDS` | 45: an unanswered in-app call is ended |
| `RECORDING_MAX_MINUTES` | 60: a recording nobody stops is stopped |
| `RATE_LIMIT_PER_MIN` | 240 requests a minute per device |
| `DATABASE_SCHEMA` | Optional schema for the tables (the tests use a throwaway one) |

Nothing in `.env` is logged. Log lines carry a method, a path, a status and
a duration, for the app's requests and for upstream's; never a body, a
header, a ticket, a phone number or a device id. Known secrets (and the
database's host) are redacted from error text as well.

## How it treats upstream

`src/talk.js` is the only code that knows the key. Three rules:

- **Nothing is sent twice.** Every upstream request is tried once. A
  failure goes back to the caller.
- **The allowance is counted.** Upstream allows the workspace 60 requests a
  minute. The server spends at most 50 in any 60 seconds and ranks them:
  a *refresh* (a poll wanting fresher state) is refused first, once 30 are
  used; a chat *forward* next, at 40; an *action* (something a person
  pressed) last, at 50. What is refused is refused before it is sent: a
  poll is then answered with the state the server has, a chat message is
  stored with `delivered: false`, an action gets `429 talk_busy`.
- **After upstream itself says 429**, nothing is sent until the minute
  turns (or for its `Retry-After`).

Polls never cost one upstream request each. The app's lists (`rooms`,
`messages`, `activity`, `people`, `calls/incoming`) are answered from the
database. Only three things are refreshed from upstream, each at most once
per time-to-live and once for everyone asking at the same moment
(`createSharedWork`):

| What | When | Upstream request |
| --- | --- | --- |
| An in-app call that is ringing or active | someone polls it and the server's picture is older than 5 s (ringing) or 15 s (active) | `GET /app-calls/{id}` |
| Phone calls that have not ended | someone polls one and the last look was over 5 s ago | one `GET /calls` for all of them |
| A recording that is not ready | someone polls it and the last look was over 10 s ago; a ready one when its link is 50 minutes old | `GET /recordings/{id}` |

Accept, decline and hang-up made through this server are written to the
database at once, so the refresh only matters for what happens elsewhere:
someone closing their app, a phone being put down.

Every 15 seconds (`routes.sweep`) the server also ends rings older than 45
seconds, looks once a minute at live calls nobody is polling, stops
recordings whose call or room is gone or that have run 60 minutes, and
gives up on rows left by a request that never finished. While nothing is
live this costs upstream nothing.

Upstream's errors reach the app as codes it has a sentence for
(`errors.fromTalk`): `402 balance_empty`, `409 no_caller_number`,
`409 not_ringing`, `403 not_your_call`, `503 not_connected`,
`429 talk_busy`, `502 talk_rejected` (the key was refused),
`502 upstream_failed` (no usable answer).

## Money

Rooms, tickets, chat and in-app calls are free. Two things here cost:
phone calls (TSh 300 a minute) and recordings (TSh 30 a minute). SMS
(TSh 40) is not offered by this server at all. What stands between a bug,
a retry or a double tap and the workspace balance:

- **Phone calls are off** unless `PHONE_CALLS=on`. The app cannot join a
  call's audio yet, and upstream documents no hang-up, so a dialled call
  would ring a real phone and cost until that phone hangs up.
- **No retries.** A dial or a recording start is sent upstream once.
  Upstream offers an `Idempotency-Key` only for SMS, so the server keeps
  its own: the call is set down in the database *before* it is dialled.
- **One dial per `request_id`.** A request with a `request_id` this person
  has used returns the first call and dials nothing, even if the first
  attempt failed. Enforced by a unique index, so two requests at the same
  instant cannot both dial.
- **One phone call per person at a time** (`409 call_in_progress`), also a
  unique index.
- **A dial with no usable answer** (a timeout, a dropped connection, a bare
  500) may have rung. It is kept as `failed`, the app is told "may or may
  not have been placed", and nothing is dialled again for that request.
- **One recording per room or call at a time.** Starting again returns the
  running one. A start with no usable answer is looked for upstream once
  (`GET /recordings`) and adopted if found, so that it can be stopped
  rather than run unseen.
- **Recordings stop by themselves** when their call ends, when their room
  is closed, and after `RECORDING_MAX_MINUTES`.
- **No hang-up for phone calls.** Upstream's document lists none. By
  default `DELETE /v1/phone-calls/{id}` sends upstream nothing and answers
  `hung_up: false` with "The call ends when the other side hangs up."
  With `BARUA_PHONE_HANGUP=delete` it sends `DELETE /calls/{id}`.
- **The balance is not known.** Upstream has no endpoint for it.
  `GET /v1/account` reports `empty` after a `402`, `ok` after a priced
  action went through, `unknown` otherwise, and the sum of the charges
  upstream reported for what went through this server.

There is no per-day spending cap here. Add one before real people use it.

## What is stored

In PostgreSQL, in ten tables this server creates (an idempotent migration
at start-up): `barua_talk_schema_version`, `barua_talk_people` (device id,
identity, name, last seen), `barua_talk_rooms`, `barua_talk_members` (who
is in which room, and their read mark), `barua_talk_messages` (the chat),
`barua_talk_direct_messages` and `barua_talk_direct_reads` (messages
between two people and each one's read mark; kept here only, never sent
upstream), `barua_talk_calls` (in-app calls), `barua_talk_phone_calls` (number
dialled, caller id, status, seconds, charge) and `barua_talk_recordings`
(status, seconds, charge, and the current download link). No ticket is
ever stored. A device is an opaque id the app's installation made up.
Closing a room deletes its chat.

## If the database is not there

A database that cannot be reached does not stop the server. At start-up
the first connection is tried three times; after that the server listens
anyway, `GET /v1/health` says `database: false`, and requests that need the
database get `503 database_unavailable`. Every such request (and every
health check) tries to connect again, at most once every three seconds
(`DATABASE_RETRY_MS`), runs the migration when it first succeeds, and
carries on: nothing has to be restarted.

## Tests

```sh
npm test
```

- `test/unit.test.js`: everything against fakes. `test/fakes.js` holds an
  in-process fake of the Barua Talk API written from its document (reached
  over HTTP through `BARUA_API_BASE`, so the real client code runs), and an
  in-memory database.
- `test/integration.test.js`: the same scenarios (`test/scenarios.js`) on
  the real PostgreSQL, so every query in `src/db.js` runs, plus races that
  only a real database settles (two dials at one instant) and the server as
  `npm start` runs it. Upstream is still the fake. It needs `DATABASE_URL`
  in `.env` and is skipped without it. Everything it writes is in a schema
  of its own, `barua_talk_test_<random>`, dropped at the end.
- `test/live.test.js`: **opt-in**, the real Barua Talk API:

  ```sh
  BARUA_LIVE=1 node --test test/live.test.js
  ```

  Free endpoints only (rooms, tickets, chat, in-app calls), about 15
  requests, everything named `mstest-…` and closed afterwards. Its `fetch`
  refuses any request to a priced endpoint before it leaves the process.

Phone calls and recordings have only ever been exercised against the fake.
See `docs/live-notes.md` for what was and was not confirmed against the
real API, and where it differs from its document.

## Before deploying it publicly

This is an example for a development machine. Before it faces the
internet:

- Put it behind HTTPS and set `APP_TOKEN`; better, real accounts. A device
  id is an identity anyone can make up, so anyone who can reach the server
  can read every room, ring anyone, and (with `PHONE_CALLS=on`) spend the
  balance. The rate limit is per device id and in memory.
- Add a spending cap and a list of who may dial.
- Run more than one process only with a shared allowance counter: the
  count of upstream requests is in memory, per process.
- Replace polling with a push for incoming calls: a phone that is not
  polling does not ring.
- Decide how long chat and call history are kept, and say so to users.

## Files

| | |
| --- | --- |
| `API.md` | The contract with the app |
| `docs/barua-talk-api.md` | Upstream's API, as its owner documented it |
| `docs/live-notes.md` | What was confirmed against the real API |
| `src/server.js` | Entry point; tolerant start-up |
| `src/app.js` | Token, device id, rate limit, error shape, access log, the housekeeping timer |
| `src/routes.js` | The endpoints of `API.md`; refreshing; housekeeping |
| `src/talk.js` | The Barua Talk client: one try, the allowance |
| `src/db.js` | PostgreSQL, the migration, reconnecting |
| `src/errors.js`, `src/stores.js`, `src/http.js`, `src/log.js`, `src/config.js` | Errors and upstream's mapped to them, rate limit and shared work, bodies, logging, environment |
| `test/` | See "Tests" |

## The browser test client

`http://127.0.0.1:8789/console` (served from `web/console.html`) is a second
person in a browser tab. It speaks the same contract as the app (`API.md`)
with its own device id: give it a name, and it appears under People in the
app; it can ring the app, be rung by it, make rooms and chat. It also joins
the LiveKit room with the ticket the server hands out (the `livekit-client`
library, loaded from a CDN with an integrity hash), so sound and picture can
be checked from the browser's side. The page holds no secret. Open it in two
tabs and press "Be a new person" in one to try a call without a phone.
