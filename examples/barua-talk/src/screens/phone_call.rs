//! Screen 05: a call to a phone. `design/html/05-phone-call.html`.
//!
//! Who is called, the call's clock, what it costs and who pays, three
//! controls and the red button.

use morningstar::ui::prelude::*;

use crate::components::{
    MARGIN, TOP, back_button, back_button_in, control, face, keypad, page, pill, sheet,
};
use crate::icons::{self, icon};
use crate::model::{Face, PhoneCall, PhoneStatus, clock, pretty_number};
use crate::theme::{
    ACCENT, DANGER, GROTESK, INK, LINE, MUTED, RADIUS_CARD, SOFT, SURFACE, WHITE, grotesk,
    grotesk_line, inter,
};
use crate::{BaruaTalk, Message, Screen};

/// The red button as the design's page draws it. Its column is 22.5 dp
/// taller than the 844 dp page, and the one thing in it that may shrink
/// is this `a` (`width: 72px; height: 72px; border-radius: 50%`), which
/// the browser squeezes to 49.5: an ellipse 72 by 49.5. A box's corners
/// are round, not elliptical, so it is a picture (drawn in half dp).
const SQUEEZED: VectorPicture = VectorPicture::new(
    144.0,
    99.0,
    &[Layer::ellipse(72.0, 49.5, 72.0, 49.5).fill(DANGER)],
);
/// The squeezed button's height.
const SQUEEZED_HEIGHT: f32 = 49.5;

/// What the page needs to show its red button round: the design's 844 dp
/// and the 22.5 it is short of.
const ROUND_FROM: f32 = 866.5;

/// A line of the card: `display: flex; align-items: center;
/// justify-content: space-between; height: 48px` with the label in
/// `font-size: 13px; color: #9AA1B2` and the value in `font-size: 14px`.
fn fact(label: &'static str, value: String) -> Container<Message> {
    row![
        inter(label, 13.0).color(MUTED),
        inter(value, 14.0).tabular_nums(),
    ]
    .space_between()
    .cross_align(CrossAlign::Center)
    .fill_width()
    .height(48.0)
    .merge_semantics()
}

/// A control over its word: `display: flex; flex-direction: column;
/// align-items: center; gap: 8px` with a 60 dp button and the word in
/// `font-size: 12px; color: #C9CEDA`.
fn labelled(button: Container<Message>, word: &'static str) -> Container<Message> {
    col![button, inter(word, 12.0).color(SOFT).exclude_semantics()]
        .gap(8.0)
        .cross_align(CrossAlign::Center)
        .fill_width()
}

/// `padding: 56px 24px 48px; display: flex; flex-direction: column;
/// align-items: center`. `room` is the height the screen has.
pub fn view(app: &BaruaTalk, room: f32) -> Element<Message> {
    let Some(phone) = app.phone() else {
        return page(
            Screen::PhoneCall,
            Container::new(back_button("Back to home")).padding_ltrb(14.0, TOP, 0.0, 0.0),
        )
        .into();
    };
    // On a phone shorter than the design's the page gives up its air
    // rather than push the red button off the screen: the status bar's
    // room at the top (the safe area has it already) and a third of each
    // gap. At the design's height nothing changes.
    let tight = room < crate::DESIGN_HEIGHT;
    let gap = |designed: f32| {
        if tight {
            (designed * 0.6).round()
        } else {
            designed
        }
    };
    // `width: 100%; display: flex; align-items: center; justify-content:
    // space-between`: the arrow (`margin-left: -10px`), the pill
    // (`font-size: 13px; color: #C9CEDA; border: 1px solid #2A2F3D`) and
    // an empty 34 dp that keeps the pill near the middle.
    let header = row![
        back_button_in(
            if phone.can_hang_up {
                "End call and go back"
            } else {
                "Leave call and go back"
            },
            WHITE
        ),
        pill(
            Some(&icons::PHONE_SMALL),
            inter("Phone call", 13.0),
            SOFT,
            None
        ),
        Spacer::fixed(34.0),
    ]
    .space_between()
    .cross_align(CrossAlign::Center)
    .fill_width()
    .padding_ltrb(14.0, 0.0, MARGIN, 0.0);
    // `width: 132px; height: 132px; border-radius: 50%; padding: 5px;
    // border: 2px solid {{accent}}`: 146 dp in all.
    let who = phone.face.clone().unwrap_or_else(|| Face::of("#"));
    let picture = Container::new(face(app, &who, 132.0))
        .center()
        .size(146.0, 146.0)
        .radius(73.0)
        .border(2.0, ACCENT);
    let number = pretty_number(&phone.number);
    let title = phone.name.clone().unwrap_or_else(|| number.clone());
    // `font-family: 'Space Grotesk'; font-size: 44px; font-weight: 500;
    // font-variant-numeric: tabular-nums`. Until the call is answered the
    // clock's place says what is happening (the app's own).
    let big = |words: String| {
        text(words)
            .family(GROTESK)
            .size(44.0)
            .medium()
            .line_height_dp(grotesk_line(44.0))
            .color(INK)
            .tabular_nums()
    };
    let timer = match phone.status {
        PhoneStatus::Active => {
            big(clock(phone.seconds)).semantic_label(format!("Call time {}", clock(phone.seconds)))
        }
        PhoneStatus::Ringing => big("Inaita\u{2026}".to_owned()).semantic_label("Ringing"),
        PhoneStatus::Dialling => big("Inaunganisha\u{2026}".to_owned())
            .size(32.0)
            .semantic_label("Connecting"),
    };
    // `width: 100%; background: #171A24; border-radius: 20px; padding: 6px
    // 18px`, the first two lines with `border-bottom: 1px solid #2A2F3D`.
    let card = col![
        fact("Caller ID", phone.caller_id.clone()),
        Divider::new().color(LINE),
        fact("Rate", phone.rate.clone()),
        Divider::new().color(LINE),
        fact("Paid from", "Workspace balance".to_owned()),
    ]
    .fill_width()
    .padding_xy(18.0, 6.0)
    .radius(RADIUS_CARD)
    .background(SURFACE);
    // `display: grid; grid-template-columns: repeat(3, minmax(0, 1fr));
    // gap: 12px`.
    let controls = row![
        labelled(
            control(
                if phone.muted {
                    &icons::MIC_OFF
                } else {
                    &icons::MIC
                },
                60.0,
                phone.muted
            )
            .on_press(Message::ToggleMute)
            .label("Mute microphone")
            .role(Role::Switch)
            .checked(phone.muted)
            .id("mute"),
            "Mute"
        ),
        labelled(
            control(&icons::KEYPAD, 60.0, false)
                .on_press(Message::ToggleKeypad)
                .label("Open keypad")
                .id("keypad"),
            "Keypad"
        ),
        labelled(
            control(&icons::SPEAKER, 60.0, phone.speaker)
                .on_press(Message::ToggleSpeaker)
                .label("Speaker")
                .role(Role::Switch)
                .checked(phone.speaker)
                .id("speaker"),
            "Speaker"
        ),
    ]
    .gap(12.0)
    .fill_width();
    // `margin-top: 28px; width: 72px; height: 72px; border-radius: 50%;
    // background: #D92D4B` with a 30 dp icon: round where the screen has
    // the room, squeezed as the design's page shows it where it has not.
    let round = tight || room >= ROUND_FROM;
    let plate: Element<Message> = if round {
        Container::empty()
            .size(72.0, 72.0)
            .radius(36.0)
            .background(DANGER)
            .into()
    } else {
        Picture::vector(SQUEEZED).size(72.0, SQUEEZED_HEIGHT).into()
    };
    let hang_up = stack![
        plate,
        Container::new(icon(&icons::HANG_UP, 30.0, WHITE))
            .center()
            .fill(),
    ]
    .size(72.0, if round { 72.0 } else { SQUEEZED_HEIGHT })
    .on_press(Message::HangUp)
    .label(if phone.can_hang_up {
        "Hang up"
    } else {
        "Leave call"
    })
    .id("hang-up");
    // A real call: how its sound stands, in words, where the page has
    // room for a line or two (between the header and the picture). The
    // app's own.
    let note = app.sound_note().map(|sentence| {
        inter(sentence, 12.0)
            .line_height_dp(16.0)
            .color(MUTED)
            .centered()
            .fill_width()
            .id("sound-note")
    });
    let content = Column::new()
        .push(header)
        .push(
            Container::new(Column::new().push_maybe(note).fill_width())
                .fill_width()
                .height(gap(44.0))
                .center()
                .padding_xy(MARGIN, 0.0),
        )
        .push(picture)
        .push(Spacer::fixed(gap(24.0)))
        // `font-family: 'Space Grotesk'; font-size: 28px; font-weight: 600`.
        .push(
            Container::new(
                grotesk(title, 28.0)
                    .max_lines(1)
                    .centered()
                    .fill_width()
                    .heading(),
            )
            .fill_width()
            .padding_xy(MARGIN, 0.0),
        )
        .push(Spacer::fixed(6.0))
        // `font-size: 15px; color: #C9CEDA; tabular-nums`.
        .push(inter(number, 15.0).color(SOFT).tabular_nums())
        .push(Spacer::fixed(gap(20.0)))
        .push(timer.live(Live::Polite))
        .push(Spacer::fixed(gap(28.0)))
        .push(Container::new(card).fill_width().padding_xy(MARGIN, 0.0))
        // `flex-grow: 1`.
        .push(Spacer::new())
        .push(
            Container::new(controls)
                .fill_width()
                .padding_xy(MARGIN, 0.0),
        )
        .push(Spacer::fixed(gap(28.0)))
        .push(hang_up)
        .cross_align(CrossAlign::Center)
        .fill_width()
        .min_height(room)
        .padding_ltrb(0.0, if tight { 12.0 } else { TOP }, 0.0, gap(48.0));
    page(
        Screen::PhoneCall,
        ScrollView::new(content).id(Screen::PhoneCall.scroll_id()),
    )
    .into()
}

/// The keypad during a call: the keys pressed, twelve keys, and a way to
/// close it. **The app's own** (the design has the button that opens it).
pub fn keypad_sheet(phone: &PhoneCall) -> Container<Message> {
    let pressed = if phone.keys.is_empty() {
        inter("Press a key", 15.0).color(MUTED)
    } else {
        grotesk(phone.keys.clone(), 24.0)
            .tabular_nums()
            .semantic_label(format!("Keys pressed: {}", phone.keys))
    };
    sheet(
        "keypad-sheet",
        col![
            Container::new(pressed).center().fill_width().height(40.0),
            Spacer::fixed(12.0),
            keypad(
                [('*', "Star"), ('0', "0"), ('#', "Hash")],
                Message::Key,
                "key"
            ),
            Spacer::fixed(16.0),
            crate::components::outlined_button("Close keypad", Message::ToggleKeypad)
                .id("keypad-close"),
        ]
        .fill_width(),
    )
}
