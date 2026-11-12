//! Screen 05: the subscription offer. `design/html/05-premium.html`.
//!
//! The name with "PRO" in cut-out letters, what the subscription gives,
//! the two plans as tilted cards, and the button that starts it. The
//! button stays put; the rest scrolls on a phone shorter than the design's.
//!
//! The two cards select a plan. The button records the selected plan and
//! returns; the cross returns without recording. There is no payment in
//! this app: once Premium is recorded, the button and the small print say
//! that it is active as a demo, and so do More and the profile.

use morningstar::ui::prelude::*;

use crate::Screen;
use crate::components::{page, plate, primary_button, pushed_screen};
use crate::icons::{CHECK, CLOSE};
use crate::model::Plan;
use crate::theme::{
    BLACK, MUTED, PURPLE, QUIET, RED, SILVER, WHITE, YELLOW, abril_fatface, archivo_black, caveat,
    outfit,
};

/// What the screen can ask for.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// The cross: back, with nothing recorded.
    Close,
    /// A plan card.
    Select(Plan),
    /// The button: records the selected plan and goes back.
    Start,
}

/// `padding: 52px 24px 28px; display: flex; flex-direction: column`.
/// `selected` is the plan whose card is selected; `active` whether Premium
/// has been started.
pub fn view(selected: Plan, active: bool) -> Element<Message> {
    let content = col![
        // `align-self: flex-end; width: 44px; height: 44px;
        // align-items: center; justify-content: flex-end`.
        Row::new()
            .push(Icon::vector(CLOSE).size(22.0).color(WHITE))
            .size(44.0, 44.0)
            .main_align(MainAlign::End)
            .cross_align(CrossAlign::Center)
            .align_self(CrossAlign::End)
            .on_press(Message::Close)
            .pressed_background(BLACK)
            .label("Close")
            .id("close"),
        Spacer::fixed(4.0),
        hero(),
        // `margin: 6px 0 0; font-size: 18px; line-height: 1.4; color: #C7C7CC`.
        // Chrome put the line at y = 237, 0.35 above where its half-leading
        // gives; the benefits below are where the margins put them.
        Spacer::fixed(5.65),
        outfit("Practise the real job before your first shift.", 18.0)
            .line_height_dp(25.2)
            .color(SILVER),
        Spacer::fixed(22.35),
        benefits(),
        Spacer::fixed(26.0),
        plans(selected),
    ];
    // `margin-top: auto` on the button: it is the fixed footer.
    pushed_screen(
        Screen::Premium,
        page(content),
        Some(footer(selected, active)),
    )
}

/// A cut-out letter of "PRO": `line-height: 1` and a padding (across,
/// down) on a coloured plate.
fn letter(letter: Text, size: f32, padding: (f32, f32), color: Color) -> Container<Message> {
    plate(letter.line_height_dp(size), padding.0, padding.1, color)
}

/// `position: relative; height: 130px`: the name, "PRO" at
/// `left: 150px; top: 44px` and the note at `left: 6px; top: 64px`.
fn hero() -> Container<Message> {
    stack![
        // `font-size: 40px; font-weight: 700; line-height: 1.1`.
        outfit("CallReady", 40.0).bold().line_height_dp(44.0),
        // `display: flex; align-items: flex-end; gap: 3px`.
        row![
            // `background: #9B5DE5; font-family: 'Archivo Black';
            // font-size: 56px; padding: 6px 8px; transform: rotate(-6deg)`.
            letter(archivo_black("P", 56.0), 56.0, (8.0, 6.0), PURPLE).rotate(-6.0),
            // `background: #F6C445; font-family: 'Abril Fatface';
            // font-size: 62px; padding: 2px 9px;
            // transform: rotate(5deg) translateY(-8px)`.
            letter(abril_fatface("R", 62.0), 62.0, (9.0, 2.0), YELLOW)
                .transform(Transform::rotate(5.0).and_translate(0.0, -8.0)),
            // `background: #E63946; font-family: 'Archivo Black';
            // font-size: 52px; padding: 6px 7px; transform: rotate(-3deg)`.
            letter(archivo_black("O", 52.0), 52.0, (7.0, 6.0), RED).rotate(-3.0),
        ]
        .gap(3.0)
        .cross_align(CrossAlign::End)
        // The row is 182.15 dp wide and the block 342 at the design's
        // width, so `left: 150px` is 9.85 dp short of the block's right
        // edge; pinned there, the letters stay on a narrower screen.
        .place(Alignment::TOP_RIGHT)
        .offset(-9.85, 44.0),
        // `font-family: 'Caveat'; font-size: 34px; color: #F6C445;
        // transform: rotate(-7deg)`.
        caveat("go", 34.0)
            .color(YELLOW)
            .rotate(-7.0)
            .place(Alignment::TOP_LEFT)
            .offset(6.0, 64.0),
    ]
    .fill_width()
    .height(130.0)
    // The name, three cut-out letters and a scribble: one title.
    .label("CallReady Pro")
    .role(Role::Heading)
}

/// `display: flex; flex-direction: column; gap: 14px; font-size: 17px`.
fn benefits() -> Container<Message> {
    let benefit = |label: Element<Message>| {
        // `display: flex; gap: 12px; align-items: center`.
        row![Icon::vector(CHECK).size(22.0).color(YELLOW), label,]
            .gap(12.0)
            .cross_align(CrossAlign::Center)
            .fill_width()
    };
    col![
        benefit(
            outfit("100 minutes of AI call practice a month", 17.0)
                .fill_width()
                .into()
        ),
        benefit(outfit("Every customer scenario", 17.0).fill_width().into()),
        benefit(
            outfit("Detailed feedback after each call", 17.0)
                .fill_width()
                .into()
        ),
        benefit(outfit("Interview preparation", 17.0).fill_width().into()),
        benefit(
            outfit("Your full improvement history", 17.0)
                .fill_width()
                .into()
        ),
    ]
    .gap(14.0)
    .fill_width()
}

/// `display: grid; grid-template-columns: repeat(2, minmax(0, 1fr));
/// gap: 10px`: the first card turned by -1.5 degrees, the second by 1.
fn plans(selected: Plan) -> Container<Message> {
    row![
        plan(
            Plan::Premium,
            "Premium",
            "$4.99",
            "per month",
            -1.5,
            selected
        ),
        plan(Plan::Free, "Free", "$0", "3 intro calls", 1.0, selected),
    ]
    .gap(10.0)
    .fill_width()
}

/// `border: 2px solid; border-radius: 8px; padding: 14px`, in the accent
/// colour with the "SELECTED" badge when it is the selected plan, in
/// `#3A3A3C` when it is not.
fn plan(
    plan: Plan,
    name: &'static str,
    price: &'static str,
    terms: &'static str,
    degrees: f32,
    selected: Plan,
) -> Container<Message> {
    let is_selected = plan == selected;
    let card = col![
        // `font-size: 15px; color: #C7C7CC`.
        outfit(name, 15.0).color(SILVER),
        Spacer::fixed(4.0),
        // `font-size: 30px; font-weight: 700; margin-top: 4px`.
        outfit(price, 30.0).bold(),
        // `font-size: 14px; color: #A1A1A6`.
        outfit(terms, 14.0).color(MUTED),
    ]
    .fill_width()
    .padding(16.0)
    .radius(8.0)
    .border(2.0, if is_selected { YELLOW } else { QUIET });
    // `position: absolute; top: -12px; right: 10px` from the inside of the
    // border: 10 above the card and 12 in from its right edge.
    // `font-size: 12px; font-weight: 700; padding: 3px 8px;
    // border-radius: 4px`.
    let badge = is_selected.then(|| {
        Container::new(outfit("SELECTED", 12.0).bold().color(BLACK))
            .size(76.55, 21.0)
            .center()
            .radius(4.0)
            .background(YELLOW)
            .place(Alignment::TOP_RIGHT)
            .offset(-12.0, -10.0)
    });
    Stack::new()
        .push(card)
        .push_maybe(badge)
        .fill_width()
        .rotate(degrees)
        .on_press(Message::Select(plan))
        // The badge is how the eye knows which plan is chosen; the state
        // is how the ear does.
        .label(format!("{name}, {price}, {terms}"))
        .selected(is_selected)
        .id(("plan", plan))
}

/// `margin-top: auto`: the button and the small print. They say what the
/// button will do: start Premium (the design's wording), or that the
/// selected plan is already the learner's.
fn footer(selected: Plan, active: bool) -> Container<Message> {
    let (label, small_print) = match (selected, active) {
        (Plan::Premium, false) => (
            "Start Premium",
            "Billed through Google Play \u{b7} Cancel anytime",
        ),
        (Plan::Premium, true) => ("Premium is active", "Demo only \u{b7} No payment was taken"),
        (Plan::Free, true) => ("Switch to Free", "Demo only \u{b7} Ends Premium at once"),
        (Plan::Free, false) => ("Stay on Free", "3 intro calls \u{b7} No payment needed"),
    };
    col![
        // `height: 64px; border-radius: 22px; background: #F6C445;
        // color: #0E0E0E; font-size: 20px; font-weight: 700`.
        primary_button(label, 20.0, 64.0, Message::Start).id("start"),
        // `margin: 12px 0 0; text-align: center; font-size: 14px`.
        outfit(small_print, 14.0)
            .color(MUTED)
            .centered()
            .fill_width(),
    ]
    .gap(12.0)
    .fill_width()
}
