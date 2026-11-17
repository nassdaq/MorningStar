# ScrollView

A viewport around content that is taller, or wider, than the space it has.

{{snippet examples/docs-snippets/src/layout.rs#scroll}}

{{api crates/morningstar-ui/src/element.rs ScrollView}}

## Rules

- **It fills its parent**, so it needs a height from above: put it in a column that fills, or directly in a `SafeArea`.
- **Its position belongs to its identity.** Give a scroll view whose content changes an `.id(..)` and keep it; while a scroll view with the same id is in consecutive frames, its position is kept. One that is absent from a frame starts again at the top.
- **Content that grows leaves the position where it is.** Content that shrinks under it brings it back to the new end.
- **Inside it, moving a finger more than 8 dp turns a press into a scroll.**
- **The focused text input is kept in view** while the keyboard is up.

## Moving it from code

Ask from `effects`, with a [`ScrollTo`](../reference/navigator.md#scrollto):

{{snippet examples/docs-snippets/src/motion.rs#scroll}}

## Sideways

`ScrollView::horizontal(row)` scrolls a row of chips or cards that is wider than the screen. A drag is given to whichever scroll view it moves along, so a sideways row works inside a vertical screen. [ADR 0037](../architecture/0037-scroll-views-that-scroll-sideways.md).
