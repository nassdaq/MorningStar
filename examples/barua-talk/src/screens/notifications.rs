//! What wants the user's attention: rooms and conversations with unread
//! messages and calls that were missed. **Not from the design**, whose bar has the tab
//! ("Notifications") and no page for it; it is Home's list, filtered.

use morningstar::ui::prelude::*;

use crate::components::{MARGIN, TOP, list_row, page, tab_bar};
use crate::theme::{MUTED, grotesk, inter};
use crate::{BaruaTalk, Message, Screen, Tab};

/// The screen.
pub fn view(app: &BaruaTalk) -> Element<Message> {
    let rows = app.notifications();
    let empty = rows.is_empty().then(|| {
        Container::new(
            inter(
                "Nothing new. Unread messages and missed calls show here.",
                13.0,
            )
            .line_height_dp(19.0)
            .color(MUTED)
            .centered()
            .fill_width(),
        )
        .fill_width()
        .padding_ltrb(0.0, 28.0, 0.0, 0.0)
        .id("notifications-empty")
    });
    let content = Column::new()
        .push(grotesk("Notifications", 20.0).heading())
        .push(Spacer::fixed(4.0))
        .push(inter("Unread messages and missed calls", 12.0).color(MUTED))
        .push(Spacer::fixed(16.0))
        .extend(
            rows.into_iter()
                .map(|index| list_row(app, index, &app.items()[index])),
        )
        .push_maybe(empty)
        .fill_width()
        .padding_ltrb(MARGIN, TOP, MARGIN, 16.0);
    let body = col![
        ScrollView::new(content).id(Screen::Notifications.scroll_id()),
        tab_bar(Tab::Notifications),
    ]
    .fill();
    page(Screen::Notifications, body)
        .safe_area(SafeEdges::TOP)
        .into()
}
