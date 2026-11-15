//! Renders Barua Talk's screens to PNG files on this machine's GPU, and
//! compares the designed ones with the design. No device needed.
//!
//!   cargo run -p barua-talk --example screenshot -- <output-dir> [screen ...]
//!       [--diff] [--phone | --small | --tall] [--states] [--online] [--sheet]
//!       [--semantics]
//!
//! With no screen named, every screen is rendered; name them as the design
//! does (`03-incoming`), by number (`03`) or by the app's own names
//! (`dial`). For each it writes `<name>.png`, opened directly in the state
//! the design shows it in, as a still frame.
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
//! * `--states` adds states that take a journey to reach: Home searching
//!   and with its sheet, a room after a message and its scripted answer,
//!   a call being made, a call with everything switched, a phone call
//!   ringing and with its keypad, the dial pad with a number.
//! * `--online` renders, instead of the screens, the states the app has
//!   with its server behind it: Home, a room, a message that failed, the
//!   people, a call ringing in and out and connected, a phone call, the
//!   Server screen. The tests' stand-in plays the server
//!   (`tests/support`): no network is used, and what the other people say
//!   and do is written here.
//! * `--sheet` writes `sheet.png`: everything rendered, side by side.
//! * `--semantics` also writes `<name>.semantics.png` for each screen: the
//!   screen as a screen reader finds it (ADR 0032), and prints what
//!   `Ui::accessibility_issues` finds.

#[path = "../tests/support/mod.rs"]
mod support;

use std::path::{Path, PathBuf};
use std::time::Duration;

use barua_talk::model::Filter;
use barua_talk::{BaruaTalk, DESIGN_HEIGHT, DESIGN_WIDTH, DESIGNED, Message, Screen};
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
    let ticks = |count: usize| std::iter::repeat_n(Message::Tick, count);
    let typed = |text: &str| Message::Draft(text.to_owned());
    let mut sent = vec![typed("Tukutane saa nne."), Message::Send];
    sent.extend(ticks(10));
    let mut dialled: Vec<Message> = "+255712345678".chars().map(Message::DialKey).collect();
    let pad = dialled.clone();
    dialled.push(Message::DialCall);
    let mut switched = vec![
        Message::ToggleMute,
        Message::ToggleCamera,
        Message::ToggleRecording,
        Message::VolumeUp,
    ];
    switched.extend(ticks(8));
    vec![
        (
            "home-calls",
            Screen::Home,
            vec![Message::SetFilter(Filter::Calls)],
        ),
        (
            "home-search",
            Screen::Home,
            vec![Message::ToggleSearch, Message::Query("duka".to_owned())],
        ),
        ("home-start", Screen::Home, vec![Message::OpenStart]),
        ("room-answered", Screen::Room, sent),
        ("room-playing", Screen::Room, {
            let mut play = vec![Message::PlayVoice(0)];
            play.extend(ticks(40));
            play
        }),
        (
            "call-out",
            Screen::People,
            vec![Message::CallPerson(2, true)],
        ),
        ("call-switched", Screen::VideoCall, switched),
        ("phone-ringing", Screen::Dial, dialled),
        (
            "phone-keypad",
            Screen::PhoneCall,
            vec![Message::ToggleKeypad, Message::Key('1'), Message::Key('#')],
        ),
        ("dial-number", Screen::Dial, pad),
        ("welcome-empty", Screen::Welcome, vec![Message::Continue]),
    ]
}

/// A named picture.
type Shot = (String, Vec<u8>);

/// The states the app has with its server behind it. The server is the
/// tests' stand-in (`tests/support`): it answers as `server/API.md` says,
/// and this function plays the other people by changing its state.
fn online(info: &FrameInfo) -> Result<Vec<Shot>, Box<dyn std::error::Error>> {
    use support::{Device, lock, world};
    let mut shots = Vec::new();
    let mut shoot = |name: &str, device: &mut Device| -> Result<(), Box<dyn std::error::Error>> {
        device.settle();
        let png = screenshot_frame(&mut device.ui, &device.info.with_elapsed(device.elapsed))?;
        shots.push((format!("online-{name}"), png));
        // The picture was drawn into a frame of its own, which set the
        // app up again: the device's frame is made anew to match.
        device.frame = Frame::for_app(&mut device.ui);
        device.draw();
        Ok(())
    };

    let server = world();
    lock(&server).say(
        "support-desk",
        "zawadi",
        "Zawadi Mollel",
        "Mzigo mpya umefika leo. Njooni muone kabla haujaisha.",
    );
    let mut device = Device::online(Screen::Home, *info, &server);
    shoot("home", &mut device)?;
    device.tap(("item", 0usize));
    device.ticks(2);
    device.fill("draft-field", "Asante, nakuja saa nne.");
    device.tap("send");
    device.ticks(2);
    shoot("room", &mut device)?;
    lock(&server).down = true;
    device.fill("draft-field", "Tutaonana!");
    lock(&server).down = false;
    lock(&server).fail = Some((
        "/messages",
        502,
        "upstream_failed",
        "Barua Talk did not answer.",
    ));
    device.tap("send");
    device.ticks(1);
    shoot("room-failed", &mut device)?;

    let server = world();
    lock(&server)
        .people
        .push(("guest-4f2a91".to_owned(), "guest-4f2a91".to_owned(), false));
    let mut device = Device::online(Screen::People, *info, &server);
    device.ticks(2);
    shoot("people", &mut device)?;
    device.tap(("person-video", 0usize));
    device.ticks(2);
    shoot("call-out", &mut device)?;
    lock(&server).call("call-1").status = "active";
    device.ticks(barua_talk::POLL_RINGING + 2);
    device.tap("record");
    device.ticks(2);
    shoot("call", &mut device)?;

    let server = world();
    let mut device = Device::online(Screen::Home, *info, &server);
    lock(&server).ring("c9", "baraka", "Baraka Mushi", true);
    device.ticks(barua_talk::POLL_INCOMING + 2);
    shoot("incoming", &mut device)?;

    let server = world();
    let mut device = Device::online(Screen::Dial, *info, &server);
    for key in "+255712345678".chars() {
        device.tap(("dial", key));
    }
    device.tap("dial-call");
    device.ticks(2);
    lock(&server).phones[0].status = "active";
    lock(&server).phones[0].seconds = 75;
    device.ticks(barua_talk::POLL_PHONE + 2);
    shoot("phone", &mut device)?;

    let server = world();
    let mut device = Device::online(Screen::Server, *info, &server);
    shoot("server", &mut device)?;
    Ok(shots)
}

/// Everything rendered, side by side, `columns` to a row.
fn sheet(renders: &[Vec<u8>], columns: usize) -> Result<Image, Box<dyn std::error::Error>> {
    let cells: Vec<Image> = renders
        .iter()
        .map(|png| Image::from_png(png))
        .collect::<Result<_, _>>()?;
    let Some(first) = cells.first() else {
        return Ok(Image::filled(1, 1, [58, 58, 60, 255]));
    };
    let (cell_width, cell_height, gap) = (first.width(), first.height(), 24);
    let rows = cells.len().div_ceil(columns) as u32;
    let width = columns as u32 * (cell_width + gap) + gap;
    let height = rows * (cell_height + gap) + gap;
    let mut sheet = Image::filled(width, height, [58, 58, 60, 255]);
    for (index, cell) in cells.iter().enumerate() {
        let left = gap + (index % columns) as u32 * (cell_width + gap);
        let top = gap + (index / columns) as u32 * (cell_height + gap);
        for y in 0..cell.height().min(cell_height) {
            for x in 0..cell.width().min(cell_width) {
                sheet.set_pixel(left + x, top + y, cell.pixel(x, y));
            }
        }
    }
    Ok(sheet)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut directory = None;
    let mut wanted: Vec<String> = Vec::new();
    let mut kind = "design";
    let (mut with_diff, mut with_states, mut with_sheet, mut with_semantics, mut with_online) =
        (false, false, false, false, false);
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--diff" => with_diff = true,
            "--states" => with_states = true,
            "--sheet" => with_sheet = true,
            "--semantics" => with_semantics = true,
            "--online" => with_online = true,
            "--phone" => kind = "phone",
            "--small" => kind = "small",
            "--tall" => kind = "tall",
            _ if directory.is_none() => directory = Some(PathBuf::from(argument)),
            _ => wanted.push(argument),
        }
    }
    let directory = directory.unwrap_or_else(|| PathBuf::from("out"));
    std::fs::create_dir_all(&directory)?;
    let info = frame(kind);
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut renders = Vec::new();

    if with_online {
        for (name, png) in online(&info)? {
            std::fs::write(directory.join(format!("{name}.png")), &png)?;
            println!("{name}.png");
            renders.push(png);
        }
    } else {
        let is_wanted = |name: &str| {
            wanted.is_empty()
                || wanted
                    .iter()
                    .any(|want| name == want || name.starts_with(&format!("{want}-")))
        };
        for screen in Screen::ALL {
            let name = screen.name();
            if !is_wanted(name) {
                continue;
            }
            let app = BaruaTalk::designed(name).unwrap_or_else(|| BaruaTalk::on(screen));
            let mut ui = Ui::new(app);
            let mut first = Frame::for_app(&mut ui);
            first.begin();
            ui.frame(&info, &mut first);
            let png = screenshot_frame(&mut ui, &info.with_elapsed(Duration::from_secs(1)))?;
            std::fs::write(directory.join(format!("{name}.png")), &png)?;
            println!("{name}.png  overflows: {:?}", ui.overflows());
            if with_semantics {
                ui.set_accessibility(true);
                let (semantic, _tree) =
                    screenshot_semantics(&mut ui, &info.with_elapsed(Duration::from_secs(2)))?;
                semantic.save(directory.join(format!("{name}.semantics.png")))?;
                for issue in ui.accessibility_issues() {
                    println!("  {issue}");
                }
            }
            if with_diff && DESIGNED.contains(&name) {
                let reference = Image::load(root.join(format!("design/reference/{name}.png")))?;
                let candidate = Image::from_png(&png)?;
                let comparison = compare(&reference, &candidate, &CompareOptions::default());
                println!("{}", comparison.report);
                comparison
                    .heat_map()
                    .save(directory.join(format!("{name}.diff.png")))?;
                comparison
                    .side_by_side()
                    .save(directory.join(format!("{name}.sbs.png")))?;
            }
            renders.push(png);
        }
        if with_states {
            for (name, screen, messages) in states() {
                if !is_wanted(name) && !wanted.is_empty() {
                    continue;
                }
                let mut ui = Ui::new(BaruaTalk::on(screen));
                let mut first = Frame::for_app(&mut ui);
                first.begin();
                ui.frame(&info, &mut first);
                for message in messages {
                    ui.send(message);
                }
                first.begin();
                ui.frame(&info.with_elapsed(Duration::from_millis(10)), &mut first);
                let png = screenshot_frame(&mut ui, &info.with_elapsed(Duration::from_secs(2)))?;
                std::fs::write(directory.join(format!("{name}.png")), &png)?;
                println!("{name}.png  overflows: {:?}", ui.overflows());
                renders.push(png);
            }
        }
    }
    if with_sheet {
        sheet(&renders, 6)?.save(directory.join("sheet.png"))?;
        println!("sheet.png");
    }
    Ok(())
}
