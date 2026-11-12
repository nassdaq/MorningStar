//! CallReady for someone who cannot see it (ADR 0032): every screen
//! passes the framework's accessibility check, the controls that are only
//! a picture have names, and the app's main journey can be made with
//! nothing but what TalkBack offers: the semantics tree and its actions.
//!
//! Where the design itself falls short (its touch targets are 44 dp, its
//! tip chips 34), the designed pixels are left as they are and the finding
//! is listed here, with its reason, and in `SCREENS.md`.

use std::time::Duration;

use callready::script::Scenario;
use callready::{CallReady, Message, Screen, Tab};
use morningstar::ui::{
    AccessibilityIssue, Action, Actions, IssueKind, Live, Role, SemanticsNode, States, Ui,
};
use morningstar::{App, Frame, FrameInfo, Insets, SurfaceSize};

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

struct Device {
    ui: Ui<CallReady>,
    frame: Frame,
    info: FrameInfo,
    elapsed: Duration,
}

impl Device {
    fn with(app: CallReady, info: FrameInfo) -> Self {
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
        device
    }

    fn on(screen: Screen, info: FrameInfo) -> Self {
        Self::with(CallReady::on(screen), info)
    }

    fn draw(&mut self) {
        self.frame.begin();
        let info = self.info.with_elapsed(self.elapsed);
        self.ui.frame(&info, &mut self.frame);
    }

    fn wait(&mut self, seconds: u32) {
        for _ in 0..seconds {
            self.elapsed += Duration::from_secs(1);
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
    }
}

// ------------------------------------------------------------- the check

/// Why a finding is the design's own, or `None` if it is ours to fix.
///
/// The design draws every icon-only control in a 44 px box, its profile
/// link 44 px tall and its tip chips 34 px tall. Android asks for 48 dp.
/// Enlarging them would move the designed pixels, which the fidelity
/// tests hold to the reference; so they are listed, not changed.
fn designed(issue: &AccessibilityIssue, node: Option<&SemanticsNode>) -> Option<&'static str> {
    if issue.kind != IssueKind::SmallTarget {
        return None;
    }
    let label = issue.label.as_str();
    match label {
        "Back" => Some("the design's back arrow: width: 44px; height: 44px"),
        "Close" => Some("the design's close button: 44 x 44 px"),
        "Notifications" | "Notifications, unread" => Some("the design's bell: 44 x 44 px"),
        "Scenario settings" => Some("the design's gear: 44 x 44 px"),
        "Hear the customer again" => Some("the design's loudspeaker: 44 x 44 px"),
        _ if label.starts_with("Profile, ") => {
            Some("the design's profile link: min-height: 44px (46 with its avatar)")
        }
        // The tip chips of the live call: `padding: 6px 12px`, 34 px tall.
        _ if node.is_some_and(|node| node.hint == "Answers with this") => {
            Some("the design's tip chips: 34 px tall")
        }
        _ => None,
    }
}

/// The findings of the frame that are not the design's own.
fn ours(device: &Device) -> Vec<String> {
    let tree = device.ui.semantics();
    device
        .ui
        .accessibility_issues()
        .iter()
        .filter(|issue| designed(issue, tree.node(issue.id)).is_none())
        .map(ToString::to_string)
        .collect()
}

#[test]
fn every_screen_passes_the_accessibility_check() {
    let mut allowed = std::collections::BTreeSet::new();
    for (phone, info) in [("galaxy a27", galaxy_a27()), ("design", design())] {
        for screen in Screen::ALL {
            let device = Device::on(screen, info);
            assert_eq!(
                ours(&device),
                Vec::<String>::new(),
                "{} on {phone}",
                screen.name()
            );
            let tree = device.ui.semantics();
            for issue in device.ui.accessibility_issues() {
                if let Some(reason) = designed(&issue, tree.node(issue.id)) {
                    allowed.insert(reason);
                }
            }
        }
    }
    // Every reason in the list is still met: none has gone stale.
    assert_eq!(allowed.len(), 7, "the design findings found: {allowed:#?}");
}

/// The states a screen only reaches on the way: a call in progress, muted,
/// and the offer with the other plan chosen.
#[test]
fn the_states_between_pass_too() {
    let mut device = Device::with(CallReady::default(), galaxy_a27());
    device.activate(Role::Button, "Let\u{2019}s go");
    device.activate(Role::Button, "Start call");
    assert_eq!(ours(&device), Vec::<String>::new(), "a call in progress");
    device.wait(3);
    device.activate(Role::Switch, "Mute");
    assert_eq!(ours(&device), Vec::<String>::new(), "muted");

    let mut device = Device::on(Screen::Premium, galaxy_a27());
    device.activate(Role::Button, "Free, $0, 3 intro calls");
    assert_eq!(ours(&device), Vec::<String>::new(), "the free plan chosen");
}

// ------------------------------------------------- what a screen says

#[test]
fn home_reads_as_a_screen_not_as_a_picture() {
    let device = Device::with(CallReady::default(), galaxy_a27());
    // The collage of the welcome screen is one sentence.
    assert_eq!(
        device.spoken()[0],
        "Get job-ready with real calls",
        "{:#?}",
        device.spoken()
    );
    let mut device = device;
    device.activate(Role::Button, "Let\u{2019}s go");
    assert_eq!(
        device.spoken(),
        [
            "Profile, Grace",
            "Notifications, unread",
            "Today\u{2019}s practice call",
            "Missing delivery",
            "Customer \u{b7} US accent \u{b7} 4 min",
            "Start call",
            "Scenarios",
            "Scenario settings",
            "Angry customer",
            "Refund request",
            "Billing problem",
            "Home",
            "Progress",
            "History",
            "More",
        ]
    );
    let tree = device.ui.semantics();
    // The pictures (the disc, the rings, the cards' numerals) say nothing;
    // the icon-only controls have names; the headings can be jumped to.
    assert_eq!(tree.find_by_label("Scenarios").unwrap().role, Role::Heading);
    assert_eq!(
        tree.find_by_label("Angry customer").unwrap().hint,
        "Starts a practice call"
    );
    // The tab bar: four tabs, the current one selected.
    let tabs: Vec<(&str, bool)> = tree
        .with_role(Role::Tab)
        .map(|tab| (tab.label.as_str(), tab.is(States::SELECTED)))
        .collect();
    assert_eq!(
        tabs,
        [
            ("Home", true),
            ("Progress", false),
            ("History", false),
            ("More", false)
        ]
    );
    for tab in Tab::ALL {
        assert!(tree.find(Role::Tab, tab.title()).is_some());
    }
}

#[test]
fn chosen_things_say_that_they_are_chosen() {
    // The plan cards: the badge for the eye, the state for the ear.
    let mut device = Device::on(Screen::Premium, galaxy_a27());
    assert!(
        device
            .ui
            .semantics()
            .find(Role::Heading, "CallReady Pro")
            .is_some()
    );
    let premium = device.node(Role::Button, "Premium, $4.99, per month");
    assert!(premium.is(States::SELECTED));
    assert!(
        !device
            .node(Role::Button, "Free, $0, 3 intro calls")
            .is(States::SELECTED)
    );
    device.activate(Role::Button, "Free, $0, 3 intro calls");
    assert!(
        device
            .node(Role::Button, "Free, $0, 3 intro calls")
            .is(States::SELECTED)
    );
    assert!(
        !device
            .node(Role::Button, "Premium, $4.99, per month")
            .is(States::SELECTED)
    );

    // The settings' chips.
    let mut device = Device::on(Screen::Settings, galaxy_a27());
    assert!(device.node(Role::Button, "US").is(States::SELECTED));
    assert!(!device.node(Role::Button, "UK").is(States::SELECTED));
    device.activate(Role::Button, "UK");
    assert!(device.node(Role::Button, "UK").is(States::SELECTED));
    assert!(!device.node(Role::Button, "US").is(States::SELECTED));
}

#[test]
fn the_report_says_its_scores() {
    let device = Device::on(Screen::CallReport, galaxy_a27());
    let tree = device.ui.semantics();
    assert_eq!(
        tree.find_by_label("Call report").unwrap().role,
        Role::Heading
    );
    // Two turned plates and "/100" are one number.
    assert!(
        tree.find_by_label("82 out of 100").is_some(),
        "{:#?}",
        device.spoken()
    );
    let skills: Vec<String> = tree
        .with_role(Role::Progress)
        .map(SemanticsNode::spoken)
        .collect();
    assert_eq!(
        skills,
        [
            "Clarity, 88 out of 100",
            "Empathy, 74 out of 100",
            "Listening, 85 out of 100",
            "Problem solving, 79 out of 100",
            "Professional language, 84 out of 100",
        ]
    );
}

// --------------------------------------------------------------- journey

/// Welcome, home, today's call, an answer, the red button, the report:
/// with nothing but lookups by role and label and `perform`, which is all
/// a TalkBack user has.
#[test]
fn the_main_journey_by_screen_reader_alone() {
    let mut device = Device::with(CallReady::default(), galaxy_a27());
    assert_eq!(device.screen(), Screen::Welcome);
    device.activate(Role::Button, "Let\u{2019}s go");
    assert_eq!(device.screen(), Screen::Home);
    assert!(device.node(Role::Tab, "Home").is(States::SELECTED));

    // Today's call.
    device.activate(Role::Button, "Start call");
    assert_eq!(device.screen(), Screen::LiveCall);
    assert_eq!(device.ui.app().call().scenario, Scenario::MissingDelivery);
    let tree = device.ui.semantics();
    assert!(
        tree.find_by_label("Call number 4 \u{b7} Missing delivery")
            .is_some(),
        "{:#?}",
        device.spoken()
    );
    // The caller's line and the call's status are live regions: TalkBack
    // reads them out when they change.
    let line = |device: &Device| {
        device
            .ui
            .semantics()
            .nodes()
            .iter()
            .find(|node| node.live() == Live::Polite && node.label.starts_with("CUSTOMER"))
            .expect("the caller's line is a live region")
            .clone()
    };
    let first = line(&device);
    let status = device
        .ui
        .semantics()
        .find_by_label("YOU \u{b7} SPEAKING")
        .expect("the status")
        .clone();
    assert_eq!(status.live(), Live::Polite);

    // Answer with a tip: the two chips are buttons that say so.
    device.wait(4);
    let tips: Vec<SemanticsNode> = device
        .ui
        .semantics()
        .with_role(Role::Button)
        .filter(|node| node.hint == "Answers with this")
        .cloned()
        .collect();
    assert_eq!(tips.len(), 2);
    assert!(tips.iter().all(|tip| tip.can(Actions::ACTIVATE)));
    let good = device
        .ui
        .app()
        .call()
        .exchange()
        .expect("a line is waiting")
        .good_option();
    assert!(device.ui.perform(tips[good].id, Action::Activate));
    device.draw();
    assert_eq!(device.ui.app().call().choices.len(), 1);
    // The same node, saying the caller's next line: what makes a live
    // region speak.
    let second = line(&device);
    assert_eq!(second.id, first.id);
    assert_ne!(second.label, first.label);

    // Muting is a switch that says which way it is.
    assert!(!device.node(Role::Switch, "Mute").is(States::CHECKED));
    device.activate(Role::Switch, "Mute");
    assert!(device.node(Role::Switch, "Mute").is(States::CHECKED));
    assert!(
        device
            .ui
            .semantics()
            .find_by_label("YOU \u{b7} MUTED")
            .is_some()
    );
    // Muted, the tips are there and say they cannot be used.
    assert!(
        device
            .ui
            .semantics()
            .with_role(Role::Button)
            .filter(|node| node.hint == "Answers with this")
            .all(|tip| tip.is(States::DISABLED) && tip.actions.is_empty())
    );
    device.activate(Role::Switch, "Mute");

    // The red button.
    device.wait(2);
    device.activate(Role::Button, "End call");
    assert_eq!(device.screen(), Screen::CallReport);
    let report = device.ui.app().model().report.clone();
    assert_eq!(report.choices.len(), 1);
    let tree = device.ui.semantics();
    assert!(tree.find(Role::Heading, "Call report").is_some());
    assert!(
        tree.find_by_label(&format!("{} out of 100", report.overall()))
            .is_some(),
        "{:#?}",
        device.spoken()
    );
    assert_eq!(tree.with_role(Role::Progress).count(), 5);

    // And on: the transcript, back, and home.
    device.activate(Role::Button, "Transcript");
    assert_eq!(device.screen(), Screen::Transcript);
    device.activate(Role::Button, "Back");
    assert_eq!(device.screen(), Screen::CallReport);
    // The system's Back, as TalkBack's back gesture sends it.
    let root = device.ui.semantics().root().unwrap().id;
    assert!(device.ui.perform(root, Action::Dismiss));
    device.draw();
    assert_eq!(device.screen(), Screen::Home);
    let _ = Message::Back;
}

/// What the host is told as the call goes on: after the first frame, only
/// what changed.
#[test]
fn a_call_sends_the_host_what_changed() {
    let mut device = Device::with(CallReady::on(Screen::Home), galaxy_a27());
    let mut wire = Vec::new();
    let home = device.ui.semantics().clone();
    assert!(home.encode_update(None, &mut wire));
    let whole = wire.len();

    // A frame later nothing has changed, and nothing is sent.
    device.draw();
    wire.clear();
    assert!(!device.ui.semantics().encode_update(Some(&home), &mut wire));

    // The bell read: one node changes its label; the update is a fraction
    // of the screen.
    device.activate(Role::Button, "Notifications, unread");
    device.activate(Role::Button, "Back");
    assert!(device.ui.semantics().encode_update(Some(&home), &mut wire));
    assert!(wire.len() * 4 < whole, "{} of {whole} bytes", wire.len());
    assert!(
        App::semantics(&device.ui)
            .unwrap()
            .find(Role::Button, "Notifications")
            .is_some()
    );
}
