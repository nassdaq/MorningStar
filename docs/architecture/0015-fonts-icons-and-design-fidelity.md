# 0015 — Font families, vector icons, letter spacing, dashed borders, and what a real design measured

Status: Accepted (2026-11-11). Verified on the development host only (macOS, Metal, offscreen). Nothing here has run on a device, on Vulkan or on GLES.

Extends [0009](0009-text.md) (text), [0011](0011-renderer-vocabulary.md) (renderer vocabulary) and [0013](0013-ui-layer.md) (UI layer). It removes three limitations those records list: "one font family", "no vector icons" and "no dashes", and part of a fourth ("one style per call").

## Context

The framework's purpose is to turn a design into Rust that renders the same screen. The first real test was FundiDesk (`examples/fundidesk`): twelve screens exported from a design tool as HTML, with a Chrome rendering of each as the reference. Surveying the twelve files before writing any screen showed what the UI layer could not say:

- two typefaces (Inter for text, Space Grotesk for headings and figures), where text could only use "the fonts that were added, matched by weight";
- 48 inline SVG icons, where there was a glyph-on-a-tile stand-in;
- `letter-spacing` on headings and small-caps labels;
- sentences with a bold or coloured phrase inside them, on six of the twelve screens;
- three dashed outlines;
- line heights given as lengths (`line-height: 22px`).

Each was closed in the framework. What was approximated instead is listed under "Not done".

## Decisions

### Font families are named; the weight is matched within the family

`TextStyle` gains `family: FontFamily` (`Default` or `Named(&'static str)`) and the layout cache key includes it. A named family is handed to the shaper as such; `Default` keeps the previous behaviour exactly (the fonts in the order added), so existing applications and tests are unaffected. Names are matched as the font file names itself, ignoring case; the typographic family name is used, so `Inter-SemiBold.ttf` is "Inter" at weight 600, not a family of its own.

**Fallback is kept.** A character the named family lacks is taken from the other fonts added, in order, and a family nobody added falls back the same way (tested: Arabic in a paragraph set in Inter). `Setup::has_font_family` exists because that fallback is silent.

The name is a `&'static str` so that `TextStyle`, and the UI's text nodes, stay `Copy` and constructible in constants. A family chosen at run time from a string that is not `'static` is not expressible; nothing has needed it.

In the UI layer a `Theme` gains `fonts: Fonts { body, heading }`. `display`, `title` and `headline` text takes `heading`, the rest `body`, and `Text::family(..)` overrides either. `Theme::LIGHT` and `DARK` name no family.

### Rich text: spans within one paragraph

`TextContext::draw_spans` / `measure_spans` take `&[TextSpan]`, each span optionally overriding weight, colour and family; size, line height, alignment and letter spacing belong to the paragraph. The spans are shaped as one text, so a paragraph wraps inside a span as it would anywhere. Per-span colour is stored per glyph in the cached layout and is part of the cache key. The UI has `Text::rich([span("..").semibold().color(..), ..])`.

Not rich text in general: no mixed sizes, no inline boxes, and **a span cannot be a tap target** (FundiDesk's "Create an account" link makes its whole line tappable).

### Letter spacing is part of the text style

`TextStyle::letter_spacing` in pixels (dp in the UI: `Text::letter_spacing`), with the CSS meaning: added after every character, the last included, so it changes the measured width. `Text::uppercase()` is a string operation, as `text-transform` is. `Text::line_height_dp` takes a line height as designs give it.

### Baselines round to the nearest pixel

The shaper's baseline was truncated to a whole pixel, which set a line whose baseline fell at 32.9 a pixel higher than the browser's (and higher than its neighbour at 33.0). It is now rounded. This is the one change that alters existing output: text can move by one pixel. The showcase's visual-regression references still pass within their tolerance.

### Vector icons: a small SVG subset, rasterised by `zeno` into the glyph atlas

`VectorIcon` is `Copy` data naming a `'static` description: `VectorIcon::svg(markup)` for markup pasted from a design, `VectorIcon::stroked(path)` / `filled(path)` for path data in the usual 24-unit box. An icon set is a file of constants (`examples/fundidesk/src/icons.rs`). `Frame::draw_icon(icon, rect, color, stroke_width)` draws one; the UI has `Icon::vector(icon).size(dp).color(..).stroke_width(..)`.

**Where the masks live: in the glyph atlas, as coverage. Not in the image path.** An icon here is one colour, which is exactly what a glyph is: a coverage mask multiplied by a colour. Putting the mask in the atlas means:

- an icon is drawn by the existing `DrawOp::Glyph`, so **nothing changed in the renderer or the shader**, and icons stay in the one draw call with text and rectangles. Through the image path, each distinct icon would be its own texture and split the draw call (0014): a tab bar of four icons would be four draw calls;
- one byte per texel rather than four, no mip chain, and the atlas's dirty-row upload (0011) applies;
- eviction comes for free: when the atlas fills it is rebuilt from what is on screen, icons included.

The cost is that an icon cannot have two colours or partial transparency of its own, and that masks compete with glyphs for atlas rows (a 24 dp icon at 3x is 72 x 72 texels; the atlas is 1024 wide and up to 2048 tall). Multi-colour artwork belongs in a `Picture`.

**Crisp at any scale.** The mask is rasterised for exactly the device pixels the icon covers (its rectangle is snapped to whole pixels first), with the view box scaled uniformly and centred as SVG does. Nothing is scaled at draw time, except an icon over 512 px on a side, which is rasterised at 512 and stretched.

**Cache key: (icon, pixel width, pixel height, stroke width override).** The icon's identity is its description (the `&'static str`, compared by content). Parsing happens once per icon; rasterising once per key for as long as the mask stays in the atlas.

**The subset.** One `<svg>` with a `viewBox`; `<path>`, `<circle>`, `<rect>` (with `rx`/`ry`) and `<line>`; `fill`, `stroke`, `stroke-width`, `stroke-linecap`, `stroke-linejoin`, inherited from the root or set per shape, with SVG's defaults. Every paint that is not `none` is the icon's one ink, whether written `currentColor` or as a colour. These are what the design's 48 icons use.

**Everything else is refused with an error that names it** (`VectorIcon::check() -> Result<(), IconError>`): groups and `transform`, `<ellipse>`, `<polyline>`, `<polygon>`, `<text>`, `<use>`, `<defs>`, gradients and `url(..)` paints, `opacity` in any form, `stroke-dasharray`, `stroke-miterlimit`, `fill-rule`, `style` attributes and CSS, lengths with units or percentages, and two different paints in one icon (`MultipleColors`). An icon that fails to parse draws nothing, so an icon set should be checked in a unit test, as FundiDesk's is.

**Path data is parsed here, not by `zeno`.** `zeno` rasterises and strokes; its SVG path reader mishandles a smooth curve that repeats (`s .. ..` with two argument sets) or follows another, which drew the design's eye icon wrong. The reader in `icon.rs` covers the whole path grammar, including arc flags written without separators, and is tested against that icon.

**Dependency.** `zeno 0.3.3` becomes a direct dependency of `morningstar-text` with the features `swash` already enables for it. It was already linked into every app through the glyph rasteriser, so no crate is added to the tree and no code to the binary beyond the calls made. This extends 0007's inventory.

**The existing `Icon` widget is kept.** `Icon::new("+")` is still the glyph tile, unchanged; `Icon::vector(..)` is new on the same type. On a tile (`.background(..)`) a vector icon takes half the tile's side, which is the design's 40/20 and 52/26 pattern.

### Dashed borders are a mode of the rectangle, in the shader

`DrawOp::DashedRect { rect, corner_radius, fill, border, dash }` with `Dash { length, gap }`, drawn by the same instance as a bordered rectangle: for a rectangle, the instance's spare `extra` field carries the dash length and period, and the fragment shader multiplies the border's coverage by a dash pattern laid along the middle line of the stroke, corners included (arc length on the arcs), with a pixel of falloff at dash ends. A solid border takes the same path with a zero dash length and is unchanged. One instance, the one draw call, no new varying.

The alternative was a row of small rectangles from the UI layer, which cannot follow a rounded corner, or a mask in the atlas, which would be as large as the box.

`Dash::css(rect, radius, width)` gives the pattern Chromium draws for `border-style: dashed` (dashes of three widths and gaps of two, or two and one from 3 px up, with the gap adjusted so a whole number fits the perimeter); the pattern starts at the bottom of the left edge and runs clockwise, where Skia starts a rounded rectangle. The UI has `Container::border_dashed(width, color)` for that and `.border_dash(length, gap)` for an explicit pattern.

## What the fidelity loop measured

Four screens were built to the highest fidelity reached and compared with their references at 780 px wide, 2 px per dp, full artboard height, default `morningstar-diff` settings (threshold 2, anti-aliasing radius 1), with the four 56 px corner squares the design tool rounds off left out.

| Screen | Size (px) | Pixels differing | Mean error | Max error |
| --- | --- | --- | --- | --- |
| 01 splash | 780 x 1688 | 0.2868% | 0.139 | 49.39 |
| 03 login | 780 x 1688 | 0.1671% | 0.047 | 67.63 |
| 07 dashboard | 780 x 2120 | 0.4091% | 0.110 | 72.85 |
| 10 repair details | 780 x 2760 | 0.3425% | 0.109 | 77.03 |

The first renders, before any correction, differed by 0.25%, 1.36%, 8.07% and 5.04%.

**Geometry matches.** Row by row and column by column, the extent of every block of ink on the four screens is within one pixel of the reference, and the one-pixel cases are an anti-aliased row falling either side of the detection threshold. No element is missing, extra, the wrong size or the wrong colour.

**What remains is how edges are blended.** Every listed region is text, an icon or a curved edge. The browser blends anti-aliased coverage on sRGB-encoded values; Morningstar blends in linear light (0009, 0011). A half-covered pixel of green on white is `#96C293` in the reference and `#BDCEBD` here. Dark text on a light ground therefore looks lighter and thinner than in the design, and light text on a dark ground slightly heavier: the worst regions are the semibold green links, white text on the green buttons, and the logo mark. The comparison tool accepts a differently mixed edge pixel only when it lies between its neighbours' colours within the threshold, so thin features, where most pixels are edge pixels, are what gets counted. A different glyph rasteriser (swash against Skia/CoreText) contributes the rest.

Three things the loop taught, each worth more than a per-screen fix:

1. **A browser rounds font metrics to device pixels.** Text with no `line-height` is `round(ascent) + round(descent)` *device* pixels tall, so at 2x it is a multiple of half a dp (13 px Inter is 15.5 dp, not 16). Getting this wrong moved everything below the first label by a pixel per line.
2. **In CSS, padding starts inside the border, and a border adds to a size unless `box-sizing: border-box` says otherwise.** Here a border is drawn over the box's edge and takes no room. A card with `border: 1px; padding: 16px` is padding 17; a link with `height: 48px; border: 1.5px` and no `box-sizing` is 51 tall.
3. **An `<input>` has 2 px of padding of its own.**

The tests hold each screen at its measured value rounded up to the next 0.05 (`examples/fundidesk/tests/fidelity.rs`).

## Not done, and approximations

- **Gamma of anti-aliased edges.** The dominant remaining difference, and visible: body text is lighter than the design's. Closing it means either blending in encoded space throughout (which reverses 0011's choice and would make gradients match CSS too) or correcting coverage per glyph from the text colour. Neither was attempted; it deserves its own record and measurements on a device.
- **No wrapping rows** (`flex-wrap`). Chips are laid out in explicit rows by the screen author.
- **No per-side borders.** `border-top` / `border-bottom` are a 1 dp `Divider` child, which is exact for bars.
- **No per-corner radii, no percentage sizes** (computed by hand from the fixed parent).
- **No text input.** Fields are static look-alikes.
- **A link inside a sentence** is tappable as the whole line.
- **The dash phase was not checked against a reference**: no built screen has a dashed border yet. The pattern is derived from Chromium's source and tested for geometry (exact pixels on straight edges, share of the ring covered, equal steps round a circle), not against Chrome's pixels.
- **Screenshots pass through an 8-bit linear target** before being encoded, which costs up to two levels in the darkest colours (`#14201A` reads back as about `#16221C`). Below the comparison threshold; it should become an sRGB target.
- **Icon rasterisation is on the render thread**, on first use of each (icon, size). Not timed.

## Public API added

`morningstar-core`: `Dash` (`new`, `css`, `fitted`), `DrawOp::DashedRect`, `DisplayList::{stroke_dashed_rect, fill_dashed_rect}`.

`morningstar-text`: `FontFamily`, `TextSpan`, `VectorIcon` (`svg`, `stroked`, `filled`, `check`, `aspect_ratio`), `IconError`; `TextStyle::{family, letter_spacing}` (fields and builders); `TextContext::{measure_spans, draw_spans, draw_icon, has_family}`.

`morningstar-runtime`: `Frame::{draw_text_spans, measure_text_spans, draw_icon, has_font_family}`, `Setup::has_font_family`; re-exports of the types above (also from `morningstar`).

`morningstar-ui`: `Text::{rich, family, letter_spacing, line_height_dp, uppercase}`, `Span` and `span(..)`, `Icon::{vector, stroke_width}` and `icon(..)`, `Container::{border_dashed, border_dash}`, `Fonts`, `Theme::fonts` and `Theme::font`.

`Theme` and `TextStyle` gained public fields. Code that builds either with a complete struct literal must add them; code that starts from `Theme::LIGHT` or `TextStyle::new` (everything in this repository) is unaffected.

## Verified

All on macOS, GPU tests on Metal through the offscreen renderer.

- `morningstar-core` (4 new tests): the dash operation, its fallbacks to a solid border, the fitted pattern dividing the perimeter, the perimeter of a square and a circle.
- `morningstar-gpu` `vocabulary.rs` (4 new): exact pixels of dashes on a straight edge; the fill showing in the gaps; a dashed ring covering its share of the solid ring within 3% for four shapes and never painting outside it; sixteen equal dashes round a circle. The 26 existing tests pass unchanged.
- `morningstar-text` (20 unit tests in `icon.rs`, 13 new in `layout.rs`): the SVG subset, every refusal by name, the path grammar including the smooth-curve case, exact masks for a rectangle, a stroke at three scales, caps, a circle's area, view-box origin and aspect; family selection, weight within a family, fallback for a missing family and a missing glyph, two families in one frame, letter spacing, spans (width, colours, wrapping, cache separation); icons in the atlas (one snapped quad, cache keys, survival across frames, no overlap with glyphs, refill without corruption, the 512 px cap).
- `morningstar-ui` `design.rs` (9 new): theme fonts by role, letter spacing in dp at three scales, capitals, line height in dp, rich text, vector icons at three scales bare and on a tile, culling in a scroll view, dashed borders, and odd input to all of it. The existing tests pass unchanged.
- `examples/fundidesk`: 3 unit tests (icons parse, icons distinct, default line heights) and 6 in `tests/fidelity.rs` (the four screens against their references, no overflow for all twelve at four scales, scrolling between fixed bars, navigation, a real tap, reference sizes against the HTML).
