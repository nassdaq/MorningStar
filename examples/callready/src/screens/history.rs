//! Screen 08: call history, the third tab. The app's own design, not from
//! the design file.
//!
//! Every finished call, newest first, each with its score, scenario, day
//! and length. A row opens that call's report.

use morningstar::ui::prelude::*;

use crate::components::{body, call_row, hairline, handwritten, page, page_title, tab_screen};
use crate::model::Model;
use crate::theme::{SILVER, outfit};
use crate::{Screen, Tab};

/// What the screen can ask for.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// An item of the tab bar.
    Tab(Tab),
    /// A past call, by position in the history: its report.
    Open(usize),
}

/// The whole screen.
pub fn view(model: &Model) -> Element<Message> {
    let count = model.history.len();
    let mut content = col![
        page_title("Call history"),
        Spacer::fixed(6.0),
        row![
            outfit(
                format!(
                    "{count} call{} \u{b7} average {}",
                    if count == 1 { "" } else { "s" },
                    model.list_average()
                ),
                17.0
            )
            .color(SILVER),
            handwritten("tap one", 26.0, -5.0),
        ]
        .space_between()
        .cross_align(CrossAlign::Center)
        .fill_width(),
        Spacer::fixed(10.0),
    ];
    if count == 0 {
        content = content.push(Spacer::fixed(8.0)).push(body(
            "No calls yet. Start one from home: its report will be here.",
        ));
    }
    for (index, report) in model.history.iter().enumerate() {
        let degrees = if index % 2 == 0 { -4.0 } else { 3.0 };
        content = content
            .push_maybe((index > 0).then(hairline))
            .push(call_row(report, degrees, Message::Open(index)).id(("history-call", index)));
    }
    tab_screen(Screen::History, page(content), Tab::History, Message::Tab)
}
