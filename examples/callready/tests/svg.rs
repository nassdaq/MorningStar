//! The design's inline SVGs, loaded as files, against the same pictures
//! transcribed by hand into layers (ADR 0027).
//!
//! The design export spells `viewBox` as `sc-camel-view-box` and puts the
//! page's CSS (`position`, `left`, a CSS `transform`) in a `style`
//! attribute on the root. Both belong to the HTML around the picture, not
//! to the SVG, so they are put right here and not in the loader.

use std::path::Path;

use callready::screens::{home, live_call, welcome};
use morningstar::{SvgPicture, VectorPicture};

/// The home screen's pictures as they were transcribed by hand into layers
/// before the app loaded the design's SVGs (`assets/svg`): what the loader
/// is measured against.
mod by_hand {
    use callready::theme::{BLACK, BLUE, CREAM, GREEN, YELLOW};
    use morningstar::ui::with_alpha;
    use morningstar::{Color, Layer, VectorPicture};

    /// `stroke="#F6C445" stroke-opacity="0.55" stroke-width="1"`: a groove.
    const GROOVE: Color = with_alpha(YELLOW, 0.55);

    /// A groove of the disc at radius `r`.
    const fn groove(r: f32) -> Layer {
        Layer::circle(140.0, 140.0, r).stroke(GROOVE, 1.0)
    }

    /// `<g fill="none" stroke="#0E0E0E" stroke-width="1.2"
    /// clip-path="url(#discClip)">`: a black ring that cuts the band.
    const fn cut(r: f32) -> Layer {
        Layer::circle(140.0, 140.0, r)
            .stroke(BLACK, 1.2)
            .clip_circle(140.0, 140.0, 122.0)
    }

    /// `<g fill="none" stroke="#0E0E0E" stroke-width="3" stroke-linecap="round"
    /// stroke-linejoin="round" transform="translate(122 122)">`: a part of the
    /// headset.
    const fn headset(layer: Layer) -> Layer {
        layer.stroke(BLACK, 3.0).round().translate(122.0, 122.0)
    }

    /// The vinyl disc: `<svg width="250" height="250" viewBox="0 0 280 280">`.
    /// An outer ring, the disc, a band turned by -18 degrees and clipped to the
    /// disc (`<clipPath id="discClip"><circle cx="140" cy="140" r="122"/>`),
    /// ten grooves, five black rings, the cream label and the headset.
    pub const DISC: VectorPicture = VectorPicture::new(
        280.0,
        280.0,
        &[
            // `stroke-opacity="0.25"`.
            Layer::circle(140.0, 140.0, 138.0).stroke(with_alpha(YELLOW, 0.25), 2.0),
            Layer::circle(140.0, 140.0, 122.0)
                .fill(BLACK)
                .stroke(YELLOW, 1.5),
            Layer::rect(-40.0, 114.0, 360.0, 52.0)
                .fill(YELLOW)
                .rotate(-18.0, 140.0, 140.0)
                .clip_circle(140.0, 140.0, 122.0),
            groove(114.0),
            groove(108.0),
            groove(102.0),
            groove(96.0),
            groove(90.0),
            groove(84.0),
            groove(78.0),
            groove(72.0),
            groove(66.0),
            groove(60.0),
            cut(111.0),
            cut(99.0),
            cut(87.0),
            cut(75.0),
            cut(63.0),
            Layer::circle(140.0, 140.0, 48.0)
                .fill(CREAM)
                .stroke(BLACK, 3.0),
            headset(Layer::path("M4 22v-4a14 14 0 0 1 28 0v4")),
            headset(Layer::rounded_rect(2.0, 20.0, 7.0, 11.0, 2.0)),
            headset(Layer::rounded_rect(27.0, 20.0, 7.0, 11.0, 2.0)),
            headset(Layer::path("M30 31c0 4-4 6-10 6")),
        ],
    );

    /// The four rings of one side: `<svg width="64" height="220" viewBox="0 0
    /// 64 220" fill="none" stroke=..>` around a centre outside the view box,
    /// which cuts them where the screen's edge does.
    const fn rings(cx: f32, color: Color) -> [Layer; 4] {
        [
            Layer::circle(cx, 110.0, 104.0).stroke(color, 1.2),
            Layer::circle(cx, 110.0, 96.0).stroke(color, 1.2),
            Layer::circle(cx, 110.0, 78.0).stroke(color, 16.0),
            Layer::circle(cx, 110.0, 62.0).stroke(color, 1.2),
        ]
    }

    /// The blue rings at the left edge: `cx="-48"`, `stroke="#5AA9E6"`.
    pub const RINGS_LEFT: VectorPicture = VectorPicture::new(64.0, 220.0, &rings(-48.0, BLUE));
    /// The green rings at the right edge: `cx="112"`, `stroke="#4CAF50"`.
    pub const RINGS_RIGHT: VectorPicture = VectorPicture::new(64.0, 220.0, &rings(112.0, GREEN));
}

/// The `index`-th inline `<svg>` of a design page, as an SVG file.
fn inline_svg(page: &str, index: usize) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("design/html/{page}.html"));
    let html = std::fs::read_to_string(path).expect("design html");
    let mut rest = html.as_str();
    for _ in 0..index {
        let start = rest.find("<svg").expect("an svg element");
        rest = &rest[start + 4..];
    }
    let start = rest.find("<svg").expect("an svg element");
    let end = rest[start..].find("</svg>").expect("its end") + start + 6;
    let svg = rest[start..end].replace("sc-camel-view-box", "viewBox");
    // The root's `style` is the page's layout.
    let tag_end = svg.find('>').expect("the root tag");
    let (root, body) = svg.split_at(tag_end);
    let root = match root.find(" style=\"") {
        Some(at) => {
            let close = root[at + 8..].find('"').expect("the style's end") + at + 9;
            format!("{}{}", &root[..at], &root[close..])
        }
        None => root.to_owned(),
    };
    format!("{root}{body}")
}

fn inline_svg_count(page: &str) -> usize {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("design/html/{page}.html"));
    std::fs::read_to_string(path)
        .expect("design html")
        .matches("<svg")
        .count()
}

/// How two rasterisations differ: bytes that differ at all, and the
/// largest difference of a byte.
fn difference(a: &[u8], b: &[u8]) -> (usize, u8) {
    assert_eq!(a.len(), b.len());
    let differing = a.iter().zip(b).filter(|(a, b)| a != b).count();
    let largest = a
        .iter()
        .zip(b)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap_or(0);
    (differing, largest)
}

/// Loads a design SVG and compares it with its hand transcription at the
/// design's 2 px per dp and at the test phone's 2.8125.
fn compare(name: &str, page: &str, index: usize, by_hand: &VectorPicture) {
    compare_within(name, page, index, by_hand, 0);
}

/// As [`compare`], allowing `allowed` bytes to differ by one level.
fn compare_within(name: &str, page: &str, index: usize, by_hand: &VectorPicture, allowed: usize) {
    let source = inline_svg(page, index);
    let loaded = SvgPicture::parse(&source).unwrap_or_else(|error| panic!("{name}: {error}"));
    for scale in [2.0f32, 2.8125] {
        let width = (loaded.width() * scale).round() as u32;
        let height = (loaded.height() * scale).round() as u32;
        let ours = loaded.rasterise(width, height);
        let theirs = by_hand.rasterise(width, height);
        let (differing, largest) = difference(&ours, &theirs);
        println!(
            "{name} at {scale}: {width} x {height}, {differing} of {} bytes differ, largest difference {largest}",
            ours.len()
        );
        assert!(
            differing <= allowed && largest <= 1,
            "{name} at {scale}: the loaded SVG and the hand transcription differ \
             in {differing} bytes by up to {largest}"
        );
    }
}

#[test]
fn the_home_screens_svgs_load_into_the_hand_transcribed_pictures() {
    compare("rings left", "02-home", 2, &by_hand::RINGS_LEFT);
    compare("rings right", "02-home", 3, &by_hand::RINGS_RIGHT);
    // The disc's five black rings are one clipped group in the file, which
    // the loader composites as a group, as a browser does; the hand
    // transcription clips each ring by itself. The two round differently
    // by one level in a handful of bytes: 8 of 1,000,000 at 2 px per dp
    // and 31 of 1,976,836 at 2.8125.
    compare_within("disc", "02-home", 4, &by_hand::DISC, 32);
}

/// The files the app bundles are the design's SVGs: the same pictures as
/// the inline ones, byte for byte once rasterised.
#[test]
fn the_bundled_assets_are_the_designs_svgs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/svg");
    for (file, index, in_app) in [
        ("rings-left.svg", 2, None),
        ("rings-right.svg", 3, None),
        ("disc.svg", 4, Some(home::DISC.get())),
    ] {
        let bundled = std::fs::read_to_string(root.join(file)).expect("the asset");
        let bundled = SvgPicture::parse(&bundled).unwrap_or_else(|error| panic!("{file}: {error}"));
        let inline = SvgPicture::parse(&inline_svg("02-home", index)).expect("the design's");
        assert_eq!(
            (bundled.width(), bundled.height()),
            (inline.width(), inline.height())
        );
        let (width, height) = (
            (bundled.width() * 2.0) as u32,
            (bundled.height() * 2.0) as u32,
        );
        assert_eq!(
            bundled.rasterise(width, height),
            inline.rasterise(width, height),
            "{file}"
        );
        if let Some(in_app) = in_app {
            assert_eq!(
                in_app.rasterise(width, height),
                bundled.rasterise(width, height)
            );
        }
    }
    assert!(home::DISC.check().is_ok());
}

#[test]
fn the_welcome_and_live_call_svgs_load_into_the_hand_transcribed_pictures() {
    compare("waveform", "01-welcome", 1, &welcome::WAVEFORM);
    compare("underline", "01-welcome", 2, &welcome::UNDERLINE);
    let avatar = (0..inline_svg_count("03-live-call"))
        .find(|&index| inline_svg("03-live-call", index).contains("width=\"54\""))
        .expect("the avatar disc");
    compare(
        "avatar disc",
        "03-live-call",
        avatar,
        &live_call::AVATAR_DISC,
    );
}

#[test]
fn every_inline_svg_of_the_design_loads_strictly() {
    let mut count = 0;
    for page in [
        "01-welcome",
        "02-home",
        "03-live-call",
        "04-call-report",
        "05-premium",
    ] {
        for index in 0..inline_svg_count(page) {
            let source = inline_svg(page, index);
            // One icon is turned by the page's CSS on its path.
            let source = source.replace(" style=\"transform: rotate(135deg)\"", "");
            match SvgPicture::parse(&source) {
                Ok(picture) => {
                    assert!(picture.width() > 0.0 && picture.height() > 0.0);
                    let pixels = picture.rasterise(48, 48);
                    assert!(
                        pixels.iter().any(|byte| *byte != 0),
                        "{page} #{index} is blank"
                    );
                }
                Err(error) => panic!("{page} #{index}: {error}\n{source}"),
            }
            count += 1;
        }
    }
    println!("{count} inline SVGs loaded");
    assert!(count >= 20, "only {count}");
}
