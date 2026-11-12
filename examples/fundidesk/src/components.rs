//! FundiDesk's shared pieces: everything that appears on more than one
//! screen, built once from the tokens in [`theme`](crate::theme).
//!
//! Each function returns a [`Container`] (or something that converts into
//! an [`Element`]), so the usual modifiers still apply at the call site:
//! `.on_press(..)`, `.fill_width()`, `.id(..)`, `.height(..)`. The doc
//! comment of each piece quotes the CSS it reproduces, so a screen author
//! can match a block of the design's HTML to a call.
//!
//! A screen that needs a one-off variation builds it in its own file from
//! `Container`, `row!`, `col!` and the tokens.

use std::borrow::Cow;

use morningstar::ui::prelude::*;

use crate::icons;
use crate::theme::{
    AMBER, AMBER_TINT, BLUE, BLUE_TINT, CANVAS, CHARCOAL, DASH, DOT, GREEN, GREEN_DARK, GREEN_TINT,
    HAIRLINE, INK, MUTED, NEUTRAL_TINT, OUTLINE, PLACEHOLDER, RED, RED_TINT, WHITE, grotesk, inter,
    label, meta, overline, radius, space,
};

/// Text a component shows: a literal or a formatted `String`.
pub type Label = Cow<'static, str>;

// ------------------------------------------------------------------- logo

/// The FundiDesk logo: a rounded plate `side` dp square with the phone
/// mark on it. Green plate and white mark on light screens, the reverse on
/// the splash screen (which also passes a stroke of 5 instead of 6 with
/// [`logo_with_stroke`]).
pub fn logo<M>(side: f32, plate: Color, mark: Color) -> Container<M> {
    logo_with_stroke(side, plate, mark, 6.0)
}

/// [`logo`] with the mark's stroke width given, in the mark's 88-unit box.
pub fn logo_with_stroke<M>(side: f32, plate: Color, mark: Color, stroke: f32) -> Container<M> {
    stack![
        Container::<M>::empty()
            .size(side, side)
            .radius(side * 24.0 / 88.0)
            .background(plate),
        Icon::vector(icons::LOGO_MARK)
            .size(side)
            .color(mark)
            .stroke_width(stroke),
    ]
}

// ---------------------------------------------------------------- buttons

/// A button: a label, optionally with an icon before it, centred in a
/// rounded box. Start from [`Action::primary`], [`Action::outline`] or
/// [`Action::quiet`] and adjust what the design changes.
///
/// ```ignore
/// Action::primary("Log in").on_press(Message::LogIn)
/// Action::outline("Share on WhatsApp").compact().icon(icons::CHAT, 18.0)
/// Action::quiet("Call").icon(icons::CALL, 18.0)
/// ```
///
/// It fills the width it is given unless [`hug`](Self::hug) is called, so
/// two of them in a `row!` share the row equally (the design's two-column
/// grids of buttons).
pub struct Action<M> {
    label: Label,
    height: f32,
    radius: f32,
    fill: Color,
    border: Option<(f32, Color)>,
    text_size: f32,
    color: Color,
    icon: Option<(VectorIcon, f32, Option<f32>)>,
    hug: Option<f32>,
    on_press: Option<M>,
}

impl<M> Action<M> {
    fn new(label: impl Into<Label>) -> Self {
        Self {
            label: label.into(),
            height: space::BUTTON_HEIGHT,
            radius: radius::BUTTON,
            fill: GREEN,
            border: None,
            text_size: 16.0,
            color: WHITE,
            icon: None,
            hug: None,
            on_press: None,
        }
    }

    /// `height: 54px; border-radius: 14px; background: #1E8416; color:
    /// #FFFFFF; font-size: 16px; font-weight: 600`.
    pub fn primary(label: impl Into<Label>) -> Self {
        Self::new(label)
    }

    /// `height: 54px; border-radius: 14px; border: 1.5px solid #1E8416;
    /// background: #FFFFFF; color: #176611; font-size: 15px; font-weight:
    /// 600`.
    pub fn outline(label: impl Into<Label>) -> Self {
        Self {
            fill: WHITE,
            border: Some((1.5, GREEN)),
            text_size: 15.0,
            color: GREEN_DARK,
            ..Self::new(label)
        }
    }

    /// `height: 44px; border: 1px solid #D9E0DB; border-radius: 12px;
    /// background: #FFFFFF; color: #14201A; font: 600 14px Inter`.
    pub fn quiet(label: impl Into<Label>) -> Self {
        Self {
            height: space::TOUCH,
            radius: radius::FIELD,
            fill: WHITE,
            border: Some((1.0, OUTLINE)),
            text_size: 14.0,
            color: INK,
            ..Self::new(label)
        }
    }

    /// The smaller size used inside cards: `height: 48px; border-radius:
    /// 12px; font-size: 14px`.
    pub fn compact(mut self) -> Self {
        self.height = 48.0;
        self.radius = radius::FIELD;
        self.text_size = 14.0;
        self
    }

    /// Height in dp.
    pub fn height(mut self, dp: f32) -> Self {
        self.height = dp;
        self
    }

    /// Corner radius in dp.
    pub fn radius(mut self, dp: f32) -> Self {
        self.radius = dp;
        self
    }

    /// Label size in dp.
    pub fn text_size(mut self, dp: f32) -> Self {
        self.text_size = dp;
        self
    }

    /// Background and label colours.
    pub fn colors(mut self, fill: Color, label: Color) -> Self {
        self.fill = fill;
        self.color = label;
        self
    }

    /// An icon `size` dp square before the label, 8 dp from it, in the
    /// label's colour.
    pub fn icon(mut self, icon: VectorIcon, size: f32) -> Self {
        self.icon = Some((icon, size, None));
        self
    }

    /// Like [`icon`](Self::icon) with the icon's stroke width given.
    pub fn icon_with_stroke(mut self, icon: VectorIcon, size: f32, stroke: f32) -> Self {
        self.icon = Some((icon, size, Some(stroke)));
        self
    }

    /// As wide as its label plus `padding` dp on each side, instead of
    /// filling (`padding: 0 16px`).
    pub fn hug(mut self, padding: f32) -> Self {
        self.hug = Some(padding);
        self
    }

    /// The message a tap delivers.
    pub fn on_press(mut self, message: M) -> Self {
        self.on_press = Some(message);
        self
    }

    /// The button as a box, for further modifiers.
    pub fn build(self) -> Container<M> {
        let mut content = Row::new().gap(8.0).center();
        if let Some((icon, size, stroke)) = self.icon {
            let mut icon = Icon::vector(icon).size(size).color(self.color);
            if let Some(stroke) = stroke {
                icon = icon.stroke_width(stroke);
            }
            content = content.push(icon);
        }
        let mut button = content
            .push(
                inter(self.label, self.text_size)
                    .semibold()
                    .color(self.color),
            )
            .height(self.height)
            .radius(self.radius)
            .background(self.fill)
            .on_press_maybe(self.on_press);
        if let Some((width, color)) = self.border {
            button = button.border(width, color);
        }
        let border = self.border.map_or(0.0, |(width, _)| width);
        match self.hug {
            // In CSS the padding starts inside the border.
            Some(padding) => button.padding_xy(padding + border, 0.0),
            None => button.fill_width(),
        }
    }
}

impl<M> From<Action<M>> for Element<M> {
    fn from(action: Action<M>) -> Self {
        action.build().into()
    }
}

/// A bare icon in a 44 dp touch target: back, share, show password.
/// `width: 44px; height: 44px; display: flex; align-items: center;
/// justify-content: center`.
pub fn icon_button<M>(icon: VectorIcon, size: f32, color: Color) -> Container<M> {
    Container::new(Icon::vector(icon).size(size).color(color))
        .size(space::TOUCH, space::TOUCH)
        .center()
        .radius(space::TOUCH / 2.0)
}

/// A text link: `font-size: 14px; font-weight: 600; color: #1E8416` with
/// `padding_y` dp above and below to make it tappable (`padding: 6px 0`).
pub fn link<M>(text: impl Into<Label>, padding_y: f32) -> Container<M> {
    Container::new(inter(text, 14.0).semibold().color(GREEN)).padding_xy(0.0, padding_y)
}

// ----------------------------------------------------------------- fields

/// A text field: the design's frame around a real [`TextInput`].
///
/// `height: 52px; border: 1px solid #D9E0DB; border-radius: 12px; padding:
/// 0 16px; font: 400 15px Inter`, with the text in `#14201A` for a value and
/// `#6B756F` for a placeholder. With focus the border becomes `1.5px solid
/// #1E8416` (the design's focused field) and the caret is green; the size
/// of the field does not change.
///
/// The screen owns the text: pass the value in, store what `on_input`
/// delivers, and give the field an id (it is the input's identity, and what
/// a test taps).
///
/// ```ignore
/// Field::new(&state.phone)
///     .hint("07XX XXX XXX")
///     .leading(icons::PHONE)
///     .input(|input| input.keyboard(KeyboardType::Phone).action(ImeAction::Next))
///     .on_input(Message::Phone)
///     .id("login-phone")
/// Field::new(&state.problem)
///     .hint("What does the customer say is wrong?")
///     .multiline(88.0)
///     .on_input(Message::Problem)
///     .id("repair-problem")
/// ```
pub struct Field<M> {
    input: TextInput<M>,
    leading: Option<(VectorIcon, f32, Option<f32>)>,
    prefix: Option<Label>,
    trailing: Option<Element<M>>,
    height: f32,
    padding_x: f32,
    gap: f32,
    multiline: bool,
    border: (f32, Color),
    radius: f32,
}

impl<M> Field<M> {
    /// A field holding `value`, in Inter 15.
    pub fn new(value: &str) -> Self {
        Self {
            input: TextInput::new(value)
                .style(inter("", 15.0))
                .placeholder_color(PLACEHOLDER)
                .caret_color(GREEN),
            leading: None,
            prefix: None,
            trailing: None,
            height: space::FIELD_HEIGHT,
            padding_x: 16.0,
            gap: 10.0,
            multiline: false,
            border: (1.0, OUTLINE),
            radius: radius::FIELD,
        }
    }

    /// What the field shows, in `#6B756F`, while it is empty.
    pub fn hint(mut self, text: impl Into<Label>) -> Self {
        self.input = self.input.placeholder(text);
        self
    }

    /// The message for each new value: `.on_input(Message::Phone)`.
    pub fn on_input(mut self, message: impl Fn(String) -> M + Send + Sync + 'static) -> Self {
        self.input = self.input.on_input(message);
        self
    }

    /// The input's identity: `.id("login-phone")`.
    pub fn id(mut self, id: impl std::hash::Hash) -> Self {
        self.input = self.input.id(id);
        self
    }

    /// Anything else of [`TextInput`]: the keyboard, the action key,
    /// masking, limits.
    /// `.input(|input| input.keyboard(KeyboardType::Phone).secure(true))`.
    pub fn input(mut self, configure: impl FnOnce(TextInput<M>) -> TextInput<M>) -> Self {
        self.input = configure(self.input);
        self
    }

    /// An unusual text style, such as the search field's Inter 14: only
    /// the style of `text` is used.
    pub fn text(mut self, text: Text) -> Self {
        self.input = self.input.style(text);
        self
    }

    /// An 18 dp icon before the text, in the muted colour.
    pub fn leading(mut self, icon: VectorIcon) -> Self {
        self.leading = Some((icon, 18.0, None));
        self
    }

    /// Like [`leading`](Self::leading) with the icon's stroke width given
    /// (the search field's magnifier is 2).
    pub fn leading_with_stroke(mut self, icon: VectorIcon, stroke: f32) -> Self {
        self.leading = Some((icon, 18.0, Some(stroke)));
        self
    }

    /// Fixed text before the value, followed by a vertical hairline: the
    /// `+255` of a phone field (`gap: 12px`), or `TZS`.
    pub fn prefix(mut self, text: impl Into<Label>) -> Self {
        self.prefix = Some(text.into());
        self.gap = 12.0;
        self
    }

    /// Something after the text, such as an [`icon_button`]. The right
    /// padding shrinks to 6 dp to make room for a 44 dp target (`padding:
    /// 0 6px 0 16px`).
    pub fn trailing(mut self, element: impl Into<Element<M>>) -> Self {
        self.trailing = Some(element.into());
        self
    }

    /// Height in dp (52 by default; the search field is 48, the amount
    /// field 64).
    pub fn height(mut self, dp: f32) -> Self {
        self.height = dp;
        self
    }

    /// Left and right padding in dp (16 by default; the search field 14).
    pub fn padding_x(mut self, dp: f32) -> Self {
        self.padding_x = dp;
        self
    }

    /// Border width and colour while the field is not focused.
    pub fn border(mut self, width: f32, color: Color) -> Self {
        self.border = (width, color);
        self
    }

    /// Corner radius in dp.
    pub fn radius(mut self, dp: f32) -> Self {
        self.radius = dp;
        self
    }

    /// A text area at least `height` dp tall: the text starts at the top
    /// and wraps (`padding: 14px 16px; font: 400 15px/22px Inter`), Return
    /// breaks the line, and the field grows when the text needs more room.
    pub fn multiline(mut self, height: f32) -> Self {
        self.height = height;
        self.multiline = true;
        self.input = self.input.multiline().line_height_dp(22.0);
        self
    }

    /// The field as a box, for further modifiers.
    pub fn build(self) -> Container<M> {
        // In CSS a box's padding starts inside its border; here a border is
        // drawn over the box's edge, so the border's width is added.
        let inset = self.padding_x + self.border.0;
        // A bare `<input>` beside an icon or prefix keeps the browser's
        // own 2 px of padding before its text.
        let input_padding = if self.leading.is_some() || self.prefix.is_some() {
            2.0
        } else {
            0.0
        };
        let mut row = Row::new().gap(self.gap).fill_width();
        row = if self.multiline {
            row.min_height(self.height)
        } else {
            row.height(self.height)
        };
        if let Some((icon, size, stroke)) = self.leading {
            let mut icon = Icon::vector(icon).size(size).color(MUTED);
            if let Some(stroke) = stroke {
                icon = icon.stroke_width(stroke);
            }
            row = row.push(icon);
        }
        if let Some(prefix) = self.prefix {
            row = row
                .push(inter(prefix, 15.0).medium())
                .push(Container::<M>::empty().size(1.0, 24.0).background(OUTLINE));
        }
        row = row.push(
            Row::new()
                .fill_width()
                .padding_ltrb(input_padding, 0.0, 0.0, 0.0)
                .push(self.input.fill_width()),
        );
        let right = if self.trailing.is_some() {
            6.0 + self.border.0
        } else {
            inset
        };
        row = row.push_maybe(self.trailing);
        let row = if self.multiline {
            row.padding_ltrb(inset, 14.0 + self.border.0, right, 14.0)
        } else {
            row.cross_align(CrossAlign::Center)
                .padding_ltrb(inset, 0.0, right, 0.0)
        };
        // The focused border is wider, but a border is drawn over the box's
        // edge: nothing inside moves when the field takes focus.
        row.radius(self.radius)
            .background(WHITE)
            .border(self.border.0, self.border.1)
            .focused_border(1.5, GREEN)
    }
}

impl<M> From<Field<M>> for Element<M> {
    fn from(field: Field<M>) -> Self {
        field.build().into()
    }
}

/// A label above a field (or above anything): `display: flex;
/// flex-direction: column; gap: 8px` with the label in Inter 13 medium.
pub fn labelled<M>(text: impl Into<Label>, field: impl Into<Element<M>>) -> Container<M> {
    col![label(text), field].gap(space::LABEL_GAP).fill_width()
}

// ------------------------------------------------------------------ pills

/// The colour pairs of status pills, icon tiles and banners.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// Grey: "Received".
    Neutral,
    /// Blue: "In repair", "Diagnosing", "Waiting for parts".
    Info,
    /// Green: "Ready for collection", "Approved".
    Success,
    /// Amber: "Awaiting approval".
    Warning,
    /// Red: delays.
    Danger,
}

impl Tone {
    /// The text and icon colour.
    pub const fn foreground(self) -> Color {
        match self {
            Self::Neutral => CHARCOAL,
            Self::Info => BLUE,
            Self::Success => GREEN_DARK,
            Self::Warning => AMBER,
            Self::Danger => RED,
        }
    }

    /// The tint behind it.
    pub const fn background(self) -> Color {
        match self {
            Self::Neutral => NEUTRAL_TINT,
            Self::Info => BLUE_TINT,
            Self::Success => GREEN_TINT,
            Self::Warning => AMBER_TINT,
            Self::Danger => RED_TINT,
        }
    }
}

/// A status pill: `font-size: 12px; font-weight: 600; border-radius: 999px;
/// padding: 6px 10px` in a tone's colours. Chain `.padding_xy(10.0, 5.0)`
/// for the tighter one in card headers.
pub fn pill<M>(text: impl Into<Label>, tone: Tone) -> Container<M> {
    Container::new(inter(text, 12.0).semibold().color(tone.foreground()))
        .padding_xy(10.0, 6.0)
        .radius(radius::PILL)
        .background(tone.background())
}

/// A pill with a 14 dp icon before its text: the dashboard's "Synced"
/// (`gap: 6px; padding: 8px 12px`).
pub fn icon_pill<M>(icon: VectorIcon, text: impl Into<Label>, tone: Tone) -> Container<M> {
    row![
        Icon::vector(icon).size(14.0).color(tone.foreground()),
        inter(text, 12.0).semibold().color(tone.foreground()),
    ]
    .gap(6.0)
    .cross_align(CrossAlign::Center)
    .padding_xy(12.0, 8.0)
    .radius(radius::PILL)
    .background(tone.background())
}

// ------------------------------------------------------- chips and choices

/// The box and text of something selectable: selected is `border: 1.5px
/// solid #1E8416; background: #E9F6E7; color: #176611; font-weight: 600`,
/// unselected `border: 1px solid #D9E0DB; background: #FFFFFF; color:
/// #14201A; font-weight: 500`.
fn selectable<M>(text: Label, size: f32, selected: bool) -> Container<M> {
    let text = inter(text, size);
    let frame = if selected {
        Container::new(text.semibold().color(GREEN_DARK))
            .background(GREEN_TINT)
            .border(1.5, GREEN)
    } else {
        Container::new(text.medium())
            .background(WHITE)
            .border(1.0, OUTLINE)
    };
    frame.center()
}

/// A chip that hugs its label: `height: 44px; padding: 0 14px;
/// border-radius: 999px; font-size: 13px`. Conditions and accessories on
/// the new-repair screen, in a row that wraps
/// (`Row::new().gap(8.0).line_gap(8.0).wrap()`).
pub fn chip<M>(text: impl Into<Label>, selected: bool) -> Container<M> {
    // The padding starts inside the border: a selected chip is 1 dp wider.
    let border = if selected { 1.5 } else { 1.0 };
    selectable(text.into(), 13.0, selected)
        .height(space::TOUCH)
        .padding_xy(14.0 + border, 0.0)
        .radius(radius::PILL)
}

/// A chip that fills its share of a row: the quick amounts of the payment
/// screen (`height: 44px; border-radius: 999px; font-size: 13px` in a
/// three-column grid).
pub fn wide_chip<M>(text: impl Into<Label>, selected: bool) -> Container<M> {
    selectable(text.into(), 13.0, selected)
        .height(space::TOUCH)
        .fill_width()
        .radius(radius::PILL)
}

/// A filter tab that fills its share of a row: `height: 44px;
/// border-radius: 999px; font-size: 13px`. Active is solid green with white
/// semibold text and no border.
pub fn filter_chip<M>(text: impl Into<Label>, active: bool) -> Container<M> {
    let frame = if active {
        Container::new(inter(text, 13.0).semibold().color(WHITE))
            .background(GREEN)
            .center()
    } else {
        selectable(text.into(), 13.0, false)
    };
    frame.height(space::TOUCH).fill_width().radius(radius::PILL)
}

/// One option of a segmented choice: `height: 48px; border-radius: 12px;
/// font-size: 14px`, filling its share of the row. Chain `.height(52.0)`
/// for the payment methods. For a tick before the label, build the row by
/// hand (see `choice_with_check`).
pub fn choice<M>(text: impl Into<Label>, selected: bool) -> Container<M> {
    selectable(text.into(), 14.0, selected)
        .height(48.0)
        .fill_width()
        .radius(radius::FIELD)
}

/// A selected [`choice`] with a 16 dp tick before its label (`gap: 8px`):
/// the chosen payment method.
pub fn choice_with_check<M>(text: impl Into<Label>) -> Container<M> {
    row![
        Icon::vector(icons::CHECK)
            .size(16.0)
            .color(GREEN_DARK)
            .stroke_width(3.0),
        inter(text, 14.0).semibold().color(GREEN_DARK),
    ]
    .gap(8.0)
    .center()
    .height(48.0)
    .fill_width()
    .radius(radius::FIELD)
    .background(GREEN_TINT)
    .border(1.5, GREEN)
}

/// One box of a verification code: `height: 56px; border-radius: 12px;
/// font: 600 22px 'Space Grotesk'`, filling its share of the row, around a
/// one-digit [`TextInput`] that accepts digits only, moves on to the next
/// box when it is filled and back to the previous one on backspace.
///
/// `state` is how the design draws the box and comes from the code entered
/// so far, not from where the keyboard's focus is: the box waiting for the
/// next digit is `Active` whether or not it has been tapped. `id` is the
/// input's identity.
pub fn code_box<M>(
    digit: &str,
    state: CodeBox,
    id: impl std::hash::Hash,
    on_input: impl Fn(String) -> M + Send + Sync + 'static,
) -> Container<M> {
    let (fill, border) = match state {
        CodeBox::Filled => (WHITE, (1.5, GREEN)),
        CodeBox::Active => (GREEN_TINT, (2.0, GREEN)),
        CodeBox::Empty => (WHITE, (1.0, OUTLINE)),
    };
    Container::new(
        TextInput::new(digit)
            .style(grotesk("", 22.0).semibold())
            .align(TextAlign::Center)
            .caret_color(GREEN)
            .keyboard(KeyboardType::Number)
            .digits_only()
            .max_length(1)
            .auto_advance()
            .on_input(on_input)
            .id(id),
    )
    .input_frame()
    .center()
    .height(56.0)
    .fill_width()
    .radius(radius::FIELD)
    .background(fill)
    .border(border.0, border.1)
}

/// The state of a [`code_box`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodeBox {
    /// Holds a digit: `border: 1.5px solid #1E8416`.
    Filled,
    /// Waiting for the next digit: `border: 2px solid #1E8416; background:
    /// #E9F6E7`.
    Active,
    /// Not reached: `border: 1px solid #D9E0DB`.
    Empty,
}

/// An optional extra with a dashed outline: `height: 64px; border: 1px
/// dashed #AEB9B1; border-radius: 12px; font: 500 12px Inter`, a 20 dp
/// green icon above the label (`gap: 6px`), filling its share of the row.
pub fn dashed_tile<M>(icon: VectorIcon, text: impl Into<Label>) -> Container<M> {
    col![
        Icon::vector(icon).size(20.0).color(GREEN).stroke_width(1.8),
        inter(text, 12.0).medium(),
    ]
    .gap(6.0)
    .center()
    .height(64.0)
    .fill_width()
    .radius(radius::FIELD)
    .background(WHITE)
    .border_dashed(1.0, DASH)
}

// ------------------------------------------------------------------ grids

/// The design's `display: grid; grid-template-columns: repeat(n, minmax(0,
/// 1fr)); gap: g`: rows of `columns` equal cells. Each item must fill its
/// width (the pieces in this file meant for grids do). A last row that is
/// not full is padded with empty cells, so its items keep the column width.
pub fn grid<M>(columns: usize, gap: f32, items: Vec<Element<M>>) -> Container<M> {
    let columns = columns.max(1);
    let mut rows = Column::new().gap(gap).fill_width();
    let mut items = items.into_iter().peekable();
    while items.peek().is_some() {
        let mut row = Row::new().gap(gap).fill_width();
        for _ in 0..columns {
            row = match items.next() {
                Some(item) => row.push(item),
                None => row.push(Container::<M>::empty().fill_width()),
            };
        }
        rows = rows.push(row);
    }
    rows
}

// ------------------------------------------------------------------ cards

/// A white card: `border-radius: 16px; background: #FFFFFF; border: 1px
/// solid #E3E8E4; padding: 16px; display: flex; flex-direction: column`.
/// Push its children and set its `.gap(..)`. (The padding here is 17: in
/// CSS it starts inside the 1 px border.)
pub fn card<M>() -> Container<M> {
    Column::new()
        .fill_width()
        .padding(space::CARD + 1.0)
        .radius(radius::CARD)
        .background(WHITE)
        .border(1.0, HAIRLINE)
}

/// A 1 dp line in the hairline colour: `height: 1px; background: #E3E8E4`,
/// and the `border-top` / `border-bottom` of bars.
pub fn hairline() -> Divider {
    Divider::new().color(HAIRLINE)
}

/// A rounded square tile with an icon half its size: `width: 40px; height:
/// 40px; border-radius: 12px` with a 20 dp icon, in a tone's colours.
pub fn icon_tile(icon: VectorIcon, tone: Tone) -> Icon {
    Icon::vector(icon)
        .size(40.0)
        .radius(radius::FIELD)
        .background(tone.background())
        .color(tone.foreground())
}

/// A small tile with a caption and a figure: `border-radius: 14px; padding:
/// 12px; display: flex; flex-direction: column; gap: 4px`, filling its
/// share of a row. The caller styles the two texts (their colours differ
/// per tile).
pub fn stat_tile<M>(caption: Text, figure: Text, background: Color) -> Container<M> {
    col![caption, figure]
        .gap(4.0)
        .padding(12.0)
        .fill_width()
        .radius(radius::BUTTON)
        .background(background)
}

/// A tinted strip inside a card: `border-radius: 12px; padding: 12px 14px;
/// display: flex; align-items: center`. Push its children; chain
/// `.space_between()` or `.gap(10.0)`.
pub fn banner<M>(background: Color) -> Container<M> {
    Row::new()
        .cross_align(CrossAlign::Center)
        .fill_width()
        .padding_xy(14.0, 12.0)
        .radius(radius::FIELD)
        .background(background)
}

/// A line of a cost breakdown: a muted label at the left and a value at
/// the right, Inter 14 (`display: flex; justify-content: space-between`).
pub fn detail_row<M>(name: impl Into<Label>, value: impl Into<Label>) -> Container<M> {
    row![inter(name, 14.0).color(MUTED), inter(value, 14.0)]
        .space_between()
        .fill_width()
}

/// The total line of a cost breakdown: both sides Inter 14 semibold.
pub fn total_row<M>(name: impl Into<Label>, value: impl Into<Label>) -> Container<M> {
    row![inter(name, 14.0).semibold(), inter(value, 14.0).semibold()]
        .space_between()
        .fill_width()
}

/// A fact with a fixed-width muted label: `display: flex; gap: 12px` with
/// the label `width: 92px` and the value wrapping in the rest, Inter 14/20.
pub fn fact_row<M>(name: impl Into<Label>, value: impl Into<Label>) -> Container<M> {
    row![
        inter(name, 14.0)
            .line_height_dp(20.0)
            .color(MUTED)
            .width(92.0),
        inter(value, 14.0).line_height_dp(20.0).fill_width(),
    ]
    .gap(12.0)
    .fill_width()
}

// -------------------------------------------------------------- list rows

/// The frame of a tappable list row: `border-radius: 16px; background:
/// #FFFFFF; border: 1px solid #E3E8E4`, a row with `gap: 12px` and its
/// children centred vertically.
fn list_frame<M>() -> Container<M> {
    Row::new()
        .gap(12.0)
        .cross_align(CrossAlign::Center)
        .fill_width()
        .radius(radius::CARD)
        .background(WHITE)
        .border(1.0, HAIRLINE)
}

/// A title over a line of secondary text, taking the width left in its
/// row: Inter 15 semibold over Inter 12 muted with `gap` dp between.
fn title_block<M>(title: Label, detail: Label, gap: f32) -> Container<M> {
    col![inter(title, 15.0).semibold(), meta(detail)]
        .gap(gap)
        .fill_width()
}

/// A dashboard "needs attention" row: `height: 64px; padding: 0 14px`, an
/// [`icon_tile`], a title and detail (`gap: 2px`), and a count in Space
/// Grotesk 20 bold (red when `urgent`).
pub fn attention_row<M>(
    icon: VectorIcon,
    tone: Tone,
    title: impl Into<Label>,
    detail: impl Into<Label>,
    count: impl Into<Label>,
    urgent: bool,
) -> Container<M> {
    let count = grotesk(count, 20.0).color(if urgent { RED } else { INK });
    list_frame()
        .height(64.0)
        .padding_xy(15.0, 0.0)
        .push(icon_tile(icon, tone))
        .push(title_block(title.into(), detail.into(), 2.0))
        .push(count)
}

/// A repair in a short list: `padding: 14px`, the device and a detail line
/// (`gap: 4px`), and its status [`pill`].
pub fn repair_row<M>(
    device: impl Into<Label>,
    detail: impl Into<Label>,
    status: impl Into<Label>,
    tone: Tone,
) -> Container<M> {
    list_frame()
        .padding(15.0)
        .push(title_block(device.into(), detail.into(), 4.0))
        .push(pill(status, tone))
}

/// A repair in the repairs list: `padding: 14px; display: flex;
/// flex-direction: column; gap: 10px`: the device, a detail line and the
/// status pill on top, and two notes below, left and right (Inter 12; the
/// caller styles them, as their colour and weight carry meaning).
pub fn repair_card<M>(
    device: impl Into<Label>,
    detail: impl Into<Label>,
    status: impl Into<Label>,
    tone: Tone,
    left: Text,
    right: Text,
) -> Container<M> {
    col![
        row![
            title_block(device.into(), detail.into(), 4.0),
            pill(status, tone),
        ]
        .gap(10.0)
        .fill_width(),
        row![left, right]
            .space_between()
            .cross_align(CrossAlign::Center)
            .fill_width(),
    ]
    .gap(10.0)
    .padding(15.0)
    .fill_width()
    .radius(radius::CARD)
    .background(WHITE)
    .border(1.0, HAIRLINE)
}

// --------------------------------------------------------------- timeline

/// Where a step of a repair's history stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Completed: a green dot with a white tick.
    Done,
    /// In progress: a blue ring, and the title in bold blue.
    Current,
    /// Not reached: a grey ring, and the title muted.
    Pending,
}

/// The 20 dp marker of a timeline step.
fn step_dot<M>(step: Step) -> Container<M> {
    let dot = match step {
        // `background: #1E8416` with a 12 dp tick of stroke 3.2.
        Step::Done => Container::new(
            Icon::vector(icons::CHECK)
                .size(12.0)
                .color(WHITE)
                .stroke_width(3.2),
        )
        .center()
        .background(GREEN),
        // `background: #FFFFFF; border: 6px solid #1D4ED8`.
        Step::Current => Container::empty().background(WHITE).border(6.0, BLUE),
        // `background: #FFFFFF; border: 2px solid #AEB9B1`.
        Step::Pending => Container::empty().background(WHITE).border(2.0, DASH),
    };
    dot.size(20.0, 20.0).radius(10.0)
}

/// One step of a [`timeline`]: the marker, the title, and a line of detail
/// (who and when) for steps that have happened. `display: flex;
/// align-items: flex-start; gap: 12px`.
pub fn timeline_step<M>(
    step: Step,
    title: impl Into<Label>,
    detail: Option<Label>,
) -> Container<M> {
    let title = inter(title, 14.0);
    let title = match step {
        Step::Done => title.semibold(),
        Step::Current => title.bold().color(BLUE),
        // A pending step is one line, `line-height: 20px`.
        Step::Pending => title.line_height_dp(20.0).color(MUTED),
    };
    let texts = Column::new()
        .gap(2.0)
        .fill_width()
        .push(title.fill_width())
        .push_maybe(detail.map(|detail| meta(detail).fill_width()));
    row![step_dot(step), texts].gap(12.0).fill_width()
}

/// A repair's status history: steps `14px` apart with a 2 dp line behind
/// their markers (`left: 9px; top: 10px; bottom: 10px; width: 2px;
/// background: #D9E0DB`).
pub fn timeline<M>(steps: Vec<Container<M>>) -> Container<M> {
    stack![
        Container::new(
            Container::<M>::empty()
                .width(2.0)
                .fill_height()
                .background(OUTLINE)
        )
        .padding_ltrb(9.0, 10.0, 0.0, 10.0)
        .fill(),
        Column::new().gap(14.0).fill_width().extend(steps),
    ]
    .fill_width()
}

/// A segmented progress bar: equal segments `height: 8px; border-radius:
/// 4px` with `gap: 4px`, one colour each.
pub fn progress_segments<M>(colors: &[Color]) -> Container<M> {
    Row::new()
        .gap(4.0)
        .fill_width()
        .extend(colors.iter().map(|&color| {
            Container::<M>::empty()
                .height(8.0)
                .fill_width()
                .radius(radius::BAR)
                .background(color)
        }))
}

/// Page dots: the active one `28 x 6`, the others `8 x 6`, `border-radius:
/// 3px; gap: 6px`.
pub fn page_dots<M>(active: usize, count: usize) -> Container<M> {
    Row::new()
        .gap(6.0)
        .cross_align(CrossAlign::Center)
        .extend((0..count).map(|index| {
            let (width, color) = if index == active {
                (28.0, GREEN)
            } else {
                (8.0, DOT)
            };
            Container::<M>::empty()
                .size(width, 6.0)
                .radius(3.0)
                .background(color)
        }))
}

// ------------------------------------------------------------------- bars

/// The header of a detail screen: a back chevron, a title in Space Grotesk
/// 20/26 with a line under it, and optionally something at the right, over
/// a hairline. `padding: 36px 10px 12px 10px; gap: 8px; border-bottom: 1px
/// solid #E3E8E4`.
///
/// ```ignore
/// TopBar::new("FD-2026-00142", meta("Received 9 Nov · Due 12 Nov"))
///     .back(Message::Back)
///     .trailing(icon_button(icons::SHARE, 22.0, INK))
/// ```
pub struct TopBar<M> {
    title: Label,
    subtitle: Text,
    back: Option<M>,
    trailing: Option<Element<M>>,
    right_padding: f32,
    background: Color,
}

impl<M> TopBar<M> {
    /// A bar with a title and the text under it (style it yourself: it is
    /// [`meta`] on most screens and Space Grotesk on the new-repair screen).
    pub fn new(title: impl Into<Label>, subtitle: Text) -> Self {
        Self {
            title: title.into(),
            subtitle,
            back: None,
            trailing: None,
            right_padding: 20.0,
            background: WHITE,
        }
    }

    /// The message the back chevron delivers.
    pub fn back(mut self, message: M) -> Self {
        self.back = Some(message);
        self
    }

    /// A 44 dp control at the right; the right padding becomes 10 dp
    /// (`padding: 36px 10px 12px 10px`).
    pub fn trailing(mut self, element: impl Into<Element<M>>) -> Self {
        self.trailing = Some(element.into());
        self.right_padding = 10.0;
        self
    }

    /// Text at the right, keeping the 20 dp right padding (`padding: 36px
    /// 20px 12px 10px`): the time on the new-repair screen.
    pub fn note(mut self, element: impl Into<Element<M>>) -> Self {
        self.trailing = Some(element.into());
        self
    }

    /// The bar as a box. Its top padding grows by the status bar's height
    /// on a device.
    pub fn build(self) -> Container<M> {
        let back = icon_button(icons::CHEVRON_LEFT, 24.0, INK).on_press_maybe(self.back);
        let titles = col![
            grotesk(self.title, 20.0).line_height_dp(26.0),
            self.subtitle
        ]
        .gap(2.0)
        .fill_width();
        col![
            Row::new()
                .gap(8.0)
                .cross_align(CrossAlign::Center)
                .fill_width()
                .padding_ltrb(10.0, 36.0, self.right_padding, 12.0)
                .safe_area(SafeEdges::TOP)
                .push(back)
                .push(titles)
                .push_maybe(self.trailing),
            hairline(),
        ]
        .fill_width()
        .background(self.background)
    }
}

impl<M> From<TopBar<M>> for Element<M> {
    fn from(bar: TopBar<M>) -> Self {
        bar.build().into()
    }
}

/// A bar of actions fixed to the bottom of a screen: `padding: 16px 20px
/// 24px; border-top: 1px solid #E3E8E4; background: #FFFFFF`. Its bottom
/// padding grows by the gesture bar's height on a device.
pub fn bottom_bar<M>(content: impl Into<Element<M>>) -> Container<M> {
    col![
        hairline(),
        Container::new(content)
            .fill_width()
            .padding_ltrb(20.0, 16.0, 20.0, 24.0)
            .safe_area(SafeEdges::BOTTOM),
    ]
    .fill_width()
    .background(WHITE)
}

/// The four destinations of the tab bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tab {
    /// The dashboard.
    Home,
    /// The repairs list.
    Repairs,
    /// Customers (the design links it to the repairs list).
    Customers,
    /// The account (the design links it to shop setup).
    Account,
}

impl Tab {
    /// All four, in order.
    pub const ALL: [Self; 4] = [Self::Home, Self::Repairs, Self::Customers, Self::Account];

    const fn label(self) -> &'static str {
        match self {
            Self::Home => "Home",
            Self::Repairs => "Repairs",
            Self::Customers => "Customers",
            Self::Account => "Account",
        }
    }

    /// The icon: filled for the selected tab where the design has a filled
    /// drawing (Home and Repairs).
    const fn icon(self, selected: bool) -> VectorIcon {
        match (self, selected) {
            (Self::Home, true) => icons::HOME_FILLED,
            (Self::Home, false) => icons::HOME,
            (Self::Repairs, true) => icons::WRENCH_FILLED,
            (Self::Repairs, false) => icons::WRENCH,
            (Self::Customers, _) => icons::PEOPLE,
            (Self::Account, _) => icons::PERSON,
        }
    }
}

/// The bottom tab bar: four equal columns, each `height: 52px` with a
/// 22 dp icon over an 11 dp label (`gap: 4px`), in `padding: 8px 8px 14px`
/// under a hairline. The selected tab is green with a bold label, the
/// others muted with a medium one. `message` gives the message for a tab.
pub fn tab_bar<M>(selected: Tab, message: impl Fn(Tab) -> M) -> Container<M> {
    let tabs = Tab::ALL.map(|tab| {
        let active = tab == selected;
        let color = if active { GREEN } else { MUTED };
        let label = inter(tab.label(), 11.0).color(color);
        col![
            Icon::vector(tab.icon(active)).size(22.0).color(color),
            if active { label.bold() } else { label.medium() },
        ]
        .gap(4.0)
        .center()
        .height(52.0)
        .fill_width()
        .on_press(message(tab))
        .id(("tab", tab))
    });
    col![
        hairline(),
        Row::new()
            .fill_width()
            .padding_ltrb(8.0, 8.0, 8.0, 14.0)
            .safe_area(SafeEdges::BOTTOM)
            .extend(tabs),
    ]
    .fill_width()
    .background(WHITE)
}

// ---------------------------------------------------------------- screens

/// A screen with fixed bars and a scrolling middle: `header` (if any) at
/// the top, `body` scrolling in the space left, `footer` (if any) at the
/// bottom. This is how the design's tall screens become phone screens: the
/// design draws the whole content with its last block pushed to the bottom
/// (`margin-top: auto`), and that block is the footer here.
///
/// `id` names the screen; it keeps this screen's scroll position apart
/// from every other screen's.
pub fn scaffold<M>(
    id: &'static str,
    background: Color,
    header: Option<Element<M>>,
    body: impl Into<Element<M>>,
    footer: Option<Element<M>>,
) -> Container<M> {
    // A bar keeps itself clear of the system bars; without one, the
    // screen does.
    let edges = SafeEdges::new(false, header.is_none(), false, footer.is_none());
    Column::new()
        .fill()
        .background(background)
        .safe_area(edges)
        .push_maybe(header)
        .push(ScrollView::new(body).id((id, "scroll")))
        .push_maybe(footer)
        .id(id)
}

/// A white form screen that does not scroll: `background: #FFFFFF; padding:
/// {top}px 24px 32px; display: flex; flex-direction: column`. Use
/// `Spacer::fixed(..)` for the design's `margin-top` values and
/// `Spacer::new()` for `margin-top: auto`.
pub fn form_screen<M>(id: &'static str, top: f32) -> Container<M> {
    Column::new()
        .fill()
        .background(WHITE)
        .padding_ltrb(space::FORM_MARGIN, top, space::FORM_MARGIN, 32.0)
        .safe_area(SafeEdges::ALL)
        .id(id)
}

/// The placeholder for a screen that has not been built yet: its name, a
/// note, and a button for each place the design links it to, so navigation
/// can be walked end to end.
pub fn stub_screen<M>(
    id: &'static str,
    number: &'static str,
    title: &'static str,
    links: Vec<(&'static str, M)>,
) -> Container<M> {
    let mut column = Column::new()
        .gap(12.0)
        .fill()
        .background(CANVAS)
        .padding_ltrb(space::FORM_MARGIN, 56.0, space::FORM_MARGIN, 32.0)
        .safe_area(SafeEdges::ALL)
        .push(overline(format!("Screen {number} · not built yet")))
        .push(grotesk(title, 28.0).line_height_dp(34.0).fill_width())
        .push(
            inter(
                format!("Build this screen from design/html/{number}-*.html. See SCREENS.md."),
                14.0,
            )
            .line_height_dp(21.0)
            .color(MUTED)
            .fill_width(),
        )
        .push(Spacer::fixed(12.0));
    for (text, message) in links {
        column = column.push(Action::quiet(text).on_press(message));
    }
    column.id(id)
}
