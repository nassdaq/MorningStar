use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::Duration;

use morningstar_core::{
    AtlasView, Color, DisplayList, HostState, Image, ImageError, ImageStore, ImagesView, Insets,
    Point, PointerEvent, Rect, ServiceCall, ServiceId, ServiceResult, Size, SurfaceSize,
    SystemBars, TextInputEvent, TextInputRequest,
};
use morningstar_text::{
    MAX_PICTURE_PIXELS, RasterPicture, SvgPicture, TextContext, TextSpan, TextStyle, VectorIcon,
};

/// A cached picture is dropped once this many frames have been built
/// without drawing it. Frames are only built when something changes, so
/// this is about how much else has happened, not time: a picture scrolled
/// out of view and back within a fling is not rasterised again.
const PICTURE_IDLE_FRAMES: u64 = 120;
/// Above this many bytes of cached picture pixels, pictures the previous
/// frame did not draw are dropped at once, least recently drawn first.
const PICTURE_CACHE_BYTES: usize = 32 * 1024 * 1024;

/// Identifies one rasterisation: what was drawn and at what pixel size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct PictureKey {
    content: u64,
    width: u32,
    height: u32,
}

/// A vector picture (or outlined text) rasterised and registered as an
/// image.
struct CachedPicture {
    image: Image,
    /// For outlined text: where the image goes relative to the text's
    /// origin.
    offset: Point,
    /// The frame that last drew it.
    last_used: u64,
}

/// Per-frame information handed to the application.
///
/// Construct one with [`FrameInfo::new`] and the `with_*` methods; the struct
/// gains fields over time, so it cannot be written as a literal outside this
/// crate.
///
/// # Units
///
/// Everything an app draws, and every pointer position it receives, is in
/// **physical pixels**. Designs are specified in density-independent units
/// (dp on Android, points on iOS): multiply a design measurement by
/// [`scale`](Self::scale), or call [`dp`](Self::dp), to get pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct FrameInfo {
    /// Number of frames presented so far.
    pub index: u64,
    /// Foreground time since the app started: the clock stops while the app
    /// is paused, so animations resume where they left off. It keeps running
    /// while the app is visible but idle (no frames being produced), so time
    /// an animation from the `elapsed` of the frame it started on.
    pub elapsed: Duration,
    /// Size of the drawable in physical pixels.
    pub size: SurfaceSize,
    /// Physical pixels per density-independent unit: `DisplayMetrics.density`
    /// on Android, the screen's native scale on iOS. Always finite and
    /// greater than zero; `1.0` until the host has reported it. Can change
    /// while the app runs (the window moves to another display, or the user
    /// changes the display size setting), which produces a new frame.
    pub scale: f32,
    /// The parts of the surface that system UI or the display's shape may
    /// obscure, in physical pixels: status bar, display cutout, navigation
    /// or gesture bar, home indicator. The surface extends under them, so
    /// paint backgrounds edge to edge and keep content and touch targets
    /// inside [`safe_area`](Self::safe_area). Changes (rotation) produce a
    /// new frame.
    pub insets: Insets,
    /// How much of the bottom of the surface the soft keyboard covers, in
    /// physical pixels, measured from the surface's bottom edge (so it
    /// includes the gesture bar under the keyboard). Zero while no keyboard
    /// is showing. It is not part of [`insets`](Self::insets): content
    /// should usually move or scroll clear of the keyboard rather than be
    /// padded as it is for a status bar. A change produces a new frame.
    pub keyboard_inset: f32,
    /// The user has asked the system for less motion (Android's "remove
    /// animations", an animator duration scale of zero; iOS's "reduce
    /// motion"). Animations that only decorate a change should then take
    /// no time. False until a host says otherwise; a change produces a new
    /// frame.
    pub reduce_motion: bool,
}

impl FrameInfo {
    /// Information for a first frame of `size` at time zero, with a scale of
    /// 1 and no insets.
    pub const fn new(size: SurfaceSize) -> Self {
        Self {
            index: 0,
            elapsed: Duration::ZERO,
            size,
            scale: 1.0,
            insets: Insets::ZERO,
            keyboard_inset: 0.0,
            reduce_motion: false,
        }
    }

    pub const fn with_index(mut self, index: u64) -> Self {
        self.index = index;
        self
    }

    pub const fn with_elapsed(mut self, elapsed: Duration) -> Self {
        self.elapsed = elapsed;
        self
    }

    /// Sets the scale. A value that is not finite and positive is ignored,
    /// so [`scale`](Self::scale) is always safe to multiply and divide by.
    pub fn with_scale(mut self, scale: f32) -> Self {
        if is_valid_scale(scale) {
            self.scale = scale;
        }
        self
    }

    /// Sets the insets; negative or non-finite edges become zero.
    pub fn with_insets(mut self, insets: Insets) -> Self {
        self.insets = insets.sanitized();
        self
    }

    /// Sets the keyboard inset; a negative or non-finite value becomes zero.
    pub fn with_keyboard_inset(mut self, inset: f32) -> Self {
        self.keyboard_inset = clean_inset(inset);
        self
    }

    /// Sets whether the user has asked the system for less motion.
    pub const fn with_reduce_motion(mut self, reduce: bool) -> Self {
        self.reduce_motion = reduce;
        self
    }

    /// Converts a length in density-independent units to physical pixels.
    pub fn dp(&self, length: f32) -> f32 {
        length * self.scale
    }

    /// Size of the drawable in density-independent units.
    pub fn logical_size(&self) -> Size {
        Size::new(
            self.size.width as f32 / self.scale,
            self.size.height as f32 / self.scale,
        )
    }

    /// The part of the surface inside [`insets`](Self::insets), in physical
    /// pixels. Never has a negative dimension.
    pub fn safe_area(&self) -> Rect {
        self.insets.inner_rect(self.size)
    }
}

/// A keyboard inset from a host, made safe to use.
pub(crate) fn clean_inset(inset: f32) -> f32 {
    if inset.is_finite() {
        inset.max(0.0)
    } else {
        0.0
    }
}

/// A scale factor the engine and [`FrameInfo`] accept from a host.
pub(crate) fn is_valid_scale(scale: f32) -> bool {
    scale.is_finite() && scale > 0.0
}

/// What the application wants on screen for one frame.
///
/// The engine owns one `Frame` and hands it to the app each frame with a
/// black background and an empty list, so building a frame does not allocate
/// once the list has reached its working size.
pub struct Frame {
    /// Background colour, drawn before anything in `list`.
    pub clear: Color,
    /// Drawing operations, painted in order over the background.
    pub list: DisplayList,
    text: TextContext,
    /// The registered images. Not cleared between frames.
    images: ImageStore,
    /// Rasterised vector pictures and outlined texts, as images in
    /// `images`. See ADR 0017.
    pictures: HashMap<PictureKey, CachedPicture>,
    /// Counts calls to `begin`, for the picture cache.
    generation: u64,
    redraw_requested: bool,
    redraw_after: Option<Duration>,
    text_input: Option<TextInputRequest>,
    system_bars: Option<SystemBars>,
}

impl Frame {
    /// A frame with no fonts: text measures as empty and draws nothing.
    /// Use [`for_app`](Self::for_app) to get the app's fonts.
    pub fn new() -> Self {
        Self {
            clear: Color::BLACK,
            list: DisplayList::new(),
            text: TextContext::new(),
            images: ImageStore::new(),
            pictures: HashMap::new(),
            generation: 0,
            redraw_requested: false,
            redraw_after: None,
            text_input: None,
            system_bars: None,
        }
    }

    /// A frame prepared for `app`: runs [`App::setup`] so its fonts are
    /// available. The engine does this once at startup; tests and tools that
    /// call [`App::frame`] directly should do the same.
    ///
    /// The app is set up as if there were no host: nothing stored, and the
    /// placeholder installation id. See
    /// [`for_app_with_host`](Self::for_app_with_host).
    pub fn for_app(app: &mut dyn App) -> Self {
        Self::for_app_with_host(app, &HostState::new())
    }

    /// As [`for_app`](Self::for_app), with what a host would hand the app
    /// at setup: its installation id and the values stored on earlier
    /// runs. Tests use it to start an app "on its second launch".
    pub fn for_app_with_host(app: &mut dyn App, host: &HostState) -> Self {
        let mut frame = Self::new();
        app.setup(&mut Setup {
            text: &mut frame.text,
            images: &mut frame.images,
            host,
        });
        frame
    }

    /// Clears the frame for the next call to [`App::frame`], keeping
    /// allocations and cached text layouts that are still in use.
    pub fn begin(&mut self) {
        self.clear = Color::BLACK;
        self.list.clear();
        self.text.begin_frame();
        self.evict_pictures();
        self.generation += 1;
        self.redraw_requested = false;
        self.redraw_after = None;
        self.text_input = None;
        self.system_bars = None;
    }

    /// Asks for another frame at the next display refresh.
    ///
    /// Frames are produced on demand: after this frame is shown the engine
    /// stops calling [`App::frame`], and the device can sleep its CPU and
    /// GPU, until something changes. Input, a resize, and scale or inset
    /// changes all produce a frame without being asked. Call this from
    /// [`App::frame`] only while what is on screen depends on time, which
    /// means once per frame for as long as an animation is running, and stop
    /// calling it when the animation has reached its end.
    pub fn request_redraw(&mut self) {
        self.redraw_requested = true;
    }

    /// True if [`request_redraw`](Self::request_redraw) was called since
    /// [`begin`](Self::begin). Tests use this to check that an app goes idle.
    pub fn redraw_requested(&self) -> bool {
        self.redraw_requested
    }

    /// Asks for another frame once `delay` has passed, without drawing in
    /// between: a blinking caret, a clock that shows minutes.
    ///
    /// Like [`request_redraw`](Self::request_redraw), the request lasts for
    /// one frame: whatever frame comes next (this timer, or input arriving
    /// first) replaces it, so call it again from that frame if the picture
    /// still depends on time. Several calls in one frame keep the shortest
    /// delay. While the app is hidden nothing is drawn; a frame is always
    /// produced when it becomes visible again.
    pub fn request_redraw_after(&mut self, delay: Duration) {
        self.redraw_after = Some(self.redraw_after.map_or(delay, |now| now.min(delay)));
    }

    /// The shortest delay asked for with
    /// [`request_redraw_after`](Self::request_redraw_after) since
    /// [`begin`](Self::begin).
    pub fn redraw_after(&self) -> Option<Duration> {
        self.redraw_after
    }

    /// Says that a text field has focus in this frame, and what it holds.
    ///
    /// Text input is declared per frame, like everything else a frame
    /// contains: while frames keep calling this, the platform's keyboard is
    /// up, configured by the request's traits; the first frame that does not
    /// call it hides the keyboard. What the user types arrives in
    /// [`App::text_input`]. The request's text and caret must be the field's
    /// real contents: the platform's input method edits its own copy and
    /// is corrected from here when the two differ.
    pub fn set_text_input(&mut self, request: TextInputRequest) {
        self.text_input = Some(request);
    }

    /// The request made with [`set_text_input`](Self::set_text_input) since
    /// [`begin`](Self::begin).
    pub fn text_input(&self) -> Option<&TextInputRequest> {
        self.text_input.as_ref()
    }

    /// Says whether the system's status and navigation bars should show
    /// light or dark icons over this frame. The surface is drawn edge to
    /// edge under transparent bars, so only the application knows what is
    /// behind them. Declared per frame, like the keyboard; the host is told
    /// only when it changes, and a frame that does not call this leaves the
    /// bars as they are.
    pub fn set_system_bars(&mut self, bars: SystemBars) {
        self.system_bars = Some(bars);
    }

    /// What [`set_system_bars`](Self::set_system_bars) was given since
    /// [`begin`](Self::begin).
    pub fn system_bars(&self) -> Option<SystemBars> {
        self.system_bars
    }

    /// The rectangles a selection of bytes `from..to` of `text` covers,
    /// relative to the origin the text would be drawn at with the same
    /// style and width, appended to `out`: one per visual line, and more
    /// than one on a line whose selected part is split by a change of
    /// writing direction.
    pub fn text_selection_rects(
        &mut self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f32>,
        from: usize,
        to: usize,
        out: &mut Vec<Rect>,
    ) {
        self.text
            .selection_rects(text, style, max_width, from, to, out);
    }

    /// Where a caret at byte offset `index` of `text` belongs, relative to
    /// the origin the text would be drawn at with the same style and width:
    /// a rectangle of zero width, one line tall. `index` is clamped to the
    /// text and moved back to a character boundary.
    pub fn text_caret(
        &mut self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f32>,
        index: usize,
    ) -> Rect {
        self.text.caret(text, style, max_width, index)
    }

    /// The byte offset in `text` at which a caret belongs for a touch at
    /// `point`, relative to the origin the text would be drawn at with the
    /// same style and width. Always on a character boundary; a point
    /// outside the text gives the nearest position on the nearest line.
    pub fn text_index_at(
        &mut self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f32>,
        point: Point,
    ) -> usize {
        self.text.index_at(text, style, max_width, point)
    }

    /// Draws text with the top-left corner of its layout box at `origin`,
    /// and returns the size it occupied.
    ///
    /// With `max_width`, lines wrap at word boundaries and `style.align`
    /// positions them within that width. Text is shaped properly for its
    /// script, including right-to-left scripts such as Arabic.
    pub fn draw_text(
        &mut self,
        text: &str,
        style: &TextStyle,
        origin: Point,
        max_width: Option<f32>,
    ) -> Size {
        let size = self
            .text
            .draw(&mut self.list, text, style, origin, max_width);
        self.settle_turned_text();
        size
    }

    /// The size [`draw_text`](Self::draw_text) would occupy, without drawing.
    pub fn measure_text(&mut self, text: &str, style: &TextStyle, max_width: Option<f32>) -> Size {
        self.text.measure(text, style, max_width)
    }

    /// Draws one paragraph made of several spans, each with its own weight,
    /// colour or font family where it sets one; everything else comes from
    /// `style`. The spans flow and wrap as a single text. Returns the size
    /// occupied.
    pub fn draw_text_spans(
        &mut self,
        spans: &[TextSpan<'_>],
        style: &TextStyle,
        origin: Point,
        max_width: Option<f32>,
    ) -> Size {
        let size = self
            .text
            .draw_spans(&mut self.list, spans, style, origin, max_width);
        self.settle_turned_text();
        size
    }

    /// Text drawn through a transform that changed since the previous
    /// frame is left to the renderer to turn for that frame (ADR 0029).
    /// One more frame draws it sharp once the transform has stopped.
    fn settle_turned_text(&mut self) {
        if self.text.take_settle_request() {
            self.redraw_requested = true;
        }
    }

    /// The size [`draw_text_spans`](Self::draw_text_spans) would occupy.
    pub fn measure_text_spans(
        &mut self,
        spans: &[TextSpan<'_>],
        style: &TextStyle,
        max_width: Option<f32>,
    ) -> Size {
        self.text.measure_spans(spans, style, max_width)
    }

    /// Draws a one-colour vector icon into `rect` (physical pixels).
    ///
    /// The icon is scaled uniformly to fit the rectangle, centred, and
    /// rasterised for exactly the pixels it covers, so it is sharp at any
    /// display scale. `stroke_width`, in the icon's own view-box units,
    /// replaces the width its strokes were designed with. The rasterised
    /// mask is cached (per icon, pixel size and stroke width) in the glyph
    /// atlas and drawn in the same draw call as text.
    ///
    /// An icon outside the supported subset draws nothing; see
    /// [`VectorIcon::check`].
    pub fn draw_icon(
        &mut self,
        icon: &VectorIcon,
        rect: Rect,
        color: Color,
        stroke_width: Option<f32>,
    ) {
        self.text
            .draw_icon(&mut self.list, icon, rect, color, stroke_width);
    }

    /// Draws a multi-colour vector picture into `rect` (physical pixels).
    ///
    /// The picture's view box is scaled uniformly to fit the rectangle and
    /// centred in it. The rectangle is snapped to whole pixels and the
    /// picture is rasterised for exactly that many pixels, so it is as
    /// sharp as the display allows at any scale. `corner_radius` rounds the
    /// rectangle's corners and `tint` multiplies every pixel
    /// ([`Color::WHITE`] leaves the picture as it is), as for any image.
    ///
    /// The rasterisation is an RGBA image kept in the frame's image store
    /// and drawn through the image path (so, like any image, it is its own
    /// draw call when another image is on screen; ADR 0014). It is cached
    /// by the picture's content and pixel size, and dropped again some time
    /// after it stops being drawn. It costs `4 * width * height` bytes here
    /// and a third more than that on the GPU, with mipmaps. Pictures larger
    /// than 2048 pixels on a side are rasterised at that size and
    /// stretched.
    ///
    /// `picture` is a [`VectorPicture`](morningstar_text::VectorPicture) or
    /// a loaded [`SvgPicture`]; both go through this one path and cache.
    pub fn draw_picture(
        &mut self,
        picture: &impl RasterPicture,
        rect: Rect,
        corner_radius: f32,
        tint: Color,
    ) {
        let (left, top) = (rect.x.round(), rect.y.round());
        let width = (rect.x + rect.width).round() - left;
        let height = (rect.y + rect.height).round() - top;
        // Written so that NaN is rejected.
        if !(width >= 1.0 && height >= 1.0 && width < 1.0e6 && height < 1.0e6) {
            return;
        }
        let (mut raster_width, mut raster_height) = (width as u32, height as u32);
        let longest = raster_width.max(raster_height);
        if longest > MAX_PICTURE_PIXELS {
            let shrink = |side: u32| {
                ((u64::from(side) * u64::from(MAX_PICTURE_PIXELS)).div_ceil(u64::from(longest))
                    as u32)
                    .clamp(1, MAX_PICTURE_PIXELS)
            };
            (raster_width, raster_height) = (shrink(raster_width), shrink(raster_height));
        }
        let key = PictureKey {
            content: picture.content_hash(),
            width: raster_width,
            height: raster_height,
        };
        let image = match self.pictures.get_mut(&key) {
            Some(cached) => {
                cached.last_used = self.generation;
                cached.image
            }
            None => {
                let pixels = picture.rasterise(raster_width, raster_height);
                let Ok(image) =
                    self.images
                        .add_premultiplied_rgba8_vec(raster_width, raster_height, pixels)
                else {
                    return;
                };
                self.pictures.insert(
                    key,
                    CachedPicture {
                        image,
                        offset: Point::new(0.0, 0.0),
                        last_used: self.generation,
                    },
                );
                image
            }
        };
        self.list.draw_image_region(
            image,
            image.bounds(),
            Rect::new(left, top, width, height),
            corner_radius,
            tint,
        );
    }

    /// Draws a loaded SVG as an icon in `color`, into `rect` (physical
    /// pixels), fitted and centred.
    ///
    /// A [single-ink](SvgPicture::is_single_ink) file (the usual icon: one
    /// colour or `currentColor` throughout) is a mask in the glyph atlas,
    /// like a [`VectorIcon`]: it shares text's draw call and changes
    /// colour without being rasterised again. Any other file is drawn as a
    /// [picture](Self::draw_picture) with `currentColor` set to `color`
    /// and its other colours kept, faded by the colour's alpha.
    pub fn draw_svg_icon(&mut self, picture: &SvgPicture, rect: Rect, color: Color) {
        if self
            .text
            .draw_svg_icon(&mut self.list, picture, rect, color)
        {
            return;
        }
        let opaque = Color::rgb(color.r, color.g, color.b);
        self.draw_picture(
            &picture.with_current_color(opaque),
            rect,
            0.0,
            Color::rgba(1.0, 1.0, 1.0, color.a),
        );
    }

    /// Draws text as filled and outlined lettering: CSS
    /// `-webkit-text-stroke`. The glyphs are filled with the style's colour
    /// and then stroked `stroke_width` pixels wide in `stroke`, centred on
    /// their outlines, so half the stroke lies outside the letters.
    ///
    /// The text is laid out and placed exactly as
    /// [`draw_text_spans`](Self::draw_text_spans) would place it, and takes
    /// the same room. It is drawn as one image, cached like a
    /// [picture](Self::draw_picture): meant for a large digit or a word,
    /// not for paragraphs. Per-span colours are not used.
    pub fn draw_text_outlined(
        &mut self,
        spans: &[TextSpan<'_>],
        style: &TextStyle,
        origin: Point,
        max_width: Option<f32>,
        stroke_width: f32,
        stroke: Color,
    ) -> Size {
        let size = self.text.measure_spans(spans, style, max_width);
        let mut hasher = DefaultHasher::new();
        for span in spans {
            span.text.hash(&mut hasher);
            span.weight.hash(&mut hasher);
            span.family.hash(&mut hasher);
            span.italic.hash(&mut hasher);
        }
        style.weight.hash(&mut hasher);
        style.align.hash(&mut hasher);
        style.family.hash(&mut hasher);
        style.italic.hash(&mut hasher);
        style.tabular_nums.hash(&mut hasher);
        for value in [
            style.size,
            style.line_height,
            style.letter_spacing,
            style.color.r,
            style.color.g,
            style.color.b,
            style.color.a,
            max_width.unwrap_or(-1.0),
            stroke_width,
            stroke.r,
            stroke.g,
            stroke.b,
            stroke.a,
        ] {
            value.to_bits().hash(&mut hasher);
        }
        // Zero-sized, which no picture is: the two kinds cannot collide.
        let key = PictureKey {
            content: hasher.finish(),
            width: 0,
            height: 0,
        };
        let (image, offset) = match self.pictures.get_mut(&key) {
            Some(cached) => {
                cached.last_used = self.generation;
                (cached.image, cached.offset)
            }
            None => {
                let Some(outlined) =
                    self.text
                        .rasterise_outlined(spans, style, max_width, stroke_width, stroke)
                else {
                    return size;
                };
                let Ok(image) = self.images.add_premultiplied_rgba8_vec(
                    outlined.width,
                    outlined.height,
                    outlined.pixels,
                ) else {
                    return size;
                };
                self.pictures.insert(
                    key,
                    CachedPicture {
                        image,
                        offset: outlined.offset,
                        last_used: self.generation,
                    },
                );
                (image, outlined.offset)
            }
        };
        self.list.draw_image(
            image,
            Rect::new(
                origin.x.round() + offset.x,
                origin.y.round() + offset.y,
                image.width() as f32,
                image.height() as f32,
            ),
        );
        size
    }

    /// Bytes of pixel data held for rasterised pictures and outlined
    /// texts. They are part of [`images`](Self::images) too.
    pub fn picture_cache_bytes(&self) -> usize {
        self.pictures
            .values()
            .map(|cached| picture_bytes(cached.image))
            .sum()
    }

    /// Number of rasterised pictures and outlined texts held.
    pub fn picture_cache_len(&self) -> usize {
        self.pictures.len()
    }

    /// Drops cached pictures that have not been drawn for a while, and,
    /// when the cache is over its budget, everything the frame that just
    /// ended did not draw, least recently drawn first.
    fn evict_pictures(&mut self) {
        if self.pictures.is_empty() {
            return;
        }
        // What the frame that just ended drew is marked with `now`.
        let now = self.generation;
        let images = &mut self.images;
        self.pictures.retain(|_, cached| {
            let keep = now - cached.last_used <= PICTURE_IDLE_FRAMES;
            if !keep {
                images.remove(cached.image);
            }
            keep
        });

        let mut bytes = self.picture_cache_bytes();
        if bytes <= PICTURE_CACHE_BYTES {
            return;
        }
        let mut idle: Vec<(u64, PictureKey)> = self
            .pictures
            .iter()
            .filter(|(_, cached)| cached.last_used < now)
            .map(|(key, cached)| (cached.last_used, *key))
            .collect();
        idle.sort_unstable();
        for (_, key) in idle {
            if bytes <= PICTURE_CACHE_BYTES {
                break;
            }
            if let Some(cached) = self.pictures.remove(&key) {
                bytes -= picture_bytes(cached.image);
                self.images.remove(cached.image);
            }
        }
    }

    /// True if a font of the family called `name` has been added.
    pub fn has_font_family(&self, name: &str) -> bool {
        self.text.has_family(name)
    }

    /// The glyph atlas that this frame's text refers to. Pass it to the
    /// renderer together with `list`.
    pub fn atlas(&self) -> AtlasView<'_> {
        self.text.atlas()
    }

    /// Registers an image from decoded pixels and returns its handle:
    /// `width * height * 4` bytes of RGBA, row by row from the top,
    /// sRGB-encoded with straight (not premultiplied) alpha.
    ///
    /// Draw it with [`DisplayList::draw_image`] and its relatives on
    /// [`list`](Self::list), in this frame or any later one. The image stays
    /// registered, across frames, until
    /// [`remove_image`](Self::remove_image); keep the handle in the app's
    /// state and register each image once, not once per frame.
    ///
    /// An image with a side longer than 2048 pixels is stored scaled down to
    /// that (see [`Image`]); the handle and source rectangles keep the
    /// original size. The pixels are copied and stay in memory while the
    /// image is registered, and are uploaded to the GPU the first time it is
    /// drawn.
    ///
    /// Images registered before the first frame belong in [`App::setup`].
    /// This is for those that become known later.
    pub fn add_image_rgba8(
        &mut self,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> Result<Image, ImageError> {
        self.images.add_rgba8(width, height, pixels)
    }

    /// Registers an image from the contents of a PNG file, typically
    /// `include_bytes!`. Otherwise as
    /// [`add_image_rgba8`](Self::add_image_rgba8).
    ///
    /// Decoding happens here, on the calling thread: a large photo can take
    /// longer than a frame. Data that is not a valid PNG is an
    /// [`ImageError::Decode`]. Other formats (JPEG, WebP) are not supported:
    /// decode them elsewhere and register the pixels.
    pub fn add_image_png(&mut self, data: &[u8]) -> Result<Image, ImageError> {
        let (width, height, pixels) = crate::image::decode_png(data)?;
        self.images.add_rgba8_vec(width, height, pixels)
    }

    /// Releases an image: its pixels now, its GPU texture when this frame is
    /// rendered. Returns false if it was not registered (releasing twice is
    /// harmless).
    ///
    /// The handle stays valid to hold and to pass around, but drawing with
    /// it draws nothing, including in the frame being built.
    pub fn remove_image(&mut self, image: Image) -> bool {
        self.images.remove(image)
    }

    /// True if `image` is registered and has not been removed.
    pub fn has_image(&self, image: Image) -> bool {
        self.images.contains(image)
    }

    /// The registered images that this frame's image operations refer to.
    /// Pass it to the renderer together with `list` and
    /// [`atlas`](Self::atlas).
    pub fn images(&self) -> ImagesView<'_> {
        self.images.view()
    }
}

fn picture_bytes(image: Image) -> usize {
    image.width() as usize * image.height() as usize * 4
}

impl Default for Frame {
    fn default() -> Self {
        Self::new()
    }
}

/// One-time configuration handed to [`App::setup`].
pub struct Setup<'a> {
    text: &'a mut TextContext,
    images: &'a mut ImageStore,
    host: &'a HostState,
}

impl Setup<'_> {
    /// An identifier for this installation of the app: 32 lower-case
    /// hexadecimal digits (128 bits). The host generates it once, with the
    /// platform's secure random source, and keeps it in the app's private
    /// storage, so it is the same on every launch and gone when the app is
    /// uninstalled or its data cleared. It identifies neither the device
    /// nor the user. Without a host (tests, screenshot tools) it is 32
    /// zeros, unless the test supplied one
    /// ([`Frame::for_app_with_host`]).
    pub fn install_id(&self) -> &str {
        self.host.install_id()
    }

    /// The value stored under `key` on an earlier run (see
    /// [`ServiceRequest::Store`](morningstar_core::ServiceRequest::Store)),
    /// as it was when the app started. This is how an app knows something
    /// before its first frame without waiting for an answer.
    pub fn stored(&self, key: &str) -> Option<&str> {
        self.host.stored(key)
    }

    /// Registers a TrueType/OpenType font from its file contents, typically
    /// `include_bytes!`. All text is drawn with these fonts: the first one
    /// that has a character wins, so add the primary font first and fonts
    /// for other scripts after it. Weights are matched across the fonts
    /// added (add a bold file to get real bold).
    pub fn add_font(&mut self, data: &[u8]) {
        self.text.add_font(data);
    }

    /// True if a font of the family called `name` has been added (names
    /// are compared without regard to case). Text asks for a family with
    /// [`TextStyle::family`]; one that is missing falls back silently, so
    /// this is the way to check a font file is the family it was meant to
    /// be.
    pub fn has_font_family(&self, name: &str) -> bool {
        self.text.has_family(name)
    }

    /// Makes the device's installed fonts available as a last-resort
    /// fallback. Costs startup time, and makes text look different on each
    /// platform; prefer bundling the fonts the app needs.
    pub fn load_system_fonts(&mut self) {
        self.text.load_system_fonts();
    }

    /// Registers an image from decoded pixels; see
    /// [`Frame::add_image_rgba8`]. Keep the returned handle to draw with.
    pub fn add_image_rgba8(
        &mut self,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> Result<Image, ImageError> {
        self.images.add_rgba8(width, height, pixels)
    }

    /// Registers an image from the contents of a PNG file, typically
    /// `include_bytes!`; see [`Frame::add_image_png`]. Keep the returned
    /// handle to draw with.
    pub fn add_image_png(&mut self, data: &[u8]) -> Result<Image, ImageError> {
        let (width, height, pixels) = crate::image::decode_png(data)?;
        self.images.add_rgba8_vec(width, height, pixels)
    }
}

/// A Morningstar application.
///
/// This is a low-level interface: the app paints rectangles and text by hand
/// and receives raw pointer events. It is not the eventual declarative UI
/// API, which will be built on top of it, and it will change.
///
/// All methods are called on the same thread, never concurrently.
pub trait App: Send + 'static {
    /// Called once, before the first frame. Register fonts here, and the
    /// images known from the start. The default registers none, so an app
    /// that draws text must override it.
    fn setup(&mut self, setup: &mut Setup<'_>) {
        let _ = setup;
    }

    /// Called when a new frame is needed. Fill in `frame`, which arrives
    /// cleared; paint everything, not only what changed.
    ///
    /// A frame is needed when the app becomes visible, when the surface, its
    /// scale or its insets change, after input was delivered to
    /// [`pointer`](Self::pointer) or [`text_input`](Self::text_input), when
    /// the soft keyboard appears or goes, and when the previous frame called
    /// [`Frame::request_redraw`] or the time it gave to
    /// [`Frame::request_redraw_after`] has passed. Otherwise this is not called and nothing
    /// is drawn: an app that does not ask for frames costs no power while
    /// its picture is unchanged.
    fn frame(&mut self, info: &FrameInfo, frame: &mut Frame);

    /// Called for each touch event, before the next call to
    /// [`frame`](Self::frame). Positions are in physical pixels. A frame
    /// always follows, so state changed here appears on screen without
    /// asking. The default ignores input.
    fn pointer(&mut self, event: &PointerEvent) {
        let _ = event;
    }

    /// Called for each text-input event from the platform's keyboard, while
    /// a frame has asked for text input with [`Frame::set_text_input`]. A
    /// frame always follows. The default ignores it.
    fn text_input(&mut self, event: &TextInputEvent) {
        let _ = event;
    }

    /// The system's back gesture or key (Android's Back; on iOS whatever
    /// the host maps to it). Return `true` if the app handled it, for
    /// example by going to its previous screen: a frame then follows.
    /// Return `false` to let the system do what it does when an app has
    /// nothing to go back to, which on Android is leaving the app. The
    /// default handles nothing.
    fn back(&mut self) -> bool {
        false
    }

    /// Moves the requests the app wants to make of the platform (an HTTP
    /// request, a recording, a stored value; see
    /// [`ServiceRequest`](morningstar_core::ServiceRequest)) into `out`,
    /// each with an id of the app's choosing that is not zero and not
    /// reused. Called after setup and after every other call into the app.
    /// What is handed over is in the host's hands: each request that
    /// [expects a result](morningstar_core::ServiceRequest::expects_result)
    /// is answered exactly once through
    /// [`service_result`](Self::service_result). The default asks for
    /// nothing.
    fn service_requests(&mut self, out: &mut Vec<ServiceCall>) {
        let _ = out;
    }

    /// The platform's answer to the request the app gave `id`. Called
    /// between frames, on the thread everything else is called on. Return
    /// `true` if the app was waiting for it: a frame then follows. An
    /// answer the app is not waiting for (it gave up on the request) must
    /// be ignored and `false` returned, which is what the default does.
    fn service_result(&mut self, id: ServiceId, result: ServiceResult) -> bool {
        let _ = (id, result);
        false
    }
}
