//! The description of a screen: [`Element`] and the widgets that build one.

use std::borrow::Cow;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use morningstar_core::{
    Capitalization, Color, Image, ImageFit, ImeAction, KeyboardType, Shadow, TextInputTraits,
    Transform,
};
use morningstar_layout::{Alignment, CrossAlign, Edges, MainAlign, Sizing, Style};
use morningstar_runtime::{
    FontFamily, SvgPicture, TextAlign, TextWeight, VectorIcon, VectorPicture,
};

use crate::color::{ColorRef, ThemeColor};
use crate::edit::{self, EditConfig};
use crate::motion::{Animation, Easing, Motion, Transition};
use crate::theme::TextRole;

/// Hashes an explicit widget id. Deterministic across runs and platforms.
pub(crate) fn key_hash(id: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    id.hash(&mut hasher);
    // Keep explicit ids out of the way of the root path constant.
    hasher.finish() | 1
}

/// Sizing, placement, transform and identity methods shared by every
/// widget. Expects the type to have `layout: Style`, `key: Option<u64>` and
/// `transform: Option<Transform>` fields.
macro_rules! layout_methods {
    () => {
        /// Fixed width in dp.
        pub fn width(mut self, dp: f32) -> Self {
            self.layout.width = Sizing::Fixed(dp);
            self
        }

        /// Fixed height in dp.
        pub fn height(mut self, dp: f32) -> Self {
            self.layout.height = Sizing::Fixed(dp);
            self
        }

        /// Take the width the parent has left ("fill container").
        pub fn fill_width(mut self) -> Self {
            self.layout.width = Sizing::FILL;
            self
        }

        /// Take the height the parent has left.
        pub fn fill_height(mut self) -> Self {
            self.layout.height = Sizing::FILL;
            self
        }

        /// Fill both axes.
        pub fn fill(self) -> Self {
            self.fill_width().fill_height()
        }

        /// Fill the width with a weight: siblings that fill share the space
        /// left in proportion to their weights (`flex` in Flutter).
        pub fn fill_width_weight(mut self, weight: f32) -> Self {
            self.layout.width = Sizing::Fill(weight);
            self
        }

        /// Fill the height with a weight.
        pub fn fill_height_weight(mut self, weight: f32) -> Self {
            self.layout.height = Sizing::Fill(weight);
            self
        }

        /// As wide as the contents ("hug contents"). The default for most
        /// widgets.
        pub fn hug_width(mut self) -> Self {
            self.layout.width = Sizing::Hug;
            self
        }

        /// As tall as the contents.
        pub fn hug_height(mut self) -> Self {
            self.layout.height = Sizing::Hug;
            self
        }

        /// Sets the horizontal sizing from the layout engine's own type.
        pub fn width_sizing(mut self, sizing: Sizing) -> Self {
            self.layout.width = sizing;
            self
        }

        /// Sets the vertical sizing from the layout engine's own type.
        pub fn height_sizing(mut self, sizing: Sizing) -> Self {
            self.layout.height = sizing;
            self
        }

        /// Lower bound on the width in dp, whatever the sizing.
        pub fn min_width(mut self, dp: f32) -> Self {
            self.layout.min_width = dp;
            self
        }

        /// Upper bound on the width in dp, whatever the sizing.
        pub fn max_width(mut self, dp: f32) -> Self {
            self.layout.max_width = dp;
            self
        }

        /// Lower bound on the height in dp.
        pub fn min_height(mut self, dp: f32) -> Self {
            self.layout.min_height = dp;
            self
        }

        /// Upper bound on the height in dp.
        pub fn max_height(mut self, dp: f32) -> Self {
            self.layout.max_height = dp;
            self
        }

        /// Width divided by height. Applies when the height hugs (height
        /// follows width) or when the width hugs and the height is fixed.
        pub fn aspect_ratio(mut self, ratio: f32) -> Self {
            self.layout.aspect_ratio = Some(ratio);
            self
        }

        /// Inside a row or column: overrides the parent's cross-axis
        /// alignment for this widget.
        pub fn align_self(mut self, align: CrossAlign) -> Self {
            self.layout.align_self = Some(align);
            self
        }

        /// Inside a stack: pins this widget to one of nine anchors,
        /// overriding the stack's own alignment.
        pub fn place(mut self, anchor: Alignment) -> Self {
            self.layout.place = Some(anchor);
            self
        }

        /// Moves the widget by this many dp after layout, without affecting
        /// its siblings (a badge overhanging a corner).
        pub fn offset(mut self, x: f32, y: f32) -> Self {
            self.layout.offset = morningstar_core::Point::new(x, y);
            self
        }

        /// Draws the widget, and everything inside it, through a transform
        /// about its own centre, without affecting layout: CSS
        /// `transform` with the default `transform-origin`. The widget
        /// keeps the room it was laid out in; its turned or slanted
        /// picture may overlap its neighbours.
        ///
        /// Lengths in the transform are in dp. Build it as the CSS reads:
        /// `transform: rotate(5deg) translateY(-8px)` is
        /// `.transform(Transform::rotate(5.0).and_translate(0.0, -8.0))`.
        /// A later call replaces an earlier one (and
        /// [`rotate`](Self::rotate)).
        ///
        /// Touches follow the picture: a tappable box that is turned is
        /// hit where it is drawn. What is not handled: a scroll view
        /// *inside* a transformed widget scrolls along the screen's axes,
        /// and [`Ui::rect_of`](crate::Ui::rect_of) reports the layout
        /// rectangle, before the transform.
        pub fn transform(mut self, transform: morningstar_core::Transform) -> Self {
            self.transform = Some(transform);
            self
        }

        /// Turns the widget about its centre, clockwise for positive
        /// degrees: CSS `transform: rotate(..)`. Short for
        /// `.transform(Transform::rotate(degrees))`.
        pub fn rotate(self, degrees: f32) -> Self {
            self.transform(morningstar_core::Transform::rotate(degrees))
        }

        /// Gives the widget an explicit identity. See the crate
        /// documentation, "State and identity".
        pub fn id(mut self, id: impl std::hash::Hash) -> Self {
            self.key = Some(crate::element::key_hash(id));
            self
        }
    };
}

/// Opacity and animation methods, for [`Container`] and [`Element`]. Expects
/// the type to have a `motion: Motion` field.
macro_rules! motion_methods {
    () => {
        /// Draws the widget and everything inside it at this opacity:
        /// `0.0` invisible, `1.0` (the default) as given. It is a group
        /// opacity, as in CSS: a card fades as one picture, and its
        /// label does not let its background show through. Where nothing
        /// in the widget lies over anything else (a text, a picture, a
        /// plain box, a row of those) the opacity is simply multiplied
        /// into each colour, which is the same picture for nothing; a
        /// widget whose parts overlap is drawn through a layer (ADR
        /// 0028), which costs two render passes while it is faded. An
        /// invisible widget still takes its room and its touches.
        pub fn opacity(mut self, opacity: f32) -> Self {
            self.motion.opacity = if opacity.is_nan() {
                1.0
            } else {
                opacity.clamp(0.0, 1.0)
            };
            self
        }

        /// Shows changes of this widget's own properties over `duration`
        /// instead of at once: when a later `view` gives the same widget
        /// (the same identity; give it an `.id(..)`) a different
        /// [`opacity`](Self::opacity), [`transform`](Self::transform) or
        /// [`offset`](Self::offset), solid background colour, or fixed
        /// width or height, it is drawn on the way from the old value to
        /// the new one. A change that arrives mid-way starts from where
        /// the widget has got to.
        ///
        /// Nothing else is animated: not children, not a gradient, and not
        /// a size that hugs or fills (the layout engine decides those). A
        /// width or height that is animated moves its neighbours with it,
        /// frame by frame. Frames are produced only while something is on
        /// its way.
        ///
        /// ```
        /// use std::time::Duration;
        /// use morningstar_ui::prelude::*;
        ///
        /// fn toggle(on: bool) -> Container<()> {
        ///     let knob = Container::empty()
        ///         .size(24.0, 24.0)
        ///         .radius(12.0)
        ///         .background(hex(0xFFFFFF))
        ///         .offset(if on { 20.0 } else { 0.0 }, 0.0)
        ///         .id("knob")
        ///         .animate(Duration::from_millis(160), Easing::EaseOut);
        ///     Container::new(knob)
        ///         .size(48.0, 28.0)
        ///         .padding(2.0)
        ///         .radius(14.0)
        ///         .background(if on { hex(0x34C759) } else { hex(0xC7C7CC) })
        ///         .id("track")
        ///         .animate(Duration::from_millis(160), Easing::EaseOut)
        /// }
        /// # let _ = toggle(true);
        /// ```
        pub fn animate(mut self, duration: std::time::Duration, easing: Easing) -> Self {
            self.motion.animate = Some(Animation::new(duration, easing));
            self
        }

        /// How the widget appears in the first frame it is in the tree and
        /// disappears in the first frame it is not: see [`Transition`].
        /// Identity is what "the same widget" means, so give it an
        /// `.id(..)`.
        ///
        /// A widget that has left is still drawn, where it last was and on
        /// top of the screen, until its transition has run; it takes no
        /// room and no touches while it goes. If, in the frame it leaves,
        /// another widget with a transition enters the same box, the
        /// leaving widget goes the way that newcomer's transition says
        /// (its exit), not its own: that is what lets a screen pushed
        /// from the right send the previous one off to the left.
        pub fn transition(mut self, transition: Transition) -> Self {
            self.motion.transition = Some(transition);
            self
        }
    };
}

/// `size(width, height)` for widgets where "size" means the box (on
/// [`Text`] it means the font size).
macro_rules! box_size_method {
    () => {
        /// Fixed width and height in dp.
        pub fn size(self, width: f32, height: f32) -> Self {
            self.width(width).height(height)
        }
    };
}

/// One node of a screen description. Every widget converts into it.
///
/// `view` returns an `Element`; widgets become one with `.into()`, and the
/// [`col!`](crate::col), [`row!`](crate::row) and
/// [`stack!`](crate::stack) macros convert their children for you.
pub struct Element<M> {
    pub(crate) layout: Style,
    pub(crate) key: Option<u64>,
    /// In dp, about the element's centre.
    pub(crate) transform: Option<Transform>,
    pub(crate) motion: Motion,
    pub(crate) kind: Kind<M>,
}

// A box is larger than a text; boxing it would cost an allocation per
// container per rebuild to save moving a few hundred bytes.
#[allow(clippy::large_enum_variant)]
pub(crate) enum Kind<M> {
    Box(BoxNode<M>),
    Text(TextNode),
    Image(ImageNode),
    Icon(IconNode),
    Input(InputNode<M>),
}

impl<M> Element<M> {
    layout_methods!();
    box_size_method!();
    motion_methods!();

    /// Converts the messages this subtree produces, so a view written for
    /// one message type can be embedded in a parent with another:
    /// `settings::view(&self.settings).map(Message::Settings)`.
    ///
    /// The conversion is kept by any [`TextInput`] in the subtree for as
    /// long as it is on screen (it builds a message per keystroke), hence
    /// the `Send + Sync + 'static` bounds; an enum variant, as above, or any
    /// function meets them.
    pub fn map<N>(self, convert: impl Fn(M) -> N + Copy + Send + Sync + 'static) -> Element<N>
    where
        M: 'static,
        N: 'static,
    {
        Element {
            layout: self.layout,
            key: self.key,
            transform: self.transform,
            motion: self.motion,
            kind: match self.kind {
                Kind::Text(text) => Kind::Text(text),
                Kind::Image(image) => Kind::Image(image),
                Kind::Icon(icon) => Kind::Icon(icon),
                Kind::Input(input) => Kind::Input(InputNode {
                    text: input.text,
                    value: input.value,
                    empty: input.empty,
                    secure: input.secure,
                    traits: input.traits,
                    config: input.config,
                    auto_advance: input.auto_advance,
                    caret_color: input.caret_color,
                    on_input: input.on_input.map(|handler| -> InputHandler<N> {
                        Arc::new(move |value| convert(handler(value)))
                    }),
                    on_submit: input.on_submit.map(convert),
                }),
                Kind::Box(node) => Kind::Box(BoxNode {
                    axis: node.axis,
                    children: node
                        .children
                        .into_iter()
                        .map(|child| child.map(convert))
                        .collect(),
                    fill: node.fill,
                    radius: node.radius,
                    border: node.border,
                    dash: node.dash,
                    shadow: node.shadow,
                    clip: node.clip,
                    on_press: node.on_press.map(convert),
                    pressed_fill: node.pressed_fill,
                    scroll: node.scroll,
                    block: node.block,
                    safe: node.safe,
                    spacer: node.spacer,
                    field: node.field,
                    focused_border: node.focused_border,
                    focused_fill: node.focused_fill,
                }),
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Axis {
    Row,
    Column,
    Stack,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Fill {
    Solid(ColorRef),
    Gradient(Gradient),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SpacerKind {
    Fill(f32),
    Fixed(f32),
}

/// How a border's stroke is broken up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DashKind {
    /// What a browser draws for `border-style: dashed`.
    Css,
    /// Dash and gap lengths in dp.
    Pattern(f32, f32),
}

pub(crate) struct BoxNode<M> {
    pub axis: Axis,
    pub children: Vec<Element<M>>,
    pub fill: Option<Fill>,
    pub radius: f32,
    pub border: Option<(f32, ColorRef)>,
    pub dash: Option<DashKind>,
    pub shadow: Option<Shadow>,
    pub clip: bool,
    pub on_press: Option<M>,
    pub pressed_fill: Option<ColorRef>,
    pub scroll: bool,
    pub block: bool,
    pub safe: SafeEdges,
    pub spacer: Option<SpacerKind>,
    /// The frame of the first text input inside it: a tap on the box
    /// focuses that input.
    pub field: bool,
    /// Border and background while that input has focus.
    pub focused_border: Option<(f32, ColorRef)>,
    pub focused_fill: Option<ColorRef>,
}

impl<M> BoxNode<M> {
    fn new(axis: Axis) -> Self {
        Self {
            axis,
            children: Vec::new(),
            fill: None,
            radius: 0.0,
            border: None,
            dash: None,
            shadow: None,
            clip: false,
            on_press: None,
            pressed_fill: None,
            scroll: false,
            block: false,
            safe: SafeEdges::NONE,
            spacer: None,
            field: false,
            focused_border: None,
            focused_fill: None,
        }
    }
}

pub(crate) struct TextNode {
    /// The text, when it is in one style.
    pub content: Cow<'static, str>,
    /// The text, when it is made of spans; `content` is then empty.
    pub spans: Vec<Span>,
    pub role: TextRole,
    pub size: Option<f32>,
    pub weight: Option<TextWeight>,
    pub line_height: Option<LineHeight>,
    pub color: ColorRef,
    pub align: TextAlign,
    pub max_lines: Option<u32>,
    pub family: Option<FontFamily>,
    /// Extra space after each character, in dp.
    pub letter_spacing: f32,
    /// Never wrapped, whatever width is offered (a single-line input).
    pub no_wrap: bool,
    pub italic: bool,
    pub tabular_nums: bool,
    /// A copy of the text behind it: offsets in dp, and its colour.
    pub shadow: Option<(f32, f32, ColorRef)>,
    /// A stroke around the letters: width in dp, and its colour.
    pub outline: Option<(f32, ColorRef)>,
}

/// Builds the message for a text input's new value.
pub(crate) type InputHandler<M> = Arc<dyn Fn(String) -> M + Send + Sync>;

pub(crate) struct InputNode<M> {
    /// What the field shows when it is not being edited, in the style it
    /// is shown in: the value, its bullets, or the placeholder.
    pub text: TextNode,
    pub value: String,
    /// The value is empty and `text` is the placeholder.
    pub empty: bool,
    pub secure: bool,
    pub traits: TextInputTraits,
    pub config: EditConfig,
    pub auto_advance: bool,
    pub caret_color: ColorRef,
    pub on_input: Option<InputHandler<M>>,
    pub on_submit: Option<M>,
}

/// A line height as designs give it: a multiple of the font size, or a
/// length.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum LineHeight {
    Multiple(f32),
    Dp(f32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct IconNode {
    pub icon: VectorIcon,
    pub color: ColorRef,
    /// Stroke width override in the icon's view-box units.
    pub stroke_width: Option<f32>,
}

/// One stretch of a [`Text`] with its own weight, colour or font family:
/// the bold or coloured words inside a sentence.
///
/// ```
/// use morningstar_ui::prelude::*;
/// let line: Text = Text::rich([
///     span("New to FundiDesk? "),
///     span("Create an account").semibold().color(ThemeColor::Primary),
/// ]);
/// # let _ = line;
/// ```
///
/// Size, line height, alignment and letter spacing belong to the whole
/// [`Text`]; whatever a span does not set comes from it too.
#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub(crate) content: Cow<'static, str>,
    pub(crate) weight: Option<TextWeight>,
    pub(crate) color: Option<ColorRef>,
    pub(crate) family: Option<FontFamily>,
    pub(crate) italic: bool,
}

impl Span {
    /// A span in the style of the text it is part of.
    pub fn new(content: impl Into<Cow<'static, str>>) -> Self {
        Self {
            content: content.into(),
            weight: None,
            color: None,
            family: None,
            italic: false,
        }
    }

    /// Sets this span in italics (CSS `font-style: italic`, an `<em>`).
    /// See [`Text::italic`].
    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    /// Font weight of this span.
    pub fn weight(mut self, weight: TextWeight) -> Self {
        self.weight = Some(weight);
        self
    }

    /// Regular weight (400).
    pub fn regular(self) -> Self {
        self.weight(TextWeight::REGULAR)
    }

    /// Medium weight (500).
    pub fn medium(self) -> Self {
        self.weight(TextWeight::MEDIUM)
    }

    /// Semibold weight (600).
    pub fn semibold(self) -> Self {
        self.weight(TextWeight::SEMIBOLD)
    }

    /// Bold weight (700).
    pub fn bold(self) -> Self {
        self.weight(TextWeight::BOLD)
    }

    /// Colour of this span: a [`Color`](crate::Color) or a [`ThemeColor`].
    pub fn color(mut self, color: impl Into<ColorRef>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// Font family of this span.
    pub fn family(mut self, family: impl Into<FontFamily>) -> Self {
        self.family = Some(family.into());
        self
    }
}

impl From<&'static str> for Span {
    fn from(content: &'static str) -> Self {
        Self::new(content)
    }
}

impl From<String> for Span {
    fn from(content: String) -> Self {
        Self::new(content)
    }
}

/// A [`Span`] in the surrounding text's style. Short for [`Span::new`].
pub fn span(content: impl Into<Cow<'static, str>>) -> Span {
    Span::new(content)
}

/// What a [`Picture`] shows.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PictureSource {
    Image(Image),
    Vector(VectorPicture),
    /// A loaded SVG file, with its own colours (ADR 0027).
    Svg(SvgPicture),
    /// A loaded SVG file drawn as an icon, in the node's tint.
    SvgIcon(SvgPicture),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ImageNode {
    pub source: PictureSource,
    pub fit: ImageFit,
    pub radius: f32,
    pub tint: ColorRef,
}

/// A two-colour linear gradient for [`Container::gradient`].
///
/// Colours are interpolated in linear light, so the middle of a gradient is
/// a little lighter than the same gradient in a design tool (ADR 0011).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gradient {
    pub(crate) degrees: f32,
    pub(crate) from: ColorRef,
    pub(crate) to: ColorRef,
}

impl Gradient {
    /// From `top` at the top edge to `bottom` at the bottom edge.
    pub fn vertical(top: impl Into<ColorRef>, bottom: impl Into<ColorRef>) -> Self {
        Self::angled(180.0, top, bottom)
    }

    /// From `left` at the left edge to `right` at the right edge.
    pub fn horizontal(left: impl Into<ColorRef>, right: impl Into<ColorRef>) -> Self {
        Self::angled(90.0, left, right)
    }

    /// From the top-left corner to the bottom-right corner.
    pub fn diagonal(top_left: impl Into<ColorRef>, bottom_right: impl Into<ColorRef>) -> Self {
        Self::angled(135.0, top_left, bottom_right)
    }

    /// At an angle with the CSS meaning: 0 runs bottom to top, 90 left to
    /// right, 180 top to bottom.
    pub fn angled(degrees: f32, from: impl Into<ColorRef>, to: impl Into<ColorRef>) -> Self {
        Self {
            degrees,
            from: from.into(),
            to: to.into(),
        }
    }
}

/// Which screen edges a [`SafeArea`] keeps its contents away from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SafeEdges {
    /// Left edge (a cutout in landscape).
    pub left: bool,
    /// Top edge (status bar, cutout).
    pub top: bool,
    /// Right edge.
    pub right: bool,
    /// Bottom edge (gesture bar, home indicator).
    pub bottom: bool,
}

impl SafeEdges {
    /// No edge.
    pub const NONE: Self = Self::new(false, false, false, false);
    /// All four edges.
    pub const ALL: Self = Self::new(true, true, true, true);
    /// Top, left and right: for a header whose background runs under the
    /// status bar.
    pub const TOP: Self = Self::new(true, true, true, false);
    /// Bottom, left and right: for a bottom bar whose background runs under
    /// the gesture bar.
    pub const BOTTOM: Self = Self::new(true, false, true, true);
    /// Left and right only.
    pub const HORIZONTAL: Self = Self::new(true, false, true, false);

    /// Edges in the order left, top, right, bottom.
    pub const fn new(left: bool, top: bool, right: bool, bottom: bool) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }
}

/// Ready-made soft shadows for [`Container::shadow`], in dp.
pub mod elevation {
    use morningstar_core::{Color, Shadow};

    /// A hairline lift: list cards on a tinted background.
    pub const LOW: Shadow = Shadow::new(0.0, 1.0, 4.0, Color::rgba(0.0, 0.0, 0.0, 0.10));
    /// A raised card.
    pub const MEDIUM: Shadow = Shadow::new(0.0, 4.0, 14.0, Color::rgba(0.0, 0.0, 0.0, 0.14));
    /// A floating element: hero cards, sheets, floating buttons.
    pub const HIGH: Shadow = Shadow::new(0.0, 10.0, 28.0, Color::rgba(0.0, 0.0, 0.0, 0.20));
}

/// A box: the one widget that has children, padding and decoration.
///
/// [`Column`], [`Row`], [`Stack`], [`ScrollView`], [`SafeArea`],
/// [`Pressable`], [`Spacer`] and the `col!`, `row!` and `stack!` macros
/// all produce a `Container` with a different preset, so every method here
/// works on all of them. It corresponds to a frame with auto-layout in a
/// design tool.
///
/// Paint order: shadow, background, border, then the children in order
/// (later children cover earlier ones).
pub struct Container<M> {
    layout: Style,
    key: Option<u64>,
    transform: Option<Transform>,
    motion: Motion,
    node: BoxNode<M>,
}

impl<M> Container<M> {
    fn with_axis(axis: Axis) -> Self {
        Self {
            layout: Style::new(),
            key: None,
            transform: None,
            motion: Motion::PLAIN,
            node: BoxNode::new(axis),
        }
    }

    /// A box around one child. It hugs the child; the child sits at the
    /// top-left unless [`content_align`](Self::content_align) or
    /// [`center`](Self::center) says otherwise. Use it to give something
    /// padding, a background, a border, a shadow or a tap handler.
    pub fn new(child: impl Into<Element<M>>) -> Self {
        Self::with_axis(Axis::Stack).push(child)
    }

    /// A box with no children: a coloured shape.
    pub fn empty() -> Self {
        Self::with_axis(Axis::Stack)
    }

    layout_methods!();
    box_size_method!();
    motion_methods!();

    /// Adds a child after the existing ones.
    pub fn push(mut self, child: impl Into<Element<M>>) -> Self {
        self.node.children.push(child.into());
        self
    }

    /// Adds a child if there is one: `.push_maybe(self.error.as_ref().map(error_banner))`.
    pub fn push_maybe(self, child: Option<impl Into<Element<M>>>) -> Self {
        match child {
            Some(child) => self.push(child),
            None => self,
        }
    }

    /// Adds every item of an iterator as a child:
    /// `Column::new().extend(self.items.iter().map(item_row))`.
    pub fn extend<E: Into<Element<M>>>(mut self, children: impl IntoIterator<Item = E>) -> Self {
        self.node
            .children
            .extend(children.into_iter().map(Into::into));
        self
    }

    /// The same padding on all four sides, in dp.
    pub fn padding(mut self, all: f32) -> Self {
        self.layout.padding = Edges::all(all);
        self
    }

    /// `horizontal` padding on the left and right, `vertical` on the top
    /// and bottom.
    pub fn padding_xy(mut self, horizontal: f32, vertical: f32) -> Self {
        self.layout.padding = Edges::symmetric(horizontal, vertical);
        self
    }

    /// Padding per edge in the order left, top, right, bottom.
    pub fn padding_ltrb(mut self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
        self.layout.padding = Edges::new(left, top, right, bottom);
        self
    }

    /// Space between adjacent children of a row or column, in dp. Negative
    /// values overlap them. Ignored by stacks.
    pub fn gap(mut self, dp: f32) -> Self {
        self.layout.gap = dp;
        self
    }

    /// Makes a row wrap: children that do not fit on a line continue on
    /// the next, as CSS `flex-wrap: wrap` does. [`gap`](Self::gap) stays
    /// the space between neighbours on a line;
    /// [`line_gap`](Self::line_gap) is the space between lines. Ignored by
    /// columns and stacks.
    ///
    /// ```
    /// use morningstar_ui::prelude::*;
    /// let tags = ["Screen", "Battery", "Water damage", "Charging port"];
    /// let chips: Container<()> = Row::new()
    ///     .extend(tags.map(|tag| {
    ///         Container::new(text(tag).caption())
    ///             .padding_xy(12.0, 6.0)
    ///             .radius(999.0)
    ///             .border(1.0, ThemeColor::Divider)
    ///     }))
    ///     .gap(8.0)
    ///     .line_gap(8.0)
    ///     .wrap()
    ///     .fill_width();
    /// # let _ = chips;
    /// ```
    ///
    /// The rules are the layout engine's ("Wrapping rows" in
    /// `morningstar-layout`): a child moves to the next line when it does
    /// not fit after the ones before it; a hugging child is offered the
    /// whole line, so a long label wraps inside its chip before anything
    /// overflows; a `fill_width()` child takes what its line has left; a
    /// line is as tall as its tallest child; alignment
    /// ([`main_align`](Self::main_align), [`cross_align`](Self::cross_align))
    /// acts within each line; and the row's height, when it hugs, is that
    /// of all its lines, also inside a [`ScrollView`]. A row whose
    /// children fit on one line is laid out exactly as without `wrap()`.
    pub fn wrap(mut self) -> Self {
        self.layout.wrap = true;
        self
    }

    /// Space between the lines of a [`wrap`](Self::wrap)ping row, in dp
    /// (CSS `row-gap`). Zero by default.
    pub fn line_gap(mut self, dp: f32) -> Self {
        self.layout.line_gap = dp;
        self
    }

    /// How a row or column distributes its children along its own axis
    /// when there is space left over.
    pub fn main_align(mut self, align: MainAlign) -> Self {
        self.layout.main_align = align;
        self
    }

    /// Where a row or column puts each child across its axis: vertically
    /// in a row, horizontally in a column.
    pub fn cross_align(mut self, align: CrossAlign) -> Self {
        self.layout.cross_align = align;
        self
    }

    /// Where a stack (or a [`Container::new`]) puts its children: one of
    /// nine anchors.
    pub fn content_align(mut self, anchor: Alignment) -> Self {
        self.layout.content_align = anchor;
        self
    }

    /// Centres the children on both axes, whatever kind of box this is.
    pub fn center(mut self) -> Self {
        self.layout.main_align = MainAlign::Center;
        self.layout.cross_align = CrossAlign::Center;
        self.layout.content_align = Alignment::CENTER;
        self
    }

    /// First child at the start, last at the end, equal space between
    /// (rows and columns).
    pub fn space_between(self) -> Self {
        self.main_align(MainAlign::SpaceBetween)
    }

    /// Fills the box with a colour.
    pub fn background(mut self, color: impl Into<ColorRef>) -> Self {
        self.node.fill = Some(Fill::Solid(color.into()));
        self
    }

    /// Fills the box with a gradient.
    pub fn gradient(mut self, gradient: Gradient) -> Self {
        self.node.fill = Some(Fill::Gradient(gradient));
        self
    }

    /// Corner radius in dp, for the background, border, shadow and clip.
    /// A large value gives a pill or a circle.
    pub fn radius(mut self, dp: f32) -> Self {
        self.node.radius = dp;
        self
    }

    /// A stroke of `width` dp along the inside of the box's edge.
    pub fn border(mut self, width: f32, color: impl Into<ColorRef>) -> Self {
        self.node.border = Some((width, color.into()));
        self
    }

    /// A dashed stroke of `width` dp along the inside of the box's edge,
    /// following its rounded corners: CSS `border: 1px dashed`. The dashes
    /// are the ones a browser draws (three widths long, about two apart,
    /// spaced so a whole number fits).
    pub fn border_dashed(mut self, width: f32, color: impl Into<ColorRef>) -> Self {
        self.node.border = Some((width, color.into()));
        self.node.dash = Some(DashKind::Css);
        self
    }

    /// Breaks the border into dashes `length` dp long and `gap` dp apart,
    /// measured along the middle of the stroke (a design tool's "dash" and
    /// "gap"). The pattern starts at the top of the left edge and runs
    /// down it. Needs a [`border`](Self::border).
    pub fn border_dash(mut self, length: f32, gap: f32) -> Self {
        self.node.dash = Some(DashKind::Pattern(length, gap));
        self
    }

    /// A drop shadow, with offsets, blur and spread in dp. See
    /// [`elevation`] for presets.
    pub fn shadow(mut self, shadow: Shadow) -> Self {
        self.node.shadow = Some(shadow);
        self
    }

    /// Clips the children to the box, following its corner radius.
    pub fn clip(mut self) -> Self {
        self.node.clip = true;
        self
    }

    /// Makes the box tappable: a tap on it delivers `message` to
    /// [`Application::update`](crate::Application::update). While a finger
    /// is on it, the box is drawn pressed.
    pub fn on_press(mut self, message: M) -> Self {
        self.node.on_press = Some(message);
        self
    }

    /// Like [`on_press`](Self::on_press), tappable only with `Some`.
    pub fn on_press_maybe(mut self, message: Option<M>) -> Self {
        self.node.on_press = message;
        self
    }

    /// The background while pressed. Without it, the theme's translucent
    /// `pressed_overlay` is painted over the box.
    pub fn pressed_background(mut self, color: impl Into<ColorRef>) -> Self {
        self.node.pressed_fill = Some(color.into());
        self
    }

    /// Swallows touches on the box so that nothing underneath it reacts: a
    /// modal scrim or a sheet over other content.
    pub fn block_touches(mut self) -> Self {
        self.node.block = true;
        self
    }

    /// Adds the screen's safe-area insets on these edges to the padding.
    /// See [`SafeArea`]. On the bottom edge, the soft keyboard counts while
    /// it is up: a bottom bar rides above it.
    pub fn safe_area(mut self, edges: SafeEdges) -> Self {
        self.node.safe = edges;
        self
    }

    /// Makes this box the frame of the [`TextInput`] inside it (the first
    /// one, if there are several): a tap anywhere on the box, its padding
    /// and icons included, focuses the input. Pressables inside it (a
    /// "show password" button) still get their own taps, and tapping them
    /// does not take the focus away.
    pub fn input_frame(mut self) -> Self {
        self.node.field = true;
        self
    }

    /// The border while the [`TextInput`] inside has focus, replacing
    /// [`border`](Self::border). Makes the box an
    /// [`input_frame`](Self::input_frame).
    pub fn focused_border(mut self, width: f32, color: impl Into<ColorRef>) -> Self {
        self.node.focused_border = Some((width, color.into()));
        self.input_frame()
    }

    /// The background while the [`TextInput`] inside has focus, replacing
    /// [`background`](Self::background). Makes the box an
    /// [`input_frame`](Self::input_frame).
    pub fn focused_background(mut self, color: impl Into<ColorRef>) -> Self {
        self.node.focused_fill = Some(color.into());
        self.input_frame()
    }
}

impl<M> From<Container<M>> for Element<M> {
    fn from(container: Container<M>) -> Self {
        Self {
            layout: container.layout,
            key: container.key,
            transform: container.transform,
            motion: container.motion,
            kind: Kind::Box(container.node),
        }
    }
}

/// Children laid out top to bottom. `Column::new()` gives an empty
/// [`Container`]; [`col!`](crate::col) fills one in.
pub struct Column;

impl Column {
    /// An empty column that hugs its children.
    #[allow(clippy::new_ret_no_self)]
    pub fn new<M>() -> Container<M> {
        Container::with_axis(Axis::Column)
    }
}

/// Children laid out left to right. `Row::new()` gives an empty
/// [`Container`]; [`row!`](crate::row) fills one in.
pub struct Row;

impl Row {
    /// An empty row that hugs its children.
    #[allow(clippy::new_ret_no_self)]
    pub fn new<M>() -> Container<M> {
        Container::with_axis(Axis::Row)
    }
}

/// Children laid out left to right, continuing on a new line when they do
/// not fit: a group of chips or tags. `Wrap::new()` is
/// `Row::new().wrap()`; see [`Container::wrap`].
pub struct Wrap;

impl Wrap {
    /// An empty wrapping row that hugs its children.
    #[allow(clippy::new_ret_no_self)]
    pub fn new<M>() -> Container<M> {
        Container::with_axis(Axis::Row).wrap()
    }
}

/// Children drawn on top of one another, each pinned to one of nine
/// anchors (top-left by default; a child overrides with `.place(..)`).
/// `Stack::new()` gives an empty [`Container`]; [`stack!`](crate::stack)
/// fills one in.
pub struct Stack;

impl Stack {
    /// An empty stack that hugs its largest child.
    #[allow(clippy::new_ret_no_self)]
    pub fn new<M>() -> Container<M> {
        Container::with_axis(Axis::Stack)
    }
}

/// A vertically scrolling viewport.
///
/// `ScrollView::new(content)` fills the space its parent gives it, clips,
/// and lets the content be as tall as it likes; dragging scrolls and a
/// flick keeps going and slows down. The content is stretched to the
/// viewport's width.
///
/// It must be given a height by its parent chain (a `fill` inside a
/// screen-sized column, or a fixed `.height(..)`): a scroll view inside a
/// box that hugs has no height of its own and collapses.
///
/// The scroll position is widget state: see "State and identity" in the
/// crate documentation, and give scroll views on different screens an
/// `.id(..)`.
pub struct ScrollView;

impl ScrollView {
    /// A viewport around `content`, filling its parent.
    #[allow(clippy::new_ret_no_self)]
    pub fn new<M>(content: impl Into<Element<M>>) -> Container<M> {
        let mut container = Container::with_axis(Axis::Column).fill().push(content);
        container.layout.cross_align = CrossAlign::Stretch;
        container.node.scroll = true;
        container.node.clip = true;
        container
    }
}

/// Keeps its child clear of the status bar, display cutout and gesture bar.
///
/// The surface runs edge to edge under the system bars. `SafeArea::new`
/// fills its parent and pads its child by the insets, so put it at the root
/// of a screen. For a header or bottom bar whose *background* should run
/// under the system bar while its *content* stays clear, use
/// [`Container::safe_area`] with [`SafeEdges::TOP`] or
/// [`SafeEdges::BOTTOM`] on the bar itself instead.
///
/// The insets are those of the screen, so this is meant for boxes that
/// touch the screen's edges.
pub struct SafeArea;

impl SafeArea {
    /// A full-size box that insets `child` on all four edges.
    #[allow(clippy::new_ret_no_self)]
    pub fn new<M>(child: impl Into<Element<M>>) -> Container<M> {
        let mut container = Container::with_axis(Axis::Column)
            .fill()
            .push(child)
            .safe_area(SafeEdges::ALL);
        container.layout.cross_align = CrossAlign::Stretch;
        container
    }
}

/// A tappable wrapper around anything: a list row, a card, an icon.
///
/// `Pressable::new(child).on_press(message)` is exactly
/// [`Container::new`] with a tap handler; any `Container` becomes tappable
/// with [`Container::on_press`].
pub struct Pressable;

impl Pressable {
    /// A box around `child`, ready for `.on_press(..)`.
    #[allow(clippy::new_ret_no_self)]
    pub fn new<M>(child: impl Into<Element<M>>) -> Container<M> {
        Container::new(child)
    }
}

/// Empty space inside a row or column.
pub struct Spacer;

impl Spacer {
    /// Takes all the space left along the parent's axis, pushing what
    /// follows to the far end.
    #[allow(clippy::new_ret_no_self)]
    pub fn new<M>() -> Container<M> {
        Self::weighted(1.0)
    }

    /// Like [`new`](Self::new), sharing the space left with other fills in
    /// proportion to `weight`.
    pub fn weighted<M>(weight: f32) -> Container<M> {
        let mut container = Container::empty();
        container.node.spacer = Some(SpacerKind::Fill(weight));
        container
    }

    /// Exactly `dp` along the parent's axis and nothing across it.
    pub fn fixed<M>(dp: f32) -> Container<M> {
        let mut container = Container::empty();
        container.node.spacer = Some(SpacerKind::Fixed(dp));
        container
    }
}

/// A run of text in one style.
///
/// By default it is body text in the theme's text colour, hugs its
/// contents, and wraps when its parent offers less width than it needs.
/// Text that should wrap inside a row next to other things should
/// `.fill_width()`.
///
/// A `Text` has no padding or background of its own: wrap it in a
/// [`Container`] for those.
pub struct Text {
    layout: Style,
    key: Option<u64>,
    transform: Option<Transform>,
    pub(crate) node: TextNode,
}

impl Text {
    /// Text from a literal or a `String`. A `&'static str` is not copied.
    pub fn new(content: impl Into<Cow<'static, str>>) -> Self {
        Self {
            layout: Style::new(),
            key: None,
            transform: None,
            node: TextNode {
                content: content.into(),
                spans: Vec::new(),
                role: TextRole::Body,
                size: None,
                weight: None,
                line_height: None,
                color: ColorRef::Token(ThemeColor::Text),
                align: TextAlign::Start,
                max_lines: None,
                family: None,
                letter_spacing: 0.0,
                no_wrap: false,
                italic: false,
                tabular_nums: false,
                shadow: None,
                outline: None,
            },
        }
    }

    /// Text made of several [`Span`]s, each of which may have its own
    /// weight, colour or font family: a sentence with a bold or coloured
    /// phrase in it. The spans flow and wrap as one paragraph.
    ///
    /// ```
    /// use morningstar_ui::prelude::*;
    /// let hint: Text = Text::rich([
    ///     span("Enter the code we sent to "),
    ///     span("+255 7XX XXX 678").semibold().color(ThemeColor::Text),
    ///     span("."),
    /// ])
    /// .muted()
    /// .fill_width();
    /// # let _ = hint;
    /// ```
    pub fn rich<S: Into<Span>>(spans: impl IntoIterator<Item = S>) -> Self {
        let mut text = Self::new("");
        text.node.spans = spans.into_iter().map(Into::into).collect();
        text
    }

    layout_methods!();

    /// Uses a step of the theme's type scale (size, weight and line
    /// height). Calls to `size`, `weight` and `line_height` override parts
    /// of it.
    pub fn role(mut self, role: TextRole) -> Self {
        self.node.role = role;
        self
    }

    /// The one big number or word on a screen.
    pub fn display(self) -> Self {
        self.role(TextRole::Display)
    }

    /// A screen title.
    pub fn title(self) -> Self {
        self.role(TextRole::Title)
    }

    /// A section or card heading.
    pub fn headline(self) -> Self {
        self.role(TextRole::Headline)
    }

    /// Running text. The default.
    pub fn body(self) -> Self {
        self.role(TextRole::Body)
    }

    /// Button, tab and field labels.
    pub fn label(self) -> Self {
        self.role(TextRole::Label)
    }

    /// Metadata, timestamps and hints.
    pub fn caption(self) -> Self {
        self.role(TextRole::Caption)
    }

    /// Font size in dp, overriding the role's.
    pub fn size(mut self, dp: f32) -> Self {
        self.node.size = Some(dp);
        self
    }

    /// Font weight, overriding the role's.
    pub fn weight(mut self, weight: TextWeight) -> Self {
        self.node.weight = Some(weight);
        self
    }

    /// Regular weight (400).
    pub fn regular(self) -> Self {
        self.weight(TextWeight::REGULAR)
    }

    /// Medium weight (500).
    pub fn medium(self) -> Self {
        self.weight(TextWeight::MEDIUM)
    }

    /// Semibold weight (600).
    pub fn semibold(self) -> Self {
        self.weight(TextWeight::SEMIBOLD)
    }

    /// Bold weight (700).
    pub fn bold(self) -> Self {
        self.weight(TextWeight::BOLD)
    }

    /// Text colour: a [`Color`](crate::Color) or a [`ThemeColor`].
    pub fn color(mut self, color: impl Into<ColorRef>) -> Self {
        self.node.color = color.into();
        self
    }

    /// The theme's secondary text colour.
    pub fn muted(self) -> Self {
        self.color(ThemeColor::TextMuted)
    }

    /// Alignment of the lines within the text's own box. It only shows
    /// when the box is wider than the lines: combine it with
    /// `.fill_width()` or a fixed width, or let the parent centre a
    /// hugging text instead.
    pub fn align(mut self, align: TextAlign) -> Self {
        self.node.align = align;
        self
    }

    /// Centres the lines within the text's box. See [`align`](Self::align).
    pub fn centered(self) -> Self {
        self.align(TextAlign::Center)
    }

    /// Line height as a multiple of the font size, overriding the role's.
    pub fn line_height(mut self, multiple: f32) -> Self {
        self.node.line_height = Some(LineHeight::Multiple(multiple));
        self
    }

    /// Line height in dp, as CSS and design tools usually give it
    /// (`line-height: 22px`), overriding the role's.
    pub fn line_height_dp(mut self, dp: f32) -> Self {
        self.node.line_height = Some(LineHeight::Dp(dp));
        self
    }

    /// The font family, overriding the theme's choice for the text's role
    /// (see [`Fonts`](crate::Fonts)): `.family("Space Grotesk")`. The font
    /// must have been added in `setup`.
    pub fn family(mut self, family: impl Into<FontFamily>) -> Self {
        self.node.family = Some(family.into());
        self
    }

    /// Extra space after every character, in dp (CSS `letter-spacing`,
    /// a design tool's "letter spacing" in pixels). Negative tightens.
    pub fn letter_spacing(mut self, dp: f32) -> Self {
        self.node.letter_spacing = dp;
        self
    }

    /// Sets the text in italics (CSS `font-style: italic`). If an italic
    /// font of the family was added in `setup` it is used; otherwise the
    /// upright letters are slanted by 14 degrees, as a browser does for a
    /// family without an italic. The text takes the same room either way.
    pub fn italic(mut self) -> Self {
        self.node.italic = true;
        self
    }

    /// Sets digits on a common width, so that columns of numbers line up
    /// and a running timer keeps still (CSS
    /// `font-variant-numeric: tabular-nums`). It needs the font to have the
    /// `tnum` feature and does nothing otherwise.
    pub fn tabular_nums(mut self) -> Self {
        self.node.tabular_nums = true;
        self
    }

    /// A hard shadow: the same text drawn once more behind it, moved by
    /// `(x, y)` dp, in `color` (CSS `text-shadow: 3px 3px 0 #fff`). There
    /// is no blur. The shadow takes no room in layout.
    pub fn shadow(mut self, x: f32, y: f32, color: impl Into<ColorRef>) -> Self {
        self.node.shadow = Some((x, y, color.into()));
        self
    }

    /// Outlines the letters with a stroke `width` dp wide, centred on
    /// their edges, over the fill (CSS `-webkit-text-stroke: 1.5px #fff`).
    /// For a large digit or a word, not for paragraphs: outlined text is
    /// drawn as an image (ADR 0017), in the text's one colour (span
    /// colours are ignored).
    pub fn outline(mut self, width: f32, color: impl Into<ColorRef>) -> Self {
        self.node.outline = Some((width, color.into()));
        self
    }

    /// Shows the text in capitals (CSS `text-transform: uppercase`). This
    /// rewrites the characters; it is not a font feature.
    pub fn uppercase(mut self) -> Self {
        let upper = |content: &mut Cow<'static, str>| {
            if content.chars().any(char::is_lowercase) {
                *content = Cow::Owned(content.to_uppercase());
            }
        };
        upper(&mut self.node.content);
        self.node
            .spans
            .iter_mut()
            .for_each(|span| upper(&mut span.content));
        self
    }

    /// Shows at most this many lines; the rest is cut off (there is no
    /// ellipsis yet).
    pub fn max_lines(mut self, lines: u32) -> Self {
        self.node.max_lines = Some(lines);
        self
    }
}

impl<M> From<Text> for Element<M> {
    fn from(text: Text) -> Self {
        Self {
            layout: text.layout,
            key: text.key,
            transform: text.transform,
            motion: Motion::PLAIN,
            kind: Kind::Text(text.node),
        }
    }
}

impl<M> From<&'static str> for Element<M> {
    fn from(content: &'static str) -> Self {
        Text::new(content).into()
    }
}

impl<M> From<String> for Element<M> {
    fn from(content: String) -> Self {
        Text::new(content).into()
    }
}

/// Body text. Short for [`Text::new`].
pub fn text(content: impl Into<Cow<'static, str>>) -> Text {
    Text::new(content)
}

/// An editable line (or, with [`multiline`](Self::multiline), paragraph)
/// of text.
///
/// The application owns the string. The widget shows `value` and, when the
/// user types, hands the *whole new string* to
/// [`on_input`](Self::on_input); `update` stores it and the next `view`
/// passes it back in:
///
/// ```
/// use morningstar_ui::prelude::*;
///
/// #[derive(Clone)]
/// enum Message { Phone(String), Password(String), LogIn }
///
/// struct Login { phone: String, password: String }
///
/// fn form(state: &Login) -> Container<Message> {
///     let frame = |input: TextInput<Message>| {
///         Container::new(input)
///             .fill_width()
///             .padding_xy(16.0, 14.0)
///             .radius(12.0)
///             .border(1.0, ThemeColor::Divider)
///             .focused_border(1.5, ThemeColor::Primary)
///     };
///     col![
///         frame(
///             TextInput::new(&state.phone)
///                 .placeholder("07XX XXX XXX")
///                 .keyboard(KeyboardType::Phone)
///                 .action(ImeAction::Next)
///                 .on_input(Message::Phone)
///                 .id("phone"),
///         ),
///         frame(
///             TextInput::new(&state.password)
///                 .placeholder("Password")
///                 .secure(true)
///                 .on_input(Message::Password)
///                 .on_submit(Message::LogIn)
///                 .id("password"),
///         ),
///     ]
///     .gap(12.0)
/// }
/// # let _ = form(&Login { phone: String::new(), password: String::new() });
/// ```
///
/// Like [`Text`], a `TextInput` is only the text: it has no padding,
/// border or background. Put it in a [`Container`] for those and call
/// [`Container::focused_border`] (or [`Container::input_frame`]) on that
/// box, which also makes the whole box tappable to focus. Unfocused, it
/// measures and draws exactly as a `Text` with the same style would, so a
/// field can be matched to a design before it is ever typed in.
///
/// Behaviour: a tap focuses it and puts the caret where the finger was;
/// the platform keyboard appears, of the kind [`keyboard`](Self::keyboard)
/// asks for; typing inserts at the caret and backspace removes a whole
/// character (a letter with its combining marks); a single-line field
/// scrolls sideways to keep the caret in view; the keyboard's Next key
/// moves to the next `TextInput` in the tree, Done (or tapping anywhere
/// else) gives up focus. A value the application changes or refuses in
/// `update` is what the field then shows.
///
/// Which field has focus, and where its caret is, is widget state keyed by
/// identity ("State and identity" in the crate documentation): give inputs
/// an `.id(..)` when fields can appear or disappear around them.
///
/// It fills the width it is given; [`hug_width`](Self::hug_width) makes
/// it as wide as its text (a centred code box). A
/// [`multiline`](Self::multiline) field grows with its text, up to
/// [`max_lines`](Self::max_lines) if that is set, and then scrolls inside
/// itself.
///
/// Selection and the clipboard work as in a native field and need
/// nothing from the application: a double tap or a long press selects a
/// word and shows two handles and a menu (Cut, Copy, Paste, Select all;
/// only what applies), dragging a handle extends the selection and scrolls
/// the field at its edges, typing replaces what is selected, and a
/// hardware keyboard's arrows, Home, End and Ctrl/Cmd shortcuts work. A
/// [`secure`](Self::secure) field can be pasted into and never copied
/// from; pasted text passes [`digits_only`](Self::digits_only) and
/// [`max_length`](Self::max_length) like typed text. `on_input` still
/// delivers only the new string.
pub struct TextInput<M> {
    layout: Style,
    key: Option<u64>,
    transform: Option<Transform>,
    value: String,
    placeholder: Cow<'static, str>,
    placeholder_color: ColorRef,
    style: TextNode,
    secure: bool,
    traits: TextInputTraits,
    action_set: bool,
    config: EditConfig,
    auto_advance: bool,
    caret_color: ColorRef,
    on_input: Option<InputHandler<M>>,
    on_submit: Option<M>,
}

impl<M> TextInput<M> {
    /// A single-line field showing `value`, in the theme's body text.
    pub fn new(value: impl Into<String>) -> Self {
        let mut style = Text::new("").node;
        style.no_wrap = true;
        Self {
            layout: Style::new().width(Sizing::FILL),
            key: None,
            transform: None,
            value: value.into(),
            placeholder: Cow::Borrowed(""),
            placeholder_color: ColorRef::Token(ThemeColor::TextMuted),
            style,
            secure: false,
            traits: TextInputTraits {
                autocorrect: true,
                ..TextInputTraits::default()
            },
            action_set: false,
            config: EditConfig::default(),
            auto_advance: false,
            caret_color: ColorRef::Token(ThemeColor::Primary),
            on_input: None,
            on_submit: None,
        }
    }

    layout_methods!();

    /// Shown, in the [`placeholder_color`](Self::placeholder_color), while
    /// the value is empty.
    pub fn placeholder(mut self, text: impl Into<Cow<'static, str>>) -> Self {
        self.placeholder = text.into();
        self
    }

    /// Colour of the placeholder (the theme's muted text by default).
    pub fn placeholder_color(mut self, color: impl Into<ColorRef>) -> Self {
        self.placeholder_color = color.into();
        self
    }

    /// The message for a new value. Called with the whole text after every
    /// change; store it and pass it back as the value. Without it the field
    /// can be focused but not changed.
    pub fn on_input(mut self, message: impl Fn(String) -> M + Send + Sync + 'static) -> Self {
        self.on_input = Some(Arc::new(message));
        self
    }

    /// The message delivered when the keyboard's action key (Done, Go,
    /// Search, Send) is pressed. The field gives up focus first. The Next
    /// action moves focus instead and delivers nothing.
    pub fn on_submit(mut self, message: M) -> Self {
        self.on_submit = Some(message);
        self
    }

    /// A password: each character is shown as a bullet, and the keyboard
    /// neither learns nor suggests.
    pub fn secure(mut self, secure: bool) -> Self {
        self.secure = secure;
        self
    }

    /// Which keyboard to show. [`KeyboardType::Number`] and the others
    /// only choose the keys offered; to refuse everything but digits, also
    /// call [`digits_only`](Self::digits_only).
    pub fn keyboard(mut self, keyboard: KeyboardType) -> Self {
        self.traits.keyboard = keyboard;
        self
    }

    /// What the keyboard capitalises by itself (nothing by default).
    pub fn capitalization(mut self, capitalization: Capitalization) -> Self {
        self.traits.capitalization = capitalization;
        self
    }

    /// The keyboard's action key: [`ImeAction::Done`] by default,
    /// [`ImeAction::Next`] for a field followed by another.
    pub fn action(mut self, action: ImeAction) -> Self {
        self.traits.action = action;
        self.action_set = true;
        self
    }

    /// Turns the keyboard's correction and suggestions off (they are on by
    /// default, and always off for a secure field): for names, codes and
    /// references.
    pub fn no_autocorrect(mut self) -> Self {
        self.traits.autocorrect = false;
        self
    }

    /// Several lines: the text wraps at the field's width, the field grows
    /// downwards with it, and the Return key inserts a line break. Bound
    /// the growth with [`max_lines`](Self::max_lines).
    pub fn multiline(mut self) -> Self {
        self.config.multiline = true;
        self.traits.multiline = true;
        self.style.no_wrap = false;
        self
    }

    /// For a [`multiline`](Self::multiline) field: it grows to at most
    /// this many lines and then scrolls inside itself. The layout never
    /// sees more than that height, so nothing overflows; the text is
    /// clipped to the field and scrolled so that the caret stays in view
    /// (with a line of context where there is room, so tapping the first
    /// or last visible line brings the next one in), and a drag on the
    /// field scrolls it like a [`ScrollView`]. `.max_height(dp)` bounds it
    /// by a length instead, and both may be given.
    ///
    /// The position is widget state kept by identity, like a scroll
    /// view's: give the field an `.id(..)`.
    pub fn max_lines(mut self, lines: u32) -> Self {
        self.style.max_lines = Some(lines.max(1));
        self
    }

    /// Most characters the value may hold. What does not fit is dropped.
    pub fn max_length(mut self, characters: usize) -> Self {
        self.config.max_length = Some(characters);
        self
    }

    /// Keeps only the digits 0 to 9 of whatever is typed or pasted by the
    /// keyboard.
    pub fn digits_only(mut self) -> Self {
        self.config.digits_only = true;
        self
    }

    /// For one field of a row of code boxes, with
    /// [`max_length`](Self::max_length): when the field becomes full, focus
    /// moves to the next `TextInput`; backspace in an empty field moves to
    /// the previous one and deletes there; and a character typed into a
    /// full field replaces the one it holds.
    pub fn auto_advance(mut self) -> Self {
        self.auto_advance = true;
        self.config.overwrite_when_full = true;
        self
    }

    /// Takes size, weight, line height, colour, alignment, family and
    /// letter spacing from a [`Text`] (its content is ignored), so a field
    /// can share a design's text style helper: `.style(body("", 15.0))`.
    pub fn style(mut self, text: Text) -> Self {
        let no_wrap = self.style.no_wrap;
        let max_lines = self.style.max_lines;
        self.style = TextNode {
            content: Cow::Borrowed(""),
            spans: Vec::new(),
            max_lines,
            no_wrap,
            ..text.node
        };
        let align = self.style.align;
        self.align(align)
    }

    /// Uses a step of the theme's type scale; see [`Text::role`].
    pub fn role(mut self, role: TextRole) -> Self {
        self.style.role = role;
        self
    }

    /// Font size in dp.
    pub fn size(mut self, dp: f32) -> Self {
        self.style.size = Some(dp);
        self
    }

    /// Font weight.
    pub fn weight(mut self, weight: TextWeight) -> Self {
        self.style.weight = Some(weight);
        self
    }

    /// Colour of the value (the theme's text colour by default).
    pub fn color(mut self, color: impl Into<ColorRef>) -> Self {
        self.style.color = color.into();
        self
    }

    /// Colour of the caret (the theme's primary colour by default).
    pub fn caret_color(mut self, color: impl Into<ColorRef>) -> Self {
        self.caret_color = color.into();
        self
    }

    /// The font family; see [`Text::family`].
    pub fn family(mut self, family: impl Into<FontFamily>) -> Self {
        self.style.family = Some(family.into());
        self
    }

    /// Line height as a multiple of the font size.
    pub fn line_height(mut self, multiple: f32) -> Self {
        self.style.line_height = Some(LineHeight::Multiple(multiple));
        self
    }

    /// Line height in dp.
    pub fn line_height_dp(mut self, dp: f32) -> Self {
        self.style.line_height = Some(LineHeight::Dp(dp));
        self
    }

    /// Extra space after every character, in dp.
    pub fn letter_spacing(mut self, dp: f32) -> Self {
        self.style.letter_spacing = dp;
        self
    }

    /// Where the text sits in the field. Centred or end-aligned text makes
    /// the field hug its text, so that the box around it can place it:
    /// centre a one-digit box with `Container::new(input).center()`.
    pub fn align(mut self, align: TextAlign) -> Self {
        self.style.align = align;
        if align != TextAlign::Start && !self.config.multiline {
            self.layout.width = Sizing::Hug;
        }
        self
    }
}

impl<M> From<TextInput<M>> for Element<M> {
    fn from(input: TextInput<M>) -> Self {
        let empty = input.value.is_empty();
        let mut text = input.style;
        if empty {
            text.content = input.placeholder;
            text.color = input.placeholder_color;
        } else {
            text.content = Cow::Owned(edit::display(&input.value, input.secure).into_owned());
        }
        let mut traits = input.traits;
        traits.secure = input.secure;
        if !input.action_set && input.on_submit.is_none() && !traits.multiline {
            // Nothing to submit: Done just puts the keyboard away.
            traits.action = ImeAction::Done;
        }
        Self {
            layout: input.layout,
            key: input.key,
            transform: input.transform,
            motion: Motion::PLAIN,
            kind: Kind::Input(InputNode {
                text,
                value: input.value,
                empty,
                secure: input.secure,
                traits,
                config: input.config,
                auto_advance: input.auto_advance,
                caret_color: input.caret_color,
                on_input: input.on_input,
                on_submit: input.on_submit,
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ButtonKind {
    Primary,
    Secondary,
    Outlined,
    Plain,
    Destructive,
}

/// A labelled button.
///
/// ```
/// use morningstar_ui::prelude::*;
/// #[derive(Clone)]
/// enum Message { Save, Delete }
/// let save: Button<Message> = Button::new("Save").on_press(Message::Save).fill_width();
/// let delete = Button::new("Delete account").destructive().on_press(Message::Delete);
/// # let _ = (save, delete);
/// ```
///
/// It is at least 48 dp tall, hugs its label horizontally (use
/// `.fill_width()` for a full-width button) and takes its colours from the
/// theme. A button without `on_press` is drawn disabled and ignores
/// touches. For anything a `Button` cannot express, build one from a
/// [`Container`] with [`on_press`](Container::on_press).
pub struct Button<M> {
    layout: Style,
    key: Option<u64>,
    transform: Option<Transform>,
    label: Cow<'static, str>,
    on_press: Option<M>,
    kind: ButtonKind,
    radius: f32,
}

impl<M> Button<M> {
    /// A filled button in the theme's primary colour.
    pub fn new(label: impl Into<Cow<'static, str>>) -> Self {
        Self {
            layout: Style::new()
                .min_height(48.0)
                .padding(Edges::symmetric(20.0, 10.0))
                .content_align(Alignment::CENTER),
            key: None,
            transform: None,
            label: label.into(),
            on_press: None,
            kind: ButtonKind::Primary,
            radius: 12.0,
        }
    }

    layout_methods!();
    box_size_method!();

    /// The message a tap delivers.
    pub fn on_press(mut self, message: M) -> Self {
        self.on_press = Some(message);
        self
    }

    /// Enabled only with `Some`.
    pub fn on_press_maybe(mut self, message: Option<M>) -> Self {
        self.on_press = message;
        self
    }

    /// Filled with the primary colour. The default.
    pub fn primary(mut self) -> Self {
        self.kind = ButtonKind::Primary;
        self
    }

    /// Filled with the quiet `SurfaceVariant` colour.
    pub fn secondary(mut self) -> Self {
        self.kind = ButtonKind::Secondary;
        self
    }

    /// Transparent with an outline.
    pub fn outlined(mut self) -> Self {
        self.kind = ButtonKind::Outlined;
        self
    }

    /// Label only, in the primary colour.
    pub fn plain(mut self) -> Self {
        self.kind = ButtonKind::Plain;
        self
    }

    /// For actions that delete or sign out: filled with the danger colour.
    pub fn destructive(mut self) -> Self {
        self.kind = ButtonKind::Destructive;
        self
    }

    /// Corner radius in dp (12 by default).
    pub fn radius(mut self, dp: f32) -> Self {
        self.radius = dp;
        self
    }
}

impl<M> From<Button<M>> for Element<M> {
    fn from(button: Button<M>) -> Self {
        use ThemeColor as T;
        let enabled = button.on_press.is_some();
        let (fill, foreground, border): (Option<ColorRef>, ColorRef, Option<ColorRef>) =
            match (button.kind, enabled) {
                (ButtonKind::Plain, false) => (None, T::TextMuted.into(), None),
                (ButtonKind::Outlined, false) => {
                    (None, T::TextMuted.into(), Some(T::Divider.into()))
                }
                (_, false) => (Some(T::SurfaceVariant.into()), T::TextMuted.into(), None),
                (ButtonKind::Primary, true) => (Some(T::Primary.into()), T::OnPrimary.into(), None),
                (ButtonKind::Secondary, true) => {
                    (Some(T::SurfaceVariant.into()), T::Text.into(), None)
                }
                (ButtonKind::Outlined, true) => (None, T::Primary.into(), Some(T::Divider.into())),
                (ButtonKind::Plain, true) => (None, T::Primary.into(), None),
                (ButtonKind::Destructive, true) => {
                    (Some(T::Danger.into()), T::OnDanger.into(), None)
                }
            };

        let mut node = BoxNode::new(Axis::Stack);
        node.children
            .push(Text::new(button.label).label().color(foreground).into());
        node.fill = fill.map(Fill::Solid);
        node.border = border.map(|color| (1.0, color));
        node.radius = button.radius;
        node.on_press = button.on_press;
        Self {
            layout: button.layout,
            key: button.key,
            transform: button.transform,
            motion: Motion::PLAIN,
            kind: Kind::Box(node),
        }
    }
}

/// A hairline between rows or sections.
///
/// Horizontal by default: it fills the width and is 1 dp thick, in the
/// theme's divider colour.
pub struct Divider {
    thickness: f32,
    inset: f32,
    color: ColorRef,
    vertical: bool,
}

impl Divider {
    /// A horizontal divider.
    pub fn new() -> Self {
        Self {
            thickness: 1.0,
            inset: 0.0,
            color: ColorRef::Token(ThemeColor::Divider),
            vertical: false,
        }
    }

    /// A vertical divider, for rows: 1 dp wide, filling the height.
    pub fn vertical() -> Self {
        Self {
            vertical: true,
            ..Self::new()
        }
    }

    /// Thickness in dp.
    pub fn thickness(mut self, dp: f32) -> Self {
        self.thickness = dp;
        self
    }

    /// Leaves `dp` clear at the leading end (left of a horizontal divider,
    /// top of a vertical one), so the line starts where a row's text does.
    pub fn inset(mut self, dp: f32) -> Self {
        self.inset = dp;
        self
    }

    /// Line colour.
    pub fn color(mut self, color: impl Into<ColorRef>) -> Self {
        self.color = color.into();
        self
    }
}

impl Default for Divider {
    fn default() -> Self {
        Self::new()
    }
}

impl<M> From<Divider> for Element<M> {
    fn from(divider: Divider) -> Self {
        let line: Container<M> = Container::empty().background(divider.color);
        if divider.vertical {
            let line = line.width(divider.thickness).fill_height();
            if divider.inset > 0.0 {
                Column::new()
                    .fill_height()
                    .padding_ltrb(0.0, divider.inset, 0.0, 0.0)
                    .push(line)
                    .into()
            } else {
                line.into()
            }
        } else {
            let line = line.height(divider.thickness).fill_width();
            if divider.inset > 0.0 {
                Row::new()
                    .fill_width()
                    .padding_ltrb(divider.inset, 0.0, 0.0, 0.0)
                    .push(line)
                    .into()
            } else {
                line.into()
            }
        }
    }
}

/// A registered image, or a multi-colour vector picture, drawn into a box.
///
/// Register the image once, in [`Application::setup`]
/// (crate::Application::setup), and keep the handle in the application
/// state:
///
/// ```ignore
/// fn setup(&mut self, setup: &mut Setup<'_>) {
///     self.logo = setup.add_image_png(include_bytes!("../assets/logo.png")).ok();
/// }
/// // in view:
/// Picture::new(logo).size(96.0, 96.0).radius(16.0)
/// ```
///
/// Sizing: with no size given, a picture is as many dp as the image has
/// pixels. With one dimension given (fixed or fill), the other follows the
/// image's aspect ratio. With both given, [`fit`](Self::fit) decides how
/// the image fills the box: [`ImageFit::Cover`] (the default) crops,
/// `Contain` letterboxes, `Stretch` distorts.
///
/// (Named `Picture` because [`Image`] is the handle.)
///
/// # Vector pictures
///
/// [`Picture::vector`] shows a [`VectorPicture`]: an illustration in
/// several colours described in Rust, layer by layer, from the SVG a design
/// has inline. It is rasterised for the exact pixels it covers, so it is
/// sharp at any size and display scale.
///
/// ```
/// use morningstar_ui::prelude::*;
///
/// const INK: Color = hex(0x0E0E0E);
/// const GOLD: Color = hex(0xF6C445);
///
/// // <svg width="250" height="250" viewBox="0 0 280 280">
/// //   <circle cx="140" cy="140" r="122" fill="#0E0E0E" stroke="#F6C445" stroke-width="1.5"/>
/// //   <rect x="-40" y="114" width="360" height="52" fill="#F6C445"
/// //         transform="rotate(-18 140 140)" clip-path="(the circle above)"/>
/// //   <circle cx="140" cy="140" r="114" fill="none" stroke="#F6C445" stroke-opacity="0.55"/>
/// // </svg>
/// const DISC: VectorPicture = VectorPicture::new(
///     280.0,
///     280.0,
///     &[
///         Layer::circle(140.0, 140.0, 122.0).fill(INK).stroke(GOLD, 1.5),
///         Layer::rect(-40.0, 114.0, 360.0, 52.0)
///             .fill(GOLD)
///             .rotate(-18.0, 140.0, 140.0)
///             .clip_circle(140.0, 140.0, 122.0),
///         Layer::circle(140.0, 140.0, 114.0).stroke(with_alpha(GOLD, 0.55), 1.0),
///     ],
/// );
///
/// // The SVG's `width` and `height` are the size in dp.
/// let disc = Picture::vector(DISC).size(250.0, 250.0);
/// # let _ = disc;
/// ```
///
/// With no size given a vector picture is as many dp as its view box has
/// units; with one dimension given the other follows the view box's shape.
/// It is always shown whole, centred in its box ([`fit`](Self::fit) does
/// not apply); [`radius`](Self::radius) and [`tint`](Self::tint) work as
/// for an image.
pub struct Picture {
    layout: Style,
    key: Option<u64>,
    transform: Option<Transform>,
    node: ImageNode,
}

impl Picture {
    /// A picture of `image`.
    pub fn new(image: Image) -> Self {
        Self {
            layout: Style::new(),
            key: None,
            transform: None,
            node: ImageNode {
                source: PictureSource::Image(image),
                fit: ImageFit::Cover,
                radius: 0.0,
                tint: ColorRef::Value(Color::WHITE),
            },
        }
    }

    /// A multi-colour vector picture. See "Vector pictures" above.
    pub fn vector(picture: VectorPicture) -> Self {
        Self {
            layout: Style::new(),
            key: None,
            transform: None,
            node: ImageNode {
                source: PictureSource::Vector(picture),
                fit: ImageFit::Contain,
                radius: 0.0,
                tint: ColorRef::Value(Color::WHITE),
            },
        }
    }

    /// A picture loaded from an SVG file: [`SvgPicture::parse`], or a
    /// `static` [`SvgAsset`](crate::SvgAsset) beside `include_str!`. Parse
    /// the file once and keep the value; this takes a cheap handle to it.
    ///
    /// ```
    /// use morningstar_ui::prelude::*;
    ///
    /// // In an application: SvgAsset::new(include_str!("../assets/svg/disc.svg"))
    /// static DISC: SvgAsset = SvgAsset::new(
    ///     r##"<svg viewBox="0 0 100 100"><circle cx="50" cy="50" r="48" fill="#0E0E0E"/></svg>"##,
    /// );
    /// let disc = Picture::svg(DISC.get()).size(250.0, 250.0);
    /// # let _ = disc;
    /// ```
    ///
    /// With no size given it is as many dp as the file's `width` and
    /// `height` say, or its view box where they are absent. Otherwise as a
    /// [vector picture](Self::vector): shown whole, centred, rasterised
    /// for the pixels it covers; `currentColor` in the file is the colour
    /// of [`SvgPicture::with_current_color`], black unless set.
    pub fn svg(picture: &SvgPicture) -> Self {
        Self {
            layout: Style::new(),
            key: None,
            transform: None,
            node: ImageNode {
                source: PictureSource::Svg(picture.clone()),
                fit: ImageFit::Contain,
                radius: 0.0,
                tint: ColorRef::Value(Color::WHITE),
            },
        }
    }

    layout_methods!();
    box_size_method!();

    /// How the image fills a box of a different shape.
    pub fn fit(mut self, fit: ImageFit) -> Self {
        self.node.fit = fit;
        self
    }

    /// Corner radius in dp. Half the side of a square picture gives a
    /// circle.
    pub fn radius(mut self, dp: f32) -> Self {
        self.node.radius = dp;
        self
    }

    /// Multiplies every pixel by a colour: white with an alpha of 0.5 draws
    /// the image half transparent; a colour recolours an icon drawn in
    /// white.
    pub fn tint(mut self, color: impl Into<ColorRef>) -> Self {
        self.node.tint = color.into();
        self
    }
}

impl<M> From<Picture> for Element<M> {
    fn from(picture: Picture) -> Self {
        let mut layout = picture.layout;
        let (width, height) = match &picture.node.source {
            PictureSource::Image(image) => (image.width() as f32, image.height() as f32),
            PictureSource::Vector(vector) => (vector.width(), vector.height()),
            PictureSource::Svg(svg) | PictureSource::SvgIcon(svg) => (svg.width(), svg.height()),
        };
        match (layout.width, layout.height) {
            // Natural size: one dp per image pixel or view-box unit.
            (Sizing::Hug, Sizing::Hug) => {
                layout.width = Sizing::Fixed(width);
                layout.height = Sizing::Fixed(height);
            }
            // One dimension given: the other keeps the image's shape.
            (Sizing::Hug, _) | (_, Sizing::Hug)
                if layout.aspect_ratio.is_none() && width > 0.0 && height > 0.0 =>
            {
                layout.aspect_ratio = Some(width / height);
            }
            _ => {}
        }
        Self {
            layout,
            key: picture.key,
            transform: picture.transform,
            motion: Motion::PLAIN,
            kind: Kind::Image(picture.node),
        }
    }
}

/// A round picture of a person: their image if there is one, otherwise
/// their initials on a coloured circle.
pub struct Avatar {
    layout: Style,
    key: Option<u64>,
    transform: Option<Transform>,
    initials: String,
    image: Option<Image>,
    diameter: f32,
    background: ColorRef,
    color: ColorRef,
}

impl Avatar {
    /// An avatar for a person called `name`: "Amina Juma" shows "AJ".
    pub fn new(name: &str) -> Self {
        let initials = name
            .split_whitespace()
            .filter_map(|word| word.chars().next())
            .take(2)
            .flat_map(char::to_uppercase)
            .collect();
        Self {
            layout: Style::new(),
            key: None,
            transform: None,
            initials,
            image: None,
            diameter: 40.0,
            background: ColorRef::Token(ThemeColor::PrimaryContainer),
            color: ColorRef::Token(ThemeColor::Primary),
        }
    }

    layout_methods!();

    /// Diameter in dp (40 by default). The initials scale with it.
    pub fn size(mut self, dp: f32) -> Self {
        self.diameter = dp;
        self
    }

    /// Shows this image, cropped to the circle, instead of the initials.
    pub fn image(mut self, image: Image) -> Self {
        self.image = Some(image);
        self
    }

    /// Like [`image`](Self::image) when there may be none: the initials
    /// are the fallback.
    pub fn image_maybe(mut self, image: Option<Image>) -> Self {
        self.image = image;
        self
    }

    /// Circle colour.
    pub fn background(mut self, color: impl Into<ColorRef>) -> Self {
        self.background = color.into();
        self
    }

    /// Colour of the initials.
    pub fn color(mut self, color: impl Into<ColorRef>) -> Self {
        self.color = color.into();
        self
    }
}

impl<M> From<Avatar> for Element<M> {
    fn from(avatar: Avatar) -> Self {
        if let Some(image) = avatar.image {
            let mut picture = Picture::new(image).radius(avatar.diameter / 2.0);
            picture.layout = Style {
                width: Sizing::Fixed(avatar.diameter),
                height: Sizing::Fixed(avatar.diameter),
                ..avatar.layout
            };
            picture.key = avatar.key;
            picture.transform = avatar.transform;
            return picture.into();
        }
        let mut tile: Container<M> = Container::new(
            Text::new(avatar.initials)
                .size(avatar.diameter * 0.38)
                .line_height(1.2)
                .semibold()
                .color(avatar.color),
        )
        .radius(avatar.diameter)
        .background(avatar.background);
        tile.layout = Style {
            content_align: Alignment::CENTER,
            width: Sizing::Fixed(avatar.diameter),
            height: Sizing::Fixed(avatar.diameter),
            ..avatar.layout
        };
        tile.key = avatar.key;
        tile.transform = avatar.transform;
        tile.into()
    }
}

/// An icon: a [`VectorIcon`] drawn at any size in any colour, or, as a
/// stand-in when there is no artwork, a short label on a tinted tile.
///
/// ```
/// use morningstar_ui::prelude::*;
///
/// const CHECK: VectorIcon = VectorIcon::stroked("M5 12l5 5 9-10");
///
/// // 20 dp, in the theme's text colour.
/// let plain = Icon::vector(CHECK).size(20.0);
/// // In a colour, with the stroke the design gives (view-box units).
/// let tick = Icon::vector(CHECK).size(14.0).color(ThemeColor::Success).stroke_width(2.4);
/// // On a 40 dp rounded tile; the icon takes half the tile.
/// let tile = Icon::vector(CHECK).size(40.0).background(ThemeColor::PrimaryContainer);
/// // The stand-in: a glyph on a tile.
/// let fallback = Icon::new("+");
/// # let _ = (plain, tick, tile, fallback);
/// ```
///
/// A vector icon is rasterised for the exact pixels it covers, so it is
/// sharp at every display scale. Define icons as constants from the SVG the
/// design exports ([`VectorIcon::svg`]) or from path data alone
/// ([`VectorIcon::stroked`], [`VectorIcon::filled`]); only a small subset of
/// SVG is accepted and an icon outside it draws nothing, so test an icon set
/// with [`VectorIcon::check`].
pub struct Icon {
    layout: Style,
    key: Option<u64>,
    transform: Option<Transform>,
    glyph: Cow<'static, str>,
    vector: Option<VectorIcon>,
    svg: Option<SvgPicture>,
    stroke_width: Option<f32>,
    side: f32,
    radius: f32,
    background: Option<ColorRef>,
    color: ColorRef,
}

impl Icon {
    /// An icon slot showing `glyph` (a letter, a digit or a symbol the
    /// app's fonts contain), 40 dp square on a rounded tile. The stand-in
    /// for an icon there is no artwork for.
    pub fn new(glyph: impl Into<Cow<'static, str>>) -> Self {
        Self {
            layout: Style::new(),
            key: None,
            transform: None,
            glyph: glyph.into(),
            vector: None,
            svg: None,
            stroke_width: None,
            side: 40.0,
            radius: 12.0,
            background: Some(ColorRef::Token(ThemeColor::PrimaryContainer)),
            color: ColorRef::Token(ThemeColor::Primary),
        }
    }

    /// A vector icon, 24 dp square, in the theme's text colour, with no
    /// tile behind it.
    pub fn vector(icon: VectorIcon) -> Self {
        Self {
            vector: Some(icon),
            side: 24.0,
            background: None,
            color: ColorRef::Token(ThemeColor::Text),
            ..Self::new("")
        }
    }

    /// An icon from a loaded SVG file ([`SvgPicture`]), 24 dp square, in
    /// the theme's text colour, with no tile behind it. Otherwise as
    /// [`vector`](Self::vector), without its limits on what the file may
    /// contain.
    ///
    /// A file painted in one colour or in `currentColor` throughout takes
    /// [`color`](Self::color) and shares text's draw call. A file with
    /// several colours keeps them, with `currentColor` drawn in
    /// [`color`](Self::color), and is drawn as a picture.
    /// [`stroke_width`](Self::stroke_width) does not apply.
    pub fn svg(picture: &SvgPicture) -> Self {
        Self {
            svg: Some(picture.clone()),
            side: 24.0,
            background: None,
            color: ColorRef::Token(ThemeColor::Text),
            ..Self::new("")
        }
    }

    layout_methods!();

    /// Side length in dp: of the icon itself, or of the tile when there is
    /// one (40 by default for a glyph tile, 24 for a vector icon). On a
    /// tile, a vector icon takes half the side and a glyph scales with it.
    pub fn size(mut self, dp: f32) -> Self {
        self.side = dp;
        self
    }

    /// Corner radius of the tile in dp (12 by default).
    pub fn radius(mut self, dp: f32) -> Self {
        self.radius = dp;
        self
    }

    /// A circular tile.
    pub fn round(self) -> Self {
        let side = self.side;
        self.radius(side)
    }

    /// Tile colour. Gives a vector icon a tile.
    pub fn background(mut self, color: impl Into<ColorRef>) -> Self {
        self.background = Some(color.into());
        self
    }

    /// No tile: the glyph or icon alone.
    pub fn bare(mut self) -> Self {
        self.background = None;
        self
    }

    /// Colour of the icon or glyph.
    pub fn color(mut self, color: impl Into<ColorRef>) -> Self {
        self.color = color.into();
        self
    }

    /// Stroke width of a vector icon, in the icon's own view-box units
    /// (the `stroke-width` of its SVG), replacing the one it was defined
    /// with. The same outline icon can then be drawn light or heavy.
    pub fn stroke_width(mut self, units: f32) -> Self {
        self.stroke_width = Some(units);
        self
    }
}

impl<M> From<Icon> for Element<M> {
    fn from(icon: Icon) -> Self {
        let sized = |layout: Style, side: f32| Style {
            width: Sizing::Fixed(side),
            height: Sizing::Fixed(side),
            ..layout
        };
        let art = match (icon.svg, icon.vector) {
            (Some(svg), _) => Some(Kind::Image(ImageNode {
                source: PictureSource::SvgIcon(svg),
                fit: ImageFit::Contain,
                radius: 0.0,
                tint: icon.color,
            })),
            (None, Some(vector)) => Some(Kind::Icon(IconNode {
                icon: vector,
                color: icon.color,
                stroke_width: icon.stroke_width,
            })),
            (None, None) => None,
        };
        let content: Element<M> = match art {
            Some(kind) => {
                let Some(_) = icon.background else {
                    // Bare: the element is the icon itself.
                    return Self {
                        layout: sized(icon.layout, icon.side),
                        key: icon.key,
                        transform: icon.transform,
                        motion: Motion::PLAIN,
                        kind,
                    };
                };
                Self {
                    layout: sized(Style::new(), icon.side / 2.0),
                    key: None,
                    transform: None,
                    motion: Motion::PLAIN,
                    kind,
                }
            }
            None => Text::new(icon.glyph)
                .size(icon.side * 0.45)
                .line_height(1.2)
                .semibold()
                .color(icon.color)
                .into(),
        };
        let mut tile: Container<M> = Container::new(content).radius(icon.radius);
        if let Some(background) = icon.background {
            tile = tile.background(background);
        }
        tile.layout = Style {
            content_align: Alignment::CENTER,
            ..sized(icon.layout, icon.side)
        };
        tile.key = icon.key;
        tile.transform = icon.transform;
        tile.into()
    }
}

/// A bare [`VectorIcon`], 24 dp. Short for [`Icon::vector`].
pub fn icon(icon: VectorIcon) -> Icon {
    Icon::vector(icon)
}

/// A [`Column`] from a list of children, each converted with `.into()`:
///
/// ```
/// use morningstar_ui::prelude::*;
/// let card: Container<()> = col![
///     text("Salio").caption().muted(),
///     text("TSh 1,250,000").display(),
/// ]
/// .gap(4.0)
/// .padding(16.0);
/// # let _ = card;
/// ```
#[macro_export]
macro_rules! col {
    ($($child:expr),* $(,)?) => {
        $crate::Column::new()$(.push($child))*
    };
}

/// A [`Row`] from a list of children, each converted with `.into()`.
#[macro_export]
macro_rules! row {
    ($($child:expr),* $(,)?) => {
        $crate::Row::new()$(.push($child))*
    };
}

/// A [`Stack`] from a list of children, each converted with `.into()`.
/// Later children are drawn on top.
#[macro_export]
macro_rules! stack {
    ($($child:expr),* $(,)?) => {
        $crate::Stack::new()$(.push($child))*
    };
}
