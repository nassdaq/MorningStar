//! Renders CallReady's screens to PNG files on this machine's GPU, and
//! compares the designed ones with the design. No device needed.
//!
//!   cargo run -p callready --example screenshot -- <output-dir> [screen ...]
//!       [--diff] [--phone | --small | --tall] [--states] [--online] [--sheet]
//!
//! With no screen named, every screen is rendered; name them as the design
//! does (`02-home`) or by number (`02`). For each screen it writes
//! `<name>.png`, opened directly in the state the design shows it in.
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
//! * `--states` adds states that need a journey to reach: a call just
//!   connected, a call two answers in with the customer repeating, the
//!   report and transcript of that call, and the profile and menu with
//!   Premium active.
//! * `--online` renders, instead of the screens, the states the app has
//!   with a server behind it: home live and offline, a typed call
//!   connected, being answered, with the caller thinking, after a turn and
//!   after a failure, a spoken call with the caller speaking, ready,
//!   recording and with the microphone refused, a call that fell back to
//!   the script, the server's report (scored, not scored, from the
//!   history), its transcript, and history, progress, the menu and the
//!   Server screen online. A scripted stand-in plays the server, the
//!   microphone and the speaker (`Ui::testing`); nothing is sent anywhere.
//! * `--sheet` writes `sheet.png`: everything rendered, side by side.

use std::path::{Path, PathBuf};

use callready::model::Plan;
use callready::screens::{call_report, home, live_call, more, premium};
use callready::{Answer, CallReady, DESIGN_HEIGHT, DESIGN_WIDTH, Message, Screen, Tab};
use morningstar::ui::Ui;
use morningstar::{
    AudioError, Frame, FrameInfo, HostState, HttpError, HttpResponse, Insets, SurfaceSize,
};
use morningstar_devtools::{CompareOptions, Image, compare, screenshot_frame};

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
/// lead there from Home.
fn states() -> Vec<(&'static str, Vec<Message>)> {
    let connect = vec![Message::Home(home::Message::Scenario(0))];
    let mut answering = connect.clone();
    answering.extend(std::iter::repeat_n(Message::Tick, 47));
    answering.extend([
        Message::LiveCall(live_call::Message::Choose(1)),
        Message::LiveCall(live_call::Message::Choose(1)),
        Message::LiveCall(live_call::Message::Replay),
    ]);
    let mut report = answering.clone();
    report.extend([
        Message::LiveCall(live_call::Message::Choose(0)),
        Message::LiveCall(live_call::Message::Choose(1)),
    ]);
    let mut transcript = report.clone();
    transcript.push(Message::CallReport(call_report::Message::Transcript));
    let premium = vec![
        Message::Open(Screen::Premium),
        Message::Premium(premium::Message::Select(Plan::Premium)),
        Message::Premium(premium::Message::Start),
    ];
    let mut profile = premium.clone();
    profile.push(Message::Open(Screen::Profile));
    let mut more = premium.clone();
    more.push(Message::Open(Screen::More));
    let mut offer = premium.clone();
    offer.push(Message::Open(Screen::Premium));
    vec![
        ("03b-live-call-connected", connect),
        ("03c-live-call-answering", answering),
        ("04b-call-report-derived", report),
        ("10b-transcript-derived", transcript),
        ("05b-premium-active", offer),
        ("06b-profile-premium", profile),
        ("12b-more-premium", more),
    ]
}

// ------------------------------------------------- with a server behind it

const CALL: &str = "c_00000000000000000000000000000004";
const OLD: &str = "c_00000000000000000000000000000003";
const AUDIO: &str = "/v1/audio/a_000000000000000000000001.mp3";

fn json(body: String) -> Answer {
    Ok(HttpResponse::new(200)
        .with_header("Content-Type", "application/json")
        .with_body(body.into_bytes()))
}

fn profile(calls: u32) -> String {
    format!(
        r#"{{"name": "Asha Mrema", "accent": "us", "difficulty": "standard", "minutes": 4,
            "plan": "free", "calls": {calls}, "average_score": 79, "streak_days": 2}}"#
    )
}

fn history(with_new: bool) -> String {
    let new = format!(
        r#"{{"call_id": "{CALL}", "number": 4, "scenario": "angry_customer",
            "title": "Angry customer", "started_at": "2026-11-12T09:14:03Z",
            "duration_seconds": 187, "minutes": 4, "score": 82}},"#
    );
    format!(
        r#"{{"calls": [{}{{"call_id": "{OLD}", "number": 3, "scenario": "refund_request",
            "title": "Refund request", "started_at": "2026-11-11T16:02:40Z",
            "duration_seconds": 201, "minutes": 4, "score": 76}}]}}"#,
        if with_new { new.as_str() } else { "" }
    )
}

fn call_start(voice: bool) -> String {
    let audio = if voice {
        format!("\"{AUDIO}\"")
    } else {
        "null".to_owned()
    };
    format!(
        r#"{{"call_id": "{CALL}", "number": 4, "scenario": "angry_customer",
            "title": "Angry customer", "caller_name": "Dana",
            "goal": "Let the caller be heard, then fix it",
            "accent": "us", "difficulty": "standard", "minutes": 4, "voice": {voice},
            "line": {{"text": "This is the third time I’ve called today!", "audio_url": {audio}}},
            "tips": ["Take ownership", "Ask what happened"]}}"#
    )
}

fn turn() -> String {
    r#"{"heard": "I’m sorry you had to call again. I will stay with you until this is fixed.",
        "line": {"text": "Fine. But your app keeps logging me out, and I want it fixed today.", "audio_url": null},
        "tips": ["Ask what they see", "Give a clear next step"], "turn": 2, "done": false}"#
        .to_owned()
}

fn report(id: &str, scored: bool) -> String {
    let (score, skills) = if scored {
        (
            "82",
            r#"{"clarity": 88, "empathy": 74, "listening": 85, "problem_solving": 79, "professional_language": 84}"#,
        )
    } else {
        ("null", "null")
    };
    let (did_well, try_next, rest) = if scored {
        (
            "You took ownership early and kept a calm tone.",
            "Ask what the caller sees before you explain.",
            r#", {"speaker": "learner", "text": "I’m sorry you had to call again. I will stay with you until this is fixed.", "note": "Took ownership"},
               {"speaker": "caller", "text": "Fine. But your app keeps logging me out, and I want it fixed today."},
               {"speaker": "learner", "text": "It does work for most people, though.", "note": "Defended the product"},
               {"speaker": "caller", "text": "That is no help to me. Goodbye."}"#,
        )
    } else {
        (
            "You picked up the call.",
            "Say a first line: a greeting is enough to begin.",
            "",
        )
    };
    format!(
        r#"{{"call_id": "{id}", "number": 4, "scenario": "angry_customer",
            "title": "Angry customer", "caller_name": "Dana",
            "started_at": "2026-11-12T09:14:03Z", "duration_seconds": 187,
            "score": {score}, "skills": {skills},
            "did_well": "{did_well}", "try_next": "{try_next}",
            "transcript": [{{"speaker": "caller", "text": "This is the third time I’ve called today!"}}{rest}]}}"#
    )
}

/// The app on Home with a stand-in host behind it that has answered the
/// questions of start-up: online, with or without a voice.
fn connected(voice: bool) -> Ui<CallReady> {
    let mut ui = Ui::testing(CallReady::on(Screen::Home));
    // Runs `setup`, which asks the first question.
    let _ = Frame::for_app_with_host(
        &mut ui,
        &HostState::new().with_install_id("0123456789abcdef0123456789abcdef"),
    );
    ui.answer_http(json(format!(
        r#"{{"ok": true, "model": true, "voice": {voice}, "database": true}}"#
    )));
    ui.answer_http(json(profile(1)));
    ui.answer_http(json(history(false)));
    ui.answer_http(json(
        report(OLD, true).replace("Angry customer", "Refund request"),
    ));
    ui
}

/// A call played by the stand-in server, just connected.
fn in_call(voice: bool) -> Ui<CallReady> {
    let mut ui = connected(voice);
    ui.send(Message::Home(home::Message::Scenario(0)));
    ui.answer_http(json(call_start(voice)));
    for _ in 0..6 {
        ui.send(Message::Tick);
    }
    ui
}

fn live(message: live_call::Message) -> Message {
    Message::LiveCall(message)
}

const REPLY: &str =
    "I\u{2019}m sorry you had to call again. I will stay with you until this is fixed.";

/// The states the app has with a server behind it.
fn online_states() -> Vec<(&'static str, Ui<CallReady>)> {
    let mut states = vec![("02c-home-live", connected(false))];

    let mut offline = Ui::testing(CallReady::on(Screen::Home));
    let _ = Frame::for_app_with_host(&mut offline, &HostState::new());
    offline.answer_http(Err(HttpError::Offline));
    states.push(("02d-home-offline", offline));

    let mut ringing = connected(false);
    ringing.send(Message::Home(home::Message::Scenario(0)));
    states.push(("03d-typed-ringing", ringing));
    states.push(("03e-typed-connected", in_call(false)));

    let mut typing = in_call(false);
    typing.send(live(live_call::Message::Draft(REPLY.to_owned())));
    states.push(("03f-typed-answering", typing));

    let mut thinking = in_call(false);
    thinking.send(live(live_call::Message::Draft(REPLY.to_owned())));
    thinking.send(live(live_call::Message::Send));
    states.push(("03g-typed-thinking", thinking));

    let mut second = in_call(false);
    second.send(live(live_call::Message::Draft(REPLY.to_owned())));
    second.send(live(live_call::Message::Send));
    second.answer_http(json(turn()));
    for _ in 0..41 {
        second.send(Message::Tick);
    }
    states.push(("03h-typed-second-line", second));

    let mut failed = in_call(false);
    failed.send(live(live_call::Message::Draft(REPLY.to_owned())));
    failed.send(live(live_call::Message::Send));
    failed.answer_http(Err(HttpError::Offline));
    states.push(("03i-typed-failed-retry", failed));

    // A spoken call: the opening line's audio arrives and plays.
    let speaking = || {
        let mut ui = in_call(true);
        ui.answer_http(Ok(HttpResponse::new(200)
            .with_header("Content-Type", "audio/mpeg")
            .with_body(vec![0; 64])));
        ui
    };
    states.push(("03j-spoken-caller-speaking", speaking()));
    let ready = || {
        let mut ui = speaking();
        ui.answer_playback(Ok(()));
        ui
    };
    states.push(("03k-spoken-ready", ready()));
    let mut recording = ready();
    recording.send(live(live_call::Message::Talk));
    recording.answer_recording_started(Ok(()));
    for _ in 0..7 {
        recording.send(Message::Tick);
    }
    states.push(("03l-spoken-recording", recording));
    let mut denied = ready();
    denied.send(live(live_call::Message::Talk));
    denied.answer_recording_started(Err(AudioError::PermissionDenied));
    states.push(("03m-spoken-microphone-refused", denied));

    let mut fallback = connected(false);
    fallback.send(Message::Home(home::Message::Scenario(0)));
    fallback.answer_http(Err(HttpError::Offline));
    fallback.send(Message::Tick);
    states.push(("03n-fell-back-to-the-script", fallback));

    let ended = |scored: bool| {
        let mut ui = in_call(false);
        if scored {
            ui.send(live(live_call::Message::Draft(REPLY.to_owned())));
            ui.send(live(live_call::Message::Send));
            ui.answer_http(json(turn()));
        }
        ui.send(live(live_call::Message::EndCall));
        ui
    };
    states.push(("03o-ended-writing-the-report", ended(true)));
    let reported = |scored: bool| {
        let mut ui = ended(scored);
        ui.answer_http(json(report(CALL, scored)));
        if scored {
            ui.answer_http(json(profile(2)));
            ui.answer_http(json(history(true)));
        }
        ui
    };
    states.push(("04c-report-from-the-server", reported(true)));
    states.push(("04d-report-not-scored", reported(false)));
    let mut transcript = reported(true);
    transcript.send(Message::CallReport(call_report::Message::Transcript));
    states.push(("10c-transcript-from-the-server", transcript));

    let tab = |tab: Tab| {
        let mut ui = reported(true);
        ui.send(Message::Back);
        ui.send(Message::Home(home::Message::Tab(tab)));
        ui
    };
    states.push(("08b-history-online", tab(Tab::History)));
    states.push(("11b-progress-online", tab(Tab::Progress)));
    states.push(("12c-more-online", tab(Tab::More)));
    let mut server = tab(Tab::More);
    server.send(Message::More(more::Message::Server));
    states.push(("13b-server-connected", server));
    let mut unreachable = Ui::testing(CallReady::on(Screen::More));
    let _ = Frame::for_app_with_host(&mut unreachable, &HostState::new());
    unreachable.answer_http(Err(HttpError::Offline));
    unreachable.send(Message::More(more::Message::Server));
    states.push(("13c-server-unreachable", unreachable));
    states
}

fn render(mut ui: Ui<CallReady>, info: &FrameInfo) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
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
    let comparison = compare(&reference, &candidate, &CompareOptions::default());
    println!("{}", comparison.report);
    comparison
        .heat_map()
        .save(directory.join(format!("{}.diff.png", screen.name())))?;
    comparison
        .side_by_side()
        .save(directory.join(format!("{}.sbs.png", screen.name())))?;
    Ok(())
}

/// Every render side by side, six to a row, each 360 px wide.
fn sheet(renders: &[Vec<u8>]) -> Result<Image, Box<dyn std::error::Error>> {
    const COLUMNS: usize = 6;
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
    let rows = cells.len().div_ceil(COLUMNS) as u32;
    let width = GAP + COLUMNS as u32 * (CELL + GAP);
    let height = GAP + rows * (cell_height + GAP);
    let mut sheet = Image::filled(width, height, [58, 58, 60, 255]);
    for (index, cell) in cells.iter().enumerate() {
        let left = GAP + (index % COLUMNS) as u32 * (CELL + GAP);
        let top = GAP + (index / COLUMNS) as u32 * (cell_height + GAP);
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
    let mut screens = Vec::new();
    let mut kind = "design";
    let (mut with_diff, mut with_states, mut with_sheet) = (false, false, false);
    let mut with_online = false;
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--diff" => with_diff = true,
            "--states" => with_states = true,
            "--sheet" => with_sheet = true,
            "--online" => with_online = true,
            "--phone" => kind = "phone",
            "--small" => kind = "small",
            "--tall" => kind = "tall",
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
    if screens.is_empty() && !with_online {
        screens.extend(Screen::ALL);
    }
    let info = frame(kind);
    let mut renders = Vec::new();

    for screen in screens {
        let path = directory.join(format!("{}.png", screen.name()));
        let png = render(Ui::new(CallReady::on(screen)), &info)?;
        std::fs::write(&path, &png)?;
        println!("{}", path.display());
        if with_diff {
            if screen.is_designed() && kind == "design" {
                diff(screen, &png, &directory)?;
            } else {
                println!("  no comparison: not a designed screen in the design's frame");
            }
        }
        renders.push(png);
    }

    if with_states {
        for (name, messages) in states() {
            let mut ui = Ui::new(CallReady::on(Screen::Home));
            for message in messages {
                ui.send(message);
            }
            let path = directory.join(format!("{name}.png"));
            let png = render(ui, &info)?;
            std::fs::write(&path, &png)?;
            println!("{}", path.display());
            renders.push(png);
        }
    }

    if with_online {
        for (name, ui) in online_states() {
            let path = directory.join(format!("{name}.png"));
            let png = render(ui, &info)?;
            std::fs::write(&path, &png)?;
            println!("{}", path.display());
            renders.push(png);
        }
    }

    if with_sheet {
        let path = directory.join("sheet.png");
        sheet(&renders)?.save(&path)?;
        println!("{}", path.display());
    }
    Ok(())
}
