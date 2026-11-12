//! Development tooling for Morningstar apps.
//!
//! The central tool is [`screenshot`]: it runs one frame of an app through
//! the real renderer on the development machine's GPU and returns a PNG. No
//! phone, emulator or window is involved.
//!
//! This is what makes a design-to-code loop possible. A developer, or an AI
//! agent given a picture of a design, can write UI code, render it, look at
//! the result next to the design and correct it, in seconds.
//!
//! The other half of that loop is [`compare`]: it measures how far a
//! screenshot is from a reference image and reports where, as a [`Report`]
//! a program can act on, a heat map and a side-by-side picture. The
//! `morningstar-diff` binary in this crate is the same thing on the command
//! line, and [`assert_matches_reference`] turns it into a visual regression
//! test. `docs/design-to-code.md` walks through the whole loop.
//!
//! Separately, [`Identity`] and [`IconSet`] turn what an app declares in
//! its `Cargo.toml` (a name, an id, an icon) into the launcher icons and
//! names the Android and iOS builds need; the `morningstar-icon` binary is
//! what the build scripts run (ADR 0020).

mod diff;
mod frame;
mod icon;
mod identity;
mod image;
mod json;
mod png;
mod reference;

use std::time::Duration;

use morningstar_gpu::{GpuError, render_offscreen_with_images};
use morningstar_runtime::{App, Frame, FrameInfo, SurfaceSize};

pub use diff::{
    AspectMismatch, CompareOptions, Comparison, Fit, Region, Report, Tolerance, compare,
};
pub use frame::{screenshot_frame, screenshot_transparent};
pub use icon::{Generated, IconSet, Mask, Written, write_generated};
pub use identity::{IconError, Identity, cargo_metadata};
pub use image::{Image, ImageError, Rect};
pub use png::encode as encode_png;
pub use reference::{
    ReferenceError, ReferenceOutcome, UPDATE_ENV, assert_matches_reference, check_reference,
};

/// Renders one frame of `app` at `size` and returns it as PNG bytes.
///
/// `elapsed` is the app clock for the frame, so an animation can be captured
/// at any chosen moment. Drive the app into the state to capture (for example
/// by sending pointer events) before calling this.
pub fn screenshot(
    app: &mut dyn App,
    size: SurfaceSize,
    elapsed: Duration,
) -> Result<Vec<u8>, GpuError> {
    let mut frame = Frame::for_app(app);
    frame.begin();
    app.frame(&FrameInfo::new(size).with_elapsed(elapsed), &mut frame);

    let mut pixels = render_offscreen_with_images(
        size,
        frame.clear,
        &frame.list,
        frame.atlas(),
        frame.images(),
    )?;
    // The renderer works in linear light; image files are sRGB-encoded.
    // Without this the picture would look much darker than on a device.
    let table = srgb_table();
    for pixel in pixels.chunks_exact_mut(4) {
        for channel in &mut pixel[..3] {
            *channel = table[usize::from(*channel)];
        }
    }
    Ok(png::encode(size.width, size.height, &pixels))
}

/// Linear-to-sRGB transfer function for every 8-bit value.
fn srgb_table() -> [u8; 256] {
    std::array::from_fn(|value| {
        let linear = value as f32 / 255.0;
        let encoded = if linear <= 0.003_130_8 {
            linear * 12.92
        } else {
            1.055 * linear.powf(1.0 / 2.4) - 0.055
        };
        (encoded * 255.0).round() as u8
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_table_endpoints_and_midpoint() {
        let table = srgb_table();
        assert_eq!(table[0], 0);
        assert_eq!(table[255], 255);
        // Linear 50% grey is sRGB 188.
        assert_eq!(table[128], 188);
        assert!(table.windows(2).all(|pair| pair[0] <= pair[1]), "monotonic");
    }
}
