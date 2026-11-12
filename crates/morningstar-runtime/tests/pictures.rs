//! Vector pictures and outlined text through the application-facing API
//! (ADR 0017): from layers to pixels on the real GPU, and the cache that
//! keeps their rasterisations.
//!
//! The offscreen target is linear RGBA8, so expected bytes are the blended
//! linear value times 255.

use morningstar_core::{Color, Point, Rect, SurfaceSize, Transform};
use morningstar_gpu::{GpuError, render_offscreen_with_images};
use morningstar_runtime::{Frame, Layer, LineCap, TextSpan, TextStyle, VectorPicture};

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const GREEN: Color = Color::rgb(0.0, 1.0, 0.0);
const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);

const R: [u8; 4] = [255, 0, 0, 255];
const G: [u8; 4] = [0, 255, 0, 255];
const B: [u8; 4] = [0, 0, 255, 255];
const K: [u8; 4] = [0, 0, 0, 255];

struct Rendered {
    width: u32,
    pixels: Vec<u8>,
}

impl Rendered {
    fn at(&self, x: u32, y: u32) -> [u8; 4] {
        let start = ((y * self.width + x) * 4) as usize;
        self.pixels[start..start + 4].try_into().unwrap()
    }
}

fn render(size: SurfaceSize, frame: &Frame) -> Option<Rendered> {
    match render_offscreen_with_images(
        size,
        frame.clear,
        &frame.list,
        frame.atlas(),
        frame.images(),
    ) {
        Ok(pixels) => Some(Rendered {
            width: size.width,
            pixels,
        }),
        Err(GpuError::NoAdapter(reason)) => {
            eprintln!("skipping: no GPU adapter available ({reason})");
            None
        }
        Err(other) => panic!("offscreen render failed: {other}"),
    }
}

/// A frame with `picture` drawn into `rect` over black.
fn drawn(picture: &VectorPicture, rect: Rect) -> Frame {
    let mut frame = Frame::new();
    frame.begin();
    frame.draw_picture(picture, rect, 0.0, Color::WHITE);
    frame
}

const SIZE: SurfaceSize = SurfaceSize::new(32, 32);
/// The view box is 16 units; drawn at 32 px, one unit is two pixels.
const WHOLE: Rect = Rect::new(0.0, 0.0, 32.0, 32.0);

#[test]
fn fills_strokes_and_paint_order_reach_the_screen() {
    static LAYERS: [Layer; 3] = [
        // Left half red.
        Layer::rect(0.0, 0.0, 8.0, 16.0).fill(RED),
        // A blue square over the middle: covers red and nothing alike.
        Layer::rect(4.0, 4.0, 8.0, 8.0).fill(BLUE),
        // A green line across everything, 2 units thick.
        Layer::line(0.0, 8.0, 16.0, 8.0).stroke(GREEN, 2.0),
    ];
    let frame = drawn(&VectorPicture::new(16.0, 16.0, &LAYERS), WHOLE);
    let Some(image) = render(SIZE, &frame) else {
        return;
    };
    for y in 0..32u32 {
        for x in 0..32u32 {
            let expected = if (14..18).contains(&y) {
                G
            } else if (8..24).contains(&x) && (8..24).contains(&y) {
                B
            } else if x < 16 {
                R
            } else {
                K
            };
            assert_eq!(image.at(x, y), expected, "pixel {x},{y}");
        }
    }
}

#[test]
fn opacity_and_translucent_paints_blend_in_linear_light() {
    static LAYERS: [Layer; 3] = [
        Layer::rect(0.0, 0.0, 16.0, 8.0).fill(RED),
        // Half-opaque blue over red (top) and over nothing (bottom).
        Layer::rect(0.0, 0.0, 8.0, 16.0).fill(BLUE).opacity(0.5),
        // `stroke-opacity`: a colour with alpha.
        Layer::line(12.0, 0.0, 12.0, 16.0).stroke(Color::rgba(0.0, 1.0, 0.0, 0.25), 4.0),
    ];
    let frame = drawn(&VectorPicture::new(16.0, 16.0, &LAYERS), WHOLE);
    let Some(image) = render(SIZE, &frame) else {
        return;
    };
    let close = |actual: [u8; 4], expected: [u8; 4], what: &str| {
        for channel in 0..4 {
            assert!(
                actual[channel].abs_diff(expected[channel]) <= 2,
                "{what}: {actual:?} against {expected:?}"
            );
        }
    };
    // The picture is stored sRGB-encoded and comes back linear: half blue
    // over red is half of each.
    close(image.at(4, 4), [128, 0, 128, 255], "blue over red");
    // Over the black background: half blue.
    close(image.at(4, 24), [0, 0, 128, 255], "blue over nothing");
    // A quarter green over red, and over black.
    close(image.at(24, 4), [191, 64, 0, 255], "green over red");
    close(image.at(24, 24), [0, 64, 0, 255], "green over nothing");
    close(image.at(18, 24), K, "untouched");
}

#[test]
fn a_clipped_and_turned_layer_is_cut_by_the_unturned_clip() {
    // The vinyl-disc band of the design in miniature: a wide bar turned
    // 45 degrees about the centre, cut to a circle.
    static LAYERS: [Layer; 1] = [Layer::rect(-8.0, 6.0, 32.0, 4.0)
        .fill(RED)
        .rotate(45.0, 8.0, 8.0)
        .clip_circle(8.0, 8.0, 6.0)];
    let frame = drawn(&VectorPicture::new(16.0, 16.0, &LAYERS), WHOLE);
    let Some(image) = render(SIZE, &frame) else {
        return;
    };
    // Along the falling diagonal the band shows, inside the circle only
    // (radius 12 px about (16, 16)).
    assert_eq!(image.at(16, 16), R);
    assert_eq!(image.at(10, 10), R);
    assert_eq!(image.at(22, 22), R);
    assert_eq!(image.at(4, 4), K, "beyond the circle");
    assert_eq!(image.at(28, 28), K, "beyond the circle");
    // Off the diagonal there is no band: it is 8 px thick.
    assert_eq!(image.at(22, 10), K);
    assert_eq!(image.at(10, 22), K);
    // Unturned it would have covered this pixel.
    assert_eq!(image.at(6, 16), K);
}

#[test]
fn round_caps_and_layer_transforms_are_honoured() {
    static LAYERS: [Layer; 1] = [Layer::path("M0 0h6")
        .stroke(GREEN, 2.0)
        .cap(LineCap::Round)
        .translate(5.0, 8.0)];
    let frame = drawn(&VectorPicture::new(16.0, 16.0, &LAYERS), WHOLE);
    let Some(image) = render(SIZE, &frame) else {
        return;
    };
    // The line runs from x = 5 to 11 units (10 to 22 px) at y = 8 (16 px),
    // 4 px thick, with a 2 px round cap beyond each end.
    assert_eq!(image.at(16, 16), G);
    assert_eq!(image.at(16, 14), G);
    assert_eq!(image.at(16, 13), K);
    assert_eq!(image.at(9, 16), G, "inside the cap");
    assert_eq!(image.at(7, 16), K, "beyond the cap");
    let corner = image.at(8, 14)[1];
    assert!((1..255).contains(&corner), "the cap is round: {corner}");
}

#[test]
fn a_picture_is_sharp_at_any_size_and_placed_on_whole_pixels() {
    static LAYERS: [Layer; 1] = [Layer::rect(1.0, 1.0, 2.0, 2.0).fill(RED)];
    let picture = VectorPicture::new(4.0, 4.0, &LAYERS);
    // At 6 px per unit, at a fractional position: snapped to (3, 5).
    let frame = drawn(&picture, Rect::new(3.2, 4.6, 24.0, 24.0));
    let Some(image) = render(SIZE, &frame) else {
        return;
    };
    for y in 0..32u32 {
        for x in 0..32u32 {
            let inside = (9..21).contains(&x) && (11..23).contains(&y);
            assert_eq!(image.at(x, y), if inside { R } else { K }, "pixel {x},{y}");
        }
    }
}

#[test]
fn a_picture_turns_under_a_transform_and_takes_a_tint() {
    static LAYERS: [Layer; 2] = [
        Layer::rect(0.0, 0.0, 8.0, 4.0).fill(Color::WHITE),
        Layer::rect(0.0, 0.0, 2.0, 4.0).fill(RED),
    ];
    let picture = VectorPicture::new(8.0, 4.0, &LAYERS);
    let rect = Rect::new(8.0, 12.0, 16.0, 8.0);
    let mut frame = Frame::new();
    frame.begin();
    frame
        .list
        .push_transform(Transform::rotate(90.0).about(rect.center()));
    frame.draw_picture(&picture, rect, 0.0, Color::rgb(0.0, 0.0, 1.0));
    frame.list.pop_transform();
    let Some(image) = render(SIZE, &frame) else {
        return;
    };
    // 16 x 8 turned a quarter about (16, 16): 8 x 16 at (12, 8). The left
    // end (red) is now the top; the tint keeps only blue, so red becomes
    // black and white becomes blue.
    assert_eq!(image.at(16, 9), K, "red, tinted blue, is black");
    assert_eq!(image.at(16, 14), B);
    assert_eq!(image.at(16, 22), B);
    assert_eq!(image.at(10, 16), K);
    assert_eq!(image.at(21, 16), K);
    assert_eq!(image.at(16, 6), K);
}

#[test]
fn the_same_picture_at_the_same_size_is_rasterised_once() {
    static LAYERS: [Layer; 1] = [Layer::circle(8.0, 8.0, 6.0).fill(RED)];
    // Two values with the same content share one rasterisation.
    let first = VectorPicture::new(16.0, 16.0, &LAYERS);
    let second = VectorPicture::new(16.0, 16.0, &LAYERS);
    let mut frame = Frame::new();
    frame.begin();
    frame.draw_picture(&first, WHOLE, 0.0, Color::WHITE);
    frame.draw_picture(&second, Rect::new(40.0, 0.0, 32.0, 32.0), 0.0, Color::WHITE);
    assert_eq!(frame.picture_cache_len(), 1);
    assert_eq!(frame.images().len(), 1);
    assert_eq!(frame.picture_cache_bytes(), 32 * 32 * 4);
    assert_eq!(frame.list.len(), 2);

    // Another size is another rasterisation.
    frame.draw_picture(&first, Rect::new(0.0, 0.0, 16.0, 16.0), 0.0, Color::WHITE);
    assert_eq!(frame.picture_cache_len(), 2);
    assert_eq!(frame.picture_cache_bytes(), (32 * 32 + 16 * 16) * 4);

    // Nothing is drawn, or kept, for a rectangle without pixels.
    frame.draw_picture(&first, Rect::new(0.0, 0.0, 0.2, 16.0), 0.0, Color::WHITE);
    frame.draw_picture(
        &first,
        Rect::new(0.0, 0.0, f32::NAN, 16.0),
        0.0,
        Color::WHITE,
    );
    assert_eq!(frame.picture_cache_len(), 2);
    assert_eq!(frame.list.len(), 3);
}

#[test]
fn a_picture_no_longer_drawn_is_dropped_after_a_while() {
    static LAYERS: [Layer; 1] = [Layer::circle(8.0, 8.0, 6.0).fill(RED)];
    static OTHER: [Layer; 1] = [Layer::circle(8.0, 8.0, 5.0).fill(BLUE)];
    let picture = VectorPicture::new(16.0, 16.0, &LAYERS);
    let other = VectorPicture::new(16.0, 16.0, &OTHER);
    let mut frame = Frame::new();
    frame.begin();
    frame.draw_picture(&picture, WHOLE, 0.0, Color::WHITE);
    frame.draw_picture(&other, WHOLE, 0.0, Color::WHITE);
    assert_eq!(frame.picture_cache_len(), 2);

    // Kept through frames that do not draw it (scrolled out of view), and
    // drawn again from the cache: the image is the same one.
    for _ in 0..100 {
        frame.begin();
        frame.draw_picture(&other, WHOLE, 0.0, Color::WHITE);
    }
    assert_eq!(frame.picture_cache_len(), 2);
    let before = frame.images().generation();
    frame.draw_picture(&picture, WHOLE, 0.0, Color::WHITE);
    assert_eq!(frame.images().generation(), before, "no new image");

    // Left undrawn for long enough, it and its image go.
    for _ in 0..200 {
        frame.begin();
        frame.draw_picture(&other, WHOLE, 0.0, Color::WHITE);
    }
    assert_eq!(frame.picture_cache_len(), 1);
    assert_eq!(frame.images().len(), 1);
    assert_eq!(frame.picture_cache_bytes(), 32 * 32 * 4);
}

#[test]
fn over_budget_the_cache_drops_what_the_last_frame_did_not_draw() {
    static LAYERS: [Layer; 1] = [Layer::rect(0.0, 0.0, 16.0, 16.0).fill(RED)];
    let picture = VectorPicture::new(16.0, 16.0, &LAYERS);
    let mut frame = Frame::new();
    // Each size is its own 16 MiB rasterisation; three exceed the budget.
    for side in [2048.0, 2047.0, 2046.0] {
        frame.begin();
        frame.draw_picture(&picture, Rect::new(0.0, 0.0, side, side), 0.0, Color::WHITE);
    }
    // What a frame draws is never dropped under it, so the cache is over
    // budget until the next frame starts...
    assert_eq!(frame.picture_cache_len(), 3);
    // ...which drops the least recently drawn of the others.
    frame.begin();
    assert_eq!(frame.picture_cache_len(), 2);
    frame.draw_picture(&picture, Rect::new(0.0, 0.0, 8.0, 8.0), 0.0, Color::WHITE);
    frame.begin();
    assert!(
        frame.picture_cache_bytes() <= 32 * 1024 * 1024,
        "{} bytes",
        frame.picture_cache_bytes()
    );
    assert_eq!(frame.images().len(), frame.picture_cache_len());
}

#[test]
fn an_oversized_picture_is_rasterised_at_the_limit_and_stretched() {
    static LAYERS: [Layer; 1] = [Layer::rect(0.0, 0.0, 16.0, 8.0).fill(RED)];
    let picture = VectorPicture::new(16.0, 8.0, &LAYERS);
    let mut frame = Frame::new();
    frame.begin();
    frame.draw_picture(
        &picture,
        Rect::new(0.0, 0.0, 4096.0, 2048.0),
        0.0,
        Color::WHITE,
    );
    assert_eq!(frame.picture_cache_bytes(), 2048 * 1024 * 4);
    assert!(matches!(
        frame.list.ops()[0],
        morningstar_runtime::DrawOp::Image { rect, .. } if rect.width == 4096.0
    ));
}

const FONT: &[u8] = include_bytes!("../../../examples/hello-gpu/assets/fonts/NotoSans-Bold.ttf");

struct Fonts;

impl morningstar_runtime::App for Fonts {
    fn setup(&mut self, setup: &mut morningstar_runtime::Setup<'_>) {
        setup.add_font(FONT);
    }

    fn frame(&mut self, _: &morningstar_runtime::FrameInfo, _: &mut Frame) {}
}

#[test]
fn outlined_text_is_drawn_where_the_text_would_be_with_a_stroke_around_it() {
    let mut frame = Frame::for_app(&mut Fonts);
    frame.begin();
    let style = TextStyle::new(64.0, RED).line_height(1.0);
    let spans = [TextSpan::new("I")];
    let origin = Point::new(24.0, 8.0);
    let size = frame.draw_text_outlined(&spans, &style, origin, None, 4.0, Color::WHITE);
    assert_eq!(size, frame.measure_text("I", &style, None));
    assert_eq!(frame.picture_cache_len(), 1);
    // The same text again reuses the image.
    frame.draw_text_outlined(&spans, &style, origin, None, 4.0, Color::WHITE);
    assert_eq!(frame.picture_cache_len(), 1);

    let Some(image) = render(SurfaceSize::new(80, 80), &frame) else {
        return;
    };
    // Across the middle of the stem: black, white, red, white, black.
    let row: Vec<[u8; 4]> = (0..80).map(|x| image.at(x, 40)).collect();
    let first_white = row.iter().position(|p| *p == [255; 4]).expect("stroke");
    let first_red = row.iter().position(|p| *p == R).expect("fill");
    let last_white = row.iter().rposition(|p| *p == [255; 4]).unwrap();
    assert!(first_white < first_red && first_red < last_white, "{row:?}");
    assert_eq!(row[first_white - 2], K);
    assert_eq!(row[last_white + 2], K);
    let white = row.iter().filter(|p| **p == [255; 4]).count();
    assert!((6..=8).contains(&white), "{white} stroke pixels");
    // It starts where the plain text's stem starts, less half the stroke.
    let mut plain = Frame::for_app(&mut Fonts);
    plain.begin();
    plain.draw_text("I", &style, origin, None);
    let plain = render(SurfaceSize::new(80, 80), &plain).unwrap();
    let stem = (0..80).position(|x| plain.at(x, 40)[0] > 200).unwrap();
    assert!(
        (stem as i32 - 2 - first_white as i32).abs() <= 1,
        "stem at {stem}, stroke from {first_white}"
    );
}
