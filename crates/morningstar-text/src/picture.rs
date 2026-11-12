//! Multi-colour vector pictures: layers of shapes, each with its own fill,
//! stroke, opacity, transform and clip, composited on the CPU into one RGBA
//! image at exactly the pixel size asked for. See ADR 0017.
//!
//! Where a [`VectorIcon`](crate::VectorIcon) is one coverage mask tinted
//! when drawn, a picture is a small illustration: a disc of concentric
//! rings, a quartered circle, a striped square. It is described in Rust,
//! layer by layer, in the vocabulary of the SVG it is transcribed from.

use std::hash::{Hash, Hasher};

use morningstar_core::{Color, Transform};
use zeno::{Cap, Command, Fill, Join, Mask, PathBuilder, Stroke};

use crate::icon::{IconError, path_commands};

/// Longest side, in pixels, a picture is rasterised at: the largest image
/// every target device can hold. A larger picture is rasterised at this
/// size and stretched when drawn.
pub const MAX_PICTURE_PIXELS: u32 = morningstar_core::MAX_IMAGE_DIMENSION;

/// The geometry of a [`Layer`] or of its clip, in the picture's view-box
/// units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// SVG path data: the `d` attribute of a `<path>`.
    Path(&'static str),
    /// `<circle cx cy r>`.
    Circle { cx: f32, cy: f32, r: f32 },
    /// `<ellipse cx cy rx ry>`.
    Ellipse { cx: f32, cy: f32, rx: f32, ry: f32 },
    /// `<rect x y width height rx>`; `radius` is zero for square corners.
    Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radius: f32,
    },
    /// `<line x1 y1 x2 y2>`. Has no interior: only a stroke shows.
    Line { x1: f32, y1: f32, x2: f32, y2: f32 },
}

impl Shape {
    fn commands(&self) -> Result<Vec<Command>, IconError> {
        let mut commands: Vec<Command> = Vec::new();
        match *self {
            Self::Path(data) => return path_commands(data),
            Self::Circle { cx, cy, r } => {
                commands.add_circle((cx, cy), r.max(0.0));
            }
            Self::Ellipse { cx, cy, rx, ry } => {
                commands.add_ellipse((cx, cy), rx.max(0.0), ry.max(0.0));
            }
            Self::Rect {
                x,
                y,
                width,
                height,
                radius,
            } => {
                let (width, height) = (width.max(0.0), height.max(0.0));
                // As in SVG, a radius is at most half the side.
                let radius = radius.min(width / 2.0).min(height / 2.0);
                if radius > 0.0 {
                    commands.add_round_rect((x, y), width, height, radius, radius);
                } else {
                    commands.add_rect((x, y), width, height);
                }
            }
            Self::Line { x1, y1, x2, y2 } => {
                commands.move_to((x1, y1));
                commands.line_to((x2, y2));
            }
        }
        Ok(commands)
    }

    fn hash_into(&self, state: &mut impl Hasher) {
        match *self {
            Self::Path(data) => {
                0u8.hash(state);
                data.hash(state);
            }
            Self::Circle { cx, cy, r } => hash_numbers(state, 1, &[cx, cy, r]),
            Self::Ellipse { cx, cy, rx, ry } => hash_numbers(state, 2, &[cx, cy, rx, ry]),
            Self::Rect {
                x,
                y,
                width,
                height,
                radius,
            } => hash_numbers(state, 3, &[x, y, width, height, radius]),
            Self::Line { x1, y1, x2, y2 } => hash_numbers(state, 4, &[x1, y1, x2, y2]),
        }
    }
}

/// Hashes a tag and the bits of some numbers.
fn hash_numbers(state: &mut impl Hasher, tag: u8, values: &[f32]) {
    tag.hash(state);
    for value in values {
        value.to_bits().hash(state);
    }
}

fn hash_color(state: &mut impl Hasher, color: Option<Color>) {
    match color {
        Some(color) => hash_numbers(state, 1, &[color.r, color.g, color.b, color.a]),
        None => state.write_u8(0),
    }
}

/// A layer's transform as it was asked for. Kept in this form, and turned
/// into a matrix when the picture is rasterised, so that a rotation can be
/// written in a constant (sines and cosines are not `const`).
#[derive(Clone, Copy, Debug, PartialEq)]
enum LayerTransform {
    Matrix(Transform),
    Rotate { degrees: f32, cx: f32, cy: f32 },
}

impl LayerTransform {
    fn matrix(self) -> Transform {
        match self {
            Self::Matrix(matrix) => matrix,
            Self::Rotate { degrees, cx, cy } => {
                Transform::rotate(degrees).about(morningstar_core::Point::new(cx, cy))
            }
        }
    }
}

/// How the ends of an open stroke are finished: SVG `stroke-linecap`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LineCap {
    /// Cut off square at the end point. The SVG default.
    #[default]
    Butt,
    /// A half circle around the end point.
    Round,
    /// Cut off square, half the stroke width beyond the end point.
    Square,
}

/// How the corners of a stroke are finished: SVG `stroke-linejoin`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LineJoin {
    /// A sharp corner. The SVG default.
    #[default]
    Miter,
    /// A rounded corner.
    Round,
    /// A corner cut off flat.
    Bevel,
}

/// One shape of a [`VectorPicture`] and how it is painted.
///
/// Build one from a shape and chain what the SVG element says:
///
/// ```
/// use morningstar_core::Color;
/// use morningstar_text::{Layer, LineCap};
///
/// const GOLD: Color = Color::rgb(0.92, 0.55, 0.06);
/// // <circle cx="140" cy="140" r="122" fill="#000" stroke="gold" stroke-width="1.5"/>
/// const DISC: Layer = Layer::circle(140.0, 140.0, 122.0)
///     .fill(Color::BLACK)
///     .stroke(GOLD, 1.5);
/// // <path d="M4 22v-4a14 14 0 0 1 28 0v4" fill="none" stroke="#000" stroke-width="3"
/// //       stroke-linecap="round" transform="translate(122 122)"/>
/// const BAND: Layer = Layer::path("M4 22v-4a14 14 0 0 1 28 0v4")
///     .stroke(Color::BLACK, 3.0)
///     .cap(LineCap::Round)
///     .translate(122.0, 122.0);
/// # let _ = (DISC, BAND);
/// ```
///
/// A layer with neither fill nor stroke paints nothing. With both, the
/// stroke is painted over the fill, centred on the outline, as in SVG.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layer {
    shape: Shape,
    fill: Option<Color>,
    even_odd: bool,
    stroke: Option<Color>,
    stroke_width: f32,
    cap: LineCap,
    join: LineJoin,
    opacity: f32,
    transform: Option<LayerTransform>,
    clip: Option<Shape>,
}

impl Layer {
    /// A layer of any [`Shape`], not yet painted.
    pub const fn new(shape: Shape) -> Self {
        Self {
            shape,
            fill: None,
            even_odd: false,
            stroke: None,
            stroke_width: 1.0,
            cap: LineCap::Butt,
            join: LineJoin::Miter,
            opacity: 1.0,
            transform: None,
            clip: None,
        }
    }

    /// A layer from SVG path data (`<path d="...">`).
    pub const fn path(data: &'static str) -> Self {
        Self::new(Shape::Path(data))
    }

    /// `<circle cx cy r>`.
    pub const fn circle(cx: f32, cy: f32, r: f32) -> Self {
        Self::new(Shape::Circle { cx, cy, r })
    }

    /// `<ellipse cx cy rx ry>`.
    pub const fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> Self {
        Self::new(Shape::Ellipse { cx, cy, rx, ry })
    }

    /// `<rect x y width height>`.
    pub const fn rect(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self::rounded_rect(x, y, width, height, 0.0)
    }

    /// `<rect x y width height rx>`.
    pub const fn rounded_rect(x: f32, y: f32, width: f32, height: f32, radius: f32) -> Self {
        Self::new(Shape::Rect {
            x,
            y,
            width,
            height,
            radius,
        })
    }

    /// `<line x1 y1 x2 y2>`.
    pub const fn line(x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        Self::new(Shape::Line { x1, y1, x2, y2 })
    }

    /// Fills the shape (`fill="..."`). For `fill-opacity`, give a colour
    /// with that alpha.
    pub const fn fill(mut self, color: Color) -> Self {
        self.fill = Some(color);
        self
    }

    /// Fills by the even-odd rule instead of the non-zero one
    /// (`fill-rule="evenodd"`): overlapping subpaths cut holes.
    pub const fn even_odd(mut self) -> Self {
        self.even_odd = true;
        self
    }

    /// Strokes the outline, `width` view-box units wide, centred on it
    /// (`stroke="..." stroke-width="..."`). For `stroke-opacity`, give a
    /// colour with that alpha.
    pub const fn stroke(mut self, color: Color, width: f32) -> Self {
        self.stroke = Some(color);
        self.stroke_width = width;
        self
    }

    /// `stroke-linecap`.
    pub const fn cap(mut self, cap: LineCap) -> Self {
        self.cap = cap;
        self
    }

    /// `stroke-linejoin`.
    pub const fn join(mut self, join: LineJoin) -> Self {
        self.join = join;
        self
    }

    /// Round caps and joins, as most outline artwork has.
    pub const fn round(self) -> Self {
        self.cap(LineCap::Round).join(LineJoin::Round)
    }

    /// Fades the whole layer (`opacity="..."`): fill and stroke are
    /// composited together first, so a stroke does not show its fill
    /// through where they overlap.
    pub const fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }

    /// Draws the shape through a transform, in view-box units
    /// (`transform="..."`). Replaces any transform set before. The stroke
    /// width scales with it. The [`clip`](Self::clip) is not transformed.
    pub const fn transform(mut self, transform: Transform) -> Self {
        self.transform = Some(LayerTransform::Matrix(transform));
        self
    }

    /// `transform="translate(x y)"`.
    pub const fn translate(self, x: f32, y: f32) -> Self {
        self.transform(Transform::translate(x, y))
    }

    /// `transform="rotate(degrees cx cy)"`: clockwise about `(cx, cy)`.
    pub const fn rotate(mut self, degrees: f32, cx: f32, cy: f32) -> Self {
        self.transform = Some(LayerTransform::Rotate { degrees, cx, cy });
        self
    }

    /// Shows the layer only inside `shape` (`clip-path`), which is in
    /// view-box units and is not moved by the layer's own transform: a
    /// rotated band cut to a circle. The clip's edge is anti-aliased.
    pub const fn clip(mut self, shape: Shape) -> Self {
        self.clip = Some(shape);
        self
    }

    /// [`clip`](Self::clip) to a circle.
    pub const fn clip_circle(self, cx: f32, cy: f32, r: f32) -> Self {
        self.clip(Shape::Circle { cx, cy, r })
    }

    /// [`clip`](Self::clip) to a rectangle.
    pub const fn clip_rect(self, x: f32, y: f32, width: f32, height: f32) -> Self {
        self.clip(Shape::Rect {
            x,
            y,
            width,
            height,
            radius: 0.0,
        })
    }

    fn hash_into(&self, state: &mut impl Hasher) {
        self.shape.hash_into(state);
        hash_color(state, self.fill);
        self.even_odd.hash(state);
        hash_color(state, self.stroke);
        self.stroke_width.to_bits().hash(state);
        self.cap.hash(state);
        self.join.hash(state);
        self.opacity.to_bits().hash(state);
        match self.transform {
            Some(LayerTransform::Matrix(t)) => {
                hash_numbers(state, 1, &[t.a, t.b, t.c, t.d, t.tx, t.ty]);
            }
            Some(LayerTransform::Rotate { degrees, cx, cy }) => {
                hash_numbers(state, 2, &[degrees, cx, cy]);
            }
            None => state.write_u8(0),
        }
        match self.clip {
            Some(clip) => clip.hash_into(state),
            None => state.write_u8(9),
        }
    }
}

/// A multi-colour vector picture: a view box and layers painted in order,
/// later ones over earlier ones.
///
/// The value is `Copy` and names `'static` data, so pictures are constants,
/// and every method of [`Layer`] is `const`:
///
/// ```
/// use morningstar_core::Color;
/// use morningstar_text::{Layer, VectorPicture};
///
/// const INK: Color = Color::rgb(0.0, 0.0, 0.0);
/// const CREAM: Color = Color::rgb(0.8, 0.75, 0.6);
///
/// // <svg viewBox="0 0 100 100">
/// //   <circle cx="50" cy="50" r="48" fill="#000"/>
/// //   <circle cx="50" cy="50" r="20" fill="cream" stroke="#000" stroke-width="3"/>
/// // </svg>
/// pub const DISC: VectorPicture = VectorPicture::new(
///     100.0,
///     100.0,
///     &[
///         Layer::circle(50.0, 50.0, 48.0).fill(INK),
///         Layer::circle(50.0, 50.0, 20.0).fill(CREAM).stroke(INK, 3.0),
///     ],
/// );
/// assert!(DISC.check().is_ok());
/// ```
///
/// A picture is rasterised for the exact pixels it covers, so it is sharp at
/// every display scale; its layers are composited as sRGB-encoded values
/// with premultiplied alpha, as the renderer and browsers blend (ADR 0023). Whatever falls outside the
/// view box is cut off.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VectorPicture {
    view_box: [f32; 4],
    layers: &'static [Layer],
}

impl VectorPicture {
    /// A picture whose view box is `0 0 width height`.
    pub const fn new(width: f32, height: f32, layers: &'static [Layer]) -> Self {
        Self::with_view_box(0.0, 0.0, width, height, layers)
    }

    /// A picture with the view box `x y width height` of an SVG.
    pub const fn with_view_box(
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        layers: &'static [Layer],
    ) -> Self {
        Self {
            view_box: [x, y, width, height],
            layers,
        }
    }

    /// Width of the view box.
    pub const fn width(&self) -> f32 {
        self.view_box[2]
    }

    /// Height of the view box.
    pub const fn height(&self) -> f32 {
        self.view_box[3]
    }

    /// The layers, in paint order.
    pub const fn layers(&self) -> &'static [Layer] {
        self.layers
    }

    /// Checks that every layer's path data parses and that the view box
    /// has an area. Worth a unit test over an application's pictures: a
    /// layer that does not parse is otherwise only noticed as a gap.
    pub fn check(&self) -> Result<(), IconError> {
        if !(self.view_box[2] > 0.0 && self.view_box[3] > 0.0) {
            return Err(IconError::MissingViewBox);
        }
        for layer in self.layers {
            layer.shape.commands()?;
            if let Some(clip) = layer.clip {
                clip.commands()?;
            }
        }
        Ok(())
    }

    /// A number that is the same for pictures with the same content and,
    /// for all practical purposes, different otherwise: the key a
    /// rasterisation is cached under.
    pub fn content_hash(&self) -> u64 {
        // Fixed keys: the hash must not change between runs of a test.
        let mut state = std::hash::DefaultHasher::new();
        for value in self.view_box {
            value.to_bits().hash(&mut state);
        }
        self.layers.len().hash(&mut state);
        for layer in self.layers {
            layer.hash_into(&mut state);
        }
        state.finish()
    }

    /// Rasterises the picture into `width * height * 4` bytes of RGBA, row
    /// by row from the top, in the form the image store keeps
    /// (`ImageStore::add_premultiplied_rgba8_vec`): sRGB-encoded colour
    /// premultiplied by alpha.
    ///
    /// The view box is scaled uniformly to fit and centred, as SVG does by
    /// default. A layer whose path data does not parse is skipped.
    pub fn rasterise(&self, width: u32, height: u32) -> Vec<u8> {
        let mut canvas = Canvas::new(width, height);
        let [box_x, box_y, box_width, box_height] = self.view_box;
        if canvas.is_empty() || !(box_width > 0.0 && box_height > 0.0) {
            return canvas.finish();
        }
        let scale = (width as f32 / box_width).min(height as f32 / box_height);
        let to_pixels = Transform::translate(-box_x, -box_y)
            .then(Transform::scale(scale, scale))
            .then(Transform::translate(
                (width as f32 - box_width * scale) / 2.0,
                (height as f32 - box_height * scale) / 2.0,
            ));
        // Nothing outside the view box shows, even where the box does not
        // fill the image.
        let view_box = Shape::Rect {
            x: box_x,
            y: box_y,
            width: box_width,
            height: box_height,
            radius: 0.0,
        };
        let letterboxed = (box_width * scale - width as f32).abs() > 0.01
            || (box_height * scale - height as f32).abs() > 0.01;

        for layer in self.layers {
            let Ok(commands) = layer.shape.commands() else {
                continue;
            };
            let clip = match layer.clip {
                Some(clip) => match clip.commands() {
                    Ok(commands) => Some(commands),
                    Err(_) => continue,
                },
                None if letterboxed => view_box.commands().ok(),
                None => None,
            };
            let transform = match layer.transform {
                Some(own) => own.matrix().then(to_pixels),
                None => to_pixels,
            };
            let clip_part = clip.as_deref().map(|commands| {
                [ClipPart {
                    commands,
                    transform: to_pixels,
                    even_odd: false,
                }]
            });
            let clip_parts = clip_part.as_ref().map(|part| [&part[..]]);
            canvas.paint(&Paint {
                commands: &commands,
                transform,
                fill: layer.fill.map(Ink::color),
                even_odd: layer.even_odd,
                stroke: layer.stroke.map(|color| StrokePaint {
                    ink: Ink::color(color),
                    width: layer.stroke_width,
                    cap: layer.cap,
                    join: layer.join,
                    miter_limit: DEFAULT_MITER_LIMIT,
                    dashes: &[],
                    dash_offset: 0.0,
                }),
                opacity: layer.opacity,
                clips: clip_parts.as_ref().map_or(&[], |parts| &parts[..]),
            });
        }
        canvas.finish()
    }
}

/// SVG's initial `stroke-miterlimit`, which is also `zeno`'s default.
pub(crate) const DEFAULT_MITER_LIMIT: f32 = 4.0;

/// What a fill or a stroke is painted with.
#[derive(Clone, Copy)]
pub(crate) enum Ink<'a> {
    /// One colour: sRGB-encoded and premultiplied by its alpha.
    Solid([f32; 4]),
    /// A gradient, evaluated at each pixel's centre.
    Gradient(&'a GradientInk),
}

impl Ink<'_> {
    pub(crate) fn color(color: Color) -> Self {
        Self::Solid(premultiplied(color))
    }

    fn paints(&self) -> bool {
        match self {
            Self::Solid(color) => color[3] > 0.0,
            Self::Gradient(gradient) => !gradient.stops.is_empty(),
        }
    }

    /// The premultiplied colour at the pixel whose top-left is `(x, y)`.
    #[inline]
    fn at(&self, x: u32, y: u32) -> [f32; 4] {
        match self {
            Self::Solid(color) => *color,
            Self::Gradient(gradient) => gradient.at(x as f32 + 0.5, y as f32 + 0.5),
        }
    }
}

/// How a gradient continues outside its vector: SVG `spreadMethod`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Spread {
    Pad,
    Reflect,
    Repeat,
}

/// The geometry of a gradient, in its own coordinate space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum GradientShape {
    /// From `(x1, y1)` to `(x2, y2)`.
    Linear { from: (f32, f32), to: (f32, f32) },
    /// A circle of `radius` about `center`, with colour 0 at `focus`.
    Radial {
        center: (f32, f32),
        radius: f32,
        focus: (f32, f32),
    },
}

/// A gradient ready to be evaluated per pixel.
pub(crate) struct GradientInk {
    pub shape: GradientShape,
    /// From pixels to the gradient's own space.
    pub from_pixels: Transform,
    pub spread: Spread,
    /// Offsets, never decreasing, in `0..=1`, with sRGB-encoded colours
    /// and straight alpha: SVG interpolates the channels and the alpha
    /// separately, unlike CSS gradients. Empty paints nothing.
    pub stops: Vec<(f32, [f32; 4])>,
}

impl GradientInk {
    /// Position along the gradient of a point in pixels, before spreading.
    fn parameter(&self, x: f32, y: f32) -> f32 {
        let point = self.from_pixels.apply(morningstar_core::Point::new(x, y));
        match self.shape {
            GradientShape::Linear { from, to } => {
                let (dx, dy) = (to.0 - from.0, to.1 - from.1);
                let length = dx * dx + dy * dy;
                if length > 0.0 {
                    ((point.x - from.0) * dx + (point.y - from.1) * dy) / length
                } else {
                    // A zero-length vector paints the last stop.
                    1.0
                }
            }
            GradientShape::Radial {
                center,
                radius,
                focus,
            } => {
                if radius <= 0.0 || radius.is_nan() {
                    return 1.0;
                }
                // The circle through the point, of radius `t * radius`,
                // centred `t` of the way from the focus to the centre:
                // |d - t * cf| = t * radius, the larger root.
                let (dx, dy) = (point.x - focus.0, point.y - focus.1);
                let (cx, cy) = (center.0 - focus.0, center.1 - focus.1);
                let a = radius * radius - (cx * cx + cy * cy);
                let b = dx * cx + dy * cy;
                let c = dx * dx + dy * dy;
                if a <= 0.0 {
                    return 1.0;
                }
                (-b + (b * b + a * c).max(0.0).sqrt()).max(0.0) / a
            }
        }
    }

    fn at(&self, x: f32, y: f32) -> [f32; 4] {
        let Some(&(first_offset, first)) = self.stops.first() else {
            return [0.0; 4];
        };
        let mut t = self.parameter(x, y);
        if t.is_nan() {
            t = 0.0;
        }
        let t = match self.spread {
            Spread::Pad => t.clamp(0.0, 1.0),
            Spread::Repeat => t - t.floor(),
            Spread::Reflect => {
                let folded = t.rem_euclid(2.0);
                if folded > 1.0 { 2.0 - folded } else { folded }
            }
        };
        let mut color = first;
        if t > first_offset {
            let mut below = (first_offset, first);
            color = self.stops[self.stops.len() - 1].1;
            for &(offset, stop) in &self.stops[1..] {
                if t < offset {
                    let amount = (t - below.0) / (offset - below.0);
                    for channel in 0..4 {
                        color[channel] =
                            below.1[channel] + (stop[channel] - below.1[channel]) * amount;
                    }
                    break;
                }
                below = (offset, stop);
            }
        }
        [
            color[0] * color[3],
            color[1] * color[3],
            color[2] * color[3],
            color[3],
        ]
    }
}

pub(crate) struct StrokePaint<'a> {
    pub ink: Ink<'a>,
    /// In the units of the path, before the transform.
    pub width: f32,
    pub cap: LineCap,
    pub join: LineJoin,
    pub miter_limit: f32,
    /// Lengths of dashes and gaps in the units of the path; empty for a
    /// solid line. Never all zero, never negative, and of even length.
    pub dashes: &'a [f32],
    pub dash_offset: f32,
}

/// One shape of a clip, with its own transform to pixels.
#[derive(Clone, Copy)]
pub(crate) struct ClipPart<'a> {
    pub commands: &'a [Command],
    pub transform: Transform,
    pub even_odd: bool,
}

/// One painting step: a path, how it is filled and stroked, and what cuts
/// it.
pub(crate) struct Paint<'a> {
    pub commands: &'a [Command],
    /// From the path's units to pixels.
    pub transform: Transform,
    pub fill: Option<Ink<'a>>,
    pub even_odd: bool,
    pub stroke: Option<StrokePaint<'a>>,
    pub opacity: f32,
    /// Clips, all of which apply: each is the union of its parts. No clips
    /// leaves the step uncut; a clip with no parts cuts everything away.
    pub clips: &'a [&'a [ClipPart<'a>]],
}

/// An image being composited: sRGB-encoded RGBA premultiplied by alpha, in
/// `f32`. The encoded values are what is blended (ADR 0023).
pub(crate) struct Canvas {
    width: u32,
    height: u32,
    pixels: Vec<[f32; 4]>,
    /// Coverage of the fill, the stroke and the clip of the step being
    /// painted. Kept between steps to reuse the allocations.
    fill: Vec<u8>,
    stroke: Vec<u8>,
    clip: Vec<u8>,
    /// Further coverage buffers, for clips made of several shapes.
    scratch: Vec<u8>,
    union: Vec<u8>,
}

fn to_zeno(transform: Transform) -> zeno::Transform {
    // zeno maps x to `x * xx + y * yx + x0`: its `xy` is our `b`.
    zeno::Transform::new(
        transform.a,
        transform.b,
        transform.c,
        transform.d,
        transform.tx,
        transform.ty,
    )
}

/// How far outside the image, in pixels, geometry may reach and still be
/// handed to the rasteriser as it is. `zeno` rasterises in 24.8 fixed
/// point and multiplies the spans of a line by 256 again, so a line may
/// span some 16000 pixels in all; an image is at most 2048.
const RASTER_MARGIN: f32 = 1024.0;

/// True if every point of the path, and of its stroke, lands within
/// [`RASTER_MARGIN`] of the image.
fn in_raster_range(
    commands: &[Command],
    transform: Transform,
    style: &zeno::Style<'_>,
    width: u32,
    height: u32,
) -> bool {
    let mut margin = RASTER_MARGIN;
    if let zeno::Style::Stroke(stroke) = style {
        // How far a stroke reaches from its path, at most: half the
        // width times the mitre limit, stretched by the transform.
        let stretch = (transform.a * transform.a
            + transform.b * transform.b
            + transform.c * transform.c
            + transform.d * transform.d)
            .sqrt();
        margin -= stroke.width.abs() * stroke.miter_limit.max(1.5) * stretch;
        if margin <= 0.0 || margin.is_nan() {
            return false;
        }
    }
    let (low, high_x, high_y) = (-margin, width as f32 + margin, height as f32 + margin);
    let inside = |point: &zeno::Point| {
        let x = point.x * transform.a + point.y * transform.c + transform.tx;
        let y = point.x * transform.b + point.y * transform.d + transform.ty;
        // Written so that NaN fails.
        x >= low && x <= high_x && y >= low && y <= high_y
    };
    commands.iter().all(|command| match command {
        Command::MoveTo(p) | Command::LineTo(p) => inside(p),
        Command::QuadTo(c, p) => inside(c) && inside(p),
        Command::CurveTo(a, b, p) => inside(a) && inside(b) && inside(p),
        Command::Close => true,
    })
}

/// Most points an outline is flattened into before curves are cut short.
const MAX_OUTLINE_POINTS: usize = 1 << 20;

/// The outline of a styled path in pixels, as polygons cut to a box
/// [`RASTER_MARGIN`] around the image, and the rule to fill them by.
fn clipped_outline(
    commands: &[Command],
    transform: Transform,
    style: zeno::Style<'_>,
    width: u32,
    height: u32,
) -> (Vec<Command>, Fill) {
    let mut outline: Vec<Command> = Vec::new();
    let rule = zeno::apply(commands, style, Some(to_zeno(transform)), &mut outline);
    let bounds = [
        -RASTER_MARGIN,
        -RASTER_MARGIN,
        width as f32 + RASTER_MARGIN,
        height as f32 + RASTER_MARGIN,
    ];

    // Flatten into polygons, leaving alone what lies wholly to one side
    // of the box.
    type P = (f32, f32);
    fn outside(points: &[P], bounds: &[f32; 4]) -> bool {
        points.iter().all(|p| p.0 < bounds[0])
            || points.iter().all(|p| p.1 < bounds[1])
            || points.iter().all(|p| p.0 > bounds[2])
            || points.iter().all(|p| p.1 > bounds[3])
    }
    fn middle(a: P, b: P) -> P {
        ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0)
    }
    fn cubic(points: [P; 4], depth: u32, bounds: &[f32; 4], out: &mut Vec<P>) {
        let [p0, p1, p2, p3] = points;
        // Flat enough when the control points are within a twentieth of
        // a pixel of the chord's thirds.
        let flat = |a: P, b: P| (a.0 - b.0).abs() + (a.1 - b.1).abs() < 0.05;
        let third = |t: f32| (p0.0 + (p3.0 - p0.0) * t, p0.1 + (p3.1 - p0.1) * t);
        if depth >= 24
            || out.len() >= MAX_OUTLINE_POINTS
            || outside(&points, bounds)
            || (flat(p1, third(1.0 / 3.0)) && flat(p2, third(2.0 / 3.0)))
        {
            out.push(p3);
            return;
        }
        let (a, b, c) = (middle(p0, p1), middle(p1, p2), middle(p2, p3));
        let (d, e) = (middle(a, b), middle(b, c));
        let f = middle(d, e);
        cubic([p0, a, d, f], depth + 1, bounds, out);
        cubic([f, e, c, p3], depth + 1, bounds, out);
    }
    let finite = |p: zeno::Point| -> P {
        // Far beyond the box either way; the cut brings it back.
        let clean = |value: f32| {
            if value.is_nan() {
                0.0
            } else {
                value.clamp(-1.0e9, 1.0e9)
            }
        };
        (clean(p.x), clean(p.y))
    };
    let mut polygons: Vec<Vec<P>> = Vec::new();
    let mut current: Vec<P> = Vec::new();
    for command in &outline {
        match *command {
            Command::MoveTo(p) => {
                if current.len() > 2 {
                    polygons.push(std::mem::take(&mut current));
                }
                current.clear();
                current.push(finite(p));
            }
            Command::LineTo(p) => current.push(finite(p)),
            Command::QuadTo(c, p) => {
                let Some(&from) = current.last() else {
                    continue;
                };
                let (c, p) = (finite(c), finite(p));
                let a = (
                    from.0 + (c.0 - from.0) * 2.0 / 3.0,
                    from.1 + (c.1 - from.1) * 2.0 / 3.0,
                );
                let b = (p.0 + (c.0 - p.0) * 2.0 / 3.0, p.1 + (c.1 - p.1) * 2.0 / 3.0);
                cubic([from, a, b, p], 0, &bounds, &mut current);
            }
            Command::CurveTo(a, b, p) => {
                let Some(&from) = current.last() else {
                    continue;
                };
                cubic(
                    [from, finite(a), finite(b), finite(p)],
                    0,
                    &bounds,
                    &mut current,
                );
            }
            Command::Close => {
                // The next subpath starts where this one did.
                let start = current.first().copied();
                if current.len() > 2 {
                    polygons.push(std::mem::take(&mut current));
                }
                current.clear();
                current.extend(start);
            }
        }
    }
    if current.len() > 2 {
        polygons.push(current);
    }

    // Cut each polygon by the four sides in turn. The pieces that run
    // along a side are outside the image, where they change nothing.
    let mut result: Vec<Command> = Vec::new();
    let mut scratch: Vec<P> = Vec::new();
    for mut polygon in polygons {
        for side in 0..4 {
            let keep = |p: P| match side {
                0 => p.0 >= bounds[0],
                1 => p.1 >= bounds[1],
                2 => p.0 <= bounds[2],
                _ => p.1 <= bounds[3],
            };
            let cross = |a: P, b: P| -> P {
                if side % 2 == 0 {
                    let x = bounds[side];
                    (x, a.1 + (b.1 - a.1) * ((x - a.0) / (b.0 - a.0)))
                } else {
                    let y = bounds[side];
                    (a.0 + (b.0 - a.0) * ((y - a.1) / (b.1 - a.1)), y)
                }
            };
            scratch.clear();
            let Some(&last) = polygon.last() else { break };
            let mut previous = last;
            for &point in &polygon {
                match (keep(previous), keep(point)) {
                    (true, true) => scratch.push(point),
                    (true, false) => scratch.push(cross(previous, point)),
                    (false, true) => {
                        scratch.push(cross(previous, point));
                        scratch.push(point);
                    }
                    (false, false) => {}
                }
                previous = point;
            }
            std::mem::swap(&mut polygon, &mut scratch);
        }
        // The crossing points are computed, so hold them to the box.
        let mut points = polygon.iter().map(|p| {
            (
                p.0.clamp(bounds[0], bounds[2]),
                p.1.clamp(bounds[1], bounds[3]),
            )
        });
        if polygon.len() > 2 {
            if let Some(first) = points.next() {
                result.move_to(first);
                for point in points {
                    result.line_to(point);
                }
                result.close();
            }
        }
    }
    (result, rule)
}

fn premultiplied(color: Color) -> [f32; 4] {
    let [r, g, b, alpha] = color.srgb();
    [r * alpha, g * alpha, b * alpha, alpha]
}

impl Canvas {
    pub(crate) fn new(width: u32, height: u32) -> Self {
        let length = width as usize * height as usize;
        Self {
            width,
            height,
            pixels: vec![[0.0; 4]; length],
            fill: Vec::new(),
            stroke: Vec::new(),
            clip: Vec::new(),
            scratch: Vec::new(),
            union: Vec::new(),
        }
    }

    fn is_empty(&self) -> bool {
        self.pixels.is_empty()
    }

    fn mask(
        target: &mut Vec<u8>,
        (width, height): (u32, u32),
        commands: &[Command],
        transform: Transform,
        style: zeno::Style<'_>,
    ) {
        target.clear();
        target.resize(width as usize * height as usize, 0);
        if in_raster_range(commands, transform, &style, width, height) {
            Mask::new(commands)
                .size(width, height)
                .transform(Some(to_zeno(transform)))
                .style(style)
                .render_into(target, None);
            return;
        }
        // Geometry far outside the image: the rasteriser works in 32-bit
        // fixed point and cannot take it. Outline it here, cut the
        // outline to a box around the image, and fill that.
        let (outline, rule) = clipped_outline(commands, transform, style, width, height);
        if !outline.is_empty() {
            Mask::new(&outline)
                .size(width, height)
                .style(rule)
                .render_into(target, None);
        }
    }

    /// Renders the coverage of `clips` into `self.clip`. False if a part's
    /// transform cannot be drawn through, or if nothing is left.
    fn clip_mask(&mut self, clips: &[&[ClipPart<'_>]]) -> bool {
        let size = (self.width, self.height);
        let rule = |part: &ClipPart<'_>| {
            if part.even_odd {
                Fill::EvenOdd
            } else {
                Fill::NonZero
            }
        };
        for (number, parts) in clips.iter().enumerate() {
            let Some((first, rest)) = parts.split_first() else {
                return false;
            };
            if parts.iter().any(|part| !part.transform.is_finite()) {
                return false;
            }
            // The first clip goes straight into the buffer, each further
            // one into `union` and is then multiplied in.
            let target = if number == 0 {
                &mut self.clip
            } else {
                &mut self.union
            };
            Self::mask(
                target,
                size,
                first.commands,
                first.transform,
                rule(first).into(),
            );
            for part in rest {
                Self::mask(
                    &mut self.scratch,
                    size,
                    part.commands,
                    part.transform,
                    rule(part).into(),
                );
                for (below, above) in target.iter_mut().zip(&self.scratch) {
                    let (b, a) = (u32::from(*below), u32::from(*above));
                    *below = (a + b - (a * b + 127) / 255) as u8;
                }
            }
            if number > 0 {
                for (all, one) in self.clip.iter_mut().zip(&self.union) {
                    *all = ((u32::from(*all) * u32::from(*one) + 127) / 255) as u8;
                }
            }
        }
        true
    }

    /// Composites another canvas of the same size over this one, as a
    /// group: faded as a whole by `opacity` and cut by `clips`.
    pub(crate) fn blend(&mut self, group: &Canvas, opacity: f32, clips: &[&[ClipPart<'_>]]) {
        let opacity = if opacity > 0.0 {
            opacity.min(1.0)
        } else {
            return;
        };
        if self.is_empty() || group.pixels.len() != self.pixels.len() {
            return;
        }
        let clipped = !clips.is_empty();
        if clipped && !self.clip_mask(clips) {
            return;
        }
        for (index, (pixel, source)) in self.pixels.iter_mut().zip(&group.pixels).enumerate() {
            if source[3] <= 0.0 {
                continue;
            }
            let mut visible = opacity;
            if clipped {
                if self.clip[index] == 0 {
                    continue;
                }
                visible *= f32::from(self.clip[index]) / 255.0;
            }
            let keep = 1.0 - source[3] * visible;
            for channel in 0..4 {
                pixel[channel] = source[channel] * visible + pixel[channel] * keep;
            }
        }
    }

    /// Paints one step over what is there.
    pub(crate) fn paint(&mut self, paint: &Paint<'_>) {
        // Written so that NaN counts as invisible.
        let opacity = if paint.opacity > 0.0 {
            paint.opacity.min(1.0)
        } else {
            return;
        };
        if self.is_empty() || !paint.transform.is_finite() {
            return;
        }
        let size = (self.width, self.height);
        let fill = paint.fill.filter(Ink::paints);
        let stroke = paint
            .stroke
            .as_ref()
            .filter(|stroke| stroke.width > 0.0 && stroke.width.is_finite())
            .filter(|stroke| stroke.ink.paints());
        if fill.is_none() && stroke.is_none() {
            return;
        }

        if fill.is_some() {
            let rule = if paint.even_odd {
                Fill::EvenOdd
            } else {
                Fill::NonZero
            };
            Self::mask(
                &mut self.fill,
                size,
                paint.commands,
                paint.transform,
                rule.into(),
            );
        }
        if let Some(stroke) = stroke {
            // The width is in the path's units; `scale` (on by default)
            // applies the transform to it.
            let mut style = Stroke::new(stroke.width);
            style
                .cap(match stroke.cap {
                    LineCap::Butt => Cap::Butt,
                    LineCap::Round => Cap::Round,
                    LineCap::Square => Cap::Square,
                })
                .join(match stroke.join {
                    LineJoin::Miter => Join::Miter,
                    LineJoin::Round => Join::Round,
                    LineJoin::Bevel => Join::Bevel,
                });
            if stroke.miter_limit != DEFAULT_MITER_LIMIT {
                style.miter_limit(stroke.miter_limit);
            }
            if !stroke.dashes.is_empty() {
                style.dash(stroke.dashes, stroke.dash_offset);
            }
            Self::mask(
                &mut self.stroke,
                size,
                paint.commands,
                paint.transform,
                style.into(),
            );
        }
        let clipped = !paint.clips.is_empty();
        if clipped && !self.clip_mask(paint.clips) {
            return;
        }

        let width = self.width.max(1);
        let coverage = |mask: &[u8], index: usize| f32::from(mask[index]) / 255.0;
        for (index, pixel) in self.pixels.iter_mut().enumerate() {
            let mut visible = opacity;
            if clipped {
                if self.clip[index] == 0 {
                    continue;
                }
                visible *= coverage(&self.clip, index);
            }
            let (x, y) = (index as u32 % width, index as u32 / width);
            // The layer's own colour here: stroke over fill.
            let mut layer = [0.0f32; 4];
            if let Some(ink) = &fill {
                let amount = self.fill[index];
                if amount > 0 {
                    let amount = f32::from(amount) / 255.0;
                    layer = ink.at(x, y).map(|channel| channel * amount);
                }
            }
            if let Some(stroke) = stroke {
                let amount = self.stroke[index];
                if amount > 0 {
                    let amount = f32::from(amount) / 255.0;
                    let color = stroke.ink.at(x, y);
                    let keep = 1.0 - color[3] * amount;
                    for channel in 0..4 {
                        layer[channel] = color[channel] * amount + layer[channel] * keep;
                    }
                }
            }
            if layer[3] <= 0.0 {
                continue;
            }
            // Source over destination, premultiplied.
            let keep = 1.0 - layer[3] * visible;
            for channel in 0..4 {
                pixel[channel] = layer[channel] * visible + pixel[channel] * keep;
            }
        }
    }

    /// The alpha of every pixel, as coverage bytes.
    pub(crate) fn alpha(&self) -> Vec<u8> {
        self.pixels
            .iter()
            .map(|pixel| (pixel[3].clamp(0.0, 1.0) * 255.0 + 0.5) as u8)
            .collect()
    }

    /// The image as stored bytes: sRGB-encoded premultiplied RGBA.
    pub(crate) fn finish(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.pixels.len() * 4);
        for [r, g, b, a] in self.pixels {
            if a <= 0.0 {
                bytes.extend_from_slice(&[0; 4]);
                continue;
            }
            // The float-to-integer cast saturates.
            bytes
                .extend_from_slice(&[r, g, b, a].map(|value| (value.min(1.0) * 255.0 + 0.5) as u8));
        }
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Color = Color::rgb(1.0, 0.0, 0.0);
    const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);

    fn at(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let start = ((y * width + x) * 4) as usize;
        pixels[start..start + 4].try_into().unwrap()
    }

    #[test]
    fn a_filled_rect_covers_its_pixels_and_nothing_else() {
        static LAYERS: [Layer; 1] = [Layer::rect(2.0, 1.0, 4.0, 3.0).fill(RED)];
        let picture = VectorPicture::new(8.0, 8.0, &LAYERS);
        // At twice the view box: 8 x 6 pixels at (4, 2).
        let pixels = picture.rasterise(16, 16);
        assert_eq!(pixels.len(), 16 * 16 * 4);
        for y in 0..16 {
            for x in 0..16 {
                let inside = (4..12).contains(&x) && (2..8).contains(&y);
                let expected = if inside { [255, 0, 0, 255] } else { [0; 4] };
                assert_eq!(at(&pixels, 16, x, y), expected, "pixel {x},{y}");
            }
        }
    }

    #[test]
    fn later_layers_cover_earlier_ones() {
        static LAYERS: [Layer; 2] = [
            Layer::rect(0.0, 0.0, 6.0, 8.0).fill(RED),
            Layer::rect(4.0, 0.0, 4.0, 8.0).fill(BLUE),
        ];
        let pixels = VectorPicture::new(8.0, 8.0, &LAYERS).rasterise(8, 8);
        assert_eq!(at(&pixels, 8, 2, 4), [255, 0, 0, 255]);
        assert_eq!(at(&pixels, 8, 5, 4), [0, 0, 255, 255], "the overlap");
        assert_eq!(at(&pixels, 8, 7, 4), [0, 0, 255, 255]);
    }

    #[test]
    fn opacity_blends_the_encoded_values_and_is_stored_premultiplied() {
        static LAYERS: [Layer; 2] = [
            Layer::rect(0.0, 0.0, 8.0, 4.0).fill(RED),
            Layer::rect(0.0, 0.0, 4.0, 8.0).fill(BLUE).opacity(0.5),
        ];
        let pixels = VectorPicture::new(8.0, 8.0, &LAYERS).rasterise(8, 8);
        // Blue at half opacity over red: half of each encoded value, as a
        // browser shows it (half of each light would be byte 188).
        assert_eq!(at(&pixels, 8, 1, 1), [128, 0, 128, 255]);
        // Over nothing: half-transparent blue, premultiplied.
        assert_eq!(at(&pixels, 8, 1, 6), [0, 0, 128, 128]);
        assert_eq!(at(&pixels, 8, 6, 1), [255, 0, 0, 255]);
    }

    #[test]
    fn layer_opacity_fades_fill_and_stroke_as_one() {
        // A stroke over its own fill, at half opacity: where they overlap
        // the result is half the stroke, not stroke over half-shown fill.
        static LAYERS: [Layer; 1] = [Layer::rect(4.0, 4.0, 8.0, 8.0)
            .fill(RED)
            .stroke(BLUE, 4.0)
            .opacity(0.5)];
        let pixels = VectorPicture::new(16.0, 16.0, &LAYERS).rasterise(16, 16);
        assert_eq!(at(&pixels, 16, 8, 8), [128, 0, 0, 128], "fill only");
        assert_eq!(at(&pixels, 16, 5, 8), [0, 0, 128, 128], "stroke over fill");
        assert_eq!(at(&pixels, 16, 3, 8), [0, 0, 128, 128], "stroke outside");
        assert_eq!(at(&pixels, 16, 1, 8), [0; 4]);
    }

    #[test]
    fn a_stroke_is_centred_on_the_outline() {
        static LAYERS: [Layer; 1] = [Layer::line(2.0, 8.0, 14.0, 8.0).stroke(RED, 4.0)];
        let pixels = VectorPicture::new(16.0, 16.0, &LAYERS).rasterise(16, 16);
        for y in 0..16 {
            let expected = if (6..10).contains(&y) {
                [255, 0, 0, 255]
            } else {
                [0; 4]
            };
            assert_eq!(at(&pixels, 16, 8, y), expected, "row {y}");
        }
        // Butt caps: nothing beyond the end points.
        assert_eq!(at(&pixels, 16, 1, 8), [0; 4]);
        assert_eq!(at(&pixels, 16, 14, 8), [0; 4]);

        static ROUND: [Layer; 1] = [Layer::line(4.0, 8.0, 12.0, 8.0)
            .stroke(RED, 4.0)
            .cap(LineCap::Round)];
        let pixels = VectorPicture::new(16.0, 16.0, &ROUND).rasterise(16, 16);
        assert_eq!(at(&pixels, 16, 3, 8)[3], 255, "inside the round cap");
        // The cap's corner pixel is partly covered: a square cap would
        // fill it, a butt cap leave it empty.
        let corner = at(&pixels, 16, 2, 6)[3];
        assert!((1..255).contains(&corner), "corner alpha was {corner}");
    }

    #[test]
    fn a_clip_cuts_a_layer_and_ignores_the_layer_transform() {
        // A band across the whole picture, cut to the right half; then the
        // same band moved down by its transform, with the clip staying put.
        static LAYERS: [Layer; 1] = [Layer::rect(0.0, 6.0, 16.0, 4.0)
            .fill(RED)
            .clip_rect(8.0, 0.0, 8.0, 16.0)];
        let pixels = VectorPicture::new(16.0, 16.0, &LAYERS).rasterise(16, 16);
        assert_eq!(at(&pixels, 16, 4, 8), [0; 4], "clipped away");
        assert_eq!(at(&pixels, 16, 12, 8), [255, 0, 0, 255]);

        static MOVED: [Layer; 1] = [Layer::rect(0.0, 6.0, 16.0, 4.0)
            .fill(RED)
            .translate(-4.0, 4.0)
            .clip_rect(8.0, 0.0, 8.0, 16.0)];
        let pixels = VectorPicture::new(16.0, 16.0, &MOVED).rasterise(16, 16);
        assert_eq!(at(&pixels, 16, 9, 8), [0; 4], "the band moved down");
        assert_eq!(at(&pixels, 16, 9, 12), [255, 0, 0, 255]);
        assert_eq!(at(&pixels, 16, 4, 12), [0; 4], "the clip did not move");
        assert_eq!(at(&pixels, 16, 13, 12), [0; 4], "nor grow to the right");
    }

    #[test]
    fn a_layer_transform_turns_the_shape_about_a_point() {
        // 12 x 4 at (2, 6), turned a quarter about the centre (8, 8):
        // 4 x 12 at (6, 2).
        static LAYERS: [Layer; 1] = [Layer::rect(2.0, 6.0, 12.0, 4.0)
            .fill(RED)
            .rotate(90.0, 8.0, 8.0)];
        let pixels = VectorPicture::new(16.0, 16.0, &LAYERS).rasterise(16, 16);
        for y in 0..16 {
            for x in 0..16 {
                let inside = (6..10).contains(&x) && (2..14).contains(&y);
                let expected = if inside { [255, 0, 0, 255] } else { [0; 4] };
                assert_eq!(at(&pixels, 16, x, y), expected, "pixel {x},{y}");
            }
        }
    }

    #[test]
    fn circles_ellipses_and_paths_are_anti_aliased() {
        static LAYERS: [Layer; 3] = [
            Layer::circle(16.0, 16.0, 12.0).fill(RED),
            Layer::ellipse(16.0, 16.0, 14.0, 4.0).stroke(BLUE, 2.0),
            Layer::path("M4 28h24").stroke(BLUE, 2.0),
        ];
        let picture = VectorPicture::new(32.0, 32.0, &LAYERS);
        assert!(picture.check().is_ok());
        let pixels = picture.rasterise(32, 32);
        assert_eq!(at(&pixels, 32, 16, 8), [255, 0, 0, 255]);
        assert_eq!(at(&pixels, 32, 16, 2), [0; 4]);
        // The ellipse's outline crosses the circle at its left and right.
        assert_eq!(at(&pixels, 32, 2, 16), [0, 0, 255, 255]);
        assert_eq!(at(&pixels, 32, 16, 28)[2], 255);
        // The circle alone: its area, from its alpha, is pi * 12^2 = 452.4.
        let alone = VectorPicture::new(32.0, 32.0, &LAYERS[..1]).rasterise(32, 32);
        let disc: f32 = alone
            .chunks_exact(4)
            .map(|pixel| f32::from(pixel[3]) / 255.0)
            .sum();
        // (The rasteriser flattens curves, which loses a little.)
        assert!((disc - 452.4).abs() < 4.0, "disc area was {disc}");
        let partial = pixels
            .chunks_exact(4)
            .filter(|pixel| (1..255).contains(&pixel[3]))
            .count();
        assert!(partial > 30, "only {partial} edge pixels");
    }

    #[test]
    fn the_view_box_is_fitted_centred_and_cuts_what_lies_outside() {
        // A square view box in a wide image: centred, with nothing in the
        // margins although the layer extends far beyond the box.
        static LAYERS: [Layer; 1] = [Layer::rect(-100.0, -100.0, 300.0, 300.0).fill(RED)];
        let picture = VectorPicture::with_view_box(10.0, 10.0, 8.0, 8.0, &LAYERS);
        let pixels = picture.rasterise(16, 8);
        assert_eq!(at(&pixels, 16, 2, 4), [0; 4]);
        assert_eq!(at(&pixels, 16, 4, 4), [255, 0, 0, 255]);
        assert_eq!(at(&pixels, 16, 11, 4), [255, 0, 0, 255]);
        assert_eq!(at(&pixels, 16, 13, 4), [0; 4]);
    }

    #[test]
    fn bad_layers_are_reported_and_skipped() {
        static LAYERS: [Layer; 2] = [
            Layer::path("M0 0 L").fill(RED),
            Layer::rect(0.0, 0.0, 4.0, 4.0).fill(BLUE),
        ];
        let picture = VectorPicture::new(4.0, 4.0, &LAYERS);
        assert!(matches!(
            picture.check(),
            Err(IconError::InvalidPath { .. })
        ));
        assert_eq!(at(&picture.rasterise(4, 4), 4, 2, 2), [0, 0, 255, 255]);
        assert_eq!(
            VectorPicture::new(0.0, 4.0, &LAYERS).check(),
            Err(IconError::MissingViewBox)
        );
        assert_eq!(
            VectorPicture::new(0.0, 4.0, &LAYERS).rasterise(2, 2),
            [0; 16]
        );
        assert!(
            VectorPicture::new(4.0, 4.0, &LAYERS)
                .rasterise(0, 4)
                .is_empty()
        );
    }

    #[test]
    fn the_content_hash_follows_the_content() {
        static A: [Layer; 1] = [Layer::circle(4.0, 4.0, 3.0).fill(RED)];
        static B: [Layer; 1] = [Layer::circle(4.0, 4.0, 3.0).fill(RED)];
        static C: [Layer; 1] = [Layer::circle(4.0, 4.0, 3.0).fill(BLUE)];
        static D: [Layer; 1] = [Layer::circle(4.0, 4.0, 3.0).fill(RED).opacity(0.5)];
        let hash = |layers| VectorPicture::new(8.0, 8.0, layers).content_hash();
        assert_eq!(hash(&A), hash(&B));
        assert_ne!(hash(&A), hash(&C));
        assert_ne!(hash(&A), hash(&D));
        assert_ne!(hash(&A), VectorPicture::new(8.0, 9.0, &A).content_hash());
    }
}
