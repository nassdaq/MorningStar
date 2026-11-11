//! The node arena and the layout algorithm.
//!
//! The algorithm is four linear sweeps over a pre-order list of the nodes
//! under the root (see the crate documentation for the model they
//! implement):
//!
//! 1. bottom-up: natural (unconstrained) widths;
//! 2. top-down: final widths;
//! 3. bottom-up: natural heights, given the final widths;
//! 4. top-down: final heights and positions.
//!
//! Bottom-up sweeps walk the pre-order list backwards, which visits every
//! node after all of its descendants. Nothing recurses and nothing is
//! allocated.

use morningstar_core::{Rect, Size};

use crate::style::{
    Align, Alignment, CrossAlign, MAX_LENGTH, MainAlign, Sizing, Style, clean_length,
};

/// Absent link in the intrusive child lists.
const NONE: u32 = u32::MAX;

const X: usize = 0;
const Y: usize = 1;

/// An opaque value identifying a measured leaf to the measure callback.
///
/// The layout engine never interprets it. A widget layer would typically use
/// the index of the text widget in its own storage.
pub type MeasureKey = u64;

/// The boxed-free form of the measure callback used internally.
type MeasureFn<'a> = dyn FnMut(MeasureKey, Option<f32>) -> Size + 'a;

/// Handle to a node in a [`LayoutTree`].
///
/// Ids are indices into the tree that created them. They are invalidated by
/// [`LayoutTree::clear`]; using a stale or foreign id never panics, but it
/// refers to whatever node now has that index, or to nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(u32);

impl NodeId {
    /// The node's index: nodes are numbered from zero in creation order.
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Leaf,
    Measured,
    Row,
    Column,
    Stack,
}

impl Kind {
    fn is_container(self) -> bool {
        matches!(self, Self::Row | Self::Column | Self::Stack)
    }
}

/// What is known about a measured leaf during one `compute`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Measured {
    /// The callback has not been called.
    No,
    /// Called once for the natural size; the result may be reused for the
    /// final width if the two are compatible.
    Natural,
    /// Called with the final width; the stored height is the final answer.
    Final,
}

#[derive(Clone, Debug)]
struct Node {
    style: Style,
    kind: Kind,
    key: MeasureKey,

    parent: u32,
    first_child: u32,
    last_child: u32,
    next_sibling: u32,
    child_count: u32,

    // Results.
    pos: [f32; 2],
    size: [f32; 2],
    overflow: [f32; 2],

    // Scratch for the layout pass.
    natural: [f32; 2],
    share: f32,
    frozen: bool,
    measured: Measured,
    /// Width constraint of the last measure call (infinity for `None`).
    measured_query: f32,
    /// Result of the last measure call.
    measured_size: [f32; 2],
}

/// A tree of layout nodes and, after [`compute`](Self::compute), their
/// rectangles.
///
/// Nodes live in one `Vec` and are referred to by [`NodeId`]. The intended
/// use is to rebuild the tree every time the UI changes (every frame if
/// need be): [`clear`](Self::clear) empties it but keeps its allocation, so
/// a warmed-up tree allocates nothing.
///
/// See the [crate documentation](crate) for the layout model and a complete
/// example.
#[derive(Clone, Debug, Default)]
pub struct LayoutTree {
    nodes: Vec<Node>,
    /// Pre-order traversal of the subtree being laid out. Kept between
    /// calls only for its allocation.
    order: Vec<u32>,
}

impl LayoutTree {
    /// An empty tree. Allocates nothing until the first node is added.
    pub const fn new() -> Self {
        Self {
            nodes: Vec::new(),
            order: Vec::new(),
        }
    }

    /// An empty tree with room for `nodes` nodes.
    pub fn with_capacity(nodes: usize) -> Self {
        Self {
            nodes: Vec::with_capacity(nodes),
            order: Vec::with_capacity(nodes),
        }
    }

    /// Removes every node, keeping the allocations for reuse. All
    /// [`NodeId`]s handed out so far become invalid.
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.order.clear();
    }

    /// Number of nodes in the tree.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// True if the tree has no nodes.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Adds a leaf: a box with no children, sized entirely by its style.
    ///
    /// With `Sizing::Fixed` it is an icon or image placeholder; with
    /// `Sizing::Fill` it is a spacer; with `Sizing::Hug` it has no contents
    /// and collapses to its minimum size.
    pub fn leaf(&mut self, style: Style) -> NodeId {
        self.push(Kind::Leaf, style, 0)
    }

    /// Adds a measured leaf: a box whose contents are sized by the callback
    /// passed to [`compute`](Self::compute). This is how text plugs in.
    ///
    /// `key` is handed back to the callback so it can tell which leaf is
    /// being measured. Only axes whose sizing is [`Sizing::Hug`] take their
    /// size from the callback; a `Fixed` or `Fill` width is passed to the
    /// callback as the wrapping width instead.
    pub fn measured(&mut self, style: Style, key: MeasureKey) -> NodeId {
        self.push(Kind::Measured, style, key)
    }

    /// Adds a row: children are placed left to right. The main axis is
    /// horizontal, the cross axis vertical.
    ///
    /// `children` are attached in order, as if by
    /// [`add_child`](Self::add_child) (children that cannot be attached are
    /// skipped). More can be added later.
    pub fn row(&mut self, style: Style, children: &[NodeId]) -> NodeId {
        self.container(Kind::Row, style, children)
    }

    /// Adds a column: children are placed top to bottom. The main axis is
    /// vertical, the cross axis horizontal.
    ///
    /// `children` are attached as for [`row`](Self::row).
    pub fn column(&mut self, style: Style, children: &[NodeId]) -> NodeId {
        self.container(Kind::Column, style, children)
    }

    /// Adds a stack (a `ZStack`): every child is placed independently
    /// inside the stack's content box, overlapping its siblings. Later
    /// children are meant to be drawn on top of earlier ones.
    ///
    /// Each child is positioned by its own [`Style::place`], or by the
    /// stack's [`Style::content_align`] if it has none. A hugging stack is
    /// as large as its largest non-`Fill` child plus padding.
    ///
    /// `children` are attached as for [`row`](Self::row).
    pub fn stack(&mut self, style: Style, children: &[NodeId]) -> NodeId {
        self.container(Kind::Stack, style, children)
    }

    /// Appends `child` to `parent`'s children.
    ///
    /// Returns `false`, changing nothing, if that would not leave a tree:
    /// either id is invalid, `parent` is a leaf, `child` already has a
    /// parent, or `child` is `parent` itself or one of its ancestors.
    ///
    /// The ancestor check walks from `parent` to its root, so attaching is
    /// O(depth). Building bottom-up (children before parents, as the
    /// `children` argument of [`row`](Self::row) encourages) keeps that walk
    /// to a single step.
    pub fn add_child(&mut self, parent: NodeId, child: NodeId) -> bool {
        let (Some(parent_index), Some(child_index)) = (self.slot(parent), self.slot(child)) else {
            return false;
        };
        if !self.nodes[parent_index].kind.is_container() || self.nodes[child_index].parent != NONE {
            return false;
        }
        // `child` has no parent, so a cycle is possible only if it is the
        // root of the tree `parent` belongs to.
        let mut ancestor = parent.0;
        while ancestor != NONE {
            if ancestor == child.0 {
                return false;
            }
            ancestor = self.nodes[ancestor as usize].parent;
        }

        let last = self.nodes[parent_index].last_child;
        if last == NONE {
            self.nodes[parent_index].first_child = child.0;
        } else {
            self.nodes[last as usize].next_sibling = child.0;
        }
        self.nodes[parent_index].last_child = child.0;
        self.nodes[parent_index].child_count += 1;
        self.nodes[child_index].parent = parent.0;
        true
    }

    /// The node's style as stored, that is, after sanitising. `None` for an
    /// invalid id.
    pub fn style(&self, id: NodeId) -> Option<&Style> {
        self.slot(id).map(|index| &self.nodes[index].style)
    }

    /// Replaces the node's style. Returns `false` for an invalid id. Takes
    /// effect at the next [`compute`](Self::compute).
    pub fn set_style(&mut self, id: NodeId, style: Style) -> bool {
        match self.slot(id) {
            Some(index) => {
                self.nodes[index].style = style.sanitized();
                true
            }
            None => false,
        }
    }

    /// The node's parent, or `None` for a root or an invalid id.
    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        let parent = self.nodes.get(id.index())?.parent;
        (parent != NONE).then_some(NodeId(parent))
    }

    /// The node's children in order. Empty for a leaf or an invalid id.
    pub fn children(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        let mut next = self.nodes.get(id.index()).map_or(NONE, |n| n.first_child);
        std::iter::from_fn(move || {
            let current = self.nodes.get(next as usize)?;
            let id = NodeId(next);
            next = current.next_sibling;
            Some(id)
        })
    }

    /// Lays out `root` and everything below it.
    ///
    /// `bounds` gives the origin the root is placed at (`x`, `y`) and the
    /// available size (`width`, `height`). The available size matters to
    /// the root only through its sizing: a `Fill` root takes it, a `Hug`
    /// root is at most as wide as it, a `Fixed` root ignores it. Positive
    /// infinity means "unbounded" on that axis (a `Fill` root then hugs);
    /// NaN and negative sizes are treated as `0`, and a NaN or infinite
    /// origin as `0`.
    ///
    /// `measure` is called for measured leaves with the leaf's key and the
    /// width it has to fit: `Some(width)` to wrap at that width, `None` for
    /// the natural, unwrapped size. NaN and negative components of what it
    /// returns are treated as `0`. If the tree has no measured leaves it is
    /// never called; pass `|_, _| Size::ZERO`.
    ///
    /// # How often `measure` is called
    ///
    /// **At most twice per measured leaf per `compute`**, and never for a
    /// leaf whose size does not depend on it:
    ///
    /// * not at all if neither axis hugs, or if the width is `Fixed` or
    ///   `Fill` and the aspect ratio decides the height;
    /// * once, with `Some(width)`, if the width is `Fixed` or `Fill` (text
    ///   that fills its column: the cheap way to wrap);
    /// * once, with `None` (or `Some(max_width)` if the style has one), if
    ///   the width hugs and the natural width fits;
    /// * twice if the width hugs or is stretched and the natural width does
    ///   not fit: once for the natural size, once with the width that is
    ///   actually available.
    ///
    /// To make the second call unnecessary in the common case, the engine
    /// assumes that a leaf which reported a natural width `w` reports the
    /// same size for any constraint of at least `w`. Text behaves this way.
    ///
    /// # Result
    ///
    /// Returns the root's rectangle. Afterwards [`rect`](Self::rect),
    /// [`rounded_rect`](Self::rounded_rect) and
    /// [`overflow`](Self::overflow) answer for every node under the root.
    /// Nodes outside that subtree keep whatever they had, so several roots
    /// (a screen and an overlay, say) can live in one tree and be computed
    /// one after another.
    ///
    /// Does nothing and returns an empty rectangle if `root` is invalid.
    pub fn compute<F>(&mut self, root: NodeId, bounds: Rect, mut measure: F) -> Rect
    where
        F: FnMut(MeasureKey, Option<f32>) -> Size,
    {
        self.compute_dyn(root, bounds, &mut measure)
    }

    /// The node's rectangle from the last [`compute`](Self::compute) that
    /// covered it, in the coordinate space of the `bounds` given to that
    /// call (not relative to the node's parent).
    ///
    /// All four components are finite. Returns an empty rectangle at the
    /// origin for a node that has not been laid out or an invalid id.
    pub fn rect(&self, id: NodeId) -> Rect {
        self.nodes.get(id.index()).map_or(Rect::default(), |node| {
            Rect::new(node.pos[X], node.pos[Y], node.size[X], node.size[Y])
        })
    }

    /// [`rect`](Self::rect) with its edges rounded to whole units; see
    /// [`round_rect`].
    pub fn rounded_rect(&self, id: NodeId) -> Rect {
        round_rect(self.rect(id))
    }

    /// How far the node's children extend past its content box (the node
    /// minus its padding), per axis; zero when everything fits.
    ///
    /// Layout never clips or shrinks to prevent overflow: children keep the
    /// sizes their styles ask for and this reports the excess, so a caller
    /// can clip, scroll, or flag a layout bug.
    ///
    /// * Row or column, main axis: the children's total size plus gaps,
    ///   minus the content box's size.
    /// * Row or column, cross axis, and both axes of a stack: the largest
    ///   child's size minus the content box's size.
    ///
    /// [`Style::offset`] is not taken into account, and a leaf never
    /// overflows (a measured leaf's contents are the callback's business).
    /// Amounts within float rounding error of zero are reported as zero.
    pub fn overflow(&self, id: NodeId) -> Size {
        self.nodes.get(id.index()).map_or(Size::ZERO, |node| {
            Size::new(node.overflow[X], node.overflow[Y])
        })
    }

    /// Every node whose children overflow it, with the amount, in creation
    /// order. Scans the whole tree; meant for diagnostics and tests.
    pub fn overflowing(&self) -> impl Iterator<Item = (NodeId, Size)> + '_ {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.overflow[X] > 0.0 || node.overflow[Y] > 0.0)
            .map(|(index, node)| {
                (
                    NodeId(index as u32),
                    Size::new(node.overflow[X], node.overflow[Y]),
                )
            })
    }

    // ----------------------------------------------------------------
    // Construction internals
    // ----------------------------------------------------------------

    fn push(&mut self, kind: Kind, style: Style, key: MeasureKey) -> NodeId {
        // `NONE` is reserved, so the last usable index is `u32::MAX - 1`.
        // A tree that large cannot exist in memory; saturate rather than
        // wrap if it somehow does.
        let index = self.nodes.len().min(NONE as usize - 1) as u32;
        self.nodes.push(Node {
            style: style.sanitized(),
            kind,
            key,
            parent: NONE,
            first_child: NONE,
            last_child: NONE,
            next_sibling: NONE,
            child_count: 0,
            pos: [0.0; 2],
            size: [0.0; 2],
            overflow: [0.0; 2],
            natural: [0.0; 2],
            share: 0.0,
            frozen: false,
            measured: Measured::No,
            measured_query: 0.0,
            measured_size: [0.0; 2],
        });
        NodeId(index)
    }

    fn container(&mut self, kind: Kind, style: Style, children: &[NodeId]) -> NodeId {
        let id = self.push(kind, style, 0);
        for &child in children {
            self.add_child(id, child);
        }
        id
    }

    fn slot(&self, id: NodeId) -> Option<usize> {
        (id.index() < self.nodes.len()).then_some(id.index())
    }

    // ----------------------------------------------------------------
    // Layout
    // ----------------------------------------------------------------

    fn compute_dyn(&mut self, root: NodeId, bounds: Rect, measure: &mut MeasureFn<'_>) -> Rect {
        let Some(root_index) = self.slot(root) else {
            return Rect::default();
        };

        let mut order = std::mem::take(&mut self.order);
        self.collect_preorder(root.0, &mut order);

        let available = [
            clean_available(bounds.width),
            clean_available(bounds.height),
        ];
        // A `Fill` root with nothing to fill behaves as `Hug`.
        let root_hugs = [!available[X].is_finite(), !available[Y].is_finite()];

        // 1. Natural widths, bottom-up.
        for &index in order.iter().rev() {
            let fill_hugs = index == root.0 && root_hugs[X];
            self.natural_width(index as usize, fill_hugs, measure);
        }

        // 2. Final widths, top-down.
        let root_style = self.nodes[root_index].style;
        let root_width = match root_style.width {
            Sizing::Fill(_) if !root_hugs[X] => root_style.clamp(X, available[X]),
            Sizing::Hug if root_style.ratio_for(X).is_none() => {
                self.hug_width(root_index, available[X], measure)
            }
            _ => self.nodes[root_index].natural[X],
        };
        self.nodes[root_index].size[X] = root_width;
        for &index in &order {
            self.assign_child_widths(index as usize, measure);
        }

        // 3. Natural heights, bottom-up, now that every width is final.
        for &index in order.iter().rev() {
            let fill_hugs = index == root.0 && root_hugs[Y];
            self.natural_height(index as usize, fill_hugs, measure);
        }

        // 4. Final heights and positions, top-down.
        let root_node = &mut self.nodes[root_index];
        root_node.size[Y] = match root_style.height {
            Sizing::Fill(_) if !root_hugs[Y] => root_style.clamp(Y, available[Y]),
            _ => root_node.natural[Y],
        };
        root_node.pos = [
            clean_origin(bounds.x) + root_style.offset_on(X),
            clean_origin(bounds.y) + root_style.offset_on(Y),
        ];
        for &index in &order {
            self.place_children(index as usize);
        }

        self.order = order;
        self.rect(root)
    }

    /// Fills `order` with the pre-order traversal of the subtree at `root`,
    /// following the child and parent links instead of using a stack.
    fn collect_preorder(&self, root: u32, order: &mut Vec<u32>) {
        order.clear();
        let mut current = root;
        loop {
            order.push(current);
            let first_child = self.nodes[current as usize].first_child;
            if first_child != NONE {
                current = first_child;
                continue;
            }
            // Leaf: go to the next sibling of the nearest ancestor (or
            // self) that has one, without climbing past the root.
            loop {
                if current == root {
                    return;
                }
                let node = &self.nodes[current as usize];
                if node.next_sibling != NONE {
                    current = node.next_sibling;
                    break;
                }
                current = node.parent;
            }
        }
    }

    /// What `child` adds to a hugging parent along `axis`: its natural
    /// size, or only its minimum if it fills.
    fn contribution(&self, child: usize, axis: usize) -> f32 {
        let node = &self.nodes[child];
        match node.style.sizing(axis) {
            Sizing::Fill(_) => node.style.min(axis),
            _ => node.natural[axis],
        }
    }

    /// Natural size of a container's contents along `axis`, padding
    /// included: the sum of the children plus gaps if `axis` is the main
    /// axis, the largest child otherwise.
    fn content_size(&self, index: usize, axis: usize) -> f32 {
        let node = &self.nodes[index];
        let is_main = matches!((node.kind, axis), (Kind::Row, X) | (Kind::Column, Y));
        let mut total = 0.0_f32;
        let mut child = node.first_child;
        while child != NONE {
            let contribution = self.contribution(child as usize, axis);
            total = if is_main {
                total + contribution
            } else {
                total.max(contribution)
            };
            child = self.nodes[child as usize].next_sibling;
        }
        if is_main {
            total += node.style.gap * node.child_count.saturating_sub(1) as f32;
        }
        (total.max(0.0) + node.style.pad_sum(axis)).min(MAX_LENGTH)
    }

    /// Sweep 1. Also resets the per-compute scratch state of the node.
    fn natural_width(&mut self, index: usize, fill_hugs: bool, measure: &mut MeasureFn<'_>) {
        let node = &self.nodes[index];
        let style = node.style;
        let mut measured = Measured::No;
        let mut measured_query = 0.0;
        let mut measured_size = [0.0; 2];

        let natural = if let Sizing::Fixed(width) = style.width {
            width
        } else if let (Some(ratio), Sizing::Fixed(height)) = (style.ratio_for(X), style.height) {
            (style.clamp(Y, height) * ratio).min(MAX_LENGTH)
        } else {
            match node.kind {
                Kind::Leaf => 0.0,
                Kind::Measured => {
                    let hugs = style.width == Sizing::Hug || fill_hugs;
                    // With an aspect ratio deciding the height, a hugging
                    // width still comes from the contents.
                    if hugs {
                        let query = style.max_width;
                        let size = call_measure(measure, node.key, query);
                        measured = Measured::Natural;
                        measured_query = query;
                        measured_size = size;
                        size[X]
                    } else {
                        0.0
                    }
                }
                Kind::Row | Kind::Column | Kind::Stack => self.content_size(index, X),
            }
        };

        let node = &mut self.nodes[index];
        node.natural[X] = style.clamp(X, natural);
        node.measured = measured;
        node.measured_query = measured_query;
        node.measured_size = measured_size;
        node.overflow = [0.0; 2];
    }

    /// Final width of a hugging node offered `cap`: its natural width if
    /// that fits, otherwise `cap`. A measured leaf that does not fit is
    /// re-measured at `cap` so that it hugs its wrapped contents.
    fn hug_width(&mut self, index: usize, cap: f32, measure: &mut MeasureFn<'_>) -> f32 {
        let node = &mut self.nodes[index];
        let natural = node.natural[X];
        if natural <= cap {
            return natural;
        }
        let style = node.style;
        let offered = style.clamp(X, cap);
        if node.kind == Kind::Measured && node.measured == Measured::Natural {
            let size = call_measure(measure, node.key, offered);
            node.measured = Measured::Final;
            node.measured_query = offered;
            node.measured_size = size;
            // A leaf that cannot get as narrow as asked (one long word)
            // keeps the width it reports and overflows.
            return style.clamp(X, size[X]);
        }
        offered
    }

    /// True if `child`, hugging along `axis`, should instead be stretched
    /// to its row or column parent's content box.
    fn stretches(parent: &Style, child: &Style, axis: usize) -> bool {
        child.align_self.unwrap_or(parent.cross_align) == CrossAlign::Stretch
            && child.ratio_for(axis).is_none()
    }

    /// Sweep 2: given this node's final width, decide its children's.
    fn assign_child_widths(&mut self, index: usize, measure: &mut MeasureFn<'_>) {
        let node = &self.nodes[index];
        let (kind, style) = (node.kind, node.style);
        let first_child = node.first_child;
        let inner = (node.size[X] - style.pad_sum(X)).max(0.0);

        match kind {
            Kind::Leaf | Kind::Measured => {}
            Kind::Column | Kind::Stack => {
                let mut child = first_child;
                while child != NONE {
                    let child_index = child as usize;
                    let child_style = self.nodes[child_index].style;
                    let width = match child_style.width {
                        Sizing::Fixed(_) => self.nodes[child_index].natural[X],
                        Sizing::Fill(_) => child_style.clamp(X, inner),
                        Sizing::Hug if child_style.ratio_for(X).is_some() => {
                            self.nodes[child_index].natural[X]
                        }
                        Sizing::Hug
                            if kind == Kind::Column && Self::stretches(&style, &child_style, X) =>
                        {
                            child_style.clamp(X, inner)
                        }
                        Sizing::Hug => self.hug_width(child_index, inner, measure),
                    };
                    self.nodes[child_index].size[X] = width;
                    child = self.nodes[child_index].next_sibling;
                }
            }
            Kind::Row => {
                let gaps = style.gap * node.child_count.saturating_sub(1) as f32;

                // Children whose width does not depend on the space on
                // offer come off the top.
                let mut used = 0.0_f32;
                let mut child = first_child;
                while child != NONE {
                    let child_node = &mut self.nodes[child as usize];
                    if is_rigid(&child_node.style, X) {
                        child_node.size[X] = child_node.natural[X];
                        used += child_node.size[X];
                    }
                    child = child_node.next_sibling;
                }

                // Every hugging child may be as wide as what is left, on
                // its own. Two long hugging texts in one row therefore
                // overflow rather than share; make one of them `Fill`.
                let cap = (inner - gaps - used).max(0.0);
                let mut child = first_child;
                while child != NONE {
                    let child_index = child as usize;
                    let child_style = self.nodes[child_index].style;
                    if child_style.width == Sizing::Hug && !is_rigid(&child_style, X) {
                        let width = self.hug_width(child_index, cap, measure);
                        self.nodes[child_index].size[X] = width;
                        used += width;
                    }
                    child = self.nodes[child_index].next_sibling;
                }

                self.distribute_fill(index, X, inner - gaps - used);
            }
        }
    }

    /// Shares `space` among the children of `index` that fill along `axis`
    /// (the container's main axis), in proportion to their weights and
    /// respecting their min/max sizes.
    ///
    /// When a child's share violates its min or max it is frozen at the
    /// bound and the rest is shared again among the others, as in CSS
    /// flexbox. Each round freezes at least one child, so this is at worst
    /// quadratic in the number of fill children with *active* bounds;
    /// without such bounds it is a single round.
    fn distribute_fill(&mut self, index: usize, axis: usize, space: f32) {
        let first_child = self.nodes[index].first_child;

        let mut child = first_child;
        while child != NONE {
            let node = &mut self.nodes[child as usize];
            node.frozen = false;
            child = node.next_sibling;
        }

        loop {
            let mut total_weight = 0.0_f32;
            let mut frozen_size = 0.0_f32;
            let mut any_flexible = false;
            let mut child = first_child;
            while child != NONE {
                let node = &self.nodes[child as usize];
                if let Sizing::Fill(weight) = node.style.sizing(axis) {
                    if node.frozen {
                        frozen_size += node.size[axis];
                    } else {
                        total_weight += weight;
                        any_flexible = true;
                    }
                }
                child = node.next_sibling;
            }
            if !any_flexible {
                return;
            }

            let remaining = (space - frozen_size).max(0.0);
            // Positive if clamping made the children larger in total than
            // their shares (minimums dominate), negative if smaller.
            let mut violation = 0.0_f32;
            let mut child = first_child;
            while child != NONE {
                let node = &mut self.nodes[child as usize];
                if let (Sizing::Fill(weight), false) = (node.style.sizing(axis), node.frozen) {
                    let share = if total_weight > 0.0 {
                        remaining * (weight / total_weight)
                    } else {
                        0.0
                    };
                    let size = node.style.clamp(axis, share);
                    node.share = share;
                    node.size[axis] = size;
                    violation += size - share;
                }
                child = node.next_sibling;
            }
            if violation == 0.0 {
                return;
            }

            let mut child = first_child;
            while child != NONE {
                let node = &mut self.nodes[child as usize];
                if matches!(node.style.sizing(axis), Sizing::Fill(_)) && !node.frozen {
                    let size = node.size[axis];
                    node.frozen = if violation > 0.0 {
                        size > node.share
                    } else {
                        size < node.share
                    };
                }
                child = node.next_sibling;
            }
        }
    }

    /// Sweep 3.
    fn natural_height(&mut self, index: usize, fill_hugs: bool, measure: &mut MeasureFn<'_>) {
        let node = &self.nodes[index];
        let style = node.style;
        let width = node.size[X];

        let natural = if let Sizing::Fixed(height) = style.height {
            height
        } else if let Some(ratio) = style.ratio_for(Y) {
            (width / ratio).min(MAX_LENGTH)
        } else {
            match node.kind {
                Kind::Leaf => 0.0,
                Kind::Measured => {
                    if style.height == Sizing::Hug || fill_hugs {
                        self.measured_height(index, measure)
                    } else {
                        0.0
                    }
                }
                Kind::Row | Kind::Column | Kind::Stack => self.content_size(index, Y),
            }
        };
        self.nodes[index].natural[Y] = style.clamp(Y, natural);
    }

    /// Height of a measured leaf at its final width, calling the callback
    /// only if no earlier result applies.
    fn measured_height(&mut self, index: usize, measure: &mut MeasureFn<'_>) -> f32 {
        let node = &mut self.nodes[index];
        let width = node.size[X];
        let reusable = match node.measured {
            Measured::Final => true,
            // The natural measurement stands if the leaf got at least the
            // width it asked for and no more than it was told it could have.
            Measured::Natural => node.measured_size[X] <= width && width <= node.measured_query,
            Measured::No => false,
        };
        if !reusable {
            node.measured_size = call_measure(measure, node.key, width);
            node.measured_query = width;
            node.measured = Measured::Final;
        }
        node.measured_size[Y]
    }

    /// Sweep 4: given this node's final rectangle, decide its children's
    /// heights and positions, and record overflow.
    fn place_children(&mut self, index: usize) {
        let node = &self.nodes[index];
        let (kind, style) = (node.kind, node.style);
        let first_child = node.first_child;
        let child_count = node.child_count;
        let inner = [
            (node.size[X] - style.pad_sum(X)).max(0.0),
            (node.size[Y] - style.pad_sum(Y)).max(0.0),
        ];
        let origin = [
            node.pos[X] + style.pad_start(X),
            node.pos[Y] + style.pad_start(Y),
        ];
        if first_child == NONE {
            return;
        }

        // Heights. Widths were settled in sweep 2.
        match kind {
            Kind::Leaf | Kind::Measured => return,
            Kind::Row | Kind::Stack => {
                let mut child = first_child;
                while child != NONE {
                    let child_node = &mut self.nodes[child as usize];
                    let child_style = child_node.style;
                    child_node.size[Y] = match child_style.height {
                        Sizing::Fill(_) => child_style.clamp(Y, inner[Y]),
                        Sizing::Hug
                            if kind == Kind::Row && Self::stretches(&style, &child_style, Y) =>
                        {
                            child_style.clamp(Y, inner[Y])
                        }
                        _ => child_node.natural[Y],
                    };
                    child = child_node.next_sibling;
                }
            }
            Kind::Column => {
                let gaps = style.gap * child_count.saturating_sub(1) as f32;
                let mut used = 0.0_f32;
                let mut child = first_child;
                while child != NONE {
                    let child_node = &mut self.nodes[child as usize];
                    if !matches!(child_node.style.height, Sizing::Fill(_)) {
                        child_node.size[Y] = child_node.natural[Y];
                        used += child_node.size[Y];
                    }
                    child = child_node.next_sibling;
                }
                self.distribute_fill(index, Y, inner[Y] - gaps - used);
            }
        }

        let overflow = match kind {
            Kind::Stack => self.place_in_stack(first_child, &style, origin, inner),
            Kind::Row => self.place_along(first_child, child_count, &style, X, origin, inner),
            _ => self.place_along(first_child, child_count, &style, Y, origin, inner),
        };
        self.nodes[index].overflow = [
            significant(overflow[X], inner[X]),
            significant(overflow[Y], inner[Y]),
        ];
    }

    /// Positions the children of a stack. Returns the raw overflow.
    fn place_in_stack(
        &mut self,
        first_child: u32,
        style: &Style,
        origin: [f32; 2],
        inner: [f32; 2],
    ) -> [f32; 2] {
        let mut overflow = [0.0_f32; 2];
        let mut child = first_child;
        while child != NONE {
            let node = &mut self.nodes[child as usize];
            let Alignment { x, y } = node.style.place.unwrap_or(style.content_align);
            for (axis, align) in [(X, x), (Y, y)] {
                let free = inner[axis] - node.size[axis];
                let shift = match align {
                    Align::Start => 0.0,
                    Align::Center => free / 2.0,
                    Align::End => free,
                };
                node.pos[axis] = origin[axis] + shift + node.style.offset_on(axis);
                overflow[axis] = overflow[axis].max(-free);
            }
            child = node.next_sibling;
        }
        overflow
    }

    /// Positions the children of a row (`main == X`) or column
    /// (`main == Y`). Returns the raw overflow.
    fn place_along(
        &mut self,
        first_child: u32,
        child_count: u32,
        style: &Style,
        main: usize,
        origin: [f32; 2],
        inner: [f32; 2],
    ) -> [f32; 2] {
        let cross = 1 - main;

        let mut used = style.gap * child_count.saturating_sub(1) as f32;
        let mut child = first_child;
        while child != NONE {
            let node = &self.nodes[child as usize];
            used += node.size[main];
            child = node.next_sibling;
        }

        let count = child_count as f32;
        let free = inner[main] - used;
        let spare = free.max(0.0);
        // (space before the first child, extra space between neighbours)
        let (lead, between) = match style.main_align {
            MainAlign::Start => (0.0, 0.0),
            MainAlign::Center => (free / 2.0, 0.0),
            MainAlign::End => (free, 0.0),
            MainAlign::SpaceBetween if child_count > 1 => (0.0, spare / (count - 1.0)),
            MainAlign::SpaceBetween => (0.0, 0.0),
            MainAlign::SpaceAround => (spare / count / 2.0, spare / count),
            MainAlign::SpaceEvenly => (spare / (count + 1.0), spare / (count + 1.0)),
        };

        let mut overflow = [0.0_f32; 2];
        overflow[main] = -free;

        // The cursor is kept in absolute coordinates and advanced by
        // exactly each child's size, so that with no gap one child's far
        // edge and the next child's near edge are the same float and round
        // to the same whole unit.
        let mut cursor = origin[main] + lead;
        let mut child = first_child;
        while child != NONE {
            let node = &mut self.nodes[child as usize];
            let cross_free = inner[cross] - node.size[cross];
            let shift = match node.style.align_self.unwrap_or(style.cross_align) {
                CrossAlign::Start | CrossAlign::Stretch => 0.0,
                CrossAlign::Center => cross_free / 2.0,
                CrossAlign::End => cross_free,
            };
            node.pos[main] = cursor + node.style.offset_on(main);
            node.pos[cross] = origin[cross] + shift + node.style.offset_on(cross);
            overflow[cross] = overflow[cross].max(-cross_free);
            cursor += node.size[main];
            cursor += style.gap + between;
            child = node.next_sibling;
        }
        overflow
    }
}

/// True if the node's size along `axis` is known without reference to the
/// space its parent has: fixed, or derived from a fixed size by the aspect
/// ratio.
fn is_rigid(style: &Style, axis: usize) -> bool {
    match style.sizing(axis) {
        Sizing::Fixed(_) => true,
        Sizing::Hug => axis == X && style.ratio_for(X).is_some(),
        Sizing::Fill(_) => false,
    }
}

/// Calls the measure callback with `max_width` (infinity meaning
/// unconstrained) and sanitises what comes back.
fn call_measure(measure: &mut MeasureFn<'_>, key: MeasureKey, max_width: f32) -> [f32; 2] {
    let size = measure(key, max_width.is_finite().then_some(max_width));
    [clean_length(size.width), clean_length(size.height)]
}

/// An available size: NaN and negatives become `0`, positive infinity is
/// kept as "unbounded".
fn clean_available(value: f32) -> f32 {
    if value == f32::INFINITY {
        value
    } else {
        clean_length(value)
    }
}

/// An origin coordinate: NaN and infinities become `0`.
fn clean_origin(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(-MAX_LENGTH, MAX_LENGTH)
    } else {
        0.0
    }
}

/// An overflow amount, or zero if it is within float error of fitting
/// exactly (shares of a fill distribution rarely sum to precisely the
/// space they divide).
fn significant(amount: f32, extent: f32) -> f32 {
    let tolerance = 1.0e-3_f32.max(extent * 1.0e-5);
    if amount > tolerance { amount } else { 0.0 }
}

/// Rounds a rectangle's four *edges* to the nearest whole unit.
///
/// Rounding edges rather than position and size is what keeps layouts
/// seamless: two rectangles that share an edge before rounding share it
/// afterwards, so no one-unit gaps or overlaps appear between siblings, and
/// errors do not accumulate along a row. The price is that equal sizes can
/// round to sizes that differ by one (three thirds of 100 become 33, 34,
/// 33).
///
/// Halves round away from zero. Use [`snap_rect`] when layout units are not
/// device pixels.
pub fn round_rect(rect: Rect) -> Rect {
    snap_rect(rect, 1.0)
}

/// Rounds a rectangle's edges to the device pixel grid, for layouts done in
/// density-independent units.
///
/// `scale` is device pixels per layout unit. The result is still in layout
/// units, with each edge at a whole number of device pixels. A `scale` that
/// is not finite and positive is treated as `1`. Like [`round_rect`], this
/// rounds edges, so shared edges stay shared.
///
/// The rectangle is expected to be finite, as everything a [`LayoutTree`]
/// returns is; NaN in gives NaN out.
pub fn snap_rect(rect: Rect, scale: f32) -> Rect {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let snap = |value: f32| (value * scale).round() / scale;
    let left = snap(rect.x);
    let top = snap(rect.y);
    let right = snap(rect.x + rect.width);
    let bottom = snap(rect.y + rect.height);
    Rect::new(left, top, right - left, bottom - top)
}
