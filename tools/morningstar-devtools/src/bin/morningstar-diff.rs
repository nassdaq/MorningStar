//! Compares a screenshot with a reference image.
//!
//!   cargo run -p morningstar-devtools --bin morningstar-diff -- \
//!       design.png screenshot.png --out diff.png --side-by-side sbs.png
//!
//! Prints a report and exits with 0 when the images match within the
//! tolerance, 1 when they do not, and 2 when it could not compare them.

use std::path::PathBuf;
use std::process::ExitCode;

use morningstar_devtools::{CompareOptions, Fit, Image, Rect, Tolerance, compare};

const USAGE: &str = "\
Compares a candidate image (a screenshot) with a reference (a design).

Usage: morningstar-diff <reference.png> <candidate.png> [options]

The reference is resampled to the candidate's size; all coordinates in the
report and in the options are candidate pixels, from the top-left corner.

Options:
  --out <file>             write a heat map PNG (candidate in grey, differences
                           yellow to red, listed regions outlined and numbered)
  --side-by-side <file>    write reference | candidate | heat map as one PNG
  --threshold <error>      smallest colour error that counts as a difference;
                           0 identical, 2 just visible, 100 black vs white
                           [default: 2]
  --aa-radius <pixels>     accept edges that moved by up to this many pixels or
                           are anti-aliased differently; 0 is strict [default: 1]
  --max-diff <percent>     pass when at most this percentage of the compared
                           pixels differ [default: 0.1]
  --ignore <x,y,w,h>       leave a rectangle out of the comparison; repeatable
  --fit <width|stretch>    on an aspect-ratio mismatch: scale the reference to
                           the candidate's width and align the tops, or distort
                           it to cover the candidate [default: width]
  --allow-aspect-mismatch  do not fail just because the aspect ratios differ
  --max-regions <count>    how many regions to list [default: 10]
  --json                   print the report as JSON instead of text
  -h, --help               print this help

Exit status: 0 the images match, 1 they differ, 2 usage or file error.
";

struct Arguments {
    reference: PathBuf,
    candidate: PathBuf,
    out: Option<PathBuf>,
    side_by_side: Option<PathBuf>,
    json: bool,
    options: CompareOptions,
}

enum Parsed {
    Run(Box<Arguments>),
    Help,
}

fn parse(arguments: impl Iterator<Item = String>) -> Result<Parsed, String> {
    let mut arguments = arguments.peekable();
    let mut paths = Vec::new();
    let mut out = None;
    let mut side_by_side = None;
    let mut json = false;
    let mut options = CompareOptions {
        tolerance: Tolerance::DEFAULT,
        ..CompareOptions::default()
    };

    while let Some(argument) = arguments.next() {
        // Accept both `--name value` and `--name=value`.
        let (name, inline) = match argument.split_once('=') {
            Some((name, value)) if name.starts_with("--") => {
                (name.to_owned(), Some(value.to_owned()))
            }
            _ => (argument.clone(), None),
        };
        let mut value = |what: &str| {
            inline
                .clone()
                .or_else(|| arguments.next())
                .ok_or_else(|| format!("{name} needs {what}"))
        };
        match name.as_str() {
            "-h" | "--help" => return Ok(Parsed::Help),
            "--out" | "-o" => out = Some(PathBuf::from(value("a file path")?)),
            "--side-by-side" => side_by_side = Some(PathBuf::from(value("a file path")?)),
            "--threshold" => {
                options.tolerance.threshold = number(&name, &value("a number")?, 0.0, 200.0)?;
            }
            "--max-diff" => {
                options.tolerance.max_diff_percent =
                    number(&name, &value("a percentage")?, 0.0, 100.0)?;
            }
            "--aa-radius" => {
                options.tolerance.aa_radius = integer(&name, &value("a pixel count")?, 4)?;
            }
            "--max-regions" => {
                options.max_regions = integer(&name, &value("a count")?, 1000)? as usize;
            }
            "--ignore" => options.ignore.push(rectangle(&value("x,y,w,h")?)?),
            "--fit" => {
                options.fit = match value("width or stretch")?.as_str() {
                    "width" => Fit::Width,
                    "stretch" => Fit::Stretch,
                    other => return Err(format!("--fit is width or stretch, not {other:?}")),
                };
            }
            "--allow-aspect-mismatch" => options.allow_aspect_mismatch = true,
            "--json" => json = true,
            flag if flag.starts_with('-') && flag.len() > 1 => {
                return Err(format!("unknown option {flag}"));
            }
            _ => paths.push(PathBuf::from(argument)),
        }
    }

    let [reference, candidate] = <[PathBuf; 2]>::try_from(paths).map_err(|paths| {
        format!(
            "expected a reference and a candidate image, got {} path(s)",
            paths.len()
        )
    })?;
    Ok(Parsed::Run(Box::new(Arguments {
        reference,
        candidate,
        out,
        side_by_side,
        json,
        options,
    })))
}

fn number(name: &str, text: &str, min: f32, max: f32) -> Result<f32, String> {
    match text.parse::<f32>() {
        Ok(value) if value >= min && value <= max => Ok(value),
        _ => Err(format!(
            "{name} must be a number from {min} to {max}, not {text:?}"
        )),
    }
}

fn integer(name: &str, text: &str, max: u32) -> Result<u32, String> {
    match text.parse::<u32>() {
        Ok(value) if value <= max => Ok(value),
        _ => Err(format!(
            "{name} must be a whole number from 0 to {max}, not {text:?}"
        )),
    }
}

fn rectangle(text: &str) -> Result<Rect, String> {
    let parts: Vec<Option<u32>> = text
        .split(',')
        .map(|part| part.trim().parse().ok())
        .collect();
    match parts[..] {
        [Some(x), Some(y), Some(width), Some(height)] => Ok(Rect::new(x, y, width, height)),
        _ => Err(format!(
            "--ignore takes x,y,w,h as whole pixels, not {text:?}"
        )),
    }
}

fn run(arguments: &Arguments) -> Result<bool, String> {
    let load = |role: &str, path: &PathBuf| {
        Image::load(path).map_err(|error| format!("cannot read the {role} image: {error}"))
    };
    let reference = load("reference", &arguments.reference)?;
    let candidate = load("candidate", &arguments.candidate)?;

    let comparison = compare(&reference, &candidate, &arguments.options);
    if arguments.json {
        print!("{}", comparison.report.to_json());
    } else {
        print!("{}", comparison.report.to_text());
    }

    let save = |image: Image, path: &PathBuf| {
        image
            .save(path)
            .map_err(|error| format!("cannot write {error}"))
    };
    if let Some(path) = &arguments.out {
        save(comparison.heat_map(), path)?;
    }
    if let Some(path) = &arguments.side_by_side {
        save(comparison.side_by_side(), path)?;
    }
    Ok(comparison.report.passed)
}

fn main() -> ExitCode {
    let arguments = match parse(std::env::args().skip(1)) {
        Ok(Parsed::Run(arguments)) => arguments,
        Ok(Parsed::Help) => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("morningstar-diff: {message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match run(&arguments) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(message) => {
            eprintln!("morningstar-diff: {message}");
            ExitCode::from(2)
        }
    }
}
