//! The client of the Barua Talk server: every request the app sends and
//! every answer it understands (`server/API.md` is the contract).
//!
//! Nothing here touches the network or the app's state. [`Client`] builds
//! [`HttpRequest`]s; the `decode_*` functions turn what came back into
//! plain data or a [`Failure`]. `lib.rs` hands the requests to the platform
//! (`Application::effects`) and feeds the results to the app.
//!
//! Everything a server sends is untrusted. A body that is not the JSON the
//! contract describes is [`Failure::Malformed`], never a panic; every
//! string is cleaned of control characters and cut to a length the screens
//! can show ([`clean`], and [`clean_block`] for a message of several
//! lines); an id or an address that will go into a URL is checked to be
//! one before it is kept.
//!
//! The shape of this module, and the parts that are not about rooms and
//! calls (`clean`, `Failure`, the address and id helpers), are those of
//! `examples/atlas/src/api.rs`.

use std::time::Duration;

use morningstar::ui::prelude::{HttpError, HttpRequest, HttpResponse};

use crate::json::{self, Value};

/// Where the server is when nobody has said otherwise: the development
/// machine, which a phone reaches through `adb reverse tcp:8789 tcp:8789`.
///
/// A build for a deployed server names it at compile time instead:
/// `BARUA_TALK_SERVER_URL=https://… tools/build-android.sh`. The address
/// stays out of the repository that way.
pub const DEFAULT_BASE_URL: &str = match option_env!("BARUA_TALK_SERVER_URL") {
    Some(url) => url,
    None => "http://127.0.0.1:8789",
};

/// The most characters of a message the server accepts.
pub const MESSAGE_LIMIT: usize = 2000;
/// The most characters of a person's name the server accepts.
pub const NAME_LIMIT: usize = 40;
/// The most characters of a room's name the server accepts.
pub const ROOM_NAME_LIMIT: usize = 60;

// ----------------------------------------------------------------- limits

const TITLE_LIMIT: usize = 60;
const PREVIEW_LIMIT: usize = 80;
const ID_LIMIT: usize = 80;
const ERROR_LIMIT: usize = 140;
const LIST_LIMIT: usize = 100;
const NUMBER_LIMIT: usize = 24;
const RATE_LIMIT: usize = 28;
const TICKET_LIMIT: usize = 4096;

/// A run of characters with no space in it is broken after this many, so
/// that no text can be wider than the screen.
const WORD_LIMIT: usize = 28;

/// Characters that reorder or hide text: they have no place in a line.
const fn invisible(character: char) -> bool {
    matches!(character, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}')
}

/// Text from the server made safe to show on one line: control characters
/// and line breaks become spaces, runs of spaces one space, the ends are
/// trimmed, a "word" longer than a line is broken, and at most `limit`
/// characters are kept (the last of them an ellipsis when something was
/// cut).
pub fn clean(text: &str, limit: usize) -> String {
    let mut out = String::new();
    let mut count = 0;
    let mut run = 0;
    let mut pending_space = false;
    let mut cut = false;
    for character in text.chars() {
        if character.is_whitespace() || character.is_control() {
            pending_space = count > 0;
            run = 0;
            continue;
        }
        if invisible(character) {
            continue;
        }
        let space = pending_space || run >= WORD_LIMIT;
        if count + usize::from(space) + 1 > limit {
            cut = true;
            break;
        }
        if space {
            out.push(' ');
            count += 1;
            run = 0;
        }
        pending_space = false;
        out.push(character);
        count += 1;
        run += 1;
    }
    if cut && limit > 0 {
        if count >= limit {
            out.pop();
        }
        out.push('\u{2026}');
    }
    out
}

/// Text of several lines from the server made safe to show: line breaks
/// are kept (at most one blank line in a row, none at the ends), tabs
/// become two spaces, other control characters and characters that hide
/// or reorder text are dropped, spaces at the end of a line go, and at
/// most `limit` characters are kept (the last an ellipsis when something
/// was cut). Spaces inside a line are kept as they are: they are the
/// indentation of code.
pub fn clean_block(text: &str, limit: usize) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut count = 0;
    let mut cut = false;
    'lines: for raw in text.split('\n') {
        let mut line = String::new();
        for character in raw.chars() {
            let character = match character {
                '\t' => {
                    line.push(' ');
                    ' '
                }
                c if c.is_control() || invisible(c) => continue,
                c if c.is_whitespace() => ' ',
                c => c,
            };
            line.push(character);
        }
        let line = line.trim_end().to_owned();
        let blank = line.is_empty();
        if blank && lines.last().is_none_or(String::is_empty) {
            continue;
        }
        let cost = line.chars().count() + 1;
        if count + cost > limit {
            let room = limit.saturating_sub(count + 1);
            if room > 0 {
                lines.push(line.chars().take(room).collect());
            }
            cut = true;
            break 'lines;
        }
        count += cost;
        lines.push(line);
    }
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    let mut out = lines.join("\n");
    if cut {
        out.push('\u{2026}');
    }
    out
}

// --------------------------------------------------------------- failures

/// Why a request gave nothing to use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    /// No connection to the server.
    Offline,
    /// No answer in time.
    Timeout,
    /// This device cannot make requests at all (no host behind the app).
    Unsupported,
    /// The request could not be made: a bad address, a refused scheme.
    Network(String),
    /// The server answered with an error.
    Server {
        /// The HTTP status.
        status: u16,
        /// The contract's error code (`upstream_failed`), or empty.
        code: String,
        /// The server's sentence for a person, cleaned; may be empty.
        message: String,
        /// Seconds to wait, from `Retry-After`.
        retry_after: Option<u32>,
    },
    /// The server answered with something the contract does not describe.
    Malformed(&'static str),
}

impl Failure {
    /// True when the server was not reached at all: the app is offline.
    pub fn unreachable(&self) -> bool {
        matches!(
            self,
            Self::Offline | Self::Timeout | Self::Unsupported | Self::Network(_)
        )
    }

    /// The HTTP status, if the server answered with an error.
    pub fn status(&self) -> Option<u16> {
        match self {
            Self::Server { status, .. } => Some(*status),
            _ => None,
        }
    }

    /// What happened, in plain words, as one short sentence.
    pub fn sentence(&self) -> String {
        match self {
            Self::Offline => "The server could not be reached.".to_owned(),
            Self::Timeout => "The server took too long to answer.".to_owned(),
            Self::Unsupported => "This device cannot make network requests.".to_owned(),
            Self::Network(reason) => format!("The request could not be made: {reason}."),
            Self::Malformed(_) => {
                "The server sent something this app does not understand.".to_owned()
            }
            Self::Server {
                status,
                code,
                message,
                retry_after,
            } => match status {
                // The contract: "`message` can be shown as it is."
                _ if !message.is_empty() && *status != 401 && *status != 429 => message.clone(),
                401 => "The server refused this app: it wants a token.".to_owned(),
                404 => "The server has no such room, person or call.".to_owned(),
                413 => "That was too large to send.".to_owned(),
                429 => match retry_after {
                    Some(seconds) => {
                        format!("Too many requests. Wait {seconds} seconds and try again.")
                    }
                    None => "Too many requests. Wait a moment and try again.".to_owned(),
                },
                402 => "The workspace balance is empty.".to_owned(),
                502 => "Barua Talk did not answer the server.".to_owned(),
                503 if code == "database_unavailable" => {
                    "The server\u{2019}s database is not answering.".to_owned()
                }
                503 => "The server cannot do that right now.".to_owned(),
                _ => format!("The server had a problem ({status})."),
            },
        }
    }
}

impl From<HttpError> for Failure {
    fn from(error: HttpError) -> Self {
        match error {
            HttpError::Offline => Self::Offline,
            HttpError::Timeout => Self::Timeout,
            HttpError::Unsupported => Self::Unsupported,
            HttpError::InvalidUrl => Self::Network("the address is not a valid one".to_owned()),
            HttpError::TooLarge => Self::Malformed("an answer larger than 10 MB"),
            HttpError::Failed(reason) => Self::Network(clean(&reason, 80)),
        }
    }
}

/// What a platform answer holds: the successful response, or the failure
/// a transport error or an error status is.
fn successful(result: Result<HttpResponse, HttpError>) -> Result<HttpResponse, Failure> {
    let response = result?;
    if response.is_success() {
        return Ok(response);
    }
    // `{"error": {"code": "snake_case", "message": "for a person"}}`, when
    // the body is that; a proxy's HTML error page is just its status.
    let error = response
        .text()
        .and_then(|text| json::parse(text).ok())
        .and_then(|body| body.get("error").cloned());
    let field = |name: &str, limit: usize| {
        error
            .as_ref()
            .and_then(|error| error.get(name))
            .and_then(Value::as_str)
            .map(|text| clean(text, limit))
            .unwrap_or_default()
    };
    Err(Failure::Server {
        status: response.status,
        code: field("code", 40),
        message: field("message", ERROR_LIMIT),
        retry_after: response
            .header("retry-after")
            .and_then(|value| value.trim().parse::<u32>().ok())
            .map(|seconds| seconds.min(3600)),
    })
}

/// The JSON object of a successful answer.
fn object(result: Result<HttpResponse, HttpError>) -> Result<Value, Failure> {
    let response = successful(result)?;
    let text = response
        .text()
        .ok_or(Failure::Malformed("a body that is not UTF-8"))?;
    let value = json::parse(text).map_err(|_| Failure::Malformed("a body that is not JSON"))?;
    match value {
        Value::Object(_) => Ok(value),
        _ => Err(Failure::Malformed("JSON that is not an object")),
    }
}

// ------------------------------------------------------------ field readers

type Field<T> = Result<T, Failure>;

fn text(value: &Value, name: &'static str, limit: usize) -> Field<String> {
    value
        .get(name)
        .and_then(Value::as_str)
        .map(|text| clean(text, limit))
        .ok_or(Failure::Malformed(name))
}

fn filled(value: &Value, name: &'static str, limit: usize) -> Field<String> {
    let text = text(value, name, limit)?;
    if text.is_empty() {
        Err(Failure::Malformed(name))
    } else {
        Ok(text)
    }
}

/// A message's text: several lines, never empty.
fn block(value: &Value, name: &'static str) -> Field<String> {
    let text = value
        .get(name)
        .and_then(Value::as_str)
        .map(|text| clean_block(text, MESSAGE_LIMIT))
        .ok_or(Failure::Malformed(name))?;
    if text.trim().is_empty() {
        Err(Failure::Malformed(name))
    } else {
        Ok(text)
    }
}

fn integer(value: &Value, name: &'static str, low: u32, high: u32) -> Field<u32> {
    value
        .get(name)
        .and_then(|number| number.as_integer(low, high))
        .ok_or(Failure::Malformed(name))
}

fn boolean(value: &Value, name: &'static str) -> Field<bool> {
    value
        .get(name)
        .and_then(Value::as_bool)
        .ok_or(Failure::Malformed(name))
}

// ---------------------------------------------------------- the address

/// `segment` percent-encoded for use as one segment of a URL's path:
/// everything but the unreserved characters of RFC 3986 is escaped.
pub fn encode_segment(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Why an address typed on the Server screen cannot be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UrlProblem {
    /// Nothing was typed.
    Empty,
    /// It does not start with `http://` or `https://`.
    Scheme,
    /// There is no host, or there are characters an address cannot have.
    Host,
}

impl UrlProblem {
    /// The problem in plain words.
    pub const fn sentence(self) -> &'static str {
        match self {
            Self::Empty => "Type the server\u{2019}s address first.",
            Self::Scheme => "The address must start with http:// or https://.",
            Self::Host => "That is not an address this app can use.",
        }
    }
}

/// A server address as typed, made into the base URL requests are built
/// on: trimmed, the scheme in lower case, no path beyond an optional
/// prefix, no trailing slash, no query, no user name.
pub fn normalize_base_url(typed: &str) -> Result<String, UrlProblem> {
    let typed = typed.trim();
    if typed.is_empty() {
        return Err(UrlProblem::Empty);
    }
    let (scheme, rest) = match typed.split_once("://") {
        Some((scheme, rest)) if scheme.eq_ignore_ascii_case("http") => ("http", rest),
        Some((scheme, rest)) if scheme.eq_ignore_ascii_case("https") => ("https", rest),
        _ => return Err(UrlProblem::Scheme),
    };
    let rest = rest.trim_end_matches('/');
    let host = rest.split('/').next().unwrap_or_default();
    let allowed = |byte: u8| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'~' | b':' | b'/')
    };
    let bracketed = host.starts_with('[') && host.contains(']');
    let fine = !host.is_empty()
        && rest.len() <= 200
        && !rest.contains("//")
        && !rest.contains("..")
        && (bracketed
            || (rest.bytes().all(allowed) && host.bytes().any(|b| b.is_ascii_alphanumeric())))
        && rest
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && !matches!(byte, b'?' | b'#' | b'@' | b'\\'));
    if fine {
        Ok(format!("{scheme}://{rest}"))
    } else {
        Err(UrlProblem::Host)
    }
}

/// An id as the contract wants it: 8 to 64 characters of `[A-Za-z0-9_-]`.
/// The host's installation id (32 hex digits) is one already; anything
/// else is cut and padded into shape rather than sent as it is.
pub fn device_id(prefix: &str, install_id: &str) -> String {
    let mut id: String = prefix
        .chars()
        .chain(install_id.chars())
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        .take(64)
        .collect();
    while id.len() < 8 {
        id.push('0');
    }
    id
}

/// A message as the server will take it: trimmed, and cut to the most it
/// accepts.
pub fn tidy_message(text: &str) -> String {
    text.trim().chars().take(MESSAGE_LIMIT).collect()
}

/// A string the app sends back as it is (`client_id`, `request_id`): 8 to
/// 64 characters of `[A-Za-z0-9_-]`, made from `prefix`, the installation
/// and a number that never repeats on this phone.
pub fn request_id(prefix: &str, device_id: &str, serial: u32) -> String {
    let device: String = device_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(12)
        .collect();
    format!("{prefix}-{device}-{serial}")
}

/// A person's name as the server will take it: one line, trimmed, at most
/// forty characters.
pub fn tidy_name(text: &str) -> String {
    clean(text, NAME_LIMIT + 1)
        .chars()
        .take(NAME_LIMIT)
        .collect::<String>()
        .trim()
        .to_owned()
}

// ------------------------------------------------------------ field readers

/// An id the app will put into a URL or send back: short and printable.
fn token_like(text: &str) -> bool {
    !text.is_empty()
        && text.chars().count() <= ID_LIMIT
        && text.chars().all(|c| c.is_ascii_graphic())
}

fn id(value: &Value, name: &'static str) -> Field<String> {
    value
        .get(name)
        .and_then(Value::as_str)
        .filter(|id| token_like(id))
        .map(str::to_owned)
        .ok_or(Failure::Malformed(name))
}

/// A whole number that may be larger than a `u32`: a message's id.
fn big(value: &Value, name: &'static str) -> Field<u64> {
    value
        .get(name)
        .and_then(Value::as_number)
        .filter(|number| number.fract() == 0.0 && (0.0..9.0e15).contains(number))
        .map(|number| number as u64)
        .ok_or(Failure::Malformed(name))
}

/// A string that may be `null` or missing: cleaned, and `None` when
/// empty.
fn maybe(value: &Value, name: &str, limit: usize) -> Option<String> {
    value
        .get(name)
        .and_then(Value::as_str)
        .map(|text| clean(text, limit))
        .filter(|text| !text.is_empty())
}

/// A time as the server writes it, kept as text for
/// [`ago`](crate::model::ago); empty when it is not there.
fn time(value: &Value, name: &str) -> String {
    maybe(value, name, 40).unwrap_or_default()
}

fn list<'a>(value: &'a Value, name: &'static str) -> Field<&'a [Value]> {
    value
        .get(name)
        .and_then(Value::as_array)
        .ok_or(Failure::Malformed(name))
}

/// The id of the recording that is running, from a `recording` member
/// that is `null` or `{"id": "…", "status": "recording"}`.
fn running_recording(value: &Value) -> Option<String> {
    let recording = value.get("recording")?;
    let running = recording.get("status").and_then(Value::as_str) == Some("recording");
    running.then(|| id(recording, "id").ok()).flatten()
}

// ---------------------------------------------------------------- answers

/// A LiveKit ticket: what a media client would join a room with. The app
/// cannot use one yet; it keeps it for the day it can.
#[derive(Clone, PartialEq, Eq)]
pub struct Ticket {
    /// The token.
    pub token: String,
    /// Where to connect (`wss://…`).
    pub url: String,
}

impl std::fmt::Debug for Ticket {
    // A token is a key: it is never printed.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Ticket({} characters, {})", self.token.len(), self.url)
    }
}

/// The ticket of an answer, if it carries a usable one.
fn ticket(body: &Value) -> Option<Ticket> {
    let take = |name: &str| {
        body.get(name)
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty() && text.len() <= TICKET_LIMIT)
            .filter(|text| text.chars().all(|c| c.is_ascii_graphic()))
            .map(str::to_owned)
    };
    let (token, url) = (take("token")?, take("url")?);
    (url.starts_with("wss://") || url.starts_with("ws://")).then_some(Ticket { token, url })
}

/// `GET /v1/health`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Health {
    /// The server has a Barua key and its database answers.
    pub ok: bool,
    /// It has a Barua key: rooms can be made and calls can ring.
    pub talk: bool,
    /// Its database answers.
    pub database: bool,
    /// Phone calls look usable.
    pub phone: bool,
}

/// Decodes `GET /v1/health`.
pub fn decode_health(result: Result<HttpResponse, HttpError>) -> Result<Health, Failure> {
    let body = object(result)?;
    Ok(Health {
        ok: boolean(&body, "ok")?,
        talk: boolean(&body, "talk")?,
        database: boolean(&body, "database")?,
        phone: boolean(&body, "phone")?,
    })
}

/// A person as the server names them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Peer {
    /// Their address.
    pub identity: String,
    /// What they are called (their identity while they have no name).
    pub name: String,
}

fn peer(value: &Value) -> Field<Peer> {
    let identity = id(value, "identity")?;
    let name = maybe(value, "name", NAME_LIMIT).unwrap_or_else(|| identity.clone());
    Ok(Peer { identity, name })
}

/// Decodes `PUT /v1/me`: who this installation is.
pub fn decode_me(result: Result<HttpResponse, HttpError>) -> Result<Peer, Failure> {
    peer(&object(result)?)
}

/// A row of `GET /v1/people`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonRow {
    /// Who.
    pub peer: Peer,
    /// Their app is open.
    pub online: bool,
}

/// Decodes `GET /v1/people`.
pub fn decode_people(result: Result<HttpResponse, HttpError>) -> Result<Vec<PersonRow>, Failure> {
    let body = object(result)?;
    list(&body, "people")?
        .iter()
        .take(LIST_LIMIT)
        .map(|row| {
            Ok(PersonRow {
                peer: peer(row)?,
                online: boolean(row, "online")?,
            })
        })
        .collect()
}

/// `GET /v1/account`: what the Phone call screen shows around the call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    /// Phone calls look usable.
    pub phone: bool,
    /// The number the other phone sees.
    pub caller_id: Option<String>,
    /// "TSh 300 / minute".
    pub phone_rate: String,
    /// "TSh 30 / minute".
    pub recording_rate: String,
}

fn rate_text(value: Option<&Value>) -> Option<String> {
    maybe(value?, "text", RATE_LIMIT)
}

/// Decodes `GET /v1/account`.
pub fn decode_account(result: Result<HttpResponse, HttpError>) -> Result<Account, Failure> {
    let body = object(result)?;
    let phone = body.get("phone").ok_or(Failure::Malformed("phone"))?;
    let rates = body.get("rates").ok_or(Failure::Malformed("rates"))?;
    Ok(Account {
        phone: boolean(phone, "enabled")?,
        caller_id: maybe(phone, "caller_id", NUMBER_LIMIT),
        phone_rate: rate_text(rates.get("phone_call")).ok_or(Failure::Malformed("rates"))?,
        recording_rate: rate_text(rates.get("recording")).ok_or(Failure::Malformed("rates"))?,
    })
}

/// A room as the server describes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomRow {
    /// Its address.
    pub name: String,
    /// What it is called.
    pub display_name: String,
    /// How many people are members.
    pub members: u32,
    /// Something happened in it in the last ten minutes.
    pub live: bool,
    /// The id of the recording of it that is running.
    pub recording: Option<String>,
}

fn room(value: &Value) -> Field<RoomRow> {
    Ok(RoomRow {
        name: id(value, "name")?,
        display_name: filled(value, "display_name", TITLE_LIMIT)?,
        members: integer(value, "members", 0, 1_000_000)?,
        live: boolean(value, "live")?,
        recording: running_recording(value),
    })
}

/// Decodes `GET /v1/rooms`: most recently active first.
pub fn decode_rooms(result: Result<HttpResponse, HttpError>) -> Result<Vec<RoomRow>, Failure> {
    let body = object(result)?;
    list(&body, "rooms")?
        .iter()
        .take(LIST_LIMIT)
        .map(room)
        .collect()
}

/// Decodes `POST /v1/rooms`: the room made, or the one that was there.
pub fn decode_room(result: Result<HttpResponse, HttpError>) -> Result<RoomRow, Failure> {
    let body = object(result)?;
    room(body.get("room").ok_or(Failure::Malformed("room"))?)
}

/// A message of a room's chat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageRow {
    /// Its id: a later message has a larger one.
    pub id: u64,
    /// What its author is called.
    pub from_name: String,
    /// This person wrote it.
    pub mine: bool,
    /// What it says.
    pub text: String,
    /// When it was written.
    pub created_at: String,
}

fn message(value: &Value) -> Field<MessageRow> {
    let from = id(value, "from")?;
    Ok(MessageRow {
        id: big(value, "id")?,
        from_name: maybe(value, "from_name", NAME_LIMIT).unwrap_or(from),
        mine: boolean(value, "mine")?,
        text: block(value, "text")?,
        created_at: time(value, "created_at"),
    })
}

/// `GET /v1/rooms/{name}/messages`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Messages {
    /// The page, oldest first.
    pub messages: Vec<MessageRow>,
    /// The largest id in the room.
    pub last_id: u64,
    /// More remain after this page: ask again at once.
    pub more: bool,
    /// The server's clock.
    pub now: String,
}

/// Decodes `GET /v1/rooms/{name}/messages`.
pub fn decode_messages(result: Result<HttpResponse, HttpError>) -> Result<Messages, Failure> {
    let body = object(result)?;
    Ok(Messages {
        messages: list(&body, "messages")?
            .iter()
            .take(LIST_LIMIT)
            .map(message)
            .collect::<Field<_>>()?,
        last_id: big(&body, "last_id")?,
        more: boolean(&body, "more")?,
        now: time(&body, "now"),
    })
}

/// Decodes `POST /v1/rooms/{name}/messages`: the message as stored.
pub fn decode_posted(result: Result<HttpResponse, HttpError>) -> Result<MessageRow, Failure> {
    let body = object(result)?;
    message(body.get("message").ok_or(Failure::Malformed("message"))?)
}

/// Decodes `POST /v1/rooms/{name}/ticket`.
pub fn decode_ticket(result: Result<HttpResponse, HttpError>) -> Result<Ticket, Failure> {
    ticket(&object(result)?).ok_or(Failure::Malformed("token"))
}

/// How far an in-app call has got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallState {
    /// Ringing.
    Ringing,
    /// Answered.
    Active,
    /// The person it rang for said no.
    Declined,
    /// It was answered and is over.
    Ended,
    /// It never was answered.
    Missed,
}

impl CallState {
    /// True once the call is over, however it ended.
    pub fn over(self) -> bool {
        matches!(self, Self::Declined | Self::Ended | Self::Missed)
    }
}

/// An in-app call, from this person's side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallRow {
    /// Its id.
    pub id: String,
    /// The other person rang this one.
    pub incoming: bool,
    /// With video.
    pub video: bool,
    /// How far it has got.
    pub status: CallState,
    /// The other person.
    pub peer: Peer,
    /// Seconds of talk.
    pub seconds: u32,
    /// The id of its recording that is running.
    pub recording: Option<String>,
}

fn call(value: &Value) -> Field<CallRow> {
    Ok(CallRow {
        id: id(value, "id")?,
        incoming: match value.get("direction").and_then(Value::as_str) {
            Some("incoming") => true,
            Some("outgoing") => false,
            _ => return Err(Failure::Malformed("direction")),
        },
        video: boolean(value, "video")?,
        status: match value.get("status").and_then(Value::as_str) {
            Some("ringing") => CallState::Ringing,
            Some("active") => CallState::Active,
            Some("declined") => CallState::Declined,
            Some("ended") => CallState::Ended,
            Some("missed") => CallState::Missed,
            _ => return Err(Failure::Malformed("status")),
        },
        peer: peer(value.get("peer").ok_or(Failure::Malformed("peer"))?)?,
        seconds: integer(value, "seconds", 0, 10_000_000)?,
        recording: running_recording(value),
    })
}

/// Decodes an answer that holds one call and perhaps a ticket: `POST
/// /v1/calls`, `GET /v1/calls/{id}`, accept, decline and hang up.
pub fn decode_call(
    result: Result<HttpResponse, HttpError>,
) -> Result<(CallRow, Option<Ticket>), Failure> {
    let body = object(result)?;
    let row = call(body.get("call").ok_or(Failure::Malformed("call"))?)?;
    Ok((row, ticket(&body)))
}

/// Decodes `GET /v1/calls/incoming`: the calls ringing for this person,
/// oldest first.
pub fn decode_incoming(result: Result<HttpResponse, HttpError>) -> Result<Vec<CallRow>, Failure> {
    let body = object(result)?;
    list(&body, "calls")?
        .iter()
        .take(LIST_LIMIT)
        .map(call)
        .collect()
}

/// How far a call to a phone has got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhoneState {
    /// The other phone rings.
    Ringing,
    /// Answered.
    Active,
    /// Over, after being answered.
    Ended,
    /// Not answered, busy or cancelled.
    Missed,
    /// It could not be placed.
    Failed,
}

impl PhoneState {
    /// True once the call is over.
    pub fn over(self) -> bool {
        matches!(self, Self::Ended | Self::Missed | Self::Failed)
    }
}

/// A call to a phone number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhoneRow {
    /// Its id.
    pub id: String,
    /// The number called.
    pub to: String,
    /// The number the other phone sees.
    pub caller_id: Option<String>,
    /// How far it has got.
    pub status: PhoneState,
    /// Seconds of talk.
    pub seconds: u32,
    /// What it cost, once it is over.
    pub price_tzs: Option<u32>,
    /// "TSh 300 / minute".
    pub rate: Option<String>,
    /// The server can end it.
    pub can_hang_up: bool,
}

fn phone(value: &Value) -> Field<PhoneRow> {
    Ok(PhoneRow {
        id: id(value, "id")?,
        to: filled(value, "to", NUMBER_LIMIT)?,
        caller_id: maybe(value, "caller_id", NUMBER_LIMIT),
        status: match value.get("status").and_then(Value::as_str) {
            Some("ringing") => PhoneState::Ringing,
            Some("active") => PhoneState::Active,
            Some("ended") => PhoneState::Ended,
            Some("missed") => PhoneState::Missed,
            Some("failed") => PhoneState::Failed,
            _ => return Err(Failure::Malformed("status")),
        },
        seconds: integer(value, "seconds", 0, 10_000_000)?,
        price_tzs: value
            .get("price_tzs")
            .and_then(|price| price.as_integer(0, 100_000_000)),
        rate: rate_text(value.get("rate")),
        can_hang_up: boolean(value, "can_hang_up")?,
    })
}

/// A phone call, a ticket if one came with it, and what the server says
/// about hanging up ("The call ends when the other side hangs up.").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhoneAnswer {
    /// The call.
    pub call: PhoneRow,
    /// The ticket into its room.
    pub ticket: Option<Ticket>,
    /// The server's sentence, if it sent one.
    pub message: Option<String>,
}

/// Decodes an answer that holds one phone call: `POST /v1/phone-calls`,
/// `GET /v1/phone-calls/{id}` and hang up.
pub fn decode_phone(result: Result<HttpResponse, HttpError>) -> Result<PhoneAnswer, Failure> {
    let body = object(result)?;
    Ok(PhoneAnswer {
        call: phone(body.get("call").ok_or(Failure::Malformed("call"))?)?,
        ticket: ticket(&body),
        message: maybe(&body, "message", ERROR_LIMIT),
    })
}

/// Decodes an answer that holds one recording (`POST /v1/recordings`,
/// stopping one): its id, and whether it is running.
pub fn decode_recording(
    result: Result<HttpResponse, HttpError>,
) -> Result<(String, bool), Failure> {
    let body = object(result)?;
    let recording = body
        .get("recording")
        .ok_or(Failure::Malformed("recording"))?;
    let status = recording
        .get("status")
        .and_then(Value::as_str)
        .ok_or(Failure::Malformed("status"))?;
    Ok((id(recording, "id")?, status == "recording"))
}

/// What a row of `GET /v1/activity` is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityKind {
    /// A room.
    Room,
    /// An in-app call.
    Call,
    /// A call to a phone.
    Phone,
}

/// A row of `GET /v1/activity`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityRow {
    /// What it is about.
    pub kind: ActivityKind,
    /// The room's address, or the call's id.
    pub id: String,
    /// The room's name, the other person's, or the phone number.
    pub title: String,
    /// One line to show as it is.
    pub preview: String,
    /// When.
    pub at: String,
    /// Unread messages (rooms only).
    pub unread: u32,
    /// The call's status, or `open` for a room.
    pub status: String,
    /// With video.
    pub video: bool,
    /// The other person of an in-app call.
    pub peer: Option<Peer>,
}

/// Decodes `GET /v1/activity`: the rows, newest first, and the server's
/// clock.
pub fn decode_activity(
    result: Result<HttpResponse, HttpError>,
) -> Result<(Vec<ActivityRow>, String), Failure> {
    let body = object(result)?;
    let rows = list(&body, "items")?
        .iter()
        .take(LIST_LIMIT)
        .map(|row| {
            Ok(ActivityRow {
                kind: match row.get("kind").and_then(Value::as_str) {
                    Some("room") => ActivityKind::Room,
                    Some("call") => ActivityKind::Call,
                    Some("phone") => ActivityKind::Phone,
                    _ => return Err(Failure::Malformed("kind")),
                },
                id: id(row, "id")?,
                title: filled(row, "title", TITLE_LIMIT)?,
                preview: text(row, "preview", PREVIEW_LIMIT)?,
                at: time(row, "at"),
                unread: integer(row, "unread", 0, 1_000_000)?,
                status: text(row, "status", 20)?,
                video: row.get("video").and_then(Value::as_bool).unwrap_or(false),
                peer: row.get("peer").and_then(|value| peer(value).ok()),
            })
        })
        .collect::<Field<_>>()?;
    Ok((rows, time(&body, "now")))
}

// ----------------------------------------------------------------- client

/// What a recording is of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recorded<'a> {
    /// A room, by its address.
    Room(&'a str),
    /// An in-app call, by its id.
    Call(&'a str),
}

/// Builds the requests of one installation to one server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Client {
    base: String,
    device_id: String,
}

impl Client {
    /// A client of the server at `base` (as [`normalize_base_url`] returns
    /// it) for the installation `device_id`.
    pub fn new(base: impl Into<String>, device_id: impl Into<String>) -> Self {
        Self {
            base: base.into(),
            device_id: device_id.into(),
        }
    }

    /// The server's base URL.
    pub fn base(&self) -> &str {
        &self.base
    }

    /// The installation's id, sent as `X-Device-Id`.
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    fn signed(&self, request: HttpRequest, seconds: u64) -> HttpRequest {
        request
            .header("X-Device-Id", self.device_id.clone())
            .header("Accept", "application/json")
            .timeout(Duration::from_secs(seconds))
    }

    /// A question the server answers from its own state: at once.
    fn ask(&self, path: &str) -> HttpRequest {
        self.signed(HttpRequest::get(self.url(path)), 8)
    }

    /// An action, which may go upstream: the contract allows 20 seconds.
    fn act(&self, path: &str, body: String) -> HttpRequest {
        self.signed(
            HttpRequest::post(self.url(path)).json_body(body.into_bytes()),
            20,
        )
    }

    /// `GET /v1/health`. Short: it decides between online and offline.
    pub fn health(&self) -> HttpRequest {
        HttpRequest::get(self.url("/v1/health"))
            .header("Accept", "application/json")
            .timeout(Duration::from_secs(6))
    }

    /// `PUT /v1/me`: this installation's name; the first one also gives
    /// it its identity.
    pub fn put_me(&self, name: &str) -> HttpRequest {
        let body = format!("{{\"name\":{}}}", json::string(&tidy_name(name)));
        self.signed(
            HttpRequest::put(self.url("/v1/me")).json_body(body.into_bytes()),
            20,
        )
    }

    /// `GET /v1/people`.
    pub fn people(&self) -> HttpRequest {
        self.ask("/v1/people")
    }

    /// `GET /v1/account`.
    pub fn account(&self) -> HttpRequest {
        self.ask("/v1/account")
    }

    /// `GET /v1/rooms`.
    pub fn rooms(&self) -> HttpRequest {
        self.ask("/v1/rooms")
    }

    /// `GET /v1/activity`.
    pub fn activity(&self) -> HttpRequest {
        self.ask("/v1/activity")
    }

    /// `POST /v1/rooms`.
    pub fn create_room(&self, display_name: &str) -> HttpRequest {
        self.act(
            "/v1/rooms",
            format!("{{\"display_name\":{}}}", json::string(display_name)),
        )
    }

    fn room_path(room: &str, tail: &str) -> String {
        format!("/v1/rooms/{}{tail}", encode_segment(room))
    }

    /// `GET /v1/rooms/{name}/messages`, moving the read mark: the latest
    /// page, or what came after message `after`.
    pub fn messages(&self, room: &str, after: Option<u64>) -> HttpRequest {
        let query = match after {
            Some(id) => format!("/messages?after={id}&read=1"),
            None => "/messages?read=1".to_owned(),
        };
        self.ask(&Self::room_path(room, &query))
    }

    /// `POST /v1/rooms/{name}/messages`. Sending it again with the same
    /// `client_id` stores nothing twice.
    pub fn post_message(&self, room: &str, text: &str, client_id: &str) -> HttpRequest {
        self.act(
            &Self::room_path(room, "/messages"),
            format!(
                "{{\"text\":{},\"client_id\":{}}}",
                json::string(text),
                json::string(client_id)
            ),
        )
    }

    /// `POST /v1/rooms/{name}/ticket`.
    pub fn ticket(&self, room: &str, video: bool) -> HttpRequest {
        self.act(
            &Self::room_path(room, "/ticket"),
            format!("{{\"video\":{video}}}"),
        )
    }

    /// `POST /v1/calls`: rings `to`.
    pub fn ring(&self, to: &str, video: bool) -> HttpRequest {
        self.act(
            "/v1/calls",
            format!("{{\"to\":{},\"video\":{video}}}", json::string(to)),
        )
    }

    /// `GET /v1/calls/incoming`: the poll that makes this phone ring.
    pub fn incoming(&self) -> HttpRequest {
        self.ask("/v1/calls/incoming")
    }

    fn call_path(id: &str, tail: &str) -> String {
        format!("/v1/calls/{}{tail}", encode_segment(id))
    }

    /// `GET /v1/calls/{id}`.
    pub fn call(&self, id: &str) -> HttpRequest {
        self.ask(&Self::call_path(id, ""))
    }

    /// `POST /v1/calls/{id}/accept`.
    pub fn accept(&self, id: &str) -> HttpRequest {
        self.act(&Self::call_path(id, "/accept"), "{}".to_owned())
    }

    /// `POST /v1/calls/{id}/decline`.
    pub fn decline(&self, id: &str) -> HttpRequest {
        self.act(&Self::call_path(id, "/decline"), "{}".to_owned())
    }

    /// `POST /v1/calls/{id}/hangup`.
    pub fn hang_up(&self, id: &str) -> HttpRequest {
        self.act(&Self::call_path(id, "/hangup"), "{}".to_owned())
    }

    /// `POST /v1/phone-calls`. `request_id` is new for every press of the
    /// dial button, so that the same press never dials, or pays, twice.
    pub fn dial(&self, to: &str, request_id: &str) -> HttpRequest {
        self.act(
            "/v1/phone-calls",
            format!(
                "{{\"to\":{},\"request_id\":{}}}",
                json::string(to),
                json::string(request_id)
            ),
        )
    }

    /// `GET /v1/phone-calls/{id}`.
    pub fn phone_call(&self, id: &str) -> HttpRequest {
        self.ask(&format!("/v1/phone-calls/{}", encode_segment(id)))
    }

    /// `POST /v1/phone-calls/{id}/hangup`.
    pub fn phone_hang_up(&self, id: &str) -> HttpRequest {
        self.act(
            &format!("/v1/phone-calls/{}/hangup", encode_segment(id)),
            "{}".to_owned(),
        )
    }

    /// `POST /v1/recordings`.
    pub fn start_recording(&self, of: Recorded<'_>) -> HttpRequest {
        let body = match of {
            Recorded::Room(name) => format!("{{\"room\":{}}}", json::string(name)),
            Recorded::Call(id) => format!("{{\"call\":{}}}", json::string(id)),
        };
        self.act("/v1/recordings", body)
    }

    /// `POST /v1/recordings/{id}/stop`.
    pub fn stop_recording(&self, id: &str) -> HttpRequest {
        self.act(
            &format!("/v1/recordings/{}/stop", encode_segment(id)),
            "{}".to_owned(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(body: &str) -> Result<HttpResponse, HttpError> {
        Ok(HttpResponse::new(200).with_body(body.as_bytes().to_vec()))
    }

    #[test]
    fn requests_are_the_contracts() {
        let client = Client::new("http://127.0.0.1:8789", "device-1234");
        let health = client.health();
        assert_eq!(health.url, "http://127.0.0.1:8789/v1/health");
        assert_eq!(health.header_value("X-Device-Id"), None);
        let post = client.post_message("support-desk", "Habari \"wote\"", "m-abc-1");
        assert_eq!(
            post.url,
            "http://127.0.0.1:8789/v1/rooms/support-desk/messages"
        );
        assert_eq!(post.header_value("X-Device-Id"), Some("device-1234"));
        assert_eq!(
            String::from_utf8(post.body.clone()).unwrap(),
            r#"{"text":"Habari \"wote\"","client_id":"m-abc-1"}"#
        );
        assert_eq!(
            client.messages("a b", Some(43)).url,
            "http://127.0.0.1:8789/v1/rooms/a%20b/messages?after=43&read=1"
        );
        assert_eq!(
            client.messages("duka", None).url,
            "http://127.0.0.1:8789/v1/rooms/duka/messages?read=1"
        );
        assert_eq!(
            String::from_utf8(client.ring("baraka", true).body.clone()).unwrap(),
            r#"{"to":"baraka","video":true}"#
        );
        assert_eq!(
            String::from_utf8(client.dial("+255712345678", "dial-x-1").body.clone()).unwrap(),
            r#"{"to":"+255712345678","request_id":"dial-x-1"}"#
        );
        assert_eq!(
            client.hang_up("f3/b1").url,
            "http://127.0.0.1:8789/v1/calls/f3%2Fb1/hangup"
        );
        assert_eq!(
            String::from_utf8(client.start_recording(Recorded::Call("f3b1")).body.clone()).unwrap(),
            r#"{"call":"f3b1"}"#
        );
        assert_eq!(request_id("m", "ab-cd_ef0123456789", 7), "m-abcdef012345-7");
    }

    #[test]
    fn the_contracts_examples_decode() {
        assert_eq!(
            decode_health(ok(
                r#"{"ok": true, "talk": true, "database": true, "phone": false}"#
            )),
            Ok(Health {
                ok: true,
                talk: true,
                database: true,
                phone: false
            })
        );
        assert_eq!(
            decode_me(ok(r#"{"identity": "neema-mushi", "name": "Neema Mushi"}"#)),
            Ok(Peer {
                identity: "neema-mushi".into(),
                name: "Neema Mushi".into()
            })
        );
        let people = decode_people(ok(
            r#"{"people": [{"identity": "baraka", "name": "Baraka", "seen_at": "2026-11-15T07:01:10Z", "online": true}], "now": "2026-11-15T07:01:12Z"}"#,
        ))
        .unwrap();
        assert_eq!(people[0].peer.name, "Baraka");
        assert!(people[0].online);
        let account = decode_account(ok(
            r#"{"phone": {"enabled": false, "caller_id": "+255800110011"},
                "rates": {"phone_call": {"tzs": 300, "per": "minute", "text": "TSh 300 / minute"},
                          "recording": {"tzs": 30, "per": "minute", "text": "TSh 30 / minute"}},
                "balance": {"state": "unknown", "tzs": null}, "spent_tzs": 900}"#,
        ))
        .unwrap();
        assert_eq!(account.caller_id.as_deref(), Some("+255800110011"));
        assert_eq!(account.recording_rate, "TSh 30 / minute");
        let rooms = decode_rooms(ok(
            r#"{"rooms": [{"name": "support-desk", "display_name": "Support desk",
                "created_by": {"identity": "neema", "name": "Neema"}, "mine": true,
                "member": true, "members": 3, "unread": 2,
                "last_message": null, "active_at": "2026-11-15T07:00:30Z", "live": true,
                "recording": {"id": "r9a2", "status": "recording"},
                "created_at": "2026-11-15T07:00:00Z"}], "now": "2026-11-15T07:01:12Z"}"#,
        ))
        .unwrap();
        assert_eq!(rooms[0].display_name, "Support desk");
        assert_eq!(rooms[0].recording.as_deref(), Some("r9a2"));
        let page = decode_messages(ok(
            r#"{"messages": [{"id": 43, "from": "neema", "from_name": "Neema", "mine": true,
                "text": "Habari za asubuhi", "created_at": "2026-11-15T07:01:00Z", "delivered": true}],
                "last_id": 43, "more": false, "unread": 0, "now": "2026-11-15T07:01:12Z"}"#,
        ))
        .unwrap();
        assert_eq!(
            (page.messages[0].id, page.last_id, page.more),
            (43, 43, false)
        );
        let (call, ticket) = decode_call(ok(
            r#"{"call": {"id": "f3b1c2d4", "direction": "outgoing", "video": true, "status": "ringing",
                "peer": {"identity": "baraka", "name": "Baraka"},
                "created_at": "2026-11-15T07:02:00Z", "answered_at": null, "ended_at": null,
                "seconds": 0, "recording": null}, "token": "eyJhbGciOi", "url": "wss://talk.example"}"#,
        ))
        .unwrap();
        assert_eq!((call.status, call.incoming), (CallState::Ringing, false));
        let ticket = ticket.unwrap();
        assert_eq!(ticket.url, "wss://talk.example");
        assert!(
            !format!("{ticket:?}").contains("eyJ"),
            "a token is never printed"
        );
        let answer = decode_phone(ok(
            r#"{"call": {"id": "c81e728d", "to": "+255712345678", "caller_id": "+255800110011",
                "status": "ringing", "created_at": "2026-11-15T07:05:00Z", "started_at": null,
                "ended_at": null, "seconds": 0, "price_tzs": null,
                "rate": {"tzs": 300, "per": "minute", "text": "TSh 300 / minute"},
                "can_hang_up": false}, "hung_up": false,
                "message": "The call ends when the other side hangs up."}"#,
        ))
        .unwrap();
        assert_eq!(answer.call.status, PhoneState::Ringing);
        assert!(!answer.call.can_hang_up);
        assert_eq!(
            answer.message.as_deref(),
            Some("The call ends when the other side hangs up.")
        );
        assert_eq!(
            decode_recording(ok(
                r#"{"recording": {"id": "r9a2", "status": "recording"}}"#
            )),
            Ok(("r9a2".to_owned(), true))
        );
        let (rows, now) = decode_activity(ok(r#"{"items": [
              {"kind": "room", "id": "support-desk", "title": "Support desk",
               "preview": "Baraka: Karibu!", "at": "2026-11-15T07:00:30Z",
               "unread": 2, "live": true, "status": "open",
               "direction": null, "video": null, "seconds": null, "peer": null},
              {"kind": "call", "id": "f3b1c2d4", "title": "Baraka",
               "preview": "Missed video call", "at": "2026-11-15T06:40:00Z",
               "unread": 0, "live": false, "status": "missed",
               "direction": "incoming", "video": true, "seconds": 0,
               "peer": {"identity": "baraka", "name": "Baraka"}},
              {"kind": "phone", "id": "c81e728d", "title": "+255712345678",
               "preview": "Phone call, 2 min 5 s", "at": "2026-11-15T06:10:00Z",
               "unread": 0, "live": false, "status": "ended",
               "direction": "outgoing", "video": false, "seconds": 125, "peer": null}
             ], "now": "2026-11-15T07:01:12Z"}"#))
        .unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!((rows[0].kind, rows[0].unread), (ActivityKind::Room, 2));
        assert_eq!(rows[1].peer.as_ref().unwrap().identity, "baraka");
        assert_eq!(now, "2026-11-15T07:01:12Z");
    }

    #[test]
    fn what_is_not_the_contract_is_refused_not_trusted() {
        assert_eq!(
            decode_health(ok("<html>")),
            Err(Failure::Malformed("a body that is not JSON"))
        );
        assert_eq!(
            decode_me(ok(r#"{"identity": "a b\n", "name": "X"}"#)),
            Err(Failure::Malformed("identity"))
        );
        // A ticket that is not a web-socket address is no ticket.
        let (_, ticket) = decode_call(ok(
            r#"{"call": {"id": "x1", "direction": "incoming", "video": false, "status": "active",
                "peer": {"identity": "baraka", "name": null}, "seconds": 3, "recording": null},
                "token": "t", "url": "javascript:alert(1)"}"#,
        ))
        .unwrap();
        assert_eq!(ticket, None);
        let failure = decode_rooms(Ok(HttpResponse::new(503).with_body(
            br#"{"error": {"code": "talk_unavailable", "message": "The server has no Barua key."}}"#
                .to_vec(),
        )))
        .unwrap_err();
        assert_eq!(failure.sentence(), "The server has no Barua key.");
        assert_eq!(
            Failure::from(HttpError::Offline).sentence(),
            "The server could not be reached."
        );
        assert_eq!(tidy_name("  Neema\n Mushi  "), "Neema Mushi");
        assert_eq!(
            normalize_base_url("HTTP://127.0.0.1:8789/"),
            Ok("http://127.0.0.1:8789".into())
        );
    }
}
