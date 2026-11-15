//! Draws Barua Talk's launcher icon from the app's own art: the design's
//! send plane on a disc of its accent, as the Welcome screen draws it, on
//! the page colour. No device needed.
//!
//!   cargo run -p barua-talk --example icon
//!
//! writes `assets/icon.png`, the picture `Cargo.toml` names under
//! `[package.metadata.morningstar]` (ADR 0020): 1024 x 1024 and
//! full-bleed, the disc across 58% of the width, inside the circle a
//! round launcher mask keeps. There is no separate adaptive foreground:
//! the page colour runs to the edges and every mask cuts only that.
//!
//! The picture is committed: run this again only when the mark changes.
//! Look at the result at launcher sizes with
//!
//!   cargo run -q -p morningstar-devtools --bin morningstar-icon -- \
//!       barua-talk --preview out/icon-preview.png

use std::path::Path;

use barua_talk::screens::welcome::mark;
use barua_talk::theme;
use morningstar::ui::Ui;
use morningstar::ui::prelude::*;
use morningstar::{FrameInfo, SurfaceSize};
use morningstar_devtools::screenshot_frame;

/// The side of the picture, in pixels (and in dp: the scale is 1).
const SIDE: u32 = 1024;
/// The disc's share of the icon's width.
const DISC_SHARE: f32 = 0.58;

/// The mark in the middle of the page colour, under a glow of the accent.
struct Art;

impl Application for Art {
    type Message = ();

    fn theme(&self) -> Theme {
        theme::THEME
    }

    fn view(&self) -> Element<()> {
        let side = (SIDE as f32 * DISC_SHARE).round();
        Container::new(mark(side))
            .center()
            .fill()
            .background(theme::PAGE)
            .radial(
                Radial::circle()
                    .at(0.5, 0.5)
                    .stop(0.0, with_alpha(theme::ACCENT, 0.35))
                    .stop(0.62, with_alpha(theme::ACCENT, 0.0)),
            )
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
