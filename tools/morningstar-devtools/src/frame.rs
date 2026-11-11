//! Screenshots at a chosen display scale and with safe-area insets.

use morningstar_gpu::{GpuError, render_offscreen_with_images};
use morningstar_runtime::{App, Frame, FrameInfo};

/// Renders one frame of `app` as a device described by `info` would show
/// it, and returns it as PNG bytes.
///
/// Unlike [`screenshot`](crate::screenshot), which always draws at a scale
/// of 1 with no insets, this takes the whole [`FrameInfo`]: surface size,
/// display scale, safe-area insets and the app clock. A typical phone is
///
/// ```
/// use morningstar_runtime::{FrameInfo, Insets, SurfaceSize};
/// let phone = FrameInfo::new(SurfaceSize::new(1080, 2400))
///     .with_scale(3.0)
///     .with_insets(Insets::new(0.0, 96.0, 0.0, 60.0));
/// # let _ = phone;
/// ```
///
/// Drive the app into the state to capture before calling this. An app
/// that hit-tests against the frame it last drew (every app built on the UI
/// layer does) can be drawn with this first and sent pointer events after.
pub fn screenshot_frame(app: &mut dyn App, info: &FrameInfo) -> Result<Vec<u8>, GpuError> {
    let mut frame = Frame::for_app(app);
    frame.begin();
    app.frame(info, &mut frame);

    let mut pixels = render_offscreen_with_images(
        info.size,
        frame.clear,
        &frame.list,
        frame.atlas(),
        frame.images(),
    )?;
    // The renderer works in linear light; image files are sRGB-encoded.
    let table = super::srgb_table();
    for pixel in pixels.chunks_exact_mut(4) {
        for channel in &mut pixel[..3] {
            *channel = table[usize::from(*channel)];
        }
    }
    Ok(super::png::encode(
        info.size.width,
        info.size.height,
        &pixels,
    ))
}
