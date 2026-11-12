# 0017 — Affine transforms, multi-colour vector pictures, and four text effects

Status: Accepted (2026-11-12). Verified on the development host only (macOS, Metal, offscreen). Nothing here has run on a device, on Vulkan or on GLES, and nothing here has been timed.

Extends [0011](0011-renderer-vocabulary.md) (renderer vocabulary: the instance grows again, and clips gain a second space), [0013](0013-ui-layer.md) (UI layer), [0014](0014-images.md) (images: pictures are drawn through that path) and [0015](0015-fonts-icons-and-design-fidelity.md) (one-colour icons; this adds the multi-colour case it refused). It changes no decision of theirs.

## Context

A second real design (`examples/callready`, five screens) uses things the renderer could not draw:

- `transform: rotate(-8deg)`, `rotate(5deg) translateY(-8px)`, `rotate(135deg)` on filled boxes, on bordered boxes with text inside, on text and on icons: about thirty occurrences;
- `font-style: italic` in a family that has no italic face, which a browser synthesises by slanting the glyphs;
- inline SVG illustrations with several fills and strokes, `stroke-opacity`, a `clip-path` and per-element `transform`s (a 250 dp disc of concentric rings with a rotated band cut to a circle), plus shapes CSS draws that the box model here cannot: an ellipse outline, a `repeating-linear-gradient` striped square;
- `text-shadow: 3px 3px 0 #fff` (no blur), `-webkit-text-stroke: 1.5px #fff` on one large digit, and `font-variant-numeric: tabular-nums`.

Until now every operation was an axis-aligned rectangle in pixels, an icon was one coverage mask in one colour ([0015](0015-fonts-icons-and-design-fidelity.md) refuses a second paint by name), and text was glyph masks at whole-pixel positions.

## Decisions

### A transform is display-list state, carried per instance

`morningstar-core` gains `Transform`, a 2x3 affine matrix (`a b c d tx ty`, the convention of CSS `matrix()`; `y` down, so positive rotation is clockwise, as in CSS), and the display list gains `PushTransform { transform }` / `PopTransform` (`DisplayList::push_transform`, `pop_transform`). The transform is in the same physical-pixel space as rectangles: an operation is recorded exactly as before, and each of its points then goes through the transforms open around it, innermost first. `Transform::about(point)` gives rotation about a centre. Quarter turns are exact.

Two ways of combining transforms are offered because both are needed and they read in opposite orders: `a.then(b)` (move by `a`, then by `b`, in the fixed frame) and the `and_*` builders, which append a function as a CSS `transform` list does (`Transform::rotate(5.0).and_translate(0.0, -8.0)` is `rotate(5deg) translate(0, -8px)`; tested against the matrix product).

**It stays one draw call.** The GPU crate composes the open transforms on the CPU while it builds the instance stream and writes the result into every instance. The vertex stage puts the quad's four corners through the matrix. The fragment stage is unchanged in kind: it still evaluates the rounded box, the border ring, the dash pattern, the gradient, the shadow integral and the atlas or image coordinates in the quad's own untransformed space, interpolated across the transformed quad. A turned shape is therefore the same shape, turned: corners, borders and dashes keep their geometry, a gradient turns with its rectangle (its points are in the same local space), and every edge keeps its one-pixel anti-aliased falloff. No stencil, no offscreen pass, no second pipeline, and images, which are instances of the same pipeline ([0014](0014-images.md)), need nothing of their own.

Three details make that exact rather than approximate:

- **Edge width under scale.** Distances in the fragment stage are in local pixels. Each instance carries the transform's linear scale as one number, `sqrt(|det|)`, and the shader multiplies distances by it before turning them into coverage, and divides the quad's one-pixel fringe by it. It is exactly 1 for rotations, slants and moves. For a transform that scales differently along two axes the falloff is a compromise between them; nothing in the design needs that.
- **Glyphs.** A glyph quad maps texels to pixels one to one and has no fringe, and a mask may be solid right up to its edge (the stem of an "l"). Under a transform that does more than move, glyph quads get the fringe, their atlas coordinates are extrapolated over it and clamped to the centres of the region's outermost texels, and the quad's own edge is anti-aliased like a rectangle's. Under identity or a move none of this is active.
- **Identity is free and exact.** With the identity matrix every changed expression reduces to the previous one bit for bit (`x * 1 + y * 0 + 0`, a distance times 1, a fringe divided by 1). A pixel test renders one of every operation kind with and without identity transforms and compares the bytes; all 69 earlier pixel tests of the GPU crate pass unmodified; and FundiDesk's twelve fidelity figures are identical to the fourth decimal (below).

A transform that is not finite or has a zero determinant draws nothing until it is popped. Operations wholly outside the clip are still dropped on the CPU, using the bounding box of the transformed quad.

### Instance layout: 144 bytes (was 112)

Nine `vec4<f32>` (was seven), 9 of 16 vertex attributes, stride 144 of 2048; eleven varyings (limit 16; 15 on WebGL2-class GLES). All within `Limits::downlevel_defaults()`.

| Location | Field | Contents |
| --- | --- | --- |
| 0 to 6 | | unchanged ([0011](0011-renderer-vocabulary.md), [0014](0014-images.md)) |
| 7 | `matrix` | `a, b, c, d`: the linear part |
| 8 | `placement` | `tx, ty`, edge scale, flags (1: the transform does more than move; 2: the shaped clip is in local space) |

At 1,000 instances the per-frame upload is 144 KB against 112 KB, a 29% increase, paid by every instance whether transformed or not. The alternative, a transform table indexed per instance, needs a data texture or storage buffer read in the vertex stage; [0011](0011-renderer-vocabulary.md) weighed the same trade for clips and declined it for lack of a measurement, and that has not changed. **The cost on a device is unmeasured**: the benchmarks are device-only ([benchmarks/README.md](../../benchmarks/README.md)) and no phone was touched for this work. The shader does one more 2x2 multiply per vertex and three more multiplies per fragment.

### Clips and transforms

[0011](0011-renderer-vocabulary.md)'s clip state is a screen rectangle intersected with at most one rounded rectangle. That state now has the shaped slot in one of two spaces.

- **A transform inside a clip** (a rotated tile inside a scroll view or a clipped card): the clip stays where it is on screen and cuts the transformed content. Exact for square and rounded clips, with no new mechanism: the fragment's screen position is what clips are tested against, as before.
- **A clip inside a transform** (a rotated card that clips its children) is transformed with the content. It is kept in the transform's local space, in the shaped slot (which now may hold a square rectangle too), and evaluated there by the fragment stage, so it is exact and anti-aliased at any angle. One such clip is exact at a time, the innermost; square ones directly inside one another intersect exactly.
- **What degrades**, always to a bounding rectangle on screen, never to nothing and never by clipping too much: a rounded screen clip while a local clip holds the slot; a local clip with a rounded local clip nested in it; a local clip with a further non-translating transform pushed inside it (its rectangle is not a rectangle in the new space). A clip pushed under a transform that only moves is a screen clip, moved, and a move pushed inside a local clip shifts it; both exact.

Doing the general case properly (arbitrary nesting of rounded clips in different spaces) needs a stencil or a clip mask texture and would split the draw call. The design has no such case.

Pushes and pops of clips and transforms must be properly nested with each other; popping a transform restores the clip state that was current when it was pushed.

### The UI layer: a modifier on every widget, about its centre, outside layout

Every widget (`Container` and all its presets, `Text`, `TextInput`, `Button`, `Icon`, `Picture`, `Avatar`, `Element`) gains `.transform(Transform)` and `.rotate(degrees)`. Lengths are dp; the transform is applied about the centre of the widget's laid-out rectangle (CSS's default `transform-origin`), to the widget and its whole subtree. **Layout does not see it**: the widget keeps its room, and the rectangles painted inside are the same ones, between a push and a pop (tested by comparing the operation lists).

A transform that only moves is not sent to the renderer at all: it becomes a whole-pixel translation of the subtree, the same mechanism scrolling uses, so text stays on the pixel grid and clipping, culling and hit-testing are untouched.

**Hit-testing follows the picture.** A touch region inside a transformed subtree records the inverse of the composed transform and the screen-space clip that was in effect outside the outermost transform; a touch is on it if the screen point is inside that clip and its inverse image is inside the region's (local) rectangle. Nested transforms compose. A tap on a text field inside a transformed widget places the caret through the same inverse. The cost is one optional 2x3 matrix per region and six multiplies per test for transformed regions only.

Limits, all documented on the method: inside a transformed subtree, clips pushed between two nested transforms no longer limit *touch* (they still limit what is drawn); scroll-view culling inside it is off (the cull rectangle is not a rectangle there); a `ScrollView` *inside* a transformed widget drags along the screen's axes; `Ui::rect_of` reports the layout rectangle. A subtree under a zero scale is skipped whole.

### Synthetic italic is done by the rasteriser, not by the transform

`TextStyle::italic` (and `TextSpan::italic`; `Text::italic()`, `Span::italic()` in the UI) asks the shaper for an italic face. When the family has none, `cosmic-text` marks the glyphs and its rasteriser slants the outlines by 14 degrees (tan 14° = 0.2493; Skia uses 0.25) before filling them, which is what a browser does. The slanted glyphs are rasterised sharp at their final shape, per glyph about its own baseline, with unchanged advances, so layout is identical to the upright text (tested: same measured size, stem leaning by a quarter of its height). This is better than the alternative considered, shearing upright glyph bitmaps on the GPU with `Transform::shear_x(-0.25)`, which resamples them and needs the baseline of each line; that remains possible and is tested as a renderer capability, but no screen should use it for italics.

### Multi-colour vector pictures: composited on the CPU into one image

`morningstar-text` (the crate that owns `zeno`) gains `VectorPicture`: a view box and a `'static` slice of `Layer`s. A `Layer` is a `Shape` (`Path` in SVG `d` syntax, `Circle`, `Ellipse`, `Rect` with a radius, `Line`) with an optional fill colour and fill rule, an optional stroke (colour, width, cap, join), an opacity, an optional transform (a matrix, or `rotate(deg, cx, cy)` kept symbolic so it can be written in a constant) and an optional clip shape. Every builder is `const`, so a picture is a constant, like an icon. It is the vocabulary of the SVG it is transcribed from, written in Rust; there is no SVG parser for pictures (see "Not done").

`VectorPicture::rasterise(width, height)` fits the view box uniformly, centred, and composites the layers in order into a `width * height` RGBA image:

- each layer's fill, stroke and clip are `zeno` coverage masks at device pixels;
- the layer's colour at a pixel is its stroke over its fill, then multiplied by the layer's opacity and the clip's coverage (so `opacity` is group opacity, as in SVG), then composited over what is there;
- all of it in **linear light with premultiplied alpha in `f32`**, the space the pipeline blends in, and stored as the image store keeps images (sRGB-encoded colour premultiplied in linear light), through the new `ImageStore::add_premultiplied_rgba8_vec` and `srgb_encode`, so there is no round trip through straight alpha;
- a clip is in view-box space and is not moved by the layer's transform (the design's rotated band inside an unrotated circle); whatever lies outside the view box is cut off.

`Frame::draw_picture(&picture, rect, corner_radius, tint)` snaps the rectangle to whole pixels, rasterises for exactly that many, registers the image and draws it with the existing image operation. In the UI it is `Picture::vector(PICTURE)`, which takes sizing, `radius`, `tint` and the transform modifier like any picture; its natural size is its view box in dp.

**Why an image and not coverage masks in the glyph atlas.** Drawing each layer as a tinted mask quad would keep pictures in the one draw call and blend on the GPU. It was rejected because of size: the disc is about 700 px across on a phone and has about twenty layers, most of them rings spanning the whole picture, which is some 10 MB of single-channel masks in an atlas that is 0.5 to 2 MB and is rebuilt when it fills. One RGBA image is 2 MB. A layer clipped by another shape would also need a mask per (layer, clip) pair.

**Costs, recorded:**

- Memory: `4 * w * h` bytes in the frame's image store plus a third more on the GPU with mipmaps: 1.96 MB + 2.6 MB for the 700 px disc, 16 KB + 22 KB for a 64 px one.
- Draw calls: a picture is an image, and consecutive instances sampling different images are different draw calls ([0014](0014-images.md)). A screen with four distinct pictures is four draw calls instead of one, and more if other images are painted between them. The quad pipeline and the instance stream are still single; nothing but a change of image splits them, and a transform never does (tested).
- Time: rasterisation is on the frame-building thread, once per (picture, pixel size): one coverage mask per fill, stroke and clip, and one pass over the image per layer. Not timed, on the host or on a device.
- A picture under a rotating `transform` is the upright raster resampled, slightly softer than a browser's (which rasterises the rotated geometry). To turn a picture sharply by a fixed angle, rotate its layers instead.

**Cache and eviction.** `Frame` keeps rasterisations keyed by (content hash of the picture, pixel width, pixel height); the hash covers every number, colour and path string, so two constants with the same content share one image and a value needs no identity. An entry is dropped, with its image and so its texture, once 120 frames have been built without drawing it (frames are demand-driven, so a picture scrolled out and back during a fling is not rasterised again, while one left behind by navigation goes after some interaction), and at once, least recently drawn first, whenever the cache holds more than 32 MiB that the previous frame did not draw. What a frame draws is never evicted under it. Pictures the UI knows are clipped out of view are not rasterised. Sides above 2048 px are rasterised at 2048 and stretched.

### Text effects

- **Hard shadow** (`Text::shadow(x, y, colour)`): the same glyphs drawn once more first, moved by whole pixels, all in the shadow's colour. No blur exists, and none is implied by the name.
- **Outline** (`Text::outline(width, colour)`, `Frame::draw_text_outlined`): glyph outlines are available from the shaper's scaler for free, so `TextContext::rasterise_outlined` collects them for a laid-out text and paints them with the picture compositor: fill in the text's colour, then a stroke centred on the outline with mitre joins, which is `-webkit-text-stroke`'s geometry. The result is an image placed where the glyphs would be and cached like a picture, keyed by text, style and stroke. Meant for a digit or a word. Per-span colours are ignored.
- **Tabular numbers** (`TextStyle::tabular_nums`, `Text::tabular_nums()`): the OpenType `tnum` feature, passed to the shaper. It matters for this design: its text face (Outfit) has proportional digits by default (a "1" is 347 units wide, a "0" 656), so a right-aligned score or a timer is laid out differently without it. A font without the feature is unaffected.

## Not done, and approximations

- **Rotated text and pictures are resampled bitmaps.** Glyphs are rasterised upright and turned by bilinear sampling, so text at an angle is a little softer than a browser's. Rasterising glyphs per angle would need the angle in the atlas key; not attempted. Small angles (the design's ±1 to 8 degrees) look right by eye on the host.
- **Shadows under a non-uniform scale** keep a circular blur in local space, so the blur is stretched with the shape.
- **No perspective, no `transform-origin` other than the centre** in the UI (the display-list `Transform::about` takes any point).
- **No SVG parser for pictures.** A picture is transcribed by hand into layers; `<g>` attributes are repeated on each layer and `stroke-opacity` / `fill-opacity` become the colour's alpha. Gradients, patterns, masks, dashes and text inside a picture do not exist. A `repeating-linear-gradient` is written as a fill plus stroked lines (recipe in `docs/building-screens.md`).
- **Pictures are `'static`.** A picture computed at run time (a progress ring) is not expressible as one; draw it with boxes, or as several constants.
- **Clips: one exact local clip at a time** (above).
- **Outlined text ignores span colours and is not hit-tested differently**; its image extends past the text's box by the stroke, like any overflow.
- **`cargo ndk clippy` and the iOS type-check were not run** for this change (disk). The change contains no platform-specific code.

## Public API added

`morningstar-core`: `Transform` (`IDENTITY`, `matrix`, `translate`, `scale`, `rotate`, `skew_x`, `skew_y`, `shear_x`, `then`, `and_rotate`, `and_translate`, `and_scale`, `and_skew_x`, `about`, `with_translation_scaled`, `apply`, `inverse`, `determinant`, `bounding_rect`, `is_identity`, `is_translation`, `is_finite`); `DrawOp::{PushTransform, PopTransform}`; `DisplayList::{push_transform, pop_transform}`; `ImageStore::add_premultiplied_rgba8_vec`; `srgb_encode`.

`morningstar-text`: `VectorPicture` (`new`, `with_view_box`, `width`, `height`, `layers`, `check`, `content_hash`, `rasterise`), `Layer` (`new`, `path`, `circle`, `ellipse`, `rect`, `rounded_rect`, `line`, `fill`, `even_odd`, `stroke`, `cap`, `join`, `round`, `opacity`, `transform`, `translate`, `rotate`, `clip`, `clip_circle`, `clip_rect`), `Shape`, `LineCap`, `LineJoin`, `MAX_PICTURE_PIXELS`; `TextStyle::{italic, tabular_nums}` (fields and builders), `TextSpan::italic`; `TextContext::rasterise_outlined`, `OutlinedText`.

`morningstar-runtime`: `Frame::{draw_picture, draw_text_outlined, picture_cache_bytes, picture_cache_len}`; re-exports of `Transform`, `VectorPicture`, `Layer`, `Shape`, `LineCap`, `LineJoin` (also from `morningstar`).

`morningstar-ui`: `.transform(..)` and `.rotate(..)` on every widget; `Picture::vector`; `Text::{italic, tabular_nums, shadow, outline}`; `Span::italic`; the types above in the prelude.

`TextStyle` and `TextSpan` gained public fields. Code that builds either with a complete struct literal must add them; code that starts from `TextStyle::new` or `TextSpan::new` (everything in this repository outside the UI layer) is unaffected. No `wgpu` type appears in any of it, and no dependency was added: `zeno` and `cosmic-text` were already there. `morningstar-ui` gained a *dev*-dependency on the repository's own `morningstar-devtools`, for one rendered test.

## Verified

All on macOS; GPU tests on Metal through the offscreen renderer. 640 tests pass in the workspace without `examples/callready` (563 before; 77 new).

- `morningstar-core` (8 new): rotation direction and exact quarter turns, `then` against CSS list order, `about`, inverse, shear, bounding rectangle; the two operations recorded; premultiplied registration and `srgb_encode`.
- `morningstar-gpu` unit (5 new): composition order, undrawable transforms, a clip under a move, a clip under a turn (local, square-in-square exact, rounded takes the slot), a local clip following a move and flattening under a further turn. The four earlier clip tests pass with the new signature.
- `morningstar-gpu` `tests/transforms.rs` (18 new pixel tests): a rect turned 45 degrees covers a diamond (inside, outside, an anti-aliased edge value, area within 1%); a quarter turn maps every pixel exactly; a half turn equals the reversed gradient and border within one level; a turned rounded bordered rect against the shape evaluated at each pixel's inverse-rotated centre, within 2/255, everywhere; a glyph quad's texels at 90 and 180 degrees; a turned solid glyph keeps its area and has a graded edge; an image turned a quarter, pixel for pixel; **identity changes no byte** across one of every operation kind; a whole-pixel move equals drawing there; nesting; a scale of 2 keeps edges on the grid and doubles a border; a shear by rows; a square and a rounded clip outside a transform; a square, a nested square and a rounded clip inside one; undrawable transforms; one draw call and CPU culling with transforms.
- The 69 earlier pixel tests of `morningstar-gpu` (`offscreen.rs` 11, `vocabulary.rs` 30, `images.rs` 23, `atlas_upload.rs` 5) and the runtime's image tests pass unchanged.
- `morningstar-text` (11 unit tests in `picture.rs`, 5 in `tests/effects.rs`): fill, paint order, opacity in linear light stored premultiplied, group opacity of fill and stroke, a centred stroke with butt and round caps, a clip that ignores the layer's transform, a layer rotated about a point, anti-aliased curves and a circle's area, view-box fitting and cutting, bad layers reported and skipped, the content hash; italic without an italic face (same advances, stem leaning 0.25 of its height), an italic span, outlined lettering (stroke, fill, stroke across a stem; placement against the plain glyph), nothing to outline, tabular digits with Inter and a font without the feature.
- `morningstar-runtime` `tests/pictures.rs` (11 new, on the GPU): fills, strokes and paint order; opacity and `stroke-opacity` blending; a turned band cut by an unturned circle; round caps and a layer transform; pixel snapping; a picture under a transform with a tint; one rasterisation per (content, size); eviction after idle frames and over budget; the 2048 px limit; outlined text against the plain text's position.
- `morningstar-ui` `tests/transforms.rs` (15 new): layout unchanged by a turn; the same operations between a push and a pop, about the centre; dp lengths and CSS order; a pure move needs no transform; a flattened subtree; a turned button tapped where it is drawn and not where it was laid out, slide-off, an outer clip, nested turns; italic, shadow and outline in the display list; a vector picture's sizing, one image operation, radius, tint and transform, and no rasterisation out of view. `tests/rendered.rs` (1 new) renders a screen using all of it on the GPU, asserts pixels of the turned tile, the disc (transcribed from the design's SVG) and the outlined digit, and writes the image when `MORNINGSTAR_DUMP_DIR` is set; it was inspected by eye beside the design's Chrome rendering.
- **FundiDesk fidelity is unchanged**: all twelve screens measure exactly what `examples/fundidesk/tests/fidelity.rs` records (0.2868, 0.3615, 0.1671, 0.0953, 0.1398, 0.1772, 0.4091, 0.2576, 0.4870, 0.3425, 0.2672, 0.2666 percent). The showcase's visual-regression references pass.
- `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` are clean.

## Not verified

- Anything on a device: Vulkan, GLES, the larger instance on a mobile GPU, frame times, picture rasterisation time in a release build, memory with pictures.
- The Android and iOS cross type-checks.
- The design's five screens themselves, which are built on this by others.
