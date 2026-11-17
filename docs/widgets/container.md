# Container

A box: the one widget that has children, padding and decoration. `Column`, `Row`, `Wrap`, `Stack`, `ScrollView`, `SafeArea`, `Pressable` and `Spacer`, and the `col!`, `row!` and `stack!` macros, all produce a `Container` with a different preset, so every method here works on all of them. It corresponds to a frame with auto-layout in a design tool.

Paint order: shadow, background, border, then the children in order. Later children cover earlier ones.

{{snippet examples/docs-snippets/src/widgets.rs#card}}

## Creating and filling

{{api crates/morningstar-ui/src/element.rs Container only=new,empty,push,push_maybe}}

## Padding and spacing

{{api crates/morningstar-ui/src/element.rs Container only=padding,padding_xy,padding_ltrb,gap,wrap,line_gap,safe_area}}

## Alignment

{{api crates/morningstar-ui/src/element.rs Container only=main_align,cross_align,content_align,center,space_between}}

## Paint

{{api crates/morningstar-ui/src/element.rs Container only=background,gradient,radial,radials,blur,radius,radii,border,border_dashed,border_dash,shadow,shadows,clip}}

[Colours, gradients and shadows](paint.md) explains each kind of paint.

## Touch

{{api crates/morningstar-ui/src/element.rs Container only=on_press,on_press_maybe,pressed_background,block_touches,on_increment,on_decrement}}

## Around a text input

{{api crates/morningstar-ui/src/element.rs Container only=input_frame,focused_border,focused_background}}

Every [common modifier](modifiers.md) applies as well.
