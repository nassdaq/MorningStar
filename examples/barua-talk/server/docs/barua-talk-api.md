# Barua Talk API

The upstream API this server calls, as its owner documented it on
2026-11-15. Kept here so the server can be read without the website.

Real-time voice, video, screen share, chat, in-app calls, recordings, phone
calls and SMS — one REST API, one key, one balance, billed in Tanzanian
shillings.

Talk is built so the expensive part of your product is free: **rooms, video
minutes, chat and in-app calls cost nothing**. You pay only for what costs
us per unit — phone minutes, SMS, and recorded minutes.

```
Base URL   https://barua.tz/api/v1/talk
Auth       Authorization: Bearer barua_...
Content    application/json
```

## Authentication

Every request carries an API key from **Settings → API** in your Barua
workspace. Keys are scoped:

| Scope | Lets the key |
|---|---|
| `talk:read` | List and inspect rooms, calls, recordings, messages |
| `talk:write` | Create rooms, mint tickets, dial, record, send |

Keep the key on your server. Browsers and apps never see it — they receive
short-lived **tickets** (see Rooms) instead.

Requests are limited to **60 per minute per key**. Beyond that the API
answers `429` until the minute turns.

## Errors

Every error is JSON with a machine code and a human sentence:

```json
{ "error": { "code": "room_not_found", "message": "No room called desk-4. Create it first." } }
```

| Status | Meaning |
|---|---|
| `401 invalid_key` | Missing or unknown API key |
| `402 balance_empty` | The balance cannot cover this priced action |
| `403 insufficient_scope` | The key lacks the needed scope |
| `404 *_not_found` | No such room, call, recording or message |
| `409` | The thing is not in a state that allows this |
| `422 invalid_request` | The body failed validation; the message says which field |
| `429 rate_limited` | Over 60 requests this minute |
| `503 not_connected` | This capability is not provisioned for the platform yet |

---

## Rooms

A room is one address for everything live: voice, video, screen share and
chat. Create it from your server, then hand each participant a **ticket** —
a short-lived token their browser or app joins with. Rooms are free, however
long and however many people.

### Create a room

```
POST /rooms
```

```json
{ "name": "support-desk", "displayName": "Support desk", "identity": "neema", "video": true }
```

- `name` — the address: lower case letters, digits, dashes (2–63 chars).
  Creating the same name again returns the same room, not an error.
- `identity` — optional; include it and the response carries the creator's
  ticket, so one call both makes the room and gets you in.
- `video` — `false` mints a voice-only ticket; video includes screen share.

**201**

```json
{
  "room": { "name": "support-desk", "displayName": "Support desk", "createdAt": "2026-11-15T07:00:00Z" },
  "token": "eyJhbGciOi...",
  "url": "wss://..."
}
```

### List rooms — `GET /rooms` → `{ "rooms": [ { "name", "displayName", "createdAt" } ] }`

### Close a room — `DELETE /rooms/{name}`

Everyone is disconnected and the name frees. → `{ "room": { "name": "support-desk", "closed": true } }`

### Mint a ticket

```
POST /rooms/{name}/tokens
```

```json
{ "identity": "guest-7", "name": "Baraka", "video": true, "ttlSeconds": 3600 }
```

One ticket per participant, minted by your server at the moment they should
enter. `identity` is who they are inside the room; `name` is the label other
participants see; `ttlSeconds` (60–86400) is how long the ticket is valid
for joining. **200** → `{ "token", "url" }`.

### Joining from the browser

Tickets work with the standard [LiveKit client SDKs](https://docs.livekit.io/client-sdks/)
(web, iOS, Android, Flutter, React Native, Unity):

```js
import { Room } from "livekit-client";

const { token, url } = await fetch("/your-server/ticket").then(r => r.json());
const room = new Room();
await room.connect(url, token);
await room.localParticipant.setMicrophoneEnabled(true);
await room.localParticipant.setCameraEnabled(true);        // video tickets
await room.localParticipant.setScreenShareEnabled(true);   // screen share
```

### Chat

Participants chat through the client SDK with no API call at all: every
ticket can publish data, and messages travel on the `"chat"` data topic.

```js
room.localParticipant.publishData(
  new TextEncoder().encode(JSON.stringify({ from: "neema", text: "Karibu!" })),
  { topic: "chat", reliable: true },
);
room.on("dataReceived", (payload, participant, kind, topic) => {
  if (topic === "chat") console.log(JSON.parse(new TextDecoder().decode(payload)));
});
```

Your **server** can also speak into the room — a bot answering, an order
notice, "the doctor will join shortly":

```
POST /rooms/{name}/chat     { "from": "server", "text": "The doctor joins in 2 minutes." }
GET  /rooms/{name}/chat     → { "messages": [ { "id", "from", "text", "createdAt" } ] }
```

The POST lands on the same `"chat"` topic every client already listens to,
and is kept so GET can replay what the server said. Chat is free.

---

## In-app calls

One person in **your** app calling another, by identity — like WhatsApp
calling, but inside your product. Voice or video. Underneath it is a room,
so in-app calls are **free**.

### Ring someone

```
POST /app-calls
```

```json
{ "from": "neema", "to": "baraka", "video": true }
```

**201** — the caller's ticket arrives at once; they join and wait:

```json
{
  "call": { "id": "f3b1...", "from": "neema", "to": "baraka", "video": true, "status": "ringing" },
  "token": "eyJhbGciOi...",
  "url": "wss://..."
}
```

### Let the callee know

Delivering the ring is your app's job — your push notification or socket.
Between pushes, a device can simply poll:

```
GET /app-calls?identity=baraka&status=ringing
```

→ `{ "calls": [ { "id", "from", "to", "video", "status", "createdAt", ... } ] }`

### Answer or decline

```
POST /app-calls/{id}
```

```json
{ "action": "accept", "identity": "baraka" }
```

`accept` → **200** with the callee's `token` and `url` into the same room;
`decline` ends the call. Only the identity the call rings for may answer
(`403 not_your_call` otherwise); a call that is no longer ringing answers
`409 not_ringing`.

### Hang up — `DELETE /app-calls/{id}`

Either side. A call ends as `ended` (it was answered — `seconds` holds the
talk time) or `missed` (it never was). If someone just closes their app
instead of hanging up, the platform notices the departure and ends the call
the same way.

### Watch a call — `GET /app-calls/{id}` → `{ "call": { ..., "status": "ringing" | "active" | "declined" | "ended" | "missed" } }`

The caller polls this to learn "answered yet?".

---

## Recordings — TSh 30 per minute

LiveKit Egress composes the whole room — voices, cameras, screen shares —
into one MP4.

```
POST   /recordings          { "room": "support-desk" }      → 201 { "recording": { "id", "room", "status": "recording" } }
GET    /recordings          → { "recordings": [ { "id", "room", "status", "seconds", "sizeBytes", "priceTzs", "createdAt", "readyAt" } ] }
GET    /recordings/{id}     → the same fields plus "url" once ready
DELETE /recordings/{id}     stops a running recording → { "recording": { "id", "status": "processing" } }
```

`status` walks `recording → processing → ready`. Once `ready`,
`GET /recordings/{id}` returns a **signed download URL valid for one hour**;
ask again for a fresh one. Length, size and the charge are settled when
processing finishes. Starting a recording needs a positive balance (`402`
otherwise).

---

## Phone calls — TSh 300 per minute

A phone call is a room with a phone leg in it: your app talks to the phone
the way it talks to any participant.

```
POST /calls     { "to": "+255712345678", "from": "+255800110011", "identity": "agent-3" }
```

- `to` — E.164.
- `from` — optional; one of your account's voice numbers (your caller id).
  Omitted, your first voice number serves. No voice number on the account
  yet → `409 no_caller_number`.

**201** → `{ "call": { "id", "to", "from", "status": "ringing" }, "token", "url" }` —
join with the ticket and speak when they pick up.

```
GET /calls      → { "calls": [ { "id", "direction", "status", "from", "to", "startedAt", "endedAt", "seconds", "priceTzs" } ] }
```

Seconds and the charge are written when the call ends.

## SMS — TSh 40 per message

```
POST /messages     { "to": "+255712345678", "body": "Habari!", "from": "DUKA" }
```

`from` is a sender id (3–11 letters/digits) or one of your numbers. Send an
`Idempotency-Key` header and a replayed request returns the first send
instead of sending — and billing — twice.

**201** → `{ "id", "direction", "status", "from", "to", "body", "priceTzs", "createdAt", "deliveredAt", "error" }`

```
GET /messages      → { "messages": [ ...same shape... ] }
```

---

## Pricing

| What | Price |
|---|---|
| Rooms, video, voice, screen share | **Free** |
| Chat (client and server) | **Free** |
| In-app calls, voice or video | **Free** |
| Recording | TSh 30 / minute |
| Phone call | TSh 300 / minute |
| SMS | TSh 40 / message |

Priced actions draw from your workspace balance (**Settings → Payments**);
an empty balance answers `402 balance_empty` rather than ever surprising
you on an invoice.

## Try it without writing code

Your workspace has a live console at **barua.tz/talk/console**: every
endpoint as a button, the raw JSON of every response, and the room itself
joinable in the page.
