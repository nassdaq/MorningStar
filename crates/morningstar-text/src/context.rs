use std::collections::HashMap;
use std::hash::{BuildHasher, Hash, Hasher, RandomState};

use cosmic_text::{
    Align, Attrs, AttrsList, Buffer, CacheKey, CacheKeyFlags, Family, FeatureTag, FontFeatures,
    FontSystem, LayoutGlyph, LayoutRun, Metrics, ShapeWord, Shaping, Style, SubpixelBin,
    SwashCache, SwashContent, Weight, fontdb,
};
use morningstar_core::{AtlasView, Color, DisplayList, Point, Rect, Size, Transform};

use crate::atlas::{Atlas, AtlasKey, Lookup};
use crate::icon::{Art, IconError, IconKey, MAX_ICON_PIXELS, VectorIcon};
use crate::picture::{
    Canvas, DEFAULT_MITER_LIMIT, Ink, LineCap, LineJoin, MAX_PICTURE_PIXELS, Paint, StrokePaint,
};
use crate::{FontFamily, TextAlign, TextSpan, TextStyle};

/// Filled and outlined lettering as an image, from
/// [`TextContext::rasterise_outlined`].
#[derive(Clone, Debug, PartialEq)]
pub struct OutlinedText {
    /// Size of the image in pixels.
    pub width: u32,
    pub height: u32,
    /// Where the image's top-left corner goes, relative to the origin the
    /// text would be drawn at. Whole pixels; usually negative, since the
    /// stroke reaches outside the text's box.
    pub offset: Point,
    /// `width * height * 4` bytes of RGBA, row by row from the top:
    /// sRGB-encoded colour premultiplied by alpha, as the image store
    /// keeps images.
    pub pixels: Vec<u8>,
}

/// One glyph of a laid-out paragraph, relative to the paragraph's origin.
#[derive(Clone, Copy)]
struct PlacedGlyph {
    /// The glyph at a whole pixel; the horizontal sub-pixel bin is chosen
    /// when it is drawn, from where its paragraph is.
    key: CacheKey,
    /// Pen position from the paragraph's left edge, in fractional pixels.
    x: f32,
    /// The top of the glyph's line from the paragraph's top, unrounded.
    line_top: f32,
    /// From the line's top, once that is on a pixel row, down to the pen:
    /// a whole number of pixels (see [`baseline`]).
    rise: f32,
    /// Which span the glyph belongs to: its colour is the span's, looked
    /// up when the glyph is drawn, so that a paragraph whose colours change
    /// (a fade) keeps its shaping.
    span: u32,
}

/// A shaped and line-broken paragraph, kept between frames.
struct Layout {
    /// The spans' text, concatenated. Kept to detect hash collisions.
    text: String,
    glyphs: Vec<PlacedGlyph>,
    size: Size,
    last_used: u64,
    /// Where carets go. Built the first time it is asked for, which is only
    /// for a text field being edited.
    carets: Option<CaretMap>,
}

/// The hit box of one glyph: the bytes of the text its cluster covers and
/// where it sits on its line.
#[derive(Clone, Copy)]
struct Stop {
    start: usize,
    end: usize,
    x: f32,
    width: f32,
    rtl: bool,
}

/// One visual line of a paragraph.
struct CaretLine {
    top: f32,
    height: f32,
    /// The byte of the text this line starts at.
    start: usize,
    /// Its glyphs, as a range of `CaretMap::stops`.
    stops: std::ops::Range<usize>,
}

/// Everything needed to turn a byte offset into a caret position and a
/// touch position into a byte offset.
struct CaretMap {
    lines: Vec<CaretLine>,
    stops: Vec<Stop>,
}

impl CaretMap {
    fn caret(&self, index: usize) -> Rect {
        // The last line that starts at or before the index: at a soft wrap
        // the caret goes to the start of the following line.
        let Some(line) = self
            .lines
            .iter()
            .rev()
            .find(|line| line.start <= index)
            .or(self.lines.first())
        else {
            return Rect::new(0.0, 0.0, 0.0, 0.0);
        };
        let stops = &self.stops[line.stops.clone()];
        // A cluster can be several glyphs (a letter and its marks); the
        // caret moves across their union.
        let span = |matches: &dyn Fn(&Stop) -> bool| {
            let mut found: Option<(f32, f32, Stop)> = None;
            for stop in stops.iter().filter(|stop| matches(stop)) {
                let (left, right) = (stop.x, stop.x + stop.width);
                found = Some(match found {
                    Some((l, r, first)) => (l.min(left), r.max(right), first),
                    None => (left, right, *stop),
                });
            }
            found
        };
        let x = if let Some((left, right, stop)) =
            span(&|stop| stop.start <= index && index < stop.end)
        {
            let part = (index - stop.start) as f32 / (stop.end - stop.start).max(1) as f32;
            if stop.rtl {
                right - (right - left) * part
            } else {
                left + (right - left) * part
            }
        } else if let Some((left, right, stop)) = span(&|stop| stop.end == index) {
            // After the last cluster of the line.
            if stop.rtl { left } else { right }
        } else {
            0.0
        };
        Rect::new(x, line.top, 0.0, line.height)
    }

    /// The rectangles that cover bytes `from..to`, one or more per visual
    /// line: a line that mixes directions has one per run of the selection.
    fn selection(&self, from: usize, to: usize, out: &mut Vec<Rect>) {
        let mut spans: Vec<(f32, f32)> = Vec::new();
        for line in &self.lines {
            spans.clear();
            for stop in &self.stops[line.stops.clone()] {
                let (start, end) = (stop.start.max(from), stop.end.min(to));
                if start >= end || stop.width <= 0.0 {
                    continue;
                }
                // The part of the cluster that is selected, from its
                // leading edge.
                let length = (stop.end - stop.start).max(1) as f32;
                let near = (start - stop.start) as f32 / length * stop.width;
                let far = (end - stop.start) as f32 / length * stop.width;
                spans.push(if stop.rtl {
                    let right = stop.x + stop.width;
                    (right - far, right - near)
                } else {
                    (stop.x + near, stop.x + far)
                });
            }
            spans.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut current: Option<(f32, f32)> = None;
            for &(left, right) in &spans {
                current = Some(match current {
                    // Neighbours, give or take the rounding of positions.
                    Some((start, end)) if left <= end + 0.5 => (start, end.max(right)),
                    Some((start, end)) => {
                        out.push(Rect::new(start, line.top, end - start, line.height));
                        (left, right)
                    }
                    None => (left, right),
                });
            }
            if let Some((start, end)) = current {
                out.push(Rect::new(start, line.top, end - start, line.height));
            }
        }
    }

    fn index_at(&self, point: Point) -> usize {
        let Some(line) = self
            .lines
            .iter()
            .find(|line| point.y < line.top + line.height)
            .or(self.lines.last())
        else {
            return 0;
        };
        let stops = &self.stops[line.stops.clone()];
        let mut leftmost: Option<Stop> = None;
        let mut rightmost: Option<Stop> = None;
        for stop in stops.iter().filter(|stop| stop.width > 0.0) {
            if point.x >= stop.x && point.x < stop.x + stop.width {
                // The nearer edge of the glyph under the finger.
                let left_half = point.x < stop.x + stop.width / 2.0;
                return if left_half != stop.rtl {
                    stop.start
                } else {
                    stop.end
                };
            }
            if leftmost.is_none_or(|other| stop.x < other.x) {
                leftmost = Some(*stop);
            }
            if rightmost.is_none_or(|other| stop.x + stop.width > other.x + other.width) {
                rightmost = Some(*stop);
            }
        }
        match (leftmost, rightmost) {
            (Some(left), Some(right)) => {
                if point.x < left.x {
                    if left.rtl { left.end } else { left.start }
                } else if right.rtl {
                    right.start
                } else {
                    right.end
                }
            }
            _ => line.start,
        }
    }
}

/// Fonts, layout cache and glyph atlas.
///
/// Text is drawn in immediate style: call [`draw`](Self::draw) every frame.
/// Shaping, the expensive part, is cached per (text, size, weight, family,
/// letter spacing, alignment, width) and reused for as long as the same text
/// keeps being drawn; a paragraph that stops being drawn is dropped at the
/// next frame boundary.
///
/// The same atlas holds [vector icons](VectorIcon), drawn with
/// [`draw_icon`](Self::draw_icon).
pub struct TextContext {
    fonts: FontSystem,
    rasteriser: SwashCache,
    atlas: Atlas,
    layouts: HashMap<u64, Layout>,
    hasher: RandomState,
    frame: u64,
    /// Set when the atlas overflowed; it is rebuilt at the next frame start.
    atlas_full: bool,
    /// Every icon drawn so far, parsed once.
    icons: HashMap<VectorIcon, Result<Art, IconError>>,
    /// Kerning across a space, per font and pair of letters, in ems. See
    /// [`space_kerning`](Self::space_kerning).
    space_kerns: HashMap<SpaceKern, f32>,
    /// The texts drawn turned in the previous frame and in this one, by
    /// layout, each with the transforms (linear parts, quantised) it was
    /// drawn through. See [`draw_spans`](Self::draw_spans).
    turned_before: HashMap<u64, Vec<[i32; 4]>>,
    turned_now: HashMap<u64, Vec<[i32; 4]>>,
    /// Set when a turned text was drawn resampled because its transform
    /// had just changed: one more frame will draw it sharp.
    settle: bool,
    /// Paragraphs shaped since the context was made.
    shaped: u64,
}

/// Quantisation of a transform's linear part in a turned glyph's key:
/// steps of 1/4096, a seventieth of a degree.
const TURN_STEPS: f32 = 4096.0;
/// Largest glyph size in pixels, font size times the transform's scale,
/// that is rasterised turned. Larger text is resampled by the renderer.
const MAX_TURNED_SIZE: f32 = 160.0;

/// The linear part of `transform` as it keys turned glyphs, if text drawn
/// through it should be rasterised for it: it turns, slants or scales, can
/// be undone, and does not make `size` pixel glyphs too large.
fn turned_linear(transform: Transform, size: f32) -> Option<[i32; 4]> {
    if transform.is_translation() || !transform.is_finite() {
        return None;
    }
    let scale = transform.determinant().abs().sqrt();
    // Written so that NaN fails.
    if !(scale > 1.0e-3 && scale * size <= MAX_TURNED_SIZE) {
        return None;
    }
    let step = |value: f32| (value * TURN_STEPS).round();
    let quantised = [transform.a, transform.b, transform.c, transform.d].map(step);
    // Within the range of an `i32` by a wide margin, given the scale.
    quantised
        .iter()
        .all(|value| value.abs() < 1.0e9)
        .then(|| quantised.map(|value| value as i32))
}

impl Default for TextContext {
    fn default() -> Self {
        Self::new()
    }
}

impl TextContext {
    /// Creates a context with no fonts. Until a font is added, text measures
    /// as empty and draws nothing.
    pub fn new() -> Self {
        Self {
            // A fixed locale keeps layout identical on every device.
            fonts: FontSystem::new_with_locale_and_db("en-US".to_owned(), fontdb::Database::new()),
            rasteriser: SwashCache::new(),
            atlas: Atlas::new(),
            layouts: HashMap::new(),
            hasher: RandomState::new(),
            frame: 0,
            atlas_full: false,
            icons: HashMap::new(),
            space_kerns: HashMap::new(),
            turned_before: HashMap::new(),
            turned_now: HashMap::new(),
            settle: false,
            shaped: 0,
        }
    }

    /// Adds every face in a TrueType/OpenType file or collection. Fonts added
    /// earlier are preferred; later ones serve as fallback for characters the
    /// earlier ones lack.
    pub fn add_font(&mut self, data: &[u8]) {
        self.fonts.db_mut().load_font_data(data.to_vec());
        // Cached layouts may have used a fallback this font now replaces.
        self.layouts.clear();
        self.space_kerns.clear();
    }

    /// Adds the fonts installed on the device, as fallback after any added
    /// with [`add_font`](Self::add_font). This scans the system font
    /// directory, which takes noticeable time at startup; skip it if the
    /// app's own fonts cover its text.
    pub fn load_system_fonts(&mut self) {
        let db = self.fonts.db_mut();
        if cfg!(target_os = "android") {
            db.load_fonts_dir("/system/fonts");
        } else if cfg!(target_os = "ios") {
            db.load_fonts_dir("/System/Library/Fonts");
        } else {
            db.load_system_fonts();
        }
        self.layouts.clear();
    }

    /// Number of font faces available.
    pub fn font_count(&self) -> usize {
        self.fonts.db().len()
    }

    /// True if a font of this family has been added. Names are compared
    /// without regard to case, as they are when text is drawn.
    pub fn has_family(&self, name: &str) -> bool {
        self.fonts.db().faces().any(|face| {
            face.families
                .iter()
                .any(|(family, _)| family.eq_ignore_ascii_case(name))
        })
    }

    /// Marks a frame boundary: paragraphs not drawn or measured during the
    /// frame that just ended are forgotten.
    pub fn begin_frame(&mut self) {
        let previous = self.frame;
        self.layouts
            .retain(|_, layout| layout.last_used >= previous);
        self.frame += 1;
        std::mem::swap(&mut self.turned_before, &mut self.turned_now);
        self.turned_now.clear();
        self.settle = false;

        // The renderer has had the chance to upload the frame that just
        // ended; from here on the atlas records what changes relative to it.
        // Done before the rebuild below, which invalidates all of it.
        self.atlas.mark_clean();
        if self.atlas_full {
            // Start over with only what is on screen now. Glyphs in use are
            // re-rasterised as they are drawn this frame.
            self.atlas.clear();
            self.atlas_full = false;
        }
    }

    /// Size the text would occupy, without drawing it.
    ///
    /// With `max_width`, lines wrap at word boundaries to fit it. The
    /// returned width is that of the longest line, not `max_width`.
    pub fn measure(&mut self, text: &str, style: &TextStyle, max_width: Option<f32>) -> Size {
        self.measure_spans(&[TextSpan::new(text)], style, max_width)
    }

    /// Size a paragraph made of several spans would occupy. See
    /// [`draw_spans`](Self::draw_spans).
    pub fn measure_spans(
        &mut self,
        spans: &[TextSpan<'_>],
        style: &TextStyle,
        max_width: Option<f32>,
    ) -> Size {
        let key = self.layout(spans, style, max_width);
        self.layouts[&key].size
    }

    /// Draws text with the top-left of its layout box at `origin`, and
    /// returns the size it occupied.
    ///
    /// `origin` may be anywhere. Horizontally glyphs keep their fractional
    /// position to a quarter of a pixel, as browsers place them. Vertically
    /// each line's top goes to the nearest pixel row and its baseline a
    /// whole number of rows below (half the leading rounded down, then the
    /// font's rounded ascent), so text is always sharp across its stems'
    /// tops and feet. See ADR 0022.
    pub fn draw(
        &mut self,
        list: &mut DisplayList,
        text: &str,
        style: &TextStyle,
        origin: Point,
        max_width: Option<f32>,
    ) -> Size {
        self.draw_spans(list, &[TextSpan::new(text)], style, origin, max_width)
    }

    /// Draws one paragraph made of several spans, each of which may have
    /// its own weight, colour and family; the rest comes from `style`. The
    /// spans flow as one text: lines break anywhere a single string would
    /// break, including inside a span. Returns the size occupied.
    pub fn draw_spans(
        &mut self,
        list: &mut DisplayList,
        spans: &[TextSpan<'_>],
        style: &TextStyle,
        origin: Point,
        max_width: Option<f32>,
    ) -> Size {
        let key = self.layout(spans, style, max_width);
        let through = list.current_transform();
        if let Some(linear) = turned_linear(through, style.size)
            && self.admit_turned(key, linear)
        {
            return self.draw_turned(list, key, spans, style, origin, (through, linear));
        }
        let layout = &self.layouts[&key];
        // Positions far outside any surface would overflow the whole-pixel
        // arithmetic below; NaN draws at the origin.
        let limit = 1.0e7;
        let origin = Point::new(
            if origin.x.is_nan() {
                0.0
            } else {
                origin.x.clamp(-limit, limit)
            },
            if origin.y.is_nan() {
                0.0
            } else {
                origin.y.clamp(-limit, limit)
            },
        );

        let mut toned: Option<(Color, u8)> = None;
        for glyph in &layout.glyphs {
            // Horizontally the pen keeps its fraction, to a quarter of a
            // pixel: the glyph is rasterised for that offset.
            let (pen_x, x_bin) = SubpixelBin::new(origin.x + glyph.x);
            let pen_y = baseline(origin.y, glyph.line_top, glyph.rise) as i32;
            let glyph_key = CacheKey { x_bin, ..glyph.key };
            let color = spans
                .get(glyph.span as usize)
                .and_then(|span| span.color)
                .unwrap_or(style.color);
            // Runs of one colour are the rule: its tone is worked out once.
            let tone = match toned {
                Some((of, tone)) if of == color => tone,
                _ => {
                    let tone = tone(color);
                    toned = Some((color, tone));
                    tone
                }
            };
            let slot = match self.atlas.lookup((glyph_key, tone)) {
                Lookup::Found(slot) => slot,
                Lookup::Blank => continue,
                Lookup::Missing => {
                    match rasterise(
                        &mut self.fonts,
                        &mut self.rasteriser,
                        &mut self.atlas,
                        glyph_key,
                        tone,
                    ) {
                        Rasterised::Slot(slot) => slot,
                        Rasterised::Blank => continue,
                        Rasterised::AtlasFull => {
                            // Skip this glyph for one frame rather than
                            // corrupt glyphs already emitted.
                            self.atlas_full = true;
                            continue;
                        }
                    }
                }
            };

            list.draw_glyph(
                Rect::new(
                    (pen_x + slot.left) as f32,
                    (pen_y - slot.top) as f32,
                    slot.width as f32,
                    slot.height as f32,
                ),
                Rect::new(
                    slot.x as f32,
                    slot.y as f32,
                    slot.width as f32,
                    slot.height as f32,
                ),
                color,
            );
        }
        layout.size
    }

    /// Records that the layout `key` is drawn through `linear` in this
    /// frame, and decides how: true to rasterise its glyphs for the
    /// transform, false to leave the turning to the renderer.
    ///
    /// The policy: a text is rasterised for its transform unless the same
    /// text was drawn in the previous frame through other transforms and
    /// not this one. That is a text whose angle or scale is changing, an
    /// animation: every frame of it would put a whole new set of glyphs in
    /// the atlas for one frame's use, so it is resampled instead, and one
    /// more frame is asked for ([`take_settle_request`]), in which the
    /// transform, if it has stopped changing, is the previous frame's and
    /// the text is drawn sharp. A text that only moves keeps its linear
    /// part and stays sharp.
    ///
    /// [`take_settle_request`]: Self::take_settle_request
    fn admit_turned(&mut self, key: u64, linear: [i32; 4]) -> bool {
        let now = self.turned_now.entry(key).or_default();
        if !now.contains(&linear) {
            now.push(linear);
        }
        match self.turned_before.get(&key) {
            Some(before) if !before.contains(&linear) => {
                self.settle = true;
                false
            }
            _ => true,
        }
    }

    /// How many paragraphs have been shaped since the context was made:
    /// the expensive step, which a text drawn again unchanged skips. A
    /// change of colour alone, of the style or of a span, is not a change
    /// to the shaping. For tests and benchmarks.
    pub fn shaped_count(&self) -> u64 {
        self.shaped
    }

    /// True, once, if a text drawn since [`begin_frame`](Self::begin_frame)
    /// was left to the renderer to turn because its transform had just
    /// changed, and so wants one more frame to be drawn sharp in. Whoever
    /// schedules frames asks after drawing and schedules one if so.
    pub fn take_settle_request(&mut self) -> bool {
        std::mem::take(&mut self.settle)
    }

    /// Draws a laid-out text through the transform `through`, whose
    /// quantised linear part is `linear`: each glyph's outline is
    /// rasterised turned, slanted and scaled as the transform does it, for
    /// the fraction of a pixel its pen lands on, and drawn upright where
    /// the transform takes the pen, in screen space. The glyph is then as
    /// sharp as upright text, where the renderer turning an upright
    /// bitmap resamples it (ADR 0029).
    fn draw_turned(
        &mut self,
        list: &mut DisplayList,
        key: u64,
        spans: &[TextSpan<'_>],
        style: &TextStyle,
        origin: Point,
        (through, linear): (Transform, [i32; 4]),
    ) -> Size {
        let layout = &self.layouts[&key];
        let origin = Point::new(
            if origin.x.is_nan() { 0.0 } else { origin.x },
            if origin.y.is_nan() { 0.0 } else { origin.y },
        );
        list.push_screen_space();
        let mut toned: Option<(Color, u8)> = None;
        for glyph in &layout.glyphs {
            // The pen where upright text has it, without its quarter-pixel
            // rounding across (the rounding is made on screen, below), and
            // then through the transform.
            let pen = through.apply(Point::new(
                origin.x + glyph.x,
                baseline(origin.y, glyph.line_top, glyph.rise),
            ));
            // Written so that NaN is skipped.
            if !(pen.x.abs() < 1.0e7 && pen.y.abs() < 1.0e7) {
                continue;
            }
            let (pen_x, x_bin) = SubpixelBin::new(pen.x);
            let (pen_y, y_bin) = SubpixelBin::new(pen.y);
            let glyph_key = CacheKey {
                x_bin,
                y_bin,
                ..glyph.key
            };
            let color = spans
                .get(glyph.span as usize)
                .and_then(|span| span.color)
                .unwrap_or(style.color);
            let tone = match toned {
                Some((of, tone)) if of == color => tone,
                _ => {
                    let tone = tone(color);
                    toned = Some((color, tone));
                    tone
                }
            };
            let atlas_key = AtlasKey::Turned(glyph_key, tone, linear);
            let slot = match self.atlas.lookup(atlas_key) {
                Lookup::Found(slot) => slot,
                Lookup::Blank => continue,
                Lookup::Missing => {
                    match rasterise_turned(
                        &mut self.fonts,
                        &mut self.rasteriser,
                        &mut self.atlas,
                        glyph_key,
                        tone,
                        linear,
                    ) {
                        Rasterised::Slot(slot) => slot,
                        Rasterised::Blank => continue,
                        Rasterised::AtlasFull => {
                            self.atlas_full = true;
                            continue;
                        }
                    }
                }
            };
            list.draw_glyph(
                Rect::new(
                    (pen_x + slot.left) as f32,
                    (pen_y - slot.top) as f32,
                    slot.width as f32,
                    slot.height as f32,
                ),
                Rect::new(
                    slot.x as f32,
                    slot.y as f32,
                    slot.width as f32,
                    slot.height as f32,
                ),
                color,
            );
        }
        list.pop_screen_space();
        layout.size
    }

    /// Where a caret at byte offset `index` of `text` belongs, relative to
    /// the origin the text is drawn at with the same style and width: a
    /// rectangle of zero width and one line's height. `index` is clamped to
    /// the text and moved back to a character boundary. Right-to-left runs
    /// are followed: the caret after the last letter of an Arabic word is at
    /// the word's left edge.
    ///
    /// Empty text has one empty line: the caret is at the origin.
    pub fn caret(
        &mut self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f32>,
        index: usize,
    ) -> Rect {
        let mut index = index.min(text.len());
        while !text.is_char_boundary(index) {
            index -= 1;
        }
        match self.caret_map(text, style, max_width) {
            Some(map) => map.caret(index),
            None => Rect::new(0.0, 0.0, 0.0, line_height(style)),
        }
    }

    /// The byte offset in `text` where a caret belongs for a touch at
    /// `point`, relative to the origin the text is drawn at with the same
    /// style and width. The result is on a character boundary: the nearer
    /// edge of the character under the point, on the line under the point
    /// (the first or last line for a point above or below the text).
    pub fn index_at(
        &mut self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f32>,
        point: Point,
    ) -> usize {
        self.caret_map(text, style, max_width)
            .map_or(0, |map| map.index_at(point))
    }

    /// The rectangles a selection of bytes `from..to` of `text` covers,
    /// relative to the origin the text is drawn at with the same style and
    /// width, appended to `out`: one per visual line the selection touches,
    /// each one line tall, and more than one on a line where the selection
    /// is split by a change of direction (the middle of an Arabic word and
    /// the Latin word after it are not next to each other on screen). The
    /// offsets are clamped to the text and moved back to character
    /// boundaries; a line break inside the selection covers nothing.
    pub fn selection_rects(
        &mut self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f32>,
        from: usize,
        to: usize,
        out: &mut Vec<Rect>,
    ) {
        let clamp = |index: usize| {
            let mut index = index.min(text.len());
            while !text.is_char_boundary(index) {
                index -= 1;
            }
            index
        };
        let (from, to) = (clamp(from.min(to)), clamp(from.max(to)));
        if from < to
            && let Some(map) = self.caret_map(text, style, max_width)
        {
            map.selection(from, to, out);
        }
    }

    /// The caret map of a text, built on first use and kept with its layout.
    /// `None` for text that has no layout (empty, or no fonts).
    fn caret_map(
        &mut self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f32>,
    ) -> Option<&CaretMap> {
        let spans = [TextSpan::new(text)];
        let key = self.layout(&spans, style, max_width);
        if self.layouts[&key].carets.is_none() {
            // Shaping again is the price of not keeping every paragraph's
            // glyph boxes: it happens once per edit of the one focused
            // field.
            let buffer = self.buffer(&spans, style, max_width)?;
            let mut map = CaretMap {
                lines: Vec::new(),
                stops: Vec::new(),
            };
            // Glyph offsets are relative to their paragraph.
            let mut paragraph_starts = Vec::with_capacity(buffer.lines.len());
            let mut offset = 0;
            for line in &buffer.lines {
                paragraph_starts.push(offset);
                offset += line.text().len() + line.ending().as_str().len();
            }
            // Where the next visual line of the current paragraph starts.
            let mut cursor = (usize::MAX, 0);
            for run in buffer.layout_runs() {
                let base = paragraph_starts.get(run.line_i).copied().unwrap_or(0);
                if cursor.0 != run.line_i {
                    cursor = (run.line_i, base);
                }
                let first = map.stops.len();
                let mut end = cursor.1;
                let moved = self.space_kerning(&run, &spans, style);
                for (index, glyph) in run.glyphs.iter().enumerate() {
                    map.stops.push(Stop {
                        start: base + glyph.start,
                        end: base + glyph.end,
                        x: glyph.x + moved.at(index),
                        width: glyph.w,
                        rtl: glyph.level.is_rtl(),
                    });
                    end = end.max(base + glyph.end);
                }
                map.lines.push(CaretLine {
                    top: run.line_top,
                    height: run.line_height,
                    start: cursor.1,
                    stops: first..map.stops.len(),
                });
                cursor.1 = end;
            }
            self.layouts.get_mut(&key)?.carets = Some(map);
        }
        self.layouts[&key].carets.as_ref()
    }

    /// Draws a vector icon into `rect` (physical pixels), in `color`.
    ///
    /// The icon's view box is scaled uniformly to fit the rectangle and
    /// centred in it. The rectangle is snapped to whole pixels and the icon
    /// is rasterised for exactly that many pixels, so strokes are as sharp
    /// as the display allows at any scale. `stroke_width`, in the icon's
    /// view-box units, replaces the width its strokes were drawn with.
    ///
    /// The mask is kept in the glyph atlas, keyed by icon, pixel size and
    /// stroke width, and drawn as a glyph, so icons share text's single
    /// draw call. An icon that is outside the supported subset (see
    /// [`VectorIcon::check`]) draws nothing. Icons larger than 512 pixels
    /// are rasterised at 512 and stretched.
    pub fn draw_icon(
        &mut self,
        list: &mut DisplayList,
        icon: &VectorIcon,
        rect: Rect,
        color: Color,
        stroke_width: Option<f32>,
    ) {
        let (left, top) = (rect.x.round(), rect.y.round());
        let width = (rect.x + rect.width).round() - left;
        let height = (rect.y + rect.height).round() - top;
        // Written so that NaN is rejected.
        if !(width >= 1.0 && height >= 1.0 && width < 1.0e6 && height < 1.0e6) {
            return;
        }
        let (width, height) = (width as u32, height as u32);
        // Keep the mask within a size the atlas can hold.
        let longest = width.max(height);
        let (mask_width, mask_height) = if longest > MAX_ICON_PIXELS {
            let shrink = |side: u32| (side * MAX_ICON_PIXELS).div_ceil(longest).max(1);
            (shrink(width), shrink(height))
        } else {
            (width, height)
        };
        let stroke_width = stroke_width.filter(|width| width.is_finite() && *width > 0.0);
        let key = IconKey {
            icon: *icon,
            width: mask_width,
            height: mask_height,
            stroke: stroke_width.map(f32::to_bits),
        };

        let slot = match self.atlas.lookup(key) {
            Lookup::Found(slot) => slot,
            Lookup::Blank => return,
            Lookup::Missing => {
                let Ok(art) = self.icons.entry(*icon).or_insert_with(|| icon.parse()) else {
                    self.atlas.insert_blank(key);
                    return;
                };
                let coverage = art.rasterise(mask_width, mask_height, stroke_width);
                match self
                    .atlas
                    .insert(key, mask_width, mask_height, 0, 0, &coverage)
                {
                    Some(slot) => slot,
                    None => {
                        // As for glyphs: skip it for this frame and rebuild.
                        self.atlas_full = true;
                        return;
                    }
                }
            }
        };
        list.draw_glyph(
            Rect::new(left, top, width as f32, height as f32),
            Rect::new(
                slot.x as f32,
                slot.y as f32,
                slot.width as f32,
                slot.height as f32,
            ),
            color,
        );
    }

    /// Draws a loaded SVG as a one-colour icon into `rect` (physical
    /// pixels), in `color`, if it is one: returns false, having drawn
    /// nothing, for a picture that is not
    /// [single-ink](crate::SvgPicture::is_single_ink), which belongs on the
    /// image path instead.
    ///
    /// As for [`draw_icon`](Self::draw_icon): fitted and centred, snapped
    /// to whole pixels, rasterised for exactly that many, and kept in the
    /// glyph atlas (keyed by the picture's content and pixel size) so that
    /// it shares text's draw call and takes any colour without being
    /// rasterised again. The picture's own colour is not used. Larger than
    /// 512 pixels, it is rasterised at 512 and stretched.
    pub fn draw_svg_icon(
        &mut self,
        list: &mut DisplayList,
        picture: &crate::SvgPicture,
        rect: Rect,
        color: Color,
    ) -> bool {
        if !picture.is_single_ink() {
            return false;
        }
        let (left, top) = (rect.x.round(), rect.y.round());
        let width = (rect.x + rect.width).round() - left;
        let height = (rect.y + rect.height).round() - top;
        // Written so that NaN is rejected.
        if !(width >= 1.0 && height >= 1.0 && width < 1.0e6 && height < 1.0e6) {
            return true;
        }
        let (width, height) = (width as u32, height as u32);
        let longest = width.max(height);
        let (mask_width, mask_height) = if longest > MAX_ICON_PIXELS {
            let shrink = |side: u32| (side * MAX_ICON_PIXELS).div_ceil(longest).max(1);
            (shrink(width), shrink(height))
        } else {
            (width, height)
        };
        let key = crate::atlas::AtlasKey::Svg {
            // The mask does not depend on the current colour.
            content: picture.with_current_color(Color::BLACK).content_hash(),
            width: mask_width,
            height: mask_height,
        };
        let slot = match self.atlas.lookup(key) {
            Lookup::Found(slot) => slot,
            Lookup::Blank => return true,
            Lookup::Missing => {
                let coverage = picture
                    .with_current_color(Color::BLACK)
                    .rasterise_mask(mask_width, mask_height);
                match self
                    .atlas
                    .insert(key, mask_width, mask_height, 0, 0, &coverage)
                {
                    Some(slot) => slot,
                    None => {
                        self.atlas_full = true;
                        return true;
                    }
                }
            }
        };
        list.draw_glyph(
            Rect::new(left, top, width as f32, height as f32),
            Rect::new(
                slot.x as f32,
                slot.y as f32,
                slot.width as f32,
                slot.height as f32,
            ),
            color,
        );
        true
    }

    /// Rasterises a short text as filled and outlined lettering (CSS
    /// `-webkit-text-stroke`): the glyphs' outlines filled with the style's
    /// colour and then stroked, centred on the outline, `stroke_width`
    /// pixels wide in `stroke`.
    ///
    /// The result is an RGBA image (see [`OutlinedText`]) for the
    /// renderer's image path, not glyphs in the atlas: it is meant for a
    /// large digit or a word, and the caller keeps it for as long as it is
    /// shown. Per-span colours are not used; the whole text is filled with
    /// `style.color`. `None` if there is nothing to draw (no text, no
    /// fonts, only blank glyphs, or glyphs without outlines such as
    /// bitmaps).
    pub fn rasterise_outlined(
        &mut self,
        spans: &[TextSpan<'_>],
        style: &TextStyle,
        max_width: Option<f32>,
        stroke_width: f32,
        stroke: Color,
    ) -> Option<OutlinedText> {
        let stroke_width = if stroke_width > 0.0 && stroke_width.is_finite() {
            stroke_width
        } else {
            0.0
        };
        let key = self.layout(spans, style, max_width);
        let layout = &self.layouts[&key];

        // All the outlines as one path, relative to the layout's origin,
        // with y downwards.
        let mut path: Vec<zeno::Command> = Vec::new();
        for glyph in &layout.glyphs {
            let Some(commands) = self
                .rasteriser
                .get_outline_commands(&mut self.fonts, glyph.key)
            else {
                continue;
            };
            // Outlines are y-up, relative to the pen position. The image
            // is placed at a whole pixel, so the paragraph is taken to
            // start on one.
            let place = zeno::Transform::new(
                1.0,
                0.0,
                0.0,
                -1.0,
                glyph.x,
                baseline(0.0, glyph.line_top, glyph.rise),
            );
            path.extend(commands.iter().map(|command| command.transform(&place)));
        }

        // The pixels the lettering can touch: its control points bound it,
        // half the stroke lies outside (a mitre reaches further, up to the
        // limit of four half-widths), and a pixel for the edge.
        let mut bounds: Option<[f32; 4]> = None;
        let mut include = |point: zeno::Point| {
            let b = bounds.get_or_insert([point.x, point.y, point.x, point.y]);
            *b = [
                b[0].min(point.x),
                b[1].min(point.y),
                b[2].max(point.x),
                b[3].max(point.y),
            ];
        };
        for command in &path {
            match *command {
                zeno::Command::MoveTo(to) | zeno::Command::LineTo(to) => include(to),
                zeno::Command::QuadTo(control, to) => {
                    include(control);
                    include(to);
                }
                zeno::Command::CurveTo(first, second, to) => {
                    include(first);
                    include(second);
                    include(to);
                }
                zeno::Command::Close => {}
            }
        }
        let [x0, y0, x1, y1] = bounds?;
        let margin = (stroke_width * 2.0).ceil() + 1.0;
        let (left, top) = ((x0 - margin).floor(), (y0 - margin).floor());
        let (width, height) = ((x1 + margin).ceil() - left, (y1 + margin).ceil() - top);
        let limit = MAX_PICTURE_PIXELS as f32;
        // Written so that NaN is rejected.
        if !(width >= 1.0 && height >= 1.0 && width <= limit && height <= limit) {
            return None;
        }

        let mut canvas = Canvas::new(width as u32, height as u32);
        canvas.paint(&Paint {
            commands: &path,
            transform: Transform::translate(-left, -top),
            fill: Some(Ink::color(style.color)),
            even_odd: false,
            stroke: (stroke_width > 0.0).then_some(StrokePaint {
                ink: Ink::color(stroke),
                width: stroke_width,
                cap: LineCap::Butt,
                join: LineJoin::Miter,
                miter_limit: DEFAULT_MITER_LIMIT,
                dashes: &[],
                dash_offset: 0.0,
            }),
            opacity: 1.0,
            clips: &[],
        });
        Some(OutlinedText {
            width: width as u32,
            height: height as u32,
            offset: Point::new(left, top),
            pixels: canvas.finish(),
        })
    }

    /// The atlas the glyphs drawn so far refer to. Hand it to the renderer
    /// together with the display list.
    pub fn atlas(&self) -> AtlasView<'_> {
        self.atlas.view()
    }

    /// Returns the cache key of the layout for these inputs, shaping the
    /// text if it is not cached.
    fn layout(&mut self, spans: &[TextSpan<'_>], style: &TextStyle, max_width: Option<f32>) -> u64 {
        let mut hasher = self.hasher.build_hasher();
        spans.len().hash(&mut hasher);
        for span in spans {
            span.text.hash(&mut hasher);
            span.weight.hash(&mut hasher);
            span.family.hash(&mut hasher);
            span.italic.hash(&mut hasher);
            // Not the colour: it changes nothing about the shaping, and a
            // text fading in would otherwise be shaped anew every frame.
        }
        style.size.to_bits().hash(&mut hasher);
        style.line_height.to_bits().hash(&mut hasher);
        style.weight.hash(&mut hasher);
        style.align.hash(&mut hasher);
        style.family.hash(&mut hasher);
        style.letter_spacing.to_bits().hash(&mut hasher);
        style.italic.hash(&mut hasher);
        style.tabular_nums.hash(&mut hasher);
        max_width.map(f32::to_bits).hash(&mut hasher);
        let key = hasher.finish();

        if let Some(layout) = self.layouts.get_mut(&key)
            && same_text(&layout.text, spans)
        {
            layout.last_used = self.frame;
            return key;
        }

        self.shaped += 1;
        let (glyphs, size) = self.shape(spans, style, max_width);
        self.layouts.insert(
            key,
            Layout {
                text: spans.iter().map(|span| span.text).collect(),
                glyphs,
                size,
                last_used: self.frame,
                carets: None,
            },
        );
        key
    }

    fn shape(
        &mut self,
        spans: &[TextSpan<'_>],
        style: &TextStyle,
        max_width: Option<f32>,
    ) -> (Vec<PlacedGlyph>, Size) {
        let Some(buffer) = self.buffer(spans, style, max_width) else {
            return (Vec::new(), Size::ZERO);
        };

        // The paragraph's own font decides where baselines go (CSS calls
        // it the strut): the font of the first glyph that belongs to a
        // span without a family of its own, else of the first glyph.
        let strut = buffer
            .layout_runs()
            .flat_map(|run| run.glyphs.iter())
            .find(|glyph| {
                spans
                    .get(glyph.metadata)
                    .is_none_or(|span| span.family.is_none())
            })
            .or_else(|| buffer.layout_runs().find_map(|run| run.glyphs.first()))
            .map(|glyph| (glyph.font_id, glyph.font_weight, glyph.font_size));
        let strut = strut.and_then(|(id, weight, size)| {
            let font = self.fonts.get_font(id, weight)?;
            let metrics = font.metrics();
            let units = f32::from(metrics.units_per_em);
            (units > 0.0)
                .then(|| Strut::new(metrics.ascent / units, -metrics.descent / units, size))
        });

        let mut glyphs = Vec::new();
        let mut size = Size::ZERO;
        for run in buffer.layout_runs() {
            let rise = match strut {
                Some(strut) => strut.rise(run.line_height),
                // No metrics: the shaper's baseline, on a pixel row.
                None => (run.line_y - run.line_top).round(),
            };
            let moved = self.space_kerning(&run, spans, style);
            for (index, glyph) in run.glyphs.iter().enumerate() {
                let (key, _, _) = CacheKey::new(
                    glyph.font_id,
                    glyph.glyph_id,
                    glyph.font_size,
                    (0.0, 0.0),
                    glyph.font_weight,
                    glyph.cache_key_flags,
                );
                glyphs.push(PlacedGlyph {
                    key,
                    x: glyph.x + glyph.font_size * glyph.x_offset + moved.at(index),
                    line_top: run.line_top,
                    // Marks raised or lowered by the font stay on whole
                    // pixels with their base.
                    rise: rise + (glyph.y - glyph.font_size * glyph.y_offset).round(),
                    span: glyph.metadata as u32,
                });
            }
            size.width = size.width.max(run.line_w + moved.total);
            size.height = size.height.max(run.line_top + run.line_height);
        }
        (glyphs, size)
    }

    /// How far each glyph of a line moves for the kerning its font has
    /// between a letter and a space, or a space and a letter.
    ///
    /// The shaper shapes word by word and spaces by themselves, so such
    /// pairs (Outfit closes up "f A" by 4% of an em) are never seen by it;
    /// a browser shapes the run whole. Each pair is shaped once here as
    /// one piece, compared with its parts, and remembered. Lines are still
    /// broken by the unkerned widths, and right-to-left lines are left as
    /// they are.
    fn space_kerning(
        &mut self,
        run: &LayoutRun<'_>,
        spans: &[TextSpan<'_>],
        style: &TextStyle,
    ) -> Moved {
        let mut moved = Moved::default();
        if run.rtl || run.glyphs.len() < 3 {
            return moved;
        }
        let text_of = |glyph: &LayoutGlyph| run.text.get(glyph.start..glyph.end).unwrap_or("");
        for index in 1..run.glyphs.len() - 1 {
            let [before, space, after] =
                [-1, 0, 1].map(|d| &run.glyphs[(index as isize + d) as usize]);
            let (Some(left), Some(right)) = (
                text_of(before).chars().next_back(),
                text_of(after).chars().next(),
            ) else {
                continue;
            };
            if text_of(space) != " "
                || left.is_whitespace()
                || right.is_whitespace()
                || before.font_id != after.font_id
                || before.font_id != space.font_id
                || before.font_weight != after.font_weight
                || before.font_size != after.font_size
                || before.metadata != after.metadata
                || [before, space, after].iter().any(|glyph| glyph.level.is_rtl())
                // In visual order, which for left-to-right text is the
                // order of the bytes.
                || !(before.start < space.start && space.start < after.start)
            {
                continue;
            }
            let key = SpaceKern {
                font: before.font_id,
                weight: before.font_weight.0,
                tabular: style.tabular_nums,
                left,
                right,
            };
            let ems = match self.space_kerns.get(&key) {
                Some(ems) => *ems,
                None => {
                    let attrs = attrs_of(style, spans.get(before.metadata), false);
                    let attrs = AttrsList::new(&attrs);
                    let fonts = &mut self.fonts;
                    let mut advance = |piece: &str| {
                        let word = ShapeWord::new(
                            fonts,
                            piece,
                            &attrs,
                            0..piece.len(),
                            before.level,
                            false,
                            Shaping::Advanced,
                        );
                        // A piece that fell back to another font says
                        // nothing about this one.
                        word.glyphs
                            .iter()
                            .all(|glyph| glyph.font_id == key.font)
                            .then(|| word.glyphs.iter().map(|glyph| glyph.x_advance).sum::<f32>())
                    };
                    let mut buffer = [0u8; 4];
                    let whole = format!("{left} {right}");
                    let ems = (|| {
                        Some(
                            advance(&whole)?
                                - advance(left.encode_utf8(&mut buffer))?
                                - advance(" ")?
                                - advance(right.encode_utf8(&mut buffer))?,
                        )
                    })()
                    .filter(|ems| ems.is_finite() && ems.abs() < 1.0)
                    .unwrap_or(0.0);
                    self.space_kerns.insert(key, ems);
                    ems
                }
            };
            if ems != 0.0 {
                moved.from.push((index + 1, ems * before.font_size));
            }
        }
        moved.total = moved.from.iter().map(|(_, by)| by).sum();
        // A line that got narrower keeps its alignment.
        moved.line = match style.align {
            TextAlign::Start => 0.0,
            TextAlign::Center => -moved.total / 2.0,
            TextAlign::End => -moved.total,
        };
        moved
    }

    /// Shapes and line-breaks the spans. `None` when there is nothing to
    /// lay out: no text, or no fonts.
    fn buffer(
        &mut self,
        spans: &[TextSpan<'_>],
        style: &TextStyle,
        max_width: Option<f32>,
    ) -> Option<Buffer> {
        // Guard against sizes the layout engine rejects (zero, negative, NaN).
        let font_size = if style.size > 0.0 { style.size } else { 1.0 };
        let line_height = line_height(style);
        if spans.iter().all(|span| span.text.is_empty()) || self.fonts.db().is_empty() {
            return None;
        }

        let attrs = attrs_of(style, None, true);
        let align = match style.align {
            // `None` lets each paragraph follow its own direction.
            TextAlign::Start => None,
            TextAlign::Center => Some(Align::Center),
            TextAlign::End => Some(Align::End),
        };

        let mut buffer = Buffer::new_empty(Metrics::new(font_size, line_height));
        buffer.set_size(&mut self.fonts, max_width, None);
        if let [span] = spans
            && span.weight.is_none()
            && span.family.is_none()
            && span.italic.is_none()
        {
            buffer.set_text(&mut self.fonts, span.text, &attrs, Shaping::Advanced, align);
        } else {
            // `metadata` carries the span's index to its glyphs.
            let rich = spans.iter().enumerate().map(|(index, span)| {
                (span.text, attrs_of(style, Some(span), true).metadata(index))
            });
            buffer.set_rich_text(&mut self.fonts, rich, &attrs, Shaping::Advanced, align);
        }
        buffer.shape_until_scroll(&mut self.fonts, false);
        Some(buffer)
    }
}

/// The height of one line of text in `style`, in the units of its size.
fn line_height(style: &TextStyle) -> f32 {
    let font_size = if style.size > 0.0 { style.size } else { 1.0 };
    if style.line_height > 0.0 {
        font_size * style.line_height
    } else {
        font_size
    }
}

/// What identifies the kerning of a letter, a space and a letter.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct SpaceKern {
    font: fontdb::ID,
    weight: u16,
    tabular: bool,
    left: char,
    right: char,
}

/// Horizontal corrections to the glyphs of one line.
#[derive(Default)]
struct Moved {
    /// From this glyph of the line on, this many more pixels.
    from: Vec<(usize, f32)>,
    /// The change of the line's width.
    total: f32,
    /// The move of the whole line that keeps it aligned.
    line: f32,
}

impl Moved {
    fn at(&self, glyph: usize) -> f32 {
        self.line
            + self
                .from
                .iter()
                .take_while(|(from, _)| *from <= glyph)
                .map(|(_, by)| by)
                .sum::<f32>()
    }
}

/// The shaper's attributes for a style, and for a span of it.
fn attrs_of<'a>(style: &TextStyle, span: Option<&TextSpan<'_>>, letter_spacing: bool) -> Attrs<'a> {
    let family = |family: FontFamily| match family {
        FontFamily::Default => Family::SansSerif,
        FontFamily::Named(name) => Family::Name(name),
    };
    // Asking for italic picks an italic face when the family has one;
    // when it has none the shaper marks the glyphs to be slanted as
    // they are rasterised (and as their outlines are produced).
    let slant = |italic: bool| {
        if italic { Style::Italic } else { Style::Normal }
    };
    let font_size = if style.size > 0.0 { style.size } else { 1.0 };
    let mut attrs = Attrs::new()
        // Outlines as the font has them, not fitted to the pixel grid: a
        // browser on a dense screen does not hint either, and hinted stems
        // would not sit where fractional positions put them (ADR 0022).
        .cache_key_flags(CacheKeyFlags::DISABLE_HINTING)
        .family(family(
            span.and_then(|span| span.family).unwrap_or(style.family),
        ))
        .weight(Weight(
            span.and_then(|span| span.weight).unwrap_or(style.weight).0,
        ))
        .style(slant(
            span.and_then(|span| span.italic).unwrap_or(style.italic),
        ));
    let spaced = style.letter_spacing.is_finite() && style.letter_spacing != 0.0;
    if style.tabular_nums || spaced {
        let mut features = FontFeatures::new();
        if style.tabular_nums {
            features.enable(FeatureTag::new(b"tnum"));
        }
        if spaced {
            // CSS: "when the effective letter-spacing between two
            // characters is not zero, user agents should not apply
            // optional ligatures". Spaced-out letters would otherwise keep
            // an "fi" joined in the middle of a word.
            features.disable(FeatureTag::new(b"liga"));
            features.disable(FeatureTag::new(b"clig"));
        }
        attrs = attrs.font_features(features);
    }
    if letter_spacing && spaced {
        // The shaper takes it as a fraction of the font size.
        attrs = attrs.letter_spacing(style.letter_spacing / font_size);
    }
    attrs
}

/// A paragraph font's vertical metrics in pixels, each rounded to a whole
/// pixel as a browser rounds them.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Strut {
    ascent: f32,
    descent: f32,
}

impl Strut {
    /// `ascent` and `descent` as fractions of the font size, both positive.
    fn new(ascent: f32, descent: f32, font_size: f32) -> Self {
        Self {
            ascent: (ascent * font_size).round(),
            descent: (descent * font_size).round(),
        }
    }

    /// From the top of a line `line_height` pixels tall down to its
    /// baseline: half the leading, rounded down to a whole pixel (also when
    /// it is negative, for a line shorter than its font), and the ascent.
    ///
    /// This is the rule Chrome's line boxes follow, derived from the
    /// measured boxes in `examples/callready/design/layout` (ADR 0022):
    /// 27 px Outfit on a 35.1 px line at 2 px per dp has 1.1 px of
    /// half-leading and starts 1 px down; 32 px bold on a 35.84 px line
    /// has -4.66 px and starts 5 px up.
    fn rise(self, line_height: f32) -> f32 {
        ((line_height - (self.ascent + self.descent)) / 2.0).floor() + self.ascent
    }
}

/// The pixel row of a baseline: the line's top goes to the nearest row
/// (halves down the page, as a browser's layout unit rounds), and the
/// baseline is a whole number of rows below it. So lines of a paragraph
/// with a fractional line height are not evenly spaced: each is where its
/// own top rounds to.
fn baseline(origin_y: f32, line_top: f32, rise: f32) -> f32 {
    (origin_y + line_top + 0.5).floor() + rise
}

/// True if the spans' text, joined, is `stored`.
fn same_text(stored: &str, spans: &[TextSpan<'_>]) -> bool {
    let mut rest = stored;
    for span in spans {
        match rest.strip_prefix(span.text) {
            Some(tail) => rest = tail,
            None => return false,
        }
    }
    rest.is_empty()
}

enum Rasterised {
    Slot(crate::atlas::Slot),
    Blank,
    AtlasFull,
}

/// How many tones glyph images are weighted for, from dark text to light.
const TONES: u8 = 4;

/// The exponent's divisor for white text; black text has 1 and tones
/// between lie evenly between. Fitted to the Chrome references of both
/// example apps (ADR 0023): the share of differing pixels over all
/// seventeen screens falls from 3.38 to 2.89 between 1 and 3 and is flat
/// beyond.
const LIGHT_TEXT_GAMMA: f32 = 3.0;

/// The tone a glyph image is weighted for when drawn in `color`: 0 for
/// dark text up to `TONES - 1` for white, by the luma of its encoded
/// colour.
///
/// A browser's text is not its outline's plain coverage: light text on
/// dark is drawn visibly heavier (font smoothing; Skia's text gamma), and
/// compositing encoded values, unlike blending in linear light, does not
/// produce that by itself. Coverage `c` becomes `c^(1 / g)`, with `g` from
/// 1 for black text to [`LIGHT_TEXT_GAMMA`] for white. Icons and pictures
/// are vector paths, which a browser does not weight, and are left alone.
fn tone(color: Color) -> u8 {
    let [r, g, b, _] = color.srgb();
    let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    // The float-to-integer cast saturates.
    ((luma * f32::from(TONES - 1) + 0.5) as u8).min(TONES - 1)
}

/// Coverage to weighted coverage for a tone above 0.
fn tone_curve(tone: u8) -> &'static [u8; 256] {
    static CURVES: std::sync::OnceLock<Vec<[u8; 256]>> = std::sync::OnceLock::new();
    let curves = CURVES.get_or_init(|| {
        (1..TONES)
            .map(|tone| {
                let share = f32::from(tone) / f32::from(TONES - 1);
                let exponent = 1.0 / (1.0 + (LIGHT_TEXT_GAMMA - 1.0) * share);
                std::array::from_fn(|coverage| {
                    ((coverage as f32 / 255.0).powf(exponent) * 255.0 + 0.5) as u8
                })
            })
            .collect()
    });
    &curves[usize::from(tone.clamp(1, TONES - 1)) - 1]
}

/// Rasterises a glyph's outline through `linear` (a transform's linear
/// part, in 4096ths) for the pen fractions in `key`'s bins, and puts it in
/// the atlas. The outline is the one upright text is filled from, with the
/// synthetic slant if the glyph has one, unhinted.
fn rasterise_turned(
    fonts: &mut FontSystem,
    rasteriser: &mut SwashCache,
    atlas: &mut Atlas,
    key: CacheKey,
    tone: u8,
    linear: [i32; 4],
) -> Rasterised {
    let atlas_key = AtlasKey::Turned(key, tone, linear);
    // One outline per glyph, whatever the fraction.
    let outline = CacheKey {
        x_bin: SubpixelBin::Zero,
        y_bin: SubpixelBin::Zero,
        ..key
    };
    let Some(commands) = rasteriser.get_outline_commands(fonts, outline) else {
        atlas.insert_blank(atlas_key);
        return Rasterised::Blank;
    };
    let [a, b, c, d] = linear.map(|value| value as f32 / TURN_STEPS);
    // The outline is y-up about the pen; the screen is y-down. Flip, then
    // the transform's linear part, then the pen's fraction of a pixel.
    let place = zeno::Transform::new(a, b, -c, -d, key.x_bin.as_float(), key.y_bin.as_float());
    let (coverage, placement) = zeno::Mask::new(commands).transform(Some(place)).render();
    if placement.width == 0 || placement.height == 0 {
        atlas.insert_blank(atlas_key);
        return Rasterised::Blank;
    }
    let weighted;
    let data = if tone == 0 {
        &coverage
    } else {
        let curve = tone_curve(tone);
        weighted = coverage
            .iter()
            .map(|&coverage| curve[usize::from(coverage)])
            .collect::<Vec<u8>>();
        &weighted
    };
    match atlas.insert(
        atlas_key,
        placement.width,
        placement.height,
        placement.left,
        // The slot's `top` is measured upwards; the mask's downwards.
        -placement.top,
        data,
    ) {
        Some(slot) => Rasterised::Slot(slot),
        None => Rasterised::AtlasFull,
    }
}

fn rasterise(
    fonts: &mut FontSystem,
    rasteriser: &mut SwashCache,
    atlas: &mut Atlas,
    key: CacheKey,
    tone: u8,
) -> Rasterised {
    let image = rasteriser.get_image_uncached(fonts, key);
    let Some(image) = image.filter(|image| {
        // Colour glyphs (emoji) and subpixel masks are not supported yet.
        image.content == SwashContent::Mask
            && image.placement.width > 0
            && image.placement.height > 0
    }) else {
        atlas.insert_blank((key, tone));
        return Rasterised::Blank;
    };

    let weighted;
    let data = if tone == 0 {
        &image.data
    } else {
        let curve = tone_curve(tone);
        weighted = image
            .data
            .iter()
            .map(|&coverage| curve[usize::from(coverage)])
            .collect::<Vec<u8>>();
        &weighted
    };
    match atlas.insert(
        (key, tone),
        image.placement.width,
        image.placement.height,
        image.placement.left,
        image.placement.top,
        data,
    ) {
        Some(slot) => Rasterised::Slot(slot),
        None => Rasterised::AtlasFull,
    }
}
