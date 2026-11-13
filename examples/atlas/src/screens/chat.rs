//! Screens 04 and 10: a conversation. `design/html/04-chat.html` (and
//! `10-chat-simplified.html`, the same page whose back arrow leads to 09).
//!
//! The title and the model chip, the exchanges, and the composer. Each
//! exchange is what the user sent (with its attachment), the assistant's
//! first line, a card that says which step it is on, and the answer once
//! the steps are done. The card unfolds into the list of steps, with
//! "Cancel task" while there is something to cancel.
//!
//! With a server behind the app the same card says "Thinking" while the
//! message is with the model and then runs through the steps the server
//! returned. What the design does not show is the app's own, built from
//! the design's parts: the line and the "Retry" pill under a message the
//! server did not answer (the "Cancel task" pill again), the answer's
//! light formatting (paragraphs, list items, code on a tinted card; there
//! is no monospace face among the app's fonts, so code is set in the same
//! one), the model that answered when the choice was Auto, "Stop" beside
//! an answer being spoken, a chat that is still loading, and "Delete this
//! chat" under a chat the server holds. None of it appears in a scripted
//! conversation.

use morningstar::ui::prelude::*;

use crate::components::{
    MARGIN, TOP, attachment_chip, composer, model_chip, page, spinner, top_bar,
};
use crate::format::{self, Block, Run};
use crate::icons::{self, icon};
use crate::screens::home::pending_attachment;
use crate::theme::{
    ACCENT, INK_05, LINE, LINE_20, MUTED, QUIET, RADIUS_CARD, RADIUS_STEPS, RADIUS_TAIL,
    RADIUS_TILE, TINT_09, WHITE, jakarta, jakarta_rich,
};
use crate::{Atlas, Loaded, Message, Phase, Turn};

/// The design's small outlined pill ("Cancel task"): `height: 44px;
/// padding: 0 18px; border-radius: 22px; border: 1px solid
/// rgba(29,24,48,0.2); font-size: 13px; font-weight: 600`. The app's own
/// pills ("Retry", "Stop") are the same at `height` 48, the least Android
/// asks of something to tap; the design's stays as drawn.
fn pill(label: &'static str, height: f32, message: Message) -> Container<Message> {
    Container::new(jakarta(label, 13.0).semibold())
        .center()
        .height(height)
        .padding_xy(18.0, 0.0)
        .radius(height / 2.0)
        .border(1.0, LINE_20)
        .on_press(message)
}

/// The 18 dp dashed ring of a step not reached: `border: 1.5px dashed
/// #8E88A3`.
fn ring() -> Container<Message> {
    Container::empty()
        .size(18.0, 18.0)
        .radius(9.0)
        .border_dashed(1.5, QUIET)
}

/// What the user sent: `align-self: flex-end; max-width: 290px; display:
/// flex; flex-direction: column; align-items: flex-end; gap: 8px`.
fn sent(index: usize, turn: &Turn) -> Container<Message> {
    Column::new()
        .push_maybe(turn.attachment.clone().map(|name| {
            attachment_chip(name.clone())
                .on_press(Message::AttachmentInfo)
                .label(format!("Attachment: {name}"))
                .id(("attachment", index))
        }))
        .push(
            // `padding: 12px 16px; border-radius: 20px 20px 6px 20px;
            // background: rgba(160,40,122,0.09); font-size: 15px;
            // line-height: 1.5`.
            Container::new(jakarta(turn.prompt.clone(), 15.0).line_height_dp(22.5))
                .padding_xy(16.0, 12.0)
                .radius(RADIUS_CARD)
                .radii(RADIUS_CARD, RADIUS_CARD, RADIUS_TAIL, RADIUS_CARD)
                .background(TINT_09),
        )
        .gap(8.0)
        .cross_align(CrossAlign::End)
        .max_width(290.0)
        .align_self(CrossAlign::End)
}

/// One line of the unfolded card: `display: flex; align-items: center;
/// gap: 12px`, an 18 dp mark and `font-size: 14px`.
fn step_line(mark: Element<Message>, name: String, current: bool) -> Container<Message> {
    let name = jakarta(name, 14.0);
    row![
        mark,
        if current {
            name.bold()
        } else {
            name.color(MUTED)
        }
        .fill_width(),
    ]
    .gap(12.0)
    .cross_align(CrossAlign::Center)
    .fill_width()
}

/// The step card:
///
/// ```css
/// div    { display: flex; flex-direction: column; border-radius: 18px;
///          background: #FFFFFF; border: 1px solid rgba(29,24,48,0.08) }
/// button { min-height: 52px; display: flex; align-items: center; gap: 12px; padding: 0 14px }
/// list   { display: flex; flex-direction: column; gap: 12px; padding: 4px 14px 14px;
///          border-top: 1px solid rgba(29,24,48,0.08) }
/// ```
fn steps(app: &Atlas, index: usize, turn: &Turn) -> Container<Message> {
    let total = turn.steps.len();
    let tick = || icon(&icons::CHECK, 18.0, ACCENT);
    // The header says where the answer is. The design shows it mid-way
    // ("Comparing totals", "Step 3 of 4"); before and after are the app's.
    let (mark, title, count): (Element<Message>, String, String) = match turn.phase {
        Phase::Thinking => (spinner(app.spin()).into(), "Thinking".into(), String::new()),
        Phase::Working(step) => (
            spinner(app.spin()).into(),
            turn.steps[step].doing.clone(),
            format!("Step {} of {total}", step + 1),
        ),
        Phase::Done => (
            tick().into(),
            "Done".into(),
            format!("{total} step{}", if total == 1 { "" } else { "s" }),
        ),
        Phase::Cancelled => (ring().into(), "Cancelled".into(), String::new()),
        Phase::Failed => (ring().into(), "No answer".into(), String::new()),
    };
    let header = row![
        mark,
        // `flex-grow: 1; font-size: 14px; font-weight: 600`.
        jakarta(title, 14.0).semibold().fill_width(),
        // `font-size: 13px; color: #5E5873`.
        jakarta(count, 13.0).color(MUTED),
        icon(&icons::CHEVRON_DOWN, 15.0, MUTED),
    ]
    .gap(12.0)
    .cross_align(CrossAlign::Center)
    .fill_width()
    .min_height(52.0)
    .padding_xy(14.0, 0.0)
    .radius(RADIUS_STEPS - 1.0)
    .on_press(Message::ToggleSteps(index))
    .role(Role::Button)
    .hint(if turn.open {
        "Hides the steps"
    } else {
        "Shows the steps"
    })
    .live(Live::Polite)
    .id(("steps", index));
    let mut card = Column::new().push(header);
    // A message that is with the server has no steps yet, but it can be
    // abandoned: its card unfolds to "Cancel task" alone.
    if turn.open && (total > 0 || turn.waiting()) {
        let current = match turn.phase {
            Phase::Working(step) => step,
            Phase::Done => total,
            Phase::Thinking | Phase::Cancelled | Phase::Failed => 0,
        };
        let working = matches!(turn.phase, Phase::Working(_));
        let mut list = Column::new();
        for (step, item) in turn.steps.iter().enumerate() {
            let mark: Element<Message> = if step < current {
                // `stroke="#A0287A" stroke-width="2.4"`, 18 dp.
                tick().into()
            } else if step == current && working {
                spinner(app.spin()).into()
            } else {
                ring().into()
            };
            list = list.push(step_line(
                mark,
                item.name.clone(),
                step == current && working,
            ));
        }
        // `align-self: flex-start; height: 44px; margin-top: 2px; padding:
        // 0 18px; border-radius: 22px; border: 1px solid
        // rgba(29,24,48,0.2); font-size: 13px; font-weight: 600`.
        let cancel = (working || turn.waiting()).then(|| {
            col![
                Spacer::fixed(2.0),
                pill("Cancel task", 44.0, Message::CancelTask(index)).id(("cancel", index)),
            ]
        });
        card = card.push(Divider::new().color(LINE)).push(
            list.push_maybe(cancel)
                .gap(12.0)
                .fill_width()
                // `padding: 4px 14px 14px`, and `margin-top: 10px` on the
                // first line.
                .padding_ltrb(14.0, 14.0, 14.0, 14.0),
        );
    }
    card.fill_width()
        .padding(1.0)
        .radius(RADIUS_STEPS)
        .background(WHITE)
        .border(1.0, LINE)
}

/// The answer's running text: `font-size: 16px; line-height: 1.6`.
fn paragraph(text: String) -> Text {
    jakarta(text, 16.0).line_height_dp(25.6).fill_width()
}

/// A line of an answer with `inline code` in it. There is no monospace
/// face among the app's fonts, so code is the same face, a weight
/// heavier.
fn line(text: &str) -> Text {
    if !text.contains('`') {
        return paragraph(text.to_owned());
    }
    let spans = format::inline(text).into_iter().map(|run| match run {
        Run::Plain(text) => Span::new(text),
        Run::Code(code) => Span::new(code).semibold(),
    });
    jakarta_rich(spans, 16.0).line_height_dp(25.6).fill_width()
}

/// A list item: its mark, and its text hanging beside it.
fn item(mark: String, text: &str) -> Container<Message> {
    row![
        jakarta(mark, 16.0).line_height_dp(25.6).color(MUTED),
        line(text)
    ]
    .gap(10.0)
    .cross_align(CrossAlign::Start)
    .fill_width()
    .merge_semantics()
}

/// The answer. A scripted one (and the design's) is one paragraph, laid
/// out exactly as before; one from the server may have paragraphs, list
/// items and code (`server/API.md`), one under the other.
fn reply(index: usize, text: String) -> Element<Message> {
    if format::plain(&text) {
        return paragraph(text)
            .live(Live::Polite)
            .id(("reply", index))
            .into();
    }
    let mut column = Column::new();
    let mut list = false;
    for (place, block) in format::blocks(&text).into_iter().enumerate() {
        let in_list = matches!(block, Block::Bullet(_) | Block::Numbered(..));
        // Items of one list sit closer together than paragraphs do.
        if place > 0 {
            column = column.push(Spacer::fixed(if list && in_list { 4.0 } else { 12.0 }));
        }
        list = in_list;
        column = match block {
            Block::Paragraph(text) => column.push(line(&text)),
            Block::Bullet(text) => column.push(item("\u{2022}".to_owned(), &text)),
            Block::Numbered(number, text) => column.push(item(number, &text)),
            // On a tinted card, like the composer's "+": `background:
            // rgba(29,24,48,0.05)`.
            Block::Code(code) => column.push(
                Container::new(jakarta(code, 13.0).line_height_dp(20.0).fill_width())
                    .fill_width()
                    .padding(12.0)
                    .radius(RADIUS_TILE)
                    .background(INK_05),
            ),
        };
    }
    column
        .fill_width()
        .live(Live::Polite)
        .id(("reply", index))
        .into()
}

/// Under a message the server did not answer: why, and "Retry", which
/// sends it again. What was typed stays where it is, in the bubble above.
fn failed(index: usize, error: String) -> Container<Message> {
    row![
        jakarta(error, 13.0)
            .line_height_dp(19.0)
            .color(MUTED)
            .fill_width()
            .live(Live::Polite),
        pill("Retry", 48.0, Message::Retry(index))
            .label("Retry: send the message again")
            .id(("retry", index)),
    ]
    .gap(12.0)
    .cross_align(CrossAlign::Center)
    .fill_width()
    .id(("failed", index))
}

/// Beside an answer that is being read aloud: that it is, and "Stop".
fn speaking(index: usize) -> Container<Message> {
    row![
        icon(&icons::WAVEFORM, 17.0, ACCENT),
        jakarta("Speaking the answer", 13.0)
            .bold()
            .color(ACCENT)
            .fill_width(),
        pill("Stop", 48.0, Message::StopSpeaking)
            .label("Stop speaking")
            .id(("stop-speaking", index)),
    ]
    .gap(8.0)
    .cross_align(CrossAlign::Center)
    .fill_width()
}

/// One exchange: `gap: 22px` between what was sent and the answer, the
/// answer `display: flex; flex-direction: column; gap: 14px`.
fn exchange(app: &Atlas, index: usize, turn: &Turn) -> Container<Message> {
    let answer = Column::new()
        .push_maybe(turn.intro.clone().map(paragraph))
        .push(steps(app, index, turn))
        .push_maybe(
            (turn.phase == Phase::Failed)
                .then(|| turn.error.clone())
                .flatten()
                .map(|error| failed(index, error)),
        )
        .push_maybe((app.speaking() == Some(index)).then(|| speaking(index)))
        .push_maybe(
            (turn.phase == Phase::Done)
                .then(|| turn.reply.clone())
                .flatten()
                .map(|text| reply(index, text)),
        )
        .push_maybe(
            turn.model_used
                .filter(|_| turn.phase == Phase::Done)
                .map(|mode| {
                    // The picker said Auto: which model it took.
                    jakarta(format!("Answered by {}", mode.name()), 12.0)
                        .color(MUTED)
                        .fill_width()
                }),
        )
        .gap(14.0)
        .fill_width();
    col![sent(index, turn), answer]
        .gap(22.0)
        .fill_width()
        .id(("turn", index))
}

/// `padding: 52px 20px 20px; display: flex; flex-direction: column; gap:
/// 20px` on `#FBF8FC`.
pub fn view(app: &Atlas) -> Element<Message> {
    let screen = app.screen();
    let chat = app.conversation();
    // A chat of the server's that is not here yet, or could not be fetched.
    let fetching: Option<Container<Message>> = match chat.loaded() {
        Loaded::Full | Loaded::Summary => None,
        Loaded::Loading => Some(
            row![
                spinner(app.spin()),
                jakarta("Loading this chat\u{2026}", 14.0).color(MUTED),
            ]
            .gap(12.0)
            .cross_align(CrossAlign::Center)
            .merge_semantics()
            .live(Live::Polite)
            .id("loading"),
        ),
        Loaded::Failed(why) => Some(
            row![
                jakarta(why.clone(), 13.0)
                    .line_height_dp(19.0)
                    .color(MUTED)
                    .fill_width()
                    .live(Live::Polite),
                pill("Retry", 48.0, Message::Reload)
                    .label("Retry: load the chat again")
                    .id("reload"),
            ]
            .gap(12.0)
            .cross_align(CrossAlign::Center)
            .fill_width(),
        ),
    };
    // Under a chat the server holds: the way to delete it, asked twice.
    // (Not while a message is with the server: there is nothing settled
    // to delete yet.)
    let settled = *chat.loaded() == Loaded::Full && !chat.turns.is_empty() && !chat.busy();
    let delete = (chat.is_remote() && settled).then(|| {
        let label = if app.deleting() {
            "Deleting\u{2026}"
        } else if app.confirming_delete() {
            "Tap again to delete it for good"
        } else {
            "Delete this chat"
        };
        Container::new(jakarta(label, 13.0).semibold().color(MUTED))
            .center()
            .height(48.0)
            .padding_xy(18.0, 0.0)
            .radius(24.0)
            .on_press(Message::DeleteChat)
            .live(Live::Polite)
            .align_self(CrossAlign::Center)
            .id("delete-chat")
    });
    let turns = Column::new()
        .push_maybe(fetching)
        .extend(
            chat.turns
                .iter()
                .enumerate()
                .map(|(index, turn)| exchange(app, index, turn)),
        )
        .push_maybe(delete)
        .gap(22.0)
        .fill_width()
        .padding_ltrb(MARGIN, 0.0, MARGIN, 20.0);
    let body = col![
        // `margin: 0 -10px`: the bar runs from 10 dp to 10 dp.
        Container::new(top_bar(
            chat.title.clone(),
            Some(model_chip(app.mode(), 10.0))
        ))
        .fill_width()
        .padding_ltrb(10.0, TOP, 10.0, 20.0),
        ScrollView::new(turns).id(screen.scroll_id()),
        Column::new()
            .push_maybe(pending_attachment(app))
            .push(composer(app.draft(), "Reply"))
            .fill_width()
            .padding_ltrb(MARGIN, 0.0, MARGIN, 20.0),
    ]
    .fill();
    page(screen, None, body).into()
}
