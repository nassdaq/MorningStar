# CallReady server API

The contract between the CallReady app (`examples/callready`) and its
server (`examples/callready/server`). The app holds no secrets: the
language model, the voice service and the database are reached only by the
server.

- Base URL: configurable in the app; `http://127.0.0.1:8787` in development
  (the phone reaches it with `adb reverse tcp:8787 tcp:8787`).
- All bodies are JSON (`Content-Type: application/json`) unless stated.
- Every request carries `X-Device-Id: <opaque id, 8..64 chars of [A-Za-z0-9_-]>`,
  the installation's identity. If the server is started with `APP_TOKEN`,
  every request must also carry `Authorization: Bearer <token>`.
- Errors: a non-2xx status with `{"error": {"code": "snake_case", "message": "for a person"}}`.
  Codes: `bad_request` 400, `unauthorized` 401, `not_found` 404,
  `call_ended` 409, `too_large` 413, `upstream_failed` 502 (the model or the
  voice service failed), `voice_unavailable` 503 (audio sent but no voice
  key configured).
- Enumerations: `scenario` is `missing_delivery`, `angry_customer`,
  `refund_request` or `billing_problem`; `accent` is `us`, `uk`,
  `australian` or `indian`; `difficulty` is `calm`, `standard` or `tough`;
  `minutes` is 2, 4 or 6.

## `GET /v1/health`

```json
{"ok": true, "model": true, "voice": false, "database": true}
```

`voice` is true when the server has a voice key: turns may then be sent as
audio and replies come with audio. When false the app runs calls in text.

## `GET /v1/profile` and `PUT /v1/profile`

```json
{"name": "Grace Mushi", "accent": "us", "difficulty": "standard", "minutes": 4,
 "plan": "free", "calls": 3, "average_score": 84, "streak_days": 3}
```

`PUT` takes any subset of `name` (1..24 chars), `accent`, `difficulty`,
`minutes`, `plan` (`free` or `premium`; a demo flag, no payment) and returns
the whole profile. A device seen for the first time gets a default profile
(`name` "", `calls` 0, `average_score` null, `streak_days` 0).

## `POST /v1/calls`

Starts a call. Body: `{"scenario": "angry_customer"}`; accent, difficulty
and minutes come from the profile unless given here.

```json
{"call_id": "c_9f2…", "number": 4, "scenario": "angry_customer",
 "title": "Angry customer", "caller_name": "Dana",
 "goal": "Let the caller be heard, then fix it",
 "accent": "us", "difficulty": "standard", "minutes": 4, "voice": false,
 "line": {"text": "This is the third time I’ve called today!", "audio_url": null},
 "tips": ["Take ownership", "Ask what happened"]}
```

`line` is the caller's opening line. `audio_url`, when not null, is a path
on this server (`/v1/audio/a_….mp3`) to fetch with the same headers.
`tips` are two short suggestions (at most 28 characters each) for what the
learner could do next; they are advice, not buttons with fixed answers.

## `POST /v1/calls/{call_id}/turns`

The learner's reply. Either JSON `{"text": "I’m sorry about that…"}`, or
the recorded audio itself as the body with its `Content-Type` (`audio/mp4`
from Android, also `audio/mpeg`, `audio/wav`, `audio/webm`; at most 5 MB),
which the server transcribes.

```json
{"heard": "I’m sorry about that, let me look into it.",
 "line": {"text": "Fine. But I want it fixed today.", "audio_url": null},
 "tips": ["Give a clear next step", "Confirm the details"],
 "turn": 2, "done": false}
```

`heard` is what the learner said (the transcript of the audio, or the text
sent). `done` is true when the caller has ended the conversation or the
call's time is used up; `line` is then the caller's closing line and the app
should end the call.

## `POST /v1/calls/{call_id}/end`

Ends the call (idempotent) and returns its report.

```json
{"call_id": "c_9f2…", "number": 4, "scenario": "angry_customer",
 "title": "Angry customer", "caller_name": "Dana",
 "started_at": "2026-11-12T09:14:03Z", "duration_seconds": 187,
 "score": 82,
 "skills": {"clarity": 88, "empathy": 74, "listening": 85,
            "problem_solving": 79, "professional_language": 84},
 "did_well": "You apologised early and kept a calm tone.",
 "try_next": "Ask for the order number before explaining.",
 "transcript": [
   {"speaker": "caller", "text": "This is the third time I’ve called today!"},
   {"speaker": "learner", "text": "I’m sorry about that, let me look into it.",
    "note": "Took ownership"}
 ]}
```

Scores are integers 0..100. `did_well` and `try_next` are one sentence each,
at most 90 characters. `note` on a learner's line is optional, at most 40
characters. A call ended with no learner turn has `score` null, `skills`
null and is not counted in the profile or the history.

## `GET /v1/calls/{call_id}`

The same report, for an ended call (404 while it is running or unknown, and
for another device's call).

## `GET /v1/calls`

The device's ended, scored calls, newest first, at most 50:

```json
{"calls": [{"call_id": "c_9f2…", "number": 4, "scenario": "angry_customer",
            "title": "Angry customer", "started_at": "2026-11-12T09:14:03Z",
            "duration_seconds": 187, "minutes": 4, "score": 82}]}
```

## `GET /v1/audio/{audio_id}.mp3`

The bytes of a line's speech, `Content-Type: audio/mpeg`. Kept for at least
ten minutes after it was made.

## As implemented: additions the client must handle

- `429 rate_limited` (with `Retry-After`) above 60 requests a minute per
  device; `500 internal_error` for unexpected failures.
- `GET /v1/health` needs no `X-Device-Id`.
- A turn's `text` is 1..600 characters after whitespace is collapsed, else
  400. A recording with no speech in it is 400 `bad_request`.
- After a turn returned `done: true`, further turns are `409 call_ended`
  even before `end` is called.
- A failed turn (502) stores nothing and may be resent; a failed `end`
  (502) leaves the call open and may be retried.
- An unscored report has fixed sentences in `did_well` and `try_next`.
- Audio is served only to the device whose call produced it.
- The opening line, caller name, title and goal of each scenario are those
  of the app's offline script (`src/script.rs`); starting a call makes no
  model request.
