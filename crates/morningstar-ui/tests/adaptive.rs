//! ADR 0021: the viewport in `view`, wrapping rows, scroll control, a
//! bounded multi-line input, animation and the navigator, driven the way
//! the engine drives a `Ui`: frames on demand at stated times, pointer and
//! text events in between.

use std::hash::Hash;
use std::sync::Mutex;
use std::time::Duration;

use morningstar_core::{
    DrawOp, Insets, Point, PointerEvent, PointerPhase, Rect, Size, SurfaceSize, TextInputEvent,
};
use morningstar_devtools::{Image as Screenshot, screenshot_frame};
use morningstar_runtime::{App, Frame, FrameInfo, GpuError};
use morningstar_ui::prelude::*;

const REGULAR: &[u8] =
    include_bytes!("../../../examples/hello-gpu/assets/fonts/NotoSans-Regular.ttf");

const PAPER: Color = hex(0xFFFFFF);
const INK: Color = hex(0x101418);
const RED: Color = hex(0xD0021B);
const BLUE: Color = hex(0x0B5FFF);
const GREEN: Color = hex(0x1E8416);
const CARET: Color = hex(0xC2185B);

const SCALE: f32 = 3.0;

#[derive(Clone, Debug, PartialEq)]
enum Msg {
    Flag(usize),
    Count(usize),
    Text(String),
    Scroll(ScrollTo),
    Push(u8),
    Replace(u8),
    Tapped(&'static str),
}

type View = fn(&Lab, &Viewport) -> Element<Msg>;

/// What a view saw of the screen it was built for.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Seen {
    size: Size,
    insets: Insets,
    keyboard: f32,
    safe: Size,
    focused: bool,
}

struct Lab {
    view: View,
    flags: [bool; 4],
    count: usize,
    text: String,
    screens: Navigator<u8>,
    scroll: Option<ScrollTo>,
    log: Vec<Msg>,
    seen: Mutex<Option<Seen>>,
    measured: Mutex<Vec<Size>>,
}

impl Lab {
    fn new(view: View) -> Self {
        Self {
            view,
            flags: [false; 4],
            count: 0,
            text: String::new(),
            screens: Navigator::new(0),
            scroll: None,
            log: Vec::new(),
            seen: Mutex::new(None),
            measured: Mutex::new(Vec::new()),
        }
    }
}

impl Application for Lab {
    type Message = Msg;

    fn setup(&mut self, setup: &mut Setup<'_>) {
        setup.add_font(REGULAR);
    }

    fn theme(&self) -> Theme {
        let mut theme = Theme::LIGHT;
        theme.colors.background = PAPER;
        theme
    }

    fn view_in(&self, viewport: &Viewport<'_>) -> Element<Msg> {
        *self.seen.lock().unwrap() = Some(Seen {
            size: viewport.size,
            insets: viewport.insets,
            keyboard: viewport.keyboard_inset,
            safe: viewport.safe_size(),
            focused: viewport.is_focused("note"),
        });
        (self.view)(self, viewport)
    }

    fn update(&mut self, message: Msg) {
        self.log.push(message.clone());
        match message {
            Msg::Flag(index) => self.flags[index] = !self.flags[index],
            Msg::Count(count) => self.count = count,
            Msg::Text(text) => self.text = text,
            Msg::Scroll(to) => self.scroll = Some(to),
            Msg::Push(screen) => self.screens.push(screen),
            Msg::Replace(screen) => self.screens.replace(screen),
            Msg::Tapped(_) => {}
        }
    }

    fn back(&mut self) -> bool {
        self.screens.back()
    }

    fn effects(&mut self, out: &mut Effects<Msg>) {
        if let Some(to) = self.scroll.take() {
            out.scroll(to);
        }
    }
}

struct Device {
    ui: Ui<Lab>,
    frame: Frame,
    info: FrameInfo,
    time: Duration,
}

impl Device {
    /// 360 x 800 dp at 3 px per dp.
    fn new(view: View) -> Self {
        Self::with(
            view,
            FrameInfo::new(SurfaceSize::new(1080, 2400)).with_scale(SCALE),
        )
    }

    fn with(view: View, info: FrameInfo) -> Self {
        let mut ui = Ui::new(Lab::new(view));
        let frame = Frame::for_app(&mut ui);
        let mut device = Self {
            ui,
            frame,
            info,
            time: Duration::ZERO,
        };
        device.draw();
        device
    }

    /// Builds a frame at the current time; true if it asked for another
    /// at once.
    fn draw(&mut self) -> bool {
        self.frame.begin();
        self.ui
            .frame(&self.info.with_elapsed(self.time), &mut self.frame);
        self.frame.redraw_requested()
    }

    /// Advances the clock and builds a frame.
    fn step(&mut self, millis: u64) -> bool {
        self.time += Duration::from_millis(millis);
        self.draw()
    }

    /// Delivers a message and builds the frame that follows it.
    fn send(&mut self, message: Msg) -> bool {
        self.ui.send(message);
        self.draw()
    }

    /// True if the last frame asked for nothing more: no frame at once,
    /// none later.
    fn idle(&self) -> bool {
        !self.frame.redraw_requested() && self.frame.redraw_after().is_none()
    }

    fn touch(&mut self, phase: PointerPhase, x: f32, y: f32) {
        self.ui.pointer(&PointerEvent {
            id: 1,
            phase,
            position: Point::new(x * SCALE, y * SCALE),
        });
    }

    /// A tap at a position in dp, with the frames the engine would build.
    fn tap_at(&mut self, x: f32, y: f32) {
        self.touch(PointerPhase::Down, x, y);
        self.draw();
        self.touch(PointerPhase::Up, x, y);
        self.draw();
    }

    fn tap(&mut self, id: impl Hash) {
        let rect = self.rect(id);
        self.tap_at(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    }

    fn key(&mut self, event: TextInputEvent) {
        self.ui.text_input(&event);
        self.draw();
    }

    /// Where the widget with this id was laid out, in dp.
    fn rect(&self, id: impl Hash) -> Rect {
        dp(self.ui.rect_of(id).expect("the widget is in the tree"))
    }

    fn visible(&self, id: impl Hash) -> Option<Rect> {
        self.ui.visible_rect_of(id).map(dp)
    }

    fn offset(&self, id: impl Hash) -> f32 {
        self.ui.scroll_offset(id).expect("the scroll view is there")
    }

    fn seen(&self) -> Seen {
        self.ui.app().seen.lock().unwrap().expect("a frame")
    }

    /// Filled rectangles of the frame whose colour is `color` at any
    /// opacity, with the opacity they are shown at, in dp: the colour's
    /// own times that of the layers they are drawn through (ADR 0028).
    fn fills(&self, color: Color) -> Vec<(Rect, f32)> {
        let mut layers: Vec<f32> = Vec::new();
        let mut found = Vec::new();
        for op in self.frame.list.ops() {
            match *op {
                DrawOp::PushLayer { opacity } => layers.push(opacity),
                DrawOp::PopLayer => {
                    layers.pop();
                }
                DrawOp::Rect { rect, color: c, .. }
                    if (c.r, c.g, c.b) == (color.r, color.g, color.b) =>
                {
                    found.push((dp(rect), c.a * layers.iter().product::<f32>()));
                }
                _ => {}
            }
        }
        found
    }

    fn fill(&self, color: Color) -> (Rect, f32) {
        let fills = self.fills(color);
        assert_eq!(fills.len(), 1, "one box of that colour: {fills:?}");
        fills[0]
    }
}

fn dp(rect: Rect) -> Rect {
    Rect::new(
        rect.x / SCALE,
        rect.y / SCALE,
        rect.width / SCALE,
        rect.height / SCALE,
    )
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() <= 0.34
}

fn chip(label: &'static str, width: f32) -> Container<Msg> {
    Container::new(text(label).size(12.0))
        .size(width, 30.0)
        .id(label)
}

fn rows(count: usize, height: f32, prefix: &'static str) -> Container<Msg> {
    Column::new()
        .extend((0..count).map(|index| {
            Container::<Msg>::empty()
                .fill_width()
                .height(height)
                .id((prefix, index))
        }))
        .fill_width()
}

// ------------------------------------------------------------- viewport

fn plain(_: &Lab, _: &Viewport) -> Element<Msg> {
    Container::empty().fill().id("root").into()
}

#[test]
fn a_view_is_told_the_screen_in_dp_with_its_insets_and_the_keyboard() {
    let info = FrameInfo::new(SurfaceSize::new(1080, 2400))
        .with_scale(SCALE)
        .with_insets(Insets::new(0.0, 96.0, 0.0, 60.0));
    let mut device = Device::with(plain, info);
    assert_eq!(
        device.seen(),
        Seen {
            size: Size::new(360.0, 800.0),
            insets: Insets::new(0.0, 32.0, 0.0, 20.0),
            keyboard: 0.0,
            safe: Size::new(360.0, 748.0),
            focused: false,
        }
    );

    // The keyboard comes up over 300 dp: the next frame's view knows.
    device.info = info.with_keyboard_inset(900.0);
    device.draw();
    assert_eq!(device.seen().keyboard, 300.0);
    assert_eq!(device.seen().safe, Size::new(360.0, 468.0));

    // The phone is turned: so is what the view is told.
    device.info = FrameInfo::new(SurfaceSize::new(2400, 1080)).with_scale(SCALE);
    device.draw();
    assert_eq!(device.seen().size, Size::new(800.0, 360.0));
}

fn panes(_: &Lab, viewport: &Viewport) -> Element<Msg> {
    let list = Container::<Msg>::empty().id("list");
    let detail = Container::<Msg>::empty().id("detail");
    if viewport.width() >= 600.0 {
        row![list.width(320.0).fill_height(), detail.fill()]
            .fill()
            .into()
    } else {
        col![list.fill()].fill().into()
    }
}

#[test]
fn a_view_can_choose_its_layout_from_the_width() {
    let mut device = Device::new(panes);
    assert_eq!(device.rect("list"), Rect::new(0.0, 0.0, 360.0, 800.0));
    assert!(device.ui.rect_of("detail").is_none());

    device.info = FrameInfo::new(SurfaceSize::new(2400, 1080)).with_scale(SCALE);
    device.draw();
    assert_eq!(device.rect("list"), Rect::new(0.0, 0.0, 320.0, 360.0));
    assert_eq!(device.rect("detail"), Rect::new(320.0, 0.0, 480.0, 360.0));
}

const LONG: &str = "Hujambo dunia, habari za asubuhi na karibu sana";

fn measuring(lab: &Lab, viewport: &Viewport) -> Element<Msg> {
    let label = || text(LONG).size(14.0);
    let natural = viewport.measure(&label());
    let wrapped = viewport.measure_wrapped(&label(), 200.0);
    let lines = viewport.line_count(&label(), 200.0);
    let cut = viewport.measure_wrapped(&label().max_lines(1), 200.0);
    *lab.measured.lock().unwrap() = vec![natural, wrapped, Size::new(lines as f32, 0.0), cut];
    col![
        label().id("natural"),
        Container::new(label().fill_width().id("wrapped")).width(200.0),
    ]
    .cross_align(CrossAlign::Start)
    .into()
}

#[test]
fn a_view_measures_text_as_layout_will() {
    let device = Device::new(measuring);
    let measured = device.ui.app().measured.lock().unwrap().clone();
    let (natural, wrapped, lines, cut) = (measured[0], measured[1], measured[2].width, measured[3]);

    let laid_out = device.rect("natural");
    assert!(
        close(natural.width, laid_out.width),
        "{natural:?} {laid_out:?}"
    );
    assert!(close(natural.height, laid_out.height));
    assert!(natural.width > 250.0, "one long line: {natural:?}");

    let laid_out = device.rect("wrapped");
    assert!(
        close(wrapped.height, laid_out.height),
        "{wrapped:?} {laid_out:?}"
    );
    assert!(wrapped.width <= 200.0);
    assert!(lines >= 2.0, "{lines} lines at 200 dp");
    assert!(close(wrapped.height, lines * natural.height));
    // `max_lines` bounds what is measured, as it bounds the layout.
    assert!(close(cut.height, natural.height));
}

// ----------------------------------------------------------------- wrap

fn chips(lab: &Lab, _: &Viewport) -> Element<Msg> {
    let group = Row::new()
        .push(chip("a", 100.0))
        .push(chip("b", 120.0))
        .push(chip("c", 150.0))
        .push(chip("d", 90.0))
        .push(chip("e", 200.0))
        .gap(10.0)
        .line_gap(6.0)
        .fill_width();
    let group = if lab.flags[0] { group } else { group.wrap() };
    ScrollView::new(
        col![
            group.id("chips"),
            Container::empty().fill_width().height(900.0).id("below"),
        ]
        .padding(20.0)
        .fill_width(),
    )
    .id("page")
    .into()
}

#[test]
fn a_wrapping_row_breaks_into_lines_and_hugs_their_height() {
    let device = Device::new(chips);
    // 320 dp inside the padding. Line 1: 100 + 10 + 120 = 230 (150 more
    // would not fit). Line 2: 150 + 10 + 90 = 250. Line 3: 200.
    assert_eq!(device.rect("a"), Rect::new(20.0, 20.0, 100.0, 30.0));
    assert_eq!(device.rect("b"), Rect::new(130.0, 20.0, 120.0, 30.0));
    assert_eq!(device.rect("c"), Rect::new(20.0, 56.0, 150.0, 30.0));
    assert_eq!(device.rect("d"), Rect::new(180.0, 56.0, 90.0, 30.0));
    assert_eq!(device.rect("e"), Rect::new(20.0, 92.0, 200.0, 30.0));
    // Three lines of 30 and two gaps of 6; what follows starts under them.
    assert_eq!(device.rect("chips"), Rect::new(20.0, 20.0, 320.0, 102.0));
    assert_eq!(device.rect("below").y, 122.0);
    assert!(
        device.ui.overflows().is_empty(),
        "{:?}",
        device.ui.overflows()
    );
    // The scroll view scrolls the wrapped height: 20 + 102 + 900 + 20.
    assert_eq!(device.ui.max_scroll_offset("page"), Some(1042.0 - 800.0));
}

#[test]
fn without_wrap_the_same_row_overflows() {
    let mut device = Device::new(chips);
    device.send(Msg::Flag(0));
    assert_eq!(device.rect("e").y, 20.0);
    let overflows = device.ui.overflows();
    assert_eq!(overflows.len(), 1, "{overflows:?}");
    // 100 + 120 + 150 + 90 + 200 + 4 * 10 = 700 in 320.
    assert_eq!(overflows[0].amount, Size::new(380.0, 0.0));
}

fn fitting(lab: &Lab, _: &Viewport) -> Element<Msg> {
    let line = |wrap: bool| {
        let row = Row::new()
            .push(text("One").size(14.0))
            .push(
                Container::new(text("Two").size(14.0))
                    .padding(8.0)
                    .background(BLUE),
            )
            .push(Spacer::new())
            .push(text("Three").size(20.0))
            .gap(12.0)
            .cross_align(CrossAlign::Center)
            .fill_width();
        if wrap { row.wrap() } else { row }
    };
    col![line(lab.flags[0]), text("after")]
        .gap(8.0)
        .padding(16.0)
        .into()
}

#[test]
fn a_row_that_fits_is_drawn_the_same_with_and_without_wrap() {
    let mut device = Device::new(fitting);
    let plain = device.frame.list.ops().to_vec();
    device.send(Msg::Flag(0));
    assert_eq!(device.frame.list.ops(), plain);
    assert!(plain.len() > 10);
}

#[test]
fn wrap_new_is_a_row_that_wraps() {
    fn view(_: &Lab, _: &Viewport) -> Element<Msg> {
        col![
            Wrap::new()
                .push(chip("a", 200.0))
                .push(chip("b", 200.0))
                .width(360.0)
        ]
        .into()
    }
    let device = Device::new(view);
    assert_eq!(device.rect("b"), Rect::new(0.0, 30.0, 200.0, 30.0));
}

// --------------------------------------------------------------- scroll

fn list(lab: &Lab, _: &Viewport) -> Element<Msg> {
    ScrollView::new(rows(30 + lab.count, 80.0, "row"))
        .id("list")
        .into()
}

#[test]
fn an_application_scrolls_a_list_to_its_end_its_top_and_an_offset() {
    let mut device = Device::new(list);
    assert_eq!(device.offset("list"), 0.0);
    // 30 rows of 80 in 800: 1600 to scroll.
    device.send(Msg::Scroll(ScrollTo::end("list")));
    assert_eq!(device.offset("list"), 1600.0);
    assert_eq!(device.rect(("row", 29_usize)).y, 720.0);
    assert!(device.idle(), "a jump needs no further frame");

    device.send(Msg::Scroll(ScrollTo::offset("list", 240.0)));
    assert_eq!(device.offset("list"), 240.0);
    device.send(Msg::Scroll(ScrollTo::offset("list", 1.0e9)));
    assert_eq!(device.offset("list"), 1600.0, "kept within the content");
    device.send(Msg::Scroll(ScrollTo::top("list")));
    assert_eq!(device.offset("list"), 0.0);

    // A request for something that is not there is dropped, not kept.
    device.send(Msg::Scroll(ScrollTo::end("no such list")));
    device.send(Msg::Scroll(ScrollTo::reveal("no such row")));
    assert_eq!(device.offset("list"), 0.0);
    assert!(device.idle());
}

#[test]
fn an_animated_scroll_glides_asks_for_frames_only_meanwhile_and_ends_exactly() {
    let mut device = Device::new(list);
    assert!(device.send(Msg::Scroll(ScrollTo::end("list").animated())));
    assert_eq!(device.offset("list"), 0.0, "it starts where it was");
    // 250 ms, easing out: half-way in time is 1 - 0.5^3 of the way.
    assert!(device.step(125));
    assert!(close(device.offset("list"), 1600.0 * 0.875));
    assert!(device.step(100));
    assert!(device.offset("list") < 1600.0);
    assert!(!device.step(25), "the last frame asks for no other");
    assert_eq!(device.offset("list"), 1600.0);
    assert!(device.idle());
    assert!(!device.step(1000));
    assert_eq!(device.offset("list"), 1600.0);
}

#[test]
fn a_touch_stops_a_glide_where_it_is() {
    let mut device = Device::new(list);
    device.send(Msg::Scroll(ScrollTo::end("list").animated()));
    device.step(125);
    let reached = device.offset("list");
    device.touch(PointerPhase::Down, 180.0, 400.0);
    assert!(!device.step(50), "nothing moves under a resting finger");
    assert_eq!(device.offset("list"), reached);
    device.touch(PointerPhase::Up, 180.0, 400.0);
    assert!(!device.step(500));
    assert_eq!(device.offset("list"), reached);
}

#[test]
fn with_reduced_motion_an_animated_scroll_jumps() {
    let mut device = Device::new(list);
    device.ui.set_reduce_motion(true);
    assert!(!device.send(Msg::Scroll(ScrollTo::end("list").animated())));
    assert_eq!(device.offset("list"), 1600.0);
}

#[test]
fn a_widget_is_revealed_by_scrolling_as_little_as_needed() {
    let mut device = Device::new(list);
    // Row 4 (320..400) is in view: nothing moves.
    device.send(Msg::Scroll(ScrollTo::reveal(("row", 4_usize))));
    assert_eq!(device.offset("list"), 0.0);
    // Row 14 is at 1120..1200: its bottom comes to the viewport's.
    device.send(Msg::Scroll(ScrollTo::reveal(("row", 14_usize))));
    assert_eq!(device.offset("list"), 400.0);
    assert_eq!(
        device.visible(("row", 14_usize)),
        Some(Rect::new(0.0, 720.0, 360.0, 80.0))
    );
    // Row 2 is now above: its top comes to the viewport's, with a margin.
    device.send(Msg::Scroll(ScrollTo::reveal(("row", 2_usize)).margin(10.0)));
    assert_eq!(device.offset("list"), 150.0);
    // The last row with a margin: the end is as far as it goes.
    device.send(Msg::Scroll(
        ScrollTo::reveal(("row", 29_usize)).margin(50.0),
    ));
    assert_eq!(device.offset("list"), 1600.0);
    // Gliding there.
    assert!(device.send(Msg::Scroll(ScrollTo::reveal(("row", 0_usize)).animated())));
    device.step(125);
    assert!(close(device.offset("list"), 1600.0 * 0.125));
    assert!(!device.step(125));
    assert_eq!(device.offset("list"), 0.0);
}

fn nested(_: &Lab, _: &Viewport) -> Element<Msg> {
    ScrollView::new(
        col![
            Container::empty().fill_width().height(1000.0),
            ScrollView::new(rows(30, 50.0, "inner-row"))
                .fill_width()
                .height(300.0)
                .id("inner"),
            Container::empty().fill_width().height(1000.0),
        ]
        .fill_width(),
    )
    .id("outer")
    .into()
}

#[test]
fn revealing_scrolls_every_scroll_view_around_the_widget() {
    let mut device = Device::new(nested);
    device.send(Msg::Scroll(ScrollTo::reveal(("inner-row", 20_usize))));
    // The row is 1000..1050 inside the inner content: the inner view (300
    // tall) scrolls its bottom into view, 1050 - 300. The row is then at
    // 1250..1300 of the outer content, and the outer view (800 tall)
    // scrolls that into view in turn.
    assert_eq!(device.offset("inner"), 750.0);
    assert_eq!(device.offset("outer"), 500.0);
    assert_eq!(
        device.visible(("inner-row", 20_usize)),
        Some(Rect::new(0.0, 750.0, 360.0, 50.0))
    );
}

#[test]
fn a_scroll_position_survives_rebuilds_and_growing_content() {
    let mut device = Device::new(list);
    device.send(Msg::Scroll(ScrollTo::offset("list", 1500.0)));
    // The content grows by five rows: the position is kept, and there is
    // further to go.
    device.send(Msg::Count(5));
    assert_eq!(device.offset("list"), 1500.0);
    assert_eq!(device.ui.max_scroll_offset("list"), Some(2000.0));
    // A chat's rule: the line is added, and the end is asked for, in one
    // update. The end is that of the content with the new line in it.
    device.ui.send(Msg::Count(10));
    device.send(Msg::Scroll(ScrollTo::end("list")));
    assert_eq!(device.offset("list"), 2400.0);
    // The content shrinks under the position: it is brought back in.
    device.send(Msg::Count(0));
    assert_eq!(device.offset("list"), 1600.0);
}

#[test]
fn a_widget_scrolled_out_of_view_still_has_its_laid_out_rectangle() {
    let mut device = Device::new(list);
    // Far below the viewport and never painted.
    assert_eq!(
        device.rect(("row", 25_usize)),
        Rect::new(0.0, 2000.0, 360.0, 80.0)
    );
    assert_eq!(device.visible(("row", 25_usize)), None);
    assert_eq!(
        device.visible(("row", 3_usize)),
        Some(Rect::new(0.0, 240.0, 360.0, 80.0))
    );

    device.send(Msg::Scroll(ScrollTo::offset("list", 1030.0)));
    // Row 0 is now far above; row 12 (960..1040) shows its last 10 dp.
    assert_eq!(
        device.rect(("row", 0_usize)),
        Rect::new(0.0, -1030.0, 360.0, 80.0)
    );
    assert_eq!(device.visible(("row", 0_usize)), None);
    assert_eq!(device.rect(("row", 12_usize)).y, -70.0);
    assert_eq!(
        device.visible(("row", 12_usize)),
        Some(Rect::new(0.0, 0.0, 360.0, 10.0))
    );
    assert!(device.ui.rect_of("nothing").is_none());
}

fn field() -> TextInput<Msg> {
    TextInput::new("")
        .size(15.0)
        .line_height_dp(20.0)
        .caret_color(CARET)
        .on_input(Msg::Text)
}

fn low_scroll_box(_: &Lab, _: &Viewport) -> Element<Msg> {
    col![
        Spacer::new(),
        ScrollView::new(col![field().id("note"), Container::empty().height(300.0)].fill_width())
            .fill_width()
            .height(120.0)
            .id("box"),
    ]
    .fill()
    .into()
}

#[test]
fn a_field_its_scroll_view_cannot_bring_clear_of_the_keyboard_is_panned_clear() {
    let mut device = Device::new(low_scroll_box);
    assert_eq!(device.rect("note").y, 680.0);
    device.tap("note");
    assert!(device.ui.is_focused("note"));
    assert!(
        device.seen().focused,
        "the view is told which field has focus"
    );
    // The keyboard covers the lower 400 dp, the whole scroll box with it.
    // The field is at the top of the box already, so scrolling cannot
    // help; the screen is drawn shifted up until the field, with its
    // margin, is above the keyboard: 680 + 20 + 28 - 400.
    device.info = device.info.with_keyboard_inset(400.0 * SCALE);
    device.draw();
    assert_eq!(device.offset("box"), 0.0);
    assert_eq!(device.rect("note").y, 680.0 - 328.0);
    // The keyboard goes: so does the shift.
    device.info = device.info.with_keyboard_inset(0.0);
    device.draw();
    assert_eq!(device.rect("note").y, 680.0);
}

fn nested_form(_: &Lab, _: &Viewport) -> Element<Msg> {
    ScrollView::new(
        col![
            Container::empty().fill_width().height(600.0),
            ScrollView::new(
                col![
                    Container::empty().fill_width().height(500.0),
                    field().id("note"),
                    Container::empty().fill_width().height(500.0),
                ]
                .fill_width()
            )
            .fill_width()
            .height(300.0)
            .id("inner"),
            Container::empty().fill_width().height(900.0),
        ]
        .fill_width(),
    )
    .id("outer")
    .into()
}

#[test]
fn a_focused_field_is_revealed_through_every_scroll_view_around_it() {
    let mut device = Device::new(nested_form);
    // Bring the field's place into view by hand, and focus it.
    device.send(Msg::Scroll(ScrollTo::offset("inner", 400.0)));
    device.send(Msg::Scroll(ScrollTo::offset("outer", 300.0)));
    assert_eq!(device.rect("note").y, 600.0 + 500.0 - 400.0 - 300.0);
    device.tap("note");
    assert!(device.ui.is_focused("note"));
    // The keyboard takes the lower half. The inner view keeps the field
    // (already inside it) where it is; the outer one scrolls until the
    // field and its 28 dp are above the keyboard's top at 400.
    device.info = device.info.with_keyboard_inset(400.0 * SCALE);
    device.draw();
    assert_eq!(device.offset("inner"), 400.0);
    let field = device.rect("note");
    assert!(close(field.y + field.height + 28.0, 400.0), "{field:?}");
    assert!(device.offset("outer") > 300.0);
}

// -------------------------------------------------- bounded multi-line

fn note(lab: &Lab, _: &Viewport) -> Element<Msg> {
    col![
        Container::new(
            TextInput::new(&lab.text)
                .size(15.0)
                .line_height_dp(20.0)
                .caret_color(CARET)
                .multiline()
                .max_lines(3)
                .on_input(Msg::Text)
                .id("note"),
        )
        .fill_width()
        .padding(10.0)
        .border(1.0, INK)
        .input_frame()
        .id("frame"),
        Container::empty().fill_width().height(40.0).id("after"),
    ]
    .padding(20.0)
    .fill_width()
    .into()
}

impl Device {
    fn caret(&self) -> Rect {
        self.fill(CARET).0
    }

    fn type_lines(&mut self, lines: usize) {
        for line in 0..lines {
            if line > 0 {
                self.key(TextInputEvent::Enter);
            }
            self.key(TextInputEvent::Commit(format!("line {line}")));
        }
    }
}

#[test]
fn a_bounded_field_grows_to_its_limit_and_then_scrolls_to_follow_the_caret() {
    let mut device = Device::new(note);
    device.tap("frame");
    assert!(device.ui.is_focused("note"));
    let mut heights = Vec::new();
    for line in 0..6 {
        if line > 0 {
            device.key(TextInputEvent::Enter);
        }
        device.key(TextInputEvent::Commit(format!("line {line}")));
        heights.push(device.rect("note").height);
        // Nothing sticks out of anything, however much is typed.
        assert!(
            device.ui.overflows().is_empty(),
            "{:?}",
            device.ui.overflows()
        );
        // The caret is always inside the field.
        let (field, caret) = (device.rect("note"), device.caret());
        assert!(
            caret.y >= field.y - 0.5 && caret.y + caret.height <= field.y + field.height + 0.5,
            "line {line}: caret {caret:?} in {field:?}"
        );
    }
    assert_eq!(heights, [20.0, 40.0, 60.0, 60.0, 60.0, 60.0]);
    assert_eq!(device.ui.app().text.lines().count(), 6);
    // What follows the field stays where three lines put it.
    assert_eq!(device.rect("after").y, 20.0 + 10.0 + 60.0 + 10.0);
    // Six lines in a window of three: scrolled to the end.
    assert_eq!(device.offset("note"), 60.0);
    // The text is clipped to the field (plus the caret's width).
    let field = device.ui.rect_of("note").unwrap();
    assert!(device.frame.list.ops().iter().any(|op| matches!(
        *op,
        DrawOp::PushClip { rect, .. } if rect.y == field.y && rect.height == field.height
    )));
}

#[test]
fn tapping_the_first_visible_line_brings_the_one_before_it_in() {
    let mut device = Device::new(note);
    device.tap("frame");
    device.type_lines(6);
    assert_eq!(device.offset("note"), 60.0);
    // Lines 3, 4 and 5 show. A tap on line 3 puts the caret there and
    // scrolls one line of context in above it.
    let field = device.rect("note");
    device.tap_at(field.x + 30.0, field.y + 8.0);
    assert_eq!(device.ui.caret().map(|caret| caret / 7), Some(3));
    assert_eq!(device.offset("note"), 40.0);
    let caret = device.caret();
    assert!(
        close(caret.y, field.y + 20.0) || caret.y > field.y,
        "{caret:?}"
    );
}

#[test]
fn a_drag_scrolls_a_bounded_field_and_it_keeps_its_place_without_focus() {
    let mut device = Device::new(note);
    device.tap("frame");
    device.type_lines(6);
    let field = device.rect("note");
    let (x, y) = (field.x + 100.0, field.y + 10.0);
    // A finger moves down 10 dp (past the slop, where the drag is
    // anchored) and 30 more: the text follows it.
    device.touch(PointerPhase::Down, x, y);
    device.draw();
    device.touch(PointerPhase::Moved, x, y + 10.0);
    device.draw();
    device.touch(PointerPhase::Moved, x, y + 40.0);
    device.step(400);
    assert_eq!(device.offset("note"), 30.0);
    device.touch(PointerPhase::Up, x, y + 40.0);
    device.step(400);
    assert_eq!(device.offset("note"), 30.0);
    assert!(
        device.ui.is_focused("note"),
        "a drag is not a tap elsewhere"
    );

    // Focus goes; the field shows the same lines.
    device.ui.clear_focus();
    device.draw();
    assert_eq!(device.offset("note"), 30.0);
    assert_eq!(device.rect("note").height, 60.0);
    assert!(device.ui.overflows().is_empty());
}

#[test]
fn a_field_can_be_bounded_by_a_height_instead() {
    fn view(lab: &Lab, _: &Viewport) -> Element<Msg> {
        Container::new(
            TextInput::new(&lab.text)
                .size(15.0)
                .line_height_dp(20.0)
                .multiline()
                .max_height(50.0)
                .on_input(Msg::Text)
                .id("note"),
        )
        .width(300.0)
        .input_frame()
        .id("frame")
        .into()
    }
    let mut device = Device::new(view);
    device.tap("frame");
    device.type_lines(5);
    assert_eq!(device.rect("note").height, 50.0);
    assert_eq!(device.offset("note"), 50.0);
    assert!(device.ui.overflows().is_empty());
}

// ------------------------------------------------------------ animation

const TIME: Duration = Duration::from_millis(200);

fn knob(lab: &Lab, _: &Viewport) -> Element<Msg> {
    let on = lab.flags[0];
    col![
        Container::empty()
            .size(if lab.flags[1] { 120.0 } else { 40.0 }, 40.0)
            .background(if on { GREEN } else { RED })
            .offset(if on { 100.0 } else { 0.0 }, 0.0)
            .opacity(if lab.flags[2] { 0.25 } else { 1.0 })
            .id("knob")
            .animate(TIME, Easing::Linear),
        Container::empty().size(10.0, 10.0).id("after"),
    ]
    .cross_align(CrossAlign::Start)
    .into()
}

#[test]
fn a_change_is_shown_on_its_way_and_frames_stop_when_it_arrives() {
    let mut device = Device::new(knob);
    assert!(device.idle(), "nothing moves at rest");
    assert!(!device.ui.is_animating());

    // The view now says 100 to the right. This frame still shows the old
    // place, and asks for the next.
    assert!(device.send(Msg::Flag(0)));
    assert_eq!(device.rect("knob").x, 0.0);
    assert!(device.ui.is_animating());
    assert!(device.step(50));
    assert_eq!(device.rect("knob").x, 25.0);
    assert!(device.step(100));
    assert_eq!(device.rect("knob").x, 75.0);
    // The frame that shows the end asks for nothing more.
    assert!(!device.step(50));
    assert_eq!(device.rect("knob").x, 100.0);
    assert!(device.idle());
    assert!(!device.ui.is_animating());
    for _ in 0..3 {
        assert!(!device.step(1000), "and stays idle");
    }
}

#[test]
fn the_background_and_opacity_are_mixed_on_the_way() {
    let mut device = Device::new(knob);
    assert_eq!(device.fill(RED).1, 1.0);
    device.send(Msg::Flag(0));
    device.step(100);
    // Half-way between red and green, channel by channel.
    let mixed = device
        .frame
        .list
        .ops()
        .iter()
        .find_map(|op| match *op {
            DrawOp::Rect { color, .. } => Some(color),
            _ => None,
        })
        .expect("the knob");
    for (got, from, to) in [
        (mixed.r, RED.r, GREEN.r),
        (mixed.g, RED.g, GREEN.g),
        (mixed.b, RED.b, GREEN.b),
    ] {
        assert!((got - (from + to) / 2.0).abs() < 1.0e-4, "{mixed:?}");
    }
    device.step(100);
    assert_eq!(device.fill(GREEN).1, 1.0);

    // Opacity is multiplied into what is drawn: 1 to 0.25, half-way.
    device.send(Msg::Flag(2));
    device.step(100);
    assert!((device.fill(GREEN).1 - 0.625).abs() < 1.0e-4);
    device.step(100);
    assert_eq!(device.fill(GREEN).1, 0.25);
    assert!(device.idle());
}

#[test]
fn an_animated_size_moves_its_neighbours_frame_by_frame() {
    let mut device = Device::new(knob);
    device.send(Msg::Flag(1));
    device.step(100);
    assert_eq!(device.rect("knob").width, 80.0);
    assert_eq!(device.rect("after").y, 40.0);
    device.step(100);
    assert_eq!(device.rect("knob").width, 120.0);
    assert!(device.ui.overflows().is_empty());
}

#[test]
fn a_change_of_mind_starts_from_where_the_widget_has_got_to() {
    let mut device = Device::new(knob);
    device.send(Msg::Flag(0));
    device.step(100);
    assert_eq!(device.rect("knob").x, 50.0);
    // Back to the left, from 50, in another 200 ms.
    device.send(Msg::Flag(0));
    assert_eq!(device.rect("knob").x, 50.0);
    device.step(100);
    assert_eq!(device.rect("knob").x, 25.0);
    assert!(!device.step(100));
    assert_eq!(device.rect("knob").x, 0.0);
}

#[test]
fn with_reduced_motion_a_change_shows_at_once() {
    let mut device = Device::new(knob);
    device.ui.set_reduce_motion(true);
    assert!(!device.send(Msg::Flag(0)));
    assert_eq!(device.rect("knob").x, 100.0);
    assert!(device.idle());
}

#[test]
fn easing_shapes_the_way() {
    fn view(lab: &Lab, _: &Viewport) -> Element<Msg> {
        Container::new(
            Container::empty()
                .size(10.0, 10.0)
                .transform(Transform::translate(
                    if lab.flags[0] { 80.0 } else { 0.0 },
                    0.0,
                ))
                .id("dot")
                .animate(TIME, Easing::EaseOut),
        )
        .into()
    }
    let mut device = Device::new(view);
    device.send(Msg::Flag(0));
    device.step(100);
    // 1 - 0.5^3 of 80 dp.
    assert_eq!(device.rect("dot").x, 70.0);
    device.step(100);
    assert_eq!(device.rect("dot").x, 80.0);
}

fn banner(lab: &Lab, _: &Viewport) -> Element<Msg> {
    let slide = Transition::slide(0.0, -1.0)
        .with_fade()
        .duration(TIME)
        .easing(Easing::Linear);
    Stack::new()
        .push(
            Container::empty()
                .fill()
                .on_press(Msg::Tapped("page"))
                .id("page"),
        )
        .push_maybe(lab.flags[0].then(|| {
            Container::<Msg>::empty()
                .size(360.0, 60.0)
                .background(BLUE)
                .on_press(Msg::Tapped("banner"))
                .id("banner")
                .transition(slide)
        }))
        .fill()
        .into()
}

#[test]
fn a_widget_with_a_transition_enters_and_is_still_seen_leaving() {
    let mut device = Device::new(banner);
    assert!(device.idle());

    // It appears: one height above its place and transparent, on its way.
    assert!(device.send(Msg::Flag(0)));
    assert_eq!(device.rect("banner").y, -60.0);
    assert!(device.fills(BLUE).is_empty(), "wholly transparent");
    assert!(device.step(100));
    assert_eq!(device.rect("banner").y, -30.0);
    assert_eq!(device.fill(BLUE).1, 0.5);
    assert!(!device.step(100));
    assert_eq!(device.rect("banner").y, 0.0);
    assert_eq!(device.fill(BLUE).1, 1.0);
    assert!(device.idle());

    // It is gone from the view: it is still drawn, leaving the way it
    // came, but is no longer in the tree.
    assert!(device.send(Msg::Flag(0)));
    assert!(device.ui.rect_of("banner").is_none());
    assert_eq!(device.fill(BLUE), (Rect::new(0.0, 0.0, 360.0, 60.0), 1.0));
    assert!(device.step(100));
    assert_eq!(device.fill(BLUE), (Rect::new(0.0, -30.0, 360.0, 60.0), 0.5));
    // A tap where it is drawn goes to what is under it.
    device.tap_at(180.0, 10.0);
    assert_eq!(device.ui.app().log.last(), Some(&Msg::Tapped("page")));
    assert!(!device.step(100));
    assert!(device.fills(BLUE).is_empty());
    assert!(device.idle());
    assert!(!device.ui.is_animating());
    assert!(!device.step(5000));
}

#[test]
fn a_widget_that_comes_back_before_it_has_gone_returns_from_where_it_was() {
    let mut device = Device::new(banner);
    device.send(Msg::Flag(0));
    device.step(200);
    device.send(Msg::Flag(0));
    device.step(50);
    assert_eq!(device.fill(BLUE).0.y, -15.0);
    // Back in the view a quarter of the way out.
    device.send(Msg::Flag(0));
    assert_eq!(
        device.fills(BLUE).len(),
        1,
        "one banner, not it and its ghost"
    );
    assert_eq!(device.rect("banner").y, -15.0);
    // From three quarters there to all there in 200 ms: 0.85 after 80.
    device.step(80);
    assert_eq!(device.rect("banner").y, -9.0);
    assert!(!device.step(120));
    assert_eq!(device.rect("banner").y, 0.0);
    assert!(device.idle());
}

#[test]
fn with_reduced_motion_widgets_are_there_or_not() {
    let mut device = Device::new(banner);
    device.ui.set_reduce_motion(true);
    assert!(!device.send(Msg::Flag(0)));
    assert_eq!(device.fill(BLUE), (Rect::new(0.0, 0.0, 360.0, 60.0), 1.0));
    assert!(!device.send(Msg::Flag(0)));
    assert!(device.fills(BLUE).is_empty());
    assert!(device.idle());
}

// ------------------------------------------------------------ navigator

const SCREEN_COLORS: [Color; 3] = [RED, GREEN, BLUE];

fn screens(lab: &Lab, _: &Viewport) -> Element<Msg> {
    let screen = *lab.screens.current();
    let next = (screen + 1) % 3;
    Container::new(
        Button::new("Next")
            .on_press(Msg::Push(next))
            .id(("next", screen)),
    )
    .fill()
    .background(SCREEN_COLORS[screen as usize])
    .id(("screen", screen))
    .transition(lab.screens.transition().easing(Easing::Linear))
    .into()
}

#[test]
fn a_navigator_stacks_screens_and_back_pops_them() {
    let mut device = Device::new(screens);
    device.ui.set_reduce_motion(true);
    assert!(
        !device.ui.back(),
        "nothing to go back to: the system leaves"
    );
    device.tap(("next", 0_u8));
    device.tap(("next", 1_u8));
    assert_eq!(device.ui.app().screens.stack(), [0, 1, 2]);
    assert!(device.ui.back());
    device.draw();
    assert_eq!(device.ui.app().screens.stack(), [0, 1]);
    assert!(device.ui.rect_of(("screen", 1_u8)).is_some());
    device.send(Msg::Replace(2));
    assert_eq!(device.ui.app().screens.stack(), [0, 2]);
    assert!(device.ui.back());
    assert!(!device.ui.back());
}

#[test]
fn a_pushed_screen_slides_in_while_the_one_under_it_moves_away() {
    let mut device = Device::new(screens);
    assert!(device.idle(), "the first screen does not animate in");
    assert_eq!(device.fill(RED), (Rect::new(0.0, 0.0, 360.0, 800.0), 1.0));

    // Push: 300 ms. The new screen comes from one width to the right; the
    // old one, no longer in the view, goes a quarter width left and fades.
    assert!(device.send(Msg::Push(1)));
    assert_eq!(device.fill(GREEN).0.x, 360.0);
    assert_eq!(device.fill(RED), (Rect::new(0.0, 0.0, 360.0, 800.0), 1.0));
    assert!(device.step(150));
    assert_eq!(
        device.fill(GREEN),
        (Rect::new(180.0, 0.0, 360.0, 800.0), 1.0)
    );
    assert_eq!(device.fill(RED), (Rect::new(-45.0, 0.0, 360.0, 800.0), 0.5));
    // The screen that is leaving takes no taps; the one arriving does,
    // where it is drawn.
    assert!(device.ui.rect_of(("next", 0_u8)).is_none());
    assert!(!device.step(150));
    assert_eq!(device.fill(GREEN), (Rect::new(0.0, 0.0, 360.0, 800.0), 1.0));
    assert!(device.fills(RED).is_empty());
    assert!(device.idle());

    // Back: the same backwards. The popped screen leaves to the right,
    // told so by the screen that returns.
    assert!(device.ui.back());
    assert!(device.draw());
    assert!(device.step(150));
    assert_eq!(
        device.fill(GREEN),
        (Rect::new(180.0, 0.0, 360.0, 800.0), 1.0)
    );
    assert_eq!(device.fill(RED), (Rect::new(-45.0, 0.0, 360.0, 800.0), 0.5));
    assert!(!device.step(150));
    assert_eq!(device.fill(RED), (Rect::new(0.0, 0.0, 360.0, 800.0), 1.0));
    assert!(device.fills(GREEN).is_empty());
    assert!(device.idle());
}

#[test]
fn a_replaced_screen_cross_fades() {
    let mut device = Device::new(screens);
    assert!(device.send(Msg::Replace(2)));
    device.step(150);
    assert_eq!(device.fill(BLUE), (Rect::new(0.0, 0.0, 360.0, 800.0), 0.5));
    assert_eq!(device.fill(RED), (Rect::new(0.0, 0.0, 360.0, 800.0), 0.5));
    assert!(!device.step(150));
    assert!(device.fills(RED).is_empty());
    assert_eq!(device.fill(BLUE).1, 1.0);
}

// ------------------------------------------------------- on a real GPU

fn stage(lab: &Lab, _: &Viewport) -> Element<Msg> {
    let on = lab.flags[0];
    Stack::new()
        .push(
            Container::empty()
                .size(40.0, 40.0)
                .background(RED)
                .offset(if on { 200.0 } else { 20.0 }, 20.0)
                .id("mover")
                .animate(TIME, Easing::Linear),
        )
        .push(
            Container::empty()
                .size(if on { 200.0 } else { 40.0 }, 40.0)
                .background(BLUE)
                .offset(20.0, 100.0)
                .id("grower")
                .animate(TIME, Easing::Linear),
        )
        .push_maybe((!on).then(|| {
            Container::<Msg>::empty()
                .size(100.0, 40.0)
                .background(GREEN)
                .offset(20.0, 180.0)
                .id("leaver")
                .transition(Transition::fade().duration(TIME).easing(Easing::Linear))
        }))
        .fill()
        .into()
}

/// The frame at `millis` after the change, rendered. `None` without a GPU.
fn shot_at(millis: u64) -> Option<Screenshot> {
    let info = FrameInfo::new(SurfaceSize::new(360, 260));
    let mut ui = Ui::new(Lab::new(stage));
    let mut frame = Frame::for_app(&mut ui);
    // The first frames: the green box fades in, as anything with a
    // transition does when it appears. Then the change, at one second.
    let second = Duration::from_secs(1);
    for (at, change) in [(Duration::ZERO, false), (second, false), (second, true)] {
        if change {
            ui.send(Msg::Flag(0));
        }
        frame.begin();
        ui.frame(&info.with_elapsed(at), &mut frame);
    }
    let info = info.with_elapsed(second + Duration::from_millis(millis));
    let png = match screenshot_frame(&mut ui, &info) {
        Ok(png) => png,
        Err(GpuError::NoAdapter(reason)) => {
            eprintln!("skipping: no GPU adapter available ({reason})");
            return None;
        }
        Err(other) => panic!("render failed: {other}"),
    };
    if let Some(directory) = std::env::var_os("MORNINGSTAR_DUMP_DIR") {
        let path = std::path::Path::new(&directory).join(format!("ui_motion_{millis}.png"));
        std::fs::write(&path, &png).expect("dump");
    }
    Some(Screenshot::from_png(&png).expect("a PNG"))
}

/// A pixel's colour, comparable with a design colour within rounding.
#[derive(Clone, Copy, Debug)]
struct Rgb([u8; 3]);

impl PartialEq<[u8; 3]> for Rgb {
    fn eq(&self, other: &[u8; 3]) -> bool {
        (0..3).all(|channel| self.0[channel].abs_diff(other[channel]) <= 2)
    }
}

fn rgb(shot: &Screenshot, x: u32, y: u32) -> Rgb {
    let pixel = shot.pixel(x, y);
    Rgb([pixel[0], pixel[1], pixel[2]])
}

#[test]
fn intermediate_frames_as_pixels() {
    const WHITE: [u8; 3] = [255, 255, 255];
    const RED8: [u8; 3] = [0xD0, 0x02, 0x1B];
    const BLUE8: [u8; 3] = [0x0B, 0x5F, 0xFF];
    const GREEN8: [u8; 3] = [0x1E, 0x84, 0x16];
    let (Some(start), Some(middle), Some(end)) = (shot_at(0), shot_at(100), shot_at(200)) else {
        return;
    };

    // The red box travels from x 20 to 200 (40 wide, at y 20..60): at the
    // middle it covers 110..150 and neither end.
    assert_eq!(rgb(&start, 40, 40), RED8);
    assert_eq!(rgb(&start, 130, 40), WHITE);
    assert_eq!(rgb(&middle, 40, 40), WHITE);
    assert_eq!(rgb(&middle, 112, 40), RED8);
    assert_eq!(rgb(&middle, 148, 40), RED8);
    assert_eq!(rgb(&middle, 152, 40), WHITE);
    assert_eq!(rgb(&middle, 220, 40), WHITE);
    assert_eq!(rgb(&end, 220, 40), RED8);
    assert_eq!(rgb(&end, 130, 40), WHITE);

    // The blue box grows from 40 to 200 wide (from x 20, at y 100..140):
    // 120 wide at the middle.
    assert_eq!(rgb(&start, 58, 120), BLUE8);
    assert_eq!(rgb(&start, 62, 120), WHITE);
    assert_eq!(rgb(&middle, 138, 120), BLUE8);
    assert_eq!(rgb(&middle, 142, 120), WHITE);
    assert_eq!(rgb(&end, 218, 120), BLUE8);
    assert_eq!(rgb(&end, 222, 120), WHITE);

    // The green box has left the view and fades where it was (x 20..120,
    // y 180..220): solid, then between green and the white behind it,
    // then gone.
    assert_eq!(rgb(&start, 70, 200), GREEN8);
    let faded = rgb(&middle, 70, 200).0;
    for channel in 0..3 {
        assert!(
            faded[channel] > GREEN8[channel] + 20 && faded[channel] < 235,
            "half-faded green over white: {faded:?}"
        );
    }
    assert_eq!(rgb(&end, 70, 200), WHITE);
}

#[test]
fn asking_for_less_motion_mid_way_ends_what_was_moving() {
    let mut device = Device::new(knob);
    device.send(Msg::Flag(0));
    device.step(50);
    assert_eq!(device.rect("knob").x, 25.0);
    device.ui.set_reduce_motion(true);
    assert!(!device.step(10));
    assert_eq!(device.rect("knob").x, 100.0);
    assert!(device.idle());
}
