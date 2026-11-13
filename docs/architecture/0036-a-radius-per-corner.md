# 0036 — A radius per corner, for solid and bordered rectangles

Status: Accepted (2026-11-13). Verified on the development host only (macOS, Metal, offscreen). Nothing here has run on a device.

Extends [0011](0011-renderer-vocabulary.md) and uses the instance slot of [0033](0033-radial-gradients.md). Closes "per-corner radii" in the list of what is not available in `docs/building-screens.md`.

## Context

A chat bubble points at its speaker with one tighter corner (`border-radius: 20px 20px 6px 20px` on a translucent fill, which cannot be faked by overlapping two boxes), and a bottom sheet has round corners at the top only.

## Decision

`DisplayList::fill_corners_rect(rect, CornerRadii, fill, border)` (`DrawOp::CornersRect`) is `fill_bordered_rect` with four radii, clockwise from the top-left as in CSS, carried in `extra2`. The fragment stage takes the radius of the quadrant the pixel is in and evaluates the same rounded-box distance with it; the fill, the inside border and their anti-aliasing are the rectangle's. With four equal radii the picture is identical to the plain rectangle's, byte for byte (tested). Each radius is clamped to half the shorter side; CSS's proportional reduction when two radii meet on a side is not reproduced.

In the UI layer, `Container::radii(top_left, top_right, bottom_right, bottom_left)`.

**Only what needed it.** The four radii apply to a solid background and a solid border. A gradient fill, a dashed border and a clip use the box's single `radius` (the gradient's and the clip's instances have no room for four; the dash pattern is laid along a uniform corner), and a shadow is cast with the largest of the four. A box that needs both a radius per corner and one of those sets `radius` to the one that matters most. No screen here does.

## Verification

`each_corner_has_its_own_radius` and `equal_radii_are_the_plain_rounded_rectangle` in `crates/morningstar-gpu/tests/radial.rs`; the operation the UI layer emits in `crates/morningstar-ui/tests/decor.rs`. Atlas's bubble and sheet are inside their screens' fidelity limits (0.01% and 0.02%).
