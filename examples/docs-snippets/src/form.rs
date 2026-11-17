//! Samples for text input: a field, its frame, and the keyboard.

use morningstar::ui::prelude::*;

/// A sign-in form.
#[derive(Default)]
pub struct SignIn {
    pub phone: String,
    pub password: String,
    pub submitted: bool,
}

#[derive(Clone)]
pub enum Message {
    Phone(String),
    Password(String),
    Submit,
}

impl SignIn {
    // docs:begin field
    fn phone_field(&self) -> Element<Message> {
        // The input is only its text. The box around it is the field:
        // its height, padding, border, and its look while focused.
        row![
            TextInput::new(self.phone.clone())
                .placeholder("0712 345 678")
                .keyboard(KeyboardType::Phone)
                .action(ImeAction::Next)
                .max_length(13)
                // The application owns the string: every change arrives
                // as a message, and `update` stores it.
                .on_input(Message::Phone)
                .semantic_label("Phone number")
                // Focus stays with the field when the screen changes.
                .id("phone")
                .fill_width(),
        ]
        .height(52.0)
        .cross_align(CrossAlign::Center)
        .padding_xy(16.0, 0.0)
        .radius(12.0)
        .border(1.0, ThemeColor::Divider)
        .focused_border(2.0, ThemeColor::Primary)
        // A tap anywhere in the box focuses the input inside it.
        .input_frame()
        .fill_width()
        .into()
    }
    // docs:end field

    // docs:begin secure
    fn password_field(&self) -> Element<Message> {
        row![
            TextInput::new(self.password.clone())
                .placeholder("Password")
                .secure(true)
                .no_autocorrect()
                .action(ImeAction::Done)
                .on_input(Message::Password)
                // The keyboard's action key.
                .on_submit(Message::Submit)
                .semantic_label("Password")
                .id("password")
                .fill_width(),
        ]
        .height(52.0)
        .cross_align(CrossAlign::Center)
        .padding_xy(16.0, 0.0)
        .radius(12.0)
        .border(1.0, ThemeColor::Divider)
        .focused_border(2.0, ThemeColor::Primary)
        .input_frame()
        .fill_width()
        .into()
    }
    // docs:end secure
}

impl Application for SignIn {
    type Message = Message;

    fn view(&self) -> Element<Message> {
        let ready = !self.phone.is_empty() && !self.password.is_empty();
        SafeArea::new(
            col![
                text("Sign in").title(),
                self.phone_field(),
                self.password_field(),
                Button::new("Continue")
                    .on_press_maybe(ready.then_some(Message::Submit))
                    .fill_width(),
            ]
            .gap(16.0)
            .padding(20.0)
            .fill_width(),
        )
        .into()
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::Phone(phone) => self.phone = phone,
            Message::Password(password) => self.password = password,
            Message::Submit => self.submitted = true,
        }
    }
}
