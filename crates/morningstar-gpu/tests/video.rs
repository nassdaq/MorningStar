//! Pixel tests for video (ADR 0039): a frame's three planes converted to
//! colour (BT.601, limited range), turned and mirrored as the frame says,
//! and drawn like an image: fitted, rounded, clipped, tinted, replaced by
//! the next frame and gone when the video stops.
//!
//! Like the other pixel tests these run on the real GPU of the machine and
//! skip where there is none. The reference for every expectation is
//! `VideoFrame::to_rgba8`, the conversion written out on the CPU.

use morningstar_core::{
    AtlasView, Color, DisplayList, ImageFit, ImageStore, Rect, Size, SurfaceSize, VideoFrame,
    VideoRotation, VideoTrack,
};
use morningstar_gpu::{GpuError, OffscreenRenderer, render_offscreen_with_images};

const TRACK: VideoTrack = VideoTrack(7);
const NOTHING: Color = Color::rgba(0.0, 0.0, 0.0, 0.0);
const ATLAS: AtlasView<'static> = AtlasView::new(1, 1, &[255], 1);

const RED: [u8; 3] = [220, 30, 40];
const GREEN: [u8; 3] = [30, 190, 60];
const BLUE: [u8; 3] = [20, 50, 220];
const WHITE: [u8; 3] = [255, 255, 255];

struct Shown {
    width: u32,
    pixels: Vec<u8>,
}

impl Shown {
    fn at(&self, x: u32, y: u32) -> [u8; 4] {
        let start = ((y * self.width + x) * 4) as usize;
        self.pixels[start..start + 4].try_into().unwrap()
    }

    #[track_caller]
    fn assert_near(&self, x: u32, y: u32, want: [u8; 3]) {
        let got = self.at(x, y);
        let close = got[..3].iter().zip(want).all(|(a, b)| a.abs_diff(b) <= 4);
        assert!(
            close && got[3] == 255,
            "({x}, {y}) is {got:?}, wanted about {want:?}"
        );
    }
}

fn skip_without_gpu<T>(result: Result<T, GpuError>) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(GpuError::NoAdapter(reason)) => {
            eprintln!("skipping: no GPU adapter available ({reason})");
            None
        }
        Err(other) => panic!("offscreen render failed: {other}"),
    }
}

fn render(size: (u32, u32), list: &DisplayList, store: &ImageStore) -> Option<Shown> {
    let pixels = skip_without_gpu(render_offscreen_with_images(
        SurfaceSize::new(size.0, size.1),
        NOTHING,
        list,
        ATLAS,
        store.view(),
    ))?;
    Some(Shown {
        width: size.0,
        pixels,
    })
}

/// A frame of four quadrants, each `half` pixels square: red and green on
/// top, blue and white below.
fn quadrants(half: u32) -> VideoFrame {
    let side = half * 2;
    let mut pixels = Vec::new();
    for y in 0..side {
        for x in 0..side {
            let colour = match (x < half, y < half) {
                (true, true) => RED,
                (false, true) => GREEN,
                (true, false) => BLUE,
                (false, false) => WHITE,
            };
            pixels.extend_from_slice(&[colour[0], colour[1], colour[2], 255]);
        }
    }
    VideoFrame::from_rgba8(side, side, &pixels).unwrap()
}

fn solid(width: u32, height: u32, colour: [u8; 3]) -> VideoFrame {
    let pixels: Vec<u8> = (0..width * height)
        .flat_map(|_| [colour[0], colour[1], colour[2], 255])
        .collect();
    VideoFrame::from_rgba8(width, height, &pixels).unwrap()
}

fn whole(store: &ImageStore, bounds: Rect, radius: f32, tint: Color) -> DisplayList {
    let mut list = DisplayList::new();
    let size = store.video_size(TRACK).unwrap_or(Size::new(1.0, 1.0));
    list.draw_video_fit(TRACK, size, bounds, ImageFit::Stretch, radius, tint);
    list
}

#[test]
fn the_planes_become_the_colours_they_encode() {
    let mut store = ImageStore::new();
    let frame = quadrants(8);
    let (_, _, reference) = frame.to_rgba8();
    store.set_video_frame(TRACK, frame);
    let list = whole(&store, Rect::new(0.0, 0.0, 16.0, 16.0), 0.0, Color::WHITE);
    let Some(shown) = render((16, 16), &list, &store) else {
        return;
    };
    // Away from the seams, where colour (stored at half size) is one value.
    for (x, y, want) in [(3, 3, RED), (12, 3, GREEN), (3, 12, BLUE), (12, 12, WHITE)] {
        shown.assert_near(x, y, want);
        let at = ((y * 16 + x) * 4) as usize;
        let cpu: [u8; 3] = reference[at..at + 3].try_into().unwrap();
        shown.assert_near(x, y, cpu);
    }
}

#[test]
fn black_and_white_are_the_ends_of_the_limited_range() {
    for (colour, want) in [
        ([0, 0, 0], [0, 0, 0]),
        (WHITE, WHITE),
        ([128, 128, 128], [128, 128, 128]),
    ] {
        let mut store = ImageStore::new();
        store.set_video_frame(TRACK, solid(4, 4, colour));
        let list = whole(&store, Rect::new(0.0, 0.0, 4.0, 4.0), 0.0, Color::WHITE);
        let Some(shown) = render((4, 4), &list, &store) else {
            return;
        };
        shown.assert_near(1, 1, want);
    }
}

#[test]
fn a_frame_is_turned_and_mirrored_as_it_says() {
    // Where the red quadrant (top left as stored) is shown.
    let cases = [
        (VideoRotation::None, false, (3, 3)),
        (VideoRotation::Clockwise90, false, (12, 3)),
        (VideoRotation::Half, false, (12, 12)),
        (VideoRotation::Clockwise270, false, (3, 12)),
        (VideoRotation::None, true, (12, 3)),
        (VideoRotation::Clockwise90, true, (3, 3)),
    ];
    for (rotation, mirrored, (x, y)) in cases {
        let mut store = ImageStore::new();
        let frame = quadrants(8).with_rotation(rotation).with_mirrored(mirrored);
        let (_, _, reference) = frame.to_rgba8();
        store.set_video_frame(TRACK, frame);
        let list = whole(&store, Rect::new(0.0, 0.0, 16.0, 16.0), 0.0, Color::WHITE);
        let Some(shown) = render((16, 16), &list, &store) else {
            return;
        };
        shown.assert_near(x, y, RED);
        // And every quadrant agrees with the conversion on the CPU.
        for (x, y) in [(3, 3), (12, 3), (3, 12), (12, 12)] {
            let at = ((y * 16 + x) * 4) as usize;
            shown.assert_near(x, y, reference[at..at + 3].try_into().unwrap());
        }
    }
}

#[test]
fn a_turned_frame_that_is_not_square_swaps_its_sides() {
    // 8 wide and 4 high: red left half, blue right half. A quarter turn
    // clockwise shows it 4 wide and 8 high, red on top.
    let mut pixels = Vec::new();
    for _ in 0..4 {
        for x in 0..8 {
            let colour = if x < 4 { RED } else { BLUE };
            pixels.extend_from_slice(&[colour[0], colour[1], colour[2], 255]);
        }
    }
    let frame = VideoFrame::from_rgba8(8, 4, &pixels)
        .unwrap()
        .with_rotation(VideoRotation::Clockwise90);
    let mut store = ImageStore::new();
    store.set_video_frame(TRACK, frame);
    assert_eq!(store.video_size(TRACK), Some(Size::new(4.0, 8.0)));
    let mut list = DisplayList::new();
    // Drawn at its own size, at (2, 0), in a taller target.
    list.draw_video_fit(
        TRACK,
        Size::new(4.0, 8.0),
        Rect::new(2.0, 0.0, 4.0, 8.0),
        ImageFit::Stretch,
        0.0,
        Color::WHITE,
    );
    let Some(shown) = render((8, 8), &list, &store) else {
        return;
    };
    shown.assert_near(3, 1, RED);
    shown.assert_near(4, 6, BLUE);
    assert_eq!(shown.at(0, 4), [0, 0, 0, 0], "nothing outside the picture");
    assert_eq!(shown.at(7, 4), [0, 0, 0, 0]);
}

#[test]
fn a_video_is_rounded_clipped_and_tinted_like_an_image() {
    let mut store = ImageStore::new();
    store.set_video_frame(TRACK, solid(16, 16, WHITE));
    let bounds = Rect::new(0.0, 0.0, 16.0, 16.0);

    let round = whole(&store, bounds, 8.0, Color::WHITE);
    let Some(shown) = render((16, 16), &round, &store) else {
        return;
    };
    assert_eq!(shown.at(0, 0), [0, 0, 0, 0], "the corner is cut away");
    shown.assert_near(8, 8, WHITE);

    let half = whole(&store, bounds, 0.0, Color::rgba(1.0, 1.0, 1.0, 0.5));
    let shown = render((16, 16), &half, &store).unwrap();
    let got = shown.at(8, 8);
    assert!(
        got[3].abs_diff(128) <= 2 && got[0].abs_diff(128) <= 2,
        "{got:?}"
    );

    let mut clipped = DisplayList::new();
    clipped.push_clip(Rect::new(0.0, 0.0, 8.0, 16.0));
    clipped.draw_video_fit(
        TRACK,
        Size::new(16.0, 16.0),
        bounds,
        ImageFit::Stretch,
        0.0,
        Color::WHITE,
    );
    clipped.pop_clip();
    let shown = render((16, 16), &clipped, &store).unwrap();
    shown.assert_near(3, 8, WHITE);
    assert_eq!(shown.at(12, 8), [0, 0, 0, 0]);
}

#[test]
fn a_cover_fit_crops_the_picture_and_other_things_keep_their_order() {
    // 16 x 8, left half red and right half blue, covering an 8 x 8 box:
    // the middle of the picture is shown, the seam in the centre.
    let mut pixels = Vec::new();
    for _ in 0..8 {
        for x in 0..16 {
            let colour = if x < 8 { RED } else { BLUE };
            pixels.extend_from_slice(&[colour[0], colour[1], colour[2], 255]);
        }
    }
    let mut store = ImageStore::new();
    store.set_video_frame(TRACK, VideoFrame::from_rgba8(16, 8, &pixels).unwrap());
    let mut list = DisplayList::new();
    list.fill_rect(Rect::new(0.0, 0.0, 16.0, 8.0), Color::rgb(0.0, 1.0, 0.0));
    list.draw_video_fit(
        TRACK,
        Size::new(16.0, 8.0),
        Rect::new(0.0, 0.0, 8.0, 8.0),
        ImageFit::Cover,
        0.0,
        Color::WHITE,
    );
    list.fill_rect(Rect::new(0.0, 0.0, 2.0, 2.0), Color::rgb(1.0, 1.0, 1.0));
    let Some(shown) = render((16, 8), &list, &store) else {
        return;
    };
    shown.assert_near(1, 6, RED);
    shown.assert_near(6, 6, BLUE);
    shown.assert_near(0, 0, WHITE);
    shown.assert_near(12, 4, [0, 255, 0]);
}

#[test]
fn the_next_frame_replaces_the_picture_and_a_stopped_video_draws_nothing() {
    let Some(mut renderer) = skip_without_gpu(OffscreenRenderer::new()) else {
        return;
    };
    let size = SurfaceSize::new(8, 8);
    let mut store = ImageStore::new();
    let bounds = Rect::new(0.0, 0.0, 8.0, 8.0);
    let shown = |store: &ImageStore, renderer: &mut OffscreenRenderer| {
        let list = whole(store, bounds, 0.0, Color::WHITE);
        let pixels = renderer
            .render_with_images(size, NOTHING, &list, ATLAS, store.view())
            .unwrap();
        Shown { width: 8, pixels }
    };

    // No frame yet: nothing.
    assert_eq!(shown(&store, &mut renderer).at(4, 4), [0, 0, 0, 0]);

    store.set_video_frame(TRACK, solid(8, 8, RED));
    shown(&store, &mut renderer).assert_near(4, 4, RED);
    assert_eq!(renderer.last_stats().video_conversions, 1);

    // The same frame again costs no conversion.
    shown(&store, &mut renderer).assert_near(4, 4, RED);
    assert_eq!(renderer.last_stats().video_conversions, 0);
    assert_eq!(renderer.last_stats().video_textures, 1);

    // A new frame, of another size.
    let before = store.set_video_frame(TRACK, solid(4, 6, BLUE));
    assert!(
        before.is_some(),
        "the frame before is handed back for reuse"
    );
    shown(&store, &mut renderer).assert_near(4, 4, BLUE);
    assert_eq!(renderer.last_stats().video_conversions, 1);

    assert!(store.remove_video(TRACK));
    assert_eq!(shown(&store, &mut renderer).at(4, 4), [0, 0, 0, 0]);
    assert_eq!(renderer.last_stats().video_textures, 0);
    assert!(!store.remove_video(TRACK));
}

#[test]
fn a_video_out_of_view_is_not_converted() {
    let Some(mut renderer) = skip_without_gpu(OffscreenRenderer::new()) else {
        return;
    };
    let mut store = ImageStore::new();
    store.set_video_frame(TRACK, solid(8, 8, RED));
    // Scrolled out of the box that clips it.
    let mut list = DisplayList::new();
    list.push_clip(Rect::new(0.0, 0.0, 8.0, 8.0));
    list.draw_video_fit(
        TRACK,
        Size::new(8.0, 8.0),
        Rect::new(100.0, 100.0, 8.0, 8.0),
        ImageFit::Stretch,
        0.0,
        Color::WHITE,
    );
    list.pop_clip();
    renderer
        .render_with_images(SurfaceSize::new(8, 8), NOTHING, &list, ATLAS, store.view())
        .unwrap();
    assert_eq!(renderer.last_stats().video_conversions, 0);
    assert_eq!(renderer.last_stats().video_textures, 0);
}
