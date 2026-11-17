# Text, fonts and pictures

## Text

A text takes a step of the theme's type scale, and any property can be overridden.

{{snippet examples/docs-snippets/src/text.rs#roles}}

| Role | Default size | Weight | Used for |
| --- | --- | --- | --- |
| `display` | 34 dp | Bold | The one big number or word on a screen |
| `title` | 24 dp | Bold | A screen title |
| `headline` | 17 dp | Semibold | A section or card heading |
| `body` | 15 dp | Regular | Running text. The default |
| `label` | 14 dp | Semibold | Buttons, tabs, field labels |
| `caption` | 12 dp | Regular | Metadata, timestamps, hints |

{{snippet examples/docs-snippets/src/text.rs#styled}}

Text wraps at the width of its box, so a text that should wrap needs a width: usually `.fill_width()`. Scripts that run right to left, such as Arabic, are shaped and ordered correctly within the text's own box.

Every method is listed under [Text](../widgets/text.md).

## Fonts

An app bundles its fonts and adds them at setup. Nothing is drawn from a system font unless the app asks for the system's fonts.

```rust
fn setup(&mut self, setup: &mut Setup<'_>) {
    setup.add_font(include_bytes!("../assets/fonts/Inter-Regular.ttf"));
    setup.add_font(include_bytes!("../assets/fonts/SpaceGrotesk-Bold.ttf"));
}
```

A family is chosen per text with `.family("Space Grotesk")`, or for the whole app in the theme: `Fonts::new(body, heading)` sets the family for body roles and for heading roles. If the family has no italic file, italic is synthesised by slanting.

## Icons

An icon is one-colour vector artwork that takes its colour where it is drawn.

{{snippet examples/docs-snippets/src/widgets.rs#icons}}

For icons kept as files, load them as SVG (below) and draw them with `Icon::svg(asset.get())`. A file that uses a single ink is tinted like any other icon.

## SVG files

SVG is loaded at run time by a strict reader written for the purpose. It supports a static subset: shapes, groups, transforms, clips, gradients, dashes and `currentColor`. Anything else is an error that names the line, so a file either draws as designed or is refused.

{{snippet examples/docs-snippets/src/pictures.rs#svg}}

Check a file before using it: `morningstar-svg art.svg out.png --scale 3` renders it exactly as an app would. See [Tools](../reference/tools.md#morningstar-svg) and [ADR 0027](../architecture/0027-svg-loading.md).

## Images

Images come from PNG data or raw pixels, added once at setup. A `Picture` draws one with a fit, a corner radius and an optional tint; an `Avatar` crops one to a circle and falls back to initials.

{{snippet examples/docs-snippets/src/pictures.rs#image}}

Only PNG is decoded. The Barua Talk example keeps its photographs as JPEG for the design and converts them to PNG with a small script, `examples/barua-talk/design/prepare-images.py`.

Each image is its own texture and splits the frame's draw call ([ADR 0014](../architecture/0014-images.md)), which is worth knowing for a long list of photographs.

## Turned text, outlines and shadows

A widget can be drawn through a transform (`.rotate(degrees)`, `.transform(..)`), and text under a transform is rasterised for it, so turned text stays sharp. `Text::shadow` draws a hard shadow and `Text::outline` a stroke around the letters. [Building screens](../building-screens.md#turned-things-illustrations-and-text-effects) has worked examples of each.
