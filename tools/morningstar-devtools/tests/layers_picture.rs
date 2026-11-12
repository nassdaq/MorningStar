//! One picture of what a layer is for (ADR 0028): the same card at half
//! opacity, faded colour by colour on the left and as a group on the
//! right. Written to `MORNINGSTAR_DUMP_DIR` when that is set.

use morningstar_devtools::{Image, encode_png};
use morningstar_gpu::{GpuError, render_offscreen};
use morningstar_runtime::{AtlasView, Color, DisplayList, Rect, SurfaceSize};

const SIZE: SurfaceSize = SurfaceSize::new(480, 240);

fn gray(level: f32) -> Color {
    Color::rgb(level, level, level)
}

/// A white card with a dark band across its top and a disc that hangs
/// over its corner, every colour at `alpha`.
fn card(list: &mut DisplayList, x: f32, alpha: f32) {
    let fade = |color: Color| Color::rgba(color.r, color.g, color.b, color.a * alpha);
    let body = Rect::new(x, 50.0, 160.0, 140.0);
    list.fill_rounded_rect(body, 16.0, fade(Color::WHITE));
    list.push_rounded_clip(body, 16.0);
    list.fill_rect(
        Rect::new(x, 50.0, 160.0, 44.0),
        fade(Color::rgb(0.0, 0.05, 0.3)),
    );
    list.pop_clip();
    list.fill_rounded_rect(
        Rect::new(x + 110.0, 70.0, 70.0, 70.0),
        35.0,
        fade(Color::rgb(1.0, 0.0, 0.0)),
    );
}

#[test]
fn a_group_fades_as_one_picture_and_colours_faded_singly_do_not() {
    let mut list = DisplayList::new();
    // Stripes behind, so that what shows through can be seen.
    for stripe in 0..12 {
        list.fill_rect(
            Rect::new(0.0, stripe as f32 * 20.0, 480.0, 10.0),
            Color::rgb(0.05, 0.25, 0.1),
        );
    }
    card(&mut list, 40.0, 0.5);
    list.push_layer(0.5);
    card(&mut list, 270.0, 1.0);
    list.pop_layer();

    let pixels = match render_offscreen(SIZE, gray(0.6), &list, AtlasView::EMPTY) {
        Ok(pixels) => pixels,
        Err(GpuError::NoAdapter(reason)) => {
            eprintln!("skipping: no GPU adapter available ({reason})");
            return;
        }
        Err(other) => panic!("offscreen render failed: {other}"),
    };
    let image = Image::new(SIZE.width, SIZE.height, pixels.clone());
    if let Ok(directory) = std::env::var("MORNINGSTAR_DUMP_DIR") {
        let path = std::path::Path::new(&directory).join("group-opacity.png");
        std::fs::write(&path, encode_png(SIZE.width, SIZE.height, &pixels)).expect("write");
        println!("{}", path.display());
    }

    // Where the disc lies over the card: as a group, half of the disc's
    // red over the background; colour by colour, the card's white shows
    // through it and lightens it.
    let by_colour = image.pixel(40 + 125, 120);
    let as_group = image.pixel(270 + 125, 120);
    assert!(
        by_colour[1] > as_group[1] + 15,
        "{by_colour:?} against {as_group:?}"
    );
    // The band over the card: colour by colour, the card's white shows
    // through the band and lightens it; as a group the band is itself.
    let by_colour = image.pixel(40 + 40, 70);
    let as_group = image.pixel(270 + 40, 70);
    assert!(
        by_colour[0] > as_group[0] + 8,
        "{by_colour:?} against {as_group:?}"
    );
    // Outside both cards the two halves are the same picture.
    assert_eq!(image.pixel(20, 20), image.pixel(250, 20));
}
