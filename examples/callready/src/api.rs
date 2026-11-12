//! The client of the CallReady server: every request the app sends and
//! every answer it understands (`server/API.md` is the contract).
//!
//! Nothing here touches the network or the app's state. [`Client`] builds
//! [`HttpRequest`]s; the `decode_*` functions turn what came back into
//! plain data or a [`Failure`]. `lib.rs` hands the requests to the platform
//! (`Application::effects`) and feeds the results to the model.
//!
//! Everything a server sends is untrusted. A body that is not the JSON the
//! contract describes is [`Failure::Malformed`], never a panic; every
//! string is cleaned of control characters and cut to a length the screens
//! can show ([`clean`]); numbers out of range are refused; a path to audio
//! is followed only if it is a path under `/v1/audio/` on the same server.

use std::time::Duration;

use morningstar::ui::prelude::{HttpError, HttpRequest, HttpResponse, Recording};

use crate::json::{self, Value};
use crate::model::{Accent, Difficulty, Length};
use crate::script::Scenario;

/// Where the server is when the learner has not said otherwise: the
/// development machine, which a phone reaches through
/// `adb reverse tcp:8787 tcp:8787`.
pub const DEFAULT_BASE_URL: &str = "http://127.0.0.1:8787";

/// The most characters of a reply the server accepts.
pub const REPLY_LIMIT: usize = 600;

// ----------------------------------------------------------------- limits

const TITLE_LIMIT: usize = 40;
const CALLER_LIMIT: usize = 24;
const GOAL_LIMIT: usize = 80;
const LINE_LIMIT: usize = 320;
const TIP_LIMIT: usize = 28;
const FEEDBACK_LIMIT: usize = 140;
const NOTE_LIMIT: usize = 48;
const NAME_LIMIT: usize = 24;
const ID_LIMIT: usize = 80;
const MESSAGE_LIMIT: usize = 120;
const TRANSCRIPT_LIMIT: usize = 120;
const HISTORY_LIMIT: usize = 50;
/// A run of characters with no space in it is broken after this many, so
/// that no text can be wider than the screen.
const WORD_LIMIT: usize = 28;

/// Text from the server made safe to show: control characters and line
/// breaks become spaces, runs of spaces one space, the ends are trimmed, a
/// "word" longer than a line is broken, and at most `limit` characters are
/// kept (the last of them an ellipsis when something was cut).
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
        // Characters that reorder or hide text have no place in a line.
        if matches!(character, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}')
        {
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
        /// The contract's error code (`call_ended`), or empty.
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
                message,
                retry_after,
                ..
            } => match status {
                400 if !message.is_empty() => message.clone(),
                401 => "The server refused this app: it wants a token.".to_owned(),
                404 => "The server does not know this call.".to_owned(),
                409 => "The call has already ended.".to_owned(),
                413 => "That was too large to send.".to_owned(),
                429 => match retry_after {
                    Some(seconds) => {
                        format!("Too many requests. Wait {seconds} seconds and try again.")
                    }
                    None => "Too many requests. Wait a moment and try again.".to_owned(),
                },
                502 => "The AI caller did not answer. Try again.".to_owned(),
                503 => "The server has no voice right now.".to_owned(),
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
        message: field("message", MESSAGE_LIMIT),
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

/// A score, or `None` for JSON `null`.
fn score(value: &Value, name: &'static str) -> Field<Option<u8>> {
    match value.get(name) {
        Some(Value::Null) => Ok(None),
        Some(number) => number
            .as_integer(0, 100)
            .map(|score| Some(score as u8))
            .ok_or(Failure::Malformed(name)),
        None => Err(Failure::Malformed(name)),
    }
}

/// A call's id: opaque, but short and printable, since it goes into a URL.
fn call_id(value: &Value) -> Field<String> {
    let id = value
        .get("call_id")
        .and_then(Value::as_str)
        .ok_or(Failure::Malformed("call_id"))?;
    let fine = !id.is_empty()
        && id.chars().count() <= ID_LIMIT
        && id.chars().all(|c| c.is_ascii_graphic());
    if fine {
        Ok(id.to_owned())
    } else {
        Err(Failure::Malformed("call_id"))
    }
}

// ----------------------------------------------------------- enumerations

/// The contract's name of a scenario.
pub const fn scenario_name(scenario: Scenario) -> &'static str {
    match scenario {
        Scenario::MissingDelivery => "missing_delivery",
        Scenario::AngryCustomer => "angry_customer",
        Scenario::RefundRequest => "refund_request",
        Scenario::BillingProblem => "billing_problem",
    }
}

/// The scenario with this contract name.
pub fn scenario_named(name: &str) -> Option<Scenario> {
    Scenario::ALL
        .into_iter()
        .find(|scenario| scenario_name(*scenario) == name)
}

/// The contract's name of an accent.
pub const fn accent_name(accent: Accent) -> &'static str {
    match accent {
        Accent::Us => "us",
        Accent::Uk => "uk",
        Accent::Australian => "australian",
        Accent::Indian => "indian",
    }
}

/// The contract's name of a difficulty.
pub const fn difficulty_name(difficulty: Difficulty) -> &'static str {
    match difficulty {
        Difficulty::Calm => "calm",
        Difficulty::Standard => "standard",
        Difficulty::Tough => "tough",
    }
}

fn accent(value: &Value) -> Field<Accent> {
    let name = value.get("accent").and_then(Value::as_str);
    Accent::ALL
        .into_iter()
        .find(|accent| Some(accent_name(*accent)) == name)
        .ok_or(Failure::Malformed("accent"))
}

fn difficulty(value: &Value) -> Field<Difficulty> {
    let name = value.get("difficulty").and_then(Value::as_str);
    Difficulty::ALL
        .into_iter()
        .find(|difficulty| Some(difficulty_name(*difficulty)) == name)
        .ok_or(Failure::Malformed("difficulty"))
}

fn length(value: &Value) -> Field<Length> {
    let minutes = value.get("minutes").and_then(|m| m.as_integer(1, 60));
    Length::ALL
        .into_iter()
        .find(|length| Some(length.minutes()) == minutes)
        .ok_or(Failure::Malformed("minutes"))
}

// ---------------------------------------------------------------- answers

/// `GET /v1/health`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Health {
    /// The server can play calls: it has a model key and its database
    /// answers.
    pub ok: bool,
    /// It has a model key.
    pub model: bool,
    /// It has a voice key: calls are spoken.
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    /// The learner's name; empty for a device seen for the first time.
    pub name: String,
    /// The caller's accent.
    pub accent: Accent,
    /// The caller's patience.
    pub difficulty: Difficulty,
    /// The call's length.
    pub length: Length,
    /// Whether the plan is `premium` (a demo flag).
    pub premium: bool,
    /// Scored calls so far.
    pub calls: u32,
    /// Their mean score; `None` with no call.
    pub average_score: Option<u8>,
    /// Days in a row with a call.
    pub streak_days: u32,
}

/// Decodes a profile.
pub fn decode_profile(result: Result<HttpResponse, HttpError>) -> Result<Profile, Failure> {
    let body = object(result)?;
    let plan = body.get("plan").and_then(Value::as_str);
    Ok(Profile {
        name: text(&body, "name", NAME_LIMIT)?,
        accent: accent(&body)?,
        difficulty: difficulty(&body)?,
        length: length(&body)?,
        premium: match plan {
            Some("premium") => true,
            Some("free") => false,
            _ => return Err(Failure::Malformed("plan")),
        },
        calls: integer(&body, "calls", 0, 1_000_000)?,
        average_score: score(&body, "average_score")?,
        streak_days: integer(&body, "streak_days", 0, 100_000)?,
    })
}

/// A line the caller says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// The words.
    pub text: String,
    /// The path of its speech on this server, when there is one and it is
    /// a path the app will follow ([`audio_path`]).
    pub audio: Option<String>,
}

fn line(value: &Value) -> Field<Line> {
    let line = value.get("line").ok_or(Failure::Malformed("line"))?;
    let audio = match line.get("audio_url") {
        None | Some(Value::Null) => None,
        // A URL the app will not follow is no audio, not a failed turn.
        Some(url) => url.as_str().and_then(audio_path),
    };
    Ok(Line {
        text: filled(line, "text", LINE_LIMIT)?,
        audio,
    })
}

/// At most two tips, each cut to what a chip holds. Anything else in the
/// list is left out: a call can go on without advice.
fn tips(value: &Value) -> Vec<String> {
    value
        .get("tips")
        .and_then(Value::as_array)
        .unwrap_or_default()
        .iter()
        .filter_map(Value::as_str)
        .map(|tip| clean(tip, TIP_LIMIT))
        .filter(|tip| !tip.is_empty())
        .take(2)
        .collect()
}

/// `POST /v1/calls`: a call that has just connected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallStart {
    /// The call's id.
    pub call_id: String,
    /// Its number in the learner's history.
    pub number: u32,
    /// The scenario, if it is one the app knows.
    pub scenario: Option<Scenario>,
    /// The scenario's name.
    pub title: String,
    /// The caller's first name.
    pub caller_name: String,
    /// What the learner is to achieve.
    pub goal: String,
    /// The caller's accent.
    pub accent: Accent,
    /// The call's length.
    pub length: Length,
    /// Whether the call is spoken: turns may be audio and lines have audio.
    pub voice: bool,
    /// The caller's opening line.
    pub line: Line,
    /// Two pieces of advice.
    pub tips: Vec<String>,
}

/// Decodes `POST /v1/calls`.
pub fn decode_call_start(result: Result<HttpResponse, HttpError>) -> Result<CallStart, Failure> {
    let body = object(result)?;
    Ok(CallStart {
        call_id: call_id(&body)?,
        number: integer(&body, "number", 1, 1_000_000)?,
        scenario: body
            .get("scenario")
            .and_then(Value::as_str)
            .and_then(scenario_named),
        title: filled(&body, "title", TITLE_LIMIT)?,
        caller_name: filled(&body, "caller_name", CALLER_LIMIT)?,
        goal: filled(&body, "goal", GOAL_LIMIT)?,
        accent: accent(&body)?,
        length: length(&body)?,
        voice: boolean(&body, "voice")?,
        line: line(&body)?,
        tips: tips(&body),
    })
}

/// `POST /v1/calls/{id}/turns`: the caller's answer to the learner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Turn {
    /// What the learner said, as the server heard it.
    pub heard: String,
    /// The caller's next line; the closing line when `done`.
    pub line: Line,
    /// Two pieces of advice.
    pub tips: Vec<String>,
    /// The number of the turn that comes next, from 2.
    pub turn: u32,
    /// The caller has ended the conversation, or the time is used up.
    pub done: bool,
}

/// Decodes a turn's answer.
pub fn decode_turn(result: Result<HttpResponse, HttpError>) -> Result<Turn, Failure> {
    let body = object(result)?;
    Ok(Turn {
        heard: text(&body, "heard", REPLY_LIMIT)?,
        line: line(&body)?,
        tips: tips(&body),
        turn: integer(&body, "turn", 1, 10_000)?,
        done: boolean(&body, "done")?,
    })
}

/// One line of a finished call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spoken {
    /// True for the caller, false for the learner.
    pub caller: bool,
    /// The words.
    pub text: String,
    /// The trainer's note on a learner's line.
    pub note: Option<String>,
}

/// The five skill scores, in the report's order
/// ([`Skill::ALL`](crate::script::Skill::ALL)).
pub const SKILL_NAMES: [&str; 5] = [
    "clarity",
    "empathy",
    "listening",
    "problem_solving",
    "professional_language",
];

/// `POST /v1/calls/{id}/end` and `GET /v1/calls/{id}`: a finished call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallReport {
    /// The call's id.
    pub call_id: String,
    /// Its number in the learner's history.
    pub number: u32,
    /// The scenario, if it is one the app knows.
    pub scenario: Option<Scenario>,
    /// The scenario's name.
    pub title: String,
    /// The caller's first name.
    pub caller_name: String,
    /// When it began, as the server wrote it (`2026-11-12T09:14:03Z`).
    pub started_at: String,
    /// How long it lasted.
    pub duration_seconds: u32,
    /// The score and the five skills; `None` for a call ended with no
    /// learner turn, which is not scored.
    pub scores: Option<(u8, [u8; 5])>,
    /// What went well, one sentence.
    pub did_well: String,
    /// What to try next, one sentence.
    pub try_next: String,
    /// The call, line by line.
    pub transcript: Vec<Spoken>,
}

/// Decodes a report.
pub fn decode_report(result: Result<HttpResponse, HttpError>) -> Result<CallReport, Failure> {
    let body = object(result)?;
    let overall = score(&body, "score")?;
    let skills = match body.get("skills") {
        Some(Value::Null) => None,
        Some(skills @ Value::Object(_)) => {
            let mut scores = [0; 5];
            for (slot, name) in scores.iter_mut().zip(SKILL_NAMES) {
                *slot = skills
                    .get(name)
                    .and_then(|score| score.as_integer(0, 100))
                    .ok_or(Failure::Malformed("skills"))? as u8;
            }
            Some(scores)
        }
        _ => return Err(Failure::Malformed("skills")),
    };
    let scores = match (overall, skills) {
        (Some(overall), Some(skills)) => Some((overall, skills)),
        (None, None) => None,
        // Half a score is not a score.
        _ => return Err(Failure::Malformed("score")),
    };
    let lines = body
        .get("transcript")
        .and_then(Value::as_array)
        .ok_or(Failure::Malformed("transcript"))?;
    let mut transcript = Vec::new();
    for entry in lines.iter().take(TRANSCRIPT_LIMIT) {
        let caller = match entry.get("speaker").and_then(Value::as_str) {
            Some("caller") => true,
            Some("learner") => false,
            _ => return Err(Failure::Malformed("transcript")),
        };
        let note = entry
            .get("note")
            .and_then(Value::as_str)
            .map(|note| clean(note, NOTE_LIMIT))
            .filter(|note| !caller && !note.is_empty());
        transcript.push(Spoken {
            caller,
            text: text(entry, "text", REPLY_LIMIT)?,
            note,
        });
    }
    Ok(CallReport {
        call_id: call_id(&body)?,
        number: integer(&body, "number", 1, 1_000_000)?,
        scenario: body
            .get("scenario")
            .and_then(Value::as_str)
            .and_then(scenario_named),
        title: filled(&body, "title", TITLE_LIMIT)?,
        caller_name: filled(&body, "caller_name", CALLER_LIMIT)?,
        started_at: text(&body, "started_at", 40)?,
        duration_seconds: integer(&body, "duration_seconds", 0, 86_400)?,
        scores,
        did_well: text(&body, "did_well", FEEDBACK_LIMIT)?,
        try_next: text(&body, "try_next", FEEDBACK_LIMIT)?,
        transcript,
    })
}

/// A row of `GET /v1/calls`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallSummary {
    /// The call's id.
    pub call_id: String,
    /// Its number in the learner's history.
    pub number: u32,
    /// The scenario, if it is one the app knows.
    pub scenario: Option<Scenario>,
    /// The scenario's name.
    pub title: String,
    /// When it began, as the server wrote it.
    pub started_at: String,
    /// How long it lasted.
    pub duration_seconds: u32,
    /// Its score.
    pub score: u8,
}

/// Decodes `GET /v1/calls`: the ended, scored calls, newest first. A row
/// the app cannot read fails the whole list: half a history would look
/// like the truth.
pub fn decode_calls(result: Result<HttpResponse, HttpError>) -> Result<Vec<CallSummary>, Failure> {
    let body = object(result)?;
    let rows = body
        .get("calls")
        .and_then(Value::as_array)
        .ok_or(Failure::Malformed("calls"))?;
    rows.iter()
        .take(HISTORY_LIMIT)
        .map(|row| {
            Ok(CallSummary {
                call_id: call_id(row)?,
                number: integer(row, "number", 1, 1_000_000)?,
                scenario: row
                    .get("scenario")
                    .and_then(Value::as_str)
                    .and_then(scenario_named),
                title: filled(row, "title", TITLE_LIMIT)?,
                started_at: text(row, "started_at", 40)?,
                duration_seconds: integer(row, "duration_seconds", 0, 86_400)?,
                score: integer(row, "score", 0, 100)? as u8,
            })
        })
        .collect()
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

/// "12 Nov" from `2026-11-12T09:14:03Z`; empty for anything else.
pub fn day_label(started_at: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let bytes = started_at.as_bytes();
    let digits = |from: usize, to: usize| -> Option<usize> {
        let part = started_at.get(from..to)?;
        (part.bytes().all(|byte| byte.is_ascii_digit())).then(|| part.parse().ok())?
    };
    let label = || -> Option<String> {
        if bytes.get(4) != Some(&b'-') || bytes.get(7) != Some(&b'-') {
            return None;
        }
        digits(0, 4)?;
        let month = MONTHS.get(digits(5, 7)?.checked_sub(1)?)?;
        let day = digits(8, 10).filter(|day| (1..=31).contains(day))?;
        Some(format!("{day} {month}"))
    };
    label().unwrap_or_default()
}

// --------------------------------------------------------------- requests

/// `url` as the path of a caller line's audio, if it is one the app will
/// fetch: a path under `/v1/audio/` on the app's own server, made of
/// unreserved characters. Anything else (another host, a query, `..`) is
/// refused, so a server answer cannot send the app elsewhere.
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

/// What to change of the profile: any subset.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProfilePatch {
    /// A new name, 1 to 24 characters.
    pub name: Option<String>,
    /// A new accent.
    pub accent: Option<Accent>,
    /// A new difficulty.
    pub difficulty: Option<Difficulty>,
    /// A new call length.
    pub length: Option<Length>,
    /// A new plan: premium, or free.
    pub premium: Option<bool>,
}

impl ProfilePatch {
    /// The patch as the body of `PUT /v1/profile`.
    pub fn to_json(&self) -> String {
        let mut members = Vec::new();
        if let Some(name) = &self.name {
            members.push(format!("\"name\":{}", json::string(name)));
        }
        if let Some(accent) = self.accent {
            members.push(format!("\"accent\":\"{}\"", accent_name(accent)));
        }
        if let Some(difficulty) = self.difficulty {
            members.push(format!(
                "\"difficulty\":\"{}\"",
                difficulty_name(difficulty)
            ));
        }
        if let Some(length) = self.length {
            members.push(format!("\"minutes\":{}", length.minutes()));
        }
        if let Some(premium) = self.premium {
            let plan = if premium { "premium" } else { "free" };
            members.push(format!("\"plan\":\"{plan}\""));
        }
        format!("{{{}}}", members.join(","))
    }
}

/// A reply as the server will count it: white space collapsed and trimmed.
pub fn tidy_reply(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
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

    /// `GET /v1/calls`.
    pub fn calls(&self) -> HttpRequest {
        self.request(HttpRequest::get(self.url("/v1/calls")), 10)
    }

    /// `GET /v1/calls/{call_id}`.
    pub fn call(&self, call_id: &str) -> HttpRequest {
        let path = format!("/v1/calls/{}", encode_segment(call_id));
        self.request(HttpRequest::get(self.url(&path)), 10)
    }

    /// `POST /v1/calls`. With voice the server speaks the opening line
    /// before it answers.
    pub fn start_call(&self, scenario: Scenario) -> HttpRequest {
        let body = format!("{{\"scenario\":\"{}\"}}", scenario_name(scenario));
        self.request(HttpRequest::post(self.url("/v1/calls")), 20)
            .json_body(body)
    }

    /// `POST /v1/calls/{call_id}/turns` with the learner's words. The
    /// server may ask its model twice and then speak the line.
    pub fn text_turn(&self, call_id: &str, text: &str) -> HttpRequest {
        let body = format!("{{\"text\":{}}}", json::string(text));
        self.request(HttpRequest::post(self.turns_url(call_id)), 60)
            .json_body(body)
    }

    /// `POST /v1/calls/{call_id}/turns` with the learner's recording: the
    /// bytes as the body, the recording's own type as `Content-Type`.
    pub fn audio_turn(&self, call_id: &str, recording: &Recording) -> HttpRequest {
        self.request(HttpRequest::post(self.turns_url(call_id)), 75)
            .header("Content-Type", recording.mime.clone())
            .body(recording.bytes.clone())
    }

    fn turns_url(&self, call_id: &str) -> String {
        self.url(&format!("/v1/calls/{}/turns", encode_segment(call_id)))
    }

    /// `POST /v1/calls/{call_id}/end`. Scoring is one model request.
    pub fn end_call(&self, call_id: &str) -> HttpRequest {
        let path = format!("/v1/calls/{}/end", encode_segment(call_id));
        self.request(HttpRequest::post(self.url(&path)), 45)
    }

    /// `GET` of a caller line's audio, with the same headers. `path` is
    /// one [`audio_path`] accepted.
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
    const PROFILE: &str = r#"{"name": "Grace Mushi", "accent": "us", "difficulty": "standard", "minutes": 4,
 "plan": "free", "calls": 3, "average_score": 84, "streak_days": 3}"#;
    const CALL: &str = r#"{"call_id": "c_9f2…", "number": 4, "scenario": "angry_customer",
 "title": "Angry customer", "caller_name": "Dana",
 "goal": "Let the caller be heard, then fix it",
 "accent": "us", "difficulty": "standard", "minutes": 4, "voice": false,
 "line": {"text": "This is the third time I’ve called today!", "audio_url": null},
 "tips": ["Take ownership", "Ask what happened"]}"#;
    const TURN: &str = r#"{"heard": "I’m sorry about that, let me look into it.",
 "line": {"text": "Fine. But I want it fixed today.", "audio_url": null},
 "tips": ["Give a clear next step", "Confirm the details"],
 "turn": 2, "done": false}"#;
    const REPORT: &str = r#"{"call_id": "c_9f2…", "number": 4, "scenario": "angry_customer",
 "title": "Angry customer", "caller_name": "Dana",
 "started_at": "2026-11-12T09:14:03Z", "duration_seconds": 187,
 "score": 82,
 "skills": {"clarity": 88, "empathy": 74, "listening": 85,
            "problem_solving": 79, "professional_language": 84},
 "did_well": "You apologised early and kept a calm tone.",
 "try_next": "Ask for the order number before explaining.",
 "transcript": [
   {"speaker": "caller", "text": "This is the third time I’ve called today!"},
   {"speaker": "learner", "text": "I’m sorry about that, let me look into it.",
    "note": "Took ownership"}
 ]}"#;
    const CALLS: &str = r#"{"calls": [{"call_id": "c_9f2…", "number": 4, "scenario": "angry_customer",
            "title": "Angry customer", "started_at": "2026-11-12T09:14:03Z",
            "duration_seconds": 187, "minutes": 4, "score": 82}]}"#;

    /// The contract's examples write a call id with an ellipsis; a real
    /// one is `c_` and 32 hex digits.
    fn real(example: &str) -> String {
        example.replace("c_9f2…", "c_9f2")
    }

    #[test]
    fn decodes_the_contracts_health() {
        assert_eq!(
            decode_health(ok(HEALTH)),
            Ok(Health {
                ok: true,
                model: true,
                voice: false,
                database: true
            })
        );
    }

    #[test]
    fn decodes_the_contracts_profile() {
        assert_eq!(
            decode_profile(ok(PROFILE)),
            Ok(Profile {
                name: "Grace Mushi".to_owned(),
                accent: Accent::Us,
                difficulty: Difficulty::Standard,
                length: Length::Four,
                premium: false,
                calls: 3,
                average_score: Some(84),
                streak_days: 3,
            })
        );
        // A device seen for the first time.
        let fresh = r#"{"name":"","accent":"indian","difficulty":"tough","minutes":6,
            "plan":"premium","calls":0,"average_score":null,"streak_days":0}"#;
        let profile = decode_profile(ok(fresh)).expect("a profile");
        assert_eq!(
            (profile.name.as_str(), profile.accent, profile.difficulty),
            ("", Accent::Indian, Difficulty::Tough)
        );
        assert_eq!(
            (profile.length, profile.premium, profile.average_score),
            (Length::Six, true, None)
        );
    }

    #[test]
    fn decodes_the_contracts_call() {
        // The example's id holds an ellipsis, which no real id does: the
        // id goes into a URL and must be printable ASCII.
        assert_eq!(
            decode_call_start(ok(CALL)),
            Err(Failure::Malformed("call_id"))
        );
        let call = decode_call_start(ok(&real(CALL))).expect("a call");
        assert_eq!(call.call_id, "c_9f2");
        assert_eq!(call.number, 4);
        assert_eq!(call.scenario, Some(Scenario::AngryCustomer));
        assert_eq!(call.title, "Angry customer");
        assert_eq!(call.caller_name, "Dana");
        assert_eq!(call.goal, "Let the caller be heard, then fix it");
        assert_eq!(
            (call.accent, call.length, call.voice),
            (Accent::Us, Length::Four, false)
        );
        assert_eq!(
            call.line,
            Line {
                text: "This is the third time I\u{2019}ve called today!".to_owned(),
                audio: None
            }
        );
        assert_eq!(call.tips, ["Take ownership", "Ask what happened"]);
    }

    #[test]
    fn decodes_the_contracts_turn() {
        let turn = decode_turn(ok(TURN)).expect("a turn");
        assert_eq!(
            turn.heard,
            "I\u{2019}m sorry about that, let me look into it."
        );
        assert_eq!(turn.line.text, "Fine. But I want it fixed today.");
        assert_eq!(turn.line.audio, None);
        assert_eq!(turn.tips, ["Give a clear next step", "Confirm the details"]);
        assert_eq!((turn.turn, turn.done), (2, false));

        let spoken = TURN.replace(
            "\"audio_url\": null",
            "\"audio_url\": \"/v1/audio/a_0123456789abcdef01234567.mp3\"",
        );
        let turn = decode_turn(ok(&spoken)).expect("a turn");
        assert_eq!(
            turn.line.audio.as_deref(),
            Some("/v1/audio/a_0123456789abcdef01234567.mp3")
        );
    }

    #[test]
    fn decodes_the_contracts_report() {
        let report = decode_report(ok(&real(REPORT))).expect("a report");
        assert_eq!((report.call_id.as_str(), report.number), ("c_9f2", 4));
        assert_eq!(report.scenario, Some(Scenario::AngryCustomer));
        assert_eq!(
            (report.title.as_str(), report.caller_name.as_str()),
            ("Angry customer", "Dana")
        );
        assert_eq!(report.started_at, "2026-11-12T09:14:03Z");
        assert_eq!(report.duration_seconds, 187);
        assert_eq!(report.scores, Some((82, [88, 74, 85, 79, 84])));
        assert_eq!(
            report.did_well,
            "You apologised early and kept a calm tone."
        );
        assert_eq!(
            report.try_next,
            "Ask for the order number before explaining."
        );
        assert_eq!(
            report.transcript,
            [
                Spoken {
                    caller: true,
                    text: "This is the third time I\u{2019}ve called today!".to_owned(),
                    note: None
                },
                Spoken {
                    caller: false,
                    text: "I\u{2019}m sorry about that, let me look into it.".to_owned(),
                    note: Some("Took ownership".to_owned())
                },
            ]
        );
        assert_eq!(day_label(&report.started_at), "12 Nov");
    }

    #[test]
    fn decodes_an_unscored_report() {
        let unscored = real(REPORT)
            .replace("\"score\": 82", "\"score\": null")
            .replace(
                r#"{"clarity": 88, "empathy": 74, "listening": 85,
            "problem_solving": 79, "professional_language": 84}"#,
                "null",
            );
        let report = decode_report(ok(&unscored)).expect("a report");
        assert_eq!(report.scores, None);
        // A score without skills, or skills without a score, is neither.
        let half = real(REPORT).replace("\"score\": 82", "\"score\": null");
        assert_eq!(decode_report(ok(&half)), Err(Failure::Malformed("score")));
    }

    #[test]
    fn decodes_the_contracts_history() {
        let calls = decode_calls(ok(&real(CALLS))).expect("a history");
        assert_eq!(
            calls,
            [CallSummary {
                call_id: "c_9f2".to_owned(),
                number: 4,
                scenario: Some(Scenario::AngryCustomer),
                title: "Angry customer".to_owned(),
                started_at: "2026-11-12T09:14:03Z".to_owned(),
                duration_seconds: 187,
                score: 82,
            }]
        );
        assert_eq!(decode_calls(ok(r#"{"calls": []}"#)), Ok(Vec::new()));
    }

    #[test]
    fn errors_carry_their_code_and_a_sentence() {
        let ended = decode_turn(status(
            409,
            r#"{"error": {"code": "call_ended", "message": "This call has ended."}}"#,
        ));
        assert_eq!(
            ended,
            Err(Failure::Server {
                status: 409,
                code: "call_ended".to_owned(),
                message: "This call has ended.".to_owned(),
                retry_after: None,
            })
        );
        let limited = decode_turn(Ok(HttpResponse::new(429)
            .with_header("Retry-After", "17")
            .with_body(
                br#"{"error": {"code": "rate_limited", "message": "Slow down."}}"#.to_vec(),
            )));
        let failure = limited.expect_err("an error");
        assert_eq!(failure.status(), Some(429));
        assert_eq!(
            failure.sentence(),
            "Too many requests. Wait 17 seconds and try again."
        );
        assert!(!failure.unreachable());
        // A 400 says what the server said, cleaned.
        let bad = decode_turn(status(
            400,
            "{\"error\": {\"code\": \"bad_request\", \"message\": \"No speech was\\nheard in the recording.\"}}",
        ))
        .expect_err("an error");
        assert_eq!(bad.sentence(), "No speech was heard in the recording.");
        // A proxy's error page is its status and nothing else.
        let page = decode_health(status(502, "<html>Bad Gateway</html>")).expect_err("an error");
        assert_eq!(
            page,
            Failure::Server {
                status: 502,
                code: String::new(),
                message: String::new(),
                retry_after: None
            }
        );
        assert_eq!(page.sentence(), "The AI caller did not answer. Try again.");

        for (error, sentence) in [
            (HttpError::Offline, "The server could not be reached."),
            (HttpError::Timeout, "The server took too long to answer."),
            (
                HttpError::Unsupported,
                "This device cannot make network requests.",
            ),
            (
                HttpError::Failed("cleartext not permitted".into()),
                "The request could not be made: cleartext not permitted.",
            ),
        ] {
            let failure = decode_health(Err(error)).expect_err("an error");
            assert!(failure.unreachable());
            assert_eq!(failure.sentence(), sentence);
        }
    }

    #[test]
    fn unexpected_bodies_are_malformed_not_a_panic() {
        fn malformed<T>(result: Result<T, Failure>) -> bool {
            matches!(result, Err(Failure::Malformed(_)))
        }
        for body in [
            "",
            "null",
            "[]",
            "\"ok\"",
            "{",
            "{}",
            "<html></html>",
            r#"{"ok": "yes", "model": true, "voice": false, "database": true}"#,
            r#"{"ok": true, "model": true, "voice": 1, "database": true}"#,
        ] {
            assert!(malformed(decode_health(ok(body))), "health {body:?}");
            assert!(malformed(decode_profile(ok(body))), "profile {body:?}");
            assert!(malformed(decode_call_start(ok(body))), "call {body:?}");
            assert!(malformed(decode_turn(ok(body))), "turn {body:?}");
            assert!(malformed(decode_report(ok(body))), "report {body:?}");
            assert!(malformed(decode_calls(ok(body))), "calls {body:?}");
        }
        assert!(malformed(decode_health(Ok(
            HttpResponse::new(200).with_body(vec![0xff, 0xfe, 0x00])
        ))));

        // One wrong field is enough.
        for (from, to) in [
            ("\"accent\": \"us\"", "\"accent\": \"martian\""),
            ("\"minutes\": 4", "\"minutes\": 5"),
            ("\"plan\": \"free\"", "\"plan\": \"gold\""),
            ("\"calls\": 3", "\"calls\": -3"),
            ("\"average_score\": 84", "\"average_score\": 840"),
            ("\"streak_days\": 3", "\"streak_days\": 1.5"),
            ("\"name\": \"Grace Mushi\"", "\"name\": 7"),
        ] {
            assert!(PROFILE.contains(from), "{from}");
            assert!(
                malformed(decode_profile(ok(&PROFILE.replace(from, to)))),
                "{to}"
            );
        }
        for (from, to) in [
            ("\"number\": 4", "\"number\": 0"),
            ("\"title\": \"Angry customer\"", "\"title\": \"\""),
            ("\"voice\": false", "\"voice\": null"),
            (
                "{\"text\": \"This is the third time I’ve called today!\", \"audio_url\": null}",
                "{\"text\": \"\", \"audio_url\": null}",
            ),
            ("\"call_id\": \"c_9f2\"", "\"call_id\": \"\""),
            ("\"call_id\": \"c_9f2\"", "\"call_id\": \"a b\""),
        ] {
            let call = real(CALL);
            assert!(call.contains(from), "{from}");
            assert!(
                malformed(decode_call_start(ok(&call.replace(from, to)))),
                "{to}"
            );
        }
        for (from, to) in [
            ("\"clarity\": 88", "\"clarity\": 188"),
            ("\"empathy\": 74, ", ""),
            ("\"speaker\": \"learner\"", "\"speaker\": \"robot\""),
            ("\"duration_seconds\": 187", "\"duration_seconds\": \"187\""),
        ] {
            let report = real(REPORT);
            assert!(report.contains(from), "{from}");
            assert!(
                malformed(decode_report(ok(&report.replace(from, to)))),
                "{to}"
            );
        }
        assert!(malformed(decode_turn(ok(
            &TURN.replace("\"done\": false", "\"done\": 0")
        ))));
        assert!(malformed(decode_calls(ok(
            &real(CALLS).replace("\"score\": 82", "\"score\": null")
        ))));
    }

    #[test]
    fn server_text_is_cleaned_and_capped() {
        assert_eq!(clean("  a\n\tb \u{0}c  ", 20), "a b c");
        assert_eq!(clean("abcdef", 4), "abc\u{2026}");
        assert_eq!(clean("abcd", 4), "abcd");
        assert_eq!(clean("ab cd", 3), "ab\u{2026}");
        assert_eq!(clean("\u{202e}evil\u{200b}", 20), "evil");
        assert_eq!(clean("anything", 0), "");
        // A "word" wider than a line is broken.
        let broken = clean(&"x".repeat(70), 100);
        assert_eq!(
            broken.split(' ').map(str::len).collect::<Vec<_>>(),
            [28, 28, 14]
        );

        // Too many tips, a long tip, a tip that is not a string.
        let turn = TURN.replace(
            r#"["Give a clear next step", "Confirm the details"]"#,
            r#"[7, "", "A tip that is far longer than a chip can hold", "two", "three"]"#,
        );
        let turn = decode_turn(ok(&turn)).expect("a turn");
        assert_eq!(turn.tips.len(), 2);
        assert_eq!(turn.tips[0].chars().count(), 28);
        assert!(turn.tips[0].ends_with('\u{2026}'));
        assert_eq!(turn.tips[1], "two");
        // No tips at all is still a turn.
        let bare = TURN.replace(
            r#""tips": ["Give a clear next step", "Confirm the details"],"#,
            "",
        );
        assert_eq!(
            decode_turn(ok(&bare)).expect("a turn").tips,
            Vec::<String>::new()
        );

        // A very long line is cut, not refused.
        let long = TURN.replace("Fine. But I want it fixed today.", &"word ".repeat(200));
        let turn = decode_turn(ok(&long)).expect("a turn");
        assert_eq!(turn.line.text.chars().count(), LINE_LIMIT);
    }

    #[test]
    fn only_audio_on_this_server_is_followed() {
        assert_eq!(
            audio_path("/v1/audio/a_0123456789abcdef01234567.mp3").as_deref(),
            Some("/v1/audio/a_0123456789abcdef01234567.mp3")
        );
        for url in [
            "",
            "/v1/audio/",
            "https://evil.example/v1/audio/a.mp3",
            "//evil.example/v1/audio/a.mp3",
            "/v1/audio/../profile",
            "/v1/audio/a.mp3?x=1",
            "/v1/audio/a/b.mp3",
            "/v1/audio/a b.mp3",
            "/v1/audio/.hidden",
            "/v1/calls",
            "v1/audio/a.mp3",
        ] {
            assert_eq!(audio_path(url), None, "{url}");
        }
        // A turn with such a URL is a turn without audio.
        let elsewhere = TURN.replace(
            "\"audio_url\": null",
            "\"audio_url\": \"https://evil.example/a.mp3\"",
        );
        assert_eq!(
            decode_turn(ok(&elsewhere)).expect("a turn").line.audio,
            None
        );

        let clip = decode_audio(Ok(HttpResponse::new(200)
            .with_header("Content-Type", "audio/mpeg; charset=binary")
            .with_body(vec![1, 2, 3])));
        assert_eq!(clip, Ok((vec![1, 2, 3], "audio/mpeg".to_owned())));
        assert!(decode_audio(ok("{}")).is_err(), "JSON is not audio");
        assert!(
            decode_audio(Ok(
                HttpResponse::new(200).with_header("Content-Type", "audio/mpeg")
            ))
            .is_err()
        );
        assert_eq!(
            decode_audio(status(404, "")).expect_err("gone").status(),
            Some(404)
        );
    }

    #[test]
    fn addresses_are_normalised_or_refused() {
        assert_eq!(
            normalize_base_url("  HTTP://127.0.0.1:8787/ "),
            Ok("http://127.0.0.1:8787".to_owned())
        );
        assert_eq!(
            normalize_base_url("https://callready.example.com/api/"),
            Ok("https://callready.example.com/api".to_owned())
        );
        assert_eq!(
            normalize_base_url("http://[::1]:8787"),
            Ok("http://[::1]:8787".to_owned())
        );
        assert_eq!(normalize_base_url("   "), Err(UrlProblem::Empty));
        assert_eq!(
            normalize_base_url("127.0.0.1:8787"),
            Err(UrlProblem::Scheme)
        );
        assert_eq!(
            normalize_base_url("ftp://example.com"),
            Err(UrlProblem::Scheme)
        );
        for bad in [
            "http://",
            "http:///path",
            "http://exa mple.com",
            "http://user@example.com",
            "http://example.com/a?b=c",
            "http://example.com/#x",
            "http://example.com/a/../b",
            "http://example.com//a",
            "http://...",
        ] {
            assert_eq!(normalize_base_url(bad), Err(UrlProblem::Host), "{bad}");
        }
        assert_eq!(
            normalize_base_url(DEFAULT_BASE_URL).as_deref(),
            Ok(DEFAULT_BASE_URL)
        );
    }

    #[test]
    fn requests_are_built_as_the_contract_says() {
        let client = Client::new(DEFAULT_BASE_URL, "0123456789abcdef0123456789abcdef");
        let health = client.health();
        assert_eq!(
            (health.method, health.url.as_str()),
            (HttpMethod::Get, "http://127.0.0.1:8787/v1/health")
        );
        assert!(health.body.is_empty());

        for request in [
            client.health(),
            client.profile(),
            client.calls(),
            client.call("c_1"),
            client.start_call(Scenario::AngryCustomer),
            client.text_turn("c_1", "hello"),
            client.end_call("c_1"),
            client.audio("/v1/audio/a_1.mp3"),
            client.put_profile(&ProfilePatch::default()),
        ] {
            assert_eq!(
                request.header_value("X-Device-Id"),
                Some("0123456789abcdef0123456789abcdef"),
                "{}",
                request.url
            );
            assert!(request.url.starts_with("http://127.0.0.1:8787/v1/"));
            assert_eq!(request.header_value("Authorization"), None, "no secrets");
        }

        let start = client.start_call(Scenario::RefundRequest);
        assert_eq!(
            (start.method, start.url.as_str()),
            (HttpMethod::Post, "http://127.0.0.1:8787/v1/calls")
        );
        assert_eq!(start.header_value("Content-Type"), Some("application/json"));
        assert_eq!(start.body, br#"{"scenario":"refund_request"}"#);

        let turn = client.text_turn("c_9f2", "I\u{2019}m \"sorry\"\nabout that\\");
        assert_eq!(turn.url, "http://127.0.0.1:8787/v1/calls/c_9f2/turns");
        assert_eq!(turn.header_value("Content-Type"), Some("application/json"));
        let body = json::parse(std::str::from_utf8(&turn.body).expect("utf-8")).expect("json");
        assert_eq!(
            body.get("text").and_then(Value::as_str),
            Some("I\u{2019}m \"sorry\"\nabout that\\")
        );

        let recording = Recording {
            bytes: vec![0, 1, 2, 255],
            mime: "audio/mp4".to_owned(),
            duration: Duration::from_secs(2),
        };
        let audio = client.audio_turn("c_9f2", &recording);
        assert_eq!(audio.method, HttpMethod::Post);
        assert_eq!(audio.url, "http://127.0.0.1:8787/v1/calls/c_9f2/turns");
        assert_eq!(audio.header_value("Content-Type"), Some("audio/mp4"));
        assert_eq!(audio.body, [0, 1, 2, 255]);
        assert_eq!(
            audio
                .headers
                .iter()
                .filter(|(name, _)| name.eq_ignore_ascii_case("content-type"))
                .count(),
            1
        );

        let end = client.end_call("c_9f2");
        assert_eq!(
            (end.method, end.url.as_str()),
            (HttpMethod::Post, "http://127.0.0.1:8787/v1/calls/c_9f2/end")
        );
        assert_eq!(
            client.call("c_9f2").url,
            "http://127.0.0.1:8787/v1/calls/c_9f2"
        );
        // An id is one path segment whatever it holds.
        assert_eq!(
            client.call("a/b ?#%é").url,
            "http://127.0.0.1:8787/v1/calls/a%2Fb%20%3F%23%25%C3%A9"
        );
        assert_eq!(
            client.audio("/v1/audio/a_1.mp3").url,
            "http://127.0.0.1:8787/v1/audio/a_1.mp3"
        );

        let put = client.put_profile(&ProfilePatch {
            name: Some("Asha \"A\"".to_owned()),
            accent: Some(Accent::Uk),
            difficulty: Some(Difficulty::Tough),
            length: Some(Length::Six),
            premium: Some(true),
        });
        assert_eq!(
            (put.method, put.url.as_str()),
            (HttpMethod::Put, "http://127.0.0.1:8787/v1/profile")
        );
        assert_eq!(
            std::str::from_utf8(&put.body).expect("utf-8"),
            r#"{"name":"Asha \"A\"","accent":"uk","difficulty":"tough","minutes":6,"plan":"premium"}"#
        );
        assert_eq!(
            ProfilePatch {
                premium: Some(false),
                ..ProfilePatch::default()
            }
            .to_json(),
            r#"{"plan":"free"}"#
        );
    }

    #[test]
    fn ids_names_and_days() {
        assert_eq!(
            device_id("", "0123456789abcdef0123456789abcdef"),
            "0123456789abcdef0123456789abcdef"
        );
        assert_eq!(
            device_id("test_", "0123456789abcdef0123456789abcdef"),
            "test_0123456789abcdef0123456789abcdef"
        );
        assert_eq!(device_id("", "a b/c"), "abc00000");
        assert_eq!(device_id("", &"z".repeat(100)).len(), 64);

        for scenario in Scenario::ALL {
            assert_eq!(scenario_named(scenario_name(scenario)), Some(scenario));
        }
        assert_eq!(scenario_named("lost_luggage"), None);
        assert_eq!(
            tidy_reply("  I am\n sorry \t about that. "),
            "I am sorry about that."
        );

        assert_eq!(day_label("2026-01-03T00:00:00Z"), "3 Jan");
        for bad in [
            "",
            "yesterday",
            "2026-13-03T00:00:00Z",
            "2026-00-03",
            "2026-11-40",
            "2026/11/12",
        ] {
            assert_eq!(day_label(bad), "", "{bad}");
        }
    }
}
