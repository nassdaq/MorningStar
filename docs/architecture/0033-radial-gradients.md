# 0033 — Radial gradients: one instance per pair of stops in the quad pipeline; the instance grows to 160 bytes

Status: Accepted (2026-11-13). Verified on the development host only (macOS, Metal, offscreen). Nothing here has run on a device and nothing here has been timed.

Extends [0011](0011-renderer-vocabulary.md) (the drawing vocabulary and its one draw call) and [0017](0017-transforms-and-vector-pictures.md) (the instance layout). Colours are interpolated as [0023](0023-compositing-space.md) says. It changes no decision of theirs.

## Context

The Atlas design (`examples/atlas`) is built on CSS `radial-gradient`: 51 uses in ten pages. A full-screen wash behind the content, a glass orb made of four gradients stacked in one box (three translucent spots over a four-stop body), a glow and a highlight that are ellipses fading to nothing. The renderer had a two-stop linear gradient as a box fill and radial gradients only inside vector pictures, which are rasterised on the CPU into an image per size: right for an illustration, wrong for a wash the size of the screen (10 MB of texture on a 1080 x 2340 phone) and for an orb that changes size while it animates.

## Decisions

### A radial gradient is a quad fill, evaluated per pixel

`DisplayList::fill_radial_gradient_rect(rect, corner_radius, RadialGradient { center, radius_x, radius_y, stops }, blur)` fills a rounded rectangle. The gradient is CSS's: the colour at a point depends on where it lies between the centre and the *ending ellipse* (`radius_x`, `radius_y`; equal for a circle), the first stop's colour inside its offset, the last stop's beyond its offset, premultiplied sRGB-encoded mixing between neighbours. Centre and radii are in the rectangle's pixels and may lie outside it. The fragment stage computes `length((p - center) / radii)` in the quad's own space, so a gradient turns and scales with a transform like every other fill.

### Any number of stops, as one instance per pair of neighbouring stops

`DrawOp` is `Copy` and an instance is a fixed number of floats, so a list of stops fits in neither. Instead the display list holds one `DrawOp::RadialGradientRect` per *segment*: the two offsets it lies between and their two colours (`RadialSegment`). `fill_radial_gradient_rect` emits them: one for the inside of the first stop if its offset is above zero, one per pair of neighbours, one for what lies beyond the last stop (its `to` is infinite). A segment whose two colours are fully transparent is not emitted, so `rgba(..) 0%, transparent 48%` is a single instance.

A pixel belongs to the segment whose offsets it lies between, **the nearer included and the further not**. All segments of a gradient compute the same position from the same instance values, so exactly one of them paints any pixel: no seam, no double blending of translucent stops, and no anti-aliasing between segments is needed because the colour is continuous across the boundary. The pixel test compares a four-stop off-centre ellipse with the definition at every pixel.

The alternatives were a stop table in a texture or uniform buffer indexed per instance (a second binding and a data upload for something three of four gradients here do not need) and a wider instance holding four stops (48 more bytes on every instance on screen). The cost of this choice is overdraw: an N-stop gradient shades its box up to N times, each time discarding all but its own band. The orb's body is three segments and a solid remainder.

### The instance grows from 144 to 160 bytes

A segment needs its centre (2), its radii (2), its two offsets (2) and, for [0034](0034-blur-of-a-box-background.md), a blur (1): seven floats where a gradient instance had the four of `extra` and one spare in `params`. A tenth `vec4<f32>`, `extra2`, is added to every instance: ten of sixteen vertex attributes, thirteen varying locations (40 components; the limit of `Limits::downlevel_defaults()` is 60 components, and WebGL2-class GLES allows 15 locations). The same four floats carry an inner shadow's offset and spread ([0035](0035-inner-shadows-and-several-shadows.md)) and a rectangle's four radii ([0036](0036-a-radius-per-corner.md)), which is what decided for a general slot rather than a packing trick for gradients alone.

**Cost**: at 1,000 instances the per-frame upload is 160 KB against 144 KB, 11% more, paid by every instance whether it uses the slot or not. Kinds that do not read it pay two more flat varyings and nothing in the fragment stage. **The cost on a device is unmeasured**, like 0017's.

### In the UI layer: `Radial`, with CSS's shape, position and size

`Container::radial(Radial)` paints a gradient over the box's background colour and over the radial gradients added before it; `Container::radials([..])` takes a CSS `background` list as written, the first on top. A `Radial` is `circle()` or `ellipse()`, `.at(x, y)` in fractions of the box (`at 30% 22%` is `.at(0.3, 0.22)`), `.extent(RadialExtent::{ClosestSide, FarthestSide, ClosestCorner, FarthestCorner, Radii(x, y)})` with CSS's default (farthest corner), and `.stop(offset, colour)`. The radii are computed from the box when it is painted, exactly as CSS defines them (an ellipse through a corner keeps the proportions of the one that touches the sides).

```rust
// background: radial-gradient(circle at 50% 34%, rgba(246,170,214,0.32), rgba(246,170,214,0) 48%), #FBF8FC
page.background(PAGE).radial(Radial::circle().at(0.5, 0.34).stop(0.0, with_alpha(WASH, 0.32)).stop(0.48, with_alpha(WASH, 0.0)))
```

A box whose background has more than one layer is drawn through an offscreen layer when it is faded with `.opacity(..)` ([0028](0028-group-opacity-layers.md)), like any box that overlaps itself.

## Not done

- **Multi-stop and repeating linear gradients, conic gradients.** The same segment scheme would carry a multi-stop linear gradient; no design here has one.
- **A radius per corner on a gradient-filled box** ([0036](0036-a-radius-per-corner.md)): the slot is taken.
- **Colour interpolation in another space** (`in oklch`): everything is sRGB-encoded, as [0023](0023-compositing-space.md) decided.
- **Dithering.** A slow fade over a large area is banded exactly as a browser's is.

## Verification

`crates/morningstar-gpu/tests/radial.rs` (11 tests on the real GPU): two stops against the definition at every pixel; four stops on an off-centre ellipse with no seam; a fade to transparent keeping its hue; two gradients stacked in a circle; a gradient under a quarter turn. `crates/morningstar-ui/tests/decor.rs`: the display list of a box with a CSS list of layers, and the radii of every size keyword. On Atlas, the orb, drawn only with these operations and those of 0034 and 0035, is within 0.05% of Chrome's picture on the screen it fills most of (`examples/atlas/SCREENS.md`). The other examples' fidelity figures are unchanged to four decimals.
