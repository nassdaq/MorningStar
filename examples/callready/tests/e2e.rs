//! CallReady end to end, driven the way a phone drives it.
//!
//! Every step here is a pointer event in pixels at a position taken from
//! the last frame's layout (`Ui::rect_of`), a text event from the
//! keyboard, the system's Back (`Ui::back`) or time passing
//! (`FrameInfo::elapsed`, which is what delivers the call's clock ticks),
//! with a frame after each, as the engine does. Nothing is sent to the
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

use callready::model::{Accent, Difficulty, Length, Model, Plan, Scores, When};
use callready::script::Scenario;
use callready::{CallReady, Screen, Tab};
use morningstar::ui::Ui;
use morningstar::{
    App, Frame, FrameInfo, ImeAction, Insets, Point, PointerEvent, PointerPhase, Rect, SurfaceSize,
    TextEditState, TextInputEvent,
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

/// The design's artboard: 390 x 844 dp.
fn design() -> FrameInfo {
    FrameInfo::new(SurfaceSize::new(780, 1688)).with_scale(2.0)
}

/// A tall phone: 411 x 914 dp.
fn tall() -> FrameInfo {
    FrameInfo::new(SurfaceSize::new(1080, 2400)).with_scale(2.625)
}

fn phones() -> [(&'static str, FrameInfo); 2] {
    [("galaxy a27", galaxy_a27()), ("360x640", small())]
}

// ---------------------------------------------------------------- device

/// The framework's touch slop, in dp: a finger must travel further than
/// this before a touch becomes a scroll.
const TOUCH_SLOP: f32 = 8.0;

struct Device {
    ui: Ui<CallReady>,
    frame: Frame,
    info: FrameInfo,
    elapsed: Duration,
}

/// What the application remembers and shows, to tell whether a tap
/// changed anything.
type Snapshot = (Screen, Vec<Screen>, Model, Plan);

impl Device {
    /// The app as it launches, on its welcome screen.
    fn launch(info: FrameInfo) -> Self {
        Self::with(CallReady::default(), info)
    }

    /// The app past its welcome screen, on Home, by tapping "Let's go".
    fn home(info: FrameInfo) -> Self {
        let mut device = Self::launch(info);
        device.tap("welcome-start");
        assert_eq!(device.screen(), Screen::Home);
        device
    }

    fn with(app: CallReady, info: FrameInfo) -> Self {
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

    fn app(&self) -> &CallReady {
        self.ui.app()
    }

    fn model(&self) -> &Model {
        self.ui.app().model()
    }

    fn screen(&self) -> Screen {
        self.ui.app().screen()
    }

    fn snapshot(&self) -> Snapshot {
        let app = self.app();
        (
            app.screen(),
            app.stack().to_vec(),
            app.model().clone(),
            app.offer(),
        )
    }

    fn scale(&self) -> f32 {
        self.info.scale
    }

    fn rect(&self, id: impl Hash) -> Rect {
        self.ui.rect_of(id).expect("the widget is on screen")
    }

    /// The scroll view of the screen being shown, in pixels.
    fn viewport(&self) -> Rect {
        self.rect(self.screen().scroll_id())
    }

    fn offset(&self) -> f32 {
        self.ui
            .scroll_offset(self.screen().scroll_id())
            .expect("every screen has a scroll view")
    }

    fn max_offset(&self) -> f32 {
        self.ui
            .max_scroll_offset(self.screen().scroll_id())
            .expect("every screen has a scroll view")
    }

    fn pointer(&mut self, phase: PointerPhase, position: Point) {
        self.ui.pointer(&PointerEvent {
            id: 1,
            phase,
            position,
        });
        self.draw();
    }

    /// Drags a finger inside the scroll view so that the content moves up
    /// by `dp` (down for a negative `dp`): down, past the touch slop, on
    /// in three moves, up. Returns how far the content scrolled.
    fn drag(&mut self, dp: f32) -> f32 {
        let before = self.offset();
        if self.max_offset() <= 0.0 || dp == 0.0 {
            // Nothing to scroll: a finger moving here would be a tap.
            return 0.0;
        }
        let view = self.viewport();
        let direction = dp.signum();
        // No further in one drag than the finger has room for.
        let room = view.height * 0.6 - (TOUCH_SLOP + 2.0) * self.scale();
        let travel = (dp.abs() * self.scale()).min(room.max(1.0));
        let x = view.x + view.width / 2.0;
        let start = view.y + view.height * if direction > 0.0 { 0.8 } else { 0.2 };
        self.pointer(PointerPhase::Down, Point::new(x, start));
        // The scroll is anchored where the finger passes the slop.
        let anchor = start - direction * (TOUCH_SLOP + 2.0) * self.scale();
        self.pointer(PointerPhase::Moved, Point::new(x, anchor));
        for step in 1..=3 {
            let y = anchor - direction * travel * step as f32 / 3.0;
            self.pointer(PointerPhase::Moved, Point::new(x, y));
        }
        let end = anchor - direction * travel;
        self.pointer(PointerPhase::Up, Point::new(x, end));
        self.offset() - before
    }

    /// Where the widget was laid out, also when it is scrolled wholly out
    /// of its scroll view.
    fn locate(&mut self, id: impl Hash + Copy) -> Option<Rect> {
        self.ui.rect_of(id)
    }

    /// Scrolls until the widget lies wholly inside the scroll view, if it
    /// is in it. A widget of a fixed bar does not move and is left alone.
    fn reveal(&mut self, id: impl Hash + Copy) {
        for _ in 0..40 {
            let rect = self.locate(id).expect("the widget is on the screen");
            let view = self.viewport();
            let margin = 6.0 * self.scale();
            let above = (view.y + margin) - rect.y;
            let below = (rect.y + rect.height) - (view.y + view.height - margin);
            let wanted = if below > 0.0 {
                below / self.scale()
            } else if above > 0.0 {
                -above / self.scale()
            } else {
                return;
            };
            if self.drag(wanted) == 0.0 {
                // The end of the scroll, or a fixed bar.
                return;
            }
        }
        panic!("the widget could not be scrolled into view");
    }

    /// The point a finger would tap for this widget: its middle, after
    /// scrolling it into view. Panics if that point is not on the visible
    /// screen, or lies in the scroll view's content but outside the scroll
    /// view (under a bar, where a finger cannot reach it).
    fn target(&mut self, id: impl Hash + Copy) -> Point {
        self.reveal(id);
        let rect = self.rect(id);
        let view = self.viewport();
        let point = Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
        let size = self.info.size;
        let insets = self.info.insets;
        assert!(
            point.x > 0.0
                && point.x < size.width as f32
                && point.y > insets.top
                && point.y < size.height as f32 - insets.bottom,
            "{}: {rect:?} is off the visible screen",
            self.screen().name()
        );
        let inside_view =
            rect.y >= view.y - 0.5 && rect.y + rect.height <= view.y + view.height + 0.5;
        let outside_view =
            rect.y + rect.height <= view.y + 0.5 || rect.y >= view.y + view.height - 0.5;
        assert!(
            inside_view || outside_view,
            "{}: {rect:?} is cut by the edge of the scroll view {view:?}",
            self.screen().name()
        );
        point
    }

    /// A tap: finger down and up on the widget's middle.
    fn tap(&mut self, id: impl Hash + Copy) {
        let point = self.target(id);
        self.pointer(PointerPhase::Down, point);
        self.pointer(PointerPhase::Up, point);
    }

    fn tab(&mut self, tab: Tab) {
        self.tap(("tab", tab));
    }

    /// The system's Back. Returns whether the app handled it.
    fn back(&mut self) -> bool {
        let handled = self.ui.back();
        self.draw();
        handled
    }

    /// Lets `seconds` pass, a frame each second, as the engine draws them
    /// while the application's timer runs.
    fn wait(&mut self, seconds: u32) {
        for _ in 0..seconds {
            self.elapsed += Duration::from_secs(1);
            self.draw();
        }
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

    /// Answers the waiting exchange by tapping its good tip, or the weaker
    /// one.
    fn answer(&mut self, good: bool) {
        let exchange = self.app().call().exchange().expect("a line is waiting");
        let option = if good {
            exchange.good_option()
        } else {
            1 - exchange.good_option()
        };
        self.tap(("tip", option));
    }

    /// The drawing of the last frame, to tell whether the picture changed.
    fn picture(&self) -> String {
        format!("{:?}", self.frame.list.ops())
    }
}

// --------------------------------------------------------------- journey

/// Welcome, home, today's call answered to the end, its report and
/// transcript, the same call again ended early, its report, home.
#[test]
fn a_complete_journey() {
    for (phone, info) in phones() {
        let mut device = Device::launch(info);
        assert_eq!(device.screen(), Screen::Welcome, "{phone}");
        assert!(!device.back(), "Back on the first screen leaves the app");

        device.tap("welcome-start");
        assert_eq!(device.screen(), Screen::Home);
        assert!(!device.back(), "welcome is not behind home");
        assert_eq!(device.screen(), Screen::Home);

        // Today's call connects: Missing delivery, the clock at zero.
        device.tap("home-start-call");
        assert_eq!(device.screen(), Screen::LiveCall, "{phone}");
        let call = device.app().call().clone();
        assert_eq!(call.scenario, Scenario::MissingDelivery);
        assert_eq!((call.clock().as_str(), call.number), ("00:00", 4));
        assert_eq!(call.progress(), (0.0, 0.0));

        // Every exchange is answered by tapping a chip, one of them with
        // the weaker tip, a few seconds apart.
        let script = Scenario::MissingDelivery.script();
        let plan = [true, true, false, true];
        for (index, good) in plan.into_iter().enumerate() {
            assert_eq!(
                device.screen(),
                Screen::LiveCall,
                "{phone}: exchange {index}"
            );
            assert_eq!(device.app().call().choices.len(), index);
            let line = device.picture();
            device.wait(5);
            device.answer(good);
            assert_ne!(device.picture(), line, "the next line shows");
        }

        // The last answer ends the call: its report, with scores derived
        // from those answers and the time.
        assert_eq!(device.screen(), Screen::CallReport, "{phone}");
        let report = device.model().report.clone();
        assert!(report.is_complete() && !report.designed);
        assert_eq!(report.seconds, 20);
        assert_eq!(report.choices.len(), script.len());
        assert_eq!(
            report.scores,
            Scores::of(
                Scenario::MissingDelivery,
                &report.choices,
                20,
                240,
                Difficulty::Standard
            )
        );
        assert!(report.overall() > 82 && report.overall() < 98, "{report:?}");
        assert_eq!(report.try_next(), script[2].advice);
        assert_eq!(report.did_well(), script[0].praise);
        assert_eq!(device.model().history.len(), 4);
        assert_eq!(device.model().history[0], report);

        // The transcript and back.
        device.tap("transcript");
        assert_eq!(device.screen(), Screen::Transcript);
        device.tap("back");
        assert_eq!(device.screen(), Screen::CallReport);

        // Practise again: the same scenario, from the start.
        device.tap("practise-again");
        assert_eq!(device.screen(), Screen::LiveCall);
        let call = device.app().call().clone();
        assert_eq!(call.scenario, Scenario::MissingDelivery);
        assert_eq!((call.clock().as_str(), call.number), ("00:00", 5));
        assert!(call.choices.is_empty());
        assert_eq!(device.app().stack(), [Screen::Home]);

        // One answer, then the red button: a report of an unfinished call.
        device.wait(3);
        device.answer(true);
        device.tap("end-call");
        assert_eq!(device.screen(), Screen::CallReport);
        let early = device.model().report.clone();
        assert!(!early.is_complete());
        assert_eq!((early.choices.len(), early.seconds), (1, 3));
        assert!(early.overall() < report.overall());
        assert_eq!(device.model().history.len(), 5);

        // Back from the report is home, not the call that ended.
        device.tap("back");
        assert_eq!(device.screen(), Screen::Home, "{phone}");
        assert!(!device.back());
    }
}

// -------------------------------------------------------------- controls

/// How a screen is reached from Home, by taps.
fn reach(device: &mut Device, screen: Screen) {
    match screen {
        Screen::Welcome => unreachable!("welcome is left for good"),
        Screen::Home => {}
        Screen::Profile => device.tap("home-profile"),
        Screen::Notifications => device.tap("home-notifications"),
        Screen::Settings => device.tap("home-scenario-settings"),
        Screen::LiveCall => device.tap("home-start-call"),
        Screen::CallReport => {
            device.tap("home-start-call");
            device.answer(true);
            device.tap("end-call");
        }
        Screen::Transcript => {
            reach(device, Screen::CallReport);
            device.tap("transcript");
        }
        Screen::Premium => {
            device.tab(Tab::More);
            device.tap("more-premium");
        }
        Screen::History => device.tab(Tab::History),
        Screen::Progress => device.tab(Tab::Progress),
        Screen::More => device.tab(Tab::More),
        Screen::Server => {
            device.tab(Tab::More);
            device.tap("more-server");
        }
    }
    assert_eq!(device.screen(), screen);
}

/// Something a finger does on the device.
type Act = Box<dyn Fn(&mut Device)>;
/// Where a widget is.
type Probe = Box<dyn Fn(&Device) -> Rect>;

/// What a tap on a control must do.
#[derive(Clone, Copy, Debug)]
enum Effect {
    /// Shows this screen.
    Goes(Screen),
    /// Stays, and changes what the application remembers.
    Changes,
    /// Stays, and gives the text field the focus (the keyboard).
    Focuses,
}

/// A tappable widget: the name its id starts with, for the check against
/// the source, and how to find and tap it.
struct Control {
    on: Screen,
    name: &'static str,
    label: String,
    /// A control to tap first, when this one only does something after it
    /// (the chip that is already selected).
    first: Option<Act>,
    tap: Act,
    effect: Effect,
}

fn control<I: Hash + Copy + std::fmt::Debug + 'static>(
    on: Screen,
    name: &'static str,
    id: I,
    effect: Effect,
) -> Control {
    Control {
        on,
        name,
        label: format!("{id:?}"),
        first: None,
        tap: Box::new(move |device| device.tap(id)),
        effect,
    }
}

impl Control {
    fn after<I: Hash + Copy + 'static>(mut self, id: I) -> Self {
        self.first = Some(Box::new(move |device| device.tap(id)));
        self
    }

    /// For a control that only does something once a field holds
    /// something new: `text` is typed into the field `id` first.
    fn after_typing<I: Hash + Copy + 'static>(mut self, id: I, text: &'static str) -> Self {
        self.first = Some(Box::new(move |device| {
            device.tap(id);
            device.type_text(text);
        }));
        self
    }
}

/// The controls that only a call played by a server has, or a report
/// fetched from one. No server can be reached from this file (the app runs
/// here as it does on a phone with none), so `tests/online.rs` drives
/// them, with a stand-in for the server; they are listed here so that the
/// check against the source knows them.
const ONLINE_CONTROLS: [&str; 7] = [
    "reply-input",
    "reply-send",
    "talk",
    "type-instead",
    "speak-instead",
    "retry",
    "report-reload",
];

/// Every control of the app. `the_table_names_every_id_in_the_source`
/// holds this list to the screens' source.
fn controls() -> Vec<Control> {
    use Effect::{Changes, Focuses, Goes};
    let mut list = vec![
        // Home.
        control(
            Screen::Home,
            "home-profile",
            "home-profile",
            Goes(Screen::Profile),
        ),
        control(
            Screen::Home,
            "home-notifications",
            "home-notifications",
            Goes(Screen::Notifications),
        ),
        control(
            Screen::Home,
            "home-start-call",
            "home-start-call",
            Goes(Screen::LiveCall),
        ),
        control(
            Screen::Home,
            "home-scenario-settings",
            "home-scenario-settings",
            Goes(Screen::Settings),
        ),
        // A call in progress.
        control(Screen::LiveCall, "back", "back", Goes(Screen::Home)),
        control(Screen::LiveCall, "replay", "replay", Changes),
        control(Screen::LiveCall, "mute", "mute", Changes),
        control(Screen::LiveCall, "tip", ("tip", 0usize), Changes),
        control(Screen::LiveCall, "tip", ("tip", 1usize), Changes),
        control(
            Screen::LiveCall,
            "end-call",
            "end-call",
            Goes(Screen::CallReport),
        ),
        // The report and the transcript.
        control(Screen::CallReport, "back", "back", Goes(Screen::Home)),
        control(
            Screen::CallReport,
            "transcript",
            "transcript",
            Goes(Screen::Transcript),
        ),
        control(
            Screen::CallReport,
            "practise-again",
            "practise-again",
            Goes(Screen::LiveCall),
        ),
        control(Screen::Transcript, "back", "back", Goes(Screen::CallReport)),
        // The offer, reached from More.
        control(Screen::Premium, "close", "close", Goes(Screen::More)),
        control(Screen::Premium, "plan", ("plan", Plan::Free), Changes),
        control(Screen::Premium, "plan", ("plan", Plan::Premium), Changes)
            .after(("plan", Plan::Free)),
        control(Screen::Premium, "start", "start", Goes(Screen::More)),
        // The profile.
        control(Screen::Profile, "back", "back", Goes(Screen::Home)),
        control(Screen::Profile, "profile-name", "profile-name", Focuses),
        control(
            Screen::Profile,
            "profile-settings",
            "profile-settings",
            Goes(Screen::Settings),
        ),
        control(
            Screen::Profile,
            "profile-premium",
            "profile-premium",
            Goes(Screen::Premium),
        ),
        // Notifications.
        control(Screen::Notifications, "back", "back", Goes(Screen::Home)),
        control(
            Screen::Notifications,
            "notification",
            ("notification", 0usize),
            Goes(Screen::CallReport),
        ),
        control(
            Screen::Notifications,
            "notification",
            ("notification", 1usize),
            Goes(Screen::Progress),
        ),
        control(
            Screen::Notifications,
            "notification",
            ("notification", 2usize),
            Goes(Screen::Premium),
        ),
        // Scenario settings: the selected chip does nothing until another
        // has been chosen.
        control(Screen::Settings, "back", "back", Goes(Screen::Home)),
        control(Screen::Settings, "accent", ("accent", Accent::Us), Changes)
            .after(("accent", Accent::Uk)),
        control(
            Screen::Settings,
            "difficulty",
            ("difficulty", Difficulty::Standard),
            Changes,
        )
        .after(("difficulty", Difficulty::Tough)),
        control(
            Screen::Settings,
            "length",
            ("length", Length::Four),
            Changes,
        )
        .after(("length", Length::Two)),
        // Progress and More.
        control(
            Screen::Progress,
            "progress-latest",
            "progress-latest",
            Goes(Screen::CallReport),
        ),
        control(
            Screen::More,
            "more-profile",
            "more-profile",
            Goes(Screen::Profile),
        ),
        control(
            Screen::More,
            "more-notifications",
            "more-notifications",
            Goes(Screen::Notifications),
        ),
        control(
            Screen::More,
            "more-settings",
            "more-settings",
            Goes(Screen::Settings),
        ),
        control(
            Screen::More,
            "more-premium",
            "more-premium",
            Goes(Screen::Premium),
        ),
        control(
            Screen::More,
            "more-server",
            "more-server",
            Goes(Screen::Server),
        ),
        // The server's address. With no server to reach, "Check
        // connection" and "Use the default" change something only when the
        // field holds another address than the one in use.
        control(Screen::Server, "back", "back", Goes(Screen::More)),
        control(Screen::Server, "server-url", "server-url", Focuses),
        control(Screen::Server, "server-check", "server-check", Changes)
            .after_typing("server-url", "0"),
        control(Screen::Server, "server-default", "server-default", Changes)
            .after_typing("server-url", "0"),
    ];
    for index in 0..3usize {
        list.push(control(
            Screen::Home,
            "home-scenario",
            ("home-scenario", index),
            Goes(Screen::LiveCall),
        ));
        list.push(control(
            Screen::History,
            "history-call",
            ("history-call", index),
            Goes(Screen::CallReport),
        ));
    }
    for accent in [Accent::Uk, Accent::Australian, Accent::Indian] {
        list.push(control(
            Screen::Settings,
            "accent",
            ("accent", accent),
            Changes,
        ));
    }
    for difficulty in [Difficulty::Calm, Difficulty::Tough] {
        list.push(control(
            Screen::Settings,
            "difficulty",
            ("difficulty", difficulty),
            Changes,
        ));
    }
    for length in [Length::Two, Length::Six] {
        list.push(control(
            Screen::Settings,
            "length",
            ("length", length),
            Changes,
        ));
    }
    // The tab bar, on each of the four screens that carry it: every other
    // tab leads to its screen.
    for on in Tab::ALL {
        for to in Tab::ALL {
            if on != to {
                list.push(control(on.screen(), "tab", ("tab", to), Goes(to.screen())));
            }
        }
    }
    list
}

/// Every control, on both phones: it can be brought into view and tapped
/// with a finger, and the tap navigates or changes state.
#[test]
fn every_control_can_be_reached_and_does_something() {
    for (phone, info) in phones() {
        // Welcome's one control.
        let mut device = Device::launch(info);
        device.tap("welcome-start");
        assert_eq!(device.screen(), Screen::Home, "{phone}");

        for control in controls() {
            let mut device = Device::home(info);
            reach(&mut device, control.on);
            if let Some(first) = &control.first {
                first(&mut device);
            }
            let before = device.snapshot();
            (control.tap)(&mut device);
            let what = format!("{phone}: {} on {}", control.label, control.on.name());
            match control.effect {
                Effect::Goes(screen) => assert_eq!(device.screen(), screen, "{what}"),
                Effect::Changes => {
                    assert_eq!(device.screen(), control.on, "{what}");
                    assert_ne!(device.snapshot(), before, "{what} changes nothing");
                }
                Effect::Focuses => {
                    assert_eq!(device.screen(), control.on, "{what}");
                    assert!(device.ui.has_focus(), "{what}");
                    assert!(device.frame.text_input().is_some(), "{what}: no keyboard");
                }
            }
        }
    }
}

/// The few taps that rightly do nothing: the tab that is showing, the chip
/// or plan that is already selected, and a tip while the learner is muted.
#[test]
fn only_what_is_already_so_does_nothing() {
    let mut device = Device::home(galaxy_a27());
    for tab in Tab::ALL {
        device.tab(tab);
        let before = device.snapshot();
        device.tab(tab);
        assert_eq!(device.snapshot(), before, "{tab:?} is showing");
    }
    device.tab(Tab::Home);

    reach(&mut device, Screen::Settings);
    let before = device.snapshot();
    device.tap(("accent", Accent::Us));
    device.tap(("difficulty", Difficulty::Standard));
    device.tap(("length", Length::Four));
    assert_eq!(device.snapshot(), before);
    assert!(device.back());

    reach(&mut device, Screen::Premium);
    let before = device.snapshot();
    device.tap(("plan", Plan::Premium));
    assert_eq!(device.snapshot(), before);
    assert!(device.back() && device.back());

    // Muted, the learner cannot answer; unmuted, they can.
    reach(&mut device, Screen::LiveCall);
    device.tap("mute");
    assert!(device.app().call().muted);
    let before = device.snapshot();
    device.tap(("tip", 0usize));
    device.tap(("tip", 1usize));
    assert_eq!(device.snapshot(), before);
    device.tap("mute");
    device.tap(("tip", 0usize));
    assert_eq!(device.app().call().choices, [0]);
}

/// The ids written with `.id(..)` in the screens and components are
/// exactly those of the table above (and the screens' roots and scroll
/// views, which take their ids from `Screen`): a control added to a screen
/// without a line in the table fails here.
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
    in_table.insert("welcome-start".to_owned());
    in_table.extend(ONLINE_CONTROLS.map(str::to_owned));
    assert_eq!(in_source, in_table);
}

// -------------------------------------------------------------- scrolling

/// Where each screen that can be too tall is checked: the controls of its
/// fixed bars, and the last control of what scrolls.
fn tall_screens() -> Vec<(Screen, Vec<Probe>, Probe)> {
    fn at<I: Hash + Copy + 'static>(id: I) -> Probe {
        Box::new(move |device| device.rect(id))
    }
    vec![
        (
            Screen::Home,
            vec![at(("tab", Tab::Home)), at(("tab", Tab::More))],
            at(("home-scenario", 2usize)),
        ),
        (
            Screen::LiveCall,
            vec![at("back"), at("mute"), at("end-call")],
            at(("tip", 1usize)),
        ),
        (
            Screen::CallReport,
            vec![at("transcript"), at("practise-again")],
            at("back"),
        ),
        (Screen::Premium, vec![at("start")], at(("plan", Plan::Free))),
        (Screen::Profile, vec![], at("profile-premium")),
        (Screen::Notifications, vec![], at(("notification", 2usize))),
        (Screen::Settings, vec![], at(("length", Length::Six))),
        (Screen::Transcript, vec![], at("back")),
        (
            Screen::History,
            vec![at(("tab", Tab::History))],
            at(("history-call", 2usize)),
        ),
        (
            Screen::Progress,
            vec![at(("tab", Tab::Progress))],
            at("progress-latest"),
        ),
        (
            Screen::More,
            vec![at(("tab", Tab::More))],
            at("more-server"),
        ),
        (Screen::Server, vec![], at("server-default")),
    ]
}

/// On both phones every screen scrolls by a drag when it does not fit,
/// its bars stay where they are, and the end of the content comes wholly
/// into view. On the small phone the live call is one that does not fit.
/// (On the owner's it did while its tips were written out as two rows;
/// they wrap now, and a call's first two tips share a line there.)
#[test]
fn tall_screens_scroll_between_their_fixed_bars() {
    for (phone, info) in phones() {
        let mut scrolled = Vec::new();
        for (screen, bars, last) in tall_screens() {
            let mut device = Device::home(info);
            reach(&mut device, screen);
            let what = format!("{phone}: {}", screen.name());
            assert_eq!(device.offset(), 0.0, "{what} starts at the top");
            let max = device.max_offset();
            if max <= 0.0 {
                // It fits: the whole screen is in view and nothing moves.
                let view = device.viewport();
                let rect = last(&device);
                if screen != Screen::CallReport && screen != Screen::Transcript {
                    assert!(rect.y + rect.height <= view.y + view.height + 0.5, "{what}");
                }
                assert_eq!(device.drag(100.0), 0.0, "{what}");
                continue;
            }
            scrolled.push(screen);
            let before: Vec<Rect> = bars.iter().map(|bar| bar(&device)).collect();

            // A first drag moves the content by what the finger moved.
            let step = max.min(40.0);
            let moved = device.drag(step);
            assert!((moved - step).abs() < 0.01, "{what}: {moved} of {step}");

            // Drag to the end: the scroll stops at its limit.
            for _ in 0..40 {
                if device.drag(max) == 0.0 {
                    break;
                }
            }
            assert!((device.offset() - max).abs() < 0.01, "{what}");
            let after: Vec<Rect> = bars.iter().map(|bar| bar(&device)).collect();
            assert_eq!(after, before, "{what}: a bar moved");
            if screen != Screen::CallReport && screen != Screen::Transcript {
                let view = device.viewport();
                let rect = last(&device);
                assert!(
                    rect.y >= view.y && rect.y + rect.height <= view.y + view.height + 0.5,
                    "{what}: the last control {rect:?} is not wholly in {view:?}"
                );
            }

            // And back to the top.
            for _ in 0..40 {
                if device.drag(-max) == 0.0 {
                    break;
                }
            }
            assert_eq!(device.offset(), 0.0, "{what}");
            // The back arrow of a pushed screen scrolls with the page and
            // is in reach again.
            if screen == Screen::CallReport || screen == Screen::Transcript {
                let rect = last(&device);
                assert!(rect.y >= device.viewport().y, "{what}");
            }
        }
        println!(
            "{phone}: scroll: {:?}",
            scrolled.iter().map(|s| s.name()).collect::<Vec<_>>()
        );
        if phone == "360x640" {
            for screen in [
                Screen::Home,
                Screen::LiveCall,
                Screen::CallReport,
                Screen::Premium,
                Screen::Profile,
            ] {
                assert!(scrolled.contains(&screen), "{phone}: {}", screen.name());
            }
        }
    }

    // Welcome too, on the small phone: the collage scrolls over the button.
    let mut device = Device::launch(small());
    let button = device.rect("welcome-start");
    let max = device.max_offset();
    assert!(max > 0.0);
    for _ in 0..40 {
        if device.drag(max) == 0.0 {
            break;
        }
    }
    assert!((device.offset() - max).abs() < 0.01);
    assert_eq!(device.rect("welcome-start"), button);
    device.tap("welcome-start");
    assert_eq!(device.screen(), Screen::Home);
}

/// A drag that starts on a control scrolls and does not tap it.
#[test]
fn a_drag_over_a_control_is_not_a_tap() {
    let mut device = Device::home(small());
    device.tap("home-start-call");
    assert!(device.max_offset() > 0.0);
    let before = device.snapshot();
    let chip = device.target(("tip", 0usize));
    device.pointer(PointerPhase::Down, chip);
    let scale = device.scale();
    for step in 1..=4 {
        let y = chip.y - step as f32 * 10.0 * scale;
        device.pointer(PointerPhase::Moved, Point::new(chip.x, y));
    }
    device.pointer(PointerPhase::Up, Point::new(chip.x, chip.y - 40.0 * scale));
    assert_eq!(device.snapshot(), before, "the chip was not chosen");
}

// ------------------------------------------------------------------ state

/// The name on the profile is typed with the keyboard and shows on home.
#[test]
fn the_profile_name_is_edited_by_typing() {
    for (phone, info) in phones() {
        let mut device = Device::home(info);
        let home = device.picture();
        device.tap("home-profile");
        assert_eq!(device.model().name, "Grace Mushi", "{phone}");
        assert!(device.frame.text_input().is_none(), "no keyboard at rest");

        device.tap("profile-name");
        assert!(device.ui.is_focused("profile-name"));
        let request = device
            .frame
            .text_input()
            .expect("the keyboard is asked for");
        assert_eq!(request.text, "Grace Mushi");
        assert_eq!(request.traits.action, ImeAction::Done);

        // The keyboard puts the caret at the end; five backspaces and a
        // new surname.
        device.key(TextInputEvent::Edited(TextEditState {
            text: "Grace Mushi".to_owned(),
            caret: 11,
            anchor: None,
            composing: None,
        }));
        for _ in 0..5 {
            device.key(TextInputEvent::DeleteBackward);
        }
        assert_eq!(device.model().name, "Grace ");
        device.type_text("Kimaro");
        assert_eq!(device.model().name, "Grace Kimaro");
        assert_eq!(device.model().initials(), "GK");

        // A whole new name, as a soft keyboard reports it; it is cut at
        // the limit.
        device.key(TextInputEvent::Edited(TextEditState {
            text: "Neema".to_owned(),
            caret: 5,
            anchor: None,
            composing: None,
        }));
        assert_eq!(device.model().name, "Neema");
        device.type_text(" Josephine Mwakalinga");
        assert_eq!(device.model().name, "Neema Josephine Mwakalin");

        // Done closes the keyboard; back shows the new name on home.
        device.key(TextInputEvent::Action(ImeAction::Done));
        assert!(!device.ui.has_focus());
        device.tap("back");
        assert_eq!(device.screen(), Screen::Home);
        assert_eq!(device.model().first_name(), "Neema");
        assert_ne!(
            device.picture(),
            home,
            "{phone}: home greets by the new name"
        );
    }
}

/// The scenario settings show on home and are those of the next call.
#[test]
fn settings_reach_home_and_the_next_call() {
    for (_, info) in phones() {
        let mut device = Device::home(info);
        let home = device.picture();
        assert_eq!(
            device.model().settings.summary(),
            "Customer \u{b7} US accent \u{b7} 4 min"
        );
        device.tap("home-scenario-settings");
        device.tap(("accent", Accent::Australian));
        device.tap(("difficulty", Difficulty::Tough));
        device.tap(("length", Length::Two));
        device.tap("back");
        assert_eq!(device.screen(), Screen::Home);
        assert_eq!(
            device.model().settings.summary(),
            "Customer \u{b7} Australian accent \u{b7} 2 min"
        );
        assert_ne!(
            device.picture(),
            home,
            "the line under today's call changed"
        );

        // The second scenario card: Refund request, with those settings.
        device.tap(("home-scenario", 1usize));
        let call = device.app().call().clone();
        assert_eq!(call.scenario, Scenario::RefundRequest);
        assert_eq!(call.scenario.caller(), "Priya");
        assert_eq!(
            (call.accent, call.difficulty, call.limit),
            (Accent::Australian, Difficulty::Tough, 120)
        );

        // All answered with the weaker tip, on a tough caller: a low score.
        for _ in 0..4 {
            device.answer(false);
        }
        assert_eq!(device.screen(), Screen::CallReport);
        let report = &device.model().report;
        assert_eq!(
            report.scores,
            Scores::of(
                Scenario::RefundRequest,
                &report.choices,
                0,
                120,
                Difficulty::Tough
            )
        );
        assert_eq!(report.overall(), 59);
        assert_eq!(report.note(), "keep going!");
    }
}

/// Each scenario card and today's call start their own call.
#[test]
fn each_scenario_starts_its_own_call() {
    let card =
        |index: usize| -> Act { Box::new(move |device| device.tap(("home-scenario", index))) };
    let starts: [(Act, Scenario, &str); 4] = [
        (
            Box::new(|device| device.tap("home-start-call")),
            Scenario::MissingDelivery,
            "Mark",
        ),
        (card(0), Scenario::AngryCustomer, "Dana"),
        (card(1), Scenario::RefundRequest, "Priya"),
        (card(2), Scenario::BillingProblem, "Tom"),
    ];
    let mut pictures = BTreeSet::new();
    for (start, scenario, caller) in starts {
        let mut device = Device::home(galaxy_a27());
        start(&mut device);
        assert_eq!(device.screen(), Screen::LiveCall);
        let call = device.app().call();
        assert_eq!((call.scenario, call.scenario.caller()), (scenario, caller));
        assert_eq!(call.exchange(), scenario.script().first());
        pictures.insert(device.picture());
        // Back abandons the call: nothing is scored.
        assert!(device.back());
        assert_eq!(device.screen(), Screen::Home);
        assert_eq!(device.model().history.len(), 3);
        assert!(device.app().tick().is_none());
    }
    assert_eq!(pictures.len(), 4, "four different calls on screen");
}

/// Opening the notifications clears the bell's dot; each note leads where
/// it points; a finished call rings the bell again.
#[test]
fn the_bell_clears_and_its_notes_lead_somewhere() {
    let mut device = Device::home(galaxy_a27());
    let with_dot = device.picture();
    assert!(device.model().bell);

    device.tap("home-notifications");
    assert_eq!(device.screen(), Screen::Notifications);
    assert!(!device.model().bell);

    // "Your report is ready": the latest report, which is the design's.
    device.tap(("notification", 0usize));
    assert_eq!(device.screen(), Screen::CallReport);
    assert!(device.model().report.designed);
    assert_eq!(device.model().read, [true, false, false]);
    assert!(device.back());
    assert_eq!(device.screen(), Screen::Notifications);

    device.tap("back");
    assert_eq!(device.screen(), Screen::Home);
    assert_ne!(device.picture(), with_dot, "the dot is gone");
    let without_dot = device.picture();

    // A finished call: the bell rings, and the note points at its report.
    device.tap(("home-scenario", 2usize));
    for _ in 0..4 {
        device.answer(true);
    }
    device.tap("back");
    assert!(device.model().bell);
    assert_ne!(device.picture(), without_dot);
    device.tap("home-notifications");
    device.tap(("notification", 0usize));
    assert_eq!(device.model().report.scenario, Scenario::BillingProblem);
    assert_eq!(device.model().report.overall(), 98);
}

/// The history lists finished calls, grows with each, and opens reports.
#[test]
fn history_grows_and_opens_reports() {
    for (_, info) in phones() {
        let mut device = Device::home(info);
        device.tab(Tab::History);
        assert!(device.locate(("history-call", 2usize)).is_some());
        assert!(device.locate(("history-call", 3usize)).is_none());
        let average = device.model().average();

        // An old call's report, and back to the list.
        device.tap(("history-call", 1usize));
        assert_eq!(device.screen(), Screen::CallReport);
        assert_eq!(device.model().report.scenario, Scenario::RefundRequest);
        assert_eq!(device.model().report.when, When::DaysAgo(2));
        device.tap("back");
        assert_eq!(device.screen(), Screen::History);

        // A new call, finished.
        device.tab(Tab::Home);
        device.tap(("home-scenario", 0usize));
        device.wait(12);
        for _ in 0..4 {
            device.answer(true);
        }
        assert_eq!(device.screen(), Screen::CallReport);
        device.tap("back");
        device.tab(Tab::History);
        assert!(device.locate(("history-call", 3usize)).is_some());
        assert!(device.model().average() > average);
        assert_eq!(device.model().streak(), 4);

        device.tap(("history-call", 0usize));
        let report = &device.model().report;
        assert_eq!(
            (report.scenario, report.when, report.seconds),
            (Scenario::AngryCustomer, When::Today, 12)
        );
        // And the progress tab's latest call is that one.
        assert!(device.back());
        device.tab(Tab::Progress);
        device.tap("progress-latest");
        assert_eq!(device.model().report.when, When::Today);
    }
}

/// Starting Premium records the plan (there is no payment) and shows on
/// More and the profile; Free undoes it; the cross records nothing.
#[test]
fn premium_is_recorded_and_shown() {
    let mut device = Device::home(galaxy_a27());
    device.tab(Tab::More);
    let free = device.picture();
    device.tap("more-premium");
    assert_eq!(device.app().offer(), Plan::Premium);

    // The cross: nothing recorded.
    device.tap("close");
    assert_eq!(device.screen(), Screen::More);
    assert!(!device.model().premium);
    assert_eq!(device.picture(), free);

    // The button: Premium, shown on More.
    device.tap("more-premium");
    let offer = device.picture();
    device.tap("start");
    assert_eq!(device.screen(), Screen::More);
    assert!(device.model().premium);
    assert_ne!(device.picture(), free, "More says Premium is active");

    // The offer now says it is active, and the profile's plan leads to it.
    device.tap("more-profile");
    device.tap("profile-premium");
    assert_eq!(device.screen(), Screen::Premium);
    assert_ne!(device.picture(), offer);

    // Free, and the button: Premium ends.
    device.tap(("plan", Plan::Free));
    assert_eq!(device.app().offer(), Plan::Free);
    device.tap("start");
    assert_eq!(device.screen(), Screen::Profile);
    assert!(!device.model().premium);
}

/// Back arrows and the system's Back return through the stack; tabs sit
/// one step above Home; Back on Home is not handled.
#[test]
fn back_returns_to_where_the_learner_came_from() {
    let mut device = Device::home(small());

    // Home > Profile > Settings > (back) > Premium > (back) > (back).
    device.tap("home-profile");
    device.tap("profile-settings");
    assert_eq!(device.app().stack(), [Screen::Home, Screen::Profile]);
    assert!(device.back());
    assert_eq!(device.screen(), Screen::Profile);
    device.tap("profile-premium");
    device.tap("close");
    assert_eq!(device.screen(), Screen::Profile);
    device.tap("back");
    assert_eq!(device.screen(), Screen::Home);
    assert!(!device.back(), "Back on home leaves the app");
    assert_eq!(device.screen(), Screen::Home);

    // A tab, then another: Back is Home either way, then out.
    device.tab(Tab::History);
    device.tab(Tab::More);
    assert_eq!(device.app().stack(), [Screen::Home]);
    device.tap("more-settings");
    assert!(device.back());
    assert_eq!(device.screen(), Screen::More);
    assert!(device.back());
    assert_eq!(device.screen(), Screen::Home);
    assert!(!device.back());

    // A notification that points at a tab.
    device.tap("home-notifications");
    device.tap(("notification", 1usize));
    assert_eq!(device.screen(), Screen::Progress);
    assert!(device.back());
    assert_eq!(device.screen(), Screen::Home);

    // History > report > transcript, and all the way back.
    device.tab(Tab::History);
    device.tap(("history-call", 2usize));
    device.tap("transcript");
    assert_eq!(
        device.app().stack(),
        [Screen::Home, Screen::History, Screen::CallReport]
    );
    assert!(device.back() && device.back());
    assert_eq!(device.screen(), Screen::History);
    assert!(device.back());
    assert!(!device.back());

    // Back with the keyboard's field focused still returns.
    device.tap("home-profile");
    device.tap("profile-name");
    assert!(device.back());
    assert_eq!(device.screen(), Screen::Home);
    assert!(!device.ui.has_focus(), "the field left with its screen");
}

/// The call's clock: the runtime's timer ticks it once a second while the
/// call runs, and stops asking when the call ends.
#[test]
fn the_clock_runs_with_the_call_and_stops_with_it() {
    let mut device = Device::home(galaxy_a27());
    assert!(device.app().tick().is_none(), "idle on home");
    device.wait(3);

    device.tap("home-start-call");
    assert_eq!(
        device.app().tick().map(|(interval, _)| interval),
        Some(Duration::from_secs(1))
    );
    let connected = device.picture();
    device.wait(5);
    assert_eq!(device.app().call().clock(), "00:05");
    assert_ne!(
        device.picture(),
        connected,
        "the clock and the time bar moved"
    );

    // Replay: the customer's line is shown as repeated for two seconds.
    device.tap("replay");
    assert_eq!(
        (device.app().call().replays, device.app().call().repeating),
        (1, 2)
    );
    let repeating = device.picture();
    device.wait(2);
    assert_eq!(device.app().call().repeating, 0);
    assert_ne!(device.picture(), repeating);
    assert_eq!(device.app().call().clock(), "00:07");

    // An answer moves the script's bar on; the clock keeps running.
    device.answer(true);
    assert_eq!(device.app().call().progress().0, 0.25);
    device.wait(60);
    assert_eq!(device.app().call().clock(), "01:07");

    // Ended: the report has the time, and nothing ticks any more.
    device.tap("end-call");
    assert_eq!(device.screen(), Screen::CallReport);
    assert_eq!(device.model().report.seconds, 67);
    assert_eq!(device.model().report.duration(), "1 min");
    assert!(device.app().tick().is_none(), "idle after the call");
    let report = device.snapshot();
    device.wait(10);
    assert_eq!(device.snapshot(), report);

    // The clock does not run past a call that was left with Back.
    device.tap("practise-again");
    device.wait(2);
    assert!(device.back());
    assert!(device.app().tick().is_none());
    let home = device.snapshot();
    device.wait(5);
    assert_eq!(device.snapshot(), home);
}

// ---------------------------------------------------------------- layout

/// No box is too small for its children: every screen as it opens, and
/// the states a journey reaches, at four sizes and scales.
#[test]
fn nothing_overflows_at_four_sizes() {
    let frames = [
        ("galaxy a27", galaxy_a27()),
        ("360x640", small()),
        ("design", design()),
        ("411x914", tall()),
    ];
    for (name, info) in frames {
        for screen in Screen::ALL {
            let device = Device::with(CallReady::on(screen), info);
            assert_eq!(device.ui.overflows(), [], "{name}: {}", screen.name());
            assert!(!device.frame.list.is_empty());
        }

        // A journey: each screen in a state the learner made.
        let mut device = Device::home(info);
        let check = |device: &Device, what: &str| {
            assert_eq!(device.ui.overflows(), [], "{name}: {what}");
        };
        device.tap("home-profile");
        device.tap("profile-name");
        device.type_text(" Wilhelmina Mwakalinga");
        check(&device, "a long name on the profile");
        device.tap("back");
        check(&device, "a long name on home");
        device.tap("home-scenario-settings");
        device.tap(("accent", Accent::Australian));
        device.tap("back");
        check(&device, "the longest accent on home");
        for scenario in 0..3usize {
            device.tap(("home-scenario", scenario));
            check(&device, "a call connected");
            device.tap("replay");
            device.tap("mute");
            check(&device, "muted, the line repeating");
            device.tap("mute");
            for step in 0..4 {
                device.answer(step % 2 == 0);
                check(&device, "a call under way, or its report");
            }
            device.tap("transcript");
            check(&device, "a derived transcript");
            assert!(device.back() && device.back());
        }
        device.tab(Tab::More);
        device.tap("more-premium");
        device.tap("start");
        check(&device, "more, with premium");
        device.tap("more-premium");
        check(&device, "the offer, active");
        device.tap(("plan", Plan::Free));
        check(&device, "the offer, free selected");
        device.tap("close");
        device.tap("more-profile");
        check(&device, "the profile, with premium");
        device.tap("back");
        device.tab(Tab::History);
        check(&device, "a history of six");
        device.tab(Tab::Progress);
        check(&device, "progress after six calls");
        device.tab(Tab::Home);
        device.tap("home-notifications");
        check(&device, "notifications after calls");
    }
}
