# Navigator, Viewport and ScrollTo

## Navigator

The stack of screens as plain application state. [Navigation and time](../guide/navigation.md) is the guide.

{{api crates/morningstar-ui/src/navigator.rs Navigator}}

## Viewport

What `Application::view_in` is given: the screen the view is drawn on, in dp.

{{api crates/morningstar-ui/src/viewport.rs Viewport}}

Text is measured exactly as layout will measure it, so a view can choose between two layouts instead of guessing from a character count.

## ScrollTo

A request to move a scroll view, queued with `out.scroll(..)` and carried out in the next frame, after its layout.

{{api crates/morningstar-ui/src/scroll.rs ScrollTo}}

## Transition

How a widget enters and leaves, for `.transition(..)`.

{{api crates/morningstar-ui/src/motion.rs Transition}}
