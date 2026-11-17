//! Samples for work that takes time: a request to a server, a value kept
//! on the phone, and the clipboard.

use std::time::Duration;

use morningstar::ui::prelude::*;

/// The key the last order number is kept under.
pub const LAST_ORDER: &str = "last-order";
/// Where the samples' server is.
pub const SERVER: &str = "http://127.0.0.1:8790";

// docs:begin model
#[derive(Clone, Debug)]
pub enum Message {
    Refresh,
    Loaded(Result<HttpResponse, HttpError>),
    Copy,
}

/// Work `update` has decided on and `effects` has not started yet.
enum Start {
    Fetch,
    Remember(String),
    Copy(String),
}

#[derive(Default)]
pub struct Orders {
    start: Vec<Start>,
    pub loading: bool,
    pub status: String,
    pub install_id: String,
}
// docs:end model

impl Application for Orders {
    type Message = Message;

    // docs:begin setup
    fn setup(&mut self, setup: &mut Setup<'_>) {
        // Known before the first frame, with no request and no waiting:
        // this installation's id, and the values stored on earlier runs.
        self.install_id = setup.install_id().to_owned();
        self.status = match setup.stored(LAST_ORDER) {
            Some(order) => format!("Last time: {order}"),
            None => "Nothing yet.".to_owned(),
        };
    }
    // docs:end setup

    fn view(&self) -> Element<Message> {
        col![
            text(self.status.clone()).fill_width(),
            Button::new("Refresh").on_press_maybe((!self.loading).then_some(Message::Refresh)),
            Button::new("Copy").plain().on_press(Message::Copy),
        ]
        .gap(12.0)
        .padding(20.0)
        .fill_width()
        .into()
    }

    // docs:begin update
    // `update` returns at once: it records what should be started.
    fn update(&mut self, message: Message) {
        match message {
            Message::Refresh => {
                self.loading = true;
                self.start.push(Start::Fetch);
            }
            Message::Loaded(result) => {
                self.loading = false;
                self.status = match result {
                    Ok(response) if response.is_success() => {
                        let body = response.text().unwrap_or_default().to_owned();
                        self.start.push(Start::Remember(body.clone()));
                        body
                    }
                    Ok(response) => format!("The server answered {}.", response.status),
                    Err(HttpError::Offline) => "No connection.".to_owned(),
                    Err(error) => format!("No answer: {error}."),
                };
            }
            Message::Copy => self.start.push(Start::Copy(self.status.clone())),
        }
    }

    // Called after every `update`: requests leave as data, and each
    // answer comes back as a message.
    fn effects(&mut self, out: &mut Effects<Message>) {
        for start in self.start.drain(..) {
            match start {
                Start::Fetch => out.http(
                    HttpRequest::get(format!("{SERVER}/v1/orders/latest"))
                        .header("X-Device-Id", self.install_id.clone())
                        .timeout(Duration::from_secs(10)),
                    Message::Loaded,
                ),
                // Kept on the phone; there at the next `setup`.
                Start::Remember(order) => out.store(LAST_ORDER, order),
                Start::Copy(text) => out.set_clipboard(text),
            }
        }
    }
    // docs:end update
}
