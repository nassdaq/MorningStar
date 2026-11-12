//! Image resources as plain data: handles, the store that owns registered
//! pixels, the borrowed view a renderer reads them through, and the maths
//! for fitting an image into a rectangle. Nothing here knows about a GPU.
//! See ADR 0014.

use std::fmt;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{Rect, Size};

/// The longest side, in pixels, an image is stored with. A larger image is
/// scaled down to this when it is registered (see [`ImageStore::add_rgba8`]).
///
/// It is the largest texture every device Morningstar targets can hold (the
/// `downlevel_defaults` limit of the GPU backend, ADR 0002), so a registered
/// image can always be drawn.
pub const MAX_IMAGE_DIMENSION: u32 = 2048;

/// The most bytes of decoded RGBA8 pixels (256 MiB, about 8192 x 8192) an
/// image may have when it is registered. Larger ones are refused rather than
/// risking the allocation.
pub const MAX_IMAGE_BYTES: usize = 256 * 1024 * 1024;

/// Source of image ids and store generations. Process-wide, so an id names
/// one registration of one image in any store, and a generation one state of
/// one store.
static NEXT: AtomicU64 = AtomicU64::new(1);

fn next() -> u64 {
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// Identifies one registered image. Never reused within a process, so an id
/// that outlives its image names nothing rather than some other image.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ImageId(u64);

/// A handle to a registered image: its id and its size.
///
/// Small and `Copy`; keep it in application state and pass it to
/// [`DisplayList::draw_image`](crate::DisplayList::draw_image). Drawing with
/// the handle of an image that has since been released draws nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Image {
    id: ImageId,
    width: u32,
    height: u32,
}

impl Image {
    pub const fn id(&self) -> ImageId {
        self.id
    }

    /// Width in pixels of the image as it was registered. Source rectangles
    /// are in these pixels even if the image is stored scaled down.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels of the image as it was registered.
    pub const fn height(&self) -> u32 {
        self.height
    }

    pub const fn size(&self) -> Size {
        Size::new(self.width as f32, self.height as f32)
    }

    /// The whole image as a source rectangle: `(0, 0, width, height)`.
    pub const fn bounds(&self) -> Rect {
        Rect::new(0.0, 0.0, self.width as f32, self.height as f32)
    }
}

/// Why an image could not be registered.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ImageError {
    /// The width or the height is zero.
    Empty,
    /// The pixel data is not `width * height * 4` bytes long.
    WrongLength { expected: usize, actual: usize },
    /// The decoded image would exceed [`MAX_IMAGE_BYTES`].
    TooLarge { width: u32, height: u32 },
    /// The encoded data could not be decoded. Carries the decoder's message.
    Decode(String),
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("image has no pixels"),
            Self::WrongLength { expected, actual } => write!(
                f,
                "image data is {actual} bytes; its dimensions need {expected}"
            ),
            Self::TooLarge { width, height } => {
                write!(f, "a {width} x {height} image is too large to register")
            }
            Self::Decode(reason) => write!(f, "could not decode image: {reason}"),
        }
    }
}

impl std::error::Error for ImageError {}

struct Stored {
    id: ImageId,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

/// One image's pixels as a renderer uploads them.
#[derive(Clone, Copy, Debug)]
pub struct ImagePixels<'a> {
    /// Stored width, at most [`MAX_IMAGE_DIMENSION`]. May be smaller than the
    /// handle's [`Image::width`].
    pub width: u32,
    pub height: u32,
    /// `width * height * 4` bytes of RGBA, row by row from the top:
    /// sRGB-encoded colour premultiplied by alpha, the encoded value times
    /// alpha: what the renderer blends (ADR 0023). This is the content of a
    /// plain 8-bit texture, sampled and filtered as it is.
    pub pixels: &'a [u8],
}

impl ImagePixels<'_> {
    /// These pixels scaled down to `width` x `height` (each at least 1 and at
    /// most the current size) with a box filter: every output pixel is the
    /// average of the source area it covers, of the stored values. Used for
    /// oversized images and for mipmaps.
    pub fn downscaled(&self, width: u32, height: u32) -> Vec<u8> {
        downscale(self.pixels, self.width, self.height, width, height)
    }
}

/// A borrowed view of the registered images, for a renderer. Plain data, as
/// [`AtlasView`](crate::AtlasView) is for glyphs.
#[derive(Clone, Copy)]
pub struct ImagesView<'a> {
    images: &'a [Stored],
    generation: u64,
}

impl ImagesView<'static> {
    /// No images, for frames that draw none.
    pub const EMPTY: Self = Self {
        images: &[],
        generation: 0,
    };
}

impl<'a> ImagesView<'a> {
    /// Changes whenever an image is added or removed. A renderer that sees
    /// the generation it saw last frame knows the set of images is the same.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn len(&self) -> usize {
        self.images.len()
    }

    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }

    pub fn contains(&self, id: ImageId) -> bool {
        self.position(id).is_some()
    }

    /// The pixels of image `id`, or `None` if it is not (or no longer)
    /// registered.
    pub fn get(&self, id: ImageId) -> Option<ImagePixels<'a>> {
        let stored = &self.images[self.position(id)?];
        Some(ImagePixels {
            width: stored.width,
            height: stored.height,
            pixels: &stored.pixels,
        })
    }

    fn position(&self, id: ImageId) -> Option<usize> {
        // Ids only grow, so the images are in id order.
        self.images
            .binary_search_by_key(&id, |stored| stored.id)
            .ok()
    }
}

impl fmt::Debug for ImagesView<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImagesView")
            .field("len", &self.images.len())
            .field("generation", &self.generation)
            .finish()
    }
}

/// Owns the pixels of every registered image.
///
/// The store is the CPU-side source of truth: a renderer uploads an image to
/// the GPU the first time it is drawn and can upload it again after losing
/// its textures, so the pixels are kept for as long as the image is
/// registered.
pub struct ImageStore {
    images: Vec<Stored>,
    generation: u64,
}

impl Default for ImageStore {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for ImageStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.view(), f)
    }
}

impl ImageStore {
    pub fn new() -> Self {
        Self {
            images: Vec::new(),
            generation: next(),
        }
    }

    /// Registers an image from `width * height * 4` bytes of RGBA, row by
    /// row from the top, sRGB-encoded with straight (not premultiplied)
    /// alpha: what a PNG decodes to.
    ///
    /// The pixels are copied and converted to premultiplied alpha. If either
    /// side is longer than [`MAX_IMAGE_DIMENSION`] the image is stored scaled
    /// down, keeping its aspect ratio, so that its longer side is exactly
    /// that. The returned handle still reports the original size, and source
    /// rectangles stay in original pixels, so the reduction only costs
    /// sharpness.
    pub fn add_rgba8(
        &mut self,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> Result<Image, ImageError> {
        check(width, height, pixels.len())?;
        self.add_rgba8_vec(width, height, pixels.to_vec())
    }

    /// As [`add_rgba8`](Self::add_rgba8), taking the buffer instead of
    /// copying it.
    pub fn add_rgba8_vec(
        &mut self,
        width: u32,
        height: u32,
        mut pixels: Vec<u8>,
    ) -> Result<Image, ImageError> {
        check(width, height, pixels.len())?;
        premultiply(&mut pixels);

        let (mut stored_width, mut stored_height) = (width, height);
        let longest = width.max(height);
        if longest > MAX_IMAGE_DIMENSION {
            let scaled = |side: u32| {
                let exact = u64::from(side) * u64::from(MAX_IMAGE_DIMENSION);
                // Rounded to nearest, and never zero.
                (((exact + u64::from(longest) / 2) / u64::from(longest)) as u32).max(1)
            };
            (stored_width, stored_height) = (scaled(width), scaled(height));
            pixels = downscale(&pixels, width, height, stored_width, stored_height);
        }

        let id = ImageId(next());
        self.images.push(Stored {
            id,
            width: stored_width,
            height: stored_height,
            pixels,
        });
        self.generation = next();
        Ok(Image { id, width, height })
    }

    /// Registers an image whose pixels are already in the form the store
    /// keeps them in (see [`ImagePixels::pixels`]): `width * height * 4`
    /// bytes of RGBA, sRGB-encoded colour premultiplied by alpha (the
    /// encoded value times alpha). [`srgb_encode`] gives the encoded byte of
    /// a [`Color`](crate::Color) component. For
    /// pictures composited on the CPU, which would lose precision going
    /// through straight alpha and back.
    ///
    /// Unlike [`add_rgba8`](Self::add_rgba8) the image is not scaled down:
    /// a side longer than [`MAX_IMAGE_DIMENSION`] is refused with
    /// [`ImageError::TooLarge`].
    pub fn add_premultiplied_rgba8_vec(
        &mut self,
        width: u32,
        height: u32,
        pixels: Vec<u8>,
    ) -> Result<Image, ImageError> {
        check(width, height, pixels.len())?;
        if width.max(height) > MAX_IMAGE_DIMENSION {
            return Err(ImageError::TooLarge { width, height });
        }
        let id = ImageId(next());
        self.images.push(Stored {
            id,
            width,
            height,
            pixels,
        });
        self.generation = next();
        Ok(Image { id, width, height })
    }

    /// Releases an image and its pixels. Returns false if it was not
    /// registered here (for example, already removed). A renderer frees the
    /// image's texture the next time it renders with this store.
    pub fn remove(&mut self, image: Image) -> bool {
        let Some(index) = self.view().position(image.id) else {
            return false;
        };
        self.images.remove(index);
        self.generation = next();
        true
    }

    pub fn contains(&self, image: Image) -> bool {
        self.view().contains(image.id)
    }

    /// Number of registered images.
    pub fn len(&self) -> usize {
        self.images.len()
    }

    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }

    /// Bytes of pixel data held.
    pub fn bytes(&self) -> usize {
        self.images.iter().map(|stored| stored.pixels.len()).sum()
    }

    /// See [`ImagesView::generation`].
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// What a renderer needs to draw this store's images.
    pub fn view(&self) -> ImagesView<'_> {
        ImagesView {
            images: &self.images,
            generation: self.generation,
        }
    }
}

fn check(width: u32, height: u32, len: usize) -> Result<(), ImageError> {
    if width == 0 || height == 0 {
        return Err(ImageError::Empty);
    }
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|texels| texels.checked_mul(4))
        .filter(|&bytes| bytes <= MAX_IMAGE_BYTES)
        .ok_or(ImageError::TooLarge { width, height })?;
    if len != expected {
        return Err(ImageError::WrongLength {
            expected,
            actual: len,
        });
    }
    Ok(())
}

// --- Colour ------------------------------------------------------------------

/// The sRGB transfer function: an encoded value in `0..=1` to linear light.
fn decode(encoded: f32) -> f32 {
    if encoded <= 0.040_45 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    }
}

struct Tables {
    /// `thresholds[i]` is the linear value halfway, in encoded terms,
    /// between bytes `i` and `i + 1`: values at or above it encode to more
    /// than `i`.
    thresholds: [f32; 255],
    /// The byte that encodes linear `i / (GUESSES - 1)`: a starting point
    /// for `encode`, which then only has to step to the exact answer.
    guesses: [u8; GUESSES],
}

const GUESSES: usize = 4096;

fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let thresholds: [f32; 255] =
            std::array::from_fn(|byte| decode((byte as f32 + 0.5) / 255.0));
        Tables {
            guesses: std::array::from_fn(|index| {
                let linear = index as f32 / (GUESSES - 1) as f32;
                thresholds.partition_point(|&t| linear >= t) as u8
            }),
            thresholds,
        }
    })
}

/// The sRGB byte nearest (in encoded terms) to a linear value. Exactly
/// inverts `decode` on every byte. Out-of-range values clamp; NaN encodes as 0.
fn encode(tables: &Tables, linear: f32) -> u8 {
    // The float-to-integer cast saturates, and maps NaN to zero.
    let index = ((linear * (GUESSES - 1) as f32 + 0.5) as usize).min(GUESSES - 1);
    let mut byte = usize::from(tables.guesses[index]);
    // The guess is for a value within half a table step of `linear`, so it
    // is at most a couple of bytes off, and only among the darkest values.
    while byte < 255 && linear >= tables.thresholds[byte] {
        byte += 1;
    }
    while byte > 0 && linear < tables.thresholds[byte - 1] {
        byte -= 1;
    }
    byte as u8
}

/// The sRGB byte that encodes a linear value in `0..=1`, as stored images
/// hold their colour channels (see [`ImagePixels::pixels`]). Out-of-range
/// values clamp; NaN encodes as 0.
pub fn srgb_encode(linear: f32) -> u8 {
    encode(tables(), linear)
}

/// Converts straight-alpha sRGB pixels to premultiplied in place: each
/// colour channel becomes its encoded value times alpha.
///
/// Multiplying the encoded bytes is what the compositing space asks for
/// (ADR 0023): the renderer blends encoded values, as browsers and design
/// tools do, so "source plus destination times one minus alpha" must find
/// the source already scaled in that space. Fully transparent pixels become
/// transparent black, so whatever colour they carried cannot bleed into
/// their neighbours when the image is filtered.
fn premultiply(pixels: &mut [u8]) {
    for pixel in pixels.chunks_exact_mut(4) {
        match pixel[3] {
            255 => {}
            0 => pixel[..3].fill(0),
            alpha => {
                let alpha = u32::from(alpha);
                for channel in &mut pixel[..3] {
                    // Rounded to nearest; never more than alpha.
                    *channel = ((u32::from(*channel) * alpha + 127) / 255) as u8;
                }
            }
        }
    }
}

/// Box-filters premultiplied sRGB pixels down to `new_width` x `new_height`,
/// averaging the encoded values: the space everything else is blended and
/// filtered in (ADR 0023), and what a browser does when it shrinks an image.
/// Out-of-range target sizes are clamped to `1..=` the source size; a
/// mismatched buffer yields transparent pixels rather than a panic.
fn downscale(pixels: &[u8], width: u32, height: u32, new_width: u32, new_height: u32) -> Vec<u8> {
    let (new_width, new_height) = (
        new_width.clamp(1, width.max(1)),
        new_height.clamp(1, height.max(1)),
    );
    let (w, h) = (width as usize, height as usize);
    let (nw, nh) = (new_width as usize, new_height as usize);
    let mut out = vec![0u8; nw * nh * 4];
    if w == 0 || h == 0 || pixels.len() != w * h * 4 {
        return out;
    }
    if (nw, nh) == (w, h) {
        out.copy_from_slice(pixels);
        return out;
    }

    if (nw * 2, nh * 2) == (w, h) {
        // Exactly half, as for most mip levels: each output texel is the
        // mean of a two-by-two block.
        for y in 0..nh {
            let (upper, lower) = (&pixels[y * 2 * w * 4..], &pixels[(y * 2 + 1) * w * 4..]);
            for x in 0..nw {
                let block = [
                    &upper[x * 8..x * 8 + 4],
                    &upper[x * 8 + 4..x * 8 + 8],
                    &lower[x * 8..x * 8 + 4],
                    &lower[x * 8 + 4..x * 8 + 8],
                ];
                let target = &mut out[(y * nw + x) * 4..(y * nw + x) * 4 + 4];
                for channel in 0..4 {
                    let sum: u32 = block.iter().map(|texel| u32::from(texel[channel])).sum();
                    // Rounded to nearest, halves up, as the general path does.
                    target[channel] = ((sum + 2) / 4) as u8;
                }
            }
        }
        return out;
    }
    // The source columns each output column covers, with the fraction of
    // each that lies inside it. The same along y, computed per row below.
    let spans = |n: usize, size: usize| -> Vec<(usize, usize, f32, f32)> {
        (0..n)
            .map(|i| {
                let start = i as f64 * size as f64 / n as f64;
                let end = (i + 1) as f64 * size as f64 / n as f64;
                let first = (start.floor() as usize).min(size - 1);
                let last = ((end.ceil() as usize).max(first + 1)).min(size);
                // Weights of the first and last source texels; the ones
                // between count fully.
                let first_weight = ((first + 1) as f64).min(end) - start;
                let last_weight = end - ((last - 1) as f64).max(start);
                (first, last, first_weight as f32, last_weight as f32)
            })
            .collect()
    };
    let columns = spans(nw, w);
    let rows = spans(nh, h);
    let weight =
        |index: usize, (first, last, first_weight, last_weight): (usize, usize, f32, f32)| {
            if index == first {
                first_weight
            } else if index == last - 1 {
                last_weight
            } else {
                1.0
            }
        };

    for (y, &row_span) in rows.iter().enumerate() {
        for (x, &column_span) in columns.iter().enumerate() {
            let mut sum = [0.0f32; 4];
            let mut total = 0.0f32;
            for sy in row_span.0..row_span.1 {
                let wy = weight(sy, row_span);
                let row = &pixels[sy * w * 4..(sy + 1) * w * 4];
                for sx in column_span.0..column_span.1 {
                    let weight = wy * weight(sx, column_span);
                    let texel = &row[sx * 4..sx * 4 + 4];
                    for channel in 0..4 {
                        sum[channel] += weight * f32::from(texel[channel]);
                    }
                    total += weight;
                }
            }
            let target = &mut out[(y * nw + x) * 4..(y * nw + x) * 4 + 4];
            for channel in 0..4 {
                target[channel] = (sum[channel] / total).round() as u8;
            }
        }
    }
    out
}

// --- Fitting -----------------------------------------------------------------

/// How an image is fitted into a rectangle of a different shape.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ImageFit {
    /// The image is distorted to fill the rectangle exactly (CSS
    /// `object-fit: fill`).
    Stretch,
    /// The whole image is shown as large as fits, centred, keeping its
    /// aspect ratio; the rectangle may be left partly empty (`contain`).
    Contain,
    /// The rectangle is filled, keeping the aspect ratio; the image is
    /// cropped equally on both sides of the axis that overflows (`cover`).
    #[default]
    Cover,
}

/// Where a fitted image goes: which part of it, into which rectangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImagePlacement {
    /// The part of the image to draw, in image pixels.
    pub source: Rect,
    /// Where to draw it, in physical pixels.
    pub rect: Rect,
}

impl ImageFit {
    /// Fits the region `source` of an image (pass [`Image::bounds`] for the
    /// whole image) into `bounds`.
    ///
    /// If either rectangle is empty there is no aspect ratio to keep and the
    /// result is `source` into `bounds` unchanged, which draws nothing.
    pub fn place(self, source: Rect, bounds: Rect) -> ImagePlacement {
        let stretched = ImagePlacement {
            source,
            rect: bounds,
        };
        if source.is_empty() || bounds.is_empty() {
            return stretched;
        }
        let (scale_x, scale_y) = (bounds.width / source.width, bounds.height / source.height);
        match self {
            Self::Stretch => stretched,
            Self::Contain => {
                let scale = scale_x.min(scale_y);
                ImagePlacement {
                    source,
                    rect: Rect::centered(
                        bounds.center(),
                        source.width * scale,
                        source.height * scale,
                    ),
                }
            }
            Self::Cover => {
                let scale = scale_x.max(scale_y);
                ImagePlacement {
                    source: Rect::centered(
                        source.center(),
                        bounds.width / scale,
                        bounds.height / scale,
                    ),
                    rect: bounds,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premultiplied_pixels_are_stored_as_given() {
        let mut store = ImageStore::new();
        let pixels = vec![srgb_encode(0.5), 0, 0, 128, 0, 0, 0, 0];
        let image = store
            .add_premultiplied_rgba8_vec(2, 1, pixels.clone())
            .unwrap();
        assert_eq!((image.width(), image.height()), (2, 1));
        assert_eq!(store.view().get(image.id()).unwrap().pixels, &pixels[..]);
        // Half in linear light is byte 188 in sRGB.
        assert_eq!(pixels[0], 188);
        assert_eq!((srgb_encode(0.0), srgb_encode(1.0)), (0, 255));
        assert_eq!((srgb_encode(-1.0), srgb_encode(7.0)), (0, 255));
        assert_eq!(srgb_encode(f32::NAN), 0);

        assert_eq!(
            store.add_premultiplied_rgba8_vec(1, 1, vec![0; 3]),
            Err(ImageError::WrongLength {
                expected: 4,
                actual: 3
            })
        );
        let side = MAX_IMAGE_DIMENSION + 1;
        assert_eq!(
            store.add_premultiplied_rgba8_vec(side, 1, vec![0; side as usize * 4]),
            Err(ImageError::TooLarge {
                width: side,
                height: 1
            })
        );
    }

    const RED: [u8; 4] = [255, 0, 0, 255];

    fn solid(width: u32, height: u32, pixel: [u8; 4]) -> Vec<u8> {
        pixel.repeat((width * height) as usize)
    }

    #[test]
    fn register_and_release() {
        let mut store = ImageStore::new();
        assert!(store.is_empty());
        let before = store.generation();

        let a = store.add_rgba8(2, 3, &solid(2, 3, RED)).unwrap();
        let b = store.add_rgba8(1, 1, &RED).unwrap();
        assert_eq!((a.width(), a.height()), (2, 3));
        assert_eq!(a.size(), Size::new(2.0, 3.0));
        assert_eq!(a.bounds(), Rect::new(0.0, 0.0, 2.0, 3.0));
        assert_ne!(a.id(), b.id());
        assert_eq!(store.len(), 2);
        assert_eq!(store.bytes(), 28);
        assert!(store.generation() > before);
        assert!(store.contains(a) && store.contains(b));

        let view = store.view();
        let pixels = view.get(a.id()).unwrap();
        assert_eq!((pixels.width, pixels.height), (2, 3));
        assert_eq!(pixels.pixels, solid(2, 3, RED));

        let generation = store.generation();
        assert!(store.remove(a));
        assert!(store.generation() > generation);
        assert!(!store.contains(a));
        assert!(store.view().get(a.id()).is_none());
        assert!(store.contains(b), "removing one image leaves the others");
        assert_eq!(store.bytes(), 4);

        let generation = store.generation();
        assert!(!store.remove(a), "a second release is harmless");
        assert_eq!(store.generation(), generation, "and changes nothing");
    }

    #[test]
    fn ids_are_never_reused_even_across_stores() {
        let mut first = ImageStore::new();
        let mut second = ImageStore::new();
        let a = first.add_rgba8(1, 1, &RED).unwrap();
        first.remove(a);
        let b = first.add_rgba8(1, 1, &RED).unwrap();
        let c = second.add_rgba8(1, 1, &RED).unwrap();
        assert!(a.id() < b.id() && b.id() < c.id());
        assert!(!first.contains(a), "the stale handle names nothing");
        assert!(!second.contains(b), "a handle belongs to its store");
        assert_ne!(first.generation(), second.generation());
        assert!(!ImagesView::EMPTY.contains(a.id()));
        assert!(ImagesView::EMPTY.is_empty());
    }

    #[test]
    fn bad_input_is_an_error_not_a_panic() {
        let mut store = ImageStore::new();
        assert_eq!(store.add_rgba8(0, 4, &[]), Err(ImageError::Empty));
        assert_eq!(store.add_rgba8(4, 0, &[]), Err(ImageError::Empty));
        assert_eq!(
            store.add_rgba8(2, 2, &[0; 15]),
            Err(ImageError::WrongLength {
                expected: 16,
                actual: 15
            })
        );
        assert_eq!(
            store.add_rgba8(u32::MAX, u32::MAX, &[0; 4]),
            Err(ImageError::TooLarge {
                width: u32::MAX,
                height: u32::MAX
            })
        );
        assert_eq!(
            store.add_rgba8(8193, 8192, &[0; 4]),
            Err(ImageError::TooLarge {
                width: 8193,
                height: 8192
            })
        );
        assert!(store.is_empty());
        assert!(ImageError::Decode("bad".into()).to_string().contains("bad"));
    }

    #[test]
    fn srgb_bytes_round_trip_exactly() {
        let tables = tables();
        for byte in 0..=255u8 {
            assert_eq!(encode(tables, decode(f32::from(byte) / 255.0)), byte);
        }
        assert_eq!(encode(tables, -1.0), 0);
        assert_eq!(encode(tables, 2.0), 255);
        assert_eq!(encode(tables, f32::NAN), 0);
        // Middle grey: half the light is byte 188, not 128.
        assert_eq!(encode(tables, 0.5), 188);
        assert!((decode(128.0 / 255.0) - 0.2158).abs() < 1e-3);
    }

    #[test]
    fn premultiplies_the_encoded_values() {
        let mut store = ImageStore::new();
        let image = store
            .add_rgba8(
                4,
                1,
                &[
                    255, 128, 0, 255, // opaque: unchanged
                    255, 255, 255, 128, // half-transparent white
                    10, 200, 30, 0, // invisible: its colour is discarded
                    188, 0, 0, 128, // half-transparent half-intensity red
                ],
            )
            .unwrap();
        let view = store.view();
        let pixels = view.get(image.id()).unwrap().pixels;
        assert_eq!(pixels[0..4], [255, 128, 0, 255]);
        // The encoded byte times alpha, as the compositing space asks
        // (ADR 0023): 255 * 128/255, not the 188 that half the light would
        // encode to.
        assert_eq!(pixels[4..8], [128, 128, 128, 128]);
        assert_eq!(pixels[8..12], [0, 0, 0, 0]);
        // 188 * 128/255 = 94.4.
        assert_eq!(pixels[12..16], [94, 0, 0, 128]);
    }

    #[test]
    fn oversized_images_are_stored_scaled_to_the_limit() {
        let mut store = ImageStore::new();
        // 4096 x 16: columns alternate red and blue in pairs of two, so a
        // 2:1 reduction keeps them distinct.
        let (width, height) = (MAX_IMAGE_DIMENSION * 2, 16);
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for _ in 0..height {
            for x in 0..width {
                pixels.extend_from_slice(if (x / 2) % 2 == 0 {
                    &RED
                } else {
                    &[0, 0, 255, 255]
                });
            }
        }
        let image = store.add_rgba8_vec(width, height, pixels).unwrap();
        assert_eq!(
            (image.width(), image.height()),
            (width, height),
            "the handle keeps the original size"
        );
        let view = store.view();
        let stored = view.get(image.id()).unwrap();
        assert_eq!((stored.width, stored.height), (MAX_IMAGE_DIMENSION, 8));
        assert_eq!(stored.pixels.len(), (MAX_IMAGE_DIMENSION * 8 * 4) as usize);
        assert_eq!(stored.pixels[0..4], RED);
        assert_eq!(stored.pixels[4..8], [0, 0, 255, 255]);

        // A tall one: the longer side lands exactly on the limit and the
        // shorter keeps the aspect ratio, never reaching zero.
        let tall = store.add_rgba8_vec(3, 6000, solid(3, 6000, RED)).unwrap();
        let view = store.view();
        let stored = view.get(tall.id()).unwrap();
        assert_eq!((stored.width, stored.height), (1, MAX_IMAGE_DIMENSION));

        // At the limit nothing is rescaled.
        let exact = store
            .add_rgba8_vec(MAX_IMAGE_DIMENSION, 1, solid(MAX_IMAGE_DIMENSION, 1, RED))
            .unwrap();
        let view = store.view();
        assert_eq!(view.get(exact.id()).unwrap().width, MAX_IMAGE_DIMENSION);
    }

    #[test]
    fn downscale_averages_the_encoded_bytes() {
        // Black and white texels average to the middle byte, as in a
        // browser; half the light would be 188.
        let pixels = [0, 0, 0, 255, 255, 255, 255, 255];
        let half = ImagePixels {
            width: 2,
            height: 1,
            pixels: &pixels,
        }
        .downscaled(1, 1);
        assert_eq!(half, [128, 128, 128, 255]);

        // Opaque red beside nothing: half-covered red, premultiplied, with
        // no contribution from the transparent texel's (absent) colour.
        let pixels = [255, 0, 0, 255, 0, 0, 0, 0];
        let half = ImagePixels {
            width: 2,
            height: 1,
            pixels: &pixels,
        }
        .downscaled(1, 1);
        assert_eq!(half, [128, 0, 0, 128]);
    }

    #[test]
    fn downscale_weights_partial_texels_and_tolerates_bad_sizes() {
        // Three texels into two: each output covers one and a half inputs.
        let pixels = [255, 255, 255, 255, 0, 0, 0, 255, 255, 255, 255, 255];
        let view = ImagePixels {
            width: 3,
            height: 1,
            pixels: &pixels,
        };
        let out = view.downscaled(2, 1);
        // Two thirds white, one third black: two thirds of 255.
        assert_eq!(out, [170, 170, 170, 255, 170, 170, 170, 255]);

        assert_eq!(view.downscaled(3, 1), pixels, "same size is a copy");
        assert_eq!(view.downscaled(0, 0).len(), 4, "clamped up to one texel");
        assert_eq!(view.downscaled(9, 9), pixels, "never scales up");
        let broken = ImagePixels {
            width: 3,
            height: 1,
            pixels: &pixels[..5],
        };
        assert_eq!(broken.downscaled(1, 1), [0, 0, 0, 0]);
    }

    #[test]
    fn stretch_uses_everything_and_fills_everything() {
        let source = Rect::new(0.0, 0.0, 200.0, 100.0);
        let bounds = Rect::new(10.0, 20.0, 50.0, 50.0);
        assert_eq!(
            ImageFit::Stretch.place(source, bounds),
            ImagePlacement {
                source,
                rect: bounds
            }
        );
    }

    #[test]
    fn contain_letterboxes_and_centres() {
        let wide = Rect::new(0.0, 0.0, 200.0, 100.0);
        let bounds = Rect::new(10.0, 20.0, 50.0, 50.0);
        let placed = ImageFit::Contain.place(wide, bounds);
        assert_eq!(placed.source, wide, "nothing is cropped");
        assert_eq!(placed.rect, Rect::new(10.0, 32.5, 50.0, 25.0));

        let tall = Rect::new(0.0, 0.0, 100.0, 400.0);
        let placed = ImageFit::Contain.place(tall, bounds);
        assert_eq!(placed.rect, Rect::new(28.75, 20.0, 12.5, 50.0));

        // A small image is scaled up to fit.
        let small = Rect::new(0.0, 0.0, 10.0, 10.0);
        let placed = ImageFit::Contain.place(small, Rect::new(0.0, 0.0, 80.0, 40.0));
        assert_eq!(placed.rect, Rect::new(20.0, 0.0, 40.0, 40.0));
    }

    #[test]
    fn cover_fills_and_crops_the_overflowing_axis_equally() {
        let wide = Rect::new(0.0, 0.0, 200.0, 100.0);
        let bounds = Rect::new(10.0, 20.0, 50.0, 50.0);
        let placed = ImageFit::Cover.place(wide, bounds);
        assert_eq!(placed.rect, bounds, "the rectangle is filled");
        assert_eq!(placed.source, Rect::new(50.0, 0.0, 100.0, 100.0));

        let tall = Rect::new(0.0, 0.0, 100.0, 400.0);
        let placed = ImageFit::Cover.place(tall, Rect::new(0.0, 0.0, 50.0, 100.0));
        assert_eq!(placed.source, Rect::new(0.0, 100.0, 100.0, 200.0));

        // The same shape: nothing to crop.
        let placed = ImageFit::Cover.place(wide, Rect::new(0.0, 0.0, 20.0, 10.0));
        assert_eq!(placed.source, wide);
        assert_eq!(ImageFit::default(), ImageFit::Cover);
    }

    #[test]
    fn fitting_works_within_a_region_of_a_sprite_sheet() {
        // A 32 x 16 sprite at (64, 32) covering a square.
        let sprite = Rect::new(64.0, 32.0, 32.0, 16.0);
        let placed = ImageFit::Cover.place(sprite, Rect::new(0.0, 0.0, 10.0, 10.0));
        assert_eq!(placed.source, Rect::new(72.0, 32.0, 16.0, 16.0));
        let placed = ImageFit::Contain.place(sprite, Rect::new(0.0, 0.0, 10.0, 10.0));
        assert_eq!(placed.source, sprite);
        assert_eq!(placed.rect, Rect::new(0.0, 2.5, 10.0, 5.0));
    }

    #[test]
    fn fitting_degenerate_rectangles_does_not_divide_by_zero() {
        let empty = Rect::new(0.0, 0.0, 0.0, 10.0);
        let bounds = Rect::new(0.0, 0.0, 10.0, 10.0);
        for fit in [ImageFit::Stretch, ImageFit::Contain, ImageFit::Cover] {
            let placed = fit.place(empty, bounds);
            assert_eq!((placed.source, placed.rect), (empty, bounds));
            let placed = fit.place(bounds, empty);
            assert_eq!((placed.source, placed.rect), (bounds, empty));
            let nan = fit.place(Rect::new(0.0, 0.0, f32::NAN, 1.0), bounds);
            assert_eq!(nan.rect, bounds);
        }
    }
}
