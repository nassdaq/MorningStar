# Common modifiers

Every widget has these. They are chained after the widget's own methods, in any order.

## Size and position

{{api crates/morningstar-ui/src/element.rs macro:layout_methods}}

`.size(width, height)` is short for a fixed width and height.

## Identity

`.id(value)` gives a widget an explicit identity, which is what scroll positions, focus, presses and animations belong to. [The application model](../guide/application-model.md#identity) says when one is needed.

## Motion

{{api crates/morningstar-ui/src/element.rs macro:motion_methods}}

See [Themes and motion](../guide/theming-and-motion.md#motion).

## Accessibility

{{api crates/morningstar-ui/src/element.rs macro:semantics_methods}}

On widgets where `label` and `role` are not already taken by the text style, the two short names exist as well:

{{api crates/morningstar-ui/src/element.rs macro:semantics_short_methods}}

See [Accessibility](../guide/accessibility.md).

## Converting messages

`Element::map` converts the messages a subtree produces, so a view written for one message type can be used inside an app with another.

{{api crates/morningstar-ui/src/element.rs Element}}
