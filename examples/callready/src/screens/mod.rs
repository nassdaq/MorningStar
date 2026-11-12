//! One module per screen.
//!
//! Every screen module has the same shape, so each can be written without
//! touching another:
//!
//! ```ignore
//! pub enum Message { .. }                          // Clone, Debug, PartialEq
//! pub fn view(what it shows) -> Element<Message>;  // from the model, read-only
//! ```
//!
//! A screen never changes anything: its messages say what was tapped, and
//! `lib.rs` decides what that does to the [`Model`](crate::model::Model)
//! and where it leads. Every screen is a scroll view between its fixed
//! bars, so it is usable on a phone shorter than the design's.
//!
//! The first five are reproduced from the design and compared with its
//! pictures; the rest are the app's own.

pub mod call_report;
pub mod home;
pub mod live_call;
pub mod premium;
pub mod welcome;

pub mod history;
pub mod more;
pub mod notifications;
pub mod profile;
pub mod progress;
pub mod server;
pub mod settings;
pub mod transcript;
