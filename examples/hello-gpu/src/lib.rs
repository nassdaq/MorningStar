//! The whole application: a tap counter. No Kotlin, no Swift, no platform
//! `cfg`.
//!
//! A title, a large count, a button and a few lines of text in English,
//! Swahili and Arabic. Everything is positioned and hit-tested by hand,
//! which is exactly the work the UI engine will take over.
//!
//! It shows the three things every Morningstar app has to get right:
//!
//! - **Sizes are designed in dp** and multiplied by the display scale, so
//!   the button is the same physical size on every screen.
//! - **Content stays inside the safe area**, while the background runs edge
//!   to edge under the status bar and the gesture bar.
//! - **It draws only when something changed.** Left alone it produces no
//!   frames at all. A tap starts a short colour animation, which asks for
//!   frames while it runs and stops asking when it ends.

use std::time::Duration;

use morningstar::prelude::*;

// The design, in dp. It needs at least `DESIGN_WIDTH` x `DESIGN_HEIGHT` of
// safe area; on anything smaller (a phone in landscape) it is scaled down to
// fit rather than clipped, since there is no scrolling yet.
const DESIGN_WIDTH: f32 = 320.0;
const DESIGN_HEIGHT: f32 = 500.0;
/// Space kept clear on the left and right of the safe area.
const MARGIN: f32 = 28.0;
// Vertical positions, measured from the top of the design.
const TITLE_TOP: f32 = 8.0;
const COUNT_TOP: f32 = 100.0;
const BUTTON_CENTER: f32 = 290.0;
const GREETINGS_TOP: f32 = 352.0;
const FOOTER_GAP: f32 = 12.0;
// Sizes.
const BUTTON_WIDTH: f32 = 192.0;
const BUTTON_HEIGHT: f32 = 60.0;
const TITLE_SIZE: f32 = 34.0;
const TAGLINE_SIZE: f32 = 16.0;
const COUNT_SIZE: f32 = 112.0;
const UNIT_SIZE: f32 = 18.0;
const LABEL_SIZE: f32 = 20.0;
const BODY_SIZE: f32 = 17.0;
const FOOTER_SIZE: f32 = 14.0;

/// Degrees of hue the colour scheme advances with each counted tap.
const HUE_STEP: f32 = 30.0;
/// How long the colour takes to get there.
const HUE_DURATION: Duration = Duration::from_millis(450);

const FONT_REGULAR: &[u8] = include_bytes!("../assets/fonts/NotoSans-Regular.ttf");
const FONT_BOLD: &[u8] = include_bytes!("../assets/fonts/NotoSans-Bold.ttf");
const FONT_ARABIC: &[u8] = include_bytes!("../assets/fonts/NotoSansArabic-Regular.ttf");

const TITLE: &str = "Morningstar";
const TAGLINE: &str = "Rust-native. GPU-first.";
const BUTTON_LABEL: &str = "Tap me";
const GREETING_SWAHILI: &str = "Habari ya asubuhi! Karibu Morningstar.";
const GREETING_ARABIC: &str = "صباح الخير، مرحبا بالعالم";
const FOOTER: &str = "This screen is drawn by one Rust source file, the same on Android and iOS.";

pub struct HelloGpu {
    taps: u32,
    /// `taps` as text, rebuilt only when it changes rather than every frame.
    count_text: String,
    /// The finger currently holding the button down, if any.
    pressed_by: Option<u64>,
    /// Where the button was in the last frame drawn; touches are hit-tested
    /// against what the user actually saw.
    button: Option<Rect>,
    hue: HueAnimation,
}

impl Default for HelloGpu {
    fn default() -> Self {
        Self::with_taps(0)
    }
}

/// The colour scheme's hue. It is at rest except for a short move after a
/// counted tap, which is the only time this app asks for frames.
struct HueAnimation {
    from: f32,
    to: f32,
    /// The hue of the last frame drawn, where a new move starts from.
    shown: f32,
    state: HueState,
}

#[derive(Clone, Copy, PartialEq)]
enum HueState {
    Resting,
    /// A tap was counted; the move starts at the next frame's clock.
    Starting,
    /// Moving since this time on the app clock.
    Moving(Duration),
}

impl HueAnimation {
    fn at_rest(hue: f32) -> Self {
        Self {
            from: hue,
            to: hue,
            shown: hue,
            state: HueState::Resting,
        }
    }

    fn move_to(&mut self, hue: f32) {
        self.from = self.shown;
        self.to = hue;
        self.state = HueState::Starting;
    }

    /// The hue to draw at `now`, and whether it is still moving (so another
    /// frame is needed).
    fn advance(&mut self, now: Duration) -> (f32, bool) {
        let started = match self.state {
            HueState::Resting => {
                self.shown = self.to;
                return (self.to, false);
            }
            HueState::Starting => now,
            HueState::Moving(started) => started,
        };
        let progress =
            (now.saturating_sub(started).as_secs_f32() / HUE_DURATION.as_secs_f32()).min(1.0);
        let moving = progress < 1.0;
        self.state = if moving {
            HueState::Moving(started)
        } else {
            HueState::Resting
        };
        // Ease out: fast at first, settling gently.
        let eased = 1.0 - (1.0 - progress).powi(3);
        self.shown = self.from + (self.to - self.from) * eased;
        (self.shown, moving)
    }
}

/// Where things are for a given frame, in physical pixels.
struct Layout {
    /// Physical pixels per design dp: the display scale, reduced if the
    /// design would not otherwise fit the safe area.
    dp: f32,
    /// The area content may occupy.
    safe: Rect,
    /// Top edge of the design, which is centred vertically in `safe`.
    top: f32,
    button: Rect,
}

impl Layout {
    /// `None` if there is no room to draw anything.
    fn new(info: &FrameInfo) -> Option<Self> {
        let safe = info.safe_area();
        if safe.is_empty() {
            return None;
        }
        let fit = (safe.width / info.dp(DESIGN_WIDTH))
            .min(safe.height / info.dp(DESIGN_HEIGHT))
            .min(1.0);
        let dp = info.scale * fit;
        let top = safe.y + (safe.height - DESIGN_HEIGHT * dp) / 2.0;
        let button = Rect::centered(
            Point::new(safe.x + safe.width / 2.0, top + BUTTON_CENTER * dp),
            BUTTON_WIDTH * dp,
            BUTTON_HEIGHT * dp,
        );
        Some(Self {
            dp,
            safe,
            top,
            button,
        })
    }

    /// Pixels for a length in dp.
    fn px(&self, dp: f32) -> f32 {
        dp * self.dp
    }

    /// The y coordinate `dp` below the top of the design.
    fn y(&self, dp: f32) -> f32 {
        self.top + dp * self.dp
    }
}

impl HelloGpu {
    pub fn with_taps(taps: u32) -> Self {
        Self {
            taps,
            count_text: taps.to_string(),
            pressed_by: None,
            button: None,
            hue: HueAnimation::at_rest(hue_for(taps)),
        }
    }

    pub fn taps(&self) -> u32 {
        self.taps
    }

    fn is_pressed(&self) -> bool {
        self.pressed_by.is_some()
    }

    fn add_tap(&mut self) {
        self.taps += 1;
        self.count_text = self.taps.to_string();
        self.hue.move_to(hue_for(self.taps));
    }
}

/// The hue the colour scheme rests at after `taps` taps.
fn hue_for(taps: u32) -> f32 {
    // Reduced before the conversion so a large count loses no precision.
    (taps % 12) as f32 * HUE_STEP + (taps / 12) as f32 * 360.0
}

/// Draws one horizontally centred block of text with its top at `top`, and
/// returns the y coordinate just below it. `size` is in dp.
fn centered(
    frame: &mut Frame,
    layout: &Layout,
    text: &str,
    size: f32,
    color: Color,
    bold: bool,
    top: f32,
) -> f32 {
    let mut style = TextStyle::new(layout.px(size), color).align(TextAlign::Center);
    if bold {
        style = style.bold();
    }
    // A margin on each side of the safe area; text wraps inside it.
    let margin = layout.px(MARGIN);
    let size = frame.draw_text(
        text,
        &style,
        Point::new(layout.safe.x + margin, top),
        Some(layout.safe.width - 2.0 * margin),
    );
    top + size.height
}

impl App for HelloGpu {
    fn setup(&mut self, setup: &mut Setup<'_>) {
        // First font with a character wins: Latin first, Arabic as fallback.
        setup.add_font(FONT_REGULAR);
        setup.add_font(FONT_BOLD);
        setup.add_font(FONT_ARABIC);
    }

    fn frame(&mut self, info: &FrameInfo, frame: &mut Frame) {
        let (hue, moving) = self.hue.advance(info.elapsed);
        if moving {
            // The only time this app asks for a frame. Without it the engine
            // draws again only when the user touches the screen or the
            // surface changes, and the device can idle.
            frame.request_redraw();
        }

        // The background is the clear colour, so it covers the whole surface
        // including the parts under the system bars.
        frame.clear = Color::from_hsv(hue, 0.55, 0.05);

        let layout = Layout::new(info);
        self.button = layout.as_ref().map(|layout| layout.button);
        let Some(layout) = layout else {
            return;
        };
        let ink = Color::WHITE;
        let dim = Color::rgba(1.0, 1.0, 1.0, 0.6);

        // Header.
        let y = centered(
            frame,
            &layout,
            TITLE,
            TITLE_SIZE,
            ink,
            true,
            layout.y(TITLE_TOP),
        );
        centered(frame, &layout, TAGLINE, TAGLINE_SIZE, dim, false, y);

        // The count.
        let margin = layout.px(MARGIN);
        let count_style = TextStyle::new(layout.px(COUNT_SIZE), ink)
            .bold()
            .line_height(1.0)
            .align(TextAlign::Center);
        let count = frame.draw_text(
            &self.count_text,
            &count_style,
            Point::new(layout.safe.x + margin, layout.y(COUNT_TOP)),
            Some(layout.safe.width - 2.0 * margin),
        );
        let unit_word = if self.taps == 1 { "tap" } else { "taps" };
        centered(
            frame,
            &layout,
            unit_word,
            UNIT_SIZE,
            dim,
            false,
            layout.y(COUNT_TOP) + count.height,
        );

        // Button: dimmer and slightly smaller while held.
        let (value, shrink) = if self.is_pressed() {
            (0.5, 0.96)
        } else {
            (0.85, 1.0)
        };
        let button = Rect::centered(
            layout.button.center(),
            layout.button.width * shrink,
            layout.button.height * shrink,
        );
        frame.list.fill_rounded_rect(
            button,
            button.height / 2.0,
            Color::from_hsv(hue + 180.0, 0.6, value),
        );
        let label_style =
            TextStyle::new(layout.px(LABEL_SIZE) * shrink, Color::rgb(0.02, 0.02, 0.05)).bold();
        let label = frame.measure_text(BUTTON_LABEL, &label_style, None);
        frame.draw_text(
            BUTTON_LABEL,
            &label_style,
            Point::new(
                button.center().x - label.width / 2.0,
                button.center().y - label.height / 2.0,
            ),
            None,
        );

        // Greetings and a wrapped paragraph.
        let mut y = layout.y(GREETINGS_TOP);
        y = centered(frame, &layout, GREETING_SWAHILI, BODY_SIZE, ink, false, y);
        y = centered(frame, &layout, GREETING_ARABIC, BODY_SIZE, ink, false, y);
        y += layout.px(FOOTER_GAP);
        centered(frame, &layout, FOOTER, FOOTER_SIZE, dim, false, y);
    }

    // The engine draws a frame after every touch event, so nothing here
    // needs to ask for one: the pressed look and the new count just appear.
    fn pointer(&mut self, event: &PointerEvent) {
        let Some(button) = self.button else {
            return; // Nothing has been drawn, so there is nothing to hit.
        };
        let on_button = button.contains(event.position);

        match event.phase {
            PointerPhase::Down => {
                // The first finger to land on the button owns the press.
                if on_button && self.pressed_by.is_none() {
                    self.pressed_by = Some(event.id);
                }
            }
            PointerPhase::Up if self.pressed_by == Some(event.id) => {
                self.pressed_by = None;
                // Sliding off before lifting abandons the tap, as on both
                // platforms' native buttons.
                if on_button {
                    self.add_tap();
                }
            }
            PointerPhase::Cancelled if self.pressed_by == Some(event.id) => {
                self.pressed_by = None;
            }
            _ => {}
        }
    }
}

morningstar::app!(HelloGpu::default());

#[cfg(test)]
mod tests {
    use super::*;
    use morningstar::DrawOp;

    const SIZE: SurfaceSize = SurfaceSize::new(1080, 2400);
    /// A typical phone: 3 px per dp, a status bar and a gesture bar.
    const SCALE: f32 = 3.0;
    const INSETS: Insets = Insets::new(0.0, 96.0, 0.0, 60.0);
    const OUTSIDE: Point = Point::new(5.0, 5.0);

    fn phone() -> FrameInfo {
        FrameInfo::new(SIZE).with_scale(SCALE).with_insets(INSETS)
    }

    fn at(seconds: f32) -> FrameInfo {
        phone().with_elapsed(Duration::from_secs_f32(seconds))
    }

    /// Draws one frame the way the engine does, into a fresh `Frame`.
    fn draw(app: &mut HelloGpu, info: FrameInfo) -> Frame {
        let mut frame = Frame::for_app(app);
        frame.begin();
        app.frame(&info, &mut frame);
        frame
    }

    /// An app that has drawn once, so it knows where its button is.
    fn app() -> HelloGpu {
        let mut app = HelloGpu::default();
        draw(&mut app, phone());
        app
    }

    fn button_of(info: FrameInfo) -> Rect {
        Layout::new(&info).unwrap().button
    }

    fn button_center() -> Point {
        button_of(phone()).center()
    }

    fn send(app: &mut HelloGpu, id: u64, phase: PointerPhase, position: Point) {
        app.pointer(&PointerEvent {
            id,
            phase,
            position,
        });
    }

    fn tap(app: &mut HelloGpu) {
        send(app, 1, PointerPhase::Down, button_center());
        send(app, 1, PointerPhase::Up, button_center());
    }

    fn glyphs(frame: &Frame) -> Vec<Rect> {
        frame
            .list
            .ops()
            .iter()
            .filter_map(|op| match *op {
                DrawOp::Glyph { rect, .. } => Some(rect),
                _ => None,
            })
            .collect()
    }

    fn drawn_button(frame: &Frame) -> Rect {
        let rects: Vec<Rect> = frame
            .list
            .ops()
            .iter()
            .filter_map(|op| match *op {
                DrawOp::Rect { rect, .. } => Some(rect),
                _ => None,
            })
            .collect();
        assert_eq!(rects.len(), 1, "the button is the only rectangle");
        rects[0]
    }

    fn inside(inner: Rect, outer: Rect) -> bool {
        // Half a pixel of tolerance for anti-aliased glyph edges.
        inner.x >= outer.x - 0.5
            && inner.y >= outer.y - 0.5
            && inner.x + inner.width <= outer.x + outer.width + 0.5
            && inner.y + inner.height <= outer.y + outer.height + 0.5
    }

    fn overlap(a: Rect, b: Rect) -> bool {
        a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
    }

    // --- Touch --------------------------------------------------------------

    #[test]
    fn tap_on_button_counts() {
        let mut app = app();
        send(&mut app, 1, PointerPhase::Down, button_center());
        assert!(app.is_pressed());
        assert_eq!(app.taps(), 0, "counts on release, not on touch");
        send(&mut app, 1, PointerPhase::Up, button_center());
        assert!(!app.is_pressed());
        assert_eq!(app.taps(), 1);
        assert_eq!(app.count_text, "1");
    }

    #[test]
    fn tap_elsewhere_does_nothing() {
        let mut app = app();
        send(&mut app, 1, PointerPhase::Down, OUTSIDE);
        assert!(!app.is_pressed());
        send(&mut app, 1, PointerPhase::Up, OUTSIDE);
        assert_eq!(app.taps(), 0);
    }

    #[test]
    fn sliding_off_before_release_abandons_the_tap() {
        let mut app = app();
        send(&mut app, 1, PointerPhase::Down, button_center());
        send(&mut app, 1, PointerPhase::Moved, OUTSIDE);
        send(&mut app, 1, PointerPhase::Up, OUTSIDE);
        assert_eq!(app.taps(), 0);
        assert!(!app.is_pressed());
    }

    #[test]
    fn cancel_releases_without_counting() {
        let mut app = app();
        send(&mut app, 1, PointerPhase::Down, button_center());
        send(&mut app, 1, PointerPhase::Cancelled, button_center());
        assert_eq!(app.taps(), 0);
        assert!(!app.is_pressed());
    }

    #[test]
    fn a_second_finger_cannot_complete_the_first_fingers_press() {
        let mut app = app();
        send(&mut app, 1, PointerPhase::Down, button_center());
        send(&mut app, 2, PointerPhase::Down, button_center());
        send(&mut app, 2, PointerPhase::Up, button_center());
        assert!(app.is_pressed(), "finger 1 is still down");
        assert_eq!(app.taps(), 0);
        send(&mut app, 1, PointerPhase::Up, button_center());
        assert_eq!(app.taps(), 1);
    }

    #[test]
    fn touches_before_the_first_frame_are_ignored() {
        let mut app = HelloGpu::default();
        send(&mut app, 1, PointerPhase::Down, button_center());
        assert!(!app.is_pressed());
    }

    #[test]
    fn touches_hit_the_button_where_it_was_last_drawn() {
        // After a rotation the button is somewhere else; the old position
        // must stop working and the new one must work.
        let mut app = app();
        let landscape = FrameInfo::new(SurfaceSize::new(2400, 1080))
            .with_scale(SCALE)
            .with_insets(Insets::new(96.0, 0.0, 0.0, 60.0));
        draw(&mut app, landscape);
        let old = button_center();
        let new = button_of(landscape).center();
        assert!(!button_of(landscape).contains(old));

        send(&mut app, 1, PointerPhase::Down, old);
        assert!(!app.is_pressed());
        send(&mut app, 1, PointerPhase::Up, old);
        send(&mut app, 1, PointerPhase::Down, new);
        send(&mut app, 1, PointerPhase::Up, new);
        assert_eq!(app.taps(), 1);
    }

    // --- Content ------------------------------------------------------------

    #[test]
    fn frame_has_one_button_and_text() {
        let frame = draw(&mut HelloGpu::default(), phone());
        assert_eq!(drawn_button(&frame), button_of(phone()));
        assert!(glyphs(&frame).len() > 100, "several lines of text");
    }

    #[test]
    fn a_longer_count_draws_more_glyphs() {
        let one_digit = glyphs(&draw(&mut HelloGpu::with_taps(7), phone())).len();
        let three_digits = glyphs(&draw(&mut HelloGpu::with_taps(123), phone())).len();
        assert_eq!(three_digits, one_digit + 2);
    }

    // --- Scale: sizes are designed in dp -----------------------------------

    #[test]
    fn the_button_is_its_design_size_in_dp_at_every_scale() {
        for scale in [1.0, 1.5, 2.0, 2.8125, 3.0, 3.5] {
            // The same 360 x 800 dp screen at each density.
            let size = SurfaceSize::new((360.0 * scale) as u32, (800.0 * scale) as u32);
            let info = FrameInfo::new(size).with_scale(scale);
            let button = drawn_button(&draw(&mut HelloGpu::default(), info));
            assert_eq!(
                (button.width, button.height),
                (BUTTON_WIDTH * scale, BUTTON_HEIGHT * scale),
                "at {scale}x"
            );
        }
    }

    #[test]
    fn the_layout_is_the_same_in_dp_at_every_scale() {
        // Every glyph of the 3x frame is where the 1x frame has it, times 3,
        // give or take glyph rasterisation (origins snap to whole pixels).
        let small = FrameInfo::new(SurfaceSize::new(360, 800));
        let large = FrameInfo::new(SurfaceSize::new(1080, 2400)).with_scale(3.0);
        let at_1x = glyphs(&draw(&mut HelloGpu::default(), small));
        let at_3x = glyphs(&draw(&mut HelloGpu::default(), large));

        assert_eq!(at_1x.len(), at_3x.len(), "same lines, same wrapping");
        for (a, b) in at_1x.iter().zip(&at_3x) {
            assert!(
                (a.x * 3.0 - b.x).abs() <= 6.0 && (a.y * 3.0 - b.y).abs() <= 6.0,
                "{a:?} at 1x is {b:?} at 3x"
            );
        }
    }

    #[test]
    fn a_higher_scale_on_the_same_pixels_makes_things_bigger() {
        // What the old fraction-of-the-screen layout could not do: the same
        // surface at another density changes the physical size of things.
        let size = SurfaceSize::new(1440, 3200);
        let dense = drawn_button(&draw(
            &mut HelloGpu::default(),
            FrameInfo::new(size).with_scale(4.0),
        ));
        let sparse = drawn_button(&draw(
            &mut HelloGpu::default(),
            FrameInfo::new(size).with_scale(2.0),
        ));
        assert_eq!(dense.width, 2.0 * sparse.width);
    }

    // --- Safe area ----------------------------------------------------------

    /// Phones and orientations the layout must work on: (size, scale, insets).
    fn screens() -> Vec<FrameInfo> {
        let screen = |width, height, scale: f32, insets| {
            FrameInfo::new(SurfaceSize::new(width, height))
                .with_scale(scale)
                .with_insets(insets)
        };
        vec![
            phone(),
            // The Galaxy A27 this is tested on.
            screen(1080, 2340, 2.8125, Insets::new(0.0, 84.0, 0.0, 135.0)),
            // Landscape: cutout on the left, three-button bar on the right.
            screen(2400, 1080, 3.0, Insets::new(96.0, 0.0, 144.0, 0.0)),
            // Landscape with a status bar and gesture bar.
            screen(2340, 1080, 2.8125, Insets::new(84.0, 60.0, 0.0, 48.0)),
            // A small, low-density phone.
            screen(720, 1280, 2.0, Insets::new(0.0, 48.0, 0.0, 96.0)),
            screen(480, 800, 1.5, Insets::new(0.0, 36.0, 0.0, 72.0)),
            // An iPhone with a notch and home indicator, and a tablet.
            screen(1170, 2532, 3.0, Insets::new(0.0, 141.0, 0.0, 102.0)),
            screen(1600, 2560, 2.0, Insets::new(0.0, 48.0, 0.0, 48.0)),
            // No insets at all, and a split-screen sliver.
            screen(1080, 2400, 3.0, Insets::ZERO),
            screen(1080, 900, 3.0, Insets::new(0.0, 96.0, 0.0, 0.0)),
        ]
    }

    #[test]
    fn everything_stays_inside_the_safe_area() {
        for info in screens() {
            let safe = info.safe_area();
            let frame = draw(&mut HelloGpu::with_taps(1234), info);
            assert!(inside(drawn_button(&frame), safe), "button at {info:?}");
            for rect in glyphs(&frame) {
                assert!(inside(rect, safe), "{rect:?} outside {safe:?} at {info:?}");
            }
        }
    }

    #[test]
    fn only_its_label_is_drawn_over_the_button() {
        for info in screens() {
            let frame = draw(&mut HelloGpu::with_taps(1234), info);
            let button = drawn_button(&frame);
            let mut label = 0;
            for rect in glyphs(&frame) {
                if inside(rect, button) {
                    label += 1;
                } else {
                    assert!(!overlap(rect, button), "{rect:?} collides at {info:?}");
                }
            }
            assert_eq!(label, BUTTON_LABEL.replace(' ', "").len(), "at {info:?}");
        }
    }

    #[test]
    fn insets_move_the_content_but_not_the_background() {
        let without = FrameInfo::new(SIZE).with_scale(SCALE);
        let with = without.with_insets(Insets::new(0.0, 300.0, 0.0, 0.0));
        let plain = draw(&mut HelloGpu::default(), without);
        let inset = draw(&mut HelloGpu::default(), with);

        // Content is centred in what is left: half the inset further down.
        assert_eq!(
            drawn_button(&inset).y - drawn_button(&plain).y,
            150.0,
            "button"
        );
        assert!(glyphs(&inset).iter().all(|glyph| glyph.y >= 300.0));
        // The background is the clear colour, which has no geometry: it
        // always covers the whole surface, under the bars too.
        assert_eq!(inset.clear, plain.clear);
    }

    #[test]
    fn a_side_inset_recentres_the_content_horizontally() {
        let info = FrameInfo::new(SurfaceSize::new(2400, 1080))
            .with_scale(SCALE)
            .with_insets(Insets::new(200.0, 0.0, 0.0, 0.0));
        let button = drawn_button(&draw(&mut HelloGpu::default(), info));
        assert_eq!(button.center().x, 200.0 + 2200.0 / 2.0);
    }

    #[test]
    fn a_surface_with_no_safe_area_draws_only_the_background() {
        let info = FrameInfo::new(SurfaceSize::new(200, 100))
            .with_insets(Insets::new(0.0, 80.0, 0.0, 80.0));
        let mut app = app();
        let frame = draw(&mut app, info);
        assert!(frame.list.is_empty());
        // And there is no button to hit any more.
        send(&mut app, 1, PointerPhase::Down, button_center());
        assert!(!app.is_pressed());
    }

    // --- Redraw on demand ---------------------------------------------------

    #[test]
    fn an_untouched_app_never_asks_for_a_frame() {
        let mut app = HelloGpu::default();
        let first = draw(&mut app, at(0.0));
        assert!(!first.redraw_requested());

        // However much later it is drawn again (a rotation, say), it still
        // does not ask, and the picture has not changed with time.
        let later = draw(&mut app, at(3600.0));
        assert!(!later.redraw_requested());
        assert_eq!(later.clear, first.clear);
        assert_eq!(later.list.ops(), first.list.ops());
    }

    #[test]
    fn pressing_the_button_changes_the_picture_without_animating() {
        let mut app = app();
        let idle = draw(&mut app, at(1.0));
        send(&mut app, 1, PointerPhase::Down, button_center());
        let pressed = draw(&mut app, at(1.1));

        assert!(
            drawn_button(&pressed).width < drawn_button(&idle).width,
            "the held button is drawn smaller"
        );
        assert!(!pressed.redraw_requested(), "one frame is enough");
    }

    #[test]
    fn a_tap_animates_the_colour_for_a_moment_then_stops() {
        let mut app = app();
        let before = draw(&mut app, at(10.0)).clear;
        tap(&mut app);

        // The frame that follows the tap starts the animation...
        let start = draw(&mut app, at(10.0));
        assert!(start.redraw_requested());
        assert_eq!(start.clear, before, "it starts from the current colour");

        // ...it keeps asking for frames while the colour moves...
        let mut previous = start.clear;
        for step in 1..=4 {
            let frame = draw(&mut app, at(10.0 + 0.1 * step as f32));
            assert!(frame.redraw_requested(), "step {step}");
            assert_ne!(frame.clear, previous, "step {step}");
            previous = frame.clear;
        }

        // ...and stops once it has arrived, on the colour a fresh app with
        // one tap would show.
        let end = draw(&mut app, at(10.0 + HUE_DURATION.as_secs_f32() + 0.01));
        assert!(!end.redraw_requested());
        assert_eq!(end.clear, draw(&mut HelloGpu::with_taps(1), phone()).clear);
        assert_ne!(end.clear, before);

        let after = draw(&mut app, at(500.0));
        assert!(!after.redraw_requested(), "and it stays stopped");
        assert_eq!(after.clear, end.clear);
    }

    #[test]
    fn the_animation_ends_even_if_only_one_more_frame_is_ever_drawn() {
        // If the app is backgrounded mid-animation, the next frame may come
        // much later. It must finish the animation, not restart it.
        let mut app = app();
        tap(&mut app);
        assert!(draw(&mut app, at(1.0)).redraw_requested());
        assert!(!draw(&mut app, at(90.0)).redraw_requested());
    }

    #[test]
    fn a_tap_during_the_animation_continues_from_the_colour_on_screen() {
        let mut app = app();
        tap(&mut app);
        draw(&mut app, at(1.0));
        let midway = draw(&mut app, at(1.2)).clear;

        tap(&mut app);
        let restarted = draw(&mut app, at(1.2));
        assert_eq!(restarted.clear, midway, "no jump");
        assert!(restarted.redraw_requested());

        let end = draw(&mut app, at(5.0));
        assert!(!end.redraw_requested());
        assert_eq!(end.clear, draw(&mut HelloGpu::with_taps(2), phone()).clear);
    }

    #[test]
    fn an_abandoned_press_does_not_animate() {
        let mut app = app();
        send(&mut app, 1, PointerPhase::Down, button_center());
        send(&mut app, 1, PointerPhase::Up, OUTSIDE);
        assert!(!draw(&mut app, at(1.0)).redraw_requested());
    }

    /// Runs the app the way a host does for ten seconds at 120 Hz, drawing
    /// only when the engine would, and counts the frames.
    #[test]
    fn ten_seconds_with_one_tap_draw_only_a_handful_of_frames() {
        let mut app = HelloGpu::default();
        let mut wanted = true; // The first frame.
        let mut drawn = 0;
        for tick in 0..1200u32 {
            if tick == 600 {
                tap(&mut app);
                wanted = true; // Input always produces a frame.
            }
            if wanted {
                let frame = draw(&mut app, at(tick as f32 / 120.0));
                wanted = frame.redraw_requested();
                drawn += 1;
            }
        }
        // One at start, then the animation: 450 ms at 120 Hz is 54 frames,
        // plus the frame that starts it and the one that ends it.
        assert!((50..=60).contains(&drawn), "{drawn} frames drawn");
        assert_eq!(app.taps(), 1);
    }

    // --- Pixels -------------------------------------------------------------

    /// Renders the real frame on the GPU and checks what a user would see.
    #[test]
    fn rendered_frame_shows_button_label_and_count() {
        // A small surface keeps the readback cheap: 320 x 540 dp at 1x, with
        // a status bar.
        let size = SurfaceSize::new(320, 540);
        let info = FrameInfo::new(size).with_insets(Insets::new(0.0, 24.0, 0.0, 16.0));
        let frame = draw(&mut HelloGpu::with_taps(8), info);

        let pixels = match morningstar_gpu::render_offscreen_with_images(
            size,
            frame.clear,
            &frame.list,
            frame.atlas(),
            frame.images(),
        ) {
            Ok(pixels) => pixels,
            Err(morningstar_gpu::GpuError::NoAdapter(reason)) => {
                eprintln!("skipping: no GPU adapter available ({reason})");
                return;
            }
            Err(other) => panic!("render failed: {other}"),
        };
        let green = |x: f32, y: f32| pixels[((y as u32 * size.width + x as u32) * 4 + 1) as usize];
        let layout = Layout::new(&info).unwrap();
        let button = layout.button;

        // The button is bright near its left end, clear of the label.
        let fill = green(button.x + button.height / 2.0, button.center().y);
        assert!(fill > 150, "button fill was {fill}");
        // Its dark label makes some pixels along the middle much darker.
        let darkest = (0..button.width as u32)
            .map(|dx| green(button.x + dx as f32, button.center().y))
            .min()
            .unwrap();
        assert!(darkest < 60, "no label ink found on the button: {darkest}");

        // The big white count has ink in the band where it is drawn.
        let band = layout.y(COUNT_TOP) as u32..layout.y(COUNT_TOP + COUNT_SIZE) as u32;
        let brightest = (0..size.width)
            .flat_map(|x| band.clone().map(move |y| (x, y)))
            .map(|(x, y)| green(x as f32, y as f32))
            .max()
            .unwrap();
        assert!(brightest > 240, "no count ink found: {brightest}");

        // The area under the status bar is background, edge to edge: dark,
        // and not left unpainted.
        let under_bar = (0..size.width)
            .map(|x| green(x as f32, 10.0))
            .max()
            .unwrap();
        assert!(under_bar < 40, "content under the status bar: {under_bar}");
    }
}
