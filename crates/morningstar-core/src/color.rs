/// A colour in linear RGB with straight (non-premultiplied) alpha.
///
/// Components are nominally in `0.0..=1.0`. The renderer is responsible for
/// encoding to the surface's transfer function.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const BLACK: Self = Self::rgb(0.0, 0.0, 0.0);
    pub const WHITE: Self = Self::rgb(1.0, 1.0, 1.0);

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Builds an opaque colour from hue (degrees, any range), saturation and
    /// value (both `0.0..=1.0`). The resulting components are used as linear
    /// RGB.
    pub fn from_hsv(hue_degrees: f32, saturation: f32, value: f32) -> Self {
        let h = hue_degrees.rem_euclid(360.0) / 60.0;
        let s = saturation.clamp(0.0, 1.0);
        let v = value.clamp(0.0, 1.0);

        let c = v * s;
        let x = c * (1.0 - (h % 2.0 - 1.0).abs());
        let m = v - c;

        let (r, g, b) = match h as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        Self::rgb(r + m, g + m, b + m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Color, b: Color) -> bool {
        let eq = |x: f32, y: f32| (x - y).abs() < 1e-5;
        eq(a.r, b.r) && eq(a.g, b.g) && eq(a.b, b.b) && eq(a.a, b.a)
    }

    #[test]
    fn hsv_primaries() {
        assert!(close(
            Color::from_hsv(0.0, 1.0, 1.0),
            Color::rgb(1.0, 0.0, 0.0)
        ));
        assert!(close(
            Color::from_hsv(120.0, 1.0, 1.0),
            Color::rgb(0.0, 1.0, 0.0)
        ));
        assert!(close(
            Color::from_hsv(240.0, 1.0, 1.0),
            Color::rgb(0.0, 0.0, 1.0)
        ));
    }

    #[test]
    fn hsv_wraps_and_clamps() {
        assert!(close(
            Color::from_hsv(360.0, 1.0, 1.0),
            Color::from_hsv(0.0, 1.0, 1.0)
        ));
        assert!(close(
            Color::from_hsv(-120.0, 1.0, 1.0),
            Color::from_hsv(240.0, 1.0, 1.0)
        ));
        assert!(close(Color::from_hsv(42.0, 0.0, 2.0), Color::WHITE));
    }
}
