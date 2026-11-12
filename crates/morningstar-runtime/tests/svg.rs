//! Loaded SVG files through the application-facing API (ADR 0027): the
//! same picture path and cache as a `VectorPicture`, and one-colour files
//! as masks in the glyph atlas.

use morningstar_core::{Color, DrawOp, Rect, SurfaceSize};
use morningstar_gpu::{GpuError, render_offscreen_with_images};
use morningstar_runtime::{Frame, Layer, SvgPicture, VectorPicture};

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);
const SIZE: SurfaceSize = SurfaceSize::new(32, 32);
const WHOLE: Rect = Rect::new(0.0, 0.0, 32.0, 32.0);

fn render(frame: &Frame) -> Option<Vec<u8>> {
    match render_offscreen_with_images(
        SIZE,
        frame.clear,
        &frame.list,
        frame.atlas(),
        frame.images(),
    ) {
        Ok(pixels) => Some(pixels),
        Err(GpuError::NoAdapter(reason)) => {
            eprintln!("skipping: no GPU adapter available ({reason})");
            None
        }
        Err(other) => panic!("offscreen render failed: {other}"),
    }
}

fn at(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let start = ((y * SIZE.width + x) * 4) as usize;
    pixels[start..start + 4].try_into().unwrap()
}

fn two_tone() -> SvgPicture {
    SvgPicture::parse(
        r#"<svg viewBox="0 0 16 16">
             <rect width="8" height="16" fill="red"/>
             <rect x="8" width="8" height="16" fill="currentColor"/>
           </svg>"#,
    )
    .unwrap()
}

fn outline() -> SvgPicture {
    SvgPicture::parse(
        r#"<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="4">
             <path d="M0 8h16"/>
           </svg>"#,
    )
    .unwrap()
}

#[test]
fn a_loaded_svg_is_drawn_and_cached_like_a_vector_picture() {
    static LAYERS: [Layer; 2] = [
        Layer::rect(0.0, 0.0, 8.0, 16.0).fill(RED),
        Layer::rect(8.0, 0.0, 8.0, 16.0).fill(BLUE),
    ];
    let by_hand = VectorPicture::new(16.0, 16.0, &LAYERS);
    let loaded = two_tone().with_current_color(BLUE);

    let mut frame = Frame::new();
    frame.begin();
    frame.draw_picture(&loaded, WHOLE, 0.0, Color::WHITE);
    assert_eq!(frame.picture_cache_len(), 1);
    assert_eq!(frame.picture_cache_bytes(), 32 * 32 * 4);
    let ops: Vec<_> = frame.list.ops().to_vec();
    assert!(matches!(ops[..], [DrawOp::Image { rect, .. }] if rect == WHOLE));
    let Some(svg) = render(&frame) else { return };
    assert_eq!(at(&svg, 4, 16), [255, 0, 0, 255]);
    assert_eq!(at(&svg, 28, 16), [0, 0, 255, 255]);

    // The same pixels as the picture written as layers.
    let mut other = Frame::new();
    other.begin();
    other.draw_picture(&by_hand, WHOLE, 0.0, Color::WHITE);
    assert_eq!(render(&other).unwrap(), svg);

    // Drawn again, and a clone of the handle: no second rasterisation.
    frame.begin();
    frame.draw_picture(&loaded, WHOLE, 0.0, Color::WHITE);
    frame.draw_picture(&loaded.clone(), WHOLE, 0.0, Color::WHITE);
    assert_eq!(frame.picture_cache_len(), 1);
    // Another current colour is another picture; another size too.
    frame.draw_picture(&loaded.with_current_color(RED), WHOLE, 0.0, Color::WHITE);
    frame.draw_picture(&loaded, Rect::new(0.0, 0.0, 16.0, 16.0), 0.0, Color::WHITE);
    assert_eq!(frame.picture_cache_len(), 3);
}

#[test]
fn a_single_ink_svg_icon_is_a_glyph_in_any_colour() {
    let icon = outline();
    assert!(icon.is_single_ink());
    let mut frame = Frame::new();
    frame.begin();
    frame.draw_svg_icon(&icon, WHOLE, RED);
    frame.draw_svg_icon(&icon, Rect::new(0.0, 0.0, 32.0, 32.0), BLUE);
    // No image, no picture: two glyph quads on one atlas region.
    assert_eq!(frame.picture_cache_len(), 0);
    assert_eq!(frame.images().len(), 0);
    let regions: Vec<Rect> = frame
        .list
        .ops()
        .iter()
        .map(|op| match *op {
            DrawOp::Glyph { rect, atlas, color } => {
                assert_eq!(rect, WHOLE);
                assert!(color == RED || color == BLUE);
                atlas
            }
            ref other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(regions.len(), 2);
    assert_eq!(regions[0], regions[1], "one mask for both colours");
    assert_eq!((regions[0].width, regions[0].height), (32.0, 32.0));

    let mut frame = Frame::new();
    frame.begin();
    frame.draw_svg_icon(&icon, WHOLE, RED);
    let Some(pixels) = render(&frame) else { return };
    assert_eq!(at(&pixels, 16, 16), [255, 0, 0, 255]);
    assert_eq!(at(&pixels, 16, 10), [0, 0, 0, 255], "above the stroke");
    assert_eq!(at(&pixels, 16, 13), [255, 0, 0, 255]);
}

#[test]
fn a_multi_colour_svg_icon_keeps_its_colours_and_takes_current_color() {
    let icon = two_tone();
    assert!(!icon.is_single_ink());
    let mut frame = Frame::new();
    frame.begin();
    frame.draw_svg_icon(&icon, WHOLE, Color::rgba(0.0, 0.0, 1.0, 0.5));
    // A picture, faded by the colour's alpha through the tint.
    assert_eq!(frame.picture_cache_len(), 1);
    assert!(matches!(
        frame.list.ops()[..],
        [DrawOp::Image { tint, .. }] if tint == Color::rgba(1.0, 1.0, 1.0, 0.5)
    ));
    let Some(pixels) = render(&frame) else { return };
    assert_eq!(
        at(&pixels, 4, 16),
        [128, 0, 0, 255],
        "the file's red, at half"
    );
    assert_eq!(
        at(&pixels, 28, 16),
        [0, 0, 128, 255],
        "currentColor, at half"
    );
}
