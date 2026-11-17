//! The first sample a reader sees: a whole application.

// docs:begin app
use morningstar::ui::prelude::*;

/// The application's state.
#[derive(Default)]
pub struct Counter {
    pub count: u32,
}

/// What can happen to it.
#[derive(Clone)]
pub enum Message {
    Tapped,
}

impl Application for Counter {
    type Message = Message;

    // What the screen looks like, as a function of the state.
    fn view(&self) -> Element<Message> {
        SafeArea::new(
            col![
                text(format!("Tapped {} times", self.count)).title(),
                Button::new("Tap me").on_press(Message::Tapped).fill_width(),
            ]
            .gap(16.0)
            .padding(24.0)
            .fill_width(),
        )
        .into()
    }

    // The only place the state changes.
    fn update(&mut self, message: Message) {
        match message {
            Message::Tapped => self.count += 1,
        }
    }
}
// docs:end app

// docs:begin register
// The one line that makes the crate an app: the hosts call into it.
morningstar::app!(Ui::new(Counter::default()));
// docs:end register
