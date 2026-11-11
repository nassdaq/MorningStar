//! Screen 12: the customer's tracking page. `design/html/12-tracking.html`.
//!
//! The design draws 1000 px of content with the "Tracked with FundiDesk"
//! line pushed to the bottom. Here the shop's header and that line are
//! fixed and the cards scroll between them.

use morningstar::ui::prelude::*;

use crate::components::{
    Action, banner, card, detail_row, hairline, logo, progress_segments, scaffold,
};
use crate::icons;
use crate::theme::{
    BLUE, CANVAS, GREEN, GREEN_DARK, GREEN_TINT, HAIRLINE, INK, MUTED, OUTLINE, WHITE, grotesk,
    heading, inter, inter_rich, meta, radius, space,
};
use crate::{Nav, Screen};

/// This screen is built from the design.
pub const BUILT: bool = true;

/// The page shows one fixed repair and remembers nothing.
#[derive(Debug, Default)]
pub struct State {}

/// What the tracking page can ask for.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// "Call shop" (`tel:`) and "WhatsApp" (`#top`): nothing to show in
    /// the app.
    NotLinked,
    /// The "Tracked with FundiDesk" line. The design gives this page no
    /// way back into the app (it is the customer's web page), so this line
    /// returns to the repair it was shared from.
    BackToRepair,
}

/// The shop's header, scrolling cards, the fixed line at the bottom.
pub fn view(_state: &State) -> Element<Message> {
    // `display: flex; flex-direction: column; gap: 12px; padding: 16px 20px 0`.
    let body = col![status(), cost(), collect(), contact()]
        .gap(space::CARD_GAP)
        .fill_width()
        .padding_ltrb(space::PAGE_MARGIN, 16.0, space::PAGE_MARGIN, 0.0);

    scaffold(
        "tracking",
        CANVAS,
        Some(header().into()),
        body,
        Some(footer().into()),
    )
    .into()
}

/// `display: flex; align-items: center; gap: 12px; padding: 20px;
/// background: #FFFFFF; border-bottom: 1px solid #E3E8E4`: the shop's
/// initials on a green tile, its name, and the page's name.
fn header() -> Container<Message> {
    // `width: 40px; height: 40px; border-radius: 12px; background:
    // #1E8416; color: #FFFFFF; font: 700 16px 'Space Grotesk'`.
    let initials = Container::new(grotesk("KP", 16.0).color(WHITE))
        .size(40.0, 40.0)
        .center()
        .radius(radius::FIELD)
        .background(GREEN);
    col![
        row![
            initials,
            col![
                grotesk("Kariakoo Phone Clinic", 16.0),
                meta("Repair tracking"),
            ]
            .gap(2.0),
        ]
        .gap(12.0)
        .cross_align(CrossAlign::Center)
        .fill_width()
        .padding(20.0)
        .safe_area(SafeEdges::TOP),
        hairline(),
    ]
    .fill_width()
    .background(WHITE)
}

/// The status card: `border-radius: 20px; background: #FFFFFF; border: 1px
/// solid #E3E8E4; padding: 20px; gap: 16px`. (The padding is 21: in CSS it
/// starts inside the border.)
fn status() -> Container<Message> {
    // Step 5 of 7: four done, one current, two to come.
    let segments = [GREEN, GREEN, GREEN, GREEN, BLUE, OUTLINE, OUTLINE];
    col![
        col![
            grotesk("FD-2026-00142", 13.0).semibold().color(MUTED),
            inter("Samsung Galaxy A14", 15.0).semibold(),
        ]
        .gap(4.0),
        col![
            // `font-size: 12px; font-weight: 600; letter-spacing: 0.6px;
            // text-transform: uppercase; color: #5B6660`.
            inter("Current status", 12.0)
                .semibold()
                .letter_spacing(0.6)
                .uppercase()
                .color(MUTED),
            // `font-size: 32px; line-height: 38px; color: #1D4ED8;
            // letter-spacing: -0.4px`.
            grotesk("In repair", 32.0)
                .line_height_dp(38.0)
                .letter_spacing(-0.4)
                .color(BLUE),
            inter("A technician is working on your phone now.", 14.0)
                .line_height_dp(21.0)
                .fill_width(),
        ]
        .gap(6.0)
        .fill_width(),
        col![
            progress_segments(&segments),
            row![meta("Received 9 Nov"), meta("Next: Ready for collection")]
                .space_between()
                .fill_width(),
        ]
        .gap(8.0)
        .fill_width(),
        banner(GREEN_TINT)
            .gap(10.0)
            // The sentence is wider than the strip, so the browser shrinks
            // both flex items in proportion to their widths before the
            // text wraps: the 20 px icon is drawn 18.5 wide (and as high),
            // centred in its 20 px of height.
            .push(
                Container::new(
                    Icon::vector(icons::CALENDAR)
                        .size(18.5)
                        .color(GREEN_DARK)
                        .stroke_width(1.8),
                )
                .size(18.5, 20.0)
                .center(),
            )
            .push(
                inter_rich(
                    [
                        span("Expected ready "),
                        span("Thursday, 12 November").bold(),
                    ],
                    14.0,
                )
                .color(GREEN_DARK)
                .fill_width(),
            ),
    ]
    .gap(16.0)
    .fill_width()
    .padding(21.0)
    .radius(radius::HERO)
    .background(WHITE)
    .border(1.0, HAIRLINE)
}

/// "Cost": two lines, a hairline, and what is left to pay, `gap: 10px`.
fn cost() -> Container<Message> {
    card().gap(10.0).push(heading("Cost", 16.0)).push(
        col![
            detail_row("Approved estimate", "TZS 60,000"),
            detail_row("Deposit paid", "TZS 20,000"),
            hairline(),
            // `justify-content: space-between; align-items: center;
            // font-weight: 600` with the amount in Space Grotesk 18 bold.
            row![
                inter("To pay on collection", 14.0).semibold(),
                grotesk("TZS 40,000", 18.0),
            ]
            .space_between()
            .cross_align(CrossAlign::Center)
            .fill_width(),
        ]
        .gap(10.0)
        .fill_width(),
    )
}

/// "When you collect": a paragraph, `gap: 8px`.
fn collect() -> Container<Message> {
    card()
        .gap(8.0)
        .push(heading("When you collect", 16.0))
        .push(
            inter(
                "Bring this reference number or the phone number you gave the shop. The balance \
                 is paid at the shop by cash or mobile money.",
                14.0,
            )
            .line_height_dp(21.0)
            .fill_width(),
        )
}

/// "Contact the shop": the address and hours, and two ways to reach it.
fn contact() -> Container<Message> {
    card()
        .gap(12.0)
        .push(
            col![
                heading("Contact the shop", 16.0),
                inter(
                    "[Shop address], Kariakoo, Dar es Salaam\n[Opening hours]",
                    14.0
                )
                .line_height_dp(21.0)
                .color(MUTED)
                .fill_width(),
            ]
            .gap(4.0)
            .fill_width(),
        )
        .push(
            // `grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px`.
            // "Call shop" has `box-sizing: border-box`: its 1.5 px border
            // is inside its 48 px.
            row![
                Action::outline("Call shop")
                    .compact()
                    .icon(icons::CALL, 18.0)
                    .on_press(Message::NotLinked),
                Action::primary("WhatsApp")
                    .compact()
                    .icon(icons::CHAT, 18.0)
                    .on_press(Message::NotLinked),
            ]
            .gap(8.0)
            .fill_width(),
        )
}

/// `display: flex; align-items: center; justify-content: center; gap: 8px;
/// padding: 20px`: an 18 px logo and "Tracked with FundiDesk".
fn footer() -> Container<Message> {
    row![
        logo(18.0, GREEN, WHITE),
        inter_rich(
            [
                span("Tracked with "),
                span("FundiDesk").semibold().color(INK),
            ],
            12.0,
        )
        .color(MUTED),
    ]
    .gap(8.0)
    .center()
    .fill_width()
    .padding(20.0)
    .safe_area(SafeEdges::BOTTOM)
    .on_press(Message::BackToRepair)
    .pressed_background(CANVAS)
}

/// The page links nowhere in the app except back to the repair.
pub fn update(_state: &mut State, message: Message) -> Nav {
    match message {
        Message::NotLinked => Nav::Stay,
        Message::BackToRepair => Nav::To(Screen::RepairDetails),
    }
}
