//! Naming a new room. **Not from the design**, which has the dashed ring
//! that leads here ("New room"); built from a room's header, its message
//! field and the app's two buttons.

use morningstar::ui::prelude::*;

use crate::api::ROOM_NAME_LIMIT;
use crate::components::{MARGIN, field, note, page, primary_button, problem, section, top_bar};
use crate::{BaruaTalk, Message, Screen};

/// The screen.
pub fn view(app: &BaruaTalk) -> Element<Message> {
    let (name, wrong) = app.new_room();
    let content = Column::new()
        .push(section("Room name"))
        .push(
            field(
                TextInput::new(name)
                    .placeholder("Support desk")
                    .max_length(ROOM_NAME_LIMIT)
                    .capitalization(Capitalization::Sentences)
                    .action(ImeAction::Done)
                    .on_input(Message::RoomName)
                    .on_submit(Message::CreateRoom)
                    .semantic_label("Room name")
                    .id("room-name"),
            )
            .input_frame()
            .id("room-name-field"),
        )
        .push_maybe(wrong.map(|wrong| {
            problem(wrong.to_owned())
                .live(Live::Polite)
                .id("room-problem")
        }))
        .push(Spacer::fixed(4.0))
        .push(
            primary_button(
                if app.making_room() {
                    "Creating\u{2026}"
                } else {
                    "Create room"
                },
                Message::CreateRoom,
            )
            .id("create-room"),
        )
        .push(Spacer::fixed(4.0))
        .push(note(
            "A room is one address for chat, voice and video. Everyone in the workspace can see it and write in it. Rooms are free, however long.",
        ))
        .gap(12.0)
        .fill_width()
        .padding_ltrb(MARGIN, 24.0, MARGIN, 24.0);
    let body = col![
        top_bar("New room"),
        ScrollView::new(content).id(Screen::NewRoom.scroll_id()),
    ]
    .fill();
    page(Screen::NewRoom, body).into()
}
