//! Screen 10: one repair in detail. `design/html/10-repair-details.html`.
//!
//! The design draws 1380 px of content. Here the header and the two
//! actions at the bottom are fixed and the cards scroll between them.

use morningstar::ui::prelude::*;

use crate::components::{
    Action, Step, Tone, TopBar, banner, bottom_bar, card, detail_row, fact_row, hairline,
    icon_button, pill, scaffold, timeline, timeline_step, total_row,
};
use crate::icons;
use crate::theme::{
    AMBER_DARK, AMBER_TINT, CANVAS, GREEN, INK, MUTED, grotesk, heading, inter, meta, space,
};
use crate::{Nav, Screen};

/// This screen is built from the design.
pub const BUILT: bool = true;

/// The screen shows one fixed repair and remembers nothing.
#[derive(Debug, Default)]
pub struct State {}

/// What the repair screen can ask for.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// The back chevron: the repairs list.
    Back,
    /// The share icon, "Call", "WhatsApp" and "Mark as ready": the design
    /// gives them no destination.
    NotLinked,
    /// "Share on WhatsApp": the customer's tracking page.
    OpenTracking,
    /// "Record payment".
    RecordPayment,
}

/// Header, scrolling cards, fixed actions.
pub fn view(_state: &State) -> Element<Message> {
    let header = TopBar::new("FD-2026-00142", meta("Received 9 Nov · Due 12 Nov"))
        .back(Message::Back)
        .trailing(icon_button(icons::SHARE, 22.0, INK).on_press(Message::NotLinked));

    // `display: flex; flex-direction: column; gap: 12px; padding: 16px 20px 0`.
    let body = col![summary(), history(), quotation(), intake(), tracking()]
        .gap(space::CARD_GAP)
        .fill_width()
        .padding_ltrb(space::PAGE_MARGIN, 16.0, space::PAGE_MARGIN, 0.0);

    // `grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px`.
    let actions = row![
        Action::outline("Record payment").on_press(Message::RecordPayment),
        Action::primary("Mark as ready")
            .text_size(15.0)
            .on_press(Message::NotLinked),
    ]
    .gap(10.0)
    .fill_width();

    scaffold(
        "repair-details",
        CANVAS,
        Some(header.into()),
        body,
        Some(bottom_bar(actions).into()),
    )
    .into()
}

/// The device, its owner, its status, the problem, and two ways to reach
/// the customer: `gap: 14px`.
fn summary() -> Container<Message> {
    card()
        .gap(14.0)
        .push(
            // `align-items: flex-start; justify-content: space-between`.
            row![
                col![
                    grotesk("Samsung Galaxy A14", 19.0),
                    inter("Amina Juma · 0712 345 678", 13.0).color(MUTED),
                ]
                .gap(4.0),
                pill("In repair", Tone::Info),
            ]
            .space_between()
            .fill_width(),
        )
        .push(
            inter(
                "Screen cracked after a fall. Touch does not respond on the lower half.",
                14.0,
            )
            .line_height_dp(21.0)
            .fill_width(),
        )
        .push(
            row![
                Action::quiet("Call")
                    .icon(icons::CALL, 18.0)
                    .on_press(Message::NotLinked),
                Action::quiet("WhatsApp")
                    .icon(icons::CHAT, 18.0)
                    .on_press(Message::NotLinked),
            ]
            .gap(8.0)
            .fill_width(),
        )
}

/// "Status history": seven steps, four done, one current, two to come.
fn history() -> Container<Message> {
    let done = |title: &'static str, detail: &'static str| {
        timeline_step(Step::Done, title, Some(detail.into()))
    };
    card()
        .gap(14.0)
        .push(heading("Status history", 16.0))
        .push(timeline(vec![
            done("Received", "9 Nov, 09:12 · Baraka"),
            done("Diagnosing", "9 Nov, 09:40 · Baraka"),
            done(
                "Awaiting approval",
                "9 Nov, 11:05 · Quote approved by customer",
            ),
            done("Waiting for parts", "9 Nov, 14:20 · Mwanaidi"),
            timeline_step(
                Step::Current,
                "In repair",
                Some("Today, 08:30 · Baraka".into()),
            ),
            timeline_step(Step::Pending, "Ready for collection", None),
            timeline_step(Step::Pending, "Collected", None),
        ]))
}

/// "Quotation": the lines, the total and deposit, and the balance due.
fn quotation() -> Container<Message> {
    card()
        .gap(12.0)
        .push(
            row![
                heading("Quotation", 16.0),
                // `padding: 5px 10px`.
                pill("Approved", Tone::Success).padding_xy(10.0, 5.0),
            ]
            .space_between()
            .cross_align(CrossAlign::Center)
            .fill_width(),
        )
        .push(
            col![
                detail_row("Diagnosis fee", "TZS 5,000"),
                detail_row("Screen replacement", "TZS 45,000"),
                detail_row("Labour", "TZS 10,000"),
            ]
            .gap(10.0)
            .fill_width(),
        )
        .push(hairline())
        .push(
            col![
                total_row("Total", "TZS 60,000"),
                detail_row("Deposit paid · M-Pesa", "TZS 20,000"),
            ]
            .gap(10.0)
            .fill_width(),
        )
        .push(
            banner(AMBER_TINT)
                .space_between()
                .push(
                    inter("Balance to collect", 14.0)
                        .semibold()
                        .color(AMBER_DARK),
                )
                .push(grotesk("TZS 40,000", 18.0).color(AMBER_DARK)),
        )
}

/// "Recorded at intake": two facts, `gap: 8px`.
fn intake() -> Container<Message> {
    card()
        .gap(10.0)
        .push(heading("Recorded at intake", 16.0))
        .push(
            col![
                fact_row("Condition", "Cracked screen, scratches on back"),
                fact_row("Accessories", "SIM tray, case"),
            ]
            .gap(8.0)
            .fill_width(),
        )
}

/// "Customer tracking link": the link and a button to share it.
fn tracking() -> Container<Message> {
    card()
        .gap(12.0)
        .push(
            col![
                heading("Customer tracking link", 16.0),
                grotesk("fundi.tz/r/FD-2026-00142", 14.0)
                    .medium()
                    .color(GREEN),
            ]
            .gap(4.0),
        )
        .push(
            // This link has no `box-sizing: border-box`, so its 1.5 px
            // border adds to its `height: 48px`.
            Action::outline("Share on WhatsApp")
                .compact()
                .height(51.0)
                .icon(icons::CHAT, 18.0)
                .on_press(Message::OpenTracking),
        )
}

/// Follows the design's links.
pub fn update(_state: &mut State, message: Message) -> Nav {
    match message {
        Message::Back => Nav::To(Screen::RepairsList),
        Message::NotLinked => Nav::Stay,
        Message::OpenTracking => Nav::To(Screen::Tracking),
        Message::RecordPayment => Nav::To(Screen::RecordPayment),
    }
}
