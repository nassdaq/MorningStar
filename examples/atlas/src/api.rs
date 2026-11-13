//! The client of the Atlas server: every request the app sends and every
//! answer it understands (`server/API.md` is the contract).
//!
//! Nothing here touches the network or the app's state. [`Client`] builds
//! [`HttpRequest`]s; the `decode_*` functions turn what came back into
//! plain data or a [`Failure`]. `lib.rs` hands the requests to the platform
//! (`Application::effects`) and feeds the results to the app.
//!
//! Everything a server sends is untrusted. A body that is not the JSON the
//! contract describes is [`Failure::Malformed`], never a panic; every
//! string is cleaned of control characters and cut to a length the screens
//! can show ([`clean`], and [`clean_block`] for an answer of several
//! lines); a path to audio is followed only if it is a path under
//! `/v1/audio/` on the same server.
//!
//! The shape of this module, and the parts that are not about chats
//! (`clean`, `Failure`, the address and id helpers), are those of
//! `examples/callready/src/api.rs`.

use std::time::Duration;

use morningstar::ui::prelude::{HttpError, HttpRequest, HttpResponse, Recording};

use crate::json::{self, Value};
use crate::{Mode, Plan};

/// Where the server is when nobody has said otherwise: the development
/// machine, which a phone reaches through `adb reverse tcp:8788 tcp:8788`.
pub const DEFAULT_BASE_URL: &str = "http://127.0.0.1:8788";

/// The most characters of a message the server accepts.
pub const MESSAGE_LIMIT: usize = 2000;

// ----------------------------------------------------------------- limits

const TITLE_LIMIT: usize = 48;
const STEP_LIMIT: usize = 44;
const STEPS: usize = 4;
const ANSWER_LIMIT: usize = 4200;
const ATTACHMENT_LIMIT: usize = 60;
const ID_LIMIT: usize = 80;
const ERROR_LIMIT: usize = 120;
const CHATS_LIMIT: usize = 50;
const MESSAGES_LIMIT: usize = 100;
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
                400 if !message.is_empty() => message.clone(),
                401 => "The server refused this app: it wants a token.".to_owned(),
                404 => "The server no longer has this chat.".to_owned(),
                413 => "That was too large to send.".to_owned(),
                429 => match retry_after {
                    Some(seconds) => {
                        format!("Too many requests. Wait {seconds} seconds and try again.")
                    }
                    None => "Too many requests. Wait a moment and try again.".to_owned(),
                },
                502 => "Atlas could not answer just now.".to_owned(),
                503 if code == "voice_unavailable" => {
                    "The server has no voice right now.".to_owned()
                }
                503 => "The server\u{2019}s database is not answering.".to_owned(),
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

/// An answer's text: several lines, never empty.
fn block(value: &Value, name: &'static str) -> Field<String> {
    let text = value
        .get(name)
        .and_then(Value::as_str)
        .map(|text| clean_block(text, ANSWER_LIMIT))
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

/// A chat's id: opaque, but short and printable, since it goes into a URL.
fn chat_id(value: &Value) -> Field<String> {
    let id = value
        .get("chat_id")
        .and_then(Value::as_str)
        .ok_or(Failure::Malformed("chat_id"))?;
    let fine = !id.is_empty()
        && id.chars().count() <= ID_LIMIT
        && id.chars().all(|c| c.is_ascii_graphic());
    if fine {
        Ok(id.to_owned())
    } else {
        Err(Failure::Malformed("chat_id"))
    }
}

/// `[{"label": "Reading the request", "state": "done"}]`: the labels, at
/// most four. An item that is not that is left out rather than refused: a
/// step is decoration, the answer is not.
fn steps(value: &Value) -> Field<Vec<String>> {
    let list = value
        .get("steps")
        .and_then(Value::as_array)
        .ok_or(Failure::Malformed("steps"))?;
    Ok(list
        .iter()
        .filter_map(|step| step.get("label").and_then(Value::as_str))
        .map(|label| clean(label, STEP_LIMIT))
        .filter(|label| !label.is_empty())
        .take(STEPS)
        .collect())
}

// ----------------------------------------------------------- enumerations

/// The contract's name of a model choice.
pub const fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Auto => "auto",
        Mode::Fast => "fast",
        Mode::Smart => "smart",
        Mode::Code => "code",
    }
}

/// The model choice with this contract name.
pub fn mode_named(name: &str) -> Option<Mode> {
    Mode::ALL.into_iter().find(|mode| mode_name(*mode) == name)
}

/// The contract's name of a plan.
pub const fn plan_name(plan: Plan) -> &'static str {
    match plan {
        Plan::Free => "free",
        Plan::Basic => "basic",
        Plan::Plus => "plus",
        Plan::Pro => "pro",
    }
}

/// The plan with this contract name.
pub fn plan_named(name: &str) -> Option<Plan> {
    [Plan::Free, Plan::Basic, Plan::Plus, Plan::Pro]
        .into_iter()
        .find(|plan| plan_name(*plan) == name)
}

/// `model_used`: one of the three models that answer (never `auto`).
fn model_used(value: &Value) -> Field<Mode> {
    value
        .get("model_used")
        .and_then(Value::as_str)
        .and_then(mode_named)
        .filter(|mode| *mode != Mode::Auto)
        .ok_or(Failure::Malformed("model_used"))
}

// ---------------------------------------------------------------- answers

/// `GET /v1/health`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Health {
    /// The server can answer: it has a model key and its database answers.
    pub ok: bool,
    /// It has a model key.
    pub model: bool,
    /// It has a voice key: messages can be spoken and answers heard.
    pub voice: bool,
    /// Its database answers.
    pub database: bool,
}

/// Decodes `GET /v1/health`.
pub fn decode_health(result: Result<HttpResponse, HttpError>) -> Result<Health, Failure> {
    let body = object(result)?;
    Ok(Health {
        ok: boolean(&body, "ok")?,
        model: boolean(&body, "model")?,
        voice: boolean(&body, "voice")?,
        database: boolean(&body, "database")?,
    })
}

/// `GET` and `PUT /v1/profile`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Profile {
    /// The model the picker shows.
    pub mode: Mode,
    /// The plan (a demo flag).
    pub plan: Plan,
}

/// Decodes a profile.
pub fn decode_profile(result: Result<HttpResponse, HttpError>) -> Result<Profile, Failure> {
    let body = object(result)?;
    Ok(Profile {
        mode: body
            .get("model")
            .and_then(Value::as_str)
            .and_then(mode_named)
            .ok_or(Failure::Malformed("model"))?,
        plan: body
            .get("plan")
            .and_then(Value::as_str)
            .and_then(plan_named)
            .ok_or(Failure::Malformed("plan"))?,
    })
}

/// A row of `GET /v1/chats`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatSummary {
    /// The chat's id.
    pub id: String,
    /// Its title.
    pub title: String,
    /// How many exchanges it holds.
    pub messages: u32,
}

/// Decodes `GET /v1/chats`: newest first, as the server sends them.
pub fn decode_chats(result: Result<HttpResponse, HttpError>) -> Result<Vec<ChatSummary>, Failure> {
    let body = object(result)?;
    let rows = body
        .get("chats")
        .and_then(Value::as_array)
        .ok_or(Failure::Malformed("chats"))?;
    rows.iter()
        .take(CHATS_LIMIT)
        .map(|row| {
            Ok(ChatSummary {
                id: chat_id(row)?,
                title: filled(row, "title", TITLE_LIMIT)?,
                messages: integer(row, "messages", 0, 100_000)?,
            })
        })
        .collect()
}

/// The answer to a message: `POST /v1/chats` and `POST …/messages`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reply {
    /// The chat the message is in (a new one, for `POST /v1/chats`).
    pub chat_id: String,
    /// The chat's title.
    pub title: String,
    /// The message as the server took it: the transcript, for a recording.
    pub heard: String,
    /// What the assistant did, in order: at most four labels.
    pub steps: Vec<String>,
    /// The answer, in the contract's light formatting.
    pub text: String,
    /// The path of the spoken answer on the server, if it was spoken.
    pub audio: Option<String>,
    /// The model that answered.
    pub model_used: Mode,
    /// The number of this exchange in its chat, from 1.
    pub turn: u32,
}

/// Decodes the answer to a message.
pub fn decode_reply(result: Result<HttpResponse, HttpError>) -> Result<Reply, Failure> {
    let body = object(result)?;
    let answer = body.get("answer").ok_or(Failure::Malformed("answer"))?;
    let audio = match answer.get("audio_url") {
        Some(Value::Null) | None => None,
        // A URL the app will not follow is no audio, not a failed answer.
        Some(url) => url.as_str().and_then(audio_path),
    };
    Ok(Reply {
        chat_id: chat_id(&body)?,
        title: filled(&body, "title", TITLE_LIMIT)?,
        heard: {
            let heard = body
                .get("heard")
                .and_then(Value::as_str)
                .map(|heard| clean_block(heard, MESSAGE_LIMIT))
                .ok_or(Failure::Malformed("heard"))?;
            if heard.is_empty() {
                return Err(Failure::Malformed("heard"));
            }
            heard
        },
        steps: steps(&body)?,
        text: block(answer, "text")?,
        audio,
        model_used: model_used(&body)?,
        turn: integer(&body, "turn", 1, 100_000)?,
    })
}

/// A stored exchange of `GET /v1/chats/{id}`: a message and its answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stored {
    /// What the user sent.
    pub prompt: String,
    /// What was attached, as one label ("quote-a.pdf", "3 files").
    pub attachment: Option<String>,
    /// What the assistant did.
    pub steps: Vec<String>,
    /// The answer.
    pub text: String,
    /// The model that answered.
    pub model_used: Mode,
}

/// `GET /v1/chats/{id}`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chat {
    /// The chat's id.
    pub id: String,
    /// Its title.
    pub title: String,
    /// Its exchanges, oldest first.
    pub exchanges: Vec<Stored>,
}

/// The one label of a message's attachments: the name of one, or how many.
fn attachment_label(message: &Value) -> Option<String> {
    let names: Vec<String> = message
        .get("attachments")
        .and_then(Value::as_array)
        .unwrap_or_default()
        .iter()
        .filter_map(|attachment| attachment.get("name").and_then(Value::as_str))
        .map(|name| clean(name, ATTACHMENT_LIMIT))
        .filter(|name| !name.is_empty())
        .collect();
    match names.len() {
        0 => None,
        1 => names.into_iter().next(),
        count => Some(format!("{count} files")),
    }
}

/// Decodes `GET /v1/chats/{id}`. Messages come in pairs, a user's and the
/// assistant's answer to it; one without its partner is left out.
pub fn decode_chat(result: Result<HttpResponse, HttpError>) -> Result<Chat, Failure> {
    let body = object(result)?;
    let messages = body
        .get("messages")
        .and_then(Value::as_array)
        .ok_or(Failure::Malformed("messages"))?;
    let role = |message: &Value| {
        message
            .get("role")
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    let mut exchanges = Vec::new();
    let mut index = 0;
    while index + 1 < messages.len() && index < MESSAGES_LIMIT {
        let (user, assistant) = (&messages[index], &messages[index + 1]);
        if role(user).as_deref() != Some("user") || role(assistant).as_deref() != Some("assistant")
        {
            index += 1;
            continue;
        }
        exchanges.push(Stored {
            prompt: user
                .get("text")
                .and_then(Value::as_str)
                .map(|text| clean_block(text, MESSAGE_LIMIT))
                .ok_or(Failure::Malformed("text"))?,
            attachment: attachment_label(user),
            steps: steps(assistant)?,
            text: block(assistant, "text")?,
            model_used: model_used(assistant)?,
        });
        index += 2;
    }
    Ok(Chat {
        id: chat_id(&body)?,
        title: filled(&body, "title", TITLE_LIMIT)?,
        exchanges,
    })
}

/// Decodes `DELETE /v1/chats/{id}`. A chat the server no longer has is as
/// deleted as one it has just deleted.
pub fn decode_deleted(result: Result<HttpResponse, HttpError>) -> Result<(), Failure> {
    match object(result) {
        Ok(body) if boolean(&body, "deleted")? => Ok(()),
        Ok(_) => Err(Failure::Malformed("deleted")),
        Err(failure) if failure.status() == Some(404) => Ok(()),
        Err(failure) => Err(failure),
    }
}

/// Decodes `GET /v1/audio/{id}.mp3`: the bytes and their type.
pub fn decode_audio(result: Result<HttpResponse, HttpError>) -> Result<(Vec<u8>, String), Failure> {
    let response = successful(result)?;
    let mime = response
        .header("content-type")
        .and_then(|value| value.split(';').next())
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_default();
    if !mime.starts_with("audio/") {
        return Err(Failure::Malformed("audio of an unknown type"));
    }
    if response.body.is_empty() {
        return Err(Failure::Malformed("empty audio"));
    }
    Ok((response.body, mime))
}

// --------------------------------------------------------------- requests

/// `url` as the path of a spoken answer, if it is one the app will fetch:
/// a path under `/v1/audio/` on the app's own server, made of unreserved
/// characters. Anything else (another host, a query, `..`) is refused, so
/// a server answer cannot send the app elsewhere.
pub fn audio_path(url: &str) -> Option<String> {
    let name = url.strip_prefix("/v1/audio/")?;
    let fine = !name.is_empty()
        && name.len() <= 120
        && !name.starts_with('.')
        && !name.contains("..")
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'));
    fine.then(|| url.to_owned())
}

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

/// What to change of the profile: any subset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProfilePatch {
    /// A new model choice.
    pub mode: Option<Mode>,
    /// A new plan.
    pub plan: Option<Plan>,
}

impl ProfilePatch {
    /// The patch as the body of `PUT /v1/profile`.
    pub fn to_json(&self) -> String {
        let mut members = Vec::new();
        if let Some(mode) = self.mode {
            members.push(format!("\"model\":\"{}\"", mode_name(mode)));
        }
        if let Some(plan) = self.plan {
            members.push(format!("\"plan\":\"{}\"", plan_name(plan)));
        }
        format!("{{{}}}", members.join(","))
    }
}

/// The body of a typed message: its text, the model chosen, and the name
/// of what is attached (a name only: this example uploads no file).
fn message_json(text: &str, mode: Mode, attachment: Option<&str>) -> String {
    let mut body = format!(
        "{{\"text\":{},\"model\":\"{}\"",
        json::string(text),
        mode_name(mode)
    );
    if let Some(name) = attachment {
        let name: String = name.chars().take(ATTACHMENT_LIMIT).collect();
        body.push_str(&format!(
            ",\"attachments\":[{{\"name\":{},\"kind\":\"file\"}}]",
            json::string(&name)
        ));
    }
    body.push('}');
    body
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

    fn request(&self, request: HttpRequest, seconds: u64) -> HttpRequest {
        request
            .header("X-Device-Id", self.device_id.clone())
            .header("Accept", "application/json")
            .timeout(Duration::from_secs(seconds))
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    fn chat_url(&self, chat_id: &str, tail: &str) -> String {
        self.url(&format!("/v1/chats/{}{tail}", encode_segment(chat_id)))
    }

    /// Where a message goes: a chat's `messages`, or `/v1/chats` for the
    /// first message of a new one.
    fn messages_url(&self, chat_id: Option<&str>) -> String {
        match chat_id {
            Some(id) => self.chat_url(id, "/messages"),
            None => self.url("/v1/chats"),
        }
    }

    /// `GET /v1/health`. Short: it decides between online and offline.
    pub fn health(&self) -> HttpRequest {
        self.request(HttpRequest::get(self.url("/v1/health")), 6)
    }

    /// `GET /v1/profile`.
    pub fn profile(&self) -> HttpRequest {
        self.request(HttpRequest::get(self.url("/v1/profile")), 10)
    }

    /// `PUT /v1/profile`.
    pub fn put_profile(&self, patch: &ProfilePatch) -> HttpRequest {
        self.request(HttpRequest::put(self.url("/v1/profile")), 10)
            .json_body(patch.to_json())
    }

    /// `GET /v1/chats`.
    pub fn chats(&self) -> HttpRequest {
        self.request(HttpRequest::get(self.url("/v1/chats")), 10)
    }

    /// `GET /v1/chats/{chat_id}`.
    pub fn chat(&self, chat_id: &str) -> HttpRequest {
        self.request(HttpRequest::get(self.chat_url(chat_id, "")), 15)
    }

    /// `DELETE /v1/chats/{chat_id}`.
    pub fn delete_chat(&self, chat_id: &str) -> HttpRequest {
        self.request(HttpRequest::delete(self.chat_url(chat_id, "")), 15)
    }

    /// A typed message: `POST /v1/chats/{chat_id}/messages`, or
    /// `POST /v1/chats` when there is no chat yet. The server may ask its
    /// model twice, and `smart` thinks first: API.md says to allow 90
    /// seconds.
    pub fn message(
        &self,
        chat_id: Option<&str>,
        text: &str,
        mode: Mode,
        attachment: Option<&str>,
    ) -> HttpRequest {
        self.request(HttpRequest::post(self.messages_url(chat_id)), 100)
            .json_body(message_json(text, mode, attachment))
    }

    /// A spoken message: the recording as the body, its own type as
    /// `Content-Type`, the model in `X-Model`, and `X-Speak: 1` so that
    /// the answer comes back spoken too.
    pub fn spoken_message(
        &self,
        chat_id: Option<&str>,
        recording: &Recording,
        mode: Mode,
    ) -> HttpRequest {
        self.request(HttpRequest::post(self.messages_url(chat_id)), 120)
            .header("Content-Type", recording.mime.clone())
            .header("X-Model", mode_name(mode))
            .header("X-Speak", "1")
            .body(recording.bytes.clone())
    }

    /// `GET` of a spoken answer, with the same headers. `path` is one
    /// [`audio_path`] accepted.
    pub fn audio(&self, path: &str) -> HttpRequest {
        HttpRequest::get(self.url(path))
            .header("X-Device-Id", self.device_id.clone())
            .header("Accept", "audio/mpeg")
            .timeout(Duration::from_secs(20))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use morningstar::ui::prelude::HttpMethod;

    fn ok(body: &str) -> Result<HttpResponse, HttpError> {
        Ok(HttpResponse::new(200)
            .with_header("Content-Type", "application/json")
            .with_body(body.as_bytes().to_vec()))
    }

    fn status(status: u16, body: &str) -> Result<HttpResponse, HttpError> {
        Ok(HttpResponse::new(status).with_body(body.as_bytes().to_vec()))
    }

    // The examples of server/API.md, character for character.
    const HEALTH: &str = r#"{"ok": true, "model": true, "voice": false, "database": true}"#;
    const PROFILE: &str = r#"{"model": "auto", "plan": "free"}"#;
    const REPLY: &str = r#"{"chat_id": "h_9f2c…", "title": "Comparing three quotes",
 "heard": "Compare the prices in these three quotes and find the cheapest.",
 "steps": [{"label": "Reading the request", "state": "done"},
           {"label": "Checking the attachments", "state": "done"},
           {"label": "Writing the answer", "state": "done"}],
 "answer": {"text": "I can’t open the attached files here: only their names reach me.\n\nPaste the totals and I’ll compare them:\n- Quote A\n- Quote B\n- Quote C",
            "audio_url": null},
 "model_used": "smart", "turn": 1}"#;
    const CHATS: &str = r#"{"chats": [{"chat_id": "h_9f2c…", "title": "Comparing three quotes",
            "updated_at": "2026-11-13T09:14:03Z", "messages": 2,
            "state": "answered"}]}"#;
    const CHAT: &str = r#"{"chat_id": "h_9f2c…", "title": "Comparing three quotes",
 "updated_at": "2026-11-13T09:14:03Z",
 "messages": [
   {"role": "user", "text": "Compare the prices in these three quotes…",
    "attachments": [{"name": "quote-a.pdf", "kind": "pdf"}]},
   {"role": "assistant", "text": "I can’t open the attached files here…",
    "steps": [{"label": "Reading the request", "state": "done"}],
    "model_used": "smart"}
 ]}"#;

    /// The contract's examples write a chat id with an ellipsis; a real
    /// one is `h_` and 32 hex digits.
    fn real(example: &str) -> String {
        example.replace("h_9f2c…", "h_9f2c")
    }

    /// The API.md beside this crate holds every example above.
    #[test]
    fn the_examples_are_the_contracts() {
        let contract = include_str!("../server/API.md");
        for example in [HEALTH, PROFILE, REPLY, CHATS, CHAT] {
            assert!(contract.contains(example), "not in API.md: {example}");
        }
    }

    #[test]
    fn decodes_the_contracts_health_and_profile() {
        assert_eq!(
            decode_health(ok(HEALTH)),
            Ok(Health {
                ok: true,
                model: true,
                voice: false,
                database: true
            })
        );
        assert_eq!(
            decode_profile(ok(PROFILE)),
            Ok(Profile {
                mode: Mode::Auto,
                plan: Plan::Free
            })
        );
        assert_eq!(
            decode_profile(ok(r#"{"model": "code", "plan": "pro"}"#)),
            Ok(Profile {
                mode: Mode::Code,
                plan: Plan::Pro
            })
        );
        for bad in [
            r#"{"model": "genius", "plan": "free"}"#,
            r#"{"model": "auto", "plan": "gold"}"#,
            r#"{"model": "auto"}"#,
            r#"{"ok": "yes", "model": true, "voice": false, "database": true}"#,
        ] {
            assert!(matches!(
                decode_profile(ok(bad)),
                Err(Failure::Malformed(_))
            ));
            assert!(matches!(decode_health(ok(bad)), Err(Failure::Malformed(_))));
        }
    }

    #[test]
    fn decodes_the_contracts_reply() {
        let reply = decode_reply(ok(&real(REPLY))).expect("the contract's example");
        assert_eq!(reply.chat_id, "h_9f2c");
        assert_eq!(reply.title, "Comparing three quotes");
        assert_eq!(
            reply.heard,
            "Compare the prices in these three quotes and find the cheapest."
        );
        assert_eq!(
            reply.steps,
            [
                "Reading the request",
                "Checking the attachments",
                "Writing the answer"
            ]
        );
        assert_eq!(
            reply.text,
            "I can\u{2019}t open the attached files here: only their names reach me.\n\nPaste the totals and I\u{2019}ll compare them:\n- Quote A\n- Quote B\n- Quote C"
        );
        assert_eq!(reply.audio, None);
        assert_eq!((reply.model_used, reply.turn), (Mode::Smart, 1));
    }

    #[test]
    fn a_reply_is_cleaned_and_its_audio_followed_only_on_this_server() {
        let body = |audio: &str, steps: &str, text: &str| {
            format!(
                r#"{{"chat_id": "h_1", "title": "  A\ttitle\u0007 ", "heard": "hi",
                    "steps": {steps}, "answer": {{"text": "{text}", "audio_url": {audio}}},
                    "model_used": "fast", "turn": 3}}"#
            )
        };
        let fine = decode_reply(ok(&body(
            r#""/v1/audio/a_0123.mp3""#,
            r#"[{"label": "One"}, 7, {"label": ""}, {"label": "Two"}, {"label": "Three"}, {"label": "Four"}, {"label": "Five"}]"#,
            "a\\u202eb\\r\\n\\n\\n\\n\\tcode  x\\u0000",
        )))
        .expect("a reply");
        assert_eq!(fine.title, "A title");
        assert_eq!(fine.audio.as_deref(), Some("/v1/audio/a_0123.mp3"));
        assert_eq!(fine.steps, ["One", "Two", "Three", "Four"]);
        assert_eq!(fine.text, "ab\n\n  code  x");
        for elsewhere in [
            r#""https://evil.example/v1/audio/a.mp3""#,
            r#""/v1/audio/../profile""#,
            r#""/v1/audio/a.mp3?x=1""#,
            "42",
        ] {
            let reply = decode_reply(ok(&body(elsewhere, "[]", "x"))).expect("a reply");
            assert_eq!(reply.audio, None, "{elsewhere}");
            assert!(reply.steps.is_empty());
        }
        // No answer, no steps list, a model the app does not know, turn 0.
        for (from, to) in [
            (r#""text": "x""#, r#""text": "  ""#),
            (r#""steps": []"#, r#""steps": "none""#),
            (r#""model_used": "fast""#, r#""model_used": "auto""#),
            (r#""turn": 3"#, r#""turn": 0"#),
            (r#""chat_id": "h_1""#, r#""chat_id": "h 1""#),
            (r#""heard": "hi""#, r#""heard": """#),
        ] {
            let broken = body("null", "[]", "x").replace(from, to);
            assert_ne!(broken, body("null", "[]", "x"), "{from}");
            assert!(
                matches!(decode_reply(ok(&broken)), Err(Failure::Malformed(_))),
                "{to}"
            );
        }
    }

    #[test]
    fn decodes_the_contracts_chats_and_chat() {
        assert_eq!(
            decode_chats(ok(&real(CHATS))),
            Ok(vec![ChatSummary {
                id: "h_9f2c".to_owned(),
                title: "Comparing three quotes".to_owned(),
                messages: 2,
            }])
        );
        assert_eq!(decode_chats(ok(r#"{"chats": []}"#)), Ok(Vec::new()));
        assert!(decode_chats(ok(r#"{"chats": {}}"#)).is_err());
        assert!(
            decode_chats(ok(
                r#"{"chats": [{"chat_id": "h_1", "title": "", "messages": 1}]}"#
            ))
            .is_err()
        );

        let chat = decode_chat(ok(&real(CHAT))).expect("the contract's example");
        assert_eq!(
            (chat.id.as_str(), chat.title.as_str()),
            ("h_9f2c", "Comparing three quotes")
        );
        assert_eq!(
            chat.exchanges,
            [Stored {
                prompt: "Compare the prices in these three quotes\u{2026}".to_owned(),
                attachment: Some("quote-a.pdf".to_owned()),
                steps: vec!["Reading the request".to_owned()],
                text: "I can\u{2019}t open the attached files here\u{2026}".to_owned(),
                model_used: Mode::Smart,
            }]
        );
    }

    #[test]
    fn a_chat_keeps_whole_exchanges_only() {
        let chat = decode_chat(ok(r#"{"chat_id": "h_1", "title": "T", "messages": [
            {"role": "assistant", "text": "orphan", "steps": [], "model_used": "fast"},
            {"role": "user", "text": "one", "attachments": [{"name": "a"}, {"name": "b"}]},
            {"role": "assistant", "text": "1", "steps": [], "model_used": "code"},
            {"role": "user", "text": "two"}
        ]}"#))
        .expect("a chat");
        assert_eq!(chat.exchanges.len(), 1);
        assert_eq!(chat.exchanges[0].attachment.as_deref(), Some("2 files"));
        assert_eq!(chat.exchanges[0].model_used, Mode::Code);
        assert!(decode_chat(ok(r#"{"chat_id": "h_1", "title": "T"}"#)).is_err());
    }

    #[test]
    fn deleted_is_deleted_and_so_is_gone() {
        assert_eq!(decode_deleted(ok(r#"{"deleted": true}"#)), Ok(()));
        assert_eq!(
            decode_deleted(status(
                404,
                r#"{"error": {"code": "not_found", "message": "No such chat."}}"#
            )),
            Ok(())
        );
        assert!(decode_deleted(ok(r#"{"deleted": false}"#)).is_err());
        assert!(decode_deleted(status(503, "")).is_err());
        assert_eq!(
            decode_deleted(Err(HttpError::Offline)),
            Err(Failure::Offline)
        );
    }

    #[test]
    fn errors_become_failures_with_a_sentence() {
        let failure = decode_health(status(
            502,
            r#"{"error": {"code": "upstream_failed", "message": "Atlas could not answer just now. Please try again."}}"#,
        ))
        .unwrap_err();
        assert_eq!(failure.status(), Some(502));
        assert!(!failure.unreachable());
        assert_eq!(failure.sentence(), "Atlas could not answer just now.");
        let limited = decode_health(Ok(HttpResponse::new(429)
            .with_header("Retry-After", "12")
            .with_body(b"{}".to_vec())))
        .unwrap_err();
        assert_eq!(
            limited.sentence(),
            "Too many requests. Wait 12 seconds and try again."
        );
        let sentence = |status_code: u16, body: &str| {
            decode_health(status(status_code, body))
                .unwrap_err()
                .sentence()
        };
        assert_eq!(
            sentence(
                400,
                r#"{"error": {"code": "bad_request", "message": "\"text\" must not be empty."}}"#
            ),
            "\"text\" must not be empty."
        );
        assert_eq!(
            sentence(401, ""),
            "The server refused this app: it wants a token."
        );
        assert_eq!(
            sentence(404, "<html>"),
            "The server no longer has this chat."
        );
        assert_eq!(
            sentence(
                503,
                r#"{"error": {"code": "voice_unavailable", "message": "x"}}"#
            ),
            "The server has no voice right now."
        );
        assert_eq!(
            sentence(
                503,
                r#"{"error": {"code": "database_unavailable", "message": "x"}}"#
            ),
            "The server\u{2019}s database is not answering."
        );
        assert_eq!(sentence(500, ""), "The server had a problem (500).");
        for (error, unreachable) in [
            (HttpError::Offline, true),
            (HttpError::Timeout, true),
            (HttpError::Unsupported, true),
            (HttpError::InvalidUrl, true),
            (HttpError::Failed("tls\nbroke".to_owned()), true),
            (HttpError::TooLarge, false),
        ] {
            let failure = decode_health(Err(error)).unwrap_err();
            assert_eq!(failure.unreachable(), unreachable, "{failure:?}");
            assert!(failure.sentence().ends_with('.') && !failure.sentence().contains('\n'));
        }
        for junk in ["", "not json", "[1]", "\"text\""] {
            assert!(matches!(
                decode_health(ok(junk)),
                Err(Failure::Malformed(_))
            ));
        }
        assert!(matches!(
            decode_health(Ok(HttpResponse::new(200).with_body(vec![0xff, 0xfe]))),
            Err(Failure::Malformed(_))
        ));
    }

    #[test]
    fn text_is_cleaned_for_one_line_and_for_several() {
        assert_eq!(clean("  a\n\tb\u{7}  c ", 40), "a b c");
        assert_eq!(clean("abcdefghij", 5), "abcd\u{2026}");
        assert_eq!(
            clean(&"x".repeat(60), 80),
            format!("{} {} xxxx", "x".repeat(28), "x".repeat(28))
        );
        assert_eq!(clean("a\u{202e}b", 10), "ab");
        assert_eq!(clean_block("a  \r\n\n\n\nb\n\n", 100), "a\n\nb");
        assert_eq!(
            clean_block("\n\n  fn x() {\n\treturn 1;\n  }", 100),
            "  fn x() {\n  return 1;\n  }"
        );
        let cut = clean_block(&"line\n".repeat(100), 22);
        assert!(
            cut.chars().count() <= 22 && cut.ends_with('\u{2026}'),
            "{cut:?}"
        );
        assert_eq!(clean_block("", 10), "");
    }

    #[test]
    fn audio_arrives_as_bytes_of_a_known_type() {
        let mp3 = Ok(HttpResponse::new(200)
            .with_header("Content-Type", "audio/mpeg; x=1")
            .with_body(vec![1, 2, 3]));
        assert_eq!(
            decode_audio(mp3),
            Ok((vec![1, 2, 3], "audio/mpeg".to_owned()))
        );
        let html = Ok(HttpResponse::new(200)
            .with_header("Content-Type", "text/html")
            .with_body(vec![1]));
        assert!(decode_audio(html).is_err());
        let empty = Ok(HttpResponse::new(200).with_header("Content-Type", "audio/mpeg"));
        assert!(decode_audio(empty).is_err());
        assert_eq!(
            decode_audio(status(404, "")).unwrap_err().status(),
            Some(404)
        );
    }

    #[test]
    fn addresses_and_ids_are_put_into_shape() {
        assert_eq!(
            normalize_base_url("  HTTP://127.0.0.1:8788/ "),
            Ok("http://127.0.0.1:8788".to_owned())
        );
        assert_eq!(
            normalize_base_url("https://atlas.example.com/api/"),
            Ok("https://atlas.example.com/api".to_owned())
        );
        assert_eq!(normalize_base_url(""), Err(UrlProblem::Empty));
        assert_eq!(
            normalize_base_url("127.0.0.1:8788"),
            Err(UrlProblem::Scheme)
        );
        assert_eq!(normalize_base_url("ftp://x"), Err(UrlProblem::Scheme));
        for bad in [
            "http://",
            "http://a b",
            "http://u@h",
            "http://h?x=1",
            "http://h/../x",
            "http://h//x",
        ] {
            assert_eq!(normalize_base_url(bad), Err(UrlProblem::Host), "{bad}");
        }
        assert_eq!(device_id("", "0123456789abcdef"), "0123456789abcdef");
        assert_eq!(device_id("test_", "ab cd;ef"), "test_abcdef");
        assert_eq!(device_id("", "ab"), "ab000000");
        assert_eq!(device_id("", &"x".repeat(90)).len(), 64);
        assert_eq!(encode_segment("h_1/../x y"), "h_1%2F..%2Fx%20y");
        assert_eq!(
            audio_path("/v1/audio/a_1.mp3").as_deref(),
            Some("/v1/audio/a_1.mp3")
        );
        assert_eq!(audio_path("/v1/audio/"), None);
        assert_eq!(audio_path("/v1/chats"), None);
    }

    #[test]
    fn requests_are_the_contracts() {
        let client = Client::new("http://127.0.0.1:8788", "0123456789abcdef");
        let health = client.health();
        assert_eq!(health.method, HttpMethod::Get);
        assert_eq!(health.url, "http://127.0.0.1:8788/v1/health");
        assert_eq!(health.header_value("X-Device-Id"), Some("0123456789abcdef"));
        assert_eq!(health.header_value("Authorization"), None);
        assert_eq!(client.profile().url, "http://127.0.0.1:8788/v1/profile");
        assert_eq!(client.chats().url, "http://127.0.0.1:8788/v1/chats");
        assert_eq!(client.chat("h_1").url, "http://127.0.0.1:8788/v1/chats/h_1");
        let delete = client.delete_chat("h_1/x");
        assert_eq!(delete.method, HttpMethod::Delete);
        assert_eq!(delete.url, "http://127.0.0.1:8788/v1/chats/h_1%2Fx");

        let put = client.put_profile(&ProfilePatch {
            mode: Some(Mode::Smart),
            plan: None,
        });
        assert_eq!(put.method, HttpMethod::Put);
        assert_eq!(put.body, br#"{"model":"smart"}"#);
        assert_eq!(put.header_value("Content-Type"), Some("application/json"));
        assert_eq!(
            ProfilePatch {
                mode: Some(Mode::Auto),
                plan: Some(Plan::Plus)
            }
            .to_json(),
            r#"{"model":"auto","plan":"plus"}"#
        );

        let first = client.message(None, "Say \"hi\"\nplease", Mode::Auto, Some("3 PDFs"));
        assert_eq!(first.method, HttpMethod::Post);
        assert_eq!(first.url, "http://127.0.0.1:8788/v1/chats");
        assert_eq!(
            std::str::from_utf8(&first.body).unwrap(),
            r#"{"text":"Say \"hi\"\nplease","model":"auto","attachments":[{"name":"3 PDFs","kind":"file"}]}"#
        );
        assert!(json::parse(std::str::from_utf8(&first.body).unwrap()).is_ok());
        assert_eq!(first.timeout, Duration::from_secs(100));
        let next = client.message(Some("h_1"), "More", Mode::Code, None);
        assert_eq!(next.url, "http://127.0.0.1:8788/v1/chats/h_1/messages");
        assert_eq!(next.body, br#"{"text":"More","model":"code"}"#);

        let recording = Recording {
            bytes: vec![1, 2, 3],
            mime: "audio/mp4".to_owned(),
            duration: Duration::from_secs(2),
        };
        let spoken = client.spoken_message(None, &recording, Mode::Fast);
        assert_eq!(spoken.url, "http://127.0.0.1:8788/v1/chats");
        assert_eq!(spoken.body, [1, 2, 3]);
        assert_eq!(spoken.header_value("Content-Type"), Some("audio/mp4"));
        assert_eq!(spoken.header_value("X-Model"), Some("fast"));
        assert_eq!(spoken.header_value("X-Speak"), Some("1"));
        assert_eq!(
            client
                .spoken_message(Some("h_1"), &recording, Mode::Auto)
                .url,
            "http://127.0.0.1:8788/v1/chats/h_1/messages"
        );
        let audio = client.audio("/v1/audio/a_1.mp3");
        assert_eq!(audio.url, "http://127.0.0.1:8788/v1/audio/a_1.mp3");
        assert_eq!(audio.header_value("Accept"), Some("audio/mpeg"));
        assert_eq!(tidy_message("  hi  "), "hi");
        assert_eq!(tidy_message(&"x".repeat(3000)).len(), MESSAGE_LIMIT);
    }
}
