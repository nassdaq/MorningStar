//! Which app a command is about: the package of the directory it is run in.

use std::path::{Path, PathBuf};

/// The package whose `Cargo.toml` is `manifest_path`, or else the nearest
/// one in this directory or above it that has a `[package]` table.
pub fn current_package(manifest_path: Option<&Path>) -> Result<String, String> {
    let manifest = match manifest_path {
        Some(path) => path.to_owned(),
        None => nearest_package_manifest()?,
    };
    let text = std::fs::read_to_string(&manifest)
        .map_err(|error| format!("{}: {error}", manifest.display()))?;
    package_name(&text).ok_or_else(|| {
        format!(
            "{} has no [package] name: run this in an app's directory, or name the app",
            manifest.display()
        )
    })
}

/// The `Cargo.toml` of the nearest package in this directory or above it.
pub fn nearest_package_manifest() -> Result<PathBuf, String> {
    let here = std::env::current_dir().map_err(|error| format!("this directory: {error}"))?;
    here.ancestors()
        .map(|directory| directory.join("Cargo.toml"))
        .find(|manifest| {
            std::fs::read_to_string(manifest).is_ok_and(|text| package_name(&text).is_some())
        })
        .ok_or_else(|| {
            "no app here: no Cargo.toml with a [package] in this directory or above it. \
             Run this in an app's directory (morningstar new makes one)"
                .to_owned()
        })
}

/// `name = "..."` of the `[package]` table of a `Cargo.toml`.
///
/// Read line by line rather than parsed: Cargo writes the key this way in
/// every manifest it makes, and `morningstar new` does too. A manifest
/// written otherwise is still read by Cargo itself, which every command
/// asks for the rest.
pub fn package_name(manifest: &str) -> Option<String> {
    let mut in_package = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        let Some(rest) = line.strip_prefix("name") else {
            continue;
        };
        let Some(value) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        let value = value.trim();
        let quote = value.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        let inner = &value[1..];
        return inner.find(quote).map(|end| inner[..end].to_owned());
    }
    None
}

/// `arguments` for a tool that takes the package first: as they are when
/// they start with one, and with the current package put first when they
/// start with an option or are empty.
pub fn with_package(arguments: Vec<String>) -> Result<Vec<String>, String> {
    match arguments.first().map(String::as_str) {
        Some("-h" | "--help") => Ok(arguments),
        Some(first) if !first.starts_with('-') => Ok(arguments),
        _ => {
            let manifest = arguments
                .iter()
                .position(|argument| argument == "--manifest-path")
                .and_then(|at| arguments.get(at + 1))
                .map(PathBuf::from);
            let mut with = vec![current_package(manifest.as_deref())?];
            with.extend(arguments);
            Ok(with)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_package_name_is_read_from_its_table_only() {
        let manifest = "\
[workspace]
name = \"not-this\"

[package]
description = \"x\"
name = \"my-app\" # the app

[dependencies]
name = \"nor-this\"
";
        assert_eq!(package_name(manifest).as_deref(), Some("my-app"));
        assert_eq!(
            package_name("[package]\nname='single'\n").as_deref(),
            Some("single")
        );
        assert_eq!(package_name("[workspace]\nmembers = []\n"), None);
        assert_eq!(package_name("[package]\nnamespace = \"x\"\n"), None);
        assert_eq!(package_name("[package]\nname.workspace = true\n"), None);
    }

    #[test]
    fn a_tool_is_given_the_package_only_when_none_is_named() {
        let named = vec![
            "my-app".to_owned(),
            "--preview".to_owned(),
            "x.png".to_owned(),
        ];
        assert_eq!(with_package(named.clone()).unwrap(), named);
        let help = vec!["--help".to_owned()];
        assert_eq!(with_package(help.clone()).unwrap(), help);
    }
}
