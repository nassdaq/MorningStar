# Button and Pressable

## Button

A button with a label, in one of five kinds.

{{snippet examples/docs-snippets/src/widgets.rs#buttons}}

{{api crates/morningstar-ui/src/element.rs Button}}

A button without a message is drawn disabled and cannot be tapped. `.on_press_maybe(..)` makes that depend on the state.

A button hugs its label. `.fill_width()` makes it as wide as its parent, which is what most forms want.

## Pressable

Anything can be tapped: `.on_press(message)` works on every box. `Pressable::new(child)` is a box around a child, ready for it.

{{snippet examples/docs-snippets/src/widgets.rs#pressable}}

{{api crates/morningstar-ui/src/element.rs Pressable}}

{{api crates/morningstar-ui/src/element.rs Container only=on_press,on_press_maybe,pressed_background,block_touches}}

A pressable is one stop for a screen reader and reads the texts inside it in order. One with no text inside, such as an icon button, needs a `.label(..)`. The accessibility check also reports any control smaller than 48 by 48 dp.
