//! The people who can be written to and called. **Not from the design**,
//! whose bar has the tab ("Contacts") and no page for it; built from
//! Home's rows, round buttons and unread count.

use morningstar::ui::prelude::*;

use crate::components::{MARGIN, TOP, count, face, outlined_icon_button, page, tab_bar};
use crate::icons;
use crate::model::Person;
use crate::theme::{ACCENT, MUTED, QUIET, grotesk, inter};
use crate::{BaruaTalk, Link, Message, Screen, Tab};

/// A person: Home's row. Tapping the person opens their conversation; the
/// ways to call them are where the time is; beside their state is how many
/// of their messages are unread.
fn person_row(app: &BaruaTalk, index: usize, person: &Person) -> Container<Message> {
    let (dot, state) = match (&person.phone, person.online) {
        (Some(_), _) => (QUIET, "Phone contact".to_owned()),
        (None, true) => (ACCENT, "Online".to_owned()),
        (None, false) => (QUIET, "Offline".to_owned()),
    };
    let buttons: Element<Message> = match &person.phone {
        Some(_) => outlined_icon_button(
            &icons::PHONE,
            "Call their phone",
            Message::PhonePerson(index),
        )
        .size(48.0, 48.0)
        .radius(24.0)
        .id(("person-phone", index))
        .into(),
        None => row![
            outlined_icon_button(&icons::MIC, "Voice call", Message::CallPerson(index, false))
                .size(48.0, 48.0)
                .radius(24.0)
                .id(("person-voice", index)),
            outlined_icon_button(
                &icons::VIDEO,
                "Video call",
                Message::CallPerson(index, true)
            )
            .size(48.0, 48.0)
            .radius(24.0)
            .id(("person-video", index)),
        ]
        .gap(8.0)
        .into(),
    };
    // The person themselves is the way into their conversation, as a row
    // of Home is the way into a room; a phone contact has none.
    let who = row![
        face(app, &person.face, 52.0),
        col![
            inter(person.name.clone(), 16.0)
                .semibold()
                .max_lines(1)
                .fill_width(),
            row![
                Container::empty()
                    .size(8.0, 8.0)
                    .radius(4.0)
                    .background(dot),
                inter(state.clone(), 13.0).color(MUTED),
            ]
            .push_maybe((person.unread > 0).then(|| count(person.unread)))
            .gap(6.0)
            .cross_align(CrossAlign::Center),
        ]
        .gap(4.0)
        .fill_width(),
    ]
    .gap(14.0)
    .cross_align(CrossAlign::Center)
    .fill_width()
    .height(76.0);
    let who = if person.phone.is_none() {
        let unread = match person.unread {
            0 => String::new(),
            1 => ", 1 unread message".to_owned(),
            unread => format!(", {unread} unread messages"),
        };
        who.on_press(Message::MessagePerson(index))
            .label(format!("Message {}, {state}{unread}", person.name))
            .id(("person-message", index))
    } else {
        who.merge_semantics()
    };
    row![who, buttons]
        .gap(14.0)
        .cross_align(CrossAlign::Center)
        .fill_width()
        .height(76.0)
        .semantics_group()
        .label(person.name.clone())
        .id(("person", index))
}

/// The screen.
pub fn view(app: &BaruaTalk) -> Element<Message> {
    let people = app.people();
    let empty = people.is_empty().then(|| {
        let line = if matches!(app.link(), Link::Online { .. }) {
            "Nobody else is here yet. Two phones can call each other once both have opened the app."
        } else {
            "Nobody here."
        };
        Container::new(
            inter(line, 13.0)
                .line_height_dp(19.0)
                .color(MUTED)
                .centered()
                .fill_width(),
        )
        .fill_width()
        .padding_ltrb(0.0, 28.0, 0.0, 0.0)
        .id("people-empty")
    });
    let content = Column::new()
        .push(grotesk("People", 20.0).heading())
        .push(Spacer::fixed(4.0))
        .push(
            inter(
                "Tap a person to write. Messages and in-app calls are free",
                12.0,
            )
            .color(MUTED),
        )
        .push(Spacer::fixed(16.0))
        .extend(
            people
                .iter()
                .enumerate()
                .map(|(index, person)| person_row(app, index, person)),
        )
        .push_maybe(empty)
        .fill_width()
        .padding_ltrb(MARGIN, TOP, MARGIN, 16.0);
    let body = col![
        ScrollView::new(content).id(Screen::People.scroll_id()),
        tab_bar(Tab::Contacts),
    ]
    .fill();
    page(Screen::People, body).safe_area(SafeEdges::TOP).into()
}
