//! A smoke test for platform services (ADR 0019), meant to be run on a
//! phone by a person: one button per service, and under each the result it
//! last gave, as text. Nothing here is a design; it exists so that HTTP,
//! recording, playback and storage can each be seen to work, or to fail
//! with a readable reason, on a real device.
//!
//! Build and install it like any example:
//! `MORNINGSTAR_APP=services-check tools/build-android.sh --install`.
//!
//! It is also the smallest complete application that uses every effect, and
//! `tests/script.rs` shows the same application driven with no device.

use std::time::Duration;

use morningstar::ui::prelude::*;

/// Where the GET goes.
pub const URL: &str = "https://example.com";
/// How long "Record 3 s" records.
pub const RECORD_SECONDS: u32 = 3;
/// The key the launch counter is stored under.
pub const LAUNCHES_KEY: &str = "launches";

/// What a tap or a result asks of the application.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// "GET https://example.com".
    Get,
    /// The server's answer, or why there was none.
    Got(Result<HttpResponse, HttpError>),
    /// "Record 3 s".
    Record,
    /// The microphone is running, or why it is not.
    RecordingStarted(Result<(), AudioError>),
    /// One second of the recording has passed.
    Second,
    /// The recording, or why there is none.
    Recorded(Result<Recording, AudioError>),
    /// "Play it back".
    Play,
    /// The clip finished, or why it did not.
    Played(Result<(), AudioError>),
    /// "Stop".
    StopPlaying,
    /// "Store / Load": stores a number one higher, then loads it back.
    StoreLoad,
    /// What was loaded.
    Loaded(Option<String>),
}

/// Work `update` has decided on and `effects` has not started yet.
#[derive(Clone, Debug, PartialEq)]
enum Start {
    Get,
    Recording,
    StopRecording,
    Play(AudioClip),
    StopPlayback,
    StoreLoad(String),
}

/// The smoke test's state: one line of result per service.
#[derive(Default)]
pub struct ServicesCheck {
    start: Vec<Start>,
    /// The installation id the host handed over at setup.
    pub install_id: String,
    /// The launch counter as it was stored when the app started.
    pub launches_at_start: Option<String>,
    /// The number most recently stored.
    pub launches: u32,
    /// The result line of each service.
    pub http: String,
    /// See [`http`](Self::http).
    pub recording: String,
    /// See [`http`](Self::http).
    pub playback: String,
    /// See [`http`](Self::http).
    pub storage: String,
    /// Seconds left of the recording in progress; zero when none is.
    pub seconds_left: u32,
    /// The last recording, kept to be played back.
    pub last_recording: Option<Recording>,
    /// A request is out: its button does nothing until the answer.
    pub getting: bool,
    /// See [`getting`](Self::getting).
    pub playing: bool,
}

impl ServicesCheck {
    /// The application as it starts.
    pub fn new() -> Self {
        Self {
            http: "not tried".into(),
            recording: "not tried".into(),
            playback: "nothing recorded yet".into(),
            storage: "not tried".into(),
            ..Self::default()
        }
    }
}

impl Application for ServicesCheck {
    type Message = Message;

    fn setup(&mut self, setup: &mut Setup<'_>) {
        setup.add_font(include_bytes!(
            "../../hello-gpu/assets/fonts/NotoSans-Regular.ttf"
        ));
        setup.add_font(include_bytes!(
            "../../hello-gpu/assets/fonts/NotoSans-Bold.ttf"
        ));
        // Known before the first frame, with no request and no waiting.
        self.install_id = setup.install_id().to_owned();
        self.launches_at_start = setup.stored(LAUNCHES_KEY).map(str::to_owned);
        self.launches = self
            .launches_at_start
            .as_deref()
            .and_then(|text| text.parse().ok())
            .unwrap_or(0);
    }

    fn view(&self) -> Element<Message> {
        let section = |label: &'static str, message: Message, result: &str| {
            col![
                Button::new(label).on_press(message).fill_width(),
                text(result.to_owned()).caption().muted().fill_width(),
            ]
            .gap(6.0)
            .fill_width()
        };
        let record_label = if self.seconds_left > 0 {
            "Recording..."
        } else {
            "Record 3 s"
        };
        SafeArea::new(
            ScrollView::new(
                col![
                    text("Services check").title(),
                    text(format!("install id {}", self.install_id))
                        .caption()
                        .muted()
                        .fill_width(),
                    text(format!(
                        "stored at start: {}",
                        self.launches_at_start.as_deref().unwrap_or("nothing")
                    ))
                    .caption()
                    .muted()
                    .fill_width(),
                    section("GET https://example.com", Message::Get, &self.http),
                    section(record_label, Message::Record, &self.recording),
                    section("Play it back", Message::Play, &self.playback),
                    Button::new("Stop")
                        .plain()
                        .on_press(Message::StopPlaying)
                        .fill_width(),
                    section("Store / Load", Message::StoreLoad, &self.storage),
                ]
                .gap(18.0)
                .padding(20.0)
                .fill_width(),
            )
            .id("services"),
        )
        .into()
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::Get => {
                if !self.getting {
                    self.getting = true;
                    self.http = "waiting for the server...".into();
                    self.start.push(Start::Get);
                }
            }
            Message::Got(result) => {
                self.getting = false;
                self.http = match result {
                    Ok(response) => format!(
                        "status {}, {} bytes, {}",
                        response.status,
                        response.body.len(),
                        response.header("content-type").unwrap_or("no content type"),
                    ),
                    Err(error) => format!("failed: {error}"),
                };
            }
            Message::Record => {
                if self.seconds_left == 0 {
                    self.recording = "asking for the microphone...".into();
                    self.start.push(Start::Recording);
                }
            }
            Message::RecordingStarted(Ok(())) => {
                self.seconds_left = RECORD_SECONDS;
                self.recording = format!("recording, {RECORD_SECONDS} s left");
            }
            Message::RecordingStarted(Err(error)) => {
                self.recording = format!("could not record: {error}");
            }
            Message::Second => {
                self.seconds_left = self.seconds_left.saturating_sub(1);
                if self.seconds_left == 0 {
                    self.recording = "stopping...".into();
                    self.start.push(Start::StopRecording);
                } else {
                    self.recording = format!("recording, {} s left", self.seconds_left);
                }
            }
            Message::Recorded(Ok(recording)) => {
                self.recording = format!(
                    "{} bytes of {}, {:.1} s",
                    recording.bytes.len(),
                    recording.mime,
                    recording.duration.as_secs_f32(),
                );
                self.playback = "ready to play".into();
                self.last_recording = Some(recording);
            }
            Message::Recorded(Err(error)) => {
                self.recording = format!("no recording: {error}");
            }
            Message::Play => match &self.last_recording {
                Some(recording) if !self.playing => {
                    self.playing = true;
                    self.playback = "playing...".into();
                    self.start.push(Start::Play(recording.clone().into_clip()));
                }
                Some(_) => {}
                None => self.playback = "nothing recorded yet".into(),
            },
            Message::Played(result) => {
                self.playing = false;
                self.playback = match result {
                    Ok(()) => "played to the end".into(),
                    Err(error) => format!("ended: {error}"),
                };
            }
            Message::StopPlaying => self.start.push(Start::StopPlayback),
            Message::StoreLoad => {
                self.launches += 1;
                self.storage = format!("stored {}, loading...", self.launches);
                self.start.push(Start::StoreLoad(self.launches.to_string()));
            }
            Message::Loaded(value) => {
                self.storage = match value {
                    Some(value) => format!("loaded {value}"),
                    None => "loaded nothing".into(),
                };
            }
        }
    }

    fn tick(&self) -> Option<(Duration, Message)> {
        (self.seconds_left > 0).then_some((Duration::from_secs(1), Message::Second))
    }

    fn effects(&mut self, out: &mut Effects<Message>) {
        for start in self.start.drain(..) {
            match start {
                Start::Get => out.http(
                    HttpRequest::get(URL).timeout(Duration::from_secs(15)),
                    Message::Got,
                ),
                Start::Recording => out.start_recording(Message::RecordingStarted),
                Start::StopRecording => out.stop_recording(Message::Recorded),
                Start::Play(clip) => out.play(clip, Message::Played),
                Start::StopPlayback => out.stop_playback(),
                Start::StoreLoad(value) => {
                    out.store(LAUNCHES_KEY, value);
                    out.load(LAUNCHES_KEY, Message::Loaded);
                }
            }
        }
    }
}

morningstar::app!(Ui::new(ServicesCheck::new()));
