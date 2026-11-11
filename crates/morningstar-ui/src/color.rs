//! Colours as designs give them: sRGB hex values and theme tokens.

use morningstar_core::Color;

use crate::theme::Theme;

/// An opaque colour from a design-tool hex value, `0xRRGGBB`.
///
/// Design tools and CSS give colours as sRGB-encoded bytes; the renderer
/// works in linear light. This converts, so `hex(0x1A73E8)` on screen is the
/// colour the design shows. It is a `const fn`: palettes can be constants.
///
/// ```
/// use morningstar_ui::{Color, hex};
/// const BRAND: Color = hex(0x1A73E8);
/// assert_eq!(hex(0xFFFFFF), Color::WHITE);
/// assert_eq!(hex(0x000000), Color::BLACK);
/// assert!(BRAND.b > BRAND.r);
/// ```
pub const fn hex(rgb: u32) -> Color {
    rgb8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

/// A colour with alpha from `0xRRGGBBAA`, as design tools write 8-digit hex.
pub const fn hexa(rgba: u32) -> Color {
    let color = hex(rgba >> 8);
    with_alpha(color, (rgba & 0xFF) as f32 / 255.0)
}

/// An opaque colour from sRGB-encoded bytes, as in CSS `rgb(26, 115, 232)`.
pub const fn rgb8(red: u8, green: u8, blue: u8) -> Color {
    Color::rgb(
        srgb_to_linear(red),
        srgb_to_linear(green),
        srgb_to_linear(blue),
    )
}

/// The same colour with its opacity replaced by `alpha` (`0.0..=1.0`).
pub const fn with_alpha(color: Color, alpha: f32) -> Color {
    Color::rgba(color.r, color.g, color.b, alpha)
}

/// The sRGB transfer function, usable in constants.
const fn srgb_to_linear(value: u8) -> f32 {
    let encoded = value as f64 / 255.0;
    if encoded <= 0.040_45 {
        (encoded / 12.92) as f32
    } else {
        // x^2.4 = x^2 * (x^(1/5))^2, with the fifth root by Newton's method
        // (`powf` is not available in constant evaluation). Starting from 1,
        // above the root, the iteration decreases monotonically to it.
        let x = (encoded + 0.055) / 1.055;
        let mut root = 1.0_f64;
        let mut step = 0;
        while step < 48 {
            let fourth = root * root * root * root;
            root -= (fourth * root - x) / (5.0 * fourth);
            step += 1;
        }
        (x * x * root * root) as f32
    }
}

/// A colour named by its role in the [`Theme`] rather than by value.
///
/// Every method that takes a colour accepts either a [`Color`] or one of
/// these, so a screen written with tokens follows the theme:
///
/// ```
/// use morningstar_ui::prelude::*;
/// let label: Text = text("Balance").color(ThemeColor::TextMuted);
/// let brand: Text = text("M-Pesa").color(hex(0x00A651));
/// # let _ = (label, brand);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ThemeColor {
    /// The screen background.
    Background,
    /// Cards, sheets, bars: surfaces that sit on the background.
    Surface,
    /// A quieter surface: input fields, chips, secondary buttons.
    SurfaceVariant,
    /// The brand or accent colour.
    Primary,
    /// Text and icons drawn on [`Primary`](Self::Primary).
    OnPrimary,
    /// A tint of the primary colour for soft highlights.
    PrimaryContainer,
    /// Main text.
    Text,
    /// Secondary text: captions, hints, metadata.
    TextMuted,
    /// Hairlines and outlines.
    Divider,
    /// Destructive actions and errors.
    Danger,
    /// Text and icons drawn on [`Danger`](Self::Danger).
    OnDanger,
    /// Positive amounts and confirmations.
    Success,
}

/// Either a literal [`Color`] or a [`ThemeColor`] token. Colour parameters
/// are `impl Into<ColorRef>`; application code rarely names this type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ColorRef {
    /// A literal colour.
    Value(Color),
    /// A colour looked up in the theme when the frame is built.
    Token(ThemeColor),
}

impl ColorRef {
    /// The colour this refers to under `theme`.
    pub fn resolve(self, theme: &Theme) -> Color {
        match self {
            Self::Value(color) => color,
            Self::Token(token) => theme.color(token),
        }
    }
}

impl From<Color> for ColorRef {
    fn from(color: Color) -> Self {
        Self::Value(color)
    }
}

impl From<ThemeColor> for ColorRef {
    fn from(token: ThemeColor) -> Self {
        Self::Token(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn const_transfer_function_matches_powf() {
        for value in 0..=255_u8 {
            let encoded = f32::from(value) / 255.0;
            let expected = if encoded <= 0.040_45 {
                encoded / 12.92
            } else {
                ((encoded + 0.055) / 1.055).powf(2.4)
            };
            let actual = srgb_to_linear(value);
            assert!(
                (actual - expected).abs() < 1e-6,
                "{value}: {actual} vs {expected}"
            );
        }
    }

    #[test]
    fn hex_orders_channels_and_reads_alpha() {
        let color = hex(0xFF0000);
        assert_eq!((color.r, color.g, color.b, color.a), (1.0, 0.0, 0.0, 1.0));
        let color = hexa(0x0000FF80);
        assert_eq!((color.r, color.g, color.b), (0.0, 0.0, 1.0));
        assert!((color.a - 128.0 / 255.0).abs() < 1e-6);
        // sRGB mid grey is about 21.6% linear.
        assert!((hex(0x808080).r - 0.2158).abs() < 1e-3);
    }
}
