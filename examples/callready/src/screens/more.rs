//! Screen 12: the menu, the fourth tab. The app's own design, not from the
//! design file.
//!
//! The ways to everything that is not a tab: the profile, notifications,
//! the scenario settings and the offer, each row saying where things
//! stand; the server the calls are played by; and what kind of app this
//! is: live conversations with an AI caller when a server is connected,
//! scripted calls on the phone when none is.

use morningstar::ui::prelude::*;

use crate::components::{body, hairline, link_row, page, page_title, tab_screen};
use crate::model::Model;
use crate::{Screen, Tab};

/// What the screen can ask for.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// An item of the tab bar.
    Tab(Tab),
    /// The profile.
    Profile,
    /// The notifications.
    Notifications,
    /// The scenario settings.
    Settings,
    /// The offer.
    Premium,
    /// The server the calls are played by.
    Server,
}

/// The whole screen.
pub fn view(model: &Model) -> Element<Message> {
    let unread = model.notifications().iter().filter(|n| !n.read).count();
    let settings = &model.settings;
    let content = col![
        page_title("More"),
        Spacer::fixed(10.0),
        link_row("Profile", model.name.clone(), Message::Profile).id("more-profile"),
        hairline(),
        link_row(
            "Notifications",
            match (model.bell, unread) {
                (_, 0) => "All read".to_owned(),
                (true, count) => format!("{count} new"),
                (false, count) => format!("{count} unopened"),
            },
            Message::Notifications,
        )
        .id("more-notifications"),
        hairline(),
        link_row(
            "Scenario settings",
            format!(
                "{} accent \u{b7} {} \u{b7} {} min",
                settings.accent.label(),
                settings.difficulty.label(),
                settings.length.minutes()
            ),
            Message::Settings,
        )
        .id("more-settings"),
        hairline(),
        link_row(
            "Premium",
            if model.premium {
                "Premium \u{b7} active (demo)"
            } else {
                "Free plan \u{b7} 3 intro calls"
            },
            Message::Premium,
        )
        .id("more-premium"),
        hairline(),
        link_row("Server", model.link.status(), Message::Server).id("more-server"),
        hairline(),
        Spacer::fixed(20.0),
        body(if model.link.is_online() {
            "Calls are live conversations with an AI caller. What you type or say \
             is sent to your CallReady server, which plays the caller and scores \
             the call. Premium here is a demo: no payment is taken."
        } else {
            "CallReady\u{2019}s practice calls are scripted and run on this phone. \
             Nothing is recorded or sent anywhere, and Premium here is a demo: \
             no payment is taken."
        }),
    ];
    tab_screen(Screen::More, page(content), Tab::More, Message::Tab)
}
