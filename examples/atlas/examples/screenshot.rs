//! Renders Atlas's screens to PNG files on this machine's GPU, and
//! compares the designed ones with the design. No device needed.
//!
//!   cargo run -p atlas --example screenshot -- <output-dir> [screen ...]
//!       [--diff] [--phone | --small | --tall] [--states] [--sheet] [--semantics]
//!
//! With no screen named, every screen is rendered; name them as the design
//! does (`03-home`) or by number (`03`). For each it writes `<name>.png`,
//! opened directly in the state the design shows it in, as a still frame.
//!
//! * By default the frame is the design's: 390 x 844 dp at 2 px per dp
//!   (780 x 1688 px), with no insets, like the reference images.
//! * `--phone` is a Galaxy A27: 1080 x 2340 px at 2.8125 px per dp
//!   (384 x 832 dp) with the status bar (102 px) and the gesture bar
//!   (42 px) as insets. `--small` is 360 x 640 dp at 2 px per dp and
//!   `--tall` 411 x 914 dp at 2.625.
//! * `--diff` compares each designed screen with
//!   `design/reference/<name>.png`, prints the report and writes
//!   `<name>.diff.png` (heat map) and `<name>.sbs.png` (reference, render
//!   and heat map side by side). Only meaningful in the design's frame.
//! * `--states` adds states that take a journey to reach: the sign-in form
//!   refusing an address, the picker over a chat with another model
//!   chosen, a chat thinking, working with its steps unfolded and
//!   answered, voice half-way through its line, Explore filtered, a plan
//!   chosen, and the app's own two screens.
//! * `--sheet` writes `sheet.png`: everything rendered, side by side.
//! * `--semantics` also writes `<name>.semantics.png` for each screen: the
//!   screen as a screen reader finds it (ADR 0032), and prints what
//!   `Ui::accessibility_issues` finds.

use std::path::{Path, PathBuf};
use std::time::Duration;

use atlas::{
    Atlas, Category, DESIGN_HEIGHT, DESIGN_WIDTH, DESIGNED, Flow, Message, Mode, Plan, Screen,
};
use morningstar::ui::Ui;
use morningstar::{App, Frame, FrameInfo, Insets, SurfaceSize};
use morningstar_devtools::{
    CompareOptions, Image, compare, screenshot_frame, screenshot_semantics,
};

/// The frame to render in.
fn frame(kind: &str) -> FrameInfo {
    match kind {
        "phone" => FrameInfo::new(SurfaceSize::new(1080, 2340))
            .with_scale(2.8125)
            .with_insets(Insets::new(0.0, 102.0, 0.0, 42.0)),
        "small" => FrameInfo::new(SurfaceSize::new(720, 1280)).with_scale(2.0),
        "tall" => FrameInfo::new(SurfaceSize::new(1080, 2400)).with_scale(2.625),
        _ => FrameInfo::new(SurfaceSize::new(
            (DESIGN_WIDTH * 2.0) as u32,
            (DESIGN_HEIGHT * 2.0) as u32,
        ))
        .with_scale(2.0),
    }
}

/// The states that take a journey to reach, each with the messages that
/// lead there from the screen it starts on.
fn states() -> Vec<(&'static str, Screen, Vec<Message>)> {
    let home = Screen::Home(Flow::Standard);
    let ticks = |count: usize| std::iter::repeat_n(Message::Tick, count);
    let ask = |after: usize| {
        let mut messages = vec![
            Message::Attach,
            Message::Draft("Compare the prices in these quotes".to_owned()),
            Message::Send,
        ];
        messages.extend(ticks(after));
        messages
    };
    let mut working = ask(50);
    working.push(Message::ToggleSteps(0));
    vec![
        (
            "02b-sign-in-refused",
            Screen::SignIn,
            vec![Message::Email("amina".to_owned()), Message::SignIn],
        ),
        (
            "03b-home-typing",
            home,
            vec![
                Message::Attach,
                Message::Draft("Compare these quotes".to_owned()),
            ],
        ),
        ("04b-chat-thinking", home, ask(3)),
        ("04c-chat-working-unfolded", home, working),
        ("04d-chat-answered", home, ask(200)),
        (
            "05b-picker-over-chat",
            Screen::Chat(Flow::Standard),
            vec![Message::SelectMode(Mode::Smart), Message::OpenPicker],
        ),
        (
            "06b-voice-listening",
            home,
            std::iter::once(Message::OpenVoice)
                .chain(ticks(40))
                .collect(),
        ),
        (
            "07b-explore-filtered",
            Screen::Explore,
            vec![
                Message::Filter(Category::Writing),
                Message::Query("s".to_owned()),
            ],
        ),
        (
            "08b-plans-chosen",
            Screen::Plans,
            vec![Message::Choose(Plan::Basic)],
        ),
        ("11-library", Screen::Library, Vec::new()),
        ("12-models", Screen::Models, Vec::new()),
    ]
}

/// One frame to lay the screen out, then the picture a second later, once
/// whatever slides or fades in has arrived.
fn render(ui: &mut Ui<Atlas>, info: &FrameInfo) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut first = Frame::for_app(ui);
    first.begin();
    ui.frame(info, &mut first);
    let png = screenshot_frame(ui, &info.with_elapsed(Duration::from_secs(1)))?;
    let overflows = ui.overflows();
    if !overflows.is_empty() {
        println!("  layout overflows: {overflows:?}");
    }
    Ok(png)
}

fn diff(name: &str, png: &[u8], directory: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let reference = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("design/reference")
        .join(format!("{name}.png"));
    let reference = Image::load(&reference)?;
    let candidate = Image::from_png(png)?;
    let comparison = compare(&reference, &candidate, &CompareOptions::default());
    println!("{}", comparison.report);
    comparison
        .heat_map()
        .save(directory.join(format!("{name}.diff.png")))?;
    comparison
        .side_by_side()
        .save(directory.join(format!("{name}.sbs.png")))?;
    Ok(())
}

/// The screen as a screen reader finds it, and what the checker says.
fn semantics(
    ui: &mut Ui<Atlas>,
    info: &FrameInfo,
    name: &str,
    directory: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let (picture, _) = screenshot_semantics(ui, info)?;
    picture.save(directory.join(format!("{name}.semantics.png")))?;
    ui.set_accessibility(true);
    let mut frame = Frame::for_app(ui);
    frame.begin();
    ui.frame(info, &mut frame);
    for issue in ui.accessibility_issues() {
        println!("  {issue:?}");
    }
    Ok(())
}

/// Every render side by side, `columns` to a row, each 360 px wide.
fn sheet(renders: &[Vec<u8>], columns: usize) -> Result<Image, Box<dyn std::error::Error>> {
    const CELL: u32 = 360;
    const GAP: u32 = 12;
    let mut cells = Vec::new();
    for png in renders {
        let image = Image::from_png(png)?;
        let height =
            (u64::from(image.height()) * u64::from(CELL) / u64::from(image.width())) as u32;
        cells.push(image.resized(CELL, height));
    }
    let cell_height = cells.iter().map(Image::height).max().unwrap_or(0);
    let rows = cells.len().div_ceil(columns) as u32;
    let width = GAP + columns as u32 * (CELL + GAP);
    let height = GAP + rows * (cell_height + GAP);
    let mut sheet = Image::filled(width, height, [58, 58, 60, 255]);
    for (index, cell) in cells.iter().enumerate() {
        let left = GAP + (index % columns) as u32 * (CELL + GAP);
        let top = GAP + (index / columns) as u32 * (cell_height + GAP);
        for y in 0..cell.height() {
            for x in 0..cell.width() {
                sheet.set_pixel(left + x, top + y, cell.pixel(x, y));
            }
        }
    }
    Ok(sheet)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut directory = None;
    let mut wanted = Vec::new();
    let mut kind = "design";
    let (mut with_diff, mut with_states, mut with_sheet, mut with_semantics) =
        (false, false, false, false);
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--semantics" => with_semantics = true,
            "--diff" => with_diff = true,
            "--states" => with_states = true,
            "--sheet" => with_sheet = true,
            "--phone" => kind = "phone",
            "--small" => kind = "small",
            "--tall" => kind = "tall",
            other if directory.is_none() => directory = Some(PathBuf::from(other)),
            other => wanted.push(other.to_owned()),
        }
    }
    let directory = directory.unwrap_or_else(|| PathBuf::from("out"));
    std::fs::create_dir_all(&directory)?;
    let info = frame(kind);
    let named = |name: &str| {
        wanted.is_empty()
            || wanted
                .iter()
                .any(|want| name == want || name.starts_with(&format!("{want}-")))
    };

    let mut renders = Vec::new();
    for name in DESIGNED {
        if !named(name) {
            continue;
        }
        let mut ui = Ui::new(Atlas::designed(name).expect("a designed page"));
        let png = render(&mut ui, &info)?;
        std::fs::write(directory.join(format!("{name}.png")), &png)?;
        println!("{name}");
        if with_diff {
            diff(name, &png, &directory)?;
        }
        if with_semantics {
            let mut ui = Ui::new(Atlas::designed(name).expect("a designed page"));
            semantics(&mut ui, &info, name, &directory)?;
        }
        renders.push(png);
    }
    if with_states {
        for (name, screen, messages) in states() {
            if !named(name) {
                continue;
            }
            let mut ui = Ui::new(Atlas::on(screen));
            let mut first = Frame::for_app(&mut ui);
            first.begin();
            ui.frame(&info, &mut first);
            for message in messages {
                ui.send(message);
            }
            let png = render(&mut ui, &info)?;
            std::fs::write(directory.join(format!("{name}.png")), &png)?;
            println!("{name}");
            if with_semantics {
                semantics(&mut ui, &info, name, &directory)?;
            }
            renders.push(png);
        }
    }
    if with_sheet {
        sheet(&renders, 5)?.save(directory.join("sheet.png"))?;
        println!("sheet.png");
    }
    Ok(())
}
