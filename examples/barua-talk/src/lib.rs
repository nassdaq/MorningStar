//! Barua Talk: rooms, chat, calls in the app and calls to phones,
//! reproduced from a design, that are real when its server is there and
//! scripted when it is not.
//!
//! The five screens of `design/` are in [`screens`], one module each,
//! beside the app's own; the shared pieces are in [`components`], the
//! tokens in [`theme`], the icons in [`icons`] and the data in [`model`].
//! This file is the application: the state, what each tap does to it, and
//! the timer that runs a call's clock and asks the server what is new.
//! `SCREENS.md` has the table of screens, how close each is to its
//! picture, and what is the app's own design.
//!
//! # With and without a server
//!
//! On start the app asks its server (`server/`, which holds the Barua
//! Talk key) how it is, says who it is (`PUT /v1/me`) and fetches the
//! rooms and the Home list. What follows is one of the states of [`Link`]:
//!
//! * [`Link::Online`]: the rooms, messages, people and calls are the
//!   server's. Home, an open room and a call are polled on the timer, and
//!   so is "is anyone ringing me?", which is what makes the Incoming
//!   screen appear.
//! * [`Link::Offline`] (the server did not answer) and
//!   [`Link::Standalone`] (nothing behind the app can make a request: a
//!   test, a screenshot): the scripted app, exactly. The rooms and people
//!   are the design's ([`sample`]), a message is answered, a call is
//!   "answered" two seconds later.
//!
//! [`api`] builds every request and decodes every answer (`server/API.md`
//! is the contract); this file hands the requests to the platform in
//! `Application::effects` and takes the answers as [`Message::Net`]. An
//! answer for a call or a check that is no longer the current one changes
//! nothing.
//!
//! # Sound, and no pictures
//!
//! A real call carries sound. The ticket the server hands out for a call
//! or a room ([`model::Call::ticket`], [`model::PhoneCall::ticket`]) is
//! what the framework's live room is joined with (`Effects::join_live`,
//! ADR 0038): an in-app call when it becomes active, a room's call when
//! its ticket arrives, a phone call as soon as it is dialled. The mute
//! and loudspeaker buttons act on that room, and the room is left when
//! the call ends, however it ends. [`Sound`] is how that stands, and the
//! call screens say it in words ([`BaruaTalk::sound_note`]): what is
//! connected, and what is not and why.
//!
//! A call with video carries pictures too (ADR 0039): the room is joined
//! with the camera as the call's button stands, the other side's camera
//! fills the screen, the user's own is in the tile (touch it for the other
//! camera), a screen someone shares takes the place of their face, and the
//! "Share your screen" button shares this phone's. [`Pictures`] is what
//! the room has said about them; the pictures themselves never pass
//! through the app. A message sent to an open room still goes to the
//! server, which keeps it; what the live room adds is the moment: data on
//! its "chat" topic asks the server for the room's messages at once. The
//! scripted app joins nothing and shows the design's photographs.

pub mod api;
pub mod components;
pub mod icons;
pub mod json;
pub mod model;
pub mod sample;
pub mod screens;
pub mod theme;

use std::time::Duration;

use morningstar::ui::prelude::*;

use api::{Client, Failure};
use model::{
    Body, Call, CallStatus, ChatMessage, Delivery, Entry, Face, Filter, Item, Loaded, Mark, Person,
    PhoneCall, PhoneStatus, Photo, Recording, Room, Subject, Target, VOLUME_STEPS,
};

/// The key the server's address is stored under on the phone.
pub const SERVER_KEY: &str = "barua-talk.server";
/// The key the user's name is stored under.
pub const NAME_KEY: &str = "barua-talk.name";
/// The key of the number that makes `client_id`s and `request_id`s never
/// repeat on this phone.
pub const SERIAL_KEY: &str = "barua-talk.serial";
/// The longest address the Server screen's field takes, in characters.
pub const URL_LIMIT: usize = 200;
/// The longest number the dial pad takes: `+` and fifteen digits.
pub const DIAL_LIMIT: usize = 16;

/// The design's artboard, in dp.
pub const DESIGN_WIDTH: f32 = 390.0;
/// The design's artboard, in dp.
pub const DESIGN_HEIGHT: f32 = 844.0;

/// The design's pages, in its order: the names of `design/html`.
pub const DESIGNED: [&str; 5] = [
    "01-home",
    "02-room",
    "03-incoming",
    "04-video-call",
    "05-phone-call",
];

/// A screen of the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Screen {
    /// Page 01: the greeting, the live rooms, the list, the tab bar.
    Home,
    /// Page 02: a room's chat.
    Room,
    /// Page 03: a call that rings, in either direction.
    Incoming,
    /// Page 04: a call in the app, answered.
    VideoCall,
    /// Page 05: a call to a phone.
    PhoneCall,
    /// The app's own: the first run's question, "what is your name?".
    Welcome,
    /// The app's own: the dial pad.
    Dial,
    /// The app's own: naming a new room.
    NewRoom,
    /// The app's own: the people who can be called.
    People,
    /// The app's own: unread rooms and missed calls.
    Notifications,
    /// The app's own: the user's name and the server's address.
    Server,
}

impl Screen {
    /// Every screen: the design's five, then the app's own.
    pub const ALL: [Self; 11] = [
        Self::Home,
        Self::Room,
        Self::Incoming,
        Self::VideoCall,
        Self::PhoneCall,
        Self::Welcome,
        Self::Dial,
        Self::NewRoom,
        Self::People,
        Self::Notifications,
        Self::Server,
    ];

    /// The screen's name: the design's file name for its pages.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Home => "01-home",
            Self::Room => "02-room",
            Self::Incoming => "03-incoming",
            Self::VideoCall => "04-video-call",
            Self::PhoneCall => "05-phone-call",
            Self::Welcome => "welcome",
            Self::Dial => "dial",
            Self::NewRoom => "new-room",
            Self::People => "people",
            Self::Notifications => "notifications",
            Self::Server => "server",
        }
    }

    /// The id of the screen's scrolling part.
    pub const fn scroll_id(self) -> &'static str {
        match self {
            Self::Home => "scroll-home",
            Self::Room => "scroll-room",
            Self::Incoming => "scroll-incoming",
            Self::VideoCall => "scroll-video-call",
            Self::PhoneCall => "scroll-phone-call",
            Self::Welcome => "scroll-welcome",
            Self::Dial => "scroll-dial",
            Self::NewRoom => "scroll-new-room",
            Self::People => "scroll-people",
            Self::Notifications => "scroll-notifications",
            Self::Server => "scroll-server",
        }
    }

    /// The tab this screen is the root of.
    pub const fn tab(self) -> Option<Tab> {
        match self {
            Self::Home => Some(Tab::Home),
            Self::Notifications => Some(Tab::Notifications),
            Self::People => Some(Tab::Contacts),
            _ => None,
        }
    }

    /// True for the three screens of a call.
    pub const fn is_call(self) -> bool {
        matches!(self, Self::Incoming | Self::VideoCall | Self::PhoneCall)
    }
}

/// A tab of the bottom bar that is a screen. (Its "Calls" button shows
/// Home's calls and its middle button opens a sheet.)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tab {
    /// Home.
    Home,
    /// Notifications.
    Notifications,
    /// Contacts: the People screen.
    Contacts,
}

impl Tab {
    /// The screen at the tab's root.
    pub const fn screen(self) -> Screen {
        match self {
            Self::Home => Screen::Home,
            Self::Notifications => Screen::Notifications,
            Self::Contacts => Screen::People,
        }
    }
}

/// How the app stands with its server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Link {
    /// Nothing behind the app can make a request (a test, a screenshot),
    /// or nothing has been asked yet: the scripted app, as designed.
    Standalone,
    /// The server is being asked how it is.
    Checking,
    /// The server answers: rooms, messages and calls are real.
    Online {
        /// It has a Barua key: rooms can be made and calls can ring.
        talk: bool,
        /// Phone calls look usable.
        phone: bool,
    },
    /// The server cannot be used, and why, in plain words: the scripted
    /// app.
    Offline(String),
}

impl Link {
    /// True when rooms and calls are the server's.
    pub fn is_online(&self) -> bool {
        matches!(self, Self::Online { .. })
    }

    /// The state in plain words, for the Server screen.
    pub fn status(&self) -> String {
        match self {
            Self::Standalone => "Not connected. Rooms and calls are a scripted demo.".to_owned(),
            Self::Checking => "Checking\u{2026}".to_owned(),
            Self::Online { talk: false, .. } => {
                "Connected, but the server has no Barua key: stored chat works, rooms cannot be made and nothing can ring.".to_owned()
            }
            Self::Online { phone: true, .. } => {
                "Connected. Rooms, messages and calls are real; phone calls are on.".to_owned()
            }
            Self::Online { .. } => {
                "Connected. Rooms, messages and calls are real; phone calls are off on this server.".to_owned()
            }
            Self::Offline(why) => format!("Offline. {why} Rooms and calls are a scripted demo."),
        }
    }
}

/// How often the timer ticks while something needs it.
pub const TICKS_PER_SECOND: u32 = 4;
/// How long a toast stays: three seconds.
pub const TOAST_TICKS: u32 = 3 * TICKS_PER_SECOND;
/// How long the scripted app lets a call ring before it is answered.
pub const ANSWER_TICKS: u32 = 2 * TICKS_PER_SECOND;
/// How long after a message the scripted app marks it seen.
pub const SEEN_TICKS: u32 = TICKS_PER_SECOND;
/// How long after a message the scripted app answers it.
pub const REPLY_TICKS: u32 = 2 * TICKS_PER_SECOND;
/// "Is anyone ringing me?": every 2.5 seconds, as the contract suggests.
pub const POLL_INCOMING: u32 = 10;
/// Home's rooms and list: every 5 seconds.
pub const POLL_HOME: u32 = 20;
/// An open room's new messages: every 2 seconds.
pub const POLL_ROOM: u32 = 8;
/// The people: every 5 seconds.
pub const POLL_PEOPLE: u32 = 20;
/// A call that rings: every 2 seconds.
pub const POLL_RINGING: u32 = 8;
/// A call that is answered: every 5 seconds.
pub const POLL_ACTIVE: u32 = 20;
/// A call to a phone: every 4 seconds.
pub const POLL_PHONE: u32 = 16;

/// How the sound of the call in progress stands: whether the live room its
/// ticket leads to is joined. Only a real call has any; the scripted app's
/// calls stay [`Off`](Self::Off) and say nothing about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sound {
    /// No room is joined: there is no call, it is not answered yet, or its
    /// ticket has not come.
    Off,
    /// The room is being joined: the microphone may be being asked for.
    Joining,
    /// Sound flows both ways (one way while the microphone is muted).
    On,
    /// The connection dropped and is being made again.
    Reconnecting,
    /// The app left the screen and the platform left the room with it; it
    /// is joined again when the app is back.
    Away,
    /// There is no sound, and this sentence says why.
    Failed(&'static str),
}

impl Sound {
    /// A room is joined or being joined: there is something to leave.
    fn in_room(&self) -> bool {
        matches!(self, Self::Joining | Self::On | Self::Reconnecting)
    }
}

/// What the call screens say about the sound of a real call.
pub mod sound {
    /// The call is connected and its ticket has not come yet.
    pub const WAITING: &str = "Connected. Waiting for the sound's ticket\u{2026}";
    /// The server sent no ticket with the call.
    pub const NO_TICKET: &str = "Connected without sound: the server sent no ticket.";
    /// The room is being joined.
    pub const JOINING: &str = "Connecting sound\u{2026}";
    /// Sound flows and nobody else has joined yet.
    pub const ALONE: &str = "Sound is live. Waiting for the other side.";
    /// Sound flows.
    pub const LIVE: &str = "Sound is live.";
    /// Sound flows on a call with video, and the other side's picture is
    /// there.
    pub const LIVE_VIDEO: &str = "Sound and video are live.";
    /// Sound flows on a call with video; the other side shows no picture.
    pub const LIVE_NO_PICTURE: &str = "Sound is live. The other side's camera is off.";
    /// Someone else's screen is what the call shows.
    pub const THEIR_SCREEN: &str = "Sound is live. You are seeing a shared screen.";
    /// This phone's screen is shared.
    pub const SHARING: &str = "You are sharing your screen. Sound is live.";
    /// The connection is being made again.
    pub const RECONNECTING: &str = "Reconnecting sound\u{2026}";
    /// The app left the screen.
    pub const AWAY: &str = "Sound paused while the app was away.";
    /// The user refused the microphone.
    pub const NO_MICROPHONE: &str =
        "No sound: the microphone is not allowed. Allow it in Settings, then call again.";
    /// This build has no live rooms (iOS, or an app built without them).
    pub const UNAVAILABLE: &str = "No sound: live audio is not part of this build of the app.";
    /// The room could not be joined.
    pub const FAILED: &str = "No sound: the live connection failed. Hang up and call again.";
    /// The connection was lost for good.
    pub const LOST: &str = "The sound was lost with the connection. Hang up and call again.";
    /// The room was closed from the other side.
    pub const ENDED: &str = "The sound was ended from the other side.";
    /// Sound flows through a Bluetooth headset.
    pub const LIVE_BLUETOOTH: &str = "Sound is live on your Bluetooth headset.";
    /// Sound flows through a headset on a wire.
    pub const LIVE_WIRED: &str = "Sound is live on your headset.";
    /// The user ended the call from its notification while the app was
    /// behind another one.
    pub const HUNG_UP: &str = "You ended the call from its notification.";
    /// The second line of the notification a call goes on under.
    pub const NOTICE: &str = "Tap to return to the call.";
}

/// What the call screens say when the camera or the screen could not be
/// turned on. The call goes on without it.
pub mod pictures {
    /// The user refused the camera.
    pub const NO_CAMERA: &str = "The camera is not allowed. Allow it in Settings.";
    /// This build has no camera in calls.
    pub const CAMERA_UNAVAILABLE: &str = "This build of the app has no camera in calls.";
    /// The camera failed.
    pub const CAMERA_FAILED: &str = "The camera could not be started.";
    /// The user said no to sharing the screen.
    pub const NO_SCREEN: &str = "The screen is not shared: it was not allowed.";
    /// This build cannot share the screen.
    pub const SCREEN_UNAVAILABLE: &str = "This build of the app cannot share the screen.";
    /// Sharing failed.
    pub const SCREEN_FAILED: &str = "The screen could not be shared.";
    /// "Share your screen" in a call with no live room behind it.
    pub const SCREEN_NEEDS_SOUND: &str = "The screen is shared once the call's sound is live.";
    /// "Share your screen" in the scripted app.
    pub const SCREEN_NEEDS_SERVER: &str = "Screen sharing needs a call through the server.";
}

/// The videos of the call in progress, as its live room has announced
/// them (ADR 0039): which track each is drawn by. The pictures themselves
/// go from the room to the screen without passing through the app, so
/// "there is a track" is all it knows: until a track's first frame the
/// screen shows what is underneath.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pictures {
    /// The other side's camera: the one that started last, in a room with
    /// several.
    pub theirs: Option<VideoTrack>,
    /// A screen someone else shares.
    pub screen: Option<VideoTrack>,
    /// This phone's own camera.
    pub own: Option<VideoTrack>,
    /// This phone's screen is shared.
    pub sharing: bool,
}

/// What the platform answers a request with.
pub type Wire = Result<HttpResponse, HttpError>;

/// An answer from the server. Those about a check carry its number, those
/// about a call the call's.
#[derive(Clone, Debug, PartialEq)]
pub enum Net {
    /// `GET /v1/health` of check `.0`.
    Health(u32, Wire),
    /// `PUT /v1/me` of check `.0`.
    Me(u32, Wire),
    /// `GET /v1/rooms`, of check `.0`.
    Rooms(u32, Wire),
    /// `GET /v1/activity`, of check `.0`.
    Activity(u32, Wire),
    /// `GET /v1/account`.
    Account(Wire),
    /// `PUT /v1/me` after a change of name.
    Renamed(Wire),
    /// `GET /v1/people`.
    People(Wire),
    /// `GET /v1/rooms/{name}/messages` for room `.0`.
    Messages(String, Wire),
    /// `POST /v1/rooms/{name}/messages` for room `.0`, the message the
    /// app numbered `.1`.
    Posted(String, u32, Wire),
    /// `POST /v1/rooms`.
    RoomMade(Wire),
    /// `GET /v1/calls/incoming`.
    Incoming(Wire),
    /// `POST /v1/calls` for call `.0`.
    Rung(u32, Wire),
    /// `GET /v1/calls/{id}` for call `.0`.
    CallPolled(u32, Wire),
    /// `POST /v1/calls/{id}/accept` for call `.0`.
    Accepted(u32, Wire),
    /// `POST /v1/rooms/{name}/ticket` for call `.0`.
    RoomTicket(u32, Wire),
    /// `POST /v1/recordings` for call `.0`.
    RecordingStarted(u32, Wire),
    /// Stopping the recording of call `.0`.
    RecordingStopped(u32, Wire),
    /// `POST /v1/phone-calls` for phone call `.0`.
    Dialled(u32, Wire),
    /// `GET /v1/phone-calls/{id}` for phone call `.0`.
    PhonePolled(u32, Wire),
    /// Hanging up a phone call: the server may have a sentence about it.
    PhoneLeft(Wire),
    /// An answer nothing waits for: a decline, a hang-up.
    Done(Wire),
}

/// What a tap, a key or the timer asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// The back arrow of a pushed screen.
    Back,
    /// The timer.
    Tick,
    /// An answer from the server.
    Net(Net),
    /// Something happened in the live room joined for the call with this
    /// serial: its sound.
    Live(u32, LiveEvent),

    /// Typing in a name field (Welcome, Server).
    Name(String),
    /// Welcome's button: keep the name and go in.
    Continue,
    /// "Save name" on the Server screen.
    SaveName,

    /// A tab of the bottom bar.
    Show(Tab),
    /// The bar's "Calls": Home, with its calls.
    ShowCalls,
    /// One of Home's three tabs.
    SetFilter(Filter),
    /// The magnifier: opens and closes Home's search.
    ToggleSearch,
    /// Typing in Home's search.
    Query(String),
    /// A row of the Home list, by its place in [`BaruaTalk::items`].
    OpenItem(usize),
    /// A ring of "Live rooms", by the room's address.
    OpenRoom(String),
    /// "New room".
    OpenNewRoom,
    /// "Dial a phone number".
    OpenDial,
    /// The bar's middle button: the sheet of things to start.
    OpenStart,
    /// The sheet's scrim.
    CloseStart,
    /// The user's picture: the Server screen.
    OpenServer,
    /// The sheet's "Call someone".
    OpenPeople,

    /// Typing a new room's name.
    RoomName(String),
    /// "Create room".
    CreateRoom,

    /// Typing a message.
    Draft(String),
    /// The send button, or the keyboard's Send.
    Send,
    /// A message that did not get there, tapped: send it again.
    Retry(u32),
    /// The paperclip.
    Attach,
    /// The play button of the voice message at this place in the room.
    PlayVoice(usize),
    /// "Add a phone call to this room".
    RoomPhone,
    /// "Start video in this room".
    RoomVideo,

    /// Calls the person at this place in [`BaruaTalk::people`], with or
    /// without video.
    CallPerson(usize, bool),
    /// Dials the phone number of the person at this place.
    PhonePerson(usize),
    /// Opens the conversation with the person at this place.
    MessagePerson(usize),

    /// "Accept".
    Accept,
    /// "Decline", or "Cancel" on a call that the user made.
    Decline,
    /// "Reply with a message instead".
    ReplyInstead,

    /// The red button of a call.
    HangUp,
    /// The microphone of either kind of call.
    ToggleMute,
    /// The camera.
    ToggleCamera,
    /// The camera's own tile: the other camera of the phone.
    SwitchCamera,
    /// The record button.
    ToggleRecording,
    /// "Share your screen".
    ShareScreen,
    /// "Add someone to the call".
    AddPerson,
    /// The upper half of the volume slider.
    VolumeUp,
    /// Its lower half.
    VolumeDown,
    /// The loudspeaker of a phone call.
    ToggleSpeaker,
    /// "Open keypad", and the keypad's close.
    ToggleKeypad,
    /// A key of the keypad during a phone call.
    Key(char),

    /// A key of the dial pad.
    DialKey(char),
    /// The dial pad's delete.
    DialBackspace,
    /// The dial pad's green button.
    DialCall,

    /// Typing the server's address.
    ServerUrl(String),
    /// "Check connection".
    CheckServer,
    /// "Use the default address".
    DefaultServer,
}

/// What `update` has decided the platform should do; `effects` hands it
/// over.
enum Ask {
    Scroll(ScrollTo),
    Announce(String),
    /// A request, and the message its answer becomes.
    Http(HttpRequest, Route),
    Store(&'static str, String),
    /// Join the live room of the call with this serial.
    Join(u32, LiveJoin),
    /// Leave the live room.
    Leave,
    /// Open or mute the microphone in it.
    Microphone(bool),
    /// Its sound to the loudspeaker or the earpiece.
    Speaker(bool),
    /// Its sound to this route.
    Route(LiveAudioRoute),
    /// This phone's camera on or off in it.
    Camera(bool),
    /// The phone's other camera.
    SwitchCamera,
    /// Share this phone's screen with it, or stop.
    ShareScreen(bool),
}

/// Which [`Net`] message an HTTP answer becomes.
enum Route {
    Health(u32),
    Me(u32),
    Rooms(u32),
    Activity(u32),
    Account,
    Renamed,
    People,
    Messages(String),
    Posted(String, u32),
    RoomMade,
    Incoming,
    Rung(u32),
    CallPolled(u32),
    Accepted(u32),
    RoomTicket(u32),
    RecordingStarted(u32),
    RecordingStopped(u32),
    Dialled(u32),
    PhonePolled(u32),
    PhoneLeft,
    Done,
}

impl Route {
    fn message(self, wire: Wire) -> Message {
        Message::Net(match self {
            Self::Health(sync) => Net::Health(sync, wire),
            Self::Me(sync) => Net::Me(sync, wire),
            Self::Rooms(sync) => Net::Rooms(sync, wire),
            Self::Activity(sync) => Net::Activity(sync, wire),
            Self::Account => Net::Account(wire),
            Self::Renamed => Net::Renamed(wire),
            Self::People => Net::People(wire),
            Self::Messages(room) => Net::Messages(room, wire),
            Self::Posted(room, local) => Net::Posted(room, local, wire),
            Self::RoomMade => Net::RoomMade(wire),
            Self::Incoming => Net::Incoming(wire),
            Self::Rung(serial) => Net::Rung(serial, wire),
            Self::CallPolled(serial) => Net::CallPolled(serial, wire),
            Self::Accepted(serial) => Net::Accepted(serial, wire),
            Self::RoomTicket(serial) => Net::RoomTicket(serial, wire),
            Self::RecordingStarted(serial) => Net::RecordingStarted(serial, wire),
            Self::RecordingStopped(serial) => Net::RecordingStopped(serial, wire),
            Self::Dialled(serial) => Net::Dialled(serial, wire),
            Self::PhonePolled(serial) => Net::PhonePolled(serial, wire),
            Self::PhoneLeft => Net::PhoneLeft(wire),
            Self::Done => Net::Done(wire),
        })
    }
}

/// A question asked of the server again and again: when it is next due,
/// and whether its last asking is still unanswered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Poll {
    due: u32,
    flying: bool,
}

impl Poll {
    /// True when it is time to ask and nothing is in the air.
    fn ready(&self, clock: u32) -> bool {
        !self.flying && clock >= self.due
    }

    fn sent(&mut self, clock: u32, every: u32) {
        self.flying = true;
        self.due = clock + every;
    }
}

/// Something the scripted app does a moment later.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Script {
    /// The message numbered `.1` of room `.0` is seen.
    Seen(String, u32),
    /// Someone answers in room `.0`.
    Reply(String),
}

/// The voice message that is playing: its room, its place there, and the
/// ticks it has played for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Playing {
    /// The room's address.
    pub room: String,
    /// The message's place in the room.
    pub index: usize,
    /// Timer ticks played.
    pub ticks: u32,
}

/// The application.
pub struct BaruaTalk {
    nav: Navigator<Screen>,
    /// The user's name; empty until the first run's question is answered.
    name: String,
    /// What a name field holds.
    name_draft: String,
    name_problem: Option<&'static str>,
    /// Who the user is to the server.
    identity: Option<String>,
    /// The photographs, registered in `setup`, in [`Photo::ALL`]'s order.
    images: Vec<Option<Image>>,

    rooms: Vec<Room>,
    /// How many of `rooms`, from the first, have a ring on Home.
    rings: usize,
    items: Vec<Item>,
    people: Vec<Person>,
    filter: Filter,
    searching: bool,
    query: String,
    /// The sheet of things to start is open.
    start: bool,

    /// The address of the room the Room screen shows.
    room: Option<String>,
    draft: String,
    playing: Option<Playing>,
    new_room: String,
    new_room_problem: Option<String>,
    /// A room is being made on the server.
    making_room: bool,

    call: Option<Call>,
    phone: Option<PhoneCall>,
    dial: String,
    dial_problem: Option<String>,
    /// Ids of calls that are over on this phone: a late poll that still
    /// lists one does not ring again.
    dismissed: Vec<String>,

    notice: Option<(String, u32)>,
    /// Ticks since the app started.
    clock: u32,
    script: Vec<(u32, Script)>,
    /// A still frame: nothing moves until something is touched. The
    /// design's pages are opened like this, so a screenshot is the
    /// design's picture and not a moment of a running clock.
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
    /// What the check has learnt so far.
    sync_health: Option<api::Health>,
    sync_rooms: Option<Vec<api::RoomRow>>,
    /// Numbers calls, so that a late answer is told from a current one.
    serials: u32,
    /// Numbers messages and dials for good: kept on the phone.
    lasting: u32,
    /// The number the other phone sees and the two prices.
    account: Option<api::Account>,
    poll_incoming: Poll,
    poll_home: Poll,
    poll_room: Poll,
    poll_people: Poll,
    poll_call: Poll,

    // -------------------------------------------------------- the sound
    /// How the sound of the call in progress stands.
    sound: Sound,
    /// The serial of the call whose live room the app has asked for:
    /// joined once per call, and left when another call or none is the
    /// one in progress.
    sound_for: Option<u32>,
    /// How many others are in the live room.
    sound_others: usize,
    /// The videos the live room has announced.
    pictures: Pictures,
    /// Where the live room's sound can go and where it goes, as the
    /// platform last said; empty until it has.
    routes: LiveAudioRoutes,
}

impl Default for BaruaTalk {
    /// The app as it is installed: nobody has said their name yet.
    fn default() -> Self {
        Self::on(Screen::Welcome)
    }
}

impl BaruaTalk {
    /// The app opened directly on `screen`, as the design's Neema would
    /// find it ([`Screen::Welcome`]: as nobody yet).
    pub fn on(screen: Screen) -> Self {
        let people = sample::people();
        let baraka = people[0].clone();
        let mut app = Self {
            nav: Navigator::new(screen),
            name: if screen == Screen::Welcome {
                String::new()
            } else {
                sample::ME.to_owned()
            },
            name_draft: String::new(),
            name_problem: None,
            identity: None,
            images: Vec::new(),
            rooms: sample::rooms(),
            rings: sample::LIVE_ROOMS,
            items: sample::items(),
            people,
            filter: Filter::All,
            searching: false,
            query: String::new(),
            start: false,
            room: None,
            draft: String::new(),
            playing: None,
            new_room: String::new(),
            new_room_problem: None,
            making_room: false,
            call: None,
            phone: None,
            dial: String::new(),
            dial_problem: None,
            dismissed: Vec::new(),
            notice: None,
            clock: 0,
            script: Vec::new(),
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
            sync_health: None,
            sync_rooms: None,
            serials: 0,
            lasting: 0,
            account: None,
            poll_incoming: Poll::default(),
            poll_home: Poll::default(),
            poll_room: Poll::default(),
            poll_people: Poll::default(),
            poll_call: Poll::default(),
            sound: Sound::Off,
            sound_for: None,
            sound_others: 0,
            pictures: Pictures::default(),
            routes: LiveAudioRoutes::default(),
        };
        app.name_draft.clone_from(&app.name);
        match screen {
            Screen::Room => app.room = Some("support-desk".to_owned()),
            Screen::Incoming => {
                app.call = Some(app.new_call(Subject::Person(baraka), true, true, true));
            }
            Screen::VideoCall => {
                // Page 04: twelve minutes in, recording.
                let mut call = app.new_call(Subject::Person(baraka), true, true, true);
                call.status = CallStatus::Active;
                call.seconds = 12 * 60 + 25;
                call.recording = Recording::On(None);
                app.call = Some(call);
            }
            Screen::PhoneCall => {
                // Page 05: two minutes in.
                let mut phone = app.new_phone(sample::HAMISI_NUMBER, true);
                phone.status = PhoneStatus::Active;
                phone.seconds = 2 * 60 + 14;
                app.phone = Some(phone);
            }
            _ => {}
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
        app.still = true;
        Some(app)
    }

    /// Puts `prefix` before the installation's id: a test against a real
    /// server is another person than the phone it runs beside.
    pub fn with_device_prefix(mut self, prefix: &'static str) -> Self {
        self.device_prefix = prefix;
        self
    }

    // ------------------------------------------------------------ reading

    /// The screen on top.
    pub fn screen(&self) -> Screen {
        *self.nav.current()
    }

    /// The screens, from the root to the one on top.
    pub fn stack(&self) -> &[Screen] {
        self.nav.stack()
    }

    /// How the app stands with its server.
    pub fn link(&self) -> &Link {
        &self.link
    }

    /// The user's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Who the user is to the server, once it has said.
    pub fn identity(&self) -> Option<&str> {
        self.identity.as_deref()
    }

    /// What a name field holds.
    pub fn name_draft(&self) -> &str {
        &self.name_draft
    }

    /// What is wrong with the name typed.
    pub fn name_problem(&self) -> Option<&'static str> {
        self.name_problem
    }

    /// The user's picture: the design's Neema has one, anyone else their
    /// initials.
    pub fn my_face(&self) -> Face {
        if self.name == sample::ME {
            Face::Photo(Photo::Neema)
        } else {
            Face::of(&self.name)
        }
    }

    /// The line over the user's name on Home: the design's greeting in
    /// the scripted app. The app has no clock to tell the morning by, so
    /// with a server it says "Karibu" at any hour.
    pub fn greeting(&self) -> &'static str {
        if self.link.is_online() {
            "Karibu"
        } else {
            "Habari za asubuhi"
        }
    }

    /// The registered image of `photo`.
    pub fn image(&self, photo: Photo) -> Option<Image> {
        self.images.get(photo.index()).copied().flatten()
    }

    /// Every room the app knows.
    pub fn rooms(&self) -> &[Room] {
        &self.rooms
    }

    /// The rooms with a ring on Home.
    pub fn live_rooms(&self) -> &[Room] {
        &self.rooms[..self.rings.min(self.rooms.len())]
    }

    /// The room the Room screen shows.
    pub fn room(&self) -> Option<&Room> {
        let name = self.room.as_deref()?;
        self.rooms.iter().find(|room| room.name == name)
    }

    /// The rows of the Home list.
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// The places in [`items`](Self::items) of the rows Home shows now:
    /// those of the chosen tab that match what is searched for.
    pub fn shown_items(&self) -> Vec<usize> {
        let query = self.query.trim().to_lowercase();
        self.items
            .iter()
            .enumerate()
            .filter(|(_, item)| self.filter.shows(item))
            .filter(|(_, item)| {
                !self.searching
                    || query.is_empty()
                    || item.title.to_lowercase().contains(&query)
                    || item.preview.to_lowercase().contains(&query)
            })
            .map(|(index, _)| index)
            .collect()
    }

    /// The places of the rows the Notifications screen lists: unread
    /// rooms and missed calls.
    pub fn notifications(&self) -> Vec<usize> {
        self.items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.unread > 0 || item.missed)
            .map(|(index, _)| index)
            .collect()
    }

    /// Unread messages in all rooms: the number beside "All".
    pub fn unread(&self) -> u32 {
        self.items.iter().map(|item| item.unread).sum()
    }

    /// Home's tab.
    pub fn filter(&self) -> Filter {
        self.filter
    }

    /// Home's search is open, and what is typed in it.
    pub fn search(&self) -> Option<&str> {
        self.searching.then_some(self.query.as_str())
    }

    /// The sheet of things to start is open.
    pub fn start_open(&self) -> bool {
        self.start
    }

    /// The message being typed.
    pub fn draft(&self) -> &str {
        &self.draft
    }

    /// The voice message that is playing.
    pub fn playing(&self) -> Option<&Playing> {
        self.playing.as_ref()
    }

    /// The name typed for a new room, and what is wrong with it.
    pub fn new_room(&self) -> (&str, Option<&str>) {
        (&self.new_room, self.new_room_problem.as_deref())
    }

    /// A room is being made on the server.
    pub fn making_room(&self) -> bool {
        self.making_room
    }

    /// The people who can be called.
    pub fn people(&self) -> &[Person] {
        &self.people
    }

    /// The call in the app that is ringing or answered.
    pub fn call(&self) -> Option<&Call> {
        self.call.as_ref()
    }

    /// How the sound of the call in progress stands.
    pub fn sound(&self) -> &Sound {
        &self.sound
    }

    /// The videos of the call in progress.
    pub fn pictures(&self) -> &Pictures {
        &self.pictures
    }

    /// What a call screen says about the sound of the call in progress,
    /// in words: `None` for the scripted app's calls (and when there is no
    /// call), which have none and show the design's picture.
    pub fn sound_note(&self) -> Option<&'static str> {
        let (scripted, video, answered, ticket) = match (&self.phone, &self.call) {
            (Some(phone), _) => (phone.scripted, false, true, phone.ticket.is_some()),
            (None, Some(call)) => (
                call.scripted,
                call.video,
                call.status == CallStatus::Active,
                call.ticket.is_some(),
            ),
            (None, None) => return None,
        };
        if scripted {
            return None;
        }
        Some(match &self.sound {
            Sound::Off if !answered => return None,
            // A room's ticket is asked for when its call opens; a call
            // between two people brings its ticket with it or has none.
            Sound::Off if ticket => sound::JOINING,
            Sound::Off => match self.call.as_ref().map(|call| &call.subject) {
                Some(Subject::Room { .. }) => sound::WAITING,
                _ if self.phone.as_ref().is_some_and(|phone| phone.id.is_none()) => sound::WAITING,
                _ => sound::NO_TICKET,
            },
            Sound::Joining => sound::JOINING,
            Sound::On if self.pictures.sharing => sound::SHARING,
            Sound::On if self.sound_others == 0 => sound::ALONE,
            Sound::On if self.pictures.screen.is_some() => sound::THEIR_SCREEN,
            Sound::On if self.pictures.theirs.is_some() => sound::LIVE_VIDEO,
            Sound::On if video => sound::LIVE_NO_PICTURE,
            Sound::On => match self.routes.selected {
                Some(LiveAudioRoute::Bluetooth) => sound::LIVE_BLUETOOTH,
                Some(LiveAudioRoute::Wired) => sound::LIVE_WIRED,
                _ => sound::LIVE,
            },
            Sound::Reconnecting => sound::RECONNECTING,
            Sound::Away => sound::AWAY,
            Sound::Failed(sentence) => sentence,
        })
    }

    /// The call to a phone.
    pub fn phone(&self) -> Option<&PhoneCall> {
        self.phone.as_ref()
    }

    /// The number on the dial pad, and what is wrong with it.
    pub fn dial(&self) -> (&str, Option<&str>) {
        (&self.dial, self.dial_problem.as_deref())
    }

    /// What a recording costs, as the pill of a call says it.
    pub fn recording_rate(&self) -> String {
        match &self.account {
            Some(account) if self.link.is_online() => account.recording_rate.clone(),
            _ => sample::RECORDING_RATE.to_owned(),
        }
    }

    /// What a phone call costs.
    pub fn phone_rate(&self) -> String {
        match &self.account {
            Some(account) if self.link.is_online() => account.phone_rate.clone(),
            _ => sample::PHONE_RATE.to_owned(),
        }
    }

    /// The toast, if one is showing.
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_ref().map(|(text, _)| text.as_str())
    }

    /// The installation's id, as sent to the server.
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// The server's address in use.
    pub fn server_url(&self) -> &str {
        &self.server_url
    }

    /// What the Server screen's field holds.
    pub fn server_draft(&self) -> &str {
        &self.server_draft
    }

    /// What is wrong with the address, or with checking.
    pub fn server_problem(&self) -> Option<&str> {
        self.server_problem.as_deref()
    }

    /// Ticks since the app started.
    pub fn clock(&self) -> u32 {
        self.clock
    }

    // ------------------------------------------------------------ helpers

    fn toast(&mut self, text: impl Into<String>) {
        let text = text.into();
        self.asks.push(Ask::Announce(text.clone()));
        self.notice = Some((text, self.clock));
    }

    fn next_serial(&mut self) -> u32 {
        self.serials += 1;
        self.serials
    }

    /// A number that is never given twice on this phone.
    fn next_lasting(&mut self) -> u32 {
        self.lasting += 1;
        self.asks
            .push(Ask::Store(SERIAL_KEY, self.lasting.to_string()));
        self.lasting
    }

    fn ready(&self) -> bool {
        !self.device_id.is_empty()
    }

    fn client(&self) -> Client {
        Client::new(self.server_url.clone(), self.device_id.clone())
    }

    fn http(&mut self, request: HttpRequest, route: Route) {
        self.asks.push(Ask::Http(request, route));
    }

    fn room_mut(&mut self, name: &str) -> Option<&mut Room> {
        self.rooms.iter_mut().find(|room| room.name == name)
    }

    /// The place in the list of the row that stands for room `name`.
    fn item_of_room(&self, name: &str) -> Option<usize> {
        self.items.iter().position(|item| match &item.target {
            Target::Room(room) | Target::Ring(room) => room == name,
            _ => false,
        })
    }

    fn person(&self, identity: &str) -> Option<&Person> {
        self.people
            .iter()
            .find(|person| person.identity == identity)
    }

    fn new_call(&mut self, subject: Subject, incoming: bool, video: bool, scripted: bool) -> Call {
        Call {
            serial: self.next_serial(),
            subject,
            incoming,
            video,
            status: CallStatus::Ringing,
            id: None,
            ticket: None,
            seconds: 0,
            ticks: 0,
            muted: false,
            camera: true,
            volume: 3,
            recording: Recording::Off,
            scripted,
        }
    }

    fn new_phone(&mut self, number: &str, scripted: bool) -> PhoneCall {
        let known = self
            .people
            .iter()
            .find(|person| person.phone.as_deref() == Some(number));
        let (name, face) = match known {
            Some(person) => (Some(person.name.clone()), Some(person.face.clone())),
            None => (None, None),
        };
        PhoneCall {
            serial: self.next_serial(),
            number: number.to_owned(),
            name,
            face,
            status: PhoneStatus::Ringing,
            id: None,
            ticket: None,
            seconds: 0,
            ticks: 0,
            caller_id: match &self.account {
                Some(account) if !scripted => account
                    .caller_id
                    .as_deref()
                    .map(model::pretty_number)
                    .unwrap_or_default(),
                _ => sample::CALLER_ID.to_owned(),
            },
            rate: self.phone_rate(),
            can_hang_up: scripted,
            muted: false,
            speaker: false,
            keypad: false,
            keys: String::new(),
            scripted,
        }
    }

    /// The scripted app's world again: what an app that has lost its
    /// server falls back to.
    fn load_sample(&mut self) {
        self.rooms = sample::rooms();
        self.rings = sample::LIVE_ROOMS;
        self.items = sample::items();
        self.people = sample::people();
        self.identity = None;
        self.account = None;
    }

    /// The screens that show something of the other world leave the
    /// stack: a room, a call. (The dial pad and the new room's form hold
    /// only what was typed, and stay.)
    fn drop_live_screens(&mut self) {
        self.call = None;
        self.phone = None;
        self.room = None;
        self.playing = None;
        self.script.clear();
        self.making_room = false;
        self.start = false;
        let stack: Vec<Screen> = self
            .nav
            .stack()
            .iter()
            .copied()
            .filter(|screen| {
                matches!(
                    screen,
                    Screen::Home
                        | Screen::People
                        | Screen::Notifications
                        | Screen::Server
                        | Screen::Dial
                        | Screen::NewRoom
                )
            })
            .collect();
        if stack.is_empty() {
            self.nav.reset(Screen::Home);
        } else if stack.len() != self.nav.stack().len() {
            self.nav.set(stack);
        }
    }

    // --------------------------------------------------------- the server

    /// Asks the server how it is; who the user is, the rooms and the list
    /// follow if it answers. Answers to earlier checks are ignored from
    /// here on.
    fn check_server(&mut self) {
        if !self.ready() || self.name.is_empty() {
            return;
        }
        self.sync += 1;
        self.sync_health = None;
        self.sync_rooms = None;
        if !self.link.is_online() {
            self.link = Link::Checking;
        }
        self.http(self.client().health(), Route::Health(self.sync));
    }

    /// The server cannot be used: the app is the scripted one.
    fn offline(&mut self, failure: &Failure) {
        let was_online = self.link.is_online();
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
        if was_online {
            self.load_sample();
            self.drop_live_screens();
        }
        self.poll_incoming = Poll::default();
        self.poll_home = Poll::default();
        self.poll_room = Poll::default();
        self.poll_people = Poll::default();
        self.poll_call = Poll::default();
    }

    /// A request of an online session failed: if the server was not
    /// reached, the session is over until it answers again.
    fn note_failure(&mut self, failure: &Failure) {
        if failure.unreachable() && self.link.is_online() {
            self.offline(failure);
        }
    }

    /// The server's rooms become the app's. A room already here keeps
    /// what is loaded of it.
    fn take_rooms(&mut self, rows: Vec<api::RoomRow>) {
        let mut old = std::mem::take(&mut self.rooms);
        for row in rows {
            let known = old.iter().position(|room| room.name == row.name);
            let mut room = match known {
                Some(index) => old.remove(index),
                None => Room {
                    name: row.name.clone(),
                    title: String::new(),
                    short: String::new(),
                    face: Face::of(&row.display_name),
                    status: String::new(),
                    live: false,
                    entries: Vec::new(),
                    loaded: Loaded::No,
                    last_id: 0,
                    recording: None,
                    replier: None,
                },
            };
            room.short = short_name(&row.display_name);
            room.face = Face::of(&row.display_name);
            room.title = row.display_name;
            room.status = match row.members {
                1 => "1 member".to_owned(),
                members => format!("{members} members"),
            };
            room.live = row.live;
            room.recording = row.recording;
            self.rooms.push(room);
        }
        self.rings = self.rooms.len();
        // Conversations between two people are not in the server's list of
        // rooms: they stay, after the rooms, and have no ring on Home.
        self.rooms
            .extend(old.into_iter().filter(|room| room.peer().is_some()));
        // The room on screen is gone: someone closed it.
        if self.room.is_some() && self.room().is_none() {
            let had_room_video = matches!(
                self.call.as_ref().map(|call| &call.subject),
                Some(Subject::Room { .. })
            );
            if had_room_video {
                self.call = None;
            }
            self.room = None;
            self.nav.pop_to(|screen| *screen == Screen::Home);
            if self.screen() != Screen::Home {
                self.nav.reset(Screen::Home);
            }
            self.toast("That room was closed.");
        }
    }

    /// The server's list becomes Home's.
    fn take_activity(&mut self, rows: Vec<api::ActivityRow>, now: &str) {
        for row in &rows {
            if row.kind == api::ActivityKind::Person {
                self.ensure_direct(&row.id, &row.title);
            }
        }
        self.items = rows
            .into_iter()
            .map(|row| {
                let when = model::ago(&row.at, now);
                match row.kind {
                    api::ActivityKind::Person => Item {
                        face: Face::of(&row.title),
                        target: Target::Room(model::direct_room(&row.id)),
                        title: row.title,
                        preview: row.preview,
                        mark: Mark::None,
                        when,
                        unread: row.unread,
                        missed: false,
                    },
                    api::ActivityKind::Room => Item {
                        face: Face::of(&row.title),
                        target: Target::Room(row.id),
                        title: row.title,
                        preview: row.preview,
                        mark: Mark::None,
                        when,
                        unread: row.unread,
                        missed: false,
                    },
                    api::ActivityKind::Call => Item {
                        face: Face::of(&row.title),
                        target: match row.peer {
                            Some(peer) => Target::Person {
                                identity: peer.identity,
                                video: row.video,
                            },
                            // A call with nobody to call back: only a row.
                            None => Target::Person {
                                identity: String::new(),
                                video: row.video,
                            },
                        },
                        title: row.title,
                        preview: row.preview,
                        mark: if row.video { Mark::Video } else { Mark::Phone },
                        when,
                        unread: 0,
                        missed: row.status == "missed",
                    },
                    api::ActivityKind::Phone => Item {
                        face: Face::of("#"),
                        title: model::pretty_number(&row.title),
                        target: Target::Phone(row.title),
                        preview: row.preview,
                        mark: Mark::Phone,
                        when,
                        unread: 0,
                        missed: false,
                    },
                }
            })
            .collect();
    }

    /// The conversation with the person `identity`, made if the app does
    /// not hold it yet: its address among the rooms.
    fn ensure_direct(&mut self, identity: &str, name: &str) -> String {
        let address = model::direct_room(identity);
        let status = match self.person(identity).map(|person| person.online) {
            Some(true) => "Online",
            Some(false) => "Offline",
            None => "Direct messages",
        }
        .to_owned();
        match self.room_mut(&address) {
            Some(room) => {
                room.title = name.to_owned();
                room.short = short_name(name);
                room.face = Face::of(name);
                room.status = status;
            }
            None => self.rooms.push(Room {
                name: address.clone(),
                title: name.to_owned(),
                short: short_name(name),
                face: Face::of(name),
                status,
                live: false,
                entries: Vec::new(),
                loaded: Loaded::No,
                last_id: 0,
                recording: None,
                replier: None,
            }),
        }
        address
    }

    /// Opens the conversation with `person`: the server's when there is
    /// one, the scripted app's room of that person otherwise.
    fn message_person(&mut self, person: &Person) {
        if self.link.is_online() {
            let address = self.ensure_direct(&person.identity, &person.name);
            self.open_room(&address);
        } else if self.room_mut(&person.identity).is_some() {
            let address = person.identity.clone();
            self.open_room(&address);
        } else {
            self.toast(format!(
                "There is no conversation with {} yet.",
                person.name
            ));
        }
    }

    /// The other person of the conversation on screen, to ring: `None` in
    /// a room.
    fn direct_peer(&self) -> Option<Person> {
        let room = self.room()?;
        let identity = room.peer()?;
        Some(self.person(identity).cloned().unwrap_or_else(|| Person {
            identity: identity.to_owned(),
            name: room.title.clone(),
            face: room.face.clone(),
            online: true,
            unread: 0,
            phone: None,
        }))
    }

    /// The server answered everything a check asks: its world is the
    /// app's.
    fn go_online(&mut self, health: api::Health) {
        let was_online = self.link.is_online();
        self.link = Link::Online {
            talk: health.talk,
            phone: health.phone,
        };
        self.checked_by_hand = false;
        if !was_online {
            // The scripted rooms, people and calls are not the server's.
            self.rooms.clear();
            self.items.clear();
            self.people.clear();
            self.dial_problem = None;
            self.drop_live_screens();
            self.poll_incoming = Poll {
                due: self.clock,
                flying: false,
            };
            self.http(self.client().account(), Route::Account);
        }
    }

    /// Home's rooms and list, asked again.
    fn refresh_home(&mut self) {
        if !self.link.is_online() || self.poll_home.flying {
            return;
        }
        self.poll_home.sent(self.clock, POLL_HOME);
        self.http(self.client().rooms(), Route::Rooms(self.sync));
    }

    fn refresh_people(&mut self) {
        if !self.link.is_online() || self.poll_people.flying {
            return;
        }
        self.poll_people.sent(self.clock, POLL_PEOPLE);
        self.http(self.client().people(), Route::People);
    }

    /// The open room's messages: all of them the first time, what is new
    /// after that.
    fn refresh_room(&mut self) {
        if !self.link.is_online() || self.poll_room.flying {
            return;
        }
        let Some(room) = self.room.clone() else {
            return;
        };
        let Some(state) = self.room_mut(&room) else {
            return;
        };
        let after = match state.loaded {
            Loaded::Yes => Some(state.last_id),
            _ => {
                state.loaded = Loaded::Loading;
                None
            }
        };
        self.poll_room.sent(self.clock, POLL_ROOM);
        self.http(self.client().messages(&room, after), Route::Messages(room));
    }

    /// Takes the address in the Server screen's field into use.
    fn use_typed_server(&mut self) -> bool {
        match api::normalize_base_url(&self.server_draft) {
            Ok(url) => {
                self.server_problem = None;
                if url != self.server_url {
                    self.server_url.clone_from(&url);
                    self.asks.push(Ask::Store(SERVER_KEY, url.clone()));
                    // Another server is another world.
                    if self.link.is_online() {
                        self.load_sample();
                        self.drop_live_screens();
                    }
                    self.link = Link::Standalone;
                }
                self.server_draft = url;
                true
            }
            Err(problem) => {
                self.server_problem = Some(problem.sentence().to_owned());
                false
            }
        }
    }

    /// Keeps the name in a name field. False, with the problem noted,
    /// when it is not one.
    fn keep_name(&mut self) -> bool {
        let name = api::tidy_name(&self.name_draft);
        if name.is_empty() {
            self.name_problem = Some("Type the name other people will see.");
            return false;
        }
        self.name_problem = None;
        self.name_draft.clone_from(&name);
        if name != self.name {
            self.name.clone_from(&name);
            self.asks.push(Ask::Store(NAME_KEY, name));
        }
        true
    }

    // -------------------------------------------------------------- rooms

    fn open_room(&mut self, name: &str) {
        if self.room_mut(name).is_none() {
            self.toast("That room is not there any more.");
            return;
        }
        self.room = Some(name.to_owned());
        self.draft.clear();
        self.playing = None;
        if let Some(index) = self.item_of_room(name) {
            self.items[index].unread = 0;
        }
        if let Some(identity) = name.strip_prefix(model::DIRECT) {
            // Opening the conversation reads it.
            for person in &mut self.people {
                if person.identity == identity {
                    person.unread = 0;
                }
            }
        }
        self.poll_room = Poll::default();
        if self.screen() == Screen::Room {
            self.refresh_room();
        } else {
            self.nav.push(Screen::Room);
        }
        self.asks
            .push(Ask::Scroll(ScrollTo::end(Screen::Room.scroll_id())));
    }

    fn show_latest(&mut self) {
        self.asks.push(Ask::Scroll(
            ScrollTo::end(Screen::Room.scroll_id()).animated(),
        ));
    }

    /// The row of room `name` says `preview`, now, at the top of the list.
    fn bump_item(&mut self, name: &str, preview: String) {
        let Some(index) = self.item_of_room(name) else {
            return;
        };
        let mut item = self.items.remove(index);
        item.preview = preview;
        item.when = "Sasa".to_owned();
        item.unread = 0;
        self.items.insert(0, item);
    }

    fn send(&mut self) {
        let text = api::tidy_message(&self.draft);
        let Some(name) = self.room.clone() else {
            return;
        };
        if text.is_empty() {
            return;
        }
        self.draft.clear();
        let online = self.link.is_online();
        let local = self.next_lasting();
        let message = ChatMessage {
            id: 0,
            local,
            author: self.name.clone(),
            face: self.my_face(),
            mine: true,
            time: "Sasa".to_owned(),
            body: Body::Text(text.clone()),
            delivery: if online {
                Delivery::Sending
            } else {
                Delivery::Sent
            },
        };
        let Some(room) = self.room_mut(&name) else {
            return;
        };
        room.entries.push(Entry::Message(message));
        let replier = room.replier.is_some();
        self.bump_item(&name, format!("You: {}", one_line(&text)));
        self.show_latest();
        if online {
            self.post(&name, local, &text);
        } else {
            self.script
                .push((self.clock + SEEN_TICKS, Script::Seen(name.clone(), local)));
            if replier {
                self.script
                    .push((self.clock + REPLY_TICKS, Script::Reply(name)));
            }
        }
    }

    fn post(&mut self, room: &str, local: u32, text: &str) {
        let client_id = api::request_id("m", &self.device_id, local);
        self.http(
            self.client().post_message(room, text, &client_id),
            Route::Posted(room.to_owned(), local),
        );
    }

    fn retry(&mut self, local: u32) {
        let Some(name) = self.room.clone() else {
            return;
        };
        let Some(room) = self.room_mut(&name) else {
            return;
        };
        let found = room.entries.iter_mut().find_map(|entry| match entry {
            Entry::Message(message)
                if message.local == local && matches!(message.delivery, Delivery::Failed(_)) =>
            {
                Some(message)
            }
            _ => None,
        });
        let Some(message) = found else {
            return;
        };
        let Body::Text(text) = message.body.clone() else {
            return;
        };
        message.delivery = Delivery::Sending;
        self.post(&name, local, &text);
    }

    /// What the scripted app does a moment after a message.
    fn play_script(&mut self, script: Script) {
        match script {
            Script::Seen(name, local) => {
                let Some(room) = self.room_mut(&name) else {
                    return;
                };
                let readers = if name == "support-desk" { 3 } else { 1 };
                for entry in &mut room.entries {
                    if let Entry::Message(message) = entry
                        && message.local == local
                        && message.mine
                    {
                        message.delivery = Delivery::Seen(readers);
                    }
                }
            }
            Script::Reply(name) => {
                let open =
                    self.room.as_deref() == Some(name.as_str()) && self.screen() == Screen::Room;
                let Some(room) = self.room_mut(&name) else {
                    return;
                };
                let Some((author, photo)) = room.replier.clone() else {
                    return;
                };
                let group = room.entries.iter().any(|entry| {
                    matches!(entry, Entry::Message(message) if !message.mine && message.author != author)
                });
                room.entries.push(Entry::Message(ChatMessage {
                    id: 0,
                    local: 0,
                    author: author.clone(),
                    face: Face::Photo(photo),
                    mine: false,
                    time: "Sasa".to_owned(),
                    body: Body::Text(sample::SCRIPTED_REPLY.to_owned()),
                    delivery: Delivery::Received,
                }));
                let first = author.split(' ').next().unwrap_or_default().to_owned();
                let preview = if group {
                    format!("{first}: {}", sample::SCRIPTED_REPLY)
                } else {
                    sample::SCRIPTED_REPLY.to_owned()
                };
                self.bump_item(&name, preview);
                if open {
                    self.show_latest();
                } else if let Some(index) = self.item_of_room(&name) {
                    self.items[index].unread = 1;
                }
            }
        }
    }

    fn create_room(&mut self) {
        let title = api::clean(&self.new_room, api::ROOM_NAME_LIMIT);
        if title.chars().count() < 2 {
            self.new_room_problem = Some("Give the room a name of two letters or more.".to_owned());
            return;
        }
        let name = slug(&title);
        if name.len() < 2 {
            self.new_room_problem =
                Some("Use some letters or digits: they make the room\u{2019}s address.".to_owned());
            return;
        }
        self.new_room_problem = None;
        if self.link.is_online() {
            if !self.making_room {
                self.making_room = true;
                self.http(self.client().create_room(&title), Route::RoomMade);
            }
            return;
        }
        // The scripted app: the room is there at once; one of that
        // address is opened rather than made twice, as the server does.
        if self.room_mut(&name).is_none() {
            self.rooms.insert(
                0,
                Room {
                    name: name.clone(),
                    short: short_name(&title),
                    face: Face::of(&title),
                    title: title.clone(),
                    status: "1 active".to_owned(),
                    live: true,
                    entries: Vec::new(),
                    loaded: Loaded::Yes,
                    last_id: 0,
                    recording: None,
                    replier: None,
                },
            );
            self.rings += 1;
            self.items.insert(
                0,
                Item {
                    target: Target::Room(name.clone()),
                    face: Face::of(&title),
                    title,
                    preview: "No messages yet".to_owned(),
                    mark: Mark::None,
                    when: "Sasa".to_owned(),
                    unread: 0,
                    missed: false,
                },
            );
        }
        self.enter_new_room(&name);
    }

    /// The new-room form gives way to the room it made.
    fn enter_new_room(&mut self, name: &str) {
        self.new_room.clear();
        if self.screen() == Screen::NewRoom {
            self.nav.pop();
        }
        self.open_room(name);
    }

    // -------------------------------------------------------------- calls

    /// Rings `person`.
    fn ring(&mut self, person: Person, video: bool) {
        if self.call.is_some() || self.phone.is_some() {
            return;
        }
        let online = self.link.is_online();
        let identity = person.identity.clone();
        let mut call = self.new_call(Subject::Person(person), false, video, !online);
        if online {
            call.status = CallStatus::Starting;
            self.http(
                self.client().ring(&identity, video),
                Route::Rung(call.serial),
            );
        }
        self.call = Some(call);
        self.nav.push(Screen::Incoming);
    }

    /// The call is answered: the ringing screen gives way to the call.
    fn activate_call(&mut self, seconds: u32) {
        let Some(call) = &mut self.call else {
            return;
        };
        call.status = CallStatus::Active;
        call.seconds = seconds;
        call.ticks = 0;
        self.poll_call = Poll {
            due: self.clock + POLL_ACTIVE,
            flying: false,
        };
        if self.screen() == Screen::Incoming {
            self.nav.replace(Screen::VideoCall);
        }
    }

    /// The call is over: its screens leave, a row about it joins the
    /// scripted list, and `note` is said.
    fn end_call(&mut self, note: Option<String>) {
        let Some(call) = self.call.take() else {
            return;
        };
        if let Some(id) = call.id {
            self.dismissed.push(id);
            if self.dismissed.len() > 16 {
                self.dismissed.remove(0);
            }
        }
        if call.scripted
            && let Subject::Person(person) = &call.subject
        {
            let answered = call.status == CallStatus::Active;
            let kind = if call.video {
                "Video call"
            } else {
                "Voice call"
            };
            self.items.insert(
                0,
                Item {
                    target: Target::Person {
                        identity: person.identity.clone(),
                        video: call.video,
                    },
                    title: person.name.clone(),
                    preview: if answered {
                        format!("{kind} \u{b7} {}", model::duration(call.seconds))
                    } else if call.incoming {
                        format!("Missed {}", kind.to_lowercase())
                    } else {
                        format!("{kind} \u{b7} no answer")
                    },
                    mark: if call.video { Mark::Video } else { Mark::Phone },
                    when: "Sasa".to_owned(),
                    unread: 0,
                    face: person.face.clone(),
                    missed: call.incoming && !answered,
                },
            );
        }
        self.leave(&[Screen::Incoming, Screen::VideoCall]);
        if let Some(note) = note {
            self.toast(note);
        }
        if self.link.is_online() {
            self.poll_home.due = self.clock;
        }
    }

    /// Takes `screens` off the top of the stack; with nothing under them
    /// (a design page opened directly), Home is.
    fn leave(&mut self, screens: &[Screen]) {
        while screens.contains(&self.screen()) {
            if self.nav.pop().is_none() {
                self.nav.reset(Screen::Home);
                break;
            }
        }
    }

    fn hang_up_call(&mut self, note: Option<String>) {
        let Some(call) = &self.call else {
            return;
        };
        if let Some(id) = call.id.clone() {
            let declining = call.incoming && call.status != CallStatus::Active;
            let request = if declining {
                self.client().decline(&id)
            } else {
                self.client().hang_up(&id)
            };
            self.http(request, Route::Done);
        }
        // A call the server has not answered about yet (`Starting`) has
        // no id to hang up: its answer finds no call and hangs it up then.
        self.end_call(note);
    }

    fn accept(&mut self) {
        let Some(call) = &mut self.call else {
            return;
        };
        if !call.incoming || call.status != CallStatus::Ringing {
            return;
        }
        match call.id.clone() {
            Some(id) if !call.scripted => {
                call.status = CallStatus::Starting;
                let serial = call.serial;
                self.http(self.client().accept(&id), Route::Accepted(serial));
            }
            _ => self.activate_call(0),
        }
    }

    fn toggle_recording(&mut self) {
        let client = self.client();
        let Some(call) = &mut self.call else {
            return;
        };
        if call.status != CallStatus::Active {
            return;
        }
        if call.scripted {
            call.recording = match call.recording {
                Recording::Off => Recording::On(None),
                _ => Recording::Off,
            };
            return;
        }
        let serial = call.serial;
        match call.recording.clone() {
            Recording::Off => {
                let of = match (&call.subject, &call.id) {
                    (Subject::Room { name, .. }, _) => api::Recorded::Room(name),
                    (Subject::Person(_), Some(id)) => api::Recorded::Call(id),
                    (Subject::Person(_), None) => return,
                };
                let request = client.start_recording(of);
                call.recording = Recording::Starting;
                self.http(request, Route::RecordingStarted(serial));
            }
            Recording::On(Some(id)) => {
                let request = client.stop_recording(&id);
                call.recording = Recording::Stopping(id);
                self.http(request, Route::RecordingStopped(serial));
            }
            Recording::On(None) => call.recording = Recording::Off,
            Recording::Starting | Recording::Stopping(_) => {}
        }
    }

    /// A room's video remembers its recording in the room, so that
    /// coming back to it shows it still running.
    fn note_room_recording(&mut self) {
        let Some(call) = &self.call else {
            return;
        };
        let Subject::Room { name, .. } = &call.subject else {
            return;
        };
        let (name, id) = (
            name.clone(),
            match &call.recording {
                Recording::On(id) => id.clone(),
                _ => None,
            },
        );
        if let Some(room) = self.room_mut(&name) {
            room.recording = id;
        }
    }

    fn room_video(&mut self) {
        if self.call.is_some() || self.phone.is_some() {
            return;
        }
        let Some(room) = self.room().cloned() else {
            return;
        };
        let online = self.link.is_online();
        let subject = Subject::Room {
            name: room.name.clone(),
            title: room.title,
            face: room.face,
        };
        let mut call = self.new_call(subject, false, true, !online);
        call.status = CallStatus::Active;
        if online {
            if let Some(id) = room.recording {
                call.recording = Recording::On(Some(id));
            }
            self.http(
                self.client().ticket(&room.name, true),
                Route::RoomTicket(call.serial),
            );
        }
        self.call = Some(call);
        self.nav.push(Screen::VideoCall);
    }

    // ------------------------------------------------------------- phones

    /// Dials `number` (already checked by [`model::dialable`]).
    fn start_phone(&mut self, number: &str) {
        if self.call.is_some() || self.phone.is_some() {
            return;
        }
        let online = self.link.is_online();
        let mut phone = self.new_phone(number, !online);
        if online {
            phone.status = PhoneStatus::Dialling;
            let serial = self.next_lasting();
            let request_id = api::request_id("dial", &self.device_id, serial);
            self.http(
                self.client().dial(number, &request_id),
                Route::Dialled(phone.serial),
            );
        }
        self.phone = Some(phone);
        self.nav.push(Screen::PhoneCall);
    }

    /// The phone call is over: its screen and the dial pad under it leave.
    fn end_phone(&mut self, note: Option<String>) {
        let Some(phone) = self.phone.take() else {
            return;
        };
        if phone.scripted {
            let answered = phone.status == PhoneStatus::Active;
            self.items.insert(
                0,
                Item {
                    target: Target::Phone(phone.number.clone()),
                    title: phone
                        .name
                        .clone()
                        .unwrap_or_else(|| model::pretty_number(&phone.number)),
                    preview: if answered {
                        format!("Phone call \u{b7} {}", model::duration(phone.seconds))
                    } else {
                        "Phone call \u{b7} no answer".to_owned()
                    },
                    mark: Mark::Phone,
                    when: "Sasa".to_owned(),
                    unread: 0,
                    face: phone.face.clone().unwrap_or_else(|| Face::of("#")),
                    missed: false,
                },
            );
        }
        self.dial.clear();
        self.leave(&[Screen::PhoneCall, Screen::Dial]);
        if let Some(note) = note {
            self.toast(note);
        }
        if self.link.is_online() {
            self.poll_home.due = self.clock;
        }
    }

    fn hang_up_phone(&mut self) {
        let Some(phone) = &self.phone else {
            return;
        };
        let (scripted, id, can_hang_up) = (phone.scripted, phone.id.clone(), phone.can_hang_up);
        let mut note = None;
        if !scripted {
            if let Some(id) = id {
                self.http(self.client().phone_hang_up(&id), Route::PhoneLeft);
            }
            if !can_hang_up {
                note = Some("You left the call. It ends when the other side hangs up.".to_owned());
            }
        }
        self.end_phone(note);
    }

    /// The phone call as the server describes it now.
    fn take_phone(&mut self, answer: api::PhoneAnswer) {
        let Some(phone) = &mut self.phone else {
            return;
        };
        let row = answer.call;
        phone.id = Some(row.id);
        if answer.ticket.is_some() {
            phone.ticket = answer.ticket;
        }
        if let Some(caller_id) = &row.caller_id {
            phone.caller_id = model::pretty_number(caller_id);
        }
        if let Some(rate) = row.rate {
            phone.rate = rate;
        }
        phone.can_hang_up = row.can_hang_up;
        match row.status {
            api::PhoneState::Ringing => phone.status = PhoneStatus::Ringing,
            api::PhoneState::Active => {
                if phone.status != PhoneStatus::Active {
                    phone.status = PhoneStatus::Active;
                    phone.ticks = 0;
                }
                phone.seconds = row.seconds;
            }
            api::PhoneState::Ended => {
                let cost = match row.price_tzs {
                    Some(price) => format!(" \u{b7} TSh {price}"),
                    None => String::new(),
                };
                self.end_phone(Some(format!(
                    "Call ended \u{b7} {}{cost}",
                    model::duration(row.seconds)
                )));
            }
            api::PhoneState::Missed => self.end_phone(Some("No answer.".to_owned())),
            api::PhoneState::Failed => {
                self.end_phone(Some("The call could not be placed.".to_owned()));
            }
        }
    }

    // ----------------------------------------------------------- the sound

    /// The live room the call in progress should be in: its serial, its
    /// ticket, and how the microphone and the loudspeaker stand. A phone
    /// call's room is joined as soon as its ticket is there, so that the
    /// first words of whoever picks up are heard; a call in the app when
    /// it is answered; a room's call when its ticket arrives. The call
    /// screen is held in front of the face, so its sound is the
    /// loudspeaker's; a phone call starts at the ear.
    fn sound_wanted(&self) -> Option<(u32, LiveJoin)> {
        if let Some(phone) = &self.phone {
            let ticket = phone.ticket.as_ref().filter(|_| !phone.scripted)?;
            // A call is a conversation: it goes on while the user looks
            // something up in another app or the screen locks, under a
            // notification that names it and can end it (ADR 0042).
            let title = format!(
                "Call to {}",
                phone.name.as_deref().unwrap_or(phone.number.as_str())
            );
            return Some((
                phone.serial,
                LiveJoin::new(ticket.url.clone(), ticket.token.clone())
                    .microphone(!phone.muted)
                    .speaker(phone.speaker)
                    .background(LiveBackground::new(title, sound::NOTICE)),
            ));
        }
        let call = self.call.as_ref()?;
        let ticket = call
            .ticket
            .as_ref()
            .filter(|_| !call.scripted && call.status == CallStatus::Active)?;
        Some((
            call.serial,
            LiveJoin::new(ticket.url.clone(), ticket.token.clone())
                .microphone(!call.muted)
                .speaker(true)
                // A call with video starts with the camera as its button
                // stands; a voice call starts without.
                .camera(call.video && call.camera)
                .background(LiveBackground::new(
                    format!("Call with {}", call.subject.title()),
                    sound::NOTICE,
                )),
        ))
    }

    /// Makes the live room follow the call: run after every message.
    /// Whatever ended the call in progress (a button, Back, the other
    /// side, the server going away), its room is left here, in one place,
    /// and a call's room is joined once: a room that failed is not tried
    /// again behind the user's back.
    fn follow_call(&mut self) {
        let wanted = self.sound_wanted();
        if self.sound_for == wanted.as_ref().map(|(serial, _)| *serial) {
            return;
        }
        if self.sound.in_room() {
            self.asks.push(Ask::Leave);
        }
        self.sound = Sound::Off;
        self.sound_for = None;
        self.sound_others = 0;
        self.pictures = Pictures::default();
        self.routes = LiveAudioRoutes::default();
        if let Some((serial, join)) = wanted {
            self.sound = Sound::Joining;
            self.sound_for = Some(serial);
            self.asks.push(Ask::Join(serial, join));
        }
    }

    /// Something happened in the live room joined for the call `serial`.
    fn heard_live(&mut self, serial: u32, event: LiveEvent) {
        // The end of a room that was left for another call, or for none.
        if self.sound_for != Some(serial) {
            return;
        }
        let sentence = match event {
            LiveEvent::Connected | LiveEvent::Reconnected => {
                self.sound = Sound::On;
                return;
            }
            LiveEvent::Reconnecting => {
                self.sound = Sound::Reconnecting;
                return;
            }
            LiveEvent::ParticipantJoined(_) => {
                self.sound_others += 1;
                return;
            }
            LiveEvent::ParticipantLeft(_) => {
                self.sound_others = self.sound_others.saturating_sub(1);
                // The other side is gone. Whether the call is over is the
                // server's to say, and it is asked now rather than at the
                // next tick: a call that goes on behind another app has no
                // ticks, and must not sit there alone with the microphone
                // open until the user comes back.
                if self.sound_others == 0 {
                    self.poll_call.due = self.clock;
                    self.poll();
                }
                return;
            }
            // Where the sound is: the Speaker button follows what is true
            // (a headset that connects takes the sound from the loudspeaker).
            LiveEvent::AudioRoutes(routes) => {
                if let Some(phone) = &mut self.phone {
                    phone.speaker = routes.selected == Some(LiveAudioRoute::Speaker);
                }
                self.routes = routes;
                return;
            }
            // Ended from the notification, while the app was behind another
            // one: the room is closed already, and the call is hung up as
            // the red button hangs it up.
            LiveEvent::Disconnected(LiveDisconnect::HungUp) => {
                self.sound = Sound::Failed(sound::HUNG_UP);
                self.sound_others = 0;
                self.pictures = Pictures::default();
                if self.phone.is_some() {
                    self.hang_up_phone();
                } else {
                    self.hang_up_call(None);
                }
                return;
            }
            // Not asked for.
            LiveEvent::Speaking(_) => return,
            LiveEvent::VideoStarted(video) => {
                match (video.local, video.source) {
                    (true, LiveVideoSource::Camera) => self.pictures.own = Some(video.track),
                    (true, LiveVideoSource::Screen) => self.pictures.sharing = true,
                    (false, LiveVideoSource::Camera) => self.pictures.theirs = Some(video.track),
                    (false, LiveVideoSource::Screen) => self.pictures.screen = Some(video.track),
                }
                return;
            }
            LiveEvent::VideoStopped(video) => {
                let over = Some(video.track);
                if video.local && video.source == LiveVideoSource::Screen {
                    self.pictures.sharing = false;
                }
                for held in [
                    &mut self.pictures.own,
                    &mut self.pictures.theirs,
                    &mut self.pictures.screen,
                ] {
                    if *held == over {
                        *held = None;
                    }
                }
                return;
            }
            // The call goes on without the camera or the screen; the
            // button goes back to what is true, and the reason is said.
            LiveEvent::MediaFailed { source, error } => {
                let sentence = match (source, error) {
                    (LiveVideoSource::Camera, LiveError::PermissionDenied) => pictures::NO_CAMERA,
                    (LiveVideoSource::Camera, LiveError::Unsupported) => {
                        pictures::CAMERA_UNAVAILABLE
                    }
                    (LiveVideoSource::Camera, LiveError::Failed(_)) => pictures::CAMERA_FAILED,
                    (LiveVideoSource::Screen, LiveError::PermissionDenied) => pictures::NO_SCREEN,
                    (LiveVideoSource::Screen, LiveError::Unsupported) => {
                        pictures::SCREEN_UNAVAILABLE
                    }
                    (LiveVideoSource::Screen, LiveError::Failed(_)) => pictures::SCREEN_FAILED,
                };
                match source {
                    LiveVideoSource::Camera => {
                        self.pictures.own = None;
                        if let Some(call) = &mut self.call {
                            call.camera = false;
                        }
                    }
                    LiveVideoSource::Screen => self.pictures.sharing = false,
                }
                self.toast(sentence);
                return;
            }
            // Someone wrote in the room this call belongs to: its messages
            // are the server's to give, and are asked for now rather than
            // at the next poll. What the data says is not read.
            LiveEvent::Data(data) => {
                if data.topic == "chat" {
                    self.poll_room.due = self.clock;
                    self.refresh_room();
                }
                return;
            }
            // The platform left the room because the app left the screen.
            // The next tick of the timer, which only comes with a frame
            // and so only when the app is on screen again, joins it anew.
            LiveEvent::Disconnected(LiveDisconnect::Hidden) => {
                self.sound = Sound::Away;
                self.sound_others = 0;
                self.pictures = Pictures::default();
                return;
            }
            // The app's own doing; nothing to say.
            LiveEvent::Disconnected(LiveDisconnect::Left | LiveDisconnect::Replaced) => {
                self.sound = Sound::Off;
                self.sound_others = 0;
                self.pictures = Pictures::default();
                return;
            }
            LiveEvent::Disconnected(LiveDisconnect::Ended) => sound::ENDED,
            LiveEvent::Disconnected(LiveDisconnect::Lost) => sound::LOST,
            LiveEvent::Failed(LiveError::PermissionDenied) => sound::NO_MICROPHONE,
            LiveEvent::Failed(LiveError::Unsupported) => sound::UNAVAILABLE,
            // The platform's own text is for its log, not for the screen.
            LiveEvent::Failed(LiveError::Failed(_)) => sound::FAILED,
        };
        self.sound = Sound::Failed(sentence);
        self.sound_others = 0;
        self.pictures = Pictures::default();
        self.asks.push(Ask::Announce(sentence.to_owned()));
    }

    // ----------------------------------------------------------- the timer

    fn tick(&mut self) {
        self.clock += 1;
        // A tick comes with a frame, and a frame only while the app is on
        // screen: it is back, and the room it lost by leaving is joined
        // again (`follow_call`, after this message).
        if self.sound == Sound::Away {
            self.sound = Sound::Off;
            self.sound_for = None;
        }
        if let Some((_, since)) = &self.notice
            && self.clock - since >= TOAST_TICKS
        {
            self.notice = None;
        }
        while let Some(index) = self.script.iter().position(|(due, _)| *due <= self.clock) {
            let (_, script) = self.script.remove(index);
            self.play_script(script);
        }
        if let Some(playing) = &mut self.playing {
            playing.ticks += 1;
            let length = self
                .rooms
                .iter()
                .find(|room| room.name == playing.room)
                .and_then(|room| room.entries.get(playing.index))
                .and_then(|entry| match entry {
                    Entry::Message(ChatMessage {
                        body: Body::Voice(seconds),
                        ..
                    }) => Some(*seconds),
                    _ => None,
                })
                .unwrap_or(0);
            if playing.ticks >= length * TICKS_PER_SECOND {
                self.playing = None;
            }
        }
        self.tick_call();
        self.tick_phone();
        if self.link.is_online() {
            self.poll();
        }
    }

    fn tick_call(&mut self) {
        let Some(call) = &mut self.call else {
            return;
        };
        call.ticks += 1;
        match call.status {
            CallStatus::Active if call.ticks >= TICKS_PER_SECOND => {
                call.ticks = 0;
                call.seconds += 1;
            }
            // The scripted app: the other side answers two seconds in,
            // if their app is open.
            CallStatus::Ringing if call.scripted && !call.incoming => {
                let (online, name) = match &call.subject {
                    Subject::Person(person) => (person.online, person.name.clone()),
                    Subject::Room { .. } => (true, String::new()),
                };
                if online && call.ticks >= ANSWER_TICKS {
                    self.activate_call(0);
                } else if !online && call.ticks >= 2 * ANSWER_TICKS {
                    self.end_call(Some(format!("{name} did not answer.")));
                }
            }
            _ => {}
        }
    }

    fn tick_phone(&mut self) {
        let Some(phone) = &mut self.phone else {
            return;
        };
        phone.ticks += 1;
        match phone.status {
            PhoneStatus::Active if phone.ticks >= TICKS_PER_SECOND => {
                phone.ticks = 0;
                phone.seconds += 1;
            }
            PhoneStatus::Ringing if phone.scripted && phone.ticks >= ANSWER_TICKS => {
                phone.status = PhoneStatus::Active;
                phone.ticks = 0;
            }
            _ => {}
        }
    }

    /// Asks the server whatever the screen on top needs to know again.
    fn poll(&mut self) {
        let clock = self.clock;
        if self.call.is_none()
            && self.phone.is_none()
            && self.screen() != Screen::Welcome
            && self.poll_incoming.ready(clock)
        {
            self.poll_incoming.sent(clock, POLL_INCOMING);
            self.http(self.client().incoming(), Route::Incoming);
        }
        match self.screen() {
            Screen::Home | Screen::Notifications if self.poll_home.ready(clock) => {
                self.refresh_home();
            }
            Screen::Room if self.poll_room.ready(clock) => self.refresh_room(),
            Screen::People if self.poll_people.ready(clock) => self.refresh_people(),
            Screen::Incoming | Screen::VideoCall if self.poll_call.ready(clock) => {
                let Some(call) = &self.call else {
                    return;
                };
                let Some(id) = call.id.clone() else {
                    return;
                };
                let (serial, active) = (call.serial, call.status == CallStatus::Active);
                self.poll_call
                    .sent(clock, if active { POLL_ACTIVE } else { POLL_RINGING });
                self.http(self.client().call(&id), Route::CallPolled(serial));
            }
            Screen::PhoneCall if self.poll_call.ready(clock) => {
                let Some(phone) = &self.phone else {
                    return;
                };
                let Some(id) = phone.id.clone() else {
                    return;
                };
                let serial = phone.serial;
                self.poll_call.sent(clock, POLL_PHONE);
                self.http(self.client().phone_call(&id), Route::PhonePolled(serial));
            }
            _ => {}
        }
    }

    // --------------------------------------------------------- the answers

    fn net(&mut self, net: Net) {
        match net {
            Net::Health(sync, wire) => {
                if sync != self.sync {
                    return;
                }
                match api::decode_health(wire) {
                    Ok(health) if health.database => {
                        // Not online yet: who the user is, then the rooms
                        // and the list. The name goes first, so that the
                        // identity the server derives is the name's.
                        self.sync_health = Some(health);
                        self.http(self.client().put_me(&self.name), Route::Me(sync));
                    }
                    Ok(_) => {
                        self.offline(&Failure::Server {
                            status: 503,
                            code: "database_unavailable".to_owned(),
                            message: String::new(),
                            retry_after: None,
                        });
                    }
                    Err(failure) => self.offline(&failure),
                }
            }
            Net::Me(sync, wire) => {
                if sync != self.sync {
                    return;
                }
                match api::decode_me(wire) {
                    Ok(me) => {
                        self.identity = Some(me.identity);
                        self.http(self.client().rooms(), Route::Rooms(sync));
                    }
                    Err(failure) => self.offline(&failure),
                }
            }
            Net::Rooms(sync, wire) => {
                if sync != self.sync {
                    return;
                }
                match api::decode_rooms(wire) {
                    Ok(rooms) => {
                        self.sync_rooms = Some(rooms);
                        self.http(self.client().activity(), Route::Activity(sync));
                    }
                    Err(failure) => {
                        self.poll_home.flying = false;
                        if self.link.is_online() {
                            self.note_failure(&failure);
                        } else {
                            self.offline(&failure);
                        }
                    }
                }
            }
            Net::Activity(sync, wire) => {
                if sync != self.sync {
                    return;
                }
                self.poll_home.flying = false;
                let Some(rooms) = self.sync_rooms.take() else {
                    return;
                };
                match api::decode_activity(wire) {
                    Ok((rows, now)) => {
                        if let Some(health) = self.sync_health.take() {
                            self.go_online(health);
                        }
                        if self.link.is_online() {
                            self.take_rooms(rooms);
                            self.take_activity(rows, &now);
                        }
                    }
                    Err(failure) => {
                        if self.link.is_online() {
                            self.note_failure(&failure);
                        } else {
                            self.offline(&failure);
                        }
                    }
                }
            }
            Net::Account(wire) => {
                if let Ok(account) = api::decode_account(wire) {
                    self.account = Some(account);
                }
            }
            Net::Renamed(wire) => match api::decode_me(wire) {
                Ok(me) => {
                    self.identity = Some(me.identity);
                    self.toast("Name saved.");
                }
                Err(failure) => {
                    self.toast(format!("The name was not saved. {}", failure.sentence()));
                    self.note_failure(&failure);
                }
            },
            Net::People(wire) => {
                self.poll_people.flying = false;
                match api::decode_people(wire) {
                    Ok(rows) if self.link.is_online() => {
                        self.people = rows
                            .into_iter()
                            .map(|row| Person {
                                face: Face::of(&row.peer.name),
                                identity: row.peer.identity,
                                name: row.peer.name,
                                online: row.online,
                                unread: row.unread,
                                phone: None,
                            })
                            .collect();
                        // The line under a conversation's name follows.
                        for index in 0..self.rooms.len() {
                            let Some(identity) = self.rooms[index].peer() else {
                                continue;
                            };
                            if let Some(person) = self.person(identity) {
                                self.rooms[index].status =
                                    if person.online { "Online" } else { "Offline" }.to_owned();
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(failure) => self.note_failure(&failure),
                }
            }
            Net::Messages(name, wire) => self.took_messages(&name, wire),
            Net::Posted(name, local, wire) => self.took_posted(&name, local, wire),
            Net::RoomMade(wire) => {
                self.making_room = false;
                match api::decode_room(wire) {
                    Ok(row) if self.link.is_online() => {
                        let name = row.name.clone();
                        if self.room_mut(&name).is_none() {
                            let mut rows = vec![row];
                            let rooms = || self.rooms.iter().filter(|room| room.peer().is_none());
                            rows.extend(rooms().map(|room| api::RoomRow {
                                name: room.name.clone(),
                                display_name: room.title.clone(),
                                members: 0,
                                live: room.live,
                                recording: room.recording.clone(),
                            }));
                            let statuses: Vec<String> =
                                rooms().map(|room| room.status.clone()).collect();
                            self.take_rooms(rows);
                            for (room, status) in self.rooms.iter_mut().skip(1).zip(statuses) {
                                room.status = status;
                            }
                        }
                        if self.screen() == Screen::NewRoom {
                            self.enter_new_room(&name);
                        }
                    }
                    Ok(_) => {}
                    Err(failure) => {
                        self.new_room_problem = Some(failure.sentence());
                        self.note_failure(&failure);
                    }
                }
            }
            Net::Incoming(wire) => {
                self.poll_incoming.flying = false;
                match api::decode_incoming(wire) {
                    Ok(rows) => {
                        let ringing = rows
                            .into_iter()
                            .find(|row| !self.dismissed.contains(&row.id));
                        if let Some(row) = ringing
                            && self.call.is_none()
                            && self.phone.is_none()
                            && self.link.is_online()
                            && self.screen() != Screen::Welcome
                        {
                            self.ringing(row);
                        }
                    }
                    Err(failure) => self.note_failure(&failure),
                }
            }
            Net::Rung(serial, wire) => {
                let current = self.call.as_ref().map(|call| call.serial) == Some(serial);
                match api::decode_call(wire) {
                    Ok((row, ticket)) if current => {
                        if let Some(call) = &mut self.call {
                            call.id = Some(row.id.clone());
                            call.ticket = ticket;
                            call.status = CallStatus::Ringing;
                            call.ticks = 0;
                        }
                        self.poll_call = Poll {
                            due: self.clock + POLL_RINGING,
                            flying: false,
                        };
                        self.take_call(row);
                    }
                    // The user gave up before the server said the call
                    // exists: it is hung up now.
                    Ok((row, _)) => {
                        if !row.status.over() {
                            self.http(self.client().hang_up(&row.id), Route::Done);
                        }
                        self.dismissed.push(row.id);
                    }
                    Err(failure) => {
                        if current {
                            self.end_call(Some(failure.sentence()));
                        }
                        self.note_failure(&failure);
                    }
                }
            }
            Net::CallPolled(serial, wire) => {
                if self.call.as_ref().map(|call| call.serial) != Some(serial) {
                    return;
                }
                self.poll_call.flying = false;
                match api::decode_call(wire) {
                    Ok((row, _)) => self.take_call(row),
                    Err(failure) if failure.status() == Some(404) => {
                        self.end_call(Some("The call is over.".to_owned()));
                    }
                    Err(failure) => self.note_failure(&failure),
                }
            }
            Net::Accepted(serial, wire) => {
                if self.call.as_ref().map(|call| call.serial) != Some(serial) {
                    return;
                }
                match api::decode_call(wire) {
                    Ok((row, ticket)) => {
                        if let Some(call) = &mut self.call {
                            call.ticket = ticket;
                            call.status = CallStatus::Ringing;
                        }
                        self.take_call(row);
                    }
                    Err(failure) if failure.status() == Some(409) => {
                        self.end_call(Some("The caller hung up first.".to_owned()));
                    }
                    Err(failure) => {
                        if let Some(call) = &mut self.call {
                            call.status = CallStatus::Ringing;
                        }
                        self.toast(failure.sentence());
                        self.note_failure(&failure);
                    }
                }
            }
            Net::RoomTicket(serial, wire) => {
                if self.call.as_ref().map(|call| call.serial) != Some(serial) {
                    return;
                }
                match api::decode_ticket(wire) {
                    Ok(ticket) => {
                        if let Some(call) = &mut self.call {
                            call.ticket = Some(ticket);
                        }
                    }
                    Err(failure) => {
                        self.end_call(Some(failure.sentence()));
                        self.note_failure(&failure);
                    }
                }
            }
            Net::RecordingStarted(serial, wire) => {
                let current = self.call.as_ref().map(|call| call.serial) == Some(serial);
                match api::decode_recording(wire) {
                    Ok((id, running)) => {
                        if let Some(call) = &mut self.call
                            && current
                        {
                            call.recording = if running {
                                Recording::On(Some(id))
                            } else {
                                Recording::Off
                            };
                        } else if running {
                            // The call is gone: so is the wish to record
                            // (and pay for) it.
                            self.http(self.client().stop_recording(&id), Route::Done);
                        }
                    }
                    Err(failure) => {
                        if let Some(call) = &mut self.call
                            && current
                        {
                            call.recording = Recording::Off;
                            self.toast(format!("Not recording. {}", failure.sentence()));
                        }
                        self.note_failure(&failure);
                    }
                }
                self.note_room_recording();
            }
            Net::RecordingStopped(serial, wire) => {
                let current = self.call.as_ref().map(|call| call.serial) == Some(serial);
                match api::decode_recording(wire) {
                    Ok((id, running)) => {
                        if let Some(call) = &mut self.call
                            && current
                        {
                            call.recording = if running {
                                Recording::On(Some(id))
                            } else {
                                Recording::Off
                            };
                        }
                    }
                    Err(failure) => {
                        if let Some(call) = &mut self.call
                            && current
                        {
                            // Still recording, as far as the app knows:
                            // the button stops it again.
                            if let Recording::Stopping(id) = call.recording.clone() {
                                call.recording = Recording::On(Some(id));
                            }
                            self.toast(format!(
                                "The recording may still be running. {}",
                                failure.sentence()
                            ));
                        }
                        self.note_failure(&failure);
                    }
                }
                self.note_room_recording();
            }
            Net::Dialled(serial, wire) => {
                let current = self.phone.as_ref().map(|phone| phone.serial) == Some(serial);
                match api::decode_phone(wire) {
                    Ok(answer) if current => {
                        self.poll_call = Poll {
                            due: self.clock + POLL_PHONE,
                            flying: false,
                        };
                        if let Some(phone) = &mut self.phone {
                            phone.ticks = 0;
                        }
                        self.take_phone(answer);
                    }
                    // The user left before the server said the call
                    // exists: the server is told so now.
                    Ok(answer) => {
                        if !answer.call.status.over() {
                            self.http(self.client().phone_hang_up(&answer.call.id), Route::Done);
                        }
                    }
                    Err(failure) => {
                        if current {
                            // Back to the dial pad, which says why.
                            self.phone = None;
                            self.leave(&[Screen::PhoneCall]);
                            let sentence = failure.sentence();
                            if self.screen() == Screen::Dial {
                                self.dial_problem = Some(sentence);
                            } else {
                                self.toast(sentence);
                            }
                        }
                        self.note_failure(&failure);
                    }
                }
            }
            Net::PhonePolled(serial, wire) => {
                if self.phone.as_ref().map(|phone| phone.serial) != Some(serial) {
                    return;
                }
                self.poll_call.flying = false;
                match api::decode_phone(wire) {
                    Ok(answer) => self.take_phone(answer),
                    Err(failure) if failure.status() == Some(404) => {
                        self.end_phone(Some("The call is over.".to_owned()));
                    }
                    Err(failure) => self.note_failure(&failure),
                }
            }
            Net::PhoneLeft(wire) => match api::decode_phone(wire) {
                Ok(answer) => {
                    if let Some(message) = answer.message {
                        self.toast(message);
                    }
                }
                Err(failure) => self.note_failure(&failure),
            },
            Net::Done(wire) => {
                if let Err(error) = wire {
                    self.note_failure(&Failure::from(error));
                }
            }
        }
    }

    /// Someone rings the user: the Incoming screen comes up.
    fn ringing(&mut self, row: api::CallRow) {
        let person = Person {
            face: Face::of(&row.peer.name),
            identity: row.peer.identity,
            name: row.peer.name,
            online: true,
            unread: 0,
            phone: None,
        };
        self.asks
            .push(Ask::Announce(format!("{} is calling.", person.name)));
        let mut call = self.new_call(Subject::Person(person), true, row.video, false);
        call.id = Some(row.id);
        self.call = Some(call);
        self.start = false;
        self.poll_call = Poll {
            due: self.clock + POLL_RINGING,
            flying: false,
        };
        self.nav.push(Screen::Incoming);
    }

    /// The call as the server describes it now.
    fn take_call(&mut self, row: api::CallRow) {
        let Some(call) = &mut self.call else {
            return;
        };
        let name = call.subject.title().to_owned();
        let incoming = call.incoming;
        match row.status {
            api::CallState::Ringing => {}
            api::CallState::Active => {
                // An accept that is still in the air decides for itself.
                if call.status == CallStatus::Starting {
                    return;
                }
                match (&call.recording, row.recording) {
                    (Recording::Off, Some(id)) => call.recording = Recording::On(Some(id)),
                    (Recording::On(_), None) => call.recording = Recording::Off,
                    _ => {}
                }
                if call.status == CallStatus::Active {
                    // The server's count wins over the app's ticks.
                    if call.seconds.abs_diff(row.seconds) > 1 {
                        call.seconds = row.seconds;
                    }
                } else {
                    self.activate_call(row.seconds);
                }
            }
            api::CallState::Declined => self.end_call(Some(format!("{name} declined."))),
            api::CallState::Missed if incoming => {
                self.end_call(Some(format!("Missed call from {name}.")));
            }
            api::CallState::Missed => self.end_call(Some(format!("{name} did not answer."))),
            api::CallState::Ended => self.end_call(Some(format!(
                "Call ended \u{b7} {}",
                model::duration(row.seconds)
            ))),
        }
    }

    fn took_messages(&mut self, name: &str, wire: Wire) {
        if self.room.as_deref() == Some(name) {
            self.poll_room.flying = false;
        }
        let online = self.link.is_online();
        let open = self.room.as_deref() == Some(name) && self.screen() == Screen::Room;
        let result = api::decode_messages(wire);
        let Some(room) = self.room_mut(name) else {
            return;
        };
        match result {
            Ok(page) if online => {
                let first = room.loaded != Loaded::Yes;
                if first {
                    // What was typed and not yet stored stays.
                    room.entries.retain(
                        |entry| matches!(entry, Entry::Message(message) if message.id == 0 && message.mine),
                    );
                }
                room.loaded = Loaded::Yes;
                let mut added = false;
                for row in page.messages {
                    room.last_id = room.last_id.max(row.id);
                    let known = room.entries.iter().any(|entry| match entry {
                        Entry::Message(message) => {
                            message.id == row.id
                                // The user's own, whose answer is still
                                // in the air: it becomes this one then.
                                || (row.mine
                                    && message.id == 0
                                    && message.delivery == Delivery::Sending
                                    && message.body == Body::Text(row.text.clone()))
                        }
                        Entry::Notice(_) => false,
                    });
                    if known {
                        continue;
                    }
                    added = true;
                    let message = Entry::Message(ChatMessage {
                        id: row.id,
                        local: 0,
                        face: Face::of(&row.from_name),
                        author: row.from_name,
                        mine: row.mine,
                        time: model::ago(&row.created_at, &page.now),
                        body: Body::Text(row.text),
                        delivery: if row.mine {
                            Delivery::Sent
                        } else {
                            Delivery::Received
                        },
                    });
                    // Stored messages come before the ones on their way.
                    let at = room
                        .entries
                        .iter()
                        .position(
                            |entry| matches!(entry, Entry::Message(message) if message.id == 0),
                        )
                        .unwrap_or(room.entries.len());
                    room.entries.insert(at, message);
                }
                if !page.more {
                    // The whole room is known up to here; with more to
                    // come, the next page starts after this one's last.
                    room.last_id = room.last_id.max(page.last_id);
                }
                if page.more && open {
                    // More than a page was new: the rest, at once.
                    self.poll_room.due = self.clock;
                }
                if open && (added || first) {
                    if first {
                        self.asks
                            .push(Ask::Scroll(ScrollTo::end(Screen::Room.scroll_id())));
                    } else {
                        self.show_latest();
                    }
                }
            }
            Ok(_) => {}
            Err(failure) => {
                if room.loaded != Loaded::Yes {
                    room.loaded = Loaded::Failed(failure.sentence());
                }
                self.note_failure(&failure);
            }
        }
    }

    fn took_posted(&mut self, name: &str, local: u32, wire: Wire) {
        let result = api::decode_posted(wire);
        let Some(room) = self.room_mut(name) else {
            return;
        };
        let Some(index) = room.entries.iter().position(
            |entry| matches!(entry, Entry::Message(message) if message.local == local && message.mine),
        ) else {
            return;
        };
        match result {
            Ok(row) => {
                room.last_id = room.last_id.max(row.id);
                let twin = room
                    .entries
                    .iter()
                    .any(|entry| matches!(entry, Entry::Message(message) if message.id == row.id));
                if twin {
                    // A poll brought it first.
                    room.entries.remove(index);
                } else if let Entry::Message(message) = &mut room.entries[index] {
                    message.id = row.id;
                    message.body = Body::Text(row.text);
                    message.delivery = Delivery::Sent;
                }
            }
            Err(failure) => {
                if let Entry::Message(message) = &mut room.entries[index] {
                    message.delivery = Delivery::Failed(failure.sentence());
                }
                self.note_failure(&failure);
            }
        }
    }

    // ------------------------------------------------------------ the taps

    /// What reaching a screen sets going.
    fn arrived(&mut self, from: Screen) {
        let screen = self.screen();
        if screen == from {
            return;
        }
        if from == Screen::Home {
            self.start = false;
        }
        match screen {
            Screen::Home | Screen::Notifications => {
                if matches!(self.link, Link::Offline(_)) {
                    self.check_server();
                } else {
                    self.poll_home.due = self.clock;
                    self.refresh_home();
                }
            }
            Screen::Room => {
                self.poll_room.due = self.clock;
                self.refresh_room();
            }
            Screen::People => {
                self.poll_people.due = self.clock;
                self.refresh_people();
            }
            Screen::Server => {
                self.name_draft.clone_from(&self.name);
                self.name_problem = None;
            }
            _ => {}
        }
    }

    fn open_item(&mut self, index: usize) {
        let Some(item) = self.items.get(index) else {
            return;
        };
        match item.target.clone() {
            Target::Room(name) => self.open_room(&name),
            Target::Ring(identity) => {
                // The design's first row leads to its Incoming page: in
                // the scripted app, this person rings.
                let Some(person) = self.person(&identity).cloned() else {
                    return;
                };
                if self.call.is_none() && self.phone.is_none() {
                    self.items[index].unread = 0;
                    self.call = Some(self.new_call(Subject::Person(person), true, true, true));
                    self.nav.push(Screen::Incoming);
                }
            }
            Target::Person { identity, video } => {
                let person = self.person(&identity).cloned().or_else(|| {
                    // Not in the list of people (it is fetched when its
                    // screen opens): the row knows who it is.
                    (!identity.is_empty()).then(|| Person {
                        face: item.face.clone(),
                        name: item.title.clone(),
                        identity,
                        online: true,
                        unread: 0,
                        phone: None,
                    })
                });
                match person {
                    Some(person) => self.ring(person, video),
                    None => self.toast("There is nobody to call back."),
                }
            }
            Target::Phone(number) => {
                if self.link.is_online() {
                    // A phone call costs money: the number is put on the
                    // dial pad and dialled by a press of its own.
                    self.dial = number;
                    self.dial_problem = None;
                    self.nav.push(Screen::Dial);
                } else {
                    self.start_phone(&number);
                }
            }
        }
    }

    fn show_tab(&mut self, tab: Tab) {
        self.start = false;
        if self.screen() != tab.screen() {
            self.nav.reset(tab.screen());
        }
    }

    fn handle(&mut self, message: Message) {
        match message {
            Message::Back => {
                self.go_back();
            }
            Message::Tick => self.tick(),
            Message::Net(net) => self.net(net),
            Message::Live(serial, event) => self.heard_live(serial, event),

            Message::Name(name) => {
                self.name_draft = name.chars().take(api::NAME_LIMIT).collect();
                self.name_problem = None;
            }
            Message::Continue => {
                if self.keep_name() {
                    self.nav.reset(Screen::Home);
                    self.check_server();
                }
            }
            Message::SaveName => {
                let before = self.name.clone();
                if self.keep_name() {
                    if self.link.is_online() && self.name != before {
                        self.http(self.client().put_me(&self.name), Route::Renamed);
                    } else {
                        self.toast("Name saved.");
                    }
                }
            }

            Message::Show(tab) => self.show_tab(tab),
            Message::ShowCalls => {
                self.show_tab(Tab::Home);
                self.filter = Filter::Calls;
            }
            Message::SetFilter(filter) => self.filter = filter,
            Message::ToggleSearch => {
                self.searching = !self.searching;
                self.query.clear();
            }
            Message::Query(query) => self.query = query.chars().take(60).collect(),
            Message::OpenItem(index) => self.open_item(index),
            Message::OpenRoom(name) => self.open_room(&name),
            Message::OpenNewRoom => {
                self.start = false;
                self.new_room.clear();
                self.new_room_problem = None;
                self.nav.push(Screen::NewRoom);
            }
            Message::OpenDial => {
                self.start = false;
                self.dial.clear();
                self.dial_problem = None;
                self.nav.push(Screen::Dial);
            }
            Message::OpenStart => self.start = true,
            Message::CloseStart => self.start = false,
            Message::OpenServer => self.nav.push(Screen::Server),
            Message::OpenPeople => {
                self.start = false;
                self.nav.push(Screen::People);
            }

            Message::RoomName(name) => {
                self.new_room = name.chars().take(api::ROOM_NAME_LIMIT).collect();
                self.new_room_problem = None;
            }
            Message::CreateRoom => self.create_room(),

            Message::Draft(draft) => {
                self.draft = draft.chars().take(api::MESSAGE_LIMIT).collect();
            }
            Message::Send => self.send(),
            Message::Retry(local) => self.retry(local),
            Message::Attach => self.toast("There is no file picker in this version."),
            Message::PlayVoice(index) => {
                let Some(room) = self.room.clone() else {
                    return;
                };
                let same = self
                    .playing
                    .as_ref()
                    .is_some_and(|playing| playing.room == room && playing.index == index);
                if same {
                    self.playing = None;
                } else {
                    self.playing = Some(Playing {
                        room,
                        index,
                        ticks: 0,
                    });
                    self.toast("A sample voice message: it has no sound.");
                }
            }
            Message::RoomPhone => {
                // In a conversation between two people the two buttons
                // ring the other person, free, in the app.
                if let Some(person) = self.direct_peer() {
                    self.ring(person, false);
                    return;
                }
                self.dial.clear();
                self.dial_problem = None;
                self.nav.push(Screen::Dial);
            }
            Message::RoomVideo => match self.direct_peer() {
                Some(person) => self.ring(person, true),
                None => self.room_video(),
            },

            Message::CallPerson(index, video) => {
                if let Some(person) = self.people.get(index).cloned() {
                    self.ring(person, video);
                }
            }
            Message::MessagePerson(index) => {
                if let Some(person) = self.people.get(index).cloned() {
                    self.message_person(&person);
                }
            }
            Message::PhonePerson(index) => {
                let number = self
                    .people
                    .get(index)
                    .and_then(|person| person.phone.clone());
                if let Some(number) = number {
                    self.start_phone(&number);
                }
            }

            Message::Accept => self.accept(),
            Message::Decline => self.hang_up_call(None),
            Message::ReplyInstead => self.reply_instead(),

            Message::HangUp => {
                if self.phone.is_some() {
                    self.hang_up_phone();
                } else {
                    self.hang_up_call(None);
                }
            }
            Message::ToggleMute => {
                let muted = if let Some(phone) = &mut self.phone {
                    phone.muted = !phone.muted;
                    Some(phone.muted)
                } else if let Some(call) = &mut self.call {
                    call.muted = !call.muted;
                    Some(call.muted)
                } else {
                    None
                };
                // The button acts on the room that is joined; a room
                // joined later starts as the button stands.
                if let Some(muted) = muted
                    && self.sound.in_room()
                {
                    self.asks.push(Ask::Microphone(!muted));
                }
            }
            Message::ToggleCamera => {
                let camera = self.call.as_mut().map(|call| {
                    call.camera = !call.camera;
                    (call.camera, call.scripted)
                });
                // The button acts on the room that is joined; a room
                // joined later starts as the button stands.
                if let Some((on, false)) = camera
                    && self.sound.in_room()
                {
                    if !on {
                        self.pictures.own = None;
                    }
                    self.asks.push(Ask::Camera(on));
                }
            }
            Message::SwitchCamera => {
                if self.pictures.own.is_some() {
                    self.asks.push(Ask::SwitchCamera);
                }
            }
            Message::ToggleRecording => self.toggle_recording(),
            Message::ShareScreen => {
                let scripted = self.call.as_ref().is_none_or(|call| call.scripted);
                if scripted {
                    self.toast(pictures::SCREEN_NEEDS_SERVER);
                } else if self.sound != Sound::On {
                    self.toast(pictures::SCREEN_NEEDS_SOUND);
                } else {
                    // The system asks the user; what follows is the room's
                    // to say (`heard_live`).
                    self.asks.push(Ask::ShareScreen(!self.pictures.sharing));
                }
            }
            Message::AddPerson => self.toast("Adding people to a call needs the next version."),
            Message::VolumeUp => {
                if let Some(call) = &mut self.call {
                    call.volume = (call.volume + 1).min(VOLUME_STEPS.len() - 1);
                }
            }
            Message::VolumeDown => {
                if let Some(call) = &mut self.call {
                    call.volume = call.volume.saturating_sub(1);
                }
            }
            Message::ToggleSpeaker => {
                if let Some(phone) = &mut self.phone {
                    phone.speaker = !phone.speaker;
                    if self.sound.in_room() {
                        // On: the loudspeaker. Off: back to the best of
                        // what else there is, a headset before the ear.
                        // Before the platform has said what there is, the
                        // plain choice between loudspeaker and earpiece.
                        let quiet = [
                            LiveAudioRoute::Bluetooth,
                            LiveAudioRoute::Wired,
                            LiveAudioRoute::Earpiece,
                        ]
                        .into_iter()
                        .find(|route| self.routes.available.contains(route));
                        self.asks.push(match (phone.speaker, quiet) {
                            (true, Some(_)) => Ask::Route(LiveAudioRoute::Speaker),
                            (false, Some(route)) => Ask::Route(route),
                            (on, None) => Ask::Speaker(on),
                        });
                    }
                }
            }
            Message::ToggleKeypad => {
                if let Some(phone) = &mut self.phone {
                    phone.keypad = !phone.keypad;
                }
            }
            Message::Key(key) => {
                if let Some(phone) = &mut self.phone
                    && phone.keys.chars().count() < 24
                    && (key.is_ascii_digit() || matches!(key, '*' | '#'))
                {
                    phone.keys.push(key);
                }
            }

            Message::DialKey(key) => {
                let fits = self.dial.chars().count() < DIAL_LIMIT;
                let allowed = key.is_ascii_digit() || (key == '+' && self.dial.is_empty());
                if fits && allowed {
                    self.dial.push(key);
                }
                self.dial_problem = None;
            }
            Message::DialBackspace => {
                self.dial.pop();
                self.dial_problem = None;
            }
            Message::DialCall => match model::dialable(&self.dial) {
                Ok(number) => {
                    self.dial_problem = None;
                    self.start_phone(&number);
                }
                Err(problem) => self.dial_problem = Some(problem.to_owned()),
            },

            Message::ServerUrl(url) => {
                self.server_draft = url;
                self.server_problem = None;
            }
            Message::CheckServer => {
                if self.use_typed_server() {
                    self.checked_by_hand = true;
                    self.check_server();
                }
            }
            Message::DefaultServer => {
                api::DEFAULT_BASE_URL.clone_into(&mut self.server_draft);
                if self.use_typed_server() {
                    self.checked_by_hand = true;
                    self.check_server();
                }
            }
        }
    }

    /// "Reply with a message instead": the call is declined and the
    /// caller's conversation opens: the server's with a server, the
    /// scripted app's room of that person without.
    fn reply_instead(&mut self) {
        let Some(call) = &self.call else {
            return;
        };
        if !call.incoming || call.status == CallStatus::Active {
            return;
        }
        let scripted = call.scripted;
        let subject = call.subject.clone();
        // The call is declined either way; then the conversation opens.
        self.hang_up_call(None);
        match subject {
            Subject::Person(person) if !scripted => self.message_person(&person),
            Subject::Person(person) => {
                if self.room_mut(&person.identity).is_some() {
                    self.open_room(&person.identity);
                }
            }
            Subject::Room { name, .. } => {
                if self.room_mut(&name).is_some() {
                    self.open_room(&name);
                }
            }
        }
    }

    /// What the back arrow and the system's Back do. False when the
    /// system should take it (leave the app).
    pub fn go_back(&mut self) -> bool {
        if self.start {
            self.start = false;
            return true;
        }
        if self.searching && self.screen() == Screen::Home {
            self.searching = false;
            self.query.clear();
            return true;
        }
        match self.screen() {
            Screen::PhoneCall => {
                if self.phone.as_ref().is_some_and(|phone| phone.keypad) {
                    if let Some(phone) = &mut self.phone {
                        phone.keypad = false;
                    }
                } else {
                    self.hang_up_phone();
                }
                return true;
            }
            // There is no call in the background in this version: leaving
            // a call's screen ends it.
            Screen::Incoming | Screen::VideoCall => {
                self.hang_up_call(None);
                return true;
            }
            _ => {}
        }
        if self.nav.back() {
            return true;
        }
        // Opened directly on a pushed screen (a design page): its arrow
        // leads where the design's link does.
        match self.screen() {
            Screen::Home | Screen::Welcome => false,
            Screen::Notifications | Screen::People => {
                self.nav.reset(Screen::Home);
                true
            }
            _ => {
                self.nav.reset(Screen::Home);
                true
            }
        }
    }
}

/// The word under a room's ring: the first word of its name, cut to what
/// fits under a 60 dp ring.
fn short_name(title: &str) -> String {
    let word = title.split_whitespace().next().unwrap_or_default();
    let mut short: String = word.chars().take(8).collect();
    if short.chars().count() < word.chars().count() {
        short.push('\u{2026}');
    }
    short
}

/// A room's address from its name, as the server derives it: lower case
/// letters and digits, everything else a dash.
fn slug(title: &str) -> String {
    let mut out = String::new();
    for character in title.to_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').chars().take(63).collect()
}

/// A message on one line, for a row's second line.
fn one_line(text: &str) -> String {
    api::clean(text, 80)
}

impl Application for BaruaTalk {
    type Message = Message;

    fn setup(&mut self, setup: &mut Setup<'_>) {
        setup.add_font(include_bytes!("../assets/fonts/Inter-Regular.ttf"));
        setup.add_font(include_bytes!("../assets/fonts/Inter-Medium.ttf"));
        setup.add_font(include_bytes!("../assets/fonts/Inter-SemiBold.ttf"));
        setup.add_font(include_bytes!("../assets/fonts/SpaceGrotesk-Medium.ttf"));
        setup.add_font(include_bytes!("../assets/fonts/SpaceGrotesk-SemiBold.ttf"));
        self.images = Photo::ALL
            .iter()
            .map(|photo| setup.add_image_png(photo.png()).ok())
            .collect();
        // Once: who this installation is, what it is called, where its
        // server is, and the first question to it. (A tool may set an app
        // up twice.) With nothing behind the app the question fails at
        // once as "unsupported", and the app stays the scripted one.
        if !self.ready() {
            self.device_id = api::device_id(self.device_prefix, setup.install_id());
            self.lasting = setup
                .stored(SERIAL_KEY)
                .and_then(|stored| stored.parse().ok())
                .unwrap_or(0);
            if let Some(url) = setup
                .stored(SERVER_KEY)
                .and_then(|stored| api::normalize_base_url(stored).ok())
            {
                self.server_draft.clone_from(&url);
                self.server_url = url;
            }
            let stored = setup
                .stored(NAME_KEY)
                .map(api::tidy_name)
                .unwrap_or_default();
            if self.name.is_empty() && !stored.is_empty() {
                // Not the first run: straight in.
                self.name.clone_from(&stored);
                self.name_draft = stored;
                if self.screen() == Screen::Welcome {
                    self.nav.reset(Screen::Home);
                }
            }
            self.check_server();
        }
    }

    fn theme(&self) -> Theme {
        theme::THEME
    }

    fn back(&mut self) -> bool {
        self.still = false;
        let from = self.screen();
        let handled = self.go_back();
        self.arrived(from);
        // Back from a call's screen ended the call: its room is left.
        self.follow_call();
        handled
    }

    fn tick(&self) -> Option<(Duration, Message)> {
        let moving = self.notice.is_some()
            || !self.script.is_empty()
            || self.playing.is_some()
            || self.call.is_some()
            || self.phone.is_some()
            || self.link.is_online();
        (moving && !self.still).then_some((
            Duration::from_nanos(1_000_000_000 / u64::from(TICKS_PER_SECOND)),
            Message::Tick,
        ))
    }

    fn system_bars(&self) -> Option<SystemBars> {
        // Every screen is dark, and so are the photographs under the two
        // call screens at their top and bottom edges.
        Some(SystemBars::LIGHT_CONTENT)
    }

    fn view_in(&self, viewport: &Viewport<'_>) -> Element<Message> {
        let room = viewport.safe_size().height;
        let screen = self.screen();
        let content: Element<Message> = match screen {
            Screen::Home => screens::home::view(self),
            Screen::Room => screens::room::view(self),
            Screen::Incoming => screens::incoming::view(self, room),
            Screen::VideoCall => screens::video_call::view(self, viewport),
            Screen::PhoneCall => screens::phone_call::view(self, room),
            Screen::Welcome => screens::welcome::view(self),
            Screen::Dial => screens::dial::view(self, room),
            Screen::NewRoom => screens::new_room::view(self),
            Screen::People => screens::people::view(self),
            Screen::Notifications => screens::notifications::view(self),
            Screen::Server => screens::server::view(self),
        };
        let content = content.transition(self.nav.transition());
        let keypad = self
            .phone
            .as_ref()
            .filter(|phone| phone.keypad && screen == Screen::PhoneCall);
        Stack::new()
            .push(content)
            .push_maybe(self.start.then(components::scrim::<Message>).map(|scrim| {
                scrim
                    .on_press(Message::CloseStart)
                    .label("Close")
                    .id("start-scrim")
            }))
            .push_maybe(self.start.then(screens::home::start_sheet))
            .push_maybe(keypad.map(|_| {
                components::scrim::<Message>()
                    .on_press(Message::ToggleKeypad)
                    .label("Dismiss keypad")
                    .id("keypad-scrim")
            }))
            .push_maybe(keypad.map(screens::phone_call::keypad_sheet))
            .push_maybe(self.notice().map(|text| components::toast(text.to_owned())))
            .fill()
            .into()
    }

    fn update(&mut self, message: Message) {
        // The timer and the server's answers are not touches: a still
        // frame stays still (and a design page stays the design's picture
        // when its first question to a server that is not there fails).
        let touch = !matches!(message, Message::Tick | Message::Net(_) | Message::Live(..));
        if touch {
            self.still = false;
        }
        let from = self.screen();
        self.handle(message);
        self.arrived(from);
        // Whatever that did to the call in progress, its sound follows.
        self.follow_call();
    }

    fn effects(&mut self, out: &mut Effects<Message>) {
        for ask in self.asks.drain(..) {
            match ask {
                Ask::Scroll(to) => out.scroll(to),
                Ask::Announce(text) => out.announce(text),
                Ask::Http(request, route) => out.http(request, move |wire| route.message(wire)),
                Ask::Store(key, value) => out.store(key, value),
                Ask::Join(serial, join) => {
                    out.join_live(join, move |event| Message::Live(serial, event));
                }
                Ask::Leave => out.leave_live(),
                Ask::Microphone(live) => out.set_live_microphone(live),
                Ask::Speaker(on) => out.set_live_speaker(on),
                Ask::Route(route) => out.set_live_audio_route(route),
                Ask::Camera(on) => out.set_live_camera(on),
                Ask::SwitchCamera => out.switch_live_camera(),
                Ask::ShareScreen(on) => out.set_live_screen_share(on),
            }
        }
    }
}

morningstar::app!(Ui::new(BaruaTalk::default()));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_and_short_names() {
        assert_eq!(slug("Support desk"), "support-desk");
        assert_eq!(slug("  Duka la Mama Asha!  "), "duka-la-mama-asha");
        assert_eq!(slug("---"), "");
        assert_eq!(short_name("Support desk"), "Support");
        assert_eq!(short_name("Wafanyabiashara wote"), "Wafanyab\u{2026}");
    }

    #[test]
    fn every_photo_has_its_place() {
        for (index, photo) in Photo::ALL.into_iter().enumerate() {
            assert_eq!(photo.index(), index);
            assert!(photo.png().starts_with(b"\x89PNG"));
        }
    }
}
