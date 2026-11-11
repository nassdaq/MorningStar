//! Renders every showcase screen to PNG files on this machine's GPU. No
//! device needed.
//!
//!   cargo run -p showcase --example screenshot [output-dir] [scale]
//!
//! Writes the three screens as a 1080 x 2400 phone shows them (3 px per dp
//! unless another scale is given, with a status bar and a gesture bar), plus
//! a few states reached by sending messages and touches: the home screen
//! scrolled, in English, and with a row held down.

use std::path::PathBuf;

use morningstar::ui::Ui;
use morningstar::{App, Frame, FrameInfo, Insets, Point, PointerEvent, PointerPhase, SurfaceSize};
use morningstar_devtools::screenshot_frame;
use showcase::{Message, Screen, Showcase};

const SIZE: SurfaceSize = SurfaceSize::new(1080, 2400);

/// Builds a frame without rendering it, so the app knows where things are
/// and can be touched.
fn lay_out(ui: &mut Ui<Showcase>, info: &FrameInfo) {
    let mut frame = Frame::for_app(ui);
    frame.begin();
    ui.frame(info, &mut frame);
}

fn touch(ui: &mut Ui<Showcase>, phase: PointerPhase, x: f32, y: f32) {
    ui.pointer(&PointerEvent {
        id: 1,
        phase,
        position: Point::new(x, y),
    });
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let directory = PathBuf::from(args.next().unwrap_or_else(|| ".".to_owned()));
    let scale: f32 = args.next().map_or(Ok(3.0), |value| value.parse())?;
    std::fs::create_dir_all(&directory)?;

    // Status bar 32 dp, gesture bar 20 dp.
    let info = FrameInfo::new(SIZE)
        .with_scale(scale)
        .with_insets(Insets::new(0.0, 32.0 * scale, 0.0, 20.0 * scale));
    let save = |ui: &mut Ui<Showcase>, name: &str| {
        let path = directory.join(name);
        std::fs::write(&path, screenshot_frame(ui, &info)?)?;
        println!("{}", path.display());
        let overflows = ui.overflows();
        if !overflows.is_empty() {
            println!("  layout overflows: {overflows:?}");
        }
        Ok::<_, Box<dyn std::error::Error>>(())
    };

    // The three screens, each from a fresh app.
    for (screen, name) in [
        (Screen::Home, "home.png"),
        (Screen::Settings, "settings.png"),
        (Screen::Login, "login.png"),
    ] {
        save(&mut Ui::new(Showcase::on(screen)), name)?;
    }

    // States reached the way a user reaches them.
    let mut ui = Ui::new(Showcase::default());
    lay_out(&mut ui, &info);
    // Drag the list up by 300 dp, then rest so that nothing flings.
    let x = 540.0;
    touch(&mut ui, PointerPhase::Down, x, 2000.0);
    touch(&mut ui, PointerPhase::Moved, x, 2000.0 - 10.0 * scale);
    touch(&mut ui, PointerPhase::Moved, x, 2000.0 - 310.0 * scale);
    touch(&mut ui, PointerPhase::Cancelled, x, 0.0);
    save(&mut ui, "home-scrolled.png")?;

    // Hold a transaction row: pressed visuals.
    let row = ui
        .rect_of(("transaction", 5_usize))
        .expect("row 5 is on screen");
    touch(
        &mut ui,
        PointerPhase::Down,
        row.x + row.width / 2.0,
        row.y + row.height / 2.0,
    );
    save(&mut ui, "home-pressed.png")?;
    touch(&mut ui, PointerPhase::Cancelled, 0.0, 0.0);

    let mut ui = Ui::new(Showcase::default());
    ui.send(Message::ToggleLanguage);
    ui.send(Message::ToggleBalance);
    save(&mut ui, "home-english.png")?;

    let mut ui = Ui::new(Showcase::on(Screen::Settings));
    ui.send(Message::ToggleLanguage);
    ui.send(Message::ToggleBiometrics);
    save(&mut ui, "settings-english.png")?;

    let mut ui = Ui::new(Showcase::on(Screen::Login));
    ui.send(Message::ToggleLanguage);
    save(&mut ui, "login-english.png")?;
    Ok(())
}
