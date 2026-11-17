//! Samples for the layout pages: sizes, rows and columns, stacks, wrapping
//! and scrolling.

use morningstar::ui::prelude::*;

/// A row where one text gives way and the other keeps its width.
pub fn sizes<M: 'static>() -> Element<M> {
    // docs:begin sizes
    row![
        // Fixed: exactly 48 by 48 dp.
        Container::empty()
            .size(48.0, 48.0)
            .radius(12.0)
            .background(ThemeColor::PrimaryContainer),
        // Fill: takes the width the row has left, so this is the text
        // that wraps when the screen is narrow.
        text("Fundi wa simu, Kariakoo").fill_width(),
        // Hug (the default): as wide as its words.
        text("08:02").caption().muted(),
    ]
    .gap(12.0)
    .cross_align(CrossAlign::Center)
    .fill_width()
    // docs:end sizes
    .into()
}

/// A column with its children spread out, and a row split by weight.
pub fn rows_and_columns<M: 'static>() -> Element<M> {
    // docs:begin rows
    col![
        text("Summary").headline(),
        row![
            // Two fills share the row two to one.
            Container::new(text("Paid"))
                .padding(12.0)
                .background(ThemeColor::Surface)
                .fill_width_weight(2.0),
            Container::new(text("Due"))
                .padding(12.0)
                .background(ThemeColor::SurfaceVariant)
                .fill_width_weight(1.0),
        ]
        .gap(8.0)
        .fill_width(),
        // A spacer takes what is left, pushing the button to the bottom.
        Spacer::new(),
        Button::new("Continue").fill_width(),
    ]
    .gap(16.0)
    .padding(20.0)
    .fill()
    // docs:end rows
    .into()
}

/// A badge pinned to the corner of a tile.
pub fn stack_with_badge<M: 'static>() -> Element<M> {
    // docs:begin stack
    stack![
        Container::empty()
            .size(56.0, 56.0)
            .radius(16.0)
            .background(ThemeColor::SurfaceVariant),
        // Pinned to one of nine anchors, then nudged outwards.
        Container::new(text("3").caption().color(ThemeColor::OnPrimary))
            .padding_xy(6.0, 2.0)
            .radius(999.0)
            .background(ThemeColor::Primary)
            .place(Alignment::TOP_RIGHT)
            .offset(6.0, -6.0),
    ]
    // docs:end stack
    .into()
}

/// Chips that continue on the next line when they run out of room.
pub fn wrapping_chips<M: 'static>(tags: &[&'static str]) -> Element<M> {
    // docs:begin wrap
    let mut chips = Wrap::new().gap(8.0).line_gap(8.0).fill_width();
    for tag in tags {
        chips = chips.push(
            Container::new(text(*tag).label())
                .padding_xy(12.0, 6.0)
                .radius(999.0)
                .border(1.0, ThemeColor::Divider),
        );
    }
    // docs:end wrap
    chips.into()
}

/// A screen that scrolls, inside the safe area.
pub fn scrolling<M: 'static>(rows: Vec<String>) -> Element<M> {
    // docs:begin scroll
    let mut list = Column::new().gap(12.0).padding(20.0).fill_width();
    for (index, row) in rows.into_iter().enumerate() {
        // An id per row keeps a finger's press with its row when rows
        // are added or removed above it.
        list = list.push(text(row).fill_width().id(("row", index)));
    }
    // The id keeps this screen's scroll position its own.
    SafeArea::new(ScrollView::new(list).id("orders-scroll"))
        .id("orders")
        // docs:end scroll
        .into()
}

/// A layout that depends on the screen it is drawn on.
pub struct Wide;

impl Application for Wide {
    type Message = ();

    // docs:begin viewport
    fn view_in(&self, viewport: &Viewport<'_>) -> Element<()> {
        // Side by side on a wide screen, one under the other on a
        // narrow one. Widths are in dp, like everything else.
        let cards = if viewport.width() >= 600.0 {
            Row::new()
        } else {
            Column::new()
        };
        cards
            .gap(16.0)
            .push(text("Today"))
            .push(text("This week"))
            .into()
    }
    // docs:end viewport

    fn update(&mut self, _message: ()) {}
}
