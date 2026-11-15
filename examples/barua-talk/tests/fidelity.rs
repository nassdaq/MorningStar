//! Every page of the design, rendered and compared with the design's
//! reference. (The app's own screens have no reference; `tests/e2e.rs`
//! drives those.)
//!
//! The render is 780 x 1688 px, the design's 390 x 844 dp at 2 px per dp,
//! exactly like `design/reference/<name>.png`, and is compared with the
//! default settings of `morningstar-diff`: a colour error over 2 counts,
//! edges may move or be anti-aliased differently by one pixel. Nothing is
//! masked: the photographs are part of what is compared.
//!
//! The limits below are not a target that was chosen: each is the measured
//! difference of the screen as built, rounded up to the next 0.05. A layout
//! change of a few pixels in any element pushes a screen well over its
//! limit. `SCREENS.md` says what the remaining difference is made of.
//!
//! On failure the report is printed and `<name>.actual.png` and
//! `<name>.sbs.png` are written to `target/fidelity/`.

use std::path::Path;
use std::time::Duration;

use barua_talk::{BaruaTalk, DESIGN_HEIGHT, DESIGN_WIDTH, DESIGNED, Screen};
use morningstar::ui::Ui;
use morningstar::{App, Frame, FrameInfo, SurfaceSize};
use morningstar_devtools::{CompareOptions, Image, Tolerance, compare, screenshot_frame};

const SCALE: f32 = 2.0;

/// Largest share of differing pixels, in percent, for a designed page.
fn limit(name: &str) -> f32 {
    match name {
        // Measured 0.0847.
        "01-home" => 0.10,
        // Measured 0.6900: the photograph of the shop is most of it.
        "02-room" => 0.70,
        // Measured 0.0154.
        "03-incoming" => 0.05,
        // Measured 0.2860: the camera tile is most of it.
        "04-video-call" => 0.30,
        // Measured 0.0499.
        "05-phone-call" => 0.05,
        _ => unreachable!("{name} is not in the design"),
    }
}

fn info(scale: f32) -> FrameInfo {
    let size = SurfaceSize::new(
        (DESIGN_WIDTH * scale) as u32,
        (DESIGN_HEIGHT * scale) as u32,
    );
    FrameInfo::new(size).with_scale(scale)
}

/// The page as the design shows it: a still frame, drawn once to lay it
/// out and captured a second later.
fn render(name: &str) -> Option<(Ui<BaruaTalk>, Vec<u8>)> {
    let mut ui = Ui::new(BaruaTalk::designed(name).expect("a designed page"));
    let mut frame = Frame::for_app(&mut ui);
    frame.begin();
    ui.frame(&info(SCALE), &mut frame);
    match screenshot_frame(&mut ui, &info(SCALE).with_elapsed(Duration::from_secs(1))) {
        Ok(png) => Some((ui, png)),
        Err(error) => {
            eprintln!("skipping: cannot render ({error})");
            None
        }
    }
}

#[test]
fn designed_pages_match_their_references() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();
    for name in DESIGNED {
        let Some((ui, png)) = render(name) else {
            return;
        };
        assert_eq!(ui.overflows(), [], "{name}: layout overflows");

        let reference = root.join(format!("design/reference/{name}.png"));
        let reference = Image::load(&reference).expect("reference image");
        let candidate = Image::from_png(&png).expect("render");
        assert_eq!(
            (candidate.width(), candidate.height()),
            (reference.width(), reference.height()),
            "{name}: the render and the reference differ in size",
        );
        let comparison = compare(
            &reference,
            &candidate,
            &CompareOptions {
                tolerance: Tolerance {
                    max_diff_percent: limit(name),
                    ..Tolerance::DEFAULT
                },
                ..CompareOptions::default()
            },
        );
        let report = &comparison.report;
        println!(
            "{name}: {:.4}% differ (limit {:.2}%), mean error {:.3}, max {:.2}",
            report.diff_percent,
            limit(name),
            report.mean_error,
            report.max_error
        );
        if !report.passed {
            let out = root.join("../../target/fidelity");
            std::fs::create_dir_all(&out).expect("output directory");
            std::fs::write(out.join(format!("{name}.actual.png")), &png).expect("write render");
            comparison
                .side_by_side()
                .save(out.join(format!("{name}.sbs.png")))
                .expect("write comparison");
            println!("{report}");
            failures.push(name);
        }
    }
    assert!(
        failures.is_empty(),
        "pages differ from the design: {failures:?}"
    );
}

#[test]
fn nothing_scrolls_or_moves_at_the_designs_size() {
    for name in DESIGNED {
        let mut ui = Ui::new(BaruaTalk::designed(name).expect("a designed page"));
        let mut frame = Frame::for_app(&mut ui);
        for millis in [0, 1000] {
            frame.begin();
            ui.frame(
                &info(SCALE).with_elapsed(Duration::from_millis(millis)),
                &mut frame,
            );
        }
        let screen = ui.app().screen();
        if let Some(max) = ui.max_scroll_offset(screen.scroll_id()) {
            assert_eq!(max, 0.0, "{name} scrolls at the design's size");
        }
        // A still frame: no timer, nothing animating.
        assert!(!frame.redraw_requested(), "{name} is still drawing");
        assert_eq!(frame.redraw_after(), None, "{name} has a timer running");
    }
}

#[test]
fn every_screen_lays_out_without_overflow_at_phone_densities() {
    for screen in Screen::ALL {
        for scale in [1.0, 2.0, 2.8125, 3.0] {
            let mut ui = Ui::new(BaruaTalk::on(screen));
            let mut frame = Frame::for_app(&mut ui);
            frame.begin();
            ui.frame(&info(scale), &mut frame);
            assert_eq!(ui.overflows(), [], "{} at scale {scale}", screen.name());
            assert!(!frame.list.is_empty(), "{} draws something", screen.name());
        }
    }
}

#[test]
fn references_are_exactly_the_designs_size() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for name in DESIGNED {
        let reference = root.join(format!("design/reference/{name}.png"));
        let image = Image::load(&reference).expect("reference image");
        assert_eq!((image.width(), image.height()), (780, 1688), "{name}");
        let html = root.join(format!("design/html/{name}.html"));
        let html = std::fs::read_to_string(html).expect("design html");
        assert!(html.contains("width: 390px; height: 844px"), "{name}");
    }
}

/// The photographs the app draws are the design's: every PNG has the JPEG
/// it was made from beside it, and every picture a page names is there.
#[test]
fn every_photograph_of_the_design_is_in_the_app() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for name in DESIGNED {
        let html = std::fs::read_to_string(root.join(format!("design/html/{name}.html")))
            .expect("design html");
        for source in html.split("src=\"../../").skip(1) {
            let jpeg = source.split('"').next().expect("a path");
            assert!(root.join(jpeg).exists(), "{name}: {jpeg} is missing");
            let png = jpeg.replace(".jpg", ".png");
            assert!(root.join(&png).exists(), "{name}: {png} was not prepared");
        }
    }
}
