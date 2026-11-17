# Themes and motion

## The theme

A theme is plain data: colour roles, a type scale, spacing, radii and font families. Start from `Theme::LIGHT` or `Theme::DARK` and change what the design changes.

{{snippet examples/docs-snippets/src/theme.rs#theme}}

Widgets take a colour either as a value (`hex(0x2F6BFF)`) or as a role (`ThemeColor::Primary`). Roles follow whatever theme `theme()` returns, so one view can serve a light and a dark theme.

{{variants crates/morningstar-ui/src/color.rs ThemeColor}}

The fields of each part are in the [Theme reference](../reference/theme.md).

## The system's bars

The screen is drawn edge to edge, under the status bar and the navigation bar. Their icons are dark over a light theme and light over a dark one without the app saying anything. `system_bars()` overrides that for a screen whose edges are not the theme's background, as in the sample above. [ADR 0026](../architecture/0026-system-bars-and-reduce-motion.md).

This has been tested on the development machine only; the bars' colours have not yet been checked on a phone.

## Motion

`view` always describes the screen as it should be. A widget shows a change gradually only if it asks to.

{{snippet examples/docs-snippets/src/motion.rs#motion}}

| You want | Use |
| --- | --- |
| A change of opacity, transform, offset, solid background or fixed size to be seen happening | `.animate(duration, easing)` on the widget |
| A widget to fade, slide or scale in when it appears and out when it goes | `.transition(Transition::fade())`, `::slide(x, y)`, `::scale(from)` |
| Screens to slide in the direction of the navigation | `.transition(navigator.transition())` on each screen's root |
| A scroll that glides | `ScrollTo::..().animated()` |

Easing is `Linear`, `EaseIn`, `EaseOut` (the default) or `EaseInOut`.

Frames are produced only while something is on its way. When the last animation ends, the app is as idle as it was.

## Reduce motion

When the user has asked the system for less motion, every animation, transition and gliding scroll takes no time. The app does nothing for this. In a test, `Ui::set_reduce_motion(true)` does the same.
