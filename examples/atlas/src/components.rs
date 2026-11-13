//! The pieces the screens share, each written from the design's CSS (quoted
//! above it) so that a screen reads like the design's layer list.
//!
//! Sizes are the design's. Where a `div`, `a` or `span` has both a `height`
//! and a `border`, the border is added to the height (they are content
//! boxes), and where a `button` has, it is not (buttons are border boxes):
//! the comments say which.

use std::borrow::Cow;
use std::time::Duration;

use morningstar::ui::prelude::*;

use crate::icons::{self, icon};
use crate::theme::*;
use crate::{Message, Mode, Screen, Tab};

/// The design's side margin on screens with lists: `padding: .. 20px`.
pub const MARGIN: f32 = 20.0;
/// The design's top padding, under the status bar: `padding: 52px ..`.
pub const TOP: f32 = 52.0;

// ----------------------------------------------------------------- the orb

/// The four gradients of the orb, in the design's order (the first on
/// top):
///
/// ```css
/// background:
///   radial-gradient(circle at 30% 22%, rgba(255,255,255,0.95) 0%, rgba(255,255,255,0) 24%),
///   radial-gradient(circle at 72% 86%, rgba(120,232,236,0.95) 0%, rgba(120,232,236,0) 46%),
///   radial-gradient(circle at 22% 76%, rgba(150,140,255,0.8) 0%, rgba(150,140,255,0) 46%),
///   radial-gradient(circle at 50% 35%, #FFB8DE 0%, #F06CBC 45%, #C94FCB 75%, #8E5BE8 100%);
/// ```
fn orb_gradients() -> [Radial; 4] {
    let spot = |x: f32, y: f32, color: Color, alpha: f32, end: f32| {
        Radial::circle()
            .at(x, y)
            .stop(0.0, with_alpha(color, alpha))
            .stop(end, with_alpha(color, 0.0))
    };
    let mut body = Radial::circle().at(0.5, 0.35);
    for (offset, color) in ORB_BODY {
        body = body.stop(offset, color);
    }
    [
        spot(0.30, 0.22, WHITE, 0.95, 0.24),
        spot(0.72, 0.86, ORB_CYAN, 0.95, 0.46),
        spot(0.22, 0.76, ORB_VIOLET, 0.8, 0.46),
        body,
    ]
}

/// The small orb beside the app's name and above "Welcome back":
/// `width: 24px; height: 24px; border-radius: 50%` with the four
/// gradients and nothing else.
pub fn orb_mark<M>(size: f32) -> Container<M> {
    Container::empty()
        .size(size, size)
        .radius(size / 2.0)
        .radials(orb_gradients())
}

/// The orb: the design draws it at 96, 176 and 200 dp, every length a
/// rounded fraction of its size (the design tool's own arithmetic, read
/// off the three sizes).
///
/// ```css
/// /* the glow under it */
/// left: 18px; top: 158px; width: 141px; height: 42px; border-radius: 50%;
/// background: radial-gradient(ellipse, rgba(214,92,190,0.5), rgba(214,92,190,0) 70%);
/// filter: blur(9px);
/// /* the ball */
/// width: 176px; height: 176px; border-radius: 50%; background: (the four gradients);
/// box-shadow: inset 0 -18px 35px rgba(255,255,255,0.5), inset 0 9px 21px rgba(255,255,255,0.55),
///   inset 0 0 0 1px rgba(255,255,255,0.6), 0 18px 39px rgba(190,80,190,0.3);
/// /* the highlight */
/// left: 35px; top: 18px; width: 74px; height: 39px; border-radius: 50%;
/// background: radial-gradient(ellipse, rgba(255,255,255,0.9), rgba(255,255,255,0) 70%);
/// transform: rotate(-22deg);
/// ```
///
/// The glow and the highlight are ellipses in the design (`border-radius:
/// 50%` on a box that is not square). Their gradients reach zero at 70% of
/// the farthest corner, which is 99% of the way to the ellipse's edge, so
/// the box's shape never shows and a pill is the same picture.
pub fn orb<M: 'static>(size: f32) -> Container<M> {
    let part = |fraction: f32| (size * fraction).round();
    let fade = |color: Color, alpha: f32| {
        Radial::ellipse()
            .stop(0.0, with_alpha(color, alpha))
            .stop(0.7, with_alpha(color, 0.0))
    };
    let glow = Container::empty()
        .size(part(0.8), part(0.24))
        .radius(part(0.24) / 2.0)
        .radial(fade(GLOW, 0.5))
        .blur(part(0.05))
        .place(Alignment::TOP_LEFT)
        .offset(part(0.1), part(0.9));
    let ball = Container::empty()
        .size(size, size)
        .radius(size / 2.0)
        .radials(orb_gradients())
        .shadows([
            Shadow::inset(0.0, -part(0.1), part(0.2), with_alpha(WHITE, 0.5)),
            Shadow::inset(0.0, part(0.05), part(0.12), with_alpha(WHITE, 0.55)),
            Shadow::inset(0.0, 0.0, 0.0, with_alpha(WHITE, 0.6)).with_spread(1.0),
            Shadow::new(0.0, part(0.1), part(0.22), ORB_SHADOW),
        ]);
    let highlight = Container::empty()
        .size(part(0.42), part(0.22))
        .radius(part(0.22) / 2.0)
        .radial(fade(WHITE, 0.9))
        .rotate(-22.0)
        .place(Alignment::TOP_LEFT)
        .offset(part(0.2), part(0.1));
    stack![glow, ball, highlight]
        .size(size, size)
        .exclude_semantics()
}

/// The wash behind the orb: `radial-gradient(circle at 50% Y, rgba(246,
/// 170,214,A), rgba(246,170,214,0) END)` over the page colour.
pub fn wash(y: f32, alpha: f32, end: f32) -> Radial {
    Radial::circle()
        .at(0.5, y)
        .stop(0.0, with_alpha(WASH, alpha))
        .stop(end, with_alpha(WASH, 0.0))
}

/// Onboarding's and live voice's wash: `circle at 50% 34%, 0.32 .. 48%`.
pub fn wash_hero() -> Radial {
    wash(0.34, 0.32, 0.48)
}

/// Home's wash: `circle at 50% 28%, 0.3 .. 46%`.
pub fn wash_home() -> Radial {
    wash(0.28, 0.3, 0.46)
}

/// `<span style="font-size: 18px; font-weight: 700; letter-spacing:
/// -0.01em">Atlas</span>` after the small orb, `gap: 9px`.
pub fn wordmark<M: 'static>() -> Container<M> {
    row![
        orb_mark(24.0),
        jakarta("Atlas", 18.0).bold().letter_spacing(-0.18)
    ]
    .gap(9.0)
    .cross_align(CrossAlign::Center)
    .label("Atlas")
}

// ------------------------------------------------------------------ pages

/// A whole screen: the page colour (and the wash, if the screen has one)
/// edge to edge under the system bars, the content inside the safe area.
pub fn page<M: 'static>(
    screen: Screen,
    wash: Option<Radial>,
    content: impl Into<Element<M>>,
) -> Container<M> {
    let mut page = Container::new(content).fill().background(PAGE);
    if let Some(wash) = wash {
        page = page.radial(wash);
    }
    page.safe_area(SafeEdges::ALL).id(screen.name())
}

// ---------------------------------------------------------------- buttons

/// The primary button:
/// `height: 54px; border-radius: 27px; background: linear-gradient(135deg,
/// #C2338F, #8B3FD9); color: #FFFFFF; font-size: 15px; font-weight: 600`.
pub fn primary_button(label: impl Into<Cow<'static, str>>, message: Message) -> Container<Message> {
    Container::new(jakarta(label, 15.0).semibold().color(WHITE))
        .center()
        .fill_width()
        .height(54.0)
        .radius(27.0)
        .gradient(Gradient::angled(135.0, BRAND_FROM, BRAND_TO))
        .on_press(message)
}

/// The outlined button ("Continue with Google"): an `a` with
/// `height: 54px; border-radius: 27px; background: #FFFFFF; border: 1px
/// solid rgba(29,24,48,0.16)`, so 56 dp with its border.
pub fn outlined_button(
    label: impl Into<Cow<'static, str>>,
    message: Message,
) -> Container<Message> {
    Container::new(jakarta(label, 15.0).semibold())
        .center()
        .fill_width()
        .height(56.0)
        .radius(28.0)
        .background(WHITE)
        .border(1.0, LINE_16)
        .on_press(message)
}

/// A button that is only words: `height: 46px; font-size: 14px;
/// font-weight: 600`.
pub fn text_button(label: impl Into<Cow<'static, str>>, message: Message) -> Container<Message> {
    Container::new(jakarta(label, 14.0).semibold())
        .center()
        .fill_width()
        .height(46.0)
        .radius(23.0)
        .on_press(message)
}

/// A round icon button `side` dp across with no plate: back, minimise, the
/// composer's microphone.
pub fn icon_button(
    asset: &'static SvgAsset,
    size: f32,
    side: f32,
    label: &'static str,
    message: Message,
) -> Container<Message> {
    Container::new(icon(asset, size, INK))
        .center()
        .size(side, side)
        .radius(side / 2.0)
        .on_press(message)
        .label(label)
}

/// The profile button: an `a` with `width: 44px; height: 44px;
/// border-radius: 50%; background: #FFFFFF; border: 1px solid
/// rgba(29,24,48,0.08)`, 46 dp with its border, around a 20 dp icon.
pub fn profile_button() -> Container<Message> {
    Container::new(icon(&icons::USER, 20.0, INK))
        .center()
        .size(46.0, 46.0)
        .radius(23.0)
        .background(WHITE)
        .border(1.0, LINE)
        .on_press(Message::OpenPlans)
        .label("Profile and settings")
        .id("profile")
}

/// The model's name with a chevron: `height: 44px; gap: 4px; padding: 0
/// {pad}px; color: #5E5873; font-size: 13px; font-weight: 600`, with a
/// 13 dp chevron. Opens the model picker.
pub fn model_chip(mode: Mode, pad: f32) -> Container<Message> {
    row![
        jakarta(mode.name(), 13.0).semibold().color(MUTED),
        icon(&icons::CHEVRON_DOWN, 13.0, MUTED),
    ]
    .gap(4.0)
    .cross_align(CrossAlign::Center)
    .height(44.0)
    .padding_xy(pad, 0.0)
    .radius(22.0)
    .on_press(Message::OpenPicker)
    .label(format!("Model: {}", mode.name()))
    .id("model")
}

// ------------------------------------------------------------------- bars

/// The first row of a pushed screen: `display: flex; align-items: center;
/// gap: 4px; margin: 0 -10px`, a 44 dp back arrow with a 22 dp icon, the
/// title (`font-size: 16px; font-weight: 700; white-space: nowrap`), and
/// whatever the screen puts at the end.
pub fn top_bar(
    title: impl Into<Cow<'static, str>>,
    trailing: Option<Container<Message>>,
) -> Container<Message> {
    Row::new()
        .push(icon_button(&icons::BACK, 22.0, 44.0, "Back", Message::Back).id("back"))
        .push(
            jakarta(title, 16.0)
                .bold()
                .max_lines(1)
                .fill_width()
                .heading(),
        )
        .push_maybe(trailing)
        .gap(4.0)
        .cross_align(CrossAlign::Center)
        .fill_width()
}

/// The tab bar:
///
/// ```css
/// nav  { height: 74px; display: flex; align-items: center; justify-content: space-around;
///        background: #FFFFFF; border-top: 1px solid rgba(29,24,48,0.08) }
/// a    { width: 96px; height: 60px; flex-direction: column; align-items: center;
///        justify-content: center; gap: 4px; font-size: 12px; font-weight: 500 (700 when current) }
/// span { width: 56px; height: 30px; border-radius: 15px;
///        background: rgba(160,40,122,0.12) (when current) }
/// ```
///
/// Its white runs under the gesture bar; the items stay above it.
pub fn tab_bar(current: Tab) -> Container<Message> {
    let item = |tab: Tab| {
        let on = tab == current;
        let (asset, color) = if on {
            (tab.icon_active(), ACCENT)
        } else {
            (tab.icon(), MUTED)
        };
        let label = jakarta(tab.title(), 12.0);
        col![
            Container::new(icon(asset, 21.0, color))
                .center()
                .size(56.0, 30.0)
                .radius(15.0)
                .background(if on { TINT_12 } else { with_alpha(ACCENT, 0.0) }),
            if on {
                label.bold()
            } else {
                label.medium().color(MUTED)
            },
        ]
        .gap(4.0)
        .center()
        .size(96.0, 60.0)
        .radius(16.0)
        .on_press(Message::Tab(tab))
        .role(Role::Tab)
        .selected(on)
        .id(("tab", tab))
    };
    col![
        Divider::new().color(LINE),
        Row::new()
            .extend(Tab::ALL.map(item))
            .main_align(MainAlign::SpaceAround)
            .cross_align(CrossAlign::Center)
            .fill_width()
            .height(74.0),
    ]
    .fill_width()
    .background(WHITE)
    .safe_area(SafeEdges::BOTTOM)
    .id("tabs")
}

// ------------------------------------------------------------ rows, cards

/// A white card of rows with hairlines between them: `display: flex;
/// flex-direction: column; border-radius: {radius}px; background: #FFFFFF;
/// border: 1px solid rgba(29,24,48,0.08)`. A `div`: the border is outside
/// the rows, and each row's `border-bottom` is its last pixel row.
pub fn list_card(rows: Vec<Container<Message>>, radius: f32) -> Container<Message> {
    let mut card = Column::new();
    let last = rows.len().saturating_sub(1);
    for (index, row) in rows.into_iter().enumerate() {
        card = card.push(row);
        if index < last {
            card = card.push(Divider::new().color(LINE));
        }
    }
    card.fill_width()
        .padding(1.0)
        .radius(radius + 1.0)
        .background(WHITE)
        .border(1.0, LINE)
}

/// One row of a [`list_card`]: `min-height: {height}px; display: flex;
/// align-items: center; gap: {gap}px; padding: 0 {pad}px`, with the
/// hairline under it counted in the height unless it is the last.
pub fn list_row(
    content: Container<Message>,
    height: f32,
    gap: f32,
    pad: f32,
    last: bool,
) -> Container<Message> {
    content
        .gap(gap)
        .cross_align(CrossAlign::Center)
        .fill_width()
        .min_height(if last { height } else { height - 1.0 })
        .padding_xy(pad, 0.0)
}

/// The two lines of a row: `font-size: 15px; font-weight: 700` over
/// `font-size: 13px; color: #5E5873`, `gap: 2px`.
pub fn two_lines<M: 'static>(
    title: impl Into<Cow<'static, str>>,
    detail: impl Into<Cow<'static, str>>,
) -> Container<M> {
    col![
        jakarta(title, 15.0).bold().fill_width(),
        jakarta(detail, 13.0).color(MUTED).fill_width(),
    ]
    .gap(2.0)
    .fill_width()
}

/// The chevron at the end of a row that leads somewhere: 16 dp, `#5E5873`.
pub fn chevron() -> Icon {
    icon(&icons::CHEVRON_RIGHT, 16.0, MUTED)
}

/// A radio button, 22 dp: `background: #A0287A` with a 12 dp white tick
/// when chosen, `border: 1.5px solid #8E88A3` when not.
pub fn radio<M: 'static>(chosen: bool) -> Container<M> {
    if chosen {
        Container::new(icon(&icons::CHECK_BOLD, 12.0, WHITE))
            .center()
            .size(22.0, 22.0)
            .radius(11.0)
            .background(ACCENT)
    } else {
        Container::empty()
            .size(22.0, 22.0)
            .radius(11.0)
            .border(1.5, QUIET)
    }
}

/// A filter chip: a `button` with `height: 44px; padding: 0 16px;
/// border-radius: 22px; font-size: 13px; font-weight: 600`, ink with white
/// text when chosen, white with a `rgba(29,24,48,0.12)` outline when not.
pub fn chip(label: &'static str, chosen: bool, message: Message) -> Container<Message> {
    let label = jakarta(label, 13.0).semibold();
    Container::new(if chosen { label.color(WHITE) } else { label })
        .center()
        .height(44.0)
        // The button's own 1 px border is inside its width.
        .padding_xy(17.0, 0.0)
        .radius(22.0)
        .background(if chosen { INK } else { WHITE })
        .border(1.0, if chosen { INK } else { LINE_12 })
        .on_press(message)
        .selected(chosen)
}

/// A small-caps heading: `font-size: 12px; font-weight: 700;
/// letter-spacing: 0.1em; text-transform: uppercase; color: #5E5873`.
pub fn section_heading(title: &'static str) -> Text {
    jakarta(title, 12.0)
        .bold()
        .letter_spacing(1.2)
        .uppercase()
        .color(MUTED)
        .heading()
}

// ---------------------------------------------------------------- fields

/// A text field's frame: a `div` with `height: 54px; gap: 12px; padding:
/// 0 16px; border-radius: 16px; background: #FFFFFF; border: 1px solid
/// rgba(29,24,48,0.16)`, 56 dp with its border (`height` is that outer
/// height), and a 19 dp icon in `#5E5873`. A bare `<input>` has 2 px of padding of its own after the
/// gap.
pub fn text_field(
    asset: &'static SvgAsset,
    input: TextInput<Message>,
    height: f32,
    radius: f32,
    outline: Color,
) -> Container<Message> {
    row![
        icon(asset, 19.0, MUTED),
        Container::new(field_text(input))
            .fill_width()
            .padding_xy(2.0, 0.0),
    ]
    .gap(12.0)
    .cross_align(CrossAlign::Center)
    .fill_width()
    .height(height)
    .padding_xy(17.0, 0.0)
    .radius(radius)
    .background(WHITE)
    .border(1.0, outline)
    .focused_border(1.5, ACCENT)
}

/// The text of every field: `color: #1D1830; font-size: 16px`, the
/// placeholder in `#6F6985`.
pub fn field_text(input: TextInput<Message>) -> TextInput<Message> {
    input
        .style(jakarta("", 16.0))
        .placeholder_color(PLACEHOLDER)
        .caret_color(ACCENT)
        .fill_width()
}

/// The message field with its buttons:
///
/// ```css
/// height: 56px; box-sizing: border-box; display: flex; align-items: center; gap: 4px;
/// padding: 0 6px; border-radius: 28px; background: #FFFFFF;
/// border: 1px solid rgba(29,24,48,0.10); box-shadow: 0 8px 24px rgba(120,80,160,0.10);
/// ```
///
/// A 44 dp "+" on `rgba(29,24,48,0.05)`, the input (`font-size: 16px;
/// padding: 0 8px`), and at the end the microphone while the field is
/// empty and the send button once it is not, as the design's script does.
pub fn composer(draft: &str, placeholder: &'static str) -> Container<Message> {
    let end = if draft.trim().is_empty() {
        icon_button(
            &icons::MIC,
            21.0,
            44.0,
            "Start voice conversation",
            Message::OpenVoice,
        )
        .id("mic")
    } else {
        Container::new(icon(&icons::SEND, 20.0, WHITE))
            .center()
            .size(44.0, 44.0)
            .radius(22.0)
            .gradient(Gradient::angled(135.0, BRAND_FROM, BRAND_TO))
            .on_press(Message::Send)
            .label("Send")
            .id("send")
    };
    row![
        Container::new(icon(&icons::PLUS, 20.0, INK))
            .center()
            .size(44.0, 44.0)
            .radius(22.0)
            .background(INK_05)
            .on_press(Message::Attach)
            .label("Add a photo, file or camera shot")
            .id("attach"),
        Container::new(field_text(
            TextInput::new(draft)
                .placeholder(placeholder)
                .action(ImeAction::Send)
                .on_input(Message::Draft)
                .on_submit(Message::Send)
                .semantic_label("Message Atlas")
                .id("draft"),
        ))
        .fill_width()
        .padding_xy(8.0, 0.0),
        end,
    ]
    .gap(4.0)
    .cross_align(CrossAlign::Center)
    .fill_width()
    .height(56.0)
    .padding_xy(7.0, 0.0)
    .radius(RADIUS_SHEET)
    .background(WHITE)
    .border(1.0, LINE_10)
    .shadow(COMPOSER_SHADOW)
    .input_frame()
    .id("composer")
}

/// An attachment: a `span` with `height: 32px; gap: 6px; padding: 0 12px;
/// border-radius: 16px; background: #FFFFFF; border: 1px solid
/// rgba(29,24,48,0.10); font-size: 12px; font-weight: 600`, 34 dp with its
/// border, and a 14 dp document in the accent.
pub fn attachment_chip(label: impl Into<Cow<'static, str>>) -> Container<Message> {
    row![
        icon(&icons::FILE_CHIP, 14.0, ACCENT),
        jakarta(label, 12.0).semibold()
    ]
    .gap(6.0)
    .cross_align(CrossAlign::Center)
    .height(34.0)
    .padding_xy(13.0, 0.0)
    .radius(17.0)
    .background(WHITE)
    .border(1.0, LINE_10)
}

/// The spinner: `width: 18px; height: 18px; border-radius: 50%; border:
/// 2.5px solid rgba(160,40,122,0.2); border-top-color: #A0287A`. A browser
/// draws the top border of a round box as the quarter from -135 to -45
/// degrees, so it is two arcs (`icons.rs`); `turn` degrees turn it.
pub fn spinner<M: 'static>(turn: f32) -> Container<M> {
    stack![
        icon(&icons::SPINNER_TRACK, 18.0, TINT_20),
        icon(&icons::SPINNER_ARC, 18.0, ACCENT),
    ]
    .size(18.0, 18.0)
    .rotate(turn)
    .exclude_semantics()
}

/// A line over the screen for a few seconds: something the demo does not
/// do, said in the design's own card style (the app's own design).
pub fn toast(text: String) -> Container<Message> {
    Container::new(
        Container::new(jakarta(text, 13.0).semibold().color(WHITE).centered())
            .padding_xy(16.0, 12.0)
            .radius(RADIUS_ROW)
            .background(INK)
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
