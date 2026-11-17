# Stack

A stack lays its children on top of each other. The first child is at the bottom. It hugs its largest child unless given a size.

{{snippet examples/docs-snippets/src/layout.rs#stack}}

## Placing children

A child is pinned to one of nine anchors with `.place(..)` and moved from there with `.offset(x, y)`, which does not affect layout.

| Anchor | | |
| --- | --- | --- |
| `Alignment::TOP_LEFT` | `Alignment::TOP_CENTER` | `Alignment::TOP_RIGHT` |
| `Alignment::CENTER_LEFT` | `Alignment::CENTER` | `Alignment::CENTER_RIGHT` |
| `Alignment::BOTTOM_LEFT` | `Alignment::BOTTOM_CENTER` | `Alignment::BOTTOM_RIGHT` |

`.content_align(anchor)` on the stack sets the anchor for every child that does not choose its own.

## Creating one

| Write | For |
| --- | --- |
| `stack![below, above]` | A stack with these children |
| `Stack::new()` | An empty one, filled with `.push(..)` |

It is a [`Container`](container.md): it takes a background, a radius, `.clip()` to cut its children to its shape, and touch.

## Uses

- A badge on an avatar or a tile.
- Text or controls over a picture or a video, as on Barua Talk's call screen.
- A sheet or a dialog over a screen: a dimmed box with `.block_touches()` under the sheet, so that nothing behind it reacts.
