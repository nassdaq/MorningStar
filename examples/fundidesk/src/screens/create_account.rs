//! Screen 04: create account. `design/html/04-create-account.html`.
//!
//! Text fields are static look-alikes: the framework has no text input
//! yet, so the fields show their placeholders and do not take focus.

use morningstar::ui::prelude::*;

use crate::components::{Action, Field, form_screen, labelled, logo};
use crate::theme::{GREEN, MUTED, WHITE, inter_rich, lead, page_title};
use crate::{Nav, Screen};

/// This screen is built from the design.
pub const BUILT: bool = true;

/// What has been typed into the four fields.
#[derive(Debug, Default)]
pub struct State {
    /// "Full name".
    pub name: String,
    /// "Your WhatsApp number", without the `+255`.
    pub whatsapp: String,
    /// "Password".
    pub password: String,
    /// "Confirm password".
    pub confirmation: String,
}

/// What the create-account screen can ask for.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// The name field changed.
    Name(String),
    /// The WhatsApp number changed.
    Whatsapp(String),
    /// The password changed.
    Password(String),
    /// The confirmation changed.
    Confirmation(String),
    /// "Create account".
    CreateAccount,
    /// "Log in".
    LogIn,
}

/// `padding: 56px 24px 32px; background: #FFFFFF`, one column.
pub fn view(state: &State) -> Element<Message> {
    form_screen("create-account", 56.0)
        .push(logo(44.0, GREEN, WHITE))
        .push(Spacer::fixed(24.0))
        .push(
            col![
                page_title("Create account"),
                lead("Set up your owner account. You will add your shop next."),
            ]
            .gap(8.0)
            .fill_width(),
        )
        .push(Spacer::fixed(28.0))
        .push(fields(state))
        .push(Spacer::fixed(28.0))
        .push(Action::primary("Create account").on_press(Message::CreateAccount))
        // `margin: auto 0 0`: pushed to the bottom.
        .push(Spacer::new())
        .push(log_in())
        .into()
}

/// The four labelled fields: `gap: 16px`.
fn fields(state: &State) -> Container<Message> {
    col![
        labelled(
            "Full name",
            Field::new(&state.name)
                .hint("Enter your name")
                .input(|input| {
                    input
                        .capitalization(Capitalization::Words)
                        .action(ImeAction::Next)
                })
                .on_input(Message::Name)
                .id("account-name"),
        ),
        labelled(
            "Your WhatsApp number",
            Field::new(&state.whatsapp)
                .hint("7XX XXX XXX")
                .prefix("+255")
                .input(|input| {
                    input
                        .keyboard(KeyboardType::Phone)
                        .max_length(12)
                        .action(ImeAction::Next)
                })
                .on_input(Message::Whatsapp)
                .id("account-whatsapp"),
        ),
        labelled(
            "Password",
            Field::new(&state.password)
                .hint("At least 8 characters")
                .input(|input| input.secure(true).action(ImeAction::Next))
                .on_input(Message::Password)
                .id("account-password"),
        ),
        labelled(
            "Confirm password",
            Field::new(&state.confirmation)
                .hint("Enter password again")
                .input(|input| input.secure(true).on_submit(Message::CreateAccount))
                .on_input(Message::Confirmation)
                .id("account-confirmation"),
        ),
    ]
    .gap(16.0)
    .fill_width()
}

/// `text-align: center; font-size: 14px; color: #5B6660` with the link in
/// semibold green. A tap anywhere on the line follows the link: a tap
/// target cannot be part of a text.
fn log_in() -> Container<Message> {
    Container::new(
        inter_rich(
            [
                span("Already have an account? "),
                span("Log in").semibold().color(GREEN),
            ],
            14.0,
        )
        .color(MUTED)
        .centered()
        .fill_width(),
    )
    .fill_width()
    .on_press(Message::LogIn)
    .pressed_background(WHITE)
}

/// Follows the design's links.
pub fn update(state: &mut State, message: Message) -> Nav {
    match message {
        Message::Name(name) => {
            state.name = name;
            Nav::Stay
        }
        Message::Whatsapp(number) => {
            state.whatsapp = number;
            Nav::Stay
        }
        Message::Password(password) => {
            state.password = password;
            Nav::Stay
        }
        Message::Confirmation(confirmation) => {
            state.confirmation = confirmation;
            Nav::Stay
        }
        Message::CreateAccount => Nav::To(Screen::Verification),
        Message::LogIn => Nav::To(Screen::Login),
    }
}
