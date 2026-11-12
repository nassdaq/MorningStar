use crate::{Color, Image, ImageFit, Point, Rect, Transform};

/// A stroke along the inside of a rectangle's edge.
///
/// The border lies entirely within the rectangle's bounds: a border of
/// `width` 2 on a 100-pixel-wide rectangle leaves 96 pixels between its inner
/// edges. Its inner corners have radius `corner_radius - width` (square once
/// the border is wider than the corner radius).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Border {
    /// Stroke width in physical pixels.
    pub width: f32,
    pub color: Color,
}

impl Border {
    pub const fn new(width: f32, color: Color) -> Self {
        Self { width, color }
    }

    /// True if the border could not affect any pixel.
    fn is_invisible(&self) -> bool {
        // Written so that NaN counts as invisible.
        !(self.width > 0.0 && self.color.a > 0.0)
    }
}

/// The pattern of a dashed border: `length` pixels of stroke, then `gap`
/// pixels of nothing, repeated along the middle of the stroke.
///
/// The pattern starts at the bottom end of the rectangle's left edge and
/// runs clockwise (up the left edge first), which is where a browser starts
/// it. See [`Dash::fitted`] for a pattern that closes without a seam.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dash {
    /// Length of each dash, in pixels, measured along the stroke's middle.
    pub length: f32,
    /// Length of the space between dashes.
    pub gap: f32,
}

impl Dash {
    /// A pattern of `length` on, `gap` off.
    pub const fn new(length: f32, gap: f32) -> Self {
        Self { length, gap }
    }

    /// The pattern CSS `border-style: dashed` produces in Chromium for a
    /// border of `width` pixels around `rect`: dashes three times as long as
    /// the border is wide with gaps of twice the width (two and one times
    /// for borders 3 pixels or wider), with the gap then adjusted so that a
    /// whole number of dashes fits the perimeter exactly.
    pub fn css(rect: Rect, corner_radius: f32, width: f32) -> Self {
        let (length, gap) = if width >= 3.0 {
            (width * 2.0, width)
        } else {
            (width * 3.0, width * 2.0)
        };
        Self::new(length, gap).fitted(rect, corner_radius, width)
    }

    /// The same dash length with the gap adjusted, as little as possible, so
    /// that a whole number of dashes fits around the rectangle and the
    /// pattern closes without a short or long dash where it meets itself.
    pub fn fitted(self, rect: Rect, corner_radius: f32, width: f32) -> Self {
        let perimeter = dash_perimeter(rect, corner_radius, width);
        let period = self.length + self.gap;
        // Written so that NaN leaves the pattern alone.
        if !(self.length > 0.0 && self.gap >= 0.0 && period > 0.0 && perimeter > period) {
            return self;
        }
        let fewer = (perimeter / period).floor();
        let more = fewer + 1.0;
        let wide = (perimeter - fewer * self.length) / fewer;
        let narrow = (perimeter - more * self.length) / more;
        let gap = if narrow <= 0.0 || (wide - self.gap).abs() < (narrow - self.gap).abs() {
            wide
        } else {
            narrow
        };
        Self::new(self.length, gap)
    }

    fn is_valid(&self) -> bool {
        // Written so that NaN counts as invalid.
        self.length > 0.0 && self.gap > 0.0 && (self.length + self.gap).is_finite()
    }
}

/// Length of the line a dashed border's pattern is laid along: the middle
/// of a stroke of `width` running inside the edge of `rect`.
fn dash_perimeter(rect: Rect, corner_radius: f32, width: f32) -> f32 {
    let half_x = (rect.width - width).max(0.0) / 2.0;
    let half_y = (rect.height - width).max(0.0) / 2.0;
    let radius = (corner_radius.min(rect.width / 2.0).min(rect.height / 2.0) - width / 2.0)
        .clamp(0.0, half_x.min(half_y));
    4.0 * (half_x - radius) + 4.0 * (half_y - radius) + std::f32::consts::TAU * radius
}

/// A two-stop linear gradient.
///
/// The colour is `start_color` at `start`, `end_color` at `end`, and varies
/// along the line between them; points before `start` or beyond `end` take
/// the nearer stop's colour. Both points are in the same physical-pixel
/// coordinates as the rectangle being filled, so they may lie anywhere, not
/// only on its edge.
///
/// # Interpolation
///
/// Colours are interpolated **in linear RGB with premultiplied alpha**, the
/// space the whole pipeline blends in. This is physically uniform but is not
/// what CSS and most design tools do by default (they interpolate
/// sRGB-encoded values), so the midpoint of a gradient between two colours is
/// lighter here than in a browser. Premultiplied interpolation means a fade
/// to a fully transparent stop keeps the hue of the opaque stop, whatever the
/// RGB of the transparent one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinearGradient {
    pub start: Point,
    pub end: Point,
    pub start_color: Color,
    pub end_color: Color,
}

impl LinearGradient {
    pub const fn new(start: Point, end: Point, start_color: Color, end_color: Color) -> Self {
        Self {
            start,
            end,
            start_color,
            end_color,
        }
    }

    /// From `top` at the top edge of `rect` to `bottom` at its bottom edge.
    pub fn vertical(rect: Rect, top: Color, bottom: Color) -> Self {
        Self::new(
            Point::new(rect.x, rect.y),
            Point::new(rect.x, rect.y + rect.height),
            top,
            bottom,
        )
    }

    /// From `left` at the left edge of `rect` to `right` at its right edge.
    pub fn horizontal(rect: Rect, left: Color, right: Color) -> Self {
        Self::new(
            Point::new(rect.x, rect.y),
            Point::new(rect.x + rect.width, rect.y),
            left,
            right,
        )
    }

    /// A gradient across `rect` at an angle, with the meaning CSS gives
    /// `linear-gradient(<angle>, from, to)`: 0 degrees runs bottom to top,
    /// 90 left to right, 180 top to bottom. The gradient line passes through
    /// the centre of `rect` and is exactly long enough that the corners take
    /// the pure stop colours.
    pub fn angled(rect: Rect, degrees: f32, from: Color, to: Color) -> Self {
        let (sin, cos) = degrees.to_radians().sin_cos();
        // Unit direction of travel; `y` grows downwards.
        let (dx, dy) = (sin, -cos);
        let half = (rect.width * sin.abs() + rect.height * cos.abs()) / 2.0;
        let center = rect.center();
        Self::new(
            Point::new(center.x - dx * half, center.y - dy * half),
            Point::new(center.x + dx * half, center.y + dy * half),
            from,
            to,
        )
    }
}

/// A drop shadow: the blurred silhouette of a rounded rectangle.
///
/// The fields mean what they do in CSS `box-shadow` and in design tools.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    /// How far the shadow is shifted right of the rectangle casting it.
    pub offset_x: f32,
    /// How far the shadow is shifted below the rectangle casting it.
    pub offset_y: f32,
    /// Blur radius in physical pixels: the shadow fades over roughly this
    /// distance either side of its edge. It is a Gaussian blur with a
    /// standard deviation of half this value. Zero gives a sharp edge.
    pub blur: f32,
    /// Grows (or, if negative, shrinks) the silhouette on every side before
    /// it is blurred.
    pub spread: f32,
    pub color: Color,
}

impl Shadow {
    /// A shadow with no spread.
    pub const fn new(offset_x: f32, offset_y: f32, blur: f32, color: Color) -> Self {
        Self {
            offset_x,
            offset_y,
            blur,
            spread: 0.0,
            color,
        }
    }

    pub const fn with_spread(mut self, spread: f32) -> Self {
        self.spread = spread;
        self
    }
}

/// One drawing instruction.
///
/// This enum is the contract between whatever produces UI and the renderer.
/// It grows as the renderer learns to draw more.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum DrawOp {
    /// A filled rectangle with optionally rounded corners and anti-aliased
    /// edges. `corner_radius` is clamped to half the shorter side, so a large
    /// value yields a pill or a circle.
    Rect {
        rect: Rect,
        color: Color,
        corner_radius: f32,
    },
    /// One glyph: the region `atlas` (in texels) of the frame's glyph atlas,
    /// used as a coverage mask, tinted with `color` and drawn into `rect`.
    ///
    /// Produced by the text subsystem; applications draw text through it
    /// rather than pushing glyphs themselves.
    Glyph {
        rect: Rect,
        atlas: Rect,
        color: Color,
    },
    /// A rounded rectangle filled with `fill` and stroked, inside its bounds,
    /// with `border`. A fully transparent `fill` gives an outline only.
    ///
    /// Where the border is translucent the fill shows through it, as with a
    /// CSS border over a background or an inside stroke in a design tool.
    BorderedRect {
        rect: Rect,
        corner_radius: f32,
        fill: Color,
        border: Border,
    },
    /// A rounded rectangle filled with a linear gradient.
    GradientRect {
        rect: Rect,
        corner_radius: f32,
        gradient: LinearGradient,
    },
    /// The shadow cast by the rounded rectangle `rect`. Drawn like any other
    /// operation, so it must come before whatever casts it.
    Shadow {
        rect: Rect,
        corner_radius: f32,
        shadow: Shadow,
    },
    /// The region `source` of a registered image (in image pixels), scaled
    /// into `rect`, multiplied by `tint` and cut to rounded corners.
    /// See [`DisplayList::draw_image_region`].
    Image {
        image: Image,
        rect: Rect,
        source: Rect,
        corner_radius: f32,
        tint: Color,
    },
    /// A rounded rectangle filled with `fill` whose border is drawn as
    /// dashes. Like [`BorderedRect`](Self::BorderedRect) in every other
    /// respect.
    DashedRect {
        /// Outer bounds, in physical pixels.
        rect: Rect,
        /// Corner radius of the outer edge.
        corner_radius: f32,
        /// Fill colour; transparent for an outline only.
        fill: Color,
        /// The stroke, drawn inside the bounds.
        border: Border,
        /// The dash pattern along the stroke.
        dash: Dash,
    },
    /// Restricts every following operation, of any kind, to `rect` until the
    /// matching [`PopClip`](Self::PopClip). See [`DisplayList::push_clip`].
    PushClip { rect: Rect, corner_radius: f32 },
    /// Ends the innermost clip.
    PopClip,
    /// Draws every following operation, of any kind, through `transform`
    /// until the matching [`PopTransform`](Self::PopTransform). See
    /// [`DisplayList::push_transform`].
    PushTransform { transform: Transform },
    /// Ends the innermost transform.
    PopTransform,
}

/// Which rows of a glyph atlas changed recently, so a renderer that already
/// holds an earlier version can upload only those.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirtyRows {
    /// The generation this record is relative to: compared with the atlas as
    /// it was at generation `since`, the dimensions are the same and only
    /// rows `start..end` differ.
    pub since: u64,
    /// First changed row.
    pub start: u32,
    /// One past the last changed row. Equal to `start` if nothing changed.
    pub end: u32,
}

/// A borrowed view of the glyph atlas a display list's glyphs refer to: a
/// single-channel coverage image.
#[derive(Clone, Copy, Debug)]
pub struct AtlasView<'a> {
    pub width: u32,
    pub height: u32,
    /// `width * height` coverage bytes, row by row from the top.
    pub pixels: &'a [u8],
    /// Changes whenever `pixels` or the dimensions change, so a renderer can
    /// skip re-uploading an unchanged atlas. A later version of the same
    /// atlas always has a larger generation.
    pub generation: u64,
    /// What changed since an earlier generation, if the producer tracks it.
    /// A renderer holding a generation in `dirty.since..generation` needs to
    /// upload only the rows named; any other renderer, or any view with
    /// `None` here, takes the whole image.
    pub dirty: Option<DirtyRows>,
}

impl AtlasView<'static> {
    /// A one-texel transparent atlas, for frames that contain no text.
    pub const EMPTY: Self = Self {
        width: 1,
        height: 1,
        pixels: &[0],
        generation: 0,
        dirty: None,
    };
}

impl<'a> AtlasView<'a> {
    /// A view with no change tracking: a renderer uploads all of it whenever
    /// `generation` differs from the one it holds.
    pub const fn new(width: u32, height: u32, pixels: &'a [u8], generation: u64) -> Self {
        Self {
            width,
            height,
            pixels,
            generation,
            dirty: None,
        }
    }
}

/// An ordered list of drawing instructions for one frame.
///
/// Operations are painted in order: later ones cover earlier ones. The list
/// is designed to be reused across frames; [`clear`](Self::clear) keeps its
/// allocation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DisplayList {
    ops: Vec<DrawOp>,
}

impl DisplayList {
    pub const fn new() -> Self {
        Self { ops: Vec::new() }
    }

    /// Removes every operation, keeping the allocation.
    pub fn clear(&mut self) {
        self.ops.clear();
    }

    pub fn ops(&self) -> &[DrawOp] {
        &self.ops
    }

    pub fn len(&self) -> usize {
        self.ops.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Adds one glyph. See [`DrawOp::Glyph`].
    pub fn draw_glyph(&mut self, rect: Rect, atlas: Rect, color: Color) {
        if rect.is_empty() || color.a.is_nan() || color.a <= 0.0 {
            return;
        }
        self.ops.push(DrawOp::Glyph { rect, atlas, color });
    }

    /// Adds a filled rectangle with square corners.
    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        self.fill_rounded_rect(rect, 0.0, color);
    }

    /// Adds a filled rectangle with rounded corners.
    ///
    /// Rectangles that could not affect any pixel (no area, or fully
    /// transparent) are dropped here rather than sent to the GPU.
    pub fn fill_rounded_rect(&mut self, rect: Rect, corner_radius: f32, color: Color) {
        if rect.is_empty() || color.a.is_nan() || color.a <= 0.0 {
            return;
        }
        self.ops.push(DrawOp::Rect {
            rect,
            color,
            // NaN or negative radii mean square corners.
            corner_radius: corner_radius.max(0.0),
        });
    }

    /// Adds the outline of a rectangle with square corners. See
    /// [`stroke_rounded_rect`](Self::stroke_rounded_rect).
    pub fn stroke_rect(&mut self, rect: Rect, border: Border) {
        self.stroke_rounded_rect(rect, 0.0, border);
    }

    /// Adds the outline of a rounded rectangle: a stroke of `border.width`
    /// running along the inside of the rectangle's edge, anti-aliased on both
    /// its outer and its inner side. Nothing is drawn inside the outline.
    pub fn stroke_rounded_rect(&mut self, rect: Rect, corner_radius: f32, border: Border) {
        self.fill_bordered_rect(rect, corner_radius, Color::rgba(0.0, 0.0, 0.0, 0.0), border);
    }

    /// Adds a rounded rectangle filled with `fill` and outlined with
    /// `border`, as one operation.
    ///
    /// Prefer this to a fill followed by a stroke: drawn together, the fill
    /// cannot show through as a fringe along the anti-aliased outer edge.
    pub fn fill_bordered_rect(
        &mut self,
        rect: Rect,
        corner_radius: f32,
        fill: Color,
        border: Border,
    ) {
        if border.is_invisible() {
            // Covers an invisible fill too.
            return self.fill_rounded_rect(rect, corner_radius, fill);
        }
        if rect.is_empty() {
            return;
        }
        self.ops.push(DrawOp::BorderedRect {
            rect,
            corner_radius: corner_radius.max(0.0),
            fill: if fill.a > 0.0 {
                fill
            } else {
                Color::rgba(0.0, 0.0, 0.0, 0.0)
            },
            border,
        });
    }

    /// Adds the outline of a rounded rectangle drawn as dashes. See
    /// [`fill_dashed_rect`](Self::fill_dashed_rect).
    pub fn stroke_dashed_rect(
        &mut self,
        rect: Rect,
        corner_radius: f32,
        border: Border,
        dash: Dash,
    ) {
        self.fill_dashed_rect(
            rect,
            corner_radius,
            Color::rgba(0.0, 0.0, 0.0, 0.0),
            border,
            dash,
        );
    }

    /// Adds a rounded rectangle filled with `fill` whose border is a row of
    /// dashes following the edge, corners included: CSS
    /// `border-style: dashed`. The pattern is measured along the middle of
    /// the stroke; [`Dash::css`] gives the pattern a browser would use.
    ///
    /// A pattern without a positive, finite dash and gap is drawn as a solid
    /// border.
    pub fn fill_dashed_rect(
        &mut self,
        rect: Rect,
        corner_radius: f32,
        fill: Color,
        border: Border,
        dash: Dash,
    ) {
        if border.is_invisible() || !dash.is_valid() {
            return self.fill_bordered_rect(rect, corner_radius, fill, border);
        }
        if rect.is_empty() {
            return;
        }
        self.ops.push(DrawOp::DashedRect {
            rect,
            corner_radius: corner_radius.max(0.0),
            fill: if fill.a > 0.0 {
                fill
            } else {
                Color::rgba(0.0, 0.0, 0.0, 0.0)
            },
            border,
            dash,
        });
    }

    /// Adds a rounded rectangle filled with a linear gradient. See
    /// [`LinearGradient`] for how colours are interpolated.
    ///
    /// A gradient whose `start` and `end` coincide (or are not finite) has no
    /// direction and is drawn as solid `start_color`.
    pub fn fill_gradient_rect(&mut self, rect: Rect, corner_radius: f32, gradient: LinearGradient) {
        let visible = |color: Color| color.a > 0.0;
        if rect.is_empty() || !(visible(gradient.start_color) || visible(gradient.end_color)) {
            return;
        }
        let (dx, dy) = (
            gradient.end.x - gradient.start.x,
            gradient.end.y - gradient.start.y,
        );
        let length_squared = dx * dx + dy * dy;
        if !(length_squared.is_finite() && length_squared > 0.0) {
            return self.fill_rounded_rect(rect, corner_radius, gradient.start_color);
        }
        self.ops.push(DrawOp::GradientRect {
            rect,
            corner_radius: corner_radius.max(0.0),
            gradient,
        });
    }

    /// Adds the shadow cast by the rounded rectangle `rect`.
    ///
    /// Call this before drawing the rectangle itself. The whole blurred
    /// silhouette is painted, including the part under the rectangle, so a
    /// translucent rectangle shows its own shadow through itself.
    pub fn draw_shadow(&mut self, rect: Rect, corner_radius: f32, shadow: Shadow) {
        let finite = shadow.offset_x.is_finite() && shadow.offset_y.is_finite();
        if rect.is_empty() || !finite || shadow.color.a.is_nan() || shadow.color.a <= 0.0 {
            return;
        }
        self.ops.push(DrawOp::Shadow {
            rect,
            corner_radius: corner_radius.max(0.0),
            shadow: Shadow {
                // NaN or negative blur means a sharp shadow.
                blur: shadow.blur.max(0.0),
                spread: if shadow.spread.is_finite() {
                    shadow.spread
                } else {
                    0.0
                },
                ..shadow
            },
        });
    }

    /// Draws the whole of `image` stretched to `rect`, unmodified.
    ///
    /// To keep the image's aspect ratio use
    /// [`draw_image_fit`](Self::draw_image_fit).
    pub fn draw_image(&mut self, image: Image, rect: Rect) {
        self.draw_image_region(image, image.bounds(), rect, 0.0, Color::WHITE);
    }

    /// Draws `image` fitted into `bounds` as `fit` says (see [`ImageFit`]),
    /// with rounded corners and a tint.
    ///
    /// The corners of the rectangle actually painted are rounded: all of
    /// `bounds` for [`ImageFit::Cover`] and [`ImageFit::Stretch`], the
    /// letterboxed rectangle for [`ImageFit::Contain`]. A `corner_radius` of
    /// half the side of a square `bounds` gives a circular avatar. For
    /// `tint` see [`draw_image_region`](Self::draw_image_region).
    pub fn draw_image_fit(
        &mut self,
        image: Image,
        bounds: Rect,
        fit: ImageFit,
        corner_radius: f32,
        tint: Color,
    ) {
        let placed = fit.place(image.bounds(), bounds);
        self.draw_image_region(image, placed.source, placed.rect, corner_radius, tint);
    }

    /// Draws the region `source` of `image` scaled into `rect`: the general
    /// form, for sprite sheets and cropping.
    ///
    /// - `source` is in the image's own pixels, as reported by
    ///   [`Image::width`] and [`Image::height`], with the origin at its
    ///   top-left. It is not clamped: a part lying outside the image repeats
    ///   the image's edge pixels.
    /// - `corner_radius` rounds the corners of `rect`, anti-aliased, and is
    ///   clamped to half its shorter side.
    /// - `tint` multiplies every pixel, colour and alpha.
    ///   [`Color::WHITE`] leaves the image as it is; white with an alpha of
    ///   0.5 draws it at half opacity; a colour recolours an icon drawn in
    ///   white.
    ///
    /// The image is filtered (bilinear, with mipmaps when reduced), so it
    /// may be drawn at any size. Like every operation it is clipped by the
    /// clips in effect and painted in list order.
    ///
    /// Nothing is recorded if `rect` or `source` is empty or `tint` is fully
    /// transparent. Drawing an image that has been released draws nothing.
    pub fn draw_image_region(
        &mut self,
        image: Image,
        source: Rect,
        rect: Rect,
        corner_radius: f32,
        tint: Color,
    ) {
        let finite = source.x.is_finite() && source.y.is_finite();
        if rect.is_empty() || source.is_empty() || !finite || tint.a.is_nan() || tint.a <= 0.0 {
            return;
        }
        self.ops.push(DrawOp::Image {
            image,
            rect,
            source,
            corner_radius: corner_radius.max(0.0),
            tint,
        });
    }

    /// Restricts everything drawn until the matching
    /// [`pop_clip`](Self::pop_clip) to `rect`.
    ///
    /// Clips nest: inside another clip, only the intersection of the two is
    /// drawn. Every kind of operation is clipped, glyphs and shadows
    /// included. A clip edge that falls between pixels is anti-aliased. An
    /// empty `rect` clips everything.
    pub fn push_clip(&mut self, rect: Rect) {
        self.push_rounded_clip(rect, 0.0);
    }

    /// Like [`push_clip`](Self::push_clip), with rounded corners.
    /// `corner_radius` is clamped to half the shorter side.
    ///
    /// # Limits
    ///
    /// One rounded clip is exact at a time, the innermost. It intersects
    /// exactly with any number of square clips around or inside it, but a
    /// rounded clip that has another rounded clip nested inside it acts as
    /// its bounding rectangle (square corners) for as long as the inner one
    /// is in effect.
    pub fn push_rounded_clip(&mut self, rect: Rect, corner_radius: f32) {
        self.ops.push(DrawOp::PushClip {
            rect,
            corner_radius: corner_radius.max(0.0),
        });
    }

    /// Ends the clip started by the most recent unmatched
    /// [`push_clip`](Self::push_clip). Without one, this does nothing.
    /// Clips still open at the end of the list end with it.
    pub fn pop_clip(&mut self) {
        self.ops.push(DrawOp::PopClip);
    }

    /// Draws everything until the matching
    /// [`pop_transform`](Self::pop_transform) through `transform`: rotated,
    /// slanted, scaled or moved.
    ///
    /// The transform is in the same physical-pixel space as rectangles: an
    /// operation's geometry is given as if there were no transform, and
    /// each of its points then goes through it. To turn something about its
    /// own centre use [`Transform::about`]. Every kind of operation is
    /// transformed whole: rounded corners, borders, dashes, gradients,
    /// shadows, glyphs and images turn with their rectangle and keep their
    /// anti-aliased edges.
    ///
    /// Transforms nest: one pushed inside another applies first. Pushes and
    /// pops of transforms and clips must be properly nested with each
    /// other.
    ///
    /// # Clips
    ///
    /// * A transform inside a clip: the clip stays where it is on screen
    ///   and cuts the transformed content. Exact, for square and rounded
    ///   clips.
    /// * A clip pushed inside a transform is transformed with the content
    ///   (a rotated card that clips its children). One such clip is exact
    ///   at a time, the innermost, whether square or rounded; square ones
    ///   directly inside one another intersect exactly. It takes the place
    ///   of the one exact rounded clip (see
    ///   [`push_rounded_clip`](Self::push_rounded_clip)): an enclosing
    ///   rounded clip, a transformed clip that has another nested in it,
    ///   and a transformed clip with a further transform pushed inside it,
    ///   each act as their bounding rectangle on screen meanwhile.
    ///
    /// A transform that is not finite, or that flattens everything onto a
    /// line (a zero scale), draws nothing until it is popped.
    pub fn push_transform(&mut self, transform: Transform) {
        self.ops.push(DrawOp::PushTransform { transform });
    }

    /// Ends the transform started by the most recent unmatched
    /// [`push_transform`](Self::push_transform). Without one, this does
    /// nothing. Transforms still open at the end of the list end with it.
    pub fn pop_transform(&mut self) {
        self.ops.push(DrawOp::PopTransform);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashed_border_is_recorded_with_its_pattern() {
        let mut list = DisplayList::new();
        let rect = Rect::new(0.0, 0.0, 100.0, 40.0);
        let border = Border::new(2.0, Color::BLACK);
        list.stroke_dashed_rect(rect, 8.0, border, Dash::new(6.0, 4.0));
        assert!(matches!(
            list.ops()[0],
            DrawOp::DashedRect { dash, fill, .. } if dash == Dash::new(6.0, 4.0) && fill.a == 0.0
        ));
    }

    #[test]
    fn a_dash_pattern_that_cannot_repeat_falls_back_to_a_solid_border() {
        let rect = Rect::new(0.0, 0.0, 100.0, 40.0);
        let border = Border::new(2.0, Color::BLACK);
        for dash in [
            Dash::new(0.0, 4.0),
            Dash::new(4.0, 0.0),
            Dash::new(f32::NAN, 4.0),
            Dash::new(4.0, f32::INFINITY),
            Dash::new(-1.0, 2.0),
        ] {
            let mut list = DisplayList::new();
            list.fill_dashed_rect(rect, 0.0, Color::WHITE, border, dash);
            assert!(
                matches!(list.ops()[0], DrawOp::BorderedRect { .. }),
                "{dash:?}"
            );
        }
        // And an invisible border leaves only the fill.
        let mut list = DisplayList::new();
        list.fill_dashed_rect(
            rect,
            0.0,
            Color::WHITE,
            Border::new(0.0, Color::BLACK),
            Dash::new(3.0, 2.0),
        );
        assert!(matches!(list.ops()[0], DrawOp::Rect { .. }));
    }

    #[test]
    fn a_fitted_dash_divides_the_perimeter_exactly() {
        for (rect, radius, width) in [
            (Rect::new(0.0, 0.0, 220.0, 128.0), 24.0, 2.0),
            (Rect::new(3.0, 5.0, 101.0, 33.0), 0.0, 1.0),
            (Rect::new(0.0, 0.0, 64.0, 64.0), 32.0, 4.0),
            (Rect::new(0.0, 0.0, 300.0, 20.0), 500.0, 3.0),
        ] {
            let dash = Dash::css(rect, radius, width);
            let perimeter = dash_perimeter(rect, radius, width);
            let count = perimeter / (dash.length + dash.gap);
            assert!(
                (count - count.round()).abs() < 1e-3,
                "{count} dashes around {rect:?}"
            );
            assert!(dash.gap > 0.0);
            let expected = if width >= 3.0 {
                2.0 * width
            } else {
                3.0 * width
            };
            assert_eq!(dash.length, expected);
        }
    }

    #[test]
    fn dash_perimeter_of_simple_shapes() {
        // A square of side 10 with a 2 px stroke: the middle line is a
        // square of side 8.
        let square = dash_perimeter(Rect::new(0.0, 0.0, 10.0, 10.0), 0.0, 2.0);
        assert!((square - 32.0).abs() < 1e-4);
        // A circle of diameter 20 with a 2 px stroke: radius 9.
        let circle = dash_perimeter(Rect::new(0.0, 0.0, 20.0, 20.0), 10.0, 2.0);
        assert!((circle - std::f32::consts::TAU * 9.0).abs() < 1e-3);
        // Degenerate input stays finite.
        assert_eq!(dash_perimeter(Rect::new(0.0, 0.0, 1.0, 1.0), 9.0, 4.0), 0.0);
        let fitted = Dash::new(3.0, 2.0).fitted(Rect::new(0.0, 0.0, 1.0, 1.0), 0.0, 1.0);
        assert_eq!(fitted, Dash::new(3.0, 2.0));
    }

    #[test]
    fn keeps_paint_order() {
        let mut list = DisplayList::new();
        list.fill_rect(Rect::new(0.0, 0.0, 1.0, 1.0), Color::BLACK);
        list.fill_rounded_rect(Rect::new(0.0, 0.0, 2.0, 2.0), 1.0, Color::WHITE);

        assert_eq!(list.len(), 2);
        assert!(matches!(list.ops()[1], DrawOp::Rect { color, .. } if color == Color::WHITE));
    }

    #[test]
    fn invisible_rects_are_dropped() {
        let mut list = DisplayList::new();
        list.fill_rect(Rect::new(0.0, 0.0, 0.0, 5.0), Color::WHITE);
        list.fill_rect(
            Rect::new(0.0, 0.0, 5.0, 5.0),
            Color::rgba(1.0, 1.0, 1.0, 0.0),
        );
        assert!(list.is_empty());
    }

    #[test]
    fn bad_radius_becomes_square() {
        let mut list = DisplayList::new();
        list.fill_rounded_rect(Rect::new(0.0, 0.0, 5.0, 5.0), -3.0, Color::WHITE);
        list.fill_rounded_rect(Rect::new(0.0, 0.0, 5.0, 5.0), f32::NAN, Color::WHITE);
        for op in list.ops() {
            assert!(matches!(*op, DrawOp::Rect { corner_radius, .. } if corner_radius == 0.0));
        }
    }

    #[test]
    fn clear_keeps_capacity() {
        let mut list = DisplayList::new();
        for _ in 0..32 {
            list.fill_rect(Rect::new(0.0, 0.0, 1.0, 1.0), Color::WHITE);
        }
        let ptr = list.ops().as_ptr();
        list.clear();
        assert!(list.is_empty());
        list.fill_rect(Rect::new(0.0, 0.0, 1.0, 1.0), Color::WHITE);
        assert_eq!(list.ops().as_ptr(), ptr, "no reallocation after clear");
    }

    #[test]
    fn invisible_border_degrades_to_fill_and_invisible_fill_to_outline() {
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
        let mut list = DisplayList::new();
        list.fill_bordered_rect(rect, 2.0, Color::WHITE, Border::new(0.0, Color::BLACK));
        list.fill_bordered_rect(rect, 2.0, Color::WHITE, Border::new(f32::NAN, Color::BLACK));
        assert!(
            list.ops()
                .iter()
                .all(|op| matches!(op, DrawOp::Rect { .. }))
        );
        assert_eq!(list.len(), 2);

        list.clear();
        list.stroke_rect(rect, Border::new(1.0, Color::rgba(1.0, 1.0, 1.0, 0.0)));
        list.stroke_rect(
            Rect::new(0.0, 0.0, 0.0, 4.0),
            Border::new(1.0, Color::WHITE),
        );
        assert!(list.is_empty(), "nothing visible to draw");

        list.stroke_rounded_rect(rect, -1.0, Border::new(1.0, Color::WHITE));
        assert!(matches!(
            list.ops()[0],
            DrawOp::BorderedRect { fill, corner_radius, .. } if fill.a == 0.0 && corner_radius == 0.0
        ));
    }

    #[test]
    fn degenerate_gradient_becomes_a_solid_fill() {
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
        let point = Point::new(3.0, 3.0);
        let mut list = DisplayList::new();
        list.fill_gradient_rect(
            rect,
            0.0,
            LinearGradient::new(point, point, Color::WHITE, Color::BLACK),
        );
        list.fill_gradient_rect(
            rect,
            0.0,
            LinearGradient::new(point, Point::new(f32::NAN, 0.0), Color::WHITE, Color::BLACK),
        );
        assert_eq!(list.len(), 2);
        for op in list.ops() {
            assert!(matches!(*op, DrawOp::Rect { color, .. } if color == Color::WHITE));
        }

        list.clear();
        let clear = Color::rgba(1.0, 1.0, 1.0, 0.0);
        list.fill_gradient_rect(rect, 0.0, LinearGradient::vertical(rect, clear, clear));
        assert!(list.is_empty());
    }

    #[test]
    fn angled_gradient_follows_the_css_convention() {
        let rect = Rect::new(10.0, 20.0, 100.0, 50.0);
        let close = |a: Point, b: Point| (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3;

        let down = LinearGradient::angled(rect, 180.0, Color::WHITE, Color::BLACK);
        assert!(close(down.start, Point::new(60.0, 20.0)), "{down:?}");
        assert!(close(down.end, Point::new(60.0, 70.0)), "{down:?}");

        let right = LinearGradient::angled(rect, 90.0, Color::WHITE, Color::BLACK);
        assert!(close(right.start, Point::new(10.0, 45.0)), "{right:?}");
        assert!(close(right.end, Point::new(110.0, 45.0)), "{right:?}");

        // At 45 degrees on a square the line runs corner to corner.
        let square = Rect::new(0.0, 0.0, 10.0, 10.0);
        let diagonal = LinearGradient::angled(square, 45.0, Color::WHITE, Color::BLACK);
        assert!(close(diagonal.start, Point::new(0.0, 10.0)), "{diagonal:?}");
        assert!(close(diagonal.end, Point::new(10.0, 0.0)), "{diagonal:?}");
    }

    #[test]
    fn shadows_are_sanitised() {
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
        let mut list = DisplayList::new();
        list.draw_shadow(
            rect,
            0.0,
            Shadow::new(0.0, 0.0, 4.0, Color::rgba(0.0, 0.0, 0.0, 0.0)),
        );
        list.draw_shadow(rect, 0.0, Shadow::new(f32::NAN, 0.0, 4.0, Color::BLACK));
        assert!(list.is_empty());

        list.draw_shadow(
            rect,
            f32::NAN,
            Shadow::new(0.0, 2.0, -5.0, Color::BLACK).with_spread(f32::INFINITY),
        );
        assert!(matches!(
            list.ops()[0],
            DrawOp::Shadow { corner_radius, shadow, .. }
                if corner_radius == 0.0 && shadow.blur == 0.0 && shadow.spread == 0.0
        ));
    }

    #[test]
    fn clips_are_recorded_even_when_empty() {
        let mut list = DisplayList::new();
        // An empty clip is not a no-op: it hides what follows.
        list.push_clip(Rect::new(0.0, 0.0, 0.0, 0.0));
        list.push_rounded_clip(Rect::new(0.0, 0.0, 8.0, 8.0), 2.0);
        list.pop_clip();
        list.pop_clip();
        assert_eq!(list.len(), 4);
        assert!(
            matches!(list.ops()[0], DrawOp::PushClip { corner_radius, .. } if corner_radius == 0.0)
        );
        assert!(
            matches!(list.ops()[1], DrawOp::PushClip { corner_radius, .. } if corner_radius == 2.0)
        );
        assert_eq!(list.ops()[3], DrawOp::PopClip);
    }

    #[test]
    fn transforms_are_recorded_as_given() {
        let mut list = DisplayList::new();
        let turn = Transform::rotate(-8.0).about(Point::new(4.0, 4.0));
        list.push_transform(turn);
        list.fill_rect(Rect::new(0.0, 0.0, 8.0, 8.0), Color::WHITE);
        list.pop_transform();
        assert_eq!(list.len(), 3);
        assert_eq!(list.ops()[0], DrawOp::PushTransform { transform: turn });
        assert_eq!(list.ops()[2], DrawOp::PopTransform);
    }

    #[test]
    fn image_operations_carry_the_fitted_placement() {
        let mut store = crate::ImageStore::new();
        let image = store.add_rgba8(4, 2, &[255; 32]).unwrap();
        let bounds = Rect::new(10.0, 10.0, 20.0, 20.0);
        let mut list = DisplayList::new();

        list.draw_image(image, bounds);
        assert_eq!(
            list.ops()[0],
            DrawOp::Image {
                image,
                rect: bounds,
                source: Rect::new(0.0, 0.0, 4.0, 2.0),
                corner_radius: 0.0,
                tint: Color::WHITE,
            }
        );

        let tint = Color::rgba(1.0, 0.0, 0.0, 0.5);
        list.draw_image_fit(image, bounds, ImageFit::Cover, 10.0, tint);
        assert_eq!(
            list.ops()[1],
            DrawOp::Image {
                image,
                rect: bounds,
                source: Rect::new(1.0, 0.0, 2.0, 2.0),
                corner_radius: 10.0,
                tint,
            }
        );

        list.draw_image_fit(image, bounds, ImageFit::Contain, f32::NAN, Color::WHITE);
        assert!(matches!(
            list.ops()[2],
            DrawOp::Image { rect, source, corner_radius, .. }
                if rect == Rect::new(10.0, 15.0, 20.0, 10.0)
                    && source == image.bounds()
                    && corner_radius == 0.0
        ));
    }

    #[test]
    fn invisible_images_are_dropped() {
        let mut store = crate::ImageStore::new();
        let image = store.add_rgba8(1, 1, &[255; 4]).unwrap();
        let rect = Rect::new(0.0, 0.0, 8.0, 8.0);
        let mut list = DisplayList::new();
        list.draw_image(image, Rect::new(0.0, 0.0, 0.0, 8.0));
        list.draw_image_region(
            image,
            Rect::new(0.0, 0.0, 0.0, 1.0),
            rect,
            0.0,
            Color::WHITE,
        );
        list.draw_image_region(
            image,
            Rect::new(f32::NAN, 0.0, 1.0, 1.0),
            rect,
            0.0,
            Color::WHITE,
        );
        list.draw_image_region(
            image,
            image.bounds(),
            rect,
            0.0,
            Color::rgba(1.0, 1.0, 1.0, 0.0),
        );
        list.draw_image_fit(
            image,
            rect,
            ImageFit::Cover,
            0.0,
            Color::rgba(1.0, 1.0, 1.0, f32::NAN),
        );
        assert!(list.is_empty());
    }

    #[test]
    fn empty_atlas_view_has_no_dirty_record() {
        assert_eq!(AtlasView::EMPTY.dirty, None);
        let view = AtlasView::new(2, 1, &[1, 2], 7);
        assert_eq!((view.width, view.height, view.generation), (2, 1, 7));
        assert_eq!(view.dirty, None);
    }
}
