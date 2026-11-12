//! Screen 07: notifications. The app's own design, not from the design
//! file.
//!
//! Three notes on tilted cards, each leading somewhere: the latest report,
//! the progress tab, the offer. Opening this screen clears the bell's dot;
//! a note keeps its own dot until it has been opened.

use morningstar::ui::prelude::*;

use crate::Screen;
use crate::components::{
    body, handwritten, page, page_header, page_title, pushed_screen, small_caps,
};
use crate::model::{Model, Notification};
use crate::theme::{BLACK, QUIET, YELLOW, outfit};

/// What the screen can ask for.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// The back arrow.
    Back,
    /// A notification, by position: where it points.
    Open(usize),
}

/// The whole screen.
pub fn view(model: &Model) -> Element<Message> {
    let notifications = model.notifications();
    let unread = notifications.iter().filter(|note| !note.read).count();
    let mut content = col![
        page_header(Message::Back, "Notifications"),
        Spacer::fixed(14.0),
        page_title("What\u{2019}s new"),
        Spacer::fixed(2.0),
        handwritten(
            match unread {
                0 => "all caught up!".to_owned(),
                1 => "1 to read".to_owned(),
                count => format!("{count} to read"),
            },
            28.0,
            -3.0
        ),
        Spacer::fixed(14.0),
    ];
    for (index, notification) in notifications.iter().enumerate() {
        let degrees = if index % 2 == 0 { -0.8 } else { 0.8 };
        content = content
            .push(card(index, notification, degrees))
            .push(Spacer::fixed(14.0));
    }
    pushed_screen(Screen::Notifications, page(content), None)
}

/// One notification: a dot while unread, the headline, the detail, when.
fn card(index: usize, notification: &Notification, degrees: f32) -> Container<Message> {
    let (dot, outline) = if notification.read {
        (BLACK, QUIET)
    } else {
        (YELLOW, YELLOW)
    };
    row![
        // The bell's dot, 6 dp down to sit beside the headline.
        Container::<Message>::empty()
            .size(9.0, 9.0)
            .radius(4.5)
            .background(dot)
            .offset(0.0, 7.0),
        col![
            outfit(notification.title, 19.0).semibold().fill_width(),
            Spacer::fixed(4.0),
            body(notification.body.clone()),
            Spacer::fixed(8.0),
            small_caps(notification.when.to_uppercase()),
        ]
        .fill_width(),
    ]
    .gap(12.0)
    .fill_width()
    .padding(16.0)
    .radius(6.0)
    .border(2.0, outline)
    .rotate(degrees)
    .on_press(Message::Open(index))
    .id(("notification", index))
}
