//! `TextInput`: focus, editing, the keyboard request, the caret, and
//! keeping the field clear of the keyboard. Driven the way the engine
//! drives a `Ui`: frames on demand, pointer and text events in between.

use std::time::Duration;

use morningstar_core::{
    Border, DrawOp, Point, PointerEvent, PointerPhase, Rect, SurfaceSize, TextEditState,
    TextInputEvent,
};
use morningstar_runtime::{App, Frame, FrameInfo};
use morningstar_ui::prelude::*;

const REGULAR: &[u8] =
    include_bytes!("../../../examples/hello-gpu/assets/fonts/NotoSans-Regular.ttf");
const ARABIC: &[u8] =
    include_bytes!("../../../examples/hello-gpu/assets/fonts/NotoSansArabic-Regular.ttf");

const INK: Color = hex(0x14201A);
const GREY: Color = hex(0x6B756F);
const OUTLINE: Color = hex(0xD9E0DB);
const GREEN: Color = hex(0x1E8416);
const CARET: Color = hex(0xC2185B);

#[derive(Clone, Debug, PartialEq)]
enum Msg {
    Name(String),
    Phone(String),
    Secret(String),
    Note(String),
    Code(usize, String),
    Submit,
    Eye,
    Button,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Screen {
    #[default]
    Form,
    /// The same fields far down a scrolling page.
    Tall,
    Codes,
    /// A look-alike of the name field made of plain `Text`.
    Lookalike,
    Empty,
}

#[derive(Default)]
struct Form {
    screen: Screen,
    name: String,
    phone: String,
    secret: String,
    note: String,
    code: [String; 3],
    hidden: bool,
    /// `update` ignores new names: the application refuses the edit.
    refuse_names: bool,
    log: Vec<Msg>,
}

fn frame(input: impl Into<Element<Msg>>) -> Container<Msg> {
    Row::new()
        .push(input)
        .fill_width()
        .height(52.0)
        .cross_align(CrossAlign::Center)
        .padding_xy(16.0, 0.0)
        .radius(12.0)
        .border(1.0, OUTLINE)
        .focused_border(2.0, GREEN)
}

fn input(value: &str) -> TextInput<Msg> {
    TextInput::new(value)
        .size(15.0)
        .color(INK)
        .placeholder_color(GREY)
        .caret_color(CARET)
}

fn fields(state: &Form) -> Container<Msg> {
    col![
        frame(
            input(&state.name)
                .placeholder("Full name")
                .action(ImeAction::Next)
                .capitalization(Capitalization::Words)
                .on_input(Msg::Name)
                .id("name")
        )
        .id("name-frame"),
        frame(
            input(&state.phone)
                .keyboard(KeyboardType::Phone)
                .digits_only()
                .max_length(10)
                .action(ImeAction::Next)
                .on_input(Msg::Phone)
                .id("phone")
        ),
        frame(
            row![
                input(&state.secret)
                    .placeholder("Password")
                    .secure(state.hidden)
                    .on_input(Msg::Secret)
                    .on_submit(Msg::Submit)
                    .id("secret"),
                Container::new(text("eye"))
                    .size(44.0, 44.0)
                    .on_press(Msg::Eye)
                    .id("eye"),
            ]
            .fill_width()
            .cross_align(CrossAlign::Center)
        )
        .id("secret-frame"),
        Container::new(
            input(&state.note)
                .multiline()
                .placeholder("Notes")
                .on_input(Msg::Note)
                .id("note")
        )
        .fill_width()
        .min_height(60.0)
        .padding(12.0)
        .input_frame()
        .id("note-frame"),
        Button::new("Go").on_press(Msg::Button).id("button"),
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

    fn view(&self) -> Element<Msg> {
        match self.screen {
            Screen::Form => col![fields(self)].padding(20.0).fill().id("form").into(),
            Screen::Tall => col![
                ScrollView::new(
                    col![Container::empty().height(900.0).fill_width(), fields(self)]
                        .padding(20.0)
                        .fill_width()
                )
                .id("scroll"),
                Container::new(
                    Button::new("Save")
                        .on_press(Msg::Button)
                        .fill_width()
                        .id("save")
                )
                .fill_width()
                .padding(16.0)
                .safe_area(SafeEdges::BOTTOM)
                .id("bar"),
            ]
            .fill()
            .id("tall")
            .into(),
            Screen::Codes => Row::new()
                .extend((0..3).map(|index| {
                    Container::new(
                        input(&self.code[index])
                            .align(TextAlign::Center)
                            .keyboard(KeyboardType::Number)
                            .digits_only()
                            .max_length(1)
                            .auto_advance()
                            .on_input(move |value| Msg::Code(index, value))
                            .id(("code", index)),
                    )
                    .center()
                    .size(56.0, 56.0)
                    .border(1.0, OUTLINE)
                    .focused_border(2.0, GREEN)
                    .id(("box", index))
                }))
                .gap(8.0)
                .padding(20.0)
                .id("codes")
                .into(),
            Screen::Lookalike => col![
                Row::new()
                    .push(
                        text(if self.name.is_empty() {
                            "Full name".to_owned()
                        } else {
                            self.name.clone()
                        })
                        .size(15.0)
                        .color(if self.name.is_empty() { GREY } else { INK })
                        .fill_width()
                    )
                    .fill_width()
                    .height(52.0)
                    .cross_align(CrossAlign::Center)
                    .padding_xy(16.0, 0.0)
                    .radius(12.0)
                    .border(1.0, OUTLINE)
            ]
            .padding(20.0)
            .fill()
            .into(),
            Screen::Empty => text("nothing").into(),
        }
    }

    fn update(&mut self, message: Msg) {
        self.log.push(message.clone());
        match message {
            Msg::Name(name) if !self.refuse_names => self.name = name,
            Msg::Phone(phone) => self.phone = phone,
            Msg::Secret(secret) => self.secret = secret,
            Msg::Note(note) => self.note = note,
            Msg::Code(index, value) => self.code[index] = value,
            Msg::Eye => self.hidden = !self.hidden,
            _ => {}
        }
    }
}

const SCALE: f32 = 3.0;

struct Harness {
    ui: Ui<Form>,
    frame: Frame,
    info: FrameInfo,
    time: Duration,
}

impl Harness {
    fn new(form: Form) -> Self {
        let mut ui = Ui::new(form);
        let frame = Frame::for_app(&mut ui);
        let mut harness = Self {
            ui,
            frame,
            info: FrameInfo::new(SurfaceSize::new(1080, 2400)).with_scale(SCALE),
            time: Duration::ZERO,
        };
        harness.draw();
        harness
    }

    fn on(screen: Screen) -> Self {
        Self::new(Form {
            screen,
            ..Form::default()
        })
    }

    fn draw(&mut self) {
        self.frame.begin();
        self.ui
            .frame(&self.info.with_elapsed(self.time), &mut self.frame);
    }

    fn step(&mut self, millis: u64) {
        self.time += Duration::from_millis(millis);
        self.draw();
    }

    /// A tap at a position in pixels, as the engine delivers it.
    fn tap_px(&mut self, x: f32, y: f32) {
        for phase in [PointerPhase::Down, PointerPhase::Up] {
            self.ui.pointer(&PointerEvent {
                id: 1,
                phase,
                position: Point::new(x, y),
            });
            self.draw();
        }
    }

    fn rect(&self, id: impl std::hash::Hash) -> Rect {
        self.ui.rect_of(id).expect("the widget is on screen")
    }

    /// Taps the middle of the widget with this id.
    fn tap(&mut self, id: impl std::hash::Hash) {
        let rect = self.rect(id);
        self.tap_px(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    }

    /// Delivers a text event and builds the frame that follows it.
    fn key(&mut self, event: TextInputEvent) {
        self.ui.text_input(&event);
        self.draw();
    }

    fn type_text(&mut self, text: &str) {
        for character in text.chars() {
            self.key(TextInputEvent::Commit(character.to_string()));
        }
    }

    fn app(&self) -> &Form {
        self.ui.app()
    }

    fn log(&self) -> &[Msg] {
        &self.ui.app().log
    }

    fn rects(&self, color: Color) -> Vec<Rect> {
        self.frame
            .list
            .ops()
            .iter()
            .filter_map(|op| match *op {
                DrawOp::Rect { rect, color: c, .. } if c == color => Some(rect),
                _ => None,
            })
            .collect()
    }

    fn caret_rect(&self) -> Option<Rect> {
        self.rects(CARET).first().copied()
    }

    fn borders(&self) -> Vec<Border> {
        self.frame
            .list
            .ops()
            .iter()
            .filter_map(|op| match *op {
                DrawOp::BorderedRect { border, .. } => Some(border),
                _ => None,
            })
            .collect()
    }

    fn glyphs(&self) -> Vec<Rect> {
        self.frame
            .list
            .ops()
            .iter()
            .filter_map(|op| match *op {
                DrawOp::Glyph { rect, .. } => Some(rect),
                _ => None,
            })
            .collect()
    }
}

// --- Fidelity ---------------------------------------------------------------

#[test]
fn an_unfocused_input_draws_exactly_what_a_text_would() {
    for name in ["", "Amina Juma"] {
        let mut real = Harness::new(Form {
            name: name.to_owned(),
            ..Form::default()
        });
        let mut fake = Harness::new(Form {
            name: name.to_owned(),
            screen: Screen::Lookalike,
            ..Form::default()
        });
        real.draw();
        fake.draw();
        // The first frame box and its text: the look-alike's whole list.
        let fake_ops = fake.frame.list.ops();
        let real_ops = &real.frame.list.ops()[..fake_ops.len()];
        assert_eq!(real_ops, fake_ops, "value {name:?}");
        assert!(real.frame.text_input().is_none());
        assert_eq!(real.frame.redraw_after(), None, "nothing blinks");
    }
}

// --- Focus --------------------------------------------------------------------

#[test]
fn a_tap_focuses_and_asks_for_the_keyboard_the_field_wants() {
    let mut h = Harness::on(Screen::Form);
    assert!(!h.ui.has_focus());
    h.tap("name");
    assert!(h.ui.is_focused("name"));

    let request = h.frame.text_input().expect("the frame asks for a keyboard");
    assert_eq!(request.text, "");
    assert_eq!(request.caret, 0);
    assert_eq!(request.traits.keyboard, KeyboardType::Text);
    assert_eq!(request.traits.capitalization, Capitalization::Words);
    assert_eq!(request.traits.action, ImeAction::Next);
    assert!(!request.traits.secure && !request.traits.multiline);
    let name_session = request.session;

    h.tap("phone");
    let request = h.frame.text_input().unwrap();
    assert_ne!(
        request.session, name_session,
        "another field, another session"
    );
    assert_eq!(request.traits.keyboard, KeyboardType::Phone);
    assert!(h.log().is_empty(), "focusing delivers no message");
}

#[test]
fn tapping_the_frame_focuses_its_input() {
    let mut h = Harness::on(Screen::Form);
    let frame = h.rect("name-frame");
    // In the frame's padding, left of the text.
    h.tap_px(frame.x + 6.0, frame.y + 6.0);
    assert!(h.ui.is_focused("name"));
}

#[test]
fn the_focused_frame_takes_its_focused_border() {
    let mut h = Harness::on(Screen::Form);
    let plain = Border::new(3.0, OUTLINE);
    assert!(h.borders().iter().all(|border| *border == plain));
    h.tap("name");
    let focused = Border::new(6.0, GREEN);
    assert_eq!(h.borders().iter().filter(|b| **b == focused).count(), 1);
    assert_eq!(h.borders()[0], focused, "the name frame is the first box");
}

#[test]
fn tapping_outside_or_pressing_a_button_gives_up_focus() {
    let mut h = Harness::on(Screen::Form);
    h.tap("name");
    // Empty space at the bottom of the screen.
    h.tap_px(540.0, 2300.0);
    assert!(!h.ui.has_focus());
    assert!(h.frame.text_input().is_none(), "the keyboard is let go");

    h.tap("name");
    h.tap("button");
    assert!(!h.ui.has_focus());
    assert_eq!(h.log(), [Msg::Button]);
}

#[test]
fn a_button_inside_the_fields_own_frame_keeps_the_focus() {
    let mut h = Harness::new(Form {
        hidden: true,
        secret: "siri".to_owned(),
        ..Form::default()
    });
    h.tap("secret");
    assert!(h.frame.text_input().unwrap().traits.secure);
    h.tap("eye");
    assert_eq!(h.log(), [Msg::Eye]);
    assert!(
        h.ui.is_focused("secret"),
        "the eye does not close the keyboard"
    );
    // Unmasked now: other traits, so the engine restarts the keyboard.
    assert!(!h.frame.text_input().unwrap().traits.secure);
}

#[test]
fn focus_ends_when_the_field_leaves_the_screen() {
    let mut h = Harness::on(Screen::Form);
    h.tap("name");
    h.ui.app_mut().screen = Screen::Empty;
    h.draw();
    assert!(!h.ui.has_focus());
    assert!(h.frame.text_input().is_none());
    // Text arriving late goes nowhere.
    h.key(TextInputEvent::Commit("x".to_owned()));
    assert!(h.log().is_empty());
}

#[test]
fn the_platform_dismissing_the_keyboard_clears_focus() {
    let mut h = Harness::on(Screen::Form);
    h.tap("name");
    h.key(TextInputEvent::FocusLost);
    assert!(!h.ui.has_focus());
    assert!(h.frame.text_input().is_none());
}

// --- Editing ------------------------------------------------------------------

#[test]
fn typing_delivers_the_whole_new_value_in_order() {
    let mut h = Harness::on(Screen::Form);
    h.tap("name");
    h.type_text("Juma");
    assert_eq!(
        h.log(),
        ["J", "Ju", "Jum", "Juma"].map(|name| Msg::Name(name.to_owned()))
    );
    assert_eq!(h.app().name, "Juma");
    assert_eq!(h.ui.caret(), Some(4));
    let request = h.frame.text_input().unwrap();
    assert_eq!((request.text.as_str(), request.caret), ("Juma", 4));
}

#[test]
fn several_events_before_a_frame_build_on_each_other() {
    let mut h = Harness::on(Screen::Form);
    h.tap("name");
    for letter in ["a", "b", "c"] {
        h.ui.text_input(&TextInputEvent::Commit(letter.to_owned()));
    }
    h.ui.text_input(&TextInputEvent::DeleteBackward);
    h.draw();
    assert_eq!(h.app().name, "ab");
    assert_eq!(h.log().len(), 4);
}

#[test]
fn backspace_removes_a_letter_with_its_marks() {
    let mut h = Harness::new(Form {
        name: "مَرْحَبًا".to_owned(),
        ..Form::default()
    });
    h.tap("name");
    // Put the caret at the end whatever the tap hit.
    h.key(TextInputEvent::Edited(TextEditState {
        text: "مَرْحَبًا".to_owned(),
        caret: "مَرْحَبًا".len(),
        composing: None,
    }));
    for remaining in (0..5).rev() {
        h.key(TextInputEvent::DeleteBackward);
        let name = &h.app().name;
        assert_eq!(
            name.chars()
                .filter(|c| ('\u{0621}'..='\u{064A}').contains(c))
                .count(),
            remaining,
            "{name:?}"
        );
    }
    assert_eq!(h.app().name, "");
    h.key(TextInputEvent::DeleteBackward);
    assert_eq!(h.log().len(), 5, "nothing left to delete, no message");
}

#[test]
fn a_tap_places_the_caret_under_the_finger() {
    let mut h = Harness::new(Form {
        name: "Habari za asubuhi".to_owned(),
        ..Form::default()
    });
    let field = h.rect("name");
    let y = field.y + field.height / 2.0;
    h.tap_px(field.x + 1.0, y);
    assert_eq!(h.ui.caret(), Some(0));
    h.tap_px(field.x + field.width - 2.0, y);
    assert_eq!(h.ui.caret(), Some(17), "past the text: its end");

    // Somewhere inside: the caret is drawn where the finger was.
    let x = field.x + 150.0;
    h.tap_px(x, y);
    let caret = h.ui.caret().unwrap();
    assert!(caret > 0 && caret < 17);
    let drawn = h.caret_rect().expect("the caret is drawn after a tap");
    assert!((drawn.x - x).abs() < 15.0, "{drawn:?} near {x}");
    // Typing goes in there.
    h.key(TextInputEvent::Commit("X".to_owned()));
    assert_eq!(h.app().name.find('X'), Some(caret));
    assert_eq!(h.ui.caret(), Some(caret + 1));
}

#[test]
fn a_tap_on_right_to_left_text_follows_its_direction() {
    let mut h = Harness::new(Form {
        name: "سلام".to_owned(),
        ..Form::default()
    });
    let field = h.rect("name");
    let y = field.y + field.height / 2.0;
    // The word starts at its right edge; its left edge is its end.
    h.tap_px(field.x + 1.0, y);
    assert_eq!(h.ui.caret(), Some("سلام".len()));
    let left = h.caret_rect().unwrap().x;
    h.tap_px(field.x + field.width - 2.0, y);
    assert_eq!(h.ui.caret(), Some(0));
    assert!(h.caret_rect().unwrap().x > left);
}

#[test]
fn composing_text_is_underlined_until_committed() {
    let mut h = Harness::on(Screen::Form);
    h.tap("name");
    assert!(h.rects(INK).is_empty());
    h.key(TextInputEvent::Compose("hab".to_owned()));
    assert_eq!(h.app().name, "hab");
    assert_eq!(h.ui.composing(), Some((0, 3)));
    let underline = h.rects(INK);
    assert_eq!(underline.len(), 1, "one underline in the text colour");
    assert!(underline[0].height == SCALE && underline[0].width > 30.0);

    h.key(TextInputEvent::Compose("habari".to_owned()));
    assert_eq!(h.app().name, "habari");
    h.key(TextInputEvent::Commit("Habari ".to_owned()));
    assert_eq!(h.app().name, "Habari ");
    assert_eq!(h.ui.composing(), None);
    assert!(h.rects(INK).is_empty());
}

#[test]
fn the_platforms_own_edit_is_adopted_with_its_composition() {
    let mut h = Harness::on(Screen::Form);
    h.tap("name");
    h.key(TextInputEvent::Edited(TextEditState {
        text: "Amina J".to_owned(),
        caret: 7,
        composing: Some((6, 7)),
    }));
    assert_eq!(h.app().name, "Amina J");
    assert_eq!(h.ui.caret(), Some(7));
    assert_eq!(h.ui.composing(), Some((6, 7)));
    // The request repeats what the platform holds: nothing to push back.
    let request = h.frame.text_input().unwrap();
    assert_eq!((request.text.as_str(), request.caret), ("Amina J", 7));
}

#[test]
fn a_filtered_field_asks_the_platform_to_drop_what_it_refused() {
    let mut h = Harness::on(Screen::Form);
    h.tap("phone");
    h.key(TextInputEvent::Edited(TextEditState {
        text: "07x".to_owned(),
        caret: 3,
        composing: Some((2, 3)),
    }));
    assert_eq!(h.app().phone, "07");
    assert_eq!(h.ui.composing(), None);
    let request = h.frame.text_input().unwrap();
    assert_eq!((request.text.as_str(), request.caret), ("07", 2));

    h.type_text("12345678 extra 99");
    assert_eq!(h.app().phone, "0712345678", "ten digits and no more");
}

#[test]
fn a_value_the_application_refuses_stays_as_the_application_has_it() {
    let mut h = Harness::new(Form {
        name: "Asha".to_owned(),
        refuse_names: true,
        ..Form::default()
    });
    h.tap("name");
    h.key(TextInputEvent::Edited(TextEditState {
        text: "Ashax".to_owned(),
        caret: 5,
        composing: Some((4, 5)),
    }));
    assert_eq!(h.log(), [Msg::Name("Ashax".to_owned())]);
    assert_eq!(h.app().name, "Asha");
    // The field shows, and reports to the platform, the real value.
    let request = h.frame.text_input().unwrap();
    assert_eq!((request.text.as_str(), request.caret), ("Asha", 4));
    assert_eq!(h.ui.composing(), None);
}

#[test]
fn a_secure_field_shows_bullets_and_can_be_unmasked() {
    let mut h = Harness::new(Form {
        hidden: true,
        ..Form::default()
    });
    h.tap("secret");
    h.type_text("siri1");
    assert_eq!(h.app().secret, "siri1");
    let field = h.rect("secret");
    let inside = |h: &Harness| {
        h.glyphs()
            .into_iter()
            .filter(|glyph| glyph.x >= field.x && glyph.x < field.x + field.width)
            .filter(|glyph| glyph.y >= field.y && glyph.y < field.y + field.height)
            .collect::<Vec<_>>()
    };
    let bullets = inside(&h);
    assert_eq!(bullets.len(), 5);
    assert!(
        bullets
            .windows(2)
            .all(|pair| pair[0].width == pair[1].width),
        "five identical bullets"
    );
    // The caret is after the fifth bullet.
    let caret = h.caret_rect().unwrap();
    assert!(caret.x >= bullets[4].x + bullets[4].width - 1.0);

    h.tap("eye");
    let letters = inside(&h);
    assert_eq!(letters.len(), 5);
    assert!(
        letters
            .windows(2)
            .any(|pair| pair[0].width != pair[1].width)
    );
    // The value the platform is given is the real one either way.
    assert_eq!(h.frame.text_input().unwrap().text, "siri1");
}

#[test]
fn enter_breaks_the_line_only_in_a_multiline_field() {
    let mut h = Harness::on(Screen::Form);
    h.tap("note");
    assert!(h.frame.text_input().unwrap().traits.multiline);
    let before = h.rect("note-frame").height;
    h.type_text("moja");
    h.key(TextInputEvent::Enter);
    h.type_text("mbili");
    assert_eq!(h.app().note, "moja\nmbili");
    assert!(
        h.rect("note-frame").height > before,
        "the field grew a line"
    );
    assert!(h.ui.is_focused("note"));
}

// --- Actions ------------------------------------------------------------------

#[test]
fn next_moves_through_the_fields_and_done_submits() {
    let mut h = Harness::on(Screen::Form);
    h.tap("name");
    h.type_text("A");
    h.key(TextInputEvent::Action(ImeAction::Next));
    assert!(h.ui.is_focused("phone"));
    h.type_text("07");
    // The Return key of a hardware keyboard does the field's own action.
    h.key(TextInputEvent::Enter);
    assert!(h.ui.is_focused("secret"));
    h.type_text("s");
    h.key(TextInputEvent::Action(ImeAction::Done));
    assert!(!h.ui.has_focus(), "done puts the keyboard away");
    assert_eq!(
        h.log(),
        [
            Msg::Name("A".to_owned()),
            Msg::Phone("0".to_owned()),
            Msg::Phone("07".to_owned()),
            Msg::Secret("s".to_owned()),
            Msg::Submit,
        ]
    );
}

#[test]
fn next_on_the_last_field_just_closes_the_keyboard() {
    let mut h = Harness::on(Screen::Form);
    h.tap("note");
    h.key(TextInputEvent::Action(ImeAction::Next));
    assert!(!h.ui.has_focus());
    assert!(h.log().is_empty());
}

#[test]
fn code_boxes_advance_when_full_and_step_back_on_backspace() {
    let mut h = Harness::on(Screen::Codes);
    h.tap(("box", 0usize));
    assert!(h.ui.is_focused(("code", 0usize)));
    // Letters are refused and do not advance.
    h.key(TextInputEvent::Commit("x".to_owned()));
    assert!(h.ui.is_focused(("code", 0usize)));

    // Three digits with no frame in between, as fast as keys can arrive.
    for digit in ["4", "7", "1"] {
        h.ui.text_input(&TextInputEvent::Commit(digit.to_owned()));
    }
    h.draw();
    assert_eq!(h.app().code, ["4", "7", "1"]);
    assert!(
        h.ui.is_focused(("code", 2usize)),
        "the last box keeps the focus"
    );

    // Backspace empties the last box, then walks back, deleting.
    h.key(TextInputEvent::DeleteBackward);
    assert_eq!(h.app().code, ["4", "7", ""]);
    assert!(h.ui.is_focused(("code", 2usize)));
    h.key(TextInputEvent::DeleteBackward);
    assert_eq!(h.app().code, ["4", "", ""]);
    assert!(h.ui.is_focused(("code", 1usize)));

    // A digit typed into a full box replaces it and moves on.
    h.tap(("box", 0usize));
    h.key(TextInputEvent::Commit("9".to_owned()));
    assert_eq!(h.app().code, ["9", "", ""]);
    assert!(h.ui.is_focused(("code", 1usize)));
    assert_eq!(h.log().last(), Some(&Msg::Code(0, "9".to_owned())));
}

// --- Caret --------------------------------------------------------------------

#[test]
fn the_caret_blinks_on_a_timer_not_by_rendering_continuously() {
    let mut h = Harness::on(Screen::Form);
    h.tap("name");
    assert!(h.caret_rect().is_some(), "solid right after the tap");
    assert!(!h.frame.redraw_requested(), "no frame at the next refresh");
    assert_eq!(h.frame.redraw_after(), Some(Duration::from_millis(500)));

    h.step(200);
    assert!(h.caret_rect().is_some());
    assert_eq!(h.frame.redraw_after(), Some(Duration::from_millis(300)));
    h.step(300);
    assert!(
        h.caret_rect().is_none(),
        "hidden for the second half second"
    );
    assert_eq!(h.frame.redraw_after(), Some(Duration::from_millis(500)));
    h.step(500);
    assert!(h.caret_rect().is_some());

    // Typing makes it solid again at once.
    h.step(700);
    assert!(h.caret_rect().is_none());
    h.key(TextInputEvent::Commit("a".to_owned()));
    assert!(h.caret_rect().is_some());

    // Without focus nothing asks for frames.
    h.key(TextInputEvent::Action(ImeAction::Done));
    assert_eq!(h.frame.redraw_after(), None);
    assert!(h.caret_rect().is_none());
}

#[test]
fn the_caret_stands_in_an_empty_field_and_follows_the_text() {
    let mut h = Harness::on(Screen::Form);
    h.tap("name");
    let field = h.rect("name");
    let empty = h.caret_rect().unwrap();
    assert_eq!(empty.x, field.x, "at the start, before the placeholder");
    assert!(empty.height >= 15.0 * SCALE && empty.height <= field.height);
    assert_eq!(empty.width, (1.5 * SCALE).round());
    h.type_text("Wa");
    let after = h.caret_rect().unwrap();
    assert!(after.x > empty.x + 20.0);
    assert_eq!((after.y, after.height), (empty.y, empty.height));
}

#[test]
fn a_long_single_line_scrolls_to_keep_the_caret_in_the_field() {
    let mut h = Harness::on(Screen::Form);
    h.tap("name");
    let field = h.rect("name");
    let long = "Mtaa wa Kariakoo, jengo la tatu, ghorofa ya pili, Dar es Salaam";
    h.key(TextInputEvent::Commit(long.to_owned()));
    assert_eq!(h.rect("name"), field, "the field does not grow or wrap");
    let caret = h.caret_rect().unwrap();
    assert!(
        caret.x >= field.x && caret.x <= field.x + field.width,
        "{caret:?} in {field:?}"
    );
    // The start of the text is scrolled out to the left, and clipped.
    assert!(h.glyphs().iter().any(|glyph| glyph.x < field.x));
    assert!(h.frame.list.ops().iter().any(|op| matches!(
        *op,
        DrawOp::PushClip { rect, .. } if rect.x == field.x && rect.y == field.y
    )));

    // Tapping the left edge walks the caret (and the text) back.
    h.tap_px(field.x + 2.0, field.y + field.height / 2.0);
    let moved = h.ui.caret().unwrap();
    assert!(moved < long.len() - 10);
    let caret = h.caret_rect().unwrap();
    assert!(caret.x >= field.x && caret.x <= field.x + field.width);
}

// --- Keyboard avoidance -------------------------------------------------------

#[test]
fn a_focused_field_is_scrolled_clear_of_the_keyboard() {
    let mut h = Harness::on(Screen::Tall);
    assert_eq!(h.ui.scroll_offset("scroll"), Some(0.0));
    let max = h.ui.max_scroll_offset("scroll").unwrap();

    // Scroll to the end by dragging, then tap the note field.
    h.ui.app_mut().screen = Screen::Tall;
    for _ in 0..30 {
        for (phase, y) in [
            (PointerPhase::Down, 1500.0),
            (PointerPhase::Moved, 1000.0),
            (PointerPhase::Moved, 300.0),
        ] {
            h.ui.pointer(&PointerEvent {
                id: 1,
                phase,
                position: Point::new(540.0, y),
            });
            h.step(400);
        }
        h.ui.pointer(&PointerEvent {
            id: 1,
            phase: PointerPhase::Cancelled,
            position: Point::new(540.0, 300.0),
        });
        h.step(400);
    }
    assert_eq!(h.ui.scroll_offset("scroll"), Some(max));
    h.tap("note");
    assert!(h.ui.is_focused("note"));
    let save_before = h.rect("save");

    // The keyboard takes the bottom 900 px (300 dp).
    h.info = h.info.with_keyboard_inset(900.0);
    h.draw();
    let keyboard_top = 2400.0 - 900.0;
    let note = h.rect("note");
    assert!(
        note.y + note.height <= keyboard_top - 20.0 * SCALE,
        "{note:?} above the keyboard at {keyboard_top}"
    );
    assert!(note.y >= 0.0);
    // The bottom bar pads for the keyboard: its button rides above it,
    // and the field is above the button too.
    let save = h.rect("save");
    assert!(
        (save.y + save.height + 16.0 * SCALE - keyboard_top).abs() < 1.0,
        "{save:?}"
    );
    assert!(note.y + note.height < save.y);

    // The keyboard goes: the bar returns, the content is still in range.
    h.info = h.info.with_keyboard_inset(0.0);
    h.draw();
    assert_eq!(h.rect("save"), save_before);
    assert!(h.ui.scroll_offset("scroll").unwrap() <= max);
}

/// The same page without a bottom bar: nothing resizes, so the scroll view
/// itself must make room.
#[test]
fn a_scroll_view_under_the_keyboard_can_scroll_its_end_clear_of_it() {
    let mut h = Harness::on(Screen::Tall);
    let max = h.ui.max_scroll_offset("scroll").unwrap();
    let viewport = h.rect("scroll");
    // A keyboard that reaches 200 dp into the viewport from below the bar.
    let bar_height = 2400.0 - (viewport.y + viewport.height);
    h.info = h.info.with_keyboard_inset(0.0);
    h.draw();
    assert_eq!(h.ui.max_scroll_offset("scroll"), Some(max));
    // With the bar's safe-area padding the viewport shrinks instead, and
    // the range grows by exactly what it lost.
    h.info = h.info.with_keyboard_inset(bar_height + 600.0);
    h.draw();
    let shrunk = h.rect("scroll");
    assert!(shrunk.height < viewport.height);
    let grown = h.ui.max_scroll_offset("scroll").unwrap();
    assert!(
        (grown - max - (viewport.height - shrunk.height) / SCALE).abs() < 0.5,
        "{grown} vs {max}"
    );
}

#[test]
fn a_screen_that_does_not_scroll_is_shifted_clear_of_the_keyboard() {
    let mut h = Harness::on(Screen::Form);
    h.tap("note");
    let before = h.rect("note");
    let secret_before = h.rect("secret");
    // A keyboard that covers the note field and the foot of the form.
    let keyboard_top = before.y - 30.0;
    h.info = h.info.with_keyboard_inset(2400.0 - keyboard_top);
    h.draw();
    let note = h.rect("note");
    assert!(
        note.y + note.height <= keyboard_top - 20.0 * SCALE,
        "{note:?} above {keyboard_top}"
    );
    // Everything moved together, and taps land on what is drawn.
    let shift = before.y - note.y;
    assert!(shift > 0.0);
    assert_eq!(h.rect("secret").y, secret_before.y - shift);
    h.tap("secret");
    assert!(h.ui.is_focused("secret"));
    // That field is above the keyboard anyway: nothing is shifted for it.
    assert_eq!(h.rect("secret"), secret_before);

    h.info = h.info.with_keyboard_inset(0.0);
    h.tap("note");
    assert_eq!(h.rect("note"), before);
}

#[test]
fn moving_to_a_field_out_of_view_scrolls_it_into_view() {
    let mut h = Harness::on(Screen::Tall);
    let viewport_bottom = h.rect("scroll").y + h.rect("scroll").height;
    // The fields are 900 dp down the page: scroll until the first shows.
    for _ in 0..3 {
        for (phase, y) in [
            (PointerPhase::Down, 1500.0),
            (PointerPhase::Moved, 1000.0),
            (PointerPhase::Moved, 300.0),
            (PointerPhase::Cancelled, 300.0),
        ] {
            h.ui.pointer(&PointerEvent {
                id: 1,
                phase,
                position: Point::new(540.0, y),
            });
            h.step(400);
        }
    }
    h.tap("name");
    assert!(h.ui.is_focused("name"));
    // The keyboard's Next key walks down the form; each field it reaches
    // is brought into the viewport.
    for _ in 0..3 {
        h.key(TextInputEvent::Action(ImeAction::Next));
    }
    assert!(h.ui.is_focused("note"));
    let note = h.rect("note");
    assert!(note.y + note.height <= viewport_bottom, "{note:?}");
}
