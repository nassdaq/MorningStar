//! FundiDesk's design tokens, taken from the design's HTML: colours, type,
//! radii, spacing and shadows.
//!
//! Every value here is a number that appears in
//! `design/html/*.html`. Screens use these names, never raw hex values, so a
//! colour can be traced back to the design and changed in one place.
//!
//! **Frozen for the parallel phase.** Screens are written against this
//! file; see `SCREENS.md` for how to ask for an addition.

use std::borrow::Cow;

use morningstar::ui::prelude::*;

// ---------------------------------------------------------------- colours

/// Brand green: primary buttons, links, the logo plate, selected outlines.
pub const GREEN: Color = hex(0x1E8416);
/// Darker green: text on a green tint, inner tiles of the green hero card.
pub const GREEN_DARK: Color = hex(0x176611);
/// Pale green: selected chips, success pills, soft panels.
pub const GREEN_TINT: Color = hex(0xE9F6E7);
/// A lighter green bar inside the onboarding illustration.
pub const GREEN_SOFT: Color = hex(0xB9DDB5);

/// Main text and dark shapes.
pub const INK: Color = hex(0x14201A);
/// Secondary text, quiet icons.
pub const MUTED: Color = hex(0x5B6660);
/// Placeholder text in fields (`input::placeholder`).
pub const PLACEHOLDER: Color = hex(0x6B756F);
/// Text of the neutral ("Received") pill.
pub const CHARCOAL: Color = hex(0x3F4A44);

/// White: cards, fields, bars, text on green.
pub const WHITE: Color = hex(0xFFFFFF);
/// The app's background on dashboard-like screens.
pub const CANVAS: Color = hex(0xF6F8F6);
/// A quiet grey panel: the fingerprint button, neutral stat tiles.
pub const PANEL: Color = hex(0xF3F5F3);
/// Background of the neutral pill.
pub const NEUTRAL_TINT: Color = hex(0xEDF0EE);

/// Outlines of fields, chips and quiet buttons; the timeline's line.
pub const OUTLINE: Color = hex(0xD9E0DB);
/// Card borders and hairlines between sections.
pub const HAIRLINE: Color = hex(0xE3E8E4);
/// Dashed outlines and the rings of steps not reached yet.
pub const DASH: Color = hex(0xAEB9B1);
/// Inactive page dots.
pub const DOT: Color = hex(0xC9D3CC);

/// Informational blue: "in repair", "diagnosing", the current step.
pub const BLUE: Color = hex(0x1D4ED8);
/// Pale blue behind blue text.
pub const BLUE_TINT: Color = hex(0xE6EEFF);
/// Amber text on its tint: "awaiting approval".
pub const AMBER: Color = hex(0x8A5A00);
/// Darker amber for amounts on the amber tint.
pub const AMBER_DARK: Color = hex(0x6B4500);
/// Pale amber: balances due.
pub const AMBER_TINT: Color = hex(0xFFF1D6);
/// Red: delays.
pub const RED: Color = hex(0xB42318);
/// Pale red behind a red icon.
pub const RED_TINT: Color = hex(0xFDECEA);

/// `box-shadow: 0 12px 28px rgba(20, 32, 26, 0.14)`: the floating card of
/// the onboarding illustration.
pub const SHADOW_CARD: Shadow = Shadow::new(0.0, 12.0, 28.0, with_alpha(INK, 0.14));
/// `box-shadow: 0 10px 22px rgba(30, 132, 22, 0.3)`: the floating green
/// tile of the onboarding illustration.
pub const SHADOW_GREEN: Shadow = Shadow::new(0.0, 10.0, 22.0, with_alpha(GREEN, 0.3));

// ------------------------------------------------------------------ radii

/// Corner radii the design uses, in dp.
pub mod radius {
    /// Progress segments and thin bars.
    pub const BAR: f32 = 4.0;
    /// Small panels inside the onboarding illustration.
    pub const SMALL: f32 = 10.0;
    /// Fields, quiet buttons, icon tiles, banners.
    pub const FIELD: f32 = 12.0;
    /// Primary buttons, stat tiles.
    pub const BUTTON: f32 = 14.0;
    /// Cards and list rows.
    pub const CARD: f32 = 16.0;
    /// The dashboard's hero card, the tracking page's status card.
    pub const HERO: f32 = 20.0;
    /// Pills and chips (`border-radius: 999px`), circles (`50%`).
    pub const PILL: f32 = 999.0;
}

// ---------------------------------------------------------------- spacing

/// Recurring distances, in dp.
pub mod space {
    /// Side margin of the white form screens (`padding: .. 24px ..`).
    pub const FORM_MARGIN: f32 = 24.0;
    /// Side margin of the grey list screens (`padding: .. 20px ..`).
    pub const PAGE_MARGIN: f32 = 20.0;
    /// Padding inside a card.
    pub const CARD: f32 = 16.0;
    /// Between a field's label and the field.
    pub const LABEL_GAP: f32 = 8.0;
    /// Between cards in a list.
    pub const CARD_GAP: f32 = 12.0;
    /// Height of a text field.
    pub const FIELD_HEIGHT: f32 = 52.0;
    /// Height of a primary button.
    pub const BUTTON_HEIGHT: f32 = 54.0;
    /// Side of a tappable icon button.
    pub const TOUCH: f32 = 44.0;
}

// ------------------------------------------------------------------- type

/// The body typeface.
pub const INTER: FontFamily = FontFamily::named("Inter");
/// The heading and numeral typeface.
pub const GROTESK: FontFamily = FontFamily::named("Space Grotesk");

/// The theme handed to the framework: the palette's roles, and the two
/// families as body and heading fonts.
pub const THEME: Theme = Theme {
    colors: Colors {
        background: CANVAS,
        surface: WHITE,
        surface_variant: PANEL,
        primary: GREEN,
        on_primary: WHITE,
        primary_container: GREEN_TINT,
        text: INK,
        text_muted: MUTED,
        divider: HAIRLINE,
        danger: RED,
        on_danger: WHITE,
        success: GREEN_DARK,
        pressed_overlay: with_alpha(INK, 0.08),
    },
    fonts: Fonts::new(INTER, GROTESK),
    ..Theme::LIGHT
};

/// Vertical metrics of a font file as fractions of the font size: ascent
/// and descent from its `hhea` table.
#[derive(Clone, Copy, Debug)]
struct Metrics {
    ascent: f32,
    descent: f32,
}

/// Inter: 1984 and 494 of 2048 units.
const INTER_METRICS: Metrics = Metrics {
    ascent: 1984.0 / 2048.0,
    descent: 494.0 / 2048.0,
};
/// Space Grotesk: 984 and 292 of 1000 units.
const GROTESK_METRICS: Metrics = Metrics {
    ascent: 0.984,
    descent: 0.292,
};

/// Pixels per dp of the design's reference renders.
const DESIGN_SCALE: f32 = 2.0;

/// The height of a line of text that has no `line-height` in the design
/// (CSS `line-height: normal`), in dp. The browser rounds the font's ascent
/// and descent to whole *device* pixels separately and adds them; the
/// references were rendered at 2 px per dp, so heights come in halves: 13 px
/// Inter is 15.5, 15 px is 18, 22 px Space Grotesk is 28.
fn normal_line_height(metrics: Metrics, size: f32) -> f32 {
    let pixels = size * DESIGN_SCALE;
    ((metrics.ascent * pixels).round() + (metrics.descent * pixels).round()) / DESIGN_SCALE
}

/// Inter, regular, in the ink colour, at `size` dp with the browser's
/// default line height. Chain `.medium()`, `.semibold()`, `.bold()`,
/// `.color(MUTED)`, and `.line_height_dp(22.0)` where the design gives a
/// `line-height`.
pub fn inter(content: impl Into<Cow<'static, str>>, size: f32) -> Text {
    text(content)
        .family(INTER)
        .size(size)
        .regular()
        .line_height_dp(normal_line_height(INTER_METRICS, size))
        .color(INK)
}

/// Like [`inter`] for a paragraph with differently styled phrases in it:
/// `inter_rich([span("New here? "), span("Join").semibold().color(GREEN)], 14.0)`.
pub fn inter_rich<S: Into<Span>>(spans: impl IntoIterator<Item = S>, size: f32) -> Text {
    Text::rich(spans)
        .family(INTER)
        .size(size)
        .regular()
        .line_height_dp(normal_line_height(INTER_METRICS, size))
        .color(INK)
}

/// Space Grotesk, bold, in the ink colour, at `size` dp with the browser's
/// default line height: headings, amounts and reference numbers. Chain
/// `.medium()` or `.semibold()` for the lighter cuts.
pub fn grotesk(content: impl Into<Cow<'static, str>>, size: f32) -> Text {
    text(content)
        .family(GROTESK)
        .size(size)
        .bold()
        .line_height_dp(normal_line_height(GROTESK_METRICS, size))
        .color(INK)
}

/// The page title of the form screens: Space Grotesk 28/34, bold, with
/// `letter-spacing: -0.4px`.
pub fn page_title(content: impl Into<Cow<'static, str>>) -> Text {
    grotesk(content, 28.0)
        .line_height_dp(34.0)
        .letter_spacing(-0.4)
        .heading()
}

/// The paragraph under a page title: Inter 15/22 in the muted colour. It
/// fills the width so that it wraps.
pub fn lead(content: impl Into<Cow<'static, str>>) -> Text {
    inter(content, 15.0)
        .line_height_dp(22.0)
        .color(MUTED)
        .fill_width()
}

/// A card or section heading: Space Grotesk bold, 16 dp inside cards and
/// 17 dp on the dashboard.
pub fn heading(content: impl Into<Cow<'static, str>>, size: f32) -> Text {
    grotesk(content, size)
}

/// A small-caps section label: Inter 13 bold, capitals, `letter-spacing:
/// 0.6px`, muted.
pub fn overline(content: impl Into<Cow<'static, str>>) -> Text {
    inter(content, 13.0)
        .bold()
        .letter_spacing(0.6)
        .uppercase()
        .color(MUTED)
}

/// A field label: Inter 13 medium.
pub fn label(content: impl Into<Cow<'static, str>>) -> Text {
    inter(content, 13.0).medium()
}

/// Secondary information under a title: Inter 12 muted.
pub fn meta(content: impl Into<Cow<'static, str>>) -> Text {
    inter(content, 12.0).color(MUTED)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_line_heights_are_the_browsers() {
        let inter = |size| normal_line_height(INTER_METRICS, size);
        assert_eq!(
            [11.0, 12.0, 13.0, 14.0, 15.0, 16.0].map(inter),
            [13.0, 14.5, 15.5, 17.0, 18.0, 19.5]
        );
        let grotesk = |size| normal_line_height(GROTESK_METRICS, size);
        assert_eq!(
            [13.0, 14.0, 16.0, 17.0, 18.0, 19.0, 20.0, 22.0, 38.0].map(grotesk),
            [17.0, 18.0, 20.0, 21.5, 23.0, 24.0, 25.5, 28.0, 48.5]
        );
    }
}
