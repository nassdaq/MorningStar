//! Morningstar's application runtime.
//!
//! The [`Engine`] is the single object a platform host drives. Hosts
//! translate native callbacks into its methods and call [`Engine::frame`]
//! at the display's pace while [`Engine::needs_frame`] is true; everything
//! else is shared across platforms.

mod app;
mod engine;
mod image;

pub use app::{App, Frame, FrameInfo, Setup};
pub use engine::{Engine, EngineReport};
pub use morningstar_core::{
    AtlasView, Color, Dash, DisplayList, DrawOp, FrameSummary, Insets, Point, PointerEvent,
    PointerPhase, Rect, Size, SurfaceSize, Transform,
};
pub use morningstar_core::{
    AudioClip, AudioError, DEFAULT_HTTP_TIMEOUT, HostState, HttpError, HttpMethod, HttpRequest,
    HttpResponse, MAX_HTTP_RESPONSE_BYTES, MAX_RECORDING_DURATION, NO_INSTALL_ID, Recording,
    ServiceCall, ServiceId, ServiceRequest, ServiceResult,
};
pub use morningstar_core::{
    Capitalization, ImeAction, KeyboardType, TextEditState, TextInputCommand, TextInputEvent,
    TextInputRequest, TextInputTraits, byte_to_utf16_offset, utf16_to_byte_offset,
};
pub use morningstar_core::{Image, ImageError, ImageFit, ImageId, ImagePlacement, ImagesView};
pub use morningstar_gpu::{AdapterInfo, FrameOutcome, GpuError, NativeSurface};
pub use morningstar_text::{
    FontFamily, IconError, Layer, LineCap, LineJoin, Shape, TextAlign, TextSpan, TextStyle,
    TextWeight, VectorIcon, VectorPicture,
};

/// Registers the application type for the platform host to start.
///
/// Takes an expression that constructs a value implementing [`App`]. Use it
/// exactly once, in the crate that is built as the app's native library.
///
/// ```ignore
/// morningstar::app!(MyApp::default());
/// ```
#[macro_export]
macro_rules! app {
    ($constructor:expr) => {
        #[doc(hidden)]
        #[unsafe(no_mangle)]
        pub fn __morningstar_create_app() -> ::std::boxed::Box<dyn $crate::App> {
            ::std::boxed::Box::new($constructor)
        }
    };
}
