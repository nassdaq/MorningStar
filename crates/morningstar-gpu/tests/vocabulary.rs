//! Pixel tests for the drawing vocabulary beyond filled rectangles and
//! glyphs: borders, linear gradients, box shadows and clipping (ADR 0011).
//!
//! Like `offscreen.rs` these run on the real GPU of the machine and skip
//! where there is none. The target is RGBA8 holding sRGB-encoded values,
//! composited as such (ADR 0023); the colours here have components of 0 or
//! 1, so expected bytes are the blended value times 255.
//!
//! Set `MORNINGSTAR_DUMP_DIR` to a directory to have the scenes that are
//! meant to be judged by eye written there as sRGB-encoded PPM files.

use morningstar_core::{
    AtlasView, Border, Color, Dash, DisplayList, LinearGradient, Point, Rect, Shadow, SurfaceSize,
};
use morningstar_gpu::{GpuError, render_offscreen};

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const GREEN: Color = Color::rgb(0.0, 1.0, 0.0);
const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);

const RED_PX: [u8; 4] = [255, 0, 0, 255];
const GREEN_PX: [u8; 4] = [0, 255, 0, 255];
const BLUE_PX: [u8; 4] = [0, 0, 255, 255];
const BLACK_PX: [u8; 4] = [0, 0, 0, 255];
const WHITE_PX: [u8; 4] = [255, 255, 255, 255];

/// A one-texel, fully covered atlas: glyphs drawn from it are solid boxes.
const SOLID_ATLAS: AtlasView<'static> = AtlasView::new(1, 1, &[255], 1);
const SOLID_GLYPH: Rect = Rect::new(0.0, 0.0, 1.0, 1.0);

/// A rendered frame with pixel lookup.
struct Image {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Image {
    fn at(&self, x: u32, y: u32) -> [u8; 4] {
        let start = ((y * self.width + x) * 4) as usize;
        self.pixels[start..start + 4].try_into().unwrap()
    }

    /// Writes the image where a person can look at it, if asked to.
    fn dump(&self, name: &str) {
        let Some(dir) = std::env::var_os("MORNINGSTAR_DUMP_DIR") else {
            return;
        };
        // The target's bytes are what a screen shows.
        let encode = |byte: u8| byte;
        let mut file = format!("P6\n{} {}\n255\n", self.width, self.height).into_bytes();
        for pixel in self.pixels.chunks_exact(4) {
            file.extend(pixel[..3].iter().map(|&channel| encode(channel)));
        }
        let path = std::path::Path::new(&dir).join(format!("{name}.ppm"));
        std::fs::write(&path, file).expect("write dump");
    }
}

fn render_with(
    size: SurfaceSize,
    clear: Color,
    list: &DisplayList,
    atlas: AtlasView<'_>,
) -> Option<Image> {
    match render_offscreen(size, clear, list, atlas) {
        Ok(pixels) => {
            assert_eq!(pixels.len(), (size.width * size.height * 4) as usize);
            Some(Image {
                width: size.width,
                height: size.height,
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

fn render(size: SurfaceSize, clear: Color, list: &DisplayList) -> Option<Image> {
    render_with(size, clear, list, AtlasView::EMPTY)
}

fn close(a: [u8; 4], b: [u8; 4], tolerance: u8) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= tolerance)
}

// ---------------------------------------------------------------- borders

#[test]
fn stroke_draws_only_a_ring_inside_the_bounds() {
    let mut list = DisplayList::new();
    list.stroke_rect(Rect::new(2.0, 2.0, 12.0, 12.0), Border::new(2.0, RED));
    let Some(image) = render(SurfaceSize::new(16, 16), BLUE, &list) else {
        return;
    };

    for y in 0..16 {
        for x in 0..16 {
            let in_bounds = (2..14).contains(&x) && (2..14).contains(&y);
            let in_hole = (4..12).contains(&x) && (4..12).contains(&y);
            let expected = if in_bounds && !in_hole {
                RED_PX
            } else {
                BLUE_PX
            };
            assert_eq!(image.at(x, y), expected, "pixel {x},{y}");
        }
    }
}

#[test]
fn fill_and_border_are_drawn_together() {
    let mut list = DisplayList::new();
    list.fill_bordered_rect(
        Rect::new(1.0, 1.0, 14.0, 10.0),
        0.0,
        GREEN,
        Border::new(3.0, RED),
    );
    let Some(image) = render(SurfaceSize::new(16, 12), BLUE, &list) else {
        return;
    };

    for y in 0..12 {
        for x in 0..16 {
            let in_bounds = (1..15).contains(&x) && (1..11).contains(&y);
            let in_fill = (4..12).contains(&x) && (4..8).contains(&y);
            let expected = match (in_bounds, in_fill) {
                (_, true) => GREEN_PX,
                (true, false) => RED_PX,
                (false, false) => BLUE_PX,
            };
            assert_eq!(image.at(x, y), expected, "pixel {x},{y}");
        }
    }
}

#[test]
fn translucent_border_shows_the_fill_beneath_it() {
    let mut list = DisplayList::new();
    list.fill_bordered_rect(
        Rect::new(0.0, 0.0, 12.0, 12.0),
        0.0,
        BLUE,
        Border::new(3.0, Color::rgba(1.0, 1.0, 1.0, 0.5)),
    );
    // The same border with nothing beneath it but the background.
    list.stroke_rect(
        Rect::new(12.0, 0.0, 12.0, 12.0),
        Border::new(3.0, Color::rgba(1.0, 1.0, 1.0, 0.5)),
    );
    let Some(image) = render(SurfaceSize::new(24, 12), Color::BLACK, &list) else {
        return;
    };

    assert_eq!(image.at(6, 6), BLUE_PX, "fill alone");
    // 50% white over blue.
    assert!(
        close(image.at(1, 6), [128, 128, 255, 255], 1),
        "{:?}",
        image.at(1, 6)
    );
    // 50% white over black.
    assert!(
        close(image.at(13, 6), [128, 128, 128, 255], 1),
        "{:?}",
        image.at(13, 6)
    );
    assert_eq!(image.at(18, 6), BLACK_PX, "an outline has no fill");
}

#[test]
fn border_inner_edge_is_antialiased() {
    // Border 2.5 wide: on the left it covers pixels 0 and 1 fully and half
    // of pixel 2; on the right half of pixel 9 and all of 10 and 11.
    let mut list = DisplayList::new();
    list.fill_bordered_rect(
        Rect::new(0.0, 0.0, 12.0, 12.0),
        0.0,
        Color::BLACK,
        Border::new(2.5, Color::WHITE),
    );
    let Some(image) = render(SurfaceSize::new(12, 12), BLUE, &list) else {
        return;
    };

    let row: Vec<u8> = (0..12).map(|x| image.at(x, 6)[0]).collect();
    assert_eq!((row[0], row[1], row[10], row[11]), (255, 255, 255, 255));
    assert!(row[3..9].iter().all(|&v| v == 0), "{row:?}");
    assert!(
        row[2].abs_diff(128) <= 2 && row[9].abs_diff(128) <= 2,
        "{row:?}"
    );
    // The fill is opaque black, so blue never shows through the seam.
    for x in 0..12 {
        assert_eq!(image.at(x, 6)[2], image.at(x, 6)[0], "pixel {x}");
    }
}

#[test]
fn rounded_stroke_is_antialiased_on_both_edges_without_fill_fringe() {
    // A ring: outer radius 16, inner radius 12, centred at (16, 16).
    let ring = Rect::new(0.0, 0.0, 32.0, 32.0);
    let mut list = DisplayList::new();
    list.fill_bordered_rect(ring, 1000.0, GREEN, Border::new(4.0, RED));
    let Some(image) = render(SurfaceSize::new(32, 32), Color::BLACK, &list) else {
        return;
    };

    assert_eq!(image.at(16, 16), GREEN_PX, "centre is fill");
    assert_eq!(image.at(16, 1), RED_PX, "top of the ring");
    assert_eq!(image.at(0, 0), BLACK_PX, "outside");

    let mut outer_partial = 0;
    let mut inner_partial = 0;
    for y in 0..32 {
        for x in 0..32 {
            let [r, g, b, a] = image.at(x, y);
            let (dx, dy) = (x as f32 + 0.5 - 16.0, y as f32 + 0.5 - 16.0);
            let distance = (dx * dx + dy * dy).sqrt();
            assert_eq!((b, a), (0, 255), "pixel {x},{y}");

            // Compare with the exact shape, within anti-aliasing accuracy.
            let outer = (0.5 - (distance - 16.0)).clamp(0.0, 1.0);
            let inner = (0.5 - (distance - 12.0)).clamp(0.0, 1.0);
            let expected_red = (outer - inner) * 255.0;
            let expected_green = inner * 255.0;
            assert!(
                (f32::from(r) - expected_red).abs() <= 3.0
                    && (f32::from(g) - expected_green).abs() <= 3.0,
                "pixel {x},{y}: got {r},{g}, expected {expected_red},{expected_green}"
            );

            // The fill must not leak through the anti-aliased outer edge.
            if distance > 13.0 {
                assert_eq!(g, 0, "green fringe at {x},{y}");
            }
            if (15.6..16.4).contains(&distance) && r > 8 && r < 247 {
                outer_partial += 1;
            }
            if (11.6..12.4).contains(&distance) && r > 8 && r < 247 && g > 8 {
                inner_partial += 1;
            }
        }
    }
    assert!(outer_partial >= 8, "outer edge is anti-aliased");
    assert!(inner_partial >= 8, "inner edge is anti-aliased");

    for i in 0..16 {
        assert_eq!(image.at(i, i), image.at(31 - i, i), "mirror at {i}");
        assert_eq!(image.at(i, i), image.at(i, 31 - i), "flip at {i}");
    }
}

#[test]
fn border_wider_than_the_corner_radius_has_square_inner_corners() {
    // Radius 4, border 6: the hole is the sharp rectangle 6..26.
    let mut list = DisplayList::new();
    list.stroke_rounded_rect(Rect::new(0.0, 0.0, 32.0, 32.0), 4.0, Border::new(6.0, RED));
    let Some(image) = render(SurfaceSize::new(32, 32), BLUE, &list) else {
        return;
    };

    assert_eq!(image.at(6, 6), BLUE_PX, "inner corner pixel is hole");
    assert_eq!(image.at(5, 5), RED_PX);
    assert_eq!(image.at(5, 16), RED_PX);
    assert_eq!(image.at(6, 16), BLUE_PX);
    assert_eq!(image.at(25, 25), BLUE_PX);
    assert_eq!(image.at(26, 26), RED_PX);
    assert_eq!(image.at(0, 0), BLUE_PX, "outer corner is rounded");
}

// -------------------------------------------------------------- gradients

// ---------------------------------------------------------------- dashes

#[test]
fn dashes_on_a_straight_edge_are_exact() {
    let mut list = DisplayList::new();
    // Middle line: a 38 x 18 box. The pattern starts at the bottom of the
    // left edge, so the top edge begins 18 px into it.
    list.stroke_dashed_rect(
        Rect::new(2.0, 2.0, 40.0, 20.0),
        0.0,
        Border::new(2.0, RED),
        Dash::new(4.0, 4.0),
    );
    let Some(image) = render(SurfaceSize::new(44, 24), BLUE, &list) else {
        return;
    };
    for x in 5..39 {
        // Distance along the pattern of this pixel's centre. The pattern
        // starts at the top of the left edge and runs down it, round the
        // bottom and up the right (18 + 38 + 18 along the middle of the
        // stroke), so the top edge comes last, right to left, and ends
        // where the pattern began, 112 along. (It ran clockwise from the
        // bottom of the left edge until Chrome's was measured; ADR 0022.)
        let along = 112.0 - (x as f32 + 0.5 - 3.0);
        let expected = if along.rem_euclid(8.0) < 4.0 {
            RED_PX
        } else {
            BLUE_PX
        };
        for y in 2..4 {
            assert_eq!(image.at(x, y), expected, "pixel {x},{y}");
        }
        // Nothing inside the border.
        assert_eq!(image.at(x, 5), BLUE_PX, "pixel {x},5");
    }
}

#[test]
fn gaps_of_a_dashed_border_show_the_fill() {
    let mut list = DisplayList::new();
    list.fill_dashed_rect(
        Rect::new(2.0, 2.0, 40.0, 20.0),
        0.0,
        GREEN,
        Border::new(2.0, RED),
        Dash::new(4.0, 4.0),
    );
    let Some(image) = render(SurfaceSize::new(44, 24), BLUE, &list) else {
        return;
    };
    let mut seen = (false, false);
    for x in 5..39 {
        let pixel = image.at(x, 2);
        assert!(
            pixel == RED_PX || pixel == GREEN_PX,
            "pixel {x},2: {pixel:?}"
        );
        seen.0 |= pixel == RED_PX;
        seen.1 |= pixel == GREEN_PX;
        assert_eq!(image.at(x, 10), GREEN_PX);
    }
    assert_eq!(seen, (true, true));
}

/// Sum of the red channel over the image, in pixels of full coverage.
fn red_area(image: &Image) -> f32 {
    let mut total = 0.0;
    for y in 0..image.height {
        for x in 0..image.width {
            total += f32::from(image.at(x, y)[0]) / 255.0;
        }
    }
    total
}

#[test]
fn a_dashed_ring_covers_its_share_of_the_solid_ring() {
    let size = SurfaceSize::new(128, 80);
    let black = Color::rgb(0.0, 0.0, 0.0);
    for (rect, radius, width, dash) in [
        (
            Rect::new(4.0, 4.0, 120.0, 72.0),
            24.0,
            2.0,
            Dash::new(6.0, 4.0),
        ),
        (
            Rect::new(4.0, 4.0, 120.0, 72.0),
            24.0,
            2.0,
            Dash::new(6.0, 6.0),
        ),
        (
            Rect::new(10.0, 6.0, 64.0, 64.0),
            32.0,
            4.0,
            Dash::new(8.0, 4.0),
        ),
        (
            Rect::new(3.5, 4.5, 99.0, 41.0),
            0.0,
            3.0,
            Dash::new(5.0, 7.0),
        ),
    ] {
        let border = Border::new(width, RED);
        let dash = dash.fitted(rect, radius, width);

        let mut solid = DisplayList::new();
        solid.stroke_rounded_rect(rect, radius, border);
        let mut dashed = DisplayList::new();
        dashed.stroke_dashed_rect(rect, radius, border, dash);
        let (Some(solid), Some(dashed)) =
            (render(size, black, &solid), render(size, black, &dashed))
        else {
            return;
        };

        let share = red_area(&dashed) / red_area(&solid);
        let expected = dash.length / (dash.length + dash.gap);
        assert!(
            (share - expected).abs() < 0.03,
            "{rect:?} r{radius} {dash:?}: covers {share}, expected {expected}"
        );
        // Dashes never paint where the solid border does not.
        for y in 0..size.height {
            for x in 0..size.width {
                assert!(
                    dashed.at(x, y)[0] <= solid.at(x, y)[0].saturating_add(1),
                    "pixel {x},{y}"
                );
            }
        }
    }
}

#[test]
fn dashes_follow_a_circle_in_equal_steps() {
    // A circle of middle radius 28 has a circumference of about 175.93;
    // 16 dashes of half a period each.
    let rect = Rect::new(2.0, 2.0, 60.0, 60.0);
    let period = std::f32::consts::TAU * 28.0 / 16.0;
    let mut list = DisplayList::new();
    list.stroke_dashed_rect(
        rect,
        30.0,
        Border::new(4.0, RED),
        Dash::new(period / 2.0, period / 2.0),
    );
    let black = Color::rgb(0.0, 0.0, 0.0);
    let Some(image) = render(SurfaceSize::new(64, 64), black, &list) else {
        return;
    };
    // Walk the middle of the stroke and count dark-to-red transitions.
    let mut transitions = 0;
    let mut previous = None;
    let steps = 1440;
    for step in 0..=steps {
        let angle = step as f32 / steps as f32 * std::f32::consts::TAU;
        let x = (32.0 + 28.0 * angle.cos()) as u32;
        let y = (32.0 + 28.0 * angle.sin()) as u32;
        let on = image.at(x, y)[0] > 127;
        if previous == Some(false) && on {
            transitions += 1;
        }
        previous = Some(on);
    }
    assert_eq!(transitions, 16);
}

#[test]
fn horizontal_gradient_is_linear_in_the_composited_values() {
    let rect = Rect::new(0.0, 0.0, 256.0, 4.0);
    let mut list = DisplayList::new();
    list.fill_gradient_rect(
        rect,
        0.0,
        LinearGradient::horizontal(rect, Color::BLACK, Color::WHITE),
    );
    let Some(image) = render(SurfaceSize::new(256, 4), RED, &list) else {
        return;
    };

    for x in 0..256 {
        // Sampled at the pixel centre; the bytes on screen rise evenly, as
        // a CSS gradient's do.
        let expected = (x as f32 + 0.5) / 256.0 * 255.0;
        let [r, g, b, a] = image.at(x, 2);
        assert!(
            (f32::from(r) - expected).abs() <= 1.0,
            "column {x}: {r} vs {expected}"
        );
        assert_eq!((r, 255), (g, a), "grey and opaque at {x}");
        assert_eq!(g, b);
    }
}

#[test]
fn gradient_holds_its_end_colours_beyond_the_stops() {
    // Stops at x = 8 and x = 24 of a 32-wide rectangle.
    let mut list = DisplayList::new();
    list.fill_gradient_rect(
        Rect::new(0.0, 0.0, 32.0, 4.0),
        0.0,
        LinearGradient::new(Point::new(8.0, 0.0), Point::new(24.0, 0.0), RED, BLUE),
    );
    let Some(image) = render(SurfaceSize::new(32, 4), Color::BLACK, &list) else {
        return;
    };

    for x in 0..8 {
        assert_eq!(image.at(x, 1), RED_PX, "before the first stop, column {x}");
    }
    for x in 24..32 {
        assert_eq!(image.at(x, 1), BLUE_PX, "after the last stop, column {x}");
    }
    assert!(
        close(image.at(16, 1), [120, 0, 135, 255], 1),
        "{:?}",
        image.at(16, 1)
    );
}

#[test]
fn angled_gradient_runs_along_its_angle() {
    // 45 degrees on a square: from the bottom-left corner to the top-right.
    let rect = Rect::new(0.0, 0.0, 64.0, 64.0);
    let mut list = DisplayList::new();
    list.fill_gradient_rect(
        rect,
        0.0,
        LinearGradient::angled(rect, 45.0, Color::BLACK, Color::WHITE),
    );
    let Some(image) = render(SurfaceSize::new(64, 64), RED, &list) else {
        return;
    };

    for y in 0..64 {
        for x in 0..64 {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let expected = (px + (64.0 - py)) / 128.0 * 255.0;
            let value = image.at(x, y)[0];
            assert!(
                (f32::from(value) - expected).abs() <= 1.5,
                "pixel {x},{y}: {value} vs {expected}"
            );
        }
    }
    // Lines of constant colour run perpendicular to the direction.
    assert_eq!(image.at(10, 10), image.at(40, 40));

    // 180 degrees is top to bottom, as in CSS.
    let mut list = DisplayList::new();
    list.fill_gradient_rect(
        rect,
        0.0,
        LinearGradient::angled(rect, 180.0, Color::BLACK, Color::WHITE),
    );
    let image = render(SurfaceSize::new(64, 64), RED, &list).unwrap();
    assert!(image.at(5, 0)[0] < 4 && image.at(60, 63)[0] > 251);
    assert_eq!(image.at(3, 30), image.at(50, 30));
}

#[test]
fn gradient_to_transparent_keeps_its_hue() {
    // Opaque red to transparent blue: premultiplied interpolation fades the
    // red out without ever mixing blue in.
    let rect = Rect::new(0.0, 0.0, 64.0, 4.0);
    let mut list = DisplayList::new();
    list.fill_gradient_rect(
        rect,
        0.0,
        LinearGradient::horizontal(rect, RED, Color::rgba(0.0, 0.0, 1.0, 0.0)),
    );
    let Some(image) = render(SurfaceSize::new(64, 4), Color::BLACK, &list) else {
        return;
    };

    for x in 0..64 {
        let [r, g, b, _] = image.at(x, 2);
        let expected = (1.0 - (x as f32 + 0.5) / 64.0) * 255.0;
        assert!((f32::from(r) - expected).abs() <= 1.0, "column {x}: {r}");
        assert_eq!((g, b), (0, 0), "column {x}");
    }
}

#[test]
fn gradient_fill_respects_rounded_corners() {
    let rect = Rect::new(0.0, 0.0, 32.0, 32.0);
    let mut list = DisplayList::new();
    list.fill_gradient_rect(rect, 1000.0, LinearGradient::vertical(rect, RED, GREEN));
    let mut solid = DisplayList::new();
    solid.fill_rounded_rect(rect, 1000.0, Color::WHITE);
    let size = SurfaceSize::new(32, 32);
    let (Some(image), Some(mask)) = (
        render(size, Color::BLACK, &list),
        render(size, Color::BLACK, &solid),
    ) else {
        return;
    };

    assert_eq!(image.at(0, 0), BLACK_PX);
    assert!(image.at(16, 1)[0] > 240 && image.at(16, 30)[1] > 240);
    // Same silhouette as a solid circle: red + green adds up to its coverage.
    for y in 0..32 {
        for x in 0..32 {
            let [r, g, _, _] = image.at(x, y);
            let coverage = mask.at(x, y)[0];
            assert!(
                (u16::from(r) + u16::from(g)).abs_diff(u16::from(coverage)) <= 1,
                "pixel {x},{y}: {r}+{g} vs {coverage}"
            );
        }
    }
}

// ---------------------------------------------------------------- shadows

/// Coverage of a rounded rectangle blurred by a Gaussian, computed the slow
/// and obvious way: rasterise the shape with 4x4 supersampling, then
/// convolve with a sampled Gaussian kernel along each axis.
fn reference_shadow(size: SurfaceSize, rect: Rect, radius: f32, sigma: f32) -> Vec<f32> {
    let pad = (sigma * 4.0).ceil() as i32;
    let (w, h) = (size.width as i32 + 2 * pad, size.height as i32 + 2 * pad);
    let inside = |x: f32, y: f32| {
        let (hx, hy) = (rect.width / 2.0, rect.height / 2.0);
        let (px, py) = (
            (x - (rect.x + hx)).abs() - hx + radius,
            (y - (rect.y + hy)).abs() - hy + radius,
        );
        let outside = (px.max(0.0).powi(2) + py.max(0.0).powi(2)).sqrt();
        outside + px.max(py).min(0.0) - radius <= 0.0
    };

    let mut mask = vec![0.0f32; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let mut hits = 0;
            for sy in 0..4 {
                for sx in 0..4 {
                    let fx = (x - pad) as f32 + (sx as f32 + 0.5) / 4.0;
                    let fy = (y - pad) as f32 + (sy as f32 + 0.5) / 4.0;
                    hits += u32::from(inside(fx, fy));
                }
            }
            mask[(y * w + x) as usize] = hits as f32 / 16.0;
        }
    }

    let kernel: Vec<f32> = (-pad..=pad)
        .map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp())
        .collect();
    let total: f32 = kernel.iter().sum();
    let blur = |source: &[f32], horizontal: bool| {
        let mut out = vec![0.0f32; source.len()];
        for y in 0..h {
            for x in 0..w {
                let mut sum = 0.0;
                for (k, weight) in kernel.iter().enumerate() {
                    let offset = k as i32 - pad;
                    let (sx, sy) = if horizontal {
                        (x + offset, y)
                    } else {
                        (x, y + offset)
                    };
                    if sx >= 0 && sx < w && sy >= 0 && sy < h {
                        sum += source[(sy * w + sx) as usize] * weight;
                    }
                }
                out[(y * w + x) as usize] = sum / total;
            }
        }
        out
    };
    let blurred = blur(&blur(&mask, true), false);

    let mut result = Vec::with_capacity((size.width * size.height) as usize);
    for y in 0..size.height as i32 {
        for x in 0..size.width as i32 {
            result.push(blurred[((y + pad) * w + x + pad) as usize]);
        }
    }
    result
}

#[test]
fn shadow_matches_a_gaussian_blur_of_the_rounded_rect() {
    let size = SurfaceSize::new(200, 160);
    // (rect, corner radius, blur radius): typical cards, a pill, a small
    // box under a blur larger than itself, and square corners.
    let cases = [
        (Rect::new(50.0, 40.0, 100.0, 80.0), 12.0, 4.0),
        (Rect::new(50.0, 40.0, 100.0, 80.0), 12.0, 16.0),
        (Rect::new(50.0, 40.0, 100.0, 80.0), 12.0, 40.0),
        (Rect::new(50.0, 40.0, 100.0, 80.0), 0.0, 24.0),
        (Rect::new(40.0, 60.0, 120.0, 40.0), 20.0, 12.0),
        (Rect::new(85.0, 65.0, 30.0, 30.0), 6.0, 40.0),
        (Rect::new(60.5, 50.25, 81.0, 61.5), 30.75, 9.0),
    ];
    for (rect, radius, blur) in cases {
        let mut list = DisplayList::new();
        list.draw_shadow(rect, radius, Shadow::new(0.0, 0.0, blur, Color::WHITE));
        let Some(image) = render(size, Color::BLACK, &list) else {
            return;
        };
        let reference = reference_shadow(size, rect, radius, blur / 2.0);

        let mut worst = (0.0f32, 0, 0);
        for y in 0..size.height {
            for x in 0..size.width {
                let expected = reference[(y * size.width + x) as usize] * 255.0;
                let error = (f32::from(image.at(x, y)[0]) - expected).abs();
                if error > worst.0 {
                    worst = (error, x, y);
                }
            }
        }
        eprintln!(
            "shadow {rect:?} radius {radius} blur {blur}: worst error {:.2}/255 at {},{}",
            worst.0, worst.1, worst.2
        );
        assert!(
            worst.0 <= 2.5,
            "blur {blur}, radius {radius}, {rect:?}: off by {} at {},{}",
            worst.0,
            worst.1,
            worst.2
        );
    }
}

#[test]
fn shadow_is_smooth_and_fades_to_nothing() {
    // A wide blur under a card: walking outwards from the edge the shadow
    // must only ever get lighter, in small steps, and reach zero before the
    // quad that draws it ends.
    let rect = Rect::new(100.0, 100.0, 120.0, 80.0);
    let mut list = DisplayList::new();
    list.draw_shadow(rect, 16.0, Shadow::new(0.0, 0.0, 40.0, Color::WHITE));
    let Some(image) = render(SurfaceSize::new(320, 280), Color::BLACK, &list) else {
        return;
    };

    let row: Vec<u8> = (0..320).map(|x| image.at(x, 140)[0]).collect();
    let column: Vec<u8> = (0..280).map(|y| image.at(160, y)[0]).collect();
    let diagonal: Vec<u8> = (0..100).map(|i| image.at(i, i)[0]).collect();
    for (name, line, centre) in [("row", &row, 160), ("column", &column, 140)] {
        for i in 1..line.len() {
            let (previous, current) = (i16::from(line[i - 1]), i16::from(line[i]));
            let step = current - previous;
            assert!(step.abs() <= 7, "{name} jumps by {step} at {i}");
            if i <= centre {
                assert!(step >= 0, "{name} dips at {i}");
            } else {
                assert!(step <= 0, "{name} rises at {i}");
            }
        }
        assert_eq!(line[0], 0, "{name} starts clear");
        assert_eq!(line[line.len() - 1], 0, "{name} ends clear");
    }
    for i in 1..diagonal.len() {
        assert!(diagonal[i] >= diagonal[i - 1], "diagonal dips at {i}");
    }
    // Where the quad ends (3 sigma + 1 = 61 pixels out) the shadow is so
    // faint that the cut-off cannot be seen.
    assert!(
        row[100 - 61] <= 1 && row[100 - 62] == 0,
        "{:?}",
        &row[30..45]
    );
}

#[test]
fn shadow_offset_spread_and_zero_blur() {
    // No blur: the shadow is the rectangle itself, moved and grown.
    let rect = Rect::new(8.0, 8.0, 8.0, 8.0);
    let mut list = DisplayList::new();
    list.draw_shadow(rect, 0.0, Shadow::new(4.0, 6.0, 0.0, RED).with_spread(2.0));
    let Some(image) = render(SurfaceSize::new(32, 32), BLUE, &list) else {
        return;
    };
    for y in 0..32 {
        for x in 0..32 {
            // 8..16 moved by (4, 6) and grown by 2 on each side.
            let inside = (10..22).contains(&x) && (12..24).contains(&y);
            let expected = if inside { RED_PX } else { BLUE_PX };
            assert_eq!(image.at(x, y), expected, "pixel {x},{y}");
        }
    }

    // With blur, an offset moves the whole shadow by exactly that much.
    let size = SurfaceSize::new(96, 96);
    let card = Rect::new(32.0, 32.0, 24.0, 24.0);
    let mut centred = DisplayList::new();
    centred.draw_shadow(card, 6.0, Shadow::new(0.0, 0.0, 12.0, Color::WHITE));
    let mut moved = DisplayList::new();
    moved.draw_shadow(card, 6.0, Shadow::new(5.0, 9.0, 12.0, Color::WHITE));
    let centred = render(size, Color::BLACK, &centred).unwrap();
    let moved = render(size, Color::BLACK, &moved).unwrap();
    assert!(centred.at(44, 44)[0] > 200);
    for y in 0..80 {
        for x in 0..80 {
            assert!(
                close(centred.at(x, y), moved.at(x + 5, y + 9), 1),
                "pixel {x},{y}"
            );
        }
    }
}

#[test]
fn shadow_is_tinted_and_lies_beneath_what_is_drawn_after_it() {
    let card = Rect::new(24.0, 24.0, 48.0, 48.0);
    let mut list = DisplayList::new();
    list.draw_shadow(
        card,
        8.0,
        Shadow::new(0.0, 8.0, 16.0, Color::rgba(0.0, 0.0, 0.0, 0.5)),
    );
    list.fill_rounded_rect(card, 8.0, Color::WHITE);
    let Some(image) = render(SurfaceSize::new(96, 96), Color::WHITE, &list) else {
        return;
    };

    assert_eq!(image.at(48, 48), WHITE_PX, "the card covers its shadow");
    let below = image.at(48, 76)[0];
    let above = image.at(48, 20)[0];
    assert!(below < 200, "shadow visible below the card: {below}");
    assert!(above > below + 40, "and much fainter above: {above}");
    assert!(below >= 127, "never darker than its 50% alpha allows");
    assert_eq!(image.at(2, 2), WHITE_PX);
}

// --------------------------------------------------------------- clipping

#[test]
fn clip_restricts_drawing_until_popped() {
    let mut list = DisplayList::new();
    list.push_clip(Rect::new(4.0, 2.0, 6.0, 8.0));
    list.fill_rect(Rect::new(0.0, 0.0, 16.0, 16.0), RED);
    list.pop_clip();
    list.fill_rect(Rect::new(12.0, 12.0, 4.0, 4.0), GREEN);
    let Some(image) = render(SurfaceSize::new(16, 16), BLUE, &list) else {
        return;
    };

    for y in 0..16 {
        for x in 0..16 {
            let expected = if (4..10).contains(&x) && (2..10).contains(&y) {
                RED_PX
            } else if x >= 12 && y >= 12 {
                GREEN_PX
            } else {
                BLUE_PX
            };
            assert_eq!(image.at(x, y), expected, "pixel {x},{y}");
        }
    }
}

#[test]
fn nested_clips_intersect_and_unwind_in_order() {
    let all = Rect::new(0.0, 0.0, 16.0, 16.0);
    let mut list = DisplayList::new();
    list.push_clip(Rect::new(0.0, 0.0, 10.0, 16.0));
    list.push_clip(Rect::new(6.0, 4.0, 10.0, 8.0));
    list.fill_rect(all, RED); // x 6..10, y 4..12
    list.pop_clip();
    list.fill_rect(Rect::new(0.0, 14.0, 16.0, 2.0), GREEN); // x 0..10
    list.pop_clip();
    // More pops than pushes are harmless.
    list.pop_clip();
    list.fill_rect(Rect::new(14.0, 0.0, 2.0, 2.0), GREEN);
    let Some(image) = render(SurfaceSize::new(16, 16), BLUE, &list) else {
        return;
    };

    for y in 0..16 {
        for x in 0..16 {
            let expected = if (6..10).contains(&x) && (4..12).contains(&y) {
                RED_PX
            } else if (x < 10 && y >= 14) || (x >= 14 && y < 2) {
                GREEN_PX
            } else {
                BLUE_PX
            };
            assert_eq!(image.at(x, y), expected, "pixel {x},{y}");
        }
    }
}

#[test]
fn clip_applies_to_every_kind_of_operation() {
    // Five 16x16 cells in a row, each clipped to its middle 8x8, each
    // holding one kind of operation that would otherwise fill the cell.
    let mut list = DisplayList::new();
    for cell in 0..5 {
        let x = cell as f32 * 16.0;
        let whole = Rect::new(x, 0.0, 16.0, 16.0);
        list.push_clip(Rect::new(x + 4.0, 4.0, 8.0, 8.0));
        match cell {
            0 => list.fill_rounded_rect(whole, 3.0, RED),
            1 => list.draw_glyph(whole, SOLID_GLYPH, RED),
            2 => list.fill_bordered_rect(whole, 0.0, RED, Border::new(6.0, RED)),
            3 => list.fill_gradient_rect(whole, 0.0, LinearGradient::vertical(whole, RED, RED)),
            _ => list.draw_shadow(whole, 0.0, Shadow::new(0.0, 0.0, 8.0, RED)),
        }
        list.pop_clip();
    }
    let Some(image) = render_with(SurfaceSize::new(80, 16), BLUE, &list, SOLID_ATLAS) else {
        return;
    };

    for y in 0..16 {
        for x in 0..80 {
            let inside = (4..12).contains(&(x % 16)) && (4..12).contains(&y);
            let pixel = image.at(x, y);
            if !inside {
                assert_eq!(pixel, BLUE_PX, "pixel {x},{y} is outside its clip");
            } else if x < 64 {
                assert_eq!(pixel, RED_PX, "pixel {x},{y}");
            } else {
                // The middle of a blurred shadow: nearly, not exactly, full.
                assert!(pixel[0] > 150 && pixel[1] == 0, "pixel {x},{y}: {pixel:?}");
            }
        }
    }
}

#[test]
fn clipped_glyph_keeps_its_texels_in_place() {
    // Clipping must cut the glyph, not squeeze or shift it.
    #[rustfmt::skip]
    let texels = [
         10,  20,  30,  40,
         50,  60,  70,  80,
         90, 100, 110, 120,
        130, 140, 150, 160,
    ];
    let atlas = AtlasView::new(4, 4, &texels, 1);
    let mut list = DisplayList::new();
    list.push_clip(Rect::new(3.0, 3.0, 2.0, 2.0));
    list.draw_glyph(
        Rect::new(2.0, 2.0, 4.0, 4.0),
        Rect::new(0.0, 0.0, 4.0, 4.0),
        Color::WHITE,
    );
    list.pop_clip();
    let Some(image) = render_with(SurfaceSize::new(8, 8), Color::BLACK, &list, atlas) else {
        return;
    };

    for y in 0..8u32 {
        for x in 0..8u32 {
            let expected = if (3..5).contains(&x) && (3..5).contains(&y) {
                texels[((y - 2) * 4 + (x - 2)) as usize]
            } else {
                0
            };
            assert_eq!(image.at(x, y)[0], expected, "pixel {x},{y}");
        }
    }
}

#[test]
fn clip_edge_between_pixels_is_antialiased() {
    // Clip spans x = 2.5..5.5, exactly like the half-pixel rectangle test.
    let mut list = DisplayList::new();
    list.push_clip(Rect::new(2.5, 0.0, 3.0, 4.0));
    list.fill_rect(Rect::new(0.0, 0.0, 8.0, 4.0), Color::WHITE);
    list.pop_clip();
    // A clip narrower than a pixel lets through only its own width.
    list.push_clip(Rect::new(7.25, 0.0, 0.5, 4.0));
    list.fill_rect(Rect::new(0.0, 0.0, 8.0, 4.0), Color::WHITE);
    list.pop_clip();
    let Some(image) = render(SurfaceSize::new(8, 4), Color::BLACK, &list) else {
        return;
    };

    let row: Vec<u8> = (0..8).map(|x| image.at(x, 2)[0]).collect();
    assert_eq!((row[0], row[1], row[6]), (0, 0, 0), "{row:?}");
    assert_eq!((row[3], row[4]), (255, 255), "{row:?}");
    assert!(
        row[2].abs_diff(128) <= 2 && row[5].abs_diff(128) <= 2,
        "{row:?}"
    );
    assert!(row[7].abs_diff(128) <= 2, "{row:?}");
}

#[test]
fn empty_clip_hides_everything_inside_it() {
    let all = Rect::new(0.0, 0.0, 8.0, 8.0);
    let mut list = DisplayList::new();
    list.push_clip(Rect::new(4.0, 4.0, 0.0, 0.0));
    list.fill_rect(all, RED);
    list.draw_shadow(all, 0.0, Shadow::new(0.0, 0.0, 8.0, RED));
    // Nothing nested inside an empty clip can bring pixels back.
    list.push_clip(all);
    list.fill_rect(all, RED);
    list.pop_clip();
    list.pop_clip();
    // Two clips that do not overlap.
    list.push_clip(Rect::new(0.0, 0.0, 4.0, 8.0));
    list.push_rounded_clip(Rect::new(4.0, 0.0, 4.0, 8.0), 2.0);
    list.fill_rect(all, RED);
    list.pop_clip();
    list.pop_clip();
    let Some(image) = render(SurfaceSize::new(8, 8), BLUE, &list) else {
        return;
    };
    for y in 0..8 {
        for x in 0..8 {
            assert_eq!(image.at(x, y), BLUE_PX, "pixel {x},{y}");
        }
    }
}

#[test]
fn rounded_clip_matches_a_rounded_rect() {
    // Filling everything through a rounded clip must give the same pixels
    // as drawing that rounded rectangle.
    let shape = Rect::new(3.0, 5.0, 40.0, 28.0);
    let size = SurfaceSize::new(48, 40);
    let mut clipped = DisplayList::new();
    clipped.push_rounded_clip(shape, 9.0);
    clipped.fill_rect(Rect::new(0.0, 0.0, 48.0, 40.0), RED);
    clipped.pop_clip();
    clipped.fill_rect(Rect::new(0.0, 0.0, 2.0, 2.0), GREEN);
    let mut direct = DisplayList::new();
    direct.fill_rounded_rect(shape, 9.0, RED);
    direct.fill_rect(Rect::new(0.0, 0.0, 2.0, 2.0), GREEN);
    let (Some(clipped), Some(direct)) = (
        render(size, Color::BLACK, &clipped),
        render(size, Color::BLACK, &direct),
    ) else {
        return;
    };

    let mut partial = 0;
    for y in 0..40 {
        for x in 0..48 {
            assert!(
                close(clipped.at(x, y), direct.at(x, y), 1),
                "pixel {x},{y}: {:?} vs {:?}",
                clipped.at(x, y),
                direct.at(x, y)
            );
            let red = clipped.at(x, y)[0];
            partial += u32::from(red > 8 && red < 247);
        }
    }
    assert!(partial >= 8, "the clip's corners are anti-aliased");
    assert_eq!(clipped.at(3, 5), BLACK_PX, "corner is clipped away");
    assert_eq!(clipped.at(1, 1), GREEN_PX, "the clip ended at its pop");
}

#[test]
fn rounded_clip_inside_a_square_clip_keeps_its_corners() {
    // The scrolling case: a rounded card partly scrolled out of a viewport.
    // The viewport cuts it straight; the corners still in view stay round.
    let card = Rect::new(4.0, 4.0, 40.0, 40.0);
    let viewport = Rect::new(0.0, 20.0, 48.0, 28.0);
    let everything = Rect::new(0.0, 0.0, 48.0, 48.0);
    let size = SurfaceSize::new(48, 48);

    let mut list = DisplayList::new();
    list.push_clip(viewport);
    list.push_rounded_clip(card, 12.0);
    list.fill_rect(everything, RED);
    list.pop_clip();
    list.pop_clip();
    // The other nesting order must give the same result.
    let mut swapped = DisplayList::new();
    swapped.push_rounded_clip(card, 12.0);
    swapped.push_clip(viewport);
    swapped.fill_rect(everything, RED);
    swapped.pop_clip();
    swapped.pop_clip();
    let mut whole = DisplayList::new();
    whole.fill_rounded_rect(card, 12.0, RED);

    let (Some(image), Some(swapped), Some(whole)) = (
        render(size, Color::BLACK, &list),
        render(size, Color::BLACK, &swapped),
        render(size, Color::BLACK, &whole),
    ) else {
        return;
    };
    assert_eq!(image.pixels, swapped.pixels);
    for y in 0..48 {
        for x in 0..48 {
            let expected = if y >= 20 { whole.at(x, y) } else { BLACK_PX };
            assert!(
                close(image.at(x, y), expected, 1),
                "pixel {x},{y}: {:?} vs {expected:?}",
                image.at(x, y)
            );
        }
    }
    assert_eq!(
        image.at(4, 20),
        RED_PX,
        "cut straight, not rounded, at the top"
    );
    assert_eq!(image.at(5, 42), BLACK_PX, "bottom corner still rounded");
}

#[test]
fn nested_rounded_clips_round_only_the_innermost() {
    // The documented limit: with one rounded clip inside another, the inner
    // one is exact and the outer one acts as its bounding rectangle.
    let outer = Rect::new(0.0, 0.0, 32.0, 32.0);
    let inner = Rect::new(0.0, 0.0, 64.0, 64.0);
    let mut list = DisplayList::new();
    list.push_rounded_clip(outer, 16.0);
    list.push_rounded_clip(inner, 4.0);
    list.fill_rect(Rect::new(0.0, 0.0, 64.0, 64.0), RED);
    list.pop_clip();
    // Back to the outer clip alone: round again.
    list.fill_rect(Rect::new(0.0, 0.0, 64.0, 64.0), GREEN);
    list.pop_clip();
    let Some(image) = render(SurfaceSize::new(64, 64), BLUE, &list) else {
        return;
    };

    assert_eq!(image.at(0, 0), BLUE_PX, "inner clip's own corner is round");
    assert_eq!(
        image.at(31, 31),
        RED_PX,
        "outer circle's corner was not cut"
    );
    assert_eq!(image.at(31, 2), RED_PX);
    assert_eq!(image.at(32, 16), BLUE_PX, "but its bounds still apply");
    assert_eq!(
        image.at(16, 16),
        GREEN_PX,
        "the outer clip alone, after the pop"
    );
}

// ------------------------------------------------------------- by eye

/// A screen of the kind the vocabulary exists for. Asserts little; its job
/// is to be looked at (see the module documentation).
#[test]
fn card_scene() {
    let size = SurfaceSize::new(720, 560);
    let background = Color::rgb(0.90, 0.91, 0.93);
    let ink = Color::rgb(0.03, 0.04, 0.07);
    let mut list = DisplayList::new();

    // Top row: the same card under blur 4, 12, 24 and 40.
    for (i, blur) in [4.0, 12.0, 24.0, 40.0].into_iter().enumerate() {
        let card = Rect::new(30.0 + i as f32 * 172.0, 30.0, 140.0, 100.0);
        list.draw_shadow(
            card,
            14.0,
            Shadow::new(0.0, blur / 3.0, blur, Color::rgba(0.0, 0.0, 0.0, 0.35)),
        );
        list.fill_rounded_rect(card, 14.0, Color::WHITE);
    }

    // Second row: gradients at four angles, the last fading to transparent.
    let blue = Color::rgb(0.05, 0.2, 0.9);
    let pink = Color::rgb(0.9, 0.1, 0.4);
    for (i, angle) in [0.0, 90.0, 135.0, 180.0].into_iter().enumerate() {
        let rect = Rect::new(30.0 + i as f32 * 172.0, 170.0, 140.0, 80.0);
        let to = if i == 3 {
            Color::rgba(0.9, 0.1, 0.4, 0.0)
        } else {
            pink
        };
        list.fill_gradient_rect(rect, 18.0, LinearGradient::angled(rect, angle, blue, to));
    }

    // Third row: borders. Hairline, thick, pill button, translucent.
    let row = 290.0;
    list.stroke_rounded_rect(
        Rect::new(30.0, row, 140.0, 80.0),
        12.0,
        Border::new(1.0, ink),
    );
    list.fill_bordered_rect(
        Rect::new(202.0, row, 140.0, 80.0),
        24.0,
        Color::rgb(1.0, 0.85, 0.2),
        Border::new(8.0, ink),
    );
    let pill = Rect::new(374.0, row + 16.0, 140.0, 48.0);
    list.draw_shadow(
        pill,
        24.0,
        Shadow::new(0.0, 6.0, 16.0, Color::rgba(0.05, 0.2, 0.9, 0.45)),
    );
    list.fill_gradient_rect(
        pill,
        24.0,
        LinearGradient::vertical(pill, Color::rgb(0.2, 0.45, 1.0), blue),
    );
    list.stroke_rounded_rect(
        pill,
        24.0,
        Border::new(1.5, Color::rgba(1.0, 1.0, 1.0, 0.5)),
    );
    list.fill_bordered_rect(
        Rect::new(546.0, row, 140.0, 80.0),
        4.0,
        pink,
        Border::new(10.0, Color::rgba(0.0, 0.0, 0.0, 0.3)),
    );

    // Bottom row: clipping. A rounded card with a header band and overflowing
    // stripes, whole; then the same card scrolled half out of a viewport.
    let card_at = |list: &mut DisplayList, card: Rect| {
        list.draw_shadow(
            card,
            20.0,
            Shadow::new(0.0, 8.0, 24.0, Color::rgba(0.0, 0.0, 0.0, 0.3)),
        );
        list.push_rounded_clip(card, 20.0);
        list.fill_rect(card, Color::WHITE);
        list.fill_gradient_rect(
            Rect::new(card.x, card.y, card.width, 40.0),
            0.0,
            LinearGradient::horizontal(card, blue, pink),
        );
        for stripe in 0..8 {
            list.fill_rect(
                Rect::new(
                    card.x - 20.0 + stripe as f32 * 40.0,
                    card.y + card.height - 30.0,
                    20.0,
                    60.0,
                ),
                ink,
            );
        }
        list.pop_clip();
    };
    card_at(&mut list, Rect::new(30.0, 410.0, 240.0, 120.0));
    let viewport = Rect::new(320.0, 410.0, 370.0, 120.0);
    list.fill_rect(viewport, Color::rgb(0.8, 0.82, 0.86));
    list.push_clip(viewport);
    card_at(&mut list, Rect::new(340.0, 360.0, 150.0, 120.0));
    card_at(&mut list, Rect::new(520.0, 470.0, 150.0, 120.0));
    list.pop_clip();

    let Some(image) = render(size, background, &list) else {
        return;
    };
    image.dump("card_scene");

    assert_eq!(image.at(100, 80), WHITE_PX, "a card");
    let under_card = image.at(444, 140);
    let page = image.at(8, 8);
    assert!(
        under_card[0] < page[0],
        "a shadow darkens the page below a card"
    );
    assert_eq!(
        image.at(700, 400),
        page,
        "nothing escapes the viewport clip"
    );
}

/// Shadows at the blur radii designs use, light on dark and dark on light,
/// for judging by eye.
#[test]
fn shadow_sheet() {
    let size = SurfaceSize::new(760, 420);
    let mut list = DisplayList::new();
    list.fill_rect(
        Rect::new(0.0, 210.0, 760.0, 210.0),
        Color::rgb(0.02, 0.02, 0.03),
    );
    for (i, blur) in [4.0, 8.0, 16.0, 24.0, 40.0].into_iter().enumerate() {
        let x = 30.0 + i as f32 * 148.0;
        let light = Rect::new(x, 50.0, 100.0, 100.0);
        list.draw_shadow(
            light,
            16.0,
            Shadow::new(0.0, 0.0, blur, Color::rgba(0.0, 0.0, 0.0, 0.8)),
        );
        list.fill_rounded_rect(light, 16.0, Color::WHITE);
        // A glow: shadow with nothing on top, where banding would show most.
        list.draw_shadow(
            Rect::new(x, 265.0, 100.0, 100.0),
            if i % 2 == 0 { 16.0 } else { 50.0 },
            Shadow::new(0.0, 0.0, blur, Color::WHITE),
        );
    }
    let Some(image) = render(size, Color::WHITE, &list) else {
        return;
    };
    image.dump("shadow_sheet");
    assert_eq!(image.at(80, 100), WHITE_PX);
    assert!(
        image.at(80, 315)[0] > 240,
        "the middle of a glow is nearly solid"
    );
}
