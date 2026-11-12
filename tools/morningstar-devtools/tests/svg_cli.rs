//! The `morningstar-svg` binary and `render_svg`, on the sample files of
//! `tests/svg`: the styles of the common icon sets, an illustration and a
//! drawing program's export.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use morningstar_devtools::{Image, SvgBackground, color_from_srgb, render_svg};
use morningstar_runtime::SvgPicture;

fn samples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/svg")
}

fn run(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_morningstar-svg"))
        .current_dir(samples())
        .args(arguments)
        .output()
        .expect("the binary runs")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

/// A directory for generated files, removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("morningstar-svg-cli-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn file(&self, name: &str) -> String {
        self.0.join(name).to_str().unwrap().to_owned()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn load(name: &str) -> SvgPicture {
    let source = std::fs::read_to_string(samples().join(name)).unwrap();
    SvgPicture::parse(&source).unwrap_or_else(|error| panic!("{name}: {error}"))
}

#[test]
fn every_sample_loads_strictly_and_draws_something() {
    for (name, single_ink) in [
        ("stroke-bell.svg", true),
        ("stroke-settings.svg", true),
        ("filled-home.svg", true),
        ("filled-heart.svg", true),
        ("illustration.svg", false),
        ("drawing-program.svg", false),
    ] {
        let picture = load(name);
        assert_eq!(picture.is_single_ink(), single_ink, "{name}");
        let image = render_svg(&picture, 96, 96, SvgBackground::Transparent);
        let covered = image.rgba().chunks_exact(4).filter(|p| p[3] > 0).count();
        assert!(covered > 500, "{name}: {covered} pixels");
    }
}

#[test]
fn the_icon_samples_look_like_what_they_are() {
    // The bell: an outline. Its body is empty inside, its rim is inked.
    let bell = render_svg(&load("stroke-bell.svg"), 96, 96, SvgBackground::Transparent);
    assert_eq!(bell.pixel(48, 40)[3], 0, "inside the bell");
    assert_eq!(
        bell.pixel(48, 68),
        [0, 0, 0, 255],
        "the rim, 2 units wide at y 17"
    );
    assert_eq!(bell.pixel(4, 4)[3], 0);
    // In a colour.
    let blue = load("stroke-bell.svg").with_current_color(color_from_srgb([0x1D, 0x35, 0x57]));
    let blue = render_svg(&blue, 96, 96, SvgBackground::Color([255, 255, 255]));
    assert_eq!(blue.pixel(48, 68), [0x1D, 0x35, 0x57, 255]);
    assert_eq!(blue.pixel(48, 40), [255, 255, 255, 255]);

    // The house: solid, with its door cut out and nothing from the
    // `fill="none"` rectangle that covers the whole box.
    let home = render_svg(&load("filled-home.svg"), 96, 96, SvgBackground::Transparent);
    assert_eq!(home.pixel(48, 30), [0, 0, 0, 255], "the roof");
    assert_eq!(home.pixel(48, 70)[3], 0, "the door");
    assert_eq!(home.pixel(30, 70), [0, 0, 0, 255], "the wall");
    assert_eq!(home.pixel(2, 2)[3], 0);

    // The heart: an outline by the even-odd rule over a 30% tone.
    let heart = render_svg(
        &load("filled-heart.svg"),
        96,
        96,
        SvgBackground::Transparent,
    );
    assert_eq!(heart.pixel(48, 48)[3], 77, "the faded inside: 0.3 of 255");
    assert_eq!(heart.pixel(48, 82)[3], 255, "the outline at the tip");
}

#[test]
fn the_illustration_has_its_gradients_clip_and_faded_group() {
    let image = render_svg(
        &load("illustration.svg"),
        640,
        480,
        SvgBackground::Transparent,
    );
    // Outside the round window: nothing, though the sky's rect is there.
    assert_eq!(image.pixel(8, 8)[3], 0);
    assert_eq!(image.pixel(630, 470)[3], 0);
    // The sky runs from navy at the top towards orange.
    let top = image.pixel(320, 40);
    let lower = image.pixel(100, 230);
    assert!(top[2] > top[0], "navy at the top: {top:?}");
    assert!(lower[0] > lower[2] + 60, "orange lower down: {lower:?}");
    // The sun glows above the hills, brightest towards its middle.
    let sun = image.pixel(304, 240);
    assert!(sun[0] > 240 && sun[1] > 195, "{sun:?}");
    // The sea is opaque and dark; a dash of the glint is on it.
    let sea = image.pixel(100, 420);
    assert!(sea[3] == 255 && sea[0] < 60, "{sea:?}");
    let dash = image.pixel(228, 344);
    let gap = image.pixel(258, 344);
    assert!(dash[0] > 180 && dash[1] > 150, "a dash: {dash:?}");
    assert!(gap[0] < 80, "a gap: {gap:?}");
    // A hill: green, with the sky showing through its 60%.
    let hill = image.pixel(560, 308);
    assert!(
        hill[1] > hill[0] && hill[1] > hill[2] && hill[3] == 255,
        "{hill:?}"
    );
}

#[test]
fn the_binary_writes_a_png_and_says_what_it_read() {
    let scratch = Scratch::new("render");
    let out = scratch.file("bell.png");
    let output = run(&[
        "stroke-bell.svg",
        &out,
        "--scale",
        "4",
        "--color",
        "#E63946",
    ]);
    assert!(output.status.success(), "{}", text(&output.stderr));
    let said = text(&output.stdout);
    assert!(
        said.contains("24 x 24 units, view box 0 0 24 24, single-ink"),
        "{said}"
    );
    assert!(said.contains("96 x 96 px"), "{said}");
    let image = Image::load(&out).unwrap();
    assert_eq!((image.width(), image.height()), (96, 96));
    assert_eq!(image.pixel(48, 68), [0xE6, 0x39, 0x46, 255]);
    assert_eq!(image.pixel(48, 40)[3], 0, "transparent by default");

    // A size and a background: fitted and centred.
    let out = scratch.file("picture.png");
    let output = run(&[
        "illustration.svg",
        &out,
        "--size",
        "200x200",
        "--background",
        "#FFFFFF",
    ]);
    assert!(output.status.success(), "{}", text(&output.stderr));
    assert!(text(&output.stdout).contains("multi-colour"));
    let image = Image::load(&out).unwrap();
    assert_eq!((image.width(), image.height()), (200, 200));
    assert!(image.is_opaque());
    assert_eq!(
        image.pixel(100, 10),
        [255, 255, 255, 255],
        "above the 4:3 picture"
    );
    assert_ne!(image.pixel(100, 100), [255, 255, 255, 255]);

    let out = scratch.file("checker.png");
    assert!(
        run(&[
            "filled-home.svg",
            &out,
            "--size",
            "64",
            "--background",
            "checker"
        ])
        .status
        .success()
    );
    let image = Image::load(&out).unwrap();
    assert_eq!(image.pixel(1, 1), [0xC8, 0xC8, 0xC8, 255]);
    assert_eq!(image.pixel(9, 1), [0x90, 0x90, 0x90, 255]);
}

#[test]
fn the_binary_refuses_a_file_outside_the_subset_and_says_where() {
    let scratch = Scratch::new("refuse");
    let out = scratch.file("text.png");
    let output = run(&["unsupported-text.svg", &out]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        text(&output.stderr).trim(),
        "error: unsupported-text.svg:3:3: <text> is not supported"
    );
    assert!(!Path::new(&out).exists(), "nothing is written");

    // Leniently: the rest is drawn and the omission is listed.
    let output = run(&["unsupported-text.svg", &out, "--lenient", "--scale", "2"]);
    assert!(output.status.success());
    assert!(
        text(&output.stderr)
            .contains("left out: unsupported-text.svg:3:3: <text> is not supported")
    );
    let image = Image::load(&out).unwrap();
    assert_eq!((image.width(), image.height()), (96, 48));
    assert_eq!(image.pixel(48, 24), [0x1D, 0x35, 0x57, 255]);
}

#[test]
fn the_binary_reports_usage_and_file_errors() {
    for (arguments, message) in [
        (&["only.svg"][..], "expected an SVG file and the PNG"),
        (&["a.svg", "b.png", "--size", "0"][..], "--size 0"),
        (
            &["a.svg", "b.png", "--size", "9000x10"][..],
            "--size 9000x10",
        ),
        (&["a.svg", "b.png", "--scale", "-1"][..], "--scale -1"),
        (
            &["a.svg", "b.png", "--background", "blue"][..],
            "--background blue",
        ),
        (&["a.svg", "b.png", "--color", "red"][..], "--color red"),
        (&["a.svg", "b.png", "--frobnicate"][..], "unknown option"),
        (&["missing.svg", "b.png"][..], "missing.svg"),
    ] {
        let output = run(arguments);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        assert!(
            text(&output.stderr).contains(message),
            "{arguments:?}: {}",
            text(&output.stderr)
        );
    }
    let output = run(&["--help"]);
    assert!(output.status.success());
    assert!(text(&output.stdout).contains("Usage: morningstar-svg"));
}
