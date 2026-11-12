//! Draws a display list as instanced quads: rectangles (solid, bordered,
//! gradient-filled), shadows, glyphs and images, each clipped per instance.
//!
//! Transforms (ADR 0017) are carried per instance too: the vertex stage moves
//! the quad's corners through the instance's matrix while the fragment stage
//! keeps working in the quad's own untransformed space.
//!
//! Everything is one instance stream in paint order. It is drawn with one
//! draw call unless the list uses more than one image: each image is its own
//! texture, so the stream is cut into consecutive runs wherever the image
//! being sampled changes (ADR 0014).

use morningstar_core::{
    AtlasView, Color, DisplayList, DrawOp, Image, ImageId, ImagesView, Rect, SurfaceSize, Transform,
};

use crate::images::ImageTextures;

/// `f32`s per instance. Nine `vec4`s, in the order of `Instance` in
/// `quad.wgsl`: rect, colour, second colour (or an image's sampling bounds),
/// params, atlas rect or gradient points or image source region, rectangular
/// clip, rounded clip, the transform's linear part, and its translation
/// with the edge scale and flags.
const INSTANCE_FLOATS: usize = 36;
/// Values of the instance's flags. Must match `quad.wgsl`.
const FLAG_TURNED: f32 = 1.0;
const FLAG_LOCAL_CLIP: f32 = 2.0;
/// Bytes per instance.
const INSTANCE_SIZE: u64 = (INSTANCE_FLOATS * size_of::<f32>()) as u64;
/// Values of the instance's `kind` parameter. Must match `quad.wgsl`.
const KIND_RECT: f32 = 0.0;
const KIND_GLYPH: f32 = 1.0;
const KIND_GRADIENT: f32 = 2.0;
const KIND_SHADOW: f32 = 3.0;
const KIND_IMAGE: f32 = 4.0;
/// How far, in standard deviations, a shadow is drawn beyond its edge. Must
/// match `quad.wgsl`.
const SHADOW_EXTENT: f32 = 3.0;
/// Below this standard deviation a shadow is drawn as a plain anti-aliased
/// rectangle: the blur would be narrower than the edge's own falloff.
const MIN_SHADOW_SIGMA: f32 = 0.125;
/// Bytes in the uniform block: viewport size and atlas size.
const UNIFORM_SIZE: u64 = 16;
/// Smallest instance buffer, in rectangles. Avoids regrowing through tiny
/// sizes during the first frames.
const MIN_CAPACITY: u64 = 64;

/// What rendering a frame did with the glyph atlas.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AtlasUpload {
    /// Nothing was uploaded: the atlas was unchanged, or the frame drew
    /// nothing.
    #[default]
    None,
    /// Only rows `start..end` were uploaded.
    Rows { start: u32, end: u32 },
    /// The whole atlas was uploaded.
    Full,
}

/// What rendering a frame cost, for benchmarks, diagnostics and tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderStats {
    /// Instances drawn: operations left after clipping.
    pub instances: u32,
    /// Draw calls issued. One, unless the frame switched between images
    /// (see ADR 0014); zero for a frame that drew nothing.
    pub draw_calls: u32,
    /// Images uploaded to the GPU during the frame: those drawn for the
    /// first time.
    pub image_uploads: u32,
    /// Image textures alive after the frame.
    pub image_textures: u32,
    /// Bytes of texel data in those textures, mipmaps included.
    pub image_texture_bytes: u64,
}

/// A rectangle as its minimum and maximum corners: `[x0, y0, x1, y1]`.
type Bounds = [f32; 4];

/// Stands in for "no clip". Finite, so arithmetic on it in the shader cannot
/// produce NaN, and far beyond any surface.
const FAR: f32 = 1.0e9;
const UNBOUNDED: Bounds = [-FAR, -FAR, FAR, FAR];

fn bounds_of(rect: Rect) -> Bounds {
    if rect.is_empty() {
        // Includes NaN. Any empty bounds will do: they clip everything.
        return [0.0; 4];
    }
    [rect.x, rect.y, rect.x + rect.width, rect.y + rect.height]
}

fn intersect(a: Bounds, b: Bounds) -> Bounds {
    [
        a[0].max(b[0]),
        a[1].max(b[1]),
        a[2].min(b[2]),
        a[3].min(b[3]),
    ]
}

fn is_empty(bounds: Bounds) -> bool {
    !(bounds[2] > bounds[0] && bounds[3] > bounds[1])
}

fn rect_of(bounds: Bounds) -> Rect {
    Rect::new(
        bounds[0],
        bounds[1],
        bounds[2] - bounds[0],
        bounds[3] - bounds[1],
    )
}

/// The transform in effect at some point in a display list: the composition
/// of the open `PushTransform`s, with what the instances need of it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Xform {
    /// From the space operations are given in to the screen.
    matrix: Transform,
    /// Screen pixels per local pixel, as one number: the square root of the
    /// area scale. Exactly 1 for rotations, slants and moves. The shader
    /// multiplies edge distances by it so that an anti-aliased edge stays
    /// one screen pixel wide.
    edge_scale: f32,
    /// True unless the transform only moves things. Only then do glyph
    /// quads need an anti-aliased edge and clips their own space.
    turned: bool,
    /// The transform cannot be drawn through: nothing is.
    hidden: bool,
}

impl Xform {
    const NONE: Self = Self {
        matrix: Transform::IDENTITY,
        edge_scale: 1.0,
        turned: false,
        hidden: false,
    };

    /// This transform with `inner` applied before it.
    fn pushed(self, inner: Transform) -> Self {
        let matrix = inner.then(self.matrix);
        let edge_scale = matrix.determinant().abs().sqrt();
        // Written so that NaN counts as hidden.
        let drawable = matrix.is_finite() && edge_scale > 0.0 && edge_scale.is_finite();
        if self.hidden || !drawable {
            return Self {
                hidden: true,
                ..self
            };
        }
        let turned = !matrix.is_translation();
        Self {
            matrix,
            // Exactly 1 where it is 1 in principle, so that such instances
            // shade exactly as untransformed ones do.
            edge_scale: if turned { edge_scale } else { 1.0 },
            turned,
            hidden: false,
        }
    }

    /// The screen bounds of local `bounds`.
    fn screen_bounds(&self, bounds: Bounds) -> Bounds {
        if self.matrix.is_identity() || is_empty(bounds) {
            return bounds;
        }
        bounds_of(self.matrix.bounding_rect(rect_of(bounds)))
    }
}

/// The clip in effect at some point in a display list: a rectangle on the
/// screen intersected with at most one further shape, which is either a
/// rounded rectangle on the screen or a (rounded or square) rectangle in the
/// space of the transform in effect.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Clip {
    scissor: Bounds,
    rounded: Bounds,
    /// Corner radius of `rounded`.
    radius: f32,
    /// `rounded` is in the space of the current transform, not on the
    /// screen. It is then in use even with a zero radius.
    local: bool,
}

impl Clip {
    const NONE: Self = Self {
        scissor: UNBOUNDED,
        rounded: UNBOUNDED,
        radius: 0.0,
        local: false,
    };

    /// True if the second shape is in use.
    fn has_shape(&self) -> bool {
        self.radius > 0.0 || self.local
    }

    /// The screen rectangle with the second shape reduced to its bounding
    /// rectangle on screen: what remains of it once its slot is needed for
    /// another. `xform` is the transform a local shape is in.
    fn flattened(&self, xform: &Xform) -> Bounds {
        if !self.has_shape() {
            self.scissor
        } else if self.local {
            intersect(self.scissor, xform.screen_bounds(self.rounded))
        } else {
            intersect(self.scissor, self.rounded)
        }
    }

    /// This clip further restricted to `rect`, given under `xform`.
    fn pushed(self, rect: Rect, corner_radius: f32, xform: &Xform) -> Self {
        let radius = corner_radius.min(rect.width.min(rect.height) / 2.0);
        if xform.turned {
            let bounds = bounds_of(rect);
            // Written so that a NaN radius counts as square.
            let rounded = radius > 0.0;
            if self.local && self.radius == 0.0 && !rounded {
                // Square inside square, in the same space: exact.
                return Self {
                    rounded: intersect(self.rounded, bounds),
                    ..self
                };
            }
            return Self {
                scissor: self.flattened(xform),
                rounded: bounds,
                radius: if rounded { radius } else { 0.0 },
                local: true,
            };
        }

        // Only moved, if at all: the clip is a rectangle on the screen.
        let bounds = xform.screen_bounds(bounds_of(rect));
        // Written so that a NaN radius counts as square.
        if radius > 0.0 && !is_empty(bounds) {
            // Only one rounded clip is carried. An enclosing one gives up
            // its corners and continues as its bounding rectangle.
            Self {
                scissor: self.flattened(xform),
                rounded: bounds,
                radius,
                local: false,
            }
        } else {
            Self {
                scissor: intersect(self.scissor, bounds),
                ..self
            }
        }
    }

    /// This clip as it continues inside a further transform `inner`, pushed
    /// under `xform`. A shape in `xform`'s space cannot follow into the new
    /// space unless that only moves; otherwise it continues as its bounding
    /// rectangle on screen.
    fn entering(self, inner: &Transform, xform: &Xform) -> Self {
        if !self.local {
            return self;
        }
        if inner.is_translation() && inner.is_finite() {
            let [x0, y0, x1, y1] = self.rounded;
            return Self {
                rounded: [x0 - inner.tx, y0 - inner.ty, x1 - inner.tx, y1 - inner.ty],
                ..self
            };
        }
        Self {
            scissor: self.flattened(xform),
            ..Self::NONE
        }
    }

    /// False if nothing inside `bounds`, given under `xform`, can be
    /// visible through this clip.
    fn may_show(&self, bounds: Bounds, xform: &Xform) -> bool {
        if xform.hidden {
            return false;
        }
        let on_screen = intersect(self.scissor, xform.screen_bounds(bounds));
        if self.local {
            !is_empty(on_screen) && !is_empty(intersect(self.rounded, bounds))
        } else {
            !is_empty(intersect(on_screen, self.rounded))
        }
    }
}

/// One instance's fields, apart from the clip. Colours are premultiplied.
struct Quad {
    rect: Rect,
    color: [f32; 4],
    /// Second colour, or the bounds an image's sampling is confined to.
    color2: [f32; 4],
    corner_radius: f32,
    kind: f32,
    /// Border width, a shadow's sigma, or an image's mip level.
    extent: f32,
    /// Atlas region, gradient start and end points, or an image's source
    /// region in texture coordinates.
    extra: [f32; 4],
}

impl Quad {
    fn solid(rect: Rect, corner_radius: f32, color: Color) -> Self {
        Self {
            rect,
            color: premultiplied(color),
            color2: [0.0; 4],
            corner_radius,
            kind: KIND_RECT,
            extent: 0.0,
            extra: [0.0; 4],
        }
    }

    /// The rectangle, in the quad's own space, outside which it paints
    /// nothing when drawn through `xform`.
    fn painted_bounds(&self, xform: &Xform) -> Bounds {
        // Half a screen pixel of anti-aliased edge, in local pixels.
        let edge = 0.5 / xform.edge_scale;
        let margin = if self.kind == KIND_GLYPH {
            // A glyph has an edge of its own only when turned.
            if xform.turned { edge } else { 0.0 }
        } else if self.kind == KIND_SHADOW {
            edge + SHADOW_EXTENT * self.extent
        } else {
            edge
        };
        [
            self.rect.x - margin,
            self.rect.y - margin,
            self.rect.x + self.rect.width + margin,
            self.rect.y + self.rect.height + margin,
        ]
    }
}

/// An image operation as a quad, given the size in texels of the texture
/// that holds the image (which may be smaller than the image's own size).
fn image_quad(
    image: Image,
    rect: Rect,
    source: Rect,
    corner_radius: f32,
    tint: Color,
    (texture_width, texture_height): (u32, u32),
    edge_scale: f32,
) -> Quad {
    let (width, height) = (image.width() as f32, image.height() as f32);
    // The source region in texture coordinates.
    let (u, v) = (source.x / width, source.y / height);
    let (du, dv) = (source.width / width, source.height / height);

    // Texels per pixel along each axis. Above one the image is being
    // reduced, and sampling a single level would skip texels and shimmer;
    // the mip level whose texels are about a pixel in size is used instead.
    // For an axis-aligned quad this is what the GPU would derive from
    // screen-space derivatives, without needing them.
    let (texels_x, texels_y) = (du * texture_width as f32, dv * texture_height as f32);
    // A transform that enlarges or shrinks changes the ratio with it.
    let reduction = (texels_x / rect.width).max(texels_y / rect.height) / edge_scale;
    // Written so that NaN counts as no reduction.
    let level = if reduction > 1.0 {
        reduction.log2()
    } else {
        0.0
    };

    // Filtering reads half a texel (of the level in use) either side of the
    // sampling point. Keeping the point that far inside the region stops a
    // sprite's edge from picking up its neighbour in the sheet. Sampling at
    // texel centres, as for an unscaled image, is unaffected.
    let half_texel = 0.5 * level.exp2();
    let inset_u = (half_texel / texture_width as f32).min(du / 2.0);
    let inset_v = (half_texel / texture_height as f32).min(dv / 2.0);

    Quad {
        color2: [u + inset_u, v + inset_v, u + du - inset_u, v + dv - inset_v],
        kind: KIND_IMAGE,
        extent: level,
        extra: [u, v, du, dv],
        ..Quad::solid(rect, corner_radius, tint)
    }
}

/// A colour as the pipeline composites it: sRGB-encoded, then multiplied
/// by alpha, as the blend state expects. Everything is blended, interpolated
/// and filtered on these encoded values and written to a target that stores
/// them unchanged (ADR 0023).
fn premultiplied(color: Color) -> [f32; 4] {
    let [r, g, b, a] = color.srgb();
    [r * a, g * a, b * a, a]
}

/// Translates one drawing operation. `None` for clip operations, images
/// (which need their texture; see `prepare`), operations this renderer does
/// not know, and ones that turn out to paint nothing.
fn quad_for(op: &DrawOp) -> Option<Quad> {
    Some(match *op {
        DrawOp::Rect {
            rect,
            color,
            corner_radius,
        } => Quad::solid(rect, corner_radius, color),
        DrawOp::Glyph {
            rect,
            atlas: region,
            color,
        } => Quad {
            kind: KIND_GLYPH,
            extra: [region.x, region.y, region.width, region.height],
            ..Quad::solid(rect, 0.0, color)
        },
        DrawOp::BorderedRect {
            rect,
            corner_radius,
            fill,
            border,
        } => Quad {
            color2: premultiplied(border.color),
            extent: border.width.max(0.0),
            ..Quad::solid(rect, corner_radius, fill)
        },
        DrawOp::DashedRect {
            rect,
            corner_radius,
            fill,
            border,
            dash,
        } => Quad {
            color2: premultiplied(border.color),
            extent: border.width.max(0.0),
            // A rectangle with a positive dash length in `extra` is dashed.
            extra: [dash.length, dash.length + dash.gap, 0.0, 0.0],
            ..Quad::solid(rect, corner_radius, fill)
        },
        DrawOp::GradientRect {
            rect,
            corner_radius,
            gradient,
        } => {
            let (dx, dy) = (
                gradient.end.x - gradient.start.x,
                gradient.end.y - gradient.start.y,
            );
            let length_squared = dx * dx + dy * dy;
            if !(length_squared.is_finite() && length_squared > 0.0) {
                // No direction. `DisplayList` does the same; this covers
                // lists built by other means.
                return Some(Quad::solid(rect, corner_radius, gradient.start_color));
            }
            Quad {
                color2: premultiplied(gradient.end_color),
                kind: KIND_GRADIENT,
                extra: [
                    gradient.start.x,
                    gradient.start.y,
                    gradient.end.x,
                    gradient.end.y,
                ],
                ..Quad::solid(rect, corner_radius, gradient.start_color)
            }
        }
        DrawOp::Shadow {
            rect,
            corner_radius,
            shadow,
        } => {
            let spread = shadow.spread;
            let silhouette = Rect::new(
                rect.x + shadow.offset_x - spread,
                rect.y + shadow.offset_y - spread,
                rect.width + 2.0 * spread,
                rect.height + 2.0 * spread,
            );
            if silhouette.is_empty() {
                return None;
            }
            // As in CSS: spread rounds the corners further, except that
            // square corners stay square.
            let radius = if corner_radius > 0.0 {
                (corner_radius + spread).max(0.0)
            } else {
                0.0
            };
            // A blur radius is two standard deviations.
            let sigma = shadow.blur / 2.0;
            let solid = Quad::solid(silhouette, radius, shadow.color);
            // Written so that NaN counts as no blur.
            if sigma >= MIN_SHADOW_SIGMA {
                Quad {
                    kind: KIND_SHADOW,
                    extent: sigma,
                    ..solid
                }
            } else {
                solid
            }
        }
        // Clips are handled by the caller. `DrawOp` is non-exhaustive:
        // operations this renderer does not know yet are skipped rather than
        // failing the frame.
        _ => return None,
    })
}

/// A stretch of consecutive instances drawn by one draw call.
struct Run {
    /// One past the last instance of the run. It starts where the previous
    /// run ended.
    end: u32,
    /// The image its image instances sample. `None` if it has none.
    image: Option<wgpu::BindGroup>,
}

pub(crate) struct QuadRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    atlas: wgpu::Texture,
    /// Generation of the atlas contents currently in `atlas`.
    atlas_generation: Option<u64>,
    uniform: wgpu::Buffer,
    instances: wgpu::Buffer,
    /// Capacity of `instances`, in rectangles.
    capacity: u64,
    /// Instance data for the current frame. Kept to reuse its allocation.
    scratch: Vec<u8>,
    /// Instances uploaded by the last `prepare`.
    count: u32,
    /// Clips open at the current point of the list being prepared. Kept to
    /// reuse its allocation.
    clips: Vec<Clip>,
    /// Transforms open at the current point, each with the clip to return
    /// to when it ends. Kept to reuse its allocation.
    transforms: Vec<(Xform, Clip)>,
    /// Image textures, by image id.
    images: ImageTextures,
    /// The draw calls of the current frame. Kept to reuse its allocation.
    runs: Vec<Run>,
    last_atlas_upload: AtlasUpload,
    format: wgpu::TextureFormat,
}

impl QuadRenderer {
    /// Builds the pipeline for render targets of `format`.
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("quad shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("quad.wgsl").into()),
        });

        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: INSTANCE_SIZE,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![
                0 => Float32x4,
                1 => Float32x4,
                2 => Float32x4,
                3 => Float32x4,
                4 => Float32x4,
                5 => Float32x4,
                6 => Float32x4,
                7 => Float32x4,
                8 => Float32x4,
            ],
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("quad pipeline"),
            // Derived from the shader. Group 0: uniforms, atlas texture,
            // sampler. Group 1: image texture, sampler.
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(instance_layout)],
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
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("quad viewport"),
            size: UNIFORM_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Glyphs are drawn one texel per pixel, where linear filtering
        // equals nearest; linear keeps them smooth if they are ever scaled.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("glyph atlas"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let atlas = create_atlas_texture(device, 1, 1);
        let bind_group = create_bind_group(device, &pipeline, &uniform, &atlas, &sampler);
        let images = ImageTextures::new(device, pipeline.get_bind_group_layout(1));

        Self {
            pipeline,
            bind_group,
            sampler,
            atlas,
            atlas_generation: None,
            uniform,
            instances: create_instance_buffer(device, MIN_CAPACITY),
            capacity: MIN_CAPACITY,
            scratch: Vec::new(),
            count: 0,
            clips: Vec::new(),
            transforms: Vec::new(),
            images,
            runs: Vec::new(),
            last_atlas_upload: AtlasUpload::None,
            format,
        }
    }

    /// The render-target format this pipeline was built for.
    pub(crate) fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// What the last [`prepare`](Self::prepare) uploaded of the glyph atlas.
    pub(crate) fn last_atlas_upload(&self) -> AtlasUpload {
        self.last_atlas_upload
    }

    /// What the last [`prepare`](Self::prepare) amounted to.
    pub(crate) fn stats(&self) -> RenderStats {
        RenderStats {
            instances: self.count,
            draw_calls: self.runs.len() as u32,
            image_uploads: self.images.uploads(),
            image_textures: self.images.count(),
            image_texture_bytes: self.images.bytes(),
        }
    }

    /// Uploads `list` for drawing into a target of `size`. Call once per
    /// frame, before [`draw`](Self::draw).
    ///
    /// `images` is the set of registered images. Those the list draws are
    /// uploaded on first use; textures of images no longer in the set are
    /// freed, whether or not the list draws anything.
    pub(crate) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: SurfaceSize,
        list: &DisplayList,
        atlas: AtlasView<'_>,
        images: ImagesView<'_>,
    ) {
        self.scratch.clear();
        self.clips.clear();
        self.transforms.clear();
        self.runs.clear();
        self.last_atlas_upload = AtlasUpload::None;
        self.images.begin_frame(&images);
        let mut clip = Clip::NONE;
        let mut xform = Xform::NONE;
        // The image the instances since the last run boundary sample, once
        // one of them is an image. Rectangles and glyphs sample none, so
        // they join whatever run is open.
        let mut run_image: Option<ImageId> = None;
        for op in list.ops() {
            match *op {
                DrawOp::PushClip {
                    rect,
                    corner_radius,
                } => {
                    self.clips.push(clip);
                    clip = clip.pushed(rect, corner_radius, &xform);
                }
                // A pop with nothing to match is ignored.
                DrawOp::PopClip => clip = self.clips.pop().unwrap_or(clip),
                DrawOp::PushTransform { transform } => {
                    self.transforms.push((xform, clip));
                    clip = clip.entering(&transform, &xform);
                    xform = xform.pushed(transform);
                }
                DrawOp::PopTransform => {
                    (xform, clip) = self.transforms.pop().unwrap_or((xform, clip));
                }
                DrawOp::Image {
                    image,
                    rect,
                    source,
                    corner_radius,
                    tint,
                } => {
                    // Checked first, so an image scrolled out of view is
                    // not uploaded for nothing.
                    if !clip.may_show(Quad::solid(rect, 0.0, tint).painted_bounds(&xform), &xform) {
                        continue;
                    }
                    // A released image has no texture and draws nothing.
                    let Some(texture) = self.images.get(device, queue, &images, image.id()) else {
                        continue;
                    };
                    let quad = image_quad(
                        image,
                        rect,
                        source,
                        corner_radius,
                        tint,
                        (texture.width, texture.height),
                        xform.edge_scale,
                    );
                    if run_image.is_some_and(|current| current != image.id()) {
                        // A different texture: what came before is drawn
                        // first, by its own draw call, so paint order holds.
                        self.end_run(device, queue, &images, run_image);
                    }
                    run_image = Some(image.id());
                    self.push_instance(&quad, &clip, &xform);
                }
                _ => {
                    if let Some(quad) = quad_for(op)
                        // Clipped-out operations cost nothing on the GPU.
                        && clip.may_show(quad.painted_bounds(&xform), &xform)
                    {
                        self.push_instance(&quad, &clip, &xform);
                    }
                }
            }
        }

        let needed = self.scratch.len() as u64 / INSTANCE_SIZE;
        // One draw call's instance count is a `u32`.
        self.count = u32::try_from(needed).unwrap_or(u32::MAX);
        if self.count == 0 {
            return;
        }
        self.end_run(device, queue, &images, run_image);

        if needed > self.capacity {
            // Grow geometrically so a slowly growing UI does not reallocate
            // every frame. The old buffer is freed when dropped.
            self.capacity = needed.next_power_of_two();
            self.instances = create_instance_buffer(device, self.capacity);
        }

        self.upload_atlas(device, queue, atlas);

        let mut globals = [0u8; UNIFORM_SIZE as usize];
        for (slot, value) in [
            size.width as f32,
            size.height as f32,
            atlas.width as f32,
            atlas.height as f32,
        ]
        .into_iter()
        .enumerate()
        {
            globals[slot * 4..slot * 4 + 4].copy_from_slice(&value.to_ne_bytes());
        }
        queue.write_buffer(&self.uniform, 0, &globals);
        queue.write_buffer(
            &self.instances,
            0,
            &self.scratch[..(u64::from(self.count) * INSTANCE_SIZE) as usize],
        );
    }

    /// Closes the run of instances pushed since the previous one ended.
    fn end_run(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        images: &ImagesView<'_>,
        image: Option<ImageId>,
    ) {
        let end = u32::try_from(self.scratch.len() as u64 / INSTANCE_SIZE).unwrap_or(u32::MAX);
        // Already uploaded when its first instance was pushed.
        let image = image
            .and_then(|id| self.images.get(device, queue, images, id))
            .map(|texture| texture.bind_group.clone());
        self.runs.push(Run { end, image });
    }

    fn push_instance(&mut self, quad: &Quad, clip: &Clip, xform: &Xform) {
        let matrix = xform.matrix;
        let mut flags = 0.0;
        if xform.turned {
            flags += FLAG_TURNED;
        }
        if clip.local {
            flags += FLAG_LOCAL_CLIP;
        }
        let Quad {
            rect,
            color,
            color2,
            extra,
            ..
        } = *quad;
        let instance: [f32; INSTANCE_FLOATS] = [
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            color[0],
            color[1],
            color[2],
            color[3],
            color2[0],
            color2[1],
            color2[2],
            color2[3],
            quad.corner_radius,
            quad.kind,
            quad.extent,
            clip.radius,
            extra[0],
            extra[1],
            extra[2],
            extra[3],
            clip.scissor[0],
            clip.scissor[1],
            clip.scissor[2],
            clip.scissor[3],
            clip.rounded[0],
            clip.rounded[1],
            clip.rounded[2],
            clip.rounded[3],
            matrix.a,
            matrix.b,
            matrix.c,
            matrix.d,
            matrix.tx,
            matrix.ty,
            xform.edge_scale,
            flags,
        ];
        for value in instance {
            self.scratch.extend_from_slice(&value.to_ne_bytes());
        }
    }

    /// Copies the glyph atlas to the GPU if it changed since the last frame.
    ///
    /// If the view says which rows changed since the generation already on
    /// the GPU, only those rows are uploaded; otherwise the whole image is.
    fn upload_atlas(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, atlas: AtlasView<'_>) {
        if self.atlas_generation == Some(atlas.generation) {
            return;
        }
        let expected = atlas.width as usize * atlas.height as usize;
        if atlas.width == 0 || atlas.height == 0 || atlas.pixels.len() != expected {
            return; // Malformed view; keep drawing with the previous atlas.
        }

        let same_size = self.atlas.width() == atlas.width && self.atlas.height() == atlas.height;
        // The dirty record is relative to generation `since`. It describes
        // everything this texture lacks only if the texture holds that
        // generation or a later one of the same atlas.
        let rows = atlas.dirty.filter(|dirty| {
            same_size
                && self
                    .atlas_generation
                    .is_some_and(|held| (dirty.since..atlas.generation).contains(&held))
                && dirty.start <= dirty.end
                && dirty.end <= atlas.height
        });

        let (start, end) = match rows {
            Some(dirty) => (dirty.start, dirty.end),
            None => (0, atlas.height),
        };
        if !same_size {
            self.atlas = create_atlas_texture(device, atlas.width, atlas.height);
            self.bind_group = create_bind_group(
                device,
                &self.pipeline,
                &self.uniform,
                &self.atlas,
                &self.sampler,
            );
        }
        if end > start {
            let width = atlas.width as usize;
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.atlas,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: start,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &atlas.pixels[start as usize * width..end as usize * width],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(atlas.width),
                    rows_per_image: Some(end - start),
                },
                wgpu::Extent3d {
                    width: atlas.width,
                    height: end - start,
                    depth_or_array_layers: 1,
                },
            );
        }
        self.last_atlas_upload = match rows {
            Some(_) if end > start => AtlasUpload::Rows { start, end },
            Some(_) => AtlasUpload::None,
            None => AtlasUpload::Full,
        };
        self.atlas_generation = Some(atlas.generation);
    }

    /// Draws what the last [`prepare`](Self::prepare) uploaded.
    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        let mut start = 0;
        for run in &self.runs {
            pass.set_bind_group(
                1,
                run.image.as_ref().unwrap_or(self.images.placeholder()),
                &[],
            );
            // Each run binds its own slice of the instance buffer, rather
            // than drawing a range of instances from one binding: a non-zero
            // first instance is the less portable of the two.
            pass.set_vertex_buffer(
                0,
                self.instances
                    .slice(u64::from(start) * INSTANCE_SIZE..u64::from(run.end) * INSTANCE_SIZE),
            );
            pass.draw(0..4, 0..run.end - start);
            start = run.end;
        }
    }
}

fn create_instance_buffer(device: &wgpu::Device, capacity: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("quad instances"),
        size: capacity * INSTANCE_SIZE,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_atlas_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("glyph atlas"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        // One channel: glyph coverage.
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn create_bind_group(
    device: &wgpu::Device,
    pipeline: &wgpu::RenderPipeline,
    uniform: &wgpu::Buffer,
    atlas: &wgpu::Texture,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    let view = atlas.create_view(&wgpu::TextureViewDescriptor::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("quad globals"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_is_nine_vec4s() {
        assert_eq!(INSTANCE_SIZE, 144);
    }

    const TURN: Transform = Transform::matrix(0.0, 1.0, -1.0, 0.0, 0.0, 0.0);

    #[test]
    fn transforms_compose_innermost_first() {
        let outer = Transform::translate(10.0, 0.0);
        let xform = Xform::NONE.pushed(outer).pushed(TURN);
        // (1, 0) is turned to (0, 1), then moved right by 10.
        let point = xform.matrix.apply(morningstar_core::Point::new(1.0, 0.0));
        assert_eq!((point.x, point.y), (10.0, 1.0));
        assert!(xform.turned && !xform.hidden);
        assert_eq!(xform.edge_scale, 1.0);

        let moved = Xform::NONE.pushed(outer);
        assert!(!moved.turned);
        assert_eq!(
            moved.screen_bounds([0.0, 0.0, 4.0, 4.0]),
            [10.0, 0.0, 14.0, 4.0]
        );
        assert_eq!(
            Xform::NONE.pushed(Transform::scale(2.0, 2.0)).edge_scale,
            2.0
        );
    }

    #[test]
    fn undrawable_transforms_hide_everything_until_popped() {
        for bad in [
            Transform::scale(0.0, 1.0),
            Transform::translate(f32::NAN, 0.0),
            Transform::scale(f32::INFINITY, 1.0),
        ] {
            let xform = Xform::NONE.pushed(bad);
            assert!(xform.hidden, "{bad:?}");
            assert!(!Clip::NONE.may_show(UNBOUNDED, &xform));
            assert!(
                xform.pushed(Transform::IDENTITY).hidden,
                "stays hidden inside"
            );
        }
    }

    #[test]
    fn a_clip_under_a_move_is_a_screen_clip() {
        let moved = Xform::NONE.pushed(Transform::translate(100.0, 0.0));
        let clip = Clip::NONE.pushed(Rect::new(0.0, 0.0, 10.0, 10.0), 0.0, &moved);
        assert_eq!(clip.scissor, [100.0, 0.0, 110.0, 10.0]);
        assert!(!clip.local);
        // Local bounds 0..10 land inside it; 20..30 do not.
        assert!(clip.may_show([0.0, 0.0, 10.0, 10.0], &moved));
        assert!(!clip.may_show([20.0, 0.0, 30.0, 10.0], &moved));
    }

    #[test]
    fn a_clip_under_a_turn_is_kept_in_the_turned_space() {
        let turned = Xform::NONE.pushed(TURN);
        let outer = Clip::NONE.pushed(Rect::new(-50.0, 0.0, 100.0, 100.0), 0.0, &Xform::NONE);
        let clip = outer.pushed(Rect::new(0.0, 0.0, 20.0, 10.0), 0.0, &turned);
        assert!(clip.local);
        assert_eq!(clip.rounded, [0.0, 0.0, 20.0, 10.0]);
        assert_eq!(clip.scissor, outer.scissor, "the screen clip is untouched");
        assert!(clip.may_show([5.0, 5.0, 6.0, 6.0], &turned));
        assert!(
            !clip.may_show([25.0, 5.0, 30.0, 6.0], &turned),
            "outside locally"
        );

        // Square in square, same space: exact intersection.
        let both = clip.pushed(Rect::new(10.0, 0.0, 20.0, 10.0), 0.0, &turned);
        assert_eq!(both.rounded, [10.0, 0.0, 20.0, 10.0]);
        assert_eq!(both.radius, 0.0);

        // A rounded one takes the slot; the square one becomes its bounds
        // on screen: local 0..20 x 0..10 turns into -10..0 x 0..20.
        let rounded = clip.pushed(Rect::new(0.0, 0.0, 10.0, 10.0), 5.0, &turned);
        assert!(rounded.local);
        assert_eq!(rounded.radius, 5.0);
        assert_eq!(rounded.scissor, [-10.0, 0.0, 0.0, 20.0]);
    }

    #[test]
    fn a_turned_clip_follows_a_move_and_flattens_under_a_further_turn() {
        let turned = Xform::NONE.pushed(TURN);
        let clip = Clip::NONE.pushed(Rect::new(0.0, 0.0, 20.0, 10.0), 3.0, &turned);

        let shifted = clip.entering(&Transform::translate(5.0, 1.0), &turned);
        assert!(shifted.local);
        assert_eq!(shifted.rounded, [-5.0, -1.0, 15.0, 9.0]);

        let flattened = clip.entering(&TURN, &turned);
        assert!(!flattened.local && !flattened.has_shape());
        assert_eq!(flattened.scissor, [-10.0, 0.0, 0.0, 20.0]);

        // A rounded clip on the screen is unaffected by transforms inside.
        let screen = Clip::NONE.pushed(Rect::new(0.0, 0.0, 20.0, 20.0), 4.0, &Xform::NONE);
        assert_eq!(screen.entering(&TURN, &Xform::NONE), screen);
    }

    #[test]
    fn square_clips_intersect() {
        let clip = Clip::NONE
            .pushed(Rect::new(0.0, 0.0, 10.0, 10.0), 0.0, &Xform::NONE)
            .pushed(Rect::new(5.0, -5.0, 10.0, 10.0), 0.0, &Xform::NONE);
        assert_eq!(clip.scissor, [5.0, 0.0, 10.0, 5.0]);
        assert_eq!(clip.radius, 0.0);
        assert!(clip.may_show([9.0, 4.0, 20.0, 20.0], &Xform::NONE));
        assert!(
            !clip.may_show([10.0, 0.0, 20.0, 5.0], &Xform::NONE),
            "touching only"
        );
    }

    #[test]
    fn disjoint_or_empty_clips_show_nothing() {
        let anything = [-FAR, -FAR, FAR, FAR];
        let disjoint = Clip::NONE
            .pushed(Rect::new(0.0, 0.0, 4.0, 4.0), 0.0, &Xform::NONE)
            .pushed(Rect::new(8.0, 0.0, 4.0, 4.0), 0.0, &Xform::NONE);
        assert!(!disjoint.may_show(anything, &Xform::NONE));
        let empty = Clip::NONE.pushed(Rect::new(0.0, 0.0, 0.0, 4.0), 0.0, &Xform::NONE);
        assert!(!empty.may_show(anything, &Xform::NONE));
        let nan = Clip::NONE.pushed(Rect::new(0.0, 0.0, f32::NAN, 4.0), 3.0, &Xform::NONE);
        assert!(!nan.may_show(anything, &Xform::NONE));
    }

    #[test]
    fn innermost_rounded_clip_wins_and_the_outer_becomes_its_bounds() {
        let outer = Clip::NONE.pushed(Rect::new(0.0, 0.0, 100.0, 100.0), 20.0, &Xform::NONE);
        assert_eq!(
            outer.scissor, UNBOUNDED,
            "a rounded clip is not also a scissor"
        );
        assert_eq!(
            (outer.rounded, outer.radius),
            ([0.0, 0.0, 100.0, 100.0], 20.0)
        );

        let scrolled = outer.pushed(Rect::new(0.0, 30.0, 100.0, 40.0), 0.0, &Xform::NONE);
        assert_eq!(scrolled.scissor, [0.0, 30.0, 100.0, 70.0]);
        assert_eq!(
            scrolled.radius, 20.0,
            "square inside rounded keeps the corners"
        );

        let inner = scrolled.pushed(Rect::new(10.0, 10.0, 50.0, 50.0), 1000.0, &Xform::NONE);
        assert_eq!(inner.scissor, [0.0, 30.0, 100.0, 70.0]);
        assert_eq!(
            (inner.rounded, inner.radius),
            ([10.0, 10.0, 60.0, 60.0], 25.0)
        );
    }
}
