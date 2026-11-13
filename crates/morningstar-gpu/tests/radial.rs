//! Pixel tests for radial gradients (plain and blurred), inner shadows and
//! rectangles with a radius per corner (ADR 0033 to 0036).
//!
//! Like `vocabulary.rs` these run on the real GPU of the machine and skip
//! where there is none. The target holds sRGB-encoded values composited as
//! such (ADR 0023), so the expected byte of a mix is the mix of the bytes.

use morningstar_core::{
    Border, Color, CornerRadii, DisplayList, DrawOp, GradientStop, Point, RadialGradient, Rect,
    Shadow, SurfaceSize, Transform,
};
use morningstar_gpu::{GpuError, render_offscreen};

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const GREEN: Color = Color::rgb(0.0, 1.0, 0.0);
const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);
const WHITE: Color = Color::rgb(1.0, 1.0, 1.0);
const BLACK: Color = Color::rgb(0.0, 0.0, 0.0);
const CLEAR: Color = Color::rgba(0.0, 0.0, 0.0, 0.0);

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
    match render_offscreen(size, clear, list, morningstar_core::AtlasView::EMPTY) {
        Ok(pixels) => Some(Image {
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

/// What a radial gradient is at a pixel centre, as premultiplied values in
/// 0..=1: the definition the shader is held to.
fn radial_at(gradient: &RadialGradient<'_>, x: f32, y: f32) -> [f32; 4] {
    let along = ((x - gradient.center.x) / gradient.radius_x)
        .hypot((y - gradient.center.y) / gradient.radius_y);
    let premultiplied = |color: Color| {
        [
            color.r * color.a,
            color.g * color.a,
            color.b * color.a,
            color.a,
        ]
    };
    let stops = gradient.stops;
    if along <= stops[0].offset {
        return premultiplied(stops[0].color);
    }
    for pair in stops.windows(2) {
        if along < pair[1].offset {
            let t = (along - pair[0].offset) / (pair[1].offset - pair[0].offset);
            let (a, b) = (premultiplied(pair[0].color), premultiplied(pair[1].color));
            return std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t);
        }
    }
    premultiplied(stops[stops.len() - 1].color)
}

/// `source` over opaque `under`, as a byte.
fn over(source: [f32; 4], under: [f32; 3]) -> [u8; 3] {
    std::array::from_fn(|i| ((source[i] + under[i] * (1.0 - source[3])) * 255.0).round() as u8)
}

fn assert_close(actual: [u8; 4], expected: [u8; 3], tolerance: u8, what: &str) {
    for channel in 0..3 {
        assert!(
            actual[channel].abs_diff(expected[channel]) <= tolerance,
            "{what}: got {actual:?}, expected {expected:?}"
        );
    }
}

#[test]
fn two_stops_are_linear_along_every_ray_and_hold_their_ends() {
    let rect = Rect::new(0.0, 0.0, 64.0, 64.0);
    let stops = [GradientStop::new(0.25, RED), GradientStop::new(0.75, BLUE)];
    let gradient = RadialGradient {
        center: Point::new(32.0, 32.0),
        radius_x: 40.0,
        radius_y: 40.0,
        stops: &stops,
    };
    let mut list = DisplayList::new();
    list.fill_radial_gradient_rect(rect, 0.0, gradient, 0.0);
    // Inside the first stop, between the two, beyond the last.
    assert_eq!(list.len(), 3);
    let Some(image) = render(SurfaceSize::new(64, 64), BLACK, &list) else {
        return;
    };
    for y in 0..64 {
        for x in 0..64 {
            let expected = over(
                radial_at(&gradient, x as f32 + 0.5, y as f32 + 0.5),
                [0.0; 3],
            );
            assert_close(image.at(x, y), expected, 2, &format!("pixel {x},{y}"));
        }
    }
    // The centre is the first colour, the corner the last.
    assert_eq!(image.at(32, 32), [255, 0, 0, 255]);
    assert_eq!(image.at(0, 0), [0, 0, 255, 255]);
}

#[test]
fn four_stops_on_an_off_centre_ellipse_leave_no_seam() {
    let rect = Rect::new(4.0, 2.0, 88.0, 60.0);
    let stops = [
        GradientStop::new(0.0, WHITE),
        GradientStop::new(0.45, RED),
        GradientStop::new(0.75, GREEN),
        GradientStop::new(1.0, BLUE),
    ];
    let gradient = RadialGradient {
        center: Point::new(30.4, 15.2),
        radius_x: 70.0,
        radius_y: 48.0,
        stops: &stops,
    };
    let mut list = DisplayList::new();
    list.fill_radial_gradient_rect(rect, 0.0, gradient, 0.0);
    // Three segments and the solid part beyond the last stop.
    assert_eq!(list.len(), 4);
    let Some(image) = render(SurfaceSize::new(96, 64), BLACK, &list) else {
        return;
    };
    for y in 2..62 {
        for x in 4..92 {
            let expected = over(
                radial_at(&gradient, x as f32 + 0.5, y as f32 + 0.5),
                [0.0; 3],
            );
            // A seam between segments would be a dark or doubled pixel.
            assert_close(image.at(x, y), expected, 3, &format!("pixel {x},{y}"));
        }
    }
    // Outside the rectangle nothing is painted.
    assert_eq!(image.at(1, 1), [0, 0, 0, 255]);
    assert_eq!(image.at(94, 32), [0, 0, 0, 255]);
}

#[test]
fn a_fade_to_transparent_keeps_its_hue_and_lets_what_is_under_show() {
    // rgba(255,0,0,1) to rgba(0,0,255,0): premultiplied, the blue never
    // appears, and half way the red is at half strength over the white.
    let stops = [
        GradientStop::new(0.0, RED),
        GradientStop::new(1.0, Color::rgba(0.0, 0.0, 1.0, 0.0)),
    ];
    let gradient = RadialGradient {
        center: Point::new(0.0, 8.0),
        radius_x: 64.0,
        radius_y: 64.0,
        stops: &stops,
    };
    let mut list = DisplayList::new();
    list.fill_radial_gradient_rect(Rect::new(0.0, 0.0, 80.0, 16.0), 0.0, gradient, 0.0);
    // The transparent part beyond the last stop is not drawn at all.
    assert_eq!(list.len(), 1);
    let Some(image) = render(SurfaceSize::new(80, 16), WHITE, &list) else {
        return;
    };
    let middle = image.at(32, 8);
    // The pixel's centre is 32.5 of 64 along: 49.2% red over white.
    assert_close(middle, [255, 130, 130], 2, "half way");
    assert_eq!(image.at(72, 8), [255, 255, 255, 255]);
}

#[test]
fn stacked_gradients_composite_in_order_and_follow_a_rounded_box() {
    let rect = Rect::new(8.0, 8.0, 48.0, 48.0);
    let base = [GradientStop::new(0.0, BLUE), GradientStop::new(1.0, GREEN)];
    let glow = [
        GradientStop::new(0.0, Color::rgba(1.0, 1.0, 1.0, 0.5)),
        GradientStop::new(0.5, Color::rgba(1.0, 1.0, 1.0, 0.0)),
    ];
    let lower = RadialGradient {
        center: Point::new(32.0, 32.0),
        radius_x: 24.0,
        radius_y: 24.0,
        stops: &base,
    };
    let upper = RadialGradient {
        center: Point::new(24.0, 20.0),
        radius_x: 24.0,
        radius_y: 24.0,
        stops: &glow,
    };
    let mut list = DisplayList::new();
    list.fill_radial_gradient_rect(rect, 24.0, lower, 0.0);
    list.fill_radial_gradient_rect(rect, 24.0, upper, 0.0);
    let Some(image) = render(SurfaceSize::new(64, 64), BLACK, &list) else {
        return;
    };
    // The corners of the box are outside the circle.
    assert_eq!(image.at(9, 9), [0, 0, 0, 255]);
    assert_eq!(image.at(54, 54), [0, 0, 0, 255]);
    // At the glow's centre: half white over the lower gradient there.
    let under = over(radial_at(&lower, 24.5, 20.5), [0.0; 3]);
    let top = radial_at(&upper, 24.5, 20.5);
    let expected = over(top, std::array::from_fn(|i| under[i] as f32 / 255.0));
    assert_close(image.at(24, 20), expected, 3, "glow over base");
}

#[test]
fn a_gradient_moves_and_turns_with_its_transform() {
    let stops = [GradientStop::new(0.0, RED), GradientStop::new(1.0, BLUE)];
    let gradient = RadialGradient {
        center: Point::new(8.0, 8.0),
        radius_x: 16.0,
        radius_y: 4.0,
        stops: &stops,
    };
    let mut list = DisplayList::new();
    // A quarter turn about the centre of the 32 x 32 surface, then the box.
    list.push_transform(
        Transform::translate(16.0, 16.0)
            .and_rotate(90.0)
            .and_translate(-16.0, -16.0),
    );
    list.fill_radial_gradient_rect(Rect::new(0.0, 0.0, 32.0, 16.0), 0.0, gradient, 0.0);
    list.pop_transform();
    let Some(image) = render(SurfaceSize::new(32, 32), BLACK, &list) else {
        return;
    };
    // (8, 8) turns to (24, 8); the ellipse's long axis is now vertical.
    // That pixel's centre is (8.5, 7.5) in the box: 0.129 of the way out.
    assert_close(image.at(24, 8), [222, 0, 33], 3, "centre");
    let along = image.at(24, 14);
    let across = image.at(26, 8);
    assert!(
        along[0] > across[0],
        "long axis is vertical: {along:?} {across:?}"
    );
    // The left half of the surface is where the box is not.
    assert_eq!(image.at(4, 16), [0, 0, 0, 255]);
}

/// Brute-force Gaussian blur of `picture` (premultiplied, 0..=1) at one
/// pixel.
fn blurred(picture: &dyn Fn(f32, f32) -> [f32; 4], x: f32, y: f32, sigma: f32) -> [f32; 4] {
    let reach = (4.0 * sigma).ceil() as i32;
    let mut sum = [0.0f32; 4];
    let mut total = 0.0;
    // Four samples per pixel each way.
    for j in -reach * 4..=reach * 4 {
        for i in -reach * 4..=reach * 4 {
            let (dx, dy) = (i as f32 / 4.0, j as f32 / 4.0);
            let weight = (-(dx * dx + dy * dy) / (2.0 * sigma * sigma)).exp();
            let value = picture(x + dx, y + dy);
            for channel in 0..4 {
                sum[channel] += value[channel] * weight;
            }
            total += weight;
        }
    }
    sum.map(|value| value / total)
}

#[test]
fn a_blurred_glow_matches_a_gaussian_blur_of_the_gradient() {
    // The orb's glow in miniature: an ellipse whose colour fades to nothing
    // just inside its box, blurred by a sigma of 3.
    let rect = Rect::new(16.0, 18.0, 48.0, 14.0);
    let stops = [
        GradientStop::new(0.0, Color::rgba(1.0, 0.0, 1.0, 0.5)),
        GradientStop::new(0.7, Color::rgba(1.0, 0.0, 1.0, 0.0)),
    ];
    let gradient = RadialGradient {
        center: Point::new(40.0, 25.0),
        radius_x: 24.0 * std::f32::consts::SQRT_2,
        radius_y: 7.0 * std::f32::consts::SQRT_2,
        stops: &stops,
    };
    let sigma = 3.0;
    let mut list = DisplayList::new();
    list.fill_radial_gradient_rect(rect, 7.0, gradient, sigma);
    assert!(matches!(
        list.ops()[0],
        DrawOp::RadialGradientRect { blur, .. } if blur == sigma
    ));
    let Some(image) = render(SurfaceSize::new(80, 50), WHITE, &list) else {
        return;
    };
    let picture = |x: f32, y: f32| {
        let inside = (16.0..64.0).contains(&x) && (18.0..32.0).contains(&y);
        if inside {
            radial_at(&gradient, x, y)
        } else {
            [0.0; 4]
        }
    };
    let mut worst = 0u8;
    for y in (1..50).step_by(3) {
        for x in (1..80).step_by(3) {
            let expected = over(
                blurred(&picture, x as f32 + 0.5, y as f32 + 0.5, sigma),
                [1.0; 3],
            );
            let actual = image.at(x, y);
            for channel in 0..3 {
                worst = worst.max(actual[channel].abs_diff(expected[channel]));
            }
        }
    }
    // The quadrature is good to about 1% of the glow's strength (ADR 0034).
    assert!(worst <= 4, "worst channel difference {worst}");
    // The blur spreads beyond the box and reaches nothing far from it.
    assert_ne!(image.at(40, 15), [255, 255, 255, 255]);
    assert_eq!(image.at(40, 2), [255, 255, 255, 255]);
    assert_eq!(image.at(2, 25), [255, 255, 255, 255]);
}

/// Coverage of a rounded box, for the references below.
fn in_rounded(x: f32, y: f32, rect: Rect, radius: f32) -> bool {
    let (hx, hy) = (rect.width / 2.0, rect.height / 2.0);
    let qx = (x - rect.x - hx).abs() - hx + radius;
    let qy = (y - rect.y - hy).abs() - hy + radius;
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius <= 0.0
}

#[test]
fn an_inner_shadow_is_the_blurred_outside_seen_through_the_box() {
    // CSS: box-shadow: inset 0 6px 8px white, on a black circle.
    let rect = Rect::new(8.0, 8.0, 48.0, 48.0);
    let shadow = Shadow::inset(0.0, 6.0, 8.0, WHITE);
    let mut list = DisplayList::new();
    list.fill_rounded_rect(rect, 24.0, BLACK);
    list.draw_inset_shadow(rect, 24.0, shadow);
    let Some(image) = render(SurfaceSize::new(64, 64), BLUE, &list) else {
        return;
    };
    let hole = |x: f32, y: f32| {
        let value = f32::from(in_rounded(x, y - 6.0, rect, 24.0));
        [value; 4]
    };
    let mut worst = 0u8;
    for y in (9..55).step_by(3) {
        for x in (9..55).step_by(3) {
            let (cx, cy) = (x as f32 + 0.5, y as f32 + 0.5);
            // Only pixels well inside the circle: its own edge is
            // anti-aliased against the blue.
            if !in_rounded(cx, cy, Rect::new(10.0, 10.0, 44.0, 44.0), 22.0) {
                continue;
            }
            let lit = 1.0 - blurred(&hole, cx, cy, 4.0)[0];
            let expected = (lit * 255.0).round() as u8;
            worst = worst.max(image.at(x, y)[0].abs_diff(expected));
        }
    }
    assert!(worst <= 4, "worst difference {worst}");
    // Light at the top edge, none at the bottom, nothing outside the box.
    assert!(image.at(32, 10)[0] > 150, "{:?}", image.at(32, 10));
    assert!(image.at(32, 52)[0] < 8, "{:?}", image.at(32, 52));
    assert_eq!(image.at(2, 2), [0, 0, 255, 255]);
    assert_eq!(image.at(32, 60), [0, 0, 255, 255]);
}

#[test]
fn a_sharp_inner_shadow_with_spread_is_a_ring() {
    // CSS: box-shadow: inset 0 0 0 2px red.
    let rect = Rect::new(4.0, 4.0, 24.0, 16.0);
    let mut list = DisplayList::new();
    list.fill_rect(rect, BLACK);
    list.draw_inset_shadow(
        rect,
        0.0,
        Shadow::inset(0.0, 0.0, 0.0, RED).with_spread(2.0),
    );
    let Some(image) = render(SurfaceSize::new(32, 24), BLUE, &list) else {
        return;
    };
    for y in 0..24 {
        for x in 0..32 {
            let in_box = (4..28).contains(&x) && (4..20).contains(&y);
            let in_hole = (6..26).contains(&x) && (6..18).contains(&y);
            let expected = match (in_box, in_hole) {
                (false, _) => [0, 0, 255, 255],
                (true, false) => [255, 0, 0, 255],
                (true, true) => [0, 0, 0, 255],
            };
            assert_eq!(image.at(x, y), expected, "pixel {x},{y}");
        }
    }
}

#[test]
fn each_corner_has_its_own_radius() {
    // border-radius: 12px 0 6px 0, with a 2 px green border.
    let rect = Rect::new(4.0, 4.0, 40.0, 32.0);
    let radii = CornerRadii::new(12.0, 0.0, 6.0, 0.0);
    let mut list = DisplayList::new();
    list.fill_corners_rect(rect, radii, RED, Border::new(2.0, GREEN));
    let Some(image) = render(SurfaceSize::new(48, 40), BLUE, &list) else {
        return;
    };
    let blue = [0, 0, 255, 255];
    let green = [0, 255, 0, 255];
    // Square corners are full to the corner pixel; round ones are cut.
    assert_eq!(image.at(4, 4), blue, "top-left is round");
    assert_eq!(image.at(6, 6), blue, "top-left is round by 12");
    assert_eq!(image.at(43, 4), green, "top-right is square");
    assert_eq!(image.at(4, 35), green, "bottom-left is square");
    assert_eq!(image.at(43, 35), blue, "bottom-right is round");
    assert_eq!(image.at(41, 33), green, "bottom-right is round by only 6");
    // The border follows the curve, the fill is inside it.
    assert_eq!(image.at(24, 4), green);
    assert_eq!(image.at(24, 20), [255, 0, 0, 255]);
    let on_curve = image.at(8, 8);
    assert!(on_curve[1] > 200 && on_curve[0] < 60, "{on_curve:?}");
    // The area: a 40 x 32 box less the two cut corners.
    let painted = (0..40u32)
        .flat_map(|y| (0..48u32).map(move |x| (x, y)))
        .map(|(x, y)| f32::from(255 - image.at(x, y)[2]) / 255.0)
        .sum::<f32>();
    let cut = (1.0 - std::f32::consts::FRAC_PI_4) * (144.0 + 36.0);
    assert!((painted - (1280.0 - cut)).abs() < 2.0, "area {painted}");
}

#[test]
fn equal_radii_are_the_plain_rounded_rectangle() {
    let rect = Rect::new(3.0, 3.0, 26.0, 20.0);
    let mut plain = DisplayList::new();
    plain.fill_bordered_rect(rect, 7.0, RED, Border::new(2.0, GREEN));
    let mut cornered = DisplayList::new();
    cornered.fill_corners_rect(
        rect,
        CornerRadii::uniform(7.0),
        RED,
        Border::new(2.0, GREEN),
    );
    let size = SurfaceSize::new(32, 26);
    let (Some(a), Some(b)) = (render(size, BLUE, &plain), render(size, BLUE, &cornered)) else {
        return;
    };
    assert_eq!(a.pixels, b.pixels);
}

#[test]
fn nothing_is_drawn_for_gradients_and_shadows_that_cannot_show() {
    let rect = Rect::new(0.0, 0.0, 8.0, 8.0);
    let stops = [GradientStop::new(0.0, RED), GradientStop::new(1.0, BLUE)];
    let mut list = DisplayList::new();
    fn gradient(radius_x: f32, stops: &[GradientStop]) -> RadialGradient<'_> {
        RadialGradient {
            center: Point::new(4.0, 4.0),
            radius_x,
            radius_y: 4.0,
            stops,
        }
    }
    let clear = [GradientStop::new(0.0, CLEAR), GradientStop::new(1.0, CLEAR)];
    list.fill_radial_gradient_rect(rect, 0.0, gradient(0.0, &stops), 0.0);
    list.fill_radial_gradient_rect(rect, 0.0, gradient(f32::NAN, &stops), 0.0);
    list.fill_radial_gradient_rect(rect, 0.0, gradient(4.0, &[]), 0.0);
    list.fill_radial_gradient_rect(Rect::default(), 0.0, gradient(4.0, &stops), 0.0);
    list.fill_radial_gradient_rect(rect, 0.0, gradient(4.0, &clear), 0.0);
    list.draw_inset_shadow(rect, 0.0, Shadow::inset(0.0, 0.0, 4.0, CLEAR));
    list.fill_corners_rect(
        rect,
        CornerRadii::uniform(2.0),
        CLEAR,
        Border::new(0.0, RED),
    );
    assert!(list.is_empty());
    // One stop is that colour throughout.
    list.fill_radial_gradient_rect(rect, 0.0, gradient(4.0, &stops[..1]), 0.0);
    assert_eq!(list.len(), 1);
}
