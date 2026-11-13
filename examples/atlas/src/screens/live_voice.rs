//! Screen 06: live voice. `design/html/06-live-voice.html`.
//!
//! When the app's server has a voice, the screen records: the orb
//! breathes while the microphone is open, the microphone button (or the
//! cross) sends what was said, the server's transcript shows in the
//! design's quotes for a moment, and then the chat opens with that message
//! in it and its answer, which is also spoken. The chevron goes back and
//! sends nothing. A refused microphone, or a recording that does not get
//! through, is said in a line under the status (with "Send again" when
//! there is a recording to send); those lines and the "Sending" and
//! "Heard" states are the app's own, in the design's type.
//!
//! Without a server with a voice nothing is recorded: the screen plays
//! the design's own line, a word at a time, while the orb breathes, then
//! hands the line to the chat as if it had been said. The microphone
//! button pauses and resumes that; the cross ends it at once with whatever
//! has been "heard". When there is a server (so that the difference
//! matters) the status says "demo".

use morningstar::ui::prelude::*;

use crate::components::{TOP, icon_button, model_chip, orb, page, spinner, wash_hero};
use crate::icons::{self, icon};
use crate::theme::{ACCENT, BRAND_FROM, BRAND_TO, INK, LINE, LINE_20, MUTED, WHITE, jakarta};
use crate::{Atlas, Link, Listen, Message, Screen, VOICE_LINE};

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
    // The demo says that it is one wherever a real voice could have been.
    let demo = !matches!(app.link(), Link::Standalone | Link::Checking);
    let quoted = |text: &str| format!("\u{201c}{text}\u{201d}");
    // What the status says, whether it is in the accent, the big line, and
    // the small one under it.
    let (status, lit, line, hint): (&str, bool, String, Option<String>) = match voice.live {
        Listen::Demo => (
            match (voice.muted, demo) {
                (false, false) => "Listening",
                (true, false) => "Paused",
                (false, true) => "Listening \u{b7} demo",
                (true, true) => "Paused \u{b7} demo",
            },
            !voice.muted,
            line,
            None,
        ),
        Listen::Starting => (
            "Starting the microphone",
            false,
            "\u{2026}".to_owned(),
            None,
        ),
        Listen::Recording => (
            "Listening",
            true,
            "\u{2026}".to_owned(),
            Some("Say it, then tap the microphone to send.".to_owned()),
        ),
        Listen::Stopping | Listen::Sending => (
            "Sending",
            true,
            "\u{2026}".to_owned(),
            Some("Atlas is listening to what you said.".to_owned()),
        ),
        Listen::Heard => (
            "Heard",
            true,
            quoted(app.voice_heard().unwrap_or_default()),
            None,
        ),
        Listen::Failed => (
            "Not sent",
            false,
            "\u{2026}".to_owned(),
            app.voice_error().map(str::to_owned),
        ),
    };
    let tone = if lit { ACCENT } else { MUTED };
    let sending = matches!(voice.live, Listen::Stopping | Listen::Sending);
    let mark: Element<Message> = if sending {
        spinner(app.spin()).into()
    } else {
        icon(&icons::WAVEFORM, 17.0, tone).into()
    };
    // The microphone button: a pause in the demo, "send" while recording.
    let (mic_label, mic_hint) = match voice.live {
        Listen::Demo => ("Mute microphone", "Pauses the demo"),
        Listen::Recording => (
            "Send what you said",
            "Stops recording and sends it to Atlas",
        ),
        Listen::Failed => ("Record again", "Starts the microphone again"),
        Listen::Starting | Listen::Stopping | Listen::Sending | Listen::Heard => {
            ("Microphone", "Busy for a moment")
        }
    };
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
            row![mark, jakarta(status, 13.0).bold().color(tone),]
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
        // The app's own: what to do, or what went wrong, in the Plans
        // page's small type; and the way to send a recording again.
        .push_maybe(hint.map(|hint| {
            jakarta(hint, 13.0)
                .line_height_dp(19.0)
                .color(MUTED)
                .centered()
                .fill_width()
                .live(Live::Polite)
                .id("voice-hint")
        }))
        .push_maybe(app.voice_can_resend().then(|| {
            Container::new(jakarta("Send again", 13.0).semibold())
                .center()
                .height(48.0)
                .padding_xy(18.0, 0.0)
                .radius(24.0)
                .border(1.0, LINE_20)
                .on_press(Message::VoiceRetry)
                .id("voice-retry")
        }))
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
                .label(mic_label)
                .hint(mic_hint)
                .role(if voice.live == Listen::Demo {
                    Role::Switch
                } else {
                    Role::Button
                })
                .checked(voice.live == Listen::Demo && voice.muted)
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
