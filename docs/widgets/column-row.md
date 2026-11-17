# Column, Row and Wrap

A column arranges its children top to bottom, a row side by side. Most screens are columns and rows nested inside each other.

{{snippet examples/docs-snippets/src/layout.rs#rows}}

## Creating one

| Write | For |
| --- | --- |
| `col![a, b, c]` | A column with these children |
| `row![a, b, c]` | A row with these children |
| `Column::new()`, `Row::new()` | An empty one, filled with `.push(..)` in a loop |
| `Wrap::new()` | A row whose children continue on the next line |

Each returns a [`Container`](container.md), so padding, gap, alignment, paint and touch are the container's methods.

## Spacing and alignment

{{api crates/morningstar-ui/src/element.rs Container only=gap,main_align,cross_align,center,space_between,wrap,line_gap}}

A child overrides the parent's cross-axis placement with `.align_self(..)`.

## Sharing the space

Children that fill share what is left along the axis, by weight:

{{snippet examples/docs-snippets/src/layout.rs#sizes}}

## Wrapping onto new lines

{{snippet examples/docs-snippets/src/layout.rs#wrap}}

`.gap(..)` is the space between neighbours on a line and `.line_gap(..)` the space between lines.

> **Long lists.** A column builds every child each frame. That is what the example apps do for lists of tens of rows. There is no widget that builds rows only as they scroll into view.

Related: [Stack](stack.md), [SafeArea and Spacer](safe-area-spacer.md), and the [layout rules](../guide/layout.md#rules-worth-knowing).
