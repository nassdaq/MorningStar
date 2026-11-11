# 0011 — Renderer vocabulary: borders, gradients, shadows, clipping, partial atlas upload

Status: Accepted (2026-11-11). Verified on macOS/Metal only. Not yet run on Vulkan or GLES, nor on a device.

## Context

The renderer drew two things: filled rounded rectangles and glyphs. Real designs (cards, buttons, lists) also need outlines, gradient fills, drop shadows and clipping. Separately, ADR 0009 recorded that the whole glyph atlas (0.5 to 2 MB) was re-uploaded whenever one glyph was added, the suspected cause of a worse p95 frame interval on the phone.

The existing design is one instanced draw call in painter's order (ADR 0006, 0009). The question for each addition was whether it could be had without giving that up.

## Decisions

**Everything stays in the one draw call.** Borders, gradients and shadows are new *kinds* of the same instanced quad, selected per instance in the fragment shader. Clipping is carried per instance and applied as coverage in the fragment shader, not as scissor state, so a clip does not split the draw call and clipped and unclipped operations keep their paint order.

**The display list gains five operations** (`DrawOp` is `#[non_exhaustive]`; existing variants and methods are unchanged):

| `DisplayList` method | `DrawOp` |
| --- | --- |
| `stroke_rect`, `stroke_rounded_rect`, `fill_bordered_rect` | `BorderedRect { rect, corner_radius, fill, border }` |
| `fill_gradient_rect` | `GradientRect { rect, corner_radius, gradient }` |
| `draw_shadow` | `Shadow { rect, corner_radius, shadow }` |
| `push_clip`, `push_rounded_clip` | `PushClip { rect, corner_radius }` |
| `pop_clip` | `PopClip` |

with the plain-data types `Border { width, color }`, `LinearGradient { start, end, start_color, end_color }` (constructors `new`, `vertical`, `horizontal`, `angled`) and `Shadow { offset_x, offset_y, blur, spread, color }`.

**Borders are drawn inside the rectangle's bounds**, together with the fill, in one instance. A translucent border shows the fill beneath it (as a CSS border over a background, or an inside stroke in a design tool).

**Gradients are two-stop and interpolate in linear RGB with premultiplied alpha**, the space the pipeline blends in. This differs from CSS and design tools, which by default interpolate sRGB-encoded values: the middle of a gradient is lighter here. Matching them exactly would need a per-gradient colour-space switch in the shader; it is not done. Start and end are arbitrary points in pixel coordinates; `LinearGradient::angled` gives the CSS meaning of an angle.

**Shadows are analytic**: one quad, no offscreen pass, no blur passes. `blur` has the CSS meaning (a Gaussian with a standard deviation of half the blur radius); `spread` grows the silhouette first.

**Clips nest by intersection. One rounded clip is exact at a time.** The clip state is a rectangle intersected with at most one rounded rectangle. A rounded clip intersects exactly with any number of square clips around or inside it, in either order. That covers the case that matters most, a rounded card partly scrolled out of a square viewport. When a rounded clip is pushed inside another rounded clip, the inner one is exact and the outer one acts as its bounding rectangle until the inner one is popped.

**The atlas says which rows changed.** `AtlasView` gains `dirty: Option<DirtyRows>`, where `DirtyRows { since, start, end }` means: compared with the atlas at generation `since`, the dimensions are the same and only rows `start..end` differ. A renderer that holds a generation in `since..generation` uploads only those rows; any other renderer, and any view with `None`, takes the whole image. The record is a statement about the data, not about any particular renderer, so it stays plain data in `morningstar-core`, needs no acknowledgement from the renderer back to the text system, and is safe when frames are skipped or several renderers share an atlas: the worst case is a full upload, never a stale texture.

## Instance layout

112 bytes per instance (was 64): seven `vec4<f32>`, one vertex attribute each.

| Location | Field | Contents |
| --- | --- | --- |
| 0 | `rect` | x, y, width, height |
| 1 | `color` | fill; gradient start; shadow or glyph colour (premultiplied) |
| 2 | `color2` | border colour; gradient end (premultiplied) |
| 3 | `params` | corner radius, kind, border width *or* shadow sigma, rounded-clip radius |
| 4 | `extra` | glyph: atlas region; gradient: start and end points |
| 5 | `scissor` | rectangular clip as min and max |
| 6 | `clip` | rounded clip as min and max |

Kinds: 0 rectangle (solid, bordered or outline), 1 glyph, 2 gradient, 3 shadow. "No clip" is a finite rectangle of ±1e9, so no branch or NaN is involved.

The vertex-to-fragment interface is nine varyings (limit 16; 15 on WebGL2-class GLES). The vertex attribute count is 7 of 16 and the stride 112 of 2048, all within `Limits::downlevel_defaults()`.

The size was not optimised. Of the 48 added bytes, 32 are the two clip rectangles and 16 the second colour. Two reductions are available if measurement shows the instance upload matters: colours as `Unorm16x4` (saves 16 bytes; clamps colours to `0..=1`), and a clip table indexed per instance (saves about 28 bytes; needs a data texture read in the vertex shader). Neither was done because there is no measurement yet that asks for it; at 1,000 instances the buffer is 112 KB per frame against 64 KB before.

Operations entirely outside the current clip are dropped on the CPU while the instance buffer is built, so content scrolled out of a viewport costs no instances and no fragments.

## Shader approach

- **Rectangle and border.** One signed distance `d` to the rounded box. Outer coverage is `clamp(0.5 - d)`; inner coverage is `clamp(0.5 - (d + border_width))`, which is the exact inset shape, including the inner corner radius `r - width` and square inner corners once the border is wider than the radius. The pixel is split into fill-only (`inner`), border-over-fill (`outer - inner`) and empty, and composited in that one expression. Both edges are anti-aliased, and because the fill is never blended separately under the outer edge it cannot leak there as a fringe. With no border the expression reduces to the previous one, so existing output is unchanged.
- **Gradient.** The position along the gradient line is computed per vertex (it is linear in position, so interpolation is exact), clamped in the fragment shader and used to `mix` the two premultiplied colours.
- **Shadow.** After Evan Wallace's "Fast Rounded Rectangle Shadows": the Gaussian is separable, the x integral over one row of a rounded box is closed-form (two `erf`), and the y integral is taken numerically only over the rows the box occupies. The midpoint rule of the original is replaced with 6-point Gauss–Legendre quadrature, which measured about as accurate as a 12-point midpoint rule at half the samples (worst error against a brute-force reference, before the tail shift described next: 1.8/255, against 1.7/255 for 12 midpoints, 4.6/255 for 6 and 9/255 for 4; 2.1/255 as shipped). `erf` is the Abramowitz–Stegun 7.1.27 approximation. The quad extends three standard deviations beyond the silhouette, and the result is shifted down by the Gaussian tail at that distance so the shadow reaches zero at the quad's edge rather than ending in a step. A blur below 0.25 px is drawn as a plain rectangle.
- **Clip.** The fragment's pixel position is an interpolated varying. The rectangular clip contributes the exact fraction of the pixel inside it (per-axis interval overlap, correct even for a clip thinner than a pixel); the rounded clip contributes the same distance-based coverage as a rounded rectangle's edge. Both multiply the colour of every kind, glyphs included.
- **Portability.** No derivatives, no implicit-level sampling (the atlas is sampled unconditionally with `textureSampleLevel`), so the branches on the per-instance kind are valid in non-uniform control flow. No `@builtin(position)` in the fragment shader, so nothing depends on a backend's framebuffer orientation. No arrays, no loops, no extra bindings or features.

## Partial atlas upload

- `morningstar-text`'s atlas keeps a dirty row range and the generation it is relative to. `TextContext::begin_frame` starts a new record (`Atlas::mark_clean`), since by then the renderer has had the chance to upload the previous frame. Growing or clearing the atlas marks the whole image as replaced (`dirty: None`) until the next frame.
- Generations now come from one process-wide counter, so a generation identifies one state of one atlas, and a renderer cannot mistake a view of a second atlas for a newer version of the first.
- The renderer uploads rows `start..end` at full width with one `write_texture` when the record applies, and otherwise everything. A changed size always means a new texture and a full upload. A malformed record is ignored.
- `Renderer::last_atlas_upload()` and `OffscreenRenderer::last_atlas_upload()` report what was uploaded (`AtlasUpload::None | Rows { start, end } | Full`), so the effect can be confirmed on a device.
- `OffscreenRenderer` is new: an offscreen renderer that keeps its device and atlas texture across frames, which the tests of this need. `render_offscreen` is now a thin wrapper over it.

A glyph added to a settled screen now uploads its shelf's rows (about 1 KB per row; two 32 px glyphs dirtied fewer than 64 rows in the test) instead of 512 KB to 2 MB.

## Verified

All on macOS/Metal, on the real GPU. 29 + 4 + 42 + 6 + 15 tests pass in `morningstar-core`, `morningstar-gpu` (unit, then `offscreen.rs` 11, `vocabulary.rs` 26, `atlas_upload.rs` 5) and `morningstar-text` (unit 6, `layout.rs` 15).

- The 11 existing pixel tests pass with only the new `dirty: None` field added to their two `AtlasView` literals; the 13 existing text tests pass unchanged.
- Borders: exact pixels for aligned strokes and fill-plus-border; half coverage at a half-pixel inner edge; a ring compared with its exact shape at every pixel, anti-aliased on both edges, with no fill fringe outside; translucent border over fill; square inner corners when the border is wider than the radius.
- Gradients: linear to within 1/255 across 256 columns; end colours held beyond the stops; a 45-degree gradient checked at every pixel; fade to transparent without hue shift; rounded silhouette identical to a solid fill's.
- Shadows: seven shapes (blur 4 to 40, card, pill, square corners, a box smaller than its blur, fractional geometry) compared at every pixel with a CPU Gaussian blur of the supersampled shape, within 2.5/255; monotonic and step-free falloff at blur 40; offset, spread and zero blur exact.
- Clipping: exact pixels for nested clips and unbalanced pops; every kind of operation clipped; a clipped glyph keeps its texels in place; half coverage at a fractional clip edge; a rounded clip pixel-identical (±1) to the rounded rectangle; rounded inside square and square inside rounded give identical output; the nested-rounded limit is pinned by a test.
- Atlas: the GPU's copy of the atlas is read back after each step. Rows only are uploaded when the record applies (shown by an undeclared change *not* reaching the GPU); full uploads on first sight, a missed frame, an out-of-range record, growth, shrink and clear.
- By eye (sRGB-encoded dumps of `card_scene` and `shadow_sheet` in `vocabulary.rs`, written when `MORNINGSTAR_DUMP_DIR` is set): card shadows at blur 4, 12, 24 and 40, gradients at four angles, hairline, thick and translucent borders, a rounded card clipped by a viewport.

Not verified:

- **Vulkan and GLES.** The shader passed wgpu's validation on Metal only. It was written to the constraints above but has not been compiled to SPIR-V or GLSL, and has not run on the Android device.
- **The frame-time effect of partial upload.** The mechanism is tested; whether it removes the p95 regression of ADR 0009 has to be measured on the phone.
- **The cost of the larger instance and the heavier fragment shader** (a shadow fragment evaluates six rows, each with two `erf`, one `exp` and one `sqrt`). No frame-time measurements were taken.

## Known limitations

- **Two gradient stops, linear only.** No multi-stop, radial or conic gradients.
- **Gradient with a border is two operations** (`fill_gradient_rect`, then `stroke_rounded_rect`), because an instance has two colours. Drawn that way the fill can show faintly through the anti-aliased outer edge of rounded corners.
- **Gradients are not sRGB-interpolated**, so they do not match CSS or design tools exactly (see above).
- **No dithering.** Wide gradients and large faint shadows can band on 8-bit surfaces. (The linear RGBA8 offscreen target bands much more in dark tones than an sRGB surface does; do not judge banding from it.)
- **Shadows are not knocked out** under the shape casting them: a translucent card shows its own shadow through itself. No inset shadows.
- **Shadow accuracy** is about 2/255 of the shadow's own alpha at worst, at rounded corners under a small blur.
- **Nested rounded clips**: only the innermost is rounded (see Decisions).
- **Clip coverage multiplies shape coverage.** Where a clip edge and a shape edge coincide at the same sub-pixel position, the edge pixel is slightly too transparent. Pixel-aligned edges are exact.
- **Uniform corner radius and border width.** No per-corner radii, per-side borders or dashes.
- **Atlas uploads are whole rows**, 1024 texels wide, not the glyph's rectangle.

## What an image operation would need

Images were deliberately left out. They need:

- **Texture resource management**, which does not exist: handles in `morningstar-core` (plain IDs, like `AtlasView` is plain data), creation, upload and eviction in `morningstar-gpu`, decoding somewhere that is not the render thread, and a lifetime rule across surface loss (the device survives; textures should too).
- **A way to bind more than one texture.** With one draw call there is one bind group. Either an image atlas or texture array shared by all images (keeps the single draw call; limits image size and needs packing and mip handling), or splitting the instance stream into consecutive draw calls wherever the bound texture changes (keeps painter's order; costs draw calls). The second is the smaller step and what the instance buffer already allows: a run of instances is a range.
- **An RGBA sampling kind in the shader**, with premultiplied data, UV rectangle in `extra`, tint in `color`, and the existing rounded-box coverage and clip applied unchanged. That gives rounded and clipped images for free. Colour emoji (ADR 0009) want the same thing.
- **Filtering and mipmaps**, since images are scaled where glyphs are not, and sRGB texture formats so sampling is linear.
