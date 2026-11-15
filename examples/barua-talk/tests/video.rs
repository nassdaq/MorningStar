//! The pictures of a real call (ADR 0039): the two cameras, a shared
//! screen and "someone wrote in the room", driven by fingers, with the
//! stand-in server of `support` behind the app and this test playing the
//! live room: "Baraka's camera started", "the user refused the camera",
//! "someone shares a screen".
//!
//! What is checked is what the app asks of the room and when, that the
//! screen draws a video where its picture belongs once a frame is there
//! and the design's substitute until then, and that it says what is true.
//! No camera, no decoder and no LiveKit are used: what a phone does with
//! the same requests is not tested here (`SCREENS.md`, "The test on a
//! phone").

mod support;

use barua_talk::model::CallStatus;
use barua_talk::{POLL_INCOMING, Pictures, Screen, pictures, sound};
use morningstar::ui::LiveRoom;
use morningstar::{
    DrawOp, LiveData, LiveError, LiveEvent, LiveParticipant, LiveVideoSource, LiveVideoStream,
    VideoFrame, VideoTrack,
};

use support::{Device, Shared, galaxy_a27, lock, world};

const THEIRS: VideoTrack = VideoTrack(11);
const SCREEN: VideoTrack = VideoTrack(12);
const OWN: VideoTrack = VideoTrack(13);

fn room(device: &Device) -> &LiveRoom {
    device.ui.live_room().expect("a live room is joined")
}

fn happens(device: &mut Device, event: LiveEvent) {
    assert!(device.ui.live_event(event), "no live room is joined");
    device.draw();
}

fn stream(track: VideoTrack, source: LiveVideoSource, local: bool) -> LiveVideoStream {
    LiveVideoStream {
        identity: if local { "neema" } else { "baraka" }.into(),
        source,
        local,
        track,
    }
}

fn started(track: VideoTrack, source: LiveVideoSource, local: bool) -> LiveEvent {
    LiveEvent::VideoStarted(stream(track, source, local))
}

fn stopped(track: VideoTrack, source: LiveVideoSource, local: bool) -> LiveEvent {
    LiveEvent::VideoStopped(stream(track, source, local))
}

fn picture() -> VideoFrame {
    VideoFrame::from_rgba8(8, 8, &[90; 8 * 8 * 4]).expect("a frame")
}

/// The videos the last frame drew, in paint order.
fn drawn(device: &Device) -> Vec<VideoTrack> {
    device
        .frame
        .list
        .ops()
        .iter()
        .filter_map(|op| match op {
            DrawOp::Video { track, .. } => Some(*track),
            _ => None,
        })
        .collect()
}

/// Baraka rings, the user accepts, the room connects and Baraka is in it.
fn in_call(world: &Shared, video: bool) -> Device {
    let mut device = Device::online(Screen::Home, galaxy_a27(), world);
    lock(world).ring("c9", "baraka", "Baraka Mushi", video);
    device.ticks(POLL_INCOMING + 3);
    device.tap("accept");
    device.ticks(2);
    assert_eq!(device.app().call().unwrap().status, CallStatus::Active);
    happens(&mut device, LiveEvent::Connected);
    happens(
        &mut device,
        LiveEvent::ParticipantJoined(LiveParticipant {
            identity: "baraka".into(),
            name: "Baraka Mushi".into(),
        }),
    );
    device
}

// --- The cameras -------------------------------------------------------------

#[test]
fn a_call_with_video_joins_with_the_camera_and_a_voice_call_without() {
    assert!(room(&in_call(&world(), true)).join.camera);
    let world = world();
    let voice = in_call(&world, false);
    assert!(!room(&voice).join.camera);
    assert_eq!(voice.app().sound_note(), Some(sound::LIVE));
}

#[test]
fn the_other_sides_camera_fills_the_screen_once_it_has_a_picture() {
    let world = world();
    let mut device = in_call(&world, true);
    assert_eq!(device.app().sound_note(), Some(sound::LIVE_NO_PICTURE));
    assert!(!device.has("their-video"));

    happens(&mut device, started(THEIRS, LiveVideoSource::Camera, false));
    assert_eq!(device.app().sound_note(), Some(sound::LIVE_VIDEO));
    assert!(device.has("their-video"));
    // Announced, no frame yet: the face underneath is what is drawn.
    assert_eq!(drawn(&device), []);

    device.frame.set_video_frame(THEIRS, picture());
    device.draw();
    assert_eq!(drawn(&device), [THEIRS]);
    let whole = device.rect(Screen::VideoCall.name());
    assert_eq!(
        device.rect("their-video"),
        whole,
        "full-bleed, as the design's frame is"
    );

    // Their camera goes off: the face is back, and the note says so.
    happens(&mut device, stopped(THEIRS, LiveVideoSource::Camera, false));
    assert_eq!(drawn(&device), []);
    assert_eq!(device.app().sound_note(), Some(sound::LIVE_NO_PICTURE));
}

#[test]
fn the_users_own_camera_is_in_the_tile_and_the_button_turns_it_off_and_on() {
    let world = world();
    let mut device = in_call(&world, true);
    assert!(room(&device).camera);
    happens(&mut device, started(OWN, LiveVideoSource::Camera, true));
    device.frame.set_video_frame(OWN, picture());
    device.draw();
    assert_eq!(drawn(&device), [OWN]);
    let tile = device.rect("camera-tile");
    let own = device.rect("own-video");
    assert!(
        own.width < tile.width && own.x > tile.x,
        "inside the designed tile"
    );

    device.tap("camera");
    assert!(!room(&device).camera, "the button acts on the room");
    assert!(!device.app().call().unwrap().camera);
    assert_eq!(drawn(&device), [], "off at once, before the room says so");
    happens(&mut device, stopped(OWN, LiveVideoSource::Camera, true));

    device.tap("camera");
    assert!(room(&device).camera);
    happens(
        &mut device,
        started(VideoTrack(14), LiveVideoSource::Camera, true),
    );
    assert_eq!(device.app().pictures().own, Some(VideoTrack(14)));
}

#[test]
fn touching_the_tile_asks_for_the_other_camera() {
    let world = world();
    let mut device = in_call(&world, true);
    happens(&mut device, started(OWN, LiveVideoSource::Camera, true));
    assert_eq!(room(&device).camera_switches, 0);
    device.tap("own-video");
    assert_eq!(room(&device).camera_switches, 1);
}

#[test]
fn a_refused_camera_is_said_and_the_button_goes_back() {
    for (error, sentence) in [
        (LiveError::PermissionDenied, pictures::NO_CAMERA),
        (LiveError::Unsupported, pictures::CAMERA_UNAVAILABLE),
        (
            LiveError::Failed("camera in use".into()),
            pictures::CAMERA_FAILED,
        ),
    ] {
        let world = world();
        let mut device = in_call(&world, true);
        happens(
            &mut device,
            LiveEvent::MediaFailed {
                source: LiveVideoSource::Camera,
                error,
            },
        );
        assert_eq!(device.app().notice(), Some(sentence));
        assert!(
            !device.app().call().unwrap().camera,
            "the button shows what is true"
        );
        assert!(device.ui.live_room().is_some(), "the call goes on");
        assert!(
            !sentence.contains("camera in use"),
            "not the platform's words"
        );
    }
}

// --- Screens -----------------------------------------------------------------

#[test]
fn a_shared_screen_is_shown_whole_in_place_of_the_face() {
    let world = world();
    let mut device = in_call(&world, true);
    happens(&mut device, started(THEIRS, LiveVideoSource::Camera, false));
    happens(&mut device, started(SCREEN, LiveVideoSource::Screen, false));
    assert_eq!(device.app().sound_note(), Some(sound::THEIR_SCREEN));
    device.frame.set_video_frame(THEIRS, picture());
    device.frame.set_video_frame(SCREEN, picture());
    device.draw();
    assert_eq!(drawn(&device), [THEIRS, SCREEN], "the screen is on top");
    // Whole: a square picture in a tall box is as wide as the box.
    let shown = device.frame.list.ops().iter().find_map(|op| match op {
        DrawOp::Video {
            track,
            rect,
            source,
            ..
        } if *track == SCREEN => Some((*rect, *source)),
        _ => None,
    });
    let (rect, source) = shown.unwrap();
    assert_eq!(
        (source.width, source.height),
        (8.0, 8.0),
        "nothing is cut off"
    );
    assert!((rect.width - rect.height).abs() < 1.0);

    happens(&mut device, stopped(SCREEN, LiveVideoSource::Screen, false));
    assert_eq!(drawn(&device), [THEIRS]);
    assert_eq!(device.app().sound_note(), Some(sound::LIVE_VIDEO));
}

#[test]
fn the_share_button_shares_this_phones_screen_and_stops() {
    let world = world();
    let mut device = in_call(&world, true);
    assert!(!room(&device).screen_share);
    device.tap("share");
    assert!(room(&device).screen_share, "asked of the room");
    assert!(
        !device.app().pictures().sharing,
        "not shared until the room says so"
    );

    happens(
        &mut device,
        started(VideoTrack(20), LiveVideoSource::Screen, true),
    );
    assert!(device.app().pictures().sharing);
    assert_eq!(device.app().sound_note(), Some(sound::SHARING));
    assert_eq!(
        drawn(&device),
        [],
        "one's own screen is not shown to oneself"
    );

    device.tap("share");
    assert!(!room(&device).screen_share);
    happens(
        &mut device,
        stopped(VideoTrack(20), LiveVideoSource::Screen, true),
    );
    assert_eq!(device.app().pictures(), &Pictures::default());
}

#[test]
fn a_declined_or_impossible_share_is_said() {
    for (error, sentence) in [
        (LiveError::PermissionDenied, pictures::NO_SCREEN),
        (LiveError::Unsupported, pictures::SCREEN_UNAVAILABLE),
        (LiveError::Failed("x".into()), pictures::SCREEN_FAILED),
    ] {
        let world = world();
        let mut device = in_call(&world, true);
        device.tap("share");
        happens(
            &mut device,
            LiveEvent::MediaFailed {
                source: LiveVideoSource::Screen,
                error,
            },
        );
        assert_eq!(device.app().notice(), Some(sentence));
        assert!(!device.app().pictures().sharing);
    }
}

#[test]
fn sharing_waits_for_the_sound_to_be_live() {
    let world = world();
    let mut device = Device::online(Screen::Home, galaxy_a27(), &world);
    lock(&world).ring("c9", "baraka", "Baraka Mushi", true);
    device.ticks(POLL_INCOMING + 3);
    device.tap("accept");
    device.ticks(2);
    // Joining, not connected.
    device.tap("share");
    assert!(!room(&device).screen_share);
    assert_eq!(device.app().notice(), Some(pictures::SCREEN_NEEDS_SOUND));
}

// --- The end -----------------------------------------------------------------

#[test]
fn hanging_up_forgets_the_pictures() {
    let world = world();
    let mut device = in_call(&world, true);
    happens(&mut device, started(THEIRS, LiveVideoSource::Camera, false));
    happens(&mut device, started(OWN, LiveVideoSource::Camera, true));
    device.tap("hang-up");
    device.ticks(2);
    assert!(device.ui.live_room().is_none());
    assert_eq!(device.app().pictures(), &Pictures::default());
}

// --- Chat through the room ---------------------------------------------------

#[test]
fn data_on_the_chat_topic_asks_for_the_rooms_messages_at_once() {
    let world = world();
    let mut device = Device::online(Screen::Home, galaxy_a27(), &world);
    device.tap(("item", 0usize));
    device.ticks(2);
    device.tap("room-video");
    device.ticks(2);
    assert_eq!(device.screen(), Screen::VideoCall);
    happens(&mut device, LiveEvent::Connected);

    let before = lock(&world).asked("/messages");
    let data = |topic: &str| {
        LiveEvent::Data(LiveData {
            from: "baraka".into(),
            topic: topic.into(),
            payload: br#"{"from":"baraka","text":"Karibu!"}"#.to_vec(),
        })
    };
    // Something else on the wire is not chat.
    happens(&mut device, data("cursor"));
    assert_eq!(lock(&world).asked("/messages"), before);

    lock(&world).say("support-desk", "baraka", "Baraka Mushi", "Karibu!");
    happens(&mut device, data("chat"));
    assert_eq!(
        lock(&world).asked("/messages"),
        before + 1,
        "asked for without waiting for the poll"
    );
}

// --- On the GPU ---------------------------------------------------------------

/// A frame of one colour with a lighter band across its top, so that a
/// picture shown upside down or cut wrongly is seen.
fn coloured(rgb: [u8; 3]) -> VideoFrame {
    let (width, height) = (96u32, 128u32);
    let mut pixels = Vec::new();
    for y in 0..height {
        for _ in 0..width {
            let lift = if y < 16 { 60 } else { 0 };
            pixels.extend_from_slice(&[
                rgb[0].saturating_add(lift),
                rgb[1].saturating_add(lift),
                rgb[2].saturating_add(lift),
                255,
            ]);
        }
    }
    VideoFrame::from_rgba8(width, height, &pixels).expect("a frame")
}

#[test]
fn the_call_screen_shows_both_pictures_where_the_design_has_them() {
    use morningstar_devtools::{Image, screenshot_frame_with_videos};
    let world = world();
    let mut device = in_call(&world, true);
    happens(&mut device, started(THEIRS, LiveVideoSource::Camera, false));
    happens(&mut device, started(OWN, LiveVideoSource::Camera, true));
    let (blue, green) = ([30, 60, 200], [30, 160, 70]);
    let videos = [(THEIRS, coloured(blue)), (OWN, coloured(green))];
    let png = match screenshot_frame_with_videos(&mut device.ui, &device.info, &videos) {
        Ok(png) => png,
        // No GPU on this machine: the other tests have said what is drawn.
        Err(error) => {
            eprintln!("skipping: {error}");
            return;
        }
    };
    // Kept for a person to look at: target/barua-talk-video-call.png.
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    let _ = std::fs::write(out.join("barua-talk-video-call.png"), &png);
    let shot = Image::from_png(&png).expect("a PNG");
    let near = |x: f32, y: f32, want: [u8; 3]| {
        let got = shot.pixel(x as u32, y as u32);
        got[..3].iter().zip(want).all(|(a, b)| a.abs_diff(b) <= 6)
    };
    // The other side fills the screen: its colour in the free middle.
    let whole = device.rect(Screen::VideoCall.name());
    assert!(near(whole.width * 0.5, whole.height * 0.55, blue));
    // The user's own camera is in the tile.
    let tile = device.rect("own-video");
    assert!(near(
        tile.x + tile.width / 2.0,
        tile.y + tile.height / 2.0,
        green
    ));
}
