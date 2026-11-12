#[cfg(any(target_os = "android", target_vendor = "apple"))]
use std::{ffi::c_void, ptr::NonNull};

use std::time::{Duration, Instant};

use morningstar_core::{AtlasView, Color, DisplayList, ImagesView, SurfaceSize};

use crate::GpuError;
use crate::quads::{AtlasUpload, QuadRenderer, RenderStats};

/// A native drawable handed over by a platform host.
///
/// Has no variants on targets that are neither Android nor Apple: there the
/// renderer can only be used offscreen.
#[derive(Clone, Copy, Debug)]
pub enum NativeSurface {
    /// An `ANativeWindow*`.
    #[cfg(target_os = "android")]
    AndroidNativeWindow(NonNull<c_void>),
    /// A `CAMetalLayer*`.
    #[cfg(target_vendor = "apple")]
    CoreAnimationLayer(NonNull<c_void>),
}

/// What happened when a frame was requested.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FrameOutcome {
    /// A frame was submitted and queued for presentation.
    Presented(FrameTimings),
    /// No frame was produced (no surface, occluded, or the swapchain had to
    /// be rebuilt). Not an error; try again on the next frame.
    Skipped,
}

/// Where the time of one presented frame went.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameTimings {
    /// Waiting for the next drawable. With vsync this includes pacing.
    pub acquire: Duration,
    /// Encoding and submitting GPU commands: the CPU cost Morningstar controls.
    pub encode: Duration,
    /// Handing the frame to the compositor.
    pub present: Duration,
}

/// Description of the GPU the renderer is running on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterInfo {
    pub name: String,
    /// Graphics API in use, e.g. `Vulkan` or `Metal`.
    pub backend: String,
    pub driver: String,
}

pub(crate) struct Gpu {
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) info: AdapterInfo,
    adapter: wgpu::Adapter,
}

struct SurfaceState {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    /// The format frames are rendered in: the surface's, without any sRGB
    /// encoding on write.
    target_format: wgpu::TextureFormat,
    /// False while the surface has a zero dimension and so is unconfigured.
    configured: bool,
}

/// Owns the GPU device and, while the platform provides one, a surface.
///
/// The device outlives surfaces: on Android the drawable is destroyed every
/// time the app is backgrounded, and recreating the device each time would
/// throw away every GPU resource.
pub struct Renderer {
    instance: wgpu::Instance,
    gpu: Option<Gpu>,
    surface: Option<SurfaceState>,
    /// Built for the surface's format on first attach; kept across surfaces.
    /// Owns the glyph atlas texture and the image textures, so they survive
    /// a surface being destroyed and recreated.
    quads: Option<QuadRenderer>,
    follow_up: FollowUp,
}

/// Decides when a presented frame should be followed by one more.
///
/// A frame acquired from a *suboptimal* swapchain is still shown, and the
/// swapchain is rebuilt afterwards, but only a further frame is drawn into
/// the rebuilt one. With demand-driven rendering (ADR 0012) that frame has
/// to be asked for. It is asked for once per run of suboptimal frames: a
/// swapchain that reports suboptimal on every frame (a platform that never
/// matches the surface's transform) must not keep the app rendering.
#[derive(Clone, Copy, Debug, Default)]
struct FollowUp {
    /// The previous presented frame was suboptimal.
    was_suboptimal: bool,
    /// A follow-up frame is wanted and has not been collected yet.
    wanted: bool,
}

impl FollowUp {
    /// Records a presented frame.
    fn presented(&mut self, suboptimal: bool) {
        if suboptimal && !self.was_suboptimal {
            self.wanted = true;
        }
        self.was_suboptimal = suboptimal;
    }

    /// The surface went away or was replaced: nothing is owed to the old
    /// one, and the new one starts a new run.
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn take(&mut self) -> bool {
        std::mem::take(&mut self.wanted)
    }
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer {
    /// Creates a renderer with no device and no surface. Cheap; the device is
    /// created by the first [`attach_surface`](Self::attach_surface).
    pub fn new() -> Self {
        Self {
            instance: new_instance(),
            gpu: None,
            surface: None,
            quads: None,
            follow_up: FollowUp::default(),
        }
    }

    /// Attaches to a native drawable, creating the GPU device on first use.
    ///
    /// Blocks while the adapter and device are created (first call only).
    /// Any previously attached surface is released first.
    ///
    /// # Safety
    /// `native` must point to a live drawable of the stated kind, and it must
    /// stay alive until [`detach_surface`](Self::detach_surface) returns or
    /// the renderer is dropped.
    pub unsafe fn attach_surface(
        &mut self,
        native: NativeSurface,
        size: SurfaceSize,
    ) -> Result<(), GpuError> {
        self.surface = None;
        self.follow_up.reset();

        let target = surface_target(native);
        // SAFETY: the caller guarantees the handle is valid and outlives the
        // surface, which is dropped in `detach_surface` or with `self`.
        let surface = unsafe { self.instance.create_surface_unsafe(target) }
            .map_err(|e| GpuError::SurfaceCreation(e.to_string()))?;

        if self.gpu.is_none() {
            self.gpu = Some(pollster::block_on(request_gpu(
                &self.instance,
                Some(&surface),
            ))?);
        }
        let gpu = self.gpu.as_ref().expect("gpu initialised above");

        let caps = surface.get_capabilities(&gpu.adapter);
        // Colours are encoded on the CPU and composited as encoded values
        // (ADR 0023), so the target must store what the shader writes
        // unchanged: a format without the sRGB suffix. Where the surface
        // offers only sRGB formats, the same texture is rendered to through
        // a view of its plain twin.
        let surface_format = caps
            .formats
            .iter()
            .copied()
            .find(|format| !format.is_srgb())
            .or_else(|| caps.formats.first().copied())
            .ok_or(GpuError::UnsupportedSurface)?;
        let format = surface_format.remove_srgb_suffix();
        let alpha_mode = *caps
            .alpha_modes
            .first()
            .ok_or(GpuError::UnsupportedSurface)?;

        let mut config = surface
            .get_default_config(&gpu.adapter, size.width.max(1), size.height.max(1))
            .ok_or(GpuError::UnsupportedSurface)?;
        config.format = surface_format;
        if format != surface_format {
            config.view_formats = vec![format];
        }
        config.alpha_mode = alpha_mode;
        // Vsync. Frame pacing comes from the display, not from a busy loop.
        config.present_mode = wgpu::PresentMode::Fifo;

        let mut state = SurfaceState {
            surface,
            config,
            target_format: format,
            configured: false,
        };
        state.configure(&gpu.device, size);
        self.surface = Some(state);

        // Pipelines are compiled per target format. A recreated surface
        // almost always has the same format, so this normally runs once.
        if self.quads.as_ref().map(QuadRenderer::format) != Some(format) {
            self.quads = Some(QuadRenderer::new(&gpu.device, format));
        }
        Ok(())
    }

    /// Releases the surface. The device and its resources are kept.
    ///
    /// After this returns the native drawable is no longer referenced.
    pub fn detach_surface(&mut self) {
        self.surface = None;
        self.follow_up.reset();
    }

    pub fn has_surface(&self) -> bool {
        self.surface.is_some()
    }

    /// Reconfigures the swapchain for a new drawable size. No-op without a
    /// surface.
    pub fn resize(&mut self, size: SurfaceSize) {
        if let (Some(gpu), Some(state)) = (&self.gpu, &mut self.surface) {
            state.configure(&gpu.device, size);
        }
    }

    /// Information about the GPU in use, once a device exists.
    pub fn adapter_info(&self) -> Option<&AdapterInfo> {
        self.gpu.as_ref().map(|gpu| &gpu.info)
    }

    /// What the most recent presented frame uploaded of the glyph atlas:
    /// nothing, some rows, or all of it. For benchmarks and diagnostics.
    pub fn last_atlas_upload(&self) -> AtlasUpload {
        self.quads
            .as_ref()
            .map_or(AtlasUpload::None, QuadRenderer::last_atlas_upload)
    }

    /// What the most recent presented frame cost: instances, draw calls,
    /// image uploads and the image textures held. For benchmarks and
    /// diagnostics.
    pub fn last_stats(&self) -> RenderStats {
        self.quads
            .as_ref()
            .map_or_else(RenderStats::default, QuadRenderer::stats)
    }

    /// True, once, if the frame just presented should be followed by another
    /// although nothing in the picture changed: it was drawn into a
    /// swapchain that no longer matches the surface (reported as
    /// *suboptimal*, typically after a rotation that changes no size), and
    /// the swapchain has been rebuilt since. Until a frame is drawn into the
    /// rebuilt swapchain the compositor keeps correcting the old one, which
    /// is right on screen but not the cheapest way to show it.
    ///
    /// Whoever schedules frames calls this after each presented frame and
    /// schedules one more if it returns true. Asking clears the request. At
    /// most one request is made per unbroken run of suboptimal frames, so a
    /// swapchain that is always suboptimal cannot keep the app rendering.
    pub fn take_frame_request(&mut self) -> bool {
        self.follow_up.take()
    }

    /// Clears the surface to `clear`, draws `list` over it and presents.
    /// `atlas` is the glyph atlas that the list's glyphs refer to.
    ///
    /// The frame has no images: image operations in `list` draw nothing.
    /// Use [`render_with_images`](Self::render_with_images) for a list that
    /// draws images.
    pub fn render(
        &mut self,
        clear: Color,
        list: &DisplayList,
        atlas: AtlasView<'_>,
    ) -> Result<FrameOutcome, GpuError> {
        self.render_with_images(clear, list, atlas, ImagesView::EMPTY)
    }

    /// As [`render`](Self::render), with the registered images that the
    /// list's image operations refer to.
    ///
    /// An image is uploaded to the GPU the first time it is drawn, and its
    /// texture is freed by the first frame rendered after it has left
    /// `images`. Textures belong to the device, not the surface, so they
    /// survive [`detach_surface`](Self::detach_surface).
    pub fn render_with_images(
        &mut self,
        clear: Color,
        list: &DisplayList,
        atlas: AtlasView<'_>,
        images: ImagesView<'_>,
    ) -> Result<FrameOutcome, GpuError> {
        let (Some(gpu), Some(state), Some(quads)) = (&self.gpu, &mut self.surface, &mut self.quads)
        else {
            return Ok(FrameOutcome::Skipped);
        };
        if !state.configured {
            return Ok(FrameOutcome::Skipped);
        }

        let acquire_start = Instant::now();
        let (frame, suboptimal) = match state.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => (frame, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => (frame, true),
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(FrameOutcome::Skipped);
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                state.surface.configure(&gpu.device, &state.config);
                return Ok(FrameOutcome::Skipped);
            }
            wgpu::CurrentSurfaceTexture::Validation => return Err(GpuError::Validation),
        };
        let acquire = acquire_start.elapsed();

        let encode_start = Instant::now();
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(state.target_format),
            ..Default::default()
        });
        let size = SurfaceSize::new(state.config.width, state.config.height);
        quads.prepare(&gpu.device, &gpu.queue, size, list, atlas, images);
        gpu.queue
            .submit([encode_frame(&gpu.device, &view, clear, quads)]);
        let encode = encode_start.elapsed();

        let present_start = Instant::now();
        gpu.queue.present(frame);
        let present = present_start.elapsed();

        if suboptimal {
            // The swapchain still works but no longer matches the surface
            // (typically after rotation). Rebuild it for the next frame,
            // which `take_frame_request` lets the caller schedule.
            state.surface.configure(&gpu.device, &state.config);
        }
        self.follow_up.presented(suboptimal);

        Ok(FrameOutcome::Presented(FrameTimings {
            acquire,
            encode,
            present,
        }))
    }
}

impl SurfaceState {
    fn configure(&mut self, device: &wgpu::Device, size: SurfaceSize) {
        if size.is_empty() {
            // A zero-sized swapchain is invalid; stay unconfigured until a
            // real size arrives.
            self.configured = false;
            return;
        }
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(device, &self.config);
        self.configured = true;
    }
}

pub(crate) fn new_instance() -> wgpu::Instance {
    wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle())
}

pub(crate) async fn request_gpu(
    instance: &wgpu::Instance,
    compatible_surface: Option<&wgpu::Surface<'_>>,
) -> Result<Gpu, GpuError> {
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            // UI rendering is light GPU work; favour battery life.
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface,
            ..Default::default()
        })
        .await
        .map_err(|e| GpuError::NoAdapter(e.to_string()))?;

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("morningstar"),
            // Stay within what every target device supports.
            required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            ..Default::default()
        })
        .await
        .map_err(|e| GpuError::DeviceCreation(e.to_string()))?;

    let raw = adapter.get_info();
    let info = AdapterInfo {
        name: raw.name,
        backend: format!("{:?}", raw.backend),
        driver: format!("{} {}", raw.driver, raw.driver_info)
            .trim()
            .to_owned(),
    };

    Ok(Gpu {
        device,
        queue,
        info,
        adapter,
    })
}

/// Encodes one render pass that clears `view` to `color` and then draws the
/// rectangles `quads` was last prepared with.
pub(crate) fn encode_frame(
    device: &wgpu::Device,
    view: &wgpu::TextureView,
    color: Color,
    quads: &QuadRenderer,
) -> wgpu::CommandBuffer {
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("morningstar frame"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    // Encoded, like every colour drawn over it.
                    load: wgpu::LoadOp::Clear({
                        let [r, g, b, a] = color.srgb();
                        wgpu::Color {
                            r: f64::from(r),
                            g: f64::from(g),
                            b: f64::from(b),
                            a: f64::from(a),
                        }
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        quads.draw(&mut pass);
    }
    encoder.finish()
}

fn surface_target(native: NativeSurface) -> wgpu::SurfaceTargetUnsafe {
    match native {
        #[cfg(target_os = "android")]
        NativeSurface::AndroidNativeWindow(window) => {
            use wgpu::rwh::{
                AndroidDisplayHandle, AndroidNdkWindowHandle, RawDisplayHandle, RawWindowHandle,
            };
            wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(RawDisplayHandle::Android(AndroidDisplayHandle::new())),
                raw_window_handle: RawWindowHandle::AndroidNdk(AndroidNdkWindowHandle::new(window)),
            }
        }
        #[cfg(target_vendor = "apple")]
        NativeSurface::CoreAnimationLayer(layer) => {
            wgpu::SurfaceTargetUnsafe::CoreAnimationLayer(layer.as_ptr())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_suboptimal_frame_asks_for_exactly_one_more() {
        let mut follow_up = FollowUp::default();
        follow_up.presented(false);
        assert!(!follow_up.take(), "an ordinary frame asks for nothing");

        follow_up.presented(true);
        assert!(follow_up.take());
        assert!(!follow_up.take(), "asking clears the request");

        // The follow-up frame lands in the rebuilt swapchain.
        follow_up.presented(false);
        assert!(!follow_up.take());

        // A later rotation starts over.
        follow_up.presented(true);
        assert!(follow_up.take());
    }

    #[test]
    fn a_permanently_suboptimal_swapchain_does_not_render_forever() {
        let mut follow_up = FollowUp::default();
        follow_up.presented(true);
        assert!(follow_up.take());
        for _ in 0..10 {
            follow_up.presented(true);
            assert!(
                !follow_up.take(),
                "one request per run of suboptimal frames"
            );
        }
    }

    #[test]
    fn an_uncollected_request_survives_frames_but_not_the_surface() {
        let mut follow_up = FollowUp::default();
        follow_up.presented(true);
        follow_up.presented(false);
        assert!(follow_up.take(), "still owed until collected");

        follow_up.presented(true);
        follow_up.reset();
        assert!(!follow_up.take(), "the surface it concerned is gone");
        follow_up.presented(true);
        assert!(follow_up.take(), "a new surface starts a new run");
    }

    #[test]
    fn a_renderer_without_a_surface_reports_nothing() {
        let mut renderer = Renderer::new();
        assert!(!renderer.take_frame_request());
        assert_eq!(renderer.last_stats(), RenderStats::default());
    }
}
