//! The pieces the screens share.
//!
//! The first group is the design's own: each was written out in more than
//! one of its five screens (the CSS is quoted where the piece is first
//! used, in `screens/`). The second group is the app's own, for the
//! screens the design does not have, built from the same tokens.
//!
//! Every piece is generic over the message type, so each screen keeps its
//! own `Message`. A tappable piece gets its `.id(..)` where a screen uses
//! it (the back arrow and the tab bar, which are the same everywhere, name
//! themselves), so that `tests/e2e.rs` can find every control in the
//! screens' source.

use std::borrow::Cow;

use morningstar::ui::prelude::*;

use crate::icons::{BACK, CHEVRON, TAB_HISTORY, TAB_HOME, TAB_MORE, TAB_PROGRESS};
use crate::script::{Scenario, Skill};
use crate::theme::{
    AVATAR, BLACK, BLUE, GREY, MUTED, PURPLE, QUIET, RED, SILVER, SURFACE, TEAL, TRACK, WHITE,
    YELLOW, abril_fatface, archivo_black, caveat, outfit,
};
use crate::{Screen, Tab};

/// The design's `padding: .. 24px ..` on every page.
pub const MARGIN: f32 = 24.0;
/// The design's `padding: 52px ..` above every page's first row.
pub const TOP: f32 = 52.0;

// ------------------------------------------------------- from the design

/// The back arrow: `width: 44px; height: 44px; display: flex; align-items:
/// center; justify-content: flex-start` around the 24 dp icon.
pub fn back_button<M>(message: M) -> Container<M> {
    Row::new()
        .push(Icon::vector(BACK).size(24.0).color(WHITE))
        .size(44.0, 44.0)
        .cross_align(CrossAlign::Center)
        .on_press(message)
        .pressed_background(BLACK)
        .label("Back")
        .id("back")
}

/// A track (`background: #2C2C2E`) with its first `share` filled in
/// `color`. The design gives the fill as a percentage of the track; both
/// parts take a share of the row's width by weight.
pub fn progress_bar<M>(share: f32, height: f32, radius: f32, color: Color) -> Container<M> {
    let share = if share.is_finite() {
        share.clamp(0.0, 1.0)
    } else {
        0.0
    };
    Row::new()
        .push_maybe((share > 0.0).then(|| {
            Container::<M>::empty()
                .fill_width_weight(share)
                .height(height)
                .radius(radius)
                .background(color)
        }))
        .push_maybe((share < 1.0).then(|| {
            Container::<M>::empty()
                .fill_width_weight(1.0 - share)
                .height(height)
        }))
        .fill_width()
        .height(height)
        .radius(radius)
        .background(TRACK)
}

/// A cut-out letter: a black letter on a coloured plate that hugs it with
/// the design's padding (across, down). The caller gives the letter its
/// `line-height` and turns the plate.
pub fn plate<M>(letter: Text, across: f32, down: f32, color: Color) -> Container<M> {
    Container::new(letter.color(BLACK))
        .padding_xy(across, down)
        .background(color)
}

/// A score as two cut-out digits and "/100":
/// `display: flex; align-items: flex-end; gap: 3px`.
pub fn score_plates<M: 'static>(score: u8) -> Container<M> {
    let score = score.min(99);
    let (tens, units) = (score / 10, score % 10);
    // `background: #F6C445; font-family: 'Abril Fatface'; font-size: 72px;
    // line-height: 1; padding: 2px 10px; transform: rotate(-5deg)`.
    let first: Container<M> = plate(
        abril_fatface(tens.to_string(), 72.0).line_height_dp(72.0),
        10.0,
        2.0,
        YELLOW,
    );
    // `background: #FFFFFF; font-family: 'Archivo Black'; font-size: 64px;
    // line-height: 1; padding: 6px 8px; transform: rotate(4deg)
    // translateY(-6px)`.
    let second: Container<M> = plate(
        archivo_black(units.to_string(), 64.0).line_height_dp(64.0),
        8.0,
        6.0,
        WHITE,
    );
    row![
        first.rotate(-5.0),
        second.transform(Transform::rotate(4.0).and_translate(0.0, -6.0)),
        // `font-size: 22px; color: #A1A1A6; margin-left: 8px;
        // margin-bottom: 6px`.
        Container::new(outfit("/100", 22.0).color(MUTED)).padding_ltrb(8.0, 0.0, 0.0, 6.0),
    ]
    .gap(3.0)
    .cross_align(CrossAlign::End)
    // Two turned plates and a suffix are one number to a screen reader.
    .label(format!("{score} out of 100"))
}

/// The score block of the call report: `display: flex; align-items:
/// flex-end; gap: 16px`: a label (`font-size: 17px; color: #C7C7CC`) over
/// the score's plates and, 30 dp above the block's foot, a handwritten note
/// (`font-family: 'Caveat'; font-size: 38px; color: #F6C445; transform:
/// rotate(-8deg); margin-bottom: 30px`).
///
/// The note is pinned where the design's row puts it, 198 dp from the
/// left (the label column's 182 and the gap), rather than laid out in a
/// row: on a narrow phone it then runs into the page's margin instead of
/// not fitting.
pub fn score_block<M: 'static>(
    label: impl Into<Cow<'static, str>>,
    score: u8,
    note: impl Into<Cow<'static, str>>,
) -> Container<M> {
    stack![
        col![
            outfit(label, 17.0).color(SILVER).heading(),
            // `margin-top: 10px`.
            score_plates(score),
        ]
        .gap(10.0),
        Container::new(caveat(note, 38.0).color(YELLOW).rotate(-8.0))
            .padding_ltrb(0.0, 0.0, 0.0, 30.0)
            .place(Alignment::BOTTOM_LEFT)
            .offset(198.0, 0.0),
    ]
    .fill_width()
}

/// A tilted, outlined card with a small-caps label: `border: 2px solid;
/// border-radius: 6px; padding: 12px; transform: rotate(..)`, the border
/// inside the box as in CSS. The label is `font-size: 13px; font-weight:
/// 700; letter-spacing: 1px` in the card's colour, 8 dp above the body.
pub fn note_card<M, E: Into<Element<M>>>(
    label: &'static str,
    color: Color,
    degrees: f32,
    body: impl IntoIterator<Item = E>,
) -> Container<M> {
    Column::new()
        .push(outfit(label, 13.0).bold().letter_spacing(1.0).color(color))
        .push(Spacer::fixed(8.0))
        .extend(body)
        .fill_width()
        .padding(14.0)
        .radius(6.0)
        .border(2.0, color)
        .rotate(degrees)
}

/// The yellow button: `border-radius: 22px; background: #F6C445; color:
/// #0E0E0E; font-weight: 700`, across the width, `height` tall with a
/// label of `size`.
pub fn primary_button<M>(
    label: impl Into<Cow<'static, str>>,
    size: f32,
    height: f32,
    message: M,
) -> Container<M> {
    Container::new(outfit(label, size).bold().color(BLACK))
        .center()
        .fill_width()
        .height(height)
        .radius(22.0)
        .background(YELLOW)
        .on_press(message)
}

/// The quiet button: `border-radius: 22px; border: 2px solid #3A3A3C;
/// font-size: 17px; font-weight: 600`. The caller gives it a size.
pub fn outline_button<M>(label: impl Into<Cow<'static, str>>, message: M) -> Container<M> {
    Container::new(outfit(label, 17.0).semibold())
        .center()
        .radius(22.0)
        .border(2.0, QUIET)
        .on_press(message)
}

/// A small-caps label: `font-size: 13px; font-weight: 600; letter-spacing:
/// 1.2px; color: #A1A1A6`. The text is written in capitals.
pub fn small_caps(label: impl Into<Cow<'static, str>>) -> Text {
    outfit(label, 13.0)
        .semibold()
        .letter_spacing(1.2)
        .color(MUTED)
}

/// A tip chip: `border: 1.5px solid; border-radius: 999px; padding: 6px
/// 12px; font-size: 15px`, 34 dp tall. With a `width` it is the size the
/// design's chip was laid out at; without, it hugs its label with the same
/// padding.
pub fn chip<M>(
    label: impl Into<Cow<'static, str>>,
    width: Option<f32>,
    ink: Color,
    outline: Color,
) -> Container<M> {
    let chip = Container::new(outfit(label, 15.0).color(ink))
        .height(34.0)
        .center()
        .radius(999.0)
        .border(1.5, outline);
    match width {
        Some(width) => chip.width(width),
        None => chip.padding_xy(13.5, 0.0),
    }
}

/// A skill's name and score (`font-size: 16px`, the score with
/// `tabular-nums`) over a bar filled to that percentage: `height: 8px;
/// border-radius: 4px; background: #2C2C2E; margin-top: 6px`.
pub fn skill_bar<M>(skill: Skill, percent: u8) -> Container<M> {
    col![
        row![
            outfit(skill.name(), 16.0),
            outfit(percent.to_string(), 16.0).tabular_nums(),
        ]
        .space_between()
        .fill_width(),
        progress_bar(f32::from(percent) / 100.0, 8.0, 4.0, skill_color(skill)),
    ]
    .gap(6.0)
    .fill_width()
    // One stop: the skill, and how far its bar is filled.
    .role(Role::Progress)
    .label(skill.name())
    .value(format!("{percent} out of 100"))
}

/// The design's colour for each skill's bar.
pub const fn skill_color(skill: Skill) -> Color {
    match skill {
        Skill::Clarity => YELLOW,
        Skill::Empathy => PURPLE,
        Skill::Listening => BLUE,
        Skill::ProblemSolving => TEAL,
        Skill::Professional => RED,
    }
}

/// The five skill bars: `display: flex; flex-direction: column; gap: 14px`.
pub fn skill_bars<M>(scores: &crate::model::Scores) -> Container<M> {
    Column::new()
        .extend(
            Skill::ALL
                .into_iter()
                .map(|skill| skill_bar(skill, scores.get(skill))),
        )
        .gap(14.0)
        .fill_width()
}

/// The round avatar with the learner's initials: `border-radius: 50%;
/// border: 2px solid #F6C445; background: #2A2A2E; font-weight: 700`.
/// Home's is 46 dp with 15 dp letters.
pub fn avatar<M>(initials: String, size: f32, letters: f32) -> Container<M> {
    // The initials are a picture of the learner: whatever the avatar is
    // part of says who it is.
    Container::new(outfit(initials, letters).bold().exclude_semantics())
        .size(size, size)
        .center()
        .radius(size / 2.0)
        .background(AVATAR)
        .border(2.0, YELLOW)
}

/// The tab bar: `height: 66px; border-radius: 24px; background: #1C1C1E;
/// display: grid; grid-template-columns: repeat(4, minmax(0, 1fr));
/// align-items: center`, each item `height: 56px` around a 26 dp icon, the
/// current one in the accent colour.
pub fn tab_bar<M>(current: Tab, message: impl Fn(Tab) -> M) -> Container<M> {
    let item = |tab: Tab, icon: VectorIcon| {
        let color = if tab == current { YELLOW } else { GREY };
        Container::new(Icon::vector(icon).size(26.0).color(color))
            .fill_width()
            .height(56.0)
            .center()
            .on_press(message(tab))
            .pressed_background(SURFACE)
            // An icon alone: its name, that it is a tab, and whether it
            // is the one showing.
            .label(tab.title())
            .role(Role::Tab)
            .selected(tab == current)
            .id(("tab", tab))
    };
    row![
        item(Tab::Home, TAB_HOME),
        item(Tab::Progress, TAB_PROGRESS),
        item(Tab::History, TAB_HISTORY),
        item(Tab::More, TAB_MORE),
    ]
    .cross_align(CrossAlign::Center)
    .fill_width()
    .height(66.0)
    .radius(24.0)
    .background(SURFACE)
}

/// A tab-level screen: `content` scrolls when it does not fit and the tab
/// bar stays at the bottom (`padding: .. 24px 20px` under it), as the
/// design's home does.
pub fn tab_screen<M: 'static>(
    screen: Screen,
    content: impl Into<Element<M>>,
    current: Tab,
    message: impl Fn(Tab) -> M,
) -> Element<M> {
    col![
        ScrollView::new(content).id(screen.scroll_id()),
        Container::new(tab_bar(current, message))
            .fill_width()
            .padding_ltrb(MARGIN, 0.0, MARGIN, 20.0),
    ]
    .fill()
    .background(BLACK)
    .safe_area(SafeEdges::ALL)
    .id(screen.name())
    .into()
}

/// A screen that is pushed over another: `content` scrolls when it does
/// not fit; `footer`, if any, stays at the bottom (`padding: .. 24px
/// 28px`), as the design's buttons do.
pub fn pushed_screen<M: 'static>(
    screen: Screen,
    content: impl Into<Element<M>>,
    footer: Option<Container<M>>,
) -> Element<M> {
    Column::new()
        .push(ScrollView::new(content).id(screen.scroll_id()))
        .push_maybe(footer.map(|footer| {
            Container::new(footer)
                .fill_width()
                .padding_ltrb(MARGIN, 0.0, MARGIN, 28.0)
        }))
        .fill()
        .background(BLACK)
        .safe_area(SafeEdges::ALL)
        .id(screen.name())
        .into()
}

/// The padding of a page's scrolling content: the design's 52 dp above and
/// 24 dp at the sides, and room below for what is turned or hangs out.
pub fn page<M>(content: Container<M>) -> Container<M> {
    content.fill_width().padding_ltrb(MARGIN, TOP, MARGIN, 20.0)
}

// ------------------------------------------------------- the app's own

/// The first row of the app's own pushed screens, after the call report's:
/// the back arrow and a quiet caption (`font-size: 15px; color: #A1A1A6`).
pub fn page_header<M>(back: M, caption: impl Into<Cow<'static, str>>) -> Container<M> {
    row![back_button(back), outfit(caption, 15.0).color(MUTED)]
        .space_between()
        .cross_align(CrossAlign::Center)
        .fill_width()
}

/// A page's title, in the live call's heading style: `font-size: 32px;
/// line-height: 1.12; font-weight: 700`.
pub fn page_title(title: impl Into<Cow<'static, str>>) -> Text {
    outfit(title, 32.0)
        .bold()
        .line_height_dp(35.84)
        .fill_width()
        .heading()
}

/// A handwritten note in the accent colour, turned a little.
pub fn handwritten(note: impl Into<Cow<'static, str>>, size: f32, degrees: f32) -> Text {
    caveat(note, size).color(YELLOW).rotate(degrees)
}

/// A row that leads somewhere: a title, a quieter detail under it and a
/// chevron, 14 dp above and below.
pub fn link_row<M>(
    title: impl Into<Cow<'static, str>>,
    detail: impl Into<Cow<'static, str>>,
    message: M,
) -> Container<M> {
    row![
        col![
            outfit(title, 19.0).semibold().fill_width(),
            outfit(detail, 15.0).color(MUTED).fill_width(),
        ]
        .gap(2.0)
        .fill_width(),
        Icon::vector(CHEVRON).size(16.0).color(GREY),
    ]
    .gap(14.0)
    .cross_align(CrossAlign::Center)
    .fill_width()
    .padding_xy(0.0, 14.0)
    .on_press(message)
    .pressed_background(BLACK)
}

/// A hairline between rows.
pub fn hairline() -> Divider {
    Divider::new().color(TRACK)
}

/// A choice among a few: a pill like the tip chip, 40 dp tall, filled in
/// the accent colour when it is the chosen one. What is tapped is 48 dp
/// tall, the pill in its middle: a finger needs that much
/// (`Ui::accessibility_issues`), and these are the app's own, not the
/// design's.
pub fn option_chip<M>(label: &'static str, selected: bool, message: M) -> Container<M> {
    let (ink, fill, outline) = if selected {
        (BLACK, YELLOW, YELLOW)
    } else {
        (WHITE, BLACK, QUIET)
    };
    Container::new(
        Container::new(outfit(label, 15.0).medium().color(ink))
            .height(40.0)
            .center()
            .padding_xy(15.0, 0.0)
            .radius(999.0)
            .background(fill)
            .border(1.5, outline),
    )
    .height(48.0)
    .min_width(48.0)
    .center()
    .on_press(message)
    .pressed_background(BLACK)
    .selected(selected)
}

/// The colour a scenario wears: its card's on home, and teal for today's
/// call.
pub const fn scenario_color(scenario: Scenario) -> Color {
    match scenario {
        Scenario::MissingDelivery => TEAL,
        Scenario::AngryCustomer => PURPLE,
        Scenario::RefundRequest => BLUE,
        Scenario::BillingProblem => YELLOW,
    }
}

/// A past call as a row: its score on a small turned plate in the
/// scenario's colour, the scenario, and when and how long.
pub fn call_row<M>(report: &crate::model::Report, degrees: f32, message: M) -> Container<M> {
    row![
        Container::new(
            outfit(report.overall().to_string(), 22.0)
                .bold()
                .tabular_nums()
                .color(BLACK)
        )
        .size(48.0, 48.0)
        .center()
        .background(scenario_color(report.scenario))
        .rotate(degrees),
        col![
            outfit(report.title(), 19.0).semibold().fill_width(),
            outfit(
                format!("{} \u{b7} {}", report.day(), report.duration()),
                15.0
            )
            .color(MUTED)
            .fill_width(),
        ]
        .gap(2.0)
        .fill_width(),
        Icon::vector(CHEVRON).size(16.0).color(GREY),
    ]
    .gap(16.0)
    .cross_align(CrossAlign::Center)
    .fill_width()
    .padding_xy(0.0, 14.0)
    .on_press(message)
    .pressed_background(BLACK)
    // The scenario first; the number on the plate is its score.
    .label(format!(
        "{}, score {}, {} \u{b7} {}",
        report.title(),
        report.overall(),
        report.day(),
        report.duration()
    ))
}

/// A paragraph of quiet body text: `font-size: 15px; line-height: 1.35;
/// color: #C7C7CC`.
pub fn body(content: impl Into<Cow<'static, str>>) -> Text {
    outfit(content, 15.0)
        .line_height_dp(20.25)
        .color(SILVER)
        .fill_width()
}

/// A line shown for a few seconds over whatever screen is up, under the
/// status bar: a save that failed, a call that fell back to the script.
/// It takes no touches and goes by itself.
pub fn toast<M: 'static>(message: String) -> Element<M> {
    SafeArea::new(
        Container::new(
            outfit(message, 15.0)
                .line_height_dp(20.25)
                .color(WHITE)
                .fill_width(),
        )
        .fill_width()
        .padding_xy(16.0, 12.0)
        .radius(14.0)
        .background(SURFACE)
        .border(1.5, YELLOW)
        // Gone in a few seconds: read out without being looked for.
        .live(Live::Polite),
    )
    .padding_ltrb(MARGIN, 8.0, MARGIN, 0.0)
    .into()
}
