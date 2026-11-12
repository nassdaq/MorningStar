//! Draws FundiDesk's launcher icon from the app's own art: the logo of the
//! splash screen on the brand green. No device needed.
//!
//!   cargo run -p fundidesk --example icon
//!
//! writes `assets/icon.png`, 1024 x 1024 and full-bleed, which `Cargo.toml`
//! names under `[package.metadata.morningstar]` (ADR 0020). It is the only
//! picture: Android's adaptive icon is made from it by the single-icon
//! rule, with the green of its corner as the background, so the logo's
//! plate (six tenths of the width) is inside what any launcher's mask keeps.
//!
//! The picture is committed: run this again only when the logo changes.
//! Look at the result at launcher sizes with
//!
//!   cargo run -q -p morningstar-devtools --bin morningstar-icon -- \
//!       fundidesk --preview out/icon-preview.png

use std::path::Path;

use fundidesk::components::logo;
use fundidesk::theme::{self, GREEN, WHITE};
use morningstar::ui::Ui;
use morningstar::ui::prelude::*;
use morningstar::{FrameInfo, SurfaceSize};
use morningstar_devtools::screenshot_frame;

/// The side of the picture, in pixels (and in dp: the scale is 1).
const SIDE: u32 = 1024;
/// The logo plate's share of the icon's width.
const LOGO_SHARE: f32 = 0.6;

/// The splash screen's logo, alone, on the brand green.
struct Art;

impl Application for Art {
    type Message = ();

    fn theme(&self) -> Theme {
        theme::THEME
    }

    fn view(&self) -> Element<()> {
        // The mark's usual stroke of 6 rather than the splash screen's 5:
        // at 48 pixels every bit of weight counts.
        col![logo(SIDE as f32 * LOGO_SHARE, WHITE, GREEN)]
            .center()
            .background(GREEN)
            .into()
    }

    fn update(&mut self, (): ()) {}
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/icon.png");
    let info = FrameInfo::new(SurfaceSize::new(SIDE, SIDE));
    std::fs::write(&path, screenshot_frame(&mut Ui::new(Art), &info)?)?;
    println!("{}", path.display());
    Ok(())
}
