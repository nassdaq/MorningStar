//! The pieces the screens share, each written from the design's CSS (quoted
//! above it) so that a screen reads like the design's layer list.
//!
//! Sizes are the design's. Where a `div`, `a`, `span` or `img` has both a
//! size and a `border`, the border is added to the size (they are content
//! boxes), and where a `button` has, it is not (buttons are border boxes):
//! the comments say which.

use std::borrow::Cow;
use std::time::Duration;

use morningstar::ui::prelude::*;

use crate::icons::{self, icon};
use crate::model::{Face, Item, Mark, Photo};
use crate::theme::*;
use crate::{BaruaTalk, Message, Screen, Tab};

/// The design's side margin on Home and the phone call: `padding: .. 24px`.
pub const MARGIN: f32 = 24.0;
/// The side margin of a room: `padding: .. 20px`.
pub const ROOM_MARGIN: f32 = 20.0;
/// The design's top padding, under the status bar: `padding: 56px ..`.
pub const TOP: f32 = 56.0;

// ------------------------------------------------------------------ pages

/// A whole screen: the page colour edge to edge under the system bars,
/// the content inside the safe area.
pub fn page<M: 'static>(screen: Screen, content: impl Into<Element<M>>) -> Container<M> {
    Container::new(content)
        .fill()
        .background(PAGE)
        .safe_area(SafeEdges::ALL)
        .id(screen.name())
}

/// A hairline across a screen: `border-bottom: 1px solid #1F2330`.
pub fn hairline() -> Divider {
    Divider::new().color(HAIRLINE)
}

// ------------------------------------------------------------------ faces

/// A round photograph: `border-radius: 50%; object-fit: cover`.
pub fn portrait<M: 'static>(app: &BaruaTalk, photo: Photo, size: f32) -> Element<M> {
    match app.image(photo) {
        Some(image) => Picture::new(image)
            .size(size, size)
            .radius(size / 2.0)
            .into(),
        // The picture did not load: a quiet disc of its size.
        None => Container::empty()
            .size(size, size)
            .radius(size / 2.0)
            .background(LINE)
            .into(),
    }
}

/// The picture of a person or a room, `size` dp across: the photograph,
/// the group's four-part picture, or initials on a quiet disc for someone
/// the app has no picture of (the app's own, in the "+1" disc's colours).
pub fn face<M: 'static>(app: &BaruaTalk, face: &Face, size: f32) -> Element<M> {
    match face {
        Face::Photo(photo) => portrait(app, *photo, size),
        Face::Group(photos, more) => group(app, photos, *more, size),
        Face::Initials(name) => Avatar::new(name)
            .size(size)
            .background(LINE)
            .color(INK)
            .into(),
    }
}

/// The picture of a group:
///
/// ```css
/// width: 52px; height: 52px; border-radius: 18px; background: #171A24;
/// display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 2px;
/// padding: 4px; box-sizing: border-box
/// ```
///
/// with three round pictures 21 dp across and a `#2A2F3D` disc saying how
/// many more there are (`font-size: 11px`).
fn group<M: 'static>(app: &BaruaTalk, photos: &[Photo; 3], more: u32, size: f32) -> Element<M> {
    let cell = (size - 8.0 - 2.0) / 2.0;
    let rest = Container::new(inter(format!("+{more}"), 11.0))
        .center()
        .size(cell, cell)
        .radius(cell / 2.0)
        .background(LINE);
    col![
        row![
            portrait(app, photos[0], cell),
            portrait(app, photos[1], cell)
        ]
        .gap(2.0),
        row![portrait(app, photos[2], cell), rest].gap(2.0),
    ]
    .gap(2.0)
    .size(size, size)
    .padding(4.0)
    .radius(size * 18.0 / 52.0)
    .background(SURFACE)
    .into()
}

// ---------------------------------------------------------------- buttons

/// A button that is only an icon, `size` dp square, with nothing drawn
/// around it: the back arrow, a room's two header buttons, the paperclip
/// (`width: 44px; height: 44px; display: flex; align-items: center;
/// justify-content: center`).
pub fn icon_button(
    asset: &'static SvgAsset,
    size: f32,
    icon_size: f32,
    color: Color,
    label: &'static str,
    message: Message,
) -> Container<Message> {
    Container::new(icon(asset, icon_size, color))
        .center()
        .size(size, size)
        .radius(size / 2.0)
        .on_press(message)
        .label(label)
}

/// The back arrow of a pushed screen: 44 dp, a 22 dp arrow, in the main
/// text colour (`a { color: #F4F5F8 }` on the room's page).
pub fn back_button(label: &'static str) -> Container<Message> {
    back_button_in(label, INK)
}

/// The back arrow in another colour: white on the two call pages (`a {
/// color: #FFFFFF }`).
pub fn back_button_in(label: &'static str, color: Color) -> Container<Message> {
    icon_button(&icons::BACK, 44.0, 22.0, color, label, Message::Back).id("back")
}

/// A round outlined button with an icon: `width: 44px; height: 44px;
/// border-radius: 50%; border: 1px solid #2A2F3D; background: transparent`
/// (a `button`: 44 dp with its border).
pub fn outlined_icon_button(
    asset: &'static SvgAsset,
    label: &'static str,
    message: Message,
) -> Container<Message> {
    Container::new(icon(asset, 20.0, INK))
        .center()
        .size(44.0, 44.0)
        .radius(22.0)
        .border(1.0, LINE)
        .on_press(message)
        .label(label)
}

/// A control of a call: `border-radius: 50%; border: 1px solid #2A2F3D;
/// background: #171A24; color: #F4F5F8` (a `button`: `size` with its
/// border), 52 dp with a 22 dp icon on the video call and 60 with 24 on
/// the phone call. Switched on it is the app's own: the colours swapped,
/// as the design's middle button of Home is drawn.
pub fn control(asset: &'static SvgAsset, size: f32, on: bool) -> Container<Message> {
    let icon_size = if size >= 60.0 { 24.0 } else { 22.0 };
    let (ink, fill, line) = if on {
        (PAGE, INK, INK)
    } else {
        (INK, SURFACE, LINE)
    };
    Container::new(icon(asset, icon_size, ink))
        .center()
        .size(size, size)
        .radius(size / 2.0)
        .background(fill)
        .border(1.0, line)
}

/// A round button in one colour with a white icon: decline, accept, hang
/// up (`border-radius: 50%; background: #D92D4B`).
pub fn disc(
    asset: &'static SvgAsset,
    size: f32,
    icon_size: f32,
    fill: Color,
) -> Container<Message> {
    Container::new(icon(asset, icon_size, WHITE))
        .center()
        .size(size, size)
        .radius(size / 2.0)
        .background(fill)
}

/// The main button of the app's own screens, in the manner of the
/// design's send button and unread counts: the accent, a pill 52 dp tall,
/// white 15 px semibold.
pub fn primary_button(label: impl Into<Cow<'static, str>>, message: Message) -> Container<Message> {
    Container::new(inter(label, 15.0).semibold().color(WHITE))
        .center()
        .fill_width()
        .height(52.0)
        .radius(26.0)
        .background(ACCENT)
        .on_press(message)
}

/// The second button of the app's own screens, in the manner of the
/// design's outlined pills: `border: 1px solid #2A2F3D`, 52 dp tall.
pub fn outlined_button(
    label: impl Into<Cow<'static, str>>,
    message: Message,
) -> Container<Message> {
    Container::new(inter(label, 15.0).semibold().color(INK))
        .center()
        .fill_width()
        .height(52.0)
        .radius(26.0)
        .border(1.0, LINE)
        .on_press(message)
}

// ------------------------------------------------------------ small parts

/// An unread count: `min-width: 20px; height: 20px; border-radius: 10px;
/// background: {{accent}}; color: #FFFFFF; font-size: 12px; font-weight:
/// 600`.
pub fn count<M: 'static>(count: u32) -> Container<M> {
    let label = if count > 99 {
        "99+".to_owned()
    } else {
        count.to_string()
    };
    Container::new(inter(label, 12.0).semibold().color(WHITE))
        .center()
        .min_width(20.0)
        .height(20.0)
        .padding_xy(if count > 9 { 5.0 } else { 0.0 }, 0.0)
        .radius(10.0)
        .background(ACCENT)
}

/// A pill with a small icon and a line of text: `display: flex;
/// align-items: center; gap: 8px; padding: 8px 14px; border-radius:
/// 999px`, outlined (`border: 1px solid #2A2F3D`, which adds to its size)
/// or filled.
pub fn pill<M: 'static>(
    asset: Option<&'static SvgAsset>,
    label: Text,
    color: Color,
    fill: Option<Color>,
) -> Container<M> {
    let outlined = fill.is_none();
    let edge = if outlined { 1.0 } else { 0.0 };
    let mut pill = Row::new()
        .push_maybe(asset.map(|asset| icon(asset, 14.0, color)))
        .push(label.color(color))
        .gap(8.0)
        .cross_align(CrossAlign::Center)
        .padding_xy(14.0 + edge, 8.0 + edge)
        .radius(999.0);
    if let Some(fill) = fill {
        pill = pill.background(fill);
    } else {
        pill = pill.border(1.0, LINE);
    }
    pill
}

/// A line over the screen for a few seconds: something this version does
/// not do, or what just happened to a call. The app's own, in the
/// design's raised surface and outline.
pub fn toast(text: String) -> Container<Message> {
    Container::new(
        Container::new(inter(text, 13.0).line_height_dp(18.0).color(INK).centered())
            .padding_xy(16.0, 12.0)
            .radius(18.0)
            .background(SURFACE)
            .border(1.0, LINE)
            .live(Live::Polite),
    )
    .fill_width()
    .center()
    .padding_ltrb(MARGIN, 8.0, MARGIN, 0.0)
    .safe_area(SafeEdges::TOP)
    .place(Alignment::TOP_CENTER)
    .id("toast")
    .transition(
        Transition::slide(0.0, -1.0)
            .with_fade()
            .duration(Duration::from_millis(180)),
    )
}

/// How long a sheet takes to rise.
const RISE: Duration = Duration::from_millis(220);

/// The dimmed screen under a sheet: the page colour at 60% (the design's
/// own veil over a photograph), fading in. The caller says what a tap on
/// it does.
pub fn scrim<M: 'static>() -> Container<M> {
    Container::empty()
        .fill()
        .background(VEIL_60)
        .pressed_background(VEIL_60)
        .transition(Transition::fade().duration(RISE))
}

/// A sheet rising from the bottom edge: the raised surface with 24 dp top
/// corners. The app's own.
pub fn sheet(id: &'static str, content: impl Into<Element<Message>>) -> Container<Message> {
    Container::new(content)
        .fill_width()
        .padding_ltrb(MARGIN, 20.0, MARGIN, 24.0)
        .radii(24.0, 24.0, 0.0, 0.0)
        .background(SURFACE)
        .block_touches()
        .safe_area(SafeEdges::BOTTOM)
        .place(Alignment::BOTTOM_CENTER)
        .id(id)
        .transition(Transition::slide(0.0, 1.0).duration(RISE))
}

// ----------------------------------------------------------------- fields

/// The text of every field: `color: #F4F5F8; font-size: 15px`, the
/// placeholder in `#9AA1B2` (`input::placeholder`).
pub fn field_text(input: TextInput<Message>) -> TextInput<Message> {
    input
        .style(inter("", 15.0))
        .placeholder_color(MUTED)
        .caret_color(ACCENT)
        .fill_width()
}

/// A text field, as the room's message field is drawn:
///
/// ```css
/// height: 48px; border-radius: 999px; border: 1px solid #2A2F3D; background: #171A24;
/// color: #F4F5F8; font-size: 15px; padding: 0 18px; box-sizing: border-box
/// ```
pub fn field(input: TextInput<Message>) -> Container<Message> {
    Row::new()
        .push(field_text(input))
        .cross_align(CrossAlign::Center)
        .fill_width()
        .height(48.0)
        // The border is inside the box; the padding starts after it.
        .padding_xy(19.0, 0.0)
        .radius(24.0)
        .background(SURFACE)
        .border(1.0, LINE)
        .focused_border(1.5, ACCENT)
}

/// A small heading over a part of one of the app's own screens, in the
/// manner of Home's "Live rooms": `font-size: 14px; color: #9AA1B2`.
pub fn section(title: &'static str) -> Text {
    inter(title, 14.0).color(MUTED).heading()
}

/// A paragraph of explanation on one of the app's own screens: 13 px in
/// the secondary colour.
pub fn note(text: impl Into<Cow<'static, str>>) -> Text {
    inter(text, 13.0)
        .line_height_dp(19.0)
        .color(MUTED)
        .fill_width()
}

/// A line that says what is wrong with what was typed: 13 px in the
/// recording dot's red, read out when it appears.
pub fn problem(text: impl Into<Cow<'static, str>>) -> Text {
    inter(text, 13.0)
        .line_height_dp(19.0)
        .color(RECORDING)
        .fill_width()
}

// ------------------------------------------------------------- the header

/// The header of the app's own pushed screens, as a room's is drawn: the
/// back arrow, then the title in the room name's type (`font-family:
/// 'Space Grotesk'; font-size: 17px; font-weight: 600`), `padding-bottom:
/// 18px` and a hairline under it.
pub fn top_bar(title: &'static str) -> Container<Message> {
    col![
        row![back_button("Back"), grotesk(title, 17.0).heading()]
            .gap(10.0)
            .cross_align(CrossAlign::Center)
            .fill_width()
            .padding_ltrb(10.0, TOP, ROOM_MARGIN, 18.0),
        Container::new(hairline())
            .fill_width()
            .padding_xy(ROOM_MARGIN, 0.0),
    ]
    .fill_width()
}

// ------------------------------------------------------------ the tab bar

/// The bar at the bottom of Home, and of the two tabs that are the app's
/// own:
///
/// ```css
/// height: 96px; background: #0D0F15; border-top: 1px solid #1F2330; display: flex;
/// align-items: center; justify-content: space-around; padding: 0 16px 12px; box-sizing: border-box
/// ```
///
/// Four `48px` buttons with 24 dp icons (`#F4F5F8` and filled for the
/// tab that is shown, `#9AA1B2` otherwise) around a 60 dp square with 20
/// dp corners in `#F4F5F8`. The design draws only Home selected; the
/// other two tabs take its colour when they are.
pub fn tab_bar(current: Tab) -> Container<Message> {
    let tab = |asset: &'static SvgAsset, label: &'static str, on: bool, message: Message| {
        Container::new(icon(asset, 24.0, if on { INK } else { MUTED }))
            .center()
            .size(48.0, 48.0)
            .radius(24.0)
            .on_press(message)
            .label(label)
            .role(Role::Tab)
            .selected(on)
    };
    let start = Container::new(icon(&icons::PLUS_BOLD, 26.0, PAGE))
        .center()
        .size(60.0, 60.0)
        .radius(20.0)
        .background(INK)
        .on_press(Message::OpenStart)
        .label("Start a call or room")
        .id("start");
    col![
        Divider::new().color(HAIRLINE),
        row![
            tab(
                &icons::HOME,
                "Home",
                current == Tab::Home,
                Message::Show(Tab::Home)
            )
            .id("tab-home"),
            tab(
                &icons::BELL,
                "Notifications",
                current == Tab::Notifications,
                Message::Show(Tab::Notifications)
            )
            .id("tab-notifications"),
            start,
            // Not a screen of its own: Home, with its calls.
            Container::new(icon(&icons::PHONE, 24.0, MUTED))
                .center()
                .size(48.0, 48.0)
                .radius(24.0)
                .on_press(Message::ShowCalls)
                .label("Calls")
                .id("tab-calls"),
            tab(
                &icons::CONTACTS,
                "Contacts",
                current == Tab::Contacts,
                Message::Show(Tab::Contacts)
            )
            .id("tab-contacts"),
        ]
        .main_align(MainAlign::SpaceAround)
        .cross_align(CrossAlign::Center)
        .fill_width()
        .height(95.0)
        .padding_ltrb(16.0, 0.0, 16.0, 12.0),
    ]
    .fill_width()
    .background(PAGE)
    .safe_area(SafeEdges::BOTTOM)
    .id("tab-bar")
}

// ------------------------------------------------------- a row of the list

/// A row of Home's list, an `a` with `display: flex; align-items: center;
/// gap: 14px; height: 76px`: a 52 dp picture, the name (`font-size: 16px;
/// font-weight: 600`) over a line in `#9AA1B2` at 13 px (`gap: 4px`), with
/// a 14 dp icon before it for a call (`gap: 6px`), and at the right the
/// time (`font-size: 12px`) over the unread count (`gap: 6px`), or the
/// time alone 18 dp from the top (`align-self: flex-start; margin-top:
/// 18px`).
pub fn list_row(app: &BaruaTalk, index: usize, item: &Item) -> Container<Message> {
    let mark = match item.mark {
        Mark::None => None,
        Mark::Video => Some(&icons::VIDEO_SMALL),
        Mark::Phone => Some(&icons::PHONE_SMALL),
    };
    let preview = Row::new()
        .push_maybe(mark.map(|asset| icon(asset, 14.0, MUTED)))
        .push(
            inter(item.preview.clone(), 13.0)
                .color(MUTED)
                .max_lines(1)
                .fill_width(),
        )
        .gap(6.0)
        .cross_align(CrossAlign::Center)
        .fill_width();
    let when = inter(item.when.clone(), 12.0).color(MUTED);
    let right: Element<Message> = if item.unread > 0 {
        col![when, count(item.unread)]
            .gap(6.0)
            .cross_align(CrossAlign::End)
            .into()
    } else {
        Container::new(when)
            .padding_ltrb(0.0, 18.0, 0.0, 0.0)
            .align_self(CrossAlign::Start)
            .into()
    };
    let label = if item.unread > 0 {
        format!(
            "{}, {}, {}, {} unread",
            item.title, item.preview, item.when, item.unread
        )
    } else {
        format!("{}, {}, {}", item.title, item.preview, item.when)
    };
    row![
        face(app, &item.face, 52.0),
        col![
            inter(item.title.clone(), 16.0)
                .semibold()
                .max_lines(1)
                .fill_width(),
            preview,
        ]
        .gap(4.0)
        .fill_width(),
        right,
    ]
    .gap(14.0)
    .cross_align(CrossAlign::Center)
    .fill_width()
    .height(76.0)
    .on_press(Message::OpenItem(index))
    .label(label)
    .id(("item", index))
}

// ------------------------------------------------------------- the keypad

/// A dial pad: four rows of three round keys, each a 64 dp disc in the
/// raised surface with the design's outline and a digit in the timer's
/// type. The app's own (the design has only the button that opens one).
/// `last_row` are the three keys under 7, 8 and 9.
pub fn keypad(
    last_row: [(char, &'static str); 3],
    press: fn(char) -> Message,
    prefix: &'static str,
) -> Container<Message> {
    let key = move |key: char, label: &'static str| {
        // "<" is the key that deletes: the app's own backspace icon.
        let face: Element<Message> = if key == '<' {
            icon(&icons::BACKSPACE, 24.0, INK).into()
        } else {
            text(key.to_string())
                .family(GROTESK)
                .size(26.0)
                .medium()
                .line_height_dp(grotesk_line(26.0))
                .color(INK)
                .into()
        };
        Container::new(face)
            .center()
            .size(64.0, 64.0)
            .radius(32.0)
            .background(SURFACE)
            .border(1.0, LINE)
            .on_press(press(key))
            .label(label)
            .id((prefix, key))
    };
    let digit = move |digit: char| {
        let label = match digit {
            '1' => "1",
            '2' => "2",
            '3' => "3",
            '4' => "4",
            '5' => "5",
            '6' => "6",
            '7' => "7",
            '8' => "8",
            _ => "9",
        };
        key(digit, label)
    };
    let line = |keys: [Container<Message>; 3]| {
        Row::new()
            .extend(keys)
            .gap(28.0)
            .main_align(MainAlign::Center)
            .fill_width()
    };
    col![
        line(['1', '2', '3'].map(digit)),
        line(['4', '5', '6'].map(digit)),
        line(['7', '8', '9'].map(digit)),
        line(last_row.map(|(character, label)| key(character, label))),
    ]
    .gap(16.0)
    .fill_width()
}
