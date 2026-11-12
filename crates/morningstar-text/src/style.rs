use morningstar_core::Color;

/// Horizontal alignment of lines within the available width.
///
/// Alignment only has an effect when text is laid out with a maximum width.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextAlign {
    /// Left for left-to-right paragraphs, right for right-to-left ones.
    #[default]
    Start,
    Center,
    /// Right for left-to-right paragraphs, left for right-to-left ones.
    End,
}

/// Font weight on the usual 100 to 900 scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextWeight(pub u16);

impl TextWeight {
    pub const REGULAR: Self = Self(400);
    pub const MEDIUM: Self = Self(500);
    pub const SEMIBOLD: Self = Self(600);
    pub const BOLD: Self = Self(700);
}

impl Default for TextWeight {
    fn default() -> Self {
        Self::REGULAR
    }
}

/// Which typeface a run of text is set in.
///
/// A family is named as the font file names itself ("Inter", "Space
/// Grotesk"); the name is matched without regard to case. The weight is
/// chosen among the faces of that family, and a character the family lacks
/// is taken from the other fonts that were added, so naming a Latin family
/// does not lose Arabic text.
///
/// ```
/// use morningstar_text::FontFamily;
/// const HEADING: FontFamily = FontFamily::named("Space Grotesk");
/// assert_eq!(HEADING, FontFamily::from("Space Grotesk"));
/// assert_eq!(FontFamily::default(), FontFamily::Default);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FontFamily {
    /// No preference: the fonts are tried in the order they were added, as
    /// before families could be named.
    #[default]
    Default,
    /// The family with this name. If no added font has it, the text falls
    /// back to the other fonts as with `Default`.
    Named(&'static str),
}

impl FontFamily {
    /// The family called `name`.
    pub const fn named(name: &'static str) -> Self {
        Self::Named(name)
    }
}

impl From<&'static str> for FontFamily {
    fn from(name: &'static str) -> Self {
        Self::Named(name)
    }
}

/// How a run of text looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    /// Font size in physical pixels.
    pub size: f32,
    /// Line height as a multiple of `size`.
    pub line_height: f32,
    pub color: Color,
    pub weight: TextWeight,
    pub align: TextAlign,
    /// The typeface. [`FontFamily::Default`] unless set.
    pub family: FontFamily,
    /// Extra space after every character, in physical pixels (CSS
    /// `letter-spacing`). Negative values tighten. Zero unless set.
    pub letter_spacing: f32,
    /// Italic (CSS `font-style: italic`). An italic face of the family is
    /// used if one was added; otherwise the upright face is slanted by 14
    /// degrees when its glyphs are rasterised, which is what a browser
    /// does. Either way the text takes the room the upright text would.
    pub italic: bool,
    /// Sets digits on a common width so that columns of numbers line up
    /// and a running timer does not jitter (CSS
    /// `font-variant-numeric: tabular-nums`, the OpenType `tnum` feature).
    /// Has no effect on a font without the feature; many fonts' digits are
    /// tabular already.
    pub tabular_nums: bool,
}

impl TextStyle {
    /// Regular weight, start-aligned, with a line height of 1.3.
    pub const fn new(size: f32, color: Color) -> Self {
        Self {
            size,
            line_height: 1.3,
            color,
            weight: TextWeight::REGULAR,
            align: TextAlign::Start,
            family: FontFamily::Default,
            letter_spacing: 0.0,
            italic: false,
            tabular_nums: false,
        }
    }

    /// Sets digits on a common width. See
    /// [`tabular_nums`](Self::tabular_nums).
    pub const fn tabular_nums(mut self) -> Self {
        self.tabular_nums = true;
        self
    }

    /// Makes the text italic. See [`italic`](Self::italic).
    pub const fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    /// Sets the typeface.
    pub const fn family(mut self, family: FontFamily) -> Self {
        self.family = family;
        self
    }

    /// Sets the extra space after every character, in physical pixels.
    pub const fn letter_spacing(mut self, pixels: f32) -> Self {
        self.letter_spacing = pixels;
        self
    }

    pub const fn weight(mut self, weight: TextWeight) -> Self {
        self.weight = weight;
        self
    }

    pub const fn bold(self) -> Self {
        self.weight(TextWeight::BOLD)
    }

    pub const fn align(mut self, align: TextAlign) -> Self {
        self.align = align;
        self
    }

    pub const fn line_height(mut self, multiple: f32) -> Self {
        self.line_height = multiple;
        self
    }
}

/// One stretch of a paragraph with its own weight, colour or family, for
/// [`TextContext::draw_spans`](crate::TextContext::draw_spans). Whatever a
/// span leaves unset comes from the paragraph's [`TextStyle`]. Size, line
/// height, alignment and letter spacing belong to the paragraph.
///
/// ```
/// use morningstar_text::{TextSpan, TextWeight};
/// let spans = [
///     TextSpan::new("New here? "),
///     TextSpan::new("Create an account").weight(TextWeight::SEMIBOLD),
/// ];
/// # let _ = spans;
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextSpan<'a> {
    /// The characters of this span.
    pub text: &'a str,
    /// Weight, if different from the paragraph's.
    pub weight: Option<TextWeight>,
    /// Colour, if different from the paragraph's.
    pub color: Option<Color>,
    /// Family, if different from the paragraph's.
    pub family: Option<FontFamily>,
    /// Italic or upright, if different from the paragraph's
    /// ([`TextStyle::italic`]).
    pub italic: Option<bool>,
}

impl<'a> TextSpan<'a> {
    /// A span in the paragraph's own style.
    pub const fn new(text: &'a str) -> Self {
        Self {
            text,
            weight: None,
            color: None,
            family: None,
            italic: None,
        }
    }

    /// Makes the span italic. See [`TextStyle::italic`].
    pub const fn italic(mut self) -> Self {
        self.italic = Some(true);
        self
    }

    /// Sets the span's weight.
    pub const fn weight(mut self, weight: TextWeight) -> Self {
        self.weight = Some(weight);
        self
    }

    /// Sets the span's colour.
    pub const fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Sets the span's family.
    pub const fn family(mut self, family: FontFamily) -> Self {
        self.family = Some(family);
        self
    }
}
