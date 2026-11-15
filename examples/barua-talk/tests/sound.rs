//! The sound of a real call: the live room (ADR 0038) the app joins with
//! the ticket its server hands out, driven by fingers, with the stand-in
//! server of `support` behind the app and this test playing the room:
//! "it connected", "Baraka came in", "the connection was lost", "the user
//! refused the microphone".
//!
//! What is checked is when the app joins and with what, that the mute and
//! loudspeaker buttons act on the room, that every way a call ends leaves
//! it, and that the screen says what is true. No network, no microphone
//! and no LiveKit are used: what a phone does with the same requests is
//! not tested here (`SCREENS.md`, "The test on a phone").

mod support;

use barua_talk::model::{CallStatus, PhoneStatus};
use barua_talk::{POLL_INCOMING, POLL_PHONE, POLL_RINGING, Screen, Sound, sound};
use morningstar::ui::LiveRoom;
use morningstar::{
    LiveDisconnect, LiveError, LiveEvent, LiveParticipant, ServiceRequest, ServiceResult,
};

use support::{Device, Shared, design, galaxy_a27, lock, world};

const URL: &str = "wss://talk.example";

fn online(world: &Shared) -> Device {
    let device = Device::online(Screen::Home, galaxy_a27(), world);
    assert!(device.app().link().is_online(), "{:?}", device.app().link());
    device
}

/// The room the app is in, as the platform sees it.
fn room(device: &Device) -> &LiveRoom {
    device.ui.live_room().expect("a live room is joined")
}

fn joined(device: &Device) -> bool {
    device.ui.live_room().is_some()
}

/// Something happens in the room, and the screen is drawn again.
fn happens(device: &mut Device, event: LiveEvent) {
    assert!(device.ui.live_event(event), "no live room is joined");
    device.draw();
}

fn baraka() -> LiveEvent {
    LiveEvent::ParticipantJoined(LiveParticipant {
        identity: "baraka".into(),
        name: "Baraka Mushi".into(),
    })
}

fn note(device: &Device) -> Option<&'static str> {
    device.app().sound_note()
}

/// Baraka rings and the user accepts: the call is active on the server.
fn accepted_call(world: &Shared, video: bool) -> Device {
    let mut device = online(world);
    lock(world).ring("c9", "baraka", "Baraka Mushi", video);
    device.ticks(POLL_INCOMING + 3);
    assert_eq!(device.screen(), Screen::Incoming);
    assert!(!joined(&device), "a ringing call has no sound to join");
    assert_eq!(note(&device), None);
    device.tap("accept");
    device.ticks(2);
    assert_eq!(device.screen(), Screen::VideoCall);
    assert_eq!(device.app().call().unwrap().status, CallStatus::Active);
    device
}

fn dial(device: &mut Device) {
    device.tap("dial");
    for key in "+255712345678".chars() {
        device.tap(("dial", key));
    }
    device.tap("dial-call");
    assert_eq!(device.screen(), Screen::PhoneCall);
    device.ticks(2);
}

// --- A call between two people ---------------------------------------------

#[test]
fn an_accepted_call_joins_its_room_with_the_callees_ticket() {
    let world = world();
    let mut device = accepted_call(&world, true);
    let live = room(&device);
    assert_eq!(live.join.url, URL);
    assert_eq!(live.join.token, "callee-ticket");
    assert!(live.microphone, "the microphone is live");
    assert!(live.speaker, "the call screen is held in front of the face");
    assert!(!live.join.report_speaking);
    assert_eq!(device.app().sound(), &Sound::Joining);
    assert_eq!(note(&device), Some(sound::JOINING));
    assert!(device.has("sound-note"));

    // Connected, and alone until the other side's client has joined.
    happens(&mut device, LiveEvent::Connected);
    assert_eq!(device.app().sound(), &Sound::On);
    assert_eq!(note(&device), Some(sound::ALONE));
    happens(&mut device, baraka());
    // Sound is there; of video the note says what is true (no picture of
    // the other side has been announced; `tests/video.rs` has the rest).
    assert_eq!(note(&device), Some(sound::LIVE_NO_PICTURE));
    assert!(
        !note(&device).unwrap().contains("audio"),
        "no 'next version' for sound"
    );
    happens(&mut device, LiveEvent::ParticipantLeft("baraka".into()));
    assert_eq!(note(&device), Some(sound::ALONE));
}

#[test]
fn a_voice_call_says_only_that_sound_is_live() {
    let world = world();
    let mut device = accepted_call(&world, false);
    happens(&mut device, LiveEvent::Connected);
    happens(&mut device, baraka());
    assert_eq!(note(&device), Some(sound::LIVE));
}

#[test]
fn a_call_one_makes_joins_only_when_it_is_answered() {
    let world = world();
    let mut device = online(&world);
    device.tap("tab-contacts");
    device.ticks(2);
    device.tap(("person-voice", 0usize));
    device.ticks(2);
    // It rings: the caller's ticket is held, and nothing is joined yet.
    assert_eq!(device.app().call().unwrap().status, CallStatus::Ringing);
    assert!(device.app().call().unwrap().ticket.is_some());
    assert!(!joined(&device));
    device.ticks(POLL_RINGING + 3);
    assert!(!joined(&device));

    lock(&world).call("call-1").status = "active";
    device.ticks(POLL_RINGING + 3);
    assert_eq!(device.screen(), Screen::VideoCall);
    assert_eq!(room(&device).join.token, "caller-ticket");
}

#[test]
fn the_mute_button_acts_on_the_room() {
    let world = world();
    let mut device = accepted_call(&world, true);
    happens(&mut device, LiveEvent::Connected);
    assert!(room(&device).microphone);
    device.tap("mute");
    assert!(device.app().call().unwrap().muted);
    assert!(
        !room(&device).microphone,
        "muted in the room, not only on the button"
    );
    device.tap("mute");
    assert!(room(&device).microphone);
    // The room itself is the same one throughout.
    assert!(room(&device).connected);
}

#[test]
fn a_call_muted_while_it_connects_stays_muted() {
    let world = world();
    let mut device = accepted_call(&world, true);
    // Still joining: the platform has the request and applies it.
    device.tap("mute");
    assert!(!room(&device).microphone);
    happens(&mut device, LiveEvent::Connected);
    assert!(!room(&device).microphone);
}

#[test]
fn hanging_up_leaves_the_room() {
    let world = world();
    let mut device = accepted_call(&world, true);
    happens(&mut device, LiveEvent::Connected);
    device.tap("hang-up");
    assert!(
        !joined(&device),
        "left at once, not when the server answers"
    );
    device.ticks(2);
    assert_eq!(device.screen(), Screen::Home);
    assert_eq!(device.app().sound(), &Sound::Off);
    assert_eq!(lock(&world).call("c9").status, "ended");
    assert_eq!(note(&device), None);
}

#[test]
fn back_from_the_call_leaves_the_room() {
    let world = world();
    let mut device = accepted_call(&world, true);
    happens(&mut device, LiveEvent::Connected);
    assert!(device.back(), "Back ends the call");
    assert!(!joined(&device));
    assert!(device.app().call().is_none());
    assert_eq!(device.app().sound(), &Sound::Off);
}

#[test]
fn the_back_arrow_of_the_call_screen_leaves_the_room() {
    let world = world();
    let mut device = accepted_call(&world, true);
    happens(&mut device, LiveEvent::Connected);
    device.tap("back");
    assert!(!joined(&device));
    assert!(device.app().call().is_none());
}

#[test]
fn the_other_side_hanging_up_leaves_the_room() {
    let world = world();
    let mut device = accepted_call(&world, true);
    happens(&mut device, LiveEvent::Connected);
    lock(&world).call("c9").status = "ended";
    for _ in 0..40 {
        if device.app().call().is_none() {
            break;
        }
        device.ticks(1);
    }
    assert!(device.app().call().is_none(), "the poll says it is over");
    assert!(!joined(&device));
    assert_eq!(device.app().sound(), &Sound::Off);
}

#[test]
fn a_declined_call_never_joins_anything() {
    let world = world();
    let mut device = online(&world);
    lock(&world).ring("c9", "baraka", "Baraka Mushi", true);
    device.ticks(POLL_INCOMING + 3);
    device.tap("decline");
    device.ticks(2);
    assert!(!joined(&device));
    assert_eq!(device.app().sound(), &Sound::Off);
    assert!(
        device
            .ui
            .pending_effects()
            .iter()
            .all(|call| !matches!(call.request, ServiceRequest::LiveJoin(_))),
        "no join was ever asked for"
    );
}

#[test]
fn the_end_of_a_room_that_was_left_changes_nothing_in_the_next_call() {
    let world = world();
    let mut device = accepted_call(&world, true);
    happens(&mut device, LiveEvent::Connected);
    device.tap("hang-up");
    device.ticks(2);
    // The platform's "left" for the first room arrived in those frames.
    // A second call has a room of its own.
    lock(&world).ring("c10", "baraka", "Baraka Mushi", false);
    device.ticks(POLL_INCOMING + 6);
    assert_eq!(device.screen(), Screen::Incoming);
    device.tap("accept");
    device.ticks(2);
    assert_eq!(device.app().sound(), &Sound::Joining);
    happens(&mut device, LiveEvent::Connected);
    assert_eq!(device.app().sound(), &Sound::On);
}

// --- What can go wrong ------------------------------------------------------

#[test]
fn a_refused_microphone_is_said_plainly_and_the_call_goes_on() {
    let world = world();
    let mut device = accepted_call(&world, true);
    happens(&mut device, LiveEvent::Failed(LiveError::PermissionDenied));
    assert_eq!(device.app().sound(), &Sound::Failed(sound::NO_MICROPHONE));
    assert_eq!(note(&device), Some(sound::NO_MICROPHONE));
    assert!(note(&device).unwrap().contains("microphone is not allowed"));
    assert!(device.has("sound-note"));
    // The call itself is still there, with its buttons, and is not
    // joined again behind the user's back.
    assert_eq!(device.screen(), Screen::VideoCall);
    device.ticks(12);
    assert!(!joined(&device));
    // Muting now has no room to act on and asks for nothing.
    device.tap("mute");
    assert!(device.app().call().unwrap().muted);
    assert!(!joined(&device));
    device.tap("hang-up");
    device.ticks(2);
    assert_eq!(device.screen(), Screen::Home);
}

#[test]
fn a_connection_that_fails_is_said_without_the_platforms_words() {
    let world = world();
    let mut device = accepted_call(&world, true);
    happens(
        &mut device,
        LiveEvent::Failed(LiveError::Failed(
            "ConnectException: wss://talk.example refused".into(),
        )),
    );
    assert_eq!(note(&device), Some(sound::FAILED));
    assert!(!note(&device).unwrap().contains("talk.example"));
    assert!(!joined(&device));
}

#[test]
fn a_build_without_live_rooms_says_so() {
    // What an app built without the service, or on iOS, answers.
    let shared = world();
    let world = std::sync::Arc::clone(&shared);
    let ui = morningstar::ui::Ui::with_services(
        barua_talk::BaruaTalk::on(Screen::Home),
        move |request| match request {
            ServiceRequest::Http(request) => lock(&world).answer(request).map(ServiceResult::Http),
            ServiceRequest::LiveJoin(_) => Some(ServiceResult::Live(LiveEvent::Failed(
                LiveError::Unsupported,
            ))),
            _ => None,
        },
    );
    let mut device = Device::with(ui, galaxy_a27());
    device.ticks(6);
    assert!(device.app().link().is_online());
    lock(&shared).ring("c9", "baraka", "Baraka Mushi", true);
    device.ticks(POLL_INCOMING + 3);
    device.tap("accept");
    device.ticks(3);
    assert_eq!(device.screen(), Screen::VideoCall);
    assert_eq!(note(&device), Some(sound::UNAVAILABLE));
    assert_eq!(device.app().call().unwrap().status, CallStatus::Active);
}

#[test]
fn a_dropped_connection_is_shown_while_it_is_made_again_and_when_it_is_lost() {
    let world = world();
    let mut device = accepted_call(&world, false);
    happens(&mut device, LiveEvent::Connected);
    happens(&mut device, baraka());
    happens(&mut device, LiveEvent::Reconnecting);
    assert_eq!(note(&device), Some(sound::RECONNECTING));
    happens(&mut device, LiveEvent::Reconnected);
    assert_eq!(note(&device), Some(sound::LIVE));
    happens(&mut device, LiveEvent::Reconnecting);
    happens(&mut device, LiveEvent::Disconnected(LiveDisconnect::Lost));
    assert_eq!(note(&device), Some(sound::LOST));
    assert!(!joined(&device));
    device.ticks(12);
    assert!(!joined(&device), "not tried again by itself");
}

#[test]
fn a_room_closed_from_the_other_side_is_said() {
    let world = world();
    let mut device = accepted_call(&world, false);
    happens(&mut device, LiveEvent::Connected);
    happens(&mut device, LiveEvent::Disconnected(LiveDisconnect::Ended));
    assert_eq!(note(&device), Some(sound::ENDED));
}

#[test]
fn leaving_the_screen_pauses_the_sound_and_coming_back_joins_again() {
    let world = world();
    let mut device = accepted_call(&world, true);
    happens(&mut device, LiveEvent::Connected);
    happens(&mut device, baraka());
    device.tap("mute");

    // The platform leaves the room when the app leaves the screen. No
    // frame is drawn while it is away, so no tick comes either.
    assert!(
        device
            .ui
            .live_event(LiveEvent::Disconnected(LiveDisconnect::Hidden))
    );
    assert_eq!(device.app().sound(), &Sound::Away);
    assert_eq!(note(&device), Some(sound::AWAY));
    assert!(!joined(&device), "no microphone behind a hidden screen");

    // Back on screen: the first tick joins the same call's room again,
    // as the button stands (muted).
    device.ticks(1);
    assert_eq!(device.app().sound(), &Sound::Joining);
    let live = room(&device);
    assert_eq!(live.join.token, "callee-ticket");
    assert!(!live.microphone, "still muted");
    happens(&mut device, LiveEvent::Connected);
    assert_eq!(device.app().sound(), &Sound::On);
}

#[test]
fn a_join_refused_because_the_app_is_still_hidden_waits_for_the_next_tick() {
    let world = world();
    let mut device = accepted_call(&world, true);
    happens(&mut device, LiveEvent::Connected);
    device
        .ui
        .live_event(LiveEvent::Disconnected(LiveDisconnect::Hidden));
    device.ticks(1);
    // The host says "still hidden" to the new join: no loop of joins.
    device
        .ui
        .live_event(LiveEvent::Disconnected(LiveDisconnect::Hidden));
    assert_eq!(device.app().sound(), &Sound::Away);
    assert!(!joined(&device));
    assert!(
        device
            .ui
            .pending_effects()
            .iter()
            .all(|call| !matches!(call.request, ServiceRequest::LiveJoin(_)))
    );
}

// --- A room's call ----------------------------------------------------------

#[test]
fn a_rooms_call_joins_when_its_ticket_arrives_and_back_to_chat_leaves() {
    let world = world();
    let mut device = online(&world);
    device.tap(("item", 0usize));
    device.ticks(2);
    lock(&world).hold = Some("/ticket");
    device.tap("room-video");
    device.ticks(1);
    assert_eq!(device.screen(), Screen::VideoCall);
    assert!(!joined(&device), "no ticket yet");
    assert_eq!(note(&device), Some(sound::WAITING));
    lock(&world).hold = None;
    let ticket = morningstar::HttpResponse::new(200).with_body(
        br#"{"token": "room-ticket", "url": "wss://talk.example", "identity": "neema",
            "room": "support-desk", "video": true, "expires_at": "2026-11-15T08:01:12Z"}"#
            .to_vec(),
    );
    assert!(device.ui.answer_http(Ok(ticket)));
    device.draw();
    assert_eq!(room(&device).join.token, "room-ticket");
    assert!(room(&device).speaker);
    happens(&mut device, LiveEvent::Connected);
    assert_eq!(note(&device), Some(sound::ALONE));
    happens(&mut device, baraka());
    assert_eq!(note(&device), Some(sound::LIVE_NO_PICTURE));

    device.tap("back");
    assert_eq!(device.screen(), Screen::Room, "back to chat");
    assert!(!joined(&device));
    // Opening it again is a new room with a new ticket.
    device.tap("room-video");
    device.ticks(2);
    assert_eq!(device.app().sound(), &Sound::Joining);
    assert!(joined(&device));
}

// --- A phone call -----------------------------------------------------------

#[test]
fn a_phone_call_joins_as_soon_as_it_is_dialled_at_the_ear() {
    let world = world();
    let mut device = online(&world);
    dial(&mut device);
    assert_eq!(device.app().phone().unwrap().status, PhoneStatus::Ringing);
    let live = room(&device);
    assert_eq!(live.join.token, "phone-ticket");
    assert_eq!(live.join.url, URL);
    assert!(live.microphone);
    assert!(!live.speaker, "a phone call starts in the earpiece");
    assert_eq!(note(&device), Some(sound::JOINING));
    happens(&mut device, LiveEvent::Connected);
    assert_eq!(note(&device), Some(sound::ALONE));
    // The other phone is answered: its line joins the room.
    {
        let mut world = lock(&world);
        world.phones[0].status = "active";
        world.phones[0].seconds = 1;
    }
    device.ticks(POLL_PHONE + 3);
    happens(
        &mut device,
        LiveEvent::ParticipantJoined(LiveParticipant {
            identity: "sip_+255712345678".into(),
            name: String::new(),
        }),
    );
    assert_eq!(note(&device), Some(sound::LIVE));
    assert!(device.has("sound-note"));
}

#[test]
fn the_speaker_and_mute_buttons_of_a_phone_call_act_on_the_room() {
    let world = world();
    let mut device = online(&world);
    dial(&mut device);
    happens(&mut device, LiveEvent::Connected);
    device.tap("speaker");
    assert!(device.app().phone().unwrap().speaker);
    assert!(room(&device).speaker);
    device.tap("speaker");
    assert!(!room(&device).speaker);
    device.tap("mute");
    assert!(!room(&device).microphone);
    device.tap("mute");
    assert!(room(&device).microphone);
}

#[test]
fn leaving_a_phone_call_leaves_its_room_and_so_does_its_end() {
    let world = world();
    let mut device = online(&world);
    dial(&mut device);
    happens(&mut device, LiveEvent::Connected);
    device.tap("hang-up");
    assert!(!joined(&device));
    device.ticks(2);
    assert_eq!(device.screen(), Screen::Home);
    assert_eq!(device.app().sound(), &Sound::Off);

    // The other side ends the second one.
    dial(&mut device);
    happens(&mut device, LiveEvent::Connected);
    {
        let mut world = lock(&world);
        world.phones[1].status = "ended";
        world.phones[1].seconds = 20;
    }
    for _ in 0..40 {
        if device.app().phone().is_none() {
            break;
        }
        device.ticks(1);
    }
    assert!(device.app().phone().is_none());
    assert!(!joined(&device));
}

#[test]
fn back_from_a_phone_call_leaves_its_room() {
    let world = world();
    let mut device = online(&world);
    dial(&mut device);
    happens(&mut device, LiveEvent::Connected);
    assert!(device.back());
    assert!(!joined(&device));
    assert!(device.app().phone().is_none());
}

#[test]
fn a_dial_the_server_refuses_joins_nothing() {
    let world = world();
    lock(&world).phone = false;
    let mut device = online(&world);
    device.tap("dial");
    for key in "+255712345678".chars() {
        device.tap(("dial", key));
    }
    device.tap("dial-call");
    device.ticks(2);
    assert_eq!(device.screen(), Screen::Dial);
    assert!(!joined(&device));
    assert_eq!(device.app().sound(), &Sound::Off);
}

// --- When the server goes away, and the scripted app ------------------------

#[test]
fn losing_the_server_in_a_call_leaves_the_room() {
    let world = world();
    let mut device = accepted_call(&world, true);
    happens(&mut device, LiveEvent::Connected);
    assert!(joined(&device));
    // Going offline drops whatever call was open (the scripted world and
    // the server's are never mixed): its room must go with it, without
    // the user touching anything.
    lock(&world).down = true;
    for _ in 0..60 {
        if !device.app().link().is_online() {
            break;
        }
        device.ticks(1);
    }
    assert!(!device.app().link().is_online());
    assert!(device.app().call().is_none());
    assert!(!joined(&device));
    assert_eq!(device.app().sound(), &Sound::Off);
}

#[test]
fn the_scripted_app_never_joins_anything() {
    // Offline: the server does not answer, and the app is the scripted one.
    let world = world();
    lock(&world).down = true;
    let mut device = Device::online(Screen::Home, design(), &world);
    assert!(!device.app().link().is_online());
    let never_joined = |device: &Device| {
        assert!(!joined(device));
        assert_eq!(device.app().sound(), &Sound::Off);
        assert_eq!(device.app().sound_note(), None);
        assert!(!device.has("sound-note"));
        assert!(
            device
                .ui
                .pending_effects()
                .iter()
                .all(|call| !matches!(call.request, ServiceRequest::LiveJoin(_)))
        );
    };
    // Baraka "rings" and is accepted, by script.
    device.tap(("item", 0usize));
    assert_eq!(device.screen(), Screen::Incoming);
    device.tap("accept");
    assert_eq!(device.screen(), Screen::VideoCall);
    device.ticks(6);
    never_joined(&device);
    device.tap("mute");
    never_joined(&device);
    device.tap("hang-up");
    // A phone "picks up", by script.
    device.tap("dial");
    for key in "+255712345678".chars() {
        device.tap(("dial", key));
    }
    device.tap("dial-call");
    assert_eq!(device.screen(), Screen::PhoneCall);
    device.ticks(12);
    device.tap("speaker");
    device.tap("mute");
    never_joined(&device);
}

#[test]
fn the_designs_own_pages_join_nothing() {
    // Standalone, as the fidelity tests and the screenshots open them.
    for screen in [Screen::VideoCall, Screen::PhoneCall, Screen::Incoming] {
        let mut device = Device::on(screen, design());
        device.ticks(3);
        assert!(!joined(&device));
        assert_eq!(device.app().sound(), &Sound::Off);
        assert_eq!(device.app().sound_note(), None);
    }
}
