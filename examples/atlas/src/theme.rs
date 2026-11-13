//! Atlas's design tokens, taken from the design's HTML: colours, radii,
//! shadows and type.
//!
//! Every value here is a number that appears in `design/html/*.html`.
//! Screens use these names, never raw values, so a colour can be traced
//! back to the design and changed in one place.

use std::borrow::Cow;

use morningstar::ui::prelude::*;

// ---------------------------------------------------------------- colours

/// The page: `background: #FBF8FC`.
pub const PAGE: Color = hex(0xFBF8FC);
/// Main text and icons, the selected chip, the "end voice" button:
/// `color: #1D1830`.
pub const INK: Color = hex(0x1D1830);
/// Secondary text and quiet icons: `#5E5873`.
pub const MUTED: Color = hex(0x5E5873);
/// `input::placeholder { color: #6F6985 }`.
pub const PLACEHOLDER: Color = hex(0x6F6985);
/// The accent: links (`a { color: #A0287A }`), tinted icons, ticks, the
/// selected plan's outline.
pub const ACCENT: Color = hex(0xA0287A);
/// Cards, fields, bars and text on the gradient: `#FFFFFF`.
pub const WHITE: Color = hex(0xFFFFFF);
/// Empty radio buttons and the dashed ring of a step not started:
/// `#8E88A3`.
pub const QUIET: Color = hex(0x8E88A3);

/// `linear-gradient(135deg, #C2338F, #8B3FD9)`: the primary button, the
/// send button, the microphone of live voice.
pub const BRAND_FROM: Color = hex(0xC2338F);
/// The gradient's second colour.
pub const BRAND_TO: Color = hex(0x8B3FD9);

/// `rgba(29,24,48, a)`: ink at the alphas the design draws lines and
/// quiet fills with.
const fn ink(alpha: f32) -> Color {
    with_alpha(INK, alpha)
}
/// The "+" button's disc: `rgba(29,24,48,0.05)`.
pub const INK_05: Color = ink(0.05);
/// Hairlines and the outline of cards and round buttons: `0.08`.
pub const LINE: Color = ink(0.08);
/// The outline of the composer: `0.10`.
pub const LINE_10: Color = ink(0.10);
/// The outline of the search field and of chips, the "or" rules: `0.12`.
pub const LINE_12: Color = ink(0.12);
/// The outline of a text field and of the Google button: `0.16`.
pub const LINE_16: Color = ink(0.16);
/// The sheet's grabber: `0.18`.
pub const GRABBER: Color = ink(0.18);
/// The outline of "Cancel task": `0.2`.
pub const LINE_20: Color = ink(0.2);
/// The scrim under the sheet: `rgba(29,24,48,0.42)`.
pub const SCRIM: Color = ink(0.42);

/// `rgba(160,40,122, a)`: the accent as a tint.
const fn accent(alpha: f32) -> Color {
    with_alpha(ACCENT, alpha)
}
/// The selected model's row and a tool's tile: `rgba(160,40,122,0.08)`.
pub const TINT_08: Color = accent(0.08);
/// The user's bubble: `0.09`.
pub const TINT_09: Color = accent(0.09);
/// The "Recommended" pill: `0.10`.
pub const TINT_10: Color = accent(0.10);
/// The selected tab's pill: `0.12`.
pub const TINT_12: Color = accent(0.12);
/// The spinner's track: `0.2`.
pub const TINT_20: Color = accent(0.2);

/// The wash behind the orb: `rgba(246,170,214, a)` fading to nothing.
pub const WASH: Color = hex(0xF6AAD6);
/// The glow under the orb: `rgba(214,92,190,0.5)` fading to nothing.
pub const GLOW: Color = hex(0xD65CBE);
/// The orb's drop shadow: `rgba(190,80,190,0.3)`.
pub const ORB_SHADOW: Color = with_alpha(hex(0xBE50BE), 0.3);
/// The orb's body, from its centre out: `#FFB8DE 0%, #F06CBC 45%,
/// #C94FCB 75%, #8E5BE8 100%`.
pub const ORB_BODY: [(f32, Color); 4] = [
    (0.0, hex(0xFFB8DE)),
    (0.45, hex(0xF06CBC)),
    (0.75, hex(0xC94FCB)),
    (1.0, hex(0x8E5BE8)),
];
/// The orb's violet reflection: `rgba(150,140,255,0.8)`.
pub const ORB_VIOLET: Color = hex(0x968CFF);
/// The orb's cyan reflection: `rgba(120,232,236,0.95)`.
pub const ORB_CYAN: Color = hex(0x78E8EC);

// ------------------------------------------------------------------ radii

/// Rows, text fields and the selected model: `border-radius: 16px`.
pub const RADIUS_ROW: f32 = 16.0;
/// The step card of a chat: `18px`.
pub const RADIUS_STEPS: f32 = 18.0;
/// List cards and the user's bubble: `20px`.
pub const RADIUS_CARD: f32 = 20.0;
/// The plan card: `24px`.
pub const RADIUS_PLAN: f32 = 24.0;
/// The sheet's top corners and the composer: `28px`.
pub const RADIUS_SHEET: f32 = 28.0;
/// A tool's tile: `12px`.
pub const RADIUS_TILE: f32 = 12.0;
/// The corner of the user's bubble that points at them: `6px`.
pub const RADIUS_TAIL: f32 = 6.0;

// ---------------------------------------------------------------- shadows

/// The composer: `box-shadow: 0 8px 24px rgba(120,80,160,0.10)`.
pub const COMPOSER_SHADOW: Shadow = Shadow::new(0.0, 8.0, 24.0, with_alpha(hex(0x7850A0), 0.10));
/// The sheet: `box-shadow: 0 -10px 40px rgba(29,24,48,0.18)`.
pub const SHEET_SHADOW: Shadow = Shadow::new(0.0, -10.0, 40.0, ink(0.18));

// ------------------------------------------------------------------- type

/// The one typeface, in four weights.
pub const JAKARTA: FontFamily = FontFamily::named("Plus Jakarta Sans");

/// The theme handed to the framework: the palette's roles on the light
/// page, so the system bars get dark icons and the caret and selection
/// the accent.
pub const THEME: Theme = Theme {
    colors: Colors {
        background: PAGE,
        surface: WHITE,
        primary: ACCENT,
        on_primary: WHITE,
        primary_container: TINT_08,
        text: INK,
        text_muted: MUTED,
        divider: LINE,
        pressed_overlay: INK_05,
        ..Theme::LIGHT.colors
    },
    fonts: Fonts::new(JAKARTA, JAKARTA),
    ..Theme::LIGHT
};

/// Plus Jakarta Sans's `hhea` ascent and descent as fractions of the font
/// size (`assets/fonts/README.md`).
const ASCENT: f32 = 1.038;
const DESCENT: f32 = 0.222;

/// Pixels per dp of the design's reference renders.
const DESIGN_SCALE: f32 = 2.0;

/// The height of a line of text that has no `line-height` in the design
/// (CSS `line-height: normal`), in dp. The browser rounds the font's ascent
/// and descent to whole *device* pixels separately and adds them; the
/// references were rendered at 2 px per dp, so heights come in halves:
/// 13 px is 16.5, 15 px is 19, 18 px is 22.5.
pub fn normal_line_height(size: f32) -> f32 {
    let pixels = size * DESIGN_SCALE;
    ((ASCENT * pixels).round() + (DESCENT * pixels).round()) / DESIGN_SCALE
}

/// Plus Jakarta Sans, regular, ink, at `size` dp with the browser's
/// default line height. Chain `.medium()`, `.semibold()`, `.bold()`,
/// `.color(MUTED)`, and `.line_height_dp(size * 1.5)` where the design
/// gives a `line-height`.
pub fn jakarta(content: impl Into<Cow<'static, str>>, size: f32) -> Text {
    text(content)
        .family(JAKARTA)
        .size(size)
        .regular()
        .line_height_dp(normal_line_height(size))
        .color(INK)
}

/// Like [`jakarta`] for a paragraph with differently styled phrases.
pub fn jakarta_rich<S: Into<Span>>(spans: impl IntoIterator<Item = S>, size: f32) -> Text {
    Text::rich(spans)
        .family(JAKARTA)
        .size(size)
        .regular()
        .line_height_dp(normal_line_height(size))
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
            [12.0, 13.0, 14.0, 15.0, 16.0, 18.0, 20.0, 28.0, 30.0].map(normal_line_height),
            [15.0, 16.5, 17.5, 19.0, 20.0, 22.5, 25.5, 35.0, 37.5]
        );
    }
}
