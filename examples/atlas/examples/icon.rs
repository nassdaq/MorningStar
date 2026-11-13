//! Draws Atlas's launcher icon from the app's own art: the orb, as the
//! screens draw it, on the page colour under onboarding's wash. No device
//! needed.
//!
//!   cargo run -p atlas --example icon
//!
//! writes `assets/icon.png`, the picture `Cargo.toml` names under
//! `[package.metadata.morningstar]` (ADR 0020): 1024 x 1024 and
//! full-bleed, the orb across 60% of the width, a little above the middle
//! so that it and the glow under it sit together inside the circle a round
//! launcher mask keeps. There is no separate adaptive foreground: the
//! page colour runs to the edges and every mask cuts only that.
//!
//! The picture is committed: run this again only when the orb changes.
//! Look at the result at launcher sizes with
//!
//!   cargo run -q -p morningstar-devtools --bin morningstar-icon -- \
//!       atlas --preview out/icon-preview.png

use std::path::Path;

use atlas::components::{orb, wash};
use atlas::theme;
use morningstar::ui::Ui;
use morningstar::ui::prelude::*;
use morningstar::{FrameInfo, SurfaceSize};
use morningstar_devtools::screenshot_frame;

/// The side of the picture, in pixels (and in dp: the scale is 1).
const SIDE: u32 = 1024;
/// The orb's share of the icon's width.
const ORB_SHARE: f32 = 0.60;

/// The orb in the middle of the page colour and its wash.
struct Art;

impl Application for Art {
    type Message = ();

    fn theme(&self) -> Theme {
        theme::THEME
    }

    fn view(&self) -> Element<()> {
        let side = (SIDE as f32 * ORB_SHARE).round();
        Container::new(orb(side).offset(0.0, -side * 0.04))
            .center()
            .fill()
            .background(theme::PAGE)
            .radial(wash(0.5, 0.5, 0.62))
            .into()
    }

    fn update(&mut self, (): ()) {}
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    let info = FrameInfo::new(SurfaceSize::new(SIDE, SIDE));
    let icon = screenshot_frame(&mut Ui::new(Art), &info)?;
    std::fs::write(assets.join("icon.png"), &icon)?;
    println!("{}", assets.join("icon.png").display());
    Ok(())
}
