# 0010 — Layout

Status: Accepted (2026-11-11). Implemented in `crates/morningstar-layout` and verified by host tests only. Nothing consumes it yet; no timings have been taken on any machine.

## Context

ADR 0006 left the layout algorithm open ("single-pass box constraints, or flexbox via an existing crate"), and ADR 0008 lists layout as the next thing the design-to-code loop needs, "so code describes structure rather than pixel arithmetic".

The consumer is a declarative widget layer (Column, Row, Stack, Padding, Text, Button) whose test is whether an AI agent can look at a picture of a screen and reproduce it. That shapes the requirements more than generality does:

- The vocabulary should be the one designs are drawn in: Figma auto-layout, SwiftUI stacks, Flutter rows and columns. Fixed / hug / fill, padding, gap, alignment. Not CSS, whose flexbox has many interacting properties (basis, shrink, wrap, auto margins, `min-content`) that an agent has to get exactly right to predict a result.
- Results must be predictable by hand. If a rectangle is wrong, a person or an agent should be able to work out why with arithmetic.
- Text is measured by a callback and measurement is the expensive step, so the number of calls has to be bounded and known.
- Layout will run whenever the UI changes, possibly every frame, on the UI thread.

## Decision

**Morningstar owns its layout engine; it is not flexbox and does not wrap a crate.** `morningstar-layout` depends only on `morningstar-core` and reuses its `Point`, `Size` and `Rect`. The engine is about 1800 lines of safe Rust including documentation (`#![forbid(unsafe_code)]`), roughly 600 of them the algorithm itself. Owning it adds no dependency and keeps the model under our control (AGENTS.md, Architectural Discipline).

**The model.**

- Containers: `row`, `column` (main axis plus cross axis) and `stack` (children overlaid, each pinned to one of nine anchors by `Style::place` or the stack's `content_align`).
- Leaves: `leaf` (sized by style alone: an icon, an image placeholder, a spacer) and `measured` (sized by a callback: text).
- Per axis, `Sizing::Fixed(f32)`, `Sizing::Hug` or `Sizing::Fill(weight)`.
- Per node, one `Style`: padding per edge, gap, `MainAlign` (start, center, end, space-between, space-around, space-evenly), `CrossAlign` (start, center, end, stretch) with a per-child `align_self`, min/max size, a limited aspect ratio, and a post-layout `offset` (for a badge that overhangs the corner it is pinned to).

**Width flows down, height flows up.** This is the one asymmetry in the model and it exists because of text: how tall a paragraph is depends on how wide it is allowed to be, never the reverse. So a hugging node is never made wider than its parent offers (text wraps), while a hugging node is as tall as its contents with no cap (content taller than the screen overflows, which is also what a future scroll container needs to read).

**Four linear sweeps, no recursion.** `compute` builds a pre-order list of the subtree, then:

1. bottom-up, natural (unconstrained) widths;
2. top-down, final widths;
3. bottom-up, natural heights given the final widths;
4. top-down, final heights and positions.

A bottom-up sweep is the pre-order list walked backwards. There is no recursion, so tree depth is not a stack hazard, and each node is visited four times whatever the tree's shape.

The alternative considered was the usual single recursive "constraints down, sizes up" function. It needs a second layout of a child whenever the parent learns its own size from its children and then stretches them, and nested hug-column / row / hug-column trees (a card containing a row containing a column of text) make that exponential in depth unless every node caches results. Separating "what would you like" from "what you get" makes the cost linear by construction, with no cache to get wrong.

**The measure callback is `FnMut(MeasureKey, Option<f32>) -> Size`**, passed to `compute`, where `MeasureKey` is a `u64` the caller attached to the leaf. It matches `TextContext::measure(text, style, max_width)` in `morningstar-text`. Keys rather than a closure per leaf: no allocation per node, no lifetimes in the tree, and the widget layer keeps its text wherever it likes.

It is called **at most twice per measured leaf per `compute`**:

| Leaf | Calls |
| --- | --- |
| Neither axis hugs, or width is fixed/fill and the aspect ratio gives the height | 0 |
| Width `Fixed` or `Fill` | 1, with `Some(final width)` |
| Width hugs and the natural width fits | 1, with `None` (or `Some(max_width)` if the style sets one) |
| Width hugs or is stretched, and the natural width does not fit | 2: natural, then `Some(available width)` |

The engine assumes that a leaf which reported natural width `w` reports the same size under any constraint of at least `w`. That holds for text and is what makes the one-call cases one call.

**Arena storage.** Nodes are a `Vec`, `NodeId` is a `u32` index, children are intrusive sibling links. `clear()` keeps both vectors' capacity. A warmed-up tree allocates nothing to rebuild and nothing to lay out. The tree is meant to be rebuilt, not mutated; there is no node removal.

**Overflow is allowed and reported, never repaired.** Nothing is shrunk below what its style asks and nothing is clipped. `overflow(node)` gives how far a container's children exceed its content box per axis, and `overflowing()` lists every such node. For the design-to-code loop this is a diagnostic an agent can act on ("the row at … overflows by 60") without rendering.

**Unit-agnostic, unrounded.** The engine computes in `f32` and stores exact results. `round_rect` and `snap_rect(rect, scale)` round a rectangle's *edges* on the way out, so siblings that share an edge still share it and no gaps accumulate; the cost is that equal thirds of 100 come out as 33, 34, 33. `snap_rect` exists for layouts in density-independent units once the hosts supply a scale factor (ADR 0008, item 4). Child positions are accumulated in absolute coordinates so that with zero gap one child's far edge and the next child's near edge are the identical float.

**Bad input has defined results and never panics.** Styles are sanitised when stored. NaN or negative sizes, padding and minimums are 0; a NaN maximum is no maximum; min beats max; NaN gap and offset are 0; a non-finite origin is 0; an infinite available size means unbounded; every length is capped at 1e9 so sums stay finite; NaN, negative or infinite measure results are sanitised the same way; invalid ids are ignored and `add_child` refuses anything that would not leave a tree (leaf parent, second parent, cycle).

## Rules that are deliberate and may surprise

- **Fill inside hug collapses.** A `Fill` child adds only its minimum size to a hugging parent's natural size. A hugging row containing only a fill-width text is zero wide. Figma forbids the combination; Flutter asserts; here it is defined and shows up as overflow.
- **Hugging children of a row are each offered everything left by fixed siblings and gaps**, independently. An icon, a hugging text and a fixed badge wrap correctly. Two long hugging texts in one row each take the lot and overflow. The remedy is `Fill` on the one that should give way.
- **`CrossAlign::Stretch` affects only children that hug on the cross axis.** Fixed children keep their size. `Fill` on the cross axis always fills.
- **Default alignment is start/start (top-left)**, including for stacks, as in Figma and unlike SwiftUI's centred `ZStack`.
- **When children do not fit**, `Center` and `End` still apply (overflowing on both sides or at the start) and the three `Space*` modes fall back to `Start`.
- **Gap may be negative** (overlapping avatars); padding may not.

## Consequences

- The widget layer gets a small, regular target: every design element maps to one of five node kinds and one `Style`.
- Text costs one measurement when it is `Fill` or fits, two when a hugging paragraph has to wrap. Widgets should make body text `Fill`.
- The whole subtree is recomputed on every `compute`. There is no dirty tracking; ADR 0006's "which subtrees re-lay-out" is not addressed here. Whether it is needed depends on measurements that have not been made. Because cost is linear and allocation-free, the expectation is that a screen of a few hundred nodes is cheap relative to text shaping, but that is an expectation, not a result.
- `morningstar-core`'s geometry types are documented as "physical pixels"; this crate uses them for unit-agnostic values. The doc comments there should be relaxed when density-independent units arrive.
- AGENTS.md's crate table and dependency direction do not mention `morningstar-layout` yet (`core` ← `layout`); that file was outside this change.

## Verified

All on the development host (macOS, `cargo test -p morningstar-layout`); 77 tests pass, `cargo clippy -p morningstar-layout --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean.

- `tests/layout.rs` (72 tests), expected rectangles computed by hand: root sizing against the available size; padding, gap, nesting; all six main alignments in rows and columns; all four cross alignments, `align_self`, stretch limits; fill weights, remainders, min/max redistribution, zero space, invalid weights, fill on the cross axis, fill inside hug; min/max on every sizing mode; measured leaves (natural, wrapping when hugging / filling / fixed / stretched, `max_width`, an unbreakable word, height pushing siblings down); the exact sequence of measure calls for a nested tree, and that 20 levels of the hug-column / row nesting still cost two calls; the nine stack anchors, `content_align` against `place`, `offset`; overflow on main axis, cross axis and stacks, and alignment under overflow; aspect ratio; rounding, including a sweep of 800 rows asserting rounded siblings tile exactly; NaN, negative, infinite and huge inputs; invalid ids; `add_child` refusals; `clear`, `set_style`, several roots; a tree 100 000 levels deep.
- A randomised test: 2000 trees of up to 41 nodes with styles, bounds and measure results drawn from NaN, infinities, `f32::MAX`, negatives and ordinary values. Asserts no panic, every rectangle finite with non-negative size, every constraint passed to the callback finite, and at most two calls per leaf.
- `tests/screen.rs` (2 tests): a 360 x 640 inbox screen (header with title and avatar plus status dot, two list rows with icon, two lines of text and a badge, bottom button) with all 23 rectangles hand-computed and one measure call per text; and the same row on a 120-wide screen, checking where the overflow is reported.
- `tests/alloc.rs` (1 test): with a counting global allocator, ten rounds of `clear`, rebuild (1201 nodes) and `compute` on a warmed-up tree perform zero allocations. This test contains the crate's only `unsafe` (the `GlobalAlloc` impl, which forwards to `System`); the library itself forbids unsafe code.
- 2 doctests, one of which is the crate-level example screen.

Not verified:

- **No timings.** No benchmark exists, on the host or on a device. The cost claims above are about complexity and call counts, which are tested, not about milliseconds.
- **Not run on Android or iOS.** The crate is pure arithmetic with no platform code or dependencies, so no difference is expected, but it has not been compiled for either target as part of this work.
- **Not exercised with real text.** Tests use a stand-in measurer (fixed-width characters, breaking anywhere). The "same size for any constraint at least the natural width" assumption has not been checked against `morningstar-text`; real line breaking at word boundaries and sub-pixel widths could expose cases where a constraint equal to the natural width wraps differently.
- **Nothing rendered.** No screenshot has been made from layout output.

## Known limitations

- **No right-to-left.** `Start` is always left. Arabic is a first-class target (ADR 0006), so mirroring rows and alignments is required before this is complete; it should be a per-tree or per-container direction applied at placement, which the algorithm leaves room for.
- **No wrapping containers and no grid.** A row never breaks onto a second line. Chip groups and photo grids have to be built from nested rows and columns.
- **No baseline alignment.** Text of different sizes in a row can be aligned top, centre or bottom, not on a shared baseline. The measure callback returns only a size.
- **No free absolute positioning.** A stack child goes to one of nine anchors plus an `offset`; there is no "left 12, top 40, right 12" inset model. Padding on the stack or a wrapping node covers most cases.
- **No margins, no percentage sizes**, no per-child gap overrides.
- **Aspect ratio covers two cases only**: height from width when the height hugs, width from a fixed height when the width hugs. Min/max can break the ratio. "Fit inside both dimensions" is not supported.
- **A hugging container does not shrink to wrapped text.** A chat bubble with `max_width` 280 around text that wraps to 240 is 280 wide, as in CSS. Shrinking it would need the wrapped width before the container's width is decided.
- **Measured leaves cannot depend on height**, and a leaf cannot report a minimum (unbreakable) width up front; it simply reports wider than asked and overflows.
- **No incremental layout and no caching between `compute` calls.**
- **Stale ids are not detected.** After `clear`, an old `NodeId` silently refers to whichever new node has that index. A generation counter would fix this at four bytes per id.
- **Fill distribution is quadratic in the worst case**: each round freezes at least one child that hit its min or max, so `k` fill siblings all with active, different bounds take up to `k` rounds of `k` steps. It is arithmetic only (no measurement) and is one round when no bound is active.
- **`f32` precision.** Coordinates beyond about 16 million units lose sub-unit precision; lengths are capped at 1e9.
- **`overflow` ignores `offset`** and does not look inside measured leaves.
