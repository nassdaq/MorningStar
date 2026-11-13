//! Screen 06: live voice. `design/html/06-live-voice.html`.
//!
//! There is no microphone in this example and nothing is recorded: the
//! screen plays the design's own line, a word at a time, while the orb
//! breathes, then hands the line to the chat as if it had been said. The
//! microphone button pauses and resumes that; the cross ends it at once
//! with whatever has been "heard"; the chevron goes back and sends nothing.

use morningstar::ui::prelude::*;

use crate::components::{TOP, icon_button, model_chip, orb, page, wash_hero};
use crate::icons::{self, icon};
use crate::theme::{ACCENT, BRAND_FROM, BRAND_TO, INK, LINE, MUTED, WHITE, jakarta};
use crate::{Atlas, Message, Screen, VOICE_LINE};

/// `padding: 52px 20px 44px; display: flex; flex-direction: column;
/// align-items: center` over onboarding's wash. `room` is the height the
/// screen has.
pub fn view(app: &Atlas, room: f32) -> Element<Message> {
    let voice = app.voice();
    let words: Vec<&str> = VOICE_LINE.split(' ').collect();
    let heard = words[..voice.words.min(words.len())].join(" ");
    let complete = voice.words >= words.len();
    // The design's line is in curly quotes; they close when it is whole.
    let line = if heard.is_empty() {
        "\u{2026}".to_owned()
    } else if complete {
        format!("\u{201c}{heard}\u{201d}")
    } else {
        format!("\u{201c}{heard}")
    };
    let status = if voice.muted { "Paused" } else { "Listening" };
    let round = |side: f32| {
        Container::<Message>::empty()
            .size(side, side)
            .radius(side / 2.0)
    };
    let content = col![
        // `align-self: stretch; display: flex; align-items: center;
        // justify-content: space-between; margin: 0 -10px`.
        row![
            icon_button(
                &icons::MINIMISE,
                22.0,
                44.0,
                "Minimise voice",
                Message::Back
            )
            .id("minimise"),
            model_chip(app.mode(), 10.0),
        ]
        .space_between()
        .cross_align(CrossAlign::Center)
        .fill_width()
        .padding_xy(10.0, 0.0),
        // `flex-grow: 1; display: flex; align-items: center;
        // justify-content: center` around the 200 dp orb.
        Container::new(
            orb(200.0)
                .transform(Transform::scale(app.pulse(), app.pulse()))
                .id("orb")
        )
        .center()
        .fill_width()
        .fill_height()
        .min_height(264.0),
        // `display: flex; flex-direction: column; align-items: center; gap:
        // 10px; text-align: center; padding: 0 8px`.
        col![
            // `display: flex; align-items: center; gap: 8px; font-size:
            // 13px; font-weight: 700; color: #A0287A`, a 17 dp waveform.
            row![
                icon(
                    &icons::WAVEFORM,
                    17.0,
                    if voice.muted { MUTED } else { ACCENT }
                ),
                jakarta(status, 13.0)
                    .bold()
                    .color(if voice.muted { MUTED } else { ACCENT }),
            ]
            .gap(8.0)
            .cross_align(CrossAlign::Center)
            .merge_semantics()
            .live(Live::Polite),
            // `font-size: 22px; font-weight: 600; line-height: 1.35;
            // letter-spacing: -0.01em`.
            jakarta(line, 22.0)
                .semibold()
                .line_height_dp(29.7)
                .letter_spacing(-0.22)
                .centered()
                .fill_width()
                .id("heard"),
        ]
        .gap(10.0)
        .cross_align(CrossAlign::Center)
        .fill_width()
        .padding_xy(28.0, 0.0),
        // `display: flex; align-items: center; gap: 22px; margin-top: 36px`.
        Spacer::fixed(36.0),
        row![
            // A `button`: `width: 56px; height: 56px; border-radius: 50%;
            // background: #FFFFFF; border: 1px solid rgba(29,24,48,0.08)`.
            round(56.0)
                .push(icon(&icons::CAMERA, 22.0, INK))
                .center()
                .background(WHITE)
                .border(1.0, LINE)
                .on_press(Message::VoiceCamera)
                .label("Show Atlas what you see")
                .id("camera"),
            // `width: 72px; height: 72px; background:
            // linear-gradient(135deg, #C2338F, #8B3FD9)`.
            round(72.0)
                .push(icon(&icons::MIC_LARGE, 28.0, WHITE))
                .center()
                .gradient(Gradient::angled(135.0, BRAND_FROM, BRAND_TO))
                .on_press(Message::VoiceMute)
                .label("Mute microphone")
                .role(Role::Switch)
                .checked(voice.muted)
                .id("mute"),
            // `width: 56px; height: 56px; background: #1D1830`.
            round(56.0)
                .push(icon(&icons::CLOSE, 22.0, WHITE))
                .center()
                .background(INK)
                .on_press(Message::VoiceEnd)
                .label("End voice and view transcript")
                .id("end"),
        ]
        .gap(22.0)
        .cross_align(CrossAlign::Center)
        .align_self(CrossAlign::Center),
    ]
    .fill_width()
    .min_height(room)
    .padding_ltrb(0.0, TOP, 0.0, 44.0);
    page(
        Screen::LiveVoice,
        Some(wash_hero()),
        ScrollView::new(content).id(Screen::LiveVoice.scroll_id()),
    )
    .into()
}
