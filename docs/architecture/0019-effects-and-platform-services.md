# 0019 — Effects and platform services: HTTP, recording, playback and stored values

Status: Accepted (2026-11-12). Verified on the development host only. Nothing here has run on a device: the Rust type-checks for Android and iOS, the Java compiles against the API 36 SDK, and no request has been made, no sound recorded or played and no value stored on a phone. The Swift has never been compiled. What that leaves open is under "Not verified".

Adds to the UI layer of [ADR 0013](0013-ui-layer.md) the one thing an Elm-style application could not do at all: start work that finishes later. Uses the Rust-to-Java upcall of [ADR 0016](0016-text-input.md), the threading of [ADR 0004](0004-threading-and-frame-pacing.md), the thin hosts of [ADR 0003](0003-platform-hosts.md) and the dependency policy of [ADR 0007](0007-dependency-policy.md). It is also the first design of the "cross-thread wake-ups" that ADRs 0016 and 0018 left open: an answer from another thread wakes the render thread.

## Context

`update` changes the state and returns; `view` describes it. Nothing in that loop can wait, and nothing in it can touch the platform. So an application could not talk to a server, record the user's voice, play a reply, or remember its own identity between launches. The first real client (`examples/callready`) needs all four.

Each of them is something the platform already does well and a pure-Rust implementation would do worse or at great cost: TLS with the system's certificates, an AAC encoder, an audio output that respects phone calls, a private preference store. And each takes time, so none can happen on the render thread.

## Decision

### Requests are data; answers are messages

An application asks for work with a value and gets the result as a message:

```rust
// morningstar::ui::Application
fn effects(&mut self, out: &mut Effects<Self::Message>) {}
```

The runtime calls `effects` once after `setup` and once after every `update` (and after `back`). `update` records in the state that work is wanted; `effects` moves it into `out`:

```rust
out.http(HttpRequest::post(url).header("X-Device-Id", id).json_body(bytes), Message::Replied);
out.start_recording(Message::RecordingStarted);   // Result<(), AudioError>
out.stop_recording(Message::Recorded);            // Result<Recording, AudioError>
out.play(AudioClip::new(bytes, "audio/mpeg"), Message::Played);   // Result<(), AudioError>
out.stop_playback();
out.store("device_id", value);  out.remove("device_id");
out.load("device_id", Message::Loaded);           // Option<String>
out.forget_pending();                             // give up on everything asked so far
```

The last argument makes the message: an enum variant that holds the result, or a closure. Each request that has an answer gets exactly one, an error included; nothing fails by staying silent.

`effects` is a separate method, not a return value of `update`, so that every existing application compiles unchanged and `update` keeps its signature. It takes `&mut self` so it can `take()` what `update` recorded.

**Three layers, by the dependency direction of [ADR 0001](0001-crate-boundaries.md):**

| Crate | Holds |
| --- | --- |
| `morningstar-core` (`services.rs`) | The data: `HttpRequest`, `HttpResponse`, `HttpError`, `AudioClip`, `Recording`, `AudioError`, `ServiceRequest`, `ServiceResult`, `ServiceCall { id, request }`, `ServiceId`, `HostState`. No platform type, no thread, no callback. |
| `morningstar-runtime` | The plumbing. `App::service_requests(&mut Vec<ServiceCall>)` and `App::service_result(id, result) -> bool`, both defaulted; `Engine::take_service_requests` and `Engine::service_result`; `Engine::with_host_state`; `Setup::install_id` and `Setup::stored`. |
| `morningstar-ui` (`effects.rs`) | `Effects<M>`, the ids, the table of closures waiting for answers, and the test driver. |

A low-level `App` can use the services through the two runtime methods without the UI layer. Request ids are chosen by `Ui` (a counter from 1, never reused) and never shown to the application.

### When an answer is delivered

`Engine::service_result(id, result)` hands the answer to the application at once, on the render thread, between frames, exactly as `Engine::pointer` hands over a touch: `update` runs, then `effects`, and if the application was waiting for that id a frame is scheduled. An id nobody is waiting for (answered already, given up on with `forget_pending`, or from before the application was replaced) is dropped and schedules nothing.

The answer is **not held until a frame can be drawn**. An application that is hidden still gets its reply, moves its state on and may ask for the next thing (store what the server said); the frame comes when it is visible again, as after any input while hidden. Holding answers for a frame would make a chain of requests stop whenever the screen is off.

The cost: like a message from a tap, an answer clears the touch regions until the next frame is laid out, so a finger that lifts in the milliseconds between an answer and that frame does not complete its tap.

### What the application knows before its first frame

Waiting for a `load` to learn one's own device id means a first frame that cannot know it. So the host hands two things over at setup, synchronously:

- `Setup::stored(key)`: the values stored on earlier runs, as they were when the process started.
- `Setup::install_id()`: **a 128-bit random identifier as 32 lower-case hexadecimal digits**, generated by the host once per installation with the platform's secure random source (`SecureRandom` on Android, `SecRandomCopyBytes` on iOS) and kept in the app's private storage. It is the same on every launch, different on every installation, gone when the app's data is cleared, and identifies neither the device nor the user. This is how an application gets a random id without a random-number dependency. Without a host it is 32 zeros.

Both come from a `HostState` given to `Engine::with_host_state`; `Engine::new` passes an empty one.

### Hosts carry requests out

After creating the engine, and after every call into it, a host calls `Engine::take_service_requests`, which is the same rule as for `needs_frame` and the keyboard: requests only appear inside calls into the engine. The host does the work off the render thread and answers through `Engine::service_result`. A request it cannot carry out it answers immediately with `ServiceRequest::unsupported()`.

#### Android

`MorningstarServices.java` (new) does all of it; `MorningstarActivity` only tells it when the activity starts, stops and is destroyed, and passes on the permission result.

- **Out**: two more static methods looked up in `java.rs` next to `onTextInputCommand`: `onHttpRequest(long id, String method, String url, String headers, byte[] body, int timeoutMs)` and `onServiceCommand(int command, long id, String first, String second, byte[] data)` for the other six requests. Both return at once.
- **In**: three native methods, `nativeHttpResult`, `nativeAudioResult`, `nativeLoadResult`, callable from any thread. Each copies its arguments and posts `Message::Service` to the render thread.
- **Start**: `nativeInit(String installId, String[] stored)` now carries the host state. The JNI interface has 19 native methods and 4 upcalls.

Bytes cross as `byte[]`, copied with `GetByteArrayRegion` / `SetByteArrayRegion` into and out of buffers of exactly the stated length: no pointer into the Java heap is held and nothing needs releasing. Headers cross as one string of `Name: value` lines, which is lossless because `HttpRequest::header` removes CR and LF. Five JNI functions were added to the hand-declared table (17 in all), their positions checked against `jni.h` of NDK 27.1.

| Service | Implementation |
| --- | --- |
| HTTP | `HttpURLConnection` on a fixed pool of 4 daemon threads. Platform TLS, trust store, proxy and redirect handling. Connect and read timeouts are the request's timeout, and a watchdog disconnects at the same deadline so that the timeout bounds the whole exchange. The body is read up to 10 MB and the request fails as `TooLarge` beyond (checked against `Content-Length` first). A body is sent only with POST, PUT, PATCH, and DELETE when not empty: `HttpURLConnection` turns a GET with a body into a POST. |
| Recording | `MediaRecorder`, microphone source, AAC in an MPEG-4 container, mono, 16 kHz at 32 kbit/s, falling back to 44.1 kHz at 64 kbit/s if the device's encoder refuses the first; reported as `audio/mp4`. Written to one file in the cache directory, read into memory at stop and deleted. Maximum 60 s (`setMaxDuration`); a recording that reaches it, or is cut off by the activity stopping, is closed and kept for the `stop_recording` that follows. |
| Playback | `MediaPlayer` from a file in the cache directory (written on the service thread, deleted when the clip ends), `prepareAsync`, transient audio focus for the duration. One clip at a time. Ends as `Interrupted` on `stop_playback`, a new `play`, loss of audio focus, or the activity stopping; `Busy` if focus is refused. |
| Storage | `SharedPreferences` file `morningstar_store`, `apply()`. |
| Install id | `SharedPreferences` file `morningstar_host`, `SecureRandom`, `commit()`. |

#### iOS

The C interface gained `morningstar_ios_create_with_state`, a polled queue `morningstar_ios_take_service_request` (the host drains it after every call, as it drains `morningstar_ios_take_text_input`), and `morningstar_ios_http_result`, `morningstar_ios_audio_result`, `morningstar_ios_load_result`. A request's strings and bytes are lent until the next take. `HostServices.swift` (new) implements the services with `URLSession` (ephemeral configuration, no cookies, no cache, delegate on the main queue so that the 10 MB cap can stop a download), `AVAudioRecorder` (the same 16 kHz mono AAC in M4A), `AVAudioPlayer` and `UserDefaults`, and `project.yml` declares `NSMicrophoneUsageDescription`. **The Swift has been parsed and nothing more.**

#### No host

`Ui::new` under nothing (a screenshot tool, a test that does not care) has nobody to carry requests out. They are failed, at the start of the next frame, with the `Unsupported` error of their kind (`load` gives `None`), so an application shows its error state instead of waiting for ever. `Ui` knows a host is there from the host's first call to `service_requests`, which is why a host makes that call before the first frame.

### A test driver instead of a mock platform

`Ui::testing(app)` holds every request for the test:

```rust
let mut ui = Ui::testing(app);
ui.send(Message::Send);
assert_eq!(ui.pending_effects().len(), 1);                       // &[ServiceCall]
ui.answer_http(Ok(HttpResponse::new(200).with_body(reply)));     // the server replies
ui.answer_recording_started(Ok(()));
ui.answer_recording(Ok(Recording { bytes, mime: "audio/mp4".into(), duration }));
ui.answer_playback(Ok(()));
ui.resolve(id, ServiceResult::Http(Err(HttpError::Timeout)));    // any request, by id
```

Answering delivers the message immediately, like `Ui::send`. Stored values live in the driver's memory (`ui.stored(key)`, `ui.set_stored(key, value)`), so `store` and `load` need no answering; playback behaves as on a device (`stop_playback` or a second `play` ends the pending clip as `Interrupted`). `Ui::with_services(app, |request| ...)` adds a closure that answers what it wants to and leaves the rest pending. Answers that do not come from the test's own call (a `load`, the closure's) are delivered at the start of the next frame, never inside the `update` that asked, so the "loading" state in between can be asserted; `ui.deliver_effects()` delivers them without a frame.

## Alternatives considered

**A Rust HTTP and TLS stack** (`ureq` or `reqwest` over `rustls`, on a thread of our own). Rejected:

- It is the largest dependency the project would have: `rustls` with a crypto provider (`ring` or `aws-lc-rs`, C and assembly that must cross-compile for every Android ABI and iOS), `webpki`, and a URL and HTTP library, against a policy ([ADR 0007](0007-dependency-policy.md)) that admits a dependency only where writing or obtaining the equivalent is a significant cost. Here the equivalent is already on the device.
- **Trust.** A Rust TLS stack does not see the system's certificate store by itself. It either ships a frozen copy of Mozilla's roots (which stops trusting what the user's device trusts, and needs an app update to revoke a root) or reaches the platform verifier through more JNI and Objective-C than this ADR adds in total.
- **Policy.** Android's network security config and iOS's App Transport Security, per-app VPNs, proxies and captive-portal handling apply to the platform stack and not to a socket opened from Rust. The cleartext rule below would have to be reimplemented and could be got wrong.
- Binary size, on a framework whose library is currently dominated by the GPU stack.

What it would have bought is identical behaviour on both platforms and requests in host tests. The first is not needed for requests this simple; the second is what the test driver is for.

**Audio in Rust** (`cpal` plus an encoder and decoders). The same argument, more strongly: AAC encoding and MP3 decoding are on the device, patent-cleared and hardware-backed.

**`update` returns a command** (`fn update(&mut self, m) -> Command<M>`, as in Elm and iced). The better shape in a new API; here it changes the signature every existing application implements. A defaulted second method costs existing code nothing.

**A `Subscription`-style stream of events** for recording levels or download progress. No need yet; see Limitations.

**A blocking question to the host for stored values** at setup (a JNI call from the render thread that returns the value). Works on Android, but the render thread would block on disk; handing over a snapshot at start costs one small file read on the main thread instead, once per process.

**Delivering answers at the start of the next frame.** Simpler to state, and the same on screen, but it ties progress to visibility (above).

**Storage as a file written from Rust** given a directory by the host. Less interface, but it puts file I/O on the render thread or needs a thread of our own, and loses what the platforms' stores do about atomic writes.

## Threading

- **Render thread** (Android) / main thread (iOS): everything the application sees. `effects`, `update` with an answer, `view`. Never blocks on a service.
- **Android HTTP pool**: 4 daemon threads, one request each; a fifth request waits for a thread. Plus one timer thread for deadlines.
- **Android service thread**: one `HandlerThread` for recording, playback, storage and their file I/O, in request order, which is what makes a `load` after a `store` see the new value. `MediaRecorder` and `MediaPlayer` are created there, so their callbacks arrive there.
- **Android main thread**: only `requestPermissions`, its result, and reading the host state at start.
- An answer crosses to the render thread through the existing message channel (`mpsc::Sender` is usable from any thread) and wakes it if it is blocked. No lock is shared between threads on the Rust side.
- **iOS**: `URLSession` and the audio classes work on their own threads; `HostServices` itself runs on the main thread, where the engine lives, and answers on a later turn of the run loop, never inside the call that drained the request.

## Lifecycle

- **Android's engine lives as long as the process** ([ADR 0004](0004-threading-and-frame-pacing.md)), not the activity. `MorningstarServices` holds the application context and never an activity; it looks one up only to ask for the microphone. So activity recreation loses nothing: requests in flight complete and are delivered to the same application. A second `nativeInit` in a live process is ignored.
- **The activity stops** (`onStop`): the playing clip ends as `Interrupted`; a running recording is closed and kept; `play` and `start_recording` while off screen fail at once (`Interrupted`, and `Failed`). HTTP continues and its answer is delivered to the hidden application.
- **The activity is destroyed while the microphone question is up**: the pending start fails.
- **Nothing arrives after teardown.** Answers are matched by id against the closures `Ui` holds; when the `Ui` is dropped the closures are dropped uncalled. On iOS, where the engine dies with the view controller, `HostServices` is shut down first (session invalidated, players stopped) and every answer goes through a weak reference to the controller, which runs it only while it has an engine.
- **Temporary files** (`morningstar-recording.m4a`, `morningstar-clip-N.*` in the cache directory) are deleted when their request ends, and any left by a crash at the next start.

## Security

- **Cleartext HTTP is refused**, by the platform, not by a check of ours: `res/xml/network_security_config.xml` sets `cleartextTrafficPermitted="false"` as the base and allows only `127.0.0.1`, `localhost` and `10.0.2.2`, which all mean the developer's own machine (`adb reverse`, and the emulator's name for its host) and cannot name another machine on a network. The exception is in release builds too; it does not weaken them. A refused request fails as `HttpError::Failed` with the platform's wording. On iOS, App Transport Security with one exception domain, `localhost` (an IP address cannot be an exception domain there).
- **Certificates**: the system's authorities. User-installed authorities are not trusted (the platform default from API 24). There is no pinning.
- **Header injection**: `HttpRequest::header` strips CR and LF, so a value taken from user input cannot add a header.
- **Permissions**: the manifest declares `INTERNET` and `RECORD_AUDIO` (and the microphone as an optional feature). The microphone is asked for the first time the application starts a recording, never at launch. Denial is `AudioError::PermissionDenied`. Every later start asks again; once the user has refused for good Android answers "denied" without a dialog, which is the same error. An application that uses neither should remove the declarations: a store listing shows them.
- **What is stored where**: the application's values in `SharedPreferences` (`morningstar_store`) / `UserDefaults`, private to the app and not encrypted beyond what the platform does for all app data; the manifest has `allowBackup="false"`, so they do not leave the device in a backup. Not a place for a password or a long-lived token of value: that needs the Keystore / Keychain, which is not part of this record. Recordings and clips exist as files in the app's private cache directory only while their request runs.
- **Logs**: the render thread logs each request and answer by kind, id and size. URLs, headers, bodies, keys and values are never logged.

## Consequences

- The host interfaces grew, both halves together (above). **An application library and a host built from different sides of this change do not work together**: `nativeInit` has the same symbol and different arguments.
- `Application` has one more defaulted method, `App` two. The examples compile untouched.
- Every Morningstar APK now declares two permissions and a network security config, whether or not the application uses them.
- `Ui::frame` does one more check per frame (is anything ready to deliver), and each `update` is followed by one call to `effects`, which for an application that asks for nothing is an empty default.
- A request's bytes are copied at each boundary: into the Java array, and back. For the sizes involved (a 60 s recording is about 240 kB, a spoken reply some hundreds of kB) that is not measured and not expected to matter; the 10 MB cap bounds it.

## Limitations

- **No streaming, progress, cookies or multipart helpers.** A body is bytes in and bytes out, whole. No redirect policy beyond the platform's.
- **A request cannot be cancelled on the wire.** `forget_pending` drops answers; the work still finishes. An application that needs to ignore one late reply puts a sequence number in the closure.
- **The body of a GET or HEAD is not sent.**
- **One recording and one clip at a time.** No levels while recording, no pause, no seek, no playback position.
- **Recording stops when the app leaves the screen**, and cannot start while it is off screen.
- **`Setup::stored` is a snapshot** from process start; it does not see what the running application stores.
- **Keys and values are strings.** On iOS a NUL inside one is removed.
- **`Busy` for the microphone** is only reported when this application is already recording. Another app holding the microphone surfaces as `Failed` with the platform's text.
- **The fallback sample rate** means a recording's rate is 16 kHz or 44.1 kHz; it is not reported, the container says.
- **iOS asks for the microphone once.** After a refusal only Settings changes it.

## Verified

On the development host (macOS) only:

- Data types: 8 tests in `crates/morningstar-core/src/services.rs`.
- Engine: 12 tests in `crates/morningstar-runtime/src/engine.rs`. Requests reach the host once and in order; an awaited answer produces exactly one frame and the engine is idle before and after; an unawaited or repeated answer changes nothing; answers arrive in the host's order; an answer while hidden is delivered at once and drawn on return; a request made in response is there to take; setup sees the host state.
- UI layer: 42 tests in `crates/morningstar-ui/tests/effects.rs`. When `effects` is called; ordering and ids; each answer kind and each error; the wrong kind of answer is a failure, not a panic; storage round trip, replace, remove, seeding; playback interruption; the stand-in host and its frame-start delivery; `Unsupported` without a host; the `App` path a real host uses, including that a frame neither fails nor answers a request in flight; `forget_pending`; closures dropped uncalled at teardown and called exactly once otherwise; a `Ui` under a real `Engine` from setup to answer.
- `examples/services-check`: 9 tests script the server, the microphone and the speaker against a complete application, and a tenth is the example of `docs/building-screens.md` compiled and run; its screen was rendered and looked at.
- `cargo test --workspace`: 767 tests pass; `cargo clippy --workspace --all-targets -- -D warnings` is clean.
- `cargo ndk -t arm64-v8a clippy -p services-check -- -D warnings` (NDK 27.1) and `cargo clippy --target aarch64-apple-ios -p morningstar-ios -- -D warnings` are clean.
- The three Java files compile with `javac -Xlint:all -Werror` against `android-36/android.jar`. The JNI descriptors of the two new upcalls (`javap -s`) and the parameter lists of the four changed or new native methods (`javac -h`) were compared with the Rust side.
- `morningstar_ios.h` passes `cc -fsyntax-only -Wall -Wextra`. The Swift passes `swiftc -parse` (syntax only).

## Not verified

- **Everything on an Android device.** No APK was built or installed for this record. Unseen on hardware: that the upcalls are found and a request leaves the phone; TLS to a real server; the mapping of failures to `Offline` / `Timeout` (airplane mode, a dead port, a slow server); the cleartext rule, both that `http://127.0.0.1` through `adb reverse` works and that another `http://` is refused; the 10 MB cap; the permission dialog, denial, and "don't ask again"; that 16 kHz AAC is accepted by the device's encoder and what the file sounds like; the 60 s limit; MP3 and M4A playback, audio focus against a phone call or a music app; behaviour across `onStop` and activity recreation; that `SharedPreferences` values and the install id survive a restart and reach `setup`.
- **The APK build**: the manifest with its new permissions and `networkSecurityConfig`, and the new `res/xml` directory, have not been through Gradle or lint.
- **Android API 24 to 35.** The code paths for API < 26 (audio focus) and < 31 (`MediaRecorder` constructor) are compiled and unrun.
- **Everything on iOS**, beyond the Rust half type-checking. The Swift has never been compiled; the `info:` section of `project.yml` (App Transport Security through a generated `Info.plist` merged with `GENERATE_INFOPLIST_FILE`) is written from XcodeGen's documentation.
- **Cost**: the copies at the JNI boundary, the main-thread read of the host state at start, and the wake-up latency of an answer were not measured.
