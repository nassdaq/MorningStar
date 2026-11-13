//! Atlas for someone who cannot see it (ADR 0032): every screen passes the
//! framework's accessibility check, the controls that are only a picture
//! have names, and the app's main journey can be made with nothing but
//! what TalkBack offers: the semantics tree and its actions.
//!
//! Where the design itself falls short (its icon buttons and chips are
//! 44 dp), the designed pixels are left as
//! they are and the finding is listed here, with its reason, and in
//! `SCREENS.md`.

use std::time::Duration;

use atlas::{Atlas, Flow, Message, Mode, Phase, Plan, Screen, VOICE_LINE};
use morningstar::ui::{AccessibilityIssue, Action, IssueKind, Role, SemanticsNode, States, Ui};
use morningstar::{
    App, Frame, FrameInfo, HttpError, HttpResponse, Insets, Recording, SurfaceSize, TextInputEvent,
};

/// The owner's phone: 384 x 832 dp at 2.8125, with its bars.
fn galaxy_a27() -> FrameInfo {
    FrameInfo::new(SurfaceSize::new(1080, 2340))
        .with_scale(2.8125)
        .with_insets(Insets::new(0.0, 102.0, 0.0, 42.0))
}

/// The design's artboard: 390 x 844 dp.
fn design() -> FrameInfo {
    FrameInfo::new(SurfaceSize::new(780, 1688)).with_scale(2.0)
}

const HOME: Screen = Screen::Home(Flow::Standard);
const CHAT: Screen = Screen::Chat(Flow::Standard);

struct Device {
    ui: Ui<Atlas>,
    frame: Frame,
    info: FrameInfo,
    elapsed: Duration,
}

impl Device {
    fn with(app: Atlas, info: FrameInfo) -> Self {
        let mut ui = Ui::testing(app);
        // What a host does when TalkBack is on.
        ui.set_accessibility(true);
        let frame = Frame::for_app(&mut ui);
        let mut device = Self {
            ui,
            frame,
            info,
            elapsed: Duration::ZERO,
        };
        device.draw();
        device.wait(2);
        device
    }

    fn on(screen: Screen, info: FrameInfo) -> Self {
        Self::with(Atlas::on(screen), info)
    }

    fn draw(&mut self) {
        self.frame.begin();
        let info = self.info.with_elapsed(self.elapsed);
        self.ui.frame(&info, &mut self.frame);
    }

    /// Lets `frames` frames go by, 400 ms apart: transitions end.
    fn wait(&mut self, frames: u32) {
        for _ in 0..frames {
            self.elapsed += Duration::from_millis(400);
            self.draw();
        }
    }

    /// Lets `count` ticks of the app's timer pass.
    fn ticks(&mut self, count: u32) {
        for _ in 0..count {
            self.elapsed += Duration::from_millis(34);
            self.draw();
        }
    }

    fn screen(&self) -> Screen {
        self.ui.app().screen()
    }

    fn spoken(&self) -> Vec<String> {
        self.ui.semantics().spoken()
    }

    /// The node with this role and label, as a screen reader's user would
    /// look for it.
    fn node(&self, role: Role, label: &str) -> SemanticsNode {
        self.ui
            .semantics()
            .find(role, label)
            .unwrap_or_else(|| panic!("no {role:?} called {label:?} among {:#?}", self.spoken()))
            .clone()
    }

    /// A double tap on the control with this role and label.
    fn activate(&mut self, role: Role, label: &str) {
        let node = self.node(role, label);
        assert!(
            self.ui.perform(node.id, Action::Activate),
            "{label:?} cannot be activated"
        );
        self.draw();
        self.wait(2);
    }

    /// Focuses the field with this label and types into it.
    fn type_into(&mut self, label: &str, text: &str) {
        self.activate(Role::TextField, label);
        for character in text.chars() {
            self.ui
                .text_input(&TextInputEvent::Commit(character.to_string()));
            self.draw();
        }
    }
}

// ------------------------------------------------------------- the check

/// Why a finding is the design's own, or `None` if it is ours to fix.
///
/// The design draws its icon buttons, its chips, its model chip and its
/// "Forgot password?" link 44 px tall, its second button 46, its
/// attachment chip 32 and its last line 18. Android asks for 48 dp. Enlarging them would move the designed
/// pixels, which the fidelity tests hold to the reference; so they are
/// listed, not changed. Nothing else is found: every text of the design
/// has the contrast the checker asks for.
fn designed(issue: &AccessibilityIssue) -> Option<&'static str> {
    let label = issue.label.as_str();
    match issue.kind {
        IssueKind::SmallTarget => match label {
            "Back" | "Minimise voice" => Some("the design's arrows: width: 44px; height: 44px"),
            "Profile and settings" => Some("the design's profile button: 44 px and a 1 px border"),
            "Add a photo, file or camera shot" | "Start voice conversation" | "Send" => {
                Some("the composer's buttons: width: 44px; height: 44px")
            }
            "Forgot password?" => Some("the design's link: min-height: 44px"),
            "I already have an account" => Some("the design's second button: height: 46px"),
            "New to Atlas? Create an account" => {
                Some("the design's last line: a 14 px sentence with a link in it")
            }
            "Cancel task" => Some("the design's button: height: 44px"),
            "All" | "Writing" | "Research" | "Coding" | "Education" => {
                Some("the design's chips: height: 44px")
            }
            _ if label.starts_with("Model: ") => Some("the design's model chip: height: 44px"),
            _ if label.starts_with("Attachment: ") => {
                Some("the design's attachment chip: height: 32px")
            }
            _ if label.ends_with("remove") => Some("the attachment chip again, before sending"),
            _ => None,
        },
        _ => None,
    }
}

fn unexplained(device: &Device) -> Vec<String> {
    device
        .ui
        .accessibility_issues()
        .iter()
        .filter(|issue| designed(issue).is_none())
        .map(ToString::to_string)
        .collect()
}

#[test]
fn every_screen_passes_the_check_but_for_the_designs_own_findings() {
    let mut findings = std::collections::BTreeSet::new();
    for info in [galaxy_a27(), design()] {
        for screen in Screen::ALL {
            let device = Device::on(screen, info);
            findings.extend(unexplained(&device));
        }
        // And the states a journey adds.
        let states: [(Screen, Vec<Message>); 7] = [
            (Screen::SignIn, vec![Message::SignIn]),
            (HOME, vec![Message::Attach, Message::Draft("Hello".into())]),
            (HOME, vec![Message::OpenPicker]),
            (CHAT, vec![Message::ToggleSteps(0)]),
            (HOME, vec![Message::Suggest(0)]),
            (Screen::Plans, vec![Message::Choose(Plan::Pro)]),
            (Screen::Explore, vec![Message::Query("zzz".into())]),
        ];
        for (screen, messages) in states {
            let mut device = Device::on(screen, info);
            for message in messages {
                device.ui.send(message);
            }
            device.draw();
            device.wait(2);
            findings.extend(unexplained(&device));
        }
    }
    assert!(findings.is_empty(), "{findings:#?}");
}

#[test]
fn every_allowed_finding_is_still_found() {
    // An allowance nothing needs any more should go.
    let mut found = std::collections::BTreeSet::new();
    for screen in Screen::ALL {
        let mut device = Device::on(screen, design());
        let mut note = |device: &Device| {
            for issue in device.ui.accessibility_issues() {
                if let Some(reason) = designed(&issue) {
                    found.insert(reason);
                }
            }
        };
        note(&device);
        if screen == HOME {
            device.ui.send(Message::Attach);
            device.ui.send(Message::Draft("Hello".into()));
            device.draw();
            note(&device);
        }
        if screen == CHAT {
            device.ui.send(Message::ToggleSteps(0));
            device.draw();
            note(&device);
        }
    }
    println!("{found:#?}");
    assert_eq!(found.len(), 11, "{found:#?}");
}

// -------------------------------------------------------- what is spoken

#[test]
fn pictures_say_nothing_and_controls_have_names() {
    let device = Device::on(HOME, galaxy_a27());
    let spoken = device.spoken();
    println!("{spoken:#?}");
    // The orb is decoration; the name is said once.
    assert_eq!(
        spoken
            .iter()
            .filter(|line| line.starts_with("Atlas"))
            .count(),
        1
    );
    for (role, label) in [
        (Role::Button, "Profile and settings"),
        (Role::Heading, "What can I help with?"),
        (Role::Button, "Summarise a PDF"),
        (Role::Button, "Model: Auto"),
        (Role::Button, "Add a photo, file or camera shot"),
        (Role::TextField, "Message Atlas"),
        (Role::Button, "Start voice conversation"),
        (Role::Tab, "Explore"),
    ] {
        device.node(role, label);
    }
    assert!(device.node(Role::Tab, "Home").is(States::SELECTED));
    assert!(!device.node(Role::Tab, "Library").is(States::SELECTED));

    // The picker: four choices, one of them made.
    let mut device = Device::on(HOME, galaxy_a27());
    device.activate(Role::Button, "Model: Auto");
    let auto = device.node(Role::Radio, "Auto, Picks the best fit for each request");
    assert!(auto.is(States::CHECKED));
    device.node(Role::Button, "Close model picker");

    // The password is never read out.
    let mut device = Device::on(Screen::SignIn, galaxy_a27());
    device.type_into("Password", "secret");
    assert!(device.spoken().iter().all(|line| !line.contains("secret")));
}

// ------------------------------------------------------------ the journey

/// Sign in, pick a model, ask, hear the answer arrive, speak, and choose a
/// plan: by the semantics tree and its actions alone.
#[test]
fn the_journey_by_semantics_alone() {
    let mut device = Device::with(Atlas::default(), galaxy_a27());
    device.node(Role::Heading, "Every AI, in one place");
    device.activate(Role::Button, "Get started");
    assert_eq!(device.screen(), Screen::SignIn);
    device.type_into("Email", "amina@example.com");
    device.type_into("Password", "secret");
    device.activate(Role::Button, "Sign in");
    assert_eq!(device.ui.app().stack(), [HOME]);

    device.activate(Role::Button, "Model: Auto");
    device.activate(Role::Radio, "Fast, Quick everyday answers, lowest cost");
    assert_eq!(device.ui.app().mode(), Mode::Fast);
    device.node(Role::Button, "Model: Fast");

    device.type_into("Message Atlas", "Compare these quotes");
    device.activate(Role::Button, "Send");
    assert_eq!(device.screen(), CHAT);
    device.ticks(150);
    let turn = &device.ui.app().conversation().turns[0];
    assert_eq!(turn.phase, Phase::Done);
    let reply = turn.reply.clone().unwrap();
    assert!(
        device.spoken().iter().any(|line| line.contains(&reply)),
        "the answer is in the tree: {:#?}",
        device.spoken()
    );

    device.activate(Role::Button, "Start voice conversation");
    assert_eq!(device.screen(), Screen::LiveVoice);
    let mute = device.node(Role::Switch, "Mute microphone");
    assert!(!mute.is(States::CHECKED));
    device.ticks(30);
    device.activate(Role::Button, "End voice and view transcript");
    assert_eq!(device.screen(), CHAT);
    device.ticks(150);
    let said = &device.ui.app().conversation().turns[1].prompt;
    assert!(VOICE_LINE.starts_with(said.as_str()));

    device.activate(Role::Button, "Back");
    device.activate(Role::Button, "Profile and settings");
    assert_eq!(device.screen(), Screen::Plans);
    device.activate(Role::Button, "Get Plus");
    assert_eq!(device.ui.app().plan(), Some(Plan::Plus));
    assert!(
        device
            .ui
            .announcements()
            .iter()
            .any(|line| line.contains("Plus")),
        "the toast is announced"
    );
}

// ------------------------------------------------------ with a server

/// A server's JSON answer.
fn json(body: &str) -> Result<HttpResponse, HttpError> {
    Ok(HttpResponse::new(200)
        .with_header("Content-Type", "application/json")
        .with_body(body.as_bytes().to_vec()))
}

const REPLY: &str = r#"{"chat_id": "h_0000000000000000000000000000000a", "title": "Buying or leasing",
    "heard": "Compare buying and leasing a car",
    "steps": [{"label": "Reading the request", "state": "done"}, {"label": "Writing the answer", "state": "done"}],
    "answer": {"text": "Buying wins over ten years.\n\n- Leases never end\n- `total = monthly * 120`\n\n1. Add it up\n\n```\nprint(450 * 120)\n```", "audio_url": "/v1/audio/a_000000000000000000000001.mp3"},
    "model_used": "smart", "turn": 1}"#;

/// The app online and on home: the three questions of start-up answered.
fn online(info: FrameInfo, voice: bool) -> Device {
    let mut device = Device::with(Atlas::default(), info);
    let health = format!(r#"{{"ok": true, "model": true, "voice": {voice}, "database": true}}"#);
    for body in [
        health.as_str(),
        r#"{"model": "auto", "plan": "free"}"#,
        r#"{"chats": [{"chat_id": "h_0000000000000000000000000000000b", "title": "A week in Lisbon", "updated_at": "2026-11-13T09:14:03Z", "messages": 1, "state": "answered"}]}"#,
    ] {
        assert!(device.ui.answer_http(json(body)));
        device.draw();
    }
    assert!(device.ui.app().link().is_online());
    device.ui.send(Message::Google);
    device.draw();
    device.wait(2);
    device
}

/// Lets the screen settle, finds each expected control as a screen reader
/// would, and notes what the check finds.
fn look(
    findings: &mut std::collections::BTreeSet<String>,
    device: &mut Device,
    expected: &[(Role, &str)],
) {
    device.draw();
    device.wait(2);
    for (role, label) in expected {
        device.node(*role, label);
    }
    findings.extend(unexplained(device));
}

/// What only exists with a server behind the app: named for a screen
/// reader, and passing the same check as everything else (these are the
/// app's own controls, so none of them needs the design's allowance).
#[test]
fn the_online_controls_have_names_and_pass_the_check() {
    for info in [galaxy_a27(), design()] {
        let mut findings = std::collections::BTreeSet::new();

        // Home: the mark that says answers are live.
        let mut device = online(info, true);
        look(
            &mut findings,
            &mut device,
            &[(Role::Button, "Answers are live. Opens the server screen")],
        );

        // A message the server does not answer, then one it does.
        device
            .ui
            .send(Message::Draft("Compare buying and leasing a car".into()));
        device.ui.send(Message::Send);
        look(&mut findings, &mut device, &[(Role::Button, "Thinking")]);
        assert!(device.ui.answer_http(Ok(HttpResponse::new(502))));
        look(
            &mut findings,
            &mut device,
            &[
                (Role::Button, "No answer"),
                (Role::Button, "Retry: send the message again"),
            ],
        );
        assert!(
            device
                .ui
                .announcements()
                .iter()
                .any(|line| line == "Atlas could not answer just now."),
            "the failure is announced"
        );
        device.activate(Role::Button, "Retry: send the message again");
        assert!(device.ui.answer_http(json(REPLY)));
        device.ticks(80);
        assert_eq!(device.ui.app().conversation().turns[0].phase, Phase::Done);
        look(
            &mut findings,
            &mut device,
            &[
                (Role::Heading, "Buying or leasing"),
                (Role::Button, "Delete this chat"),
            ],
        );
        let spoken = device.spoken().join("\n");
        for part in [
            "Buying wins over ten years.",
            "Leases never end",
            "Add it up",
            "print(450 * 120)",
            "Answered by Smart",
        ] {
            assert!(spoken.contains(part), "{part:?} is in the tree: {spoken}");
        }
        device.activate(Role::Button, "Delete this chat");
        look(
            &mut findings,
            &mut device,
            &[(Role::Button, "Tap again to delete it for good")],
        );

        // The Library, the Models screen and the Server screen, online.
        for (screen, expected) in [
            (
                Screen::Library,
                vec![(Role::Button, "A week in Lisbon, 1 message \u{b7} answered")],
            ),
            (
                Screen::Models,
                vec![(
                    Role::Button,
                    "Server, Connected. Answers are live, and so is voice.",
                )],
            ),
        ] {
            let mut device = online(info, true);
            device.ui.send(match screen {
                Screen::Library => Message::Tab(atlas::Tab::Library),
                _ => Message::AllModels,
            });
            look(&mut findings, &mut device, &expected);
            if screen == Screen::Models {
                device.activate(
                    Role::Button,
                    "Server, Connected. Answers are live, and so is voice.",
                );
                assert_eq!(device.screen(), Screen::Server);
                look(
                    &mut findings,
                    &mut device,
                    &[
                        (Role::Heading, "Server"),
                        (Role::TextField, "Server address"),
                        (Role::Button, "Check connection"),
                        (Role::Button, "Use the default address"),
                    ],
                );
            }
        }

        // Live voice, for real: recording, failed, heard, and the spoken answer.
        let mut device = online(info, true);
        device.activate(Role::Button, "Start voice conversation");
        assert!(device.ui.answer_recording_started(Ok(())));
        look(
            &mut findings,
            &mut device,
            &[(Role::Button, "Send what you said")],
        );
        device.activate(Role::Button, "Send what you said");
        assert!(device.ui.answer_recording(Ok(Recording {
            bytes: vec![1, 2, 3],
            mime: "audio/mp4".into(),
            duration: Duration::from_secs(2),
        })));
        device.draw();
        assert!(device.ui.answer_http(Ok(HttpResponse::new(502))));
        look(
            &mut findings,
            &mut device,
            &[(Role::Button, "Record again"), (Role::Button, "Send again")],
        );
        device.activate(Role::Button, "Send again");
        assert!(device.ui.answer_http(json(REPLY)));
        device.draw();
        assert!(
            device
                .spoken()
                .iter()
                .any(|line| line.contains("Compare buying and leasing a car")),
            "what was heard is in the tree: {:#?}",
            device.spoken()
        );
        findings.extend(unexplained(&device));
        // The audio arrives; the chat opens; the answer is spoken.
        assert!(
            device.ui.answer_http(Ok(HttpResponse::new(200)
                .with_header("Content-Type", "audio/mpeg")
                .with_body(vec![1, 2, 3])))
        );
        device.ticks(60);
        assert_eq!(device.screen(), CHAT);
        look(
            &mut findings,
            &mut device,
            &[(Role::Button, "Stop speaking")],
        );
        device.activate(Role::Button, "Stop speaking");
        assert_eq!(device.ui.app().speaking(), None);

        assert!(findings.is_empty(), "{findings:#?}");
    }
}
