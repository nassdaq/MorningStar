//! CallReady: a call-centre practice app.
//!
//! Five of its screens are reproduced from a design: five pages exported
//! as HTML (`design/html`), with a browser's rendering of each
//! (`design/reference`). They are illustration (rotated cut-out letters,
//! handwriting, tilted cards, a vinyl disc drawn from thirty circles) and
//! are the test of the framework's transforms and vector pictures
//! (ADR 0017). The other seven are the app's own, in the same visual
//! language, so that every control of the design leads somewhere: a
//! profile, notifications, call history, scenario settings, a transcript,
//! progress, a "more" menu and the server's address.
//!
//! # Online and offline
//!
//! The app is a client of the CallReady server (`server/`, whose contract
//! is `server/API.md`). At start-up it asks the server how it is
//! (`GET /v1/health`), then for the learner's profile and history. When
//! those answer, the app is **online**: the name, the settings, the plan,
//! the numbers and the history are the server's, edits are sent to it, and
//! a practice call is a real conversation with an AI caller, typed when
//! the server has no voice key (`health.voice` false) and spoken when it
//! has one. Nothing in the app changes between the two: the server says
//! which it is, per call.
//!
//! When the server does not answer, the app is **offline** and is exactly
//! what it was before it had a server: the calls are the four scripted
//! ones of [`script`], scored on the phone, over a sample history. Home
//! and the live call say "offline · practice script"; nothing blocks. The
//! server is asked again whenever the learner returns to Home, and from
//! the Server screen.
//!
//! The app holds no secrets: the model and voice keys are the server's.
//! It identifies itself with the installation's id (`X-Device-Id`), which
//! the host makes at first launch.
//!
//! To use it with a phone in development: run the server on the computer
//! (`cd examples/callready/server && npm start`, which listens on
//! `127.0.0.1:8787`), connect the phone over USB and run
//! `adb reverse tcp:8787 tcp:8787`. The app's default address,
//! `http://127.0.0.1:8787`, then reaches it; another address is typed on
//! the Server screen (More, Server) and remembered. Adding
//! `ELEVENLABS_API_KEY` to the server's `.env` and restarting it turns
//! calls from typed to spoken.
//!
//! * [`theme`]: the design's colours and its five typefaces.
//! * [`icons`]: the one-colour icons.
//! * [`components`]: the pieces the screens share.
//! * [`script`]: the four scripted practice calls.
//! * [`json`]: a small JSON reader and string writer.
//! * [`api`]: the server's client: requests built, answers decoded.
//! * [`online`]: a call played by the server, as a state machine.
//! * [`model`]: everything the application remembers.
//! * [`screens`]: one file per screen: its messages and its `view`.
//! * This file: the [`Screen`] list, the application that shows one screen
//!   at a time, the navigation stack, every change of state, and the
//!   requests to the platform ([`Application::effects`]).
//!
//! `update` never talks to anything: it notes what should be asked in
//! `CallReady::out`, and `effects` hands that to the platform. Each answer
//! comes back as a [`Net`] message carrying the numbers it was asked
//! with, and an answer whose numbers are no longer current (an older
//! check of the server, a call that was left) changes nothing.
//!
//! # Structure
//!
//! ```text
//! Welcome (first launch only)
//!   └─ Home ─────────── tab ─┬─ Progress ─ tab
//!       │                    ├─ History ── tab
//!       │                    └─ More ───── tab ─ Server
//!       ├─ Profile ─ Scenario settings, Premium
//!       ├─ Notifications ─ Call report, Progress, Premium
//!       ├─ Scenario settings
//!       └─ Live call ─ Call report ─ Transcript
//! ```
//!
//! The four tab screens carry the tab bar; Home is the root and the other
//! three sit one step above it. Everything else is pushed on a history
//! stack: its back arrow, and the system's Back, return to where the
//! learner came from. Back on Home (or on Welcome) is not handled, which
//! tells the host to leave the app.
//!
//! `SCREENS.md` says how the screens were built and tested;
//! `design/README.md` where the reference images came from.

pub mod api;
pub mod components;
pub mod icons;
pub mod json;
pub mod model;
pub mod online;
pub mod screens;
pub mod script;
pub mod theme;

use std::time::Duration;

use morningstar::ui::prelude::*;

use api::{Client, Failure, ProfilePatch};
use model::{Call, Detail, Link, Live, Model, Plan, Report, Settings, Target};
use online::{Command, OnlineCall};
use screens::{
    call_report, history, home, live_call, more, notifications, premium, profile, progress, server,
    settings, transcript, welcome,
};
use script::Scenario;

/// The key the server's address is stored under.
pub const SERVER_KEY: &str = "server_url";

/// Every screen: the design's five in the design's order, then the app's
/// own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Screen {
    /// 01: the collage that introduces the app.
    #[default]
    Welcome,
    /// 02: today's practice call and the scenarios.
    Home,
    /// 03: a practice call in progress.
    LiveCall,
    /// 04: the score after a call.
    CallReport,
    /// 05: the subscription offer.
    Premium,
    /// 06: the learner's name, numbers, set-up and plan.
    Profile,
    /// 07: what the bell has to say.
    Notifications,
    /// 08: past calls (the third tab).
    History,
    /// 09: the caller's accent and patience, and the call's length.
    Settings,
    /// 10: a finished call, line by line.
    Transcript,
    /// 11: the average score and skills (the second tab).
    Progress,
    /// 12: the menu (the fourth tab).
    More,
    /// 13: where the server is, and whether it answers.
    Server,
}

impl Screen {
    /// Every screen.
    pub const ALL: [Self; 13] = [
        Self::Welcome,
        Self::Home,
        Self::LiveCall,
        Self::CallReport,
        Self::Premium,
        Self::Profile,
        Self::Notifications,
        Self::History,
        Self::Settings,
        Self::Transcript,
        Self::Progress,
        Self::More,
        Self::Server,
    ];

    /// The five screens of the design, in the design's order. Each has a
    /// page in `design/html` and a picture in `design/reference`.
    pub const DESIGNED: [Self; 5] = [
        Self::Welcome,
        Self::Home,
        Self::LiveCall,
        Self::CallReport,
        Self::Premium,
    ];

    /// The screen's name: for a designed screen, its file name in
    /// `design/html` and `design/reference` without extension
    /// (`"02-home"`). Also the id of the screen's root widget.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Welcome => "01-welcome",
            Self::Home => "02-home",
            Self::LiveCall => "03-live-call",
            Self::CallReport => "04-call-report",
            Self::Premium => "05-premium",
            Self::Profile => "06-profile",
            Self::Notifications => "07-notifications",
            Self::History => "08-history",
            Self::Settings => "09-settings",
            Self::Transcript => "10-transcript",
            Self::Progress => "11-progress",
            Self::More => "12-more",
            Self::Server => "13-server",
        }
    }

    /// The id of the screen's scroll view, for
    /// [`Ui::scroll_offset`](morningstar::ui::Ui::scroll_offset) and
    /// [`Ui::rect_of`](morningstar::ui::Ui::rect_of). Every screen has
    /// one: whatever is not a fixed bar scrolls when it does not fit.
    pub const fn scroll_id(self) -> &'static str {
        match self {
            Self::Welcome => "welcome-scroll",
            Self::Home => "home-scroll",
            Self::LiveCall => "live-call-scroll",
            Self::CallReport => "call-report-scroll",
            Self::Premium => "premium-scroll",
            Self::Profile => "profile-scroll",
            Self::Notifications => "notifications-scroll",
            Self::History => "history-scroll",
            Self::Settings => "settings-scroll",
            Self::Transcript => "transcript-scroll",
            Self::Progress => "progress-scroll",
            Self::More => "more-scroll",
            Self::Server => "server-scroll",
        }
    }

    /// True for the five screens that come from the design.
    pub fn is_designed(self) -> bool {
        Self::DESIGNED.contains(&self)
    }

    /// The tab this screen is, if it is one of the four that carry the
    /// tab bar.
    pub const fn tab(self) -> Option<Tab> {
        match self {
            Self::Home => Some(Tab::Home),
            Self::Progress => Some(Tab::Progress),
            Self::History => Some(Tab::History),
            Self::More => Some(Tab::More),
            _ => None,
        }
    }
}

/// An item of the tab bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tab {
    /// Today's call and the scenarios.
    Home,
    /// The average score and skills.
    Progress,
    /// Past calls.
    History,
    /// The menu.
    More,
}

impl Tab {
    /// Every tab, in the bar's order.
    pub const ALL: [Self; 4] = [Self::Home, Self::Progress, Self::History, Self::More];

    /// The screen the tab shows.
    pub const fn screen(self) -> Screen {
        match self {
            Self::Home => Screen::Home,
            Self::Progress => Screen::Progress,
            Self::History => Screen::History,
            Self::More => Screen::More,
        }
    }
}

/// Width of every artboard in the design, in dp.
pub const DESIGN_WIDTH: f32 = 390.0;
/// Height of every artboard in the design, in dp: the phone it was drawn
/// for. On a shorter phone each screen scrolls between its fixed bars.
pub const DESIGN_HEIGHT: f32 = 844.0;

/// Everything a widget can ask for: the message of the screen it is on.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// From the welcome screen.
    Welcome(welcome::Message),
    /// From the home screen.
    Home(home::Message),
    /// From a call in progress.
    LiveCall(live_call::Message),
    /// From the call report.
    CallReport(call_report::Message),
    /// From the subscription offer.
    Premium(premium::Message),
    /// From the profile.
    Profile(profile::Message),
    /// From the notifications.
    Notifications(notifications::Message),
    /// From the call history.
    History(history::Message),
    /// From the scenario settings.
    Settings(settings::Message),
    /// From the transcript.
    Transcript(transcript::Message),
    /// From the progress tab.
    Progress(progress::Message),
    /// From the menu.
    More(more::Message),
    /// From the server's screen.
    Server(server::Message),
    /// An answer from the platform: the server's, the microphone's or the
    /// speaker's.
    Net(Net),
    /// Show a screen directly, in the state the design shows it in (the
    /// call paused at 02:14, the report of 82): for tests and the
    /// screenshot tool. The screen is pushed, so Back returns.
    Open(Screen),
    /// One second has passed: the clock of a running call advances. The
    /// runtime delivers it while [`CallReady::tick`] asks for it.
    Tick,
    /// Back, as a message: the same as the system's Back
    /// ([`CallReady::go_back`]), for the screenshot tool.
    Back,
}

/// What the platform answers a request with.
pub type Answer = Result<HttpResponse, HttpError>;

/// An answer from the platform, with the numbers it was asked with. `sync`
/// numbers the checks of the server, `call` the calls played online, `seq`
/// what the learner sent within a call and `line` the caller's lines.
#[derive(Clone, Debug, PartialEq)]
pub enum Net {
    /// `GET /v1/health` of check `sync`.
    Health(u32, Answer),
    /// `GET /v1/profile` of check `sync`.
    Profile(u32, Answer),
    /// `GET /v1/calls` of check `sync`.
    Calls(u32, Answer),
    /// `GET /v1/profile` again, after a call was scored.
    FreshProfile(u32, Answer),
    /// `GET /v1/calls` again, after a call was scored.
    FreshCalls(u32, Answer),
    /// `PUT /v1/profile` number `save`.
    Saved(u32, Answer),
    /// `GET /v1/calls/{id}` for the call with this id.
    Report(String, Answer),
    /// `POST /v1/calls` for call `call`.
    CallStarted(u32, Answer),
    /// `POST …/turns` number `seq` of call `call`.
    Turn(u32, u32, Answer),
    /// `POST …/end` number `seq` of call `call`.
    Ended(u32, u32, Answer),
    /// The audio of line `line` of call `call`.
    Audio(u32, u32, Answer),
    /// Line `line` of call `call` finished playing.
    Played(u32, u32, Result<(), AudioError>),
    /// The microphone started for call `call`.
    RecordingStarted(u32, Result<(), AudioError>),
    /// The recording stopped with number `seq` of call `call`.
    Recorded(u32, u32, Result<Recording, AudioError>),
}

/// Something `update` has decided to ask of the platform; `effects` asks.
#[derive(Clone, Debug, PartialEq)]
enum Ask {
    /// A request, and the message its answer becomes.
    Http(HttpRequest, Reply),
    Play(AudioClip, u32, u32),
    StopPlayback,
    StartRecording(u32),
    StopRecording(u32, u32),
    Store(&'static str, String),
}

/// Which [`Net`] message an HTTP answer becomes.
#[derive(Clone, Debug, PartialEq)]
enum Reply {
    Health(u32),
    Profile(u32),
    Calls(u32),
    FreshProfile(u32),
    FreshCalls(u32),
    Saved(u32),
    Report(String),
    CallStarted(u32),
    Turn(u32, u32),
    Ended(u32, u32),
    Audio(u32, u32),
    /// An answer nobody waits for (closing a call that was left while it
    /// was still ringing).
    Nobody,
}

impl Reply {
    fn message(self, answer: Answer) -> Message {
        Message::Net(match self {
            Self::Health(sync) => Net::Health(sync, answer),
            Self::Profile(sync) => Net::Profile(sync, answer),
            Self::Calls(sync) => Net::Calls(sync, answer),
            Self::FreshProfile(sync) => Net::FreshProfile(sync, answer),
            Self::FreshCalls(sync) => Net::FreshCalls(sync, answer),
            Self::Saved(save) => Net::Saved(save, answer),
            Self::Report(id) => Net::Report(id, answer),
            Self::CallStarted(call) => Net::CallStarted(call, answer),
            Self::Turn(call, seq) => Net::Turn(call, seq, answer),
            Self::Ended(call, seq) => Net::Ended(call, seq, answer),
            Self::Audio(call, line) => Net::Audio(call, line, answer),
            // Call 0 is never played: the answer falls through every check.
            Self::Nobody => Net::Ended(0, 0, answer),
        })
    }
}

/// The application: the screen being shown, the screens behind it and
/// everything it remembers.
#[derive(Default)]
pub struct CallReady {
    screen: Screen,
    /// The screens Back returns to, the nearest last.
    stack: Vec<Screen>,
    model: Model,
    /// The plan whose card is selected on the offer.
    offer: Plan,
    /// What `update` wants asked of the platform; `effects` empties it.
    out: Vec<Ask>,
    /// Put before the installation's id to make the device id (a test's
    /// rows on a real server are marked this way).
    device_prefix: &'static str,
    /// The id sent as `X-Device-Id`. Empty until `setup` has run: until
    /// then nothing is asked of any server.
    device_id: String,
    /// The number of the latest check of the server.
    sync: u32,
    /// Whether the server of the check in progress has a voice.
    sync_voice: bool,
    /// The profile of the check in progress, until its history arrives.
    checking: Option<(bool, api::Profile)>,
    /// The number of the latest call played online. A call that is left
    /// takes a number too, so that nothing of it can reach the next.
    call: u32,
    /// The number of the latest `PUT /v1/profile`, the patch it carried,
    /// and whether it has been tried a second time.
    save: u32,
    saving: Option<(ProfilePatch, bool)>,
    /// What the server last confirmed of the profile: the state a failed
    /// save returns to.
    confirmed: Option<(String, Settings, bool)>,
    /// The name has been typed in and not sent yet.
    name_dirty: bool,
}

impl CallReady {
    /// The application showing `screen` in the state the design shows it
    /// in, with nothing behind it.
    pub fn on(screen: Screen) -> Self {
        Self {
            screen,
            ..Self::default()
        }
    }

    /// The application with `prefix` before the installation's id in the
    /// device id it sends. For the test that plays a call against a real
    /// server, whose rows are then recognisable (`test_…`).
    pub fn with_device_prefix(mut self, prefix: &'static str) -> Self {
        self.device_prefix = prefix;
        self
    }

    /// The id sent to the server as `X-Device-Id`; empty before `setup`.
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// The screen being shown.
    pub fn screen(&self) -> Screen {
        self.screen
    }

    /// The screens Back returns to, the nearest last.
    pub fn stack(&self) -> &[Screen] {
        &self.stack
    }

    /// Everything the application remembers.
    pub fn model(&self) -> &Model {
        &self.model
    }

    /// The call in progress.
    pub fn call(&self) -> &Call {
        &self.model.call
    }

    /// The plan whose card is selected on the offer.
    pub fn offer(&self) -> Plan {
        self.offer
    }

    /// Returns to where the learner came from: the system's Back and every
    /// back arrow. Leaving a call this way abandons it; nothing is scored.
    ///
    /// Returns false when there is nowhere to return to (Home, or Welcome
    /// on first launch): the host should leave the app. This is the
    /// application's [`Application::back`].
    pub fn go_back(&mut self) -> bool {
        let from = self.screen;
        match from {
            Screen::LiveCall => self.leave_call(),
            Screen::Profile => self.commit_name(),
            _ => {}
        }
        let handled = match self.stack.pop() {
            Some(screen) => {
                self.screen = screen;
                true
            }
            // A screen shown directly, with nothing behind it.
            None if !matches!(self.screen, Screen::Home | Screen::Welcome) => {
                self.screen = Screen::Home;
                true
            }
            None => false,
        };
        self.arrived(from);
        handled
    }

    /// While a call runs, or a notice shows, the message to deliver every
    /// second; `None` when nothing in the app moves by itself. This is the
    /// application's [`Application::tick`].
    pub fn tick(&self) -> Option<(Duration, Message)> {
        let call_runs = self.screen == Screen::LiveCall
            && match &self.model.live {
                Some(Live::Active(_)) => true,
                Some(Live::Dialing(_)) => false,
                None => self.model.call.is_running(),
            };
        (call_runs || self.model.notice.is_some())
            .then_some((Duration::from_secs(1), Message::Tick))
    }

    /// Shows `screen` over the current one.
    fn push(&mut self, screen: Screen) {
        if screen != self.screen {
            self.stack.push(self.screen);
            self.screen = screen;
        }
    }

    /// Shows a tab: Home is the root, the others sit one step above it.
    /// Tapping the tab that is showing changes nothing.
    fn show_tab(&mut self, tab: Tab) {
        let screen = tab.screen();
        if screen == self.screen {
            return;
        }
        self.stack.clear();
        if screen != Screen::Home {
            self.stack.push(Screen::Home);
        }
        self.screen = screen;
    }

    /// After a change of screen: back on Home after the server failed, it
    /// is asked again.
    fn arrived(&mut self, from: Screen) {
        if self.screen == Screen::Home
            && from != Screen::Home
            && matches!(self.model.link, Link::Offline(_))
        {
            self.check_server();
        }
    }

    // ------------------------------------------------------------- server

    /// True once `setup` has run: there is an id to send.
    fn ready(&self) -> bool {
        !self.device_id.is_empty()
    }

    fn client(&self) -> Client {
        Client::new(self.model.server_url.clone(), self.device_id.clone())
    }

    fn ask(&mut self, request: HttpRequest, reply: Reply) {
        self.out.push(Ask::Http(request, reply));
    }

    /// Asks the server how it is; its profile and history follow if it
    /// answers. Answers to earlier checks are ignored from here on.
    fn check_server(&mut self) {
        if !self.ready() {
            return;
        }
        self.sync += 1;
        self.checking = None;
        self.model.link = Link::Checking;
        self.ask(self.client().health(), Reply::Health(self.sync));
    }

    /// The server cannot be used: calls are the practice script.
    fn offline(&mut self, failure: &Failure) {
        self.checking = None;
        self.model.link = match failure {
            // No host behind the app at all: nothing to be offline from.
            Failure::Unsupported => Link::Standalone,
            _ => Link::Offline(failure.sentence()),
        };
    }

    /// A request of an online session failed: if the server was not
    /// reached, the session is over.
    fn note_failure(&mut self, failure: &Failure) {
        if failure.unreachable() && self.model.link.is_online() {
            self.offline(failure);
        }
    }

    /// Takes the server's profile, keeping a name that is being typed.
    fn take_profile(&mut self, profile: &api::Profile) {
        let typed = self.name_dirty.then(|| self.model.name.clone());
        self.model.apply_profile(profile);
        self.confirmed = Some((profile.name.clone(), self.model.settings, profile.premium));
        if let Some(name) = typed {
            self.model.name = name;
        }
    }

    /// Sends a change of the profile, if a server is there to take it. The
    /// screen already shows the change; a failure puts it back.
    fn save(&mut self, patch: ProfilePatch) {
        if !self.model.link.is_online() || !self.ready() {
            return;
        }
        self.save += 1;
        self.ask(self.client().put_profile(&patch), Reply::Saved(self.save));
        self.saving = Some((patch, false));
    }

    /// Sends a name typed on the profile, once the learner is done with
    /// it. An empty name cannot be stored: the last one returns.
    fn commit_name(&mut self) {
        if !std::mem::take(&mut self.name_dirty) {
            return;
        }
        let name = api::tidy_reply(&self.model.name);
        let confirmed = self.confirmed.as_ref().map(|(name, ..)| name.clone());
        if name.is_empty() {
            if let Some(confirmed) = confirmed {
                self.model.name = confirmed;
            }
            return;
        }
        self.model.name.clone_from(&name);
        if confirmed.as_deref() != Some(name.as_str()) {
            self.save(ProfilePatch {
                name: Some(name),
                ..ProfilePatch::default()
            });
        }
    }

    /// A setting was chosen: shown at once, and sent.
    fn set_settings(&mut self, settings: Settings) {
        let before = self.model.settings;
        if settings == before {
            return;
        }
        self.model.settings = settings;
        self.save(ProfilePatch {
            accent: (settings.accent != before.accent).then_some(settings.accent),
            difficulty: (settings.difficulty != before.difficulty).then_some(settings.difficulty),
            length: (settings.length != before.length).then_some(settings.length),
            ..ProfilePatch::default()
        });
    }

    /// Uses the address typed on the Server screen and asks that server
    /// how it is.
    fn use_typed_server(&mut self) {
        match api::normalize_base_url(&self.model.server_draft) {
            Err(problem) => self.model.server_problem = Some(problem.sentence()),
            Ok(url) => {
                self.model.server_problem = None;
                self.model.server_draft.clone_from(&url);
                if url != self.model.server_url {
                    self.model.server_url.clone_from(&url);
                    self.out.push(Ask::Store(SERVER_KEY, url));
                }
                self.check_server();
            }
        }
    }

    // -------------------------------------------------------------- calls

    /// Connects a new call for `scenario`: the server's when there is one,
    /// otherwise the script's. From a report ("Practise again") the call
    /// takes the report's place, so Back does not return to a report that
    /// is no longer the latest.
    fn start_call(&mut self, scenario: Scenario) {
        if self.model.link.is_online() && self.ready() {
            self.call += 1;
            self.model.live = Some(Live::Dialing(scenario));
            self.ask(
                self.client().start_call(scenario),
                Reply::CallStarted(self.call),
            );
        } else {
            self.start_scripted(scenario);
        }
        if self.screen == Screen::CallReport {
            self.screen = Screen::LiveCall;
        } else {
            self.push(Screen::LiveCall);
        }
    }

    fn start_scripted(&mut self, scenario: Scenario) {
        self.model.live = None;
        self.model.call = Call::start(
            scenario,
            self.model.next_call_number(),
            &self.model.settings,
        );
    }

    /// Ends the scripted call and shows its report in the call's place, so
    /// Back from the report does not return into a call that is over.
    fn end_call(&mut self) {
        self.model.finish_call();
        self.screen = Screen::CallReport;
    }

    /// Leaves the call on the line without ending it: whatever it was
    /// recording or playing stops, and its answers no longer count.
    fn leave_call(&mut self) {
        if let Some(Live::Active(mut call)) = self.model.live.take() {
            let commands = call.abandon();
            self.run(&call.id, commands);
        }
        self.call += 1;
    }

    /// Hands an event to the call the server is playing, if it is call
    /// `number` (or whichever it is, for a tap), and carries out what the
    /// call asks for.
    fn in_call(
        &mut self,
        number: Option<u32>,
        event: impl FnOnce(&mut OnlineCall) -> Vec<Command>,
    ) {
        if number.is_some_and(|number| number != self.call) {
            return;
        }
        let Some(Live::Active(call)) = &mut self.model.live else {
            return;
        };
        let commands = event(call);
        let id = call.id.clone();
        self.run(&id, commands);
    }

    /// Carries out what a call asked for.
    fn run(&mut self, call_id: &str, commands: Vec<Command>) {
        let client = self.client();
        let number = self.call;
        for command in commands {
            match command {
                Command::PostText { seq, text } => {
                    self.ask(client.text_turn(call_id, &text), Reply::Turn(number, seq))
                }
                Command::PostAudio { seq, recording } => self.ask(
                    client.audio_turn(call_id, &recording),
                    Reply::Turn(number, seq),
                ),
                Command::FetchAudio { line, path } => {
                    self.ask(client.audio(&path), Reply::Audio(number, line));
                }
                Command::Play { line, clip } => self.out.push(Ask::Play(clip, number, line)),
                Command::StopPlayback => self.out.push(Ask::StopPlayback),
                Command::StartRecording => self.out.push(Ask::StartRecording(number)),
                Command::StopRecording { seq } => {
                    self.out.push(Ask::StopRecording(number, seq));
                }
                Command::End { seq } => {
                    self.ask(client.end_call(call_id), Reply::Ended(number, seq));
                }
                Command::Report(report) => self.show_report(&report),
                Command::Lost(message) => {
                    self.model.live = None;
                    self.call += 1;
                    if self.screen == Screen::LiveCall {
                        self.go_back();
                    }
                    self.model.say(message);
                }
            }
        }
    }

    /// The server has ended the call: its report takes the call's place.
    /// A scored call is news for the bell and changes the numbers, which
    /// are fetched again.
    fn show_report(&mut self, report: &api::CallReport) {
        let report = Report::from_server(report);
        let scored = report.score();
        let known = self
            .model
            .history
            .iter()
            .any(|known| known.call_id() == report.call_id());
        self.model.take_report(report);
        self.model.live = None;
        self.call += 1;
        if self.screen == Screen::LiveCall {
            self.screen = Screen::CallReport;
        }
        let Some(score) = scored else {
            return;
        };
        self.model.bell = true;
        self.model.read[0] = false;
        if !known && let Some(stats) = &mut self.model.stats {
            // Until the server's own figures arrive.
            let total = u32::from(stats.average.unwrap_or(0)) * stats.calls + u32::from(score);
            stats.calls += 1;
            stats.average = Some((total as f32 / stats.calls as f32).round() as u8);
            stats.streak = stats.streak.max(1);
        }
        if self.model.link.is_online() {
            let client = self.client();
            self.ask(client.profile(), Reply::FreshProfile(self.sync));
            self.ask(client.calls(), Reply::FreshCalls(self.sync));
        }
    }

    /// Shows the report of the call at `index` of the history.
    fn open_report(&mut self, index: usize) {
        if let Some(report) = self.model.history.get(index) {
            self.model.report = report.clone();
            self.push(Screen::CallReport);
            self.load_report();
        }
    }

    /// Fetches the rest of the report being looked at, if it is a row of
    /// the server's history.
    fn load_report(&mut self) {
        let Some(server) = &self.model.report.server else {
            return;
        };
        if !matches!(server.detail, Detail::Summary | Detail::Failed(_)) || !self.ready() {
            return;
        }
        let id = server.call_id.clone();
        self.model.set_detail(&id, &Detail::Loading);
        self.ask(self.client().call(&id), Reply::Report(id));
    }

    fn open_notifications(&mut self) {
        self.model.bell = false;
        self.push(Screen::Notifications);
    }

    /// An answer from the platform.
    fn answered(&mut self, net: Net) {
        match net {
            Net::Health(sync, answer) => {
                if sync != self.sync {
                    return;
                }
                match api::decode_health(answer) {
                    Ok(health) if health.ok => {
                        // Not online yet: the profile and the history first.
                        self.checking = None;
                        self.sync_voice = health.voice;
                        self.ask(self.client().profile(), Reply::Profile(sync));
                    }
                    Ok(health) => {
                        self.model.link = Link::Offline(
                            if health.model {
                                "The server\u{2019}s database is not answering."
                            } else {
                                "The server has no AI model key."
                            }
                            .to_owned(),
                        );
                    }
                    Err(failure) => self.offline(&failure),
                }
            }
            Net::Profile(sync, answer) => {
                if sync != self.sync {
                    return;
                }
                match api::decode_profile(answer) {
                    Ok(profile) => {
                        self.checking = Some((self.sync_voice, profile));
                        self.ask(self.client().calls(), Reply::Calls(sync));
                    }
                    Err(failure) => self.offline(&failure),
                }
            }
            Net::Calls(sync, answer) => {
                if sync != self.sync {
                    return;
                }
                let Some((voice, profile)) = self.checking.take() else {
                    return;
                };
                match api::decode_calls(answer) {
                    Ok(calls) => {
                        self.name_dirty = false;
                        self.take_profile(&profile);
                        self.model.apply_history(&calls);
                        self.model.link = Link::Online { voice };
                        // The latest report in full: the progress tab and
                        // the bell's note lead to it.
                        if let Some(latest) = self.model.history.first()
                            && let Some(id) = latest.call_id().map(str::to_owned)
                            && !latest.has_skills()
                        {
                            self.model.set_detail(&id, &Detail::Loading);
                            self.ask(self.client().call(&id), Reply::Report(id));
                        }
                    }
                    Err(failure) => self.offline(&failure),
                }
            }
            Net::FreshProfile(sync, answer) => {
                if sync == self.sync
                    && self.model.link.is_online()
                    && let Ok(profile) = api::decode_profile(answer)
                {
                    self.take_profile(&profile);
                }
            }
            Net::FreshCalls(sync, answer) => {
                if sync == self.sync
                    && self.model.link.is_online()
                    && let Ok(calls) = api::decode_calls(answer)
                {
                    self.model.apply_history(&calls);
                }
            }
            Net::Saved(save, answer) => {
                if save != self.save {
                    return;
                }
                match api::decode_profile(answer) {
                    Ok(profile) => {
                        self.saving = None;
                        self.take_profile(&profile);
                    }
                    Err(failure) => self.save_failed(&failure),
                }
            }
            Net::Report(id, answer) => match api::decode_report(answer) {
                Ok(report) if report.call_id == id => {
                    self.model.fill_report(&Report::from_server(&report));
                }
                Ok(_) => {
                    let failure = Failure::Malformed("another call's report");
                    self.model
                        .set_detail(&id, &Detail::Failed(failure.sentence()));
                }
                Err(failure) => {
                    self.model
                        .set_detail(&id, &Detail::Failed(failure.sentence()));
                }
            },
            Net::CallStarted(number, answer) => self.call_started(number, answer),
            Net::Turn(number, seq, answer) => {
                let result = api::decode_turn(answer);
                self.in_call(Some(number), |call| call.replied(seq, result));
            }
            Net::Ended(number, seq, answer) => {
                let result = api::decode_report(answer);
                self.in_call(Some(number), |call| call.ended(seq, result));
            }
            Net::Audio(number, line, answer) => {
                let result = api::decode_audio(answer);
                self.in_call(Some(number), |call| call.audio_fetched(line, result));
            }
            Net::Played(number, line, result) => {
                self.in_call(Some(number), |call| call.played(line, result));
            }
            Net::RecordingStarted(number, result) => {
                if number == self.call && self.model.online_call().is_some() {
                    self.in_call(Some(number), |call| call.recording_started(result));
                } else if result.is_ok() {
                    // The call it was for is gone: the microphone must not
                    // be left running.
                    self.out.push(Ask::StopRecording(0, 0));
                }
            }
            Net::Recorded(number, seq, result) => {
                self.in_call(Some(number), |call| call.recorded(seq, result));
            }
        }
    }

    /// The answer to `POST /v1/calls`.
    fn call_started(&mut self, number: u32, answer: Answer) {
        let result = api::decode_call_start(answer);
        let scenario = match &self.model.live {
            Some(Live::Dialing(scenario)) if number == self.call => *scenario,
            _ => {
                // The learner hung up while it rang. The call the server
                // made is closed, unscored, so that it is not left open.
                if let Ok(start) = result
                    && self.ready()
                {
                    self.ask(self.client().end_call(&start.call_id), Reply::Nobody);
                }
                return;
            }
        };
        match result {
            Ok(start) => {
                let (call, commands) = OnlineCall::connect(start, scenario, false);
                let id = call.id.clone();
                self.model.live = Some(Live::Active(Box::new(call)));
                self.run(&id, commands);
            }
            // No call from the server: the scripted one, and a word about
            // it.
            Err(failure) => {
                self.start_scripted(scenario);
                self.model.say(if failure.unreachable() {
                    "The server could not be reached. This call is the practice script.".to_owned()
                } else {
                    format!("{} This call is the practice script.", failure.sentence())
                });
                self.note_failure(&failure);
            }
        }
    }

    /// A change of the profile was not stored: once more, quietly; then
    /// the screen returns to what the server holds, with a line saying so.
    fn save_failed(&mut self, failure: &Failure) {
        let again = failure.unreachable() || failure.status().is_some_and(|status| status >= 500);
        match self.saving.take() {
            Some((patch, false)) if again => {
                self.ask(self.client().put_profile(&patch), Reply::Saved(self.save));
                self.saving = Some((patch, true));
            }
            _ => {
                if let Some((name, settings, premium)) = self.confirmed.clone() {
                    if !self.name_dirty {
                        self.model.name = name;
                    }
                    self.model.settings = settings;
                    self.model.premium = premium;
                }
                self.model.say(format!("Not saved. {}", failure.sentence()));
                self.note_failure(failure);
            }
        }
    }
}

impl Application for CallReady {
    type Message = Message;

    fn setup(&mut self, setup: &mut Setup<'_>) {
        // Outfit 400 to 700 first (it is the fallback for what the display
        // faces lack), then the four display faces. All SIL OFL; see
        // assets/fonts/README.md.
        let fonts: [&[u8]; 8] = [
            include_bytes!("../assets/fonts/Outfit-Regular.ttf"),
            include_bytes!("../assets/fonts/Outfit-Medium.ttf"),
            include_bytes!("../assets/fonts/Outfit-SemiBold.ttf"),
            include_bytes!("../assets/fonts/Outfit-Bold.ttf"),
            include_bytes!("../assets/fonts/Caveat-SemiBold.ttf"),
            include_bytes!("../assets/fonts/ArchivoBlack-Regular.ttf"),
            include_bytes!("../assets/fonts/AbrilFatface-Regular.ttf"),
            include_bytes!("../assets/fonts/Anton-Regular.ttf"),
        ];
        for font in fonts {
            setup.add_font(font);
        }
        // Once: who this installation is, where its server is, and the
        // first question to it. (A tool may set an app up twice.)
        if !self.ready() {
            self.device_id = api::device_id(self.device_prefix, setup.install_id());
            if let Some(url) = setup
                .stored(SERVER_KEY)
                .and_then(|stored| api::normalize_base_url(stored).ok())
            {
                self.model.server_draft.clone_from(&url);
                self.model.server_url = url;
            }
            self.check_server();
        }
    }

    fn theme(&self) -> Theme {
        theme::THEME
    }

    fn back(&mut self) -> bool {
        self.go_back()
    }

    fn tick(&self) -> Option<(Duration, Message)> {
        CallReady::tick(self)
    }

    fn view(&self) -> Element<Message> {
        let model = &self.model;
        let screen = match self.screen {
            Screen::Welcome => welcome::view().map(Message::Welcome),
            Screen::Home => home::view(model).map(Message::Home),
            Screen::LiveCall => live_call::view(model).map(Message::LiveCall),
            Screen::CallReport => call_report::view(&model.report).map(Message::CallReport),
            Screen::Premium => premium::view(self.offer, model.premium).map(Message::Premium),
            Screen::Profile => profile::view(model).map(Message::Profile),
            Screen::Notifications => notifications::view(model).map(Message::Notifications),
            Screen::History => history::view(model).map(Message::History),
            Screen::Settings => settings::view(&model.settings).map(Message::Settings),
            Screen::Transcript => transcript::view(&model.report).map(Message::Transcript),
            Screen::Progress => progress::view(model).map(Message::Progress),
            Screen::More => more::view(model).map(Message::More),
            Screen::Server => server::view(model).map(Message::Server),
        };
        match &model.notice {
            None => screen,
            // A line over the screen for a few seconds.
            Some(notice) => stack![screen, components::toast(notice.text.clone())]
                .fill()
                .into(),
        }
    }

    fn update(&mut self, message: Message) {
        let from = self.screen;
        self.handle(message);
        self.arrived(from);
    }

    fn effects(&mut self, out: &mut Effects<Message>) {
        for ask in self.out.drain(..) {
            match ask {
                Ask::Http(request, reply) => {
                    out.http(request, move |answer| reply.message(answer));
                }
                Ask::Play(clip, call, line) => out.play(clip, move |result| {
                    Message::Net(Net::Played(call, line, result))
                }),
                Ask::StopPlayback => out.stop_playback(),
                Ask::StartRecording(call) => out.start_recording(move |result| {
                    Message::Net(Net::RecordingStarted(call, result))
                }),
                Ask::StopRecording(call, seq) => {
                    out.stop_recording(move |result| Message::Net(Net::Recorded(call, seq, result)))
                }
                Ask::Store(key, value) => out.store(key, value),
            }
        }
    }
}

impl CallReady {
    /// Every change of state a message makes.
    fn handle(&mut self, message: Message) {
        match message {
            // "Let's go": Home becomes the root. Welcome is not behind it,
            // so it is seen once per launch.
            Message::Welcome(welcome::Message::Start) => {
                self.stack.clear();
                self.screen = Screen::Home;
            }

            Message::Home(message) => match message {
                home::Message::Profile => self.push(Screen::Profile),
                home::Message::Notifications => self.open_notifications(),
                home::Message::StartCall => self.start_call(Scenario::MissingDelivery),
                home::Message::ScenarioSettings => self.push(Screen::Settings),
                home::Message::Scenario(index) => {
                    if let Some(scenario) = Scenario::CARDS.get(index) {
                        self.start_call(*scenario);
                    }
                }
                home::Message::Tab(tab) => self.show_tab(tab),
            },

            Message::LiveCall(message) => self.live_call(message),

            Message::CallReport(message) => match message {
                call_report::Message::Back => {
                    self.go_back();
                }
                call_report::Message::Transcript => self.push(Screen::Transcript),
                call_report::Message::PractiseAgain => {
                    self.start_call(self.model.report.scenario);
                }
                call_report::Message::Reload => self.load_report(),
            },

            Message::Premium(message) => match message {
                premium::Message::Close => {
                    self.go_back();
                }
                premium::Message::Select(plan) => self.offer = plan,
                // There is no payment: the plan is recorded (on the server
                // too, when there is one) and shown as a demo state on
                // More and the profile.
                premium::Message::Start => {
                    let premium = self.offer == Plan::Premium;
                    if premium != self.model.premium {
                        self.model.premium = premium;
                        self.save(ProfilePatch {
                            premium: Some(premium),
                            ..ProfilePatch::default()
                        });
                    }
                    self.go_back();
                }
            },

            Message::Profile(message) => match message {
                profile::Message::Back => {
                    self.go_back();
                }
                profile::Message::Name(name) => {
                    self.model.set_name(&name);
                    self.name_dirty = self.model.link.is_online();
                }
                // The keyboard's Done: the name is stored here, and sent
                // to the server when there is one.
                profile::Message::NameDone => self.commit_name(),
                profile::Message::Settings => {
                    self.commit_name();
                    self.push(Screen::Settings);
                }
                profile::Message::Premium => {
                    self.commit_name();
                    self.push(Screen::Premium);
                }
            },

            Message::Notifications(message) => match message {
                notifications::Message::Back => {
                    self.go_back();
                }
                notifications::Message::Open(index) => {
                    let Some(notification) = self.model.notifications().get(index).cloned() else {
                        return;
                    };
                    self.model.read[index] = true;
                    match notification.target {
                        Target::LatestReport => self.open_report(0),
                        Target::Progress => self.show_tab(Tab::Progress),
                        Target::Premium => self.push(Screen::Premium),
                    }
                }
            },

            Message::History(message) => match message {
                history::Message::Tab(tab) => self.show_tab(tab),
                history::Message::Open(index) => self.open_report(index),
            },

            Message::Settings(message) => {
                let settings = self.model.settings;
                match message {
                    settings::Message::Back => {
                        self.go_back();
                    }
                    settings::Message::Accent(accent) => {
                        self.set_settings(Settings { accent, ..settings });
                    }
                    settings::Message::Difficulty(difficulty) => {
                        self.set_settings(Settings {
                            difficulty,
                            ..settings
                        });
                    }
                    settings::Message::Length(length) => {
                        self.set_settings(Settings { length, ..settings });
                    }
                }
            }

            Message::Transcript(transcript::Message::Back) => {
                self.go_back();
            }

            Message::Progress(message) => match message {
                progress::Message::Tab(tab) => self.show_tab(tab),
                progress::Message::Latest => self.open_report(0),
            },

            Message::More(message) => match message {
                more::Message::Tab(tab) => self.show_tab(tab),
                more::Message::Profile => self.push(Screen::Profile),
                more::Message::Notifications => self.open_notifications(),
                more::Message::Settings => self.push(Screen::Settings),
                more::Message::Premium => self.push(Screen::Premium),
                more::Message::Server => {
                    self.model.server_draft.clone_from(&self.model.server_url);
                    self.model.server_problem = None;
                    self.push(Screen::Server);
                }
            },

            Message::Server(message) => match message {
                server::Message::Back => {
                    self.go_back();
                }
                server::Message::Url(url) => {
                    self.model.server_draft = url.chars().take(server::URL_LIMIT).collect();
                    self.model.server_problem = None;
                }
                server::Message::Check => self.use_typed_server(),
                server::Message::Reset => {
                    self.model.server_draft = api::DEFAULT_BASE_URL.to_owned();
                    self.use_typed_server();
                }
            },

            Message::Net(net) => self.answered(net),

            Message::Open(screen) => {
                match screen {
                    Screen::LiveCall => {
                        self.leave_call();
                        self.model.call = Call::designed();
                    }
                    Screen::CallReport | Screen::Transcript => {
                        self.model.report = Report::designed();
                    }
                    _ => {}
                }
                self.push(screen);
            }

            Message::Tick => {
                if let Some(notice) = &mut self.model.notice {
                    notice.seconds = notice.seconds.saturating_sub(1);
                    if notice.seconds == 0 {
                        self.model.notice = None;
                    }
                }
                if self.screen == Screen::LiveCall {
                    match &self.model.live {
                        Some(Live::Active(_)) => self.in_call(None, OnlineCall::tick),
                        Some(Live::Dialing(_)) => {}
                        None if self.model.call.is_running() => self.model.call.tick(),
                        None => {}
                    }
                }
            }

            Message::Back => {
                self.go_back();
            }
        }
    }

    /// A tap on the live-call screen: for the call the server is playing
    /// when there is one, otherwise for the scripted call.
    fn live_call(&mut self, message: live_call::Message) {
        use live_call::Message as Tap;
        let online = self.model.online_call().is_some();
        let dialing = matches!(self.model.live, Some(Live::Dialing(_)));
        match message {
            Tap::Back => {
                self.go_back();
            }
            // Hanging up while it rings is leaving.
            Tap::EndCall if dialing => {
                self.go_back();
            }
            _ if dialing => {}
            Tap::Replay if online => self.in_call(None, OnlineCall::replay),
            Tap::ToggleMute if online => self.in_call(None, OnlineCall::toggle_mute),
            Tap::EndCall if online => self.in_call(None, OnlineCall::end),
            Tap::Draft(text) => self.in_call(None, |call| {
                call.input(text);
                Vec::new()
            }),
            Tap::Send => self.in_call(None, OnlineCall::send),
            Tap::Talk => self.in_call(None, OnlineCall::mic),
            Tap::TypeInstead => self.in_call(None, |call| {
                call.type_instead();
                Vec::new()
            }),
            Tap::SpeakInstead => self.in_call(None, |call| {
                call.speak_instead();
                Vec::new()
            }),
            Tap::Retry => self.in_call(None, OnlineCall::retry),
            // A tip chip belongs to the scripted call alone.
            Tap::Choose(_) if online => {}

            Tap::Replay => self.model.call.replay(),
            Tap::ToggleMute => {
                self.model.call.muted = !self.model.call.muted;
            }
            Tap::Choose(option) => {
                if self.model.call.choose(option) && self.model.call.is_finished() {
                    self.end_call();
                }
            }
            Tap::EndCall => self.end_call(),
        }
    }
}

morningstar::app!(Ui::new(CallReady::default()));
