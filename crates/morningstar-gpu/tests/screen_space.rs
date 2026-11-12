//! Pixel tests for `push_screen_space` (ADR 0029): operations inside it
//! are drawn in the surface's pixels whatever transform is open, clips
//! stay in force, and the transform is back afterwards.

use morningstar_core::{AtlasView, Color, DisplayList, Point, Rect, SurfaceSize, Transform};
use morningstar_gpu::{GpuError, OffscreenRenderer};

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);
const R: [u8; 4] = [255, 0, 0, 255];
const B: [u8; 4] = [0, 0, 255, 255];
const K: [u8; 4] = [0, 0, 0, 255];
const SIZE: SurfaceSize = SurfaceSize::new(32, 32);
const SOLID_ATLAS: AtlasView<'static> = AtlasView::new(1, 1, &[255], 1);

struct Frame {
    pixels: Vec<u8>,
    draw_calls: u32,
}

impl Frame {
    fn at(&self, x: u32, y: u32) -> [u8; 4] {
        let start = ((y * SIZE.width + x) * 4) as usize;
        self.pixels[start..start + 4].try_into().unwrap()
    }
}

fn render(list: &DisplayList) -> Option<Frame> {
    let mut renderer = match OffscreenRenderer::new() {
        Ok(renderer) => renderer,
        Err(GpuError::NoAdapter(reason)) => {
            eprintln!("skipping: no GPU adapter available ({reason})");
            return None;
        }
        Err(other) => panic!("no offscreen renderer: {other}"),
    };
    let pixels = renderer
        .render(SIZE, Color::BLACK, list, SOLID_ATLAS)
        .expect("offscreen render");
    Some(Frame {
        pixels,
        draw_calls: renderer.last_stats().draw_calls,
    })
}

fn turn() -> Transform {
    Transform::rotate(45.0).about(Point::new(16.0, 16.0))
}

#[test]
fn screen_space_ignores_the_open_transform_and_gives_it_back() {
    let mut list = DisplayList::new();
    list.push_transform(turn());
    list.push_screen_space();
    // A glyph and a rectangle where they are given, on the pixel grid.
    list.draw_glyph(
        Rect::new(2.0, 2.0, 6.0, 4.0),
        Rect::new(0.0, 0.0, 1.0, 1.0),
        RED,
    );
    list.fill_rect(Rect::new(24.0, 2.0, 4.0, 4.0), RED);
    list.pop_screen_space();
    // Back under the turn: a square about the centre becomes a diamond.
    list.fill_rect(Rect::new(10.0, 10.0, 12.0, 12.0), BLUE);
    list.pop_transform();
    let Some(frame) = render(&list) else { return };
    for (x, y) in [(2, 2), (7, 5), (24, 2), (27, 5)] {
        assert_eq!(frame.at(x, y), R, "{x},{y}");
    }
    for (x, y) in [(1, 2), (8, 5), (2, 6), (23, 2), (28, 5)] {
        assert_eq!(frame.at(x, y), K, "{x},{y}: a sharp, unturned edge");
    }
    assert_eq!(frame.at(16, 16), B);
    assert_eq!(frame.at(16, 9), B, "the diamond's upper corner");
    assert_eq!(
        frame.at(10, 10),
        K,
        "outside the diamond, inside the square"
    );
    assert_eq!(frame.draw_calls, 1, "it splits nothing");
}

#[test]
fn clips_stay_in_force_in_screen_space() {
    // A screen clip around everything: exact.
    let mut list = DisplayList::new();
    list.push_clip(Rect::new(0.0, 0.0, 16.0, 32.0));
    list.push_transform(turn());
    list.push_screen_space();
    list.fill_rect(Rect::new(8.0, 8.0, 16.0, 4.0), RED);
    list.pop_screen_space();
    list.pop_transform();
    list.pop_clip();
    let Some(frame) = render(&list) else { return };
    assert_eq!(frame.at(15, 9), R);
    assert_eq!(frame.at(16, 9), K);

    // A clip pushed under the turn lives in the turned space: in screen
    // space it acts as its bounding rectangle, never cutting more. The
    // 8 x 8 square about the centre turned by 45 degrees is a diamond
    // whose bounds are 16 +- 5.66.
    let mut list = DisplayList::new();
    list.push_transform(turn());
    list.push_clip(Rect::new(12.0, 12.0, 8.0, 8.0));
    list.push_screen_space();
    list.fill_rect(Rect::new(0.0, 0.0, 32.0, 32.0), RED);
    list.pop_screen_space();
    list.fill_rect(Rect::new(0.0, 0.0, 32.0, 32.0), BLUE);
    list.pop_clip();
    list.pop_transform();
    let frame = render(&list).unwrap();
    assert_eq!(frame.at(16, 16), B, "the exact diamond, drawn after");
    assert_eq!(
        frame.at(12, 12),
        R,
        "a corner of the bounds, outside the diamond"
    );
    assert_eq!(frame.at(20, 20), R);
    assert_eq!(frame.at(9, 16), K, "outside the bounds");
    assert_eq!(frame.at(16, 23), K);
}

#[test]
fn screen_space_nests_with_layers_and_survives_missing_pops() {
    let mut list = DisplayList::new();
    list.push_layer(0.5);
    list.push_transform(turn());
    list.push_screen_space();
    list.fill_rect(Rect::new(2.0, 2.0, 8.0, 8.0), RED);
    list.fill_rect(Rect::new(6.0, 6.0, 8.0, 8.0), BLUE);
    list.pop_screen_space();
    list.pop_transform();
    list.pop_layer();
    // A pop with nothing open, then a stretch never closed.
    list.pop_screen_space();
    list.push_screen_space();
    list.fill_rect(Rect::new(24.0, 24.0, 4.0, 4.0), RED);
    let Some(frame) = render(&list) else { return };
    assert_eq!(frame.at(3, 3), [128, 0, 0, 255]);
    assert_eq!(frame.at(8, 8), [0, 0, 128, 255], "grouped: no doubling");
    assert_eq!(frame.at(25, 25), R);

    // Under a transform that cannot be drawn through, nothing is drawn,
    // in screen space or out of it.
    let mut list = DisplayList::new();
    list.push_transform(Transform::scale(0.0, 0.0));
    list.push_screen_space();
    list.fill_rect(Rect::new(2.0, 2.0, 8.0, 8.0), RED);
    list.pop_screen_space();
    list.pop_transform();
    let frame = render(&list).unwrap();
    assert_eq!(frame.at(4, 4), K);
}
