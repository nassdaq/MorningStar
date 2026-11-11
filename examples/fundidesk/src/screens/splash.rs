//! Screen 01: the splash screen. `design/html/01-splash.html`.
//!
//! The logo, the name and the tagline centred on the brand green.

use morningstar::ui::prelude::*;

use crate::components::logo_with_stroke;
use crate::theme::{GREEN, WHITE, grotesk, inter};
use crate::{Nav, Screen};

/// This screen is built from the design.
pub const BUILT: bool = true;

/// The splash screen remembers nothing.
#[derive(Debug, Default)]
pub struct State {}

/// What the splash screen can ask for.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// The screen was tapped. The design has no link here; a real app
    /// would move on by itself once it has loaded.
    Continue,
}

/// `display: flex; flex-direction: column; align-items: center;
/// justify-content: center; gap: 20px; padding: 40px; background: #1E8416`.
pub fn view(_state: &State) -> Element<Message> {
    col![
        // The one place the mark is drawn with a stroke of 5.
        logo_with_stroke(96.0, WHITE, GREEN, 5.0),
        col![
            // `font-size: 38px; font-weight: 700; letter-spacing: -0.5px`.
            grotesk("FundiDesk", 38.0).letter_spacing(-0.5).color(WHITE),
            inter("Your repair shop. Under control.", 16.0)
                .line_height_dp(24.0)
                .color(WHITE),
        ]
        .gap(8.0)
        .cross_align(CrossAlign::Center),
    ]
    .gap(20.0)
    .center()
    .padding(40.0)
    .background(GREEN)
    .on_press(Message::Continue)
    .pressed_background(GREEN)
    .id("splash")
    .into()
}

/// A tap leads to onboarding.
pub fn update(_state: &mut State, message: Message) -> Nav {
    match message {
        Message::Continue => Nav::To(Screen::Onboarding),
    }
}
