//! The `morningstar-diff` binary, run as a user or a script would run it.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use morningstar_devtools::{Image, Rect};

const WHITE: [u8; 4] = [255, 255, 255, 255];
const BLUE: [u8; 4] = [37, 99, 235, 255];
const RED: [u8; 4] = [220, 38, 38, 255];

/// A directory of test images, removed on drop.
struct Fixture(PathBuf);

impl Fixture {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "morningstar-diff-cli-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        let fixture = Self(path);

        // The design: a blue card on white, with a "status bar" row on top.
        let mut design = Image::filled(120, 200, WHITE);
        design.fill_rect(Rect::new(0, 0, 120, 10), [0, 0, 0, 255]);
        design.fill_rect(Rect::new(20, 40, 80, 50), BLUE);
        design.save(fixture.path("design.png")).unwrap();

        // A correct rendering, which has no status bar.
        let mut good = Image::filled(120, 200, WHITE);
        good.fill_rect(Rect::new(20, 40, 80, 50), BLUE);
        good.save(fixture.path("good.png")).unwrap();

        // A wrong one: the card is red.
        let mut bad = good.clone();
        bad.fill_rect(Rect::new(20, 40, 80, 50), RED);
        bad.save(fixture.path("bad.png")).unwrap();

        // A correct one on a taller screen.
        let mut tall = Image::filled(120, 260, WHITE);
        tall.fill_rect(Rect::new(20, 40, 80, 50), BLUE);
        tall.save(fixture.path("tall.png")).unwrap();

        std::fs::write(fixture.path("broken.png"), b"\x89PNG\r\n\x1a\nnope").unwrap();
        fixture
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn run(&self, arguments: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_morningstar-diff"))
            .current_dir(&self.0)
            .args(arguments)
            .output()
            .expect("the binary runs")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

const IGNORE_STATUS_BAR: [&str; 2] = ["--ignore", "0,0,120,10"];

#[test]
fn matching_images_exit_zero() {
    let fixture = Fixture::new("match");
    let output = fixture.run(&["good.png", "good.png"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(
        text.starts_with("morningstar-diff report v1\nresult: PASS\n"),
        "{text}"
    );
    assert!(text.contains("differing_pixels: 0\n"));
}

#[test]
fn differing_images_exit_one_and_say_where() {
    let fixture = Fixture::new("differ");
    let mut arguments = vec!["design.png", "bad.png"];
    arguments.extend(IGNORE_STATUS_BAR);
    let output = fixture.run(&arguments);
    assert_eq!(output.status.code(), Some(1));
    let text = stdout(&output);
    assert!(text.contains("result: FAIL\n"), "{text}");
    assert!(text.contains("ignored_pixels: 1200\n"), "{text}");
    assert!(
        text.contains("region 1: x=20 y=40 w=80 h=50 pixels=4000 "),
        "{text}"
    );
    assert!(
        text.contains("reference=#2563EB candidate=#DC2626"),
        "{text}"
    );
}

#[test]
fn ignore_regions_and_tolerances_change_the_verdict() {
    let fixture = Fixture::new("options");
    // The status bar alone makes the good rendering fail...
    let output = fixture.run(&["design.png", "good.png"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).contains("region 1: x=0 y=0 w=120 h=10 "));
    // ...ignoring it makes it pass...
    let mut arguments = vec!["design.png", "good.png"];
    arguments.extend(IGNORE_STATUS_BAR);
    assert_eq!(fixture.run(&arguments).status.code(), Some(0));
    // ...and so does allowing 5% of the pixels to differ (`=` form).
    let output = fixture.run(&["design.png", "good.png", "--max-diff=5"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).contains("diff_percent: 5.0000\n"));
    // A huge threshold accepts even the red card.
    let mut arguments = vec!["design.png", "bad.png", "--threshold", "150"];
    arguments.extend(IGNORE_STATUS_BAR);
    assert_eq!(fixture.run(&arguments).status.code(), Some(0));
}

#[test]
fn pictures_are_written_and_readable() {
    let fixture = Fixture::new("pictures");
    let output = fixture.run(&[
        "design.png",
        "bad.png",
        "--out",
        "out/heat.png",
        "--side-by-side",
        "out/sbs.png",
    ]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let heat = Image::load(fixture.path("out/heat.png")).unwrap();
    assert_eq!((heat.width(), heat.height()), (120, 200));
    assert_eq!(heat.pixel(60, 60)[0], 255, "the card is hot");
    assert!(heat.pixel(60, 60)[1] < 120);
    let side_by_side = Image::load(fixture.path("out/sbs.png")).unwrap();
    assert_eq!((side_by_side.width(), side_by_side.height()), (364, 200));
    assert_eq!(side_by_side.pixel(60, 60), BLUE);
    assert_eq!(side_by_side.pixel(122 + 60, 60), RED);
}

#[test]
fn aspect_mismatch_fails_unless_allowed() {
    let fixture = Fixture::new("aspect");
    let mut arguments = vec!["design.png", "tall.png"];
    arguments.extend(IGNORE_STATUS_BAR);
    let output = fixture.run(&arguments);
    assert_eq!(output.status.code(), Some(1));
    let text = stdout(&output);
    assert!(
        text.contains("aspect: MISMATCH reference=0.6000 candidate=0.4615"),
        "{text}"
    );
    assert!(text.contains("uncovered_rows=60"), "{text}");
    assert!(text.contains("differing_pixels: 0\n"), "{text}");

    arguments.push("--allow-aspect-mismatch");
    assert_eq!(fixture.run(&arguments).status.code(), Some(0));

    // Stretching instead distorts the card, which then differs.
    arguments.extend(["--fit", "stretch"]);
    let output = fixture.run(&arguments);
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).contains("handling=stretched"));
}

#[test]
fn json_output() {
    let fixture = Fixture::new("json");
    let output = fixture.run(&["design.png", "bad.png", "--json", "--max-regions", "1"]);
    assert_eq!(output.status.code(), Some(1));
    let json = stdout(&output);
    assert!(json.starts_with("{\n  \"format\": \"morningstar-diff report v1\",\n"));
    assert!(json.trim_end().ends_with('}'));
    assert!(json.contains("\"passed\": false"));
    assert!(json.contains("\"region_count\": 2"));
    assert_eq!(json.matches("\"rank\":").count(), 1);
}

#[test]
fn bad_input_exits_two_with_a_message() {
    let fixture = Fixture::new("errors");
    let cases: [(&[&str], &str); 9] = [
        (&[], "expected a reference and a candidate"),
        (&["design.png"], "expected a reference and a candidate"),
        (&["design.png", "good.png", "bad.png"], "got 3 path(s)"),
        (
            &["design.png", "nowhere.png"],
            "cannot read the candidate image",
        ),
        (&["broken.png", "good.png"], "not a readable PNG"),
        (&["design.png", "good.png", "--wat"], "unknown option --wat"),
        (
            &["design.png", "good.png", "--threshold", "lots"],
            "--threshold must be",
        ),
        (
            &["design.png", "good.png", "--ignore", "1,2,3"],
            "--ignore takes x,y,w,h",
        ),
        (
            &["design.png", "good.png", "--out"],
            "--out needs a file path",
        ),
    ];
    for (arguments, message) in cases {
        let output = fixture.run(arguments);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        assert!(
            stderr(&output).contains(message),
            "{arguments:?}: {}",
            stderr(&output)
        );
    }
    // A directory in place of an output file is a file error too.
    let output = fixture.run(&["good.png", "good.png", "--out", "."]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn help_exits_zero() {
    let fixture = Fixture::new("help");
    let output = fixture.run(&["--help"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).contains("Usage: morningstar-diff"));
    let _: &Path = &fixture.0;
}
