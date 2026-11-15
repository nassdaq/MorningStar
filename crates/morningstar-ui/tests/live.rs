//! The live room (ADR 0038): the one platform service whose request is
//! answered more than once. Driven by the test driver, by a stand-in host
//! closure, with nothing behind the application at all, and through the
//! `App` interface and an `Engine` as a real host drives a `Ui`.

use std::sync::{Arc, Mutex};

use morningstar_core::{ServiceCall, ServiceId, SurfaceSize};
use morningstar_runtime::{App, Engine, Frame, FrameInfo};
use morningstar_ui::prelude::*;

const SIZE: SurfaceSize = SurfaceSize::new(360, 640);
const URL: &str = "wss://live.example.com";

#[derive(Clone, Debug, PartialEq)]
enum Want {
    Join(&'static str),
    JoinMutedOnSpeaker(&'static str),
    Leave,
    Microphone(bool),
    Speaker(bool),
    Forget,
    Get,
}

#[derive(Clone, Debug, PartialEq)]
enum Msg {
    Do(Want),
    /// An event of the room joined with this ticket.
    Live(&'static str, LiveEvent),
    Replied(Result<HttpResponse, HttpError>),
}

/// Joins what it is asked to and records every event it is told.
#[derive(Default)]
struct Caller {
    wanted: Vec<Want>,
    log: Vec<Msg>,
    /// A copy of the log that outlives the application.
    heard: Arc<Mutex<Vec<Msg>>>,
    /// Leaves the room as soon as it is connected.
    leave_when_connected: bool,
}

impl Application for Caller {
    type Message = Msg;

    fn view(&self) -> Element<Msg> {
        col![].into()
    }

    fn update(&mut self, message: Msg) {
        match message {
            Msg::Do(want) => self.wanted.push(want),
            other => {
                if self.leave_when_connected && matches!(other, Msg::Live(_, LiveEvent::Connected))
                {
                    self.wanted.push(Want::Leave);
                }
                self.heard.lock().unwrap().push(other.clone());
                self.log.push(other);
            }
        }
    }

    fn effects(&mut self, out: &mut Effects<Msg>) {
        for want in self.wanted.drain(..) {
            match want {
                Want::Join(ticket) => out.join_live(LiveJoin::new(URL, ticket), move |event| {
                    Msg::Live(ticket, event)
                }),
                Want::JoinMutedOnSpeaker(ticket) => out.join_live(
                    LiveJoin::new(URL, ticket).microphone(false).speaker(true),
                    move |event| Msg::Live(ticket, event),
                ),
                Want::Leave => out.leave_live(),
                Want::Microphone(live) => out.set_live_microphone(live),
                Want::Speaker(on) => out.set_live_speaker(on),
                Want::Forget => out.forget_pending(),
                Want::Get => out.http(HttpRequest::get("https://example.com"), Msg::Replied),
            }
        }
    }
}

fn testing() -> Ui<Caller> {
    Ui::testing(Caller::default())
}

fn want(ui: &mut Ui<Caller>, want: Want) {
    ui.send(Msg::Do(want));
}

fn someone(identity: &str, name: &str) -> LiveParticipant {
    LiveParticipant {
        identity: identity.into(),
        name: name.into(),
    }
}

fn left() -> LiveEvent {
    LiveEvent::Disconnected(LiveDisconnect::Left)
}

/// What a host does: takes the requests made since it last asked.
fn host_takes(ui: &mut Ui<Caller>) -> Vec<ServiceCall> {
    let mut out = Vec::new();
    App::service_requests(ui, &mut out);
    out
}

// --- The test driver --------------------------------------------------------

#[test]
fn a_join_is_pending_and_the_driver_knows_the_room() {
    let mut ui = testing();
    assert!(ui.live_room().is_none());
    want(&mut ui, Want::Join("ticket-a"));

    let pending = ui.pending_effects();
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending[0].request,
        ServiceRequest::LiveJoin(LiveJoin::new(URL, "ticket-a"))
    );
    let room = ui.live_room().expect("a room is joined");
    assert_eq!(room.join.url, URL);
    assert_eq!(room.join.token, "ticket-a");
    assert!(room.microphone && !room.speaker && !room.connected);
    assert_eq!(ui.effects_in_flight(), 1);
    assert!(ui.app().log.is_empty(), "nothing has happened in it yet");
}

#[test]
fn every_event_of_the_room_is_a_message_in_order_until_the_final_one() {
    let mut ui = testing();
    want(&mut ui, Want::Join("t"));
    let events = [
        LiveEvent::Connected,
        LiveEvent::ParticipantJoined(someone("baraka", "Baraka")),
        LiveEvent::Speaking(vec!["baraka".into()]),
        LiveEvent::Reconnecting,
        LiveEvent::Reconnected,
        LiveEvent::ParticipantLeft("baraka".into()),
        LiveEvent::Disconnected(LiveDisconnect::Ended),
    ];
    for event in &events {
        assert!(ui.live_event(event.clone()), "{event:?}");
    }
    let expected: Vec<Msg> = events.iter().cloned().map(|e| Msg::Live("t", e)).collect();
    assert_eq!(ui.app().log, expected);

    // The room is over: nothing is joined, nothing is waited for, and
    // nothing more can happen in it.
    assert!(ui.live_room().is_none());
    assert_eq!(ui.effects_in_flight(), 0);
    assert!(ui.pending_effects().is_empty());
    assert!(!ui.live_event(LiveEvent::Connected));
    assert_eq!(ui.app().log.len(), events.len());
}

#[test]
fn the_first_event_takes_the_join_off_the_pending_list_and_the_room_stays() {
    let mut ui = testing();
    want(&mut ui, Want::Join("t"));
    assert!(ui.live_event(LiveEvent::Connected));
    assert!(ui.pending_effects().is_empty());
    assert!(ui.live_room().expect("still joined").connected);
    assert_eq!(ui.effects_in_flight(), 1, "its events are still wanted");
}

#[test]
fn a_failure_is_final_whatever_its_reason() {
    for error in [
        LiveError::PermissionDenied,
        LiveError::Unsupported,
        LiveError::Failed("the server refused the ticket".into()),
    ] {
        let mut ui = testing();
        want(&mut ui, Want::Join("t"));
        assert!(ui.live_event(LiveEvent::Failed(error.clone())));
        assert_eq!(ui.app().log, [Msg::Live("t", LiveEvent::Failed(error))]);
        assert!(ui.live_room().is_none());
        assert_eq!(ui.effects_in_flight(), 0);
    }
}

#[test]
fn leaving_ends_the_room_as_left_at_the_next_delivery() {
    let mut ui = testing();
    want(&mut ui, Want::Join("t"));
    ui.live_event(LiveEvent::Connected);
    want(&mut ui, Want::Leave);

    // As on a device: the host has been told, the room is gone, and the
    // application hears of it a moment later, not inside its own update.
    assert!(ui.live_room().is_none());
    assert_eq!(ui.app().log, [Msg::Live("t", LiveEvent::Connected)]);
    assert_eq!(ui.deliver_effects(), 1);
    assert_eq!(ui.app().log.last(), Some(&Msg::Live("t", left())));
    assert_eq!(ui.effects_in_flight(), 0);
}

#[test]
fn leaving_a_room_that_never_connected_ends_it_too() {
    let mut ui = testing();
    want(&mut ui, Want::Join("t"));
    want(&mut ui, Want::Leave);
    assert!(ui.pending_effects().is_empty());
    ui.deliver_effects();
    assert_eq!(ui.app().log, [Msg::Live("t", left())]);
}

#[test]
fn leaving_with_no_room_does_nothing() {
    let mut ui = testing();
    want(&mut ui, Want::Leave);
    want(&mut ui, Want::Microphone(false));
    want(&mut ui, Want::Speaker(true));
    assert_eq!(ui.deliver_effects(), 0);
    assert!(ui.pending_effects().is_empty());
    assert!(ui.live_room().is_none());
    assert!(ui.app().log.is_empty());
}

#[test]
fn there_is_one_room_at_a_time_a_second_join_replaces_the_first() {
    let mut ui = testing();
    want(&mut ui, Want::Join("first"));
    ui.live_event(LiveEvent::Connected);
    want(&mut ui, Want::Join("second"));

    assert_eq!(ui.live_room().expect("the second").join.token, "second");
    ui.deliver_effects();
    assert_eq!(
        ui.app().log,
        [
            Msg::Live("first", LiveEvent::Connected),
            Msg::Live("first", LiveEvent::Disconnected(LiveDisconnect::Replaced)),
        ]
    );
    // What happens now happens in the second room.
    ui.live_event(LiveEvent::Connected);
    assert_eq!(
        ui.app().log.last(),
        Some(&Msg::Live("second", LiveEvent::Connected))
    );
    assert_eq!(ui.effects_in_flight(), 1);
}

#[test]
fn the_microphone_and_the_loudspeaker_are_set_on_the_room() {
    let mut ui = testing();
    want(&mut ui, Want::JoinMutedOnSpeaker("t"));
    let room = ui.live_room().unwrap();
    assert!(!room.microphone && room.speaker);
    assert!(!room.join.microphone && room.join.speaker);

    want(&mut ui, Want::Microphone(true));
    want(&mut ui, Want::Speaker(false));
    let room = ui.live_room().unwrap();
    assert!(room.microphone && !room.speaker);
    // What it was joined with is not rewritten.
    assert!(!room.join.microphone && room.join.speaker);

    want(&mut ui, Want::Microphone(false));
    assert!(!ui.live_room().unwrap().microphone);
    assert!(ui.app().log.is_empty(), "none of these has a message");
}

#[test]
fn the_app_leaving_the_screen_ends_the_room_as_hidden() {
    let mut ui = testing();
    want(&mut ui, Want::Join("t"));
    ui.live_event(LiveEvent::Connected);
    assert!(ui.live_event(LiveEvent::Disconnected(LiveDisconnect::Hidden)));
    assert!(ui.live_room().is_none());
    // Back on screen the application may join again, as a new room.
    want(&mut ui, Want::Join("t"));
    assert!(ui.live_room().is_some());
    assert_eq!(ui.effects_in_flight(), 1);
}

#[test]
fn forgetting_pending_work_leaves_the_room_and_drops_its_events() {
    let mut ui = testing();
    want(&mut ui, Want::Join("t"));
    ui.live_event(LiveEvent::Connected);
    want(&mut ui, Want::Get);
    want(&mut ui, Want::Forget);

    assert!(ui.live_room().is_none(), "never left connected and unheard");
    assert_eq!(ui.effects_in_flight(), 0);
    assert!(ui.pending_effects().is_empty());
    assert_eq!(ui.deliver_effects(), 0, "its end is not a message either");
    assert!(!ui.live_event(LiveEvent::Reconnecting));
    assert_eq!(ui.app().log, [Msg::Live("t", LiveEvent::Connected)]);
}

#[test]
fn a_room_joined_after_forgetting_is_a_new_one() {
    let mut ui = testing();
    want(&mut ui, Want::Join("old"));
    ui.send(Msg::Do(Want::Forget));
    want(&mut ui, Want::Join("new"));
    assert_eq!(ui.live_room().unwrap().join.token, "new");
    ui.deliver_effects();
    ui.live_event(LiveEvent::Connected);
    assert_eq!(ui.app().log, [Msg::Live("new", LiveEvent::Connected)]);
}

#[test]
fn an_answer_of_the_wrong_kind_fails_the_room_instead_of_panicking() {
    let mut ui = testing();
    want(&mut ui, Want::Join("t"));
    let id = ui.pending_effects()[0].id;
    assert!(ui.resolve(id, ServiceResult::Loaded(None)));
    let [Msg::Live("t", LiveEvent::Failed(LiveError::Failed(_)))] = ui.app().log.as_slice() else {
        panic!("expected a failure, got {:?}", ui.app().log);
    };
    assert!(ui.live_room().is_none());
    assert_eq!(ui.effects_in_flight(), 0);
}

#[test]
fn an_application_may_leave_from_inside_an_event() {
    let mut ui = Ui::testing(Caller {
        leave_when_connected: true,
        ..Caller::default()
    });
    want(&mut ui, Want::Join("t"));
    ui.live_event(LiveEvent::Connected);
    ui.deliver_effects();
    assert_eq!(
        ui.app().log,
        [Msg::Live("t", LiveEvent::Connected), Msg::Live("t", left())]
    );
}

// --- A stand-in host --------------------------------------------------------

#[test]
fn a_stand_in_host_connects_the_room_at_the_next_frame() {
    let mut ui = Ui::with_services(Caller::default(), |request| match request {
        ServiceRequest::LiveJoin(_) => Some(ServiceResult::Live(LiveEvent::Connected)),
        _ => None,
    });
    want(&mut ui, Want::Join("t"));
    assert!(
        ui.app().log.is_empty(),
        "the joining state can be looked at"
    );
    assert!(!ui.live_room().unwrap().connected);
    assert_eq!(ui.deliver_effects(), 1);
    assert_eq!(ui.app().log, [Msg::Live("t", LiveEvent::Connected)]);
    assert!(ui.live_room().unwrap().connected);
    // The test goes on playing the room.
    assert!(ui.live_event(LiveEvent::ParticipantJoined(someone("asha", "Asha"))));
    assert_eq!(ui.app().log.len(), 2);
}

// --- Nothing behind the application ----------------------------------------

#[test]
fn with_no_host_a_join_fails_as_unsupported_at_the_next_frame() {
    let mut ui = Ui::new(Caller::default());
    let mut frame = Frame::for_app(&mut ui);
    want(&mut ui, Want::Join("t"));
    want(&mut ui, Want::Microphone(false));
    assert!(ui.app().log.is_empty());
    frame.begin();
    App::frame(&mut ui, &FrameInfo::new(SIZE), &mut frame);
    assert_eq!(
        ui.app().log,
        [Msg::Live("t", LiveEvent::Failed(LiveError::Unsupported))]
    );
    assert_eq!(ui.effects_in_flight(), 0);
    assert!(ui.live_room().is_none(), "there is no driver to hold one");
}

// --- As a real host drives it ----------------------------------------------

#[test]
fn a_host_is_handed_the_join_and_its_events_arrive_under_the_joins_id() {
    let mut ui = Ui::new(Caller::default());
    assert!(host_takes(&mut ui).is_empty());
    want(&mut ui, Want::Join("t"));
    want(&mut ui, Want::Speaker(true));
    let calls = host_takes(&mut ui);
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls[0].request,
        ServiceRequest::LiveJoin(LiveJoin::new(URL, "t"))
    );
    assert_eq!(calls[1].request, ServiceRequest::LiveSpeaker(true));
    let join = calls[0].id;

    assert!(App::service_result(
        &mut ui,
        join,
        ServiceResult::Live(LiveEvent::Connected)
    ));
    assert!(App::service_result(
        &mut ui,
        join,
        ServiceResult::Live(LiveEvent::Reconnecting)
    ));
    // An event under another id is nobody's.
    assert!(!App::service_result(
        &mut ui,
        calls[1].id,
        ServiceResult::Live(LiveEvent::Connected)
    ));
    assert!(App::service_result(
        &mut ui,
        join,
        ServiceResult::Live(LiveEvent::Disconnected(LiveDisconnect::Lost))
    ));
    // After the end the host may still be sending what was on its way.
    assert!(!App::service_result(
        &mut ui,
        join,
        ServiceResult::Live(LiveEvent::Reconnected)
    ));
    assert_eq!(
        ui.app().log,
        [
            Msg::Live("t", LiveEvent::Connected),
            Msg::Live("t", LiveEvent::Reconnecting),
            Msg::Live("t", LiveEvent::Disconnected(LiveDisconnect::Lost)),
        ]
    );
}

#[test]
fn a_host_is_told_to_leave_when_the_application_forgets_a_joined_room() {
    let mut ui = Ui::new(Caller::default());
    host_takes(&mut ui);
    want(&mut ui, Want::Join("t"));
    let join = host_takes(&mut ui)[0].id;
    App::service_result(&mut ui, join, ServiceResult::Live(LiveEvent::Connected));

    want(&mut ui, Want::Forget);
    let calls = host_takes(&mut ui);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].request, ServiceRequest::LiveLeave);
    // The host's "left" finds nobody: the application gave the room up.
    assert!(!App::service_result(
        &mut ui,
        join,
        ServiceResult::Live(left())
    ));
    assert_eq!(ui.app().log, [Msg::Live("t", LiveEvent::Connected)]);
}

#[test]
fn a_join_forgotten_before_the_host_took_it_is_never_made() {
    let mut ui = Ui::new(Caller::default());
    host_takes(&mut ui);
    want(&mut ui, Want::Join("t"));
    want(&mut ui, Want::Forget);
    let calls = host_takes(&mut ui);
    // Only the leave, which a host with no room ignores: the ticket never
    // left the application.
    assert_eq!(
        calls.iter().map(|call| &call.request).collect::<Vec<_>>(),
        [&ServiceRequest::LiveLeave]
    );
}

#[test]
fn with_a_host_two_joins_are_two_rooms_until_the_first_is_reported_replaced() {
    let mut ui = Ui::new(Caller::default());
    host_takes(&mut ui);
    want(&mut ui, Want::Join("first"));
    want(&mut ui, Want::Join("second"));
    let calls = host_takes(&mut ui);
    let (first, second) = (calls[0].id, calls[1].id);
    assert_eq!(ui.effects_in_flight(), 2);

    App::service_result(
        &mut ui,
        first,
        ServiceResult::Live(LiveEvent::Disconnected(LiveDisconnect::Replaced)),
    );
    App::service_result(&mut ui, second, ServiceResult::Live(LiveEvent::Connected));
    assert_eq!(
        ui.app().log,
        [
            Msg::Live("first", LiveEvent::Disconnected(LiveDisconnect::Replaced)),
            Msg::Live("second", LiveEvent::Connected),
        ]
    );
    assert_eq!(ui.effects_in_flight(), 1);
}

#[test]
fn under_an_engine_each_event_is_delivered_at_once() {
    let heard = Arc::new(Mutex::new(Vec::new()));
    let app = Caller {
        wanted: vec![Want::Join("t")],
        heard: heard.clone(),
        ..Caller::default()
    };
    let mut engine = Engine::new(Box::new(Ui::new(app)));
    let mut calls = Vec::new();
    engine.take_service_requests(&mut calls);
    assert_eq!(calls.len(), 1, "joined from setup, there before any frame");
    let join = calls[0].id;

    for event in [
        LiveEvent::Connected,
        LiveEvent::ParticipantJoined(someone("baraka", "Baraka")),
        LiveEvent::Disconnected(LiveDisconnect::Ended),
    ] {
        assert!(engine.service_result(join, ServiceResult::Live(event)));
    }
    assert_eq!(heard.lock().unwrap().len(), 3);
    assert!(!engine.service_result(join, ServiceResult::Live(LiveEvent::Connected)));
    assert!(!engine.service_result(ServiceId(99), ServiceResult::Live(LiveEvent::Connected)));
    assert_eq!(heard.lock().unwrap().len(), 3);
}

#[test]
fn a_dropped_ui_drops_the_rooms_handler_uncalled() {
    let heard = Arc::new(Mutex::new(Vec::new()));
    let mut ui = Ui::testing(Caller {
        heard: heard.clone(),
        ..Caller::default()
    });
    want(&mut ui, Want::Join("t"));
    ui.live_event(LiveEvent::Connected);
    drop(ui);
    assert_eq!(
        heard.lock().unwrap().len(),
        1,
        "nothing arrives after teardown"
    );
}
