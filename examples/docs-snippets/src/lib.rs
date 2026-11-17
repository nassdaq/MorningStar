//! The code samples of the documentation site (`docs/`, built into `site/`
//! by `tools/build-docs.py`).
//!
//! Each sample is a region of one of these files, between a line
//! `// docs:begin name` and a line `// docs:end name`. A page includes it
//! with `{{snippet examples/docs-snippets/src/file.rs#name}}`. Nothing here
//! is an application; the crate exists so that every sample is compiled and
//! linted with the rest of the workspace, and so that the ones with
//! behaviour are tested (`tests/`).

pub mod counter;
pub mod effects;
pub mod form;
pub mod layout;
pub mod live;
pub mod motion;
pub mod navigation;
pub mod pictures;
pub mod text;
pub mod theme;
pub mod widgets;
