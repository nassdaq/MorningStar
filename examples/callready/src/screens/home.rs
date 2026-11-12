//! Screen 02: home. `design/html/02-home.html`.
//!
//! Today's practice call (a vinyl disc between two half-rings cut by the
//! screen's edges), the three scenario cards and the tab bar. Everything
//! above the tab bar scrolls on a phone shorter than the design's; on the
//! design's 844 dp nothing moves.
//!
//! Where it leads: the avatar and name open the profile, the bell the
//! notifications, the gear the scenario settings; the call button starts
//! today's call and each card its own scenario; the tab bar switches tabs.
//! The name, the bell's dot and the line under today's call come from the
//! [`Model`].

use morningstar::ui::prelude::*;

use crate::components::{MARGIN, TOP, avatar, tab_screen};
use crate::icons::{BELL, CHEVRON, GEAR, HANDSET};
use crate::model::Model;
use crate::script::Scenario;
use crate::theme::{BLACK, BLUE, GREY, MUTED, PURPLE, TEAL, WHITE, YELLOW, anton, outfit};
use crate::{Screen, Tab};

/// What the screen can ask for.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// The avatar, the name and the chevron: the profile.
    Profile,
    /// The bell: the notifications.
    Notifications,
    /// The round call button: today's call.
    StartCall,
    /// The gear beside "Scenarios": the scenario settings.
    ScenarioSettings,
    /// A scenario card, by position in [`Scenario::CARDS`].
    Scenario(usize),
    /// An item of the tab bar.
    Tab(Tab),
}

// ---------------------------------------------------------- illustrations

/// The vinyl disc: `<svg width="250" height="250" viewBox="0 0 280 280">`,
/// the design's own SVG, loaded (ADR 0027). An outer ring, the disc, a band
/// turned by -18 degrees and clipped to the disc, ten grooves, five black
/// rings, the cream label and the headset.
///
/// Public because it is also the app's launcher icon (`examples/icon.rs`).
pub static DISC: SvgAsset = SvgAsset::new(include_str!("../../assets/svg/disc.svg"));

/// `<svg width="64" height="220" viewBox="0 0 64 220" fill="none"
/// stroke="#5AA9E6">`: four rings about a centre outside the view box, so
/// that the box cuts them to arcs.
static RINGS_LEFT: SvgAsset = SvgAsset::new(include_str!("../../assets/svg/rings-left.svg"));
/// The same about a centre beyond the right edge, `stroke="#4CAF50"`.
static RINGS_RIGHT: SvgAsset = SvgAsset::new(include_str!("../../assets/svg/rings-right.svg"));

/// `width: 46px; height: 46px; background: repeating-linear-gradient(45deg,
/// #F6C445 0 4px, #0E0E0E 4px 7px)`: black, with ten yellow lines 4 wide
/// running down to the right, 7 apart, the first centred 2 from the
/// bottom-left corner (the recipe of `docs/building-screens.md`).
const STRIPES: VectorPicture = VectorPicture::new(
    46.0,
    46.0,
    &[
        Layer::rect(0.0, 0.0, 46.0, 46.0).fill(BLACK),
        Layer::path(
            "M-47.17 -4l54 54M-37.27 -4l54 54M-27.37 -4l54 54M-17.47 -4l54 54\
             M-7.57 -4l54 54M2.33 -4l54 54M12.22 -4l54 54M22.12 -4l54 54\
             M32.02 -4l54 54M41.92 -4l54 54",
        )
        .stroke(YELLOW, 4.0),
    ],
);

// ------------------------------------------------------------------- view

/// `padding: 52px 24px 20px; display: flex; flex-direction: column`, with
/// the tab bar at the bottom (`margin-top: auto`). The illustration has
/// `margin: 0 -24px`, so the page itself has no side padding and every
/// other block takes [`MARGIN`].
pub fn view(model: &Model) -> Element<Message> {
    let content = col![
        header(model),
        // `margin: 18px 0 0; font-size: 15px; color: #A1A1A6`.
        Spacer::fixed(18.0),
        today_caption(model),
        Spacer::fixed(8.0),
        illustration(),
        Spacer::fixed(8.0),
        todays_call(model),
        Spacer::fixed(24.0),
        scenarios_heading(),
        Spacer::fixed(6.0),
        scenarios(),
        // Room for the numerals, which hang up to 8 dp below their cards
        // and are turned.
        Spacer::fixed(16.0),
    ]
    .fill_width()
    .padding_ltrb(0.0, TOP, 0.0, 0.0);
    tab_screen(Screen::Home, content, Tab::Home, Message::Tab)
}

/// "Today's practice call", and beside it, once that is known, how calls
/// are played right now: "live · voice", "live · text", or "offline ·
/// practice script". The design has only the caption, and so has the app
/// while there is nothing to say.
fn today_caption(model: &Model) -> Container<Message> {
    let caption = outfit("Today\u{2019}s practice call", 15.0).color(MUTED);
    match model.link.caption() {
        None => Container::new(caption).padding_xy(MARGIN, 0.0),
        Some(note) => row![
            caption,
            Spacer::new(),
            outfit(note, 13.0).color(if model.link.is_online() { TEAL } else { GREY }),
        ]
        .cross_align(CrossAlign::Center)
        .fill_width()
        .padding_xy(MARGIN, 0.0),
    }
}

/// `display: flex; align-items: center; justify-content: space-between`:
/// the profile link and the bell.
fn header(model: &Model) -> Container<Message> {
    row![profile(model), notifications(model.bell)]
        .space_between()
        .cross_align(CrossAlign::Center)
        .fill_width()
        .padding_xy(MARGIN, 0.0)
}

/// `display: flex; align-items: center; gap: 10px; min-height: 44px`: the
/// avatar, the name and a chevron.
fn profile(model: &Model) -> Container<Message> {
    row![
        // `width: 42px; height: 42px; border-radius: 50%; border: 2px solid
        // #F6C445; background: #2A2A2E` (content-box: 46 dp outside);
        // `font-weight: 700; font-size: 15px`.
        avatar(model.initials(), 46.0, 15.0),
        // `font-size: 22px; font-weight: 600`.
        outfit(greeting(model), 22.0).semibold(),
        Icon::vector(CHEVRON).size(16.0).color(WHITE),
    ]
    .gap(10.0)
    .cross_align(CrossAlign::Center)
    .on_press(Message::Profile)
    .pressed_background(BLACK)
    .label(format!("Profile, {}", model.first_name()))
    .id("home-profile")
}

/// The learner's first name, cut to what fits beside the bell on a narrow
/// phone.
fn greeting(model: &Model) -> String {
    model.first_name().chars().take(12).collect()
}

/// `position: relative; width: 44px; height: 44px` around the 24 dp bell,
/// with a dot: `position: absolute; top: 8px; right: 9px; width: 9px;
/// height: 9px; border-radius: 50%; background: #F6C445`. The dot shows
/// while something is unread.
fn notifications(unread: bool) -> Container<Message> {
    Stack::new()
        .push(
            Icon::vector(BELL)
                .size(24.0)
                .color(WHITE)
                .place(Alignment::CENTER),
        )
        .push_maybe(unread.then(|| {
            Container::<Message>::empty()
                .size(9.0, 9.0)
                .radius(4.5)
                .background(YELLOW)
                .place(Alignment::TOP_RIGHT)
                .offset(-9.0, 8.0)
        }))
        .size(44.0, 44.0)
        .on_press(Message::Notifications)
        .pressed_background(BLACK)
        // The dot is all that says something is unread.
        .label(if unread {
            "Notifications, unread"
        } else {
            "Notifications"
        })
        .id("home-notifications")
}

/// `position: relative; height: 250px; margin: 8px -24px 0`: the disc
/// (`left: 70px; top: 0`) between the rings (`left: 0; top: 6px` and
/// `right: 0; top: 6px`), across the whole width of the screen.
fn illustration() -> Container<Message> {
    stack![
        Picture::svg(RINGS_LEFT.get())
            .size(64.0, 220.0)
            .place(Alignment::TOP_LEFT)
            .offset(0.0, 6.0),
        Picture::svg(RINGS_RIGHT.get())
            .size(64.0, 220.0)
            .place(Alignment::TOP_RIGHT)
            .offset(0.0, 6.0),
        // `left: 70px` of the design's 390: the middle, at any width.
        Picture::svg(DISC.get())
            .size(250.0, 250.0)
            .place(Alignment::TOP_CENTER),
    ]
    .fill_width()
    .height(250.0)
}

/// `display: flex; align-items: center; justify-content: center; gap:
/// 18px`: the call's title and details, centred on each other, and the
/// round call button.
fn todays_call(model: &Model) -> Container<Message> {
    row![
        col![
            // `font-size: 30px; font-weight: 600; line-height: 1.1`.
            outfit(Scenario::MissingDelivery.title(), 30.0)
                .semibold()
                .line_height_dp(33.0)
                .heading(),
            // `font-size: 16px; color: #A1A1A6; margin-top: 4px`.
            outfit(model.settings.summary(), 16.0).color(MUTED),
        ]
        .gap(4.0)
        .cross_align(CrossAlign::Center),
        // `width: 60px; height: 60px; border-radius: 50%; background:
        // #F6C445` around a 26 dp handset.
        Container::new(Icon::vector(HANDSET).size(26.0).color(BLACK))
            .size(60.0, 60.0)
            .center()
            .radius(30.0)
            .background(YELLOW)
            .on_press(Message::StartCall)
            .label("Start call")
            .id("home-start-call"),
    ]
    .gap(18.0)
    .center()
    .fill_width()
    .padding_xy(MARGIN, 0.0)
}

/// `display: flex; align-items: center; justify-content: space-between`:
/// "Scenarios" (`font-size: 26px; font-weight: 700`) and the 44 dp gear
/// button.
fn scenarios_heading() -> Container<Message> {
    row![
        outfit("Scenarios", 26.0).bold().heading(),
        Container::new(Icon::vector(GEAR).size(24.0).color(WHITE).offset(-1.0, 0.0))
            .size(44.0, 44.0)
            .center()
            .on_press(Message::ScenarioSettings)
            .pressed_background(BLACK)
            .label("Scenario settings")
            .id("home-scenario-settings"),
    ]
    .space_between()
    .cross_align(CrossAlign::Center)
    .fill_width()
    .padding_xy(MARGIN, 0.0)
}

/// `display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap:
/// 12px; padding: 0 2px`.
fn scenarios() -> Container<Message> {
    row![
        // 1: two purple outlines, a thin purple bar and a purple "1" with a
        // white shadow.
        card(
            0,
            Outline::new(PURPLE, 2.0, -2.0, (0.0, 0.0)),
            // `inset: 2px -2px -2px 2px; .. rotate(1.2deg); opacity: 0.8`.
            Outline::new(with_alpha(PURPLE, 0.8), 1.5, 1.2, (2.0, 2.0)),
            // `right: 14px; bottom: 8px; width: 12px; height: 70px;
            // background: #9B5DE5; transform: rotate(4deg)`.
            Container::empty()
                .size(12.0, 70.0)
                .background(PURPLE)
                .rotate(4.0)
                .place(Alignment::BOTTOM_RIGHT)
                .offset(-14.0, -8.0)
                .into(),
            // `right: 18px; bottom: -6px; font-size: 80px; line-height: 1;
            // color: #9B5DE5; text-shadow: 3px 3px 0 #FFFFFF; transform:
            // rotate(3deg)`.
            numeral("1", 80.0, PURPLE)
                .shadow(3.0, 3.0, WHITE)
                .rotate(3.0)
                .place(Alignment::BOTTOM_RIGHT)
                .offset(-18.0, 6.0),
        ),
        // 2: a blue and a white outline, a blue block and a black "2"
        // outlined in white.
        card(
            1,
            Outline::new(BLUE, 2.0, 0.8, (0.0, 0.0)),
            // `inset: 3px 1px -3px -1px; border: 1.5px solid #FFFFFF; ..
            // rotate(-1deg); opacity: 0.55`.
            Outline::new(with_alpha(WHITE, 0.55), 1.5, -1.0, (-1.0, 3.0)),
            // `right: 10px; bottom: 6px; width: 40px; height: 66px;
            // background: #5AA9E6; transform: rotate(-3deg)`.
            Container::empty()
                .size(40.0, 66.0)
                .background(BLUE)
                .rotate(-3.0)
                .place(Alignment::BOTTOM_RIGHT)
                .offset(-10.0, -6.0)
                .into(),
            // `right: 18px; bottom: 2px; font-size: 70px; line-height: 1;
            // color: #0E0E0E; -webkit-text-stroke: 1.5px #FFFFFF;
            // transform: rotate(-3deg)`.
            numeral("2", 70.0, BLACK)
                .outline(1.5, WHITE)
                .rotate(-3.0)
                .place(Alignment::BOTTOM_RIGHT)
                .offset(-18.0, -2.0),
        ),
        // 3: two yellow outlines, a striped square and a yellow "3" with a
        // black shadow.
        card(
            2,
            Outline::new(YELLOW, 2.0, 2.0, (0.0, 0.0)),
            // `inset: -2px 2px 2px -2px; .. rotate(-0.8deg); opacity: 0.7`.
            Outline::new(with_alpha(YELLOW, 0.7), 1.5, -0.8, (-2.0, -2.0)),
            // `right: 22px; bottom: 4px; width: 46px; height: 46px;
            // background: repeating-linear-gradient(..); transform:
            // rotate(-6deg)`.
            Picture::vector(STRIPES)
                .size(46.0, 46.0)
                .rotate(-6.0)
                .place(Alignment::BOTTOM_RIGHT)
                .offset(-22.0, -4.0)
                .into(),
            // `right: 12px; bottom: -8px; font-size: 82px; line-height: 1;
            // color: #F6C445; text-shadow: -2px 2px 0 #0E0E0E; transform:
            // rotate(-5deg)`.
            numeral("3", 82.0, YELLOW)
                .shadow(-2.0, 2.0, BLACK)
                .rotate(-5.0)
                .place(Alignment::BOTTOM_RIGHT)
                .offset(-12.0, 8.0),
        ),
    ]
    .gap(12.0)
    .fill_width()
    .padding_xy(MARGIN + 2.0, 0.0)
}

/// One of a scenario card's two tilted frames: `position: absolute; inset: ..;
/// border: {width}px solid {color}; border-radius: 4px; transform:
/// rotate({degrees}deg)`. Every `inset` in the design moves the frame
/// without resizing it; `shift` is that movement.
struct Outline {
    color: Color,
    width: f32,
    degrees: f32,
    shift: (f32, f32),
}

impl Outline {
    const fn new(color: Color, width: f32, degrees: f32, shift: (f32, f32)) -> Self {
        Self {
            color,
            width,
            degrees,
            shift,
        }
    }

    fn view(&self) -> Container<Message> {
        Container::empty()
            .fill()
            .border(self.width, self.color)
            .radius(4.0)
            .rotate(self.degrees)
            .offset(self.shift.0, self.shift.1)
    }
}

/// `font-family: 'Anton'; font-size: {size}px; line-height: 1`.
fn numeral(digit: &'static str, size: f32, color: Color) -> Text {
    anton(digit, size).line_height_dp(size).color(color)
}

/// A scenario card: `position: relative; display: block; height: 138px`
/// holding two frames, the label (`left: 12px; top: 12px; right: 8px;
/// font-size: 16px; font-weight: 600; line-height: 1.2`), a colour block
/// and a numeral over it. Only the frames, the block and the numeral are
/// turned.
fn card(
    index: usize,
    front: Outline,
    back: Outline,
    block: Element<Message>,
    numeral: Text,
) -> Container<Message> {
    stack![
        // The design paints the translucent frame second.
        front.view(),
        back.view(),
        Container::new(
            outfit(Scenario::CARDS[index].title(), 16.0)
                .semibold()
                .line_height_dp(19.2)
                .fill_width()
        )
        .fill_width()
        .padding_ltrb(12.0, 12.0, 8.0, 0.0),
        block,
        numeral,
    ]
    .fill_width()
    .height(138.0)
    .on_press(Message::Scenario(index))
    .pressed_background(BLACK)
    // The numeral is an ornament; the card is its scenario.
    .label(Scenario::CARDS[index].title())
    .hint("Starts a practice call")
    .id(("home-scenario", index))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_picture_is_drawable() {
        for (name, asset) in [
            ("disc", &DISC),
            ("left rings", &RINGS_LEFT),
            ("right rings", &RINGS_RIGHT),
        ] {
            assert!(asset.check().is_ok(), "{name}: {:?}", asset.check());
        }
        assert!(STRIPES.check().is_ok(), "{:?}", STRIPES.check());
        assert_eq!((DISC.get().width(), DISC.get().height()), (250.0, 250.0));
        assert_eq!(DISC.get().view_box(), [0.0, 0.0, 280.0, 280.0]);
    }
}
