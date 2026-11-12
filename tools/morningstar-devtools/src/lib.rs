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

    // The renderer composites sRGB-encoded values (ADR 0023): its pixels
    // are an image file's, as they are.
    let pixels = render_offscreen_with_images(
        size,
        frame.clear,
        &frame.list,
        frame.atlas(),
        frame.images(),
    )?;
    Ok(png::encode(size.width, size.height, &pixels))
}
