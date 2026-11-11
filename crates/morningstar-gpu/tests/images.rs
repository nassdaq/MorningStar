//! Pixel tests for images (ADR 0014): registration and release as the
//! renderer sees them, sampling, colour, rounding, tint, clipping, paint
//! order across draw calls, and mipmaps.
//!
//! Like the other pixel tests these run on the real GPU of the machine and
//! skip where there is none. The target is linear RGBA8, so expected bytes
//! are the blended linear value times 255, while image pixels are given
//! sRGB-encoded, as in a PNG. Bytes 0 and 255 are the same in both.

use morningstar_core::{
    AtlasView, Color, DisplayList, Image, ImageFit, ImageStore, ImagesView, MAX_IMAGE_DIMENSION,
    Rect, SurfaceSize,
};
use morningstar_gpu::{
    GpuError, OffscreenRenderer, RenderStats, render_offscreen, render_offscreen_with_images,
};

const R: [u8; 4] = [255, 0, 0, 255];
const G: [u8; 4] = [0, 255, 0, 255];
const B: [u8; 4] = [0, 0, 255, 255];
const W: [u8; 4] = [255, 255, 255, 255];
const K: [u8; 4] = [0, 0, 0, 255];
const CLEAR: [u8; 4] = [0, 0, 0, 0];

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);
const NOTHING: Color = Color::rgba(0.0, 0.0, 0.0, 0.0);

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

fn render_on(
    size: SurfaceSize,
    clear: Color,
    list: &DisplayList,
    store: &ImageStore,
) -> Option<Frame> {
    let pixels = skip_without_gpu(render_offscreen_with_images(
        size,
        clear,
        list,
        SOLID_ATLAS,
        store.view(),
    ))?;
    assert_eq!(pixels.len(), (size.width * size.height * 4) as usize);
    Some(Frame {
        width: size.width,
        pixels,
    })
}

/// Renders over a transparent target.
fn render(size: SurfaceSize, list: &DisplayList, store: &ImageStore) -> Option<Frame> {
    render_on(size, NOTHING, list, store)
}

/// An image from rows of pixels.
fn image<const N: usize>(store: &mut ImageStore, rows: &[[[u8; 4]; N]]) -> Image {
    let pixels: Vec<u8> = rows.iter().flatten().flatten().copied().collect();
    store
        .add_rgba8(N as u32, rows.len() as u32, &pixels)
        .expect("valid image")
}

fn close(a: [u8; 4], b: [u8; 4], tolerance: u8) -> bool {
    a.iter().zip(b).all(|(&x, y)| x.abs_diff(y) <= tolerance)
}

/// What an sRGB-encoded byte is as a linear byte in the target.
fn linear(encoded: u8) -> u8 {
    let v = f32::from(encoded) / 255.0;
    let light = if v <= 0.040_45 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    };
    (light * 255.0).round() as u8
}

// --- Sampling ----------------------------------------------------------------

#[test]
fn unscaled_opaque_image_is_copied_exactly() {
    let mut store = ImageStore::new();
    let rows = [[R, G, B, W], [K, W, R, G], [B, K, W, R]];
    let picture = image(&mut store, &rows);
    let mut list = DisplayList::new();
    list.draw_image(picture, Rect::new(3.0, 2.0, 4.0, 3.0));

    let Some(frame) = render(SurfaceSize::new(10, 8), &list, &store) else {
        return;
    };
    for y in 0..8 {
        for x in 0..10 {
            let inside = (3..7).contains(&x) && (2..5).contains(&y);
            let expected = if inside {
                rows[(y - 2) as usize][(x - 3) as usize]
            } else {
                CLEAR
            };
            assert_eq!(frame.at(x, y), expected, "pixel ({x}, {y})");
        }
    }
}

#[test]
fn source_rect_crops_without_bleeding_from_neighbours() {
    let mut store = ImageStore::new();
    // A 2 x 2 sheet of 2 x 2 sprites.
    let sheet = image(
        &mut store,
        &[[R, R, G, G], [R, R, G, G], [B, B, W, W], [B, B, W, W]],
    );
    let mut list = DisplayList::new();
    // Each sprite magnified eight times: bilinear filtering would blend the
    // neighbouring sprite into the edges if sampling were not confined.
    let sprites = [(2.0, 0.0, G), (0.0, 2.0, B), (2.0, 2.0, W), (0.0, 0.0, R)];
    for (index, &(x, y, _)) in sprites.iter().enumerate() {
        list.draw_image_region(
            sheet,
            Rect::new(x, y, 2.0, 2.0),
            Rect::new(index as f32 * 16.0, 0.0, 16.0, 16.0),
            0.0,
            Color::WHITE,
        );
    }

    let Some(frame) = render(SurfaceSize::new(64, 16), &list, &store) else {
        return;
    };
    for (index, &(_, _, expected)) in sprites.iter().enumerate() {
        for y in 0..16 {
            for x in 0..16 {
                let x = index as u32 * 16 + x;
                assert_eq!(frame.at(x, y), expected, "pixel ({x}, {y})");
            }
        }
    }
}

#[test]
fn magnified_image_is_filtered_between_texels() {
    let mut store = ImageStore::new();
    let pair = image(&mut store, &[[K, W]]);
    let mut list = DisplayList::new();
    list.draw_image(pair, Rect::new(0.0, 0.0, 16.0, 1.0));
    let Some(frame) = render(SurfaceSize::new(16, 1), &list, &store) else {
        return;
    };
    // Flat up to each texel's centre, a linear ramp (in light) between.
    assert_eq!(frame.at(0, 0), K);
    assert_eq!(frame.at(3, 0), K);
    assert_eq!(frame.at(12, 0), W);
    assert_eq!(frame.at(15, 0), W);
    let mut previous = 0;
    for x in 4..12 {
        let value = frame.at(x, 0)[0];
        assert!(value > previous, "ramp rises at {x}: {value}");
        previous = value;
    }
    // Pixel 7's centre is 7/16 of the way from the black texel's centre to
    // the white one's; pixel 8's is 9/16.
    assert!(close(frame.at(7, 0), [112, 112, 112, 255], 1));
    assert!(close(frame.at(8, 0), [143, 143, 143, 255], 1));
}

#[test]
fn fit_helpers_place_the_image_on_the_gpu() {
    let mut store = ImageStore::new();
    let wide = image(&mut store, &[[R, G, G, B], [R, G, G, B]]);
    let bounds = Rect::new(0.0, 0.0, 8.0, 8.0);

    // Cover: the square shows the middle of the wide image only.
    let mut list = DisplayList::new();
    list.draw_image_fit(wide, bounds, ImageFit::Cover, 0.0, Color::WHITE);
    let Some(frame) = render(SurfaceSize::new(8, 8), &list, &store) else {
        return;
    };
    for (x, y) in [(0, 0), (7, 0), (0, 7), (7, 7), (4, 4)] {
        assert_eq!(frame.at(x, y), G, "pixel ({x}, {y})");
    }

    // Contain: the whole image, letterboxed above and below.
    list.clear();
    list.draw_image_fit(wide, bounds, ImageFit::Contain, 0.0, Color::WHITE);
    let frame = render(SurfaceSize::new(8, 8), &list, &store).unwrap();
    assert_eq!(frame.at(0, 1), CLEAR);
    assert_eq!(frame.at(0, 6), CLEAR);
    assert_eq!(frame.at(0, 2), R);
    assert_eq!(frame.at(0, 5), R);
    assert_eq!(frame.at(7, 3), B);
    assert_eq!(frame.at(3, 3), G);

    // Stretch: everything, distorted.
    list.clear();
    list.draw_image_fit(wide, bounds, ImageFit::Stretch, 0.0, Color::WHITE);
    let frame = render(SurfaceSize::new(8, 8), &list, &store).unwrap();
    assert_eq!(frame.at(0, 0), R);
    assert_eq!(frame.at(7, 7), B);
    assert_eq!(frame.at(4, 0), G);
}

// --- Colour ------------------------------------------------------------------

#[test]
fn image_bytes_are_decoded_from_srgb_to_linear() {
    let mut store = ImageStore::new();
    let grey = |v: u8| [v, v, v, 255];
    let values = [1, 10, 55, 128, 188, 240];
    let ramp = image(&mut store, &[values.map(grey)]);
    let mut list = DisplayList::new();
    list.draw_image(ramp, Rect::new(0.0, 0.0, 6.0, 1.0));
    let Some(frame) = render(SurfaceSize::new(6, 1), &list, &store) else {
        return;
    };
    for (x, value) in values.into_iter().enumerate() {
        let expected = linear(value);
        assert!(
            close(frame.at(x as u32, 0), grey(expected), 1),
            "sRGB {value} should be linear {expected}, got {:?}",
            frame.at(x as u32, 0)
        );
    }
    // The two that matter by eye: mid-grey 128 is 22% light, and half the
    // light is 188. Reading the bytes as linear would give 128 for both.
    assert_eq!(linear(128), 55);
    assert_eq!(linear(188), 128);
}

#[test]
fn translucent_pixels_are_premultiplied_in_linear_light() {
    let mut store = ImageStore::new();
    // Straight alpha, as in a PNG: full-intensity colour, half alpha.
    let half = image(&mut store, &[[[255, 255, 255, 128], [255, 0, 0, 128]]]);
    let mut list = DisplayList::new();
    list.draw_image(half, Rect::new(0.0, 0.0, 2.0, 1.0));

    // On nothing: premultiplied colour and coverage, both one half.
    let Some(frame) = render(SurfaceSize::new(2, 1), &list, &store) else {
        return;
    };
    assert!(close(frame.at(0, 0), [128, 128, 128, 128], 1));
    assert!(close(frame.at(1, 0), [128, 0, 0, 128], 1));

    // Over opaque blue: half of each.
    let frame = render_on(SurfaceSize::new(2, 1), BLUE, &list, &store).unwrap();
    assert!(close(frame.at(0, 0), [128, 128, 255, 255], 1));
    assert!(close(frame.at(1, 0), [128, 0, 127, 255], 1));
}

#[test]
fn transparent_edges_have_no_dark_or_coloured_fringe() {
    let mut store = ImageStore::new();
    // Opaque red beside fully transparent pixels that still carry a colour,
    // as exported images often do: green in one row, black in the other.
    let green_ghost = [0, 255, 0, 0];
    let black_ghost = [0, 0, 0, 0];
    let edge = image(
        &mut store,
        &[
            [R, R, green_ghost, green_ghost],
            [R, R, black_ghost, black_ghost],
        ],
    );
    let mut list = DisplayList::new();
    // Magnified, so the filter blends across the edge over many pixels.
    list.draw_image(edge, Rect::new(0.0, 0.0, 64.0, 32.0));

    // Over a red background the result must be red everywhere. Blending the
    // ghost colours in (straight-alpha filtering) would darken the edge or
    // tint it green.
    let Some(frame) = render_on(SurfaceSize::new(64, 32), RED, &list, &store) else {
        return;
    };
    for y in 0..32 {
        for x in 0..64 {
            assert!(
                close(frame.at(x, y), R, 1),
                "fringe at ({x}, {y}): {:?}",
                frame.at(x, y)
            );
        }
    }
}

#[test]
fn transparent_edges_have_no_light_fringe() {
    let mut store = ImageStore::new();
    // Opaque black beside transparent white, over black: any white leaking
    // through the filter would show as a light halo.
    let white_ghost = [255, 255, 255, 0];
    let edge = image(&mut store, &[[K, white_ghost], [white_ghost, K]]);
    let mut list = DisplayList::new();
    list.draw_image(edge, Rect::new(0.0, 0.0, 32.0, 32.0));
    let Some(frame) = render_on(SurfaceSize::new(32, 32), Color::BLACK, &list, &store) else {
        return;
    };
    for y in 0..32 {
        for x in 0..32 {
            assert_eq!(frame.at(x, y), K, "fringe at ({x}, {y})");
        }
    }
}

#[test]
fn tint_multiplies_colour_and_opacity() {
    let mut store = ImageStore::new();
    let white = image(&mut store, &[[W]]);
    let colours = image(&mut store, &[[[255, 255, 0, 255]]]);
    let mut list = DisplayList::new();
    let cell = |index: u32| Rect::new(index as f32 * 4.0, 0.0, 4.0, 4.0);
    // An icon drawn in white takes the tint's colour.
    list.draw_image_region(white, white.bounds(), cell(0), 0.0, RED);
    // White at half alpha is half opacity.
    list.draw_image_region(
        white,
        white.bounds(),
        cell(1),
        0.0,
        Color::rgba(1.0, 1.0, 1.0, 0.5),
    );
    // A tint filters the image's own colour: yellow through red is red.
    list.draw_image_region(colours, colours.bounds(), cell(2), 0.0, RED);
    // Colour and opacity together.
    list.draw_image_region(
        colours,
        colours.bounds(),
        cell(3),
        0.0,
        Color::rgba(0.0, 1.0, 0.0, 0.25),
    );

    let Some(frame) = render_on(SurfaceSize::new(16, 4), BLUE, &list, &store) else {
        return;
    };
    assert_eq!(frame.at(1, 1), R);
    assert!(close(frame.at(5, 1), [128, 128, 255, 255], 1));
    assert_eq!(frame.at(9, 1), R);
    assert!(close(frame.at(13, 1), [0, 64, 191, 255], 1));
}

// --- Shape and clipping ------------------------------------------------------

#[test]
fn rounded_image_covers_exactly_what_a_rounded_rect_does() {
    let mut store = ImageStore::new();
    let red = image(&mut store, &[[R, R], [R, R]]);
    let size = SurfaceSize::new(48, 40);
    let rect = Rect::new(4.5, 3.0, 39.0, 33.0);

    for radius in [0.0, 6.0, 1000.0] {
        let mut list = DisplayList::new();
        list.draw_image_region(red, red.bounds(), rect, radius, Color::WHITE);
        let Some(pictured) = render(size, &list, &store) else {
            return;
        };
        list.clear();
        list.fill_rounded_rect(rect, radius, RED);
        let filled = render(size, &list, &store).unwrap();

        let mut partial = 0;
        for y in 0..size.height {
            for x in 0..size.width {
                let (a, b) = (pictured.at(x, y), filled.at(x, y));
                assert!(
                    close(a, b, 1),
                    "radius {radius} at ({x}, {y}): {a:?} vs {b:?}"
                );
                partial += u32::from(a[3] > 0 && a[3] < 255);
            }
        }
        assert!(partial > 0, "the edge is anti-aliased");
    }
}

#[test]
fn circular_avatar_shows_the_image_inside_and_nothing_at_the_corners() {
    let mut store = ImageStore::new();
    let quarters = image(&mut store, &[[R, G], [B, W]]);
    let mut list = DisplayList::new();
    // Nearest-looking quarters: sample well inside each to avoid the blend.
    list.draw_image_fit(
        quarters,
        Rect::new(0.0, 0.0, 40.0, 40.0),
        ImageFit::Cover,
        20.0,
        Color::WHITE,
    );
    let Some(frame) = render(SurfaceSize::new(40, 40), &list, &store) else {
        return;
    };
    for corner in [(0, 0), (39, 0), (0, 39), (39, 39), (4, 4), (35, 35)] {
        assert_eq!(frame.at(corner.0, corner.1), CLEAR, "{corner:?}");
    }
    assert_eq!(frame.at(8, 8), R);
    assert_eq!(frame.at(31, 8), G);
    assert_eq!(frame.at(8, 31), B);
    assert_eq!(frame.at(31, 31), W);
    assert!(frame.at(20, 0)[3] > 250, "the circle touches the edge");
}

#[test]
fn images_are_clipped_like_everything_else() {
    let mut store = ImageStore::new();
    let red = image(&mut store, &[[R]]);
    let rect = Rect::new(0.0, 0.0, 16.0, 16.0);
    let mut list = DisplayList::new();
    list.push_clip(Rect::new(4.0, 2.0, 8.0, 6.5));
    list.draw_image(red, rect);
    list.pop_clip();

    let Some(frame) = render(SurfaceSize::new(16, 16), &list, &store) else {
        return;
    };
    assert_eq!(frame.at(4, 2), R);
    assert_eq!(frame.at(11, 7), R);
    assert_eq!(frame.at(3, 4), CLEAR);
    assert_eq!(frame.at(12, 4), CLEAR);
    assert_eq!(frame.at(6, 1), CLEAR);
    assert_eq!(frame.at(6, 9), CLEAR);
    // The clip's bottom edge falls mid-pixel: half coverage.
    assert!(close(frame.at(6, 8), [128, 0, 0, 128], 1));

    // A rounded clip cuts an image to the shape a rounded rect has.
    list.clear();
    list.push_rounded_clip(rect, 8.0);
    list.draw_image(red, rect);
    list.pop_clip();
    let clipped = render(SurfaceSize::new(16, 16), &list, &store).unwrap();
    list.clear();
    list.fill_rounded_rect(rect, 8.0, RED);
    let disc = render(SurfaceSize::new(16, 16), &list, &store).unwrap();
    for y in 0..16 {
        for x in 0..16 {
            assert!(close(clipped.at(x, y), disc.at(x, y), 1), "({x}, {y})");
        }
    }
}

#[test]
fn clipped_image_keeps_its_texels_in_place() {
    let mut store = ImageStore::new();
    let stripes = image(&mut store, &[[R, G, B, W, R, G, B, W]]);
    let mut list = DisplayList::new();
    list.push_clip(Rect::new(2.0, 0.0, 3.0, 4.0));
    list.draw_image(stripes, Rect::new(0.0, 0.0, 8.0, 4.0));
    list.pop_clip();
    let Some(frame) = render(SurfaceSize::new(8, 4), &list, &store) else {
        return;
    };
    let expected = [CLEAR, CLEAR, B, W, R, CLEAR, CLEAR, CLEAR];
    for (x, expected) in expected.into_iter().enumerate() {
        assert_eq!(frame.at(x as u32, 2), expected, "column {x}");
    }
}

// --- Paint order and batching ------------------------------------------------

/// Renders with a renderer the caller keeps, and reports what it cost.
fn render_kept(
    renderer: &mut OffscreenRenderer,
    size: SurfaceSize,
    list: &DisplayList,
    images: ImagesView<'_>,
) -> (Frame, RenderStats) {
    let pixels = renderer
        .render_with_images(size, NOTHING, list, SOLID_ATLAS, images)
        .expect("render");
    (
        Frame {
            width: size.width,
            pixels,
        },
        renderer.last_stats(),
    )
}

#[test]
fn paint_order_holds_across_images_rects_and_glyphs() {
    let Some(mut renderer) = skip_without_gpu(OffscreenRenderer::new()) else {
        return;
    };
    let mut store = ImageStore::new();
    let red = image(&mut store, &[[R]]);
    let green = image(&mut store, &[[G]]);
    let square = |x: f32, y: f32| Rect::new(x, y, 8.0, 8.0);

    // A staircase of overlapping squares, each covering the bottom-right
    // part of the one before: rect, image, glyph, other image, rect, first
    // image again, glyph.
    let mut list = DisplayList::new();
    list.fill_rect(square(0.0, 0.0), Color::WHITE);
    list.draw_image(red, square(4.0, 4.0));
    list.draw_glyph(square(8.0, 8.0), SOLID_GLYPH, BLUE);
    list.draw_image(green, square(12.0, 12.0));
    list.fill_rect(square(16.0, 16.0), Color::BLACK);
    list.draw_image(red, square(20.0, 20.0));
    list.draw_glyph(square(24.0, 24.0), SOLID_GLYPH, Color::WHITE);

    let size = SurfaceSize::new(32, 32);
    let (frame, stats) = render_kept(&mut renderer, size, &list, store.view());
    let steps = [W, R, B, G, K, R, W];
    for (index, expected) in steps.into_iter().enumerate() {
        let origin = index as u32 * 4;
        // The top-left part of each square is its own colour...
        assert_eq!(frame.at(origin + 1, origin + 1), expected, "step {index}");
        // ...and from its middle on it lies under the next one.
        if let Some(&next) = steps.get(index + 1) {
            assert_eq!(frame.at(origin + 5, origin + 5), next, "over step {index}");
        }
    }

    // Three image changes: red, green, red. Rectangles and glyphs join the
    // run that is open, so seven operations are three draw calls.
    assert_eq!(stats.instances, 7);
    assert_eq!(stats.draw_calls, 3);
    assert_eq!((stats.image_uploads, stats.image_textures), (2, 2));
}

#[test]
fn one_image_drawn_many_times_is_one_draw_call() {
    let Some(mut renderer) = skip_without_gpu(OffscreenRenderer::new()) else {
        return;
    };
    let mut store = ImageStore::new();
    // A sprite sheet: many pictures, one texture.
    let sheet = image(&mut store, &[[R, G], [B, W]]);
    let mut list = DisplayList::new();
    for index in 0..12 {
        let cell = Rect::new((index % 4) as f32 * 4.0, (index / 4) as f32 * 4.0, 4.0, 4.0);
        list.fill_rect(cell, Color::BLACK);
        list.draw_image_region(
            sheet,
            Rect::new((index % 2) as f32, ((index / 2) % 2) as f32, 1.0, 1.0),
            cell,
            0.0,
            Color::WHITE,
        );
    }
    let (frame, stats) = render_kept(&mut renderer, SurfaceSize::new(16, 12), &list, store.view());
    assert_eq!((stats.instances, stats.draw_calls), (24, 1));
    assert_eq!(frame.at(1, 1), R);
    assert_eq!(frame.at(5, 1), G);
    assert_eq!(frame.at(9, 1), B);
    assert_eq!(frame.at(13, 1), W);

    // And a list without images is still exactly one draw call.
    list.clear();
    list.fill_rect(Rect::new(0.0, 0.0, 4.0, 4.0), RED);
    list.draw_glyph(Rect::new(4.0, 0.0, 4.0, 4.0), SOLID_GLYPH, BLUE);
    let (frame, stats) = render_kept(&mut renderer, SurfaceSize::new(16, 12), &list, store.view());
    assert_eq!((stats.instances, stats.draw_calls), (2, 1));
    assert_eq!((frame.at(1, 1), frame.at(5, 1)), (R, B));
}

#[test]
fn many_distinct_images_in_one_frame() {
    let Some(mut renderer) = skip_without_gpu(OffscreenRenderer::new()) else {
        return;
    };
    let mut store = ImageStore::new();
    const COLUMNS: u32 = 10;
    const COUNT: u32 = 60;
    // Channel values 0 and 255 only, so every colour is exact.
    let colour = |index: u32| {
        let bit = |n: u32| if (index >> n) & 1 == 1 { 255 } else { 0 };
        [bit(0), bit(1), bit(2), 255]
    };
    let cell = |index: u32| {
        Rect::new(
            (index % COLUMNS) as f32 * 4.0,
            (index / COLUMNS) as f32 * 4.0,
            4.0,
            4.0,
        )
    };
    let mut list = DisplayList::new();
    for index in 0..COUNT {
        let picture = image(&mut store, &[[colour(index)]]);
        list.draw_image(picture, cell(index));
    }

    let size = SurfaceSize::new(COLUMNS * 4, COUNT / COLUMNS * 4);
    let (frame, stats) = render_kept(&mut renderer, size, &list, store.view());
    for index in 0..COUNT {
        let rect = cell(index);
        for (dx, dy) in [(0, 0), (3, 3)] {
            assert_eq!(
                frame.at(rect.x as u32 + dx, rect.y as u32 + dy),
                colour(index),
                "image {index}"
            );
        }
    }
    assert_eq!(stats.draw_calls, COUNT, "one per image");
    assert_eq!(stats.image_textures, COUNT);
    assert_eq!(stats.image_uploads, COUNT);
    assert_eq!(stats.image_texture_bytes, u64::from(COUNT) * 4);

    // The next frame uploads nothing and looks the same.
    let (again, stats) = render_kept(&mut renderer, size, &list, store.view());
    assert_eq!(stats.image_uploads, 0);
    assert_eq!(again.pixels, frame.pixels);
}

// --- Lifecycle ---------------------------------------------------------------

#[test]
fn textures_are_uploaded_on_first_draw_and_freed_on_release() {
    let Some(mut renderer) = skip_without_gpu(OffscreenRenderer::new()) else {
        return;
    };
    let size = SurfaceSize::new(8, 4);
    let mut store = ImageStore::new();
    let red = image(&mut store, &[[R]]);
    let green = image(&mut store, &[[G]]);
    let left = Rect::new(0.0, 0.0, 4.0, 4.0);
    let right = Rect::new(4.0, 0.0, 4.0, 4.0);

    // Registered but not drawn: nothing reaches the GPU.
    let mut list = DisplayList::new();
    list.fill_rect(left, BLUE);
    let (_, stats) = render_kept(&mut renderer, size, &list, store.view());
    assert_eq!((stats.image_uploads, stats.image_textures), (0, 0));

    // Drawn but entirely clipped away: still nothing.
    list.clear();
    list.push_clip(left);
    list.draw_image(green, Rect::new(5.0, 0.0, 3.0, 4.0));
    list.pop_clip();
    let (_, stats) = render_kept(&mut renderer, size, &list, store.view());
    assert_eq!((stats.image_uploads, stats.image_textures), (0, 0));

    list.clear();
    list.draw_image(red, left);
    list.draw_image(green, right);
    let (frame, stats) = render_kept(&mut renderer, size, &list, store.view());
    assert_eq!((frame.at(1, 1), frame.at(5, 1)), (R, G));
    assert_eq!((stats.image_uploads, stats.image_textures), (2, 2));
    assert_eq!(stats.image_texture_bytes, 8);

    // Releasing one image frees its texture on the next frame. The stale
    // handle is still in the list; drawing it is harmless and draws nothing.
    assert!(store.remove(red));
    let (frame, stats) = render_kept(&mut renderer, size, &list, store.view());
    assert_eq!((frame.at(1, 1), frame.at(5, 1)), (CLEAR, G));
    assert_eq!((stats.image_uploads, stats.image_textures), (0, 1));
    assert_eq!((stats.instances, stats.draw_calls), (1, 1));

    // Releasing the last one is noticed even by a frame that draws nothing.
    assert!(store.remove(green));
    let (_, stats) = render_kept(&mut renderer, size, &DisplayList::new(), store.view());
    assert_eq!(stats.image_textures, 0);
    assert_eq!(stats.image_texture_bytes, 0);
    assert_eq!(stats.draw_calls, 0);

    // A frame of stale handles only.
    let (frame, stats) = render_kept(&mut renderer, size, &list, store.view());
    assert!(frame.pixels.iter().all(|&byte| byte == 0));
    assert_eq!((stats.instances, stats.image_textures), (0, 0));
}

#[test]
fn a_new_renderer_uploads_again_from_the_same_store() {
    // What happens when the device (or the pipeline, on a surface format
    // change) is recreated: the old renderer's textures go with it, and the
    // new one fills up again from the pixels the store kept.
    let mut store = ImageStore::new();
    let red = image(&mut store, &[[R]]);
    let mut list = DisplayList::new();
    list.draw_image(red, Rect::new(0.0, 0.0, 4.0, 4.0));
    let size = SurfaceSize::new(4, 4);

    for _ in 0..2 {
        let Some(mut renderer) = skip_without_gpu(OffscreenRenderer::new()) else {
            return;
        };
        assert_eq!(renderer.last_stats(), RenderStats::default());
        let (frame, stats) = render_kept(&mut renderer, size, &list, store.view());
        assert_eq!(frame.at(2, 2), R);
        assert_eq!((stats.image_uploads, stats.image_textures), (1, 1));
    }
}

#[test]
fn a_renderer_given_another_store_drops_the_first_ones_textures() {
    let Some(mut renderer) = skip_without_gpu(OffscreenRenderer::new()) else {
        return;
    };
    let size = SurfaceSize::new(4, 4);
    let rect = Rect::new(0.0, 0.0, 4.0, 4.0);
    let mut first = ImageStore::new();
    let mut second = ImageStore::new();
    let red = image(&mut first, &[[R]]);
    let green = image(&mut second, &[[G]]);

    let mut list = DisplayList::new();
    list.draw_image(red, rect);
    let (frame, _) = render_kept(&mut renderer, size, &list, first.view());
    assert_eq!(frame.at(0, 0), R);

    // A handle from the wrong store names nothing.
    let (frame, stats) = render_kept(&mut renderer, size, &list, second.view());
    assert_eq!(frame.at(0, 0), CLEAR);
    assert_eq!(stats.image_textures, 0);

    list.clear();
    list.draw_image(green, rect);
    let (frame, stats) = render_kept(&mut renderer, size, &list, second.view());
    assert_eq!(frame.at(0, 0), G);
    assert_eq!(stats.image_textures, 1);
}

#[test]
fn entry_points_without_images_draw_everything_else() {
    let mut store = ImageStore::new();
    let red = image(&mut store, &[[R]]);
    let mut list = DisplayList::new();
    list.fill_rect(Rect::new(0.0, 0.0, 2.0, 2.0), BLUE);
    list.draw_image(red, Rect::new(2.0, 0.0, 2.0, 2.0));
    list.fill_rect(Rect::new(4.0, 0.0, 2.0, 2.0), BLUE);

    let size = SurfaceSize::new(6, 2);
    let Some(pixels) = skip_without_gpu(render_offscreen(size, NOTHING, &list, AtlasView::EMPTY))
    else {
        return;
    };
    let frame = Frame {
        width: size.width,
        pixels,
    };
    assert_eq!(frame.at(0, 0), B);
    assert_eq!(frame.at(2, 0), CLEAR, "no images were supplied");
    assert_eq!(frame.at(4, 0), B);

    let mut renderer = OffscreenRenderer::new().unwrap();
    let pixels = renderer
        .render(size, NOTHING, &list, AtlasView::EMPTY)
        .unwrap();
    assert_eq!(pixels, frame.pixels);
    assert_eq!(renderer.last_stats().draw_calls, 1);
}

// --- Size --------------------------------------------------------------------

#[test]
fn oversized_image_is_drawn_from_its_reduced_copy() {
    let mut store = ImageStore::new();
    // Twice the limit wide: red on the left half, blue on the right.
    let width = MAX_IMAGE_DIMENSION * 2;
    let mut pixels = Vec::with_capacity(width as usize * 2 * 4);
    for _ in 0..2 {
        for x in 0..width {
            pixels.extend_from_slice(if x < width / 2 { &R } else { &B });
        }
    }
    let huge = store.add_rgba8_vec(width, 2, pixels).unwrap();
    assert_eq!(huge.width(), width, "the handle keeps the original size");

    let Some(mut renderer) = skip_without_gpu(OffscreenRenderer::new()) else {
        return;
    };
    let mut list = DisplayList::new();
    list.draw_image(huge, Rect::new(0.0, 0.0, 16.0, 4.0));
    // Source rectangles stay in original pixels: the last quarter is blue,
    // the second quarter red.
    let quarter = width as f32 / 4.0;
    list.draw_image_region(
        huge,
        Rect::new(quarter * 3.0, 0.0, quarter, 2.0),
        Rect::new(0.0, 4.0, 8.0, 4.0),
        0.0,
        Color::WHITE,
    );
    list.draw_image_region(
        huge,
        Rect::new(quarter, 0.0, quarter, 2.0),
        Rect::new(8.0, 4.0, 8.0, 4.0),
        0.0,
        Color::WHITE,
    );
    let (frame, stats) = render_kept(&mut renderer, SurfaceSize::new(16, 8), &list, store.view());
    assert_eq!(frame.at(1, 1), R);
    assert_eq!(frame.at(6, 2), R);
    assert_eq!(frame.at(9, 1), B);
    assert_eq!(frame.at(14, 2), B);
    assert_eq!(frame.at(3, 6), B);
    assert_eq!(frame.at(12, 6), R);
    // Stored at the limit, 2048 x 1, with its eleven smaller mip levels.
    assert_eq!(stats.image_textures, 1);
    assert_eq!(stats.image_texture_bytes, 4 * (2048 * 2 - 1));
}

#[test]
fn a_large_image_drawn_small_is_averaged_not_point_sampled() {
    let mut store = ImageStore::new();
    // Vertical stripes four texels wide, black and white, phased so that
    // every sampling point of a 16-times reduction lands inside a white
    // stripe: without mipmaps the reduced image would be pure white.
    const SIDE: u32 = 256;
    let mut pixels = Vec::with_capacity((SIDE * SIDE * 4) as usize);
    for _ in 0..SIDE {
        for x in 0..SIDE {
            pixels.extend_from_slice(if ((x + 2) / 4) % 2 == 0 { &W } else { &K });
        }
    }
    let stripes = store.add_rgba8_vec(SIDE, SIDE, pixels).unwrap();
    let mut list = DisplayList::new();
    list.draw_image(stripes, Rect::new(0.0, 0.0, 16.0, 16.0));
    // The same image at its own size keeps its stripes exactly.
    list.draw_image_region(
        stripes,
        Rect::new(0.0, 0.0, 16.0, 16.0),
        Rect::new(16.0, 0.0, 16.0, 16.0),
        0.0,
        Color::WHITE,
    );

    let Some(frame) = render(SurfaceSize::new(32, 16), &list, &store) else {
        return;
    };
    for y in 0..16 {
        for x in 0..16 {
            // Half the light: 128 in the linear target.
            assert!(
                close(frame.at(x, y), [128, 128, 128, 255], 2),
                "({x}, {y}): {:?}",
                frame.at(x, y)
            );
            let expected = if ((x + 2) / 4) % 2 == 0 { W } else { K };
            assert_eq!(frame.at(16 + x, y), expected, "unscaled ({x}, {y})");
        }
    }
}

#[test]
fn reduction_between_mip_levels_stays_smooth() {
    let mut store = ImageStore::new();
    // A fine checkerboard reduced by a factor that is not a power of two.
    const SIDE: u32 = 128;
    let mut pixels = Vec::with_capacity((SIDE * SIDE * 4) as usize);
    for y in 0..SIDE {
        for x in 0..SIDE {
            pixels.extend_from_slice(if (x + y) % 2 == 0 { &W } else { &K });
        }
    }
    let checks = store.add_rgba8_vec(SIDE, SIDE, pixels).unwrap();
    let mut list = DisplayList::new();
    list.draw_image(checks, Rect::new(0.0, 0.0, 20.0, 20.0));
    let Some(frame) = render(SurfaceSize::new(20, 20), &list, &store) else {
        return;
    };
    for y in 0..20 {
        for x in 0..20 {
            assert!(
                close(frame.at(x, y), [128, 128, 128, 255], 3),
                "({x}, {y}): {:?}",
                frame.at(x, y)
            );
        }
    }
}
