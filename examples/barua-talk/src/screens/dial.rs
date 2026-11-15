//! The dial pad. **Not from the design**, which has the button that opens
//! it ("Dial a phone number") and the call it leads to; built from the
//! phone call's header and pill, the timer's type and its round buttons.

use morningstar::ui::prelude::*;

use crate::components::{MARGIN, TOP, back_button, disc, keypad, note, page, pill, problem};
use crate::icons;
use crate::model::pretty_number;
use crate::theme::{ACCEPT, GROTESK, INK, MUTED, SOFT, grotesk_line, inter};
use crate::{BaruaTalk, Link, Message, Screen};

/// The screen. `room` is the height the screen has.
pub fn view(app: &BaruaTalk, room: f32) -> Element<Message> {
    let (number, wrong) = app.dial();
    let header = row![
        back_button("Back"),
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
    let shown = if number.is_empty() {
        "+255".to_owned()
    } else {
        pretty_number(number)
    };
    let display = text(shown)
        .family(GROTESK)
        .size(30.0)
        .medium()
        .line_height_dp(grotesk_line(30.0))
        .color(if number.is_empty() { MUTED } else { INK })
        .tabular_nums()
        .max_lines(1)
        .semantic_label(if number.is_empty() {
            "No number yet".to_owned()
        } else {
            format!("Number: {number}")
        })
        .live(Live::Polite)
        .id("dial-number");
    let phone_off = matches!(app.link(), Link::Online { phone: false, .. });
    let under: Element<Message> = match wrong {
        Some(wrong) => problem(wrong.to_owned())
            .centered()
            .live(Live::Polite)
            .id("dial-problem")
            .into(),
        None if phone_off => note("Phone calls are switched off on this server.")
            .centered()
            .into(),
        None => note(format!(
            "{} \u{b7} paid from the workspace balance",
            app.phone_rate()
        ))
        .centered()
        .into(),
    };
    let content = col![
        header,
        Spacer::fixed(36.0),
        Container::new(display).center().fill_width().height(44.0),
        Spacer::fixed(8.0),
        Container::new(under)
            .fill_width()
            .min_height(38.0)
            .padding_xy(MARGIN, 0.0),
        Spacer::new().min_height(16.0),
        keypad(
            [('+', "Plus"), ('0', "0"), ('<', "Delete the last digit")],
            dial_key,
            "dial"
        ),
        Spacer::fixed(24.0),
        disc(&icons::PHONE, 72.0, 30.0, ACCEPT)
            .on_press(Message::DialCall)
            .label("Call this number")
            .id("dial-call"),
    ]
    .cross_align(CrossAlign::Center)
    .fill_width()
    .min_height(room)
    .padding_ltrb(0.0, TOP, 0.0, 40.0);
    page(
        Screen::Dial,
        ScrollView::new(content).id(Screen::Dial.scroll_id()),
    )
    .into()
}

/// What a key of the pad sends: its third-row "<" deletes.
fn dial_key(key: char) -> Message {
    if key == '<' {
        Message::DialBackspace
    } else {
        Message::DialKey(key)
    }
}
