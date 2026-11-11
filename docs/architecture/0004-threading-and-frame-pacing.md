# 0004 — Threading and frame pacing

Status: Accepted for Android. Open for iOS.

## Context

The roadmap asks for three execution domains (main/UI, workers, render) and warns that the actual arrangement must be validated per platform. It also requires idle rendering for battery life.

## Decision

**`Engine` is single-threaded by contract.** It is created on, and only called from, one thread. It contains no locks. Which thread that is belongs to the platform crate.

**Android: a dedicated render thread.** JNI callbacks arrive on the main thread and each posts a message to a channel; the render thread owns the `Engine` and the `ANativeWindow`. The main thread never touches the GPU and never blocks, with one required exception (`surfaceDestroyed`, bounded by the frame in flight). The thread lives for the whole process, because Android can destroy and recreate the activity many times in one process.

**iOS: the main thread, for now.** Frames are produced from the `CADisplayLink` callback. This is the simplest correct arrangement and avoids `CAMetalLayer` threading rules we have not tested. It means a slow frame blocks UIKit event delivery.

**Pacing comes from vsync, not from timers.** The surface uses `Fifo`; acquiring the next drawable blocks until the display is ready. On iOS the display link provides the tick.

**No rendering while invisible.** `Lifecycle::should_render` is true only when the app is visible and has a non-empty surface. Otherwise the Android render thread blocks on its channel using no CPU, and the iOS display link is paused. A frame that cannot be produced (occluded, swapchain being rebuilt, error) backs off for 16 ms rather than spinning.

**The application clock counts foreground time only**, so animation resumes where it stopped.

## Open questions

- **Rendering every vsync is not idle rendering.** Today a visible app redraws continuously even if nothing changed. Demand-driven frames need dirty tracking from the UI engine (ADR 0006) and, on Android, `Choreographer` for timing instead of blocking in acquire.
- **iOS render thread.** Decide with measurements once there is real per-frame work: main-thread hitches versus the complexity of presenting from another thread.
- **High refresh rate.** Neither host requests 90/120 Hz yet (`Surface.setFrameRate`; `CADisplayLink.preferredFrameRateRange` plus the Info.plist opt-in).
- **Worker threads and async** are not designed. Per the roadmap, no custom async runtime until profiling shows a need.
