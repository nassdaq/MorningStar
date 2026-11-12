//! The selection of a focused `TextInput` as pixels on the real GPU: the
//! highlight behind the text, the two handles and the contextual menu, in
//! the light and the dark theme, at the frame of a Galaxy A27. A few
//! pixels are asserted; the rest is meant to be judged by eye.
//!
//! Set `MORNINGSTAR_DUMP_DIR` to a directory to have the screens written
//! there as `selection_light.png` and `selection_dark.png`. Skips where
//! there is no GPU.

use std::time::Duration;

use morningstar_core::{Insets, Point, PointerEvent, PointerPhase, Rect, SurfaceSize};
use morningstar_devtools::{Image as Screenshot, screenshot_frame};
use morningstar_runtime::{App, Frame, FrameInfo, GpuError};
use morningstar_ui::prelude::*;

const REGULAR: &[u8] =
    include_bytes!("../../../examples/hello-gpu/assets/fonts/NotoSans-Regular.ttf");
const BOLD: &[u8] = include_bytes!("../../../examples/hello-gpu/assets/fonts/NotoSans-Bold.ttf");
const ARABIC: &[u8] =
    include_bytes!("../../../examples/hello-gpu/assets/fonts/NotoSansArabic-Regular.ttf");

const NOTE: &str = "Fundi atafika kesho asubuhi na vifaa vyote vya kazi, tafadhali \
                    hakikisha mlango uko wazi na umeme upo";

struct Form {
    dark: bool,
}

fn field(label: &'static str, input: TextInput<()>) -> Container<()> {
    col![
        text(label).caption().color(ThemeColor::TextMuted),
        Container::new(input)
            .fill_width()
            .padding_xy(16.0, 14.0)
            .radius(12.0)
            .background(ThemeColor::Surface)
            .border(1.0, ThemeColor::Divider)
            .focused_border(2.0, ThemeColor::Primary)
            .input_frame(),
    ]
    .gap(6.0)
    .fill_width()
}

impl Application for Form {
    type Message = ();

    fn setup(&mut self, setup: &mut Setup<'_>) {
        setup.add_font(REGULAR);
        setup.add_font(BOLD);
        setup.add_font(ARABIC);
    }

    fn theme(&self) -> Theme {
        if self.dark { Theme::DARK } else { Theme::LIGHT }
    }

    fn view(&self) -> Element<()> {
        col![
            text("Kazi mpya").title(),
            field(
                "Jina la mteja",
                TextInput::new("Juma Mwangi wa Arusha")
                    .on_input(|_| ())
                    .id("name")
            ),
            field(
                "Maelezo",
                TextInput::new(NOTE).multiline().on_input(|_| ()).id("note")
            ),
            field(
                "Salamu",
                TextInput::new("مَرْحَبًا بك يا صديقي")
                    .on_input(|_| ())
                    .id("greeting")
            ),
        ]
        .gap(18.0)
        .padding(20.0)
        .fill()
        .safe_area(SafeEdges::ALL)
        .into()
    }

    fn update(&mut self, _: ()) {}
}

/// A colour as the byte triple the renderer writes for it.
fn encoded(color: Color) -> [f32; 3] {
    let [r, g, b, _] = color.srgb();
    [r * 255.0, g * 255.0, b * 255.0]
}

fn assert_near(shot: &Screenshot, at: Point, expected: [f32; 3], what: &str) {
    let actual = shot.pixel(at.x as u32, at.y as u32);
    for channel in 0..3 {
        assert!(
            (f32::from(actual[channel]) - expected[channel]).abs() <= 3.0,
            "{what} at {at:?}: {actual:?}, expected {expected:?}"
        );
    }
}

/// Long-presses in the middle of the second line of the note, drags the
/// end handle to the next line, and renders what is on screen then.
fn selection_screen(dark: bool, file: &str) {
    let info = FrameInfo::new(SurfaceSize::new(1080, 2340))
        .with_scale(2.8125)
        .with_insets(Insets::new(0.0, 102.0, 0.0, 42.0));
    let mut ui = Ui::testing(Form { dark });
    let mut frame = Frame::for_app(&mut ui);
    let mut time = Duration::ZERO;
    let mut draw = |ui: &mut Ui<Form>, frame: &mut Frame, after: u64| {
        time += Duration::from_millis(after);
        frame.begin();
        ui.frame(&info.with_elapsed(time), frame);
        time
    };
    let touch = |ui: &mut Ui<Form>, phase: PointerPhase, position: Point| {
        ui.pointer(&PointerEvent {
            id: 1,
            phase,
            position,
        });
    };
    draw(&mut ui, &mut frame, 0);
    let note = ui.rect_of("note").unwrap();
    let line = note.height / 3.0;
    let press = Point::new(note.x + 260.0, note.y + line * 1.5);
    touch(&mut ui, PointerPhase::Down, press);
    draw(&mut ui, &mut frame, 0);
    draw(&mut ui, &mut frame, 600);
    touch(&mut ui, PointerPhase::Up, press);
    draw(&mut ui, &mut frame, 10);
    assert!(ui.selection().is_some(), "a word is selected");
    // The end handle, one line down and to the right.
    let [_, end] = ui.selection_handles();
    let end = end.expect("the end handle");
    let grab = Point::new(end.x + end.width / 2.0, end.y + end.height / 2.0);
    let to = Point::new(grab.x + 300.0, grab.y + line);
    touch(&mut ui, PointerPhase::Down, grab);
    draw(&mut ui, &mut frame, 10);
    touch(&mut ui, PointerPhase::Moved, to);
    draw(&mut ui, &mut frame, 16);
    touch(&mut ui, PointerPhase::Up, to);
    let now = draw(&mut ui, &mut frame, 16);
    let (start, finish) = ui.selection().unwrap();
    assert!(NOTE[start..finish].contains(' '), "more than one word");
    let menu = ui.selection_menu();
    assert_eq!(menu.len(), 4);
    let [first, last] = ui.selection_handles().map(Option::unwrap);

    let png = match screenshot_frame(&mut ui, &info.with_elapsed(now)) {
        Ok(png) => png,
        Err(GpuError::NoAdapter(reason)) => {
            eprintln!("skipping: no GPU adapter available ({reason})");
            return;
        }
        Err(other) => panic!("render failed: {other}"),
    };
    if let Some(directory) = std::env::var_os("MORNINGSTAR_DUMP_DIR") {
        let path = std::path::Path::new(&directory).join(file);
        std::fs::write(&path, &png).expect("dump");
        eprintln!("wrote {}", path.display());
    }
    let shot = Screenshot::from_png(&png).expect("a PNG");
    let colors = ui.app().theme().colors;

    // The highlight: the primary colour at 40% over the field, composited
    // on encoded values (ADR 0023). Sampled at the foot of each end's
    // line, below the ink of letters without descenders and clear of the
    // menu's shadow, just inside where the handle hangs.
    let (primary, surface) = (encoded(colors.primary), encoded(colors.surface));
    let blend = [0, 1, 2].map(|channel| surface[channel] * 0.6 + primary[channel] * 0.4);
    let handle_tip = |rect: Rect, leans_left: bool| {
        // The touch rectangle reaches 12 dp either side of the 20 dp disc
        // and 6 dp above it.
        let reach = 12.0 * 2.8125;
        let disc = Rect::new(
            rect.x + reach,
            rect.y + reach / 2.0,
            rect.width - 2.0 * reach,
            rect.height - 1.5 * reach,
        );
        let x = if leans_left {
            disc.x + disc.width
        } else {
            disc.x
        };
        (Point::new(x, disc.y), disc)
    };
    let (start_tip, start_disc) = handle_tip(first, true);
    let (end_tip, end_disc) = handle_tip(last, false);
    assert_near(
        &shot,
        Point::new(end_tip.x - 6.0, end_tip.y - 3.0),
        blend,
        "highlight before the end",
    );
    assert_near(
        &shot,
        Point::new(start_tip.x + 6.0, start_tip.y - 3.0),
        blend,
        "highlight after the start",
    );
    // Outside the selection, on the same lines: the field.
    assert_near(
        &shot,
        Point::new(start_tip.x - 6.0, start_tip.y - 3.0),
        surface,
        "before the start",
    );
    assert_near(
        &shot,
        Point::new(end_tip.x + 6.0, end_tip.y - 3.0),
        surface,
        "after the end",
    );
    // The handles: solid discs in the primary colour, with their square
    // corner on the text and a round one away from it.
    assert_near(&shot, start_disc.center(), primary, "start handle");
    assert_near(&shot, end_disc.center(), primary, "end handle");
    assert_near(
        &shot,
        Point::new(start_tip.x - 2.0, start_tip.y + 2.0),
        primary,
        "the start handle's square corner",
    );
    assert_near(
        &shot,
        Point::new(end_tip.x + 2.0, end_tip.y + 2.0),
        primary,
        "the end handle's square corner",
    );
    let round = shot.pixel(
        start_disc.x as u32 + 1,
        (start_disc.y + start_disc.height) as u32 - 2,
    );
    assert!(
        (0..3).any(|channel| (f32::from(round[channel]) - primary[channel]).abs() > 20.0),
        "the corner away from the text is cut round: {round:?}"
    );
    // The menu: the surface colour, with its labels in the text colour
    // somewhere inside each item.
    let (_, cut) = menu[0];
    assert_near(
        &shot,
        Point::new(cut.x + 6.0, cut.y + cut.height / 2.0),
        surface,
        "menu body",
    );
    let ink = encoded(colors.text);
    for (label, item) in &menu {
        let mut found = false;
        for y in (item.y as u32 + 30)..(item.y + item.height) as u32 - 30 {
            for x in (item.x as u32 + 20)..(item.x + item.width) as u32 - 20 {
                let pixel = shot.pixel(x, y);
                found |= (0..3).all(|c| (f32::from(pixel[c]) - ink[c]).abs() <= 24.0);
            }
        }
        assert!(found, "no ink for {label:?}");
    }
}

#[test]
fn the_selection_its_handles_and_its_menu_in_the_light_theme() {
    selection_screen(false, "selection_light.png");
}

#[test]
fn the_selection_its_handles_and_its_menu_in_the_dark_theme() {
    selection_screen(true, "selection_dark.png");
}
