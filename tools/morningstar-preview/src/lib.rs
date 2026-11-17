//! The development preview: a Morningstar app in a window on the Mac, live.
//!
//! ```sh
//! tools/preview.sh barua-talk
//! ```
//!
//! Two processes (ADR 0040):
//!
//! - **The supervisor** ([`supervisor`], the `morningstar-preview` binary)
//!   writes a tiny runner crate for the app under `target/preview/`, builds
//!   it, starts it, and watches the sources. When they change it builds
//!   again and, if the build passed, replaces the running app with the new
//!   one, in the same place on the screen. A build that fails leaves the old
//!   window up, with the compiler's errors in the terminal.
//! - **The host** ([`host`], linked into the runner beside the app) is a
//!   platform host like the Android and iOS ones (ADR 0003): it owns a
//!   window, drives the same [`Engine`](morningstar_runtime::Engine), turns
//!   the mouse into touches and the keyboard into text input, and carries
//!   out the platform services a Mac can ([`services`]).
//!
//! Nothing here is linked into an app built for a phone.

pub mod host;
pub mod http;
pub mod input;
pub mod options;
pub mod runner;
pub mod services;
pub mod store;
pub mod supervisor;
pub mod watch;

pub use morningstar_runtime::App;
