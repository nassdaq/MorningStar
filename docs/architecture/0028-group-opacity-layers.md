# 0028 — Group opacity: display-list layers drawn into an offscreen target, used only where a faded group overlaps itself

Status: Accepted (2026-11-12). Verified on the development host only (macOS, Metal, offscreen). Nothing here has run on a device, on Vulkan or on GLES, and nothing here has been timed.

Supersedes the "opacity is multiplied into every colour drawn" decision of [0021](0021-viewport-wrap-scroll-animation-navigation.md) and its "no group opacity" limit. Extends [0011](0011-renderer-vocabulary.md) (the one draw call) and [0014](0014-images.md) (what splits it).

## Context

`.opacity(x)`, and the opacity of every transition, were multiplied into each colour of the subtree. For one primitive that is exact. For a card with a label it is not: at 50% the card's background shows through the label, the card through anything laid over it, a shadow through the box that casts it. 0021 recorded this as right for a quick fade and wrong for holding a group at 50%, and left the offscreen layer out.

## Decisions

### `PushLayer { opacity }` / `PopLayer` in the display list

`DisplayList::push_layer(opacity)` and `pop_layer()` bracket a group. What is between them is drawn as it would be without them (clips and transforms open around the layer apply to its content; those pushed inside must be popped inside), composed by itself, and then shown at `opacity`. Layers nest. An opacity of 1 or more is no layer at all; 0, negative or NaN draws nothing until the pop, at no cost; a pop without a push is ignored and a layer left open ends with the list.

### The renderer: one target per nesting depth, passes split at layer boundaries

- A layer is drawn into an offscreen target of the surface's size and format, cleared to transparent. Because the target has the surface's size, the content's instances are exactly the ones it would have had: same positions, same clips, same transforms, same shader. Nothing is re-laid-out for the layer.
- When the layer ends, the target is blended into its parent (the frame, or the layer around it) as one instance of the existing image kind: an untransformed, unclipped quad over the **bounding rectangle of what was drawn into the layer** (tracked on the CPU from the instances' painted bounds and clips, snapped outwards to whole pixels), sampling the target one texel per pixel, tinted by the opacity. A layer that drew nothing blends nothing.
- Targets are kept **per nesting depth, not per layer**: two faded cards side by side use the same target one after the other. That is possible because the frame's instance stream is cut into render passes at layer boundaries, in list order: what precedes a layer goes to its target first; then the layer's content to the layer's target; then the parent carries on, loading what it had, starting with the blend. So a layer costs **two more render passes** (its own, and the continuation of its parent), **one more instance** and at least one more draw call; a frame's layer memory is `depth * width * height * 4` bytes, whatever the number of layers.
- Targets are created when a frame first needs them and **freed by the first frame that does not**: a frame without layers holds none.
- Nesting deeper than eight draws the deeper layers into their parent without their own opacity (eight surface-sized targets is the bound on memory). No screen nests fades that deep.

### A frame without layers is rendered exactly as before

With no `PushLayer` in the list the pass structure is one pass with the same commands in the same order: clear, pipeline, globals, and per run the image bind group, the instance slice and the draw. The instance layout, the shader and the pipeline are unchanged; there is no new uniform and no per-instance field. Tested: such a frame reports one pass and one draw call, a layer at opacity 1 gives the same statistics and the same bytes, and all 107 earlier pixel tests of `morningstar-gpu` pass unmodified, as do the FundiDesk, CallReady and showcase fidelity figures.

### The UI layer: a layer only where the faded widget overlaps itself

`.opacity(x)` and transitions now mean group opacity. The rule that decides how it is drawn, evaluated per faded widget per frame (`overlaps_itself` in `ui.rs`):

- **A layer**, when the widget's opacity is below 1 and it paints things that lie over one another: a box with a background or border and something painted inside it; a box over its own shadow; a stack with more than one painted child; or any such box further down the subtree.
- **Colour by colour, as before**, otherwise: a text, a picture, an icon, a box that is one primitive (a fill with its border), a row or column of those. For these the two are the same picture, and the cheap one costs nothing.

When a widget takes a layer, the opacity is *not* multiplied into the subtree's colours; a faded widget inside it that does not overlap itself still fades colour by colour within the group. The layer is pushed inside the widget's own transform and popped before it. The check walks the faded widget's subtree, so it is proportional to the size of what is being faded, and is not made at all for widgets at full opacity.

The consequence for transitions: a screen that slides or cross-fades through `Navigator` is one full-screen layer while it moves (three passes, or five for a cross-fade of two screens), where it used to be none. That is the price of the fade being right; **what it costs in frame time on a phone is not measured.** A list whose rows each fade in separately takes a layer per row that overlaps itself, two passes each. If that proves too much on a device, the place to change is the rule above (for instance, colour by colour for transitions shorter than some duration), not the renderer.

## Costs

- **Memory**: `width * height * 4` bytes of GPU texture per nesting depth in use, only in frames that have layers: 10.4 MB for one level on a 1080 x 2400 surface. None otherwise. No change to instance size (144 bytes), the atlas or image textures.
- **GPU work per layer**: a clear of the target, the group's own instances (which would have been drawn anyway), and one blended quad the size of the group's bounding rectangle. On a tile-based mobile GPU each extra pass is a tile flush; unmeasured.
- **CPU**: per frame with layers, a few comparisons per instance for the bounds; the UI's overlap check per faded widget.
- **Precision**: a layer holds 8-bit premultiplied values before it is faded, so a layered group can differ from the same colours faded directly by one level (tested).

## Not done

- **Layer targets are the size of the surface**, not of the group. A target the size of the group's bounds would save memory and fill for small groups, at the cost of re-basing every instance inside it.
- **No blend modes, no backdrop effects, no masks**: a layer is opacity only.
- **The overlap rule does not look at geometry.** Two siblings moved over each other by `offset` or a transform inside a row are not seen as overlapping; a card whose only child happens to be invisible takes a layer it did not need. Text with a hard shadow or an outline is treated as one primitive.
- **Pictures** (`VectorPicture`, `SvgPicture`) composite their own groups on the CPU ([0017](0017-transforms-and-vector-pictures.md), [0027](0027-svg-loading.md)) and do not use this.
- Nothing on a device: Vulkan and GLES with several passes per frame and a sampled render target in the surface's format, frame time during a screen transition, memory.

## Public API added

`morningstar-core`: `DrawOp::{PushLayer, PopLayer}`; `DisplayList::{push_layer, pop_layer}`.

`morningstar-gpu`: `RenderStats::{passes, layer_textures, layer_texture_bytes}`. No `wgpu` type appears in the public API.

`morningstar-ui`: no new API; `.opacity()` and transitions changed meaning as above.

## Verified

- `crates/morningstar-gpu/tests/layers.rs` (11 pixel tests, on Metal): two overlapping rectangles in a layer at 50% show the upper one alone in the overlap (`[0, 0, 128]`), where colour by colour gives `[64, 0, 128]`; a frame without layers is one pass and one draw call, and a layer at opacity 1 is byte-identical to none; passes, instances and targets with one layer, with two side by side (one target), and none the frame after; paint order around a layer; nested layers; a clip around a layer and a rounded clip inside one; a transform around and inside (identical bytes) and a 30-degree turn; every operation kind inside a layer, including an image; one primitive in a layer within one level of its colour faded; opacity 0, negative and NaN, an unmatched pop, an unclosed layer, a layer that draws nothing; nesting past eight.
- `crates/morningstar-core` (1): the operations recorded.
- `crates/morningstar-ui/tests/layers.rs` (6): the rule, as display lists: a faded card with a label is one layer with whole colours; a text, a box, a bordered box and a row are faded colour by colour with no layer; a box on its shadow and a stack are layers and a stack of one painted child is not; a faded group far down makes its faded ancestor a layer; the layer sits inside the widget's transform; nothing at full opacity. `tests/adaptive.rs`: the two screen-transition tests read the opacity a fill is shown at through its layers (their helper changed; their expectations did not).
- `tools/morningstar-devtools/tests/layers_picture.rs` (1): one picture of the difference, looked at.
- FundiDesk, CallReady and showcase fidelity and regression suites: unchanged.
