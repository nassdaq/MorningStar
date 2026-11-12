//! An app's launcher icons and names, from what it declares in its
//! `Cargo.toml` (ADR 0020).
//!
//!   cargo run -q -p morningstar-devtools --bin morningstar-icon -- \
//!       callready --android templates/android/app/build/morningstar/res
//!
//! `tools/build-android.sh` and `tools/build-ios.sh` run this; run it by
//! hand with `--preview` to look at an icon before it is on a phone.

use std::path::PathBuf;
use std::process::ExitCode;

use morningstar_devtools::{IconSet, Identity, cargo_metadata, write_generated};

const USAGE: &str = "\
Generates an app's launcher icons, and prints its name and ids, from the
[package.metadata.morningstar] table of its Cargo.toml:

    [package.metadata.morningstar]
    name = \"CallReady\"                # under the icon (default: the package name)
    id = \"dev.morningstar.callready\"  # application id (default: dev.morningstar.<lib>)
    icon = \"assets/icon.png\"          # square PNG, 512 or more, best 1024, full-bleed
                                      # (default: this path if the file exists,
                                      # otherwise the built-in Morningstar icon)
    icon_foreground = \"assets/icon-foreground.png\"
                                      # optional Android adaptive foreground: square,
                                      # transparent, content in the middle 61%
    icon_background = \"#0E0E0E\"       # optional colour behind the foreground
                                      # (default: the icon's top-left pixel)

Usage: morningstar-icon <package> [options]

Options:
  --android <dir>        write an Android resource directory: mipmap-*/ic_launcher
                         (.png, _round.png, _foreground.png), the adaptive icon in
                         mipmap-anydpi-v26 and its colour in values
  --ios <dir>            write Assets.xcassets (AppIcon: one 1024x1024 opaque PNG)
                         and Morningstar.xcconfig (bundle id and names) for the iOS
                         host. Not verified with Xcode.
  --preview <file.png>   write one picture of the icon in every form: square,
                         round, and adaptive under a circle and a squircle, at
                         192, 96 and 48 pixels
  --print <what>         print one value and a newline: name, id (Android
                         application id), bundle-id (iOS), lib (the Rust
                         library's name) or icon (the source picture, or
                         `built-in`)
  --manifest-path <file> a Cargo.toml of the workspace, if not the current
                         directory's
  -h, --help             print this help

Files whose content would not change are left untouched.
Exit status: 0 done, 2 usage error or a problem with the manifest or icon.
";

#[derive(Default)]
struct Arguments {
    package: Option<String>,
    android: Option<PathBuf>,
    ios: Option<PathBuf>,
    preview: Option<PathBuf>,
    print: Vec<String>,
    manifest_path: Option<PathBuf>,
}

fn parse(mut arguments: impl Iterator<Item = String>) -> Result<Option<Arguments>, String> {
    let mut parsed = Arguments::default();
    while let Some(argument) = arguments.next() {
        let mut value = || {
            arguments
                .next()
                .ok_or_else(|| format!("{argument} needs a value"))
        };
        match argument.as_str() {
            "-h" | "--help" => return Ok(None),
            "--android" => parsed.android = Some(value()?.into()),
            "--ios" => parsed.ios = Some(value()?.into()),
            "--preview" => parsed.preview = Some(value()?.into()),
            "--manifest-path" => parsed.manifest_path = Some(value()?.into()),
            "--print" => {
                let what = value()?;
                if !["name", "id", "bundle-id", "lib", "icon"].contains(&what.as_str()) {
                    return Err(format!(
                        "--print {what}: expected name, id, bundle-id, lib or icon"
                    ));
                }
                parsed.print.push(what);
            }
            option if option.starts_with('-') => return Err(format!("unknown option {option}")),
            _ if parsed.package.is_none() => parsed.package = Some(argument),
            _ => return Err(format!("unexpected argument {argument}")),
        }
    }
    if parsed.package.is_none() {
        return Err("which package? Give its name first".to_owned());
    }
    Ok(Some(parsed))
}

fn run(arguments: &Arguments) -> Result<(), Box<dyn std::error::Error>> {
    let package = arguments.package.as_deref().unwrap_or_default();
    let metadata = cargo_metadata(arguments.manifest_path.as_deref())?;
    let identity = Identity::from_cargo_metadata(&metadata, package)?;

    // Standard output carries only what --print asked for, so a script can
    // capture it; everything else goes to standard error.
    for what in &arguments.print {
        match what.as_str() {
            "name" => println!("{}", identity.name),
            "id" => println!("{}", identity.id),
            "bundle-id" => println!("{}", identity.bundle_id()),
            "lib" => println!("{}", identity.lib),
            _ => match &identity.icon {
                Some(path) => println!("{}", path.display()),
                None => println!("built-in"),
            },
        }
    }
    if arguments.android.is_none() && arguments.ios.is_none() && arguments.preview.is_none() {
        return Ok(());
    }

    let icons = IconSet::for_identity(&identity)?;
    let source = match &identity.icon {
        Some(path) => path.display().to_string(),
        None => "the built-in icon".to_owned(),
    };
    let outputs = [
        (
            &arguments.android,
            "Android",
            icons.android_files().map(Some),
        ),
        (
            &arguments.ios,
            "iOS",
            match arguments.ios {
                Some(_) => icons.ios_files(&identity).map(Some),
                None => Ok(None),
            },
        ),
    ];
    for (directory, platform, files) in outputs {
        let (Some(directory), Some(files)) = (directory, files?) else {
            continue;
        };
        let done = write_generated(directory, &files)?;
        eprintln!(
            "{platform} icons of {package} (\"{}\", from {source}): {} written, {} unchanged in {}",
            identity.name,
            done.written.len(),
            done.unchanged,
            directory.display()
        );
    }
    if let Some(path) = &arguments.preview {
        icons.preview()?.save(path)?;
        eprintln!("preview: {}", path.display());
    }
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
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(2)
        }
    }
}
