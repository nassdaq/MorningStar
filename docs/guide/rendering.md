# How rendering works

A frame starts as your state and ends as pixels. This page follows it through the crates in between. You do not need any of it to write an app; it explains why the rules elsewhere in these pages are what they are.

## From state to pixels

{{steps}}
1. **`view` builds a tree.** The framework calls your `view` and gets an `Element` tree: boxes, texts, pictures. The tree is thrown away after the frame. The few pieces of widget state that outlive it (a scroll position, the focused field, a press in progress, an animation on its way) are kept by identity; see [the application model](application-model.md#identity).

2. **Layout turns the tree into rectangles.** The layout engine (`morningstar-layout`) knows three kinds of size: fixed, hug and fill. It resolves a tree in four linear sweeps, with no constraint solver. Lengths are in dp; the result is snapped to whole physical pixels using the display's scale. [ADR 0010](../architecture/0010-layout.md), [ADR 0012](../architecture/0012-scale-insets-and-demand-rendering.md).

3. **Text is shaped once and cached.** `morningstar-text` shapes and wraps text with the fonts the app bundles, and rasterises glyphs into an atlas. A text that is shown again costs a cache lookup. Text is measured and placed the way a browser does it, so a screen can match a design drawn in one. [ADR 0009](../architecture/0009-text.md), [ADR 0022](../architecture/0022-text-metrics-and-pixel-snapping.md).

4. **Painting writes a display list.** Each widget appends drawing operations: rectangles with a radius, a border, a gradient or a shadow; glyphs; images; clips; layers for group opacity. The list is plain data in `morningstar-core`, which is what lets a test or the screenshot tool inspect a frame without a GPU. [ADR 0011](../architecture/0011-renderer-vocabulary.md).

5. **The renderer draws the list.** `morningstar-gpu` is the only crate that touches the GPU, through `wgpu`: Vulkan or GLES on Android, Metal on Apple hardware. A screen without pictures is one draw call in one render pass; each image splits the draw call. [ADR 0002](../architecture/0002-gpu-backend-wgpu.md), [ADR 0014](../architecture/0014-images.md).
{{/steps}}

## When a frame is drawn

Rendering is demand-driven. A frame is produced after input, when an answer to an effect arrives, while something animates, and when a timer the app asked for fires. Otherwise nothing is drawn: on the phone the render thread is blocked and the app uses no CPU or GPU. The first device session recorded exactly one frame in six idle seconds.

On Android a dedicated render thread paces frames to the display's vsync and stops when the app is not visible. [ADR 0004](../architecture/0004-threading-and-frame-pacing.md).

## Colour

Colours are composited as sRGB-encoded values, the way the web platform does it, so a translucent layer over a background gives the colour a browser or a design tool shows. Give colours as the design's hex values through `hex(0xRRGGBB)`. [ADR 0023](../architecture/0023-compositing-space.md).

## The hosts

The Android project (Java, with Kotlin for live rooms) and the iOS project (Swift) contain no application logic. They create the surface, forward touches and the keyboard, and carry out platform services such as HTTP requests and recording. The app crate is the native library the host loads; `morningstar::app!` registers it. [ADR 0003](../architecture/0003-platform-hosts.md), [ADR 0005](../architecture/0005-application-entry-point.md).

## What a frame costs

Only what has been measured is stated in [Performance](../reference/performance.md): a baseline on the development machine, and one early session on a phone.
