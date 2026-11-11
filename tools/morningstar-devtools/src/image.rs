//! An RGBA8 image in memory, and resampling between sizes.
//!
//! This module knows nothing about the renderer: it is plain pixel
//! arithmetic, so everything built on it (comparison, the CLI, the
//! regression helper) can be tested without a GPU.

use std::fmt;
use std::path::{Path, PathBuf};

/// The largest image, in pixels, that decoding and resizing will allocate.
/// A corrupt or hostile header must not be able to ask for gigabytes.
pub(crate) const MAX_PIXELS: u64 = 100_000_000;

/// A rectangle in whole pixels, with the origin at the top-left corner.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Whether the pixel at (`x`, `y`) is inside the rectangle.
    pub const fn contains(&self, x: u32, y: u32) -> bool {
        x >= self.x && y >= self.y && x - self.x < self.width && y - self.y < self.height
    }
}

/// Why an image could not be read or written.
#[derive(Debug)]
pub enum ImageError {
    /// The file could not be read or written.
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    /// The bytes are not a PNG this tool can decode. The message says why.
    Decode(String),
    /// The image is larger than this tool is willing to hold in memory.
    TooLarge { width: u32, height: u32 },
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Decode(message) => write!(f, "not a readable PNG: {message}"),
            Self::TooLarge { width, height } => write!(
                f,
                "image is {width}x{height}, more than the {MAX_PIXELS} pixels supported"
            ),
        }
    }
}

impl std::error::Error for ImageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Tightly packed 8-bit RGBA pixels, row by row from the top, sRGB-encoded
/// with straight (not premultiplied) alpha. Never empty.
#[derive(Clone, PartialEq, Eq)]
pub struct Image {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl fmt::Debug for Image {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Image({}x{})", self.width, self.height)
    }
}

impl Image {
    /// Wraps existing pixels.
    ///
    /// # Panics
    /// Panics if either dimension is zero or `rgba` is not exactly
    /// `width * height * 4` bytes.
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        assert!(
            width > 0 && height > 0,
            "an image cannot be {width}x{height}"
        );
        assert_eq!(
            rgba.len() as u64,
            u64::from(width) * u64::from(height) * 4,
            "pixel buffer does not match {width}x{height}"
        );
        Self {
            width,
            height,
            rgba,
        }
    }

    /// An image of one colour.
    ///
    /// # Panics
    /// Panics if either dimension is zero.
    pub fn filled(width: u32, height: u32, color: [u8; 4]) -> Self {
        let pixels = width as usize * height as usize;
        Self::new(width, height, color.repeat(pixels))
    }

    /// Decodes a PNG of any bit depth, colour type or interlacing.
    pub fn from_png(bytes: &[u8]) -> Result<Self, ImageError> {
        crate::png::decode(bytes)
    }

    /// Encodes the image as a compressed PNG.
    pub fn to_png(&self) -> Vec<u8> {
        crate::png::encode(self.width, self.height, &self.rgba)
    }

    /// Reads and decodes a PNG file.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ImageError> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(|source| ImageError::Io {
            path: path.to_owned(),
            source,
        })?;
        Self::from_png(&bytes)
    }

    /// Writes the image as a PNG file, creating missing parent directories.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), ImageError> {
        write_file(path.as_ref(), &self.to_png())
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// The pixels: `width * height * 4` bytes, row by row from the top.
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    pub fn into_rgba(self) -> Vec<u8> {
        self.rgba
    }

    /// The pixels, for overwriting in place; the size cannot change.
    pub(crate) fn rgba_mut(&mut self) -> &mut [u8] {
        &mut self.rgba
    }

    /// The pixel at (`x`, `y`).
    ///
    /// # Panics
    /// Panics if the position is outside the image.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let at = self.offset(x, y);
        [
            self.rgba[at],
            self.rgba[at + 1],
            self.rgba[at + 2],
            self.rgba[at + 3],
        ]
    }

    /// Replaces the pixel at (`x`, `y`).
    ///
    /// # Panics
    /// Panics if the position is outside the image.
    pub fn set_pixel(&mut self, x: u32, y: u32, color: [u8; 4]) {
        let at = self.offset(x, y);
        self.rgba[at..at + 4].copy_from_slice(&color);
    }

    /// Fills the part of `rect` that lies inside the image.
    pub fn fill_rect(&mut self, rect: Rect, color: [u8; 4]) {
        let right = rect.x.saturating_add(rect.width).min(self.width);
        let bottom = rect.y.saturating_add(rect.height).min(self.height);
        for y in rect.y..bottom {
            for x in rect.x..right {
                self.set_pixel(x, y, color);
            }
        }
    }

    /// Whether every pixel has full alpha.
    pub fn is_opaque(&self) -> bool {
        self.rgba.chunks_exact(4).all(|pixel| pixel[3] == 255)
    }

    /// Resamples the image to `width` x `height`.
    ///
    /// Each axis is handled on its own: shrinking averages exactly the
    /// source area each new pixel covers (so a 3x export reduced to 1x is the
    /// mean of each 3x3 block, with no aliasing and no ringing), and
    /// enlarging interpolates linearly between pixel centres. Arithmetic is
    /// done in linear light with premultiplied alpha, so a half-covered edge
    /// keeps the brightness it has on screen.
    ///
    /// # Panics
    /// Panics if either dimension is zero.
    pub fn resized(&self, width: u32, height: u32) -> Self {
        assert!(width > 0 && height > 0, "cannot resize to {width}x{height}");
        if (width, height) == (self.width, self.height) {
            return self.clone();
        }
        let to_linear = srgb_to_linear_table();
        let (src_w, src_h) = (self.width as usize, self.height as usize);
        let (dst_w, dst_h) = (width as usize, height as usize);

        // Horizontal pass: source rows, destination columns.
        let columns = axis_weights(self.width, width);
        let mut wide = vec![[0f32; 4]; dst_w * src_h];
        let mut row = vec![[0f32; 4]; src_w];
        for y in 0..src_h {
            let bytes = &self.rgba[y * src_w * 4..(y + 1) * src_w * 4];
            for (texel, pixel) in row.iter_mut().zip(bytes.chunks_exact(4)) {
                let alpha = f32::from(pixel[3]) / 255.0;
                *texel = [
                    to_linear[usize::from(pixel[0])] * alpha,
                    to_linear[usize::from(pixel[1])] * alpha,
                    to_linear[usize::from(pixel[2])] * alpha,
                    alpha,
                ];
            }
            for (out, taps) in wide[y * dst_w..(y + 1) * dst_w].iter_mut().zip(&columns) {
                for (offset, weight) in taps.weights.iter().enumerate() {
                    let texel = row[taps.start + offset];
                    for channel in 0..4 {
                        out[channel] += texel[channel] * weight;
                    }
                }
            }
        }

        // Vertical pass, straight into bytes.
        let rows = axis_weights(self.height, height);
        let mut rgba = Vec::with_capacity(dst_w * dst_h * 4);
        for taps in &rows {
            for x in 0..dst_w {
                let mut sum = [0f32; 4];
                for (offset, weight) in taps.weights.iter().enumerate() {
                    let texel = wide[(taps.start + offset) * dst_w + x];
                    for channel in 0..4 {
                        sum[channel] += texel[channel] * weight;
                    }
                }
                let alpha = sum[3].clamp(0.0, 1.0);
                let scale = if alpha > 0.0 { 1.0 / alpha } else { 0.0 };
                for &channel in &sum[..3] {
                    rgba.push(linear_to_srgb(channel * scale));
                }
                rgba.push((alpha * 255.0).round() as u8);
            }
        }
        Self::new(width, height, rgba)
    }

    fn offset(&self, x: u32, y: u32) -> usize {
        assert!(
            x < self.width && y < self.height,
            "({x}, {y}) is outside a {}x{} image",
            self.width,
            self.height
        );
        (y as usize * self.width as usize + x as usize) * 4
    }
}

/// Writes `bytes` to `path`, creating missing parent directories.
pub(crate) fn write_file(path: &Path, bytes: &[u8]) -> Result<(), ImageError> {
    let io = |source| ImageError::Io {
        path: path.to_owned(),
        source,
    };
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(io)?;
    }
    std::fs::write(path, bytes).map_err(io)
}

/// The source pixels one destination pixel is made from.
struct Taps {
    start: usize,
    weights: Vec<f32>,
}

/// For each of `dst` destination pixels along one axis, which of the `src`
/// source pixels contribute and by how much. Weights sum to one.
fn axis_weights(src: u32, dst: u32) -> Vec<Taps> {
    let (src_len, dst_len) = (f64::from(src), f64::from(dst));
    let last = src as usize - 1;
    (0..dst)
        .map(|index| {
            if src == dst {
                Taps {
                    start: index as usize,
                    weights: vec![1.0],
                }
            } else if src > dst {
                // Area average: the destination pixel covers [from, to) in
                // source coordinates; weight each source pixel by overlap.
                let scale = src_len / dst_len;
                let from = f64::from(index) * scale;
                let to = (f64::from(index) + 1.0) * scale;
                let start = (from.floor() as usize).min(last);
                let end = ((to.ceil() as usize).max(start + 1)).min(last + 1);
                let weights = (start..end)
                    .map(|pixel| {
                        let overlap = to.min(pixel as f64 + 1.0) - from.max(pixel as f64);
                        (overlap.max(0.0) / scale) as f32
                    })
                    .collect();
                Taps { start, weights }
            } else {
                // Linear interpolation between the two nearest source pixel
                // centres; the edge pixel is repeated outside the image.
                let centre = (f64::from(index) + 0.5) * src_len / dst_len - 0.5;
                let centre = centre.clamp(0.0, last as f64);
                let start = (centre.floor() as usize).min(last);
                let fraction = (centre - start as f64) as f32;
                if start == last || fraction == 0.0 {
                    Taps {
                        start,
                        weights: vec![1.0],
                    }
                } else {
                    Taps {
                        start,
                        weights: vec![1.0 - fraction, fraction],
                    }
                }
            }
        })
        .collect()
}

/// sRGB-encoded byte to linear light, for every byte value.
pub(crate) fn srgb_to_linear_table() -> [f32; 256] {
    std::array::from_fn(|value| {
        let encoded = value as f32 / 255.0;
        if encoded <= 0.040_45 {
            encoded / 12.92
        } else {
            ((encoded + 0.055) / 1.055).powf(2.4)
        }
    })
}

/// Linear light to an sRGB-encoded byte.
pub(crate) fn linear_to_srgb(linear: f32) -> u8 {
    let linear = linear.clamp(0.0, 1.0);
    let encoded = if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLACK: [u8; 4] = [0, 0, 0, 255];
    const WHITE: [u8; 4] = [255, 255, 255, 255];

    #[test]
    fn transfer_functions_round_trip_every_byte() {
        let table = srgb_to_linear_table();
        for value in 0..=255u8 {
            assert_eq!(linear_to_srgb(table[usize::from(value)]), value);
        }
    }

    #[test]
    fn resizing_to_the_same_size_changes_nothing() {
        let mut image = Image::filled(5, 3, [10, 20, 30, 255]);
        image.set_pixel(4, 2, [200, 100, 50, 128]);
        assert_eq!(image.resized(5, 3), image);
    }

    #[test]
    fn a_flat_colour_survives_any_resize_exactly() {
        let image = Image::filled(7, 5, [37, 99, 235, 255]);
        for (width, height) in [(21, 15), (3, 2), (1, 1), (10, 4), (4, 10)] {
            let resized = image.resized(width, height);
            assert_eq!((resized.width(), resized.height()), (width, height));
            assert!(
                resized
                    .rgba()
                    .chunks_exact(4)
                    .all(|pixel| pixel == [37, 99, 235, 255]),
                "{width}x{height}"
            );
        }
    }

    #[test]
    fn shrinking_averages_in_linear_light() {
        // A black/white checker averaged to one pixel is 50% linear grey,
        // which is sRGB 188, not the naive 128.
        let mut checker = Image::filled(2, 2, BLACK);
        checker.set_pixel(0, 0, WHITE);
        checker.set_pixel(1, 1, WHITE);
        assert_eq!(checker.resized(1, 1).pixel(0, 0), [188, 188, 188, 255]);
    }

    #[test]
    fn shrinking_by_an_integer_factor_keeps_block_colours() {
        // A 3x export of a 2x1 design: each 3x3 block is one design pixel.
        let mut image = Image::filled(6, 3, [255, 0, 0, 255]);
        image.fill_rect(Rect::new(3, 0, 3, 3), [0, 0, 255, 255]);
        let small = image.resized(2, 1);
        assert_eq!(small.pixel(0, 0), [255, 0, 0, 255]);
        assert_eq!(small.pixel(1, 0), [0, 0, 255, 255]);
    }

    #[test]
    fn shrinking_by_a_fractional_factor_weights_by_coverage() {
        // 3 -> 2: each output covers 1.5 inputs. The middle (white) pixel
        // contributes a third of each output: linear 1/3 is sRGB 156.
        let mut image = Image::filled(3, 1, BLACK);
        image.set_pixel(1, 0, WHITE);
        let small = image.resized(2, 1);
        assert_eq!(small.pixel(0, 0), [156, 156, 156, 255]);
        assert_eq!(small.pixel(1, 0), [156, 156, 156, 255]);
    }

    #[test]
    fn enlarging_keeps_edges_in_place_and_interpolates_between() {
        // 2 -> 4 around a black|white edge: outer pixels stay pure, the two
        // inner ones are 25% and 75% linear white (sRGB 137 and 225).
        let mut image = Image::filled(2, 1, BLACK);
        image.set_pixel(1, 0, WHITE);
        let big = image.resized(4, 1);
        let reds: Vec<u8> = (0..4).map(|x| big.pixel(x, 0)[0]).collect();
        assert_eq!(reds, [0, 137, 225, 255]);
    }

    #[test]
    fn enlarging_by_an_integer_factor_then_shrinking_is_the_identity() {
        let mut image = Image::filled(4, 4, [12, 34, 56, 255]);
        image.fill_rect(Rect::new(1, 1, 2, 2), [250, 128, 0, 255]);
        // Nearest-style 3x enlargement built by hand, as a design tool
        // exports it.
        let mut big = Image::filled(12, 12, [0; 4]);
        for y in 0..12 {
            for x in 0..12 {
                big.set_pixel(x, y, image.pixel(x / 3, y / 3));
            }
        }
        assert_eq!(big.resized(4, 4), image);
    }

    #[test]
    fn transparent_pixels_do_not_bleed_their_colour() {
        // Opaque red next to fully transparent green: the average is half
        // transparent and still pure red.
        let mut image = Image::filled(2, 1, [255, 0, 0, 255]);
        image.set_pixel(1, 0, [0, 255, 0, 0]);
        assert_eq!(image.resized(1, 1).pixel(0, 0), [255, 0, 0, 128]);
    }

    #[test]
    fn axis_weights_sum_to_one() {
        for (src, dst) in [
            (1, 1),
            (1, 7),
            (7, 1),
            (5, 3),
            (3, 5),
            (1080, 393),
            (393, 1080),
        ] {
            for taps in axis_weights(src, dst) {
                let sum: f32 = taps.weights.iter().sum();
                assert!((sum - 1.0).abs() < 1e-4, "{src}->{dst}: {sum}");
                assert!(taps.start + taps.weights.len() <= src as usize);
            }
        }
    }

    #[test]
    fn fill_rect_is_clipped_to_the_image() {
        let mut image = Image::filled(4, 4, BLACK);
        image.fill_rect(Rect::new(2, 2, 100, u32::MAX), WHITE);
        assert_eq!(image.pixel(1, 1), BLACK);
        assert_eq!(image.pixel(3, 3), WHITE);
        assert!(Rect::new(2, 2, 1, 1).contains(2, 2));
        assert!(!Rect::new(2, 2, 1, 1).contains(3, 2));
        assert!(!Rect::new(2, 2, 1, 1).contains(1, 2));
    }

    #[test]
    #[should_panic(expected = "does not match")]
    fn wrong_buffer_size_panics() {
        let _ = Image::new(2, 2, vec![0; 4]);
    }
}
