# 0021 — The viewport in `view`, wrapping rows, scroll control, a bounded text field, animation and a navigator

Status: Accepted (2026-11-12). Implemented in `crates/morningstar-layout` and `crates/morningstar-ui`; `crates/morningstar-runtime` and the hosts are unchanged. Verified on the development host only (macOS, Metal, offscreen): nothing here has run on a device, and nothing has been timed.

Lifts, without changing their decisions: "No wrapping containers" of [ADR 0010](0010-layout.md); "No animation API", "no programmatic scroll-to" of [ADR 0013](0013-ui-layer.md); "Multi-line fields do not scroll internally and have no maximum height" and "Scroll-into-view adjusts the innermost scroll view only" of [ADR 0016](0016-text-input.md). It answers the gaps two applications recorded while being built on the layer (`examples/callready/SCREENS.md`, `examples/fundidesk/SCREENS.md`): a view cannot know the screen or measure text, rows cannot wrap, a scroll view cannot be scrolled by the application, a multi-line field grows without bound, nothing animates, `Ui::rect_of` is `None` for what is scrolled away, and every application writes its own screen stack.

## Context

ADR 0013 fixed the model: the application is state, `view(&self)` returns plain data without a lifetime, widgets carry message values, the tree is rebuilt for every frame, and frames are produced on demand. Everything below had to fit that model rather than add a second one beside it. Three properties decided most of the choices:

- **`view` is called once per frame anyway.** There is no retained tree whose rebuild a cleverer mechanism could avoid.
- **An `Element` has no lifetime**, so nothing in the tree can borrow the application's state. A closure stored in the tree would have to own what it reads.
- **Widget state lives in `Ui`, by identity**, for as long as the identity is in consecutive frames. Scroll positions and focus already work this way.

## Decision

### 1. The viewport is an argument of the view

```rust
fn view_in(&self, viewport: &Viewport<'_>) -> Element<Message>   // default: self.view()
```

`Viewport` has the surface's `size`, the safe-area `insets` and the `keyboard_inset`, all in dp; `scale`; `reduce_motion`; `safe_size()`, `keyboard_visible()`, `has_focus()` and `is_focused(id)`; and measures text: `measure(&Text) -> Size` on one line, `measure_wrapped(&Text, max_width)`, `line_count(&Text, max_width)`. `Application::view` gained a default body (an empty screen) so that an application implements one of the two.

Rejected: **an element resolved during layout** (`with_viewport(|viewport| ..)`, or a `LayoutBuilder` told the width its parent offers). The first saves nothing, because `view` already runs for every frame, including every frame of a keyboard change; and its closure would be `'static`, so it could read no application state without cloning it into the closure first, which is the borrowing problem ADR 0013 chose message values to avoid. The second would be more powerful (the offered width, not the screen's) but a subtree that is only known once its parent's width is cannot be laid out by four linear sweeps (ADR 0010): it needs layout to call back into view building and start again for that subtree. It can be added later as its own node kind; what the examples needed was the screen and the size of a text.

Measurement goes through the frame's text engine with the same function layout uses (`measure_text`), so the numbers agree with what is laid out (a test compares them), and measuring a text that is then shown shapes it once: layout finds it in the text engine's cache. `Viewport` holds the frame behind a `RefCell` so that a view passes `&Viewport` to its sub-views.

Cost when unused: none. The `Viewport` is seven numbers copied per frame.

### 2. Rows wrap in the layout engine

`Style::wrap` and `Style::line_gap` (layout), `Container::wrap()`, `Container::line_gap(dp)` and the preset `Wrap::new()` (UI). Rows only; columns and stacks ignore it.

It is done inside the four sweeps, with one new bit of scratch per node (`line_break`) and no allocation:

- **Sweep 1** (natural widths): a wrapping row's natural width is that of one line, as for any row.
- **Sweep 2** (final widths) is where lines are broken, because it is the first moment the row's width is known. Fixed children keep their width; a hugging child is offered the *whole line* (so a long label wraps inside its chip before anything overflows, and the bound of two measurements per text holds); a `Fill` child counts as its minimum width and then shares what its own line has left, by the same weighted distribution a plain row uses, restricted to the line. A child starts a new line when it does not fit after the ones before it; the first child of a line always stays. The comparison has the tolerance overflow reporting has, so a hugging row given exactly its natural width never wraps through rounding (a test sweeps 1100 rows of thirds and sevenths).
- **Sweep 3** (natural heights): the tallest child of each line, summed, plus `line_gap` between lines. This is the height a parent, and a scroll view, see.
- **Sweep 4** (placement): line by line; `main_align`, `cross_align` and `align_self` act within a line. A row taller than its lines shares the spare height equally among them (CSS `align-content: stretch`), which is the rule that makes **a wrapping row whose children fit on one line identical to a row that does not wrap** (pinned by comparing display lists).

Overflow: horizontally, only a single child wider than a line; vertically, lines taller than a row with a fixed height.

Not done: the row does not shrink to its widest line when it hugs and wraps (it is as wide as offered, like a container around wrapped text, ADR 0010); no `wrap-reverse`, no `align-content` other than the one rule, no wrapping columns (height flows up, so a column has no height to wrap at).

Cost when unused: one branch per row in three of the sweeps.

### 3. Scroll control

**Positions stay widget state.** A scroll position is changed by the user's finger between frames; if `view` declared it, every drag would have to be reported back through `update` to be declared again. So the application does not set a position, it *asks for a move*, once, through the channel it already uses for everything that is not a change of its own state:

```rust
fn effects(&mut self, out: &mut Effects<Message>) {
    if std::mem::take(&mut self.show_latest) {
        out.scroll(ScrollTo::end("chat").animated());
    }
}
```

`ScrollTo::top(id)`, `::end(id)`, `::offset(id, dp)`, `::reveal(widget_id)` (with `.margin(dp)`), each optionally `.animated()`. Requests are held until the next frame and carried out after its layout and before its paint, so "the end" is the end of the content that frame has (the line added by the same `update` included), and a request whose target is not in that frame is dropped. `Ui::scroll_to` does the same from a test.

`reveal` scrolls **every** scroll view around the widget, the innermost first, each as little as brings the widget into its viewport; the rectangle is carried outwards minus each offset. An animated request is a 250 ms ease-out glide evaluated from the frame clock, which a touch stops.

**(b) The rule for rebuilds** was already true and is now stated and tested: while a scroll view with the same identity is in consecutive frames its position is kept; content that grows leaves it where it is; content that shrinks under it brings it back to the new end.

**(c) One way of keeping the focused field in view.** ADR 0016 had three mechanisms chosen by where the field is, and a field inside a scroll view that could not scroll far enough was left under the keyboard. Now: every scroll view around the field is scrolled (the same `reveal`, with the field's margins; the keyboard lowers what counts as visible in the outermost one), and then, if the keyboard still covers where the field ends up, or there is no scroll view at all, the whole screen is drawn shifted up by the remainder. For a field in a scroll view the shift is decided when the field is revealed (on focus, on an edit, when the keyboard changes) and kept, so that scrolling by hand afterwards does not drag the screen about. Padding the bottom safe edge for the keyboard is unchanged.

**(d) `Ui::rect_of(id)` is where a widget was laid out**, whether or not it can be seen; `Ui::visible_rect_of(id)` is the part its clips leave. The first used to be `None` for a widget in a subtree that painting skipped for being far outside its scroll view. Lowering now lists the elements that have explicit ids (in tree order, with their record), and when painting skips a subtree it reports the ids inside it from that list: a binary search per skipped subtree and a step per id in it. For those, a scroll view or a moving transform between the skipped ancestor and the widget is not counted.

Cost when unused: an empty-vector test per frame for requests; one push per element with an explicit id and per scroll view (into vectors reused across frames).

### 4. A bounded multi-line field is a scroll view of its own text

`TextInput::max_lines(n)`, or `.max_height(dp)`. The bound is put on the *layout* (the measured leaf reports at most that height), so nothing overflows by construction. The text is clipped to the field and translated by an offset that is an ordinary scroll state under the field's own identity: it survives rebuilds, is dragged and flung like a scroll view, and is kept while the field has no focus. When the caret may have moved (a key, a tap, a value changed by the application) the offset is adjusted to hold the caret's line in the field with one line of context above and below where there is room; that margin is what lets a reader reach hidden lines by tapping the first or last visible one. A drag does not move the caret, so the two do not fight.

Rejected: a clipped window pinned to the end of the text (what CallReady had built by hand; it overflowed, and showed the wrong lines once the caret was moved up).

Cost when unused: one subtraction per text input painted.

### 5. Animation: `view` says where things are; a widget may ask to be shown getting there

Nothing in the application drives an animation, and there is no animation object to hold. Three entry points:

```rust
container.animate(Duration::from_millis(160), Easing::EaseOut)   // implicit, by identity
container.transition(Transition::slide(0.0, -1.0).with_fade())   // entering and leaving
ScrollTo::end("chat").animated()
```

**`.animate`** covers the widget's own opacity, transform, layout offset, solid background colour and fixed width and height. Per identity the layer keeps what the view last asked for and, while a change is in progress, where it started and when; the shown value is the eased mix at the frame's `elapsed`. A change that arrives mid-way starts from the shown value. A widget seen for the first time is shown as asked. Size and offset are substituted into the style *before* layout, so neighbours move frame by frame; opacity, transform and colour are applied at paint. A size that hugs or fills is not animated: the layout engine decides it, and animating it would mean laying out twice.

**Opacity is multiplied into every colour drawn** in the subtree (fills, borders, text, shadows, tints). The display list has no group opacity, and adding an offscreen layer to the renderer was outside this change; so overlapping children show through each other while faded. That is right for fading something in or out and wrong for holding a group at 50%. `.opacity(x)` is available on its own with the same meaning.

**`.transition`** is about identity entering and leaving the tree. Entering: the widget starts hidden as the transition says (transparent, displaced by a multiple of its own size, scaled) and arrives. Leaving is the hard half in a layer that throws the tree away: when an identity with a transition is absent from a frame, its subtree is *taken out of the previous frame's tree* and kept as a ghost, laid out as a second root in the rectangle it last had and painted after the screen, with no touch regions, no focus, no ids, and its scroll views held where they were. It is dropped when its exit has run, or revived (from where it had got to) if the identity returns first. Two consequences are part of the contract: a leaving widget is drawn **on top of the screen**, not at its old depth, and it **takes no room**, so its neighbours close up at once.

The previous frame's tree is kept only while some widget in it carries a transition, and is searched only in a frame where one of them has left.

**What replaces a widget says how it leaves.** A screen pushed from the right should send the old screen left; popped, the same screen leaves right. The old screen's own transition was built before anyone knew which. So: if, in the frame a widget leaves, another widget with a transition enters *the same box*, the leaving widget takes the newcomer's `exit`; otherwise its own. One value read from application state when the frame is built (`navigator.transition()`) then describes both halves. This is the role an `AnimatedSwitcher` plays elsewhere, without a widget for it.

**Frames.** Each of the three sets the frame's redraw request while it is on its way and at no other time; the frame that shows the end asks for nothing. Tests assert `redraw_requested()` on every frame of each kind and that `redraw_after()` is `None` afterwards.

**Reduce motion.** `Ui::set_reduce_motion(true)` makes every duration zero: changes show at once, nothing enters or leaves gradually, animated scrolls jump. Flings are unaffected. `Viewport::reduce_motion` tells a view. **The hosts do not read the platform setting yet** (Android `Settings.Global.ANIMATOR_DURATION_SCALE == 0`, iOS `UIAccessibility.isReduceMotionEnabled`): that needs a value on `FrameInfo` and one call in each host, and is not done.

**Flings: checked, unchanged.** Scrolling does not stop dead on finger-up: ADR 0013's fling (exponential decay, integrated exactly per frame) is there and tested. There is still no overscroll or bounce; adding it would change what every scrolled test and screen shows at its ends and was left out.

Cost when unused: per element, one comparison in lowering and one in painting; an empty-map test per frame.

### 6. `Navigator<S>`

A `Vec<S>` that is never empty, with `new`, `current`, `current_mut`, `root`, `stack`, `depth`, `can_pop`, `push`, `replace`, `pop`, `back() -> bool` (the answer `Application::back` wants), `pop_to_root`, `pop_to`, `reset`, `set`, `last_change()` and `transition()`. It is application state and nothing else: it holds no widget state, does not know what a screen looks like and starts nothing. `transition()` returns the `Transition` for the current screen's root that suits the last change (push: in from the right, the old one a quarter left and fading; pop: the reverse; replace: cross-fade; none at start).

It is not a router: no paths, no guards, no nested navigators, no results passed back. Those are application decisions and an `enum` with data does them.

## Used in the examples

- **CallReady**: the tip chips are one wrapping row, in the scripted call and the server's (the character-count estimate that chose a chip's form is gone); the online call's scroll view keeps one id and the application asks for its top when the caller says a new line; the reply field is `.max_lines(3)` and `tests/online.rs` allows no overflow; the history stack is a `Navigator<Screen>`; the end-to-end tests no longer scroll to find a control.
- **FundiDesk**: the condition and accessory chips of the new-repair screen are wrapping rows. It has no history stack to replace (its design links every screen by name).
- **Fidelity** did not move: with the old and the new code built against the same renderer, CallReady's `03-live-call` differs from its reference in 0.0064% of pixels both ways, and all twelve FundiDesk figures are identical to the fourth decimal. (The figures in the two `SCREENS.md` files changed on the same day for another reason, ADR 0022 and 0023.)
- One visible difference outside the designed frame, intended: where two tips are short they now share a line, as the design's `flex-wrap` says. On the Galaxy A27 a scripted call's first moment therefore fits without scrolling, and two end-to-end tests that used it as "a screen that scrolls" use the small phone instead.

## Consequences

- `Application::view` is no longer a required method. An application that implements neither shows an empty screen instead of failing to compile.
- `morningstar_layout::Style` has two more public fields. Code that builds one with struct-update syntax or the builder methods is unaffected; an exhaustive literal is not (there was none in the repository).
- `Ui::rect_of` returns `Some` where it returned `None` for a widget scrolled far out of view. Tests that used `None` to mean "not visible" now ask `visible_rect_of` (two in this repository were changed, and two test harnesses that scrolled to look for a widget no longer do).
- `Effects` carries requests that are not for the platform. They never reach a host.
- While a widget with a transition is on screen, the element tree is freed one frame late.
- A fading rich text whose spans have their own colours is shaped again for every frame of the fade: the text engine's cache keys on span colours (it does not key on the text's own colour, so plain text fades for free). The fix belongs in `morningstar-text`.

## Limitations

- **No group opacity**, as above.
- **A leaving widget is on top and takes no room.** A row removed from the middle of a list fades over its neighbours, which have already closed up; animating the gap closing needs the size of the leaving widget to stay in layout, which is not built.
- **Transitions need distinct identities.** The same screen pushed twice with the same id neither enters nor leaves.
- **A transform is mixed component-wise**: exact for moves and scales, an approximation for turns of more than a few degrees (it shrinks slightly mid-way).
- **Colours are mixed in the renderer's own colour values**, channel by channel.
- **No keyframes, springs, delays, repeats or completion messages.** An application that needs "when the animation is done" has `tick`.
- **Hug and fill sizes are not animated.**
- **`reveal` and focus scrolling treat nested viewports in layout coordinates**: an inner scroll view's own displacement by an outer one is accounted for by carrying the rectangle outwards, but the keyboard is only considered at the outermost.
- **A bounded field cannot be scrolled by `ScrollTo`**, and does not hand a drag on to the scroll view around it when it reaches its end (ADR 0013: nested scroll views do not hand off).
- **`Viewport` is the screen**, not the box a view will be placed in.
- **Wrapping**: rows only; no shrink-to-widest-line.
- **Not accessible**: reduce motion is a flag nobody sets yet.

## Verified

On the development host (macOS; GPU tests on Metal through the offscreen renderer):

- `crates/morningstar-layout`: 13 new tests in `tests/wrap.rs` with hand-computed rectangles (one line identical to a plain row; breaking; alignment per line; hugging; height seen by what follows and by a viewport; text offered the whole line, with the exact measure calls; fill within a line; stretch and fill-height within a line; spare height; overflow; ignored on columns; no wrap at the natural width over 1100 rows; 3000 random rows with hostile values). The zero-allocation test now builds 25 wrapping rows. 90 tests in the crate.
- `crates/morningstar-ui`: 35 tests in `tests/adaptive.rs`, 5 unit tests in `motion.rs` and `navigator.rs`, and the doctests of every new type. The viewport's numbers through a keyboard change and a rotation; a layout chosen from the width; measurement against layout; wrapping in dp, inside a scroll view, and identical display lists when everything fits; scroll to end, top, offset; a glide at fixed times, its frames and its end; a touch stopping it; reveal with and without margin, nested; positions through growing and shrinking content; laid-out and visible rectangles; the focused field through nested scroll views and panned when scrolling cannot help; the bounded field's heights, caret, clip, tap, drag and unfocused state; an animated offset, size, colour and opacity at fixed times, a change of mind, easing; a transition entering, leaving (still drawn, not tappable, then gone), returning mid-way; reduce motion for each; the navigator with Back and with push, pop and replace transitions frame by frame; idleness after every one.
- One GPU pixel test (`intermediate_frames_as_pixels`): three frames at 0, 100 and 200 ms of a 200 ms change, with a box mid-way along its path, a box mid-way through growing and a leaving box half faded; the middle frame was also looked at.
- The examples' own suites pass with their fidelity limits unchanged; the live call and the online call were rendered and looked at.

## Not verified

- **Anything on a device**: how a glide, a slide between screens or a fade looks at 120 Hz on the Galaxy A27; whether 250 to 300 ms and ease-out feel right; frame times while a screen transition lays out and paints two screens.
- **Performance**: no timings. The cost statements above are counts of operations read from the code.
- **The bounded field with a real keyboard**: composition inside a scrolled field, and the input method's own idea of where the caret is.
- **iOS**: nothing here is platform code, but nothing was compiled for it either.
