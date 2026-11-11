//! Design tokens: the colours, type scale, spacing and radii a screen is
//! built from.

use morningstar_core::Color;
use morningstar_runtime::{FontFamily, TextWeight};

use crate::color::{ThemeColor, hex, with_alpha};

/// The colour roles of a theme. Each field is the value of the
/// [`ThemeColor`] token of the same name.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Colors {
    /// The screen background; also the frame's clear colour.
    pub background: Color,
    /// Cards, sheets and bars.
    pub surface: Color,
    /// Input fields, chips, secondary buttons.
    pub surface_variant: Color,
    /// Brand or accent colour.
    pub primary: Color,
    /// Text on `primary`.
    pub on_primary: Color,
    /// A soft tint of `primary`.
    pub primary_container: Color,
    /// Main text.
    pub text: Color,
    /// Secondary text.
    pub text_muted: Color,
    /// Hairlines and outlines.
    pub divider: Color,
    /// Destructive actions and errors.
    pub danger: Color,
    /// Text on `danger`.
    pub on_danger: Color,
    /// Positive amounts and confirmations.
    pub success: Color,
    /// Painted over a pressable while a finger is on it (unless the
    /// pressable sets its own pressed background). Translucent.
    pub pressed_overlay: Color,
}

/// One step of the type scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextSpec {
    /// Font size in dp.
    pub size: f32,
    /// Font weight.
    pub weight: TextWeight,
    /// Line height as a multiple of the size.
    pub line_height: f32,
}

impl TextSpec {
    /// A step of the given size, weight and line-height multiple.
    pub const fn new(size: f32, weight: TextWeight, line_height: f32) -> Self {
        Self {
            size,
            weight,
            line_height,
        }
    }
}

/// The type scale. [`Text`](crate::Text) picks a step with `.display()`,
/// `.title()`, `.headline()`, `.body()` (the default), `.label()` or
/// `.caption()`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypeScale {
    /// The one big number or word on a screen (a balance).
    pub display: TextSpec,
    /// Screen titles.
    pub title: TextSpec,
    /// Section and card headings.
    pub headline: TextSpec,
    /// Running text and list rows. The default.
    pub body: TextSpec,
    /// Buttons, tabs, field labels.
    pub label: TextSpec,
    /// Metadata, timestamps, hints.
    pub caption: TextSpec,
}

/// A step of the [`TypeScale`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextRole {
    /// [`TypeScale::display`].
    Display,
    /// [`TypeScale::title`].
    Title,
    /// [`TypeScale::headline`].
    Headline,
    /// [`TypeScale::body`].
    #[default]
    Body,
    /// [`TypeScale::label`].
    Label,
    /// [`TypeScale::caption`].
    Caption,
}

/// The font families of a theme.
///
/// A design usually has one typeface for running text and sometimes a
/// second for headings and numbers. [`Text`](crate::Text) takes `heading`
/// for the `display`, `title` and `headline` roles and `body` for the rest;
/// `.family(..)` on a text overrides either.
///
/// ```
/// use morningstar_ui::prelude::*;
/// const THEME: Theme = Theme {
///     fonts: Fonts::new(FontFamily::named("Inter"), FontFamily::named("Space Grotesk")),
///     ..Theme::LIGHT
/// };
/// assert_eq!(THEME.font(TextRole::Title), FontFamily::named("Space Grotesk"));
/// assert_eq!(THEME.font(TextRole::Caption), FontFamily::named("Inter"));
/// ```
///
/// The fonts themselves are added in `Application::setup`; a family nobody
/// added falls back to the fonts in the order they were added.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fonts {
    /// Running text, labels and captions.
    pub body: FontFamily,
    /// Display, title and headline text.
    pub heading: FontFamily,
}

impl Fonts {
    /// No named families: every text uses the fonts in the order they were
    /// added, as before families existed.
    pub const DEFAULT: Self = Self::new(FontFamily::Default, FontFamily::Default);

    /// A body family and a heading family.
    pub const fn new(body: FontFamily, heading: FontFamily) -> Self {
        Self { body, heading }
    }
}

impl Default for Fonts {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// The spacing scale, in dp. For padding and gaps:
/// `col![..].padding(SPACING.lg).gap(SPACING.sm)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spacing {
    /// 4 by default.
    pub xs: f32,
    /// 8 by default.
    pub sm: f32,
    /// 12 by default.
    pub md: f32,
    /// 16 by default: the usual screen margin.
    pub lg: f32,
    /// 24 by default.
    pub xl: f32,
    /// 32 by default.
    pub xxl: f32,
}

/// The corner-radius scale, in dp.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Radii {
    /// 8 by default: chips, small tiles.
    pub sm: f32,
    /// 12 by default: buttons, fields.
    pub md: f32,
    /// 16 by default: cards.
    pub lg: f32,
    /// 24 by default: hero cards, sheets.
    pub xl: f32,
    /// Large enough to make any box a pill or a circle.
    pub full: f32,
}

/// The tokens a screen is built from.
///
/// An application returns one from [`Application::theme`]
/// (crate::Application::theme). Built-in widgets read their colours and text
/// styles from it; application code refers to colours with [`ThemeColor`]
/// tokens and to spacing and radii through a constant such as
/// [`Theme::LIGHT`]:
///
/// ```
/// use morningstar_ui::prelude::*;
/// const T: Theme = Theme::LIGHT;
/// let card: Container<()> = col![text("Hello")]
///     .padding(T.spacing.lg)
///     .radius(T.radii.lg)
///     .background(ThemeColor::Surface);
/// # let _ = card;
/// ```
///
/// All fields are public: start from `Theme::LIGHT` and change what the
/// design changes, `Theme { colors: Colors { primary: hex(0x00A651),
/// ..Theme::LIGHT.colors }, ..Theme::LIGHT }`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    /// Colour roles.
    pub colors: Colors,
    /// Text styles.
    pub type_scale: TypeScale,
    /// Spacing scale.
    pub spacing: Spacing,
    /// Corner-radius scale.
    pub radii: Radii,
    /// Font families.
    pub fonts: Fonts,
}

const TYPE_SCALE: TypeScale = TypeScale {
    display: TextSpec::new(34.0, TextWeight::BOLD, 1.2),
    title: TextSpec::new(24.0, TextWeight::BOLD, 1.25),
    headline: TextSpec::new(17.0, TextWeight::SEMIBOLD, 1.3),
    body: TextSpec::new(15.0, TextWeight::REGULAR, 1.4),
    label: TextSpec::new(14.0, TextWeight::SEMIBOLD, 1.3),
    caption: TextSpec::new(12.0, TextWeight::REGULAR, 1.35),
};

const SPACING: Spacing = Spacing {
    xs: 4.0,
    sm: 8.0,
    md: 12.0,
    lg: 16.0,
    xl: 24.0,
    xxl: 32.0,
};

const RADII: Radii = Radii {
    sm: 8.0,
    md: 12.0,
    lg: 16.0,
    xl: 24.0,
    full: 9999.0,
};

impl Theme {
    /// The default light theme: a cool grey background, white surfaces and
    /// a blue accent.
    pub const LIGHT: Self = Self {
        colors: Colors {
            background: hex(0xF4F6FA),
            surface: hex(0xFFFFFF),
            surface_variant: hex(0xEBEEF4),
            primary: hex(0x2457F5),
            on_primary: hex(0xFFFFFF),
            primary_container: hex(0xE2E9FF),
            text: hex(0x131722),
            text_muted: hex(0x69707F),
            divider: hex(0xE3E6ED),
            danger: hex(0xD92D2D),
            on_danger: hex(0xFFFFFF),
            success: hex(0x12925A),
            pressed_overlay: with_alpha(hex(0x000000), 0.10),
        },
        type_scale: TYPE_SCALE,
        spacing: SPACING,
        radii: RADII,
        fonts: Fonts::DEFAULT,
    };

    /// A dark counterpart of [`LIGHT`](Self::LIGHT).
    pub const DARK: Self = Self {
        colors: Colors {
            background: hex(0x0E1016),
            surface: hex(0x1A1D26),
            surface_variant: hex(0x262A36),
            primary: hex(0x7C9BFF),
            on_primary: hex(0x0B1535),
            primary_container: hex(0x1F2B55),
            text: hex(0xF1F3F8),
            text_muted: hex(0x9BA2B2),
            divider: hex(0x2C303C),
            danger: hex(0xFF6B6B),
            on_danger: hex(0x3A0A0A),
            success: hex(0x4CD18F),
            pressed_overlay: with_alpha(hex(0xFFFFFF), 0.12),
        },
        type_scale: TYPE_SCALE,
        spacing: SPACING,
        radii: RADII,
        fonts: Fonts::DEFAULT,
    };

    /// Same as [`Theme::LIGHT`].
    pub const fn light() -> Self {
        Self::LIGHT
    }

    /// Same as [`Theme::DARK`].
    pub const fn dark() -> Self {
        Self::DARK
    }

    /// The value of a colour token.
    pub const fn color(&self, token: ThemeColor) -> Color {
        let colors = &self.colors;
        match token {
            ThemeColor::Background => colors.background,
            ThemeColor::Surface => colors.surface,
            ThemeColor::SurfaceVariant => colors.surface_variant,
            ThemeColor::Primary => colors.primary,
            ThemeColor::OnPrimary => colors.on_primary,
            ThemeColor::PrimaryContainer => colors.primary_container,
            ThemeColor::Text => colors.text,
            ThemeColor::TextMuted => colors.text_muted,
            ThemeColor::Divider => colors.divider,
            ThemeColor::Danger => colors.danger,
            ThemeColor::OnDanger => colors.on_danger,
            ThemeColor::Success => colors.success,
        }
    }

    /// The font family of a step of the type scale: `fonts.heading` for
    /// display, title and headline, `fonts.body` for the rest.
    pub const fn font(&self, role: TextRole) -> FontFamily {
        match role {
            TextRole::Display | TextRole::Title | TextRole::Headline => self.fonts.heading,
            TextRole::Body | TextRole::Label | TextRole::Caption => self.fonts.body,
        }
    }

    /// The text style of a step of the type scale.
    pub const fn text(&self, role: TextRole) -> TextSpec {
        let scale = &self.type_scale;
        match role {
            TextRole::Display => scale.display,
            TextRole::Title => scale.title,
            TextRole::Headline => scale.headline,
            TextRole::Body => scale.body,
            TextRole::Label => scale.label,
            TextRole::Caption => scale.caption,
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::LIGHT
    }
}
