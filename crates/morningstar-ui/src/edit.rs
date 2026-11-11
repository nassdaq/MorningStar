//! The editing model of a text field: pure functions from a value, a caret
//! and an input event to a new value and caret. No widgets, no platform.
//!
//! Offsets are bytes into the value and always sit on grapheme-cluster
//! boundaries, so a caret never splits what a reader sees as one character
//! (a letter with its combining marks, a surrogate pair's code point).

use std::borrow::Cow;

use morningstar_core::TextEditState;
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
    pub caret: usize,
    /// The part of the value still being composed by the input method.
    pub composing: Option<(usize, usize)>,
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
    let range = state.composing.take().unwrap_or((state.caret, state.caret));
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
    let range = state.composing.unwrap_or((state.caret, state.caret));
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
    let caret = snap(value, state.caret);
    let start = previous(value, caret);
    state.caret = start;
    remove(value, state, start, caret)
}

/// Forward delete: removes the character after the caret.
pub(crate) fn delete_forward(value: &str, state: &mut EditState) -> Option<String> {
    let caret = snap(value, state.caret);
    state.caret = caret;
    remove(value, state, caret, next(value, caret))
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
    // A composition only survives if the field took the text as it was.
    state.composing = edited
        .composing
        .filter(|_| unchanged_by_field)
        .map(|(from, to)| (snap(&text, from), snap(&text, to)))
        .filter(|(from, to)| to > from);
    (*text != *value).then(|| text.into_owned())
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
        EditState {
            caret,
            composing: None,
        }
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
            composing: Some((2, 5)),
        };
        let value = delete_backward("abcde", &mut state).unwrap();
        assert_eq!(value, "abcd");
        assert_eq!((state.caret, state.composing), (4, Some((2, 4))));
        let mut state = EditState {
            caret: 3,
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
            composing: Some((2, 4)),
        };
        assert_eq!(adopt("12", &mut state, &edited, &digits).unwrap(), "123");
        // The caret was after "12ab": after "12" once the letters are gone.
        assert_eq!((state.caret, state.composing), (2, None));

        let edited = TextEditState {
            text: "12345678".to_owned(),
            caret: 8,
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
}
