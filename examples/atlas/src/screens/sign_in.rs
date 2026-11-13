//! Screen 02: sign in. `design/html/02-sign-in.html`.
//!
//! There is no account system in this example: "Continue with Google"
//! leads home as the design's link does, and the form accepts any email
//! address that looks like one with a password of six characters or more,
//! saying what is wrong otherwise in the design's link style.

use morningstar::ui::prelude::*;

use crate::components::{orb_mark, outlined_button, page, primary_button, text_field};
use crate::icons;
use crate::theme::{ACCENT, LINE_12, LINE_16, MUTED, RADIUS_ROW, jakarta, jakarta_rich};
use crate::{Atlas, Message, Screen};

/// A label over its field: `display: flex; flex-direction: column; gap:
/// 8px`, the label `font-size: 13px; font-weight: 600`. The label is the
/// field's name for a screen reader.
fn labelled(label: &'static str, field: Container<Message>) -> Container<Message> {
    col![
        jakarta(label, 13.0).semibold().exclude_semantics(),
        field.input_frame().label(label),
    ]
    .gap(8.0)
    .fill_width()
}

/// `padding: 72px 24px 36px; display: flex; flex-direction: column` on
/// `#FBF8FC`.
pub fn view(app: &Atlas, room: f32) -> Element<Message> {
    let rule = || {
        Container::empty()
            .fill_width()
            .height(1.0)
            .background(LINE_12)
    };
    let email = TextInput::new(app.email())
        .placeholder("you@example.com")
        .keyboard(KeyboardType::Email)
        .capitalization(Capitalization::None)
        .no_autocorrect()
        .action(ImeAction::Next)
        .on_input(Message::Email)
        .id("email");
    let password = TextInput::new(app.password())
        .placeholder("Your password")
        .secure(true)
        .action(ImeAction::Done)
        .on_input(Message::Password)
        .on_submit(Message::SignIn)
        .id("password");
    let content = col![
        // `display: flex; flex-direction: column; gap: 10px`.
        col![
            orb_mark(40.0),
            // `margin: 14px 0 0; font-size: 30px; font-weight: 700;
            // letter-spacing: -0.02em`: 14 and the gap of 10.
            Spacer::fixed(24.0),
            jakarta("Welcome back", 30.0)
                .bold()
                .letter_spacing(-0.6)
                .heading(),
            Spacer::fixed(10.0),
            // `font-size: 15px; line-height: 1.5; color: #5E5873`.
            jakarta("Sign in to pick up where you left off.", 15.0)
                .line_height_dp(22.5)
                .color(MUTED),
        ]
        .fill_width(),
        // `margin-top: 32px`.
        Spacer::fixed(32.0),
        outlined_button("Continue with Google", Message::Google).id("google"),
        // `display: flex; align-items: center; gap: 12px; margin: 22px 0;
        // color: #5E5873; font-size: 13px`, a 1 dp rule either side.
        row![rule(), jakarta("or use email", 13.0).color(MUTED), rule()]
            .gap(12.0)
            .cross_align(CrossAlign::Center)
            .fill_width()
            .padding_xy(0.0, 22.0),
        // `display: flex; flex-direction: column; gap: 16px`.
        Column::new()
            .push(labelled(
                "Email",
                text_field(&icons::MAIL, email, 56.0, RADIUS_ROW, LINE_16).id("email-field"),
            ))
            .push(Spacer::fixed(16.0))
            .push(labelled(
                "Password",
                text_field(&icons::LOCK, password, 56.0, RADIUS_ROW, LINE_16).id("password-field"),
            ))
            // What is wrong with the form, where the eye is: the app's
            // own, in the design's link style.
            .push_maybe(app.sign_in_error().map(|error| {
                Container::new(
                    jakarta(error, 13.0)
                        .semibold()
                        .color(ACCENT)
                        .fill_width()
                        .live(Live::Polite),
                )
                .fill_width()
                .padding_ltrb(0.0, 10.0, 0.0, 0.0)
                .id("sign-in-error")
            }))
            // `align-self: flex-end; min-height: 44px; display: flex;
            // align-items: center; font-size: 13px; font-weight: 600;
            // margin-top: -8px` (16 - 8 after the field).
            .push(Spacer::fixed(8.0))
            .push(
                Container::new(jakarta("Forgot password?", 13.0).semibold().color(ACCENT))
                    .center()
                    .min_height(44.0)
                    .align_self(CrossAlign::End)
                    .on_press(Message::ForgotPassword)
                    .id("forgot"),
            )
            .push(Spacer::fixed(16.0))
            .push(primary_button("Sign in", Message::SignIn).id("sign-in"))
            .fill_width(),
        // `margin: auto 0 0; text-align: center; font-size: 14px; color:
        // #5E5873`, the link `font-weight: 700` in the accent. A tap
        // target inside a sentence does not exist: the line is the target.
        Spacer::new().min_height(16.0),
        Container::new(
            jakarta_rich(
                [
                    span("New to Atlas? ").color(MUTED),
                    span("Create an account").bold().color(ACCENT),
                ],
                14.0
            )
            .centered()
            .fill_width(),
        )
        .fill_width()
        .on_press(Message::CreateAccount)
        .id("create-account"),
    ]
    .fill_width()
    .min_height(room)
    .padding_ltrb(24.0, 72.0, 24.0, 36.0);
    page(
        Screen::SignIn,
        None,
        ScrollView::new(content).id(Screen::SignIn.scroll_id()),
    )
    .into()
}
