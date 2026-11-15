//! Moving pictures: the latest decoded frame of each video in a live room
//! (ADR 0039), as plain data. A host hands frames over as I420, which is
//! what a video decoder produces; a renderer turns the three planes into
//! colour on the GPU. Nothing here knows about either.

use crate::Size;

/// Longest side of a frame that is accepted. A larger one is not a picture
/// from a call but a mistake, and is dropped rather than allocated.
pub const MAX_VIDEO_DIMENSION: u32 = 4096;

/// Identifies one video in a live room: someone's camera, or a shared
/// screen. Given by the host in
/// [`LiveEvent::VideoStarted`](crate::LiveEvent::VideoStarted), never
/// reused while the app runs, so the id of a video that has stopped names
/// nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VideoTrack(pub u32);

/// How far a frame must be turned, clockwise, to be upright. A phone's
/// camera delivers its pictures as the sensor lies; the turn travels with
/// each frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VideoRotation {
    /// Upright as it is.
    #[default]
    None,
    /// A quarter turn clockwise.
    Clockwise90,
    /// Upside down.
    Half,
    /// Three quarters clockwise: a quarter turn anticlockwise.
    Clockwise270,
}

impl VideoRotation {
    /// From degrees clockwise, as WebRTC states them. Anything that is not
    /// 90, 180 or 270 is upright.
    pub fn from_degrees(degrees: i32) -> Self {
        match degrees.rem_euclid(360) {
            90 => Self::Clockwise90,
            180 => Self::Half,
            270 => Self::Clockwise270,
            _ => Self::None,
        }
    }

    /// The number of quarter turns clockwise, 0 to 3.
    pub fn quarter_turns(self) -> u32 {
        match self {
            Self::None => 0,
            Self::Clockwise90 => 1,
            Self::Half => 2,
            Self::Clockwise270 => 3,
        }
    }

    fn swaps_sides(self) -> bool {
        matches!(self, Self::Clockwise90 | Self::Clockwise270)
    }
}

/// One decoded picture of a video, as I420: a full-size plane of
/// brightness (Y) and two planes of colour (U and V) at half the size each
/// way, BT.601 limited range, which is what a call's decoder produces.
///
/// The planes are tightly packed: `width` bytes a row for Y, and
/// `(width + 1) / 2` for U and V over `(height + 1) / 2` rows.
#[derive(Clone, PartialEq, Eq)]
pub struct VideoFrame {
    width: u32,
    height: u32,
    rotation: VideoRotation,
    mirrored: bool,
    y: Vec<u8>,
    u: Vec<u8>,
    v: Vec<u8>,
}

impl std::fmt::Debug for VideoFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never the picture itself: it is somebody's face.
        f.debug_struct("VideoFrame")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("rotation", &self.rotation)
            .field("mirrored", &self.mirrored)
            .finish()
    }
}

impl VideoFrame {
    /// A frame from three planes that may have padding at the end of each
    /// row (`*_stride` bytes from one row to the next), copying them into
    /// `reuse` if that is given, so that a stream of frames allocates
    /// once. `None` if the size is zero or over [`MAX_VIDEO_DIMENSION`],
    /// or a plane is shorter than its size and stride say.
    #[allow(clippy::too_many_arguments)]
    pub fn from_i420(
        width: u32,
        height: u32,
        rotation: VideoRotation,
        (y, y_stride): (&[u8], usize),
        (u, u_stride): (&[u8], usize),
        (v, v_stride): (&[u8], usize),
        reuse: Option<VideoFrame>,
    ) -> Option<Self> {
        if width == 0 || height == 0 || width.max(height) > MAX_VIDEO_DIMENSION {
            return None;
        }
        let (w, h) = (width as usize, height as usize);
        let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
        let fits = |plane: &[u8], stride: usize, row: usize, rows: usize| {
            stride >= row && plane.len() >= stride * (rows - 1) + row
        };
        if !fits(y, y_stride, w, h) || !fits(u, u_stride, cw, ch) || !fits(v, v_stride, cw, ch) {
            return None;
        }
        let mut frame = reuse.unwrap_or(Self {
            width: 0,
            height: 0,
            rotation,
            mirrored: false,
            y: Vec::new(),
            u: Vec::new(),
            v: Vec::new(),
        });
        let pack = |into: &mut Vec<u8>, plane: &[u8], stride: usize, row: usize, rows: usize| {
            into.clear();
            into.reserve(row * rows);
            for index in 0..rows {
                let start = index * stride;
                into.extend_from_slice(&plane[start..start + row]);
            }
        };
        pack(&mut frame.y, y, y_stride, w, h);
        pack(&mut frame.u, u, u_stride, cw, ch);
        pack(&mut frame.v, v, v_stride, cw, ch);
        frame.width = width;
        frame.height = height;
        frame.rotation = rotation;
        frame.mirrored = false;
        Some(frame)
    }

    /// A frame made from `width * height * 4` bytes of RGBA (alpha is
    /// ignored): for tests and tools, which have no decoder. Colour is
    /// averaged over each two-by-two block, as an encoder does. `None` for
    /// a size that [`from_i420`](Self::from_i420) would refuse, or the
    /// wrong number of bytes.
    pub fn from_rgba8(width: u32, height: u32, pixels: &[u8]) -> Option<Self> {
        let (w, h) = (width as usize, height as usize);
        if width == 0
            || height == 0
            || width.max(height) > MAX_VIDEO_DIMENSION
            || pixels.len() != w * h * 4
        {
            return None;
        }
        let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
        let rgb = |x: usize, y: usize| {
            let at = (y * w + x) * 4;
            (
                f32::from(pixels[at]),
                f32::from(pixels[at + 1]),
                f32::from(pixels[at + 2]),
            )
        };
        let byte = |value: f32| value.round().clamp(0.0, 255.0) as u8;
        let mut luma = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let (r, g, b) = rgb(x, y);
                luma.push(byte(16.0 + 0.257 * r + 0.504 * g + 0.098 * b));
            }
        }
        let (mut cb, mut cr) = (Vec::with_capacity(cw * ch), Vec::with_capacity(cw * ch));
        for cy in 0..ch {
            for cx in 0..cw {
                let (mut r, mut g, mut b, mut count) = (0.0, 0.0, 0.0, 0.0);
                for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let (x, y) = (cx * 2 + x, cy * 2 + y);
                    if x < w && y < h {
                        let (pr, pg, pb) = rgb(x, y);
                        r += pr;
                        g += pg;
                        b += pb;
                        count += 1.0;
                    }
                }
                let (r, g, b) = (r / count, g / count, b / count);
                cb.push(byte(128.0 - 0.148 * r - 0.291 * g + 0.439 * b));
                cr.push(byte(128.0 + 0.439 * r - 0.368 * g - 0.071 * b));
            }
        }
        Some(Self {
            width,
            height,
            rotation: VideoRotation::None,
            mirrored: false,
            y: luma,
            u: cb,
            v: cr,
        })
    }

    /// The same frame, to be turned by `rotation` when shown.
    pub fn with_rotation(mut self, rotation: VideoRotation) -> Self {
        self.rotation = rotation;
        self
    }

    /// The same frame, shown as in a mirror (after turning): what a front
    /// camera's own preview looks like.
    pub fn with_mirrored(mut self, mirrored: bool) -> Self {
        self.mirrored = mirrored;
        self
    }

    /// Width of the planes as stored, before turning.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height of the planes as stored, before turning.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// How far the picture is turned when shown.
    pub fn rotation(&self) -> VideoRotation {
        self.rotation
    }

    /// Whether the picture is shown as in a mirror.
    pub fn mirrored(&self) -> bool {
        self.mirrored
    }

    /// Width and height of the picture as it is shown: turned upright.
    pub fn display_size(&self) -> (u32, u32) {
        if self.rotation.swaps_sides() {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        }
    }

    /// [`display_size`](Self::display_size) as a [`Size`].
    pub fn size(&self) -> Size {
        let (width, height) = self.display_size();
        Size::new(width as f32, height as f32)
    }

    /// The brightness plane: `width * height` bytes.
    pub fn y(&self) -> &[u8] {
        &self.y
    }

    /// The first colour plane (Cb), half size each way.
    pub fn u(&self) -> &[u8] {
        &self.u
    }

    /// The second colour plane (Cr), half size each way.
    pub fn v(&self) -> &[u8] {
        &self.v
    }

    /// Width and height of each colour plane.
    pub fn chroma_size(&self) -> (u32, u32) {
        (self.width.div_ceil(2), self.height.div_ceil(2))
    }

    /// Where in the stored planes, as fractions of their width and height,
    /// the point `(u, v)` of the shown picture comes from: the turn and
    /// the mirror undone. This is the one statement of what "rotation"
    /// means; the renderer's shader computes the same.
    pub fn source_point(&self, u: f32, v: f32) -> (f32, f32) {
        let u = if self.mirrored { 1.0 - u } else { u };
        match self.rotation {
            VideoRotation::None => (u, v),
            VideoRotation::Clockwise90 => (v, 1.0 - u),
            VideoRotation::Half => (1.0 - u, 1.0 - v),
            VideoRotation::Clockwise270 => (1.0 - v, u),
        }
    }

    /// The picture as it is shown, in RGBA: turned, mirrored and converted
    /// (BT.601, limited range), with colour taken from the nearest sample.
    /// A reference for tests and for tools without a GPU; a renderer does
    /// this in a shader, where colour is also interpolated.
    pub fn to_rgba8(&self) -> (u32, u32, Vec<u8>) {
        let (width, height) = self.display_size();
        let (cw, ch) = self.chroma_size();
        let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
        for row in 0..height {
            for column in 0..width {
                let (su, sv) = self.source_point(
                    (column as f32 + 0.5) / width as f32,
                    (row as f32 + 0.5) / height as f32,
                );
                let at = |fraction: f32, side: u32| {
                    ((fraction * side as f32) as u32).min(side - 1) as usize
                };
                let luma = self.y[at(sv, self.height) * self.width as usize + at(su, self.width)];
                let chroma = at(sv, ch) * cw as usize + at(su, cw);
                pixels.extend_from_slice(&yuv_to_rgba(luma, self.u[chroma], self.v[chroma]));
            }
        }
        (width, height, pixels)
    }
}

/// One sample converted: BT.601 limited range to RGB, opaque.
pub fn yuv_to_rgba(y: u8, u: u8, v: u8) -> [u8; 4] {
    let y = 1.164 * (f32::from(y) - 16.0);
    let (u, v) = (f32::from(u) - 128.0, f32::from(v) - 128.0);
    let byte = |value: f32| value.round().clamp(0.0, 255.0) as u8;
    [
        byte(y + 1.596 * v),
        byte(y - 0.392 * u - 0.813 * v),
        byte(y + 2.017 * u),
        255,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32, rgb: [u8; 3]) -> VideoFrame {
        let pixels: Vec<u8> = (0..width * height)
            .flat_map(|_| [rgb[0], rgb[1], rgb[2], 255])
            .collect();
        VideoFrame::from_rgba8(width, height, &pixels).unwrap()
    }

    #[test]
    fn a_colour_survives_the_trip_through_i420() {
        for rgb in [
            [255, 255, 255],
            [0, 0, 0],
            [200, 30, 40],
            [20, 180, 60],
            [10, 40, 220],
        ] {
            let (_, _, pixels) = solid(4, 2, rgb).to_rgba8();
            for (got, want) in pixels[..3].iter().zip(rgb) {
                assert!(
                    got.abs_diff(want) <= 3,
                    "{rgb:?} came back as {:?}",
                    &pixels[..4]
                );
            }
            assert_eq!(pixels[3], 255);
        }
    }

    #[test]
    fn white_and_black_are_the_limited_range_ends() {
        assert_eq!(solid(2, 2, [255, 255, 255]).y()[0], 235);
        assert_eq!(solid(2, 2, [0, 0, 0]).y()[0], 16);
        assert_eq!(yuv_to_rgba(235, 128, 128), [255, 255, 255, 255]);
        assert_eq!(yuv_to_rgba(16, 128, 128), [0, 0, 0, 255]);
    }

    #[test]
    fn padding_at_the_end_of_rows_is_dropped() {
        // 3 x 2, each row of Y padded to 4 bytes and of U and V to 3.
        let y = [1, 2, 3, 99, 4, 5, 6, 99];
        let (u, v) = ([7, 8, 99], [9, 10, 99]);
        let frame =
            VideoFrame::from_i420(3, 2, VideoRotation::None, (&y, 4), (&u, 3), (&v, 3), None)
                .unwrap();
        assert_eq!(frame.y(), [1, 2, 3, 4, 5, 6]);
        assert_eq!((frame.u(), frame.v()), (&[7, 8][..], &[9, 10][..]));
        assert_eq!(frame.chroma_size(), (2, 1));
    }

    #[test]
    fn a_frame_that_is_too_short_too_large_or_empty_is_refused() {
        let plane = [0u8; 16];
        let make = |width, height, y: &[u8]| {
            VideoFrame::from_i420(
                width,
                height,
                VideoRotation::None,
                (y, width as usize),
                (&plane, 2),
                (&plane, 2),
                None,
            )
        };
        assert!(make(4, 4, &plane).is_some());
        assert!(make(4, 4, &plane[..15]).is_none());
        assert!(make(0, 4, &plane).is_none());
        assert!(make(MAX_VIDEO_DIMENSION + 1, 1, &plane).is_none());
        assert!(VideoFrame::from_rgba8(2, 2, &[0; 15]).is_none());
    }

    #[test]
    fn the_buffers_of_the_frame_before_are_used_again() {
        let plane = vec![7u8; 64];
        let first = VideoFrame::from_i420(
            8,
            8,
            VideoRotation::None,
            (&plane, 8),
            (&plane, 4),
            (&plane, 4),
            None,
        )
        .unwrap();
        let address = first.y().as_ptr();
        let second = VideoFrame::from_i420(
            8,
            8,
            VideoRotation::Half,
            (&plane, 8),
            (&plane, 4),
            (&plane, 4),
            Some(first.with_mirrored(true)),
        )
        .unwrap();
        assert_eq!(second.y().as_ptr(), address);
        assert_eq!(second.rotation(), VideoRotation::Half);
        assert!(!second.mirrored(), "a reused frame starts unmirrored");
    }

    #[test]
    fn turning_swaps_the_sides_and_moves_the_corners() {
        // Left half white, right half black, 4 x 2.
        let mut pixels = Vec::new();
        for _ in 0..2 {
            pixels.extend_from_slice(&[255, 255, 255, 255, 255, 255, 255, 255]);
            pixels.extend_from_slice(&[0, 0, 0, 255, 0, 0, 0, 255]);
        }
        let frame = VideoFrame::from_rgba8(4, 2, &pixels).unwrap();
        let white_at = |frame: &VideoFrame, x: u32, y: u32| {
            let (width, _, pixels) = frame.to_rgba8();
            pixels[((y * width + x) * 4) as usize] > 128
        };
        assert_eq!(frame.display_size(), (4, 2));
        assert!(white_at(&frame, 0, 0) && !white_at(&frame, 3, 0));

        // A quarter turn clockwise: the left edge becomes the top.
        let turned = frame.clone().with_rotation(VideoRotation::Clockwise90);
        assert_eq!(turned.display_size(), (2, 4));
        assert!(white_at(&turned, 0, 0) && white_at(&turned, 1, 0));
        assert!(!white_at(&turned, 0, 3) && !white_at(&turned, 1, 3));

        let half = frame.clone().with_rotation(VideoRotation::Half);
        assert!(!white_at(&half, 0, 0) && white_at(&half, 3, 1));

        // Three quarters: the left edge becomes the bottom.
        let back = frame.clone().with_rotation(VideoRotation::Clockwise270);
        assert_eq!(back.display_size(), (2, 4));
        assert!(!white_at(&back, 0, 0) && white_at(&back, 0, 3));

        let mirrored = frame.with_mirrored(true);
        assert!(!white_at(&mirrored, 0, 0) && white_at(&mirrored, 3, 0));
    }

    #[test]
    fn degrees_are_read_as_quarter_turns() {
        assert_eq!(VideoRotation::from_degrees(0), VideoRotation::None);
        assert_eq!(VideoRotation::from_degrees(90), VideoRotation::Clockwise90);
        assert_eq!(
            VideoRotation::from_degrees(-90),
            VideoRotation::Clockwise270
        );
        assert_eq!(VideoRotation::from_degrees(540), VideoRotation::Half);
        assert_eq!(VideoRotation::from_degrees(45), VideoRotation::None);
        assert_eq!(VideoRotation::Clockwise270.quarter_turns(), 3);
    }

    #[test]
    fn a_frame_does_not_print_its_picture() {
        let text = format!("{:?}", solid(2, 2, [1, 2, 3]));
        assert!(text.contains("width: 2") && !text.contains('['));
    }
}
