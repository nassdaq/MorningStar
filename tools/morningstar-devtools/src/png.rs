//! PNG reading and writing, through the `png` crate.
//!
//! Decoding has to cope with whatever a design tool exports (palettes,
//! 16-bit channels, greyscale, interlacing), which means a real inflate
//! implementation; that is the reason for the dependency (ADR 0007). No
//! `png` type appears in this crate's public API.

use std::io::Cursor;

use crate::image::{Image, ImageError, MAX_PIXELS};

/// Encodes tightly packed RGBA8 pixels (row by row from the top) as a
/// compressed PNG marked as sRGB.
///
/// # Panics
/// Panics if `rgba` is not exactly `width * height * 4` bytes, or if either
/// dimension is zero (a PNG cannot be empty).
pub fn encode(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    assert_eq!(
        rgba.len() as u64,
        u64::from(width) * u64::from(height) * 4,
        "pixel buffer does not match {width}x{height}"
    );
    assert!(
        width > 0 && height > 0,
        "a PNG cannot be empty ({width}x{height})"
    );

    let mut out = Vec::with_capacity(rgba.len() / 8 + 256);
    let mut encoder = ::png::Encoder::new(&mut out, width, height);
    encoder.set_color(::png::ColorType::Rgba);
    encoder.set_depth(::png::BitDepth::Eight);
    // The fast mode is a deflate tuned for PNG: UI screenshots shrink by
    // well over 95% and encoding stays quick even in an unoptimised build.
    encoder.set_compression(::png::Compression::Fast);
    encoder.set_source_srgb(::png::SrgbRenderingIntent::Perceptual);
    // Writing into a Vec cannot fail and the header was validated above.
    let mut writer = encoder
        .write_header()
        .expect("PNG header for a non-empty RGBA8 image");
    writer
        .write_image_data(rgba)
        .expect("PNG data of the declared size");
    writer.finish().expect("PNG trailer");
    out
}

/// Encodes tightly packed RGBA8 pixels as a compressed sRGB PNG with no
/// alpha channel at all: the alpha bytes are dropped, not blended, so
/// flatten the picture first. For the one consumer that refuses a PNG with
/// an alpha channel however opaque: an iOS app icon (ADR 0020).
///
/// # Panics
/// As [`encode`].
pub(crate) fn encode_opaque(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    assert_eq!(
        rgba.len() as u64,
        u64::from(width) * u64::from(height) * 4,
        "pixel buffer does not match {width}x{height}"
    );
    assert!(
        width > 0 && height > 0,
        "a PNG cannot be empty ({width}x{height})"
    );
    let rgb: Vec<u8> = rgba
        .chunks_exact(4)
        .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();

    let mut out = Vec::with_capacity(rgb.len() / 8 + 256);
    let mut encoder = ::png::Encoder::new(&mut out, width, height);
    encoder.set_color(::png::ColorType::Rgb);
    encoder.set_depth(::png::BitDepth::Eight);
    encoder.set_compression(::png::Compression::Fast);
    encoder.set_source_srgb(::png::SrgbRenderingIntent::Perceptual);
    let mut writer = encoder
        .write_header()
        .expect("PNG header for a non-empty RGB8 image");
    writer
        .write_image_data(&rgb)
        .expect("PNG data of the declared size");
    writer.finish().expect("PNG trailer");
    out
}

/// Decodes a PNG into straight-alpha RGBA8.
///
/// Every bit depth, colour type and interlacing mode is accepted; 16-bit
/// channels are reduced to 8. An animated PNG yields its first frame. Colour
/// profiles and gamma chunks are ignored: pixels are taken to be sRGB.
pub fn decode(bytes: &[u8]) -> Result<Image, ImageError> {
    let failed = |error: ::png::DecodingError| ImageError::Decode(error.to_string());

    let mut decoder = ::png::Decoder::new(Cursor::new(bytes));
    // Palette -> RGB, low bit depths -> 8, tRNS -> alpha, 16 -> 8.
    decoder.set_transformations(::png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(failed)?;

    let (width, height) = {
        let info = reader.info();
        (info.width, info.height)
    };
    let too_large = ImageError::TooLarge { width, height };
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(too_large);
    }
    let Some(size) = reader.output_buffer_size() else {
        return Err(too_large);
    };

    let mut buffer = vec![0u8; size];
    let frame = reader.next_frame(&mut buffer).map_err(failed)?;
    if (frame.width, frame.height) != (width, height) || width == 0 || height == 0 {
        return Err(ImageError::Decode(format!(
            "unexpected frame size {}x{} in a {width}x{height} image",
            frame.width, frame.height
        )));
    }
    buffer.truncate(frame.buffer_size());

    let pixels = width as usize * height as usize;
    let channels = match frame.color_type {
        ::png::ColorType::Grayscale => 1,
        ::png::ColorType::GrayscaleAlpha => 2,
        ::png::ColorType::Rgb => 3,
        ::png::ColorType::Rgba => 4,
        ::png::ColorType::Indexed => {
            return Err(ImageError::Decode("palette was not expanded".to_owned()));
        }
    };
    if frame.bit_depth != ::png::BitDepth::Eight || buffer.len() != pixels * channels {
        return Err(ImageError::Decode(format!(
            "unexpected decoded layout: {:?} {:?}, {} bytes for {width}x{height}",
            frame.color_type,
            frame.bit_depth,
            buffer.len()
        )));
    }

    let rgba = if channels == 4 {
        buffer
    } else {
        let mut rgba = Vec::with_capacity(pixels * 4);
        for pixel in buffer.chunks_exact(channels) {
            let expanded = match *pixel {
                [grey] => [grey, grey, grey, 255],
                [grey, alpha] => [grey, grey, grey, alpha],
                [red, green, blue] => [red, green, blue, 255],
                _ => unreachable!("channel count is 1, 2 or 3 here"),
            };
            rgba.extend_from_slice(&expanded);
        }
        rgba
    };
    Ok(Image::new(width, height, rgba))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encodes with arbitrary PNG settings, to produce the kinds of file a
    /// design tool might export.
    fn exotic(
        width: u32,
        height: u32,
        color: ::png::ColorType,
        depth: ::png::BitDepth,
        palette: Option<(&[u8], Option<&[u8]>)>,
        data: &[u8],
    ) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = ::png::Encoder::new(&mut out, width, height);
        encoder.set_color(color);
        encoder.set_depth(depth);
        if let Some((entries, transparency)) = palette {
            encoder.set_palette(entries.to_vec());
            if let Some(transparency) = transparency {
                encoder.set_trns(transparency.to_vec());
            }
        }
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(data).unwrap();
        writer.finish().unwrap();
        out
    }

    fn gradient(width: u32, height: u32) -> Vec<u8> {
        let mut rgba = Vec::new();
        for y in 0..height {
            for x in 0..width {
                rgba.extend_from_slice(&[(x * 7) as u8, (y * 11) as u8, (x + y) as u8, 255]);
            }
        }
        rgba
    }

    #[test]
    fn rgba_round_trips_exactly() {
        let mut rgba = gradient(17, 9);
        rgba[3] = 0;
        rgba[7] = 77;
        let png = encode(17, 9, &rgba);
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let image = decode(&png).unwrap();
        assert_eq!((image.width(), image.height()), (17, 9));
        assert_eq!(image.rgba(), &rgba[..]);
    }

    #[test]
    fn a_flat_screenshot_is_compressed() {
        // The uncompressed encoder this replaced needed 4 bytes a pixel.
        let rgba = [18u8, 52, 86, 255].repeat(256 * 256);
        let png = encode(256, 256, &rgba);
        assert!(png.len() < rgba.len() / 50, "{} bytes", png.len());
        assert_eq!(decode(&png).unwrap().rgba(), &rgba[..]);
    }

    #[test]
    fn rgb_and_greyscale_gain_opaque_alpha() {
        let rgb = exotic(
            2,
            1,
            ::png::ColorType::Rgb,
            ::png::BitDepth::Eight,
            None,
            &[1, 2, 3, 4, 5, 6],
        );
        assert_eq!(decode(&rgb).unwrap().rgba(), [1, 2, 3, 255, 4, 5, 6, 255]);

        let grey = exotic(
            2,
            1,
            ::png::ColorType::Grayscale,
            ::png::BitDepth::Eight,
            None,
            &[9, 200],
        );
        assert_eq!(
            decode(&grey).unwrap().rgba(),
            [9, 9, 9, 255, 200, 200, 200, 255]
        );

        let grey_alpha = exotic(
            1,
            1,
            ::png::ColorType::GrayscaleAlpha,
            ::png::BitDepth::Eight,
            None,
            &[50, 60],
        );
        assert_eq!(decode(&grey_alpha).unwrap().rgba(), [50, 50, 50, 60]);
    }

    #[test]
    fn low_bit_depth_greyscale_is_expanded() {
        // 1 bit per pixel, 8 pixels in one byte: 1010_0000.
        let bits = exotic(
            3,
            1,
            ::png::ColorType::Grayscale,
            ::png::BitDepth::One,
            None,
            &[0b1010_0000],
        );
        assert_eq!(
            decode(&bits).unwrap().rgba(),
            [255, 255, 255, 255, 0, 0, 0, 255, 255, 255, 255, 255]
        );
    }

    #[test]
    fn sixteen_bit_channels_are_reduced_to_eight() {
        // Big-endian samples; the high byte is what survives.
        let data = [0x12, 0x34, 0xAB, 0xCD, 0xFF, 0xFF, 0x80, 0x00];
        let png = exotic(
            1,
            1,
            ::png::ColorType::Rgba,
            ::png::BitDepth::Sixteen,
            None,
            &data,
        );
        assert_eq!(decode(&png).unwrap().rgba(), [0x12, 0xAB, 0xFF, 0x80]);
    }

    #[test]
    fn palettes_and_their_transparency_are_expanded() {
        let palette = [255, 0, 0, 0, 255, 0, 0, 0, 255];
        let transparency = [255, 128];
        let png = exotic(
            3,
            1,
            ::png::ColorType::Indexed,
            ::png::BitDepth::Eight,
            Some((&palette, Some(&transparency))),
            &[2, 1, 0],
        );
        assert_eq!(
            decode(&png).unwrap().rgba(),
            [0, 0, 255, 255, 0, 255, 0, 128, 255, 0, 0, 255]
        );

        let opaque = exotic(
            2,
            1,
            ::png::ColorType::Indexed,
            ::png::BitDepth::Two,
            Some((&palette, None)),
            &[0b0110_0000],
        );
        assert_eq!(
            decode(&opaque).unwrap().rgba(),
            [0, 255, 0, 255, 0, 0, 255, 255]
        );
    }

    /// A hand-assembled Adam7-interlaced PNG (the `png` encoder cannot write
    /// one), using stored deflate blocks and the checksums PNG requires.
    fn interlaced_rgb(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u8; 3]) -> Vec<u8> {
        fn crc32(data: &[u8]) -> u32 {
            let mut crc = u32::MAX;
            for &byte in data {
                crc ^= u32::from(byte);
                for _ in 0..8 {
                    crc = (crc >> 1) ^ (0xEDB8_8320 & (crc & 1).wrapping_neg());
                }
            }
            !crc
        }
        fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            let start = out.len();
            out.extend_from_slice(kind);
            out.extend_from_slice(data);
            let crc = crc32(&out[start..]);
            out.extend_from_slice(&crc.to_be_bytes());
        }

        // (x start, y start, x step, y step) for the seven passes.
        const PASSES: [(u32, u32, u32, u32); 7] = [
            (0, 0, 8, 8),
            (4, 0, 8, 8),
            (0, 4, 4, 8),
            (2, 0, 4, 4),
            (0, 2, 2, 4),
            (1, 0, 2, 2),
            (0, 1, 1, 2),
        ];
        let mut raw = Vec::new();
        for (x0, y0, dx, dy) in PASSES {
            if x0 >= width {
                continue;
            }
            for y in (y0..height).step_by(dy as usize) {
                raw.push(0); // filter: none
                for x in (x0..width).step_by(dx as usize) {
                    raw.extend_from_slice(&pixel(x, y));
                }
            }
        }
        assert!(raw.len() <= 65_535, "one stored block is enough for tests");
        let (mut a, mut b) = (1u32, 0u32);
        for &byte in &raw {
            a = (a + u32::from(byte)) % 65_521;
            b = (b + a) % 65_521;
        }
        let mut zlib = vec![0x78, 0x01, 1];
        zlib.extend_from_slice(&(raw.len() as u16).to_le_bytes());
        zlib.extend_from_slice(&(!(raw.len() as u16)).to_le_bytes());
        zlib.extend_from_slice(&raw);
        zlib.extend_from_slice(&((b << 16) | a).to_be_bytes());

        let mut header = Vec::new();
        header.extend_from_slice(&width.to_be_bytes());
        header.extend_from_slice(&height.to_be_bytes());
        header.extend_from_slice(&[8, 2, 0, 0, 1]); // 8-bit RGB, Adam7

        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        chunk(&mut png, b"IHDR", &header);
        chunk(&mut png, b"IDAT", &zlib);
        chunk(&mut png, b"IEND", &[]);
        png
    }

    #[test]
    fn interlaced_images_are_deinterlaced() {
        let pixel = |x: u32, y: u32| [(x * 20) as u8, (y * 20) as u8, (x * y) as u8];
        let image = decode(&interlaced_rgb(11, 9, pixel)).unwrap();
        assert_eq!((image.width(), image.height()), (11, 9));
        for y in 0..9 {
            for x in 0..11 {
                let [r, g, b] = pixel(x, y);
                assert_eq!(image.pixel(x, y), [r, g, b, 255], "({x}, {y})");
            }
        }
    }

    #[test]
    fn malformed_input_is_an_error_not_a_panic() {
        let good = encode(8, 8, &gradient(8, 8));

        let cases: Vec<(&str, Vec<u8>)> = vec![
            ("empty", Vec::new()),
            ("not a png", b"GIF89a just some other bytes".to_vec()),
            ("signature only", good[..8].to_vec()),
            ("cut in the header", good[..20].to_vec()),
            ("cut in the data", good[..good.len() - 20].to_vec()),
            ("corrupt checksum", {
                let mut bytes = good.clone();
                let at = bytes.len() - 16; // inside the IDAT checksum/data tail
                bytes[at] ^= 0xFF;
                bytes
            }),
            ("corrupt data", {
                let mut bytes = good.clone();
                let middle = bytes.len() / 2;
                for byte in &mut bytes[middle..middle + 8] {
                    *byte = !*byte;
                }
                bytes
            }),
        ];
        for (name, bytes) in cases {
            match decode(&bytes) {
                Err(ImageError::Decode(message)) => assert!(!message.is_empty(), "{name}"),
                other => panic!("{name}: expected a decode error, got {other:?}"),
            }
        }
    }

    #[test]
    fn every_truncation_and_single_byte_corruption_is_handled() {
        // Whatever happens to the bytes, decoding returns: it never panics.
        let good = encode(5, 4, &gradient(5, 4));
        // Losing only the 12-byte end marker still leaves a whole picture,
        // which the decoder may accept; anything shorter is an error.
        for length in 0..good.len() {
            let result = decode(&good[..length]);
            if length < good.len() - 12 {
                assert!(result.is_err(), "truncated to {length}");
            }
        }
        for index in 0..good.len() {
            let mut bytes = good.clone();
            bytes[index] ^= 0x55;
            let _ = decode(&bytes);
        }
    }

    #[test]
    fn absurd_dimensions_are_refused_before_allocating() {
        let mut bytes = encode(1, 1, &[1, 2, 3, 4]);
        // Rewrite IHDR's width and height (bytes 16..24) to 2^31 - 1 each.
        // The checksum is now wrong, which the decoder may report first;
        // either way the answer is an error and no multi-gigabyte buffer.
        bytes[16..24].copy_from_slice(&[0x7F, 0xFF, 0xFF, 0xFF, 0x7F, 0xFF, 0xFF, 0xFF]);
        assert!(decode(&bytes).is_err());

        let huge = interlaced_header_only(20_000, 20_000);
        assert!(matches!(
            decode(&huge),
            Err(ImageError::TooLarge {
                width: 20_000,
                height: 20_000
            })
        ));
    }

    /// A PNG with a valid header claiming a huge size, and no pixel data.
    fn interlaced_header_only(width: u32, height: u32) -> Vec<u8> {
        let mut png = interlaced_rgb(1, 1, |_, _| [0, 0, 0]);
        // Keep the signature, replace IHDR (8 length/type + 13 data + 4 crc).
        let mut out = png[..8].to_vec();
        let mut body = b"IHDR".to_vec();
        body.extend_from_slice(&width.to_be_bytes());
        body.extend_from_slice(&height.to_be_bytes());
        body.extend_from_slice(&[8, 2, 0, 0, 0]);
        let mut crc = u32::MAX;
        for &byte in &body {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xEDB8_8320 & (crc & 1).wrapping_neg());
            }
        }
        out.extend_from_slice(&13u32.to_be_bytes());
        out.extend_from_slice(&body);
        out.extend_from_slice(&(!crc).to_be_bytes());
        out.extend_from_slice(&png.split_off(8 + 25));
        out
    }

    #[test]
    #[should_panic(expected = "does not match")]
    fn wrong_buffer_size_panics() {
        encode(2, 2, &[0; 4]);
    }
}
