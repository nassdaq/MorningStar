//! Screen 03: log in. `design/html/03-login.html`.
//!
//! The phone number and the password are real text fields; the eye shows
//! or masks the password.

use morningstar::ui::prelude::*;

use crate::components::{Action, Field, form_screen, icon_button, labelled, link, logo};
use crate::icons;
use crate::theme::{GREEN, INK, MUTED, OUTLINE, PANEL, WHITE, inter, inter_rich, lead, page_title};
use crate::{Nav, Screen};

/// This screen is built from the design.
pub const BUILT: bool = true;

/// The login screen's state.
#[derive(Debug, Default)]
pub struct State {
    /// The phone number as typed.
    pub phone: String,
    /// The password as typed.
    pub password: String,
    /// Whether the password is shown as typed instead of as bullets. The
    /// eye toggles it.
    pub password_visible: bool,
}

/// What the login screen can ask for.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// The phone field changed.
    Phone(String),
    /// The password field changed.
    Password(String),
    /// The eye in the password field.
    TogglePassword,
    /// "Forgot password?"
    ForgotPassword,
    /// "Log in".
    LogIn,
    /// The fingerprint button. The design gives it no destination.
    Fingerprint,
    /// "Create an account".
    CreateAccount,
}

/// `padding: 56px 24px 32px; background: #FFFFFF`, one column.
pub fn view(state: &State) -> Element<Message> {
    form_screen("login", 56.0)
        .push(logo(44.0, GREEN, WHITE))
        .push(Spacer::fixed(28.0))
        .push(
            col![
                page_title("Welcome back"),
                lead("Log in to your FundiDesk shop account."),
            ]
            .gap(8.0)
            .fill_width(),
        )
        .push(Spacer::fixed(32.0))
        .push(fields(state))
        .push(Spacer::fixed(20.0))
        .push(Action::primary("Log in").on_press(Message::LogIn))
        .push(Spacer::fixed(32.0))
        .push(fingerprint())
        // `margin: auto 0 0`: pushed to the bottom.
        .push(Spacer::new())
        .push(sign_up())
        .into()
}

/// The two fields and the link under them: `gap: 18px`.
fn fields(state: &State) -> Container<Message> {
    let masked = !state.password_visible;
    col![
        labelled(
            "Phone number",
            Field::new(&state.phone)
                .hint("07XX XXX XXX")
                .leading(icons::PHONE)
                .input(|input| {
                    input
                        .keyboard(KeyboardType::Phone)
                        .max_length(16)
                        .action(ImeAction::Next)
                })
                .on_input(Message::Phone)
                .id("login-phone"),
        ),
        labelled(
            "Password",
            Field::new(&state.password)
                .hint("Enter password")
                .leading(icons::LOCK)
                // The eye is inside the field's frame: tapping it keeps the
                // keyboard up.
                .trailing(
                    icon_button(icons::EYE, 20.0, MUTED)
                        .on_press(Message::TogglePassword)
                        .id("login-eye"),
                )
                .input(|input| input.secure(masked).on_submit(Message::LogIn))
                .on_input(Message::Password)
                .id("login-password"),
        ),
        // `align-self: flex-start; padding: 6px 0`.
        link("Forgot password?", 6.0).on_press(Message::ForgotPassword),
    ]
    .gap(18.0)
    .fill_width()
}

/// `display: flex; flex-direction: column; align-items: center; gap: 14px`.
fn fingerprint() -> Container<Message> {
    col![
        inter("or unlock with your fingerprint", 13.0).color(MUTED),
        // `width: 68px; height: 68px; border-radius: 50%; border: 1px solid
        // #D9E0DB; background: #F3F5F3` around a 32 px icon.
        Container::new(Icon::vector(icons::FINGERPRINT).size(32.0).color(INK))
            .size(68.0, 68.0)
            .center()
            .radius(34.0)
            .background(PANEL)
            .border(1.0, OUTLINE)
            .on_press(Message::Fingerprint),
    ]
    .gap(14.0)
    .cross_align(CrossAlign::Center)
    .fill_width()
}

/// `text-align: center; font-size: 14px; color: #5B6660` with the link in
/// semibold green. A tap anywhere on the line follows the link: a tap
/// target cannot be part of a text.
fn sign_up() -> Container<Message> {
    Container::new(
        inter_rich(
            [
                span("New to FundiDesk? "),
                span("Create an account").semibold().color(GREEN),
            ],
            14.0,
        )
        .color(MUTED)
        .centered()
        .fill_width(),
    )
    .fill_width()
    .on_press(Message::CreateAccount)
    .pressed_background(WHITE)
}

/// Follows the design's links.
pub fn update(state: &mut State, message: Message) -> Nav {
    match message {
        Message::Phone(phone) => {
            state.phone = phone;
            Nav::Stay
        }
        Message::Password(password) => {
            state.password = password;
            Nav::Stay
        }
        Message::TogglePassword => {
            state.password_visible = !state.password_visible;
            Nav::Stay
        }
        Message::ForgotPassword => Nav::To(Screen::Verification),
        Message::LogIn => Nav::To(Screen::Dashboard),
        Message::Fingerprint => Nav::Stay,
        Message::CreateAccount => Nav::To(Screen::CreateAccount),
    }
}
