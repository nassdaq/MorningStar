//! A small JSON reader, for the output of `cargo metadata`.
//!
//! It reads all of JSON (RFC 8259) into a tree and nothing more: no
//! writing, no borrowing from the input, no number type beyond `f64`. It
//! exists so the icon tool can take an app's identity from Cargo itself
//! without a dependency (ADR 0020). Input is never trusted: nesting is
//! limited and every error says where it is.

use std::fmt;

/// How deep arrays and objects may nest. `cargo metadata` needs about ten.
const MAX_DEPTH: usize = 64;

/// A JSON value.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Value>),
    /// Members in the order written. A repeated key keeps both; [`get`]
    /// returns the first.
    ///
    /// [`get`]: Value::get
    Object(Vec<(String, Value)>),
}

impl Value {
    /// The member `key` of an object; `None` for anything else.
    pub(crate) fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Self::Object(members) => members
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value),
            _ => None,
        }
    }

    pub(crate) fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(text) => Some(text),
            _ => None,
        }
    }

    pub(crate) fn as_array(&self) -> Option<&[Value]> {
        match self {
            Self::Array(items) => Some(items),
            _ => None,
        }
    }

    /// What kind of value this is, for an error message.
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool(_) => "a boolean",
            Self::Number(_) => "a number",
            Self::String(_) => "a string",
            Self::Array(_) => "an array",
            Self::Object(_) => "a table",
        }
    }
}

/// Why text is not JSON: what was wrong and at which byte.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct JsonError {
    pub(crate) message: &'static str,
    pub(crate) offset: usize,
}

impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.message, self.offset)
    }
}

/// Reads one JSON document.
pub(crate) fn parse(text: &str) -> Result<Value, JsonError> {
    let mut reader = Reader {
        bytes: text.as_bytes(),
        at: 0,
    };
    let value = reader.value(0)?;
    reader.skip_space();
    if reader.at != reader.bytes.len() {
        return Err(reader.error("text after the JSON value"));
    }
    Ok(value)
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn error(&self, message: &'static str) -> JsonError {
        JsonError {
            message,
            offset: self.at,
        }
    }

    fn skip_space(&mut self) {
        while matches!(self.bytes.get(self.at), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn literal(&mut self, word: &str, value: Value) -> Result<Value, JsonError> {
        if self.bytes[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            Ok(value)
        } else {
            Err(self.error("not a JSON value"))
        }
    }

    fn value(&mut self, depth: usize) -> Result<Value, JsonError> {
        if depth > MAX_DEPTH {
            return Err(self.error("nested too deeply"));
        }
        self.skip_space();
        match self.bytes.get(self.at) {
            None => Err(self.error("the text ends where a value should be")),
            Some(b'n') => self.literal("null", Value::Null),
            Some(b't') => self.literal("true", Value::Bool(true)),
            Some(b'f') => self.literal("false", Value::Bool(false)),
            Some(b'"') => self.string().map(Value::String),
            Some(b'[') => {
                self.at += 1;
                let mut items = Vec::new();
                self.skip_space();
                if self.bytes.get(self.at) == Some(&b']') {
                    self.at += 1;
                    return Ok(Value::Array(items));
                }
                loop {
                    items.push(self.value(depth + 1)?);
                    self.skip_space();
                    match self.bytes.get(self.at) {
                        Some(b',') => self.at += 1,
                        Some(b']') => {
                            self.at += 1;
                            return Ok(Value::Array(items));
                        }
                        _ => return Err(self.error("expected , or ] in an array")),
                    }
                }
            }
            Some(b'{') => {
                self.at += 1;
                let mut members = Vec::new();
                self.skip_space();
                if self.bytes.get(self.at) == Some(&b'}') {
                    self.at += 1;
                    return Ok(Value::Object(members));
                }
                loop {
                    self.skip_space();
                    if self.bytes.get(self.at) != Some(&b'"') {
                        return Err(self.error("expected a member name"));
                    }
                    let name = self.string()?;
                    self.skip_space();
                    if self.bytes.get(self.at) != Some(&b':') {
                        return Err(self.error("expected : after a member name"));
                    }
                    self.at += 1;
                    members.push((name, self.value(depth + 1)?));
                    self.skip_space();
                    match self.bytes.get(self.at) {
                        Some(b',') => self.at += 1,
                        Some(b'}') => {
                            self.at += 1;
                            return Ok(Value::Object(members));
                        }
                        _ => return Err(self.error("expected , or } in an object")),
                    }
                }
            }
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.error("not a JSON value")),
        }
    }

    fn number(&mut self) -> Result<Value, JsonError> {
        let start = self.at;
        while matches!(
            self.bytes.get(self.at),
            Some(b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
        ) {
            self.at += 1;
        }
        // The slice is ASCII by construction. Rust's grammar for f64 is a
        // superset of JSON's, which is close enough for a reader.
        std::str::from_utf8(&self.bytes[start..self.at])
            .ok()
            .and_then(|digits| digits.parse::<f64>().ok())
            .filter(|number| number.is_finite())
            .map(Value::Number)
            .ok_or(JsonError {
                message: "not a number",
                offset: start,
            })
    }

    /// Four hexadecimal digits after `\u`.
    fn hex4(&mut self) -> Result<u32, JsonError> {
        let digits = self
            .bytes
            .get(self.at..self.at + 4)
            .and_then(|digits| std::str::from_utf8(digits).ok())
            .filter(|digits| digits.bytes().all(|digit| digit.is_ascii_hexdigit()))
            .ok_or_else(|| self.error("\\u needs four hexadecimal digits"))?;
        self.at += 4;
        Ok(u32::from_str_radix(digits, 16).expect("four hexadecimal digits"))
    }

    fn string(&mut self) -> Result<String, JsonError> {
        self.at += 1; // The opening quote.
        let mut out = String::new();
        loop {
            let start = self.at;
            while !matches!(
                self.bytes.get(self.at),
                None | Some(b'"' | b'\\' | 0..=0x1F)
            ) {
                self.at += 1;
            }
            // The input is a &str and the run stops only at ASCII bytes, so
            // it is whole UTF-8 characters.
            out.push_str(std::str::from_utf8(&self.bytes[start..self.at]).expect("UTF-8 input"));
            match self.bytes.get(self.at) {
                None => return Err(self.error("the text ends inside a string")),
                Some(b'"') => {
                    self.at += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.at += 1;
                    let escape = self.bytes.get(self.at).copied();
                    self.at += 1;
                    match escape {
                        Some(b'"') => out.push('"'),
                        Some(b'\\') => out.push('\\'),
                        Some(b'/') => out.push('/'),
                        Some(b'b') => out.push('\u{8}'),
                        Some(b'f') => out.push('\u{C}'),
                        Some(b'n') => out.push('\n'),
                        Some(b'r') => out.push('\r'),
                        Some(b't') => out.push('\t'),
                        Some(b'u') => {
                            let mut code = self.hex4()?;
                            if (0xD800..0xDC00).contains(&code)
                                && self.bytes[self.at..].starts_with(b"\\u")
                            {
                                // A surrogate pair: one character in two escapes.
                                self.at += 2;
                                let low = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&low) {
                                    return Err(self.error("half a surrogate pair"));
                                }
                                code = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00);
                            }
                            out.push(
                                char::from_u32(code)
                                    .ok_or_else(|| self.error("half a surrogate pair"))?,
                            );
                        }
                        _ => {
                            self.at -= 1;
                            return Err(self.error("unknown escape in a string"));
                        }
                    }
                }
                Some(_) => return Err(self.error("a control character inside a string")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_every_kind_of_value() {
        let value =
            parse(r#" {"a": [1, -2.5e1, true, false, null], "b": {"c": "d"}, "e": [], "f": {}} "#)
                .unwrap();
        assert_eq!(
            value.get("a").and_then(Value::as_array),
            Some(
                &[
                    Value::Number(1.0),
                    Value::Number(-25.0),
                    Value::Bool(true),
                    Value::Bool(false),
                    Value::Null,
                ][..]
            )
        );
        assert_eq!(
            value
                .get("b")
                .and_then(|b| b.get("c"))
                .and_then(Value::as_str),
            Some("d")
        );
        assert_eq!(value.get("e"), Some(&Value::Array(Vec::new())));
        assert_eq!(value.get("f"), Some(&Value::Object(Vec::new())));
        assert_eq!(value.get("missing"), None);
        assert_eq!(Value::Null.get("a"), None);
    }

    #[test]
    fn reads_escapes_and_text_outside_ascii() {
        let value = parse(r#""a\"b\\c\/d\n\té😀 – 日本""#).unwrap();
        assert_eq!(value.as_str(), Some("a\"b\\c/d\n\té😀 – 日本"));
    }

    #[test]
    fn errors_say_where() {
        for (text, offset) in [
            ("", 0),
            ("[1,", 3),
            ("[1 2]", 3),
            (r#"{"a" 1}"#, 5),
            (r#"{a: 1}"#, 1),
            (r#""abc"#, 4),
            (r#""\x""#, 2),
            (r#""\ud83d""#, 7),
            (r#""\u12""#, 3),
            ("\"a\nb\"", 2),
            ("nul", 0),
            ("1 1", 2),
            ("--1", 0),
            ("1e999", 0),
        ] {
            let error = parse(text).expect_err(text);
            assert_eq!(error.offset, offset, "{text:?}: {error}");
        }
    }

    #[test]
    fn deep_nesting_is_refused_not_overflowed() {
        let deep = "[".repeat(100_000);
        assert_eq!(parse(&deep).unwrap_err().message, "nested too deeply");
        let fine = format!("{}{}", "[".repeat(60), "]".repeat(60));
        assert!(parse(&fine).is_ok());
    }
}
