//! Foundation types shared by every Morningstar subsystem.
//!
//! This crate has no dependencies and knows nothing about GPUs or operating
//! systems, so everything in it is testable on any host.

mod color;
mod display_list;
mod frame_stats;
mod geometry;
mod image;
mod input;
mod insets;
mod lifecycle;

pub use color::Color;
pub use display_list::{
    AtlasView, Border, Dash, DirtyRows, DisplayList, DrawOp, LinearGradient, Shadow,
};
pub use frame_stats::{FrameStats, FrameSummary};
pub use geometry::{Point, Rect, Size};
pub use image::{
    Image, ImageError, ImageFit, ImageId, ImagePixels, ImagePlacement, ImageStore, ImagesView,
    MAX_IMAGE_BYTES, MAX_IMAGE_DIMENSION,
};
pub use input::{
    Capitalization, ImeAction, KeyboardType, PointerEvent, PointerPhase, TextEditState,
    TextInputCommand, TextInputEvent, TextInputRequest, TextInputTraits, byte_to_utf16_offset,
    utf16_to_byte_offset,
};
pub use insets::Insets;
pub use lifecycle::{Lifecycle, LifecycleEvent, SurfaceSize};
