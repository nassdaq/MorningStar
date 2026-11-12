//! The editing model of a text field: pure functions from a value, a caret
//! and an input event to a new value and caret. No widgets, no platform.
//!
//! Offsets are bytes into the value and always sit on grapheme-cluster
//! boundaries, so a caret never splits what a reader sees as one character
//! (a letter with its combining marks, a surrogate pair's code point).

use std::borrow::Cow;

use morningstar_core::{CaretMove, TextEditState};
use unicode_segmentation::UnicodeSegmentation;

/// The character a secure field shows for each character of its value.
const BULLET: &str = "\u{2022}";

/// What a field accepts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct EditConfig {
    /// Line breaks are kept; otherwise they are dropped.
    pub multiline: bool,
    /// Most characters (grapheme clusters) the value may hold.
    pub max_length: Option<usize>,
    /// Only the ASCII digits 0 to 9 are kept.
    pub digits_only: bool,
    /// A character typed into a full field replaces the one before the
    /// caret instead of being refused (a one-digit code box).
    pub overwrite_when_full: bool,
}

/// The transient state of the field being edited.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct EditState {
    /// Where typing goes. With a selection, the end of it that moves.
    pub caret: usize,
    /// The other end of the selection: where it was started from, before
    /// or after the caret. `None`, or equal to the caret, when nothing is
    /// selected.
    pub anchor: Option<usize>,
    /// The part of the value still being composed by the input method.
    pub composing: Option<(usize, usize)>,
}

impl EditState {
    /// A caret at `caret`, nothing selected, nothing being composed.
    pub(crate) const fn at(caret: usize) -> Self {
        Self {
            caret,
            anchor: None,
            composing: None,
        }
    }

    /// The selected range, start before end, if anything is selected.
    pub(crate) fn selection(&self) -> Option<(usize, usize)> {
        let anchor = self.anchor.filter(|anchor| *anchor != self.caret)?;
        Some((anchor.min(self.caret), anchor.max(self.caret)))
    }

    /// The selection of `value`, with both ends moved to character
    /// boundaries inside it.
    pub(crate) fn selection_in(&self, value: &str) -> Option<(usize, usize)> {
        let (start, end) = self.selection()?;
        let (start, end) = (snap(value, start), snap(value, end));
        (start < end).then_some((start, end))
    }

    /// Selects `start..end` with the caret at `end`; a range of nothing is
    /// a caret.
    pub(crate) fn select(&mut self, start: usize, end: usize) {
        self.caret = end;
        self.anchor = (start != end).then_some(start);
        self.composing = None;
    }
}

/// Number of characters (grapheme clusters) in `text`.
pub(crate) fn length(text: &str) -> usize {
    text.graphemes(true).count()
}

/// The largest grapheme boundary of `value` that is not after `index`.
pub(crate) fn snap(value: &str, index: usize) -> usize {
    if index >= value.len() {
        return value.len();
    }
    value
        .grapheme_indices(true)
        .map(|(start, _)| start)
        .take_while(|start| *start <= index)
        .last()
        .unwrap_or(0)
}

/// The boundary one character before `index` (0 at the start).
pub(crate) fn previous(value: &str, index: usize) -> usize {
    let index = snap(value, index);
    value[..index]
        .grapheme_indices(true)
        .next_back()
        .map_or(0, |(start, _)| start)
}

/// The boundary one character after `index` (the length at the end).
pub(crate) fn next(value: &str, index: usize) -> usize {
    let index = snap(value, index);
    value[index..]
        .graphemes(true)
        .next()
        .map_or(value.len(), |grapheme| index + grapheme.len())
}

/// `text` with what the field does not accept removed.
fn accepted<'a>(text: &'a str, config: &EditConfig) -> Cow<'a, str> {
    let keep = |character: char| {
        if config.digits_only {
            character.is_ascii_digit()
        } else if character == '\n' {
            config.multiline
        } else {
            // Other control characters (a carriage return, a tab from a
            // hardware keyboard) have no place in a field.
            !character.is_control()
        }
    };
    if text.chars().all(keep) {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(text.chars().filter(|character| keep(*character)).collect())
    }
}

/// The first `count` characters of `text`.
fn first(text: &str, count: usize) -> &str {
    let end = text
        .grapheme_indices(true)
        .nth(count)
        .map_or(text.len(), |(start, _)| start);
    &text[..end]
}

/// Replaces `range` of `value` with as much of `text` as the field accepts
/// and has room for. Returns the new value and the end of the inserted
/// text, or `None` if nothing changes.
fn replace(
    value: &str,
    range: (usize, usize),
    text: &str,
    config: &EditConfig,
) -> Option<(String, usize)> {
    let (mut start, end) = (snap(value, range.0), snap(value, range.1.max(range.0)));
    let text = accepted(text, config);
    let mut text: &str = &text;
    if let Some(max) = config.max_length {
        let kept = length(&value[..start]) + length(&value[end..]);
        let room = max.saturating_sub(kept);
        if room == 0 && config.overwrite_when_full && !text.is_empty() && start == end {
            // Full: the new character takes the place of the one before the
            // caret (or after it, at the very start).
            if start > 0 {
                start = previous(value, start);
                text = first(text, 1);
                let mut result = String::with_capacity(value.len() + text.len());
                result.push_str(&value[..start]);
                result.push_str(text);
                let caret = result.len();
                result.push_str(&value[end..]);
                return (result != value).then_some((result, caret));
            }
            let after = next(value, 0);
            text = first(text, 1);
            let result = format!("{text}{}", &value[after..]);
            return (result != value).then_some((result, text.len()));
        }
        text = first(text, room);
    }
    if text.is_empty() && start == end {
        return None;
    }
    let mut result = String::with_capacity(value.len() - (end - start) + text.len());
    result.push_str(&value[..start]);
    result.push_str(text);
    let caret = result.len();
    result.push_str(&value[end..]);
    Some((result, caret))
}

/// Committed text: replaces the composing text if there is any, otherwise
/// goes in at the caret. Returns the new value if it changed. The
/// composition is over either way.
pub(crate) fn insert(
    value: &str,
    state: &mut EditState,
    text: &str,
    config: &EditConfig,
) -> Option<String> {
    let range = state
        .composing
        .take()
        .or(state.selection())
        .unwrap_or((state.caret, state.caret));
    state.anchor = None;
    let caret = snap(value, state.caret);
    match replace(value, range, text, config) {
        Some((result, caret)) => {
            state.caret = caret;
            Some(result)
        }
        None => {
            state.caret = caret;
            None
        }
    }
}

/// Composing (pre-edit) text: replaces the previous composing text, or
/// starts at the caret. What is in the value afterwards is marked as being
/// composed; empty text ends the composition.
pub(crate) fn compose(
    value: &str,
    state: &mut EditState,
    text: &str,
    config: &EditConfig,
) -> Option<String> {
    let range = state
        .composing
        .or(state.selection())
        .unwrap_or((state.caret, state.caret));
    state.anchor = None;
    let start = snap(value, range.0);
    match replace(value, range, text, config) {
        Some((result, caret)) => {
            state.caret = caret;
            state.composing = (caret > start).then_some((start, caret));
            Some(result)
        }
        None => {
            if text.is_empty() {
                state.composing = None;
            }
            None
        }
    }
}

/// Backspace: removes the character before the caret, all of it (a letter
/// with its marks goes as one).
pub(crate) fn delete_backward(value: &str, state: &mut EditState) -> Option<String> {
    if let Some(removed) = delete_selection(value, state) {
        return Some(removed);
    }
    let caret = snap(value, state.caret);
    let start = previous(value, caret);
    state.caret = start;
    remove(value, state, start, caret)
}

/// Forward delete: removes the character after the caret.
pub(crate) fn delete_forward(value: &str, state: &mut EditState) -> Option<String> {
    if let Some(removed) = delete_selection(value, state) {
        return Some(removed);
    }
    let caret = snap(value, state.caret);
    state.caret = caret;
    remove(value, state, caret, next(value, caret))
}

/// Removes the selected text, if any, leaving the caret where it was.
/// `None` if nothing is selected.
pub(crate) fn delete_selection(value: &str, state: &mut EditState) -> Option<String> {
    let selection = state.selection_in(value);
    state.anchor = None;
    let (start, end) = selection?;
    state.caret = start;
    remove(value, state, start, end)
}

fn remove(value: &str, state: &mut EditState, start: usize, end: usize) -> Option<String> {
    if start == end {
        return None;
    }
    let removed = end - start;
    // Keep what is left of a composition that the deletion cut into.
    state.composing = state.composing.and_then(|(from, to)| {
        let shift = |offset: usize| {
            if offset >= end {
                offset - removed
            } else {
                offset.min(start)
            }
        };
        let (from, to) = (shift(from), shift(to));
        (to > from).then_some((from, to))
    });
    let mut result = String::with_capacity(value.len() - removed);
    result.push_str(&value[..start]);
    result.push_str(&value[end..]);
    Some(result)
}

/// The platform's editor reports its whole state after an edit of its own.
/// The field takes it, minus what it does not accept. Returns the new value
/// if it differs from `value`; `state` is updated either way.
pub(crate) fn adopt(
    value: &str,
    state: &mut EditState,
    edited: &TextEditState,
    config: &EditConfig,
) -> Option<String> {
    let caret = edited.caret.min(edited.text.len());
    let mut text = accepted(&edited.text, config);
    // Where the caret is once what was refused before it is gone.
    let mut new_caret = if text.len() == edited.text.len() {
        caret
    } else {
        let mut boundary = caret;
        while !edited.text.is_char_boundary(boundary) {
            boundary -= 1;
        }
        accepted(&edited.text[..boundary], config).len()
    };
    if let Some(max) = config.max_length
        && length(&text) > max
    {
        let cut = if config.overwrite_when_full {
            // Keep the characters that end at the caret: the newest ones.
            let end = snap(&text, new_caret);
            let mut start = end;
            for _ in 0..max {
                start = previous(&text, start);
            }
            new_caret = end - start;
            text[start..end].to_owned()
        } else {
            first(&text, max).to_owned()
        };
        text = Cow::Owned(cut);
    }
    let unchanged_by_field = *text == *edited.text;
    state.caret = snap(&text, new_caret);
    // The platform's own selection is taken as it is; one over text the
    // field changed means nothing any more.
    state.anchor = edited
        .anchor
        .filter(|_| unchanged_by_field)
        .map(|anchor| snap(&text, anchor))
        .filter(|anchor| *anchor != state.caret);
    // A composition only survives if the field took the text as it was.
    state.composing = edited
        .composing
        .filter(|_| unchanged_by_field)
        .map(|(from, to)| (snap(&text, from), snap(&text, to)))
        .filter(|(from, to)| to > from);
    (*text != *value).then(|| text.into_owned())
}

/// True for whitespace, which is never a word.
fn is_space(segment: &str) -> bool {
    segment.chars().all(char::is_whitespace)
}

/// The word a double tap or a long press at `index` selects: the word (or
/// run of punctuation) the index is in or at the end of. On whitespace, or
/// in an empty value, nothing: the range is empty, at the index.
pub(crate) fn word_at(value: &str, index: usize) -> (usize, usize) {
    let index = snap(value, index);
    let mut before: Option<(usize, usize)> = None;
    for (start, segment) in value.split_word_bound_indices() {
        let end = start + segment.len();
        if index < start {
            break;
        }
        if index < end {
            if !is_space(segment) {
                return (start, end);
            }
            // On a space: the word that ends exactly here, if there is one.
            return before
                .filter(|word| word.1 == index)
                .unwrap_or((index, index));
        }
        before = (!is_space(segment)).then_some((start, end));
    }
    // At the very end of the value.
    before
        .filter(|word| word.1 == index)
        .unwrap_or((index, index))
}

/// Where Ctrl+Left goes from `index`: the start of the word the index is in
/// or after.
pub(crate) fn word_start_before(value: &str, index: usize) -> usize {
    let index = snap(value, index);
    value
        .unicode_word_indices()
        .map(|(start, _)| start)
        .take_while(|start| *start < index)
        .last()
        .unwrap_or(0)
}

/// Where Ctrl+Right goes from `index`: the end of the word the index is in
/// or before.
pub(crate) fn word_end_after(value: &str, index: usize) -> usize {
    let index = snap(value, index);
    value
        .unicode_word_indices()
        .map(|(start, word)| start + word.len())
        .find(|end| *end > index)
        .unwrap_or(value.len())
}

/// True if `value` reads from right to left: its first letter is Hebrew or
/// Arabic (the first-strong rule of the bidirectional algorithm, for the
/// scripts the bundled fonts cover).
pub(crate) fn reads_right_to_left(value: &str) -> bool {
    value
        .chars()
        .find(|character| character.is_alphabetic())
        .is_some_and(|character| {
            matches!(
                u32::from(character),
                0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF
            )
        })
}

/// Moves the caret as a key does. Returns false for the moves that depend
/// on how the text is laid out in lines ([`CaretMove::Up`],
/// [`CaretMove::Down`], [`CaretMove::LineStart`], [`CaretMove::LineEnd`]),
/// which the caller works out with the text engine; they change nothing
/// here.
///
/// Left and right are the reading direction's backward and forward: in a
/// value that reads right to left the Left key moves on. Without `extend`
/// a selection collapses to the end the key points at instead of moving
/// from it, as on every platform.
pub(crate) fn move_caret(value: &str, state: &mut EditState, to: CaretMove, extend: bool) -> bool {
    let caret = snap(value, state.caret);
    let rtl = reads_right_to_left(value);
    let backward = |to: CaretMove| match to {
        CaretMove::Left | CaretMove::WordLeft => !rtl,
        _ => rtl,
    };
    let selection = state.selection_in(value);
    let target = match to {
        CaretMove::Left | CaretMove::Right => match selection.filter(|_| !extend) {
            Some((start, end)) => {
                if backward(to) {
                    start
                } else {
                    end
                }
            }
            None if backward(to) => previous(value, caret),
            None => next(value, caret),
        },
        CaretMove::WordLeft | CaretMove::WordRight => {
            if backward(to) {
                word_start_before(value, caret)
            } else {
                word_end_after(value, caret)
            }
        }
        CaretMove::TextStart => 0,
        CaretMove::TextEnd => value.len(),
        CaretMove::Up | CaretMove::Down | CaretMove::LineStart | CaretMove::LineEnd => {
            return false;
        }
    };
    place_caret(state, caret, target, extend);
    true
}

/// Puts the caret at `target`. With `extend` the other end of the
/// selection stays where it is (or starts at `from`, where the caret was).
pub(crate) fn place_caret(state: &mut EditState, from: usize, target: usize, extend: bool) {
    let anchor = if extend {
        Some(state.anchor.unwrap_or(from))
    } else {
        None
    };
    state.caret = target;
    state.anchor = anchor.filter(|anchor| *anchor != target);
    // Moving the caret ends a composition where it stands.
    state.composing = None;
}

/// Text from the clipboard, as a field takes it: line breaks of every
/// kind become `\n` for a field of several lines and a space for a field
/// of one, so that two pasted lines do not run together into one word.
/// What the field does not accept, and what does not fit, is then dropped
/// by [`insert`] like typed text.
pub(crate) fn pasted<'a>(text: &'a str, config: &EditConfig) -> Cow<'a, str> {
    if !text.contains(['\r', '\n', '\u{2028}', '\u{2029}']) {
        return Cow::Borrowed(text);
    }
    let line_break = if config.multiline { "\n" } else { " " };
    let mut result = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\r' => {
                if characters.peek() == Some(&'\n') {
                    characters.next();
                }
                result.push_str(line_break);
            }
            '\n' | '\u{2028}' | '\u{2029}' => result.push_str(line_break),
            other => result.push(other),
        }
    }
    // A single line does not end in the space its last line break became.
    if !config.multiline {
        let kept = result.trim_end_matches(' ').len();
        if text.ends_with(['\r', '\n', '\u{2028}', '\u{2029}']) {
            result.truncate(kept);
        }
    }
    Cow::Owned(result)
}

/// Most steps a field can be taken back.
const HISTORY_LIMIT: usize = 100;

/// What kind of edit the last recorded step was, so that a run of typing
/// (or of backspaces) is taken back as one step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditKind {
    Typing,
    Deleting,
    /// A paste, a cut, a replaced selection: always a step of its own.
    Other,
}

/// A field as it was before an edit.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Snapshot {
    value: String,
    caret: usize,
    anchor: Option<usize>,
}

/// The edits made to a field since it got the focus, for undo and redo.
/// Bounded: beyond [`HISTORY_LIMIT`] steps the oldest is forgotten.
#[derive(Clone, Debug, Default)]
pub(crate) struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    last: Option<EditKind>,
}

impl History {
    /// Notes the field as it is before an edit of `kind` changes it. An
    /// edit that continues a run of the same kind is part of that run's
    /// step.
    pub(crate) fn record(&mut self, value: &str, state: &EditState, kind: EditKind) {
        self.redo.clear();
        let continues = kind != EditKind::Other && self.last == Some(kind);
        self.last = Some(kind);
        if continues {
            return;
        }
        if self.undo.len() == HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.undo.push(Snapshot {
            value: value.to_owned(),
            caret: state.caret,
            anchor: state.anchor,
        });
    }

    /// The caret moved, or something else happened that ends a run: the
    /// next edit is a new step.
    pub(crate) fn break_run(&mut self) {
        self.last = None;
    }

    /// Takes the last step back: returns the value before it and restores
    /// the caret of that time. `None` if there is nothing to take back.
    pub(crate) fn undo(&mut self, value: &str, state: &mut EditState) -> Option<String> {
        Self::step(&mut self.undo, &mut self.redo, value, state).inspect(|_| self.last = None)
    }

    /// Makes the step last taken back again.
    pub(crate) fn redo(&mut self, value: &str, state: &mut EditState) -> Option<String> {
        Self::step(&mut self.redo, &mut self.undo, value, state).inspect(|_| self.last = None)
    }

    fn step(
        from: &mut Vec<Snapshot>,
        to: &mut Vec<Snapshot>,
        value: &str,
        state: &mut EditState,
    ) -> Option<String> {
        let snapshot = from.pop()?;
        to.push(Snapshot {
            value: value.to_owned(),
            caret: state.caret,
            anchor: state.anchor,
        });
        state.caret = snapshot.caret;
        state.anchor = snapshot.anchor;
        state.composing = None;
        Some(snapshot.value)
    }

    /// Forgets everything: the field holds something no step here led to.
    pub(crate) fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.last = None;
    }
}

/// What a field shows for `value`: the value itself, or one bullet per
/// character when it is secure.
pub(crate) fn display(value: &str, secure: bool) -> Cow<'_, str> {
    if secure {
        Cow::Owned(BULLET.repeat(length(value)))
    } else {
        Cow::Borrowed(value)
    }
}

/// The offset in [`display`]'s text that corresponds to `index` in `value`.
pub(crate) fn display_index(value: &str, index: usize, secure: bool) -> usize {
    let index = snap(value, index);
    if secure {
        length(&value[..index]) * BULLET.len()
    } else {
        index
    }
}

/// The offset in `value` that corresponds to `index` in [`display`]'s text.
pub(crate) fn value_index(value: &str, index: usize, secure: bool) -> usize {
    if secure {
        let count = index / BULLET.len();
        value
            .grapheme_indices(true)
            .nth(count)
            .map_or(value.len(), |(start, _)| start)
    } else {
        snap(value, index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FREE: EditConfig = EditConfig {
        multiline: false,
        max_length: None,
        digits_only: false,
        overwrite_when_full: false,
    };

    fn at(caret: usize) -> EditState {
        EditState::at(caret)
    }

    #[test]
    fn typing_inserts_at_the_caret() {
        let mut state = at(0);
        let value = insert("", &mut state, "Hab", &FREE).unwrap();
        assert_eq!((value.as_str(), state.caret), ("Hab", 3));
        let value = insert(&value, &mut state, "ari", &FREE).unwrap();
        assert_eq!((value.as_str(), state.caret), ("Habari", 6));

        // In the middle.
        let mut state = at(2);
        let value = insert("Hari", &mut state, "ba", &FREE).unwrap();
        assert_eq!((value.as_str(), state.caret), ("Habari", 4));
        assert_eq!(insert(&value, &mut state, "", &FREE), None);
    }

    #[test]
    fn backspace_removes_a_whole_character() {
        // Swahili, ASCII.
        let mut state = at(5);
        assert_eq!(delete_backward("Simba", &mut state).unwrap(), "Simb");
        assert_eq!(state.caret, 4);

        // Arabic "marhaban" with vowel marks: each letter and its marks
        // (two or three code points) go together.
        let word = "مَرْحَبًا";
        let mut value = word.to_owned();
        let mut state = at(value.len());
        let mut steps = 0;
        while let Some(shorter) = delete_backward(&value, &mut state) {
            assert!(shorter.is_char_boundary(state.caret));
            assert_eq!(state.caret, shorter.len());
            // Never leaves a mark without its letter.
            assert!(
                shorter
                    .chars()
                    .next_back()
                    .is_none_or(|last| ('\u{0621}'..='\u{064A}').contains(&last)
                        || ('\u{064B}'..='\u{0652}').contains(&last)),
            );
            value = shorter;
            steps += 1;
        }
        assert_eq!(value, "");
        // Five letters, not nine code points.
        assert_eq!(word.chars().count(), 9);
        assert_eq!(steps, 5);

        // "e" + combining acute is one character; so is a surrogate pair's.
        let mut state = at("e\u{301}".len());
        assert_eq!(delete_backward("e\u{301}", &mut state).unwrap(), "");
        let mut state = at("a\u{1D11E}".len());
        assert_eq!(delete_backward("a\u{1D11E}", &mut state).unwrap(), "a");

        // Nothing before the caret.
        let mut state = at(0);
        assert_eq!(delete_backward("abc", &mut state), None);
        assert_eq!(state.caret, 0);
    }

    #[test]
    fn a_letter_with_a_shadda_and_a_vowel_is_one_deletion() {
        // Dal, shadda, fatha: three code points, one character.
        let value = "\u{062F}\u{0651}\u{064E}";
        let mut state = at(value.len());
        assert_eq!(delete_backward(value, &mut state).unwrap(), "");
        // A caret that somehow sits between the marks is first moved to the
        // boundary before it: nothing to delete there.
        let mut state = at(2);
        assert_eq!(delete_backward(value, &mut state), None);
        assert_eq!(state.caret, 0);
    }

    #[test]
    fn forward_delete_removes_the_character_after_the_caret() {
        let mut state = at(1);
        assert_eq!(delete_forward("abc", &mut state).unwrap(), "ac");
        assert_eq!(state.caret, 1);
        let mut state = at(3);
        assert_eq!(delete_forward("abc", &mut state), None);
        let mut state = at(0);
        assert_eq!(delete_forward("e\u{301}x", &mut state).unwrap(), "x");
    }

    #[test]
    fn carets_move_by_whole_characters() {
        let value = "ae\u{301}\u{1D11E}b";
        // a | e+acute | clef | b
        let stops = [0, 1, 4, 8, 9];
        for pair in stops.windows(2) {
            assert_eq!(next(value, pair[0]), pair[1]);
            assert_eq!(previous(value, pair[1]), pair[0]);
        }
        assert_eq!(previous(value, 0), 0);
        assert_eq!(next(value, value.len()), value.len());
        // Offsets inside a character snap back to its start.
        assert_eq!(snap(value, 2), 1);
        assert_eq!(snap(value, 3), 1);
        assert_eq!(snap(value, 6), 4);
        assert_eq!(snap(value, 99), value.len());
    }

    #[test]
    fn max_length_counts_characters_and_cuts_what_does_not_fit() {
        let config = EditConfig {
            max_length: Some(4),
            ..FREE
        };
        let mut state = at(2);
        let value = insert("ab", &mut state, "cdef", &config).unwrap();
        assert_eq!((value.as_str(), state.caret), ("abcd", 4));
        assert_eq!(insert(&value, &mut state, "x", &config), None);
        // Characters, not bytes: four Arabic letters with marks fit.
        let mut state = at(0);
        let value = insert("", &mut state, "مَرْحَبًا", &config).unwrap();
        assert_eq!(length(&value), 4);
        // Replacing a composition frees its room first.
        let mut state = EditState {
            caret: 4,
            anchor: None,
            composing: Some((2, 4)),
        };
        assert_eq!(insert("abcd", &mut state, "XY", &config).unwrap(), "abXY");
    }

    #[test]
    fn a_full_overwriting_field_replaces_its_last_character() {
        let config = EditConfig {
            max_length: Some(1),
            digits_only: true,
            overwrite_when_full: true,
            ..FREE
        };
        let mut state = at(1);
        assert_eq!(insert("4", &mut state, "7", &config).unwrap(), "7");
        assert_eq!(state.caret, 1);
        // The same digit again changes nothing.
        assert_eq!(insert("7", &mut state, "7", &config), None);
        // A letter is refused before it can overwrite anything.
        assert_eq!(insert("7", &mut state, "x", &config), None);
        // Caret at the start: the character after it is replaced.
        let mut state = at(0);
        assert_eq!(insert("4", &mut state, "9", &config).unwrap(), "9");
        assert_eq!(state.caret, 1);
    }

    #[test]
    fn digits_only_drops_everything_else() {
        let config = EditConfig {
            digits_only: true,
            ..FREE
        };
        let mut state = at(0);
        let value = insert("", &mut state, "07 12-34a٣", &config).unwrap();
        assert_eq!((value.as_str(), state.caret), ("071234", 6));
        assert_eq!(insert(&value, &mut state, "abc", &config), None);
    }

    #[test]
    fn line_breaks_are_kept_only_by_multiline_fields() {
        let mut state = at(1);
        assert_eq!(insert("ab", &mut state, "\n", &FREE), None);
        assert_eq!(insert("ab", &mut state, "x\r\ny\t", &FREE).unwrap(), "axyb");
        let multiline = EditConfig {
            multiline: true,
            ..FREE
        };
        let mut state = at(1);
        assert_eq!(insert("ab", &mut state, "\n", &multiline).unwrap(), "a\nb");
        assert_eq!(state.caret, 2);
    }

    #[test]
    fn composing_text_is_replaced_until_it_is_committed() {
        let mut state = at(3);
        let value = compose("Na ", &mut state, "h", &FREE).unwrap();
        assert_eq!(value, "Na h");
        assert_eq!(state.composing, Some((3, 4)));
        let value = compose(&value, &mut state, "hab", &FREE).unwrap();
        assert_eq!(value, "Na hab");
        assert_eq!((state.caret, state.composing), (6, Some((3, 6))));
        // The input method corrects the word, then commits it.
        let value = compose(&value, &mut state, "habari", &FREE).unwrap();
        assert_eq!(value, "Na habari");
        let value = insert(&value, &mut state, "Habari ", &FREE).unwrap();
        assert_eq!(value, "Na Habari ");
        assert_eq!((state.caret, state.composing), (10, None));
    }

    #[test]
    fn an_empty_composition_removes_the_composing_text() {
        let mut state = at(0);
        let value = compose("", &mut state, "ab", &FREE).unwrap();
        let value = compose(&value, &mut state, "", &FREE).unwrap();
        assert_eq!(value, "");
        assert_eq!((state.caret, state.composing), (0, None));
        // Ending a composition that never started changes nothing.
        assert_eq!(compose("x", &mut at(1), "", &FREE), None);
    }

    #[test]
    fn backspace_inside_a_composition_shrinks_it() {
        let mut state = EditState {
            caret: 5,
            anchor: None,
            composing: Some((2, 5)),
        };
        let value = delete_backward("abcde", &mut state).unwrap();
        assert_eq!(value, "abcd");
        assert_eq!((state.caret, state.composing), (4, Some((2, 4))));
        let mut state = EditState {
            caret: 3,
            anchor: None,
            composing: Some((2, 3)),
        };
        delete_backward("abc", &mut state).unwrap();
        assert_eq!(state.composing, None);
    }

    #[test]
    fn the_platforms_edit_is_adopted_whole_when_acceptable() {
        let mut state = at(0);
        let edited = TextEditState {
            text: "Juma".to_owned(),
            caret: 4,
            anchor: None,
            composing: Some((0, 4)),
        };
        assert_eq!(adopt("", &mut state, &edited, &FREE).unwrap(), "Juma");
        assert_eq!((state.caret, state.composing), (4, Some((0, 4))));
        // The same state again is not a change.
        assert_eq!(adopt("Juma", &mut state, &edited, &FREE), None);
    }

    #[test]
    fn the_platforms_edit_is_filtered_and_its_composition_dropped() {
        let digits = EditConfig {
            digits_only: true,
            max_length: Some(6),
            ..FREE
        };
        let mut state = at(2);
        let edited = TextEditState {
            text: "12ab3".to_owned(),
            caret: 4,
            anchor: None,
            composing: Some((2, 4)),
        };
        assert_eq!(adopt("12", &mut state, &edited, &digits).unwrap(), "123");
        // The caret was after "12ab": after "12" once the letters are gone.
        assert_eq!((state.caret, state.composing), (2, None));

        let edited = TextEditState {
            text: "12345678".to_owned(),
            caret: 8,
            anchor: None,
            composing: None,
        };
        assert_eq!(
            adopt("123", &mut state, &edited, &digits).unwrap(),
            "123456"
        );
        assert_eq!(state.caret, 6);
    }

    #[test]
    fn a_full_overwriting_field_adopts_the_newest_character() {
        let boxed = EditConfig {
            digits_only: true,
            max_length: Some(1),
            overwrite_when_full: true,
            ..FREE
        };
        let mut state = at(1);
        let edited = TextEditState {
            text: "47".to_owned(),
            caret: 2,
            anchor: None,
            composing: None,
        };
        assert_eq!(adopt("4", &mut state, &edited, &boxed).unwrap(), "7");
        assert_eq!(state.caret, 1);
    }

    #[test]
    fn a_secure_field_shows_one_bullet_per_character() {
        assert_eq!(display("siri", true), "••••");
        assert_eq!(display("siri", false), "siri");
        assert_eq!(display("", true), "");
        // A letter with marks is one bullet.
        let value = "ae\u{301}b";
        assert_eq!(display(value, true).chars().count(), 3);
        // Offsets map both ways.
        let bullet = BULLET.len();
        assert_eq!(display_index(value, 0, true), 0);
        assert_eq!(display_index(value, 1, true), bullet);
        assert_eq!(display_index(value, 4, true), 2 * bullet);
        assert_eq!(display_index(value, value.len(), true), 3 * bullet);
        assert_eq!(value_index(value, 2 * bullet, true), 4);
        assert_eq!(value_index(value, 99 * bullet, true), value.len());
        assert_eq!(value_index(value, 2, false), 1, "snapped to a boundary");
        assert_eq!(display_index(value, 4, false), 4);
    }

    // ------------------------------------------------------------ selection

    fn selecting(anchor: usize, caret: usize) -> EditState {
        EditState {
            caret,
            anchor: Some(anchor),
            composing: None,
        }
    }

    #[test]
    fn a_selection_is_ordered_and_an_empty_one_is_none() {
        assert_eq!(selecting(2, 5).selection(), Some((2, 5)));
        assert_eq!(selecting(5, 2).selection(), Some((2, 5)));
        assert_eq!(selecting(3, 3).selection(), None);
        assert_eq!(at(3).selection(), None);
        // Ends inside a character move back to its start.
        let value = "ae\u{301}b";
        assert_eq!(selecting(0, 2).selection_in(value), Some((0, 1)));
        assert_eq!(selecting(1, 3).selection_in(value), None);
        let mut state = at(0);
        state.select(4, 1);
        assert_eq!((state.caret, state.anchor), (1, Some(4)));
        state.select(2, 2);
        assert_eq!((state.caret, state.anchor), (2, None));
    }

    #[test]
    fn typing_replaces_the_selection_whichever_way_it_was_made() {
        for mut state in [selecting(7, 11), selecting(11, 7)] {
            let value = insert("Habari yako rafiki", &mut state, "zetu", &FREE).unwrap();
            assert_eq!(value, "Habari zetu rafiki");
            assert_eq!((state.caret, state.anchor), (11, None));
        }
        // Text the field refuses still removes what was selected.
        let digits = EditConfig {
            digits_only: true,
            ..FREE
        };
        let mut state = selecting(0, 3);
        assert_eq!(insert("123456", &mut state, "x", &digits).unwrap(), "456");
        assert_eq!((state.caret, state.anchor), (0, None));
        // A composition starts in place of the selection.
        let mut state = selecting(3, 5);
        assert_eq!(compose("Na we", &mut state, "mi", &FREE).unwrap(), "Na mi");
        assert_eq!((state.anchor, state.composing), (None, Some((3, 5))));
    }

    #[test]
    fn replacing_a_selection_frees_its_room_in_a_full_field() {
        let config = EditConfig {
            max_length: Some(6),
            ..FREE
        };
        let mut state = selecting(1, 5);
        // Four characters go, so five fit beside the two that stay... but
        // only four of them: the limit is six.
        assert_eq!(
            insert("abcdef", &mut state, "VWXYZ", &config).unwrap(),
            "aVWXYf"
        );
        assert_eq!(state.caret, 5);
    }

    #[test]
    fn backspace_and_delete_remove_the_selection_and_nothing_more() {
        let mut state = selecting(7, 12);
        assert_eq!(
            delete_backward("Habari yako rafiki", &mut state).unwrap(),
            "Habari rafiki"
        );
        assert_eq!((state.caret, state.anchor), (7, None));
        let mut state = selecting(12, 7);
        assert_eq!(
            delete_forward("Habari yako rafiki", &mut state).unwrap(),
            "Habari rafiki"
        );
        assert_eq!((state.caret, state.anchor), (7, None));
        // A selection of whole Arabic characters with their marks.
        let word = "مَرْحَبًا";
        let second = next(word, next(word, 0));
        let mut state = selecting(0, second);
        let rest = delete_selection(word, &mut state).unwrap();
        assert_eq!(length(&rest), 3);
        assert_eq!(state.caret, 0);
        assert_eq!(delete_selection("abc", &mut at(1)), None);
    }

    #[test]
    fn the_platforms_selection_is_adopted_and_dropped_if_the_text_was_changed() {
        let mut state = at(0);
        let edited = TextEditState {
            text: "Habari yako".to_owned(),
            caret: 11,
            anchor: Some(7),
            composing: None,
        };
        assert_eq!(adopt("Habari yako", &mut state, &edited, &FREE), None);
        assert_eq!(state.selection(), Some((7, 11)));
        // A collapsed one is a caret.
        let collapsed = TextEditState {
            anchor: Some(11),
            ..edited.clone()
        };
        adopt("Habari yako", &mut state, &collapsed, &FREE);
        assert_eq!((state.caret, state.anchor), (11, None));
        // The field refused part of the text: its offsets mean nothing.
        let digits = EditConfig {
            digits_only: true,
            ..FREE
        };
        let edited = TextEditState {
            text: "12ab34".to_owned(),
            caret: 6,
            anchor: Some(1),
            composing: None,
        };
        assert_eq!(adopt("12", &mut state, &edited, &digits).unwrap(), "1234");
        assert_eq!((state.caret, state.anchor), (4, None));
    }

    #[test]
    fn a_word_is_what_a_reader_calls_one() {
        let value = "Habari yako, rafiki!";
        assert_eq!(word_at(value, 0), (0, 6));
        assert_eq!(word_at(value, 3), (0, 6));
        // At the end of a word: that word.
        assert_eq!(word_at(value, 6), (0, 6));
        assert_eq!(word_at(value, 8), (7, 11));
        // Punctuation is a "word" of its own; the space after it is not.
        assert_eq!(word_at(value, 11), (11, 12));
        assert_eq!(word_at(value, 12), (11, 12));
        assert_eq!(word_at(value, 20), (19, 20));
        // In the middle of a run of spaces: nothing.
        assert_eq!(word_at("a   b", 2), (2, 2));
        assert_eq!(word_at("", 0), (0, 0));
        assert_eq!(word_at("ab ", 3), (3, 3));
        // An apostrophe inside a word, and a number with a decimal point.
        assert_eq!(word_at("ng'ombe 12.5", 4), (0, 7));
        assert_eq!(word_at("ng'ombe 12.5", 10), (8, 12));
        // Arabic: a word with its marks, never half a character.
        let arabic = "مَرْحَبًا بك";
        let space = arabic.find(' ').unwrap();
        assert_eq!(word_at(arabic, 4), (0, space));
        assert_eq!(word_at(arabic, space + 1), (space + 1, arabic.len()));
        // An index inside a character is first moved to its start.
        assert_eq!(word_at("e\u{301}t", 1), (0, 4));
    }

    #[test]
    fn word_jumps_skip_spaces_and_punctuation() {
        let value = "Habari yako, rafiki!";
        assert_eq!(word_end_after(value, 0), 6);
        assert_eq!(word_end_after(value, 6), 11);
        assert_eq!(word_end_after(value, 11), 19);
        assert_eq!(word_end_after(value, 19), 20, "nothing but the end");
        assert_eq!(word_start_before(value, 20), 13);
        assert_eq!(word_start_before(value, 13), 7);
        assert_eq!(word_start_before(value, 9), 7);
        assert_eq!(word_start_before(value, 7), 0);
        assert_eq!(word_start_before(value, 0), 0);
    }

    #[test]
    fn keys_move_the_caret_and_shift_extends() {
        let value = "Habari yako";
        let mut state = at(3);
        assert!(move_caret(value, &mut state, CaretMove::Right, false));
        assert_eq!((state.caret, state.anchor), (4, None));
        assert!(move_caret(value, &mut state, CaretMove::Left, true));
        assert!(move_caret(value, &mut state, CaretMove::Left, true));
        assert_eq!((state.caret, state.anchor), (2, Some(4)));
        // Back to where it started: no selection is left.
        move_caret(value, &mut state, CaretMove::Right, true);
        move_caret(value, &mut state, CaretMove::Right, true);
        assert_eq!((state.caret, state.anchor), (4, None));
        // A selection collapses to the side the key points at.
        let mut state = selecting(2, 6);
        move_caret(value, &mut state, CaretMove::Left, false);
        assert_eq!((state.caret, state.anchor), (2, None));
        let mut state = selecting(6, 2);
        move_caret(value, &mut state, CaretMove::Right, false);
        assert_eq!((state.caret, state.anchor), (6, None));
        move_caret(value, &mut state, CaretMove::WordRight, true);
        assert_eq!(state.selection(), Some((6, 11)));
        move_caret(value, &mut state, CaretMove::TextStart, true);
        assert_eq!(state.selection(), Some((0, 6)));
        move_caret(value, &mut state, CaretMove::TextEnd, false);
        assert_eq!((state.caret, state.anchor), (11, None));
        // By whole characters.
        let mut state = at(0);
        move_caret("e\u{301}x", &mut state, CaretMove::Right, false);
        assert_eq!(state.caret, 3);
        // A move ends a composition; moves by lines are not made here.
        let mut state = EditState {
            caret: 4,
            anchor: None,
            composing: Some((2, 4)),
        };
        assert!(!move_caret(value, &mut state, CaretMove::Up, false));
        assert_eq!(state.composing, Some((2, 4)));
        move_caret(value, &mut state, CaretMove::Left, false);
        assert_eq!(state.composing, None);
    }

    #[test]
    fn in_right_to_left_text_the_left_key_moves_on() {
        let value = "مَرْحَبًا بك";
        assert!(reads_right_to_left(value));
        assert!(reads_right_to_left("123 שלום"));
        assert!(!reads_right_to_left("abc مرحبا"));
        assert!(!reads_right_to_left(""));
        let mut state = at(0);
        move_caret(value, &mut state, CaretMove::Left, false);
        assert_eq!(state.caret, next(value, 0));
        move_caret(value, &mut state, CaretMove::Right, false);
        assert_eq!(state.caret, 0);
        move_caret(value, &mut state, CaretMove::WordLeft, true);
        assert_eq!(state.selection(), Some((0, value.find(' ').unwrap())));
    }

    #[test]
    fn pasted_line_breaks_fit_the_field() {
        let multiline = EditConfig {
            multiline: true,
            ..FREE
        };
        assert_eq!(pasted("a\r\nb\rc\nd", &FREE), "a b c d");
        assert_eq!(pasted("a\r\nb\rc\nd", &multiline), "a\nb\nc\nd");
        assert_eq!(pasted("line\n", &FREE), "line");
        assert_eq!(pasted("line\n", &multiline), "line\n");
        assert_eq!(pasted("a\u{2028}b", &FREE), "a b");
        assert!(matches!(pasted("plain", &FREE), Cow::Borrowed("plain")));
        // What is left goes through the same rules as typed text.
        let phone = EditConfig {
            digits_only: true,
            max_length: Some(10),
            ..FREE
        };
        let mut state = at(0);
        let text = pasted("+255 712 345 678\n", &phone);
        assert_eq!(insert("", &mut state, &text, &phone).unwrap(), "2557123456");
        // A tab or another control character never gets in.
        let mut state = at(0);
        assert_eq!(
            insert("", &mut state, &pasted("a\tb\u{7}c", &FREE), &FREE).unwrap(),
            "abc"
        );
    }

    #[test]
    fn history_steps_back_by_runs_and_forward_again() {
        let mut history = History::default();
        let mut value = String::new();
        let mut state = at(0);
        let apply =
            |history: &mut History, value: &mut String, state: &mut EditState, text: &str| {
                history.record(value, state, EditKind::Typing);
                *value = insert(value, state, text, &FREE).unwrap();
            };
        apply(&mut history, &mut value, &mut state, "H");
        apply(&mut history, &mut value, &mut state, "a");
        history.break_run();
        apply(&mut history, &mut value, &mut state, "b");
        assert_eq!(value, "Hab");
        value = history.undo(&value, &mut state).unwrap();
        assert_eq!((value.as_str(), state.caret), ("Ha", 2));
        value = history.undo(&value, &mut state).unwrap();
        assert_eq!((value.as_str(), state.caret), ("", 0));
        assert_eq!(history.undo(&value, &mut state), None);
        value = history.redo(&value, &mut state).unwrap();
        value = history.redo(&value, &mut state).unwrap();
        assert_eq!((value.as_str(), state.caret), ("Hab", 3));
        assert_eq!(history.redo(&value, &mut state), None);
        // A new edit after stepping back forgets what was ahead.
        value = history.undo(&value, &mut state).unwrap();
        apply(&mut history, &mut value, &mut state, "!");
        assert_eq!(history.redo(&value, &mut state), None);
        // A selection is restored with the text it was in.
        let mut state = selecting(0, 3);
        history.record("Ha!", &state, EditKind::Other);
        let cut = delete_selection("Ha!", &mut state).unwrap();
        assert_eq!(history.undo(&cut, &mut state).unwrap(), "Ha!");
        assert_eq!(state.selection(), Some((0, 3)));
    }

    #[test]
    fn history_is_bounded() {
        let mut history = History::default();
        let mut state = at(0);
        for step in 0..(HISTORY_LIMIT + 50) {
            history.record(&step.to_string(), &state, EditKind::Other);
        }
        let mut steps = 0;
        let mut oldest = String::new();
        while let Some(value) = history.undo("now", &mut state) {
            oldest = value;
            steps += 1;
        }
        assert_eq!(steps, HISTORY_LIMIT);
        assert_eq!(oldest, "50");
        history.clear();
        assert_eq!(history.redo("now", &mut state), None);
    }
}
