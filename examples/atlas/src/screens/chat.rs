//! Screens 04 and 10: a conversation. `design/html/04-chat.html` (and
//! `10-chat-simplified.html`, the same page whose back arrow leads to 09).
//!
//! The title and the model chip, the exchanges, and the composer. Each
//! exchange is what the user sent (with its attachment), the assistant's
//! first line, a card that says which step it is on, and the answer once
//! the steps are done. The card unfolds into the list of steps, with
//! "Cancel task" while there is something to cancel.

use morningstar::ui::prelude::*;

use crate::components::{
    MARGIN, TOP, attachment_chip, composer, model_chip, page, spinner, top_bar,
};
use crate::icons::{self, icon};
use crate::screens::home::pending_attachment;
use crate::theme::{
    ACCENT, LINE, LINE_20, MUTED, QUIET, RADIUS_CARD, RADIUS_STEPS, RADIUS_TAIL, TINT_09, WHITE,
    jakarta,
};
use crate::{Atlas, Message, Phase, Turn};

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
        Phase::Cancelled => (
            Container::empty()
                .size(18.0, 18.0)
                .radius(9.0)
                .border_dashed(1.5, QUIET)
                .into(),
            "Cancelled".into(),
            String::new(),
        ),
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
    if turn.open && total > 0 {
        let current = match turn.phase {
            Phase::Working(step) => step,
            Phase::Done => total,
            Phase::Thinking | Phase::Cancelled => 0,
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
                // `width: 18px; height: 18px; border-radius: 50%; border:
                // 1.5px dashed #8E88A3`.
                Container::empty()
                    .size(18.0, 18.0)
                    .radius(9.0)
                    .border_dashed(1.5, QUIET)
                    .into()
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
        let cancel = working.then(|| {
            col![
                Spacer::fixed(2.0),
                Container::new(jakarta("Cancel task", 13.0).semibold())
                    .center()
                    .height(44.0)
                    .padding_xy(18.0, 0.0)
                    .radius(22.0)
                    .border(1.0, LINE_20)
                    .on_press(Message::CancelTask(index))
                    .id(("cancel", index)),
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

/// One exchange: `gap: 22px` between what was sent and the answer, the
/// answer `display: flex; flex-direction: column; gap: 14px`.
fn exchange(app: &Atlas, index: usize, turn: &Turn) -> Container<Message> {
    // `font-size: 16px; line-height: 1.6`.
    let paragraph = |text: String| jakarta(text, 16.0).line_height_dp(25.6).fill_width();
    let answer = Column::new()
        .push_maybe(turn.intro.clone().map(paragraph))
        .push(steps(app, index, turn))
        .push_maybe(
            (turn.phase == Phase::Done)
                .then(|| turn.reply.clone())
                .flatten()
                .map(|reply| paragraph(reply).live(Live::Polite).id(("reply", index))),
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
    let turns = Column::new()
        .extend(
            chat.turns
                .iter()
                .enumerate()
                .map(|(index, turn)| exchange(app, index, turn)),
        )
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
