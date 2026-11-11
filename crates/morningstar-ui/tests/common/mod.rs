//! A small application and a harness that drives [`Ui`] the way the engine
//! does: frames on demand, pointer events in physical pixels.

#![allow(dead_code)]

use std::time::Duration;

use morningstar_core::{DrawOp, Insets, Point, PointerEvent, PointerPhase, Rect, SurfaceSize};
use morningstar_runtime::{App, Frame, FrameInfo};
use morningstar_ui::prelude::*;

pub const REGULAR: &[u8] =
    include_bytes!("../../../../examples/hello-gpu/assets/fonts/NotoSans-Regular.ttf");
pub const BOLD: &[u8] =
    include_bytes!("../../../../examples/hello-gpu/assets/fonts/NotoSans-Bold.ttf");
pub const ARABIC: &[u8] =
    include_bytes!("../../../../examples/hello-gpu/assets/fonts/NotoSansArabic-Regular.ttf");

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Tap(u32),
    Toggle,
}

#[derive(Default)]
pub struct Model {
    pub flag: bool,
    pub count: u32,
    /// A 4 x 2 opaque white image, registered in `setup`.
    pub image: Option<Image>,
}

pub type View = fn(&Model) -> Element<Msg>;

pub struct Probe {
    pub view: View,
    pub model: Model,
    pub log: Vec<Msg>,
    pub theme: Theme,
}

impl Application for Probe {
    type Message = Msg;

    fn setup(&mut self, setup: &mut Setup<'_>) {
        setup.add_font(REGULAR);
        setup.add_font(BOLD);
        setup.add_font(ARABIC);
        self.model.image = setup.add_image_rgba8(4, 2, &[255; 32]).ok();
    }

    fn theme(&self) -> Theme {
        self.theme
    }

    fn view(&self) -> Element<Msg> {
        (self.view)(&self.model)
    }

    fn update(&mut self, message: Msg) {
        if message == Msg::Toggle {
            self.model.flag = !self.model.flag;
        }
        self.model.count += 1;
        self.log.push(message);
    }
}

/// 360 x 800 dp at 3 px per dp, no insets.
pub fn phone() -> FrameInfo {
    FrameInfo::new(SurfaceSize::new(1080, 2400)).with_scale(3.0)
}

pub fn phone_with_bars() -> FrameInfo {
    phone().with_insets(Insets::new(0.0, 96.0, 0.0, 60.0))
}

pub struct Harness {
    pub ui: Ui<Probe>,
    pub frame: Frame,
    pub info: FrameInfo,
    pub time: f32,
}

impl Harness {
    pub fn new(view: View) -> Self {
        Self::with(view, phone())
    }

    pub fn with(view: View, info: FrameInfo) -> Self {
        let mut ui = Ui::new(Probe {
            view,
            model: Model::default(),
            log: Vec::new(),
            theme: Theme::LIGHT,
        });
        let frame = Frame::for_app(&mut ui);
        let mut harness = Self {
            ui,
            frame,
            info,
            time: 0.0,
        };
        harness.draw();
        harness
    }

    /// Builds a frame at the current time; true if it asked for another.
    pub fn draw(&mut self) -> bool {
        self.frame.begin();
        let info = self.info.with_elapsed(Duration::from_secs_f32(self.time));
        self.ui.frame(&info, &mut self.frame);
        self.frame.redraw_requested()
    }

    /// Advances the clock and builds a frame.
    pub fn step(&mut self, seconds: f32) -> bool {
        self.time += seconds;
        self.draw()
    }

    /// Sends a pointer event at a position in dp.
    pub fn touch(&mut self, id: u64, phase: PointerPhase, x: f32, y: f32) {
        let scale = self.info.scale;
        self.ui.pointer(&PointerEvent {
            id,
            phase,
            position: Point::new(x * scale, y * scale),
        });
    }

    pub fn down(&mut self, x: f32, y: f32) {
        self.touch(1, PointerPhase::Down, x, y);
    }

    pub fn moved(&mut self, x: f32, y: f32) {
        self.touch(1, PointerPhase::Moved, x, y);
    }

    pub fn up(&mut self, x: f32, y: f32) {
        self.touch(1, PointerPhase::Up, x, y);
    }

    /// Down, frame, up, frame: a tap as the engine delivers it.
    pub fn tap(&mut self, x: f32, y: f32) {
        self.down(x, y);
        self.draw();
        self.up(x, y);
        self.draw();
    }

    pub fn log(&self) -> &[Msg] {
        &self.ui.app().log
    }

    /// The rectangle of the widget with this id, in dp.
    pub fn dp_rect(&self, id: &str) -> Rect {
        let rect = self
            .ui
            .rect_of(id)
            .unwrap_or_else(|| panic!("no widget with id {id:?}"));
        let scale = self.info.scale;
        Rect::new(
            rect.x / scale,
            rect.y / scale,
            rect.width / scale,
            rect.height / scale,
        )
    }

    pub fn px_rect(&self, id: &str) -> Rect {
        self.ui
            .rect_of(id)
            .unwrap_or_else(|| panic!("no widget with id {id:?}"))
    }

    pub fn ops(&self) -> &[DrawOp] {
        self.frame.list.ops()
    }

    /// Solid rectangles of the frame with exactly this colour.
    pub fn rects_with_color(&self, color: Color) -> Vec<Rect> {
        self.ops()
            .iter()
            .filter_map(|op| match *op {
                DrawOp::Rect { rect, color: c, .. } if c == color => Some(rect),
                DrawOp::BorderedRect { rect, fill, .. } if fill == color => Some(rect),
                _ => None,
            })
            .collect()
    }

    pub fn glyph_count(&self) -> usize {
        self.ops()
            .iter()
            .filter(|op| matches!(op, DrawOp::Glyph { .. }))
            .count()
    }
}

pub fn close(a: f32, b: f32, tolerance: f32) -> bool {
    (a - b).abs() <= tolerance
}
