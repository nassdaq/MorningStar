//! A new Morningstar application, ready to test, look at and install
//! (ADR 0041).
//!
//!   cargo run -q -p morningstar-devtools --bin morningstar-new -- \
//!       my-app --name "My App" --id com.example.myapp
//!
//! `docs/getting-started.md` walks from here to the app on a phone.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use morningstar_devtools::{CreatedApp, DEFAULT_APP_DIR, DEFAULT_SERVER_PORT, NewApp, create_app};

const USAGE: &str = "\
Creates a new Morningstar application in this workspace: a crate that builds,
passes its own tests and can be installed on a phone as it is.

Usage: morningstar-new <package> [options]

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
  --dir <directory>      where the app's directory is made, relative to the
                         workspace root (default: apps)
  --workspace <dir>      the workspace root, if not this directory or one above
  -h, --help             print this help

What is written: Cargo.toml (name, id, icon and services, declared once),
src/lib.rs (two screens, navigation, a stored value), tests (journeys,
accessibility, pictures of the screens), examples/screenshot.rs, a launcher
icon, fonts, README.md, SCREENS.md and design/README.md; and the app is added
to the workspace's members. Nothing is overwritten.

Exit status: 0 created, 2 usage error or nothing written.
";

struct Arguments {
    app: NewApp,
    workspace: Option<PathBuf>,
}

fn parse(mut arguments: impl Iterator<Item = String>) -> Result<Option<Arguments>, String> {
    let mut package = None;
    let mut name = None;
    let mut id = None;
    let mut services = None;
    let mut server = false;
    let mut port = None;
    let mut directory = None;
    let mut workspace = None;
    while let Some(argument) = arguments.next() {
        let mut value = || {
            arguments
                .next()
                .ok_or_else(|| format!("{argument} needs a value"))
        };
        match argument.as_str() {
            "-h" | "--help" => return Ok(None),
            "--name" => name = Some(value()?),
            "--id" => id = Some(value()?),
            "--services" => services = Some(value()?),
            "--server" => server = true,
            "--port" => port = Some(value()?),
            "--dir" => directory = Some(value()?),
            "--workspace" => workspace = Some(PathBuf::from(value()?)),
            option if option.starts_with('-') => return Err(format!("unknown option {option}")),
            _ if package.is_none() => package = Some(argument),
            _ => return Err(format!("unexpected argument {argument}")),
        }
    }
    let Some(package) = package else {
        return Err("what is the app called? Give its package name first, like my-app".to_owned());
    };
    if port.is_some() && !server {
        return Err("--port is the server's: give --server as well".to_owned());
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
        if let Some(directory) = &directory {
            app = app.with_directory(directory)?;
        }
        Ok(app)
    };
    let app = build().map_err(|error| error.to_string())?;
    Ok(Some(Arguments { app, workspace }))
}

/// The nearest directory, from `start` upwards, whose `Cargo.toml` has a
/// `[workspace]` table.
fn workspace_root(start: &Path) -> Option<PathBuf> {
    start.ancestors().find_map(|directory| {
        let manifest = std::fs::read_to_string(directory.join("Cargo.toml")).ok()?;
        manifest
            .lines()
            .any(|line| line.trim() == "[workspace]")
            .then(|| directory.to_owned())
    })
}

fn next_steps(app: &NewApp, created: &CreatedApp) -> String {
    let package = app.package();
    let member = &created.member;
    let mut commands = vec![
        (
            format!("cargo test -p {package}"),
            "passes; writes tests/references/*.png",
        ),
        (
            format!("cargo run -p {package} --example screenshot -- out/"),
            "look at out/home.png",
        ),
        (
            format!("tools/preview.sh {package}"),
            "live in a window; reloads on save",
        ),
        (
            format!(
                "MORNINGSTAR_APP={package} MORNINGSTAR_ABIS=arm64-v8a tools/build-android.sh --install"
            ),
            "onto a phone",
        ),
    ];
    if app.server() {
        let port = app.port();
        commands.push((
            format!("(cd {member}/server && npm test && npm start)"),
            "its server, on 127.0.0.1",
        ));
        commands.push((
            format!("adb reverse tcp:{port} tcp:{port}"),
            "lets a phone on USB reach it",
        ));
    }
    let mut steps = format!(
        "Created {} ({}) in {member}: {} files, and added to the workspace.\n\n\
         Next, from the workspace root:\n",
        app.name(),
        app.id(),
        created.files.len(),
    );
    for (command, what) in commands {
        steps.push_str(&format!("  {command}\n      # {what}\n"));
    }
    steps.push_str(&format!(
        "\nRead {member}/README.md, then docs/getting-started.md.\n"
    ));
    steps
}

fn main() -> ExitCode {
    let arguments = match parse(std::env::args().skip(1)) {
        Ok(Some(arguments)) => arguments,
        Ok(None) => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("morningstar-new: {message}\n\nRun it with --help for the options.");
            return ExitCode::from(2);
        }
    };
    let workspace = match arguments.workspace.clone().or_else(|| {
        std::env::current_dir()
            .ok()
            .and_then(|here| workspace_root(&here))
    }) {
        Some(workspace) => workspace,
        None => {
            eprintln!(
                "morningstar-new: no workspace here: run this inside the Morningstar \
                 repository, or name its root with --workspace"
            );
            return ExitCode::from(2);
        }
    };
    match create_app(&workspace, &arguments.app) {
        Ok(created) => {
            print!("{}", next_steps(&arguments.app, &created));
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("morningstar-new: {error}");
            ExitCode::from(2)
        }
    }
}

// Named so that the defaults in the help text cannot drift from the code.
const _: () = {
    assert!(DEFAULT_SERVER_PORT == 8790);
    let apps = DEFAULT_APP_DIR.as_bytes();
    assert!(apps.len() == 4 && apps[0] == b'a' && apps[3] == b's');
};
