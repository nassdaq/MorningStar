//! Samples for the text page.

use morningstar::ui::prelude::*;

/// The six steps of the type scale.
pub fn roles<M: 'static>() -> Element<M> {
    // docs:begin roles
    col![
        text("TSh 1,250,000").display(),
        text("Repairs").title(),
        text("Waiting for parts").headline(),
        text("The screen was replaced and tested with the customer."),
        text("SAVE").label(),
        text("Updated 08:02").caption().muted(),
    ]
    .gap(8.0)
    // docs:end roles
    .into()
}

/// Overrides of a role, and a phrase styled inside a sentence.
pub fn styled<M: 'static>() -> Element<M> {
    // docs:begin styled
    col![
        text("Order 1042")
            .size(22.0)
            .semibold()
            .letter_spacing(-0.3)
            .line_height_dp(28.0),
        // One text, three styles: spans share the line breaking.
        Text::rich([
            span("By continuing you accept the "),
            span("terms of service").bold().color(ThemeColor::Primary),
            span("."),
        ])
        .muted()
        .fill_width(),
        // Digits on a common width, so a column of amounts lines up.
        text("12,400.00").tabular_nums(),
        // At most two lines; the rest is cut off.
        text("A long note that the customer wrote when the phone was handed in.")
            .max_lines(2)
            .fill_width(),
    ]
    .gap(8.0)
    .fill_width()
    // docs:end styled
    .into()
}
