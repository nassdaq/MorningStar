//! `morningstar-new` from end to end (ADR 0041): the command makes an app
//! in a workspace, and that app builds, passes its own tests the first
//! time, declares what was asked for, and its server's tests pass.
//!
//! The workspace is a throwaway one under `target/`, made of the real
//! crates by path and the real `Cargo.lock`, so nothing is downloaded and
//! the repository's own member list is never touched. It shares the
//! repository's target directory, so what the app is built from is
//! already compiled. Removed afterwards, pass or fail.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use morningstar_devtools::{Identity, cargo_metadata};

const NEW: &str = env!("CARGO_BIN_EXE_morningstar-new");

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("tools/morningstar-devtools is two levels down")
        .to_owned()
}

/// A workspace with no members that can take a Morningstar app.
struct Workspace {
    root: PathBuf,
}

impl Workspace {
    fn new(name: &str) -> Self {
        let repository = repository();
        let root = repository
            .join("target")
            .join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("target/ can be written");
        let manifest = format!(
            "[workspace]\nresolver = \"3\"\nmembers = []\n\n\
             [workspace.package]\nversion = \"0.0.1\"\nedition = \"2024\"\n\
             rust-version = \"1.87\"\nlicense = \"MIT OR Apache-2.0\"\npublish = false\n\n\
             [workspace.dependencies]\n\
             morningstar = {{ path = {:?} }}\n\
             morningstar-devtools = {{ path = {:?} }}\n",
            repository.join("crates/morningstar"),
            repository.join("tools/morningstar-devtools"),
        );
        std::fs::write(root.join("Cargo.toml"), manifest).expect("the manifest");
        std::fs::copy(repository.join("Cargo.lock"), root.join("Cargo.lock")).expect("the lock");
        Self { root }
    }

    fn new_app(&self, arguments: &[&str]) -> Output {
        Command::new(NEW)
            .arg("--workspace")
            .arg(&self.root)
            .args(arguments)
            .output()
            .expect("morningstar-new runs")
    }

    fn cargo(&self, arguments: &[&str]) -> Output {
        Command::new(env!("CARGO"))
            .args(arguments)
            .arg("--offline")
            .current_dir(&self.root)
            .env("CARGO_TARGET_DIR", repository().join("target"))
            // A reference that exists is compared, not rewritten, whatever
            // the outer run was asked to do.
            .env_remove("MORNINGSTAR_UPDATE_REFERENCES")
            .output()
            .expect("cargo runs")
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn a_generated_app_builds_passes_its_tests_and_is_what_was_asked_for() {
    let workspace = Workspace::new("morningstar-new-test");
    let created = workspace.new_app(&[
        "corner-shop",
        "--name",
        "Corner \"Shop\"",
        "--id",
        "com.example.cornershop",
        "--services",
        "live",
        "--server",
        "--port",
        "9321",
    ]);
    assert!(created.status.success(), "{}", text(&created));
    let said = text(&created);
    assert!(
        said.contains("Created Corner \"Shop\" (com.example.cornershop) in apps/corner-shop"),
        "{said}"
    );
    assert!(said.contains("cargo test -p corner-shop"), "{said}");
    assert!(said.contains("adb reverse tcp:9321 tcp:9321"), "{said}");

    let app = workspace.root.join("apps/corner-shop");
    for file in [
        "Cargo.toml",
        "README.md",
        "SCREENS.md",
        "design/README.md",
        "src/lib.rs",
        "src/json.rs",
        "examples/screenshot.rs",
        "tests/e2e.rs",
        "tests/accessibility.rs",
        "tests/screens.rs",
        "tests/support/mod.rs",
        "assets/icon.png",
        "assets/fonts/NotoSans-Regular.ttf",
        "assets/fonts/OFL.txt",
        "server/package.json",
        "server/.env.example",
        "server/API.md",
        "server/src/app.js",
        "server/test/app.test.js",
    ] {
        assert!(app.join(file).is_file(), "{file} was not written");
    }
    assert!(
        !app.join("server/.env").exists(),
        "no settings file is made"
    );
    let manifest = std::fs::read_to_string(workspace.root.join("Cargo.toml")).unwrap();
    assert!(
        manifest.contains("members = [\n    \"apps/corner-shop\",\n]"),
        "{manifest}"
    );

    // What it declares is read back by what the build scripts use.
    let metadata = cargo_metadata(Some(&workspace.root.join("Cargo.toml"))).expect("metadata");
    let identity = Identity::from_cargo_metadata(&metadata, "corner-shop").expect("identity");
    assert_eq!(identity.name, "Corner \"Shop\"");
    assert_eq!(identity.id, "com.example.cornershop");
    assert_eq!(identity.lib, "corner_shop");
    assert_eq!(identity.services, ["live"]);
    assert_eq!(identity.icon, Some(app.join("assets/icon.png")));

    // A second time, nothing is written and the reason is said.
    let again = workspace.new_app(&["corner-shop"]);
    assert_eq!(again.status.code(), Some(2));
    assert!(text(&again).contains("already has a package called `corner-shop`"));
    let elsewhere = workspace.new_app(&["corner-shop", "--dir", "other"]);
    assert_eq!(elsewhere.status.code(), Some(2), "one package, one name");
    assert!(!workspace.root.join("other").exists());

    // It builds, and its tests pass the first time: journeys, the
    // accessibility check, and the pictures, which this run writes.
    let tested = workspace.cargo(&["test", "-p", "corner-shop"]);
    assert!(tested.status.success(), "{}", text(&tested));
    let report = text(&tested);
    let passed: u32 = report
        .lines()
        .filter(|line| line.starts_with("test result: ok."))
        .filter_map(|line| line.split_whitespace().nth(3)?.parse::<u32>().ok())
        .sum();
    assert_eq!(
        passed, 11,
        "3 for the JSON, 5 journeys, 2 accessibility, 1 pictures\n{report}"
    );
    for picture in ["home", "settings"] {
        let reference = app.join(format!("tests/references/{picture}.png"));
        let size = std::fs::metadata(&reference)
            .map(|file| file.len())
            .unwrap_or(0);
        assert!(
            size > 2_000,
            "{picture}.png is a picture of something: {size} bytes"
        );
    }
    // With the pictures there, they are what the screens are held to.
    let held = workspace.cargo(&["test", "-p", "corner-shop", "--test", "screens"]);
    assert!(held.status.success(), "{}", text(&held));

    // The server's own tests, where there is a Node to run them.
    match Command::new("node")
        .args(["--test"])
        .current_dir(app.join("server"))
        .output()
    {
        Ok(server) => assert!(server.status.success(), "{}", text(&server)),
        Err(_) => eprintln!("node is not installed: the server's tests were not run"),
    }
}

#[test]
fn what_cannot_be_made_is_refused_with_status_2_and_nothing_written() {
    let workspace = Workspace::new("morningstar-new-refusals");
    for (arguments, reason) in [
        (&[][..], "what is the app called?"),
        (&["My App"][..], "cannot be the app's package name"),
        (&["my-app", "--id", "myapp"][..], "like dev.example.app"),
        (&["my-app", "--services", "camera"][..], "is not a service"),
        (&["my-app", "--port", "9000"][..], "give --server as well"),
        (
            &["my-app", "--server", "--port", "80"][..],
            "from 1024 to 65535",
        ),
        (&["my-app", "--dir", "../out"][..], "inside the workspace"),
        (
            &["my-app", "--colour", "red"][..],
            "unknown option --colour",
        ),
        (&["my-app", "second"][..], "unexpected argument second"),
        (&["my-app", "--name"][..], "--name needs a value"),
    ] {
        let refused = workspace.new_app(arguments);
        assert_eq!(refused.status.code(), Some(2), "{arguments:?}");
        let said = text(&refused);
        assert!(said.contains(reason), "{arguments:?}: {said}");
    }
    assert!(!workspace.root.join("apps").exists());
    let manifest = std::fs::read_to_string(workspace.root.join("Cargo.toml")).unwrap();
    assert!(
        manifest.contains("members = []"),
        "the members are as they were"
    );

    // A directory in the way is not written into.
    std::fs::create_dir_all(workspace.root.join("apps/my-app")).unwrap();
    let refused = workspace.new_app(&["my-app"]);
    assert_eq!(refused.status.code(), Some(2));
    assert!(text(&refused).contains("already exists; nothing was written"));

    let help = workspace.new_app(&["--help"]);
    assert!(help.status.success());
    assert!(text(&help).contains("Usage: morningstar-new <package> [options]"));
}
