# 0012 — Display scale, safe-area insets and demand-driven rendering

Status: Accepted (2026-11-11). Verified on the development host only: not yet run on an Android device, and the Swift has never been compiled.

Supersedes the open question "Rendering every vsync is not idle rendering" of [ADR 0004](0004-threading-and-frame-pacing.md), and its statement that a visible app draws at every refresh. Everything else in 0004 stands. Resolves the limitation "sizes are in physical pixels … until the hosts pass a scale factor" noted in [ADR 0009](0009-text.md).

## Context

Three things a real application needs were missing from the host interface:

1. **Scale.** Designs are specified in density-independent units (dp on Android, points on iOS). The app only knew physical pixels, so the example sized everything as a fraction of the screen, which makes a button physically larger on a tablet than on a phone.
2. **Safe area.** The surface covers the whole window. The status bar, a display cutout, the navigation or gesture bar and the home indicator sit on top of it, and the app had no way to know where.
3. **Idle.** A visible app redrew at every display refresh whether or not anything had changed: at 120 Hz, 120 frames a second of CPU, GPU and display-pipeline work to show a still picture. ADR 0004 recorded this as a known battery cost.

## Decision

### The display list stays in physical pixels

Scale is information handed to the app, not a transform applied by the renderer. `FrameInfo` gains `scale: f32` (physical pixels per dp) with the helpers `dp(length)`, `logical_size()` and `safe_area()`. An app, and the widget layer that will be built on `App`, works in dp and multiplies by the scale when it emits drawing operations and when it reads pointer positions, which also stay in pixels.

Reasons: the renderer, the text crate and the pixel tests already agree on pixels, and that contract belongs to other subsystems; a layer that owns layout must know the scale anyway, to snap edges to whole pixels and to choose glyph sizes; and a renderer-side transform would have to be undone for hit-testing.

`scale` is `DisplayMetrics.density` on Android and the screen's `nativeScale` (the view's `contentScaleFactor`) on iOS. It is `1.0` until a host reports it and is never zero, negative or non-finite: such a value from a host is ignored.

### Insets are per-edge, in physical pixels, and the surface is edge to edge

`FrameInfo.insets` is an `Insets { left, top, right, bottom }`, a new plain type in `morningstar-core` next to `Rect`. It is the union of the system bars and the display cutout on Android (`WindowInsets`), and `safeAreaInsets` on iOS. The soft keyboard is deliberately not part of it: it is a different concern with different behaviour, and will be reported separately when text input exists.

The app paints its background over the whole surface and keeps content and touch targets inside `info.safe_area()`. The engine does not clip or offset anything.

The Android host makes the window edge to edge itself on every supported API level, because the platform only does so from API 35: transparent bar colours, `setDecorFitsSystemWindows(false)` from API 30 and the `SYSTEM_UI_FLAG_LAYOUT_*` flags before it, and a `layoutInDisplayCutoutMode` that lets the window into the cutout from API 28. Insets are read with `WindowInsets.getInsets(systemBars | displayCutout)` from API 30, and with the system-window insets plus the cutout's safe insets before it.

### Scale and insets are engine state, set by the host, and can change at any time

`Engine::set_scale` and `Engine::set_insets` may be called at any point and as often as the host likes; each returns whether the value changed. A change produces a frame. Hosts therefore resend on every event that might have changed them (configuration change, layout, surface resize) without tracking the previous value.

During a rotation the new size and the new insets arrive as separate calls, so one frame can combine the new size with the old insets. `safe_area()` never returns a negative size, which makes that frame harmless. Delivering both atomically is possible later if it proves visible.

### Frames are produced on demand

The engine holds one flag: *a frame is wanted*. `Engine::needs_frame()` is that flag and visibility together. The flag is set by

- becoming visible, and a surface being created or resized;
- a scale or inset change;
- every pointer event;
- the app calling `frame.request_redraw()` while building a frame;
- the host calling `Engine::request_redraw()` (Android forwards `surfaceRedrawNeeded`);
- a frame that did not reach the display (skipped by the renderer, or failed).

It is cleared at the start of the frame that answers it, so a request made while that frame is being built is for the next one, and it is restored if the frame was not presented. When it is clear, `Engine::frame` returns without calling `App::frame` and without touching the GPU.

**The app-facing rule** is one method. `App::frame` runs when something changed; an app whose picture depends on time calls `frame.request_redraw()` once per frame for as long as that is true. The default is the efficient case: an app that never calls it costs nothing while its picture is still.

The alternatives considered:

- *`App::frame` returns an enum (`Idle` / `Animating`).* Forces every implementation to answer and breaks the signature for no gain; forgetting is impossible, but the safe-looking answer (`Animating`) is the wasteful one.
- *The app declares dirtiness after input (`pointer` returns whether to redraw).* More efficient when a touch changes nothing, but a wrong `false` is a screen that silently does not update, which is the failure this design is arranged to make difficult. Instead **input always produces a frame**; several events delivered before a frame share it. The cost is one frame per display refresh while a finger moves on the screen. The widget layer, which knows what a touch changed, can refine this later without changing the engine's rule.

What can go wrong, and how it shows: an app that animates and forgets `request_redraw` freezes on the animation's first frame, immediately and every time, which is hard to miss in development. An app that calls it unconditionally renders continuously, which is the old behaviour, not a new failure.

**There is no way to request a frame from another thread, or at a future time.** Both will be needed (a network response arriving, a blinking cursor) and both need a wake-up path into each host. They belong with the worker-thread design that ADR 0004 leaves open, and are left out here rather than guessed at.

### How each host idles

**Android.** The render thread's loop asks `needs_frame()`. If true it drains pending messages without blocking and calls `frame()`; vsync pacing is still the blocking acquire of the next drawable, so `Choreographer` was not needed. If false it blocks in `recv()` on its message channel. Only a message from the main thread can make a frame wanted, so nothing is missed by sleeping, and a blocked thread uses no CPU. This is the same wait that was already used while the app was invisible; it now also covers "visible, nothing changed".

To make idling observable without looking at the screen, the thread writes its frame count to the log one second after it runs out of work and once more five seconds later, then waits with no timeout. Two equal counts, seconds apart, show that nothing was drawn. The cost is two timer wake-ups per idle period.

**iOS.** The `CADisplayLink` is paused whenever `morningstar_ios_needs_frame()` is false. Because that answer can only change inside a call the host makes into Rust, the host follows one rule: every method that calls into the engine ends by setting `displayLink.isPaused` from it. A paused display link wakes nothing.

### Statistics describe frames, not waiting

The frame interval is recorded only between two frames that were produced back to back: the earlier one asked for the later one (an animation, or continuous mode). A frame produced after an idle period has no interval, since the time since the previous frame says nothing about how fast it was. CPU time and present time are recorded for every frame. `EngineReport.interval` is therefore optional.

A consequence: frames driven only by input (a drag, each frame triggered by the next touch event) contribute CPU and present time but no intervals. A latency measure from "frame wanted" to "frame presented" would cover them and is not built.

### Continuous mode, for benchmarks only

A steady stream of frames is still needed to measure frame pacing. `Engine::set_continuous(true)` makes every frame ask for another. Hosts expose it as a launch switch (`--ez dev.morningstar.continuous true` on Android, the `-morningstar-continuous` launch argument on iOS); `benchmarks/android/measure.sh` uses it for its frame statistics and measures idle behaviour separately. It is not an API for applications.

## Consequences

- `FrameInfo` is `#[non_exhaustive]` and built with `FrameInfo::new(size)` and `with_*` methods, so adding a field no longer breaks every test and tool that constructs one.
- The host interfaces grew: four JNI functions (`nativeSetScale`, `nativeSetInsets`, `nativeRequestRedraw`, `nativeSetContinuous`) and four C functions (`morningstar_ios_set_scale`, `_set_insets`, `_needs_frame`, `_set_continuous`). ADR 0003's counts of seven and eight are out of date.
- `Engine` reaches the GPU through a private `Presenter` trait with one real implementation, so that scheduling, propagation and statistics can be tested on a machine with no surface. The cost is one dynamic call per frame.
- An animation must be timed from the `elapsed` of the frame it starts on: the foreground clock keeps running while the app is visible and idle, so `elapsed` no longer advances in steps of one refresh period.
- Frame statistics are logged every 600 presented frames, which for an app that mostly idles can be a long time. That is correct, and benchmarks use continuous mode.
- Earlier benchmark reports describe an app that never idled; their "idle" figures are really continuous-rendering figures and are comparable only with continuous mode.

## Verified

- Scheduling, scale and inset propagation, and statistics: 28 unit tests in `crates/morningstar-runtime/src/engine.rs` against a fake presenter, on the development host.
- The example lays out in dp, stays inside the safe area across ten screen configurations, and asks for frames only during its tap animation: 25 tests in `examples/hello-gpu`, and screenshots (portrait and landscape, safe area outlined) inspected by eye.
- The Android library builds for arm64 and exports all eleven JNI entry points; the Java host compiles and packages into an APK.
- The iOS Rust crate type-checks and passes clippy for `aarch64-apple-ios`.

## Not verified

- **Anything on a device.** The phone was disconnected before this change could be installed, so scale and inset values, edge-to-edge layout and idling on real hardware are unproven. The idle log lines described above exist so that this can be checked without looking at the screen: after launch, expect `scale`, `insets`, then two `idle` lines with the same `frames=` count.
- All Swift (`templates/ios/Sources/HostViewController.swift`): never compiled, so display-link pausing, `viewSafeAreaInsetsDidChange` and the scale path on iOS are unproven.
- Android API levels below 36. The edge-to-edge and inset code paths for API 24 to 34 are written from the platform documentation and have not been run.
- Rotation, multi-window, and a density change on a running app, on any device.
- Power. Idle is argued from frame counts and a blocked thread, not from an energy measurement.

## Known gaps

- A frame the renderer presents on a *suboptimal* swapchain (for example after a 180-degree rotation, which changes no size) used to be corrected by the next vsync's frame. Now there may be no next frame, so the compositor keeps transforming that one until something else causes a redraw. The picture is right; the composition is not the cheapest. The renderer should report this so the engine can ask for one more frame.
- `morningstar_devtools::screenshot` draws at a scale of 1 with no insets. The example's screenshot tool builds its own `FrameInfo` instead.
