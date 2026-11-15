//! Barua Talk end to end, driven the way a phone drives it, with no
//! server: the scripted app.
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

mod support;

use barua_talk::model::{
    Body, CallStatus, Delivery, Entry, Face, Filter, PhoneStatus, Recording, Subject,
};
use barua_talk::{ANSWER_TICKS, BaruaTalk, Link, NAME_KEY, Screen, Sound, TOAST_TICKS};
use morningstar::ui::Ui;
use morningstar::{App, Frame};

use support::{Device, design, phones};

fn last_message(device: &Device) -> barua_talk::model::ChatMessage {
    let room = device.app().room().expect("a room is open");
    match room.entries.last() {
        Some(Entry::Message(message)) => message.clone(),
        other => panic!("the room ends with {other:?}"),
    }
}

#[test]
fn the_first_run_asks_for_a_name_and_keeps_it() {
    for (phone, info) in phones() {
        let mut device = Device::launched(Ui::testing(BaruaTalk::default()), info, &[]);
        assert_eq!(device.screen(), Screen::Welcome, "{phone}");
        // No name, no entry.
        device.tap("continue");
        assert_eq!(device.screen(), Screen::Welcome, "{phone}");
        assert!(device.app().name_problem().is_some());
        device.fill("name-field", "  Amina Juma ");
        device.tap("continue");
        assert_eq!(device.screen(), Screen::Home, "{phone}");
        assert_eq!(device.app().name(), "Amina Juma");
        assert_eq!(device.ui.stored(NAME_KEY), Some("Amina Juma"));
        // Someone who is not the design's Neema has no photograph.
        assert_eq!(device.app().my_face(), Face::of("Amina Juma"));
        // Back does not return to the question.
        assert!(!device.back(), "{phone}");

        // The next launch goes straight in.
        let again = Device::launched(
            Ui::testing(BaruaTalk::default()),
            info,
            &[(NAME_KEY, "Amina Juma")],
        );
        assert_eq!(again.screen(), Screen::Home, "{phone}");
        assert_eq!(again.app().name(), "Amina Juma");
    }
}

#[test]
fn homes_tabs_filter_the_list_and_search_narrows_it() {
    for (phone, info) in phones() {
        let mut device = Device::on(Screen::Home, info);
        assert_eq!(device.app().shown_items(), [0, 1, 2, 3, 4], "{phone}");
        assert_eq!(device.app().unread(), 6, "the design's count beside All");
        device.tap(("filter", Filter::Rooms));
        assert_eq!(device.app().shown_items(), [0, 1, 4], "{phone}");
        device.tap(("filter", Filter::Calls));
        assert_eq!(device.app().shown_items(), [2, 3], "{phone}");
        assert!(
            !device.has(("item", 0usize)),
            "{phone}: a filtered row is gone"
        );
        device.tap(("filter", Filter::All));
        assert_eq!(device.app().shown_items().len(), 5);

        device.tap("search");
        assert_eq!(device.app().search(), Some(""));
        device.fill("query-field", "duka");
        assert_eq!(
            device.app().shown_items(),
            [1],
            "{phone}: one row mentions it"
        );
        device.type_text("zzz");
        assert!(device.app().shown_items().is_empty());
        assert!(device.has("empty"), "{phone}: an empty list says so");
        // Back closes the keyboard, then the search.
        device.back();
        if device.app().search().is_some() {
            device.back();
        }
        assert_eq!(device.app().search(), None, "{phone}");
        assert_eq!(device.app().shown_items().len(), 5);
    }
}

#[test]
fn a_room_opens_a_message_is_sent_seen_and_answered() {
    for (phone, info) in phones() {
        let mut device = Device::on(Screen::Home, info);
        device.tap(("item", 1usize));
        assert_eq!(device.screen(), Screen::Room, "{phone}");
        let room = device.app().room().unwrap();
        assert_eq!(
            (room.title.as_str(), room.entries.len()),
            ("Support desk", 4)
        );
        assert_eq!(device.app().unread(), 2, "{phone}: the room is read");

        // An empty message is not one.
        device.tap("send");
        assert_eq!(device.app().room().unwrap().entries.len(), 4);
        device.fill("draft-field", "Tukutane saa nne.");
        device.tap("send");
        assert_eq!(device.app().draft(), "");
        let mine = last_message(&device);
        assert!(mine.mine, "{phone}");
        assert_eq!(mine.body, Body::Text("Tukutane saa nne.".to_owned()));
        // Seen a second later, answered a second after that.
        device.ticks(10);
        let entries = &device.app().room().unwrap().entries;
        assert_eq!(entries.len(), 6, "{phone}: someone answered");
        assert!(matches!(
            &entries[4],
            Entry::Message(message) if message.delivery == Delivery::Seen(3)
        ));
        let answer = last_message(&device);
        assert_eq!(
            (answer.mine, answer.author.as_str()),
            (false, "Zawadi Mollel")
        );
        // The newest message is on the screen.
        assert!(
            device.ui.visible_rect_of(("entry", 5usize)).is_some(),
            "{phone}: the chat follows the newest message"
        );
        // The keyboard's Send sends too.
        device.fill("draft-field", "Asante");
        device.key(morningstar::TextInputEvent::Action(
            morningstar::ImeAction::Send,
        ));
        assert_eq!(last_message(&device).body, Body::Text("Asante".to_owned()));

        device.back();
        if device.screen() == Screen::Room {
            device.back();
        }
        assert_eq!(device.screen(), Screen::Home, "{phone}");
        // The room's row is on top, with what was said last.
        assert_eq!(device.app().items()[0].title, "Support desk");
        assert_eq!(device.app().items()[0].preview, "You: Asante");
    }
}

#[test]
fn a_voice_message_plays_and_stops() {
    let mut device = Device::on(Screen::Room, design());
    device.tap(("play", 0usize));
    assert!(device.app().playing().is_some());
    assert!(device.app().notice().is_some(), "it says there is no sound");
    device.ticks(8);
    assert_eq!(
        device.app().playing().map(|playing| playing.ticks >= 8),
        Some(true)
    );
    device.tap(("play", 0usize));
    assert!(device.app().playing().is_none());
    // The paperclip says what it does not do.
    device.ticks(TOAST_TICKS + 1);
    assert_eq!(device.app().notice(), None);
    device.tap("attach");
    assert!(device.app().notice().is_some());
}

#[test]
fn a_new_room_is_made_opened_and_written_in() {
    for (phone, info) in phones() {
        let mut device = Device::on(Screen::Home, info);
        device.tap("new-room");
        assert_eq!(device.screen(), Screen::NewRoom, "{phone}");
        device.tap("create-room");
        assert!(
            device.app().new_room().1.is_some(),
            "{phone}: a name is asked for"
        );
        assert_eq!(device.screen(), Screen::NewRoom);
        device.fill("room-name-field", "Wauzaji wa soko");
        device.tap("create-room");
        assert_eq!(device.screen(), Screen::Room, "{phone}");
        let room = device.app().room().unwrap();
        assert_eq!(
            (room.title.as_str(), room.name.as_str(), room.entries.len()),
            ("Wauzaji wa soko", "wauzaji-wa-soko", 0)
        );
        assert!(device.has("room-empty"), "{phone}: an empty room says so");
        device.fill("draft-field", "Karibuni");
        device.tap("send");
        assert_eq!(device.app().room().unwrap().entries.len(), 1);
        // Back leads home, not to the form.
        device.back();
        if device.screen() == Screen::Room {
            device.back();
        }
        assert_eq!(device.screen(), Screen::Home, "{phone}");
        assert_eq!(device.app().live_rooms().len(), 5);
        // Its ring is the first, and opens it again.
        device.tap(("ring", "wauzaji-wa-soko".to_owned()));
        assert_eq!(device.screen(), Screen::Room, "{phone}");
        assert_eq!(device.app().room().unwrap().title, "Wauzaji wa soko");
    }
}

#[test]
fn an_incoming_call_can_be_declined_answered_with_a_message_or_accepted() {
    for (phone, info) in phones() {
        // The design's first row leads to its Incoming page.
        let mut device = Device::on(Screen::Home, info);
        device.tap(("item", 0usize));
        assert_eq!(device.screen(), Screen::Incoming, "{phone}");
        let call = device.app().call().unwrap();
        assert!(call.incoming && call.video && call.status == CallStatus::Ringing);
        assert_eq!(call.subject.title(), "Baraka Mushi");
        device.tap("decline");
        assert_eq!(device.screen(), Screen::Home, "{phone}");
        assert!(device.app().call().is_none());
        // A missed call is on top of the list and under Notifications.
        assert!(device.app().items()[0].missed, "{phone}");
        assert!(device.app().notifications().contains(&0));

        // "Reply with a message instead" declines and opens his chat.
        device.tap(("item", 1usize));
        assert_eq!(device.screen(), Screen::Incoming, "{phone}");
        device.tap("reply-instead");
        assert_eq!(device.screen(), Screen::Room, "{phone}");
        assert_eq!(device.app().room().unwrap().title, "Baraka Mushi");
        assert!(device.app().call().is_none());
        device.back();
        if device.screen() == Screen::Room {
            device.back();
        }

        // Accept: the call's screen, its clock running.
        let row = device
            .app()
            .items()
            .iter()
            .position(|item| item.title == "Baraka Mushi" && !item.is_call())
            .unwrap();
        device.tap(("item", row));
        device.tap("accept");
        assert_eq!(device.screen(), Screen::VideoCall, "{phone}");
        assert_eq!(device.app().call().unwrap().status, CallStatus::Active);
        device.ticks(9);
        assert!(
            device.app().call().unwrap().seconds >= 2,
            "{phone}: the clock runs"
        );
        device.tap("hang-up");
        assert_eq!(device.screen(), Screen::Home, "{phone}");
        assert!(device.app().items()[0].preview.starts_with("Video call"));
    }
}

#[test]
fn a_call_is_made_answered_switched_and_ended() {
    for (phone, info) in phones() {
        let mut device = Device::on(Screen::Home, info);
        device.tap("tab-contacts");
        assert_eq!(device.screen(), Screen::People, "{phone}");
        // Dr. Asha Mrema, with video.
        device.tap(("person-video", 2usize));
        assert_eq!(device.screen(), Screen::Incoming, "{phone}");
        let call = device.app().call().unwrap();
        assert!(!call.incoming && call.scripted && call.status == CallStatus::Ringing);
        assert!(
            !device.has("accept"),
            "{phone}: a call one makes has one button"
        );
        device.ticks(ANSWER_TICKS + 1);
        assert_eq!(device.screen(), Screen::VideoCall, "{phone}: she answered");

        device.tap("mute");
        device.tap("camera");
        assert!(device.app().call().unwrap().muted);
        assert!(!device.app().call().unwrap().camera);
        device.tap("mute");
        assert!(!device.app().call().unwrap().muted);
        // Recording shows its pill and its price.
        assert!(!device.has("recording-pill"));
        device.tap("record");
        assert_eq!(device.app().call().unwrap().recording, Recording::On(None));
        assert!(device.has("recording-pill"), "{phone}");
        device.tap("record");
        assert_eq!(device.app().call().unwrap().recording, Recording::Off);
        // The volume goes up and down by steps, and stops at its ends.
        assert_eq!(device.app().call().unwrap().volume, 3);
        device.tap("volume-up");
        device.tap("volume-up");
        device.tap("volume-up");
        assert_eq!(device.app().call().unwrap().volume, 5);
        device.tap("volume-down");
        assert_eq!(device.app().call().unwrap().volume, 4);
        // What this version does not do is said, not faked.
        device.tap("share");
        assert_eq!(
            device.app().notice(),
            Some("Screen sharing needs the next version.")
        );
        device.tap("add-person");
        assert!(device.app().notice().unwrap().starts_with("Adding people"));
        // A scripted call is the design's frame: no line about sound,
        // and no live room behind it.
        assert!(!device.has("sound-note"));
        assert_eq!(device.app().sound_note(), None);
        assert_eq!(device.app().sound(), &Sound::Off);

        let before = device.app().call().unwrap().seconds;
        device.ticks(5);
        assert!(device.app().call().unwrap().seconds > before, "{phone}");
        device.tap("hang-up");
        assert_eq!(
            device.screen(),
            Screen::People,
            "{phone}: back where it began"
        );
        assert!(device.app().call().is_none());
        assert_eq!(device.app().items()[0].title, "Dr. Asha Mrema");

        // Someone whose app is closed does not answer.
        device.tap(("person-voice", 3usize));
        assert_eq!(device.screen(), Screen::Incoming, "{phone}");
        assert!(!device.app().call().unwrap().video);
        device.ticks(2 * ANSWER_TICKS + 1);
        assert_eq!(device.screen(), Screen::People, "{phone}");
        assert_eq!(device.app().notice(), Some("Juma Kileo did not answer."));
        // And a call can be given up while it rings.
        device.tap(("person-video", 0usize));
        device.tap("decline");
        assert_eq!(device.screen(), Screen::People, "{phone}");
        assert!(device.app().call().is_none());
    }
}

#[test]
fn a_number_is_dialled_from_home_and_the_call_has_its_keypad() {
    for (phone, info) in phones() {
        let mut device = Device::on(Screen::Home, info);
        device.tap("dial");
        assert_eq!(device.screen(), Screen::Dial, "{phone}");
        device.tap("dial-call");
        assert!(
            device.app().dial().1.is_some(),
            "{phone}: no number, no call"
        );
        for key in "+2557123".chars() {
            device.tap(("dial", key));
        }
        // A plus is only the first key; the pad's last key deletes.
        device.tap(("dial", '+'));
        assert_eq!(device.app().dial().0, "+2557123");
        device.tap(("dial", '<'));
        assert_eq!(device.app().dial().0, "+255712");
        device.tap("dial-call");
        assert_eq!(device.screen(), Screen::Dial, "{phone}: too short to call");
        assert!(device.app().dial().1.is_some());
        for key in "345678".chars() {
            device.tap(("dial", key));
        }
        device.tap("dial-call");
        assert_eq!(device.screen(), Screen::PhoneCall, "{phone}");
        let call = device.app().phone().unwrap();
        assert_eq!(call.status, PhoneStatus::Ringing);
        // The number is the design's Mzee Hamisi.
        assert_eq!(call.name.as_deref(), Some("Mzee Hamisi"));
        device.ticks(ANSWER_TICKS + 1);
        assert_eq!(device.app().phone().unwrap().status, PhoneStatus::Active);
        device.ticks(5);
        assert!(device.app().phone().unwrap().seconds >= 1, "{phone}");

        device.tap("mute");
        device.tap("speaker");
        let call = device.app().phone().unwrap();
        assert!(call.muted && call.speaker);
        device.tap("keypad");
        assert!(device.app().phone().unwrap().keypad);
        device.tap(("key", '1'));
        device.tap(("key", '#'));
        assert_eq!(device.app().phone().unwrap().keys, "1#");
        // Back closes the keypad before it ends the call.
        assert!(device.back());
        assert!(!device.app().phone().unwrap().keypad, "{phone}");
        assert_eq!(device.screen(), Screen::PhoneCall);

        device.tap("hang-up");
        // Home, not the dial pad.
        assert_eq!(device.screen(), Screen::Home, "{phone}");
        assert!(device.app().phone().is_none());
        assert!(device.app().items()[0].preview.starts_with("Phone call"));
    }
}

#[test]
fn the_designs_links_lead_where_the_design_says() {
    let mut device = Device::on(Screen::Home, design());
    // Page 01: `href="Call.dc.html"` on the video call's row.
    device.tap(("item", 2usize));
    assert_eq!(device.screen(), Screen::Incoming);
    device.ticks(ANSWER_TICKS + 1);
    assert_eq!(device.screen(), Screen::VideoCall);
    assert_eq!(
        device.app().call().unwrap().subject.title(),
        "Dr. Asha Mrema"
    );
    device.tap("hang-up");
    assert_eq!(device.screen(), Screen::Home);
    // `href="Dial.dc.html"` on the phone call's row: the Phone call page.
    let row = device
        .app()
        .items()
        .iter()
        .position(|item| item.title == "Mzee Hamisi")
        .unwrap();
    device.tap(("item", row));
    assert_eq!(device.screen(), Screen::PhoneCall);
    device.tap("back");
    assert_eq!(device.screen(), Screen::Home);
    // A ring of "Live rooms" and `href="Room.dc.html"`.
    device.tap(("ring", "support-desk".to_owned()));
    assert_eq!(device.screen(), Screen::Room);
    // Page 02: `href="Call.dc.html"`, and page 04's "Back to chat".
    device.tap("room-video");
    assert_eq!(device.screen(), Screen::VideoCall);
    assert!(matches!(
        device.app().call().unwrap().subject,
        Subject::Room { .. }
    ));
    device.tap("back");
    assert_eq!(device.screen(), Screen::Room);
    assert!(device.app().call().is_none(), "leaving the video ends it");
    // `href="Dial.dc.html"` from a room: a number to add is asked for.
    device.tap("room-phone");
    assert_eq!(device.screen(), Screen::Dial);
    device.tap("back");
    assert_eq!(device.screen(), Screen::Room);
    // `href="Main.dc.html"`.
    device.tap("back");
    assert_eq!(device.screen(), Screen::Home);

    // A design page opened by itself still leads home.
    for name in ["02-room", "03-incoming", "04-video-call", "05-phone-call"] {
        let app = BaruaTalk::designed(name).unwrap();
        let mut device = Device::with(Ui::new(app), design());
        assert!(device.idle(), "{name} is a still frame");
        let button = match name {
            "02-room" => "back",
            "03-incoming" => "decline",
            _ => "hang-up",
        };
        device.tap(button);
        assert_eq!(device.screen(), Screen::Home, "{name}");
    }
    // Page 03's "Accept" leads to page 04.
    let mut device = Device::with(
        Ui::new(BaruaTalk::designed("03-incoming").unwrap()),
        design(),
    );
    device.tap("accept");
    assert_eq!(device.screen(), Screen::VideoCall);
}

#[test]
fn the_bar_its_sheet_and_the_two_tabs_of_the_apps_own() {
    for (phone, info) in phones() {
        let mut device = Device::on(Screen::Home, info);
        device.tap("tab-notifications");
        assert_eq!(device.screen(), Screen::Notifications, "{phone}");
        // The two rows with unread messages.
        assert_eq!(device.app().notifications(), [0, 1]);
        device.tap(("item", 1usize));
        assert_eq!(device.screen(), Screen::Room, "{phone}");
        device.tap("back");
        assert_eq!(device.screen(), Screen::Notifications, "{phone}");
        assert_eq!(device.app().notifications(), [0], "{phone}: it was read");

        device.tap("tab-calls");
        assert_eq!(device.screen(), Screen::Home, "{phone}");
        assert_eq!(device.app().filter(), Filter::Calls);

        device.tap("start");
        assert!(device.app().start_open(), "{phone}");
        // Its scrim closes it, and so does Back.
        let scrim = device.rect("start-scrim");
        device.tap_at(morningstar::Point::new(
            scrim.x + scrim.width / 2.0,
            scrim.y + 200.0,
        ));
        assert!(!device.app().start_open(), "{phone}");
        device.tap("start");
        assert!(device.back());
        assert!(!device.app().start_open(), "{phone}");

        device.tap("start");
        device.tap("start-person");
        assert_eq!(device.screen(), Screen::People, "{phone}");
        assert!(device.back());
        assert_eq!(device.screen(), Screen::Home, "{phone}");
        device.tap("start");
        device.tap("start-room");
        assert_eq!(device.screen(), Screen::NewRoom, "{phone}");
        device.back();
        device.tap("start");
        device.tap("start-dial");
        assert_eq!(device.screen(), Screen::Dial, "{phone}");
        device.back();

        // A phone contact is called on their phone.
        device.tap("tab-contacts");
        device.tap(("person-phone", 4usize));
        assert_eq!(device.screen(), Screen::PhoneCall, "{phone}");
        device.tap("hang-up");
        assert_eq!(device.screen(), Screen::People, "{phone}");
        device.tap("tab-home");
        assert_eq!(device.screen(), Screen::Home, "{phone}");
        // On a tab's root Back is the system's.
        assert!(!device.back(), "{phone}");
    }
}

#[test]
fn the_name_and_the_servers_address_are_kept() {
    for (phone, info) in phones() {
        let mut device = Device::with(Ui::testing(BaruaTalk::on(Screen::Home)), info);
        device.tap("me");
        assert_eq!(device.screen(), Screen::Server, "{phone}");
        assert_eq!(device.app().name_draft(), "Neema");
        device.fill("name-field", " Mushi");
        device.tap("save-name");
        assert_eq!(device.app().name(), "Neema Mushi", "{phone}");
        assert_eq!(device.ui.stored(NAME_KEY), Some("Neema Mushi"));

        // An address that is not one is refused where it is typed.
        device.tap("server-url-field");
        device.key(morningstar::TextInputEvent::SelectAll);
        device.type_text("ftp://example");
        device.tap("server-check");
        assert!(device.app().server_problem().is_some(), "{phone}");
        assert_eq!(device.app().server_url(), barua_talk::api::DEFAULT_BASE_URL);
        device.tap("server-default");
        assert_eq!(device.app().server_problem(), None, "{phone}");
        assert_eq!(
            device.app().server_draft(),
            barua_talk::api::DEFAULT_BASE_URL
        );
        device.tap("back");
        assert_eq!(device.screen(), Screen::Home, "{phone}");
        assert_eq!(device.app().name(), "Neema Mushi");
    }
}

#[test]
fn the_app_is_idle_when_nothing_moves_and_standalone_without_a_host() {
    let mut device = Device::on(Screen::Home, design());
    // Nothing behind the app: its first question failed at once.
    assert_eq!(*device.app().link(), Link::Standalone);
    assert!(device.idle(), "home draws once and waits");
    device.tap(("filter", Filter::Calls));
    assert!(device.idle());
    // A toast ticks until it is gone, and then nothing does.
    device.tap(("filter", Filter::All));
    device.tap(("item", 1usize));
    device.tap("attach");
    assert!(!device.idle());
    device.ticks(TOAST_TICKS + 2);
    device.settle();
    assert_eq!(device.app().notice(), None);
    assert!(device.idle(), "the toast is gone and nothing moves");
}

#[test]
fn every_screen_fits_the_phones() {
    for (phone, info) in phones() {
        for screen in Screen::ALL {
            let mut ui = Ui::new(BaruaTalk::on(screen));
            let mut frame = Frame::for_app(&mut ui);
            frame.begin();
            ui.frame(&info, &mut frame);
            assert_eq!(ui.overflows(), [], "{phone}: {}", screen.name());
        }
    }
}
