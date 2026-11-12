//! Where text lands, to the pixel: fractional widths and pen positions,
//! and the browser's rule for baselines (ADR 0022). The expected numbers
//! are Chrome's, from `examples/callready/design/layout/*.txt`, which were
//! measured at 2 px per dp.

use morningstar_core::{Color, DisplayList, DrawOp, Point, Rect};
use morningstar_text::{FontFamily, TextAlign, TextContext, TextStyle, TextWeight};

const OUTFIT: &[u8] = include_bytes!("../../../examples/callready/assets/fonts/Outfit-Regular.ttf");
const OUTFIT_BOLD: &[u8] =
    include_bytes!("../../../examples/callready/assets/fonts/Outfit-Bold.ttf");
const ABRIL: &[u8] =
    include_bytes!("../../../examples/callready/assets/fonts/AbrilFatface-Regular.ttf");

fn context() -> TextContext {
    let mut text = TextContext::new();
    text.add_font(OUTFIT);
    text.add_font(OUTFIT_BOLD);
    text.add_font(ABRIL);
    text
}

fn outfit(size: f32) -> TextStyle {
    TextStyle::new(size, Color::WHITE).family(FontFamily::named("Outfit"))
}

/// (glyph rectangle, atlas rectangle) of every glyph drawn.
fn glyphs(
    text: &mut TextContext,
    string: &str,
    style: &TextStyle,
    origin: Point,
    width: Option<f32>,
) -> Vec<(Rect, Rect)> {
    let mut list = DisplayList::new();
    text.draw(&mut list, string, style, origin, width);
    list.ops()
        .iter()
        .filter_map(|op| match *op {
            DrawOp::Glyph { rect, atlas, .. } => Some((rect, atlas)),
            _ => None,
        })
        .collect()
}

/// The pixel row a flat-footed capital stands on: the bottom of its image.
fn foot(text: &mut TextContext, style: &TextStyle, origin_y: f32) -> f32 {
    let (rect, _) = glyphs(text, "H", style, Point::new(0.0, origin_y), None)[0];
    rect.y + rect.height
}

#[test]
fn widths_keep_their_fractions_and_match_the_browsers() {
    let mut text = context();
    // "“You charged me " is 131.7 dp in the design; trailing space included.
    let width = text
        .measure("\u{201c}You charged me ", &outfit(34.0), None)
        .width;
    assert!((width - 263.4).abs() < 0.1, "{width}");
    assert_ne!(width.fract(), 0.0);
}

#[test]
fn kerning_between_a_letter_a_space_and_a_letter_is_applied() {
    let mut text = context();
    let style = outfit(34.0);
    let mut width = |string: &str| text.measure(string, &style, None).width;
    let (whole, parts) = (width("f A"), width("f") + width(" ") + width("A"));
    assert!(whole < parts - 0.5, "{whole} against {parts}");
    // The design's line, 289.66 dp wide in Chrome; 290.33 without.
    let line = width("100 minutes of AI call practice a month");
    assert!((line - 579.32).abs() < 0.5, "{line}");
    // Pairs without kerning, and other spaces, are untouched.
    assert!((width("n n") - (width("n") * 2.0 + width(" "))).abs() < 0.001);
}

#[test]
fn kerned_lines_keep_their_alignment_and_their_carets() {
    let mut text = context();
    let string = "of AI";
    let start = outfit(34.0);
    let natural = text.measure(string, &start, None).width;
    let right = |text: &mut TextContext, style: &TextStyle| {
        glyphs(text, string, style, Point::new(0.0, 0.0), Some(400.0))
            .iter()
            .map(|(rect, _)| rect.x + rect.width)
            .fold(0.0, f32::max)
    };
    let at_start = right(&mut text, &start);
    let at_end = right(&mut text, &start.align(TextAlign::End));
    // The last letter's ink ends as far from the box's right edge as it
    // ends from the text's own right edge when it starts at the left.
    assert!(((400.0 - at_end) - (natural - at_start)).abs() <= 1.0);
    let caret = text.caret(string, &start, None, string.len());
    assert!(
        (caret.x - natural).abs() < 0.01,
        "{} against {natural}",
        caret.x
    );
}

#[test]
fn a_fractional_origin_selects_a_glyph_image_for_that_fraction() {
    let mut text = context();
    let style = outfit(34.0);
    let at =
        |text: &mut TextContext, x: f32| glyphs(text, "H", &style, Point::new(x, 0.0), None)[0];
    let (whole, quarter, half, next) = (
        at(&mut text, 10.0),
        at(&mut text, 10.25),
        at(&mut text, 10.5),
        at(&mut text, 11.0),
    );
    // Quads stay on whole pixels; the fraction is in the image.
    for (rect, _) in [whole, quarter, half, next] {
        assert_eq!((rect.x.fract(), rect.y.fract()), (0.0, 0.0));
    }
    assert_ne!(whole.1, quarter.1, "a quarter of a pixel is another image");
    assert_ne!(quarter.1, half.1);
    assert_eq!(whole.1, next.1, "a whole pixel is the same image, moved");
    assert_eq!(next.0.x - whole.0.x, 1.0);
    // A fraction too small to matter is the nearest quarter.
    assert_eq!(at(&mut text, 10.05).1, whole.1);
    assert_eq!(at(&mut text, 10.95), next);
}

#[test]
fn the_baseline_follows_the_browsers_half_leading() {
    let mut text = context();
    // 27 px medium on a 35.1 px line at 2 px per dp: ascent 54, descent
    // 14, half-leading 1.1 px, rounded down to 1: baseline 55 below the top.
    let style = TextStyle {
        line_height: 70.2 / 54.0,
        ..outfit(54.0)
    };
    assert_eq!(foot(&mut text, &style, 0.0), 55.0);
    // 32 px bold on a 35.84 px line: ascent 64, descent 17, half-leading
    // -4.66 px, rounded down to -5 (Chrome's line box starts 2.5 dp above
    // the paragraph): baseline 59 below the top.
    let heading = TextStyle {
        line_height: 71.68 / 64.0,
        ..outfit(64.0).weight(TextWeight::BOLD)
    };
    assert_eq!(foot(&mut text, &heading, 0.0), 59.0);
    // `line-height: 1` on a tall display face, 62 px Abril Fatface: ascent
    // 131, descent 36, half-leading -21.5, rounded down to -22.
    let display = TextStyle {
        line_height: 1.0,
        ..TextStyle::new(124.0, Color::WHITE).family(FontFamily::named("Abril Fatface"))
    };
    assert_eq!(foot(&mut text, &display, 0.0), 109.0);
}

#[test]
fn a_lines_top_goes_to_the_nearest_pixel_row_halves_down_the_page() {
    let mut text = context();
    let style = TextStyle {
        line_height: 60.0 / 48.0,
        ..outfit(48.0)
    };
    let at_zero = foot(&mut text, &style, 0.0);
    assert_eq!(foot(&mut text, &style, 0.49), at_zero);
    assert_eq!(foot(&mut text, &style, 0.5), at_zero + 1.0);
    assert_eq!(foot(&mut text, &style, 1.49), at_zero + 1.0);
    assert_eq!(foot(&mut text, &style, -0.5), at_zero);
    assert_eq!(foot(&mut text, &style, -0.51), at_zero - 1.0);
}

#[test]
fn each_line_rounds_its_own_top() {
    let mut text = context();
    // The design's heading: lines 71.68 px apart, the paragraph at 299 px
    // (149.5 dp). Chrome's line boxes are at 147 and 182.84 dp: baselines
    // on rows 294 + 64 and 366 + 64, which are 72 apart, not 71.68.
    let style = TextStyle {
        line_height: 71.68 / 64.0,
        ..outfit(64.0).weight(TextWeight::BOLD)
    };
    let drawn = glyphs(&mut text, "H\nH", &style, Point::new(0.0, 299.0), None);
    let feet: Vec<f32> = drawn.iter().map(|(rect, _)| rect.y + rect.height).collect();
    assert_eq!(feet, [358.0, 430.0]);
    // The paragraph is as tall as its lines, unrounded.
    let height = text.measure("H\nH", &style, None).height;
    assert!((height - 143.36).abs() < 0.01, "{height}");
}

#[test]
fn positions_far_outside_any_surface_and_nan_are_harmless() {
    let mut text = context();
    let style = outfit(20.0);
    for origin in [
        Point::new(f32::NAN, f32::NAN),
        Point::new(f32::INFINITY, f32::NEG_INFINITY),
        Point::new(1.0e30, -1.0e30),
    ] {
        let drawn = glyphs(&mut text, "H", &style, origin, None);
        assert!(
            drawn
                .iter()
                .all(|(rect, _)| rect.x.is_finite() && rect.y.is_finite())
        );
    }
}

/// The atlas bytes of a glyph image.
fn image(text: &TextContext, region: Rect) -> Vec<u8> {
    let atlas = text.atlas();
    let mut bytes = Vec::new();
    for y in region.y as u32..(region.y + region.height) as u32 {
        let start = (y * atlas.width + region.x as u32) as usize;
        bytes.extend_from_slice(&atlas.pixels[start..start + region.width as usize]);
    }
    bytes
}

#[test]
fn light_text_is_weighted_and_dark_text_is_the_plain_coverage() {
    let mut text = context();
    let style = |color: Color| TextStyle {
        color,
        ..outfit(34.0)
    };
    let origin = Point::new(0.0, 0.0);
    let (_, black) = glyphs(&mut text, "e", &style(Color::BLACK), origin, None)[0];
    let (_, white) = glyphs(&mut text, "e", &style(Color::WHITE), origin, None)[0];
    let (_, dark) = glyphs(
        &mut text,
        "e",
        &style(Color::rgb(0.01, 0.01, 0.02)),
        origin,
        None,
    )[0];
    assert_eq!(black, dark, "dark colours share the unweighted image");
    assert_ne!(black, white, "light text has an image of its own");
    assert_eq!((black.width, black.height), (white.width, white.height));

    let (plain, weighted) = (image(&text, black), image(&text, white));
    // Coverage c becomes c^(1/3) for white: nothing appears where there was
    // none, solid stays solid, and every edge pixel is heavier.
    for (&plain, &weighted) in plain.iter().zip(&weighted) {
        let expected = ((f32::from(plain) / 255.0).powf(1.0 / 3.0) * 255.0 + 0.5) as u8;
        assert_eq!(weighted, expected, "coverage {plain}");
    }
    assert!(
        plain.iter().any(|&c| c > 0 && c < 255),
        "the glyph has an edge"
    );
    let ink = |bytes: &[u8]| bytes.iter().map(|&c| u32::from(c)).sum::<u32>();
    assert!(ink(&weighted) > ink(&plain));
}

#[test]
fn tones_run_from_black_to_white_by_the_colours_luma() {
    let mut text = context();
    let mut slot = |color: Color| {
        let style = TextStyle {
            color,
            ..outfit(34.0)
        };
        glyphs(&mut text, "e", &style, Point::new(0.0, 0.0), None)[0].1
    };
    // Four tones: greys at encoded 0, 1/3, 2/3 and 1 each have their own,
    // and a colour takes the tone of its luma (pure blue is dark).
    let grey = |encoded: f32| {
        let linear = ((encoded + 0.055) / 1.055_f32).powf(2.4);
        Color::rgb(linear, linear, linear)
    };
    let tones = [
        slot(Color::BLACK),
        slot(grey(0.34)),
        slot(grey(0.67)),
        slot(Color::WHITE),
    ];
    for (index, tone) in tones.iter().enumerate() {
        assert!(
            !tones[..index].contains(tone),
            "tone {index} is its own image"
        );
    }
    assert_eq!(slot(Color::rgb(0.0, 0.0, 1.0)), tones[0]);
    assert_eq!(slot(grey(0.95)), tones[3]);
    // Alpha does not change the tone.
    assert_eq!(slot(Color::rgba(1.0, 1.0, 1.0, 0.3)), tones[3]);
}

const GROTESK: &[u8] =
    include_bytes!("../../../examples/fundidesk/assets/fonts/SpaceGrotesk-Bold.ttf");

#[test]
fn letter_spacing_turns_optional_ligatures_off() {
    let mut text = TextContext::new();
    text.add_font(GROTESK);
    let style = TextStyle::new(40.0, Color::WHITE).family(FontFamily::named("Space Grotesk"));
    let count = |text: &mut TextContext, style: &TextStyle| {
        glyphs(text, "fi", style, Point::new(0.0, 0.0), None).len()
    };
    // Space Grotesk joins "fi" into one glyph; spaced out, CSS asks for
    // the two letters.
    assert_eq!(count(&mut text, &style), 1, "the font has the ligature");
    assert_eq!(count(&mut text, &style.letter_spacing(2.0)), 2);
    assert_eq!(count(&mut text, &style.letter_spacing(-0.5)), 2);
}

#[test]
fn a_line_may_break_after_a_hyphen() {
    let mut text = context();
    let style = outfit(20.0);
    let whole = text.measure("well-known", &style, None);
    let first = text.measure("well-", &style, None).width;
    let wrapped = text.measure("well-known", &style, Some(first + 2.0));
    assert!(
        wrapped.height > whole.height * 1.5,
        "{wrapped:?} against {whole:?}"
    );
    assert!(wrapped.width <= first + 2.0);
}
