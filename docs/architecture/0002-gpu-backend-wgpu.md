# 0002 — GPU backend: wgpu behind a Morningstar-owned crate

Status: Accepted (2026-11-11)

## Context

Morningstar needs Vulkan on Android and Metal on iOS. Writing and maintaining two native backends is a large cost that buys nothing a user can see.

## Decision

Use `wgpu` (currently 30.x) as the graphics abstraction, with WGSL for shaders, and confine it to `morningstar-gpu`. No `wgpu` type appears in that crate's public API: errors are carried as strings, handles as `NativeSurface`, GPU identity as `AdapterInfo`.

Specific choices in the current implementation:

- **Features**: `metal`, `vulkan`, `gles`, `wgsl` only. GLES stays enabled as the fallback for Android devices with missing or broken Vulkan drivers.
- **The device outlives surfaces.** Android destroys the drawable whenever the app is backgrounded. `Renderer` drops only the surface and keeps the device, so GPU resources survive.
- **Adapter selection** is deferred to the first surface (so the adapter is known to be compatible with it) and prefers low power.
- **Limits** are `downlevel_defaults`, so nothing can accidentally rely on a capability a low-end device lacks.
- **Surface format** prefers sRGB, so `Color` is linear and encoding is done by the GPU.
- **Present mode** is `Fifo` (vsync). See ADR 0004.

## Consequences

- wgpu's API changes materially between major versions (in 30: `Queue::present`, the `CurrentSurfaceTexture` enum, fallible `get_mapped_range`). Confinement to one crate bounds the cost of each upgrade.
- wgpu adds roughly 5 MB to the release `.so` per ABI before any Morningstar rendering code exists. Binary size must be tracked in the benchmark suite.
- We inherit wgpu's handling of platform details we have not yet validated ourselves. Known ones to check on devices:
  - **Android surface pre-rotation.** If the swapchain is not pre-rotated, the compositor rotates every frame, which costs GPU time and power in landscape.
  - **Device loss.** There is no recovery path yet; a lost device currently means frames fail and are logged.
- If wgpu proves limiting, the replacement is contained: reimplement `Renderer` and `clear_offscreen`.
