//! An app's identity: the name under its icon, its application id and
//! where its icon is, as declared in its `Cargo.toml` (ADR 0020).
//!
//! ```toml
//! [package.metadata.morningstar]
//! name = "CallReady"
//! id = "dev.morningstar.callready"
//! icon = "assets/icon.png"
//! icon_foreground = "assets/icon-foreground.png"
//! icon_background = "#0E0E0E"
//! ```
//!
//! Every key is optional, and so is the table. The manifest is read by
//! Cargo, not here: [`cargo_metadata`] runs `cargo metadata` and
//! [`Identity::from_cargo_metadata`] takes the few fields it needs from the
//! JSON. That way every spelling TOML allows (comments, either kind of
//! quote, an inline table, dotted keys) means what Cargo says it means, the
//! package is found wherever it lives in the workspace, and the library
//! name is the one Cargo will build.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::image::ImageError;
use crate::json::{self, Value};

/// The keys of `[package.metadata.morningstar]`.
const KEYS: [&str; 5] = ["name", "id", "icon", "icon_foreground", "icon_background"];

/// Where an icon is looked for when the manifest does not say.
const DEFAULT_ICON: &str = "assets/icon.png";

/// Why an app's identity or icon could not be produced. Every message
/// names the key or the file at fault.
#[derive(Debug)]
pub enum IconError {
    /// `cargo metadata` could not be run, or failed. The text is Cargo's.
    Cargo(String),
    /// Cargo's answer was not what `cargo metadata` is documented to print.
    Metadata(String),
    /// The workspace has no package of that name.
    NoPackage {
        package: String,
        available: Vec<String>,
    },
    /// `[package.metadata.morningstar]` is not a table.
    NotATable {
        package: String,
        found: &'static str,
    },
    /// A key this tool does not know: most likely a misspelling.
    UnknownKey { package: String, key: String },
    /// A key whose value is not a string.
    NotAString {
        package: String,
        key: String,
        found: &'static str,
    },
    /// A value that cannot be used. `reason` completes "…: it ".
    BadValue {
        key: &'static str,
        value: String,
        reason: &'static str,
    },
    /// A file named in the manifest does not exist.
    MissingFile { key: &'static str, path: PathBuf },
    /// An icon file could not be read, or is not a PNG.
    Image { path: PathBuf, source: ImageError },
    /// An icon that is not square.
    NotSquare {
        path: PathBuf,
        width: u32,
        height: u32,
    },
    /// An icon with too few pixels to be scaled to every size.
    TooSmall {
        path: PathBuf,
        width: u32,
        minimum: u32,
    },
    /// No background colour was given and the icon's corner cannot stand
    /// in for one.
    NoBackground { path: PathBuf },
    /// A generated file could not be written.
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for IconError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const TABLE: &str = "[package.metadata.morningstar]";
        match self {
            Self::Cargo(message) => write!(f, "cargo metadata failed: {message}"),
            Self::Metadata(message) => {
                write!(f, "unexpected output from cargo metadata: {message}")
            }
            Self::NoPackage { package, available } => write!(
                f,
                "no package called `{package}` in this workspace (there are: {})",
                available.join(", ")
            ),
            Self::NotATable { package, found } => write!(
                f,
                "{TABLE} of `{package}` must be a table of strings, not {found}"
            ),
            Self::UnknownKey { package, key } => write!(
                f,
                "unknown key `{key}` in {TABLE} of `{package}` (the keys are: {})",
                KEYS.join(", ")
            ),
            Self::NotAString {
                package,
                key,
                found,
            } => write!(
                f,
                "`{key}` in {TABLE} of `{package}` must be a string in quotes, not {found}"
            ),
            Self::BadValue { key, value, reason } => {
                write!(f, "`{key} = \"{value}\"` in {TABLE}: it {reason}")
            }
            Self::MissingFile { key, path } => write!(
                f,
                "`{key}` in {TABLE} names {}, which does not exist",
                path.display()
            ),
            Self::Image { path, source } => match source {
                // An I/O error already says which file.
                ImageError::Io { .. } => write!(f, "{source}"),
                _ => write!(f, "{}: {source}; an icon must be a PNG", path.display()),
            },
            Self::NotSquare {
                path,
                width,
                height,
            } => write!(
                f,
                "{} is {width}x{height}: an icon must be square",
                path.display()
            ),
            Self::TooSmall {
                path,
                width,
                minimum,
            } => write!(
                f,
                "{} is {width}x{width}: an icon must be at least {minimum}x{minimum} \
                 (1024x1024 is best)",
                path.display()
            ),
            Self::NoBackground { path } => write!(
                f,
                "the corner pixel of {} is transparent, so it cannot be the icon's \
                 background colour: set `icon_background = \"#RRGGBB\"` in {TABLE}",
                path.display()
            ),
            Self::Write { path, source } => write!(f, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for IconError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Image { source, .. } => Some(source),
            Self::Write { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// What an app is called and where its icon is, with every default
/// applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    /// The Cargo package.
    pub package: String,
    /// The directory of the package's `Cargo.toml`; icon paths are
    /// relative to it.
    pub manifest_dir: PathBuf,
    /// The name shown under the icon. Default: the package name.
    pub name: String,
    /// The Android application id. Default: `dev.morningstar.` and the
    /// library name.
    pub id: String,
    /// The Rust library the host loads: the package's `cdylib` target, with
    /// underscores (`libhello_gpu.so` is `hello_gpu`).
    pub lib: String,
    /// The full-bleed square icon. `None`: the built-in Morningstar icon.
    pub icon: Option<PathBuf>,
    /// The Android adaptive foreground, if the app draws its own.
    pub icon_foreground: Option<PathBuf>,
    /// The adaptive background colour as sRGB bytes, if given.
    pub icon_background: Option<[u8; 3]>,
}

impl Identity {
    /// Reads the identity of `package` from the output of
    /// `cargo metadata --format-version 1 --no-deps`.
    ///
    /// The only thing looked up outside `metadata` is whether the icon
    /// files exist.
    pub fn from_cargo_metadata(metadata: &str, package: &str) -> Result<Self, IconError> {
        let unexpected = |what: &str| IconError::Metadata(what.to_owned());
        let root = json::parse(metadata).map_err(|error| IconError::Metadata(error.to_string()))?;
        let packages = root
            .get("packages")
            .and_then(Value::as_array)
            .ok_or_else(|| unexpected("no `packages` array"))?;
        fn name_of(entry: &Value) -> Option<&str> {
            entry.get("name").and_then(Value::as_str)
        }
        let Some(entry) = packages
            .iter()
            .find(|entry| name_of(entry) == Some(package))
        else {
            return Err(IconError::NoPackage {
                package: package.to_owned(),
                available: packages
                    .iter()
                    .filter_map(name_of)
                    .map(str::to_owned)
                    .collect(),
            });
        };
        let manifest_dir = entry
            .get("manifest_path")
            .and_then(Value::as_str)
            .and_then(|path| Path::new(path).parent())
            .ok_or_else(|| unexpected("a package without a `manifest_path`"))?
            .to_owned();

        // The library Cargo builds for the host to load. Cargo has printed
        // the target's name with hyphens at some versions and underscores
        // at others; the file is always named with underscores.
        let lib = entry
            .get("targets")
            .and_then(Value::as_array)
            .unwrap_or_default()
            .iter()
            .find(|target| {
                target
                    .get("crate_types")
                    .and_then(Value::as_array)
                    .unwrap_or_default()
                    .iter()
                    .any(|kind| kind.as_str() == Some("cdylib"))
            })
            .and_then(|target| target.get("name"))
            .and_then(Value::as_str)
            .unwrap_or(package)
            .replace('-', "_");

        // `metadata` is null without a [package.metadata] table.
        let table = match entry.get("metadata").and_then(|all| all.get("morningstar")) {
            None | Some(Value::Null) => &[][..],
            Some(Value::Object(members)) => &members[..],
            Some(other) => {
                return Err(IconError::NotATable {
                    package: package.to_owned(),
                    found: other.kind(),
                });
            }
        };
        let mut values: [Option<&str>; KEYS.len()] = [None; KEYS.len()];
        for (key, value) in table {
            let Some(index) = KEYS.iter().position(|known| known == key) else {
                return Err(IconError::UnknownKey {
                    package: package.to_owned(),
                    key: key.clone(),
                });
            };
            let Some(text) = value.as_str() else {
                return Err(IconError::NotAString {
                    package: package.to_owned(),
                    key: key.clone(),
                    found: value.kind(),
                });
            };
            values[index] = Some(text);
        }
        let [name, id, icon, icon_foreground, icon_background] = values;

        let name = match name {
            Some(name) => checked_name(name)?,
            None => package.to_owned(),
        };
        let id = match id {
            Some(id) => checked_id(id)?,
            None => format!("dev.morningstar.{lib}"),
        };
        let file = |key: &'static str, value: &str| {
            let path = manifest_dir.join(value);
            if value.is_empty() || !path.is_file() {
                return Err(IconError::MissingFile { key, path });
            }
            Ok(path)
        };
        let icon = match icon {
            Some(icon) => Some(file("icon", icon)?),
            None => Some(manifest_dir.join(DEFAULT_ICON)).filter(|path| path.is_file()),
        };
        let icon_foreground = match icon_foreground {
            Some(_) if icon.is_none() => {
                return Err(IconError::BadValue {
                    key: "icon_foreground",
                    value: icon_foreground.unwrap_or_default().to_owned(),
                    reason: "needs `icon` as well: the foreground alone cannot make the \
                             square and iOS icons",
                });
            }
            Some(foreground) => Some(file("icon_foreground", foreground)?),
            None => None,
        };
        let icon_background = icon_background.map(parse_colour).transpose()?;

        Ok(Self {
            package: package.to_owned(),
            manifest_dir,
            name,
            id,
            lib,
            icon,
            icon_foreground,
            icon_background,
        })
    }

    /// The iOS bundle identifier: the application id, with the one
    /// character Android allows and iOS does not (`_`) written as `-`.
    pub fn bundle_id(&self) -> String {
        self.id.replace('_', "-")
    }
}

/// Runs `cargo metadata --format-version 1 --no-deps` and returns what it
/// printed. With no `manifest_path`, Cargo looks upwards from the current
/// directory, as it always does.
pub fn cargo_metadata(manifest_path: Option<&Path>) -> Result<String, IconError> {
    // Under `cargo run`, CARGO is the Cargo that started this.
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut command = Command::new(cargo);
    command.args(["metadata", "--format-version", "1", "--no-deps"]);
    if let Some(path) = manifest_path {
        command.arg("--manifest-path").arg(path);
    }
    let output = command
        .output()
        .map_err(|error| IconError::Cargo(format!("could not run cargo: {error}")))?;
    if !output.status.success() {
        return Err(IconError::Cargo(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|_| IconError::Metadata("the output is not UTF-8".to_owned()))
}

/// A display name both platforms take as it is.
fn checked_name(name: &str) -> Result<String, IconError> {
    let bad = |reason| {
        Err(IconError::BadValue {
            key: "name",
            value: name.escape_debug().to_string(),
            reason,
        })
    };
    if name.trim().is_empty() {
        return bad("is empty");
    }
    if name != name.trim() {
        return bad("begins or ends with a space");
    }
    if name.chars().any(char::is_control) {
        return bad("contains a control character");
    }
    if name.chars().count() > 60 {
        return bad("is longer than 60 characters");
    }
    // Android reads a label that starts like this as a resource reference.
    if name.starts_with(['@', '?']) {
        return bad("begins with @ or ?, which Android reads as a reference to a resource");
    }
    // An Xcode configuration file has no way to write these in a value.
    if name.contains("//") || name.contains('$') {
        return bad("contains // or $, which the iOS build settings cannot carry");
    }
    Ok(name.to_owned())
}

/// An Android application id: two or more segments separated by dots, each
/// a letter followed by letters, digits and underscores.
fn checked_id(id: &str) -> Result<String, IconError> {
    let segment = |segment: &str| {
        segment.starts_with(|first: char| first.is_ascii_alphabetic())
            && segment
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
    };
    if id.split('.').count() < 2 || !id.split('.').all(segment) {
        return Err(IconError::BadValue {
            key: "id",
            value: id.escape_debug().to_string(),
            reason: "must be two or more names joined by dots, each a letter followed by \
                     letters, digits or underscores, like dev.example.app",
        });
    }
    Ok(id.to_owned())
}

/// `#RRGGBB` or `#RGB`, in either case.
pub(crate) fn parse_colour(text: &str) -> Result<[u8; 3], IconError> {
    let bad = || IconError::BadValue {
        key: "icon_background",
        value: text.escape_debug().to_string(),
        reason: "must be a colour written #RRGGBB or #RGB",
    };
    let digits = text.strip_prefix('#').ok_or_else(bad)?;
    if !digits.bytes().all(|digit| digit.is_ascii_hexdigit()) {
        return Err(bad());
    }
    let nibble = |at: usize| u8::from_str_radix(&digits[at..=at], 16).expect("a hex digit");
    match digits.len() {
        3 => Ok([nibble(0) * 17, nibble(1) * 17, nibble(2) * 17]),
        6 => Ok([
            nibble(0) * 16 + nibble(1),
            nibble(2) * 16 + nibble(3),
            nibble(4) * 16 + nibble(5),
        ]),
        _ => Err(bad()),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// What `cargo metadata` prints for one package, with `metadata` as
    /// its `metadata` member.
    pub(crate) fn metadata_json(package: &str, manifest_dir: &Path, metadata: &str) -> String {
        let dir = manifest_dir.to_str().unwrap().replace('\\', "\\\\");
        format!(
            r#"{{"packages":[
                {{"name":"other","manifest_path":"/nowhere/other/Cargo.toml","targets":[],"metadata":null}},
                {{"name":"{package}","version":"0.1.0","manifest_path":"{dir}/Cargo.toml",
                  "targets":[{{"kind":["example"],"crate_types":["bin"],"name":"screenshot"}},
                             {{"kind":["cdylib","lib"],"crate_types":["cdylib","lib"],"name":"{package}"}}],
                  "metadata":{metadata}}}],
               "workspace_members":[],"version":1}}"#
        )
    }

    fn identity(metadata: &str) -> Result<Identity, IconError> {
        Identity::from_cargo_metadata(
            &metadata_json("my-app", Path::new("/nowhere/my-app"), metadata),
            "my-app",
        )
    }

    #[test]
    fn an_app_that_declares_nothing_gets_the_defaults() {
        for metadata in [
            "null",
            "{}",
            r#"{"docs":{"rs":true}}"#,
            r#"{"morningstar":{}}"#,
        ] {
            let identity = identity(metadata).unwrap();
            assert_eq!(identity.package, "my-app");
            assert_eq!(identity.name, "my-app");
            assert_eq!(identity.lib, "my_app");
            assert_eq!(identity.id, "dev.morningstar.my_app");
            assert_eq!(identity.bundle_id(), "dev.morningstar.my-app");
            assert_eq!(identity.manifest_dir, Path::new("/nowhere/my-app"));
            assert_eq!(identity.icon, None);
            assert_eq!(identity.icon_foreground, None);
            assert_eq!(identity.icon_background, None);
        }
    }

    #[test]
    fn declared_values_are_used() {
        let identity = identity(
            r##"{"morningstar":{"name":"Fundi – Desk \"2\"","id":"com.example.fundi_desk",
                 "icon_background":"#1e8416"}}"##,
        )
        .unwrap();
        assert_eq!(identity.name, "Fundi – Desk \"2\"");
        assert_eq!(identity.id, "com.example.fundi_desk");
        assert_eq!(identity.bundle_id(), "com.example.fundi-desk");
        assert_eq!(identity.icon_background, Some([0x1E, 0x84, 0x16]));
    }

    #[test]
    fn an_unknown_key_is_an_error_that_names_it() {
        let error = identity(r#"{"morningstar":{"name":"A","icno":"assets/icon.png"}}"#)
            .unwrap_err()
            .to_string();
        assert!(error.contains("unknown key `icno`"), "{error}");
        assert!(error.contains("icon_foreground"), "lists the keys: {error}");
    }

    #[test]
    fn a_value_that_is_not_a_string_is_an_error_that_names_the_key() {
        for (metadata, expected) in [
            (r#"{"morningstar":{"name":7}}"#, "`name`"),
            (r#"{"morningstar":{"icon":["a.png"]}}"#, "`icon`"),
            (
                r#"{"morningstar":{"icon_background":{"r":1}}}"#,
                "`icon_background`",
            ),
            (r#"{"morningstar":"CallReady"}"#, "must be a table"),
        ] {
            let error = identity(metadata).unwrap_err().to_string();
            assert!(error.contains(expected), "{metadata}: {error}");
        }
    }

    #[test]
    fn bad_names_ids_and_colours_are_refused() {
        for (metadata, expected) in [
            (r#"{"morningstar":{"name":""}}"#, "is empty"),
            (r#"{"morningstar":{"name":" A"}}"#, "space"),
            (r#"{"morningstar":{"name":"A\nB"}}"#, "control character"),
            (r#"{"morningstar":{"name":"@string/app"}}"#, "reference"),
            (r#"{"morningstar":{"name":"A // B"}}"#, "iOS"),
            (r#"{"morningstar":{"id":"callready"}}"#, "`id"),
            (r#"{"morningstar":{"id":"dev.9lives.app"}}"#, "`id"),
            (r#"{"morningstar":{"id":"dev.my-app.app"}}"#, "`id"),
            (r#"{"morningstar":{"id":"dev..app"}}"#, "`id"),
            (r#"{"morningstar":{"icon_background":"0E0E0E"}}"#, "#RRGGBB"),
            (
                r##"{"morningstar":{"icon_background":"#0E0E0"}}"##,
                "#RRGGBB",
            ),
            (
                r##"{"morningstar":{"icon_background":"#GGGGGG"}}"##,
                "#RRGGBB",
            ),
            (r#"{"morningstar":{"icon_background":"red"}}"#, "#RRGGBB"),
            (
                r##"{"morningstar":{"icon_background":"#é0E0E"}}"##,
                "#RRGGBB",
            ),
        ] {
            let error = identity(metadata).unwrap_err().to_string();
            assert!(error.contains(expected), "{metadata}: {error}");
        }
        assert_eq!(parse_colour("#fa0").unwrap(), [0xFF, 0xAA, 0x00]);
        assert_eq!(parse_colour("#0E0e0E").unwrap(), [14, 14, 14]);
    }

    #[test]
    fn a_missing_icon_file_is_an_error_that_names_the_key_and_the_path() {
        let error = identity(r#"{"morningstar":{"icon":"art/icon.png"}}"#)
            .unwrap_err()
            .to_string();
        assert!(error.contains("`icon`"), "{error}");
        assert!(error.contains("/nowhere/my-app/art/icon.png"), "{error}");

        let error = identity(r#"{"morningstar":{"icon_foreground":"fg.png"}}"#)
            .unwrap_err()
            .to_string();
        assert!(error.contains("needs `icon`"), "{error}");
    }

    #[test]
    fn a_package_that_is_not_there_is_an_error_that_lists_the_others() {
        let json = metadata_json("my-app", Path::new("/nowhere/my-app"), "null");
        let error = Identity::from_cargo_metadata(&json, "myapp")
            .unwrap_err()
            .to_string();
        assert!(error.contains("`myapp`"), "{error}");
        assert!(error.contains("other, my-app"), "{error}");
    }

    #[test]
    fn output_that_is_not_cargo_metadata_is_an_error() {
        for text in ["", "{", "[]", r#"{"packages":7}"#] {
            assert!(
                matches!(
                    Identity::from_cargo_metadata(text, "a"),
                    Err(IconError::Metadata(_))
                ),
                "{text}"
            );
        }
    }

    /// The whole route, through Cargo itself: a manifest written the odd
    /// ways TOML allows means what Cargo says it means.
    #[test]
    fn a_real_manifest_is_read_through_cargo() {
        let dir = std::env::temp_dir().join(format!("morningstar-identity-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(dir.join("src/lib.rs"), "").unwrap();
        std::fs::write(dir.join("assets/icon.png"), b"checked later").unwrap();
        let manifest = |table: &str| {
            std::fs::write(
                dir.join("Cargo.toml"),
                format!(
                    "[package]\nname = \"odd-app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\
                     [lib]\nname = \"odd_core\"\ncrate-type = [\"cdylib\", \"lib\"]\n\
                     [workspace] # stands alone wherever the temporary directory is\n{table}"
                ),
            )
            .unwrap();
            let metadata = cargo_metadata(Some(&dir.join("Cargo.toml")))?;
            Identity::from_cargo_metadata(&metadata, "odd-app")
        };

        // Comments, a literal string, an escape, and keys in any order.
        let identity = manifest(
            "# The app's identity.\n\
             [package.metadata.morningstar] # read by morningstar-icon\n\
             icon_background = '#0E0E0E' # the app's black\n\
             name = \"Odd \\u2013 \\\"App\\\" # not a comment\"\n",
        )
        .unwrap();
        assert_eq!(identity.name, "Odd – \"App\" # not a comment");
        assert_eq!(identity.icon_background, Some([14, 14, 14]));
        // The [lib] name, not the package's, is what the host loads.
        assert_eq!(identity.lib, "odd_core");
        assert_eq!(identity.id, "dev.morningstar.odd_core");
        // assets/icon.png exists, so it is the icon without being named.
        assert_eq!(
            identity
                .icon
                .as_deref()
                .map(|path| path.ends_with(DEFAULT_ICON)),
            Some(true)
        );

        // No table at all.
        let identity = manifest("").unwrap();
        assert_eq!(identity.name, "odd-app");

        // An inline table and dotted keys are the same table to Cargo.
        let identity = manifest("[package.metadata]\nmorningstar = { name = 'Inline' }\n").unwrap();
        assert_eq!(identity.name, "Inline");
        let identity = manifest("[package.metadata]\nmorningstar.name = 'Dotted'\n").unwrap();
        assert_eq!(identity.name, "Dotted");

        // A misspelt key and a value of the wrong type.
        let error = manifest("[package.metadata.morningstar]\nnmae = 'A'\n").unwrap_err();
        assert!(error.to_string().contains("unknown key `nmae`"), "{error}");
        let error = manifest("[package.metadata.morningstar]\nname = true\n").unwrap_err();
        assert!(error.to_string().contains("not a boolean"), "{error}");

        // A manifest Cargo cannot read is Cargo's error, passed on.
        let error = manifest("[package.metadata.morningstar]\nname = 'A\n").unwrap_err();
        assert!(matches!(error, IconError::Cargo(_)), "{error}");

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
