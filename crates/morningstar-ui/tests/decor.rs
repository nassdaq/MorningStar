//! Radial gradients, blur, several shadows and per-corner radii on a box
//! (ADR 0033 to 0036), and scroll views that scroll sideways (ADR 0037):
//! what the UI layer puts in the display list for them, and how a finger
//! moves them.

use std::hash::Hash;
use std::time::Duration;

use morningstar_core::{DrawOp, Point, PointerEvent, PointerPhase, Rect, SurfaceSize};
use morningstar_runtime::{App, Frame, FrameInfo};
use morningstar_ui::prelude::*;

const REGULAR: &[u8] =
    include_bytes!("../../../examples/hello-gpu/assets/fonts/NotoSans-Regular.ttf");

const WHITE: Color = hex(0xFFFFFF);
const PINK: Color = hex(0xF06CBC);
const INK: Color = hex(0x1D1830);
const SCALE: f32 = 2.0;

#[derive(Clone, Debug, PartialEq)]
enum Msg {
    Chip(usize),
}

struct Lab {
    view: fn() -> Element<Msg>,
    tapped: Vec<usize>,
}

impl Application for Lab {
    type Message = Msg;

    fn setup(&mut self, setup: &mut Setup<'_>) {
        setup.add_font(REGULAR);
    }

    fn view(&self) -> Element<Msg> {
        (self.view)()
    }

    fn update(&mut self, message: Msg) {
        let Msg::Chip(index) = message;
        self.tapped.push(index);
    }
}

struct Device {
    ui: Ui<Lab>,
    frame: Frame,
    info: FrameInfo,
    time: Duration,
}

impl Device {
    /// A 200 x 300 dp screen at 2 px per dp.
    fn new(view: fn() -> Element<Msg>) -> Self {
        let mut ui = Ui::new(Lab {
            view,
            tapped: Vec::new(),
        });
        let frame = Frame::for_app(&mut ui);
        let info = FrameInfo::new(SurfaceSize::new(400, 600)).with_scale(SCALE);
        let mut device = Self {
            ui,
            frame,
            info,
            time: Duration::ZERO,
        };
        device.draw();
        device
    }

    fn draw(&mut self) {
        self.time += Duration::from_millis(16);
        self.frame.begin();
        self.ui
            .frame(&self.info.with_elapsed(self.time), &mut self.frame);
    }

    fn ops(&self) -> &[DrawOp] {
        self.frame.list.ops()
    }

    fn touch(&mut self, phase: PointerPhase, x: f32, y: f32) {
        self.ui.pointer(&PointerEvent {
            id: 1,
            phase,
            position: Point::new(x * SCALE, y * SCALE),
        });
        self.draw();
    }

    /// A finger down at `from`, moved to `to` in steps, and lifted after
    /// resting (so nothing is flung).
    fn drag(&mut self, from: (f32, f32), to: (f32, f32)) {
        self.touch(PointerPhase::Down, from.0, from.1);
        for step in 1..=8 {
            let t = step as f32 / 8.0;
            self.touch(
                PointerPhase::Moved,
                from.0 + (to.0 - from.0) * t,
                from.1 + (to.1 - from.1) * t,
            );
        }
        self.time += Duration::from_millis(400);
        self.draw();
        self.touch(PointerPhase::Up, to.0, to.1);
    }

    fn tap(&mut self, id: impl Hash) {
        let rect = self.rect(id);
        let (x, y) = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
        self.touch(PointerPhase::Down, x, y);
        self.touch(PointerPhase::Up, x, y);
    }

    /// Where a widget was laid out, in dp.
    fn rect(&self, id: impl Hash) -> Rect {
        let rect = self.ui.rect_of(id).expect("the widget is in the tree");
        Rect::new(
            rect.x / SCALE,
            rect.y / SCALE,
            rect.width / SCALE,
            rect.height / SCALE,
        )
    }
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

// ------------------------------------------------------ radial gradients

fn orb() -> Element<Msg> {
    // background: radial-gradient(circle at 30% 22%, rgba(255,255,255,.95) 0%, transparent 24%),
    //             radial-gradient(circle at 50% 35%, pink 0%, ink 100%), white
    Container::new(
        Container::empty()
            .size(100.0, 100.0)
            .radius(50.0)
            .background(WHITE)
            .radials([
                Radial::circle()
                    .at(0.3, 0.22)
                    .stop(0.0, with_alpha(WHITE, 0.95))
                    .stop(0.24, with_alpha(WHITE, 0.0)),
                Radial::circle()
                    .at(0.5, 0.35)
                    .stop(0.0, PINK)
                    .stop(1.0, INK),
            ])
            .id("orb"),
    )
    .padding(20.0)
    .into()
}

#[test]
fn css_layers_are_painted_from_the_last_up_with_css_geometry() {
    let device = Device::new(orb);
    let ops = device.ops();
    // The fill, the lower gradient (one segment and what lies beyond its
    // last stop), then the upper one (its transparent end is not drawn).
    let [
        DrawOp::Rect { rect, .. },
        DrawOp::RadialGradientRect {
            segment: body,
            corner_radius,
            blur,
            ..
        },
        DrawOp::RadialGradientRect {
            segment: beyond, ..
        },
        DrawOp::RadialGradientRect { segment: shine, .. },
    ] = ops
    else {
        panic!("unexpected display list: {ops:#?}");
    };
    assert_eq!(*rect, Rect::new(40.0, 40.0, 200.0, 200.0));
    assert_eq!((*corner_radius, *blur), (100.0, 0.0));
    // `circle at 50% 35%`, farthest corner: 100 px across, 130 px down.
    assert_eq!(body.center, Point::new(140.0, 110.0));
    assert!(close(body.radius_x, 100.0_f32.hypot(130.0)));
    assert_eq!(body.radius_x, body.radius_y);
    assert_eq!((body.from, body.to), (0.0, 1.0));
    assert_eq!((beyond.from, beyond.to), (1.0, f32::INFINITY));
    assert_eq!(beyond.from_color, beyond.to_color);
    // `circle at 30% 22%`: the farthest corner is the bottom right.
    assert_eq!(shine.center, Point::new(100.0, 84.0));
    assert!(close(shine.radius_x, 140.0_f32.hypot(156.0)));
    assert_eq!((shine.from, shine.to), (0.0, 0.24));
    assert_eq!(shine.to_color.a, 0.0);
}

fn extents() -> Element<Msg> {
    let stops = |radial: Radial| radial.stop(0.0, PINK).stop(1.0, INK);
    let tile = |radial: Radial| {
        Container::<Msg>::empty()
            .size(100.0, 40.0)
            .radial(stops(radial))
    };
    col![
        // The default ellipse reaches the corners: the sides times root 2.
        tile(Radial::ellipse()),
        tile(
            Radial::ellipse()
                .at(0.25, 0.5)
                .extent(RadialExtent::ClosestSide)
        ),
        tile(
            Radial::circle()
                .at(0.25, 0.5)
                .extent(RadialExtent::ClosestSide)
        ),
        tile(
            Radial::circle()
                .at(0.25, 0.5)
                .extent(RadialExtent::FarthestSide)
        ),
        tile(Radial::circle().extent(RadialExtent::ClosestCorner)),
        tile(Radial::ellipse().extent(RadialExtent::Radii(30.0, 10.0))),
    ]
    .into()
}

#[test]
fn every_css_size_keyword_gives_its_radii() {
    let device = Device::new(extents);
    let radii: Vec<(f32, f32)> = device
        .ops()
        .iter()
        .filter_map(|op| match op {
            DrawOp::RadialGradientRect { segment, .. } if segment.from == 0.0 => {
                Some((segment.radius_x, segment.radius_y))
            }
            _ => None,
        })
        .collect();
    let root2 = std::f32::consts::SQRT_2;
    let expected = [
        (100.0 * root2, 40.0 * root2),
        (50.0, 40.0),
        (40.0, 40.0),
        (150.0, 150.0),
        (100.0_f32.hypot(40.0), 100.0_f32.hypot(40.0)),
        (60.0, 20.0),
    ];
    assert_eq!(radii.len(), expected.len());
    for (got, want) in radii.iter().zip(expected) {
        assert!(
            close(got.0, want.0) && close(got.1, want.1),
            "{got:?} for {want:?}"
        );
    }
}

// --------------------------------------------------- shadows, blur, radii

fn decorated() -> Element<Msg> {
    col![
        // box-shadow: inset 0 -4px 8px white, inset 0 0 0 1px ink, 0 6px 12px pink
        Container::new(text("x"))
            .size(60.0, 60.0)
            .radius(30.0)
            .background(INK)
            .border(1.0, WHITE)
            .shadows([
                Shadow::inset(0.0, -4.0, 8.0, WHITE),
                Shadow::inset(0.0, 0.0, 0.0, INK).with_spread(1.0),
                Shadow::new(0.0, 6.0, 12.0, PINK),
            ]),
        // filter: blur(5px) on a solid box and on a gradient.
        Container::empty()
            .size(60.0, 20.0)
            .radius(10.0)
            .background(PINK)
            .blur(5.0),
        Container::empty()
            .size(60.0, 20.0)
            .radial(
                Radial::ellipse()
                    .stop(0.0, PINK)
                    .stop(0.7, with_alpha(PINK, 0.0))
            )
            .blur(5.0),
        // border-radius: 20px 20px 6px 20px
        Container::empty()
            .size(60.0, 50.0)
            .radii(20.0, 20.0, 6.0, 20.0)
            .background(PINK)
            .border(1.0, INK)
            .shadow(Shadow::new(0.0, 2.0, 4.0, INK)),
    ]
    .gap(30.0)
    .padding(20.0)
    .into()
}

#[test]
fn shadows_go_under_and_over_the_fill_in_css_order_and_the_border_last() {
    let device = Device::new(decorated);
    let ops = device.ops();
    let kinds: Vec<&str> = ops
        .iter()
        .map(|op| match op {
            DrawOp::Shadow { .. } => "shadow",
            DrawOp::InsetShadow { .. } => "inset",
            DrawOp::Rect { .. } => "rect",
            DrawOp::BorderedRect { .. } => "bordered",
            DrawOp::CornersRect { .. } => "corners",
            DrawOp::RadialGradientRect { .. } => "radial",
            DrawOp::Glyph { .. } => "glyph",
            _ => "other",
        })
        .collect();
    assert_eq!(
        kinds,
        [
            // The first box: its drop shadow, its fill, the inner shadows
            // from the last listed up, then its border and its text.
            "shadow", "rect", "inset", "inset", "bordered", "glyph",
            // A blurred solid box is its own shadow.
            "shadow", // A blurred gradient.
            "radial", // The box with a radius per corner, on its shadow.
            "shadow", "corners",
        ]
    );
    // In pixels, the last listed inner shadow first.
    let DrawOp::InsetShadow { shadow: ring, .. } = ops[2] else {
        unreachable!()
    };
    assert_eq!((ring.blur, ring.spread), (0.0, 2.0));
    let DrawOp::InsetShadow { shadow: light, .. } = ops[3] else {
        unreachable!()
    };
    assert_eq!(
        (light.offset_y, light.blur, light.inset),
        (-8.0, 16.0, true)
    );
    // The border is a stroke alone: the fill was drawn under the shadows.
    let DrawOp::BorderedRect { fill, .. } = ops[4] else {
        unreachable!()
    };
    assert_eq!(fill.a, 0.0);
    // blur(5px) at 2 px per dp is a sigma of 10 px: a shadow's blur
    // radius of 20, or the gradient's own blur.
    let DrawOp::Shadow { shadow, .. } = ops[6] else {
        unreachable!()
    };
    assert_eq!(
        (shadow.blur, shadow.offset_x, shadow.offset_y),
        (20.0, 0.0, 0.0)
    );
    let DrawOp::RadialGradientRect { blur, .. } = ops[7] else {
        unreachable!()
    };
    assert_eq!(blur, 10.0);
    // The radii, in pixels, and the shadow with the largest of them.
    let (DrawOp::Shadow { corner_radius, .. }, DrawOp::CornersRect { radii, border, .. }) =
        (ops[8], ops[9])
    else {
        unreachable!()
    };
    assert_eq!(corner_radius, 40.0);
    assert_eq!(
        (
            radii.top_left,
            radii.top_right,
            radii.bottom_right,
            radii.bottom_left
        ),
        (40.0, 40.0, 12.0, 40.0)
    );
    assert_eq!(border.width, 2.0);
}

// ----------------------------------------------------- sideways scrolling

fn chip(index: usize) -> Container<Msg> {
    Container::new(text(format!("Category {index}")).size(13.0))
        .padding_xy(16.0, 12.0)
        .radius(22.0)
        .background(PINK)
        .on_press(Msg::Chip(index))
        .id(("chip", index))
}

fn chips() -> Element<Msg> {
    col![
        ScrollView::horizontal(
            Row::new()
                .extend((0..6).map(chip))
                .gap(8.0)
                .padding_xy(20.0, 0.0)
                .id("row"),
        )
        .id("chips"),
        text("Below").id("below"),
    ]
    .gap(10.0)
    .into()
}

#[test]
fn a_sideways_scroll_view_lays_its_content_out_as_wide_as_it_likes() {
    let device = Device::new(chips);
    let viewport = device.rect("chips");
    let row = device.rect("row");
    assert_eq!(viewport.width, 200.0);
    // Six chips on one line, none squeezed or wrapped: all the same
    // height, and the row is as tall as one of them.
    let first = device.rect(("chip", 0usize));
    let last = device.rect(("chip", 5usize));
    assert_eq!(first.x, 20.0);
    assert_eq!(first.height, last.height);
    assert_eq!(viewport.height, first.height);
    assert!((first.width - last.width).abs() < 2.0, "{first:?} {last:?}");
    assert!(row.width > 400.0, "{row:?}");
    assert!((last.x + last.width + 20.0 - row.width).abs() <= 0.5);
    // What follows is under it, not beside it.
    assert_eq!(device.rect("below").y, viewport.height + 10.0);
    // It is not a layout fault, and it can scroll by what does not fit.
    assert_eq!(device.ui.overflows(), []);
    let max = device.ui.max_scroll_offset("chips").unwrap();
    assert!((max - (row.width - 200.0)).abs() < 1.0, "{max}");
    assert_eq!(device.ui.scroll_offset("chips"), Some(0.0));
    // The last chip is laid out off screen and nothing of it shows.
    assert!(last.x > 200.0);
    assert_eq!(device.ui.visible_rect_of(("chip", 5usize)), None);
}

#[test]
fn a_sideways_drag_scrolls_it_and_a_tap_still_presses() {
    let mut device = Device::new(chips);
    let max = device.ui.max_scroll_offset("chips").unwrap();
    device.drag((150.0, 20.0), (50.0, 22.0));
    // The first 8 dp of travel are the slop; the rest moves the row.
    let offset = device.ui.scroll_offset("chips").unwrap();
    assert!((85.0..=100.0).contains(&offset), "{offset}");
    assert!((device.rect(("chip", 0usize)).x - (20.0 - offset)).abs() <= 0.5);
    assert_eq!(device.ui.app().tapped, [], "a drag is not a tap");
    // Further than there is: it stops at the end, the last chip 20 dp in.
    device.drag((180.0, 20.0), (-400.0, 20.0));
    let offset = device.ui.scroll_offset("chips").unwrap();
    assert!(close(offset, max), "{offset} of {max}");
    let last = device.rect(("chip", 5usize));
    assert!((last.x + last.width - 180.0).abs() <= 0.5, "{last:?}");
    device.tap(("chip", 5usize));
    assert_eq!(device.ui.app().tapped, [5]);
    // And back.
    device.drag((20.0, 20.0), (600.0, 20.0));
    assert_eq!(device.ui.scroll_offset("chips"), Some(0.0));
    // A drag up or down on it moves nothing: it has no room that way.
    device.drag((100.0, 20.0), (104.0, 200.0));
    assert_eq!(device.ui.scroll_offset("chips"), Some(0.0));
    // The application can ask, as for any scroll view.
    device.ui.scroll_to(ScrollTo::end("chips"));
    device.draw();
    assert!(close(device.ui.scroll_offset("chips").unwrap(), max));
}

fn nested() -> Element<Msg> {
    ScrollView::new(
        col![
            ScrollView::horizontal(Row::new().extend((0..6).map(chip)).gap(8.0)).id("chips"),
            Container::empty()
                .fill_width()
                .height(700.0)
                .background(INK),
        ]
        .gap(10.0),
    )
    .id("page")
    .into()
}

#[test]
fn inside_a_page_that_scrolls_a_drag_goes_to_whichever_runs_its_way() {
    let mut device = Device::new(nested);
    // Down the page, starting on the chips.
    device.drag((100.0, 30.0), (104.0, -60.0));
    let page = device.ui.scroll_offset("page").unwrap();
    assert!(page > 70.0, "{page}");
    assert_eq!(device.ui.scroll_offset("chips"), Some(0.0));
    // Back up, then along the chips: the page stays.
    device.drag((100.0, 100.0), (100.0, 400.0));
    assert_eq!(device.ui.scroll_offset("page"), Some(0.0));
    device.drag((150.0, 20.0), (40.0, 26.0));
    assert!(device.ui.scroll_offset("chips").unwrap() > 90.0);
    assert_eq!(device.ui.scroll_offset("page"), Some(0.0));
}
