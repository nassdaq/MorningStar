//! A screen using everything ADR 0017 added, rendered on the real GPU:
//! turned tiles with text, a turned text, synthetic italics, a multi-colour
//! vector picture, a hard text shadow and an outlined digit. A few pixels
//! are asserted; the rest is meant to be judged by eye.
//!
//! Set `MORNINGSTAR_DUMP_DIR` to a directory to have the screen written
//! there as `ui_transforms.png`. Skips where there is no GPU.

use morningstar_devtools::{Image as Screenshot, screenshot_frame};
use morningstar_runtime::{FrameInfo, GpuError};
use morningstar_ui::prelude::*;

const REGULAR: &[u8] =
    include_bytes!("../../../examples/hello-gpu/assets/fonts/NotoSans-Regular.ttf");
const BOLD: &[u8] = include_bytes!("../../../examples/hello-gpu/assets/fonts/NotoSans-Bold.ttf");

const INK: Color = hex(0x0E0E0E);
const GOLD: Color = hex(0xF6C445);
const CREAM: Color = hex(0xE9E2CF);
const PURPLE: Color = hex(0x9B5DE5);
const SKY: Color = hex(0x5AA9E6);
const WHITE: Color = hex(0xFFFFFF);

/// The practice-call disc of the design this work was done for, shortened:
/// transcribed from its inline SVG (view box 280, drawn at 250 dp).
const DISC: VectorPicture = VectorPicture::new(
    280.0,
    280.0,
    &[
        Layer::circle(140.0, 140.0, 138.0).stroke(with_alpha(GOLD, 0.25), 2.0),
        Layer::circle(140.0, 140.0, 122.0)
            .fill(INK)
            .stroke(GOLD, 1.5),
        Layer::rect(-40.0, 114.0, 360.0, 52.0)
            .fill(GOLD)
            .rotate(-18.0, 140.0, 140.0)
            .clip_circle(140.0, 140.0, 122.0),
        Layer::circle(140.0, 140.0, 108.0).stroke(with_alpha(GOLD, 0.55), 1.0),
        Layer::circle(140.0, 140.0, 96.0).stroke(with_alpha(GOLD, 0.55), 1.0),
        Layer::circle(140.0, 140.0, 84.0).stroke(with_alpha(GOLD, 0.55), 1.0),
        Layer::circle(140.0, 140.0, 72.0).stroke(with_alpha(GOLD, 0.55), 1.0),
        Layer::circle(140.0, 140.0, 99.0)
            .stroke(INK, 1.2)
            .clip_circle(140.0, 140.0, 122.0),
        Layer::circle(140.0, 140.0, 75.0)
            .stroke(INK, 1.2)
            .clip_circle(140.0, 140.0, 122.0),
        Layer::circle(140.0, 140.0, 48.0)
            .fill(CREAM)
            .stroke(INK, 3.0),
        Layer::path("M4 22v-4a14 14 0 0 1 28 0v4")
            .stroke(INK, 3.0)
            .round()
            .translate(122.0, 122.0),
        Layer::rounded_rect(2.0, 20.0, 7.0, 11.0, 2.0)
            .stroke(INK, 3.0)
            .round()
            .translate(122.0, 122.0),
        Layer::rounded_rect(27.0, 20.0, 7.0, 11.0, 2.0)
            .stroke(INK, 3.0)
            .round()
            .translate(122.0, 122.0),
    ],
);

/// `repeating-linear-gradient(45deg, gold 0 4px, ink 4px 7px)` on a 46 dp
/// square, as `docs/building-screens.md` says to write it: the second
/// colour as a fill, the first as lines 4 wide running down to the right,
/// 7 apart, the first one centred 2 from the bottom-left corner. Each line
/// is `y = x - c` with `c = (2 + 7k) * sqrt(2) - 46`, drawn a little past
/// the square, which cuts it off.
const STRIPES: VectorPicture = VectorPicture::new(
    46.0,
    46.0,
    &[
        Layer::rect(0.0, 0.0, 46.0, 46.0).fill(INK),
        Layer::path(
            "M-47.17 -4l54 54M-37.27 -4l54 54M-27.37 -4l54 54M-17.47 -4l54 54\
             M-7.57 -4l54 54M2.33 -4l54 54M12.22 -4l54 54M22.12 -4l54 54\
             M32.02 -4l54 54M41.92 -4l54 54",
        )
        .stroke(GOLD, 4.0),
    ],
);

struct Screen;

impl Application for Screen {
    type Message = ();

    fn setup(&mut self, setup: &mut Setup<'_>) {
        setup.add_font(REGULAR);
        setup.add_font(BOLD);
    }

    fn view(&self) -> Element<()> {
        let tile = |label: &'static str, color: Color, degrees: f32| {
            stack![
                Container::empty()
                    .fill()
                    .border(2.0, color)
                    .radius(4.0)
                    .rotate(degrees),
                Container::new(text(label).size(16.0).semibold().color(WHITE)).padding(12.0),
            ]
            .fill_width()
            .height(100.0)
        };
        col![
            row![
                // A solid tile turned with its text, as one.
                Container::new(text("TURNED").size(18.0).bold().color(INK))
                    .padding_xy(16.0, 10.0)
                    .background(GOLD)
                    .radius(6.0)
                    .rotate(-8.0)
                    .id("turned"),
                // A text turned on its own, moved up as the CSS list reads.
                text("rotate(5deg) translateY(-8px)")
                    .size(13.0)
                    .color(SKY)
                    .transform(Transform::rotate(5.0).and_translate(0.0, -8.0)),
            ]
            .gap(24.0)
            .cross_align(CrossAlign::Center),
            Text::rich([
                span("\u{201c}Sorry, could you say that "),
                span("slower").italic().color(GOLD),
                span("?\u{201d}"),
            ])
            .size(20.0)
            .color(WHITE),
            text("Synthetic italic, fourteen degrees")
                .size(20.0)
                .italic()
                .color(CREAM),
            row![
                Picture::vector(DISC).size(250.0, 250.0).id("disc"),
                col![
                    Picture::vector(STRIPES).size(46.0, 46.0).rotate(-6.0),
                    Picture::vector(DISC).size(60.0, 60.0).rotate(135.0),
                ]
                .gap(30.0),
            ]
            .gap(20.0)
            .cross_align(CrossAlign::Center),
            row![
                tile("Angry customer", PURPLE, -2.0),
                tile("Refund request", SKY, 0.8),
                tile("Billing problem", GOLD, 2.0),
            ]
            .fill_width()
            .gap(12.0),
            row![
                text("1")
                    .size(80.0)
                    .bold()
                    .line_height(1.0)
                    .color(PURPLE)
                    .shadow(3.0, 3.0, WHITE)
                    .rotate(3.0),
                text("2")
                    .size(70.0)
                    .bold()
                    .line_height(1.0)
                    .color(INK)
                    .outline(1.5, WHITE)
                    .rotate(-3.0)
                    .id("two"),
                text("3")
                    .size(82.0)
                    .bold()
                    .line_height(1.0)
                    .color(GOLD)
                    .shadow(-2.0, 2.0, SKY)
                    .rotate(-5.0),
                Container::empty()
                    .size(12.0, 70.0)
                    .background(PURPLE)
                    .rotate(4.0)
                    .id("bar"),
                // A turned box that clips its child, which overhangs
                // its rounded corner.
                Container::new(
                    Container::empty()
                        .size(36.0, 36.0)
                        .background(INK)
                        .offset(46.0, 16.0),
                )
                .size(70.0, 40.0)
                .background(GOLD)
                .radius(12.0)
                .clip()
                .rotate(20.0),
            ]
            .gap(28.0)
            .cross_align(CrossAlign::Center),
        ]
        .gap(18.0)
        .padding(24.0)
        .background(hex(0x17171A))
        .into()
    }

    fn update(&mut self, _: ()) {}
}

fn to_srgb(color: Color) -> [u8; 3] {
    let encode = |linear: f32| {
        let encoded = if linear <= 0.003_130_8 {
            linear * 12.92
        } else {
            1.055 * linear.powf(1.0 / 2.4) - 0.055
        };
        (encoded * 255.0).round() as u8
    };
    [encode(color.r), encode(color.g), encode(color.b)]
}

fn assert_color(shot: &Screenshot, x: u32, y: u32, color: Color, what: &str) {
    let actual = shot.pixel(x, y);
    let expected = to_srgb(color);
    for channel in 0..3 {
        assert!(
            actual[channel].abs_diff(expected[channel]) <= 2,
            "{what} at {x},{y}: {actual:?}, expected {expected:?}"
        );
    }
}

#[test]
fn a_screen_with_transforms_pictures_and_text_effects() {
    let scale = 2.0;
    let info = FrameInfo::new(morningstar_core::SurfaceSize::new(780, 1400)).with_scale(scale);
    let mut ui = Ui::new(Screen);
    let png = match screenshot_frame(&mut ui, &info) {
        Ok(png) => png,
        Err(GpuError::NoAdapter(reason)) => {
            eprintln!("skipping: no GPU adapter available ({reason})");
            return;
        }
        Err(other) => panic!("render failed: {other}"),
    };
    if let Some(directory) = std::env::var_os("MORNINGSTAR_DUMP_DIR") {
        let path = std::path::Path::new(&directory).join("ui_transforms.png");
        std::fs::write(&path, &png).expect("dump");
        eprintln!("wrote {}", path.display());
    }
    let shot = Screenshot::from_png(&png).expect("a PNG");
    assert!(ui.overflows().is_empty(), "{:?}", ui.overflows());

    // The turned tile keeps its centre, in its colour; its top-left
    // corner, which the turn swings down and left, is background now.
    let tile = ui.rect_of("turned").unwrap();
    let centre = tile.center();
    assert_color(&shot, tile.x as u32 + 6, centre.y as u32, GOLD, "tile");
    assert_color(
        &shot,
        (tile.x + tile.width) as u32 - 4,
        tile.y as u32 + 2,
        hex(0x17171A),
        "tile's old corner",
    );

    // The disc: its cream centre, the gold band through the middle, the
    // dark vinyl above it, and nothing outside its outer ring.
    let disc = ui.rect_of("disc").unwrap();
    let (cx, cy) = (disc.center().x as u32, disc.center().y as u32);
    let unit = disc.width / 280.0;
    let at = |dx: f32, dy: f32| {
        (
            (cx as f32 + dx * unit) as u32,
            (cy as f32 + dy * unit) as u32,
        )
    };
    let (x, y) = at(0.0, -30.0);
    assert_color(&shot, x, y, CREAM, "disc centre");
    let (x, y) = at(-90.0, 29.0);
    assert_color(
        &shot,
        x,
        y,
        GOLD,
        "band, lower at the left (turned -18 degrees)",
    );
    let (x, y) = at(90.0, -29.0);
    assert_color(&shot, x, y, GOLD, "band, higher at the right");
    let (x, y) = at(0.0, -90.0);
    assert_color(&shot, x, y, INK, "vinyl");
    let (x, y) = at(-134.0, -134.0);
    assert_color(&shot, x, y, hex(0x17171A), "outside the disc");

    // The outlined digit: white stroke pixels and ink fill inside its box.
    let two = ui.rect_of("two").unwrap();
    let mut white = 0;
    let mut ink = 0;
    for y in two.y as u32..(two.y + two.height) as u32 {
        for x in two.x as u32..(two.x + two.width) as u32 {
            let [r, g, b, _] = shot.pixel(x, y);
            white += u32::from(r > 240 && g > 240 && b > 240);
            ink += u32::from(r < 20 && g < 20 && b < 20);
        }
    }
    assert!(white > 300 && ink > 300, "{white} white, {ink} ink");

    // The purple bar, turned 4 degrees about its centre.
    let bar = ui.rect_of("bar").unwrap();
    assert_color(
        &shot,
        bar.center().x as u32,
        bar.center().y as u32,
        PURPLE,
        "bar",
    );
}
