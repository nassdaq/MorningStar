//! Samples for themes and the system's bars.

use morningstar::ui::prelude::*;

// docs:begin theme
/// The dark theme with the brand's colours.
const THEME: Theme = Theme {
    colors: Colors {
        background: hex(0x0B0B0C),
        surface: hex(0x121214),
        primary: hex(0xE3AE3A),
        on_primary: hex(0x141005),
        ..Theme::DARK.colors
    },
    ..Theme::DARK
};

pub struct Branded {
    pub onboarding: bool,
}

impl Application for Branded {
    type Message = ();

    fn theme(&self) -> Theme {
        THEME
    }

    fn view(&self) -> Element<()> {
        // Colours are named, not copied: the same view follows whatever
        // theme the application returns.
        Container::new(text("Morningstar").title().color(ThemeColor::Primary))
            .background(ThemeColor::Background)
            .center()
            .fill()
            .into()
    }

    fn update(&mut self, _message: ()) {}

    // The status and navigation bars' icons follow the theme by
    // themselves. A screen whose edges are not the theme's background
    // says what is behind them: here, a light picture under onboarding.
    fn system_bars(&self) -> Option<SystemBars> {
        self.onboarding.then_some(SystemBars::DARK_CONTENT)
    }
}
// docs:end theme
