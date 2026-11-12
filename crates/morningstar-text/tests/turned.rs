//! Text drawn through a transform (ADR 0029): its glyphs are rasterised
//! for the transform and placed in screen space, unless the transform is
//! changing from frame to frame. With a real font (Noto Sans; SIL OFL).

use morningstar_core::{AtlasView, Color, DisplayList, DrawOp, Point, Rect, Transform};
use morningstar_text::{TextContext, TextStyle};

const LATIN: &[u8] =
    include_bytes!("../../../examples/hello-gpu/assets/fonts/NotoSans-Regular.ttf");

const STYLE: TextStyle = TextStyle::new(48.0, Color::BLACK);

fn context() -> TextContext {
    let mut text = TextContext::new();
    text.add_font(LATIN);
    text
}

/// One frame: `string` drawn at `origin` through `transform`.
fn frame(
    text: &mut TextContext,
    string: &str,
    origin: Point,
    transform: Option<Transform>,
) -> DisplayList {
    text.begin_frame();
    let mut list = DisplayList::new();
    if let Some(transform) = transform {
        list.push_transform(transform);
    }
    text.draw(&mut list, string, &STYLE, origin, None);
    if transform.is_some() {
        list.pop_transform();
    }
    list
}

fn glyphs(list: &DisplayList) -> Vec<(Rect, Rect)> {
    list.ops()
        .iter()
        .filter_map(|op| match *op {
            DrawOp::Glyph { rect, atlas, .. } => Some((rect, atlas)),
            _ => None,
        })
        .collect()
}

fn in_screen_space(list: &DisplayList) -> bool {
    list.ops()
        .iter()
        .any(|op| matches!(op, DrawOp::PushScreenSpace))
}

/// The coverage of an atlas region, row by row.
fn mask(atlas: AtlasView<'_>, region: Rect) -> Vec<Vec<u8>> {
    (0..region.height as u32)
        .map(|row| {
            let start = ((region.y as u32 + row) * atlas.width + region.x as u32) as usize;
            atlas.pixels[start..start + region.width as usize].to_vec()
        })
        .collect()
}

fn area(mask: &[Vec<u8>]) -> f32 {
    mask.iter()
        .flatten()
        .map(|&coverage| f32::from(coverage) / 255.0)
        .sum()
}

const ORIGIN: Point = Point::new(100.0, 100.0);

#[test]
fn upright_and_moved_text_is_drawn_as_it_always_was() {
    let mut text = context();
    let plain = frame(&mut text, "Hello", ORIGIN, None);
    assert!(!in_screen_space(&plain));
    assert_eq!(plain.len(), 5, "five glyphs and nothing else");
    // A move is not a turn: the same glyph operations, under the push.
    let mut text = context();
    let moved = frame(
        &mut text,
        "Hello",
        ORIGIN,
        Some(Transform::translate(7.0, 3.0)),
    );
    assert!(!in_screen_space(&moved));
    assert_eq!(glyphs(&moved), glyphs(&plain));
}

#[test]
fn turned_text_is_rasterised_turned_and_placed_on_the_pixel_grid() {
    let turn = Transform::rotate(90.0);
    let mut upright = context();
    let plain = glyphs(&frame(&mut upright, "l", Point::new(0.0, 0.0), None));
    let mut text = context();
    let list = frame(&mut text, "l", Point::new(0.0, 0.0), Some(turn));
    // The glyphs are bracketed: screen space, inside the transform.
    assert!(matches!(list.ops()[0], DrawOp::PushTransform { .. }));
    assert_eq!(list.ops()[1], DrawOp::PushScreenSpace);
    assert_eq!(list.ops()[list.len() - 2], DrawOp::PopScreenSpace);
    assert_eq!(list.ops()[list.len() - 1], DrawOp::PopTransform);
    let turned = glyphs(&list);
    assert_eq!(turned.len(), 1);

    // An "l" is a tall stem; a quarter turn clockwise lays it on its side,
    // pointing left of the pen: the mask's sides swap.
    let (stem, stem_region) = plain[0];
    let (lying, lying_region) = turned[0];
    assert!(stem.height > 3.0 * stem.width, "{stem:?}");
    assert!(
        (lying.width - stem.height).abs() <= 1.0,
        "{lying:?} against {stem:?}"
    );
    assert!((lying.height - stem.width).abs() <= 1.0);
    assert_eq!(
        lying,
        Rect::new(lying.x.round(), lying.y.round(), lying.width, lying.height)
    );
    // (x, y) goes to (-y, x): what was above the baseline is to its left.
    assert!(
        (lying.x + lying.width - (-stem.y)).abs() <= 1.0,
        "{lying:?}"
    );
    assert!((lying.y - stem.x).abs() <= 1.0);

    // The same ink: the stem's area, and the mask itself turned.
    let before = mask(upright.atlas(), stem_region);
    let after = mask(text.atlas(), lying_region);
    assert!((area(&before) - area(&after)).abs() < area(&before) * 0.02);
    let middle_row = &after[after.len() / 2];
    assert!(
        middle_row
            .iter()
            .filter(|&&coverage| coverage == 255)
            .count() as f32
            > stem.height - 4.0,
        "a solid row along the lying stem"
    );
}

#[test]
fn a_slanted_stem_is_sharp_where_resampling_would_blur_it() {
    // An "l" turned by 8 degrees: across the middle of the stem a row has
    // solid ink between two partly covered pixels, as an upright stem has.
    let mut text = context();
    let list = frame(&mut text, "l", ORIGIN, Some(Transform::rotate(8.0)));
    let (rect, region) = glyphs(&list)[0];
    assert!(rect.width > 8.0, "the slant widens the mask: {rect:?}");
    let rows = mask(text.atlas(), region);
    let row = &rows[rows.len() / 2];
    let solid = row.iter().filter(|&&coverage| coverage == 255).count();
    let partial = row
        .iter()
        .filter(|&&coverage| (1..255).contains(&coverage))
        .count();
    assert!(solid >= 3, "the stem's middle is solid: {row:?}");
    assert!(partial <= 3, "one graded pixel either side: {row:?}");
}

#[test]
fn glyphs_follow_the_transform_along_the_baseline() {
    let turn = Transform::rotate(30.0).about(ORIGIN);
    let mut text = context();
    let list = frame(&mut text, "nnnn", ORIGIN, Some(turn));
    let rects: Vec<Rect> = glyphs(&list).iter().map(|glyph| glyph.0).collect();
    assert_eq!(rects.len(), 4);
    // Each next "n" is further right and further down, by the advance
    // along a line at 30 degrees: a step of (cos, sin) times the advance.
    let steps: Vec<(f32, f32)> = rects
        .windows(2)
        .map(|pair| (pair[1].x - pair[0].x, pair[1].y - pair[0].y))
        .collect();
    for (dx, dy) in &steps {
        assert!(*dx > 0.0 && *dy > 0.0);
        assert!((dy / dx - 0.577).abs() < 0.08, "tan 30: {dx} {dy}");
    }
    // The same letter at different fractions of a pixel: more than one
    // mask of it, at most sixteen.
    let mut regions: Vec<[u32; 4]> = glyphs(&list)
        .iter()
        .map(|glyph| [glyph.1.x, glyph.1.y, glyph.1.width, glyph.1.height].map(|v| v as u32))
        .collect();
    regions.sort_unstable();
    regions.dedup();
    assert!((2..=4).contains(&regions.len()), "{regions:?}");
}

#[test]
fn a_scale_rasterises_the_glyphs_at_their_size_on_screen() {
    let mut upright = context();
    let plain = glyphs(&frame(&mut upright, "H", Point::new(0.0, 0.0), None))[0].0;
    let mut text = context();
    let list = frame(
        &mut text,
        "H",
        Point::new(0.0, 0.0),
        Some(Transform::scale(2.0, 2.0)),
    );
    assert!(in_screen_space(&list));
    let doubled = glyphs(&list)[0].0;
    assert!(
        (doubled.width - 2.0 * plain.width).abs() <= 2.0,
        "{doubled:?} {plain:?}"
    );
    assert!((doubled.height - 2.0 * plain.height).abs() <= 2.0);

    // Too large for the atlas to be a sensible home: left to the renderer.
    let mut text = context();
    let list = frame(
        &mut text,
        "H",
        Point::new(0.0, 0.0),
        Some(Transform::scale(4.0, 4.0)),
    );
    assert!(!in_screen_space(&list), "48 px at 4x is over the limit");
    assert_eq!(glyphs(&list)[0].0, plain);
    // And a transform that cannot be drawn through draws as before.
    for transform in [Transform::scale(0.0, 1.0), Transform::scale(f32::NAN, 1.0)] {
        let mut text = context();
        let list = frame(&mut text, "H", Point::new(0.0, 0.0), Some(transform));
        assert!(!in_screen_space(&list));
    }
}

#[test]
fn a_changing_angle_is_resampled_and_the_frame_after_it_stops_is_sharp() {
    let mut text = context();
    let at = |degrees: f32| Some(Transform::rotate(degrees).about(ORIGIN));

    // The first frame of a text is sharp, whatever its angle.
    let first = frame(&mut text, "Hello", ORIGIN, at(5.0));
    assert!(in_screen_space(&first));
    assert!(!text.take_settle_request());
    let after_first = text.atlas().generation;

    // The angle changes every frame: the renderer turns the upright
    // glyphs, as before this existed, and one more frame is asked for.
    let mut upright = context();
    let plain = glyphs(&frame(&mut upright, "Hello", ORIGIN, None));
    let mut generation = None;
    for step in 1..=40 {
        let list = frame(&mut text, "Hello", ORIGIN, at(5.0 + step as f32));
        assert!(!in_screen_space(&list), "step {step}");
        let drawn: Vec<Rect> = glyphs(&list).iter().map(|glyph| glyph.0).collect();
        let expected: Vec<Rect> = plain.iter().map(|glyph| glyph.0).collect();
        assert_eq!(
            drawn, expected,
            "the upright rectangles, under the transform"
        );
        assert!(text.take_settle_request());
        assert!(!text.take_settle_request(), "asking clears it");
        // Nothing enters the atlas after the upright glyphs have, once.
        if let Some(generation) = generation {
            assert_eq!(text.atlas().generation, generation, "step {step}");
        }
        generation = Some(text.atlas().generation);
    }
    assert_ne!(generation, Some(after_first), "the upright glyphs went in");

    // The frame that was asked for: the angle held, so it is sharp, and
    // nothing more is asked.
    let settled = frame(&mut text, "Hello", ORIGIN, at(45.0));
    assert!(in_screen_space(&settled));
    assert!(!text.take_settle_request());
    let again = frame(&mut text, "Hello", ORIGIN, at(45.0));
    assert_eq!(glyphs(&again), glyphs(&settled));
    assert!(!text.take_settle_request());
}

#[test]
fn moving_turned_text_keeps_it_sharp() {
    let mut text = context();
    let turn = |x: f32| Some(Transform::rotate(-8.0).then(Transform::translate(x, 0.0)));
    let first = frame(&mut text, "Hello", ORIGIN, turn(0.0));
    let generation = text.atlas().generation;
    let mut widest = 0;
    for step in 1..=40 {
        // A slide: whole and fractional pixels.
        let list = frame(&mut text, "Hello", ORIGIN, turn(step as f32 * 3.3));
        assert!(in_screen_space(&list));
        assert!(!text.take_settle_request());
        assert_eq!(glyphs(&list).len(), glyphs(&first).len());
        widest = widest.max(text.atlas().height);
    }
    // New masks only for new fractions of a pixel, sixteen per glyph at
    // most: the atlas stays its first size.
    assert_ne!(text.atlas().generation, generation);
    assert_eq!(widest, text.atlas().height);
    assert!(text.atlas().height <= 512, "{}", text.atlas().height);
}

#[test]
fn the_same_text_at_two_angles_in_one_frame_is_sharp_at_both() {
    let mut text = context();
    for _ in 0..3 {
        text.begin_frame();
        let mut list = DisplayList::new();
        for degrees in [5.0, -5.0] {
            list.push_transform(Transform::rotate(degrees));
            text.draw(&mut list, "Angry", &STYLE, ORIGIN, None);
            list.pop_transform();
        }
        let spaces = list
            .ops()
            .iter()
            .filter(|op| matches!(op, DrawOp::PushScreenSpace))
            .count();
        assert_eq!(spaces, 2);
        assert!(!text.take_settle_request());
    }
}
