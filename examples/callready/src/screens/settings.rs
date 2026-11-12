//! Screen 09: scenario settings. The app's own design, not from the design
//! file.
//!
//! The caller's accent, how patient they are and how long a call should
//! take, each a row of chips. The choice applies to the next call: it
//! shows in the line under today's call on home and in the call bar, sets
//! the thin time bar of a call, and the difficulty sets what a weaker
//! answer costs.

use morningstar::ui::prelude::*;

use crate::Screen;
use crate::components::{
    body, handwritten, option_chip, page, page_header, page_title, pushed_screen, small_caps,
};
use crate::model::{Accent, Difficulty, Length, Settings};
use crate::theme::{MUTED, outfit};

/// What the screen can ask for.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// The back arrow.
    Back,
    /// An accent chip.
    Accent(Accent),
    /// A difficulty chip.
    Difficulty(Difficulty),
    /// A call-length chip.
    Length(Length),
}

/// The whole screen.
pub fn view(settings: &Settings) -> Element<Message> {
    let accents = Row::new()
        .extend(Accent::ALL.map(|accent| {
            option_chip(
                accent.label(),
                accent == settings.accent,
                Message::Accent(accent),
            )
            .id(("accent", accent))
        }))
        .gap(8.0);
    let difficulties = Row::new()
        .extend(Difficulty::ALL.map(|difficulty| {
            option_chip(
                difficulty.label(),
                difficulty == settings.difficulty,
                Message::Difficulty(difficulty),
            )
            .id(("difficulty", difficulty))
        }))
        .gap(8.0);
    let lengths = Row::new()
        .extend(Length::ALL.map(|length| {
            option_chip(
                match length {
                    Length::Two => "2 min",
                    Length::Four => "4 min",
                    Length::Six => "6 min",
                },
                length == settings.length,
                Message::Length(length),
            )
            .id(("length", length))
        }))
        .gap(8.0);
    let content = col![
        page_header(Message::Back, "Scenario settings"),
        Spacer::fixed(14.0),
        page_title("Your caller"),
        Spacer::fixed(6.0),
        // The line home shows under today's call.
        outfit(settings.summary(), 16.0).color(MUTED),
        Spacer::fixed(28.0),
        small_caps("CALLER ACCENT"),
        Spacer::fixed(12.0),
        accents,
        Spacer::fixed(28.0),
        small_caps("DIFFICULTY"),
        Spacer::fixed(12.0),
        difficulties,
        Spacer::fixed(10.0),
        body(settings.difficulty.description()),
        Spacer::fixed(28.0),
        small_caps("CALL LENGTH"),
        Spacer::fixed(12.0),
        lengths,
        Spacer::fixed(26.0),
        handwritten("used for your next call", 28.0, -3.0),
    ];
    pushed_screen(Screen::Settings, page(content), None)
}
