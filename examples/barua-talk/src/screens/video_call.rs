//! Screen 04: a call in the app. `design/html/04-video-call.html`.
//!
//! The other person fills the screen; over them the header with the
//! call's clock, the recording pill, the user's own camera, the volume,
//! and the five controls on a bar with an arched top.
//!
//! **No sound and no pictures flow yet**: the framework has no media
//! client. In the scripted app the screen is the design's frame (its
//! video still of Baraka, its camera tile). In a real call the big
//! picture is the other person's face, the tile says the camera has no
//! picture, and a line says what is missing.

use morningstar::ui::prelude::*;

use crate::components::{back_button_in, control, disc, face};
use crate::icons::{self, icon};
use crate::model::{Call, Face, Recording, Subject, VOLUME_STEPS, clock};
use crate::screens::incoming::backdrop;
use crate::theme::{
    DANGER, INK, PAGE, RECORDING, SURFACE, VEIL_55, VEIL_70, WHITE, grotesk, inter,
};
use crate::{BaruaTalk, Message, NO_MEDIA, Screen};

/// The arched top of the bar: `border-radius: 50% 50% 0 0 / 28px 28px 0
/// 0` on a box 390 wide is the upper half of an ellipse 390 by 56. A box
/// has round corners, not elliptical ones, so the arch is a picture, as
/// wide as the screen and 28/390 of that tall.
const ARCH: VectorPicture = VectorPicture::new(
    390.0,
    28.0,
    &[Layer::path("M0 28A195 28 0 0 1 390 28Z").fill(PAGE)],
);

/// What fills the screen: the design's video frame in the scripted app
/// (`object-fit: cover; object-position: 30% 0`, cut to that window by
/// `design/prepare-images.py`), otherwise the person's backdrop with
/// their face in the middle.
fn remote(app: &BaruaTalk, call: &Call) -> Element<Message> {
    let subject_face = call.subject.face();
    let frame = subject_face
        .photo()
        .filter(|_| call.scripted)
        .and_then(|photo| app.image(photo.on_video()));
    match frame {
        Some(image) => Picture::new(image)
            .fill()
            .semantic_label(format!("{} on video", call.subject.title()))
            .into(),
        None => stack![
            backdrop(app, subject_face),
            Container::new(face(app, subject_face, 124.0))
                .center()
                .fill()
                // Between the camera's tile and the bar, where the
                // screen is free.
                .padding_ltrb(0.0, 220.0, 0.0, 148.0),
        ]
        .fill()
        .into(),
    }
}

/// `left: 20px; right: 20px; top: 56px; display: flex; align-items:
/// center; gap: 10px`: the arrow, the picture with `border: 2px solid
/// #FFFFFF; box-sizing: border-box`, the name over the clock
/// (`font-size: 13px; font-variant-numeric: tabular-nums`, `gap: 2px`),
/// and a 44 dp disc in `rgba(13,15,21,0.55)`.
fn header(app: &BaruaTalk, call: &Call) -> Container<Message> {
    let small: Face = call.subject.face().clone();
    row![
        back_button_in(
            match call.subject {
                Subject::Room { .. } => "Back to chat",
                Subject::Person(_) => "End call and go back",
            },
            WHITE
        ),
        // The border is part of the 44 dp: the picture is the 40 inside.
        Container::new(face(app, &small, 40.0))
            .center()
            .size(44.0, 44.0)
            .radius(22.0)
            .border(2.0, WHITE),
        col![
            grotesk(call.subject.title().to_owned(), 17.0)
                .max_lines(1)
                .fill_width()
                .heading(),
            inter(clock(call.seconds), 13.0)
                .tabular_nums()
                .semantic_label(format!("Call time {}", clock(call.seconds))),
        ]
        .gap(2.0)
        .fill_width(),
        Container::new(icon(&icons::ADD_PERSON, 20.0, WHITE))
            .center()
            .size(44.0, 44.0)
            .radius(22.0)
            .background(VEIL_55)
            .on_press(Message::AddPerson)
            .label("Add someone to the call")
            .id("add-person"),
    ]
    .gap(10.0)
    .cross_align(CrossAlign::Center)
    .fill_width()
    .padding_ltrb(10.0, 56.0, 20.0, 0.0)
}

/// `left: 20px; top: 124px; display: flex; align-items: center; gap: 8px;
/// padding: 8px 12px; border-radius: 999px; background:
/// rgba(13,15,21,0.7); font-size: 12px; color: #FFFFFF` with an 8 dp dot
/// in `#FF5A72`.
fn recording_pill(rate: String) -> Container<Message> {
    row![
        Container::empty()
            .size(8.0, 8.0)
            .radius(4.0)
            .background(RECORDING),
        inter(format!("Recording \u{b7} {rate}"), 12.0).color(WHITE),
    ]
    .gap(8.0)
    .cross_align(CrossAlign::Center)
    .padding_xy(12.0, 8.0)
    .radius(999.0)
    .background(VEIL_70)
    .live(Live::Polite)
    .id("recording-pill")
}

/// `right: 20px; top: 180px; width: 108px; height: 150px; border-radius:
/// 18px; overflow: hidden; border: 2px solid rgba(255,255,255,0.85);
/// box-sizing: border-box` around the user's camera (`object-position:
/// 50% 20%`). With the camera off, or in a real call (there is no
/// picture to show), the raised surface with the camera's icon: the app's
/// own.
fn camera_tile(app: &BaruaTalk, call: &Call) -> Container<Message> {
    let picture = app
        .image(crate::model::Photo::SelfCamera)
        .filter(|_| call.scripted && call.camera);
    let inside: Element<Message> = match picture {
        Some(image) => Picture::new(image)
            .size(104.0, 146.0)
            .radius(16.0)
            .semantic_label("Your camera")
            .into(),
        None => Container::new(icon(
            if call.camera {
                &icons::VIDEO
            } else {
                &icons::CAMERA_OFF
            },
            24.0,
            INK,
        ))
        .center()
        .size(104.0, 146.0)
        .radius(16.0)
        .background(SURFACE)
        .label(if call.camera {
            "Your camera: no picture in this version"
        } else {
            "Your camera is off"
        })
        .into(),
    };
    Container::new(inside)
        .center()
        .size(108.0, 150.0)
        .radius(18.0)
        .border(2.0, with_alpha(WHITE, 0.85))
        .id("camera-tile")
}

/// The volume:
///
/// ```css
/// left: 20px; top: 380px; width: 48px; padding: 14px 0; border-radius: 24px;
/// background: rgba(13,15,21,0.55); display: flex; flex-direction: column;
/// align-items: center; gap: 12px
/// ```
///
/// around a 4 by 120 dp track in `rgba(255,255,255,0.35)`, filled white
/// from the bottom, with an 18 by 6 dp handle, over the loudspeaker. Its
/// upper half turns it up a step and its lower half down (the app's own
/// behaviour; the design draws it at 76 of 120).
fn volume(call: &Call) -> Container<Message> {
    let level = VOLUME_STEPS[call.volume.min(VOLUME_STEPS.len() - 1)];
    let track = stack![
        Container::empty()
            .size(4.0, 120.0)
            .radius(2.0)
            .background(with_alpha(WHITE, 0.35))
            .place(Alignment::TOP_CENTER),
        Container::empty()
            .size(4.0, level)
            .radius(2.0)
            .background(WHITE)
            .place(Alignment::BOTTOM_CENTER),
        // `left: -7px; bottom: 70px; width: 18px; height: 6px`: its
        // bottom edge 6 dp under the top of the filled part.
        Container::empty()
            .size(18.0, 6.0)
            .radius(3.0)
            .background(WHITE)
            .place(Alignment::BOTTOM_CENTER)
            .offset(0.0, -(level - 6.0).max(0.0)),
    ]
    .size(18.0, 120.0);
    let half = |message: Message, id: &'static str| {
        Container::empty()
            .fill_width()
            .height(90.0)
            .on_press(message)
            .exclude_semantics()
            .id(id)
    };
    stack![
        col![track, icon(&icons::SPEAKER, 20.0, WHITE)]
            .gap(12.0)
            .cross_align(CrossAlign::Center)
            .width(48.0)
            .padding_xy(0.0, 14.0)
            .radius(24.0)
            .background(VEIL_55),
        col![
            half(Message::VolumeUp, "volume-up"),
            half(Message::VolumeDown, "volume-down"),
        ]
        .size(48.0, 180.0),
    ]
    .size(48.0, 180.0)
    .role(Role::Slider)
    .label("Speaker volume")
    .value(format!("{} of {}", call.volume, VOLUME_STEPS.len() - 1))
    .on_increment(Message::VolumeUp)
    .on_decrement(Message::VolumeDown)
    .id("volume")
}

/// The bar:
///
/// ```css
/// height: 148px; background: #0D0F15; border-radius: 50% 50% 0 0 / 28px 28px 0 0;
/// display: flex; align-items: center; justify-content: center; gap: 22px;
/// padding-bottom: 16px; box-sizing: border-box
/// ```
///
/// Four 52 dp controls around the 68 dp red button (a 28 dp icon). The
/// record button's dot is `#FF5A72` while recording and `#F4F5F8` when
/// not (`{{recDot}}`): a circle of radius 4 in the icon's 24, so 7.33 dp
/// across in its 22.
fn bar(call: &Call, width: f32) -> Container<Message> {
    let recording = call.recording.is_on();
    let busy = matches!(call.recording, Recording::Starting | Recording::Stopping(_));
    let record = stack![
        control(&icons::RECORD, 52.0, false),
        Container::new(
            Container::empty()
                .size(22.0 / 3.0, 22.0 / 3.0)
                .radius(11.0 / 3.0)
                .background(if recording { RECORDING } else { INK })
        )
        .center()
        .size(52.0, 52.0),
    ]
    .size(52.0, 52.0)
    .on_press_maybe((!busy).then_some(Message::ToggleRecording))
    .label(if recording {
        "Stop recording"
    } else {
        "Start recording"
    })
    .id("record");
    let controls = row![
        control(
            if call.camera {
                &icons::VIDEO
            } else {
                &icons::CAMERA_OFF
            },
            52.0,
            !call.camera
        )
        .on_press(Message::ToggleCamera)
        .label("Camera")
        .role(Role::Switch)
        .checked(call.camera)
        .id("camera"),
        control(&icons::SHARE_SCREEN, 52.0, false)
            .on_press(Message::ShareScreen)
            .label("Share your screen")
            .id("share"),
        disc(&icons::HANG_UP, 68.0, 28.0, DANGER)
            .on_press(Message::HangUp)
            .label("Hang up")
            .id("hang-up"),
        record,
        control(
            if call.muted {
                &icons::MIC_OFF
            } else {
                &icons::MIC
            },
            52.0,
            call.muted
        )
        .on_press(Message::ToggleMute)
        .label("Mute microphone")
        .role(Role::Switch)
        .checked(call.muted)
        .id("mute"),
    ]
    // 22 dp apart where the screen is as wide as the design's; closer on
    // a narrower phone, so that the five still fit.
    .gap(((width - 276.0 - 16.0) / 4.0).clamp(8.0, 22.0))
    .center()
    .fill_width()
    .height(132.0);
    let arch = (28.0 * width / 390.0).round();
    stack![
        // One dp under the arch, so that no seam shows between the two.
        Container::empty()
            .fill()
            .background(PAGE)
            .padding_ltrb(0.0, 0.0, 0.0, 0.0)
            .place(Alignment::TOP_LEFT)
            .offset(0.0, arch - 1.0),
        Picture::vector(ARCH)
            .width(width)
            .place(Alignment::TOP_LEFT),
        col![controls, Spacer::fixed(16.0)].fill_width(),
    ]
    .fill_width()
    .clip()
    .safe_area(SafeEdges::BOTTOM)
    .id("call-bar")
}

/// Everything is placed over the picture, as the design's absolute boxes
/// are: the header 56 dp from the top, the pill at 124, the camera at
/// 180, the volume at 380, the bar at the bottom.
pub fn view(app: &BaruaTalk, viewport: &Viewport<'_>) -> Element<Message> {
    let Some(call) = app.call() else {
        return Container::empty().fill().background(PAGE).into();
    };
    let (width, height) = (viewport.width(), viewport.safe_size().height);
    // On a phone shorter than the design's the volume moves up to stay
    // clear of the bar; at the design's size it is at 380.
    let volume_top = (height - 148.0 - 180.0 - 16.0).clamp(132.0, 380.0);
    let note = (!call.scripted).then(|| {
        Container::new(
            inter(NO_MEDIA, 12.0)
                .line_height_dp(17.0)
                .color(WHITE)
                .fill_width(),
        )
        .width(216.0)
        .padding_xy(12.0, 8.0)
        .radius(16.0)
        .background(VEIL_70)
        .id("no-media")
    });
    let pills = Column::new()
        .push_maybe(
            call.recording
                .is_on()
                .then(|| recording_pill(app.recording_rate())),
        )
        .push_maybe(note)
        .gap(8.0)
        .place(Alignment::TOP_LEFT)
        .offset(20.0, 124.0);
    let overlays = stack![
        header(app, call).place(Alignment::TOP_LEFT),
        pills,
        camera_tile(app, call)
            .place(Alignment::TOP_RIGHT)
            .offset(-20.0, 180.0),
        volume(call)
            .place(Alignment::TOP_LEFT)
            .offset(20.0, volume_top),
    ]
    .fill();
    stack![
        remote(app, call),
        // `height: 200px; background: linear-gradient(#0D0F15,
        // rgba(13,15,21,0))`.
        Container::empty()
            .fill_width()
            .height(200.0 + viewport.insets.top)
            .gradient(Gradient::vertical(PAGE, with_alpha(PAGE, 0.0)))
            .place(Alignment::TOP_LEFT),
        Container::new(overlays).fill().safe_area(SafeEdges::TOP),
        bar(call, width).place(Alignment::BOTTOM_LEFT),
    ]
    .fill()
    .background(PAGE)
    .id(Screen::VideoCall.name())
    .into()
}
