//! Transforms, text effects and vector pictures in the UI layer (ADR 0017):
//! what they put in the display list, that layout does not notice them, and
//! that touches follow what is drawn.

mod common;

use common::{Harness, Model, Msg};
use morningstar_core::{DrawOp, Point, Rect};
use morningstar_ui::prelude::*;

const RED: Color = hex(0xFF0000);
const BLUE: Color = hex(0x0000FF);

fn transforms(harness: &Harness) -> Vec<Transform> {
    harness
        .ops()
        .iter()
        .filter_map(|op| match *op {
            DrawOp::PushTransform { transform } => Some(transform),
            _ => None,
        })
        .collect()
}

fn close(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() < 1e-2 && (a.y - b.y).abs() < 1e-2
}

// ---------------------------------------------------------------- paint

fn tile(turn: Option<f32>) -> Element<Msg> {
    let tile = Container::new(text("Angry customer").id("label"))
        .size(200.0, 40.0)
        .padding(8.0)
        .background(RED)
        .border(2.0, BLUE)
        .radius(4.0)
        .id("tile");
    let tile = match turn {
        Some(degrees) => tile.rotate(degrees),
        None => tile,
    };
    col![
        Container::empty().size(10.0, 20.0).id("above"),
        tile,
        Container::empty().size(10.0, 20.0).id("below"),
    ]
    .padding(30.0)
    .into()
}

#[test]
fn a_turned_tile_is_laid_out_as_if_it_were_not() {
    let plain = Harness::new(|_| tile(None));
    let turned = Harness::new(|_| tile(Some(-8.0)));
    for id in ["above", "tile", "label", "below"] {
        assert_eq!(plain.px_rect(id), turned.px_rect(id), "{id}");
    }
    assert!(turned.ui.overflows().is_empty());
    assert!(transforms(&plain).is_empty());
}

#[test]
fn a_turned_tile_is_drawn_through_a_transform_about_its_centre() {
    let plain = Harness::new(|_| tile(None));
    let turned = Harness::new(|_| tile(Some(90.0)));
    let [transform] = transforms(&turned)[..] else {
        panic!("one transform expected: {:?}", transforms(&turned));
    };
    // The tile is 200 x 40 dp at (30, 50): 600 x 120 px at (90, 150). Its
    // centre (390, 210) stays put; its left end goes to the top.
    let rect = turned.px_rect("tile");
    assert_eq!(rect, Rect::new(90.0, 150.0, 600.0, 120.0));
    assert!(close(transform.apply(rect.center()), rect.center()));
    assert!(close(
        transform.apply(Point::new(90.0, 210.0)),
        Point::new(390.0, -90.0)
    ));

    // The tile's own operations are the same as without the turn, between
    // the push and the pop: background with border, then the label.
    let ops = turned.ops();
    let push = ops
        .iter()
        .position(|op| matches!(op, DrawOp::PushTransform { .. }))
        .unwrap();
    let pop = ops
        .iter()
        .position(|op| matches!(op, DrawOp::PopTransform))
        .unwrap();
    let inside = &ops[push + 1..pop];
    assert!(matches!(inside[0], DrawOp::BorderedRect { rect: r, .. } if r == rect));
    assert_eq!(
        inside
            .iter()
            .filter(|op| matches!(op, DrawOp::Glyph { .. }))
            .count(),
        13,
        "the label turns with the tile"
    );
    let unturned: Vec<_> = ops
        .iter()
        .filter(|op| !matches!(op, DrawOp::PushTransform { .. } | DrawOp::PopTransform))
        .collect();
    assert_eq!(unturned.len(), plain.ops().len());
    assert!(unturned.iter().zip(plain.ops()).all(|(a, b)| *a == b));
}

#[test]
fn transform_lengths_are_dp_and_css_lists_read_left_to_right() {
    // CSS `transform: rotate(90deg) translateY(-8px)`: the move is along
    // the turned axes, so "up" has become "right" on screen.
    let harness = Harness::new(|_| {
        Container::new(
            Container::empty()
                .size(40.0, 20.0)
                .background(RED)
                .transform(Transform::rotate(90.0).and_translate(0.0, -8.0))
                .id("box"),
        )
        .padding(50.0)
        .into()
    });
    let [transform] = transforms(&harness)[..] else {
        panic!("one transform expected");
    };
    let rect = harness.px_rect("box");
    assert_eq!(rect, Rect::new(150.0, 150.0, 120.0, 60.0));
    // 8 dp is 24 px at this scale.
    let centre = rect.center();
    assert!(close(
        transform.apply(centre),
        Point::new(centre.x + 24.0, centre.y)
    ));
}

#[test]
fn a_transform_that_only_moves_needs_no_display_list_transform() {
    let harness = Harness::new(|_| {
        Container::new(
            Container::empty()
                .size(40.0, 20.0)
                .background(RED)
                .on_press(Msg::Tap(1))
                .transform(Transform::translate(10.0, -5.0))
                .id("box"),
        )
        .padding(50.0)
        .into()
    });
    assert!(transforms(&harness).is_empty());
    // Moved by whole pixels, like `offset`: 30 px right, 15 px up.
    assert_eq!(harness.px_rect("box"), Rect::new(180.0, 135.0, 120.0, 60.0));
    assert_eq!(
        harness.rects_with_color(RED),
        [Rect::new(180.0, 135.0, 120.0, 60.0)]
    );
    let mut harness = harness;
    harness.tap(98.0, 50.0);
    assert_eq!(harness.log(), [Msg::Tap(1)]);
    harness.tap(52.0, 66.0);
    assert_eq!(harness.log().len(), 1, "where it was laid out is empty now");
}

#[test]
fn a_flattened_widget_is_skipped_whole_and_its_siblings_are_unharmed() {
    let harness = Harness::new(|_| {
        col![
            Container::new(text("gone").id("hidden"))
                .background(RED)
                .on_press(Msg::Tap(1))
                .transform(Transform::scale(0.0, 1.0)),
            text("kept").id("kept"),
            Container::empty().size(20.0, 20.0).background(BLUE),
        ]
        .into()
    });
    assert!(transforms(&harness).is_empty());
    assert!(harness.rects_with_color(RED).is_empty());
    assert_eq!(harness.glyph_count(), 4, "only the second text is drawn");
    assert_eq!(harness.rects_with_color(BLUE).len(), 1);
    // It still takes its room in the column.
    assert!(harness.px_rect("kept").y > 0.0);
    let mut harness = harness;
    harness.tap(10.0, 5.0);
    assert!(harness.log().is_empty());
}

// ------------------------------------------------------------------ touch

/// A 200 x 20 dp button at (100, 200), turned a quarter about its centre
/// (200, 210): on screen it is 20 x 200 at (190, 110).
fn turned_button(_: &Model) -> Element<Msg> {
    stack![
        Container::empty()
            .size(200.0, 20.0)
            .background(RED)
            .on_press(Msg::Tap(7))
            .rotate(90.0)
            .offset(100.0, 200.0)
            .id("button"),
    ]
    .fill()
    .into()
}

#[test]
fn a_turned_button_is_tapped_where_it_is_drawn() {
    let mut harness = Harness::new(turned_button);
    assert_eq!(
        harness.dp_rect("button"),
        Rect::new(100.0, 200.0, 200.0, 20.0),
        "rect_of reports the layout rectangle"
    );
    // The ends of the unturned bar are empty now.
    harness.tap(110.0, 210.0);
    harness.tap(290.0, 210.0);
    assert!(harness.log().is_empty());
    // The turned bar: above, below and at its centre.
    harness.tap(200.0, 115.0);
    harness.tap(200.0, 305.0);
    harness.tap(200.0, 210.0);
    assert_eq!(harness.log(), [Msg::Tap(7), Msg::Tap(7), Msg::Tap(7)]);
    // Just beside it, and just beyond its ends.
    harness.tap(188.0, 210.0);
    harness.tap(211.0, 210.0);
    harness.tap(200.0, 108.0);
    harness.tap(200.0, 311.0);
    assert_eq!(harness.log().len(), 3);
}

#[test]
fn sliding_off_a_turned_button_cancels_the_tap() {
    let mut harness = Harness::new(turned_button);
    harness.down(200.0, 120.0);
    harness.draw();
    // Along the turned bar: still inside. Sideways: off it.
    harness.moved(200.0, 124.0);
    harness.up(200.0, 124.0);
    assert_eq!(harness.log().len(), 1);
    harness.draw();
    harness.down(200.0, 120.0);
    harness.draw();
    harness.moved(204.0, 120.0);
    harness.moved(214.0, 120.0);
    harness.up(214.0, 120.0);
    assert_eq!(harness.log().len(), 1);
}

#[test]
fn a_clip_around_a_turned_button_still_limits_where_it_can_be_tapped() {
    // The same turned bar inside a 60 dp tall clipping box around its
    // middle: only the part inside the box shows and reacts.
    let mut harness = Harness::new(|_| {
        stack![
            Container::new(
                Container::empty()
                    .size(200.0, 20.0)
                    .background(RED)
                    .on_press(Msg::Tap(7))
                    .rotate(90.0)
                    .offset(0.0, 20.0),
            )
            .size(200.0, 60.0)
            .clip()
            .offset(100.0, 180.0),
        ]
        .fill()
        .into()
    });
    // The bar is drawn at x 190..210, y 110..310; the clip is y 180..240.
    harness.tap(200.0, 210.0);
    assert_eq!(harness.log(), [Msg::Tap(7)]);
    harness.tap(200.0, 150.0);
    harness.tap(200.0, 280.0);
    assert_eq!(harness.log().len(), 1, "outside the clip");
}

#[test]
fn nested_transforms_compose_for_drawing_and_for_touch() {
    // A quarter turn inside a quarter turn is a half turn: a button at the
    // left end of a bar ends up at its right end.
    let mut harness = Harness::new(|_| {
        stack![
            Container::new(
                row![
                    Container::empty()
                        .size(40.0, 40.0)
                        .background(RED)
                        .on_press(Msg::Tap(3)),
                    Container::empty().size(160.0, 40.0),
                ]
                .rotate(90.0),
            )
            .rotate(90.0)
            .offset(100.0, 200.0),
        ]
        .fill()
        .into()
    });
    assert_eq!(transforms(&harness).len(), 2);
    // The bar spans x 100..300, y 200..240; the red square was at its
    // left end and is drawn at its right end.
    harness.tap(120.0, 220.0);
    assert!(harness.log().is_empty());
    harness.tap(280.0, 220.0);
    assert_eq!(harness.log(), [Msg::Tap(3)]);
}

// ------------------------------------------------------------------- text

fn glyph_rects(harness: &Harness) -> Vec<(Rect, Color)> {
    harness
        .ops()
        .iter()
        .filter_map(|op| match *op {
            DrawOp::Glyph { rect, color, .. } => Some((rect, color)),
            _ => None,
        })
        .collect()
}

#[test]
fn italic_text_takes_the_same_room_with_slanted_glyphs() {
    let upright = Harness::new(|_| text("Illicit").size(40.0).id("t").into());
    let italic = Harness::new(|_| text("Illicit").size(40.0).italic().id("t").into());
    assert_eq!(upright.px_rect("t"), italic.px_rect("t"));
    let (upright, italic) = (glyph_rects(&upright), glyph_rects(&italic));
    assert_eq!(upright.len(), italic.len());
    // Every stem leans: its mask is wider.
    for (plain, slanted) in upright.iter().zip(&italic) {
        assert!(slanted.0.width > plain.0.width, "{plain:?} {slanted:?}");
    }
    assert!(transforms(&Harness::new(|_| text("x").italic().into())).is_empty());

    // One span of a rich text on its own.
    let rich = Harness::new(|_| {
        Text::rich([span("ll"), span("ll").italic()])
            .size(40.0)
            .into()
    });
    let widths: Vec<f32> = glyph_rects(&rich).iter().map(|g| g.0.width).collect();
    assert_eq!(widths.len(), 4);
    assert_eq!(widths[0], widths[1]);
    assert!(widths[2] > widths[0] && widths[3] > widths[0], "{widths:?}");
}

#[test]
fn a_text_shadow_is_the_same_glyphs_drawn_first_and_moved() {
    let plain = Harness::new(|_| text("31").size(40.0).color(RED).id("t").into());
    let shadowed = Harness::new(|_| {
        text("31")
            .size(40.0)
            .color(RED)
            .shadow(3.0, -2.0, BLUE)
            .id("t")
            .into()
    });
    assert_eq!(plain.px_rect("t"), shadowed.px_rect("t"), "no room taken");
    let (plain, shadowed) = (glyph_rects(&plain), glyph_rects(&shadowed));
    assert_eq!(shadowed.len(), 4);
    // The shadow comes first, in its own colour, 9 px right and 6 px up.
    for index in 0..2 {
        let (shadow, glyph) = (shadowed[index], shadowed[index + 2]);
        assert_eq!(glyph, plain[index]);
        assert_eq!(shadow.1, BLUE);
        assert_eq!(
            shadow.0,
            Rect::new(
                glyph.0.x + 9.0,
                glyph.0.y - 6.0,
                glyph.0.width,
                glyph.0.height
            )
        );
    }

    // A shadow under a rich text is all in the shadow's colour.
    let rich = Harness::new(|_| {
        Text::rich([span("3").color(RED), span("1")])
            .shadow(1.0, 1.0, BLUE)
            .into()
    });
    let colors: Vec<Color> = glyph_rects(&rich).iter().map(|g| g.1).collect();
    assert_eq!(colors[..2], [BLUE, BLUE]);
    assert_eq!(colors[2], RED);
}

#[test]
fn outlined_text_is_one_image_in_the_place_of_the_glyphs() {
    let plain = Harness::new(|_| text("2").size(70.0).color(RED).id("t").into());
    let outlined = Harness::new(|_| {
        text("2")
            .size(70.0)
            .color(RED)
            .outline(1.5, BLUE)
            .id("t")
            .into()
    });
    assert_eq!(plain.px_rect("t"), outlined.px_rect("t"));
    assert_eq!(outlined.glyph_count(), 0);
    let images: Vec<Rect> = outlined
        .ops()
        .iter()
        .filter_map(|op| match *op {
            DrawOp::Image { rect, .. } => Some(rect),
            _ => None,
        })
        .collect();
    let [image] = images[..] else {
        panic!("one image expected: {images:?}");
    };
    // The image covers the digit's glyph, with room for the stroke (1.5 dp
    // is 4.5 px, half of it outside the outline).
    let (glyph, _) = glyph_rects(&plain)[0];
    assert!(
        image.x <= glyph.x - 2.0 && image.x > glyph.x - 14.0,
        "{image:?} {glyph:?}"
    );
    assert!(image.y <= glyph.y - 2.0 && image.y > glyph.y - 14.0);
    assert!(image.x + image.width >= glyph.x + glyph.width + 2.0);
    assert!(image.y + image.height >= glyph.y + glyph.height + 2.0);
    assert_eq!(outlined.frame.picture_cache_len(), 1);

    // The next frame draws the same image again without rasterising.
    let mut outlined = outlined;
    let before = outlined.frame.images().generation();
    outlined.draw();
    assert_eq!(outlined.frame.images().generation(), before);
}

// --------------------------------------------------------------- pictures

const QUARTERS: VectorPicture = VectorPicture::new(
    40.0,
    20.0,
    &[
        Layer::rect(0.0, 0.0, 20.0, 20.0).fill(RED),
        Layer::circle(30.0, 10.0, 8.0)
            .fill(BLUE)
            .stroke(with_alpha(RED, 0.5), 2.0),
    ],
);

fn images(harness: &Harness) -> Vec<(Rect, Rect, f32, Color)> {
    harness
        .ops()
        .iter()
        .filter_map(|op| match *op {
            DrawOp::Image {
                rect,
                source,
                corner_radius,
                tint,
                ..
            } => Some((rect, source, corner_radius, tint)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_vector_picture_is_sized_by_its_view_box_unless_told_otherwise() {
    let harness = Harness::new(|_| {
        col![
            Picture::vector(QUARTERS).id("natural"),
            Picture::vector(QUARTERS).width(100.0).id("by-width"),
            Picture::vector(QUARTERS).height(40.0).id("by-height"),
            Picture::vector(QUARTERS).size(30.0, 30.0).id("boxed"),
        ]
        .into()
    });
    assert!(QUARTERS.check().is_ok());
    assert_eq!(harness.dp_rect("natural"), Rect::new(0.0, 0.0, 40.0, 20.0));
    assert_eq!(
        harness.dp_rect("by-width"),
        Rect::new(0.0, 20.0, 100.0, 50.0)
    );
    assert_eq!(
        harness.dp_rect("by-height"),
        Rect::new(0.0, 70.0, 80.0, 40.0)
    );
    assert_eq!(harness.dp_rect("boxed"), Rect::new(0.0, 110.0, 30.0, 30.0));

    // Each is one image operation, rasterised for the pixels it covers
    // (3 px per dp): the image's own pixels map one to one.
    let drawn = images(&harness);
    assert_eq!(drawn.len(), 4);
    for (rect, source, ..) in &drawn {
        assert_eq!((source.width, source.height), (rect.width, rect.height));
    }
    assert_eq!(drawn[0].0, Rect::new(0.0, 0.0, 120.0, 60.0));
    assert_eq!(drawn[3].0, Rect::new(0.0, 330.0, 90.0, 90.0));
    assert_eq!(harness.frame.picture_cache_len(), 4);
    assert_eq!(harness.frame.images().len(), 5, "and the harness's image");
}

#[test]
fn a_vector_picture_takes_radius_tint_and_a_transform() {
    let harness = Harness::new(|_| {
        col![
            Picture::vector(QUARTERS)
                .size(80.0, 40.0)
                .radius(6.0)
                .tint(with_alpha(Color::WHITE, 0.5))
                .rotate(-18.0)
        ]
        .into()
    });
    let ops = harness.ops();
    assert!(matches!(ops[0], DrawOp::PushTransform { .. }));
    assert!(matches!(
        ops[1],
        DrawOp::Image { rect, corner_radius, tint, .. }
            if rect == Rect::new(0.0, 0.0, 240.0, 120.0) && corner_radius == 18.0 && tint.a == 0.5
    ));
    assert_eq!(ops[2], DrawOp::PopTransform);
}

#[test]
fn a_vector_picture_out_of_view_is_not_rasterised() {
    let mut harness = Harness::new(|_| {
        ScrollView::new(col![
            Container::empty().size(10.0, 1000.0),
            Picture::vector(QUARTERS).size(80.0, 40.0),
            Container::empty().size(10.0, 1000.0),
        ])
        .into()
    });
    // The picture is 200 dp below the bottom of the 800 dp screen.
    assert_eq!(harness.frame.picture_cache_len(), 0);
    assert!(images(&harness).is_empty());
    // Drag up by 600 dp: now it is in view, and rasterised.
    harness.down(100.0, 700.0);
    for step in 1..=30 {
        harness.moved(100.0, 700.0 - 20.0 * step as f32);
        harness.step(0.016);
    }
    assert_eq!(images(&harness).len(), 1);
    assert_eq!(harness.frame.picture_cache_len(), 1);
}
