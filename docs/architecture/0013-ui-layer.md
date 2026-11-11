# 0013 — The declarative UI layer

Status: Accepted (2026-11-11). Implemented in `crates/morningstar-ui`, re-exported as `morningstar::ui`. Verified on the development host only (macOS, Metal, offscreen): never run on a device.

Resolves the state-model question left open in [ADR 0006](0006-ui-architecture-direction.md), and is the "widget vocabulary" of [ADR 0008](0008-design-to-code-loop.md), item 5. It does **not** implement 0006's retained tree with reconciliation and dirty tracking; see "What this is not".

## Context

Applications painted a display list by hand and hit-tested raw pointer events (`examples/hello-gpu`). Everything needed for something better existed underneath: the renderer vocabulary (0011), text (0009), layout (0010), scale, insets and demand-driven frames (0012), images (0014).

The product goal sets the requirements: a developer or an AI agent is handed a picture of a screen and must reproduce it quickly and predictably. So the API must read like the structure of a design, have few concepts, sensible defaults and no rules that have to be discovered by failing.

## Decision

### An Elm-style application

```rust
impl Application for Wallet {
    type Message = Message;                       // Clone + Send + 'static
    fn view(&self) -> Element<Message> { .. }     // the screen for this state
    fn update(&mut self, message: Message) { .. } // the only place state changes
    fn setup(&mut self, setup: &mut Setup) { .. } // fonts, images (optional)
    fn theme(&self) -> Theme { .. }               // tokens (optional)
}
morningstar::app!(Ui::new(Wallet::default()));
```

Widgets carry message *values* (`Button::new("Send").on_press(Message::Send)`), not closures. This is what resolves the ownership problem recorded in 0006: `view` borrows `&self` and returns plain data; nothing in the tree borrows the state, so nothing needs `Rc`, `RefCell` or a lifetime parameter, and `Element<M>` has no lifetime. Signals and immediate-mode closures were the alternatives; both put borrowed or shared state inside the tree, which is exactly what is hard to generate correctly.

`Message: Clone` because the framework keeps the messages of the widgets on screen between frames and hands out a copy per tap. `Element::map` embeds a view written for one message type in a parent with another.

### One box type, a handful of leaves

`Container<M>` is the only widget with children, padding and decoration; it is a frame with auto-layout. `Column`, `Row`, `Stack`, `ScrollView`, `SafeArea`, `Pressable`, `Spacer` and the macros `col!`, `row!`, `stack!` are presets of it, so every modifier works on all of them. Leaves: `Text`, `Picture` (an image). Composites that lower to those: `Button`, `Divider`, `Avatar`, `Icon`.

Rejected: a `Widget` trait with boxed trait objects (an allocation per node and an open set of behaviours, for extensibility nobody needs yet), and SwiftUI-style wrapping modifiers where `.padding().background()` and `.background().padding()` differ (a rule that has to be learned by getting it wrong). Here a box has *one* padding and *one* background and their order in the chain does not matter. The price: a `Text` has no padding or background; wrap it in a `Container`.

The macro is `col!`, not `column!`: a glob-imported `column!` is ambiguous with the standard library's built-in macro of that name (E0659).

### Units are dp, everywhere

Every length in the API, font sizes included, is in dp. Layout runs in dp on the unit-agnostic layout engine; text is measured in physical pixels (`dp × scale`) and reported back in dp; each rectangle's *edges* are rounded to device pixels on the way out, so siblings that share an edge in dp share it in pixels at any scale, and anything with a size keeps at least one pixel. Border widths round to whole pixels (minimum one). Scroll translation is a whole number of pixels.

Colours: `hex(0xRRGGBB)` is a `const fn` that converts sRGB design values to the linear colours the renderer uses, so palettes are constants.

### Theme tokens resolved at paint time

`Theme { colors, type_scale, spacing, radii }` is plain `Copy` data with `Theme::LIGHT` and `Theme::DARK`. Every colour parameter is `impl Into<ColorRef>`: a literal `Color` or a `ThemeColor` token. Tokens, and `Text` roles (`.title()`, `.caption()`, …), are stored in the tree and resolved against `Application::theme()` when the frame is painted. That is why `view` needs no theme parameter and built-in widgets follow the theme. Spacing and radii are for application code (`THEME.spacing.lg`); `Button`'s metrics (48 dp minimum height, radius 12) are fixed and overridable per button.

### The frame pipeline

`Ui<A>` implements the runtime's `App`. Per frame:

1. `theme()`, `view()`.
2. **Lower** the element tree into the reused `LayoutTree` (cleared, not reallocated). The root is forced to fill the surface. `SafeArea` adds the insets (converted to dp) to a box's padding. A `Spacer` takes its axis from its parent.
3. **Compute** in dp. The measure callback calls `Frame::measure_text`.
4. **Paint**: walk the element tree again in the same order, emitting display-list operations (shadow, fill, border, pressed overlay, clip, children) and recording touch regions, already intersected with the clips in effect.

Text is drawn with exactly the constraint it was last measured with, so the text engine's shaping cache (keyed on text, style and width) is hit, not duplicated.

### Input

Messages are delivered in `App::pointer`, immediately, in event order. Rules:

- A tap is a finger down and up on the same pressable. Sliding off cancels; sliding back before lifting re-arms; a system cancel abandons.
- One gesture at a time. Fingers that land while one is in progress are ignored entirely.
- The innermost pressable under the finger wins. `block_touches()` makes a box swallow touches (a scrim).
- Inside a `ScrollView`, more than 8 dp of mostly vertical travel turns the gesture into a scroll and abandons the press, provided the content is taller than the viewport.
- Touching a list that is flinging stops it and is not a tap.
- After a message is delivered the touch regions are discarded. Until the next frame is laid out, nothing can be tapped: a screen that the update may have replaced is never hit-tested.
- If a pressed widget or a dragged scroll view is absent from a rebuilt frame, the gesture ends.

### Scrolling

Vertical only. The content is laid out at its natural height (the layout engine lets height overflow, as 0010 anticipated) and painted translated and clipped. Subtrees more than 96 dp outside the viewport are skipped at paint time, so a long list costs layout for all rows but drawing for the visible ones.

Fling: exponential decay with a 0.325 s time constant, integrated in closed form over each frame's `elapsed` step, so the distance does not depend on the frame rate. It stops at 12 dp/s or at either end of the content (no overscroll, no bounce). `request_redraw` is called only while a fling is moving; that is the layer's only source of animation frames. Pressed states are instantaneous.

Finger velocity is sampled once per frame (offset change over the frame interval, lightly smoothed), because pointer events carry no timestamps; a release more than 0.1 s after the last frame does not fling.

## The identity rule

The tree returned by `view` is discarded after every frame. Two pieces of state outlive it, held in `Ui` and keyed by widget identity: which pressable is held, and each scroll view's position and motion.

1. **Identity is position**: the chain of child indices from the root.
2. **`.id(value)` replaces it** with the hash of `value`. Ids are global to the screen, not scoped by the parent; two live widgets must not share one. Descendants of a widget with an id are positions under that id.
3. **State lasts while the identity is present in consecutive frames.** A scroll view missing from one frame starts again at the top.

Consequence, pinned by a test: two different screens with a scroll view at the same position share its offset unless the screens have ids. Hence the convention: `.id("home")` on each screen's root, and `.id(("row", item.id))` on list rows that can be inserted or removed.

## What allocates

Per rebuild:

- the element tree: one `Vec` per box that has children, plus the `String`s the application formats (`Text` takes `Cow<'static, str>`; a literal is not copied);
- one `Vec` of text references built during lowering.

Reused across frames, allocation-free once at working size: the layout tree, the node records, text metadata, touch regions, the message table, the scroll-state map, the display list. Shaping is cached by the text engine while the same text keeps appearing.

This is by construction and by reading the code. There is no counting-allocator test and no timing of any kind.

## What this is not

- **Not retained.** ADR 0006 sketched a retained tree with reconciliation and three levels of dirty tracking. This layer rebuilds, re-lays-out and repaints the whole screen for each frame. Frames are demand-driven (0012), so the cost is paid per input event or animation frame, not per vsync. Whether that is fast enough on a low-end phone is unmeasured. The public API (state, messages, `view`) does not preclude diffing underneath later.
- **Not accessible.** No semantic tree is produced.

## Limitations

- **No text input**, focus or keyboard.
- **No animation API.** The only motion is fling. No transitions, no animated pressed state.
- **Scrolling**: vertical only; no overscroll; no scroll bars; no programmatic scroll-to; nested scroll views do not hand off (the innermost takes the drag); all rows are laid out and measured even when off screen (no virtualised list).
- **Culling assumes containment**: a descendant that overflows its ancestor by more than 96 dp is not painted while that ancestor is off screen.
- **`max_lines` clips**; there is no ellipsis.
- **Text alignment acts within the text's own box**; a hugging single line has no slack, so centring it is the parent's job. A right-to-left paragraph should `fill_width()`; there is no RTL layout mode (0010).
- **One font family, no rich text, no baseline alignment** (0009, 0010).
- **A stack hugs its largest child.** Decoration that should not affect size goes in a layer that fills (`stack![..].fill()`), as the showcase's balance card does. `Ui::overflows()` then reports nothing only if such decoration fits its layer.
- **`SafeArea` uses the screen's insets wherever the box is**; it is meant for boxes that touch the screen's edges.
- **Gradient with border** is two operations and can show a faint fringe at rounded corners (0011).
- **Icons**: there are no vector icons. `Icon` is a glyph on a tile; the bundled Noto Sans has no arrows or symbols beyond `+ × • … › ‹ « »` and similar.
- **No `request_redraw` from `update`**, timers, or off-thread wake-ups (0012 has none either).
- **Gesture arbitration is minimal**: no long-press, double-tap, horizontal swipes or multi-touch gestures.

### Gaps found in lower layers, and the guards kept here

These are worked around in `morningstar-ui` and should be fixed where they belong:

1. **Huge font sizes panic in the text engine.** A font size around 1e9 px overflows glyph positions inside `cosmic-text` (`glyph_cache.rs`, "attempt to add with overflow"). *Guard here:* font size is capped at 2048 px (`MAX_FONT_PIXELS`). *Fix:* clamp `font_size` in `TextContext::shape` (`crates/morningstar-text/src/context.rs`).
2. **Huge text origins can overflow.** `TextContext::draw` computes `origin_x + glyph.x + slot.left` in `i32`; an origin near `i32::MAX` overflows in debug builds. *Guard here:* the origin is clamped to ±1e7 px (`MAX_TEXT_ORIGIN`), and text outside the clip is not drawn. *Fix:* saturating arithmetic in `draw`.
3. **`PointerEvent` has no timestamp** (`crates/morningstar-core/src/input.rs`), so velocity is per-frame. *Fix:* carry the host's event time.

## ADR 0010's unchecked assumption

0010 assumes that text reporting natural width `w` reports the same size under any constraint of at least `w`, and notes it was never checked against the real text engine. It holds: `text_measured_at_its_natural_width_does_not_wrap` measures seven strings (Latin, Swahili, Arabic) at 311 sizes each, at their natural width, its ceiling and one pixel more, and none wraps.

The remaining hazard is arithmetic, not text: dp round-trips can offer a hair less than the natural width. The measure callback therefore returns the natural layout whenever the constraint is within half a pixel of it (`a_padded_box_around_a_label_never_wraps_it` sweeps 160 padding and scale combinations).

## Verified

All on macOS; GPU tests on Metal through the offscreen renderer.

- `crates/morningstar-ui`: 2 unit tests, 48 integration tests (`tests/ui.rs`), 7 doctests. Layout in dp and px at seven scales, shared edges, safe area, spacers, stack anchors, overflow reporting; text sizing, wrapping, `max_lines`, centring; pictures (sizing, the emitted operation, avatar fallback); tap, slide-off and re-arm, cancel, second finger, message order, stale-screen taps, pressed visuals, nested pressables, blockers, disabled buttons, a pressed widget disappearing, `map`; scroll clamping, clipping and culling, taps through scrolled content, fling settle and idle, frame-rate independence, fling at an end, resting before lifting, stopping a fling; state retention and the identity rule both ways; redraw only while moving; theme switching; decoration order and pixel values; NaN, infinite and zero-size input; an app with no fonts; deterministic rebuilds.
- `examples/showcase`: 6 tests (`tests/screens.rs`). Navigation between the three screens by tapping, state changes, scroll then tap, no layout overflow and no text outside the safe area for 3 screens × 2 languages × 4 scales, an offscreen GPU render with pixel assertions (background, gradient card, white balance text, image avatar, tab bar, primary button, Arabic ink), and visual regression against three 360 × 800 references (190 KB in total).
- The three screens were rendered at 1080 × 2400 (scales 3 and 2.625) and inspected by eye.

## Not verified

- Anything on a device, on Vulkan or GLES, or on iOS.
- Performance: no timings, no allocation counts.
- Touch feel. Slop, decay and velocity sampling are reasoned values exercised only by synthetic events.
