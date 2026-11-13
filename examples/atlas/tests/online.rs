//! Atlas with a server behind it, driven the way a phone drives it.
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
//! One test at the bottom is ignored by default: it holds a typed chat of
//! two messages against a real server on 127.0.0.1:8788, carrying each
//! request there with `curl`.

use std::hash::Hash;
use std::time::Duration;

use atlas::{
    Atlas, Flow, HOLD_TICKS, Link, Listen, Loaded, Mode, Phase, Plan, RECORDING_TICKS, SERVER_KEY,
    STEP_TICKS, Screen, Tab, VOICE_LINE, Wire,
};
use morningstar::ui::Ui;
use morningstar::{
    App, AudioClip, AudioError, Frame, FrameInfo, HostState, HttpError, HttpMethod, HttpRequest,
    HttpResponse, Insets, Point, PointerEvent, PointerPhase, Recording, Rect, ServiceId,
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

const HOME: Screen = Screen::Home(Flow::Standard);
const CHAT: Screen = Screen::Chat(Flow::Standard);

// ------------------------------------------------------------ the server

const INSTALL: &str = "0123456789abcdef0123456789abcdef";
const BASE: &str = "http://127.0.0.1:8788";
const CHAT_A: &str = "h_0000000000000000000000000000000a";
const CHAT_B: &str = "h_0000000000000000000000000000000b";
const AUDIO: &str = "/v1/audio/a_000000000000000000000001.mp3";
const HEARD: &str = "What is the capital of Tanzania?";

fn json(body: impl Into<String>) -> Wire {
    Ok(HttpResponse::new(200)
        .with_header("Content-Type", "application/json; charset=utf-8")
        .with_body(body.into().into_bytes()))
}

fn error(status: u16, code: &str, message: &str) -> Wire {
    Ok(HttpResponse::new(status)
        .with_header("Content-Type", "application/json; charset=utf-8")
        .with_body(
            format!(r#"{{"error": {{"code": "{code}", "message": "{message}"}}}}"#).into_bytes(),
        ))
}

fn mp3(bytes: &[u8]) -> Wire {
    Ok(HttpResponse::new(200)
        .with_header("Content-Type", "audio/mpeg")
        .with_body(bytes.to_vec()))
}

fn health(voice: bool) -> String {
    format!(r#"{{"ok": true, "model": true, "voice": {voice}, "database": true}}"#)
}

fn profile(model: &str, plan: &str) -> String {
    format!(r#"{{"model": "{model}", "plan": "{plan}"}}"#)
}

fn summary(id: &str, title: &str, messages: u32) -> String {
    format!(
        r#"{{"chat_id": "{id}", "title": "{title}", "updated_at": "2026-11-13T09:14:03Z",
            "messages": {messages}, "state": "answered"}}"#
    )
}

fn chats(rows: &[String]) -> String {
    format!(r#"{{"chats": [{}]}}"#, rows.join(","))
}

/// A JSON string literal of `text`.
fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// The answer to a message, as API.md shows it.
fn reply(id: &str, title: &str, heard: &str, text: &str, used: &str, turn: u32) -> String {
    reply_with(id, title, heard, text, used, turn, None)
}

fn reply_with(
    id: &str,
    title: &str,
    heard: &str,
    text: &str,
    used: &str,
    turn: u32,
    audio: Option<&str>,
) -> String {
    format!(
        r#"{{"chat_id": "{id}", "title": "{title}", "heard": {},
            "steps": [{{"label": "Reading the request", "state": "done"}},
                      {{"label": "Comparing the two", "state": "done"}},
                      {{"label": "Writing the answer", "state": "done"}}],
            "answer": {{"text": {}, "audio_url": {}}},
            "model_used": "{used}", "turn": {turn}}}"#,
        quote(heard),
        quote(text),
        audio.map_or("null".to_owned(), quote),
    )
}

/// `GET /v1/chats/{id}` for a chat of two exchanges.
fn stored(id: &str, title: &str) -> String {
    format!(
        r#"{{"chat_id": "{id}", "title": "{title}", "updated_at": "2026-11-13T09:14:03Z",
            "messages": [
              {{"role": "user", "text": "Compare buying and leasing a car",
                "attachments": [{{"name": "quote-a.pdf", "kind": "pdf"}}]}},
              {{"role": "assistant", "text": "Leasing costs less each month.\n\n- Buying builds equity\n- Leasing has mileage limits",
                "steps": [{{"label": "Reading the request", "state": "done"}},
                          {{"label": "Writing the answer", "state": "done"}}],
                "model_used": "smart"}},
              {{"role": "user", "text": "And over ten years?", "attachments": []}},
              {{"role": "assistant", "text": "Over ten years buying is cheaper.",
                "steps": [{{"label": "Working it out", "state": "done"}}],
                "model_used": "fast"}}
            ]}}"#
    )
}

/// An answer that uses all of the light formatting, with a line of code
/// that has no space to break at.
const LONG_ANSWER: &str = "Over ten years buying usually wins.\n\nWhy:\n- Lease payments never stop\n- A bought car is worth something at the end\n- `total = monthly * 120` shows it quickly\n\n1. Add up ten years of lease payments\n2. Add up the loan, then subtract what the car is still worth\n3. Compare the two\n\n```python\ndef total_cost_of_ownership_over_the_whole_period(monthly_payment, months):\n    return monthly_payment * months\nprint(total_cost_of_ownership_over_the_whole_period(450, 120))\n```\n\nIf you drive little, leasing can still be the cheaper of the two, because the mileage limit never bites and the car is always under warranty.";

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
/// One tick of the app's timer, and a little.
const TICK: Duration = Duration::from_millis(34);

struct Phone {
    ui: Ui<Atlas>,
    frame: Frame,
    info: FrameInfo,
    elapsed: Duration,
    /// Where the app's server is expected to be.
    base: String,
    /// What the app must call itself.
    device: String,
}

impl Phone {
    /// The app as a phone launches it for the first time, with a host
    /// behind it: the test.
    fn launch(info: FrameInfo) -> Self {
        Self::launch_with(
            info,
            &HostState::new().with_install_id(INSTALL),
            Atlas::default(),
        )
    }

    fn launch_with(info: FrameInfo, host: &HostState, app: Atlas) -> Self {
        let mut ui = Ui::testing(app);
        let frame = Frame::for_app_with_host(&mut ui, host);
        let device = ui.app().device_id().to_owned();
        let mut phone = Self {
            ui,
            frame,
            info,
            elapsed: Duration::ZERO,
            base: BASE.to_owned(),
            device,
        };
        phone.draw();
        phone
    }

    /// The app online and on Home: the server has answered the three
    /// questions of start-up, with these chats (newest first).
    fn online(info: FrameInfo, voice: bool, rows: &[String]) -> Self {
        let mut phone = Self::launch(info);
        phone.connect(voice, "auto", "free", rows);
        phone.go_home();
        phone
    }

    /// Answers a check of the server: health, profile, chats.
    fn connect(&mut self, voice: bool, model: &str, plan: &str, rows: &[String]) {
        self.reply(HttpMethod::Get, "/v1/health", json(health(voice)));
        assert_eq!(*self.app().link(), Link::Checking, "not online yet");
        self.reply(HttpMethod::Get, "/v1/profile", json(profile(model, plan)));
        self.reply(HttpMethod::Get, "/v1/chats", json(chats(rows)));
        assert_eq!(*self.app().link(), Link::Online { voice });
        self.quiet();
    }

    /// Onboarding and sign-in, the short way.
    fn go_home(&mut self) {
        self.tap("get-started");
        self.tap("google");
        assert_eq!(self.screen(), HOME);
    }

    fn draw(&mut self) {
        self.frame.begin();
        let info = self.info.with_elapsed(self.elapsed);
        self.ui.frame(&info, &mut self.frame);
        // Nothing may overflow, in any frame, whatever the server sent.
        // (A tenth of a dp is let through: the design's model sheet is
        // that much short of its content for the frames in which it
        // slides away, here as in `tests/e2e.rs`, where only settled
        // frames are checked.)
        let overflows: Vec<_> = self
            .ui
            .overflows()
            .into_iter()
            .filter(|overflow| overflow.amount.width > 0.25 || overflow.amount.height > 0.25)
            .collect();
        assert_eq!(overflows, [], "layout overflow on {}", self.screen().name());
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

    // ------------------------------------------------------------ fingers

    fn rect(&self, id: impl Hash) -> Rect {
        self.ui
            .rect_of(id)
            .unwrap_or_else(|| panic!("{}: the widget is not on the screen", self.screen().name()))
    }

    fn shows(&self, id: impl Hash) -> bool {
        self.ui.rect_of(id).is_some()
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

    /// Drags a finger inside the screen's scroll view so that its content
    /// moves by `dp` towards its start. The finger rests before it lifts.
    fn drag(&mut self, view: Rect, dp: f32) {
        let direction = dp.signum();
        let room = view.height * 0.6 - (TOUCH_SLOP + 2.0) * self.scale();
        let travel = (dp.abs() * self.scale()).min(room.max(1.0));
        let from = view.y + view.height * if direction > 0.0 { 0.8 } else { 0.2 };
        let x = view.x + view.width / 2.0;
        self.pointer(PointerPhase::Down, Point::new(x, from));
        let anchor = from - direction * (TOUCH_SLOP + 2.0) * self.scale();
        self.pointer(PointerPhase::Moved, Point::new(x, anchor));
        for step in 1..=3 {
            let y = anchor - direction * travel * step as f32 / 3.0;
            self.pointer(PointerPhase::Moved, Point::new(x, y));
        }
        self.elapsed += Duration::from_millis(300);
        self.draw();
        self.pointer(PointerPhase::Up, Point::new(x, anchor - direction * travel));
    }

    /// Brings the widget wholly into view by dragging the screen's scroll
    /// view.
    fn reveal(&mut self, id: impl Hash + Copy) {
        let margin = 4.0 * self.scale();
        for _ in 0..80 {
            let rect = self.rect(id);
            let shown = self.ui.visible_rect_of(id).unwrap_or_default();
            if shown.width >= rect.width - 1.0 && shown.height >= rect.height - 1.0 {
                return;
            }
            let view = self.rect(self.screen().scroll_id());
            let over = rect.y + rect.height + margin - (view.y + view.height);
            let under = view.y + margin - rect.y;
            let wanted = if over > 0.0 { over } else { -under };
            self.drag(view, wanted / self.scale());
        }
        panic!("the widget could not be scrolled into view");
    }

    /// A tap: the widget is brought into view, then finger down and up on
    /// its middle, which must be on the visible screen.
    fn tap(&mut self, id: impl Hash + Copy) {
        self.reveal(id);
        let rect = self.rect(id);
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
        self.pointer(PointerPhase::Down, point);
        self.pointer(PointerPhase::Up, point);
        self.settle();
    }

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

    /// Empties the focused field, wherever in it the tap put the caret.
    fn clear_field(&mut self) {
        for _ in 0..64 {
            self.key(TextInputEvent::DeleteBackward);
            self.key(TextInputEvent::DeleteForward);
        }
    }

    /// Types a message into the composer and taps send.
    fn send(&mut self, text: &str) {
        self.tap("draft");
        assert!(self.ui.is_focused("draft"));
        self.type_text(text);
        assert_eq!(self.app().draft(), text);
        self.tap("send");
    }

    /// Lets the steps of the answer on screen run out.
    fn play_out(&mut self) {
        for _ in 0..40 {
            if !self.app().answering() {
                return;
            }
            self.ticks(STEP_TICKS);
        }
        panic!("the answer never finished");
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
        let url = format!("{}{path}", self.base);
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
        assert_eq!(
            found.1.header_value("X-Device-Id"),
            Some(self.device.as_str())
        );
        assert_eq!(found.1.header_value("Authorization"), None);
        found.clone()
    }

    fn expect(&self, method: HttpMethod, path: &str) -> HttpRequest {
        self.find(method, path).1
    }

    /// Answers that request. Returns the request.
    fn reply(&mut self, method: HttpMethod, path: &str, answer: Wire) -> HttpRequest {
        let (id, request) = self.find(method, path);
        assert!(self.ui.resolve(id, ServiceResult::Http(answer)));
        self.draw();
        request
    }

    fn body(request: &HttpRequest) -> &str {
        std::str::from_utf8(&request.body).expect("a text body")
    }

    /// The turns of the conversation on screen, as their phases.
    fn phases(&self) -> Vec<Phase> {
        self.app()
            .conversation()
            .turns
            .iter()
            .map(|turn| turn.phase)
            .collect()
    }

    /// Taps the composer's microphone on a server with a voice, lets the
    /// microphone start, speaks for a moment and taps the microphone
    /// button: the recording is with the server. Returns the request.
    fn say(&mut self, path: &str) -> HttpRequest {
        self.tap("mic");
        assert_eq!(self.screen(), Screen::LiveVoice);
        assert_eq!(self.app().voice().live, Listen::Starting);
        assert_eq!(self.pending(), [ServiceRequest::StartRecording]);
        assert!(self.ui.answer_recording_started(Ok(())));
        self.draw();
        assert_eq!(self.app().voice().live, Listen::Recording);
        self.ticks(20);
        self.tap("mute");
        assert_eq!(self.app().voice().live, Listen::Stopping);
        assert_eq!(self.pending(), [ServiceRequest::StopRecording]);
        assert!(self.ui.answer_recording(Ok(recording(2))));
        self.draw();
        assert_eq!(self.app().voice().live, Listen::Sending);
        self.expect(HttpMethod::Post, path)
    }
}

// --------------------------------------------------------------- start-up

#[test]
fn start_up_asks_health_then_profile_then_chats() {
    for (name, info) in phones() {
        let mut phone = Phone::launch(info);
        assert_eq!(phone.device, INSTALL);
        // One question at a time: nothing else is asked before health answers.
        assert_eq!(phone.requests().len(), 1, "{name}");
        let asked = phone.expect(HttpMethod::Get, "/v1/health");
        assert_eq!(asked.timeout, Duration::from_secs(6));
        assert_eq!(*phone.app().link(), Link::Checking);
        phone.connect(
            true,
            "smart",
            "plus",
            &[summary(CHAT_A, "Buying or leasing a car", 2)],
        );
        // The profile's model and plan are the app's.
        assert_eq!(phone.app().mode(), Mode::Smart);
        assert_eq!(phone.app().plan(), Some(Plan::Plus));
        assert!(phone.app().link().has_voice());
        // The design's own conversation is still there, but the Library
        // lists the server's.
        assert_eq!(phone.app().chats().len(), 2);
        assert_eq!(phone.app().library(), [1]);
        phone.go_home();
        // The mark says so, and leads to the Server screen.
        assert!(phone.shows("link"), "{name}");
        phone.tap("link");
        assert_eq!(phone.screen(), Screen::Server);
        assert!(phone.shows("server-status"));
        assert_eq!(phone.app().server_draft(), BASE);
        assert!(phone.back());
        assert_eq!(phone.screen(), HOME);
        phone.quiet();
    }
}

// -------------------------------------------------------- the text journey

/// Health, chats, a new chat, its steps and answer, a second message, the
/// Library, and (on a later launch) reopening and deleting it.
#[test]
fn the_text_journey() {
    for (name, info) in phones() {
        let mut phone = Phone::online(info, false, &[]);
        assert_eq!(
            phone.app().library(),
            [],
            "{name}: a new phone has no chats"
        );

        // ---- the first message starts a chat on the server
        phone.send("Compare buying and leasing a car");
        assert_eq!(phone.screen(), CHAT);
        assert_eq!(phone.app().draft(), "");
        let asked = phone.expect(HttpMethod::Post, "/v1/chats");
        assert_eq!(
            Phone::body(&asked),
            r#"{"text":"Compare buying and leasing a car","model":"auto"}"#
        );
        assert_eq!(asked.header_value("Content-Type"), Some("application/json"));
        assert_eq!(asked.header_value("X-Speak"), None);
        // While it waits: "Thinking", for as long as the server takes. No
        // scripted answer turns up.
        assert_eq!(phone.phases(), [Phase::Thinking]);
        assert!(phone.app().conversation().turns[0].waiting());
        phone.ticks(60);
        assert_eq!(phone.phases(), [Phase::Thinking]);
        assert!(phone.app().conversation().turns[0].steps.is_empty());
        // The provisional title is the message's start.
        assert_eq!(
            phone.app().conversation().title,
            "Compare buying and leasing a\u{2026}"
        );

        // ---- the answer arrives: the card runs through the server's steps
        phone.reply(
            HttpMethod::Post,
            "/v1/chats",
            json(reply(
                CHAT_A,
                "Buying or leasing a car",
                "Compare buying and leasing a car",
                "Leasing costs less each month; buying costs less in the end.",
                "smart",
                1,
            )),
        );
        let chat = phone.app().conversation().clone();
        assert_eq!(chat.title, "Buying or leasing a car", "the server's title");
        assert_eq!(chat.id(), Some(CHAT_A));
        assert_eq!(chat.turns[0].phase, Phase::Working(0));
        assert_eq!(
            chat.turns[0]
                .steps
                .iter()
                .map(|step| step.doing.as_str())
                .collect::<Vec<_>>(),
            [
                "Reading the request",
                "Comparing the two",
                "Writing the answer"
            ]
        );
        assert!(!phone.shows(("reply", 0usize)), "not before the steps end");
        phone.ticks(STEP_TICKS + 1);
        assert_eq!(phone.phases(), [Phase::Working(1)]);
        phone.play_out();
        assert_eq!(phone.phases(), [Phase::Done]);
        assert!(phone.shows(("reply", 0usize)));
        // Auto was the choice: the answer says which model took it.
        assert_eq!(
            phone.app().conversation().turns[0].model_used,
            Some(Mode::Smart)
        );
        phone.quiet();

        // ---- a second message goes to the same chat, and a long answer scrolls
        phone.send("And over ten years?");
        let path = format!("/v1/chats/{CHAT_A}/messages");
        let asked = phone.expect(HttpMethod::Post, &path);
        assert_eq!(
            Phone::body(&asked),
            r#"{"text":"And over ten years?","model":"auto"}"#
        );
        phone.reply(
            HttpMethod::Post,
            &path,
            json(reply(
                CHAT_A,
                "Buying or leasing a car",
                "And over ten years?",
                LONG_ANSWER,
                "fast",
                2,
            )),
        );
        phone.play_out();
        assert_eq!(phone.phases(), [Phase::Done, Phase::Done]);
        assert_eq!(
            phone.app().conversation().turns[1].reply.as_deref(),
            Some(LONG_ANSWER)
        );
        let scroll = CHAT.scroll_id();
        assert!(
            phone.ui.max_scroll_offset(scroll).unwrap_or(0.0) > 100.0,
            "{name}: a long answer scrolls"
        );
        // The whole of it is inside the screen's width.
        let shown = phone.rect(("reply", 1usize));
        assert!(shown.x >= 0.0 && shown.x + shown.width <= phone.info.size.width as f32);

        // ---- the Library lists it
        assert!(phone.back());
        assert_eq!(phone.screen(), HOME);
        phone.tap(("tab", Tab::Library));
        let listed = phone.app().library();
        assert_eq!(listed.len(), 1);
        assert_eq!(
            phone.app().chats()[listed[0]].title,
            "Buying or leasing a car"
        );
        assert!(phone.shows(("chat", listed[0])));
        phone.quiet();

        // ---- another day: the server lists it, the app reopens and deletes it
        let mut phone = Phone::online(
            info,
            false,
            &[
                summary(CHAT_B, "A week in Lisbon", 1),
                summary(CHAT_A, "Buying or leasing a car", 2),
            ],
        );
        phone.tap(("tab", Tab::Library));
        let listed = phone.app().library();
        assert_eq!(listed.len(), 2);
        // Newest last in the list, so newest first on the screen.
        let (older, newer) = (listed[0], listed[1]);
        assert_eq!(phone.app().chats()[newer].title, "A week in Lisbon");
        assert!(phone.rect(("chat", newer)).y < phone.rect(("chat", older)).y);
        assert_eq!(*phone.app().chats()[older].loaded(), Loaded::Summary);

        phone.tap(("chat", older));
        assert_eq!(phone.screen(), CHAT);
        assert_eq!(*phone.app().conversation().loaded(), Loaded::Loading);
        assert!(phone.shows("loading"));
        assert!(!phone.shows("delete-chat"), "nothing to delete yet");
        phone.reply(
            HttpMethod::Get,
            &format!("/v1/chats/{CHAT_A}"),
            json(stored(CHAT_A, "Buying or leasing a car")),
        );
        let chat = phone.app().conversation().clone();
        assert_eq!(*chat.loaded(), Loaded::Full);
        assert_eq!(chat.turns.len(), 2);
        assert_eq!(chat.turns[0].attachment.as_deref(), Some("quote-a.pdf"));
        assert_eq!(chat.turns[0].phase, Phase::Done);
        assert_eq!(
            chat.turns[1].reply.as_deref(),
            Some("Over ten years buying is cheaper.")
        );
        assert!(!phone.shows("loading"));
        // A reopened chat can be continued.
        phone.send("Thanks");
        phone.reply(
            HttpMethod::Post,
            &format!("/v1/chats/{CHAT_A}/messages"),
            json(reply(
                CHAT_A,
                "Buying or leasing a car",
                "Thanks",
                "Any time.",
                "fast",
                3,
            )),
        );
        phone.play_out();
        assert_eq!(phone.phases(), [Phase::Done; 3]);

        // Deleting is asked twice.
        phone.tap("delete-chat");
        assert!(phone.app().confirming_delete());
        phone.quiet();
        phone.tap("delete-chat");
        assert!(phone.app().deleting());
        phone.reply(
            HttpMethod::Delete,
            &format!("/v1/chats/{CHAT_A}"),
            json(r#"{"deleted": true}"#),
        );
        phone.settle();
        assert_eq!(phone.screen(), Screen::Library, "{name}");
        assert_eq!(phone.app().library().len(), 1);
        assert_eq!(phone.app().notice(), Some("Chat deleted."));
        assert!(
            phone
                .app()
                .chats()
                .iter()
                .all(|chat| chat.id() != Some(CHAT_A))
        );
        phone.quiet();
    }
}

#[test]
fn a_tap_elsewhere_takes_back_the_first_tap_on_delete() {
    let mut phone = Phone::online(galaxy_a27(), false, &[summary(CHAT_A, "A chat", 2)]);
    phone.tap(("tab", Tab::Library));
    let index = phone.app().library()[0];
    phone.tap(("chat", index));
    phone.reply(
        HttpMethod::Get,
        &format!("/v1/chats/{CHAT_A}"),
        json(stored(CHAT_A, "A chat")),
    );
    phone.tap("delete-chat");
    assert!(phone.app().confirming_delete());
    phone.tap(("steps", 0usize));
    assert!(!phone.app().confirming_delete());
    // And Back does too.
    phone.tap("delete-chat");
    assert!(phone.back());
    assert!(!phone.app().confirming_delete());
    phone.quiet();
}

// ------------------------------------------------- the model and the plan

#[test]
fn the_model_choice_is_sent_and_saved_and_so_is_the_plan() {
    for (name, info) in phones() {
        let mut phone = Phone::online(info, false, &[]);
        assert_eq!(phone.app().mode(), Mode::Auto, "{name}");

        phone.tap("model");
        phone.tap(("mode", Mode::Code));
        assert_eq!(phone.app().mode(), Mode::Code);
        let saved = phone.reply(
            HttpMethod::Put,
            "/v1/profile",
            json(profile("code", "free")),
        );
        assert_eq!(Phone::body(&saved), r#"{"model":"code"}"#);
        // The same choice again saves nothing.
        phone.tap("model");
        phone.tap(("mode", Mode::Code));
        phone.quiet();

        phone.send("Reverse a string in Rust");
        let asked = phone.expect(HttpMethod::Post, "/v1/chats");
        assert_eq!(
            Phone::body(&asked),
            r#"{"text":"Reverse a string in Rust","model":"code"}"#
        );
        phone.reply(
            HttpMethod::Post,
            "/v1/chats",
            json(reply(
                CHAT_A,
                "Reversing a string",
                "Reverse a string in Rust",
                "```rust\nlet r: String = s.chars().rev().collect();\n```",
                "code",
                1,
            )),
        );
        phone.play_out();
        // The choice was not Auto: the answer does not say who answered.
        assert_eq!(phone.app().conversation().turns[0].model_used, None);
        // With an attachment, its name goes too (a name only).
        phone.tap("attach");
        phone.send("Read these");
        let asked = phone.expect(HttpMethod::Post, &format!("/v1/chats/{CHAT_A}/messages"));
        assert_eq!(
            Phone::body(&asked),
            r#"{"text":"Read these","model":"code","attachments":[{"name":"3 PDFs","kind":"file"}]}"#
        );
        phone.reply(
            HttpMethod::Post,
            &format!("/v1/chats/{CHAT_A}/messages"),
            json(reply(
                CHAT_A,
                "Reversing a string",
                "Read these",
                "I cannot open them.",
                "code",
                2,
            )),
        );
        phone.play_out();

        // The plan: a demo, saved all the same.
        assert!(phone.back());
        phone.tap("profile");
        assert_eq!(phone.screen(), Screen::Plans);
        phone.tap(("plan", Plan::Pro));
        assert_eq!(phone.app().plan(), Some(Plan::Pro));
        let saved = phone.reply(HttpMethod::Put, "/v1/profile", json(profile("code", "pro")));
        assert_eq!(Phone::body(&saved), r#"{"plan":"pro"}"#);
        assert!(
            phone
                .app()
                .notice()
                .is_some_and(|text| text.contains("nothing was charged"))
        );

        // A save that fails says so; the choice stays on the screen.
        phone.tap("get-plus");
        phone.reply(
            HttpMethod::Put,
            "/v1/profile",
            error(503, "database_unavailable", "x"),
        );
        assert_eq!(phone.app().plan(), Some(Plan::Plus));
        assert_eq!(
            phone.app().notice(),
            Some("Not saved on the server. The server\u{2019}s database is not answering.")
        );
        assert!(phone.app().link().is_online(), "the server did answer");
        phone.quiet();
    }
}

// ---------------------------------------------------------------- failures

/// Every way a message can fail: the text stays, one line says why, and
/// "Retry" sends the same message again.
#[test]
fn a_message_that_fails_keeps_its_text_and_can_be_retried() {
    let failures: [(Wire, &str, bool); 8] = [
        (
            error(
                502,
                "upstream_failed",
                "Atlas could not answer just now. Please try again.",
            ),
            "Atlas could not answer just now.",
            false,
        ),
        (
            error(503, "database_unavailable", "x"),
            "The server\u{2019}s database is not answering.",
            false,
        ),
        (
            Ok(HttpResponse::new(429).with_header("Retry-After", "20")),
            "Too many requests. Wait 20 seconds and try again.",
            false,
        ),
        (
            error(500, "internal_error", "Something went wrong on the server."),
            "The server had a problem (500).",
            false,
        ),
        (
            json(r#"{"chat_id": "h_1", "title": "T"}"#),
            "The server sent something this app does not understand.",
            false,
        ),
        (
            json("<html>"),
            "The server sent something this app does not understand.",
            false,
        ),
        (
            Err(HttpError::Timeout),
            "The server took too long to answer.",
            true,
        ),
        (
            Err(HttpError::Offline),
            "The server could not be reached.",
            true,
        ),
    ];
    for (name, info) in phones() {
        for (failure, sentence, unreachable) in failures.clone() {
            let mut phone = Phone::online(info, false, &[]);
            phone.send("Compare buying and leasing a car");
            phone.reply(HttpMethod::Post, "/v1/chats", failure);
            let turn = phone.app().conversation().turns[0].clone();
            assert_eq!(turn.phase, Phase::Failed, "{name}: {sentence}");
            assert_eq!(turn.error.as_deref(), Some(sentence));
            // What was typed is still there, in its bubble; nothing is lost.
            assert_eq!(turn.prompt, "Compare buying and leasing a car");
            assert!(!turn.waiting());
            assert!(phone.shows(("failed", 0usize)) && phone.shows(("retry", 0usize)));
            assert!(!phone.app().answering(), "nothing spins");
            assert_eq!(phone.app().link().is_online(), !unreachable);
            phone.quiet();

            // Retry: the same message, as a first message again.
            phone.tap(("retry", 0usize));
            assert_eq!(phone.phases(), [Phase::Thinking]);
            let again = phone.expect(HttpMethod::Post, "/v1/chats");
            assert_eq!(
                Phone::body(&again),
                r#"{"text":"Compare buying and leasing a car","model":"auto"}"#
            );
            phone.reply(
                HttpMethod::Post,
                "/v1/chats",
                json(reply(
                    CHAT_A,
                    "Buying or leasing",
                    "x",
                    "Buying.",
                    "fast",
                    1,
                )),
            );
            phone.play_out();
            assert_eq!(phone.phases(), [Phase::Done]);
            assert_eq!(phone.app().conversation().turns.len(), 1, "sent once");
            if unreachable {
                // It answers again: the app asks what else it has.
                phone.connect(
                    false,
                    "auto",
                    "free",
                    &[summary(CHAT_A, "Buying or leasing", 1)],
                );
                assert_eq!(phone.screen(), CHAT, "the chat stays open");
                assert_eq!(phone.app().conversation().id(), Some(CHAT_A));
                assert_eq!(phone.app().library().len(), 1);
            }
            phone.quiet();
        }
    }
}

#[test]
fn a_failed_message_in_an_existing_chat_and_a_chat_that_is_gone() {
    let mut phone = Phone::online(small(), false, &[]);
    phone.send("One");
    phone.reply(
        HttpMethod::Post,
        "/v1/chats",
        json(reply(CHAT_A, "Numbers", "One", "1.", "fast", 1)),
    );
    phone.play_out();
    let path = format!("/v1/chats/{CHAT_A}/messages");

    // A second message fails: the first exchange is untouched.
    phone.send("Two");
    phone.reply(HttpMethod::Post, &path, error(502, "upstream_failed", "x"));
    assert_eq!(phone.phases(), [Phase::Done, Phase::Failed]);
    // While it is failed, another message can still be sent after it.
    phone.tap(("retry", 1usize));
    // One message of a chat at a time: what is typed meanwhile stays typed.
    phone.tap("draft");
    phone.type_text("Three");
    phone.tap("send");
    assert_eq!(phone.app().draft(), "Three");
    assert_eq!(
        phone.app().notice(),
        Some("Atlas is still answering your last message.")
    );
    assert_eq!(phone.requests().len(), 1);

    // The server no longer has the chat: the next try starts a new one.
    phone.reply(
        HttpMethod::Post,
        &path,
        error(404, "not_found", "No such chat."),
    );
    assert_eq!(phone.phases(), [Phase::Done, Phase::Failed]);
    assert_eq!(
        phone.app().conversation().turns[1].error.as_deref(),
        Some("The server no longer has this chat.")
    );
    assert_eq!(phone.app().conversation().id(), None);
    phone.tap(("retry", 1usize));
    phone.reply(
        HttpMethod::Post,
        "/v1/chats",
        json(reply(CHAT_B, "Numbers again", "Two", "2.", "fast", 1)),
    );
    phone.play_out();
    assert_eq!(phone.app().conversation().id(), Some(CHAT_B));
    assert_eq!(phone.phases(), [Phase::Done, Phase::Done]);
    phone.quiet();
}

#[test]
fn a_server_that_cannot_be_used_leaves_the_scripted_app() {
    let cases: [(Vec<Wire>, &str); 6] = [
        (
            vec![Err(HttpError::Offline)],
            "The server could not be reached.",
        ),
        (
            vec![Err(HttpError::Timeout)],
            "The server took too long to answer.",
        ),
        (
            vec![json(
                r#"{"ok": false, "model": true, "voice": false, "database": false}"#,
            )],
            "The server\u{2019}s database is not answering.",
        ),
        (
            vec![json(
                r#"{"ok": false, "model": false, "voice": false, "database": true}"#,
            )],
            "The server has no AI model key.",
        ),
        (
            vec![json(health(false)), error(401, "unauthorized", "x")],
            "The server refused this app: it wants a token.",
        ),
        (
            vec![
                json(health(false)),
                json(profile("auto", "free")),
                json("[]"),
            ],
            "The server sent something this app does not understand.",
        ),
    ];
    for (name, info) in phones() {
        for (answers, why) in cases.clone() {
            let mut phone = Phone::launch(info);
            for (answer, path) in
                answers
                    .into_iter()
                    .zip(["/v1/health", "/v1/profile", "/v1/chats"])
            {
                phone.reply(HttpMethod::Get, path, answer);
            }
            assert_eq!(*phone.app().link(), Link::Offline(why.to_owned()), "{name}");
            phone.quiet();
            phone.go_home();
            // Arriving on Home asks again; it still does not answer.
            phone.reply(HttpMethod::Get, "/v1/health", Err(HttpError::Offline));
            assert!(matches!(phone.app().link(), Link::Offline(_)));
            assert!(phone.shows("link"), "the mark says offline");

            // Today's scripted behaviour, exactly: no request, the script's
            // answer by the timer, the design's conversation in the Library.
            phone.send("Compare the prices in these quotes");
            phone.quiet();
            assert_eq!(phone.screen(), CHAT);
            assert!(!phone.app().conversation().is_remote());
            phone.play_out();
            let turn = &phone.app().conversation().turns[0];
            assert_eq!(turn.phase, Phase::Done);
            assert_eq!(turn.steps.len(), 4);
            assert!(
                turn.reply
                    .as_deref()
                    .is_some_and(|reply| reply.starts_with("Quote B"))
            );
            assert!(turn.intro.is_some());
            assert!(!phone.shows("delete-chat"));
            assert!(phone.back());
            // Home again: asked again. This time it is there.
            phone.connect(
                false,
                "fast",
                "basic",
                &[summary(CHAT_A, "From the server", 1)],
            );
            assert_eq!(phone.app().mode(), Mode::Fast);
            phone.tap(("tab", Tab::Library));
            let listed = phone.app().library();
            assert_eq!(listed.len(), 1);
            assert_eq!(phone.app().chats()[listed[0]].title, "From the server");
            // The scripted ones are kept, for when it is offline again.
            assert_eq!(phone.app().chats().len(), 3);
            phone.quiet();
        }
    }
}

#[test]
fn going_online_closes_a_scripted_chat_and_live_voice_without_a_voice_is_the_demo() {
    for (_, info) in phones() {
        let mut phone = Phone::launch(info);
        phone.reply(HttpMethod::Get, "/v1/health", Err(HttpError::Offline));
        phone.go_home();
        phone.send("Hello");
        phone.play_out();
        assert_eq!(phone.screen(), CHAT);
        // The check from Home is still out; it now succeeds, under the chat.
        phone.connect(false, "auto", "free", &[]);
        phone.settle();
        assert_eq!(
            phone.screen(),
            HOME,
            "a scripted chat is not continued online"
        );

        // No voice on the server: the microphone opens the demo, and says so.
        phone.tap("mic");
        assert_eq!(phone.screen(), Screen::LiveVoice);
        assert_eq!(phone.app().voice().live, Listen::Demo);
        assert_eq!(phone.pending(), [], "nothing is recorded");
        phone.ticks(30);
        assert!(phone.app().voice().words > 0);
        // Its line is a message like any other: it goes to the server.
        phone.tap("end");
        assert_eq!(phone.screen(), CHAT);
        let asked = phone.expect(HttpMethod::Post, "/v1/chats");
        let said = phone.app().conversation().turns[0].prompt.clone();
        assert!(VOICE_LINE.starts_with(&said) && !said.is_empty());
        assert!(Phone::body(&asked).contains(&said));
        assert_eq!(asked.header_value("X-Speak"), None);
        phone.reply(
            HttpMethod::Post,
            "/v1/chats",
            json(reply(
                CHAT_A,
                "A meeting",
                &said,
                "I cannot hear recordings.",
                "fast",
                1,
            )),
        );
        phone.play_out();
        phone.quiet();
    }
}

#[test]
fn a_chat_that_cannot_be_loaded_or_deleted_says_so() {
    for (name, info) in phones() {
        let mut phone = Phone::online(
            info,
            false,
            &[summary(CHAT_B, "Second", 1), summary(CHAT_A, "First", 2)],
        );
        phone.tap(("tab", Tab::Library));
        let listed = phone.app().library();
        let path_a = format!("/v1/chats/{CHAT_A}");

        // ---- loading fails: a line and "Retry"
        phone.tap(("chat", listed[0]));
        phone.reply(
            HttpMethod::Get,
            &path_a,
            error(503, "database_unavailable", "x"),
        );
        assert_eq!(
            *phone.app().conversation().loaded(),
            Loaded::Failed("The server\u{2019}s database is not answering.".to_owned()),
            "{name}"
        );
        assert!(phone.shows("reload"));
        // Nothing can be sent into a chat that is not here.
        phone.tap("draft");
        phone.type_text("Hi");
        phone.tap("send");
        assert_eq!(phone.app().draft(), "Hi");
        assert_eq!(phone.app().notice(), Some("This chat has not loaded yet."));
        phone.quiet();
        phone.tap("reload");
        assert_eq!(*phone.app().conversation().loaded(), Loaded::Loading);
        phone.reply(HttpMethod::Get, &path_a, json(stored(CHAT_A, "First")));
        assert_eq!(phone.app().conversation().turns.len(), 2);

        // ---- deleting fails: the chat stays
        phone.tap("delete-chat");
        phone.tap("delete-chat");
        phone.reply(
            HttpMethod::Delete,
            &path_a,
            error(500, "internal_error", "x"),
        );
        assert_eq!(phone.screen(), CHAT);
        assert!(!phone.app().deleting());
        assert_eq!(
            phone.app().notice(),
            Some("Not deleted. The server had a problem (500).")
        );
        // Deleted elsewhere already: as good as deleted.
        phone.tap("delete-chat");
        phone.tap("delete-chat");
        phone.reply(
            HttpMethod::Delete,
            &path_a,
            error(404, "not_found", "No such chat."),
        );
        phone.settle();
        assert_eq!(phone.screen(), Screen::Library);
        assert_eq!(phone.app().library().len(), 1);

        // ---- a chat that is gone when it is opened leaves the list
        let listed = phone.app().library();
        phone.tap(("chat", listed[0]));
        phone.reply(
            HttpMethod::Get,
            &format!("/v1/chats/{CHAT_B}"),
            error(404, "not_found", "No such chat."),
        );
        phone.settle();
        assert_eq!(phone.screen(), Screen::Library);
        assert_eq!(phone.app().library(), []);
        assert!(phone.shows("no-chats"));
        assert_eq!(
            phone.app().notice(),
            Some("That chat is no longer on the server.")
        );

        // ---- the server stops answering while a chat loads: offline
        let mut phone = Phone::online(info, false, &[summary(CHAT_A, "First", 2)]);
        phone.tap(("tab", Tab::Library));
        let listed = phone.app().library();
        phone.tap(("chat", listed[0]));
        phone.reply(HttpMethod::Get, &path_a, Err(HttpError::Offline));
        assert!(matches!(phone.app().link(), Link::Offline(_)));
        assert!(phone.shows("reload"));
        phone.quiet();
    }
}

// ------------------------------------------------------------ late answers

#[test]
fn late_answers_are_ignored() {
    for (name, info) in phones() {
        // ---- "Cancel task" abandons the request; its answer changes nothing
        let mut phone = Phone::online(info, false, &[]);
        phone.send("A slow question");
        let (first, _) = phone.find(HttpMethod::Post, "/v1/chats");
        phone.tap(("steps", 0usize));
        assert!(
            phone.shows(("cancel", 0usize)),
            "{name}: a waiting message can be cancelled"
        );
        phone.tap(("cancel", 0usize));
        assert_eq!(phone.phases(), [Phase::Cancelled]);
        assert!(!phone.app().answering(), "the timer stops");
        let before = phone.app().conversation().clone();
        assert!(phone.ui.resolve(
            first,
            ServiceResult::Http(json(reply(
                CHAT_A,
                "Late",
                "A slow question",
                "Late answer.",
                "fast",
                1
            )))
        ));
        phone.draw();
        phone.ticks(60);
        assert_eq!(*phone.app().conversation(), before, "{name}");
        assert_eq!(before.id(), None);

        // The next message of that conversation starts a chat of its own.
        phone.send("A quick one");
        phone.reply(
            HttpMethod::Post,
            "/v1/chats",
            json(reply(CHAT_B, "Quick", "A quick one", "Done.", "fast", 1)),
        );
        phone.play_out();
        assert_eq!(phone.phases(), [Phase::Cancelled, Phase::Done]);
        assert_eq!(phone.app().conversation().id(), Some(CHAT_B));

        // ---- a retry's first answer arrives after the second was sent
        phone.send("Again");
        let path = format!("/v1/chats/{CHAT_B}/messages");
        phone.reply(HttpMethod::Post, &path, error(502, "upstream_failed", "x"));
        phone.tap(("retry", 2usize));
        let (second, _) = phone.find(HttpMethod::Post, &path);
        // Cancel that one too, retry is not offered for a cancelled turn.
        phone.tap(("steps", 2usize));
        phone.tap(("cancel", 2usize));
        assert!(
            phone
                .ui
                .resolve(second, ServiceResult::Http(Err(HttpError::Timeout)))
        );
        phone.draw();
        assert_eq!(phone.phases()[2], Phase::Cancelled);
        assert!(
            phone.app().link().is_online(),
            "a cancelled request's failure is nobody's"
        );
        phone.quiet();

        // ---- an answer for a chat that has been deleted meanwhile
        let mut phone = Phone::online(info, false, &[summary(CHAT_A, "First", 2)]);
        phone.tap(("tab", Tab::Library));
        let listed = phone.app().library();
        phone.tap(("chat", listed[0]));
        let (load, _) = phone.find(HttpMethod::Get, &format!("/v1/chats/{CHAT_A}"));
        // Left while loading, then the address changes: the chat is gone.
        assert!(phone.back());
        phone.tap(("tab", Tab::Home));
        phone.tap("link");
        phone.tap("server-url");
        phone.clear_field();
        phone.type_text("http://localhost:9000");
        phone.tap("server-check");
        phone.base = "http://localhost:9000".to_owned();
        assert_eq!(
            phone.app().chats().len(),
            1,
            "the other server's chats are gone"
        );
        assert!(
            phone
                .ui
                .resolve(load, ServiceResult::Http(json(stored(CHAT_A, "First"))))
        );
        phone.draw();
        assert_eq!(phone.app().chats().len(), 1);
        assert_eq!(
            phone.app().chats()[0].turns.len(),
            1,
            "the design's conversation, untouched"
        );
        phone.reply(HttpMethod::Get, "/v1/health", json(health(false)));
        phone.reply(
            HttpMethod::Get,
            "/v1/profile",
            json(profile("auto", "free")),
        );
        phone.reply(HttpMethod::Get, "/v1/chats", json(chats(&[])));
        phone.quiet();
    }
}

#[test]
fn an_older_check_of_the_server_does_not_overrule_a_newer_one() {
    let mut phone = Phone::launch(galaxy_a27());
    let (first, _) = phone.find(HttpMethod::Get, "/v1/health");
    phone.go_home();
    phone.tap("model");
    phone.tap("all-models");
    phone.tap("server");
    assert_eq!(phone.screen(), Screen::Server);
    phone.tap("server-check");
    let waiting = phone.requests();
    assert_eq!(waiting.len(), 2);
    let second = waiting[1].0;
    // The newer one is answered first, and well.
    assert!(
        phone
            .ui
            .resolve(second, ServiceResult::Http(json(health(true))))
    );
    phone.draw();
    phone.reply(
        HttpMethod::Get,
        "/v1/profile",
        json(profile("auto", "free")),
    );
    phone.reply(HttpMethod::Get, "/v1/chats", json(chats(&[])));
    assert_eq!(*phone.app().link(), Link::Online { voice: true });
    // The older one fails afterwards: nobody is listening.
    assert!(
        phone
            .ui
            .resolve(first, ServiceResult::Http(Err(HttpError::Offline)))
    );
    phone.draw();
    assert_eq!(*phone.app().link(), Link::Online { voice: true });
    phone.quiet();
}

// -------------------------------------------------------------- the server

#[test]
fn the_server_address_can_be_changed_and_is_remembered() {
    for (name, info) in phones() {
        let mut phone = Phone::online(info, false, &[summary(CHAT_A, "First", 1)]);
        phone.tap("model");
        phone.tap("all-models");
        assert_eq!(phone.screen(), Screen::Models);
        phone.tap("server");
        assert_eq!(phone.screen(), Screen::Server, "{name}");
        assert_eq!(phone.app().server_draft(), BASE);

        // ---- an address that cannot be one
        phone.tap("server-url");
        assert!(phone.ui.is_focused("server-url"));
        phone.clear_field();
        assert_eq!(phone.app().server_draft(), "");
        phone.tap("server-check");
        assert_eq!(
            phone.app().server_problem(),
            Some("Type the server\u{2019}s address first.")
        );
        phone.tap("server-url");
        phone.type_text("atlas.example.com");
        phone.tap("server-check");
        assert_eq!(
            phone.app().server_problem(),
            Some("The address must start with http:// or https://.")
        );
        assert_eq!(phone.app().server_url(), BASE, "nothing changed yet");
        assert_eq!(phone.ui.stored(SERVER_KEY), None);
        phone.quiet();

        // ---- another server: stored, asked, and the old one's chats go
        phone.tap("server-url");
        phone.clear_field();
        phone.type_text("HTTPS://atlas.example.com/api/");
        phone.tap("server-check");
        assert_eq!(phone.app().server_problem(), None);
        assert_eq!(phone.app().server_url(), "https://atlas.example.com/api");
        assert_eq!(phone.app().server_draft(), "https://atlas.example.com/api");
        assert_eq!(
            phone.ui.stored(SERVER_KEY),
            Some("https://atlas.example.com/api")
        );
        assert_eq!(*phone.app().link(), Link::Checking);
        assert_eq!(phone.app().chats().len(), 1);
        phone.base = "https://atlas.example.com/api".to_owned();
        phone.reply(HttpMethod::Get, "/v1/health", Err(HttpError::Offline));
        assert_eq!(
            *phone.app().link(),
            Link::Offline("The server could not be reached.".to_owned())
        );

        // ---- the default again
        phone.tap("server-default");
        phone.base = BASE.to_owned();
        assert_eq!(phone.app().server_url(), BASE);
        assert_eq!(phone.ui.stored(SERVER_KEY), Some(BASE));
        phone.connect(true, "auto", "free", &[summary(CHAT_A, "First", 1)]);
        assert_eq!(phone.app().library().len(), 1);
        // Checking the same address again is just a check.
        phone.tap("server-check");
        phone.connect(true, "auto", "free", &[summary(CHAT_A, "First", 1)]);
        assert_eq!(phone.app().library().len(), 1);

        // ---- the next launch starts from the stored address
        let host = HostState::new()
            .with_install_id(INSTALL)
            .with_stored(SERVER_KEY, "http://10.0.2.2:8788");
        let mut phone = Phone::launch_with(info, &host, Atlas::default());
        phone.base = "http://10.0.2.2:8788".to_owned();
        phone.expect(HttpMethod::Get, "/v1/health");
        assert_eq!(phone.app().server_url(), "http://10.0.2.2:8788");
        // A stored address that is not one is not used.
        let host = HostState::new()
            .with_install_id(INSTALL)
            .with_stored(SERVER_KEY, "javascript:alert(1)");
        let phone = Phone::launch_with(info, &host, Atlas::default());
        phone.expect(HttpMethod::Get, "/v1/health");
    }
}

#[test]
fn with_nothing_behind_the_app_it_is_the_designed_one() {
    // `Ui::new`: every request fails as "unsupported" at the next frame.
    let mut ui = Ui::new(Atlas::on(HOME));
    let mut frame = Frame::for_app(&mut ui);
    for millis in [0, 400, 800] {
        frame.begin();
        ui.frame(
            &galaxy_a27().with_elapsed(Duration::from_millis(millis)),
            &mut frame,
        );
    }
    assert_eq!(*ui.app().link(), Link::Standalone);
    assert!(ui.rect_of("link").is_none(), "no mark on the designed home");
    assert_eq!(ui.app().library(), [0]);
    assert!(!frame.redraw_requested() && frame.redraw_after().is_none());
}

// ---------------------------------------------------------------- voice

/// Speak, be heard, see the transcript, land in the chat with the answer
/// playing, and stop it.
#[test]
fn the_voice_journey() {
    for (name, info) in phones() {
        let mut phone = Phone::online(info, true, &[]);

        // ---- from Home: a new chat
        let asked = phone.say("/v1/chats");
        assert_eq!(asked.body, recording(2).bytes);
        assert_eq!(asked.header_value("Content-Type"), Some("audio/mp4"));
        assert_eq!(asked.header_value("X-Speak"), Some("1"));
        assert_eq!(asked.header_value("X-Model"), Some("auto"));
        assert_eq!(asked.timeout, Duration::from_secs(120));
        phone.ticks(30);
        assert_eq!(phone.app().voice().live, Listen::Sending, "{name}");

        phone.reply(
            HttpMethod::Post,
            "/v1/chats",
            json(reply_with(
                CHAT_A,
                "Capital of Tanzania",
                HEARD,
                "Dodoma is the capital of Tanzania.",
                "fast",
                1,
                Some(AUDIO),
            )),
        );
        // The transcript shows, in the design's quotes, before the chat does.
        assert_eq!(phone.app().voice().live, Listen::Heard);
        assert_eq!(phone.app().voice_heard(), Some(HEARD));
        assert_eq!(phone.screen(), Screen::LiveVoice);
        assert!(phone.shows("heard"));
        // Its audio is fetched meanwhile.
        phone.reply(HttpMethod::Get, AUDIO, mp3(b"spoken answer"));
        assert_eq!(phone.pending(), [], "not played before the chat opens");

        phone.ticks(HOLD_TICKS + 1);
        assert_eq!(phone.screen(), CHAT);
        assert_eq!(phone.app().stack(), [HOME, CHAT]);
        let chat = phone.app().conversation().clone();
        assert_eq!(chat.title, "Capital of Tanzania");
        assert_eq!(chat.id(), Some(CHAT_A));
        assert_eq!(chat.turns.len(), 1);
        assert_eq!(chat.turns[0].prompt, HEARD);
        assert!(matches!(chat.turns[0].phase, Phase::Working(_)));
        // Since the user spoke, the answer is spoken.
        assert_eq!(
            phone.pending(),
            [ServiceRequest::Play(AudioClip::new(
                b"spoken answer".to_vec(),
                "audio/mpeg"
            ))]
        );
        assert_eq!(phone.app().speaking(), Some(0));
        assert!(phone.shows(("stop-speaking", 0usize)));
        phone.play_out();
        assert_eq!(phone.phases(), [Phase::Done]);
        // It ends by itself.
        assert!(phone.ui.answer_playback(Ok(())));
        phone.draw();
        assert_eq!(phone.app().speaking(), None);
        assert!(!phone.shows(("stop-speaking", 0usize)));
        phone.quiet();

        // ---- from the chat: the same chat, and "Stop" stops the speech
        let path = format!("/v1/chats/{CHAT_A}/messages");
        phone.say(&path);
        phone.reply(
            HttpMethod::Post,
            &path,
            json(reply_with(
                CHAT_A,
                "Capital of Tanzania",
                "And of Kenya?",
                "Nairobi.",
                "fast",
                2,
                Some(AUDIO),
            )),
        );
        // The cross opens the chat at once; the audio arrives after.
        phone.tap("end");
        assert_eq!(phone.app().stack(), [HOME, CHAT]);
        assert_eq!(phone.app().conversation().turns.len(), 2);
        assert_eq!(phone.app().conversation().turns[1].prompt, "And of Kenya?");
        assert_eq!(phone.app().speaking(), Some(1), "its audio is on the way");
        assert_eq!(phone.pending(), []);
        phone.reply(HttpMethod::Get, AUDIO, mp3(b"nairobi"));
        assert_eq!(
            phone.pending(),
            [ServiceRequest::Play(AudioClip::new(
                b"nairobi".to_vec(),
                "audio/mpeg"
            ))]
        );
        phone.tap(("stop-speaking", 1usize));
        assert_eq!(phone.app().speaking(), None);
        // The platform reports the clip it stopped as interrupted (the
        // driver does that by itself); nobody minds.
        phone.ticks(2);
        assert_eq!(phone.pending(), []);
        assert_eq!(phone.app().speaking(), None);
        phone.play_out();
        phone.quiet();

        // ---- leaving the chat stops a spoken answer too
        phone.say(&path);
        phone.reply(
            HttpMethod::Post,
            &path,
            json(reply_with(
                CHAT_A,
                "Capital of Tanzania",
                "Thanks",
                "Any time.",
                "fast",
                3,
                Some(AUDIO),
            )),
        );
        phone.reply(HttpMethod::Get, AUDIO, mp3(b"any time"));
        phone.ticks(HOLD_TICKS + 1);
        assert_eq!(phone.app().speaking(), Some(2));
        assert!(phone.back());
        assert_eq!(phone.screen(), HOME);
        assert_eq!(phone.app().speaking(), None);
        assert_eq!(phone.pending(), [], "the clip was stopped");
        phone.play_out();
        phone.quiet();

        // ---- an answer that could not be voiced is still an answer
        phone.say("/v1/chats");
        phone.reply(
            HttpMethod::Post,
            "/v1/chats",
            json(reply(CHAT_B, "Hello", "Hello", "Hello to you.", "fast", 1)),
        );
        phone.ticks(HOLD_TICKS + 1);
        assert_eq!(phone.screen(), CHAT);
        assert_eq!(phone.app().speaking(), None);
        phone.play_out();
        phone.quiet();
        // And one whose audio cannot be fetched.
        let path_b = format!("/v1/chats/{CHAT_B}/messages");
        phone.say(&path_b);
        phone.reply(
            HttpMethod::Post,
            &path_b,
            json(reply_with(
                CHAT_B,
                "Hello",
                "Again",
                "Again.",
                "fast",
                2,
                Some(AUDIO),
            )),
        );
        phone.ticks(HOLD_TICKS + 1);
        assert_eq!(phone.app().speaking(), Some(1));
        phone.reply(
            HttpMethod::Get,
            AUDIO,
            error(404, "not_found", "This audio is no longer available."),
        );
        assert_eq!(phone.app().speaking(), None);
        assert_eq!(phone.pending(), []);
        phone.play_out();
        assert_eq!(phone.phases(), [Phase::Done, Phase::Done]);
        phone.quiet();
    }
}

#[test]
fn voice_failures_are_explained_and_fall_back() {
    for (name, info) in phones() {
        // ---- the microphone is refused: the demo, and why
        let mut phone = Phone::online(info, true, &[]);
        phone.tap("mic");
        assert!(
            phone
                .ui
                .answer_recording_started(Err(AudioError::PermissionDenied))
        );
        phone.draw();
        assert_eq!(phone.app().voice().live, Listen::Demo, "{name}");
        assert!(
            phone
                .app()
                .notice()
                .is_some_and(|text| text.starts_with("The microphone is not allowed")),
            "{:?}",
            phone.app().notice()
        );
        phone.ticks(40);
        assert!(phone.app().voice().words > 0, "the demo runs");
        // The typed path is still there: back, and type.
        assert!(phone.back());
        assert_eq!(phone.screen(), HOME);
        phone.quiet();
        // A microphone that is busy: the same fallback.
        phone.tap("mic");
        assert!(phone.ui.answer_recording_started(Err(AudioError::Busy)));
        phone.draw();
        assert_eq!(phone.app().voice().live, Listen::Demo);
        assert!(phone.back());

        // ---- a recording too short to hold words
        phone.tap("mic");
        assert!(phone.ui.answer_recording_started(Ok(())));
        phone.draw();
        phone.tap("mute");
        assert!(phone.ui.answer_recording(Ok(Recording {
            duration: Duration::from_millis(120),
            ..recording(0)
        })));
        phone.draw();
        assert_eq!(phone.app().voice().live, Listen::Failed);
        assert!(phone.shows("voice-hint"));
        assert_eq!(
            phone.app().voice_error(),
            Some("That was too short to hear. Tap the microphone and try again.")
        );
        assert!(!phone.shows("voice-retry"), "nothing to send again");
        phone.quiet();
        // The microphone button records again.
        phone.tap("mute");
        assert_eq!(phone.pending(), [ServiceRequest::StartRecording]);
        assert!(phone.ui.answer_recording_started(Ok(())));
        phone.draw();
        assert_eq!(phone.app().voice().live, Listen::Recording);
        // The recorder itself fails.
        phone.tap("end");
        assert_eq!(
            phone.pending(),
            [ServiceRequest::StopRecording],
            "the cross sends too"
        );
        assert!(
            phone
                .ui
                .answer_recording(Err(AudioError::Failed("no input".to_owned())))
        );
        phone.draw();
        assert_eq!(phone.app().voice().live, Listen::Failed);
        assert!(phone.back());
        phone.quiet();

        // ---- the recording does not get through: "Send again" resends it
        let failures: [(Wire, &str, bool); 4] = [
            (
                error(
                    502,
                    "upstream_failed",
                    "The voice service could not transcribe the recording.",
                ),
                "Atlas could not answer just now.",
                true,
            ),
            (
                error(400, "bad_request", "No speech was heard in the recording."),
                "No speech was heard in the recording.",
                true,
            ),
            (
                error(503, "voice_unavailable", "x"),
                "The server has no voice right now.",
                true,
            ),
            (
                Err(HttpError::Offline),
                "The server could not be reached.",
                false,
            ),
        ];
        for (failure, sentence, still_online) in failures {
            let mut phone = Phone::online(info, true, &[]);
            let first = phone.say("/v1/chats");
            phone.reply(HttpMethod::Post, "/v1/chats", failure);
            assert_eq!(phone.app().voice().live, Listen::Failed, "{sentence}");
            assert_eq!(phone.app().voice_error(), Some(sentence));
            assert_eq!(phone.app().link().is_online(), still_online);
            assert_eq!(phone.screen(), Screen::LiveVoice);
            assert!(phone.shows("voice-retry"));
            phone.tap("voice-retry");
            assert_eq!(phone.app().voice().live, Listen::Sending);
            let again = phone.expect(HttpMethod::Post, "/v1/chats");
            assert_eq!(again.body, first.body);
            phone.reply(
                HttpMethod::Post,
                "/v1/chats",
                json(reply(CHAT_A, "Capital", HEARD, "Dodoma.", "fast", 1)),
            );
            assert_eq!(phone.app().voice().live, Listen::Heard);
            phone.ticks(HOLD_TICKS + 1);
            assert_eq!(phone.screen(), CHAT);
            assert_eq!(phone.app().conversation().turns[0].prompt, HEARD);
            phone.play_out();
            if !still_online {
                phone.connect(true, "auto", "free", &[summary(CHAT_A, "Capital", 1)]);
            }
            phone.quiet();
        }
    }
}

#[test]
fn voice_that_is_left_sends_nothing_and_hears_no_late_answer() {
    for (_, info) in phones() {
        let mut phone = Phone::online(info, true, &[]);

        // ---- minimised while recording: the microphone stops, nothing is sent
        phone.tap("mic");
        assert!(phone.ui.answer_recording_started(Ok(())));
        phone.draw();
        phone.tap("minimise");
        assert_eq!(phone.screen(), HOME);
        assert_eq!(phone.pending(), [ServiceRequest::StopRecording]);
        assert!(phone.ui.answer_recording(Ok(recording(3))));
        phone.draw();
        phone.quiet();
        assert_eq!(phone.app().chats().len(), 1, "no chat was made");

        // ---- left while the microphone is still starting
        phone.tap("mic");
        assert!(phone.back());
        // The start is answered afterwards; the stop that was asked for is too.
        assert!(phone.ui.answer_recording_started(Ok(())));
        phone.draw();
        assert!(phone.ui.answer_recording(Err(AudioError::NotRecording)));
        phone.draw();
        assert_eq!(phone.screen(), HOME);
        phone.quiet();

        // ---- left while the recording is with the server: the answer is dropped
        phone.say("/v1/chats");
        let (sent, _) = phone.find(HttpMethod::Post, "/v1/chats");
        phone.tap("end");
        assert_eq!(phone.screen(), HOME);
        assert!(phone.ui.resolve(
            sent,
            ServiceResult::Http(json(reply_with(
                CHAT_A,
                "Late",
                HEARD,
                "Late.",
                "fast",
                1,
                Some(AUDIO)
            )))
        ));
        phone.draw();
        phone.ticks(HOLD_TICKS + 5);
        assert_eq!(phone.screen(), HOME);
        assert_eq!(phone.app().chats().len(), 1);
        phone.quiet();

        // ---- left while the transcript shows: no chat, and no sound
        phone.say("/v1/chats");
        phone.reply(
            HttpMethod::Post,
            "/v1/chats",
            json(reply_with(
                CHAT_A,
                "Capital",
                HEARD,
                "Dodoma.",
                "fast",
                1,
                Some(AUDIO),
            )),
        );
        let (audio, _) = phone.find(HttpMethod::Get, AUDIO);
        phone.tap("minimise");
        assert_eq!(phone.screen(), HOME);
        assert!(phone.ui.resolve(audio, ServiceResult::Http(mp3(b"late"))));
        phone.draw();
        phone.ticks(HOLD_TICKS + 5);
        assert_eq!(phone.pending(), [], "nothing is played");
        assert_eq!(phone.app().chats().len(), 1);
        phone.quiet();
    }
}

#[test]
fn a_recording_stops_by_itself_before_the_platforms_limit() {
    let mut phone = Phone::online(galaxy_a27(), true, &[]);
    phone.tap("mic");
    assert!(phone.ui.answer_recording_started(Ok(())));
    phone.draw();
    phone.ticks(RECORDING_TICKS - 40);
    assert_eq!(phone.app().voice().live, Listen::Recording);
    phone.ticks(60);
    assert_eq!(phone.app().voice().live, Listen::Stopping);
    assert_eq!(phone.pending(), [ServiceRequest::StopRecording]);
    assert!(phone.ui.answer_recording(Ok(recording(55))));
    phone.draw();
    phone.expect(HttpMethod::Post, "/v1/chats");
}

// --------------------------------------------------------- a real server

/// Carries one request to the real server with `curl`.
fn carry(request: &HttpRequest) -> Wire {
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
            request.url.starts_with("http://127.0.0.1:8788/v1/"),
            "{}",
            request.url
        );
        assert!(
            !request.url.contains("/audio/") && request.header_value("X-Speak").is_none(),
            "a typed chat asks for no speech"
        );
        let answer = carry(&request);
        println!(
            "{} {} -> {}",
            request.method.as_str(),
            request.url.trim_start_matches("http://127.0.0.1:8788"),
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

/// A typed chat against the real server, end to end. Run it with the
/// server started on 127.0.0.1:8788:
///
/// ```sh
/// cargo test -p atlas --test online -- --ignored --nocapture a_real_typed_chat
/// ```
///
/// It uses the server's language model (two messages) and never its voice:
/// nothing is recorded and no answer is asked for spoken. The device id
/// begins with `test_`, and the chat is deleted at the end (the device's
/// profile row stays; the server's test suite removes `test_` devices).
#[test]
#[ignore = "needs the Atlas server on 127.0.0.1:8788"]
fn a_real_typed_chat() {
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
        Atlas::default().with_device_prefix("test_"),
    );
    assert_eq!(phone.device, format!("test_{install}"));
    println!("device {}", phone.device);

    pump(&mut phone);
    println!("link: {}", phone.app().link().status());
    assert!(phone.app().link().is_online(), "the server must be up");
    assert_eq!(
        phone.app().library(),
        [],
        "a device the server has not seen"
    );
    assert_eq!(
        (phone.app().mode(), phone.app().plan()),
        (Mode::Auto, Some(Plan::Free))
    );
    phone.go_home();

    let show = |phone: &Phone, index: usize| {
        let chat = phone.app().conversation();
        let turn = &chat.turns[index];
        println!("  title: {}", chat.title);
        println!("  you:   {}", turn.prompt);
        println!(
            "  steps: {}",
            turn.steps
                .iter()
                .map(|step| step.doing.as_str())
                .collect::<Vec<_>>()
                .join(" / ")
        );
        println!(
            "  atlas{}: {}",
            turn.model_used
                .map(|mode| format!(" ({})", mode.name()))
                .unwrap_or_default(),
            turn.reply
                .clone()
                .unwrap_or_default()
                .replace('\n', "\n         ")
        );
    };

    // Auto, with the sample attachment: the model must say it cannot read it.
    phone.tap("attach");
    phone.send("What is the total across these three quotes?");
    assert_eq!(phone.phases(), [Phase::Thinking]);
    pump(&mut phone);
    assert_eq!(
        phone.app().conversation().turns[0].error,
        None,
        "the message failed"
    );
    phone.play_out();
    assert_eq!(phone.phases(), [Phase::Done]);
    show(&phone, 0);
    let chat = phone.app().conversation().clone();
    let id = chat.id().expect("the server's id").to_owned();
    assert!(id.starts_with("h_") && id.len() == 34);
    assert!((2..=4).contains(&chat.turns[0].steps.len()));
    assert!(chat.turns[0].model_used.is_some(), "Auto says who answered");
    assert!(!chat.title.ends_with('\u{2026}') || chat.title.chars().count() <= 40);

    // Code, chosen in the picker (saved to the profile), in the same chat.
    phone.tap("model");
    phone.tap(("mode", Mode::Code));
    pump(&mut phone);
    phone.send("Write a Rust function that reverses a string.");
    pump(&mut phone);
    phone.play_out();
    assert_eq!(phone.phases(), [Phase::Done, Phase::Done]);
    show(&phone, 1);
    let code = phone.app().conversation().turns[1].reply.clone().unwrap();
    assert!(code.contains("```"), "code comes in a fenced block");
    assert_eq!(phone.app().conversation().turns[1].model_used, None);

    // A later launch of the same installation: the profile, the Library,
    // and the chat reopened from the server.
    let mut later = Phone::launch_with(
        galaxy_a27(),
        &host,
        Atlas::default().with_device_prefix("test_"),
    );
    pump(&mut later);
    assert_eq!(later.app().mode(), Mode::Code, "the saved model");
    let listed = later.app().library();
    assert_eq!(listed.len(), 1);
    assert_eq!(later.app().chats()[listed[0]].title, chat.title);
    later.go_home();
    later.tap(("tab", Tab::Library));
    later.tap(("chat", listed[0]));
    pump(&mut later);
    let reopened = later.app().conversation().clone();
    assert_eq!(*reopened.loaded(), Loaded::Full);
    assert_eq!(reopened.turns.len(), 2);
    assert_eq!(reopened.turns[1].reply.as_deref(), Some(code.as_str()));
    assert_eq!(reopened.turns[0].attachment.as_deref(), Some("3 PDFs"));
    println!(
        "reopened: {} turns of \u{201c}{}\u{201d}",
        reopened.turns.len(),
        reopened.title
    );

    later.tap("delete-chat");
    later.tap("delete-chat");
    pump(&mut later);
    later.settle();
    assert_eq!(later.screen(), Screen::Library);
    assert_eq!(later.app().library(), []);
    println!("deleted {id}");
    later.quiet();
}
