# Building screens: from a picture to code

For a developer or an AI agent handed a picture of a mobile screen. The API is `morningstar::ui`; the reasoning behind it is in [ADR 0013](architecture/0013-ui-layer.md); the compare-and-fix loop is in [design-to-code.md](design-to-code.md). `examples/showcase/src/lib.rs` is three complete screens to imitate.

## The shape of every app

```rust
use morningstar::ui::prelude::*;

#[derive(Default)]
struct Wallet { balance_hidden: bool }      // 1. state

#[derive(Clone)]
enum Message { ToggleBalance }              // 2. what widgets can ask for

impl Application for Wallet {
    type Message = Message;

    fn setup(&mut self, setup: &mut Setup<'_>) {
        setup.add_font(include_bytes!("../assets/fonts/NotoSans-Regular.ttf"));
        setup.add_font(include_bytes!("../assets/fonts/NotoSans-Bold.ttf"));
    }

    fn view(&self) -> Element<Message> {    // 3. the screen for this state
        SafeArea::new(
            col![
                text("Salio kuu").caption().muted(),
                text(if self.balance_hidden { "TSh •••••" } else { "TSh 1,250,400" }).display(),
                Button::new("Ficha").on_press(Message::ToggleBalance).fill_width(),
            ]
            .gap(8.0)
            .padding(20.0),
        )
        .into()
    }

    fn update(&mut self, message: Message) { // 4. the only place state changes
        match message {
            Message::ToggleBalance => self.balance_hidden = !self.balance_hidden,
        }
    }
}

morningstar::app!(Ui::new(Wallet::default()));
```

`Cargo.toml` depends on `morningstar` only (`morningstar-devtools` as a dev-dependency for screenshots), with `crate-type = ["cdylib", "lib"]`.

**All numbers are dp** (the units of a 1x design: a 360 or 390 wide artboard), font sizes included. Without fonts registered in `setup`, text is invisible.

## Reading a design

Work top-down. Every region of the picture is one of these:

| In the design | Write |
| --- | --- |
| Things stacked vertically | `col![a, b, c]` |
| Things side by side | `row![a, b, c]` |
| Things on top of each other (badge on avatar, text on art) | `stack![below, above]`; pin a child with `.place(Alignment::TOP_RIGHT)`, nudge with `.offset(x, y)` |
| Space between children | `.gap(12.0)` on the row or column |
| Space inside a frame | `.padding(16.0)`, `.padding_xy(h, v)`, `.padding_ltrb(l, t, r, b)` |
| Fixed size | `.width(120.0)`, `.height(48.0)`, `.size(w, h)` |
| "Hug contents" | the default |
| "Fill container" | `.fill_width()`, `.fill_height()`, `.fill()`; shares: `.fill_width_weight(2.0)` |
| Min / max | `.min_width(..)`, `.max_width(..)`, `.min_height(..)`, `.max_height(..)` |
| Children centred | `.center()` |
| Children at both ends | `row![left, Spacer::new(), right]` or `.space_between()` |
| Vertically centred items in a row | `.cross_align(CrossAlign::Center)` |
| One child aligned differently | `.align_self(CrossAlign::End)` on the child |
| Card: fill, corner radius, stroke, shadow | `.background(c).radius(16.0).border(1.0, c).shadow(elevation::LOW)` |
| Gradient fill | `.gradient(Gradient::diagonal(a, b))` (also `vertical`, `horizontal`, `angled`) |
| Content cut off by rounded corners | `.clip()` |
| Text | `text("…")` with `.display() / .title() / .headline() / .body() / .label() / .caption()`, or `.size(17.0).semibold()`; `.color(..)`, `.muted()` |
| Centred text | `text("…").centered().fill_width()` |
| Paragraph that wraps | `text("…").fill_width()` |
| One line, cut off | `.max_lines(1)` (no ellipsis) |
| Button | `Button::new("Label").on_press(msg)`; `.secondary() / .outlined() / .plain() / .destructive()`; full width: `.fill_width()` |
| Anything else tappable (row, card, chip) | `.on_press(msg)` on its box, or `Pressable::new(child).on_press(msg)` |
| Hairline | `Divider::new()`, indented: `.inset(68.0)`; in a row: `Divider::vertical()` |
| Fixed gap | `Spacer::fixed(24.0)` |
| Scrolling area | `ScrollView::new(col![..])` |
| Status bar / home indicator margins | `SafeArea::new(screen)`; for a bar whose background runs under the system bar: `.safe_area(SafeEdges::TOP)` or `BOTTOM` on the bar |
| Photo, illustration, logo | `Picture::new(image)` with `.size(..)`, `.radius(..)`, `.fit(ImageFit::Cover)`; register in `setup` with `setup.add_image_png(include_bytes!(..))` and keep the handle in the state |
| Profile picture | `Avatar::new("Amina Juma").image_maybe(self.photo)` (initials when there is no image) |
| Icon | `Icon::vector(ICON).size(20.0).color(c)`, with `const ICON: VectorIcon = VectorIcon::svg(r#"<svg viewBox=..>..</svg>"#)` pasted from the design; `Icon::new("+")` as a stand-in |
| A second typeface | `.family("Space Grotesk")` on a text; the defaults per role in `Theme { fonts: Fonts::new(body, heading), .. }` |
| Letter spacing, capitals | `.letter_spacing(0.6)` (dp), `.uppercase()` |
| Line height as a length | `.line_height_dp(22.0)` |
| A bold or coloured phrase in a sentence | `Text::rich([span("New here? "), span("Join").semibold().color(c)])` |
| Dashed outline | `.border_dashed(1.0, c)` (what a browser draws), or `.border(1.0, c).border_dash(4.0, 3.0)` |
| Colour from the design | `hex(0x2B3FE0)`, `hexa(0x2B3FE080)`; theme roles: `ThemeColor::Primary`, `Surface`, `TextMuted`, … |

Colours: put the palette in constants (`const INDIGO: Color = hex(0x2B3FE0);`) and the roles in a `Theme` returned from `theme()`:

```rust
const THEME: Theme = Theme {
    colors: Colors { primary: INDIGO, ..Theme::LIGHT.colors },
    ..Theme::LIGHT
};
```

## A worked example

The design: a white card with 20 dp corners. Inside, rows of a 40 dp round avatar, a name in semibold with a grey caption under it, and an amount on the right in green or black. Hairlines between rows start under the text.

Read it as structure first:

```
card  = column, white, radius 20, soft shadow, clipped
  row = [avatar 40] [column: name / caption — takes the remaining width] [amount]
        gap 12, padding 16 x 12, vertically centred, tappable
  divider inset by 16 + 40 + 12 = 68
```

Then write exactly that:

```rust
fn transaction_row(index: usize, name: &'static str, when: &'static str, amount: String, incoming: bool)
    -> Container<Message>
{
    row![
        Avatar::new(name),
        col![
            text(name).semibold().max_lines(1).fill_width(),
            text(when).caption().muted().max_lines(1).fill_width(),
        ]
        .gap(2.0)
        .fill_width(),                      // the part that gives way
        text(amount).label().color(if incoming {
            ColorRef::from(ThemeColor::Success)
        } else {
            ColorRef::from(ThemeColor::Text)
        }),
    ]
    .gap(12.0)
    .cross_align(CrossAlign::Center)
    .fill_width()
    .padding_xy(16.0, 12.0)
    .on_press(Message::OpenTransaction(index))
    .id(("transaction", index))
}

fn transactions(rows: Vec<Container<Message>>) -> Container<Message> {
    let mut card = Column::new()
        .fill_width()
        .radius(20.0)
        .background(ThemeColor::Surface)
        .shadow(elevation::LOW)
        .clip();
    for (index, row) in rows.into_iter().enumerate() {
        if index > 0 {
            card = card.push(Divider::new().inset(68.0));
        }
        card = card.push(row);
    }
    card
}
```

Small functions that each return one piece (`Container<Message>` or `Element<Message>`) keep `view` reading like the design's layer list. Use `Column::new().push(..)` / `.extend(iter)` / `.push_maybe(option)` when children come from data, the macros when they are written out.

## A form

A text field is two things: a `TextInput`, which is only the text (like `Text`), and the `Container` around it, which is the field's frame. The application owns the string.

```rust
#[derive(Default)]
struct Login { phone: String, password: String, show_password: bool }

#[derive(Clone)]
enum Message { Phone(String), Password(String), TogglePassword, LogIn }

fn view(&self) -> Element<Message> {
    // The design's field: 52 dp tall, 1 dp outline, 12 dp corners.
    let frame = |content: Element<Message>| {
        Row::new()
            .push(content)
            .fill_width()
            .height(52.0)
            .cross_align(CrossAlign::Center)
            .padding_xy(16.0, 0.0)
            .radius(12.0)
            .border(1.0, ThemeColor::Divider)
            .focused_border(1.5, ThemeColor::Primary) // also makes the whole box tappable
    };
    SafeArea::new(
        ScrollView::new(
            col![
                frame(
                    TextInput::new(&self.phone)
                        .placeholder("07XX XXX XXX")
                        .keyboard(KeyboardType::Phone)
                        .action(ImeAction::Next)       // the keyboard's key moves to the next field
                        .on_input(Message::Phone)
                        .id("phone")
                        .into(),
                ),
                frame(
                    row![
                        TextInput::new(&self.password)
                            .placeholder("Password")
                            .secure(!self.show_password)
                            .on_input(Message::Password)
                            .on_submit(Message::LogIn) // the keyboard's Done key
                            .id("password"),
                        Pressable::new(icon(EYE)).on_press(Message::TogglePassword),
                    ]
                    .fill_width()
                    .cross_align(CrossAlign::Center)
                    .into(),
                ),
                Button::new("Log in").on_press(Message::LogIn).fill_width(),
            ]
            .gap(16.0)
            .padding(24.0),
        ),
    )
    .id("login")
    .into()
}

fn update(&mut self, message: Message) {
    match message {
        Message::Phone(phone) => self.phone = phone,          // store what was typed...
        Message::Password(password) => self.password = password,
        Message::TogglePassword => self.show_password = !self.show_password,
        Message::LogIn => { /* ... */ }
    }
}
```

What to know:

- **`on_input` gets the whole new string**, after every change. Store it and pass it back as the value. To restrict or reformat what is typed, change it in `update` (keep only digits, upper-case it): the field then shows what you stored. For the common cases there are `.digits_only()` and `.max_length(n)`.
- **Without `on_input` a field cannot change.**
- **Style the text on the `TextInput`** (`.size()`, `.weight()`, `.color()`, `.family()`, `.placeholder_color()`, `.caret_color()`, or `.style(a_text)` to copy a `Text`'s style) **and everything else on the box around it.** Unfocused, a `TextInput` measures and draws exactly like a `Text` in the same style, so a field can be matched to a design before anyone types.
- **`focused_border` / `focused_background`** on the box are the focused look. Keep the layout the same in both states: a border is drawn over the box's edge and does not move the content, so a thicker focused border is fine.
- **Give inputs an `.id(..)`.** Focus and the caret are kept by identity (see "State and identity" in the `morningstar::ui` documentation); an id also lets a test tap the field (`ui.rect_of("phone")`).
- **Keyboard**: `.keyboard(KeyboardType::{Text, Number, Decimal, Phone, Email})`, `.capitalization(..)`, `.action(ImeAction::{Done, Next, Go, Search, Send})`, `.no_autocorrect()`, `.secure(bool)`, `.multiline()`.
- **Put a form in a `ScrollView`** if it can be taller than the space above the keyboard: the focused field is scrolled into view. A bottom bar that uses `.safe_area(SafeEdges::BOTTOM)` rides above the keyboard by itself. A screen that does not scroll is shifted up while the keyboard covers the focused field, which is a fallback, not a layout.
- **Code boxes**: one `TextInput` per box with `.max_length(1).digits_only().auto_advance().align(TextAlign::Center)`, each in a `Container::new(input).center().input_frame()`. Typing moves on, backspace moves back.
- **Focus** comes from a tap and goes when the user taps anything else, presses the keyboard's Done, or dismisses the keyboard. The application cannot set it.
- **Testing a form**: drive `Ui` as the engine does. `ui.pointer(..)` a tap on `ui.rect_of(id)`, then `ui.text_input(&TextInputEvent::Commit("a".into()))` per key, a `ui.frame(..)` after each, and assert on your state. `examples/fundidesk/tests/input.rs` is the model.
- **Not there**: selection, copy and paste, moving the caret with arrow keys, autofill, composition on iOS. See [ADR 0016](architecture/0016-text-input.md).

## The loop

Add an example next to the app (copy `examples/showcase/examples/screenshot.rs`):

```rust
use morningstar::ui::Ui;
use morningstar::{FrameInfo, Insets, SurfaceSize};

let info = FrameInfo::new(SurfaceSize::new(1080, 2400))
    .with_scale(3.0)                                  // 360 x 800 dp
    .with_insets(Insets::new(0.0, 96.0, 0.0, 60.0));  // status bar, gesture bar (px)
let mut ui = Ui::new(MyApp::default());
ui.send(Message::Open(Screen::Settings));             // reach the state the design shows
std::fs::write("out/settings.png", morningstar_devtools::screenshot_frame(&mut ui, &info)?)?;
println!("{:?}", ui.overflows());                     // boxes whose children do not fit
```

1. **Render**: `cargo run -p my-app --example screenshot out/` (seconds).
2. **Look at the PNG.** Always. Passing tests do not show that a screen looks right.
3. **Compare** with the design when there is one, rendered at the design's size times a whole scale:
   `cargo run -q -p morningstar-devtools --bin morningstar-diff -- design.png out/settings.png --out out/diff.png --ignore 0,0,1080,96`
   and fix the worst region first (how to read the report: [design-to-code.md](design-to-code.md)).
4. **Check `ui.overflows()`** is empty: an entry names a box (in dp) that is too small for its children and by how much.

To capture a state that needs touches (a pressed row, a scrolled list): build a frame first so the app knows where things are (`Frame::for_app(&mut ui)`, `frame.begin()`, `ui.frame(&info, &mut frame)`), send `PointerEvent`s in pixels to `ui.pointer`, then screenshot. `ui.rect_of("some-id")` gives the pixel rectangle of any widget with an `.id(..)`.

Lock a finished screen with a small reference (render at scale 1 to keep the PNG small):

```rust
let png = screenshot_frame(&mut ui, &FrameInfo::new(SurfaceSize::new(360, 800)))?;
assert_matches_reference(&png, "tests/references/settings.png", Tolerance::DEFAULT);
// create or update: MORNINGSTAR_UPDATE_REFERENCES=1 cargo test -p my-app
```

## Pitfalls

Each of these produces a wrong picture, not an error.

- **Fill inside hug collapses.** `fill_width()` on a child of a box that hugs gives the child zero width (text vanishes). Give the parent a width: usually `.fill_width()` up the chain to something sized by the screen.
- **In a row, exactly the element that should give way gets `.fill_width()`.** Two long hugging texts in one row overflow instead of wrapping.
- **A `ScrollView` needs a height.** It fills by default, so its parent chain must have one: `col![header, ScrollView::new(..), tab_bar]` at the root works; a scroll view inside a hugging card does not.
- **Default alignment is top-left**, for stacks too. Centring is explicit: `.center()` on the parent.
- **The root always fills the screen, edge to edge.** Content needs `SafeArea::new(..)` or it sits under the status bar.
- **`Text` has no padding or background.** Wrap it: `Container::new(text("TZS")).padding_xy(10.0, 4.0).radius(99.0).background(..)`.
- **`text(..).size(n)` is the font size**; on boxes `.size(w, h)` is width and height.
- **Text alignment needs room.** `.centered()` does nothing on a hugging text; add `.fill_width()`.
- **A stack is as large as its largest child.** Decorative shapes that should not size a card go in a layer that fills: `stack![stack![circle, blob].fill(), content]` with `.clip()` on the card.
- **Give each screen's root an `.id("name")`.** Scroll position belongs to a widget's place in the tree; two screens with a scroll view in the same place would otherwise share it. Give list rows that can come and go `.id(("row", item.id))`.
- **State changes only in `update`.** `view` takes `&self`. A value a widget should change needs a `Message`.
- **`Message` must be `Clone`** (`#[derive(Clone)]`).
- **The macro is `col!`**, not `column!` (that name belongs to the standard library).
- **Dynamic strings are `String`s**: `text(format!("{count} items"))`. A borrowed `&str` from state does not compile; literals do.
- **Icons are not in the font.** Use `Icon::vector`. Only a small subset of SVG is accepted (`path`, `circle`, `rect`, `line`; one colour; no groups, transforms or opacity) and an icon outside it draws nothing: put the icon set in one file and test every icon with `VectorIcon::check()`.
- **A font family that was not added falls back silently.** Check with `setup.has_font_family("Inter")`.
- **Arabic and other right-to-left text**: shaped and ordered correctly, but give it `.fill_width()` so it has a box to align in. Layout itself is never mirrored.
- **Gradients look slightly lighter in the middle** than in a design tool (they blend in linear light); shadows and colours match.
- **Not available yet**: text selection, copy and paste; animations other than scroll fling, horizontal scrolling, ellipsis, wrapping rows (`flex-wrap`), per-corner radii, per-side borders (use a `Divider`), a tap target inside a sentence.

## From HTML/CSS or a design tool's inspector

Learned reproducing a real twelve-screen design (`examples/fundidesk`, [ADR 0015](architecture/0015-fonts-icons-and-design-fidelity.md)). The left column is what the design says; all numbers go across unchanged as dp.

| Design property | Morningstar |
| --- | --- |
| `display: flex` (row) / `flex-direction: column` | `row![..]` / `col![..]` |
| `gap: 12px` | `.gap(12.0)` |
| `padding: 16px 20px 24px` (top, sides, bottom) | `.padding_ltrb(20.0, 16.0, 20.0, 24.0)` (note the order: left, top, right, bottom) |
| `margin-top: 28px` on a child of a column | `Spacer::fixed(28.0)` before it, or fold it into the parent's padding or gap |
| `margin-top: auto` | `Spacer::new()` before it; on a tall screen, make that block the fixed footer under a `ScrollView` |
| `align-items: center` / `flex-end` | `.cross_align(CrossAlign::Center)` / `End` |
| `justify-content: space-between` / `center` | `.space_between()` / `.main_align(MainAlign::Center)`; both axes: `.center()` |
| `flex: 1` | `.fill_width()` in a row, `.fill_height()` in a column |
| `width: 100%` | `.fill_width()` |
| `width: 70%` | no percentages: compute it from the parent's fixed width |
| `display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px` | `row![a, b].gap(8.0)` with each child `.fill_width()`; several rows in a `col!` with the same gap |
| `flex-wrap: wrap` | not available: write the rows out |
| `background: #1E8416` | `.background(hex(0x1E8416))`, from a named constant |
| `border-radius: 14px` / `999px` / `50%` | `.radius(14.0)` / `.radius(999.0)` / `.radius(side / 2.0)` |
| `border: 1px solid #D9E0DB` | `.border(1.0, c)` |
| `border: 1px dashed #AEB9B1` | `.border_dashed(1.0, c)` |
| `border-top: 1px solid ..` | a `Divider::new().color(c)` as the first child of a column |
| `box-shadow: 0 12px 28px rgba(20, 32, 26, 0.14)` | `.shadow(Shadow::new(0.0, 12.0, 28.0, with_alpha(hex(0x14201A), 0.14)))` |
| `position: absolute; left/top/..` over other content | `stack![below, above.place(Alignment::TOP_LEFT).offset(x, y)]`; decoration that must not size the stack goes in a `.fill()` layer with padding |
| `font-family`, `font-weight: 600` | `.family("Inter")` or the theme's `Fonts`; `.semibold()` |
| `font-size: 15px; line-height: 22px` | `.size(15.0).line_height_dp(22.0)` |
| no `line-height` | the browser's default for that font: see the pitfall below |
| `letter-spacing: -0.4px` | `.letter_spacing(-0.4)` |
| `text-transform: uppercase` | `.uppercase()` |
| `text-align: center` | `.centered().fill_width()` |
| `color: ..` on a parent | pass the colour to each text and icon; nothing is inherited |
| `<span style="font-weight: 600">` inside a paragraph | `Text::rich([span(".."), span("..").semibold()])` |
| `<svg viewBox=.. stroke="currentColor" ..>` | a `VectorIcon::svg(..)` constant; `Icon::vector(X).size(w).color(c)`; another `stroke-width` on the same drawing: `.stroke_width(2.4)` |
| an `<svg>` with two colours | one colour per icon: draw the plate as a box and the mark as an icon over it |
| `<input>`, `<textarea>` | a `Container` around a `TextInput` (see "A form") |
| `<a href>`, `<button>` | `.on_press(Message::..)` on the box |

### Pitfalls of translating CSS

Each of these cost a pixel or more per element until it was understood.

- **Padding starts inside the border in CSS; here the border is drawn over the box's edge and takes no room.** `border: 1px; padding: 16px` is `.border(1.0, c).padding(17.0)`. Centred content is unaffected.
- **Check `box-sizing`.** With `box-sizing: border-box`, `height: 52px` is the outer height. Without it (the default for `div` and `a`; `button` is border-box), the border and padding are added: `height: 48px; border: 1.5px solid` is 51 tall.
- **Text with no `line-height` is not `1.2 x size`.** A browser takes the font's ascent and descent, rounds each to whole *device* pixels and adds them. For a reference rendered at 2x, 13 px Inter is 15.5 dp tall and 15 px is 18. Put the font's metrics in the theme and compute it there (`examples/fundidesk/src/theme.rs`, `normal_line_height`); every text below a wrong one is displaced.
- **A bare `<input>` has 2 px of horizontal padding of its own**, so its text starts 2 px after the gap.
- **An inline `<span>` with padding** (a pill) is as tall as its line plus the padding: 12 px text with `padding: 6px 10px` is 14.5 + 12 dp.
- **Colour is inherited in CSS and not here.** An icon with `stroke="currentColor"` takes the nearest `color:`; find it and pass it.
- **`h1`..`p` in exported markup usually carry `margin: 0`.** If one does not, the browser's default margins are part of the design.
- **Anti-aliased edges are blended differently** (ADR 0015): text and thin strokes come out lighter than in a browser's rendering. It shows in the diff as many small regions over text and icons with the right extent. Do not chase it by changing weights or colours.

### The order that worked

1. Survey every screen before building one: collect the colours, type styles, radii and icons into a theme, an icon file and shared components. Count how often each value occurs; the frequent ones are tokens.
2. Build a screen top-down from the markup, one function per block, quoting the CSS in a comment.
3. Render at the reference's exact size and compare. Before reading regions, compare the *extent of ink* row by row between the two images: a first difference of one pixel that grows down the page is a height (line height, border, padding), not a rendering difference.
4. Fix heights first, then horizontal insets, then look at text and icons.
5. Stop when every remaining region is text, an icon or a curved edge with the right extent. Record the number and hold it in a test.
