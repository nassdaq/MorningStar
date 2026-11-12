//! Rendering an SVG file to an image with the loader applications use
//! ([`SvgPicture`], ADR 0027): what a file will look like in an app, seen
//! without building one.

use morningstar_runtime::{Color, SvgPicture};

use crate::Image;

/// What is put behind a rendered SVG.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SvgBackground {
    /// Nothing: the PNG keeps the picture's transparency.
    Transparent,
    /// One sRGB colour.
    Color([u8; 3]),
    /// Light and dark grey squares, which show both white and black
    /// artwork and where a picture is transparent.
    Checker,
}

/// An opaque colour from sRGB bytes, as a design writes `#RRGGBB`.
pub fn color_from_srgb(rgb: [u8; 3]) -> Color {
    let decode = |byte: u8| {
        let encoded = f32::from(byte) / 255.0;
        if encoded <= 0.040_45 {
            encoded / 12.92
        } else {
            ((encoded + 0.055) / 1.055).powf(2.4)
        }
    };
    Color::rgb(decode(rgb[0]), decode(rgb[1]), decode(rgb[2]))
}

/// Rasterises `picture` at `width` by `height` pixels over `background`,
/// on the CPU, exactly as an application's frame rasterises it before
/// handing it to the renderer as an image.
pub fn render_svg(
    picture: &SvgPicture,
    width: u32,
    height: u32,
    background: SvgBackground,
) -> Image {
    let (width, height) = (width.clamp(1, 4096), height.clamp(1, 4096));
    let mut pixels = picture.rasterise(width, height);
    for (index, pixel) in pixels.chunks_exact_mut(4).enumerate() {
        let alpha = u32::from(pixel[3]);
        let behind = match background {
            SvgBackground::Transparent => {
                // The rasterisation is premultiplied; a PNG is not.
                if alpha > 0 {
                    for channel in &mut pixel[..3] {
                        *channel = ((u32::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
                    }
                }
                continue;
            }
            SvgBackground::Color(color) => color,
            SvgBackground::Checker => {
                let (x, y) = (index as u32 % width, index as u32 / width);
                if (x / 8 + y / 8) % 2 == 0 {
                    [0xC8; 3]
                } else {
                    [0x90; 3]
                }
            }
        };
        // Source over, on encoded values, as the renderer composites.
        for (channel, behind) in pixel[..3].iter_mut().zip(behind) {
            let under = (u32::from(behind) * (255 - alpha) + 127) / 255;
            *channel = (u32::from(*channel) + under).min(255) as u8;
        }
        pixel[3] = 255;
    }
    Image::new(width, height, pixels)
}
