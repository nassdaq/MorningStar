//! Screen 07: the dashboard. `design/html/07-dashboard.html`.
//!
//! The design draws 1060 px of content with the tab bar pushed to the
//! bottom. Here the tab bar is fixed and everything above it scrolls.

use morningstar::ui::prelude::*;

use crate::components::{
    Action, Tab, Tone, attention_row, icon_pill, link, repair_row, scaffold, stat_tile, tab_bar,
};
use crate::icons;
use crate::theme::{
    CANVAS, GREEN, GREEN_DARK, MUTED, WHITE, grotesk, heading, inter, radius, space,
};
use crate::{Nav, Screen, tab_destination};

/// This screen is built from the design.
pub const BUILT: bool = true;

/// The dashboard shows fixed sample data and remembers nothing.
#[derive(Debug, Default)]
pub struct State {}

/// What the dashboard can ask for.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// "View all", "See all" and the four attention rows: the repairs list.
    OpenRepairs,
    /// A repair received today.
    OpenRepair,
    /// "New repair".
    NewRepair,
    /// A tab of the tab bar.
    Tab(Tab),
}

/// The scrolling content over the fixed tab bar.
pub fn view(_state: &State) -> Element<Message> {
    let body = col![header(), summary(), attention(), received(), new_repair()].fill_width();
    scaffold(
        "dashboard",
        CANVAS,
        None,
        body,
        Some(tab_bar(Tab::Home, Message::Tab).into()),
    )
    .into()
}

/// `display: flex; align-items: center; justify-content: space-between;
/// padding: 40px 20px 0`.
fn header() -> Container<Message> {
    row![
        col![
            inter("Wednesday, 11 November", 13.0).color(MUTED),
            grotesk("Kariakoo Phone Clinic", 22.0).line_height_dp(28.0),
        ]
        .gap(4.0),
        icon_pill(icons::CHECK, "Synced", Tone::Success),
    ]
    .space_between()
    .cross_align(CrossAlign::Center)
    .fill_width()
    .padding_ltrb(space::PAGE_MARGIN, 40.0, space::PAGE_MARGIN, 0.0)
}

/// The green card: `margin: 20px 20px 0; border-radius: 20px; background:
/// #1E8416; padding: 20px; gap: 16px`.
fn summary() -> Container<Message> {
    let tile = |caption: &'static str, amount: &'static str| {
        stat_tile(
            inter(caption, 12.0).color(WHITE),
            grotesk(amount, 17.0).color(WHITE),
            GREEN_DARK,
        )
    };
    let card = col![
        row![
            col![
                inter("Phones in your workshop", 13.0).medium().color(WHITE),
                grotesk("14", 48.0).line_height_dp(52.0).color(WHITE),
            ]
            .gap(2.0),
            // `height: 44px; padding: 0 16px; border-radius: 12px;
            // background: #FFFFFF; color: #176611; font-size: 14px`.
            Action::primary("View all")
                .colors(WHITE, GREEN_DARK)
                .height(44.0)
                .radius(radius::FIELD)
                .text_size(14.0)
                .hug(16.0)
                .on_press(Message::OpenRepairs),
        ]
        .space_between()
        .cross_align(CrossAlign::End)
        .fill_width(),
        // `grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px`.
        row![
            tile("Collected today", "TZS 185,000"),
            tile("Unpaid balances", "TZS 240,000"),
        ]
        .gap(10.0)
        .fill_width(),
    ]
    .gap(16.0)
    .padding(20.0)
    .fill_width()
    .radius(radius::HERO)
    .background(GREEN);
    section(20.0).push(card)
}

/// A block of the page: `padding: {top}px 20px 0`.
fn section(top: f32) -> Container<Message> {
    Column::new()
        .fill_width()
        .padding_ltrb(space::PAGE_MARGIN, top, space::PAGE_MARGIN, 0.0)
}

/// "Needs attention": `gap: 10px` under the heading, rows `8px` apart.
/// Every row leads to the repairs list.
fn attention() -> Container<Message> {
    let rows = [
        (
            icons::CLOCK,
            Tone::Danger,
            "Delayed repairs",
            "Past the promised date",
            "3",
        ),
        (
            icons::PHONE_CHECK,
            Tone::Success,
            "Ready, not collected",
            "Oldest waiting 6 days",
            "4",
        ),
        (
            icons::MESSAGE,
            Tone::Warning,
            "Awaiting approval",
            "Customer has not accepted the quote",
            "2",
        ),
        (
            icons::PACKAGE,
            Tone::Info,
            "Waiting for parts",
            "Parts ordered or out of stock",
            "2",
        ),
    ];
    let rows = rows
        .into_iter()
        .enumerate()
        .map(|(index, (icon, tone, title, detail, count))| {
            // Only the count of delayed repairs is red.
            attention_row(icon, tone, title, detail, count, tone == Tone::Danger)
                .on_press(Message::OpenRepairs)
                .id(("attention", index))
        });
    section(24.0)
        .gap(10.0)
        .push(heading("Needs attention", 17.0))
        .push(Column::new().gap(8.0).fill_width().extend(rows))
}

/// "Received today": the heading with a "See all" link, and two repairs.
fn received() -> Container<Message> {
    section(24.0)
        .gap(10.0)
        .push(
            row![
                heading("Received today", 17.0),
                // `padding: 8px 0`.
                link("See all", 8.0).on_press(Message::OpenRepairs),
            ]
            .space_between()
            .cross_align(CrossAlign::Center)
            .fill_width(),
        )
        .push(
            col![
                repair_row(
                    "Tecno Spark 10",
                    "Neema Kileo · FD-2026-00143",
                    "Received",
                    Tone::Neutral,
                )
                .on_press(Message::OpenRepair)
                .id(("received", 0)),
                repair_row(
                    "iPhone 11",
                    "Juma Hamisi · FD-2026-00144",
                    "Diagnosing",
                    Tone::Info,
                )
                .on_press(Message::OpenRepair)
                .id(("received", 1)),
            ]
            .gap(8.0)
            .fill_width(),
        )
}

/// `padding: 20px 20px 0` around the primary button with a 20 px plus.
fn new_repair() -> Container<Message> {
    section(20.0).push(
        Action::primary("New repair")
            .icon(icons::PLUS, 20.0)
            .on_press(Message::NewRepair),
    )
}

/// Follows the design's links.
pub fn update(_state: &mut State, message: Message) -> Nav {
    match message {
        Message::OpenRepairs => Nav::To(Screen::RepairsList),
        Message::OpenRepair => Nav::To(Screen::RepairDetails),
        Message::NewRepair => Nav::To(Screen::NewRepair),
        Message::Tab(tab) => Nav::To(tab_destination(tab)),
    }
}
