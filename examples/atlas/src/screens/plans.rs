//! Screen 08: the plans. `design/html/08-plans.html`.
//!
//! Nothing is sold in this example. "Get Plus" and the three rows record
//! the plan as chosen on this phone: its button or row then says so, and a
//! line says that nothing was charged.

use morningstar::ui::prelude::*;

use crate::components::{
    MARGIN, TOP, chevron, list_card, list_row, page, primary_button, section_heading, top_bar,
    two_lines,
};
use crate::icons::{self, icon};
use crate::theme::{ACCENT, MUTED, RADIUS_CARD, RADIUS_PLAN, TINT_10, WHITE, jakarta};
use crate::{Atlas, Message, Plan, Screen};

/// What Plus gives, in the design's order.
const BENEFITS: [&str; 4] = [
    "Broader model selection",
    "Higher usage allowance",
    "Projects and expanded memory",
    "More advanced tools",
];

/// The recommended plan:
/// `display: flex; flex-direction: column; gap: 16px; padding: 20px;
/// border-radius: 24px; background: #FFFFFF; border: 1.5px solid #A0287A`.
fn plus(chosen: bool) -> Container<Message> {
    col![
        // `display: flex; align-items: center; justify-content:
        // space-between`.
        row![
            jakarta("Plus", 18.0).bold(),
            // `height: 26px; padding: 0 10px; border-radius: 13px;
            // background: rgba(160,40,122,0.10); color: #A0287A; font-size:
            // 12px; font-weight: 700`.
            Container::new(jakarta("Recommended", 12.0).bold().color(ACCENT))
                .center()
                .height(26.0)
                .padding_xy(10.0, 0.0)
                .radius(13.0)
                .background(TINT_10),
        ]
        .space_between()
        .cross_align(CrossAlign::Center)
        .fill_width(),
        // `display: flex; align-items: baseline; gap: 6px`: `font-size:
        // 38px; font-weight: 700; letter-spacing: -0.03em; line-height: 1`
        // and `font-size: 14px; color: #5E5873`.
        row![
            jakarta("$9.99", 38.0)
                .bold()
                .letter_spacing(-1.14)
                .line_height_dp(38.0),
            // Hand-placed: there is no baseline alignment in the layout
            // engine. The price's baseline is 34.5 dp down its 38 dp line
            // and this text's 14.5 down its 17.5, so its box ends 0.5 dp
            // above the row's bottom, which Chrome rounds to 1.
            jakarta("per month", 14.0).color(MUTED).offset(0.0, -1.0),
        ]
        .gap(6.0)
        .cross_align(CrossAlign::End)
        .merge_semantics(),
        // `display: flex; flex-direction: column; gap: 10px`, each line
        // `gap: 10px; font-size: 14px` after a 16 dp tick.
        Column::new()
            .extend(BENEFITS.map(|benefit| {
                row![icon(&icons::CHECK, 16.0, ACCENT), jakarta(benefit, 14.0)]
                    .gap(10.0)
                    .cross_align(CrossAlign::Center)
            }))
            .gap(10.0)
            .fill_width(),
        primary_button(
            if chosen {
                "Plus is your plan"
            } else {
                "Get Plus"
            },
            Message::Choose(Plan::Plus)
        )
        .id("get-plus"),
    ]
    .gap(16.0)
    .fill_width()
    .padding(21.5)
    .radius(RADIUS_PLAN)
    .background(WHITE)
    .border(1.5, ACCENT)
}

/// A row of "Other plans": a `button` with `min-height: 62px; gap: 12px;
/// padding: 0 16px`, two lines, the price (`font-size: 15px; font-weight:
/// 700`) and a chevron, which is a tick on the plan that was chosen.
fn other(plan: Plan, chosen: bool, last: bool) -> Container<Message> {
    list_row(
        row![
            two_lines(plan.name(), plan.detail()),
            jakarta(plan.price(), 15.0).bold(),
            if chosen {
                icon(&icons::CHECK, 16.0, ACCENT)
            } else {
                chevron()
            },
        ],
        62.0,
        12.0,
        16.0,
        last,
    )
    .on_press(Message::Choose(plan))
    .selected(chosen)
    .id(("plan", plan))
}

/// `padding: 52px 20px 24px; display: flex; flex-direction: column; gap:
/// 16px` on `#FBF8FC`.
pub fn view(app: &Atlas) -> Element<Message> {
    let chosen = app.plan();
    let content = col![
        plus(chosen == Some(Plan::Plus)),
        // `margin-top: 6px` on the heading and `margin-top: -6px` on the
        // card after it: 22 and 10 around the heading.
        Spacer::fixed(22.0),
        section_heading("Other plans"),
        Spacer::fixed(10.0),
        list_card(
            Plan::OTHERS
                .into_iter()
                .enumerate()
                .map(|(index, plan)| other(plan, chosen == Some(plan), index == 2))
                .collect(),
            RADIUS_CARD,
        ),
        Spacer::fixed(16.0),
        // `padding: 0 4px; font-size: 12px; line-height: 1.55; color:
        // #5E5873`.
        Container::new(
            jakarta(
                "Image generation, premium models and long tasks use credits. You\u{2019}ll see an estimate before anything expensive runs.",
                12.0
            )
            .line_height_dp(18.6)
            .color(MUTED)
            .fill_width(),
        )
        .fill_width()
        .padding_xy(4.0, 0.0),
    ]
    .fill_width()
    .padding_ltrb(MARGIN, 0.0, MARGIN, 24.0);
    let body = col![
        Container::new(top_bar("Plans", None))
            .fill_width()
            .padding_ltrb(10.0, TOP, 10.0, 16.0),
        ScrollView::new(content).id(Screen::Plans.scroll_id()),
    ]
    .fill();
    page(Screen::Plans, None, body).into()
}
