//! Caret placement and hit-testing with real fonts.

use morningstar_core::{Color, Point};
use morningstar_text::{TextContext, TextStyle};

const LATIN: &[u8] =
    include_bytes!("../../../examples/hello-gpu/assets/fonts/NotoSans-Regular.ttf");
const ARABIC: &[u8] =
    include_bytes!("../../../examples/hello-gpu/assets/fonts/NotoSansArabic-Regular.ttf");

const STYLE: TextStyle = TextStyle::new(32.0, Color::WHITE);

fn context() -> TextContext {
    let mut text = TextContext::new();
    text.add_font(LATIN);
    text.add_font(ARABIC);
    text
}

#[test]
fn carets_advance_left_to_right_and_end_at_the_measured_width() {
    let mut text = context();
    let word = "Habari";
    let xs: Vec<f32> = (0..=word.len())
        .map(|index| text.caret(word, &STYLE, None, index).x)
        .collect();
    assert_eq!(xs[0], 0.0);
    assert!(xs.windows(2).all(|pair| pair[0] < pair[1]), "{xs:?}");
    let width = text.measure(word, &STYLE, None).width;
    assert!((xs[word.len()] - width).abs() < 0.01);
    let caret = text.caret(word, &STYLE, None, 3);
    assert_eq!((caret.y, caret.width), (0.0, 0.0));
    assert!((caret.height - 32.0 * STYLE.line_height).abs() < 0.01);
}

#[test]
fn a_tap_gives_the_nearer_edge_of_the_letter_under_it() {
    let mut text = context();
    let word = "Habari";
    for index in 0..word.len() {
        let left = text.caret(word, &STYLE, None, index).x;
        let right = text.caret(word, &STYLE, None, index + 1).x;
        let quarter = (right - left) / 4.0;
        let at = |x: f32| Point::new(x, 10.0);
        assert_eq!(text.index_at(word, &STYLE, None, at(left + quarter)), index);
        assert_eq!(
            text.index_at(word, &STYLE, None, at(right - quarter)),
            index + 1
        );
    }
    // Outside the text: the nearest end.
    assert_eq!(
        text.index_at(word, &STYLE, None, Point::new(-50.0, 10.0)),
        0
    );
    assert_eq!(
        text.index_at(word, &STYLE, None, Point::new(5000.0, 10.0)),
        word.len()
    );
    assert_eq!(
        text.index_at(word, &STYLE, None, Point::new(5000.0, 900.0)),
        word.len()
    );
}

#[test]
fn arabic_carets_run_right_to_left() {
    let mut text = context();
    // "salam": four letters, two bytes each, joined.
    let word = "سلام";
    let width = text.measure(word, &STYLE, None).width;
    let xs: Vec<f32> = word
        .char_indices()
        .map(|(index, _)| index)
        .chain([word.len()])
        .map(|index| text.caret(word, &STYLE, None, index).x)
        .collect();
    assert!(
        (xs[0] - width).abs() < 0.01,
        "before the first letter is the right edge"
    );
    assert!(
        xs[xs.len() - 1].abs() < 0.01,
        "after the last letter is the left edge"
    );
    assert!(xs.windows(2).all(|pair| pair[0] > pair[1]), "{xs:?}");

    // A tap near the right edge is the start of the text, near the left
    // edge its end.
    assert_eq!(
        text.index_at(word, &STYLE, None, Point::new(width - 1.0, 5.0)),
        0
    );
    assert_eq!(
        text.index_at(word, &STYLE, None, Point::new(1.0, 5.0)),
        word.len()
    );
    assert_eq!(
        text.index_at(word, &STYLE, None, Point::new(-40.0, 5.0)),
        word.len()
    );
    assert_eq!(
        text.index_at(word, &STYLE, None, Point::new(width + 40.0, 5.0)),
        0
    );
}

#[test]
fn a_tap_never_lands_inside_a_character() {
    let mut text = context();
    // Arabic with combining marks (fatha, shadda) and Swahili.
    for string in ["مَرْحَبًا بِكُمْ", "Habari za asubuhi", "aé𝄞b"] {
        let width = text.measure(string, &STYLE, None).width;
        let mut x = -5.0;
        while x < width + 5.0 {
            let index = text.index_at(string, &STYLE, None, Point::new(x, 5.0));
            assert!(string.is_char_boundary(index), "{string:?} at {x}: {index}");
            x += 1.5;
        }
    }
}

#[test]
fn an_index_inside_a_character_or_past_the_end_is_clamped() {
    let mut text = context();
    let word = "aé";
    let inside = text.caret(word, &STYLE, None, 2);
    assert_eq!(inside, text.caret(word, &STYLE, None, 1));
    assert_eq!(
        text.caret(word, &STYLE, None, 99),
        text.caret(word, &STYLE, None, word.len())
    );
}

#[test]
fn lines_have_their_own_carets() {
    let mut text = context();
    let string = "moja\nmbili\n";
    let line = 32.0 * STYLE.line_height;
    let first = text.caret(string, &STYLE, None, 2);
    let second = text.caret(string, &STYLE, None, 5);
    let last = text.caret(string, &STYLE, None, string.len());
    assert_eq!(first.y, 0.0);
    assert!((second.y - line).abs() < 0.01 && second.x == 0.0);
    // After the final line break there is an empty third line.
    assert!(
        (last.y - 2.0 * line).abs() < 0.01 && last.x == 0.0,
        "{last:?}"
    );
    // A touch on the second line lands in the second line.
    let index = text.index_at(string, &STYLE, None, Point::new(4000.0, line * 1.5));
    assert_eq!(index, 10);
}

#[test]
fn a_wrapped_line_puts_the_caret_where_the_text_continues() {
    let mut text = context();
    let string = "Simu imepokelewa na fundi wetu leo asubuhi";
    let width = Some(240.0);
    let size = text.measure(string, &STYLE, width);
    let line = 32.0 * STYLE.line_height;
    assert!(size.height > line * 1.5, "the text wraps");
    let end = text.caret(string, &STYLE, width, string.len());
    assert!((end.y + end.height - size.height).abs() < 0.01);
    for index in 0..=string.len() {
        let caret = text.caret(string, &STYLE, width, index);
        assert!(caret.x >= 0.0 && caret.x <= 240.5, "{index}: {caret:?}");
        // Round trip: tapping where the caret is gives the same index,
        // except at a wrap, where the space before it is the same place.
        let hit = text.index_at(
            string,
            &STYLE,
            width,
            Point::new(caret.x, caret.y + caret.height / 2.0),
        );
        assert!(hit.abs_diff(index) <= 1, "{index} -> {hit}");
    }
}

#[test]
fn empty_text_and_missing_fonts_are_harmless() {
    let mut text = context();
    let caret = text.caret("", &STYLE, None, 0);
    assert_eq!((caret.x, caret.y), (0.0, 0.0));
    assert!((caret.height - 32.0 * STYLE.line_height).abs() < 0.01);
    assert_eq!(text.index_at("", &STYLE, None, Point::new(40.0, 4.0)), 0);

    let mut none = TextContext::new();
    assert_eq!(none.index_at("abc", &STYLE, None, Point::new(40.0, 4.0)), 0);
    assert_eq!(none.caret("abc", &STYLE, None, 2).x, 0.0);
}
