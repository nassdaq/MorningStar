//! One module per screen.
//!
//! Every screen is a function from the application's state to an
//! [`Element`](morningstar::ui::Element) of [`Message`](crate::Message)s:
//! it changes nothing itself. The first eight are the design's pages (09
//! and 10 are `home` and `chat` again, see [`Flow`](crate::Flow)); the last
//! three are the app's own, in the same visual language.
//!
//! Each quotes the CSS it was written from. Fixed bars stay put and the
//! rest scrolls, so a screen is usable on a phone shorter than the
//! design's; a screen whose design has a stretching middle (`flex-grow: 1`)
//! gives its scrolling content the height of its viewport as a minimum, so
//! that at the design's size it is the design's layout exactly.

pub mod chat;
pub mod explore;
pub mod home;
pub mod live_voice;
pub mod model_picker;
pub mod onboarding;
pub mod plans;
pub mod sign_in;

pub mod library;
pub mod models;
pub mod server;
