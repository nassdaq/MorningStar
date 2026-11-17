//! `morningstar`: one command for a Morningstar app outside this
//! repository (ADR 0045).
//!
//! ```sh
//! cargo install morningstar-cli
//! morningstar new my-app --name "My App" --id com.example.myapp
//! cd my-app
//! cargo test
//! morningstar preview
//! morningstar build android --install
//! ```
//!
//! Every command is code this repository already has: the generator, the
//! icon, comparison and SVG tools of `morningstar-devtools`, the preview of
//! `morningstar-preview`, and the Android host of `templates/android`,
//! carried inside this binary so that no checkout is needed.

mod android;
mod app;

use std::path::PathBuf;
use std::process::ExitCode;

use morningstar_devtools::{DEFAULT_SERVER_PORT, NewApp, create_standalone_app};

const VERSION: &str = env!("CARGO_PKG_VERSION");

const USAGE: &str = "\
morningstar: create a Morningstar app, run it in a window, build it for Android.

Usage: morningstar <command> [arguments]

Commands:
  new <package> [options]   a new app in ./<package>: a project of its own on
                            this release of the framework (new --help)
  preview [options]         the app of this directory live in a window on the
                            Mac; reloads when its source changes (preview --help)
  build android [options]   the app of this directory as an APK; --install puts
                            it on the connected phone (build android --help)
  icon [options]            the app's launcher icon: --preview out/icon.png
  diff <design> <render>    how much a rendered screen differs from its design
  svg <file> <out.png>      an SVG file as an app would draw it
  version                   this release

Run them in the app's directory (or below it). cargo test and cargo run
--example screenshot need no command of their own.
";

const NEW_USAGE: &str = "\
Creates a new Morningstar app in ./<package>: a crate of its own that builds,
passes its own tests and can be installed on a phone as it is.

Usage: morningstar new <package> [options]

  <package>              the Cargo package and the app's directory: lower-case
                         letters, digits and hyphens, like my-app

Options:
  --name <text>          the name under the launcher icon
                         (default: the package in words, \"My App\")
  --id <id>              the application id, like com.example.myapp
                         (default: dev.morningstar.<package>; choose your own
                         before the app is given to anyone)
  --services <list>      opt-in platform services, separated by commas:
                         live (voice rooms) and its parts: live-video,
                         live-screen, live-background
                         (default: none; each adds to what the app carries)
  --server               also write a Node server skeleton in server/, and make
                         the app talk to it
  --port <number>        the port that server listens on (default: 8790)
  --dir <directory>      where the app's directory is made (default: here)
  -h, --help             print this help

The app depends on this release of the framework (morningstar-framework
=VERSION), so that its library and the Android host this command builds it
into are of one release. Nothing is overwritten.
";

fn main() -> ExitCode {
    let mut arguments = std::env::args().skip(1);
    let command = arguments.next();
    match command.as_deref() {
        None | Some("-h" | "--help" | "help") => {
            print!("{USAGE}");
            if command.is_none() {
                ExitCode::from(2)
            } else {
                ExitCode::SUCCESS
            }
        }
        Some("-V" | "--version" | "version") => {
            println!("morningstar {VERSION}");
            ExitCode::SUCCESS
        }
        Some("new") => new(arguments),
        Some("preview") => preview(arguments),
        Some("build") => match arguments.next().as_deref() {
            Some("android") => android::main(arguments),
            Some("-h" | "--help") => {
                print!("{}", android::USAGE);
                ExitCode::SUCCESS
            }
            Some("ios") => {
                eprintln!(
                    "morningstar: building for iOS is not part of this command yet; it needs \
                     the repository's tools/build-ios.sh and Xcode"
                );
                ExitCode::from(2)
            }
            other => {
                eprintln!(
                    "morningstar: build what? `morningstar build android`{}",
                    other.map_or(String::new(), |other| format!(" (not `{other}`)"))
                );
                ExitCode::from(2)
            }
        },
        Some("icon") => {
            let arguments: Vec<String> = arguments.collect();
            match app::with_package(arguments) {
                Ok(arguments) => morningstar_devtools::commands::icon::main(arguments.into_iter()),
                Err(error) => fail(&error),
            }
        }
        Some("diff") => morningstar_devtools::commands::diff::main(arguments),
        Some("svg") => morningstar_devtools::commands::svg::main(arguments),
        Some(other) => {
            eprintln!("morningstar: no command `{other}`\n");
            eprint!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn fail(error: &str) -> ExitCode {
    eprintln!("morningstar: {error}");
    ExitCode::from(2)
}

fn new(mut arguments: impl Iterator<Item = String>) -> ExitCode {
    let usage = NEW_USAGE.replace("VERSION", VERSION);
    let mut package = None;
    let mut name = None;
    let mut id = None;
    let mut services = None;
    let mut server = false;
    let mut port = None;
    let mut directory = PathBuf::from(".");
    while let Some(argument) = arguments.next() {
        let mut value = || {
            arguments
                .next()
                .ok_or_else(|| format!("{argument} needs a value"))
        };
        let parsed: Result<(), String> = match argument.as_str() {
            "-h" | "--help" => {
                print!("{usage}");
                return ExitCode::SUCCESS;
            }
            "--name" => value().map(|v| name = Some(v)),
            "--id" => value().map(|v| id = Some(v)),
            "--services" => value().map(|v| services = Some(v)),
            "--server" => {
                server = true;
                Ok(())
            }
            "--port" => value().map(|v| port = Some(v)),
            "--dir" => value().map(|v| directory = PathBuf::from(v)),
            option if option.starts_with('-') => Err(format!("unknown option {option}")),
            _ if package.is_none() => {
                package = Some(argument);
                Ok(())
            }
            _ => Err(format!("unexpected argument {argument}")),
        };
        if let Err(error) = parsed {
            eprintln!("morningstar new: {error}\n\n{usage}");
            return ExitCode::from(2);
        }
    }
    let Some(package) = package else {
        eprintln!(
            "morningstar new: what is the app called? Give its package name, like my-app\n\n{usage}"
        );
        return ExitCode::from(2);
    };
    if port.is_some() && !server {
        return fail("new: --port is the server's: give --server as well");
    }
    let build = || -> Result<NewApp, morningstar_devtools::ScaffoldError> {
        let mut app = NewApp::new(&package)?.with_server(server);
        if let Some(name) = &name {
            app = app.with_name(name)?;
        }
        if let Some(id) = &id {
            app = app.with_id(id)?;
        }
        if let Some(services) = &services {
            app = app.with_services(services)?;
        }
        if let Some(port) = &port {
            app = app.with_port(port)?;
        }
        Ok(app.standalone(true))
    };
    let app = match build() {
        Ok(app) => app,
        Err(error) => return fail(&format!("new: {error}")),
    };
    let created = match create_standalone_app(&directory, &app) {
        Ok(created) => created,
        Err(error) => return fail(&format!("new: {error}; nothing was written")),
    };
    println!(
        "Created {} (\"{}\", {}) in {}: {} files.\n",
        app.package(),
        app.name(),
        app.id(),
        created.directory.display(),
        created.files.len()
    );
    println!("Next:\n");
    println!("  cd {}", created.directory.display());
    println!("  cargo test                    # passes; writes tests/references/*.png");
    println!("  cargo run --example screenshot -- out/   # look at out/home.png");
    println!("  morningstar preview           # live in a window; reloads on save");
    println!("  morningstar build android --abis arm64-v8a --install   # onto a phone");
    if app.server() {
        let port = app.port();
        let default = if port == DEFAULT_SERVER_PORT {
            ""
        } else {
            " (the port you chose)"
        };
        println!("  cd server && npm test && npm start   # 127.0.0.1:{port}{default}");
    }
    println!("\nREADME.md in the app says the rest.");
    ExitCode::SUCCESS
}

fn preview(arguments: impl Iterator<Item = String>) -> ExitCode {
    use morningstar_preview::options::{Options, USAGE};
    let mut options = match Options::parse(arguments) {
        Ok(options) => options,
        Err(error) => return fail(&format!("preview: {error}")),
    };
    if options.help {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    if options.app.is_empty() {
        match app::current_package(options.manifest_path.as_deref()) {
            Ok(package) => options.app = package,
            Err(error) => return fail(&format!("preview: {error}")),
        }
    }
    morningstar_preview::supervisor::run(&options)
}
