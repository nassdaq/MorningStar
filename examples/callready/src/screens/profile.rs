//! Screen 06: the profile. The app's own design, not from the design file.
//!
//! The avatar and the learner's name, which is typed here; calls done,
//! average score and streak on three tilted cards; the practice set-up,
//! which opens the scenario settings; and the plan, which opens the offer.

use morningstar::ui::prelude::*;

use crate::Screen;
use crate::components::{
    avatar, hairline, handwritten, page, page_header, pushed_screen, small_caps,
};
use crate::icons::CHEVRON;
use crate::model::{Model, NAME_LIMIT};
use crate::theme::{GREY, MUTED, PURPLE, QUIET, TEAL, YELLOW, outfit};

/// What the screen can ask for.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// The back arrow.
    Back,
    /// The name field changed: the whole new name.
    Name(String),
    /// The keyboard's Done on the name field.
    NameDone,
    /// The practice set-up: the scenario settings.
    Settings,
    /// The plan: the offer.
    Premium,
}

/// The whole screen.
pub fn view(model: &Model) -> Element<Message> {
    let content = col![
        page_header(Message::Back, "Profile"),
        Spacer::fixed(14.0),
        identity(model),
        Spacer::fixed(30.0),
        row![
            small_caps("YOUR NUMBERS"),
            handwritten("keep it up!", 26.0, -4.0),
        ]
        .space_between()
        .cross_align(CrossAlign::Center)
        .fill_width(),
        Spacer::fixed(8.0),
        numbers(model),
        Spacer::fixed(30.0),
        small_caps("PRACTICE SET-UP"),
        Spacer::fixed(4.0),
        set_up(model),
        Spacer::fixed(26.0),
        small_caps("PLAN"),
        Spacer::fixed(12.0),
        plan(model.premium),
    ];
    pushed_screen(Screen::Profile, page(content), None)
}

/// The avatar and the name field.
fn identity(model: &Model) -> Container<Message> {
    // The field's frame: quiet until it is being typed in.
    let field = Row::new()
        .push(
            TextInput::new(&model.name)
                .style(outfit("", 20.0).semibold())
                .placeholder("Your name")
                .max_length(NAME_LIMIT)
                .capitalization(Capitalization::Words)
                .no_autocorrect()
                .action(ImeAction::Done)
                .on_input(Message::Name)
                .on_submit(Message::NameDone)
                .id("profile-name"),
        )
        .fill_width()
        .height(52.0)
        .cross_align(CrossAlign::Center)
        .padding_xy(16.0, 0.0)
        .radius(14.0)
        .border(2.0, QUIET)
        .focused_border(2.0, YELLOW);
    row![
        avatar(model.initials(), 72.0, 24.0),
        col![small_caps("YOUR NAME"), field].gap(8.0).fill_width(),
    ]
    .gap(16.0)
    .cross_align(CrossAlign::End)
    .fill_width()
}

/// Calls done, average score and streak.
fn numbers(model: &Model) -> Container<Message> {
    let number = |value: String, caption: &'static str, color: Color, degrees: f32| {
        col![
            outfit(value, 30.0).bold(),
            outfit(caption, 14.0).color(MUTED).fill_width(),
        ]
        .fill_width()
        .padding(14.0)
        .radius(6.0)
        .border(2.0, color)
        .rotate(degrees)
    };
    row![
        number(model.calls().to_string(), "Calls", PURPLE, -1.5),
        number(model.average().to_string(), "Average", YELLOW, 1.0),
        number(model.streak().to_string(), "Day streak", TEAL, -1.0),
    ]
    .gap(10.0)
    .fill_width()
}

/// The three settings, as one block that opens the settings screen.
fn set_up(model: &Model) -> Container<Message> {
    let line = |label: &'static str, value: String| {
        row![
            outfit(label, 17.0),
            Spacer::new(),
            outfit(value, 17.0).color(MUTED),
            Icon::vector(CHEVRON).size(16.0).color(GREY),
        ]
        .gap(8.0)
        .cross_align(CrossAlign::Center)
        .fill_width()
        .height(48.0)
    };
    let settings = &model.settings;
    col![
        line("Caller accent", settings.accent.label().to_owned()),
        hairline(),
        line("Difficulty", settings.difficulty.label().to_owned()),
        hairline(),
        line("Call length", format!("{} min", settings.length.minutes())),
    ]
    .fill_width()
    .on_press(Message::Settings)
    .pressed_background(crate::theme::BLACK)
    .id("profile-settings")
}

/// The plan, on an outlined card that opens the offer. Premium is a demo
/// state: nothing was paid for, and the card says so.
fn plan(premium: bool) -> Container<Message> {
    let (title, detail, outline) = if premium {
        (
            "Premium \u{b7} active (demo)",
            "No payment was taken.",
            YELLOW,
        )
    } else {
        ("Free plan", "3 intro calls. See Premium.", QUIET)
    };
    row![
        col![
            outfit(title, 19.0).semibold().fill_width(),
            outfit(detail, 15.0).color(MUTED).fill_width(),
        ]
        .gap(2.0)
        .fill_width(),
        Icon::vector(CHEVRON).size(16.0).color(GREY),
    ]
    .gap(14.0)
    .cross_align(CrossAlign::Center)
    .fill_width()
    .padding(16.0)
    .radius(8.0)
    .border(2.0, outline)
    .on_press(Message::Premium)
    .id("profile-premium")
}
