//! Screen 02: a room's chat. `design/html/02-room.html`.
//!
//! The room's name with the two ways to call from it, the conversation,
//! and the message field. The header and the field stay put; the
//! conversation scrolls and follows the newest message.

use morningstar::ui::prelude::*;

use crate::components::{ROOM_MARGIN, TOP, back_button, face, hairline, icon_button, page, pill};
use crate::icons::{self, icon};
use crate::model::{Body, ChatMessage, Delivery, Entry, Loaded, Room, short_clock};
use crate::theme::{
    ACCENT, INK, MUTED, PAGE, QUIET, RADIUS_BUBBLE, RADIUS_TAIL, SOFT, SURFACE, WHITE, grotesk,
    inter, inter_baseline_drop,
};
use crate::{BaruaTalk, Message, Screen, TICKS_PER_SECOND};

/// The header:
///
/// ```css
/// display: flex; align-items: center; gap: 10px; padding-bottom: 18px;
/// border-bottom: 1px solid #1F2330
/// ```
///
/// The arrow (`margin-left: -10px`) and the video button (`margin-right:
/// -8px`) are 44 dp targets that reach into the page's 20 dp margin.
fn header(app: &BaruaTalk, room: &Room) -> Container<Message> {
    col![
        row![
            back_button("Back to home"),
            face(app, &room.face, 44.0),
            col![
                // `font-family: 'Space Grotesk'; font-size: 17px;
                // font-weight: 600`.
                grotesk(room.title.clone(), 17.0)
                    .max_lines(1)
                    .fill_width()
                    .heading(),
                inter(room.status.clone(), 12.0).color(MUTED),
            ]
            .gap(2.0)
            .fill_width(),
            icon_button(
                &icons::PHONE,
                44.0,
                22.0,
                INK,
                "Add a phone call to this room",
                Message::RoomPhone
            )
            .id("room-phone"),
            icon_button(
                &icons::VIDEO,
                44.0,
                22.0,
                INK,
                "Start video in this room",
                Message::RoomVideo
            )
            .id("room-video"),
        ]
        .gap(10.0)
        .cross_align(CrossAlign::Center)
        .fill_width()
        .padding_ltrb(10.0, TOP, 12.0, 18.0),
        Container::new(hairline())
            .fill_width()
            .padding_xy(ROOM_MARGIN, 0.0),
    ]
    .fill_width()
}

/// `display: flex; align-items: baseline; gap: 8px`: the author in
/// `font-size: 13px; font-weight: 600` and the time in `font-size: 12px;
/// color: #9AA1B2`, on one baseline.
fn byline(message: &ChatMessage) -> Container<Message> {
    row![
        inter(message.author.clone(), 13.0).semibold(),
        Container::new(inter(message.time.clone(), 12.0).color(MUTED)).padding_ltrb(
            0.0,
            inter_baseline_drop(13.0, 12.0),
            0.0,
            0.0
        ),
    ]
    .gap(8.0)
    .cross_align(CrossAlign::Start)
}

/// The heights of the sixteen bars of a voice message, as the design's
/// two paths draw them (`M2 12v8M10 8v16…`): each a 3 dp round-capped
/// stroke 8 dp from the last, its length here plus the caps.
const BARS: [f32; 16] = [
    8.0, 16.0, 24.0, 12.0, 20.0, 28.0, 14.0, 22.0, 10.0, 18.0, 8.0, 14.0, 24.0, 12.0, 6.0, 10.0,
];
/// How many of the bars the design draws in the accent: the part played.
const PLAYED_BARS: usize = 7;

/// The waveform: `<svg width="130" height="32" stroke-width="3"
/// stroke-linecap="round">` with the first seven bars in the accent and
/// the other nine in `#4A5166`. Drawn as sixteen boxes, so that the part
/// in the accent can follow the message as it plays; a round-capped
/// stroke 3 wide is a box 3 wide with round ends.
fn waveform(played: usize) -> Container<Message> {
    let mut bars = Stack::new().size(130.0, 32.0);
    for (index, length) in BARS.into_iter().enumerate() {
        let color = if index < played { ACCENT } else { QUIET };
        bars = bars.push(
            Container::empty()
                .size(3.0, length + 3.0)
                .radius(1.5)
                .background(color)
                .place(Alignment::TOP_LEFT)
                .offset(0.5 + 8.0 * index as f32, (32.0 - length - 3.0) / 2.0),
        );
    }
    bars.exclude_semantics()
}

/// What a voice message holds: `display: flex; align-items: center; gap:
/// 12px` with a 44 dp play button in the accent, the waveform, and the
/// length (`font-size: 12px; color: #9AA1B2`).
fn voice(app: &BaruaTalk, room: &Room, index: usize, seconds: u32) -> Container<Message> {
    let playing = app
        .playing()
        .filter(|playing| playing.room == room.name && playing.index == index);
    let (played, left, label) = match playing {
        Some(playing) => {
            let total = (seconds * TICKS_PER_SECOND).max(1);
            let played = (playing.ticks * BARS.len() as u32 / total) as usize;
            (
                played,
                seconds.saturating_sub(playing.ticks / TICKS_PER_SECOND),
                "Stop voice message",
            )
        }
        None => (PLAYED_BARS, seconds, "Play voice message"),
    };
    row![
        Container::new(icon(
            if playing.is_some() {
                &icons::CLOSE
            } else {
                &icons::PLAY
            },
            18.0,
            WHITE
        ))
        .center()
        .size(44.0, 44.0)
        .radius(22.0)
        .background(ACCENT)
        .on_press(Message::PlayVoice(index))
        .label(label)
        .id(("play", index)),
        waveform(played),
        inter(short_clock(left), 12.0).color(MUTED),
    ]
    .gap(12.0)
    .cross_align(CrossAlign::Center)
}

/// Someone else's message: `display: flex; align-items: flex-start; gap:
/// 10px` with a 36 dp picture and a bubble, `background: #171A24;
/// border-radius: 6px 20px 20px 20px; display: flex; flex-direction:
/// column; gap: 10px`.
fn theirs(app: &BaruaTalk, room: &Room, index: usize, message: &ChatMessage) -> Container<Message> {
    let bubble = |content: Container<Message>| {
        content
            .radii(RADIUS_TAIL, RADIUS_BUBBLE, RADIUS_BUBBLE, RADIUS_BUBBLE)
            .background(SURFACE)
    };
    let bubble = match &message.body {
        // `padding: 12px 14px`.
        Body::Voice(seconds) => bubble(
            col![byline(message), voice(app, room, index, *seconds)]
                .gap(10.0)
                .padding_xy(14.0, 12.0),
        ),
        // `padding: 12px; width: 260px; box-sizing: border-box`, the
        // byline and the caption with `padding: 0 2px`, the picture
        // `width: 100%; height: 168px; border-radius: 12px`.
        Body::Photo {
            photo,
            alt,
            caption,
        } => bubble(
            Column::new()
                .push(Container::new(byline(message)).padding_xy(2.0, 0.0))
                .push_maybe(app.image(*photo).map(|image| {
                    Picture::new(image)
                        .size(236.0, 168.0)
                        .radius(12.0)
                        .semantic_label(*alt)
                }))
                .push(
                    Container::new(
                        inter(caption.clone(), 14.0)
                            .line_height_dp(20.0)
                            .fill_width(),
                    )
                    .fill_width()
                    .padding_xy(2.0, 0.0),
                )
                .gap(10.0)
                .width(260.0)
                .padding(12.0),
        ),
        // The app's own, for a message of words from someone else (the
        // design shows a voice message and a picture): the first bubble's
        // padding and the caption's type.
        Body::Text(words) => bubble(
            col![
                byline(message),
                inter(words.clone(), 14.0).line_height_dp(20.0),
            ]
            .gap(6.0)
            .padding_xy(14.0, 12.0)
            .max_width(264.0),
        ),
    };
    row![face(app, &message.face, 36.0), bubble]
        .gap(10.0)
        .cross_align(CrossAlign::Start)
        .fill_width()
        .id(("entry", index))
}

/// The user's own message: `align-self: flex-end; display: flex;
/// flex-direction: column; align-items: flex-end; gap: 6px` with a bubble
/// `background: {{accent}}; color: #FFFFFF; border-radius: 20px 20px 6px
/// 20px; padding: 12px 16px; font-size: 14px; line-height: 20px;
/// max-width: 250px` and, under it, how far it has got (`font-size: 12px;
/// color: #9AA1B2; display: flex; align-items: center; gap: 6px`).
fn mine(index: usize, message: &ChatMessage) -> Container<Message> {
    let words = match &message.body {
        Body::Text(words) => words.clone(),
        Body::Voice(_) => "Voice message".to_owned(),
        Body::Photo { caption, .. } => caption.clone(),
    };
    let bubble = Container::new(inter(words, 14.0).line_height_dp(20.0).color(WHITE))
        // The 250 dp are the text's; the padding is added.
        .max_width(282.0)
        .padding_xy(16.0, 12.0)
        .radii(RADIUS_BUBBLE, RADIUS_BUBBLE, RADIUS_TAIL, RADIUS_BUBBLE)
        .background(ACCENT);
    let status = |asset: Option<&'static SvgAsset>, line: String| {
        Row::new()
            .push_maybe(asset.map(|asset| icon(asset, 14.0, MUTED)))
            .push(inter(line, 12.0).color(MUTED))
            .gap(6.0)
            .cross_align(CrossAlign::Center)
    };
    let status = match &message.delivery {
        Delivery::Seen(readers) => status(Some(&icons::SEEN), format!("Seen by {readers}")),
        // The app's own three, in the same line's type.
        Delivery::Sent | Delivery::Received => status(Some(&icons::CHECK), "Sent".to_owned()),
        Delivery::Sending => status(None, "Sending\u{2026}".to_owned()),
        Delivery::Failed(why) => Container::new(
            inter(format!("Not sent. {why} Tap to try again."), 12.0)
                .line_height_dp(17.0)
                .color(SOFT)
                .align(TextAlign::End),
        )
        .max_width(282.0)
        .min_height(48.0)
        .content_align(Alignment::CENTER_RIGHT)
        .on_press(Message::Retry(message.local))
        .id(("retry", message.local)),
    };
    col![bubble, status]
        .gap(6.0)
        .cross_align(CrossAlign::End)
        .fill_width()
        .id(("entry", index))
}

/// A notice in the middle of the chat: `align-self: center; display:
/// flex; align-items: center; gap: 8px; padding: 8px 14px; border: 1px
/// solid #2A2F3D; border-radius: 999px; font-size: 12px; color: #C9CEDA`
/// with the server's 14 dp icon.
fn notice(index: usize, line: &str) -> Container<Message> {
    Container::new(pill(
        Some(&icons::SERVER),
        inter(line.to_owned(), 12.0),
        SOFT,
        None,
    ))
    .center()
    .fill_width()
    .id(("entry", index))
}

/// The bar at the bottom:
///
/// ```css
/// padding: 12px 20px 28px; background: #0D0F15; border-top: 1px solid #1F2330;
/// display: flex; align-items: center; gap: 10px
/// ```
///
/// with the paperclip (44 dp, a 22 dp icon in `#C9CEDA`), the field
/// (`height: 48px; border-radius: 999px; border: 1px solid #2A2F3D;
/// background: #171A24; font-size: 15px; padding: 0 18px`) and the send
/// button (48 dp in the accent, a 20 dp icon).
fn composer(draft: &str) -> Container<Message> {
    let input = TextInput::new(draft)
        .placeholder("Andika ujumbe\u{2026}")
        .capitalization(Capitalization::Sentences)
        .action(ImeAction::Send)
        .on_input(Message::Draft)
        .on_submit(Message::Send)
        .semantic_label("Message")
        .id("draft");
    col![
        hairline(),
        row![
            icon_button(
                &icons::ATTACH,
                44.0,
                22.0,
                SOFT,
                "Attach a file",
                Message::Attach
            )
            .id("attach"),
            crate::components::field(input)
                .input_frame()
                .id("draft-field"),
            Container::new(icon(&icons::SEND, 20.0, WHITE))
                .center()
                .size(48.0, 48.0)
                .radius(24.0)
                .background(ACCENT)
                .on_press(Message::Send)
                .label("Send message")
                .id("send"),
        ]
        .gap(10.0)
        .cross_align(CrossAlign::Center)
        .fill_width()
        .padding_ltrb(ROOM_MARGIN, 12.0, ROOM_MARGIN, 28.0),
    ]
    .fill_width()
    .background(PAGE)
    .safe_area(SafeEdges::BOTTOM)
}

/// One line in the middle of an empty conversation. The app's own.
fn quiet_line(line: String) -> Container<Message> {
    Container::new(
        inter(line, 13.0)
            .line_height_dp(19.0)
            .color(MUTED)
            .centered()
            .fill_width(),
    )
    .fill_width()
    .padding_ltrb(0.0, 28.0, 0.0, 0.0)
    .id("room-empty")
}

/// `padding: 56px 20px 0`, a column, with the message field over its
/// bottom edge.
pub fn view(app: &BaruaTalk) -> Element<Message> {
    let Some(room) = app.room() else {
        // The room went away under the screen: only the way back.
        return page(
            Screen::Room,
            Container::new(back_button("Back to home")).padding_ltrb(10.0, TOP, 0.0, 0.0),
        )
        .into();
    };
    let empty = match &room.loaded {
        Loaded::Loading | Loaded::No if room.entries.is_empty() => {
            Some(quiet_line("Loading this room\u{2026}".to_owned()))
        }
        Loaded::Failed(why) if room.entries.is_empty() => Some(quiet_line(format!(
            "The messages could not be fetched. {why}"
        ))),
        Loaded::Yes if room.entries.is_empty() => Some(quiet_line(
            "No messages yet. Write the first one.".to_owned(),
        )),
        _ => None,
    };
    // `margin-top: 20px; display: flex; flex-direction: column; gap: 16px`.
    let conversation = Column::new()
        .extend(
            room.entries
                .iter()
                .enumerate()
                .map(|(index, entry)| match entry {
                    Entry::Message(message) if message.mine => mine(index, message),
                    Entry::Message(message) => theirs(app, room, index, message),
                    Entry::Notice(line) => notice(index, line),
                }),
        )
        .push_maybe(empty)
        .gap(16.0)
        .fill_width()
        .padding_ltrb(ROOM_MARGIN, 20.0, ROOM_MARGIN, 20.0);
    let body = col![
        header(app, room),
        ScrollView::new(conversation).id(Screen::Room.scroll_id()),
        composer(app.draft()),
    ]
    .fill();
    page(Screen::Room, body).safe_area(SafeEdges::TOP).into()
}
