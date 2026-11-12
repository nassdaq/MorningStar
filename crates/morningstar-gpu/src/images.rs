//! GPU textures for registered images: one texture per image, created the
//! first time the image is drawn and dropped when it is released. See
//! ADR 0014.

use std::collections::HashMap;

use morningstar_core::{ImageId, ImagePixels, ImagesView};

/// Plain 8-bit, not sRGB: the pixels arrive sRGB-encoded and premultiplied
/// (see `ImagePixels`) and are sampled, filtered and blended as those
/// encoded values, like every other colour (ADR 0023).
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// One uploaded image.
pub(crate) struct ImageTexture {
    /// Binds the texture and the image sampler as group 1.
    pub(crate) bind_group: wgpu::BindGroup,
    /// Size of mip level 0, in texels.
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// Bytes of texel data in all mip levels.
    bytes: u64,
    /// Kept so the texture's lifetime is plainly this entry's.
    _texture: wgpu::Texture,
}

pub(crate) struct ImageTextures {
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// Bound while a draw call contains no image, because the pipeline needs
    /// something in group 1.
    placeholder: wgpu::BindGroup,
    textures: HashMap<ImageId, ImageTexture>,
    /// Generation of the image set `textures` was last pruned against.
    generation: Option<u64>,
    /// Longest side the device can hold.
    max_dimension: u32,
    /// Textures created since the last `begin_frame`.
    uploads: u32,
}

impl ImageTextures {
    /// `layout` is the pipeline's layout for group 1.
    pub(crate) fn new(device: &wgpu::Device, layout: wgpu::BindGroupLayout) -> Self {
        // Bilinear within a level and linear between levels. The level is
        // chosen per instance, so images drawn at their own size sample
        // level 0 only, where bilinear at texel centres is exact.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("image"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let max_dimension = device.limits().max_texture_dimension_2d;
        let transparent = ImagePixels {
            width: 1,
            height: 1,
            pixels: &[0; 4],
        };
        let placeholder = upload(device, None, &layout, &sampler, max_dimension, transparent)
            .expect("a one-texel texture is valid")
            .bind_group;
        Self {
            layout,
            sampler,
            placeholder,
            textures: HashMap::new(),
            generation: None,
            max_dimension,
            uploads: 0,
        }
    }

    pub(crate) fn placeholder(&self) -> &wgpu::BindGroup {
        &self.placeholder
    }

    /// Call once per frame, before any [`get`](Self::get): frees the
    /// textures of images that are no longer registered.
    pub(crate) fn begin_frame(&mut self, images: &ImagesView<'_>) {
        self.uploads = 0;
        if self.generation != Some(images.generation()) {
            // Dropping the entry drops the last references to the texture.
            self.textures.retain(|&id, _| images.contains(id));
            self.generation = Some(images.generation());
        }
    }

    /// The texture for image `id`, uploading it if this is the first time it
    /// is drawn. `None` if the image is not registered (it was released, or
    /// belongs to another store): the caller draws nothing.
    pub(crate) fn get(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        images: &ImagesView<'_>,
        id: ImageId,
    ) -> Option<&ImageTexture> {
        if !self.textures.contains_key(&id) {
            let texture = upload(
                device,
                Some(queue),
                &self.layout,
                &self.sampler,
                self.max_dimension,
                images.get(id)?,
            )?;
            self.textures.insert(id, texture);
            self.uploads += 1;
        }
        self.textures.get(&id)
    }

    pub(crate) fn count(&self) -> u32 {
        self.textures.len() as u32
    }

    pub(crate) fn bytes(&self) -> u64 {
        self.textures.values().map(|texture| texture.bytes).sum()
    }

    pub(crate) fn uploads(&self) -> u32 {
        self.uploads
    }
}

/// Number of mip levels down to one texel.
fn mip_levels(width: u32, height: u32) -> u32 {
    32 - width.max(height).leading_zeros()
}

/// Creates a texture with a full mip chain for `pixels`. `None` if the view
/// is malformed. Without a queue the texture is left zeroed (transparent).
fn upload(
    device: &wgpu::Device,
    queue: Option<&wgpu::Queue>,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    max_dimension: u32,
    pixels: ImagePixels<'_>,
) -> Option<ImageTexture> {
    let texels = (pixels.width as usize).checked_mul(pixels.height as usize)?;
    if texels == 0 || pixels.pixels.len() != texels.checked_mul(4)? {
        return None;
    }

    // Registration already limits images to what every target device can
    // hold. Should a device hold less, reduce further rather than fail.
    let reduced;
    let mut level = pixels;
    let longest = level.width.max(level.height);
    if longest > max_dimension {
        let scaled = |side: u32| {
            ((u64::from(side) * u64::from(max_dimension) / u64::from(longest)) as u32).max(1)
        };
        let (width, height) = (scaled(level.width), scaled(level.height));
        reduced = level.downscaled(width, height);
        level = ImagePixels {
            width,
            height,
            pixels: &reduced,
        };
    }

    let (width, height) = (level.width, level.height);
    let levels = mip_levels(width, height);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("image"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    let mut bytes = 0;
    if let Some(queue) = queue {
        // Each level is a box filter of the one before, averaged on the
        // CPU: the same result on every backend, with no
        // render-target support needed for the texture format.
        let mut smaller;
        for mip_level in 0..levels {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                level.pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(level.width * 4),
                    rows_per_image: Some(level.height),
                },
                wgpu::Extent3d {
                    width: level.width,
                    height: level.height,
                    depth_or_array_layers: 1,
                },
            );
            bytes += level.pixels.len() as u64;
            if mip_level + 1 < levels {
                let (next_width, next_height) =
                    ((level.width / 2).max(1), (level.height / 2).max(1));
                smaller = level.downscaled(next_width, next_height);
                level = ImagePixels {
                    width: next_width,
                    height: next_height,
                    pixels: &smaller,
                };
            }
        }
    }

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("image"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    });
    Some(ImageTexture {
        bind_group,
        width,
        height,
        bytes,
        _texture: texture,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mip_chain_ends_at_one_texel() {
        assert_eq!(mip_levels(1, 1), 1);
        assert_eq!(mip_levels(2, 1), 2);
        assert_eq!(mip_levels(3, 3), 2);
        assert_eq!(mip_levels(4, 1), 3);
        assert_eq!(mip_levels(1, 2048), 12);
        assert_eq!(mip_levels(2047, 5), 11);
    }
}
