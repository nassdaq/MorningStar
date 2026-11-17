# Barua Talk server API

The contract between the Barua Talk app (`examples/barua-talk`) and its
server (`examples/barua-talk/server`). The app holds no secrets: the Barua
Talk API (`docs/barua-talk-api.md`, called "upstream" below) and the
database are reached only by the server.

Everything here is plain request and response, and the app learns of
changes by polling. Wherever upstream hands out a LiveKit ticket the server
passes it on (`token`, `url`). The app joins the room's audio with it, on
Android (`../SCREENS.md`, "Sound"; video is not in the app yet); the
contract did not change for that.

- Base URL: configurable in the app; `http://127.0.0.1:8789` in development
  (the phone reaches it with `adb reverse tcp:8789 tcp:8789`).
- All bodies are JSON (`Content-Type: application/json`). A body is at most
  16 KB.
- Every request except `GET /v1/health` carries
  `X-Device-Id: <opaque id, 8..64 chars of [A-Za-z0-9_-]>`, the
  installation's identity. If the server is started with `APP_TOKEN`, every
  request must also carry `Authorization: Bearer <token>`.
- Times are UTC, `2026-11-15T07:00:00Z`. Responses that are polled carry
  `now`, the server's clock, so the app never needs its own to show "2 min
  ago" or a running call timer.
- Every action that is not a `GET` is also reachable as a `POST` (noted at
  each endpoint), for a client that prefers two verbs.

## Errors

A non-2xx status with

```json
{"error": {"code": "snake_case", "message": "A sentence for a person."}}
```

`message` can be shown as it is. `code` is one of:

| Status | `code` | When |
|---|---|---|
| 400 | `bad_request` | The request is malformed; the message names the field. Also upstream's `422 invalid_request`. |
| 401 | `unauthorized` | `APP_TOKEN` is set and the request did not carry it. |
| 402 | `balance_empty` | The workspace balance cannot cover a phone call or a recording. Top up, then try again. |
| 403 | `not_your_call` | Answering a call that rings for someone else. |
| 403 | `not_yours` | Closing a room someone else made. |
| 404 | `not_found` | No such room, person, call or recording (also for one this person may not see). |
| 409 | `not_ringing` | Accepting or declining a call that is no longer ringing. |
| 409 | `no_caller_number` | The workspace has no voice number, so it cannot dial phones. |
| 409 | `call_in_progress` | This person already has a phone call that has not ended. |
| 409 | `recording_unsupported` | This call cannot be recorded (upstream did not say which room it is). |
| 409 | `conflict` | Any other "not in a state that allows this" from upstream. |
| 413 | `too_large` | The body is over 16 KB. |
| 429 | `rate_limited` | This device sent more than 240 requests in a minute. `Retry-After` says how long to wait. |
| 429 | `talk_busy` | The server has used its upstream allowance for this minute (upstream allows 60 requests a minute for the whole workspace). `Retry-After` says how long to wait. Only actions get this; polls are answered from the server's own state. |
| 500 | `internal_error` | A bug in the server. |
| 502 | `upstream_failed` | Upstream did not answer, or answered nonsense. For a phone call this means "may or may not have been placed": look at `GET /v1/phone-calls` before dialling again. |
| 502 | `talk_rejected` | Upstream refused the server's key (`invalid_key`, `insufficient_scope`). Nothing the app can do. |
| 503 | `talk_unavailable` | The server has no Barua key. Rooms cannot be made, nothing can ring. Stored chat still works. |
| 503 | `phone_unavailable` | Phone calls are switched off on this server (`PHONE_CALLS` is not `on`). |
| 503 | `not_connected` | Upstream has not provisioned this capability (phone, recording) for the workspace yet. |
| 503 | `database_unavailable` | The database cannot be reached just now; the server reconnects by itself, so send the same request again. |

## People

The server keeps one person per installation.

```json
{"identity": "neema", "name": "Neema"}
```

- `identity` is the person's address: who they are in a room, who a call is
  from and to. Lower case letters, digits and dashes, 2 to 40 characters,
  unique, and **it never changes once given**.
- `name` is what other people see. Wherever a person other than "me"
  appears in a response, `name` is always a string: their name, or their
  identity while they have none.

How an identity comes about: the first `PUT /v1/me` with a name derives it
from the name (`"Neema Mushi"` → `neema-mushi`, then `neema-mushi-2` if
taken). A device that does anything else first (lists rooms, posts,
calls …) is given `guest-` and six hex digits, for good. So an app that
wants pretty identities sends `PUT /v1/me` before anything else; an app
with no naming screen need not care.

### `GET /v1/me`

```json
{"identity": null, "name": null}
```

for a device seen for the first time (this request alone gives it no
identity), otherwise the person as above (`name` null until one is set).

### `PUT /v1/me`

```json
{"name": "Neema Mushi"}
```

`name` is 1..40 characters after trimming. Returns the person:
`{"identity": "neema-mushi", "name": "Neema Mushi"}`. Sending it again
changes the name only.

### `GET /v1/people`

Everyone else who has an identity, most recently seen first, at most 200:

```json
{"people": [{"identity": "baraka", "name": "Baraka", "seen_at": "2026-11-15T07:01:10Z", "online": true,
             "unread": 1, "last_message": …message…}],
 "now": "2026-11-15T07:01:12Z"}
```

`unread` and `last_message` are this person's conversation with that
person ("Messages between two people" below): 0 and `null` when nothing
was ever written.

`online` is true when that person's app made any request in the last 30
seconds (an app that polls for incoming calls is therefore online while it
is open).

## `GET /v1/health`

```json
{"ok": true, "talk": true, "database": true, "phone": false}
```

`talk`: the server has a Barua key. `database`: its database answers.
`ok`: both. `phone`: phone calls look usable: there is a key, phone calls
are switched on, and upstream has not last answered a dial with
`no_caller_number` or `not_connected`. Needs no device id.

## `GET /v1/account`

What the Phone call screen shows around the call, and the prices.

```json
{"phone": {"enabled": false, "caller_id": "+255800110011"},
 "rates": {"phone_call": {"tzs": 300, "per": "minute", "text": "TSh 300 / minute"},
           "recording":  {"tzs": 30,  "per": "minute", "text": "TSh 30 / minute"}},
 "balance": {"state": "unknown", "tzs": null},
 "spent_tzs": 900}
```

- `phone.enabled` is health's `phone`. `phone.caller_id` is the number the
  other phone will see: the server's `BARUA_CALLER_ID`, else the `from` of
  the last call upstream placed, else null.
- `balance`: **upstream has no endpoint that tells the balance**, so `tzs`
  is always null for now. `state` is what the server has seen: `empty`
  after upstream answered a priced action with `402`, `ok` after a priced
  action went through since, `unknown` otherwise.
- `spent_tzs` is the sum of the charges upstream reported for the phone
  calls and recordings made through this server (all people).

## Rooms

One workspace, one set of rooms: every person sees every open room. A
person is a **member** of the rooms they made, posted in, opened
(`GET …/messages`, `POST …/read`) or took a ticket for; unread counts exist
for members only.

```json
{"name": "support-desk", "display_name": "Support desk",
 "created_by": {"identity": "neema", "name": "Neema"}, "mine": true,
 "member": true, "members": 3, "unread": 2,
 "last_message": {"id": 41, "from": "baraka", "from_name": "Baraka", "mine": false,
                  "text": "Karibu!", "created_at": "2026-11-15T07:00:30Z", "delivered": true},
 "active_at": "2026-11-15T07:00:30Z", "live": true,
 "recording": null,
 "created_at": "2026-11-15T07:00:00Z"}
```

- `name` is the room's address (lower case letters, digits, dashes, 2..63);
  it goes in the paths below.
- `mine`: this person made it. `members`: how many people are members.
- `unread`: messages by others after this person's read mark (0 for a
  non-member).
- `last_message` is null for a room nobody has written in.
- `active_at` is the latest of: the room was made, a message was posted, a
  ticket was taken. `live` is true when that was within the last 10
  minutes. (Upstream does not say who is connected to a room, so this is
  the nearest thing to "live" the server can know.)
- `recording` is null, or `{"id": "…", "status": "recording"}` while a
  recording of the room runs.

### `GET /v1/rooms`

```json
{"rooms": [ …room… ], "now": "2026-11-15T07:01:12Z"}
```

Open rooms, most recently active first, at most 100. Answered from the
database: polling it costs upstream nothing.

### `POST /v1/rooms`

```json
{"display_name": "Support desk"}
```

`display_name` is 2..60 characters. The address is derived from it
(`support-desk`). Returns `{"room": …room…}` with **201** when the room was
made and **200** when a room with that address already existed (that room
is returned, with its own display name): sending the same request twice is
harmless. Needs upstream (`503 talk_unavailable` without a key; nothing is
stored then). An optional `"name": "support-desk"` sets the address
explicitly.

### `GET /v1/rooms/{name}`

`{"room": …room…, "now": "…"}`.

### `DELETE /v1/rooms/{name}` (or `POST /v1/rooms/{name}/close`)

Closes the room upstream (everyone is disconnected) and deletes it and its
messages here. Only the person who made it may (`403 not_yours`).
`{"closed": true}`.

### `POST /v1/rooms/{name}/ticket`

```json
{"video": true}
```

`video` is optional (false: a voice-only ticket).

```json
{"token": "eyJhbGciOi…", "url": "wss://…", "identity": "neema", "room": "support-desk",
 "video": true, "expires_at": "2026-11-15T08:01:12Z"}
```

A LiveKit ticket for this person, good for joining for one hour. The app
joins the room's audio with it when its call screen opens. One upstream
request per call: ask when entering the room, not on a timer.

## Room chat

The server keeps the chat itself (upstream replays only what a server
said). Every posted message is also forwarded upstream to the room's
`"chat"` topic, so LiveKit clients connected to the room see it live.

```json
{"id": 43, "from": "neema", "from_name": "Neema", "mine": true,
 "text": "Habari za asubuhi", "created_at": "2026-11-15T07:01:00Z", "delivered": true}
```

`id` is an integer that only grows: a later message always has a larger
id, across all rooms. `delivered` says whether upstream took the forward;
false is not an error (the message is stored and every app sees it), it
only means LiveKit clients did not get it live.

### `GET /v1/rooms/{name}/messages`

- no query: the latest 50 messages, oldest first (opening the screen);
- `?after=43`: the messages with an id above 43, oldest first, at most 50
  (polling: pass the largest id seen);
- `?limit=N` (1..100) changes the 50.

```json
{"messages": [ …message… ], "last_id": 43, "more": false, "unread": 0, "now": "2026-11-15T07:01:12Z"}
```

`last_id` is the largest id in the room (0 for an empty room; pass it as
the next `after`). `more` is true when `after` was given and messages
beyond this page remain: ask again at once with the last id of the page.
`unread` is this person's unread count in the room. Reading makes the
person a member. Add `&read=1` to also move the read mark to the newest
message returned (what a chat screen that is showing the messages wants).
Answered from the database.

### `POST /v1/rooms/{name}/messages`

```json
{"text": "Habari za asubuhi", "client_id": "m-7f3a9c21"}
```

`text` is 1..2000 characters after trimming. `client_id` is optional
(8..64 characters of `[A-Za-z0-9_-]`): a message from the same person with
a `client_id` already stored is not stored again, the first is returned
(with 200), so a send that timed out can simply be repeated.

**201** `{"message": …message…}`. The author's read mark moves to it.

The message is stored first. If the forward upstream fails (no key,
upstream down, the minute's allowance used up) the answer is still 201,
with `"delivered": false`. A forward is tried once, never again.

### `POST /v1/rooms/{name}/read`

```json
{"up_to": 43}
```

Marks the room read up to that message id (left out: up to the newest).
The mark never moves backwards. `{"unread": 0}`.

## Messages between two people

A conversation between this person and one other, by identity. It is kept
by the server only: nothing is asked of upstream, so it costs nothing,
works without a Barua key, and is not sent through any LiveKit room (an
open screen learns of a new message by polling). Only the two people can
read it: every path means "my conversation with `{identity}`", so there is
no address for anyone else's. The shapes are those of room chat:

```json
{"id": 7, "from": "neema", "from_name": "Neema", "mine": true,
 "text": "Habari", "created_at": "2026-11-15T07:01:00Z", "delivered": true}
```

`id` only grows, across all conversations (it is a different series from
room messages' ids). `delivered` is always true: stored is delivered.

### `GET /v1/people/{identity}/messages`

As `GET /v1/rooms/{name}/messages`: no query for the latest 50, oldest
first; `?after=7` to poll; `?limit=N` (1..100); `&read=1` to move this
person's read mark to the newest message returned.

```json
{"peer": {"identity": "baraka", "name": "Baraka", "online": true},
 "messages": [ …message… ], "last_id": 7, "more": false, "unread": 0, "now": "2026-11-15T07:01:12Z"}
```

`404 not_found` when nobody has that identity; `400` for one's own.

### `POST /v1/people/{identity}/messages`

```json
{"text": "Habari", "client_id": "m-7f3a9c21"}
```

`text` and `client_id` as for a room. **201** `{"message": …message…}`;
the author's read mark moves to it. A `client_id` this person already used
returns that first message with 200, or `409 client_id_used` if it went to
someone else.

### `POST /v1/people/{identity}/read`

```json
{"up_to": 7}
```

Marks the conversation read up to that id (left out: the newest). Never
backwards. `{"unread": 0}`.

## In-app calls

One person calling another by identity, voice or video. Free.

```json
{"id": "f3b1c2d4", "direction": "outgoing", "video": true, "status": "ringing",
 "peer": {"identity": "baraka", "name": "Baraka"},
 "created_at": "2026-11-15T07:02:00Z", "answered_at": null, "ended_at": null,
 "seconds": 0, "recording": null}
```

- `direction` is from this person's side: `outgoing` or `incoming`. `peer`
  is the other person.
- `status`: `ringing`, `active` (answered), `declined`, `ended` (it was
  answered and is over), `missed` (it never was).
- `seconds`: talk time. While `active` it is counted by the server from
  `answered_at` to `now`; once `ended` it is upstream's figure.
- `recording`: null, or `{"id": "…", "status": "recording"}`.

### `POST /v1/calls`

```json
{"to": "baraka", "video": true}
```

`to` is a person's identity (`404 not_found` if nobody has it; calling
oneself is `400`). `video` is optional (false).

**201** `{"call": …call…, "token": "…", "url": "wss://…"}`: the caller's
ticket into the call's room.

A call that nobody answers is ended by the server after 45 seconds and
becomes `missed`.

### `GET /v1/calls/incoming`

```json
{"calls": [ …call… ], "now": "2026-11-15T07:02:03Z"}
```

The calls ringing for this person right now, oldest first (usually none or
one). **This is the poll that makes a phone ring: every 2 to 3 seconds
while the app is open.** It is answered from the database; see "What
polling costs" for when upstream is asked.

### `GET /v1/calls/{id}`

`{"call": …call…, "now": "…"}`. Either side polls this (every 2 seconds
while `ringing`, every 5 while `active`) to learn "answered yet?", "still
there?". Only the two people of the call may see it (`404` otherwise).

### `POST /v1/calls/{id}/accept`

**200** `{"call": …call…, "token": "…", "url": "wss://…"}`: `status` is
`active` and the ticket is the callee's. `403 not_your_call` for anyone but
the person it rings for; `409 not_ringing` if the caller gave up first (the
call in that state is what `GET /v1/calls/{id}` shows).

### `POST /v1/calls/{id}/decline`

**200** `{"call": …call…}` with `status` `declined`. Same errors.

### `DELETE /v1/calls/{id}` (or `POST /v1/calls/{id}/hangup`)

Either side, at any time. **200** `{"call": …call…}`: `ended` if it had
been answered, `missed` if not. Hanging up a call that is already over is
not an error: the call is returned as it is. A recording of the call that
is still running is stopped.

### `GET /v1/calls`

`{"calls": [ …call… ], "now": "…"}`: this person's calls, newest first, at
most 50. From the database.

## Phone calls: TSh 300 a minute

A call to a real phone number, paid from the workspace balance.

```json
{"id": "c81e728d", "to": "+255712345678", "caller_id": "+255800110011",
 "status": "ringing",
 "created_at": "2026-11-15T07:05:00Z", "started_at": null, "ended_at": null,
 "seconds": 0, "price_tzs": null,
 "rate": {"tzs": 300, "per": "minute", "text": "TSh 300 / minute"},
 "can_hang_up": false}
```

- `status`: `ringing`, `active`, `ended`, `missed` (not answered, busy,
  cancelled), `failed`.
- `seconds` and `price_tzs` are upstream's, written when the call ends
  (`price_tzs` null until then). While the call is `active` and upstream
  has given a `started_at`, `seconds` is counted by the server from it.
- `caller_id` is the number the other phone sees.
- `can_hang_up`: see hanging up, below.

### `POST /v1/phone-calls`

```json
{"to": "+255712345678", "request_id": "dial-5b21c9aa"}
```

`to` is E.164 (`+` and 8 to 15 digits). `request_id` is optional but the
app should always send one (8..64 characters of `[A-Za-z0-9_-]`, new for
each press of the dial button): a request from the same person with a
`request_id` already used returns the first call (**200**) instead of
dialling, and paying, again.

**201** `{"call": …phone call…, "token": "…", "url": "wss://…"}`.

Errors worth a sentence in the app: `402 balance_empty`,
`409 no_caller_number`, `409 call_in_progress` (this person's previous
phone call has not ended; it is one call at a time), `503 not_connected`,
`503 phone_unavailable`, `503 talk_unavailable`, and `502 upstream_failed`
("may or may not have been placed").

The server never dials twice for one request and never retries a dial.

### `GET /v1/phone-calls/{id}`

`{"call": …phone call…, "now": "…"}`. Poll every 3 to 5 seconds while the
status is `ringing` or `active`. Only the person who dialled may see it.

### `GET /v1/phone-calls`

`{"calls": [ …phone call… ], "now": "…"}`: this person's phone calls,
newest first, at most 50.

### `DELETE /v1/phone-calls/{id}` (or `POST /v1/phone-calls/{id}/hangup`)

**Upstream documents no way to hang up a phone call**, so by default the
server cannot end one: it sends upstream nothing, notes that this person
left the call, and answers

```json
{"call": …phone call…, "hung_up": false,
 "message": "The call ends when the other side hangs up."}
```

The call's `status` stays whatever upstream says, and it goes on costing
until upstream ends it. `can_hang_up` in the phone call is false in this
mode, so the app can word its button honestly.

If the server is started with `BARUA_PHONE_HANGUP=delete` (for an upstream
that answers `DELETE /calls/{id}`), it sends that, `can_hang_up` is true,
and the answer is `{"call": …, "hung_up": true}` with the call `ended`.

Because the app cannot yet join a call's audio, and a call cannot be ended
from here, **phone calls are off unless the server is started with
`PHONE_CALLS=on`** (`503 phone_unavailable` otherwise).

## Recordings: TSh 30 a minute

```json
{"id": "r9a2", "room": "support-desk", "call": null, "status": "recording",
 "started_by": {"identity": "neema", "name": "Neema"},
 "created_at": "2026-11-15T07:10:00Z", "ready_at": null,
 "seconds": 0, "size_bytes": null, "price_tzs": null, "url": null,
 "rate": {"tzs": 30, "per": "minute", "text": "TSh 30 / minute"}}
```

- `status`: `recording`, `processing`, `ready`, `failed`.
- Exactly one of `room` (a room's name) and `call` (an in-app call's id)
  is set.
- `seconds` while `recording` is counted by the server from `created_at`;
  afterwards it, `size_bytes` and `price_tzs` are upstream's, settled when
  the status becomes `ready`.
- `url`, once `ready`, is a download link that is good for one hour; the
  server fetches a fresh one when the one it holds is over 50 minutes old.

### `POST /v1/recordings`

`{"room": "support-desk"}` or `{"call": "f3b1c2d4"}` (a call that is
`active`, and this person's).

**201** `{"recording": …}`. If that room or call is already being recorded
the running recording is returned with **200** and nothing is started:
pressing twice does not pay twice. Errors: `402 balance_empty`,
`409 recording_unsupported`, `503 not_connected`, `503 talk_unavailable`.

The server stops a recording by itself when its call ends, and when it has
run for 60 minutes (`RECORDING_MAX_MINUTES`).

### `GET /v1/recordings/{id}`

`{"recording": …, "now": "…"}`. Poll every 10 seconds while `processing`
if the app wants the link. Visible to the members of the room, or the two
people of the call.

### `DELETE /v1/recordings/{id}` (or `POST /v1/recordings/{id}/stop`)

**200** `{"recording": …}` with `status` `processing`. Stopping one that
is not running returns it as it is.

### `GET /v1/recordings`

`{"recordings": [ … ], "now": "…"}`: the recordings this person started,
newest first, at most 50. `?room=support-desk` or `?call=f3b1c2d4`: the
recordings of that room or call instead (for a member or a participant).

## `GET /v1/activity`

The Home list: this person's rooms (those they are a member of), their
conversations with people, in-app calls and phone calls in one list,
newest first, at most 50
(`?limit=N`, 1..100).

```json
{"items": [
  {"kind": "room", "id": "support-desk", "title": "Support desk",
   "preview": "Baraka: Karibu!", "at": "2026-11-15T07:00:30Z",
   "unread": 2, "live": true, "status": "open",
   "direction": null, "video": null, "seconds": null, "peer": null},
  {"kind": "call", "id": "f3b1c2d4", "title": "Baraka",
   "preview": "Missed video call", "at": "2026-11-15T06:40:00Z",
   "unread": 0, "live": false, "status": "missed",
   "direction": "incoming", "video": true, "seconds": 0,
   "peer": {"identity": "baraka", "name": "Baraka"}},
  {"kind": "phone", "id": "c81e728d", "title": "+255712345678",
   "preview": "Phone call, 2 min 5 s", "at": "2026-11-15T06:10:00Z",
   "unread": 0, "live": false, "status": "ended",
   "direction": "outgoing", "video": false, "seconds": 125, "peer": null}
 ],
 "now": "2026-11-15T07:01:12Z"}
```

- `kind`: `room`, `person` (a conversation between two people: `id` is the
  other person's identity, `peer` is them, `status` is `open`, `live` is
  false, `at` is the last message's time, and `preview` is the last
  message, with `You: ` in front of one's own), `call` (in-app) or
  `phone`. `id` is what the endpoints of that kind take.
- `title`: the room's display name, the other person's name, the phone
  number. `preview`: one line the app can show as it is (the last message
  as `Name: text`, or `You: text`; a description of the call). The fields
  after it are there for an app that words it itself.
- `at`: a room's `active_at`; a call's `created_at`.
- `status`: `open` for a room, else the call's status. `live`: the room's
  `live`; for calls, `ringing` or `active`.
- `unread` is only ever non-zero for rooms and conversations.

Answered from the database.

## What polling costs

Upstream allows the whole workspace 60 requests a minute, so the server
answers polls from its own database and asks upstream only this much:

| The app polls | Suggested | Upstream requests it causes |
|---|---|---|
| `GET /v1/calls/incoming` | 2 to 3 s | None while nothing rings. |
| `GET /v1/calls/{id}` | 2 s ringing, 5 s active | For each call that is `ringing` or `active`, at most one `GET /app-calls/{id}` every 5 s (ringing) or 15 s (active), **shared by everyone polling it** (caller, callee, `incoming`). Accept, decline and hang-up made through this server are known at once without it; the refresh is only there to notice someone who closed their app. |
| `GET /v1/phone-calls/{id}` | 3 to 5 s | While any phone call has not ended, at most one `GET /calls` every 5 s for all of them together. |
| `GET /v1/recordings/{id}` | 10 s | At most one `GET /recordings/{id}` every 10 s per recording that is not `ready`. |
| `GET /v1/rooms`, `…/messages`, `/v1/activity`, `/v1/people`, `/v1/me`, `/v1/account` | 2 to 5 s where a screen shows them | None, ever. |

Actions (make a room, ticket, post, ring, accept, decline, hang up, dial,
record, stop) are one upstream request each.

The server keeps count. Of each minute's allowance it holds part back for
actions: refreshes stop first (polls are then answered with the state the
server has, which may be some seconds old), chat forwards next
(`"delivered": false`), and only then are actions refused, with
`429 talk_busy`. After upstream itself answers `429` the server sends
nothing more until the minute turns.

A device may send 240 requests a minute to this server.

## Notes for a client

- Allow 20 seconds for any request that is an action; polls answer at
  once.
- Nothing is pushed. A phone that is not polling `incoming` does not ring.
- Two phones can call each other once both have made any request (they
  then appear in each other's `GET /v1/people`).
- A priced action (`POST /v1/phone-calls`, `POST /v1/recordings`) must not
  be retried blindly after a timeout or a `502`: look at the list first, or
  repeat it with the same `request_id`.

## As implemented

What the server does where the contract above was silent, and where the
real Barua Talk API turned out to differ from its document (details in
`docs/live-notes.md`). No shape above was changed.

- **Recording an in-app call does not work against the real API today.**
  The real service's call object names no room, so
  `POST /v1/recordings {"call": …}` answers `409 recording_unsupported`.
  Recording a room works as described. The Video call screen's recording
  toggle should treat `recording_unsupported` as "not available".
- **Making a room and ringing a call work against the real service.**
  For part of 2026-11-15 it answered `500` to both (its room backend was
  not answering); later that day it answered `201`, and the opt-in live
  test (`BARUA_LIVE=1 node --test test/live.test.js`: a room made,
  ticketed, written in and closed; a call rung, accepted and hung up, and
  one declined) passed 3 of 3. Should upstream fail like that again, the
  app sees `502 upstream_failed` for `POST /v1/rooms` and
  `POST /v1/calls`, and nothing is stored.
- `POST` bodies may be left out where every field is optional
  (`…/ticket`, `…/read`, `…/accept`, `…/decline`, `…/hangup`, `…/close`,
  `…/stop`).
- `POST /v1/phone-calls` also reads the `request_id` from an
  `Idempotency-Key` header. A repeated request (200) carries
  `"token": null, "url": null`: the ticket is given once, with the 201.
  A `request_id` whose first attempt ended as `failed` still returns that
  failed call: a new press needs a new `request_id`.
- Hanging up an in-app call always succeeds for the person who hangs up,
  even when upstream cannot be told (it is then over here, and upstream
  ends it by its own rules).
- An upstream `403` other than `not_your_call` reaches the app as
  `403 not_yours`.
- A person's new name reaches other people's screens within 30 seconds
  (names are remembered that long).
- Besides answering polls, the server looks every 15 seconds for rings
  older than 45 seconds, for recordings to stop, and once a minute at live
  calls and phone calls nobody is polling; so `GET /v1/activity` does not
  show a call as live for ever when both phones have gone.
- With the database this example uses (far away: about 0.2 s a query), a
  poll takes 0.2 to 0.5 s and an action up to about a second plus
  upstream's time.
