//! Atlas end to end, driven the way a phone drives it.
//!
//! Every step here is a pointer event in pixels at a position taken from
//! the last frame's layout (`Ui::rect_of`), a text event from the
//! keyboard, the system's Back (`Ui::back`) or time passing
//! (`FrameInfo::elapsed`, which is what delivers the timer's ticks), with
//! a frame after each, as the engine does. Nothing is sent to the
//! application directly: if a control cannot be reached by a finger, or
//! does nothing, a test here fails.
//!
//! Two frames are used throughout: the owner's Galaxy A27 (1080 x 2340 px
//! at 2.8125 px per dp, with its status and gesture bars as insets), where
//! the design's 844 dp do not fit, and a small 360 x 640 dp phone.

use std::collections::BTreeSet;
use std::hash::Hash;
use std::path::Path;
use std::time::Duration;

use atlas::{
    Atlas, Category, Flow, Mode, Phase, Plan, STEP_TICKS, Screen, THINKING_TICKS, Tab, VOICE_LINE,
};
use morningstar::ui::Ui;
use morningstar::{
    App, Frame, FrameInfo, ImeAction, Insets, KeyboardType, Point, PointerEvent, PointerPhase,
    Rect, SurfaceSize, TextInputEvent,
};

// ---------------------------------------------------------------- frames

/// The owner's phone: 384 x 832 dp, 36 dp of status bar and 15 dp of
/// gesture bar.
fn galaxy_a27() -> FrameInfo {
    FrameInfo::new(SurfaceSize::new(1080, 2340))
        .with_scale(2.8125)
        .with_insets(Insets::new(0.0, 102.0, 0.0, 42.0))
}

/// A small phone: 360 x 640 dp.
fn small() -> FrameInfo {
    FrameInfo::new(SurfaceSize::new(720, 1280)).with_scale(2.0)
}

/// The design's own frame.
fn design() -> FrameInfo {
    FrameInfo::new(SurfaceSize::new(780, 1688)).with_scale(2.0)
}

/// A tall phone: 411 x 914 dp.
fn tall() -> FrameInfo {
    FrameInfo::new(SurfaceSize::new(1080, 2400)).with_scale(2.625)
}

fn phones() -> [(&'static str, FrameInfo); 2] {
    [("Galaxy A27", galaxy_a27()), ("360 x 640", small())]
}

const HOME: Screen = Screen::Home(Flow::Standard);
const CHAT: Screen = Screen::Chat(Flow::Standard);

// ---------------------------------------------------------------- device

/// A finger must travel this far (dp) before a press becomes a scroll.
const TOUCH_SLOP: f32 = 8.0;
/// One tick of the app's timer, and a little.
const TICK: Duration = Duration::from_millis(34);

struct Device {
    ui: Ui<Atlas>,
    frame: Frame,
    info: FrameInfo,
    elapsed: Duration,
}

impl Device {
    fn on(screen: Screen, info: FrameInfo) -> Self {
        Self::with(Atlas::on(screen), info)
    }

    fn with(app: Atlas, info: FrameInfo) -> Self {
        let mut ui = Ui::new(app);
        let frame = Frame::for_app(&mut ui);
        let mut device = Self {
            ui,
            frame,
            info,
            elapsed: Duration::ZERO,
        };
        device.draw();
        device
    }

    fn draw(&mut self) {
        self.frame.begin();
        let info = self.info.with_elapsed(self.elapsed);
        self.ui.frame(&info, &mut self.frame);
    }

    fn app(&self) -> &Atlas {
        self.ui.app()
    }

    fn screen(&self) -> Screen {
        self.app().screen()
    }

    fn scale(&self) -> f32 {
        self.info.scale
    }

    /// Everything the application remembers and shows, and which field
    /// has the keyboard: to tell whether a tap did anything.
    fn snapshot(&self) -> String {
        let app = self.app();
        let focus =
            ["email", "password", "search", "draft", "server-url"].map(|id| self.ui.is_focused(id));
        format!(
            "{:?}",
            (
                app.stack(),
                (app.picker_open(), app.mode(), app.plan(), app.providers()),
                app.chats(),
                (app.draft(), app.attachment(), app.email(), app.password()),
                (
                    app.voice().muted,
                    app.filter(),
                    app.sign_in_error(),
                    app.notice()
                ),
                (app.server_draft(), app.server_problem()),
                focus,
            )
        )
    }

    fn rect(&self, id: impl Hash) -> Rect {
        self.ui
            .rect_of(id)
            .unwrap_or_else(|| panic!("{}: the widget is not on the screen", self.screen().name()))
    }

    fn pointer(&mut self, phase: PointerPhase, position: Point) {
        self.ui.pointer(&PointerEvent {
            id: 1,
            phase,
            position,
        });
        self.draw();
    }

    /// Lets whatever slides or fades finish.
    fn settle(&mut self) {
        for _ in 0..2 {
            self.elapsed += Duration::from_millis(400);
            self.draw();
        }
    }

    /// Lets `count` ticks of the app's timer pass, a frame each.
    fn ticks(&mut self, count: u32) {
        for _ in 0..count {
            self.elapsed += TICK;
            self.draw();
        }
    }

    /// Drags a finger inside the scroll view `view` (px) so that its
    /// content moves by `dp` towards its start (the other way for a
    /// negative `dp`), along x if `sideways`. The finger rests before it
    /// lifts, so nothing is flung.
    fn drag(&mut self, view: Rect, dp: f32, sideways: bool) {
        let direction = dp.signum();
        let (start, length) = if sideways {
            (view.x, view.width)
        } else {
            (view.y, view.height)
        };
        let room = length * 0.6 - (TOUCH_SLOP + 2.0) * self.scale();
        let travel = (dp.abs() * self.scale()).min(room.max(1.0));
        let from = start + length * if direction > 0.0 { 0.8 } else { 0.2 };
        let across = if sideways {
            view.y + view.height / 2.0
        } else {
            view.x + view.width / 2.0
        };
        let at = |along: f32| {
            if sideways {
                Point::new(along, across)
            } else {
                Point::new(across, along)
            }
        };
        self.pointer(PointerPhase::Down, at(from));
        let anchor = from - direction * (TOUCH_SLOP + 2.0) * self.scale();
        self.pointer(PointerPhase::Moved, at(anchor));
        for step in 1..=3 {
            self.pointer(
                PointerPhase::Moved,
                at(anchor - direction * travel * step as f32 / 3.0),
            );
        }
        self.elapsed += Duration::from_millis(300);
        self.draw();
        self.pointer(PointerPhase::Up, at(anchor - direction * travel));
    }

    /// Brings the widget wholly into view by dragging the scroll views it
    /// is in: the chips' row sideways, the screen's own up and down.
    fn reveal(&mut self, id: impl Hash + Copy) {
        let margin = 4.0 * self.scale();
        for _ in 0..60 {
            let rect = self.rect(id);
            let shown = self.ui.visible_rect_of(id).unwrap_or_default();
            if shown.width >= rect.width - 1.0 && shown.height >= rect.height - 1.0 {
                return;
            }
            let chips = self.ui.rect_of("chips");
            let page = self.ui.rect_of(self.screen().scroll_id());
            if let Some(view) =
                chips.filter(|view| rect.y >= view.y - 1.0 && rect.y < view.y + view.height)
            {
                let over = rect.x + rect.width + margin - (view.x + view.width);
                let under = view.x + margin - rect.x;
                let wanted = if over > 0.0 { over } else { -under };
                self.drag(view, wanted / self.scale(), true);
            } else if let Some(view) = page {
                let over = rect.y + rect.height + margin - (view.y + view.height);
                let under = view.y + margin - rect.y;
                let wanted = if over > 0.0 { over } else { -under };
                self.drag(view, wanted / self.scale(), false);
            } else {
                panic!(
                    "{}: {rect:?} is cut off and nothing scrolls",
                    self.screen().name()
                );
            }
        }
        panic!("the widget could not be scrolled into view");
    }

    /// A tap at a point in pixels.
    fn tap_at(&mut self, point: Point) {
        let size = self.info.size;
        let insets = self.info.insets;
        assert!(
            point.x > 0.0
                && point.x < size.width as f32
                && point.y > insets.top
                && point.y < size.height as f32 - insets.bottom,
            "{}: {point:?} is off the visible screen",
            self.screen().name()
        );
        self.pointer(PointerPhase::Down, point);
        self.pointer(PointerPhase::Up, point);
        self.settle();
    }

    /// A tap: the widget is brought into view, then finger down and up on
    /// its middle, then whatever it set moving is left to finish.
    fn tap(&mut self, id: impl Hash + Copy) {
        self.reveal(id);
        let rect = self.rect(id);
        self.tap_at(Point::new(
            rect.x + rect.width / 2.0,
            rect.y + rect.height / 2.0,
        ));
    }

    /// The system's Back. Returns whether the app handled it.
    fn back(&mut self) -> bool {
        let handled = self.ui.back();
        self.draw();
        self.settle();
        handled
    }

    fn key(&mut self, event: TextInputEvent) {
        self.ui.text_input(&event);
        self.draw();
    }

    fn type_text(&mut self, text: &str) {
        for character in text.chars() {
            self.key(TextInputEvent::Commit(character.to_string()));
        }
    }

    /// True if the last frame asked for nothing more: the app is idle.
    fn idle(&self) -> bool {
        !self.frame.redraw_requested() && self.frame.redraw_after().is_none()
    }

    /// Lets the answer on its way arrive, however many steps it has.
    fn wait_for_the_answer(&mut self) {
        for _ in 0..40 {
            if !self.app().answering() {
                return;
            }
            self.ticks(STEP_TICKS);
        }
        panic!("the answer never arrived");
    }
}

// --------------------------------------------------------------- journey

/// Onboarding, sign in by typing, home, a model picked in the sheet, a
/// message sent and answered step by step, voice and back to the chat,
/// Explore searched and filtered, a plan chosen, and the back stack home.
#[test]
fn a_complete_journey() {
    for (phone, info) in phones() {
        let mut device = Device::with(Atlas::default(), info);
        assert_eq!(device.screen(), Screen::Onboarding, "{phone}");
        device.settle();
        assert!(device.idle(), "{phone}: onboarding is still");
        device.tap("get-started");
        assert_eq!(device.app().stack(), [Screen::Onboarding, Screen::SignIn]);

        // An empty form is refused, in words.
        device.tap("sign-in");
        assert!(device.app().sign_in_error().is_some());
        assert_eq!(device.screen(), Screen::SignIn);

        // The email keyboard, with Next; then the password, hidden, with
        // Done, which signs in.
        device.tap("email-field");
        assert!(device.ui.is_focused("email"));
        let request = device
            .frame
            .text_input()
            .expect("the keyboard is asked for");
        assert_eq!(request.traits.keyboard, KeyboardType::Email);
        assert_eq!(request.traits.action, ImeAction::Next);
        device.type_text("amina@example.com");
        assert_eq!(
            device.app().sign_in_error(),
            None,
            "typing clears the message"
        );
        device.key(TextInputEvent::Action(ImeAction::Next));
        assert!(device.ui.is_focused("password"));
        let request = device.frame.text_input().expect("the keyboard stays");
        assert!(request.traits.secure, "the password is not shown");
        assert_eq!(request.traits.action, ImeAction::Done);
        device.type_text("secret");
        device.key(TextInputEvent::Action(ImeAction::Done));
        device.settle();
        // Home is the only screen left: there is no going back to sign-in.
        assert_eq!(device.app().stack(), [HOME], "{phone}");
        assert_eq!(device.app().password(), "", "the password is not kept");

        // The model picker rises over home; a choice closes it and shows
        // everywhere.
        device.tap("model");
        assert!(device.app().picker_open());
        device.tap(("mode", Mode::Smart));
        assert!(!device.app().picker_open());
        assert_eq!(device.app().mode(), Mode::Smart);
        assert!(device.idle(), "{phone}: home is still");

        // A message: the send button appears with the first letter.
        assert!(device.ui.rect_of("send").is_none());
        device.tap("draft");
        device.type_text("Compare the prices in these quotes");
        assert!(device.ui.rect_of("mic").is_none());
        device.tap("send");
        assert_eq!(device.app().stack(), [HOME, CHAT], "{phone}");
        assert_eq!(device.app().draft(), "");
        let chat = device.app().conversation();
        assert_eq!(chat.turns.len(), 1);
        assert_eq!(chat.turns[0].prompt, "Compare the prices in these quotes");
        assert_eq!(chat.turns[0].phase, Phase::Thinking);
        assert!(
            !device.idle(),
            "the timer runs while an answer is on its way"
        );

        // The answer arrives and its four steps go by one at a time.
        device.ticks(THINKING_TICKS);
        let mut seen = Vec::new();
        for _ in 0..6 {
            let turn = &device.app().conversation().turns[0];
            seen.push(turn.phase);
            if turn.phase == Phase::Done {
                break;
            }
            device.ticks(STEP_TICKS);
        }
        assert_eq!(
            seen,
            [
                Phase::Working(0),
                Phase::Working(1),
                Phase::Working(2),
                Phase::Working(3),
                Phase::Done
            ],
            "{phone}"
        );
        let turn = &device.app().conversation().turns[0];
        assert!(turn.reply.as_deref().unwrap().ends_with("(Smart)"));
        device.settle();
        // The reply is on screen: the list followed it down.
        let reply = device
            .ui
            .visible_rect_of(("reply", 0usize))
            .expect("the reply shows");
        assert!(reply.height > 0.0);
        let scroll = CHAT.scroll_id();
        assert_eq!(
            device.ui.scroll_offset(scroll),
            device.ui.max_scroll_offset(scroll),
            "{phone}: scrolled to the newest"
        );
        // The keyboard is still up after sending, as in any chat (its
        // caret blinks); the user puts it away.
        assert!(device.ui.is_focused("draft"));
        device.key(TextInputEvent::FocusLost);
        device.settle();
        assert!(device.idle(), "{phone}: the chat is still once answered");

        // Voice: it hears the design's line a word at a time and hands it
        // to the same chat.
        device.tap("mic");
        assert_eq!(device.screen(), Screen::LiveVoice);
        assert_eq!(device.app().voice().words, 0);
        device.ticks(30);
        let heard = device.app().voice().words;
        assert!(
            heard > 1 && heard < 11,
            "{phone}: {heard} words after a second"
        );
        device.ticks(120);
        device.settle();
        assert_eq!(device.app().stack(), [HOME, CHAT], "{phone}");
        let chat = device.app().conversation();
        assert_eq!(chat.turns.len(), 2);
        assert_eq!(chat.turns[1].prompt, VOICE_LINE);
        device.wait_for_the_answer();
        device.settle();
        assert_eq!(device.app().conversation().turns[1].phase, Phase::Done);
        assert!(device.idle());

        // Back to home, then Explore: search, a chip, a tool.
        assert!(device.back());
        assert_eq!(device.app().stack(), [HOME]);
        device.tap(("tab", Tab::Explore));
        assert_eq!(device.app().stack(), [Screen::Explore]);
        device.tap("search-field");
        device.type_text("trans");
        assert_eq!(
            device.app().tools(),
            [2, 5],
            "Translate and Transcribe audio"
        );
        device.tap(("chip", Category::Education));
        assert_eq!(device.app().filter(), ("trans", Category::Education));
        assert_eq!(device.app().tools(), [2, 5]);
        device.tap(("chip", Category::Coding));
        assert_eq!(device.app().tools(), [], "nothing in Coding matches");
        assert!(device.ui.rect_of("no-tools").is_some());
        device.tap(("chip", Category::All));
        device.tap(("tool", 5usize));
        assert_eq!(device.app().stack(), [Screen::Explore, CHAT]);
        assert_eq!(device.app().chats().len(), 3, "a tool starts its own chat");
        device.wait_for_the_answer();
        assert!(device.back());

        // The profile button leads to the plans; a plan is chosen.
        device.tap("profile");
        assert_eq!(device.app().stack(), [Screen::Explore, Screen::Plans]);
        device.tap("get-plus");
        assert_eq!(device.app().plan(), Some(Plan::Plus));
        assert!(device.app().notice().is_some());
        device.ticks(80);
        assert_eq!(device.app().notice(), None, "the toast goes by itself");
        assert!(device.back());
        device.tap(("tab", Tab::Library));
        assert_eq!(device.app().stack(), [Screen::Library]);
        device.tap(("tab", Tab::Home));
        assert_eq!(device.app().stack(), [HOME]);
        device.settle();
        assert!(device.idle(), "{phone}: nothing moves at the end");
        // Back on the root is the system's: it leaves the app.
        assert!(!device.back(), "{phone}");
    }
}

// --------------------------------------------------------------- controls

/// A control, where it is, what leads to it and the tap on it.
struct Control {
    name: &'static str,
    screen: Screen,
    prepare: fn(&mut Device),
    act: fn(&mut Device),
}

fn open_picker(device: &mut Device) {
    device.tap("model");
}

fn nothing(_: &mut Device) {}

fn controls() -> Vec<Control> {
    let control = |name, screen, prepare: fn(&mut Device), act: fn(&mut Device)| Control {
        name,
        screen,
        prepare,
        act,
    };
    vec![
        control("get-started", Screen::Onboarding, nothing, |d| {
            d.tap("get-started")
        }),
        control("have-account", Screen::Onboarding, nothing, |d| {
            d.tap("have-account")
        }),
        control("google", Screen::SignIn, nothing, |d| d.tap("google")),
        control("email-field", Screen::SignIn, nothing, |d| {
            d.tap("email-field")
        }),
        control(
            "email",
            Screen::SignIn,
            |d| d.tap("email"),
            |d| d.type_text("a"),
        ),
        control("password-field", Screen::SignIn, nothing, |d| {
            d.tap("password-field")
        }),
        control(
            "password",
            Screen::SignIn,
            |d| d.tap("password"),
            |d| d.type_text("a"),
        ),
        control("forgot", Screen::SignIn, nothing, |d| d.tap("forgot")),
        control("sign-in", Screen::SignIn, nothing, |d| d.tap("sign-in")),
        control("create-account", Screen::SignIn, nothing, |d| {
            d.tap("create-account")
        }),
        control("profile", HOME, nothing, |d| d.tap("profile")),
        control("suggestion", HOME, nothing, |d| {
            d.tap(("suggestion", 1usize))
        }),
        control("model", HOME, nothing, open_picker),
        control("scrim", HOME, open_picker, |d| {
            let top = d.info.insets.top + 60.0 * d.scale();
            d.tap_at(Point::new(100.0 * d.scale(), top));
        }),
        control("mode", HOME, open_picker, |d| d.tap(("mode", Mode::Code))),
        control("all-models", HOME, open_picker, |d| d.tap("all-models")),
        control("pick", Screen::Models, nothing, |d| {
            d.tap(("pick", Mode::Fast))
        }),
        control("provider", Screen::Models, nothing, |d| {
            d.tap(("provider", 1usize))
        }),
        control("back", Screen::Plans, nothing, |d| d.tap("back")),
        control("tab", HOME, nothing, |d| d.tap(("tab", Tab::Explore))),
        control("attach", HOME, nothing, |d| d.tap("attach")),
        control("detach", HOME, |d| d.tap("attach"), |d| d.tap("detach")),
        control("draft", HOME, nothing, |d| d.tap("draft")),
        control("mic", HOME, nothing, |d| d.tap("mic")),
        control(
            "send",
            HOME,
            |d| {
                d.tap("draft");
                d.type_text("Hello");
            },
            |d| d.tap("send"),
        ),
        control("steps", CHAT, nothing, |d| d.tap(("steps", 0usize))),
        control(
            "cancel",
            CHAT,
            |d| d.tap(("steps", 0usize)),
            |d| d.tap(("cancel", 0usize)),
        ),
        control("attachment", CHAT, nothing, |d| {
            d.tap(("attachment", 0usize))
        }),
        control("minimise", Screen::LiveVoice, nothing, |d| {
            d.tap("minimise")
        }),
        control("camera", Screen::LiveVoice, nothing, |d| d.tap("camera")),
        control("mute", Screen::LiveVoice, nothing, |d| d.tap("mute")),
        control("end", Screen::LiveVoice, nothing, |d| d.tap("end")),
        control("search-field", Screen::Explore, nothing, |d| {
            d.tap("search-field")
        }),
        control(
            "search",
            Screen::Explore,
            |d| d.tap("search"),
            |d| d.type_text("a"),
        ),
        control("chip", Screen::Explore, nothing, |d| {
            d.tap(("chip", Category::Education))
        }),
        control("tool", Screen::Explore, nothing, |d| {
            d.tap(("tool", 5usize))
        }),
        control("get-plus", Screen::Plans, nothing, |d| d.tap("get-plus")),
        control("plan", Screen::Plans, nothing, |d| {
            d.tap(("plan", Plan::Pro))
        }),
        control("chat", Screen::Library, nothing, |d| {
            d.tap(("chat", 0usize))
        }),
        control("new-chat", Screen::Library, nothing, |d| d.tap("new-chat")),
        control("server", Screen::Models, nothing, |d| d.tap("server")),
        control("server-url-field", Screen::Server, nothing, |d| {
            d.tap("server-url-field")
        }),
        control(
            "server-url",
            Screen::Server,
            |d| d.tap("server-url"),
            |d| d.type_text("a"),
        ),
        // With nothing behind the app there is no server to check, and
        // both buttons say so.
        control("server-check", Screen::Server, nothing, |d| {
            d.tap("server-check")
        }),
        control("server-default", Screen::Server, nothing, |d| {
            d.tap("server-default")
        }),
    ]
}

/// Controls that exist only with a server behind the app. Nothing here can
/// reach them (there is no server in this file); `tests/online.rs` taps
/// every one of them, on both phones, against a scripted server.
const ONLINE_CONTROLS: [&str; 6] = [
    "delete-chat",
    "link",
    "reload",
    "retry",
    "stop-speaking",
    "voice-retry",
];

/// Ids in the source that are not controls: containers, texts and what a
/// test looks for.
const NOT_CONTROLS: [&str; 18] = [
    "chips",
    "composer",
    "failed",
    "heard",
    "loading",
    "models-note",
    "no-chats",
    "no-tools",
    "orb",
    "picker",
    "reply",
    "server-status",
    "sign-in-error",
    "tabs",
    "toast",
    "turn",
    "voice-hint",
    "row",
];

#[test]
fn every_control_can_be_reached_and_does_something() {
    for (phone, info) in phones() {
        for control in controls() {
            let mut device = Device::on(control.screen, info);
            (control.prepare)(&mut device);
            let before = device.snapshot();
            (control.act)(&mut device);
            assert_ne!(
                before,
                device.snapshot(),
                "{phone}: {} on {} did nothing",
                control.name,
                control.screen.name()
            );
        }
    }
}

#[test]
fn the_table_names_every_id_in_the_source() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = vec![root.join("components.rs")];
    for entry in std::fs::read_dir(root.join("screens")).expect("src/screens") {
        files.push(entry.expect("a file").path());
    }
    let mut in_source = BTreeSet::new();
    for file in files {
        let source = std::fs::read_to_string(&file).expect("source");
        for (start, _) in source.match_indices(".id(") {
            let rest =
                source[start + 4..].trim_start_matches(|c: char| c == '(' || c.is_whitespace());
            if rest.starts_with('.') {
                // `.id(..)` in a comment.
            } else if let Some(literal) = rest.strip_prefix('"') {
                let name = &literal[..literal.find('"').expect("a closed string")];
                in_source.insert(name.to_owned());
            } else {
                assert!(
                    rest.starts_with("screen.") || rest.starts_with("Screen::"),
                    "{}: an id that is neither a name nor a screen's: {}",
                    file.display(),
                    &rest[..rest.len().min(40)]
                );
            }
        }
    }
    let mut in_table: BTreeSet<String> = controls()
        .iter()
        .map(|control| control.name.to_owned())
        .collect();
    in_table.extend(NOT_CONTROLS.map(str::to_owned));
    in_table.extend(ONLINE_CONTROLS.map(str::to_owned));
    in_table.remove("row");
    assert_eq!(in_source, in_table);
}

// ------------------------------------------------------------ the details

/// Pages 09 and 10 are 03 and 04 with three other links.
#[test]
fn the_simplified_pair_links_to_itself() {
    let simple_home = Screen::Home(Flow::Simplified);
    let simple_chat = Screen::Chat(Flow::Simplified);
    for (_, info) in phones() {
        // 09's send button leads to 10, and 10's arrow back to 09.
        let mut device = Device::with(Atlas::designed("09-home-simplified").unwrap(), info);
        device.tap("draft");
        device.type_text("Hello");
        device.tap("send");
        assert_eq!(device.app().stack(), [simple_home, simple_chat]);
        device.tap("back");
        assert_eq!(device.app().stack(), [simple_home]);
        // 09's Home tab is 09 itself.
        device.tap(("tab", Tab::Home));
        assert_eq!(device.app().stack(), [simple_home]);
        // 10 opened by itself: its arrow leads to 09, as 04's does to 03.
        let mut device = Device::with(Atlas::designed("10-chat-simplified").unwrap(), info);
        device.tap("back");
        assert_eq!(device.app().stack(), [simple_home]);
        let mut device = Device::with(Atlas::designed("04-chat").unwrap(), info);
        device.tap("back");
        assert_eq!(device.app().stack(), [HOME]);
        // Every other link of 09 is 03's: Explore's Home tab is 03.
        let mut device = Device::with(Atlas::designed("09-home-simplified").unwrap(), info);
        device.tap(("tab", Tab::Explore));
        device.tap(("tab", Tab::Home));
        assert_eq!(device.app().stack(), [HOME]);
    }
}

#[test]
fn the_design_pages_start_still_and_move_once_touched() {
    let mut device = Device::with(Atlas::designed("04-chat").unwrap(), galaxy_a27());
    device.ticks(40);
    assert!(device.idle(), "the design's chat is a still frame");
    assert_eq!(
        device.app().conversation().turns[0].phase,
        Phase::Working(2)
    );
    // Unfolding the steps is a touch: the answer then goes on.
    device.tap(("steps", 0usize));
    assert!(device.app().conversation().turns[0].open);
    assert!(!device.idle());
    device.wait_for_the_answer();
    assert_eq!(device.app().conversation().turns[0].phase, Phase::Done);
    device.settle();
    assert!(device.idle());
}

#[test]
fn a_task_can_be_cancelled_and_the_timer_stops() {
    let mut device = Device::on(HOME, small());
    device.tap(("suggestion", 0usize));
    assert_eq!(device.screen(), CHAT);
    device.ticks(THINKING_TICKS + 2);
    device.tap(("steps", 0usize));
    let chat = device.app().conversation();
    assert!(matches!(chat.turns[0].phase, Phase::Working(_)));
    device.tap(("cancel", 0usize));
    let turn = &device.app().conversation().turns[0];
    assert_eq!(turn.phase, Phase::Cancelled);
    assert!(device.ui.rect_of(("reply", 0usize)).is_none());
    device.ticks(60);
    assert_eq!(device.app().conversation().turns[0].phase, Phase::Cancelled);
    assert!(device.idle());
}

#[test]
fn voice_pauses_ends_early_and_can_be_left() {
    for (phone, info) in phones() {
        let mut device = Device::on(HOME, info);
        device.tap("mic");
        device.ticks(20);
        let heard = device.app().voice().words;
        // Muted: nothing more is heard, and nothing moves.
        device.tap("mute");
        assert!(device.app().voice().muted);
        device.ticks(40);
        assert!(device.app().voice().words <= heard + 1, "{phone}");
        let paused = device.app().voice().words;
        device.ticks(40);
        assert_eq!(device.app().voice().words, paused);
        assert!(device.idle(), "{phone}: paused voice is still");
        device.tap("mute");
        device.ticks(10);
        assert!(device.app().voice().words > paused);
        // The cross sends what was heard so far.
        let heard = device.app().voice().words;
        device.tap("end");
        assert_eq!(device.app().stack(), [HOME, CHAT], "{phone}");
        let said = &device.app().conversation().turns[0].prompt;
        assert_eq!(said.split(' ').count(), heard);
        assert!(VOICE_LINE.starts_with(said.as_str()));

        // The chevron leaves and sends nothing; so does the system's Back.
        let mut device = Device::on(HOME, info);
        let chats = device.app().chats().len();
        device.tap("mic");
        device.ticks(20);
        device.tap("minimise");
        assert_eq!(device.app().stack(), [HOME]);
        device.tap("mic");
        assert!(device.back());
        assert_eq!(device.app().stack(), [HOME]);
        assert_eq!(device.app().chats().len(), chats);
        assert!(device.idle());
    }
}

#[test]
fn the_picker_opens_over_any_screen_and_back_closes_it_first() {
    for (phone, info) in phones() {
        for screen in [HOME, CHAT, Screen::LiveVoice] {
            let mut device = Device::on(screen, info);
            device.tap("model");
            assert!(device.app().picker_open(), "{phone}: {screen:?}");
            // The sheet is on the screen, its last row above the gesture bar.
            let last = device.rect("all-models");
            let bottom = info.size.height as f32 - info.insets.bottom;
            assert!(last.y + last.height <= bottom, "{phone}: {last:?}");
            assert!(device.back(), "Back closes the sheet");
            assert!(!device.app().picker_open());
            assert_eq!(device.screen(), screen, "and nothing else");
        }
        // "All models and providers" leads to the models screen, where the
        // choice is the same one.
        let mut device = Device::on(HOME, info);
        device.tap("model");
        device.tap("all-models");
        assert_eq!(device.app().stack(), [HOME, Screen::Models]);
        device.tap(("pick", Mode::Code));
        assert_eq!(device.app().mode(), Mode::Code);
        device.tap("back");
        assert_eq!(device.app().stack(), [HOME]);
    }
}

#[test]
fn the_keyboard_does_not_cover_the_field_being_typed_in() {
    for (phone, info) in phones() {
        let keyboard = 300.0 * info.scale;
        let top = info.size.height as f32 - keyboard;
        for (screen, field, frame) in [
            (Screen::SignIn, "password", "password-field"),
            (HOME, "draft", "composer"),
            (CHAT, "draft", "composer"),
            (Screen::Explore, "search", "search-field"),
        ] {
            let mut device = Device::on(screen, info);
            device.tap(frame);
            assert!(device.ui.is_focused(field), "{phone}: {field}");
            device.info = info.with_keyboard_inset(keyboard);
            device.draw();
            device.settle();
            let shown = device
                .ui
                .visible_rect_of(frame)
                .unwrap_or_else(|| panic!("{phone}: {frame} is hidden"));
            let whole = device.rect(frame);
            assert!(
                shown.height >= whole.height - 1.0 && shown.y + shown.height <= top + 1.0,
                "{phone}: {frame} {shown:?} is under the keyboard at {top}"
            );
            assert!(
                shown.y >= info.insets.top,
                "{phone}: {frame} is under the status bar"
            );
        }
    }
}

#[test]
fn bars_stay_put_while_the_middle_scrolls() {
    let info = small();
    let mut device = Device::on(HOME, info);
    let (profile, tabs, composer) = (
        device.rect("profile"),
        device.rect("tabs"),
        device.rect("composer"),
    );
    let view = device.rect(HOME.scroll_id());
    assert!(device.ui.max_scroll_offset(HOME.scroll_id()).unwrap() > 0.0);
    device.drag(view, 120.0, false);
    assert!(device.ui.scroll_offset(HOME.scroll_id()).unwrap() > 0.0);
    assert_eq!(device.rect("profile"), profile);
    assert_eq!(device.rect("tabs"), tabs);
    assert_eq!(device.rect("composer"), composer);
    // The tab bar ends at the bottom of the screen.
    assert_eq!(tabs.y + tabs.height, info.size.height as f32);

    // Explore's chips scroll sideways under the search field; the list
    // scrolls under them.
    let mut device = Device::on(Screen::Explore, info);
    let chips = device.rect("chips");
    assert!(device.ui.max_scroll_offset("chips").unwrap() > 0.0);
    device.tap(("chip", Category::Education));
    assert!(device.ui.scroll_offset("chips").unwrap() > 0.0);
    assert_eq!(device.rect("chips"), chips);
    let last = device.rect(("chip", Category::Education));
    assert!(last.x + last.width <= info.size.width as f32);
    assert_eq!(device.app().filter().1, Category::Education);
}

#[test]
fn a_drag_over_a_control_is_not_a_tap() {
    let mut device = Device::on(Screen::Explore, small());
    let before = device.snapshot();
    let view = device.rect(Screen::Explore.scroll_id());
    device.drag(view, 80.0, false);
    device.drag(view, -80.0, false);
    assert_eq!(device.snapshot(), before);
}

#[test]
fn the_library_lists_the_chats_and_opens_them() {
    let mut device = Device::on(HOME, galaxy_a27());
    device.tap(("suggestion", 2usize));
    device.wait_for_the_answer();
    assert!(device.back());
    device.tap(("tab", Tab::Library));
    assert_eq!(device.app().chats().len(), 2);
    // The design's conversation is the first; it opens as page 04.
    device.tap(("chat", 0usize));
    assert_eq!(device.app().stack(), [Screen::Library, CHAT]);
    assert_eq!(device.app().conversation().title, "Compare supplier quotes");
    assert!(device.back());
    device.tap(("chat", 1usize));
    assert_eq!(
        device.app().conversation().turns[0].prompt,
        "Create an image for me."
    );
    assert!(device.back());
    device.tap("new-chat");
    assert_eq!(device.app().stack(), [HOME]);
}

#[test]
fn nothing_overflows_at_four_sizes() {
    for info in [galaxy_a27(), small(), design(), tall()] {
        for screen in Screen::ALL {
            let mut device = Device::on(screen, info);
            assert_eq!(
                device.ui.overflows(),
                [],
                "{} at {:?}",
                screen.name(),
                info.size
            );
            // And with what a journey adds: the picker, an attachment, a
            // toast, the unfolded steps, a refused form.
            match screen {
                Screen::Home(_) => {
                    device.tap("attach");
                    device.tap("model");
                }
                Screen::Chat(_) => device.tap(("steps", 0usize)),
                Screen::SignIn => device.tap("sign-in"),
                Screen::Plans => device.tap("get-plus"),
                Screen::Explore => device.tap(("chip", Category::Coding)),
                _ => {}
            }
            assert_eq!(device.ui.overflows(), [], "{} after a tap", screen.name());
        }
    }
}
