//! Screen 13: the server. The app's own design, not from the design file.
//!
//! Where the CallReady server is, typed in a field and remembered; whether
//! it can be reached and how it plays calls ("connected · text mode",
//! "connected · voice", or "unreachable" and why, in plain words); and how
//! to run one beside a phone in development. "Check connection" asks the
//! server again, taking the address in the field; "Use the default" puts
//! the development address back.

use morningstar::ui::prelude::*;

use crate::Screen;
use crate::api::DEFAULT_BASE_URL;
use crate::components::{
    body, handwritten, outline_button, page, page_header, page_title, primary_button,
    pushed_screen, small_caps,
};
use crate::model::{Link, Model};
use crate::theme::{GREY, QUIET, SILVER, TEAL, YELLOW, outfit};

/// What the screen can ask for.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// The back arrow.
    Back,
    /// The address field changed: the whole new text.
    Url(String),
    /// "Check connection", or the keyboard's Go on the address: use the
    /// address in the field and ask the server how it is.
    Check,
    /// "Use the default": the development address, checked at once.
    Reset,
}

/// The longest address the field takes, in characters.
pub const URL_LIMIT: usize = 200;

/// The whole screen.
pub fn view(model: &Model) -> Element<Message> {
    let field = Row::new()
        .push(
            TextInput::new(&model.server_draft)
                .style(outfit("", 17.0))
                .placeholder(DEFAULT_BASE_URL)
                .max_length(URL_LIMIT)
                .no_autocorrect()
                .action(ImeAction::Go)
                .on_input(Message::Url)
                .on_submit(Message::Check)
                .id("server-url"),
        )
        .fill_width()
        .height(52.0)
        .cross_align(CrossAlign::Center)
        .padding_xy(16.0, 0.0)
        .radius(14.0)
        .border(2.0, QUIET)
        .focused_border(2.0, YELLOW);
    let (status, color) = match (&model.server_problem, &model.link) {
        (Some(problem), _) => ((*problem).to_owned(), YELLOW),
        (None, link @ Link::Online { .. }) => (link.status(), TEAL),
        (None, link @ (Link::Unknown | Link::Checking)) => (link.status(), SILVER),
        (None, link) => (link.status(), YELLOW),
    };
    let checking = model.link == Link::Checking;
    let content = col![
        page_header(Message::Back, "Server"),
        Spacer::fixed(14.0),
        page_title("Your server"),
        Spacer::fixed(2.0),
        handwritten(
            if model.link.is_online() {
                "calls are live"
            } else {
                "calls are the script"
            },
            28.0,
            -3.0
        ),
        Spacer::fixed(22.0),
        small_caps("ADDRESS"),
        Spacer::fixed(8.0),
        field,
        Spacer::fixed(14.0),
        small_caps("CONNECTION"),
        Spacer::fixed(6.0),
        outfit(status, 17.0)
            .line_height_dp(22.95)
            .color(color)
            .fill_width(),
        Spacer::fixed(20.0),
        primary_button(
            if checking {
                "Checking\u{2026}"
            } else {
                "Check connection"
            },
            17.0,
            56.0,
            Message::Check
        )
        .id("server-check"),
        Spacer::fixed(10.0),
        outline_button("Use the default", Message::Reset)
            .fill_width()
            .height(52.0)
            .id("server-default"),
        Spacer::fixed(24.0),
        body(
            "With a server, practice calls are conversations with an AI caller: \
             typed when the server has no voice, spoken when it has. Without \
             one they are the practice script on this phone."
        ),
        Spacer::fixed(12.0),
        body(
            "In development the server runs on your computer (npm start in \
             examples/callready/server) and the phone reaches it over USB with: \
             adb reverse tcp:8787 tcp:8787"
        )
        .color(GREY),
    ];
    pushed_screen(Screen::Server, page(content), None)
}
