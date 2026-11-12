# 0027 — SVG files loaded at run time: a strict reader for a static subset, drawn through the vector-picture path

Status: Accepted (2026-11-12). Verified on the development host only (macOS; the rasteriser is CPU code, the two draw paths are the image path and the glyph atlas of earlier records). Nothing here has run on a device and nothing here has been timed.

Extends [0015](0015-fonts-icons-and-design-fidelity.md) (one-colour icons) and [0017](0017-transforms-and-vector-pictures.md) (multi-colour pictures, which said "there is no SVG parser for pictures"). It changes no decision of theirs: `VectorIcon` and `VectorPicture` stay, as the way to write a drawing in Rust.

## Context

Every illustration and most icons of the two reproduced designs were transcribed by hand from the design's SVG into `Layer`s: repeating a group's attributes on each layer, folding `stroke-opacity` into a colour, combining transforms. It is slow, it is where mistakes come from, and a designer's file cannot simply be dropped in. `VectorIcon::svg` reads markup, but only one colour, four element kinds and no groups, transforms or opacity.

## Decisions

### An owned picture parsed at run time, by a reader written here

`morningstar-text` gains `SvgPicture::parse(&str) -> Result<SvgPicture, SvgError>`. The result is a shared handle (`Arc`), cheap to clone, to a tree of shapes with their transforms resolved, the gradients and the clips. It is rasterised by the compositor `VectorPicture` already uses (`picture.rs`'s canvas, which gained gradient inks, clips of several shapes and group compositing), registered as an image and cached by `Frame::draw_picture`, which now takes either kind of picture through one trait, `RasterPicture` (`content_hash`, `rasterise`).

No XML or SVG dependency was added ([0007](0007-dependency-policy.md)). The XML reader is about 250 lines and reads what SVG files contain: elements, attributes in either quote, comments, the prolog, a DOCTYPE without declarations, the five predefined entities and character references. It does not read CDATA sections or a DOCTYPE with an internal subset (entity declarations; refused, which also removes entity expansion as an attack) and it has no namespace processing: a prefixed attribute (`xlink:href`, `inkscape:*`, `sodipodi:*`, `xmlns:*`, `xml:space`) describes nothing drawn and is passed over, except `xlink:href` on a gradient, which is read as `href`; a prefixed *element* is another vocabulary's and is skipped whole, except the prefix `svg:`, which is refused rather than skipped, since skipping it would lose drawing.

### The subset, and that everything else is an error

Read: `svg` (`viewBox`, `width`, `height`), `g`, `defs`, `path`, `rect` (`rx`, `ry`), `circle`, `ellipse`, `line`, `polyline`, `polygon`, `clipPath` (shapes, user space, each with its transform and `clip-rule`), `linearGradient` and `radialGradient` with `stop`s (`gradientUnits` both ways, `gradientTransform`, `spreadMethod`, a focal point, `href` to another gradient for stops and attributes, percentages); `fill`, `stroke`, `fill-opacity`, `stroke-opacity`, `opacity`, `fill-rule`, `clip-rule`, `stroke-width`, `stroke-linecap`, `stroke-linejoin`, `stroke-miterlimit`, `stroke-dasharray`, `stroke-dashoffset` (`zeno` strokes dashes), `color`, `display`, `visibility`, `clip-path`, `transform` lists (`matrix`, `translate`, `scale`, `rotate` with or without a centre, `skewX`, `skewY`), as attributes or in a `style` attribute (which wins, as CSS does), inherited as SVG says; colours as hex with 3, 4, 6 or 8 digits, `rgb()`/`rgba()` with numbers or percentages, all the CSS named colours, `transparent`, `none`, `currentColor`; numbers optionally in `px`. `title`, `desc` and `metadata` are passed over.

Refused, with an `SvgError` carrying the kind (`UnsupportedElement`, `UnsupportedAttribute`, `UnsupportedValue`, `MissingAttribute`, `UnknownReference`, `InvalidPath`, `Syntax`, `NoSize`, `Limit`), the byte offset, the line and the column: `text`, `use`, `image`, `mask`, `filter`, `pattern`, `style` elements (and so classes do nothing, which is why the `class` attribute may be passed over), `symbol`, `marker`, `a`, `switch`, `foreignObject`, a nested `svg`, animation elements, unknown attributes, CSS `transform` in `style`, units other than `px`, percentages outside gradients, `preserveAspectRatio` other than the default, `clipPathUnits="objectBoundingBox"`, a `clip-path` on a `clipPath`, `paint-order` that puts the stroke first, `vector-effect`, blend modes, `fr` on a radial gradient, `hsl()` and system colours.

**Why strict.** A picture that silently lacks its text or its mask is worse than a build that says so: it is found by a designer on a phone, weeks later. Properties that cannot change a picture *within this subset* (font and text properties, `pointer-events`, `shape-rendering`, `isolation`, `mask: none` and the like, which drawing programs write on every element) are passed over, because refusing them would refuse every exported file for nothing. One of them is a known difference: `shape-rendering="crispEdges"` is drawn anti-aliased.

**A lenient mode exists, for one purpose.** `SvgPicture::parse_lenient` leaves out what the strict mode refuses and returns the list of what it left out, each with its position. It is what `morningstar-svg --lenient` uses to show a file that has not been cleaned up, and what was dropped from it. It is not silent, it is a different cache entry from the strict reading of the same file, and structure, a missing size and the limits below stay fatal. An application should bundle files that load strictly.

### Where the semantics are SVG's and where they are not

- **Group opacity and group clips are done as groups.** A `g` with `opacity` or `clip-path` and more than one painted child is composited into a canvas of its own and then faded and cut as one, so overlapping children do not show through each other, as in a browser. With one child the opacity and the clip fold into the child (the same result, without the canvas); with neither, the group disappears into its children. Each nested group that needs a canvas costs 16 bytes per pixel while the picture is rasterised, so they nest at most four deep (a `Limit` beyond).
- **A clip is in the user space of the element that refers to it**, including that element's own `transform`, as SVG says. `VectorPicture`'s clip ignores the layer's transform, which is the special case of a clip on an enclosing group.
- **Gradients interpolate as SVG does**: each channel and the alpha separately (not premultiplied, unlike CSS gradients), on sRGB-encoded values, the space everything is composited in ([0023](0023-compositing-space.md)). Evaluated per pixel centre on the CPU; no dithering.
- **The fit is `xMidYMid meet`, and the viewport is the image.** What lies outside the view box but inside the pixels asked for shows, as in a browser. (`VectorPicture` cuts at the view box; the two agree whenever the box and the image have the same shape, which is how the UI sizes both.)
- **Size.** `SvgPicture::width`/`height` are the root's `width` and `height` where they are given in pixels, otherwise the view box's: `<svg width="250" viewBox="0 0 280 280">` is 250 dp wide in the UI unless sized otherwise, as it is on a web page.
- **Dashes** start where SVG's do; `zeno` accumulates their lengths in `f32`, so a dash pattern can drift by a fraction of a pixel after a few thousand dashes.

### `currentColor`, and icons that take a colour

`currentColor` is the colour given with `SvgPicture::with_current_color` (a cheap clone of the handle; black by default, as on a page that sets no colour), unless a `color` attribute in the file sets it. The current colour is part of the cache key only for files that use it.

A file is **single-ink** when everything in it is painted in one colour at whatever opacities: only `currentColor`, or only one literal colour (the usual filled icon has no paint attribute at all, so it is all black). `Frame::draw_svg_icon` and `Icon::svg` draw such a file as a coverage mask in the glyph atlas, exactly as a `VectorIcon` is drawn: one mask per (file, pixel size), any colour without rasterising again, in text's draw call. Any other file given to `Icon::svg` is drawn as a picture with `currentColor` set to the icon's colour and its own colours kept. This rule is the one `VectorIcon` already had ("every paint that is not `none` is the icon's one ink"), so the two icon routes agree.

### Parse once: where the value lives

Parsing is proportional to the file's length and allocates the tree; it must not happen per frame. Two places to keep the result: the application's own state, for a file that arrives at run time; and `SvgAsset`, a `static`-friendly holder (`const fn new(&'static str)`, a `OnceLock` inside) for a file bundled with `include_str!`:

```rust
static DISC: SvgAsset = SvgAsset::new(include_str!("../assets/svg/disc.svg"));
// in a view:
Picture::svg(DISC.get()).size(250.0, 250.0)
```

`get()` parses on first use, from any thread, and returns the same picture afterwards. A file that does not load gives an empty picture, as an icon outside its subset draws nothing, and `DISC.check()` returns the error: a unit test over the application's assets is the place for it (`examples/callready/src/screens/home.rs`).

### Untrusted input

A file may come from a server. The reader and the rasteriser are bounded:

| Bound | Value |
| --- | --- |
| Source length | 4 MiB |
| Element nesting | 64 (the build recursion is therefore at most 64 deep) |
| Elements | 20,000 |
| Attributes on one element | 128 |
| Path commands in the whole file, after arcs become curves | 200,000 |
| Magnitude of any coordinate or length | 1,000,000; of a composed matrix entry, 10^9 |
| Gradient stops; dash array entries | 256; 64 |
| Dashes along one path (control-polygon length over the pattern's period) | 100,000 |
| Nested groups needing their own canvas | 4 |
| Gradients referring to one another | 8, and no loops |
| Rasterisation size | 2048 px a side through `Frame::draw_picture` ([0017](0017-transforms-and-vector-pictures.md)) |

Numbers are parsed by a scanner that accepts only the SVG number grammar (no `inf`, `NaN` or hex floats) and rejects what is not finite. Error messages shorten values to 60 characters.

**The rasteriser needed a guard.** `zeno` rasterises in 24.8 fixed point and multiplies line spans by 256 again in `i32`: a line spanning more than about 16,000 pixels overflows, which panics in a debug build and draws garbage in a release one. A 24-unit icon with a background rectangle of ±10,000 units, drawn at 512 px, is such a case, so this is not only an attack. Before a mask is rendered, the path's control points (and the reach of its stroke) are checked against a box 1024 px around the image. Inside it, which is every case that existed before, the path goes to `zeno` as it did and the bytes are unchanged. Outside it, the path is outlined in `f32` (`zeno::apply`, which also strokes and dashes), flattened adaptively to 0.05 px with whatever lies wholly to one side of the box left as straight lines, cut to the box side by side (Sutherland–Hodgman, which preserves winding inside the box), and the polygons are filled. This also protects `VectorPicture` and outlined text.

The time to rasterise remains proportional to shapes times pixels: 20,000 shapes at 2048 px is minutes of CPU. An application that draws files from an untrusted source should bound their size itself or rasterise off the frame-building thread; there is no time limit here.

## Costs

- **Memory.** A parsed picture holds its path commands (20 bytes each) and a few words per shape; the 28 inline SVGs of the CallReady design are between 1 and 40 shapes each. While rasterising: 16 bytes per pixel for the canvas (unchanged from 0017), one byte per pixel for each of up to five coverage buffers (three before; the two new ones are allocated only for a clip made of several shapes or for two clips on one shape), and 16 bytes per pixel more per nested composited group. Stored: as any picture, `4 * w * h` bytes plus a third more on the GPU ([0017](0017-transforms-and-vector-pictures.md)); a single-ink icon instead `w * h` bytes of the atlas, at most 512 px a side.
- **Instances, textures, draw calls**: unchanged. A loaded picture is an image like a `VectorPicture`; a single-ink icon is a glyph quad.
- **Binary size**: about 2,000 lines of Rust and a 148-entry colour table; not measured in bytes.
- **Time**: not measured. Parsing happens once per file; rasterisation once per (file, current colour, pixel size).

## Proof

- **The design's own SVGs against the hand transcriptions** (`examples/callready/tests/svg.rs`). The inline SVGs of `02-home.html` (the disc and both ring sets), `01-welcome.html` (waveform, underline) and `03-live-call.html` (avatar disc) are extracted from the HTML (the export's `sc-camel-view-box` put back to `viewBox` and the root's page-layout `style` removed, in the test), loaded, and compared byte for byte with the `VectorPicture`s at 2 and 2.8125 px per dp. Five of the six are **identical in every byte at both scales**. The disc differs in 8 of 1,000,000 bytes at 2x and 31 of 1,976,836 at 2.8125x, each by one level: its five black rings are one clipped group in the file, composited as a group here and ring by ring in the transcription, and the two round differently. All 28 inline SVGs of the five designed pages load strictly.
- **CallReady's home screen now loads its three pictures from `examples/callready/assets/svg/*.svg`.** Its fidelity figure is unchanged to the fourth decimal (0.0209%, mean error 0.044, largest 45.72), as are the other four screens'.
- **Sample files** in `tools/morningstar-devtools/tests/svg`, written for this in the styles of common sets: two 24 px stroke icons, two 24 px filled icons (one with an even-odd hole and a second tone), an illustration with linear and radial gradients, an inherited gradient, a clip, a faded group, dashes and transforms, and a drawing program's export with everything in `style` attributes and editor metadata throughout. Rendered with `morningstar-svg` and looked at; held by `tests/svg_cli.rs`.
- `crates/morningstar-text/tests/svg.rs` (43 tests): every element and property of the subset as pixels; refusals by name and position; the lenient mode; every limit; extreme geometry rasterised without panicking and, where it is visible, correctly; a sample cut at every byte and damaged 6,000 ways without a panic.

## Not done

- `use`, `symbol`, `text`, `mask`, `filter`, `pattern`, `image`, CSS in `style` elements, `paint-order`, `vector-effect`, `marker`s, `clipPathUnits="objectBoundingBox"`, a `clip-path` inside a `clipPath`. `use` is the one a designer's file is most likely to need.
- No per-shape bounding boxes in the compositor: each shape still costs a pass over the whole image. Fine for illustrations of tens of shapes; a file of thousands is slow.
- No comparison against a browser's rendering of the sample files (the designed screens are compared with Chrome through the fidelity tests; the samples were looked at).
- A picture cannot be recoloured except through `currentColor` and `tint`.
- Nothing on a device.

## Public API added

`morningstar-text`: `SvgPicture` (`parse`, `parse_lenient`, `width`, `height`, `view_box`, `with_current_color`, `uses_current_color`, `is_single_ink`, `content_hash`, `rasterise`, `rasterise_mask`), `SvgAsset` (`new`, `get`, `check`), `SvgError`, `SvgErrorKind`, `RasterPicture`, `MAX_SVG_BYTES`; `TextContext::draw_svg_icon`.

`morningstar-runtime`: `Frame::draw_picture` takes `&impl RasterPicture` (source-compatible: a `&VectorPicture` still works); `Frame::draw_svg_icon`; re-exports. `morningstar`: re-exports.

`morningstar-ui`: `Picture::svg(&SvgPicture)`, `Icon::svg(&SvgPicture)`; `SvgPicture` and `SvgAsset` in the prelude. The internal `PictureSource` and `ImageNode` are no longer `Copy`.

`morningstar-devtools`: `render_svg`, `SvgBackground`, `color_from_srgb`, and the `morningstar-svg` binary.

No `wgpu` type appears in any of it, no dependency was added, and there is no `unsafe`.
