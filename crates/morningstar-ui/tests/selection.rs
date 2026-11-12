//! Selection, the clipboard, caret keys and undo in a `TextInput`, and the
//! system-bar and reduce-motion wiring: driven by pointer, key and
//! input-method events at the frame of a Galaxy A27 (1080 x 2340 px at
//! 2.8125 px per dp), the way the engine drives a `Ui`.

use std::time::Duration;

use morningstar_core::{
    CaretMove, DrawOp, Insets, Point, PointerEvent, PointerPhase, Rect, SurfaceSize, TextEditState,
    TextInputEvent,
};
use morningstar_runtime::{App, Frame, FrameInfo, TextStyle};
use morningstar_ui::prelude::*;

const REGULAR: &[u8] =
    include_bytes!("../../../examples/hello-gpu/assets/fonts/NotoSans-Regular.ttf");
const ARABIC: &[u8] =
    include_bytes!("../../../examples/hello-gpu/assets/fonts/NotoSansArabic-Regular.ttf");

const SCALE: f32 = 2.8125;
const NOTE: &str = "Fundi atafika kesho asubuhi na vifaa vyote vya kazi, tafadhali \
                    hakikisha mlango uko wazi na umeme upo";

#[derive(Clone, Debug, PartialEq)]
enum Msg {
    Name(String),
    Phone(String),
    Secret(String),
    Note(String),
    Pasted(Option<String>),
    CopyCode,
    AskPaste,
}

#[derive(Default)]
struct Form {
    name: String,
    phone: String,
    secret: String,
    note: String,
    /// The fields are far down a scrolling page.
    tall: bool,
    dark: bool,
    bars: Option<SystemBars>,
    wants: Option<Msg>,
    pasted: Option<Option<String>>,
}

fn field(input: TextInput<Msg>) -> Container<Msg> {
    Container::new(input)
        .fill_width()
        .padding_xy(16.0, 14.0)
        .radius(12.0)
        .border(1.0, ThemeColor::Divider)
        .focused_border(2.0, ThemeColor::Primary)
        .input_frame()
}

fn fields(form: &Form) -> Container<Msg> {
    col![
        field(TextInput::new(&form.name).on_input(Msg::Name).id("name")),
        field(
            TextInput::new(&form.phone)
                .digits_only()
                .max_length(10)
                .on_input(Msg::Phone)
                .id("phone")
        ),
        field(
            TextInput::new(&form.secret)
                .secure(true)
                .on_input(Msg::Secret)
                .id("secret")
        ),
        field(
            TextInput::new(&form.note)
                .multiline()
                .max_lines(3)
                .on_input(Msg::Note)
                .id("note")
        ),
        field(TextInput::new("read only").id("fixed")),
    ]
    .gap(12.0)
    .fill_width()
}

impl Application for Form {
    type Message = Msg;

    fn setup(&mut self, setup: &mut Setup<'_>) {
        setup.add_font(REGULAR);
        setup.add_font(ARABIC);
    }

    fn theme(&self) -> Theme {
        if self.dark { Theme::DARK } else { Theme::LIGHT }
    }

    fn view(&self) -> Element<Msg> {
        if self.tall {
            ScrollView::new(
                col![
                    Container::empty().height(500.0).fill_width(),
                    fields(self),
                    Container::empty().height(900.0).fill_width(),
                ]
                .padding(20.0)
                .fill_width(),
            )
            .id("scroll")
            .into()
        } else {
            col![fields(self)]
                .padding(20.0)
                .fill()
                .safe_area(SafeEdges::ALL)
                .into()
        }
    }

    fn update(&mut self, message: Msg) {
        match message {
            Msg::Name(name) => self.name = name,
            Msg::Phone(phone) => self.phone = phone,
            Msg::Secret(secret) => self.secret = secret,
            Msg::Note(note) => self.note = note,
            Msg::Pasted(text) => self.pasted = Some(text),
            other => self.wants = Some(other),
        }
    }

    fn effects(&mut self, out: &mut Effects<Msg>) {
        match self.wants.take() {
            Some(Msg::CopyCode) => out.set_clipboard("4471"),
            Some(Msg::AskPaste) => out.clipboard(Msg::Pasted),
            _ => {}
        }
    }

    fn system_bars(&self) -> Option<SystemBars> {
        self.bars
    }
}

struct Phone {
    ui: Ui<Form>,
    frame: Frame,
    info: FrameInfo,
    time: Duration,
}

impl Phone {
    fn new(form: Form) -> Self {
        let mut ui = Ui::testing(form);
        let frame = Frame::for_app(&mut ui);
        let mut phone = Self {
            ui,
            frame,
            info: FrameInfo::new(SurfaceSize::new(1080, 2340))
                .with_scale(SCALE)
                .with_insets(Insets::new(0.0, 102.0, 0.0, 42.0)),
            time: Duration::ZERO,
        };
        phone.draw();
        phone
    }

    fn named(name: &str) -> Self {
        Self::new(Form {
            name: name.to_owned(),
            ..Form::default()
        })
    }

    fn draw(&mut self) -> bool {
        self.frame.begin();
        let info = self.info.with_elapsed(self.time);
        self.ui.frame(&info, &mut self.frame);
        self.frame.redraw_requested()
    }

    fn wait(&mut self, millis: u64) {
        self.time += Duration::from_millis(millis);
        self.draw();
    }

    fn touch(&mut self, phase: PointerPhase, at: Point) {
        self.ui.pointer(&PointerEvent {
            id: 1,
            phase,
            position: at,
        });
    }

    /// Down, frame, up, frame, 60 ms in all: a tap as the engine delivers
    /// it.
    fn tap(&mut self, at: Point) {
        self.touch(PointerPhase::Down, at);
        self.wait(10);
        self.touch(PointerPhase::Up, at);
        self.wait(50);
    }

    fn double_tap(&mut self, at: Point) {
        self.tap(at);
        self.tap(at);
    }

    /// A finger put down and held still for 600 ms, then lifted.
    fn long_press(&mut self, at: Point) {
        self.touch(PointerPhase::Down, at);
        self.wait(0);
        self.wait(600);
        self.touch(PointerPhase::Up, at);
        self.wait(10);
    }

    /// A drag from `from` to `to` in `steps` moves, a frame after each,
    /// then `frames` more frames with the finger held at `to`.
    fn drag(&mut self, from: Point, to: Point, steps: u32, frames: u32) {
        self.touch(PointerPhase::Down, from);
        self.wait(10);
        for step in 1..=steps {
            let part = step as f32 / steps as f32;
            self.touch(
                PointerPhase::Moved,
                Point::new(
                    from.x + (to.x - from.x) * part,
                    from.y + (to.y - from.y) * part,
                ),
            );
            self.wait(16);
        }
        for _ in 0..frames {
            self.wait(16);
        }
        self.touch(PointerPhase::Up, to);
        self.wait(10);
    }

    fn key(&mut self, event: TextInputEvent) {
        self.ui.text_input(&event);
        self.draw();
    }

    fn go(&mut self, to: CaretMove, extend: bool) {
        self.key(TextInputEvent::Move { to, extend });
    }

    fn rect(&self, id: &str) -> Rect {
        self.ui.rect_of(id).expect("the widget is on screen")
    }

    /// The point on screen in the middle of the line of field `id`, at
    /// byte `index` of its (single-line, unscrolled) text.
    fn at(&mut self, id: &str, text: &str, index: usize) -> Point {
        let rect = self.rect(id);
        let style = TextStyle::new(15.0 * SCALE, Color::BLACK);
        let caret = self.frame.text_caret(text, &style, None, index);
        Point::new(rect.x + caret.x, rect.y + caret.y + caret.height / 2.0)
    }

    fn center(rect: Rect) -> Point {
        Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
    }

    fn menu(&self) -> Vec<&'static str> {
        self.ui
            .selection_menu()
            .into_iter()
            .map(|(label, _)| label)
            .collect()
    }

    fn choose(&mut self, label: &str) {
        let (_, rect) = self
            .ui
            .selection_menu()
            .into_iter()
            .find(|(candidate, _)| *candidate == label)
            .unwrap_or_else(|| panic!("no menu item {label:?}"));
        self.tap(Self::center(rect));
    }

    fn selected(&self) -> Option<&str> {
        let (start, end) = self.ui.selection()?;
        let app = self.ui.app();
        [&app.name, &app.phone, &app.secret, &app.note]
            .into_iter()
            .chain([&"read only".to_owned()].map(|_| &app.name))
            .find(|value| value.len() >= end && value.is_char_boundary(start))
            .map(|value| &value[start..end])
    }

    /// The highlight rectangles of the last frame: fills in the primary
    /// colour at the selection's opacity.
    fn highlights(&self) -> Vec<Rect> {
        let primary = self.ui.app().theme().colors.primary;
        self.frame
            .list
            .ops()
            .iter()
            .filter_map(|op| match op {
                DrawOp::Rect { rect, color, .. }
                    if (color.a - 0.4).abs() < 1e-6
                        && color.r == primary.r
                        && color.g == primary.g
                        && color.b == primary.b =>
                {
                    Some(*rect)
                }
                _ => None,
            })
            .collect()
    }

    /// The solid primary-coloured discs of the last frame: the handles.
    fn discs(&self) -> Vec<Rect> {
        let primary = self.ui.app().theme().colors.primary;
        self.frame
            .list
            .ops()
            .iter()
            .filter_map(|op| match op {
                DrawOp::Rect {
                    rect,
                    color,
                    corner_radius,
                } if *color == primary && *corner_radius > 0.0 && rect.width == rect.height => {
                    Some(*rect)
                }
                _ => None,
            })
            .collect()
    }
}

// ------------------------------------------------------------- selecting

#[test]
fn a_double_tap_selects_the_word_and_brings_up_handles_and_the_menu() {
    let mut phone = Phone::named("Habari yako rafiki");
    let at = phone.at("name", "Habari yako rafiki", 9);
    phone.double_tap(at);
    assert_eq!(phone.ui.selection(), Some((7, 11)));
    assert_eq!(phone.selected(), Some("yako"));
    assert!(phone.ui.selection_menu_shown());
    assert_eq!(phone.menu(), ["Cut", "Copy", "Paste", "Select all"]);
    let [start, end] = phone.ui.selection_handles();
    let (start, end) = (start.unwrap(), end.unwrap());
    assert!(start.x < end.x, "the start's handle is on the left");
    // One highlight on the one line, exactly over the word, and two discs.
    let highlights = phone.highlights();
    assert_eq!(highlights.len(), 1);
    let (left, right) = (
        phone.at("name", "Habari yako rafiki", 7).x,
        phone.at("name", "Habari yako rafiki", 11).x,
    );
    assert!((highlights[0].x - left).abs() <= 1.0);
    assert!((highlights[0].x + highlights[0].width - right).abs() <= 1.0);
    assert_eq!(phone.discs().len(), 2);
    // A selection has no caret and asks for no blink frames.
    assert_eq!(phone.frame.redraw_after(), None);
    // The first field has the status bar above it and no room for a
    // menu: it goes below the line and its handles, inside the screen.
    let name = phone.rect("name");
    let (_, handle) = (start, end);
    for (_, item) in phone.ui.selection_menu() {
        assert!(
            item.y >= handle.y + handle.height - 40.0,
            "{item:?} {handle:?}"
        );
        assert!(item.y > name.y + name.height);
        assert!(item.x >= 0.0 && item.x + item.width <= 1080.0);
    }
    // Lower down there is room: the menu sits above the line.
    let fixed = phone.rect("fixed");
    phone.long_press(Point::new(fixed.x + 20.0, fixed.y + fixed.height / 2.0));
    assert_eq!(phone.menu(), ["Copy", "Select all"]);
    for (_, item) in phone.ui.selection_menu() {
        assert!(item.y + item.height <= fixed.y, "{item:?} {fixed:?}");
        assert!(item.y >= 102.0, "below the status bar");
    }
}

#[test]
fn two_taps_far_apart_in_time_or_place_only_move_the_caret() {
    let mut phone = Phone::named("Habari yako rafiki");
    let at = phone.at("name", "Habari yako rafiki", 9);
    phone.tap(at);
    phone.wait(600);
    phone.tap(at);
    assert_eq!(phone.ui.selection(), None);
    assert_eq!(phone.ui.caret(), Some(9));
    assert!(!phone.ui.selection_menu_shown());

    // Quickly, but on another word.
    let elsewhere = phone.at("name", "Habari yako rafiki", 2);
    phone.wait(600);
    phone.tap(at);
    phone.tap(elsewhere);
    assert_eq!(phone.ui.selection(), None);
    assert_eq!(phone.ui.caret(), Some(2));
    assert!(phone.highlights().is_empty());
}

#[test]
fn a_triple_tap_selects_everything() {
    let mut phone = Phone::named("Habari yako rafiki");
    let at = phone.at("name", "Habari yako rafiki", 9);
    phone.tap(at);
    phone.tap(at);
    phone.tap(at);
    assert_eq!(phone.ui.selection(), Some((0, 18)));
    // Nothing more to select.
    assert_eq!(phone.menu(), ["Cut", "Copy", "Paste"]);
}

#[test]
fn a_long_press_selects_the_word_under_the_finger() {
    let mut phone = Phone::named("Habari yako rafiki");
    let at = phone.at("name", "Habari yako rafiki", 14);
    phone.touch(PointerPhase::Down, at);
    phone.wait(0);
    // Asked to be woken when the press has lasted long enough.
    assert_eq!(phone.frame.redraw_after(), Some(Duration::from_millis(500)));
    assert!(!phone.ui.has_focus());
    phone.wait(300);
    assert!(!phone.ui.has_focus(), "not yet");
    phone.wait(250);
    assert_eq!(phone.ui.selection(), Some((12, 18)));
    assert!(phone.ui.selection_menu_shown());
    // Lifting the finger is not a tap: the selection stays.
    phone.touch(PointerPhase::Up, at);
    phone.wait(10);
    assert_eq!(phone.selected(), Some("rafiki"));
    assert_eq!(phone.discs().len(), 2);
}

#[test]
fn a_finger_that_moves_or_lifts_early_is_not_a_long_press() {
    let mut phone = Phone::named("Habari yako rafiki");
    let at = phone.at("name", "Habari yako rafiki", 14);
    phone.touch(PointerPhase::Down, at);
    phone.wait(0);
    phone.touch(PointerPhase::Moved, Point::new(at.x + 60.0, at.y));
    phone.wait(600);
    assert_eq!(phone.ui.selection(), None);
    phone.touch(PointerPhase::Up, Point::new(at.x + 60.0, at.y));
    phone.wait(10);
    assert_eq!(phone.ui.selection(), None);
}

#[test]
fn a_long_press_on_an_empty_field_offers_only_paste() {
    let mut phone = Phone::named("");
    let at = Phone::center(phone.rect("name"));
    phone.long_press(at);
    assert!(phone.ui.is_focused("name"));
    assert_eq!(phone.ui.selection(), None);
    assert_eq!(phone.menu(), ["Paste"]);
    assert_eq!(phone.ui.selection_handles(), [None, None]);
    // A field nothing listens to offers nothing to do with emptiness, and
    // only copying of what it holds.
    let fixed = Phone::center(phone.rect("fixed"));
    let fixed = Point::new(phone.rect("fixed").x + 20.0, fixed.y);
    phone.long_press(fixed);
    assert_eq!(phone.ui.selection(), Some((0, 4)));
    assert_eq!(phone.menu(), ["Copy", "Select all"]);
}

#[test]
fn typing_replaces_the_selection_and_backspace_removes_it() {
    let mut phone = Phone::named("Habari yako rafiki");
    let at = phone.at("name", "Habari yako rafiki", 9);
    phone.double_tap(at);
    phone.key(TextInputEvent::Commit("zetu".to_owned()));
    assert_eq!(phone.ui.app().name, "Habari zetu rafiki");
    assert_eq!((phone.ui.caret(), phone.ui.selection()), (Some(11), None));
    assert!(
        !phone.ui.selection_menu_shown(),
        "typing dismisses the menu"
    );
    assert!(phone.menu().is_empty());
    assert!(phone.discs().is_empty());

    phone.wait(600);
    phone.double_tap(at);
    phone.key(TextInputEvent::DeleteBackward);
    assert_eq!(phone.ui.app().name, "Habari  rafiki");
    assert_eq!(phone.ui.caret(), Some(7));
}

#[test]
fn a_tap_elsewhere_in_the_field_collapses_the_selection_and_outside_it_ends_editing() {
    let mut phone = Phone::named("Habari yako rafiki");
    let at = phone.at("name", "Habari yako rafiki", 9);
    phone.double_tap(at);
    phone.wait(600);
    let start = phone.at("name", "Habari yako rafiki", 3);
    phone.tap(start);
    assert_eq!((phone.ui.selection(), phone.ui.caret()), (None, Some(3)));
    assert!(phone.menu().is_empty());

    phone.wait(600);
    phone.double_tap(at);
    assert!(phone.ui.selection_menu_shown());
    phone.tap(Point::new(540.0, 2000.0));
    assert!(!phone.ui.has_focus());
    assert!(phone.menu().is_empty());
    assert!(phone.highlights().is_empty());
}

#[test]
fn a_tap_can_put_the_caret_between_the_letters_of_a_ligature() {
    // Noto Sans draws "fi" as one glyph.
    let text = "rafiki";
    let mut phone = Phone::named(text);
    let (f, i, k) = (
        phone.at("name", text, 2),
        phone.at("name", text, 3),
        phone.at("name", text, 4),
    );
    assert!(f.x < i.x && i.x < k.x);
    phone.tap(Point::new(i.x + 1.0, i.y));
    assert_eq!(phone.ui.caret(), Some(3));
    phone.wait(600);
    phone.tap(Point::new(k.x - 1.0, i.y));
    assert_eq!(phone.ui.caret(), Some(4));
    phone.wait(600);
    phone.tap(Point::new(f.x + 1.0, i.y));
    assert_eq!(phone.ui.caret(), Some(2));
}

// ----------------------------------------------------------------- handles

#[test]
fn dragging_the_end_handle_extends_the_selection_and_the_start_handle_shrinks_it() {
    let text = "Habari yako rafiki";
    let mut phone = Phone::named(text);
    let at = phone.at("name", text, 9);
    phone.double_tap(at);
    let [_, end] = phone.ui.selection_handles();
    let grab = Phone::center(end.unwrap());
    let target = phone.at("name", text, 15);
    // The finger stays as far below the line as where it took hold.
    let to = Point::new(grab.x + (target.x - phone.at("name", text, 11).x), grab.y);
    phone.touch(PointerPhase::Down, grab);
    phone.wait(10);
    phone.touch(PointerPhase::Moved, to);
    phone.wait(16);
    assert_eq!(phone.ui.selection(), Some((7, 15)));
    assert!(phone.menu().is_empty(), "no menu while a handle is held");
    assert_eq!(phone.discs().len(), 2);
    phone.touch(PointerPhase::Up, to);
    phone.wait(10);
    assert_eq!(phone.selected(), Some("yako raf"));
    assert!(
        phone.ui.selection_menu_shown(),
        "and back when it is let go"
    );

    // The start handle, to the right: the selection shrinks from the left.
    let [start, _] = phone.ui.selection_handles();
    let grab = Phone::center(start.unwrap());
    let by = phone.at("name", text, 9).x - phone.at("name", text, 7).x;
    phone.drag(grab, Point::new(grab.x + by, grab.y), 4, 0);
    assert_eq!(phone.ui.selection(), Some((9, 15)));

    // Dragged past the other end, the two ends never meet: the selection
    // turns over and the caret end is the one that moved.
    let [start, _] = phone.ui.selection_handles();
    let grab = Phone::center(start.unwrap());
    let by = phone.at("name", text, 17).x - phone.at("name", text, 9).x;
    phone.drag(grab, Point::new(grab.x + by, grab.y), 4, 0);
    assert_eq!(phone.ui.selection(), Some((15, 17)));
    assert_eq!(phone.ui.caret(), Some(17));
}

#[test]
fn a_handle_dragged_down_selects_across_wrapped_lines() {
    let mut phone = Phone::new(Form {
        note: NOTE.to_owned(),
        ..Form::default()
    });
    let note = phone.rect("note");
    let line = note.height / 3.0;
    // The first word of the first line.
    phone.double_tap(Point::new(note.x + 30.0, note.y + line / 2.0));
    assert_eq!(phone.selected(), Some("Fundi"));
    let [_, end] = phone.ui.selection_handles();
    let grab = Phone::center(end.unwrap());
    // One line down and well to the right.
    let to = Point::new(grab.x + 400.0, grab.y + line);
    phone.drag(grab, to, 6, 0);
    let (start, end) = phone.ui.selection().unwrap();
    assert_eq!(start, 0);
    assert!(end > 40, "into the second line: {end}");
    // One rectangle per line: the first runs from the text's left edge to
    // the end of its line, the second starts at the left edge again.
    let highlights = phone.highlights();
    assert_eq!(highlights.len(), 2, "{highlights:?}");
    assert!((highlights[0].x - note.x).abs() <= 1.0);
    assert!((highlights[1].x - note.x).abs() <= 1.0);
    assert_eq!(highlights[0].y + highlights[0].height, highlights[1].y);
    assert!(highlights[0].width > highlights[1].width);
    // The handles hang from different lines.
    let [start, end] = phone.ui.selection_handles();
    assert!(end.unwrap().y > start.unwrap().y + line / 2.0);
}

#[test]
fn a_handle_held_at_the_edge_of_a_field_scrolls_its_text() {
    // A single line wider than its field.
    let text = "Habari yako rafiki yangu wa zamani kutoka Dar es Salaam na Arusha";
    let mut phone = Phone::named(text);
    let at = phone.at("name", text, 2);
    phone.double_tap(at);
    assert_eq!(phone.selected(), Some("Habari"));
    let name = phone.rect("name");
    let [_, end] = phone.ui.selection_handles();
    let grab = Phone::center(end.unwrap());
    // Held just inside the right edge for a second and a half.
    let to = Point::new(name.x + name.width - 6.0, grab.y);
    phone.touch(PointerPhase::Down, grab);
    phone.wait(10);
    phone.touch(PointerPhase::Moved, to);
    phone.wait(16);
    let early = phone.ui.selection().unwrap().1;
    assert!(phone.draw(), "frames keep coming while the text moves");
    for _ in 0..90 {
        phone.wait(16);
    }
    let late = phone.ui.selection().unwrap().1;
    assert!(late > early + 5, "{early} -> {late}");
    assert!(
        late >= text.len() - 1,
        "to the end, or the letter under the finger"
    );
    assert!(
        !phone.draw(),
        "and stop when there is nothing left to scroll"
    );
    phone.touch(PointerPhase::Up, to);
    phone.wait(10);
    assert_eq!(phone.ui.selection(), Some((0, late)));

    // A field of three lines holding more: held at its bottom edge, the
    // lines come up.
    let mut phone = Phone::new(Form {
        note: format!("{NOTE} {NOTE}"),
        ..Form::default()
    });
    let note = phone.rect("note");
    phone.double_tap(Point::new(note.x + 30.0, note.y + 20.0));
    let [_, end] = phone.ui.selection_handles();
    let grab = Phone::center(end.unwrap());
    let to = Point::new(grab.x, note.y + note.height + grab.y - note.y);
    phone.drag(grab, to, 4, 60);
    assert!(phone.ui.scroll_offset("note").unwrap() > 20.0);
    assert!(phone.ui.selection().unwrap().1 > 100);
}

#[test]
fn a_handle_held_at_the_edge_of_the_screen_scrolls_the_page() {
    let mut phone = Phone::new(Form {
        note: NOTE.to_owned(),
        tall: true,
        ..Form::default()
    });
    phone.ui.scroll_to(ScrollTo::offset("scroll", 300.0));
    phone.draw();
    let note = phone.rect("note");
    phone.double_tap(Point::new(note.x + 30.0, note.y + 20.0));
    assert_eq!(phone.selected(), Some("Fundi"));
    let before = phone.ui.scroll_offset("scroll").unwrap();
    let [start, _] = phone.ui.selection_handles();
    let grab = Phone::center(start.unwrap());
    // Up to the top of the screen and held there.
    phone.drag(grab, Point::new(grab.x, 20.0), 6, 40);
    let after = phone.ui.scroll_offset("scroll").unwrap();
    assert!(after < before - 30.0, "{before} -> {after}");
}

#[test]
fn scrolling_the_page_dismisses_the_menu_and_keeps_the_selection() {
    let mut phone = Phone::new(Form {
        name: "Habari yako".to_owned(),
        tall: true,
        ..Form::default()
    });
    phone.ui.scroll_to(ScrollTo::offset("scroll", 300.0));
    phone.draw();
    let name = phone.rect("name");
    phone.double_tap(Point::new(name.x + 30.0, name.y + name.height / 2.0));
    assert_eq!(phone.menu().len(), 4);
    phone.drag(Point::new(540.0, 1800.0), Point::new(540.0, 1500.0), 6, 0);
    assert!(!phone.ui.selection_menu_shown());
    assert_eq!(phone.ui.selection(), Some((0, 6)));
    assert_eq!(phone.discs().len(), 2, "the handles ride along");
}

// --------------------------------------------------------------- clipboard

#[test]
fn copy_cut_and_paste_through_the_menu() {
    let text = "Habari yako rafiki";
    let mut phone = Phone::named(text);
    let at = phone.at("name", text, 9);
    phone.double_tap(at);
    phone.choose("Copy");
    assert_eq!(phone.ui.clipboard(), Some("yako"));
    assert_eq!(phone.ui.app().name, text);
    assert!(!phone.ui.selection_menu_shown());
    assert_eq!(phone.ui.selection(), Some((7, 11)), "copying keeps it");
    assert!(phone.ui.pending_effects().is_empty());

    // Cut the last word.
    phone.wait(600);
    let last = phone.at("name", text, 15);
    phone.double_tap(last);
    phone.choose("Cut");
    assert_eq!(phone.ui.clipboard(), Some("rafiki"));
    assert_eq!(phone.ui.app().name, "Habari yako ");
    assert_eq!(phone.ui.caret(), Some(12));

    // Paste it over the first.
    phone.wait(600);
    let first = phone.at("name", "Habari yako ", 2);
    phone.double_tap(first);
    phone.choose("Paste");
    // The answer of the clipboard is delivered with the next frame.
    assert_eq!(phone.ui.app().name, "rafiki yako ");
    assert_eq!((phone.ui.caret(), phone.ui.selection()), (Some(6), None));
}

#[test]
fn the_clipboard_is_read_only_when_paste_is_chosen() {
    let mut phone = Phone::new(Form::default());
    phone.ui.set_clipboard("kutoka kwingine");
    let at = Phone::center(phone.rect("name"));
    phone.long_press(at);
    assert_eq!(phone.menu(), ["Paste"]);
    assert_eq!(phone.ui.app().name, "", "showing the menu read nothing");
    phone.choose("Paste");
    assert_eq!(phone.ui.app().name, "kutoka kwingine");
    // An empty clipboard pastes nothing and breaks nothing.
    phone.ui.set_clipboard("");
    phone.key(TextInputEvent::Paste);
    phone.draw();
    assert_eq!(phone.ui.app().name, "kutoka kwingine");
}

#[test]
fn pasted_text_passes_the_fields_rules() {
    let mut phone = Phone::new(Form::default());
    phone.ui.set_clipboard("+255 712-345 678 901");
    let at = Phone::center(phone.rect("phone"));
    phone.tap(at);
    phone.key(TextInputEvent::Paste);
    phone.draw();
    // Digits only, and the first ten of them.
    assert_eq!(phone.ui.app().phone, "2557123456");

    // Line breaks become spaces in a field of one line and stay in a
    // field of several.
    phone.ui.set_clipboard("Juma\r\nMwangi\n");
    let at = Phone::center(phone.rect("name"));
    phone.tap(at);
    phone.key(TextInputEvent::Paste);
    phone.draw();
    assert_eq!(phone.ui.app().name, "Juma Mwangi");
    let at = Phone::center(phone.rect("note"));
    phone.tap(at);
    phone.key(TextInputEvent::Paste);
    phone.draw();
    assert_eq!(phone.ui.app().note, "Juma\nMwangi\n");
}

#[test]
fn a_password_can_be_pasted_into_and_never_copied_out_of() {
    let mut phone = Phone::new(Form {
        secret: "siri kuu".to_owned(),
        ..Form::default()
    });
    let at = Phone::center(phone.rect("secret"));
    let at = Point::new(phone.rect("secret").x + 20.0, at.y);
    phone.long_press(at);
    // Bullets have no words: all of it is selected.
    assert_eq!(phone.ui.selection(), Some((0, 8)));
    assert_eq!(phone.menu(), ["Paste"]);
    phone.key(TextInputEvent::Copy);
    assert_eq!(phone.ui.clipboard(), None);
    phone.key(TextInputEvent::Cut);
    assert_eq!(phone.ui.clipboard(), None);
    assert_eq!(phone.ui.app().secret, "siri kuu", "nor cut");
    phone.ui.set_clipboard("mpya");
    phone.key(TextInputEvent::Paste);
    phone.draw();
    assert_eq!(phone.ui.app().secret, "mpya");
}

#[test]
fn keyboard_shortcuts_select_copy_cut_and_paste() {
    let mut phone = Phone::named("Habari yako");
    let at = phone.at("name", "Habari yako", 11);
    phone.tap(at);
    phone.key(TextInputEvent::SelectAll);
    assert_eq!(phone.ui.selection(), Some((0, 11)));
    assert!(phone.menu().is_empty(), "a key brings no menu");
    assert_eq!(phone.ui.selection_handles(), [None, None]);
    assert_eq!(phone.highlights().len(), 1);
    phone.key(TextInputEvent::Cut);
    assert_eq!(phone.ui.app().name, "");
    phone.key(TextInputEvent::Paste);
    phone.draw();
    phone.key(TextInputEvent::Paste);
    phone.draw();
    assert_eq!(phone.ui.app().name, "Habari yakoHabari yako");
}

#[test]
fn an_application_can_use_the_clipboard_itself() {
    let mut phone = Phone::new(Form::default());
    phone.ui.send(Msg::CopyCode);
    assert_eq!(phone.ui.clipboard(), Some("4471"));
    phone.ui.send(Msg::AskPaste);
    assert_eq!(phone.ui.app().pasted, None, "not inside the update");
    phone.draw();
    assert_eq!(phone.ui.app().pasted, Some(Some("4471".to_owned())));
}

// -------------------------------------------------------------------- keys

#[test]
fn arrows_home_and_end_move_the_caret_and_shift_selects() {
    let text = "Habari yako rafiki";
    let mut phone = Phone::named(text);
    let at = phone.at("name", text, 9);
    phone.tap(at);
    phone.go(CaretMove::Left, false);
    assert_eq!(phone.ui.caret(), Some(8));
    phone.go(CaretMove::Right, false);
    phone.go(CaretMove::Right, false);
    assert_eq!(phone.ui.caret(), Some(10));
    phone.go(CaretMove::WordLeft, false);
    assert_eq!(phone.ui.caret(), Some(7));
    phone.go(CaretMove::WordRight, true);
    assert_eq!(phone.ui.selection(), Some((7, 11)));
    phone.go(CaretMove::WordRight, true);
    assert_eq!(phone.ui.selection(), Some((7, 18)));
    phone.go(CaretMove::Left, true);
    assert_eq!(phone.ui.selection(), Some((7, 17)));
    // Without Shift a selection collapses to the side the key points at.
    phone.go(CaretMove::Left, false);
    assert_eq!((phone.ui.selection(), phone.ui.caret()), (None, Some(7)));
    phone.go(CaretMove::LineEnd, true);
    assert_eq!(phone.ui.selection(), Some((7, 18)));
    phone.go(CaretMove::LineStart, false);
    assert_eq!((phone.ui.selection(), phone.ui.caret()), (None, Some(0)));
    phone.go(CaretMove::TextEnd, true);
    assert_eq!(phone.ui.selection(), Some((0, 18)));
    phone.go(CaretMove::TextStart, false);
    assert_eq!(phone.ui.caret(), Some(0));
    // At the ends, nothing.
    phone.go(CaretMove::Left, false);
    assert_eq!(phone.ui.caret(), Some(0));
}

#[test]
fn up_and_down_move_between_wrapped_lines() {
    let mut phone = Phone::new(Form {
        note: NOTE.to_owned(),
        ..Form::default()
    });
    let note = phone.rect("note");
    phone.tap(Point::new(note.x + 300.0, note.y + 20.0));
    let first = phone.ui.caret().unwrap();
    assert!(first > 3 && first < 40);
    phone.go(CaretMove::Down, false);
    let second = phone.ui.caret().unwrap();
    assert!(second > first + 20, "{first} -> {second}");
    phone.go(CaretMove::LineStart, false);
    let start = phone.ui.caret().unwrap();
    assert!(start > first && start < second);
    assert_eq!(
        &NOTE[start - 1..start],
        " ",
        "the line starts after a space"
    );
    phone.go(CaretMove::LineEnd, true);
    let (_, end) = phone.ui.selection().unwrap();
    assert!(end > second);
    assert_eq!(phone.highlights().len(), 1, "one line, to its end");
    phone.go(CaretMove::Up, false);
    phone.go(CaretMove::Up, false);
    assert_eq!(phone.ui.caret(), Some(0), "up from the first line");
    phone.go(CaretMove::Down, false);
    phone.go(CaretMove::Down, false);
    phone.go(CaretMove::Down, false);
    phone.go(CaretMove::Down, false);
    assert_eq!(phone.ui.caret(), Some(NOTE.len()), "down from the last");
}

#[test]
fn undo_takes_back_a_run_of_typing_and_redo_makes_it_again() {
    let mut phone = Phone::named("");
    let at = Phone::center(phone.rect("name"));
    phone.tap(at);
    for letter in ["H", "a", "b", " "] {
        phone.key(TextInputEvent::Commit(letter.to_owned()));
    }
    for letter in ["y", "a"] {
        phone.key(TextInputEvent::Commit(letter.to_owned()));
    }
    phone.key(TextInputEvent::DeleteBackward);
    phone.key(TextInputEvent::DeleteBackward);
    assert_eq!(phone.ui.app().name, "Hab ");
    phone.key(TextInputEvent::Undo);
    assert_eq!(phone.ui.app().name, "Hab ya", "the backspaces");
    phone.key(TextInputEvent::Undo);
    assert_eq!(phone.ui.app().name, "Hab ", "the second word");
    phone.key(TextInputEvent::Undo);
    assert_eq!(phone.ui.app().name, "Hab", "the space");
    phone.key(TextInputEvent::Undo);
    assert_eq!(phone.ui.app().name, "", "the first word");
    phone.key(TextInputEvent::Undo);
    assert_eq!(phone.ui.app().name, "", "nothing further back");
    phone.key(TextInputEvent::Redo);
    phone.key(TextInputEvent::Redo);
    assert_eq!(phone.ui.app().name, "Hab ");
    assert_eq!(phone.ui.caret(), Some(4));
    // A new edit ends what could be made again.
    phone.key(TextInputEvent::Commit("x".to_owned()));
    phone.key(TextInputEvent::Redo);
    assert_eq!(phone.ui.app().name, "Hab x");
}

// ------------------------------------------------------- the input method

#[test]
fn the_input_method_is_told_the_selection_and_its_own_is_honoured() {
    let text = "Habari yako rafiki";
    let mut phone = Phone::named(text);
    let at = phone.at("name", text, 9);
    phone.tap(at);
    let request = phone.frame.text_input().unwrap();
    assert_eq!((request.caret, request.anchor), (9, None));
    phone.wait(600);
    phone.double_tap(at);
    let request = phone.frame.text_input().unwrap();
    assert_eq!((request.caret, request.anchor), (11, Some(7)));

    // The input method selects another range itself (`setSelection`).
    phone.key(TextInputEvent::Edited(TextEditState {
        text: text.to_owned(),
        caret: 6,
        anchor: Some(0),
        composing: None,
    }));
    assert_eq!(phone.ui.selection(), Some((0, 6)));
    assert_eq!(phone.ui.caret(), Some(6));
    let request = phone.frame.text_input().unwrap();
    assert_eq!((request.caret, request.anchor), (6, Some(0)));
    assert_eq!(phone.highlights().len(), 1);
    // And replaces it by typing.
    phone.key(TextInputEvent::Edited(TextEditState {
        text: "Salamu yako rafiki".to_owned(),
        caret: 6,
        anchor: None,
        composing: None,
    }));
    assert_eq!(phone.ui.app().name, "Salamu yako rafiki");
    assert_eq!(phone.ui.selection(), None);
    assert!(phone.highlights().is_empty());
}

// -------------------------------------------------------- right to left

#[test]
fn arabic_is_selected_by_whole_characters_from_right_to_left() {
    // "marhaban bik": two words, the first with vowel marks.
    let text = "مَرْحَبًا بك";
    let mut phone = Phone::named(text);
    let name = phone.rect("name");
    let style = TextStyle::new(15.0 * SCALE, Color::BLACK);
    let width = phone.frame.measure_text(text, &style, None).width;
    let y = name.y + name.height / 2.0;
    // A tap anywhere lands on a character boundary, never inside a
    // letter and its marks.
    let boundaries: Vec<usize> = {
        use std::collections::BTreeSet;
        let mut seen = BTreeSet::new();
        for step in 0..=40 {
            phone.wait(600);
            phone.tap(Point::new(name.x + width * step as f32 / 40.0, y));
            seen.insert(phone.ui.caret().unwrap());
        }
        seen.into_iter().collect()
    };
    assert_eq!(boundaries.first(), Some(&0));
    assert_eq!(boundaries.last(), Some(&text.len()));
    // Five letters with marks, a space, two letters: eight characters,
    // nine places for a caret, out of 23 bytes.
    assert_eq!(boundaries.len(), 9, "{boundaries:?}");
    for boundary in &boundaries {
        let rest = &text[*boundary..];
        assert!(
            rest.chars()
                .next()
                .is_none_or(|first| !('\u{064B}'..='\u{0652}').contains(&first)),
            "a caret before a mark at {boundary}"
        );
    }

    // The first word is on the right. A double tap there selects it.
    phone.wait(600);
    phone.double_tap(Point::new(name.x + width - 40.0, y));
    let first = text.find(' ').unwrap();
    assert_eq!(phone.ui.selection(), Some((0, first)));
    let highlights = phone.highlights();
    assert_eq!(highlights.len(), 1);
    assert!(
        (highlights[0].x + highlights[0].width - (name.x + width)).abs() <= 2.0,
        "the highlight ends at the right edge of the text"
    );
    // Its start hangs on the right and its end on the left.
    let [start, end] = phone.ui.selection_handles();
    assert!(start.unwrap().x > end.unwrap().x);

    // The Left key moves on through the text, the Right key back.
    phone.go(CaretMove::Left, false);
    assert_eq!(phone.ui.caret(), Some(first), "collapsed to the left end");
    phone.go(CaretMove::Left, false);
    assert_eq!(phone.ui.caret(), Some(first + 1));
    phone.go(CaretMove::Right, false);
    phone.go(CaretMove::Right, false);
    assert_eq!(
        phone.ui.caret(),
        Some(boundaries[4]),
        "back over one letter and its mark"
    );
}

#[test]
fn a_selection_across_two_directions_is_drawn_as_separate_runs() {
    // Latin, then Arabic, then Latin, in a left-to-right line.
    let text = "abc مرحبا xyz";
    let mut phone = Phone::named(text);
    let at = Phone::center(phone.rect("name"));
    phone.tap(at);
    // From the middle of "abc" to the middle of the Arabic word: on
    // screen that is "bc " and, further right, the word's first letters.
    let middle = text.find('ح').unwrap();
    assert!(phone.ui.select("name", 1..middle));
    phone.draw();
    assert_eq!(phone.ui.selection(), Some((1, middle)));
    let highlights = phone.highlights();
    assert_eq!(highlights.len(), 2, "{highlights:?}");
    assert!(highlights[0].x + highlights[0].width < highlights[1].x);
}

// ------------------------------------------------------- programmatic API

#[test]
fn a_selection_can_be_made_and_read_from_outside() {
    let mut phone = Phone::named("Habari yako");
    assert_eq!(phone.ui.selection(), None);
    assert!(!phone.ui.select("nobody", 0..3));
    assert!(phone.ui.select("name", 7..11));
    assert!(phone.ui.is_focused("name"));
    assert_eq!(phone.ui.selection(), Some((7, 11)));
    phone.draw();
    assert_eq!(phone.highlights().len(), 1);
    assert_eq!(phone.discs().len(), 2);
    assert!(phone.menu().is_empty());
    // Out of range is clamped; an empty range is a caret.
    assert!(phone.ui.select("name", 7..99));
    assert_eq!(phone.ui.selection(), Some((7, 11)));
    assert!(phone.ui.select("name", 3..3));
    assert_eq!((phone.ui.selection(), phone.ui.caret()), (None, Some(3)));
}

#[test]
fn a_field_without_focus_draws_nothing_new() {
    let mut phone = Phone::named("Habari yako");
    let before = phone.frame.list.ops().to_vec();
    let at = phone.at("name", "Habari yako", 9);
    phone.double_tap(at);
    assert_ne!(phone.frame.list.ops(), &before[..]);
    phone.ui.clear_focus();
    phone.draw();
    assert_eq!(phone.frame.list.ops(), &before[..]);
    assert_eq!(phone.frame.redraw_after(), None);
}

// --------------------------------------------- system bars, reduce motion

#[test]
fn the_system_bars_follow_the_theme_unless_the_application_says() {
    let mut phone = Phone::new(Form::default());
    assert_eq!(phone.frame.system_bars(), Some(SystemBars::DARK_CONTENT));
    phone.ui.app_mut().dark = true;
    phone.draw();
    assert_eq!(phone.frame.system_bars(), Some(SystemBars::LIGHT_CONTENT));
    // A screen that knows better: dark icons at the top only.
    let mixed = SystemBars {
        status: BarContent::Dark,
        navigation: BarContent::Light,
    };
    phone.ui.app_mut().bars = Some(mixed);
    phone.draw();
    assert_eq!(phone.frame.system_bars(), Some(mixed));
}

#[test]
fn reduce_motion_is_taken_from_the_host() {
    let mut phone = Phone::new(Form {
        tall: true,
        ..Form::default()
    });
    phone
        .ui
        .scroll_to(ScrollTo::offset("scroll", 200.0).animated());
    assert!(phone.draw(), "gliding");
    assert!(phone.ui.scroll_offset("scroll").unwrap() < 200.0);
    phone.wait(1000);

    phone.info = phone.info.with_reduce_motion(true);
    phone
        .ui
        .scroll_to(ScrollTo::offset("scroll", 400.0).animated());
    assert!(!phone.draw(), "at once");
    assert_eq!(phone.ui.scroll_offset("scroll"), Some(400.0));

    // The application's own switch holds whatever the host says.
    phone.info = phone.info.with_reduce_motion(false);
    phone.ui.set_reduce_motion(true);
    phone
        .ui
        .scroll_to(ScrollTo::offset("scroll", 100.0).animated());
    assert!(!phone.draw());
    assert_eq!(phone.ui.scroll_offset("scroll"), Some(100.0));
}
