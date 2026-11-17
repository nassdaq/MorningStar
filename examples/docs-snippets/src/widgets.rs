//! Samples for the widget pages: boxes and their paint, buttons, dividers,
//! avatars and icons.

use morningstar::ui::prelude::*;

/// A card: fill, radius, border and shadow on one box.
pub fn card<M: 'static>() -> Element<M> {
    // docs:begin card
    Container::new(
        col![
            text("Samsung A15").headline(),
            text("Screen replacement").muted(),
        ]
        .gap(4.0),
    )
    .padding(16.0)
    .radius(16.0)
    .background(ThemeColor::Surface)
    .border(1.0, ThemeColor::Divider)
    // One of three ready-made shadows; `Shadow::new(x, y, blur, colour)`
    // takes the numbers a design gives.
    .shadow(elevation::MEDIUM)
    .fill_width()
    // docs:end card
    .into()
}

/// Gradients: two colours along a line, and stops around a centre.
pub fn gradients<M: 'static>() -> Element<M> {
    // docs:begin gradients
    col![
        Container::empty()
            .height(80.0)
            .fill_width()
            .radius(16.0)
            .gradient(Gradient::vertical(hex(0x2F6BFF), hex(0x1B3FA6))),
        Container::empty()
            .height(80.0)
            .fill_width()
            .radius(16.0)
            .background(hex(0x0D0F15))
            // A glow: a circle centred a third of the way down.
            .radial(
                Radial::circle()
                    .at(0.5, 0.33)
                    .stop(0.0, hexa(0x7C5CFFCC))
                    .stop(1.0, hexa(0x7C5CFF00)),
            ),
    ]
    .gap(12.0)
    .fill_width()
    // docs:end gradients
    .into()
}

/// The five kinds of button, and one that is disabled.
pub fn buttons(can_save: bool) -> Element<Choice> {
    // docs:begin buttons
    col![
        Button::new("Save").on_press(Choice::Save),
        Button::new("Later").secondary().on_press(Choice::Later),
        Button::new("Details").outlined().on_press(Choice::Details),
        Button::new("Skip").plain().on_press(Choice::Skip),
        Button::new("Delete").destructive().on_press(Choice::Delete),
        // No message, no tap: the button is drawn disabled.
        Button::new("Save").on_press_maybe(can_save.then_some(Choice::Save)),
    ]
    .gap(8.0)
    // docs:end buttons
    .into()
}

/// What the buttons above send.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Save,
    Later,
    Details,
    Skip,
    Delete,
}

/// Anything can be tapped: a row made into one target.
pub fn pressable_row(order: u32) -> Element<u32> {
    // docs:begin pressable
    Pressable::new(
        row![
            Avatar::new("Amina Juma").size(40.0),
            col![text("Amina Juma"), text("Order 1042").caption().muted()].fill_width(),
            text("TSh 45,000").label(),
        ]
        .gap(12.0)
        .cross_align(CrossAlign::Center)
        .fill_width(),
    )
    .padding_xy(16.0, 12.0)
    .on_press(order)
    .fill_width()
    // docs:end pressable
    .into()
}

/// Lines between rows.
pub fn dividers<M: 'static>() -> Element<M> {
    // docs:begin dividers
    col![
        text("Inbox"),
        Divider::new(),
        text("Drafts"),
        // Starts 56 dp in, level with the text beside an avatar.
        Divider::new().inset(56.0),
        text("Sent"),
    ]
    .gap(12.0)
    .fill_width()
    // docs:end dividers
    .into()
}

// docs:begin icons
/// One-colour markup in the source: the path of a 24 by 24 icon.
const CHECK: VectorIcon = VectorIcon::stroked("M5 12l5 5L20 7");

/// An icon alone, an icon on a round tile, and a glyph as a stand-in.
pub fn icons<M: 'static>() -> Element<M> {
    row![
        icon(CHECK).color(ThemeColor::Success),
        Icon::vector(CHECK)
            .size(40.0)
            .round()
            .background(ThemeColor::PrimaryContainer)
            .color(ThemeColor::Primary),
        Icon::new("?").size(40.0),
    ]
    .gap(12.0)
    .cross_align(CrossAlign::Center)
    .into()
}
// docs:end icons
