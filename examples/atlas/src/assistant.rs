//! The scripted assistant: what answers when the app's server does not.
//!
//! [`respond`] is the whole of it: the chat hands it a [`Request`] and
//! gets a [`Response`] back, which the app then plays out (the steps one
//! by one on the timer, then the reply). It reads nothing and sends
//! nothing.
//!
//! With a server behind the app (`server/`, and [`crate::api`]) the same
//! [`Response`] is built from the server's answer instead (its steps and
//! its text; `Atlas::take_reply` in `lib.rs`) and played out by the same
//! code, so the chat screen does not know where an answer came from.
//! Streaming is not available in the effects API (ADR 0019), so an answer
//! arrives whole.

use crate::Mode;

/// What is asked of the model.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    /// The model the user chose in the picker.
    pub mode: Mode,
    /// What the user wrote or said.
    pub prompt: String,
    /// What is attached, as the chip names it ("3 PDFs").
    pub attachment: Option<String>,
    /// The earlier prompts of the conversation, oldest first.
    pub history: Vec<String>,
}

/// One thing the assistant does on the way to its answer.
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    /// While it is being done: "Comparing totals".
    pub doing: String,
    /// In the list: "Compare totals".
    pub name: String,
}

/// The model's answer.
#[derive(Clone, Debug, PartialEq)]
pub struct Response {
    /// Said at once: "On it. ..".
    pub intro: String,
    /// What it does before answering, shown as "Step 2 of 4".
    pub steps: Vec<Step>,
    /// The answer itself.
    pub reply: String,
}

fn steps(pairs: &[(&str, &str)]) -> Vec<Step> {
    pairs
        .iter()
        .map(|(doing, name)| Step {
            doing: (*doing).to_owned(),
            name: (*name).to_owned(),
        })
        .collect()
}

/// The design's own conversation: the steps of "Compare supplier quotes".
pub fn quote_steps() -> Vec<Step> {
    steps(&[
        ("Reading documents", "Read 3 documents"),
        ("Extracting prices", "Extract line items and prices"),
        ("Comparing totals", "Compare totals"),
        ("Writing summary", "Write summary"),
    ])
}

/// The design's own first line of an answer.
pub const QUOTE_INTRO: &str =
    "On it. I\u{2019}ll check the totals against each other before I write the summary.";

/// The scripted stand-in for a model: picks an answer by what the prompt
/// is about. It reads nothing and sends nothing.
pub fn respond(request: &Request) -> Response {
    let prompt = request.prompt.to_lowercase();
    let about = |words: &[&str]| words.iter().any(|word| prompt.contains(word));
    let model = match request.mode {
        Mode::Auto => "",
        Mode::Fast => " (Fast)",
        Mode::Smart => " (Smart)",
        Mode::Code => " (Code)",
    };
    let (intro, steps, reply) = if about(&["quote", "price", "cheapest"]) {
        (
            QUOTE_INTRO.to_owned(),
            quote_steps(),
            "Quote B is the cheapest at $4,180, 6% under Quote A and 11% under Quote C. \
             A is the only one that includes delivery; with delivery added, B still wins by \
             $120. I\u{2019}d go with B and ask them to match A\u{2019}s 30-day terms."
                .to_owned(),
        )
    } else if about(&["meeting", "recorded", "transcri", "action item"]) {
        (
            "Sure. I\u{2019}ll go through the recording first.".to_owned(),
            steps(&[
                ("Transcribing audio", "Transcribe the recording"),
                ("Finding decisions", "Find decisions"),
                ("Listing actions", "List action items"),
            ]),
            "The meeting agreed to ship the pilot on the 14th. Actions: Amina sends the \
             revised budget by Friday; Jonas books the venue; you confirm the guest list."
                .to_owned(),
        )
    } else if about(&["pdf", "summar", "document", "file"]) {
        (
            "On it. I\u{2019}ll read it through before I summarise.".to_owned(),
            steps(&[
                ("Reading the document", "Read the document"),
                ("Finding key points", "Find the key points"),
                ("Writing summary", "Write summary"),
            ]),
            "Attach the PDF with + and I\u{2019}ll condense it to the five points that \
             matter, with page numbers so you can check each one."
                .to_owned(),
        )
    } else if about(&["research", "source", "web"]) {
        (
            "Let me look that up and check the sources against each other.".to_owned(),
            steps(&[
                ("Searching", "Search the web"),
                ("Reading sources", "Read 6 sources"),
                ("Cross-checking", "Cross-check claims"),
                ("Writing answer", "Write the answer"),
            ]),
            "Tell me the topic and how deep to go. I\u{2019}ll answer with numbered \
             sources you can open, and say where they disagree."
                .to_owned(),
        )
    } else if about(&["image", "picture", "draw"]) {
        (
            "Happy to. Image generation uses credits, so I\u{2019}ll show an estimate first."
                .to_owned(),
            steps(&[
                ("Estimating credits", "Estimate credits"),
                ("Drafting a prompt", "Draft a prompt"),
            ]),
            "Describe the picture: subject, style and shape. One image is about 4 credits \
             on your plan."
                .to_owned(),
        )
    } else if about(&["translat"]) {
        (
            "Of course.".to_owned(),
            steps(&[("Detecting language", "Detect the language")]),
            "Send me the text, say it aloud, or take a photo of a sign, and tell me the \
             language you want it in."
                .to_owned(),
        )
    } else if about(&["spreadsheet", "data", "table"]) {
        (
            "Sure. I\u{2019}ll look at the columns first.".to_owned(),
            steps(&[
                ("Reading the sheet", "Read the sheet"),
                ("Checking the data", "Check the data"),
            ]),
            "Attach the spreadsheet with + and ask in plain words: totals, trends, \
             outliers, or a chart."
                .to_owned(),
        )
    } else {
        (
            "Let me think about that.".to_owned(),
            steps(&[
                ("Thinking", "Think it through"),
                ("Writing answer", "Write the answer"),
            ]),
            "This is a scripted reply: this example has no model behind it. The steps \
             above, this bubble and the model you picked are the whole round trip a real \
             one would make."
                .to_owned(),
        )
    };
    Response {
        intro,
        steps,
        reply: format!("{reply}{model}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(prompt: &str, mode: Mode) -> Response {
        respond(&Request {
            mode,
            prompt: prompt.to_owned(),
            attachment: None,
            history: Vec::new(),
        })
    }

    #[test]
    fn the_answer_follows_the_prompt_and_names_the_model() {
        assert_eq!(
            ask("Compare the prices in these quotes", Mode::Auto)
                .steps
                .len(),
            4
        );
        assert_eq!(ask("Summarise a PDF", Mode::Auto).steps.len(), 3);
        assert!(ask("hello", Mode::Smart).reply.ends_with("(Smart)"));
        assert!(!ask("hello", Mode::Auto).reply.contains('('));
        for prompt in ["", "Create an image", "Translate this", "Research a topic"] {
            let response = ask(prompt, Mode::Fast);
            assert!(!response.steps.is_empty() && !response.reply.is_empty());
        }
    }
}
