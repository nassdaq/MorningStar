//! The compositing space (ADR 0023): colours are sRGB-encoded before they
//! are blended, interpolated or filtered, as in a browser, and the target
//! holds the result unchanged. Every expected byte here is what Chrome
//! shows for the same CSS; the value linear-light blending would give is
//! named beside it, because the two differ by a factor of two in places.
//!
//! Pixel tests against the real GPU; they skip where there is none.

use morningstar_core::{
    AtlasView, Color, DisplayList, ImageStore, LinearGradient, Rect, Shadow, SurfaceSize,
};
use morningstar_gpu::{GpuError, render_offscreen, render_offscreen_with_images};

/// The colour a design writes as `#RRGGBB`, with an alpha.
fn hex(rgb: u32, alpha: f32) -> Color {
    let channel = |shift: u32| {
        let encoded = ((rgb >> shift) & 0xff) as f32 / 255.0;
        if encoded <= 0.040_45 {
            encoded / 12.92
        } else {
            ((encoded + 0.055) / 1.055).powf(2.4)
        }
    };
    Color::rgba(channel(16), channel(8), channel(0), alpha)
}

fn skip<T>(result: Result<T, GpuError>) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(GpuError::NoAdapter(reason)) => {
            eprintln!("skipping: no GPU adapter available ({reason})");
            None
        }
        Err(other) => panic!("offscreen render failed: {other}"),
    }
}

fn render(width: u32, height: u32, clear: Color, list: &DisplayList) -> Option<Vec<[u8; 4]>> {
    let pixels = skip(render_offscreen(
        SurfaceSize::new(width, height),
        clear,
        list,
        AtlasView::new(1, 1, &[128], 1),
    ))?;
    Some(
        pixels
            .chunks_exact(4)
            .map(|p| [p[0], p[1], p[2], p[3]])
            .collect(),
    )
}

fn close(actual: [u8; 4], expected: [u8; 4]) -> bool {
    actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= 1)
}

#[test]
fn a_designs_colour_is_the_byte_on_screen() {
    let mut list = DisplayList::new();
    list.fill_rect(Rect::new(0.0, 0.0, 1.0, 1.0), hex(0xF6C445, 1.0));
    list.fill_rect(Rect::new(1.0, 0.0, 1.0, 1.0), hex(0x808080, 1.0));
    let Some(frame) = render(3, 1, hex(0x0E0E0E, 1.0), &list) else {
        return;
    };
    assert_eq!(frame[0], [0xF6, 0xC4, 0x45, 255]);
    assert_eq!(frame[1], [0x80, 0x80, 0x80, 255]);
    assert_eq!(frame[2], [0x0E, 0x0E, 0x0E, 255], "the clear colour too");
}

#[test]
fn a_translucent_colour_shows_that_share_of_its_encoded_value() {
    // CallReady's ring: `stroke="#F6C445" stroke-opacity="0.25"` on black.
    // A browser shows #3E3111; blended in linear light it was #83692A,
    // about twice as bright.
    let mut list = DisplayList::new();
    list.fill_rect(Rect::new(0.0, 0.0, 1.0, 1.0), hex(0xF6C445, 0.25));
    // 14% of ink over white, as a faint shadow is: #DEDFDF, not the
    // #EFF0EF of linear light, which looked half as dark.
    list.fill_rect(Rect::new(1.0, 0.0, 1.0, 1.0), Color::WHITE);
    list.fill_rect(Rect::new(1.0, 0.0, 1.0, 1.0), hex(0x14201A, 0.14));
    // Half black over white is the middle byte, not 188.
    list.fill_rect(Rect::new(2.0, 0.0, 1.0, 1.0), Color::WHITE);
    list.fill_rect(
        Rect::new(2.0, 0.0, 1.0, 1.0),
        Color::rgba(0.0, 0.0, 0.0, 0.5),
    );
    let Some(frame) = render(3, 1, Color::BLACK, &list) else {
        return;
    };
    assert!(close(frame[0], [0x3E, 0x31, 0x11, 255]), "{:?}", frame[0]);
    assert!(close(frame[1], [0xDE, 0xDF, 0xDF, 255]), "{:?}", frame[1]);
    assert!(close(frame[2], [128, 128, 128, 255]), "{:?}", frame[2]);
}

#[test]
fn an_edge_covering_half_a_pixel_is_the_middle_byte_on_light_and_on_dark() {
    // What makes text and thin strokes as heavy as a browser's: half
    // coverage of black on white is 128 (in linear light it was 188,
    // lighter), and of white on black 128 too (it was 188, heavier).
    let mut list = DisplayList::new();
    list.fill_rect(Rect::new(0.0, 0.0, 4.0, 1.0), Color::WHITE);
    list.fill_rect(Rect::new(0.0, 0.0, 1.5, 1.0), Color::BLACK);
    list.fill_rect(Rect::new(4.0, 0.0, 1.5, 1.0), Color::WHITE);
    // Glyph coverage is blended the same way: the atlas texel is 128.
    let texel = Rect::new(0.0, 0.0, 1.0, 1.0);
    list.draw_glyph(Rect::new(3.0, 0.0, 1.0, 1.0), texel, Color::BLACK);
    list.draw_glyph(Rect::new(7.0, 0.0, 1.0, 1.0), texel, Color::WHITE);
    let Some(frame) = render(8, 1, Color::BLACK, &list) else {
        return;
    };
    assert!(close(frame[1], [128, 128, 128, 255]), "{:?}", frame[1]);
    assert!(close(frame[5], [128, 128, 128, 255]), "{:?}", frame[5]);
    assert!(close(frame[3], [127, 127, 127, 255]), "{:?}", frame[3]);
    assert!(close(frame[7], [128, 128, 128, 255]), "{:?}", frame[7]);
}

#[test]
fn a_gradient_passes_through_the_mean_of_its_encoded_ends() {
    // `linear-gradient(90deg, #1E8416, #F6C445)`: a browser's midpoint is
    // the mean of the bytes, #8AA42E (in linear light it was #B4A832,
    // lighter, which the design guide used to warn about).
    let rect = Rect::new(0.0, 0.0, 65.0, 1.0);
    let mut list = DisplayList::new();
    list.fill_gradient_rect(
        rect,
        0.0,
        LinearGradient::horizontal(rect, hex(0x1E8416, 1.0), hex(0xF6C445, 1.0)),
    );
    let Some(frame) = render(65, 1, Color::BLACK, &list) else {
        return;
    };
    assert!(close(frame[32], [0x8A, 0xA4, 0x2E, 255]), "{:?}", frame[32]);
    // To transparent: the colour keeps its hue and fades by its alpha.
    let mut list = DisplayList::new();
    list.fill_gradient_rect(
        rect,
        0.0,
        LinearGradient::horizontal(rect, hex(0xF6C445, 1.0), hex(0xF6C445, 0.0)),
    );
    let frame = render(65, 1, Color::BLACK, &list).unwrap();
    assert!(close(frame[32], [0x7B, 0x62, 0x23, 255]), "{:?}", frame[32]);
}

#[test]
fn a_translucent_image_pixel_is_blended_as_its_bytes() {
    let mut store = ImageStore::new();
    // Straight alpha, as in a PNG: grey 200 at half alpha.
    let image = store.add_rgba8(1, 1, &[200, 200, 200, 128]).unwrap();
    let mut list = DisplayList::new();
    list.draw_image(image, Rect::new(0.0, 0.0, 1.0, 1.0));
    let Some(pixels) = skip(render_offscreen_with_images(
        SurfaceSize::new(1, 1),
        Color::WHITE,
        &list,
        AtlasView::EMPTY,
        store.view(),
    )) else {
        return;
    };
    // 200 * 0.502 + 255 * 0.498 = 227, as a browser draws the PNG.
    assert!(close(
        [pixels[0], pixels[1], pixels[2], pixels[3]],
        [227, 227, 227, 255]
    ));
}

#[test]
fn a_shadows_darkest_point_is_its_alpha_of_the_way_to_its_colour() {
    // A large box's shadow, far from its edges, is the shadow's colour at
    // its alpha: 28% of #14201A over white is #BDC1BF. (In linear light
    // 14% gave about this much; FundiDesk doubled its alphas for that.)
    let mut list = DisplayList::new();
    list.draw_shadow(
        Rect::new(-100.0, -100.0, 264.0, 264.0),
        0.0,
        Shadow::new(0.0, 0.0, 8.0, hex(0x14201A, 0.28)),
    );
    let Some(frame) = render(64, 64, Color::WHITE, &list) else {
        return;
    };
    let centre = frame[32 * 64 + 32];
    assert!(close(centre, [0xBD, 0xC1, 0xBF, 255]), "{centre:?}");
}
