//! Layout tests with real fonts (Noto Sans, Noto Sans Arabic; SIL OFL).

use morningstar_core::{Color, DisplayList, DrawOp, Point, Rect};
use morningstar_text::{TextAlign, TextContext, TextStyle};

const LATIN: &[u8] =
    include_bytes!("../../../examples/hello-gpu/assets/fonts/NotoSans-Regular.ttf");
const BOLD: &[u8] = include_bytes!("../../../examples/hello-gpu/assets/fonts/NotoSans-Bold.ttf");
const ARABIC: &[u8] =
    include_bytes!("../../../examples/hello-gpu/assets/fonts/NotoSansArabic-Regular.ttf");

const STYLE: TextStyle = TextStyle::new(32.0, Color::WHITE);

fn context() -> TextContext {
    let mut text = TextContext::new();
    text.add_font(LATIN);
    text.add_font(BOLD);
    text.add_font(ARABIC);
    text
}

fn glyph_rects(list: &DisplayList) -> Vec<Rect> {
    list.ops()
        .iter()
        .filter_map(|op| match *op {
            DrawOp::Glyph { rect, .. } => Some(rect),
            _ => None,
        })
        .collect()
}

fn draw(text: &mut TextContext, string: &str, style: &TextStyle, width: Option<f32>) -> Vec<Rect> {
    let mut list = DisplayList::new();
    text.draw(&mut list, string, style, Point::new(0.0, 0.0), width);
    glyph_rects(&list)
}

#[test]
fn without_fonts_text_is_empty_and_harmless() {
    let mut text = TextContext::new();
    assert_eq!(text.font_count(), 0);
    assert_eq!(text.measure("Hello", &STYLE, None).width, 0.0);
    assert!(draw(&mut text, "Hello", &STYLE, None).is_empty());
}

#[test]
fn one_glyph_per_visible_letter_left_to_right() {
    let mut text = context();
    let rects = draw(&mut text, "Habari yako", &STYLE, None);
    // Ten letters; the space has no image.
    assert_eq!(rects.len(), 10);
    assert!(
        rects.windows(2).all(|pair| pair[0].x < pair[1].x),
        "glyphs advance left to right"
    );
}

#[test]
fn measure_matches_draw_and_scales_with_size() {
    let mut text = context();
    let small = text.measure("Morningstar", &STYLE, None);
    let large = text.measure("Morningstar", &TextStyle::new(64.0, Color::WHITE), None);
    assert!(small.width > 100.0 && small.width < 300.0, "{small:?}");
    assert!(
        (large.width / small.width - 2.0).abs() < 0.05,
        "{small:?} {large:?}"
    );
    // One line: 32px at the default 1.3 line height.
    assert!((small.height - 41.6).abs() < 0.5, "{small:?}");

    let mut list = DisplayList::new();
    let drawn = text.draw(&mut list, "Morningstar", &STYLE, Point::new(0.0, 0.0), None);
    assert_eq!(drawn, small);
    let right_edge = glyph_rects(&list)
        .iter()
        .map(|rect| rect.x + rect.width)
        .fold(0.0, f32::max);
    assert!(
        (right_edge - small.width).abs() < 4.0,
        "ink ends near the measured width"
    );
}

#[test]
fn bold_is_wider_than_regular() {
    let mut text = context();
    let regular = text.measure("Increment", &STYLE, None);
    let bold = text.measure("Increment", &STYLE.bold(), None);
    assert!(bold.width > regular.width, "{regular:?} vs {bold:?}");
}

#[test]
fn long_text_wraps_within_the_width() {
    let mut text = context();
    let sentence = "Morningstar is a Rust-native mobile application framework";
    let single = text.measure(sentence, &STYLE, None);
    let wrapped = text.measure(sentence, &STYLE, Some(300.0));

    assert!(single.width > 300.0);
    assert!(wrapped.width <= 300.0, "{wrapped:?}");
    assert!(
        wrapped.height >= single.height * 3.0,
        "several lines: {wrapped:?}"
    );

    let rects = draw(&mut text, sentence, &STYLE, Some(300.0));
    assert!(rects.iter().all(|rect| rect.x + rect.width <= 301.0));
}

#[test]
fn explicit_newlines_break_lines() {
    let mut text = context();
    let one = text.measure("Moja", &STYLE, None);
    let two = text.measure("Moja\nMbili", &STYLE, None);
    assert!((two.height - one.height * 2.0).abs() < 0.5);
}

#[test]
fn centre_and_end_alignment_shift_the_line() {
    let mut text = context();
    let left = draw(&mut text, "Sawa", &STYLE, Some(400.0))[0].x;
    let centre = draw(
        &mut text,
        "Sawa",
        &STYLE.align(TextAlign::Center),
        Some(400.0),
    )[0]
    .x;
    let end = draw(&mut text, "Sawa", &STYLE.align(TextAlign::End), Some(400.0))[0].x;
    assert!(left < 5.0, "starts at the left: {left}");
    assert!(centre > 100.0 && centre < 250.0, "centred: {centre}");
    assert!(end > centre + 50.0, "pushed to the right: {end}");
}

#[test]
fn arabic_is_shaped_and_laid_out_right_to_left() {
    let mut text = context();
    // "Peace be upon you": two words, joined letters.
    let phrase = "السلام عليكم";

    let rects = draw(&mut text, phrase, &STYLE, Some(400.0));
    assert!(!rects.is_empty(), "the Arabic font was used");

    // A right-to-left paragraph hugs the right edge of the available width.
    let right_edge = rects.iter().map(|r| r.x + r.width).fold(0.0, f32::max);
    let left_edge = rects.iter().map(|r| r.x).fold(f32::MAX, f32::min);
    assert!(right_edge > 380.0, "right edge at {right_edge}");
    assert!(left_edge > 150.0, "left edge at {left_edge}");
}

#[test]
fn arabic_letters_take_their_joined_forms() {
    let mut text = context();
    // Three "beh". Joined, they take compact initial, medial and final
    // forms. A zero-width non-joiner between them forces the wide isolated
    // form of each. If shaping were not applied the two would be identical.
    let joined = text.measure("ببب", &STYLE, None).width;
    let separated = text.measure("ب\u{200C}ب\u{200C}ب", &STYLE, None).width;
    assert!(joined > 0.0);
    assert!(
        joined < separated * 0.85,
        "joined {joined} should be clearly narrower than separated {separated}"
    );
}

#[test]
fn mixed_direction_text_uses_both_fonts() {
    let mut text = context();
    let latin = draw(&mut text, "Karibu", &STYLE, None).len();
    let arabic = draw(&mut text, "مرحبا", &STYLE, None).len();
    let mixed = draw(&mut text, "Karibu مرحبا", &STYLE, None).len();
    assert!(arabic > 0);
    assert_eq!(mixed, latin + arabic);
}

#[test]
fn a_whole_pixel_origin_moves_every_glyph_by_exactly_that() {
    let mut text = context();
    let at_zero = draw(&mut text, "Aa", &STYLE, None);

    let mut list = DisplayList::new();
    text.draw(&mut list, "Aa", &STYLE, Point::new(100.0, 51.0), None);
    let moved = glyph_rects(&list);

    assert_eq!(at_zero.len(), moved.len());
    for (a, b) in at_zero.iter().zip(&moved) {
        assert_eq!(b.x - a.x, 100.0);
        assert_eq!(b.y - a.y, 51.0);
    }
}

#[test]
fn repeated_text_reuses_atlas_and_frames_evict_unused_layouts() {
    let mut text = context();
    draw(&mut text, "Hello", &STYLE, None);
    let after_first = text.atlas().generation;
    // "l" appears twice in "Hello" but only one "l" image was rasterised,
    // and drawing the same text again adds nothing.
    draw(&mut text, "Hello", &STYLE, None);
    assert_eq!(text.atlas().generation, after_first);

    // Layouts survive while drawn each frame and are dropped when not.
    text.begin_frame();
    draw(&mut text, "Hello", &STYLE, None);
    text.begin_frame();
    text.begin_frame();
    // Still correct after eviction: it is simply shaped again.
    assert_eq!(draw(&mut text, "Hello", &STYLE, None).len(), 5);
    assert_eq!(
        text.atlas().generation,
        after_first,
        "glyph images were kept"
    );
}

#[test]
fn degenerate_styles_do_not_panic() {
    let mut text = context();
    for size in [0.0, -5.0, f32::NAN] {
        let style = TextStyle::new(size, Color::WHITE);
        let _ = text.measure("x", &style, None);
    }
    let _ = text.measure("x", &STYLE.line_height(0.0), None);
    let _ = text.measure("x", &STYLE, Some(0.0));
    let _ = text.measure("", &STYLE, None);
}

/// Every non-zero texel that `after` has and `before` lacks (or has with a
/// different value) must lie in the rows `after` reports as dirty.
fn changes_lie_within_dirty_rows(before: &[u8], after: &morningstar_core::AtlasView<'_>) {
    let dirty = after.dirty.expect("row tracking");
    assert_eq!(before.len(), after.pixels.len(), "same dimensions");
    let width = after.width as usize;
    for (index, (old, new)) in before.iter().zip(after.pixels).enumerate() {
        let row = (index / width) as u32;
        if old != new {
            assert!(
                (dirty.start..dirty.end).contains(&row),
                "row {row} changed outside {dirty:?}"
            );
        }
    }
}

#[test]
fn atlas_reports_only_the_rows_new_glyphs_touched() {
    let mut text = context();
    // First frame: nothing has been uploaded, so there is nothing to be
    // relative to.
    draw(&mut text, "Hello", &STYLE, None);
    assert_eq!(text.atlas().dirty, None);

    // Second frame, same text: nothing changed.
    text.begin_frame();
    let uploaded = text.atlas().generation;
    let before = text.atlas().pixels.to_vec();
    draw(&mut text, "Hello", &STYLE, None);
    let dirty = text.atlas().dirty.expect("row tracking");
    assert_eq!((dirty.since, dirty.start, dirty.end), (uploaded, 0, 0));
    assert_eq!(text.atlas().generation, uploaded);

    // New glyphs in that frame: a few rows, relative to what was uploaded.
    draw(&mut text, "Wq", &STYLE, None);
    let view = text.atlas();
    let dirty = view.dirty.expect("row tracking");
    assert!(view.generation > uploaded);
    assert_eq!(dirty.since, uploaded);
    assert!(dirty.start < dirty.end);
    assert!(
        dirty.end - dirty.start < 64,
        "two 32px glyphs dirtied rows {}..{}",
        dirty.start,
        dirty.end
    );
    changes_lie_within_dirty_rows(&before, &view);
    assert_ne!(before, view.pixels, "the new glyphs are in the image");

    // The next frame starts a new record relative to the new contents.
    text.begin_frame();
    let before = text.atlas().pixels.to_vec();
    let uploaded = text.atlas().generation;
    draw(&mut text, "Zz", &STYLE, None);
    assert_eq!(text.atlas().dirty.expect("row tracking").since, uploaded);
    changes_lie_within_dirty_rows(&before, &text.atlas());
}

#[test]
fn atlas_growth_and_rebuild_ask_for_a_full_upload() {
    let mut text = context();
    draw(&mut text, "a", &STYLE, None);
    text.begin_frame();
    let initial_height = text.atlas().height;

    // Large glyphs until the atlas has to grow.
    let big = TextStyle::new(160.0, Color::WHITE);
    let mut letters = ('A'..='Z').chain('a'..='z');
    while text.atlas().height == initial_height {
        let letter = letters
            .next()
            .expect("the atlas grew before the alphabet ran out");
        draw(&mut text, &letter.to_string(), &big, None);
    }
    let view = text.atlas();
    assert_eq!(view.pixels.len(), (view.width * view.height) as usize);
    assert_eq!(
        view.dirty, None,
        "a resized atlas cannot be patched by rows"
    );

    // Once that has been uploaded, tracking resumes at the new size.
    text.begin_frame();
    assert!(text.atlas().dirty.is_some());

    // Overflow: more distinct large glyphs than the atlas can ever hold. The
    // frame after the overflow rebuilds it, which again needs a full upload.
    let huge = TextStyle::new(400.0, Color::WHITE);
    let before_overflow = text.atlas().generation;
    for letter in ('A'..='Z').chain('a'..='z') {
        draw(&mut text, &letter.to_string(), &huge, None);
    }
    text.begin_frame();
    let view = text.atlas();
    assert_eq!(view.height, initial_height, "rebuilt at the initial size");
    assert!(view.generation > before_overflow);
    assert_eq!(view.dirty, None);
    assert!(view.pixels.iter().all(|&texel| texel == 0));

    // Glyphs drawn after the rebuild are part of that same full upload, and
    // the frame after it is back to rows.
    draw(&mut text, "a", &STYLE, None);
    assert_eq!(text.atlas().dirty, None);
    text.begin_frame();
    draw(&mut text, "b", &STYLE, None);
    let dirty = text.atlas().dirty.expect("row tracking");
    assert!(dirty.start < dirty.end);
}

// ------------------------------------------------- families, spacing, spans

const INTER: &[u8] = include_bytes!("../../../examples/fundidesk/assets/fonts/Inter-Regular.ttf");
const INTER_BOLD: &[u8] = include_bytes!("../../../examples/fundidesk/assets/fonts/Inter-Bold.ttf");
const GROTESK: &[u8] =
    include_bytes!("../../../examples/fundidesk/assets/fonts/SpaceGrotesk-Bold.ttf");

/// Noto Sans first (so it is what unnamed text gets), then two named
/// families and the Arabic fallback.
fn families() -> TextContext {
    let mut text = context();
    text.add_font(INTER);
    text.add_font(INTER_BOLD);
    text.add_font(GROTESK);
    text
}

fn glyph_ops(list: &DisplayList) -> Vec<(Rect, Rect, Color)> {
    list.ops()
        .iter()
        .filter_map(|op| match *op {
            DrawOp::Glyph { rect, atlas, color } => Some((rect, atlas, color)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_named_family_selects_its_font() {
    use morningstar_text::FontFamily;
    let mut text = families();
    assert!(text.has_family("Inter"));
    assert!(text.has_family("space grotesk"), "names ignore case");
    assert!(!text.has_family("Helvetica"));

    let sample = "Kariakoo Phone Clinic 0123456789";
    let unnamed = text.measure(sample, &STYLE, None);
    let noto = text.measure(sample, &STYLE.family(FontFamily::named("Noto Sans")), None);
    let inter = text.measure(sample, &STYLE.family(FontFamily::named("Inter")), None);
    let grotesk = text.measure(sample, &STYLE.family("Space Grotesk".into()).bold(), None);
    assert_eq!(unnamed, noto, "unnamed text uses the first font added");
    assert!((inter.width - noto.width).abs() > 4.0, "{inter:?} {noto:?}");
    let inter_bold = text.measure(sample, &STYLE.family("Inter".into()).bold(), None);
    assert!(
        (grotesk.width - inter_bold.width).abs() > 4.0,
        "{grotesk:?} {inter_bold:?}"
    );
    // The families' own line metrics differ too.
    assert_eq!(
        grotesk.height, inter.height,
        "line height comes from the style"
    );

    // A family nobody added falls back to the fonts in the order added.
    let missing = text.measure(sample, &STYLE.family("Helvetica".into()), None);
    assert_eq!(missing, unnamed);
}

#[test]
fn two_families_in_one_frame_use_different_glyphs() {
    let mut text = families();
    let mut list = DisplayList::new();
    let inter = STYLE.family("Inter".into());
    let grotesk = STYLE.family("Space Grotesk".into()).bold();
    text.draw(&mut list, "Rag", &inter, Point::new(0.0, 0.0), None);
    text.draw(&mut list, "Rag", &grotesk, Point::new(0.0, 60.0), None);
    text.draw(&mut list, "Rag", &inter, Point::new(0.0, 120.0), None);
    let glyphs = glyph_ops(&list);
    assert_eq!(glyphs.len(), 9);
    for index in 0..3 {
        assert_ne!(
            glyphs[index].1,
            glyphs[index + 3].1,
            "different atlas slots"
        );
        assert_eq!(glyphs[index].1, glyphs[index + 6].1, "the same glyph again");
    }
}

#[test]
fn a_family_weight_is_matched_within_the_family() {
    let mut text = families();
    let regular = STYLE.family("Inter".into());
    let narrow = text.measure("Balance to collect", &regular, None);
    let wide = text.measure("Balance to collect", &regular.bold(), None);
    assert!(wide.width > narrow.width + 2.0);
}

#[test]
fn characters_a_family_lacks_come_from_the_other_fonts() {
    let mut text = families();
    let inter = STYLE.family("Inter".into());
    // Inter has no Arabic.
    let arabic = "\u{645}\u{631}\u{62d}\u{628}\u{627}";
    let named = draw(&mut text, arabic, &inter, None);
    let unnamed = draw(&mut text, arabic, &STYLE, None);
    assert!(!named.is_empty(), "the Arabic font is used as fallback");
    assert_eq!(named, unnamed);
    // Mixed: the Latin part is Inter, the Arabic part is the fallback.
    let mixed = text.measure(&format!("Inter {arabic}"), &inter, None);
    let latin = text.measure("Inter ", &inter, None);
    assert!(mixed.width > latin.width + 20.0);
}

#[test]
fn letter_spacing_adds_to_every_advance() {
    let mut text = families();
    let style = STYLE.family("Inter".into());
    let word = "CUSTOMER";
    let plain = text.measure(word, &style, None);
    for spacing in [1.5f32, -0.8, 4.0] {
        let spaced = text.measure(word, &style.letter_spacing(spacing), None);
        let expected = plain.width + spacing * word.len() as f32;
        assert!(
            (spaced.width - expected).abs() < 0.6,
            "spacing {spacing}: {} vs {expected}",
            spaced.width
        );
        assert_eq!(spaced.height, plain.height);
    }
    // Zero and nonsense leave the text alone.
    assert_eq!(text.measure(word, &style.letter_spacing(0.0), None), plain);
    assert_eq!(
        text.measure(word, &style.letter_spacing(f32::NAN), None),
        plain
    );
    // Glyphs move apart, not just the measured box.
    let near = draw(&mut text, word, &style, None);
    let far = draw(&mut text, word, &style.letter_spacing(4.0), None);
    assert!((far[7].x - near[7].x - 28.0).abs() <= 1.0);
}

#[test]
fn spans_flow_as_one_paragraph_with_their_own_weight_and_colour() {
    use morningstar_text::{TextSpan, TextWeight};
    let mut text = families();
    let style = STYLE.family("Inter".into());
    let green = Color::rgb(0.0, 1.0, 0.0);
    let spans = [
        TextSpan::new("New here? "),
        TextSpan::new("Join").weight(TextWeight::BOLD).color(green),
    ];

    let lead = text.measure("New here? Join", &style, None);
    let whole = text.measure_spans(&spans, &style, None);
    let bold = text.measure("Join", &style.bold(), None);
    let regular = text.measure("Join", &style, None);
    assert_eq!(whole.height, lead.height, "one line");
    let expected = lead.width - regular.width + bold.width;
    assert!(
        (whole.width - expected).abs() < 1.5,
        "{whole:?} vs {expected}"
    );

    let mut list = DisplayList::new();
    let drawn = text.draw_spans(&mut list, &spans, &style, Point::new(0.0, 0.0), None);
    assert_eq!(drawn, whole);
    let glyphs = glyph_ops(&list);
    assert_eq!(glyphs.len(), 12);
    assert!(glyphs[..8].iter().all(|glyph| glyph.2 == Color::WHITE));
    assert!(glyphs[8..].iter().all(|glyph| glyph.2 == green));

    // The spans wrap as one text: narrow enough, the paragraph breaks.
    let wrapped = text.measure_spans(&spans, &style, Some(whole.width - 10.0));
    assert!(wrapped.height > whole.height * 1.5);

    // A single plain span is exactly the plain call.
    assert_eq!(
        text.measure_spans(&[TextSpan::new("New here? Join")], &style, None),
        lead
    );
    // Different colours are different cache entries, not a stale reuse.
    let red = Color::rgb(1.0, 0.0, 0.0);
    let recoloured = [spans[0], spans[1].color(red)];
    let mut list = DisplayList::new();
    text.draw_spans(&mut list, &recoloured, &style, Point::new(0.0, 0.0), None);
    assert!(glyph_ops(&list)[8..].iter().all(|glyph| glyph.2 == red));
}

#[test]
fn a_span_can_change_family() {
    use morningstar_text::{FontFamily, TextSpan};
    let mut text = families();
    let style = STYLE.family("Inter".into());
    let grotesk = FontFamily::named("Space Grotesk");
    let plain = text.measure("Ref FD-2026", &style, None);
    let mixed = text.measure_spans(
        &[
            TextSpan::new("Ref "),
            TextSpan::new("FD-2026").family(grotesk),
        ],
        &style,
        None,
    );
    assert!((mixed.width - plain.width).abs() > 1.0);
}

// -------------------------------------------------------------------- icons

mod icons {
    use morningstar_core::{Color, DisplayList, DrawOp, Rect};
    use morningstar_text::{TextContext, VectorIcon};

    const CHECK: VectorIcon = VectorIcon::stroked("M5 12l5 5 9-10");
    const SQUARE: VectorIcon =
        VectorIcon::svg(r#"<svg viewBox="0 0 8 8"><rect x="2" y="2" width="4" height="4"/></svg>"#);
    const BROKEN: VectorIcon = VectorIcon::svg(r#"<svg viewBox="0 0 8 8"><g/></svg>"#);

    fn glyphs(list: &DisplayList) -> Vec<(Rect, Rect, Color)> {
        list.ops()
            .iter()
            .filter_map(|op| match *op {
                DrawOp::Glyph { rect, atlas, color } => Some((rect, atlas, color)),
                _ => None,
            })
            .collect()
    }

    /// The atlas texels of one drawn icon, row by row.
    fn texels(text: &TextContext, region: Rect) -> Vec<u8> {
        let atlas = text.atlas();
        let mut out = Vec::new();
        for row in 0..region.height as u32 {
            let start = ((region.y as u32 + row) * atlas.width + region.x as u32) as usize;
            out.extend_from_slice(&atlas.pixels[start..start + region.width as usize]);
        }
        out
    }

    #[test]
    fn an_icon_is_one_glyph_quad_snapped_to_pixels_in_its_colour() {
        let mut text = TextContext::new();
        let mut list = DisplayList::new();
        let red = Color::rgb(1.0, 0.0, 0.0);
        text.draw_icon(
            &mut list,
            &SQUARE,
            Rect::new(10.4, 20.6, 16.0, 16.0),
            red,
            None,
        );
        let drawn = glyphs(&list);
        assert_eq!(drawn.len(), 1);
        let (rect, region, color) = drawn[0];
        assert_eq!(rect, Rect::new(10.0, 21.0, 16.0, 16.0));
        assert_eq!((region.width, region.height), (16.0, 16.0));
        assert_eq!(color, red);

        // The mask is the icon at exactly that size: a 8 x 8 square at 4,4.
        let mask = texels(&text, region);
        for y in 0..16 {
            for x in 0..16 {
                let inside = (4..12).contains(&x) && (4..12).contains(&y);
                assert_eq!(mask[y * 16 + x], if inside { 255 } else { 0 }, "{x},{y}");
            }
        }
    }

    #[test]
    fn masks_are_cached_by_icon_size_and_stroke_width() {
        let mut text = TextContext::new();
        let mut list = DisplayList::new();
        let white = Color::WHITE;
        let at = |x: f32, side: f32| Rect::new(x, 0.0, side, side);
        text.draw_icon(&mut list, &CHECK, at(0.0, 24.0), white, None);
        let generation = text.atlas().generation;
        // Again, elsewhere and in another colour: the same mask, no new work.
        text.draw_icon(&mut list, &CHECK, at(100.0, 24.0), Color::BLACK, None);
        assert_eq!(text.atlas().generation, generation);
        // Another size, another stroke width, another icon: new masks.
        text.draw_icon(&mut list, &CHECK, at(0.0, 48.0), white, None);
        text.draw_icon(&mut list, &CHECK, at(0.0, 24.0), white, Some(3.0));
        text.draw_icon(&mut list, &SQUARE, at(0.0, 24.0), white, None);
        let drawn = glyphs(&list);
        assert_eq!(drawn.len(), 5);
        assert_eq!(drawn[0].1, drawn[1].1);
        let regions: std::collections::HashSet<_> = drawn
            .iter()
            .map(|glyph| (glyph.1.x as u32, glyph.1.y as u32))
            .collect();
        assert_eq!(regions.len(), 4);
        assert_ne!(text.atlas().generation, generation);

        // A heavier stroke covers more.
        let ink =
            |region: Rect| -> u32 { texels(&text, region).iter().map(|&v| u32::from(v)).sum() };
        assert!(ink(drawn[3].1) > ink(drawn[0].1) * 5 / 4);
        // Twice the size is four times the ink, to within anti-aliasing.
        let ratio = ink(drawn[2].1) as f32 / ink(drawn[0].1) as f32;
        assert!((ratio - 4.0).abs() < 0.2, "{ratio}");
    }

    #[test]
    fn icons_survive_frames_and_share_the_atlas_with_text() {
        const FONT: &[u8] =
            include_bytes!("../../../examples/hello-gpu/assets/fonts/NotoSans-Regular.ttf");
        let mut text = TextContext::new();
        text.add_font(FONT);
        let style = morningstar_text::TextStyle::new(20.0, Color::WHITE);
        let mut first = DisplayList::new();
        text.draw(
            &mut first,
            "Ab",
            &style,
            morningstar_core::Point::new(0.0, 0.0),
            None,
        );
        text.draw_icon(
            &mut first,
            &CHECK,
            Rect::new(0.0, 0.0, 24.0, 24.0),
            Color::WHITE,
            None,
        );
        let before = glyphs(&first);
        assert_eq!(before.len(), 3);
        // No two of them overlap in the atlas.
        for a in 0..3 {
            for b in a + 1..3 {
                let (p, q) = (before[a].1, before[b].1);
                let apart = p.x + p.width <= q.x
                    || q.x + q.width <= p.x
                    || p.y + p.height <= q.y
                    || q.y + q.height <= p.y;
                assert!(apart, "{p:?} overlaps {q:?}");
            }
        }
        for _ in 0..3 {
            text.begin_frame();
        }
        let generation = text.atlas().generation;
        let mut later = DisplayList::new();
        text.draw_icon(
            &mut later,
            &CHECK,
            Rect::new(0.0, 0.0, 24.0, 24.0),
            Color::WHITE,
            None,
        );
        assert_eq!(glyphs(&later)[0].1, before[2].1);
        assert_eq!(text.atlas().generation, generation);
    }

    #[test]
    fn unsupported_and_degenerate_icons_draw_nothing() {
        let mut text = TextContext::new();
        let mut list = DisplayList::new();
        assert!(BROKEN.check().is_err());
        for _ in 0..2 {
            text.draw_icon(
                &mut list,
                &BROKEN,
                Rect::new(0.0, 0.0, 24.0, 24.0),
                Color::WHITE,
                None,
            );
        }
        for rect in [
            Rect::new(0.0, 0.0, 0.0, 24.0),
            Rect::new(0.0, 0.0, 0.3, 0.3),
            Rect::new(0.0, 0.0, f32::NAN, 24.0),
            Rect::new(f32::NAN, 0.0, 24.0, 24.0),
            Rect::new(0.0, 0.0, f32::INFINITY, 24.0),
            Rect::new(0.0, 0.0, -5.0, 24.0),
        ] {
            text.draw_icon(&mut list, &CHECK, rect, Color::WHITE, None);
        }
        // A transparent colour is dropped by the display list.
        let clear = Color::rgba(1.0, 1.0, 1.0, 0.0);
        text.draw_icon(
            &mut list,
            &CHECK,
            Rect::new(0.0, 0.0, 24.0, 24.0),
            clear,
            None,
        );
        assert!(list.is_empty());
    }

    #[test]
    fn a_huge_icon_is_rasterised_at_the_cap_and_stretched() {
        let mut text = TextContext::new();
        let mut list = DisplayList::new();
        text.draw_icon(
            &mut list,
            &SQUARE,
            Rect::new(0.0, 0.0, 2000.0, 1000.0),
            Color::WHITE,
            None,
        );
        let (rect, region, _) = glyphs(&list)[0];
        assert_eq!((rect.width, rect.height), (2000.0, 1000.0));
        assert_eq!((region.width, region.height), (512.0, 256.0));
        // Drawing it again next frame does not rebuild the atlas.
        text.begin_frame();
        let generation = text.atlas().generation;
        text.draw_icon(
            &mut list,
            &SQUARE,
            Rect::new(0.0, 0.0, 2000.0, 1000.0),
            Color::WHITE,
            None,
        );
        assert_eq!(text.atlas().generation, generation);
    }

    #[test]
    fn many_icon_sizes_refill_the_atlas_without_corrupting_it() {
        let mut text = TextContext::new();
        // Far more mask area than the atlas holds (2048 rows at most).
        for round in 0..3 {
            text.begin_frame();
            let mut list = DisplayList::new();
            for side in 0..40u32 {
                let side = (300 + side * 5 + round) as f32;
                text.draw_icon(
                    &mut list,
                    &SQUARE,
                    Rect::new(0.0, 0.0, side, side),
                    Color::WHITE,
                    None,
                );
            }
            for (_, region, _) in glyphs(&list) {
                let atlas = text.atlas();
                assert!(region.x + region.width <= atlas.width as f32);
                assert!(region.y + region.height <= atlas.height as f32);
                // The middle of every mask drawn this frame is ink.
                let x = (region.x + region.width / 2.0) as u32;
                let y = (region.y + region.height / 2.0) as u32;
                assert_eq!(atlas.pixels[(y * atlas.width + x) as usize], 255);
            }
        }
    }
}
