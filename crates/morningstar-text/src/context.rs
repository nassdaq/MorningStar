use std::collections::HashMap;
use std::hash::{BuildHasher, Hash, Hasher, RandomState};

use cosmic_text::{
    Align, Attrs, Buffer, CacheKey, Family, FontSystem, Metrics, Shaping, SwashCache, SwashContent,
    Weight, fontdb,
};
use morningstar_core::{AtlasView, Color, DisplayList, Point, Rect, Size};

use crate::atlas::{Atlas, Lookup};
use crate::icon::{Art, IconError, IconKey, MAX_ICON_PIXELS, VectorIcon};
use crate::{FontFamily, TextAlign, TextSpan, TextStyle};

/// One glyph of a laid-out paragraph, relative to the paragraph's origin.
#[derive(Clone, Copy)]
struct PlacedGlyph {
    key: CacheKey,
    /// Pen position: horizontal offset and baseline.
    x: i32,
    y: i32,
    /// The colour of the span the glyph belongs to, if the span sets one.
    color: Option<Color>,
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
        }
    }

    /// Adds every face in a TrueType/OpenType file or collection. Fonts added
    /// earlier are preferred; later ones serve as fallback for characters the
    /// earlier ones lack.
    pub fn add_font(&mut self, data: &[u8]) {
        self.fonts.db_mut().load_font_data(data.to_vec());
        // Cached layouts may have used a fallback this font now replaces.
        self.layouts.clear();
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
    /// `origin` is snapped to whole pixels so glyphs stay sharp.
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
        let layout = &self.layouts[&key];
        let (origin_x, origin_y) = (origin.x.round() as i32, origin.y.round() as i32);

        for glyph in &layout.glyphs {
            let slot = match self.atlas.lookup(glyph.key) {
                Lookup::Found(slot) => slot,
                Lookup::Blank => continue,
                Lookup::Missing => {
                    match rasterise(
                        &mut self.fonts,
                        &mut self.rasteriser,
                        &mut self.atlas,
                        glyph.key,
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
                    (origin_x + glyph.x + slot.left) as f32,
                    (origin_y + glyph.y - slot.top) as f32,
                    slot.width as f32,
                    slot.height as f32,
                ),
                Rect::new(
                    slot.x as f32,
                    slot.y as f32,
                    slot.width as f32,
                    slot.height as f32,
                ),
                glyph.color.unwrap_or(style.color),
            );
        }
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
                for glyph in run.glyphs {
                    map.stops.push(Stop {
                        start: base + glyph.start,
                        end: base + glyph.end,
                        x: glyph.x,
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
            span.color
                .map(|color| [color.r, color.g, color.b, color.a].map(f32::to_bits))
                .hash(&mut hasher);
        }
        style.size.to_bits().hash(&mut hasher);
        style.line_height.to_bits().hash(&mut hasher);
        style.weight.hash(&mut hasher);
        style.align.hash(&mut hasher);
        style.family.hash(&mut hasher);
        style.letter_spacing.to_bits().hash(&mut hasher);
        max_width.map(f32::to_bits).hash(&mut hasher);
        let key = hasher.finish();

        if let Some(layout) = self.layouts.get_mut(&key)
            && same_text(&layout.text, spans)
        {
            layout.last_used = self.frame;
            return key;
        }

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

        let mut glyphs = Vec::new();
        let mut size = Size::ZERO;
        for run in buffer.layout_runs() {
            for glyph in run.glyphs {
                // The baseline goes to the nearest pixel row. (Left as it
                // is, the shaper would truncate it, which sets a line whose
                // baseline falls at 32.9 a pixel higher than one at 33.0.)
                let physical = glyph.physical((0.0, run.line_y.round()), 1.0);
                glyphs.push(PlacedGlyph {
                    key: physical.cache_key,
                    x: physical.x,
                    y: physical.y,
                    color: spans.get(glyph.metadata).and_then(|span| span.color),
                });
            }
            size.width = size.width.max(run.line_w);
            size.height = size.height.max(run.line_top + run.line_height);
        }
        (glyphs, size)
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

        let family = |family: FontFamily| match family {
            FontFamily::Default => Family::SansSerif,
            FontFamily::Named(name) => Family::Name(name),
        };
        let mut attrs = Attrs::new()
            .family(family(style.family))
            .weight(Weight(style.weight.0));
        if style.letter_spacing.is_finite() && style.letter_spacing != 0.0 {
            // The shaper takes it as a fraction of the font size.
            attrs = attrs.letter_spacing(style.letter_spacing / font_size);
        }
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
        {
            buffer.set_text(&mut self.fonts, span.text, &attrs, Shaping::Advanced, align);
        } else {
            // `metadata` carries the span's index to its glyphs.
            let rich = spans.iter().enumerate().map(|(index, span)| {
                let mut attrs = attrs.clone().metadata(index);
                if let Some(weight) = span.weight {
                    attrs = attrs.weight(Weight(weight.0));
                }
                if let Some(name) = span.family {
                    attrs = attrs.family(family(name));
                }
                (span.text, attrs)
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

fn rasterise(
    fonts: &mut FontSystem,
    rasteriser: &mut SwashCache,
    atlas: &mut Atlas,
    key: CacheKey,
) -> Rasterised {
    let image = rasteriser.get_image_uncached(fonts, key);
    let Some(image) = image.filter(|image| {
        // Colour glyphs (emoji) and subpixel masks are not supported yet.
        image.content == SwashContent::Mask
            && image.placement.width > 0
            && image.placement.height > 0
    }) else {
        atlas.insert_blank(key);
        return Rasterised::Blank;
    };

    match atlas.insert(
        key,
        image.placement.width,
        image.placement.height,
        image.placement.left,
        image.placement.top,
        &image.data,
    ) {
        Some(slot) => Rasterised::Slot(slot),
        None => Rasterised::AtlasFull,
    }
}
