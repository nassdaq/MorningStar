//! Images through the application-facing API (ADR 0014): PNG decoding into
//! registered images, their way from `Frame` to the GPU, and one scene meant
//! to be judged by eye.
//!
//! Set `MORNINGSTAR_DUMP_DIR` to a directory to have the scene written
//! there as `image_scene.png` (sRGB-encoded, as a screen would show it).

use morningstar_core::{
    AtlasView, Border, Color, DisplayList, ImageFit, LinearGradient, Point, Rect, Shadow,
    SurfaceSize,
};
use morningstar_gpu::{GpuError, render_offscreen_with_images};
use morningstar_runtime::{App, Frame, FrameInfo, Image, ImageError, Setup};

// --- PNG helpers -------------------------------------------------------------

/// Encodes a PNG. `depth` is bits per sample.
fn encode_png(
    width: u32,
    height: u32,
    color: png::ColorType,
    depth: png::BitDepth,
    palette: Option<(&[u8], &[u8])>,
    data: &[u8],
) -> Vec<u8> {
    let mut file = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut file, width, height);
        encoder.set_color(color);
        encoder.set_depth(depth);
        if let Some((palette, transparency)) = palette {
            encoder.set_palette(palette.to_vec());
            encoder.set_trns(transparency.to_vec());
        }
        let mut writer = encoder.write_header().expect("png header");
        writer.write_image_data(data).expect("png data");
        writer.finish().expect("png end");
    }
    file
}

fn rgba_png(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
    encode_png(
        width,
        height,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        None,
        pixels,
    )
}

/// CRC-32 as PNG chunks use it.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// The pixels a frame's store holds for `image`: premultiplied sRGB.
fn stored(frame: &Frame, image: Image) -> (u32, u32, Vec<u8>) {
    let view = frame.images();
    let pixels = view.get(image.id()).expect("registered");
    (pixels.width, pixels.height, pixels.pixels.to_vec())
}

// --- Decoding ----------------------------------------------------------------

#[test]
fn rgba_png_decodes_to_the_pixels_it_was_made_from() {
    // Opaque pixels only, so premultiplication leaves them unchanged.
    let pixels = [
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, //
        10, 20, 30, 255, 128, 128, 128, 255, 255, 255, 255, 255,
    ];
    let mut frame = Frame::new();
    let image = frame.add_image_png(&rgba_png(3, 2, &pixels)).unwrap();
    assert_eq!((image.width(), image.height()), (3, 2));
    assert!(frame.has_image(image));
    assert_eq!(stored(&frame, image), (3, 2, pixels.to_vec()));
}

#[test]
fn png_alpha_is_straight_and_gets_premultiplied_on_registration() {
    let mut frame = Frame::new();
    let image = frame
        .add_image_png(&rgba_png(2, 1, &[255, 255, 255, 128, 200, 100, 50, 0]))
        .unwrap();
    // Half-transparent white is its bytes times alpha (ADR 0023: encoded
    // values are what is blended), and an invisible pixel loses its colour.
    assert_eq!(stored(&frame, image).2, [128, 128, 128, 128, 0, 0, 0, 0]);
    // Registering the same bytes as raw pixels gives the same image data.
    let raw = frame
        .add_image_rgba8(2, 1, &[255, 255, 255, 128, 200, 100, 50, 0])
        .unwrap();
    assert_eq!(stored(&frame, raw).2, stored(&frame, image).2);
    assert_ne!(raw.id(), image.id());
}

#[test]
fn every_png_colour_type_becomes_rgba() {
    use png::{BitDepth, ColorType};
    let mut frame = Frame::new();
    let mut decode = |file: Vec<u8>| {
        let image = frame.add_image_png(&file).expect("decodes");
        stored(&frame, image).2
    };

    let rgb = encode_png(
        2,
        1,
        ColorType::Rgb,
        BitDepth::Eight,
        None,
        &[9, 8, 7, 1, 2, 3],
    );
    assert_eq!(decode(rgb), [9, 8, 7, 255, 1, 2, 3, 255]);

    let grey = encode_png(2, 1, ColorType::Grayscale, BitDepth::Eight, None, &[0, 200]);
    assert_eq!(decode(grey), [0, 0, 0, 255, 200, 200, 200, 255]);

    let grey_alpha = encode_png(
        2,
        1,
        ColorType::GrayscaleAlpha,
        BitDepth::Eight,
        None,
        &[77, 255, 255, 0],
    );
    assert_eq!(decode(grey_alpha), [77, 77, 77, 255, 0, 0, 0, 0]);

    // One bit per pixel: black, white, white, black.
    let bits = encode_png(
        4,
        1,
        ColorType::Grayscale,
        BitDepth::One,
        None,
        &[0b0110_0000],
    );
    assert_eq!(
        decode(bits),
        [
            0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 255
        ]
    );

    // A palette of red, green and a transparent third entry.
    let palette = [255, 0, 0, 0, 255, 0, 0, 0, 255];
    let indexed = encode_png(
        3,
        1,
        ColorType::Indexed,
        BitDepth::Eight,
        Some((&palette, &[255, 255, 0])),
        &[1, 0, 2],
    );
    assert_eq!(
        decode(indexed),
        [0, 255, 0, 255, 255, 0, 0, 255, 0, 0, 0, 0]
    );

    // Sixteen bits per sample keep their high byte.
    let deep = encode_png(
        1,
        1,
        ColorType::Rgba,
        BitDepth::Sixteen,
        None,
        &[0xFF, 0xFF, 0x80, 0x00, 0x00, 0x00, 0xFF, 0xFF],
    );
    assert_eq!(decode(deep), [255, 128, 0, 255]);
}

#[test]
fn bad_png_data_is_an_error_not_a_panic() {
    let good = rgba_png(4, 4, &[200; 64]);
    let mut frame = Frame::new();
    let mut fails = |data: &[u8], what: &str| {
        let result = frame.add_image_png(data);
        assert!(
            matches!(result, Err(ImageError::Decode(_))),
            "{what}: {result:?}"
        );
    };

    fails(b"", "no data");
    fails(b"definitely not a png", "not a png");
    fails(
        &[0xFF, 0xD8, 0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F'],
        "a JPEG",
    );
    fails(&good[..8], "signature only");
    fails(&good[..good.len() / 2], "truncated");
    fails(&good[..good.len() - 12], "missing its end");

    // One flipped bit in the header fails its checksum.
    let mut corrupt = good.clone();
    corrupt[17] ^= 0x01;
    fails(&corrupt, "corrupt header");

    // Compressed data that is not a valid stream.
    let mut garbled = good.clone();
    let data_start = garbled
        .windows(4)
        .position(|window| window == b"IDAT")
        .expect("has image data")
        + 4;
    for byte in &mut garbled[data_start..data_start + 8] {
        *byte = !*byte;
    }
    fails(&garbled, "garbled image data");

    assert!(frame.images().is_empty(), "nothing was registered");
    // The error says something a developer can act on.
    let message = frame.add_image_png(b"nope").unwrap_err().to_string();
    assert!(message.starts_with("could not decode image: "), "{message}");
}

#[test]
fn a_png_claiming_an_enormous_size_is_refused_before_allocating() {
    // A valid signature and header for a 60000 x 60000 RGBA image (14 GB),
    // followed by nothing.
    let mut file = vec![0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];
    let mut header = b"IHDR".to_vec();
    header.extend_from_slice(&60_000u32.to_be_bytes());
    header.extend_from_slice(&60_000u32.to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]);
    file.extend_from_slice(&13u32.to_be_bytes());
    file.extend_from_slice(&header);
    file.extend_from_slice(&crc32(&header).to_be_bytes());

    let mut frame = Frame::new();
    assert_eq!(
        frame.add_image_png(&file),
        Err(ImageError::TooLarge {
            width: 60_000,
            height: 60_000
        })
    );
}

#[test]
fn raw_pixels_are_validated() {
    let mut frame = Frame::new();
    assert_eq!(frame.add_image_rgba8(0, 1, &[]), Err(ImageError::Empty));
    assert_eq!(
        frame.add_image_rgba8(2, 2, &[0; 12]),
        Err(ImageError::WrongLength {
            expected: 16,
            actual: 12
        })
    );
}

// --- Lifecycle through the app API -------------------------------------------

#[test]
fn images_live_from_setup_across_frames_until_removed() {
    struct Logo {
        logo: Option<Image>,
    }
    impl App for Logo {
        fn setup(&mut self, setup: &mut Setup<'_>) {
            let png = rgba_png(2, 2, &[255; 16]);
            self.logo = Some(setup.add_image_png(&png).unwrap());
            assert!(setup.add_image_png(b"broken").is_err());
            assert!(setup.add_image_rgba8(1, 1, &[1, 2, 3, 255]).is_ok());
        }

        fn frame(&mut self, _info: &FrameInfo, frame: &mut Frame) {
            let logo = self.logo.expect("set up");
            frame.list.draw_image(logo, Rect::new(0.0, 0.0, 2.0, 2.0));
        }
    }

    let mut app = Logo { logo: None };
    let mut frame = Frame::for_app(&mut app);
    let logo = app.logo.unwrap();
    assert_eq!(frame.images().len(), 2);
    let generation = frame.images().generation();

    let info = FrameInfo::new(SurfaceSize::new(4, 4));
    for _ in 0..3 {
        frame.begin();
        app.frame(&info, &mut frame);
        assert_eq!(frame.list.len(), 1);
        assert!(frame.has_image(logo), "begin() keeps the images");
    }
    assert_eq!(
        frame.images().generation(),
        generation,
        "drawing changes nothing a renderer must react to"
    );

    assert!(frame.remove_image(logo));
    assert!(!frame.has_image(logo));
    assert!(!frame.remove_image(logo));
    assert_eq!(frame.images().len(), 1);
    assert_ne!(frame.images().generation(), generation);
    // The app does not know yet and draws it anyway: recorded, harmless.
    frame.begin();
    app.frame(&info, &mut frame);
    assert_eq!(frame.list.len(), 1);
}

// --- On the GPU --------------------------------------------------------------

struct Picture {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Picture {
    fn at(&self, x: u32, y: u32) -> [u8; 4] {
        let start = ((y * self.width + x) * 4) as usize;
        self.pixels[start..start + 4].try_into().unwrap()
    }
}

fn render(
    size: SurfaceSize,
    frame: &Frame,
    list: &DisplayList,
    atlas: AtlasView<'_>,
) -> Option<Picture> {
    match render_offscreen_with_images(size, frame.clear, list, atlas, frame.images()) {
        Ok(pixels) => Some(Picture {
            width: size.width,
            height: size.height,
            pixels,
        }),
        Err(GpuError::NoAdapter(reason)) => {
            eprintln!("skipping: no GPU adapter available ({reason})");
            None
        }
        Err(other) => panic!("offscreen render failed: {other}"),
    }
}

#[test]
fn a_png_drawn_through_a_frame_reaches_the_screen_pixel_for_pixel() {
    let mut frame = Frame::new();
    let red = [255, 0, 0, 255];
    let blue = [0, 0, 255, 255];
    let clear = [0, 255, 0, 0];
    let png = rgba_png(2, 2, &[red, blue, blue, clear].concat());
    let image = frame.add_image_png(&png).unwrap();
    frame.clear = Color::WHITE;
    frame.list.draw_image(image, Rect::new(1.0, 1.0, 2.0, 2.0));

    let Some(picture) = render(SurfaceSize::new(4, 4), &frame, &frame.list, frame.atlas()) else {
        return;
    };
    let white = [255; 4];
    assert_eq!(picture.at(1, 1), red);
    assert_eq!(picture.at(2, 1), blue);
    assert_eq!(picture.at(1, 2), blue);
    assert_eq!(
        picture.at(2, 2),
        white,
        "the transparent texel shows the background"
    );
    assert_eq!(picture.at(0, 0), white);
    assert_eq!(picture.at(3, 3), white);
}

// --- The scene ---------------------------------------------------------------

/// A colour given the way design tools give it, as sRGB bytes. `Color` is
/// linear.
fn srgb(red: u8, green: u8, blue: u8) -> Color {
    let linear = |byte: u8| {
        let v = f32::from(byte) / 255.0;
        if v <= 0.040_45 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    Color::rgb(linear(red), linear(green), linear(blue))
}

fn with_alpha(color: Color, alpha: f32) -> Color {
    Color::rgba(color.r, color.g, color.b, alpha)
}

/// Builds straight-alpha sRGB pixels from a function of normalised position.
fn paint(width: u32, height: u32, pixel: impl Fn(f32, f32) -> [f32; 4]) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let rgba = pixel(
                (x as f32 + 0.5) / width as f32,
                (y as f32 + 0.5) / height as f32,
            );
            pixels.extend(rgba.map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8));
        }
    }
    pixels
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

/// One at `edge` and inside, falling to zero over `soft` beyond it.
fn falloff(distance: f32, edge: f32, soft: f32) -> f32 {
    (1.0 - (distance - edge) / soft).clamp(0.0, 1.0)
}

/// A sunset: sky gradient, sun, two ranges of hills. Wider than tall.
fn landscape() -> Vec<u8> {
    paint(480, 270, |x, y| {
        let sky = mix([0.16, 0.20, 0.52], [1.0, 0.62, 0.32], y * 1.25);
        let sun = falloff(
            ((x - 0.68).powi(2) * 3.2 + (y - 0.52).powi(2)).sqrt(),
            0.09,
            0.01,
        );
        let glow = falloff(
            ((x - 0.68).powi(2) * 3.2 + (y - 0.52).powi(2)).sqrt(),
            0.0,
            0.5,
        );
        let mut colour = mix(sky, [1.0, 0.86, 0.55], glow * 0.5);
        colour = mix(colour, [1.0, 0.97, 0.82], sun);
        let far = 0.70 + 0.05 * (x * 9.0).sin() + 0.02 * (x * 23.0).cos();
        colour = mix(colour, [0.36, 0.24, 0.42], falloff(far - y, 0.0, 0.006));
        let near = 0.84 + 0.06 * (x * 5.0 + 1.0).sin();
        colour = mix(colour, [0.13, 0.11, 0.24], falloff(near - y, 0.0, 0.006));
        [colour[0], colour[1], colour[2], 1.0]
    })
}

/// A portrait-shaped stand-in for a photo of a person. Taller than wide, so
/// a circular avatar has to crop it.
fn portrait() -> Vec<u8> {
    paint(180, 260, |x, y| {
        let background = mix([0.20, 0.55, 0.60], [0.10, 0.28, 0.42], y);
        let aspect = 260.0 / 180.0;
        let head = ((x - 0.5).powi(2) + ((y - 0.40) * aspect).powi(2)).sqrt();
        let body = ((x - 0.5).powi(2) * 0.55 + ((y - 1.0) * aspect).powi(2) * 0.6).sqrt();
        let mut colour = mix(background, [0.95, 0.92, 0.86], falloff(body, 0.36, 0.01));
        colour = mix(colour, [0.98, 0.80, 0.66], falloff(head, 0.23, 0.01));
        [colour[0], colour[1], colour[2], 1.0]
    })
}

/// A white five-pointed star on transparency, with a soft edge: an icon to
/// be tinted.
fn star() -> Vec<u8> {
    paint(96, 96, |x, y| {
        let (dx, dy) = (x - 0.5, y - 0.53);
        // Angle from straight up, folded into the tenth of a turn between a
        // tip (0) and the notch beside it.
        let sector = std::f32::consts::TAU / 5.0;
        let turned = dx.atan2(-dy).rem_euclid(sector);
        let folded = turned.min(sector - turned);
        // The edge is the straight line from tip to notch, in polar form.
        let (tip, notch, half) = (0.47, 0.20, sector / 2.0);
        let edge = tip * notch * half.sin() / (tip * folded.sin() + notch * (half - folded).sin());
        let coverage = falloff((dx * dx + dy * dy).sqrt(), edge - 0.01, 0.02);
        // The transparent pixels carry magenta: it must never show.
        if coverage == 0.0 {
            [1.0, 0.0, 1.0, 0.0]
        } else {
            [1.0, 1.0, 1.0, coverage]
        }
    })
}

/// A sprite sheet of four 32-pixel icons in a row: disc, ring, square, bars.
fn sprites() -> Vec<u8> {
    paint(128, 32, |x, y| {
        let cell = (x * 4.0).floor();
        let (u, v) = (x * 4.0 - cell - 0.5, y - 0.5);
        let radius = (u * u + v * v).sqrt();
        let coverage = match cell as u32 {
            0 => falloff(radius, 0.36, 0.04),
            1 => falloff((radius - 0.30).abs(), 0.07, 0.04),
            2 => falloff(u.abs().max(v.abs()), 0.32, 0.04),
            _ => {
                let bar = ((v + 0.5) * 3.0).fract();
                if u.abs() < 0.38 && (0.2..0.8).contains(&bar) {
                    1.0
                } else {
                    0.0
                }
            }
        };
        [1.0, 1.0, 1.0, coverage]
    })
}

/// A checkerboard with a coloured cross through its centre, so cropping and
/// scrolling are easy to see.
fn checkerboard() -> Vec<u8> {
    paint(64, 64, |x, y| {
        let dark = ((x * 8.0) as u32 + (y * 8.0) as u32).is_multiple_of(2);
        let base = if dark {
            [0.20, 0.22, 0.28]
        } else {
            [0.92, 0.93, 0.95]
        };
        let cross = (x - 0.5).abs() < 0.04 || (y - 0.5).abs() < 0.04;
        let colour = if cross { [0.95, 0.30, 0.25] } else { base };
        [colour[0], colour[1], colour[2], 1.0]
    })
}

/// A soft white glow fading to nothing: translucency over its whole area.
fn glow() -> Vec<u8> {
    paint(64, 64, |x, y| {
        let radius = ((x - 0.5).powi(2) + (y - 0.5).powi(2)).sqrt() * 2.0;
        let alpha = (1.0 - radius).clamp(0.0, 1.0).powi(2);
        // Transparent pixels are black here: a straight-alpha filter would
        // turn the glow's rim grey.
        if alpha == 0.0 {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            [1.0, 1.0, 1.0, alpha]
        }
    })
}

/// A glyph atlas of text-like marks: row 0 holds "letters" of four widths,
/// each a rounded blob 12 texels tall.
const GLYPH_HEIGHT: u32 = 12;
const GLYPH_WIDTHS: [u32; 4] = [5, 8, 3, 10];

fn glyph_atlas() -> (u32, u32, Vec<u8>) {
    let width: u32 = GLYPH_WIDTHS.iter().sum();
    let mut pixels = vec![0u8; (width * GLYPH_HEIGHT) as usize];
    let mut left = 0;
    for glyph_width in GLYPH_WIDTHS {
        for y in 0..GLYPH_HEIGHT {
            for x in 0..glyph_width {
                let edge_x = x.min(glyph_width - 1 - x);
                let edge_y = y.min(GLYPH_HEIGHT - 1 - y);
                let corner = edge_x == 0 && edge_y == 0;
                pixels[(y * width + left + x) as usize] = if corner { 96 } else { 255 };
            }
        }
        left += glyph_width;
    }
    (width, GLYPH_HEIGHT, pixels)
}

/// Draws a line of text-like glyphs from the atlas above, at `scale`, up to
/// `max_width`. Returns nothing: the point is glyph operations in the list.
fn scribble(list: &mut DisplayList, origin: Point, max_width: f32, scale: f32, color: Color) {
    let mut x = origin.x;
    let mut index = 0usize;
    loop {
        let which = (index * 7 + 3) % GLYPH_WIDTHS.len();
        let width = GLYPH_WIDTHS[which] as f32 * scale;
        if x + width > origin.x + max_width {
            break;
        }
        // A gap every few glyphs reads as a space between words.
        if index % 5 != 4 {
            let atlas_x: u32 = GLYPH_WIDTHS[..which].iter().sum();
            list.draw_glyph(
                Rect::new(x, origin.y, width, GLYPH_HEIGHT as f32 * scale),
                Rect::new(
                    atlas_x as f32,
                    0.0,
                    GLYPH_WIDTHS[which] as f32,
                    GLYPH_HEIGHT as f32,
                ),
                color,
            );
        }
        x += width + 2.0 * scale;
        index += 1;
    }
}

fn register(frame: &mut Frame, width: u32, height: u32, pixels: Vec<u8>) -> Image {
    // Through PNG, as an app's bundled assets would come.
    frame
        .add_image_png(&rgba_png(width, height, &pixels))
        .expect("scene image")
}

/// The scene: every way of drawing an image, in the company of the other
/// drawing operations. Checked in a few places and dumped for the eye.
#[test]
fn image_scene() {
    let size = SurfaceSize::new(420, 660);
    let mut frame = Frame::new();
    let landscape = register(&mut frame, 480, 270, landscape());
    let portrait = register(&mut frame, 180, 260, portrait());
    let star = register(&mut frame, 96, 96, star());
    let sprites = register(&mut frame, 128, 32, sprites());
    let checks = register(&mut frame, 64, 64, checkerboard());
    let glow = register(&mut frame, 64, 64, glow());

    let ink = srgb(24, 28, 40);
    let muted = with_alpha(ink, 0.45);
    let white = Color::WHITE;
    frame.clear = srgb(236, 238, 243);
    let mut list = DisplayList::new();

    // Header: a circular avatar cropped from a portrait-shaped image, with
    // a ring, and two lines of text beside it.
    let avatar = Rect::new(24.0, 24.0, 64.0, 64.0);
    list.draw_image_fit(portrait, avatar, ImageFit::Cover, 32.0, white);
    list.stroke_rounded_rect(avatar, 32.0, Border::new(2.0, white));
    scribble(&mut list, Point::new(104.0, 34.0), 200.0, 1.5, ink);
    scribble(&mut list, Point::new(104.0, 62.0), 150.0, 1.0, muted);
    // A small avatar from the same image: reduced six times.
    list.draw_image_fit(
        portrait,
        Rect::new(366.0, 38.0, 30.0, 30.0),
        ImageFit::Cover,
        15.0,
        white,
    );

    // Card: shadow, white body, a cover-fitted photo cut by the card's
    // rounded clip, a scrim and a caption over the photo, text below.
    let card = Rect::new(24.0, 112.0, 372.0, 262.0);
    list.draw_shadow(
        card,
        22.0,
        Shadow::new(0.0, 10.0, 28.0, Color::rgba(0.0, 0.0, 0.1, 0.28)),
    );
    list.fill_rounded_rect(card, 22.0, white);
    let photo = Rect::new(card.x, card.y, card.width, 190.0);
    list.push_rounded_clip(card, 22.0);
    list.draw_image_fit(landscape, photo, ImageFit::Cover, 0.0, white);
    list.fill_gradient_rect(
        photo,
        0.0,
        LinearGradient::new(
            Point::new(0.0, photo.y + 110.0),
            Point::new(0.0, photo.y + photo.height),
            Color::rgba(0.0, 0.0, 0.0, 0.0),
            Color::rgba(0.0, 0.0, 0.0, 0.6),
        ),
    );
    scribble(
        &mut list,
        Point::new(card.x + 18.0, photo.y + 158.0),
        220.0,
        1.5,
        white,
    );
    // A tinted star in the photo's corner, over a translucent chip.
    let chip = Rect::new(card.x + card.width - 54.0, card.y + 14.0, 40.0, 40.0);
    list.fill_rounded_rect(chip, 20.0, Color::rgba(0.0, 0.0, 0.0, 0.35));
    list.draw_image_region(
        star,
        star.bounds(),
        Rect::new(chip.x + 8.0, chip.y + 8.0, 24.0, 24.0),
        0.0,
        srgb(255, 205, 60),
    );
    list.pop_clip();
    scribble(
        &mut list,
        Point::new(card.x + 18.0, card.y + 208.0),
        300.0,
        1.0,
        ink,
    );
    scribble(
        &mut list,
        Point::new(card.x + 18.0, card.y + 230.0),
        240.0,
        1.0,
        muted,
    );

    // A row of one icon in five tints, each on a chip, then the four
    // sprites of a sheet, tinted, the last at half opacity.
    let tints = [
        srgb(225, 45, 60),
        srgb(240, 150, 20),
        srgb(30, 160, 90),
        srgb(40, 110, 230),
        srgb(140, 70, 210),
    ];
    for (index, tint) in tints.into_iter().enumerate() {
        let cell = Rect::new(24.0 + index as f32 * 46.0, 396.0, 38.0, 38.0);
        list.fill_rounded_rect(cell, 10.0, with_alpha(tint, 0.12));
        list.draw_image_region(
            star,
            star.bounds(),
            Rect::new(cell.x + 5.0, cell.y + 5.0, 28.0, 28.0),
            0.0,
            tint,
        );
    }
    for index in 0..4 {
        let opacity = if index == 3 { 0.5 } else { 1.0 };
        list.draw_image_region(
            sprites,
            Rect::new(index as f32 * 32.0, 0.0, 32.0, 32.0),
            Rect::new(262.0 + index as f32 * 34.0, 399.0, 32.0, 32.0),
            0.0,
            with_alpha(ink, opacity),
        );
    }

    // Bottom left: an image twice the size of a viewport that clips it,
    // scrolled so its centre cross is off-centre. Then the same image
    // contained in a wide box, letterboxed.
    let viewport = Rect::new(24.0, 458.0, 120.0, 120.0);
    list.fill_rounded_rect(viewport, 14.0, ink);
    list.push_rounded_clip(viewport, 14.0);
    list.draw_image(
        checks,
        Rect::new(viewport.x - 70.0, viewport.y - 40.0, 240.0, 240.0),
    );
    list.pop_clip();
    list.stroke_rounded_rect(viewport, 14.0, Border::new(1.0, muted));
    let letterbox = Rect::new(24.0, 596.0, 120.0, 40.0);
    list.fill_rect(letterbox, ink);
    list.draw_image_fit(checks, letterbox, ImageFit::Contain, 0.0, white);

    // Bottom right: ordering. From back to front: a dark panel, a glow
    // image, a rectangle, an image over the rectangle, a rectangle over the
    // image, text over that, and a half-transparent image over everything.
    let panel = Rect::new(164.0, 458.0, 232.0, 178.0);
    list.fill_rounded_rect(panel, 14.0, srgb(22, 24, 44));
    list.draw_image(glow, Rect::new(panel.x + 6.0, panel.y + 4.0, 170.0, 170.0));
    list.fill_rounded_rect(
        Rect::new(panel.x + 20.0, panel.y + 24.0, 110.0, 70.0),
        8.0,
        srgb(50, 120, 235),
    );
    list.draw_image_fit(
        landscape,
        Rect::new(panel.x + 60.0, panel.y + 50.0, 110.0, 80.0),
        ImageFit::Cover,
        10.0,
        white,
    );
    list.fill_rounded_rect(
        Rect::new(panel.x + 110.0, panel.y + 96.0, 100.0, 50.0),
        8.0,
        srgb(246, 246, 250),
    );
    scribble(
        &mut list,
        Point::new(panel.x + 118.0, panel.y + 104.0),
        86.0,
        1.0,
        ink,
    );
    scribble(
        &mut list,
        Point::new(panel.x + 118.0, panel.y + 124.0),
        60.0,
        1.0,
        muted,
    );
    list.draw_image_region(
        portrait,
        portrait.bounds(),
        Rect::new(panel.x + 150.0, panel.y + 20.0, 64.0, 92.0),
        8.0,
        Color::rgba(1.0, 1.0, 1.0, 0.5),
    );

    let (atlas_width, atlas_height, atlas_pixels) = glyph_atlas();
    let atlas = AtlasView::new(atlas_width, atlas_height, &atlas_pixels, u64::MAX);
    let Some(picture) = render(size, &frame, &list, atlas) else {
        return;
    };
    dump(&picture, "image_scene");

    let background = picture.at(2, 2);
    let near = |a: [u8; 4], b: [u8; 4], tolerance: u8| {
        a.iter().zip(b).all(|(&x, y)| x.abs_diff(y) <= tolerance)
    };
    // The avatar is round: its bounding box's corner is background, its
    // centre is the portrait's face.
    assert_eq!(picture.at(26, 26), background);
    assert_eq!(picture.at(86, 86), background);
    let face = picture.at(56, 50);
    assert!(
        face[0] > 200 && face[1] > 120 && face[2] < face[0],
        "{face:?}"
    );
    // The card's corner is cut out of the photo; inside, the sky is blue.
    assert_ne!(
        picture.at(card.x as u32 + 1, card.y as u32 + 1),
        picture.at(60, 130)
    );
    let sky = picture.at(60, 130);
    assert!(sky[2] > sky[0] && sky[2] > sky[1], "{sky:?}");
    // Tinted icons: the centre of the first star is the first tint, with no
    // trace of the magenta its transparent pixels carry.
    let first = picture.at(24 + 19, 396 + 20);
    // The target holds the colour sRGB-encoded (ADR 0023).
    let byte = |channel: f32| (channel * 255.0).round() as u8;
    let [r, g, b, _] = tints[0].srgb();
    assert!(
        near(first, [byte(r), byte(g), byte(b), 255], 2),
        "{first:?}"
    );
    for x in 24..62 {
        for y in 396..434 {
            let pixel = picture.at(x, y);
            assert!(
                !(pixel[2] > pixel[1].saturating_add(40) && pixel[0] > 150 && pixel[2] > 150),
                "magenta fringe at ({x}, {y}): {pixel:?}"
            );
        }
    }
    // The clipped checkerboard stays inside its viewport.
    assert_eq!(picture.at(150, 500), background);
    assert_eq!(picture.at(20, 500), background);
    // The glow's rim fades into the panel without a darker or lighter band:
    // along a ray from its centre, light only ever decreases.
    let mut previous = 255;
    for step in 0..80 {
        let value = picture.at(panel.x as u32 + 91, panel.y as u32 + 89 - step)[1];
        if step > 66 {
            assert!(
                value <= previous.max(4),
                "glow rim brightens at step {step}"
            );
        }
        previous = value;
    }
    // Ordering: the light rectangle covers the landscape, and the text on
    // it is dark; the landscape covers the blue rectangle.
    let light = picture.at(panel.x as u32 + 200, panel.y as u32 + 140);
    // The scene's `srgb(246, 246, 250)` and `srgb(50, 120, 235)`: the
    // target holds the bytes a screen shows (ADR 0023), which are the
    // colours as written. (They were the linear values 234, 234, 244 and
    // 8, 47, 213 while the target was linear.)
    assert!(near(light, [246, 246, 250, 255], 1), "{light:?}");
    let over_blue = picture.at(panel.x as u32 + 100, panel.y as u32 + 80);
    assert!(!near(over_blue, [50, 120, 235, 255], 40), "{over_blue:?}");
    assert_eq!(frame.images().len(), 6);
}

/// Writes a rendered frame as an sRGB PNG, if asked to.
fn dump(picture: &Picture, name: &str) {
    let Some(dir) = std::env::var_os("MORNINGSTAR_DUMP_DIR") else {
        return;
    };
    // The target is linear; encode to sRGB so it looks as on a screen.
    let encode = |linear: u8| {
        let v = f32::from(linear) / 255.0;
        let srgb = if v <= 0.003_130_8 {
            v * 12.92
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        };
        (srgb * 255.0).round() as u8
    };
    let rgb: Vec<u8> = picture
        .pixels
        .chunks_exact(4)
        .flat_map(|pixel| [encode(pixel[0]), encode(pixel[1]), encode(pixel[2])])
        .collect();
    let file = encode_png(
        picture.width,
        picture.height,
        png::ColorType::Rgb,
        png::BitDepth::Eight,
        None,
        &rgb,
    );
    let path = std::path::Path::new(&dir).join(format!("{name}.png"));
    std::fs::write(&path, file).expect("write dump");
}
