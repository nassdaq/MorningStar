# 0038 — Live rooms: voice through LiveKit as an opt-in platform service

Status: Accepted (2026-11-15). Verified on the development host only, and by building two APKs and reading them back. **Nothing here has run on a device and no room has been joined**: the Rust type-checks for Android and iOS, the Android host (Java and one Kotlin file) compiles and packages against LiveKit 2.30.0, and the server that hands out tickets could not be used (it answered 500 to creating rooms and calls while this was written). The Swift has never been compiled, and on iOS the service is an honest refusal. What that leaves open is under "Not verified".

Extends [0019](0019-effects-and-platform-services.md) (requests as data, answers as messages, hosts that carry them out, a test driver in place of a host) with its first service that is answered more than once, and [0020](0020-app-identity-and-launcher-icons.md) (an app declares itself in its `Cargo.toml`) with the first thing an app declares that changes what its host is built from. Uses the dependency policy of [0007](0007-dependency-policy.md).

## Context

Barua Talk (`examples/barua-talk`) rings, answers, chats and dials through its server, and for every room and call the server hands the app a LiveKit ticket: a `wss://` address and a token. The framework had nothing to join with, so a connected call showed "Live audio and video need the next version" and carried no sound.

A media client is the opposite of the services of ADR 0019 in three ways:

- **It does not finish.** A request and one answer cannot say "someone came in", "the connection dropped and came back", "the other side ended it".
- **It holds the microphone open** for as long as it lasts. An HTTP request that is forgotten wastes a reply; a room that is forgotten is a live microphone nobody knows about.
- **It is large.** WebRTC is tens of megabytes of native code. ADR 0019 could say that every APK declares two permissions it may not use; it cannot be said that every APK carries WebRTC.

## Decision

### The surface

One live room at a time, voice only. The data is in `morningstar-core` (`services.rs`), the plumbing in `morningstar-runtime` is unchanged, and the API is on `Effects` in `morningstar-ui`:

```rust
out.join_live(
    LiveJoin::new(url, token)       // the ticket; microphone live, sound in the earpiece
        .microphone(false)          // join muted
        .speaker(true)              // sound in the loudspeaker
        .report_speaking(true),     // ask for LiveEvent::Speaking
    Message::Live,                  // called for every event of the room
);
out.set_live_microphone(false);     // mute; true opens it again
out.set_live_speaker(true);         // loudspeaker; false is the earpiece
out.leave_live();
```

`Message::Live` receives each `LiveEvent`, in order:

| Event | Meaning |
| --- | --- |
| `Connected` | The room is joined; the others are heard, and hear this device unless it joined muted. |
| `ParticipantJoined(LiveParticipant { identity, name })` | Someone is in the room: sent for everyone already there, right after `Connected`, and for whoever comes in later. |
| `ParticipantLeft(identity)` | Someone left. |
| `Speaking(Vec<identity>)` | Who is speaking, loudest first; empty when nobody is. Only if the join asked for it. |
| `Reconnecting`, `Reconnected` | The connection dropped and is being made again; it is back. |
| `Disconnected(LiveDisconnect)` | **Final.** `Left` (the application left), `Replaced` (it joined another room), `Hidden` (the app left the screen), `Ended` (closed, removed or duplicated from the other side), `Lost` (the connection could not be kept). |
| `Failed(LiveError)` | **Final.** `PermissionDenied` (the microphone), `Unsupported` (nothing here can join: see below), `Failed(text)` (anything else, in the platform's words, for logs). |

The rules an application can rely on:

- **Every join ends with exactly one final event**, and nothing follows it. A join never fails by staying silent. When the final event arrives the room has been released by the host: the microphone is closed.
- **One room at a time.** A second join ends the first with `Replaced`.
- **The microphone is asked for at the join**, also when joining muted, so that unmuting never has to stop for a question. Refusal is `PermissionDenied` and the room is not joined.
- **Leaving, muting and the loudspeaker are not answered.** Leaving is confirmed by the join's own `Disconnected(Left)`. A host that cannot mute ends the room with `Failed` rather than leave the microphone open behind a button that says it is closed.
- **`Unsupported`** is the answer where nothing can join: no host (a screenshot tool, `Ui::new` in a test), an app built without the service (below), and iOS for now.

`LiveJoin`'s `Debug` prints neither the token nor the address, so neither can reach a log or a failed assertion's output through `{request:?}`; a test in `morningstar-core` holds that.

### A request that is answered more than once

`ServiceRequest::LiveJoin` is answered with `ServiceResult::Live(LiveEvent)` under the join's id, as often as there are events. Nothing in `morningstar-runtime` had to change: `Engine::service_result(id, result)` already hands whatever it is given to the application and schedules a frame if the application took it. `Ui` keeps the room's handler (an `FnMut`, where every other request's is an `FnOnce`) until the final event, then drops it; an event that arrives later under that id finds nobody and is dropped, exactly as a second answer to an HTTP request is.

The alternative was a general subscription mechanism (a `Subscription` trait, streams of messages). It was not built, for the reason ADR 0019 gives for not building it then: there is one user. The table of handlers has one more map, and the rule "a join is answered until its final event" is the whole of the generalisation. A second streaming service (download progress, recording levels) can take the same shape or prompt the general one.

Each event is a message and a frame, like a tap. Speaking reports are the only events that come at a rate, which is why they are off unless asked for.

### Lifecycle: a room is never connected by accident

The microphone is the reason for every rule here. Each is tested where it can be (`crates/morningstar-ui/tests/live.rs`, `crates/morningstar-runtime/src/engine.rs`, `examples/barua-talk/tests/sound.rs`); the host's half compiles and is unrun.

| What happens | What the room does | Where |
| --- | --- | --- |
| The app leaves the screen (`onStop`: Home, another app, the screen locked) | The host leaves the room and the application is told `Disconnected(Hidden)`, at once, while hidden. It may join again when it is back. | `MorningstarLive.activityStopped`; delivery while hidden tested in `engine.rs` |
| A join arrives while the app is off screen | Not joined: `Disconnected(Hidden)`. | `MorningstarLive.join` |
| The surface is destroyed | Nothing by itself: on Android that happens with `onStop`, which is the rule above; rotation is a surface change, not a recreation (ADR 0003's `configChanges`). | |
| Back | The application's decision, like every Back (ADR 0018). The framework leaves nothing by itself; Barua Talk ends the call, which leaves the room. | `examples/barua-talk/tests/sound.rs` |
| `forget_pending` | **Leaves the room**, as well as dropping its events. Everywhere else `forget_pending` only stops listening; a room nobody listens to would stay connected. | `EffectState::forget`, tested with the driver and as a host sees it |
| The application is replaced or dropped (`Ui` dropped) | The handler is dropped uncalled, like every other. The host is the one that knows: the Android render thread sends a leave when its loop ends, before the engine goes; `HostServices.shutDown` is the place on iOS. | `render_thread.rs`; iOS has no room to leave yet |
| The process dies | The socket and the audio device die with it. | The operating system |
| The user refuses the microphone | Not joined: `Failed(PermissionDenied)`. | |
| Muting fails | The room ends with `Failed`. | `MorningstarLive.setMicrophone` |

**A room does not continue in the background.** Android silences the microphone of a background app and may freeze its process unless a foreground service with a notification holds it; without one, a call that "continues" behind another app is a call that is silently broken, and until it breaks the user is heard from an app they cannot see. So this version ends the room, says so (`Hidden`), and lets the application join again on return, which Barua Talk does by itself. While a room is joined the host keeps the screen on (`FLAG_KEEP_SCREEN_ON`), since a screen that times out is the app leaving the screen. A call that survives the background needs a foreground service of type `microphone` and its notification; that is the next record, not this one.

### Opt-in per app

An app that joins live rooms says so where it says its name (ADR 0020):

```toml
[package.metadata.morningstar]
name = "Barua Talk"
services = ["live"]
```

- `morningstar-icon <package> --print services` prints the list, comma-separated, an empty line for none (`identity.rs`: a list of known names; an unknown name, or anything but a list of strings, is an error that names the key and lists the services).
- `tools/build-android.sh` reads it with the id, the name and the library and passes `-Pmorningstar.services=live` to Gradle. No script knows any app by name.
- `templates/android/app/build.gradle.kts` compiles `src/live/java` (the Kotlin `MorningstarLive`, which is LiveKit) **or** `src/nolive/java` (a Java `MorningstarLive` with the same static methods that answers every join `Unsupported` and logs why), and adds the dependency only in the first case. `settings.gradle.kts` adds the JitPack repository, for the one group that needs it, only in that case too. An unknown service name fails the build.
- So exactly one `MorningstarLive` is in any APK, `MorningstarNative.onServiceCommand` calls it without knowing which, and an app that does not declare the service carries no LiveKit, no WebRTC and no Kotlin.

Two smaller things came with making this true, both in `build.gradle.kts`:

- **ABIs.** LiveKit ships WebRTC for four ABIs. The host now packages native libraries only for the ABIs the app's Rust library was built for (the directories under `jniLibs`); otherwise an arm64 app carried three WebRTC builds that could never run (74 MB instead of 34).
- **The Kotlin standard library.** The Android Gradle plugin 9 compiles Kotlin by itself and adds the standard library to every app, used or not (2.4 MB of classes, in every Morningstar APK until now). It is excluded when the service is not declared.

`tools/build-android.sh` also deletes the previous APK before packaging: Gradle rewrites it in place, and an APK that was larger before keeps that size as dead space, which made every size comparison wrong.

The alternative, product flavours (`live` and `plain`), is Gradle's own mechanism for this and gives each variant a manifest. It was not used because flavours multiply (two services are four variants), change the task and APK names every script uses, and are chosen by task name rather than by a property a script can pass through. The price of the property is that both variants share one manifest (below).

### Android: LiveKit's client

`io.livekit:livekit-android:2.30.0`, the newest stable release on Maven Central when this was written (published 2026-11-03), Apache-2.0. From its repository and its published metadata rather than from memory:

| | LiveKit 2.30.0 | The host |
| --- | --- | --- |
| minSdk | 21 | 24 |
| Kotlin | built with 1.9.25; the host compiles its one file with the plugin's own compiler (2.2.10) | Java, plus that file |
| Repositories | Maven Central, and JitPack for `com.github.davidliu:audioswitch` | Google and Maven Central; JitPack for that group only, with the service |
| Permissions it declares | `INTERNET`, `RECORD_AUDIO`, `ACCESS_NETWORK_STATE`, `CAMERA`, `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_MEDIA_PROJECTION`; its dependencies add `MODIFY_AUDIO_SETTINGS` and `BLUETOOTH` (to API 30) | see below |
| Brings | `io.github.webrtc-sdk:android-prefixed` (WebRTC, a 12.3 MB library for arm64), `livekit-ffi-android` (1.6 MB) with JNA, OkHttp, kotlinx coroutines and serialization, protobuf-javalite, Dagger, `androidx.core` | |

**Why a dependency at all** (ADR 0007: only where writing the equivalent is a significant, ongoing cost). The equivalent is a WebRTC stack (ICE, DTLS-SRTP, Opus, echo cancellation, jitter buffers, the audio device) and LiveKit's signalling protocol on top. Neither is something this project could write or keep up with, and the server side is LiveKit already, so the client is not a choice between vendors. The dependency is confined as the policy asks: one Kotlin file in the host, no type of it in any Rust crate or public API. The application sees `LiveJoin` and `LiveEvent`; another media server would be another host implementation.

**Why Kotlin.** LiveKit's API is `suspend` functions and a flow of events. Java can call a `suspend` function only through a hand-written `Continuation`, for each call. One Kotlin file of under 500 lines, comments included, is smaller and more likely to be right than that. The Android Gradle plugin compiles it without a further plugin.

**What it costs**, measured on debug APKs for arm64 only, freshly packaged (`MORNINGSTAR_ABIS=arm64-v8a tools/build-android.sh`):

| APK | Size | Native libraries |
| --- | --- | --- |
| Atlas (no service) | 9,915,690 bytes | `libatlas.so` only; no `kotlin`, `livekit`, `webrtc` or `okhttp` in its file list or its classes |
| Barua Talk's library in a host built without the service | 14,672,659 bytes | `libbarua_talk.so` only |
| Barua Talk (`services = ["live"]`) | 34,241,966 bytes | `libbarua_talk.so` 14.4 MB, `liblkjingle_peerconnection_so.so` 12.3 MB, `liblivekit_ffi.so` 1.6 MB, `libjnidispatch.so` 0.2 MB |

**The service costs 19.6 MB** of a debug APK for one ABI: 14.0 MB of native code and 13.6 MB of classes (5.5 MB compressed), unminified. A release build with R8 would shed most of the classes; that was not measured. (The "about 20 MB" Atlas APK of earlier notes is the default build for two ABIs; for arm64 alone it is 9.9 MB.)

**`MorningstarLive.kt`** (`templates/android/app/src/live/java`):

- **Threads.** `command` is called on Rust's render thread and only posts. Everything else runs on the main thread, where its state is confined and its coroutines run; LiveKit does the socket, WebRTC and the audio device on threads of its own. Nothing blocks the main thread or the render thread.
- **Permission.** The microphone question is the one recording asks. `MorningstarServices.withMicrophone` is that flow made callable (one question on screen however many asked; the recording start uses it too, with the same behaviour as before).
- **Joining.** `LiveKit.create`, events collected before `connect`, `room.connect(url, token)`, `localParticipant.setMicrophoneEnabled(true)` unless muted, then `Connected` and the people already there. An address that is not `ws(s)://` or an empty ticket fails before anything is asked.
- **Audio.** LiveKit's `AudioSwitchHandler` puts the device into `MODE_IN_COMMUNICATION` and takes the audio focus when the room connects, and gives both back when the room is released. The host chooses the order of outputs: a headset if there is one, then the loudspeaker or the earpiece as the application said; `set_live_speaker` selects the other. The volume keys set the call's volume while a room is joined.
- **Ending.** One function ends a session, once: it cancels the coroutines, calls `room.disconnect()` and `room.release()`, and only then reports the final event.
- **Logs** (tag `Morningstar`, prefix `live:`): join, connected, someone in or out (a count, never who), reconnecting, the audio mode and output a second after connecting and after releasing, every failure. Never the ticket, the address, or an identity. A failure's text is the SDK's with the ticket cut out of it, since a connection error may quote the address it was connecting to and the ticket travels in that address.

**The manifest.** The live room is voice only, so the merged manifest should not ask for a camera or tell a store that the device needs one. Because both variants share the manifest, it carries four `tools:node="remove"` lines for `CAMERA`, `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_MEDIA_PROJECTION` and `BLUETOOTH`, which remove nothing in an app without the service. Read back from the two APKs (`aapt2 dump permissions`): Atlas has `INTERNET` and `RECORD_AUDIO`, as before; Barua Talk has those, `ACCESS_NETWORK_STATE` and `MODIFY_AUDIO_SETTINGS`, and no implied camera or Bluetooth feature.

**The interface** grew on both sides together:

- Out: no new upcall. The four commands go through `onServiceCommand` (`SERVICE_LIVE_JOIN` 10: first the address, second the ticket, data three bytes of settings; `SERVICE_LIVE_LEAVE` 11; `SERVICE_LIVE_MICROPHONE` 12 and `SERVICE_LIVE_SPEAKER` 13: data one byte).
- In: one native method, `nativeLiveEvent(long id, int kind, int code, String first, String second)` (25 native methods on both sides; descriptor `(JIILjava/lang/String;Ljava/lang/String;)V`, compared with `javap -s`). The four values are decoded by `LiveEvent::from_wire` in `morningstar-core`, which both hosts use and whose table is the definition; an unknown kind is ignored, an unknown ending still ends the room.

### iOS

The two halves are in step and the service is not there. `morningstar_ios.h` has the four request kinds (`MORNINGSTAR_SERVICE_LIVE_JOIN` 11 to `_SPEAKER` 14), the event kinds and codes, and `morningstar_ios_live_event`; `morningstar-ios` implements them (the same `from_wire`); `HostServices.swift` answers every join `MORNINGSTAR_LIVE_FAILED` with `MORNINGSTAR_LIVE_ERROR_UNSUPPORTED`, logs "live rooms are not available on iOS yet", and ignores the other three. An application therefore shows its "not available" sentence on iOS instead of a call that never connects.

An implementation would use LiveKit's Swift client (Swift Package Manager, which `project.yml` can declare per app in the same opt-in way), an `AVAudioSession` in `.playAndRecord` with mode `.voiceChat` for the length of the room, and the microphone question `HostServices` already asks for recording. iOS has never been compiled on this machine.

### The test driver

`Ui::testing` plays the room, as it plays the server:

```rust
ui.send(Message::Call);
let room = ui.live_room().unwrap();              // what the application joined with
assert_eq!(room.join.token, "the-ticket");
assert!(room.microphone && !room.speaker);
ui.live_event(LiveEvent::Connected);             // "the room connected"
ui.send(Message::Mute);
assert!(!ui.live_room().unwrap().microphone);    // the button acted on the room
ui.live_event(LiveEvent::Disconnected(LiveDisconnect::Lost));
assert!(ui.live_room().is_none());
```

The driver behaves as a host does: one room at a time (a second join ends the first with `Replaced`), leaving ends it with `Left`, `forget_pending` leaves it, and those endings are delivered at the start of the next frame or by `deliver_effects`, not inside the `update` that caused them. A stand-in host (`Ui::with_services`) can answer a join with its first event; with no host at all a join fails as `Unsupported` at the next frame.

### Barua Talk

The first user (`examples/barua-talk/SCREENS.md`, "Sound"). One function, run after every message, makes the room follow the call in progress: joined once when a call between two people becomes active, when a room's call gets its ticket, and as soon as a phone call is dialled; left when that call is no longer the one in progress, however that came about. Mute and Speaker act on the room. The call screens say how the sound stands in words. The scripted app joins nothing.

`examples/services-check`, the device smoke test, declares the service too and has a button for it. With no server and no ticket it shows the way to the door: the microphone question, LiveKit's libraries loading, a connection that is refused, and the failure as a readable line.

## Video: the design to follow

Not built. Voice had to be whole first, and video cannot be finished properly without a device: nothing about it (a camera, a decoder's output format, the cost of getting 30 pictures a second onto the screen) can be checked on this machine. This is the design that would be followed.

**The question** is how a decoded frame gets onto the screen beside a `wgpu` surface. There are two answers.

1. **A native video view** (`SurfaceViewRenderer` on Android, LiveKit's `VideoView` on iOS) placed by the host at a rectangle the application declares, in its own surface under or over Morningstar's. Cheapest in CPU and memory: the frame goes from the decoder to the compositor without being touched. But the picture is then outside the renderer: it cannot be clipped to rounded corners, faded, transformed or drawn over unless Morningstar's surface is made translucent and stacked above it, which depends on the device's compositor and on a pre-multiplied alpha mode that not every Android Vulkan surface offers; the pixel tests and the fidelity tests cannot see it; and the two platforms differ. Barua Talk's own design draws its controls over the picture and rounds the camera tile.
2. **Frames as textures in the display list.** The host hands each decoded frame to Rust and the renderer draws it like an image: every clip, corner, transform and opacity applies, the UI draws over it, and both platforms are the same code above the host.

**The second is the one to build**, in its smallest form first:

- **Out of WebRTC**: a `VideoSink` on the track. Frames arrive on a WebRTC thread as I420 (`VideoFrame.Buffer.toI420()`: three planes with strides, as direct buffers).
- **Across**: one native method, `nativeVideoFrame(track, width, height, rotation, y, u, v, strides)`, which copies the three planes with `GetDirectBufferAddress` into a slot for that track that holds **only the latest frame** (no queue: a frame the renderer has not taken is replaced), and wakes the render thread as an answer does.
- **Onto the GPU**: three `R8` textures per track (1.5 bytes a pixel instead of 4), written with `queue.write_texture` when the slot has a new frame, and a new quad kind that samples them and converts YUV to RGB (BT.601 or BT.709 by the frame's size, limited range) in the fragment shader. No conversion on the CPU. This is a renderer change with pixel tests of its own (known planes in, known colours out), which is what makes it checkable on the host.
- **In the UI**: `LiveVideo::of(identity)`, an element that is drawn with the track's textures as `Image` is drawn with its own (fit, radius, clip), or with a placeholder while there is no frame. A frame is requested only while such an element is on screen and a new picture has arrived: demand-driven rendering (ADR 0012) at the video's rate, and idle otherwise.
- **In the service**: `LiveJoin::camera(bool)`, `set_live_camera`, `LiveEvent::VideoStarted { identity, track }` and `VideoStopped`; the local camera's preview is the same sink on the local track. The `CAMERA` permission is asked for at the first use, and declared by a second opt-in name (`services = ["live", "live-video"]`) so that a voice-only app's store listing does not show a camera.
- **To measure before calling it done** (AGENTS.md: no performance claim without measurements): the copy and upload per frame (a 720p I420 frame is 1.4 MB: 41 MB a second at 30 frames), frame time and battery on a mid-range phone with one and with four pictures, and the latency added. If the copy is too dear, the zero-copy route is importing the decoder's `AHardwareBuffer` (Android) or `CVPixelBuffer` through a Metal texture cache (iOS) into `wgpu` through its HAL, which is unsafe, per-backend, and exactly the kind of thing to do only with numbers in hand. If that is too dear as well, the first answer remains for a full-screen picture.

## Consequences

- `ServiceRequest` has four more variants and `ServiceResult` one; a low-level `App` that matches on them exhaustively must add arms. `Application` is unchanged.
- "Every request is answered exactly once" (ADR 0019) now has one stated exception.
- `forget_pending` now does something on the device (it leaves a joined room).
- The host interfaces grew, both halves together. As always, an application library and a host from different sides of this change must not be combined.
- An app's `Cargo.toml` can now change what its host is built from, and `tools/build-android.sh` passes one more property. Building in `templates/android` by hand without it builds the host without the service.
- Every Morningstar APK is 2.4 MB of classes lighter (the Kotlin library), and is packaged for the app's ABIs only.
- An app with the service: 19.6 MB larger for one ABI (debug), two more install-time permissions, and a build that needs JitPack to be reachable the first time.

## Limitations

- **Voice only.** No camera, no remote pictures, no screen sharing.
- **One room at a time**, and no way to be in a room without the microphone permission (listening only).
- **Not in the background.** Leaving the screen, or the screen locking, ends the room. There is no proximity sensor handling either: at the ear, the screen stays on.
- **No Bluetooth headset.** The permissions for one are not declared or asked for. A wired headset is used when plugged in.
- **No chat or data messages through the room**, no audio levels (only who is speaking), no choice of input device, no control of what is subscribed to. The media volume control of Barua Talk's call screen still changes a number only; the volume keys set the call's volume.
- **The ticket is used as given.** It is not refreshed; a room that outlives its ticket's hour ends as the server ends it.
- **`Failed(text)` is the SDK's wording** and is for logs. Applications show a sentence of their own.
- **iOS has no live rooms.**

## Verified

On the development host (macOS):

- Data types and the wire decoding: 13 tests in `crates/morningstar-core/src/services.rs` (5 new: defaults, that nothing prints the ticket, which events are final, the four values of each event, unknown codes).
- Engine: 2 new tests in `crates/morningstar-runtime/src/engine.rs` (a join answered once per event with one frame each and idle between; events delivered to a hidden app).
- UI layer: 22 tests in `crates/morningstar-ui/tests/live.rs`. Every event as a message, in order; nothing after the final one; one room at a time; leave, mute and speaker; `Hidden`; `forget_pending` leaves, with the driver and as a host sees it; a join forgotten before the host took it is never made; a wrong kind of answer; leaving from inside an event; a stand-in host; `Unsupported` with no host; the `App` path a real host uses; a `Ui` under an `Engine`; the handler dropped uncalled at teardown.
- `examples/barua-talk`: 27 tests in `tests/sound.rs` drive the app with fingers against the stand-in server while the test plays the room; the suite is 81 tests, the five fidelity figures unchanged.
- `examples/services-check`: 2 new tests for its live-room button.
- `tools/morningstar-devtools`: the `services` key (2 tests) and `--print services` for three real packages.
- `cargo clippy ... --all-targets -- -D warnings` for core, runtime, ui, morningstar, barua-talk, services-check and devtools; `cargo ndk -t arm64-v8a clippy -p barua-talk -- -D warnings` and the same for `services-check` (NDK 27.1); `cargo clippy --target aarch64-apple-ios -p morningstar-ios -- -D warnings`.
- **The Android host builds**, with Gradle 9.6.0 and AGP 9.5.1: with the service (the Kotlin file against LiveKit 2.30.0, the Java against it) and without. The two APKs were read back: file lists, the strings of their classes, `aapt2 dump permissions` and `badging`. Atlas's holds the refusing `MorningstarLive` and nothing of LiveKit, WebRTC or Kotlin.
- `morningstar_ios.h` passes `cc -fsyntax-only -Wall -Wextra`; `HostServices.swift` passes `swiftc -parse` (syntax only).

## Not verified

- **Everything on an Android device.** No APK was installed. Unseen: that LiveKit's native libraries load beside the app's; the microphone question from a join, its refusal, and "don't ask again"; that a real ticket connects; that the microphone is published and heard, and remote sound plays; `MODE_IN_COMMUNICATION` set and restored, and audio focus against music or a phone call; earpiece and loudspeaker, and a wired headset; that muting silences the microphone; reconnection across a network change; `onStop` ending the room and the return joining again; the kept-on screen and the volume keys; Bluetooth's absence being harmless on Android 11 and below, where LiveKit's audio library expects its permission; echo, delay and battery; API 24 to 35.
- **Against the real service.** The Talk API answered 500 to creating rooms and calls, so no ticket could be obtained and no room joined, by any client. Nothing was tested against a LiveKit server.
- **A release build** (R8 with LiveKit's consumer rules, and what the service then costs).
- **Everything on iOS**, beyond the Rust half type-checking: the Swift is uncompiled, and the service is a refusal.
- **Cost**: an event's path to the application, the SDK's CPU and memory, were not measured.

The test to run on a phone once the API answers is in `examples/barua-talk/SCREENS.md`, "The test on a phone", with the log line that proves each step.
