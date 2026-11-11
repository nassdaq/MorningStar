# 0008 — Design-to-code loop

Status: Accepted (2026-11-11). The loop exists; what it can express is limited by what the renderer can draw.

## Context

A goal for Morningstar is that a developer, or an AI agent, can be handed a picture of a screen and produce that screen in Rust. Generating plausible UI code from a picture is the easy half. The hard half is knowing whether the result matches, and correcting it. On mobile that normally means build, install, launch, navigate and photograph: minutes per attempt, and not something an agent can do unattended.

## Decision

Make rendering an app to an image a first-class, device-free operation.

`morningstar_devtools::screenshot(app, size, elapsed)` runs one frame of any `App` through the real renderer on the development machine's GPU and returns a PNG. It uses the same display list and the same shader as a phone does. The app can be driven into any state first by calling `App::pointer`.

The intended loop:

1. Read the design image.
2. Write or edit the UI code.
3. `cargo run --example screenshot` (seconds).
4. Compare the PNG with the design; go to 2.

This depends on properties the architecture already has, and which must be kept:

- **The renderer does not need a window** (`render_offscreen`).
- **An app is a pure function of its state, the clock and the surface size.** `App::frame` receives the clock rather than reading one, so a frame is reproducible.
- **Input is data** (`PointerEvent`), so any state is reachable from a script.

## What is still missing for this to be useful on real designs

In rough order of how much each unlocks:

1. ~~Text.~~ Done (ADR 0009), without emoji or mixed styles.
2. ~~Layout~~ Done as an engine (ADR 0010); the widget layer that uses it is ADR 0013.
3. Borders, shadows, gradients and clipping: done (ADR 0011). **Images**: ADR 0014.
4. ~~Density-independent units.~~ The scale factor reaches the app (ADR 0012).
5. **A widget vocabulary** small and regular enough that an agent maps design elements onto it reliably.
6. ~~An image-diff tool.~~ Done: `morningstar-diff`, see `docs/design-to-code.md`.

## Consequences

- Visual regression tests come almost free: a screenshot compared with a stored reference.
- The Metal output on a Mac is not guaranteed identical to Vulkan on a phone, particularly for anti-aliased edges and, later, text. Screenshots prove layout and colour, not device-exact pixels.
- Screenshot PNGs are compressed (about 165 KB at 1080x2400).
