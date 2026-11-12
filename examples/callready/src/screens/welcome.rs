//! Screen 01-welcome. `design/html/01-welcome.html`.
//!
//! A collage: the headline, "real CALLS" as five turned cut-out letters
//! with a heart and a waveform, six things a caller says scattered down the
//! page with doodles between them, and the "Let's go" button, which opens
//! Home.
//!
//! The design places almost everything with `position: absolute` inside
//! two relatively positioned blocks. Each block is a `stack!` here and
//! each piece is pinned to the corner the CSS names and moved by the CSS's
//! own `left` / `right` / `top`.

use morningstar::ui::prelude::*;

use crate::Screen;
use crate::components::{plate, primary_button};
use crate::theme::{
    BLACK, PURPLE, RED, SILVER, TEAL, WHITE, YELLOW, abril_fatface, archivo_black, caveat, outfit,
};

/// What the screen can ask for.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// "Let's go".
    Start,
}

// ---------------------------------------------------------------- drawings

/// The heart beside "real": `width="34" height="34"`, `stroke="currentColor"`.
const HEART: VectorIcon = VectorIcon::svg(
    r#"<svg viewBox="0 0 34 34" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M9 7c2 0 5 4 5 4s4-5 9-3 3 8-1 12l-8 8-8-9C2 13 4 7 9 7z"/></svg>"#,
);

/// The star over the italic quote: `width="22" height="22"`.
const STAR: VectorIcon = VectorIcon::svg(
    r#"<svg viewBox="0 0 22 22" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M11 2l2.6 6 6.4.6-4.8 4.3 1.4 6.3L11 16l-5.6 3.2 1.4-6.3L2 8.6 8.4 8z"/></svg>"#,
);

/// The three strokes in the top right corner: `width="60" height="60"`.
const SPARK: VectorIcon = VectorIcon::svg(
    r#"<svg viewBox="0 0 60 60" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round"><path d="M14 10l6 12M30 4l-2 18M46 16l-12 10"/></svg>"#,
);

/// The waveform left of the letters: `width="120" height="44"
/// viewBox="0 0 120 44" fill="none" stroke="#FFFFFF" stroke-width="1.6"
/// stroke-linecap="round"`. Not square, so a picture and not an icon.
const WAVEFORM: VectorPicture = VectorPicture::new(
    120.0,
    44.0,
    &[Layer::path(
        "M2 22h4l2-8 3 18 3-26 3 30 3-24 3 20 3-14 3 10 3-18 3 22 3-26 3 30 3-22 3 16 3-10 3 6 \
         3-14 3 18 3-12 3 8 3-4 3 2h10",
    )
    .stroke(WHITE, 1.6)
    .cap(LineCap::Round)],
);

/// The line under the first quote: `width="140" height="10"
/// viewBox="0 0 140 10" fill="none" stroke="#FFFFFF" stroke-width="1.5"
/// stroke-linecap="round"`.
const UNDERLINE: VectorPicture = VectorPicture::new(
    140.0,
    10.0,
    &[Layer::path("M2 6c30-5 80-5 136-1")
        .stroke(WHITE, 1.5)
        .cap(LineCap::Round)],
);

/// The ring around "twice": `border: 1.5px solid #FFFFFF; border-radius:
/// 50%; padding: 2px 8px` on an inline span: the word's 40 dp by the
/// font's 21.5 dp content area, plus padding and border, is 58.93 by 28.5 dp.
/// An ellipse, which a box's radius cannot draw.
const RING: VectorPicture = VectorPicture::new(
    RING_WIDTH,
    RING_HEIGHT,
    &[Layer::ellipse(
        RING_WIDTH / 2.0,
        RING_HEIGHT / 2.0,
        (RING_WIDTH - 1.5) / 2.0,
        (RING_HEIGHT - 1.5) / 2.0,
    )
    .stroke(WHITE, 1.5)],
);
const RING_WIDTH: f32 = 58.93;
const RING_HEIGHT: f32 = 28.5;
/// How far the ring starts above its line: the line's 2 dp of half-leading
/// less the span's 2 dp of padding and 1.5 dp of border.
const RING_RISE: f32 = 1.5;

// -------------------------------------------------------------------- view

/// The page's side padding.
const SIDE: f32 = 28.0;

/// `padding: 84px 28px 32px; display: flex; flex-direction: column;
/// background: #0E0E0E`, with the spark pinned to the top right corner
/// (`position: absolute; right: 18px; top: 22px`).
///
/// The collage scrolls on a phone shorter than the design's; the button
/// stays at the bottom.
pub fn view() -> Element<Message> {
    let collage = stack![
        col![
            // `font-size: 44px; line-height: 1.08; font-weight: 700;
            // letter-spacing: -0.5px`.
            outfit("Get job-ready with", 44.0)
                .bold()
                .line_height_dp(44.0 * 1.08)
                .letter_spacing(-0.5)
                .fill_width(),
            // `margin-top: 4px`.
            Spacer::fixed(4.0),
            hero(),
            // `margin-top: 8px`.
            Spacer::fixed(8.0),
            quotes(),
        ]
        .fill_width()
        .padding_ltrb(SIDE, 84.0, SIDE, 12.0),
        Icon::vector(SPARK)
            .size(60.0)
            .color(WHITE)
            .place(Alignment::TOP_RIGHT)
            .offset(-18.0, 22.0),
    ]
    .fill_width();
    col![
        ScrollView::new(collage).id(Screen::Welcome.scroll_id()),
        Container::new(start_button())
            .fill_width()
            .padding_ltrb(SIDE, 0.0, SIDE, 32.0),
    ]
    .fill()
    .background(BLACK)
    .safe_area(SafeEdges::ALL)
    .id(Screen::Welcome.name())
    .into()
}

/// `position: relative; height: 150px`: "real", the heart, the five
/// letters and the waveform.
fn hero() -> Container<Message> {
    stack![
        // `left: 118px; top: 0; font-family: 'Caveat'; font-size: 46px;
        // color: #F6C445; transform: rotate(-8deg)`.
        caveat("real", 46.0)
            .color(YELLOW)
            .rotate(-8.0)
            .offset(118.0, 0.0),
        // `left: 196px; top: -6px`.
        Icon::vector(HEART)
            .size(34.0)
            .color(RED)
            .offset(196.0, -6.0),
        // `left: 70px; top: 54px; display: flex; align-items: flex-end;
        // gap: 2px`. The row is 280 dp wide and the block 334 at the
        // design's width, so that is 16 dp past the block's right edge;
        // pinned there, the letters stay on a narrower screen.
        row![
            cutout(
                archivo_black("C", 62.0),
                RED,
                (8.0, 6.0),
                Transform::rotate(-8.0)
            ),
            cutout(
                abril_fatface("A", 58.0),
                WHITE,
                (8.0, 4.0),
                Transform::rotate(5.0).and_translate(0.0, -8.0),
            ),
            cutout(
                archivo_black("L", 56.0),
                PURPLE,
                (7.0, 6.0),
                Transform::rotate(-3.0).and_translate(0.0, 6.0),
            ),
            cutout(
                abril_fatface("L", 62.0),
                YELLOW,
                (9.0, 4.0),
                Transform::rotate(7.0).and_translate(0.0, -12.0),
            ),
            cutout(
                archivo_black("S", 46.0),
                TEAL,
                (6.0, 6.0),
                Transform::rotate(-6.0)
            ),
        ]
        .gap(2.0)
        .cross_align(CrossAlign::End)
        .place(Alignment::TOP_RIGHT)
        .offset(LETTERS_OVERHANG, 54.0),
        // `left: -24px; top: 92px`.
        Picture::vector(WAVEFORM)
            .size(120.0, 44.0)
            .offset(-24.0, 92.0),
    ]
    .fill_width()
    .height(150.0)
}

/// How far the row of letters reaches past the right edge of its block:
/// `left: 70px` plus the row's 280.004 dp, less the block's 334 dp at the
/// design's width.
const LETTERS_OVERHANG: f32 = 16.004;

/// One cut-out letter: `display: inline-block; color: #0E0E0E;
/// line-height: 1` on a coloured plate that hugs it with the design's
/// `padding` (across, down), turned and moved by `transform`.
fn cutout(
    letter: Text,
    color: Color,
    padding: (f32, f32),
    transform: Transform,
) -> Container<Message> {
    plate(letter.line_height(1.0), padding.0, padding.1, color).transform(transform)
}

/// A caller's line: `font-size: 17px; line-height: 1.5; color: #C7C7CC`.
fn quote(content: &'static str) -> Text {
    outfit(content, 17.0)
        .line_height_dp(QUOTE_LINE)
        .color(SILVER)
}

/// `line-height: 1.5` of the quotes' 17 dp.
const QUOTE_LINE: f32 = 25.5;

/// `position: relative; flex: 1`: the six lines and their doodles, each
/// `position: absolute` at the `left` or `right` and `top` quoted. The
/// block is as tall as its last line reaches, so that it can scroll.
fn quotes() -> Container<Message> {
    let (charged, twice, bill) = charged_twice();
    stack![
        // `left: -6px; top: 0; width: 300px`.
        quote("\u{201c}I ordered this two weeks ago and it\u{2019}s still not here!\u{201d}")
            .width(300.0)
            .offset(-6.0, 0.0),
        // `left: 70px; top: 50px`.
        Picture::vector(UNDERLINE)
            .size(140.0, 10.0)
            .offset(70.0, 50.0),
        // `left: 30px; top: 78px; width: 300px`.
        quote("\u{201c}Can I speak to your manager, please?\u{201d}")
            .width(300.0)
            .offset(30.0, 78.0),
        // The paragraph's `<span style="font-family: 'Caveat'; font-size:
        // 30px; color: #FFFFFF; margin-left: 6px">!!</span>` does not fit
        // on the first line and starts the second: 6 dp in, one quote line
        // down, in a line of its own 45 dp (`line-height: 1.5` of 30).
        caveat("!!", 30.0)
            .line_height_dp(45.0)
            .offset(30.0 + 6.0, 78.0 + QUOTE_LINE),
        // `right: -4px; top: 132px; font-family: 'Abril Fatface';
        // font-size: 30px; letter-spacing: 1px; color: #FFFFFF`, with the
        // block's `line-height: 1.5`.
        abril_fatface("ANGRY CALLER", 30.0)
            .line_height_dp(45.0)
            .letter_spacing(1.0)
            .place(Alignment::TOP_RIGHT)
            .offset(4.0, 132.0),
        // `left: -6px; top: 186px; width: 330px`: one line of three
        // pieces, the ring starting 1.5 dp higher than the words, which it
        // overhangs.
        charged.offset(-6.0, 186.0),
        twice.offset(-6.0 + CHARGED_WIDTH, 186.0 - RING_RISE),
        bill.offset(-6.0 + CHARGED_WIDTH + RING_WIDTH, 186.0),
        // `right: -4px; top: 246px; width: 260px; font-style: italic;
        // text-align: right`.
        quote("\u{201c}Sorry, could you say that slower?\u{201d}")
            .italic()
            .align(TextAlign::End)
            .width(260.0)
            .place(Alignment::TOP_RIGHT)
            .offset(4.0, 246.0),
        // `left: 82px; top: 252px`: 230 dp from the block's right edge at
        // the design's width, and pinned there, since it belongs to the
        // right-aligned line beside it.
        Icon::vector(STAR)
            .size(22.0)
            .color(WHITE)
            .place(Alignment::TOP_RIGHT)
            .offset(-230.0, 252.0),
        // `left: -6px; top: 306px; font-family: 'Archivo Black';
        // font-size: 26px; letter-spacing: 0.5px; color: #FFFFFF`.
        archivo_black("REFUND", 26.0)
            .line_height_dp(39.0)
            .letter_spacing(0.5)
            .offset(-6.0, 306.0),
        // `left: 112px; top: 314px; width: 230px`.
        quote("\u{201c}I just want my money back.\u{201d}")
            .width(230.0)
            .offset(112.0, 314.0),
    ]
    .fill_width()
    .height(346.0)
}

/// "You charged me (twice) for the same bill.": one line of three pieces.
/// The ringed word is `<span style="border: 1.5px solid #FFFFFF;
/// border-radius: 50%; padding: 2px 8px">twice</span>`: an inline box 59 dp
/// wide that takes its width in the line but none of its height, so the
/// ring overhangs the 25.5 dp line above and below.
///
/// The pieces are placed one after another by their measured widths rather
/// than put in a row, so that the line may run into the page's margin on a
/// narrow screen instead of not fitting its block.
fn charged_twice() -> (Text, Container<Message>, Text) {
    (
        quote("\u{201c}You charged me ").width(CHARGED_WIDTH),
        stack![
            Picture::vector(RING).size(RING_WIDTH, RING_HEIGHT),
            quote("twice")
                .place(Alignment::TOP_CENTER)
                .offset(0.0, RING_RISE),
        ]
        .size(RING_WIDTH, RING_HEIGHT),
        quote(" for the same bill.\u{201d}"),
    )
}

/// The width Chrome measured for "“You charged me ", trailing space
/// included. The three pieces are placed by offsets, which a view cannot
/// compute from a text it has not measured (no inline boxes yet).
const CHARGED_WIDTH: f32 = 131.7;

/// `display: flex; align-items: center; justify-content: center; height:
/// 64px; border-radius: 22px; background: #F6C445; color: #0E0E0E;
/// font-size: 20px; font-weight: 700`.
fn start_button() -> Container<Message> {
    primary_button("Let\u{2019}s go", 20.0, 64.0, Message::Start).id("welcome-start")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_drawing_is_within_the_supported_subset() {
        for (name, icon) in [("heart", HEART), ("star", STAR), ("spark", SPARK)] {
            assert_eq!(icon.check(), Ok(()), "{name}");
        }
        for (name, picture) in [
            ("waveform", WAVEFORM),
            ("underline", UNDERLINE),
            ("ring", RING),
        ] {
            assert_eq!(picture.check(), Ok(()), "{name}");
        }
    }
}
