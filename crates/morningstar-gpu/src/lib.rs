//! Morningstar's GPU backend.
//!
//! This crate is the only place that talks to `wgpu`. Nothing in its public
//! API exposes a `wgpu` type, so the graphics abstraction can be changed
//! without touching the runtime, the platform hosts or application code.
//!
//! Scope today: own a device, attach to and detach from a native drawable,
//! and draw a display list as instanced quads: rounded rectangles (solid,
//! bordered or gradient-filled), box shadows, glyphs and images, with
//! per-instance clipping. One draw call, plus one for each change of image.
//! See ADR 0011 and ADR 0014.

mod error;
mod images;
mod offscreen;
mod quads;
mod renderer;

pub use error::GpuError;
pub use offscreen::{OffscreenRenderer, render_offscreen, render_offscreen_with_images};
pub use quads::{AtlasUpload, RenderStats};
pub use renderer::{AdapterInfo, FrameOutcome, FrameTimings, NativeSurface, Renderer};
