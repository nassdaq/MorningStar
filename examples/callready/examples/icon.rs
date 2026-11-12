//! Draws CallReady's launcher icon from the app's own art: the vinyl disc
//! of the home screen on the app's black. No device needed.
//!
//!   cargo run -p callready --example icon
//!
//! writes, into `assets/`, the two pictures `Cargo.toml` names under
//! `[package.metadata.morningstar]` (ADR 0020):
//!
//! * `icon.png`, 1024 x 1024 and full-bleed: the disc across 80% of the
//!   width, on black;
//! * `icon-foreground.png`, 1024 x 1024 and transparent: the disc alone
//!   for Android's adaptive icon, two thirds the size, so that in the
//!   72 of 108 dp a launcher shows it sits exactly as in `icon.png` and
//!   stays inside the 66 dp circle every mask keeps.
//!
//! Both are committed: run this again only when the disc changes. Look at
//! the result at launcher sizes with
//!
//!   cargo run -q -p morningstar-devtools --bin morningstar-icon -- \
//!       callready --preview out/icon-preview.png

use std::path::Path;

use callready::screens::home::DISC;
use callready::theme;
use morningstar::ui::Ui;
use morningstar::ui::prelude::*;
use morningstar::{FrameInfo, SurfaceSize};
use morningstar_devtools::{Image, screenshot_frame, screenshot_transparent};

/// The side of both pictures, in pixels (and in dp: the scale is 1).
const SIDE: u32 = 1024;
/// The disc's share of the full icon's width.
const DISC_SHARE: f32 = 0.80;

/// The disc, `side` across, in the middle of the app's background.
struct Art {
    side: f32,
}

impl Application for Art {
    type Message = ();

    fn theme(&self) -> Theme {
        theme::THEME
    }

    fn view(&self) -> Element<()> {
        col![Picture::svg(DISC.get()).size(self.side, self.side)]
            .center()
            .into()
    }

    fn update(&mut self, (): ()) {}
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    let info = FrameInfo::new(SurfaceSize::new(SIDE, SIDE));
    let side = SIDE as f32 * DISC_SHARE;

    let icon = screenshot_frame(&mut Ui::new(Art { side }), &info)?;
    std::fs::write(assets.join("icon.png"), &icon)?;
    println!("{}", assets.join("icon.png").display());

    let foreground: Image = screenshot_transparent(
        &mut Ui::new(Art {
            side: side * 72.0 / 108.0,
        }),
        &info,
    )?;
    foreground.save(assets.join("icon-foreground.png"))?;
    println!("{}", assets.join("icon-foreground.png").display());
    Ok(())
}
