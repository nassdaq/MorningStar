//! Pixel tests against the real GPU on the machine running them (Metal on a
//! macOS dev host). They skip, rather than fail, where no adapter exists
//! (e.g. CI containers without a GPU).
//!
//! The offscreen target is RGBA8 with premultiplied alpha, holding colours
//! as the pipeline composites them: sRGB-encoded (ADR 0023). The colours
//! used here have components of 0 or 1, which encode to themselves, so
//! expected bytes are simply the blended value times 255;
//! `tests/compositing.rs` covers the colours in between.

use morningstar_core::{AtlasView, Color, DisplayList, Rect, SurfaceSize};
use morningstar_gpu::{FrameOutcome, GpuError, Renderer, render_offscreen};

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const GREEN: Color = Color::rgb(0.0, 1.0, 0.0);
const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);

/// A rendered frame with pixel lookup.
struct Image {
    width: u32,
    pixels: Vec<u8>,
}

impl Image {
    fn at(&self, x: u32, y: u32) -> [u8; 4] {
        let start = ((y * self.width + x) * 4) as usize;
        self.pixels[start..start + 4].try_into().unwrap()
    }
}

fn render(size: SurfaceSize, clear: Color, list: &DisplayList) -> Option<Image> {
    match render_offscreen(size, clear, list, AtlasView::EMPTY) {
        Ok(pixels) => {
            assert_eq!(pixels.len(), (size.width * size.height * 4) as usize);
            Some(Image {
                width: size.width,
                pixels,
            })
        }
        Err(GpuError::NoAdapter(reason)) => {
            eprintln!("skipping: no GPU adapter available ({reason})");
            None
        }
        Err(other) => panic!("offscreen render failed: {other}"),
    }
}

#[test]
fn empty_list_clears_every_pixel() {
    // 13 wide so rows need padding in the readback buffer.
    let size = SurfaceSize::new(13, 7);
    let Some(image) = render(size, Color::rgb(1.0, 0.5, 0.0), &DisplayList::new()) else {
        return;
    };

    for y in 0..7 {
        for x in 0..13 {
            let [r, g, b, a] = image.at(x, y);
            assert_eq!((r, b, a), (255, 0, 255), "pixel {x},{y}");
            // Half the light is byte 188 once sRGB-encoded, which is what a
            // screen shows for it (it was 128 while the target held linear
            // values; ADR 0023).
            assert!(g.abs_diff(188) <= 1, "green was {g} at {x},{y}");
        }
    }
}

#[test]
fn pixel_aligned_rect_fills_exactly_its_pixels() {
    let mut list = DisplayList::new();
    list.fill_rect(Rect::new(4.0, 6.0, 8.0, 5.0), RED);
    let Some(image) = render(SurfaceSize::new(16, 16), BLUE, &list) else {
        return;
    };

    for y in 0..16 {
        for x in 0..16 {
            let inside = (4..12).contains(&x) && (6..11).contains(&y);
            let expected = if inside {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            };
            assert_eq!(image.at(x, y), expected, "pixel {x},{y}");
        }
    }
}

#[test]
fn later_operations_paint_over_earlier_ones() {
    let mut list = DisplayList::new();
    list.fill_rect(Rect::new(0.0, 0.0, 8.0, 8.0), RED);
    list.fill_rect(Rect::new(4.0, 0.0, 8.0, 8.0), GREEN);
    let Some(image) = render(SurfaceSize::new(16, 8), Color::BLACK, &list) else {
        return;
    };

    assert_eq!(image.at(2, 4), [255, 0, 0, 255], "red only");
    assert_eq!(
        image.at(6, 4),
        [0, 255, 0, 255],
        "overlap shows the later rect"
    );
    assert_eq!(image.at(10, 4), [0, 255, 0, 255], "green only");
    assert_eq!(image.at(14, 4), [0, 0, 0, 255], "untouched");
}

#[test]
fn translucent_rect_blends_with_what_is_beneath() {
    let mut list = DisplayList::new();
    list.fill_rect(
        Rect::new(0.0, 0.0, 8.0, 8.0),
        Color::rgba(1.0, 1.0, 1.0, 0.5),
    );
    let Some(image) = render(SurfaceSize::new(8, 8), BLUE, &list) else {
        return;
    };

    // 50% white over opaque blue: (0.5, 0.5, 1.0), still opaque.
    let [r, g, b, a] = image.at(4, 4);
    assert!(r.abs_diff(128) <= 1 && g.abs_diff(128) <= 1, "got {r},{g}");
    assert_eq!((b, a), (255, 255));
}

#[test]
fn rounded_rect_cuts_corners_and_antialiases_the_edge() {
    // Radius far larger than the box: clamped, giving a circle of radius 16
    // centred at (16, 16).
    let mut list = DisplayList::new();
    list.fill_rounded_rect(Rect::new(0.0, 0.0, 32.0, 32.0), 1000.0, RED);
    let Some(image) = render(SurfaceSize::new(32, 32), Color::BLACK, &list) else {
        return;
    };

    assert_eq!(image.at(16, 16), [255, 0, 0, 255], "centre is filled");
    assert_eq!(
        image.at(0, 0),
        [0, 0, 0, 255],
        "corner is outside the circle"
    );
    assert_eq!(image.at(31, 31), [0, 0, 0, 255], "opposite corner too");
    assert_eq!(image.at(16, 1), [255, 0, 0, 255], "just inside the top");

    // Somewhere along the 45-degree diagonal the edge crosses pixels
    // partially; those must be neither background nor full colour.
    let partial = (0..16)
        .map(|i| image.at(i, i)[0])
        .filter(|&red| red > 8 && red < 247)
        .count();
    assert!(partial >= 1, "expected an anti-aliased edge pixel");

    // The shape is symmetric, so its rendering must be too.
    for i in 0..16 {
        assert_eq!(image.at(i, i), image.at(31 - i, i), "mirror at {i}");
        assert_eq!(image.at(i, i), image.at(i, 31 - i), "flip at {i}");
    }
}

#[test]
fn half_pixel_edges_get_half_coverage() {
    // Spans x = 2.5..5.5: pixels 2 and 5 are half covered, 3 and 4 fully.
    let mut list = DisplayList::new();
    list.fill_rect(Rect::new(2.5, 0.0, 3.0, 4.0), Color::WHITE);
    let Some(image) = render(SurfaceSize::new(8, 4), Color::BLACK, &list) else {
        return;
    };

    let row: Vec<u8> = (0..8).map(|x| image.at(x, 2)[0]).collect();
    assert_eq!((row[0], row[1], row[6], row[7]), (0, 0, 0, 0), "{row:?}");
    assert_eq!((row[3], row[4]), (255, 255), "{row:?}");
    assert!(
        row[2].abs_diff(128) <= 2 && row[5].abs_diff(128) <= 2,
        "{row:?}"
    );
}

#[test]
fn more_rects_than_the_initial_buffer_holds() {
    // 300 one-pixel columns, alternating colours: forces the instance buffer
    // to grow past its initial 64 and checks none are lost or reordered.
    let mut list = DisplayList::new();
    for x in 0..300 {
        let color = if x % 2 == 0 { RED } else { GREEN };
        list.fill_rect(Rect::new(x as f32, 0.0, 1.0, 2.0), color);
    }
    let Some(image) = render(SurfaceSize::new(300, 2), Color::BLACK, &list) else {
        return;
    };

    for x in 0..300 {
        let expected = if x % 2 == 0 {
            [255, 0, 0, 255]
        } else {
            [0, 255, 0, 255]
        };
        assert_eq!(image.at(x, 1), expected, "column {x}");
    }
}

#[test]
fn empty_target_yields_no_pixels() {
    let pixels = render_offscreen(
        SurfaceSize::new(0, 16),
        Color::WHITE,
        &DisplayList::new(),
        AtlasView::EMPTY,
    );
    assert_eq!(pixels, Ok(Vec::new()));
}

#[test]
fn renderer_without_surface_skips_frames() {
    let mut renderer = Renderer::new();
    assert!(!renderer.has_surface());
    assert_eq!(renderer.adapter_info(), None);
    assert_eq!(
        renderer.render(Color::BLACK, &DisplayList::new(), AtlasView::EMPTY),
        Ok(FrameOutcome::Skipped)
    );
    // Must be harmless with nothing attached.
    renderer.resize(SurfaceSize::new(100, 100));
    renderer.detach_surface();
}

#[test]
fn glyph_draws_its_atlas_region_tinted() {
    // A 4x2 atlas. The glyph uses the 2x2 region at (1, 0):
    //   row 0: 255 128
    //   row 1:   0 255
    #[rustfmt::skip]
    let pixels = [
        9, 255, 128, 9,
        9,   0, 255, 9,
    ];
    let atlas = AtlasView {
        width: 4,
        height: 2,
        pixels: &pixels,
        generation: 1,
        dirty: None,
    };

    let mut list = DisplayList::new();
    list.draw_glyph(
        Rect::new(3.0, 2.0, 2.0, 2.0),
        Rect::new(1.0, 0.0, 2.0, 2.0),
        GREEN,
    );
    let size = SurfaceSize::new(8, 6);
    let pixels = match render_offscreen(size, Color::BLACK, &list, atlas) {
        Ok(pixels) => pixels,
        Err(GpuError::NoAdapter(_)) => return,
        Err(other) => panic!("offscreen render failed: {other}"),
    };
    let image = Image { width: 8, pixels };

    // Coverage 255 -> full green; 128 -> half; 0 -> background.
    assert_eq!(image.at(3, 2), [0, 255, 0, 255]);
    assert!(image.at(4, 2)[1].abs_diff(128) <= 1, "{:?}", image.at(4, 2));
    assert_eq!(image.at(3, 3), [0, 0, 0, 255]);
    assert_eq!(image.at(4, 3), [0, 255, 0, 255]);
    // Neighbouring atlas texels (the 9s) must not leak in, and nothing is
    // drawn outside the glyph's rectangle.
    for (x, y) in [(2, 2), (5, 2), (2, 3), (5, 3), (3, 1), (4, 4)] {
        assert_eq!(image.at(x, y), [0, 0, 0, 255], "pixel {x},{y}");
    }
}

#[test]
fn rects_and_glyphs_keep_paint_order() {
    let atlas = AtlasView {
        width: 1,
        height: 1,
        pixels: &[255],
        generation: 1,
        dirty: None,
    };
    let solid = Rect::new(0.0, 0.0, 1.0, 1.0);

    // Rect, then glyph over its left half, then rect over the glyph's top.
    let mut list = DisplayList::new();
    list.fill_rect(Rect::new(0.0, 0.0, 8.0, 8.0), RED);
    list.draw_glyph(Rect::new(0.0, 0.0, 4.0, 8.0), solid, GREEN);
    list.fill_rect(Rect::new(0.0, 0.0, 8.0, 4.0), BLUE);

    let pixels = match render_offscreen(SurfaceSize::new(8, 8), Color::BLACK, &list, atlas) {
        Ok(pixels) => pixels,
        Err(GpuError::NoAdapter(_)) => return,
        Err(other) => panic!("offscreen render failed: {other}"),
    };
    let image = Image { width: 8, pixels };

    assert_eq!(image.at(2, 2), [0, 0, 255, 255], "last rect on top");
    assert_eq!(image.at(2, 6), [0, 255, 0, 255], "glyph over first rect");
    assert_eq!(image.at(6, 6), [255, 0, 0, 255], "first rect alone");
}
