# SafeArea and Spacer

## SafeArea

The screen is drawn edge to edge, under the status bar, the gesture bar and any display cutout. `SafeArea` keeps its child clear of them.

{{api crates/morningstar-ui/src/element.rs SafeArea}}

For a bar that should reach the screen's edge with only its contents kept clear, use `.safe_area(edges)` on the bar's own box instead:

{{api crates/morningstar-ui/src/element.rs SafeEdges}}

While the keyboard is up, a box that pads for the bottom safe area pads for the keyboard instead.

## Spacer

A spacer is an empty box along the parent's axis.

{{api crates/morningstar-ui/src/element.rs Spacer}}

`Spacer::new()` between two children pushes them to opposite ends; at the end of a column it pushes what follows to the bottom. For a fixed distance between all children, `.gap(..)` on the parent is simpler.

## Pressable

`Pressable::new(child)` is a box around a child, ready for `.on_press(..)`. It is described with [Button](button.md).
