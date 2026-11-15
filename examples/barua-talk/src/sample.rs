//! The scripted app's world: the design's own people, rooms and messages.
//!
//! With no server behind the app this is what it shows and works on, end
//! to end: the rooms can be opened and written in, someone answers, people
//! can be called and "answer". Nothing here leaves the phone. With a
//! server none of it is shown: the rooms, people and calls are the
//! server's.

use crate::model::{
    Body, ChatMessage, Delivery, Entry, Face, Item, Loaded, Mark, Person, Photo, Room, Target,
};

/// The user of the design.
pub const ME: &str = "Neema";
/// The number the design's phone call is to.
pub const HAMISI_NUMBER: &str = "+255712345678";
/// The number the other phone sees (`Caller ID` on page 05).
pub const CALLER_ID: &str = "+255 800 110 011";
/// What a phone call costs (page 05).
pub const PHONE_RATE: &str = "TSh 300 / minute";
/// What a recording costs, as the pill of page 04 writes it.
pub const RECORDING_RATE: &str = "TSh 30 / min";
/// What the scripted app answers a message with.
pub const SCRIPTED_REPLY: &str = "Sawa, nimepata.";

fn person(identity: &str, name: &str, photo: Photo, online: bool) -> Person {
    Person {
        identity: identity.to_owned(),
        name: name.to_owned(),
        face: Face::Photo(photo),
        online,
        unread: 0,
        phone: None,
    }
}

/// The people of the design, in the order the People screen lists them.
pub fn people() -> Vec<Person> {
    vec![
        person("baraka", "Baraka Mushi", Photo::Baraka, true),
        person("zawadi", "Zawadi Mollel", Photo::Zawadi, true),
        person("asha", "Dr. Asha Mrema", Photo::Asha, true),
        person("juma", "Juma Kileo", Photo::Juma, false),
        Person {
            phone: Some(HAMISI_NUMBER.to_owned()),
            ..person("hamisi", "Mzee Hamisi", Photo::Hamisi, false)
        },
    ]
}

fn theirs(author: &str, photo: Photo, time: &str, body: Body) -> Entry {
    Entry::Message(ChatMessage {
        id: 0,
        local: 0,
        author: author.to_owned(),
        face: Face::Photo(photo),
        mine: false,
        time: time.to_owned(),
        body,
        delivery: Delivery::Received,
    })
}

fn said(author: &str, photo: Photo, time: &str, words: &str) -> Entry {
    theirs(author, photo, time, Body::Text(words.to_owned()))
}

fn mine(time: &str, words: &str, seen: u32) -> Entry {
    Entry::Message(ChatMessage {
        id: 0,
        local: 0,
        author: ME.to_owned(),
        face: Face::Photo(Photo::Neema),
        mine: true,
        time: time.to_owned(),
        body: Body::Text(words.to_owned()),
        delivery: Delivery::Seen(seen),
    })
}

fn room(
    name: &str,
    title: &str,
    short: &str,
    photo: Photo,
    (status, live): (&str, bool),
    replier: Option<(&str, Photo)>,
    entries: Vec<Entry>,
) -> Room {
    Room {
        name: name.to_owned(),
        title: title.to_owned(),
        short: short.to_owned(),
        face: Face::Photo(photo),
        status: status.to_owned(),
        live,
        entries,
        loaded: Loaded::Yes,
        last_id: 0,
        recording: None,
        replier: replier.map(|(name, photo)| (name.to_owned(), photo)),
    }
}

/// The design's rooms. The first four are the rings of Home's "Live
/// rooms"; the fifth is the conversation with Baraka that "Reply with a
/// message instead" opens.
pub fn rooms() -> Vec<Room> {
    vec![
        // Page 02, message for message.
        room(
            "support-desk",
            "Support desk",
            "Support",
            Photo::Support,
            ("4 active", true),
            Some(("Zawadi Mollel", Photo::Zawadi)),
            vec![
                theirs("Baraka Mushi", Photo::Baraka, "07:41", Body::Voice(24)),
                theirs(
                    "Zawadi Mollel",
                    Photo::Zawadi,
                    "07:48",
                    Body::Photo {
                        photo: Photo::Shop,
                        alt: "A small shop lit with neon at night, a bajaji parked in front",
                        caption: "Picha ya duka jana usiku".to_owned(),
                    },
                ),
                Entry::Notice("server \u{b7} Rehema joins in 2 minutes".to_owned()),
                mine("07:52", "Asante Zawadi! Tuanze video sasa.", 3),
            ],
        ),
        room(
            "clinic",
            "Clinic",
            "Clinic",
            Photo::Asha,
            ("2 active", true),
            Some(("Dr. Asha Mrema", Photo::Asha)),
            vec![said(
                "Dr. Asha Mrema",
                Photo::Asha,
                "Jana",
                "Karibu kliniki. Tunafungua saa mbili asubuhi.",
            )],
        ),
        room(
            "duka",
            "Duka",
            "Duka",
            Photo::Zawadi,
            ("3 active", true),
            Some(("Zawadi Mollel", Photo::Zawadi)),
            vec![said(
                "Zawadi Mollel",
                Photo::Zawadi,
                "Jana",
                "Mzigo mpya umefika leo.",
            )],
        ),
        room(
            "juma",
            "Juma Kileo",
            "Juma",
            Photo::Juma,
            ("Last seen Ijumaa", false),
            Some(("Juma Kileo", Photo::Juma)),
            vec![
                mine("Ijumaa", "Tukutane kesho saa nne?", 1),
                said("Juma Kileo", Photo::Juma, "Ijumaa", "Sawa, tutaonana kesho"),
            ],
        ),
        room(
            "baraka",
            "Baraka Mushi",
            "Baraka",
            Photo::Baraka,
            ("Active now", true),
            Some(("Baraka Mushi", Photo::Baraka)),
            vec![
                said(
                    "Baraka Mushi",
                    Photo::Baraka,
                    "07:58",
                    "Habari za asubuhi Neema",
                ),
                said(
                    "Baraka Mushi",
                    Photo::Baraka,
                    "08:02",
                    "Nipigie ukipata nafasi",
                ),
            ],
        ),
    ]
}

/// How many of [`rooms`] have a ring on Home (the design draws four).
pub const LIVE_ROOMS: usize = 4;

/// Home's list as the design shows it (page 01), row for row.
pub fn items() -> Vec<Item> {
    vec![
        Item {
            // `href="Incoming.dc.html"`: the design's way to its Incoming
            // page. In the scripted app Baraka rings when his row is
            // tapped.
            target: Target::Ring("baraka".to_owned()),
            title: "Baraka Mushi".to_owned(),
            preview: "Nipigie ukipata nafasi".to_owned(),
            mark: Mark::None,
            when: "08:02".to_owned(),
            unread: 2,
            face: Face::Photo(Photo::Baraka),
            missed: false,
        },
        Item {
            target: Target::Room("support-desk".to_owned()),
            title: "Support desk".to_owned(),
            preview: "Zawadi: Picha ya duka jana usiku".to_owned(),
            mark: Mark::None,
            when: "07:48".to_owned(),
            unread: 4,
            face: Face::Group([Photo::Support, Photo::Zawadi, Photo::Baraka], 1),
            missed: false,
        },
        Item {
            target: Target::Person {
                identity: "asha".to_owned(),
                video: true,
            },
            title: "Dr. Asha Mrema".to_owned(),
            preview: "Video call \u{b7} 12 min".to_owned(),
            mark: Mark::Video,
            when: "Jana".to_owned(),
            unread: 0,
            face: Face::Photo(Photo::Asha),
            missed: false,
        },
        Item {
            target: Target::Phone(HAMISI_NUMBER.to_owned()),
            title: "Mzee Hamisi".to_owned(),
            preview: "Phone call \u{b7} +255 712 345 678".to_owned(),
            mark: Mark::Phone,
            when: "Jana".to_owned(),
            unread: 0,
            face: Face::Photo(Photo::Hamisi),
            missed: false,
        },
        Item {
            target: Target::Room("juma".to_owned()),
            title: "Juma Kileo".to_owned(),
            preview: "Sawa, tutaonana kesho".to_owned(),
            mark: Mark::None,
            when: "Ijumaa".to_owned(),
            unread: 0,
            face: Face::Photo(Photo::Juma),
            missed: false,
        },
    ]
}
