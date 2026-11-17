# Layout

Layout uses the words a design tool uses. A box is a column, a row or a stack. Its size on each axis is fixed, hugs its contents, or fills its parent. There is padding inside a box and a gap between its children.

## Fixed, hug and fill

{{snippet examples/docs-snippets/src/layout.rs#sizes}}

| Size | Method | Meaning |
| --- | --- | --- |
| Fixed | `.width(dp)`, `.height(dp)`, `.size(w, h)` | Exactly this many dp |
| Hug | `.hug_width()`, `.hug_height()` | As big as the contents. The default |
| Fill | `.fill_width()`, `.fill_height()`, `.fill()` | The space the parent has left |
| Fill by weight | `.fill_width_weight(n)`, `.fill_height_weight(n)` | A share of that space, against the other fills |

`.min_width`, `.max_width`, `.min_height` and `.max_height` bound any of them, and `.aspect_ratio(w / h)` gives a height from a width.

## Rows and columns

{{snippet examples/docs-snippets/src/layout.rs#rows}}

`col![..]` and `row![..]` are short for `Column::new().push(..)` and `Row::new().push(..)`. Both are a [`Container`](../widgets/container.md), so both take padding, a gap, alignment and paint.

- `.gap(dp)` is the space between neighbours.
- `.main_align(..)` distributes children along the box's own axis; `.space_between()` is the common case.
- `.cross_align(..)` places each child across the axis; a child overrides it with `.align_self(..)`.
- `.center()` centres on both axes.

## Stacks

A stack lays its children on top of each other, the later over the earlier. A child is pinned with `.place(..)` to one of nine anchors and nudged with `.offset(x, y)`.

{{snippet examples/docs-snippets/src/layout.rs#stack}}

## Rows that wrap

A row wraps only if told to. Then a child that does not fit starts the next line, and the row is as tall as its lines.

{{snippet examples/docs-snippets/src/layout.rs#wrap}}

## Scrolling

{{snippet examples/docs-snippets/src/layout.rs#scroll}}

A `ScrollView` fills its parent, so it needs a height from above. Its position belongs to the widget's identity: while a scroll view with the same id is in consecutive frames, its position is kept. `ScrollView::horizontal(..)` scrolls sideways. To move one from code, see [`ScrollTo`](../reference/navigator.md#scrollto).

## The safe area and the keyboard

`SafeArea::new(child)` insets its child by the status bar, the gesture bar and any display cutout. `Container::safe_area(edges)` does the same for chosen edges, for a bar that should reach the screen's edge with its contents kept clear.

While the keyboard is up, boxes that pad for the bottom safe area pad for the keyboard instead, and the focused field is scrolled into view.

## A layout that depends on the screen

Implement `view_in` instead of `view` to be told what you are drawn on. The `Viewport` gives the size, the insets and the keyboard's height in dp, and measures text exactly as layout will.

{{snippet examples/docs-snippets/src/layout.rs#viewport}}

## Rules worth knowing

- **Sizes hug by default.** A box is as big as its children; a text as big as its words.
- **Fill inside hug collapses.** A `fill_width()` child of a box that hugs its width gets no width. Give the parent a width.
- **In a row, the text that should wrap must `fill_width()`.** Two long hugging texts in one row overflow.
- **Children start at the top-left** unless the parent says otherwise.
- **There is no right-to-left mode.** `Start` is left.

`Ui::overflows()` lists boxes whose children do not fit; the example apps' tests assert that it is empty.

The engine's design is [ADR 0010](../architecture/0010-layout.md); wrapping, the viewport and scrolling from code are [ADR 0021](../architecture/0021-viewport-wrap-scroll-animation-navigation.md).
