//! The Library: the conversations of this session. **Not from the
//! design**, whose tab bar has a Library tab that leads nowhere
//! (`href="#"`); built from Explore's parts: its heading, its list card
//! and its rows.
//!
//! With a server behind the app these are the chats it holds for this
//! phone, newest first: a row opens one and fetches its messages. Without
//! one, the design's own conversation is always there and every chat
//! started on home, in Explore or by voice is added; nothing of those is
//! stored: the list is this run of the app.

use morningstar::ui::prelude::*;

use crate::components::{
    MARGIN, TOP, chevron, list_card, list_row, page, primary_button, profile_button,
    section_heading, tab_bar, two_lines,
};
use crate::icons::{self, icon};
use crate::theme::{ACCENT, MUTED, RADIUS_CARD, RADIUS_TILE, TINT_08, jakarta};
use crate::{Atlas, Conversation, Loaded, Message, Phase, Screen, Tab};

fn summary(chat: &Conversation) -> String {
    // Of a chat the server holds, the app may have the count alone.
    let listed = chat
        .remote
        .as_ref()
        .filter(|remote| remote.loaded != Loaded::Full)
        .map(|remote| remote.count as usize);
    let count = listed.unwrap_or(chat.turns.len());
    let state = match chat.turns.last().map(|turn| turn.phase) {
        _ if listed.is_some() => "answered",
        Some(Phase::Thinking | Phase::Working(_)) => "working",
        Some(Phase::Cancelled) => "cancelled",
        Some(Phase::Failed) => "not answered",
        Some(Phase::Done) => "answered",
        None => "empty",
    };
    format!(
        "{count} message{} \u{b7} {state}",
        if count == 1 { "" } else { "s" }
    )
}

/// The screen.
pub fn view(app: &Atlas) -> Element<Message> {
    let chats = app.chats();
    let listed = app.library();
    let oldest = listed.first().copied();
    let rows: Vec<Container<Message>> = listed
        .iter()
        .rev()
        .map(|&index| {
            let chat = &chats[index];
            list_row(
                row![
                    Container::new(icon(&icons::FILE, 19.0, ACCENT))
                        .center()
                        .size(40.0, 40.0)
                        .radius(RADIUS_TILE)
                        .background(TINT_08),
                    two_lines(chat.title.clone(), summary(chat)),
                    chevron(),
                ],
                68.0,
                14.0,
                14.0,
                Some(index) == oldest,
            )
            .on_press(Message::OpenChat(index))
            .id(("chat", index))
        })
        .collect();
    // A phone the server has not seen has no chats yet.
    let list: Element<Message> = if rows.is_empty() {
        Container::new(
            jakarta(
                "No chats yet. What you ask Atlas is kept on its server and listed here.",
                14.0,
            )
            .line_height_dp(21.0)
            .color(MUTED)
            .fill_width(),
        )
        .fill_width()
        .padding_xy(4.0, 8.0)
        .id("no-chats")
        .into()
    } else {
        list_card(rows, RADIUS_CARD).into()
    };
    let content = col![
        section_heading("Recent chats"),
        list,
        Spacer::fixed(4.0),
        primary_button("New chat", Message::NewChat).id("new-chat"),
    ]
    .gap(12.0)
    .fill_width()
    .padding_ltrb(MARGIN, 0.0, MARGIN, 20.0);
    let body = col![
        row![
            jakarta("Library", 28.0)
                .bold()
                .letter_spacing(-0.56)
                .heading(),
            profile_button(),
        ]
        .space_between()
        .cross_align(CrossAlign::Center)
        .fill_width()
        .padding_ltrb(MARGIN, TOP, MARGIN, 14.0),
        ScrollView::new(content).id(Screen::Library.scroll_id()),
        tab_bar(Tab::Library),
    ]
    .fill();
    page(Screen::Library, None, body)
        .safe_area(SafeEdges::TOP)
        .into()
}
