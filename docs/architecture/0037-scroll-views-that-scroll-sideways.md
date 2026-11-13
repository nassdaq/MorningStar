# 0037 — Scroll views that scroll sideways

Status: Accepted (2026-11-13). Verified in tests on the host only. How it feels under a finger (the slop, the fling, the choice between two directions) has not been tried on a device.

Extends [0013](0013-ui-layer.md) and [0021](0021-viewport-wrap-scroll-animation-navigation.md) (scroll positions kept by identity, moves asked for through effects) and [0010](0010-layout.md). Closes "horizontal scrolling" in the list of what is not available in `docs/building-screens.md`.

## Context

A row of filter chips wider than the screen is the commonest horizontal scroller on a phone; Atlas's Explore has one (in the design's picture the fifth chip is cut off by the screen's edge). A scroll view scrolled along y only: its content overflowed downwards and was translated.

## Decisions

**Layout: a row may offer its children unlimited width.** A hugging child of a row is normally offered what the row has left, so that text wraps instead of overflowing ([0010](0010-layout.md)). `Style::unbounded` (rows only) offers infinity instead: each hugging child takes its natural width, and what does not fit runs past the row's end. Nothing else in the four sweeps changes. That overflow is the scroll view's content and is not reported by `Ui::overflows`.

**`ScrollView::horizontal(content)`** is such a row with the scroll flag: it fills its parent's width, is as tall as its content, clips, and translates its children along x by the same per-identity state a vertical scroll view has (offset, fling, glide). `ScrollTo::offset` and `end`, `Ui::scroll_offset`, `max_scroll_offset`, `rect_of` and `visible_rect_of` work on it unchanged. What can be scrolled is the content's right edge plus the viewport's right padding less the viewport's width.

**A drag goes to the scroll view that runs its way.** When a finger passes the slop, its dominant direction is taken, and the scroll view under it, or failing that the nearest one around it, that scrolls in that direction and has room to is the one dragged for the rest of the gesture. A sideways row inside a vertical page therefore scrolls sideways under a sideways drag and lets the page scroll under a vertical one; a vertical drag on a sideways row that is not inside a page does nothing. (Before, a drag was vertical or it was a press.)

**Vertical only:** `ScrollTo::reveal` and the keeping in view of the focused field skip sideways scroll views (a text field inside one is brought up or down, never along it); the keyboard does not shorten them; a transformed sideways scroll view scrolls along the screen's x axis, the limitation 0017 already states for y.

## Verification

`crates/morningstar-ui/tests/decor.rs`: natural-width layout with no overflow reported, a drag, the end stop, a tap after a drag, a request from the application, and the choice between a sideways row and the page around it. `examples/atlas/tests/e2e.rs` reaches and taps the chip that starts off screen, by dragging, on a 384 dp and a 360 dp wide phone. The other examples' suites are unchanged and pass.
