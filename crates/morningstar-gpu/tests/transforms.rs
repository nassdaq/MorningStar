//! Pixel tests for transforms (ADR 0017): `push_transform` turns, slants,
//! scales and moves every kind of operation as a whole, keeps its edges
//! anti-aliased, composes when nested, and interacts with clips as the
//! display list documents.
//!
//! Like the other pixel tests these run on the real GPU of the machine and
//! skip where there is none. The target is RGBA8 with premultiplied alpha and
//! holds sRGB-encoded values (ADR 0023); the colours here have components
//! of 0 or 1, so expected bytes are the blended value times 255.

use morningstar_core::{
    AtlasView, Border, Color, Dash, DisplayList, ImageStore, LinearGradient, Point, Rect, Shadow,
    SurfaceSize, Transform,
};
use morningstar_gpu::{GpuError, OffscreenRenderer, render_offscreen_with_images};

const R: [u8; 4] = [255, 0, 0, 255];
const B: [u8; 4] = [0, 0, 255, 255];

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const GREEN: Color = Color::rgb(0.0, 1.0, 0.0);
const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);

/// A one-texel, fully covered atlas: glyphs drawn from it are solid boxes.
const SOLID_ATLAS: AtlasView<'static> = AtlasView::new(1, 1, &[255], 1);
const SOLID_GLYPH: Rect = Rect::new(0.0, 0.0, 1.0, 1.0);

/// A rendered frame with pixel lookup.
struct Frame {
    width: u32,
    pixels: Vec<u8>,
}

impl Frame {
    fn at(&self, x: u32, y: u32) -> [u8; 4] {
        let start = ((y * self.width + x) * 4) as usize;
        self.pixels[start..start + 4].try_into().unwrap()
    }

    /// Sum of one channel over the frame, in whole pixels of full value.
    fn total(&self, channel: usize) -> f32 {
        self.pixels
            .chunks_exact(4)
            .map(|pixel| f32::from(pixel[channel]) / 255.0)
            .sum()
    }
}

fn skip_without_gpu<T>(result: Result<T, GpuError>) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(GpuError::NoAdapter(reason)) => {
            eprintln!("skipping: no GPU adapter available ({reason})");
            None
        }
        Err(other) => panic!("offscreen render failed: {other}"),
    }
}

fn render_with(
    size: SurfaceSize,
    clear: Color,
    list: &DisplayList,
    atlas: AtlasView<'_>,
    store: &ImageStore,
) -> Option<Frame> {
    let pixels = skip_without_gpu(render_offscreen_with_images(
        size,
        clear,
        list,
        atlas,
        store.view(),
    ))?;
    assert_eq!(pixels.len(), (size.width * size.height * 4) as usize);
    Some(Frame {
        width: size.width,
        pixels,
    })
}

fn render(size: SurfaceSize, clear: Color, list: &DisplayList) -> Option<Frame> {
    render_with(size, clear, list, SOLID_ATLAS, &ImageStore::new())
}

/// `transform` carried out about the centre of `rect`.
fn about_center(transform: Transform, rect: Rect) -> Transform {
    transform.about(rect.center())
}

fn assert_close(actual: [u8; 4], expected: [f32; 4], tolerance: f32, what: &str) {
    for channel in 0..4 {
        let difference = (f32::from(actual[channel]) - expected[channel] * 255.0).abs();
        assert!(
            difference <= tolerance,
            "{what}: channel {channel} is {} but {} was expected",
            actual[channel],
            expected[channel] * 255.0
        );
    }
}

/// Signed distance to a rounded box centred on the origin, as the shader
/// computes it.
fn rounded_box_distance(point: (f32, f32), half: (f32, f32), radius: f32) -> f32 {
    let q = (
        point.0.abs() - half.0 + radius,
        point.1.abs() - half.1 + radius,
    );
    (q.0.max(0.0).powi(2) + q.1.max(0.0).powi(2)).sqrt() + q.0.max(q.1).min(0.0) - radius
}

#[test]
fn a_rect_turned_an_eighth_covers_a_diamond() {
    // A 20 px square centred on (32, 32), turned 45 degrees: a diamond
    // whose points are 14.14 px from the centre.
    let square = Rect::new(22.0, 22.0, 20.0, 20.0);
    let mut list = DisplayList::new();
    list.push_transform(about_center(Transform::rotate(45.0), square));
    list.fill_rect(square, RED);
    list.pop_transform();
    let Some(frame) = render(SurfaceSize::new(64, 64), BLUE, &list) else {
        return;
    };

    // Centre and well inside each point of the diamond.
    for (x, y) in [(32, 32), (32, 20), (32, 43), (20, 32), (43, 32)] {
        assert_eq!(frame.at(x, y), R, "inside at {x},{y}");
    }
    // The square's own corners are now outside, as is everything beyond
    // the diamond's points.
    for (x, y) in [(23, 23), (40, 23), (23, 40), (40, 40), (32, 16), (48, 32)] {
        assert_eq!(frame.at(x, y), B, "outside at {x},{y}");
    }
    // On the slanted edge the pixel is partly covered: anti-aliased, not
    // stair-stepped. This pixel's centre is 0.1 px inside the edge.
    let [red, _, blue, alpha] = frame.at(32, 18);
    assert!((140..=170).contains(&red), "edge red was {red}");
    assert_eq!(u16::from(red) + u16::from(blue), 255);
    assert_eq!(alpha, 255);
    // Turning does not change the area: 400 px, to within the edge's
    // falloff.
    let area = frame.total(0);
    assert!((area - 400.0).abs() < 4.0, "red area was {area}");
}

#[test]
fn a_quarter_turn_maps_pixels_onto_pixels_exactly() {
    // 30 x 10 at (10, 20), turned 90 degrees about its centre (25, 25):
    // 10 x 30 at (20, 10).
    let rect = Rect::new(10.0, 20.0, 30.0, 10.0);
    let mut list = DisplayList::new();
    list.push_transform(about_center(Transform::rotate(90.0), rect));
    list.fill_rect(rect, RED);
    list.pop_transform();
    let Some(frame) = render(SurfaceSize::new(50, 50), BLUE, &list) else {
        return;
    };
    for y in 0..50 {
        for x in 0..50 {
            let inside = (20..30).contains(&x) && (10..40).contains(&y);
            assert_eq!(frame.at(x, y), if inside { R } else { B }, "pixel {x},{y}");
        }
    }
}

#[test]
fn a_half_turn_reverses_a_gradient_and_a_border_exactly() {
    let rect = Rect::new(4.0, 6.0, 24.0, 12.0);
    let size = SurfaceSize::new(32, 24);
    let draw = |list: &mut DisplayList, left: Color, right: Color| {
        list.fill_gradient_rect(rect, 3.0, LinearGradient::horizontal(rect, left, right));
        list.stroke_rounded_rect(rect, 3.0, Border::new(2.0, GREEN));
    };

    let mut turned = DisplayList::new();
    turned.push_transform(about_center(Transform::rotate(180.0), rect));
    draw(&mut turned, RED, BLUE);
    turned.pop_transform();
    let mut reversed = DisplayList::new();
    draw(&mut reversed, BLUE, RED);

    let (Some(turned), Some(reversed)) = (
        render(size, Color::BLACK, &turned),
        render(size, Color::BLACK, &reversed),
    ) else {
        return;
    };
    for y in 0..size.height {
        for x in 0..size.width {
            let (a, b) = (turned.at(x, y), reversed.at(x, y));
            for channel in 0..4 {
                assert!(
                    a[channel].abs_diff(b[channel]) <= 1,
                    "pixel {x},{y}: {a:?} against {b:?}"
                );
            }
        }
    }
}

#[test]
fn a_turned_rounded_bordered_rect_is_the_same_shape_turned() {
    // Every pixel is compared with the shape evaluated at the pixel's
    // centre turned back: fill, border ring, both anti-aliased edges and
    // the rounded corners.
    let rect = Rect::new(14.0, 22.0, 36.0, 20.0);
    let (radius, width, degrees) = (6.0, 3.0, 30.0f32);
    let mut list = DisplayList::new();
    list.push_transform(about_center(Transform::rotate(degrees), rect));
    list.fill_bordered_rect(rect, radius, RED, Border::new(width, GREEN));
    list.pop_transform();
    let Some(frame) = render(SurfaceSize::new(64, 64), BLUE, &list) else {
        return;
    };

    let center = rect.center();
    let (sin, cos) = degrees.to_radians().sin_cos();
    let (mut ring, mut fill) = (0, 0);
    for y in 0..64 {
        for x in 0..64 {
            let (dx, dy) = (x as f32 + 0.5 - center.x, y as f32 + 0.5 - center.y);
            // The inverse rotation.
            let local = (dx * cos + dy * sin, -dx * sin + dy * cos);
            let distance =
                rounded_box_distance(local, (rect.width / 2.0, rect.height / 2.0), radius);
            let outer = (0.5 - distance).clamp(0.0, 1.0);
            let inner = (0.5 - (distance + width)).clamp(0.0, 1.0);
            let expected = [inner, outer - inner, 1.0 - outer, 1.0];
            assert_close(frame.at(x, y), expected, 2.0, &format!("pixel {x},{y}"));
            ring += u32::from(outer - inner > 0.99);
            fill += u32::from(inner > 0.99);
        }
    }
    // The comparison saw all three regions.
    assert!(ring > 100 && fill > 200, "ring {ring}, fill {fill}");
}

/// A 4 x 4 atlas whose texels all differ: texel (x, y) is `60 + 12 * (4y + x)`.
fn patterned_atlas() -> [u8; 16] {
    std::array::from_fn(|index| 60 + 12 * index as u8)
}

#[test]
fn a_glyph_quad_turns_with_its_texels() {
    let texels = patterned_atlas();
    let atlas = AtlasView::new(4, 4, &texels, 1);
    let whole = Rect::new(0.0, 0.0, 4.0, 4.0);
    let rect = Rect::new(6.0, 6.0, 4.0, 4.0);
    let size = SurfaceSize::new(16, 16);
    let store = ImageStore::new();
    let render_turned = |degrees: f32| {
        let mut list = DisplayList::new();
        list.push_transform(about_center(Transform::rotate(degrees), rect));
        list.draw_glyph(rect, whole, Color::WHITE);
        list.pop_transform();
        render_with(size, Color::BLACK, &list, atlas, &store)
    };
    let texel = |x: u32, y: u32| texels[(y * 4 + x) as usize];

    // A quarter turn clockwise: the atlas's left column becomes the top
    // row, read right to left.
    let Some(quarter) = render_turned(90.0) else {
        return;
    };
    let half = render_turned(180.0).unwrap();
    for y in 0..4 {
        for x in 0..4 {
            let got = quarter.at(6 + x, 6 + y)[0];
            assert!(
                got.abs_diff(texel(y, 3 - x)) <= 1,
                "quarter turn at {x},{y}: {got}"
            );
            let got = half.at(6 + x, 6 + y)[0];
            assert!(
                got.abs_diff(texel(3 - x, 3 - y)) <= 1,
                "half turn at {x},{y}: {got}"
            );
        }
    }
    // Nothing spills outside the quad.
    for frame in [&quarter, &half] {
        assert_eq!(frame.at(5, 8), [0, 0, 0, 255]);
        assert_eq!(frame.at(10, 8), [0, 0, 0, 255]);
        assert_eq!(frame.at(8, 5), [0, 0, 0, 255]);
        assert_eq!(frame.at(8, 10), [0, 0, 0, 255]);
    }
}

#[test]
fn a_turned_glyph_quad_has_an_anti_aliased_edge_and_keeps_its_area() {
    // A solid 20 px glyph box (the worst case: coverage is full right up
    // to the edge of the quad) turned 30 degrees.
    let rect = Rect::new(22.0, 22.0, 20.0, 20.0);
    let mut list = DisplayList::new();
    list.push_transform(about_center(Transform::rotate(30.0), rect));
    list.draw_glyph(rect, SOLID_GLYPH, RED);
    list.pop_transform();
    let Some(frame) = render(SurfaceSize::new(64, 64), Color::BLACK, &list) else {
        return;
    };
    assert_eq!(frame.at(32, 32), R);
    assert_eq!(frame.at(4, 4), [0, 0, 0, 255]);
    let area = frame.total(0);
    assert!((area - 400.0).abs() < 4.0, "red area was {area}");
    // Along the slanted edges there are partly covered pixels, of many
    // different values: a hard, stair-stepped edge would have none.
    let mut partial: Vec<u8> = frame
        .pixels
        .chunks_exact(4)
        .map(|pixel| pixel[0])
        .filter(|red| (8..248).contains(red))
        .collect();
    partial.sort_unstable();
    partial.dedup();
    assert!(partial.len() > 12, "only {} edge values", partial.len());
}

#[test]
fn an_image_turns_with_its_pixels() {
    // A 3 x 2 image with six different opaque colours, drawn one texel per
    // pixel and turned a quarter: 2 x 3 on screen.
    let colors: [[u8; 4]; 6] = [
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [255, 255, 0, 255],
        [0, 255, 255, 255],
        [255, 0, 255, 255],
    ];
    let mut store = ImageStore::new();
    let image = store.add_rgba8(3, 2, colors.as_flattened()).unwrap();
    // Centre (9.5, 9): the turned image lands on whole pixels, 2 wide and
    // 3 tall at (8.5 - ..): choose a centre that keeps it on the grid.
    let rect = Rect::new(8.0, 8.0, 3.0, 2.0);
    let turn = Transform::rotate(90.0).then(Transform::translate(20.0, 0.0));
    let mut list = DisplayList::new();
    list.push_transform(turn);
    list.draw_image(image, rect);
    list.pop_transform();
    let Some(frame) = render_with(
        SurfaceSize::new(24, 24),
        Color::BLACK,
        &list,
        SOLID_ATLAS,
        &store,
    ) else {
        return;
    };
    // (x, y) goes to (20 - y, x): image pixel (i, j) at (8 + i, 8 + j)
    // lands on the screen pixel (11 - j, 8 + i).
    for j in 0..2u32 {
        for i in 0..3u32 {
            assert_eq!(
                frame.at(11 - j, 8 + i),
                colors[(j * 3 + i) as usize],
                "image pixel {i},{j}"
            );
        }
    }
    assert_eq!(frame.at(9, 9), [0, 0, 0, 255]);
    assert_eq!(frame.at(12, 9), [0, 0, 0, 255]);
}

/// One of everything the renderer draws.
fn every_kind(list: &mut DisplayList, image: morningstar_core::Image) {
    let rect = |x: f32, y: f32| Rect::new(x, y, 21.0, 13.0);
    list.draw_shadow(
        rect(4.5, 4.0),
        4.0,
        Shadow::new(1.0, 2.0, 5.0, Color::BLACK),
    );
    list.fill_rounded_rect(rect(4.5, 4.0), 4.0, RED);
    list.fill_bordered_rect(
        rect(30.0, 4.25),
        5.0,
        Color::rgba(0.0, 0.0, 1.0, 0.5),
        Border::new(1.5, GREEN),
    );
    list.fill_gradient_rect(
        rect(4.0, 24.0),
        2.0,
        LinearGradient::angled(rect(4.0, 24.0), 30.0, RED, BLUE),
    );
    list.fill_dashed_rect(
        rect(30.0, 24.0),
        6.0,
        Color::WHITE,
        Border::new(2.0, BLUE),
        Dash::new(4.0, 3.0),
    );
    list.push_rounded_clip(Rect::new(2.0, 40.0, 40.0, 18.0), 5.0);
    list.push_clip(Rect::new(6.5, 41.0, 60.0, 30.0));
    list.draw_glyph(Rect::new(3.0, 42.0, 9.0, 11.0), SOLID_GLYPH, GREEN);
    list.draw_image_region(
        image,
        Rect::new(0.0, 0.0, 2.0, 2.0),
        Rect::new(14.0, 42.5, 17.0, 12.0),
        3.0,
        Color::rgba(1.0, 1.0, 1.0, 0.8),
    );
    list.pop_clip();
    list.pop_clip();
}

fn checker_image(store: &mut ImageStore) -> morningstar_core::Image {
    let pixels: [[u8; 4]; 4] = [
        [255, 0, 0, 255],
        [0, 255, 0, 128],
        [0, 0, 255, 255],
        [255, 255, 255, 255],
    ];
    store.add_rgba8(2, 2, pixels.as_flattened()).unwrap()
}

#[test]
fn the_identity_transform_changes_no_byte() {
    let mut store = ImageStore::new();
    let image = checker_image(&mut store);
    let size = SurfaceSize::new(56, 60);
    let clear = Color::rgb(0.2, 0.3, 0.4);

    let mut plain = DisplayList::new();
    every_kind(&mut plain, image);

    let mut wrapped = DisplayList::new();
    wrapped.push_transform(Transform::IDENTITY);
    wrapped.push_transform(Transform::rotate(0.0));
    every_kind(&mut wrapped, image);
    wrapped.pop_transform();
    wrapped.pop_transform();

    let Some(plain) = render_with(size, clear, &plain, SOLID_ATLAS, &store) else {
        return;
    };
    let wrapped = render_with(size, clear, &wrapped, SOLID_ATLAS, &store).unwrap();
    assert!(
        plain.pixels == wrapped.pixels,
        "identity changed the output"
    );
    // And the scene is not blank.
    assert!(plain.total(0) > 100.0 && plain.total(1) > 100.0);
}

#[test]
fn a_whole_pixel_move_equals_drawing_it_there() {
    // Covers every kind, and a clip pushed under a transform that only
    // moves: it is a screen clip, moved.
    let mut store = ImageStore::new();
    let image = checker_image(&mut store);
    let size = SurfaceSize::new(64, 70);
    let clear = Color::rgb(0.2, 0.3, 0.4);

    // Moved by the transform, in two nested steps.
    let mut moved = DisplayList::new();
    moved.push_transform(Transform::translate(5.0, 2.0));
    moved.push_transform(Transform::translate(2.0, 5.0));
    every_kind(&mut moved, image);
    moved.pop_transform();
    moved.pop_transform();

    // The same scene with a 7 px margin rendered directly: compare the
    // moved render against the plain one shifted.
    let mut plain = DisplayList::new();
    every_kind(&mut plain, image);

    let Some(moved) = render_with(size, clear, &moved, SOLID_ATLAS, &store) else {
        return;
    };
    let plain = render_with(size, clear, &plain, SOLID_ATLAS, &store).unwrap();
    // Equal to within the rounding of a value interpolated at another
    // place on the target (the gradient's middle lands on 127.5).
    for y in 7..size.height {
        for x in 7..size.width {
            let (a, b) = (moved.at(x, y), plain.at(x - 7, y - 7));
            for channel in 0..4 {
                assert!(
                    a[channel].abs_diff(b[channel]) <= 1,
                    "pixel {x},{y}: {a:?} against {b:?}"
                );
            }
        }
    }
}

#[test]
fn nested_transforms_compose_innermost_first() {
    let rect = Rect::new(0.0, 0.0, 12.0, 6.0);
    let size = SurfaceSize::new(40, 40);
    // Inner: a quarter turn about the origin (the rect then spans x -6..0,
    // y 0..12). Outer: a move to (20, 10).
    let mut nested = DisplayList::new();
    nested.push_transform(Transform::translate(20.0, 10.0));
    nested.push_transform(Transform::rotate(90.0));
    nested.fill_rect(rect, RED);
    nested.pop_transform();
    // After the inner pop only the outer applies.
    nested.fill_rect(Rect::new(0.0, 20.0, 4.0, 4.0), GREEN);
    nested.pop_transform();
    // And after the outer pop, none.
    nested.fill_rect(Rect::new(0.0, 0.0, 2.0, 2.0), GREEN);

    let Some(frame) = render(size, BLUE, &nested) else {
        return;
    };
    for y in 0..40 {
        for x in 0..40 {
            let expected = if (14..20).contains(&x) && (10..22).contains(&y) {
                R
            } else if ((20..24).contains(&x) && (30..34).contains(&y)) || (x < 2 && y < 2) {
                [0, 255, 0, 255]
            } else {
                B
            };
            assert_eq!(frame.at(x, y), expected, "pixel {x},{y}");
        }
    }
}

#[test]
fn scaling_keeps_edges_one_pixel_sharp_and_scales_borders() {
    // A 10 px square with a 1 px border, doubled about the origin: 20 px
    // with a 2 px border, every edge still on the pixel grid.
    let mut list = DisplayList::new();
    list.push_transform(Transform::scale(2.0, 2.0));
    list.fill_bordered_rect(
        Rect::new(2.0, 2.0, 10.0, 10.0),
        0.0,
        RED,
        Border::new(1.0, GREEN),
    );
    list.pop_transform();
    let Some(frame) = render(SurfaceSize::new(32, 32), BLUE, &list) else {
        return;
    };
    for y in 0..32u32 {
        for x in 0..32u32 {
            let outer = (4..24).contains(&x) && (4..24).contains(&y);
            let inner = (6..22).contains(&x) && (6..22).contains(&y);
            let expected = match (outer, inner) {
                (true, true) => R,
                (true, false) => [0, 255, 0, 255],
                _ => B,
            };
            assert_eq!(frame.at(x, y), expected, "pixel {x},{y}");
        }
    }
}

#[test]
fn a_shear_slants_rows_by_their_distance_from_the_origin() {
    // The synthetic-italic transform about a baseline at y = 18: a row
    // `d` above the baseline moves right by a quarter of `d`.
    let rect = Rect::new(10.0, 10.0, 10.0, 8.0);
    let mut list = DisplayList::new();
    list.push_transform(Transform::shear_x(-0.25).about(Point::new(0.0, 18.0)));
    list.fill_rect(rect, RED);
    list.pop_transform();
    let Some(frame) = render(SurfaceSize::new(32, 28), BLUE, &list) else {
        return;
    };
    // Top row (centre 7.5 above the baseline): moved right by 1.875, so it
    // spans 11.875 to 21.875: an eighth of pixel 11, most of pixel 21.
    assert_eq!(frame.at(10, 10), B);
    assert!(frame.at(11, 10)[0].abs_diff(32) <= 2);
    assert_eq!(frame.at(12, 10), R);
    assert_eq!(frame.at(20, 10), R);
    assert!(frame.at(21, 10)[0].abs_diff(223) <= 2);
    assert_eq!(frame.at(22, 10), B);
    // Bottom row (0.5 above): moved by an eighth of a pixel.
    assert_eq!(frame.at(11, 17), R);
    assert_eq!(frame.at(19, 17), R);
    assert_eq!(frame.at(21, 17), B);
    // Nothing above or below.
    assert_eq!(frame.at(15, 8), B);
    assert_eq!(frame.at(15, 19), B);
    // A shear keeps the area: 80 px.
    let area = frame.total(0);
    assert!((area - 80.0).abs() < 2.0, "red area was {area}");
}

#[test]
fn a_clip_outside_a_transform_stays_where_it_is() {
    // A large square turned 45 degrees inside a small square clip: the
    // clip cuts it on the pixel grid, unturned.
    let big = Rect::new(2.0, 2.0, 60.0, 60.0);
    let mut list = DisplayList::new();
    list.push_clip(Rect::new(20.0, 24.0, 24.0, 12.0));
    list.push_transform(about_center(Transform::rotate(45.0), big));
    list.fill_rect(big, RED);
    list.pop_transform();
    list.pop_clip();
    let Some(frame) = render(SurfaceSize::new(64, 64), BLUE, &list) else {
        return;
    };
    for y in 0..64 {
        for x in 0..64 {
            let inside = (20..44).contains(&x) && (24..36).contains(&y);
            assert_eq!(frame.at(x, y), if inside { R } else { B }, "pixel {x},{y}");
        }
    }
}

#[test]
fn a_rounded_clip_outside_a_transform_keeps_its_corners() {
    let big = Rect::new(0.0, 0.0, 48.0, 48.0);
    let clip = Rect::new(8.0, 8.0, 32.0, 32.0);
    let mut turned = DisplayList::new();
    turned.push_rounded_clip(clip, 10.0);
    turned.push_transform(about_center(Transform::rotate(20.0), big));
    turned.fill_rect(Rect::new(-40.0, -40.0, 130.0, 130.0), RED);
    turned.pop_transform();
    turned.pop_clip();
    // What shows is exactly the clip's shape.
    let mut shape = DisplayList::new();
    shape.fill_rounded_rect(clip, 10.0, RED);

    let size = SurfaceSize::new(48, 48);
    let (Some(turned), Some(shape)) = (render(size, BLUE, &turned), render(size, BLUE, &shape))
    else {
        return;
    };
    assert!(turned.pixels == shape.pixels);
}

#[test]
fn a_clip_inside_a_transform_turns_with_it() {
    // Under a quarter turn about (25, 25), the local clip 30 x 10 at
    // (10, 20) is 10 x 30 at (20, 10) on screen. The content is far larger.
    let clip = Rect::new(10.0, 20.0, 30.0, 10.0);
    let mut list = DisplayList::new();
    list.push_transform(about_center(Transform::rotate(90.0), clip));
    list.push_clip(clip);
    list.fill_rect(Rect::new(-100.0, -100.0, 300.0, 300.0), RED);
    // A second square clip inside the first intersects exactly: the left
    // 12 local pixels, which are the top 12 on screen.
    list.push_clip(Rect::new(0.0, 0.0, 22.0, 100.0));
    list.fill_rect(Rect::new(-100.0, -100.0, 300.0, 300.0), GREEN);
    list.pop_clip();
    list.pop_clip();
    list.pop_transform();
    let Some(frame) = render(SurfaceSize::new(50, 50), BLUE, &list) else {
        return;
    };
    for y in 0..50 {
        for x in 0..50 {
            let expected = if !((20..30).contains(&x) && (10..40).contains(&y)) {
                B
            } else if y < 22 {
                [0, 255, 0, 255]
            } else {
                R
            };
            assert_eq!(frame.at(x, y), expected, "pixel {x},{y}");
        }
    }
}

#[test]
fn a_rounded_clip_inside_a_transform_is_the_turned_rounded_shape() {
    // Content clipped to a rounded rectangle, all turned 25 degrees, is the
    // same picture as that rounded rectangle filled and turned.
    let card = Rect::new(12.0, 20.0, 40.0, 24.0);
    let turn = about_center(Transform::rotate(25.0), card);
    let size = SurfaceSize::new(64, 64);

    let mut clipped = DisplayList::new();
    clipped.push_transform(turn);
    clipped.push_rounded_clip(card, 8.0);
    clipped.fill_rect(Rect::new(-50.0, -50.0, 200.0, 200.0), RED);
    clipped.pop_clip();
    clipped.pop_transform();

    let mut filled = DisplayList::new();
    filled.push_transform(turn);
    filled.fill_rounded_rect(card, 8.0, RED);
    filled.pop_transform();

    let (Some(clipped), Some(filled)) = (render(size, BLUE, &clipped), render(size, BLUE, &filled))
    else {
        return;
    };
    for y in 0..64 {
        for x in 0..64 {
            let (a, b) = (clipped.at(x, y), filled.at(x, y));
            for channel in 0..4 {
                assert!(
                    a[channel].abs_diff(b[channel]) <= 1,
                    "pixel {x},{y}: {a:?} against {b:?}"
                );
            }
        }
    }
    assert!(filled.total(0) > 800.0);
}

#[test]
fn an_undrawable_transform_draws_nothing_until_it_is_popped() {
    let mut list = DisplayList::new();
    for bad in [
        Transform::scale(0.0, 1.0),
        Transform::translate(f32::NAN, 0.0),
        Transform::matrix(f32::INFINITY, 0.0, 0.0, 1.0, 0.0, 0.0),
    ] {
        list.push_transform(bad);
        list.fill_rect(Rect::new(0.0, 0.0, 16.0, 16.0), RED);
        list.pop_transform();
    }
    // A pop with nothing to match is ignored.
    list.pop_transform();
    list.fill_rect(Rect::new(0.0, 0.0, 2.0, 2.0), GREEN);
    let Some(frame) = render(SurfaceSize::new(16, 16), BLUE, &list) else {
        return;
    };
    assert_eq!(frame.at(0, 0), [0, 255, 0, 255]);
    assert_eq!(frame.at(8, 8), B);
    assert_eq!(frame.total(0), 0.0);
}

#[test]
fn transforms_do_not_split_the_draw_call() {
    let Some(mut renderer) = skip_without_gpu(OffscreenRenderer::new()) else {
        return;
    };
    let mut list = DisplayList::new();
    for index in 0..6 {
        let rect = Rect::new(4.0 + 8.0 * index as f32, 8.0, 6.0, 6.0);
        list.push_transform(about_center(Transform::rotate(13.0 * index as f32), rect));
        list.fill_rect(rect, RED);
        list.draw_glyph(rect, SOLID_GLYPH, GREEN);
        list.pop_transform();
    }
    // Something turned wholly out of the clip costs no instance.
    list.push_clip(Rect::new(0.0, 0.0, 64.0, 24.0));
    list.push_transform(Transform::translate(500.0, 0.0).and_rotate(45.0));
    list.fill_rect(Rect::new(0.0, 0.0, 8.0, 8.0), RED);
    list.pop_transform();
    list.pop_clip();
    renderer
        .render(SurfaceSize::new(64, 24), BLUE, &list, SOLID_ATLAS)
        .unwrap();
    let stats = renderer.last_stats();
    assert_eq!(stats.draw_calls, 1);
    assert_eq!(stats.instances, 12);
}
