//! Barua Talk with its server behind it, driven by fingers.
//!
//! The server is a stand-in (`support::World`) that answers the app's
//! requests as `server/API.md` describes, from state the test reads and
//! changes to play the other people: someone rings, someone answers,
//! someone writes. No network is used. What is checked is what the app
//! asks for, in which order, and what it shows of the answers, including
//! the ones that fail and the ones that come too late.

mod support;

use barua_talk::model::{CallStatus, Delivery, Entry, PhoneStatus, Recording};
use barua_talk::{
    BaruaTalk, Link, NO_MEDIA, POLL_ACTIVE, POLL_HOME, POLL_INCOMING, POLL_PHONE, POLL_RINGING,
    POLL_ROOM, Screen,
};

use support::{Device, Shared, design, galaxy_a27, lock, phones, world};

fn online(screen: Screen, world: &Shared) -> Device {
    let device = Device::online(screen, galaxy_a27(), world);
    assert!(device.app().link().is_online(), "{:?}", device.app().link());
    device
}

/// Lets time pass until `screen` is on top (a poll's worth at most).
fn until(device: &mut Device, screen: Screen) {
    for _ in 0..(POLL_ACTIVE + 6) {
        if device.screen() == screen {
            return;
        }
        device.ticks(1);
    }
    panic!("{screen:?} did not come; still on {:?}", device.screen());
}

fn messages(device: &Device) -> Vec<(String, Delivery)> {
    device
        .app()
        .room()
        .expect("a room is open")
        .entries
        .iter()
        .filter_map(|entry| match entry {
            Entry::Message(message) => match &message.body {
                barua_talk::model::Body::Text(text) => {
                    Some((text.clone(), message.delivery.clone()))
                }
                _ => None,
            },
            Entry::Notice(_) => None,
        })
        .collect()
}

#[test]
fn the_app_says_who_it_is_first_and_then_shows_the_servers_world() {
    for (phone, info) in phones() {
        let world = world();
        let device = Device::online(Screen::Home, info, &world);
        assert_eq!(
            *device.app().link(),
            Link::Online {
                talk: true,
                phone: true
            },
            "{phone}"
        );
        {
            let world = lock(&world);
            // Health, then the name (so that the identity is the name's),
            // then everything else.
            assert_eq!(world.log[0], "GET /v1/health");
            assert_eq!(world.log[1], "PUT /v1/me");
            assert_eq!(world.log[2], "GET /v1/rooms");
            assert_eq!(world.log[3], "GET /v1/activity");
            assert_eq!(world.me, Some(("neema".to_owned(), "Neema".to_owned())));
        }
        assert_eq!(device.app().identity(), Some("neema"));
        assert_eq!(device.app().greeting(), "Karibu");
        // None of the design's sample is left.
        let rooms: Vec<&str> = device
            .app()
            .rooms()
            .iter()
            .map(|room| room.title.as_str())
            .collect();
        assert_eq!(rooms, ["Support desk"], "{phone}");
        assert_eq!(device.app().items().len(), 1);
        assert_eq!(device.app().items()[0].preview, "Baraka Mushi: Karibu!");
        assert!(device.app().people().is_empty());
        assert!(device.has(("ring", "support-desk".to_owned())), "{phone}");
    }
}

#[test]
fn home_is_asked_again_on_the_timer_and_the_phone_asks_who_rings() {
    let world = world();
    let mut device = online(Screen::Home, &world);
    let (rooms, incoming) = {
        let world = lock(&world);
        (
            world.asked("GET /v1/rooms"),
            world.asked("/v1/calls/incoming"),
        )
    };
    lock(&world).rooms[0].display_name = "Support desk 2".to_owned();
    device.ticks(POLL_HOME + POLL_INCOMING + 4);
    let world = lock(&world);
    assert!(world.asked("GET /v1/rooms") > rooms, "home was asked again");
    assert!(world.asked("/v1/calls/incoming") > incoming + 1);
    assert_eq!(device.app().rooms()[0].title, "Support desk 2");
    // Polls do not pile up: at most one in ten ticks for the ring.
    assert!(world.asked("/v1/calls/incoming") <= incoming + 5);
}

#[test]
fn a_room_is_read_written_in_and_followed() {
    for (phone, info) in phones() {
        let world = world();
        let mut device = Device::online(Screen::Home, info, &world);
        device.tap(("item", 0usize));
        assert_eq!(device.screen(), Screen::Room, "{phone}");
        device.ticks(2);
        assert_eq!(
            messages(&device),
            [("Karibu!".to_owned(), Delivery::Received)],
            "{phone}"
        );
        assert!(
            lock(&world)
                .log
                .iter()
                .any(|line| line == "GET /v1/rooms/support-desk/messages?read=1")
        );

        device.fill("draft-field", "Asante, nakuja.");
        device.tap("send");
        device.ticks(2);
        assert_eq!(
            messages(&device)[1],
            ("Asante, nakuja.".to_owned(), Delivery::Sent),
            "{phone}"
        );
        {
            let mut world = lock(&world);
            let stored = &world.room("support-desk").unwrap().messages;
            assert_eq!(stored.len(), 2);
            assert!(stored[1].client_id.starts_with("m-"), "{phone}");
            // Someone answers.
            world.say("support-desk", "baraka", "Baraka Mushi", "Sawa, karibu.");
        }
        device.ticks(POLL_ROOM + 3);
        assert_eq!(messages(&device).len(), 3, "{phone}: the poll brought it");
        assert_eq!(messages(&device)[2].0, "Sawa, karibu.");
        // The poll asks only for what is new.
        assert!(
            lock(&world)
                .log
                .iter()
                .any(|line| line == "GET /v1/rooms/support-desk/messages?after=41&read=1")
        );
        // Its own message is not shown twice.
        assert_eq!(
            messages(&device)
                .iter()
                .filter(|(text, _)| text == "Asante, nakuja.")
                .count(),
            1
        );
    }
}

#[test]
fn a_message_that_fails_says_so_and_is_sent_again_once() {
    let world = world();
    let mut device = online(Screen::Home, &world);
    device.tap(("item", 0usize));
    device.ticks(2);
    device.fill("draft-field", "Tutaonana!");
    // The next request about messages fails. That may be the room's poll
    // rather than the post, depending on the tick: then the post goes
    // through, and there is nothing to send again.
    lock(&world).fail = Some((
        "/messages",
        502,
        "upstream_failed",
        "Barua Talk did not answer.",
    ));
    device.tap("send");
    device.ticks(2);
    let sent = messages(&device);
    let failed = sent
        .iter()
        .find(|(text, _)| text == "Tutaonana!")
        .expect("the message stays on the screen");
    let Delivery::Failed(why) = &failed.1 else {
        // A poll took the failure instead of the post: the message went.
        assert_eq!(failed.1, Delivery::Sent);
        return;
    };
    assert_eq!(why, "Barua Talk did not answer.");
    assert_eq!(lock(&world).room("support-desk").unwrap().messages.len(), 1);
    let local = device
        .app()
        .room()
        .unwrap()
        .entries
        .iter()
        .find_map(|entry| match entry {
            Entry::Message(message) if message.mine => Some(message.local),
            _ => None,
        })
        .unwrap();
    device.tap(("retry", local));
    device.ticks(2);
    assert_eq!(
        messages(&device).last().unwrap(),
        &("Tutaonana!".to_owned(), Delivery::Sent)
    );
    let mut world = lock(&world);
    let stored = &world.room("support-desk").unwrap().messages;
    assert_eq!(stored.len(), 2, "stored once");
    // The retry carried the first attempt's id.
    assert_eq!(world.asked("POST /v1/rooms/support-desk/messages"), 2);
}

#[test]
fn a_room_is_made_on_the_server() {
    let world = world();
    let mut device = online(Screen::Home, &world);
    device.tap("new-room");
    device.fill("room-name-field", "Wauzaji");
    device.tap("create-room");
    device.ticks(3);
    assert_eq!(device.screen(), Screen::Room);
    assert_eq!(device.app().room().unwrap().name, "wauzaji");
    assert_eq!(lock(&world).rooms[0].display_name, "Wauzaji");
    assert!(device.has("room-empty"));
    device.tap("back");
    assert_eq!(device.screen(), Screen::Home, "not back to the form");

    // A server with no key says why nothing was made.
    lock(&world).talk = false;
    device.tap("new-room");
    device.fill("room-name-field", "Kliniki");
    device.tap("create-room");
    device.ticks(3);
    assert_eq!(device.screen(), Screen::NewRoom);
    assert_eq!(
        device.app().new_room().1,
        Some("The server has no Barua key.")
    );
}

#[test]
fn someone_rings_and_the_call_is_accepted_recorded_and_ended_by_them() {
    for (phone, info) in phones() {
        let world = world();
        let mut device = Device::online(Screen::Home, info, &world);
        lock(&world).ring("c9", "baraka", "Baraka Mushi", true);
        device.ticks(POLL_INCOMING + 3);
        assert_eq!(
            device.screen(),
            Screen::Incoming,
            "{phone}: the phone rings"
        );
        let call = device.app().call().unwrap();
        assert!(call.incoming && !call.scripted);
        assert_eq!(call.subject.title(), "Baraka Mushi");

        device.tap("accept");
        device.ticks(2);
        assert_eq!(device.screen(), Screen::VideoCall, "{phone}");
        assert_eq!(lock(&world).call("c9").status, "active");
        let call = device.app().call().unwrap();
        assert_eq!(call.status, CallStatus::Active);
        // The ticket is kept for a media service that is not there yet,
        // and the screen says what is missing rather than pretending.
        assert_eq!(call.ticket.as_ref().unwrap().token, "callee-ticket");
        assert!(device.has("no-media"), "{phone}: {NO_MEDIA}");

        device.tap("record");
        device.ticks(2);
        assert_eq!(
            device.app().call().unwrap().recording,
            Recording::On(Some("rec-1".to_owned()))
        );
        assert_eq!(lock(&world).call("c9").recording.as_deref(), Some("rec-1"));
        assert!(device.has("recording-pill"), "{phone}");
        device.tap("record");
        device.ticks(2);
        assert_eq!(device.app().call().unwrap().recording, Recording::Off);
        assert_eq!(lock(&world).call("c9").recording, None);

        // The server's count of seconds wins over the app's.
        lock(&world).call("c9").seconds = 75;
        device.ticks(POLL_ACTIVE + 3);
        assert!(device.app().call().unwrap().seconds >= 75, "{phone}");
        // The other side hangs up.
        {
            let mut world = lock(&world);
            let call = world.call("c9");
            call.status = "ended";
            call.seconds = 80;
        }
        until(&mut device, Screen::Home);
        assert_eq!(device.app().notice(), Some("Call ended \u{b7} 1 min"));
        // It does not ring again.
        device.ticks(POLL_INCOMING + 3);
        assert_eq!(device.screen(), Screen::Home, "{phone}");
    }
}

#[test]
fn a_ringing_call_can_be_declined_or_missed() {
    let world = world();
    let mut device = online(Screen::Home, &world);
    lock(&world).ring("c1", "baraka", "Baraka Mushi", false);
    device.ticks(POLL_INCOMING + 3);
    assert_eq!(device.screen(), Screen::Incoming);
    device.tap("decline");
    device.ticks(2);
    assert_eq!(device.screen(), Screen::Home);
    assert_eq!(lock(&world).call("c1").status, "declined");

    // The caller gives up while it rings.
    lock(&world).ring("c2", "baraka", "Baraka Mushi", true);
    device.ticks(POLL_INCOMING + 3);
    assert_eq!(device.screen(), Screen::Incoming);
    lock(&world).call("c2").status = "missed";
    until(&mut device, Screen::Home);
    assert_eq!(
        device.app().notice(),
        Some("Missed call from Baraka Mushi.")
    );

    // "Reply with a message instead" declines, and says that there is no
    // conversation between two people on the server.
    lock(&world).ring("c3", "baraka", "Baraka Mushi", true);
    device.ticks(POLL_INCOMING + 3);
    device.tap("reply-instead");
    device.ticks(2);
    assert_eq!(device.screen(), Screen::Home);
    assert_eq!(lock(&world).call("c3").status, "declined");
    assert!(device.app().notice().unwrap().starts_with("Declined."));

    // Accepting a call the caller has just given up says so.
    lock(&world).ring("c4", "baraka", "Baraka Mushi", true);
    device.ticks(POLL_INCOMING + 3);
    assert_eq!(device.screen(), Screen::Incoming);
    lock(&world).call("c4").status = "missed";
    device.tap("accept");
    device.ticks(2);
    assert_eq!(device.screen(), Screen::Home);
    assert_eq!(device.app().notice(), Some("The caller hung up first."));
}

#[test]
fn a_person_is_called_and_answers_declines_or_is_given_up_on() {
    let world = world();
    let mut device = online(Screen::Home, &world);
    device.tap("tab-contacts");
    device.ticks(2);
    assert_eq!(device.app().people().len(), 1);
    assert!(device.app().people()[0].online);

    device.tap(("person-video", 0usize));
    assert_eq!(device.screen(), Screen::Incoming);
    device.ticks(2);
    let call = device.app().call().unwrap();
    assert_eq!(call.id.as_deref(), Some("call-1"));
    assert_eq!(call.status, CallStatus::Ringing);
    assert_eq!(call.ticket.as_ref().unwrap().token, "caller-ticket");
    // Nobody answers by script here: it rings until the server says so.
    device.ticks(POLL_RINGING + 3);
    assert_eq!(device.screen(), Screen::Incoming);
    lock(&world).call("call-1").status = "active";
    device.ticks(POLL_RINGING + 3);
    assert_eq!(device.screen(), Screen::VideoCall);
    device.tap("hang-up");
    device.ticks(2);
    assert_eq!(device.screen(), Screen::People);
    assert_eq!(lock(&world).call("call-1").status, "ended");

    // Declined.
    device.tap(("person-voice", 0usize));
    device.ticks(2);
    lock(&world).call("call-2").status = "declined";
    until(&mut device, Screen::People);
    assert_eq!(device.app().notice(), Some("Baraka Mushi declined."));

    // Given up on while it rings: the server is told.
    device.tap(("person-video", 0usize));
    device.ticks(2);
    device.tap("decline");
    device.ticks(2);
    assert_eq!(lock(&world).call("call-3").status, "missed");

    // Given up on before the server has even answered: the call is hung
    // up when its answer comes.
    lock(&world).hold = Some("/v1/calls");
    device.tap(("person-video", 0usize));
    device.ticks(1);
    assert_eq!(device.app().call().unwrap().status, CallStatus::Starting);
    device.tap("decline");
    assert_eq!(device.screen(), Screen::People);
    lock(&world).hold = None;
    // The held request is answered by hand, as the server would have.
    let answer = morningstar::HttpResponse::new(201).with_body(
        br#"{"call": {"id": "late-1", "direction": "outgoing", "video": true, "status": "ringing",
            "peer": {"identity": "baraka", "name": "Baraka Mushi"}, "seconds": 0, "recording": null},
            "token": "t", "url": "wss://talk.example"}"#
            .to_vec(),
    );
    assert!(device.ui.answer_http(Ok(answer)));
    device.ticks(2);
    assert_eq!(device.screen(), Screen::People);
    assert!(device.app().call().is_none());
    assert_eq!(lock(&world).asked("POST /v1/calls/late-1/hangup"), 1);
}

#[test]
fn a_rooms_video_takes_a_ticket_and_records_the_room() {
    let world = world();
    let mut device = online(Screen::Home, &world);
    device.tap(("item", 0usize));
    device.ticks(2);
    device.tap("room-video");
    device.ticks(2);
    assert_eq!(device.screen(), Screen::VideoCall);
    let call = device.app().call().unwrap();
    assert_eq!(call.ticket.as_ref().unwrap().token, "room-ticket");
    assert!(device.has("no-media"));
    device.tap("record");
    device.ticks(2);
    assert_eq!(
        lock(&world)
            .room("support-desk")
            .unwrap()
            .recording
            .as_deref(),
        Some("rec-1")
    );
    device.tap("back");
    assert_eq!(device.screen(), Screen::Room, "back to chat");

    // The room is still being recorded: its video shows so at once.
    device.tap("room-video");
    device.ticks(2);
    assert!(device.app().call().unwrap().recording.is_on());
    // A stop the server does not take leaves it recording, and says so.
    lock(&world).fail = Some((
        "/stop",
        502,
        "upstream_failed",
        "Barua Talk did not answer.",
    ));
    device.tap("record");
    device.ticks(2);
    assert!(device.app().call().unwrap().recording.is_on());
    assert!(
        device
            .app()
            .notice()
            .unwrap()
            .starts_with("The recording may still be running.")
    );
    device.tap("record");
    device.ticks(2);
    assert_eq!(device.app().call().unwrap().recording, Recording::Off);
    // A recording the server refuses is said, and nothing is shown as
    // recording.
    lock(&world).fail = Some((
        "/v1/recordings",
        402,
        "balance_empty",
        "The workspace balance is empty. Top up, then try again.",
    ));
    device.tap("record");
    device.ticks(2);
    assert_eq!(device.app().call().unwrap().recording, Recording::Off);
    assert_eq!(
        device.app().notice(),
        Some("Not recording. The workspace balance is empty. Top up, then try again.")
    );
}

#[test]
fn a_phone_is_dialled_once_watched_and_left() {
    let world = world();
    let mut device = online(Screen::Home, &world);
    device.tap("dial");
    for key in "+255712345678".chars() {
        device.tap(("dial", key));
    }
    device.tap("dial-call");
    assert_eq!(device.screen(), Screen::PhoneCall);
    device.ticks(2);
    let call = device.app().phone().unwrap();
    assert_eq!(call.status, PhoneStatus::Ringing);
    assert_eq!(call.caller_id, "+255 800 110 011");
    assert!(!call.can_hang_up && !call.scripted);
    assert_eq!(call.ticket.as_ref().unwrap().token, "phone-ticket");
    assert!(device.has("no-media"));
    {
        let mut world = lock(&world);
        assert_eq!(world.phones.len(), 1, "dialled once");
        assert!(world.phones[0].request_id.starts_with("dial-"));
        world.phones[0].status = "active";
        world.phones[0].seconds = 5;
    }
    device.ticks(POLL_PHONE + 3);
    let call = device.app().phone().unwrap();
    assert_eq!(call.status, PhoneStatus::Active);
    assert!(call.seconds >= 5);
    // The server cannot end a phone call: the app says so and leaves.
    device.tap("hang-up");
    device.ticks(2);
    assert_eq!(device.screen(), Screen::Home);
    assert_eq!(
        device.app().notice(),
        Some("The call ends when the other side hangs up.")
    );
    assert_eq!(lock(&world).asked("POST /v1/phone-calls/phone-1/hangup"), 1);

    // A second press is a second request id.
    device.tap("dial");
    for key in "+255712345678".chars() {
        device.tap(("dial", key));
    }
    device.tap("dial-call");
    device.ticks(2);
    {
        let mut world = lock(&world);
        assert_eq!(world.phones.len(), 2);
        assert_ne!(world.phones[0].request_id, world.phones[1].request_id);
        // The other side ends it: its length and its price are said.
        world.phones[1].status = "ended";
        world.phones[1].seconds = 125;
        world.phones[1].price = Some(900);
    }
    until(&mut device, Screen::Home);
    assert_eq!(
        device.app().notice(),
        Some("Call ended \u{b7} 2 min \u{b7} TSh 900")
    );

    // A server with phone calls switched off says so on the dial pad.
    lock(&world).phone = false;
    device.tap("dial");
    for key in "+255712345678".chars() {
        device.tap(("dial", key));
    }
    device.tap("dial-call");
    device.ticks(2);
    assert_eq!(device.screen(), Screen::Dial);
    assert_eq!(
        device.app().dial().1,
        Some("Phone calls are switched off on this server.")
    );
    assert_eq!(lock(&world).phones.len(), 2);
}

#[test]
fn a_row_of_a_phone_call_fills_the_dial_pad_and_does_not_dial() {
    let world = world();
    lock(&world).phones.push(support::StoredPhone {
        id: "old".to_owned(),
        to: "+255712345678".to_owned(),
        status: "ended",
        seconds: 75,
        price: Some(600),
        request_id: "dial-old-1".to_owned(),
    });
    let mut device = online(Screen::Home, &world);
    let row = device
        .app()
        .items()
        .iter()
        .position(|item| item.title == "+255 712 345 678")
        .expect("the phone call's row");
    device.tap(("item", row));
    // A phone call costs money: it takes a press of its own.
    assert_eq!(device.screen(), Screen::Dial);
    assert_eq!(device.app().dial().0, "+255712345678");
    assert_eq!(lock(&world).asked("POST /v1/phone-calls"), 0);
}

#[test]
fn without_its_server_the_app_is_the_scripted_one_and_comes_back() {
    let world = world();
    lock(&world).down = true;
    let mut device = Device::online(Screen::Home, design(), &world);
    assert!(matches!(device.app().link(), Link::Offline(_)));
    // The design's own people and rooms, working.
    assert_eq!(device.app().items().len(), 5);
    assert_eq!(device.app().greeting(), "Habari za asubuhi");
    device.tap(("item", 1usize));
    assert_eq!(device.screen(), Screen::Room);
    device.fill("draft-field", "Habari");
    device.tap("send");
    device.ticks(10);
    assert_eq!(device.app().room().unwrap().entries.len(), 6);

    // The server comes back: reaching Home asks again.
    lock(&world).down = false;
    device.tap("back");
    if device.screen() == Screen::Room {
        device.tap("back");
    }
    device.ticks(6);
    assert!(device.app().link().is_online());
    assert_eq!(device.app().items().len(), 1, "the server's list");

    // And goes away in the middle of a room: the app falls back.
    device.tap(("item", 0usize));
    device.ticks(2);
    assert_eq!(device.screen(), Screen::Room);
    lock(&world).down = true;
    device.ticks(POLL_ROOM + 3);
    assert!(matches!(device.app().link(), Link::Offline(_)));
    assert_eq!(device.screen(), Screen::Home, "the server's room is gone");
    assert_eq!(device.app().items().len(), 5, "the scripted list is back");
}

#[test]
fn the_server_screen_checks_renames_and_changes_the_address() {
    let world = world();
    let mut device = online(Screen::Home, &world);
    device.tap("me");
    assert_eq!(device.screen(), Screen::Server);
    assert!(device.has("identity"));
    device.fill("name-field", " Mushi");
    device.tap("save-name");
    device.ticks(2);
    assert_eq!(
        lock(&world).me,
        Some(("neema".to_owned(), "Neema Mushi".to_owned())),
        "the name changes, the identity does not"
    );
    assert_eq!(device.app().notice(), Some("Name saved."));

    let checks = lock(&world).asked("/v1/health");
    device.tap("server-check");
    device.ticks(6);
    assert_eq!(lock(&world).asked("/v1/health"), checks + 1);
    assert!(device.app().link().is_online());
    assert_eq!(
        device.screen(),
        Screen::Server,
        "a check does not leave the screen"
    );

    // A server whose database is down is no server.
    lock(&world).database = false;
    device.tap("server-check");
    device.ticks(6);
    assert!(matches!(device.app().link(), Link::Offline(_)));
    assert_eq!(device.app().items().len(), 5);
}

#[test]
fn a_test_against_a_real_server_is_another_person() {
    let world = world();
    let app = BaruaTalk::on(Screen::Home).with_device_prefix("test_");
    let device = Device::with(support::serve(app, &world), design());
    assert!(device.app().device_id().starts_with("test_"));
}
