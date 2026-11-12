//! Screen 10: the transcript of a finished call. The app's own design, not
//! from the design file.
//!
//! Every exchange that was answered: the customer's line and the learner's
//! sentence, the words their tip chose in teal when it was the good answer
//! and in the accent colour, with the better tip named, when it was not.
//!
//! A call the server played is shown as the server kept it: each line as
//! it was said, and under a learner's line the trainer's note, when there
//! is one, in the handwriting the app's remarks are made in.

use morningstar::ui::prelude::*;

use crate::Screen;
use crate::api::Spoken;
use crate::components::{
    body, hairline, handwritten, page, page_header, page_title, pushed_screen, small_caps,
};
use crate::model::{Detail, Report, ServerReport};
use crate::script::Exchange;
use crate::theme::{MUTED, SILVER, TEAL, WHITE, YELLOW, outfit, outfit_rich};

/// What the screen can ask for.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// The back arrow.
    Back,
}

/// The whole screen.
pub fn view(report: &Report) -> Element<Message> {
    let mut content = col![
        page_header(Message::Back, report.caption()),
        Spacer::fixed(14.0),
        page_title("Transcript"),
        Spacer::fixed(6.0),
        outfit(
            match report.score() {
                Some(score) => format!(
                    "{} \u{b7} {} \u{b7} {score}/100",
                    report.caller(),
                    report.day()
                ),
                None => format!(
                    "{} \u{b7} {} \u{b7} not scored",
                    report.caller(),
                    report.day()
                ),
            },
            16.0
        )
        .color(MUTED),
        Spacer::fixed(8.0),
    ];
    if let Some(server) = &report.server {
        return pushed_screen(Screen::Transcript, page(spoken(content, server)), None);
    }
    let mut answered = 0;
    for (exchange, option) in report.answered() {
        content = content
            .push_maybe((answered > 0).then(hairline))
            .push(lines(exchange, option));
        answered += 1;
    }
    let left = report.scenario.script().len() - answered;
    if left > 0 {
        content = content
            .push_maybe((answered > 0).then(hairline))
            .push(Spacer::fixed(18.0))
            .push(body(match (answered, left) {
                (0, _) => "The call ended before a line was spoken.".to_owned(),
                (_, 1) => "The call ended here, with 1 line still to answer.".to_owned(),
                (_, left) => {
                    format!("The call ended here, with {left} lines still to answer.")
                }
            }));
    }
    pushed_screen(Screen::Transcript, page(content), None)
}

/// The lines of a call the server played, or where they are.
fn spoken(mut content: Container<Message>, server: &ServerReport) -> Container<Message> {
    if server.transcript.is_empty() {
        let note = match &server.detail {
            Detail::Loaded => "Nothing was said on this call.".to_owned(),
            Detail::Failed(reason) => format!("The transcript could not be loaded. {reason}"),
            Detail::Summary | Detail::Loading => "Loading the transcript\u{2026}".to_owned(),
        };
        return content.push(Spacer::fixed(18.0)).push(body(note));
    }
    for (index, line) in server.transcript.iter().enumerate() {
        content = content
            .push_maybe((index > 0 && line.caller).then(hairline))
            .push(said(line));
    }
    content
}

/// One line as it was said, and the trainer's note on it.
fn said(line: &Spoken) -> Container<Message> {
    Column::new()
        .push(small_caps(if line.caller { "CUSTOMER" } else { "YOU" }))
        .push(Spacer::fixed(4.0))
        .push(
            outfit(line.text.clone(), 19.0)
                .line_height_dp(25.65)
                .color(if line.caller { SILVER } else { WHITE })
                .fill_width(),
        )
        .push_maybe(line.note.as_ref().map(|note| {
            Container::new(handwritten(note.clone(), 24.0, -1.5)).padding_ltrb(0.0, 6.0, 0.0, 0.0)
        }))
        .fill_width()
        .padding_ltrb(0.0, if line.caller { 18.0 } else { 4.0 }, 0.0, 14.0)
}

/// One exchange: what the customer said and what the learner answered.
fn lines(exchange: &Exchange, option: usize) -> Container<Message> {
    let choice = exchange.options[option];
    let better = exchange.options[exchange.good_option()];
    col![
        small_caps("CUSTOMER"),
        Spacer::fixed(4.0),
        outfit(exchange.customer(), 19.0)
            .line_height_dp(25.65)
            .color(SILVER)
            .fill_width(),
        Spacer::fixed(14.0),
        small_caps("YOU"),
        Spacer::fixed(4.0),
        outfit_rich(
            [
                span(format!("{} ", exchange.reply)),
                span(choice.words).color(if choice.good { TEAL } else { YELLOW }),
            ],
            19.0,
        )
        .line_height_dp(25.65)
        .fill_width(),
        Spacer::fixed(8.0),
        if choice.good {
            outfit(format!("\u{2713} {}", choice.tip), 15.0).color(TEAL)
        } else {
            outfit(format!("Stronger: {}", better.tip), 15.0).color(MUTED)
        }
        .fill_width(),
    ]
    .fill_width()
    .padding_xy(0.0, 18.0)
}
