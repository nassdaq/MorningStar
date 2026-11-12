use morningstar_core::{AtlasView, Color, DisplayList, ImagesView, SurfaceSize};

use crate::GpuError;
use crate::quads::{AtlasUpload, QuadRenderer, RenderStats};
use crate::renderer::{Gpu, encode_frame, new_instance, request_gpu};

/// Without sRGB encoding on write, like a surface's target: readback bytes
/// are the composited values themselves.
const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Renders one frame into an offscreen texture and returns its pixels as
/// tightly packed RGBA8, row by row from the top.
///
/// `atlas` is the glyph atlas the list's glyphs refer to; pass
/// [`AtlasView::EMPTY`] for a list without text.
///
/// The bytes are what a screen would show: sRGB-encoded colour, composited
/// in that encoding (ADR 0023), premultiplied by alpha where the clear
/// colour left the target translucent. Over an opaque clear colour they
/// are the bytes of an image file.
///
/// This runs the same frame-encoding path as on-screen rendering, without a
/// window, so it can back tests and benchmarks on any machine with a GPU.
/// It creates its own device and is not meant for per-frame use; to render
/// several frames, keep an [`OffscreenRenderer`].
pub fn render_offscreen(
    size: SurfaceSize,
    clear: Color,
    list: &DisplayList,
    atlas: AtlasView<'_>,
) -> Result<Vec<u8>, GpuError> {
    render_offscreen_with_images(size, clear, list, atlas, ImagesView::EMPTY)
}

/// As [`render_offscreen`], for a list that draws images: `images` is the
/// set of registered images its image operations refer to (for example
/// `ImageStore::view()`).
pub fn render_offscreen_with_images(
    size: SurfaceSize,
    clear: Color,
    list: &DisplayList,
    atlas: AtlasView<'_>,
    images: ImagesView<'_>,
) -> Result<Vec<u8>, GpuError> {
    if size.is_empty() {
        return Ok(Vec::new());
    }
    OffscreenRenderer::new()?.render_with_images(size, clear, list, atlas, images)
}

/// A renderer without a window that keeps its device, pipeline and glyph
/// atlas texture from one frame to the next, as the on-screen [`Renderer`]
/// does. Each frame's pixels are read back.
///
/// It exists for tests and benchmarks of behaviour that spans frames, such
/// as incremental glyph-atlas uploads.
///
/// [`Renderer`]: crate::Renderer
pub struct OffscreenRenderer {
    gpu: Gpu,
    quads: QuadRenderer,
}

impl OffscreenRenderer {
    /// Creates a GPU device. Fails with [`GpuError::NoAdapter`] on a machine
    /// without a usable GPU.
    pub fn new() -> Result<Self, GpuError> {
        let gpu = pollster::block_on(request_gpu(&new_instance(), None))?;
        let quads = QuadRenderer::new(&gpu.device, TARGET_FORMAT);
        Ok(Self { gpu, quads })
    }

    /// What the last [`render`](Self::render) uploaded of the glyph atlas.
    pub fn last_atlas_upload(&self) -> AtlasUpload {
        self.quads.last_atlas_upload()
    }

    /// What the last frame rendered cost: draw calls, image uploads, image
    /// textures held.
    pub fn last_stats(&self) -> RenderStats {
        self.quads.stats()
    }

    /// Renders one frame and returns its pixels, in the format described at
    /// [`render_offscreen`]. An empty `size` yields no pixels.
    ///
    /// The frame has no images: image operations in `list` draw nothing, and
    /// image textures kept from earlier frames are freed. Use
    /// [`render_with_images`](Self::render_with_images) for a list that
    /// draws images.
    pub fn render(
        &mut self,
        size: SurfaceSize,
        clear: Color,
        list: &DisplayList,
        atlas: AtlasView<'_>,
    ) -> Result<Vec<u8>, GpuError> {
        self.render_with_images(size, clear, list, atlas, ImagesView::EMPTY)
    }

    /// As [`render`](Self::render), with the registered images that the
    /// list's image operations refer to. Textures are kept from frame to
    /// frame for as long as their image stays in `images`.
    pub fn render_with_images(
        &mut self,
        size: SurfaceSize,
        clear: Color,
        list: &DisplayList,
        atlas: AtlasView<'_>,
        images: ImagesView<'_>,
    ) -> Result<Vec<u8>, GpuError> {
        if size.is_empty() {
            return Ok(Vec::new());
        }
        let (gpu, quads) = (&self.gpu, &mut self.quads);
        let extent = wgpu::Extent3d {
            width: size.width,
            height: size.height,
            depth_or_array_layers: 1,
        };
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen target"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TARGET_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Rows in a texture-to-buffer copy must be padded to a fixed alignment.
        const BYTES_PER_PIXEL: u32 = 4;
        let unpadded_row = size.width * BYTES_PER_PIXEL;
        let padded_row = unpadded_row.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);

        let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("offscreen readback"),
            size: u64::from(padded_row) * u64::from(size.height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut copy = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("readback"),
            });
        copy.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row),
                    rows_per_image: Some(size.height),
                },
            },
            extent,
        );

        quads.prepare(&gpu.device, &gpu.queue, size, list, atlas, images);
        gpu.queue.submit([
            encode_frame(&gpu.device, &view, clear, quads),
            copy.finish(),
        ]);

        let (sender, receiver) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
        gpu.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| GpuError::Readback(e.to_string()))?;
        receiver
            .recv()
            .map_err(|e| GpuError::Readback(e.to_string()))?
            .map_err(|e| GpuError::Readback(e.to_string()))?;

        let mapped = buffer
            .slice(..)
            .get_mapped_range()
            .map_err(|e| GpuError::Readback(e.to_string()))?;
        let mut pixels = Vec::with_capacity((unpadded_row * size.height) as usize);
        for row in mapped.chunks_exact(padded_row as usize) {
            pixels.extend_from_slice(&row[..unpadded_row as usize]);
        }
        Ok(pixels)
    }
}
