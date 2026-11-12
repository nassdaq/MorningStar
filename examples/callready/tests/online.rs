//! CallReady with a server behind it, driven the way a phone drives it.
//!
//! As in `tests/e2e.rs`, every step is a pointer event at a position taken
//! from the last frame's layout, a text event from the keyboard, the
//! system's Back or time passing, with a frame after each. What is new is
//! the other side: the application's requests to the platform are held by
//! the test driver (`Ui::testing`) and the test answers each one as the
//! server, the microphone or the speaker would (`server/API.md` is the
//! contract the answers follow). No network is used.
//!
//! Every frame is checked for layout overflow, on the owner's Galaxy A27
//! (1080 x 2340 px at 2.8125, insets 102 and 42 px) and on 360 x 640 dp.
//!
//! One test at the bottom is ignored by default: it plays a typed call
//! against a real server on 127.0.0.1:8787, carrying each request there
//! with `curl`.

use std::hash::Hash;
use std::time::Duration;

use callready::model::{Accent, Detail, Difficulty, Length, Link, Live, Model};
use callready::online::{OnlineCall, Phase, Sound};
use callready::script::Scenario;
use callready::{Answer, CallReady, SERVER_KEY, Screen, Tab};
use morningstar::ui::Ui;
use morningstar::{
    App, AudioClip, AudioError, Frame, FrameInfo, HostState, HttpError, HttpMethod, HttpRequest,
    HttpResponse, ImeAction, Insets, Point, PointerEvent, PointerPhase, Recording, Rect, ServiceId,
    ServiceRequest, ServiceResult, SurfaceSize, TextInputEvent,
};

// ---------------------------------------------------------------- frames

fn galaxy_a27() -> FrameInfo {
    FrameInfo::new(SurfaceSize::new(1080, 2340))
        .with_scale(2.8125)
        .with_insets(Insets::new(0.0, 102.0, 0.0, 42.0))
}

fn small() -> FrameInfo {
    FrameInfo::new(SurfaceSize::new(720, 1280)).with_scale(2.0)
}

fn phones() -> [(&'static str, FrameInfo); 2] {
    [("galaxy a27", galaxy_a27()), ("360x640", small())]
}

// ------------------------------------------------------------ the server

const INSTALL: &str = "0123456789abcdef0123456789abcdef";
const BASE: &str = "http://127.0.0.1:8787";
const NEW_CALL: &str = "c_00000000000000000000000000000004";
const OLD_CALL: &str = "c_00000000000000000000000000000003";
const OLDER_CALL: &str = "c_00000000000000000000000000000002";
const OPENING_AUDIO: &str = "/v1/audio/a_000000000000000000000001.mp3";
const REPLY_AUDIO: &str = "/v1/audio/a_000000000000000000000002.mp3";

fn json(body: impl Into<String>) -> Answer {
    Ok(HttpResponse::new(200)
        .with_header("Content-Type", "application/json; charset=utf-8")
        .with_body(body.into().into_bytes()))
}

fn error(status: u16, code: &str, message: &str) -> Answer {
    Ok(HttpResponse::new(status)
        .with_header("Content-Type", "application/json; charset=utf-8")
        .with_body(
            format!(r#"{{"error": {{"code": "{code}", "message": "{message}"}}}}"#).into_bytes(),
        ))
}

fn mp3(bytes: &[u8]) -> Answer {
    Ok(HttpResponse::new(200)
        .with_header("Content-Type", "audio/mpeg")
        .with_body(bytes.to_vec()))
}

fn health(voice: bool) -> String {
    format!(r#"{{"ok": true, "model": true, "voice": {voice}, "database": true}}"#)
}

fn profile(name: &str, accent: &str, plan: &str, calls: u32) -> String {
    let average = if calls == 0 {
        "null".to_owned()
    } else {
        "84".to_owned()
    };
    format!(
        r#"{{"name": "{name}", "accent": "{accent}", "difficulty": "standard", "minutes": 4,
            "plan": "{plan}", "calls": {calls}, "average_score": {average}, "streak_days": {calls}}}"#
    )
}

fn summary(id: &str, number: u32, score: u8) -> String {
    format!(
        r#"{{"call_id": "{id}", "number": {number}, "scenario": "refund_request",
            "title": "Refund request", "started_at": "2026-11-0{number}T09:14:03Z",
            "duration_seconds": 187, "minutes": 4, "score": {score}}}"#
    )
}

fn calls(rows: &[String]) -> String {
    format!(r#"{{"calls": [{}]}}"#, rows.join(","))
}

fn quoted(value: Option<&str>) -> String {
    value.map_or("null".to_owned(), |value| format!("\"{value}\""))
}

/// `POST /v1/calls` for "Angry customer", as API.md shows it.
fn call_start(voice: bool) -> String {
    format!(
        r#"{{"call_id": "{NEW_CALL}", "number": 4, "scenario": "angry_customer",
            "title": "Angry customer", "caller_name": "Dana",
            "goal": "Let the caller be heard, then fix it",
            "accent": "us", "difficulty": "standard", "minutes": 4, "voice": {voice},
            "line": {{"text": "This is the third time I’ve called today!", "audio_url": {}}},
            "tips": ["Take ownership", "Ask what happened"]}}"#,
        quoted(voice.then_some(OPENING_AUDIO))
    )
}

fn turn(heard: &str, line: &str, next: u32, done: bool, audio: Option<&str>) -> String {
    format!(
        r#"{{"heard": "{heard}", "line": {{"text": "{line}", "audio_url": {}}},
            "tips": ["Give a clear next step", "Confirm the details"],
            "turn": {next}, "done": {done}}}"#,
        quoted(audio)
    )
}

fn report(id: &str, number: u32, scored: bool) -> String {
    let (score, skills, did_well, try_next) = if scored {
        (
            "82",
            r#"{"clarity": 88, "empathy": 74, "listening": 85, "problem_solving": 79, "professional_language": 84}"#,
            "You apologised early and kept a calm tone.",
            "Ask for the order number before explaining.",
        )
    } else {
        (
            "null",
            "null",
            "You picked up the call.",
            "Say a first line: a greeting is enough to begin.",
        )
    };
    let learner = if scored {
        r#", {"speaker": "learner", "text": "I’m sorry about that, let me look into it.", "note": "Took ownership"},
            {"speaker": "caller", "text": "Fine. But I want it fixed today."},
            {"speaker": "learner", "text": "I will stay with you until it is fixed."}"#
    } else {
        ""
    };
    format!(
        r#"{{"call_id": "{id}", "number": {number}, "scenario": "angry_customer",
            "title": "Angry customer", "caller_name": "Dana",
            "started_at": "2026-11-12T09:14:03Z", "duration_seconds": 187,
            "score": {score}, "skills": {skills},
            "did_well": "{did_well}", "try_next": "{try_next}",
            "transcript": [{{"speaker": "caller", "text": "This is the third time I’ve called today!"}}{learner}]}}"#
    )
}

fn recording(seconds: u64) -> Recording {
    Recording {
        bytes: vec![0, 0, 0, 24, b'f', b't', b'y', b'p', 1, 2, 3, 255],
        mime: "audio/mp4".to_owned(),
        duration: Duration::from_secs(seconds),
    }
}

// ---------------------------------------------------------------- device

/// The framework's touch slop, in dp.
const TOUCH_SLOP: f32 = 8.0;

struct Phone {
    ui: Ui<CallReady>,
    frame: Frame,
    info: FrameInfo,
    elapsed: Duration,
}

impl Phone {
    /// The app as a phone launches it for the first time, with a host
    /// behind it: the test.
    fn launch(info: FrameInfo) -> Self {
        Self::launch_with(
            info,
            &HostState::new().with_install_id(INSTALL),
            CallReady::default(),
        )
    }

    fn launch_with(info: FrameInfo, host: &HostState, app: CallReady) -> Self {
        let mut ui = Ui::testing(app);
        let frame = Frame::for_app_with_host(&mut ui, host);
        let mut phone = Self {
            ui,
            frame,
            info,
            elapsed: Duration::ZERO,
        };
        phone.draw();
        phone
    }

    /// The app online and on Home: the server has answered the three
    /// questions of start-up, with a history of `history` calls (newest
    /// first), and the report of the newest in full.
    fn online(info: FrameInfo, voice: bool, history: &[(&str, u32)]) -> Self {
        let mut phone = Self::launch(info);
        phone.connect(voice, history);
        phone.tap("welcome-start");
        assert_eq!(phone.screen(), Screen::Home);
        phone
    }

    /// Answers a check of the server: health, profile, history, and the
    /// newest call's report.
    fn connect(&mut self, voice: bool, history: &[(&str, u32)]) {
        self.reply(HttpMethod::Get, "/v1/health", json(health(voice)));
        self.reply(
            HttpMethod::Get,
            "/v1/profile",
            json(profile("Asha Mrema", "us", "free", history.len() as u32)),
        );
        let rows: Vec<String> = history
            .iter()
            .map(|(id, number)| summary(id, *number, 70 + *number as u8))
            .collect();
        self.reply(HttpMethod::Get, "/v1/calls", json(calls(&rows)));
        if let Some((id, number)) = history.first()
            && self
                .model()
                .history
                .first()
                .is_some_and(|report| !report.has_skills())
        {
            self.reply(
                HttpMethod::Get,
                &format!("/v1/calls/{id}"),
                json(report(id, *number, true)),
            );
        }
        self.quiet();
    }

    fn draw(&mut self) {
        self.frame.begin();
        let info = self.info.with_elapsed(self.elapsed);
        self.ui.frame(&info, &mut self.frame);
        // Nothing may overflow, in any frame: a long reply scrolls inside
        // its three-line field (`live_call::reply_field`).
        assert_eq!(
            self.ui.overflows(),
            [],
            "layout overflow on {}",
            self.screen().name()
        );
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

    /// The call the server is playing.
    fn call(&self) -> &OnlineCall {
        self.model().online_call().expect("an online call")
    }

    fn scale(&self) -> f32 {
        self.info.scale
    }

    // ------------------------------------------------------------ fingers

    fn rect(&self, id: impl Hash) -> Rect {
        self.ui.rect_of(id).expect("the widget is on screen")
    }

    fn shows(&self, id: impl Hash) -> bool {
        self.ui.rect_of(id).is_some()
    }

    /// The scroll view of the screen being shown, in pixels.
    fn viewport(&self) -> Rect {
        self.rect(self.screen().scroll_id())
    }

    fn scroll(&self) -> (f32, f32) {
        let name = self.screen().scroll_id();
        let (offset, max) = (self.ui.scroll_offset(name), self.ui.max_scroll_offset(name));
        (offset.expect("a scroll view"), max.expect("a scroll view"))
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
    /// by `dp`. Returns how far it scrolled.
    fn drag(&mut self, dp: f32) -> f32 {
        let (before, max) = self.scroll();
        if max <= 0.0 || dp == 0.0 {
            return 0.0;
        }
        let view = self.viewport();
        let direction = dp.signum();
        let room = view.height * 0.6 - (TOUCH_SLOP + 2.0) * self.scale();
        let travel = (dp.abs() * self.scale()).min(room.max(1.0));
        let x = view.x + view.width / 2.0;
        let start = view.y + view.height * if direction > 0.0 { 0.8 } else { 0.2 };
        self.pointer(PointerPhase::Down, Point::new(x, start));
        let anchor = start - direction * (TOUCH_SLOP + 2.0) * self.scale();
        self.pointer(PointerPhase::Moved, Point::new(x, anchor));
        for step in 1..=3 {
            let y = anchor - direction * travel * step as f32 / 3.0;
            self.pointer(PointerPhase::Moved, Point::new(x, y));
        }
        self.pointer(PointerPhase::Up, Point::new(x, anchor - direction * travel));
        self.scroll().0 - before
    }

    /// Scrolls until the widget lies wholly inside the scroll view, if it
    /// is in it. Where it was laid out is known even when it is out of
    /// sight (`Ui::rect_of`), so the finger drags straight towards it.
    fn reveal(&mut self, id: impl Hash + Copy) {
        for _ in 0..40 {
            let rect = self.rect(id);
            let view = self.viewport();
            if view.height < 80.0 * self.scale() {
                // Too little of the scroll view shows to drag in (the
                // keyboard is up): what is tapped then is in a fixed bar.
                return;
            }
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
                return;
            }
        }
        panic!("the widget could not be scrolled into view");
    }

    /// A tap: finger down and up on the widget's middle, which must be on
    /// the visible screen and not cut by the scroll view's edge.
    fn tap(&mut self, id: impl Hash + Copy) {
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
        let inside = rect.y >= view.y - 0.5 && rect.y + rect.height <= view.y + view.height + 0.5;
        let outside = rect.y + rect.height <= view.y + 0.5 || rect.y >= view.y + view.height - 0.5;
        assert!(
            inside || outside,
            "{}: {rect:?} is cut by the edge of the scroll view {view:?}",
            self.screen().name()
        );
        self.pointer(PointerPhase::Down, point);
        self.pointer(PointerPhase::Up, point);
    }

    fn tab(&mut self, tab: Tab) {
        self.tap(("tab", tab));
    }

    fn back(&mut self) -> bool {
        let handled = self.ui.back();
        self.draw();
        handled
    }

    /// Lets `seconds` pass, a frame each second.
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

    /// Empties the focused field, wherever in it the tap put the caret.
    fn clear_field(&mut self) {
        for _ in 0..64 {
            self.key(TextInputEvent::DeleteBackward);
            self.key(TextInputEvent::DeleteForward);
        }
    }

    fn type_text(&mut self, text: &str) {
        for character in text.chars() {
            self.key(TextInputEvent::Commit(character.to_string()));
        }
    }

    /// Types a reply into the call's field.
    fn type_reply(&mut self, text: &str) {
        self.tap("reply-input");
        assert!(self.ui.is_focused("reply-input"));
        assert!(
            self.frame.text_input().is_some(),
            "the keyboard is asked for"
        );
        self.type_text(text);
        assert_eq!(self.call().draft, text);
    }

    // ------------------------------------------------- the other side

    /// What the application is waiting for from the microphone and the
    /// speaker, oldest first.
    fn pending(&self) -> Vec<ServiceRequest> {
        self.ui
            .pending_effects()
            .iter()
            .map(|call| call.request.clone())
            .filter(|request| !matches!(request, ServiceRequest::Http(_)))
            .collect()
    }

    /// The HTTP requests the application is waiting for, oldest first.
    fn requests(&self) -> Vec<(ServiceId, HttpRequest)> {
        self.ui
            .pending_effects()
            .iter()
            .filter_map(|call| match &call.request {
                ServiceRequest::Http(request) => Some((call.id, request.clone())),
                _ => None,
            })
            .collect()
    }

    /// The application is waiting for nothing.
    fn quiet(&self) {
        assert_eq!(self.ui.pending_effects(), [], "requests nobody answered");
    }

    /// The oldest waiting request of this method to this path, which the
    /// application must be waiting for, from this installation.
    fn find(&self, method: HttpMethod, path: &str) -> (ServiceId, HttpRequest) {
        let url = format!("{BASE}{path}");
        let requests = self.requests();
        let found = requests
            .iter()
            .find(|(_, request)| request.method == method && request.url == url)
            .unwrap_or_else(|| {
                panic!(
                    "no {} {path} is waiting; waiting: {:?}",
                    method.as_str(),
                    requests
                        .iter()
                        .map(|(_, request)| format!("{} {}", request.method.as_str(), request.url))
                        .collect::<Vec<_>>()
                )
            });
        assert_eq!(found.1.header_value("X-Device-Id"), Some(INSTALL));
        assert_eq!(found.1.header_value("Authorization"), None);
        found.clone()
    }

    fn expect(&self, method: HttpMethod, path: &str) -> HttpRequest {
        self.find(method, path).1
    }

    /// Answers that request. Returns the request.
    fn reply(&mut self, method: HttpMethod, path: &str, answer: Answer) -> HttpRequest {
        let (id, request) = self.find(method, path);
        assert!(self.ui.resolve(id, ServiceResult::Http(answer)));
        self.draw();
        request
    }

    fn body(request: &HttpRequest) -> &str {
        std::str::from_utf8(&request.body).expect("a text body")
    }

    /// Starts "Angry customer" from Home and has the server connect it.
    fn start_call(&mut self, voice: bool) {
        self.tap(("home-scenario", 0usize));
        assert_eq!(self.screen(), Screen::LiveCall);
        let request = self.reply(HttpMethod::Post, "/v1/calls", json(call_start(voice)));
        assert_eq!(Self::body(&request), r#"{"scenario":"angry_customer"}"#);
        assert_eq!(self.call().id, NEW_CALL);
    }

    /// The opening line's audio arrives and is played to the end.
    fn hear_opening(&mut self) {
        self.reply(HttpMethod::Get, OPENING_AUDIO, mp3(b"opening"));
        assert_eq!(
            self.pending(),
            [ServiceRequest::Play(AudioClip::new(
                b"opening".to_vec(),
                "audio/mpeg"
            ))]
        );
        assert_eq!(self.call().sound, Sound::Speaking);
        assert!(self.ui.answer_playback(Ok(())));
        self.draw();
        assert_eq!(self.call().sound, Sound::Silent);
    }

    /// Taps the microphone, speaks for `seconds`, taps again: the turn is
    /// with the server. Returns the request.
    fn say(&mut self, seconds: u32) -> HttpRequest {
        self.tap("talk");
        assert_eq!(self.pending(), [ServiceRequest::StartRecording]);
        assert!(self.ui.answer_recording_started(Ok(())));
        self.draw();
        self.wait(seconds);
        assert_eq!(self.call().phase, Phase::Recording(seconds));
        self.tap("talk");
        assert_eq!(self.pending(), [ServiceRequest::StopRecording]);
        assert!(self.ui.answer_recording(Ok(recording(u64::from(seconds)))));
        self.draw();
        self.expect(HttpMethod::Post, &format!("/v1/calls/{NEW_CALL}/turns"))
    }

    /// Types `text` and taps send: the turn is with the server.
    fn send(&mut self, text: &str) {
        self.type_reply(text);
        self.tap("reply-send");
        assert_eq!(self.call().phase, Phase::Sending { audio: false });
    }

    fn turns_path() -> String {
        format!("/v1/calls/{NEW_CALL}/turns")
    }

    fn end_path() -> String {
        format!("/v1/calls/{NEW_CALL}/end")
    }

    /// The refresh of the numbers and the history after a scored call.
    fn refresh(&mut self, history: &[(&str, u32)]) {
        self.reply(
            HttpMethod::Get,
            "/v1/profile",
            json(profile("Asha Mrema", "us", "free", history.len() as u32)),
        );
        let rows: Vec<String> = history
            .iter()
            .map(|(id, number)| summary(id, *number, 70 + *number as u8))
            .collect();
        self.reply(HttpMethod::Get, "/v1/calls", json(calls(&rows)));
        self.quiet();
    }
}

// ---------------------------------------------------------------- start-up

/// Health, then the profile, then the history: only when all three have
/// answered is the app online, and then everything it shows about the
/// learner is the server's.
#[test]
fn start_up_asks_the_server_and_takes_its_word() {
    for (name, info) in phones() {
        let mut phone = Phone::launch(info);
        assert_eq!(phone.screen(), Screen::Welcome, "{name}");
        assert_eq!(phone.app().device_id(), INSTALL);
        assert_eq!(phone.model().link, Link::Checking);
        // The sample learner, until the server says who this is.
        assert_eq!(phone.model().name, "Grace Mushi");

        let health_request = phone.expect(HttpMethod::Get, "/v1/health");
        assert!(health_request.timeout <= Duration::from_secs(10));
        assert_eq!(phone.requests().len(), 1, "one question at a time");
        phone.reply(HttpMethod::Get, "/v1/health", json(health(false)));
        assert_eq!(phone.model().link, Link::Checking, "not online yet");
        phone.reply(
            HttpMethod::Get,
            "/v1/profile",
            json(profile("Asha Mrema", "uk", "premium", 2)),
        );
        assert_eq!(phone.model().name, "Grace Mushi", "nor yet");
        phone.reply(
            HttpMethod::Get,
            "/v1/calls",
            json(calls(&[
                summary(OLD_CALL, 3, 91),
                summary(OLDER_CALL, 2, 77),
            ])),
        );

        let model = phone.model().clone();
        assert_eq!(model.link, Link::Online { voice: false }, "{name}");
        assert_eq!(model.name, "Asha Mrema");
        assert_eq!(model.settings.accent, Accent::Uk);
        assert!(model.premium);
        assert_eq!((model.calls(), model.average(), model.streak()), (2, 84, 2));
        assert_eq!(model.next_call_number(), 3);
        assert_eq!(model.history.len(), 2);
        assert_eq!(
            (model.history[0].title(), model.history[0].overall()),
            ("Refund request".to_owned(), 91)
        );
        assert_eq!(model.history[0].day(), "3 Nov");
        assert_eq!(model.history[0].call_id(), Some(OLD_CALL));
        assert_eq!(
            model.notifications()[0].body,
            "Refund request \u{b7} 91/100"
        );

        // The newest report is fetched in full, quietly: the progress tab
        // and the bell's note lead to it.
        phone.reply(
            HttpMethod::Get,
            &format!("/v1/calls/{OLD_CALL}"),
            json(report(OLD_CALL, 3, true)),
        );
        phone.quiet();
        assert!(phone.model().history[0].has_skills());
        assert!(!phone.model().history[1].has_skills());

        phone.tap("welcome-start");
        assert_eq!(phone.screen(), Screen::Home);
        phone.quiet();

        // Home, the profile, progress and the menu show the server's
        // learner.
        phone.tap("home-profile");
        assert_eq!(phone.screen(), Screen::Profile);
        assert!(phone.back());
        phone.tab(Tab::Progress);
        assert_eq!(
            phone.model().skills().0,
            Some("SKILLS \u{b7} LATEST REPORT")
        );
        assert_eq!(phone.model().skills().1.0, [88, 74, 85, 79, 84]);
        phone.tab(Tab::More);
        assert_eq!(phone.model().link.status(), "connected \u{b7} text mode");
        phone.tab(Tab::Home);
        phone.quiet();
    }
}

#[test]
fn a_device_seen_for_the_first_time_has_nothing_yet() {
    let mut phone = Phone::launch(galaxy_a27());
    phone.reply(HttpMethod::Get, "/v1/health", json(health(false)));
    phone.reply(
        HttpMethod::Get,
        "/v1/profile",
        json(profile("", "us", "free", 0)),
    );
    phone.reply(HttpMethod::Get, "/v1/calls", json(calls(&[])));
    phone.quiet();
    phone.tap("welcome-start");
    let model = phone.model().clone();
    assert_eq!(model.name, "");
    assert_eq!(model.first_name(), "You");
    assert_eq!((model.calls(), model.average(), model.streak()), (0, 0, 0));
    assert_eq!(model.history, []);
    assert_eq!(model.next_call_number(), 1);
    // Every tab draws with nothing in it.
    for tab in [Tab::Progress, Tab::History, Tab::More, Tab::Home] {
        phone.tab(tab);
    }
    phone.tap("home-notifications");
    assert_eq!(
        phone.model().notifications()[0].body,
        "Finish a call to get one."
    );
    // The bell's first note has no report to open.
    phone.tap(("notification", 0usize));
    assert_eq!(phone.screen(), Screen::Notifications);
    phone.quiet();
}

/// Whatever goes wrong at start-up, the app is the scripted app, says so
/// in a small note, and blocks nothing.
#[test]
fn without_a_server_the_app_is_the_scripted_app() {
    let failures: [(&str, Vec<Answer>, &str); 7] = [
        (
            "offline",
            vec![Err(HttpError::Offline)],
            "The server could not be reached.",
        ),
        (
            "timeout",
            vec![Err(HttpError::Timeout)],
            "The server took too long to answer.",
        ),
        (
            "cleartext refused",
            vec![Err(HttpError::Failed("plain http is not allowed".into()))],
            "The request could not be made: plain http is not allowed.",
        ),
        (
            "not a callready server",
            vec![Ok(
                HttpResponse::new(200).with_body(b"<html>It works!</html>".to_vec())
            )],
            "The server sent something this app does not understand.",
        ),
        (
            "no model key",
            vec![json(
                r#"{"ok": false, "model": false, "voice": false, "database": true}"#,
            )],
            "The server has no AI model key.",
        ),
        (
            "profile fails",
            vec![
                json(health(true)),
                error(500, "internal_error", "Something went wrong."),
            ],
            "The server had a problem (500).",
        ),
        (
            "history is not a history",
            vec![
                json(health(true)),
                json(profile("Asha", "us", "free", 1)),
                json(r#"{"calls": [{"call_id": 7}]}"#),
            ],
            "The server sent something this app does not understand.",
        ),
    ];
    for (name, answers, reason) in failures {
        let mut phone = Phone::launch(small());
        for answer in answers {
            assert!(phone.ui.answer_http(answer), "{name}");
            phone.draw();
        }
        phone.quiet();
        let model = phone.model().clone();
        assert_eq!(model.link, Link::Offline(reason.to_owned()), "{name}");
        assert_eq!(model.link.caption(), Some("offline \u{b7} practice script"));
        // Exactly the local model the app has always had.
        assert_eq!(
            (model.name.as_str(), model.history.len(), model.stats),
            ("Grace Mushi", 3, None),
            "{name}"
        );

        // Arriving on home, the server is asked once more, without a word;
        // it is still not there.
        phone.tap("welcome-start");
        assert_eq!(phone.model().link, Link::Checking);
        phone.reply(HttpMethod::Get, "/v1/health", Err(HttpError::Offline));
        assert!(matches!(phone.model().link, Link::Offline(_)));
        phone.quiet();
        // A call is the script: nothing is asked of anybody.
        phone.tap("home-start-call");
        assert_eq!(phone.screen(), Screen::LiveCall);
        assert_eq!(phone.model().live, None);
        assert!(phone.app().call().is_running());
        phone.quiet();
        phone.wait(2);
        phone.tap(("tip", 0usize));
        assert_eq!(phone.app().call().choices, [0]);
        phone.tap("end-call");
        assert_eq!(phone.screen(), Screen::CallReport);
        assert_eq!(phone.model().history.len(), 4);
        assert!(phone.model().report.server.is_none());
        phone.quiet();

        // Back on home the server is asked again, without a word.
        phone.tap("back");
        assert_eq!(phone.screen(), Screen::Home);
        assert_eq!(phone.model().link, Link::Checking);
        phone.expect(HttpMethod::Get, "/v1/health");
        assert_eq!(phone.requests().len(), 1);
        // Still nothing: offline again, and no question while home stays.
        phone.reply(HttpMethod::Get, "/v1/health", Err(HttpError::Offline));
        assert!(matches!(phone.model().link, Link::Offline(_)));
        phone.quiet();
        phone.tab(Tab::More);
        phone.quiet();
        // The menu's note is the scripted one.
        assert!(!phone.model().link.is_online());
        // The next return to home finds the server: online from here on.
        phone.tab(Tab::Home);
        phone.connect(false, &[(OLD_CALL, 3)]);
        assert_eq!(phone.model().link, Link::Online { voice: false });
        assert_eq!(phone.model().name, "Asha Mrema");
        assert_eq!(phone.model().history.len(), 1, "the server's history");
    }
}

// -------------------------------------------------------- a call in text

/// Health to history: a whole typed call.
#[test]
fn a_typed_call_from_health_to_history() {
    for (name, info) in phones() {
        let mut phone = Phone::online(info, false, &[(OLD_CALL, 3), (OLDER_CALL, 2)]);
        assert_eq!(phone.model().link.caption(), Some("live \u{b7} text"));

        // The call rings, then connects with the server's words.
        phone.tap(("home-scenario", 0usize));
        assert_eq!(phone.screen(), Screen::LiveCall, "{name}");
        assert_eq!(
            phone.model().live,
            Some(Live::Dialing(Scenario::AngryCustomer))
        );
        assert!(phone.app().tick().is_none(), "no clock while it rings");
        let request = phone.reply(HttpMethod::Post, "/v1/calls", json(call_start(false)));
        assert_eq!(
            request.header_value("Content-Type"),
            Some("application/json")
        );
        assert_eq!(Phone::body(&request), r#"{"scenario":"angry_customer"}"#);

        let call = phone.call().clone();
        assert_eq!((call.id.as_str(), call.number), (NEW_CALL, 4));
        assert_eq!(
            (call.title.as_str(), call.caller.as_str()),
            ("Angry customer", "Dana")
        );
        assert_eq!(call.goal, "Let the caller be heard, then fix it");
        assert_eq!(
            call.line,
            "This is the third time I\u{2019}ve called today!"
        );
        assert_eq!(call.tips, ["Take ownership", "Ask what happened"]);
        assert!(!call.voice && !call.speaks() && call.can_type());
        assert_eq!((call.clock().as_str(), call.limit), ("00:00", 240));
        phone.quiet();
        // A typed call has a field and a send button, and no microphone.
        assert!(phone.shows("reply-input") && phone.shows("reply-send"));
        assert!(!phone.shows("talk") && !phone.shows("type-instead"));
        assert!(!phone.shows("speak-instead"));
        // The tips are advice: there is nothing to tap.
        assert!(!phone.shows(("tip", 0usize)));

        // The clock counts.
        phone.wait(5);
        assert_eq!(phone.call().clock(), "00:05");

        // Send does nothing with nothing typed.
        phone.tap("reply-send");
        phone.quiet();

        // First turn: typed, sent with the button.
        phone.send("I\u{2019}m sorry about that, let me look into it.");
        let request = phone.expect(HttpMethod::Post, &Phone::turns_path());
        assert_eq!(
            request.header_value("Content-Type"),
            Some("application/json")
        );
        assert_eq!(
            Phone::body(&request),
            "{\"text\":\"I\u{2019}m sorry about that, let me look into it.\"}"
        );
        // While the caller thinks: the words stay, and cannot be sent twice.
        assert_eq!(
            phone.call().draft,
            "I\u{2019}m sorry about that, let me look into it."
        );
        phone.tap("reply-send");
        phone.type_text("x");
        assert_eq!(phone.requests().len(), 1, "one turn at a time");
        assert_eq!(
            phone.call().draft,
            "I\u{2019}m sorry about that, let me look into it."
        );
        phone.wait(2);
        phone.reply(
            HttpMethod::Post,
            &Phone::turns_path(),
            json(turn(
                "I\u{2019}m sorry about that, let me look into it.",
                "Fine. But I want it fixed today.",
                2,
                false,
                None,
            )),
        );
        let call = phone.call().clone();
        assert_eq!(call.phase, Phase::Idle);
        assert_eq!(call.line, "Fine. But I want it fixed today.");
        assert_eq!(
            call.said.as_deref(),
            Some("I\u{2019}m sorry about that, let me look into it.")
        );
        assert_eq!(call.tips, ["Give a clear next step", "Confirm the details"]);
        assert_eq!((call.turns, call.draft.as_str()), (1, ""));
        assert_eq!(call.clock(), "00:07");
        // The new line is in view: the conversation starts at its top.
        assert_eq!(phone.scroll().0, 0.0);
        phone.quiet();

        // Second turn: sent with the keyboard's Send.
        phone.type_reply("I will stay with you until it is fixed.");
        phone.key(TextInputEvent::Action(ImeAction::Send));
        let request = phone.reply(
            HttpMethod::Post,
            &Phone::turns_path(),
            json(turn(
                "I will stay with you until it is fixed.",
                "All right. What do you need from me?",
                3,
                false,
                None,
            )),
        );
        assert_eq!(
            Phone::body(&request),
            r#"{"text":"I will stay with you until it is fixed."}"#
        );
        assert_eq!(phone.call().turns, 2);

        // The loudspeaker has no audio to play in a typed call: the line
        // lights up for two seconds.
        phone.tap("replay");
        assert_eq!(phone.call().highlight, 2);
        phone.quiet();
        phone.wait(2);
        assert_eq!(phone.call().highlight, 0);

        // Third turn: the caller closes, and the call ends itself.
        phone.send("Just the email on your account, please.");
        phone.reply(
            HttpMethod::Post,
            &Phone::turns_path(),
            json(turn(
                "Just the email on your account, please.",
                "It is dana@example.com. Thank you, goodbye.",
                4,
                true,
                None,
            )),
        );
        let call = phone.call().clone();
        assert!(call.done && call.is_over());
        assert_eq!(call.line, "It is dana@example.com. Thank you, goodbye.");
        assert_eq!(phone.screen(), Screen::LiveCall, "the last words show");
        assert!(!phone.shows("reply-input"), "nothing more to say");
        let request = phone.expect(HttpMethod::Post, &Phone::end_path());
        assert!(request.body.is_empty());
        assert_eq!(phone.requests().len(), 1);

        // The report.
        phone.reply(
            HttpMethod::Post,
            &Phone::end_path(),
            json(report(NEW_CALL, 4, true)),
        );
        assert_eq!(phone.screen(), Screen::CallReport, "{name}");
        assert_eq!(phone.app().stack(), [Screen::Home]);
        assert_eq!(phone.model().live, None);
        let shown = phone.model().report.clone();
        assert_eq!(shown.score(), Some(82));
        assert_eq!(shown.scores.0, [88, 74, 85, 79, 84]);
        assert_eq!(
            shown.did_well(),
            "You apologised early and kept a calm tone."
        );
        assert_eq!(
            shown.try_next(),
            "Ask for the order number before explaining."
        );
        assert_eq!(shown.caption(), "Angry customer \u{b7} 3 min");
        assert_eq!(shown.note(), "nice work!");
        // History, the numbers and the bell know at once...
        assert_eq!(phone.model().history.len(), 3);
        assert_eq!(phone.model().history[0], shown);
        assert_eq!(phone.model().calls(), 3);
        assert!(phone.model().bell);
        assert_eq!(
            phone.model().notifications()[0].body,
            "Angry customer \u{b7} 82/100"
        );
        // ...and the server's own figures follow.
        phone.refresh(&[(NEW_CALL, 4), (OLD_CALL, 3), (OLDER_CALL, 2)]);
        assert_eq!(phone.model().calls(), 3);
        assert_eq!(phone.model().history.len(), 3);
        assert_eq!(phone.model().history[0], shown, "the whole report is kept");

        // The transcript, with the trainer's note.
        phone.tap("transcript");
        assert_eq!(phone.screen(), Screen::Transcript);
        let server = phone.model().report.server.clone().expect("a server's");
        assert_eq!(server.transcript.len(), 4);
        assert_eq!(server.transcript[1].note.as_deref(), Some("Took ownership"));
        phone.tap("back");
        assert_eq!(phone.screen(), Screen::CallReport);
        phone.tap("back");
        assert_eq!(phone.screen(), Screen::Home, "not the call that is over");

        // History: three rows; the oldest opens with its score and its
        // details are fetched.
        phone.tab(Tab::History);
        phone.quiet();
        phone.tap(("history-call", 2usize));
        assert_eq!(phone.screen(), Screen::CallReport);
        assert_eq!(phone.model().report.score(), Some(72));
        assert!(!phone.model().report.has_skills());
        phone.reply(
            HttpMethod::Get,
            &format!("/v1/calls/{OLDER_CALL}"),
            json(report(OLDER_CALL, 2, true)),
        );
        assert!(phone.model().report.has_skills());
        assert!(phone.model().history[2].has_skills());
        phone.tap("back");
        // Opened again, nothing is fetched again.
        phone.tap(("history-call", 2usize));
        phone.quiet();
        phone.tap("back");
        // The newest, from the bell.
        phone.tab(Tab::Home);
        phone.tap("home-notifications");
        phone.tap(("notification", 0usize));
        assert_eq!(phone.screen(), Screen::CallReport);
        assert_eq!(phone.model().report, shown);
        phone.quiet();
    }
}

/// The red button ends a call that is going on; a call ended before the
/// learner said anything is not scored, and nothing pretends it was.
#[test]
fn the_red_button_ends_the_call_and_an_unanswered_call_has_no_score() {
    for (name, info) in phones() {
        let mut phone = Phone::online(info, false, &[(OLD_CALL, 3)]);
        phone.start_call(false);
        phone.wait(3);
        phone.tap("end-call");
        assert!(phone.call().is_over(), "{name}");
        assert_eq!(phone.screen(), Screen::LiveCall);
        phone.reply(
            HttpMethod::Post,
            &Phone::end_path(),
            json(report(NEW_CALL, 4, false)),
        );
        assert_eq!(phone.screen(), Screen::CallReport);
        let shown = phone.model().report.clone();
        assert_eq!(shown.score(), None, "no number is made up");
        assert_eq!(shown.did_well(), "You picked up the call.");
        // Not counted, not in the history, no news, nothing more to ask.
        assert_eq!(phone.model().history.len(), 1);
        assert_eq!(phone.model().calls(), 1);
        phone.quiet();
        // Its transcript is the one line that was said.
        phone.tap("transcript");
        phone.tap("back");
        // Practise again starts the same scenario afresh.
        phone.tap("practise-again");
        assert_eq!(phone.screen(), Screen::LiveCall);
        let request = phone.reply(HttpMethod::Post, "/v1/calls", json(call_start(false)));
        assert_eq!(Phone::body(&request), r#"{"scenario":"angry_customer"}"#);
        assert_eq!(phone.app().stack(), [Screen::Home]);

        // One turn, then the red button: a scored call.
        phone.send("I am sorry you had to call again.");
        phone.reply(
            HttpMethod::Post,
            &Phone::turns_path(),
            json(turn(
                "I am sorry you had to call again.",
                "Well?",
                2,
                false,
                None,
            )),
        );
        phone.tap("end-call");
        phone.reply(
            HttpMethod::Post,
            &Phone::end_path(),
            json(report(NEW_CALL, 4, true)),
        );
        assert_eq!(phone.model().report.score(), Some(82));
        assert_eq!(phone.model().history.len(), 2);
        phone.refresh(&[(NEW_CALL, 4), (OLD_CALL, 3)]);
    }
}

// ------------------------------------------------------- a call in voice

/// Health with a voice to the report: a whole spoken call.
#[test]
fn a_spoken_call_from_health_to_the_report() {
    for (name, info) in phones() {
        let mut phone = Phone::online(info, true, &[(OLD_CALL, 3)]);
        assert_eq!(phone.model().link, Link::Online { voice: true }, "{name}");
        assert_eq!(phone.model().link.caption(), Some("live \u{b7} voice"));

        phone.start_call(true);
        let call = phone.call().clone();
        assert!(call.voice && call.speaks());
        assert_eq!(call.sound, Sound::Fetching);
        // A spoken call has the microphone, and "Aa" to type instead.
        assert!(phone.shows("talk") && phone.shows("type-instead"));
        assert!(!phone.shows("reply-input"));

        // The opening line's audio is fetched with the same headers
        // (`expect` checks the device id) and played.
        let request = phone.expect(HttpMethod::Get, OPENING_AUDIO);
        assert!(request.body.is_empty());
        phone.reply(HttpMethod::Get, OPENING_AUDIO, mp3(b"opening"));
        assert_eq!(
            phone.pending(),
            [ServiceRequest::Play(AudioClip::new(
                b"opening".to_vec(),
                "audio/mpeg"
            ))]
        );
        // The line shows while it plays, and the microphone waits.
        assert_eq!(phone.call().sound, Sound::Speaking);
        assert_eq!(
            phone.call().line,
            "This is the third time I\u{2019}ve called today!"
        );
        assert!(!phone.call().mic_ready());
        phone.tap("talk");
        assert_eq!(phone.pending().len(), 1, "no recording over the caller");
        assert_eq!(phone.call().phase, Phase::Idle);
        assert!(phone.ui.answer_playback(Ok(())));
        phone.draw();
        assert!(phone.call().mic_ready());

        // Tap, speak for four seconds, tap: the recording goes as it is,
        // under its own type.
        let request = phone.say(4);
        assert_eq!(request.header_value("Content-Type"), Some("audio/mp4"));
        assert_eq!(request.body, recording(4).bytes);
        assert_eq!(
            request
                .headers
                .iter()
                .filter(|(name, _)| name.eq_ignore_ascii_case("content-type"))
                .count(),
            1
        );
        assert_eq!(phone.call().phase, Phase::Sending { audio: true });
        assert!(!phone.call().mic_ready());
        phone.tap("talk");
        assert_eq!(phone.requests().len(), 1);

        // The caller answers, in words and in sound.
        phone.reply(
            HttpMethod::Post,
            &Phone::turns_path(),
            json(turn(
                "I am sorry you had to call again.",
                "Fine. But I want it fixed today.",
                2,
                false,
                Some(REPLY_AUDIO),
            )),
        );
        assert_eq!(
            phone.call().said.as_deref(),
            Some("I am sorry you had to call again.")
        );
        assert_eq!(phone.call().line, "Fine. But I want it fixed today.");
        phone.reply(HttpMethod::Get, REPLY_AUDIO, mp3(b"reply"));
        assert_eq!(
            phone.pending(),
            [ServiceRequest::Play(AudioClip::new(
                b"reply".to_vec(),
                "audio/mpeg"
            ))]
        );
        assert!(phone.ui.answer_playback(Ok(())));
        phone.draw();
        phone.quiet();

        // Replay plays the same sound again without fetching it.
        phone.tap("replay");
        assert_eq!(
            phone.pending(),
            [ServiceRequest::Play(AudioClip::new(
                b"reply".to_vec(),
                "audio/mpeg"
            ))]
        );
        // Sound off, mid-sentence: playback is stopped (the speaker says
        // so), and the next line is not fetched at all.
        phone.tap("mute");
        assert!(phone.call().muted);
        assert_eq!(phone.call().sound, Sound::Silent);
        // (The stand-in speaker ends the clip as a real one does: its
        // message says it was interrupted, which is not news.)
        assert_eq!(phone.pending(), [], "the clip was stopped");
        assert_eq!(phone.call().notice, None);
        let _ = phone.say(2);
        phone.reply(
            HttpMethod::Post,
            &Phone::turns_path(),
            json(turn(
                "Let me fix it now.",
                "Go on, then.",
                3,
                false,
                Some(REPLY_AUDIO),
            )),
        );
        assert_eq!(phone.call().line, "Go on, then.", "the line still shows");
        phone.quiet();
        phone.tap("replay");
        assert_eq!(phone.call().highlight, 2);
        phone.quiet();
        // Sound on again: replay fetches the line and plays it.
        phone.tap("mute");
        phone.tap("replay");
        phone.reply(HttpMethod::Get, REPLY_AUDIO, mp3(b"again"));
        assert!(phone.ui.answer_playback(Ok(())));
        phone.draw();

        // The closing line is heard out before the report shows.
        let _ = phone.say(2);
        phone.reply(
            HttpMethod::Post,
            &Phone::turns_path(),
            json(turn(
                "It is done.",
                "Thank you. Goodbye.",
                4,
                true,
                Some(REPLY_AUDIO),
            )),
        );
        let paths: Vec<String> = phone
            .requests()
            .iter()
            .map(|(_, request)| request.url.clone())
            .collect();
        assert_eq!(
            paths,
            [
                format!("{BASE}{REPLY_AUDIO}"),
                format!("{BASE}{}", Phone::end_path())
            ]
        );
        phone.reply(HttpMethod::Get, REPLY_AUDIO, mp3(b"goodbye"));
        phone.reply(
            HttpMethod::Post,
            &Phone::end_path(),
            json(report(NEW_CALL, 4, true)),
        );
        assert_eq!(phone.screen(), Screen::LiveCall, "still saying goodbye");
        assert!(phone.ui.answer_playback(Ok(())));
        phone.draw();
        assert_eq!(phone.screen(), Screen::CallReport, "{name}");
        assert_eq!(phone.model().report.score(), Some(82));
        phone.refresh(&[(NEW_CALL, 4), (OLD_CALL, 3)]);
    }
}

/// The microphone refused, busy, interrupted, too short, a minute long,
/// and a recording with no words in it.
#[test]
fn the_microphones_failures_are_explained_and_survivable() {
    for (name, info) in phones() {
        // Refused: typing takes over for this call.
        let mut phone = Phone::online(info, true, &[]);
        phone.start_call(true);
        phone.hear_opening();
        phone.tap("talk");
        assert!(
            phone
                .ui
                .answer_recording_started(Err(AudioError::PermissionDenied))
        );
        phone.draw();
        let call = phone.call().clone();
        assert!(call.typing && call.mic_denied, "{name}");
        assert_eq!(
            call.notice.as_ref().map(|notice| notice.text.as_str()),
            Some("The microphone is not allowed for CallReady. Type your answers for this call.")
        );
        assert!(phone.shows("reply-input") && !phone.shows("talk"));
        assert!(!phone.shows("speak-instead"), "no way back to it");
        assert!(!phone.shows("retry"));
        // The typed turn goes as text, and the caller still has a voice.
        phone.send("I am sorry about that.");
        let request = phone.reply(
            HttpMethod::Post,
            &Phone::turns_path(),
            json(turn(
                "I am sorry about that.",
                "Hm.",
                2,
                false,
                Some(REPLY_AUDIO),
            )),
        );
        assert_eq!(
            request.header_value("Content-Type"),
            Some("application/json")
        );
        phone.reply(HttpMethod::Get, REPLY_AUDIO, mp3(b"hm"));
        assert_eq!(phone.pending().len(), 1, "played");

        // Busy: said, and the microphone can be tapped again.
        let mut phone = Phone::online(info, true, &[]);
        phone.start_call(true);
        phone.hear_opening();
        phone.tap("talk");
        assert!(phone.ui.answer_recording_started(Err(AudioError::Busy)));
        phone.draw();
        assert!(
            phone
                .call()
                .notice
                .as_ref()
                .is_some_and(|notice| notice.text.contains("in use"))
        );
        assert!(phone.call().mic_ready() && phone.shows("talk"));

        // Interrupted while stopping: said, nothing sent.
        phone.tap("talk");
        assert!(phone.ui.answer_recording_started(Ok(())));
        phone.draw();
        assert_eq!(phone.call().notice, None, "a new try clears the old news");
        phone.wait(1);
        phone.tap("talk");
        assert!(phone.ui.answer_recording(Err(AudioError::Interrupted)));
        phone.draw();
        assert!(
            phone
                .call()
                .notice
                .as_ref()
                .is_some_and(|notice| notice.text.contains("interrupted"))
        );
        phone.quiet();

        // Too short to hold a word: not sent.
        phone.tap("talk");
        assert!(phone.ui.answer_recording_started(Ok(())));
        phone.draw();
        phone.tap("talk");
        assert!(phone.ui.answer_recording(Ok(Recording {
            duration: Duration::from_millis(150),
            ..recording(0)
        })));
        phone.draw();
        assert!(
            phone
                .call()
                .notice
                .as_ref()
                .is_some_and(|notice| notice.text.contains("too short"))
        );
        phone.quiet();

        // A minute: the recording stops itself and is sent.
        phone.tap("talk");
        assert!(phone.ui.answer_recording_started(Ok(())));
        phone.draw();
        phone.wait(59);
        assert_eq!(phone.call().phase, Phase::Recording(59));
        phone.quiet();
        phone.wait(1);
        assert_eq!(phone.pending(), [ServiceRequest::StopRecording]);
        assert_eq!(phone.call().phase, Phase::Stopping);
        assert!(phone.ui.answer_recording(Ok(recording(60))));
        phone.draw();
        phone.expect(HttpMethod::Post, &Phone::turns_path());

        // No speech in it (400): a gentle word, and the microphone again.
        phone.reply(
            HttpMethod::Post,
            &Phone::turns_path(),
            error(400, "bad_request", "No speech was heard in the recording."),
        );
        let call = phone.call().clone();
        assert_eq!(
            call.notice.as_ref().map(|notice| notice.text.as_str()),
            Some("I could not hear any words in that. Tap the microphone and try again.")
        );
        assert!(!phone.shows("retry"), "there is nothing to send again");
        assert!(call.mic_ready() && call.turns == 0);

        // "Aa": type instead, by choice, and the small microphone back.
        phone.tap("type-instead");
        assert!(phone.call().typing && phone.shows("reply-input"));
        assert!(phone.shows("speak-instead"));
        phone.tap("speak-instead");
        assert!(phone.call().speaks() && phone.shows("talk"));
    }
}

// --------------------------------------------------------------- failures

/// Offline, a timeout, the model failing, too many requests: the words
/// stay in the field, one line says what happened, Retry sends again.
#[test]
fn a_failed_typed_turn_loses_nothing_and_can_be_retried() {
    let limited: Answer = Ok(HttpResponse::new(429)
        .with_header("Retry-After", "20")
        .with_body(
            br#"{"error": {"code": "rate_limited", "message": "Too many requests."}}"#.to_vec(),
        ));
    let failures: [(Answer, &str); 5] = [
        (Err(HttpError::Offline), "The server could not be reached."),
        (
            Err(HttpError::Timeout),
            "The server took too long to answer.",
        ),
        (
            error(
                502,
                "upstream_failed",
                "The caller could not answer just now.",
            ),
            "The AI caller did not answer. Try again.",
        ),
        (limited, "Too many requests. Wait 20 seconds and try again."),
        (
            json(r#"{"heard": "x"}"#),
            "The server sent something this app does not understand.",
        ),
    ];
    for (name, info) in phones() {
        let mut phone = Phone::online(info, false, &[]);
        phone.start_call(false);
        for (turns, (answer, sentence)) in failures.clone().into_iter().enumerate() {
            phone.send("I am sorry about that.");
            phone.reply(HttpMethod::Post, &Phone::turns_path(), answer);
            let call = phone.call().clone();
            assert_eq!(call.phase, Phase::Idle, "{name}: {sentence}");
            assert_eq!(call.draft, "I am sorry about that.", "nothing is lost");
            assert_eq!(
                call.notice.as_ref().map(|notice| notice.text.as_str()),
                Some(sentence)
            );
            assert_eq!(call.turns as usize, turns, "a failed turn is no turn");
            assert_eq!(phone.screen(), Screen::LiveCall);
            phone.quiet();

            // Retry sends the same words again.
            phone.tap("retry");
            let request = phone.reply(
                HttpMethod::Post,
                &Phone::turns_path(),
                json(turn("I am sorry about that.", "Hm.", 2, false, None)),
            );
            assert_eq!(
                Phone::body(&request),
                r#"{"text":"I am sorry about that."}"#
            );
            let call = phone.call().clone();
            assert_eq!((call.draft.as_str(), call.notice), ("", None));
            assert_eq!(call.turns as usize, turns + 1);
            assert!(!phone.shows("retry"));
        }
    }
}

#[test]
fn a_failed_spoken_turn_keeps_the_recording() {
    let mut phone = Phone::online(galaxy_a27(), true, &[]);
    phone.start_call(true);
    phone.hear_opening();
    let first = phone.say(3);
    phone.reply(
        HttpMethod::Post,
        &Phone::turns_path(),
        error(
            502,
            "upstream_failed",
            "The voice service could not transcribe.",
        ),
    );
    assert_eq!(phone.call().kept(), Some(&recording(3)));
    assert!(phone.shows("retry") && phone.shows("talk"));
    phone.tap("retry");
    let second = phone.expect(HttpMethod::Post, &Phone::turns_path());
    assert_eq!(second.body, first.body, "the same recording");
    assert_eq!(second.header_value("Content-Type"), Some("audio/mp4"));
    phone.reply(
        HttpMethod::Post,
        &Phone::turns_path(),
        json(turn("Sorry.", "Hm.", 2, false, None)),
    );
    assert_eq!(phone.call().kept(), None);
    assert_eq!(phone.call().turns, 1);

    // The server lost its voice mid-call (503): typing takes over.
    let _ = phone.say(2);
    phone.reply(
        HttpMethod::Post,
        &Phone::turns_path(),
        error(503, "voice_unavailable", "No voice key is configured."),
    );
    assert!(phone.call().typing && phone.shows("reply-input"));
    assert!(!phone.shows("retry"));
}

/// 409: the call ended on the server. The app asks for the report.
#[test]
fn a_call_that_has_ended_on_the_server_goes_to_its_report() {
    for (name, info) in phones() {
        let mut phone = Phone::online(info, false, &[]);
        phone.start_call(false);
        phone.send("Hello?");
        phone.reply(
            HttpMethod::Post,
            &Phone::turns_path(),
            error(409, "call_ended", "This call has ended."),
        );
        assert!(phone.call().is_over(), "{name}");
        // The report fails once (scoring is a model request), and can be
        // asked for again: by Retry, or by the red button.
        phone.reply(
            HttpMethod::Post,
            &Phone::end_path(),
            error(
                502,
                "upstream_failed",
                "The call could not be scored just now.",
            ),
        );
        assert_eq!(phone.call().phase, Phase::EndFailed);
        assert_eq!(
            phone.call().notice.as_ref().map(|n| n.text.as_str()),
            Some("Your report is not ready. The AI caller did not answer. Try again.")
        );
        assert!(!phone.shows("reply-input"), "the call is over all the same");
        phone.tap("retry");
        phone.reply(
            HttpMethod::Post,
            &Phone::end_path(),
            Err(HttpError::Timeout),
        );
        phone.tap("end-call");
        phone.reply(
            HttpMethod::Post,
            &Phone::end_path(),
            json(report(NEW_CALL, 4, true)),
        );
        assert_eq!(phone.screen(), Screen::CallReport);
        assert_eq!(phone.model().report.score(), Some(82));
        phone.refresh(&[(NEW_CALL, 4)]);
    }
}

/// A call the server no longer knows (404) is left, with a word.
#[test]
fn a_call_the_server_has_lost_is_left() {
    let mut phone = Phone::online(small(), false, &[]);
    phone.start_call(false);
    phone.send("Hello?");
    phone.reply(
        HttpMethod::Post,
        &Phone::turns_path(),
        error(404, "not_found", "No such call."),
    );
    assert_eq!(phone.screen(), Screen::Home);
    assert_eq!(phone.model().live, None);
    assert_eq!(
        phone.model().notice.as_ref().map(|n| n.text.as_str()),
        Some("The server no longer knows that call. Nothing was scored.")
    );
    // The notice goes by itself.
    assert!(phone.app().tick().is_some());
    phone.wait(7);
    assert_eq!(phone.model().notice, None);
    assert!(phone.app().tick().is_none(), "and the app is idle again");
    phone.quiet();
}

/// If the server cannot be reached when a call is started, the call is the
/// practice script, and says so.
#[test]
fn a_call_that_cannot_start_online_is_the_practice_script() {
    for (name, info) in phones() {
        let mut phone = Phone::online(info, false, &[(OLD_CALL, 3)]);
        phone.tap("home-start-call");
        assert_eq!(
            phone.model().live,
            Some(Live::Dialing(Scenario::MissingDelivery))
        );
        phone.reply(HttpMethod::Post, "/v1/calls", Err(HttpError::Offline));
        assert_eq!(phone.screen(), Screen::LiveCall, "{name}");
        assert_eq!(phone.model().live, None);
        let call = phone.app().call().clone();
        assert_eq!(call.scenario, Scenario::MissingDelivery);
        assert!(call.is_running());
        assert_eq!(
            phone.model().notice.as_ref().map(|n| n.text.as_str()),
            Some("The server could not be reached. This call is the practice script.")
        );
        assert_eq!(
            phone.model().link,
            Link::Offline("The server could not be reached.".to_owned())
        );
        phone.quiet();
        // It plays as the script does.
        phone.tap(("tip", 0usize));
        assert_eq!(phone.app().call().choices, [0]);
        phone.wait(8);
        assert_eq!(phone.model().notice, None);
        phone.tap("end-call");
        assert_eq!(phone.screen(), Screen::CallReport);
        assert!(phone.model().report.server.is_none());
        phone.quiet();
        // Back on home, the server is asked again.
        phone.tap("back");
        phone.connect(false, &[(OLD_CALL, 3)]);
        assert!(phone.model().link.is_online());

        // The server answers but cannot start a call (the model is down):
        // the script again, and the app stays online.
        phone.tap(("home-scenario", 1usize));
        phone.reply(
            HttpMethod::Post,
            "/v1/calls",
            error(500, "internal_error", "Something went wrong."),
        );
        assert_eq!(phone.model().live, None);
        assert_eq!(phone.app().call().scenario, Scenario::RefundRequest);
        assert_eq!(
            phone.model().notice.as_ref().map(|n| n.text.as_str()),
            Some("The server had a problem (500). This call is the practice script.")
        );
        assert!(phone.model().link.is_online());
        assert!(phone.back());
        phone.quiet();
    }
}

/// Answers that arrive after the learner has moved on change nothing.
#[test]
fn late_answers_are_ignored() {
    let mut phone = Phone::online(galaxy_a27(), true, &[]);

    // Hung up while it rang: when the server's call arrives after all, it
    // is closed (unscored), not shown.
    phone.tap(("home-scenario", 0usize));
    let ringing = phone.requests()[0].0;
    phone.tap("end-call");
    assert_eq!(phone.screen(), Screen::Home);
    assert_eq!(phone.model().live, None);
    assert!(
        phone
            .ui
            .resolve(ringing, ServiceResult::Http(json(call_start(true))))
    );
    phone.draw();
    assert_eq!(phone.screen(), Screen::Home);
    assert_eq!(phone.model().live, None);
    let closing = phone.expect(HttpMethod::Post, &Phone::end_path());
    assert!(closing.body.is_empty());
    phone.reply(
        HttpMethod::Post,
        &Phone::end_path(),
        json(report(NEW_CALL, 4, false)),
    );
    assert_eq!(phone.screen(), Screen::Home, "nobody waits for that report");
    phone.quiet();

    // A call is left mid-turn, mid-recording and mid-sentence.
    phone.start_call(true);
    let audio = phone.requests()[0].0;
    phone.tap("talk");
    assert!(phone.ui.answer_recording_started(Ok(())));
    phone.draw();
    assert!(phone.back(), "Back abandons the call");
    assert_eq!(phone.screen(), Screen::Home);
    assert_eq!(phone.model().live, None);
    // The microphone is stopped; what it recorded is nobody's.
    assert_eq!(phone.pending().last(), Some(&ServiceRequest::StopRecording));
    assert!(phone.ui.answer_recording(Ok(recording(3))));
    phone.draw();
    assert_eq!(
        phone.requests().len(),
        1,
        "only the audio that was on its way"
    );
    let before = phone.model().clone();
    assert!(phone.ui.resolve(audio, ServiceResult::Http(mp3(b"late"))));
    phone.draw();
    assert_eq!(*phone.model(), before, "late audio is not played");
    phone.quiet();

    // A second call: answers of the first, were they to come now, are not
    // this call's. (The same paths, the same call id even: only the number
    // the app gave each call tells them apart.)
    phone.start_call(true);
    phone.hear_opening();
    phone.tap("type-instead");
    phone.send("First try.");
    let first = phone.requests()[0].0;
    phone.tap("back");
    assert_eq!(phone.screen(), Screen::Home);
    phone.start_call(true);
    phone.hear_opening();
    let untouched = phone.call().clone();
    assert!(phone.ui.resolve(
        first,
        ServiceResult::Http(json(turn("First try.", "LATE", 2, true, None)))
    ));
    phone.draw();
    assert_eq!(*phone.call(), untouched);
    assert_eq!(phone.screen(), Screen::LiveCall);
    phone.quiet();

    // Within a call: the answer to a turn that was retried is not the one
    // waited for, and neither is a report asked for before a retry.
    phone.tap("type-instead");
    phone.send("Second try.");
    let slow = phone.requests()[0].0;
    phone.tap("end-call");
    let end = phone.requests()[1].0;
    assert!(phone.ui.resolve(
        slow,
        ServiceResult::Http(json(turn("Second try.", "LATE", 2, false, None)))
    ));
    phone.draw();
    assert_ne!(phone.call().line, "LATE");
    assert!(phone.call().is_over());
    assert!(
        phone
            .ui
            .resolve(end, ServiceResult::Http(Err(HttpError::Timeout)))
    );
    phone.draw();
    phone.tap("retry");
    phone.reply(
        HttpMethod::Post,
        &Phone::end_path(),
        json(report(NEW_CALL, 4, true)),
    );
    assert_eq!(phone.screen(), Screen::CallReport);
    phone.refresh(&[(NEW_CALL, 4)]);

    // An older check of the server answers after a newer one began.
    phone.tap("back");
    phone.tab(Tab::More);
    phone.tap("more-server");
    phone.tap("server-check");
    let older = phone.requests()[0].0;
    phone.tap("server-check");
    assert_eq!(phone.requests().len(), 2);
    assert!(
        phone
            .ui
            .resolve(older, ServiceResult::Http(Err(HttpError::Offline)))
    );
    phone.draw();
    assert_eq!(
        phone.model().link,
        Link::Checking,
        "the older answer is old"
    );
    phone.connect(false, &[(NEW_CALL, 4)]);
    assert_eq!(phone.model().link, Link::Online { voice: false });
}

// ------------------------------------------------------- profile, server

/// Edits show at once and are sent; a failure is tried once more quietly,
/// then put back with a line.
#[test]
fn edits_are_saved_to_the_server_and_put_back_when_they_fail() {
    for (name, info) in phones() {
        let mut phone = Phone::online(info, false, &[]);

        // A setting: shown at once, sent as the one field.
        phone.tap("home-scenario-settings");
        phone.tap(("accent", Accent::Uk));
        assert_eq!(phone.model().settings.accent, Accent::Uk, "{name}");
        let request = phone.reply(
            HttpMethod::Put,
            "/v1/profile",
            json(profile("Asha Mrema", "uk", "free", 0)),
        );
        assert_eq!(
            request.header_value("Content-Type"),
            Some("application/json")
        );
        assert_eq!(Phone::body(&request), r#"{"accent":"uk"}"#);
        assert_eq!(phone.model().settings.accent, Accent::Uk);
        // The chip that is already chosen asks nothing.
        phone.tap(("accent", Accent::Uk));
        phone.quiet();

        phone.tap(("difficulty", Difficulty::Tough));
        let request = phone.expect(HttpMethod::Put, "/v1/profile");
        assert_eq!(Phone::body(&request), r#"{"difficulty":"tough"}"#);
        // It fails; once more, quietly; fails again: put back, and said.
        phone.reply(HttpMethod::Put, "/v1/profile", Err(HttpError::Timeout));
        assert_eq!(phone.model().settings.difficulty, Difficulty::Tough);
        assert_eq!(phone.model().notice, None, "quiet the first time");
        let again = phone.expect(HttpMethod::Put, "/v1/profile");
        assert_eq!(Phone::body(&again), r#"{"difficulty":"tough"}"#);
        phone.reply(
            HttpMethod::Put,
            "/v1/profile",
            error(500, "internal_error", "Something went wrong."),
        );
        assert_eq!(phone.model().settings.difficulty, Difficulty::Standard);
        assert_eq!(
            phone.model().settings.accent,
            Accent::Uk,
            "what was saved stays"
        );
        assert_eq!(
            phone.model().notice.as_ref().map(|n| n.text.as_str()),
            Some("Not saved. The server had a problem (500).")
        );
        phone.quiet();
        // A refusal (400) is not tried twice.
        phone.tap(("length", Length::Six));
        let request = phone.reply(
            HttpMethod::Put,
            "/v1/profile",
            error(
                400,
                "bad_request",
                "\\\"minutes\\\" must be one of: 2, 4, 6.",
            ),
        );
        assert_eq!(Phone::body(&request), r#"{"minutes":6}"#);
        assert_eq!(phone.model().settings.length, Length::Four);
        phone.quiet();
        // The second attempt of a save succeeding is a save.
        phone.tap(("length", Length::Two));
        phone.reply(HttpMethod::Put, "/v1/profile", Err(HttpError::Offline));
        let saved =
            profile("Asha Mrema", "uk", "free", 0).replace("\"minutes\": 4", "\"minutes\": 2");
        phone.reply(HttpMethod::Put, "/v1/profile", json(saved.clone()));
        assert_eq!(phone.model().settings.length, Length::Two);
        assert!(phone.model().link.is_online());
        phone.wait(7);
        assert!(phone.back());

        // The name: typed freely, sent when the learner is done with it.
        phone.tap("home-profile");
        phone.tap("profile-name");
        phone.clear_field();
        phone.type_text("Neema");
        assert_eq!(phone.model().name, "Neema");
        phone.quiet();
        phone.key(TextInputEvent::Action(ImeAction::Done));
        let request = phone.reply(
            HttpMethod::Put,
            "/v1/profile",
            json(saved.replace("Asha Mrema", "Neema")),
        );
        assert_eq!(Phone::body(&request), r#"{"name":"Neema"}"#);
        assert_eq!(phone.model().name, "Neema");
        // Left empty, the name the server holds returns; nothing is sent.
        phone.tap("profile-name");
        phone.clear_field();
        assert_eq!(phone.model().name, "");
        phone.tap("back");
        assert_eq!(phone.screen(), Screen::Home);
        assert_eq!(phone.model().name, "Neema");
        phone.quiet();
        // Typed and left by Back: sent then.
        phone.tap("home-profile");
        phone.tap("profile-name");
        phone.clear_field();
        phone.type_text("Neema J");
        assert!(phone.back(), "the keyboard's Back is the framework's");
        if phone.screen() == Screen::Profile {
            assert!(phone.back());
        }
        let request = phone.reply(
            HttpMethod::Put,
            "/v1/profile",
            json(saved.replace("Asha Mrema", "Neema J")),
        );
        assert_eq!(Phone::body(&request), r#"{"name":"Neema J"}"#);

        // Premium: a demo flag, on the server too.
        phone.tab(Tab::More);
        phone.tap("more-premium");
        phone.tap("start");
        assert!(phone.model().premium);
        let request = phone.reply(
            HttpMethod::Put,
            "/v1/profile",
            json(
                saved
                    .replace("Asha Mrema", "Neema J")
                    .replace("\"free\"", "\"premium\""),
            ),
        );
        assert_eq!(Phone::body(&request), r#"{"plan":"premium"}"#);
        assert!(phone.model().premium);
        assert_eq!(phone.screen(), Screen::More);
        phone.quiet();
    }
}

/// The Server screen: the address in a real field, remembered; the check
/// and its result in plain words.
#[test]
fn the_server_address_is_typed_checked_and_remembered() {
    for (name, info) in phones() {
        let mut phone = Phone::launch(info);
        phone.reply(HttpMethod::Get, "/v1/health", Err(HttpError::Offline));
        phone.tap("welcome-start");
        phone.reply(HttpMethod::Get, "/v1/health", Err(HttpError::Offline));
        phone.tab(Tab::More);
        assert_eq!(
            phone.model().link.status(),
            "unreachable \u{b7} The server could not be reached."
        );
        phone.tap("more-server");
        assert_eq!(phone.screen(), Screen::Server, "{name}");
        assert_eq!(phone.model().server_draft, BASE);

        // An address that cannot be one: said, nothing asked or stored.
        phone.tap("server-url");
        assert!(phone.ui.is_focused("server-url"));
        phone.clear_field();
        phone.type_text("mac.local:9000");
        phone.tap("server-check");
        assert_eq!(
            phone.model().server_problem,
            Some("The address must start with http:// or https://.")
        );
        assert_eq!(phone.model().server_url, BASE);
        assert_eq!(phone.ui.stored(SERVER_KEY), None);
        phone.quiet();

        // A real one, checked with the keyboard's Go: used and stored.
        phone.tap("server-url");
        phone.clear_field();
        phone.type_text("https://calls.example.com/");
        assert_eq!(phone.model().server_problem, None);
        phone.key(TextInputEvent::Action(ImeAction::Go));
        assert_eq!(phone.model().server_url, "https://calls.example.com");
        assert_eq!(phone.model().server_draft, "https://calls.example.com");
        assert_eq!(
            phone.ui.stored(SERVER_KEY),
            Some("https://calls.example.com")
        );
        assert_eq!(phone.model().link, Link::Checking);
        let requests = phone.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].1.url, "https://calls.example.com/v1/health");
        assert_eq!(requests[0].1.header_value("X-Device-Id"), Some(INSTALL));
        // Connected, with a voice.
        for body in [
            health(true),
            profile("Asha Mrema", "us", "free", 0),
            calls(&[]),
        ] {
            assert!(phone.ui.answer_http(json(body)));
            phone.draw();
        }
        assert_eq!(phone.model().link.status(), "connected \u{b7} voice");
        assert_eq!(phone.model().name, "Asha Mrema");
        phone.quiet();

        // Checked again, it is unreachable this time, and says why.
        phone.tap("server-check");
        assert!(
            phone
                .ui
                .answer_http(Err(HttpError::Failed("certificate not trusted".into())))
        );
        phone.draw();
        assert_eq!(
            phone.model().link.status(),
            "unreachable \u{b7} The request could not be made: certificate not trusted."
        );

        // "Use the default": the development address, checked at once.
        phone.tap("server-default");
        assert_eq!(phone.model().server_url, BASE);
        assert_eq!(phone.ui.stored(SERVER_KEY), Some(BASE));
        phone.connect(false, &[]);
        assert_eq!(phone.model().link.status(), "connected \u{b7} text mode");
        phone.tap("back");
        assert_eq!(phone.screen(), Screen::More);
        phone.quiet();
    }

    // On the next launch the stored address is the one asked.
    let host = HostState::new()
        .with_install_id(INSTALL)
        .with_stored(SERVER_KEY, "https://calls.example.com");
    let phone = Phone::launch_with(galaxy_a27(), &host, CallReady::default());
    assert_eq!(phone.model().server_url, "https://calls.example.com");
    assert_eq!(
        phone.requests()[0].1.url,
        "https://calls.example.com/v1/health"
    );
    // A stored value that is no address is not used.
    let host = HostState::new()
        .with_install_id(INSTALL)
        .with_stored(SERVER_KEY, "javascript:alert(1)");
    let phone = Phone::launch_with(galaxy_a27(), &host, CallReady::default());
    assert_eq!(phone.requests()[0].1.url, format!("{BASE}/v1/health"));
}

/// A report opened from the history whose details cannot be fetched says
/// so, and can be tried again.
#[test]
fn a_report_that_cannot_be_loaded_can_be_tried_again() {
    for (name, info) in phones() {
        let mut phone = Phone::online(info, false, &[(OLD_CALL, 3), (OLDER_CALL, 2)]);
        phone.tab(Tab::History);
        phone.tap(("history-call", 1usize));
        assert_eq!(phone.screen(), Screen::CallReport, "{name}");
        assert_eq!(phone.model().report.score(), Some(72));
        let path = format!("/v1/calls/{OLDER_CALL}");
        phone.reply(HttpMethod::Get, &path, Err(HttpError::Timeout));
        let detail = phone
            .model()
            .report
            .server
            .as_ref()
            .map(|server| server.detail.clone());
        assert_eq!(
            detail,
            Some(Detail::Failed(
                "The server took too long to answer.".to_owned()
            ))
        );
        // The transcript says the same instead of showing nothing.
        phone.tap("transcript");
        phone.tap("back");
        phone.tap("report-reload");
        // Another call's report is not this call's.
        phone.reply(HttpMethod::Get, &path, json(report(OLD_CALL, 3, true)));
        assert!(!phone.model().report.has_skills());
        phone.tap("report-reload");
        phone.reply(HttpMethod::Get, &path, json(report(OLDER_CALL, 2, true)));
        assert!(phone.model().report.has_skills());
        assert!(!phone.shows("report-reload"));
        phone.quiet();
    }
}

// ----------------------------------------------------------------- layout

/// With the keyboard up the field and its send button ride above it, and
/// long lines, long tips and a long reply fit the screen.
#[test]
fn the_call_fits_with_the_keyboard_up_and_with_long_text() {
    for (name, info) in phones() {
        let mut phone = Phone::online(info, false, &[]);
        phone.tap(("home-scenario", 0usize));
        let long_title = "A".repeat(60);
        let start = call_start(false)
            .replace("Angry customer", &long_title)
            .replace("Dana", "Bartholomew-Maximilian-Alexander")
            .replace(
                "This is the third time I’ve called today!",
                &"I have been waiting for a very long time. ".repeat(12),
            )
            .replace("Take ownership", &"t".repeat(80));
        phone.reply(HttpMethod::Post, "/v1/calls", json(start));
        let call = phone.call().clone();
        assert_eq!(call.title.chars().count(), 40, "{name}");
        assert_eq!(call.caller.chars().count(), 24);
        assert!(call.line.chars().count() <= 320);
        assert_eq!(call.tips[0].chars().count(), 28);

        // The keyboard comes up: 300 dp of it.
        phone.tap("reply-input");
        phone.info = info.with_keyboard_inset(300.0 * info.scale);
        phone.draw();
        let keyboard_top = info.size.height as f32 - 300.0 * info.scale;
        for id in ["reply-input", "reply-send"] {
            let rect = phone.rect(id);
            assert!(
                rect.y + rect.height <= keyboard_top + 0.5,
                "{name}: {id} at {rect:?} is under the keyboard ({keyboard_top})"
            );
            assert!(rect.y >= info.insets.top);
        }
        // A reply of several lines: the field grows, and still fits.
        phone.type_text(
            "I am so sorry that you have had to call three times today about this. \
             Let me stay on the line with you until it is fixed.",
        );
        let rect = phone.rect("reply-send");
        assert!(rect.y + rect.height <= keyboard_top + 0.5, "{name}");
        // It has grown to its three lines and no further: the rest of the
        // reply is scrolled inside it, the caret (at the end) in view.
        let three_lines = 3.0 * 22.95 * info.scale;
        let field = phone.rect("reply-input");
        assert!(
            (field.height - three_lines).abs() <= 1.0,
            "{name}: the field is {} px tall, three lines are {three_lines}",
            field.height
        );
        let inside = phone.ui.scroll_offset("reply-input");
        assert!(
            inside.is_some_and(|offset| offset > 0.0),
            "{name}: {inside:?}"
        );
        phone.tap("reply-send");
        phone.info = info;
        phone.draw();
        // The page has been read down by now (reaching for the send button
        // scrolled it). The scroll view is the same one for the whole
        // call, so it stays there until the application asks.
        let (offset, _) = phone.scroll();
        assert!(offset > 100.0, "{name}: at {offset}");
        phone.reply(
            HttpMethod::Post,
            &Phone::turns_path(),
            json(turn(
                &"I am sorry. ".repeat(40),
                &"word ".repeat(70),
                2,
                false,
                None,
            )),
        );
        assert_eq!(phone.scroll().0, 0.0, "the new line starts at the top");
        // The caller's line begins inside the view, whatever came before.
        let view = phone.viewport();
        assert!(view.height > 0.0);
    }
}

// --------------------------------------------------------- a real server

/// Carries one request to the real server with `curl`.
fn carry(request: &HttpRequest) -> Answer {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut command = Command::new("curl");
    command
        .arg("--silent")
        .arg("--show-error")
        .args(["--max-time", &request.timeout.as_secs().max(1).to_string()])
        .args(["--request", request.method.as_str()])
        .args(["--write-out", "\n%{http_code} %{content_type}"]);
    for (name, value) in &request.headers {
        command.args(["--header", &format!("{name}: {value}")]);
    }
    if !request.body.is_empty() {
        command.args(["--data-binary", "@-"]);
    }
    command
        .arg(&request.url)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("curl is installed");
    child
        .stdin
        .take()
        .expect("a pipe")
        .write_all(&request.body)
        .expect("the body is written");
    let output = child.wait_with_output().expect("curl ran");
    if !output.status.success() {
        return Err(match output.status.code() {
            Some(7) => HttpError::Offline,
            Some(28) => HttpError::Timeout,
            _ => HttpError::Failed(String::from_utf8_lossy(&output.stderr).trim().to_owned()),
        });
    }
    let split = output
        .stdout
        .iter()
        .rposition(|byte| *byte == b'\n')
        .expect("the status line");
    let trailer = String::from_utf8_lossy(&output.stdout[split + 1..]).into_owned();
    let (status, content_type) = trailer.split_once(' ').unwrap_or((trailer.as_str(), ""));
    Ok(HttpResponse::new(status.parse().expect("a status"))
        .with_header("Content-Type", content_type.trim())
        .with_body(output.stdout[..split].to_vec()))
}

/// Carries every waiting request to the real server until the application
/// waits for nothing, printing each exchange's first line.
fn pump(phone: &mut Phone) {
    for _ in 0..40 {
        let Some((id, request)) = phone.requests().into_iter().next() else {
            return;
        };
        assert!(
            request.url.starts_with("http://127.0.0.1:8787/v1/"),
            "{}",
            request.url
        );
        assert!(
            !request.url.contains("/audio/"),
            "a typed call asks for no audio"
        );
        let answer = carry(&request);
        println!(
            "{} {} -> {}",
            request.method.as_str(),
            request.url.trim_start_matches("http://127.0.0.1:8787"),
            match &answer {
                Ok(response) => response.status.to_string(),
                Err(error) => error.to_string(),
            }
        );
        assert!(phone.ui.resolve(id, ServiceResult::Http(answer)));
        phone.draw();
    }
    panic!("the application never stopped asking");
}

/// A typed call against the real server, end to end. Run it with the
/// server started on 127.0.0.1:8787:
///
/// ```sh
/// cargo test -p callready --test online -- --ignored --nocapture a_real_typed_call
/// ```
///
/// It uses the server's language model (three turns and a scoring) and no
/// voice: if the server has a voice key the test stops before starting a
/// call, since even a typed call would then be spoken by the voice
/// service. The device id begins with `test_`.
#[test]
#[ignore = "needs the CallReady server on 127.0.0.1:8787"]
fn a_real_typed_call() {
    // A new installation every run: 32 hex digits from the clock.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock")
        .as_nanos();
    let install = format!("{:032x}", nanos ^ (u128::from(std::process::id()) << 64));
    let host = HostState::new().with_install_id(&install);
    let mut phone = Phone::launch_with(
        galaxy_a27(),
        &host,
        CallReady::default().with_device_prefix("test_"),
    );
    let device = phone.app().device_id().to_owned();
    assert_eq!(device, format!("test_{install}"));
    println!("device {device}");
    for (_, request) in phone.requests() {
        assert_eq!(request.header_value("X-Device-Id"), Some(device.as_str()));
    }

    pump(&mut phone);
    println!("link: {}", phone.model().link.status());
    assert_eq!(
        phone.model().link,
        Link::Online { voice: false },
        "the server must be up, and without a voice key for this test"
    );
    assert_eq!(phone.model().calls(), 0, "a device the server has not seen");
    phone.tap("welcome-start");

    phone.tap(("home-scenario", 0usize));
    pump(&mut phone);
    let call = phone.call().clone();
    println!(
        "call {} \u{b7} {} \u{b7} {} \u{b7} goal: {}",
        call.number, call.title, call.caller, call.goal
    );
    println!("  {}: {}", call.caller, call.line);
    println!("  tips: {:?}", call.tips);
    assert!(!call.voice);
    assert_eq!(call.scenario, Scenario::AngryCustomer);
    // API.md: the opening is the offline script's.
    assert_eq!(call.line, Scenario::AngryCustomer.script()[0].customer());

    let replies = [
        "I am so sorry you have had to call three times today. I will stay with you until this is fixed. What happens just before the app logs you out?",
        "Thank you, that helps. I am resetting your session now so that it stays signed in. Could you open the app and tell me what you see?",
        "I am glad it is working. I have added a note to your account and I will email you tomorrow to check it is still fine. Is there anything else I can help with?",
    ];
    for reply in replies {
        if phone.model().online_call().is_none_or(OnlineCall::is_over) {
            break;
        }
        phone.wait(2);
        phone.send(reply);
        pump(&mut phone);
        let Some(call) = phone.model().online_call() else {
            break;
        };
        assert_eq!(call.notice, None, "the turn failed");
        println!("  You: {}", call.said.clone().unwrap_or_default());
        println!("  {}: {}", call.caller, call.line);
        println!("  tips: {:?} \u{b7} done: {}", call.tips, call.done);
    }
    if phone.screen() == Screen::LiveCall {
        phone.tap("end-call");
        pump(&mut phone);
    }
    assert_eq!(phone.screen(), Screen::CallReport);
    let report = phone.model().report.clone();
    let server = report.server.clone().expect("the server's report");
    println!(
        "report: score {:?} \u{b7} skills {:?} \u{b7} {} s",
        report.score(),
        report.scores.0,
        report.seconds
    );
    println!("  did well: {}", report.did_well());
    println!("  try next: {}", report.try_next());
    for line in &server.transcript {
        println!(
            "  [{}] {}{}",
            if line.caller { "caller" } else { "learner" },
            line.text,
            line.note
                .as_ref()
                .map(|note| format!("  ({note})"))
                .unwrap_or_default()
        );
    }
    assert!(report.score().is_some(), "a call with turns is scored");
    assert!(server.transcript.len() >= 3);
    // The refresh has run: the server counts the call.
    assert_eq!(phone.model().calls(), 1);
    assert_eq!(phone.model().history.len(), 1);
    assert_eq!(phone.model().history[0].call_id(), report.call_id());
    assert_eq!(
        phone.model().stats.and_then(|stats| stats.average),
        report.score()
    );
    phone.tap("transcript");
    phone.tap("back");
    phone.tap("back");
    phone.tab(Tab::History);
    phone.tap(("history-call", 0usize));
    assert_eq!(phone.model().report.score(), report.score());
    phone.quiet();
}
