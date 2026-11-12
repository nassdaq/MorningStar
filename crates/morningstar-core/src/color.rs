/// A colour in linear RGB with straight (non-premultiplied) alpha.
///
/// Components are nominally in `0.0..=1.0`. The renderer encodes them with
/// the sRGB transfer function ([`srgb`](Self::srgb)) and composites the
/// encoded values, as browsers and design tools do (ADR 0023): a colour
/// with alpha 0.25 over black shows a quarter of its *encoded* value.
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

    /// The colour as the renderer composites it: red, green and blue
    /// encoded with the sRGB transfer function, and alpha as it is (not
    /// premultiplied). Components outside `0..=1` are clamped; NaN encodes
    /// as 0. A colour made from a design's `#RRGGBB` gives those bytes
    /// back, over 255.
    pub fn srgb(self) -> [f32; 4] {
        let encode = |linear: f32| {
            if linear.is_nan() || linear <= 0.0 {
                0.0
            } else if linear >= 1.0 {
                1.0
            } else if linear <= 0.003_130_8 {
                linear * 12.92
            } else {
                1.055 * linear.powf(1.0 / 2.4) - 0.055
            }
        };
        [
            encode(self.r),
            encode(self.g),
            encode(self.b),
            if self.a.is_nan() {
                0.0
            } else {
                self.a.clamp(0.0, 1.0)
            },
        ]
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
    fn srgb_encodes_the_colour_and_leaves_alpha() {
        assert_eq!(
            Color::rgba(0.0, 1.0, 0.0, 0.25).srgb(),
            [0.0, 1.0, 0.0, 0.25]
        );
        // Linear 0.2158 is byte 128; the darkest values are on the straight
        // part of the curve.
        let [r, g, b, _] = Color::rgb(0.215_86, 0.001, 0.5).srgb();
        assert!((r * 255.0 - 128.0).abs() < 0.01, "{r}");
        assert!((g - 0.012_92).abs() < 1e-6, "{g}");
        assert!((b * 255.0 - 187.5).abs() < 0.1, "{b}");
        // Every byte of a design's colour survives the round trip.
        for byte in 0..=255u8 {
            let encoded = f32::from(byte) / 255.0;
            let linear = if encoded <= 0.040_45 {
                encoded / 12.92
            } else {
                ((encoded + 0.055) / 1.055).powf(2.4)
            };
            let [back, ..] = Color::rgb(linear, 0.0, 0.0).srgb();
            assert_eq!((back * 255.0).round() as u8, byte);
        }
        assert_eq!(
            Color::rgba(f32::NAN, -1.0, 7.0, f32::NAN).srgb(),
            [0.0, 0.0, 1.0, 0.0]
        );
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
