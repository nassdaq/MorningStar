//! Morningstar's text subsystem.
//!
//! [`TextContext`] turns a string and a [`TextStyle`] into glyphs in a
//! display list. It owns the fonts, shapes text (ligatures, kerning, Arabic
//! joining), resolves bidirectional text, breaks lines, falls back across
//! fonts, rasterises glyphs and packs them into the atlas the renderer
//! samples.
//!
//! Shaping, line breaking and rasterisation are done by `cosmic-text`. None
//! of its types appear in this crate's API, so it can be replaced.
//!
//! Fonts are chosen by family name ([`FontFamily`]) and weight; a paragraph
//! can mix weights, colours and families span by span ([`TextSpan`]); and
//! letter spacing is part of the style.
//!
//! The same atlas holds one-colour vector icons ([`VectorIcon`]), a small
//! subset of SVG rasterised by `zeno` at the exact pixel size they are drawn
//! at. See [`TextContext::draw_icon`].
//!
//! Not supported yet: colour glyphs (emoji), mixed sizes in one paragraph,
//! and text selection.

mod atlas;
mod context;
mod icon;
mod style;

pub use context::TextContext;
pub use icon::{IconError, VectorIcon};
pub use style::{FontFamily, TextAlign, TextSpan, TextStyle, TextWeight};
