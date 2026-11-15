//! Barua Talk for someone who cannot see it (ADR 0032): every screen
//! passes the framework's accessibility check, the controls that are only
//! a picture have names, and the app's main journey can be made with
//! nothing but what TalkBack offers: the semantics tree and its actions.
//!
//! Where the design itself falls short (its icon buttons are 44 dp, its
//! unread count is small white type on the accent), the designed pixels
//! are left as they are and the finding is listed here, with its reason,
//! and in `SCREENS.md`.

mod support;

use std::collections::BTreeSet;
use std::time::Duration;

use barua_talk::model::{CallStatus, Filter};
use barua_talk::{ANSWER_TICKS, BaruaTalk, Message, POLL_INCOMING, Screen};
use morningstar::ui::{AccessibilityIssue, Action, IssueKind, Role, SemanticsNode, States, Ui};
use morningstar::{App, Frame, FrameInfo, TextInputEvent};

use support::{design, galaxy_a27, lock, serve, world};

struct Reader {
    ui: Ui<BaruaTalk>,
    frame: Frame,
    info: FrameInfo,
    elapsed: Duration,
}

impl Reader {
    fn with(mut ui: Ui<BaruaTalk>, info: FrameInfo) -> Self {
        // What a host does when TalkBack is on.
        ui.set_accessibility(true);
        let frame = Frame::for_app(&mut ui);
        let mut reader = Self {
            ui,
            frame,
            info,
            elapsed: Duration::ZERO,
        };
        reader.draw();
        reader.wait(2);
        reader
    }

    fn on(screen: Screen, info: FrameInfo) -> Self {
        Self::with(Ui::new(BaruaTalk::on(screen)), info)
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
            self.elapsed += support::TICK;
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
/// The design draws its icon buttons 44 px across (Android asks for 48),
/// and enlarging them would move the designed pixels, which the fidelity
/// tests hold to the reference; so they are listed, not changed.
fn designed(issue: &AccessibilityIssue) -> Option<&'static str> {
    let label = issue.label.as_str();
    match issue.kind {
        IssueKind::SmallTarget => match label {
            "Back"
            | "Back to home"
            | "Back to chat"
            | "End call and go back"
            | "Leave call and go back" => Some("the design's arrow: width: 44px; height: 44px"),
            "Search" | "Close search" | "Dial a phone number" => {
                Some("the design's round buttons: width: 44px; height: 44px")
            }
            "Add a phone call to this room" | "Start video in this room" => {
                Some("the room's header buttons: width: 44px; height: 44px")
            }
            "Play voice message" | "Stop voice message" => {
                Some("the design's play button: width: 44px; height: 44px")
            }
            "Attach a file" => Some("the design's paperclip: width: 44px; height: 44px"),
            "Add someone to the call" => Some("the design's button: width: 44px; height: 44px"),
            "All" | "Rooms" | "Calls" => Some("the design's tabs: height: 44px"),
            _ if label.starts_with("All, ") => Some("the design's tabs: height: 44px"),
            _ => None,
        },
        _ => None,
    }
}

fn unexplained(reader: &Reader) -> Vec<String> {
    reader
        .ui
        .accessibility_issues()
        .iter()
        .filter(|issue| designed(issue).is_none())
        .map(|issue| format!("{}: {issue}", reader.screen().name()))
        .collect()
}

#[test]
fn every_screen_passes_the_check_but_for_the_designs_own_findings() {
    let mut findings = BTreeSet::new();
    for info in [galaxy_a27(), design()] {
        for screen in Screen::ALL {
            let reader = Reader::on(screen, info);
            findings.extend(unexplained(&reader));
        }
        // And the states a journey adds.
        let states: Vec<(Screen, Vec<Message>)> = vec![
            (Screen::Home, vec![Message::ToggleSearch]),
            (Screen::Home, vec![Message::OpenStart]),
            (Screen::Home, vec![Message::SetFilter(Filter::Calls)]),
            (
                Screen::Room,
                vec![Message::Draft("Habari".to_owned()), Message::Send],
            ),
            (Screen::Room, vec![Message::PlayVoice(0)]),
            (Screen::People, vec![Message::CallPerson(2, true)]),
            (
                Screen::VideoCall,
                vec![
                    Message::ToggleMute,
                    Message::ToggleCamera,
                    Message::ToggleRecording,
                ],
            ),
            (Screen::PhoneCall, vec![Message::ToggleKeypad]),
            (Screen::Dial, vec![Message::DialCall]),
            (Screen::NewRoom, vec![Message::CreateRoom]),
            (Screen::Welcome, vec![Message::Continue]),
        ];
        for (screen, messages) in states {
            let mut reader = Reader::on(screen, info);
            for message in messages {
                reader.ui.send(message);
            }
            reader.draw();
            reader.wait(2);
            findings.extend(unexplained(&reader));
        }
    }
    assert!(
        findings.is_empty(),
        "accessibility findings that are not the design's own:\n{}",
        findings.into_iter().collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn the_online_states_pass_with_nothing_allowed_that_is_the_apps_own() {
    let world = world();
    lock(&world).ring("c1", "baraka", "Baraka Mushi", true);
    let mut reader = Reader::with(serve(BaruaTalk::on(Screen::Home), &world), galaxy_a27());
    reader.ticks(6);
    assert!(reader.ui.app().link().is_online());
    let mut findings = BTreeSet::new();
    findings.extend(unexplained(&reader));
    reader.ticks(POLL_INCOMING + 3);
    assert_eq!(reader.screen(), Screen::Incoming);
    findings.extend(unexplained(&reader));
    reader.activate(Role::Button, "Accept call");
    reader.ticks(2);
    assert_eq!(reader.screen(), Screen::VideoCall);
    // The line that says what is missing is there to be read.
    assert!(
        reader
            .spoken()
            .iter()
            .any(|line| line.contains("Live audio and video need the next version")),
        "{:#?}",
        reader.spoken()
    );
    findings.extend(unexplained(&reader));
    reader.activate(Role::Button, "Hang up");
    reader.ticks(2);
    reader.activate(
        Role::Button,
        "Support desk, Baraka Mushi: Karibu!, Sasa, 1 unread",
    );
    reader.ticks(2);
    assert_eq!(reader.screen(), Screen::Room);
    findings.extend(unexplained(&reader));
    assert!(
        findings.is_empty(),
        "findings:\n{}",
        findings.into_iter().collect::<Vec<_>>().join("\n")
    );
}

// -------------------------------------------------------- what is spoken

#[test]
fn pictures_say_nothing_and_controls_have_names() {
    let reader = Reader::on(Screen::Home, galaxy_a27());
    let spoken = reader.spoken();
    println!("{spoken:#?}");
    for name in [
        "Search",
        "Dial a phone number",
        "Create a room",
        "Start a call or room",
        "Calls",
    ] {
        reader.node(Role::Button, name);
    }
    assert!(reader.node(Role::Tab, "Home").is(States::SELECTED));
    assert!(!reader.node(Role::Tab, "Contacts").is(States::SELECTED));
    assert!(reader.node(Role::Tab, "All, 6 unread").is(States::SELECTED));
    assert!(!reader.node(Role::Tab, "Rooms").is(States::SELECTED));
    // A row is one stop that says everything about it.
    reader.node(
        Role::Button,
        "Support desk, Zawadi: Picha ya duka jana usiku, 07:48, 4 unread",
    );
    reader.node(Role::Button, "Support desk, live");

    let reader = Reader::on(Screen::VideoCall, galaxy_a27());
    assert!(
        !reader
            .node(Role::Switch, "Mute microphone")
            .is(States::CHECKED)
    );
    assert!(reader.node(Role::Switch, "Camera").is(States::CHECKED));
    reader.node(Role::Button, "Stop recording");
    reader.node(Role::Slider, "Speaker volume");
    reader.node(Role::Button, "Hang up");

    let reader = Reader::on(Screen::Room, galaxy_a27());
    reader.node(Role::Button, "Play voice message");
    reader.node(Role::Button, "Send message");
    reader.node(Role::TextField, "Message");
    assert!(
        reader
            .spoken()
            .iter()
            .any(|line| line.contains("A small shop lit with neon at night")),
        "the photograph has its description"
    );
}

#[test]
fn the_journey_by_semantics_alone() {
    let mut reader = Reader::on(Screen::Home, galaxy_a27());
    // Into a room, a message, and back.
    reader.activate(Role::Button, "Support desk, live");
    assert_eq!(reader.screen(), Screen::Room);
    reader.type_into("Message", "Habari zenu");
    reader.activate(Role::Button, "Send message");
    assert!(
        reader
            .spoken()
            .iter()
            .any(|line| line.contains("Habari zenu")),
        "{:#?}",
        reader.spoken()
    );
    reader.activate(Role::Button, "Back to home");
    assert_eq!(reader.screen(), Screen::Home);

    // A call: made, answered, muted, turned up, ended.
    reader.activate(Role::Tab, "Contacts");
    assert_eq!(reader.screen(), Screen::People);
    let video = reader
        .ui
        .semantics()
        .nodes()
        .iter()
        .find(|node| node.label == "Video call")
        .expect("a way to call")
        .id;
    assert!(reader.ui.perform(video, Action::Activate));
    reader.draw();
    assert_eq!(reader.screen(), Screen::Incoming);
    reader.ticks(ANSWER_TICKS + 2);
    assert_eq!(reader.screen(), Screen::VideoCall);
    reader.activate(Role::Switch, "Mute microphone");
    assert!(
        reader
            .node(Role::Switch, "Mute microphone")
            .is(States::CHECKED)
    );
    let volume = reader.node(Role::Slider, "Speaker volume");
    assert!(reader.ui.perform(volume.id, Action::Increment));
    reader.draw();
    assert_eq!(reader.ui.app().call().unwrap().volume, 4);
    reader.activate(Role::Button, "Hang up");
    assert_eq!(reader.screen(), Screen::People);

    // A number, dialled.
    reader.activate(Role::Tab, "Home");
    reader.activate(Role::Button, "Dial a phone number");
    for key in [
        "Plus", "2", "5", "5", "7", "1", "2", "3", "4", "5", "6", "7", "8",
    ] {
        reader.activate(Role::Button, key);
    }
    reader.activate(Role::Button, "Call this number");
    assert_eq!(reader.screen(), Screen::PhoneCall);
    reader.activate(Role::Switch, "Speaker");
    assert!(reader.ui.app().phone().unwrap().speaker);
    reader.activate(Role::Button, "Hang up");
    assert_eq!(reader.screen(), Screen::Home);

    // An incoming call, accepted.
    reader.activate(
        Role::Button,
        "Baraka Mushi, Nipigie ukipata nafasi, 08:02, 2 unread",
    );
    assert_eq!(reader.screen(), Screen::Incoming);
    reader.activate(Role::Button, "Accept call");
    assert_eq!(reader.screen(), Screen::VideoCall);
    assert_eq!(reader.ui.app().call().unwrap().status, CallStatus::Active);
}
