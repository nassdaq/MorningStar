//! One module per screen of the design.
//!
//! Every screen module has the same shape, so the application can host
//! them all the same way and each can be written without touching another
//! file:
//!
//! ```ignore
//! pub const BUILT: bool;                       // false while a placeholder
//! pub struct State { .. }                      // Default
//! pub enum Message { .. }                      // Clone, Debug, PartialEq
//! pub fn view(state: &State) -> Element<Message>;
//! pub fn update(state: &mut State, message: Message) -> Nav;
//! ```

pub mod create_account;
pub mod dashboard;
pub mod login;
pub mod new_repair;
pub mod onboarding;
pub mod record_payment;
pub mod repair_details;
pub mod repairs_list;
pub mod shop_setup;
pub mod splash;
pub mod tracking;
pub mod verification;
