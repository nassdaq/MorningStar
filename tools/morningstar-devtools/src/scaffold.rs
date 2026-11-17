//! A new application from nothing (ADR 0041): the files of a complete app
//! crate, written from templates kept in this binary, and its entry among
//! the workspace's members.
//!
//! ```no_run
//! use morningstar_devtools::{NewApp, create_app};
//!
//! let app = NewApp::new("my-app")?;
//! let created = create_app(std::path::Path::new("."), &app)?;
//! println!("{} files in {}", created.files.len(), created.directory.display());
//! # Ok::<(), morningstar_devtools::ScaffoldError>(())
//! ```
//!
//! What the app is called and what its host is built with is validated by
//! the same rules that read it back ([`Identity`](crate::Identity)), so an
//! app that was generated is an app that builds.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::identity::{SERVICES, checked_id, checked_name};
use crate::image::Image;

/// Where a new app goes when `--dir` does not say (ADR 0041).
pub const DEFAULT_APP_DIR: &str = "apps";
/// The port a generated server listens on when `--port` does not say.
pub const DEFAULT_SERVER_PORT: u16 = 8790;
/// The longest package name taken. It keeps every generated line of Rust
/// within what `rustfmt` leaves alone.
pub const MAX_PACKAGE_LENGTH: usize = 40;

/// The brand colours a new app may be given, as sRGB. Each carries white
/// text at 4.5 to 1 or better, so the generated screens pass the
/// accessibility check with any of them.
const ACCENTS: [[u8; 3]; 6] = [
    [0x2F, 0x5B, 0xD9],
    [0x0F, 0x76, 0x6E],
    [0x7C, 0x3A, 0xED],
    [0xB4, 0x37, 0x1F],
    [0x1D, 0x6F, 0x42],
    [0xA2, 0x1C, 0x5B],
];

/// The side of the generated launcher icon, in pixels.
const ICON_SIDE: u32 = 1024;

/// Why an app could not be created. Every message says what to change.
#[derive(Debug)]
pub enum ScaffoldError {
    /// The package name cannot be a Cargo package and a library.
    BadPackage {
        /// The name given.
        package: String,
        /// What is wrong with it, after "it".
        reason: &'static str,
    },
    /// The display name or the application id was refused; the text is the
    /// reason, as the icon tool would give it.
    BadIdentity(String),
    /// A service that does not exist, or one without the one it needs.
    BadServices(String),
    /// The directory the app would go in is not a plain relative path.
    BadDirectory(String),
    /// The port is not one a server can be given.
    BadPort(String),
    /// Something is already where the app would go.
    Exists(PathBuf),
    /// The workspace already has a package of that name.
    AlreadyMember(String),
    /// The workspace's `Cargo.toml` is missing or has no member list.
    Workspace(String),
    /// A file could not be read or written.
    Io(PathBuf, std::io::Error),
}

impl fmt::Display for ScaffoldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadPackage { package, reason } => write!(
                f,
                "`{}` cannot be the app's package name: it {reason}. Use lower-case letters, \
                 digits and hyphens, starting with a letter, like `my-app`",
                package.escape_debug()
            ),
            Self::BadIdentity(reason) => f.write_str(reason),
            Self::BadServices(reason) => write!(
                f,
                "--services {reason} (the services are: {})",
                SERVICES.join(", ")
            ),
            Self::BadDirectory(directory) => write!(
                f,
                "--dir {directory}: expected a directory inside the workspace, written \
                 relative to its root, like `apps`"
            ),
            Self::BadPort(port) => write!(f, "--port {port}: expected a number from 1024 to 65535"),
            Self::Exists(path) => write!(
                f,
                "{} already exists; nothing was written. Choose another name or remove it",
                path.display()
            ),
            Self::AlreadyMember(package) => write!(
                f,
                "the workspace already has a package called `{package}`; nothing was written"
            ),
            Self::Workspace(reason) => f.write_str(reason),
            Self::Io(path, error) => write!(f, "{}: {error}", path.display()),
        }
    }
}

impl std::error::Error for ScaffoldError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(_, error) => Some(error),
            _ => None,
        }
    }
}

/// What to create. Every field has been checked: build one with
/// [`NewApp::new`] and the `with_` methods.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewApp {
    package: String,
    name: String,
    id: String,
    services: Vec<&'static str>,
    server: bool,
    port: u16,
    directory: String,
}

impl NewApp {
    /// An app called `package`, with a name and an id made from it
    /// (`my-app`: "My App", `dev.morningstar.my_app`), no services, no
    /// server, in [`DEFAULT_APP_DIR`].
    pub fn new(package: &str) -> Result<Self, ScaffoldError> {
        let package = checked_package(package)?;
        let name = package
            .split('-')
            .map(|word| {
                let mut letters = word.chars();
                letters.next().map_or(String::new(), |first| {
                    first.to_ascii_uppercase().to_string() + letters.as_str()
                })
            })
            .collect::<Vec<_>>()
            .join(" ");
        Ok(Self {
            id: format!("dev.morningstar.{}", package.replace('-', "_")),
            package,
            name,
            services: Vec::new(),
            server: false,
            port: DEFAULT_SERVER_PORT,
            directory: DEFAULT_APP_DIR.to_owned(),
        })
    }

    /// The name under the launcher icon.
    pub fn with_name(mut self, name: &str) -> Result<Self, ScaffoldError> {
        self.name =
            checked_name(name).map_err(|error| ScaffoldError::BadIdentity(error.to_string()))?;
        Ok(self)
    }

    /// The Android application id (and, lower-cased by the host's build,
    /// the iOS bundle id).
    pub fn with_id(mut self, id: &str) -> Result<Self, ScaffoldError> {
        self.id = checked_id(id).map_err(|error| ScaffoldError::BadIdentity(error.to_string()))?;
        Ok(self)
    }

    /// The opt-in platform services, as `live,live-video`. An empty string
    /// is none.
    pub fn with_services(mut self, services: &str) -> Result<Self, ScaffoldError> {
        let mut wanted = Vec::new();
        for name in services.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let Some(known) = SERVICES.iter().find(|known| **known == name) else {
                return Err(ScaffoldError::BadServices(format!(
                    "names `{}`, which is not a service",
                    name.escape_debug()
                )));
            };
            if !wanted.contains(known) {
                wanted.push(*known);
            }
        }
        if let Some(part) = wanted.iter().find(|name| **name != "live") {
            if !wanted.contains(&"live") {
                return Err(ScaffoldError::BadServices(format!(
                    "names `{part}` without `live`, which it is a part of"
                )));
            }
        }
        // In the order the services are listed in, as `Identity` reads them.
        self.services = SERVICES
            .iter()
            .copied()
            .filter(|known| wanted.contains(known))
            .collect();
        Ok(self)
    }

    /// Whether a server skeleton is written beside the app, and the app
    /// talks to it.
    pub fn with_server(mut self, server: bool) -> Self {
        self.server = server;
        self
    }

    /// The port the server listens on.
    pub fn with_port(mut self, port: &str) -> Result<Self, ScaffoldError> {
        match port.parse::<u16>() {
            Ok(port) if port >= 1024 => {
                self.port = port;
                Ok(self)
            }
            _ => Err(ScaffoldError::BadPort(port.escape_debug().to_string())),
        }
    }

    /// The directory, relative to the workspace root, the app's own
    /// directory is made in.
    pub fn with_directory(mut self, directory: &str) -> Result<Self, ScaffoldError> {
        let trimmed = directory.trim_matches('/');
        let plain = !trimmed.is_empty()
            && !directory.starts_with('/')
            && trimmed.split('/').all(|part| {
                !part.is_empty()
                    && part != "."
                    && part != ".."
                    && part
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            });
        if !plain {
            return Err(ScaffoldError::BadDirectory(
                directory.escape_debug().to_string(),
            ));
        }
        self.directory = trimmed.to_owned();
        Ok(self)
    }

    /// The Cargo package.
    pub fn package(&self) -> &str {
        &self.package
    }

    /// The name under the launcher icon.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The application id.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The library's name: the package with underscores.
    pub fn crate_name(&self) -> String {
        self.package.replace('-', "_")
    }

    /// Whether the app has a server.
    pub fn server(&self) -> bool {
        self.server
    }

    /// The server's port.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The app's directory relative to the workspace root, which is also
    /// its entry among the workspace's members.
    pub fn member(&self) -> String {
        format!("{}/{}", self.directory, self.package)
    }

    /// The app's brand colour: one of six, chosen by its name so that two
    /// apps on one phone are seldom the same colour.
    pub fn accent(&self) -> [u8; 3] {
        // FNV-1a: the same choice on every machine and in every release.
        let hash = self.package.bytes().fold(0x811C_9DC5_u32, |hash, byte| {
            (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
        });
        ACCENTS[hash as usize % ACCENTS.len()]
    }

    /// Every file of the app, as its path inside the app's directory and
    /// its content.
    pub fn files(&self) -> Vec<(PathBuf, Vec<u8>)> {
        let mut files: Vec<(PathBuf, Vec<u8>)> = Vec::new();
        let mut add = |path: &str, template: &str| {
            files.push((PathBuf::from(path), self.render(template).into_bytes()));
        };
        add(
            "Cargo.toml",
            include_str!("../templates/app/Cargo.toml.tmpl"),
        );
        add("README.md", include_str!("../templates/app/README.md.tmpl"));
        add(
            "SCREENS.md",
            include_str!("../templates/app/SCREENS.md.tmpl"),
        );
        add(
            "design/README.md",
            include_str!("../templates/app/design/README.md.tmpl"),
        );
        add(
            "src/lib.rs",
            include_str!("../templates/app/src/lib.rs.tmpl"),
        );
        add(
            "examples/screenshot.rs",
            include_str!("../templates/app/examples/screenshot.rs.tmpl"),
        );
        add(
            "tests/support/mod.rs",
            include_str!("../templates/app/tests/support/mod.rs.tmpl"),
        );
        add(
            "tests/e2e.rs",
            include_str!("../templates/app/tests/e2e.rs.tmpl"),
        );
        add(
            "tests/accessibility.rs",
            include_str!("../templates/app/tests/accessibility.rs.tmpl"),
        );
        add(
            "tests/screens.rs",
            include_str!("../templates/app/tests/screens.rs.tmpl"),
        );
        if self.server {
            add(
                "src/json.rs",
                include_str!("../templates/app/src/json.rs.tmpl"),
            );
            add(
                "server/package.json",
                include_str!("../templates/app/server/package.json.tmpl"),
            );
            add(
                "server/.env.example",
                include_str!("../templates/app/server/env.example.tmpl"),
            );
            add(
                "server/.gitignore",
                include_str!("../templates/app/server/gitignore.tmpl"),
            );
            add(
                "server/API.md",
                include_str!("../templates/app/server/API.md.tmpl"),
            );
            add(
                "server/README.md",
                include_str!("../templates/app/server/README.md.tmpl"),
            );
            add(
                "server/src/config.js",
                include_str!("../templates/app/server/src/config.js.tmpl"),
            );
            add(
                "server/src/app.js",
                include_str!("../templates/app/server/src/app.js.tmpl"),
            );
            add(
                "server/src/server.js",
                include_str!("../templates/app/server/src/server.js.tmpl"),
            );
            add(
                "server/test/app.test.js",
                include_str!("../templates/app/server/test/app.test.js.tmpl"),
            );
        }
        // The fonts the first screens are set in: Noto Sans, under the SIL
        // Open Font License, which travels with them.
        files.push((
            "assets/fonts/NotoSans-Regular.ttf".into(),
            include_bytes!("../../../examples/hello-gpu/assets/fonts/NotoSans-Regular.ttf")
                .to_vec(),
        ));
        files.push((
            "assets/fonts/NotoSans-Bold.ttf".into(),
            include_bytes!("../../../examples/hello-gpu/assets/fonts/NotoSans-Bold.ttf").to_vec(),
        ));
        files.push((
            "assets/fonts/OFL.txt".into(),
            include_bytes!("../../../examples/hello-gpu/assets/fonts/OFL.txt").to_vec(),
        ));
        files.push(("assets/icon.png".into(), icon(self.accent()).to_png()));
        files
    }

    /// `template` with its `@@if server` blocks resolved and its
    /// `{{names}}` filled in.
    fn render(&self, template: &str) -> String {
        let accent = self.accent();
        let services = self
            .services
            .iter()
            .map(|service| format!("\"{service}\""))
            .collect::<Vec<_>>()
            .join(", ");
        // The name inside a Rust, TOML or JSON string: all three escape a
        // quote and a backslash the same way, and the name has no control
        // characters.
        let name_str = self.name.replace('\\', "\\\\").replace('"', "\\\"");
        let values = [
            ("package", self.package.clone()),
            ("crate", self.crate_name()),
            ("name", self.name.clone()),
            ("name_str", name_str.clone()),
            ("name_json", name_str),
            ("id", self.id.clone()),
            ("services", services),
            ("dir", self.directory.clone()),
            // From the app's directory back to the workspace root.
            ("up", "../".repeat(self.directory.split('/').count() + 1)),
            ("port", self.port.to_string()),
            ("env", self.crate_name().to_ascii_uppercase()),
            (
                "accent",
                format!("{:02X}{:02X}{:02X}", accent[0], accent[1], accent[2]),
            ),
            (
                "plays",
                if self.server {
                    "the phone's storage and the server".to_owned()
                } else {
                    "the phone's storage".to_owned()
                },
            ),
        ];
        let mut out = String::with_capacity(template.len());
        // Whether lines are kept, for each open `@@if`.
        let mut keeping: Vec<bool> = Vec::new();
        for line in template.split_inclusive('\n') {
            match line.trim() {
                "@@if server" => keeping.push(self.server),
                "@@else" => {
                    if let Some(kept) = keeping.last_mut() {
                        *kept = !*kept;
                    }
                }
                "@@end" => {
                    keeping.pop();
                }
                _ if keeping.iter().all(|kept| *kept) => out.push_str(line),
                _ => {}
            }
        }
        for (key, value) in values {
            out = out.replace(&format!("{{{{{key}}}}}"), &value);
        }
        out
    }
}

/// A package name Cargo, the Android build and a Rust `use` all take:
/// lower-case letters, digits and single hyphens, starting with a letter.
fn checked_package(package: &str) -> Result<String, ScaffoldError> {
    let bad = |reason| {
        Err(ScaffoldError::BadPackage {
            package: package.to_owned(),
            reason,
        })
    };
    if package.is_empty() {
        return bad("is empty");
    }
    if package.len() > MAX_PACKAGE_LENGTH {
        return bad("is longer than 40 characters");
    }
    if !package.starts_with(|first: char| first.is_ascii_lowercase()) {
        return bad("does not start with a lower-case letter");
    }
    if !package
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return bad("has a character that is not a lower-case letter, a digit or a hyphen");
    }
    if package.ends_with('-') || package.contains("--") {
        return bad("has a hyphen at its end or two in a row");
    }
    // Names that are a crate already: the app could not `use` itself, or
    // would shadow what it depends on.
    const TAKEN: [&str; 8] = [
        "core",
        "std",
        "alloc",
        "test",
        "proc-macro",
        "morningstar",
        "self",
        "crate",
    ];
    if TAKEN.contains(&package) || package.starts_with("morningstar-") {
        return bad("is the name of a crate the app is built from");
    }
    Ok(package.to_owned())
}

/// The launcher icon of a new app: a four-pointed star in white on the
/// app's colour, full-bleed, drawn here so that a new app has an icon of
/// its own before anybody has designed one. The star stays inside the
/// middle 61% of the square, which every Android mask keeps.
fn icon(accent: [u8; 3]) -> Image {
    let side = ICON_SIDE as usize;
    let background = [accent[0], accent[1], accent[2], 255];
    let mut image = Image::filled(ICON_SIDE, ICON_SIDE, background);
    let centre = side as f32 / 2.0;
    // The points reach 30% of the side from the centre.
    let reach = side as f32 * 0.30;
    // |x|^p + |y|^p = r^p with p below 1 is a star with hollow sides.
    let power = 0.62_f32;
    let limit = reach.powf(power);
    // Coverage from a 4 x 4 grid of samples in each pixel.
    const GRID: usize = 4;
    let first = (centre - reach).floor() as usize;
    let last = (centre + reach).ceil() as usize;
    for y in first..last {
        for x in first..last {
            let mut inside = 0;
            for sample_y in 0..GRID {
                for sample_x in 0..GRID {
                    let dx = x as f32 + (sample_x as f32 + 0.5) / GRID as f32 - centre;
                    let dy = y as f32 + (sample_y as f32 + 0.5) / GRID as f32 - centre;
                    if dx.abs().powf(power) + dy.abs().powf(power) <= limit {
                        inside += 1;
                    }
                }
            }
            if inside == 0 {
                continue;
            }
            let cover = inside as f32 / (GRID * GRID) as f32;
            // Blended on the encoded values, as the renderer and the
            // launchers composite (ADR 0023, 0030).
            let mix = |under: u8| (f32::from(under) + (255.0 - f32::from(under)) * cover).round();
            image.set_pixel(
                x as u32,
                y as u32,
                [
                    mix(accent[0]) as u8,
                    mix(accent[1]) as u8,
                    mix(accent[2]) as u8,
                    255,
                ],
            );
        }
    }
    image
}

/// What [`create_app`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatedApp {
    /// The app's directory.
    pub directory: PathBuf,
    /// The files written, relative to the workspace root, in the order
    /// they were written.
    pub files: Vec<PathBuf>,
    /// The entry added to the workspace's members.
    pub member: String,
}

/// `manifest` (a workspace's `Cargo.toml`) with `member` added to the end
/// of `[workspace] members`. The rest of the file is returned untouched,
/// byte for byte.
pub fn add_workspace_member(manifest: &str, member: &str) -> Result<String, ScaffoldError> {
    let missing = || {
        ScaffoldError::Workspace(
            "the workspace's Cargo.toml has no `members = [...]` list under [workspace] to \
             add the app to"
                .to_owned(),
        )
    };
    let section = manifest.find("[workspace]").ok_or_else(missing)?;
    // The key at the start of a line, so that `default-members` is not it.
    let key = manifest[section..]
        .match_indices("members")
        .map(|(at, _)| section + at)
        .find(|at| {
            manifest[..*at]
                .rsplit('\n')
                .next()
                .is_some_and(|before| before.trim().is_empty())
        })
        .ok_or_else(missing)?;
    let open = key + manifest[key..].find('[').ok_or_else(missing)?;
    let close = open + manifest[open..].find(']').ok_or_else(missing)?;
    let inside = &manifest[open + 1..close];
    if inside
        .split(',')
        .any(|entry| entry.trim().trim_matches('"') == member)
    {
        return Err(ScaffoldError::AlreadyMember(member.to_owned()));
    }
    let entry = format!("    \"{member}\",\n");
    let mut out = String::with_capacity(manifest.len() + entry.len() + 2);
    if inside.contains('\n') {
        // One member a line: the new one goes on a line of its own before
        // the closing bracket, after a comma if the last one has none.
        let kept = inside.trim_end_matches([' ', '\t']);
        out.push_str(&manifest[..=open]);
        out.push_str(kept.trim_end());
        if !kept.trim_end().is_empty() && !kept.trim_end().ends_with(',') {
            out.push(',');
        }
        out.push('\n');
        out.push_str(&entry);
    } else {
        // `members = []` or `members = ["a", "b"]`: written out a line each.
        out.push_str(&manifest[..=open]);
        out.push('\n');
        for existing in inside.split(',').map(str::trim).filter(|e| !e.is_empty()) {
            out.push_str(&format!("    {existing},\n"));
        }
        out.push_str(&entry);
    }
    out.push_str(&manifest[close..]);
    Ok(out)
}

/// The names of the packages `manifest` lists as members: the last part of
/// each path.
fn member_packages(manifest: &str) -> Vec<String> {
    let Some(open) = manifest
        .find("[workspace]")
        .and_then(|section| manifest[section..].find("members").map(|at| section + at))
        .and_then(|key| manifest[key..].find('[').map(|at| key + at))
    else {
        return Vec::new();
    };
    let close = manifest[open..]
        .find(']')
        .map_or(manifest.len(), |at| open + at);
    manifest[open + 1..close]
        .split(',')
        .map(|entry| entry.trim().trim_matches('"'))
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| entry.rsplit('/').next().map(str::to_owned))
        .collect()
}

/// Creates `app` in the workspace whose root is `workspace`: writes its
/// files and adds it to the members. Nothing is written if the app's
/// directory exists or the workspace already has a package of its name.
pub fn create_app(workspace: &Path, app: &NewApp) -> Result<CreatedApp, ScaffoldError> {
    let manifest_path = workspace.join("Cargo.toml");
    let manifest = std::fs::read_to_string(&manifest_path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            ScaffoldError::Workspace(format!(
                "{} is not a workspace: it has no Cargo.toml. Run this from the repository \
                 root, or name it with --workspace",
                workspace.display()
            ))
        } else {
            ScaffoldError::Io(manifest_path.clone(), error)
        }
    })?;
    if member_packages(&manifest).contains(&app.package) {
        return Err(ScaffoldError::AlreadyMember(app.package.clone()));
    }
    let member = app.member();
    let directory = workspace.join(&member);
    if directory.exists() {
        return Err(ScaffoldError::Exists(directory));
    }
    // Decided before anything is written: a manifest that cannot take the
    // member leaves no half-made app behind.
    let with_member = add_workspace_member(&manifest, &member)?;

    let mut written = Vec::new();
    for (path, content) in app.files() {
        let target = directory.join(&path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| ScaffoldError::Io(parent.to_owned(), error))?;
        }
        std::fs::write(&target, content).map_err(|error| ScaffoldError::Io(target, error))?;
        written.push(Path::new(&member).join(path));
    }
    std::fs::write(&manifest_path, with_member)
        .map_err(|error| ScaffoldError::Io(manifest_path, error))?;
    Ok(CreatedApp {
        directory,
        files: written,
        member,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_of(app: &NewApp, path: &str) -> String {
        let (_, content) = app
            .files()
            .into_iter()
            .find(|(file, _)| file == Path::new(path))
            .unwrap_or_else(|| panic!("{path} is among the files"));
        String::from_utf8(content).expect("a text file")
    }

    #[test]
    fn a_package_name_gives_the_name_the_id_and_the_place() {
        let app = NewApp::new("my-app").unwrap();
        assert_eq!(app.package(), "my-app");
        assert_eq!(app.crate_name(), "my_app");
        assert_eq!(app.name(), "My App");
        assert_eq!(app.id(), "dev.morningstar.my_app");
        assert_eq!(app.member(), "apps/my-app");
        assert!(!app.server());
        assert_eq!(app.port(), DEFAULT_SERVER_PORT);
        assert_eq!(NewApp::new("shop2go").unwrap().name(), "Shop2go");
    }

    #[test]
    fn package_names_that_cannot_work_are_refused_with_the_reason() {
        for (package, reason) in [
            ("", "is empty"),
            ("My-App", "does not start with a lower-case letter"),
            ("2fast", "does not start with a lower-case letter"),
            ("my_app", "a lower-case letter, a digit or a hyphen"),
            ("my app", "a lower-case letter, a digit or a hyphen"),
            ("café", "a lower-case letter, a digit or a hyphen"),
            ("my--app", "two in a row"),
            ("my-app-", "at its end"),
            ("test", "a crate the app is built from"),
            ("morningstar", "a crate the app is built from"),
            ("morningstar-ui", "a crate the app is built from"),
            (
                "a-name-that-goes-on-for-longer-than-forty",
                "longer than 40",
            ),
        ] {
            let error = NewApp::new(package).unwrap_err().to_string();
            assert!(error.contains(reason), "{package}: {error}");
            assert!(error.contains("like `my-app`"), "{error}");
        }
        assert!(NewApp::new(&"a".repeat(MAX_PACKAGE_LENGTH)).is_ok());
    }

    #[test]
    fn the_name_and_the_id_are_held_to_the_rules_that_read_them_back() {
        let app = NewApp::new("my-app").unwrap();
        let named = app.clone().with_name("Duka la Neema").unwrap();
        assert_eq!(named.name(), "Duka la Neema");
        let error = app.clone().with_name(" ").unwrap_err().to_string();
        assert!(error.contains("is empty"), "{error}");
        let error = app.clone().with_name("@string/x").unwrap_err().to_string();
        assert!(error.contains("resource"), "{error}");

        let with_id = app.clone().with_id("com.example.myapp").unwrap();
        assert_eq!(with_id.id(), "com.example.myapp");
        for id in ["myapp", "com.example.my-app", "com..app", "com.1app"] {
            let error = app.clone().with_id(id).unwrap_err().to_string();
            assert!(error.contains("like dev.example.app"), "{id}: {error}");
        }
    }

    #[test]
    fn services_are_known_ordered_and_whole() {
        let app = NewApp::new("my-app").unwrap();
        let live = app.clone().with_services("live-video, live,live").unwrap();
        assert!(text_of(&live, "Cargo.toml").contains("services = [\"live\", \"live-video\"]"));
        assert!(text_of(&app, "Cargo.toml").contains("services = []"));
        assert!(
            text_of(&app.clone().with_services("").unwrap(), "Cargo.toml")
                .contains("services = []")
        );

        let error = app.clone().with_services("camera").unwrap_err().to_string();
        assert!(
            error.contains("`camera`, which is not a service"),
            "{error}"
        );
        assert!(error.contains("live, live-video, live-screen"), "{error}");
        let error = app
            .clone()
            .with_services("live-screen")
            .unwrap_err()
            .to_string();
        assert!(error.contains("without `live`"), "{error}");
    }

    #[test]
    fn the_directory_and_the_port_are_checked() {
        let app = NewApp::new("my-app").unwrap();
        assert_eq!(
            app.clone().with_directory("examples/").unwrap().member(),
            "examples/my-app"
        );
        assert_eq!(
            app.clone().with_directory("apps/mine").unwrap().member(),
            "apps/mine/my-app"
        );
        let deeper = app.clone().with_directory("apps/mine").unwrap();
        assert!(text_of(&deeper, "README.md").contains("](../../../AGENTS.md)"));
        assert!(text_of(&app, "README.md").contains("](../../AGENTS.md)"));
        for directory in ["", "/", "/tmp", "../out", "apps/../..", "a b", "apps//x"] {
            assert!(
                matches!(
                    app.clone().with_directory(directory),
                    Err(ScaffoldError::BadDirectory(_))
                ),
                "{directory}"
            );
        }
        assert_eq!(app.clone().with_port("9001").unwrap().port(), 9001);
        for port in ["80", "0", "70000", "http", ""] {
            assert!(
                matches!(app.clone().with_port(port), Err(ScaffoldError::BadPort(_))),
                "{port}"
            );
        }
    }

    #[test]
    fn a_plain_app_has_its_files_and_no_trace_of_a_server() {
        let app = NewApp::new("my-app").unwrap();
        let files = app.files();
        let paths: Vec<String> = files
            .iter()
            .map(|(path, _)| path.display().to_string())
            .collect();
        assert_eq!(
            paths,
            [
                "Cargo.toml",
                "README.md",
                "SCREENS.md",
                "design/README.md",
                "src/lib.rs",
                "examples/screenshot.rs",
                "tests/support/mod.rs",
                "tests/e2e.rs",
                "tests/accessibility.rs",
                "tests/screens.rs",
                "assets/fonts/NotoSans-Regular.ttf",
                "assets/fonts/NotoSans-Bold.ttf",
                "assets/fonts/OFL.txt",
                "assets/icon.png",
            ]
        );
        for (path, content) in &files {
            let Ok(text) = std::str::from_utf8(content) else {
                continue;
            };
            let name = path.display();
            assert!(!text.contains("@@"), "{name}: a directive was left in");
            for word in ["server", "json", "Http", "SERVER_URL"] {
                assert!(!text.contains(word), "{name} mentions {word}");
            }
        }
        let library = text_of(&app, "src/lib.rs");
        assert!(library.contains("morningstar::app!(Ui::new(Model::new()));"));
        assert!(library.contains("pub const APP_NAME: &str = \"My App\";"));
        let manifest = text_of(&app, "Cargo.toml");
        assert!(manifest.contains("name = \"my-app\""));
        assert!(manifest.contains("id = \"dev.morningstar.my_app\""));
        assert!(manifest.contains("icon = \"assets/icon.png\""));
        assert!(manifest.contains("crate-type = [\"cdylib\", \"lib\"]"));
        assert!(text_of(&app, "tests/e2e.rs").contains("use my_app::{Model, NAME_KEY, Screen};"));
    }

    #[test]
    fn a_server_adds_the_skeleton_and_the_app_that_talks_to_it() {
        let app = NewApp::new("duka")
            .unwrap()
            .with_server(true)
            .with_port("9100")
            .unwrap();
        let paths: Vec<String> = app
            .files()
            .iter()
            .map(|(path, _)| path.display().to_string())
            .collect();
        for path in [
            "src/json.rs",
            "server/package.json",
            "server/.env.example",
            "server/.gitignore",
            "server/API.md",
            "server/README.md",
            "server/src/config.js",
            "server/src/app.js",
            "server/src/server.js",
            "server/test/app.test.js",
        ] {
            assert!(paths.iter().any(|have| have == path), "{path} is missing");
        }
        // The real settings file is never written, only its example.
        assert!(!paths.iter().any(|path| path == "server/.env"));
        assert!(text_of(&app, "server/.gitignore").contains(".env\n"));
        let example = text_of(&app, "server/.env.example");
        assert!(example.contains("PORT=9100"));
        assert!(example.contains("APP_TOKEN=\n"), "no value for the token");

        let library = text_of(&app, "src/lib.rs");
        assert!(library.contains("option_env!(\"DUKA_SERVER_URL\")"));
        assert!(library.contains("None => \"http://127.0.0.1:9100\","));
        assert!(library.contains("mod json;"));
        assert!(text_of(&app, "server/src/config.js").contains("whole(get('PORT'), 9100,"));
        assert!(text_of(&app, "README.md").contains("adb reverse tcp:9100 tcp:9100"));
    }

    #[test]
    fn nothing_is_left_unfilled_and_no_line_of_rust_is_too_long() {
        // The longest values there can be: `rustfmt` must have nothing to
        // rewrap, because the generated crate is checked with the rest.
        let longest = NewApp::new(&"a".repeat(MAX_PACKAGE_LENGTH))
            .unwrap()
            .with_name(&"N".repeat(60))
            .unwrap()
            .with_server(true)
            .with_port("65535")
            .unwrap();
        for app in [longest, NewApp::new("ab").unwrap()] {
            for (path, content) in app.files() {
                let Ok(text) = String::from_utf8(content) else {
                    continue;
                };
                let name = path.display();
                assert!(!text.contains("@@"), "{name}: a directive was left in");
                for key in [
                    "package",
                    "crate",
                    "name",
                    "name_str",
                    "name_json",
                    "id",
                    "services",
                    "dir",
                    "port",
                    "env",
                    "accent",
                    "plays",
                    "up",
                ] {
                    assert!(
                        !text.contains(&format!("{{{{{key}}}}}")),
                        "{name}: {key} is unfilled"
                    );
                }
                if path.extension().is_some_and(|extension| extension == "rs") {
                    for line in text.lines() {
                        // `rustfmt` leaves a long comment as it is.
                        let code = !line.trim_start().starts_with("//");
                        assert!(!code || line.chars().count() <= 100, "{name}: {line}");
                        assert_eq!(line, line.trim_end(), "{name}: trailing space");
                    }
                }
            }
        }
    }

    #[test]
    fn a_name_with_quotes_is_escaped_where_it_is_inside_a_string() {
        let app = NewApp::new("my-app")
            .unwrap()
            .with_name("Shop \"Talk\" \\ Co")
            .unwrap()
            .with_server(true);
        let escaped = "Shop \\\"Talk\\\" \\\\ Co";
        assert!(text_of(&app, "Cargo.toml").contains(&format!("name = \"{escaped}\"")));
        assert!(
            text_of(&app, "src/lib.rs")
                .contains(&format!("pub const APP_NAME: &str = \"{escaped}\";"))
        );
        assert!(text_of(&app, "server/package.json").contains(escaped));
        assert!(text_of(&app, "README.md").starts_with("# Shop \"Talk\" \\ Co\n"));
    }

    #[test]
    fn the_colour_is_one_of_the_six_and_the_same_every_time() {
        let app = NewApp::new("my-app").unwrap();
        assert_eq!(app.accent(), NewApp::new("my-app").unwrap().accent());
        assert!(ACCENTS.contains(&app.accent()));
        let chosen: std::collections::BTreeSet<[u8; 3]> = (0..40)
            .map(|n| NewApp::new(&format!("app{n}")).unwrap().accent())
            .collect();
        assert_eq!(chosen.len(), ACCENTS.len(), "every colour is reachable");
        let [r, g, b] = app.accent();
        assert!(text_of(&app, "src/lib.rs").contains(&format!(
            "const ACCENT: Color = hex(0x{r:02X}{g:02X}{b:02X});"
        )));
    }

    #[test]
    fn every_colour_carries_white_text() {
        // WCAG relative luminance and contrast against white.
        let linear = |value: u8| {
            let v = f64::from(value) / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        for [r, g, b] in ACCENTS {
            let luminance = 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b);
            let contrast = 1.05 / (luminance + 0.05);
            assert!(contrast >= 4.5, "#{r:02X}{g:02X}{b:02X}: {contrast:.2}");
        }
    }

    #[test]
    fn the_icon_is_a_white_star_on_the_colour_inside_every_mask() {
        let accent = [0x0F, 0x76, 0x6E];
        let image = icon(accent);
        assert_eq!((image.width(), image.height()), (1024, 1024));
        assert!(image.is_opaque());
        let colour = [accent[0], accent[1], accent[2], 255];
        let white = [255, 255, 255, 255];
        assert_eq!(image.pixel(512, 512), white, "the middle");
        assert_eq!(image.pixel(512, 230), white, "near the top point");
        assert_eq!(image.pixel(790, 512), white, "near the right point");
        // Hollow sides: the diagonal leaves the star early.
        assert_eq!(image.pixel(700, 700), colour);
        for corner in [(0, 0), (1023, 0), (0, 1023), (1023, 1023)] {
            assert_eq!(image.pixel(corner.0, corner.1), colour);
        }
        // Nothing but the colour outside the middle 61%, which is what an
        // adaptive icon's mask is sure to keep.
        let keep = (1024.0 * 0.195) as u32;
        for at in 0..1024 {
            for edge in [keep - 1, 1024 - keep] {
                assert_eq!(image.pixel(at, edge), colour, "({at}, {edge})");
                assert_eq!(image.pixel(edge, at), colour, "({edge}, {at})");
            }
        }
        // And it is a PNG the icon tool reads back.
        let decoded = Image::from_png(&image.to_png()).unwrap();
        assert_eq!(decoded.pixel(512, 512), white);
    }

    const MANIFEST: &str = "\
[workspace]
resolver = \"3\"
members = [
    \"crates/morningstar\",
    \"tools/morningstar-devtools\",
]

[workspace.package]
version = \"0.0.1\"
";

    #[test]
    fn a_member_is_added_at_the_end_and_nothing_else_changes() {
        let with = add_workspace_member(MANIFEST, "apps/my-app").unwrap();
        assert_eq!(
            with,
            MANIFEST.replace(
                "    \"tools/morningstar-devtools\",\n",
                "    \"tools/morningstar-devtools\",\n    \"apps/my-app\",\n"
            )
        );
        // A last entry without its comma gets one.
        let bare = MANIFEST.replace("devtools\",\n]", "devtools\"\n]");
        assert_eq!(add_workspace_member(&bare, "apps/my-app").unwrap(), with);
    }

    #[test]
    fn a_list_on_one_line_is_written_out() {
        let empty = "[workspace]\nmembers = []\n\n[workspace.package]\n";
        assert_eq!(
            add_workspace_member(empty, "apps/a").unwrap(),
            "[workspace]\nmembers = [\n    \"apps/a\",\n]\n\n[workspace.package]\n"
        );
        let two = "[workspace]\nmembers = [\"x\", \"y\"]\n";
        assert_eq!(
            add_workspace_member(two, "apps/a").unwrap(),
            "[workspace]\nmembers = [\n    \"x\",\n    \"y\",\n    \"apps/a\",\n]\n"
        );
    }

    #[test]
    fn a_manifest_that_cannot_take_a_member_says_so() {
        for manifest in [
            "",
            "[package]\nname = \"x\"\n",
            "[workspace]\nresolver = \"3\"\n",
            "[workspace]\ndefault-members = [\"x\"]\n",
        ] {
            assert!(
                matches!(
                    add_workspace_member(manifest, "apps/a"),
                    Err(ScaffoldError::Workspace(_))
                ),
                "{manifest:?}"
            );
        }
        assert!(matches!(
            add_workspace_member(MANIFEST, "crates/morningstar"),
            Err(ScaffoldError::AlreadyMember(_))
        ));
        assert_eq!(
            member_packages(MANIFEST),
            ["morningstar", "morningstar-devtools"]
        );
    }
}
