//! Renders FundiDesk's screens to PNG files on this machine's GPU, and
//! compares them with the design. No device needed.
//!
//!   cargo run -p fundidesk --example screenshot -- <output-dir> [screen ...] [--diff] [--phone]
//!       [--semantics]
//!
//! With no screen named, every screen is rendered; name them as the design
//! does (`07-dashboard`) or by number (`07`). For each screen it writes
//! `<name>.png`: 780 px wide at 2 px per dp and as tall as the design's
//! artboard, so tall screens are rendered at their full content height and
//! can be compared whole.
//!
//! `--phone` also writes `<name>.phone.png`: the same screen in a
//! 390 x 844 dp phone viewport, where tall screens scroll.
//!
//! `--semantics` also writes `<name>.semantics.png`: the phone viewport
//! as a screen reader finds it, every node outlined and numbered in
//! reading order beside a legend (ADR 0032), and prints what
//! `Ui::accessibility_issues` finds.
//!
//! `--diff` compares each render with `design/reference/<name>.png`,
//! leaving out the four corners the design tool rounded off, prints the
//! report and writes `<name>.diff.png` (heat map) and `<name>.sbs.png`
//! (reference, render and heat map side by side).

use std::path::{Path, PathBuf};

use fundidesk::{DESIGN_HEIGHT, DESIGN_WIDTH, FundiDesk, Screen, corner_masks};
use morningstar::ui::Ui;
use morningstar::{FrameInfo, SurfaceSize};
use morningstar_devtools::{
    CompareOptions, Image, Rect, compare, screenshot_frame, screenshot_semantics,
};

/// Pixels per dp: the reference images are 2x.
const SCALE: f32 = 2.0;

fn frame(width: f32, height: f32) -> FrameInfo {
    let size = SurfaceSize::new((width * SCALE) as u32, (height * SCALE) as u32);
    FrameInfo::new(size).with_scale(SCALE)
}

fn render(screen: Screen, info: &FrameInfo) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut ui = Ui::new(FundiDesk::on(screen));
    let png = screenshot_frame(&mut ui, info)?;
    let overflows = ui.overflows();
    if !overflows.is_empty() {
        println!("  layout overflows: {overflows:?}");
    }
    Ok(png)
}

fn diff(screen: Screen, png: &[u8], directory: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let reference = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("design/reference")
        .join(format!("{}.png", screen.name()));
    let reference = Image::load(&reference)?;
    let candidate = Image::from_png(png)?;
    let ignore = corner_masks(candidate.width(), candidate.height(), SCALE)
        .map(|(x, y, width, height)| Rect::new(x, y, width, height))
        .to_vec();
    let comparison = compare(
        &reference,
        &candidate,
        &CompareOptions {
            ignore,
            ..CompareOptions::default()
        },
    );
    println!("{}", comparison.report);
    comparison
        .heat_map()
        .save(directory.join(format!("{}.diff.png", screen.name())))?;
    comparison
        .side_by_side()
        .save(directory.join(format!("{}.sbs.png", screen.name())))?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut directory = None;
    let mut screens = Vec::new();
    let (mut with_diff, mut with_phone, mut with_semantics) = (false, false, false);
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--diff" => with_diff = true,
            "--phone" => with_phone = true,
            "--semantics" => with_semantics = true,
            _ if directory.is_none() => directory = Some(PathBuf::from(argument)),
            name => {
                let screen = Screen::ALL
                    .into_iter()
                    .find(|screen| screen.name() == name || screen.name().starts_with(name))
                    .ok_or_else(|| format!("no screen called {name}"))?;
                screens.push(screen);
            }
        }
    }
    let directory = directory.unwrap_or_else(|| PathBuf::from("."));
    std::fs::create_dir_all(&directory)?;
    if screens.is_empty() {
        screens.extend(Screen::ALL);
    }

    for screen in screens {
        let path = directory.join(format!("{}.png", screen.name()));
        let png = render(screen, &frame(DESIGN_WIDTH, screen.design_height()))?;
        std::fs::write(&path, &png)?;
        println!("{}", path.display());
        if with_phone {
            let path = directory.join(format!("{}.phone.png", screen.name()));
            std::fs::write(&path, render(screen, &frame(DESIGN_WIDTH, DESIGN_HEIGHT))?)?;
            println!("{}", path.display());
        }
        if with_semantics {
            // The phone viewport, as a screen reader finds it.
            let path = directory.join(format!("{}.semantics.png", screen.name()));
            let mut ui = Ui::new(FundiDesk::on(screen));
            let (sheet, tree) = screenshot_semantics(&mut ui, &frame(DESIGN_WIDTH, DESIGN_HEIGHT))?;
            sheet.save(&path)?;
            println!(
                "{} ({} nodes)",
                path.display(),
                tree.len().saturating_sub(1)
            );
            for issue in ui.accessibility_issues() {
                println!("  {issue}");
            }
        }
        if with_diff {
            if screen.is_built() {
                diff(screen, &png, &directory)?;
            } else {
                println!("  not built yet: no comparison");
            }
        }
    }
    Ok(())
}
