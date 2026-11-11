//! Comparing a rendered screen with a reference image.
//!
//! [`compare`] answers three questions about a candidate (a screenshot) and a
//! reference (a design export, or a stored screenshot): does it match, by how
//! much does it differ, and where. The answer is a [`Report`] that prints as
//! stable plain text or JSON, plus a heat map and a side-by-side picture.
//!
//! # The error scale
//!
//! Colours are compared in Oklab, a colour space in which equal distances
//! look roughly equally different, with distances multiplied by 100:
//!
//! | Error | Meaning |
//! | --- | --- |
//! | 0 | identical |
//! | 0.3 | one step of one 8-bit channel around mid grey |
//! | 2 | about the smallest difference visible side by side (the default threshold) |
//! | 10 | clearly a different colour |
//! | 100 | black against white |
//!
//! # Anti-aliasing tolerance
//!
//! Two renderers never agree on the pixels along an edge. With
//! `aa_radius = r`, a pixel whose colours differ is still accepted when each
//! image's colour can be explained by the other image's neighbourhood within
//! `r` pixels: it equals one of those neighbours (the edge moved by up to `r`
//! pixels), or it is a mix of the two most different ones (the edge is
//! covered by a different fraction). The test runs in both directions, so
//! something present in one image and absent from the other (a missing
//! hairline, a wrong text colour) still counts, and counts with its full
//! colour difference as its error.

use std::fmt::{self, Write as _};

use crate::image::{Image, Rect, linear_to_srgb, srgb_to_linear_table};

/// How different two images may be and still match.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tolerance {
    /// A pixel differs when its error (see the [module docs](self)) is
    /// greater than this.
    pub threshold: f32,
    /// Anti-aliasing tolerance radius in candidate pixels; 0 compares each
    /// pixel only with the one at the same position.
    pub aa_radius: u32,
    /// The comparison passes when at most this percentage (0 to 100) of the
    /// compared pixels differ.
    pub max_diff_percent: f32,
}

impl Tolerance {
    /// Every pixel must have exactly the same colour.
    pub const EXACT: Self = Self {
        threshold: 0.0,
        aa_radius: 0,
        max_diff_percent: 0.0,
    };

    /// Suits a screenshot against a design, or against a reference rendered
    /// on another GPU: differences below what an eye can see are ignored,
    /// edges may move by a pixel, and one pixel in a thousand may differ.
    pub const DEFAULT: Self = Self {
        threshold: 2.0,
        aa_radius: 1,
        max_diff_percent: 0.1,
    };
}

impl Default for Tolerance {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// What to do when the reference and the candidate have different shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fit {
    /// Scale the reference uniformly so the widths match and align the top
    /// edges. Rows that only one image has are not compared. This is right
    /// for phone screens, where designs and devices share a width and differ
    /// in height.
    #[default]
    Width,
    /// Scale each axis independently so the reference covers the candidate
    /// exactly, distorting it.
    Stretch,
}

/// Options for [`compare`].
#[derive(Clone, Debug, PartialEq)]
pub struct CompareOptions {
    pub tolerance: Tolerance,
    /// Areas of the candidate, in candidate pixels, to leave out: a status
    /// bar in a mock-up, a clock, an avatar.
    pub ignore: Vec<Rect>,
    pub fit: Fit,
    /// Whether an aspect-ratio mismatch alone lets the comparison pass. Off
    /// by default: a comparison that could not cover the whole screen has
    /// not shown that the screen matches.
    pub allow_aspect_mismatch: bool,
    /// Side of the grid cells used to group differing pixels into regions,
    /// in candidate pixels. Differences closer together than about this are
    /// reported as one region. 0 picks 1/64 of the shorter side, between 4
    /// and 64.
    pub cell_size: u32,
    /// How many regions to list, worst first.
    pub max_regions: usize,
}

impl Default for CompareOptions {
    fn default() -> Self {
        Self {
            tolerance: Tolerance::DEFAULT,
            ignore: Vec::new(),
            fit: Fit::Width,
            allow_aspect_mismatch: false,
            cell_size: 0,
            max_regions: 10,
        }
    }
}

/// The two images do not have the same shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AspectMismatch {
    /// Width divided by height of the reference.
    pub reference_aspect: f32,
    /// Width divided by height of the candidate.
    pub candidate_aspect: f32,
    /// The height the candidate would have at its width and the reference's
    /// shape.
    pub expected_height: u32,
    /// Rows at the bottom of the candidate that the reference does not reach
    /// (with [`Fit::Width`]); they were not compared.
    pub uncovered_rows: u32,
    /// Rows at the bottom of the scaled reference that fall below the
    /// candidate (with [`Fit::Width`]); they were not compared.
    pub cropped_rows: u32,
    /// Whether the reference was distorted to fit ([`Fit::Stretch`]).
    pub stretched: bool,
}

/// A cluster of differing pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Region {
    /// Tight bounding box of the differing pixels, in candidate pixels.
    pub rect: Rect,
    /// Number of differing pixels inside.
    pub differing_pixels: u64,
    /// Mean error of those pixels.
    pub mean_error: f32,
    /// Largest error among them.
    pub max_error: f32,
    /// What regions are ordered by: the sum of the errors of the differing
    /// pixels. Large for big areas and for strong differences.
    pub severity: f32,
    /// Average colour of the reference over the differing pixels (sRGB).
    pub reference_color: [u8; 3],
    /// Average colour of the candidate over the differing pixels (sRGB).
    pub candidate_color: [u8; 3],
}

/// The outcome of [`compare`], as numbers.
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    /// Whether the images match within the tolerance.
    pub passed: bool,
    /// Why not, when they do not.
    pub failure: Option<String>,
    /// Width and height of the candidate; all coordinates are in this space.
    pub candidate_size: (u32, u32),
    /// Width and height of the reference as given.
    pub reference_size: (u32, u32),
    /// Whether the reference had to be resampled.
    pub resampled: bool,
    pub aspect_mismatch: Option<AspectMismatch>,
    pub tolerance: Tolerance,
    /// The anti-aliasing radius actually used: `tolerance.aa_radius`
    /// multiplied by the factor the reference was enlarged by (one reference
    /// pixel then spans that many candidate pixels), at most 4.
    pub effective_aa_radius: u32,
    /// Whether transparency took part. It does only when the reference has
    /// some; against an opaque reference the candidate's alpha is ignored.
    pub alpha_compared: bool,
    /// Pixels that were compared.
    pub compared_pixels: u64,
    /// Pixels left out by ignore regions.
    pub ignored_pixels: u64,
    /// Pixels left out because the reference does not reach them.
    pub uncovered_pixels: u64,
    /// Compared pixels whose error is above the threshold.
    pub differing_pixels: u64,
    /// `differing_pixels` as a percentage of `compared_pixels`.
    pub diff_percent: f32,
    /// Mean error over all compared pixels.
    pub mean_error: f32,
    /// Largest error of any compared pixel.
    pub max_error: f32,
    /// Where that pixel is.
    pub max_error_at: (u32, u32),
    /// How many separate regions differ; `regions` lists the worst.
    pub region_count: usize,
    /// The worst regions, most severe first.
    pub regions: Vec<Region>,
    /// Width and height in pixels of one character of `map`.
    pub map_cell: (u32, u32),
    /// A coarse picture of where the differences are, one string per row:
    /// space for none, then `.` `:` `*` `#` for under 5%, 25%, 60% and up to
    /// 100% of the cell's pixels differing, and `~` for a cell that was not
    /// compared at all.
    pub map: Vec<String>,
}

/// The outcome of [`compare`]: the [`Report`], and what is needed to draw it.
#[derive(Clone, Debug)]
pub struct Comparison {
    pub report: Report,
    reference: Image,
    candidate: Image,
    errors: Vec<f32>,
    mask: Vec<u8>,
}

const COMPARED: u8 = 0;
const IGNORED: u8 = 1;
const UNCOVERED: u8 = 2;

/// Compares `candidate` with `reference`.
///
/// The reference is resampled to the candidate's size, so every coordinate
/// in the result is a candidate pixel. See the [module docs](self) for how
/// pixels are judged.
pub fn compare(reference: &Image, candidate: &Image, options: &CompareOptions) -> Comparison {
    let (width, height) = (candidate.width(), candidate.height());
    let (ref_width, ref_height) = (reference.width(), reference.height());
    let tolerance = options.tolerance;

    // --- Bring the reference to the candidate's pixel grid. ---------------
    let expected_height = scaled_length(ref_height, width, ref_width);
    let expected_width = scaled_length(ref_width, height, ref_height);
    // Export sizes are rounded, so allow a pixel either way.
    let same_shape = expected_height.abs_diff(height) <= 1 || expected_width.abs_diff(width) <= 1;

    let (canvas, rows, aspect_mismatch) = if same_shape {
        (reference.resized(width, height), height, None)
    } else {
        let mismatch = |uncovered_rows, cropped_rows, stretched| AspectMismatch {
            reference_aspect: ref_width as f32 / ref_height as f32,
            candidate_aspect: width as f32 / height as f32,
            expected_height,
            uncovered_rows,
            cropped_rows,
            stretched,
        };
        match options.fit {
            Fit::Stretch => (
                reference.resized(width, height),
                height,
                Some(mismatch(0, 0, true)),
            ),
            Fit::Width => {
                // Only the reference rows that can land on the candidate are
                // resampled, so an absurdly tall reference costs nothing.
                let needed = scaled_length(height, ref_width, width).saturating_add(1);
                let source = top_rows(reference, needed.min(ref_height));
                let scaled_height = scaled_length(source.height(), width, ref_width);
                let scaled = source.resized(width, scaled_height);
                let rows = scaled_height.min(height);
                let mut canvas = Image::filled(width, height, UNCOVERED_FILL);
                let bytes = rows as usize * width as usize * 4;
                canvas.rgba_mut()[..bytes].copy_from_slice(&scaled.rgba()[..bytes]);
                (
                    canvas,
                    rows,
                    Some(mismatch(
                        height - rows,
                        expected_height.saturating_sub(height),
                        false,
                    )),
                )
            }
        }
    };

    let enlargement = (width.div_ceil(ref_width)).max(if options.fit == Fit::Stretch {
        height.div_ceil(ref_height)
    } else {
        1
    });
    let effective_aa_radius = tolerance
        .aa_radius
        .saturating_mul(enlargement.max(1))
        .min(4);

    // --- Per-pixel error. --------------------------------------------------
    let alpha_compared = !reference.is_opaque();
    let mut errors = if alpha_compared {
        // Transparency shows as whatever is behind it, so judge the pair
        // over both extremes and keep the worse result.
        let over_black = error_map(
            &Planes::new(&canvas, Some(0.0)),
            &Planes::new(candidate, Some(0.0)),
            width,
            rows,
            effective_aa_radius,
            tolerance.threshold,
        );
        let over_white = error_map(
            &Planes::new(&canvas, Some(1.0)),
            &Planes::new(candidate, Some(1.0)),
            width,
            rows,
            effective_aa_radius,
            tolerance.threshold,
        );
        over_black
            .into_iter()
            .zip(over_white)
            .map(|(a, b)| a.max(b))
            .collect()
    } else {
        error_map(
            &Planes::new(&canvas, None),
            &Planes::new(candidate, None),
            width,
            rows,
            effective_aa_radius,
            tolerance.threshold,
        )
    };
    // `errors` covers the compared rows; give it the full height.
    errors.resize(width as usize * height as usize, 0.0);

    // --- What to leave out. ------------------------------------------------
    let mut mask = vec![COMPARED; width as usize * height as usize];
    mask[rows as usize * width as usize..].fill(UNCOVERED);
    for rect in &options.ignore {
        let right = rect.x.saturating_add(rect.width).min(width);
        let bottom = rect.y.saturating_add(rect.height).min(rows);
        for y in rect.y..bottom {
            let row = y as usize * width as usize;
            if rect.x < right {
                mask[row + rect.x as usize..row + right as usize].fill(IGNORED);
            }
        }
    }
    for (error, &kind) in errors.iter_mut().zip(&mask) {
        if kind != COMPARED {
            *error = 0.0;
        }
    }

    // --- Totals. -----------------------------------------------------------
    let mut compared_pixels = 0u64;
    let mut ignored_pixels = 0u64;
    let mut uncovered_pixels = 0u64;
    let mut differing_pixels = 0u64;
    let mut error_sum = 0f64;
    let mut max_error = 0f32;
    let mut max_error_at = (0u32, 0u32);
    for (index, (&error, &kind)) in errors.iter().zip(&mask).enumerate() {
        match kind {
            IGNORED => ignored_pixels += 1,
            UNCOVERED => uncovered_pixels += 1,
            _ => {
                compared_pixels += 1;
                error_sum += f64::from(error);
                if error > tolerance.threshold {
                    differing_pixels += 1;
                }
                if error > max_error {
                    max_error = error;
                    max_error_at = (
                        (index % width as usize) as u32,
                        (index / width as usize) as u32,
                    );
                }
            }
        }
    }
    let (diff_percent, mean_error) = if compared_pixels == 0 {
        (0.0, 0.0)
    } else {
        (
            (differing_pixels as f64 * 100.0 / compared_pixels as f64) as f32,
            (error_sum / compared_pixels as f64) as f32,
        )
    };

    // --- Where. ------------------------------------------------------------
    let cell_size = match options.cell_size {
        0 => (width.min(height) / 64).clamp(4, 64),
        size => size,
    };
    let mut regions = find_regions(&canvas, candidate, &errors, tolerance.threshold, cell_size);
    let region_count = regions.len();
    regions.truncate(options.max_regions);
    let (map_cell, map) = ascii_map(&errors, &mask, width, height, tolerance.threshold);

    // --- Verdict. ----------------------------------------------------------
    let mut reasons = Vec::new();
    // `differing_pixels > 0` guards the comparison against rounding when the
    // limit is zero.
    if differing_pixels > 0 && diff_percent > tolerance.max_diff_percent {
        reasons.push(format!(
            "{diff_percent:.4}% of pixels differ (limit {:.4}%)",
            tolerance.max_diff_percent
        ));
    }
    if let Some(mismatch) = aspect_mismatch.filter(|_| !options.allow_aspect_mismatch) {
        reasons.push(format!(
            "aspect ratio mismatch: a {width} px wide candidate should be {} px tall to match \
             the reference, not {height}",
            mismatch.expected_height
        ));
    }
    let failure = (!reasons.is_empty()).then(|| reasons.join("; "));

    Comparison {
        report: Report {
            passed: failure.is_none(),
            failure,
            candidate_size: (width, height),
            reference_size: (ref_width, ref_height),
            resampled: (ref_width, ref_height) != (width, height),
            aspect_mismatch,
            tolerance,
            effective_aa_radius,
            alpha_compared,
            compared_pixels,
            ignored_pixels,
            uncovered_pixels,
            differing_pixels,
            diff_percent,
            mean_error,
            max_error,
            max_error_at,
            region_count,
            regions,
            map_cell,
            map,
        },
        reference: canvas,
        candidate: candidate.clone(),
        errors,
        mask,
    }
}

/// `length * numerator / denominator`, rounded, at least 1.
fn scaled_length(length: u32, numerator: u32, denominator: u32) -> u32 {
    let scaled = (u64::from(length) * u64::from(numerator) + u64::from(denominator) / 2)
        / u64::from(denominator);
    scaled.clamp(1, u64::from(u32::MAX)) as u32
}

fn top_rows(image: &Image, rows: u32) -> Image {
    let rows = rows.clamp(1, image.height());
    let bytes = rows as usize * image.width() as usize * 4;
    Image::new(image.width(), rows, image.rgba()[..bytes].to_vec())
}

// ---------------------------------------------------------------------------
// Colour
// ---------------------------------------------------------------------------

/// An image as linear RGB (for mixing) and scaled Oklab (for distances).
struct Planes {
    linear: Vec<[f32; 3]>,
    lab: Vec<[f32; 3]>,
}

impl Planes {
    /// `background` is the linear grey to composite over, or `None` to
    /// ignore alpha altogether.
    fn new(image: &Image, background: Option<f32>) -> Self {
        let table = srgb_to_linear_table();
        let pixels = image.rgba().len() / 4;
        let mut linear = Vec::with_capacity(pixels);
        let mut lab = Vec::with_capacity(pixels);
        // Screens are mostly runs of one colour; remember the last result.
        let mut last_pixel = [0u8; 4];
        let mut last = ([0f32; 3], oklab([0.0; 3]));
        let mut primed = false;
        for pixel in image.rgba().chunks_exact(4) {
            let pixel = match background {
                Some(_) => [pixel[0], pixel[1], pixel[2], pixel[3]],
                None => [pixel[0], pixel[1], pixel[2], 255],
            };
            if !primed || pixel != last_pixel {
                let mut rgb = [
                    table[usize::from(pixel[0])],
                    table[usize::from(pixel[1])],
                    table[usize::from(pixel[2])],
                ];
                if let Some(background) = background {
                    let alpha = f32::from(pixel[3]) / 255.0;
                    for channel in &mut rgb {
                        *channel = *channel * alpha + background * (1.0 - alpha);
                    }
                }
                last = (rgb, oklab(rgb));
                last_pixel = pixel;
                primed = true;
            }
            linear.push(last.0);
            lab.push(last.1);
        }
        Self { linear, lab }
    }
}

/// Linear sRGB to Oklab (Björn Ottosson, 2020), scaled so that black to
/// white is a distance of 100.
fn oklab([r, g, b]: [f32; 3]) -> [f32; 3] {
    let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    [
        100.0 * (0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s),
        100.0 * (1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s),
        100.0 * (0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s),
    ]
}

fn distance_squared(a: [f32; 3], b: [f32; 3]) -> f32 {
    let (x, y, z) = (a[0] - b[0], a[1] - b[1], a[2] - b[2]);
    x * x + y * y + z * z
}

// ---------------------------------------------------------------------------
// Per-pixel error
// ---------------------------------------------------------------------------

/// The error of each pixel in the top `rows` rows. See the module docs.
fn error_map(
    reference: &Planes,
    candidate: &Planes,
    width: u32,
    rows: u32,
    radius: u32,
    threshold: f32,
) -> Vec<f32> {
    let (width, rows) = (width as usize, rows as usize);
    let mut errors = Vec::with_capacity(width * rows);
    for y in 0..rows {
        for x in 0..width {
            let index = y * width + x;
            let direct = distance_squared(reference.lab[index], candidate.lab[index]).sqrt();
            if direct <= threshold || radius == 0 {
                errors.push(direct);
                continue;
            }
            let window = Window::new(x, y, width, rows, radius as usize);
            let reference_unexplained = unexplained(
                reference.lab[index],
                reference.linear[index],
                candidate,
                &window,
                threshold,
            );
            let candidate_unexplained = unexplained(
                candidate.lab[index],
                candidate.linear[index],
                reference,
                &window,
                threshold,
            );
            // Explained by the neighbourhood: report what little remains.
            // Not explained: report the real colour difference, so that the
            // magnitude of a differing pixel means the same at any radius.
            let remaining = reference_unexplained.max(candidate_unexplained);
            errors.push(if remaining <= threshold {
                remaining
            } else {
                direct
            });
        }
    }
    errors
}

/// The pixels within `radius` of a position, clipped to the image.
struct Window {
    x0: usize,
    x1: usize,
    y0: usize,
    y1: usize,
    stride: usize,
}

impl Window {
    fn new(x: usize, y: usize, width: usize, rows: usize, radius: usize) -> Self {
        Self {
            x0: x.saturating_sub(radius),
            x1: (x + radius).min(width - 1),
            y0: y.saturating_sub(radius),
            y1: (y + radius).min(rows - 1),
            stride: width,
        }
    }

    /// Calls `visit` with the buffer index of every pixel in the window.
    /// Plain loops: this is the hot path, and it has to stay quick in an
    /// unoptimised build.
    fn for_each(&self, mut visit: impl FnMut(usize)) {
        for y in self.y0..=self.y1 {
            let row = y * self.stride;
            for index in row + self.x0..=row + self.x1 {
                visit(index);
            }
        }
    }
}

/// How far `color` is from anything the `other` image's neighbourhood could
/// produce at this position: the distance to the nearest neighbour, or to
/// the nearest mix of the neighbourhood's two most different colours.
fn unexplained(
    lab: [f32; 3],
    linear: [f32; 3],
    other: &Planes,
    window: &Window,
    threshold: f32,
) -> f32 {
    let mut nearest = f32::INFINITY;
    let mut mean = [0f32; 3];
    let mut count = 0f32;
    window.for_each(|index| {
        let neighbour = other.lab[index];
        nearest = nearest.min(distance_squared(lab, neighbour));
        mean[0] += neighbour[0];
        mean[1] += neighbour[1];
        mean[2] += neighbour[2];
        count += 1.0;
    });
    let nearest = nearest.sqrt();
    if nearest <= threshold {
        return nearest;
    }
    for channel in &mut mean {
        *channel /= count;
    }

    // The two ends of the edge: the colour farthest from the average, and
    // the colour farthest from that one.
    let farthest_from = |origin: [f32; 3]| {
        let mut best = (window.y0 * window.stride + window.x0, -1.0f32);
        window.for_each(|index| {
            let distance = distance_squared(origin, other.lab[index]);
            if distance > best.1 {
                best = (index, distance);
            }
        });
        best
    };
    let (first, _) = farthest_from(mean);
    let (second, span) = farthest_from(other.lab[first]);
    if span <= f32::EPSILON {
        return nearest; // A flat neighbourhood has no edge to blend along.
    }

    // Coverage blends in linear light, so find the mix there and measure how
    // far it is from `color` perceptually.
    let (a, b) = (other.linear[first], other.linear[second]);
    let along = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let length_squared = along[0] * along[0] + along[1] * along[1] + along[2] * along[2];
    if length_squared <= f32::EPSILON {
        return nearest;
    }
    let t = (((linear[0] - a[0]) * along[0]
        + (linear[1] - a[1]) * along[1]
        + (linear[2] - a[2]) * along[2])
        / length_squared)
        .clamp(0.0, 1.0);
    let mix = [
        a[0] + along[0] * t,
        a[1] + along[1] * t,
        a[2] + along[2] * t,
    ];
    nearest.min(distance_squared(lab, oklab(mix)).sqrt())
}

// ---------------------------------------------------------------------------
// Regions
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Cell {
    count: u64,
    error_sum: f64,
    max_error: f32,
    min_x: u32,
    min_y: u32,
    max_x: u32,
    max_y: u32,
    reference: [f64; 3],
    candidate: [f64; 3],
}

impl Cell {
    const EMPTY: Self = Self {
        count: 0,
        error_sum: 0.0,
        max_error: 0.0,
        min_x: u32::MAX,
        min_y: u32::MAX,
        max_x: 0,
        max_y: 0,
        reference: [0.0; 3],
        candidate: [0.0; 3],
    };

    fn absorb(&mut self, other: &Self) {
        self.count += other.count;
        self.error_sum += other.error_sum;
        self.max_error = self.max_error.max(other.max_error);
        self.min_x = self.min_x.min(other.min_x);
        self.min_y = self.min_y.min(other.min_y);
        self.max_x = self.max_x.max(other.max_x);
        self.max_y = self.max_y.max(other.max_y);
        for channel in 0..3 {
            self.reference[channel] += other.reference[channel];
            self.candidate[channel] += other.candidate[channel];
        }
    }
}

/// Groups differing pixels into regions: the image is cut into square cells,
/// and cells that contain differences and touch (including diagonally) form
/// one region. Returned most severe first.
fn find_regions(
    reference: &Image,
    candidate: &Image,
    errors: &[f32],
    threshold: f32,
    cell_size: u32,
) -> Vec<Region> {
    let (width, height) = (candidate.width(), candidate.height());
    let columns = width.div_ceil(cell_size) as usize;
    let rows = height.div_ceil(cell_size) as usize;
    let table = srgb_to_linear_table();
    let mut cells = vec![Cell::EMPTY; columns * rows];

    for y in 0..height {
        for x in 0..width {
            let index = y as usize * width as usize + x as usize;
            let error = errors[index];
            if error <= threshold {
                continue;
            }
            let cell = &mut cells[(y / cell_size) as usize * columns + (x / cell_size) as usize];
            cell.count += 1;
            cell.error_sum += f64::from(error);
            cell.max_error = cell.max_error.max(error);
            cell.min_x = cell.min_x.min(x);
            cell.min_y = cell.min_y.min(y);
            cell.max_x = cell.max_x.max(x);
            cell.max_y = cell.max_y.max(y);
            let (r, c) = (
                &reference.rgba()[index * 4..],
                &candidate.rgba()[index * 4..],
            );
            for channel in 0..3 {
                cell.reference[channel] += f64::from(table[usize::from(r[channel])]);
                cell.candidate[channel] += f64::from(table[usize::from(c[channel])]);
            }
        }
    }

    let mut visited = vec![false; cells.len()];
    let mut stack = Vec::new();
    let mut regions = Vec::new();
    for start in 0..cells.len() {
        if visited[start] || cells[start].count == 0 {
            continue;
        }
        let mut total = Cell::EMPTY;
        visited[start] = true;
        stack.push(start);
        while let Some(at) = stack.pop() {
            total.absorb(&cells[at]);
            let (column, row) = (at % columns, at / columns);
            for near_row in row.saturating_sub(1)..=(row + 1).min(rows - 1) {
                for near_column in column.saturating_sub(1)..=(column + 1).min(columns - 1) {
                    let near = near_row * columns + near_column;
                    if !visited[near] && cells[near].count > 0 {
                        visited[near] = true;
                        stack.push(near);
                    }
                }
            }
        }
        let mean_color =
            |sum: [f64; 3]| sum.map(|c| linear_to_srgb((c / total.count as f64) as f32));
        regions.push(Region {
            rect: Rect::new(
                total.min_x,
                total.min_y,
                total.max_x - total.min_x + 1,
                total.max_y - total.min_y + 1,
            ),
            differing_pixels: total.count,
            mean_error: (total.error_sum / total.count as f64) as f32,
            max_error: total.max_error,
            severity: total.error_sum as f32,
            reference_color: mean_color(total.reference),
            candidate_color: mean_color(total.candidate),
        });
    }
    regions.sort_by(|a, b| {
        b.severity
            .total_cmp(&a.severity)
            .then(a.rect.y.cmp(&b.rect.y))
            .then(a.rect.x.cmp(&b.rect.x))
    });
    regions
}

/// A character grid at most 16 wide summarising where pixels differ. Cells
/// are twice as tall as wide, as characters are, so the picture keeps the
/// screen's shape in a terminal.
fn ascii_map(
    errors: &[f32],
    mask: &[u8],
    width: u32,
    height: u32,
    threshold: f32,
) -> ((u32, u32), Vec<String>) {
    let cell_width = width.div_ceil(16).max(1);
    let cell_height = cell_width * 2;
    let columns = width.div_ceil(cell_width) as usize;
    let rows = height.div_ceil(cell_height) as usize;
    let mut differing = vec![0u32; columns * rows];
    let mut compared = vec![0u32; columns * rows];
    for y in 0..height {
        for x in 0..width {
            let index = y as usize * width as usize + x as usize;
            if mask[index] != COMPARED {
                continue;
            }
            let cell = (y / cell_height) as usize * columns + (x / cell_width) as usize;
            compared[cell] += 1;
            if errors[index] > threshold {
                differing[cell] += 1;
            }
        }
    }
    let map = (0..rows)
        .map(|row| {
            (0..columns)
                .map(|column| {
                    let cell = row * columns + column;
                    let fraction = differing[cell] as f32 / compared[cell].max(1) as f32;
                    match (compared[cell], differing[cell]) {
                        (0, _) => '~',
                        (_, 0) => ' ',
                        _ if fraction < 0.05 => '.',
                        _ if fraction < 0.25 => ':',
                        _ if fraction < 0.60 => '*',
                        _ => '#',
                    }
                })
                .collect()
        })
        .collect();
    ((cell_width, cell_height), map)
}

// ---------------------------------------------------------------------------
// Text and JSON
// ---------------------------------------------------------------------------

fn hex([r, g, b]: [u8; 3]) -> String {
    format!("#{r:02X}{g:02X}{b:02X}")
}

impl Report {
    /// The report as plain text: one `key: value` per line, in a fixed
    /// order, with fixed number formats. Safe to parse and to diff.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        self.write_text(&mut out)
            .expect("writing to a String cannot fail");
        out
    }

    fn write_text(&self, out: &mut String) -> fmt::Result {
        writeln!(out, "morningstar-diff report v1")?;
        writeln!(out, "result: {}", if self.passed { "PASS" } else { "FAIL" })?;
        if let Some(failure) = &self.failure {
            writeln!(out, "reason: {failure}")?;
        }
        let (width, height) = self.candidate_size;
        let (ref_width, ref_height) = self.reference_size;
        writeln!(out, "candidate: {width}x{height}")?;
        write!(out, "reference: {ref_width}x{ref_height}")?;
        if self.resampled {
            write!(out, " (resampled to the candidate)")?;
        }
        writeln!(out)?;
        match &self.aspect_mismatch {
            None => writeln!(out, "aspect: match")?,
            Some(m) => {
                write!(
                    out,
                    "aspect: MISMATCH reference={:.4} candidate={:.4} expected_candidate_height={}",
                    m.reference_aspect, m.candidate_aspect, m.expected_height
                )?;
                if m.stretched {
                    write!(out, " handling=stretched")?;
                } else {
                    write!(
                        out,
                        " handling=top-aligned uncovered_rows={} cropped_rows={}",
                        m.uncovered_rows, m.cropped_rows
                    )?;
                }
                writeln!(out)?;
            }
        }
        writeln!(
            out,
            "threshold: {:.2} (error scale: 0 identical, 2 just visible, 100 black vs white)",
            self.tolerance.threshold
        )?;
        writeln!(
            out,
            "aa_radius: {} (effective {})",
            self.tolerance.aa_radius, self.effective_aa_radius
        )?;
        writeln!(
            out,
            "max_diff_percent: {:.4}",
            self.tolerance.max_diff_percent
        )?;
        writeln!(
            out,
            "alpha: {}",
            if self.alpha_compared {
                "compared"
            } else {
                "ignored (reference is opaque)"
            }
        )?;
        writeln!(out, "compared_pixels: {}", self.compared_pixels)?;
        writeln!(out, "ignored_pixels: {}", self.ignored_pixels)?;
        writeln!(out, "uncovered_pixels: {}", self.uncovered_pixels)?;
        writeln!(out, "differing_pixels: {}", self.differing_pixels)?;
        writeln!(out, "diff_percent: {:.4}", self.diff_percent)?;
        writeln!(out, "mean_error: {:.3}", self.mean_error)?;
        writeln!(
            out,
            "max_error: {:.2} at {},{}",
            self.max_error, self.max_error_at.0, self.max_error_at.1
        )?;
        writeln!(
            out,
            "regions: {} (listing {}, worst first; x,y from the top-left in candidate pixels)",
            self.region_count,
            self.regions.len()
        )?;
        for (index, region) in self.regions.iter().enumerate() {
            writeln!(
                out,
                "region {}: x={} y={} w={} h={} pixels={} mean_error={:.2} max_error={:.2} \
                 reference={} candidate={}",
                index + 1,
                region.rect.x,
                region.rect.y,
                region.rect.width,
                region.rect.height,
                region.differing_pixels,
                region.mean_error,
                region.max_error,
                hex(region.reference_color),
                hex(region.candidate_color),
            )?;
        }
        writeln!(
            out,
            "map: {}x{} cells of {}x{} px; ' ' none, '.' <5%, ':' <25%, '*' <60%, '#' more, \
             '~' not compared",
            self.map.first().map_or(0, String::len),
            self.map.len(),
            self.map_cell.0,
            self.map_cell.1
        )?;
        for row in &self.map {
            writeln!(out, "map |{row}|")?;
        }
        Ok(())
    }

    /// The report as a JSON object with the same content as
    /// [`to_text`](Self::to_text), keys in a fixed order.
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        self.write_json(&mut out)
            .expect("writing to a String cannot fail");
        out
    }

    fn write_json(&self, out: &mut String) -> fmt::Result {
        let size =
            |(width, height): (u32, u32)| format!("{{\"width\": {width}, \"height\": {height}}}");
        writeln!(out, "{{")?;
        writeln!(out, "  \"format\": \"morningstar-diff report v1\",")?;
        writeln!(out, "  \"passed\": {},", self.passed)?;
        match &self.failure {
            Some(failure) => writeln!(out, "  \"failure\": {},", json_string(failure))?,
            None => writeln!(out, "  \"failure\": null,")?,
        }
        writeln!(out, "  \"candidate\": {},", size(self.candidate_size))?;
        writeln!(out, "  \"reference\": {},", size(self.reference_size))?;
        writeln!(out, "  \"resampled\": {},", self.resampled)?;
        match &self.aspect_mismatch {
            None => writeln!(out, "  \"aspect_mismatch\": null,")?,
            Some(m) => writeln!(
                out,
                "  \"aspect_mismatch\": {{\"reference_aspect\": {:.4}, \"candidate_aspect\": {:.4}, \
                 \"expected_candidate_height\": {}, \"stretched\": {}, \"uncovered_rows\": {}, \
                 \"cropped_rows\": {}}},",
                m.reference_aspect,
                m.candidate_aspect,
                m.expected_height,
                m.stretched,
                m.uncovered_rows,
                m.cropped_rows
            )?,
        }
        writeln!(out, "  \"threshold\": {:.2},", self.tolerance.threshold)?;
        writeln!(out, "  \"aa_radius\": {},", self.tolerance.aa_radius)?;
        writeln!(
            out,
            "  \"effective_aa_radius\": {},",
            self.effective_aa_radius
        )?;
        writeln!(
            out,
            "  \"max_diff_percent\": {:.4},",
            self.tolerance.max_diff_percent
        )?;
        writeln!(out, "  \"alpha_compared\": {},", self.alpha_compared)?;
        writeln!(out, "  \"compared_pixels\": {},", self.compared_pixels)?;
        writeln!(out, "  \"ignored_pixels\": {},", self.ignored_pixels)?;
        writeln!(out, "  \"uncovered_pixels\": {},", self.uncovered_pixels)?;
        writeln!(out, "  \"differing_pixels\": {},", self.differing_pixels)?;
        writeln!(out, "  \"diff_percent\": {:.4},", self.diff_percent)?;
        writeln!(out, "  \"mean_error\": {:.3},", self.mean_error)?;
        writeln!(out, "  \"max_error\": {:.2},", self.max_error)?;
        writeln!(
            out,
            "  \"max_error_at\": {{\"x\": {}, \"y\": {}}},",
            self.max_error_at.0, self.max_error_at.1
        )?;
        writeln!(out, "  \"region_count\": {},", self.region_count)?;
        writeln!(out, "  \"regions\": [")?;
        for (index, region) in self.regions.iter().enumerate() {
            writeln!(
                out,
                "    {{\"rank\": {}, \"x\": {}, \"y\": {}, \"width\": {}, \"height\": {}, \
                 \"differing_pixels\": {}, \"mean_error\": {:.2}, \"max_error\": {:.2}, \
                 \"reference_color\": \"{}\", \"candidate_color\": \"{}\"}}{}",
                index + 1,
                region.rect.x,
                region.rect.y,
                region.rect.width,
                region.rect.height,
                region.differing_pixels,
                region.mean_error,
                region.max_error,
                hex(region.reference_color),
                hex(region.candidate_color),
                if index + 1 < self.regions.len() {
                    ","
                } else {
                    ""
                },
            )?;
        }
        writeln!(out, "  ],")?;
        writeln!(out, "  \"map_cell\": {},", size(self.map_cell))?;
        writeln!(out, "  \"map\": [")?;
        for (index, row) in self.map.iter().enumerate() {
            writeln!(
                out,
                "    {}{}",
                json_string(row),
                if index + 1 < self.map.len() { "," } else { "" }
            )?;
        }
        writeln!(out, "  ]")?;
        writeln!(out, "}}")
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_text())
    }
}

fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

// ---------------------------------------------------------------------------
// Pictures
// ---------------------------------------------------------------------------

/// What the reference canvas shows where the reference does not reach.
const UNCOVERED_FILL: [u8; 4] = [48, 40, 64, 255];
const REGION_OUTLINE: [u8; 4] = [0, 255, 255, 255];
const GUTTER: [u8; 4] = [16, 16, 16, 255];

/// 3x5 digits, one row per entry, most significant bit on the left.
const DIGITS: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b010, 0b010, 0b010],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
];

impl Comparison {
    /// The error of the pixel at (`x`, `y`) in candidate coordinates; 0 for
    /// pixels that were not compared.
    ///
    /// # Panics
    /// Panics if the position is outside the candidate.
    pub fn error_at(&self, x: u32, y: u32) -> f32 {
        let (width, height) = self.report.candidate_size;
        assert!(
            x < width && y < height,
            "({x}, {y}) is outside {width}x{height}"
        );
        self.errors[y as usize * width as usize + x as usize]
    }

    /// The reference as it was compared: resampled to the candidate's size.
    pub fn reference(&self) -> &Image {
        &self.reference
    }

    /// A picture of where the images differ, the size of the candidate.
    ///
    /// The candidate is shown in muted grey. Differing pixels are painted
    /// over it from yellow (just above the threshold) to red (an error of 50
    /// or more). Ignored areas are tinted blue, areas the reference does not
    /// reach purple. Each listed region is outlined in cyan and numbered as
    /// in the report.
    pub fn heat_map(&self) -> Image {
        let (width, height) = self.report.candidate_size;
        let threshold = self.report.tolerance.threshold;
        let mut rgba = Vec::with_capacity(self.errors.len() * 4);
        for ((pixel, &error), &kind) in self
            .candidate
            .rgba()
            .chunks_exact(4)
            .zip(&self.errors)
            .zip(&self.mask)
        {
            let luma =
                (u32::from(pixel[0]) * 54 + u32::from(pixel[1]) * 183 + u32::from(pixel[2]) * 19)
                    >> 8;
            let grey = (64 + luma / 2) as u8;
            let color = match kind {
                IGNORED => [grey / 2 + 16, grey / 2 + 40, grey / 2 + 110],
                UNCOVERED => [grey / 2 + 70, grey / 2 + 16, grey / 2 + 110],
                _ if error > threshold => {
                    let heat = ((error - threshold) / (50.0 - threshold).max(1.0)).clamp(0.0, 1.0);
                    [255, (225.0 * (1.0 - heat)) as u8, 0]
                }
                _ => [grey, grey, grey],
            };
            rgba.extend_from_slice(&[color[0], color[1], color[2], 255]);
        }
        let mut image = Image::new(width, height, rgba);

        let scale = (width.min(height) / 360).max(1);
        // Drawn last-to-first so the worst region's label ends up on top.
        for (index, region) in self.report.regions.iter().enumerate().rev() {
            let margin = 2 * scale;
            let left = region.rect.x.saturating_sub(margin);
            let top = region.rect.y.saturating_sub(margin);
            let right = (region.rect.x + region.rect.width + margin).min(width);
            let bottom = (region.rect.y + region.rect.height + margin).min(height);
            let outline = Rect::new(left, top, right - left, bottom - top);
            draw_outline(&mut image, outline, scale, REGION_OUTLINE);
            draw_number(&mut image, index + 1, outline, scale);
        }
        image
    }

    /// Reference, candidate and heat map next to each other, in that order,
    /// separated by dark gutters.
    pub fn side_by_side(&self) -> Image {
        let (width, height) = self.report.candidate_size;
        let gutter = (width / 100).max(2);
        let total = width * 3 + gutter * 2;
        let heat_map = self.heat_map();
        let row_bytes = width as usize * 4;
        let mut rgba = Vec::with_capacity(total as usize * height as usize * 4);
        for y in 0..height as usize {
            let row = y * row_bytes..(y + 1) * row_bytes;
            for (panel, image) in [&self.reference, &self.candidate, &heat_map]
                .into_iter()
                .enumerate()
            {
                if panel > 0 {
                    rgba.extend(GUTTER.repeat(gutter as usize));
                }
                // Show what was compared: without alpha when it was ignored.
                for pixel in image.rgba()[row.clone()].chunks_exact(4) {
                    let alpha = if self.report.alpha_compared {
                        pixel[3]
                    } else {
                        255
                    };
                    rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], alpha]);
                }
            }
        }
        Image::new(total, height, rgba)
    }
}

fn draw_outline(image: &mut Image, rect: Rect, thickness: u32, color: [u8; 4]) {
    let thickness = thickness.min(rect.width).min(rect.height);
    let (right, bottom) = (rect.x + rect.width, rect.y + rect.height);
    image.fill_rect(Rect::new(rect.x, rect.y, rect.width, thickness), color);
    image.fill_rect(
        Rect::new(rect.x, bottom - thickness, rect.width, thickness),
        color,
    );
    image.fill_rect(Rect::new(rect.x, rect.y, thickness, rect.height), color);
    image.fill_rect(
        Rect::new(right - thickness, rect.y, thickness, rect.height),
        color,
    );
}

/// Draws `number` on a small dark plate just above the top-left corner of
/// `anchor`, or just inside it when there is no room above.
fn draw_number(image: &mut Image, number: usize, anchor: Rect, scale: u32) {
    let digits: Vec<usize> = number
        .to_string()
        .bytes()
        .map(|b| usize::from(b - b'0'))
        .collect();
    let plate_width = (digits.len() as u32 * 4 + 1) * scale;
    let plate_height = 7 * scale;
    let x = anchor.x.min(image.width().saturating_sub(plate_width));
    let y = if anchor.y >= plate_height {
        anchor.y - plate_height
    } else {
        anchor.y
    };
    image.fill_rect(Rect::new(x, y, plate_width, plate_height), [0, 0, 0, 255]);
    for (position, &digit) in digits.iter().enumerate() {
        for (row, bits) in DIGITS[digit].iter().enumerate() {
            for column in 0..3u32 {
                if bits & (0b100 >> column) != 0 {
                    image.fill_rect(
                        Rect::new(
                            x + (1 + position as u32 * 4 + column) * scale,
                            y + (1 + row as u32) * scale,
                            scale,
                            scale,
                        ),
                        REGION_OUTLINE,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: [u8; 4] = [255, 255, 255, 255];
    const BLACK: [u8; 4] = [0, 0, 0, 255];
    const BLUE: [u8; 4] = [37, 99, 235, 255];
    const RED: [u8; 4] = [220, 38, 38, 255];

    fn strict() -> CompareOptions {
        CompareOptions {
            tolerance: Tolerance::EXACT,
            ..CompareOptions::default()
        }
    }

    /// A small "screen": white, with a blue card and a black hairline.
    fn screen() -> Image {
        let mut image = Image::filled(96, 160, WHITE);
        image.fill_rect(Rect::new(16, 24, 64, 40), BLUE);
        image.fill_rect(Rect::new(16, 100, 64, 1), BLACK);
        image
    }

    #[test]
    fn oklab_matches_published_values() {
        let close = |a: [f32; 3], b: [f32; 3]| distance_squared(a, b).sqrt() < 0.05;
        assert!(close(oklab([1.0, 1.0, 1.0]), [100.0, 0.0, 0.0]));
        assert!(close(oklab([0.0, 0.0, 0.0]), [0.0, 0.0, 0.0]));
        // Ottosson's reference values for the sRGB primaries.
        assert!(close(oklab([1.0, 0.0, 0.0]), [62.80, 22.49, 12.58]));
        assert!(close(oklab([0.0, 1.0, 0.0]), [86.64, -23.39, 17.95]));
        assert!(close(oklab([0.0, 0.0, 1.0]), [45.20, -3.25, -31.15]));
    }

    #[test]
    fn identical_images_score_zero() {
        for options in [strict(), CompareOptions::default()] {
            let report = compare(&screen(), &screen(), &options).report;
            assert!(report.passed);
            assert_eq!(report.failure, None);
            assert_eq!(report.differing_pixels, 0);
            assert_eq!(report.diff_percent, 0.0);
            assert_eq!(report.mean_error, 0.0);
            assert_eq!(report.max_error, 0.0);
            assert_eq!(report.compared_pixels, 96 * 160);
            assert!(report.regions.is_empty());
            assert!(!report.resampled);
            assert!(report.map.iter().all(|row| row.trim().is_empty()));
        }
    }

    #[test]
    fn black_against_white_is_one_hundred_everywhere() {
        let comparison = compare(
            &Image::filled(8, 8, WHITE),
            &Image::filled(8, 8, BLACK),
            &CompareOptions::default(),
        );
        let report = &comparison.report;
        assert!(!report.passed);
        assert_eq!(report.differing_pixels, 64);
        assert_eq!(report.diff_percent, 100.0);
        assert!(
            (report.mean_error - 100.0).abs() < 0.01,
            "{}",
            report.mean_error
        );
        assert!((report.max_error - 100.0).abs() < 0.01);
        assert_eq!(report.regions.len(), 1);
        assert_eq!(report.regions[0].rect, Rect::new(0, 0, 8, 8));
        assert_eq!(report.regions[0].reference_color, [255, 255, 255]);
        assert_eq!(report.regions[0].candidate_color, [0, 0, 0]);
    }

    #[test]
    fn a_wrong_colour_is_found_where_it_is_with_its_magnitude() {
        let reference = screen();
        let mut candidate = screen();
        // The card is drawn in red instead of blue.
        candidate.fill_rect(Rect::new(16, 24, 64, 40), RED);
        let expected_error = distance_squared(
            Planes::new(&Image::filled(1, 1, BLUE), None).lab[0],
            Planes::new(&Image::filled(1, 1, RED), None).lab[0],
        )
        .sqrt();
        assert!(expected_error > 30.0, "red and blue are far apart");

        for options in [strict(), CompareOptions::default()] {
            let comparison = compare(&reference, &candidate, &options);
            let report = &comparison.report;
            assert!(!report.passed);
            assert_eq!(report.differing_pixels, 64 * 40);
            assert_eq!(report.region_count, 1);
            let region = report.regions[0];
            assert_eq!(region.rect, Rect::new(16, 24, 64, 40));
            assert_eq!(region.differing_pixels, 64 * 40);
            assert!((region.mean_error - expected_error).abs() < 0.01);
            assert!((region.max_error - expected_error).abs() < 0.01);
            assert!((report.max_error - expected_error).abs() < 0.01);
            assert_eq!(region.reference_color, [37, 99, 235]);
            assert_eq!(region.candidate_color, [220, 38, 38]);
            assert!(report.max_error_at.0 >= 16 && report.max_error_at.0 < 80);
            assert!(report.max_error_at.1 >= 24 && report.max_error_at.1 < 64);
            assert!((comparison.error_at(40, 40) - expected_error).abs() < 0.01);
            assert_eq!(comparison.error_at(2, 2), 0.0);
            let percent = 64.0 * 40.0 * 100.0 / (96.0 * 160.0);
            assert!((report.diff_percent - percent).abs() < 0.001);
        }
    }

    #[test]
    fn regions_are_separate_and_ordered_by_severity() {
        let reference = Image::filled(200, 200, WHITE);
        let mut candidate = reference.clone();
        candidate.fill_rect(Rect::new(150, 150, 10, 10), [240, 240, 240, 255]); // faint
        candidate.fill_rect(Rect::new(10, 20, 30, 30), BLACK); // big and strong
        candidate.fill_rect(Rect::new(120, 10, 6, 6), BLACK); // small and strong
        let report = compare(&reference, &candidate, &CompareOptions::default()).report;
        let rects: Vec<Rect> = report.regions.iter().map(|region| region.rect).collect();
        assert_eq!(
            rects,
            [
                Rect::new(10, 20, 30, 30),
                Rect::new(120, 10, 6, 6),
                Rect::new(150, 150, 10, 10)
            ]
        );
        assert!(report.regions[0].severity > report.regions[1].severity);
        assert!(report.regions[1].severity > report.regions[2].severity);
        assert!(report.regions[2].mean_error > 2.0 && report.regions[2].mean_error < 10.0);

        let limited = compare(
            &reference,
            &candidate,
            &CompareOptions {
                max_regions: 1,
                ..CompareOptions::default()
            },
        )
        .report;
        assert_eq!(limited.region_count, 3);
        assert_eq!(limited.regions.len(), 1);
        assert_eq!(limited.regions[0].rect, Rect::new(10, 20, 30, 30));
    }

    #[test]
    fn the_threshold_decides_which_pixels_count() {
        let reference = Image::filled(10, 10, [128, 128, 128, 255]);
        let mut candidate = reference.clone();
        candidate.fill_rect(Rect::new(0, 0, 5, 10), [131, 131, 131, 255]);
        let run = |threshold: f32| {
            compare(
                &reference,
                &candidate,
                &CompareOptions {
                    tolerance: Tolerance {
                        threshold,
                        aa_radius: 0,
                        max_diff_percent: 0.0,
                    },
                    ..CompareOptions::default()
                },
            )
            .report
        };
        // Three steps of grey is an error of about 1: invisible.
        let exact = run(0.0);
        assert!(
            exact.max_error > 0.7 && exact.max_error < 1.3,
            "{}",
            exact.max_error
        );
        assert_eq!(exact.differing_pixels, 50);
        assert!(!exact.passed);

        let just_below = run(exact.max_error - 0.01);
        assert_eq!(just_below.differing_pixels, 50);
        let just_above = run(exact.max_error + 0.01);
        assert_eq!(just_above.differing_pixels, 0);
        assert!(just_above.passed);
        // The error itself is still reported.
        assert_eq!(just_above.max_error, exact.max_error);
        assert!(just_above.mean_error > 0.0);
        assert!(run(2.0).passed, "the default threshold tolerates it");
    }

    #[test]
    fn the_allowed_percentage_decides_the_verdict() {
        let reference = Image::filled(100, 100, WHITE);
        let mut candidate = reference.clone();
        candidate.fill_rect(Rect::new(0, 0, 10, 5), BLACK); // 50 of 10,000: 0.5%
        let run = |max_diff_percent: f32| {
            compare(
                &reference,
                &candidate,
                &CompareOptions {
                    tolerance: Tolerance {
                        max_diff_percent,
                        ..Tolerance::DEFAULT
                    },
                    ..CompareOptions::default()
                },
            )
            .report
        };
        assert!((run(0.0).diff_percent - 0.5).abs() < 1e-4);
        assert!(!run(0.0).passed);
        assert!(!run(0.49).passed);
        assert!(run(0.51).passed);
        // A passing report still lists what differs.
        assert_eq!(run(0.51).regions.len(), 1);
        assert!(
            run(0.49)
                .failure
                .unwrap()
                .contains("0.5000% of pixels differ")
        );
    }

    #[test]
    fn an_edge_moved_by_one_pixel_is_tolerated_only_with_a_radius() {
        let bar = |x: u32| {
            let mut image = Image::filled(40, 40, WHITE);
            image.fill_rect(Rect::new(x, 0, 20, 40), BLUE);
            image
        };
        let reference = bar(10);

        let exact = compare(&reference, &bar(11), &strict()).report;
        assert_eq!(exact.differing_pixels, 80, "a column of 40 on each side");

        let tolerant = compare(&reference, &bar(11), &CompareOptions::default()).report;
        assert_eq!(tolerant.differing_pixels, 0);
        assert_eq!(tolerant.max_error, 0.0);
        assert!(tolerant.passed);

        // Two pixels is beyond a radius of one...
        let report = compare(&reference, &bar(12), &CompareOptions::default()).report;
        assert_eq!(report.differing_pixels, 160, "two columns on each side");
        // ...and within a radius of two.
        let wider = CompareOptions {
            tolerance: Tolerance {
                aa_radius: 2,
                ..Tolerance::DEFAULT
            },
            ..CompareOptions::default()
        };
        assert_eq!(
            compare(&reference, &bar(12), &wider)
                .report
                .differing_pixels,
            0
        );
    }

    #[test]
    fn different_edge_coverage_is_tolerated() {
        // The same edge anti-aliased at 30% and at 70% coverage.
        let edge = |coverage: f32| {
            let mut image = Image::filled(20, 20, WHITE);
            image.fill_rect(Rect::new(10, 0, 10, 20), BLUE);
            let table = srgb_to_linear_table();
            let mix = |channel: usize| {
                let ink = table[usize::from(BLUE[channel])];
                linear_to_srgb(ink * coverage + 1.0 - coverage)
            };
            image.fill_rect(Rect::new(9, 0, 1, 20), [mix(0), mix(1), mix(2), 255]);
            image
        };
        let (soft, hard) = (edge(0.3), edge(0.7));
        let exact = compare(&soft, &hard, &strict()).report;
        assert_eq!(exact.differing_pixels, 20);
        assert!(exact.max_error > 10.0);
        let tolerant = compare(&soft, &hard, &CompareOptions::default()).report;
        assert_eq!(
            tolerant.differing_pixels, 0,
            "max error {}",
            tolerant.max_error
        );
    }

    #[test]
    fn the_radius_does_not_hide_missing_or_recoloured_thin_features() {
        let reference = screen();

        // The hairline is missing.
        let mut missing = screen();
        missing.fill_rect(Rect::new(16, 100, 64, 1), WHITE);
        let report = compare(&reference, &missing, &CompareOptions::default()).report;
        assert_eq!(report.differing_pixels, 64);
        assert_eq!(report.regions[0].rect, Rect::new(16, 100, 64, 1));
        assert!((report.max_error - 100.0).abs() < 0.01);

        // The hairline is red instead of black: not a mix of black and white.
        let mut recoloured = screen();
        recoloured.fill_rect(Rect::new(16, 100, 64, 1), RED);
        let report = compare(&reference, &recoloured, &CompareOptions::default()).report;
        assert_eq!(report.differing_pixels, 64);
        assert_eq!(report.regions[0].rect, Rect::new(16, 100, 64, 1));
        assert_eq!(report.regions[0].candidate_color, [220, 38, 38]);
    }

    #[test]
    fn ignore_regions_remove_pixels_from_the_comparison() {
        let reference = screen();
        let mut candidate = screen();
        candidate.fill_rect(Rect::new(0, 0, 96, 12), BLACK); // a status bar
        candidate.fill_rect(Rect::new(40, 120, 8, 8), RED); // a real difference

        let unmasked = compare(&reference, &candidate, &CompareOptions::default()).report;
        assert_eq!(unmasked.differing_pixels, 96 * 12 + 64);
        assert_eq!(unmasked.ignored_pixels, 0);

        let options = CompareOptions {
            // The second rectangle hangs off the image and is clipped.
            ignore: vec![Rect::new(0, 0, 96, 12), Rect::new(90, 150, 500, 500)],
            ..CompareOptions::default()
        };
        let comparison = compare(&reference, &candidate, &options);
        let report = &comparison.report;
        assert_eq!(report.ignored_pixels, 96 * 12 + 6 * 10);
        assert_eq!(report.compared_pixels, 96 * 160 - 96 * 12 - 60);
        assert_eq!(report.differing_pixels, 64);
        assert_eq!(report.region_count, 1);
        assert_eq!(report.regions[0].rect, Rect::new(40, 120, 8, 8));
        assert_eq!(comparison.error_at(5, 5), 0.0);

        // Ignoring the remaining difference too makes it pass.
        let all = CompareOptions {
            ignore: vec![Rect::new(0, 0, 96, 12), Rect::new(40, 120, 8, 8)],
            ..CompareOptions::default()
        };
        assert!(compare(&reference, &candidate, &all).report.passed);
    }

    #[test]
    fn a_reference_at_another_resolution_is_resampled() {
        // A "3x export": every pixel of the screen becomes a 3x3 block.
        let small = screen();
        let mut export = Image::filled(96 * 3, 160 * 3, WHITE);
        for y in 0..160 * 3 {
            for x in 0..96 * 3 {
                export.set_pixel(x, y, small.pixel(x / 3, y / 3));
            }
        }
        let report = compare(&export, &small, &strict()).report;
        assert!(report.passed, "{}", report.to_text());
        assert!(report.resampled);
        assert_eq!(report.aspect_mismatch, None);
        assert_eq!(report.reference_size, (288, 480));
        assert_eq!(report.candidate_size, (96, 160));
        assert_eq!(report.effective_aa_radius, 0);

        // The other way round the reference is enlarged, which blurs it: the
        // tolerance radius grows to match and the comparison still passes.
        let report = compare(&small, &export, &CompareOptions::default()).report;
        assert_eq!(report.effective_aa_radius, 3);
        assert!(report.passed, "{}", report.to_text());

        // A real difference survives the resampling, in candidate pixels.
        let mut wrong = small.clone();
        wrong.fill_rect(Rect::new(16, 24, 64, 40), RED);
        let report = compare(&export, &wrong, &CompareOptions::default()).report;
        assert_eq!(report.regions[0].rect, Rect::new(16, 24, 64, 40));
    }

    #[test]
    fn export_sizes_rounded_by_a_pixel_are_not_a_mismatch() {
        // 131x284 scaled to 360 wide is 780.46 tall; tools round either way,
        // and may round the width instead.
        let reference = Image::filled(131, 284, BLUE);
        for (width, height) in [(360, 780), (360, 781), (360, 779), (361, 782)] {
            let candidate = Image::filled(width, height, BLUE);
            let report = compare(&reference, &candidate, &strict()).report;
            assert_eq!(report.aspect_mismatch, None, "{width}x{height}");
            assert_eq!(report.compared_pixels, u64::from(width * height));
            assert!(report.passed);
        }
        let candidate = Image::filled(360, 790, BLUE);
        let report = compare(&reference, &candidate, &strict()).report;
        assert_eq!(report.aspect_mismatch.unwrap().expected_height, 780);
    }

    #[test]
    fn an_aspect_mismatch_is_reported_and_fails_unless_allowed() {
        // The design is 100x200; the screen is rendered 100x240 and the
        // content matches from the top.
        let mut reference = Image::filled(100, 200, WHITE);
        reference.fill_rect(Rect::new(10, 10, 80, 30), BLUE);
        let mut candidate = Image::filled(100, 240, WHITE);
        candidate.fill_rect(Rect::new(10, 10, 80, 30), BLUE);

        let report = compare(&reference, &candidate, &CompareOptions::default()).report;
        let mismatch = report.aspect_mismatch.expect("mismatch is reported");
        assert_eq!(mismatch.reference_aspect, 0.5);
        assert!((mismatch.candidate_aspect - 100.0 / 240.0).abs() < 1e-6);
        assert_eq!(mismatch.expected_height, 200);
        assert_eq!(mismatch.uncovered_rows, 40);
        assert_eq!(mismatch.cropped_rows, 0);
        assert!(!mismatch.stretched);
        assert_eq!(report.uncovered_pixels, 40 * 100);
        assert_eq!(report.compared_pixels, 200 * 100);
        // Nothing was stretched, so the covered part matches exactly...
        assert_eq!(report.differing_pixels, 0);
        // ...but the comparison is incomplete, so it does not pass.
        assert!(!report.passed);
        assert!(report.failure.as_deref().unwrap().contains("aspect ratio"));
        assert!(report.to_text().contains("aspect: MISMATCH"));
        assert!(report.map.last().unwrap().chars().all(|c| c == '~'));

        let allowed = CompareOptions {
            allow_aspect_mismatch: true,
            ..CompareOptions::default()
        };
        let report = compare(&reference, &candidate, &allowed).report;
        assert!(report.passed);
        assert!(report.aspect_mismatch.is_some(), "still reported");

        // A reference taller than the candidate is cropped instead.
        let report = compare(&candidate, &reference, &allowed).report;
        let mismatch = report.aspect_mismatch.unwrap();
        assert_eq!(mismatch.expected_height, 240);
        assert_eq!((mismatch.uncovered_rows, mismatch.cropped_rows), (0, 40));
        assert_eq!(report.uncovered_pixels, 0);
        assert!(report.passed);
    }

    #[test]
    fn stretching_is_opt_in_and_says_so() {
        let mut reference = Image::filled(100, 200, WHITE);
        reference.fill_rect(Rect::new(10, 100, 80, 30), BLUE);
        let mut candidate = Image::filled(100, 240, WHITE);
        candidate.fill_rect(Rect::new(10, 120, 80, 36), BLUE);
        let options = CompareOptions {
            fit: Fit::Stretch,
            allow_aspect_mismatch: true,
            ..CompareOptions::default()
        };
        let report = compare(&reference, &candidate, &options).report;
        let mismatch = report.aspect_mismatch.unwrap();
        assert!(mismatch.stretched);
        assert_eq!(report.uncovered_pixels, 0);
        assert_eq!(report.compared_pixels, 100 * 240);
        assert!(report.passed, "{}", report.to_text());
        assert!(report.to_text().contains("handling=stretched"));
    }

    #[test]
    fn an_extreme_reference_shape_does_not_blow_up() {
        let reference = Image::filled(2, 4000, WHITE);
        let candidate = Image::filled(300, 100, WHITE);
        let report = compare(&reference, &candidate, &CompareOptions::default()).report;
        let mismatch = report.aspect_mismatch.unwrap();
        assert_eq!(mismatch.expected_height, 600_000);
        assert_eq!(mismatch.cropped_rows, 599_900);
        assert_eq!(report.differing_pixels, 0);

        let report = compare(&candidate, &reference, &CompareOptions::default()).report;
        assert_eq!(report.aspect_mismatch.unwrap().uncovered_rows, 3999);
        assert_eq!(report.compared_pixels, 2);
    }

    #[test]
    fn alpha_is_ignored_against_an_opaque_reference() {
        let reference = Image::filled(4, 4, BLUE);
        let candidate = Image::filled(4, 4, [BLUE[0], BLUE[1], BLUE[2], 0]);
        let report = compare(&reference, &candidate, &strict()).report;
        assert!(!report.alpha_compared);
        assert!(report.passed);
        assert!(report.to_text().contains("alpha: ignored"));
    }

    #[test]
    fn alpha_counts_when_the_reference_has_some() {
        let mut reference = Image::filled(4, 4, BLUE);
        reference.set_pixel(0, 0, [0, 0, 0, 0]);
        // Same pixels: identical, including the transparent one, whatever
        // colour hides behind zero alpha.
        let mut same = reference.clone();
        same.set_pixel(0, 0, [255, 0, 255, 0]);
        let report = compare(&reference, &same, &strict()).report;
        assert!(report.alpha_compared);
        assert!(report.passed, "{}", report.to_text());

        // An opaque candidate differs exactly where the reference is clear.
        let opaque = Image::filled(4, 4, BLUE);
        let report = compare(&reference, &opaque, &strict()).report;
        assert_eq!(report.differing_pixels, 1);
        assert_eq!(report.max_error_at, (0, 0));
        assert!(report.max_error > 40.0);
    }

    #[test]
    fn the_text_report_is_stable() {
        let reference = Image::filled(32, 32, WHITE);
        let mut candidate = reference.clone();
        candidate.fill_rect(Rect::new(4, 8, 6, 2), BLACK);
        let options = CompareOptions {
            ignore: vec![Rect::new(0, 30, 32, 2)],
            ..CompareOptions::default()
        };
        let text = compare(&reference, &candidate, &options).report.to_text();
        let expected = "\
morningstar-diff report v1
result: FAIL
reason: 1.2500% of pixels differ (limit 0.1000%)
candidate: 32x32
reference: 32x32
aspect: match
threshold: 2.00 (error scale: 0 identical, 2 just visible, 100 black vs white)
aa_radius: 1 (effective 1)
max_diff_percent: 0.1000
alpha: ignored (reference is opaque)
compared_pixels: 960
ignored_pixels: 64
uncovered_pixels: 0
differing_pixels: 12
diff_percent: 1.2500
mean_error: 1.250
max_error: 100.00 at 4,8
regions: 1 (listing 1, worst first; x,y from the top-left in candidate pixels)
region 1: x=4 y=8 w=6 h=2 pixels=12 mean_error=100.00 max_error=100.00 reference=#FFFFFF candidate=#000000
map: 16x8 cells of 2x4 px; ' ' none, '.' <5%, ':' <25%, '*' <60%, '#' more, '~' not compared
map |                |
map |                |
map |  ***           |
map |                |
map |                |
map |                |
map |                |
map |                |
";
        assert_eq!(text, expected);
    }

    #[test]
    fn the_json_report_is_well_formed_and_complete() {
        let reference = Image::filled(100, 200, WHITE);
        let mut candidate = Image::filled(100, 240, WHITE);
        candidate.fill_rect(Rect::new(4, 8, 6, 2), BLACK);
        candidate.fill_rect(Rect::new(60, 100, 6, 2), RED);
        let report = compare(&reference, &candidate, &CompareOptions::default()).report;
        let json = report.to_json();

        // Balanced, and no trailing commas: enough to catch a slip in the
        // hand-written serialiser without a JSON parser.
        let mut depth = 0i32;
        let mut in_string = false;
        let mut previous = ' ';
        let mut last_significant = ' ';
        for character in json.chars() {
            if in_string {
                if character == '"' && previous != '\\' {
                    in_string = false;
                }
            } else {
                match character {
                    '"' => in_string = true,
                    '{' | '[' => depth += 1,
                    '}' | ']' => {
                        assert_ne!(last_significant, ',', "trailing comma");
                        depth -= 1;
                    }
                    _ => {}
                }
                assert!(depth >= 0);
            }
            if !character.is_whitespace() {
                last_significant = character;
            }
            previous = character;
        }
        assert_eq!(depth, 0);
        assert!(!in_string);

        for needle in [
            "\"passed\": false",
            "\"candidate\": {\"width\": 100, \"height\": 240}",
            "\"reference\": {\"width\": 100, \"height\": 200}",
            "\"expected_candidate_height\": 200",
            "\"uncovered_rows\": 40",
            "\"differing_pixels\": 24",
            "\"region_count\": 2",
            "\"rank\": 1, \"x\": 4, \"y\": 8, \"width\": 6, \"height\": 2",
            "\"candidate_color\": \"#DC2626\"",
            "\"failure\": \"",
        ] {
            assert!(json.contains(needle), "missing {needle} in\n{json}");
        }
        assert_eq!(json_string("a\"b\\c\nd"), "\"a\\\"b\\\\c\\nd\"");
    }

    #[test]
    fn the_heat_map_marks_differences_and_nothing_else() {
        let reference = Image::filled(64, 64, WHITE);
        let mut candidate = reference.clone();
        candidate.fill_rect(Rect::new(20, 30, 8, 8), BLACK);
        let options = CompareOptions {
            ignore: vec![Rect::new(0, 0, 64, 4)],
            ..CompareOptions::default()
        };
        let comparison = compare(&reference, &candidate, &options);
        let heat_map = comparison.heat_map();
        assert_eq!((heat_map.width(), heat_map.height()), (64, 64));
        // A differing pixel at full error is pure red.
        assert_eq!(heat_map.pixel(24, 34), [255, 0, 0, 255]);
        // An untouched white pixel is light grey.
        assert_eq!(heat_map.pixel(60, 60), [191, 191, 191, 255]);
        // An ignored pixel is tinted blue.
        let ignored = heat_map.pixel(60, 1);
        assert!(ignored[2] > ignored[0] + 50);
        // The region is outlined two pixels outside its bounding box.
        assert_eq!(heat_map.pixel(18, 34), REGION_OUTLINE);
        assert_eq!(heat_map.pixel(29, 34), REGION_OUTLINE);
        assert_eq!(heat_map.pixel(24, 39), REGION_OUTLINE);

        // A weaker difference is nearer yellow.
        let mut faint = reference.clone();
        faint.fill_rect(Rect::new(20, 30, 8, 8), [225, 225, 225, 255]);
        let pixel = compare(&reference, &faint, &options)
            .heat_map()
            .pixel(24, 34);
        assert_eq!(pixel[0], 255);
        assert!(pixel[1] > 150, "{pixel:?}");
    }

    #[test]
    fn the_side_by_side_shows_reference_candidate_and_diff() {
        let reference = Image::filled(50, 20, BLUE);
        let candidate = Image::filled(50, 20, RED);
        let comparison = compare(&reference, &candidate, &CompareOptions::default());
        let picture = comparison.side_by_side();
        assert_eq!((picture.width(), picture.height()), (50 * 3 + 2 * 2, 20));
        assert_eq!(picture.pixel(25, 10), BLUE);
        assert_eq!(picture.pixel(50, 10), GUTTER);
        assert_eq!(picture.pixel(52 + 25, 10), RED);
        assert_eq!(picture.pixel(103, 10), GUTTER);
        assert_eq!(
            picture.pixel(104 + 25, 10),
            comparison.heat_map().pixel(25, 10)
        );
        assert!(picture.is_opaque());
    }

    #[test]
    fn region_labels_fit_inside_tiny_and_edge_cases() {
        // Differences in every corner of a very small image, and more
        // regions than digits: drawing must stay inside the picture.
        let reference = Image::filled(9, 9, WHITE);
        let mut candidate = reference.clone();
        for (x, y) in [(0, 0), (8, 0), (0, 8), (8, 8)] {
            candidate.set_pixel(x, y, BLACK);
        }
        let options = CompareOptions {
            cell_size: 1,
            max_regions: 12,
            ..CompareOptions::default()
        };
        let comparison = compare(&reference, &candidate, &options);
        assert_eq!(comparison.report.region_count, 4);
        let _ = comparison.heat_map();
        let _ = comparison.side_by_side();

        let mut many = Image::filled(120, 8, WHITE);
        for index in 0..12 {
            many.set_pixel(index * 10, 4, BLACK);
        }
        let comparison = compare(&Image::filled(120, 8, WHITE), &many, &options);
        assert_eq!(comparison.report.regions.len(), 12);
        let _ = comparison.heat_map();
    }
}
