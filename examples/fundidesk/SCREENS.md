# Building a FundiDesk screen

All twelve screens are built, and their text fields are real inputs (see "Text fields" at the end). This is how a screen was built, and how to build or change one. Read `docs/building-screens.md` first, in particular "From HTML/CSS" and its pitfalls; read `src/screens/login.rs` (a form) and `src/screens/repair_details.rs` (cards between fixed bars) as the models.

## The rules

1. **You change one file: `src/screens/<your_screen>.rs`.** Screens are built in parallel.
2. **`src/theme.rs`, `src/icons.rs`, `src/components.rs`, `src/lib.rs`, `src/screens/mod.rs`, `examples/screenshot.rs` and the framework crates are frozen while screens are being built in parallel** (outside such a phase, as when text input was added, they are changed for all screens at once). They already cover all twelve screens: every colour, every icon, and every piece that occurs more than once.
3. **If something is missing, build it in your own file** from `Container`, `row!`, `col!`, `stack!` and the tokens, as a private function, and note it in your report under "would belong in components". Do not copy a shared component to change one number: most take modifiers (`.height(52.0)`, `.padding_xy(10.0, 5.0)`, `.on_press(..)`), because they return a `Container`.
4. **To request an addition** to a frozen file (a token, an icon, a component, a framework capability), write it in your report as: what the design has (quote the CSS), which screen and line, what you did instead, and what the difference costs in the diff. Additions are made between phases, by one person, for all screens at once.
5. **The one line you add outside your file** is your screen's limit in `tests/fidelity.rs` (`fn limit`), replacing the `_ => 0.50` default with your measured value. If two people touch that function at once the merge is trivial.
6. No colour literals, no new font sizes outside the design, no invented copy: every number and string comes from `design/html/NN-name.html`.

## The shape of a screen file

Keep the items the placeholder has; the application depends on them.

```rust
pub const BUILT: bool = true;                 // turns the fidelity test on for this screen
pub struct State { .. }                       // #[derive(Debug, Default)]
pub enum Message { .. }                       // #[derive(Clone, Debug, PartialEq)]
pub fn view(state: &State) -> Element<Message>;
pub fn update(state: &mut State, message: Message) -> Nav;   // Nav::Stay or Nav::To(Screen::..)
```

Navigation follows the design's `href`s (the placeholder's link list has them). A control the design gives no destination returns `Nav::Stay`. Selection state that the design shows (a chosen chip, a payment method) lives in `State` with the design's values as the default, so the default state *is* the reference picture.

Give the screen's root an id (`form_screen("verification", ..)` and `scaffold("repairs-list", ..)` do), and rows that are tappable an `.id(("row", index))`.

## Which skeleton

| Screens | Skeleton | Notes |
| --- | --- | --- |
| 02, 04, 05, 06 (390 x 844, white) | `form_screen(id, top_padding)` and push the blocks; `Spacer::fixed(n)` for each `margin-top: n`; `Spacer::new()` for `margin-top: auto` | nothing scrolls |
| 08 (tab bar) | `scaffold(id, CANVAS, None, body, Some(tab_bar(Tab::Repairs, Message::Tab).into()))` | as `dashboard.rs`; `tab_destination` maps a tab to a screen |
| 09, 11 (back bar, actions at the bottom) | `scaffold(id, WHITE, Some(TopBar::new(..).back(..).into()), body, Some(bottom_bar(..).into()))` | as `repair_details.rs`; 09's bar has `.note(meta("Today, 10:20"))` and a Space Grotesk subtitle |
| 12 (customer page) | `scaffold(id, CANVAS, Some(header), body, Some(footer))` with a header and footer built in the file | the header is a plain row, not a `TopBar` |

The block the design pushes to the bottom with `margin-top: auto` is the **footer**; everything between the header and it is the scrolling **body**. At the design's full height nothing scrolls and the picture is the reference; at 844 dp the body scrolls.

## What exists

**`theme.rs`.** Colours: `GREEN`, `GREEN_DARK`, `GREEN_TINT`, `GREEN_SOFT`, `INK`, `MUTED`, `PLACEHOLDER`, `CHARCOAL`, `WHITE`, `CANVAS`, `PANEL`, `NEUTRAL_TINT`, `OUTLINE`, `HAIRLINE`, `DASH`, `DOT`, `BLUE`, `BLUE_TINT`, `AMBER`, `AMBER_DARK`, `AMBER_TINT`, `RED`, `RED_TINT` (each documented with its hex value and use; grep the hex from the HTML to find the name). Shadows: `SHADOW_CARD`, `SHADOW_GREEN`. `radius::{BAR, SMALL, FIELD, BUTTON, CARD, HERO, PILL}`, `space::{FORM_MARGIN, PAGE_MARGIN, CARD, LABEL_GAP, CARD_GAP, FIELD_HEIGHT, BUTTON_HEIGHT, TOUCH}`.

Text always starts from one of these, never from `text(..)`:

| Function | Is |
| --- | --- |
| `inter(s, size)` | Inter 400, ink, the browser's default line height. Chain `.medium()`, `.semibold()`, `.bold()`, `.color(..)`, `.line_height_dp(n)` when the CSS has a `line-height` |
| `grotesk(s, size)` | Space Grotesk 700, likewise; `.medium()` / `.semibold()` for the lighter cuts |
| `inter_rich([span(..), ..], size)` | a sentence with styled phrases |
| `page_title(s)`, `lead(s)` | the 28/34 title and the 15/22 paragraph under it |
| `heading(s, 16.0)`, `overline(s)`, `label(s)`, `meta(s)` | card heading, small-caps section label, field label, 12 dp muted line |

**`icons.rs`.** `LOGO_MARK`, `CHECK`, `CHEVRON_LEFT`, `ARROW_RIGHT`, `PLUS`, `SEARCH`, `PHONE`, `PHONE_CHECK`, `LOCK`, `EYE`, `FINGERPRINT`, `SHOP`, `CLOCK`, `MESSAGE`, `PACKAGE`, `CHAT`, `CALL`, `SHARE`, `CALENDAR`, `CAMERA`, `BARCODE`, `DOWNLOAD`, `WRENCH`, `WRENCH_FILLED`, `HOME`, `HOME_FILLED`, `PEOPLE`, `PERSON`. Use: `Icon::vector(icons::CALENDAR).size(20.0).color(GREEN_DARK)`. The size is the `width` of the `<svg>`, the colour its `stroke` (or the `color:` of the nearest parent for `currentColor`). Each constant carries the design's most common stroke width; compare with the `stroke-width` in your HTML and add `.stroke_width(2.0)` if it differs (the doc comment lists the widths).

**`components.rs`.** Each doc comment quotes the CSS it reproduces.

| Design block | Component |
| --- | --- |
| The logo at any size | `logo(side, plate, mark)` |
| Green button; outlined green; grey-outlined | `Action::primary(..)`, `Action::outline(..)`, `Action::quiet(..)`, with `.compact()`, `.height()`, `.text_size()`, `.icon(icon, size)`, `.hug(padding)`, `.colors(fill, label)`, `.on_press()` |
| Icon in a 44 dp target | `icon_button(icon, size, color)` |
| Green text link | `link(text, padding_y)` |
| Input, with icon, `+255` prefix, trailing control, text area, search | `Field::new(&state.value).hint("placeholder").on_input(Message::X).id("screen-field")`, then `.leading(icon)`, `.leading_with_stroke(..)`, `.prefix(..)`, `.trailing(..)`, `.multiline(min_height)`, `.height()`, `.padding_x()`, `.border()`, `.radius()`, `.text(style)`, and `.input(\|input\| input.keyboard(..).secure(..).action(..))` for anything else of `TextInput`. A real input: see "Text fields" below. |
| Label over a field or a group | `labelled(text, element)` |
| Status pill; with icon | `pill(text, Tone::..)`, `icon_pill(icon, text, tone)`; `Tone::{Neutral, Info, Success, Warning, Danger}` and `.foreground()` / `.background()` |
| Chips and choices | `chip` (hugs), `wide_chip`, `filter_chip`, `choice`, `choice_with_check` |
| Code digit box | `code_box(digit, CodeBox::{Filled, Active, Empty}, id, on_input)`: a one-digit input that moves on when filled |
| Dashed "add" tile | `dashed_tile(icon, text)` |
| `display: grid` with n columns | `grid(columns, gap, vec![a.into(), b.into(), ..])`, or a `row!` when it is one row |
| White card; hairline; tinted strip | `card()`, `hairline()`, `banner(color)` |
| Icon on a tinted tile | `icon_tile(icon, tone)`; other sizes: `Icon::vector(i).size(52.0).radius(16.0).background(c).color(c)` |
| Caption over a figure | `stat_tile(caption, figure, background)` |
| Cost lines; label/value facts | `detail_row`, `total_row`, `fact_row` |
| List rows | `attention_row`, `repair_row`, `repair_card` |
| Status history | `timeline(vec![timeline_step(Step::Done, title, Some(detail)), ..])` |
| Segmented progress; page dots | `progress_segments(&[colors])`, `page_dots(active, count)` |
| Bars | `TopBar`, `bottom_bar(content)`, `tab_bar(selected, message)` |
| Screen skeletons | `form_screen`, `scaffold` |

## The loop for one screen

All scratch output goes outside the repository (`OUT` below is any directory elsewhere).

```sh
# 1. render at the reference's size and compare (a few seconds)
cargo run -q -p fundidesk --example screenshot -- "$OUT" 08 --diff --phone
# writes 08-repairs-list.png, .phone.png, .diff.png, .sbs.png and prints the report

# 2. look at "$OUT"/08-repairs-list.sbs.png: reference, render, heat map

# 3. when it is right: set BUILT, record the limit, run the tests
cargo test -p fundidesk
cargo clippy -p fundidesk --all-targets -- -D warnings && cargo fmt -p fundidesk --check
```

Work in this order; it is the order in which errors hide each other.

1. **Heights.** Compare where each block starts and ends against the reference, top to bottom. A difference that starts at one pixel and grows is a line height, a border or a padding (see the pitfalls). Blocks should land within one pixel all the way down before anything else is looked at.
2. **Horizontal insets.** The same across: card and field contents start at `border + padding`.
3. **Colours and weights**, read from the HTML, not judged from the picture.
4. **Icons**: right drawing, right size, right stroke width.
5. Also look at the `.phone.png`: the bars are fixed, the body scrolls, nothing is clipped. `layout overflows:` in the output means a box is too small for its children; fix it.

## The bar

The twelve screens differ from their references in **0.09% to 0.36%** of pixels (mean error 0.02 to 0.08) with the default comparison. A new screen is done when:

- every block is within one pixel of the reference in position and size, over the whole height;
- no region in the report is a missing, extra or miscoloured element: what remains is text, icons and curved edges of the right extent (a different rasteriser; blending, shadows and text weight are the browser's since ADR 0023);
- the differing share is **under 0.4%**; text-heavy screens sit near 0.3 to 0.4%, sparse ones near 0.1 to 0.2%. If you are above that, something structural is still wrong;
- nothing is corrected by hand: no fraction-of-a-dp nudge on a text, no pinned width on a box that hugs one, no alpha other than the design's (ADR 0022, ADR 0023). If a text sits wrong, a height above it is wrong;
- the limit in `tests/fidelity.rs` is your measured value rounded up to the next 0.05. **Never raise a limit to make a test pass**; find what moved;
- `ui.overflows()` is empty at the phone size (the test checks four scales).

Report, for your screen: the diff percentage, mean and maximum error; the three worst regions and what each is; everything approximated; everything you would move into the shared files.

## Known approximations to apply consistently

- **Inputs are real.** A `Field` is the design's frame around a `TextInput`; unfocused it renders exactly as the design's field does (the fidelity numbers did not move when the look-alikes were replaced). With focus it takes the design's focused border, `1.5px solid #1E8416`, and shows a green caret; that state is not in any reference image.
- **`flex-wrap` chips (09)**: a row that wraps, `Row::new().extend(chips).gap(8.0).line_gap(8.0).wrap().fill_width()`. It breaks where the reference breaks because the chips are as wide as the reference's; if a break differs, a chip's width does, and that is what to look at.
- **A link inside a sentence (04, 05)**: `inter_rich` for the text; make the whole line tappable.
- **Percent widths (02)**: compute from the parent's inner width.
- **`border-top` / `border-bottom`**: `hairline()` as the first or last child.
- **`position: absolute` (02)**: a `stack!` with `.place(..)` and `.offset(..)`.
- **`<br>` in a text (12)**: `\n` in the string.
- **Multi-colour SVG**: only the logo; use `logo(..)`.

## Text fields

Every `<input>` and `<textarea>` of the design is a real text field. The pattern, the same on every screen:

1. **The screen's `State` holds the string**, with the design's `value="..."` as its default (`new_repair` starts with "Amina Juma"; `verification` with the digits 4 and 7; `record_payment` with "40,000"). The default is what the fidelity test renders, so it must be what the reference shows.
2. **A message per field carries the new text** (`Message::Phone(String)`), and `update` stores it. To restrict what a field accepts, filter there (the payment amount keeps digits and commas) or on the input (`.digits_only()`, `.max_length(n)`).
3. **`view` passes the string back**: `Field::new(&state.phone).hint("07XX XXX XXX").on_input(Message::Phone).id("login-phone")`.
4. **Every field has an id**, named `screen-field`. It is the input's identity (focus and caret follow it) and what a test or a device script taps.
5. **Keyboard details go through `.input(..)`**: `KeyboardType::Phone` for phone numbers, `Number` for amounts and codes, `.secure(true)` for passwords, `ImeAction::Next` on every field but a form's last, `.on_submit(..)` on the last where the design has an obvious button (login, create account), `Capitalization::Words` for names.

| Screen | Fields (ids) |
| --- | --- |
| 03 login | `login-phone`, `login-password` (masked; `login-eye` shows and masks it and keeps the keyboard up) |
| 04 create account | `account-name`, `account-whatsapp`, `account-password`, `account-confirmation` |
| 05 verification | `("code", 0)` to `("code", 5)`: six one-digit boxes that behave as one field. A box is drawn `Filled`, `Active` or `Empty` from the digits entered, not from keyboard focus, so the design's picture (two digits, third box active) is the default state |
| 06 shop setup | `shop-name`, `shop-location` |
| 08 repairs list | `repairs-search` (the list is the design's sample and is not filtered) |
| 09 new repair | `("repair", TextField::Phone)`, `Customer`, `Brand`, `Model`, `Problem` (multi-line: at least the design's 88 dp, growing with the text) |
| 11 record payment | `payment-amount` (a quick-amount chip is selected while the field holds its figure), `payment-transaction` |

The form screens (03 to 06) do not scroll; when the keyboard would cover the focused field the screen is shifted up (ADR 0016). The tall screens scroll the field into view, and their bottom bars ride above the keyboard.

`tests/input.rs` drives these flows; its ignored test `device_tap_targets` prints pixel coordinates for a given device from the same layout code, for use with `tools/adb-guarded.sh`.
