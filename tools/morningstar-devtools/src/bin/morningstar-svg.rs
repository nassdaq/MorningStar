//! Renders an SVG file to a PNG with the loader applications use
//! (`SvgPicture`, ADR 0027), to look at a designer's file before it is in
//! an app and to see what, if anything, the loader refuses in it.
//!
//!   cargo run -q -p morningstar-devtools --bin morningstar-svg -- \
//!       assets/svg/disc.svg out/disc.png --scale 3

use std::path::PathBuf;
use std::process::ExitCode;

use morningstar_devtools::{SvgBackground, color_from_srgb, render_svg};
use morningstar_runtime::SvgPicture;

const USAGE: &str = "\
Renders an SVG file to a PNG as a Morningstar app would draw it.

Usage: morningstar-svg <file.svg> <out.png> [options]

Options:
  --size <W>[x<H>]       the image's size in pixels; one number is a square. The
                         picture is fitted and centred, as in an app
  --scale <S>            pixels per unit of the file's own size (default 1; a
                         phone is 2 to 3). Ignored with --size
  --background <B>       what is behind the picture: none (default; the PNG is
                         transparent), checker, or a colour as #RRGGBB
  --color <#RRGGBB>      the colour of `currentColor` (default black)
  --lenient              leave out what the loader does not support instead of
                         refusing the file, and list it. For a first look only:
                         an app should load a file that needs no leniency
  -h, --help             print this help

Prints the file's size, whether it is single-ink (an icon that takes any colour
from one mask, see Icon::svg), and what was written.
Exit status: 0 done, 1 the file is outside the supported subset (the message
says what and where: file:line:column), 2 usage or file error.
";

struct Arguments {
    input: PathBuf,
    output: PathBuf,
    size: Option<(u32, u32)>,
    scale: f32,
    background: SvgBackground,
    color: [u8; 3],
    lenient: bool,
}

fn hex_color(text: &str) -> Option<[u8; 3]> {
    let digits = text.strip_prefix('#')?;
    if digits.len() != 6 || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

fn parse(mut arguments: impl Iterator<Item = String>) -> Result<Option<Arguments>, String> {
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut parsed = Arguments {
        input: PathBuf::new(),
        output: PathBuf::new(),
        size: None,
        scale: 1.0,
        background: SvgBackground::Transparent,
        color: [0, 0, 0],
        lenient: false,
    };
    while let Some(argument) = arguments.next() {
        let mut value = || {
            arguments
                .next()
                .ok_or_else(|| format!("{argument} needs a value"))
        };
        match argument.as_str() {
            "-h" | "--help" => return Ok(None),
            "--lenient" => parsed.lenient = true,
            "--size" => {
                let text = value()?;
                let side = |part: &str| {
                    part.parse::<u32>()
                        .ok()
                        .filter(|side| (1..=4096).contains(side))
                };
                let size = match text.split_once('x') {
                    Some((width, height)) => side(width).zip(side(height)),
                    None => side(&text).map(|side| (side, side)),
                };
                parsed.size = Some(
                    size.ok_or_else(|| format!("--size {text}: expected W or WxH, 1 to 4096"))?,
                );
            }
            "--scale" => {
                let text = value()?;
                parsed.scale = text
                    .parse::<f32>()
                    .ok()
                    .filter(|scale| scale.is_finite() && *scale > 0.0)
                    .ok_or_else(|| format!("--scale {text}: expected a positive number"))?;
            }
            "--background" => {
                let text = value()?;
                parsed.background = match text.as_str() {
                    "none" => SvgBackground::Transparent,
                    "checker" => SvgBackground::Checker,
                    other => SvgBackground::Color(hex_color(other).ok_or_else(|| {
                        format!("--background {text}: expected none, checker or #RRGGBB")
                    })?),
                };
            }
            "--color" => {
                let text = value()?;
                parsed.color =
                    hex_color(&text).ok_or_else(|| format!("--color {text}: expected #RRGGBB"))?;
            }
            option if option.starts_with('-') => return Err(format!("unknown option {option}")),
            _ => paths.push(argument.into()),
        }
    }
    let [input, output] = <[PathBuf; 2]>::try_from(paths)
        .map_err(|_| "expected an SVG file and the PNG to write".to_owned())?;
    parsed.input = input;
    parsed.output = output;
    Ok(Some(parsed))
}

/// Exit status 1 for a file the loader refuses, 2 for anything else.
fn run(arguments: &Arguments) -> Result<(), (u8, String)> {
    let name = arguments.input.display();
    let source = std::fs::read_to_string(&arguments.input)
        .map_err(|error| (2, format!("{name}: {error}")))?;
    let refused = |error: morningstar_runtime::SvgError| {
        let detail = error.to_string();
        // "SVG line L, column C: what" as file:L:C: what.
        let what = detail
            .split_once(": ")
            .map_or(detail.as_str(), |(_, what)| what);
        (1, format!("{name}:{}:{}: {what}", error.line, error.column))
    };
    let picture = if arguments.lenient {
        let (picture, notes) = SvgPicture::parse_lenient(&source).map_err(refused)?;
        for note in &notes {
            let (_, message) = refused(note.clone());
            eprintln!("left out: {message}");
        }
        picture
    } else {
        SvgPicture::parse(&source).map_err(refused)?
    };
    let picture = picture.with_current_color(color_from_srgb(arguments.color));

    let (width, height) = arguments.size.unwrap_or_else(|| {
        let side = |units: f32| ((units * arguments.scale).round() as u32).clamp(1, 4096);
        (side(picture.width()), side(picture.height()))
    });
    let image = render_svg(&picture, width, height, arguments.background);
    image
        .save(&arguments.output)
        .map_err(|error| (2, format!("{}: {error}", arguments.output.display())))?;
    let [x, y, box_width, box_height] = picture.view_box();
    println!(
        "{name}: {} x {} units, view box {x} {y} {box_width} {box_height}, {}",
        picture.width(),
        picture.height(),
        if picture.is_single_ink() {
            "single-ink (takes a colour as an icon)"
        } else {
            "multi-colour"
        }
    );
    println!("{}: {width} x {height} px", arguments.output.display());
    Ok(())
}

fn main() -> ExitCode {
    let arguments = match parse(std::env::args().skip(1)) {
        Ok(Some(arguments)) => arguments,
        Ok(None) => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("error: {message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match run(&arguments) {
        Ok(()) => ExitCode::SUCCESS,
        Err((status, message)) => {
            eprintln!("error: {message}");
            ExitCode::from(status)
        }
    }
}
