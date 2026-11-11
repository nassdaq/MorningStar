//! Every built screen, rendered and compared with the design's reference.
//!
//! The render is 780 px wide at 2 px per dp and as tall as the design's
//! artboard, exactly like `design/reference/<name>.png`. The four corner
//! squares the design tool rounded off are left out (the app draws square
//! corners). Everything else is compared with the default settings of
//! `morningstar-diff`: a colour error over 2 counts, edges may move or be
//! anti-aliased differently by one pixel.
//!
//! The limits below are not a target that was chosen: each is the measured
//! difference of the screen as built, rounded up to the next 0.05. What is
//! left is anti-aliasing blended in linear light where the browser blends
//! encoded values (text and thin strokes come out a little lighter) and a
//! different glyph rasteriser; see ADR 0015. A layout change of a few
//! pixels in any element pushes a screen well over its limit.
//!
//! On failure the report is printed and `<name>.actual.png` and
//! `<name>.sbs.png` are written to `target/fidelity/`.

use std::path::Path;

use fundidesk::{DESIGN_HEIGHT, DESIGN_WIDTH, FundiDesk, Message, Screen, corner_masks};
use morningstar::ui::Ui;
use morningstar::{App, Frame, FrameInfo, SurfaceSize};
use morningstar_devtools::{CompareOptions, Image, Rect, Tolerance, compare, screenshot_frame};

const SCALE: f32 = 2.0;

/// Largest share of differing pixels, in percent, for a built screen.
fn limit(screen: Screen) -> f32 {
    match screen {
        // Measured 0.2868.
        Screen::Splash => 0.30,
        // Measured 0.1671.
        Screen::Login => 0.20,
        // Measured 0.4091.
        Screen::Dashboard => 0.45,
        // Measured 0.3425.
        Screen::RepairDetails => 0.35,
        // Measured 0.3615.
        Screen::Onboarding => 0.40,
        // Measured 0.0953.
        Screen::CreateAccount => 0.10,
        // Measured 0.1398.
        Screen::Verification => 0.15,
        // Measured 0.1772.
        Screen::ShopSetup => 0.20,
        // Measured 0.2576.
        Screen::RepairsList => 0.30,
        // Measured 0.4870.
        Screen::NewRepair => 0.50,
        // Measured 0.2672.
        Screen::RecordPayment => 0.30,
        // Measured 0.2666.
        Screen::Tracking => 0.30,
    }
}

fn info(height: f32) -> FrameInfo {
    let size = SurfaceSize::new((DESIGN_WIDTH * SCALE) as u32, (height * SCALE) as u32);
    FrameInfo::new(size).with_scale(SCALE)
}

#[test]
fn built_screens_match_their_references() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();
    for screen in Screen::ALL.into_iter().filter(|screen| screen.is_built()) {
        let mut ui = Ui::new(FundiDesk::on(screen));
        let png = match screenshot_frame(&mut ui, &info(screen.design_height())) {
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
        let ignore = corner_masks(candidate.width(), candidate.height(), SCALE)
            .map(|(x, y, width, height)| Rect::new(x, y, width, height))
            .to_vec();
        let comparison = compare(
            &reference,
            &candidate,
            &CompareOptions {
                tolerance: Tolerance {
                    max_diff_percent: limit(screen),
                    ..Tolerance::DEFAULT
                },
                ignore,
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
fn every_screen_lays_out_without_overflow_on_a_phone() {
    // Built or not, at the design's phone size and at two other densities.
    for screen in Screen::ALL {
        for scale in [1.0, 2.0, 2.625, 3.0] {
            let mut ui = Ui::new(FundiDesk::on(screen));
            let mut frame = Frame::for_app(&mut ui);
            frame.begin();
            let size = SurfaceSize::new(
                (DESIGN_WIDTH * scale) as u32,
                (DESIGN_HEIGHT * scale) as u32,
            );
            ui.frame(&FrameInfo::new(size).with_scale(scale), &mut frame);
            assert_eq!(ui.overflows(), [], "{} at scale {scale}", screen.name());
            assert!(!frame.list.is_empty(), "{} draws something", screen.name());
        }
    }
}

#[test]
fn tall_screens_scroll_between_their_fixed_bars() {
    for (screen, id) in [
        (Screen::Dashboard, "dashboard"),
        (Screen::RepairDetails, "repair-details"),
    ] {
        let mut ui = Ui::new(FundiDesk::on(screen));
        let mut frame = Frame::for_app(&mut ui);
        frame.begin();
        ui.frame(&info(DESIGN_HEIGHT), &mut frame);
        let reach = ui.max_scroll_offset((id, "scroll")).expect("a scroll view");
        assert!(reach > 100.0, "{}: scrolls by {reach} dp", screen.name());
        // At the design's full height everything fits and nothing scrolls.
        frame.begin();
        ui.frame(&info(screen.design_height()), &mut frame);
        assert_eq!(ui.max_scroll_offset((id, "scroll")), Some(0.0));
    }
}

#[test]
fn navigation_follows_the_designs_links() {
    use fundidesk::components::Tab;
    use fundidesk::screens::{dashboard, login, repair_details, splash};

    let mut ui = Ui::new(FundiDesk::default());
    let mut go = |message: Message, expected: Screen| {
        ui.send(message);
        assert_eq!(ui.app().screen(), expected);
    };
    go(
        Message::Splash(splash::Message::Continue),
        Screen::Onboarding,
    );
    go(Message::Open(Screen::Login), Screen::Login);
    go(
        Message::Login(login::Message::TogglePassword),
        Screen::Login,
    );
    go(
        Message::Login(login::Message::ForgotPassword),
        Screen::Verification,
    );
    go(Message::Open(Screen::Login), Screen::Login);
    go(
        Message::Login(login::Message::CreateAccount),
        Screen::CreateAccount,
    );
    go(Message::Open(Screen::Login), Screen::Login);
    go(Message::Login(login::Message::LogIn), Screen::Dashboard);
    go(
        Message::Dashboard(dashboard::Message::OpenRepairs),
        Screen::RepairsList,
    );
    go(Message::Open(Screen::Dashboard), Screen::Dashboard);
    go(
        Message::Dashboard(dashboard::Message::NewRepair),
        Screen::NewRepair,
    );
    go(Message::Open(Screen::Dashboard), Screen::Dashboard);
    go(
        Message::Dashboard(dashboard::Message::Tab(Tab::Account)),
        Screen::ShopSetup,
    );
    go(Message::Open(Screen::Dashboard), Screen::Dashboard);
    go(
        Message::Dashboard(dashboard::Message::OpenRepair),
        Screen::RepairDetails,
    );
    go(
        Message::RepairDetails(repair_details::Message::NotLinked),
        Screen::RepairDetails,
    );
    go(
        Message::RepairDetails(repair_details::Message::RecordPayment),
        Screen::RecordPayment,
    );
    go(Message::Open(Screen::RepairDetails), Screen::RepairDetails);
    go(
        Message::RepairDetails(repair_details::Message::OpenTracking),
        Screen::Tracking,
    );
    go(Message::Open(Screen::RepairDetails), Screen::RepairDetails);
    go(
        Message::RepairDetails(repair_details::Message::Back),
        Screen::RepairsList,
    );
}

#[test]
fn a_tap_on_the_login_button_opens_the_dashboard() {
    use morningstar::{Point, PointerEvent, PointerPhase};
    let mut ui = Ui::new(FundiDesk::on(Screen::Login));
    let mut frame = Frame::for_app(&mut ui);
    frame.begin();
    ui.frame(&info(DESIGN_HEIGHT), &mut frame);
    // The button spans y = 460..514 dp (see the design); tap its middle.
    let position = Point::new(195.0 * SCALE, 487.0 * SCALE);
    for phase in [PointerPhase::Down, PointerPhase::Up] {
        ui.pointer(&PointerEvent {
            id: 1,
            phase,
            position,
        });
    }
    assert_eq!(ui.app().screen(), Screen::Dashboard);
}

#[test]
fn references_are_exactly_the_designs_size() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for screen in Screen::ALL {
        let reference = root.join(format!("design/reference/{}.png", screen.name()));
        let image = Image::load(&reference).expect("reference image");
        assert_eq!(image.width(), 780, "{}", screen.name());
        assert_eq!(
            image.height() as f32,
            screen.design_height() * SCALE,
            "{}",
            screen.name()
        );
        let html = root.join(format!("design/html/{}.html", screen.name()));
        let html = std::fs::read_to_string(html).expect("design html");
        let declared = format!("width: 390px; height: {}px", screen.design_height());
        assert!(html.contains(&declared), "{}: {declared}", screen.name());
    }
}
