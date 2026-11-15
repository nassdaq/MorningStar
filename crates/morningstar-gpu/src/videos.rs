//! GPU textures for videos (ADR 0039): per video, the three planes of its
//! latest frame and a colour texture they are converted into, which the
//! quad pipeline then samples exactly as it samples an image. So a video
//! is clipped, rounded, tinted and turned by the code that does it for
//! everything else, and the conversion (BT.601 limited range, the frame's
//! turn and mirror) is one small pass per new frame of a video that is
//! actually drawn.

use std::collections::HashMap;

use morningstar_core::{ImagesView, VideoFrame, VideoPicture, VideoTrack};

use crate::images::ImageTextures;

/// What the planes are converted into: the encoded colours the quad
/// pipeline blends (ADR 0023). A decoder's output is gamma-encoded video,
/// taken as sRGB like every other picture.
const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const PLANE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

const SHADER: &str = r"
struct Params {
    // x: quarter turns clockwise to show the frame upright. y: 1 if mirrored.
    turn: vec4<f32>,
}

@group(0) @binding(0) var y_plane: texture_2d<f32>;
@group(0) @binding(1) var u_plane: texture_2d<f32>;
@group(0) @binding(2) var v_plane: texture_2d<f32>;
@group(0) @binding(3) var planes: sampler;
@group(0) @binding(4) var<uniform> params: Params;

struct Corner {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> Corner {
    let corner = vec2<f32>(f32(index & 1u), f32(index >> 1u));
    var out: Corner;
    out.position = vec4<f32>(corner.x * 2.0 - 1.0, 1.0 - corner.y * 2.0, 0.0, 1.0);
    out.uv = corner;
    return out;
}

@fragment
fn fs_main(in: Corner) -> @location(0) vec4<f32> {
    // Where in the stored planes this point of the shown picture is: the
    // mirror and the turn undone (VideoFrame::source_point).
    var shown = in.uv;
    if params.turn.y > 0.5 {
        shown.x = 1.0 - shown.x;
    }
    var at = shown;
    if params.turn.x > 2.5 {
        at = vec2<f32>(1.0 - shown.y, shown.x);
    } else if params.turn.x > 1.5 {
        at = vec2<f32>(1.0 - shown.x, 1.0 - shown.y);
    } else if params.turn.x > 0.5 {
        at = vec2<f32>(shown.y, 1.0 - shown.x);
    }
    let luma = 1.164 * (textureSampleLevel(y_plane, planes, at, 0.0).r - 16.0 / 255.0);
    let cb = textureSampleLevel(u_plane, planes, at, 0.0).r - 128.0 / 255.0;
    let cr = textureSampleLevel(v_plane, planes, at, 0.0).r - 128.0 / 255.0;
    let rgb = vec3<f32>(
        luma + 1.596 * cr,
        luma - 0.392 * cb - 0.813 * cr,
        luma + 2.017 * cb,
    );
    return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
";

/// One video's textures.
pub(crate) struct VideoTexture {
    /// Binds the converted picture as an image is bound, for group 1 of
    /// the quad pipeline.
    pub(crate) bind_group: wgpu::BindGroup,
    /// Size of the converted picture: the frame as shown.
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// Size of the planes the textures were made for.
    stored: (u32, u32),
    /// The frame that is in `target`.
    serial: u64,
    planes: [wgpu::Texture; 3],
    params: wgpu::Buffer,
    /// The three planes, their sampler and `params`, for the conversion.
    source: wgpu::BindGroup,
    target: wgpu::TextureView,
}

pub(crate) struct VideoTextures {
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    textures: HashMap<VideoTrack, VideoTexture>,
    max_dimension: u32,
    /// Frames converted since the last `begin_frame`.
    conversions: u32,
}

impl VideoTextures {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("video conversion"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("video conversion"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: TARGET_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        // Bilinear: colour is stored at half size and is interpolated up,
        // and a turned or scaled picture is filtered once more when drawn.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("video planes"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            pipeline,
            sampler,
            textures: HashMap::new(),
            max_dimension: device.limits().max_texture_dimension_2d,
            conversions: 0,
        }
    }

    /// Call once per frame, before any [`get`](Self::get): frees the
    /// textures of videos that have stopped.
    pub(crate) fn begin_frame(&mut self, images: &ImagesView<'_>) {
        self.conversions = 0;
        if !self.textures.is_empty() {
            self.textures
                .retain(|&track, _| images.video(track).is_some());
        }
    }

    /// The converted picture of video `track`, converting its latest frame
    /// first if that is not the one already there. `None` if the video has
    /// no frame, or one this device cannot hold: the caller draws nothing.
    pub(crate) fn get(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        images: &ImageTextures,
        view: &ImagesView<'_>,
        track: VideoTrack,
    ) -> Option<&VideoTexture> {
        let VideoPicture { frame, serial } = view.video(track)?;
        let stored = (frame.width(), frame.height());
        if stored.0.max(stored.1) > self.max_dimension {
            return None;
        }
        let (width, height) = frame.display_size();
        let fits = self.textures.get(&track).is_some_and(|texture| {
            texture.stored == stored && (texture.width, texture.height) == (width, height)
        });
        if !fits {
            let texture = self.create(device, images, frame);
            self.textures.insert(track, texture);
        }
        let texture = self.textures.get_mut(&track)?;
        if texture.serial != serial {
            convert(device, queue, &self.pipeline, texture, frame);
            texture.serial = serial;
            self.conversions += 1;
        }
        self.textures.get(&track)
    }

    /// The bind group of a video converted earlier in this frame.
    pub(crate) fn bind_group(&self, track: VideoTrack) -> Option<wgpu::BindGroup> {
        self.textures
            .get(&track)
            .map(|texture| texture.bind_group.clone())
    }

    pub(crate) fn count(&self) -> u32 {
        self.textures.len() as u32
    }

    pub(crate) fn conversions(&self) -> u32 {
        self.conversions
    }

    fn create(
        &self,
        device: &wgpu::Device,
        images: &ImageTextures,
        frame: &VideoFrame,
    ) -> VideoTexture {
        let texture = |label, (width, height): (u32, u32), format, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let plane_usage = wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST;
        let stored = (frame.width(), frame.height());
        let planes = [
            texture("video y", stored, PLANE_FORMAT, plane_usage),
            texture("video u", frame.chroma_size(), PLANE_FORMAT, plane_usage),
            texture("video v", frame.chroma_size(), PLANE_FORMAT, plane_usage),
        ];
        let (width, height) = frame.display_size();
        let target = texture(
            "video picture",
            (width, height),
            TARGET_FORMAT,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
        )
        .create_view(&wgpu::TextureViewDescriptor::default());
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("video turn"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let views = planes
            .each_ref()
            .map(|plane| plane.create_view(&wgpu::TextureViewDescriptor::default()));
        let source = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("video planes"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&views[0]),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&views[1]),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&views[2]),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: params.as_entire_binding(),
                },
            ],
        });
        VideoTexture {
            bind_group: images.bind_group(device, &target),
            width,
            height,
            stored,
            // No frame's: serials are never zero.
            serial: 0,
            planes,
            params,
            source,
            target,
        }
    }
}

/// Uploads `frame`'s planes and converts them into the texture's picture,
/// in a render pass of its own, submitted at once: before the frame that
/// draws it.
fn convert(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::RenderPipeline,
    texture: &VideoTexture,
    frame: &VideoFrame,
) {
    let (chroma_width, chroma_height) = frame.chroma_size();
    let planes = [
        (frame.y(), frame.width(), frame.height()),
        (frame.u(), chroma_width, chroma_height),
        (frame.v(), chroma_width, chroma_height),
    ];
    for (plane, (bytes, width, height)) in texture.planes.iter().zip(planes) {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: plane,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }
    let turn = [
        frame.rotation().quarter_turns() as f32,
        f32::from(u8::from(frame.mirrored())),
        0.0,
        0.0,
    ];
    let mut params = [0u8; 16];
    for (bytes, value) in params.chunks_exact_mut(4).zip(turn) {
        bytes.copy_from_slice(&value.to_le_bytes());
    }
    queue.write_buffer(&texture.params, 0, &params);

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("video conversion"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("video conversion"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &texture.target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &texture.source, &[]);
        pass.draw(0..4, 0..1);
    }
    queue.submit([encoder.finish()]);
}
