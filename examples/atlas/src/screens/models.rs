//! "All models and providers". **Not from the design**, which only has the
//! row that leads here; built from the Plans page's parts: its top bar,
//! its small-caps heading and its list card, with the picker's rows.
//!
//! The four models are the picker's, and choosing one here is the same
//! choice. The providers are invented and switch nothing on: this example
//! has no model behind it.

use morningstar::ui::prelude::*;

use crate::components::{
    MARGIN, TOP, list_card, list_row, page, radio, section_heading, top_bar, two_lines,
};
use crate::theme::{MUTED, RADIUS_CARD, jakarta};
use crate::{Atlas, Message, Mode, PROVIDERS, Screen};

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
        Container::new(
            jakarta(
                "This example has no model behind it: every answer is scripted, whichever of these you choose.",
                12.0
            )
            .line_height_dp(18.6)
            .color(MUTED)
            .fill_width(),
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
