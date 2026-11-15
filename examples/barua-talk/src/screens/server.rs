//! The user's name and the server. **Not from the design**; built from a
//! room's header and message field, Home's small headings and the app's
//! two buttons. Reached from the user's picture on Home.
//!
//! The name other people see; where the Barua Talk server is, typed in a
//! field and remembered on the phone; whether it answers, in plain words;
//! and how to run one beside a phone in development.

use morningstar::ui::prelude::*;

use crate::api::{DEFAULT_BASE_URL, NAME_LIMIT};
use crate::components::{
    MARGIN, field, note, outlined_button, page, primary_button, problem, section, top_bar,
};
use crate::theme::{ACCENT, INK, QUIET, inter};
use crate::{BaruaTalk, Link, Message, Screen, URL_LIMIT};

/// The screen.
pub fn view(app: &BaruaTalk) -> Element<Message> {
    let link = app.link();
    // A problem with the address comes before what the last check found.
    let (dot, status) = match (app.server_problem(), link) {
        (Some(wrong), _) => (QUIET, wrong.to_owned()),
        (None, Link::Online { .. }) => (ACCENT, link.status()),
        (None, _) => (QUIET, link.status()),
    };
    let identity = app
        .identity()
        .map(|identity| note(format!("Your address for calls: {identity}")).id("identity"));
    let content = Column::new()
        .push(section("Your name"))
        .push(
            field(
                TextInput::new(app.name_draft())
                    .placeholder("Neema")
                    .max_length(NAME_LIMIT)
                    .capitalization(Capitalization::Words)
                    .action(ImeAction::Done)
                    .on_input(Message::Name)
                    .on_submit(Message::SaveName)
                    .semantic_label("Your name")
                    .id("name"),
            )
            .input_frame()
            .id("name-field"),
        )
        .push_maybe(
            app.name_problem()
                .map(|wrong| problem(wrong).live(Live::Polite).id("name-problem")),
        )
        .push_maybe(identity)
        .push(outlined_button("Save name", Message::SaveName).id("save-name"))
        .push(Spacer::fixed(10.0))
        .push(section("Server address"))
        .push(
            field(
                TextInput::new(app.server_draft())
                    .placeholder(DEFAULT_BASE_URL)
                    .max_length(URL_LIMIT)
                    .no_autocorrect()
                    .action(ImeAction::Go)
                    .on_input(Message::ServerUrl)
                    .on_submit(Message::CheckServer)
                    .semantic_label("Server address")
                    .id("server-url"),
            )
            .input_frame()
            .id("server-url-field"),
        )
        .push(
            row![
                Container::new(Container::empty().size(8.0, 8.0).radius(4.0).background(dot))
                    .padding_ltrb(0.0, 6.0, 0.0, 0.0),
                inter(status, 14.0)
                    .line_height_dp(20.0)
                    .color(INK)
                    .fill_width(),
            ]
            .gap(10.0)
            .cross_align(CrossAlign::Start)
            .fill_width()
            .padding_xy(4.0, 4.0)
            .merge_semantics()
            .live(Live::Polite)
            .id("server-status"),
        )
        .push(
            primary_button(
                if *link == Link::Checking {
                    "Checking\u{2026}"
                } else {
                    "Check connection"
                },
                Message::CheckServer,
            )
            .id("server-check"),
        )
        .push(outlined_button("Use the default address", Message::DefaultServer).id("server-default"))
        .push(Spacer::fixed(10.0))
        .push(note(
            "With its server, rooms, messages and calls are real and reach other phones. Without it the app is a scripted demo of the design\u{2019}s own people. Live audio and video are not in this version either way: a call rings, is answered, timed and recorded, and carries no sound yet.",
        ))
        .push(note(
            "In development the server runs on your computer (npm start in examples/barua-talk/server) and a phone reaches it over USB with: adb reverse tcp:8789 tcp:8789",
        ))
        .gap(12.0)
        .fill_width()
        .padding_ltrb(MARGIN, 24.0, MARGIN, 24.0);
    let body = col![
        top_bar("You and the server"),
        ScrollView::new(content).id(Screen::Server.scroll_id()),
    ]
    .fill();
    page(Screen::Server, body).into()
}
