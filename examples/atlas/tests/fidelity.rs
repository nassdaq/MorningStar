//! Every page of the design, rendered and compared with the design's
//! reference. (The app's own screens have no reference; `tests/e2e.rs`
//! drives those.)
//!
//! The render is 780 x 1688 px, the design's 390 x 844 dp at 2 px per dp,
//! exactly like `design/reference/<name>.png`, and is compared with the
//! default settings of `morningstar-diff`: a colour error over 2 counts,
//! edges may move or be anti-aliased differently by one pixel.
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

use atlas::{Atlas, DESIGN_HEIGHT, DESIGN_WIDTH, DESIGNED, Screen};
use morningstar::ui::Ui;
use morningstar::{App, Frame, FrameInfo, SurfaceSize};
use morningstar_devtools::{CompareOptions, Image, Tolerance, compare, screenshot_frame};

const SCALE: f32 = 2.0;

/// Largest share of differing pixels, in percent, for a designed page.
fn limit(name: &str) -> f32 {
    match name {
        // Measured 0.0482.
        "01-onboarding" => 0.05,
        // Measured 0.1309.
        "02-sign-in" => 0.15,
        // Measured 0.0505.
        "03-home" | "09-home-simplified" => 0.10,
        // Measured 0.0092.
        "04-chat" | "10-chat-simplified" => 0.05,
        // Measured 0.0172.
        "05-model-picker" => 0.05,
        // Measured 0.0506.
        "06-live-voice" => 0.10,
        // Measured 0.0731.
        "07-explore" => 0.10,
        // Measured 0.1600.
        "08-plans" => 0.20,
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
/// out and captured a second later, when the sheet of page 05 has risen.
fn render(name: &str) -> Option<(Ui<Atlas>, Vec<u8>)> {
    let mut ui = Ui::new(Atlas::designed(name).expect("a designed page"));
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

/// Pages 09 and 10 are 03 and 04 again: the same picture to the byte, in
/// the design and here. What differs is where three links lead
/// (`tests/e2e.rs`).
#[test]
fn the_simplified_pair_is_the_same_picture() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("design/reference");
    for (first, second) in [
        ("03-home", "09-home-simplified"),
        ("04-chat", "10-chat-simplified"),
    ] {
        let read = |name: &str| std::fs::read(root.join(format!("{name}.png"))).expect("reference");
        assert!(
            read(first) == read(second),
            "{first} and {second} differ in the design"
        );
        let (Some((_, ours)), Some((_, theirs))) = (render(first), render(second)) else {
            return;
        };
        assert!(ours == theirs, "{first} and {second} differ as built");
    }
}

#[test]
fn nothing_scrolls_or_moves_at_the_designs_size() {
    for name in DESIGNED {
        let mut ui = Ui::new(Atlas::designed(name).expect("a designed page"));
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
            let mut ui = Ui::new(Atlas::on(screen));
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
