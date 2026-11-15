//! What the tests share: a phone driven by fingers ([`Device`]) and a
//! stand-in for the Barua Talk server ([`World`]) that answers the app's
//! requests as `server/API.md` describes, from state a test can read and
//! change to play the other people.

#![allow(dead_code)]

use std::hash::Hash;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use barua_talk::json::{self, Value};
use barua_talk::{BaruaTalk, Screen};
use morningstar::ui::Ui;
use morningstar::{
    App, Frame, FrameInfo, HostState, HttpError, HttpMethod, HttpRequest, HttpResponse, Insets,
    Point, PointerEvent, PointerPhase, Rect, ServiceRequest, ServiceResult, SurfaceSize,
    TextInputEvent,
};

// ---------------------------------------------------------------- frames

/// The owner's phone: 384 x 832 dp, 36 dp of status bar and 15 dp of
/// gesture bar.
pub fn galaxy_a27() -> FrameInfo {
    FrameInfo::new(SurfaceSize::new(1080, 2340))
        .with_scale(2.8125)
        .with_insets(Insets::new(0.0, 102.0, 0.0, 42.0))
}

/// A small phone: 360 x 640 dp.
pub fn small() -> FrameInfo {
    FrameInfo::new(SurfaceSize::new(720, 1280)).with_scale(2.0)
}

/// The design's own frame.
pub fn design() -> FrameInfo {
    FrameInfo::new(SurfaceSize::new(780, 1688)).with_scale(2.0)
}

pub fn phones() -> [(&'static str, FrameInfo); 2] {
    [("Galaxy A27", galaxy_a27()), ("360 x 640", small())]
}

// ---------------------------------------------------------------- device

/// A finger must travel this far (dp) before a press becomes a scroll.
const TOUCH_SLOP: f32 = 8.0;
/// One tick of the app's timer, and a little.
pub const TICK: Duration = Duration::from_millis(260);

pub struct Device {
    pub ui: Ui<BaruaTalk>,
    pub frame: Frame,
    pub info: FrameInfo,
    pub elapsed: Duration,
}

impl Device {
    /// The scripted app: nothing behind it can make a request.
    pub fn on(screen: Screen, info: FrameInfo) -> Self {
        Self::with(Ui::new(BaruaTalk::on(screen)), info)
    }

    /// The app with `world` as its server.
    pub fn online(screen: Screen, info: FrameInfo, world: &Shared) -> Self {
        let mut device = Self::with(serve(BaruaTalk::on(screen), world), info);
        // The check's four answers arrive a frame apart.
        device.ticks(5);
        device
    }

    pub fn with(mut ui: Ui<BaruaTalk>, info: FrameInfo) -> Self {
        let frame = Frame::for_app(&mut ui);
        let mut device = Self {
            ui,
            frame,
            info,
            elapsed: Duration::ZERO,
        };
        device.draw();
        device.settle();
        device
    }

    /// A first run, or a later one with `stored` on the phone.
    pub fn launched(mut ui: Ui<BaruaTalk>, info: FrameInfo, stored: &[(&str, &str)]) -> Self {
        let mut host = HostState::new();
        for (key, value) in stored {
            host = host.with_stored(*key, *value);
        }
        let frame = Frame::for_app_with_host(&mut ui, &host);
        let mut device = Self {
            ui,
            frame,
            info,
            elapsed: Duration::ZERO,
        };
        device.draw();
        device.settle();
        device
    }

    pub fn draw(&mut self) {
        self.frame.begin();
        let info = self.info.with_elapsed(self.elapsed);
        self.ui.frame(&info, &mut self.frame);
    }

    pub fn app(&self) -> &BaruaTalk {
        self.ui.app()
    }

    pub fn screen(&self) -> Screen {
        self.app().screen()
    }

    fn scale(&self) -> f32 {
        self.info.scale
    }

    pub fn rect(&self, id: impl Hash) -> Rect {
        self.ui
            .rect_of(&id)
            .unwrap_or_else(|| panic!("{}: the widget is not on the screen", self.screen().name()))
    }

    pub fn has(&self, id: impl Hash) -> bool {
        self.ui.rect_of(&id).is_some()
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
    pub fn settle(&mut self) {
        for _ in 0..2 {
            self.elapsed += Duration::from_millis(400);
            self.draw();
        }
    }

    /// Lets `count` ticks of the app's timer pass, a frame each.
    pub fn ticks(&mut self, count: u32) {
        for _ in 0..count {
            self.elapsed += TICK;
            self.draw();
        }
    }

    /// Drags a finger inside the scroll view `view` (px) so that its
    /// content moves by `dp` towards its start (the other way for a
    /// negative `dp`), along x if `sideways`.
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
    /// is in: Home's rings sideways, the screen's own up and down.
    pub fn reveal<I: Hash>(&mut self, id: &I) {
        let margin = 4.0 * self.scale();
        for _ in 0..60 {
            let rect = self.rect(id);
            let shown = self.ui.visible_rect_of(id).unwrap_or_default();
            if shown.width >= rect.width - 1.0 && shown.height >= rect.height - 1.0 {
                return;
            }
            let rings = self.ui.rect_of("rings");
            let page = self.ui.rect_of(self.screen().scroll_id());
            let cut_sideways = shown.width < rect.width - 1.0;
            if let Some(view) = rings.filter(|view| {
                cut_sideways && rect.y >= view.y - 1.0 && rect.y < view.y + view.height
            }) {
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
    pub fn tap_at(&mut self, point: Point) {
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
    pub fn tap(&mut self, id: impl Hash) {
        self.reveal(&id);
        let rect = self.rect(&id);
        self.tap_at(Point::new(
            rect.x + rect.width / 2.0,
            rect.y + rect.height / 2.0,
        ));
    }

    /// The system's Back. Returns whether the app handled it.
    pub fn back(&mut self) -> bool {
        let handled = self.ui.back();
        self.draw();
        self.settle();
        handled
    }

    pub fn key(&mut self, event: TextInputEvent) {
        self.ui.text_input(&event);
        self.draw();
    }

    pub fn type_text(&mut self, text: &str) {
        for character in text.chars() {
            self.key(TextInputEvent::Commit(character.to_string()));
        }
    }

    /// Taps a field and types into it.
    pub fn fill(&mut self, field: impl Hash, text: &str) {
        self.tap(field);
        self.type_text(text);
    }

    /// True if the last frame asked for nothing more: the app is idle.
    pub fn idle(&self) -> bool {
        !self.frame.redraw_requested() && self.frame.redraw_after().is_none()
    }
}

// ------------------------------------------------------------ the server

/// The server's clock in every answer.
pub const NOW: &str = "2026-11-15T07:01:12Z";

#[derive(Clone, Debug)]
pub struct StoredMessage {
    pub id: u64,
    pub from: String,
    pub from_name: String,
    pub text: String,
    pub client_id: String,
}

#[derive(Clone, Debug)]
pub struct StoredRoom {
    pub name: String,
    pub display_name: String,
    pub members: u32,
    pub live: bool,
    pub recording: Option<String>,
    pub messages: Vec<StoredMessage>,
}

#[derive(Clone, Debug)]
pub struct StoredCall {
    pub id: String,
    /// "incoming" or "outgoing", from the app's side.
    pub direction: &'static str,
    pub status: &'static str,
    pub video: bool,
    pub peer: (String, String),
    pub seconds: u32,
    pub recording: Option<String>,
}

#[derive(Clone, Debug)]
pub struct StoredPhone {
    pub id: String,
    pub to: String,
    pub status: &'static str,
    pub seconds: u32,
    pub price: Option<u32>,
    pub request_id: String,
}

/// The server, as far as one app can tell.
#[derive(Debug)]
pub struct World {
    /// Nothing answers: every request fails as offline.
    pub down: bool,
    pub talk: bool,
    pub database: bool,
    pub phone: bool,
    pub can_hang_up: bool,
    pub me: Option<(String, String)>,
    pub people: Vec<(String, String, bool)>,
    pub rooms: Vec<StoredRoom>,
    pub calls: Vec<StoredCall>,
    pub phones: Vec<StoredPhone>,
    next_id: u64,
    /// `METHOD path`, in the order asked.
    pub log: Vec<String>,
    /// The next request whose path contains `.0` is answered with status
    /// `.1`, error code `.2` and message `.3`, once.
    pub fail: Option<(&'static str, u16, &'static str, &'static str)>,
    /// Requests whose path contains this are left unanswered.
    pub hold: Option<&'static str>,
}

pub type Shared = Arc<Mutex<World>>;

/// A server with one room and one other person.
pub fn world() -> Shared {
    Arc::new(Mutex::new(World {
        down: false,
        talk: true,
        database: true,
        phone: true,
        can_hang_up: false,
        me: None,
        people: vec![("baraka".to_owned(), "Baraka Mushi".to_owned(), true)],
        rooms: vec![StoredRoom {
            name: "support-desk".to_owned(),
            display_name: "Support desk".to_owned(),
            members: 3,
            live: true,
            recording: None,
            messages: vec![StoredMessage {
                id: 40,
                from: "baraka".to_owned(),
                from_name: "Baraka Mushi".to_owned(),
                text: "Karibu!".to_owned(),
                client_id: String::new(),
            }],
        }],
        calls: Vec::new(),
        phones: Vec::new(),
        next_id: 41,
        log: Vec::new(),
        fail: None,
        hold: None,
    }))
}

pub fn lock(world: &Shared) -> MutexGuard<'_, World> {
    world.lock().expect("the world")
}

/// The app with `world` behind it.
pub fn serve(app: BaruaTalk, world: &Shared) -> Ui<BaruaTalk> {
    let world = Arc::clone(world);
    Ui::with_services(app, move |request| match request {
        ServiceRequest::Http(request) => lock(&world).answer(request).map(ServiceResult::Http),
        _ => None,
    })
}

fn quoted(text: &str) -> String {
    json::string(text)
}

fn ok(status: u16, body: String) -> Result<HttpResponse, HttpError> {
    Ok(HttpResponse::new(status)
        .with_header("Content-Type", "application/json")
        .with_body(body.into_bytes()))
}

fn error(status: u16, code: &str, message: &str) -> Result<HttpResponse, HttpError> {
    ok(
        status,
        format!(
            r#"{{"error": {{"code": {}, "message": {}}}}}"#,
            quoted(code),
            quoted(message)
        ),
    )
}

impl World {
    /// How many requests for a path containing `needle` were made.
    pub fn asked(&self, needle: &str) -> usize {
        self.log.iter().filter(|line| line.contains(needle)).count()
    }

    /// Someone else writes in a room.
    pub fn say(&mut self, room: &str, from: &str, from_name: &str, text: &str) {
        let id = self.next_id;
        self.next_id += 1;
        let room = self.room(room).expect("the room");
        room.messages.push(StoredMessage {
            id,
            from: from.to_owned(),
            from_name: from_name.to_owned(),
            text: text.to_owned(),
            client_id: String::new(),
        });
    }

    /// Someone rings the app's user.
    pub fn ring(&mut self, id: &str, identity: &str, name: &str, video: bool) {
        self.calls.push(StoredCall {
            id: id.to_owned(),
            direction: "incoming",
            status: "ringing",
            video,
            peer: (identity.to_owned(), name.to_owned()),
            seconds: 0,
            recording: None,
        });
    }

    pub fn room(&mut self, name: &str) -> Option<&mut StoredRoom> {
        self.rooms.iter_mut().find(|room| room.name == name)
    }

    pub fn call(&mut self, id: &str) -> &mut StoredCall {
        self.calls
            .iter_mut()
            .find(|call| call.id == id)
            .expect("the call")
    }

    fn me(&self) -> (String, String) {
        self.me
            .clone()
            .unwrap_or_else(|| ("guest-000000".to_owned(), "guest-000000".to_owned()))
    }

    fn room_json(room: &StoredRoom) -> String {
        let recording = match &room.recording {
            Some(id) => format!(r#"{{"id": {}, "status": "recording"}}"#, quoted(id)),
            None => "null".to_owned(),
        };
        format!(
            r#"{{"name": {}, "display_name": {}, "mine": false, "member": true, "members": {},
                "unread": 0, "last_message": null, "active_at": "2026-11-15T07:00:30Z",
                "live": {}, "recording": {recording}, "created_at": "2026-11-15T07:00:00Z"}}"#,
            quoted(&room.name),
            quoted(&room.display_name),
            room.members,
            room.live
        )
    }

    fn message_json(&self, message: &StoredMessage) -> String {
        format!(
            r#"{{"id": {}, "from": {}, "from_name": {}, "mine": {}, "text": {},
                "created_at": "2026-11-15T07:01:00Z", "delivered": true}}"#,
            message.id,
            quoted(&message.from),
            quoted(&message.from_name),
            message.from == self.me().0,
            quoted(&message.text)
        )
    }

    fn call_json(call: &StoredCall) -> String {
        let recording = match &call.recording {
            Some(id) => format!(r#"{{"id": {}, "status": "recording"}}"#, quoted(id)),
            None => "null".to_owned(),
        };
        format!(
            r#"{{"id": {}, "direction": "{}", "video": {}, "status": "{}",
                "peer": {{"identity": {}, "name": {}}}, "created_at": "2026-11-15T07:00:50Z",
                "answered_at": null, "ended_at": null, "seconds": {}, "recording": {recording}}}"#,
            quoted(&call.id),
            call.direction,
            call.video,
            call.status,
            quoted(&call.peer.0),
            quoted(&call.peer.1),
            call.seconds
        )
    }

    fn phone_json(&self, phone: &StoredPhone) -> String {
        let price = phone
            .price
            .map_or("null".to_owned(), |price| price.to_string());
        format!(
            r#"{{"id": {}, "to": {}, "caller_id": "+255800110011", "status": "{}",
                "created_at": "2026-11-15T07:00:50Z", "started_at": null, "ended_at": null,
                "seconds": {}, "price_tzs": {price},
                "rate": {{"tzs": 300, "per": "minute", "text": "TSh 300 / minute"}},
                "can_hang_up": {}}}"#,
            quoted(&phone.id),
            quoted(&phone.to),
            phone.status,
            phone.seconds,
            self.can_hang_up
        )
    }

    fn activity_json(&self) -> String {
        let mut items = Vec::new();
        for call in self.calls.iter().rev() {
            if call.status == "ringing" || call.status == "active" {
                continue;
            }
            items.push(format!(
                r#"{{"kind": "call", "id": {}, "title": {}, "preview": "{}",
                    "at": "2026-11-15T07:00:50Z", "unread": 0, "live": false, "status": "{}",
                    "direction": "{}", "video": {}, "seconds": {}, "peer": {{"identity": {}, "name": {}}}}}"#,
                quoted(&call.id),
                quoted(&call.peer.1),
                if call.status == "missed" { "Missed video call" } else { "Video call" },
                call.status,
                call.direction,
                call.video,
                call.seconds,
                quoted(&call.peer.0),
                quoted(&call.peer.1)
            ));
        }
        for room in &self.rooms {
            let (preview, unread) = match room.messages.last() {
                Some(last) => (format!("{}: {}", last.from_name, last.text), 1),
                None => (String::new(), 0),
            };
            items.push(format!(
                r#"{{"kind": "room", "id": {}, "title": {}, "preview": {}, "at": "2026-11-15T07:00:30Z",
                    "unread": {unread}, "live": {}, "status": "open", "direction": null, "video": null,
                    "seconds": null, "peer": null}}"#,
                quoted(&room.name),
                quoted(&room.display_name),
                quoted(&preview),
                room.live
            ));
        }
        for phone in self.phones.iter().rev() {
            if phone.status == "ringing" || phone.status == "active" {
                continue;
            }
            items.push(format!(
                r#"{{"kind": "phone", "id": {}, "title": {}, "preview": "Phone call, 1 min 15 s",
                    "at": "2026-11-14T06:10:00Z", "unread": 0, "live": false, "status": "{}",
                    "direction": "outgoing", "video": false, "seconds": {}, "peer": null}}"#,
                quoted(&phone.id),
                quoted(&phone.to),
                phone.status,
                phone.seconds
            ));
        }
        format!(r#"{{"items": [{}], "now": "{NOW}"}}"#, items.join(","))
    }

    /// Answers one request as the contract says, or leaves it pending.
    fn answer(&mut self, request: &HttpRequest) -> Option<Result<HttpResponse, HttpError>> {
        let method = match request.method {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            _ => "OTHER",
        };
        let path = request
            .url
            .split_once("/v1/")
            .map(|(_, path)| format!("/v1/{path}"))
            .unwrap_or_default();
        self.log.push(format!("{method} {path}"));
        if self.hold.is_some_and(|needle| path.contains(needle)) {
            return None;
        }
        if self.down {
            return Some(Err(HttpError::Offline));
        }
        if let Some((needle, status, code, message)) = self.fail
            && path.contains(needle)
        {
            self.fail = None;
            return Some(error(status, code, message));
        }
        let body = std::str::from_utf8(&request.body)
            .ok()
            .and_then(|text| json::parse(text).ok());
        let field = |name: &str| {
            body.as_ref()
                .and_then(|body| body.get(name))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let flag = |name: &str| {
            body.as_ref()
                .and_then(|body| body.get(name))
                .and_then(Value::as_bool)
                .unwrap_or(false)
        };
        if path != "/v1/health" {
            assert!(
                request.header_value("X-Device-Id").is_some(),
                "{path} carries no device id"
            );
        }
        let (route, query) = path.split_once('?').unwrap_or((path.as_str(), ""));
        let parts: Vec<&str> = route.trim_start_matches("/v1/").split('/').collect();
        Some(match (method, parts.as_slice()) {
            ("GET", ["health"]) => ok(
                200,
                format!(
                    r#"{{"ok": {}, "talk": {}, "database": {}, "phone": {}}}"#,
                    self.talk && self.database,
                    self.talk,
                    self.database,
                    self.phone
                ),
            ),
            ("PUT", ["me"]) => {
                let name = field("name");
                let identity = match &self.me {
                    Some((identity, _)) => identity.clone(),
                    None => name.to_lowercase().replace(' ', "-"),
                };
                self.me = Some((identity.clone(), name.clone()));
                ok(
                    200,
                    format!(
                        r#"{{"identity": {}, "name": {}}}"#,
                        quoted(&identity),
                        quoted(&name)
                    ),
                )
            }
            ("GET", ["people"]) => {
                let rows: Vec<String> = self
                    .people
                    .iter()
                    .map(|(identity, name, online)| {
                        format!(
                            r#"{{"identity": {}, "name": {}, "seen_at": "{NOW}", "online": {online}}}"#,
                            quoted(identity),
                            quoted(name)
                        )
                    })
                    .collect();
                ok(
                    200,
                    format!(r#"{{"people": [{}], "now": "{NOW}"}}"#, rows.join(",")),
                )
            }
            ("GET", ["account"]) => ok(
                200,
                format!(
                    r#"{{"phone": {{"enabled": {}, "caller_id": "+255800110011"}},
                        "rates": {{"phone_call": {{"tzs": 300, "per": "minute", "text": "TSh 300 / minute"}},
                                   "recording": {{"tzs": 30, "per": "minute", "text": "TSh 30 / minute"}}}},
                        "balance": {{"state": "unknown", "tzs": null}}, "spent_tzs": 0}}"#,
                    self.phone
                ),
            ),
            ("GET", ["rooms"]) => {
                let rows: Vec<String> = self.rooms.iter().map(Self::room_json).collect();
                ok(
                    200,
                    format!(r#"{{"rooms": [{}], "now": "{NOW}"}}"#, rows.join(",")),
                )
            }
            ("GET", ["activity"]) => ok(200, self.activity_json()),
            ("POST", ["rooms"]) => {
                if !self.talk {
                    return Some(error(
                        503,
                        "talk_unavailable",
                        "The server has no Barua key.",
                    ));
                }
                let display_name = field("display_name");
                let name = display_name.to_lowercase().replace(' ', "-");
                let existed = self.room(&name).is_some();
                if !existed {
                    self.rooms.insert(
                        0,
                        StoredRoom {
                            name: name.clone(),
                            display_name,
                            members: 1,
                            live: true,
                            recording: None,
                            messages: Vec::new(),
                        },
                    );
                }
                let room = Self::room_json(self.room(&name).expect("the room"));
                ok(
                    if existed { 200 } else { 201 },
                    format!(r#"{{"room": {room}}}"#),
                )
            }
            ("GET", ["rooms", name, "messages"]) => {
                let after = query
                    .split('&')
                    .find_map(|pair| pair.strip_prefix("after="))
                    .and_then(|id| id.parse::<u64>().ok());
                let Some(room) = self.rooms.iter().find(|room| room.name == *name) else {
                    return Some(error(404, "not_found", "No such room."));
                };
                let rows: Vec<String> = room
                    .messages
                    .iter()
                    .filter(|message| after.is_none_or(|after| message.id > after))
                    .map(|message| self.message_json(message))
                    .collect();
                let last = room.messages.last().map_or(0, |message| message.id);
                ok(
                    200,
                    format!(
                        r#"{{"messages": [{}], "last_id": {last}, "more": false, "unread": 0, "now": "{NOW}"}}"#,
                        rows.join(",")
                    ),
                )
            }
            ("POST", ["rooms", name, "messages"]) => {
                let (identity, my_name) = self.me();
                let (text, client_id) = (field("text"), field("client_id"));
                let id = self.next_id;
                let Some(room) = self.room(name) else {
                    return Some(error(404, "not_found", "No such room."));
                };
                let known = room
                    .messages
                    .iter()
                    .find(|message| !client_id.is_empty() && message.client_id == client_id)
                    .cloned();
                let (status, message) = match known {
                    Some(message) => (200, message),
                    None => {
                        let message = StoredMessage {
                            id,
                            from: identity,
                            from_name: my_name,
                            text,
                            client_id,
                        };
                        room.messages.push(message.clone());
                        self.next_id += 1;
                        (201, message)
                    }
                };
                ok(
                    status,
                    format!(r#"{{"message": {}}}"#, self.message_json(&message)),
                )
            }
            ("POST", ["rooms", name, "ticket"]) => ok(
                200,
                format!(
                    r#"{{"token": "room-ticket", "url": "wss://talk.example", "identity": {},
                        "room": {}, "video": {}, "expires_at": "2026-11-15T08:01:12Z"}}"#,
                    quoted(&self.me().0),
                    quoted(name),
                    flag("video")
                ),
            ),
            ("POST", ["calls"]) => {
                let to = field("to");
                let Some((_, name, _)) = self.people.iter().find(|person| person.0 == to).cloned()
                else {
                    return Some(error(404, "not_found", "Nobody has that address."));
                };
                let id = format!("call-{}", self.calls.len() + 1);
                self.calls.push(StoredCall {
                    id: id.clone(),
                    direction: "outgoing",
                    status: "ringing",
                    video: flag("video"),
                    peer: (to, name),
                    seconds: 0,
                    recording: None,
                });
                ok(
                    201,
                    format!(
                        r#"{{"call": {}, "token": "caller-ticket", "url": "wss://talk.example"}}"#,
                        Self::call_json(self.call(&id))
                    ),
                )
            }
            ("GET", ["calls", "incoming"]) => {
                let rows: Vec<String> = self
                    .calls
                    .iter()
                    .filter(|call| call.direction == "incoming" && call.status == "ringing")
                    .map(Self::call_json)
                    .collect();
                ok(
                    200,
                    format!(r#"{{"calls": [{}], "now": "{NOW}"}}"#, rows.join(",")),
                )
            }
            ("GET", ["calls", id]) => match self.calls.iter().find(|call| call.id == *id) {
                Some(call) => ok(
                    200,
                    format!(r#"{{"call": {}, "now": "{NOW}"}}"#, Self::call_json(call)),
                ),
                None => error(404, "not_found", "No such call."),
            },
            ("POST", ["calls", id, action]) => {
                let Some(call) = self.calls.iter_mut().find(|call| call.id == *id) else {
                    return Some(error(404, "not_found", "No such call."));
                };
                match *action {
                    "accept" | "decline" if call.status != "ringing" => {
                        return Some(error(409, "not_ringing", "That call is no longer ringing."));
                    }
                    "accept" => call.status = "active",
                    "decline" => call.status = "declined",
                    "hangup" => {
                        call.status = match call.status {
                            "active" => "ended",
                            "ringing" => "missed",
                            other => other,
                        };
                        call.recording = None;
                    }
                    _ => return Some(error(404, "not_found", "No such action.")),
                }
                let ticket = if *action == "accept" {
                    r#", "token": "callee-ticket", "url": "wss://talk.example""#
                } else {
                    ""
                };
                ok(
                    200,
                    format!(r#"{{"call": {}{ticket}}}"#, Self::call_json(call)),
                )
            }
            ("POST", ["phone-calls"]) => {
                if !self.phone {
                    return Some(error(
                        503,
                        "phone_unavailable",
                        "Phone calls are switched off on this server.",
                    ));
                }
                let (to, request_id) = (field("to"), field("request_id"));
                assert!(request_id.len() >= 8, "a dial carries a request id");
                let known = self
                    .phones
                    .iter()
                    .find(|phone| phone.request_id == request_id)
                    .cloned();
                let (status, phone) = match known {
                    Some(phone) => (200, phone),
                    None => {
                        let phone = StoredPhone {
                            id: format!("phone-{}", self.phones.len() + 1),
                            to,
                            status: "ringing",
                            seconds: 0,
                            price: None,
                            request_id,
                        };
                        self.phones.push(phone.clone());
                        (201, phone)
                    }
                };
                ok(
                    status,
                    format!(
                        r#"{{"call": {}, "token": "phone-ticket", "url": "wss://talk.example"}}"#,
                        self.phone_json(&phone)
                    ),
                )
            }
            ("GET", ["phone-calls", id]) => {
                match self.phones.iter().find(|phone| phone.id == *id) {
                    Some(phone) => ok(
                        200,
                        format!(r#"{{"call": {}, "now": "{NOW}"}}"#, self.phone_json(phone)),
                    ),
                    None => error(404, "not_found", "No such call."),
                }
            }
            ("POST", ["phone-calls", id, "hangup"]) => {
                let can = self.can_hang_up;
                let Some(phone) = self.phones.iter_mut().find(|phone| phone.id == *id) else {
                    return Some(error(404, "not_found", "No such call."));
                };
                if can {
                    phone.status = "ended";
                }
                let phone = phone.clone();
                let tail = if can {
                    r#""hung_up": true"#.to_owned()
                } else {
                    r#""hung_up": false, "message": "The call ends when the other side hangs up.""#
                        .to_owned()
                };
                ok(
                    200,
                    format!(r#"{{"call": {}, {tail}}}"#, self.phone_json(&phone)),
                )
            }
            ("POST", ["recordings"]) => {
                let (room, call) = (field("room"), field("call"));
                let id = "rec-1".to_owned();
                if !room.is_empty() {
                    match self.room(&room) {
                        Some(room) => room.recording = Some(id.clone()),
                        None => return Some(error(404, "not_found", "No such room.")),
                    }
                } else {
                    self.call(&call).recording = Some(id.clone());
                }
                ok(
                    201,
                    format!(
                        r#"{{"recording": {{"id": {}, "status": "recording"}}}}"#,
                        quoted(&id)
                    ),
                )
            }
            ("POST", ["recordings", id, "stop"]) => {
                for room in &mut self.rooms {
                    room.recording = None;
                }
                for call in &mut self.calls {
                    call.recording = None;
                }
                ok(
                    200,
                    format!(
                        r#"{{"recording": {{"id": {}, "status": "processing"}}}}"#,
                        quoted(id)
                    ),
                )
            }
            _ => error(404, "not_found", "No such endpoint."),
        })
    }
}
