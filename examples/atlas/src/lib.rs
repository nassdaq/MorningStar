//! Atlas: an AI-assistant app, reproduced from a design and working end to
//! end without a backend.
//!
//! The ten screens of `design/` are in [`screens`], one module each, with
//! the shared pieces in [`components`], the tokens in [`theme`] and the
//! icons in [`icons`]. This file is the application: the state, what each
//! tap does to it, and the timer that plays an answer out. `SCREENS.md`
//! has the table of screens, how close each is to its picture, and what is
//! the app's own design.
//!
//! There is no network in this example. The chat's answers come from
//! [`assistant::respond`], the one function a real model would replace.

pub mod assistant;
pub mod components;
pub mod icons;
pub mod screens;
pub mod theme;

use std::time::Duration;

use morningstar::ui::prelude::*;

use assistant::{Request, Response, Step};

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
}

impl Screen {
    /// Every screen, for tests that visit them all.
    pub const ALL: [Self; 12] = [
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
}

/// A conversation: what the chat screen shows and the Library lists.
#[derive(Clone, Debug, PartialEq)]
pub struct Conversation {
    /// The chat's title.
    pub title: String,
    /// Its exchanges, oldest first.
    pub turns: Vec<Turn>,
}

impl Conversation {
    /// The design's conversation, as page 04 shows it: three steps in.
    pub fn sample() -> Self {
        Self {
            title: "Compare supplier quotes".to_owned(),
            turns: vec![Turn {
                prompt: QUOTE_PROMPT.to_owned(),
                attachment: Some(SAMPLE_ATTACHMENT.to_owned()),
                intro: Some(assistant::QUOTE_INTRO.to_owned()),
                steps: assistant::quote_steps(),
                phase: Phase::Working(2),
                open: false,
                reply: None,
            }],
        }
    }
}

/// Live voice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Voice {
    /// How many words of [`VOICE_LINE`] have been "heard".
    pub words: usize,
    /// Ticks since the last word.
    pub since: u32,
    /// The microphone button's state.
    pub muted: bool,
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
    /// The model's answer to turn `.1` of conversation `.0`: where a real
    /// backend's reply would arrive too.
    Answered(usize, usize, Response),
    /// The timer.
    Tick,
}

/// What `update` has decided the platform should do; `effects` hands it
/// over.
enum Ask {
    Scroll(ScrollTo),
    Announce(String),
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
            },
            query: String::new(),
            category: Category::All,
            plan: None,
            providers: [true, false, false],
            notice: None,
            clock: 0,
            still: false,
            asks: Vec::new(),
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

    /// The conversations, the newest last.
    pub fn chats(&self) -> &[Conversation] {
        &self.chats
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
        if self.still || self.voice.muted {
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

    fn listening(&self) -> bool {
        self.screen() == Screen::LiveVoice && !self.voice.muted
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
        if !matches!(self.screen(), Screen::Chat(_)) {
            let mut title: String = prompt.chars().take(28).collect();
            if title.len() < prompt.len() {
                title.push('\u{2026}');
            }
            self.chats.push(Conversation {
                title,
                turns: Vec::new(),
            });
            self.current = self.chats.len() - 1;
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
        self.chats[self.current].turns.push(Turn {
            prompt,
            attachment,
            intro: None,
            steps: Vec::new(),
            phase: Phase::Thinking,
            open: false,
            reply: None,
        });
        let turn = self.chats[self.current].turns.len() - 1;
        // The boundary: a real model's answer would come back from the
        // network as `Message::Answered`; the script's comes back from
        // the timer, a moment later, the same way.
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
        turn.intro = Some(response.intro);
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

    /// Live voice ends: what it heard is sent to the chat.
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
                self.mode = mode;
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
            Message::OpenVoice => {
                self.voice = Voice {
                    words: 0,
                    since: 0,
                    muted: false,
                };
                self.nav.push(Screen::LiveVoice);
            }
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
            Message::VoiceMute => {
                self.voice.muted = !self.voice.muted;
                self.voice.since = 0;
            }
            Message::VoiceEnd => self.finish_voice(),
            Message::Query(query) => self.query = query,
            Message::Filter(category) => self.category = category,
            Message::Tool(index) => {
                if let Some(tool) = TOOLS.get(index) {
                    self.ask(tool.prompt.to_owned(), None);
                }
            }
            Message::Choose(plan) => {
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
                }
            }
            Message::NewChat => self.nav.reset(Screen::Home(Flow::Standard)),
            Message::Answered(chat, turn, response) => self.answered(chat, turn, response),
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
        if self.nav.back() {
            return true;
        }
        // Opened directly on a pushed screen (a design page): its arrow
        // leads where the design's link does.
        match self.screen() {
            Screen::Chat(flow) => self.nav.reset(Screen::Home(flow)),
            Screen::SignIn => self.nav.reset(Screen::Onboarding),
            Screen::Plans | Screen::LiveVoice | Screen::PickerBackdrop | Screen::Models => {
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
    }

    fn theme(&self) -> Theme {
        theme::THEME
    }

    fn back(&mut self) -> bool {
        self.still = false;
        self.go_back()
    }

    fn tick(&self) -> Option<(Duration, Message)> {
        let moving = self.notice.is_some() || self.answering() || self.listening();
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
        if message != Message::Tick {
            self.still = false;
        }
        self.handle(message);
    }

    fn effects(&mut self, out: &mut Effects<Message>) {
        for ask in self.asks.drain(..) {
            match ask {
                Ask::Scroll(to) => out.scroll(to),
                Ask::Announce(text) => out.announce(text),
            }
        }
    }
}

morningstar::app!(Ui::new(Atlas::default()));
