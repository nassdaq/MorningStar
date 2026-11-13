//! The light formatting of an answer (`server/API.md`): paragraphs, list
//! items, `inline code` and fenced code blocks, read out of plain text.
//!
//! Nothing here draws. [`blocks`] splits an answer into what the chat
//! screen lays out one under the other, and [`inline`] splits a line into
//! its plain and code runs. A text with none of the marks is one
//! [`Block::Paragraph`] holding the text as it is, which is what the
//! scripted answers (and the design's) are.

/// One piece of an answer, laid out under the piece before it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    /// Running text. Line breaks inside it are kept.
    Paragraph(String),
    /// A list item: a line that began `- `.
    Bullet(String),
    /// A numbered item: its number as written (`2.`) and its text.
    Numbered(String, String),
    /// A fenced code block: its lines, without the fences.
    Code(String),
}

/// A run of a line: plain, or code that was between backticks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Run {
    /// As written.
    Plain(String),
    /// Between backticks.
    Code(String),
}

fn fence(line: &str) -> bool {
    line.trim_start().starts_with("```")
}

/// `2. text` as (`2.`, `text`).
fn numbered(line: &str) -> Option<(&str, &str)> {
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 || digits > 3 {
        return None;
    }
    let rest = line[digits..].strip_prefix(". ")?;
    Some((&line[..=digits], rest.trim_start()))
}

/// The blocks of an answer, in order.
pub fn blocks(text: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut paragraph: Vec<&str> = Vec::new();
    let mut code: Option<Vec<&str>> = None;
    let flush = |paragraph: &mut Vec<&str>, out: &mut Vec<Block>| {
        if !paragraph.is_empty() {
            out.push(Block::Paragraph(paragraph.join("\n")));
            paragraph.clear();
        }
    };
    for line in text.split('\n') {
        if let Some(lines) = &mut code {
            if fence(line) {
                out.push(Block::Code(lines.join("\n")));
                code = None;
            } else {
                lines.push(line);
            }
            continue;
        }
        if fence(line) {
            flush(&mut paragraph, &mut out);
            code = Some(Vec::new());
        } else if line.trim().is_empty() {
            flush(&mut paragraph, &mut out);
        } else if let Some(item) = line.strip_prefix("- ") {
            flush(&mut paragraph, &mut out);
            out.push(Block::Bullet(item.trim_start().to_owned()));
        } else if let Some((number, item)) = numbered(line) {
            flush(&mut paragraph, &mut out);
            out.push(Block::Numbered(number.to_owned(), item.to_owned()));
        } else {
            paragraph.push(line);
        }
    }
    // A fence that was never closed still holds code.
    if let Some(lines) = code {
        out.push(Block::Code(lines.join("\n")));
    }
    flush(&mut paragraph, &mut out);
    out.retain(|block| !matches!(block, Block::Code(code) if code.trim().is_empty()));
    out
}

/// The runs of a line: what is between a pair of backticks is code. A
/// backtick with no partner is an ordinary character.
pub fn inline(line: &str) -> Vec<Run> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(open) = rest.find('`') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('`') else {
            break;
        };
        if close == 0 {
            // "``": nothing between them; keep them as they are.
            out.push(Run::Plain(rest[..open + 2].to_owned()));
            rest = &after[1..];
            continue;
        }
        if open > 0 {
            out.push(Run::Plain(rest[..open].to_owned()));
        }
        out.push(Run::Code(after[..close].to_owned()));
        rest = &after[close + 1..];
    }
    if !rest.is_empty() {
        out.push(Run::Plain(rest.to_owned()));
    }
    out
}

/// True if the text has none of the formatting: it is one paragraph with
/// no code in it, to be shown exactly as it is.
pub fn plain(text: &str) -> bool {
    !text.contains('`')
        && matches!(blocks(text).as_slice(), [Block::Paragraph(only)] if only == text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(text: &str) -> Block {
        Block::Paragraph(text.to_owned())
    }

    #[test]
    fn an_answer_is_split_into_its_blocks() {
        let text = "Quote B is the cheapest.\nBy $120.\n\n- A: $4,440\n- B: $4,180\n\n1. Ask B\n2. Match A\n10. Done\n\n```rust\nfn main() {\n\n    run();\n}\n```\nThat is all.";
        assert_eq!(
            blocks(text),
            [
                p("Quote B is the cheapest.\nBy $120."),
                Block::Bullet("A: $4,440".to_owned()),
                Block::Bullet("B: $4,180".to_owned()),
                Block::Numbered("1.".to_owned(), "Ask B".to_owned()),
                Block::Numbered("2.".to_owned(), "Match A".to_owned()),
                Block::Numbered("10.".to_owned(), "Done".to_owned()),
                Block::Code("fn main() {\n\n    run();\n}".to_owned()),
                p("That is all."),
            ]
        );
    }

    #[test]
    fn odd_input_is_still_text() {
        assert_eq!(blocks(""), []);
        assert_eq!(blocks("\n\n"), []);
        assert_eq!(
            blocks("-not a list\n3.14 is pi\n1999. A year"),
            [p("-not a list\n3.14 is pi\n1999. A year")]
        );
        assert_eq!(blocks("```\nopen"), [Block::Code("open".to_owned())]);
        assert_eq!(blocks("```\n```\nafter"), [p("after")]);
        assert_eq!(blocks("- "), [Block::Bullet(String::new())]);
    }

    #[test]
    fn inline_code_is_what_is_between_backticks() {
        let plain = |text: &str| Run::Plain(text.to_owned());
        let code = |text: &str| Run::Code(text.to_owned());
        assert_eq!(
            inline("Use `map` and `filter`."),
            [
                plain("Use "),
                code("map"),
                plain(" and "),
                code("filter"),
                plain(".")
            ]
        );
        assert_eq!(inline("no code"), [plain("no code")]);
        assert_eq!(inline("a ` alone"), [plain("a ` alone")]);
        assert_eq!(inline("`x`"), [code("x")]);
        assert_eq!(
            inline("`` and `y`"),
            [plain("``"), plain(" and "), code("y")]
        );
        assert_eq!(inline(""), []);
    }

    #[test]
    fn the_scripted_answers_are_plain() {
        assert!(plain(
            "Quote B is the cheapest at $4,180, 6% under Quote A."
        ));
        assert!(!plain("One.\n\nTwo."));
        assert!(!plain("- item"));
        assert!(!plain("Use `x`."));
        assert!(plain("One\ntwo"));
    }
}
