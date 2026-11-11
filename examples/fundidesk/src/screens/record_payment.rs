//! Screen 11: record a payment. `design/html/11-record-payment.html`.
//!
//! The design draws 1040 px of content. Here the header and the save
//! button at the bottom are fixed and the form scrolls between them. The
//! amount and the transaction ID are static look-alikes of inputs: the
//! amount follows the quick-amount chips.

use morningstar::ui::prelude::*;

use crate::components::{
    Action, Field, bottom_bar, choice, choice_with_check, grid, hairline, icon_button, scaffold,
    stat_tile, wide_chip,
};
use crate::icons;
use crate::theme::{
    AMBER_DARK, AMBER_TINT, GREEN, GREEN_DARK, GREEN_TINT, HAIRLINE, INK, MUTED, PANEL, WHITE,
    grotesk, inter, inter_rich, label, meta, overline, radius, space,
};
use crate::{Nav, Screen};

/// This screen is built from the design.
pub const BUILT: bool = true;

/// The quick amounts under the amount field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Amount {
    /// "Full balance", 40,000: chosen in the design.
    #[default]
    FullBalance,
    /// "20,000".
    TwentyThousand,
    /// "10,000".
    TenThousand,
}

impl Amount {
    const ALL: [Self; 3] = [Self::FullBalance, Self::TwentyThousand, Self::TenThousand];

    /// The chip's label.
    const fn label(self) -> &'static str {
        match self {
            Self::FullBalance => "Full balance",
            Self::TwentyThousand => "20,000",
            Self::TenThousand => "10,000",
        }
    }

    /// The amount in shillings, as the field and the button show it.
    const fn figure(self) -> &'static str {
        match self {
            Self::FullBalance => "40,000",
            Self::TwentyThousand => "20,000",
            Self::TenThousand => "10,000",
        }
    }
}

/// How the customer paid.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Method {
    /// "Cash".
    Cash,
    /// "M-Pesa": chosen in the design.
    #[default]
    MPesa,
    /// "Airtel Money".
    AirtelMoney,
    /// "Tigo Pesa".
    TigoPesa,
    /// "Other".
    Other,
}

impl Method {
    const ALL: [Self; 5] = [
        Self::Cash,
        Self::MPesa,
        Self::AirtelMoney,
        Self::TigoPesa,
        Self::Other,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::Cash => "Cash",
            Self::MPesa => "M-Pesa",
            Self::AirtelMoney => "Airtel Money",
            Self::TigoPesa => "Tigo Pesa",
            Self::Other => "Other",
        }
    }
}

/// The amount and the method; the defaults are the design's.
#[derive(Debug)]
pub struct State {
    /// The quick amount chosen last.
    pub amount: Amount,
    /// The amount field's text. Choosing a quick amount fills it in;
    /// typing replaces it.
    pub amount_text: String,
    /// "Transaction ID", as typed.
    pub transaction: String,
    /// The chosen payment method.
    pub method: Method,
}

impl Default for State {
    fn default() -> Self {
        let amount = Amount::default();
        Self {
            amount,
            amount_text: amount.figure().to_owned(),
            transaction: String::new(),
            method: Method::default(),
        }
    }
}

/// What the payment screen can ask for.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// The back chevron: the repair.
    Back,
    /// A quick amount.
    Amount(Amount),
    /// The amount field changed.
    AmountText(String),
    /// The transaction ID changed.
    Transaction(String),
    /// A payment method.
    Method(Method),
    /// "Save payment": back to the repair.
    Save,
}

/// Header, scrolling form, fixed save button.
pub fn view(state: &State) -> Element<Message> {
    let header = top_bar(
        "Record payment",
        "FD-2026-00142 · Samsung Galaxy A14 · Amina Juma",
    );

    // `display: flex; flex-direction: column; gap: 20px; padding: 20px 20px 0`.
    let body = col![
        totals(),
        amount(state),
        method(state),
        transaction(state),
        history()
    ]
    .gap(20.0)
    .fill_width()
    .padding_ltrb(space::PAGE_MARGIN, 20.0, space::PAGE_MARGIN, 0.0);

    // `display: flex; flex-direction: column; gap: 10px`.
    let footer = col![
        Action::primary(format!("Save payment of TZS {}", state.amount_text))
            .on_press(Message::Save),
        Spacer::fixed(10.0 - NOTE_LIFT),
        // `text-align: center; font-size: 12px; line-height: 18px`.
        inter(
            "Saved payments cannot be edited. Mistakes are corrected with a reversal.",
            12.0,
        )
        .line_height_dp(18.0)
        .color(MUTED)
        .centered()
        .fill_width(),
        Spacer::fixed(NOTE_LIFT),
    ]
    .fill_width();

    scaffold(
        "record-payment",
        WHITE,
        Some(header.into()),
        body,
        Some(bottom_bar(footer).into()),
    )
    .into()
}

/// How far the note under the button is raised: the browser rounds the
/// 1.75 dp of leading above a 12/18 line down to whole device pixels.
const NOTE_LIFT: f32 = 0.5;

/// The header: `display: flex; align-items: center; gap: 8px; padding: 36px
/// 20px 12px 10px; border-bottom: 1px solid #E3E8E4` around a 44 dp back
/// chevron and the title in Space Grotesk 20/26 over a 12 dp line (`gap:
/// 2px`).
///
/// This is `TopBar` with the title where the browser draws it: 0.5 dp
/// below the top of the row. Centring the 42.5 dp of text in the 44 dp row
/// puts it at 0.75 dp, which lands a device pixel lower.
fn top_bar(title: &'static str, subtitle: &'static str) -> Container<Message> {
    col![
        row![
            icon_button(icons::CHEVRON_LEFT, 24.0, INK).on_press(Message::Back),
            col![
                Spacer::fixed(0.5),
                grotesk(title, 20.0).line_height_dp(26.0),
                Spacer::fixed(2.25),
                meta(subtitle),
            ]
            .fill_width(),
        ]
        .gap(8.0)
        .fill_width()
        .padding_ltrb(10.0, 36.0, space::PAGE_MARGIN, 12.0)
        .safe_area(SafeEdges::TOP),
        hairline(),
    ]
    .fill_width()
    .background(WHITE)
}

/// Total, paid and balance: `grid-template-columns: repeat(3, minmax(0,
/// 1fr)); gap: 8px`.
fn totals() -> Container<Message> {
    let tile = |caption: &'static str, figure: &'static str, color, background| {
        stat_tile(
            inter(caption, 12.0).color(color),
            grotesk(figure, 16.0).color(color),
            background,
        )
    };
    row![
        // The caption is `#5B6660` and the figure the ink colour.
        stat_tile(
            inter("Total", 12.0).color(MUTED),
            grotesk("60,000", 16.0),
            PANEL,
        ),
        tile("Paid", "20,000", GREEN_DARK, GREEN_TINT),
        tile("Balance", "40,000", AMBER_DARK, AMBER_TINT),
    ]
    .gap(8.0)
    .fill_width()
}

/// "Amount received": the amount and the quick amounts, `gap: 10px`.
fn amount(state: &State) -> Container<Message> {
    let chips = Amount::ALL.map(|amount| {
        // A quick amount is chosen while the field holds its figure.
        wide_chip(amount.label(), amount.figure() == state.amount_text)
            .on_press(Message::Amount(amount))
            .id(("amount", amount as usize))
    });
    col![
        label("Amount received"),
        amount_field(&state.amount_text),
        // `grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 8px`.
        Row::new().gap(8.0).fill_width().extend(chips),
    ]
    .gap(10.0)
    .fill_width()
}

/// `display: flex; align-items: center; gap: 10px; height: 64px;
/// box-sizing: border-box; border: 1.5px solid #1E8416; border-radius:
/// 14px; padding: 0 16px` with "TZS" in `font-size: 15px; font-weight: 600;
/// color: #5B6660` before an input in `font: 700 28px 'Space Grotesk'`.
///
/// `Field::prefix` is the phone field's `+255` (medium, ink, a vertical
/// hairline after it, `gap: 12px`), which this is not.
///
/// The design draws this field focused (the green border); here it has that
/// border always and is a real input: digits and the thousands comma, on
/// the number keyboard.
fn amount_field(figure: &str) -> Container<Message> {
    row![
        inter("TZS", 15.0).semibold().color(MUTED),
        // A bare `<input>` keeps the browser's own 2 px of padding.
        Container::new(
            TextInput::new(figure)
                .style(grotesk("", 28.0))
                .caret_color(GREEN)
                .keyboard(KeyboardType::Number)
                .max_length(11)
                .no_autocorrect()
                .on_input(Message::AmountText)
                .id("payment-amount")
                .fill_width()
        )
        .fill_width()
        .padding_ltrb(2.0, 0.0, 2.0, 0.0),
    ]
    .gap(10.0)
    .cross_align(CrossAlign::Center)
    .fill_width()
    .height(64.0)
    // The padding starts inside the border.
    .padding_xy(16.0 + 1.5, 0.0)
    .radius(radius::BUTTON)
    .border(1.5, GREEN)
    .input_frame()
}

/// "Payment method": `gap: 10px` over a two-column grid of `height: 52px`
/// choices, the chosen one with a tick.
fn method(state: &State) -> Container<Message> {
    let choices: Vec<Element<Message>> = Method::ALL
        .map(|method| {
            let frame = if method == state.method {
                choice_with_check(method.label())
            } else {
                choice(method.label(), false)
            };
            frame
                .height(52.0)
                .on_press(Message::Method(method))
                .id(("method", method as usize))
                .into()
        })
        .into();
    col![label("Payment method"), grid(2, 8.0, choices)]
        .gap(10.0)
        .fill_width()
}

/// "Transaction ID (optional)" over its field.
fn transaction(state: &State) -> Container<Message> {
    // `<span style="color: #5B6660; font-weight: 400">` inside the label.
    let name = inter_rich(
        [
            span("Transaction ID ").medium(),
            span("(optional)").color(MUTED),
        ],
        13.0,
    );
    // `gap: 8px`, as `labelled`, whose label is a plain string.
    col![
        name,
        Field::new(&state.transaction)
            .hint("From the mobile money SMS")
            .input(|input| {
                input
                    .capitalization(Capitalization::Characters)
                    .no_autocorrect()
            })
            .on_input(Message::Transaction)
            .id("payment-transaction"),
    ]
    .gap(space::LABEL_GAP)
    .fill_width()
}

/// "Payments so far": the deposit, `gap: 10px` under the section label.
fn history() -> Container<Message> {
    col![
        overline("Payments so far"),
        // `display: flex; align-items: center; justify-content:
        // space-between; border: 1px solid #E3E8E4; border-radius: 14px;
        // padding: 12px 14px`.
        row![
            col![
                inter("Deposit · M-Pesa", 14.0).semibold(),
                meta("9 Nov, 11:20 · Recorded by Baraka"),
            ]
            .gap(2.0),
            inter("TZS 20,000", 14.0).semibold(),
        ]
        .space_between()
        .cross_align(CrossAlign::Center)
        .fill_width()
        .padding_xy(15.0, 13.0)
        .radius(radius::BUTTON)
        .border(1.0, HAIRLINE),
    ]
    .gap(10.0)
    .fill_width()
}

/// Follows the design's links.
pub fn update(state: &mut State, message: Message) -> Nav {
    match message {
        Message::Amount(amount) => {
            state.amount = amount;
            state.amount_text = amount.figure().to_owned();
            Nav::Stay
        }
        Message::AmountText(text) => {
            // Digits and commas only: what an amount is written with.
            state.amount_text = text
                .chars()
                .filter(|character| character.is_ascii_digit() || *character == ',')
                .collect();
            Nav::Stay
        }
        Message::Transaction(transaction) => {
            state.transaction = transaction;
            Nav::Stay
        }
        Message::Method(method) => {
            state.method = method;
            Nav::Stay
        }
        Message::Back | Message::Save => Nav::To(Screen::RepairDetails),
    }
}
