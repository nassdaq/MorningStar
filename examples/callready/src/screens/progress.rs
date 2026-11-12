//! Screen 11: progress, the second tab. The app's own design, not from the
//! design file.
//!
//! The average score of every finished call in the report's cut-out
//! digits, the five skills averaged as bars, and the latest call, which
//! opens its report.

use morningstar::ui::prelude::*;

use crate::components::{
    call_row, page, page_title, score_block, skill_bars, small_caps, tab_screen,
};
use crate::model::Model;
use crate::{Screen, Tab};

/// What the screen can ask for.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// An item of the tab bar.
    Tab(Tab),
    /// The latest call: its report.
    Latest,
}

/// The whole screen.
pub fn view(model: &Model) -> Element<Message> {
    let count = model.calls();
    let streak = model.streak();
    let (skills_of, skills) = model.skills();
    let content = Column::new()
        .push(page_title("Progress"))
        .push(Spacer::fixed(18.0))
        // The report's score block, for the average.
        .push(score_block(
            format!(
                "Average of {count} call{}",
                if count == 1 { "" } else { "s" }
            ),
            model.average(),
            match streak {
                0 => "call today!".to_owned(),
                1 => "1 day!".to_owned(),
                days => format!("{days} days!"),
            },
        ))
        .push(Spacer::fixed(26.0))
        // Online the bars are one call's, and say whose.
        .push_maybe(skills_of.map(small_caps))
        .push_maybe(skills_of.map(|_| Spacer::fixed(12.0)))
        .push(skill_bars(&skills))
        .push(Spacer::fixed(28.0))
        .push(small_caps("LATEST CALL"))
        .push_maybe(
            model
                .latest()
                .map(|report| call_row(report, -4.0, Message::Latest).id("progress-latest")),
        );
    tab_screen(Screen::Progress, page(content), Tab::Progress, Message::Tab)
}
