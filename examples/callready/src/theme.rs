//! CallReady's design tokens, taken from the design's HTML: colours and
//! type.
//!
//! Every value here is a number that appears in `design/html/*.html`.
//! Screens use these names, never raw hex values, so a colour can be traced
//! back to the design and changed in one place.

use std::borrow::Cow;

use morningstar::ui::prelude::*;

// ---------------------------------------------------------------- colours

/// The app's background, and text and shapes on a coloured plate.
pub const BLACK: Color = hex(0x0E0E0E);
/// Main text, strokes and doodles.
pub const WHITE: Color = hex(0xFFFFFF);

/// The accent (`{{accent}}` in the design, at its default): primary
/// buttons, the disc, handwritten notes, selected things.
pub const YELLOW: Color = hex(0xF6C445);
/// Red: the heart, the "C" plate, the end-call button.
pub const RED: Color = hex(0xE63946);
/// Purple: the first scenario, the "L" plate, the empathy bar.
pub const PURPLE: Color = hex(0x9B5DE5);
/// Teal: the "S" plate, "did well".
pub const TEAL: Color = hex(0x4ECDC4);
/// Blue: the second scenario, the left rings, the listening bar.
pub const BLUE: Color = hex(0x5AA9E6);
/// Green: the right rings, the tick after a finished line.
pub const GREEN: Color = hex(0x4CAF50);
/// Cream: the label in the middle of the disc.
pub const CREAM: Color = hex(0xE9E2CF);

/// Secondary text: quotes, leads, plan names.
pub const SILVER: Color = hex(0xC7C7CC);
/// Muted text: captions and small-caps labels.
pub const MUTED: Color = hex(0xA1A1A6);
/// Quiet icons and the blank the speaker has still to fill.
pub const GREY: Color = hex(0x8E8E93);
/// A line already spoken.
pub const DIM: Color = hex(0x6E6E73);

/// Raised surfaces: the tab bar, the call bar.
pub const SURFACE: Color = hex(0x1C1C1E);
/// The avatar's fill.
pub const AVATAR: Color = hex(0x2A2A2E);
/// Progress tracks and hairlines.
pub const TRACK: Color = hex(0x2C2C2E);
/// Quiet buttons and outlines: mute, "Transcript", the free plan.
pub const QUIET: Color = hex(0x3A3A3C);

// ------------------------------------------------------------------- type

/// The body typeface, in four weights.
pub const OUTFIT: FontFamily = FontFamily::named("Outfit");
/// The handwriting.
pub const CAVEAT: FontFamily = FontFamily::named("Caveat");
/// The heavy grotesque of the cut-out letters.
pub const ARCHIVO_BLACK: FontFamily = FontFamily::named("Archivo Black");
/// The fat-face serif of the cut-out letters.
pub const ABRIL_FATFACE: FontFamily = FontFamily::named("Abril Fatface");
/// The condensed numerals of the scenario cards.
pub const ANTON: FontFamily = FontFamily::named("Anton");

/// The theme handed to the framework: the palette's roles on the dark
/// background, Outfit for everything the screens do not style themselves.
pub const THEME: Theme = Theme {
    colors: Colors {
        background: BLACK,
        surface: SURFACE,
        surface_variant: TRACK,
        primary: YELLOW,
        on_primary: BLACK,
        primary_container: AVATAR,
        text: WHITE,
        text_muted: MUTED,
        divider: TRACK,
        danger: RED,
        on_danger: WHITE,
        success: GREEN,
        pressed_overlay: with_alpha(WHITE, 0.08),
    },
    fonts: Fonts::new(OUTFIT, OUTFIT),
    ..Theme::LIGHT
};

/// Vertical metrics of a font file as fractions of the font size: ascent
/// and descent from its `hhea` table (`assets/fonts/README.md`).
#[derive(Clone, Copy, Debug)]
struct Metrics {
    ascent: f32,
    descent: f32,
}

const OUTFIT_METRICS: Metrics = Metrics {
    ascent: 1.0,
    descent: 0.26,
};
const CAVEAT_METRICS: Metrics = Metrics {
    ascent: 0.96,
    descent: 0.30,
};
const ARCHIVO_BLACK_METRICS: Metrics = Metrics {
    ascent: 0.878,
    descent: 0.21,
};
const ABRIL_FATFACE_METRICS: Metrics = Metrics {
    ascent: 1.058,
    descent: 0.291,
};
const ANTON_METRICS: Metrics = Metrics {
    ascent: 2409.0 / 2048.0,
    descent: 674.0 / 2048.0,
};

/// Pixels per dp of the design's reference renders.
const DESIGN_SCALE: f32 = 2.0;

/// The height of a line of text that has no `line-height` in the design
/// (CSS `line-height: normal`), in dp. The browser rounds the font's ascent
/// and descent to whole *device* pixels separately and adds them; the
/// references were rendered at 2 px per dp, so heights come in halves:
/// 15 px Outfit is 19, 17 px is 21.5.
fn normal_line_height(metrics: Metrics, size: f32) -> f32 {
    let pixels = size * DESIGN_SCALE;
    ((metrics.ascent * pixels).round() + (metrics.descent * pixels).round()) / DESIGN_SCALE
}

fn face(
    content: impl Into<Cow<'static, str>>,
    family: FontFamily,
    metrics: Metrics,
    size: f32,
) -> Text {
    text(content)
        .family(family)
        .size(size)
        .regular()
        .line_height_dp(normal_line_height(metrics, size))
        .color(WHITE)
}

/// Outfit, regular, white, at `size` dp with the browser's default line
/// height. Chain `.medium()`, `.semibold()`, `.bold()`, `.color(SILVER)`,
/// and `.line_height_dp(size * 1.3)` where the design gives a
/// `line-height`.
pub fn outfit(content: impl Into<Cow<'static, str>>, size: f32) -> Text {
    face(content, OUTFIT, OUTFIT_METRICS, size)
}

/// Like [`outfit`] for a paragraph with differently styled phrases in it:
/// `outfit_rich([span("Two weeks").color(YELLOW), span(" and still nothing.")], 27.0)`.
pub fn outfit_rich<S: Into<Span>>(spans: impl IntoIterator<Item = S>, size: f32) -> Text {
    Text::rich(spans)
        .family(OUTFIT)
        .size(size)
        .regular()
        .line_height_dp(normal_line_height(OUTFIT_METRICS, size))
        .color(WHITE)
}

/// Caveat: the handwritten notes. The design declares the family at 600 and
/// 700 only and asks for the default weight, for which a browser picks 600;
/// the one face bundled is that one.
pub fn caveat(content: impl Into<Cow<'static, str>>, size: f32) -> Text {
    face(content, CAVEAT, CAVEAT_METRICS, size).semibold()
}

/// Archivo Black: cut-out letters and shouted words.
pub fn archivo_black(content: impl Into<Cow<'static, str>>, size: f32) -> Text {
    face(content, ARCHIVO_BLACK, ARCHIVO_BLACK_METRICS, size)
}

/// Abril Fatface: cut-out letters and shouted words.
pub fn abril_fatface(content: impl Into<Cow<'static, str>>, size: f32) -> Text {
    face(content, ABRIL_FATFACE, ABRIL_FATFACE_METRICS, size)
}

/// Anton: the scenario numerals.
pub fn anton(content: impl Into<Cow<'static, str>>, size: f32) -> Text {
    face(content, ANTON, ANTON_METRICS, size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_line_heights_are_the_browsers() {
        let outfit = |size| normal_line_height(OUTFIT_METRICS, size);
        assert_eq!(
            [14.0, 15.0, 16.0, 17.0, 22.0].map(outfit),
            [17.5, 19.0, 20.0, 21.5, 27.5]
        );
    }
}
