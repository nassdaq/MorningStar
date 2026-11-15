//! One module per screen.
//!
//! Every screen is a function from the application's state to an
//! [`Element`](morningstar::ui::Element) of [`Message`](crate::Message)s:
//! it changes nothing itself. The first five are the design's pages; the
//! other six are the app's own, in the same visual language.
//!
//! Each quotes the CSS it was written from. Fixed bars stay put and the
//! rest scrolls, so a screen is usable on a phone shorter than the
//! design's; a screen whose design has a stretching middle (`flex-grow: 1`)
//! gives its scrolling content the height of its viewport as a minimum, so
//! that at the design's size it is the design's layout exactly.

pub mod home;
pub mod incoming;
pub mod phone_call;
pub mod room;
pub mod video_call;

pub mod dial;
pub mod new_room;
pub mod notifications;
pub mod people;
pub mod server;
pub mod welcome;
