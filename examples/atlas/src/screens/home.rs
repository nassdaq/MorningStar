//! Screens 03 and 09: home. `design/html/03-home.html` (and
//! `09-home-simplified.html`, the same page with three other links: see
//! [`Flow`](crate::Flow)).
//!
//! The name and the profile button, the orb over the question, three
//! suggestions that each start a chat, the model chip, the composer and
//! the tab bar. The bars and the composer stay put; the middle scrolls on
//! a phone shorter than the design's.

use morningstar::ui::prelude::*;

use crate::components::{
    MARGIN, TOP, attachment_chip, chevron, composer, model_chip, orb, page, profile_button,
    tab_bar, wash_home, wordmark,
};
use crate::icons::icon;
use crate::theme::{ACCENT, LINE, MUTED, QUIET, RADIUS_ROW, WHITE, jakarta};
use crate::{Atlas, Link, Message, SUGGESTIONS, Tab};

/// Whether answers are live, beside the model chip: a dot and a word, and
/// the way to the Server screen. **The app's own**, in the model chip's
/// type (`font-size: 13px; font-weight: 600; color: #5E5873`). It is there
/// only when there is a server to be live with or offline from: with
/// nothing behind the app, home is the design's.
fn link_mark(link: &Link) -> Option<Container<Message>> {
    let (word, color, label) = match link {
        Link::Online { .. } => ("Live", ACCENT, "Answers are live. Opens the server screen"),
        Link::Offline(_) => (
            "Offline",
            QUIET,
            "The server is offline: answers are scripted. Opens the server screen",
        ),
        Link::Standalone | Link::Checking => return None,
    };
    Some(
        row![
            Container::empty()
                .size(7.0, 7.0)
                .radius(3.5)
                .background(color),
            jakarta(word, 13.0).semibold().color(MUTED),
        ]
        .gap(6.0)
        .cross_align(CrossAlign::Center)
        // The app's own, so as tall as Android asks (the chip is 44).
        .height(48.0)
        .min_width(48.0)
        .padding_xy(8.0, 0.0)
        .radius(24.0)
        .on_press(Message::OpenServer)
        .label(label)
        .id("link"),
    )
}

/// A suggestion: a `button` with `height: 52px; display: flex;
/// align-items: center; gap: 12px; padding: 0 16px; border-radius: 16px;
/// background: #FFFFFF; border: 1px solid rgba(29,24,48,0.08); font-size:
/// 14px; font-weight: 600`, an 18 dp icon in the accent and a chevron.
fn suggestion(index: usize) -> Container<Message> {
    let (label, asset, _) = SUGGESTIONS[index];
    row![
        icon(asset, 18.0, ACCENT),
        jakarta(label, 14.0).semibold().fill_width(),
        chevron(),
    ]
    .gap(12.0)
    .cross_align(CrossAlign::Center)
    .fill_width()
    .height(52.0)
    // The button's 1 px border is inside its padding box.
    .padding_xy(17.0, 0.0)
    .radius(RADIUS_ROW)
    .background(WHITE)
    .border(1.0, LINE)
    .on_press(Message::Suggest(index))
    .id(("suggestion", index))
}

/// The attachment that the next message will carry, above the composer:
/// the app's own (the design shows an attachment only once it is sent).
pub fn pending_attachment(app: &Atlas) -> Option<Container<Message>> {
    app.attachment().map(|name| {
        Container::new(
            attachment_chip(format!("{name} \u{b7} remove"))
                .on_press(Message::Detach)
                .id("detach"),
        )
        .fill_width()
        .padding_ltrb(0.0, 0.0, 0.0, 8.0)
    })
}

/// `padding: 52px 20px 88px; display: flex; flex-direction: column` over
/// `radial-gradient(circle at 50% 28%, rgba(246,170,214,0.3),
/// rgba(246,170,214,0) 46%), #FBF8FC`, with the tab bar at the bottom.
/// `room` is the height the screen has.
pub fn view(app: &Atlas, room: f32) -> Element<Message> {
    let screen = app.screen();
    // `display: flex; align-items: center; justify-content: space-between`.
    let header = row![wordmark(), profile_button()]
        .space_between()
        .cross_align(CrossAlign::Center)
        .fill_width()
        .padding_ltrb(MARGIN, TOP, MARGIN, 0.0);
    let pending = pending_attachment(app);
    // What does not scroll: the header (52 + 46), the composer with the 27
    // dp between it and the bar (56 + 14 + 13), the bar (75).
    let fixed = TOP + 46.0 + 83.0 + 75.0 + if pending.is_some() { 42.0 } else { 0.0 };
    let middle = col![
        // `flex-grow: 1; display: flex; flex-direction: column;
        // align-items: center; justify-content: center; gap: 34px`.
        col![
            orb(96.0),
            // `font-size: 28px; font-weight: 700; line-height: 1.2;
            // letter-spacing: -0.02em; text-align: center`.
            jakarta("What can I help with?", 28.0)
                .bold()
                .line_height_dp(33.6)
                .letter_spacing(-0.56)
                .centered()
                .fill_width()
                .heading(),
        ]
        .gap(34.0)
        .center()
        .fill_width()
        .fill_height()
        .min_height(200.0),
        // `display: flex; flex-direction: column; gap: 8px`.
        Column::new()
            .extend((0..SUGGESTIONS.len()).map(suggestion))
            .gap(8.0)
            .fill_width(),
        // `align-self: flex-start; margin-top: 8px; padding: 0 6px`.
        Spacer::fixed(8.0),
        match link_mark(app.link()) {
            None => model_chip(app.mode(), 6.0),
            // The same chip, with the mark at the row's other end.
            Some(mark) => row![model_chip(app.mode(), 6.0), mark]
                .space_between()
                .cross_align(CrossAlign::Center)
                .fill_width(),
        },
    ]
    .fill_width()
    .min_height((room - fixed).max(0.0))
    .padding_xy(MARGIN, 0.0);
    let body = Column::new()
        .push(header)
        .push(ScrollView::new(middle).id(screen.scroll_id()))
        .push(
            // `margin-bottom: 14px`, then the 13 dp left of the page's
            // 88 dp bottom padding above the 75 dp bar.
            Column::new()
                .push_maybe(pending)
                .push(composer(app.draft(), "Ask anything"))
                .fill_width()
                .padding_ltrb(MARGIN, 0.0, MARGIN, 27.0),
        )
        .push(tab_bar(Tab::Home))
        .fill();
    page(screen, Some(wash_home()), body)
        .safe_area(SafeEdges::TOP)
        .into()
}
