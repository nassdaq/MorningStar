//! "All models and providers". **Not from the design**, which only has the
//! row that leads here; built from the Plans page's parts: its top bar,
//! its small-caps heading and its list card, with the picker's rows.
//!
//! The four models are the picker's, and choosing one here is the same
//! choice. The providers are invented and switch nothing on. Under them is
//! the way to the Server screen, with how the app stands with its server,
//! and a note that says truthfully where answers come from right now.

use morningstar::ui::prelude::*;

use crate::components::{
    MARGIN, TOP, chevron, list_card, list_row, page, radio, section_heading, top_bar, two_lines,
};
use crate::theme::{MUTED, RADIUS_CARD, jakarta};
use crate::{Atlas, Link, Message, Mode, PROVIDERS, Screen};

/// Where answers come from right now, and what choosing a model does.
fn note(link: &Link) -> &'static str {
    match link {
        Link::Online { .. } => {
            "Answers come from a real model through this app\u{2019}s server. Fast is quick, Smart thinks first, Code is tuned for programming, and Auto picks one of the three for each message. The providers below the models are a design only."
        }
        Link::Offline(_) => {
            "The server is not answering, so every answer is scripted, whichever of these you choose. It is asked again when you go back to Home."
        }
        Link::Standalone | Link::Checking => {
            "Without its server this app has no model behind it: every answer is scripted, whichever of these you choose."
        }
    }
}

/// The screen.
pub fn view(app: &Atlas) -> Element<Message> {
    let modes = Mode::ALL
        .into_iter()
        .enumerate()
        .map(|(index, mode)| {
            let chosen = mode == app.mode();
            list_row(
                row![radio(chosen), two_lines(mode.name(), mode.detail())],
                64.0,
                14.0,
                14.0,
                index == 3,
            )
            .on_press(Message::SelectMode(mode))
            .role(Role::Radio)
            .checked(chosen)
            .id(("pick", mode))
        })
        .collect();
    let on = app.providers();
    let providers = PROVIDERS
        .into_iter()
        .enumerate()
        .map(|(index, (name, detail))| {
            list_row(
                row![two_lines(name, detail), radio(on[index])],
                64.0,
                14.0,
                14.0,
                index == 2,
            )
            .on_press(Message::ToggleProvider(index))
            .role(Role::Switch)
            .checked(on[index])
            .id(("provider", index))
        })
        .collect();
    let content = col![
        section_heading("Models"),
        list_card(modes, RADIUS_CARD),
        Spacer::fixed(10.0),
        section_heading("Providers"),
        list_card(providers, RADIUS_CARD),
        Spacer::fixed(10.0),
        section_heading("Server"),
        list_card(
            vec![
                list_row(
                    row![two_lines("Server", app.link().status()), chevron()],
                    64.0,
                    14.0,
                    14.0,
                    true,
                )
                .padding_ltrb(14.0, 10.0, 14.0, 10.0)
                .on_press(Message::OpenServer)
                .hint("Opens the server\u{2019}s address and its connection")
                .id("server")
            ],
            RADIUS_CARD
        ),
        Container::new(
            jakarta(note(app.link()), 12.0)
                .line_height_dp(18.6)
                .color(MUTED)
                .fill_width()
                .id("models-note"),
        )
        .fill_width()
        .padding_xy(4.0, 0.0),
    ]
    .gap(12.0)
    .fill_width()
    .padding_ltrb(MARGIN, 0.0, MARGIN, 24.0);
    let body = col![
        Container::new(top_bar("Models", None))
            .fill_width()
            .padding_ltrb(10.0, TOP, 10.0, 16.0),
        ScrollView::new(content).id(Screen::Models.scroll_id()),
    ]
    .fill();
    page(Screen::Models, None, body).into()
}
