//! What a live room carries besides sound (ADR 0039): messages to the
//! room, this device's camera and screen, and the pictures of the videos
//! in it, drawn by `LiveVideo`. Driven by the test driver, through an
//! `Engine` as a host drives a `Ui`, and rendered on the GPU.

use morningstar_core::{DrawOp, SurfaceSize};
use morningstar_devtools::{Image as Screenshot, screenshot_frame_with_videos};
use morningstar_runtime::{Engine, Frame, FrameInfo, GpuError, VideoFrame};
use morningstar_ui::prelude::*;

const SIZE: SurfaceSize = SurfaceSize::new(200, 300);
const THEIRS: VideoTrack = VideoTrack(3);

#[derive(Clone, Debug, PartialEq)]
enum Want {
    Join,
    Say(&'static str),
    SayTooMuch,
    Camera(bool),
    Switch,
    Share(bool),
}

#[derive(Clone, Debug, PartialEq)]
enum Msg {
    Do(Want),
    Live(LiveEvent),
}

/// Joins a room, does what it is asked, and shows the first remote video
/// it is told about over a red box.
#[derive(Default)]
struct Viewer {
    wanted: Vec<Want>,
    log: Vec<LiveEvent>,
    showing: Option<VideoTrack>,
}

impl Application for Viewer {
    type Message = Msg;

    fn view(&self) -> Element<Msg> {
        let red = Container::new(Spacer::new())
            .fill()
            .background(Color::rgb(1.0, 0.0, 0.0));
        match self.showing {
            Some(track) => stack![red, LiveVideo::new(track).fill()].into(),
            None => stack![red].into(),
        }
    }

    fn update(&mut self, message: Msg) {
        match message {
            Msg::Do(want) => self.wanted.push(want),
            Msg::Live(event) => {
                match &event {
                    LiveEvent::VideoStarted(video) if !video.local => {
                        self.showing = Some(video.track);
                    }
                    LiveEvent::VideoStopped(video) if Some(video.track) == self.showing => {
                        self.showing = None;
                    }
                    _ => {}
                }
                self.log.push(event);
            }
        }
    }

    fn effects(&mut self, out: &mut Effects<Msg>) {
        for want in self.wanted.drain(..) {
            match want {
                Want::Join => out.join_live(
                    LiveJoin::new("wss://live.example.com", "ticket").camera(true),
                    Msg::Live,
                ),
                Want::Say(text) => out.send_live_data("chat", text.as_bytes(), true),
                Want::SayTooMuch => out.send_live_data("chat", vec![0; 20_000], true),
                Want::Camera(on) => out.set_live_camera(on),
                Want::Switch => out.switch_live_camera(),
                Want::Share(on) => out.set_live_screen_share(on),
            }
        }
    }
}

fn joined() -> Ui<Viewer> {
    let mut ui = Ui::testing(Viewer::default());
    ui.send(Msg::Do(Want::Join));
    assert!(ui.live_event(LiveEvent::Connected));
    ui
}

fn their(source: LiveVideoSource) -> LiveVideoStream {
    LiveVideoStream {
        identity: "baraka".into(),
        source,
        local: false,
        track: THEIRS,
    }
}

fn picture(rgb: [u8; 3]) -> VideoFrame {
    let pixels: Vec<u8> = (0..16 * 16)
        .flat_map(|_| [rgb[0], rgb[1], rgb[2], 255])
        .collect();
    VideoFrame::from_rgba8(16, 16, &pixels).expect("a frame")
}

fn draws_video(frame: &Frame) -> usize {
    frame
        .list
        .ops()
        .iter()
        .filter(|op| matches!(op, DrawOp::Video { .. }))
        .count()
}

// --- Messages to the room ---------------------------------------------------

#[test]
fn what_is_sent_reaches_the_room_and_what_arrives_is_a_message() {
    let mut ui = joined();
    ui.send(Msg::Do(Want::Say("habari")));
    ui.send(Msg::Do(Want::Say("karibu")));
    let room = ui.live_room().expect("joined");
    assert_eq!(
        room.sent,
        [
            ("chat".to_owned(), b"habari".to_vec()),
            ("chat".to_owned(), b"karibu".to_vec())
        ]
    );

    let data = LiveData {
        from: "baraka".into(),
        topic: "chat".into(),
        payload: b"nzuri".to_vec(),
    };
    assert!(ui.live_event(LiveEvent::Data(data.clone())));
    assert_eq!(ui.app().log.last(), Some(&LiveEvent::Data(data)));
    assert!(ui.live_room().is_some(), "a message does not end the room");
}

#[test]
fn a_message_that_is_too_long_or_has_no_room_is_not_sent() {
    let mut ui = joined();
    ui.send(Msg::Do(Want::SayTooMuch));
    assert!(ui.live_room().expect("joined").sent.is_empty());

    let mut nowhere = Ui::testing(Viewer::default());
    nowhere.send(Msg::Do(Want::Say("habari")));
    assert!(nowhere.live_room().is_none());
    assert_eq!(
        nowhere.pending_effects().len(),
        0,
        "nothing waits for an answer"
    );
}

// --- The camera and the screen ----------------------------------------------

#[test]
fn the_camera_and_the_screen_are_set_on_the_room() {
    let mut ui = joined();
    let room = |ui: &Ui<Viewer>| ui.live_room().cloned().expect("joined");
    assert!(room(&ui).camera, "joined with the camera on");
    assert!(!room(&ui).screen_share);

    ui.send(Msg::Do(Want::Camera(false)));
    ui.send(Msg::Do(Want::Switch));
    ui.send(Msg::Do(Want::Share(true)));
    let now = room(&ui);
    assert!(!now.camera && now.screen_share);
    assert_eq!(now.camera_switches, 1);

    ui.send(Msg::Do(Want::Share(false)));
    assert!(!room(&ui).screen_share);
}

#[test]
fn a_refused_camera_or_screen_is_a_message_and_the_room_goes_on() {
    let mut ui = joined();
    for (source, error) in [
        (LiveVideoSource::Camera, LiveError::PermissionDenied),
        (LiveVideoSource::Screen, LiveError::Unsupported),
    ] {
        let refused = LiveEvent::MediaFailed { source, error };
        assert!(ui.live_event(refused.clone()));
        assert_eq!(ui.app().log.last(), Some(&refused));
    }
    assert!(ui.live_room().is_some());
}

// --- Pictures ---------------------------------------------------------------

#[test]
fn a_video_is_drawn_once_it_has_a_picture_and_not_before_or_after() {
    let mut ui = joined();
    let info = FrameInfo::new(SIZE);
    let mut frame = Frame::for_app(&mut ui);
    let build = |ui: &mut Ui<Viewer>, frame: &mut Frame| {
        frame.begin();
        morningstar_runtime::App::frame(ui, &info, frame);
        draws_video(frame)
    };
    assert_eq!(build(&mut ui, &mut frame), 0, "no video yet");

    // Announced, but no frame has arrived: what is underneath shows.
    ui.live_event(LiveEvent::VideoStarted(their(LiveVideoSource::Camera)));
    assert_eq!(build(&mut ui, &mut frame), 0);

    frame.set_video_frame(THEIRS, picture([0, 0, 255]));
    assert_eq!(build(&mut ui, &mut frame), 1);
    let fitted = frame.list.ops().iter().find_map(|op| match op {
        DrawOp::Video {
            track,
            rect,
            source,
            ..
        } => Some((*track, *rect, *source)),
        _ => None,
    });
    let (track, rect, source) = fitted.expect("drawn");
    assert_eq!(track, THEIRS);
    assert_eq!((rect.width, rect.height), (200.0, 300.0), "fills its box");
    // Cover: a square picture in a tall box loses its sides.
    assert!(source.width < 16.0 && (source.height - 16.0).abs() < 0.01);

    ui.live_event(LiveEvent::VideoStopped(their(LiveVideoSource::Camera)));
    assert_eq!(build(&mut ui, &mut frame), 0);
}

#[test]
fn a_shared_screen_is_a_video_like_any_other() {
    let mut ui = joined();
    ui.live_event(LiveEvent::VideoStarted(their(LiveVideoSource::Screen)));
    assert_eq!(ui.app().showing, Some(THEIRS));
    let mut frame = Frame::for_app(&mut ui);
    frame.set_video_frame(THEIRS, picture([10, 200, 10]));
    frame.begin();
    morningstar_runtime::App::frame(&mut ui, &FrameInfo::new(SIZE), &mut frame);
    assert_eq!(draws_video(&frame), 1);
}

#[test]
fn this_devices_own_video_is_told_apart() {
    let mut ui = joined();
    let own = LiveVideoStream {
        identity: "me".into(),
        source: LiveVideoSource::Camera,
        local: true,
        track: VideoTrack(1),
    };
    ui.live_event(LiveEvent::VideoStarted(own));
    assert_eq!(
        ui.app().showing,
        None,
        "the viewer shows the other side only"
    );
}

#[test]
fn the_picture_is_what_reaches_the_screen() {
    let mut ui = joined();
    ui.live_event(LiveEvent::VideoStarted(their(LiveVideoSource::Camera)));
    let info = FrameInfo::new(SIZE);
    let shot = |ui: &mut Ui<Viewer>, videos: &[(VideoTrack, VideoFrame)]| {
        match screenshot_frame_with_videos(ui, &info, videos) {
            Ok(png) => Some(Screenshot::from_png(&png).expect("a PNG")),
            Err(GpuError::NoAdapter(reason)) => {
                eprintln!("skipping: no GPU adapter available ({reason})");
                None
            }
            Err(other) => panic!("render failed: {other}"),
        }
    };
    let Some(before) = shot(&mut ui, &[]) else {
        return;
    };
    let [r, _, b, _] = before.pixel(100, 150);
    assert!(
        r > 240 && b < 10,
        "the box underneath, while there is no frame"
    );

    let blue = [20, 50, 220];
    let with = shot(&mut ui, &[(THEIRS, picture(blue))]).expect("a GPU");
    for (x, y) in [(2, 2), (100, 150), (197, 297)] {
        let got = with.pixel(x, y);
        let close = got[..3].iter().zip(blue).all(|(a, b)| a.abs_diff(b) <= 4);
        assert!(close, "({x}, {y}) is {got:?}");
    }
}

// --- Under an engine --------------------------------------------------------

#[test]
fn under_an_engine_a_new_picture_asks_for_a_frame_only_while_it_is_shown() {
    let app = Viewer {
        wanted: vec![Want::Join],
        ..Viewer::default()
    };
    let mut engine = Engine::new(Box::new(Ui::new(app)));
    let mut calls = Vec::new();
    engine.take_service_requests(&mut calls);
    let join = calls[0].id;
    // No surface: nothing is ever drawn, but what is wanted is still known
    // from the list the engine last built, which is empty here.
    engine.service_result(join, ServiceResult::Live(LiveEvent::Connected));
    assert!(engine.video_frame(THEIRS, picture([1, 2, 3])).is_none());
    assert!(engine.video_frame(THEIRS, picture([4, 5, 6])).is_some());
    engine.video_ended(THEIRS);
    engine.videos_ended();
}
