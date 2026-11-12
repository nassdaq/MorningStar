//! Everything the application remembers, in one place: who the learner
//! is, how they like their calls, the call in progress, the calls behind
//! them, their plan and what the bell has to say.
//!
//! Screens read a [`Model`] and never change it; `lib.rs` changes it in
//! `update`. Nothing here draws, so all of it is tested without a screen.
//!
//! The same model serves both ways the app runs. Offline it is what it
//! always was: a sample history and calls from the script. Online
//! ([`Link::Online`]) the name, the settings, the plan, the numbers and the
//! history are the server's, a call is an [`OnlineCall`] and a report
//! carries the server's words ([`ServerReport`]).

use std::borrow::Cow;

use crate::api;
use crate::online::OnlineCall;
use crate::script::{Exchange, Scenario, Skill};

// --------------------------------------------------------------- settings

/// The accent the AI customer speaks with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Accent {
    /// The design's default.
    #[default]
    Us,
    /// British.
    Uk,
    /// Australian.
    Australian,
    /// Indian.
    Indian,
}

impl Accent {
    /// Every accent, in the order the settings show them.
    pub const ALL: [Self; 4] = [Self::Us, Self::Uk, Self::Australian, Self::Indian];

    /// The accent's name as it stands before the word "accent".
    pub const fn label(self) -> &'static str {
        match self {
            Self::Us => "US",
            Self::Uk => "UK",
            Self::Australian => "Australian",
            Self::Indian => "Indian",
        }
    }
}

/// How patient the callers are, which is how much a weaker answer costs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Difficulty {
    /// Patient callers.
    Calm,
    /// Everyday callers.
    #[default]
    Standard,
    /// Short-tempered callers.
    Tough,
}

impl Difficulty {
    /// Every difficulty, easiest first.
    pub const ALL: [Self; 3] = [Self::Calm, Self::Standard, Self::Tough];

    /// The difficulty's name.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Calm => "Calm",
            Self::Standard => "Standard",
            Self::Tough => "Tough",
        }
    }

    /// What the setting does, in a sentence.
    pub const fn description(self) -> &'static str {
        match self {
            Self::Calm => "Patient callers. A weaker answer costs less.",
            Self::Standard => "Everyday callers.",
            Self::Tough => "Short-tempered callers. A weaker answer costs more.",
        }
    }

    /// The share of an exchange's points a weaker answer still earns.
    const fn weak_share(self) -> f32 {
        match self {
            Self::Calm => 0.55,
            Self::Standard => 0.4,
            Self::Tough => 0.25,
        }
    }
}

/// How long a call is meant to take.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Length {
    /// Two minutes.
    Two,
    /// Four minutes: the design's.
    #[default]
    Four,
    /// Six minutes.
    Six,
}

impl Length {
    /// Every length, shortest first.
    pub const ALL: [Self; 3] = [Self::Two, Self::Four, Self::Six];

    /// The length in minutes.
    pub const fn minutes(self) -> u32 {
        match self {
            Self::Two => 2,
            Self::Four => 4,
            Self::Six => 6,
        }
    }
}

/// How the learner likes their calls: the "Scenario settings" screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    /// The caller's accent.
    pub accent: Accent,
    /// The caller's patience.
    pub difficulty: Difficulty,
    /// The call's target length.
    pub length: Length,
}

impl Settings {
    /// The line under today's call: "Customer · US accent · 4 min".
    pub fn summary(&self) -> String {
        format!(
            "Customer \u{b7} {} accent \u{b7} {} min",
            self.accent.label(),
            self.length.minutes()
        )
    }
}

/// The two plans of the offer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Plan {
    /// $4.99 per month: the one the design shows selected.
    #[default]
    Premium,
    /// $0: three intro calls.
    Free,
}

// ------------------------------------------------------------------ calls

/// A call in progress.
#[derive(Clone, Debug, PartialEq)]
pub struct Call {
    /// What is being practised.
    pub scenario: Scenario,
    /// The call's number in the learner's history, from 1.
    pub number: usize,
    /// The caller's accent, from the settings when the call began.
    pub accent: Accent,
    /// The difficulty, from the settings when the call began.
    pub difficulty: Difficulty,
    /// The target length in seconds, from the settings when the call began.
    pub limit: u32,
    /// The chip chosen in each exchange answered so far.
    pub choices: Vec<usize>,
    /// Seconds on the clock.
    pub elapsed: u32,
    /// Whether the learner's microphone is muted. A muted learner cannot
    /// answer.
    pub muted: bool,
    /// How often the customer was asked to repeat a line.
    pub replays: u32,
    /// Seconds for which the customer's line is still shown as repeated.
    pub repeating: u8,
    /// True for the moment the design shows, until the learner answers: the
    /// clock stands at 02:14 and the progress bars where the design drew
    /// them.
    pub designed: bool,
}

impl Call {
    /// The moment the design shows: call 1, two minutes fourteen in, the
    /// customer's second line just said. It stands still until answered.
    pub fn designed() -> Self {
        Self {
            scenario: Scenario::MissingDelivery,
            number: 1,
            accent: Accent::Us,
            difficulty: Difficulty::Standard,
            limit: Length::Four.minutes() * 60,
            choices: vec![0],
            elapsed: 134,
            muted: false,
            replays: 0,
            repeating: 0,
            designed: true,
        }
    }

    /// A call that has just connected.
    pub fn start(scenario: Scenario, number: usize, settings: &Settings) -> Self {
        Self {
            scenario,
            number,
            accent: settings.accent,
            difficulty: settings.difficulty,
            limit: settings.length.minutes() * 60,
            choices: Vec::new(),
            elapsed: 0,
            muted: false,
            replays: 0,
            repeating: 0,
            designed: false,
        }
    }

    /// The exchange waiting for an answer; `None` once the script is done.
    pub fn exchange(&self) -> Option<&'static Exchange> {
        self.scenario.script().get(self.choices.len())
    }

    /// True once every exchange has been answered.
    pub fn is_finished(&self) -> bool {
        self.exchange().is_none()
    }

    /// True while the clock runs: a real call that has not finished.
    pub fn is_running(&self) -> bool {
        !self.designed && !self.is_finished()
    }

    /// Answers the waiting exchange with chip `option`. Returns false, and
    /// changes nothing, if there is no such chip or the learner is muted.
    pub fn choose(&mut self, option: usize) -> bool {
        let Some(exchange) = self.exchange() else {
            return false;
        };
        if self.muted || option >= exchange.options.len() {
            return false;
        }
        self.choices.push(option);
        self.repeating = 0;
        self.designed = false;
        true
    }

    /// One second passes.
    pub fn tick(&mut self) {
        if self.is_running() {
            self.elapsed = self.elapsed.saturating_add(1);
            self.repeating = self.repeating.saturating_sub(1);
        }
    }

    /// The customer repeats the line: it is shown as repeated for two
    /// seconds, or until it is answered.
    pub fn replay(&mut self) {
        if !self.is_finished() {
            self.replays += 1;
            self.repeating = 2;
        }
    }

    /// The clock: "02:14".
    pub fn clock(&self) -> String {
        format!("{:02}:{:02}", self.elapsed / 60, self.elapsed % 60)
    }

    /// The share of the script answered, and the share of the target
    /// length used, each from 0 to 1: the bar in the header and the thin
    /// bar over the call bar. The designed moment has the design's values.
    pub fn progress(&self) -> (f32, f32) {
        if self.designed {
            return (0.32, 0.22);
        }
        let script = self.choices.len() as f32 / self.scenario.script().len() as f32;
        let time = (self.elapsed as f32 / self.limit.max(1) as f32).min(1.0);
        (script, time)
    }

    /// The learner's last answer in full, and whether it was the good one.
    pub fn last_reply(&self) -> Option<(String, bool)> {
        let index = self.choices.len().checked_sub(1)?;
        let exchange = self.scenario.script().get(index)?;
        let option = *self.choices.get(index)?;
        Some((exchange.spoken(option), exchange.options[option].good))
    }
}

/// The five skill scores of a call, each out of 100, in [`Skill::ALL`]'s
/// order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scores(pub [u8; 5]);

impl Scores {
    /// The score for one skill.
    pub fn get(&self, skill: Skill) -> u8 {
        let index = Skill::ALL
            .iter()
            .position(|candidate| *candidate == skill)
            .unwrap_or(0);
        self.0[index]
    }

    /// The call's score: the mean of the five, rounded.
    pub fn overall(&self) -> u8 {
        let sum: u32 = self.0.iter().map(|score| u32::from(*score)).sum();
        ((sum as f32 / 5.0).round()) as u8
    }

    /// Scores a call from the chips chosen and the time taken.
    ///
    /// Each skill starts at 40. Every exchange is worth points to every
    /// skill, twice as many to the skill it practises; a good answer earns
    /// them all and a weaker one a share set by the difficulty. All good
    /// answers earn 52. Finishing the script adds 6 within the target
    /// length and 3 after it. So a call scores from 40 to 98, the same
    /// answers always give the same score, and an unfinished call cannot
    /// reach the top.
    pub fn of(
        scenario: Scenario,
        choices: &[usize],
        seconds: u32,
        limit: u32,
        difficulty: Difficulty,
    ) -> Self {
        let script = scenario.script();
        let finished = choices.len() >= script.len();
        let pace = match (finished, seconds <= limit) {
            (false, _) => 0.0,
            (true, true) => 6.0,
            (true, false) => 3.0,
        };
        Self(Skill::ALL.map(|skill| {
            let mut earned = 0.0;
            let mut possible = 0.0;
            for (index, exchange) in script.iter().enumerate() {
                let weight = if exchange.skill == skill { 2.0 } else { 1.0 };
                possible += weight;
                if let Some(option) = choices.get(index).and_then(|c| exchange.options.get(*c)) {
                    earned += weight
                        * if option.good {
                            1.0
                        } else {
                            difficulty.weak_share()
                        };
                }
            }
            (40.0 + 52.0 * earned / possible + pace).round() as u8
        }))
    }
}

/// When a past call was made. The app has no calendar: a call made in this
/// session is "Today" and the sample history lies behind it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When {
    /// In this session.
    Today,
    /// The day before.
    Yesterday,
    /// Further back.
    DaysAgo(u8),
}

impl When {
    /// "Today", "Yesterday", "3 days ago".
    pub fn label(self) -> String {
        match self {
            Self::Today => "Today".to_owned(),
            Self::Yesterday => "Yesterday".to_owned(),
            Self::DaysAgo(days) => format!("{days} days ago"),
        }
    }
}

/// A finished call: what the call report and the transcript show, and a
/// row of the history.
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    /// What was practised.
    pub scenario: Scenario,
    /// When.
    pub when: When,
    /// How long the call lasted, in seconds.
    pub seconds: u32,
    /// The chip chosen in each exchange that was answered.
    pub choices: Vec<usize>,
    /// The five scores.
    pub scores: Scores,
    /// True for the report the design shows, whose scores and wording are
    /// the design's rather than derived.
    pub designed: bool,
    /// The server's overall score, which is its own judgement and not the
    /// mean of the five skills. `None` for a scripted call.
    pub overall: Option<u8>,
    /// What the server said about the call, for a call played online. Such
    /// a report has no `choices`: its words are here.
    pub server: Option<ServerReport>,
}

/// How much of a server's report the app holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Detail {
    /// A row of the history: the score, and no more yet.
    Summary,
    /// The whole report has been asked for.
    Loading,
    /// It could not be fetched, for this reason.
    Failed(String),
    /// The whole report.
    Loaded,
}

/// The server's account of a call played online.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerReport {
    /// The call's id on the server.
    pub call_id: String,
    /// Its number in the learner's history.
    pub number: u32,
    /// The scenario's name, as the server gave it.
    pub title: String,
    /// The caller's first name; empty in a history row.
    pub caller: String,
    /// The day it was made: "12 Nov".
    pub day: String,
    /// False for a call ended before the learner said anything: such a
    /// call has no score.
    pub scored: bool,
    /// What went well, one sentence.
    pub did_well: String,
    /// What to try next, one sentence.
    pub try_next: String,
    /// The call, line by line.
    pub transcript: Vec<api::Spoken>,
    /// How much of this has been fetched.
    pub detail: Detail,
}

impl Report {
    /// The report the design shows: Missing delivery, four minutes, 82.
    pub fn designed() -> Self {
        Self {
            scenario: Scenario::MissingDelivery,
            when: When::Yesterday,
            seconds: 240,
            // Three good answers and "explaining" before asking for the
            // order number, as the design's feedback says.
            choices: vec![0, 0, 0, 0],
            scores: Scores([88, 74, 85, 79, 84]),
            designed: true,
            overall: None,
            server: None,
        }
    }

    /// The report of a call that has just ended.
    pub fn of(call: &Call) -> Self {
        Self {
            scenario: call.scenario,
            when: When::Today,
            seconds: call.elapsed,
            choices: call.choices.clone(),
            scores: Scores::of(
                call.scenario,
                &call.choices,
                call.elapsed,
                call.limit,
                call.difficulty,
            ),
            designed: false,
            overall: None,
            server: None,
        }
    }

    /// A call of the sample history: `good[i]` says whether exchange `i`
    /// was answered well.
    fn sample(scenario: Scenario, when: When, seconds: u32, good: &[bool]) -> Self {
        let choices: Vec<usize> = scenario
            .script()
            .iter()
            .zip(good)
            .map(|(exchange, good)| {
                if *good {
                    exchange.good_option()
                } else {
                    1 - exchange.good_option()
                }
            })
            .collect();
        Self {
            scenario,
            when,
            seconds,
            scores: Scores::of(scenario, &choices, seconds, 240, Difficulty::Standard),
            choices,
            designed: false,
            overall: None,
            server: None,
        }
    }

    /// The report of a call the server has ended.
    pub fn from_server(report: &api::CallReport) -> Self {
        Self {
            scenario: report.scenario.unwrap_or_default(),
            when: When::Today,
            seconds: report.duration_seconds,
            choices: Vec::new(),
            scores: Scores(report.scores.map_or([0; 5], |(_, skills)| skills)),
            designed: false,
            overall: None,
            server: Some(ServerReport {
                call_id: report.call_id.clone(),
                number: report.number,
                title: report.title.clone(),
                caller: report.caller_name.clone(),
                day: api::day_label(&report.started_at),
                scored: report.scores.is_some(),
                did_well: report.did_well.clone(),
                try_next: report.try_next.clone(),
                transcript: report.transcript.clone(),
                detail: Detail::Loaded,
            }),
        }
        .with_overall(report.scores.map(|(overall, _)| overall))
    }

    /// A row of the server's history: the score is known, the rest is
    /// fetched when the report is opened.
    pub fn from_summary(summary: &api::CallSummary) -> Self {
        Self {
            scenario: summary.scenario.unwrap_or_default(),
            when: When::Today,
            seconds: summary.duration_seconds,
            choices: Vec::new(),
            scores: Scores([summary.score; 5]),
            designed: false,
            overall: None,
            server: Some(ServerReport {
                call_id: summary.call_id.clone(),
                number: summary.number,
                title: summary.title.clone(),
                caller: String::new(),
                day: api::day_label(&summary.started_at),
                scored: true,
                did_well: String::new(),
                try_next: String::new(),
                transcript: Vec::new(),
                detail: Detail::Summary,
            }),
        }
        .with_overall(Some(summary.score))
    }

    fn with_overall(mut self, overall: Option<u8>) -> Self {
        self.overall = overall;
        self
    }

    /// The call's id on the server, for a call played online.
    pub fn call_id(&self) -> Option<&str> {
        self.server.as_ref().map(|server| server.call_id.as_str())
    }

    /// The call's score out of 100; 0 for a call that was not scored.
    pub fn overall(&self) -> u8 {
        match (&self.server, self.overall) {
            (Some(server), _) if !server.scored => 0,
            (_, Some(overall)) => overall,
            _ => self.scores.overall(),
        }
    }

    /// The call's score, or `None` for a call the server did not score
    /// (it ended before the learner said anything).
    pub fn score(&self) -> Option<u8> {
        match &self.server {
            Some(server) if !server.scored => None,
            _ => Some(self.overall()),
        }
    }

    /// True when the five skill scores are known: always for a scripted
    /// call, and for a server's once its whole report has arrived.
    pub fn has_skills(&self) -> bool {
        self.server
            .as_ref()
            .is_none_or(|server| server.scored && server.detail == Detail::Loaded)
    }

    /// The scenario's name: the server's for a call played online.
    pub fn title(&self) -> String {
        match &self.server {
            Some(server) => server.title.clone(),
            None => self.scenario.title().to_owned(),
        }
    }

    /// The caller's first name.
    pub fn caller(&self) -> String {
        match &self.server {
            Some(server) if !server.caller.is_empty() => server.caller.clone(),
            _ => self.scenario.caller().to_owned(),
        }
    }

    /// The day of the call: "Today", "3 days ago", or the server's date.
    pub fn day(&self) -> String {
        match &self.server {
            Some(server) if !server.day.is_empty() => server.day.clone(),
            Some(_) => "Earlier".to_owned(),
            None => self.when.label(),
        }
    }

    /// True if the whole script was answered.
    pub fn is_complete(&self) -> bool {
        self.choices.len() >= self.scenario.script().len()
    }

    /// The exchanges that were answered, each with the chip chosen.
    pub fn answered(&self) -> impl Iterator<Item = (&'static Exchange, usize)> + '_ {
        self.scenario
            .script()
            .iter()
            .zip(self.choices.iter().copied())
            .filter(|(exchange, option)| *option < exchange.options.len())
    }

    /// "4 min", "45 sec".
    pub fn duration(&self) -> String {
        if self.seconds < 60 {
            format!("{} sec", self.seconds)
        } else {
            format!("{} min", self.seconds / 60)
        }
    }

    /// "Missing delivery · 4 min".
    pub fn caption(&self) -> String {
        format!("{} \u{b7} {}", self.title(), self.duration())
    }

    /// The handwritten note beside the score.
    pub fn note(&self) -> &'static str {
        match self.overall() {
            80.. => "nice work!",
            60..=79 => "getting there",
            _ => "keep going!",
        }
    }

    /// What went well: the server's sentence for a call played online,
    /// otherwise the praise of the first good answer.
    pub fn did_well(&self) -> Cow<'static, str> {
        if let Some(server) = &self.server {
            return Cow::Owned(server.did_well.clone());
        }
        Cow::Borrowed(
            self.answered()
                .find(|(exchange, option)| exchange.options[*option].good)
                .map_or(
                    if self.choices.is_empty() {
                        "You picked up the call. That is the first step."
                    } else {
                        "You stayed on the line and kept the call going."
                    },
                    |(exchange, _)| exchange.praise,
                ),
        )
    }

    /// What to try: the server's sentence for a call played online,
    /// otherwise the advice of the first weaker answer.
    pub fn try_next(&self) -> Cow<'static, str> {
        if let Some(server) = &self.server {
            return Cow::Owned(server.try_next.clone());
        }
        Cow::Borrowed(
            self.answered()
                .find(|(exchange, option)| !exchange.options[*option].good)
                .map_or(
                    if self.is_complete() {
                        "Try a tougher caller in Scenario settings."
                    } else {
                        "Stay on the call and answer every line."
                    },
                    |(exchange, _)| exchange.advice,
                ),
        )
    }
}

// ---------------------------------------------------------- notifications

/// Where a notification leads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// The report of the latest call.
    LatestReport,
    /// The progress tab.
    Progress,
    /// The subscription offer.
    Premium,
}

/// One line of the notifications screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notification {
    /// The headline.
    pub title: &'static str,
    /// The detail under it.
    pub body: String,
    /// When it arrived.
    pub when: &'static str,
    /// Where tapping it leads.
    pub target: Target,
    /// Whether it has been opened.
    pub read: bool,
}

// ------------------------------------------------------------------ model

/// The longest name the profile accepts, in characters.
pub const NAME_LIMIT: usize = 24;

/// Whether the app has a server behind it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Link {
    /// Nothing has been tried yet.
    #[default]
    Unknown,
    /// The server is being asked.
    Checking,
    /// This device cannot make requests at all (a screenshot, a test with
    /// no host): the app is the scripted app and says nothing about it.
    Standalone,
    /// The server could not be used, for this reason in plain words. Calls
    /// are the practice script until it can.
    Offline(String),
    /// The server answered: calls are conversations with the AI caller,
    /// spoken when it has a voice.
    Online {
        /// The server has a voice key.
        voice: bool,
    },
}

impl Link {
    /// True while calls are played by the server.
    pub fn is_online(&self) -> bool {
        matches!(self, Self::Online { .. })
    }

    /// Where things stand, for the Server screen and the menu:
    /// "connected · voice", "unreachable · the reason".
    pub fn status(&self) -> String {
        match self {
            Self::Unknown => "not checked yet".to_owned(),
            Self::Checking => "checking\u{2026}".to_owned(),
            Self::Standalone => {
                "unreachable \u{b7} This device cannot make network requests.".to_owned()
            }
            Self::Offline(reason) => format!("unreachable \u{b7} {reason}"),
            Self::Online { voice: true } => "connected \u{b7} voice".to_owned(),
            Self::Online { voice: false } => "connected \u{b7} text mode".to_owned(),
        }
    }

    /// The small note on home: how calls are played right now. Nothing
    /// while that is not known, or cannot be otherwise.
    pub fn caption(&self) -> Option<&'static str> {
        match self {
            Self::Online { voice: true } => Some("live \u{b7} voice"),
            Self::Online { voice: false } => Some("live \u{b7} text"),
            Self::Offline(_) => Some("offline \u{b7} practice script"),
            Self::Unknown | Self::Checking | Self::Standalone => None,
        }
    }
}

/// The learner's numbers as the server counts them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stats {
    /// Scored calls.
    pub calls: u32,
    /// Their mean score; `None` with no call.
    pub average: Option<u8>,
    /// Days in a row with a call.
    pub streak: u32,
}

/// The call on the line when the server plays it.
#[derive(Clone, Debug, PartialEq)]
pub enum Live {
    /// `POST /v1/calls` is out: the phone is ringing.
    Dialing(Scenario),
    /// The conversation.
    Active(Box<OnlineCall>),
}

/// A line shown for a few seconds over whatever screen is up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    /// What it says.
    pub text: String,
    /// Seconds it still shows for.
    pub seconds: u8,
}

/// Everything the application remembers.
#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    /// The learner's name, as typed on the profile.
    pub name: String,
    /// How they like their calls.
    pub settings: Settings,
    /// Whether Premium was started. There is no payment in this app: it is
    /// a demo state and the screens say so.
    pub premium: bool,
    /// Finished calls, newest first.
    pub history: Vec<Report>,
    /// Whether the bell shows its dot: something arrived since the
    /// notifications were last opened.
    pub bell: bool,
    /// Which of the three notifications have been opened.
    pub read: [bool; 3],
    /// The call in progress (or the designed moment, before any call).
    pub call: Call,
    /// The report being looked at.
    pub report: Report,
    /// Whether a server is behind the app.
    pub link: Link,
    /// The server's base URL in use.
    pub server_url: String,
    /// The address as typed on the Server screen.
    pub server_draft: String,
    /// What is wrong with the typed address, if it cannot be used.
    pub server_problem: Option<&'static str>,
    /// The learner's numbers as the server counts them, once it has said.
    pub stats: Option<Stats>,
    /// The call the server is playing, if one is on the line. While it is
    /// `None` the call on the live-call screen is [`call`](Self::call).
    pub live: Option<Live>,
    /// A line to show for a few seconds: a save that failed, a call that
    /// fell back to the script.
    pub notice: Option<Notice>,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            name: "Grace Mushi".to_owned(),
            settings: Settings::default(),
            premium: false,
            history: vec![
                Report::designed(),
                Report::sample(
                    Scenario::RefundRequest,
                    When::DaysAgo(2),
                    205,
                    &[true, true, false, true],
                ),
                Report::sample(
                    Scenario::AngryCustomer,
                    When::DaysAgo(3),
                    262,
                    &[true, false, true, false],
                ),
            ],
            bell: true,
            read: [false; 3],
            call: Call::designed(),
            report: Report::designed(),
            link: Link::Unknown,
            server_url: api::DEFAULT_BASE_URL.to_owned(),
            server_draft: api::DEFAULT_BASE_URL.to_owned(),
            server_problem: None,
            stats: None,
            live: None,
            notice: None,
        }
    }
}

impl Model {
    /// The first word of the name, for the greeting on home.
    pub fn first_name(&self) -> String {
        self.name
            .split_whitespace()
            .next()
            .unwrap_or("You")
            .to_owned()
    }

    /// The initials in the avatar: of the first two words of the name.
    pub fn initials(&self) -> String {
        let initials: String = self
            .name
            .split_whitespace()
            .take(2)
            .filter_map(|word| word.chars().next())
            .flat_map(char::to_uppercase)
            .collect();
        if initials.is_empty() {
            "?".to_owned()
        } else {
            initials
        }
    }

    /// Stores a name typed on the profile, cut to [`NAME_LIMIT`].
    pub fn set_name(&mut self, name: &str) {
        self.name = name.chars().take(NAME_LIMIT).collect();
    }

    /// The latest finished call.
    pub fn latest(&self) -> Option<&Report> {
        self.history.first()
    }

    /// The mean score of all finished calls, rounded; 0 with none. The
    /// server's figure once it has given one.
    pub fn average(&self) -> u8 {
        match self.stats {
            Some(stats) => stats.average.unwrap_or(0),
            None => self.list_average(),
        }
    }

    /// The mean score of the calls in the history list, rounded.
    pub fn list_average(&self) -> u8 {
        if self.history.is_empty() {
            return 0;
        }
        let sum: u32 = self
            .history
            .iter()
            .map(|report| u32::from(report.overall()))
            .sum();
        (sum as f32 / self.history.len() as f32).round() as u8
    }

    /// How many calls have been finished: the server's count once it has
    /// given one.
    pub fn calls(&self) -> usize {
        match self.stats {
            Some(stats) => stats.calls as usize,
            None => self.history.len(),
        }
    }

    /// The five bars of the progress tab and what they are of. Offline:
    /// each skill averaged over the history. Online the server keeps no
    /// such average, so they are the latest call's, once its report is in.
    pub fn skills(&self) -> (Option<&'static str>, Scores) {
        if self.stats.is_none() {
            return (None, self.average_scores());
        }
        match self.history.iter().find(|report| report.has_skills()) {
            Some(report) => (Some("SKILLS \u{b7} LATEST REPORT"), report.scores),
            None => (Some("SKILLS \u{b7} NO REPORT YET"), Scores([0; 5])),
        }
    }

    /// The mean of each skill over all finished calls.
    pub fn average_scores(&self) -> Scores {
        if self.history.is_empty() {
            return Scores([0; 5]);
        }
        let count = self.history.len() as f32;
        let mut index = 0;
        Scores([(); 5].map(|()| {
            let sum: u32 = self
                .history
                .iter()
                .map(|report| u32::from(report.scores.0[index]))
                .sum();
            index += 1;
            (sum as f32 / count).round() as u8
        }))
    }

    /// Days in a row with a call: the three of the sample history, and
    /// today once a call has been finished in this session.
    pub fn streak(&self) -> usize {
        match self.stats {
            Some(stats) => stats.streak as usize,
            None => 3 + usize::from(self.history.iter().any(|report| report.when == When::Today)),
        }
    }

    /// The number the next call gets.
    pub fn next_call_number(&self) -> usize {
        self.calls() + 1
    }

    /// Takes the server's profile: the name, the settings, the plan and
    /// the numbers.
    pub fn apply_profile(&mut self, profile: &api::Profile) {
        self.name = profile.name.clone();
        self.settings = Settings {
            accent: profile.accent,
            difficulty: profile.difficulty,
            length: profile.length,
        };
        self.premium = profile.premium;
        self.stats = Some(Stats {
            calls: profile.calls,
            average: profile.average_score,
            streak: profile.streak_days,
        });
    }

    /// Takes the server's history, keeping the whole reports already
    /// fetched for calls that are still in it.
    pub fn apply_history(&mut self, calls: &[api::CallSummary]) {
        let known = std::mem::take(&mut self.history);
        self.history = calls
            .iter()
            .map(|summary| {
                known
                    .iter()
                    .find(|report| {
                        report.call_id() == Some(summary.call_id.as_str()) && report.has_skills()
                    })
                    .cloned()
                    .unwrap_or_else(|| Report::from_summary(summary))
            })
            .collect();
    }

    /// Records a report the server has made: it becomes the report being
    /// looked at, and, if the call was scored, the newest of the history
    /// (in place of any row already there for it).
    pub fn take_report(&mut self, report: Report) {
        self.report = report.clone();
        if report.score().is_none() {
            return;
        }
        let id = report.call_id().map(str::to_owned);
        match self
            .history
            .iter()
            .position(|known| id.is_some() && known.call_id() == id.as_deref())
        {
            Some(index) => self.history[index] = report,
            None => self.history.insert(0, report),
        }
    }

    /// Notes how fetching the whole report of the call `call_id` stands,
    /// wherever that call is held.
    pub fn set_detail(&mut self, call_id: &str, detail: &Detail) {
        for report in self.history.iter_mut().chain([&mut self.report]) {
            if let Some(server) = &mut report.server
                && server.call_id == call_id
            {
                server.detail = detail.clone();
            }
        }
    }

    /// Puts the whole report of a call in the place of its history row,
    /// and of the report being looked at if that is the same call.
    pub fn fill_report(&mut self, report: &Report) {
        let id = report.call_id().map(str::to_owned);
        for known in self.history.iter_mut().chain([&mut self.report]) {
            if id.is_some() && known.call_id() == id.as_deref() {
                *known = report.clone();
            }
        }
    }

    /// The call the server is playing, once it has connected.
    pub fn online_call(&self) -> Option<&OnlineCall> {
        match &self.live {
            Some(Live::Active(call)) => Some(call),
            _ => None,
        }
    }

    /// Shows `text` for a few seconds.
    pub fn say(&mut self, text: impl Into<String>) {
        self.notice = Some(Notice {
            text: text.into(),
            seconds: 6,
        });
    }

    /// Records the call that has just ended and makes its report the one
    /// being looked at. The designed moment, ended without an answer, is
    /// the call the designed report already describes: nothing is added.
    pub fn finish_call(&mut self) {
        if self.call.designed {
            self.report = Report::designed();
            return;
        }
        let report = Report::of(&self.call);
        self.history.insert(0, report.clone());
        self.report = report;
        // "Your report is ready" is news again.
        self.bell = true;
        self.read[0] = false;
    }

    /// The three notifications, which describe the current state.
    pub fn notifications(&self) -> [Notification; 3] {
        let latest = self.latest().map_or_else(
            || "Finish a call to get one.".to_owned(),
            |report| format!("{} \u{b7} {}/100", report.title(), report.overall()),
        );
        [
            Notification {
                title: "Your report is ready",
                body: latest,
                when: "Just now",
                target: Target::LatestReport,
                read: self.read[0],
            },
            Notification {
                title: "Keep your streak",
                body: match self.streak() {
                    0 => "Make a call today to start a streak.".to_owned(),
                    1 => "1 day so far. See how you are doing.".to_owned(),
                    days => format!("{days} days in a row. See how you are doing."),
                },
                when: "Today",
                target: Target::Progress,
                read: self.read[1],
            },
            Notification {
                title: "Practise more with Premium",
                body: "100 minutes of AI call practice a month.".to_owned(),
                when: "Yesterday",
                target: Target::Premium,
                read: self.read[2],
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn play(scenario: Scenario, good: &[bool], seconds: u32) -> Call {
        let mut call = Call::start(scenario, 4, &Settings::default());
        for good in good {
            let exchange = call.exchange().expect("an exchange is waiting");
            let option = if *good {
                exchange.good_option()
            } else {
                1 - exchange.good_option()
            };
            assert!(call.choose(option));
        }
        for _ in 0..seconds {
            call.elapsed += 1;
        }
        call
    }

    #[test]
    fn scores_follow_the_answers_and_the_time() {
        let all_good = Report::of(&play(Scenario::RefundRequest, &[true; 4], 200));
        assert_eq!(all_good.scores, Scores([98; 5]));
        assert_eq!(all_good.note(), "nice work!");

        // Over the target length: three points fewer everywhere.
        let late = Report::of(&play(Scenario::RefundRequest, &[true; 4], 300));
        assert_eq!(late.scores, Scores([95; 5]));

        // A weaker answer costs most in the skill its exchange practises.
        let one_weak = Report::of(&play(
            Scenario::RefundRequest,
            &[true, true, false, true],
            200,
        ));
        let clarity = one_weak.scores.get(Skill::Clarity);
        let empathy = one_weak.scores.get(Skill::Empathy);
        assert!(clarity < empathy && empathy < 98, "{:?}", one_weak.scores);
        assert_eq!(
            one_weak.try_next(),
            "Give the number of days, not \u{201c}soon\u{201d}."
        );

        // Ended at once: the floor, and nothing to praise but picking up.
        let nothing = Report::of(&play(Scenario::RefundRequest, &[], 3));
        assert_eq!(nothing.scores, Scores([40; 5]));
        assert_eq!(nothing.note(), "keep going!");
        assert!(!nothing.is_complete());

        // The same answers always give the same scores.
        let again = Report::of(&play(
            Scenario::RefundRequest,
            &[true, true, false, true],
            200,
        ));
        assert_eq!(again.scores, one_weak.scores);
    }

    #[test]
    fn difficulty_sets_what_a_weaker_answer_costs() {
        let score = |difficulty| {
            let mut call = play(Scenario::BillingProblem, &[false; 4], 100);
            call.difficulty = difficulty;
            Report::of(&call).overall()
        };
        assert!(score(Difficulty::Calm) > score(Difficulty::Standard));
        assert!(score(Difficulty::Standard) > score(Difficulty::Tough));
    }

    #[test]
    fn the_designed_call_and_report_are_the_designs() {
        let call = Call::designed();
        assert_eq!(call.clock(), "02:14");
        assert_eq!(call.progress(), (0.32, 0.22));
        assert!(!call.is_running());
        assert_eq!(call.exchange().map(|e| e.lead), Some("Two weeks"));

        let report = Report::designed();
        assert_eq!(report.overall(), 82);
        assert_eq!(report.caption(), "Missing delivery \u{b7} 4 min");
        assert_eq!(report.note(), "nice work!");
        assert_eq!(
            report.try_next(),
            "Ask for the order number before explaining."
        );
    }

    #[test]
    fn a_call_runs_answers_and_stops() {
        let mut call = Call::start(Scenario::AngryCustomer, 4, &Settings::default());
        assert_eq!(
            (call.clock(), call.progress()),
            ("00:00".to_owned(), (0.0, 0.0))
        );
        call.tick();
        call.tick();
        assert_eq!(call.clock(), "00:02");
        call.replay();
        assert_eq!(call.repeating, 2);
        call.tick();
        call.tick();
        assert_eq!(call.repeating, 0);

        call.muted = true;
        assert!(!call.choose(0), "a muted learner cannot answer");
        call.muted = false;
        assert!(!call.choose(2), "there are two chips");
        for _ in 0..4 {
            assert!(call.choose(1));
        }
        assert!(call.is_finished() && !call.is_running());
        let clock = call.clock();
        call.tick();
        assert_eq!(call.clock(), clock, "the clock stops with the call");
        assert!(!call.choose(0));
    }

    #[test]
    fn names_initials_and_numbers() {
        let mut model = Model::default();
        assert_eq!(
            (model.first_name().as_str(), model.initials().as_str()),
            ("Grace", "GM")
        );
        model.set_name("asha");
        assert_eq!(
            (model.first_name().as_str(), model.initials().as_str()),
            ("asha", "A")
        );
        model.set_name("");
        assert_eq!(
            (model.first_name().as_str(), model.initials().as_str()),
            ("You", "?")
        );
        model.set_name(&"x".repeat(40));
        assert_eq!(model.name.chars().count(), NAME_LIMIT);

        assert_eq!(model.history.len(), 3);
        assert_eq!(model.streak(), 3);
        assert_eq!(model.next_call_number(), 4);
        let average = model.average();
        assert!((60..=90).contains(&average), "{average}");

        model.call = play(Scenario::BillingProblem, &[true; 4], 100);
        model.bell = false;
        model.finish_call();
        assert_eq!(model.history.len(), 4);
        assert_eq!(model.streak(), 4);
        assert_eq!(model.report.scenario, Scenario::BillingProblem);
        assert!(model.bell);
        assert_eq!(
            model.notifications()[0].body,
            "Billing problem \u{b7} 98/100"
        );
    }
}
