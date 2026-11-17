//! Samples for live rooms: joining with a ticket, following the room's
//! events, and showing a picture.

use morningstar::ui::prelude::*;

#[derive(Clone, Debug)]
pub enum Message {
    /// The server handed out a ticket for the room.
    Ticket {
        url: String,
        token: String,
    },
    Live(LiveEvent),
    Mute(bool),
    HangUp,
}

enum Start {
    Join { url: String, token: String },
    Microphone(bool),
    Leave,
}

#[derive(Default)]
pub struct Call {
    start: Vec<Start>,
    pub connected: bool,
    pub muted: bool,
    pub status: String,
    /// The other person's camera or shared screen, while there is one.
    pub picture: Option<VideoTrack>,
}

impl Application for Call {
    type Message = Message;

    // docs:begin view
    fn view(&self) -> Element<Message> {
        let picture: Element<Message> = match self.picture {
            // Frames never pass through the application: it draws the
            // track, and the host fills it in.
            Some(track) => LiveVideo::new(track).fit(ImageFit::Cover).fill().into(),
            None => Container::empty()
                .background(ThemeColor::Surface)
                .fill()
                .into(),
        };
        stack![
            picture,
            col![
                text(self.status.clone()).color(ThemeColor::OnPrimary),
                Button::new(if self.muted { "Unmute" } else { "Mute" })
                    .secondary()
                    .on_press(Message::Mute(!self.muted)),
                Button::new("Hang up")
                    .destructive()
                    .on_press(Message::HangUp),
            ]
            .gap(12.0)
            .padding(20.0)
            .place(Alignment::BOTTOM_CENTER),
        ]
        .fill()
        .into()
    }
    // docs:end view

    // docs:begin events
    fn update(&mut self, message: Message) {
        match message {
            Message::Ticket { url, token } => self.start.push(Start::Join { url, token }),
            Message::Mute(muted) => {
                self.muted = muted;
                self.start.push(Start::Microphone(!muted));
            }
            Message::HangUp => self.start.push(Start::Leave),
            // One message per event, until a final one.
            Message::Live(event) => match event {
                LiveEvent::Connected => {
                    self.connected = true;
                    self.status = "Sound is live.".to_owned();
                }
                LiveEvent::VideoStarted(stream) if !stream.local => {
                    self.picture = Some(stream.track);
                }
                LiveEvent::VideoStopped(stream) if Some(stream.track) == self.picture => {
                    self.picture = None;
                }
                LiveEvent::Reconnecting => self.status = "Reconnecting...".to_owned(),
                LiveEvent::Reconnected => self.status = "Sound is live.".to_owned(),
                // Final: the room is over, whoever ended it.
                LiveEvent::Disconnected(_) => {
                    self.connected = false;
                    self.picture = None;
                    self.status = "The call ended.".to_owned();
                }
                // Final: it never started, or could not go on.
                LiveEvent::Failed(LiveError::PermissionDenied) => {
                    self.status = "The microphone is not allowed.".to_owned();
                }
                LiveEvent::Failed(error) => self.status = format!("No sound: {error}."),
                _ => {}
            },
        }
    }

    fn effects(&mut self, out: &mut Effects<Message>) {
        for start in self.start.drain(..) {
            match start {
                // The ticket comes from the app's server, never from the
                // app: a server holds the key that mints it.
                Start::Join { url, token } => out.join_live(
                    LiveJoin::new(url, token).microphone(true).camera(true),
                    Message::Live,
                ),
                Start::Microphone(live) => out.set_live_microphone(live),
                Start::Leave => out.leave_live(),
            }
        }
    }
    // docs:end events
}
