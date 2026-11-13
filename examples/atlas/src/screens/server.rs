//! The server. **Not from the design**; built from the sign-in page's
//! field, the Plans page's top bar and small-caps headings, and the
//! design's two buttons.
//!
//! Where the Atlas server is, typed in a field and remembered on the
//! phone; whether it answers, in plain words; and how to run one beside a
//! phone in development. "Check connection" asks the server again, taking
//! the address in the field; "Use the default address" puts the
//! development address back and asks that.

use morningstar::ui::prelude::*;

use crate::api::DEFAULT_BASE_URL;
use crate::components::{
    MARGIN, TOP, outlined_button, page, primary_button, section_heading, text_field, top_bar,
};
use crate::icons;
use crate::theme::{ACCENT, INK, LINE_12, MUTED, QUIET, jakarta};
use crate::{Atlas, Link, Message, Screen, URL_LIMIT};

/// The screen.
pub fn view(app: &Atlas) -> Element<Message> {
    let link = app.link();
    // `height: 56px; border-radius: 16px; border: 1px solid
    // rgba(29,24,48,0.12)`: the sign-in page's field.
    let field = text_field(
        &icons::GLOBE,
        TextInput::new(app.server_draft())
            .placeholder(DEFAULT_BASE_URL)
            .max_length(URL_LIMIT)
            .no_autocorrect()
            .action(ImeAction::Go)
            .on_input(Message::ServerUrl)
            .on_submit(Message::CheckServer)
            .semantic_label("Server address")
            .id("server-url"),
        56.0,
        16.0,
        LINE_12,
    )
    .input_frame()
    .id("server-url-field");
    // A problem with the address comes before what the last check found.
    let (dot, status) = match (app.server_problem(), link) {
        (Some(problem), _) => (QUIET, problem.to_owned()),
        (None, Link::Online { .. }) => (ACCENT, link.status()),
        (None, _) => (QUIET, link.status()),
    };
    let content = col![
        section_heading("Address"),
        field,
        Spacer::fixed(10.0),
        section_heading("Connection"),
        row![
            // The home screen's mark: accent when answers are live.
            Container::new(Container::empty().size(7.0, 7.0).radius(3.5).background(dot))
                .padding_ltrb(0.0, 8.0, 0.0, 0.0),
            jakarta(status, 15.0).line_height_dp(22.5).color(INK).fill_width(),
        ]
        .gap(10.0)
        .cross_align(CrossAlign::Start)
        .fill_width()
        .padding_xy(4.0, 0.0)
        .merge_semantics()
        .live(Live::Polite)
        .id("server-status"),
        Spacer::fixed(10.0),
        primary_button(
            if *link == Link::Checking {
                "Checking\u{2026}"
            } else {
                "Check connection"
            },
            Message::CheckServer
        )
        .id("server-check"),
        outlined_button("Use the default address", Message::DefaultServer).id("server-default"),
        Spacer::fixed(10.0),
        // `padding: 0 4px; font-size: 12px; line-height: 1.55; color:
        // #5E5873`: the Plans page's note.
        Container::new(
            col![
                jakarta(
                    "With its server, Atlas answers with a real model, keeps your chats and, if the server has a voice, hears you and speaks. Without it every answer is scripted and live voice is a demo.",
                    12.0
                )
                .line_height_dp(18.6)
                .color(MUTED)
                .fill_width(),
                jakarta(
                    "In development the server runs on your computer (npm start in examples/atlas/server) and a phone reaches it over USB with: adb reverse tcp:8788 tcp:8788",
                    12.0
                )
                .line_height_dp(18.6)
                .color(MUTED)
                .fill_width(),
            ]
            .gap(10.0)
            .fill_width(),
        )
        .fill_width()
        .padding_xy(4.0, 0.0),
    ]
    .gap(12.0)
    .fill_width()
    .padding_ltrb(MARGIN, 0.0, MARGIN, 24.0);
    let body = col![
        Container::new(top_bar("Server", None))
            .fill_width()
            .padding_ltrb(10.0, TOP, 10.0, 16.0),
        ScrollView::new(content).id(Screen::Server.scroll_id()),
    ]
    .fill();
    page(Screen::Server, None, body).into()
}
