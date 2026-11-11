//! Morningstar's layout engine: turns a tree of boxes with sizing rules into
//! rectangles.
//!
//! The model is the one designs are drawn in (Figma auto-layout, SwiftUI
//! stacks, Flutter rows and columns), not CSS:
//!
//! * **Containers.** A [row](LayoutTree::row) places children left to right,
//!   a [column](LayoutTree::column) top to bottom, and a
//!   [stack](LayoutTree::stack) overlays them, each pinned to one of nine
//!   anchor points.
//! * **Leaves.** A [leaf](LayoutTree::leaf) is a box sized by its style
//!   alone. A [measured leaf](LayoutTree::measured) asks a callback how big
//!   its contents are at a given width; that is how text plugs in.
//! * **Sizing**, per axis ([`Sizing`]): `Fixed`, `Hug` the contents, or
//!   `Fill` the parent, with a weight when several siblings fill.
//! * **Spacing and alignment** ([`Style`]): padding per edge, a gap between
//!   children, [`MainAlign`] along the container's axis, [`CrossAlign`]
//!   across it, min/max sizes, and a limited aspect ratio.
//!
//! The output is a [`Rect`](morningstar_core::Rect) per node, in the
//! coordinate space of the bounds given to [`LayoutTree::compute`].
//!
//! This crate knows nothing about widgets, text or drawing, and depends only
//! on `morningstar-core`.
//!
//! # Rules worth knowing
//!
//! **Width flows down, height flows up.** A hugging node is never made
//! wider than the space its parent offers, so text wraps. Heights are then
//! derived from the final widths, and a hugging node is as tall as its
//! contents, however tall that is.
//!
//! **Overflow is allowed and reported, never fixed up.** Nothing is shrunk
//! below the size its style asks for and nothing is clipped. A fixed child
//! wider than its parent sticks out; [`LayoutTree::overflow`] says by how
//! much.
//!
//! **Fill inside hug collapses.** A `Fill` child adds only its minimum size
//! to the natural size of a hugging parent. `Hug` row containing a `Fill`
//! text gives a zero-width text; give the row a width.
//!
//! **In a row, hugging children are each offered everything that fixed
//! siblings and gaps leave.** One hugging text beside fixed icons wraps
//! correctly. Two long hugging texts in one row each take the lot and
//! overflow; make the one that should give way `Fill`.
//!
//! **Units are whatever the caller uses.** The engine works in plain `f32`
//! and never rounds. [`LayoutTree::rounded_rect`], [`round_rect`] and
//! [`snap_rect`] round edges on the way out.
//!
//! **Bad input never panics.** NaN, negative and infinite values each have
//! a documented meaning (see [`Style`] and [`LayoutTree::compute`]), and
//! every rectangle produced is finite.
//!
//! **There is no right-to-left mode.** `Start` is always left.
//!
//! # Cost
//!
//! [`compute`](LayoutTree::compute) makes four linear sweeps over the
//! subtree, without recursion (depth is not a hazard) and without
//! allocating once the tree has reached its working size. The measure
//! callback, which for text is the expensive part, is called at most twice
//! per measured leaf and usually once; the exact rule is in the
//! documentation of `compute`. No measurements have been taken on a device
//! yet.
//!
//! # Example
//!
//! A 360 x 640 screen: a header with a title and an avatar carrying a
//! status dot, a list of rows (icon, two lines of text, trailing badge) and
//! a button bar at the bottom.
//!
//! ```
//! use morningstar_core::{Rect, Size};
//! use morningstar_layout::{
//!     Alignment, CrossAlign, Edges, LayoutTree, MainAlign, NodeId, Sizing, Style,
//! };
//!
//! // Stand-in for the text subsystem: every character is 8 units wide and
//! // lines break anywhere. Each entry is (characters, line height).
//! let mut texts: Vec<(usize, f32)> = Vec::new();
//! let measure_text = |texts: &[(usize, f32)], key: u64, max_width: Option<f32>| {
//!     let (chars, line_height) = texts[key as usize];
//!     let per_line = match max_width {
//!         Some(width) => ((width / 8.0).floor() as usize).max(1),
//!         None => usize::MAX,
//!     };
//!     let lines = chars.div_ceil(per_line).max(1);
//!     Size::new(chars.min(per_line) as f32 * 8.0, lines as f32 * line_height)
//! };
//!
//! let mut tree = LayoutTree::new();
//! // A text leaf: registers the string and returns its node.
//! let mut text = |tree: &mut LayoutTree, style: Style, chars: usize, line_height: f32| {
//!     texts.push((chars, line_height));
//!     tree.measured(style, texts.len() as u64 - 1)
//! };
//!
//! // Header: title on the left, avatar on the right, centred vertically.
//! let title = text(&mut tree, Style::new(), 5, 24.0); // "Inbox"
//! let status_dot = tree.leaf(Style::fixed(10.0, 10.0).place(Alignment::BOTTOM_RIGHT));
//! let avatar = tree.stack(Style::fixed(32.0, 32.0), &[status_dot]);
//! let header = tree.row(
//!     Style::new()
//!         .fill_width()
//!         .fixed_height(56.0)
//!         .padding(Edges::horizontal(16.0))
//!         .main_align(MainAlign::SpaceBetween)
//!         .cross_align(CrossAlign::Center),
//!     &[title, avatar],
//! );
//!
//! // One list row: icon, name over preview, unread-count badge.
//! let mut list_row = |tree: &mut LayoutTree, name: usize, preview: usize, count: usize| {
//!     let icon = tree.leaf(Style::fixed(40.0, 40.0));
//!     // Fill-width text wraps at the width the row has left.
//!     let name = text(tree, Style::new().fill_width(), name, 20.0);
//!     let preview = text(tree, Style::new().fill_width(), preview, 16.0);
//!     let lines = tree.column(Style::new().fill_width().gap(2.0), &[name, preview]);
//!     let count = text(tree, Style::new(), count, 16.0);
//!     let badge = tree.stack(Style::new().padding(Edges::symmetric(6.0, 2.0)), &[count]);
//!     let row = tree.row(
//!         Style::new()
//!             .fill_width()
//!             .padding(Edges::symmetric(16.0, 12.0))
//!             .gap(12.0)
//!             .cross_align(CrossAlign::Center),
//!         &[icon, lines, badge],
//!     );
//!     (row, preview, badge)
//! };
//! let (row_a, preview_a, badge_a) = list_row(&mut tree, 12, 60, 1);
//! let (row_b, _, _) = list_row(&mut tree, 9, 20, 2);
//! let list = tree.column(Style::new().fill(), &[row_a, row_b]);
//!
//! // Bottom bar: a full-width button with a centred label.
//! let label = text(&mut tree, Style::new(), 7, 20.0); // "Compose"
//! let button = tree.stack(
//!     Style::new()
//!         .fill_width()
//!         .fixed_height(48.0)
//!         .content_align(Alignment::CENTER),
//!     &[label],
//! );
//! let bar = tree.column(Style::new().fill_width().padding(Edges::all(16.0)), &[button]);
//!
//! let screen: NodeId = tree.column(Style::new().fill(), &[header, list, bar]);
//!
//! let mut calls = 0;
//! tree.compute(screen, Rect::new(0.0, 0.0, 360.0, 640.0), |key, max_width| {
//!     calls += 1;
//!     measure_text(&texts, key, max_width)
//! });
//!
//! assert_eq!(tree.rect(header), Rect::new(0.0, 0.0, 360.0, 56.0));
//! assert_eq!(tree.rect(avatar), Rect::new(312.0, 12.0, 32.0, 32.0));
//! assert_eq!(tree.rect(status_dot), Rect::new(334.0, 34.0, 10.0, 10.0));
//! // The list takes what the header and the bar leave.
//! assert_eq!(tree.rect(list), Rect::new(0.0, 56.0, 360.0, 504.0));
//! // The 60-character preview wraps to two lines in the 244 units left
//! // between the icon and the badge, which makes its row taller.
//! assert_eq!(tree.rect(preview_a), Rect::new(68.0, 90.0, 244.0, 32.0));
//! assert_eq!(tree.rect(row_a), Rect::new(0.0, 56.0, 360.0, 78.0));
//! assert_eq!(tree.rect(badge_a), Rect::new(324.0, 85.0, 20.0, 20.0));
//! assert_eq!(tree.rect(row_b), Rect::new(0.0, 134.0, 360.0, 64.0));
//! assert_eq!(tree.rect(button), Rect::new(16.0, 576.0, 328.0, 48.0));
//! assert_eq!(tree.rect(label), Rect::new(152.0, 590.0, 56.0, 20.0));
//!
//! // Nothing overflows, and each of the eight texts was measured once.
//! assert_eq!(tree.overflowing().count(), 0);
//! assert_eq!(calls, 8);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod style;
mod tree;

pub use style::{Align, Alignment, CrossAlign, Edges, MainAlign, Sizing, Style};
pub use tree::{LayoutTree, MeasureKey, NodeId, round_rect, snap_rect};
