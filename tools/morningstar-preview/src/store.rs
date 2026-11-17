//! What the preview keeps between runs of an app: the values the app
//! stores (ADR 0019), its installation id, and where its window was.
//!
//! All of it lives in one directory, `target/preview/<app>/state`, as
//! small text files. Because a reload starts the app again, an app that
//! stores its state comes back where it was, exactly as it does on a phone
//! after being closed.

use std::collections::BTreeMap;
use std::collections::hash_map::RandomState;
use std::fs;
use std::hash::{BuildHasher, Hasher};
use std::io;
use std::path::{Path, PathBuf};

use morningstar_runtime::HostState;

/// The stored values of one app, kept in a file.
#[derive(Debug)]
pub struct Store {
    dir: PathBuf,
    values: BTreeMap<String, String>,
}

impl Store {
    /// Opens the state directory, creating it, and reads what is stored.
    pub fn open(dir: &Path) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        let values = fs::read_to_string(dir.join("stored"))
            .map(|text| decode(&text))
            .unwrap_or_default();
        Ok(Self {
            dir: dir.to_owned(),
            values,
        })
    }

    /// A store that keeps nothing on disk, for a host run without a state
    /// directory.
    pub fn in_memory() -> Self {
        Self {
            dir: PathBuf::new(),
            values: BTreeMap::new(),
        }
    }

    /// The value under `key`.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    /// How many values are stored.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// True when nothing is stored.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Stores `value` under `key`, or forgets the key, and writes the file.
    pub fn set(&mut self, key: String, value: Option<String>) -> io::Result<()> {
        match value {
            Some(value) => {
                self.values.insert(key, value);
            }
            None => {
                self.values.remove(&key);
            }
        }
        self.save()
    }

    /// Forgets everything: the app's next start is its first launch. The
    /// installation id goes too, as it does when an app is reinstalled.
    pub fn clear(&mut self) -> io::Result<()> {
        self.values.clear();
        if self.dir.as_os_str().is_empty() {
            return Ok(());
        }
        for file in ["stored", "install-id"] {
            match fs::remove_file(self.dir.join(file)) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
                _ => {}
            }
        }
        Ok(())
    }

    fn save(&self) -> io::Result<()> {
        if self.dir.as_os_str().is_empty() {
            return Ok(());
        }
        // Written beside and renamed over, so that a reload that kills the
        // app mid-write never leaves half a file.
        let temporary = self.dir.join("stored.new");
        fs::write(&temporary, encode(&self.values))?;
        fs::rename(&temporary, self.dir.join("stored"))
    }

    /// The installation id: made once, then the same on every run, as on a
    /// phone. 32 hexadecimal digits.
    pub fn install_id(&self) -> String {
        if self.dir.as_os_str().is_empty() {
            return new_install_id();
        }
        let path = self.dir.join("install-id");
        if let Ok(id) = fs::read_to_string(&path) {
            let id = id.trim();
            if id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return id.to_ascii_lowercase();
            }
        }
        let id = new_install_id();
        // If it cannot be written the id is new each run; nothing breaks.
        let _ = fs::write(&path, &id);
        id
    }

    /// What the app is handed at setup: the id and every stored value.
    pub fn host_state(&self) -> HostState {
        let mut state = HostState::new().with_install_id(&self.install_id());
        for (key, value) in &self.values {
            state = state.with_stored(key.clone(), value.clone());
        }
        state
    }

    /// Where the window's top-left corner last was.
    pub fn window_position(&self) -> Option<(i32, i32)> {
        if self.dir.as_os_str().is_empty() {
            return None;
        }
        crate::options::parse_position(&fs::read_to_string(self.dir.join("window")).ok()?)
    }

    /// Remembers where the window is.
    pub fn set_window_position(&self, position: (i32, i32)) {
        if !self.dir.as_os_str().is_empty() {
            let _ = fs::write(
                self.dir.join("window"),
                format!("{},{}", position.0, position.1),
            );
        }
    }

    /// The directory, for files beside the store (a request's body on its
    /// way out, screenshots).
    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

/// 128 random bits as hexadecimal. The randomness is the standard
/// library's per-process hash seed, which comes from the system.
fn new_install_id() -> String {
    let word = |salt: u64| {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u64(salt);
        hasher.finish()
    };
    format!("{:016x}{:016x}", word(1), word(2))
}

/// One `key<TAB>value` per line, with backslash, tab, newline and carriage
/// return escaped in both.
fn encode(values: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    for (key, value) in values {
        escape_into(key, &mut out);
        out.push('\t');
        escape_into(value, &mut out);
        out.push('\n');
    }
    out
}

fn decode(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('\t')?;
            Some((unescape(key), unescape(value)))
        })
        .collect()
}

fn escape_into(text: &str, out: &mut String) {
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            ch => out.push(ch),
        }
    }
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ms-preview-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn values_survive_the_app_being_started_again() {
        let dir = scratch("store");
        let mut store = Store::open(&dir).unwrap();
        assert!(store.is_empty());
        store.set("name".into(), Some("Neema".into())).unwrap();
        store
            .set(
                "draft".into(),
                Some("two\nlines\tand a tab \\ and a backslash".into()),
            )
            .unwrap();
        store.set("gone".into(), Some("x".into())).unwrap();
        store.set("gone".into(), None).unwrap();

        let again = Store::open(&dir).unwrap();
        assert_eq!(again.len(), 2);
        assert_eq!(again.get("name"), Some("Neema"));
        assert_eq!(
            again.get("draft"),
            Some("two\nlines\tand a tab \\ and a backslash")
        );
        assert_eq!(again.get("gone"), None);

        let state = again.host_state();
        assert_eq!(state.stored("name"), Some("Neema"));
        assert_eq!(state.stored_len(), 2);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_installation_id_is_made_once() {
        let dir = scratch("id");
        let store = Store::open(&dir).unwrap();
        let id = store.install_id();
        assert_eq!(id.len(), 32);
        assert!(id.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_eq!(store.install_id(), id);
        assert_eq!(Store::open(&dir).unwrap().install_id(), id);
        assert_eq!(store.host_state().install_id(), id);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn clearing_is_a_first_launch() {
        let dir = scratch("clear");
        let mut store = Store::open(&dir).unwrap();
        store.set("name".into(), Some("Neema".into())).unwrap();
        let id = store.install_id();
        store.clear().unwrap();
        store.clear().unwrap();

        let again = Store::open(&dir).unwrap();
        assert!(again.is_empty());
        assert_ne!(again.install_id(), id, "a new installation");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_window_comes_back_where_it_was() {
        let dir = scratch("window");
        let store = Store::open(&dir).unwrap();
        assert_eq!(store.window_position(), None);
        store.set_window_position((1200, -40));
        assert_eq!(
            Store::open(&dir).unwrap().window_position(),
            Some((1200, -40))
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_store_without_a_directory_keeps_things_for_the_run_only() {
        let mut store = Store::in_memory();
        store.set("a".into(), Some("b".into())).unwrap();
        assert_eq!(store.get("a"), Some("b"));
        assert_eq!(store.window_position(), None);
        store.set_window_position((1, 2));
        store.clear().unwrap();
        assert!(store.is_empty());
    }

    #[test]
    fn a_damaged_file_loses_only_its_damaged_lines() {
        let decoded = decode("good\tvalue\nno tab here\n\nalso\tfine\\\n");
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded["good"], "value");
        assert_eq!(decoded["also"], "fine\\");
    }
}
