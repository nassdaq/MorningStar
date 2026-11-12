//! Accessibility (ADR 0032): the semantics tree a frame is described by,
//! the actions assistive technology performs on it, and the checks of
//! `Ui::accessibility_issues`.

use std::time::Duration;

use morningstar_core::{Insets, Rect, SurfaceSize};
use morningstar_runtime::{App, Frame, FrameInfo};
use morningstar_ui::prelude::*;
use morningstar_ui::{Actions, IssueKind, SemanticsNode, States};

const REGULAR: &[u8] =
    include_bytes!("../../../examples/hello-gpu/assets/fonts/NotoSans-Regular.ttf");
const BOLD: &[u8] = include_bytes!("../../../examples/hello-gpu/assets/fonts/NotoSans-Bold.ttf");

const BACK: VectorIcon = VectorIcon::stroked("M15 6l-6 6 6 6");

#[derive(Clone, Debug, PartialEq)]
enum Msg {
    Tap(&'static str),
    Name(String),
    Pin(String),
    Louder,
    Quieter,
    Say(&'static str),
}

#[derive(Default)]
struct State {
    name: String,
    pin: String,
    volume: u32,
    log: Vec<Msg>,
    backs: u32,
    say: Option<&'static str>,
}

struct Probe {
    view: fn(&State) -> Element<Msg>,
    state: State,
}

impl Application for Probe {
    type Message = Msg;

    fn setup(&mut self, setup: &mut Setup<'_>) {
        setup.add_font(REGULAR);
        setup.add_font(BOLD);
    }

    fn theme(&self) -> Theme {
        Theme::LIGHT
    }

    fn view(&self) -> Element<Msg> {
        (self.view)(&self.state)
    }

    fn update(&mut self, message: Msg) {
        match &message {
            Msg::Name(name) => self.state.name.clone_from(name),
            Msg::Pin(pin) => self.state.pin.clone_from(pin),
            Msg::Louder => self.state.volume += 1,
            Msg::Quieter => self.state.volume = self.state.volume.saturating_sub(1),
            Msg::Say(what) => self.state.say = Some(what),
            Msg::Tap(_) => {}
        }
        self.state.log.push(message);
    }

    fn back(&mut self) -> bool {
        self.state.backs += 1;
        true
    }

    fn effects(&mut self, out: &mut Effects<Msg>) {
        if let Some(what) = self.state.say.take() {
            out.announce(what);
        }
    }
}

struct Phone {
    ui: Ui<Probe>,
    frame: Frame,
    info: FrameInfo,
}

/// 360 x 800 dp at 3 px per dp.
fn phone(view: fn(&State) -> Element<Msg>) -> Phone {
    phone_at(
        view,
        FrameInfo::new(SurfaceSize::new(1080, 2400)).with_scale(3.0),
    )
}

fn phone_at(view: fn(&State) -> Element<Msg>, info: FrameInfo) -> Phone {
    let mut ui = Ui::testing(Probe {
        view,
        state: State::default(),
    });
    ui.set_accessibility(true);
    let frame = Frame::for_app(&mut ui);
    let mut phone = Phone { ui, frame, info };
    phone.draw();
    phone
}

impl Phone {
    fn draw(&mut self) {
        self.frame.begin();
        self.ui.frame(&self.info, &mut self.frame);
    }

    fn node(&self, label: &str) -> SemanticsNode {
        self.ui
            .semantics()
            .find_by_label(label)
            .unwrap_or_else(|| {
                panic!(
                    "no node labelled {label:?} among {:?}",
                    self.ui.semantics().spoken()
                )
            })
            .clone()
    }

    fn spoken(&self) -> Vec<String> {
        self.ui.semantics().spoken()
    }

    fn dp(&self, rect: Rect) -> Rect {
        let scale = self.info.scale;
        Rect::new(
            rect.x / scale,
            rect.y / scale,
            rect.width / scale,
            rect.height / scale,
        )
    }

    fn kinds(&self) -> Vec<(IssueKind, String)> {
        self.ui
            .accessibility_issues()
            .into_iter()
            .map(|issue| (issue.kind, issue.label))
            .collect()
    }
}

// ------------------------------------------------------------ the defaults

fn defaults(state: &State) -> Element<Msg> {
    col![
        text("Settings").title().heading(),
        text("Signed in as Amina"),
        // Decoration: says nothing.
        Icon::vector(BACK).size(24.0),
        Container::<Msg>::empty()
            .size(100.0, 4.0)
            .background(hex(0x112233)),
        Button::new("Save").on_press(Msg::Tap("save")),
        Button::<Msg>::new("Delete account"),
        // An icon-only control: its label is all a reader has.
        Pressable::new(Icon::vector(BACK).size(24.0))
            .size(48.0, 48.0)
            .on_press(Msg::Tap("back"))
            .label("Back"),
        // A row is one stop, read as its texts in order.
        row![
            Icon::vector(BACK).size(24.0),
            col![text("Juma Mwangi"), text("Screen repair").caption()],
            Spacer::new(),
            text("TSh 45,000"),
        ]
        .fill_width()
        .height(64.0)
        .on_press(Msg::Tap("row")),
        TextInput::new(&state.name)
            .placeholder("Your name")
            .on_input(Msg::Name)
            .height(48.0)
            .id("name"),
        TextInput::new(&state.pin)
            .placeholder("PIN")
            .secure(true)
            .on_input(Msg::Pin)
            .height(48.0)
            .id("pin"),
    ]
    .gap(8.0)
    .padding(16.0)
    .fill()
    .into()
}

#[test]
fn widgets_describe_themselves_without_being_asked() {
    let mut phone = phone(defaults);
    assert_eq!(
        phone.spoken(),
        [
            "Settings",
            "Signed in as Amina",
            "Save",
            "Delete account",
            "Back",
            "Juma Mwangi, Screen repair, TSh 45,000",
        ]
    );
    let tree = phone.ui.semantics();
    let roles: Vec<Role> = tree.nodes().iter().skip(1).map(|node| node.role).collect();
    assert_eq!(
        roles,
        [
            Role::Heading,
            Role::Text,
            Role::Button,
            Role::Button,
            Role::Button,
            Role::Button,
            Role::TextField,
            Role::TextField,
        ]
    );
    // The root stands for the surface and offers Back.
    let root = tree.root().unwrap();
    assert_eq!(root.bounds, Rect::new(0.0, 0.0, 1080.0, 2400.0));
    assert!(root.can(Actions::DISMISS));

    let save = phone.node("Save");
    assert!(save.can(Actions::ACTIVATE) && !save.is(States::DISABLED));
    // A button with nothing to do says so and offers nothing.
    let delete = phone.node("Delete account");
    assert!(delete.is(States::DISABLED) && delete.actions.is_empty());

    // A field: its placeholder is its hint, its text its value.
    let name = tree.with_role(Role::TextField).next().unwrap().clone();
    assert_eq!((name.hint.as_str(), name.value.as_str()), ("Your name", ""));
    assert!(
        name.can(Actions::FOCUS | Actions::SET_TEXT | Actions::SET_SELECTION | Actions::ACTIVATE)
    );
    assert!(
        phone
            .ui
            .perform(name.id, Action::SetText("Amina".to_owned()))
    );
    phone.draw();
    let name = phone.ui.semantics().node(name.id).unwrap().clone();
    assert_eq!(name.value, "Amina");
    // The placeholder is still its hint once it holds something.
    assert_eq!(name.hint, "Your name");
    assert!(name.is(States::FOCUSED));
    assert_eq!(name.selection, Some((5, 5)));
    assert_eq!(phone.ui.app().state.name, "Amina");
}

#[test]
fn a_password_is_never_in_the_tree() {
    let mut phone = phone(defaults);
    let pin = phone
        .ui
        .semantics()
        .with_role(Role::TextField)
        .nth(1)
        .unwrap()
        .clone();
    assert!(pin.is(States::PASSWORD));
    assert!(phone.ui.perform(pin.id, Action::SetText("4471".to_owned())));
    phone.draw();
    assert_eq!(phone.ui.app().state.pin, "4471");
    let tree = phone.ui.semantics();
    let pin = tree.node(pin.id).unwrap();
    assert_eq!(pin.value, "\u{2022}\u{2022}\u{2022}\u{2022}");
    assert_eq!(pin.selection, Some((4, 4)));
    assert!(
        tree.nodes().iter().all(|node| !node.label.contains("4471")
            && !node.value.contains("4471")
            && !node.hint.contains("4471")),
        "the password's characters are in the tree"
    );
    // Nor in what crosses to the host.
    let mut wire = Vec::new();
    assert!(tree.encode_update(None, &mut wire));
    assert!(!wire.windows(4).any(|window| window == b"4471"));
    // Its selection is addressed in characters, which the bullets count.
    let id = pin.id;
    assert!(
        phone
            .ui
            .perform(id, Action::SetSelection { start: 1, end: 3 })
    );
    phone.draw();
    assert_eq!(phone.ui.selection(), Some((1, 3)));
    assert_eq!(
        phone.ui.semantics().node(id).unwrap().selection,
        Some((1, 3))
    );
}

// ----------------------------------------------------------------- bounds

fn placed(_: &State) -> Element<Msg> {
    SafeArea::new(
        col![
            Button::new("Top").on_press(Msg::Tap("top")).id("top"),
            ScrollView::new(
                Column::new()
                    .extend((0..40).map(|index| {
                        Button::new(format!("Row {index}"))
                            .on_press(Msg::Tap("row"))
                            .height(56.0)
                            .fill_width()
                            .id(("row", index))
                    }))
                    .fill_width(),
            )
            .id("list"),
        ]
        .fill(),
    )
    .into()
}

#[test]
fn bounds_are_where_the_pixels_are_at_a_real_phones_scale_and_insets() {
    // 384 x 853 dp at 2.8125 with a 132 px status bar: the test phone.
    let info = FrameInfo::new(SurfaceSize::new(1080, 2400))
        .with_scale(2.8125)
        .with_insets(Insets::new(0.0, 132.0, 0.0, 48.0));
    let mut phone = phone_at(placed, info);
    let top = phone.node("Top");
    // Below the status bar, in pixels of the surface, which fills the
    // window: these are screen coordinates once the host adds the view's
    // position.
    assert_eq!(top.bounds.y, 132.0);
    assert_eq!(Some(top.bounds), phone.ui.rect_of("top"));
    // 48 dp is 135 px.
    assert_eq!(top.bounds.height, 135.0);

    let list = phone
        .ui
        .semantics()
        .with_role(Role::List)
        .next()
        .unwrap()
        .clone();
    assert_eq!(Some(list.bounds), phone.ui.visible_rect_of("list"));
    assert_eq!(list.bounds.y + list.bounds.height, 2400.0 - 48.0);
    assert!(list.can(Actions::SCROLL_FORWARD) && !list.can(Actions::SCROLL_BACKWARD));
    let first = phone.node("Row 0");
    assert_eq!(Some(first.bounds), phone.ui.rect_of(("row", 0)));
    // Rows far below the viewport are not offered at all; the last one
    // that shows is cut to what shows.
    assert!(phone.ui.semantics().find_by_label("Row 30").is_none());
    let tree = phone.ui.semantics();
    let at = tree.index_of(list.id).unwrap();
    let rows: Vec<&SemanticsNode> = tree
        .children(at)
        .map(|index| &tree.nodes()[index])
        .collect();
    let last = rows.last().unwrap();
    assert!(last.bounds.y + last.bounds.height <= list.bounds.y + list.bounds.height);
    assert!(rows.iter().all(|row| !row.bounds.is_empty()));

    // Scrolling moves the bounds with the pixels.
    assert!(phone.ui.perform(list.id, Action::ScrollForward));
    phone.draw();
    let list = phone.ui.semantics().node(list.id).unwrap().clone();
    let (position, _) = list.scroll.unwrap();
    assert!(position > 300.0, "scrolled {position} dp");
    assert!(list.can(Actions::SCROLL_BACKWARD));
    assert_eq!(phone.ui.scroll_offset("list"), Some(position));
    let moved = phone.ui.semantics().find_by_label("Row 12").unwrap();
    assert_eq!(Some(moved.bounds), phone.ui.visible_rect_of(("row", 12)));
    assert!(phone.ui.perform(list.id, Action::ScrollBackward));
    phone.draw();
    assert_eq!(phone.ui.scroll_offset("list"), Some(0.0));
    assert!(!phone.ui.perform(list.id, Action::ScrollBackward));
}

fn turned(_: &State) -> Element<Msg> {
    col![
        Spacer::fixed(100.0),
        row![
            Spacer::fixed(100.0),
            Button::new("Go")
                .on_press(Msg::Tap("go"))
                .size(100.0, 50.0)
                .rotate(90.0)
                .id("go"),
        ],
        // Scaled to nothing, and drawn at no opacity: neither is there.
        Button::new("Flat")
            .on_press(Msg::Tap("flat"))
            .transform(Transform::scale(0.0, 0.0)),
        Container::new(Button::new("Ghost").on_press(Msg::Tap("ghost"))).opacity(0.0),
        // Clipped away entirely.
        Container::new(
            Container::new(Button::new("Cut").on_press(Msg::Tap("cut"))).offset(0.0, 200.0)
        )
        .size(100.0, 50.0)
        .clip(),
    ]
    .fill()
    .into()
}

#[test]
fn bounds_follow_transforms_and_what_cannot_be_seen_is_left_out() {
    let phone = phone(turned);
    let go = phone.node("Go");
    // 100 x 50 dp turned a quarter about its centre (150, 125) covers
    // 50 x 100 dp.
    let bounds = phone.dp(go.bounds);
    assert!((bounds.x - 125.0).abs() < 0.5, "{bounds:?}");
    assert!((bounds.y - 75.0).abs() < 0.5, "{bounds:?}");
    assert!((bounds.width - 50.0).abs() < 0.5, "{bounds:?}");
    assert!((bounds.height - 100.0).abs() < 0.5, "{bounds:?}");
    assert_eq!(phone.spoken(), ["Go"]);
}

// -------------------------------------------------------------- modifiers

fn modified(state: &State) -> Element<Msg> {
    col![
        row![
            Pressable::new(text("Home"))
                .size(80.0, 48.0)
                .on_press(Msg::Tap("home"))
                .role(Role::Tab)
                .selected(true),
            Pressable::new(text("History"))
                .size(80.0, 48.0)
                .on_press(Msg::Tap("history"))
                .role(Role::Tab)
                .selected(false),
        ],
        Pressable::new(text("Mute"))
            .size(80.0, 48.0)
            .on_press(Msg::Tap("mute"))
            .role(Role::Switch)
            .checked(true)
            .hint("Silences the caller"),
        Container::<Msg>::empty()
            .size(200.0, 8.0)
            .background(hex(0x2C2C2E))
            .role(Role::Progress)
            .label("Fluency")
            .value("72 percent"),
        col![text("Calls"), text("12")].merge_semantics(),
        col![text("Caller"), text("Habari yako?")]
            .live(Live::Polite)
            .id("line"),
        text("decoration").exclude_semantics(),
        col![text("A"), text("B")].semantics_group().label("Plans"),
        row![text("Volume")]
            .size(200.0, 48.0)
            .role(Role::Slider)
            .value(format!("{}", state.volume))
            .on_increment(Msg::Louder)
            .on_decrement(Msg::Quieter),
        text("0712 345 678").semantic_label("Phone number 0712 345 678"),
    ]
    .gap(8.0)
    .fill()
    .into()
}

#[test]
fn modifiers_say_what_the_defaults_cannot() {
    let mut phone = phone(modified);
    assert_eq!(
        phone.spoken(),
        [
            "Home",
            "History",
            "Mute",
            "Fluency, 72 percent",
            "Calls, 12",
            "Caller, Habari yako?",
            "Plans",
            "A",
            "B",
            "Volume, 0",
            "Phone number 0712 345 678",
        ]
    );
    let home = phone.node("Home");
    assert_eq!(home.role, Role::Tab);
    assert!(home.is(States::SELECTED));
    assert!(!phone.node("History").is(States::SELECTED));
    let mute = phone.node("Mute");
    assert_eq!(mute.role, Role::Switch);
    assert!(mute.is(States::CHECKABLE | States::CHECKED));
    assert_eq!(mute.hint, "Silences the caller");
    let fluency = phone.node("Fluency");
    assert_eq!(
        (fluency.role, fluency.value.as_str()),
        (Role::Progress, "72 percent")
    );
    assert_eq!(phone.node("Calls, 12").role, Role::Text);
    assert_eq!(phone.node("Caller, Habari yako?").live(), Live::Polite);

    // A group keeps what is inside it as stops of their own.
    let tree = phone.ui.semantics();
    let plans = tree
        .index_of(tree.find_by_label("Plans").unwrap().id)
        .unwrap();
    assert_eq!(tree.nodes()[plans].role, Role::Group);
    let inside: Vec<&str> = tree
        .children(plans)
        .map(|index| tree.nodes()[index].label.as_str())
        .collect();
    assert_eq!(inside, ["A", "B"]);

    // Raising and lowering a value.
    let volume = phone.node("Volume");
    assert_eq!(volume.role, Role::Slider);
    assert!(volume.can(Actions::INCREMENT | Actions::DECREMENT));
    assert!(phone.ui.perform(volume.id, Action::Increment));
    phone.draw();
    assert!(phone.ui.perform(volume.id, Action::Increment));
    phone.draw();
    assert!(phone.ui.perform(volume.id, Action::Decrement));
    phone.draw();
    assert_eq!(phone.node("Volume").value, "1");
    // The same widget keeps its identity while its value changes.
    assert_eq!(phone.node("Volume").id, volume.id);
}

#[test]
fn actions_do_what_a_touch_does() {
    let mut phone = phone(modified);
    assert!(phone.ui.activate("History"));
    assert_eq!(phone.ui.app().state.log, [Msg::Tap("history")]);
    // Like a touch, an action acts on a frame that is there.
    phone.draw();
    let mute = phone.node("Mute").id;
    assert!(phone.ui.perform(mute, Action::Activate));
    // What a node does not offer is not done.
    phone.draw();
    assert!(!phone.ui.perform(mute, Action::ScrollForward));
    assert!(!phone.ui.perform(mute, Action::SetText("x".to_owned())));
    assert!(!phone.ui.perform(12345, Action::Activate));
    assert!(!phone.ui.activate("Calls, 12"));
    assert_eq!(
        phone.ui.app().state.log,
        [Msg::Tap("history"), Msg::Tap("mute")]
    );
    // Back, through the root.
    let root = phone.ui.semantics().root().unwrap().id;
    assert!(phone.ui.perform(root, Action::Dismiss));
    assert_eq!(phone.ui.app().state.backs, 1);
}

// ------------------------------------------------------- order and layers

fn layered(_: &State) -> Element<Msg> {
    stack![
        col![
            text("Under the sheet"),
            Button::new("Hidden").on_press(Msg::Tap("hidden")),
        ]
        .fill(),
        // A bar pinned to the bottom, painted before the header but read
        // after it.
        Container::new(text("Footer"))
            .fill_width()
            .height(40.0)
            .place(Alignment::BOTTOM_CENTER),
        Container::new(text("Header"))
            .fill_width()
            .height(40.0)
            .place(Alignment::TOP_CENTER),
    ]
    .fill()
    .into()
}

fn modal(_: &State) -> Element<Msg> {
    stack![
        col![
            text("Under the sheet"),
            Button::new("Hidden").on_press(Msg::Tap("hidden")),
        ]
        .fill(),
        // The scrim swallows touches: what is under it is out of reach.
        Container::new(
            col![
                text("Leave the call?"),
                Button::new("Leave").on_press(Msg::Tap("leave")),
            ]
            .padding(24.0)
            .background(hex(0xFFFFFF)),
        )
        .fill()
        .center()
        .background(hexa(0x0000_0080))
        .block_touches(),
    ]
    .fill()
    .into()
}

#[test]
fn stacked_layers_are_read_top_to_bottom() {
    // Each layer is read whole; the footer, painted before the header,
    // is read after it.
    let phone = phone(layered);
    assert_eq!(
        phone.spoken(),
        ["Under the sheet", "Hidden", "Header", "Footer"]
    );
}

#[test]
fn a_row_is_read_in_its_own_order_however_its_pieces_are_nudged() {
    // Two cut-out digits, the second raised above the first: still "8"
    // then "2", though the box around them is a stack of one.
    fn plates(_: &State) -> Element<Msg> {
        Container::new(
            row![
                text("8").rotate(-5.0),
                text("2").transform(Transform::rotate(4.0).and_translate(0.0, -6.0)),
                text("/100"),
            ]
            .cross_align(CrossAlign::End),
        )
        .padding(40.0)
        .into()
    }
    let phone = phone(plates);
    assert_eq!(phone.spoken(), ["8", "2", "/100"]);
}

#[test]
fn what_lies_under_a_scrim_is_not_offered() {
    let phone = phone(modal);
    assert_eq!(phone.spoken(), ["Leave the call?", "Leave"]);
}

fn form(state: &State) -> Element<Msg> {
    col![
        TextInput::new(&state.name)
            .placeholder("Name")
            .on_input(Msg::Name)
            .height(48.0)
            .id("name")
    ]
    .padding(16.0)
    .fill()
    .into()
}

#[test]
fn the_selection_menu_is_offered_while_it_is_up() {
    let mut phone = phone(form);
    let field = phone
        .ui
        .semantics()
        .with_role(Role::TextField)
        .next()
        .unwrap()
        .id;
    assert!(
        phone
            .ui
            .perform(field, Action::SetText("habari yako".to_owned()))
    );
    phone.draw();
    // Press and hold: the word in the middle is selected and the menu
    // comes up, as under a finger.
    assert!(phone.ui.perform(field, Action::LongPress));
    phone.draw();
    phone.draw();
    assert!(phone.ui.selection_menu_shown());
    let spoken = phone.spoken();
    assert!(
        spoken.ends_with(&["Cut".to_owned(), "Copy".to_owned(), "Select all".to_owned()])
            || spoken.contains(&"Copy".to_owned()),
        "{spoken:?}"
    );
    assert!(phone.ui.activate("Copy"));
    assert!(phone.ui.clipboard().is_some_and(|text| !text.is_empty()));
}

// ---------------------------------------------------------------- effects

fn announcer(_: &State) -> Element<Msg> {
    Button::new("Finish")
        .on_press(Msg::Say("Report ready"))
        .into()
}

#[test]
fn an_announcement_is_a_request_to_the_host() {
    let mut phone = phone(announcer);
    assert!(phone.ui.announcements().is_empty());
    assert!(phone.ui.activate("Finish"));
    assert_eq!(phone.ui.announcements(), ["Report ready"]);

    // With a host, it is one of the application's requests.
    let mut ui = Ui::new(Probe {
        view: announcer,
        state: State::default(),
    });
    let mut calls = Vec::new();
    ui.service_requests(&mut calls);
    ui.send(Msg::Say("Report ready"));
    ui.service_requests(&mut calls);
    assert_eq!(
        calls
            .iter()
            .map(|call| call.request.clone())
            .collect::<Vec<_>>(),
        [ServiceRequest::Announce {
            message: "Report ready".to_owned(),
            assertive: false
        }]
    );
    assert!(!calls[0].request.expects_result());
}

// -------------------------------------------------------- built on demand

#[test]
fn nothing_is_described_until_it_is_asked_for() {
    let mut ui = Ui::new(Probe {
        view: defaults,
        state: State::default(),
    });
    let mut frame = Frame::for_app(&mut ui);
    let off = FrameInfo::new(SurfaceSize::new(1080, 2400)).with_scale(3.0);
    frame.begin();
    ui.frame(&off, &mut frame);
    assert!(App::semantics(&ui).is_none());
    assert!(!ui.perform(1, Action::Activate));
    assert!(!ui.activate("Save"));

    // The host says a screen reader is running.
    frame.begin();
    ui.frame(&off.with_accessibility(true), &mut frame);
    let tree = App::semantics(&ui).expect("described");
    assert_eq!(tree.len(), 9);
    let save = tree.find(Role::Button, "Save").unwrap().id;
    // The engine's route for a host's action.
    assert!(ui.accessibility_action(save, &Action::Activate));
    assert_eq!(ui.app().state.log, [Msg::Tap("save")]);

    // And that it has stopped: nothing is kept.
    frame.begin();
    ui.frame(&off, &mut frame);
    assert!(App::semantics(&ui).is_none());
    assert!(!ui.accessibility_action(save, &Action::Activate));
}

#[test]
#[should_panic(expected = "set_accessibility(true)")]
fn asking_for_the_tree_of_a_frame_that_was_not_described_is_an_error() {
    let mut ui = Ui::new(Probe {
        view: defaults,
        state: State::default(),
    });
    let mut frame = Frame::for_app(&mut ui);
    frame.begin();
    ui.frame(&FrameInfo::new(SurfaceSize::new(360, 800)), &mut frame);
    let _ = ui.accessibility_issues();
}

#[test]
fn a_screen_that_only_scrolls_sends_only_what_moved() {
    let mut phone = phone(placed);
    let before = phone.ui.semantics().clone();
    let mut wire = Vec::new();
    // Nothing changed: nothing to send.
    phone.info = phone.info.with_elapsed(Duration::from_millis(16));
    phone.draw();
    assert!(!phone.ui.semantics().encode_update(Some(&before), &mut wire));
    let list = before.with_role(Role::List).next().unwrap().id;
    assert!(phone.ui.perform(list, Action::ScrollForward));
    phone.draw();
    let after = phone.ui.semantics();
    assert!(after.encode_update(Some(&before), &mut wire));
    let mut whole = Vec::new();
    after.encode_update(None, &mut whole);
    assert!(wire.len() < whole.len() + 200);
    // The button above the list did not move and is not in the update.
    assert!(!wire.windows(3).any(|window| window == b"Top"));
    assert!(whole.windows(3).any(|window| window == b"Top"));
}

// ----------------------------------------------------------------- checks

fn faulty(_: &State) -> Element<Msg> {
    col![
        // Nothing to call it.
        Pressable::new(Icon::vector(BACK).size(24.0))
            .size(48.0, 48.0)
            .on_press(Msg::Tap("back")),
        // Too small to hit.
        Pressable::new(text("Skip"))
            .size(60.0, 30.0)
            .on_press(Msg::Tap("skip")),
        // Told apart by nothing.
        row![
            Button::new("Open").on_press(Msg::Tap("one")),
            Button::new("Open").on_press(Msg::Tap("two")),
        ],
        // 2.85:1: light grey on white.
        text("Fine print").color(hex(0x999999)),
        // The same grey is enough for large text (3:1 needs #949494).
        text("Large").size(28.0).color(hex(0x8A8A8A)),
        text("Large and light").size(28.0).color(hex(0xAAAAAA)),
        // White on a mid blue plate: 3.68:1.
        Container::new(text("On blue").color(hex(0xFFFFFF)))
            .padding(8.0)
            .background(hex(0x3B8BEB)),
        // Over a gradient nothing is known, so nothing is claimed.
        Container::new(text("On a gradient").color(hex(0x999999)))
            .padding(8.0)
            .gradient(Gradient::vertical(hex(0x000000), hex(0xFFFFFF))),
        // Black at an alpha of 128/255 on white lands as #7F7F7F: 4.00:1.
        text("Faded").color(hexa(0x0000_0080)),
        // Not read out, so not judged.
        text("watermark").color(hex(0xEEEEEE)).exclude_semantics(),
        TextInput::<Msg>::new("").height(48.0),
    ]
    .gap(8.0)
    .background(hex(0xFFFFFF))
    .fill()
    .into()
}

#[test]
fn the_checker_finds_what_a_review_would() {
    let phone = phone(faulty);
    let found = phone.kinds();
    assert_eq!(
        found,
        [
            (IssueKind::MissingLabel, String::new()),
            (IssueKind::SmallTarget, "Skip".to_owned()),
            (IssueKind::MissingLabel, String::new()),
            (IssueKind::DuplicateLabel, "Open".to_owned()),
            (IssueKind::LowContrast, "Fine print".to_owned()),
            (IssueKind::LowContrast, "Large and light".to_owned()),
            (IssueKind::LowContrast, "On blue".to_owned()),
            (IssueKind::LowContrast, "Faded".to_owned()),
        ],
        "{:#?}",
        phone.ui.accessibility_issues()
    );
    let issues = phone.ui.accessibility_issues();
    let skip = &issues[1];
    assert_eq!(
        skip.to_string(),
        "small target: \"Skip\" at (0, 56) 60 x 30 dp: touch target 60 x 30 dp, smaller than 48 x 48"
    );
    let fine = issues
        .iter()
        .find(|issue| issue.label == "Fine print")
        .unwrap();
    assert!(
        fine.detail
            .starts_with("contrast 2.85:1 of #999999 on #FFFFFF, below 4.5:1 for 15 dp text"),
        "{}",
        fine.detail
    );
    let faded = issues.iter().find(|issue| issue.label == "Faded").unwrap();
    assert!(
        faded.detail.contains("#7F7F7F on #FFFFFF"),
        "{}",
        faded.detail
    );
}

#[test]
fn a_sound_screen_has_no_issues() {
    for view in [defaults, placed, modified, modal, form] {
        let phone = phone(view);
        assert_eq!(phone.ui.accessibility_issues(), []);
    }
}

#[test]
fn a_field_in_a_frame_is_as_large_as_its_frame() {
    fn framed(state: &State) -> Element<Msg> {
        col![
            row![
                Icon::vector(BACK).size(20.0),
                TextInput::new(&state.name)
                    .placeholder("Search")
                    .on_input(Msg::Name)
            ]
            .gap(8.0)
            .padding_xy(12.0, 0.0)
            .height(52.0)
            .fill_width()
            .cross_align(CrossAlign::Center)
            .input_frame()
            // A label on the frame is the field's.
            .label("Search repairs")
            .id("frame")
        ]
        .padding(16.0)
        .fill()
        .into()
    }
    let phone = phone(framed);
    let field = phone
        .ui
        .semantics()
        .with_role(Role::TextField)
        .next()
        .unwrap();
    assert_eq!(Some(field.bounds), phone.ui.rect_of("frame"));
    assert_eq!(
        (field.label.as_str(), field.hint.as_str()),
        ("Search repairs", "Search")
    );
    assert_eq!(phone.spoken(), ["Search repairs"]);
    // The text itself is one line tall; the target is the frame.
    assert_eq!(phone.ui.accessibility_issues(), []);
}
