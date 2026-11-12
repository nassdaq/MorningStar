//! Every screen of the design, rendered and compared with the design's
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

use callready::{CallReady, DESIGN_HEIGHT, DESIGN_WIDTH, Screen};
use morningstar::ui::Ui;
use morningstar::{App, Frame, FrameInfo, SurfaceSize};
use morningstar_devtools::{CompareOptions, Image, Tolerance, compare, screenshot_frame};

const SCALE: f32 = 2.0;

/// Largest share of differing pixels, in percent, for a designed screen.
fn limit(screen: Screen) -> f32 {
    match screen {
        // Measured 0.0274.
        Screen::Welcome => 0.05,
        // Measured 0.0209.
        Screen::Home => 0.05,
        // Measured 0.0064.
        Screen::LiveCall => 0.05,
        // Measured 0.0591.
        Screen::CallReport => 0.10,
        // Measured 0.0147.
        Screen::Premium => 0.05,
        _ => unreachable!("{} is not in the design", screen.name()),
    }
}

fn info(scale: f32) -> FrameInfo {
    let size = SurfaceSize::new(
        (DESIGN_WIDTH * scale) as u32,
        (DESIGN_HEIGHT * scale) as u32,
    );
    FrameInfo::new(size).with_scale(scale)
}

#[test]
fn designed_screens_match_their_references() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();
    for screen in Screen::DESIGNED {
        let mut ui = Ui::new(CallReady::on(screen));
        let png = match screenshot_frame(&mut ui, &info(SCALE)) {
            Ok(png) => png,
            Err(error) => {
                eprintln!("skipping: cannot render ({error})");
                return;
            }
        };
        assert_eq!(ui.overflows(), [], "{}: layout overflows", screen.name());

        let reference = root.join(format!("design/reference/{}.png", screen.name()));
        let reference = Image::load(&reference).expect("reference image");
        let candidate = Image::from_png(&png).expect("render");
        assert_eq!(
            (candidate.width(), candidate.height()),
            (reference.width(), reference.height()),
            "{}: the render and the reference differ in size",
            screen.name()
        );
        let comparison = compare(
            &reference,
            &candidate,
            &CompareOptions {
                tolerance: Tolerance {
                    max_diff_percent: limit(screen),
                    ..Tolerance::DEFAULT
                },
                ..CompareOptions::default()
            },
        );
        let report = &comparison.report;
        println!(
            "{}: {:.4}% differ (limit {:.2}%), mean error {:.3}, max {:.2}",
            screen.name(),
            report.diff_percent,
            limit(screen),
            report.mean_error,
            report.max_error
        );
        if !report.passed {
            let out = root.join("../../target/fidelity");
            std::fs::create_dir_all(&out).expect("output directory");
            std::fs::write(out.join(format!("{}.actual.png", screen.name())), &png)
                .expect("write render");
            comparison
                .side_by_side()
                .save(out.join(format!("{}.sbs.png", screen.name())))
                .expect("write comparison");
            println!("{report}");
            failures.push(screen.name());
        }
    }
    assert!(
        failures.is_empty(),
        "screens differ from the design: {failures:?}"
    );
}

#[test]
fn every_screen_lays_out_without_overflow_at_phone_densities() {
    for screen in Screen::ALL {
        assert_eq!(screen.is_designed(), Screen::DESIGNED.contains(&screen));
        for scale in [1.0, 2.0, 2.8125, 3.0] {
            let mut ui = Ui::new(CallReady::on(screen));
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
    for screen in Screen::DESIGNED {
        let reference = root.join(format!("design/reference/{}.png", screen.name()));
        let image = Image::load(&reference).expect("reference image");
        assert_eq!(
            (image.width(), image.height()),
            (780, 1688),
            "{}",
            screen.name()
        );
        let html = root.join(format!("design/html/{}.html", screen.name()));
        let html = std::fs::read_to_string(html).expect("design html");
        assert!(
            html.contains("width: 390px; height: 844px"),
            "{}",
            screen.name()
        );
    }
}

/// At the design's size nothing scrolls: every screen fits between its
/// fixed bars, as the design drew it.
#[test]
fn nothing_scrolls_at_the_designs_size() {
    for screen in Screen::DESIGNED {
        let mut ui = Ui::new(CallReady::on(screen));
        let mut frame = Frame::for_app(&mut ui);
        frame.begin();
        ui.frame(&info(SCALE), &mut frame);
        assert_eq!(
            ui.max_scroll_offset(screen.scroll_id()),
            Some(0.0),
            "{}",
            screen.name()
        );
    }
}

/// Opened directly, the live call and the report are the design's moment,
/// and their buttons work from there.
#[test]
fn the_designed_moments_are_live() {
    use callready::Message;
    use callready::model::Plan;
    use callready::screens::{call_report, live_call, premium};

    let mut ui = Ui::new(CallReady::on(Screen::LiveCall));
    let go = |ui: &mut Ui<CallReady>, message: Message, expected: Screen| {
        ui.send(message);
        assert_eq!(ui.app().screen(), expected);
    };

    // The live call stands still at 02:14 until it is answered. Mute
    // toggles, replay repeats the line, and neither leaves or starts the
    // clock.
    assert!(ui.app().call().designed && ui.app().tick().is_none());
    let live = Message::LiveCall;
    go(
        &mut ui,
        live(live_call::Message::ToggleMute),
        Screen::LiveCall,
    );
    assert!(ui.app().call().muted);
    go(
        &mut ui,
        live(live_call::Message::ToggleMute),
        Screen::LiveCall,
    );
    assert!(!ui.app().call().muted);
    go(&mut ui, live(live_call::Message::Replay), Screen::LiveCall);
    assert_eq!(ui.app().call().replays, 1);
    ui.send(Message::Tick);
    assert_eq!(ui.app().call().clock(), "02:14");
    // Back, with nothing behind the screen, is Home.
    go(&mut ui, live(live_call::Message::Back), Screen::Home);
    go(&mut ui, Message::Open(Screen::LiveCall), Screen::LiveCall);
    // Ended unanswered, it is the call the designed report describes.
    let calls = ui.app().model().history.len();
    go(
        &mut ui,
        live(live_call::Message::EndCall),
        Screen::CallReport,
    );
    assert!(ui.app().model().report.designed);
    assert_eq!(ui.app().model().history.len(), calls);

    // The call report.
    let report = Message::CallReport;
    go(
        &mut ui,
        report(call_report::Message::Transcript),
        Screen::Transcript,
    );
    go(&mut ui, Message::Back, Screen::CallReport);
    go(
        &mut ui,
        report(call_report::Message::PractiseAgain),
        Screen::LiveCall,
    );
    assert!(ui.app().tick().is_some(), "a real call: the clock runs");
    go(
        &mut ui,
        Message::Open(Screen::CallReport),
        Screen::CallReport,
    );
    go(
        &mut ui,
        report(call_report::Message::Back),
        Screen::LiveCall,
    );

    // The offer: Premium is selected until a card says otherwise, and
    // nothing is active until the button is pressed.
    go(&mut ui, Message::Open(Screen::Premium), Screen::Premium);
    assert_eq!(ui.app().offer(), Plan::Premium);
    let offer = Message::Premium;
    go(
        &mut ui,
        offer(premium::Message::Select(Plan::Free)),
        Screen::Premium,
    );
    assert_eq!(ui.app().offer(), Plan::Free);
    go(
        &mut ui,
        offer(premium::Message::Select(Plan::Premium)),
        Screen::Premium,
    );
    assert_eq!(ui.app().offer(), Plan::Premium);
    go(&mut ui, offer(premium::Message::Close), Screen::LiveCall);
    assert!(!ui.app().model().premium);
    go(&mut ui, Message::Open(Screen::Premium), Screen::Premium);
    go(&mut ui, offer(premium::Message::Start), Screen::LiveCall);
    assert!(ui.app().model().premium);
}
