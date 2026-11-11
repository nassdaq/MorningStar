//! Decoding of encoded images into the pixels an image is registered from.
//! The only place that uses the `png` crate; nothing of it is public
//! (ADR 0014).

use std::io::Cursor;

use morningstar_core::{ImageError, MAX_IMAGE_BYTES};

/// Decodes a PNG file's contents to `(width, height, pixels)`: tightly
/// packed RGBA8, sRGB-encoded with straight alpha, row by row from the top.
///
/// Every PNG colour type and bit depth is accepted: palettes and
/// transparency chunks are expanded, grey becomes RGB, and 16-bit samples
/// are reduced to 8. Only the first frame of an animated PNG is read.
/// Colour-space chunks (gAMA, cHRM, iCCP) are ignored: the samples are taken
/// to be sRGB, which is what virtually all PNGs are.
///
/// Malformed or truncated data is an error, never a panic.
pub(crate) fn decode_png(data: &[u8]) -> Result<(u32, u32, Vec<u8>), ImageError> {
    let failed = |error: png::DecodingError| ImageError::Decode(error.to_string());

    let mut decoder = png::Decoder::new(Cursor::new(data));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let header = decoder.read_header_info().map_err(failed)?;
    let (width, height) = (header.width, header.height);
    if width == 0 || height == 0 {
        return Err(ImageError::Empty);
    }
    // Checked before anything of that size is allocated: a few bytes of
    // header can claim an image of gigabytes.
    let too_large = ImageError::TooLarge { width, height };
    let rgba_len = (width as usize)
        .checked_mul(height as usize)
        .and_then(|texels| texels.checked_mul(4))
        .filter(|&bytes| bytes <= MAX_IMAGE_BYTES)
        .ok_or(too_large.clone())?;

    let mut reader = decoder.read_info().map_err(failed)?;
    let mut decoded = vec![0; reader.output_buffer_size().ok_or(too_large)?];
    let frame = reader.next_frame(&mut decoded).map_err(failed)?;
    decoded.truncate(frame.buffer_size());

    let channels = match frame.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        // Expanded away by the transformations asked for above.
        png::ColorType::Indexed => {
            return Err(ImageError::Decode("palette was not expanded".into()));
        }
    };
    if frame.bit_depth != png::BitDepth::Eight || decoded.len() != rgba_len / 4 * channels {
        return Err(ImageError::Decode("unexpected decoded layout".into()));
    }
    if channels == 4 {
        return Ok((width, height, decoded));
    }

    let mut pixels = Vec::with_capacity(rgba_len);
    for sample in decoded.chunks_exact(channels) {
        pixels.extend_from_slice(&match *sample {
            [grey] => [grey, grey, grey, 255],
            [grey, alpha] => [grey, grey, grey, alpha],
            [red, green, blue] => [red, green, blue, 255],
            _ => unreachable!("one to three channels"),
        });
    }
    Ok((width, height, pixels))
}
