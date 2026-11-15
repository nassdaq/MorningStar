//! The first run's question: what is your name? **Not from the design**,
//! which begins with Neema already in; built from the incoming call's
//! type, a room's message field and the app's main button.

use morningstar::ui::prelude::*;

use crate::api::NAME_LIMIT;
use crate::components::{MARGIN, field, note, page, primary_button, problem, section};
use crate::icons::{self, icon};
use crate::theme::{ACCENT, MUTED, WHITE, grotesk, inter};
use crate::{BaruaTalk, Message, Screen};

/// The app's mark: the design's send plane on a disc of the accent, as
/// the launcher icon draws it.
pub fn mark<M: 'static>(size: f32) -> Container<M> {
    Container::new(icon(&icons::SEND, size * 0.46, WHITE))
        .center()
        .size(size, size)
        .radius(size / 2.0)
        .background(ACCENT)
        .exclude_semantics()
}

/// The screen.
pub fn view(app: &BaruaTalk) -> Element<Message> {
    let content = Column::new()
        .push(mark(72.0))
        .push(Spacer::fixed(28.0))
        .push(grotesk("Barua Talk", 30.0).heading())
        .push(Spacer::fixed(8.0))
        .push(inter("Rooms, calls and chat. Free, however long.", 15.0).color(MUTED))
        .push(Spacer::fixed(40.0))
        .push(section("Your name"))
        .push(Spacer::fixed(12.0))
        .push(
            field(
                TextInput::new(app.name_draft())
                    .placeholder("Neema")
                    .max_length(NAME_LIMIT)
                    .capitalization(Capitalization::Words)
                    .action(ImeAction::Done)
                    .on_input(Message::Name)
                    .on_submit(Message::Continue)
                    .semantic_label("Your name")
                    .id("name"),
            )
            .input_frame()
            .id("name-field"),
        )
        .push_maybe(app.name_problem().map(|wrong| {
            Container::new(problem(wrong).live(Live::Polite))
                .fill_width()
                .padding_ltrb(0.0, 12.0, 0.0, 0.0)
                .id("name-problem")
        }))
        .push(Spacer::fixed(16.0))
        .push(primary_button("Continue", Message::Continue).id("continue"))
        .push(Spacer::fixed(16.0))
        .push(note(
            "It is what other people see when you write or call. There is no account and no password.",
        ))
        .fill_width()
        .padding_ltrb(MARGIN, 96.0, MARGIN, 24.0);
    page(
        Screen::Welcome,
        ScrollView::new(content).id(Screen::Welcome.scroll_id()),
    )
    .into()
}
