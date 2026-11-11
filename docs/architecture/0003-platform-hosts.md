# 0003 — Platform integration: thin native hosts

Status: Accepted (2026-11-11). iOS half is unverified on a device.

## Context

There are three ways to get a Rust renderer onto a phone screen:

1. Let a windowing framework (winit, or Android's `NativeActivity`/`GameActivity`) own the application.
2. Drive everything from Rust through bindings (JNI calls into the Android framework; `objc2` for UIKit).
3. A small native host that owns the platform view and forwards callbacks to Rust.

A mobile app is more than a surface. It needs the soft keyboard and IME composition, accessibility services, system dialogs, permission prompts, and correct lifecycle behaviour. On Android most of these are reachable only through a real `Activity` and `View`; `NativeActivity` makes text input and accessibility notably hard. The wgpu documentation makes the same distinction between taking over an application and integrating into one.

## Decision

Option 3. Each platform has a host project under `templates/` that contains no application logic and is identical for every app.

**Android** — `MorningstarActivity` (Java, ~80 lines) owns a `SurfaceView` and forwards `onStart`/`onStop` and the three `SurfaceHolder.Callback` methods, plus touch events, to seven `static native` methods. The Rust side obtains an `ANativeWindow` from the `Surface`. The app's library name comes from manifest metadata, so the host needs no per-app code.

**iOS** — a `UIView` backed by a `CAMetalLayer`, a `CADisplayLink`, and a scene delegate, forwarding to eight C functions declared in `morningstar_ios.h`.

(Counts in this record are from when it was written; scale, insets and redraw requests were added later, see ADR 0012.)

Both interfaces deliberately carry the same information: created(handle, size), resized(size), destroyed, visible(bool), and (iOS only) frame.

Details that are decisions, not accidents:

- **Java, not Kotlin, for the Android host.** It needs no language plugin or runtime, and it is generated code nobody edits.
- **No JNI or NDK binding crate.** The Android host needs four C functions; they are declared by hand in `sys.rs`, which is the crate's whole unsafe FFI surface.
- **Visibility drives rendering, not focus.** `onStart`/`onStop` rather than `onResume`/`onPause`, because a paused activity in multi-window mode is still on screen.
- **Rotation does not recreate the activity** (`configChanges`); it arrives as a surface resize.
- **`surfaceDestroyed` blocks** until the render thread has released the surface, as Android requires.
- **The scene lifecycle is used on iOS**, since Apple is moving away from the app-delegate-only lifecycle.

## Consequences

- A small amount of Java and Swift exists and must be kept in step with its Rust counterpart. The pairs are named in `AGENTS.md`.
- Application developers still write no Kotlin or Swift.
- The host is the natural place to add an `InputConnection`, an accessibility node provider and their iOS equivalents later, which is the reason for choosing it.
- Using `objc2` to remove the Swift host remains possible later; nothing here prevents it.
