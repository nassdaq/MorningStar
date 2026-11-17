# Navigation and time

## Screens

A `Navigator` is the stack of screens as plain application state. Nothing about it is special to the framework: a screen is a value of your own type, and `view` draws whichever is on top.

{{snippet examples/docs-snippets/src/navigation.rs#screens}}

Three things in that sample are worth copying:

- **`.id(screen)` on each screen's root** gives every screen its own scroll positions and focus.
- **`.transition(self.screens.transition())`** slides screens in and out in the direction of the last change: forward for a push, backward for a pop.
- **A screen can carry data.** `Screen::Order(1042)` is a different identity from `Screen::Order(1043)`.

## The system's Back

`back()` is called for the Back button and the back gesture. Return `true` when the app dealt with it, and `false` to leave it to the system, which then leaves the app. `Navigator::back()` returns exactly that: it pops if there is something to pop.

A screen with something of its own to close first (a sheet, a search field, a selection) checks that before the navigator. [ADR 0018](../architecture/0018-back-and-timers.md).

## Time

`tick()` asks for a message after a delay. It is asked again after each one, so it repeats while it returns `Some` and stops when it returns `None`. With no timer, an idle app draws nothing and wakes for nothing.

Use it for a clock that is on screen, for polling a server while a screen is open, and for timeouts. Do not use it to animate: [`.animate` and `.transition`](theming-and-motion.md#motion) do that without messages.

## Reference

Every method of `Navigator` is in the [reference](../reference/navigator.md).
