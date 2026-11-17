# Widget gallery

Every screen is a tree of these widgets. Each page lists the widget's methods, read from the source's own doc comments when the site is built, and shows a sample that is compiled with the workspace.

## Layout

{{cards}}
- [Container](container.md): The box. Padding, a background, a radius, a border, a shadow, and children.
- [Column, Row and Wrap](column-row.md): Children one under the other, side by side, or wrapping onto new lines.
- [Stack](stack.md): Children on top of each other, pinned to an anchor.
- [ScrollView](scroll-view.md): Content taller or wider than its box.
- [SafeArea and Spacer](safe-area-spacer.md): Clear of the system's bars, and flexible gaps.
{{/cards}}

## Content

{{cards}}
- [Text](text.md): Text in the theme's type scale, with spans for mixed styles.
- [TextInput](text-input.md): An editable text and its keyboard.
- [Button and Pressable](button.md): Five kinds of button, and anything else that can be tapped.
- [Divider](divider.md): A line between rows.
- [Picture and Avatar](picture.md): Images, SVG files, and people.
- [Icon](icon.md): One-colour vector artwork.
- [LiveVideo](live-video.md): A camera or a shared screen from a live room.
{{/cards}}

## Shared by all of them

{{cards}}
- [Common modifiers](modifiers.md): Size, position, identity, motion and accessibility, on every widget.
- [Colours, gradients and shadows](paint.md): What a box can be painted with.
{{/cards}}

## From a design's words to a widget

| The design has | Use |
| --- | --- |
| Vertical or horizontal auto-layout | `col![..]`, `row![..]` |
| Layers, a badge, an overlay | `stack![..]`, children pinned with `.place(..)` |
| A frame with a fill, radius, stroke, shadow | `Container` |
| Text | `text("..")` |
| A bold or coloured phrase inside a sentence | `Text::rich([..])` with `span(..)` |
| A button | `Button`; anything else tappable: `.on_press(..)` on a box |
| A line between rows | `Divider` |
| A flexible or fixed gap | `Spacer` |
| Scrolling content | `ScrollView` |
| Status bar and gesture bar margins | `SafeArea` |
| An image, a person | `Picture`, `Avatar` |
| An icon | `Icon` |
| Chips or tags that wrap | `Wrap` |
| A dashed outline | `.border_dashed(..)` |

Import everything at once:

```rust
use morningstar::ui::prelude::*;
```
