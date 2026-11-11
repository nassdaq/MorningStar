# 0001 — Crate boundaries

Status: Accepted (2026-11-11)

## Context

The roadmap sketches fourteen crates. Splitting that finely before the subsystems exist would fix boundaries we cannot yet justify, and the roadmap itself says to start with a handful.

## Decision

Start with a handful of crates and one rule about direction:

```
core  <-  gpu, text  <-  runtime  <-  android | ios  <-  morningstar  <-  application
```

(`morningstar-text` was added later; see ADR 0009. `gpu` and `text` do not depend on each other: they meet through plain data types in `core`.)

- `morningstar-core`: no dependencies. Colour, lifecycle state machine, frame statistics.
- `morningstar-gpu`: the only user of `wgpu`. Device, surface, frame submission.
- `morningstar-runtime`: `Engine`, the single object a platform host drives, and the `App` trait.
- `morningstar-android`, `morningstar-ios`: translate native callbacks into `Engine` calls. Each is empty when compiled for any other target.
- `morningstar`: the public API. Re-exports and links the right platform crate.

`core`, `gpu` and `runtime` contain no platform logic. The single exception is the `NativeSurface` enum in `gpu`, whose variants are `cfg`-gated because a native handle is inherently platform-typed.

A new crate is split out when a subsystem has (a) a public interface that other crates consume and (b) tests that run without its neighbours. Expected next: `morningstar-renderer` (display list) in Phase 2, then `-layout`, `-text`, `-ui`.

## Consequences

- The shared crates build and test on a development host with no mobile toolchain.
- Platform crates cannot be unit-tested on the host; they are kept small enough to review by reading (Android: ~360 lines, iOS: ~190).
- `runtime` currently depends on `gpu` directly. When the display list arrives, `runtime` should depend on the renderer's interface, not on the GPU backend.
