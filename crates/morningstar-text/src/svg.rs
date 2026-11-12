//! Loading SVG files: a practical static subset, read by a small strict
//! reader into an owned picture that is rasterised by the same compositor
//! as a [`VectorPicture`](crate::VectorPicture). See ADR 0027.
//!
//! The input is treated as untrusted: every count, depth and number is
//! bounded, nothing panics, and whatever is outside the subset is an
//! [`SvgError`] that names the element or attribute and where it is, rather
//! than a picture that silently differs from the file.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, OnceLock};

use morningstar_core::{Color, Point, Transform};
use zeno::{Command, PathBuilder};

use crate::icon::{IconError, path_commands};
use crate::picture::{
    Canvas, ClipPart, DEFAULT_MITER_LIMIT, GradientInk, GradientShape, Ink, LineCap, LineJoin,
    Paint, Spread, StrokePaint,
};

/// Longest SVG source accepted, in bytes.
pub const MAX_SVG_BYTES: usize = 4 * 1024 * 1024;
/// Deepest nesting of elements.
const MAX_DEPTH: usize = 64;
/// Most elements in one document.
const MAX_ELEMENTS: usize = 20_000;
/// Most attributes on one element.
const MAX_ATTRIBUTES: usize = 128;
/// Most path commands in one document, after arcs are turned into curves.
const MAX_PATH_COMMANDS: usize = 200_000;
/// Largest magnitude of a coordinate, a length or a matrix entry.
const MAX_NUMBER: f32 = 1.0e6;
/// Most stops in a gradient and entries in a dash array.
const MAX_STOPS: usize = 256;
const MAX_DASHES: usize = 64;
/// Most dashes along one path.
const MAX_DASH_COUNT: f32 = 100_000.0;
/// Deepest nesting of groups that need a canvas of their own (a faded or
/// clipped group with more than one child): each costs 16 bytes per pixel
/// while the picture is rasterised.
const MAX_ISOLATED_GROUPS: usize = 4;
/// Longest chain of gradients referring to one another.
const MAX_GRADIENT_CHAIN: usize = 8;
/// Most notes [`SvgPicture::parse_lenient`] returns.
const MAX_WARNINGS: usize = 256;

/// Why an SVG file cannot be loaded, and where in the file.
#[derive(Clone, Debug, PartialEq)]
pub struct SvgError {
    /// What is wrong.
    pub kind: SvgErrorKind,
    /// Byte offset into the source of the tag or attribute concerned.
    pub offset: usize,
    /// Line of that position, counted from 1.
    pub line: u32,
    /// Column of that position in characters, counted from 1.
    pub column: u32,
}

/// What is wrong with an SVG file.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum SvgErrorKind {
    /// The markup is not well-formed XML, or uses a part of XML this
    /// reader does not read (CDATA, a DOCTYPE with declarations).
    Syntax(&'static str),
    /// An element outside the subset: `<text>`, `<use>`, `<mask>`,
    /// `<filter>`, `<image>`, `<style>`, `<pattern>`, animation.
    UnsupportedElement(String),
    /// An attribute, or a property in a `style` attribute (written
    /// `style: name`), outside the subset.
    UnsupportedAttribute {
        /// The element it was found on.
        element: String,
        /// The attribute's name.
        attribute: String,
    },
    /// A value that is not understood: a length with a unit, an unknown
    /// colour, a paint server of an unsupported kind.
    UnsupportedValue {
        /// The element.
        element: String,
        /// The attribute or property.
        attribute: String,
        /// The value, shortened if long.
        value: String,
    },
    /// An element lacks an attribute it cannot be drawn without.
    MissingAttribute {
        /// The element.
        element: String,
        /// The attribute it needs.
        attribute: &'static str,
    },
    /// `url(#id)` names nothing in the file.
    UnknownReference {
        /// The element that refers to it.
        element: String,
        /// The id that was not found.
        id: String,
    },
    /// Path data that does not parse; `offset` is the byte within the
    /// `d` attribute's value.
    InvalidPath {
        /// Byte offset into the path data.
        offset: usize,
    },
    /// There is no `<svg>` root, or it has neither a `viewBox` nor a
    /// `width` and `height` in pixels.
    NoSize,
    /// A bound on untrusted input was exceeded: nesting, the number of
    /// elements, path commands, a number's range.
    Limit(&'static str),
}

impl fmt::Display for SvgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SVG line {}, column {}: ", self.line, self.column)?;
        match &self.kind {
            SvgErrorKind::Syntax(message) => write!(f, "{message}"),
            SvgErrorKind::UnsupportedElement(name) => {
                write!(f, "<{name}> is not supported")
            }
            SvgErrorKind::UnsupportedAttribute { element, attribute } => {
                write!(f, "`{attribute}` on <{element}> is not supported")
            }
            SvgErrorKind::UnsupportedValue {
                element,
                attribute,
                value,
            } => write!(
                f,
                "`{attribute}=\"{value}\"` on <{element}> is not supported"
            ),
            SvgErrorKind::MissingAttribute { element, attribute } => {
                write!(f, "<{element}> needs a `{attribute}` attribute")
            }
            SvgErrorKind::UnknownReference { element, id } => {
                write!(f, "<{element}> refers to `#{id}`, which is not in the file")
            }
            SvgErrorKind::InvalidPath { offset } => {
                write!(f, "invalid path data at byte {offset} of `d`")
            }
            SvgErrorKind::NoSize => {
                write!(f, "no <svg> element with a viewBox or a width and height")
            }
            SvgErrorKind::Limit(what) => write!(f, "limit exceeded: {what}"),
        }
    }
}

impl std::error::Error for SvgError {}

fn error_at(source: &str, offset: usize, kind: SvgErrorKind) -> SvgError {
    let offset = offset.min(source.len());
    let mut end = offset;
    while !source.is_char_boundary(end) {
        end -= 1;
    }
    let before = &source[..end];
    let line = before.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = before
        .rfind('\n')
        .map_or(before, |index| &before[index + 1..])
        .chars()
        .count()
        + 1;
    SvgError {
        kind,
        offset,
        line: line.min(u32::MAX as usize) as u32,
        column: column.min(u32::MAX as usize) as u32,
    }
}

/// A value as it goes into an error: short.
fn shortened(value: &str) -> String {
    let mut text: String = value.chars().take(60).collect();
    if text.len() < value.len() {
        text.push('…');
    }
    text
}

// ------------------------------------------------------------------- XML

struct Attribute<'a> {
    name: &'a str,
    value: Cow<'a, str>,
    offset: usize,
}

struct Element<'a> {
    name: &'a str,
    attributes: Vec<Attribute<'a>>,
    children: Vec<usize>,
    offset: usize,
}

impl<'a> Element<'a> {
    fn attribute(&self, name: &str) -> Option<&Attribute<'a>> {
        self.attributes
            .iter()
            .find(|attribute| attribute.name == name)
    }
}

struct XmlReader<'a> {
    source: &'a str,
    position: usize,
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b':'
}

fn is_name_byte(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit() || byte == b'-' || byte == b'.'
}

impl<'a> XmlReader<'a> {
    fn error(&self, message: &'static str) -> SvgError {
        error_at(self.source, self.position, SvgErrorKind::Syntax(message))
    }

    fn rest(&self) -> &'a str {
        &self.source[self.position..]
    }

    fn skip_whitespace(&mut self) {
        let bytes = self.source.as_bytes();
        while bytes
            .get(self.position)
            .is_some_and(|byte| matches!(byte, b' ' | b'\t' | b'\n' | b'\r'))
        {
            self.position += 1;
        }
    }

    fn name(&mut self) -> Option<&'a str> {
        let bytes = self.source.as_bytes();
        let start = self.position;
        if !bytes.get(start).copied().is_some_and(is_name_start) {
            return None;
        }
        let mut end = start + 1;
        while bytes.get(end).copied().is_some_and(is_name_byte) {
            end += 1;
        }
        self.position = end;
        Some(&self.source[start..end])
    }

    /// Skips a comment, a processing instruction or a DOCTYPE at the
    /// position, which is at `<!` or `<?`.
    fn skip_markup(&mut self) -> Result<(), SvgError> {
        let rest = self.rest();
        if let Some(comment) = rest.strip_prefix("<!--") {
            let end = comment
                .find("-->")
                .ok_or_else(|| self.error("unclosed comment"))?;
            self.position += 4 + end + 3;
        } else if rest.starts_with("<?") {
            let end = rest
                .find("?>")
                .ok_or_else(|| self.error("unclosed processing instruction"))?;
            self.position += end + 2;
        } else if rest.starts_with("<![CDATA[") {
            return Err(self.error("CDATA sections are not supported"));
        } else if rest.len() >= 9 && rest[..9].eq_ignore_ascii_case("<!DOCTYPE") {
            let end = rest
                .find('>')
                .ok_or_else(|| self.error("unclosed DOCTYPE"))?;
            if rest[..end].contains('[') {
                return Err(self.error("a DOCTYPE with declarations is not supported"));
            }
            self.position += end + 1;
        } else {
            return Err(self.error("unexpected markup declaration"));
        }
        Ok(())
    }

    /// Replaces the predefined entities and character references.
    fn unescape(&self, raw: &'a str, offset: usize) -> Result<Cow<'a, str>, SvgError> {
        if !raw.contains('&') {
            return Ok(Cow::Borrowed(raw));
        }
        let fail = |at: usize, message: &'static str| {
            error_at(self.source, offset + at, SvgErrorKind::Syntax(message))
        };
        let mut text = String::with_capacity(raw.len());
        let mut cursor = 0;
        while let Some(found) = raw[cursor..].find('&') {
            let start = cursor + found;
            text.push_str(&raw[cursor..start]);
            let end = raw[start..]
                .find(';')
                .filter(|length| *length <= 12)
                .ok_or_else(|| fail(start, "`&` that does not begin an entity"))?;
            let entity = &raw[start + 1..start + end];
            let character = match entity {
                "lt" => '<',
                "gt" => '>',
                "amp" => '&',
                "quot" => '"',
                "apos" => '\'',
                _ => {
                    let code = if let Some(hex) = entity
                        .strip_prefix("#x")
                        .or_else(|| entity.strip_prefix("#X"))
                    {
                        u32::from_str_radix(hex, 16).ok()
                    } else if let Some(decimal) = entity.strip_prefix('#') {
                        decimal.parse::<u32>().ok()
                    } else {
                        return Err(fail(start, "unknown entity"));
                    };
                    code.and_then(char::from_u32)
                        .ok_or_else(|| fail(start, "invalid character reference"))?
                }
            };
            text.push(character);
            cursor = start + end + 1;
        }
        text.push_str(&raw[cursor..]);
        Ok(Cow::Owned(text))
    }

    /// Reads the whole document into elements; the root is element 0.
    fn document(mut self) -> Result<Vec<Element<'a>>, SvgError> {
        if self.source.len() > MAX_SVG_BYTES {
            return Err(error_at(
                self.source,
                0,
                SvgErrorKind::Limit("the file is longer than 4 MiB"),
            ));
        }
        if self.source.starts_with('\u{feff}') {
            self.position = 3;
        }
        let mut elements: Vec<Element<'a>> = Vec::new();
        let mut open: Vec<usize> = Vec::new();
        let mut finished = false;
        loop {
            // Character data: nothing in the subset renders it.
            let rest = self.rest();
            let Some(next) = rest.find('<') else {
                if !open.is_empty() {
                    self.position = self.source.len();
                    return Err(self.error("the file ends inside an element"));
                }
                if !rest.trim().is_empty() {
                    return Err(self.error("text outside the root element"));
                }
                break;
            };
            if open.is_empty() && !rest[..next].trim().is_empty() {
                return Err(self.error("text outside the root element"));
            }
            self.position += next;
            let rest = self.rest();
            if rest.starts_with("<!") || rest.starts_with("<?") {
                self.skip_markup()?;
                continue;
            }
            if rest.starts_with("</") {
                self.position += 2;
                let name = self
                    .name()
                    .ok_or_else(|| self.error("expected an element name"))?;
                self.skip_whitespace();
                if !self.rest().starts_with('>') {
                    return Err(self.error("expected `>`"));
                }
                self.position += 1;
                match open.pop() {
                    Some(index) if elements[index].name == name => {}
                    _ => return Err(self.error("closing tag does not match the open element")),
                }
                finished = open.is_empty();
                continue;
            }

            let offset = self.position;
            self.position += 1;
            let name = self
                .name()
                .ok_or_else(|| self.error("expected an element name"))?;
            if finished {
                self.position = offset;
                return Err(self.error("a second root element"));
            }
            if open.len() >= MAX_DEPTH {
                return Err(error_at(
                    self.source,
                    offset,
                    SvgErrorKind::Limit("elements nested more than 64 deep"),
                ));
            }
            if elements.len() >= MAX_ELEMENTS {
                return Err(error_at(
                    self.source,
                    offset,
                    SvgErrorKind::Limit("more than 20000 elements"),
                ));
            }
            let mut attributes: Vec<Attribute<'a>> = Vec::new();
            let self_closing = loop {
                let before = self.position;
                self.skip_whitespace();
                let rest = self.rest();
                if rest.starts_with("/>") {
                    self.position += 2;
                    break true;
                }
                if rest.starts_with('>') {
                    self.position += 1;
                    break false;
                }
                if self.position == before {
                    return Err(self.error("expected whitespace, `>` or `/>`"));
                }
                let at = self.position;
                let attribute = self
                    .name()
                    .ok_or_else(|| self.error("expected an attribute name, `>` or `/>`"))?;
                self.skip_whitespace();
                if !self.rest().starts_with('=') {
                    return Err(self.error("expected `=` after an attribute name"));
                }
                self.position += 1;
                self.skip_whitespace();
                let quote = match self.rest().as_bytes().first() {
                    Some(quote @ (b'"' | b'\'')) => *quote as char,
                    _ => return Err(self.error("expected a quoted attribute value")),
                };
                self.position += 1;
                let rest = self.rest();
                let end = rest
                    .find(quote)
                    .ok_or_else(|| self.error("unclosed attribute value"))?;
                let raw = &rest[..end];
                if raw.contains('<') {
                    return Err(self.error("`<` in an attribute value"));
                }
                if attributes.iter().any(|known| known.name == attribute) {
                    self.position = at;
                    return Err(self.error("attribute given twice"));
                }
                if attributes.len() >= MAX_ATTRIBUTES {
                    return Err(error_at(
                        self.source,
                        at,
                        SvgErrorKind::Limit("more than 128 attributes on an element"),
                    ));
                }
                let value = self.unescape(raw, self.position)?;
                attributes.push(Attribute {
                    name: attribute,
                    value,
                    offset: at,
                });
                self.position += end + 1;
            };
            let index = elements.len();
            elements.push(Element {
                name,
                attributes,
                children: Vec::new(),
                offset,
            });
            if let Some(&parent) = open.last() {
                elements[parent].children.push(index);
            }
            if self_closing {
                finished = open.is_empty();
            } else {
                open.push(index);
            }
        }
        if elements.is_empty() {
            return Err(error_at(self.source, 0, SvgErrorKind::NoSize));
        }
        Ok(elements)
    }
}

// ---------------------------------------------------------------- values

/// Reads numbers written as SVG writes lists of them: separated by
/// whitespace or commas, or by nothing where a sign or a point starts the
/// next one.
struct Numbers<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Numbers<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            bytes: text.as_bytes(),
            position: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }

    fn skip_whitespace(&mut self) {
        while self.peek().is_some_and(|byte| byte.is_ascii_whitespace()) {
            self.position += 1;
        }
    }

    /// Whitespace and at most one comma.
    fn skip_separator(&mut self) {
        self.skip_whitespace();
        if self.peek() == Some(b',') {
            self.position += 1;
            self.skip_whitespace();
        }
    }

    fn at_end(&mut self) -> bool {
        self.skip_whitespace();
        self.position >= self.bytes.len()
    }

    fn digits(&mut self) -> usize {
        let from = self.position;
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.position += 1;
        }
        self.position - from
    }

    /// The next number, finite and within [`MAX_NUMBER`].
    fn number(&mut self) -> Option<f32> {
        self.skip_whitespace();
        let start = self.position;
        if matches!(self.peek(), Some(b'+' | b'-')) {
            self.position += 1;
        }
        let mut count = self.digits();
        if self.peek() == Some(b'.') {
            self.position += 1;
            count += self.digits();
        }
        if count == 0 {
            self.position = start;
            return None;
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            let mark = self.position;
            self.position += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.position += 1;
            }
            if self.digits() == 0 {
                self.position = mark;
            }
        }
        let number = std::str::from_utf8(&self.bytes[start..self.position])
            .ok()
            .and_then(|text| text.parse::<f32>().ok())
            .filter(|number| number.is_finite() && number.abs() <= MAX_NUMBER);
        if number.is_none() {
            self.position = start;
        }
        number
    }

    fn rest(&self) -> &'a str {
        std::str::from_utf8(&self.bytes[self.position.min(self.bytes.len())..]).unwrap_or("")
    }
}

/// A number alone, optionally in `px`.
fn length(value: &str) -> Option<f32> {
    let value = value.trim();
    let value = value.strip_suffix("px").unwrap_or(value);
    let mut numbers = Numbers::new(value);
    let number = numbers.number()?;
    numbers.at_end().then_some(number)
}

/// A number or a percentage, as a fraction: opacities and stop offsets.
fn fraction(value: &str) -> Option<f32> {
    let value = value.trim();
    match value.strip_suffix('%') {
        Some(percent) => length_plain(percent).map(|number| number / 100.0),
        None => length_plain(value),
    }
}

/// A number alone, with no unit.
fn length_plain(value: &str) -> Option<f32> {
    let mut numbers = Numbers::new(value);
    let number = numbers.number()?;
    numbers.at_end().then_some(number)
}

/// A gradient coordinate: a number, or a share of the reference length.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Coordinate {
    Number(f32),
    Percent(f32),
}

impl Coordinate {
    fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        match value.strip_suffix('%') {
            Some(percent) => length_plain(percent).map(Self::Percent),
            None => length(value).map(Self::Number),
        }
    }

    /// The value in bounding-box units, or in user units where a
    /// percentage is of `reference`.
    fn resolve(self, bounding_box: bool, reference: f32) -> f32 {
        match self {
            Self::Number(number) => number,
            Self::Percent(percent) if bounding_box => percent / 100.0,
            Self::Percent(percent) => percent / 100.0 * reference,
        }
    }
}

/// The CSS named colours.
const NAMED_COLORS: &[(&str, u32)] = &[
    ("aliceblue", 0xf0f8ff),
    ("antiquewhite", 0xfaebd7),
    ("aqua", 0x00ffff),
    ("aquamarine", 0x7fffd4),
    ("azure", 0xf0ffff),
    ("beige", 0xf5f5dc),
    ("bisque", 0xffe4c4),
    ("black", 0x000000),
    ("blanchedalmond", 0xffebcd),
    ("blue", 0x0000ff),
    ("blueviolet", 0x8a2be2),
    ("brown", 0xa52a2a),
    ("burlywood", 0xdeb887),
    ("cadetblue", 0x5f9ea0),
    ("chartreuse", 0x7fff00),
    ("chocolate", 0xd2691e),
    ("coral", 0xff7f50),
    ("cornflowerblue", 0x6495ed),
    ("cornsilk", 0xfff8dc),
    ("crimson", 0xdc143c),
    ("cyan", 0x00ffff),
    ("darkblue", 0x00008b),
    ("darkcyan", 0x008b8b),
    ("darkgoldenrod", 0xb8860b),
    ("darkgray", 0xa9a9a9),
    ("darkgreen", 0x006400),
    ("darkgrey", 0xa9a9a9),
    ("darkkhaki", 0xbdb76b),
    ("darkmagenta", 0x8b008b),
    ("darkolivegreen", 0x556b2f),
    ("darkorange", 0xff8c00),
    ("darkorchid", 0x9932cc),
    ("darkred", 0x8b0000),
    ("darksalmon", 0xe9967a),
    ("darkseagreen", 0x8fbc8f),
    ("darkslateblue", 0x483d8b),
    ("darkslategray", 0x2f4f4f),
    ("darkslategrey", 0x2f4f4f),
    ("darkturquoise", 0x00ced1),
    ("darkviolet", 0x9400d3),
    ("deeppink", 0xff1493),
    ("deepskyblue", 0x00bfff),
    ("dimgray", 0x696969),
    ("dimgrey", 0x696969),
    ("dodgerblue", 0x1e90ff),
    ("firebrick", 0xb22222),
    ("floralwhite", 0xfffaf0),
    ("forestgreen", 0x228b22),
    ("fuchsia", 0xff00ff),
    ("gainsboro", 0xdcdcdc),
    ("ghostwhite", 0xf8f8ff),
    ("gold", 0xffd700),
    ("goldenrod", 0xdaa520),
    ("gray", 0x808080),
    ("green", 0x008000),
    ("greenyellow", 0xadff2f),
    ("grey", 0x808080),
    ("honeydew", 0xf0fff0),
    ("hotpink", 0xff69b4),
    ("indianred", 0xcd5c5c),
    ("indigo", 0x4b0082),
    ("ivory", 0xfffff0),
    ("khaki", 0xf0e68c),
    ("lavender", 0xe6e6fa),
    ("lavenderblush", 0xfff0f5),
    ("lawngreen", 0x7cfc00),
    ("lemonchiffon", 0xfffacd),
    ("lightblue", 0xadd8e6),
    ("lightcoral", 0xf08080),
    ("lightcyan", 0xe0ffff),
    ("lightgoldenrodyellow", 0xfafad2),
    ("lightgray", 0xd3d3d3),
    ("lightgreen", 0x90ee90),
    ("lightgrey", 0xd3d3d3),
    ("lightpink", 0xffb6c1),
    ("lightsalmon", 0xffa07a),
    ("lightseagreen", 0x20b2aa),
    ("lightskyblue", 0x87cefa),
    ("lightslategray", 0x778899),
    ("lightslategrey", 0x778899),
    ("lightsteelblue", 0xb0c4de),
    ("lightyellow", 0xffffe0),
    ("lime", 0x00ff00),
    ("limegreen", 0x32cd32),
    ("linen", 0xfaf0e6),
    ("magenta", 0xff00ff),
    ("maroon", 0x800000),
    ("mediumaquamarine", 0x66cdaa),
    ("mediumblue", 0x0000cd),
    ("mediumorchid", 0xba55d3),
    ("mediumpurple", 0x9370db),
    ("mediumseagreen", 0x3cb371),
    ("mediumslateblue", 0x7b68ee),
    ("mediumspringgreen", 0x00fa9a),
    ("mediumturquoise", 0x48d1cc),
    ("mediumvioletred", 0xc71585),
    ("midnightblue", 0x191970),
    ("mintcream", 0xf5fffa),
    ("mistyrose", 0xffe4e1),
    ("moccasin", 0xffe4b5),
    ("navajowhite", 0xffdead),
    ("navy", 0x000080),
    ("oldlace", 0xfdf5e6),
    ("olive", 0x808000),
    ("olivedrab", 0x6b8e23),
    ("orange", 0xffa500),
    ("orangered", 0xff4500),
    ("orchid", 0xda70d6),
    ("palegoldenrod", 0xeee8aa),
    ("palegreen", 0x98fb98),
    ("paleturquoise", 0xafeeee),
    ("palevioletred", 0xdb7093),
    ("papayawhip", 0xffefd5),
    ("peachpuff", 0xffdab9),
    ("peru", 0xcd853f),
    ("pink", 0xffc0cb),
    ("plum", 0xdda0dd),
    ("powderblue", 0xb0e0e6),
    ("purple", 0x800080),
    ("rebeccapurple", 0x663399),
    ("red", 0xff0000),
    ("rosybrown", 0xbc8f8f),
    ("royalblue", 0x4169e1),
    ("saddlebrown", 0x8b4513),
    ("salmon", 0xfa8072),
    ("sandybrown", 0xf4a460),
    ("seagreen", 0x2e8b57),
    ("seashell", 0xfff5ee),
    ("sienna", 0xa0522d),
    ("silver", 0xc0c0c0),
    ("skyblue", 0x87ceeb),
    ("slateblue", 0x6a5acd),
    ("slategray", 0x708090),
    ("slategrey", 0x708090),
    ("snow", 0xfffafa),
    ("springgreen", 0x00ff7f),
    ("steelblue", 0x4682b4),
    ("tan", 0xd2b48c),
    ("teal", 0x008080),
    ("thistle", 0xd8bfd8),
    ("tomato", 0xff6347),
    ("turquoise", 0x40e0d0),
    ("violet", 0xee82ee),
    ("wheat", 0xf5deb3),
    ("white", 0xffffff),
    ("whitesmoke", 0xf5f5f5),
    ("yellow", 0xffff00),
    ("yellowgreen", 0x9acd32),
];

/// A colour as written: sRGB-encoded channels and straight alpha.
type Rgba = [f32; 4];

fn rgb_bytes(rgb: u32, alpha: f32) -> Rgba {
    [
        ((rgb >> 16) & 0xff) as f32 / 255.0,
        ((rgb >> 8) & 0xff) as f32 / 255.0,
        (rgb & 0xff) as f32 / 255.0,
        alpha,
    ]
}

/// A CSS colour: hex with 3, 4, 6 or 8 digits, `rgb()` and `rgba()`, a
/// named colour, `transparent`. Not `currentColor`, which the caller
/// handles.
fn parse_color(value: &str) -> Option<Rgba> {
    let value = value.trim();
    if let Some(hex) = value.strip_prefix('#') {
        if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let digit = |index: usize| u32::from_str_radix(&hex[index..=index], 16).ok();
        let pair = |index: usize| u32::from_str_radix(&hex[index..index + 2], 16).ok();
        return match hex.len() {
            3 | 4 => {
                let rgb = ((digit(0)? * 17) << 16) | ((digit(1)? * 17) << 8) | (digit(2)? * 17);
                let alpha = if hex.len() == 4 {
                    (digit(3)? * 17) as f32 / 255.0
                } else {
                    1.0
                };
                Some(rgb_bytes(rgb, alpha))
            }
            6 | 8 => {
                let rgb = (pair(0)? << 16) | (pair(2)? << 8) | pair(4)?;
                let alpha = if hex.len() == 8 {
                    pair(6)? as f32 / 255.0
                } else {
                    1.0
                };
                Some(rgb_bytes(rgb, alpha))
            }
            _ => None,
        };
    }
    let lower = value.to_ascii_lowercase();
    if lower == "transparent" {
        return Some([0.0; 4]);
    }
    if let Some(arguments) = lower
        .strip_prefix("rgba(")
        .or_else(|| lower.strip_prefix("rgb("))
    {
        let arguments = arguments.strip_suffix(')')?;
        let mut parts = arguments
            .split(|c: char| c.is_ascii_whitespace() || c == ',' || c == '/')
            .filter(|part| !part.is_empty());
        let mut channel = || -> Option<f32> {
            let part = parts.next()?;
            let value = match part.strip_suffix('%') {
                Some(percent) => length_plain(percent)? * 2.55,
                None => length_plain(part)?,
            };
            // A browser rounds to the nearest byte.
            Some(value.clamp(0.0, 255.0).round() / 255.0)
        };
        let (r, g, b) = (channel()?, channel()?, channel()?);
        let alpha = match parts.next() {
            Some(part) => fraction(part)?.clamp(0.0, 1.0),
            None => 1.0,
        };
        if parts.next().is_some() {
            return None;
        }
        return Some([r, g, b, alpha]);
    }
    NAMED_COLORS
        .iter()
        .find(|(name, _)| *name == lower)
        .map(|&(_, rgb)| rgb_bytes(rgb, 1.0))
}

/// A `transform` attribute: a list of `matrix`, `translate`, `scale`,
/// `rotate`, `skewX` and `skewY`, the first applied last, as in SVG.
fn parse_transform(value: &str) -> Option<Transform> {
    let mut result = Transform::IDENTITY;
    let mut rest = value.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == ',');
    let mut count = 0;
    while !rest.is_empty() {
        count += 1;
        if count > 64 {
            return None;
        }
        let open = rest.find('(')?;
        let close = rest.find(')')?;
        if close < open {
            return None;
        }
        let name = rest[..open].trim();
        let mut numbers = Numbers::new(&rest[open + 1..close]);
        let mut values = [0.0f32; 6];
        let mut given = 0;
        while !numbers.at_end() {
            if given == 6 {
                return None;
            }
            values[given] = numbers.number()?;
            given += 1;
            numbers.skip_separator();
        }
        let [a, b, c, d, e, f] = values;
        let function = match (name, given) {
            ("matrix", 6) => Transform::matrix(a, b, c, d, e, f),
            ("translate", 1 | 2) => Transform::translate(a, b),
            ("scale", 1) => Transform::scale(a, a),
            ("scale", 2) => Transform::scale(a, b),
            ("rotate", 1) => Transform::rotate(a),
            ("rotate", 3) => Transform::rotate(a).about(Point::new(b, c)),
            ("skewX", 1) => Transform::skew_x(a),
            ("skewY", 1) => Transform::skew_y(a),
            _ => return None,
        };
        // Later functions act on the point first.
        result = function.then(result);
        rest = rest[close + 1..].trim_start_matches(|c: char| c.is_ascii_whitespace() || c == ',');
    }
    in_range(result).then_some(result)
}

fn in_range(transform: Transform) -> bool {
    [
        transform.a,
        transform.b,
        transform.c,
        transform.d,
        transform.tx,
        transform.ty,
    ]
    .iter()
    .all(|value| value.is_finite() && value.abs() <= MAX_NUMBER * 1000.0)
}

/// The id in `url(#id)`, and what follows the closing parenthesis.
fn parse_url(value: &str) -> Option<(&str, &str)> {
    let inner = value.strip_prefix("url(")?;
    let close = inner.find(')')?;
    let target = inner[..close]
        .trim()
        .trim_matches(|c| c == '"' || c == '\'')
        .trim();
    let id = target.strip_prefix('#')?;
    (!id.is_empty()).then_some((id, inner[close + 1..].trim()))
}

// ----------------------------------------------------------------- model

/// A paint as an element inherits it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum PaintSpec {
    None,
    Color(Rgba),
    Current,
    /// The gradient element's index.
    Gradient(usize),
}

/// Inherited presentation.
#[derive(Clone, Debug)]
struct Style {
    fill: PaintSpec,
    stroke: PaintSpec,
    fill_opacity: f32,
    stroke_opacity: f32,
    even_odd: bool,
    clip_even_odd: bool,
    stroke_width: f32,
    cap: LineCap,
    join: LineJoin,
    miter_limit: f32,
    dashes: Vec<f32>,
    dash_offset: f32,
    /// `color`, where an ancestor sets it; otherwise the colour given when
    /// the picture is drawn.
    color: Option<Rgba>,
    visible: bool,
}

impl Style {
    fn initial() -> Self {
        Self {
            fill: PaintSpec::Color([0.0, 0.0, 0.0, 1.0]),
            stroke: PaintSpec::None,
            fill_opacity: 1.0,
            stroke_opacity: 1.0,
            even_odd: false,
            clip_even_odd: false,
            stroke_width: 1.0,
            cap: LineCap::Butt,
            join: LineJoin::Miter,
            miter_limit: DEFAULT_MITER_LIMIT,
            dashes: Vec::new(),
            dash_offset: 0.0,
            color: None,
            visible: true,
        }
    }
}

/// What an element says that its children do not inherit.
struct Local {
    opacity: f32,
    transform: Transform,
    /// The `clipPath` element's index.
    clip: Option<usize>,
    display: bool,
}

/// A colour that may wait for the one given at draw time.
#[derive(Clone, Copy, Debug, PartialEq)]
enum InkColor {
    Fixed(Rgba),
    /// `currentColor`, with an opacity of its own.
    Current(f32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum InkDef {
    Color(InkColor),
    Gradient { index: usize, alpha: f32 },
}

#[derive(Debug)]
struct GradientDef {
    linear: bool,
    /// `x1 y1 x2 y2`, or `cx cy r fx fy`.
    geometry: [Coordinate; 5],
    bounding_box: bool,
    transform: Transform,
    spread: Spread,
    stops: Vec<(f32, InkColor)>,
}

#[derive(Debug)]
struct ClipShape {
    commands: Vec<Command>,
    /// To the user space of the element that uses the clip.
    transform: Transform,
    even_odd: bool,
}

/// A use of a clip: which, and the space of the element using it.
#[derive(Clone, Copy, Debug)]
struct ClipUse {
    clip: usize,
    transform: Transform,
}

#[derive(Debug)]
struct StrokeDef {
    ink: InkDef,
    width: f32,
    cap: LineCap,
    join: LineJoin,
    miter_limit: f32,
    dashes: Vec<f32>,
    dash_offset: f32,
}

#[derive(Debug)]
struct ShapeNode {
    commands: Vec<Command>,
    /// To view-box space.
    transform: Transform,
    /// The path's bounding box in its own space, for gradients.
    bounds: [f32; 4],
    fill: Option<InkDef>,
    even_odd: bool,
    stroke: Option<StrokeDef>,
    opacity: f32,
    clips: Vec<ClipUse>,
}

#[derive(Debug)]
enum Node {
    Shape(ShapeNode),
    /// Children composited together first, then faded and cut as one.
    Group {
        opacity: f32,
        clips: Vec<ClipUse>,
        children: Vec<Node>,
    },
}

#[derive(Debug)]
struct Art {
    view_box: [f32; 4],
    width: f32,
    height: f32,
    nodes: Vec<Node>,
    gradients: Vec<GradientDef>,
    clips: Vec<Vec<ClipShape>>,
    hash: u64,
    uses_current: bool,
    single_ink: bool,
}

// --------------------------------------------------------------- builder

const SHAPES: &[&str] = &[
    "path", "rect", "circle", "ellipse", "line", "polyline", "polygon",
];

/// Properties that cannot change how this subset is drawn: they act on
/// text, on interaction, or on features that are refused where they are
/// used.
const INERT_PROPERTIES: &[&str] = &[
    "shape-rendering",
    "color-interpolation",
    "color-interpolation-filters",
    "color-rendering",
    "image-rendering",
    "text-rendering",
    "pointer-events",
    "cursor",
    "overflow",
    "clip",
    "enable-background",
    "isolation",
    "font",
    "font-family",
    "font-size",
    "font-size-adjust",
    "font-stretch",
    "font-style",
    "font-variant",
    "font-weight",
    "font-feature-settings",
    "font-variation-settings",
    "font-kerning",
    "text-anchor",
    "text-decoration",
    "text-decoration-line",
    "text-decoration-style",
    "text-decoration-color",
    "text-align",
    "text-indent",
    "text-transform",
    "text-orientation",
    "letter-spacing",
    "word-spacing",
    "writing-mode",
    "direction",
    "unicode-bidi",
    "dominant-baseline",
    "alignment-baseline",
    "baseline-shift",
    "line-height",
    "white-space",
    "glyph-orientation-horizontal",
    "glyph-orientation-vertical",
    "kerning",
    "stop-color",
    "stop-opacity",
    "flood-color",
    "flood-opacity",
    "lighting-color",
    "block-progression",
    "shape-inside",
    "shape-padding",
    "shape-margin",
    "inline-size",
    "solid-color",
    "solid-opacity",
];

/// Attributes that describe nothing drawn.
fn is_inert_attribute(name: &str) -> bool {
    matches!(
        name,
        "id" | "class"
            | "xmlns"
            | "version"
            | "baseProfile"
            | "lang"
            | "role"
            | "tabindex"
            | "focusable"
            | "contentScriptType"
            | "contentStyleType"
            | "zoomAndPan"
    ) || name.contains(':')
        || name.starts_with("data-")
        || name.starts_with("aria-")
}

fn geometry_attributes(element: &str) -> &'static [&'static str] {
    match element {
        "svg" => &[
            "viewBox",
            "width",
            "height",
            "x",
            "y",
            "preserveAspectRatio",
        ],
        "path" => &["d"],
        "rect" => &["x", "y", "width", "height", "rx", "ry"],
        "circle" => &["cx", "cy", "r"],
        "ellipse" => &["cx", "cy", "rx", "ry"],
        "line" => &["x1", "y1", "x2", "y2"],
        "polyline" | "polygon" => &["points"],
        "clipPath" => &["clipPathUnits"],
        _ => &[],
    }
}

enum Property {
    Applied,
    Unknown,
}

struct Builder<'a> {
    source: &'a str,
    elements: &'a [Element<'a>],
    ids: HashMap<&'a str, usize>,
    lenient: bool,
    warnings: Vec<SvgError>,
    commands: usize,
    view_box: [f32; 4],
    gradients: Vec<GradientDef>,
    /// By element index; `None` for a gradient that paints nothing.
    gradient_of: HashMap<usize, Option<usize>>,
    clips: Vec<Vec<ClipShape>>,
    clip_of: HashMap<usize, usize>,
    uses_current: bool,
    uses_gradient: bool,
    /// The distinct literal colours painted with, without their alpha.
    literals: Vec<[u32; 3]>,
}

impl<'a> Builder<'a> {
    fn error(&self, offset: usize, kind: SvgErrorKind) -> SvgError {
        error_at(self.source, offset, kind)
    }

    /// Something outside the subset: fatal, or noted and skipped by the
    /// caller in the lenient mode.
    fn refuse(&mut self, error: SvgError) -> Result<(), SvgError> {
        if !self.lenient {
            return Err(error);
        }
        if self.warnings.len() < MAX_WARNINGS {
            self.warnings.push(error);
        }
        Ok(())
    }

    fn bad_value(
        &self,
        element: &Element<'_>,
        attribute: &str,
        value: &str,
        at: usize,
    ) -> SvgError {
        self.error(
            at,
            SvgErrorKind::UnsupportedValue {
                element: element.name.to_owned(),
                attribute: attribute.to_owned(),
                value: shortened(value),
            },
        )
    }

    /// A length attribute of a shape: `default` when absent or, in the
    /// lenient mode, unreadable.
    fn number(&mut self, element: &Element<'a>, name: &str, default: f32) -> Result<f32, SvgError> {
        let Some(attribute) = element.attribute(name) else {
            return Ok(default);
        };
        match length(&attribute.value) {
            Some(number) => Ok(number),
            None => {
                let error = self.bad_value(element, name, &attribute.value, attribute.offset);
                self.refuse(error)?;
                Ok(default)
            }
        }
    }

    /// A length that must not be negative.
    fn size(&mut self, element: &Element<'a>, name: &str, default: f32) -> Result<f32, SvgError> {
        let number = self.number(element, name, default)?;
        if number < 0.0 {
            let attribute = element.attribute(name);
            let error = self.bad_value(
                element,
                name,
                attribute.map_or("", |attribute| &attribute.value),
                attribute.map_or(element.offset, |attribute| attribute.offset),
            );
            self.refuse(error)?;
            return Ok(0.0);
        }
        Ok(number)
    }

    fn paint(
        &mut self,
        element: &Element<'a>,
        name: &str,
        value: &str,
        at: usize,
    ) -> Result<Option<PaintSpec>, SvgError> {
        let value = value.trim();
        if value == "none" {
            return Ok(Some(PaintSpec::None));
        }
        if value == "inherit" {
            return Ok(None);
        }
        if value.eq_ignore_ascii_case("currentcolor") {
            return Ok(Some(PaintSpec::Current));
        }
        if value.starts_with("url(") {
            let Some((id, fallback)) = parse_url(value) else {
                let error = self.bad_value(element, name, value, at);
                self.refuse(error)?;
                return Ok(None);
            };
            match self.ids.get(id) {
                Some(&index)
                    if matches!(
                        self.elements[index].name,
                        "linearGradient" | "radialGradient"
                    ) =>
                {
                    return Ok(Some(PaintSpec::Gradient(index)));
                }
                Some(_) => {
                    let error = self.bad_value(element, name, value, at);
                    self.refuse(error)?;
                    return Ok(None);
                }
                None if fallback.is_empty() => {
                    let error = self.error(
                        at,
                        SvgErrorKind::UnknownReference {
                            element: element.name.to_owned(),
                            id: shortened(id),
                        },
                    );
                    self.refuse(error)?;
                    // As browsers draw it: nothing.
                    return Ok(Some(PaintSpec::None));
                }
                // As SVG says: the fallback after a missing reference.
                None => return self.paint(element, name, fallback, at),
            }
        }
        match parse_color(value) {
            Some(color) => Ok(Some(PaintSpec::Color(color))),
            None => {
                let error = self.bad_value(element, name, value, at);
                self.refuse(error)?;
                Ok(None)
            }
        }
    }

    /// Applies one presentation property, from an attribute or from
    /// `style`.
    fn property(
        &mut self,
        element: &Element<'a>,
        name: &str,
        value: &str,
        at: usize,
        style: &mut Style,
        local: &mut Local,
    ) -> Result<Property, SvgError> {
        let value = value.trim();
        let mut bad = false;
        match name {
            "fill" | "stroke" => {
                if let Some(paint) = self.paint(element, name, value, at)? {
                    if name == "fill" {
                        style.fill = paint;
                    } else {
                        style.stroke = paint;
                    }
                }
            }
            "color" => {
                if !(value == "inherit" || value.eq_ignore_ascii_case("currentcolor")) {
                    match parse_color(value) {
                        Some(color) => style.color = Some(color),
                        None => bad = true,
                    }
                }
            }
            "fill-opacity" | "stroke-opacity" | "opacity" => match (name, fraction(value)) {
                ("fill-opacity", _) if value == "inherit" => {}
                ("stroke-opacity", _) if value == "inherit" => {}
                ("fill-opacity", Some(amount)) => style.fill_opacity = amount.clamp(0.0, 1.0),
                ("stroke-opacity", Some(amount)) => {
                    style.stroke_opacity = amount.clamp(0.0, 1.0);
                }
                ("opacity", Some(amount)) => local.opacity = amount.clamp(0.0, 1.0),
                _ => bad = true,
            },
            "fill-rule" | "clip-rule" => {
                let even_odd = match value {
                    "nonzero" => Some(false),
                    "evenodd" => Some(true),
                    "inherit" => None,
                    _ => {
                        bad = true;
                        None
                    }
                };
                if let Some(even_odd) = even_odd {
                    if name == "fill-rule" {
                        style.even_odd = even_odd;
                    } else {
                        style.clip_even_odd = even_odd;
                    }
                }
            }
            "stroke-width" => match length(value) {
                _ if value == "inherit" => {}
                Some(width) if width >= 0.0 => style.stroke_width = width,
                _ => bad = true,
            },
            "stroke-linecap" => match value {
                "butt" => style.cap = LineCap::Butt,
                "round" => style.cap = LineCap::Round,
                "square" => style.cap = LineCap::Square,
                "inherit" => {}
                _ => bad = true,
            },
            "stroke-linejoin" => match value {
                "miter" => style.join = LineJoin::Miter,
                "round" => style.join = LineJoin::Round,
                "bevel" => style.join = LineJoin::Bevel,
                "inherit" => {}
                _ => bad = true,
            },
            "stroke-miterlimit" => match length_plain(value) {
                _ if value == "inherit" => {}
                Some(limit) if limit >= 1.0 => style.miter_limit = limit,
                _ => bad = true,
            },
            "stroke-dasharray" => {
                if value == "none" {
                    style.dashes.clear();
                } else if value != "inherit" {
                    let mut dashes = Vec::new();
                    let mut ok = true;
                    for part in value
                        .split(|c: char| c.is_ascii_whitespace() || c == ',')
                        .filter(|part| !part.is_empty())
                    {
                        match length(part) {
                            Some(dash) if dash >= 0.0 && dashes.len() < MAX_DASHES => {
                                dashes.push(dash);
                            }
                            _ => ok = false,
                        }
                    }
                    if ok && !dashes.is_empty() {
                        // An odd list is repeated to make an even one; a
                        // list of zeros is a solid line.
                        if dashes.len() % 2 == 1 {
                            dashes.extend_from_within(..);
                        }
                        if dashes.iter().sum::<f32>() <= 0.0 {
                            dashes.clear();
                        }
                        style.dashes = dashes;
                    } else {
                        bad = true;
                    }
                }
            }
            "stroke-dashoffset" => match length(value) {
                _ if value == "inherit" => {}
                Some(offset) => style.dash_offset = offset,
                None => bad = true,
            },
            "display" => local.display = value != "none",
            "visibility" => match value {
                "visible" => style.visible = true,
                "hidden" | "collapse" => style.visible = false,
                "inherit" => {}
                _ => bad = true,
            },
            "clip-path" => {
                if value == "none" {
                    local.clip = None;
                } else {
                    match parse_url(value) {
                        Some((id, "")) => match self.ids.get(id) {
                            Some(&index) if self.elements[index].name == "clipPath" => {
                                local.clip = Some(index);
                            }
                            Some(_) => bad = true,
                            None => {
                                let error = self.error(
                                    at,
                                    SvgErrorKind::UnknownReference {
                                        element: element.name.to_owned(),
                                        id: shortened(id),
                                    },
                                );
                                self.refuse(error)?;
                            }
                        },
                        _ => bad = true,
                    }
                }
            }
            // Features that are not drawn: only their "off" value passes.
            "mask" | "filter" | "marker" | "marker-start" | "marker-mid" | "marker-end"
            | "vector-effect" => bad = value != "none",
            "mix-blend-mode" => bad = value != "normal",
            "paint-order" => {
                // Anything that still paints the fill before the stroke.
                let mut order = value.split_ascii_whitespace();
                bad = !matches!(
                    (order.next(), order.next()),
                    (Some("normal" | "fill"), _) | (Some("markers"), Some("fill") | None)
                );
            }
            _ if INERT_PROPERTIES.contains(&name) || name.starts_with("-inkscape-") => {}
            _ => return Ok(Property::Unknown),
        }
        if bad {
            let error = self.bad_value(element, name, value, at);
            self.refuse(error)?;
        }
        Ok(Property::Applied)
    }

    /// Resolves an element's attributes against what it inherits.
    /// `geometry` names the attributes the caller reads itself.
    fn resolve(
        &mut self,
        element: &Element<'a>,
        inherited: &Style,
        geometry: &[&str],
    ) -> Result<(Style, Local), SvgError> {
        let mut style = inherited.clone();
        let mut local = Local {
            opacity: 1.0,
            transform: Transform::IDENTITY,
            clip: None,
            display: true,
        };
        // Presentation attributes first: a `style` declaration wins over
        // them wherever it is written.
        for attribute in &element.attributes {
            let (name, value) = (attribute.name, &*attribute.value);
            if name == "style" || geometry.contains(&name) {
                continue;
            }
            if name == "transform" {
                match parse_transform(value) {
                    Some(transform) => local.transform = transform,
                    None => {
                        let error = self.bad_value(element, name, value, attribute.offset);
                        self.refuse(error)?;
                    }
                }
                continue;
            }
            if let Property::Applied = self.property(
                element,
                name,
                value,
                attribute.offset,
                &mut style,
                &mut local,
            )? {
                continue;
            }
            if !is_inert_attribute(name) {
                let error = self.error(
                    attribute.offset,
                    SvgErrorKind::UnsupportedAttribute {
                        element: element.name.to_owned(),
                        attribute: name.to_owned(),
                    },
                );
                self.refuse(error)?;
            }
        }
        if let Some(attribute) = element.attribute("style") {
            for declaration in attribute.value.split(';') {
                if declaration.trim().is_empty() {
                    continue;
                }
                let applied = match declaration.split_once(':') {
                    Some((name, value)) => {
                        let name = name.trim();
                        let value = value.trim();
                        let value = value
                            .strip_suffix("!important")
                            .map_or(value, str::trim_end);
                        matches!(
                            self.property(
                                element,
                                name,
                                value,
                                attribute.offset,
                                &mut style,
                                &mut local,
                            )?,
                            Property::Applied
                        )
                    }
                    None => false,
                };
                if !applied {
                    let name = declaration.split(':').next().unwrap_or("").trim();
                    let error = self.error(
                        attribute.offset,
                        SvgErrorKind::UnsupportedAttribute {
                            element: element.name.to_owned(),
                            attribute: format!("style: {}", shortened(name)),
                        },
                    );
                    self.refuse(error)?;
                }
            }
        }
        Ok((style, local))
    }

    /// Children of a shape, a stop or a gradient that draw nothing.
    fn only_descriptions(&mut self, element: &Element<'a>, also: &[&str]) -> Result<(), SvgError> {
        for &child in &element.children {
            let child = &self.elements[child];
            if matches!(child.name, "title" | "desc" | "metadata")
                || also.contains(&child.name)
                || is_foreign(child.name)
            {
                continue;
            }
            let error = self.error(
                child.offset,
                SvgErrorKind::UnsupportedElement(child.name.to_owned()),
            );
            self.refuse(error)?;
        }
        Ok(())
    }

    /// The outline of a shape element, in its own user space. `None` for
    /// a shape that draws nothing (no radius, no points).
    fn outline(&mut self, element: &Element<'a>) -> Result<Option<Vec<Command>>, SvgError> {
        let mut commands: Vec<Command> = Vec::new();
        match element.name {
            "path" => {
                let Some(attribute) = element.attribute("d") else {
                    return Ok(None);
                };
                let data = attribute.value.trim();
                if data.is_empty() || data == "none" {
                    return Ok(None);
                }
                match path_commands(data) {
                    Ok(path) => commands = path,
                    Err(IconError::Empty) => return Ok(None),
                    Err(error) => {
                        let offset = match error {
                            IconError::InvalidPath { offset } => offset,
                            _ => 0,
                        };
                        let error =
                            self.error(attribute.offset, SvgErrorKind::InvalidPath { offset });
                        self.refuse(error)?;
                        return Ok(None);
                    }
                }
            }
            "rect" => {
                let (x, y) = (
                    self.number(element, "x", 0.0)?,
                    self.number(element, "y", 0.0)?,
                );
                let (width, height) = (
                    self.size(element, "width", 0.0)?,
                    self.size(element, "height", 0.0)?,
                );
                if !(width > 0.0 && height > 0.0) {
                    return Ok(None);
                }
                // As in SVG: one radius given stands for both, and each is
                // at most half its side.
                let (rx, ry) = match (element.attribute("rx"), element.attribute("ry")) {
                    (None, None) => (0.0, 0.0),
                    (Some(_), None) => {
                        let rx = self.size(element, "rx", 0.0)?;
                        (rx, rx)
                    }
                    (None, Some(_)) => {
                        let ry = self.size(element, "ry", 0.0)?;
                        (ry, ry)
                    }
                    (Some(_), Some(_)) => (
                        self.size(element, "rx", 0.0)?,
                        self.size(element, "ry", 0.0)?,
                    ),
                };
                let (rx, ry) = (rx.min(width / 2.0), ry.min(height / 2.0));
                if rx > 0.0 && ry > 0.0 {
                    commands.add_round_rect((x, y), width, height, rx, ry);
                } else {
                    commands.add_rect((x, y), width, height);
                }
            }
            "circle" => {
                let center = (
                    self.number(element, "cx", 0.0)?,
                    self.number(element, "cy", 0.0)?,
                );
                let radius = self.size(element, "r", 0.0)?;
                if radius <= 0.0 {
                    return Ok(None);
                }
                commands.add_circle(center, radius);
            }
            "ellipse" => {
                let center = (
                    self.number(element, "cx", 0.0)?,
                    self.number(element, "cy", 0.0)?,
                );
                let (rx, ry) = (
                    self.size(element, "rx", 0.0)?,
                    self.size(element, "ry", 0.0)?,
                );
                if !(rx > 0.0 && ry > 0.0) {
                    return Ok(None);
                }
                commands.add_ellipse(center, rx, ry);
            }
            "line" => {
                commands.move_to((
                    self.number(element, "x1", 0.0)?,
                    self.number(element, "y1", 0.0)?,
                ));
                commands.line_to((
                    self.number(element, "x2", 0.0)?,
                    self.number(element, "y2", 0.0)?,
                ));
            }
            _ => {
                // polyline, polygon
                let Some(attribute) = element.attribute("points") else {
                    return Ok(None);
                };
                let mut numbers = Numbers::new(&attribute.value);
                let mut first = true;
                let mut valid = true;
                while !numbers.at_end() {
                    let point = numbers.number().and_then(|x| {
                        numbers.skip_separator();
                        numbers.number().map(|y| (x, y))
                    });
                    let Some(point) = point else {
                        valid = false;
                        break;
                    };
                    numbers.skip_separator();
                    if first {
                        commands.move_to(point);
                        first = false;
                    } else {
                        commands.line_to(point);
                    }
                    if commands.len() > MAX_PATH_COMMANDS {
                        break;
                    }
                }
                if !valid {
                    let error = self.bad_value(element, "points", numbers.rest(), attribute.offset);
                    self.refuse(error)?;
                    return Ok(None);
                }
                if commands.len() < 2 {
                    return Ok(None);
                }
                if element.name == "polygon" {
                    commands.close();
                }
            }
        }

        self.commands += commands.len();
        if self.commands > MAX_PATH_COMMANDS {
            return Err(self.error(
                element.offset,
                SvgErrorKind::Limit("more than 200000 path commands"),
            ));
        }
        let in_range = |point: &zeno::Point| {
            point.x.is_finite()
                && point.y.is_finite()
                && point.x.abs() <= MAX_NUMBER
                && point.y.abs() <= MAX_NUMBER
        };
        let ok = commands.iter().all(|command| match command {
            Command::MoveTo(p) | Command::LineTo(p) => in_range(p),
            Command::QuadTo(c, p) => in_range(c) && in_range(p),
            Command::CurveTo(a, b, p) => in_range(a) && in_range(b) && in_range(p),
            Command::Close => true,
        });
        if !ok {
            return Err(self.error(
                element.offset,
                SvgErrorKind::Limit("a coordinate beyond a million units"),
            ));
        }
        Ok(Some(commands))
    }

    fn note_color(&mut self, color: InkColor) {
        match color {
            InkColor::Current(_) => self.uses_current = true,
            InkColor::Fixed(color) => {
                let key = [color[0].to_bits(), color[1].to_bits(), color[2].to_bits()];
                if self.literals.len() < 2 && !self.literals.contains(&key) {
                    self.literals.push(key);
                }
            }
        }
    }

    /// What a shape is filled or stroked with, from its inherited paint.
    fn ink(
        &mut self,
        paint: PaintSpec,
        opacity: f32,
        style: &Style,
    ) -> Result<Option<InkDef>, SvgError> {
        let ink = match paint {
            PaintSpec::None => return Ok(None),
            PaintSpec::Color(color) => InkDef::Color(InkColor::Fixed([
                color[0],
                color[1],
                color[2],
                color[3] * opacity,
            ])),
            PaintSpec::Current => InkDef::Color(match style.color {
                Some(color) => InkColor::Fixed([color[0], color[1], color[2], color[3] * opacity]),
                None => InkColor::Current(opacity),
            }),
            PaintSpec::Gradient(element) => match self.gradient(element)? {
                Some(index) => {
                    self.uses_gradient = true;
                    InkDef::Gradient {
                        index,
                        alpha: opacity,
                    }
                }
                None => return Ok(None),
            },
        };
        if let InkDef::Color(color) = ink {
            self.note_color(color);
        }
        Ok(Some(ink))
    }

    fn shape(
        &mut self,
        index: usize,
        inherited: &Style,
        parent: Transform,
        out: &mut Vec<Node>,
    ) -> Result<(), SvgError> {
        let element = &self.elements[index];
        self.only_descriptions(element, &[])?;
        let (style, local) = self.resolve(element, inherited, geometry_attributes(element.name))?;
        let Some(commands) = self.outline(element)? else {
            return Ok(());
        };
        if !local.display || !style.visible {
            return Ok(());
        }
        let transform = local.transform.then(parent);
        if !in_range(transform) {
            return Err(self.error(
                element.offset,
                SvgErrorKind::Limit("a transform out of range"),
            ));
        }
        let fill = if element.name == "line" {
            // A line has no interior.
            None
        } else {
            self.ink(style.fill, style.fill_opacity, &style)?
        };
        let stroke = if style.stroke_width > 0.0 {
            self.ink(style.stroke, style.stroke_opacity, &style)?
        } else {
            None
        };
        let stroke = match stroke {
            Some(ink) => {
                let mut dashes = style.dashes.clone();
                let mut dash_offset = 0.0;
                if !dashes.is_empty() {
                    let period: f32 = dashes.iter().sum();
                    if polygon_length(&commands) / period > MAX_DASH_COUNT {
                        return Err(self.error(
                            element.offset,
                            SvgErrorKind::Limit("more than 100000 dashes along a path"),
                        ));
                    }
                    dash_offset = style.dash_offset.rem_euclid(period);
                    if !dash_offset.is_finite() {
                        dash_offset = 0.0;
                        dashes.clear();
                    }
                }
                Some(StrokeDef {
                    ink,
                    width: style.stroke_width,
                    cap: style.cap,
                    join: style.join,
                    miter_limit: style.miter_limit,
                    dashes,
                    dash_offset,
                })
            }
            None => None,
        };
        if fill.is_none() && stroke.is_none() {
            return Ok(());
        }
        let mut clips = Vec::new();
        if let Some(clip) = local.clip {
            clips.push(ClipUse {
                clip: self.clip(clip)?,
                transform,
            });
        }
        out.push(Node::Shape(ShapeNode {
            bounds: bounds(&commands),
            commands,
            transform,
            fill,
            even_odd: style.even_odd,
            stroke,
            opacity: local.opacity,
            clips,
        }));
        Ok(())
    }

    /// Adds what a group contributes to `out`: its children as they are
    /// when the group neither fades nor clips, its one child with the
    /// group's opacity and clip folded in, or a group to composite.
    fn group(
        &mut self,
        opacity: f32,
        clip: Option<ClipUse>,
        mut children: Vec<Node>,
        out: &mut Vec<Node>,
    ) {
        if opacity >= 1.0 && clip.is_none() {
            out.append(&mut children);
            return;
        }
        if opacity <= 0.0 {
            return;
        }
        if children.len() == 1 {
            // Fading or cutting one node is the same as fading or cutting
            // a group that holds only it.
            let mut child = children.remove(0);
            match &mut child {
                Node::Shape(shape) => {
                    shape.opacity *= opacity;
                    shape.clips.extend(clip);
                }
                Node::Group {
                    opacity: own,
                    clips,
                    ..
                } => {
                    *own *= opacity;
                    clips.extend(clip);
                }
            }
            out.push(child);
        } else if !children.is_empty() {
            out.push(Node::Group {
                opacity,
                clips: clip.into_iter().collect(),
                children,
            });
        }
    }

    fn children(
        &mut self,
        parent: usize,
        style: &Style,
        transform: Transform,
        out: &mut Vec<Node>,
    ) -> Result<(), SvgError> {
        let elements = self.elements;
        for &index in &elements[parent].children {
            let element = &elements[index];
            match element.name {
                "title" | "desc" | "metadata" => {}
                name if is_foreign(name) => {}
                "defs" => {
                    // Nothing in it is drawn, but what it defines is
                    // checked whether or not it is used.
                    let (style, _) = self.resolve(element, style, &[])?;
                    let mut unused = Vec::new();
                    self.children(index, &style, transform, &mut unused)?;
                }
                "g" => {
                    let (style, local) = self.resolve(element, style, &[])?;
                    let transform = local.transform.then(transform);
                    if !in_range(transform) {
                        return Err(self.error(
                            element.offset,
                            SvgErrorKind::Limit("a transform out of range"),
                        ));
                    }
                    let mut nodes = Vec::new();
                    self.children(index, &style, transform, &mut nodes)?;
                    if !local.display {
                        continue;
                    }
                    let clip = match local.clip {
                        Some(clip) => Some(ClipUse {
                            clip: self.clip(clip)?,
                            transform,
                        }),
                        None => None,
                    };
                    self.group(local.opacity, clip, nodes, out);
                }
                "linearGradient" | "radialGradient" => {
                    self.gradient(index)?;
                }
                "clipPath" => {
                    self.clip(index)?;
                }
                name if SHAPES.contains(&name) => self.shape(index, style, transform, out)?,
                name => {
                    let error = self.error(
                        element.offset,
                        SvgErrorKind::UnsupportedElement(name.to_owned()),
                    );
                    self.refuse(error)?;
                }
            }
        }
        Ok(())
    }

    /// The clip a `clipPath` element describes, built once.
    fn clip(&mut self, index: usize) -> Result<usize, SvgError> {
        if let Some(&known) = self.clip_of.get(&index) {
            return Ok(known);
        }
        let elements = self.elements;
        let element = &elements[index];
        let (style, local) =
            self.resolve(element, &Style::initial(), geometry_attributes("clipPath"))?;
        if let Some(units) = element.attribute("clipPathUnits") {
            if units.value.trim() != "userSpaceOnUse" {
                let error = self.bad_value(element, "clipPathUnits", &units.value, units.offset);
                self.refuse(error)?;
            }
        }
        if local.clip.is_some() {
            let error = self.error(
                element.offset,
                SvgErrorKind::UnsupportedAttribute {
                    element: "clipPath".to_owned(),
                    attribute: "clip-path".to_owned(),
                },
            );
            self.refuse(error)?;
        }
        let mut shapes = Vec::new();
        for &child in &element.children {
            let child = &elements[child];
            if matches!(child.name, "title" | "desc" | "metadata") || is_foreign(child.name) {
                continue;
            }
            if !SHAPES.contains(&child.name) {
                let error = self.error(
                    child.offset,
                    SvgErrorKind::UnsupportedElement(child.name.to_owned()),
                );
                self.refuse(error)?;
                continue;
            }
            self.only_descriptions(child, &[])?;
            let (child_style, child_local) =
                self.resolve(child, &style, geometry_attributes(child.name))?;
            if child_local.clip.is_some() {
                let error = self.error(
                    child.offset,
                    SvgErrorKind::UnsupportedAttribute {
                        element: child.name.to_owned(),
                        attribute: "clip-path".to_owned(),
                    },
                );
                self.refuse(error)?;
            }
            let Some(commands) = self.outline(child)? else {
                continue;
            };
            if !child_local.display || !child_style.visible || child.name == "line" {
                continue;
            }
            let transform = child_local.transform.then(local.transform);
            if !in_range(transform) {
                return Err(self.error(
                    child.offset,
                    SvgErrorKind::Limit("a transform out of range"),
                ));
            }
            shapes.push(ClipShape {
                commands,
                transform,
                even_odd: child_style.clip_even_odd,
            });
        }
        let number = self.clips.len();
        self.clips.push(shapes);
        self.clip_of.insert(index, number);
        Ok(number)
    }

    /// The gradient a gradient element describes, built once. `None` for
    /// one without stops, which paints nothing.
    fn gradient(&mut self, index: usize) -> Result<Option<usize>, SvgError> {
        if let Some(&known) = self.gradient_of.get(&index) {
            return Ok(known);
        }
        let elements = self.elements;
        let element = &elements[index];
        let linear = element.name == "linearGradient";

        // The element and the gradients it refers to, nearest first.
        let mut chain = vec![index];
        loop {
            let last = &elements[chain[chain.len() - 1]];
            let Some(reference) = last
                .attribute("href")
                .or_else(|| last.attribute("xlink:href"))
            else {
                break;
            };
            let target = reference
                .value
                .trim()
                .strip_prefix('#')
                .and_then(|id| self.ids.get(id).copied());
            match target {
                Some(target)
                    if matches!(elements[target].name, "linearGradient" | "radialGradient") =>
                {
                    if chain.contains(&target) || chain.len() >= MAX_GRADIENT_CHAIN {
                        return Err(self.error(
                            reference.offset,
                            SvgErrorKind::Limit("gradients referring to one another in a loop"),
                        ));
                    }
                    chain.push(target);
                }
                _ => {
                    let error = self.error(
                        reference.offset,
                        SvgErrorKind::UnknownReference {
                            element: last.name.to_owned(),
                            id: shortened(reference.value.trim().trim_start_matches('#')),
                        },
                    );
                    self.refuse(error)?;
                    break;
                }
            }
        }

        // Every element of the chain is checked for attributes outside
        // the subset.
        for &link in &chain {
            let link = &elements[link];
            let allowed: &[&str] = if link.name == "linearGradient" {
                &["x1", "y1", "x2", "y2"]
            } else {
                &["cx", "cy", "r", "fx", "fy"]
            };
            for attribute in &link.attributes {
                let name = attribute.name;
                let known = allowed.contains(&name)
                    || matches!(
                        name,
                        "gradientUnits" | "gradientTransform" | "spreadMethod" | "href"
                    )
                    || is_inert_attribute(name)
                    || INERT_PROPERTIES.contains(&name)
                    || (name == "fr" && length(&attribute.value) == Some(0.0));
                if !known {
                    let error = self.error(
                        attribute.offset,
                        SvgErrorKind::UnsupportedAttribute {
                            element: link.name.to_owned(),
                            attribute: name.to_owned(),
                        },
                    );
                    self.refuse(error)?;
                }
            }
            self.only_descriptions(link, &["stop"])?;
        }

        // An attribute comes from the nearest gradient that has it;
        // geometry only from one of the same kind.
        let inherited = |name: &str, same_kind: bool| {
            chain
                .iter()
                .map(|&link| &elements[link])
                .filter(|link| !same_kind || link.name == element.name)
                .find_map(|link| link.attribute(name).map(|attribute| (link, attribute)))
        };
        let mut geometry = if linear {
            [
                Coordinate::Percent(0.0),
                Coordinate::Percent(0.0),
                Coordinate::Percent(100.0),
                Coordinate::Percent(0.0),
                Coordinate::Number(0.0),
            ]
        } else {
            [Coordinate::Percent(50.0); 5]
        };
        let names: &[&str] = if linear {
            &["x1", "y1", "x2", "y2"]
        } else {
            &["cx", "cy", "r", "fx", "fy"]
        };
        let mut focus_given = [false; 2];
        for (slot, name) in names.iter().enumerate() {
            let Some((owner, attribute)) = inherited(name, true) else {
                continue;
            };
            match Coordinate::parse(&attribute.value) {
                Some(coordinate) => {
                    geometry[slot] = coordinate;
                    if slot >= 3 {
                        focus_given[slot - 3] = true;
                    }
                }
                None => {
                    let error = self.bad_value(owner, name, &attribute.value, attribute.offset);
                    self.refuse(error)?;
                }
            }
        }
        if !linear {
            // The focus is the centre unless it is given.
            if !focus_given[0] {
                geometry[3] = geometry[0];
            }
            if !focus_given[1] {
                geometry[4] = geometry[1];
            }
        }
        let mut bounding_box = true;
        if let Some((owner, attribute)) = inherited("gradientUnits", false) {
            match attribute.value.trim() {
                "objectBoundingBox" => {}
                "userSpaceOnUse" => bounding_box = false,
                _ => {
                    let error =
                        self.bad_value(owner, "gradientUnits", &attribute.value, attribute.offset);
                    self.refuse(error)?;
                }
            }
        }
        let mut transform = Transform::IDENTITY;
        if let Some((owner, attribute)) = inherited("gradientTransform", false) {
            match parse_transform(&attribute.value) {
                Some(parsed) => transform = parsed,
                None => {
                    let error = self.bad_value(
                        owner,
                        "gradientTransform",
                        &attribute.value,
                        attribute.offset,
                    );
                    self.refuse(error)?;
                }
            }
        }
        let mut spread = Spread::Pad;
        if let Some((owner, attribute)) = inherited("spreadMethod", false) {
            match attribute.value.trim() {
                "pad" => {}
                "reflect" => spread = Spread::Reflect,
                "repeat" => spread = Spread::Repeat,
                _ => {
                    let error =
                        self.bad_value(owner, "spreadMethod", &attribute.value, attribute.offset);
                    self.refuse(error)?;
                }
            }
        }

        // Stops come from the nearest gradient that has any.
        let owner = chain.iter().map(|&link| &elements[link]).find(|link| {
            link.children
                .iter()
                .any(|&child| elements[child].name == "stop")
        });
        let mut stops: Vec<(f32, InkColor)> = Vec::new();
        for &child in owner.map_or(&[][..], |owner| &owner.children[..]) {
            let stop = &elements[child];
            if stop.name != "stop" {
                continue;
            }
            self.only_descriptions(stop, &[])?;
            if stops.len() >= MAX_STOPS {
                return Err(self.error(
                    stop.offset,
                    SvgErrorKind::Limit("more than 256 stops in a gradient"),
                ));
            }
            let mut offset = 0.0;
            let mut color = InkColor::Fixed([0.0, 0.0, 0.0, 1.0]);
            let mut opacity = 1.0;
            let mut declarations: Vec<(&str, &str, usize)> = Vec::new();
            for attribute in &stop.attributes {
                match attribute.name {
                    "style" => {}
                    "offset" | "stop-color" | "stop-opacity" => {
                        declarations.push((attribute.name, &attribute.value, attribute.offset));
                    }
                    name if is_inert_attribute(name) => {}
                    name => {
                        let error = self.error(
                            attribute.offset,
                            SvgErrorKind::UnsupportedAttribute {
                                element: "stop".to_owned(),
                                attribute: name.to_owned(),
                            },
                        );
                        self.refuse(error)?;
                    }
                }
            }
            if let Some(attribute) = stop.attribute("style") {
                for declaration in attribute.value.split(';') {
                    match declaration.split_once(':') {
                        Some((name, value))
                            if matches!(name.trim(), "stop-color" | "stop-opacity") =>
                        {
                            declarations.push((name.trim(), value.trim(), attribute.offset));
                        }
                        _ if declaration.trim().is_empty() => {}
                        _ => {
                            let name = declaration.split(':').next().unwrap_or("").trim();
                            let error = self.error(
                                attribute.offset,
                                SvgErrorKind::UnsupportedAttribute {
                                    element: "stop".to_owned(),
                                    attribute: format!("style: {}", shortened(name)),
                                },
                            );
                            self.refuse(error)?;
                        }
                    }
                }
            }
            for (name, value, at) in declarations {
                let ok = match name {
                    "offset" => fraction(value).map(|value| offset = value).is_some(),
                    "stop-opacity" => fraction(value).map(|value| opacity = value).is_some(),
                    _ if value.trim().eq_ignore_ascii_case("currentcolor") => {
                        color = InkColor::Current(1.0);
                        true
                    }
                    _ => parse_color(value)
                        .map(|value| color = InkColor::Fixed(value))
                        .is_some(),
                };
                if !ok {
                    let error = self.bad_value(stop, name, value, at);
                    self.refuse(error)?;
                }
            }
            let opacity = opacity.clamp(0.0, 1.0);
            let color = match color {
                InkColor::Fixed(c) => InkColor::Fixed([c[0], c[1], c[2], c[3] * opacity]),
                InkColor::Current(alpha) => InkColor::Current(alpha * opacity),
            };
            if let InkColor::Current(_) = color {
                self.uses_current = true;
            }
            // Offsets never decrease: a smaller one takes the one before.
            let offset = offset
                .clamp(0.0, 1.0)
                .max(stops.last().map_or(0.0, |stop| stop.0));
            stops.push((offset, color));
        }

        let number = if stops.is_empty() {
            None
        } else {
            self.gradients.push(GradientDef {
                linear,
                geometry,
                bounding_box,
                transform,
                spread,
                stops,
            });
            Some(self.gradients.len() - 1)
        };
        self.gradient_of.insert(index, number);
        Ok(number)
    }
}

/// An element of another vocabulary (an editor's metadata), which draws
/// nothing. `svg:` is the SVG vocabulary under a prefix, which this reader
/// does not follow, so it is not passed over.
fn is_foreign(name: &str) -> bool {
    name.split_once(':')
        .is_some_and(|(prefix, _)| prefix != "svg")
}

/// Length of a path's control polygon: at least the path's own length.
fn polygon_length(commands: &[Command]) -> f32 {
    let mut length = 0.0f32;
    let (mut current, mut start) = (zeno::Point::new(0.0, 0.0), zeno::Point::new(0.0, 0.0));
    let step = |from: zeno::Point, to: zeno::Point| {
        let (dx, dy) = (to.x - from.x, to.y - from.y);
        (dx * dx + dy * dy).sqrt()
    };
    for command in commands {
        match *command {
            Command::MoveTo(p) => {
                current = p;
                start = p;
            }
            Command::LineTo(p) => {
                length += step(current, p);
                current = p;
            }
            Command::QuadTo(c, p) => {
                length += step(current, c) + step(c, p);
                current = p;
            }
            Command::CurveTo(a, b, p) => {
                length += step(current, a) + step(a, b) + step(b, p);
                current = p;
            }
            Command::Close => {
                length += step(current, start);
                current = start;
            }
        }
    }
    length
}

/// The bounding box of a path's geometry: `x y width height`.
fn bounds(commands: &[Command]) -> [f32; 4] {
    let mut low = (f32::INFINITY, f32::INFINITY);
    let mut high = (f32::NEG_INFINITY, f32::NEG_INFINITY);
    let mut add = |x: f32, y: f32| {
        low = (low.0.min(x), low.1.min(y));
        high = (high.0.max(x), high.1.max(y));
    };
    // Where one coordinate of a cubic turns back, within the curve.
    let turns = |p0: f32, p1: f32, p2: f32, p3: f32| -> [Option<f32>; 2] {
        let a = -p0 + 3.0 * p1 - 3.0 * p2 + p3;
        let b = 2.0 * (p0 - 2.0 * p1 + p2);
        let c = p1 - p0;
        let inside = |t: f32| (t > 0.0 && t < 1.0).then_some(t);
        if a.abs() < 1.0e-12 {
            return [if b != 0.0 { inside(-c / b) } else { None }, None];
        }
        let root = b * b - 4.0 * a * c;
        if root < 0.0 {
            return [None, None];
        }
        let root = root.sqrt();
        [
            inside((-b + root) / (2.0 * a)),
            inside((-b - root) / (2.0 * a)),
        ]
    };
    let cubic = |p0: f32, p1: f32, p2: f32, p3: f32, t: f32| {
        let u = 1.0 - t;
        u * u * u * p0 + 3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t * p3
    };
    let mut current = zeno::Point::new(0.0, 0.0);
    for command in commands {
        match *command {
            Command::MoveTo(p) | Command::LineTo(p) => {
                add(p.x, p.y);
                current = p;
            }
            Command::QuadTo(c, p) => {
                // As the cubic with the same shape.
                let a = zeno::Point::new(
                    current.x + (c.x - current.x) * 2.0 / 3.0,
                    current.y + (c.y - current.y) * 2.0 / 3.0,
                );
                let b =
                    zeno::Point::new(p.x + (c.x - p.x) * 2.0 / 3.0, p.y + (c.y - p.y) * 2.0 / 3.0);
                for t in turns(current.x, a.x, b.x, p.x).into_iter().flatten() {
                    add(cubic(current.x, a.x, b.x, p.x, t), current.y.min(p.y));
                    add(cubic(current.x, a.x, b.x, p.x, t), current.y.max(p.y));
                }
                for t in turns(current.y, a.y, b.y, p.y).into_iter().flatten() {
                    add(current.x.min(p.x), cubic(current.y, a.y, b.y, p.y, t));
                    add(current.x.max(p.x), cubic(current.y, a.y, b.y, p.y, t));
                }
                add(p.x, p.y);
                current = p;
            }
            Command::CurveTo(a, b, p) => {
                for t in turns(current.x, a.x, b.x, p.x).into_iter().flatten() {
                    add(
                        cubic(current.x, a.x, b.x, p.x, t),
                        cubic(current.y, a.y, b.y, p.y, t),
                    );
                }
                for t in turns(current.y, a.y, b.y, p.y).into_iter().flatten() {
                    add(
                        cubic(current.x, a.x, b.x, p.x, t),
                        cubic(current.y, a.y, b.y, p.y, t),
                    );
                }
                add(p.x, p.y);
                current = p;
            }
            Command::Close => {}
        }
    }
    if low.0 > high.0 {
        return [0.0; 4];
    }
    [low.0, low.1, high.0 - low.0, high.1 - low.1]
}

fn isolation_depth(nodes: &[Node]) -> usize {
    nodes
        .iter()
        .map(|node| match node {
            Node::Shape(_) => 0,
            Node::Group { children, .. } => 1 + isolation_depth(children),
        })
        .max()
        .unwrap_or(0)
}

fn build(source: &str, lenient: bool) -> Result<(Art, Vec<SvgError>), SvgError> {
    let elements = XmlReader {
        source,
        position: 0,
    }
    .document()?;
    let root = &elements[0];
    if root.name != "svg" {
        return Err(error_at(source, root.offset, SvgErrorKind::NoSize));
    }
    let mut ids = HashMap::new();
    for (index, element) in elements.iter().enumerate() {
        if let Some(id) = element.attribute("id") {
            // As in a browser, the first element with an id has it.
            ids.entry(id.value.trim()).or_insert(index);
        }
    }
    // `ids` borrows the elements' values; the keys live as long as they do.
    let ids: HashMap<&str, usize> = ids;
    let mut builder = Builder {
        source,
        elements: &elements,
        ids,
        lenient,
        warnings: Vec::new(),
        commands: 0,
        view_box: [0.0; 4],
        gradients: Vec::new(),
        gradient_of: HashMap::new(),
        clips: Vec::new(),
        clip_of: HashMap::new(),
        uses_current: false,
        uses_gradient: false,
        literals: Vec::new(),
    };

    // The size: a view box, or a width and height that stand for one.
    let pixels = |name: &str| {
        root.attribute(name)
            .and_then(|attribute| length(&attribute.value))
            .filter(|value| *value > 0.0)
    };
    let view_box = match root.attribute("viewBox") {
        Some(attribute) => {
            let mut numbers = Numbers::new(&attribute.value);
            let mut values = [0.0f32; 4];
            let mut ok = true;
            for value in &mut values {
                match numbers.number() {
                    Some(number) => *value = number,
                    None => ok = false,
                }
                numbers.skip_separator();
            }
            if !(ok && numbers.at_end() && values[2] > 0.0 && values[3] > 0.0) {
                // A picture cannot be sized without it: fatal in either
                // mode.
                return Err(builder.bad_value(root, "viewBox", &attribute.value, attribute.offset));
            }
            values
        }
        None => match (pixels("width"), pixels("height")) {
            (Some(width), Some(height)) => [0.0, 0.0, width, height],
            _ => return Err(error_at(source, root.offset, SvgErrorKind::NoSize)),
        },
    };
    builder.view_box = view_box;
    if let Some(attribute) = root.attribute("preserveAspectRatio") {
        let value = attribute.value.trim();
        if !matches!(value, "xMidYMid" | "xMidYMid meet") {
            let error = builder.bad_value(root, "preserveAspectRatio", value, attribute.offset);
            builder.refuse(error)?;
        }
    }
    // A width or height in another unit, or a percentage, says nothing
    // about the picture's own size: the view box stands in.
    let width = pixels("width").unwrap_or(view_box[2]);
    let height = pixels("height").unwrap_or(view_box[3]);

    let (style, local) = builder.resolve(root, &Style::initial(), geometry_attributes("svg"))?;
    let mut children = Vec::new();
    builder.children(0, &style, local.transform, &mut children)?;
    let mut nodes = Vec::new();
    if local.display {
        let clip = match local.clip {
            Some(clip) => Some(ClipUse {
                clip: builder.clip(clip)?,
                transform: local.transform,
            }),
            None => None,
        };
        builder.group(local.opacity, clip, children, &mut nodes);
    }
    if isolation_depth(&nodes) > MAX_ISOLATED_GROUPS {
        return Err(error_at(
            source,
            root.offset,
            SvgErrorKind::Limit("faded or clipped groups nested more than 4 deep"),
        ));
    }

    // Fixed keys: the hash must not change between runs.
    let mut state = std::hash::DefaultHasher::new();
    source.hash(&mut state);
    lenient.hash(&mut state);
    let single_ink = !builder.uses_gradient
        && matches!(
            (builder.uses_current, builder.literals.len()),
            (true, 0) | (false, 1)
        );
    let art = Art {
        view_box,
        width,
        height,
        nodes,
        gradients: builder.gradients,
        clips: builder.clips,
        hash: state.finish(),
        uses_current: builder.uses_current,
        single_ink,
    };
    Ok((art, builder.warnings))
}

// ---------------------------------------------------------------- public

/// A picture that can be rasterised for the image path and cached by its
/// content: a [`VectorPicture`](crate::VectorPicture) or an
/// [`SvgPicture`]. `Frame::draw_picture` takes either.
pub trait RasterPicture {
    /// A number that is the same for pictures with the same content and,
    /// for all practical purposes, different otherwise.
    fn content_hash(&self) -> u64;

    /// `width * height * 4` bytes of RGBA, row by row from the top:
    /// sRGB-encoded colour premultiplied by alpha.
    fn rasterise(&self, width: u32, height: u32) -> Vec<u8>;
}

impl RasterPicture for crate::VectorPicture {
    fn content_hash(&self) -> u64 {
        crate::VectorPicture::content_hash(self)
    }

    fn rasterise(&self, width: u32, height: u32) -> Vec<u8> {
        crate::VectorPicture::rasterise(self, width, height)
    }
}

impl RasterPicture for SvgPicture {
    fn content_hash(&self) -> u64 {
        SvgPicture::content_hash(self)
    }

    fn rasterise(&self, width: u32, height: u32) -> Vec<u8> {
        SvgPicture::rasterise(self, width, height)
    }
}

/// A picture loaded from an SVG file.
///
/// Parse a file once and keep the value: in the application's state, or in
/// a `static` [`SvgAsset`] for a file bundled with `include_str!`. The
/// value is a shared handle, so cloning it is cheap and does not copy the
/// picture. Drawing it rasterises it for the exact pixels it covers, once
/// per size, through the same compositor and cache as a
/// [`VectorPicture`](crate::VectorPicture).
///
/// ```
/// use morningstar_text::SvgPicture;
///
/// let badge = SvgPicture::parse(
///     r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
///           <defs>
///             <linearGradient id="sky" x1="0" y1="0" x2="0" y2="1">
///               <stop offset="0" stop-color="#5AA9E6"/>
///               <stop offset="1" stop-color="#1D3557"/>
///             </linearGradient>
///           </defs>
///           <circle cx="12" cy="12" r="11" fill="url(#sky)"/>
///           <path d="M7 12l3.5 3.5L17 9" fill="none" stroke="currentColor"
///                 stroke-width="2" stroke-linecap="round"/>
///         </svg>"##,
/// )
/// .unwrap();
/// assert_eq!((badge.width(), badge.height()), (24.0, 24.0));
/// let pixels = badge.rasterise(48, 48);
/// assert_eq!(pixels.len(), 48 * 48 * 4);
/// ```
///
/// # The subset of SVG that is read
///
/// * Elements: `svg`, `g`, `defs`, `path`, `rect` (with `rx`, `ry`),
///   `circle`, `ellipse`, `line`, `polyline`, `polygon`, `clipPath` (of
///   shapes, in user space), `linearGradient` and `radialGradient` with
///   `stop`s (both `gradientUnits`, `gradientTransform`, `spreadMethod`, a
///   focal point, and `href` to another gradient). `title`, `desc`,
///   `metadata` and elements of other vocabularies (`inkscape:`,
///   `sodipodi:`, `rdf:`) are passed over.
/// * Presentation, as attributes or in a `style` attribute, inherited as
///   SVG says: `fill`, `stroke`, `fill-opacity`, `stroke-opacity`,
///   `opacity` (of a group as a whole), `fill-rule`, `clip-rule`,
///   `stroke-width`, `stroke-linecap`, `stroke-linejoin`,
///   `stroke-miterlimit`, `stroke-dasharray`, `stroke-dashoffset`,
///   `color`, `display`, `visibility`, `clip-path`, and `transform` lists
///   of `matrix`, `translate`, `scale`, `rotate`, `skewX` and `skewY`.
/// * Colours: hex with 3, 4, 6 or 8 digits, `rgb()` and `rgba()`, the CSS
///   named colours, `transparent`, `none`, and `currentColor`, which is
///   the colour given with [`with_current_color`](Self::with_current_color)
///   wherever no `color` attribute sets it (black otherwise).
/// * Lengths are numbers in user units, optionally written with `px`.
///
/// Everything else is refused with an [`SvgError`] that names it and its
/// line and column: `text`, `use`, `image`, `mask`, `filter`, `pattern`,
/// `style` elements and so classes, `symbol`, `marker`, nested `svg`,
/// animation, other units and percentages, `preserveAspectRatio` other
/// than the default, `clipPathUnits="objectBoundingBox"`, CSS transforms
/// in `style`. Properties that cannot change a picture within this subset
/// (font properties, `shape-rendering`, `pointer-events` and the like) are
/// passed over; `shape-rendering="crispEdges"` is therefore drawn
/// anti-aliased.
///
/// # Where it differs from a browser
///
/// * The picture is fitted to the pixels asked for as
///   `preserveAspectRatio="xMidYMid meet"` does, and what lies outside the
///   view box but inside those pixels shows, as in a browser.
/// * Gradient stops are interpolated as SVG says, channel by channel with
///   straight alpha, in the encoded space the renderer composites in.
#[derive(Clone)]
pub struct SvgPicture {
    art: Arc<Art>,
    current: Color,
}

impl fmt::Debug for SvgPicture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SvgPicture")
            .field("view_box", &self.art.view_box)
            .field("content_hash", &self.content_hash())
            .finish()
    }
}

impl PartialEq for SvgPicture {
    fn eq(&self, other: &Self) -> bool {
        self.content_hash() == other.content_hash()
    }
}

impl SvgPicture {
    /// Reads an SVG file. Anything outside the subset is an error; see the
    /// type's documentation. The work is proportional to the length of the
    /// source, and is meant to be done once, not per frame.
    pub fn parse(source: &str) -> Result<Self, SvgError> {
        build(source, false).map(|(art, _)| Self::from_art(art))
    }

    /// Reads an SVG file, leaving out what is outside the subset instead
    /// of refusing the file, and returns what was left out. Meant for
    /// looking at a file that has not been cleaned up yet: the picture can
    /// differ from the file wherever a note says so. Markup that is not
    /// well formed, a missing size and exceeded limits are still errors.
    pub fn parse_lenient(source: &str) -> Result<(Self, Vec<SvgError>), SvgError> {
        build(source, true).map(|(art, warnings)| (Self::from_art(art), warnings))
    }

    fn from_art(art: Art) -> Self {
        Self {
            art: Arc::new(art),
            current: Color::BLACK,
        }
    }

    /// A picture of nothing, one unit square.
    fn empty() -> Self {
        Self::from_art(Art {
            view_box: [0.0, 0.0, 1.0, 1.0],
            width: 0.0,
            height: 0.0,
            nodes: Vec::new(),
            gradients: Vec::new(),
            clips: Vec::new(),
            hash: 0,
            uses_current: false,
            single_ink: false,
        })
    }

    /// The picture's own width: the root's `width` where it is given in
    /// pixels, otherwise the view box's.
    pub fn width(&self) -> f32 {
        self.art.width
    }

    /// The picture's own height; see [`width`](Self::width).
    pub fn height(&self) -> f32 {
        self.art.height
    }

    /// The view box: `x y width height`.
    pub fn view_box(&self) -> [f32; 4] {
        self.art.view_box
    }

    /// The same picture with `currentColor` drawn in `color`. Cheap: the
    /// picture is shared, not copied.
    pub fn with_current_color(&self, color: Color) -> Self {
        Self {
            art: Arc::clone(&self.art),
            current: color,
        }
    }

    /// True if the file paints with `currentColor` anywhere.
    pub fn uses_current_color(&self) -> bool {
        self.art.uses_current
    }

    /// True if everything is painted in one colour (whatever its opacity):
    /// only `currentColor`, or only one literal colour. Such a picture is
    /// an icon that can take any colour when drawn, from one mask.
    pub fn is_single_ink(&self) -> bool {
        self.art.single_ink
    }

    /// A number that is the same for the same source and current colour:
    /// the key a rasterisation is cached under.
    pub fn content_hash(&self) -> u64 {
        if !self.art.uses_current {
            return self.art.hash;
        }
        let mut state = std::hash::DefaultHasher::new();
        self.art.hash.hash(&mut state);
        for value in [
            self.current.r,
            self.current.g,
            self.current.b,
            self.current.a,
        ] {
            value.to_bits().hash(&mut state);
        }
        state.finish()
    }

    /// Rasterises the picture into `width * height * 4` bytes of RGBA, row
    /// by row from the top: sRGB-encoded colour premultiplied by alpha, as
    /// the image store keeps images.
    ///
    /// The view box is scaled uniformly to fit and centred. Costs one
    /// coverage mask per fill, stroke and clip and one pass over the image
    /// per shape, and 16 bytes per pixel while it runs (as much again for
    /// each nested group that is faded or clipped as a whole).
    pub fn rasterise(&self, width: u32, height: u32) -> Vec<u8> {
        self.render(width, height).finish()
    }

    /// The picture's coverage alone: `width * height` bytes. For a
    /// [single-ink](Self::is_single_ink) picture this is the mask to tint.
    pub fn rasterise_mask(&self, width: u32, height: u32) -> Vec<u8> {
        self.render(width, height).alpha()
    }

    fn render(&self, width: u32, height: u32) -> Canvas {
        let mut canvas = Canvas::new(width, height);
        let [box_x, box_y, box_width, box_height] = self.art.view_box;
        if width == 0 || height == 0 || !(box_width > 0.0 && box_height > 0.0) {
            return canvas;
        }
        let scale = (width as f32 / box_width).min(height as f32 / box_height);
        let to_pixels = Transform::translate(-box_x, -box_y)
            .then(Transform::scale(scale, scale))
            .then(Transform::translate(
                (width as f32 - box_width * scale) / 2.0,
                (height as f32 - box_height * scale) / 2.0,
            ));
        self.draw(&mut canvas, &self.art.nodes, to_pixels, (width, height));
        canvas
    }

    fn color(&self, color: InkColor) -> Rgba {
        match color {
            InkColor::Fixed(color) => color,
            InkColor::Current(alpha) => {
                let [r, g, b, a] = self.current.srgb();
                [r, g, b, a * alpha]
            }
        }
    }

    /// The ink for one fill or stroke. A gradient is built into `storage`.
    fn ink<'a>(
        &self,
        ink: InkDef,
        shape: &ShapeNode,
        to_pixels: Transform,
        storage: &'a mut Option<GradientInk>,
    ) -> Option<Ink<'a>> {
        let (index, alpha) = match ink {
            InkDef::Color(color) => {
                let [r, g, b, a] = self.color(color);
                return Some(Ink::Solid([r * a, g * a, b * a, a]));
            }
            InkDef::Gradient { index, alpha } => (index, alpha),
        };
        let gradient = &self.art.gradients[index];
        let [x, y, width, height] = shape.bounds;
        let view = self.art.view_box;
        let mut own = gradient.transform;
        if gradient.bounding_box {
            // A box without area has no bounding-box space.
            if !(width > 0.0 && height > 0.0) {
                return None;
            }
            own = own
                .then(Transform::scale(width, height))
                .then(Transform::translate(x, y));
        }
        let from_pixels = own.then(shape.transform).then(to_pixels).inverse()?;
        if !from_pixels.is_finite() {
            return None;
        }
        let units = gradient.bounding_box;
        let g = gradient.geometry;
        let geometry = if gradient.linear {
            GradientShape::Linear {
                from: (g[0].resolve(units, view[2]), g[1].resolve(units, view[3])),
                to: (g[2].resolve(units, view[2]), g[3].resolve(units, view[3])),
            }
        } else {
            let diagonal = ((view[2] * view[2] + view[3] * view[3]) / 2.0).sqrt();
            let center = (g[0].resolve(units, view[2]), g[1].resolve(units, view[3]));
            let radius = g[2].resolve(units, diagonal).max(0.0);
            let mut focus = (g[3].resolve(units, view[2]), g[4].resolve(units, view[3]));
            // A focus outside the circle is moved onto it, just inside.
            let (dx, dy) = (focus.0 - center.0, focus.1 - center.1);
            let distance = (dx * dx + dy * dy).sqrt();
            if distance > radius * 0.999 && distance > 0.0 {
                let shrink = radius * 0.999 / distance;
                focus = (center.0 + dx * shrink, center.1 + dy * shrink);
            }
            GradientShape::Radial {
                center,
                radius,
                focus,
            }
        };
        let stops = gradient
            .stops
            .iter()
            .map(|&(offset, color)| {
                let [r, g, b, a] = self.color(color);
                (offset, [r, g, b, a * alpha])
            })
            .collect();
        Some(Ink::Gradient(storage.insert(GradientInk {
            shape: geometry,
            from_pixels,
            spread: gradient.spread,
            stops,
        })))
    }

    fn draw(&self, canvas: &mut Canvas, nodes: &[Node], to_pixels: Transform, size: (u32, u32)) {
        for node in nodes {
            let (clips, opacity) = match node {
                Node::Shape(shape) => (&shape.clips, shape.opacity),
                Node::Group { clips, opacity, .. } => (clips, *opacity),
            };
            let parts: Vec<Vec<ClipPart<'_>>> = clips
                .iter()
                .map(|used| {
                    self.art.clips[used.clip]
                        .iter()
                        .map(|shape| ClipPart {
                            commands: &shape.commands,
                            transform: shape.transform.then(used.transform).then(to_pixels),
                            even_odd: shape.even_odd,
                        })
                        .collect()
                })
                .collect();
            let clips: Vec<&[ClipPart<'_>]> = parts.iter().map(Vec::as_slice).collect();
            match node {
                Node::Shape(shape) => {
                    let (mut fill_gradient, mut stroke_gradient) = (None, None);
                    let fill = shape
                        .fill
                        .and_then(|ink| self.ink(ink, shape, to_pixels, &mut fill_gradient));
                    let stroke = shape.stroke.as_ref().and_then(|stroke| {
                        let ink = self.ink(stroke.ink, shape, to_pixels, &mut stroke_gradient)?;
                        Some(StrokePaint {
                            ink,
                            width: stroke.width,
                            cap: stroke.cap,
                            join: stroke.join,
                            miter_limit: stroke.miter_limit,
                            dashes: &stroke.dashes,
                            dash_offset: stroke.dash_offset,
                        })
                    });
                    canvas.paint(&Paint {
                        commands: &shape.commands,
                        transform: shape.transform.then(to_pixels),
                        fill,
                        even_odd: shape.even_odd,
                        stroke,
                        opacity,
                        clips: &clips,
                    });
                }
                Node::Group { children, .. } => {
                    let mut group = Canvas::new(size.0, size.1);
                    self.draw(&mut group, children, to_pixels, size);
                    canvas.blend(&group, opacity, &clips);
                }
            }
        }
    }
}

/// An SVG file bundled with the application, parsed the first time it is
/// used and kept: the `static` to put beside `include_str!`.
///
/// ```
/// use morningstar_text::SvgAsset;
///
/// // In an application: SvgAsset::new(include_str!("../assets/svg/disc.svg"))
/// static DOT: SvgAsset = SvgAsset::new(
///     r#"<svg viewBox="0 0 8 8"><circle cx="4" cy="4" r="3" fill="red"/></svg>"#,
/// );
/// assert!(DOT.check().is_ok());
/// assert_eq!(DOT.get().width(), 8.0);
/// ```
///
/// A file that does not load draws nothing, like an icon outside its
/// subset; [`check`](Self::check) says why, and belongs in a unit test
/// over an application's assets.
pub struct SvgAsset {
    source: &'static str,
    parsed: OnceLock<(SvgPicture, Option<SvgError>)>,
}

impl SvgAsset {
    /// An asset from the text of an SVG file. Nothing is parsed yet.
    pub const fn new(source: &'static str) -> Self {
        Self {
            source,
            parsed: OnceLock::new(),
        }
    }

    fn parsed(&self) -> &(SvgPicture, Option<SvgError>) {
        self.parsed
            .get_or_init(|| match SvgPicture::parse(self.source) {
                Ok(picture) => (picture, None),
                Err(error) => (SvgPicture::empty(), Some(error)),
            })
    }

    /// The picture: parsed on the first call, from any thread, and the
    /// same value afterwards. Empty if the file does not load.
    pub fn get(&self) -> &SvgPicture {
        &self.parsed().0
    }

    /// Why the file does not load, if it does not.
    pub fn check(&self) -> Result<(), SvgError> {
        match &self.parsed().1 {
            Some(error) => Err(error.clone()),
            None => Ok(()),
        }
    }
}
