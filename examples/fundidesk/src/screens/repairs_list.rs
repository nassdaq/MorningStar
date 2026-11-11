//! Screen 08: the repairs list. `design/html/08-repairs-list.html`.
//!
//! The design draws 900 px of content with the tab bar pushed to the
//! bottom. Here the tab bar is fixed and everything above it scrolls. The
//! search field is a static look-alike: there is no text input yet.

use morningstar::ui::prelude::*;

use crate::components::{Field, Tab, Tone, filter_chip, repair_card, scaffold, tab_bar};
use crate::icons;
use crate::theme::{CANVAS, GREEN, MUTED, RED, WHITE, grotesk, inter, radius, space};
use crate::{Nav, Screen, tab_destination};

/// This screen is built from the design.
pub const BUILT: bool = true;

/// The filter tabs under the search field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Filter {
    /// "All 14": chosen in the design.
    #[default]
    All,
    /// "Delayed 3".
    Delayed,
    /// "Ready 4".
    Ready,
    /// "Unpaid 6".
    Unpaid,
}

impl Filter {
    const ALL: [Self; 4] = [Self::All, Self::Delayed, Self::Ready, Self::Unpaid];

    const fn label(self) -> &'static str {
        match self {
            Self::All => "All 14",
            Self::Delayed => "Delayed 3",
            Self::Ready => "Ready 4",
            Self::Unpaid => "Unpaid 6",
        }
    }
}

/// The chosen filter tab; the default is the design's. The list shows the
/// design's five sample repairs whatever the tab.
#[derive(Debug, Default)]
pub struct State {
    /// What has been typed into the search field. The list is the
    /// design's sample and is not filtered by it.
    pub query: String,
    /// The highlighted filter tab.
    pub filter: Filter,
}

/// What the repairs list can ask for.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// The green plus: a new repair.
    NewRepair,
    /// The search field changed.
    Search(String),
    /// A filter tab.
    Filter(Filter),
    /// A repair in the list.
    OpenRepair,
    /// A tab of the tab bar.
    Tab(Tab),
}

/// The scrolling content over the fixed tab bar.
pub fn view(state: &State) -> Element<Message> {
    let body = col![header(state), repairs()].fill_width();
    scaffold(
        "repairs-list",
        CANVAS,
        None,
        body,
        Some(tab_bar(Tab::Repairs, Message::Tab).into()),
    )
    .into()
}

/// The title with the "new repair" button, the search field and the filter
/// tabs: `display: flex; flex-direction: column; gap: 16px; padding: 40px
/// 20px 0`.
fn header(state: &State) -> Container<Message> {
    // `grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 8px`.
    let filters = Filter::ALL.map(|filter| {
        filter_chip(filter.label(), filter == state.filter)
            .on_press(Message::Filter(filter))
            .id(("filter", filter as usize))
    });
    col![
        row![
            grotesk("Repairs", 26.0).line_height_dp(32.0),
            // `width: 44px; height: 44px; border-radius: 14px; background:
            // #1E8416; color: #FFFFFF` around a 22 px plus.
            Container::new(Icon::vector(icons::PLUS).size(22.0).color(WHITE))
                .size(space::TOUCH, space::TOUCH)
                .center()
                .radius(radius::BUTTON)
                .background(GREEN)
                .on_press(Message::NewRepair)
                .id("new-repair"),
        ]
        .space_between()
        .cross_align(CrossAlign::Center)
        .fill_width(),
        // `gap: 10px; height: 48px; border: 1px solid #D9E0DB;
        // border-radius: 12px; padding: 0 14px` with a `font: 400 14px
        // Inter` input.
        Field::new(&state.query)
            .text(inter("", 14.0))
            .hint("Search name, phone number or reference")
            .input(|input| input.action(ImeAction::Search).no_autocorrect())
            .on_input(Message::Search)
            .id("repairs-search")
            .leading_with_stroke(icons::SEARCH, 2.0)
            .height(48.0)
            .padding_x(14.0),
        Row::new().gap(8.0).fill_width().extend(filters),
    ]
    .gap(16.0)
    .fill_width()
    .padding_ltrb(space::PAGE_MARGIN, 40.0, space::PAGE_MARGIN, 0.0)
}

/// The list: `display: flex; flex-direction: column; gap: 10px; padding:
/// 16px 20px 0`. Every card opens the repair's details.
fn repairs() -> Container<Message> {
    // The notes under a card are `font-size: 12px`: muted, or semibold.
    let muted = |note: &'static str| inter(note, 12.0).color(MUTED);
    let strong = |note: &'static str| inter(note, 12.0).semibold();
    let cards = [
        repair_card(
            "Samsung Galaxy A14",
            "Amina Juma · FD-2026-\u{200B}00142",
            "In repair",
            Tone::Info,
            muted("Due tomorrow"),
            strong("Balance TZS 40,000"),
        ),
        repair_card(
            "Infinix Hot 30",
            "Baraka Mushi · FD-2026-\u{200B}00138",
            "Waiting for parts",
            Tone::Info,
            // `color: #B42318; font-weight: 600`.
            strong("Delayed 2 days").color(RED),
            strong("Balance TZS 25,000"),
        ),
        repair_card(
            "iPhone XR",
            "Rehema Salum · FD-2026-\u{200B}00131",
            "Ready for collection",
            Tone::Success,
            muted("Waiting 6 days"),
            strong("Balance TZS 70,000"),
        ),
        repair_card(
            "Oppo A17",
            "Daudi Mwakyusa · FD-2026-\u{200B}00140",
            "Awaiting approval",
            Tone::Warning,
            muted("Quote sent yesterday"),
            strong("Quote TZS 35,000"),
        ),
        repair_card(
            "Tecno Spark 10",
            "Neema Kileo · FD-2026-\u{200B}00143",
            "Received",
            Tone::Neutral,
            muted("Received today, 08:45"),
            muted("No quote yet"),
        ),
    ];
    let cards = cards
        .into_iter()
        .enumerate()
        .map(|(index, card)| card.on_press(Message::OpenRepair).id(("repair", index)));
    Column::new()
        .gap(10.0)
        .fill_width()
        .padding_ltrb(space::PAGE_MARGIN, 16.0, space::PAGE_MARGIN, 0.0)
        .extend(cards)
}

/// Follows the design's links.
pub fn update(state: &mut State, message: Message) -> Nav {
    match message {
        Message::NewRepair => Nav::To(Screen::NewRepair),
        Message::Search(query) => {
            state.query = query;
            Nav::Stay
        }
        Message::Filter(filter) => {
            state.filter = filter;
            Nav::Stay
        }
        Message::OpenRepair => Nav::To(Screen::RepairDetails),
        Message::Tab(tab) => Nav::To(tab_destination(tab)),
    }
}
