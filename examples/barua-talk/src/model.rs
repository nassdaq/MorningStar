//! What the app knows: people, rooms and their messages, the Home list,
//! and a call in progress. Plain data; `lib.rs` changes it and the screens
//! draw it.

use crate::api::Ticket;

// ----------------------------------------------------------------- photos

/// A photograph that came with the design (`assets/images`). The app
/// registers each once and draws it by this name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Photo {
    /// The user of the design: Neema.
    Neema,
    /// The picture of the Support desk room.
    Support,
    /// Dr. Asha Mrema, and the Clinic room.
    Asha,
    /// Zawadi Mollel, and the Duka room.
    Zawadi,
    /// Juma Kileo.
    Juma,
    /// Baraka Mushi.
    Baraka,
    /// Mzee Hamisi.
    Hamisi,
    /// The shop at night that Zawadi sent.
    Shop,
    /// Baraka on video: the frame of page 04.
    BarakaVideo,
    /// The user's own camera: the small tile of page 04.
    SelfCamera,
    /// A portrait blurred and darkened, behind that person's call
    /// (`design/prepare-images.py`).
    Backdrop(Caller),
}

/// The people of the design who can ring: each has a blurred backdrop.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Caller {
    /// Dr. Asha Mrema.
    Asha,
    /// Zawadi Mollel.
    Zawadi,
    /// Juma Kileo.
    Juma,
    /// Baraka Mushi.
    Baraka,
    /// Mzee Hamisi.
    Hamisi,
}

impl Photo {
    /// Every photograph, in the order they are registered.
    pub const ALL: [Self; 15] = [
        Self::Neema,
        Self::Support,
        Self::Asha,
        Self::Zawadi,
        Self::Juma,
        Self::Baraka,
        Self::Hamisi,
        Self::Shop,
        Self::BarakaVideo,
        Self::SelfCamera,
        Self::Backdrop(Caller::Asha),
        Self::Backdrop(Caller::Zawadi),
        Self::Backdrop(Caller::Juma),
        Self::Backdrop(Caller::Baraka),
        Self::Backdrop(Caller::Hamisi),
    ];

    /// Its place in [`ALL`](Self::ALL).
    pub fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|photo| *photo == self)
            .unwrap_or(0)
    }

    /// The file, as the app draws it: a PNG made from the design's JPEG
    /// by `design/prepare-images.py`.
    pub const fn png(self) -> &'static [u8] {
        match self {
            Self::Neema => include_bytes!("../assets/images/neema.png"),
            Self::Support => include_bytes!("../assets/images/support.png"),
            Self::Asha => include_bytes!("../assets/images/asha.png"),
            Self::Zawadi => include_bytes!("../assets/images/zawadi.png"),
            Self::Juma => include_bytes!("../assets/images/juma.png"),
            Self::Baraka => include_bytes!("../assets/images/baraka.png"),
            Self::Hamisi => include_bytes!("../assets/images/hamisi.png"),
            Self::Shop => include_bytes!("../assets/images/shop.png"),
            Self::BarakaVideo => include_bytes!("../assets/images/baraka-video.png"),
            Self::SelfCamera => include_bytes!("../assets/images/self-camera.png"),
            Self::Backdrop(Caller::Asha) => include_bytes!("../assets/images/backdrop-asha.png"),
            Self::Backdrop(Caller::Zawadi) => {
                include_bytes!("../assets/images/backdrop-zawadi.png")
            }
            Self::Backdrop(Caller::Juma) => include_bytes!("../assets/images/backdrop-juma.png"),
            Self::Backdrop(Caller::Baraka) => {
                include_bytes!("../assets/images/backdrop-baraka.png")
            }
            Self::Backdrop(Caller::Hamisi) => {
                include_bytes!("../assets/images/backdrop-hamisi.png")
            }
        }
    }

    /// The blurred backdrop of this portrait, if it has one.
    pub const fn backdrop(self) -> Option<Self> {
        match self {
            Self::Asha => Some(Self::Backdrop(Caller::Asha)),
            Self::Zawadi => Some(Self::Backdrop(Caller::Zawadi)),
            Self::Juma => Some(Self::Backdrop(Caller::Juma)),
            Self::Baraka => Some(Self::Backdrop(Caller::Baraka)),
            Self::Hamisi => Some(Self::Backdrop(Caller::Hamisi)),
            _ => None,
        }
    }

    /// What fills the screen of a video call with this person: the design
    /// has a video frame of Baraka; for everyone else it is their portrait.
    pub const fn on_video(self) -> Self {
        match self {
            Self::Baraka => Self::BarakaVideo,
            other => other,
        }
    }
}

/// What stands for a person or a room in a round or square picture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Face {
    /// A photograph.
    Photo(Photo),
    /// The four-part picture of a group: three members and how many more.
    Group([Photo; 3], u32),
    /// No picture: the initials of this name.
    Initials(String),
}

impl Face {
    /// The face of someone the app has no picture of.
    pub fn of(name: &str) -> Self {
        Self::Initials(name.to_owned())
    }

    /// The photograph, if it is one.
    pub fn photo(&self) -> Option<Photo> {
        match self {
            Self::Photo(photo) => Some(*photo),
            _ => None,
        }
    }
}

// ----------------------------------------------------------------- people

/// Someone who can be called.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Person {
    /// Their address: who a call is to.
    pub identity: String,
    /// What they are called.
    pub name: String,
    /// Their picture.
    pub face: Face,
    /// Their app is open (the server saw it in the last half minute).
    pub online: bool,
    /// Unread messages from them in the conversation with the user (with
    /// a server; the scripted app counts on Home's rows only).
    pub unread: u32,
    /// A phone number, for a contact reached by phone rather than in the
    /// app (the design's Mzee Hamisi).
    pub phone: Option<String>,
}

// ------------------------------------------------------------------ rooms

/// What a message holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    /// Words.
    Text(String),
    /// A voice message of this many seconds. Only the design's sample has
    /// one: the server's chat is text.
    Voice(u32),
    /// A photograph with a line under it. Only the design's sample has one.
    Photo {
        /// The picture.
        photo: Photo,
        /// What it shows, for someone who cannot see it.
        alt: &'static str,
        /// The line under it.
        caption: String,
    },
}

/// How far one of the user's own messages has got.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Delivery {
    /// Someone else's message.
    Received,
    /// On its way to the server.
    Sending,
    /// Stored.
    Sent,
    /// Read by this many people (the scripted app, as the design shows).
    Seen(u32),
    /// It did not get there, and why.
    Failed(String),
}

/// One message of a room.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatMessage {
    /// The server's id; 0 for a message the server has not stored.
    pub id: u64,
    /// The app's own number for a message it sent, so that the answer to
    /// it, or a retry, finds it.
    pub local: u32,
    /// Who wrote it.
    pub author: String,
    /// Their picture.
    pub face: Face,
    /// The user wrote it.
    pub mine: bool,
    /// When, as the screen shows it ("07:41", "5 min").
    pub time: String,
    /// What it holds.
    pub body: Body,
    /// How far it has got, for the user's own.
    pub delivery: Delivery,
}

/// A line of a room's conversation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    /// Something a person wrote.
    Message(ChatMessage),
    /// A notice in the middle of the chat ("server · Rehema joins in 2
    /// minutes"). Only the design's sample has one.
    Notice(String),
}

/// How much of a room's conversation is here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Loaded {
    /// Not asked for yet.
    No,
    /// Asked for.
    Loading,
    /// Here.
    Yes,
    /// It could not be fetched, and why.
    Failed(String),
}

/// A room: an address for chat, voice and video.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Room {
    /// The room's address (`support-desk`).
    pub name: String,
    /// What it is called.
    pub title: String,
    /// The word under its ring on Home ("Support").
    pub short: String,
    /// Its picture.
    pub face: Face,
    /// The line under its name ("4 active", "3 members").
    pub status: String,
    /// Something happened in it lately: its ring on Home is the accent.
    pub live: bool,
    /// Its conversation, oldest first.
    pub entries: Vec<Entry>,
    /// How much of the conversation is here.
    pub loaded: Loaded,
    /// The largest message id the server has told of.
    pub last_id: u64,
    /// The id of the recording of this room that is running.
    pub recording: Option<String>,
    /// Who answers in the scripted app, if anyone.
    pub replier: Option<(String, Photo)>,
}

/// What the address of a conversation between two people starts with. A
/// room's address never does (`[a-z0-9-]` only), so the two cannot meet.
pub const DIRECT: char = '@';

/// The address, among the app's rooms, of the conversation with the person
/// `identity`: the same screen and the same list as a room, kept by the
/// server under `/v1/people/{identity}` instead.
pub fn direct_room(identity: &str) -> String {
    format!("{DIRECT}{identity}")
}

impl Room {
    /// The other person, when this is a conversation between two people
    /// that a server keeps; `None` for a room.
    pub fn peer(&self) -> Option<&str> {
        self.name.strip_prefix(DIRECT)
    }
}

// --------------------------------------------------------------- the list

/// What tapping a row of the Home list does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// Opens the room with this address.
    Room(String),
    /// Rings this person.
    Person {
        /// Who.
        identity: String,
        /// With video.
        video: bool,
    },
    /// Dials this number.
    Phone(String),
    /// The scripted app's incoming call: this person rings the user, as
    /// the design's first row leads to its Incoming page.
    Ring(String),
}

/// The small icon before a row's second line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// None: a message.
    None,
    /// A video call.
    Video,
    /// A phone call.
    Phone,
}

/// A row of the Home list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// What tapping it does.
    pub target: Target,
    /// Its first line.
    pub title: String,
    /// Its second line.
    pub preview: String,
    /// The icon before the second line.
    pub mark: Mark,
    /// When, as the screen shows it ("08:02", "Jana").
    pub when: String,
    /// Unread messages.
    pub unread: u32,
    /// Its picture.
    pub face: Face,
    /// A call that was not answered: it is listed under Notifications.
    pub missed: bool,
}

impl Item {
    /// True for a call of either kind, false for a conversation.
    pub fn is_call(&self) -> bool {
        self.mark != Mark::None
    }
}

/// Which rows the Home list shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Filter {
    /// Everything.
    #[default]
    All,
    /// Conversations.
    Rooms,
    /// Calls, in the app and to phones.
    Calls,
}

impl Filter {
    /// The three, in the design's order.
    pub const ALL: [Self; 3] = [Self::All, Self::Rooms, Self::Calls];

    /// The tab's label.
    pub const fn title(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Rooms => "Rooms",
            Self::Calls => "Calls",
        }
    }

    /// Whether `item` is shown under this tab.
    pub fn shows(self, item: &Item) -> bool {
        match self {
            Self::All => true,
            Self::Rooms => !item.is_call(),
            Self::Calls => item.is_call(),
        }
    }
}

// ------------------------------------------------------------------ calls

/// Who a call in the app is with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Subject {
    /// One person.
    Person(Person),
    /// Everyone in a room: its video.
    Room {
        /// The room's address.
        name: String,
        /// What it is called.
        title: String,
        /// Its picture.
        face: Face,
    },
}

impl Subject {
    /// The name on the call screen.
    pub fn title(&self) -> &str {
        match self {
            Self::Person(person) => &person.name,
            Self::Room { title, .. } => title,
        }
    }

    /// The picture on the call screen.
    pub fn face(&self) -> &Face {
        match self {
            Self::Person(person) => &person.face,
            Self::Room { face, .. } => face,
        }
    }
}

/// How far a call has got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallStatus {
    /// Asked for; the server has not answered yet.
    Starting,
    /// Ringing, on this side or the other.
    Ringing,
    /// Answered.
    Active,
}

/// The recording of a call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Recording {
    /// Not recording.
    Off,
    /// Asked the server to start.
    Starting,
    /// Recording; the server's id of it, when there is a server.
    On(Option<String>),
    /// Asked the server to stop the recording with this id.
    Stopping(String),
}

impl Recording {
    /// True while the screen says "Recording".
    pub fn is_on(&self) -> bool {
        matches!(self, Self::On(_) | Self::Stopping(_))
    }
}

/// The steps of the speaker's volume: the height of the filled part of
/// the 120 dp slider. The design shows the fourth.
pub const VOLUME_STEPS: [f32; 6] = [0.0, 26.0, 51.0, 76.0, 98.0, 120.0];

/// A call in the app: with a person, or the video of a room.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
    /// The app's own number for it: an answer for another call is dropped.
    pub serial: u32,
    /// Who it is with.
    pub subject: Subject,
    /// They rang the user.
    pub incoming: bool,
    /// With video.
    pub video: bool,
    /// How far it has got.
    pub status: CallStatus,
    /// The server's id of it.
    pub id: Option<String>,
    /// The ticket into the call's room: what its sound is joined with
    /// once the call is answered.
    pub ticket: Option<Ticket>,
    /// Seconds of talk.
    pub seconds: u32,
    /// Timer ticks since the last whole second, or since it began to ring.
    pub ticks: u32,
    /// The microphone is muted.
    pub muted: bool,
    /// The camera is on.
    pub camera: bool,
    /// The speaker's volume: an index into [`VOLUME_STEPS`].
    pub volume: usize,
    /// Its recording.
    pub recording: Recording,
    /// No server is part of it: the scripted app's call, with the
    /// design's pictures.
    pub scripted: bool,
}

/// How far a call to a phone has got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhoneStatus {
    /// Asked for; the server has not answered yet.
    Dialling,
    /// The other phone rings.
    Ringing,
    /// Answered.
    Active,
}

/// A call to a phone number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhoneCall {
    /// The app's own number for it.
    pub serial: u32,
    /// The number, as dialled (`+255712345678`).
    pub number: String,
    /// Whose it is, if the app knows.
    pub name: Option<String>,
    /// Their picture.
    pub face: Option<Face>,
    /// How far it has got.
    pub status: PhoneStatus,
    /// The server's id of it.
    pub id: Option<String>,
    /// The ticket into the call's room: what its sound is joined with.
    pub ticket: Option<Ticket>,
    /// Seconds of talk.
    pub seconds: u32,
    /// Timer ticks since the last whole second, or since it was dialled.
    pub ticks: u32,
    /// The number the other phone sees.
    pub caller_id: String,
    /// What it costs ("TSh 300 / minute").
    pub rate: String,
    /// The server can end it; if not, it ends when the other side hangs
    /// up and the button says "leave".
    pub can_hang_up: bool,
    /// The microphone is muted.
    pub muted: bool,
    /// The loudspeaker is on.
    pub speaker: bool,
    /// The keypad is open.
    pub keypad: bool,
    /// The keys pressed on it.
    pub keys: String,
    /// No server is part of it.
    pub scripted: bool,
}

// ------------------------------------------------------------------- time

/// `mm:ss`, or `h:mm:ss` from an hour on: a call's timer.
pub fn clock(seconds: u32) -> String {
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

/// `m:ss`: the length of a voice message.
pub fn short_clock(seconds: u32) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// A phone number in groups, as the design writes it: `+255 712 345 678`
/// for a Tanzanian number, groups of three otherwise. What is not a
/// number in international form is returned as it is.
pub fn pretty_number(number: &str) -> String {
    let Some(digits) = number.strip_prefix('+') else {
        return number.to_owned();
    };
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return number.to_owned();
    }
    let mut out = String::from("+");
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && index % 3 == 0 {
            out.push(' ');
        }
        out.push(digit);
    }
    out
}

/// The number a person typed, as the server takes it: `+` and digits. An
/// error says, in a sentence, what is wrong with it.
pub fn dialable(typed: &str) -> Result<String, &'static str> {
    let compact: String = typed.chars().filter(|c| !c.is_whitespace()).collect();
    let Some(digits) = compact.strip_prefix('+') else {
        return Err("Start with + and the country code: +255 712 345 678.");
    };
    if !digits.chars().all(|c| c.is_ascii_digit()) {
        return Err("A number is + and digits only.");
    }
    match digits.len() {
        0..=7 => Err("That number is too short."),
        8..=15 => Ok(compact),
        _ => Err("That number is too long."),
    }
}

/// Seconds since 1970 of `2026-11-15T07:00:00Z` (with or without a
/// fraction of a second). `None` for anything else.
pub fn parse_time(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    if bytes.len() < 20 || !text.is_ascii() || !text.ends_with('Z') {
        return None;
    }
    let number = |from: usize, to: usize| text.get(from..to)?.parse::<i64>().ok();
    let separators = [(4, b'-'), (7, b'-'), (10, b'T'), (13, b':'), (16, b':')];
    if separators.iter().any(|(at, byte)| bytes[*at] != *byte) {
        return None;
    }
    let (year, month, day) = (number(0, 4)?, number(5, 7)?, number(8, 10)?);
    let (hour, minute, second) = (number(11, 13)?, number(14, 16)?, number(17, 19)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 {
        return None;
    }
    // Days from the civil date (Howard Hinnant's algorithm).
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let shifted = (month + 9) % 12;
    let day_of_year = (153 * shifted + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    Some(days * 86_400 + hour * 3600 + minute * 60 + second.min(60))
}

/// The days of the week in the design's language, from Thursday (the day
/// 1970 began on).
const DAYS: [&str; 7] = [
    "Alhamisi", "Ijumaa", "Jumamosi", "Jumapili", "Jumatatu", "Jumanne", "Jumatano",
];

/// How long ago `at` was, seen from `now`, in the design's manner: "Sasa"
/// within a minute, "5 min", "3 h", "Jana" for the day before, the day of
/// the week within a week, a date after that. The app has no clock and no
/// time zone of its own, so this is said relative to the server's `now`
/// and never as a time of day. Empty when either is not a time.
pub fn ago(at: &str, now: &str) -> String {
    let (Some(at), Some(now)) = (parse_time(at), parse_time(now)) else {
        return String::new();
    };
    let passed = (now - at).max(0);
    let days = now.div_euclid(86_400) - at.div_euclid(86_400);
    if passed < 60 {
        "Sasa".to_owned()
    } else if passed < 3600 {
        format!("{} min", passed / 60)
    } else if passed < 86_400 && days <= 1 {
        format!("{} h", passed / 3600)
    } else if days <= 1 {
        "Jana".to_owned()
    } else if days < 7 {
        DAYS[at.div_euclid(86_400).rem_euclid(7) as usize].to_owned()
    } else {
        // Day and month of `at`, from its day number.
        let shifted = at.div_euclid(86_400) + 719_468;
        let day_of_era = shifted.rem_euclid(146_097);
        let year_of_era =
            (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let month_index = (5 * day_of_year + 2) / 153;
        let day = day_of_year - (153 * month_index + 2) / 5 + 1;
        let month = if month_index < 10 {
            month_index + 3
        } else {
            month_index - 9
        };
        format!("{day}/{month}")
    }
}

/// "12 min", "45 s", "1 h 5 min": how long a call lasted.
pub fn duration(seconds: u32) -> String {
    match seconds {
        0..=59 => format!("{seconds} s"),
        60..=3599 => format!("{} min", seconds / 60),
        _ => format!("{} h {} min", seconds / 3600, seconds / 60 % 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timers_read_as_the_design_writes_them() {
        assert_eq!(clock(745), "12:25");
        assert_eq!(clock(134), "02:14");
        assert_eq!(clock(3725), "1:02:05");
        assert_eq!(short_clock(24), "0:24");
    }

    #[test]
    fn numbers_are_grouped_and_checked() {
        assert_eq!(pretty_number("+255712345678"), "+255 712 345 678");
        assert_eq!(pretty_number("0712"), "0712");
        assert_eq!(dialable("+255 712 345 678"), Ok("+255712345678".to_owned()));
        assert!(dialable("0712345678").is_err());
        assert!(dialable("+2557").is_err());
        assert!(dialable("+25571234567890123").is_err());
        assert!(dialable("+255-712").is_err());
    }

    #[test]
    fn times_are_read_and_said_relatively() {
        assert_eq!(parse_time("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_time("2026-11-15T07:00:00Z"), Some(1_794_726_000));
        assert_eq!(parse_time("2026-11-15T07:00:00.123Z"), Some(1_794_726_000));
        assert_eq!(parse_time("yesterday"), None);
        assert_eq!(parse_time("2026-13-15T07:00:00Z"), None);
        let now = "2026-11-15T07:01:12Z";
        assert_eq!(ago("2026-11-15T07:01:00Z", now), "Sasa");
        assert_eq!(ago("2026-11-15T06:40:00Z", now), "21 min");
        assert_eq!(ago("2026-11-15T03:00:00Z", now), "4 h");
        assert_eq!(ago("2026-11-14T02:00:00Z", now), "Jana");
        // 2026-11-13 was a Friday.
        assert_eq!(ago("2026-11-13T09:00:00Z", now), "Ijumaa");
        assert_eq!(ago("2026-10-02T09:00:00Z", now), "2/10");
        assert_eq!(ago("soon", now), "");
        assert_eq!(duration(125), "2 min");
        assert_eq!(duration(40), "40 s");
    }
}
