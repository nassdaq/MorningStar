//! Helpers shared by the layout tests.
#![allow(dead_code)]

use morningstar_core::{Rect, Size};
use morningstar_layout::{LayoutTree, NodeId, Style};

pub fn r(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect::new(x, y, width, height)
}

/// Bounds at the origin with the given available size.
pub fn avail(width: f32, height: f32) -> Rect {
    Rect::new(0.0, 0.0, width, height)
}

/// Measure callback for trees that must not need one.
pub fn no_measure(key: u64, max_width: Option<f32>) -> Size {
    panic!("measure called unexpectedly: key {key}, max_width {max_width:?}");
}

/// A stand-in for the text subsystem. Every character is `CHAR` units wide,
/// a line is `line_height` tall, and lines break after any character.
#[derive(Default)]
pub struct Texts {
    /// (characters, line height) per key.
    specs: Vec<(usize, f32)>,
    /// Every call made, in order: (key, max_width).
    pub calls: Vec<(u64, Option<f32>)>,
}

pub const CHAR: f32 = 10.0;

impl Texts {
    /// Adds a measured leaf showing `chars` characters.
    pub fn leaf(
        &mut self,
        tree: &mut LayoutTree,
        style: Style,
        chars: usize,
        line_height: f32,
    ) -> NodeId {
        self.specs.push((chars, line_height));
        tree.measured(style, self.specs.len() as u64 - 1)
    }

    pub fn measure(&mut self, key: u64, max_width: Option<f32>) -> Size {
        self.calls.push((key, max_width));
        let (chars, line_height) = self.specs[key as usize];
        let per_line = match max_width {
            Some(width) => ((width / CHAR).floor() as usize).max(1),
            None => usize::MAX,
        };
        let lines = chars.div_ceil(per_line).max(1);
        Size::new(
            chars.min(per_line) as f32 * CHAR,
            lines as f32 * line_height,
        )
    }

    /// The constraints `key` was measured with, in order.
    pub fn calls_for(&self, key: u64) -> Vec<Option<f32>> {
        self.calls
            .iter()
            .filter(|(k, _)| *k == key)
            .map(|(_, max_width)| *max_width)
            .collect()
    }

    pub fn compute(&mut self, tree: &mut LayoutTree, root: NodeId, bounds: Rect) -> Rect {
        tree.compute(root, bounds, |key, max_width| self.measure(key, max_width))
    }
}
