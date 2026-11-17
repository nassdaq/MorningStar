# Effects and services

An application never does I/O itself. Work that takes time is asked for from `effects` as data, the host carries it out with the platform's own stack, and the result comes back as a message. This is what lets a test drive a whole app with no device: the test answers in the host's place.

## The pattern

`update` decides what should start and records it. `effects`, which is called after every `update`, queues the requests. Each request names the message its answer becomes.

{{snippet examples/docs-snippets/src/effects.rs#model}}

{{snippet examples/docs-snippets/src/effects.rs#update}}

## What there is

| Service | Ask with | Answer |
| --- | --- | --- |
| HTTP | `out.http(request, on_result)` | `Result<HttpResponse, HttpError>` |
| A value kept on the phone | `out.store(key, value)`, `out.remove(key)` | None. Read back at the next `setup`, or with `out.load(key, on_loaded)` |
| The clipboard | `out.set_clipboard(text)`, `out.clipboard(on_text)` | `Option<String>` |
| Recording | `out.start_recording(on_result)`, `out.stop_recording(on_result)` | `Result<(), AudioError>`, then `Result<Recording, AudioError>` |
| Playback | `out.play(clip, on_finished)`, `out.stop_playback()` | `Result<(), AudioError>` when it ends |
| A screen reader announcement | `out.announce(text)` | None |
| Moving a scroll view | `out.scroll(ScrollTo::..)` | None |
| A live room | `out.join_live(join, on_event)` and its controls | One message per event. See [Live rooms](live-rooms.md) |

Every method is in the [Effects reference](../reference/effects.md).

## HTTP

A request is built from a method and a URL, with headers, a body and a timeout. The response has a status, headers and a body, read as text with `response.text()`.

- Requests run on the platform's own HTTP stack with the platform's certificates.
- Plain `http://` is refused by the hosts except to the development machine: `localhost`, `127.0.0.1` and `10.0.2.2`.
- A response larger than 10 MB is an error.

{{variants crates/morningstar-core/src/services.rs HttpError}}

There is no JSON library in the framework. The examples read the few fields they need with a small reader of their own (`src/json.rs` in a generated app).

## Values kept on the phone

`out.store(key, value)` keeps a string under a key in the app's private storage. Stored values are handed to `setup` before the first frame, with no request and no waiting, which is the usual way to read them:

{{snippet examples/docs-snippets/src/effects.rs#setup}}

`setup.install_id()` is an id made once per installation. The examples send it to their servers as `X-Device-Id` to tell one phone from another without an account.

## Recording and playback

`start_recording` asks for the microphone the first time and answers when it is running, or with `AudioError::PermissionDenied`. A recording stops by itself after 60 seconds and when the app leaves the screen. `stop_recording` hands over what was recorded as a `Recording`, which becomes an `AudioClip` to send to a server or play back.

{{variants crates/morningstar-core/src/services.rs AudioError}}

## Where services work

| Service | Android | iOS | Preview on a Mac | A test |
| --- | --- | --- | --- | --- |
| HTTP, storage, clipboard | Yes | Written, never compiled | Yes | Answered by the test |
| Recording and playback | Yes | Written, never compiled | Answers `Unsupported` | Answered by the test |
| Live rooms | Yes, for apps that opt in | Answers `Unsupported` | Answers `Unsupported` | Events sent by the test |

"Yes" for Android means the code is there and the app builds with it. What has actually been observed on a phone is in [Platform status](../reference/platform-status.md). Where nothing can do the work at all (a screenshot program, `Ui::new` in a test) every request answers `Unsupported` at once, so an app never waits for ever.

The design is [ADR 0019](../architecture/0019-effects-and-platform-services.md).
