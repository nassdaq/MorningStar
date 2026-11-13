# 0035 — Inner shadows, and several shadows on one box

Status: Accepted (2026-11-13). Verified on the development host only (macOS, Metal, offscreen). Nothing here has run on a device and nothing here has been timed.

Extends [0011](0011-renderer-vocabulary.md) (shadows) and uses the instance slot of [0033](0033-radial-gradients.md).

## Context

A box had one drop shadow. The Atlas orb has four in one `box-shadow`: two blurred inner lights (`inset 0 -18px 35px rgba(255,255,255,0.5)`, `inset 0 9px 21px ..`), a one-pixel inner ring (`inset 0 0 0 1px ..`) and a drop shadow.

## Decisions

**An inner shadow is one more kind of quad.** `DisplayList::draw_inset_shadow(rect, corner_radius, shadow)` (`DrawOp::InsetShadow`) paints the shadow's colour inside the box wherever the box's own blurred silhouette, moved by the offset and shrunk by the spread, does *not* reach: CSS's definition, a sheet of the colour with a hole in it, seen through the box. Coverage is `inside(p) * (1 - hole(p - offset))`, where `hole` is the shadow function of 0011 for a positive blur and the plain rounded-box coverage for none (so `inset 0 0 0 1px` is an exact one-pixel ring). The offset and spread travel in `extra2`. It is drawn after the fill it lights or darkens. `Shadow` gains a public `inset` field and `Shadow::inset(x, y, blur, colour)`; struct literals of `Shadow` need the new field.

**Several shadows, in CSS order.** `Container::shadows([..])` takes a `box-shadow` list as written, the first on top, of either kind. Drop shadows are drawn under the box from the last up; inner ones over the background and its gradients, from the last up; the border after them. When a box has inner shadows, gradients or a blur, its border becomes a stroke of its own drawn last (one more instance) instead of sharing the fill's instance. `Container::shadow(..)` is unchanged.

Unlike CSS, a drop shadow is still drawn under the whole box, not clipped out of it ([0011](0011-renderer-vocabulary.md)): a translucent box shows its own shadow through itself.

## Verification

`crates/morningstar-gpu/tests/radial.rs`: an inner shadow against a brute-force blur of the offset hole (within 4 of 255 inside the box), and a sharp one with a spread as an exact ring. `crates/morningstar-ui/tests/decor.rs`: the order of the operations of a box with three shadows and a border.
