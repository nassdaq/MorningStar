//! Atlas: an AI-assistant app, reproduced from a design, that answers
//! with a real model when its server is there and with a script when it
//! is not.
//!
//! The ten screens of `design/` are in [`screens`], one module each, with
//! the shared pieces in [`components`], the tokens in [`theme`] and the
//! icons in [`icons`]. This file is the application: the state, what each
//! tap does to it, and the timer that plays an answer out. `SCREENS.md`
//! has the table of screens, how close each is to its picture, and what is
//! the app's own design.
//!
//! # With and without a server
//!
//! On start the app asks its server (`server/`, a small Node program that
//! holds the DeepSeek, ElevenLabs and database keys) three questions:
//! `GET /v1/health`, `/v1/profile` and `/v1/chats`. What follows is one of
//! the states of [`Link`]:
//!
//! * [`Link::Online`]: messages are posted to the server and its answer
//!   (title, steps, text, the model that answered) is played out by the
//!   same step card the script uses; the Library lists the server's chats
//!   and reopens them; the model and the plan are saved to the profile;
//!   and, when the server has a voice, live voice records, is transcribed
//!   and hears the answer spoken.
//! * [`Link::Offline`] (the server did not answer) and
//!   [`Link::Standalone`] (nothing behind the app can make a request: a
//!   test, a screenshot): today's scripted app, exactly. The chat's answers
//!   come from [`assistant::respond`] and live voice plays the design's
//!   line.
//!
//! [`api`] builds every request and decodes every answer (`server/API.md`
//! is the contract); this file hands the requests to the platform in
//! `Application::effects` and takes the answers as [`Message::Net`]. Each
//! request carries a number, and an answer whose number is not the one
//! still waited for changes nothing: a reply that arrives after "Cancel
//! task", after the chat was left or deleted, or after live voice was
//! closed is dropped.

pub mod api;
pub mod assistant;
pub mod components;
pub mod format;
pub mod icons;
pub mod json;
pub mod screens;
pub mod theme;

use std::time::Duration;

use morningstar::ui::prelude::*;

use api::{Client, Failure, ProfilePatch};
use assistant::{Request, Response, Step};

/// The key the server's address is stored under on the phone.
pub const SERVER_KEY: &str = "atlas.server";
/// The longest address the Server screen's field takes, in characters.
pub const URL_LIMIT: usize = 200;

/// The design's artboard, in dp.
pub const DESIGN_WIDTH: f32 = 390.0;
/// The design's artboard, in dp.
pub const DESIGN_HEIGHT: f32 = 844.0;

/// The names of the design's ten pages, as its files have them.
pub const DESIGNED: [&str; 10] = [
    "01-onboarding",
    "02-sign-in",
    "03-home",
    "04-chat",
    "05-model-picker",
    "06-live-voice",
    "07-explore",
    "08-plans",
    "09-home-simplified",
    "10-chat-simplified",
];

/// Which of the design's two home-and-chat pairs a screen belongs to.
///
/// Pages 09 and 10 are pages 03 and 04 again: the same markup, the same
/// layout and byte-identical pictures. Only three links differ. In 09 the
/// send button leads to 10 and the Home tab to 09; in 10 the back arrow
/// leads to 09. Every other link of 09 and 10 (Explore, the profile, the
/// model picker, voice) is the one 03 and 04 have, and leads to pages that
/// link back to 03 and 04. So the pair is one view with two identities,
/// not two views.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Flow {
    /// Pages 03 and 04.
    Standard,
    /// Pages 09 and 10.
    Simplified,
}

/// Every screen of the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Screen {
    /// 01.
    Onboarding,
    /// 02.
    SignIn,
    /// 03 and 09.
    Home(Flow),
    /// 04 and 10.
    Chat(Flow),
    /// 05's backdrop: the design's abbreviated home, which the picker's
    /// page draws under its scrim. The app shows the picker over whatever
    /// screen it was opened from; this one is reached only by opening the
    /// app as page 05.
    PickerBackdrop,
    /// 06.
    LiveVoice,
    /// 07.
    Explore,
    /// 08.
    Plans,
    /// The app's own: the conversations of this session.
    Library,
    /// The app's own: where "All models and providers" leads.
    Models,
    /// The app's own: where the server is and whether it answers.
    Server,
}

impl Screen {
    /// Every screen, for tests that visit them all.
    pub const ALL: [Self; 13] = [
        Self::Onboarding,
        Self::SignIn,
        Self::Home(Flow::Standard),
        Self::Chat(Flow::Standard),
        Self::PickerBackdrop,
        Self::LiveVoice,
        Self::Explore,
        Self::Plans,
        Self::Home(Flow::Simplified),
        Self::Chat(Flow::Simplified),
        Self::Library,
        Self::Models,
        Self::Server,
    ];

    /// The screen's name: the design's for a designed screen.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Onboarding => "01-onboarding",
            Self::SignIn => "02-sign-in",
            Self::Home(Flow::Standard) => "03-home",
            Self::Chat(Flow::Standard) => "04-chat",
            Self::PickerBackdrop => "05-model-picker",
            Self::LiveVoice => "06-live-voice",
            Self::Explore => "07-explore",
            Self::Plans => "08-plans",
            Self::Home(Flow::Simplified) => "09-home-simplified",
            Self::Chat(Flow::Simplified) => "10-chat-simplified",
            Self::Library => "library",
            Self::Models => "models",
            Self::Server => "server",
        }
    }

    /// The id of the screen's scroll view.
    pub const fn scroll_id(self) -> &'static str {
        match self {
            Self::Onboarding => "onboarding-scroll",
            Self::SignIn => "sign-in-scroll",
            Self::Home(_) => "home-scroll",
            Self::Chat(_) => "chat-scroll",
            Self::PickerBackdrop => "backdrop-scroll",
            Self::LiveVoice => "voice-scroll",
            Self::Explore => "explore-scroll",
            Self::Plans => "plans-scroll",
            Self::Library => "library-scroll",
            Self::Models => "models-scroll",
            Self::Server => "server-scroll",
        }
    }

    /// The tab this screen is the root of.
    pub const fn tab(self) -> Option<Tab> {
        match self {
            Self::Home(_) => Some(Tab::Home),
            Self::Explore => Some(Tab::Explore),
            Self::Library => Some(Tab::Library),
            _ => None,
        }
    }
}

/// The tab bar's three places.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tab {
    /// "Home".
    Home,
    /// "Explore".
    Explore,
    /// "Library": in the design's tab bar, with no page of its own.
    Library,
}

impl Tab {
    /// In the bar's order.
    pub const ALL: [Self; 3] = [Self::Home, Self::Explore, Self::Library];

    /// The label under the icon.
    pub const fn title(self) -> &'static str {
        match self {
            Self::Home => "Home",
            Self::Explore => "Explore",
            Self::Library => "Library",
        }
    }

    /// The icon of an unselected tab.
    pub fn icon(self) -> &'static SvgAsset {
        match self {
            Self::Home => &icons::HOME,
            Self::Explore => &icons::COMPASS,
            Self::Library => &icons::LIBRARY,
        }
    }

    /// The heavier icon of the selected tab.
    pub fn icon_active(self) -> &'static SvgAsset {
        match self {
            Self::Home => &icons::HOME_ACTIVE,
            Self::Explore => &icons::COMPASS_ACTIVE,
            Self::Library => &icons::LIBRARY_ACTIVE,
        }
    }
}

/// The models of the picker.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    /// "Picks the best fit for each request".
    Auto,
    /// "Quick everyday answers, lowest cost".
    Fast,
    /// "Harder problems and long documents".
    Smart,
    /// "Writing and fixing code".
    Code,
}

impl Mode {
    /// In the picker's order.
    pub const ALL: [Self; 4] = [Self::Auto, Self::Fast, Self::Smart, Self::Code];

    /// The name, as the chip and the picker show it.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Fast => "Fast",
            Self::Smart => "Smart",
            Self::Code => "Code",
        }
    }

    /// The picker's second line.
    pub const fn detail(self) -> &'static str {
        match self {
            Self::Auto => "Picks the best fit for each request",
            Self::Fast => "Quick everyday answers, lowest cost",
            Self::Smart => "Harder problems and long documents",
            Self::Code => "Writing and fixing code",
        }
    }
}

/// The plans of page 08.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Plan {
    /// "$0".
    Free,
    /// "$4.99".
    Basic,
    /// "$9.99", the recommended one.
    Plus,
    /// "$19.99".
    Pro,
}

impl Plan {
    /// The three plans of "Other plans", in the design's order.
    pub const OTHERS: [Self; 3] = [Self::Free, Self::Basic, Self::Pro];

    /// The plan's name.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Free => "Free",
            Self::Basic => "Basic",
            Self::Plus => "Plus",
            Self::Pro => "Pro",
        }
    }

    /// The second line of its row.
    pub const fn detail(self) -> &'static str {
        match self {
            Self::Free => "Basic chats, limited models",
            Self::Basic => "More usage and model choices",
            Self::Plus => "Broader models, higher allowance",
            Self::Pro => "Premium models, highest limits",
        }
    }

    /// Its price a month.
    pub const fn price(self) -> &'static str {
        match self {
            Self::Free => "$0",
            Self::Basic => "$4.99",
            Self::Plus => "$9.99",
            Self::Pro => "$19.99",
        }
    }
}

/// Explore's filter chips.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    /// Everything.
    All,
    /// "Writing".
    Writing,
    /// "Research".
    Research,
    /// "Coding".
    Coding,
    /// "Education".
    Education,
}

impl Category {
    /// In the design's order.
    pub const ALL: [Self; 5] = [
        Self::All,
        Self::Writing,
        Self::Research,
        Self::Coding,
        Self::Education,
    ];

    /// The chip's label.
    pub const fn name(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Writing => "Writing",
            Self::Research => "Research",
            Self::Coding => "Coding",
            Self::Education => "Education",
        }
    }
}

/// One of Explore's tools.
pub struct Tool {
    /// Its name.
    pub title: &'static str,
    /// Its second line.
    pub detail: &'static str,
    /// Its icon.
    pub icon: &'static SvgAsset,
    /// The chips it is listed under (the design does not say; the app's
    /// own sorting).
    pub categories: &'static [Category],
    /// What tapping it asks the assistant.
    pub prompt: &'static str,
}

/// Explore's six tools, as page 07 lists them.
pub static TOOLS: [Tool; 6] = [
    Tool {
        title: "Web research",
        detail: "Answers with sources you can check",
        icon: &icons::GLOBE,
        categories: &[Category::Research, Category::Education],
        prompt: "Research a topic on the web for me, with sources.",
    },
    Tool {
        title: "Summarise documents",
        detail: "PDFs and long files, condensed",
        icon: &icons::FILE,
        categories: &[Category::Writing, Category::Research],
        prompt: "Summarise a document for me.",
    },
    Tool {
        title: "Translate",
        detail: "Text, voice or a photo of a sign",
        icon: &icons::TRANSLATE,
        categories: &[Category::Writing, Category::Education],
        prompt: "Translate something for me.",
    },
    Tool {
        title: "Analyse spreadsheets",
        detail: "Ask questions about your data",
        icon: &icons::TABLE,
        categories: &[Category::Research, Category::Coding],
        prompt: "Analyse a spreadsheet for me.",
    },
    Tool {
        title: "Generate images",
        detail: "From a description \u{b7} uses credits",
        icon: &icons::IMAGE,
        categories: &[Category::Writing],
        prompt: "Create an image for me.",
    },
    Tool {
        title: "Transcribe audio",
        detail: "Recordings into notes and actions",
        icon: &icons::WAVEFORM_LIGHT,
        categories: &[Category::Writing, Category::Education],
        prompt: "Transcribe a recording and list the action items.",
    },
];

/// Home's three suggestions: label, icon and what tapping one asks.
pub static SUGGESTIONS: [(&str, &SvgAsset, &str); 3] = [
    ("Summarise a PDF", &icons::FILE, "Summarise a PDF for me."),
    (
        "Research a topic with sources",
        &icons::GLOBE,
        "Research a topic with sources.",
    ),
    ("Create an image", &icons::IMAGE, "Create an image for me."),
];

/// The app's own "All models and providers": the providers a user could
/// switch on. Invented names; nothing is connected.
pub const PROVIDERS: [(&str, &str); 3] = [
    ("Atlas", "Auto, Fast, Smart and Code"),
    ("Open models", "Community models, run by Atlas"),
    ("Your own key", "Bring a provider you already pay for"),
];

/// The line live voice "hears": the design's.
pub const VOICE_LINE: &str = "Summarise the meeting I just recorded and list the action items.";
/// The design's attachment.
pub const SAMPLE_ATTACHMENT: &str = "3 PDFs";
/// The prompt of the design's conversation.
pub const QUOTE_PROMPT: &str =
    "Compare the prices in these three quotes, find the cheapest option and write me a summary.";

/// How the app stands with its server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Link {
    /// Nothing behind the app can make a request (a test, a screenshot),
    /// or nothing has been asked yet: the scripted app, as designed.
    Standalone,
    /// The server is being asked how it is.
    Checking,
    /// The server answers: chats are real.
    Online {
        /// It has a voice: live voice records and answers are spoken.
        voice: bool,
    },
    /// The server cannot be used, and why, in plain words: the scripted
    /// app.
    Offline(String),
}

impl Link {
    /// True when messages go to the server.
    pub fn is_online(&self) -> bool {
        matches!(self, Self::Online { .. })
    }

    /// True when live voice really records.
    pub fn has_voice(&self) -> bool {
        matches!(self, Self::Online { voice: true })
    }

    /// The state in plain words, for the Server and Models screens.
    pub fn status(&self) -> String {
        match self {
            Self::Standalone => "Not connected. Answers are scripted.".to_owned(),
            Self::Checking => "Checking\u{2026}".to_owned(),
            Self::Online { voice: true } => {
                "Connected. Answers are live, and so is voice.".to_owned()
            }
            Self::Online { voice: false } => {
                "Connected. Answers are live; voice is a demo.".to_owned()
            }
            Self::Offline(why) => format!("Offline. {why} Answers are scripted."),
        }
    }
}

/// How far an answer has got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Asked; nothing back yet.
    Thinking,
    /// The answer's steps are running: the index of the current one.
    Working(usize),
    /// Answered.
    Done,
    /// Stopped by "Cancel task".
    Cancelled,
    /// The server gave no answer: the turn's `error` says why, and
    /// "Retry" sends it again.
    Failed,
}

/// One exchange: what was asked and what has come back so far.
#[derive(Clone, Debug, PartialEq)]
pub struct Turn {
    /// What the user wrote or said.
    pub prompt: String,
    /// The attachment chip above it.
    pub attachment: Option<String>,
    /// The assistant's first line.
    pub intro: Option<String>,
    /// What it does before answering.
    pub steps: Vec<Step>,
    /// How far it has got.
    pub phase: Phase,
    /// Whether the step card is unfolded.
    pub open: bool,
    /// The answer, once there.
    pub reply: Option<String>,
    /// Why the server gave no answer, while `phase` is [`Phase::Failed`].
    pub error: Option<String>,
    /// The model that answered, when the choice was Auto and the server
    /// said which it took.
    pub model_used: Option<Mode>,
    /// The number of the request this turn is waiting for.
    waiting: Option<u32>,
    /// The model chip said Auto when this was sent.
    auto: bool,
}

impl Turn {
    /// A turn just asked: nothing back yet.
    fn asked(prompt: String, attachment: Option<String>) -> Self {
        Self {
            prompt,
            attachment,
            intro: None,
            steps: Vec::new(),
            phase: Phase::Thinking,
            open: false,
            reply: None,
            error: None,
            model_used: None,
            waiting: None,
            auto: false,
        }
    }

    /// True while a request for this turn is with the server (and "Cancel
    /// task" can abandon it).
    pub fn waiting(&self) -> bool {
        self.waiting.is_some()
    }
}

/// How much of a server's chat the app holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Loaded {
    /// Its messages.
    Full,
    /// Its title and how many messages it has: the Library's row.
    Summary,
    /// Its messages have been asked for.
    Loading,
    /// They could not be fetched, and why.
    Failed(String),
}

/// What makes a conversation the server's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Remote {
    /// The chat's id on the server; `None` until its first message has
    /// been answered.
    pub id: Option<String>,
    /// How many exchanges the server holds.
    pub count: u32,
    /// How much of it is here.
    pub loaded: Loaded,
    /// The number of the request `Loading` waits for.
    load: u32,
}

/// A conversation: what the chat screen shows and the Library lists.
#[derive(Clone, Debug, PartialEq)]
pub struct Conversation {
    /// The chat's title.
    pub title: String,
    /// Its exchanges, oldest first.
    pub turns: Vec<Turn>,
    /// `Some` for a chat held by the server, `None` for a scripted one of
    /// this run of the app.
    pub remote: Option<Remote>,
    /// What answers from the server find the conversation by: its place in
    /// the list can change, this cannot.
    key: u32,
}

impl Conversation {
    /// The design's conversation, as page 04 shows it: three steps in.
    pub fn sample() -> Self {
        Self {
            title: "Compare supplier quotes".to_owned(),
            turns: vec![Turn {
                intro: Some(assistant::QUOTE_INTRO.to_owned()),
                steps: assistant::quote_steps(),
                phase: Phase::Working(2),
                ..Turn::asked(QUOTE_PROMPT.to_owned(), Some(SAMPLE_ATTACHMENT.to_owned()))
            }],
            remote: None,
            key: 0,
        }
    }

    /// True for a chat held by the server.
    pub fn is_remote(&self) -> bool {
        self.remote.is_some()
    }

    /// The chat's id on the server, once it has one.
    pub fn id(&self) -> Option<&str> {
        self.remote.as_ref().and_then(|remote| remote.id.as_deref())
    }

    /// True while a message of this chat is with the server.
    pub fn busy(&self) -> bool {
        self.turns.iter().any(Turn::waiting)
    }

    /// How much of a server's chat is here; `Full` for a scripted one.
    pub fn loaded(&self) -> &Loaded {
        self.remote
            .as_ref()
            .map_or(&Loaded::Full, |remote| &remote.loaded)
    }
}

/// What live voice is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Listen {
    /// The simulation: the design's line, a word at a time. What live
    /// voice is without a server that has a voice.
    Demo,
    /// The microphone has been asked for.
    Starting,
    /// Recording.
    Recording,
    /// The recording has been asked to stop.
    Stopping,
    /// The recording is with the server.
    Sending,
    /// The server heard it: its transcript shows for a moment before the
    /// chat opens.
    Heard,
    /// Recording or sending failed: the screen says why.
    Failed,
}

/// Live voice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Voice {
    /// How many words of [`VOICE_LINE`] have been "heard".
    pub words: usize,
    /// Ticks since the last word (or since recording began, or since the
    /// transcript arrived).
    pub since: u32,
    /// The microphone button's state, in the demo.
    pub muted: bool,
    /// What it is doing.
    pub live: Listen,
}

/// What the speaker is doing with a spoken answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Speech {
    Silent,
    Fetching,
    Playing,
}

/// Ticks per second: the spinner and the orb move at this rate.
pub const TICKS_PER_SECOND: u32 = 30;
/// Ticks before the scripted answer "arrives".
pub const THINKING_TICKS: u32 = 18;
/// Ticks each step of an answer takes.
pub const STEP_TICKS: u32 = 24;
/// Ticks between the words live voice hears.
pub const WORD_TICKS: u32 = 7;
/// Ticks the finished line stays before live voice hands it to the chat.
pub const HOLD_TICKS: u32 = 36;
/// Ticks a toast stays.
pub const TOAST_TICKS: u32 = 75;
/// Ticks after which a recording is stopped and sent: 55 seconds, inside
/// the platform's own limit of 60.
pub const RECORDING_TICKS: u32 = 55 * TICKS_PER_SECOND;
/// A recording shorter than this holds no words.
const SHORTEST_RECORDING: Duration = Duration::from_millis(400);

/// What the platform answers a request with.
pub type Wire = Result<HttpResponse, HttpError>;

/// An answer from the platform: the server's, the microphone's or the
/// speaker's. Each carries the number its request was given.
#[derive(Clone, Debug, PartialEq)]
pub enum Net {
    /// `GET /v1/health` of check `sync`.
    Health(u32, Wire),
    /// `GET /v1/profile` of check `sync`.
    Profile(u32, Wire),
    /// `GET /v1/chats` of check `sync`.
    Chats(u32, Wire),
    /// `PUT /v1/profile`.
    Saved(Wire),
    /// The answer to request `.1`, a message of the conversation with key
    /// `.0`.
    Reply(u32, u32, Wire),
    /// `GET /v1/chats/{id}`, request `.1`, for the conversation with key
    /// `.0`.
    Chat(u32, u32, Wire),
    /// `DELETE /v1/chats/{id}` for the conversation with key `.0`.
    Deleted(u32, Wire),
    /// The answer to the recording of voice session `.0`, request `.1`.
    Spoken(u32, u32, Wire),
    /// The audio of spoken answer `.0`.
    Audio(u32, Wire),
    /// Spoken answer `.0` finished playing.
    Played(u32, Result<(), AudioError>),
    /// The microphone started for voice session `.0`.
    RecordingStarted(u32, Result<(), AudioError>),
    /// The recording of voice session `.0` stopped.
    Recorded(u32, Result<Recording, AudioError>),
}

/// What a tap, a key or the timer asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// Onboarding's two buttons: both lead to sign-in, as in the design.
    GetStarted,
    /// The email field.
    Email(String),
    /// The password field.
    Password(String),
    /// "Sign in", and the keyboard's Done in the password field.
    SignIn,
    /// "Continue with Google".
    Google,
    /// "Forgot password?".
    ForgotPassword,
    /// "Create an account".
    CreateAccount,
    /// The profile button.
    OpenPlans,
    /// The model chip.
    OpenPicker,
    /// The scrim.
    ClosePicker,
    /// A model in the picker or on the models screen.
    SelectMode(Mode),
    /// "All models and providers".
    AllModels,
    /// A provider's row on the models screen.
    ToggleProvider(usize),
    /// A tab.
    Tab(Tab),
    /// A back arrow, the minimise chevron.
    Back,
    /// The composer's text.
    Draft(String),
    /// The send button, and the keyboard's Send.
    Send,
    /// The composer's "+".
    Attach,
    /// The chip of the attachment waiting to be sent.
    Detach,
    /// The composer's microphone.
    OpenVoice,
    /// One of home's suggestions.
    Suggest(usize),
    /// The header of a step card.
    ToggleSteps(usize),
    /// "Cancel task".
    CancelTask(usize),
    /// An attachment chip in the conversation.
    AttachmentInfo,
    /// Live voice's camera.
    VoiceCamera,
    /// Live voice's microphone.
    VoiceMute,
    /// Live voice's cross: end, and show the transcript.
    VoiceEnd,
    /// Explore's search field.
    Query(String),
    /// One of Explore's chips.
    Filter(Category),
    /// One of Explore's tools.
    Tool(usize),
    /// "Get Plus" and the rows of "Other plans".
    Choose(Plan),
    /// A conversation in the Library.
    OpenChat(usize),
    /// The Library's "New chat".
    NewChat,
    /// The script's answer to turn `.1` of conversation `.0`. (The
    /// server's arrives as [`Net::Reply`].)
    Answered(usize, usize, Response),
    /// "Retry" under a turn the server did not answer.
    Retry(usize),
    /// "Retry" on a chat whose messages could not be fetched.
    Reload,
    /// "Delete this chat", and the same button again to confirm.
    DeleteChat,
    /// "Stop" beside a spoken answer.
    StopSpeaking,
    /// Live voice's "Send again", after a recording did not get through.
    VoiceRetry,
    /// The live/offline mark, and the Models screen's "Server" row.
    OpenServer,
    /// The Server screen's address field.
    ServerUrl(String),
    /// "Check connection", and the keyboard's Go in the address field.
    CheckServer,
    /// "Use the default address".
    DefaultServer,
    /// An answer from the platform.
    Net(Net),
    /// The timer.
    Tick,
}

/// What `update` has decided the platform should do; `effects` hands it
/// over.
enum Ask {
    Scroll(ScrollTo),
    Announce(String),
    /// A request, and the message its answer becomes.
    Http(HttpRequest, Route),
    Play(AudioClip, u32),
    StopPlayback,
    StartRecording(u32),
    StopRecording(u32),
    Store(&'static str, String),
}

/// Which [`Net`] message an HTTP answer becomes.
#[derive(Clone, Copy)]
enum Route {
    Health(u32),
    Profile(u32),
    Chats(u32),
    Saved,
    Reply(u32, u32),
    Chat(u32, u32),
    Deleted(u32),
    Spoken(u32, u32),
    Audio(u32),
}

impl Route {
    fn message(self, wire: Wire) -> Message {
        Message::Net(match self {
            Self::Health(sync) => Net::Health(sync, wire),
            Self::Profile(sync) => Net::Profile(sync, wire),
            Self::Chats(sync) => Net::Chats(sync, wire),
            Self::Saved => Net::Saved(wire),
            Self::Reply(key, request) => Net::Reply(key, request, wire),
            Self::Chat(key, request) => Net::Chat(key, request, wire),
            Self::Deleted(key) => Net::Deleted(key, wire),
            Self::Spoken(session, request) => Net::Spoken(session, request, wire),
            Self::Audio(speech) => Net::Audio(speech, wire),
        })
    }
}

/// A chat's title until the server has written one: the start of its
/// first message.
fn provisional_title(prompt: &str) -> String {
    let mut title: String = prompt.chars().take(28).collect();
    if title.len() < prompt.len() {
        title.push('\u{2026}');
    }
    title
}

/// The application.
pub struct Atlas {
    nav: Navigator<Screen>,
    /// The model picker is open over the current screen.
    picker: bool,
    mode: Mode,
    email: String,
    password: String,
    sign_in_error: Option<&'static str>,
    draft: String,
    /// The attachment that the next message will carry.
    attachment: Option<String>,
    chats: Vec<Conversation>,
    /// Which conversation the chat screen shows.
    current: usize,
    /// The answer on its way: conversation, turn, and the tick it is due.
    pending: Option<(usize, usize, Response, u32)>,
    /// The tick the current step began at.
    step_since: u32,
    voice: Voice,
    query: String,
    category: Category,
    plan: Option<Plan>,
    providers: [bool; 3],
    notice: Option<(String, u32)>,
    /// Ticks since the app started: what the spinner and the orb turn by.
    clock: u32,
    /// A still frame: nothing moves until something is touched. The
    /// design's pages are opened like this, so a screenshot is the design's
    /// picture and not a moment of an animation.
    still: bool,
    asks: Vec<Ask>,

    // ------------------------------------------------------- the server
    link: Link,
    /// Put before the installation's id (`test_` for a test against a
    /// real server).
    device_prefix: &'static str,
    /// Who this installation is to the server; empty until `setup`.
    device_id: String,
    server_url: String,
    /// What the Server screen's field holds.
    server_draft: String,
    /// What is wrong with it, or with checking at all.
    server_problem: Option<String>,
    /// "Check connection" was tapped: say so if nothing can be checked.
    checked_by_hand: bool,
    /// The number of the latest check of the server.
    sync: u32,
    /// What the check has learnt so far: the voice, then the profile.
    sync_voice: bool,
    sync_profile: Option<api::Profile>,
    /// Numbers requests, so that a late answer is told from a current one.
    requests: u32,
    /// Numbers conversations.
    keys: u32,
    /// "Delete this chat" was tapped once.
    confirm_delete: bool,
    /// The conversation being deleted on the server.
    deleting: Option<u32>,

    // ------------------------------------------------------- live voice
    /// The number of the voice session: every opening of live voice, and
    /// every leaving of it, is a new one.
    voice_no: u32,
    /// The conversation voice was opened from, which the message joins.
    voice_target: Option<u32>,
    /// The recording, kept until it is answered so that it can be resent.
    voice_kept: Option<Recording>,
    /// The request the recording is waiting for.
    voice_wait: u32,
    /// What the server heard.
    voice_heard: Option<String>,
    voice_error: Option<String>,
    /// The answer, held while the transcript shows.
    voice_reply: Option<api::Reply>,

    // ---------------------------------------------------- spoken answer
    speech: Speech,
    speech_no: u32,
    /// The answer's audio, fetched before the chat opened.
    speech_clip: Option<AudioClip>,
    /// The conversation and turn being spoken.
    speech_at: Option<(u32, usize)>,
}

impl Default for Atlas {
    fn default() -> Self {
        Self::on(Screen::Onboarding)
    }
}

impl Atlas {
    /// The app opened directly on `screen`, as a person would find it.
    pub fn on(screen: Screen) -> Self {
        let mut app = Self {
            nav: Navigator::new(screen),
            picker: false,
            mode: Mode::Auto,
            email: String::new(),
            password: String::new(),
            sign_in_error: None,
            draft: String::new(),
            attachment: None,
            chats: vec![Conversation::sample()],
            current: 0,
            pending: None,
            step_since: 0,
            voice: Voice {
                words: 0,
                since: 0,
                muted: false,
                live: Listen::Demo,
            },
            query: String::new(),
            category: Category::All,
            plan: None,
            providers: [true, false, false],
            notice: None,
            clock: 0,
            still: false,
            asks: Vec::new(),
            link: Link::Standalone,
            device_prefix: "",
            device_id: String::new(),
            server_url: api::DEFAULT_BASE_URL.to_owned(),
            server_draft: api::DEFAULT_BASE_URL.to_owned(),
            server_problem: None,
            checked_by_hand: false,
            sync: 0,
            sync_voice: false,
            sync_profile: None,
            requests: 0,
            keys: 0,
            confirm_delete: false,
            deleting: None,
            voice_no: 0,
            voice_target: None,
            voice_kept: None,
            voice_wait: 0,
            voice_heard: None,
            voice_error: None,
            voice_reply: None,
            speech: Speech::Silent,
            speech_no: 0,
            speech_clip: None,
            speech_at: None,
        };
        if screen == Screen::LiveVoice {
            app.voice.words = VOICE_LINE.split(' ').count();
        }
        if !matches!(screen, Screen::Chat(_)) {
            // Not looking at it: the design's conversation has finished.
            let turn = &mut app.chats[0].turns[0];
            let request = Request {
                mode: Mode::Auto,
                prompt: turn.prompt.clone(),
                attachment: turn.attachment.clone(),
                history: Vec::new(),
            };
            turn.reply = Some(assistant::respond(&request).reply);
            turn.phase = Phase::Done;
        }
        app
    }

    /// The app in the state one of the design's pages shows, as a still
    /// frame. `None` for a name that is not in [`DESIGNED`].
    pub fn designed(name: &str) -> Option<Self> {
        let screen = Screen::ALL
            .into_iter()
            .find(|screen| screen.name() == name)?;
        if !DESIGNED.contains(&name) {
            return None;
        }
        let mut app = Self::on(screen);
        app.picker = screen == Screen::PickerBackdrop;
        app.still = true;
        Some(app)
    }

    /// The screen on top.
    pub fn screen(&self) -> Screen {
        *self.nav.current()
    }

    /// The screens under and including the current one.
    pub fn stack(&self) -> &[Screen] {
        self.nav.stack()
    }

    /// Whether the model picker is open.
    pub fn picker_open(&self) -> bool {
        self.picker
    }

    /// The chosen model.
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// The plan chosen on this phone, if one was (a demo: nothing is
    /// bought).
    pub fn plan(&self) -> Option<Plan> {
        self.plan
    }

    /// The same app, with `prefix` before the installation's id in what
    /// it tells the server (`test_`, for a test against a real server,
    /// whose rows can then be told apart and deleted).
    #[must_use]
    pub fn with_device_prefix(mut self, prefix: &'static str) -> Self {
        self.device_prefix = prefix;
        self
    }

    /// How the app stands with its server.
    pub fn link(&self) -> &Link {
        &self.link
    }

    /// Who this installation is to the server; empty before `setup`.
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// The server's address in use.
    pub fn server_url(&self) -> &str {
        &self.server_url
    }

    /// What the Server screen's address field holds.
    pub fn server_draft(&self) -> &str {
        &self.server_draft
    }

    /// What is wrong with the address typed, or with checking at all.
    pub fn server_problem(&self) -> Option<&str> {
        self.server_problem.as_deref()
    }

    /// The conversations: the scripted ones of this run first, then the
    /// server's; in each group the newest last.
    pub fn chats(&self) -> &[Conversation] {
        &self.chats
    }

    /// The conversations the Library lists, as indices into
    /// [`Atlas::chats`], oldest first: the server's when it answers, this
    /// run's scripted ones when it does not.
    pub fn library(&self) -> Vec<usize> {
        let online = self.link.is_online();
        (0..self.chats.len())
            .filter(|&index| self.chats[index].is_remote() == online)
            .collect()
    }

    /// True once "Delete this chat" has been tapped once: the next tap
    /// deletes.
    pub fn confirming_delete(&self) -> bool {
        self.confirm_delete
    }

    /// True while the conversation on screen is being deleted.
    pub fn deleting(&self) -> bool {
        self.deleting == Some(self.chats[self.current].key)
    }

    /// The turn of the conversation on screen whose answer is being
    /// spoken (or its audio fetched).
    pub fn speaking(&self) -> Option<usize> {
        let (key, turn) = self.speech_at?;
        (self.speech != Speech::Silent && self.chats[self.current].key == key).then_some(turn)
    }

    /// What the server heard in the recording, while live voice shows it.
    pub fn voice_heard(&self) -> Option<&str> {
        self.voice_heard.as_deref()
    }

    /// Why live voice could not record or send.
    pub fn voice_error(&self) -> Option<&str> {
        self.voice_error.as_deref()
    }

    /// True if live voice holds a recording that can be sent again.
    pub fn voice_can_resend(&self) -> bool {
        self.voice.live == Listen::Failed && self.voice_kept.is_some()
    }

    /// The conversation the chat screen shows.
    pub fn conversation(&self) -> &Conversation {
        &self.chats[self.current]
    }

    /// What the composer holds.
    pub fn draft(&self) -> &str {
        &self.draft
    }

    /// The attachment waiting to be sent.
    pub fn attachment(&self) -> Option<&str> {
        self.attachment.as_deref()
    }

    /// Live voice's state.
    pub fn voice(&self) -> Voice {
        self.voice
    }

    /// Explore's search text and chip.
    pub fn filter(&self) -> (&str, Category) {
        (&self.query, self.category)
    }

    /// The tools Explore lists for its search text and chip, as indices
    /// into [`TOOLS`].
    pub fn tools(&self) -> Vec<usize> {
        let query = self.query.trim().to_lowercase();
        (0..TOOLS.len())
            .filter(|&index| {
                let tool = &TOOLS[index];
                let listed =
                    self.category == Category::All || tool.categories.contains(&self.category);
                let found = query.is_empty()
                    || tool.title.to_lowercase().contains(&query)
                    || tool.detail.to_lowercase().contains(&query);
                listed && found
            })
            .collect()
    }

    /// What the email field holds.
    pub fn email(&self) -> &str {
        &self.email
    }

    /// What the password field holds.
    pub fn password(&self) -> &str {
        &self.password
    }

    /// The sign-in form's message, if it has one.
    pub fn sign_in_error(&self) -> Option<&'static str> {
        self.sign_in_error
    }

    /// The toast, while one shows.
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_ref().map(|(text, _)| text.as_str())
    }

    /// Which providers are switched on.
    pub fn providers(&self) -> [bool; 3] {
        self.providers
    }

    /// Ticks since the app started.
    pub fn clock(&self) -> u32 {
        self.clock
    }

    /// Degrees the spinner has turned: once a second.
    pub fn spin(&self) -> f32 {
        if self.still {
            0.0
        } else {
            (self.clock % TICKS_PER_SECOND) as f32 * 360.0 / TICKS_PER_SECOND as f32
        }
    }

    /// The orb's size on live voice, as a factor: it breathes while it
    /// listens.
    pub fn pulse(&self) -> f32 {
        let breathing = match self.voice.live {
            Listen::Demo => !self.voice.muted,
            Listen::Recording => true,
            _ => false,
        };
        if self.still || !breathing {
            1.0
        } else {
            1.0 + 0.035 * (self.clock as f32 * 0.21).sin()
        }
    }

    /// True while an answer is on its way.
    pub fn answering(&self) -> bool {
        self.pending.is_some()
            || self.chats.iter().any(|chat| {
                chat.turns
                    .iter()
                    .any(|turn| matches!(turn.phase, Phase::Thinking | Phase::Working(_)))
            })
    }

    /// True while the chat on screen is being fetched (its spinner turns).
    fn loading(&self) -> bool {
        matches!(self.screen(), Screen::Chat(_))
            && (*self.chats[self.current].loaded() == Loaded::Loading || self.deleting.is_some())
    }

    /// True while the demo of live voice is "hearing" its line.
    fn listening(&self) -> bool {
        self.screen() == Screen::LiveVoice && !self.voice.muted && self.voice.live == Listen::Demo
    }

    /// True while real live voice has something moving: the orb, the
    /// spinner, the moment the transcript shows.
    fn voice_busy(&self) -> bool {
        self.screen() == Screen::LiveVoice
            && !matches!(self.voice.live, Listen::Demo | Listen::Failed)
    }

    fn flow(&self) -> Flow {
        self.nav
            .stack()
            .iter()
            .rev()
            .find_map(|screen| match screen {
                Screen::Home(flow) | Screen::Chat(flow) => Some(*flow),
                _ => None,
            })
            .unwrap_or(Flow::Standard)
    }

    fn toast(&mut self, text: &str) {
        self.notice = Some((text.to_owned(), TOAST_TICKS));
        self.asks.push(Ask::Announce(text.to_owned()));
    }

    fn show_latest(&mut self) {
        self.asks.push(Ask::Scroll(
            ScrollTo::end(Screen::Chat(Flow::Standard).scroll_id()).animated(),
        ));
    }

    /// Opens the chat screen of the current flow, once.
    fn open_chat(&mut self) {
        let chat = Screen::Chat(self.flow());
        if self.screen() != chat {
            // Voice and the other screens over home are left behind.
            self.nav.pop_to(|screen| matches!(screen, Screen::Home(_)));
            self.nav.push(chat);
        }
    }

    /// Sends `prompt` in the conversation on screen (a new one unless the
    /// chat is already open) and asks the assistant.
    fn ask(&mut self, prompt: String, attachment: Option<String>) {
        let prompt = prompt.trim().to_owned();
        if prompt.is_empty() {
            return;
        }
        let here = matches!(self.screen(), Screen::Chat(_));
        // A chat the server holds continues there; a new one is the
        // server's when it answers, and the script's when it does not.
        let remote = if here {
            self.chats[self.current].is_remote()
        } else {
            self.link.is_online()
        };
        if remote {
            self.ask_server(prompt, attachment, here);
            return;
        }
        if !here {
            // The scripted conversations stay ahead of the server's.
            let at = self.locals();
            let key = self.new_key();
            self.chats.insert(
                at,
                Conversation {
                    title: provisional_title(&prompt),
                    turns: Vec::new(),
                    remote: None,
                    key,
                },
            );
            self.current = at;
        }
        let request = Request {
            mode: self.mode,
            prompt: prompt.clone(),
            attachment: attachment.clone(),
            history: self.chats[self.current]
                .turns
                .iter()
                .map(|turn| turn.prompt.clone())
                .collect(),
        };
        self.chats[self.current]
            .turns
            .push(Turn::asked(prompt, attachment));
        let turn = self.chats[self.current].turns.len() - 1;
        // The script's answer comes back from the timer, a moment later,
        // as `Message::Answered`; the server's (`ask_server`) comes back
        // from the network as `Net::Reply`. Both end in `answered`.
        let response = assistant::respond(&request);
        self.pending = Some((self.current, turn, response, self.clock + THINKING_TICKS));
        self.open_chat();
        self.show_latest();
    }

    /// The answer to a turn has arrived: its steps start.
    fn answered(&mut self, chat: usize, turn: usize, response: Response) {
        let Some(turn) = self
            .chats
            .get_mut(chat)
            .and_then(|chat| chat.turns.get_mut(turn))
        else {
            return;
        };
        if turn.phase != Phase::Thinking {
            return;
        }
        // The server's answers have no first line of their own.
        turn.intro = (!response.intro.is_empty()).then_some(response.intro);
        turn.reply = Some(response.reply);
        turn.phase = if response.steps.is_empty() {
            Phase::Done
        } else {
            Phase::Working(0)
        };
        turn.steps = response.steps;
        self.step_since = self.clock;
        self.show_latest();
    }

    fn tick(&mut self) {
        self.clock = self.clock.wrapping_add(1);
        if let Some((_, left)) = &mut self.notice {
            *left = left.saturating_sub(1);
            if *left == 0 {
                self.notice = None;
            }
        }
        if let Some((chat, turn, _, due)) = &self.pending
            && self.clock >= *due
        {
            let (chat, turn) = (*chat, *turn);
            if let Some((_, _, response, _)) = self.pending.take() {
                self.update(Message::Answered(chat, turn, response));
            }
        }
        // Steps advance one at a time; the last one finishing is the reply.
        if self.clock.wrapping_sub(self.step_since) >= STEP_TICKS {
            let mut moved = false;
            for chat in &mut self.chats {
                for turn in &mut chat.turns {
                    if let Phase::Working(step) = turn.phase {
                        turn.phase = if step + 1 < turn.steps.len() {
                            Phase::Working(step + 1)
                        } else {
                            Phase::Done
                        };
                        moved = true;
                    }
                }
            }
            if moved {
                self.step_since = self.clock;
                if matches!(self.screen(), Screen::Chat(_)) {
                    self.show_latest();
                }
            }
        }
        if self.screen() == Screen::LiveVoice {
            match self.voice.live {
                Listen::Recording => {
                    self.voice.since += 1;
                    if self.voice.since >= RECORDING_TICKS {
                        self.stop_recording();
                    }
                }
                Listen::Heard => {
                    self.voice.since += 1;
                    if self.voice.since >= HOLD_TICKS {
                        self.deliver_voice();
                    }
                }
                _ => {}
            }
        }
        if self.listening() {
            let total = VOICE_LINE.split(' ').count();
            self.voice.since += 1;
            if self.voice.words < total {
                if self.voice.since >= WORD_TICKS {
                    self.voice.words += 1;
                    self.voice.since = 0;
                }
            } else if self.voice.since >= HOLD_TICKS {
                self.finish_voice();
            }
        }
    }

    /// Live voice's cross. In the demo, what it "heard" is sent to the
    /// chat; while recording, the recording is sent; while the transcript
    /// shows, the chat opens at once; otherwise voice is simply left.
    fn end_voice(&mut self) {
        match self.voice.live {
            Listen::Demo => self.finish_voice(),
            Listen::Recording => self.stop_recording(),
            Listen::Heard => self.deliver_voice(),
            Listen::Starting | Listen::Stopping | Listen::Sending | Listen::Failed => {
                self.go_back();
            }
        }
    }

    /// The demo of live voice ends: what it heard is sent to the chat.
    fn finish_voice(&mut self) {
        let heard: Vec<&str> = VOICE_LINE.split(' ').take(self.voice.words).collect();
        let heard = heard.join(" ");
        self.nav.pop_to(|screen| *screen != Screen::LiveVoice);
        if heard.is_empty() {
            self.open_chat();
        } else {
            self.ask(heard, None);
        }
    }

    // ---------------------------------------------------------- the server

    /// True once `setup` has run: there is an id to send.
    fn ready(&self) -> bool {
        !self.device_id.is_empty()
    }

    fn client(&self) -> Client {
        Client::new(self.server_url.clone(), self.device_id.clone())
    }

    fn http(&mut self, request: HttpRequest, route: Route) {
        self.asks.push(Ask::Http(request, route));
    }

    fn new_key(&mut self) -> u32 {
        self.keys += 1;
        self.keys
    }

    fn next_request(&mut self) -> u32 {
        self.requests += 1;
        self.requests
    }

    /// How many scripted conversations there are: they come first.
    fn locals(&self) -> usize {
        self.chats.iter().filter(|chat| !chat.is_remote()).count()
    }

    fn index_of(&self, key: u32) -> Option<usize> {
        self.chats.iter().position(|chat| chat.key == key)
    }

    /// Asks the server how it is; its profile and chats follow if it
    /// answers. Answers to earlier checks are ignored from here on.
    fn check_server(&mut self) {
        if !self.ready() {
            return;
        }
        self.sync += 1;
        self.sync_profile = None;
        self.link = Link::Checking;
        self.http(self.client().health(), Route::Health(self.sync));
    }

    /// The server cannot be used: answers are the script's.
    fn offline(&mut self, failure: &Failure) {
        self.sync_profile = None;
        self.link = match failure {
            // No host behind the app at all: nothing to be offline from.
            Failure::Unsupported => Link::Standalone,
            _ => Link::Offline(failure.sentence()),
        };
        if std::mem::take(&mut self.checked_by_hand) && self.link == Link::Standalone {
            self.server_problem = Some(
                "Nothing behind this build of the app can make a request, so there is no server to check."
                    .to_owned(),
            );
        }
    }

    /// A request of an online session failed: if the server was not
    /// reached, the session is over until it answers again.
    fn note_failure(&mut self, failure: &Failure) {
        if failure.unreachable() && self.link.is_online() {
            self.offline(failure);
        }
    }

    /// The chat screens leave the stack: the conversation they showed is
    /// gone, or is not one of the list being shown any more.
    fn drop_chat_screens(&mut self) {
        let stack: Vec<Screen> = self
            .nav
            .stack()
            .iter()
            .copied()
            .filter(|screen| !matches!(screen, Screen::Chat(_)))
            .collect();
        if stack.len() == self.nav.stack().len() {
            return;
        }
        self.stop_speech();
        if stack.is_empty() {
            self.nav.reset(Screen::Home(Flow::Standard));
        } else {
            self.nav.set(stack);
        }
    }

    /// The server answered all three questions: its profile and its chats
    /// are the app's. A chat already here keeps what is loaded of it; one
    /// the server no longer lists goes, unless its first message has not
    /// been answered yet.
    fn go_online(&mut self, voice: bool, profile: api::Profile, summaries: Vec<api::ChatSummary>) {
        let was_online = self.link.is_online();
        self.link = Link::Online { voice };
        self.mode = profile.mode;
        self.plan = Some(profile.plan);

        let shown = self.chats.get(self.current).map(|chat| chat.key);
        let locals = self.locals_sorted();
        let mut old = self.chats.split_off(locals);
        for summary in summaries.into_iter().rev() {
            let known = old
                .iter()
                .position(|chat| chat.id() == Some(summary.id.as_str()));
            let chat = match known {
                Some(index) => {
                    let mut chat = old.remove(index);
                    if !chat.busy() {
                        chat.title = summary.title;
                        let answered = chat
                            .turns
                            .iter()
                            .filter(|turn| matches!(turn.phase, Phase::Done | Phase::Working(_)))
                            .count();
                        let on_screen = shown == Some(chat.key);
                        if let Some(remote) = &mut chat.remote {
                            remote.count = summary.messages;
                            // Changed elsewhere: fetched again when opened.
                            if remote.loaded == Loaded::Full
                                && answered != summary.messages as usize
                                && !on_screen
                            {
                                remote.loaded = Loaded::Summary;
                            }
                        }
                    }
                    chat
                }
                None => Conversation {
                    title: summary.title,
                    turns: Vec::new(),
                    remote: Some(Remote {
                        id: Some(summary.id),
                        count: summary.messages,
                        loaded: Loaded::Summary,
                        load: 0,
                    }),
                    key: self.new_key(),
                },
            };
            self.chats.push(chat);
        }
        // Not on the server: kept only while it is still to be stored.
        for chat in old {
            if chat.id().is_none() && !chat.turns.is_empty() {
                self.chats.push(chat);
            }
        }
        match shown.and_then(|key| self.index_of(key)) {
            Some(index) => {
                self.current = index;
                // A scripted conversation is not continued against a server.
                if !was_online && !self.chats[index].is_remote() {
                    self.drop_chat_screens();
                }
            }
            None => {
                self.current = 0;
                self.drop_chat_screens();
            }
        }
    }

    /// The scripted conversations first, the server's after them; returns
    /// how many scripted ones there are.
    fn locals_sorted(&mut self) -> usize {
        self.chats.sort_by_key(Conversation::is_remote);
        self.locals()
    }

    /// Sends a change of the profile, if a server is there to take it.
    fn save(&mut self, patch: ProfilePatch) {
        if self.link.is_online() && self.ready() {
            self.http(self.client().put_profile(&patch), Route::Saved);
        }
    }

    /// Uses the address typed on the Server screen and asks that server
    /// how it is.
    fn use_typed_server(&mut self) {
        match api::normalize_base_url(&self.server_draft) {
            Err(problem) => self.server_problem = Some(problem.sentence().to_owned()),
            Ok(url) => {
                self.server_problem = None;
                self.server_draft.clone_from(&url);
                if url != self.server_url {
                    self.server_url.clone_from(&url);
                    self.asks.push(Ask::Store(SERVER_KEY, url));
                    // Another server's chats are not this one's.
                    self.abandon_voice();
                    self.drop_chat_screens();
                    self.chats.retain(|chat| !chat.is_remote());
                    self.current = 0;
                    self.deleting = None;
                }
                self.checked_by_hand = true;
                self.check_server();
            }
        }
    }

    // -------------------------------------------------- chats, online

    /// Sends `prompt` to the server: in the conversation on screen when
    /// `here`, otherwise as the first message of a new one.
    fn ask_server(&mut self, prompt: String, attachment: Option<String>, here: bool) {
        if !here {
            let key = self.new_key();
            self.chats.push(Conversation {
                title: provisional_title(&prompt),
                turns: Vec::new(),
                remote: Some(Remote {
                    id: None,
                    count: 0,
                    loaded: Loaded::Full,
                    load: 0,
                }),
                key,
            });
            self.current = self.chats.len() - 1;
        }
        let prompt = api::tidy_message(&prompt);
        let chat = self.current;
        self.chats[chat].turns.push(Turn::asked(prompt, attachment));
        let turn = self.chats[chat].turns.len() - 1;
        self.post(chat, turn);
        self.open_chat();
        self.show_latest();
    }

    /// Posts turn `turn` of conversation `chat`, for the first time or
    /// again. The model chosen now goes with it.
    fn post(&mut self, chat: usize, turn: usize) {
        let request = self.next_request();
        let mode = self.mode;
        let client = self.client();
        let conversation = &mut self.chats[chat];
        let key = conversation.key;
        let id = conversation.id().map(str::to_owned);
        let asked = &mut conversation.turns[turn];
        asked.phase = Phase::Thinking;
        asked.error = None;
        asked.open = false;
        asked.waiting = Some(request);
        asked.auto = mode == Mode::Auto;
        let message = client.message(
            id.as_deref(),
            &asked.prompt,
            mode,
            asked.attachment.as_deref(),
        );
        self.http(message, Route::Reply(key, request));
    }

    /// The server's answer to a turn: the chat takes its id and title, and
    /// the steps start.
    fn take_reply(&mut self, chat: usize, turn: usize, reply: api::Reply) {
        let conversation = &mut self.chats[chat];
        if let Some(remote) = &mut conversation.remote {
            remote.id.get_or_insert(reply.chat_id);
            remote.count = reply.turn;
        }
        conversation.title = reply.title;
        let asked = &mut conversation.turns[turn];
        asked.waiting = None;
        asked.model_used = asked.auto.then_some(reply.model_used);
        let response = Response {
            intro: String::new(),
            steps: reply
                .steps
                .into_iter()
                .map(|label| Step {
                    doing: label.clone(),
                    name: label,
                })
                .collect(),
            reply: reply.text,
        };
        self.answered(chat, turn, response);
    }

    /// Fetches the messages of a chat the Library listed.
    fn load_chat(&mut self, chat: usize) {
        let request = self.next_request();
        let client = self.client();
        let conversation = &mut self.chats[chat];
        let key = conversation.key;
        let Some(remote) = &mut conversation.remote else {
            return;
        };
        let Some(id) = remote.id.clone() else {
            return;
        };
        remote.loaded = Loaded::Loading;
        remote.load = request;
        self.http(client.chat(&id), Route::Chat(key, request));
    }

    /// Takes conversation `index` out of the list. If it is the one on
    /// screen, its screen goes too.
    fn remove_chat(&mut self, index: usize) {
        let shown = index == self.current;
        self.chats.remove(index);
        if shown {
            self.current = 0;
            self.drop_chat_screens();
        } else if self.current > index {
            self.current -= 1;
        }
    }

    /// "Delete this chat": asked twice, then deleted on the server.
    fn delete_chat(&mut self) {
        let chat = self.current;
        if !matches!(self.screen(), Screen::Chat(_)) || !self.chats[chat].is_remote() {
            return;
        }
        if self.deleting.is_some() {
            return;
        }
        if self.chats[chat].busy() {
            self.toast("Wait for the answer, or cancel it, before deleting.");
            return;
        }
        if !self.confirm_delete {
            self.confirm_delete = true;
            self.asks.push(Ask::Announce(
                "Tap again to delete this chat for good.".to_owned(),
            ));
            return;
        }
        self.confirm_delete = false;
        match self.chats[chat].id().map(str::to_owned) {
            Some(id) => {
                let key = self.chats[chat].key;
                self.deleting = Some(key);
                self.http(self.client().delete_chat(&id), Route::Deleted(key));
            }
            None => {
                // Never stored: there is nothing on the server to delete.
                self.remove_chat(chat);
                self.toast("Chat deleted.");
            }
        }
    }

    // ---------------------------------------------- live voice, for real

    /// Stops what the speaker is doing with a spoken answer.
    fn stop_speech(&mut self) {
        if self.speech == Speech::Playing {
            self.asks.push(Ask::StopPlayback);
        }
        if self.speech != Speech::Silent {
            // Whatever is still on its way belongs to nobody now.
            self.speech_no += 1;
        }
        self.speech = Speech::Silent;
        self.speech_clip = None;
        self.speech_at = None;
    }

    fn play(&mut self, clip: AudioClip) {
        self.speech = Speech::Playing;
        self.asks.push(Ask::Play(clip, self.speech_no));
    }

    /// Opens live voice: recording when the server has a voice, the demo
    /// when it has not.
    fn open_voice(&mut self) {
        self.stop_speech();
        self.voice_no += 1;
        self.voice = Voice {
            words: 0,
            since: 0,
            muted: false,
            live: Listen::Demo,
        };
        self.voice_kept = None;
        self.voice_heard = None;
        self.voice_error = None;
        self.voice_reply = None;
        let here = matches!(self.screen(), Screen::Chat(_));
        let chat = &self.chats[self.current];
        // From a chat of the server's, the message joins that chat.
        self.voice_target = (here && chat.is_remote()).then_some(chat.key);
        let free = !here || (!chat.busy() && *chat.loaded() == Loaded::Full);
        let scripted_chat = here && !chat.is_remote();
        if self.link.has_voice() && free && !scripted_chat {
            self.start_recording();
        }
        self.nav.push(Screen::LiveVoice);
    }

    fn start_recording(&mut self) {
        self.voice.live = Listen::Starting;
        self.voice.since = 0;
        self.voice_error = None;
        self.voice_kept = None;
        self.asks.push(Ask::StartRecording(self.voice_no));
    }

    fn stop_recording(&mut self) {
        self.voice.live = Listen::Stopping;
        self.asks.push(Ask::StopRecording(self.voice_no));
    }

    fn voice_failed(&mut self, why: impl Into<String>) {
        let why = why.into();
        self.voice.live = Listen::Failed;
        self.asks.push(Ask::Announce(why.clone()));
        self.voice_error = Some(why);
    }

    /// Posts the recording: to the chat voice was opened from, or as the
    /// first message of a new one. The answer is asked for spoken.
    fn send_voice(&mut self, recording: Recording) {
        let request = self.next_request();
        let id = self
            .voice_target
            .and_then(|key| self.index_of(key))
            .and_then(|index| self.chats[index].id().map(str::to_owned));
        let message = self
            .client()
            .spoken_message(id.as_deref(), &recording, self.mode);
        self.voice_kept = Some(recording);
        self.voice_wait = request;
        self.voice_error = None;
        self.voice.live = Listen::Sending;
        self.http(message, Route::Spoken(self.voice_no, request));
    }

    /// Live voice is left with nothing sent: the microphone stops and
    /// whatever is still on its way is no longer waited for.
    fn abandon_voice(&mut self) {
        if matches!(self.voice.live, Listen::Starting | Listen::Recording) {
            // Session 0 is nobody's: the recording is thrown away.
            self.asks.push(Ask::StopRecording(0));
        }
        if self.voice_reply.take().is_some() {
            // Its audio was being fetched for a chat that will not open.
            self.stop_speech();
        }
        self.voice_no += 1;
        self.voice.live = Listen::Demo;
        self.voice_kept = None;
        self.voice_heard = None;
        self.voice_error = None;
    }

    /// The transcript has been shown: the chat opens with the message in
    /// it and its answer starting, and the answer is spoken.
    fn deliver_voice(&mut self) {
        let Some(reply) = self.voice_reply.take() else {
            return;
        };
        self.voice.live = Listen::Demo;
        self.voice_no += 1;
        self.voice_kept = None;
        self.nav.pop_to(|screen| *screen != Screen::LiveVoice);
        let chat = match self.voice_target.and_then(|key| self.index_of(key)) {
            Some(index) => index,
            None => {
                let key = self.new_key();
                self.chats.push(Conversation {
                    title: reply.title.clone(),
                    turns: Vec::new(),
                    remote: Some(Remote {
                        id: None,
                        count: 0,
                        loaded: Loaded::Full,
                        load: 0,
                    }),
                    key,
                });
                // A chat that was on screen is not the one this joins.
                self.nav.pop_to(|screen| matches!(screen, Screen::Home(_)));
                self.chats.len() - 1
            }
        };
        self.current = chat;
        let mut asked = Turn::asked(reply.heard.clone(), None);
        asked.auto = self.mode == Mode::Auto;
        self.chats[chat].turns.push(asked);
        let turn = self.chats[chat].turns.len() - 1;
        self.take_reply(chat, turn, reply);
        if self.speech != Speech::Silent {
            self.speech_at = Some((self.chats[chat].key, turn));
            if let Some(clip) = self.speech_clip.take() {
                self.play(clip);
            }
        }
        self.open_chat();
        self.show_latest();
    }

    /// An answer from the platform.
    fn net(&mut self, net: Net) {
        match net {
            Net::Health(sync, wire) => {
                if sync != self.sync {
                    return;
                }
                match api::decode_health(wire) {
                    Ok(health) if health.ok => {
                        // Not online yet: the profile and the chats first.
                        self.sync_voice = health.voice;
                        self.http(self.client().profile(), Route::Profile(sync));
                    }
                    Ok(health) => {
                        self.checked_by_hand = false;
                        self.link = Link::Offline(
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
            Net::Profile(sync, wire) => {
                if sync != self.sync {
                    return;
                }
                match api::decode_profile(wire) {
                    Ok(profile) => {
                        self.sync_profile = Some(profile);
                        self.http(self.client().chats(), Route::Chats(sync));
                    }
                    Err(failure) => self.offline(&failure),
                }
            }
            Net::Chats(sync, wire) => {
                if sync != self.sync {
                    return;
                }
                let Some(profile) = self.sync_profile.take() else {
                    return;
                };
                match api::decode_chats(wire) {
                    Ok(chats) => {
                        self.checked_by_hand = false;
                        self.go_online(self.sync_voice, profile, chats);
                    }
                    Err(failure) => self.offline(&failure),
                }
            }
            Net::Saved(wire) => {
                // The choice already shows; only a failure is news.
                if let Err(failure) = api::decode_profile(wire) {
                    self.toast(&format!("Not saved on the server. {}", failure.sentence()));
                    self.note_failure(&failure);
                }
            }
            Net::Reply(key, request, wire) => {
                let Some(chat) = self.index_of(key) else {
                    return;
                };
                // Cancelled, retried or gone: nobody waits for this one.
                let Some(turn) = self.chats[chat]
                    .turns
                    .iter()
                    .position(|turn| turn.waiting == Some(request))
                else {
                    return;
                };
                match api::decode_reply(wire) {
                    Ok(reply) => {
                        self.take_reply(chat, turn, reply);
                        if matches!(self.link, Link::Offline(_)) {
                            // It answers again: find out what else it has.
                            self.check_server();
                        }
                    }
                    Err(failure) => {
                        let conversation = &mut self.chats[chat];
                        if failure.status() == Some(404)
                            && let Some(remote) = &mut conversation.remote
                        {
                            // The chat is gone from the server: sending
                            // again starts a new one.
                            remote.id = None;
                        }
                        let asked = &mut conversation.turns[turn];
                        asked.waiting = None;
                        asked.phase = Phase::Failed;
                        asked.error = Some(failure.sentence());
                        self.asks.push(Ask::Announce(failure.sentence()));
                        self.note_failure(&failure);
                    }
                }
            }
            Net::Chat(key, request, wire) => {
                let Some(chat) = self.index_of(key) else {
                    return;
                };
                let waited = self.chats[chat].remote.as_ref().is_some_and(|remote| {
                    remote.loaded == Loaded::Loading && remote.load == request
                });
                if !waited {
                    return;
                }
                match api::decode_chat(wire) {
                    Ok(stored) => {
                        let conversation = &mut self.chats[chat];
                        conversation.title = stored.title;
                        conversation.turns = stored
                            .exchanges
                            .into_iter()
                            .map(|exchange| Turn {
                                steps: exchange
                                    .steps
                                    .into_iter()
                                    .map(|label| Step {
                                        doing: label.clone(),
                                        name: label,
                                    })
                                    .collect(),
                                phase: Phase::Done,
                                reply: Some(exchange.text),
                                ..Turn::asked(exchange.prompt, exchange.attachment)
                            })
                            .collect();
                        let count = conversation.turns.len() as u32;
                        if let Some(remote) = &mut conversation.remote {
                            remote.loaded = Loaded::Full;
                            remote.count = count;
                        }
                        if chat == self.current && matches!(self.screen(), Screen::Chat(_)) {
                            self.asks.push(Ask::Scroll(ScrollTo::end(
                                Screen::Chat(Flow::Standard).scroll_id(),
                            )));
                        }
                    }
                    Err(failure) if failure.status() == Some(404) => {
                        self.remove_chat(chat);
                        self.toast("That chat is no longer on the server.");
                    }
                    Err(failure) => {
                        if let Some(remote) = &mut self.chats[chat].remote {
                            remote.loaded = Loaded::Failed(failure.sentence());
                        }
                        self.note_failure(&failure);
                    }
                }
            }
            Net::Deleted(key, wire) => {
                if self.deleting != Some(key) {
                    return;
                }
                self.deleting = None;
                match api::decode_deleted(wire) {
                    Ok(()) => {
                        if let Some(chat) = self.index_of(key) {
                            self.remove_chat(chat);
                        }
                        self.toast("Chat deleted.");
                    }
                    Err(failure) => {
                        self.toast(&format!("Not deleted. {}", failure.sentence()));
                        self.note_failure(&failure);
                    }
                }
            }
            Net::RecordingStarted(session, result) => {
                if session != self.voice_no || self.voice.live != Listen::Starting {
                    return;
                }
                match result {
                    Ok(()) => {
                        self.voice.live = Listen::Recording;
                        self.voice.since = 0;
                    }
                    Err(error) => {
                        // No microphone: the demo, and why.
                        self.voice.live = Listen::Demo;
                        self.voice.words = 0;
                        self.voice.since = 0;
                        self.toast(if error == AudioError::PermissionDenied {
                            "The microphone is not allowed, so this is the demo. Allow it in the phone\u{2019}s settings to talk to Atlas."
                        } else {
                            "The microphone could not start, so this is the demo."
                        });
                    }
                }
            }
            Net::Recorded(session, result) => {
                if session != self.voice_no || self.voice.live != Listen::Stopping {
                    return;
                }
                match result {
                    Ok(recording)
                        if recording.bytes.is_empty()
                            || recording.duration < SHORTEST_RECORDING =>
                    {
                        self.voice_failed(
                            "That was too short to hear. Tap the microphone and try again.",
                        );
                    }
                    Ok(recording) => self.send_voice(recording),
                    Err(error) => {
                        self.voice_failed(format!("The recording did not work: {error}."));
                    }
                }
            }
            Net::Spoken(session, request, wire) => {
                if session != self.voice_no
                    || request != self.voice_wait
                    || self.voice.live != Listen::Sending
                {
                    return;
                }
                match api::decode_reply(wire) {
                    Ok(reply) => {
                        self.voice_kept = None;
                        self.voice_heard = Some(reply.heard.clone());
                        self.asks.push(Ask::Announce(reply.heard.clone()));
                        if let Some(path) = &reply.audio {
                            // Fetched while the transcript shows.
                            self.stop_speech();
                            self.speech_no += 1;
                            self.speech = Speech::Fetching;
                            self.http(self.client().audio(path), Route::Audio(self.speech_no));
                        }
                        self.voice_reply = Some(reply);
                        self.voice.live = Listen::Heard;
                        self.voice.since = 0;
                        if matches!(self.link, Link::Offline(_)) {
                            // It answers again: find out what else it has.
                            self.check_server();
                        }
                    }
                    Err(failure) => {
                        if failure.status() == Some(404) {
                            // The chat is gone: the recording starts a new one.
                            self.voice_target = None;
                        }
                        self.voice_failed(failure.sentence());
                        self.note_failure(&failure);
                    }
                }
            }
            Net::Audio(speech, wire) => {
                if speech != self.speech_no || self.speech != Speech::Fetching {
                    return;
                }
                match api::decode_audio(wire) {
                    Ok((bytes, mime)) => {
                        let clip = AudioClip::new(bytes, mime);
                        if self.speech_at.is_some() {
                            self.play(clip);
                        } else {
                            self.speech_clip = Some(clip);
                        }
                    }
                    // The answer is on the screen all the same.
                    Err(_) => self.stop_speech(),
                }
            }
            Net::Played(speech, _) => {
                if speech == self.speech_no && self.speech == Speech::Playing {
                    self.speech = Speech::Silent;
                    self.speech_at = None;
                }
            }
        }
    }

    /// After a change of screen: a spoken answer stops when its chat is
    /// left, and a server that failed is asked again on the way back to
    /// the screens that depend on it.
    fn arrived(&mut self, from: Screen) {
        let to = self.screen();
        if to == from {
            return;
        }
        if matches!(from, Screen::Chat(_)) && to != Screen::LiveVoice {
            self.stop_speech();
        }
        if matches!(to, Screen::Home(_) | Screen::Library) && matches!(self.link, Link::Offline(_))
        {
            self.check_server();
        }
    }

    fn show_tab(&mut self, tab: Tab) {
        if self.screen().tab() == Some(tab) {
            return;
        }
        // From 09, the Home tab is 09 itself; from anywhere else the
        // design's Home link is 03.
        self.nav.reset(match tab {
            Tab::Home => Screen::Home(Flow::Standard),
            Tab::Explore => Screen::Explore,
            Tab::Library => Screen::Library,
        });
    }

    fn sign_in(&mut self) {
        let email = self.email.trim();
        let plausible = email.split_once('@').is_some_and(|(name, host)| {
            !name.is_empty() && host.contains('.') && !host.ends_with('.')
        });
        self.sign_in_error = if !plausible {
            Some("Enter an email address like you@example.com.")
        } else if self.password.chars().count() < 6 {
            Some("Your password has at least 6 characters.")
        } else {
            None
        };
        if self.sign_in_error.is_none() {
            self.password.clear();
            self.nav.reset(Screen::Home(Flow::Standard));
        }
    }

    fn handle(&mut self, message: Message) {
        match message {
            Message::GetStarted => self.nav.push(Screen::SignIn),
            Message::Email(email) => {
                self.email = email;
                self.sign_in_error = None;
            }
            Message::Password(password) => {
                self.password = password;
                self.sign_in_error = None;
            }
            Message::SignIn => self.sign_in(),
            Message::Google => {
                // No account system in this example: the design's link.
                self.nav.reset(Screen::Home(Flow::Standard));
            }
            Message::ForgotPassword => {
                self.toast("Password reset is not part of this demo.");
            }
            Message::CreateAccount => {
                self.toast("Any email and a 6-character password sign you in.");
            }
            Message::OpenPlans => {
                if self.screen() != Screen::Plans {
                    self.nav.push(Screen::Plans);
                }
            }
            Message::OpenPicker => self.picker = true,
            Message::ClosePicker => self.picker = false,
            Message::SelectMode(mode) => {
                if mode != self.mode {
                    self.mode = mode;
                    self.save(ProfilePatch {
                        mode: Some(mode),
                        plan: None,
                    });
                }
                self.picker = false;
            }
            Message::AllModels => {
                self.picker = false;
                if self.screen() != Screen::Models {
                    self.nav.push(Screen::Models);
                }
            }
            Message::ToggleProvider(index) => {
                // Atlas's own models cannot be switched off.
                if let Some(on) = self.providers.get_mut(index).filter(|_| index > 0) {
                    *on = !*on;
                } else {
                    self.toast("Atlas\u{2019}s own models are always on.");
                }
            }
            Message::Tab(tab) => self.show_tab(tab),
            Message::Back => {
                self.go_back();
            }
            Message::Draft(draft) => self.draft = draft,
            Message::Send => {
                let here = matches!(self.screen(), Screen::Chat(_));
                let chat = &self.chats[self.current];
                if here && chat.is_remote() && !self.draft.trim().is_empty() {
                    // One message of a chat at a time; what was typed stays.
                    if chat.busy() {
                        self.toast("Atlas is still answering your last message.");
                        return;
                    }
                    if *chat.loaded() != Loaded::Full {
                        self.toast("This chat has not loaded yet.");
                        return;
                    }
                }
                let draft = std::mem::take(&mut self.draft);
                if draft.trim().is_empty() {
                    self.draft = draft;
                } else {
                    let attachment = self.attachment.take();
                    self.ask(draft, attachment);
                }
            }
            Message::Attach => {
                // No file picker in this example: the design's sample.
                self.attachment = Some(SAMPLE_ATTACHMENT.to_owned());
            }
            Message::Detach => self.attachment = None,
            Message::OpenVoice => self.open_voice(),
            Message::Suggest(index) => {
                if let Some((_, _, prompt)) = SUGGESTIONS.get(index) {
                    self.ask((*prompt).to_owned(), None);
                }
            }
            Message::ToggleSteps(index) => {
                if let Some(turn) = self.chats[self.current].turns.get_mut(index) {
                    turn.open = !turn.open;
                }
            }
            Message::CancelTask(index) => {
                let current = self.current;
                if let Some(turn) = self.chats[current].turns.get_mut(index)
                    && matches!(turn.phase, Phase::Thinking | Phase::Working(_))
                {
                    // A request cannot be recalled; its answer, if one
                    // still comes, finds nobody waiting.
                    turn.waiting = None;
                    turn.phase = Phase::Cancelled;
                    turn.open = false;
                    if self
                        .pending
                        .as_ref()
                        .is_some_and(|(chat, turn, ..)| (*chat, *turn) == (current, index))
                    {
                        self.pending = None;
                    }
                }
            }
            Message::AttachmentInfo => {
                self.toast("Attachments are a sample: no file leaves this phone.");
            }
            Message::VoiceCamera => self.toast("The camera is not part of this demo."),
            Message::VoiceMute => match self.voice.live {
                Listen::Demo => {
                    self.voice.muted = !self.voice.muted;
                    self.voice.since = 0;
                }
                // The microphone button sends what was said, and starts
                // again after a failure.
                Listen::Recording => self.stop_recording(),
                Listen::Failed => self.start_recording(),
                Listen::Starting | Listen::Stopping | Listen::Sending | Listen::Heard => {}
            },
            Message::VoiceEnd => self.end_voice(),
            Message::VoiceRetry => {
                if self.voice.live == Listen::Failed
                    && let Some(recording) = self.voice_kept.take()
                {
                    self.send_voice(recording);
                }
            }
            Message::Query(query) => self.query = query,
            Message::Filter(category) => self.category = category,
            Message::Tool(index) => {
                if let Some(tool) = TOOLS.get(index) {
                    self.ask(tool.prompt.to_owned(), None);
                }
            }
            Message::Choose(plan) => {
                if self.plan != Some(plan) {
                    self.save(ProfilePatch {
                        mode: None,
                        plan: Some(plan),
                    });
                }
                self.plan = Some(plan);
                self.toast(&format!(
                    "You\u{2019}re on {} (a demo: nothing was charged).",
                    plan.name()
                ));
            }
            Message::OpenChat(index) => {
                if index < self.chats.len() {
                    self.current = index;
                    self.nav.push(Screen::Chat(Flow::Standard));
                    self.asks.push(Ask::Scroll(ScrollTo::end(
                        Screen::Chat(Flow::Standard).scroll_id(),
                    )));
                    if matches!(
                        self.chats[index].loaded(),
                        Loaded::Summary | Loaded::Failed(_)
                    ) {
                        self.load_chat(index);
                    }
                }
            }
            Message::NewChat => self.nav.reset(Screen::Home(Flow::Standard)),
            Message::Answered(chat, turn, response) => self.answered(chat, turn, response),
            Message::Retry(index) => {
                let chat = self.current;
                if self.chats[chat]
                    .turns
                    .get(index)
                    .is_some_and(|turn| turn.phase == Phase::Failed)
                    && !self.chats[chat].busy()
                {
                    self.post(chat, index);
                }
            }
            Message::Reload => {
                if matches!(self.chats[self.current].loaded(), Loaded::Failed(_)) {
                    self.load_chat(self.current);
                }
            }
            Message::DeleteChat => self.delete_chat(),
            Message::StopSpeaking => self.stop_speech(),
            Message::OpenServer => {
                self.picker = false;
                if self.screen() != Screen::Server {
                    self.server_draft.clone_from(&self.server_url);
                    self.server_problem = None;
                    self.nav.push(Screen::Server);
                }
            }
            Message::ServerUrl(url) => {
                self.server_draft = url.chars().take(URL_LIMIT).collect();
                self.server_problem = None;
            }
            Message::CheckServer => self.use_typed_server(),
            Message::DefaultServer => {
                self.server_draft = api::DEFAULT_BASE_URL.to_owned();
                self.use_typed_server();
            }
            Message::Net(net) => self.net(net),
            Message::Tick => self.tick(),
        }
    }

    /// Back, from the system or from an arrow. False on a root screen:
    /// the system then leaves the app.
    pub fn go_back(&mut self) -> bool {
        if self.picker {
            self.picker = false;
            return true;
        }
        if self.screen() == Screen::LiveVoice {
            // Nothing is sent: the microphone stops, a reply is not waited for.
            self.abandon_voice();
        }
        if self.nav.back() {
            return true;
        }
        // Opened directly on a pushed screen (a design page): its arrow
        // leads where the design's link does.
        match self.screen() {
            Screen::Chat(flow) => self.nav.reset(Screen::Home(flow)),
            Screen::SignIn => self.nav.reset(Screen::Onboarding),
            Screen::Plans
            | Screen::LiveVoice
            | Screen::PickerBackdrop
            | Screen::Models
            | Screen::Server => {
                self.nav.reset(Screen::Home(Flow::Standard));
            }
            _ => return false,
        }
        true
    }
}

impl Application for Atlas {
    type Message = Message;

    fn setup(&mut self, setup: &mut Setup<'_>) {
        setup.add_font(include_bytes!(
            "../assets/fonts/PlusJakartaSans-Regular.ttf"
        ));
        setup.add_font(include_bytes!("../assets/fonts/PlusJakartaSans-Medium.ttf"));
        setup.add_font(include_bytes!(
            "../assets/fonts/PlusJakartaSans-SemiBold.ttf"
        ));
        setup.add_font(include_bytes!("../assets/fonts/PlusJakartaSans-Bold.ttf"));
        // Once: who this installation is, where its server is, and the
        // first question to it. (A tool may set an app up twice.) With
        // nothing behind the app the question fails at once as
        // "unsupported", and the app stays the scripted one.
        if !self.ready() {
            self.device_id = api::device_id(self.device_prefix, setup.install_id());
            if let Some(url) = setup
                .stored(SERVER_KEY)
                .and_then(|stored| api::normalize_base_url(stored).ok())
            {
                self.server_draft.clone_from(&url);
                self.server_url = url;
            }
            self.check_server();
        }
    }

    fn theme(&self) -> Theme {
        theme::THEME
    }

    fn back(&mut self) -> bool {
        self.still = false;
        self.confirm_delete = false;
        let from = self.screen();
        let handled = self.go_back();
        self.arrived(from);
        handled
    }

    fn tick(&self) -> Option<(Duration, Message)> {
        let moving = self.notice.is_some()
            || self.answering()
            || self.listening()
            || self.voice_busy()
            || self.loading();
        (moving && !self.still).then_some((
            Duration::from_nanos(1_000_000_000 / u64::from(TICKS_PER_SECOND)),
            Message::Tick,
        ))
    }

    fn system_bars(&self) -> Option<SystemBars> {
        // Every screen is light, and so is the page under the picker's
        // scrim (a mid grey on which dark icons read better than light
        // ones: 8:1 against 2.6:1).
        Some(SystemBars::DARK_CONTENT)
    }

    fn view_in(&self, viewport: &Viewport<'_>) -> Element<Message> {
        let room = viewport.safe_size().height;
        let screen = self.screen();
        let content: Element<Message> = match screen {
            Screen::Onboarding => screens::onboarding::view(room),
            Screen::SignIn => screens::sign_in::view(self, room),
            Screen::Home(_) => screens::home::view(self, room),
            Screen::Chat(_) => screens::chat::view(self),
            Screen::PickerBackdrop => screens::model_picker::backdrop(),
            Screen::LiveVoice => screens::live_voice::view(self, room),
            Screen::Explore => screens::explore::view(self),
            Screen::Plans => screens::plans::view(self),
            Screen::Library => screens::library::view(self),
            Screen::Models => screens::models::view(self),
            Screen::Server => screens::server::view(self),
        };
        let content = content.transition(self.nav.transition());
        Stack::new()
            .push(content)
            .push_maybe(self.picker.then(screens::model_picker::scrim))
            .push_maybe(self.picker.then(|| screens::model_picker::sheet(self.mode)))
            .push_maybe(self.notice().map(|text| components::toast(text.to_owned())))
            .fill()
            .into()
    }

    fn update(&mut self, message: Message) {
        // The timer and the platform's answers are not touches: a still
        // frame stays still (and a design page stays the design's picture
        // when its first question to a server that is not there fails).
        let touch = !matches!(message, Message::Tick | Message::Net(_));
        if touch {
            self.still = false;
            if message != Message::DeleteChat {
                self.confirm_delete = false;
            }
        }
        let from = self.screen();
        self.handle(message);
        self.arrived(from);
    }

    fn effects(&mut self, out: &mut Effects<Message>) {
        for ask in self.asks.drain(..) {
            match ask {
                Ask::Scroll(to) => out.scroll(to),
                Ask::Announce(text) => out.announce(text),
                Ask::Http(request, route) => out.http(request, move |wire| route.message(wire)),
                Ask::Play(clip, speech) => {
                    out.play(clip, move |result| {
                        Message::Net(Net::Played(speech, result))
                    });
                }
                Ask::StopPlayback => out.stop_playback(),
                Ask::StartRecording(session) => out.start_recording(move |result| {
                    Message::Net(Net::RecordingStarted(session, result))
                }),
                Ask::StopRecording(session) => {
                    out.stop_recording(move |result| Message::Net(Net::Recorded(session, result)))
                }
                Ask::Store(key, value) => out.store(key, value),
            }
        }
    }
}

morningstar::app!(Ui::new(Atlas::default()));
