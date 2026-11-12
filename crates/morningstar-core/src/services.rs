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
//! and the application-facing API in the UI layer (`Effects`). See ADR 0019.

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
}

impl ServiceRequest {
    /// True if the host answers this request with a [`ServiceResult`].
    pub fn expects_result(&self) -> bool {
        !matches!(
            self,
            Self::StopPlayback | Self::Store { .. } | Self::SetClipboard(_)
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
            Self::StopPlayback | Self::Store { .. } | Self::SetClipboard(_) => return None,
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
}
