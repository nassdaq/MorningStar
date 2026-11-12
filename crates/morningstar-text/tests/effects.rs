//! Synthetic italic and outlined lettering (ADR 0017), with a real font
//! (Noto Sans; SIL OFL).

use morningstar_core::{AtlasView, Color, DisplayList, DrawOp, Point, Rect};
use morningstar_text::{TextContext, TextSpan, TextStyle};

const LATIN: &[u8] =
    include_bytes!("../../../examples/hello-gpu/assets/fonts/NotoSans-Regular.ttf");

/// Inter (SIL OFL): its digits are proportional unless `tnum` is asked for.
const INTER: &[u8] = include_bytes!("../../../examples/fundidesk/assets/fonts/Inter-Regular.ttf");

const STYLE: TextStyle = TextStyle::new(48.0, Color::WHITE);

fn context() -> TextContext {
    let mut text = TextContext::new();
    text.add_font(LATIN);
    text
}

/// The glyphs of a text drawn at the origin: where they go and where their
/// masks are in the atlas.
fn glyphs(text: &mut TextContext, string: &str, style: &TextStyle) -> Vec<(Rect, Rect)> {
    let mut list = DisplayList::new();
    text.draw(&mut list, string, style, Point::new(0.0, 0.0), None);
    list.ops()
        .iter()
        .filter_map(|op| match *op {
            DrawOp::Glyph { rect, atlas, .. } => Some((rect, atlas)),
            _ => None,
        })
        .collect()
}

/// The horizontal centre of the coverage in one row of an atlas region.
fn row_centre(atlas: AtlasView<'_>, region: Rect, row: u32) -> Option<f32> {
    let (mut weight, mut moment) = (0.0, 0.0);
    for x in 0..region.width as u32 {
        let index = (region.y as u32 + row) * atlas.width + region.x as u32 + x;
        let coverage = f32::from(atlas.pixels[index as usize]);
        weight += coverage;
        moment += coverage * (x as f32 + 0.5);
    }
    (weight > 0.0).then(|| moment / weight)
}

#[test]
fn italic_without_an_italic_face_slants_the_glyphs_and_keeps_the_width() {
    let mut text = context();
    let upright = text.measure("Illicit", &STYLE, None);
    let italic = text.measure("Illicit", &STYLE.italic(), None);
    assert_eq!(upright, italic, "a synthetic italic keeps the advances");

    // The stem of an "l" is vertical upright and leans right in italic:
    // by a quarter of its height (tan 14 degrees), top to bottom.
    let upright = glyphs(&mut text, "l", &STYLE);
    let slanted = glyphs(&mut text, "l", &STYLE.italic());
    let (upright_rect, upright_region) = upright[0];
    let (slanted_rect, slanted_region) = slanted[0];
    assert!(
        slanted_rect.width > upright_rect.width + 4.0,
        "the slanted stem is wider: {} against {}",
        slanted_rect.width,
        upright_rect.width
    );
    let atlas = text.atlas();
    let last = upright_region.height as u32 - 2;
    let lean = |region: Rect| {
        row_centre(atlas, region, 1).unwrap() - row_centre(atlas, region, last).unwrap()
    };
    assert!(lean(upright_region).abs() < 0.2, "upright stem is vertical");
    let expected = 0.2493 * (last - 1) as f32;
    let leaned = lean(slanted_region);
    assert!(
        (leaned - expected).abs() < 1.0,
        "the top is {leaned} px right of the bottom; expected {expected}"
    );
}

#[test]
fn a_span_can_be_italic_on_its_own() {
    let mut text = context();
    let mut list = DisplayList::new();
    let spans = [TextSpan::new("l"), TextSpan::new("l").italic()];
    text.draw_spans(&mut list, &spans, &STYLE, Point::new(0.0, 0.0), None);
    let widths: Vec<f32> = list
        .ops()
        .iter()
        .filter_map(|op| match *op {
            DrawOp::Glyph { rect, .. } => Some(rect.width),
            _ => None,
        })
        .collect();
    assert_eq!(widths.len(), 2);
    assert!(widths[1] > widths[0] + 4.0, "{widths:?}");
    // And it is a different layout from the all-upright one.
    let plain = [TextSpan::new("l"), TextSpan::new("l")];
    let mut upright = DisplayList::new();
    text.draw_spans(&mut upright, &plain, &STYLE, Point::new(0.0, 0.0), None);
    assert_ne!(list, upright);
}

fn pixel(outlined: &morningstar_text::OutlinedText, x: u32, y: u32) -> [u8; 4] {
    let start = ((y * outlined.width + x) * 4) as usize;
    outlined.pixels[start..start + 4].try_into().unwrap()
}

#[test]
fn outlined_lettering_is_the_fill_with_a_stroke_around_it() {
    let mut text = context();
    let red = Color::rgb(1.0, 0.0, 0.0);
    let style = TextStyle::new(96.0, red);
    let spans = [TextSpan::new("l")];
    let outlined = text
        .rasterise_outlined(&spans, &style, None, 4.0, Color::WHITE)
        .expect("an outline");
    assert_eq!(
        outlined.pixels.len(),
        (outlined.width * outlined.height * 4) as usize
    );

    // The image covers the glyph, which the ordinary draw places, with
    // room for the stroke around it.
    let (glyph, _) = glyphs(&mut text, "l", &style)[0];
    assert!(outlined.offset.x <= glyph.x - 2.0 && outlined.offset.y <= glyph.y - 2.0);
    assert!(outlined.offset.x + outlined.width as f32 >= glyph.x + glyph.width + 2.0);
    assert!(outlined.offset.y + outlined.height as f32 >= glyph.y + glyph.height + 2.0);

    // Across the middle of the stem: nothing, white stroke, red fill,
    // white stroke, nothing. The stroke is centred on the outline, so 2 px
    // of it cover the fill's edge and 2 px lie outside.
    let row = (glyph.y + glyph.height / 2.0 - outlined.offset.y) as u32;
    let colours: Vec<[u8; 4]> = (0..outlined.width)
        .map(|x| pixel(&outlined, x, row))
        .collect();
    let white = colours.iter().filter(|c| **c == [255; 4]).count();
    let fill = colours.iter().filter(|c| **c == [255, 0, 0, 255]).count();
    assert!(
        (6..=8).contains(&white),
        "{white} stroke pixels: {colours:?}"
    );
    assert!(fill >= 2, "{fill} fill pixels: {colours:?}");
    assert_eq!(colours[0], [0; 4]);
    assert_eq!(colours[colours.len() - 1], [0; 4]);
    let first_white = colours.iter().position(|c| *c == [255; 4]).unwrap();
    let first_fill = colours.iter().position(|c| *c == [255, 0, 0, 255]).unwrap();
    assert!(first_white < first_fill);
    // The stem is `fill + 4` wide in all (the stroke hides 2 px of each
    // side), which is the glyph's own width to within its side bearings.
    let stem = fill as f32 + 4.0;
    assert!(
        (stem - glyph.width).abs() <= 2.5,
        "stem {stem}, glyph {glyph:?}"
    );

    // Without a stroke it is the fill alone, and narrower.
    let plain = text
        .rasterise_outlined(&spans, &style, None, 0.0, Color::WHITE)
        .unwrap();
    assert!(plain.width < outlined.width);
    assert!(plain.pixels.chunks_exact(4).all(|p| p[1] == 0 && p[2] == 0));
}

#[test]
fn nothing_to_outline_gives_nothing() {
    let mut text = context();
    assert!(
        text.rasterise_outlined(&[TextSpan::new("")], &STYLE, None, 2.0, Color::WHITE)
            .is_none()
    );
    assert!(
        text.rasterise_outlined(&[TextSpan::new("   ")], &STYLE, None, 2.0, Color::WHITE)
            .is_none()
    );
    let mut empty = TextContext::new();
    assert!(
        empty
            .rasterise_outlined(&[TextSpan::new("l")], &STYLE, None, 2.0, Color::WHITE)
            .is_none()
    );
}

#[test]
fn tabular_numbers_give_every_digit_the_same_width() {
    let mut text = TextContext::new();
    text.add_font(INTER);
    let proportional = STYLE.family(morningstar_text::FontFamily::named("Inter"));
    let tabular = proportional.tabular_nums();
    let width = |text: &mut TextContext, string: &str, style: &TextStyle| {
        text.measure(string, style, None).width
    };
    // Proportional: a row of ones is narrower than a row of zeros.
    let ones = width(&mut text, "1111", &proportional);
    let zeros = width(&mut text, "0000", &proportional);
    assert!(ones < zeros - 4.0, "{ones} against {zeros}");
    // Tabular: the same width whatever the digits.
    let ones = width(&mut text, "1111", &tabular);
    let zeros = width(&mut text, "0000", &tabular);
    assert!((ones - zeros).abs() < 0.01, "{ones} against {zeros}");
    assert!(
        (width(&mut text, "02:14", &tabular) - width(&mut text, "11:11", &tabular)).abs() < 0.01
    );
    // Letters are untouched.
    assert_eq!(
        width(&mut text, "Clarity", &tabular),
        width(&mut text, "Clarity", &proportional)
    );

    // A font without the feature is left as it is.
    let mut noto = context();
    assert_eq!(
        noto.measure("1011", &STYLE, None),
        noto.measure("1011", &STYLE.tabular_nums(), None)
    );
}
