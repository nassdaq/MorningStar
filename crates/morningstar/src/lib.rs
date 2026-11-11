//! Morningstar: a Rust-native, GPU-first mobile application framework.
//!
//! This is the only crate an application depends on. It re-exports the
//! public API and links the host integration for the platform being built.
//!
//! There are two levels:
//!
//! - **[`ui`], the declarative layer, which is what applications should
//!   use.** A state struct implements [`ui::Application`]: `view` describes
//!   the screen as a tree of widgets (columns, rows, text, buttons) in dp,
//!   and `update` handles the messages the widgets send. Start with
//!   `use morningstar::ui::prelude::*;` and
//!   `morningstar::app!(Ui::new(MyApp::default()));`. See
//!   `docs/building-screens.md`.
//! - **[`App`], the low-level interface underneath it**: paint a display
//!   list by hand and receive raw touch events. The UI layer is an `App`;
//!   use this level directly only for custom rendering.
//!
//! Three things every low-level app should know (ADR 0012); the UI layer
//! takes care of all three:
//!
//! - **Units.** Everything is drawn in physical pixels. Design sizes are in
//!   dp: convert with [`FrameInfo::dp`] or [`FrameInfo::scale`].
//! - **Safe area.** The surface runs edge to edge under the status bar,
//!   cutout and gesture bar. Keep content inside [`FrameInfo::safe_area`].
//! - **Frames are on demand.** [`App::frame`] runs when something changed,
//!   not every refresh. Call [`Frame::request_redraw`] while animating.

pub use morningstar_runtime::{
    App, AtlasView, Capitalization, Color, Dash, DisplayList, DrawOp, FontFamily, Frame, FrameInfo,
    IconError, Image, ImageError, ImageFit, ImageId, ImagePlacement, ImagesView, ImeAction, Insets,
    KeyboardType, Point, PointerEvent, PointerPhase, Rect, Setup, Size, SurfaceSize, TextAlign,
    TextEditState, TextInputCommand, TextInputEvent, TextInputRequest, TextInputTraits, TextSpan,
    TextStyle, TextWeight, VectorIcon, app,
};

/// The declarative UI layer: widgets, layout, state and events.
///
/// ```
/// use morningstar::ui::prelude::*;
///
/// struct Hello;
///
/// impl Application for Hello {
///     type Message = ();
///
///     fn view(&self) -> Element<()> {
///         col![text("Habari, dunia").title()].center().into()
///     }
///
///     fn update(&mut self, _message: ()) {}
/// }
///
/// // morningstar::app!(Ui::new(Hello));
/// # let _ = Ui::new(Hello);
/// ```
pub use morningstar_ui as ui;

/// Everything a low-level [`App`] typically needs. Applications written
/// with the UI layer use [`ui::prelude`] instead.
pub mod prelude {
    pub use crate::{
        App, Color, DisplayList, Frame, FrameInfo, Image, ImageError, ImageFit, Insets, Point,
        PointerEvent, PointerPhase, Rect, Setup, Size, SurfaceSize, TextAlign, TextStyle,
        TextWeight,
    };
}

// Referencing the platform crate is what links its exported entry points
// (JNI functions on Android, the C ABI on iOS) into the application library.
#[cfg(target_os = "android")]
#[doc(hidden)]
pub use morningstar_android as __platform;
#[cfg(target_os = "ios")]
#[doc(hidden)]
pub use morningstar_ios as __platform;
