//! Screen 03: a practice call in progress. `design/html/03-live-call.html`.
//!
//! A header with the call's progress and clock, the task, the conversation
//! (the line said last, the customer's line, the learner's line with its
//! blank and the tips) and the call bar: who is on the line, mute and end
//! call. The header and the call bar stay put; the task and the
//! conversation scroll between them on a phone shorter than the design's.
//!
//! How a scripted call runs ([`Call`]): the customer says a line; the
//! learner answers by tapping one of the two tips, which completes their
//! sentence, moves it to the top as the line said last and brings the
//! customer's next line. The clock counts seconds, the bar in the header
//! shows how much of the script is done and the thin bar how much of the
//! target length is used. The loudspeaker has the customer repeat the
//! line; the microphone mutes, and a muted learner cannot answer; the red
//! button, or the last answer, ends the call and opens its report; back
//! abandons it.
//!
//! The moment the design shows ([`Call::designed`]) is drawn exactly as
//! the design draws it, with pieces placed by measurement; every other
//! moment is drawn by the same functions from the script, and re-flows at
//! any width.
//!
//! How a call played by the server runs ([`OnlineCall`], the app's own
//! design in the same language): the caller's line and two tips come from
//! the server, and the tips are advice (dashed chips that cannot be
//! tapped), not answers. The learner answers in their own words, in a bar
//! fixed above the call bar: a text field with a send button, or, when the
//! server has a voice, a microphone that is tapped to start recording and
//! tapped again to send. What was said last sits dimmed above the
//! caller's new line. Each new line starts the conversation at its top
//! again (the scroll view is a new one per line), so the caller's line is
//! never left below the fold. The bar in the header is the time used.

use morningstar::ui::prelude::*;

use crate::Screen;
use crate::api::{self, REPLY_LIMIT};
use crate::components::{MARGIN, TOP, back_button, chip, hairline, progress_bar, small_caps};
use crate::icons::{HANDSET, MICROPHONE, ORDINAL, SPEAKER, TICK, TIP_ARROW};
use crate::model::{Call, Link, Live, Model};
use crate::online::{OnlineCall, Phase, RECORDING_LIMIT, Sound};
use crate::script::{Exchange, Scenario};
use crate::theme::{
    BLACK, BLUE, DIM, GREEN, GREY, MUTED, PURPLE, QUIET, RED, SILVER, SURFACE, TRACK, WHITE,
    YELLOW, caveat, outfit, outfit_rich,
};

/// What the screen can ask for.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// The back arrow: abandons the call.
    Back,
    /// The loudspeaker: the customer repeats the line.
    Replay,
    /// The microphone button: mutes and unmutes.
    ToggleMute,
    /// A tip chip, by position: the learner's answer.
    Choose(usize),
    /// The red button: ends the call and opens its report.
    EndCall,
    /// The reply field changed: the whole new text.
    Draft(String),
    /// The send button, or the keyboard's Send: the reply goes to the
    /// caller.
    Send,
    /// The big microphone: starts recording, or stops and sends.
    Talk,
    /// "Aa" beside the microphone: type this call instead.
    TypeInstead,
    /// The small microphone beside the field: speak again.
    SpeakInstead,
    /// "Retry" beside a message: sends again what failed.
    Retry,
}

/// The customer's avatar: a disc in four colours with a black hub.
///
/// `<svg width="54" height="54" viewBox="0 0 54 54">` of four quarter
/// `<path>`s and a `<circle cx="27" cy="27" r="8">`.
const AVATAR_DISC: VectorPicture = VectorPicture::new(
    54.0,
    54.0,
    &[
        Layer::path("M27 27V0A27 27 0 0 1 54 27z").fill(BLUE),
        Layer::path("M27 27h27A27 27 0 0 1 27 54z").fill(YELLOW),
        Layer::path("M27 27v27A27 27 0 0 1 0 27z").fill(GREEN),
        Layer::path("M27 27H0A27 27 0 0 1 27 0z").fill(RED),
        Layer::circle(27.0, 27.0, 8.0).fill(BLACK),
    ],
);

// ------------------------------------------------------------------- view

/// The call on the line: the server's when there is one, otherwise the
/// script's.
pub fn view(model: &Model) -> Element<Message> {
    match &model.live {
        Some(Live::Active(call)) => online(call),
        Some(Live::Dialing(scenario)) => dialing(*scenario, model),
        None => scripted(&model.call, script_note(model)),
    }
}

/// What to say above a scripted call when a server was expected to play
/// it. Nothing for the design's moment, and nothing where no server can
/// ever be reached (a screenshot, a test with no host).
fn script_note(model: &Model) -> Option<&'static str> {
    if model.call.designed {
        return None;
    }
    match model.link {
        Link::Unknown | Link::Standalone => None,
        Link::Offline(_) => Some("OFFLINE \u{b7} PRACTICE SCRIPT"),
        Link::Checking | Link::Online { .. } => Some("PRACTICE SCRIPT"),
    }
}

/// `display: flex; flex-direction: column; background: #0E0E0E`.
fn scripted(call: &Call, note: Option<&'static str>) -> Element<Message> {
    let (script, time) = call.progress();
    col![
        header(script, call.clock()),
        ScrollView::new(
            Column::new()
                .push_maybe(note.map(|note| {
                    Container::new(small_caps(note).color(GREY))
                        .padding_ltrb(MARGIN, 14.0, MARGIN, 0.0)
                }))
                .push(task(call))
                .push(conversation(call))
                .fill_width()
                .padding_ltrb(0.0, 0.0, 0.0, 24.0)
        )
        .id(Screen::LiveCall.scroll_id()),
        // `height: 4px; background: #2C2C2E` with `width: 22%` of yellow.
        progress_bar(time, 4.0, 0.0, YELLOW),
        call_bar(call),
    ]
    .fill()
    .background(BLACK)
    .safe_area(SafeEdges::ALL)
    .id(Screen::LiveCall.name())
    .into()
}

/// `padding: 52px 24px 0; display: flex; align-items: center; gap: 12px`:
/// back, the call's progress (`width: 32%`) and its clock.
fn header(share: f32, clock: String) -> Container<Message> {
    row![
        back_button(Message::Back),
        progress_bar(share, 8.0, 4.0, YELLOW),
        // `font-size: 15px; font-variant-numeric: tabular-nums`.
        outfit(clock, 15.0).tabular_nums().color(SILVER),
    ]
    .gap(12.0)
    .cross_align(CrossAlign::Center)
    .fill_width()
    .padding_ltrb(MARGIN, TOP, MARGIN, 0.0)
}

/// `padding: 22px 24px 0`: which call this is, and what to achieve.
fn task(call: &Call) -> Container<Message> {
    // `font-size: 17px; color: #C7C7CC`. The numero sign is an "N" and a
    // drawn ordinal mark (see `ORDINAL`).
    let ordinal = Icon::vector(ORDINAL)
        .size(8.0)
        .color(SILVER)
        .place(Alignment::TOP_LEFT)
        .offset(44.5, 5.0);
    let (label, heading): (Container<Message>, Element<Message>) = if call.designed {
        (
            // "Call №1 · Angry customer", 186.98 wide in the design.
            row![
                outfit("Call N", 17.0).color(SILVER),
                Spacer::new(),
                // Chrome set the "1" one dp further right than Outfit's own
                // spacing does (it follows a glyph of another font there).
                outfit("1", 17.0).color(SILVER).offset(1.0, 0.0),
                outfit(" \u{b7} Angry customer", 17.0).color(SILVER),
            ]
            .width(186.98),
            // `margin: 10px 0 0; font-size: 32px; line-height: 1.12;
            // font-weight: 700`. The two lines the design's width produces,
            // written out: Chrome put the first at y = 147 and the second
            // at 182.84, which land on device pixels 294 and 366; one
            // wrapped text here lands its first line one pixel lower.
            col![
                heading_line("Calm the caller and").offset(0.0, -0.5),
                heading_line("solve the problem").offset(0.0, -0.34),
            ]
            .into(),
        )
    } else {
        (
            row![
                outfit("Call N", 17.0).color(SILVER),
                // Room for the ordinal mark.
                Spacer::fixed(10.0),
                outfit(
                    format!("{} \u{b7} {}", call.number, call.scenario.title()),
                    17.0
                )
                .color(SILVER),
            ],
            heading_line(call.scenario.goal()).fill_width().into(),
        )
    };
    col![stack![label, ordinal], heading]
        .gap(10.0)
        .fill_width()
        .padding_ltrb(MARGIN, 22.0, MARGIN, 0.0)
}

fn heading_line(line: &'static str) -> Text {
    outfit(line, 32.0).bold().line_height_dp(35.84)
}

/// `flex: 1; padding: 26px 24px 0; gap: 22px; overflow: hidden`.
fn conversation(call: &Call) -> Container<Message> {
    let mut lines = Column::new().push(said_last(call));
    if let Some(exchange) = call.exchange() {
        lines = lines
            .push(customer_line(exchange, call.repeating > 0))
            .push(own_line(call, exchange));
    }
    lines
        .gap(22.0)
        .fill_width()
        .padding_ltrb(MARGIN, 26.0, MARGIN, 0.0)
}

/// The line said last, with a tick when it was said well, and the button
/// that has the customer repeat theirs:
/// `font-size: 24px; line-height: 1.3; color: #6E6E73`.
fn said_last(call: &Call) -> Container<Message> {
    let dim = |content: String| {
        outfit(content, 24.0)
            .line_height_dp(31.2)
            .color(DIM)
            .fill_width()
    };
    let line: Element<Message> = if call.designed {
        // The tick is the last word of the second line.
        stack![
            dim("Hi, my parcel was due last Monday".to_owned()),
            Icon::vector(TICK)
                .size(20.0)
                .color(GREEN)
                .place(Alignment::TOP_LEFT)
                .offset(92.0, 37.0),
        ]
        .fill_width()
        .into()
    } else {
        match call.last_reply() {
            Some((reply, true)) => outfit_rich(
                [span(reply).color(DIM), span(" \u{2713}").color(GREEN)],
                24.0,
            )
            .line_height_dp(31.2)
            .fill_width()
            .into(),
            Some((reply, false)) => dim(reply).into(),
            None => dim(format!("{} is on the line.", call.scenario.caller())).into(),
        }
    };
    row![
        line,
        // `width: 44px; height: 44px`, the 22 px icon centred.
        Container::new(Icon::vector(SPEAKER).size(22.0).color(WHITE))
            .size(44.0, 44.0)
            .center()
            .on_press(Message::Replay)
            .pressed_background(BLACK)
            .id("replay"),
    ]
    .gap(12.0)
    .fill_width()
}

/// What the customer just said, the first words in the accent colour; all
/// of it while it is being repeated.
fn customer_line(exchange: &Exchange, repeating: bool) -> Container<Message> {
    let rest = span(exchange.rest);
    col![
        small_caps(if repeating {
            "CUSTOMER \u{b7} ONCE MORE"
        } else {
            "CUSTOMER"
        }),
        // `font-size: 27px; line-height: 1.3; font-weight: 500`.
        outfit_rich(
            [
                span(exchange.lead).color(YELLOW),
                if repeating { rest.color(YELLOW) } else { rest },
            ],
            27.0,
        )
        .medium()
        .line_height_dp(35.1)
        .fill_width(),
    ]
    .gap(6.0)
    .fill_width()
}

/// `border-top: 1px solid #2C2C2E; padding-top: 18px`: the learner's line
/// with the blank still to be said, and the tips.
fn own_line(call: &Call, exchange: &Exchange) -> Container<Message> {
    // `font-size: 24px; line-height: 1.35`, ending in an inline box
    // `min-width: 110px; border-bottom: 2px solid #8E8E93`.
    let sentence: Element<Message> = if call.designed {
        // Chrome put the box's border at x = 198.27, y = 543 of the page:
        // 174.27 from the paragraph's left and 55.25 below its top.
        stack![
            outfit(exchange.reply, 24.0)
                .line_height_dp(32.4)
                .fill_width(),
            Container::empty()
                .size(110.0, 2.0)
                .background(GREY)
                .place(Alignment::TOP_LEFT)
                .offset(174.27, 55.45),
        ]
        .fill_width()
        .into()
    } else {
        // An inline box cannot be put in a paragraph, so the blank is a
        // run of low lines, which flows with the words at any width.
        outfit_rich(
            [
                span(format!("{} ", exchange.reply)),
                span("________").color(GREY),
            ],
            24.0,
        )
        .line_height_dp(32.4)
        .fill_width()
        .into()
    };
    col![
        Divider::new().color(TRACK),
        Spacer::fixed(18.0),
        small_caps(if call.muted {
            "YOU \u{b7} MUTED"
        } else {
            "YOU \u{b7} SPEAKING"
        }),
        // `margin: 0 0 6px`; Chrome put this paragraph's lines 0.2 higher
        // than its half-leading gives (488.75, not 488.95).
        Spacer::fixed(5.8),
        sentence,
        Spacer::fixed(14.2),
        tips(call, exchange),
    ]
    .fill_width()
}

/// `display: flex; flex-wrap: wrap; gap: 8px`. Rows cannot wrap here, so
/// the two rows the design's width produces are written out: "tip" and the
/// first chip, then the second chip. Tapping a chip answers with it; while
/// the learner is muted the chips are grey and do nothing.
fn tips(call: &Call, exchange: &Exchange) -> Container<Message> {
    // The widths of the design's two chips (text + 27), rounded to the
    // half dp a browser snaps a box's edges to at 2 px per dp.
    let widths = if call.designed {
        [Some(180.5), Some(195.0)]
    } else {
        [None, None]
    };
    let tip = |option: usize, color: Color| {
        let (ink, outline) = if call.muted {
            (GREY, QUIET)
        } else {
            (WHITE, color)
        };
        chip(exchange.options[option].tip, widths[option], ink, outline)
            .on_press_maybe((!call.muted).then_some(Message::Choose(option)))
            .id(("tip", option))
    };
    col![
        row![
            // `font-family: 'Caveat'; font-size: 24px; color: #F6C445;
            // transform: rotate(-3deg)`, stretched to the row's 34 dp with
            // the text at its top. The chip after it starts at 83.5.
            stack![
                caveat("tip", 24.0).color(YELLOW),
                Icon::vector(TIP_ARROW)
                    .size(20.0)
                    .color(YELLOW)
                    .place(Alignment::TOP_LEFT)
                    .offset(30.1, 4.4),
            ]
            .size(51.5, 34.0)
            .rotate(-3.0),
            tip(0, BLUE),
        ]
        .gap(8.0),
        tip(1, PURPLE),
    ]
    .gap(8.0)
    .fill_width()
}

/// `background: #1C1C1E; padding: 18px 24px 30px; display: flex;
/// align-items: center; gap: 14px`.
fn call_bar(call: &Call) -> Container<Message> {
    // Muted: the button turns white and its microphone black, the way a
    // phone shows a switched-on call control.
    let (plate, ink) = if call.muted {
        (WHITE, BLACK)
    } else {
        (QUIET, WHITE)
    };
    row![
        Picture::vector(AVATAR_DISC).size(54.0, 54.0),
        col![
            // `font-size: 19px; font-weight: 600`.
            outfit(
                format!(
                    "{} \u{b7} {}",
                    call.scenario.caller(),
                    call.scenario.title()
                ),
                19.0
            )
            .semibold()
            .fill_width(),
            // `font-size: 15px; color: #A1A1A6; margin-top: 2px`.
            outfit(
                format!("AI customer \u{b7} {} accent", call.accent.label()),
                15.0
            )
            .color(MUTED)
            .fill_width(),
        ]
        .gap(2.0)
        .fill_width(),
        // `width: 52px; height: 52px; border-radius: 50%; background: #3A3A3C`.
        Container::new(Icon::vector(MICROPHONE).size(22.0).color(ink))
            .size(52.0, 52.0)
            .center()
            .radius(26.0)
            .background(plate)
            .on_press(Message::ToggleMute)
            .id("mute"),
        // The same in `#E63946`, its icon with `transform: rotate(135deg)`.
        Container::new(Icon::vector(HANDSET).size(24.0).color(WHITE).rotate(135.0))
            .size(52.0, 52.0)
            .center()
            .radius(26.0)
            .background(RED)
            .on_press(Message::EndCall)
            .id("end-call"),
    ]
    .gap(14.0)
    .cross_align(CrossAlign::Center)
    .fill_width()
    .padding_ltrb(MARGIN, 18.0, MARGIN, 30.0)
    .background(SURFACE)
}

// ------------------------------------------------- a call the server plays

/// The id of an online call's scroll view while the caller's line number
/// `line` is the current one. A new line is a new scroll view, which
/// starts at its top: that is what keeps the newest line in view.
pub fn online_scroll_id(line: u32) -> (&'static str, u32) {
    (Screen::LiveCall.scroll_id(), line)
}

/// The phone is ringing: `POST /v1/calls` is out.
fn dialing(scenario: Scenario, model: &Model) -> Element<Message> {
    col![
        header(0.0, "00:00".to_owned()),
        ScrollView::new(
            col![
                task_label(model.next_call_number() as u32, scenario.title().to_owned()),
                goal_heading(scenario.goal().to_owned()),
                Spacer::fixed(16.0),
                dim_line(format!("Calling {}\u{2026}", scenario.caller()), 24.0),
            ]
            .gap(10.0)
            .fill_width()
            .padding_ltrb(MARGIN, 22.0, MARGIN, 24.0)
        )
        .id(Screen::LiveCall.scroll_id()),
        hairline(),
        Container::new(small_caps("CONNECTING\u{2026}"))
            .fill_width()
            .padding_xy(MARGIN, 14.0),
        bar(
            scenario.caller().to_owned(),
            scenario.title().to_owned(),
            format!(
                "AI customer \u{b7} {} accent",
                model.settings.accent.label()
            ),
            false,
        ),
    ]
    .fill()
    .background(BLACK)
    .safe_area(SafeEdges::ALL)
    .id(Screen::LiveCall.name())
    .into()
}

/// A call played by the server.
fn online(call: &OnlineCall) -> Element<Message> {
    let sound = if call.muted { " \u{b7} sound off" } else { "" };
    Column::new()
        .push(header(call.time_used(), call.clock()))
        .push(
            ScrollView::new(online_conversation(call))
                .id((Screen::LiveCall.scroll_id(), call.line_no)),
        )
        .push(hairline())
        .push_maybe(
            call.notice.as_ref().map(|notice| {
                notice_row(notice.text.clone(), notice.retry.then_some(Message::Retry))
            }),
        )
        .push(answer(call))
        .push(bar(
            call.caller.clone(),
            call.title.clone(),
            format!("AI customer \u{b7} {} accent{sound}", call.accent.label()),
            call.muted,
        ))
        .fill()
        .background(BLACK)
        .safe_area(SafeEdges::ALL)
        .id(Screen::LiveCall.name())
        .into()
}

/// "Call №4 · Angry customer": the numero sign is an "N" and a drawn
/// ordinal mark, as in the scripted call.
fn task_label(number: u32, title: String) -> Container<Message> {
    stack![
        row![
            outfit("Call N", 17.0).color(SILVER),
            Spacer::fixed(10.0),
            outfit(format!("{number} \u{b7} {title}"), 17.0)
                .color(SILVER)
                .max_lines(1)
                .fill_width(),
        ]
        .fill_width(),
        Icon::vector(ORDINAL)
            .size(8.0)
            .color(SILVER)
            .place(Alignment::TOP_LEFT)
            .offset(44.5, 5.0),
    ]
    .fill_width()
}

/// The call's goal in the design's heading style.
fn goal_heading(goal: String) -> Text {
    outfit(goal, 32.0).bold().line_height_dp(35.84).fill_width()
}

/// A line already said: `color: #6E6E73`.
fn dim_line(content: String, size: f32) -> Text {
    outfit(content, size)
        .line_height_dp(size * 1.3)
        .color(DIM)
        .fill_width()
}

/// What scrolls: before the learner's first turn, the task as the design
/// shows it; after it, what was said last, the caller's line and the tips
/// first, and the goal as a reminder under them.
fn online_conversation(call: &OnlineCall) -> Container<Message> {
    let first = call.turns == 0;
    let said: Text = match &call.said {
        // The learner's own words, cut to a few lines: the whole of it is
        // in the transcript.
        Some(said) => dim_line(api::clean(said, 96), 19.0),
        None => dim_line(format!("{} is on the line.", call.caller), 24.0),
    };
    Column::new()
        .push_maybe(first.then(|| {
            col![
                task_label(call.number, call.title.clone()),
                goal_heading(call.goal.clone()),
                Spacer::fixed(4.0),
            ]
            .gap(10.0)
            .fill_width()
        }))
        .push(
            row![said, speaker_button()]
                .gap(12.0)
                .cross_align(CrossAlign::Start)
                .fill_width(),
        )
        .push(online_line(call))
        .push_maybe((!call.tips.is_empty() && !call.done).then(|| advice(&call.tips)))
        .push_maybe((!first).then(|| {
            col![
                hairline(),
                Spacer::fixed(16.0),
                small_caps(format!("CALL {} \u{b7} YOUR GOAL", call.number)),
                Spacer::fixed(6.0),
                outfit(call.goal.clone(), 17.0)
                    .line_height_dp(22.95)
                    .color(SILVER)
                    .fill_width(),
            ]
            .fill_width()
        }))
        .gap(22.0)
        .fill_width()
        .padding_ltrb(MARGIN, 22.0, MARGIN, 24.0)
}

/// The loudspeaker: `width: 44px; height: 44px`, the 22 px icon centred.
fn speaker_button() -> Container<Message> {
    Container::new(Icon::vector(SPEAKER).size(22.0).color(WHITE))
        .size(44.0, 44.0)
        .center()
        .on_press(Message::Replay)
        .pressed_background(BLACK)
        .id("replay")
}

/// Where the accent colour ends in a caller's line: after its first
/// clause if that is short, otherwise after its first two words.
fn lead_of(line: &str) -> usize {
    let clause = line
        .char_indices()
        .take(26)
        .find(|(_, character)| matches!(character, ',' | '.' | '!' | '?' | ';' | ':'))
        .map(|(index, character)| index + character.len_utf8());
    let words = line
        .match_indices(' ')
        .nth(1)
        .map_or(line.len(), |(index, _)| index);
    match clause {
        Some(end) if end < line.len() => end,
        _ => words,
    }
}

/// What the caller just said, its first words in the accent colour; all of
/// it while it is being spoken or repeated. A long line is set smaller.
fn online_line(call: &OnlineCall) -> Container<Message> {
    let speaking = call.sound == Sound::Speaking;
    let whole = speaking || call.highlight > 0;
    let label = if speaking {
        "CUSTOMER \u{b7} SPEAKING"
    } else if call.highlight > 0 {
        "CUSTOMER \u{b7} ONCE MORE"
    } else if call.done {
        "CUSTOMER \u{b7} LAST WORDS"
    } else {
        "CUSTOMER"
    };
    let size = match call.line.chars().count() {
        0..=110 => 27.0,
        111..=200 => 22.0,
        _ => 19.0,
    };
    let (lead, rest) = call.line.split_at(lead_of(&call.line));
    let rest = span(rest.to_owned());
    col![
        small_caps(label),
        outfit_rich(
            [
                span(lead.to_owned()).color(YELLOW),
                if whole { rest.color(YELLOW) } else { rest },
            ],
            size,
        )
        .medium()
        .line_height_dp(size * 1.3)
        .fill_width(),
    ]
    .gap(6.0)
    .fill_width()
}

/// Whether a tip can be set on one line beside the word "tip" on the
/// narrowest phone (212 dp of text), by a generous estimate of its width:
/// a view cannot measure text.
fn fits_a_chip(tip: &str) -> bool {
    let width: f32 = tip
        .chars()
        .map(|character| match character {
            'i' | 'j' | 'l' | 't' | 'f' | 'r' | ' ' | '.' | ',' | '\'' | '\u{2019}' | '!' => 5.5,
            'm' | 'w' => 12.5,
            'a'..='z' => 8.5,
            'M' | 'W' => 14.0,
            'A'..='Z' | '0'..='9' => 10.5,
            _ if character.is_ascii() => 9.0,
            _ => 16.0,
        })
        .sum();
    width <= 212.0
}

/// The server's two tips: advice for the answer, not answers. Dashed, so
/// that they do not look like the buttons the scripted call's tips are.
fn advice(tips: &[String]) -> Container<Message> {
    let tip = |index: usize, color: Color| {
        tips.get(index).map(|tip| {
            let label = outfit(tip.clone(), 15.0).color(SILVER);
            if fits_a_chip(tip) {
                Container::<Message>::new(label)
                    .height(34.0)
                    .center()
                    .padding_xy(13.5, 0.0)
                    .radius(999.0)
                    .border_dashed(1.5, color)
            } else {
                // Wide letters: the tip takes the row and wraps in it.
                Container::<Message>::new(label.fill_width())
                    .fill_width()
                    .padding_xy(13.5, 7.5)
                    .radius(17.0)
                    .border_dashed(1.5, color)
            }
        })
    };
    Column::new()
        .push(
            Row::new()
                .push(
                    stack![
                        caveat("tip", 24.0).color(YELLOW),
                        Icon::vector(TIP_ARROW)
                            .size(20.0)
                            .color(YELLOW)
                            .place(Alignment::TOP_LEFT)
                            .offset(30.1, 4.4),
                    ]
                    .size(51.5, 34.0)
                    .rotate(-3.0),
                )
                .push_maybe(tip(0, BLUE))
                .gap(8.0),
        )
        .push_maybe(tip(1, PURPLE))
        .gap(8.0)
        .fill_width()
}

/// A line of news above the answer bar, with "Retry" when sending again
/// would help.
fn notice_row(text: String, retry: Option<Message>) -> Container<Message> {
    Row::new()
        .push(
            outfit(text, 15.0)
                .line_height_dp(20.25)
                .color(SILVER)
                .fill_width(),
        )
        .push_maybe(retry.map(|message| {
            Container::new(outfit("Retry", 15.0).semibold().color(BLACK))
                .height(40.0)
                .center()
                .padding_xy(16.0, 0.0)
                .radius(999.0)
                .background(YELLOW)
                .on_press(message)
                .id("retry")
        }))
        .gap(12.0)
        .cross_align(CrossAlign::Center)
        .fill_width()
        .padding_ltrb(MARGIN, 12.0, MARGIN, 0.0)
}

/// "0:07".
fn seconds(seconds: u32) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// The learner's side of the call, fixed above the call bar: what is
/// happening, and the field or the microphone to answer with.
fn answer(call: &OnlineCall) -> Container<Message> {
    let caller = call.caller.to_uppercase();
    let status = match call.phase {
        Phase::Ending => "CALL ENDED \u{b7} WRITING YOUR REPORT\u{2026}".to_owned(),
        Phase::EndFailed => "CALL ENDED".to_owned(),
        Phase::Sending { .. } => format!("SENT \u{b7} {caller} IS THINKING\u{2026}"),
        Phase::Starting => "STARTING THE MICROPHONE\u{2026}".to_owned(),
        Phase::Recording(time) => format!(
            "RECORDING \u{b7} {} OF {}",
            seconds(time),
            seconds(RECORDING_LIMIT)
        ),
        Phase::Stopping => "SENDING\u{2026}".to_owned(),
        Phase::Idle if call.sound == Sound::Speaking => format!("{caller} IS SPEAKING\u{2026}"),
        Phase::Idle if call.speaks() => "YOU \u{b7} TAP TO SPEAK".to_owned(),
        Phase::Idle => "YOU \u{b7} TYPE YOUR ANSWER".to_owned(),
    };
    Column::new()
        .push(small_caps(status))
        .push_maybe((!call.is_over()).then(|| {
            if call.speaks() {
                talk(call)
            } else {
                reply_field(call)
            }
        }))
        .gap(10.0)
        .fill_width()
        .padding_ltrb(MARGIN, 10.0, MARGIN, 10.0)
}

/// The reply as text: a field that grows with what is typed, and a send
/// button. In a spoken call a small microphone leads back to speaking.
fn reply_field(call: &OnlineCall) -> Container<Message> {
    let editable = call.can_type();
    let mut input = TextInput::new(&call.draft)
        .style(
            outfit("", 17.0)
                .line_height_dp(22.95)
                .color(if editable { WHITE } else { GREY }),
        )
        .placeholder("Type what you would say")
        .multiline()
        .max_length(REPLY_LIMIT)
        .capitalization(Capitalization::Sentences)
        .action(ImeAction::Send)
        .on_submit(Message::Send)
        .id("reply-input");
    if editable {
        // Without `on_input` a field cannot change: while a turn is with
        // the caller, what was typed is held as it was sent.
        input = input.on_input(Message::Draft);
    }
    // The field grows with what is typed. A view cannot measure text, so
    // a reply that is clearly more than three lines is shown through a
    // window of three with its end in view (where the caret is while
    // typing), rather than growing until the bar leaves the screen.
    let narrow = call.voice && !call.mic_denied;
    let long = call.draft.chars().count() > if narrow { 44 } else { 66 }
        || call.draft.matches('\n').count() >= 3;
    let field: Element<Message> = if long {
        stack![input.place(Alignment::BOTTOM_LEFT)]
            .fill_width()
            .height(3.0 * 22.95)
            .clip()
            .into()
    } else {
        input.into()
    };
    let ready = call.can_send();
    let (plate, ink) = if ready {
        (YELLOW, BLACK)
    } else {
        (QUIET, GREY)
    };
    Row::new()
        .push_maybe(narrow.then(|| {
            Container::new(Icon::vector(MICROPHONE).size(20.0).color(WHITE))
                .size(48.0, 48.0)
                .center()
                .radius(24.0)
                .background(QUIET)
                .on_press_maybe(editable.then_some(Message::SpeakInstead))
                .id("speak-instead")
        }))
        .push(
            Row::new()
                .push(field)
                .fill_width()
                .min_height(48.0)
                .cross_align(CrossAlign::Center)
                .padding_xy(14.0, 12.0)
                .radius(14.0)
                .border(2.0, QUIET)
                .focused_border(2.0, YELLOW),
        )
        .push(
            Container::new(Icon::vector(TIP_ARROW).size(20.0).color(ink))
                .size(48.0, 48.0)
                .center()
                .radius(24.0)
                .background(plate)
                .on_press_maybe(ready.then_some(Message::Send))
                .id("reply-send"),
        )
        .gap(10.0)
        .cross_align(CrossAlign::End)
        .fill_width()
}

/// The reply as speech: a large microphone, tapped to start and tapped
/// again to send, what it is doing beside it, and "Aa" to type instead.
fn talk(call: &OnlineCall) -> Container<Message> {
    let ready = call.mic_ready();
    let recording = matches!(call.phase, Phase::Recording(_));
    let (plate, ink) = match (recording, ready) {
        (true, _) => (RED, WHITE),
        (false, true) => (YELLOW, BLACK),
        (false, false) => (QUIET, GREY),
    };
    let (headline, hint): (String, &'static str) = match call.phase {
        Phase::Recording(time) => (format!("Recording {}", seconds(time)), "Tap again to send."),
        Phase::Starting => (
            "One moment\u{2026}".to_owned(),
            "Asking for the microphone.",
        ),
        Phase::Stopping => ("Sending\u{2026}".to_owned(), "Your answer is on its way."),
        Phase::Sending { .. } => (
            format!("{} is thinking\u{2026}", call.caller),
            "Your answer is with the caller.",
        ),
        _ if call.sound == Sound::Speaking => (
            format!("{} is speaking", call.caller),
            "Listen, then answer.",
        ),
        _ if call.kept().is_some() => ("Not sent yet".to_owned(), "Retry, or tap to say it again."),
        _ => (
            "Tap to speak".to_owned(),
            "Up to 60 seconds. Tap again to send.",
        ),
    };
    let idle = call.phase == Phase::Idle;
    row![
        Container::new(Icon::vector(MICROPHONE).size(32.0).color(ink))
            .size(72.0, 72.0)
            .center()
            .radius(36.0)
            .background(plate)
            .on_press_maybe(ready.then_some(Message::Talk))
            .id("talk"),
        col![
            outfit(headline, 19.0).semibold().max_lines(1).fill_width(),
            outfit(hint, 15.0).color(MUTED).fill_width(),
        ]
        .gap(2.0)
        .fill_width(),
        Container::new(
            outfit("Aa", 17.0)
                .semibold()
                .color(if idle { WHITE } else { GREY })
        )
        .size(48.0, 48.0)
        .center()
        .radius(24.0)
        .border(2.0, QUIET)
        .on_press_maybe(idle.then_some(Message::TypeInstead))
        .id("type-instead"),
    ]
    .gap(14.0)
    .cross_align(CrossAlign::Center)
    .fill_width()
}

/// The call bar of a call the server plays: who is on the line, the sound
/// button (white while the sound is off) and the red button.
fn bar(caller: String, title: String, detail: String, muted: bool) -> Container<Message> {
    let (plate, ink) = if muted {
        (WHITE, BLACK)
    } else {
        (QUIET, WHITE)
    };
    row![
        Picture::vector(AVATAR_DISC).size(54.0, 54.0),
        col![
            outfit(format!("{caller} \u{b7} {title}"), 19.0)
                .semibold()
                .max_lines(1)
                .fill_width(),
            outfit(detail, 15.0).color(MUTED).max_lines(1).fill_width(),
        ]
        .gap(2.0)
        .fill_width(),
        Container::new(Icon::vector(SPEAKER).size(22.0).color(ink))
            .size(52.0, 52.0)
            .center()
            .radius(26.0)
            .background(plate)
            .on_press(Message::ToggleMute)
            .id("mute"),
        Container::new(Icon::vector(HANDSET).size(24.0).color(WHITE).rotate(135.0))
            .size(52.0, 52.0)
            .center()
            .radius(26.0)
            .background(RED)
            .on_press(Message::EndCall)
            .id("end-call"),
    ]
    .gap(14.0)
    .cross_align(CrossAlign::Center)
    .fill_width()
    .padding_ltrb(MARGIN, 12.0, MARGIN, 16.0)
    .background(SURFACE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_avatar_is_drawable() {
        AVATAR_DISC.check().expect("picture parses");
    }

    #[test]
    fn the_accent_colour_ends_after_a_clause_or_two_words() {
        let lead = |line: &'static str| &line[..lead_of(line)];
        assert_eq!(lead("Fine. But I want it fixed today."), "Fine.");
        assert_eq!(
            lead("This is the third time I\u{2019}ve called today!"),
            "This is"
        );
        assert_eq!(lead("Hello?"), "Hello?");
        assert_eq!(lead("Well"), "Well");
        assert_eq!(lead(""), "");
        assert_eq!(lead("\u{c9}t\u{e9}, alors"), "\u{c9}t\u{e9},");
    }
}
