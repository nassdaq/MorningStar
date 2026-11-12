# 0018 — System back and a repeating timer message

Status: Accepted (2026-11-12). Verified on the development host only. Nothing here has run on a device: the Java compiles against the API 36 SDK, the Rust type-checks for Android and iOS, and no Back gesture has been made on a phone. What that leaves open is under "Not verified".

Adds two things to the UI layer of [ADR 0013](0013-ui-layer.md) and lifts its limitation "No `request_redraw` from … timers" for applications (ADR 0016 lifted it for the caret only). Builds on the Rust-to-Java upcall and the timed redraw of [ADR 0016](0016-text-input.md), and on the threading of [ADR 0004](0004-threading-and-frame-pacing.md).

## Context

An application with more than one screen could not react to the system's Back: the Android host closed a keyboard if a field had focus and otherwise let the activity go, whatever screen the application was on. And an Elm-style application had no source of time: `view` is a function of the state, the state changes only in `update`, and nothing delivered a message without a touch. A countdown, a resend-code delay or a call timer could not be written.

## Decision

### Back is a question the application answers

```rust
// morningstar::ui::Application
fn back(&mut self) -> bool { false }
```

The system back gesture or key. `true`: the application handled it (it went to its previous screen). `false`: it had nowhere to go, and the system does what it does then. The default is `false`, so existing applications behave as before.

The same method, with the same default, is on the runtime's `App` trait, and `Engine::back()` calls it: a `true` schedules a frame, as input does; a `false` changes nothing. `Ui<A>` forwards `App::back` to `Application::back`, and after a `true` abandons the press in progress and forgets the touch regions, exactly as after a message: the screen under the finger may have been replaced. `Ui::back()` is public, for tests.

A keyboard is not the application's business here: Back first closes an open keyboard, in the host, as before, and that Back never reaches `back`.

### Android: ask without waiting, answer by upcall

The application lives on the render thread ([ADR 0004](0004-threading-and-frame-pacing.md)); Back arrives on the main thread. Two ways to connect them were considered.

1. **A synchronous question**: the JNI entry point sends a message with a reply channel and blocks until the render thread answers, as `nativeSurfaceDestroyed` does. The answer would be available where Android asks for it, but the main thread would wait for whatever the render thread is doing: normally the rest of one frame, which includes waiting for the display inside `present`, and at worst a stalled GPU driver or the creation of the GPU device at startup. Blocking the main thread on a frame is what the threading model exists to avoid, and `nativeSurfaceDestroyed` blocks only because Android requires it.
2. **Ask and be told later** (chosen): `nativeBack()` only posts `Message::Back` and returns. The render thread calls `Engine::back()` between frames and reports the answer with a second upcall, `MorningstarNative.onBackResult(boolean handled)`, which posts to the main thread. Not handled: the host leaves the app.

So the main thread never blocks on Back. The cost is that the host cannot tell Android synchronously "not mine", and has to do the leaving itself (below), and that the answer is late by the frame in flight, if there is one: messages are drained before the next frame is drawn, so an idle render thread answers at once.

`onBackResult` is looked up next to `onTextInputCommand` in `java.rs`, with its own method id, and called with `CallStaticVoidMethodA` and one `jboolean`. A host without the method (older than the library) is logged once and Back is then never answered by Rust; see the timeout below.

On the Java side, `MorningstarActivity.handleBack()` is the one place Back arrives:

- A field has focus and no soft keyboard absorbed the Back (a hardware keyboard): end the editing, as before, and stop. With a soft keyboard up the input method consumes Back itself and `handleBack` is not called; the insets show it going, as before.
- Otherwise count one pending Back, call `nativeBack()`, and arm a one-second timeout.

`onBackResult(handled)` on the main thread takes one pending Back off; if it was not handled, the host leaves. An answer nobody is waiting for is dropped. If no answer comes within the second (the render thread is stuck, or attaching it to the JVM failed), the host leaves anyway: a frozen application must not also swallow Back.

**Leaving** is what `Activity.onBackPressed` does when nothing handles Back, written out because the host can no longer fall through to it: from Android 12 the root activity of its task is moved to the background (`moveTaskToBack(true)`), so coming back is a warm start with the engine and the application's state intact; before Android 12, or when not the task's root, the activity is finished.

### Android 16: `OnBackInvokedCallback`

The template has `targetSdk = 36`. On Android 16, for an app targeting 36, predictive back is on by default: `onBackPressed` is no longer called and `KEYCODE_BACK` is not dispatched. An `OnBackInvokedCallback` is the only way to hear Back there. So:

- `onCreate` registers one callback at `PRIORITY_DEFAULT` on API 33 and later (in a nested class, `Api33`, so the activity class loads cleanly on older releases) and `onDestroy` unregisters it. It calls `handleBack()`.
- The manifest sets `android:enableOnBackInvokedCallback="true"` on the application, so that API 33 to 35 use the same path as 36 rather than a different one per release. Below API 33 the attribute is ignored and `onBackPressed`, kept as an override, calls `handleBack()`.

The callback is registered for the activity's whole life, because the host cannot know in advance whether the application will handle the next Back. The cost: the system never plays its own back-to-home preview animation for a Morningstar app, since to the system the app always handles Back. Declaring "I can go back" per frame, as text input is declared, would let the host register the callback only while it is true and get the system animation otherwise; that is a larger interface than was needed and is left for when someone wants the animation.

### iOS: a C function, not yet called

`bool morningstar_ios_back(MorningstarHost *host)` calls `Engine::back()` and returns its answer. On iOS the engine lives on the main thread, so the answer is immediate and no frame is built inside the call; a frame follows a `true` through the usual `needs_frame` rule. iOS has no system Back: the intended caller is a leading-edge swipe (`UIScreenEdgePanGestureRecognizer`). **The Swift host does not call it yet**; `HostViewController.swift` carries a TODO saying how. The Swift has never been compiled, and an edge recogniser changes how touches near that edge are delivered to the Metal view, which should be judged on a device rather than written blind. A `false` means the host does nothing: an iOS app is not left by a gesture of its own.

### A repeating timer message

```rust
// morningstar::ui::Application
fn tick(&self) -> Option<(std::time::Duration, Self::Message)> { None }
```

While it returns `Some((interval, message))`, `message` is delivered to `update` every `interval`, and frames come only as needed for that. `None` stops it and the application is fully idle again. It is a function of the state, like `view`: a countdown returns `Some` while it runs and `None` at zero, and there is no start or stop call to forget.

It is built entirely on what ADR 0016 added for the caret, with no thread and no clock of its own:

- `Ui` asks `tick()` at the start of every frame and keeps one value: the `FrameInfo::elapsed` at which the next message is due (`None` when there is no timer). A timer that has just appeared, or whose interval changed, is due one full interval from that frame.
- If the frame's `elapsed` has reached the due time, the message is delivered to `update`, **before** `view` is called, so the frame that was drawn for the tick shows its result. Then `tick()` is asked again, because the update may have stopped the timer or changed its pace.
- While a timer is running the frame calls `Frame::request_redraw_after(due - elapsed)`. That is the only thing keeping frames coming: the host sleeps until then (`Engine::redraw_due_in`), one frame is drawn, the message is delivered in it, and the next request is made. A one-second tick is one frame a second. It shares the timed frame with the caret: the shortest delay of a frame wins, and each frame asks again for whatever is next.
- A frame that comes early for another reason (a touch) delivers nothing and asks for the remainder.

**At most one message per frame, and never a burst.** The next due time is the previous one plus the interval, so frames that arrive a few milliseconds late do not make the timer drift. But if that time has already passed too (a whole interval or more went by with no frame), the missed ticks are dropped and the next is due one interval from now. An interval shorter than a frame therefore delivers once per frame.

**Time is foreground time.** `elapsed` stops while the app is paused ([ADR 0012](0012-scale-insets-and-demand-rendering.md)), and the engine drops a timed request on pause and always draws on resume. So a timer pauses with the app: after ten minutes in the background a one-second tick continues from where it was in its interval, and nothing is owed for the ten minutes. Where the clock keeps running with no frames (visible lifecycle but no surface, a stalled frame), the rule above applies: one message, then the rhythm restarts. An application that needs a deadline on the wall clock (an OTP that expires at a given moment) should not count ticks: it keeps a `std::time::Instant` in its state and reads it in `update` when a tick arrives.

Not chosen: a `Subscription` type as in Elm or iced (a general mechanism for one use), a timer thread posting messages (cross-thread wake-ups are still not designed, ADR 0016), and delivering every missed tick (a burst of updates after a pause that an application would have to defend against).

## Consequences

- The host interfaces grew, both halves together. JNI: `nativeBack` (16 native methods in all) and a second Rust-to-Java call, `onBackResult`. C: `morningstar_ios_back`.
- `App` and `Application` each have one more defaulted method, and `Application` has `tick`. No existing application changes: the examples compile untouched.
- Back on Android is now always answered by the host itself (move to back, or finish) instead of by `super.onBackPressed()`. For an application that does not implement `back` the visible behaviour is the same, one render-thread round trip later.
- The system's predictive back-to-home animation is not shown (above).
- `Ui::frame` calls `tick()` once per frame, twice on a frame that delivers; an application without a timer pays one call returning `None`.
- The log line about timed frames now reads "caret blink or tick". `back: handled by the app` / `back: not handled, leaving the app` on the render thread is how Back can be followed from logcat.

## Limitations

- **One timer per application.** Two rhythms are one `tick` at the faster pace and a counter in the state.
- **A tick is not a clock.** It counts foreground intervals; see above for wall-clock deadlines.
- **No animation-frame message.** `tick` with a zero interval delivers once per frame and keeps frames coming, which is continuous rendering; nothing is interpolated for the application.
- **State changed through `Ui::app_mut` or a background thread starts no timer by itself**: `tick` is read when a frame is built, and nothing asks for a frame. Cross-thread wake-ups are still not designed.
- **Back cannot be declined synchronously**, so the system's back-to-home preview is lost, and a Back the application declines takes effect after the render thread answers.
- **A Back answered after the one-second timeout is ignored**: the app has been left by then even if the application would have handled it.
- **An answer can be matched to the wrong Back** only in a window of milliseconds around an activity being destroyed and recreated with Backs in flight; the pending count belongs to the activity instance.
- **iOS has no caller** for `morningstar_ios_back`.

## Verified

On the development host (macOS) only:

- Engine: a handled Back produces exactly one frame, a declined one draws nothing, an app without `back` declines, a Back while hidden is answered and drawn on return: 4 tests in `crates/morningstar-runtime/src/engine.rs`.
- UI layer, driving frames with `FrameInfo::with_elapsed`: 17 tests in `crates/morningstar-ui/tests/back_and_tick.rs`. Back pops a navigation stack and then declines, reaches the application through `App::back`, defaults to declining, and abandons a press only when handled. Ticks: idle (no redraw request of either kind) when `tick` is `None`; one message per requested frame at the interval, with the request being exactly what a host sleeps for; nothing early and the remainder asked for; no drift from late frames; one message after a ten-second gap and the rhythm restarted; at most one per frame with a 1 ms interval; a countdown that stops itself leaves the frame with no request; the delivering frame's view sees the new state; restart and a changed interval wait a full interval; the caret and the timer share the timed frame.
- `cargo ndk -t arm64-v8a clippy -p hello-gpu -- -D warnings` (NDK 27.1) and `cargo clippy --target aarch64-apple-ios -p morningstar-ios -- -D warnings` are clean.
- `MorningstarActivity.java` and `MorningstarNative.java` compile with `javac -Xlint:all` against `android-36/android.jar`, without warnings.

## Not verified

- **Back on any device.** No APK was built or installed for this record, and no gesture or key was sent. Unseen on hardware: that the callback is invoked on Android 16 with `targetSdk` 36; that `onBackResult` is found and called (its JNI signature `(Z)V` is checked only by reading); that a handled Back redraws and a declined one moves the task to the background; the keyboard-open and hardware-keyboard cases; three-button navigation versus gestures; the timeout.
- **Android API 24 to 35**: the `onBackPressed` path below 33, and `enableOnBackInvokedCallback` on 33 to 35.
- **The APK build** with the new manifest attribute (lint may remark on an attribute newer than `minSdk`; it is not an error).
- **A tick on a device**: that a one-second timer shows as one timed frame a second in logcat and the render thread sleeps in between. On the host this is shown at the level of the frame's request, and the engine's handling of that request has its own tests from ADR 0016; the two were not run together under a real `Engine` with a real clock.
- **Everything on iOS** beyond type-checking.
- Power: one frame a second is argued from frame requests, not measured.
