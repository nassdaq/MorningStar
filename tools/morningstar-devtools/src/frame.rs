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

/// Renders one frame of `app` over nothing instead of over its background
/// colour, and returns it as an image with straight alpha: whatever the
/// app does not draw is transparent, and an anti-aliased edge keeps its
/// partial coverage.
///
/// For producing artwork from an app's own drawing, such as the foreground
/// of its launcher icon (ADR 0020). The app's background colour is the
/// frame's clear colour, which this replaces; an app that paints a
/// background of its own covers the whole picture as usual.
pub fn screenshot_transparent(
    app: &mut dyn App,
    info: &FrameInfo,
) -> Result<crate::Image, GpuError> {
    let mut frame = Frame::for_app(app);
    frame.begin();
    app.frame(info, &mut frame);
    let mut clear = frame.clear;
    (clear.r, clear.g, clear.b, clear.a) = (0.0, 0.0, 0.0, 0.0);

    let mut pixels =
        render_offscreen_with_images(info.size, clear, &frame.list, frame.atlas(), frame.images())?;
    // The renderer's pixels are linear light multiplied by their alpha; an
    // image file holds sRGB-encoded colour beside its alpha.
    for pixel in pixels.chunks_exact_mut(4) {
        let alpha = f32::from(pixel[3]);
        for channel in &mut pixel[..3] {
            *channel = if alpha > 0.0 {
                crate::image::linear_to_srgb(f32::from(*channel) / alpha)
            } else {
                0
            };
        }
    }
    Ok(crate::Image::new(info.size.width, info.size.height, pixels))
}

#[cfg(test)]
mod tests {
    use morningstar_runtime::{Color, Rect, SurfaceSize};

    use super::*;

    /// An opaque square and a half-transparent white one, on a red
    /// background the transparent screenshot must not show.
    struct Squares;

    impl App for Squares {
        fn frame(&mut self, _info: &FrameInfo, frame: &mut Frame) {
            frame.clear = Color::rgb(1.0, 0.0, 0.0);
            frame
                .list
                .fill_rect(Rect::new(0.0, 0.0, 4.0, 4.0), Color::rgb(0.0, 0.0, 1.0));
            frame.list.fill_rect(
                Rect::new(4.0, 0.0, 4.0, 4.0),
                Color::rgba(1.0, 1.0, 1.0, 0.5),
            );
        }
    }

    #[test]
    fn a_transparent_screenshot_keeps_alpha_and_drops_the_background() {
        let info = FrameInfo::new(SurfaceSize::new(8, 8));
        let image = screenshot_transparent(&mut Squares, &info).unwrap();
        assert_eq!((image.width(), image.height()), (8, 8));
        assert_eq!(image.pixel(1, 1), [0, 0, 255, 255]);
        // Straight alpha: the colour is still white, not half of it.
        let [r, g, b, a] = image.pixel(5, 1);
        assert!(r >= 254 && g >= 254 && b >= 254, "{r} {g} {b}");
        assert!((127..=128).contains(&a), "{a}");
        // Nothing drawn: nothing there, and none of the red.
        assert_eq!(image.pixel(1, 6), [0, 0, 0, 0]);
        assert_eq!(image.pixel(6, 6), [0, 0, 0, 0]);

        // The ordinary screenshot of the same app does show its background.
        let opaque = crate::Image::from_png(&screenshot_frame(&mut Squares, &info).unwrap());
        assert_eq!(opaque.unwrap().pixel(1, 6), [255, 0, 0, 255]);
    }
}
