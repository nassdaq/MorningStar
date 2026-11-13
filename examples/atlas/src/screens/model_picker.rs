//! Screen 05: the model picker, a sheet over a dimmed screen.
//! `design/html/05-model-picker.html`.
//!
//! In the app the sheet rises over whatever screen the model chip was
//! tapped on (home, a chat, live voice), with the scrim fading in. The
//! design's page draws it over an abbreviated home of its own (the orb and
//! the question only, 4 dp lower than on home, under onboarding's wash);
//! [`backdrop`] is that, for the comparison with the design's picture.

use std::time::Duration;

use morningstar::ui::prelude::*;

use crate::components::{chevron, orb, page, radio, two_lines, wash_hero};
use crate::theme::{
    GRABBER, LINE, RADIUS_ROW, RADIUS_SHEET, SCRIM, SHEET_SHADOW, TINT_08, WHITE, jakarta,
};
use crate::{Message, Mode, Screen};

const RISE: Duration = Duration::from_millis(260);

/// The page under the design's sheet: `padding: 52px 20px 0`, and inside
/// it `display: flex; flex-direction: column; align-items: center; gap:
/// 34px; padding-top: 150px`.
pub fn backdrop() -> Element<Message> {
    let content = col![
        orb(96.0),
        // `font-size: 28px; font-weight: 700; letter-spacing: -0.02em`.
        jakarta("What can I help with?", 28.0)
            .bold()
            .letter_spacing(-0.56),
    ]
    .gap(34.0)
    .cross_align(CrossAlign::Center)
    .fill_width()
    .padding_ltrb(20.0, 202.0, 20.0, 0.0);
    page(Screen::PickerBackdrop, Some(wash_hero()), content).into()
}

/// `position: absolute; left: 0; top: 0; right: 0; bottom: 0; background:
/// rgba(29,24,48,0.42)`: a tap on it closes the picker.
pub fn scrim() -> Container<Message> {
    Container::empty()
        .fill()
        .background(SCRIM)
        .on_press(Message::ClosePicker)
        .pressed_background(SCRIM)
        .label("Close model picker")
        .id("scrim")
        .transition(Transition::fade().duration(RISE))
}

/// A model: a `button` with `min-height: 64px; display: flex; align-items:
/// center; gap: 14px; padding: 0 14px; border-radius: 16px`, on
/// `rgba(160,40,122,0.08)` when chosen.
pub fn mode_row(mode: Mode, chosen: bool) -> Container<Message> {
    row![radio(chosen), two_lines(mode.name(), mode.detail())]
        .gap(14.0)
        .cross_align(CrossAlign::Center)
        .fill_width()
        .min_height(64.0)
        .padding_xy(14.0, 0.0)
        .radius(RADIUS_ROW)
        .background(if chosen {
            TINT_08
        } else {
            with_alpha(WHITE, 0.0)
        })
        .on_press(Message::SelectMode(mode))
        .role(Role::Radio)
        .checked(chosen)
        .id(("mode", mode))
}

/// The sheet:
///
/// ```css
/// position: absolute; left: 0; right: 0; bottom: 0; display: flex; flex-direction: column;
/// gap: 4px; padding: 10px 16px 28px; border-radius: 28px 28px 0 0; background: #FFFFFF;
/// box-shadow: 0 -10px 40px rgba(29,24,48,0.18);
/// ```
pub fn sheet(chosen: Mode) -> Container<Message> {
    let content = Column::new()
        // `align-self: center; width: 36px; height: 4px; border-radius:
        // 2px; background: rgba(29,24,48,0.18)`.
        .push(
            Container::empty()
                .size(36.0, 4.0)
                .radius(2.0)
                .background(GRABBER)
                .align_self(CrossAlign::Center),
        )
        // `margin: 14px 14px 8px; font-size: 20px; font-weight: 700;
        // letter-spacing: -0.01em`.
        .push(
            Container::new(jakarta("Model", 20.0).bold().letter_spacing(-0.2).heading())
                .padding_ltrb(14.0, 14.0, 14.0, 8.0),
        )
        .extend(Mode::ALL.map(|mode| mode_row(mode, mode == chosen)))
        // `height: 1px; margin: 8px 14px; background: rgba(29,24,48,0.08)`.
        .push(
            Container::new(Container::empty().fill_width().height(1.0).background(LINE))
                .fill_width()
                .padding_xy(14.0, 8.0),
        )
        // `min-height: 48px; display: flex; align-items: center;
        // justify-content: space-between; padding: 0 14px; font-size: 14px;
        // font-weight: 600`.
        .push(
            row![
                jakarta("All models and providers", 14.0).semibold(),
                chevron()
            ]
            .space_between()
            .cross_align(CrossAlign::Center)
            .fill_width()
            .min_height(48.0)
            .padding_xy(14.0, 0.0)
            .radius(RADIUS_ROW)
            .on_press(Message::AllModels)
            .id("all-models"),
        )
        .gap(4.0)
        .fill_width()
        .padding_ltrb(16.0, 10.0, 16.0, 28.0);
    Container::new(content)
        .fill_width()
        .radius(RADIUS_SHEET)
        .radii(RADIUS_SHEET, RADIUS_SHEET, 0.0, 0.0)
        .background(WHITE)
        .shadow(SHEET_SHADOW)
        .block_touches()
        .safe_area(SafeEdges::BOTTOM)
        .place(Alignment::BOTTOM_CENTER)
        .id("picker")
        .transition(Transition::slide(0.0, 1.0).duration(RISE))
}
