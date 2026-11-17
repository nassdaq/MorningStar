//! A mouse and a keyboard as a finger and a phone's keyboard.
//!
//! Everything here is plain data in and plain data out, so that it can be
//! tested without a window: the host translates the window system's events
//! into these types and delivers what comes back to the engine.

use std::time::{Duration, Instant};

use morningstar_runtime::{CaretMove, Point, PointerEvent, PointerPhase, TextInputEvent};

/// The pointer the mouse button is.
pub const MOUSE_POINTER: u64 = 1;
/// The pointer the wheel is: a second finger that drags.
pub const WHEEL_POINTER: u64 = 2;
/// The pointer a scripted tap (`--tap`) is: a finger of its own, so that
/// the real cursor moving over the window does not drag it.
pub const SCRIPT_POINTER: u64 = 3;

/// How long after the last wheel event the scrolling finger is lifted.
pub const WHEEL_LIFT_AFTER: Duration = Duration::from_millis(120);

/// How far one line of a wheel that reports lines moves the content, in dp.
pub const WHEEL_LINE_DP: f32 = 40.0;

/// The mouse as one finger: down when the button goes down, moving while
/// it is held, up when it is released.
#[derive(Clone, Copy, Debug, Default)]
pub struct Mouse {
    position: Point,
    down: bool,
}

impl Mouse {
    /// The cursor moved to `position` (physical pixels in the window).
    pub fn moved(&mut self, position: Point) -> Option<PointerEvent> {
        self.position = position;
        self.down.then_some(PointerEvent {
            id: MOUSE_POINTER,
            phase: PointerPhase::Moved,
            position,
        })
    }

    /// The button went down or up.
    pub fn button(&mut self, pressed: bool) -> Option<PointerEvent> {
        if pressed == self.down {
            return None;
        }
        self.down = pressed;
        Some(PointerEvent {
            id: MOUSE_POINTER,
            phase: if pressed {
                PointerPhase::Down
            } else {
                PointerPhase::Up
            },
            position: self.position,
        })
    }

    /// The window lost the mouse for good (it lost the focus mid-press):
    /// the touch is cancelled, so that nothing stays pressed.
    pub fn cancel(&mut self) -> Option<PointerEvent> {
        std::mem::take(&mut self.down).then_some(PointerEvent {
            id: MOUSE_POINTER,
            phase: PointerPhase::Cancelled,
            position: self.position,
        })
    }

    /// Where the cursor is.
    pub fn position(&self) -> Point {
        self.position
    }

    /// True while the button is held.
    pub fn is_down(&self) -> bool {
        self.down
    }
}

/// The wheel (or two fingers on a trackpad) as a finger that drags.
///
/// An app scrolls by being dragged; it has no notion of a wheel. So the
/// first wheel event puts a second finger down under the cursor, each
/// event moves it by what the wheel reported, and when the wheel has been
/// still for [`WHEEL_LIFT_AFTER`] the finger is lifted. The trackpad's own
/// momentum arrives as more wheel events, so the lift happens at rest and
/// the app adds no fling of its own.
#[derive(Clone, Copy, Debug, Default)]
pub struct Wheel {
    at: Option<Point>,
    last: Option<Instant>,
}

impl Wheel {
    /// The wheel moved the content by `delta` physical pixels with the
    /// cursor at `cursor`. Returns the touches to deliver, in order.
    pub fn scrolled(
        &mut self,
        cursor: Point,
        delta: (f32, f32),
        now: Instant,
    ) -> [Option<PointerEvent>; 2] {
        if !delta.0.is_finite() || !delta.1.is_finite() || delta == (0.0, 0.0) {
            return [None, None];
        }
        self.last = Some(now);
        let (down, from) = match self.at {
            Some(at) => (None, at),
            None => (
                Some(PointerEvent {
                    id: WHEEL_POINTER,
                    phase: PointerPhase::Down,
                    position: cursor,
                }),
                cursor,
            ),
        };
        let to = Point::new(from.x + delta.0, from.y + delta.1);
        self.at = Some(to);
        [
            down,
            Some(PointerEvent {
                id: WHEEL_POINTER,
                phase: PointerPhase::Moved,
                position: to,
            }),
        ]
    }

    /// When the scrolling finger is to be lifted, if it is down.
    pub fn lift_at(&self) -> Option<Instant> {
        self.at.and(self.last).map(|last| last + WHEEL_LIFT_AFTER)
    }

    /// Lifts the finger if the wheel has been still long enough.
    pub fn lift_if_still(&mut self, now: Instant) -> Option<PointerEvent> {
        if self.lift_at().is_some_and(|at| now >= at) {
            self.lift()
        } else {
            None
        }
    }

    /// Lifts the finger now, if it is down: the mouse button went down,
    /// and a press is not part of a scroll.
    pub fn lift(&mut self) -> Option<PointerEvent> {
        self.last = None;
        self.at.take().map(|position| PointerEvent {
            id: WHEEL_POINTER,
            phase: PointerPhase::Up,
            position,
        })
    }
}

/// The modifier keys held with a key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub alt: bool,
    pub command: bool,
    pub control: bool,
}

/// A key, as far as the preview tells keys apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key<'a> {
    Backspace,
    Delete,
    Enter,
    Escape,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    /// A function key, F1 being 1.
    Function(u8),
    /// A key that produces a character; this is the character without
    /// modifiers other than Shift, such as `s`.
    Character(&'a str),
    /// Any other key.
    Other,
}

/// What a key press means.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyAction {
    /// For the app's focused field, or its selection.
    Text(TextInputEvent),
    /// The system's back.
    Back,
    /// Save the screen as a picture.
    Screenshot,
    /// Start the app again; `fresh`: with nothing stored.
    Restart { fresh: bool },
    /// Turn "reduce motion" on or off.
    ToggleReduceMotion,
    /// Turn the safe-area insets on or off.
    ToggleInsets,
    /// Print the keys.
    Help,
    /// Nothing.
    None,
}

/// Decides what a key press means.
///
/// `text` is what the key types with the modifiers held, if anything.
/// `field_focused` says whether a text field has the keyboard, which on a
/// phone is whether the soft keyboard is up: without one, typing goes
/// nowhere, and Escape is the system's back rather than "keyboard away".
pub fn key_action(
    key: Key<'_>,
    text: Option<&str>,
    modifiers: Modifiers,
    field_focused: bool,
) -> KeyAction {
    use KeyAction::Text;
    let extend = modifiers.shift;
    let moved = |to| Text(TextInputEvent::Move { to, extend });

    // The preview's own keys first: they are not the app's.
    match key {
        Key::Function(1) => return KeyAction::Help,
        Key::Function(2) => return KeyAction::Screenshot,
        Key::Function(3) => return KeyAction::ToggleReduceMotion,
        Key::Function(4) => return KeyAction::ToggleInsets,
        Key::Function(5) => {
            return KeyAction::Restart {
                fresh: modifiers.shift,
            };
        }
        Key::Character(ch) if modifiers.command => {
            return match ch.to_ascii_lowercase().as_str() {
                "s" => KeyAction::Screenshot,
                "r" => KeyAction::Restart {
                    fresh: modifiers.shift,
                },
                // A selection outside a field can be copied too (ADR 0025).
                "a" => Text(TextInputEvent::SelectAll),
                "c" => Text(TextInputEvent::Copy),
                "x" if field_focused => Text(TextInputEvent::Cut),
                "v" if field_focused => Text(TextInputEvent::Paste),
                "z" if field_focused && modifiers.shift => Text(TextInputEvent::Redo),
                "z" if field_focused => Text(TextInputEvent::Undo),
                _ => KeyAction::None,
            };
        }
        Key::Escape => {
            // On a phone, back with the keyboard up puts the keyboard away.
            return if field_focused {
                Text(TextInputEvent::FocusLost)
            } else {
                KeyAction::Back
            };
        }
        _ => {}
    }

    if !field_focused {
        return KeyAction::None;
    }
    match key {
        Key::Backspace => Text(TextInputEvent::DeleteBackward),
        Key::Delete => Text(TextInputEvent::DeleteForward),
        Key::Enter => Text(TextInputEvent::Enter),
        // The Mac's own conventions: Option moves by word, Command to the
        // end of the line or of the text.
        Key::Left if modifiers.command => moved(CaretMove::LineStart),
        Key::Right if modifiers.command => moved(CaretMove::LineEnd),
        Key::Up if modifiers.command => moved(CaretMove::TextStart),
        Key::Down if modifiers.command => moved(CaretMove::TextEnd),
        Key::Left if modifiers.alt => moved(CaretMove::WordLeft),
        Key::Right if modifiers.alt => moved(CaretMove::WordRight),
        Key::Left => moved(CaretMove::Left),
        Key::Right => moved(CaretMove::Right),
        Key::Up => moved(CaretMove::Up),
        Key::Down => moved(CaretMove::Down),
        Key::Home => moved(CaretMove::LineStart),
        Key::End => moved(CaretMove::LineEnd),
        Key::Character(_) | Key::Other => match text {
            // Control combinations type control characters, which are not
            // text; Command combinations were dealt with above.
            Some(text)
                if !modifiers.control
                    && !modifiers.command
                    && !text.is_empty()
                    && !text.chars().any(char::is_control) =>
            {
                Text(TextInputEvent::Commit(text.to_owned()))
            }
            _ => KeyAction::None,
        },
        Key::Escape | Key::Function(_) => KeyAction::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f32, y: f32) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn a_click_is_a_tap_where_the_cursor_is() {
        let mut mouse = Mouse::default();
        assert_eq!(mouse.moved(at(10.0, 20.0)), None, "hovering is not a touch");
        let down = mouse.button(true).unwrap();
        assert_eq!(
            (down.id, down.phase, down.position),
            (MOUSE_POINTER, PointerPhase::Down, at(10.0, 20.0))
        );
        assert!(mouse.is_down());
        assert_eq!(mouse.button(true), None, "already down");
        let up = mouse.button(false).unwrap();
        assert_eq!((up.phase, up.position), (PointerPhase::Up, at(10.0, 20.0)));
        assert_eq!(mouse.button(false), None);
    }

    #[test]
    fn a_drag_moves_the_finger_and_lifts_it_where_it_ends() {
        let mut mouse = Mouse::default();
        mouse.moved(at(5.0, 5.0));
        mouse.button(true);
        let moved = mouse.moved(at(5.0, 80.0)).unwrap();
        assert_eq!(
            (moved.phase, moved.position),
            (PointerPhase::Moved, at(5.0, 80.0))
        );
        assert_eq!(mouse.button(false).unwrap().position, at(5.0, 80.0));
        assert_eq!(mouse.moved(at(9.0, 9.0)), None);
    }

    #[test]
    fn losing_the_window_mid_press_cancels_the_touch() {
        let mut mouse = Mouse::default();
        assert_eq!(mouse.cancel(), None);
        mouse.button(true);
        assert_eq!(mouse.cancel().unwrap().phase, PointerPhase::Cancelled);
        assert!(!mouse.is_down());
        assert_eq!(mouse.cancel(), None);
    }

    #[test]
    fn the_wheel_is_a_second_finger_that_drags_and_then_lifts() {
        let mut wheel = Wheel::default();
        let start = Instant::now();
        assert_eq!(wheel.lift_at(), None);

        let [down, moved] = wheel.scrolled(at(100.0, 400.0), (0.0, -30.0), start);
        let (down, moved) = (down.unwrap(), moved.unwrap());
        assert_eq!(
            (down.id, down.phase, down.position),
            (WHEEL_POINTER, PointerPhase::Down, at(100.0, 400.0))
        );
        assert_eq!(
            (moved.phase, moved.position),
            (PointerPhase::Moved, at(100.0, 370.0))
        );

        // More scrolling continues the same drag from where it is, wherever
        // the cursor has gone meanwhile.
        let later = start + Duration::from_millis(50);
        let [down, moved] = wheel.scrolled(at(300.0, 300.0), (4.0, -20.0), later);
        assert_eq!(down, None);
        assert_eq!(moved.unwrap().position, at(104.0, 350.0));

        assert_eq!(wheel.lift_at(), Some(later + WHEEL_LIFT_AFTER));
        assert_eq!(wheel.lift_if_still(later + Duration::from_millis(60)), None);
        let up = wheel.lift_if_still(later + WHEEL_LIFT_AFTER).unwrap();
        assert_eq!(
            (up.id, up.phase, up.position),
            (WHEEL_POINTER, PointerPhase::Up, at(104.0, 350.0))
        );
        assert_eq!(wheel.lift_at(), None);
        assert_eq!(wheel.lift(), None);
    }

    #[test]
    fn a_wheel_that_reports_nothing_or_nonsense_touches_nothing() {
        let mut wheel = Wheel::default();
        let now = Instant::now();
        assert_eq!(wheel.scrolled(at(1.0, 1.0), (0.0, 0.0), now), [None, None]);
        assert_eq!(
            wheel.scrolled(at(1.0, 1.0), (f32::NAN, 3.0), now),
            [None, None]
        );
        assert_eq!(wheel.lift_at(), None);
    }

    const NONE: Modifiers = Modifiers {
        shift: false,
        alt: false,
        command: false,
        control: false,
    };
    const COMMAND: Modifiers = Modifiers {
        command: true,
        ..NONE
    };
    const SHIFT: Modifiers = Modifiers {
        shift: true,
        ..NONE
    };

    fn text(event: TextInputEvent) -> KeyAction {
        KeyAction::Text(event)
    }

    #[test]
    fn typing_goes_to_the_focused_field_and_nowhere_without_one() {
        assert_eq!(
            key_action(Key::Character("a"), Some("a"), NONE, true),
            text(TextInputEvent::Commit("a".into()))
        );
        assert_eq!(
            key_action(Key::Character("a"), Some("A"), SHIFT, true),
            text(TextInputEvent::Commit("A".into()))
        );
        assert_eq!(
            key_action(Key::Other, Some("é"), NONE, true),
            text(TextInputEvent::Commit("é".into()))
        );
        assert_eq!(
            key_action(Key::Character("a"), Some("a"), NONE, false),
            KeyAction::None
        );
        assert_eq!(
            key_action(Key::Backspace, None, NONE, false),
            KeyAction::None
        );
    }

    #[test]
    fn editing_keys_are_the_fields_editing_events() {
        assert_eq!(
            key_action(Key::Backspace, Some("\u{8}"), NONE, true),
            text(TextInputEvent::DeleteBackward)
        );
        assert_eq!(
            key_action(Key::Delete, None, NONE, true),
            text(TextInputEvent::DeleteForward)
        );
        assert_eq!(
            key_action(Key::Enter, Some("\r"), NONE, true),
            text(TextInputEvent::Enter)
        );
        assert_eq!(
            key_action(
                Key::Character("c"),
                Some("\u{3}"),
                Modifiers {
                    control: true,
                    ..NONE
                },
                true
            ),
            KeyAction::None,
            "a control character is not text"
        );
    }

    #[test]
    fn arrows_move_the_caret_the_way_a_mac_does() {
        let moved = |to, extend| text(TextInputEvent::Move { to, extend });
        assert_eq!(
            key_action(Key::Left, None, NONE, true),
            moved(CaretMove::Left, false)
        );
        assert_eq!(
            key_action(Key::Right, None, SHIFT, true),
            moved(CaretMove::Right, true)
        );
        assert_eq!(
            key_action(Key::Left, None, Modifiers { alt: true, ..NONE }, true),
            moved(CaretMove::WordLeft, false)
        );
        assert_eq!(
            key_action(Key::Right, None, COMMAND, true),
            moved(CaretMove::LineEnd, false)
        );
        assert_eq!(
            key_action(
                Key::Up,
                None,
                Modifiers {
                    shift: true,
                    ..COMMAND
                },
                true
            ),
            moved(CaretMove::TextStart, true)
        );
        assert_eq!(
            key_action(Key::End, None, NONE, true),
            moved(CaretMove::LineEnd, false)
        );
    }

    #[test]
    fn the_clipboard_keys_reach_the_field() {
        assert_eq!(
            key_action(Key::Character("v"), Some("v"), COMMAND, true),
            text(TextInputEvent::Paste)
        );
        assert_eq!(
            key_action(Key::Character("x"), None, COMMAND, true),
            text(TextInputEvent::Cut)
        );
        assert_eq!(
            key_action(Key::Character("z"), None, COMMAND, true),
            text(TextInputEvent::Undo)
        );
        assert_eq!(
            key_action(
                Key::Character("Z"),
                None,
                Modifiers {
                    shift: true,
                    ..COMMAND
                },
                true
            ),
            text(TextInputEvent::Redo)
        );
        // Selected text outside a field can be copied, not changed.
        assert_eq!(
            key_action(Key::Character("c"), None, COMMAND, false),
            text(TextInputEvent::Copy)
        );
        assert_eq!(
            key_action(Key::Character("v"), None, COMMAND, false),
            KeyAction::None
        );
    }

    #[test]
    fn escape_is_back_unless_the_keyboard_is_up() {
        assert_eq!(key_action(Key::Escape, None, NONE, false), KeyAction::Back);
        assert_eq!(
            key_action(Key::Escape, None, NONE, true),
            text(TextInputEvent::FocusLost)
        );
    }

    #[test]
    fn the_previews_own_keys_work_with_or_without_a_field() {
        for focused in [false, true] {
            assert_eq!(
                key_action(Key::Character("s"), Some("s"), COMMAND, focused),
                KeyAction::Screenshot
            );
            assert_eq!(
                key_action(Key::Function(2), None, NONE, focused),
                KeyAction::Screenshot
            );
            assert_eq!(
                key_action(Key::Character("r"), None, COMMAND, focused),
                KeyAction::Restart { fresh: false }
            );
            assert_eq!(
                key_action(
                    Key::Character("R"),
                    None,
                    Modifiers {
                        shift: true,
                        ..COMMAND
                    },
                    focused
                ),
                KeyAction::Restart { fresh: true }
            );
            assert_eq!(
                key_action(Key::Function(5), None, SHIFT, focused),
                KeyAction::Restart { fresh: true }
            );
            assert_eq!(
                key_action(Key::Function(3), None, NONE, focused),
                KeyAction::ToggleReduceMotion
            );
            assert_eq!(
                key_action(Key::Function(4), None, NONE, focused),
                KeyAction::ToggleInsets
            );
            assert_eq!(
                key_action(Key::Function(1), None, NONE, focused),
                KeyAction::Help
            );
            assert_eq!(
                key_action(Key::Character("q"), None, COMMAND, focused),
                KeyAction::None,
                "the window system's own"
            );
        }
    }
}
