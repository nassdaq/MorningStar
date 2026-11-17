//! Samples for screens, the system's Back, and time.

use std::time::Duration;

use morningstar::ui::prelude::*;

// docs:begin screens
/// The screens there are. Plain data: a screen can carry what it shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Screen {
    Orders,
    Order(u32),
}

#[derive(Clone)]
pub enum Message {
    Open(u32),
    Back,
    Second,
}

pub struct Shop {
    pub screens: Navigator<Screen>,
    pub seconds: u32,
}

impl Application for Shop {
    type Message = Message;

    fn view(&self) -> Element<Message> {
        let screen = *self.screens.current();
        let content: Element<Message> = match screen {
            Screen::Orders => Button::new("Order 1042")
                .on_press(Message::Open(1042))
                .into(),
            Screen::Order(number) => col![
                Button::new("Back").plain().on_press(Message::Back),
                text(format!("Order {number}")).title(),
                text(format!("Open for {} s", self.seconds)).muted(),
            ]
            .gap(12.0)
            .into(),
        };
        // The id gives each screen its own state; the transition slides
        // screens in the direction of the last change.
        SafeArea::new(content)
            .id(screen)
            .transition(self.screens.transition())
            .into()
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::Open(number) => {
                self.seconds = 0;
                self.screens.push(Screen::Order(number));
            }
            Message::Back => {
                self.screens.back();
            }
            Message::Second => self.seconds += 1,
        }
    }

    // The system's Back button or gesture. `true`: the app dealt with it.
    // `false` on the first screen leaves it to the system, which leaves
    // the app.
    fn back(&mut self) -> bool {
        self.screens.back()
    }

    // A message every second, only while an order is open. With `None`
    // the app draws nothing and wakes for nothing.
    fn tick(&self) -> Option<(Duration, Message)> {
        matches!(self.screens.current(), Screen::Order(_))
            .then_some((Duration::from_secs(1), Message::Second))
    }
}
// docs:end screens

impl Default for Shop {
    fn default() -> Self {
        Self {
            screens: Navigator::new(Screen::Orders),
            seconds: 0,
        }
    }
}
