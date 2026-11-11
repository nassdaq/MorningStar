//! Screen 02: onboarding. `design/html/02-onboarding.html`.
//!
//! The first of three onboarding pages: the logo and "Skip", an
//! illustration of a phone built from boxes, the pitch, and the page dots
//! beside a round "next" button.

use morningstar::ui::prelude::*;

use crate::components::{Tone, logo, page_dots, pill};
use crate::icons;
use crate::theme::{
    GREEN, GREEN_SOFT, GREEN_TINT, INK, MUTED, OUTLINE, PANEL, WHITE, grotesk, inter, inter_rich,
    page_title, radius,
};
use crate::{Nav, Screen};

/// This screen is built from the design.
pub const BUILT: bool = true;

/// The onboarding screen remembers nothing: the design has one page.
#[derive(Debug, Default)]
pub struct State {}

/// What the onboarding screen can ask for.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// "Skip".
    Skip,
    /// The round arrow button.
    Continue,
}

/// `box-shadow: 0 12px 28px rgba(20, 32, 26, 0.14)` under the floating card.
///
/// The alpha is 0.28, not the design's 0.14: the renderer blends a shadow
/// in linear light, where 0.14 of ink over white comes out half as dark as
/// a browser's blend of the encoded values. 0.28 gives the browser's
/// shades over the whole fade (within two levels of 255).
const CARD_SHADOW: Shadow = Shadow::new(0.0, 12.0, 28.0, with_alpha(INK, 0.28));

/// `box-shadow: 0 10px 22px rgba(30, 132, 22, 0.3)` under the green tile,
/// with the alpha raised from 0.3 for the same reason as [`CARD_SHADOW`].
const TILE_SHADOW: Shadow = Shadow::new(0.0, 10.0, 22.0, with_alpha(GREEN, 0.45));

/// `padding: 28px 24px 32px; background: #FFFFFF`, one column.
pub fn view(_state: &State) -> Element<Message> {
    Column::new()
        .fill()
        .background(WHITE)
        .padding_ltrb(24.0, 28.0, 24.0, 32.0)
        .safe_area(SafeEdges::ALL)
        .push(header())
        .push(Spacer::fixed(28.0))
        .push(illustration())
        .push(Spacer::fixed(28.0))
        .push(pitch())
        // `margin-top: auto`: pushed to the bottom.
        .push(Spacer::new())
        .push(footer())
        .id("onboarding")
        .into()
}

/// `display: flex; align-items: center; justify-content: space-between`:
/// the 28 dp logo and the name (`gap: 8px`), and "Skip".
fn header() -> Container<Message> {
    row![
        row![
            logo(28.0, GREEN, WHITE),
            // `font-size: 18px; font-weight: 700; color: #1E8416`.
            grotesk("FundiDesk", 18.0).color(GREEN),
        ]
        .gap(8.0)
        .cross_align(CrossAlign::Center),
        // `font-size: 14px; font-weight: 600; color: #1E8416; padding: 12px
        // 4px`.
        Container::new(inter("Skip", 14.0).semibold().color(GREEN))
            .padding_xy(4.0, 12.0)
            .on_press(Message::Skip)
            .pressed_background(WHITE),
    ]
    .space_between()
    .cross_align(CrossAlign::Center)
    .fill_width()
}

/// `position: relative; height: 380px; display: flex; align-items: center;
/// justify-content: center`: a pale circle behind a phone, with a card
/// over its lower left and a green tile over its upper right.
fn illustration() -> Container<Message> {
    stack![
        // `width: 300px; height: 300px; border-radius: 50%; background:
        // #E9F6E7`.
        Container::<Message>::empty()
            .size(300.0, 300.0)
            .radius(150.0)
            .background(GREEN_TINT)
            .place(Alignment::CENTER),
        phone().place(Alignment::CENTER),
        // `position: absolute; left: 20px; bottom: 44px`.
        repair_card()
            .place(Alignment::BOTTOM_LEFT)
            .offset(20.0, -44.0),
        // `position: absolute; right: 18px; top: 40px`.
        wrench_tile()
            .place(Alignment::TOP_RIGHT)
            .offset(-18.0, 40.0),
    ]
    .fill_width()
    .height(380.0)
}

/// The phone: `width: 168px; height: 320px; box-sizing: border-box;
/// border-radius: 28px; border: 6px solid #14201A; background: #FFFFFF;
/// padding: 22px 12px 12px; gap: 10px`. The padding starts inside the
/// border, which leaves the content 132 dp wide.
fn phone() -> Container<Message> {
    col![
        // `height: 10px; width: 64px; border-radius: 5px`.
        bar(64.0, 10.0, INK),
        phone_panel(GREEN_TINT, (0.70, GREEN), (0.90, GREEN_SOFT)),
        phone_panel(PANEL, (0.55, MUTED), (0.80, OUTLINE)),
        phone_panel(PANEL, (0.62, MUTED), (0.72, OUTLINE)),
    ]
    .gap(10.0)
    .size(168.0, 320.0)
    .padding_ltrb(18.0, 28.0, 18.0, 18.0)
    .radius(28.0)
    .background(WHITE)
    .border(6.0, INK)
}

/// The width inside a panel of the phone: the phone's 132 dp of content
/// less the panel's `padding: 10px` on each side. The bars' percentage
/// widths are shares of it.
const PANEL_INNER: f32 = 112.0;

/// A panel on the phone's screen: `border-radius: 10px; padding: 10px;
/// gap: 6px` with a bar `height: 8px` over one `height: 6px`, each given as
/// its share of the width and its colour.
fn phone_panel(background: Color, top: (f32, Color), bottom: (f32, Color)) -> Container<Message> {
    col![
        bar(PANEL_INNER * top.0, 8.0, top.1),
        bar(PANEL_INNER * bottom.0, 6.0, bottom.1),
    ]
    .gap(6.0)
    .padding(10.0)
    .fill_width()
    .radius(radius::SMALL)
    .background(background)
}

/// A bar with fully rounded ends (`border-radius` of half its height).
fn bar(width: f32, height: f32, color: Color) -> Container<Message> {
    Container::empty()
        .size(width, height)
        .radius(height / 2.0)
        .background(color)
}

/// The floating card: `width: 214px; box-sizing: border-box; border-radius:
/// 16px; background: #FFFFFF; border: 1px solid #D9E0DB; box-shadow: 0 12px
/// 28px rgba(20, 32, 26, 0.14); padding: 14px; gap: 8px`.
fn repair_card() -> Container<Message> {
    col![
        // `font-family: 'Space Grotesk'; font-size: 13px; font-weight: 600;
        // color: #5B6660`.
        grotesk("FD-2026-00142", 13.0).semibold().color(MUTED),
        inter("Samsung Galaxy A14", 15.0).semibold(),
        // `align-self: flex-start; padding: 5px 10px`.
        pill("Ready for collection", Tone::Success).padding_xy(10.0, 5.0),
    ]
    .gap(8.0)
    .width(214.0)
    .padding(15.0)
    .radius(radius::CARD)
    .background(WHITE)
    .border(1.0, OUTLINE)
    .shadow(CARD_SHADOW)
}

/// The floating tile: `width: 56px; height: 56px; border-radius: 18px;
/// background: #1E8416; box-shadow: 0 10px 22px rgba(30, 132, 22, 0.3)`
/// around a 28 dp spanner of stroke 2.
fn wrench_tile() -> Container<Message> {
    Container::new(
        Icon::vector(icons::WRENCH)
            .size(28.0)
            .color(WHITE)
            .stroke_width(2.0),
    )
    .size(56.0, 56.0)
    .center()
    .radius(18.0)
    .background(GREEN)
    .shadow(TILE_SHADOW)
}

/// `display: flex; flex-direction: column; gap: 12px`: the title and the
/// paragraph (`font-size: 15px; line-height: 23px; color: #5B6660`, with
/// "delayed" in `color: #1E8416; font-weight: 600`).
fn pitch() -> Container<Message> {
    col![
        // After the comma stand a thin space and a hair space instead of a
        // space. Space Grotesk kerns a comma and a space together by 30 of
        // 1000 units, which a browser applies and the shaper here, working
        // word by word, does not; the pair is as wide as the kerned space
        // to within a quarter of a dp, and a line may still break there.
        page_title("Every phone in your workshop,\u{2009}\u{200a}accounted for").fill_width(),
        inter_rich(
            [
                span("Log each device the moment it comes in. See what is "),
                span("delayed").semibold().color(GREEN),
                span(", what is ready and what is still unpaid."),
            ],
            15.0,
        )
        .line_height_dp(23.0)
        .color(MUTED)
        .fill_width(),
    ]
    .gap(12.0)
    .fill_width()
}

/// `display: flex; align-items: center; justify-content: space-between`:
/// the page dots and the round "next" button.
fn footer() -> Container<Message> {
    row![page_dots(0, 3), next_button()]
        .space_between()
        .cross_align(CrossAlign::Center)
        .fill_width()
}

/// `width: 56px; height: 56px; border-radius: 50%; background: #1E8416`
/// around a 24 dp white arrow of stroke 2.2.
fn next_button() -> Container<Message> {
    Container::new(Icon::vector(icons::ARROW_RIGHT).size(24.0).color(WHITE))
        .size(56.0, 56.0)
        .center()
        .radius(28.0)
        .background(GREEN)
        .on_press(Message::Continue)
        .id("onboarding-continue")
}

/// Follows the design's links: both lead to the login screen.
pub fn update(_state: &mut State, message: Message) -> Nav {
    match message {
        Message::Skip | Message::Continue => Nav::To(Screen::Login),
    }
}
