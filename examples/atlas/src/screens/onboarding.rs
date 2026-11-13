//! Screen 01: the welcome. `design/html/01-onboarding.html`.
//!
//! The name, the orb in the middle of whatever room is left, the promise,
//! and two buttons that both lead to sign-in (both are `href="SignIn"` in
//! the design).

use morningstar::ui::prelude::*;

use crate::components::{TOP, orb, page, primary_button, text_button, wash_hero, wordmark};
use crate::theme::{MUTED, jakarta};
use crate::{Message, Screen};

/// `padding: 52px 24px 32px; display: flex; flex-direction: column`, over
/// `radial-gradient(circle at 50% 34%, rgba(246,170,214,0.32),
/// rgba(246,170,214,0) 48%), #FBF8FC`. `room` is the height the screen has.
pub fn view(room: f32) -> Element<Message> {
    let content = col![
        wordmark(),
        // `flex-grow: 1; display: flex; align-items: center;
        // justify-content: center` around the 176 dp orb.
        Container::new(orb(176.0))
            .center()
            .fill_width()
            .fill_height()
            .min_height(232.0),
        // `display: flex; flex-direction: column; align-items: center;
        // gap: 12px; text-align: center`.
        col![
            // `font-size: 30px; font-weight: 700; line-height: 1.15;
            // letter-spacing: -0.02em`.
            jakarta("Every AI, in one place", 30.0)
                .bold()
                .line_height_dp(34.5)
                .letter_spacing(-0.6)
                .centered()
                .fill_width()
                .heading(),
            // `max-width: 300px; font-size: 15px; line-height: 1.55;
            // color: #5E5873`.
            jakarta(
                "Ask by text, voice, camera or file. Atlas picks the right model for the job.",
                15.0
            )
            .line_height_dp(23.25)
            .color(MUTED)
            .centered()
            .fill_width()
            .max_width(300.0),
        ]
        .gap(12.0)
        .cross_align(CrossAlign::Center)
        .fill_width(),
        // `margin-top: 32px; display: flex; flex-direction: column; gap: 4px`.
        Spacer::fixed(32.0),
        col![
            primary_button("Get started", Message::GetStarted).id("get-started"),
            text_button("I already have an account", Message::GetStarted).id("have-account"),
        ]
        .gap(4.0)
        .fill_width(),
    ]
    .fill_width()
    .min_height(room)
    .padding_ltrb(24.0, TOP, 24.0, 32.0);
    page(
        Screen::Onboarding,
        Some(wash_hero()),
        ScrollView::new(content).id(Screen::Onboarding.scroll_id()),
    )
    .into()
}
