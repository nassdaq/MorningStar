//! Platform services: the plain data of a request an application makes of
//! the device it runs on, and of the answer.
//!
//! An application cannot do these things itself without platform code:
//! talk to a server over the platform's TLS stack, record from the
//! microphone, play sound, keep a value between launches. It asks; the
//! host (Java on Android, Swift on iOS) does the work off the render
//! thread and answers later. Everything here is data that crosses that
//! boundary: no platform type, no callback, no thread. The plumbing is in
//! the runtime (`Engine::take_service_requests`, `Engine::service_result`)
//! and the application-facing API in the UI layer (`Effects`). See ADR 0019,
//! and ADR 0038 for the live room, the one request here that is answered
//! more than once.

use crate::VideoTrack;
use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

/// A response body larger than this is not delivered: the request fails
/// with [`HttpError::TooLarge`]. Hosts stop reading at this size.
pub const MAX_HTTP_RESPONSE_BYTES: usize = 10 * 1024 * 1024;
/// A request that does not set a timeout gets this one.
pub const DEFAULT_HTTP_TIMEOUT: Duration = Duration::from_secs(30);
/// A recording is stopped by the host when it reaches this length. What
/// was recorded is kept and returned by the next "stop recording".
pub const MAX_RECORDING_DURATION: Duration = Duration::from_secs(60);
/// The id an application gets at setup when no host supplied one.
pub const NO_INSTALL_ID: &str = "00000000000000000000000000000000";

/// The method of an [`HttpRequest`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HttpMethod {
    /// `GET`
    Get,
    /// `POST`
    Post,
    /// `PUT`
    Put,
    /// `PATCH`
    Patch,
    /// `DELETE`
    Delete,
    /// `HEAD`
    Head,
}

impl HttpMethod {
    /// The method as it is written in a request: `"GET"`, `"POST"`, ...
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Head => "HEAD",
        }
    }
}

/// One HTTP or HTTPS request: method, URL, headers, body, timeout.
///
/// The whole body is sent and the whole response is read; there is no
/// streaming. There is no cookie store. Redirects, proxies, TLS versions
/// and the trusted certificates are the platform's defaults.
///
/// ```
/// use morningstar_core::HttpRequest;
/// use std::time::Duration;
///
/// let request = HttpRequest::post("https://api.example.com/v1/turn")
///     .header("X-Device-Id", "3f2a")
///     .json_body(br#"{"text":"habari"}"#.to_vec())
///     .timeout(Duration::from_secs(10));
/// assert_eq!(request.header_value("content-type"), Some("application/json"));
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpRequest {
    /// The method.
    pub method: HttpMethod,
    /// The absolute URL, `https://...` (or `http://...` where the
    /// platform's cleartext policy allows it).
    pub url: String,
    /// Header names and values, sent in this order.
    pub headers: Vec<(String, String)>,
    /// The request body; empty for none.
    pub body: Vec<u8>,
    /// How long the whole exchange may take before it fails with
    /// [`HttpError::Timeout`].
    pub timeout: Duration,
}

impl HttpRequest {
    /// A request with no headers, no body and the default timeout.
    pub fn new(method: HttpMethod, url: impl Into<String>) -> Self {
        Self {
            method,
            url: url.into(),
            headers: Vec::new(),
            body: Vec::new(),
            timeout: DEFAULT_HTTP_TIMEOUT,
        }
    }

    /// A `GET` request.
    pub fn get(url: impl Into<String>) -> Self {
        Self::new(HttpMethod::Get, url)
    }

    /// A `POST` request.
    pub fn post(url: impl Into<String>) -> Self {
        Self::new(HttpMethod::Post, url)
    }

    /// A `PUT` request.
    pub fn put(url: impl Into<String>) -> Self {
        Self::new(HttpMethod::Put, url)
    }

    /// A `DELETE` request.
    pub fn delete(url: impl Into<String>) -> Self {
        Self::new(HttpMethod::Delete, url)
    }

    /// Adds a header. Carriage returns and line feeds are removed from the
    /// name and the value: a header cannot contain them, and removing them
    /// here means a value that came from user input cannot add a header
    /// of its own.
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        let clean = |text: String| {
            if text.contains(['\r', '\n']) {
                text.replace(['\r', '\n'], "")
            } else {
                text
            }
        };
        self.headers.push((clean(name.into()), clean(value.into())));
        self
    }

    /// Sets the body. Say what it is with a `Content-Type`
    /// [`header`](Self::header).
    pub fn body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();
        self
    }

    /// Sets the body to JSON the application has already encoded, with
    /// `Content-Type: application/json`.
    pub fn json_body(self, json: impl Into<Vec<u8>>) -> Self {
        self.header("Content-Type", "application/json").body(json)
    }

    /// Sets the timeout for the whole exchange. Hosts count in whole
    /// milliseconds and treat anything shorter than one as one.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// The value of the first header called `name`, compared without
    /// regard to case.
    pub fn header_value(&self, name: &str) -> Option<&str> {
        find_header(&self.headers, name)
    }
}

/// The answer to an [`HttpRequest`]: any status the server gave, including
/// 4xx and 5xx. Only a request that got no answer is an [`HttpError`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpResponse {
    /// The status code, such as 200 or 404.
    pub status: u16,
    /// Header names and values as the platform reports them.
    pub headers: Vec<(String, String)>,
    /// The whole body, already decompressed if the platform does that.
    pub body: Vec<u8>,
}

impl HttpResponse {
    /// A response with this status, no headers and an empty body. Tests
    /// build the answer of a pretended server with this.
    pub fn new(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    /// Adds a header.
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// Sets the body.
    pub fn with_body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();
        self
    }

    /// True for a status from 200 to 299.
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// The value of the first header called `name`, compared without
    /// regard to case.
    pub fn header(&self, name: &str) -> Option<&str> {
        find_header(&self.headers, name)
    }

    /// The body as text, if it is valid UTF-8.
    pub fn text(&self) -> Option<&str> {
        std::str::from_utf8(&self.body).ok()
    }
}

fn find_header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

/// Why an [`HttpRequest`] got no response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HttpError {
    /// The server could not be reached: no network, the name did not
    /// resolve, or the connection was refused or dropped.
    Offline,
    /// The exchange took longer than the request's timeout.
    Timeout,
    /// The URL is not an absolute `http` or `https` URL.
    InvalidUrl,
    /// The response body is larger than [`MAX_HTTP_RESPONSE_BYTES`].
    TooLarge,
    /// Nothing can make requests here: there is no host (a test without a
    /// driver, a screenshot tool) or the host is older than this library.
    Unsupported,
    /// Anything else, described by the platform: a TLS failure, a
    /// cleartext request the platform's policy forbids. For logs; do not
    /// match on the text.
    Failed(String),
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Offline => f.write_str("the server could not be reached"),
            Self::Timeout => f.write_str("the request timed out"),
            Self::InvalidUrl => f.write_str("the URL is not a valid http or https URL"),
            Self::TooLarge => f.write_str("the response is larger than 10 MB"),
            Self::Unsupported => f.write_str("HTTP requests are not available here"),
            Self::Failed(reason) => write!(f, "the request failed: {reason}"),
        }
    }
}

impl std::error::Error for HttpError {}

/// Encoded sound to play: the bytes of a file and what kind of file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioClip {
    /// The whole file.
    pub bytes: Vec<u8>,
    /// Its media type. `audio/mpeg` (MP3) and `audio/mp4` (AAC in M4A) play
    /// on both platforms.
    pub mime: String,
}

impl AudioClip {
    /// A clip from the bytes of an audio file, for example the body of an
    /// [`HttpResponse`], and its media type.
    pub fn new(bytes: impl Into<Vec<u8>>, mime: impl Into<String>) -> Self {
        Self {
            bytes: bytes.into(),
            mime: mime.into(),
        }
    }
}

/// What the microphone recorded between "start" and "stop".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recording {
    /// The whole file.
    pub bytes: Vec<u8>,
    /// Its media type: `audio/mp4` (mono AAC in an M4A container) from
    /// both hosts.
    pub mime: String,
    /// How long the recording is.
    pub duration: Duration,
}

impl Recording {
    /// The recording as something to play.
    pub fn into_clip(self) -> AudioClip {
        AudioClip {
            bytes: self.bytes,
            mime: self.mime,
        }
    }
}

/// Why recording or playback did not happen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudioError {
    /// The user has not allowed the microphone. Asking again later asks
    /// the user again, unless they refused for good, in which case only
    /// the system's settings can change it.
    PermissionDenied,
    /// A recording is already running (for "start"), or the microphone or
    /// the audio output is held by something that would not give it up.
    Busy,
    /// Nothing can record or play here: there is no host (a test without a
    /// driver, a screenshot tool), the host is older than this library, or
    /// the device has no microphone.
    Unsupported,
    /// "Stop recording" with no recording to stop.
    NotRecording,
    /// Playback ended before the clip did: the application stopped it,
    /// started another clip, went to the background, or lost the audio
    /// output to a phone call.
    Interrupted,
    /// Anything else, described by the platform. For logs; do not match on
    /// the text.
    Failed(String),
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PermissionDenied => f.write_str("the microphone is not allowed"),
            Self::Busy => f.write_str("the microphone or speaker is in use"),
            Self::Unsupported => f.write_str("audio is not available here"),
            Self::NotRecording => f.write_str("no recording is running"),
            Self::Interrupted => f.write_str("playback was interrupted"),
            Self::Failed(reason) => write!(f, "audio failed: {reason}"),
        }
    }
}

impl std::error::Error for AudioError {}

/// What a live room is joined with: where it is, the ticket that lets this
/// person in, and how the microphone and the loudspeaker start.
///
/// A live room is a conversation carried by a media server (LiveKit): the
/// people in it hear each other. The address and the token are what the
/// application's own server hands out for one person and one room; the
/// token is a secret, which is why this type does not print it.
///
/// ```
/// use morningstar_core::LiveJoin;
///
/// let join = LiveJoin::new("wss://live.example.com", "a-ticket").speaker(true);
/// assert!(join.microphone && join.speaker && !join.report_speaking);
/// assert!(!format!("{join:?}").contains("a-ticket"));
/// ```
#[derive(Clone, PartialEq, Eq)]
pub struct LiveJoin {
    /// The media server's address: `wss://...`.
    pub url: String,
    /// The ticket for this person and this room. A secret: never logged.
    pub token: String,
    /// Whether the microphone is live from the start. With `false` the
    /// room is joined muted; either way the user is asked for the
    /// microphone before joining, because a room is a conversation.
    pub microphone: bool,
    /// Whether sound comes out of the loudspeaker (`true`) or the earpiece
    /// (`false`) while nothing else, such as a headset, is connected.
    pub speaker: bool,
    /// Whether [`LiveEvent::Speaking`] is wanted. Off unless asked for:
    /// every such event is a message and a frame.
    pub report_speaking: bool,
    /// Whether this device's camera is on from the start (ADR 0039). Off
    /// unless asked for; the user is asked for the camera when it is first
    /// turned on, and a refusal is
    /// [`LiveEvent::MediaFailed`], not the end of the room.
    pub camera: bool,
    /// What lets the room go on while the app is not on screen (ADR 0042):
    /// the notice the system shows for as long as it does. `None`, the
    /// default, and the room ends when the app leaves the screen
    /// ([`LiveDisconnect::Hidden`]). Only an app built with the
    /// `live-background` service can go on; in any other the notice is
    /// ignored and the room ends as before.
    pub background: Option<LiveBackground>,
}

/// The notice a live room shows while it goes on behind another app or a
/// locked screen (ADR 0042): what the user reads in the system's
/// notification, and the name of the action that ends the room from there.
/// The user must be able to see that the microphone is open and to close
/// it without finding the app again, which is why a room cannot go on
/// without one.
///
/// ```
/// use morningstar_core::{LiveBackground, LiveJoin};
///
/// let notice = LiveBackground::new("Call with Baraka", "Tap to return to the call");
/// assert_eq!(notice.hang_up, "Hang up");
/// let join = LiveJoin::new("wss://live.example.com", "a-ticket").background(notice);
/// assert!(join.background.is_some());
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveBackground {
    /// The notification's first line: what is going on ("Call with ...").
    pub title: String,
    /// Its second line.
    pub text: String,
    /// The name of the action that ends the room: "Hang up" unless
    /// changed.
    pub hang_up: String,
}

impl LiveBackground {
    /// A notice with these two lines and an action called "Hang up".
    pub fn new(title: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            text: text.into(),
            hang_up: "Hang up".to_owned(),
        }
    }

    /// Names the action that ends the room, in the user's language.
    pub fn hang_up(mut self, label: impl Into<String>) -> Self {
        self.hang_up = label.into();
        self
    }

    /// The three texts as they cross to a host: one per line, in the
    /// order title, text, action, with any line break inside one of them
    /// made a space.
    pub fn to_wire(&self) -> String {
        let line = |text: &str| text.replace(['\n', '\r'], " ");
        format!(
            "{}\n{}\n{}",
            line(&self.title),
            line(&self.text),
            line(&self.hang_up)
        )
    }
}

impl LiveJoin {
    /// Joins with the microphone live, sound in the earpiece, and no
    /// speaking reports.
    pub fn new(url: impl Into<String>, token: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            token: token.into(),
            microphone: true,
            speaker: false,
            report_speaking: false,
            camera: false,
            background: None,
        }
    }

    /// Lets the room go on while the app is not on screen, under this
    /// notice (ADR 0042). See [`LiveJoin::background`].
    pub fn background(mut self, notice: LiveBackground) -> Self {
        self.background = Some(notice);
        self
    }

    /// Whether the camera is on from the start.
    pub fn camera(mut self, on: bool) -> Self {
        self.camera = on;
        self
    }

    /// Whether the microphone is live from the start.
    pub fn microphone(mut self, live: bool) -> Self {
        self.microphone = live;
        self
    }

    /// Whether sound starts in the loudspeaker rather than the earpiece.
    pub fn speaker(mut self, on: bool) -> Self {
        self.speaker = on;
        self
    }

    /// Asks for [`LiveEvent::Speaking`] events.
    pub fn report_speaking(mut self, wanted: bool) -> Self {
        self.report_speaking = wanted;
        self
    }
}

impl fmt::Debug for LiveJoin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The token is a secret and the address says whose server it is:
        // neither is printed, so that neither reaches a log or a failed
        // test's output by accident.
        f.debug_struct("LiveJoin")
            .field("url", &format_args!("<{} bytes>", self.url.len()))
            .field("token", &format_args!("<{} bytes>", self.token.len()))
            .field("microphone", &self.microphone)
            .field("speaker", &self.speaker)
            .field("report_speaking", &self.report_speaking)
            .field("camera", &self.camera)
            .field("background", &self.background.is_some())
            .finish()
    }
}

/// Someone else in a live room.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveParticipant {
    /// Who they are to the media server: unique in the room.
    pub identity: String,
    /// The name to show; may be empty.
    pub name: String,
}

/// What a video in a live room shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LiveVideoSource {
    /// Someone's camera.
    Camera,
    /// Someone's screen, shared.
    Screen,
}

/// A video in a live room: whose it is, what it shows, and the id its
/// pictures are drawn by (ADR 0039).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveVideoStream {
    /// Whose video: their identity in the room. This device's own person
    /// has the identity of its ticket.
    pub identity: String,
    /// A camera, or a shared screen.
    pub source: LiveVideoSource,
    /// True for this device's own camera or screen.
    pub local: bool,
    /// What its pictures are drawn by. This device's own shared screen has
    /// a track like any other, but no pictures are delivered for it: the
    /// screen is already in front of its user.
    pub track: VideoTrack,
}

/// Where a live room's sound comes out, and where its microphone is (ADR
/// 0042).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LiveAudioRoute {
    /// The speaker at the ear. A tablet has none.
    Earpiece,
    /// The loudspeaker.
    Speaker,
    /// A headset or headphones on a wire.
    Wired,
    /// A Bluetooth headset.
    Bluetooth,
}

impl LiveAudioRoute {
    /// Every route, in the order of their wire numbers (1 to 4).
    pub const ALL: [Self; 4] = [Self::Earpiece, Self::Speaker, Self::Wired, Self::Bluetooth];

    /// The number this route crosses to a host as: 1 to 4.
    pub fn to_wire(self) -> u8 {
        match self {
            Self::Earpiece => 1,
            Self::Speaker => 2,
            Self::Wired => 3,
            Self::Bluetooth => 4,
        }
    }

    /// The route a host means by `number`; `None` for zero ("none") and
    /// for a number this library does not know.
    pub fn from_wire(number: u32) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|route| u32::from(route.to_wire()) == number)
    }
}

/// The routes a live room's sound can take on this device now, and the one
/// it takes (ADR 0042).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LiveAudioRoutes {
    /// What there is to choose from, in the order of
    /// [`LiveAudioRoute::ALL`].
    pub available: Vec<LiveAudioRoute>,
    /// The one in use; `None` while the platform has not settled on one.
    pub selected: Option<LiveAudioRoute>,
}

impl LiveAudioRoutes {
    /// These routes as the code of [`LiveEvent::from_wire`]'s kind 11.
    pub fn to_wire(&self) -> u32 {
        let selected = self.selected.map_or(0, |route| u32::from(route.to_wire()));
        self.available.iter().fold(selected, |code, route| {
            code | 1 << (3 + u32::from(route.to_wire()))
        })
    }

    fn from_wire(code: u32) -> Self {
        Self {
            available: LiveAudioRoute::ALL
                .into_iter()
                .filter(|route| code & 1 << (3 + u32::from(route.to_wire())) != 0)
                .collect(),
            selected: LiveAudioRoute::from_wire(code & 0xf),
        }
    }
}

/// The most that [`ServiceRequest::LiveData`] sends in one message: what
/// the media server carries reliably in one piece. Longer is not sent.
pub const MAX_LIVE_DATA_BYTES: usize = 15_000;

/// A message someone sent to the people in a live room: bytes, under a
/// topic that says what they are (ADR 0039).
#[derive(Clone, PartialEq, Eq)]
pub struct LiveData {
    /// Who sent it, by identity; empty when the server did.
    pub from: String,
    /// What kind of message it is, as the sender named it; may be empty.
    pub topic: String,
    /// The message.
    pub payload: Vec<u8>,
}

impl fmt::Debug for LiveData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // What people say to each other is not for a log.
        f.debug_struct("LiveData")
            .field("from", &format_args!("<{} bytes>", self.from.len()))
            .field("topic", &self.topic)
            .field("payload", &format_args!("<{} bytes>", self.payload.len()))
            .finish()
    }
}

/// Something that happened to the live room an application joined. A join
/// is answered with a sequence of these, which always ends with exactly
/// one [`Disconnected`](Self::Disconnected) or [`Failed`](Self::Failed);
/// nothing follows that.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveEvent {
    /// The room is joined: sound from the others plays, and the microphone
    /// is heard by them unless the room was joined muted. A
    /// [`ParticipantJoined`](Self::ParticipantJoined) follows for everyone
    /// who was already there.
    Connected,
    /// Someone is in the room: they were there already, or have come in.
    ParticipantJoined(LiveParticipant),
    /// Someone left. Their `identity`.
    ParticipantLeft(String),
    /// Who is speaking now, loudest first, by identity (this device's own
    /// person included, under the identity of its ticket); empty when
    /// nobody is. Only sent when the join asked for it.
    Speaking(Vec<String>),
    /// The connection dropped and is being made again; nothing is heard
    /// meanwhile. Followed by [`Reconnected`](Self::Reconnected), or by
    /// the end of the room.
    Reconnecting,
    /// The connection is back.
    Reconnected,
    /// A video has started: someone's camera or shared screen, this
    /// device's own included. Its pictures are drawn by its `track` until
    /// [`VideoStopped`](Self::VideoStopped).
    VideoStarted(LiveVideoStream),
    /// That video is over: its owner turned it off, or left.
    VideoStopped(LiveVideoStream),
    /// This device's camera or screen could not be turned on: the user
    /// refused it ([`LiveError::PermissionDenied`]), the app was built
    /// without it ([`LiveError::Unsupported`]), or it failed. The room
    /// goes on without it.
    MediaFailed {
        /// Which of the two.
        source: LiveVideoSource,
        /// Why.
        error: LiveError,
    },
    /// Someone sent a message to the room.
    Data(LiveData),
    /// Where the sound can go and where it goes now (ADR 0042): sent once
    /// the room is connected and again whenever either changes, a headset
    /// connecting in the middle of a call included.
    AudioRoutes(LiveAudioRoutes),
    /// The room is over for this device, for the reason given. Final.
    Disconnected(LiveDisconnect),
    /// The room could not be joined, or broke. Final.
    Failed(LiveError),
}

impl LiveEvent {
    /// True for the event that ends a room: nothing follows it, and the
    /// microphone is closed.
    pub fn is_final(&self) -> bool {
        matches!(self, Self::Disconnected(_) | Self::Failed(_))
    }

    /// The event a host reports with these four values, which is how one
    /// crosses the boundary on both platforms (`nativeLiveEvent` on
    /// Android, `morningstar_ios_live_event` on iOS). `None` for a kind
    /// this library does not know: a newer host, whose event is ignored
    /// rather than guessed at.
    ///
    /// | `kind` | Event | `code` | `first` | `second` |
    /// | --- | --- | --- | --- | --- |
    /// | 0 | `Connected` | | | |
    /// | 1 | `ParticipantJoined` | | identity | name |
    /// | 2 | `ParticipantLeft` | | identity | |
    /// | 3 | `Speaking` | | identities, one per line | |
    /// | 4 | `Reconnecting` | | | |
    /// | 5 | `Reconnected` | | | |
    /// | 6 | `Disconnected` | 0 left, 1 replaced, 2 hidden, 3 ended, 4 lost, 5 hung up | | |
    /// | 7 | `Failed` | 1 permission denied, 2 unsupported, 3 failed | the reason, for `failed` | |
    /// | 8 | `VideoStarted` | track * 4 + (2 if local) + (1 if a screen) | identity | |
    /// | 9 | `VideoStopped` | as for 8 | identity | |
    /// | 10 | `MediaFailed` | error * 2 + (1 if the screen), errors as for 7 | the reason, for `failed` | |
    /// | 11 | `AudioRoutes` | the route in use (0 none, 1 earpiece, 2 loudspeaker, 3 wired, 4 Bluetooth) + 16, 32, 64, 128 for each of those four that is available | | |
    ///
    /// [`Data`](Self::Data) does not come this way: it carries bytes, and
    /// has a call of its own on both platforms.
    ///
    /// An unknown reason for a disconnection is read as `Ended` and an
    /// unknown failure as `Failed`, so that the end of a room is never
    /// lost to a code this library has not heard of.
    pub fn from_wire(kind: u32, code: u32, first: &str, second: &str) -> Option<Self> {
        Some(match kind {
            0 => Self::Connected,
            1 => Self::ParticipantJoined(LiveParticipant {
                identity: first.to_owned(),
                name: second.to_owned(),
            }),
            2 => Self::ParticipantLeft(first.to_owned()),
            3 => Self::Speaking(
                first
                    .lines()
                    .filter(|identity| !identity.is_empty())
                    .map(str::to_owned)
                    .collect(),
            ),
            4 => Self::Reconnecting,
            5 => Self::Reconnected,
            6 => Self::Disconnected(match code {
                0 => LiveDisconnect::Left,
                1 => LiveDisconnect::Replaced,
                2 => LiveDisconnect::Hidden,
                4 => LiveDisconnect::Lost,
                5 => LiveDisconnect::HungUp,
                _ => LiveDisconnect::Ended,
            }),
            7 => Self::Failed(live_error(code, first)),
            8 | 9 => {
                let stream = LiveVideoStream {
                    identity: first.to_owned(),
                    source: live_source(code & 1),
                    local: code & 2 != 0,
                    track: VideoTrack(code >> 2),
                };
                if kind == 8 {
                    Self::VideoStarted(stream)
                } else {
                    Self::VideoStopped(stream)
                }
            }
            10 => Self::MediaFailed {
                source: live_source(code & 1),
                error: live_error(code >> 1, first),
            },
            11 => Self::AudioRoutes(LiveAudioRoutes::from_wire(code)),
            _ => return None,
        })
    }
}

fn live_error(code: u32, reason: &str) -> LiveError {
    match code {
        1 => LiveError::PermissionDenied,
        2 => LiveError::Unsupported,
        _ => LiveError::Failed(reason.to_owned()),
    }
}

fn live_source(bit: u32) -> LiveVideoSource {
    if bit == 0 {
        LiveVideoSource::Camera
    } else {
        LiveVideoSource::Screen
    }
}

/// Why a live room ended without anything having gone wrong on this
/// device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveDisconnect {
    /// The application left it.
    Left,
    /// The application joined another room: there is one at a time.
    Replaced,
    /// The app left the screen. A room does not continue in the
    /// background (the microphone would stay open where the user cannot
    /// see it) unless it was joined with a [`LiveBackground`] notice in an
    /// app built for that, or this device's screen is shared in it.
    Hidden,
    /// The user ended it from the notification of a room that was going on
    /// in the background (ADR 0042). The application has had no say: it is
    /// told so that it can do what its own hang-up does besides.
    HungUp,
    /// The other side ended it: the room was closed, this person was
    /// removed, or the same ticket joined from somewhere else.
    Ended,
    /// The connection was lost and could not be made again.
    Lost,
}

impl fmt::Display for LiveDisconnect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Left => "the room was left",
            Self::Replaced => "another room was joined",
            Self::Hidden => "the app left the screen",
            Self::Ended => "the room was ended from the other side",
            Self::Lost => "the connection was lost",
            Self::HungUp => "it was ended from the notification",
        })
    }
}

/// Why a live room could not be joined, or broke.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveError {
    /// The user has not allowed it: the microphone for a join, the camera
    /// or the screen in [`LiveEvent::MediaFailed`]. Asking again asks
    /// them again, unless they refused for good, in which case only the
    /// system's settings can change it.
    PermissionDenied,
    /// Nothing can join a live room here: there is no host (a test without
    /// a driver, a screenshot tool), the host is older than this library,
    /// the app was built without the live service (it is opt-in, see ADR
    /// 0038), or the platform has none yet (iOS).
    Unsupported,
    /// Anything else, described by the platform: the server refused the
    /// ticket, the address is wrong, the network is down. For logs; do not
    /// match on the text.
    Failed(String),
}

impl fmt::Display for LiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PermissionDenied => f.write_str("not allowed: the user refused the permission"),
            Self::Unsupported => f.write_str("live rooms are not available here"),
            Self::Failed(reason) => write!(f, "the live room failed: {reason}"),
        }
    }
}

impl std::error::Error for LiveError {}

/// Identifies one request an application made, so that its answer can be
/// matched to it. Chosen by the application side, opaque to hosts, never
/// zero, and not reused while the application lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ServiceId(pub u64);

/// Something an application asks of the platform.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServiceRequest {
    /// Make an HTTP request. Answered with [`ServiceResult::Http`].
    Http(HttpRequest),
    /// Start recording from the microphone, asking the user for permission
    /// if needed. Answered with [`ServiceResult::RecordingStarted`] once
    /// the microphone is running, or with why it is not.
    StartRecording,
    /// Stop the recording and hand it over. Answered with
    /// [`ServiceResult::Recording`].
    StopRecording,
    /// Play a clip, replacing one that is playing. Answered with
    /// [`ServiceResult::Played`] when it has finished or failed.
    Play(AudioClip),
    /// Stop whatever is playing. Not answered: the clip's own
    /// [`ServiceResult::Played`] says [`AudioError::Interrupted`].
    StopPlayback,
    /// Keep `value` under `key` between launches, or forget the key when
    /// `value` is `None`. Not answered.
    Store {
        /// The name the value is kept under.
        key: String,
        /// The value, or `None` to remove the key.
        value: Option<String>,
    },
    /// Read the value kept under `key`. Answered with
    /// [`ServiceResult::Loaded`].
    Load {
        /// The name the value was kept under.
        key: String,
    },
    /// Put this plain text on the system clipboard, replacing what is
    /// there. Not answered.
    SetClipboard(String),
    /// Read the system clipboard as plain text (formatting is dropped).
    /// Answered with [`ServiceResult::Clipboard`]. Hosts read only when
    /// asked: on Android 12 and later the system tells the user each time
    /// an application reads what another one copied.
    ReadClipboard,
    /// Have the screen reader say `message`, if one is running (ADR 0032):
    /// for something that happened and is not otherwise spoken ("Report
    /// ready"). Not answered, and nothing happens when no assistive
    /// technology is on.
    Announce {
        /// What to say.
        message: String,
        /// Say it at once, interrupting what is being said.
        assertive: bool,
    },
    /// Join a live room (ADR 0038), leaving the one that is joined: there
    /// is one at a time. The only request answered more than once: with a
    /// [`ServiceResult::Live`] for each [`LiveEvent`], under this
    /// request's id, the last of them [final](LiveEvent::is_final).
    LiveJoin(LiveJoin),
    /// Leave the live room. Not answered: the join's own events end with
    /// [`LiveDisconnect::Left`]. Nothing happens when no room is joined.
    LiveLeave,
    /// Open (`true`) or mute the microphone in the live room. Not
    /// answered. A host that cannot mute ends the room as failed rather
    /// than leave the microphone open.
    LiveMicrophone(bool),
    /// Send the live room's sound to the loudspeaker (`true`) or the
    /// earpiece. Not answered.
    LiveSpeaker(bool),
    /// Send a message to everyone in the live room (ADR 0039). Not
    /// answered, and not sent when no room is joined or the payload is
    /// over [`MAX_LIVE_DATA_BYTES`].
    LiveData {
        /// What kind of message it is; the others receive it with this.
        topic: String,
        /// The message.
        payload: Vec<u8>,
        /// Delivered for certain and in order (`true`), or as fast as
        /// possible and perhaps not at all.
        reliable: bool,
    },
    /// Turn this device's camera on or off in the live room. Not answered:
    /// [`LiveEvent::VideoStarted`], `VideoStopped` or `MediaFailed`
    /// follows among the room's events.
    LiveCamera(bool),
    /// Change between the front and the back camera. Not answered; nothing
    /// happens while the camera is off.
    LiveSwitchCamera,
    /// Start or stop sharing this device's screen with the live room. Not
    /// answered: as for the camera. The user is asked each time.
    LiveScreenShare(bool),
    /// Send the live room's sound to this route (ADR 0042). Not answered:
    /// [`LiveEvent::AudioRoutes`] says where it went. Nothing happens when
    /// the device has no such route now.
    LiveAudioRoute(LiveAudioRoute),
}

impl ServiceRequest {
    /// True if the host answers this request with a [`ServiceResult`].
    pub fn expects_result(&self) -> bool {
        !matches!(
            self,
            Self::StopPlayback
                | Self::Store { .. }
                | Self::SetClipboard(_)
                | Self::Announce { .. }
                | Self::LiveLeave
                | Self::LiveMicrophone(_)
                | Self::LiveSpeaker(_)
                | Self::LiveData { .. }
                | Self::LiveCamera(_)
                | Self::LiveSwitchCamera
                | Self::LiveScreenShare(_)
                | Self::LiveAudioRoute(_)
        )
    }

    /// The answer to give when nothing can carry the request out: the
    /// `Unsupported` error of its kind, and "nothing stored" for a load.
    /// `None` for a request that is not answered.
    pub fn unsupported(&self) -> Option<ServiceResult> {
        Some(match self {
            Self::Http(_) => ServiceResult::Http(Err(HttpError::Unsupported)),
            Self::StartRecording => ServiceResult::RecordingStarted(Err(AudioError::Unsupported)),
            Self::StopRecording => ServiceResult::Recording(Err(AudioError::Unsupported)),
            Self::Play(_) => ServiceResult::Played(Err(AudioError::Unsupported)),
            Self::Load { .. } => ServiceResult::Loaded(None),
            Self::ReadClipboard => ServiceResult::Clipboard(None),
            Self::LiveJoin(_) => ServiceResult::Live(LiveEvent::Failed(LiveError::Unsupported)),
            Self::StopPlayback
            | Self::Store { .. }
            | Self::SetClipboard(_)
            | Self::Announce { .. }
            | Self::LiveLeave
            | Self::LiveMicrophone(_)
            | Self::LiveSpeaker(_)
            | Self::LiveData { .. }
            | Self::LiveCamera(_)
            | Self::LiveSwitchCamera
            | Self::LiveScreenShare(_)
            | Self::LiveAudioRoute(_) => return None,
        })
    }

    /// A short name for logs, without any of the request's contents.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Http(_) => "http",
            Self::StartRecording => "start recording",
            Self::StopRecording => "stop recording",
            Self::Play(_) => "play",
            Self::StopPlayback => "stop playback",
            Self::Store { .. } => "store",
            Self::Load { .. } => "load",
            Self::SetClipboard(_) => "set clipboard",
            Self::ReadClipboard => "read clipboard",
            Self::Announce { .. } => "announce",
            Self::LiveJoin(_) => "join live room",
            Self::LiveLeave => "leave live room",
            Self::LiveMicrophone(true) => "live microphone on",
            Self::LiveMicrophone(false) => "live microphone muted",
            Self::LiveSpeaker(true) => "live sound to the loudspeaker",
            Self::LiveSpeaker(false) => "live sound to the earpiece",
            Self::LiveData { .. } => "live data",
            Self::LiveCamera(true) => "live camera on",
            Self::LiveCamera(false) => "live camera off",
            Self::LiveSwitchCamera => "live camera switched",
            Self::LiveScreenShare(true) => "share the screen",
            Self::LiveScreenShare(false) => "stop sharing the screen",
            Self::LiveAudioRoute(LiveAudioRoute::Earpiece) => "live sound to the earpiece",
            Self::LiveAudioRoute(LiveAudioRoute::Speaker) => "live sound to the loudspeaker",
            Self::LiveAudioRoute(LiveAudioRoute::Wired) => "live sound to the wired headset",
            Self::LiveAudioRoute(LiveAudioRoute::Bluetooth) => {
                "live sound to the Bluetooth headset"
            }
        }
    }
}

/// The platform's answer to a [`ServiceRequest`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServiceResult {
    /// For [`ServiceRequest::Http`].
    Http(Result<HttpResponse, HttpError>),
    /// For [`ServiceRequest::StartRecording`].
    RecordingStarted(Result<(), AudioError>),
    /// For [`ServiceRequest::StopRecording`].
    Recording(Result<Recording, AudioError>),
    /// For [`ServiceRequest::Play`]: `Ok` when the clip played to its end.
    Played(Result<(), AudioError>),
    /// For [`ServiceRequest::Load`]: the value, or `None` if the key holds
    /// nothing.
    Loaded(Option<String>),
    /// For [`ServiceRequest::ReadClipboard`]: the clipboard's text, or
    /// `None` if it is empty, holds no text, or cannot be read here.
    Clipboard(Option<String>),
    /// For [`ServiceRequest::LiveJoin`]: one event of the room. Given
    /// under the join's id as often as there are events, until one that
    /// is [final](LiveEvent::is_final).
    Live(LiveEvent),
}

/// A request on its way to the host, with the id its answer must carry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceCall {
    /// Give this back with the answer.
    pub id: ServiceId,
    /// What to do.
    pub request: ServiceRequest,
}

/// What a host knows before the application starts and hands to it at
/// setup: an identifier for this installation, and the values stored on
/// earlier runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostState {
    install_id: String,
    stored: HashMap<String, String>,
}

impl HostState {
    /// No stored values and the placeholder id [`NO_INSTALL_ID`]: what an
    /// application sees when there is no host.
    pub fn new() -> Self {
        Self {
            install_id: NO_INSTALL_ID.to_owned(),
            stored: HashMap::new(),
        }
    }

    /// Sets the installation id. Anything that is not exactly 32
    /// hexadecimal digits is ignored, so that an application can rely on
    /// the shape of what it gets; letters are made lower case.
    pub fn with_install_id(mut self, id: &str) -> Self {
        if id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            self.install_id = id.to_ascii_lowercase();
        }
        self
    }

    /// Adds a stored value.
    pub fn with_stored(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.stored.insert(key.into(), value.into());
        self
    }

    /// The installation id: 32 lower-case hexadecimal digits (128 bits).
    pub fn install_id(&self) -> &str {
        &self.install_id
    }

    /// The value stored under `key` when the application started.
    pub fn stored(&self, key: &str) -> Option<&str> {
        self.stored.get(key).map(String::as_str)
    }

    /// How many values were stored.
    pub fn stored_len(&self) -> usize {
        self.stored.len()
    }
}

impl Default for HostState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_starts_with_the_default_timeout_and_nothing_else() {
        let request = HttpRequest::get("https://example.com");
        assert_eq!(request.method, HttpMethod::Get);
        assert_eq!(request.timeout, Duration::from_secs(30));
        assert!(request.headers.is_empty() && request.body.is_empty());
        assert_eq!(HttpRequest::post("x").method.as_str(), "POST");
        assert_eq!(HttpRequest::put("x").method.as_str(), "PUT");
        assert_eq!(HttpRequest::delete("x").method.as_str(), "DELETE");
    }

    #[test]
    fn a_json_body_declares_its_content_type() {
        let request = HttpRequest::post("https://example.com").json_body(b"{}".to_vec());
        assert_eq!(
            request.header_value("content-type"),
            Some("application/json")
        );
        assert_eq!(request.body, b"{}");
    }

    #[test]
    fn a_header_cannot_smuggle_another_header() {
        let request = HttpRequest::get("https://example.com")
            .header("X-Name", "a\r\nX-Admin: yes")
            .header("X-\nOther", "b");
        assert_eq!(
            request.headers,
            vec![
                ("X-Name".to_owned(), "aX-Admin: yes".to_owned()),
                ("X-Other".to_owned(), "b".to_owned()),
            ]
        );
    }

    #[test]
    fn response_headers_are_found_without_regard_to_case() {
        let response = HttpResponse::new(404)
            .with_header("Content-Type", "audio/mpeg")
            .with_body(b"no".to_vec());
        assert_eq!(response.header("content-type"), Some("audio/mpeg"));
        assert_eq!(response.header("etag"), None);
        assert!(!response.is_success());
        assert!(HttpResponse::new(204).is_success());
        assert_eq!(response.text(), Some("no"));
        assert_eq!(HttpResponse::new(200).with_body(vec![0xff]).text(), None);
    }

    #[test]
    fn every_answered_request_has_an_unsupported_answer_of_its_own_kind() {
        let clip = AudioClip::new(vec![1], "audio/mpeg");
        let cases = [
            (
                ServiceRequest::Http(HttpRequest::get("https://example.com")),
                Some(ServiceResult::Http(Err(HttpError::Unsupported))),
            ),
            (
                ServiceRequest::StartRecording,
                Some(ServiceResult::RecordingStarted(Err(
                    AudioError::Unsupported,
                ))),
            ),
            (
                ServiceRequest::StopRecording,
                Some(ServiceResult::Recording(Err(AudioError::Unsupported))),
            ),
            (
                ServiceRequest::Play(clip),
                Some(ServiceResult::Played(Err(AudioError::Unsupported))),
            ),
            (
                ServiceRequest::Load { key: "k".into() },
                Some(ServiceResult::Loaded(None)),
            ),
            (
                ServiceRequest::LiveJoin(LiveJoin::new("wss://live.example.com", "t")),
                Some(ServiceResult::Live(LiveEvent::Failed(
                    LiveError::Unsupported,
                ))),
            ),
            (ServiceRequest::LiveLeave, None),
            (ServiceRequest::LiveMicrophone(false), None),
            (ServiceRequest::LiveSpeaker(true), None),
            (ServiceRequest::StopPlayback, None),
            (
                ServiceRequest::Store {
                    key: "k".into(),
                    value: None,
                },
                None,
            ),
        ];
        for (request, expected) in cases {
            assert_eq!(request.expects_result(), expected.is_some(), "{request:?}");
            assert_eq!(request.unsupported(), expected);
        }
    }

    #[test]
    fn a_recording_becomes_a_clip_of_the_same_bytes() {
        let recording = Recording {
            bytes: vec![1, 2, 3],
            mime: "audio/mp4".into(),
            duration: Duration::from_secs(3),
        };
        assert_eq!(
            recording.into_clip(),
            AudioClip::new(vec![1, 2, 3], "audio/mp4")
        );
    }

    #[test]
    fn host_state_only_accepts_a_well_formed_install_id() {
        let state = HostState::new();
        assert_eq!(state.install_id(), NO_INSTALL_ID);
        assert_eq!(state.install_id().len(), 32);
        let state = state.with_install_id("not an id");
        assert_eq!(state.install_id(), NO_INSTALL_ID);
        let state = state.with_install_id("0123456789ABCDEF0123456789abcdef");
        assert_eq!(state.install_id(), "0123456789abcdef0123456789abcdef");
    }

    #[test]
    fn host_state_holds_the_stored_values() {
        let state = HostState::new().with_stored("device_id", "abc");
        assert_eq!(state.stored("device_id"), Some("abc"));
        assert_eq!(state.stored("other"), None);
        assert_eq!(state.stored_len(), 1);
    }

    #[test]
    fn a_join_starts_with_the_microphone_live_and_the_earpiece() {
        let join = LiveJoin::new("wss://live.example.com", "ticket");
        assert!(join.microphone && !join.speaker && !join.report_speaking);
        let join = join.microphone(false).speaker(true).report_speaking(true);
        assert!(!join.microphone && join.speaker && join.report_speaking);
    }

    #[test]
    fn a_join_never_prints_its_ticket_or_its_address() {
        let request = ServiceRequest::LiveJoin(LiveJoin::new(
            "wss://secret-host.example.com",
            "eyJhbGciOi.secret.ticket",
        ));
        let printed = format!("{request:?} {}", request.name());
        assert!(!printed.contains("secret"), "{printed}");
        assert!(!printed.contains("eyJ"), "{printed}");
        let call = ServiceCall {
            id: ServiceId(1),
            request,
        };
        assert!(!format!("{call:?}").contains("secret"));
    }

    #[test]
    fn only_the_two_endings_of_a_room_are_final() {
        let someone = LiveParticipant {
            identity: "baraka".into(),
            name: "Baraka".into(),
        };
        for event in [
            LiveEvent::Connected,
            LiveEvent::ParticipantJoined(someone),
            LiveEvent::ParticipantLeft("baraka".into()),
            LiveEvent::Speaking(vec!["baraka".into()]),
            LiveEvent::Reconnecting,
            LiveEvent::Reconnected,
        ] {
            assert!(!event.is_final(), "{event:?}");
        }
        for reason in [
            LiveDisconnect::Left,
            LiveDisconnect::Replaced,
            LiveDisconnect::Hidden,
            LiveDisconnect::Ended,
            LiveDisconnect::Lost,
        ] {
            assert!(LiveEvent::Disconnected(reason).is_final());
            assert!(!reason.to_string().is_empty());
        }
        for error in [
            LiveError::PermissionDenied,
            LiveError::Unsupported,
            LiveError::Failed("refused".into()),
        ] {
            assert!(!error.to_string().is_empty());
            assert!(LiveEvent::Failed(error).is_final());
        }
    }

    #[test]
    fn an_event_is_read_from_the_four_values_a_host_reports() {
        let read = LiveEvent::from_wire;
        assert_eq!(read(0, 0, "", ""), Some(LiveEvent::Connected));
        assert_eq!(
            read(1, 0, "baraka", "Baraka M."),
            Some(LiveEvent::ParticipantJoined(LiveParticipant {
                identity: "baraka".into(),
                name: "Baraka M.".into(),
            }))
        );
        assert_eq!(
            read(2, 0, "baraka", ""),
            Some(LiveEvent::ParticipantLeft("baraka".into()))
        );
        assert_eq!(
            read(3, 0, "baraka\nneema\n", ""),
            Some(LiveEvent::Speaking(vec!["baraka".into(), "neema".into()]))
        );
        assert_eq!(read(3, 0, "", ""), Some(LiveEvent::Speaking(Vec::new())));
        assert_eq!(read(4, 0, "", ""), Some(LiveEvent::Reconnecting));
        assert_eq!(read(5, 0, "", ""), Some(LiveEvent::Reconnected));
        let reasons = [
            LiveDisconnect::Left,
            LiveDisconnect::Replaced,
            LiveDisconnect::Hidden,
            LiveDisconnect::Ended,
            LiveDisconnect::Lost,
        ];
        for (code, reason) in reasons.into_iter().enumerate() {
            assert_eq!(
                read(6, code as u32, "", ""),
                Some(LiveEvent::Disconnected(reason))
            );
        }
        assert_eq!(
            read(7, 1, "", ""),
            Some(LiveEvent::Failed(LiveError::PermissionDenied))
        );
        assert_eq!(
            read(7, 2, "ignored", ""),
            Some(LiveEvent::Failed(LiveError::Unsupported))
        );
        assert_eq!(
            read(7, 3, "refused", ""),
            Some(LiveEvent::Failed(LiveError::Failed("refused".into())))
        );
    }

    #[test]
    fn an_unknown_ending_still_ends_the_room_and_an_unknown_kind_is_ignored() {
        let ended = LiveEvent::from_wire(6, 99, "", "").expect("still a disconnection");
        assert_eq!(ended, LiveEvent::Disconnected(LiveDisconnect::Ended));
        let failed = LiveEvent::from_wire(7, 99, "why", "").expect("still a failure");
        assert_eq!(failed, LiveEvent::Failed(LiveError::Failed("why".into())));
        assert!(ended.is_final() && failed.is_final());
        assert_eq!(LiveEvent::from_wire(12, 0, "", ""), None);
    }

    #[test]
    fn videos_and_their_failures_cross_the_boundary() {
        let read = LiveEvent::from_wire;
        let stream = |source, local, track| LiveVideoStream {
            identity: "baraka".into(),
            source,
            local,
            track: VideoTrack(track),
        };
        assert_eq!(
            read(8, 7 * 4, "baraka", ""),
            Some(LiveEvent::VideoStarted(stream(
                LiveVideoSource::Camera,
                false,
                7
            )))
        );
        assert_eq!(
            read(8, 7 * 4 + 3, "baraka", ""),
            Some(LiveEvent::VideoStarted(stream(
                LiveVideoSource::Screen,
                true,
                7
            )))
        );
        assert_eq!(
            read(9, 2 * 4 + 1, "baraka", ""),
            Some(LiveEvent::VideoStopped(stream(
                LiveVideoSource::Screen,
                false,
                2
            )))
        );
        assert_eq!(
            read(10, 2, "", ""),
            Some(LiveEvent::MediaFailed {
                source: LiveVideoSource::Camera,
                error: LiveError::PermissionDenied,
            })
        );
        assert_eq!(
            read(10, 3 * 2 + 1, "no projection", ""),
            Some(LiveEvent::MediaFailed {
                source: LiveVideoSource::Screen,
                error: LiveError::Failed("no projection".into()),
            })
        );
        for event in [read(8, 0, "", ""), read(9, 0, "", ""), read(10, 2, "", "")] {
            assert!(!event.expect("known").is_final(), "the room goes on");
        }
    }

    #[test]
    fn the_new_commands_are_not_answered_and_say_nothing_of_their_contents() {
        let data = ServiceRequest::LiveData {
            topic: "chat".into(),
            payload: b"habari".to_vec(),
            reliable: true,
        };
        for request in [
            data.clone(),
            ServiceRequest::LiveCamera(true),
            ServiceRequest::LiveSwitchCamera,
            ServiceRequest::LiveScreenShare(true),
        ] {
            assert!(!request.expects_result());
            assert_eq!(request.unsupported(), None);
        }
        assert_eq!(data.name(), "live data");
        let received = LiveEvent::Data(LiveData {
            from: "baraka".into(),
            topic: "chat".into(),
            payload: b"habari".to_vec(),
        });
        let text = format!("{received:?}");
        assert!(text.contains("chat") && !text.contains("baraka") && !text.contains("104"));
        assert!(
            LiveJoin::new("wss://live.example.com", "t")
                .camera(true)
                .camera
        );
    }

    #[test]
    fn a_background_notice_crosses_as_three_lines() {
        let notice = LiveBackground::new("Call with\nBaraka", "Tap to return").hang_up("Kata simu");
        assert_eq!(
            notice.to_wire(),
            "Call with Baraka\nTap to return\nKata simu"
        );
        let join = LiveJoin::new("wss://live.example.com", "ticket");
        assert_eq!(
            join.background, None,
            "a room ends with the screen unless asked"
        );
        let join = join.background(notice.clone());
        assert_eq!(join.background, Some(notice));
        assert!(format!("{join:?}").contains("background: true"));
    }

    #[test]
    fn hung_up_from_the_notification_is_a_final_event_of_its_own() {
        let event = LiveEvent::from_wire(6, 5, "", "").unwrap();
        assert_eq!(event, LiveEvent::Disconnected(LiveDisconnect::HungUp));
        assert!(event.is_final());
    }

    #[test]
    fn audio_routes_cross_as_one_number_and_back() {
        let none = LiveAudioRoutes::default();
        assert_eq!(none.to_wire(), 0);
        assert_eq!(
            LiveEvent::from_wire(11, 0, "", ""),
            Some(LiveEvent::AudioRoutes(none))
        );
        let routes = LiveAudioRoutes {
            available: vec![
                LiveAudioRoute::Earpiece,
                LiveAudioRoute::Speaker,
                LiveAudioRoute::Bluetooth,
            ],
            selected: Some(LiveAudioRoute::Bluetooth),
        };
        assert_eq!(routes.to_wire(), 4 + 16 + 32 + 128);
        assert_eq!(
            LiveEvent::from_wire(11, routes.to_wire(), "", ""),
            Some(LiveEvent::AudioRoutes(routes))
        );
        // A route this library has not heard of is no route, not a guess.
        assert_eq!(LiveAudioRoute::from_wire(9), None);
        for route in LiveAudioRoute::ALL {
            assert_eq!(
                LiveAudioRoute::from_wire(u32::from(route.to_wire())),
                Some(route)
            );
            let request = ServiceRequest::LiveAudioRoute(route);
            assert!(!request.expects_result());
            assert_eq!(request.unsupported(), None);
        }
        assert!(!LiveEvent::AudioRoutes(LiveAudioRoutes::default()).is_final());
    }
}
