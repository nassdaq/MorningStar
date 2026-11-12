//! The smoke test's application driven with no device: the test plays the
//! server, the microphone and the speaker.

use std::time::Duration;

use morningstar::ui::Ui;
use morningstar::{App, Frame, FrameInfo, HostState, SurfaceSize};
use morningstar::{
    AudioClip, AudioError, HttpError, HttpResponse, Recording, ServiceRequest, ServiceResult,
};
use services_check::{LAUNCHES_KEY, Message, RECORD_SECONDS, ServicesCheck, URL};

const SIZE: SurfaceSize = SurfaceSize::new(1080, 2400);

struct Phone {
    ui: Ui<ServicesCheck>,
    frame: Frame,
    elapsed: Duration,
}

impl Phone {
    fn new(mut ui: Ui<ServicesCheck>, host: &HostState) -> Self {
        let frame = Frame::for_app_with_host(&mut ui, host);
        Self {
            ui,
            frame,
            elapsed: Duration::ZERO,
        }
    }

    fn fresh() -> Self {
        Self::new(Ui::testing(ServicesCheck::new()), &HostState::new())
    }

    fn frame_after(&mut self, wait: Duration) {
        self.elapsed += wait;
        self.frame.begin();
        let info = FrameInfo::new(SIZE)
            .with_scale(3.0)
            .with_elapsed(self.elapsed);
        self.ui.frame(&info, &mut self.frame);
    }
}

#[test]
fn the_server_replies_and_the_result_line_says_so() {
    let mut phone = Phone::fresh();
    phone.ui.send(Message::Get);
    assert_eq!(phone.ui.app().http, "waiting for the server...");
    let pending = phone.ui.pending_effects();
    assert_eq!(pending.len(), 1);
    let ServiceRequest::Http(request) = &pending[0].request else {
        panic!("expected a request");
    };
    assert_eq!(request.url, URL);
    assert_eq!(request.timeout, Duration::from_secs(15));

    phone.ui.send(Message::Get);
    assert_eq!(phone.ui.pending_effects().len(), 1, "one at a time");

    phone.ui.answer_http(Ok(HttpResponse::new(200)
        .with_header("Content-Type", "text/html")
        .with_body(vec![b'x'; 1256])));
    assert_eq!(phone.ui.app().http, "status 200, 1256 bytes, text/html");
}

#[test]
fn being_offline_is_a_readable_line_not_a_hang() {
    let mut phone = Phone::fresh();
    phone.ui.send(Message::Get);
    phone.ui.answer_http(Err(HttpError::Offline));
    assert_eq!(
        phone.ui.app().http,
        "failed: the server could not be reached"
    );
    phone.ui.send(Message::Get);
    assert_eq!(phone.ui.pending_effects().len(), 1, "and it can be retried");
}

#[test]
fn three_seconds_are_recorded_and_played_back() {
    let mut phone = Phone::fresh();
    phone.ui.send(Message::Record);
    assert!(phone.ui.answer_recording_started(Ok(())));
    assert_eq!(phone.ui.app().seconds_left, RECORD_SECONDS);

    // The timer counts the seconds down and then stops the recording.
    phone.frame_after(Duration::ZERO);
    for _ in 0..RECORD_SECONDS {
        assert!(phone.ui.pending_effects().is_empty());
        phone.frame_after(Duration::from_secs(1));
    }
    assert_eq!(
        phone.ui.pending_effects()[0].request,
        ServiceRequest::StopRecording
    );
    let recording = Recording {
        bytes: vec![7; 12_000],
        mime: "audio/mp4".into(),
        duration: Duration::from_millis(3020),
    };
    assert!(phone.ui.answer_recording(Ok(recording)));
    assert_eq!(phone.ui.app().recording, "12000 bytes of audio/mp4, 3.0 s");

    phone.ui.send(Message::Play);
    assert_eq!(
        phone.ui.pending_effects()[0].request,
        ServiceRequest::Play(AudioClip::new(vec![7; 12_000], "audio/mp4"))
    );
    assert!(phone.ui.answer_playback(Ok(())));
    assert_eq!(phone.ui.app().playback, "played to the end");
}

#[test]
fn a_refused_microphone_is_said_and_nothing_counts_down() {
    let mut phone = Phone::fresh();
    phone.ui.send(Message::Record);
    phone
        .ui
        .answer_recording_started(Err(AudioError::PermissionDenied));
    assert_eq!(
        phone.ui.app().recording,
        "could not record: the microphone is not allowed"
    );
    assert_eq!(phone.ui.app().seconds_left, 0);
    phone.frame_after(Duration::from_secs(5));
    assert!(phone.ui.pending_effects().is_empty());
}

#[test]
fn stopping_ends_the_clip_as_interrupted() {
    let mut phone = Phone::fresh();
    phone.ui.app_mut().last_recording = Some(Recording {
        bytes: vec![1],
        mime: "audio/mp4".into(),
        duration: Duration::from_secs(1),
    });
    phone.ui.send(Message::Play);
    phone.ui.send(Message::StopPlaying);
    phone.ui.deliver_effects();
    assert_eq!(phone.ui.app().playback, "ended: playback was interrupted");
    assert!(!phone.ui.app().playing);
}

#[test]
fn a_number_is_stored_and_loaded_back() {
    let mut phone = Phone::fresh();
    phone.ui.send(Message::StoreLoad);
    assert_eq!(phone.ui.stored(LAUNCHES_KEY), Some("1"));
    phone.frame_after(Duration::ZERO);
    assert_eq!(phone.ui.app().storage, "loaded 1");
    phone.ui.send(Message::StoreLoad);
    phone.ui.deliver_effects();
    assert_eq!(phone.ui.app().storage, "loaded 2");
}

#[test]
fn on_a_later_launch_setup_already_knows_the_count_and_the_id() {
    let host = HostState::new()
        .with_install_id("0123456789abcdef0123456789abcdef")
        .with_stored(LAUNCHES_KEY, "41");
    let mut phone = Phone::new(Ui::testing(ServicesCheck::new()), &host);
    assert_eq!(
        phone.ui.app().install_id,
        "0123456789abcdef0123456789abcdef"
    );
    assert_eq!(phone.ui.app().launches_at_start.as_deref(), Some("41"));
    phone.ui.send(Message::StoreLoad);
    assert_eq!(phone.ui.stored(LAUNCHES_KEY), Some("42"));
}

#[test]
fn a_pretend_server_can_answer_everything() {
    let ui = Ui::with_services(ServicesCheck::new(), |request| match request {
        ServiceRequest::Http(_) => Some(ServiceResult::Http(Ok(HttpResponse::new(503)))),
        ServiceRequest::StartRecording => {
            Some(ServiceResult::RecordingStarted(Err(AudioError::Busy)))
        }
        _ => None,
    });
    let mut phone = Phone::new(ui, &HostState::new());
    phone.ui.send(Message::Get);
    phone.ui.send(Message::Record);
    phone.frame_after(Duration::ZERO);
    assert_eq!(phone.ui.app().http, "status 503, 0 bytes, no content type");
    assert_eq!(
        phone.ui.app().recording,
        "could not record: the microphone or speaker is in use"
    );
}

#[test]
fn with_no_host_at_all_every_button_reports_unsupported() {
    let mut phone = Phone::new(Ui::new(ServicesCheck::new()), &HostState::new());
    phone.ui.send(Message::Get);
    phone.ui.send(Message::Record);
    phone.ui.send(Message::StoreLoad);
    phone.frame_after(Duration::ZERO);
    let app = phone.ui.app();
    assert_eq!(app.http, "failed: HTTP requests are not available here");
    assert_eq!(
        app.recording,
        "could not record: audio is not available here"
    );
    assert_eq!(app.storage, "loaded nothing");
    assert!(phone.ui.overflows().is_empty());
}
