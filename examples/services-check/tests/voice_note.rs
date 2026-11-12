//! The example of `docs/building-screens.md`, "Talking to a server, sound,
//! and remembering things", compiled and run: the application and the test
//! below are the two code blocks of that section, copied and then
//! formatted. If one changes, change the other.

use std::time::Duration;

use morningstar::ui::prelude::*;
use morningstar::{Frame, HostState};

const SERVER: &str = "https://api.example.com/v1/turn";

#[derive(Clone)]
enum Message {
    Tap,
    RecordingStarted(Result<(), AudioError>),
    Recorded(Result<Recording, AudioError>),
    Replied(Result<HttpResponse, HttpError>),
    Played(Result<(), AudioError>),
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Idle,
    Starting,
    Recording,
    Sending,
    Speaking,
}

/// What `update` has decided to start. `effects` empties it.
enum Start {
    Record,
    Stop,
    Send(Recording),
    Play(AudioClip),
    Remember,
}

struct Note {
    device_id: String,
    phase: Phase,
    error: Option<String>,
    start: Vec<Start>,
}

impl Note {
    fn new() -> Self {
        Self {
            device_id: String::new(),
            phase: Phase::Idle,
            error: None,
            start: Vec::new(),
        }
    }

    fn fail(&mut self, error: impl ToString) {
        self.phase = Phase::Idle;
        self.error = Some(error.to_string());
    }
}

impl Application for Note {
    type Message = Message;

    fn setup(&mut self, setup: &mut Setup<'_>) {
        setup.add_font(include_bytes!(
            "../../hello-gpu/assets/fonts/NotoSans-Regular.ttf"
        ));
        // Known before the first frame: what was stored last time, or else
        // the id the host made for this installation (32 hex digits).
        match setup.stored("device_id") {
            Some(id) => self.device_id = id.to_owned(),
            None => {
                self.device_id = setup.install_id().to_owned();
                self.start.push(Start::Remember); // effects runs right after setup
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let label = match self.phase {
            Phase::Idle => "Record",
            Phase::Starting => "...",
            Phase::Recording => "Send",
            Phase::Sending => "Sending...",
            Phase::Speaking => "Listening...",
        };
        let ready = matches!(self.phase, Phase::Idle | Phase::Recording);
        SafeArea::new(
            col![
                Button::new(label)
                    .on_press_maybe(ready.then_some(Message::Tap))
                    .fill_width(),
            ]
            .push_maybe(
                self.error
                    .as_ref()
                    .map(|error| text(error.clone()).caption().fill_width()),
            )
            .gap(12.0)
            .padding(20.0),
        )
        .into()
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::Tap if self.phase == Phase::Idle => {
                self.error = None;
                self.phase = Phase::Starting;
                self.start.push(Start::Record);
            }
            Message::Tap if self.phase == Phase::Recording => {
                self.phase = Phase::Sending;
                self.start.push(Start::Stop);
            }
            Message::Tap => {}
            Message::RecordingStarted(Ok(())) => self.phase = Phase::Recording,
            Message::RecordingStarted(Err(error)) => self.fail(error), // PermissionDenied, ...
            Message::Recorded(Ok(recording)) => self.start.push(Start::Send(recording)),
            Message::Recorded(Err(error)) => self.fail(error),
            Message::Replied(Ok(response)) if response.is_success() => {
                self.phase = Phase::Speaking;
                let mime = response
                    .header("content-type")
                    .unwrap_or("audio/mpeg")
                    .to_owned();
                self.start
                    .push(Start::Play(AudioClip::new(response.body, mime)));
            }
            Message::Replied(Ok(response)) => self.fail(format!("server said {}", response.status)),
            Message::Replied(Err(error)) => self.fail(error), // Offline, Timeout, ...
            // Stopped on purpose is not a failure worth showing.
            Message::Played(Ok(()) | Err(AudioError::Interrupted)) => self.phase = Phase::Idle,
            Message::Played(Err(error)) => self.fail(error),
        }
    }

    fn effects(&mut self, out: &mut Effects<Message>) {
        for start in self.start.drain(..) {
            match start {
                Start::Remember => out.store("device_id", self.device_id.clone()),
                Start::Record => out.start_recording(Message::RecordingStarted),
                Start::Stop => out.stop_recording(Message::Recorded),
                Start::Send(recording) => out.http(
                    HttpRequest::post(SERVER)
                        .header("X-Device-Id", self.device_id.clone())
                        .header("Content-Type", recording.mime.clone())
                        .body(recording.bytes),
                    Message::Replied,
                ),
                Start::Play(clip) => out.play(clip, Message::Played),
            }
        }
    }
}

#[test]
fn the_documented_example_works_as_documented() {
    let mut ui = Ui::testing(Note::new());
    let mut frame = Frame::for_app_with_host(
        // runs setup, as on a second launch
        &mut ui,
        &HostState::new().with_stored("device_id", "abc"),
    );

    ui.send(Message::Tap);
    assert!(ui.answer_recording_started(Ok(()))); // the microphone starts
    ui.send(Message::Tap);
    assert!(ui.answer_recording(Ok(Recording {
        // "the recording is these bytes"
        bytes: vec![1, 2, 3],
        mime: "audio/mp4".into(),
        duration: Duration::from_secs(2),
    })));

    // What is the application waiting for now?
    let ServiceRequest::Http(request) = &ui.pending_effects()[0].request else {
        panic!()
    };
    assert_eq!(request.header_value("X-Device-Id"), Some("abc"));
    assert_eq!(request.body, [1, 2, 3]);

    assert!(
        ui.answer_http(Ok(HttpResponse::new(200) // "the server replies this"
            .with_header("Content-Type", "audio/mpeg")
            .with_body(b"mp3 bytes".to_vec())))
    );
    assert!(ui.answer_playback(Ok(()))); // the clip finishes
    assert!(ui.app().phase == Phase::Idle);

    // And the unhappy path:
    ui.send(Message::Tap);
    ui.answer_recording_started(Err(AudioError::PermissionDenied));
    assert_eq!(
        ui.app().error.as_deref(),
        Some("the microphone is not allowed")
    );
    let _ = &mut frame;
}
