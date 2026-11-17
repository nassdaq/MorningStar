//! Noticing that a source changed, by looking.
//!
//! The watcher looks at two sets of files a few times a second and folds
//! every file's path, size and modification time into one number. When
//! the number changes, something was saved, created, renamed or deleted.
//! Which file does not matter: Cargo decides what to rebuild.
//!
//! - **What the last build read.** Cargo writes, beside each binary, the
//!   list of every file that went into it (`preview-<app>.d`): the sources
//!   of the app and of every path dependency, and whatever they include
//!   (`include_bytes!` of a font, a picture, a shader). This is exact, and
//!   it reaches outside the workspace.
//! - **Every `.rs` and `.toml` file of the workspace's packages**, by
//!   walking them. This catches what no build has read yet: a new module,
//!   a manifest, and everything before the first build has passed.
//!
//! Files that are neither (a server's log, a database, a design export
//! nothing includes) are not looked at, so they cannot start a build.
//!
//! Looking costs a `stat` per file (a few milliseconds for this
//! repository) and needs no dependency and no platform API. What it gives
//! up against the system's file events is a quarter of a second.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Directories that hold nothing a build reads, or far too much to walk:
/// build output, dependencies of other toolchains, version control.
const SKIPPED_DIRS: &[&str] = &["target", "node_modules", "build", "out"];

/// The walk stops counting here: a root that holds more files than this is
/// not a Rust package.
const MAX_FILES: usize = 50_000;

/// One number for the state of every watched file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fingerprint {
    hash: u64,
    files: usize,
}

impl Fingerprint {
    /// How many files were looked at.
    pub fn files(self) -> usize {
        self.files
    }
}

/// Fingerprints the Rust sources and manifests under `roots` and each of
/// `files`. A root or a file that cannot be read counts as absent: it
/// shows as a change when it appears.
pub fn scan(roots: &[PathBuf], files: &[PathBuf]) -> Fingerprint {
    let mut fingerprint = Fingerprint { hash: 0, files: 0 };
    for root in roots {
        walk(root, &mut fingerprint);
    }
    for file in files {
        if let Ok(metadata) = fs::metadata(file) {
            fingerprint.add(file, &metadata);
        }
    }
    fingerprint
}

impl Fingerprint {
    fn add(&mut self, path: &Path, metadata: &fs::Metadata) {
        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        metadata.len().hash(&mut hasher);
        metadata.modified().ok().hash(&mut hasher);
        // Summed, not chained: the order files are met in must not matter.
        self.hash = self.hash.wrapping_add(hasher.finish());
        self.files += 1;
    }
}

/// The files a build read, from the dependency information Cargo writes
/// beside a binary: one line, `binary: file file ...`, in which a space
/// inside a path is written `\ `.
pub fn parse_dep_info(text: &str) -> Vec<PathBuf> {
    let Some((_, list)) = text.split_once(": ") else {
        return Vec::new();
    };
    let mut files = Vec::new();
    let mut current = String::new();
    let mut chars = list.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' if chars
                .peek()
                .is_some_and(|next| *next == ' ' || *next == '\\') =>
            {
                current.extend(chars.next());
            }
            ' ' | '\n' | '\r' => {
                if !current.is_empty() {
                    files.push(PathBuf::from(std::mem::take(&mut current)));
                }
            }
            ch => current.push(ch),
        }
    }
    if !current.is_empty() {
        files.push(PathBuf::from(current));
    }
    files
}

fn walk(dir: &Path, fingerprint: &mut Fingerprint) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if fingerprint.files >= MAX_FILES {
            return;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // Hidden files are editors' and tools' own (`.git`, `.gradle`,
        // swap files), never a source.
        if name.starts_with('.') {
            continue;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() {
            if !SKIPPED_DIRS.contains(&name.as_ref()) {
                walk(&path, fingerprint);
            }
        } else if kind.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension == "rs" || extension == "toml")
        {
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            fingerprint.add(&path, &metadata);
        }
    }
}

/// Turns "something changed" into "build now", once per burst of saves.
///
/// An editor that saves all files, a formatter that runs on save, or a
/// branch switch changes many files over a fraction of a second. The build
/// starts when the files have been quiet for [`quiet`](Self::new), so that
/// it sees the whole change and runs once.
#[derive(Clone, Copy, Debug)]
pub struct Debounce {
    quiet: Duration,
    last_change: Option<Instant>,
}

impl Debounce {
    /// A debounce that waits for `quiet` without a change.
    pub fn new(quiet: Duration) -> Self {
        Self {
            quiet,
            last_change: None,
        }
    }

    /// Tells it what a look at the files found at `now`. Returns true when
    /// a build is due: there was a change, and none since for the quiet
    /// time. It then forgets the change.
    pub fn observe(&mut self, changed: bool, now: Instant) -> bool {
        if changed {
            self.last_change = Some(now);
            return false;
        }
        match self.last_change {
            Some(at) if now.duration_since(at) >= self.quiet => {
                self.last_change = None;
                true
            }
            _ => false,
        }
    }

    /// True between a change and the build it leads to.
    pub fn pending(&self) -> bool {
        self.last_change.is_some()
    }
}

/// The watcher: a fingerprint, and the debounce over its changes.
#[derive(Debug)]
pub struct Watcher {
    roots: Vec<PathBuf>,
    files: Vec<PathBuf>,
    last: Fingerprint,
    debounce: Debounce,
}

impl Watcher {
    /// Watches `roots` from their state now.
    pub fn new(roots: Vec<PathBuf>, quiet: Duration) -> Self {
        let last = scan(&roots, &[]);
        Self {
            roots,
            files: Vec::new(),
            last,
            debounce: Debounce::new(quiet),
        }
    }

    /// How many files are watched.
    pub fn files(&self) -> usize {
        self.last.files()
    }

    /// Replaces the list of files the last build read. A change made
    /// since the last look is not lost by this: it is noted first.
    pub fn set_files(&mut self, files: Vec<PathBuf>, now: Instant) {
        if scan(&self.roots, &self.files) != self.last {
            self.debounce.observe(true, now);
        }
        self.files = files;
        self.last = scan(&self.roots, &self.files);
    }

    /// Looks once. True when a build is due.
    pub fn poll(&mut self, now: Instant) -> bool {
        let current = scan(&self.roots, &self.files);
        let changed = current != self.last;
        self.last = current;
        self.debounce.observe(changed, now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ms-preview-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        dir
    }

    #[test]
    fn a_burst_of_saves_builds_once_after_it_ends() {
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        let mut debounce = Debounce::new(Duration::from_millis(200));

        assert!(!debounce.observe(false, at(0)), "nothing changed");
        assert!(!debounce.observe(true, at(100)));
        assert!(debounce.pending());
        assert!(!debounce.observe(false, at(200)), "only 100 ms of quiet");
        assert!(!debounce.observe(true, at(250)), "another save: start over");
        assert!(!debounce.observe(false, at(400)));
        assert!(
            debounce.observe(false, at(450)),
            "200 ms after the last save"
        );
        assert!(!debounce.pending());
        assert!(!debounce.observe(false, at(700)), "one build per burst");
    }

    #[test]
    fn saving_creating_and_deleting_a_file_each_show() {
        let dir = scratch("scan");
        let roots = vec![dir.clone()];
        fs::write(dir.join("src/lib.rs"), "fn a() {}").unwrap();
        let scan = |roots: &[PathBuf]| super::scan(roots, &[]);
        let first = scan(&roots);
        assert_eq!(first.files(), 1);
        assert_eq!(scan(&roots), first, "looking twice changes nothing");

        // A different length: the change shows whatever the clock's grain.
        fs::write(dir.join("src/lib.rs"), "fn a() { 1; }").unwrap();
        let saved = scan(&roots);
        assert_ne!(saved, first);

        fs::write(dir.join("src/new.rs"), "").unwrap();
        let created = scan(&roots);
        assert_ne!(created, saved);
        assert_eq!(created.files(), 2);

        fs::remove_file(dir.join("src/new.rs")).unwrap();
        assert_eq!(scan(&roots), saved, "and back");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn build_output_dependencies_and_hidden_files_are_not_watched() {
        let dir = scratch("skip");
        let roots = vec![dir.clone()];
        let scan = |roots: &[PathBuf]| super::scan(roots, &[]);
        fs::write(dir.join("src/lib.rs"), "").unwrap();
        let before = scan(&roots);

        for skipped in ["target/debug", "server/node_modules/pg", ".git", "build"] {
            fs::create_dir_all(dir.join(skipped)).unwrap();
            fs::write(dir.join(skipped).join("file.rs"), "x").unwrap();
        }
        for skipped in ["target/debug/a.rs", "build/b.rs", "src/.lib.rs.swp"] {
            fs::write(dir.join(skipped), "x").unwrap();
        }
        assert_eq!(scan(&roots), before);

        // Nor is what is neither Rust nor a manifest: a server's source,
        // its log, a database. A log that grows must not build for ever.
        for other in ["server/app.js", "server/server.log", "data.sqlite"] {
            fs::write(dir.join(other), "x").unwrap();
        }
        assert_eq!(scan(&roots), before);

        fs::write(dir.join("Cargo.toml"), "[package]").unwrap();
        assert_ne!(scan(&roots), before, "a manifest is");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_file_the_build_read_is_watched_wherever_and_whatever_it_is() {
        let dir = scratch("included");
        let font = dir.join("assets/font.ttf");
        fs::create_dir_all(font.parent().unwrap()).unwrap();
        fs::write(&font, "one").unwrap();
        let roots = vec![dir.join("src")];

        // Not a Rust file and not under a root: unseen by the walk.
        assert_eq!(super::scan(&roots, &[]).files(), 0);
        let files = vec![font.clone(), dir.join("gone.png")];
        let before = super::scan(&roots, &files);
        assert_eq!(before.files(), 1, "a listed file that is gone is skipped");
        fs::write(&font, "one two").unwrap();
        assert_ne!(super::scan(&roots, &files), before);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn cargos_dependency_information_is_read_with_its_escaped_spaces() {
        let text = "/w/target/debug/preview-app: /w/src/lib.rs /w/My\\ App/assets/a\\ b.png \
                    /w/crates/gpu/src/quad.wgsl\n";
        assert_eq!(
            parse_dep_info(text),
            vec![
                PathBuf::from("/w/src/lib.rs"),
                PathBuf::from("/w/My App/assets/a b.png"),
                PathBuf::from("/w/crates/gpu/src/quad.wgsl"),
            ]
        );
        assert!(parse_dep_info("").is_empty());
        assert!(parse_dep_info("no colon here").is_empty());
    }

    #[test]
    fn learning_what_the_build_read_does_not_lose_a_change_made_meanwhile() {
        let dir = scratch("meanwhile");
        fs::write(dir.join("src/lib.rs"), "one").unwrap();
        let picture = dir.join("picture.png");
        fs::write(&picture, "p").unwrap();
        let mut watcher = Watcher::new(vec![dir.join("src")], Duration::from_millis(100));
        let start = Instant::now();
        assert!(!watcher.poll(start));

        // Saved while the build was running, before its list arrives.
        fs::write(dir.join("src/lib.rs"), "one two").unwrap();
        watcher.set_files(vec![picture.clone()], start + Duration::from_millis(10));
        assert_eq!(watcher.files(), 2);
        assert!(!watcher.poll(start + Duration::from_millis(50)));
        assert!(
            watcher.poll(start + Duration::from_millis(150)),
            "the save still leads to a build"
        );

        // And the listed file is now watched.
        fs::write(&picture, "picture").unwrap();
        assert!(!watcher.poll(start + Duration::from_millis(200)));
        assert!(watcher.poll(start + Duration::from_millis(320)));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_missing_root_is_empty_and_shows_when_it_appears() {
        let dir = scratch("missing");
        let roots = vec![dir.join("not-yet")];
        let scan = |roots: &[PathBuf]| super::scan(roots, &[]);
        let before = scan(&roots);
        assert_eq!(before.files(), 0);
        fs::create_dir_all(dir.join("not-yet")).unwrap();
        fs::write(dir.join("not-yet/a.rs"), "").unwrap();
        assert_ne!(scan(&roots), before);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_watcher_asks_for_a_build_after_a_change_has_settled() {
        let dir = scratch("watcher");
        fs::write(dir.join("src/lib.rs"), "one").unwrap();
        let mut watcher = Watcher::new(vec![dir.clone()], Duration::from_millis(150));
        assert_eq!(watcher.files(), 1);
        let start = Instant::now();
        assert!(!watcher.poll(start));

        fs::write(dir.join("src/lib.rs"), "one two").unwrap();
        assert!(
            !watcher.poll(start + Duration::from_millis(10)),
            "just changed"
        );
        assert!(!watcher.poll(start + Duration::from_millis(100)));
        assert!(watcher.poll(start + Duration::from_millis(200)));
        assert!(!watcher.poll(start + Duration::from_millis(400)));
        fs::remove_dir_all(&dir).unwrap();
    }
}
