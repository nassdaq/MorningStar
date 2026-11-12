//! A call played by the server: its state, and what each event does to it.
//!
//! [`OnlineCall`] is a state machine with no I/O in it. Every event (a tap,
//! a second passing, an answer from the server, the microphone or the
//! speaker) is a method that changes the state and returns the
//! [`Command`]s to carry out; `lib.rs` turns those into requests to the
//! platform and feeds the answers back in. So all of a call's behaviour,
//! failures included, is tested here without a screen or a server.
//!
//! A call is spoken when the server has a voice ([`OnlineCall::voice`]) and
//! typed when it has not; a spoken call can be typed too ("type instead",
//! or a refused microphone).
//!
//! Answers can arrive late: after the learner has retried, ended the call
//! or hung up. Each request therefore carries a number, and an answer
//! whose number is not the one the call is waiting for changes nothing:
//!
//! * [`OnlineCall::seq`] numbers what the learner sends (a turn, the stop
//!   of a recording, the end of the call);
//! * [`OnlineCall::line_no`] numbers the caller's lines, for their audio.
//!
//! `lib.rs` adds a number per call on top, so that nothing of one call can
//! reach the next.

use std::time::Duration;

use morningstar::ui::prelude::{AudioClip, AudioError, Recording};

use crate::api::{self, CallReport, CallStart, Failure, Turn};
use crate::model::{Accent, Length};
use crate::script::Scenario;

/// A recording stops by itself at this length (the platform's limit).
pub const RECORDING_LIMIT: u32 = 60;
/// A recording shorter than this holds no words.
const SHORTEST_RECORDING: Duration = Duration::from_millis(400);

/// What the learner's side of the call is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Waiting for the learner: they may type, or tap the microphone.
    Idle,
    /// The microphone has been asked for.
    Starting,
    /// Recording, for this many seconds so far.
    Recording(u32),
    /// The recording has been asked to stop.
    Stopping,
    /// A turn is with the server: the caller is "thinking". `audio` says
    /// whether it was spoken.
    Sending {
        /// The turn was a recording.
        audio: bool,
    },
    /// The call has been ended and its report asked for.
    Ending,
    /// The report could not be fetched; it can be asked for again.
    EndFailed,
}

/// What the caller's side of the call is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    /// Nothing to hear.
    Silent,
    /// The line's audio is being fetched.
    Fetching,
    /// The line is being played.
    Speaking,
}

/// A line of news about the call, shown above the input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    /// What happened, in plain words.
    pub text: String,
    /// Whether "Retry" would send again what failed.
    pub retry: bool,
}

/// What the application is to do for the call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// `POST …/turns` with these words.
    PostText {
        /// The number the answer must carry.
        seq: u32,
        /// The learner's words.
        text: String,
    },
    /// `POST …/turns` with this recording.
    PostAudio {
        /// The number the answer must carry.
        seq: u32,
        /// What the learner said.
        recording: Recording,
    },
    /// `GET` the audio of the caller's line.
    FetchAudio {
        /// The line it belongs to.
        line: u32,
        /// Its path on the server.
        path: String,
    },
    /// Play the caller's line.
    Play {
        /// The line it is.
        line: u32,
        /// The sound.
        clip: AudioClip,
    },
    /// Stop whatever is playing.
    StopPlayback,
    /// Ask for the microphone and start recording.
    StartRecording,
    /// Stop recording and hand over what was recorded.
    StopRecording {
        /// The number the answer must carry.
        seq: u32,
    },
    /// `POST …/end`.
    End {
        /// The number the answer must carry.
        seq: u32,
    },
    /// The call is over: show this report.
    Report(CallReport),
    /// The call is gone (the server does not know it): leave it, saying
    /// this.
    Lost(String),
}

/// A call the server is playing.
#[derive(Clone, Debug, PartialEq)]
pub struct OnlineCall {
    /// The call's id on the server.
    pub id: String,
    /// Its number in the learner's history.
    pub number: u32,
    /// What is being practised.
    pub scenario: Scenario,
    /// The scenario's name, as the server gave it.
    pub title: String,
    /// The caller's first name.
    pub caller: String,
    /// What the learner is to achieve.
    pub goal: String,
    /// The caller's accent.
    pub accent: Accent,
    /// The call's length in seconds.
    pub limit: u32,
    /// The server speaks this call: lines have audio, turns may be audio.
    pub voice: bool,
    /// The caller's current line.
    pub line: String,
    /// Two pieces of advice for the answer.
    pub tips: Vec<String>,
    /// What the learner said last, as the server heard it.
    pub said: Option<String>,
    /// How many turns the learner has had.
    pub turns: u32,
    /// Seconds on the clock.
    pub elapsed: u32,
    /// Sound off: lines are not played (they still show).
    pub muted: bool,
    /// The learner types in a spoken call.
    pub typing: bool,
    /// The microphone cannot be used in this call (refused, or absent):
    /// typing is the only way.
    pub mic_denied: bool,
    /// What the learner has typed and not yet had answered.
    pub draft: String,
    /// The learner's side.
    pub phase: Phase,
    /// The caller's side.
    pub sound: Sound,
    /// Seconds for which the caller's line is still shown as repeated.
    pub highlight: u8,
    /// News about the call.
    pub notice: Option<Notice>,
    /// The caller has said their last line.
    pub done: bool,
    /// The number of the last thing the learner sent.
    pub seq: u32,
    /// The number of the caller's current line.
    pub line_no: u32,
    /// The path of the current line's audio.
    audio: Option<String>,
    /// The current line's audio, once fetched: what "replay" plays.
    clip: Option<AudioClip>,
    /// A recording that has not been answered yet: kept so that a failed
    /// turn can be sent again.
    kept: Option<Recording>,
    /// The report, when it arrived while the closing line was still being
    /// said.
    held: Option<CallReport>,
}

impl OnlineCall {
    /// The call the server has just connected, and what to do first (fetch
    /// the opening line's audio, if it has any).
    pub fn connect(start: CallStart, asked: Scenario, muted: bool) -> (Self, Vec<Command>) {
        let mut call = Self {
            id: start.call_id,
            number: start.number,
            scenario: start.scenario.unwrap_or(asked),
            title: start.title,
            caller: start.caller_name,
            goal: start.goal,
            accent: start.accent,
            limit: length_seconds(start.length),
            voice: start.voice,
            line: String::new(),
            tips: Vec::new(),
            said: None,
            turns: 0,
            elapsed: 0,
            muted,
            typing: false,
            mic_denied: false,
            draft: String::new(),
            phase: Phase::Idle,
            sound: Sound::Silent,
            highlight: 0,
            notice: None,
            done: false,
            seq: 0,
            line_no: 0,
            audio: None,
            clip: None,
            kept: None,
            held: None,
        };
        call.tips = start.tips;
        let commands = call.say(start.line);
        (call, commands)
    }

    // ----------------------------------------------------------- reading

    /// True when the learner answers by speaking: a spoken call that is
    /// not being typed.
    pub fn speaks(&self) -> bool {
        self.voice && !self.typing
    }

    /// True once the call has been ended (the report is on its way).
    pub fn is_over(&self) -> bool {
        matches!(self.phase, Phase::Ending | Phase::EndFailed)
    }

    /// True while the microphone can be tapped: to start when the learner
    /// may speak and the caller is not speaking, or to stop a recording.
    pub fn mic_ready(&self) -> bool {
        match self.phase {
            Phase::Idle => self.speaks() && self.sound != Sound::Speaking && !self.done,
            Phase::Recording(_) => true,
            _ => false,
        }
    }

    /// True while the draft can be edited and sent.
    pub fn can_type(&self) -> bool {
        self.phase == Phase::Idle && !self.speaks() && !self.done
    }

    /// True when Send would send something.
    pub fn can_send(&self) -> bool {
        self.can_type() && !api::tidy_reply(&self.draft).is_empty()
    }

    /// The clock: "02:14".
    pub fn clock(&self) -> String {
        format!("{:02}:{:02}", self.elapsed / 60, self.elapsed % 60)
    }

    /// The share of the call's length used, from 0 to 1.
    pub fn time_used(&self) -> f32 {
        (self.elapsed as f32 / self.limit.max(1) as f32).min(1.0)
    }

    /// The recording waiting to be sent again, if a spoken turn failed.
    pub fn kept(&self) -> Option<&Recording> {
        self.kept.as_ref()
    }

    /// The current line's audio, once it has been fetched.
    pub fn clip(&self) -> Option<&AudioClip> {
        self.clip.as_ref()
    }

    // ------------------------------------------------------------ events

    /// One second passes.
    pub fn tick(&mut self) -> Vec<Command> {
        if !self.is_over() {
            self.elapsed = self.elapsed.saturating_add(1);
        }
        self.highlight = self.highlight.saturating_sub(1);
        if let Phase::Recording(seconds) = &mut self.phase {
            *seconds += 1;
            if *seconds >= RECORDING_LIMIT {
                self.tell("That is the 60 second limit. Sending what you said.");
                return self.stop_recording();
            }
        }
        Vec::new()
    }

    /// The learner typed: the whole new text.
    pub fn input(&mut self, text: String) {
        if self.phase == Phase::Idle && !self.done {
            self.draft = text.chars().take(api::REPLY_LIMIT).collect();
        }
    }

    /// Send: the draft goes to the server. It stays in the field until the
    /// caller has answered, so a failure loses nothing.
    pub fn send(&mut self) -> Vec<Command> {
        if !self.can_send() {
            return Vec::new();
        }
        let text = api::tidy_reply(&self.draft);
        self.notice = None;
        self.kept = None;
        self.phase = Phase::Sending { audio: false };
        self.seq += 1;
        vec![Command::PostText {
            seq: self.seq,
            text,
        }]
    }

    /// The microphone was tapped: start recording, or stop and send.
    pub fn mic(&mut self) -> Vec<Command> {
        if !self.mic_ready() {
            return Vec::new();
        }
        match self.phase {
            Phase::Recording(_) => self.stop_recording(),
            _ => {
                self.notice = None;
                // A new recording replaces one that was never answered.
                self.kept = None;
                self.phase = Phase::Starting;
                vec![Command::StartRecording]
            }
        }
    }

    fn stop_recording(&mut self) -> Vec<Command> {
        self.phase = Phase::Stopping;
        self.seq += 1;
        vec![Command::StopRecording { seq: self.seq }]
    }

    /// The microphone started, or did not.
    pub fn recording_started(&mut self, result: Result<(), AudioError>) -> Vec<Command> {
        if self.phase != Phase::Starting {
            // The call moved on (it was ended, say). A microphone that did
            // start must not be left running.
            return match result {
                Ok(()) => vec![Command::StopRecording { seq: 0 }],
                Err(_) => Vec::new(),
            };
        }
        self.phase = Phase::Idle;
        match result {
            Ok(()) => self.phase = Phase::Recording(0),
            Err(AudioError::PermissionDenied) => {
                self.typing = true;
                self.mic_denied = true;
                self.tell(
                    "The microphone is not allowed for CallReady. Type your answers for this call.",
                );
            }
            Err(AudioError::Unsupported) => {
                self.typing = true;
                self.mic_denied = true;
                self.tell("This device cannot record. Type your answers for this call.");
            }
            Err(AudioError::Busy) => {
                self.tell("The microphone is in use by something else. Try again in a moment.");
            }
            Err(_) => self.tell("The microphone did not start. Try again, or type instead."),
        }
        Vec::new()
    }

    /// The recording, or why there is none.
    pub fn recorded(&mut self, seq: u32, result: Result<Recording, AudioError>) -> Vec<Command> {
        if self.phase != Phase::Stopping || seq != self.seq {
            return Vec::new();
        }
        self.phase = Phase::Idle;
        match result {
            Ok(recording)
                if recording.bytes.is_empty() || recording.duration < SHORTEST_RECORDING =>
            {
                self.tell("That was too short to hear. Tap the microphone, speak, then tap again.");
                Vec::new()
            }
            Ok(recording) => {
                self.kept = Some(recording.clone());
                self.post_audio(recording)
            }
            Err(AudioError::Interrupted) => {
                self.tell("The recording was interrupted. Tap the microphone to try again.");
                Vec::new()
            }
            Err(AudioError::NotRecording) => {
                self.tell("Nothing was recorded. Tap the microphone to try again.");
                Vec::new()
            }
            Err(_) => {
                self.tell("The recording failed. Try again, or type instead.");
                Vec::new()
            }
        }
    }

    fn post_audio(&mut self, recording: Recording) -> Vec<Command> {
        self.phase = Phase::Sending { audio: true };
        self.seq += 1;
        vec![Command::PostAudio {
            seq: self.seq,
            recording,
        }]
    }

    /// The server's answer to a turn.
    pub fn replied(&mut self, seq: u32, result: Result<Turn, Failure>) -> Vec<Command> {
        let Phase::Sending { audio } = self.phase else {
            return Vec::new();
        };
        if seq != self.seq {
            return Vec::new();
        }
        self.phase = Phase::Idle;
        match result {
            Ok(turn) => {
                self.said = Some(turn.heard);
                // `turn` numbers the turn that comes next.
                self.turns = turn.turn.saturating_sub(1).max(self.turns + 1);
                self.tips = turn.tips;
                self.kept = None;
                self.notice = None;
                if !audio {
                    self.draft.clear();
                }
                self.done = turn.done;
                let mut commands = self.say(turn.line);
                if self.done {
                    // The closing line: the call ends itself, and its
                    // report is fetched while the line is read or heard.
                    commands.extend(self.end());
                }
                commands
            }
            Err(failure) => self.turn_failed(audio, failure),
        }
    }

    fn turn_failed(&mut self, audio: bool, failure: Failure) -> Vec<Command> {
        match failure.status() {
            // The call ended on the server (its time ran out, or the caller
            // hung up in an answer that never arrived): to the report.
            Some(409) => {
                self.kept = None;
                self.end()
            }
            Some(404) => vec![Command::Lost(
                "The server no longer knows that call. Nothing was scored.".to_owned(),
            )],
            Some(400) if audio => {
                self.kept = None;
                self.tell("I could not hear any words in that. Tap the microphone and try again.");
                Vec::new()
            }
            // What was typed stays in the field to be changed.
            Some(400) => {
                self.tell(failure.sentence());
                Vec::new()
            }
            Some(413) => {
                self.kept = None;
                self.tell("That recording was too large to send. Try a shorter answer.");
                Vec::new()
            }
            Some(503) if audio => {
                self.kept = None;
                self.typing = true;
                self.tell("The server has no voice right now. Type your answer instead.");
                Vec::new()
            }
            // Offline, a timeout, the model failing (502), too many
            // requests (429), anything unexpected: nothing was stored, and
            // the same turn can be sent again.
            _ => {
                self.notice = Some(Notice {
                    text: failure.sentence(),
                    retry: true,
                });
                Vec::new()
            }
        }
    }

    /// "Retry": sends again what failed. The typed draft as it now is, the
    /// kept recording, or the request for the report.
    pub fn retry(&mut self) -> Vec<Command> {
        if !self.notice.as_ref().is_some_and(|notice| notice.retry) {
            return Vec::new();
        }
        match self.phase {
            Phase::EndFailed => self.end(),
            Phase::Idle => match self.kept.clone() {
                Some(recording) => {
                    self.notice = None;
                    self.post_audio(recording)
                }
                None => {
                    // A spoken call whose text turn failed is still typed.
                    let commands = self.send_draft();
                    if commands.is_empty() {
                        self.notice = None;
                    }
                    commands
                }
            },
            _ => Vec::new(),
        }
    }

    fn send_draft(&mut self) -> Vec<Command> {
        let text = api::tidy_reply(&self.draft);
        if text.is_empty() || self.done {
            return Vec::new();
        }
        self.notice = None;
        self.phase = Phase::Sending { audio: false };
        self.seq += 1;
        vec![Command::PostText {
            seq: self.seq,
            text,
        }]
    }

    /// "Type instead" in a spoken call.
    pub fn type_instead(&mut self) {
        if self.voice && self.phase == Phase::Idle {
            self.typing = true;
            self.notice = None;
        }
    }

    /// Back to speaking, unless the microphone cannot be used.
    pub fn speak_instead(&mut self) {
        if self.voice && !self.mic_denied && self.phase == Phase::Idle {
            self.typing = false;
            self.notice = None;
        }
    }

    /// The loudspeaker: the caller's line once more. Its audio if there is
    /// any and the sound is on, otherwise the line lights up.
    pub fn replay(&mut self) -> Vec<Command> {
        let free = self.sound == Sound::Silent
            && matches!(self.phase, Phase::Idle | Phase::Sending { .. });
        if self.muted || !free {
            self.highlight = 2;
            return Vec::new();
        }
        match (self.clip.clone(), self.audio.clone()) {
            (Some(clip), _) => {
                self.sound = Sound::Speaking;
                vec![Command::Play {
                    line: self.line_no,
                    clip,
                }]
            }
            (None, Some(path)) => {
                self.sound = Sound::Fetching;
                vec![Command::FetchAudio {
                    line: self.line_no,
                    path,
                }]
            }
            (None, None) => {
                self.highlight = 2;
                Vec::new()
            }
        }
    }

    /// The sound button: off stops what is playing and plays nothing more;
    /// lines still show.
    pub fn toggle_mute(&mut self) -> Vec<Command> {
        self.muted = !self.muted;
        if !self.muted {
            return Vec::new();
        }
        match std::mem::replace(&mut self.sound, Sound::Silent) {
            Sound::Speaking => {
                let mut commands = vec![Command::StopPlayback];
                commands.extend(self.release());
                commands
            }
            _ => self.release(),
        }
    }

    /// The audio of a line arrived, or did not.
    pub fn audio_fetched(
        &mut self,
        line: u32,
        result: Result<(Vec<u8>, String), Failure>,
    ) -> Vec<Command> {
        if line != self.line_no || self.sound != Sound::Fetching {
            // An older line's audio, or the sound was switched off
            // meanwhile: keep it for "replay" if it is this line's.
            if line == self.line_no
                && let Ok((bytes, mime)) = result
            {
                self.clip = Some(AudioClip::new(bytes, mime));
            }
            return Vec::new();
        }
        self.sound = Sound::Silent;
        let Ok((bytes, mime)) = result else {
            // The line is on the screen; its sound is not worth a message.
            return self.release();
        };
        let clip = AudioClip::new(bytes, mime);
        self.clip = Some(clip.clone());
        let recording = matches!(
            self.phase,
            Phase::Starting | Phase::Recording(_) | Phase::Stopping
        );
        if self.muted || recording {
            return self.release();
        }
        self.sound = Sound::Speaking;
        vec![Command::Play { line, clip }]
    }

    /// A line finished playing, or was stopped.
    pub fn played(&mut self, line: u32, result: Result<(), AudioError>) -> Vec<Command> {
        if line != self.line_no || self.sound != Sound::Speaking {
            return Vec::new();
        }
        self.sound = Sound::Silent;
        if let Err(error) = result
            && !matches!(error, AudioError::Interrupted)
            && self.notice.is_none()
            && !self.is_over()
        {
            self.tell("The caller\u{2019}s voice could not be played. The line is on the screen.");
        }
        self.release()
    }

    /// The red button, or the caller's closing line: ends the call and
    /// asks for its report.
    pub fn end(&mut self) -> Vec<Command> {
        if self.phase == Phase::Ending {
            return Vec::new();
        }
        let mut commands = Vec::new();
        if matches!(
            self.phase,
            Phase::Starting | Phase::Recording(_) | Phase::Stopping
        ) {
            // Whatever was being recorded is not wanted.
            commands.push(Command::StopRecording { seq: 0 });
        }
        if self.sound != Sound::Silent && !self.done {
            // Hung up on: the caller stops talking.
            if self.sound == Sound::Speaking {
                commands.push(Command::StopPlayback);
            }
            self.sound = Sound::Silent;
        }
        self.notice = None;
        self.phase = Phase::Ending;
        self.seq += 1;
        commands.push(Command::End { seq: self.seq });
        commands
    }

    /// The report, or why there is none.
    pub fn ended(&mut self, seq: u32, result: Result<CallReport, Failure>) -> Vec<Command> {
        if self.phase != Phase::Ending || seq != self.seq {
            return Vec::new();
        }
        match result {
            // The closing line is still being said: the report waits.
            Ok(report) if self.done && self.sound != Sound::Silent => {
                self.held = Some(report);
                Vec::new()
            }
            Ok(report) => vec![Command::Report(report)],
            Err(failure) if failure.status() == Some(404) => vec![Command::Lost(
                "The server no longer knows that call. Nothing was scored.".to_owned(),
            )],
            Err(failure) => {
                self.phase = Phase::EndFailed;
                self.notice = Some(Notice {
                    text: format!("Your report is not ready. {}", failure.sentence()),
                    retry: true,
                });
                Vec::new()
            }
        }
    }

    /// What to stop when the learner leaves the call without ending it.
    pub fn abandon(&mut self) -> Vec<Command> {
        let mut commands = Vec::new();
        if matches!(
            self.phase,
            Phase::Starting | Phase::Recording(_) | Phase::Stopping
        ) {
            commands.push(Command::StopRecording { seq: 0 });
        }
        if self.sound == Sound::Speaking {
            commands.push(Command::StopPlayback);
        }
        self.sound = Sound::Silent;
        commands
    }

    // ----------------------------------------------------------- helpers

    fn tell(&mut self, text: impl Into<String>) {
        self.notice = Some(Notice {
            text: text.into(),
            retry: false,
        });
    }

    /// The caller says a new line.
    fn say(&mut self, line: api::Line) -> Vec<Command> {
        self.line = line.text;
        self.line_no += 1;
        self.highlight = 0;
        self.clip = None;
        self.audio = line.audio;
        match &self.audio {
            Some(path) if !self.muted => {
                self.sound = Sound::Fetching;
                vec![Command::FetchAudio {
                    line: self.line_no,
                    path: path.clone(),
                }]
            }
            _ => {
                self.sound = Sound::Silent;
                Vec::new()
            }
        }
    }

    /// The caller has stopped: a report that was waiting for that shows.
    fn release(&mut self) -> Vec<Command> {
        match self.held.take() {
            Some(report) => vec![Command::Report(report)],
            None => Vec::new(),
        }
    }
}

/// A call's length in seconds.
fn length_seconds(length: Length) -> u32 {
    length.minutes() * 60
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{Line, Spoken};

    const AUDIO: &str = "/v1/audio/a_000000000000000000000001.mp3";

    fn start(voice: bool) -> CallStart {
        CallStart {
            call_id: "c_1".to_owned(),
            number: 4,
            scenario: Some(Scenario::AngryCustomer),
            title: "Angry customer".to_owned(),
            caller_name: "Dana".to_owned(),
            goal: "Let the caller be heard, then fix it".to_owned(),
            accent: Accent::Us,
            length: Length::Four,
            voice,
            line: Line {
                text: "This is the third time I\u{2019}ve called today!".to_owned(),
                audio: voice.then(|| AUDIO.to_owned()),
            },
            tips: vec!["Take ownership".to_owned(), "Ask what happened".to_owned()],
        }
    }

    fn turn(number: u32, done: bool, audio: bool) -> Turn {
        Turn {
            heard: format!("heard {number}"),
            line: Line {
                text: format!("line {number}"),
                audio: audio.then(|| AUDIO.to_owned()),
            },
            tips: vec!["one".to_owned(), "two".to_owned()],
            turn: number + 1,
            done,
        }
    }

    fn report(scored: bool) -> CallReport {
        CallReport {
            call_id: "c_1".to_owned(),
            number: 4,
            scenario: Some(Scenario::AngryCustomer),
            title: "Angry customer".to_owned(),
            caller_name: "Dana".to_owned(),
            started_at: "2026-11-12T09:14:03Z".to_owned(),
            duration_seconds: 187,
            scores: scored.then_some((82, [88, 74, 85, 79, 84])),
            did_well: "You apologised early.".to_owned(),
            try_next: "Ask for the order number.".to_owned(),
            transcript: vec![Spoken {
                caller: true,
                text: "Hello".to_owned(),
                note: None,
            }],
        }
    }

    fn recording(millis: u64) -> Recording {
        Recording {
            bytes: vec![9; 64],
            mime: "audio/mp4".to_owned(),
            duration: Duration::from_millis(millis),
        }
    }

    fn server(status: u16) -> Failure {
        Failure::Server {
            status,
            code: String::new(),
            message: "The server said so.".to_owned(),
            retry_after: None,
        }
    }

    fn text_call() -> OnlineCall {
        let (call, commands) = OnlineCall::connect(start(false), Scenario::AngryCustomer, false);
        assert_eq!(commands, [], "a text call has nothing to fetch");
        call
    }

    /// A spoken call whose opening line has been fetched and played.
    fn voice_call() -> OnlineCall {
        let (mut call, commands) = OnlineCall::connect(start(true), Scenario::AngryCustomer, false);
        assert_eq!(
            commands,
            [Command::FetchAudio {
                line: 1,
                path: AUDIO.to_owned()
            }]
        );
        assert_eq!(call.sound, Sound::Fetching);
        let commands = call.audio_fetched(1, Ok((vec![1, 2, 3], "audio/mpeg".to_owned())));
        assert_eq!(
            commands,
            [Command::Play {
                line: 1,
                clip: AudioClip::new(vec![1, 2, 3], "audio/mpeg")
            }]
        );
        assert_eq!(call.sound, Sound::Speaking);
        assert!(!call.mic_ready(), "the caller is speaking");
        assert_eq!(call.played(1, Ok(())), []);
        assert_eq!(call.sound, Sound::Silent);
        assert!(call.mic_ready());
        call
    }

    #[test]
    fn a_text_call_from_the_first_line_to_the_report() {
        let mut call = text_call();
        assert_eq!((call.number, call.limit, call.turns), (4, 240, 0));
        assert_eq!(
            call.line,
            "This is the third time I\u{2019}ve called today!"
        );
        assert!(!call.speaks() && call.can_type() && !call.can_send());
        assert_eq!(call.send(), [], "nothing typed");

        call.input("  I am\nsorry   about that. ".to_owned());
        assert!(call.can_send());
        assert_eq!(
            call.send(),
            [Command::PostText {
                seq: 1,
                text: "I am sorry about that.".to_owned()
            }]
        );
        assert_eq!(call.phase, Phase::Sending { audio: false });
        assert!(!call.can_type());
        call.input("changed".to_owned());
        assert_eq!(
            call.draft, "  I am\nsorry   about that. ",
            "held while sending"
        );
        assert_eq!(call.send(), [], "one turn at a time");

        assert_eq!(call.replied(1, Ok(turn(1, false, false))), []);
        assert_eq!(call.phase, Phase::Idle);
        assert_eq!((call.turns, call.line.as_str()), (1, "line 1"));
        assert_eq!(call.said.as_deref(), Some("heard 1"));
        assert_eq!(call.tips, ["one", "two"]);
        assert_eq!(call.draft, "", "answered: the field is empty again");

        // The closing line ends the call by itself.
        call.input("Thank you, goodbye.".to_owned());
        assert_eq!(call.send().len(), 1);
        assert_eq!(
            call.replied(2, Ok(turn(2, true, false))),
            [Command::End { seq: 3 }]
        );
        assert!(call.done && call.is_over());
        assert_eq!(call.line, "line 2", "the closing line shows");
        assert!(!call.can_type() && !call.mic_ready());
        assert_eq!(call.end(), [], "ending twice asks once");
        assert_eq!(
            call.ended(3, Ok(report(true))),
            [Command::Report(report(true))]
        );
    }

    #[test]
    fn the_clock_counts_until_the_call_is_ended() {
        let mut call = text_call();
        for _ in 0..134 {
            assert_eq!(call.tick(), []);
        }
        assert_eq!(call.clock(), "02:14");
        assert!((call.time_used() - 134.0 / 240.0).abs() < 1e-6);
        for _ in 0..200 {
            call.tick();
        }
        assert_eq!(call.time_used(), 1.0, "the bar is full, the call goes on");
        let clock = call.clock();
        assert_eq!(call.end(), [Command::End { seq: 1 }]);
        call.tick();
        assert_eq!(call.clock(), clock, "ended: the clock stands");
    }

    #[test]
    fn late_and_stray_answers_change_nothing() {
        let mut call = text_call();
        let untouched = call.clone();
        // Nothing was sent: no answer is expected.
        assert_eq!(call.replied(0, Ok(turn(1, false, false))), []);
        assert_eq!(call.replied(1, Ok(turn(1, true, false))), []);
        assert_eq!(call.ended(1, Ok(report(true))), []);
        assert_eq!(call.recorded(1, Ok(recording(2000))), []);
        assert_eq!(
            call.audio_fetched(1, Ok((vec![1], "audio/mpeg".into()))),
            []
        );
        assert_eq!(
            call.audio_fetched(7, Ok((vec![1], "audio/mpeg".into()))),
            []
        );
        assert_eq!(call.played(1, Ok(())), []);
        assert_eq!(call.retry(), []);
        call.clip = None;
        assert_eq!(call, untouched);

        // A turn fails and is retried: the first attempt's answer, coming
        // late, is not the one waited for.
        call.input("Hello".to_owned());
        assert_eq!(call.send().len(), 1);
        assert_eq!(call.replied(1, Err(Failure::Timeout)), []);
        assert_eq!(
            call.retry(),
            [Command::PostText {
                seq: 2,
                text: "Hello".to_owned()
            }]
        );
        assert_eq!(call.replied(1, Ok(turn(9, true, false))), []);
        assert_eq!(call.phase, Phase::Sending { audio: false });
        assert_eq!(call.replied(2, Ok(turn(1, false, false))), []);
        assert_eq!(call.line, "line 1");

        // Ended while a turn is out: the turn's answer is ignored, and so
        // is a report for an older request.
        call.input("Again".to_owned());
        assert_eq!(call.send().len(), 1);
        assert_eq!(call.end(), [Command::End { seq: 4 }]);
        assert_eq!(call.replied(3, Ok(turn(2, false, false))), []);
        assert_eq!(call.line, "line 1");
        assert_eq!(call.ended(3, Ok(report(true))), []);
        assert_eq!(call.ended(4, Ok(report(true))).len(), 1);
    }

    #[test]
    fn a_failed_text_turn_keeps_what_was_typed() {
        for failure in [
            Failure::Offline,
            Failure::Timeout,
            server(502),
            server(429),
            server(500),
            Failure::Malformed("line"),
        ] {
            let mut call = text_call();
            call.input("I am sorry about that.".to_owned());
            call.send();
            assert_eq!(call.replied(1, Err(failure.clone())), []);
            assert_eq!(call.phase, Phase::Idle, "{failure:?}");
            assert_eq!(call.draft, "I am sorry about that.", "{failure:?}");
            let notice = call.notice.clone().expect("a message");
            assert!(notice.retry, "{failure:?}");
            assert_eq!(notice.text, failure.sentence());
            assert_eq!((call.turns, call.said.as_deref()), (0, None));
            // The draft may be changed before trying again.
            call.input("I am very sorry about that.".to_owned());
            assert_eq!(
                call.retry(),
                [Command::PostText {
                    seq: 2,
                    text: "I am very sorry about that.".to_owned()
                }]
            );
            assert_eq!(call.notice, None);
        }

        // A 400 is not worth sending again as it is: the text stays to be
        // changed, with the server's words.
        let mut call = text_call();
        call.input("x".to_owned());
        call.send();
        assert_eq!(call.replied(1, Err(server(400))), []);
        assert_eq!(
            call.notice,
            Some(Notice {
                text: "The server said so.".to_owned(),
                retry: false
            })
        );
        assert_eq!(call.draft, "x");
        assert!(call.can_send());
    }

    #[test]
    fn a_call_that_ended_on_the_server_goes_to_its_report() {
        let mut call = text_call();
        call.input("Hello?".to_owned());
        call.send();
        assert_eq!(call.replied(1, Err(server(409))), [Command::End { seq: 2 }]);
        assert!(call.is_over());
        assert_eq!(call.ended(2, Ok(report(true))).len(), 1);

        // A call the server has lost is left.
        let mut call = text_call();
        call.input("Hello?".to_owned());
        call.send();
        assert!(matches!(
            call.replied(1, Err(server(404)))[..],
            [Command::Lost(_)]
        ));
        let mut call = text_call();
        call.end();
        assert!(matches!(
            call.ended(1, Err(server(404)))[..],
            [Command::Lost(_)]
        ));
    }

    #[test]
    fn a_report_that_fails_can_be_asked_for_again() {
        let mut call = text_call();
        assert_eq!(call.end(), [Command::End { seq: 1 }]);
        assert_eq!(call.ended(1, Err(server(502))), []);
        assert_eq!(call.phase, Phase::EndFailed);
        let notice = call.notice.clone().expect("a message");
        assert!(notice.retry && notice.text.starts_with("Your report is not ready."));
        assert!(!call.can_type(), "the call is over all the same");
        assert_eq!(call.retry(), [Command::End { seq: 2 }]);
        assert_eq!(call.ended(1, Ok(report(true))), [], "the first request");
        // The red button asks again too.
        assert_eq!(call.ended(2, Err(Failure::Offline)), []);
        assert_eq!(call.end(), [Command::End { seq: 3 }]);
        // Ended before a word was said: the report has no score.
        assert_eq!(
            call.ended(3, Ok(report(false))),
            [Command::Report(report(false))]
        );
    }

    #[test]
    fn a_spoken_turn_from_the_tap_to_the_callers_voice() {
        let mut call = voice_call();
        assert!(call.speaks() && !call.can_type());
        assert_eq!(call.send(), [], "a spoken call is not typed");

        assert_eq!(call.mic(), [Command::StartRecording]);
        assert_eq!(call.phase, Phase::Starting);
        assert_eq!(call.mic(), [], "still starting");
        assert_eq!(call.recording_started(Ok(())), []);
        assert_eq!(call.phase, Phase::Recording(0));
        for _ in 0..7 {
            assert_eq!(call.tick(), []);
        }
        assert_eq!(call.phase, Phase::Recording(7));

        assert_eq!(call.mic(), [Command::StopRecording { seq: 1 }]);
        assert_eq!(call.phase, Phase::Stopping);
        assert_eq!(
            call.recorded(1, Ok(recording(7000))),
            [Command::PostAudio {
                seq: 2,
                recording: recording(7000)
            }]
        );
        assert_eq!(call.phase, Phase::Sending { audio: true });
        assert!(!call.mic_ready());

        assert_eq!(
            call.replied(2, Ok(turn(1, false, true))),
            [Command::FetchAudio {
                line: 2,
                path: AUDIO.to_owned()
            }]
        );
        assert_eq!(call.said.as_deref(), Some("heard 1"));
        assert_eq!(call.kept(), None);
        // An answer for the first line's audio is not this line's.
        assert_eq!(
            call.audio_fetched(1, Ok((vec![7], "audio/mpeg".into()))),
            []
        );
        assert_eq!(call.clip(), None);
        assert_eq!(
            call.audio_fetched(2, Ok((vec![4, 5], "audio/mpeg".into()))),
            [Command::Play {
                line: 2,
                clip: AudioClip::new(vec![4, 5], "audio/mpeg")
            }]
        );
        assert!(!call.mic_ready(), "disabled while the caller speaks");
        assert_eq!(call.mic(), []);
        assert_eq!(call.played(1, Ok(())), [], "an older line's ending");
        assert_eq!(call.sound, Sound::Speaking);
        assert_eq!(call.played(2, Ok(())), []);
        assert!(call.mic_ready());

        // Replay plays the line again without fetching it again.
        assert_eq!(
            call.replay(),
            [Command::Play {
                line: 2,
                clip: AudioClip::new(vec![4, 5], "audio/mpeg")
            }]
        );
        assert_eq!(call.sound, Sound::Speaking);
        assert_eq!(call.played(2, Err(AudioError::Interrupted)), []);
        assert_eq!(call.notice, None, "stopped on purpose is not news");
    }

    #[test]
    fn the_microphone_can_be_refused_busy_or_absent() {
        let mut call = voice_call();
        call.mic();
        assert_eq!(
            call.recording_started(Err(AudioError::PermissionDenied)),
            []
        );
        assert!(call.typing && call.mic_denied && !call.speaks());
        assert!(call.can_type(), "typing takes over for this call");
        assert!(
            call.notice
                .as_ref()
                .is_some_and(|n| !n.retry && n.text.contains("not allowed"))
        );
        call.speak_instead();
        assert!(
            call.typing,
            "there is no going back to a refused microphone"
        );
        // A typed turn in a spoken call still gets the caller's voice.
        call.input("Sorry about that.".to_owned());
        assert_eq!(call.send().len(), 1);
        assert_eq!(call.replied(1, Ok(turn(1, false, true))).len(), 1);

        let mut call = voice_call();
        call.mic();
        assert_eq!(call.recording_started(Err(AudioError::Busy)), []);
        assert!(call.speaks() && call.mic_ready(), "it can be tried again");
        assert!(
            call.notice
                .as_ref()
                .is_some_and(|n| n.text.contains("in use"))
        );

        let mut call = voice_call();
        call.mic();
        call.recording_started(Err(AudioError::Unsupported));
        assert!(call.typing && call.mic_denied);

        let mut call = voice_call();
        call.mic();
        call.recording_started(Err(AudioError::Failed("no input device".into())));
        assert!(call.speaks() && call.notice.is_some());

        // "Type instead" and back, by choice.
        let mut call = voice_call();
        call.type_instead();
        assert!(call.typing && call.can_type() && !call.mic_ready());
        call.speak_instead();
        assert!(call.speaks() && call.mic_ready());
        // A text call has no microphone to go to.
        let mut call = text_call();
        call.speak_instead();
        call.type_instead();
        assert!(!call.typing && !call.speaks());
        assert_eq!(call.mic(), []);
    }

    #[test]
    fn recordings_that_hold_nothing_are_not_sent() {
        let mut call = voice_call();
        call.mic();
        call.recording_started(Ok(()));
        call.mic();
        assert_eq!(call.recorded(1, Ok(recording(120))), []);
        assert!(
            call.notice
                .as_ref()
                .is_some_and(|n| n.text.contains("too short"))
        );
        assert!(call.mic_ready());

        for error in [
            AudioError::Interrupted,
            AudioError::NotRecording,
            AudioError::Failed("codec".into()),
        ] {
            let mut call = voice_call();
            call.mic();
            call.recording_started(Ok(()));
            call.mic();
            assert_eq!(call.recorded(1, Err(error.clone())), [], "{error:?}");
            assert_eq!(call.phase, Phase::Idle);
            assert!(call.notice.as_ref().is_some_and(|n| !n.retry), "{error:?}");
            assert!(call.mic_ready());
        }
    }

    #[test]
    fn a_recording_stops_itself_at_sixty_seconds() {
        let mut call = voice_call();
        call.mic();
        call.recording_started(Ok(()));
        for _ in 0..RECORDING_LIMIT - 1 {
            assert_eq!(call.tick(), []);
        }
        assert_eq!(call.tick(), [Command::StopRecording { seq: 1 }]);
        assert_eq!(call.phase, Phase::Stopping);
        assert!(
            call.notice
                .as_ref()
                .is_some_and(|n| n.text.contains("60 second"))
        );
        assert_eq!(call.tick(), [], "asked once");
        assert_eq!(call.recorded(1, Ok(recording(60_000))).len(), 1);
    }

    #[test]
    fn a_failed_spoken_turn_keeps_the_recording() {
        let mut call = voice_call();
        call.mic();
        call.recording_started(Ok(()));
        call.mic();
        call.recorded(1, Ok(recording(3000)));
        assert_eq!(call.replied(2, Err(server(502))), []);
        assert_eq!(call.kept(), Some(&recording(3000)));
        assert!(call.notice.as_ref().is_some_and(|n| n.retry));
        assert_eq!(
            call.retry(),
            [Command::PostAudio {
                seq: 3,
                recording: recording(3000)
            }]
        );
        // Offline this time; then the learner records afresh instead.
        assert_eq!(call.replied(3, Err(Failure::Offline)), []);
        assert_eq!(call.kept(), Some(&recording(3000)));
        assert_eq!(call.mic(), [Command::StartRecording]);
        assert_eq!(call.kept(), None);
        assert_eq!(call.notice, None);
    }

    #[test]
    fn no_speech_too_large_and_no_voice_are_told_gently() {
        let spoken = |status: u16| {
            let mut call = voice_call();
            call.mic();
            call.recording_started(Ok(()));
            call.mic();
            call.recorded(1, Ok(recording(3000)));
            assert_eq!(call.replied(2, Err(server(status))), []);
            call
        };
        let call = spoken(400);
        assert_eq!(
            call.notice,
            Some(Notice {
                text: "I could not hear any words in that. Tap the microphone and try again."
                    .to_owned(),
                retry: false
            })
        );
        assert_eq!(call.kept(), None);
        assert!(call.mic_ready());

        let call = spoken(413);
        assert!(
            call.notice
                .as_ref()
                .is_some_and(|n| n.text.contains("too large"))
        );
        assert!(call.mic_ready());

        // The voice key went away mid-call: typing takes over, by choice
        // of the server, and the microphone can be tried again later.
        let mut call = spoken(503);
        assert!(call.typing && !call.mic_denied && call.can_type());
        call.speak_instead();
        assert!(call.speaks());
    }

    #[test]
    fn sound_off_blocks_playback_and_lines_still_show() {
        // Switched off while the caller speaks: playback stops.
        let (mut call, _) = OnlineCall::connect(start(true), Scenario::AngryCustomer, false);
        call.audio_fetched(1, Ok((vec![1], "audio/mpeg".into())));
        assert_eq!(call.toggle_mute(), [Command::StopPlayback]);
        assert!(call.muted);
        assert_eq!(call.sound, Sound::Silent);
        assert_eq!(call.played(1, Err(AudioError::Interrupted)), []);
        assert!(call.mic_ready(), "and the learner may answer");

        // Off: the next line's audio is not even fetched, and replay
        // lights the line up instead.
        call.type_instead();
        call.input("Hello".to_owned());
        call.send();
        assert_eq!(call.replied(1, Ok(turn(1, false, true))), []);
        assert_eq!((call.line.as_str(), call.sound), ("line 1", Sound::Silent));
        assert_eq!(call.replay(), []);
        assert_eq!(call.highlight, 2);
        call.tick();
        call.tick();
        assert_eq!(call.highlight, 0);

        // On again: replay fetches the line's audio and plays it.
        assert_eq!(call.toggle_mute(), []);
        assert_eq!(
            call.replay(),
            [Command::FetchAudio {
                line: 2,
                path: AUDIO.to_owned()
            }]
        );
        assert_eq!(
            call.audio_fetched(2, Ok((vec![2], "audio/mpeg".into())))
                .len(),
            1
        );

        // Switched off while the audio is on its way: it is kept, not
        // played.
        let (mut call, _) = OnlineCall::connect(start(true), Scenario::AngryCustomer, false);
        assert_eq!(call.toggle_mute(), []);
        assert_eq!(
            call.audio_fetched(1, Ok((vec![1], "audio/mpeg".into()))),
            []
        );
        assert_eq!(call.sound, Sound::Silent);
        assert!(call.clip().is_some());

        // A call that connects with the sound off fetches nothing.
        let (call, commands) = OnlineCall::connect(start(true), Scenario::AngryCustomer, true);
        assert_eq!(commands, []);
        assert!(call.mic_ready());

        // In a text call replay has only the line to light up.
        let mut call = text_call();
        assert_eq!(call.replay(), []);
        assert_eq!(call.highlight, 2);
    }

    #[test]
    fn audio_that_fails_leaves_the_line_and_the_call() {
        let (mut call, _) = OnlineCall::connect(start(true), Scenario::AngryCustomer, false);
        assert_eq!(call.audio_fetched(1, Err(server(404))), []);
        assert_eq!(call.sound, Sound::Silent);
        assert_eq!(call.notice, None);
        assert!(call.mic_ready());

        let mut call = voice_call();
        call.replay();
        assert_eq!(
            call.played(1, Err(AudioError::Failed("decoder".into()))),
            []
        );
        assert!(
            call.notice
                .as_ref()
                .is_some_and(|n| n.text.contains("could not be played"))
        );
        assert!(call.mic_ready());

        // Audio that arrives while the learner records is kept for later.
        let (mut call, _) = OnlineCall::connect(start(true), Scenario::AngryCustomer, false);
        assert_eq!(call.mic(), [Command::StartRecording]);
        call.recording_started(Ok(()));
        assert_eq!(
            call.audio_fetched(1, Ok((vec![1], "audio/mpeg".into()))),
            []
        );
        assert_eq!(call.sound, Sound::Silent);
        assert!(call.clip().is_some());
    }

    #[test]
    fn the_report_waits_for_the_closing_line_to_be_said() {
        let mut call = voice_call();
        call.type_instead();
        call.input("Goodbye".to_owned());
        call.send();
        assert_eq!(
            call.replied(1, Ok(turn(1, true, true))),
            [
                Command::FetchAudio {
                    line: 2,
                    path: AUDIO.to_owned()
                },
                Command::End { seq: 2 }
            ]
        );
        // The report is quicker than the voice.
        assert_eq!(call.ended(2, Ok(report(true))), []);
        assert_eq!(
            call.audio_fetched(2, Ok((vec![1], "audio/mpeg".into())))
                .len(),
            1
        );
        assert_eq!(
            call.played(2, Ok(())),
            [Command::Report(report(true))],
            "the line is over: the report shows"
        );

        // If the audio cannot be had, the report does not wait for it.
        let mut call = voice_call();
        call.type_instead();
        call.input("Goodbye".to_owned());
        call.send();
        call.replied(1, Ok(turn(1, true, true)));
        call.ended(2, Ok(report(true)));
        assert_eq!(
            call.audio_fetched(2, Err(Failure::Offline)),
            [Command::Report(report(true))]
        );

        // Nor when the learner switches the sound off.
        let mut call = voice_call();
        call.type_instead();
        call.input("Goodbye".to_owned());
        call.send();
        call.replied(1, Ok(turn(1, true, true)));
        call.audio_fetched(2, Ok((vec![1], "audio/mpeg".into())));
        call.ended(2, Ok(report(true)));
        assert_eq!(
            call.toggle_mute(),
            [Command::StopPlayback, Command::Report(report(true))]
        );
    }

    #[test]
    fn ending_and_leaving_stop_the_microphone_and_the_voice() {
        // The red button while recording.
        let mut call = voice_call();
        call.mic();
        call.recording_started(Ok(()));
        assert_eq!(
            call.end(),
            [Command::StopRecording { seq: 0 }, Command::End { seq: 1 }]
        );
        assert_eq!(call.recorded(0, Ok(recording(3000))), [], "not wanted");

        // The red button while the caller speaks.
        let (mut call, _) = OnlineCall::connect(start(true), Scenario::AngryCustomer, false);
        call.audio_fetched(1, Ok((vec![1], "audio/mpeg".into())));
        assert_eq!(call.end(), [Command::StopPlayback, Command::End { seq: 1 }]);
        assert_eq!(call.ended(1, Ok(report(false))).len(), 1, "no waiting");

        // Ended while the microphone was still being asked for: when it
        // starts after all, it is stopped.
        let mut call = voice_call();
        call.mic();
        call.end();
        assert_eq!(
            call.recording_started(Ok(())),
            [Command::StopRecording { seq: 0 }]
        );
        assert_eq!(
            call.recording_started(Err(AudioError::PermissionDenied)),
            []
        );
        assert!(!call.typing);

        // Back, mid-recording and mid-sentence.
        let mut call = voice_call();
        call.replay();
        assert_eq!(call.abandon(), [Command::StopPlayback]);
        let mut call = voice_call();
        call.mic();
        call.recording_started(Ok(()));
        assert_eq!(call.abandon(), [Command::StopRecording { seq: 0 }]);
        assert_eq!(text_call().abandon(), []);
    }
}
