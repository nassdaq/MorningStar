//! Screen 09: take in a new repair. `design/html/09-new-repair.html`.
//!
//! The design draws 1260 px of content. Here the header and the save
//! button are fixed and the form scrolls between them. Text fields are
//! static look-alikes (there is no text input yet); the chips toggle.

use morningstar::ui::prelude::*;

use crate::components::{Action, Field, TopBar, bottom_bar, chip, dashed_tile, labelled, scaffold};
use crate::icons;
use crate::theme::{GREEN_DARK, MUTED, WHITE, grotesk, inter, label, meta, overline, space};
use crate::{Nav, Screen};

/// This screen is built from the design.
pub const BUILT: bool = true;

/// "Condition on arrival", in the design's order.
const CONDITIONS: [&str; 5] = [
    "Cracked screen",
    "Scratches",
    "Cracked back",
    "Water damage",
    "Does not power on",
];

/// "Accessories received", in the design's order.
const ACCESSORIES: [&str; 5] = ["SIM tray", "SIM card", "Memory card", "Case", "Charger"];

/// One of the form's text fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextField {
    /// The customer's phone number.
    Phone,
    /// The customer's name.
    Customer,
    /// The device's brand.
    Brand,
    /// The device's model.
    Model,
    /// "Reported problem".
    Problem,
}

/// What the form holds. The default is what the design shows.
#[derive(Debug)]
pub struct State {
    /// The customer's phone number.
    pub phone: String,
    /// The customer's name.
    pub customer: String,
    /// The device's brand.
    pub brand: String,
    /// The device's model.
    pub model: String,
    /// What the customer says is wrong.
    pub problem: String,
    /// One flag per entry of the conditions, in the design's order.
    pub conditions: [bool; 5],
    /// One flag per entry of the accessories, in the design's order.
    pub accessories: [bool; 5],
}

impl Default for State {
    fn default() -> Self {
        Self {
            // The design's `value="..."` attributes.
            phone: "0712 345 678".to_owned(),
            customer: "Amina Juma".to_owned(),
            brand: "Samsung".to_owned(),
            model: "Galaxy A14".to_owned(),
            problem: String::new(),
            // `aria-pressed="true"`: cracked screen, scratches.
            conditions: [true, true, false, false, false],
            // SIM tray and case.
            accessories: [true, false, false, true, false],
        }
    }
}

/// What the new-repair form can ask for.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// The back chevron: the dashboard.
    Back,
    /// A text field changed.
    Text(TextField, String),
    /// A condition chip, by its index.
    ToggleCondition(usize),
    /// An accessory chip, by its index.
    ToggleAccessory(usize),
    /// "Add photos", "Add IMEI", "Ready date": the design gives them no
    /// destination.
    NotLinked,
    /// "Save and share receipt": the repair's details.
    Save,
}

/// Header, scrolling form, fixed save button.
pub fn view(state: &State) -> Element<Message> {
    // The line under the title is Space Grotesk 13 medium here, and the
    // time sits at the right.
    let header = TopBar::new(
        "New repair",
        grotesk("FD-2026-00145", 13.0).medium().color(MUTED),
    )
    .back(Message::Back)
    .note(meta("Today, 10:20"));

    // `display: flex; flex-direction: column; gap: 22px; padding: 20px 20px 0`.
    let body = col![
        customer(state),
        device(state),
        chips(
            "Condition on arrival",
            "condition",
            &CONDITIONS,
            &state.conditions,
            Message::ToggleCondition,
        ),
        chips(
            "Accessories received",
            "accessory",
            &ACCESSORIES,
            &state.accessories,
            Message::ToggleAccessory,
        ),
        optional(),
    ]
    .gap(22.0)
    .fill_width()
    .padding_ltrb(space::PAGE_MARGIN, 20.0, space::PAGE_MARGIN, 0.0);

    // `display: flex; flex-direction: column; gap: 10px`.
    let footer = col![
        Action::primary("Save and share receipt").on_press(Message::Save),
        // `display: flex; align-items: center; justify-content: center;
        // gap: 6px; font-size: 12px; color: #5B6660`.
        row![
            Icon::vector(icons::DOWNLOAD).size(14.0).color(MUTED),
            meta("Saved on this phone first. Works without internet."),
        ]
        .gap(6.0)
        .center()
        .fill_width(),
    ]
    .gap(10.0)
    .fill_width();

    scaffold(
        "new-repair",
        WHITE,
        Some(header.into()),
        body,
        Some(bottom_bar(footer).into()),
    )
    .into()
}

/// A section of the form: a small-caps heading and its blocks, `gap: 12px`.
fn section(title: &'static str) -> Container<Message> {
    Column::new().gap(12.0).fill_width().push(overline(title))
}

/// "Customer": the phone number with a note under it, and the name.
fn customer(state: &State) -> Container<Message> {
    section("Customer")
        .push(
            // `gap: 8px`; the note is `font-size: 12px; color: #176611;
            // font-weight: 500`.
            col![
                label("Phone number"),
                field(TextField::Phone, &state.phone)
                    .input(|input| input.keyboard(KeyboardType::Phone).max_length(16)),
                inter("Returning customer · 2 previous repairs", 12.0)
                    .medium()
                    .color(GREEN_DARK),
            ]
            .gap(space::LABEL_GAP)
            .fill_width(),
        )
        .push(labelled(
            "Customer name",
            field(TextField::Customer, &state.customer)
                .input(|input| input.capitalization(Capitalization::Words)),
        ))
}

/// "Device": brand and model side by side, and the reported problem.
fn device(state: &State) -> Container<Message> {
    section("Device")
        .push(
            // `grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px`.
            row![
                labelled("Brand", field(TextField::Brand, &state.brand)),
                labelled("Model", field(TextField::Model, &state.model)),
            ]
            .gap(10.0)
            .fill_width(),
        )
        .push(labelled(
            "Reported problem",
            // `height: 88px; box-sizing: border-box; padding: 14px 16px;
            // font: 400 15px/22px Inter`.
            Field::new(&state.problem)
                .hint("What does the customer say is wrong?")
                .multiline(88.0)
                .input(|input| input.capitalization(Capitalization::Sentences))
                .on_input(|text| Message::Text(TextField::Problem, text))
                .id(("repair", TextField::Problem)),
        ))
}

/// One single-line field of the form; the keyboard's action key moves to
/// the next one.
fn field(which: TextField, value: &str) -> Field<Message> {
    Field::new(value)
        .input(|input| input.action(ImeAction::Next))
        .on_input(move |text| Message::Text(which, text))
        .id(("repair", which))
}

/// A group of chips: `display: flex; flex-wrap: wrap; gap: 8px`. At the
/// design's width the conditions break into rows of two, two and one, and
/// the accessories into three and two.
fn chips(
    title: &'static str,
    id: &'static str,
    names: &[&'static str; 5],
    selected: &[bool; 5],
    message: fn(usize) -> Message,
) -> Container<Message> {
    section(title).push(
        Row::new()
            .extend((0..names.len()).map(|index| {
                chip(names[index], selected[index])
                    .on_press(message(index))
                    .id((id, index))
            }))
            .gap(8.0)
            .line_gap(8.0)
            .wrap()
            .fill_width(),
    )
}

/// "Optional": three dashed tiles, `grid-template-columns: repeat(3,
/// minmax(0, 1fr)); gap: 8px`.
fn optional() -> Container<Message> {
    let tile = |index: usize, icon: VectorIcon, text: &'static str| {
        dashed_tile(icon, text)
            .on_press(Message::NotLinked)
            .id(("optional", index))
    };
    section("Optional").push(
        row![
            tile(0, icons::CAMERA, "Add photos"),
            tile(1, icons::BARCODE, "Add IMEI"),
            tile(2, icons::CALENDAR, "Ready date"),
        ]
        .gap(8.0)
        .fill_width(),
    )
}

/// Toggles chips and follows the design's links.
pub fn update(state: &mut State, message: Message) -> Nav {
    match message {
        Message::Back => Nav::To(Screen::Dashboard),
        Message::Text(which, text) => {
            *match which {
                TextField::Phone => &mut state.phone,
                TextField::Customer => &mut state.customer,
                TextField::Brand => &mut state.brand,
                TextField::Model => &mut state.model,
                TextField::Problem => &mut state.problem,
            } = text;
            Nav::Stay
        }
        Message::ToggleCondition(index) => {
            if let Some(flag) = state.conditions.get_mut(index) {
                *flag = !*flag;
            }
            Nav::Stay
        }
        Message::ToggleAccessory(index) => {
            if let Some(flag) = state.accessories.get_mut(index) {
                *flag = !*flag;
            }
            Nav::Stay
        }
        Message::NotLinked => Nav::Stay,
        Message::Save => Nav::To(Screen::RepairDetails),
    }
}
