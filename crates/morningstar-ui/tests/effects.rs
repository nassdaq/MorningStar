//! `Application::effects` (ADR 0019): requests to the platform and their
//! answers as messages, driven three ways: by the test driver, by a
//! stand-in host closure, and through the `App` interface as the engine
//! and a real host drive a `Ui`.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use morningstar_core::{HostState, ServiceCall, ServiceId, SurfaceSize};
use morningstar_runtime::{App, Engine, Frame, FrameInfo};
use morningstar_ui::prelude::*;

const SIZE: SurfaceSize = SurfaceSize::new(360, 640);

/// What the application's `update` asks `effects` to start.
#[derive(Clone, Debug, PartialEq)]
enum Want {
    Get(&'static str),
    StartRecording,
    StopRecording,
    Play(&'static [u8]),
    StopPlayback,
    Store(&'static str, &'static str),
    Remove(&'static str),
    Load(&'static str),
    Forget,
}

#[derive(Clone, Debug, PartialEq)]
enum Msg {
    Do(Want),
    Replied(Result<HttpResponse, HttpError>),
    RecordingStarted(Result<(), AudioError>),
    Recorded(Result<Recording, AudioError>),
    Played(Result<(), AudioError>),
    Loaded(Option<String>),
    Second,
}

/// Records everything it is told, and starts what it is asked to.
#[derive(Default)]
struct Client {
    wanted: Vec<Want>,
    /// Every message `update` received except `Do`, in order.
    log: Vec<Msg>,
    /// How often `effects` was called.
    effects_calls: usize,
    /// Requests to make from `setup`'s first `effects` call.
    at_setup: Vec<Want>,
    /// Each HTTP answer starts another request to this URL.
    chain: Option<&'static str>,
    /// What setup saw.
    install_id: String,
    stored_device_id: Option<String>,
    /// `back` asks for this and reports it handled.
    on_back: Option<Want>,
    ticking: bool,
    /// Every tick asks for this.
    on_tick: Option<Want>,
}

impl Application for Client {
    type Message = Msg;

    fn setup(&mut self, setup: &mut Setup<'_>) {
        self.install_id = setup.install_id().to_owned();
        self.stored_device_id = setup.stored("device_id").map(str::to_owned);
        self.wanted.append(&mut self.at_setup);
    }

    fn view(&self) -> Element<Msg> {
        col![].into()
    }

    fn update(&mut self, message: Msg) {
        match message {
            Msg::Do(want) => self.wanted.push(want),
            other => {
                if matches!(other, Msg::Replied(_))
                    && let Some(url) = self.chain
                {
                    self.wanted.push(Want::Get(url));
                }
                if other == Msg::Second
                    && let Some(want) = self.on_tick.clone()
                {
                    self.wanted.push(want);
                }
                self.log.push(other);
            }
        }
    }

    fn back(&mut self) -> bool {
        match self.on_back.take() {
            Some(want) => {
                self.wanted.push(want);
                true
            }
            None => false,
        }
    }

    fn tick(&self) -> Option<(Duration, Msg)> {
        self.ticking
            .then_some((Duration::from_secs(1), Msg::Second))
    }

    fn effects(&mut self, out: &mut Effects<Msg>) {
        self.effects_calls += 1;
        for want in self.wanted.drain(..) {
            match want {
                Want::Get(url) => out.http(HttpRequest::get(url), Msg::Replied),
                Want::StartRecording => out.start_recording(Msg::RecordingStarted),
                Want::StopRecording => out.stop_recording(Msg::Recorded),
                Want::Play(bytes) => out.play(AudioClip::new(bytes, "audio/mpeg"), Msg::Played),
                Want::StopPlayback => out.stop_playback(),
                Want::Store(key, value) => out.store(key, value),
                Want::Remove(key) => out.remove(key),
                Want::Load(key) => out.load(key, Msg::Loaded),
                Want::Forget => out.forget_pending(),
            }
        }
    }
}

/// A `Ui` and the frame it draws into, driven as the engine drives them.
struct Harness {
    ui: Ui<Client>,
    frame: Frame,
    elapsed: Duration,
}

impl Harness {
    fn new(mut ui: Ui<Client>) -> Self {
        let frame = Frame::for_app(&mut ui);
        Self {
            ui,
            frame,
            elapsed: Duration::ZERO,
        }
    }

    fn testing() -> Self {
        Self::new(Ui::testing(Client::default()))
    }

    /// With no host and no driver.
    fn bare() -> Self {
        Self::new(Ui::new(Client::default()))
    }

    fn want(&mut self, want: Want) {
        self.ui.send(Msg::Do(want));
    }

    /// Builds one frame. Returns whether it asked for another at once.
    fn frame(&mut self) -> bool {
        self.frame.begin();
        let info = FrameInfo::new(SIZE).with_elapsed(self.elapsed);
        App::frame(&mut self.ui, &info, &mut self.frame);
        self.frame.redraw_requested()
    }

    /// What a host does: takes the requests made since it last asked.
    fn host_takes(&mut self) -> Vec<ServiceCall> {
        let mut out = Vec::new();
        App::service_requests(&mut self.ui, &mut out);
        out
    }

    fn log(&self) -> &[Msg] {
        &self.ui.app().log
    }
}

fn ok(status: u16) -> Result<HttpResponse, HttpError> {
    Ok(HttpResponse::new(status))
}

fn url(call: &ServiceCall) -> &str {
    match &call.request {
        ServiceRequest::Http(request) => &request.url,
        other => panic!("not an HTTP request: {other:?}"),
    }
}

// --- When effects are asked for -------------------------------------------

#[test]
fn effects_is_asked_once_after_setup_and_once_after_every_update() {
    let mut h = Harness::testing();
    assert_eq!(h.ui.app().effects_calls, 1, "after setup");
    h.want(Want::Get("https://a.example"));
    assert_eq!(h.ui.app().effects_calls, 2);
    h.ui.answer_http(ok(200));
    assert_eq!(h.ui.app().effects_calls, 3, "an answer is an update too");
    h.frame();
    assert_eq!(h.ui.app().effects_calls, 3, "a frame is not");
}

#[test]
fn a_request_made_at_setup_is_pending_before_the_first_frame() {
    let app = Client {
        at_setup: vec![Want::Get("https://a.example/hello")],
        ..Client::default()
    };
    let h = Harness::new(Ui::testing(app));
    assert_eq!(h.ui.pending_effects().len(), 1);
    assert_eq!(url(&h.ui.pending_effects()[0]), "https://a.example/hello");
}

#[test]
fn a_request_made_from_back_is_collected() {
    let mut h = Harness::testing();
    h.ui.app_mut().on_back = Some(Want::Get("https://a.example/leave"));
    assert!(h.ui.back());
    assert_eq!(h.ui.pending_effects().len(), 1);
}

#[test]
fn a_request_made_from_a_tick_is_collected() {
    let mut h = Harness::testing();
    h.ui.app_mut().ticking = true;
    h.ui.app_mut().on_tick = Some(Want::Get("https://a.example/poll"));
    h.frame();
    assert!(h.ui.pending_effects().is_empty());
    h.elapsed = Duration::from_secs(1);
    h.frame();
    assert_eq!(h.log(), [Msg::Second]);
    assert_eq!(h.ui.pending_effects().len(), 1);
}

// --- The test driver -------------------------------------------------------

#[test]
fn requests_are_pending_in_the_order_made_with_increasing_ids() {
    let mut h = Harness::testing();
    h.want(Want::Get("https://a.example/1"));
    h.want(Want::StartRecording);
    h.want(Want::Get("https://a.example/2"));
    let pending = h.ui.pending_effects();
    assert_eq!(pending.len(), 3);
    assert_eq!(url(&pending[0]), "https://a.example/1");
    assert_eq!(pending[1].request, ServiceRequest::StartRecording);
    assert_eq!(url(&pending[2]), "https://a.example/2");
    assert!(pending[0].id < pending[1].id && pending[1].id < pending[2].id);
    assert!(pending[0].id.0 != 0);
    assert_eq!(h.ui.effects_in_flight(), 3);
}

#[test]
fn the_server_replies_and_the_application_gets_the_message_at_once() {
    let mut h = Harness::testing();
    h.want(Want::Get("https://a.example"));
    assert!(h.log().is_empty(), "nothing until the server answers");
    let reply = HttpResponse::new(200)
        .with_header("Content-Type", "audio/mpeg")
        .with_body(vec![1, 2, 3]);
    assert!(h.ui.answer_http(Ok(reply.clone())));
    assert_eq!(h.log(), [Msg::Replied(Ok(reply))]);
    assert!(h.ui.pending_effects().is_empty());
    assert_eq!(h.ui.effects_in_flight(), 0);
}

#[test]
fn each_kind_of_error_reaches_the_application_as_it_is() {
    let errors = [
        HttpError::Offline,
        HttpError::Timeout,
        HttpError::InvalidUrl,
        HttpError::TooLarge,
        HttpError::Failed("handshake".into()),
    ];
    let mut h = Harness::testing();
    for error in &errors {
        h.want(Want::Get("https://a.example"));
        assert!(h.ui.answer_http(Err(error.clone())));
    }
    let expected: Vec<Msg> = errors.into_iter().map(|e| Msg::Replied(Err(e))).collect();
    assert_eq!(h.log(), expected);
}

#[test]
fn answers_go_to_the_oldest_request_of_their_kind() {
    let mut h = Harness::testing();
    h.want(Want::Get("https://a.example/1"));
    h.want(Want::Get("https://a.example/2"));
    h.ui.answer_http(ok(201));
    assert_eq!(url(&h.ui.pending_effects()[0]), "https://a.example/2");
    h.ui.answer_http(ok(202));
    assert_eq!(h.log(), [Msg::Replied(ok(201)), Msg::Replied(ok(202))]);
    assert!(!h.ui.answer_http(ok(203)), "nothing left to answer");
}

#[test]
fn a_request_can_be_answered_out_of_order_by_id() {
    let mut h = Harness::testing();
    h.want(Want::Get("https://a.example/slow"));
    h.want(Want::Get("https://a.example/fast"));
    let fast = h.ui.pending_effects()[1].id;
    assert!(h.ui.resolve(fast, ServiceResult::Http(ok(202))));
    assert_eq!(h.log(), [Msg::Replied(ok(202))]);
    assert_eq!(url(&h.ui.pending_effects()[0]), "https://a.example/slow");
}

#[test]
fn a_second_answer_to_the_same_request_is_dropped() {
    let mut h = Harness::testing();
    h.want(Want::Get("https://a.example"));
    let id = h.ui.pending_effects()[0].id;
    assert!(h.ui.resolve(id, ServiceResult::Http(ok(200))));
    assert!(!h.ui.resolve(id, ServiceResult::Http(ok(500))));
    assert!(!h.ui.resolve(ServiceId(9999), ServiceResult::Http(ok(500))));
    assert_eq!(h.log().len(), 1);
}

#[test]
fn an_answer_of_the_wrong_kind_is_a_failure_not_a_panic() {
    let mut h = Harness::testing();
    h.want(Want::Get("https://a.example"));
    h.want(Want::StartRecording);
    h.want(Want::Load("k"));
    let http = h.ui.pending_effects()[0].id;
    let start = h.ui.pending_effects()[1].id;
    assert!(h.ui.resolve(http, ServiceResult::Loaded(None)));
    assert!(h.ui.resolve(start, ServiceResult::Http(ok(200))));
    assert!(matches!(
        h.log()[0],
        Msg::Replied(Err(HttpError::Failed(_)))
    ));
    assert!(matches!(
        h.log()[1],
        Msg::RecordingStarted(Err(AudioError::Failed(_)))
    ));
}

#[test]
fn a_recording_is_scripted_from_start_to_bytes() {
    let mut h = Harness::testing();
    h.want(Want::StartRecording);
    assert!(h.ui.answer_recording_started(Ok(())));
    h.want(Want::StopRecording);
    let recording = Recording {
        bytes: vec![9; 64],
        mime: "audio/mp4".into(),
        duration: Duration::from_millis(3000),
    };
    assert!(h.ui.answer_recording(Ok(recording.clone())));
    assert_eq!(
        h.log(),
        [Msg::RecordingStarted(Ok(())), Msg::Recorded(Ok(recording))]
    );
}

#[test]
fn a_refused_microphone_is_an_error_message() {
    let mut h = Harness::testing();
    h.want(Want::StartRecording);
    h.ui.answer_recording_started(Err(AudioError::PermissionDenied));
    h.want(Want::StopRecording);
    h.ui.answer_recording(Err(AudioError::NotRecording));
    assert_eq!(
        h.log(),
        [
            Msg::RecordingStarted(Err(AudioError::PermissionDenied)),
            Msg::Recorded(Err(AudioError::NotRecording)),
        ]
    );
}

#[test]
fn playback_finishes_when_the_test_says_so() {
    let mut h = Harness::testing();
    h.want(Want::Play(b"mp3"));
    assert_eq!(
        h.ui.pending_effects()[0].request,
        ServiceRequest::Play(AudioClip::new(b"mp3".to_vec(), "audio/mpeg"))
    );
    assert!(h.ui.answer_playback(Ok(())));
    assert_eq!(h.log(), [Msg::Played(Ok(()))]);
}

#[test]
fn stopping_playback_interrupts_the_clip_that_is_playing() {
    let mut h = Harness::testing();
    h.want(Want::Play(b"one"));
    h.want(Want::StopPlayback);
    assert!(h.ui.pending_effects().is_empty(), "the clip is over");
    assert!(h.log().is_empty(), "its message comes with the next frame");
    h.frame();
    assert_eq!(h.log(), [Msg::Played(Err(AudioError::Interrupted))]);
}

#[test]
fn a_second_clip_interrupts_the_first() {
    let mut h = Harness::testing();
    h.want(Want::Play(b"one"));
    h.want(Want::Play(b"two"));
    assert_eq!(h.ui.pending_effects().len(), 1);
    assert_eq!(h.ui.deliver_effects(), 1);
    assert_eq!(h.log(), [Msg::Played(Err(AudioError::Interrupted))]);
    h.ui.answer_playback(Ok(()));
    assert_eq!(h.log().len(), 2);
    assert_eq!(h.log()[1], Msg::Played(Ok(())));
}

#[test]
fn stopping_playback_with_nothing_playing_does_nothing() {
    let mut h = Harness::testing();
    h.want(Want::StopPlayback);
    assert_eq!(h.ui.deliver_effects(), 0);
    assert!(h.log().is_empty());
    assert_eq!(h.ui.effects_in_flight(), 0);
}

// --- Storage ---------------------------------------------------------------

#[test]
fn a_stored_value_is_loaded_again() {
    let mut h = Harness::testing();
    h.want(Want::Store("device_id", "abc"));
    assert_eq!(h.ui.stored("device_id"), Some("abc"));
    h.want(Want::Load("device_id"));
    assert!(
        h.ui.pending_effects().is_empty(),
        "the driver answers loads"
    );
    assert!(h.log().is_empty(), "delivered at the start of a frame");
    h.frame();
    assert_eq!(h.log(), [Msg::Loaded(Some("abc".into()))]);
}

#[test]
fn a_key_that_holds_nothing_loads_as_none() {
    let mut h = Harness::testing();
    h.want(Want::Load("missing"));
    assert_eq!(h.ui.deliver_effects(), 1);
    assert_eq!(h.log(), [Msg::Loaded(None)]);
}

#[test]
fn storing_again_replaces_and_removing_forgets() {
    let mut h = Harness::testing();
    h.want(Want::Store("k", "one"));
    h.want(Want::Store("k", "two"));
    assert_eq!(h.ui.stored("k"), Some("two"));
    h.want(Want::Remove("k"));
    assert_eq!(h.ui.stored("k"), None);
    h.want(Want::Load("k"));
    h.ui.deliver_effects();
    assert_eq!(h.log(), [Msg::Loaded(None)]);
}

#[test]
fn a_test_can_put_a_value_there_for_the_application_to_find() {
    let mut h = Harness::testing();
    assert!(h.ui.set_stored("device_id", "from-last-time"));
    h.want(Want::Load("device_id"));
    h.ui.deliver_effects();
    assert_eq!(h.log(), [Msg::Loaded(Some("from-last-time".into()))]);
}

#[test]
fn store_then_load_in_one_update_sees_the_new_value() {
    let mut h = Harness::testing();
    h.ui.app_mut().wanted = vec![Want::Store("k", "v"), Want::Load("k")];
    h.want(Want::Load("k"));
    h.ui.deliver_effects();
    assert_eq!(
        h.log(),
        [Msg::Loaded(Some("v".into())), Msg::Loaded(Some("v".into()))]
    );
}

#[test]
fn setup_reads_what_the_host_stored_and_the_install_id() {
    let mut ui = Ui::testing(Client::default());
    let host = HostState::new()
        .with_install_id("0123456789abcdef0123456789abcdef")
        .with_stored("device_id", "abc");
    let _frame = Frame::for_app_with_host(&mut ui, &host);
    assert_eq!(ui.app().install_id, "0123456789abcdef0123456789abcdef");
    assert_eq!(ui.app().stored_device_id.as_deref(), Some("abc"));
}

#[test]
fn without_a_host_setup_gets_a_well_formed_placeholder_id() {
    let h = Harness::bare();
    assert_eq!(h.ui.app().install_id, "0".repeat(32));
    assert_eq!(h.ui.app().stored_device_id, None);
}

// --- A stand-in host -------------------------------------------------------

fn pretend_server(request: &ServiceRequest) -> Option<ServiceResult> {
    match request {
        ServiceRequest::Http(request) if request.url.ends_with("/slow") => None,
        ServiceRequest::Http(request) => Some(ServiceResult::Http(Ok(
            HttpResponse::new(200).with_body(request.url.clone().into_bytes())
        ))),
        ServiceRequest::StartRecording => Some(ServiceResult::RecordingStarted(Ok(()))),
        _ => None,
    }
}

#[test]
fn a_stand_in_host_answers_at_the_start_of_the_next_frame() {
    let mut h = Harness::new(Ui::with_services(Client::default(), pretend_server));
    h.want(Want::Get("https://a.example/x"));
    assert!(h.log().is_empty(), "not inside the update that asked");
    assert!(h.ui.pending_effects().is_empty(), "and not left pending");
    assert_eq!(h.ui.effects_in_flight(), 1);
    assert!(!h.frame(), "nothing further to deliver");
    assert_eq!(
        h.log(),
        [Msg::Replied(Ok(
            HttpResponse::new(200).with_body(b"https://a.example/x".to_vec())
        ))]
    );
}

#[test]
fn what_the_stand_in_declines_stays_pending_for_the_test() {
    let mut h = Harness::new(Ui::with_services(Client::default(), pretend_server));
    h.want(Want::Get("https://a.example/slow"));
    h.frame();
    assert!(h.log().is_empty());
    assert_eq!(h.ui.pending_effects().len(), 1);
    h.ui.answer_http(Err(HttpError::Timeout));
    assert_eq!(h.log(), [Msg::Replied(Err(HttpError::Timeout))]);
}

#[test]
fn an_answer_that_leads_to_an_answered_request_asks_for_another_frame() {
    let mut h = Harness::new(Ui::with_services(Client::default(), pretend_server));
    h.ui.app_mut().chain = Some("https://a.example/next");
    h.want(Want::Get("https://a.example/first"));
    assert!(h.frame(), "the follow-up's answer is waiting");
    assert_eq!(h.log().len(), 1, "one round per frame");
    h.ui.app_mut().chain = None;
    assert!(!h.frame());
    assert_eq!(h.log().len(), 2);
    assert!(!h.frame(), "idle once everything is delivered");
    assert_eq!(h.log().len(), 2);
}

#[test]
fn delivering_by_hand_stops_even_if_the_application_never_does() {
    let mut h = Harness::new(Ui::with_services(Client::default(), pretend_server));
    h.ui.app_mut().chain = Some("https://a.example/again");
    h.want(Want::Get("https://a.example/first"));
    assert_eq!(h.ui.deliver_effects(), 10_000);
}

// --- No host and no driver -------------------------------------------------

#[test]
fn without_a_host_every_request_fails_as_unsupported_at_the_next_frame() {
    let mut h = Harness::bare();
    h.want(Want::Get("https://a.example"));
    h.want(Want::StartRecording);
    h.want(Want::StopRecording);
    h.want(Want::Play(b"x"));
    h.want(Want::Load("k"));
    h.want(Want::Store("k", "v"));
    h.want(Want::StopPlayback);
    assert!(h.log().is_empty());
    assert!(h.ui.pending_effects().is_empty(), "no driver holds them");
    assert!(!h.frame());
    assert_eq!(
        h.log(),
        [
            Msg::Replied(Err(HttpError::Unsupported)),
            Msg::RecordingStarted(Err(AudioError::Unsupported)),
            Msg::Recorded(Err(AudioError::Unsupported)),
            Msg::Played(Err(AudioError::Unsupported)),
            Msg::Loaded(None),
        ]
    );
    assert_eq!(h.ui.effects_in_flight(), 0, "nothing is left hanging");
    assert_eq!(h.ui.stored("k"), None);
    assert!(!h.ui.set_stored("k", "v"));
}

#[test]
fn without_a_host_requests_can_be_failed_without_building_a_frame() {
    let mut h = Harness::bare();
    h.want(Want::Get("https://a.example"));
    assert_eq!(h.ui.deliver_effects(), 1);
    assert_eq!(h.log(), [Msg::Replied(Err(HttpError::Unsupported))]);
}

#[test]
fn an_application_without_effects_is_idle() {
    let mut h = Harness::bare();
    assert!(!h.frame());
    assert_eq!(h.frame.redraw_after(), None);
    assert_eq!(h.ui.deliver_effects(), 0);
    assert!(h.host_takes().is_empty());
}

// --- Through the `App` interface, as a real host drives it ------------------

#[test]
fn a_host_takes_each_request_once_in_order() {
    let mut h = Harness::bare();
    h.want(Want::Get("https://a.example/1"));
    h.want(Want::Store("k", "v"));
    h.want(Want::Get("https://a.example/2"));
    let taken = h.host_takes();
    assert_eq!(taken.len(), 3);
    assert_eq!(url(&taken[0]), "https://a.example/1");
    assert_eq!(
        taken[1].request,
        ServiceRequest::Store {
            key: "k".into(),
            value: Some("v".into())
        }
    );
    assert_eq!(url(&taken[2]), "https://a.example/2");
    assert!(h.host_takes().is_empty());
}

#[test]
fn with_a_host_a_frame_neither_fails_nor_answers_a_request_in_flight() {
    let mut h = Harness::bare();
    assert!(h.host_takes().is_empty(), "the host announces itself");
    h.want(Want::Get("https://a.example"));
    // A frame before the host has taken the request: it must wait for the
    // host, not be failed.
    assert!(!h.frame());
    assert!(h.log().is_empty());
    assert_eq!(h.host_takes().len(), 1);
    assert!(!h.frame(), "idle while the request is out");
    assert_eq!(h.frame.redraw_after(), None);
    assert!(h.log().is_empty());
}

#[test]
fn a_hosts_answer_is_delivered_between_frames_and_reported_as_delivered() {
    let mut h = Harness::bare();
    h.want(Want::Get("https://a.example"));
    let id = h.host_takes()[0].id;
    assert!(App::service_result(
        &mut h.ui,
        id,
        ServiceResult::Http(ok(200))
    ));
    assert_eq!(h.log(), [Msg::Replied(ok(200))], "before any frame");
    assert!(!App::service_result(
        &mut h.ui,
        id,
        ServiceResult::Http(ok(200))
    ));
    assert!(!App::service_result(
        &mut h.ui,
        ServiceId(4242),
        ServiceResult::Loaded(None)
    ));
    assert_eq!(h.log().len(), 1);
}

#[test]
fn a_request_made_in_answer_to_an_answer_is_there_for_the_host() {
    let mut h = Harness::bare();
    h.ui.app_mut().chain = Some("https://a.example/next");
    h.want(Want::Get("https://a.example/first"));
    let id = h.host_takes()[0].id;
    App::service_result(&mut h.ui, id, ServiceResult::Http(ok(200)));
    let taken = h.host_takes();
    assert_eq!(taken.len(), 1);
    assert_eq!(url(&taken[0]), "https://a.example/next");
    assert!(taken[0].id > id);
}

#[test]
fn a_test_driver_keeps_requests_away_from_a_host() {
    let mut h = Harness::testing();
    h.want(Want::Get("https://a.example"));
    assert!(h.host_takes().is_empty());
    assert_eq!(h.ui.pending_effects().len(), 1);
}

// --- Giving up and tearing down ---------------------------------------------

#[test]
fn forgetting_drops_the_answers_of_everything_asked_before() {
    let mut h = Harness::testing();
    h.want(Want::Get("https://a.example/old"));
    h.want(Want::Load("k"));
    let old = h.ui.pending_effects()[0].id;
    h.ui.app_mut().wanted = vec![Want::Forget, Want::Get("https://a.example/new")];
    h.want(Want::Store("k", "kept"));
    assert_eq!(h.ui.effects_in_flight(), 1, "only the new request");
    assert!(!h.ui.resolve(old, ServiceResult::Http(ok(200))));
    assert_eq!(
        h.ui.deliver_effects(),
        0,
        "the load's answer is dropped too"
    );
    assert!(h.log().is_empty());
    assert_eq!(h.ui.stored("k"), Some("kept"));
    assert!(h.ui.answer_http(ok(204)));
    assert_eq!(h.log(), [Msg::Replied(ok(204))]);
}

#[test]
fn a_host_is_not_given_requests_given_up_on_before_it_took_them() {
    let mut h = Harness::bare();
    h.want(Want::Get("https://a.example/old"));
    h.want(Want::Store("k", "v"));
    h.want(Want::Forget);
    let taken = h.host_takes();
    assert_eq!(taken.len(), 1, "the store still has to happen");
    assert!(matches!(taken[0].request, ServiceRequest::Store { .. }));
}

/// A message-making closure that records being called and being dropped.
struct Witness {
    called: Arc<AtomicUsize>,
    dropped: Arc<AtomicBool>,
}

impl Drop for Witness {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}

struct OneRequest {
    witness: Option<Witness>,
}

impl Application for OneRequest {
    type Message = ();

    fn view(&self) -> Element<()> {
        col![].into()
    }

    fn update(&mut self, _message: ()) {}

    fn effects(&mut self, out: &mut Effects<()>) {
        if let Some(witness) = self.witness.take() {
            out.http(HttpRequest::get("https://a.example"), move |_| {
                witness.called.fetch_add(1, Ordering::SeqCst);
            });
        }
    }
}

#[test]
fn tearing_the_application_down_drops_what_was_waiting_without_calling_it() {
    let called = Arc::new(AtomicUsize::new(0));
    let dropped = Arc::new(AtomicBool::new(false));
    let mut ui = Ui::new(OneRequest {
        witness: Some(Witness {
            called: called.clone(),
            dropped: dropped.clone(),
        }),
    });
    let _frame = Frame::for_app(&mut ui);
    let mut taken = Vec::new();
    App::service_requests(&mut ui, &mut taken);
    assert_eq!(taken.len(), 1, "the request is with the host");
    assert!(!dropped.load(Ordering::SeqCst));
    drop(ui);
    // The host's answer now has nowhere to go; nothing ran, nothing leaked.
    assert_eq!(called.load(Ordering::SeqCst), 0);
    assert!(dropped.load(Ordering::SeqCst));
}

#[test]
fn an_answer_is_given_to_its_closure_exactly_once() {
    let called = Arc::new(AtomicUsize::new(0));
    let dropped = Arc::new(AtomicBool::new(false));
    let mut ui = Ui::testing(OneRequest {
        witness: Some(Witness {
            called: called.clone(),
            dropped: dropped.clone(),
        }),
    });
    let _frame = Frame::for_app(&mut ui);
    assert!(ui.answer_http(ok(200)));
    assert!(!ui.answer_http(ok(200)));
    assert_eq!(called.load(Ordering::SeqCst), 1);
    assert!(dropped.load(Ordering::SeqCst), "and then released");
}

// --- Under a real engine -----------------------------------------------------

/// Registers with the server at startup, using the id it stored last time
/// or else the installation id, and shares what it heard back.
struct Registers {
    device_id: String,
    register: bool,
    heard: Arc<Mutex<Vec<u16>>>,
}

impl Application for Registers {
    type Message = Result<HttpResponse, HttpError>;

    fn setup(&mut self, setup: &mut Setup<'_>) {
        self.device_id = setup
            .stored("device_id")
            .unwrap_or(setup.install_id())
            .to_owned();
        self.register = true;
    }

    fn view(&self) -> Element<Self::Message> {
        col![].into()
    }

    fn update(&mut self, message: Self::Message) {
        if let Ok(response) = message {
            self.heard.lock().unwrap().push(response.status);
        }
    }

    fn effects(&mut self, out: &mut Effects<Self::Message>) {
        if std::mem::take(&mut self.register) {
            out.store("device_id", self.device_id.clone());
            out.http(
                HttpRequest::post("https://a.example/register")
                    .header("X-Device-Id", self.device_id.clone()),
                |result| result,
            );
        }
    }
}

#[test]
fn the_engine_carries_requests_out_and_answers_in() {
    let heard = Arc::new(Mutex::new(Vec::new()));
    let app = Registers {
        device_id: String::new(),
        register: false,
        heard: heard.clone(),
    };
    let host = HostState::new().with_install_id("ffffffffffffffffffffffffffffffff");
    let mut engine = Engine::with_host_state(Box::new(Ui::new(app)), &host);

    let mut calls = Vec::new();
    engine.take_service_requests(&mut calls);
    assert_eq!(calls.len(), 2, "made at setup, there before any frame");
    assert_eq!(
        calls[0].request,
        ServiceRequest::Store {
            key: "device_id".into(),
            value: Some("f".repeat(32)),
        }
    );
    let ServiceRequest::Http(request) = &calls[1].request else {
        panic!("expected the registration");
    };
    assert_eq!(
        request.header_value("x-device-id"),
        Some("f".repeat(32).as_str())
    );

    assert!(engine.service_result(calls[1].id, ServiceResult::Http(ok(201))));
    assert_eq!(*heard.lock().unwrap(), [201]);
    assert!(!engine.service_result(calls[1].id, ServiceResult::Http(ok(201))));
    assert!(!engine.service_result(calls[0].id, ServiceResult::Loaded(None)));
    assert_eq!(*heard.lock().unwrap(), [201]);

    calls.clear();
    engine.take_service_requests(&mut calls);
    assert!(calls.is_empty());
}

#[test]
fn on_its_second_launch_the_application_knows_its_id_at_setup() {
    let app = Registers {
        device_id: String::new(),
        register: false,
        heard: Arc::default(),
    };
    let host = HostState::new()
        .with_install_id("ffffffffffffffffffffffffffffffff")
        .with_stored("device_id", "kept-from-last-time");
    let mut engine = Engine::with_host_state(Box::new(Ui::new(app)), &host);
    let mut calls = Vec::new();
    engine.take_service_requests(&mut calls);
    let ServiceRequest::Http(request) = &calls[1].request else {
        panic!("expected the registration");
    };
    assert_eq!(
        request.header_value("X-Device-Id"),
        Some("kept-from-last-time")
    );
}
