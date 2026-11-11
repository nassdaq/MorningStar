//! Screen 06: set up your shop. `design/html/06-shop-setup.html`.
//!
//! Text fields are static look-alikes: the framework has no text input
//! yet, so the fields show their placeholders and do not take focus.

use morningstar::ui::prelude::*;

use crate::components::{Action, Field, choice, form_screen, labelled};
use crate::icons;
use crate::theme::{GREEN, GREEN_TINT, label, lead, page_title};
use crate::{Nav, Screen};

/// This screen is built from the design.
pub const BUILT: bool = true;

/// How many technicians work in the shop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Team {
    /// "Just me".
    Solo,
    /// "2 to 3": chosen in the design.
    #[default]
    Small,
    /// "4 to 10".
    Large,
}

impl Team {
    const ALL: [Self; 3] = [Self::Solo, Self::Small, Self::Large];

    const fn label(self) -> &'static str {
        match self {
            Self::Solo => "Just me",
            Self::Small => "2 to 3",
            Self::Large => "4 to 10",
        }
    }
}

/// The language of the app.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    /// Chosen in the design.
    #[default]
    Kiswahili,
    /// The other one.
    English,
}

impl Language {
    const ALL: [Self; 2] = [Self::Kiswahili, Self::English];

    const fn label(self) -> &'static str {
        match self {
            Self::Kiswahili => "Kiswahili",
            Self::English => "English",
        }
    }
}

/// The two choices the screen remembers; the defaults are the design's.
#[derive(Debug, Default)]
pub struct State {
    /// "Shop name", as typed.
    pub name: String,
    /// "Location", as typed.
    pub location: String,
    /// The chosen team size.
    pub team: Team,
    /// The chosen language.
    pub language: Language,
}

/// What the shop setup screen can ask for.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// The shop name changed.
    Name(String),
    /// The location changed.
    Location(String),
    /// A team size.
    Team(Team),
    /// A language.
    Language(Language),
    /// "Create your shop".
    Create,
}

/// `padding: 56px 24px 32px; background: #FFFFFF`, one column.
pub fn view(state: &State) -> Element<Message> {
    form_screen("shop-setup", 56.0)
        // `width: 52px; height: 52px; border-radius: 16px; background:
        // #E9F6E7; color: #1E8416` around a 26 px icon.
        .push(
            Icon::vector(icons::SHOP)
                .size(52.0)
                .radius(16.0)
                .background(GREEN_TINT)
                .color(GREEN),
        )
        .push(Spacer::fixed(20.0))
        .push(
            col![
                page_title("Set up your shop"),
                lead("These details appear on customer receipts and the repair tracking page."),
            ]
            .gap(8.0)
            .fill_width(),
        )
        .push(Spacer::fixed(26.0))
        .push(form(state))
        // `margin-top: auto`: pushed to the bottom.
        .push(Spacer::new())
        .push(Action::primary("Create your shop").on_press(Message::Create))
        .into()
}

/// The two fields and the two groups of choices: `gap: 18px`.
fn form(state: &State) -> Container<Message> {
    let teams = Team::ALL.map(|team| {
        choice(team.label(), team == state.team)
            .on_press(Message::Team(team))
            .id(("team", team as usize))
    });
    let languages = Language::ALL.map(|language| {
        choice(language.label(), language == state.language)
            .on_press(Message::Language(language))
            .id(("language", language as usize))
    });
    col![
        labelled(
            "Shop name",
            Field::new(&state.name)
                .hint("Enter the name of your shop")
                .input(|input| {
                    input
                        .capitalization(Capitalization::Words)
                        .action(ImeAction::Next)
                })
                .on_input(Message::Name)
                .id("shop-name"),
        ),
        labelled(
            "Location",
            Field::new(&state.location)
                .hint("Street or building, e.g. Kariakoo")
                .input(|input| input.capitalization(Capitalization::Words))
                .on_input(Message::Location)
                .id("shop-location"),
        ),
        group("How many technicians work here?", Row::new().extend(teams)),
        group("App language", Row::new().extend(languages)),
    ]
    .gap(18.0)
    .fill_width()
}

/// A label over a row of choices: `display: flex; flex-direction: column;
/// gap: 10px` over `display: grid; gap: 8px`.
fn group(text: &'static str, choices: Container<Message>) -> Container<Message> {
    col![label(text), choices.gap(8.0).fill_width()]
        .gap(10.0)
        .fill_width()
}

/// Follows the design's link.
pub fn update(state: &mut State, message: Message) -> Nav {
    match message {
        Message::Name(name) => {
            state.name = name;
            Nav::Stay
        }
        Message::Location(location) => {
            state.location = location;
            Nav::Stay
        }
        Message::Team(team) => {
            state.team = team;
            Nav::Stay
        }
        Message::Language(language) => {
            state.language = language;
            Nav::Stay
        }
        Message::Create => Nav::To(Screen::Dashboard),
    }
}
