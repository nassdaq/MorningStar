//! `Application::back` and `Application::tick` (ADR 0018), driven the way
//! the engine drives a `Ui`: frames on demand at a given foreground time,
//! and the frame's timed redraw request read as the host would read it.

use std::time::Duration;

use morningstar_core::{Point, PointerEvent, PointerPhase, SurfaceSize};
use morningstar_runtime::{App, Frame, FrameInfo};
use morningstar_ui::prelude::*;

const REGULAR: &[u8] =
    include_bytes!("../../../examples/hello-gpu/assets/fonts/NotoSans-Regular.ttf");

#[derive(Clone, Copy, Debug, PartialEq)]
enum Screen {
    Home,
    Detail,
    Settings,
}

#[derive(Clone, Debug, PartialEq)]
enum Msg {
    Open(Screen),
    Start(u32),
    Second,
    Field(String),
}

/// A navigation stack and a countdown: what the two features are for.
struct Nav {
    stack: Vec<Screen>,
    /// Seconds left of the countdown; zero means it is not running.
    left: u32,
    interval: Duration,
    ticks: u32,
    /// `Msg::Second` leaves the countdown where it is: a timer that never
    /// stops by itself.
    endless: bool,
    backs: u32,
    name: String,
}

impl Application for Nav {
    type Message = Msg;

    fn setup(&mut self, setup: &mut Setup<'_>) {
        setup.add_font(REGULAR);
    }

    fn view(&self) -> Element<Msg> {
        let screen = *self.stack.last().unwrap();
        col![
            Text::new(format!("{screen:?} {}", self.left)).id("title"),
            Button::new("Open")
                .on_press(Msg::Open(Screen::Detail))
                .size(200.0, 50.0)
                .id("open"),
            TextInput::new(&self.name).on_input(Msg::Field).id("name"),
        ]
        .push_maybe((self.left == 1).then(|| Text::new("Last second").id("last second")))
        .padding(20.0)
        .into()
    }

    fn update(&mut self, message: Msg) {
        match message {
            Msg::Open(screen) => self.stack.push(screen),
            Msg::Start(seconds) => self.left = seconds,
            Msg::Second => {
                self.ticks += 1;
                if !self.endless {
                    self.left = self.left.saturating_sub(1);
                }
            }
            Msg::Field(name) => self.name = name,
        }
    }

    fn back(&mut self) -> bool {
        self.backs += 1;
        if self.stack.len() > 1 {
            self.stack.pop();
            true
        } else {
            false
        }
    }

    fn tick(&self) -> Option<(Duration, Msg)> {
        (self.left > 0).then_some((self.interval, Msg::Second))
    }
}

struct Harness {
    ui: Ui<Nav>,
    frame: Frame,
    now: Duration,
}

impl Harness {
    fn new() -> Self {
        let mut ui = Ui::new(Nav {
            stack: vec![Screen::Home],
            left: 0,
            interval: Duration::from_secs(1),
            ticks: 0,
            endless: false,
            backs: 0,
            name: String::new(),
        });
        let frame = Frame::for_app(&mut ui);
        let mut harness = Self {
            ui,
            frame,
            now: Duration::ZERO,
        };
        harness.draw();
        harness
    }

    /// An endless one-second timer, started by the frame at time zero.
    fn ticking() -> Self {
        let mut harness = Self::new();
        harness.ui.app_mut().endless = true;
        harness.ui.send(Msg::Start(1));
        harness.draw();
        harness
    }

    /// Builds a frame at the current time.
    fn draw(&mut self) {
        self.frame.begin();
        let info = FrameInfo::new(SurfaceSize::new(1080, 2400))
            .with_scale(3.0)
            .with_elapsed(self.now);
        self.ui.frame(&info, &mut self.frame);
    }

    /// Advances the foreground clock and builds a frame.
    fn step(&mut self, millis: u64) {
        self.now += Duration::from_millis(millis);
        self.draw();
    }

    /// What a host does while idle: sleeps for exactly as long as the last
    /// frame asked, then builds the frame. Panics if no frame was asked
    /// for. Returns how long it slept.
    fn sleep_until_due(&mut self) -> Duration {
        let delay = self.frame.redraw_after().expect("a timed frame is due");
        self.now += delay;
        self.draw();
        delay
    }

    /// True if the last frame asked for nothing: the host sleeps until
    /// the next input.
    fn idle(&self) -> bool {
        !self.frame.redraw_requested() && self.frame.redraw_after().is_none()
    }

    fn tap(&mut self, id: &str) {
        let centre = self.ui.rect_of(id).expect("on screen").center();
        for phase in [PointerPhase::Down, PointerPhase::Up] {
            self.ui.pointer(&PointerEvent {
                id: 1,
                phase,
                position: Point::new(centre.x, centre.y),
            });
            self.draw();
        }
    }

    fn top(&self) -> Screen {
        *self.ui.app().stack.last().unwrap()
    }

    fn ticks(&self) -> u32 {
        self.ui.app().ticks
    }
}

// --- Back -------------------------------------------------------------------

#[test]
fn back_pops_the_navigation_stack_and_then_declines() {
    let mut h = Harness::new();
    h.ui.send(Msg::Open(Screen::Detail));
    h.ui.send(Msg::Open(Screen::Settings));
    h.draw();

    assert!(h.ui.back(), "handled: went from Settings to Detail");
    assert_eq!(h.top(), Screen::Detail);
    assert!(h.ui.back());
    assert_eq!(h.top(), Screen::Home);
    assert!(!h.ui.back(), "nothing left: the system leaves the app");
    assert_eq!(h.top(), Screen::Home);
    assert_eq!(h.ui.app().backs, 3);
}

#[test]
fn back_reaches_the_application_through_the_runtime_trait() {
    let mut h = Harness::new();
    h.ui.send(Msg::Open(Screen::Detail));
    // What the engine calls for the host.
    let app: &mut dyn App = &mut h.ui;
    assert!(app.back());
    assert!(!app.back());
    assert_eq!(h.top(), Screen::Home);
}

#[test]
fn an_application_without_back_lets_the_system_leave() {
    struct Plain;
    impl Application for Plain {
        type Message = ();
        fn view(&self) -> Element<()> {
            col![].into()
        }
        fn update(&mut self, (): ()) {}
    }
    let mut ui = Ui::new(Plain);
    assert!(!ui.back());
    let mut frame = Frame::for_app(&mut ui);
    frame.begin();
    ui.frame(&FrameInfo::new(SurfaceSize::new(100, 100)), &mut frame);
    assert_eq!(frame.redraw_after(), None, "and has no timer");
}

#[test]
fn a_handled_back_abandons_the_press_in_progress() {
    let mut h = Harness::new();
    h.ui.send(Msg::Open(Screen::Detail));
    h.draw();
    let centre = h.ui.rect_of("open").unwrap().center();
    let touch = |phase| PointerEvent {
        id: 1,
        phase,
        position: Point::new(centre.x, centre.y),
    };
    h.ui.pointer(&touch(PointerPhase::Down));
    h.draw();
    assert!(h.ui.back());
    // Before any new frame, as when the finger lifts right after Back.
    h.ui.pointer(&touch(PointerPhase::Up));
    h.draw();
    assert_eq!(
        h.ui.app().stack,
        [Screen::Home],
        "the button did not fire on the screen Back left"
    );
}

#[test]
fn a_declined_back_leaves_the_press_in_progress_alone() {
    let mut h = Harness::new();
    let centre = h.ui.rect_of("open").unwrap().center();
    let touch = |phase| PointerEvent {
        id: 1,
        phase,
        position: Point::new(centre.x, centre.y),
    };
    h.ui.pointer(&touch(PointerPhase::Down));
    h.draw();
    assert!(!h.ui.back());
    h.ui.pointer(&touch(PointerPhase::Up));
    h.draw();
    assert_eq!(h.top(), Screen::Detail);
}

// --- Tick -------------------------------------------------------------------

#[test]
fn an_application_without_a_running_timer_is_idle() {
    let mut h = Harness::new();
    assert!(h.idle(), "tick is None: no frame is asked for");
    h.step(5_000);
    h.tap("open");
    assert!(h.idle(), "input does not start anything");
    assert_eq!(h.ticks(), 0);
}

#[test]
fn ticks_arrive_at_the_interval_at_one_frame_each() {
    let mut h = Harness::ticking();
    assert_eq!(h.ticks(), 0, "the first is one interval away");
    assert_eq!(h.frame.redraw_after(), Some(Duration::from_secs(1)));
    assert!(
        !h.frame.redraw_requested(),
        "no frame at the next refresh: the host sleeps"
    );

    for second in 1..=5 {
        assert_eq!(h.sleep_until_due(), Duration::from_secs(1));
        assert_eq!(h.ticks(), second, "one message per frame per second");
        assert!(!h.frame.redraw_requested());
    }
    assert_eq!(h.now, Duration::from_secs(5), "five frames in five seconds");
}

#[test]
fn a_frame_before_the_tick_is_due_delivers_nothing_and_asks_for_the_rest() {
    let mut h = Harness::ticking();
    // A touch, say, 400 ms in.
    h.step(400);
    assert_eq!(h.ticks(), 0);
    assert_eq!(h.frame.redraw_after(), Some(Duration::from_millis(600)));
    h.step(599);
    assert_eq!(h.ticks(), 0);
    assert_eq!(h.frame.redraw_after(), Some(Duration::from_millis(1)));
    h.step(1);
    assert_eq!(h.ticks(), 1);
    assert_eq!(h.frame.redraw_after(), Some(Duration::from_secs(1)));
}

#[test]
fn frames_that_come_a_little_late_do_not_make_the_timer_drift() {
    let mut h = Harness::ticking();
    // Each frame arrives 30 ms after it was due.
    h.step(1_030);
    assert_eq!(h.ticks(), 1);
    assert_eq!(
        h.frame.redraw_after(),
        Some(Duration::from_millis(970)),
        "the next is due at 2 s, not at 2.03 s"
    );
    h.step(1_000);
    assert_eq!(h.ticks(), 2);
    assert_eq!(h.frame.redraw_after(), Some(Duration::from_millis(970)));
}

#[test]
fn ticks_owed_after_a_long_gap_arrive_once_not_in_a_burst() {
    let mut h = Harness::ticking();
    // Ten and a half seconds with no frame at all.
    h.step(10_500);
    assert_eq!(h.ticks(), 1, "one message, not ten");
    assert_eq!(
        h.frame.redraw_after(),
        Some(Duration::from_secs(1)),
        "and the rhythm starts again from now"
    );
    // Frames right after it deliver nothing more.
    h.step(16);
    h.step(16);
    assert_eq!(h.ticks(), 1);
    assert_eq!(h.frame.redraw_after(), Some(Duration::from_millis(968)));
    h.sleep_until_due();
    assert_eq!(h.ticks(), 2);
}

#[test]
fn a_tick_is_delivered_at_most_once_per_frame() {
    let mut h = Harness::new();
    h.ui.app_mut().endless = true;
    h.ui.app_mut().interval = Duration::from_millis(1);
    h.ui.send(Msg::Start(1));
    h.draw();
    // Frames 16 ms apart with a 1 ms interval.
    for frame in 1..=5 {
        h.step(16);
        assert_eq!(h.ticks(), frame);
    }
}

#[test]
fn a_countdown_stops_itself_and_the_app_goes_idle() {
    let mut h = Harness::new();
    h.ui.send(Msg::Start(3));
    h.draw();
    assert!(!h.idle());

    h.sleep_until_due();
    assert_eq!(h.ui.app().left, 2);
    h.sleep_until_due();
    assert_eq!(h.ui.app().left, 1);
    assert!(!h.idle());
    h.sleep_until_due();
    assert_eq!(h.ui.app().left, 0);
    assert!(
        h.idle(),
        "the frame that delivered the last tick asks for nothing"
    );
    assert_eq!(h.ticks(), 3);

    // Later frames, whatever causes them, deliver nothing.
    h.step(10_000);
    assert_eq!(h.ticks(), 3);
    assert!(h.idle());
}

#[test]
fn the_frame_that_delivers_a_tick_shows_its_result() {
    let mut h = Harness::new();
    h.ui.send(Msg::Start(2));
    h.draw();
    assert!(h.ui.rect_of("last second").is_none());
    h.sleep_until_due();
    assert!(
        h.ui.rect_of("last second").is_some(),
        "the view of the same frame was built from the updated state"
    );
}

#[test]
fn a_timer_started_again_waits_a_full_interval() {
    let mut h = Harness::new();
    h.ui.send(Msg::Start(1));
    h.draw();
    h.sleep_until_due();
    assert!(h.idle());

    // Started again 300 ms into what would have been the next interval.
    h.step(300);
    h.ui.send(Msg::Start(1));
    h.draw();
    assert_eq!(h.ticks(), 1, "nothing is owed from while it was stopped");
    assert_eq!(h.frame.redraw_after(), Some(Duration::from_secs(1)));
}

#[test]
fn a_changed_interval_starts_the_wait_again() {
    let mut h = Harness::ticking();
    h.step(900);
    h.ui.app_mut().interval = Duration::from_millis(250);
    h.draw();
    assert_eq!(h.ticks(), 0);
    assert_eq!(h.frame.redraw_after(), Some(Duration::from_millis(250)));
    h.sleep_until_due();
    assert_eq!(h.ticks(), 1);
}

#[test]
fn the_timer_and_the_caret_share_the_timed_frame() {
    let mut h = Harness::ticking();
    h.tap("name");
    assert!(h.ui.has_focus());
    // The caret wants a frame in 500 ms, the timer in 1 s: the sooner wins,
    // and each frame asks again for whichever is next.
    assert_eq!(h.sleep_until_due(), Duration::from_millis(500));
    assert_eq!(h.ticks(), 0);
    assert_eq!(h.sleep_until_due(), Duration::from_millis(500));
    assert_eq!(h.ticks(), 1);
}

#[test]
fn back_and_the_timer_work_together() {
    let mut h = Harness::ticking();
    h.ui.send(Msg::Open(Screen::Detail));
    h.step(400);
    assert!(h.ui.back());
    h.draw();
    assert_eq!(h.top(), Screen::Home);
    assert_eq!(h.frame.redraw_after(), Some(Duration::from_millis(600)));
}
