use crate::Point;

/// The stage of a touch (or other pointer) interaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerPhase {
    /// The pointer made contact.
    Down,
    /// The pointer moved while in contact.
    Moved,
    /// The pointer was lifted: the interaction completed normally.
    Up,
    /// The system took the interaction away (an incoming call, a system
    /// gesture). Any in-progress press must be abandoned, not completed.
    Cancelled,
}

/// One pointer event, in physical pixels relative to the surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerEvent {
    /// Identifies one finger from `Down` until its `Up` or `Cancelled`.
    /// Values are only meaningful for comparison and may be reused later.
    pub id: u64,
    pub phase: PointerPhase,
    pub position: Point,
}

/// Which soft keyboard a text field asks for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum KeyboardType {
    /// The ordinary keyboard.
    #[default]
    Text,
    /// Digits only (a PIN, a verification code).
    Number,
    /// Digits with a decimal separator (an amount).
    Decimal,
    /// The telephone keypad.
    Phone,
    /// The ordinary keyboard arranged for an e-mail address.
    Email,
}

/// What the keyboard capitalises by itself.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Capitalization {
    /// Nothing.
    #[default]
    None,
    /// The first letter of each sentence.
    Sentences,
    /// The first letter of each word (a person's name).
    Words,
    /// Every letter.
    Characters,
}

/// The action key of the soft keyboard: what its bottom-right key says and
/// what a [`TextInputEvent::Action`] carries when it is pressed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ImeAction {
    /// Finished with the form: the keyboard goes away.
    #[default]
    Done,
    /// Move to the next field.
    Next,
    /// Go to the address or submit the form.
    Go,
    /// Run the search.
    Search,
    /// Send the message.
    Send,
}

/// What a focused text field asks of the platform's keyboard.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextInputTraits {
    pub keyboard: KeyboardType,
    pub capitalization: Capitalization,
    pub action: ImeAction,
    /// A password: the platform must not learn, suggest or show it.
    pub secure: bool,
    /// The Return key inserts a line break instead of performing `action`.
    pub multiline: bool,
    /// Let the keyboard correct and suggest words. Ignored when `secure`.
    pub autocorrect: bool,
}

/// What the application wants from the platform's text input for the frame
/// it has just built: the focused field's traits and current contents. See
/// `Frame::set_text_input` in `morningstar-runtime`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextInputRequest {
    /// Identifies the field. A different value is a different field: the
    /// platform restarts its input method for it.
    pub session: u64,
    pub traits: TextInputTraits,
    /// The field's whole text.
    pub text: String,
    /// The caret, as a byte offset into `text` (on a character boundary).
    /// With a selection, the end of it that moves.
    pub caret: usize,
    /// The other end of the selection, as a byte offset into `text`: where
    /// it was started from, before or after the caret. `None` when nothing
    /// is selected. The platform's input method is told, so that its
    /// suggestions and its own selection gestures agree with the field.
    pub anchor: Option<usize>,
}

/// What a host must do to its keyboard, from `Engine::take_text_input_command`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextInputCommand {
    /// A field gained focus, or a different field or different traits
    /// replaced the previous ones: show the keyboard for this request.
    Show(TextInputRequest),
    /// The same field's text or caret was changed by the application (not
    /// by the platform's own editing): replace what the platform holds.
    Update(TextInputRequest),
    /// No field has focus: hide the keyboard.
    Hide,
}

/// The platform editor's whole state after an edit it made itself. Android's
/// input method edits a buffer the host keeps and reports the result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextEditState {
    pub text: String,
    /// The caret (the end of the selection), as a byte offset into `text`.
    pub caret: usize,
    /// The start of the selection (the end that stays put), if the
    /// platform's editor selected a range: `None` when nothing is selected.
    pub anchor: Option<usize>,
    /// The part of `text` still being composed (pre-edit), as byte offsets.
    pub composing: Option<(usize, usize)>,
}

/// Text input from the platform for the focused field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextInputEvent {
    /// Insert this text at the caret, replacing any composing text.
    Commit(String),
    /// Replace the composing (pre-edit) text with this; empty ends the
    /// composition with nothing.
    Compose(String),
    /// Delete the character (grapheme cluster) before the caret.
    DeleteBackward,
    /// Delete the character (grapheme cluster) after the caret.
    DeleteForward,
    /// The Return key of a hardware keyboard, or of a soft keyboard that
    /// reports it as a key: a line break in a multi-line field, the field's
    /// own action otherwise.
    Enter,
    /// The soft keyboard's action key.
    Action(ImeAction),
    /// The platform's editor made an edit of its own; this is the result.
    Edited(TextEditState),
    /// The user dismissed the keyboard (Android's back gesture, the
    /// keyboard's own hide key): the field should give up focus.
    FocusLost,
    /// A key that moves the caret (an arrow, Home, End, with or without
    /// Control), from a hardware keyboard or from an input method that
    /// moves its cursor with key events. `extend` (Shift) keeps the other
    /// end of the selection where it is.
    Move {
        /// Where to.
        to: CaretMove,
        /// Select from where the caret was instead of only moving.
        extend: bool,
    },
    /// Select the field's whole text (Ctrl/Cmd+A, an input method's
    /// "select all").
    SelectAll,
    /// Copy the selection to the clipboard (Ctrl/Cmd+C).
    Copy,
    /// Move the selection to the clipboard (Ctrl/Cmd+X).
    Cut,
    /// Replace the selection with the clipboard's text (Ctrl/Cmd+V).
    Paste,
    /// Take back the last edit (Ctrl/Cmd+Z).
    Undo,
    /// Make the edit that was taken back again (Ctrl/Cmd+Shift+Z, Ctrl+Y).
    Redo,
}

/// Where a key moves the caret of the focused field. See
/// [`TextInputEvent::Move`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CaretMove {
    /// One character towards the left of the screen.
    Left,
    /// One character towards the right of the screen.
    Right,
    /// To the edge of the word on the left.
    WordLeft,
    /// To the edge of the word on the right.
    WordRight,
    /// One line up; on the first line, to the start of the text.
    Up,
    /// One line down; on the last line, to the end of the text.
    Down,
    /// To the start of the caret's line (Home).
    LineStart,
    /// To the end of the caret's line (End).
    LineEnd,
    /// To the start of the text (Ctrl+Home).
    TextStart,
    /// To the end of the text (Ctrl+End).
    TextEnd,
}

/// Whether the icons and text the system draws in a bar over the
/// application (clock, battery, the gesture handle) should be light or
/// dark. The application draws edge to edge under transparent bars, so
/// only it knows what is behind them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BarContent {
    /// Light icons, for a dark background.
    Light,
    /// Dark icons, for a light background.
    Dark,
}

/// How the system's bars should look over the application: see
/// `Frame::set_system_bars` in `morningstar-runtime`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SystemBars {
    /// The status bar, at the top.
    pub status: BarContent,
    /// The navigation bar or gesture handle, at the bottom.
    pub navigation: BarContent,
}

impl SystemBars {
    /// Light icons in both bars, for a dark screen.
    pub const LIGHT_CONTENT: Self = Self::all(BarContent::Light);
    /// Dark icons in both bars, for a light screen.
    pub const DARK_CONTENT: Self = Self::all(BarContent::Dark);

    /// The same content colour in both bars.
    pub const fn all(content: BarContent) -> Self {
        Self {
            status: content,
            navigation: content,
        }
    }

    /// What reads on a screen whose background is `background`: dark
    /// icons on a light colour and light icons on a dark one, by the
    /// colour's relative luminance (WCAG's; a [`Color`](crate::Color) is
    /// already linear). The threshold, 0.179, is where black and white
    /// text contrast equally with the background.
    pub fn for_background(background: crate::Color) -> Self {
        let channel = |value: f32| {
            if value.is_nan() {
                0.0
            } else {
                value.clamp(0.0, 1.0)
            }
        };
        let luminance = 0.2126 * channel(background.r)
            + 0.7152 * channel(background.g)
            + 0.0722 * channel(background.b);
        if luminance > 0.179 {
            Self::DARK_CONTENT
        } else {
            Self::LIGHT_CONTENT
        }
    }
}

/// Converts an offset in UTF-16 code units (what Java and UIKit count in)
/// to a byte offset into `text`. Offsets past the end clamp to the end; an
/// offset inside a surrogate pair moves to the start of that character.
pub fn utf16_to_byte_offset(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (byte, character) in text.char_indices() {
        if units + character.len_utf16() > utf16 {
            return byte;
        }
        units += character.len_utf16();
    }
    text.len()
}

/// Converts a byte offset into `text` to an offset in UTF-16 code units.
/// An offset past the end, or inside a character, counts the characters
/// that start before it.
pub fn byte_to_utf16_offset(text: &str, byte: usize) -> usize {
    text.char_indices()
        .take_while(|(start, _)| *start < byte)
        .map(|(_, character)| character.len_utf16())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_offsets_convert_both_ways() {
        // 'a' (1 byte, 1 unit), 'é' (2, 1), '𝄞' (4, 2), 'ب' (2, 1).
        let text = "aé𝄞ب";
        let pairs = [(0, 0), (1, 1), (2, 3), (4, 7), (5, 9)];
        for (utf16, byte) in pairs {
            assert_eq!(utf16_to_byte_offset(text, utf16), byte, "utf16 {utf16}");
            assert_eq!(byte_to_utf16_offset(text, byte), utf16, "byte {byte}");
        }
    }

    #[test]
    fn offsets_out_of_range_or_inside_a_character_are_safe() {
        let text = "a𝄞";
        // Inside the surrogate pair: the start of that character.
        assert_eq!(utf16_to_byte_offset(text, 2), 1);
        assert_eq!(utf16_to_byte_offset(text, 99), text.len());
        assert_eq!(byte_to_utf16_offset(text, 99), 3);
        // Inside the four-byte character: it has started, so it is counted.
        assert_eq!(byte_to_utf16_offset(text, 3), 3);
        assert_eq!(utf16_to_byte_offset("", 4), 0);
    }

    #[test]
    fn bar_content_follows_the_luminance_of_the_background() {
        use crate::Color;
        assert_eq!(
            SystemBars::for_background(Color::WHITE),
            SystemBars::DARK_CONTENT
        );
        assert_eq!(
            SystemBars::for_background(Color::BLACK),
            SystemBars::LIGHT_CONTENT
        );
        // Mid grey in sRGB (#777777, linear 0.184) is just on the light
        // side; a saturated blue is dark, a yellow light.
        assert_eq!(
            SystemBars::for_background(Color::rgb(0.184, 0.184, 0.184)),
            SystemBars::DARK_CONTENT
        );
        assert_eq!(
            SystemBars::for_background(Color::rgb(0.0, 0.0, 1.0)),
            SystemBars::LIGHT_CONTENT
        );
        assert_eq!(
            SystemBars::for_background(Color::rgb(1.0, 1.0, 0.0)),
            SystemBars::DARK_CONTENT
        );
        assert_eq!(
            SystemBars::for_background(Color::rgb(f32::NAN, 0.0, 0.0)),
            SystemBars::LIGHT_CONTENT
        );
    }
}
