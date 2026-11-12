//! Screen 04: the score after a call. `design/html/04-call-report.html`.
//!
//! The score as two cut-out digits with a handwritten note, five skills as
//! bars, two tilted cards of feedback, and two buttons at the bottom. The
//! buttons stay put; the rest scrolls on a phone shorter than the design's.
//!
//! It shows a [`Report`]: the design's own (82, with the design's wording),
//! the report of a scripted call, whose scores and feedback derive from
//! the answers given, or the server's report of a call it played, with the
//! server's scores and sentences. Back returns to where the learner came
//! from, "Transcript" opens the call line by line and "Practise again"
//! starts the same scenario afresh.
//!
//! Two states only a server's report has: a call ended before the learner
//! said anything has no score, and the screen says so instead of showing
//! one; a report opened from the history is its score until the rest has
//! been fetched, and says so if it cannot be.

use morningstar::ui::prelude::*;

use crate::Screen;
use crate::components::{
    back_button, body, handwritten, note_card, outline_button, page, page_title, primary_button,
    pushed_screen, score_block, skill_bars,
};
use crate::model::{Detail, Report};
use crate::theme::{MUTED, SILVER, TEAL, WHITE, YELLOW, outfit};

/// What the screen can ask for.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// The back arrow.
    Back,
    /// "Transcript": the call, line by line.
    Transcript,
    /// "Practise again": a new call of the same scenario.
    PractiseAgain,
    /// "Try again" under a report whose details could not be fetched.
    Reload,
}

/// `padding: 52px 24px 28px; display: flex; flex-direction: column`.
pub fn view(report: &Report) -> Element<Message> {
    let content = match (report.score(), report.has_skills()) {
        (Some(score), true) => col![
            header(report),
            Spacer::fixed(14.0),
            score_block("Call report", score, report.note()),
            Spacer::fixed(26.0),
            skill_bars(&report.scores),
            Spacer::fixed(24.0),
            feedback(report),
        ],
        // From the history: the score, and the rest on its way.
        (Some(score), false) => col![
            header(report),
            Spacer::fixed(14.0),
            score_block("Call report", score, report.note()),
            Spacer::fixed(26.0),
            details(report),
        ],
        (None, _) => col![
            header(report),
            Spacer::fixed(14.0),
            unscored(),
            Spacer::fixed(26.0),
            feedback(report),
        ],
    };
    // `margin-top: auto` on the buttons: they are the fixed footer.
    pushed_screen(Screen::CallReport, page(content), Some(actions()))
}

/// `display: flex; align-items: center; justify-content: space-between`.
fn header(report: &Report) -> Container<Message> {
    row![
        back_button(Message::Back),
        outfit(report.caption(), 15.0).color(MUTED),
    ]
    .space_between()
    .cross_align(CrossAlign::Center)
    .fill_width()
}

/// In the score's place for a call that ended before the learner said
/// anything: the server does not score such a call, and no number is made
/// up for it.
fn unscored() -> Container<Message> {
    col![
        outfit("Call report", 17.0).color(SILVER),
        Spacer::fixed(10.0),
        page_title("Not scored"),
        Spacer::fixed(6.0),
        handwritten("say a line next time", 30.0, -3.0),
        Spacer::fixed(14.0),
        body("The call ended before you said anything, so there is nothing to score. It is not counted in your history."),
    ]
    .fill_width()
}

/// In the bars' place while the rest of a report from the history is
/// being fetched, and when it could not be.
fn details(report: &Report) -> Container<Message> {
    let detail = report.server.as_ref().map(|server| &server.detail);
    match detail {
        Some(Detail::Failed(reason)) => col![
            body(format!(
                "The rest of this report could not be loaded. {reason}"
            )),
            Spacer::fixed(14.0),
            outline_button("Try again", Message::Reload)
                .size(140.0, 52.0)
                .id("report-reload"),
        ]
        .fill_width(),
        _ => col![body("Loading the rest of this report\u{2026}")].fill_width(),
    }
}

/// `display: grid; grid-template-columns: repeat(2, minmax(0, 1fr));
/// gap: 10px`: what went well and what to try, each card turned a little.
///
/// The body is `font-size: 15px; line-height: 1.35`.
fn feedback(report: &Report) -> Container<Message> {
    let (did_well, try_next) = if report.designed {
        let line = |line: &'static str| vec![outfit(line, 15.0).line_height_dp(20.25).fill_width()];
        (
            line("You apologised early and kept a calm tone."),
            line("Ask for the order number before explaining."),
        )
    } else {
        (
            vec![body(report.did_well()).color(WHITE)],
            vec![body(report.try_next()).color(WHITE)],
        )
    };
    row![
        note_card("DID WELL", TEAL, -1.5, did_well),
        note_card("TRY NEXT", YELLOW, 1.5, try_next),
    ]
    .gap(10.0)
    .fill_width()
}

/// `margin-top: auto; display: flex; gap: 10px`: `flex: 1` and `flex: 1.4`.
fn actions() -> Container<Message> {
    row![
        // `height: 60px; border-radius: 22px; border: 2px solid #3A3A3C;
        // font-size: 17px; font-weight: 600`. Not `border-box`: the border
        // adds to the height (64) and to the flex share (136.67 + 4).
        outline_button("Transcript", Message::Transcript)
            .size(140.67, 64.0)
            .id("transcript"),
        // `height: 60px; border-radius: 22px; background: #F6C445;
        // color: #0E0E0E; font-size: 17px; font-weight: 700`.
        primary_button("Practise again", 17.0, 60.0, Message::PractiseAgain).id("practise-again"),
    ]
    .gap(10.0)
    .fill_width()
}
