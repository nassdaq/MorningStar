//! Loaded SVG files in the UI layer (ADR 0027): `Picture::svg` and
//! `Icon::svg`.

mod common;

use common::Harness;
use morningstar_core::{DrawOp, Rect};
use morningstar_ui::prelude::*;

static BADGE: SvgAsset = SvgAsset::new(
    r##"<svg width="40" height="20" viewBox="0 0 80 40">
          <rect width="80" height="40" rx="8" fill="#1D3557"/>
          <circle cx="20" cy="20" r="10" fill="currentColor"/>
        </svg>"##,
);

static CHECK: SvgAsset = SvgAsset::new(
    r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M5 12l5 5 9-10"/></svg>"#,
);

#[test]
fn an_svg_picture_is_sized_by_the_file_unless_told_otherwise() {
    assert!(BADGE.check().is_ok());
    let harness = Harness::new(|_| {
        col![
            Picture::svg(BADGE.get()).id("natural"),
            Picture::svg(BADGE.get()).width(100.0).id("by-width"),
            Picture::svg(BADGE.get()).size(30.0, 30.0).id("boxed"),
        ]
        .into()
    });
    // The file's width and height, not its view box.
    assert_eq!(harness.dp_rect("natural"), Rect::new(0.0, 0.0, 40.0, 20.0));
    assert_eq!(
        harness.dp_rect("by-width"),
        Rect::new(0.0, 20.0, 100.0, 50.0)
    );
    assert_eq!(harness.dp_rect("boxed"), Rect::new(0.0, 70.0, 30.0, 30.0));

    // Each is one image, rasterised for the pixels it covers.
    let images: Vec<(Rect, Rect)> = harness
        .ops()
        .iter()
        .filter_map(|op| match *op {
            DrawOp::Image { rect, source, .. } => Some((rect, source)),
            _ => None,
        })
        .collect();
    assert_eq!(images.len(), 3);
    for (rect, source) in &images {
        assert_eq!((source.width, source.height), (rect.width, rect.height));
    }
    assert_eq!(images[0].0, Rect::new(0.0, 0.0, 120.0, 60.0));
    assert_eq!(harness.frame.picture_cache_len(), 3);
}

#[test]
fn an_svg_picture_takes_radius_tint_and_a_transform() {
    let harness = Harness::new(|_| {
        col![
            Picture::svg(&BADGE.get().with_current_color(hex(0xF6C445)))
                .size(80.0, 40.0)
                .radius(6.0)
                .tint(with_alpha(Color::WHITE, 0.5))
                .rotate(-18.0)
        ]
        .into()
    });
    let ops = harness.ops();
    assert!(matches!(ops[0], DrawOp::PushTransform { .. }));
    assert!(matches!(
        ops[1],
        DrawOp::Image { rect, corner_radius, tint, .. }
            if rect == Rect::new(0.0, 0.0, 240.0, 120.0) && corner_radius == 18.0 && tint.a == 0.5
    ));
    assert_eq!(ops[2], DrawOp::PopTransform);
}

#[test]
fn an_svg_icon_of_one_colour_is_a_glyph_in_the_icons_colour() {
    const GOLD: Color = hex(0xF6C445);
    let harness = Harness::new(|_| {
        row![
            Icon::svg(CHECK.get()).id("plain"),
            Icon::svg(CHECK.get()).size(32.0).color(GOLD).id("gold"),
            Icon::svg(CHECK.get())
                .background(GOLD)
                .size(40.0)
                .id("tiled"),
        ]
        .into()
    });
    assert_eq!(harness.dp_rect("plain"), Rect::new(0.0, 0.0, 24.0, 24.0));
    assert_eq!(harness.dp_rect("gold").width, 32.0);
    assert_eq!(harness.dp_rect("tiled").width, 40.0);
    let glyphs: Vec<(Rect, Color)> = harness
        .ops()
        .iter()
        .filter_map(|op| match *op {
            DrawOp::Glyph { rect, color, .. } => Some((rect, color)),
            _ => None,
        })
        .collect();
    // Three masks in the atlas, no image and no picture.
    assert_eq!(glyphs.len(), 3);
    assert_eq!(glyphs[0].0, Rect::new(0.0, 0.0, 72.0, 72.0));
    assert_eq!(glyphs[1].1, GOLD);
    // On a tile the icon takes half the side.
    assert_eq!((glyphs[2].0.width, glyphs[2].0.height), (60.0, 60.0));
    assert_eq!(harness.frame.picture_cache_len(), 0);
    assert!(
        !harness
            .ops()
            .iter()
            .any(|op| matches!(op, DrawOp::Image { .. }))
    );
}

#[test]
fn an_svg_icon_of_several_colours_is_a_picture_with_current_color_set() {
    const GOLD: Color = hex(0xF6C445);
    let harness = Harness::new(|_| col![Icon::svg(BADGE.get()).size(40.0).color(GOLD)].into());
    let images = harness
        .ops()
        .iter()
        .filter(|op| matches!(op, DrawOp::Image { tint, .. } if *tint == Color::WHITE))
        .count();
    assert_eq!(images, 1);
    assert_eq!(harness.frame.picture_cache_len(), 1);
}
