# Colours, gradients and shadows

What a [`Container`](container.md) can be painted with.

## Colours

A colour is a value or a role of the theme. Give values as the design writes them.

| Write | For |
| --- | --- |
| `hex(0x2F6BFF)` | An opaque colour from `#RRGGBB` |
| `hexa(0x2F6BFFCC)` | A colour with alpha from `#RRGGBBAA` |
| `rgb8(47, 107, 255)` | An opaque colour from bytes, as CSS `rgb(..)` |
| `with_alpha(colour, 0.5)` | The same colour with another opacity |
| `ThemeColor::Primary` | A role, which follows the theme |

Colours are composited as sRGB-encoded values, as a browser does, so a translucent colour over a background gives the result a design tool shows.

## Gradients

{{snippet examples/docs-snippets/src/widgets.rs#gradients}}

{{api crates/morningstar-ui/src/element.rs Gradient}}

{{api crates/morningstar-ui/src/element.rs Radial}}

Several radial gradients go on one box with `.radials([..])`, in CSS order: the first on top. `.blur(sigma)` blurs a box's background, which is how a soft glow is drawn. [ADR 0033](../architecture/0033-radial-gradients.md), [ADR 0034](../architecture/0034-blur-of-a-box-background.md).

## Borders and corners

| Method | Effect |
| --- | --- |
| `.radius(dp)` | One radius for the background, border, shadow and clip |
| `.radii(tl, tr, br, bl)` | A radius per corner, for solid and bordered boxes |
| `.border(width, colour)` | A stroke along the inside of the edge |
| `.border_dashed(width, colour)`, `.border_dash(length, gap)` | A dashed stroke |
| `.clip()` | Cuts the children to the box, following its radius |

## Shadows

`.shadow(..)` takes one of three ready-made shadows or one built from a design's numbers.

| Shadow | For |
| --- | --- |
| `elevation::LOW` | A hairline lift: list cards on a tinted background |
| `elevation::MEDIUM` | A raised card |
| `elevation::HIGH` | A floating element: hero cards, sheets, floating buttons |
| `Shadow::new(x, y, blur, colour)` | The numbers a design gives |

`.shadows([..])` puts several on one box, in CSS order, and takes inner (`inset`) shadows too. [ADR 0035](../architecture/0035-inner-shadows-and-several-shadows.md).

## Opacity and layers

`.opacity(value)` draws a widget and everything inside it as one group at that opacity, so overlapping children do not show through each other. [ADR 0028](../architecture/0028-group-opacity-layers.md).
