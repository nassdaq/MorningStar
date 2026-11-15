//! Barua Talk's design tokens, taken from the design's HTML: colours,
//! radii and type.
//!
//! Every value here is a number that appears in `design/html/*.html`.
//! Screens use these names, never raw values, so a colour can be traced
//! back to the design and changed in one place.

use std::borrow::Cow;

use morningstar::ui::prelude::*;

// ---------------------------------------------------------------- colours

/// The page: `background: #0D0F15`.
pub const PAGE: Color = hex(0x0D0F15);
/// Raised surfaces: bubbles, the tab track, round buttons, the card of the
/// phone call (`#171A24`).
pub const SURFACE: Color = hex(0x171A24);
/// Main text and icons: `color: #F4F5F8`.
pub const INK: Color = hex(0xF4F5F8);
/// Secondary text: `#9AA1B2`.
pub const MUTED: Color = hex(0x9AA1B2);
/// Labels under round things and quiet buttons: `#C9CEDA`.
pub const SOFT: Color = hex(0xC9CEDA);
/// Text over a photograph: `#E3E6EE`.
pub const OVER_PHOTO: Color = hex(0xE3E6EE);
/// Pure white: text on the accent and on the call screens.
pub const WHITE: Color = hex(0xFFFFFF);
/// Outlines of round buttons, fields and pills: `#2A2F3D`.
pub const LINE: Color = hex(0x2A2F3D);
/// Hairlines under a header and over a bottom bar: `#1F2330`.
pub const HAIRLINE: Color = hex(0x1F2330);
/// The dashed ring of "New room", a room that is not live, the unplayed
/// part of a voice message: `#4A5166`.
pub const QUIET: Color = hex(0x4A5166);
/// The accent (`{{accent}}`, `#2F6BFF` by default): unread counts, the
/// ring of a live room, the user's bubble, send and play.
pub const ACCENT: Color = hex(0x2F6BFF);
/// Decline and hang up: `#D92D4B`.
pub const DANGER: Color = hex(0xD92D4B);
/// Accept: `#0B7A55`.
pub const ACCEPT: Color = hex(0x0B7A55);
/// The recording dot: `#FF5A72`.
pub const RECORDING: Color = hex(0xFF5A72);

/// `rgba(13,15,21, a)`: the page colour laid over a photograph.
const fn veil(alpha: f32) -> Color {
    with_alpha(PAGE, alpha)
}
/// The pill over the incoming call and round buttons over video: `0.55`.
pub const VEIL_55: Color = veil(0.55);
/// "Reply with a message instead": `0.6`.
pub const VEIL_60: Color = veil(0.6);
/// The recording pill: `0.7`.
pub const VEIL_70: Color = veil(0.7);

// ------------------------------------------------------------------ radii

/// A bubble's round corners: `20px`.
pub const RADIUS_BUBBLE: f32 = 20.0;
/// The corner of a bubble that points at its author: `6px`.
pub const RADIUS_TAIL: f32 = 6.0;
/// Cards and the group picture: `18px` and `20px`.
pub const RADIUS_CARD: f32 = 20.0;

// ------------------------------------------------------------------- type

/// Running text: Inter in three weights.
pub const INTER: FontFamily = FontFamily::named("Inter");
/// Names and the call timer: Space Grotesk in two weights.
pub const GROTESK: FontFamily = FontFamily::named("Space Grotesk");

/// The theme handed to the framework: the palette's roles on the dark
/// page, so the system bars get light icons and the caret and selection
/// the accent.
pub const THEME: Theme = Theme {
    colors: Colors {
        background: PAGE,
        surface: SURFACE,
        surface_variant: SURFACE,
        primary: ACCENT,
        on_primary: WHITE,
        primary_container: SURFACE,
        text: INK,
        text_muted: MUTED,
        divider: HAIRLINE,
        danger: DANGER,
        on_danger: WHITE,
        success: ACCEPT,
        pressed_overlay: with_alpha(WHITE, 0.06),
    },
    fonts: Fonts::new(INTER, GROTESK),
    ..Theme::DARK
};

/// `hhea` ascent and descent as fractions of the font size
/// (`assets/fonts/README.md`).
const INTER_METRICS: (f32, f32) = (1984.0 / 2048.0, 494.0 / 2048.0);
const GROTESK_METRICS: (f32, f32) = (0.984, 0.292);

/// Pixels per dp of the design's reference renders.
const DESIGN_SCALE: f32 = 2.0;

/// The height of a line of text that has no `line-height` in the design
/// (CSS `line-height: normal`), in dp. The browser rounds the font's ascent
/// and descent to whole *device* pixels separately and adds them; the
/// references were rendered at 2 px per dp, so heights come in halves:
/// 12 px Inter is 14.5, 13 is 15.5, 16 is 19.5 (`design/layout/*.txt`).
fn normal_line_height(metrics: (f32, f32), size: f32) -> f32 {
    let pixels = size * DESIGN_SCALE;
    ((metrics.0 * pixels).round() + (metrics.1 * pixels).round()) / DESIGN_SCALE
}

/// How far below the top of a line of Inter at `larger` px a line at
/// `smaller` px starts when the two share a baseline (CSS `align-items:
/// baseline`): the difference of their ascents, each rounded to a device
/// pixel as the browser does. The layout engine has no baseline alignment,
/// so a row of two sizes is aligned at the top and the smaller text is
/// moved down by this.
pub fn inter_baseline_drop(larger: f32, smaller: f32) -> f32 {
    let ascent = |size: f32| (INTER_METRICS.0 * size * DESIGN_SCALE).round() / DESIGN_SCALE;
    ascent(larger) - ascent(smaller)
}

/// Inter's default line height at `size`.
pub fn inter_line(size: f32) -> f32 {
    normal_line_height(INTER_METRICS, size)
}

/// Space Grotesk's default line height at `size`.
pub fn grotesk_line(size: f32) -> f32 {
    normal_line_height(GROTESK_METRICS, size)
}

/// Inter, regular, in the main text colour, at `size` dp with the
/// browser's default line height. Chain `.medium()`, `.semibold()`,
/// `.color(MUTED)`, and `.line_height_dp(20.0)` where the design gives a
/// `line-height`.
pub fn inter(content: impl Into<Cow<'static, str>>, size: f32) -> Text {
    text(content)
        .family(INTER)
        .size(size)
        .regular()
        .line_height_dp(inter_line(size))
        .color(INK)
}

/// Space Grotesk, semibold, in the main text colour, at `size` dp: a name.
pub fn grotesk(content: impl Into<Cow<'static, str>>, size: f32) -> Text {
    text(content)
        .family(GROTESK)
        .size(size)
        .semibold()
        .line_height_dp(grotesk_line(size))
        .color(INK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_line_heights_are_the_browsers() {
        // `design/layout/*.txt`: the heights of the line boxes of texts
        // with `line-height: normal`.
        assert_eq!(
            [11.0, 12.0, 13.0, 14.0, 15.0, 16.0].map(inter_line),
            [13.0, 14.5, 15.5, 17.0, 18.0, 19.5]
        );
        // "Live rooms" at 14 px beside 12 px, an author at 13 beside 12.
        assert_eq!(inter_baseline_drop(14.0, 12.0), 2.0);
        assert_eq!(inter_baseline_drop(13.0, 12.0), 1.0);
        assert_eq!(
            [17.0, 20.0, 28.0, 30.0, 44.0].map(grotesk_line),
            [21.5, 25.5, 35.5, 38.5, 56.5]
        );
    }
}
