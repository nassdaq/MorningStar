//! The `morningstar-icon` binary, run as the build scripts run it: in this
//! workspace, on its example apps.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use morningstar_devtools::Image;

fn run(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_morningstar-icon"))
        // The workspace root, as the scripts do.
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .args(arguments)
        .output()
        .expect("the binary runs")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

/// A directory for generated files, removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "morningstar-icon-cli-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn prints_what_the_build_scripts_read_one_value_per_line() {
    // hello-gpu keeps the application id it had before ids were declared.
    let output = run(&[
        "hello-gpu",
        "--print",
        "id",
        "--print",
        "name",
        "--print",
        "lib",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "dev.morningstar.hello\nMorningstar\nhello_gpu\n"
    );

    // An app that declares only a name gets the default id; iOS cannot
    // have the underscore.
    let output = run(&[
        "services-check",
        "--print",
        "name",
        "--print",
        "id",
        "--print",
        "bundle-id",
        "--print",
        "icon",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "Services check\ndev.morningstar.services_check\ndev.morningstar.services-check\nbuilt-in\n"
    );
}

#[test]
fn writes_android_resources_once() {
    let scratch = Scratch::new("android");
    let directory = scratch.0.to_str().unwrap();
    let output = run(&["showcase", "--android", directory, "--print", "name"]);
    assert!(output.status.success(), "{}", stderr(&output));
    // Standard output is only what was asked for; the report is on stderr.
    assert_eq!(stdout(&output), "Showcase\n");
    assert!(
        stderr(&output).contains("18 written, 0 unchanged"),
        "{}",
        stderr(&output)
    );
    let icon = Image::load(scratch.0.join("mipmap-xxxhdpi/ic_launcher.png")).unwrap();
    assert_eq!((icon.width(), icon.height()), (192, 192));
    assert!(
        scratch
            .0
            .join("mipmap-anydpi-v26/ic_launcher.xml")
            .is_file()
    );
    assert!(
        scratch
            .0
            .join("values/ic_launcher_background.xml")
            .is_file()
    );

    let again = run(&["showcase", "--android", directory]);
    assert!(again.status.success(), "{}", stderr(&again));
    assert!(
        stderr(&again).contains("0 written, 18 unchanged"),
        "{}",
        stderr(&again)
    );
}

#[test]
fn the_examples_with_icons_of_their_own_declare_pictures_that_pass() {
    for (package, name) in [("callready", "CallReady"), ("fundidesk", "FundiDesk")] {
        let scratch = Scratch::new(package);
        let preview = scratch.0.join("preview.png");
        let output = run(&[
            package,
            "--print",
            "name",
            "--print",
            "icon",
            "--preview",
            preview.to_str().unwrap(),
        ]);
        assert!(output.status.success(), "{package}: {}", stderr(&output));
        let printed = stdout(&output);
        let mut lines = printed.lines();
        assert_eq!(lines.next(), Some(name));
        assert!(
            lines.next().unwrap().ends_with("assets/icon.png"),
            "{printed}"
        );
        assert!(Image::load(&preview).is_ok());
    }
}

#[test]
fn problems_are_reported_and_exit_with_2() {
    for (arguments, expected) in [
        (
            &["no-such-app", "--print", "name"][..],
            "no package called `no-such-app`",
        ),
        (&["hello-gpu", "--print", "colour"][..], "--print colour"),
        (&["hello-gpu", "--android"][..], "--android needs a value"),
        (&["--print", "name"][..], "which package"),
        (&["hello-gpu", "--frobnicate"][..], "unknown option"),
    ] {
        let output = run(arguments);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        assert!(
            stderr(&output).contains(expected),
            "{arguments:?}: {}",
            stderr(&output)
        );
        assert_eq!(stdout(&output), "", "{arguments:?}");
    }
    let help = run(&["--help"]);
    assert!(help.status.success());
    assert!(stdout(&help).contains("[package.metadata.morningstar]"));
}
