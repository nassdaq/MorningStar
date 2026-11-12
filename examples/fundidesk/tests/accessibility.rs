//! FundiDesk for someone who cannot see it (ADR 0032): every screen passes
//! the framework's accessibility check, the fields and the icon-only
//! controls have names, what is chosen says so, and logging in and opening
//! a repair can be done with nothing but what TalkBack offers.
//!
//! The design draws its touch targets 44 px tall and some of its links
//! smaller. Those are the design's own findings: the designed pixels are
//! left as they are (the fidelity tests hold them to the reference), and
//! each finding is listed here, with its reason, and in `SCREENS.md`.
//! Its colours pass: no text of the twelve screens is below 4.5:1.

use fundidesk::{FundiDesk, Screen};
use morningstar::ui::{AccessibilityIssue, Action, IssueKind, Role, SemanticsNode, States, Ui};
use morningstar::{App, Frame, FrameInfo, Insets, SurfaceSize};

/// The design's viewport: 390 x 844 dp, where the tall screens scroll.
fn phone() -> FrameInfo {
    FrameInfo::new(SurfaceSize::new(780, 1688)).with_scale(2.0)
}

/// The owner's phone: 384 x 832 dp at 2.8125, with its bars.
fn galaxy_a27() -> FrameInfo {
    FrameInfo::new(SurfaceSize::new(1080, 2340))
        .with_scale(2.8125)
        .with_insets(Insets::new(0.0, 102.0, 0.0, 42.0))
}

struct Device {
    ui: Ui<FundiDesk>,
    frame: Frame,
    info: FrameInfo,
}

impl Device {
    fn on(screen: Screen, info: FrameInfo) -> Self {
        let mut ui = Ui::testing(FundiDesk::on(screen));
        // What a host does when TalkBack is on.
        ui.set_accessibility(true);
        let frame = Frame::for_app(&mut ui);
        let mut device = Self { ui, frame, info };
        device.draw();
        device
    }

    fn draw(&mut self) {
        self.frame.begin();
        self.ui.frame(&self.info, &mut self.frame);
    }

    fn spoken(&self) -> Vec<String> {
        self.ui.semantics().spoken()
    }

    fn node(&self, role: Role, label: &str) -> SemanticsNode {
        self.ui
            .semantics()
            .find(role, label)
            .unwrap_or_else(|| panic!("no {role:?} called {label:?} among {:#?}", self.spoken()))
            .clone()
    }

    fn perform(&mut self, role: Role, label: &str, action: Action) {
        let node = self.node(role, label);
        assert!(
            self.ui.perform(node.id, action.clone()),
            "{label:?} does not do {action:?}"
        );
        self.draw();
    }

    fn activate(&mut self, role: Role, label: &str) {
        self.perform(role, label, Action::Activate);
    }
}

// ------------------------------------------------------------- the check

/// Why a finding is the design's own, or `None` if it is ours to fix.
fn designed(issue: &AccessibilityIssue) -> Option<&'static str> {
    if issue.kind != IssueKind::SmallTarget {
        return None;
    }
    // The detail is "touch target W x H dp, ...".
    let size: Vec<f32> = issue
        .detail
        .trim_start_matches("touch target ")
        .split(" dp")
        .next()
        .unwrap_or("")
        .split(" x ")
        .filter_map(|side| side.parse().ok())
        .collect();
    let smallest = size.iter().copied().fold(f32::INFINITY, f32::min);
    match issue.label.as_str() {
        // Text links, as tall as their line and the padding the design
        // gives them.
        "Skip" => Some("the design's Skip link: padding: 12px 4px (37 x 41)"),
        "Forgot password?" => Some("the design's link: padding: 6px 0 (29 tall)"),
        "New to FundiDesk? Create an account" | "Already have an account? Log in" => {
            Some("the design's last line: one line of 14 px text (17 tall)")
        }
        "Resend" => Some("the design's Resend link: padding: 12px 4px (41 tall)"),
        "See all" => Some("the design's link: padding: 8px 0 (33 tall)"),
        // Everything else the design sizes for touch is 44 px: its icon
        // buttons, chips, filter tabs and compact buttons.
        _ if smallest >= 43.9 => Some("the design's touch target: 44 px (space::TOUCH)"),
        _ => None,
    }
}

fn ours(device: &Device) -> Vec<String> {
    device
        .ui
        .accessibility_issues()
        .iter()
        .filter(|issue| designed(issue).is_none())
        .map(ToString::to_string)
        .collect()
}

#[test]
fn every_screen_passes_the_accessibility_check() {
    let mut allowed = std::collections::BTreeSet::new();
    for (name, info) in [("phone", phone()), ("galaxy a27", galaxy_a27())] {
        for screen in Screen::ALL {
            let device = Device::on(screen, info);
            assert_eq!(
                ours(&device),
                Vec::<String>::new(),
                "{} on {name}",
                screen.name()
            );
            let issues = device.ui.accessibility_issues();
            // The design's colours pass everywhere: nothing about
            // contrast, labels or twins is ever allowed.
            assert!(
                issues
                    .iter()
                    .all(|issue| issue.kind == IssueKind::SmallTarget),
                "{}: {issues:#?}",
                screen.name()
            );
            allowed.extend(issues.iter().filter_map(designed));
        }
    }
    // Every reason in the list is still met: none has gone stale.
    assert_eq!(allowed.len(), 6, "the design findings found: {allowed:#?}");
}

// ------------------------------------------------- what a screen says

#[test]
fn fields_are_called_by_their_labels() {
    let device = Device::on(Screen::Login, phone());
    assert_eq!(
        device.spoken(),
        [
            "Welcome back",
            "Log in to your FundiDesk shop account.",
            "Phone number",
            "Password",
            "Show password",
            "Forgot password?",
            "Log in",
            "or unlock with your fingerprint",
            "Unlock with fingerprint",
            "New to FundiDesk? Create an account",
        ]
    );
    let tree = device.ui.semantics();
    assert_eq!(
        tree.find_by_label("Welcome back").unwrap().role,
        Role::Heading
    );
    let phone_field = device.node(Role::TextField, "Phone number");
    assert_eq!(phone_field.hint, "07XX XXX XXX");
    let password = device.node(Role::TextField, "Password");
    assert!(password.is(States::PASSWORD));
    let eye = device.node(Role::Switch, "Show password");
    assert!(eye.is(States::CHECKABLE) && !eye.is(States::CHECKED));

    // The fields of the new-repair form have no placeholder: the label
    // above each is its name, and is not read a second time as a text.
    let device = Device::on(Screen::NewRepair, phone());
    let fields: Vec<(String, String)> = device
        .ui
        .semantics()
        .with_role(Role::TextField)
        .map(|field| (field.label.clone(), field.value.clone()))
        .collect();
    assert_eq!(
        fields,
        [
            ("Phone number".to_owned(), "0712 345 678".to_owned()),
            ("Customer name".to_owned(), "Amina Juma".to_owned()),
            ("Brand".to_owned(), "Samsung".to_owned()),
            ("Model".to_owned(), "Galaxy A14".to_owned()),
            ("Reported problem".to_owned(), String::new()),
        ]
    );
    assert_eq!(
        device
            .spoken()
            .iter()
            .filter(|said| said.starts_with("Customer name"))
            .count(),
        1
    );

    // The six boxes of the code say which digit each is.
    let device = Device::on(Screen::Verification, phone());
    let boxes: Vec<String> = device
        .ui
        .semantics()
        .with_role(Role::TextField)
        .map(SemanticsNode::spoken)
        .collect();
    assert_eq!(boxes.len(), 6);
    assert_eq!(boxes[0], "Digit 1 of 6, 4");
    assert_eq!(boxes[5], "Digit 6 of 6");
}

#[test]
fn chosen_things_say_that_they_are_chosen() {
    // The tab bar.
    let device = Device::on(Screen::Dashboard, phone());
    let tabs: Vec<(String, bool)> = device
        .ui
        .semantics()
        .with_role(Role::Tab)
        .map(|tab| (tab.label.clone(), tab.is(States::SELECTED)))
        .collect();
    assert_eq!(
        tabs,
        [
            ("Home".to_owned(), true),
            ("Repairs".to_owned(), false),
            ("Customers".to_owned(), false),
            ("Account".to_owned(), false),
        ]
    );

    // The filter tabs of the repairs list.
    let mut device = Device::on(Screen::RepairsList, phone());
    assert!(device.node(Role::Button, "All 14").is(States::SELECTED));
    assert!(!device.node(Role::Button, "Delayed 3").is(States::SELECTED));
    device.activate(Role::Button, "Delayed 3");
    assert!(device.node(Role::Button, "Delayed 3").is(States::SELECTED));
    assert!(!device.node(Role::Button, "All 14").is(States::SELECTED));

    // The choices of the shop set-up.
    let device = Device::on(Screen::ShopSetup, phone());
    assert!(device.node(Role::Button, "2 to 3").is(States::SELECTED));
    assert!(!device.node(Role::Button, "Just me").is(States::SELECTED));
    assert!(device.node(Role::Button, "Kiswahili").is(States::SELECTED));

    // The repair's progress is a value, not seven coloured bars.
    let device = Device::on(Screen::Tracking, phone());
    let progress = device.node(Role::Progress, "Repair progress");
    assert_eq!(progress.value, "In repair, step 5 of 7");
}

// --------------------------------------------------------------- journey

/// Log in, the dashboard, a repair: with nothing but lookups by role and
/// label and `perform`, which is all a TalkBack user has.
#[test]
fn log_in_and_open_a_repair_by_screen_reader_alone() {
    let mut device = Device::on(Screen::Login, phone());
    // Typing, as TalkBack's keyboard or a braille display sets a field.
    device.perform(
        Role::TextField,
        "Phone number",
        Action::SetText("0712345678".to_owned()),
    );
    assert_eq!(device.ui.app().login().phone, "0712345678");
    assert!(
        device
            .node(Role::TextField, "Phone number")
            .is(States::FOCUSED)
    );
    device.perform(
        Role::TextField,
        "Password",
        Action::SetText("siri-kubwa".to_owned()),
    );
    assert_eq!(device.ui.app().login().password, "siri-kubwa");
    // The password is in the application and nowhere in the tree.
    let password = device.node(Role::TextField, "Password");
    assert_eq!(password.value, "\u{2022}".repeat(10));
    assert!(
        device
            .ui
            .semantics()
            .nodes()
            .iter()
            .all(|node| !node.value.contains("siri") && !node.label.contains("siri"))
    );
    // "Show password" is a switch; on, the field is no longer a password
    // field and reads its text.
    device.activate(Role::Switch, "Show password");
    assert!(
        device
            .node(Role::Switch, "Show password")
            .is(States::CHECKED)
    );
    let shown = device.node(Role::TextField, "Password");
    assert!(!shown.is(States::PASSWORD));
    assert_eq!(shown.value, "siri-kubwa");
    device.activate(Role::Switch, "Show password");
    assert_eq!(
        device.node(Role::TextField, "Password").value,
        "\u{2022}".repeat(10)
    );

    device.activate(Role::Button, "Log in");
    assert_eq!(device.ui.app().screen(), Screen::Dashboard);
    let tree = device.ui.semantics();
    assert!(
        tree.find_by_label("Kariakoo Phone Clinic").is_some(),
        "{:#?}",
        device.spoken()
    );
    assert!(device.node(Role::Tab, "Home").is(States::SELECTED));

    // The list scrolls: a page forward, as TalkBack does when it runs out
    // of rows, brings the repairs received today into reach.
    let list = device
        .ui
        .semantics()
        .with_role(Role::List)
        .next()
        .expect("the dashboard scrolls")
        .clone();
    let mut pages = 0;
    while device
        .ui
        .semantics()
        .find_containing("Tecno Spark 10")
        .is_none_or(|row| row.bounds.height < 40.0 * device.info.scale)
    {
        assert!(
            device.ui.perform(list.id, Action::ScrollForward),
            "the end of the list, and no repair"
        );
        device.draw();
        pages += 1;
        assert!(pages < 10);
    }
    let repair = device
        .ui
        .semantics()
        .find_containing("Tecno Spark 10")
        .unwrap()
        .clone();
    // One stop for the whole row: the device, the customer and reference,
    // and the status.
    assert_eq!(repair.role, Role::Button);
    assert!(repair.label.contains("Neema Kileo"), "{}", repair.label);
    assert!(repair.label.ends_with("Received"), "{}", repair.label);
    assert!(device.ui.perform(repair.id, Action::Activate));
    device.draw();
    assert_eq!(device.ui.app().screen(), Screen::RepairDetails);
    let tree = device.ui.semantics();
    assert_eq!(
        tree.find_by_label("FD-2026-00142").map(|node| node.role),
        Some(Role::Heading)
    );
    assert!(tree.find(Role::Button, "Back").is_some());
    assert!(tree.find(Role::Button, "Share").is_some());
    assert!(App::semantics(&device.ui).is_some());

    // And back, with the labelled arrow.
    device.activate(Role::Button, "Back");
    assert_ne!(device.ui.app().screen(), Screen::RepairDetails);
}
