//! Turned text before and after ADR 0029, through a `Frame` as an app
//! draws it: the frame in which the angle has just changed is the old
//! rendering (upright glyphs resampled by the renderer) and asks for one
//! more frame; that frame is the new one (glyphs rasterised for the
//! angle). Both are written to `MORNINGSTAR_DUMP_DIR` when that is set,
//! one above the other and enlarged three times.

use morningstar_devtools::Image;
use morningstar_gpu::{GpuError, render_offscreen};
use morningstar_runtime::{
    App, Color, Frame, FrameInfo, Point, Setup, SurfaceSize, TextStyle, Transform,
};

const FONT: &[u8] = include_bytes!("../../../examples/hello-gpu/assets/fonts/NotoSans-Regular.ttf");
const SIZE: SurfaceSize = SurfaceSize::new(300, 96);

struct Fonts;

impl App for Fonts {
    fn setup(&mut self, setup: &mut Setup<'_>) {
        setup.add_font(FONT);
    }

    fn frame(&mut self, _: &FrameInfo, _: &mut Frame) {}
}

/// One frame: two lines turned by `degrees` about the middle.
fn draw(frame: &mut Frame, degrees: f32) {
    frame.begin();
    frame.clear = Color::rgb(0.003, 0.003, 0.003);
    let turn = Transform::rotate(degrees).about(Point::new(150.0, 48.0));
    frame.list.push_transform(turn);
    frame.draw_text(
        "Angry customer, calm reply",
        &TextStyle::new(20.0, Color::WHITE),
        Point::new(24.0, 22.0),
        None,
    );
    frame.draw_text(
        "minimum illumination 0123456789",
        &TextStyle::new(13.0, Color::rgb(0.9, 0.55, 0.05)),
        Point::new(24.0, 54.0),
        None,
    );
    frame.list.pop_transform();
}

fn render(frame: &Frame) -> Option<Image> {
    match render_offscreen(SIZE, frame.clear, &frame.list, frame.atlas()) {
        Ok(pixels) => Some(Image::new(SIZE.width, SIZE.height, pixels)),
        Err(GpuError::NoAdapter(reason)) => {
            eprintln!("skipping: no GPU adapter available ({reason})");
            None
        }
        Err(other) => panic!("offscreen render failed: {other}"),
    }
}

/// Pixels that are neither the background nor fully the text's colour:
/// the soft ones.
fn soft(image: &Image) -> usize {
    image
        .rgba()
        .chunks_exact(4)
        .filter(|pixel| (40..215).contains(&pixel[0]))
        .count()
}

#[test]
fn text_is_sharp_once_its_angle_has_held_for_a_frame() {
    let mut frame = Frame::for_app(&mut Fonts);
    // The text exists at another angle first, as in an animation.
    draw(&mut frame, -9.0);
    assert!(!frame.redraw_requested(), "a first frame is sharp at once");
    draw(&mut frame, -8.0);
    assert!(
        frame.redraw_requested(),
        "the angle changed: drawn resampled, and one more frame is asked for"
    );
    let Some(before) = render(&frame) else { return };
    draw(&mut frame, -8.0);
    assert!(!frame.redraw_requested(), "settled");
    let after = render(&frame).unwrap();

    // The same text in the same place, with fewer in-between pixels.
    let (soft_before, soft_after) = (soft(&before), soft(&after));
    println!("soft pixels: {soft_before} resampled, {soft_after} rasterised turned");
    assert!(
        (soft_after as f32) < soft_before as f32 * 0.9,
        "{soft_after} against {soft_before}"
    );
    let ink = |image: &Image| -> f32 {
        image
            .rgba()
            .chunks_exact(4)
            .map(|pixel| f32::from(pixel[0]) / 255.0)
            .sum()
    };
    let (ink_before, ink_after) = (ink(&before), ink(&after));
    assert!(
        (ink_before - ink_after).abs() < ink_before * 0.06,
        "{ink_before} against {ink_after}"
    );

    if let Ok(directory) = std::env::var("MORNINGSTAR_DUMP_DIR") {
        // Before above after, each pixel three by three.
        const ZOOM: u32 = 3;
        let mut sheet = Image::filled(SIZE.width * ZOOM, SIZE.height * ZOOM * 2, [0, 0, 0, 255]);
        for (row, image) in [&before, &after].into_iter().enumerate() {
            for y in 0..SIZE.height * ZOOM {
                for x in 0..SIZE.width * ZOOM {
                    sheet.set_pixel(
                        x,
                        y + row as u32 * SIZE.height * ZOOM,
                        image.pixel(x / ZOOM, y / ZOOM),
                    );
                }
            }
        }
        let path = std::path::Path::new(&directory).join("turned-text-before-after.png");
        sheet.save(&path).expect("write");
        println!("{}", path.display());
    }
}
