//! Renders Atlas's screens to PNG files on this machine's GPU, and
//! compares the designed ones with the design. No device needed.
//!
//!   cargo run -p atlas --example screenshot -- <output-dir> [screen ...]
//!       [--diff] [--phone | --small | --tall] [--states] [--online] [--sheet]
//!       [--semantics]
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
//! * `--online` renders, instead of the screens, the states the app has
//!   with its server behind it: home live and offline, a chat thinking,
//!   running through the server's steps, answered (with the answer's
//!   light formatting), failed with its "Retry", loading and being spoken;
//!   live voice recording, sending, heard and failed; the Library, Models
//!   and Server screens online. A scripted stand-in plays the server, the
//!   microphone and the speaker (`Ui::testing`): no network is used, and
//!   the answers are examples written here, not a model's.
//! * `--sheet` writes `sheet.png`: everything rendered, side by side.
//! * `--semantics` also writes `<name>.semantics.png` for each screen: the
//!   screen as a screen reader finds it (ADR 0032), and prints what
//!   `Ui::accessibility_issues` finds.

use std::path::{Path, PathBuf};
use std::time::Duration;

use atlas::{
    Atlas, Category, DESIGN_HEIGHT, DESIGN_WIDTH, DESIGNED, Flow, Message, Mode, Plan, Screen, Tab,
    Wire,
};
use morningstar::ui::Ui;
use morningstar::{
    App, Frame, FrameInfo, HostState, HttpError, HttpResponse, Insets, Recording, SurfaceSize,
};
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
        ("13-server", Screen::Server, Vec::new()),
    ]
}

// ------------------------------------------------- a stand-in for the server

fn json(body: impl Into<String>) -> Wire {
    Ok(HttpResponse::new(200)
        .with_header("Content-Type", "application/json")
        .with_body(body.into().into_bytes()))
}

const CHAT_ID: &str = "h_0000000000000000000000000000000a";
const QUESTION: &str = "Compare buying and leasing a car";
const SHORT_ANSWER: &str = "Leasing costs less each month; buying costs less in the end.\n\n- Buying: higher payments, and the car is yours\n- Leasing: lower payments, mileage limits, nothing at the end\n\nIf you keep a car longer than five years, buy it.";
const LONG_ANSWER: &str = "Over ten years buying usually wins.\n\n1. Add up ten years of lease payments\n2. Add up the loan, then subtract what the car is still worth\n3. Compare the two\n\nIn code, `total = monthly * months`:\n\n```python\ndef total_cost(monthly, months):\n    return monthly * months\n\nprint(total_cost(450, 120))\n```\n\nIf you drive little, leasing can still be the cheaper of the two.";

/// A JSON string literal of `text`.
fn quote(text: &str) -> String {
    format!(
        "\"{}\"",
        text.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
    )
}

/// The answer to a message, as `server/API.md` shows it.
fn reply(heard: &str, text: &str, used: &str, turn: u32, audio: bool) -> Wire {
    json(format!(
        r#"{{"chat_id": "{CHAT_ID}", "title": "Buying or leasing a car", "heard": {},
            "steps": [{{"label": "Reading the request", "state": "done"}},
                      {{"label": "Comparing the two", "state": "done"}},
                      {{"label": "Writing the answer", "state": "done"}}],
            "answer": {{"text": {}, "audio_url": {}}}, "model_used": "{used}", "turn": {turn}}}"#,
        quote(heard),
        quote(text),
        if audio {
            "\"/v1/audio/a_000000000000000000000001.mp3\""
        } else {
            "null"
        },
    ))
}

const CHATS: &str = r#"{"chats": [
    {"chat_id": "h_0000000000000000000000000000000c", "title": "A week in Lisbon", "updated_at": "2026-11-13T09:14:03Z", "messages": 3, "state": "answered"},
    {"chat_id": "h_0000000000000000000000000000000b", "title": "Reversing a string in Rust", "updated_at": "2026-11-12T17:02:44Z", "messages": 1, "state": "answered"},
    {"chat_id": "h_0000000000000000000000000000000a", "title": "Buying or leasing a car", "updated_at": "2026-11-12T08:30:10Z", "messages": 2, "state": "answered"}]}"#;

/// The app on home with a stand-in host behind it that has answered the
/// questions of start-up: online, with or without a voice and chats.
fn connected(voice: bool, chats: &str) -> Ui<Atlas> {
    let mut ui = Ui::testing(Atlas::on(Screen::Home(Flow::Standard)));
    // Runs `setup`, which asks the first question.
    let _ = Frame::for_app_with_host(
        &mut ui,
        &HostState::new().with_install_id("0123456789abcdef0123456789abcdef"),
    );
    ui.answer_http(json(format!(
        r#"{{"ok": true, "model": true, "voice": {voice}, "database": true}}"#
    )));
    ui.answer_http(json(r#"{"model": "auto", "plan": "free"}"#));
    ui.answer_http(json(chats));
    ui
}

fn ticks(ui: &mut Ui<Atlas>, count: usize) {
    for _ in 0..count {
        ui.send(Message::Tick);
    }
}

/// A chat whose first message is with the server.
fn asked() -> Ui<Atlas> {
    let mut ui = connected(false, r#"{"chats": []}"#);
    ui.send(Message::Draft(QUESTION.to_owned()));
    ui.send(Message::Send);
    ticks(&mut ui, 8);
    ui
}

/// The same chat, answered.
fn answered() -> Ui<Atlas> {
    let mut ui = asked();
    ui.answer_http(reply(QUESTION, SHORT_ANSWER, "smart", 1, false));
    ticks(&mut ui, 100);
    ui
}

/// Live voice on a server with a voice, the microphone open.
fn recording() -> Ui<Atlas> {
    let mut ui = connected(true, r#"{"chats": []}"#);
    ui.send(Message::OpenVoice);
    ui.answer_recording_started(Ok(()));
    ticks(&mut ui, 20);
    ui
}

/// The same, with the recording sent.
fn sent() -> Ui<Atlas> {
    let mut ui = recording();
    ui.send(Message::VoiceMute);
    ui.answer_recording(Ok(Recording {
        bytes: vec![0; 64],
        mime: "audio/mp4".to_owned(),
        duration: Duration::from_secs(3),
    }));
    ticks(&mut ui, 6);
    ui
}

/// The states the app has with a server behind it.
fn online_states() -> Vec<(&'static str, Ui<Atlas>)> {
    let mut states = vec![("03c-home-live", connected(true, CHATS))];

    let mut offline = Ui::testing(Atlas::on(Screen::Home(Flow::Standard)));
    let _ = Frame::for_app_with_host(&mut offline, &HostState::new());
    offline.answer_http(Err(HttpError::Offline));
    states.push(("03d-home-offline", offline));

    let mut thinking = asked();
    thinking.send(Message::ToggleSteps(0));
    states.push(("04e-chat-thinking", thinking));

    let mut working = asked();
    working.answer_http(reply(QUESTION, SHORT_ANSWER, "smart", 1, false));
    ticks(&mut working, 30);
    working.send(Message::ToggleSteps(0));
    states.push(("04f-chat-server-steps", working));

    states.push(("04g-chat-answered", answered()));

    let mut long = answered();
    long.send(Message::Draft(
        "And over ten years? Show me in code.".to_owned(),
    ));
    long.send(Message::Send);
    long.answer_http(reply(
        "And over ten years? Show me in code.",
        LONG_ANSWER,
        "code",
        2,
        false,
    ));
    ticks(&mut long, 100);
    states.push(("04h-chat-long-answer", long));

    let mut failed = answered();
    failed.send(Message::Draft("And over ten years?".to_owned()));
    failed.send(Message::Send);
    failed.answer_http(Ok(HttpResponse::new(502)));
    states.push(("04i-chat-failed", failed));

    let mut loading = connected(false, CHATS);
    loading.send(Message::Tab(Tab::Library));
    let oldest = loading.app().library()[0];
    loading.send(Message::OpenChat(oldest));
    states.push(("04j-chat-loading", loading));

    let mut confirm = answered();
    confirm.send(Message::DeleteChat);
    states.push(("04k-chat-delete-asked", confirm));

    states.push(("06c-voice-recording", recording()));
    states.push(("06d-voice-sending", sent()));

    let mut heard = sent();
    heard.answer_http(reply(
        "What is the capital of Tanzania?",
        "Dodoma is the capital of Tanzania. Dar es Salaam is its largest city.",
        "fast",
        1,
        true,
    ));
    states.push(("06e-voice-heard", heard));

    let mut spoken = sent();
    spoken.answer_http(reply(
        "What is the capital of Tanzania?",
        "Dodoma is the capital of Tanzania. Dar es Salaam is its largest city.",
        "fast",
        1,
        true,
    ));
    spoken.answer_http(Ok(HttpResponse::new(200)
        .with_header("Content-Type", "audio/mpeg")
        .with_body(vec![1, 2, 3])));
    ticks(&mut spoken, 120);
    states.push(("04l-chat-speaking", spoken));

    let mut unsent = sent();
    unsent.answer_http(Ok(HttpResponse::new(502)));
    states.push(("06f-voice-failed", unsent));

    let mut demo = connected(false, r#"{"chats": []}"#);
    demo.send(Message::OpenVoice);
    ticks(&mut demo, 40);
    states.push(("06g-voice-demo", demo));

    let mut library = connected(false, CHATS);
    library.send(Message::Tab(Tab::Library));
    states.push(("11b-library-online", library));

    let mut empty = connected(false, r#"{"chats": []}"#);
    empty.send(Message::Tab(Tab::Library));
    states.push(("11c-library-empty", empty));

    let mut models = connected(true, CHATS);
    models.send(Message::AllModels);
    states.push(("12b-models-online", models));

    let mut server = connected(true, CHATS);
    server.send(Message::OpenServer);
    states.push(("13b-server-online", server));

    let mut unreachable = Ui::testing(Atlas::on(Screen::Home(Flow::Standard)));
    let _ = Frame::for_app_with_host(&mut unreachable, &HostState::new());
    unreachable.answer_http(Err(HttpError::Offline));
    unreachable.send(Message::OpenServer);
    states.push(("13c-server-offline", unreachable));

    states
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
    let mut with_online = false;
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--semantics" => with_semantics = true,
            "--diff" => with_diff = true,
            "--states" => with_states = true,
            "--online" => with_online = true,
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
        // `--online` alone renders the online states and nothing else.
        if !named(name) || (with_online && wanted.is_empty() && !with_states) {
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
    if with_online {
        for (name, mut ui) in online_states() {
            if !named(name) {
                continue;
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
