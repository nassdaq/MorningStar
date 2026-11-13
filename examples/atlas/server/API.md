# Atlas server API

The contract between the Atlas app (`examples/atlas`) and its server
(`examples/atlas/server`). The app holds no secrets: the language model,
the voice service and the database are reached only by the server.

- Base URL: configurable in the app; `http://127.0.0.1:8788` in development
  (the phone reaches it with `adb reverse tcp:8788 tcp:8788`).
- All bodies are JSON (`Content-Type: application/json`) unless stated.
- Every request except `GET /v1/health` carries
  `X-Device-Id: <opaque id, 8..64 chars of [A-Za-z0-9_-]>`, the
  installation's identity. If the server is started with `APP_TOKEN`, every
  request must also carry `Authorization: Bearer <token>`.
- Errors: a non-2xx status with
  `{"error": {"code": "snake_case", "message": "for a person"}}`.
  Codes: `bad_request` 400, `unauthorized` 401, `not_found` 404 (also for a
  chat that belongs to another device), `too_large` 413, `rate_limited` 429
  (with `Retry-After`; 60 requests a minute per device), `internal_error`
  500, `upstream_failed` 502 (the model or the voice service failed),
  `voice_unavailable` 503 (audio sent but no voice key configured),
  `database_unavailable` 503 (the database cannot be reached just now; the
  server reconnects by itself, so the same request may simply be sent
  again).
- Enumerations: `model` is `auto`, `fast`, `smart` or `code` (the app's
  model picker); `model_used` is `fast`, `smart` or `code`; `plan` is
  `free`, `basic`, `plus` or `pro` (a demo flag: nothing is paid for).
- Limits: a JSON body is at most 64 KB; a message's `text` is 1..2000
  characters after trimming; at most 5 attachments, each `name` 1..60 and
  `kind` 1..20 characters; a recording is at most 5 MB.

## `GET /v1/health`

```json
{"ok": true, "model": true, "voice": false, "database": true}
```

`model` is true when the server has a model key, `database` when its
database answers, `ok` when both. `voice` is true when it has a voice key:
messages may then be sent as audio, and answers can be spoken.

## `GET /v1/profile` and `PUT /v1/profile`

```json
{"model": "auto", "plan": "free"}
```

`PUT` takes any subset of `model` and `plan` and returns the whole profile.
A device seen for the first time gets `auto` and `free`.

## `POST /v1/chats`

Starts a chat with its first message and returns the assistant's answer.

```json
{"text": "Compare the prices in these three quotes and find the cheapest.",
 "model": "auto",
 "attachments": [{"name": "quote-a.pdf", "kind": "pdf"}]}
```

`model` is optional (the profile's when left out). `attachments` is
optional and holds names only: no file is uploaded in this example, and the
assistant is told that it cannot open them.

```json
{"chat_id": "h_9f2c…", "title": "Comparing three quotes",
 "heard": "Compare the prices in these three quotes and find the cheapest.",
 "steps": [{"label": "Reading the request", "state": "done"},
           {"label": "Checking the attachments", "state": "done"},
           {"label": "Writing the answer", "state": "done"}],
 "answer": {"text": "I can’t open the attached files here: only their names reach me.\n\nPaste the totals and I’ll compare them:\n- Quote A\n- Quote B\n- Quote C",
            "audio_url": null},
 "model_used": "smart", "turn": 1}
```

- `chat_id` is `h_` and 32 hex digits. `title` is a short title for the
  chat (at most 40 characters), written by the model on the first turn and
  unchanged afterwards.
- `heard` is the message as the server took it: the text sent (white space
  trimmed), or the transcript of the audio.
- `steps` are 2 to 4 short labels (each at most 40 characters) for what the
  assistant did, in order. `state` is always `done` in a stored answer; the
  app plays them one after another before it shows the answer.
- `answer.text` is at most 4000 characters of plain text with this light
  formatting and nothing else: paragraphs separated by a blank line; list
  items as lines starting `- `; numbered items as lines starting `1. `,
  `2. ` …; `` `inline code` ``; and code blocks fenced by lines of three
  backticks (an optional language name after the opening fence). No
  headings, bold, italics, tables or links.
- `answer.audio_url`, when not null, is a path on this server
  (`/v1/audio/a_….mp3`) to fetch with the same headers. It is only ever set
  when the request carried `X-Speak: 1` and the server has a voice.
- `model_used` is the model that answered: the one asked for, or the one
  `auto` chose for this message.
- `turn` is the number of this exchange in the chat, from 1.

Instead of JSON the body may be a recording, with its own `Content-Type`
(`audio/mp4` from Android; also `audio/mpeg`, `audio/wav`, `audio/webm` and
the other types in `src/voice.js`), which the server transcribes. The model
is then named in the header `X-Model` (optional). A recording carries no
attachments.

With the header `X-Speak: 1` the answer is also spoken (`audio_url`). A
spoken answer that cannot be voiced still arrives, with `audio_url` null.

A request that fails stores nothing: no chat is created by a failed first
message, and it may be sent again.

## `POST /v1/chats/{chat_id}/messages`

The next message of a chat: `{"text": "…", "model": "fast"}` or a
recording, exactly as above (`attachments` are accepted here too). The
response has the same shape; `title` is the chat's existing title. The
model is sent the last 12 messages of the chat as context.

## `GET /v1/chats`

The device's chats, most recently updated first, at most 50:

```json
{"chats": [{"chat_id": "h_9f2c…", "title": "Comparing three quotes",
            "updated_at": "2026-11-13T09:14:03Z", "messages": 2,
            "state": "answered"}]}
```

`messages` counts the exchanges (a message and its answer are one).
`state` is `answered`: a chat is stored only once its first message has
been answered, and a failed message stores nothing.

## `GET /v1/chats/{chat_id}`

```json
{"chat_id": "h_9f2c…", "title": "Comparing three quotes",
 "updated_at": "2026-11-13T09:14:03Z",
 "messages": [
   {"role": "user", "text": "Compare the prices in these three quotes…",
    "attachments": [{"name": "quote-a.pdf", "kind": "pdf"}]},
   {"role": "assistant", "text": "I can’t open the attached files here…",
    "steps": [{"label": "Reading the request", "state": "done"}],
    "model_used": "smart"}
 ]}
```

Oldest first, at most the last 100 messages; they always come in pairs
(`user`, then `assistant`).

## `DELETE /v1/chats/{chat_id}`

Deletes the chat and its messages: `{"deleted": true}`. 404 for a chat that
does not exist or is another device's.

## `GET /v1/audio/{audio_id}.mp3`

The bytes of a spoken answer, `Content-Type: audio/mpeg`. Kept for at least
ten minutes after it was made, and served only to the device whose message
produced it.

## Notes for a client

- An answer can take a while: `smart` thinks before it answers. Allow 90
  seconds for a message.
- A request cannot be cancelled on the wire. A client that abandons a
  message ("Cancel task") should expect the exchange to be stored all the
  same if the server had already answered; it shows up when the chat is
  fetched again.
- Two messages for one chat are answered in the order they arrive.
- Only the first 600 characters of an answer are spoken, and code blocks
  are left out of the speech.
