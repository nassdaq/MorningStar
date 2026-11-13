//! Pixel tests for layers (ADR 0028): `push_layer` draws a group and then
//! fades it as a whole, so what overlaps inside it does not show through;
//! layers nest, sit inside and around clips and transforms, and a frame
//! without them is rendered exactly as before they existed.
//!
//! Like the other pixel tests these run on the real GPU of the machine and
//! skip where there is none. The target is RGBA8 holding sRGB-encoded
//! values (ADR 0023); the colours here have components of 0 or 1, so
//! expected bytes are the blended value times 255.

use morningstar_core::{
    AtlasView, Color, DisplayList, ImageStore, Rect, Shadow, SurfaceSize, Transform,
};
use morningstar_gpu::{GpuError, OffscreenRenderer, RenderStats};

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const GREEN: Color = Color::rgb(0.0, 1.0, 0.0);
const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);

const K: [u8; 4] = [0, 0, 0, 255];
const SIZE: SurfaceSize = SurfaceSize::new(32, 32);

/// A one-texel, fully covered atlas: glyphs drawn from it are solid boxes.
const SOLID_ATLAS: AtlasView<'static> = AtlasView::new(1, 1, &[255], 1);

struct Frame {
    pixels: Vec<u8>,
    stats: RenderStats,
}

impl Frame {
    fn at(&self, x: u32, y: u32) -> [u8; 4] {
        let start = ((y * SIZE.width + x) * 4) as usize;
        self.pixels[start..start + 4].try_into().unwrap()
    }
}

fn renderer() -> Option<OffscreenRenderer> {
    match OffscreenRenderer::new() {
        Ok(renderer) => Some(renderer),
        Err(GpuError::NoAdapter(reason)) => {
            eprintln!("skipping: no GPU adapter available ({reason})");
            None
        }
        Err(other) => panic!("no offscreen renderer: {other}"),
    }
}

fn render_on(renderer: &mut OffscreenRenderer, list: &DisplayList) -> Frame {
    let pixels = renderer
        .render(SIZE, Color::BLACK, list, SOLID_ATLAS)
        .expect("offscreen render");
    Frame {
        pixels,
        stats: renderer.last_stats(),
    }
}

fn render(list: &DisplayList) -> Option<Frame> {
    Some(render_on(&mut renderer()?, list))
}

fn with_alpha(color: Color, alpha: f32) -> Color {
    Color::rgba(color.r, color.g, color.b, alpha)
}

/// Red on the left, blue on the right, overlapping in columns 12 to 19.
fn two_rects(list: &mut DisplayList, alpha: f32) {
    list.fill_rect(Rect::new(4.0, 4.0, 16.0, 24.0), with_alpha(RED, alpha));
    list.fill_rect(Rect::new(12.0, 4.0, 16.0, 24.0), with_alpha(BLUE, alpha));
}

#[test]
fn a_group_at_half_opacity_does_not_show_its_parts_through_each_other() {
    let mut list = DisplayList::new();
    list.push_layer(0.5);
    two_rects(&mut list, 1.0);
    list.pop_layer();
    let Some(frame) = render(&list) else { return };
    assert_eq!(frame.at(8, 16), [128, 0, 0, 255], "red alone, at half");
    assert_eq!(
        frame.at(16, 16),
        [0, 0, 128, 255],
        "the overlap: blue alone"
    );
    assert_eq!(frame.at(24, 16), [0, 0, 128, 255], "blue alone");
    assert_eq!(frame.at(2, 16), K);
    assert_eq!(frame.at(16, 2), K);
    assert_eq!(frame.at(30, 30), K);
    // Edges of the group are where the rectangles' are.
    assert_eq!(frame.at(4, 4), [128, 0, 0, 255]);
    assert_eq!(frame.at(3, 4), K);
    assert_eq!(frame.at(27, 27), [0, 0, 128, 255]);
    assert_eq!(frame.at(28, 27), K);

    // The same faded colour by colour: the red shows through the blue.
    let mut list = DisplayList::new();
    two_rects(&mut list, 0.5);
    let frame = render(&list).unwrap();
    assert_eq!(
        frame.at(16, 16),
        [64, 0, 128, 255],
        "doubled without a layer"
    );
}

#[test]
fn a_frame_without_layers_is_one_pass_and_one_draw_call_as_before() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut plain = DisplayList::new();
    two_rects(&mut plain, 1.0);
    let frame = render_on(&mut renderer, &plain);
    assert_eq!(
        (
            frame.stats.passes,
            frame.stats.draw_calls,
            frame.stats.instances
        ),
        (1, 1, 2)
    );
    assert_eq!(frame.stats.layer_textures, 0);
    assert_eq!(frame.stats.layer_texture_bytes, 0);

    // A layer at full opacity is no layer: the same passes, draw calls,
    // instances and bytes.
    let mut opaque = DisplayList::new();
    opaque.push_layer(1.0);
    two_rects(&mut opaque, 1.0);
    opaque.pop_layer();
    let same = render_on(&mut renderer, &opaque);
    assert_eq!(same.stats, frame.stats);
    assert_eq!(same.pixels, frame.pixels);

    // An empty frame is still one pass, which clears.
    let empty = render_on(&mut renderer, &DisplayList::new());
    assert_eq!((empty.stats.passes, empty.stats.draw_calls), (1, 0));
    assert_eq!(empty.at(16, 16), K);
}

#[test]
fn a_layer_costs_two_more_passes_one_instance_and_one_target_while_it_is_drawn() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut list = DisplayList::new();
    list.fill_rect(Rect::new(0.0, 0.0, 32.0, 2.0), GREEN);
    list.push_layer(0.5);
    two_rects(&mut list, 1.0);
    list.pop_layer();
    list.fill_rect(Rect::new(0.0, 30.0, 32.0, 2.0), GREEN);
    let frame = render_on(&mut renderer, &list);
    // Before the layer; the layer; its blend and what follows.
    assert_eq!(frame.stats.passes, 3);
    assert_eq!(frame.stats.draw_calls, 3);
    assert_eq!(frame.stats.instances, 5, "four rectangles and the blend");
    assert_eq!(frame.stats.layer_textures, 1);
    assert_eq!(frame.stats.layer_texture_bytes, 32 * 32 * 4);
    assert_eq!(frame.at(16, 1), [0, 255, 0, 255]);
    assert_eq!(frame.at(16, 16), [0, 0, 128, 255]);
    assert_eq!(frame.at(16, 31), [0, 255, 0, 255]);

    // Two layers side by side share the one target.
    let mut list = DisplayList::new();
    for x in [2.0, 18.0] {
        list.push_layer(0.5);
        list.fill_rect(Rect::new(x, 4.0, 12.0, 12.0), RED);
        list.fill_rect(Rect::new(x + 4.0, 8.0, 8.0, 12.0), BLUE);
        list.pop_layer();
    }
    let frame = render_on(&mut renderer, &list);
    assert_eq!(frame.stats.layer_textures, 1);
    assert_eq!(frame.stats.passes, 5, "clear, layer, blend, layer, blend");
    for x in [2, 18] {
        assert_eq!(frame.at(x + 1, 6), [128, 0, 0, 255]);
        assert_eq!(frame.at(x + 6, 12), [0, 0, 128, 255]);
        assert_eq!(frame.at(x + 6, 18), [0, 0, 128, 255]);
    }
    assert_eq!(frame.at(16, 12), K, "between them");

    // The next frame has no layer: the target is freed and the frame is
    // one pass again.
    let mut plain = DisplayList::new();
    two_rects(&mut plain, 1.0);
    let frame = render_on(&mut renderer, &plain);
    assert_eq!((frame.stats.passes, frame.stats.layer_textures), (1, 0));
}

#[test]
fn what_is_drawn_before_a_layer_is_under_it_and_what_follows_is_over_it() {
    let mut list = DisplayList::new();
    list.fill_rect(Rect::new(0.0, 0.0, 32.0, 32.0), GREEN);
    list.push_layer(0.5);
    two_rects(&mut list, 1.0);
    list.pop_layer();
    list.fill_rect(
        Rect::new(0.0, 14.0, 32.0, 4.0),
        with_alpha(Color::WHITE, 0.5),
    );
    let Some(frame) = render(&list) else { return };
    assert_eq!(frame.at(1, 8), [0, 255, 0, 255]);
    assert_eq!(frame.at(8, 8), [128, 128, 0, 255], "half red over green");
    assert_eq!(frame.at(16, 8), [0, 128, 128, 255], "half blue over green");
    assert_eq!(
        frame.at(16, 16),
        [128, 191, 191, 255],
        "then half white over that"
    );
    assert_eq!(frame.at(1, 16), [128, 255, 128, 255]);
}

#[test]
fn layers_nest() {
    let mut list = DisplayList::new();
    list.push_layer(0.5);
    list.fill_rect(Rect::new(0.0, 0.0, 32.0, 16.0), RED);
    list.push_layer(0.5);
    two_rects(&mut list, 1.0);
    list.pop_layer();
    list.fill_rect(Rect::new(0.0, 28.0, 32.0, 4.0), GREEN);
    list.pop_layer();
    let Some(frame) = render(&list) else { return };
    assert_eq!(frame.stats.layer_textures, 2);
    assert_eq!(frame.stats.passes, 5);
    // Outside the inner group: half of the outer's red.
    assert_eq!(frame.at(1, 8), [128, 0, 0, 255]);
    // The inner group is half over the outer's red, then all of it halved:
    // inner red on red stays red; inner blue is half blue over red.
    assert_eq!(frame.at(8, 8), [128, 0, 0, 255]);
    assert_eq!(frame.at(24, 8), [64, 0, 64, 255]);
    // Below the outer's red band: a quarter of the inner colours.
    assert_eq!(frame.at(8, 20), [64, 0, 0, 255]);
    assert_eq!(
        frame.at(16, 20),
        [0, 0, 64, 255],
        "no doubling at either level"
    );
    assert_eq!(
        frame.at(16, 30),
        [0, 128, 0, 255],
        "drawn after the inner group"
    );
}

#[test]
fn clips_cut_a_layer_from_outside_and_from_inside() {
    // A clip around the layer cuts the group.
    let mut list = DisplayList::new();
    list.push_clip(Rect::new(0.0, 0.0, 16.0, 16.0));
    list.push_layer(0.5);
    two_rects(&mut list, 1.0);
    list.pop_layer();
    list.pop_clip();
    let Some(frame) = render(&list) else { return };
    assert_eq!(frame.at(8, 8), [128, 0, 0, 255]);
    assert_eq!(frame.at(14, 8), [0, 0, 128, 255]);
    assert_eq!(frame.at(17, 8), K, "right of the clip");
    assert_eq!(frame.at(8, 17), K, "below the clip");

    // A rounded clip inside the layer cuts its content and ends with it.
    let mut list = DisplayList::new();
    list.push_layer(0.5);
    list.push_rounded_clip(Rect::new(4.0, 4.0, 24.0, 24.0), 12.0);
    two_rects(&mut list, 1.0);
    list.pop_clip();
    list.fill_rect(Rect::new(0.0, 0.0, 4.0, 4.0), GREEN);
    list.pop_layer();
    let frame = render(&list).unwrap();
    assert_eq!(frame.at(16, 16), [0, 0, 128, 255]);
    assert_eq!(frame.at(5, 5), K, "the rounded corner");
    assert_eq!(
        frame.at(1, 1),
        [0, 128, 0, 255],
        "after the clip, in the layer"
    );
}

#[test]
fn transforms_turn_a_layer_from_outside_and_from_inside() {
    // A quarter turn about the centre, around the layer: the group turns,
    // red to the top and blue to the bottom, and still does not double.
    let turn = Transform::rotate(90.0).about(morningstar_core::Point::new(16.0, 16.0));
    let mut outside = DisplayList::new();
    outside.push_transform(turn);
    outside.push_layer(0.5);
    two_rects(&mut outside, 1.0);
    outside.pop_layer();
    outside.pop_transform();
    let Some(frame) = render(&outside) else {
        return;
    };
    assert_eq!(frame.at(16, 8), [128, 0, 0, 255]);
    assert_eq!(frame.at(16, 16), [0, 0, 128, 255]);
    assert_eq!(frame.at(16, 24), [0, 0, 128, 255]);
    assert_eq!(frame.at(16, 2), K);
    assert_eq!(frame.at(2, 16), K);

    // The same turn inside the layer: the same picture.
    let mut inside = DisplayList::new();
    inside.push_layer(0.5);
    inside.push_transform(turn);
    two_rects(&mut inside, 1.0);
    inside.pop_transform();
    inside.pop_layer();
    assert_eq!(render(&inside).unwrap().pixels, frame.pixels);

    // A turn that is not a quarter: the group's slanted edges are
    // anti-aliased once, by the content, and the overlap is still blue.
    let mut list = DisplayList::new();
    list.push_layer(0.5);
    list.push_transform(Transform::rotate(30.0).about(morningstar_core::Point::new(16.0, 16.0)));
    two_rects(&mut list, 1.0);
    list.pop_transform();
    list.pop_layer();
    let frame = render(&list).unwrap();
    assert_eq!(frame.at(16, 16), [0, 0, 128, 255]);
    let edge = (0..32)
        .map(|x| frame.at(x, 16))
        .filter(|pixel| pixel[0] > 0 && pixel[0] < 128 && pixel[2] == 0)
        .count();
    assert!(edge >= 1, "a graded edge on the red side");
}

#[test]
fn every_kind_of_operation_can_be_in_a_layer() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut store = ImageStore::new();
    let image = store
        .add_rgba8(2, 2, &[0, 255, 0, 255].repeat(4))
        .expect("an image");
    let mut list = DisplayList::new();
    list.push_layer(0.5);
    list.draw_shadow(
        Rect::new(4.0, 4.0, 8.0, 8.0),
        0.0,
        Shadow {
            offset_x: 0.0,
            offset_y: 0.0,
            blur: 0.0,
            spread: 0.0,
            color: Color::WHITE,
            inset: false,
        },
    );
    list.fill_rounded_rect(Rect::new(4.0, 4.0, 8.0, 8.0), 2.0, RED);
    list.draw_glyph(
        Rect::new(6.0, 6.0, 4.0, 4.0),
        Rect::new(0.0, 0.0, 1.0, 1.0),
        BLUE,
    );
    list.draw_image(image, Rect::new(20.0, 4.0, 8.0, 8.0));
    list.fill_rect(Rect::new(22.0, 6.0, 4.0, 4.0), RED);
    list.pop_layer();
    let pixels = renderer
        .render_with_images(SIZE, Color::BLACK, &list, SOLID_ATLAS, store.view())
        .expect("offscreen render");
    let frame = Frame {
        pixels,
        stats: renderer.last_stats(),
    };
    assert_eq!(frame.at(5, 8), [128, 0, 0, 255], "the box over its shadow");
    assert_eq!(frame.at(8, 8), [0, 0, 128, 255], "the glyph over the box");
    assert_eq!(frame.at(21, 8), [0, 128, 0, 255], "the image");
    assert_eq!(
        frame.at(24, 8),
        [128, 0, 0, 255],
        "a rectangle over the image"
    );
    // The image inside the layer and the layer's own blend are different
    // textures: two draw calls for them, in two passes.
    assert_eq!(frame.stats.passes, 3);
}

#[test]
fn a_single_operation_in_a_layer_matches_its_colour_faded() {
    let mut layered = DisplayList::new();
    layered.push_layer(0.25);
    layered.fill_rounded_rect(Rect::new(4.0, 4.0, 24.0, 24.0), 8.0, RED);
    layered.pop_layer();
    let mut faded = DisplayList::new();
    faded.fill_rounded_rect(Rect::new(4.0, 4.0, 24.0, 24.0), 8.0, with_alpha(RED, 0.25));
    let Some(layered) = render(&layered) else {
        return;
    };
    let faded = render(&faded).unwrap();
    // The layer holds 8-bit values before it is faded: within one level.
    let worst = layered
        .pixels
        .iter()
        .zip(&faded.pixels)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    assert!(worst <= 1, "largest difference {worst}");
    assert_eq!(layered.at(16, 16), [64, 0, 0, 255]);
}

#[test]
fn invisible_and_unbalanced_layers_are_harmless() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    // Opacity 0 and NaN: nothing is drawn until the pop, at no cost, and
    // layers inside are skipped with it.
    for opacity in [0.0, -1.0, f32::NAN] {
        let mut list = DisplayList::new();
        list.push_layer(opacity);
        two_rects(&mut list, 1.0);
        list.push_layer(0.5);
        two_rects(&mut list, 1.0);
        list.pop_layer();
        list.pop_layer();
        list.fill_rect(Rect::new(0.0, 0.0, 2.0, 2.0), GREEN);
        let frame = render_on(&mut renderer, &list);
        assert_eq!(frame.at(16, 16), K, "{opacity}");
        assert_eq!(frame.at(1, 1), [0, 255, 0, 255]);
        assert_eq!(
            (
                frame.stats.passes,
                frame.stats.instances,
                frame.stats.layer_textures
            ),
            (1, 1, 0)
        );
    }

    // A pop without a push does nothing; a layer left open ends with the
    // list.
    let mut list = DisplayList::new();
    list.pop_layer();
    list.push_layer(0.5);
    two_rects(&mut list, 1.0);
    let frame = render_on(&mut renderer, &list);
    assert_eq!(frame.at(16, 16), [0, 0, 128, 255]);
    assert_eq!(frame.at(8, 16), [128, 0, 0, 255]);

    // A layer that draws nothing blends nothing.
    let mut list = DisplayList::new();
    list.fill_rect(Rect::new(0.0, 0.0, 2.0, 2.0), GREEN);
    list.push_layer(0.5);
    list.push_clip(Rect::new(0.0, 0.0, 0.0, 0.0));
    two_rects(&mut list, 1.0);
    list.pop_clip();
    list.pop_layer();
    list.push_layer(0.5);
    list.fill_rect(Rect::new(4.0, 4.0, 4.0, 4.0), RED);
    list.pop_layer();
    let frame = render_on(&mut renderer, &list);
    assert_eq!(frame.at(1, 1), [0, 255, 0, 255]);
    assert_eq!(frame.at(16, 16), K);
    assert_eq!(
        frame.at(5, 5),
        [128, 0, 0, 255],
        "the next layer starts clean"
    );
    assert_eq!(frame.stats.instances, 3);
}

#[test]
fn layers_nested_deeper_than_eight_are_drawn_without_their_own_opacity() {
    let mut list = DisplayList::new();
    for _ in 0..10 {
        list.push_layer(0.5);
    }
    list.fill_rect(Rect::new(0.0, 0.0, 32.0, 32.0), Color::WHITE);
    for _ in 0..10 {
        list.pop_layer();
    }
    let Some(frame) = render(&list) else { return };
    assert_eq!(frame.stats.layer_textures, 8);
    // 0.5 to the eighth, not the tenth: 1/256 of white.
    assert_eq!(frame.at(16, 16), [1, 1, 1, 255]);
}
