//! A small JSON reader and string writer: all the app needs to talk to its
//! server.
//!
//! Written here rather than taken from a crate because the need is small
//! (decode a handful of flat objects, encode a few strings) and the app
//! otherwise depends on nothing but the framework. Everything a server
//! sends is untrusted: the reader never panics, refuses input that is too
//! large or nested too deeply, and reports any malformed text as an
//! [`Error`]. It accepts exactly the JSON of RFC 8259.

/// The largest text [`parse`] reads, in bytes. The server's largest answer
/// (a report with its transcript) is a few kilobytes.
pub const MAX_BYTES: usize = 1024 * 1024;
/// The deepest nesting of arrays and objects [`parse`] follows.
pub const MAX_DEPTH: usize = 24;

/// A JSON value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A number.
    Number(f64),
    /// A string, unescaped.
    String(String),
    /// An array.
    Array(Vec<Value>),
    /// An object, its members in the order written.
    Object(Vec<(String, Value)>),
}

impl Value {
    /// The member `key` of an object; `None` for anything else, and for a
    /// missing member. Of two members with one name the first counts.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Self::Object(members) => members
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value),
            _ => None,
        }
    }

    /// The string, if this is one.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(text) => Some(text),
            _ => None,
        }
    }

    /// The boolean, if this is one.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    /// The number, if this is one.
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            _ => None,
        }
    }

    /// The number as a whole number from `low` to `high`, if this is one.
    /// `82.0` counts; `82.5`, and anything out of range, does not.
    pub fn as_integer(&self, low: u32, high: u32) -> Option<u32> {
        let number = self.as_number()?;
        (number.fract() == 0.0 && number >= f64::from(low) && number <= f64::from(high))
            .then_some(number as u32)
    }

    /// The elements, if this is an array.
    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Self::Array(elements) => Some(elements),
            _ => None,
        }
    }

    /// True for `null`.
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }
}

/// Why a text is not JSON this reader accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Longer than [`MAX_BYTES`].
    TooLarge,
    /// Nested deeper than [`MAX_DEPTH`].
    TooDeep,
    /// Not JSON, at this byte.
    Syntax(usize),
}

/// Reads one JSON value, which must be the whole text.
pub fn parse(text: &str) -> Result<Value, Error> {
    if text.len() > MAX_BYTES {
        return Err(Error::TooLarge);
    }
    let mut reader = Reader {
        bytes: text.as_bytes(),
        text,
        at: 0,
    };
    reader.space();
    let value = reader.value(0)?;
    reader.space();
    if reader.at == reader.bytes.len() {
        Ok(value)
    } else {
        Err(Error::Syntax(reader.at))
    }
}

/// `text` as a JSON string, with its quotes: `"I’m"` stays as it is,
/// a quote, a backslash and every control character are escaped.
pub fn string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if (control as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", control as u32));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

struct Reader<'a> {
    bytes: &'a [u8],
    text: &'a str,
    at: usize,
}

impl Reader<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn fail<T>(&self) -> Result<T, Error> {
        Err(Error::Syntax(self.at))
    }

    fn space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn word(&mut self, word: &str, value: Value) -> Result<Value, Error> {
        if self.bytes[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            Ok(value)
        } else {
            self.fail()
        }
    }

    fn value(&mut self, depth: usize) -> Result<Value, Error> {
        match self.peek() {
            Some(b'n') => self.word("null", Value::Null),
            Some(b't') => self.word("true", Value::Bool(true)),
            Some(b'f') => self.word("false", Value::Bool(false)),
            Some(b'"') => self.string().map(Value::String),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(b'[') => {
                if depth >= MAX_DEPTH {
                    return Err(Error::TooDeep);
                }
                self.at += 1;
                let mut elements = Vec::new();
                self.space();
                if self.peek() == Some(b']') {
                    self.at += 1;
                    return Ok(Value::Array(elements));
                }
                loop {
                    self.space();
                    elements.push(self.value(depth + 1)?);
                    self.space();
                    match self.peek() {
                        Some(b',') => self.at += 1,
                        Some(b']') => {
                            self.at += 1;
                            return Ok(Value::Array(elements));
                        }
                        _ => return self.fail(),
                    }
                }
            }
            Some(b'{') => {
                if depth >= MAX_DEPTH {
                    return Err(Error::TooDeep);
                }
                self.at += 1;
                let mut members = Vec::new();
                self.space();
                if self.peek() == Some(b'}') {
                    self.at += 1;
                    return Ok(Value::Object(members));
                }
                loop {
                    self.space();
                    if self.peek() != Some(b'"') {
                        return self.fail();
                    }
                    let name = self.string()?;
                    self.space();
                    if self.peek() != Some(b':') {
                        return self.fail();
                    }
                    self.at += 1;
                    self.space();
                    members.push((name, self.value(depth + 1)?));
                    self.space();
                    match self.peek() {
                        Some(b',') => self.at += 1,
                        Some(b'}') => {
                            self.at += 1;
                            return Ok(Value::Object(members));
                        }
                        _ => return self.fail(),
                    }
                }
            }
            _ => self.fail(),
        }
    }

    fn number(&mut self) -> Result<Value, Error> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        match self.peek() {
            Some(b'0') => self.at += 1,
            Some(b'1'..=b'9') => self.digits(),
            _ => return self.fail(),
        }
        if self.peek() == Some(b'.') {
            self.at += 1;
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return self.fail();
            }
            self.digits();
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return self.fail();
            }
            self.digits();
        }
        // The slice is ASCII digits, signs, a point and an exponent.
        match self.text[start..self.at].parse::<f64>() {
            Ok(number) if number.is_finite() => Ok(Value::Number(number)),
            _ => Err(Error::Syntax(start)),
        }
    }

    fn digits(&mut self) {
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.at += 1;
        }
    }

    /// Four hexadecimal digits at the cursor.
    fn hex4(&mut self) -> Result<u32, Error> {
        let Some(digits) = self.bytes.get(self.at..self.at + 4) else {
            return self.fail();
        };
        let mut value = 0;
        for digit in digits {
            let Some(digit) = char::from(*digit).to_digit(16) else {
                return self.fail();
            };
            value = value * 16 + digit;
        }
        self.at += 4;
        Ok(value)
    }

    fn string(&mut self) -> Result<String, Error> {
        // The opening quote.
        self.at += 1;
        let mut out = String::new();
        loop {
            let run = self.at;
            while !matches!(self.peek(), None | Some(b'"' | b'\\' | 0..=0x1f)) {
                self.at += 1;
            }
            // `run..at` ends before an ASCII byte or at the end, so it lies
            // on character boundaries of the text.
            out.push_str(&self.text[run..self.at]);
            match self.peek() {
                Some(b'"') => {
                    self.at += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.at += 1;
                    let escape = self.peek();
                    self.at += 1;
                    out.push(match escape {
                        Some(b'"') => '"',
                        Some(b'\\') => '\\',
                        Some(b'/') => '/',
                        Some(b'b') => '\u{8}',
                        Some(b'f') => '\u{c}',
                        Some(b'n') => '\n',
                        Some(b'r') => '\r',
                        Some(b't') => '\t',
                        Some(b'u') => self.escaped_character()?,
                        _ => {
                            self.at -= 1;
                            return self.fail();
                        }
                    });
                }
                // A raw control character, or the text ended.
                _ => return self.fail(),
            }
        }
    }

    /// The character of a `\uXXXX` escape whose `\u` has been read: one
    /// code unit, or a surrogate pair. A lone surrogate is the replacement
    /// character, as a lenient reader makes it.
    fn escaped_character(&mut self) -> Result<char, Error> {
        let first = self.hex4()?;
        if (0xD800..0xDC00).contains(&first) {
            if self.bytes[self.at..].starts_with(b"\\u") {
                let back = self.at;
                self.at += 2;
                let second = self.hex4()?;
                if (0xDC00..0xE000).contains(&second) {
                    let scalar = 0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00);
                    return Ok(char::from_u32(scalar).unwrap_or(char::REPLACEMENT_CHARACTER));
                }
                // Not a low surrogate: it is a character of its own.
                self.at = back;
            }
            return Ok(char::REPLACEMENT_CHARACTER);
        }
        Ok(char::from_u32(first).unwrap_or(char::REPLACEMENT_CHARACTER))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_every_kind_of_value() {
        let value =
            parse(r#" {"a": null, "b": [true, false], "c": -12.5e1, "d": "x", "e": {}, "f": []} "#)
                .expect("valid");
        assert!(value.get("a").is_some_and(Value::is_null));
        assert_eq!(
            value.get("b").and_then(Value::as_array),
            Some(&[Value::Bool(true), Value::Bool(false)][..])
        );
        assert_eq!(value.get("c").and_then(Value::as_number), Some(-125.0));
        assert_eq!(value.get("d").and_then(Value::as_str), Some("x"));
        assert_eq!(value.get("e"), Some(&Value::Object(Vec::new())));
        assert_eq!(value.get("f"), Some(&Value::Array(Vec::new())));
        assert_eq!(value.get("g"), None);
        assert_eq!(Value::Null.get("a"), None);
    }

    #[test]
    fn unescapes_strings() {
        let value = parse(r#""a\"b\\c\/d\n\t’😀é""#).expect("valid");
        assert_eq!(value.as_str(), Some("a\"b\\c/d\n\t\u{2019}\u{1F600}\u{e9}"));
        // Unescaped text beyond ASCII passes through.
        assert_eq!(
            parse("\"I\u{2019}m \u{1F600}\"").expect("valid").as_str(),
            Some("I\u{2019}m \u{1F600}")
        );
        // A lone surrogate does not stop the reader.
        assert_eq!(
            parse(r#""\ud83dxA""#).expect("valid").as_str(),
            Some("\u{fffd}xA")
        );
        assert_eq!(
            parse(r#""\ud83dA""#).expect("valid").as_str(),
            Some("\u{fffd}A")
        );
    }

    #[test]
    fn whole_numbers_are_told_from_the_rest() {
        let number = |text: &str| parse(text).expect("valid");
        assert_eq!(number("82").as_integer(0, 100), Some(82));
        assert_eq!(number("82.0").as_integer(0, 100), Some(82));
        assert_eq!(number("8.2e1").as_integer(0, 100), Some(82));
        assert_eq!(number("82.5").as_integer(0, 100), None);
        assert_eq!(number("101").as_integer(0, 100), None);
        assert_eq!(number("-1").as_integer(0, 100), None);
        assert_eq!(number("\"82\"").as_integer(0, 100), None);
    }

    #[test]
    fn malformed_text_is_an_error_never_a_panic() {
        for text in [
            "",
            " ",
            "{",
            "}",
            "[1,",
            "[1,]",
            "{\"a\"}",
            "{\"a\":}",
            "{a:1}",
            "{\"a\":1,}",
            "nul",
            "truex",
            "01",
            "1.",
            ".5",
            "1e",
            "-",
            "+1",
            "1e999",
            "\"abc",
            "\"a\\x\"",
            "\"a\\u12\"",
            "\"a\\u12g4\"",
            "\"a\nb\"",
            "\"\\",
            "1 2",
            "[1] x",
            "<html>502 Bad Gateway</html>",
            "\u{feff}{}",
        ] {
            assert!(
                matches!(parse(text), Err(Error::Syntax(_))),
                "{text:?} was accepted"
            );
        }
        // Every prefix of a valid document is handled.
        let document = r#"{"line": {"text": "Fine.’", "audio_url": null}, "tips": ["a", "b"], "turn": 2, "done": false}"#;
        for end in 0..document.len() {
            if document.is_char_boundary(end) {
                assert!(parse(&document[..end]).is_err(), "{end}");
            }
        }
        assert!(parse(document).is_ok());
    }

    #[test]
    fn size_and_depth_are_bounded() {
        let deep = "[".repeat(MAX_DEPTH + 1) + &"]".repeat(MAX_DEPTH + 1);
        assert_eq!(parse(&deep), Err(Error::TooDeep));
        let allowed = "[".repeat(MAX_DEPTH) + &"]".repeat(MAX_DEPTH);
        assert!(parse(&allowed).is_ok());
        // A hundred thousand brackets must not exhaust the stack.
        assert_eq!(parse(&"[".repeat(100_000)), Err(Error::TooDeep));
        let large = format!("\"{}\"", "x".repeat(MAX_BYTES));
        assert_eq!(parse(&large), Err(Error::TooLarge));
    }

    #[test]
    fn writes_strings_that_read_back() {
        for text in [
            "",
            "plain",
            "I\u{2019}m \"sorry\" \\ about that\n\ttab\r\u{1}\u{1f}",
            "\u{1F600} emoji",
        ] {
            let written = string(text);
            assert_eq!(parse(&written).expect("valid").as_str(), Some(text));
        }
        assert_eq!(string("a\"b\\\n\u{1}"), r#""a\"b\\\n\u0001""#);
    }
}
