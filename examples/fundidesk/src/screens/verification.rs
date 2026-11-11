//! Screen 05: verification code. `design/html/05-verification.html`.
//!
//! The six digit boxes are static look-alikes showing the design's state:
//! two digits entered, the third box waiting.

use morningstar::ui::prelude::*;

use crate::components::{Action, CodeBox, code_box, form_screen, icon_button};
use crate::icons;
use crate::theme::{GREEN, INK, MUTED, WHITE, inter, inter_rich, page_title};
use crate::{Nav, Screen};

/// This screen is built from the design.
pub const BUILT: bool = true;

/// Number of digits in a code.
pub const CODE_LENGTH: usize = 6;

/// The digits entered so far, one per box. The default is what the design
/// shows: two digits in, the third box waiting.
#[derive(Debug)]
pub struct State {
    /// Each box's digit, or an empty string.
    pub digits: [String; CODE_LENGTH],
}

impl Default for State {
    fn default() -> Self {
        let mut digits: [String; CODE_LENGTH] = Default::default();
        digits[0] = "4".to_owned();
        digits[1] = "7".to_owned();
        Self { digits }
    }
}

impl State {
    /// The code as one string, with nothing for an empty box.
    pub fn code(&self) -> String {
        self.digits.concat()
    }
}

/// What the verification screen can ask for.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// The digit of one box changed (to a digit, or to nothing).
    Digit(usize, String),
    /// The back chevron.
    Back,
    /// "Verify".
    Verify,
    /// "Resend". The design gives it no destination.
    Resend,
}

/// The width of a space in Inter at 14 px (576 of 2048 units): the
/// whitespace between the sentence and the "Resend" button.
const WORD_SPACE: f32 = 14.0 * 576.0 / 2048.0;

/// `padding: 44px 24px 32px; background: #FFFFFF`, one column.
pub fn view(state: &State) -> Element<Message> {
    form_screen("verification", 44.0)
        // `width: 44px; height: 44px; margin-left: -10px`.
        .push(
            icon_button(icons::CHEVRON_LEFT, 24.0, INK)
                .offset(-10.0, 0.0)
                .on_press(Message::Back),
        )
        .push(Spacer::fixed(20.0))
        .push(
            col![
                page_title("Verification code"),
                // `font-size: 15px; line-height: 22px; color: #5B6660` with
                // the number in `color: #14201A; font-weight: 600`.
                inter_rich(
                    [
                        span("Enter the 6-digit code we sent by SMS or WhatsApp to "),
                        span("+255 7XX XXX 678").semibold().color(INK),
                        span("."),
                    ],
                    15.0,
                )
                .line_height_dp(22.0)
                .color(MUTED)
                .fill_width(),
            ]
            .gap(8.0)
            .fill_width(),
        )
        .push(Spacer::fixed(32.0))
        .push(code(state))
        .push(Spacer::fixed(32.0))
        .push(Action::primary("Verify").on_press(Message::Verify))
        .push(Spacer::fixed(24.0))
        .push(resend())
        .into()
}

/// `display: grid; grid-template-columns: repeat(6, minmax(0, 1fr)); gap:
/// 8px`. A box with a digit is drawn filled; the first empty one is the
/// active box (the design's third); the rest are empty. Each box is a
/// one-digit input: typing moves on to the next box, backspace in an empty
/// box goes back, so the six behave as one field.
fn code(state: &State) -> Container<Message> {
    let active = state.digits.iter().position(String::is_empty);
    Row::new()
        .extend(state.digits.iter().enumerate().map(|(index, digit)| {
            let look = if !digit.is_empty() {
                CodeBox::Filled
            } else if active == Some(index) {
                CodeBox::Active
            } else {
                CodeBox::Empty
            };
            code_box(digit, look, ("code", index), move |digit| {
                Message::Digit(index, digit)
            })
        }))
        .gap(8.0)
        .fill_width()
}

/// `font-size: 14px; color: #5B6660` followed by a button with `padding:
/// 12px 4px; font: 600 14px Inter; color: #1E8416`. The button sits on the
/// sentence's baseline, so the line is as tall as the button.
fn resend() -> Container<Message> {
    row![
        inter("Didn’t receive a code?", 14.0).color(MUTED),
        Container::new(inter("Resend", 14.0).semibold().color(GREEN))
            .padding_ltrb(WORD_SPACE + 4.0, 12.0, 4.0, 12.0)
            .on_press(Message::Resend)
            .pressed_background(WHITE),
    ]
    .cross_align(CrossAlign::Center)
}

/// Follows the design's links.
pub fn update(state: &mut State, message: Message) -> Nav {
    match message {
        Message::Digit(index, digit) => {
            if let Some(slot) = state.digits.get_mut(index) {
                *slot = digit;
            }
            Nav::Stay
        }
        Message::Back => Nav::To(Screen::CreateAccount),
        Message::Verify => Nav::To(Screen::ShopSetup),
        Message::Resend => Nav::Stay,
    }
}
