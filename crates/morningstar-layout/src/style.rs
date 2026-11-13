//! The per-node layout description: sizing, spacing and alignment.

use morningstar_core::Point;

/// Largest magnitude any input length is allowed to have. Inputs beyond it
/// are clamped so that sums of many lengths stay finite.
pub(crate) const MAX_LENGTH: f32 = 1.0e9;

/// Turns any `f32` into a usable non-negative length: NaN, negative values
/// and negative infinity become `0`, and large values (including positive
/// infinity) are capped at [`MAX_LENGTH`].
pub(crate) fn clean_length(value: f32) -> f32 {
    if value > 0.0 {
        value.min(MAX_LENGTH)
    } else {
        // Negative, zero or NaN (every comparison with NaN is false).
        0.0
    }
}

/// Like [`clean_length`] but keeps the sign. NaN becomes `0`.
fn clean_signed(value: f32) -> f32 {
    if value.is_nan() {
        0.0
    } else {
        value.clamp(-MAX_LENGTH, MAX_LENGTH)
    }
}

/// How a node decides its size along one axis.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Sizing {
    /// Exactly this many units, whatever the parent or the contents say.
    ///
    /// NaN and negative values are treated as `0`.
    Fixed(f32),
    /// As large as the contents need ("hug contents" in Figma, `wrap_content`
    /// on Android). This is the default.
    ///
    /// * A plain leaf has no contents and hugs to zero (or to its minimum
    ///   size).
    /// * Horizontally, a hugging node is never made wider than the space its
    ///   parent offers, so that text wraps instead of overflowing. Vertically
    ///   there is no such cap: a hugging node is as tall as its contents and
    ///   may overflow its parent.
    #[default]
    Hug,
    /// Take the space the parent has left ("fill container" in Figma,
    /// `Expanded`/`flex` in Flutter).
    ///
    /// * Along the main axis of a [`Row`](crate::LayoutTree::row) or
    ///   [`Column`](crate::LayoutTree::column), the space left after fixed
    ///   and hugging siblings and gaps is shared among the `Fill` children in
    ///   proportion to their weights.
    /// * Along the cross axis, and inside a
    ///   [`Stack`](crate::LayoutTree::stack), it means "as large as the
    ///   parent's content box"; the weight is ignored.
    /// * On the root it means "the available size passed to
    ///   [`compute`](crate::LayoutTree::compute)".
    ///
    /// A `Fill` child contributes only its minimum size to a parent that
    /// hugs: fill inside hug does not make the parent grow.
    ///
    /// The weight must be finite and positive. Any other weight is treated
    /// as `0`: the child gets only its minimum size.
    Fill(f32),
}

impl Sizing {
    /// `Fill` with weight 1, the common case.
    pub const FILL: Self = Self::Fill(1.0);

    fn sanitized(self) -> Self {
        match self {
            Self::Fixed(value) => Self::Fixed(clean_length(value)),
            Self::Hug => Self::Hug,
            Self::Fill(weight) => Self::Fill(if weight.is_finite() {
                clean_length(weight)
            } else {
                0.0
            }),
        }
    }
}

/// Where a row or column puts its children along its main axis when there is
/// space left over.
///
/// Left-over space exists only when no child is [`Sizing::Fill`] on the main
/// axis (or every such child has hit its maximum size).
///
/// When the children do not fit, `Start`, `Center` and `End` still apply
/// (the children overflow at the end, on both sides, or at the start
/// respectively) and the three `Space*` modes behave like `Start`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MainAlign {
    /// Pack children at the start (left of a row, top of a column).
    #[default]
    Start,
    /// Pack children in the middle.
    Center,
    /// Pack children at the end (right of a row, bottom of a column).
    End,
    /// First child at the start, last child at the end, equal space between.
    /// A single child sits at the start.
    SpaceBetween,
    /// Equal space around every child: half a share before the first and
    /// after the last.
    SpaceAround,
    /// Equal space between children and before the first and after the last.
    SpaceEvenly,
}

/// Where a row or column puts a child along its cross axis (vertically in a
/// row, horizontally in a column).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CrossAlign {
    /// Top of a row, left of a column.
    #[default]
    Start,
    /// Centred on the cross axis.
    Center,
    /// Bottom of a row, right of a column.
    End,
    /// Children whose cross-axis sizing is [`Sizing::Hug`] are made as large
    /// as the container's content box (subject to their min/max size).
    /// Children with a `Fixed` cross size keep it and sit at the start.
    Stretch,
}

/// A position along one axis of a [`Stack`](crate::LayoutTree::stack).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    /// Left or top.
    #[default]
    Start,
    /// Centred.
    Center,
    /// Right or bottom.
    End,
}

/// A two-dimensional position inside a [`Stack`](crate::LayoutTree::stack):
/// one of nine anchor points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Alignment {
    /// Horizontal position. `Start` is the left edge.
    pub x: Align,
    /// Vertical position. `Start` is the top edge.
    pub y: Align,
}

impl Alignment {
    /// Top-left corner. The default.
    pub const TOP_LEFT: Self = Self::new(Align::Start, Align::Start);
    /// Middle of the top edge.
    pub const TOP_CENTER: Self = Self::new(Align::Center, Align::Start);
    /// Top-right corner.
    pub const TOP_RIGHT: Self = Self::new(Align::End, Align::Start);
    /// Middle of the left edge.
    pub const CENTER_LEFT: Self = Self::new(Align::Start, Align::Center);
    /// Dead centre.
    pub const CENTER: Self = Self::new(Align::Center, Align::Center);
    /// Middle of the right edge.
    pub const CENTER_RIGHT: Self = Self::new(Align::End, Align::Center);
    /// Bottom-left corner.
    pub const BOTTOM_LEFT: Self = Self::new(Align::Start, Align::End);
    /// Middle of the bottom edge.
    pub const BOTTOM_CENTER: Self = Self::new(Align::Center, Align::End);
    /// Bottom-right corner.
    pub const BOTTOM_RIGHT: Self = Self::new(Align::End, Align::End);

    /// An alignment from its horizontal and vertical parts.
    pub const fn new(x: Align, y: Align) -> Self {
        Self { x, y }
    }
}

/// Four edge insets, used for padding.
///
/// NaN and negative values are treated as `0` (negative padding is not
/// supported).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Edges {
    /// Inset from the left edge.
    pub left: f32,
    /// Inset from the top edge.
    pub top: f32,
    /// Inset from the right edge.
    pub right: f32,
    /// Inset from the bottom edge.
    pub bottom: f32,
}

impl Edges {
    /// No inset on any edge.
    pub const ZERO: Self = Self::all(0.0);

    /// Each edge given separately, in the order left, top, right, bottom.
    pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    /// The same inset on all four edges.
    pub const fn all(value: f32) -> Self {
        Self::new(value, value, value, value)
    }

    /// `horizontal` on the left and right, `vertical` on the top and bottom.
    pub const fn symmetric(horizontal: f32, vertical: f32) -> Self {
        Self::new(horizontal, vertical, horizontal, vertical)
    }

    /// `value` on the left and right, nothing on the top and bottom.
    pub const fn horizontal(value: f32) -> Self {
        Self::symmetric(value, 0.0)
    }

    /// `value` on the top and bottom, nothing on the left and right.
    pub const fn vertical(value: f32) -> Self {
        Self::symmetric(0.0, value)
    }

    fn sanitized(self) -> Self {
        Self::new(
            clean_length(self.left),
            clean_length(self.top),
            clean_length(self.right),
            clean_length(self.bottom),
        )
    }
}

/// Everything the layout engine needs to know about one node.
///
/// All fields are public and the type is `Copy`; build one with struct
/// update syntax or with the chained helper methods:
///
/// ```
/// use morningstar_layout::{CrossAlign, Edges, Sizing, Style};
///
/// let card = Style::new()
///     .fill_width()
///     .padding(Edges::all(16.0))
///     .gap(8.0)
///     .cross_align(CrossAlign::Center);
/// assert_eq!(card.width, Sizing::Fill(1.0));
/// assert_eq!(card.height, Sizing::Hug);
/// ```
///
/// Some fields describe the node as a *container* (`padding`, `gap`,
/// `main_align`, `cross_align`, `content_align`) and are ignored on leaves.
/// Others describe it as a *child* (`align_self`, `place`, `offset`) and are
/// ignored where they do not apply.
///
/// Values are sanitised when the style is stored in a tree; see the
/// individual fields for what happens to NaN, negative and infinite input.
/// [`LayoutTree::style`](crate::LayoutTree::style) returns the sanitised
/// style.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
    /// Horizontal sizing. Default: [`Sizing::Hug`].
    pub width: Sizing,
    /// Vertical sizing. Default: [`Sizing::Hug`].
    pub height: Sizing,
    /// Lower bound on the width, applied to every sizing mode including
    /// `Fixed`. Default `0`. NaN and negative values are treated as `0`.
    pub min_width: f32,
    /// Upper bound on the width, applied to every sizing mode including
    /// `Fixed`. Default: infinity (no bound). NaN means no bound; negative
    /// values are treated as `0`. If it is below `min_width`, `min_width`
    /// wins.
    pub max_width: f32,
    /// Lower bound on the height. Same rules as `min_width`.
    pub min_height: f32,
    /// Upper bound on the height. Same rules as `max_width`.
    pub max_height: f32,
    /// Width divided by height, for example `16.0 / 9.0`. Default: none.
    ///
    /// Deliberately limited to the two cases that cost nothing:
    ///
    /// * `height` is [`Sizing::Hug`]: the height becomes `width / ratio`,
    ///   whatever decided the width (fixed, fill, stretch or contents). The
    ///   node's own contents no longer influence its height.
    /// * `width` is `Hug` and `height` is [`Sizing::Fixed`]: the width
    ///   becomes `height * ratio`.
    ///
    /// In every other combination the ratio is ignored. A dimension derived
    /// from the ratio is still clamped by min/max (which can break the
    /// ratio) and is not changed by [`CrossAlign::Stretch`]. Values that are
    /// not finite and positive are treated as none.
    pub aspect_ratio: Option<f32>,
    /// Space between the node's edges and its children. Containers only.
    pub padding: Edges,
    /// Space between adjacent children along the main axis of a row or
    /// column. Ignored by stacks and leaves. Default `0`.
    ///
    /// May be negative, which makes children overlap (overlapping avatars).
    /// NaN is treated as `0`.
    pub gap: f32,
    /// Rows only: children that do not fit on one line continue on the
    /// next, as CSS `flex-wrap: wrap` does. Default `false`. Ignored by
    /// columns, stacks and leaves. See "Wrapping rows" in the crate
    /// documentation for the exact rules.
    pub wrap: bool,
    /// Rows only: hugging children are offered unlimited width, so each
    /// takes its natural width however wide the row is, and what does not
    /// fit runs past the row's end. For the content of a viewport that
    /// scrolls sideways. Default `false`; ignored by a wrapping row.
    pub unbounded: bool,
    /// Space between the lines of a wrapping row (CSS `row-gap`). Default
    /// `0`; `gap` stays the space between neighbours on a line. May be
    /// negative; NaN is treated as `0`. Ignored unless `wrap` is set.
    pub line_gap: f32,
    /// Distribution of children along the main axis of a row or column. In
    /// a wrapping row it applies to each line separately.
    pub main_align: MainAlign,
    /// Placement of children along the cross axis of a row or column. A
    /// child can override it with `align_self`.
    pub cross_align: CrossAlign,
    /// Where a stack places its children. A child can override it with
    /// `place`. Ignored by rows, columns and leaves.
    pub content_align: Alignment,
    /// As a child of a row or column: overrides the parent's `cross_align`
    /// for this node.
    pub align_self: Option<CrossAlign>,
    /// As a child of a stack: overrides the parent's `content_align` for
    /// this node. This is how a badge is pinned to a corner.
    pub place: Option<Alignment>,
    /// Moves the node, and its whole subtree, by this amount after it has
    /// been placed. It does not affect the node's size, its siblings or its
    /// parent, and is not counted as overflow. Useful for a badge that
    /// overhangs the corner it is pinned to. NaN is treated as `0`.
    pub offset: Point,
}

impl Default for Style {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Style {
    /// Hug on both axes, no padding, no gap, everything aligned to the
    /// start.
    pub const DEFAULT: Self = Self {
        width: Sizing::Hug,
        height: Sizing::Hug,
        min_width: 0.0,
        max_width: f32::INFINITY,
        min_height: 0.0,
        max_height: f32::INFINITY,
        aspect_ratio: None,
        padding: Edges::ZERO,
        gap: 0.0,
        wrap: false,
        unbounded: false,
        line_gap: 0.0,
        main_align: MainAlign::Start,
        cross_align: CrossAlign::Start,
        content_align: Alignment::TOP_LEFT,
        align_self: None,
        place: None,
        offset: Point::new(0.0, 0.0),
    };

    /// The default style. Same as [`Style::DEFAULT`].
    pub const fn new() -> Self {
        Self::DEFAULT
    }

    /// A style with a fixed width and height.
    pub const fn fixed(width: f32, height: f32) -> Self {
        Self::DEFAULT.size(width, height)
    }

    /// Sets the horizontal sizing.
    pub const fn width(mut self, width: Sizing) -> Self {
        self.width = width;
        self
    }

    /// Sets the vertical sizing.
    pub const fn height(mut self, height: Sizing) -> Self {
        self.height = height;
        self
    }

    /// Sets a fixed width and height.
    pub const fn size(mut self, width: f32, height: f32) -> Self {
        self.width = Sizing::Fixed(width);
        self.height = Sizing::Fixed(height);
        self
    }

    /// Sets the width to `Sizing::Fixed(width)`.
    pub const fn fixed_width(self, width: f32) -> Self {
        self.width(Sizing::Fixed(width))
    }

    /// Sets the height to `Sizing::Fixed(height)`.
    pub const fn fixed_height(self, height: f32) -> Self {
        self.height(Sizing::Fixed(height))
    }

    /// Sets the width to `Sizing::Fill(1.0)`.
    pub const fn fill_width(self) -> Self {
        self.width(Sizing::FILL)
    }

    /// Sets the height to `Sizing::Fill(1.0)`.
    pub const fn fill_height(self) -> Self {
        self.height(Sizing::FILL)
    }

    /// Sets both axes to `Sizing::Fill(1.0)`.
    pub const fn fill(self) -> Self {
        self.fill_width().fill_height()
    }

    /// Sets the minimum width.
    pub const fn min_width(mut self, value: f32) -> Self {
        self.min_width = value;
        self
    }

    /// Sets the maximum width.
    pub const fn max_width(mut self, value: f32) -> Self {
        self.max_width = value;
        self
    }

    /// Sets the minimum height.
    pub const fn min_height(mut self, value: f32) -> Self {
        self.min_height = value;
        self
    }

    /// Sets the maximum height.
    pub const fn max_height(mut self, value: f32) -> Self {
        self.max_height = value;
        self
    }

    /// Sets the aspect ratio (width divided by height).
    pub const fn aspect_ratio(mut self, ratio: f32) -> Self {
        self.aspect_ratio = Some(ratio);
        self
    }

    /// Sets the padding.
    pub const fn padding(mut self, padding: Edges) -> Self {
        self.padding = padding;
        self
    }

    /// Sets the gap between children.
    pub const fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    /// Makes a row wrap its children onto further lines.
    pub const fn wrap(mut self) -> Self {
        self.wrap = true;
        self
    }

    /// Offers a row's hugging children unlimited width: see
    /// [`unbounded`](Self::unbounded).
    pub const fn unbounded(mut self) -> Self {
        self.unbounded = true;
        self
    }

    /// Sets the gap between the lines of a wrapping row.
    pub const fn line_gap(mut self, gap: f32) -> Self {
        self.line_gap = gap;
        self
    }

    /// Sets the main-axis distribution of children.
    pub const fn main_align(mut self, align: MainAlign) -> Self {
        self.main_align = align;
        self
    }

    /// Sets the cross-axis placement of children.
    pub const fn cross_align(mut self, align: CrossAlign) -> Self {
        self.cross_align = align;
        self
    }

    /// Sets where a stack places its children.
    pub const fn content_align(mut self, align: Alignment) -> Self {
        self.content_align = align;
        self
    }

    /// Overrides the parent row's or column's cross-axis placement for this
    /// node.
    pub const fn align_self(mut self, align: CrossAlign) -> Self {
        self.align_self = Some(align);
        self
    }

    /// Overrides the parent stack's placement for this node.
    pub const fn place(mut self, align: Alignment) -> Self {
        self.place = Some(align);
        self
    }

    /// Sets the post-layout offset.
    pub const fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset = Point::new(x, y);
        self
    }

    /// The style with every numeric field made safe to compute with. The
    /// layout pass relies on this having been applied.
    pub(crate) fn sanitized(self) -> Self {
        let min_width = clean_length(self.min_width);
        let min_height = clean_length(self.min_height);
        Self {
            width: self.width.sanitized(),
            height: self.height.sanitized(),
            min_width,
            max_width: clean_max(self.max_width).max(min_width),
            min_height,
            max_height: clean_max(self.max_height).max(min_height),
            aspect_ratio: self
                .aspect_ratio
                .filter(|ratio| ratio.is_finite() && *ratio > 0.0),
            padding: self.padding.sanitized(),
            gap: clean_signed(self.gap),
            line_gap: clean_signed(self.line_gap),
            offset: Point::new(clean_signed(self.offset.x), clean_signed(self.offset.y)),
            ..self
        }
    }

    pub(crate) fn sizing(&self, axis: usize) -> Sizing {
        if axis == 0 { self.width } else { self.height }
    }

    pub(crate) fn min(&self, axis: usize) -> f32 {
        if axis == 0 {
            self.min_width
        } else {
            self.min_height
        }
    }

    pub(crate) fn max(&self, axis: usize) -> f32 {
        if axis == 0 {
            self.max_width
        } else {
            self.max_height
        }
    }

    /// Applies min/max along `axis`. Min wins over max.
    pub(crate) fn clamp(&self, axis: usize, value: f32) -> f32 {
        value.min(self.max(axis)).max(self.min(axis))
    }

    pub(crate) fn pad_start(&self, axis: usize) -> f32 {
        if axis == 0 {
            self.padding.left
        } else {
            self.padding.top
        }
    }

    pub(crate) fn pad_sum(&self, axis: usize) -> f32 {
        if axis == 0 {
            self.padding.left + self.padding.right
        } else {
            self.padding.top + self.padding.bottom
        }
    }

    pub(crate) fn offset_on(&self, axis: usize) -> f32 {
        if axis == 0 {
            self.offset.x
        } else {
            self.offset.y
        }
    }

    /// The aspect ratio, if it is what decides this node's size along
    /// `axis` (see [`Style::aspect_ratio`]).
    pub(crate) fn ratio_for(&self, axis: usize) -> Option<f32> {
        let ratio = self.aspect_ratio?;
        let drives_width = self.width == Sizing::Hug && matches!(self.height, Sizing::Fixed(_));
        let drives_height = self.height == Sizing::Hug;
        let applies = if axis == 0 {
            drives_width
        } else {
            drives_height
        };
        applies.then_some(ratio)
    }
}

/// A maximum size: NaN and positive infinity mean "no bound", negative
/// values mean `0`.
fn clean_max(value: f32) -> f32 {
    if value.is_nan() {
        f32::INFINITY
    } else {
        value.max(0.0)
    }
}
