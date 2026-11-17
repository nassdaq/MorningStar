# Text

Text in a step of the theme's type scale. `text("..")` is short for `Text::new("..")`.

{{snippet examples/docs-snippets/src/text.rs#roles}}

## Creating

{{api crates/morningstar-ui/src/element.rs Text only=new,rich}}

A `&'static str` or a `String` also becomes a text wherever a widget is expected.

## The type scale

{{api crates/morningstar-ui/src/element.rs Text only=role,display,title,headline,body,label,caption}}

## Size, weight, colour and family

{{api crates/morningstar-ui/src/element.rs Text only=size,weight,regular,medium,semibold,bold,color,muted,family,italic}}

## Lines

{{api crates/morningstar-ui/src/element.rs Text only=align,centered,line_height,line_height_dp,letter_spacing,max_lines,uppercase,tabular_nums}}

Alignment shows only when the text is wider than its words, so give an aligned text a width.

## Effects

{{api crates/morningstar-ui/src/element.rs Text only=shadow,outline}}

## Several styles in one text

{{snippet examples/docs-snippets/src/text.rs#styled}}

{{api crates/morningstar-ui/src/element.rs Span}}

Spans share the text's line breaking, so a bold phrase in the middle of a sentence wraps with it.

See also [Text, fonts and pictures](../guide/text-and-pictures.md) and the [common modifiers](modifiers.md).
