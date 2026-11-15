//! Screen 03: a call that rings. `design/html/03-incoming.html`.
//!
//! The caller's picture, blurred and darkened, fills the screen; over it
//! their picture in two rings, their name, and the buttons that answer.
//! The design shows someone ringing the user. A call the user makes is
//! the same screen with its own words and one button (the app's own).

use morningstar::ui::prelude::*;

use crate::components::{MARGIN, disc, face, pill};
use crate::icons::{self, icon};
use crate::model::{CallStatus, Face};
use crate::theme::{
    ACCENT, ACCEPT, DANGER, OVER_PHOTO, PAGE, VEIL_55, VEIL_60, WHITE, grotesk, inter,
};
use crate::{BaruaTalk, Message, Screen};

/// What is behind the call: the caller's portrait under the design's
/// `filter: blur(40px) brightness(0.45)` (made by
/// `design/prepare-images.py`: the framework blurs a box's background and
/// not a picture), or, for someone the app has no picture of, the page
/// with a glow of the accent where the picture would be (the app's own).
pub fn backdrop(app: &BaruaTalk, face: &Face) -> Element<Message> {
    let blurred = face
        .photo()
        .and_then(|photo| photo.backdrop())
        .and_then(|photo| app.image(photo));
    match blurred {
        Some(image) => Picture::new(image).fill().exclude_semantics().into(),
        None => Container::empty()
            .fill()
            .background(PAGE)
            .radial(
                Radial::circle()
                    .at(0.5, 0.33)
                    .stop(0.0, with_alpha(ACCENT, 0.28))
                    .stop(0.6, with_alpha(ACCENT, 0.0)),
            )
            .into(),
    }
}

/// A round button over its word: `display: flex; flex-direction: column;
/// align-items: center; gap: 10px` with a 72 dp disc, a 30 dp icon and
/// the word in `font-size: 13px; color: #E3E6EE`.
fn answer(
    asset: &'static SvgAsset,
    fill: Color,
    word: &'static str,
    label: &'static str,
    message: Option<Message>,
    id: &'static str,
) -> Container<Message> {
    col![
        disc(asset, 72.0, 30.0, fill),
        inter(word, 13.0).color(OVER_PHOTO),
    ]
    .gap(10.0)
    .cross_align(CrossAlign::Center)
    .on_press_maybe(message)
    .label(label)
    .id(id)
}

/// `padding: 96px 24px 56px; display: flex; flex-direction: column;
/// align-items: center` over the backdrop. `room` is the height the
/// screen has.
pub fn view(app: &BaruaTalk, room: f32) -> Element<Message> {
    let Some(call) = app.call() else {
        return Container::empty().fill().background(PAGE).into();
    };
    let subject = &call.subject;
    let kind = if call.video {
        "In-app video call"
    } else {
        "In-app voice call"
    };
    let status = match (call.incoming, call.status) {
        (true, CallStatus::Starting) => "Inaunganisha\u{2026}",
        (true, _) => "Anakupigia\u{2026}",
        (false, CallStatus::Starting) => "Inaunganisha\u{2026}",
        (false, _) => "Inaita\u{2026}",
    };
    // `margin-top: 56px; width: 184px; height: 184px; border-radius: 50%;
    // border: 1px solid rgba(255,255,255,0.25)` around `152px` with
    // `rgba(255,255,255,0.45)` around the 124 dp picture: divs, so each
    // border adds to its size.
    let rings = Container::new(
        Container::new(face(app, subject.face(), 124.0))
            .center()
            .size(154.0, 154.0)
            .radius(77.0)
            .border(1.0, with_alpha(WHITE, 0.45)),
    )
    .center()
    .size(186.0, 186.0)
    .radius(93.0)
    .border(1.0, with_alpha(WHITE, 0.25));
    let buttons: Element<Message> = if call.incoming {
        col![
            // `width: 100%; padding: 14px 16px; border-radius: 18px;
            // background: rgba(13,15,21,0.6); display: flex; align-items:
            // center; gap: 12px`.
            row![
                icon(&icons::MESSAGE, 20.0, OVER_PHOTO),
                inter("Reply with a message instead", 14.0).fill_width(),
            ]
            .gap(12.0)
            .cross_align(CrossAlign::Center)
            .fill_width()
            .padding_xy(16.0, 14.0)
            .radius(18.0)
            .background(VEIL_60)
            .on_press(Message::ReplyInstead)
            .id("reply-instead"),
            // `margin-top: 32px; width: 100%; display: flex;
            // justify-content: space-between; padding: 0 28px`.
            Spacer::fixed(32.0),
            row![
                answer(
                    &icons::HANG_UP,
                    DANGER,
                    "Decline",
                    "Decline call",
                    Some(Message::Decline),
                    "decline"
                ),
                answer(
                    if call.video {
                        &icons::VIDEO_SMALL
                    } else {
                        &icons::PHONE_SMALL
                    },
                    ACCEPT,
                    "Accept",
                    "Accept call",
                    // An accept on its way is not sent twice.
                    (call.status == CallStatus::Ringing).then_some(Message::Accept),
                    "accept"
                ),
            ]
            .space_between()
            .fill_width()
            .padding_xy(28.0, 0.0),
        ]
        .fill_width()
        .into()
    } else {
        // The app's own: a call the user makes has one button.
        Container::new(answer(
            &icons::HANG_UP,
            DANGER,
            "Cancel",
            "Cancel call",
            Some(Message::Decline),
            "decline",
        ))
        .center()
        .fill_width()
        .into()
    };
    let content = col![
        pill(
            Some(if call.video {
                &icons::VIDEO_SMALL
            } else {
                &icons::PHONE_SMALL
            }),
            inter(kind, 13.0),
            OVER_PHOTO,
            Some(VEIL_55)
        ),
        Spacer::fixed(56.0),
        rings,
        Spacer::fixed(32.0),
        // `font-family: 'Space Grotesk'; font-size: 30px; font-weight: 600`.
        grotesk(subject.title().to_owned(), 30.0)
            .centered()
            .fill_width()
            .heading(),
        Spacer::fixed(8.0),
        inter(status, 15.0)
            .color(OVER_PHOTO)
            .semantic_label(match (call.incoming, call.status) {
                (_, CallStatus::Starting) => "Connecting",
                (true, _) => "is calling you",
                (false, _) => "Ringing",
            })
            .live(Live::Polite),
        // `flex-grow: 1`.
        Spacer::new().min_height(24.0),
        buttons,
    ]
    .cross_align(CrossAlign::Center)
    .fill_width()
    .min_height(room)
    .padding_ltrb(MARGIN, 96.0, MARGIN, 56.0);
    stack![
        backdrop(app, subject.face()),
        Container::new(ScrollView::new(content).id(Screen::Incoming.scroll_id()))
            .fill()
            .safe_area(SafeEdges::ALL),
    ]
    .fill()
    .background(PAGE)
    .id(Screen::Incoming.name())
    .into()
}
