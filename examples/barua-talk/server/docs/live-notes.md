# Barua Talk API: what was confirmed against the real service

Notes from 2026-11-15, taken with a real key against
`https://barua.tz/api/v1/talk`, beside the owner's document
(`barua-talk-api.md`). Only free endpoints were ever called. **No priced
endpoint was called for real**: no `POST /calls`, no `POST /messages`, no
`POST /recordings`. 24 requests in all, spread over about half an hour.
Everything was named `mstest-…`; nothing was left behind (no room was ever
created, see below; the calls ended by themselves as `missed`).

To repeat: `BARUA_LIVE=1 node --test test/live.test.js` (see the README).

## Confirmed, as documented

| Request | Answer |
|---|---|
| `GET /rooms` | `200 {"rooms": []}` |
| `GET /app-calls?identity=…&status=ringing` | `200 {"calls": []}` |
| `GET /app-calls/{unknown id}` | `404 {"error": {"code": "call_not_found", "message": "No such call."}}` |
| `GET /calls` | `200 {"calls": []}` |
| `GET /recordings` | `200 {"recordings": []}` |
| `POST /rooms` with a bad `name` | `422 invalid_request`, "lower case letters, digits and dashes" |
| `POST /rooms/{unknown}/tokens`, `POST` and `GET /rooms/{unknown}/chat`, `DELETE /rooms/{unknown}` | `404 room_not_found` |

So: the key, the base URL, the bearer header, the error shape
(`{"error": {"code", "message"}}`), `application/json`, and the 404 and 422
paths are as the document says. No rate-limit headers are sent on ordinary
answers.

## Where the real service differed

1. **`POST /rooms` answers `500` with an empty body.** Every attempt (five,
   with and without `displayName`, `identity` and `video`, over half an hour).
   No room appears in `GET /rooms` afterwards. So no room could be made,
   and nothing that needs a room could be tried live: tickets, room chat,
   closing a room.

2. **`POST /app-calls` answers `500` with an empty body (both attempts),
   and yet makes the call.** `GET /app-calls?identity=<callee>` a moment later lists it,
   already over:

   ```json
   {"id": "b50fac6d-e761-4140-901f-f8f03679c873", "from": "mstest-a", "to": "mstest-b",
    "video": false, "status": "missed",
    "createdAt": "2026-11-15T05:42:38.120Z", "answeredAt": null,
    "endedAt": "2026-11-15T05:42:39.614Z", "seconds": 0}
   ```

   It looks as though the call row is written, then making its room (or
   its ticket) fails, and the call is closed as `missed` a second later.
   Both 500s point at the same place: whatever stands behind rooms
   (LiveKit) is not answering for this workspace. The document's own word
   for that is `503 not_connected`; the service says a bare 500 instead.
   So accept, decline, hang-up and watching a live call could not be tried
   live either.

3. **The shape of an in-app call**, seen in that answer: `id` is a UUID;
   times carry milliseconds; the fields are `id, from, to, video, status,
   createdAt, answeredAt, endedAt, seconds`. **There is no field naming
   the call's room.** The document's "`...`" hid nothing more.

4. **`GET /app-calls?identity=X` lists the calls ringing *for* X** (X is
   `to`). Asked with the caller's identity it returned nothing.

## What this server does about each

1. and 2. A 500 without the error shape is "no usable answer"
   (`TalkError` kind `network`): the app gets `502 upstream_failed`,
   nothing is stored, nothing is retried. Seeing that a 500 can hide a
   request that *did* take effect is exactly why a dial or a recording
   start that gets no usable answer is never sent again (README, "Money").
3. Recording an in-app call needs the name of its room, and the real
   service gives none. Against the real API `POST /v1/recordings
   {"call": …}` therefore answers `409 recording_unsupported`, and only
   rooms can be recorded. (The server uses a `room` field if upstream ever
   sends one; the fake sends it, so the path is tested.)
4. The server never uses that list: it keeps its own record of who was
   rung, and asks upstream about one call at a time.

## Not confirmed live

Everything below has only run against the fake (`test/fakes.js`), which
was written from the document:

- making, listing and closing a room; tickets; the room chat forward;
- an in-app call that actually rings: accept, decline, hang up, watching,
  and what `DELETE /app-calls/{id}` returns;
- phone calls, recordings, SMS (priced: deliberately never called);
- `429` and how long it lasts; `402`; `409 no_caller_number`;
  `503 not_connected`;
- the words upstream uses for a phone call's status after `ringing` (the
  document names only that one; `routes.phoneStatus` accepts the usual
  ones).

## What the app now asks of a ticket (2026-11-15)

Nothing here changed on the server, and none of it has been seen live:

- **Video.** The app turns the camera on and shares the screen with the
  ticket it is given. The server already passes `video` upstream for a
  room's ticket and for an in-app call, and the document says a
  `video: false` ticket is voice only and that a video ticket includes
  screen share. So a voice call's camera should be refused by the room,
  which the app shows as "The camera could not be started."
- **Chat.** The app does not publish to the `"chat"` topic itself. It
  relies on the document's statement that `POST /rooms/{name}/chat` "lands
  on the same chat topic every client already listens to": when data
  arrives on that topic, the app asks this server for the room's messages
  at once. If upstream does not in fact forward, nothing breaks: the
  five-second poll still brings the message.
