//! The supervisor: builds the app's runner, starts it, and replaces it
//! when the sources change.
//!
//! Reloading is "build again, then start again", not swapping code inside
//! a running process. The app is restarted from its `main`, so nothing of
//! the old build survives in memory: no stale layout cache, no value of a
//! type whose fields have changed. What does survive is what would on a
//! phone: the values the app stored (ADR 0019), which the host hands back
//! at setup. The window comes back where it was, without taking the
//! keyboard focus from the editor. While a build runs, and when it fails,
//! the old window stays up and says so in its title; the compiler's
//! errors are in the terminal, where they can be clicked.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use morningstar_devtools::{Identity, cargo_metadata};

use crate::host::RESTART_EXIT_CODE;
use crate::options::Options;
use crate::runner::{Runner, Workspace, sync_lock};
use crate::store::Store;
use crate::watch::{Watcher, parse_dep_info};

/// How often the supervisor looks at the sources, the build and the app.
const TICK: Duration = Duration::from_millis(250);
/// How long the sources must be quiet before a build starts.
const QUIET: Duration = Duration::from_millis(150);
/// How long a replaced app gets to close by itself.
const GRACE: Duration = Duration::from_secs(2);

/// Everything about one preview that does not change while it runs.
struct Plan {
    name: String,
    manifest: PathBuf,
    target_dir: PathBuf,
    binary: PathBuf,
    state_dir: PathBuf,
    /// For the first start: with `--position`, if one was given.
    first_args: Vec<String>,
    /// For every start after it: the window goes where it last was, which
    /// is where the user left it.
    again_args: Vec<String>,
}

/// Runs the preview of `options.app` until its window is closed.
pub fn run(options: &Options) -> ExitCode {
    match prepare(options) {
        Ok((plan, roots)) => supervise(options, &plan, roots),
        Err(error) => {
            eprintln!("preview: {error}");
            ExitCode::FAILURE
        }
    }
}

fn prepare(options: &Options) -> Result<(Plan, Vec<PathBuf>), String> {
    let metadata =
        cargo_metadata(options.manifest_path.as_deref()).map_err(|error| error.to_string())?;
    let identity = Identity::from_cargo_metadata(&metadata, &options.app)
        .map_err(|error| format!("{} cannot be previewed: {error}", options.app))?;
    let workspace = Workspace::from_cargo_metadata(&metadata)
        .ok_or("cargo metadata did not say where the workspace is")?;

    let base = workspace.target_dir.join("preview").join(&identity.package);
    let runner_dir = base.join("runner");
    let state_dir = base.join("state");
    let runner = Runner::new(&identity.package, &identity.manifest_dir);
    let manifest = runner
        .write(&runner_dir)
        .map_err(|error| format!("{}: {error}", runner_dir.display()))?;
    sync_lock(&workspace.root, &runner_dir)
        .map_err(|error| format!("the workspace's Cargo.lock could not be copied: {error}"))?;

    if options.fresh {
        Store::open(&state_dir)
            .and_then(|mut store| store.clear())
            .map_err(|error| format!("{}: {error}", state_dir.display()))?;
    }

    let host_args = |options: &Options| {
        let mut args = options.host_args();
        args.extend([
            "--state-dir".to_owned(),
            state_dir.display().to_string(),
            "--title".to_owned(),
            identity.name.clone(),
            "--supervised".to_owned(),
        ]);
        args
    };
    let first_args = host_args(options);
    let again_args = host_args(&Options {
        position: None,
        ..options.clone()
    });
    let plan = Plan {
        name: identity.name,
        manifest,
        binary: workspace
            .target_dir
            .join("debug")
            .join(runner.binary_name()),
        target_dir: workspace.target_dir,
        state_dir,
        first_args,
        again_args,
    };
    Ok((plan, workspace.members))
}

fn supervise(options: &Options, plan: &Plan, roots: Vec<PathBuf>) -> ExitCode {
    let mut watcher = (!options.no_watch).then(|| Watcher::new(roots, QUIET));
    match &watcher {
        Some(watcher) => eprintln!(
            "preview: building {}; watching {} source files. Stored values are in {}.",
            plan.name,
            watcher.files(),
            plan.state_dir.display()
        ),
        None => eprintln!("preview: building {}.", plan.name),
    }

    let mut app: Option<Child> = None;
    let mut build: Option<Child> = start_build(plan);
    let mut running_stamp: Option<SystemTime> = None;
    let mut dirty = false;
    let mut started = false;

    loop {
        // The app: closed by the user, or asking to be started again.
        if let Some(child) = app.as_mut()
            && let Ok(Some(status)) = child.try_wait()
        {
            if status.code() != Some(i32::from(RESTART_EXIT_CODE)) {
                stop(build.take());
                return ExitCode::from(status.code().unwrap_or(1).clamp(0, 255) as u8);
            }
            app = start_app(plan, Start::Again);
            running_stamp = stamp(&plan.binary);
        }

        if let Some(watcher) = watcher.as_mut()
            && watcher.poll(Instant::now())
        {
            dirty = true;
        }
        if dirty && build.is_none() {
            dirty = false;
            tell(app.as_mut(), "status rebuilding\u{2026}");
            build = start_build(plan);
        }

        if let Some(child) = build.as_mut()
            && let Ok(Some(status)) = child.try_wait()
        {
            build = None;
            if status.success() {
                // From now on, watch exactly what this build read.
                if let Some(watcher) = watcher.as_mut()
                    && let Ok(read) = std::fs::read_to_string(plan.binary.with_extension("d"))
                {
                    watcher.set_files(parse_dep_info(&read), Instant::now());
                }
                let built = stamp(&plan.binary);
                if app.is_none() {
                    app = start_app(plan, if started { Start::Again } else { Start::First });
                    started = true;
                    running_stamp = built;
                } else if built != running_stamp {
                    eprintln!("preview: reloading {}.", plan.name);
                    replace(app.take());
                    app = start_app(plan, Start::Reload);
                    running_stamp = built;
                } else {
                    // The change did not reach the app (a comment in
                    // another crate, a server's file): nothing to reload.
                    tell(app.as_mut(), "status");
                }
                if app.is_none() {
                    return ExitCode::FAILURE;
                }
            } else if watcher.is_some() {
                tell(
                    app.as_mut(),
                    "status build failed \u{2014} see the terminal",
                );
                eprintln!(
                    "preview: the build failed{}. Waiting for a change to try again.",
                    if app.is_some() {
                        "; the window shows the last build that passed"
                    } else {
                        ""
                    }
                );
            } else {
                return ExitCode::FAILURE;
            }
        }

        // Nothing running, nothing building and nobody watching: there is
        // nothing left that could happen.
        if app.is_none() && build.is_none() && watcher.is_none() {
            return ExitCode::FAILURE;
        }
        thread::sleep(TICK);
    }
}

/// Starts `cargo build` for the runner. Its output is the terminal's:
/// progress and errors appear as from any build.
fn start_build(plan: &Plan) -> Option<Child> {
    // Under `cargo run`, CARGO is the Cargo that started this.
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let spawned = Command::new(cargo)
        .arg("build")
        .arg("--manifest-path")
        .arg(&plan.manifest)
        .arg("--target-dir")
        .arg(&plan.target_dir)
        .stdin(Stdio::null())
        .spawn();
    match spawned {
        Ok(child) => Some(child),
        Err(error) => {
            eprintln!("preview: cargo could not be run: {error}");
            None
        }
    }
}

/// Which start of the app this is.
#[derive(Clone, Copy, PartialEq)]
enum Start {
    /// The first: where `--position` says, and in front.
    First,
    /// After the app asked to be started again: where it was, in front.
    Again,
    /// After a build: where it was, and behind the editor.
    Reload,
}

fn start_app(plan: &Plan, start: Start) -> Option<Child> {
    let mut command = Command::new(&plan.binary);
    command
        .args(if start == Start::First {
            &plan.first_args
        } else {
            &plan.again_args
        })
        .stdin(Stdio::piped());
    if start == Start::Reload {
        command.arg("--background");
    }
    match command.spawn() {
        Ok(child) => Some(child),
        Err(error) => {
            eprintln!(
                "preview: {} could not be started: {error}",
                plan.binary.display()
            );
            None
        }
    }
}

/// Says one line to the app on its standard input.
fn tell(app: Option<&mut Child>, line: &str) {
    if let Some(stdin) = app.and_then(|app| app.stdin.as_mut()) {
        let _ = writeln!(stdin, "{line}");
        let _ = stdin.flush();
    }
}

/// Closes the running app to make room for the new build: asked first, so
/// that it remembers where its window is, then stopped.
fn replace(app: Option<Child>) {
    let Some(mut app) = app else { return };
    tell(Some(&mut app), "quit");
    let deadline = Instant::now() + GRACE;
    while Instant::now() < deadline {
        if matches!(app.try_wait(), Ok(Some(_))) {
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
    stop(Some(app));
}

fn stop(child: Option<Child>) {
    if let Some(mut child) = child {
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// When the runner's binary was last written: how a build that changed
/// the app is told from one that found nothing to do.
fn stamp(binary: &Path) -> Option<SystemTime> {
    std::fs::metadata(binary).ok()?.modified().ok()
}
