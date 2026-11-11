//! Vector icons: a small subset of SVG, rasterised to coverage masks that
//! live in the glyph atlas.
//!
//! An icon is one colour. It is described either by SVG markup pasted from
//! a design ([`VectorIcon::svg`]) or by path data alone
//! ([`VectorIcon::stroked`], [`VectorIcon::filled`]). At draw time it is
//! rasterised by `zeno` at exactly the pixel size asked for, so it is as
//! sharp at any density as a glyph is, and drawn as a glyph quad tinted with
//! the colour asked for.
//!
//! This is not an SVG renderer. See [`IconError`] for what is refused.

use std::fmt;

use zeno::{Cap, Command, Fill, Join, Mask, PathBuilder, Stroke, Transform};

/// Longest side, in pixels, an icon is rasterised at. A larger icon is
/// rasterised at this size and stretched when drawn.
pub(crate) const MAX_ICON_PIXELS: u32 = 512;

/// A one-colour vector icon.
///
/// The value is a few words of `Copy` data naming a `'static` description,
/// so a set of icons is a list of constants:
///
/// ```
/// use morningstar_text::VectorIcon;
///
/// pub const CHECK: VectorIcon = VectorIcon::stroked("M5 12l5 5 9-10");
/// pub const LOCK: VectorIcon = VectorIcon::svg(
///     r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"
///          stroke-linecap="round" stroke-linejoin="round">
///          <rect x="4" y="10" width="16" height="11" rx="3"/>
///          <path d="M8 10V7a4 4 0 0 1 8 0v3"/>
///        </svg>"#,
/// );
/// assert!(CHECK.check().is_ok());
/// assert!(LOCK.check().is_ok());
/// ```
///
/// # The supported subset of SVG
///
/// * One `<svg>` element with a `viewBox`, containing `<path>`, `<circle>`,
///   `<rect>` (with `rx`/`ry`) and `<line>` elements.
/// * `fill`, `stroke`, `stroke-width`, `stroke-linecap` and
///   `stroke-linejoin`, on the `<svg>` element (inherited) or on a shape.
///   As in SVG, the defaults are a black fill and no stroke.
/// * Every paint that is not `none` is the icon's one ink: `currentColor`
///   and a literal colour mean the same thing, and the colour drawn is the
///   one given at draw time. Two *different* paints in one icon are refused.
/// * Lengths are plain numbers in view-box units.
///
/// Everything else is refused by [`check`](Self::check) with an
/// [`IconError`] that names the element or attribute: groups and
/// transforms, `<ellipse>`, `<polyline>`, `<polygon>`, `<text>`, `<use>`,
/// gradients, opacity, dashes, `fill-rule`, `style` attributes, CSS, units
/// and percentages. An icon that fails to parse draws nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VectorIcon {
    source: Source,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Source {
    Svg(&'static str),
    Stroked(&'static str),
    Filled(&'static str),
}

impl VectorIcon {
    /// An icon from SVG markup in the supported subset.
    pub const fn svg(markup: &'static str) -> Self {
        Self {
            source: Source::Svg(markup),
        }
    }

    /// An outline icon from SVG path data, in the convention most icon sets
    /// share: a 24 x 24 view box and a stroke 2 units wide with round caps
    /// and joins. The width can be changed where the icon is drawn.
    pub const fn stroked(path: &'static str) -> Self {
        Self {
            source: Source::Stroked(path),
        }
    }

    /// A solid icon from SVG path data in a 24 x 24 view box.
    pub const fn filled(path: &'static str) -> Self {
        Self {
            source: Source::Filled(path),
        }
    }

    /// Checks that the icon is within the supported subset, without drawing
    /// it. Worth a unit test over an application's whole icon set, because
    /// an icon that does not parse is otherwise only noticed as a gap on
    /// screen.
    pub fn check(&self) -> Result<(), IconError> {
        self.parse().map(|_| ())
    }

    /// Width divided by height of the icon's view box, or `None` if the
    /// icon does not parse.
    pub fn aspect_ratio(&self) -> Option<f32> {
        let art = self.parse().ok()?;
        Some(art.view_box[2] / art.view_box[3])
    }

    pub(crate) fn parse(&self) -> Result<Art, IconError> {
        match self.source {
            Source::Svg(markup) => parse_svg(markup),
            Source::Stroked(data) => Ok(Art {
                view_box: [0.0, 0.0, 24.0, 24.0],
                shapes: vec![Shape {
                    commands: path_commands(data)?,
                    fill: false,
                    stroke: Some(StrokeStyle {
                        width: 2.0,
                        cap: Cap::Round,
                        join: Join::Round,
                    }),
                }],
            }),
            Source::Filled(data) => Ok(Art {
                view_box: [0.0, 0.0, 24.0, 24.0],
                shapes: vec![Shape {
                    commands: path_commands(data)?,
                    fill: true,
                    stroke: None,
                }],
            }),
        }
    }
}

/// Why a [`VectorIcon`] cannot be drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IconError {
    /// The markup is not well formed. `offset` is the byte position.
    Syntax {
        /// Byte offset into the markup.
        offset: usize,
        /// What was expected there.
        message: &'static str,
    },
    /// There is no `<svg>` element, or it has no usable `viewBox`.
    MissingViewBox,
    /// An element outside the supported subset, such as `<g>` or
    /// `<polygon>`.
    UnsupportedElement(String),
    /// An attribute outside the supported subset, such as `transform` or
    /// `opacity`.
    UnsupportedAttribute {
        /// The element it was found on.
        element: String,
        /// The attribute's name.
        attribute: String,
    },
    /// An attribute whose value is not understood: a length with a unit, an
    /// unknown line cap, a negative radius.
    UnsupportedValue {
        /// The attribute's name.
        attribute: String,
        /// The value found.
        value: String,
    },
    /// A shape lacks an attribute it cannot be drawn without (`d`, `r`,
    /// `width`, `height`).
    MissingAttribute {
        /// The element.
        element: String,
        /// The attribute it needs.
        attribute: &'static str,
    },
    /// The icon paints with more than one colour. An icon is a single mask;
    /// split a multi-colour drawing into one icon per colour.
    MultipleColors,
    /// Path data that does not parse. `offset` is the byte position within
    /// the path data.
    InvalidPath {
        /// Byte offset into the path data.
        offset: usize,
    },
    /// Nothing in the icon paints anything.
    Empty,
}

impl fmt::Display for IconError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax { offset, message } => {
                write!(f, "malformed icon markup at byte {offset}: {message}")
            }
            Self::MissingViewBox => write!(f, "the icon has no <svg> element with a viewBox"),
            Self::UnsupportedElement(name) => write!(
                f,
                "<{name}> is not supported in icons (only path, circle, rect and line)"
            ),
            Self::UnsupportedAttribute { element, attribute } => write!(
                f,
                "attribute `{attribute}` on <{element}> is not supported in icons"
            ),
            Self::UnsupportedValue { attribute, value } => {
                write!(f, "`{attribute}=\"{value}\"` is not supported in icons")
            }
            Self::MissingAttribute { element, attribute } => {
                write!(f, "<{element}> needs a `{attribute}` attribute")
            }
            Self::MultipleColors => write!(
                f,
                "the icon uses more than one colour; an icon is a single-colour mask"
            ),
            Self::InvalidPath { offset } => write!(f, "invalid path data at byte {offset}"),
            Self::Empty => write!(f, "the icon paints nothing"),
        }
    }
}

impl std::error::Error for IconError {}

/// Identifies one rasterisation of an icon in the atlas.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct IconKey {
    pub icon: VectorIcon,
    pub width: u32,
    pub height: u32,
    /// Bits of the stroke width override, in view-box units.
    pub stroke: Option<u32>,
}

#[derive(Clone, Copy, Debug)]
struct StrokeStyle {
    width: f32,
    cap: Cap,
    join: Join,
}

#[derive(Clone, Debug)]
struct Shape {
    commands: Vec<Command>,
    fill: bool,
    stroke: Option<StrokeStyle>,
}

/// A parsed icon: shapes in view-box coordinates.
#[derive(Clone, Debug)]
pub(crate) struct Art {
    /// x, y, width, height.
    view_box: [f32; 4],
    shapes: Vec<Shape>,
}

impl Art {
    /// Rasterises the icon into a `width * height` coverage image. The view
    /// box is scaled uniformly to fit and centred, as SVG does by default.
    /// `stroke_width`, in view-box units, replaces the width of every
    /// stroke.
    pub(crate) fn rasterise(&self, width: u32, height: u32, stroke_width: Option<f32>) -> Vec<u8> {
        let [box_x, box_y, box_width, box_height] = self.view_box;
        let scale = (width as f32 / box_width).min(height as f32 / box_height);
        let transform = Transform::scale(scale, scale).then_translate(
            (width as f32 - box_width * scale) / 2.0 - box_x * scale,
            (height as f32 - box_height * scale) / 2.0 - box_y * scale,
        );

        let length = (width * height) as usize;
        let mut coverage = vec![0u8; length];
        let mut layer = vec![0u8; length];
        let mut first = true;
        let mut paint = |style: zeno::Style<'_>, commands: &Vec<Command>| {
            let target = if first { &mut coverage } else { &mut layer };
            target.fill(0);
            Mask::new(commands)
                .size(width, height)
                .transform(Some(transform))
                .style(style)
                .render_into(target, None);
            if !first {
                // Each shape is painted over the ones before it, in one ink.
                for (below, above) in coverage.iter_mut().zip(layer.iter()) {
                    let (b, a) = (u32::from(*below), u32::from(*above));
                    *below = (a + b - (a * b + 127) / 255) as u8;
                }
            }
            first = false;
        };

        for shape in &self.shapes {
            if shape.fill {
                paint(Fill::NonZero.into(), &shape.commands);
            }
            if let Some(stroke) = shape.stroke {
                let width = stroke_width
                    .filter(|width| width.is_finite() && *width > 0.0)
                    .unwrap_or(stroke.width);
                // The stroke width is in view-box units; `scale` (on by
                // default) applies the transform to it.
                let mut style = Stroke::new(width);
                style.cap(stroke.cap).join(stroke.join);
                paint(style.into(), &shape.commands);
            }
        }
        coverage
    }
}

/// Reads SVG path data into commands.
///
/// Every command of the path grammar is understood: `M L H V C S Q T A Z`
/// and their relative forms, repeated arguments, and numbers written
/// without separators (`a4 4 0 0 0-5.2 5.2`, `1.5.5`). Written here rather
/// than taken from `zeno`, whose own reader mishandles a smooth curve
/// (`s`, `S`) that repeats or follows another.
fn path_commands(data: &str) -> Result<Vec<Command>, IconError> {
    let mut reader = PathReader {
        bytes: data.as_bytes(),
        position: 0,
    };
    let mut commands: Vec<Command> = Vec::new();
    let (mut current, mut start) = ((0.0f32, 0.0f32), (0.0f32, 0.0f32));
    // The last control point, for the smooth commands, and what set it.
    let mut control = (0.0f32, 0.0f32);
    let mut previous = b' ';
    let mut command = b' ';

    loop {
        reader.skip_separators();
        let Some(next) = reader.peek() else { break };
        if next.is_ascii_alphabetic() {
            command = next;
            reader.position += 1;
        } else if command == b' ' || command.eq_ignore_ascii_case(&b'z') {
            // Numbers before any command, or after a close.
            return Err(reader.error());
        } else if command == b'M' {
            // Further pairs after a move are lines.
            command = b'L';
        } else if command == b'm' {
            command = b'l';
        }
        if commands.is_empty() && !command.eq_ignore_ascii_case(&b'm') {
            return Err(reader.error());
        }

        let relative = command.is_ascii_lowercase();
        let base = if relative { current } else { (0.0, 0.0) };
        let point = |reader: &mut PathReader<'_>| -> Result<(f32, f32), IconError> {
            let x = reader.number()?;
            let y = reader.number()?;
            Ok((base.0 + x, base.1 + y))
        };
        match command.to_ascii_uppercase() {
            b'M' => {
                current = point(&mut reader)?;
                start = current;
                commands.move_to(current);
            }
            b'L' => {
                current = point(&mut reader)?;
                commands.line_to(current);
            }
            b'H' => {
                current.0 = base.0 + reader.number()?;
                commands.line_to(current);
            }
            b'V' => {
                current.1 = base.1 + reader.number()?;
                commands.line_to(current);
            }
            b'C' | b'S' => {
                let first = if command.eq_ignore_ascii_case(&b'c') {
                    point(&mut reader)?
                } else if matches!(previous, b'C' | b'S') {
                    // The previous curve's second control point, mirrored.
                    (2.0 * current.0 - control.0, 2.0 * current.1 - control.1)
                } else {
                    current
                };
                control = point(&mut reader)?;
                current = point(&mut reader)?;
                commands.curve_to(first, control, current);
            }
            b'Q' | b'T' => {
                control = if command.eq_ignore_ascii_case(&b'q') {
                    point(&mut reader)?
                } else if matches!(previous, b'Q' | b'T') {
                    (2.0 * current.0 - control.0, 2.0 * current.1 - control.1)
                } else {
                    current
                };
                current = point(&mut reader)?;
                commands.quad_to(control, current);
            }
            b'A' => {
                let (rx, ry) = (reader.number()?.abs(), reader.number()?.abs());
                let rotation = reader.number()?;
                let large = reader.flag()?;
                let sweep = reader.flag()?;
                let to = point(&mut reader)?;
                if rx == 0.0 || ry == 0.0 {
                    // As the SVG specification says: a straight line.
                    commands.line_to(to);
                } else if to != current {
                    commands.arc_to(
                        rx,
                        ry,
                        zeno::Angle::from_degrees(rotation),
                        if large {
                            zeno::ArcSize::Large
                        } else {
                            zeno::ArcSize::Small
                        },
                        if sweep {
                            zeno::ArcSweep::Positive
                        } else {
                            zeno::ArcSweep::Negative
                        },
                        to,
                    );
                }
                current = to;
            }
            b'Z' => {
                commands.close();
                current = start;
            }
            _ => return Err(reader.error()),
        }
        previous = command.to_ascii_uppercase();
    }
    if commands.is_empty() {
        return Err(IconError::Empty);
    }
    Ok(commands)
}

struct PathReader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl PathReader<'_> {
    fn error(&self) -> IconError {
        IconError::InvalidPath {
            offset: self.position,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }

    fn skip_separators(&mut self) {
        while self
            .peek()
            .is_some_and(|byte| byte.is_ascii_whitespace() || byte == b',')
        {
            self.position += 1;
        }
    }

    /// An arc flag: a single `0` or `1`, which need not be followed by a
    /// separator.
    fn flag(&mut self) -> Result<bool, IconError> {
        self.skip_separators();
        let flag = match self.peek() {
            Some(b'0') => false,
            Some(b'1') => true,
            _ => return Err(self.error()),
        };
        self.position += 1;
        Ok(flag)
    }

    fn number(&mut self) -> Result<f32, IconError> {
        self.skip_separators();
        let start = self.position;
        let digits = |reader: &mut Self| {
            let from = reader.position;
            while reader.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                reader.position += 1;
            }
            reader.position - from
        };
        if matches!(self.peek(), Some(b'+' | b'-')) {
            self.position += 1;
        }
        let mut count = digits(self);
        if self.peek() == Some(b'.') {
            self.position += 1;
            count += digits(self);
        }
        if count == 0 {
            self.position = start;
            return Err(self.error());
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            let mark = self.position;
            self.position += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.position += 1;
            }
            if digits(self) == 0 {
                // Not an exponent after all.
                self.position = mark;
            }
        }
        std::str::from_utf8(&self.bytes[start..self.position])
            .ok()
            .and_then(|text| text.parse::<f32>().ok())
            .filter(|number| number.is_finite())
            .ok_or(IconError::InvalidPath { offset: start })
    }
}

/// Inherited presentation attributes.
#[derive(Clone, Copy)]
struct Presentation<'a> {
    /// `None` is `none`; otherwise the paint as written, lower-cased later.
    fill: Option<&'a str>,
    stroke: Option<&'a str>,
    width: f32,
    cap: Cap,
    join: Join,
}

/// Attributes that describe nothing visible and are skipped.
fn is_ignorable(name: &str) -> bool {
    matches!(
        name,
        "xmlns" | "version" | "role" | "id" | "class" | "focusable" | "width" | "height"
    ) || name.starts_with("xmlns:")
        || name.starts_with("aria-")
        || name.starts_with("data-")
}

fn number(attribute: &str, value: &str) -> Result<f32, IconError> {
    value
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|number| number.is_finite())
        .ok_or_else(|| IconError::UnsupportedValue {
            attribute: attribute.to_owned(),
            value: value.to_owned(),
        })
}

/// Applies one presentation attribute. Returns false if `name` is not one.
fn presentation<'a>(
    state: &mut Presentation<'a>,
    name: &str,
    value: &'a str,
) -> Result<bool, IconError> {
    let unsupported = || IconError::UnsupportedValue {
        attribute: name.to_owned(),
        value: value.to_owned(),
    };
    let paint = |value: &'a str| {
        let value = value.trim();
        if value.is_empty() || value.starts_with("url(") || value == "inherit" {
            return Err(unsupported());
        }
        Ok((value != "none" && value != "transparent").then_some(value))
    };
    match name {
        "fill" => state.fill = paint(value)?,
        "stroke" => state.stroke = paint(value)?,
        "stroke-width" => {
            state.width = number(name, value)?;
            if state.width < 0.0 {
                return Err(unsupported());
            }
        }
        "stroke-linecap" => {
            state.cap = match value.trim() {
                "butt" => Cap::Butt,
                "round" => Cap::Round,
                "square" => Cap::Square,
                _ => return Err(unsupported()),
            }
        }
        "stroke-linejoin" => {
            state.join = match value.trim() {
                "miter" => Join::Miter,
                "round" => Join::Round,
                "bevel" => Join::Bevel,
                _ => return Err(unsupported()),
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}

/// One tag of the markup.
struct Tag<'a> {
    name: &'a str,
    closing: bool,
    self_closing: bool,
    attributes: Vec<(&'a str, &'a str)>,
}

struct Reader<'a> {
    source: &'a str,
    position: usize,
}

impl<'a> Reader<'a> {
    fn error(&self, message: &'static str) -> IconError {
        IconError::Syntax {
            offset: self.position,
            message,
        }
    }

    fn rest(&self) -> &'a str {
        &self.source[self.position..]
    }

    fn skip_whitespace(&mut self) {
        let rest = self.rest();
        self.position += rest.len() - rest.trim_start().len();
    }

    fn name(&mut self) -> &'a str {
        let rest = self.rest();
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '.')))
            .unwrap_or(rest.len());
        self.position += end;
        &rest[..end]
    }

    /// The next tag, skipping comments, declarations and whitespace.
    /// `None` at the end of the markup.
    fn tag(&mut self) -> Result<Option<Tag<'a>>, IconError> {
        loop {
            self.skip_whitespace();
            let rest = self.rest();
            if rest.is_empty() {
                return Ok(None);
            }
            let skip_to = |end: &str| rest.find(end).map(|index| index + end.len());
            if rest.starts_with("<!--") {
                self.position += skip_to("-->").ok_or_else(|| self.error("unclosed comment"))?;
            } else if rest.starts_with("<?") {
                self.position += skip_to("?>").ok_or_else(|| self.error("unclosed declaration"))?;
            } else if rest.starts_with("<!") {
                self.position += skip_to(">").ok_or_else(|| self.error("unclosed declaration"))?;
            } else if rest.starts_with('<') {
                break;
            } else {
                return Err(self.error("text is not supported in icons; expected `<`"));
            }
        }

        self.position += 1;
        let closing = self.rest().starts_with('/');
        if closing {
            self.position += 1;
        }
        let name = self.name();
        if name.is_empty() {
            return Err(self.error("expected an element name"));
        }

        let mut attributes = Vec::new();
        loop {
            self.skip_whitespace();
            let rest = self.rest();
            if rest.starts_with("/>") {
                self.position += 2;
                return Ok(Some(Tag {
                    name,
                    closing,
                    self_closing: true,
                    attributes,
                }));
            }
            if rest.starts_with('>') {
                self.position += 1;
                return Ok(Some(Tag {
                    name,
                    closing,
                    self_closing: false,
                    attributes,
                }));
            }
            let attribute = self.name();
            if attribute.is_empty() {
                return Err(self.error("expected an attribute name, `>` or `/>`"));
            }
            self.skip_whitespace();
            if !self.rest().starts_with('=') {
                return Err(self.error("expected `=` after an attribute name"));
            }
            self.position += 1;
            self.skip_whitespace();
            let quote = match self.rest().chars().next() {
                Some(quote @ ('"' | '\'')) => quote,
                _ => return Err(self.error("expected a quoted attribute value")),
            };
            self.position += 1;
            let rest = self.rest();
            let end = rest
                .find(quote)
                .ok_or_else(|| self.error("unclosed attribute value"))?;
            attributes.push((attribute, &rest[..end]));
            self.position += end + 1;
        }
    }
}

fn parse_svg(markup: &str) -> Result<Art, IconError> {
    let mut reader = Reader {
        source: markup,
        position: 0,
    };

    let root = reader.tag()?.ok_or(IconError::MissingViewBox)?;
    if root.closing || root.name != "svg" {
        return Err(IconError::MissingViewBox);
    }
    let mut inherited = Presentation {
        // SVG's initial values.
        fill: Some("black"),
        stroke: None,
        width: 1.0,
        cap: Cap::Butt,
        join: Join::Miter,
    };
    let mut view_box = None;
    for &(name, value) in &root.attributes {
        if name == "viewBox" {
            let mut numbers = [0.0f32; 4];
            let mut parts = value
                .split(|c: char| c.is_whitespace() || c == ',')
                .filter(|part| !part.is_empty());
            for slot in &mut numbers {
                *slot = number(name, parts.next().unwrap_or(""))?;
            }
            if parts.next().is_some() || !(numbers[2] > 0.0 && numbers[3] > 0.0) {
                return Err(IconError::UnsupportedValue {
                    attribute: name.to_owned(),
                    value: value.to_owned(),
                });
            }
            view_box = Some(numbers);
        } else if !presentation(&mut inherited, name, value)? && !is_ignorable(name) {
            return Err(IconError::UnsupportedAttribute {
                element: "svg".to_owned(),
                attribute: name.to_owned(),
            });
        }
    }
    let view_box = view_box.ok_or(IconError::MissingViewBox)?;

    let mut shapes = Vec::new();
    // The one ink, as first written.
    let mut ink: Option<String> = None;
    let mut closed = root.self_closing;
    while !closed {
        let Some(tag) = reader.tag()? else {
            return Err(reader.error("expected `</svg>`"));
        };
        if tag.closing {
            if tag.name != "svg" {
                return Err(reader.error("unexpected closing tag"));
            }
            closed = true;
            continue;
        }
        if !matches!(tag.name, "path" | "circle" | "rect" | "line") {
            return Err(IconError::UnsupportedElement(tag.name.to_owned()));
        }

        let mut state = inherited;
        let mut geometry: Vec<(&str, &str)> = Vec::new();
        for &(name, value) in &tag.attributes {
            let geometric = match tag.name {
                "path" => name == "d",
                "circle" => matches!(name, "cx" | "cy" | "r"),
                "rect" => matches!(name, "x" | "y" | "width" | "height" | "rx" | "ry"),
                _ => matches!(name, "x1" | "y1" | "x2" | "y2"),
            };
            if geometric {
                geometry.push((name, value));
            } else if !presentation(&mut state, name, value)?
                // `width` and `height` are ignorable on <svg> only.
                && (!is_ignorable(name) || matches!(name, "width" | "height"))
            {
                return Err(IconError::UnsupportedAttribute {
                    element: tag.name.to_owned(),
                    attribute: name.to_owned(),
                });
            }
        }
        let find = |name: &str| {
            geometry
                .iter()
                .find(|(candidate, _)| *candidate == name)
                .map(|(_, value)| *value)
        };
        let optional = |name: &str| find(name).map(|value| number(name, value)).transpose();
        let required = |name: &'static str| {
            optional(name)?.ok_or_else(|| IconError::MissingAttribute {
                element: tag.name.to_owned(),
                attribute: name,
            })
        };
        let negative = |name: &str, value: f32| IconError::UnsupportedValue {
            attribute: name.to_owned(),
            value: value.to_string(),
        };

        let mut commands: Vec<Command> = Vec::new();
        match tag.name {
            "path" => {
                let data = find("d").ok_or_else(|| IconError::MissingAttribute {
                    element: "path".to_owned(),
                    attribute: "d",
                })?;
                commands = path_commands(data)?;
            }
            "circle" => {
                let center = (
                    optional("cx")?.unwrap_or(0.0),
                    optional("cy")?.unwrap_or(0.0),
                );
                let radius = required("r")?;
                if radius < 0.0 {
                    return Err(negative("r", radius));
                }
                commands.add_circle(center, radius);
            }
            "rect" => {
                let origin = (optional("x")?.unwrap_or(0.0), optional("y")?.unwrap_or(0.0));
                let (width, height) = (required("width")?, required("height")?);
                if width < 0.0 {
                    return Err(negative("width", width));
                }
                if height < 0.0 {
                    return Err(negative("height", height));
                }
                // As in SVG: one radius given stands for both.
                let (rx, ry) = match (optional("rx")?, optional("ry")?) {
                    (Some(rx), Some(ry)) => (rx, ry),
                    (Some(radius), None) | (None, Some(radius)) => (radius, radius),
                    (None, None) => (0.0, 0.0),
                };
                if rx < 0.0 || ry < 0.0 {
                    return Err(negative("rx", rx.min(ry)));
                }
                if rx > 0.0 && ry > 0.0 {
                    commands.add_round_rect(origin, width, height, rx, ry);
                } else {
                    commands.add_rect(origin, width, height);
                }
            }
            _ => {
                commands.move_to((
                    optional("x1")?.unwrap_or(0.0),
                    optional("y1")?.unwrap_or(0.0),
                ));
                commands.line_to((
                    optional("x2")?.unwrap_or(0.0),
                    optional("y2")?.unwrap_or(0.0),
                ));
                // A line has no interior.
                state.fill = None;
            }
        }

        if !tag.self_closing {
            match reader.tag()? {
                Some(end) if end.closing && end.name == tag.name => {}
                Some(child) if !child.closing => {
                    return Err(IconError::UnsupportedElement(child.name.to_owned()));
                }
                _ => return Err(reader.error("expected the element's closing tag")),
            }
        }

        let stroke = state.stroke.filter(|_| state.width > 0.0);
        for paint in [state.fill, stroke].into_iter().flatten() {
            let paint = paint.to_ascii_lowercase();
            match &ink {
                Some(ink) if *ink != paint => return Err(IconError::MultipleColors),
                Some(_) => {}
                None => ink = Some(paint),
            }
        }
        if state.fill.is_some() || stroke.is_some() {
            shapes.push(Shape {
                commands,
                fill: state.fill.is_some(),
                stroke: stroke.map(|_| StrokeStyle {
                    width: state.width,
                    cap: state.cap,
                    join: state.join,
                }),
            });
        }
    }
    if reader.tag()?.is_some() {
        return Err(reader.error("unexpected content after `</svg>`"));
    }
    if shapes.is_empty() {
        return Err(IconError::Empty);
    }
    Ok(Art { view_box, shapes })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str =
        r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">"#;

    fn svg(body: &str) -> Result<Art, IconError> {
        parse_svg(&format!("{HEAD}{body}</svg>"))
    }

    fn at(mask: &[u8], width: u32, x: u32, y: u32) -> u8 {
        mask[(y * width + x) as usize]
    }

    #[test]
    fn a_filled_rect_covers_exactly_its_pixels() {
        let art =
            parse_svg(r#"<svg viewBox="0 0 8 8"><rect x="2" y="1" width="4" height="3"/></svg>"#)
                .unwrap();
        // Twice the view box: the rectangle is at 4,2 and 8 x 6 pixels.
        let mask = art.rasterise(16, 16, None);
        for y in 0..16 {
            for x in 0..16 {
                let inside = (4..12).contains(&x) && (2..8).contains(&y);
                assert_eq!(at(&mask, 16, x, y), if inside { 255 } else { 0 }, "{x},{y}");
            }
        }
    }

    #[test]
    fn a_stroke_is_centred_on_its_path_and_scales_with_the_icon() {
        let art = svg(r#"<line x1="4" y1="12" x2="20" y2="12"/>"#).unwrap();
        for scale in [1u32, 2, 3] {
            let size = 24 * scale;
            let mask = art.rasterise(size, size, None);
            let x = size / 2;
            // Two units wide, centred on y = 12: rows 11 and 12 at scale 1.
            for y in 0..size {
                let inside = (11 * scale..13 * scale).contains(&y);
                assert_eq!(at(&mask, size, x, y), if inside { 255 } else { 0 }, "{y}");
            }
            // Butt caps: nothing before x = 4.
            assert_eq!(at(&mask, size, 4 * scale - 1, 12 * scale), 0);
            assert_eq!(at(&mask, size, 4 * scale, 12 * scale), 255);
        }
    }

    #[test]
    fn round_caps_extend_the_line_by_half_its_width() {
        let butt = svg(r#"<line x1="4" y1="12" x2="20" y2="12"/>"#).unwrap();
        let round =
            svg(r#"<line x1="4" y1="12" x2="20" y2="12" stroke-linecap="round"/>"#).unwrap();
        let (butt, round) = (butt.rasterise(48, 48, None), round.rasterise(48, 48, None));
        // One pixel left of the line's start, on its centre line.
        assert_eq!(at(&butt, 48, 7, 23), 0);
        assert_eq!(at(&round, 48, 7, 23), 255);
        // And nothing beyond the cap's radius (2 px at this scale).
        assert_eq!(at(&round, 48, 5, 23), 0);
    }

    #[test]
    fn the_stroke_width_can_be_overridden() {
        let art = svg(r#"<line x1="4" y1="12" x2="20" y2="12"/>"#).unwrap();
        let rows = |mask: &[u8]| (0..24).filter(|&y| at(mask, 24, 12, y) == 255).count();
        assert_eq!(rows(&art.rasterise(24, 24, None)), 2);
        assert_eq!(rows(&art.rasterise(24, 24, Some(4.0))), 4);
        // Nonsense widths are ignored.
        assert_eq!(rows(&art.rasterise(24, 24, Some(f32::NAN))), 2);
        assert_eq!(rows(&art.rasterise(24, 24, Some(-1.0))), 2);
    }

    #[test]
    fn a_circle_is_round_and_antialiased() {
        let art =
            parse_svg(r#"<svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="8"/></svg>"#).unwrap();
        let mask = art.rasterise(48, 48, None);
        let area: f32 = mask.iter().map(|&value| f32::from(value) / 255.0).sum();
        let expected = std::f32::consts::PI * 16.0 * 16.0;
        assert!(
            (area - expected).abs() / expected < 0.01,
            "{area} vs {expected}"
        );
        assert_eq!(at(&mask, 48, 24, 24), 255);
        assert_eq!(at(&mask, 48, 2, 2), 0);
        assert!(mask.iter().any(|&value| value > 0 && value < 255));
    }

    #[test]
    fn a_non_square_target_centres_the_view_box() {
        let art =
            parse_svg(r#"<svg viewBox="0 0 8 8"><rect width="8" height="8"/></svg>"#).unwrap();
        let mask = art.rasterise(16, 8, None);
        for x in 0..16 {
            let inside = (4..12).contains(&x);
            assert_eq!(at(&mask, 16, x, 4), if inside { 255 } else { 0 }, "{x}");
        }
    }

    #[test]
    fn a_view_box_origin_is_respected() {
        let art = parse_svg(
            r#"<svg viewBox="10 20 8 8"><rect x="10" y="20" width="4" height="4"/></svg>"#,
        )
        .unwrap();
        let mask = art.rasterise(8, 8, None);
        assert_eq!(at(&mask, 8, 0, 0), 255);
        assert_eq!(at(&mask, 8, 3, 3), 255);
        assert_eq!(at(&mask, 8, 4, 4), 0);
    }

    #[test]
    fn overlapping_shapes_do_not_exceed_full_coverage() {
        let art = parse_svg(
            r#"<svg viewBox="0 0 8 8"><rect width="6" height="6"/><rect x="2" y="2" width="6" height="6"/></svg>"#,
        )
        .unwrap();
        let mask = art.rasterise(8, 8, None);
        assert_eq!(at(&mask, 8, 3, 3), 255);
        assert_eq!(at(&mask, 8, 0, 0), 255);
        assert_eq!(at(&mask, 8, 7, 7), 255);
        assert_eq!(at(&mask, 8, 7, 0), 0);
    }

    #[test]
    fn fill_and_stroke_on_one_shape_both_paint() {
        let art = parse_svg(
            r#"<svg viewBox="0 0 24 24" fill="currentColor" stroke="currentColor" stroke-width="4">
                 <rect x="8" y="8" width="8" height="8"/></svg>"#,
        )
        .unwrap();
        let mask = art.rasterise(24, 24, None);
        assert_eq!(at(&mask, 24, 12, 12), 255, "the interior");
        assert_eq!(at(&mask, 24, 6, 12), 255, "the outer half of the stroke");
        assert_eq!(at(&mask, 24, 5, 12), 0);
    }

    #[test]
    fn presentation_is_inherited_and_overridden_per_shape() {
        let art = svg(
            r#"<path d="M2 2h4"/><circle cx="12" cy="12" r="3" fill="currentColor" stroke="none"/>"#,
        )
        .unwrap();
        assert!(!art.shapes[0].fill && art.shapes[0].stroke.is_some());
        assert!(art.shapes[1].fill && art.shapes[1].stroke.is_none());
        assert_eq!(art.shapes[0].stroke.unwrap().width, 2.0);
    }

    #[test]
    fn svg_defaults_are_a_black_fill_and_no_stroke() {
        let art = parse_svg(r#"<svg viewBox="0 0 4 4"><path d="M0 0h4v4z"/></svg>"#).unwrap();
        assert!(art.shapes[0].fill && art.shapes[0].stroke.is_none());
    }

    #[test]
    fn compact_arc_flags_parse() {
        // "0 0 0-5.2 5.2": flags run into the next number, as minifiers
        // write them.
        let icon = VectorIcon::stroked(
            "M14.7 6.3a4 4 0 0 0-5.2 5.2L3 18v3h3l6.5-6.5a4 4 0 0 0 5.2-5.2l-2.6 2.6-2.4-.6-.6-2.4z",
        );
        assert_eq!(icon.check(), Ok(()));
        assert_eq!(icon.aspect_ratio(), Some(1.0));
    }

    fn points(data: &str) -> Vec<(f32, f32)> {
        path_commands(data)
            .unwrap()
            .iter()
            .filter_map(|command| match *command {
                Command::MoveTo(p) | Command::LineTo(p) => Some((p.x, p.y)),
                Command::CurveTo(_, _, p) | Command::QuadTo(_, p) => Some((p.x, p.y)),
                Command::Close => None,
            })
            .collect()
    }

    #[test]
    fn path_data_follows_the_svg_grammar() {
        // Relative and absolute forms, implicit lines after a move,
        // horizontal and vertical lines.
        assert_eq!(
            points("M1 2 3 4l1 1H10V20h-2v-3z"),
            [
                (1.0, 2.0),
                (3.0, 4.0),
                (4.0, 5.0),
                (10.0, 5.0),
                (10.0, 20.0),
                (8.0, 20.0),
                (8.0, 17.0)
            ]
        );
        // Numbers run together: signs, second decimal points, exponents.
        assert_eq!(
            points("M.5.5-1-1L1e1,2E0"),
            [(0.5, 0.5), (-1.0, -1.0), (10.0, 2.0)]
        );
        // A relative move after a close starts from the subpath's start.
        assert_eq!(
            points("M2 2h4zm1 1h1"),
            [(2.0, 2.0), (6.0, 2.0), (3.0, 3.0), (4.0, 3.0)]
        );
        // Arc flags need no separators; the arc ends where it says.
        let arc = points("M4 20a8 8 0 1 1 4-1.3");
        assert_eq!(arc[0], (4.0, 20.0));
        let end = arc[arc.len() - 1];
        assert!((end.0 - 8.0).abs() < 1e-4 && (end.1 - 18.7).abs() < 1e-4);
        let compact = points("M14.7 6.3a4 4 0 00-5.2 5.2");
        let end = compact[compact.len() - 1];
        assert!((end.0 - 9.5).abs() < 1e-4 && (end.1 - 11.5).abs() < 1e-4);
    }

    #[test]
    fn smooth_curves_mirror_the_previous_control_point() {
        // The eye of the design: `s` twice in a row, then `S`.
        let commands = path_commands("M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12z").unwrap();
        let curves: Vec<_> = commands
            .iter()
            .filter_map(|command| match *command {
                Command::CurveTo(a, b, c) => Some([(a.x, a.y), (b.x, b.y), (c.x, c.y)]),
                _ => None,
            })
            .collect();
        let expected = [
            // No curve before it: the first control is the start.
            [(2.0, 12.0), (5.6, 5.0), (12.0, 5.0)],
            // Mirror of (5.6, 5) in (12, 5).
            [(18.4, 5.0), (22.0, 12.0), (22.0, 12.0)],
            [(22.0, 12.0), (18.4, 19.0), (12.0, 19.0)],
            [(5.6, 19.0), (2.0, 12.0), (2.0, 12.0)],
        ];
        assert_eq!(curves.len(), expected.len());
        for (curve, expected) in curves.iter().zip(expected) {
            for (point, expected) in curve.iter().zip(expected) {
                let near =
                    (point.0 - expected.0).abs() < 1e-4 && (point.1 - expected.1).abs() < 1e-4;
                assert!(near, "{point:?} vs {expected:?}");
            }
        }
        // `T` mirrors a quadratic's control; after a line it does not.
        let quads = path_commands("M0 0Q5 10 10 0T20 0L30 0T40 0").unwrap();
        assert!(matches!(quads[2], Command::QuadTo(c, _) if (c.x, c.y) == (15.0, -10.0)));
        assert!(matches!(quads[4], Command::QuadTo(c, _) if (c.x, c.y) == (30.0, 0.0)));
    }

    #[test]
    fn malformed_path_data_is_refused_with_its_position() {
        for data in [
            "L1 1",
            "1 2",
            "M1",
            "M1 2L3",
            "M1 2X3 4",
            "M0 0a1 1 0 2 0 1 1",
            "M0 0z1 1",
            "M0 0L1 oops",
            "M0 0L1 .",
        ] {
            assert!(
                matches!(path_commands(data), Err(IconError::InvalidPath { .. })),
                "{data}"
            );
        }
        // The position is where reading stopped.
        assert_eq!(
            path_commands("M0 0L1 oops").map(|_| ()),
            Err(IconError::InvalidPath { offset: 7 })
        );
        assert_eq!(path_commands("  ").map(|_| ()), Err(IconError::Empty));
    }

    #[test]
    fn everything_outside_the_subset_is_refused_by_name() {
        let unsupported_element = |name: &str| Err(IconError::UnsupportedElement(name.to_owned()));
        assert_eq!(
            svg(r#"<g><path d="M0 0h4"/></g>"#).map(|_| ()),
            unsupported_element("g")
        );
        assert_eq!(
            svg(r#"<polygon points="0,0 4,0 4,4"/>"#).map(|_| ()),
            unsupported_element("polygon")
        );
        assert_eq!(
            svg(r#"<ellipse cx="4" cy="4" rx="2" ry="1"/>"#).map(|_| ()),
            unsupported_element("ellipse")
        );
        assert_eq!(
            svg(r#"<path d="M0 0h4"><title>x</title></path>"#).map(|_| ()),
            unsupported_element("title")
        );

        let attribute = |element: &str, attribute: &str| {
            Err(IconError::UnsupportedAttribute {
                element: element.to_owned(),
                attribute: attribute.to_owned(),
            })
        };
        for name in [
            "transform",
            "opacity",
            "stroke-dasharray",
            "fill-rule",
            "style",
            "stroke-opacity",
            "clip-path",
            "width",
        ] {
            assert_eq!(
                svg(&format!(r#"<path d="M0 0h4" {name}="1"/>"#)).map(|_| ()),
                attribute("path", name)
            );
        }
        assert_eq!(
            parse_svg(r#"<svg viewBox="0 0 4 4" transform="scale(2)"/>"#).map(|_| ()),
            attribute("svg", "transform")
        );
    }

    #[test]
    fn bad_values_are_refused() {
        let value = |attribute: &str, value: &str| {
            Err(IconError::UnsupportedValue {
                attribute: attribute.to_owned(),
                value: value.to_owned(),
            })
        };
        assert_eq!(
            svg(r#"<circle cx="50%" cy="4" r="2"/>"#).map(|_| ()),
            value("cx", "50%")
        );
        assert_eq!(
            svg(r#"<rect width="2px" height="2"/>"#).map(|_| ()),
            value("width", "2px")
        );
        assert_eq!(
            svg(r#"<path d="M0 0h4" stroke-linecap="pointy"/>"#).map(|_| ()),
            value("stroke-linecap", "pointy")
        );
        assert_eq!(
            svg(r#"<path d="M0 0h4" fill="url(#g)"/>"#).map(|_| ()),
            value("fill", "url(#g)")
        );
        assert_eq!(
            parse_svg(r#"<svg viewBox="0 0 0 4"/>"#).map(|_| ()),
            value("viewBox", "0 0 0 4")
        );
        assert_eq!(svg(r#"<circle r="-1"/>"#).map(|_| ()), value("r", "-1"));
    }

    #[test]
    fn structural_errors_are_reported() {
        assert_eq!(parse_svg("").map(|_| ()), Err(IconError::MissingViewBox));
        assert_eq!(
            parse_svg(r#"<svg width="24"><path d="M0 0h4"/></svg>"#).map(|_| ()),
            Err(IconError::MissingViewBox)
        );
        assert_eq!(
            parse_svg(r#"<path d="M0 0h4"/>"#).map(|_| ()),
            Err(IconError::MissingViewBox)
        );
        assert_eq!(svg("").map(|_| ()), Err(IconError::Empty));
        assert_eq!(
            svg(r#"<path d="M0 0h4" stroke="none"/>"#).map(|_| ()),
            Err(IconError::Empty)
        );
        assert_eq!(
            svg("<circle cx=\"1\"/>").map(|_| ()),
            Err(IconError::MissingAttribute {
                element: "circle".to_owned(),
                attribute: "r"
            })
        );
        assert!(matches!(
            svg(r#"<path d="M0 0 L 5 oops"/>"#),
            Err(IconError::InvalidPath { .. })
        ));
        assert!(matches!(svg("hello"), Err(IconError::Syntax { .. })));
        assert!(matches!(
            parse_svg(r#"<svg viewBox="0 0 4 4"><path d="M0 0h4"/>"#),
            Err(IconError::Syntax { .. })
        ));
        assert!(matches!(
            svg(r#"<path d="M0 0h4"#),
            Err(IconError::Syntax { .. })
        ));
        assert!(matches!(
            parse_svg(r#"<svg viewBox="0 0 4 4"><path d="M0 0h4"/></svg><svg/>"#),
            Err(IconError::Syntax { .. })
        ));
    }

    #[test]
    fn two_inks_are_refused_and_one_literal_ink_is_accepted() {
        let logo = r##"<svg viewBox="0 0 88 88" fill="none">
            <rect width="88" height="88" rx="24" fill="#FFFFFF"/>
            <rect x="29" y="18" width="30" height="52" rx="7" stroke="#1E8416" stroke-width="5"/>
          </svg>"##;
        assert_eq!(parse_svg(logo).map(|_| ()), Err(IconError::MultipleColors));

        let one = r##"<svg viewBox="0 0 24 24" fill="none" stroke="#5B6660" stroke-width="1.8">
            <!-- a comment -->
            <rect x="6" y="2" width="12" height="20" rx="3"></rect><path d='M11 18h2'></path>
          </svg>"##;
        assert_eq!(parse_svg(one).map(|art| art.shapes.len()), Ok(2));
        // Case does not make a second ink.
        let mixed = r##"<svg viewBox="0 0 4 4" fill="#abcdef"><path d="M0 0h4v4z"/><path d="M0 0h1v1z" fill="#ABCDEF"/></svg>"##;
        assert!(parse_svg(mixed).is_ok());
    }

    #[test]
    fn errors_say_what_is_wrong() {
        let message = svg(r#"<g/>"#).unwrap_err().to_string();
        assert!(message.contains("<g>") && message.contains("path, circle, rect and line"));
        let message = svg(r#"<path d="M0 0h4" transform="x"/>"#)
            .unwrap_err()
            .to_string();
        assert!(message.contains("transform") && message.contains("<path>"));
    }
}
