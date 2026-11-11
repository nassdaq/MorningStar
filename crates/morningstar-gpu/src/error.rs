use std::fmt;

/// Failures the GPU backend can report. Messages from the underlying graphics
/// API are carried as strings so no `wgpu` type leaks into the public API.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GpuError {
    /// The native drawable could not be turned into a GPU surface.
    SurfaceCreation(String),
    /// No GPU adapter on this device can draw to the surface.
    NoAdapter(String),
    /// The adapter refused to create a device.
    DeviceCreation(String),
    /// The surface reports no usable texture format or alpha mode.
    UnsupportedSurface,
    /// The graphics API rejected a frame; this indicates a Morningstar bug.
    Validation,
    /// Reading back an offscreen frame failed.
    Readback(String),
}

impl fmt::Display for GpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SurfaceCreation(e) => write!(f, "could not create GPU surface: {e}"),
            Self::NoAdapter(e) => write!(f, "no compatible GPU adapter: {e}"),
            Self::DeviceCreation(e) => write!(f, "could not create GPU device: {e}"),
            Self::UnsupportedSurface => f.write_str("surface reports no usable format"),
            Self::Validation => {
                f.write_str("graphics API validation error while acquiring a frame")
            }
            Self::Readback(e) => write!(f, "could not read back offscreen frame: {e}"),
        }
    }
}

impl std::error::Error for GpuError {}
