# 0029 — Turned text: glyphs rasterised for their transform and placed in screen space, unless the transform is changing

Status: Accepted (2026-11-12). Verified on the development host only (macOS, Metal, offscreen). Nothing here has run on a device and nothing here has been timed.

Closes the "rotated text is a resampled bitmap" approximation of [0017](0017-transforms-and-vector-pictures.md) for text. Extends [0009](0009-text.md) (glyphs as display-list operations), [0022](0022-text-metrics-and-pixel-snapping.md) (sub-pixel pen positions) and [0017](0017-transforms-and-vector-pictures.md) (transforms as display-list state). It changes no decision of theirs.

## Context

A transform is carried per instance and applied by the GPU ([0017](0017-transforms-and-vector-pictures.md)). For a rectangle that is exact: the shape is evaluated in its own space. A glyph is a bitmap, rasterised upright; turned, it is that bitmap resampled bilinearly, softer than a browser's text, which rasterises the outline at its angle. On CallReady, a design full of tilted labels, that softness was most of what still differed from the Chrome references on three of five screens.

## Decisions

### The text engine rasterises for the transform it is drawn through

`DisplayList` now knows the transform an operation recorded at this moment will be drawn through (`current_transform()`: the composition of the open `push_transform`s, kept as they are pushed and popped). `TextContext::draw_spans` reads it. If it does more than move (it turns, slants or scales), each glyph is:

1. **rasterised from its outline through the transform's linear part** (`zeno`, the rasteriser the upright glyphs come from, on the same unhinted outline, with the synthetic slant if the glyph has one), for the fraction of a pixel its pen lands on;
2. **placed where the transform takes its pen**: the pen position upright text has (the browser's baseline rule and the fractional advance of [0022](0022-text-metrics-and-pixel-snapping.md), in the text's own space), put through the full transform, then split into whole pixels and a quarter-pixel bin on **both** axes;
3. **drawn in screen space**: between a new pair of operations, `PushScreenSpace` / `PopScreenSpace`, inside which the renderer applies no transform. The glyph quad is axis-aligned and maps texels to pixels one to one, exactly like upright text.

The mask is weighted for the text's tone as upright glyphs are ([0023](0023-compositing-space.md)) and lives in the glyph atlas under a key of (glyph, both bins, tone, linear part quantised to 1/4096). Everything stays in the one draw call.

**Both axes get a bin** because a turned baseline crosses pixel rows: measured on CallReady, whole-pixel vertical placement was worse on every screen that has turned text (0.0099, 0.0161, 0.0401, 0.0111 percent against the figures below).

**Scale is included**, not only rotation and skew: text under `scale(2)` was an enlarged bitmap and is now rasterised at its size on screen. Above 160 px (font size times the transform's scale) a glyph is left to the renderer as before, as is any text under a transform that is not finite or flattens to nothing.

### `PushScreenSpace` in the renderer, without touching the shader

Inside it the open transform is set aside (and restored by the pop, with the clip, like a transform's own pop). Clips stay in force. A clip in screen space is exact, as always. A clip that was pushed *inside* a turning transform is kept in that turned space ([0017](0017-transforms-and-vector-pictures.md)) and cannot be evaluated for an untransformed instance, so for screen-space operations it acts as **its bounding rectangle on screen**: never cutting more than the exact clip, sometimes less, at the corners of a turned clipping box. This is the same degradation 0017 already defines for clips it cannot keep exact, and the price of leaving the instance layout and the shader alone. Turned text overflowing a turned, clipping card by a corner is the case it gets wrong; no screen has one.

### While the transform changes, resample; then one frame to settle

Rasterising for the angle means a new set of masks per angle. An animated rotation would put a text's worth of glyphs in the atlas every frame, each used once, until the atlas fills and is rebuilt. So:

- A text is rasterised for its transform **unless the same text (the same layout) was drawn in the previous frame through other linear parts and not this one**. That is a text whose angle or scale is changing. For that frame it is drawn the old way: upright glyphs, turned by the renderer.
- Such a frame **asks for one more** (`TextContext::take_settle_request`, which `Frame::draw_text` and `draw_text_spans` turn into a redraw request). If the transform has stopped, the next frame finds it in the previous one and draws the text sharp, and nothing more is asked. If it has not, the next frame is an animation frame anyway.
- A text that only **moves** keeps its linear part and stays sharp; its glyphs take new masks only for new pen fractions, at most sixteen per glyph and tone.
- A text seen for the first time is sharp at once, at any angle: a static screen is right in its first and only frame.

Consequences, accepted: during a rotation the text is as soft as it always was, and sharpens one frame after it stops (at 60 Hz, 17 ms later); the first frame of an animation that starts from a turned rest state is sharp at the old angle's cost of one set of glyphs. A text that changes its *content* every frame while also turning is a new text every frame and is rasterised every frame; nothing bounds that but the atlas rebuild, as for any text that changes every frame.

## Results

CallReady, share of pixels that differ from the Chrome reference (780 x 1688, default tolerance), before and after:

| Screen | Before | After | Limit in `tests/fidelity.rs` |
| --- | --- | --- | --- |
| 01 welcome | 0.0274% | 0.0095% | 0.05 (unchanged) |
| 02 home | 0.0209% | 0.0135% | 0.05 (unchanged) |
| 03 live call | 0.0064% | 0.0060% | 0.05 (unchanged) |
| 04 call report | 0.0591% | 0.0374% | 0.05 (was 0.10) |
| 05 premium | 0.0147% | 0.0076% | 0.05 (unchanged) |

Mean error fell on every screen with turned text (0.020 to 0.015, 0.044 to 0.042, 0.037 to 0.030, 0.019 to 0.017). The limits are the measured figure rounded up to the next 0.05, as before; only the report's moves. FundiDesk and the showcase have no turned text and are unchanged.

## Costs

- **Atlas**: a turned glyph can have sixteen masks (four by four pen fractions) where an upright one has four, per tone and per distinct angle. Measured on the five CallReady screens at 2 and 2.8125 px per dp: the atlas texture is the same size on every screen (1024 x 512 or 1024 x 1024 bytes), with 0 to 38 more rows of it in use (157 to 187 of 512 on the report at 2x, the largest change).
- **CPU**: one outline fill per new (glyph, fraction, angle), as for a new upright glyph, plus one matrix application per glyph per frame. The outlines are cached by the shaper's cache. Not timed.
- **Instances, textures, draw calls**: unchanged. Two more operations in the display list per turned text.
- **One extra frame** after a rotation or scale animation ends, where text is involved.

## Not done

- **Turned icons and pictures** are still resampled: a `VectorIcon` mask and a picture's image are rasterised upright. The same method applies (their paths are at hand) and was left out.
- **Outlined text** (`draw_text_outlined`) and the caret, selection and composing underline of a turned text field are untouched; the text of a turned field is sharp, its decorations are rectangles and exact already.
- **Exact clipping of screen-space glyphs by a turned clip** (above).
- **A browser comparison at angles other than the design's** (small tilts of 1 to 8 degrees, and 90).
- Nothing on a device: atlas use and frame time there, and how the one-frame sharpening looks at the end of an animation.

## Public API added

`morningstar-core`: `DrawOp::{PushScreenSpace, PopScreenSpace}`; `DisplayList::{push_screen_space, pop_screen_space, current_transform}`.

`morningstar-text`: `TextContext::take_settle_request`. `draw` and `draw_spans` keep their signatures.

`morningstar-runtime`: none; `Frame::draw_text` and `draw_text_spans` request a redraw when the text engine asks.

## Verified

- `crates/morningstar-text/tests/turned.rs` (8): upright and moved text is recorded exactly as before; an "l" turned a quarter lies on its side with the same ink, on whole pixels, where the transform puts it; a stem at 8 degrees has a solid middle between two graded pixels; glyphs step along a 30-degree baseline and take more than one mask for their fractions; a scale rasterises at the scaled size and the size limit and undrawable transforms fall back; forty frames of a changing angle draw upright glyphs under the transform, ask for a frame each time and add nothing to the atlas after the first, and the frame after is sharp and asks for nothing; forty frames of a slide stay sharp in an atlas that does not grow; one text at two angles in one frame is sharp at both.
- `crates/morningstar-gpu/tests/screen_space.rs` (3 pixel tests): operations in screen space are unturned and sharp under a 45-degree transform, which is back afterwards, in one draw call; a screen clip is exact and a turned clip acts as its bounds; nesting with layers, unmatched pushes and pops, an undrawable transform.
- `crates/morningstar-core` (1): the current transform through pushes, pops, screen space and `clear`.
- `tools/morningstar-devtools/tests/turned_text_picture.rs` (1, through `Frame` on the GPU): the frame whose angle changed asks for a redraw and the next does not; the settled frame has 37% fewer in-between pixels (1630 against 2574) for the same ink; both pictures looked at.
- `crates/morningstar-ui/tests/transforms.rs`: the turned-tile test now expects the glyphs in screen space and every other operation unchanged.
- CallReady's fidelity limits as above; its end-to-end tests (which assert idleness) pass.
