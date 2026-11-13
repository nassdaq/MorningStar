//! Screen 07: Explore. `design/html/07-explore.html`.
//!
//! The search field filters the tools by their two lines as it is typed
//! in, and the chips by kind; both at once narrow further. The chips are a
//! row wider than the screen (`overflow: hidden` in the design, where the
//! fifth is cut off): it scrolls sideways. A tool starts a chat about it.

use morningstar::ui::prelude::*;

use crate::components::{
    MARGIN, TOP, chevron, chip, list_card, list_row, page, profile_button, tab_bar, text_field,
    two_lines,
};
use crate::icons::{self, icon};
use crate::theme::{ACCENT, LINE_12, MUTED, RADIUS_CARD, RADIUS_TILE, TINT_08, jakarta};
use crate::{Atlas, Category, Message, Screen, TOOLS, Tab};

/// A tool: a `button` with `min-height: 68px; display: flex; align-items:
/// center; gap: 14px; padding: 0 14px`, a 40 dp tile (`border-radius:
/// 12px; background: rgba(160,40,122,0.08)`) with a 19 dp icon in the
/// accent, two lines and a chevron.
fn tool(index: usize, last: bool) -> Container<Message> {
    let tool = &TOOLS[index];
    list_row(
        row![
            Container::new(icon(tool.icon, 19.0, ACCENT))
                .center()
                .size(40.0, 40.0)
                .radius(RADIUS_TILE)
                .background(TINT_08),
            two_lines(tool.title, tool.detail),
            chevron(),
        ],
        68.0,
        14.0,
        14.0,
        last,
    )
    .on_press(Message::Tool(index))
    .id(("tool", index))
}

/// `padding: 52px 20px 0; display: flex; flex-direction: column; gap:
/// 14px` on `#FBF8FC`, with the tab bar at the bottom.
pub fn view(app: &Atlas) -> Element<Message> {
    let (query, category) = app.filter();
    let shown = app.tools();
    let count = shown.len();
    let results: Element<Message> = if shown.is_empty() {
        // The app's own: the design has no empty state.
        jakarta("No tools match. Try another word or chip.", 14.0)
            .color(MUTED)
            .centered()
            .fill_width()
            .id("no-tools")
            .into()
    } else {
        list_card(
            shown
                .into_iter()
                .enumerate()
                .map(|(place, index)| tool(index, place + 1 == count))
                .collect(),
            RADIUS_CARD,
        )
        .into()
    };
    let body = col![
        // `display: flex; align-items: center; justify-content:
        // space-between`: `font-size: 28px; font-weight: 700;
        // letter-spacing: -0.02em` and the profile button.
        row![
            jakarta("Explore", 28.0)
                .bold()
                .letter_spacing(-0.56)
                .heading(),
            profile_button(),
        ]
        .space_between()
        .cross_align(CrossAlign::Center)
        .fill_width()
        .padding_ltrb(MARGIN, TOP, MARGIN, 0.0),
        Spacer::fixed(14.0),
        // A `div` with `height: 52px; border-radius: 26px; background:
        // #FFFFFF; border: 1px solid rgba(29,24,48,0.12)`: 54 dp.
        Container::new(
            text_field(
                &icons::SEARCH,
                TextInput::new(query)
                    .placeholder("Search tools")
                    .action(ImeAction::Search)
                    .no_autocorrect()
                    .on_input(Message::Query)
                    .semantic_label("Search tools")
                    .id("search"),
                54.0,
                26.0,
                LINE_12,
            )
            .input_frame()
            .id("search-field"),
        )
        .fill_width()
        .padding_xy(MARGIN, 0.0),
        Spacer::fixed(14.0),
        // `display: flex; gap: 8px; margin-right: -20px; overflow: hidden`.
        ScrollView::horizontal(
            Row::new()
                .extend(Category::ALL.map(|kind| {
                    chip(kind.name(), kind == category, Message::Filter(kind)).id(("chip", kind))
                }))
                .gap(8.0)
                .padding_xy(MARGIN, 0.0),
        )
        .height(44.0)
        .id("chips"),
        Spacer::fixed(14.0),
        ScrollView::new(
            Container::new(results)
                .fill_width()
                .padding_ltrb(MARGIN, 0.0, MARGIN, 20.0),
        )
        .id(Screen::Explore.scroll_id()),
        tab_bar(Tab::Explore),
    ]
    .fill();
    page(Screen::Explore, None, body)
        .safe_area(SafeEdges::TOP)
        .into()
}
