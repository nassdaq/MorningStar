//! Visual regression testing: compare a rendered screen with a PNG stored
//! in the repository.
//!
//! ```no_run
//! use morningstar_devtools::{Tolerance, assert_matches_reference};
//! # let png: Vec<u8> = Vec::new();
//! // let png = morningstar_devtools::screenshot(&mut app, size, elapsed)?;
//! assert_matches_reference(&png, "tests/references/home.png", Tolerance::DEFAULT);
//! ```
//!
//! A relative path is resolved against the current directory, which under
//! `cargo test` is the root of the package being tested.
//!
//! To create the reference, or to accept a deliberate visual change, run the
//! tests once with [`UPDATE_ENV`] set and review the changed PNGs like any
//! other diff:
//!
//! ```sh
//! MORNINGSTAR_UPDATE_REFERENCES=1 cargo test -p my-app
//! ```
//!
//! When a comparison fails, two files are written next to the reference:
//! `home.actual.png` (what was rendered) and `home.diff.png` (reference,
//! actual and heat map side by side). They are removed again by the next
//! passing run; add `*.actual.png` and `*.diff.png` to `.gitignore`.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::diff::{CompareOptions, Report, Tolerance, compare};
use crate::image::{Image, ImageError, write_file};

/// Set this environment variable (to anything but `0` or the empty string)
/// to make [`assert_matches_reference`] write references instead of
/// checking them.
pub const UPDATE_ENV: &str = "MORNINGSTAR_UPDATE_REFERENCES";

/// What a successful [`check_reference`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceOutcome {
    /// The image matches the stored reference.
    Matched,
    /// The reference was written, because [`UPDATE_ENV`] is set.
    Updated,
}

/// Why an image does not match its reference.
#[derive(Debug)]
pub enum ReferenceError {
    /// There is no reference file yet. The actual image was saved.
    Missing { reference: PathBuf, actual: PathBuf },
    /// The image and the reference have different sizes. Regression
    /// references are never resampled. The actual image was saved.
    SizeMismatch {
        reference: PathBuf,
        actual: PathBuf,
        reference_size: (u32, u32),
        actual_size: (u32, u32),
    },
    /// The images differ by more than the tolerance. The actual image and a
    /// side-by-side diff were saved.
    Different {
        reference: PathBuf,
        actual: PathBuf,
        diff: PathBuf,
        report: Box<Report>,
    },
    /// The image under test, the reference, or an output file could not be
    /// read or written.
    Image(ImageError),
}

impl fmt::Display for ReferenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing { reference, actual } => write!(
                f,
                "no reference image at {}\n  actual image: {}\n  \
                 look at it, then create the reference by running the test with {UPDATE_ENV}=1",
                reference.display(),
                actual.display()
            ),
            Self::SizeMismatch {
                reference,
                actual,
                reference_size,
                actual_size,
            } => write!(
                f,
                "image is {}x{} but the reference {} is {}x{}\n  actual image: {}\n  \
                 if the new size is intended, rerun with {UPDATE_ENV}=1",
                actual_size.0,
                actual_size.1,
                reference.display(),
                reference_size.0,
                reference_size.1,
                actual.display()
            ),
            Self::Different {
                reference,
                actual,
                diff,
                report,
            } => write!(
                f,
                "image does not match the reference {}\n  actual image: {}\n  \
                 reference | actual | diff: {}\n  \
                 if the change is intended, rerun with {UPDATE_ENV}=1\n{report}",
                reference.display(),
                actual.display(),
                diff.display()
            ),
            Self::Image(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ReferenceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Image(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ImageError> for ReferenceError {
    fn from(error: ImageError) -> Self {
        Self::Image(error)
    }
}

/// Asserts that `png` (typically from [`screenshot`](crate::screenshot))
/// matches the PNG stored at `reference`, within `tolerance`.
///
/// See the [module docs](self) for creating and updating references and for
/// the files written on failure.
///
/// # Panics
/// Panics, with the comparison report and the paths of the files to look
/// at, if the images differ, have different sizes, or the reference is
/// missing or unreadable.
#[track_caller]
pub fn assert_matches_reference(png: &[u8], reference: impl AsRef<Path>, tolerance: Tolerance) {
    if let Err(error) = check_reference(png, reference, tolerance) {
        panic!("{error}");
    }
}

/// [`assert_matches_reference`] without the panic.
pub fn check_reference(
    png: &[u8],
    reference: impl AsRef<Path>,
    tolerance: Tolerance,
) -> Result<ReferenceOutcome, ReferenceError> {
    check(png, reference.as_ref(), tolerance, update_requested())
}

fn update_requested() -> bool {
    std::env::var_os(UPDATE_ENV).is_some_and(|value| !value.is_empty() && value != "0")
}

/// `name.png` -> `name.<kind>.png`, in the same directory.
fn sibling(reference: &Path, kind: &str) -> PathBuf {
    let stem = reference
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    reference.with_file_name(format!("{stem}.{kind}.png"))
}

fn check(
    png: &[u8],
    reference: &Path,
    tolerance: Tolerance,
    update: bool,
) -> Result<ReferenceOutcome, ReferenceError> {
    // Decode first: a broken image must never become a reference.
    let actual = Image::from_png(png)?;
    let actual_path = sibling(reference, "actual");
    let diff_path = sibling(reference, "diff");
    let remove_leftovers = || {
        let _ = std::fs::remove_file(&actual_path);
        let _ = std::fs::remove_file(&diff_path);
    };

    if update {
        write_file(reference, png)?;
        remove_leftovers();
        return Ok(ReferenceOutcome::Updated);
    }
    if !reference.exists() {
        write_file(&actual_path, png)?;
        return Err(ReferenceError::Missing {
            reference: reference.to_owned(),
            actual: actual_path,
        });
    }

    let expected = Image::load(reference)?;
    let reference_size = (expected.width(), expected.height());
    let actual_size = (actual.width(), actual.height());
    if reference_size != actual_size {
        write_file(&actual_path, png)?;
        let _ = std::fs::remove_file(&diff_path);
        return Err(ReferenceError::SizeMismatch {
            reference: reference.to_owned(),
            actual: actual_path,
            reference_size,
            actual_size,
        });
    }

    let options = CompareOptions {
        tolerance,
        ..CompareOptions::default()
    };
    let comparison = compare(&expected, &actual, &options);
    if comparison.report.passed {
        remove_leftovers();
        return Ok(ReferenceOutcome::Matched);
    }
    write_file(&actual_path, png)?;
    comparison.side_by_side().save(&diff_path)?;
    Err(ReferenceError::Different {
        reference: reference.to_owned(),
        actual: actual_path,
        diff: diff_path,
        report: Box::new(comparison.report),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::Rect;

    /// A fresh directory under the system temporary directory, removed on
    /// drop.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "morningstar-devtools-{name}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn screen(card: [u8; 4]) -> Vec<u8> {
        let mut image = Image::filled(48, 64, [255, 255, 255, 255]);
        image.fill_rect(Rect::new(8, 8, 32, 16), card);
        image.to_png()
    }

    const BLUE: [u8; 4] = [37, 99, 235, 255];
    const RED: [u8; 4] = [220, 38, 38, 255];

    #[test]
    fn missing_reference_fails_and_saves_the_actual_image() {
        let scratch = Scratch::new("missing");
        let reference = scratch.0.join("nested/home.png");
        let error = check(&screen(BLUE), &reference, Tolerance::EXACT, false).unwrap_err();
        let ReferenceError::Missing { actual, .. } = &error else {
            panic!("{error}");
        };
        assert_eq!(actual, &scratch.0.join("nested/home.actual.png"));
        assert_eq!(std::fs::read(actual).unwrap(), screen(BLUE));
        assert!(!reference.exists(), "a reference is never created silently");
        assert!(error.to_string().contains(UPDATE_ENV));
    }

    #[test]
    fn update_writes_the_reference_which_then_matches() {
        let scratch = Scratch::new("update");
        let reference = scratch.0.join("refs/home.png");
        assert_eq!(
            check(&screen(BLUE), &reference, Tolerance::EXACT, true).unwrap(),
            ReferenceOutcome::Updated
        );
        assert_eq!(std::fs::read(&reference).unwrap(), screen(BLUE));
        assert_eq!(
            check(&screen(BLUE), &reference, Tolerance::EXACT, false).unwrap(),
            ReferenceOutcome::Matched
        );
        assert!(!sibling(&reference, "actual").exists());
        assert!(!sibling(&reference, "diff").exists());
    }

    #[test]
    fn a_difference_fails_writes_actual_and_diff_and_a_pass_cleans_up() {
        let scratch = Scratch::new("different");
        let reference = scratch.0.join("home.png");
        check(&screen(BLUE), &reference, Tolerance::EXACT, true).unwrap();

        let error = check(&screen(RED), &reference, Tolerance::DEFAULT, false).unwrap_err();
        let ReferenceError::Different {
            actual,
            diff,
            report,
            ..
        } = &error
        else {
            panic!("{error}");
        };
        assert_eq!(report.differing_pixels, 32 * 16);
        assert_eq!(report.regions[0].rect, Rect::new(8, 8, 32, 16));
        assert_eq!(std::fs::read(actual).unwrap(), screen(RED));
        let picture = Image::load(diff).unwrap();
        assert_eq!((picture.width(), picture.height()), (48 * 3 + 4, 64));
        let message = error.to_string();
        assert!(message.contains("home.actual.png"));
        assert!(message.contains("home.diff.png"));
        assert!(message.contains("region 1: x=8 y=8 w=32 h=16"));
        // The reference itself is untouched.
        assert_eq!(std::fs::read(&reference).unwrap(), screen(BLUE));

        // Once the screen is right again, the leftovers go away.
        check(&screen(BLUE), &reference, Tolerance::DEFAULT, false).unwrap();
        assert!(!actual.exists());
        assert!(!diff.exists());

        // Updating accepts the new look.
        check(&screen(RED), &reference, Tolerance::DEFAULT, true).unwrap();
        check(&screen(RED), &reference, Tolerance::EXACT, false).unwrap();
    }

    #[test]
    fn tolerance_is_respected() {
        let scratch = Scratch::new("tolerance");
        let reference = scratch.0.join("home.png");
        check(&screen(BLUE), &reference, Tolerance::EXACT, true).unwrap();
        let nearly = screen([38, 100, 236, 255]);
        assert!(check(&nearly, &reference, Tolerance::EXACT, false).is_err());
        assert!(check(&nearly, &reference, Tolerance::DEFAULT, false).is_ok());
    }

    #[test]
    fn a_different_size_is_a_failure_not_a_resample() {
        let scratch = Scratch::new("size");
        let reference = scratch.0.join("home.png");
        check(&screen(BLUE), &reference, Tolerance::EXACT, true).unwrap();
        let double = Image::filled(96, 128, BLUE).to_png();
        let error = check(&double, &reference, Tolerance::DEFAULT, false).unwrap_err();
        assert!(matches!(
            error,
            ReferenceError::SizeMismatch {
                reference_size: (48, 64),
                actual_size: (96, 128),
                ..
            }
        ));
        assert!(sibling(&reference, "actual").exists());
    }

    #[test]
    fn broken_images_are_errors_and_never_become_references() {
        let scratch = Scratch::new("broken");
        let reference = scratch.0.join("home.png");
        let error = check(b"not a png", &reference, Tolerance::EXACT, true).unwrap_err();
        assert!(matches!(
            error,
            ReferenceError::Image(ImageError::Decode(_))
        ));
        assert!(!reference.exists());

        std::fs::write(&reference, b"garbage").unwrap();
        let error = check(&screen(BLUE), &reference, Tolerance::EXACT, false).unwrap_err();
        assert!(matches!(
            error,
            ReferenceError::Image(ImageError::Decode(_))
        ));
    }

    #[test]
    #[should_panic(expected = "no reference image at")]
    fn the_assertion_panics_with_the_explanation() {
        let scratch = Scratch::new("assert");
        // Only meaningful when the suite is not being run in update mode.
        if update_requested() {
            panic!("no reference image at (skipped: {UPDATE_ENV} is set)");
        }
        assert_matches_reference(&screen(BLUE), scratch.0.join("home.png"), Tolerance::EXACT);
    }

    #[test]
    fn sibling_names() {
        assert_eq!(
            sibling(Path::new("tests/refs/home.png"), "actual"),
            Path::new("tests/refs/home.actual.png")
        );
        assert_eq!(
            sibling(Path::new("home"), "diff"),
            Path::new("home.diff.png")
        );
    }
}
