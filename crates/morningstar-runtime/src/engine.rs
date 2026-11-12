use std::time::{Duration, Instant};

use morningstar_core::{
    AtlasView, Color, DisplayList, FrameStats, FrameSummary, HostState, ImagesView, Insets,
    Lifecycle, LifecycleEvent, PointerEvent, ServiceCall, ServiceId, ServiceResult, SurfaceSize,
    SystemBars, TextInputCommand, TextInputEvent, TextInputRequest,
};
use morningstar_gpu::{AdapterInfo, FrameOutcome, GpuError, NativeSurface, Renderer};

use crate::app::{clean_inset, is_valid_scale};
use crate::{App, Frame, FrameInfo};

/// Rolling window used for frame statistics (~4 s at 120 Hz).
const STATS_WINDOW: usize = 480;

/// What the engine needs from the thing that puts a frame on screen. The
/// only real implementation is the GPU [`Renderer`]; the indirection exists
/// so the engine's scheduling can be tested on a machine with no surface.
trait Presenter {
    /// # Safety
    /// As [`Renderer::attach_surface`].
    unsafe fn attach_surface(
        &mut self,
        native: NativeSurface,
        size: SurfaceSize,
    ) -> Result<(), GpuError>;
    fn detach_surface(&mut self);
    fn resize(&mut self, size: SurfaceSize);
    fn render(
        &mut self,
        clear: Color,
        list: &DisplayList,
        atlas: AtlasView<'_>,
    ) -> Result<FrameOutcome, GpuError>;
    /// As `render`, with the registered images the list may draw. A
    /// presenter that does not draw images ignores them.
    fn render_with_images(
        &mut self,
        clear: Color,
        list: &DisplayList,
        atlas: AtlasView<'_>,
        images: ImagesView<'_>,
    ) -> Result<FrameOutcome, GpuError> {
        let _ = images;
        self.render(clear, list, atlas)
    }
    /// True, once, if the frame just presented should be followed by
    /// another although nothing changed. See
    /// [`Renderer::take_frame_request`].
    fn take_frame_request(&mut self) -> bool {
        false
    }
    fn adapter_info(&self) -> Option<&AdapterInfo>;
}

impl Presenter for Renderer {
    unsafe fn attach_surface(
        &mut self,
        native: NativeSurface,
        size: SurfaceSize,
    ) -> Result<(), GpuError> {
        // SAFETY: forwarded caller contract.
        unsafe { Renderer::attach_surface(self, native, size) }
    }

    fn detach_surface(&mut self) {
        Renderer::detach_surface(self);
    }

    fn resize(&mut self, size: SurfaceSize) {
        Renderer::resize(self, size);
    }

    fn render(
        &mut self,
        clear: Color,
        list: &DisplayList,
        atlas: AtlasView<'_>,
    ) -> Result<FrameOutcome, GpuError> {
        Renderer::render(self, clear, list, atlas)
    }

    fn render_with_images(
        &mut self,
        clear: Color,
        list: &DisplayList,
        atlas: AtlasView<'_>,
        images: ImagesView<'_>,
    ) -> Result<FrameOutcome, GpuError> {
        Renderer::render_with_images(self, clear, list, atlas, images)
    }

    fn take_frame_request(&mut self) -> bool {
        Renderer::take_frame_request(self)
    }

    fn adapter_info(&self) -> Option<&AdapterInfo> {
        Renderer::adapter_info(self)
    }
}

/// Drives one application: lifecycle in, frames out.
///
/// Frames are produced on demand. The engine keeps one flag, "a frame is
/// wanted", which is set by everything that can change the picture (see
/// [`needs_frame`](Self::needs_frame)) and cleared when a frame reaches the
/// display. A host calls [`frame`](Self::frame) at the display's pace while
/// `needs_frame` is true and sleeps otherwise.
///
/// The engine is not thread-safe by design. A host creates it on, and only
/// calls it from, the one thread it renders on.
pub struct Engine {
    app: Box<dyn App>,
    presenter: Box<dyn Presenter>,
    lifecycle: Lifecycle,
    /// Reused every frame so the display list keeps its allocation.
    frame: Frame,
    frame_index: u64,
    scale: f32,
    insets: Insets,
    /// Height of the soft keyboard over the bottom of the surface, pixels.
    keyboard_inset: f32,
    /// A frame is wanted: something changed since the last one was shown.
    redraw: bool,
    /// A frame is wanted at this time, asked for by the last frame built
    /// (`Frame::request_redraw_after`).
    redraw_at: Option<Instant>,
    /// Frames that were produced because `redraw_at` came, and for no other
    /// reason. Diagnostic: shows a caret blinking without other rendering.
    timed_frames: u64,
    /// The text input the last frame built asked for.
    text_declared: Option<TextInputRequest>,
    /// The text input the host is believed to hold: what it was last told
    /// through `take_text_input_command`, brought up to date by the edits it
    /// reports itself.
    text_host: Option<TextInputRequest>,
    /// How the last frame built said the system bars should look, and how
    /// the host is believed to show them.
    bars_declared: Option<SystemBars>,
    bars_host: Option<SystemBars>,
    /// The user asked the system for less motion.
    reduce_motion: bool,
    /// Benchmark mode: every frame asks for another.
    continuous: bool,
    /// Foreground time accumulated before the current resume.
    elapsed_before_resume: Duration,
    resumed_at: Option<Instant>,
    /// When the previous frame was presented, but only if the engine has
    /// wanted a frame ever since. `None` after an idle period, so the time
    /// spent idle is never recorded as a frame interval.
    last_present: Option<Instant>,
    intervals: FrameStats,
    cpu: FrameStats,
    present: FrameStats,
}

/// Frame statistics over the most recent window of presented frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineReport {
    /// Frames presented since the engine was created.
    pub frames: u64,
    /// Time between consecutive presented frames, for frames that followed
    /// one another without the engine going idle in between: an animation,
    /// or continuous mode. A frame produced after an idle period (for
    /// example the first frame after a touch) has no interval, because the
    /// time since the previous frame says nothing about how fast it was.
    /// `None` until two frames have been presented back to back.
    pub interval: Option<FrameSummary>,
    /// CPU time spent building each frame and encoding and submitting its
    /// GPU commands. This is the part of the frame budget Morningstar uses.
    pub cpu: FrameSummary,
    /// Time inside the present call. Depending on the driver this includes
    /// waiting for the display, so it is reported separately from `cpu`.
    pub present: FrameSummary,
}

impl Engine {
    /// An engine for `app`, set up as if there were no host: nothing
    /// stored and the placeholder installation id. A host that has those
    /// uses [`with_host_state`](Self::with_host_state).
    pub fn new(app: Box<dyn App>) -> Self {
        Self::with_host_state(app, &HostState::new())
    }

    /// An engine for `app`, which is handed `host` at setup: the
    /// installation id and the values stored on earlier runs
    /// ([`Setup::install_id`](crate::Setup::install_id),
    /// [`Setup::stored`](crate::Setup::stored)).
    pub fn with_host_state(app: Box<dyn App>, host: &HostState) -> Self {
        Self::with_presenter_and_host(app, Box::new(Renderer::new()), host)
    }

    #[cfg(test)]
    fn with_presenter(app: Box<dyn App>, presenter: Box<dyn Presenter>) -> Self {
        Self::with_presenter_and_host(app, presenter, &HostState::new())
    }

    fn with_presenter_and_host(
        mut app: Box<dyn App>,
        presenter: Box<dyn Presenter>,
        host: &HostState,
    ) -> Self {
        let frame = Frame::for_app_with_host(app.as_mut(), host);
        Self {
            app,
            presenter,
            lifecycle: Lifecycle::new(),
            frame,
            frame_index: 0,
            scale: 1.0,
            insets: Insets::ZERO,
            keyboard_inset: 0.0,
            redraw: true,
            redraw_at: None,
            timed_frames: 0,
            text_declared: None,
            text_host: None,
            bars_declared: None,
            bars_host: None,
            reduce_motion: false,
            continuous: false,
            elapsed_before_resume: Duration::ZERO,
            resumed_at: None,
            last_present: None,
            intervals: FrameStats::new(STATS_WINDOW),
            cpu: FrameStats::new(STATS_WINDOW),
            present: FrameStats::new(STATS_WINDOW),
        }
    }

    /// The app moved to the foreground. Always produces a frame.
    pub fn resume(&mut self) {
        if self.resumed_at.is_none() {
            self.resumed_at = Some(Instant::now());
        }
        self.lifecycle.apply(LifecycleEvent::Resumed);
        self.redraw = true;
    }

    /// The app left the foreground. Frames stop until [`resume`](Self::resume).
    pub fn pause(&mut self) {
        if let Some(resumed_at) = self.resumed_at.take() {
            self.elapsed_before_resume += resumed_at.elapsed();
        }
        // The gap until the next frame is not a slow frame.
        self.last_present = None;
        // Resuming always draws, so a timer has nothing left to do.
        self.redraw_at = None;
        self.lifecycle.apply(LifecycleEvent::Paused);
    }

    /// A native drawable is available. May block while the GPU device is
    /// created (first call only). Always produces a frame.
    ///
    /// # Safety
    /// `native` must point to a live drawable of the stated kind and remain
    /// valid until [`surface_destroyed`](Self::surface_destroyed) returns or
    /// the engine is dropped.
    pub unsafe fn surface_created(
        &mut self,
        native: NativeSurface,
        size: SurfaceSize,
    ) -> Result<(), GpuError> {
        // SAFETY: forwarded caller contract.
        unsafe { self.presenter.attach_surface(native, size) }?;
        self.surface_attached(size);
        Ok(())
    }

    /// The part of [`surface_created`](Self::surface_created) that does not
    /// need a native drawable.
    fn surface_attached(&mut self, size: SurfaceSize) {
        self.lifecycle.apply(LifecycleEvent::SurfaceCreated(size));
        self.redraw = true;
    }

    /// The drawable changed size. Returns true if the size is new, in which
    /// case a frame is produced.
    pub fn surface_resized(&mut self, size: SurfaceSize) -> bool {
        if self.lifecycle.surface() == Some(size) {
            return false;
        }
        self.presenter.resize(size);
        self.lifecycle.apply(LifecycleEvent::SurfaceResized(size));
        self.redraw = true;
        true
    }

    /// The native drawable is going away. When this returns, the engine no
    /// longer references it.
    pub fn surface_destroyed(&mut self) {
        self.presenter.detach_surface();
        self.last_present = None;
        self.lifecycle.apply(LifecycleEvent::SurfaceDestroyed);
    }

    /// Sets the display scale: physical pixels per density-independent unit
    /// (see [`FrameInfo::scale`]). Returns true if it changed, in which case
    /// a frame is produced. A value that is not finite and positive is
    /// ignored. Hosts call this at start and whenever the scale may have
    /// changed; repeating the current value costs nothing.
    pub fn set_scale(&mut self, scale: f32) -> bool {
        if !is_valid_scale(scale) || scale == self.scale {
            return false;
        }
        self.scale = scale;
        self.redraw = true;
        true
    }

    /// Sets the safe-area insets in physical pixels (see
    /// [`FrameInfo::insets`]). Returns true if they changed, in which case a
    /// frame is produced. Negative or non-finite edges are treated as zero.
    pub fn set_insets(&mut self, insets: Insets) -> bool {
        let insets = insets.sanitized();
        if insets == self.insets {
            return false;
        }
        self.insets = insets;
        self.redraw = true;
        true
    }

    /// Sets how much of the bottom of the surface the soft keyboard covers,
    /// in physical pixels from the surface's bottom edge (see
    /// [`FrameInfo::keyboard_inset`]); zero when it is hidden. Returns true
    /// if it changed, in which case a frame is produced. Hosts call it
    /// whenever the keyboard's frame may have changed.
    pub fn set_keyboard_inset(&mut self, inset: f32) -> bool {
        let inset = clean_inset(inset);
        if inset == self.keyboard_inset {
            return false;
        }
        self.keyboard_inset = inset;
        self.redraw = true;
        true
    }

    /// Says whether the user has asked the system for less motion
    /// (Android: the animator duration scale is zero; iOS: "reduce
    /// motion"); see [`FrameInfo::reduce_motion`]. Returns true if it
    /// changed, in which case a frame is produced. Hosts call it at start
    /// and whenever the setting may have changed.
    pub fn set_reduce_motion(&mut self, reduce: bool) -> bool {
        if reduce == self.reduce_motion {
            return false;
        }
        self.reduce_motion = reduce;
        self.redraw = true;
        true
    }

    /// The keyboard inset most recently set.
    pub fn keyboard_inset(&self) -> f32 {
        self.keyboard_inset
    }

    /// The display scale most recently accepted by [`set_scale`](Self::set_scale).
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// The insets most recently set by [`set_insets`](Self::set_insets).
    pub fn insets(&self) -> Insets {
        self.insets
    }

    /// Asks for a frame on behalf of the host, for example when the platform
    /// reports that the window's contents were lost and must be redrawn.
    pub fn request_redraw(&mut self) {
        self.redraw = true;
    }

    /// Turns continuous rendering on or off. While on, a frame is produced
    /// at every display refresh whether or not anything changed. This is for
    /// benchmarks, which need a steady stream of frames to measure; it is
    /// not how an application animates (see [`Frame::request_redraw`]).
    pub fn set_continuous(&mut self, continuous: bool) {
        self.continuous = continuous;
        if continuous {
            self.redraw = true;
        }
    }

    /// True while the app is in the foreground with a non-empty surface.
    /// Whether there is anything to draw is a separate question, answered by
    /// [`needs_frame`](Self::needs_frame).
    pub fn should_render(&self) -> bool {
        self.lifecycle.should_render()
    }

    /// True when [`frame`](Self::frame) would draw: the app is visible and a
    /// frame is wanted. A frame is wanted after
    ///
    /// - the app became visible, or its surface was created or resized;
    /// - the scale or the insets changed;
    /// - input was delivered;
    /// - the app called [`Frame::request_redraw`] during the previous frame,
    ///   or the delay it gave to [`Frame::request_redraw_after`] has passed;
    /// - text input was delivered, or the keyboard inset changed;
    /// - the app handled the system's back ([`back`](Self::back));
    /// - the answer to a platform request reached the app
    ///   ([`service_result`](Self::service_result));
    /// - the host called [`request_redraw`](Self::request_redraw);
    /// - a frame could not be shown (it was skipped or failed), until one is;
    /// - a frame was shown through a swapchain that no longer matched the
    ///   surface (for example after a half-turn rotation), which is followed
    ///   by one frame drawn into the rebuilt swapchain.
    ///
    /// While this is false the host must not call `frame` in a loop: it
    /// blocks its render thread or pauses its display link, and checks again
    /// after the next call it makes into the engine, or when the time given
    /// by [`redraw_due_in`](Self::redraw_due_in) has passed. Nothing else
    /// can change the answer.
    pub fn needs_frame(&self) -> bool {
        self.needs_frame_at(Instant::now())
    }

    fn needs_frame_at(&self, now: Instant) -> bool {
        (self.redraw || self.redraw_at.is_some_and(|at| now >= at))
            && self.lifecycle.should_render()
    }

    /// How long until a frame asked for with [`Frame::request_redraw_after`]
    /// is due: the longest a host may sleep while
    /// [`needs_frame`](Self::needs_frame) is false. `None` when no timed
    /// frame is pending, or the app is not visible (becoming visible draws
    /// anyway): the host then sleeps until its next call into the engine.
    pub fn redraw_due_in(&self) -> Option<Duration> {
        let at = self.redraw_at.filter(|_| self.lifecycle.should_render())?;
        Some(at.saturating_duration_since(Instant::now()))
    }

    /// Frames produced only because a delay given to
    /// [`Frame::request_redraw_after`] ran out. For diagnostics: a caret
    /// that blinks twice a second adds two a second here and nothing else.
    pub fn timed_frames(&self) -> u64 {
        self.timed_frames
    }

    /// Produces and presents one frame if [`needs_frame`](Self::needs_frame)
    /// is true. Otherwise returns [`FrameOutcome::Skipped`] without calling
    /// the app or touching the GPU.
    ///
    /// After `Skipped` or an error while a frame is still needed, the host
    /// should wait briefly before trying again rather than spin.
    pub fn frame(&mut self) -> Result<FrameOutcome, GpuError> {
        let Some(size) = self.lifecycle.surface().filter(|_| self.needs_frame()) else {
            return Ok(FrameOutcome::Skipped);
        };
        // This frame answers every request made so far. Requests made from
        // here on, including by the app while it builds the frame, are for
        // the next one. A timed request is answered too, whichever came
        // first: the app asks again from this frame if it still needs one.
        let timed = !self.redraw;
        self.redraw = false;
        self.redraw_at = None;

        let build_start = Instant::now();
        self.frame.begin();
        let info = FrameInfo::new(size)
            .with_index(self.frame_index)
            .with_elapsed(self.elapsed())
            .with_scale(self.scale)
            .with_insets(self.insets)
            .with_keyboard_inset(self.keyboard_inset)
            .with_reduce_motion(self.reduce_motion);
        self.app.frame(&info, &mut self.frame);
        let build = build_start.elapsed();
        self.redraw_at = self
            .frame
            .redraw_after()
            .and_then(|delay| Instant::now().checked_add(delay));
        // What the app asked of the keyboard holds whether or not the frame
        // reaches the display. Compared first so that a field that did not
        // change costs no allocation.
        if self.text_declared.as_ref() != self.frame.text_input() {
            self.text_declared = self.frame.text_input().cloned();
        }
        // A frame with no opinion about the bars leaves them as they are.
        if let Some(bars) = self.frame.system_bars() {
            self.bars_declared = Some(bars);
        }
        let again = self.continuous || self.frame.redraw_requested();

        let outcome = self.presenter.render_with_images(
            self.frame.clear,
            &self.frame.list,
            self.frame.atlas(),
            self.frame.images(),
        );
        match outcome {
            Ok(FrameOutcome::Presented(timings)) => {
                // The renderer may want one more frame of the same picture
                // (it rebuilt a suboptimal swapchain). Nothing else would
                // ask for it once the app is idle.
                let again = self.presenter.take_frame_request() || again;
                let now = Instant::now();
                if let Some(previous) = self.last_present {
                    self.intervals.record(now - previous);
                }
                // Going idle ends the run of back-to-back frames: whenever
                // the next frame comes, the time until then is not its cost.
                self.last_present = again.then_some(now);
                self.cpu.record(build + timings.encode);
                self.present.record(timings.present);
                self.frame_index += 1;
                self.timed_frames += u64::from(timed);
                self.redraw |= again;
            }
            // What the app drew never reached the display, so the frame is
            // still owed, whatever the app asked for.
            Ok(FrameOutcome::Skipped) | Err(_) => self.redraw = true,
        }
        outcome
    }

    /// Delivers a touch event to the app, and schedules a frame so that
    /// whatever the app changed in response becomes visible. Several events
    /// delivered before the next frame share that one frame.
    ///
    /// Events are accepted at any time, including while no frame can be
    /// drawn, so that an `Up` or `Cancelled` is never lost and a press
    /// cannot get stuck.
    pub fn pointer(&mut self, event: PointerEvent) {
        self.app.pointer(&event);
        self.redraw = true;
    }

    /// Delivers a text-input event from the platform's keyboard to the app,
    /// and schedules a frame.
    ///
    /// An [`Edited`](TextInputEvent::Edited) event also records what the
    /// host's editor now holds, and
    /// [`FocusLost`](TextInputEvent::FocusLost) that its keyboard is gone, so
    /// that [`take_text_input_command`](Self::take_text_input_command) only
    /// reports what the host does not already know.
    pub fn text_input(&mut self, event: TextInputEvent) {
        match &event {
            TextInputEvent::Edited(state) => {
                if let Some(host) = self.text_host.as_mut() {
                    host.text.clone_from(&state.text);
                    host.caret = state.caret;
                    host.anchor = state.anchor.filter(|anchor| *anchor != state.caret);
                }
            }
            TextInputEvent::FocusLost => {
                // The keyboard is already down. If the app keeps its field
                // focused, the next frame asks again and it is shown again.
                self.text_host = None;
                self.text_declared = None;
            }
            _ => {}
        }
        self.app.text_input(&event);
        self.redraw = true;
    }

    /// Delivers the system's back gesture or key to the app and returns
    /// whether the app handled it ([`App::back`]). Handled: a frame is
    /// scheduled, as after any input. Not handled: nothing changes here, and
    /// the host does what the platform does when an app has nowhere to go
    /// back to (on Android, leave the app).
    pub fn back(&mut self) -> bool {
        let handled = self.app.back();
        self.redraw |= handled;
        handled
    }

    /// Moves the requests the app has made of the platform since the last
    /// call into `out` ([`App::service_requests`]). Usually there are none
    /// and this costs one call.
    ///
    /// A host calls this once after creating the engine and then after
    /// every other call it makes into it, as it asks
    /// [`needs_frame`](Self::needs_frame): requests only appear inside
    /// those calls. It carries each one out off this thread and answers
    /// through [`service_result`](Self::service_result). A request it
    /// cannot carry out it answers at once with
    /// [`ServiceRequest::unsupported`](morningstar_core::ServiceRequest::unsupported),
    /// so that the app is never left waiting.
    pub fn take_service_requests(&mut self, out: &mut Vec<ServiceCall>) {
        self.app.service_requests(out);
    }

    /// Delivers the platform's answer to a request to the app
    /// ([`App::service_result`]), on this thread, between frames. If the
    /// app was waiting for it a frame is scheduled, as after input, and
    /// `true` is returned; an answer nobody is waiting for changes nothing.
    ///
    /// Answers are accepted at any time, including while the app is hidden
    /// or has no surface: the app's state moves on, and the frame comes
    /// when it is visible again. The app may make new requests in
    /// response, so call [`take_service_requests`](Self::take_service_requests)
    /// afterwards.
    pub fn service_result(&mut self, id: ServiceId, result: ServiceResult) -> bool {
        let delivered = self.app.service_result(id, result);
        self.redraw |= delivered;
        delivered
    }

    /// What the host must do to its keyboard so that it matches the last
    /// frame built: show it for a field, replace the text it holds, or hide
    /// it. `None` when the host is already up to date, which is the usual
    /// answer. Hosts call this after every [`frame`](Self::frame).
    pub fn take_text_input_command(&mut self) -> Option<TextInputCommand> {
        if self.text_declared == self.text_host {
            return None;
        }
        let command = match (&self.text_declared, &self.text_host) {
            (None, _) => TextInputCommand::Hide,
            (Some(want), Some(have))
                if want.session == have.session && want.traits == have.traits =>
            {
                TextInputCommand::Update(want.clone())
            }
            (Some(want), _) => TextInputCommand::Show(want.clone()),
        };
        self.text_host.clone_from(&self.text_declared);
        Some(command)
    }

    /// How the host must now show the system's bars (light or dark icons),
    /// if the last frame built wants them different from how the host was
    /// last told to show them; `None` otherwise, which is the usual answer.
    /// Hosts call this after every [`frame`](Self::frame), as they call
    /// [`take_text_input_command`](Self::take_text_input_command).
    pub fn take_system_bars(&mut self) -> Option<SystemBars> {
        if self.bars_declared == self.bars_host {
            return None;
        }
        self.bars_host = self.bars_declared;
        self.bars_declared
    }

    /// Number of frames presented so far.
    pub fn frames_presented(&self) -> u64 {
        self.frame_index
    }

    /// Statistics for the recent window, once a frame has been presented.
    pub fn report(&self) -> Option<EngineReport> {
        Some(EngineReport {
            frames: self.frame_index,
            interval: self.intervals.summary(),
            cpu: self.cpu.summary()?,
            present: self.present.summary()?,
        })
    }

    /// The GPU in use, once the first surface has been attached.
    pub fn adapter_info(&self) -> Option<&AdapterInfo> {
        self.presenter.adapter_info()
    }

    fn elapsed(&self) -> Duration {
        self.elapsed_before_resume + self.resumed_at.map_or(Duration::ZERO, |at| at.elapsed())
    }
}

impl std::fmt::Display for EngineReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "frames={} interval[", self.frames)?;
        match self.interval {
            Some(interval) => write!(f, "{interval}")?,
            // No two frames were back to back: the app only drew on demand.
            None => f.write_str("n=0")?,
        }
        write!(f, "] cpu[{}] present[{}]", self.cpu, self.present)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use morningstar_core::{HttpRequest, HttpResponse, ServiceRequest};
    use morningstar_core::{Point, PointerPhase, Size};
    use morningstar_gpu::FrameTimings;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    const SIZE: SurfaceSize = SurfaceSize::new(1080, 2400);

    /// What the test app saw, and how it should behave.
    #[derive(Default)]
    struct Probe {
        /// The `FrameInfo` of every call to `App::frame`.
        frames: Vec<FrameInfo>,
        pointers: u32,
        /// The app calls `request_redraw` on this many more frames, as an
        /// app running an animation of that length would.
        animate_frames: u32,
        /// The app calls `request_redraw_after` with this on every frame.
        redraw_after: Option<Duration>,
        /// The app declares this text input on every frame.
        text: Option<TextInputRequest>,
        /// The app declares these system bars on every frame.
        bars: Option<SystemBars>,
        text_events: Vec<TextInputEvent>,
        /// What the app answers to `back`, and how often it was asked.
        handles_back: bool,
        backs: u32,
        /// Requests the app has made and the host has not taken yet.
        outbox: Vec<ServiceCall>,
        /// Ids the app is waiting for an answer to.
        waiting: Vec<ServiceId>,
        /// The answers the app accepted, in order.
        results: Vec<(ServiceId, ServiceResult)>,
        /// What the app made of each accepted answer: a request to make.
        follow_up: Option<ServiceCall>,
        /// What setup saw.
        install_id: String,
        stored_device_id: Option<String>,
    }

    struct ProbeApp(Arc<Mutex<Probe>>);

    impl App for ProbeApp {
        fn setup(&mut self, setup: &mut crate::Setup<'_>) {
            let mut probe = self.0.lock().unwrap();
            probe.install_id = setup.install_id().to_owned();
            probe.stored_device_id = setup.stored("device_id").map(str::to_owned);
        }

        fn service_requests(&mut self, out: &mut Vec<ServiceCall>) {
            let mut probe = self.0.lock().unwrap();
            let calls: Vec<ServiceCall> = probe.outbox.drain(..).collect();
            for call in &calls {
                if call.request.expects_result() {
                    probe.waiting.push(call.id);
                }
            }
            out.extend(calls);
        }

        fn service_result(&mut self, id: ServiceId, result: ServiceResult) -> bool {
            let mut probe = self.0.lock().unwrap();
            let Some(index) = probe.waiting.iter().position(|waiting| *waiting == id) else {
                return false;
            };
            probe.waiting.remove(index);
            probe.results.push((id, result));
            if let Some(call) = probe.follow_up.take() {
                probe.outbox.push(call);
            }
            true
        }

        fn frame(&mut self, info: &FrameInfo, frame: &mut Frame) {
            let mut probe = self.0.lock().unwrap();
            probe.frames.push(*info);
            if probe.animate_frames > 0 {
                probe.animate_frames -= 1;
                frame.request_redraw();
            }
            if let Some(delay) = probe.redraw_after {
                frame.request_redraw_after(delay);
            }
            if let Some(request) = probe.text.clone() {
                frame.set_text_input(request);
            }
            if let Some(bars) = probe.bars {
                frame.set_system_bars(bars);
            }
        }

        fn text_input(&mut self, event: &TextInputEvent) {
            self.0.lock().unwrap().text_events.push(event.clone());
        }

        fn pointer(&mut self, _event: &PointerEvent) {
            self.0.lock().unwrap().pointers += 1;
        }

        fn back(&mut self) -> bool {
            let mut probe = self.0.lock().unwrap();
            probe.backs += 1;
            probe.handles_back
        }
    }

    /// Stands in for the GPU: counts frames and returns scripted outcomes,
    /// then "presented" once the script runs out.
    #[derive(Default)]
    struct Screen {
        renders: u32,
        script: VecDeque<Result<FrameOutcome, GpuError>>,
        resizes: Vec<SurfaceSize>,
    }

    struct FakePresenter(Arc<Mutex<Screen>>);

    const PRESENTED: FrameOutcome = FrameOutcome::Presented(FrameTimings {
        acquire: Duration::ZERO,
        encode: Duration::from_micros(300),
        present: Duration::from_micros(200),
    });

    impl Presenter for FakePresenter {
        unsafe fn attach_surface(
            &mut self,
            _native: NativeSurface,
            _size: SurfaceSize,
        ) -> Result<(), GpuError> {
            Ok(())
        }

        fn detach_surface(&mut self) {}

        fn resize(&mut self, size: SurfaceSize) {
            self.0.lock().unwrap().resizes.push(size);
        }

        fn render(
            &mut self,
            _clear: Color,
            _list: &DisplayList,
            _atlas: AtlasView<'_>,
        ) -> Result<FrameOutcome, GpuError> {
            let mut screen = self.0.lock().unwrap();
            screen.renders += 1;
            screen.script.pop_front().unwrap_or(Ok(PRESENTED))
        }

        fn adapter_info(&self) -> Option<&AdapterInfo> {
            None
        }
    }

    struct Harness {
        engine: Engine,
        probe: Arc<Mutex<Probe>>,
        screen: Arc<Mutex<Screen>>,
    }

    impl Harness {
        /// An engine with no surface, not yet resumed.
        fn new() -> Self {
            let probe = Arc::new(Mutex::new(Probe::default()));
            let screen = Arc::new(Mutex::new(Screen::default()));
            let engine = Engine::with_presenter(
                Box::new(ProbeApp(probe.clone())),
                Box::new(FakePresenter(screen.clone())),
            );
            Self {
                engine,
                probe,
                screen,
            }
        }

        /// A visible engine that has drawn its first frame and gone idle.
        fn idle() -> Self {
            let mut harness = Self::new();
            harness.engine.resume();
            harness.engine.surface_attached(SIZE);
            assert_eq!(harness.run(), 1, "the first frame");
            harness
        }

        /// Does what a host does: calls `frame` for as long as the engine
        /// needs one. Returns how many frames were presented, and panics if
        /// the engine never goes idle.
        fn run(&mut self) -> u32 {
            let before = self.engine.frames_presented();
            for _ in 0..1000 {
                if !self.engine.needs_frame() {
                    return u32::try_from(self.engine.frames_presented() - before).unwrap();
                }
                let _ = self.engine.frame();
            }
            panic!("the engine never went idle");
        }

        fn app_frames(&self) -> usize {
            self.probe.lock().unwrap().frames.len()
        }

        fn renders(&self) -> u32 {
            self.screen.lock().unwrap().renders
        }

        fn last_info(&self) -> FrameInfo {
            *self.probe.lock().unwrap().frames.last().unwrap()
        }

        fn animate(&self, frames: u32) {
            self.probe.lock().unwrap().animate_frames = frames;
        }

        fn script(&self, outcome: Result<FrameOutcome, GpuError>) {
            self.screen.lock().unwrap().script.push_back(outcome);
        }

        fn touch(&mut self, phase: PointerPhase) {
            self.engine.pointer(PointerEvent {
                id: 1,
                phase,
                position: Point::new(1.0, 2.0),
            });
        }
    }

    // --- Visibility -------------------------------------------------------

    #[test]
    fn app_is_not_asked_for_frames_while_invisible() {
        let mut h = Harness::new();

        assert_eq!(
            h.engine.frame(),
            Ok(FrameOutcome::Skipped),
            "not resumed, no surface"
        );
        h.engine.resume();
        assert!(!h.engine.should_render());
        assert!(!h.engine.needs_frame());
        assert_eq!(
            h.engine.frame(),
            Ok(FrameOutcome::Skipped),
            "resumed, still no surface"
        );

        assert_eq!(h.app_frames(), 0);
        assert_eq!(h.renders(), 0);
        assert_eq!(h.engine.frames_presented(), 0);
        assert_eq!(h.engine.report(), None);
    }

    #[test]
    fn foreground_clock_stops_while_paused() {
        let mut engine = Harness::new().engine;
        assert_eq!(
            engine.elapsed(),
            Duration::ZERO,
            "clock starts on first resume"
        );

        engine.resume();
        std::thread::sleep(Duration::from_millis(5));
        engine.pause();

        let frozen = engine.elapsed();
        assert!(frozen >= Duration::from_millis(5));
        std::thread::sleep(Duration::from_millis(5));
        assert_eq!(engine.elapsed(), frozen);

        engine.resume();
        std::thread::sleep(Duration::from_millis(1));
        assert!(engine.elapsed() > frozen);
    }

    #[test]
    fn surface_events_without_a_surface_are_harmless() {
        let mut engine = Harness::new().engine;
        engine.surface_resized(SurfaceSize::new(10, 10));
        engine.surface_destroyed();
        engine.pause();
        assert!(!engine.should_render());
        assert!(!engine.needs_frame());
    }

    #[test]
    fn pointer_events_reach_the_app_even_while_invisible() {
        let mut h = Harness::new();
        h.touch(PointerPhase::Cancelled);
        assert_eq!(h.probe.lock().unwrap().pointers, 1);
        assert!(!h.engine.needs_frame(), "but nothing can be drawn");
    }

    // --- Redraw scheduling ------------------------------------------------

    #[test]
    fn draws_one_frame_when_it_becomes_visible_then_goes_idle() {
        let mut h = Harness::new();
        h.engine.resume();
        h.engine.surface_attached(SIZE);
        assert!(h.engine.needs_frame());

        assert_eq!(h.engine.frame(), Ok(PRESENTED));
        assert!(!h.engine.needs_frame(), "nothing changed, nothing to draw");
        assert_eq!((h.app_frames(), h.renders()), (1, 1));
    }

    #[test]
    fn an_idle_engine_does_not_call_the_app_or_touch_the_gpu() {
        let mut h = Harness::idle();
        for _ in 0..100 {
            assert_eq!(h.engine.frame(), Ok(FrameOutcome::Skipped));
        }
        assert_eq!((h.app_frames(), h.renders()), (1, 1));
        assert_eq!(h.engine.frames_presented(), 1);
    }

    #[test]
    fn input_produces_exactly_one_frame() {
        let mut h = Harness::idle();
        h.touch(PointerPhase::Down);
        assert!(h.engine.needs_frame());
        assert_eq!(h.run(), 1);
        assert_eq!(h.app_frames(), 2);
    }

    #[test]
    fn events_delivered_before_a_frame_share_it() {
        let mut h = Harness::idle();
        h.touch(PointerPhase::Down);
        h.touch(PointerPhase::Moved);
        h.touch(PointerPhase::Up);
        assert_eq!(h.run(), 1);
        assert_eq!(h.probe.lock().unwrap().pointers, 3);
    }

    #[test]
    fn an_animation_runs_until_the_app_stops_asking() {
        let mut h = Harness::idle();
        h.animate(5);
        h.touch(PointerPhase::Up);
        // Five frames that each ask for another, then the one that does not.
        assert_eq!(h.run(), 6);
        assert!(!h.engine.needs_frame());
        assert_eq!(h.engine.frame(), Ok(FrameOutcome::Skipped));
        assert_eq!(h.app_frames(), 7);
    }

    #[test]
    fn a_request_made_by_the_app_does_not_outlive_its_frame() {
        // The app asks once. If the request were not cleared with the frame
        // that answers it, rendering would never stop.
        let mut h = Harness::idle();
        h.animate(1);
        h.engine.request_redraw();
        assert_eq!(h.run(), 2);
    }

    #[test]
    fn the_host_can_request_a_frame() {
        let mut h = Harness::idle();
        h.engine.request_redraw();
        h.engine.request_redraw();
        assert_eq!(h.run(), 1, "requests do not queue up");
    }

    #[test]
    fn a_resize_produces_a_frame_with_the_new_size() {
        let mut h = Harness::idle();
        let landscape = SurfaceSize::new(2400, 1080);
        assert!(h.engine.surface_resized(landscape));
        assert_eq!(h.run(), 1);
        assert_eq!(h.last_info().size, landscape);
        assert_eq!(h.screen.lock().unwrap().resizes, [landscape]);

        assert!(!h.engine.surface_resized(landscape));
        assert!(
            !h.engine.needs_frame(),
            "the same size again changes nothing"
        );
        assert_eq!(h.screen.lock().unwrap().resizes.len(), 1);
    }

    #[test]
    fn a_request_made_while_hidden_is_drawn_when_visible_again() {
        let mut h = Harness::idle();
        h.engine.pause();
        h.touch(PointerPhase::Cancelled);
        assert!(!h.engine.needs_frame());
        assert_eq!(h.engine.frame(), Ok(FrameOutcome::Skipped));
        assert_eq!(h.app_frames(), 1, "no frame is built while hidden");

        h.engine.resume();
        assert_eq!(h.run(), 1);
    }

    #[test]
    fn becoming_visible_always_draws() {
        // The window's previous contents cannot be assumed to have survived.
        let mut h = Harness::idle();
        h.engine.pause();
        h.engine.resume();
        assert_eq!(h.run(), 1);

        h.engine.surface_destroyed();
        assert!(!h.engine.needs_frame());
        h.engine.surface_attached(SIZE);
        assert_eq!(h.run(), 1);
    }

    #[test]
    fn a_skipped_frame_is_still_owed() {
        let mut h = Harness::idle();
        h.script(Ok(FrameOutcome::Skipped));
        h.script(Ok(FrameOutcome::Skipped));
        h.touch(PointerPhase::Down);

        assert_eq!(h.engine.frame(), Ok(FrameOutcome::Skipped));
        assert!(
            h.engine.needs_frame(),
            "the picture never reached the display"
        );
        assert_eq!(h.engine.frame(), Ok(FrameOutcome::Skipped));
        assert!(h.engine.needs_frame());
        assert_eq!(h.engine.frame(), Ok(PRESENTED));
        assert!(!h.engine.needs_frame());
        assert_eq!(h.engine.frames_presented(), 2);
    }

    #[test]
    fn a_failed_frame_is_still_owed() {
        let mut h = Harness::idle();
        h.script(Err(GpuError::Validation));
        h.engine.request_redraw();

        assert_eq!(h.engine.frame(), Err(GpuError::Validation));
        assert!(h.engine.needs_frame());
        assert_eq!(h.run(), 1);
        assert!(!h.engine.needs_frame());
    }

    #[test]
    fn continuous_mode_draws_every_refresh_until_turned_off() {
        let mut h = Harness::idle();
        h.engine.set_continuous(true);
        for _ in 0..10 {
            assert!(h.engine.needs_frame());
            assert_eq!(h.engine.frame(), Ok(PRESENTED));
        }
        h.engine.set_continuous(false);
        // The frame already promised is drawn; it asks for no successor.
        assert_eq!(h.run(), 1);
        assert_eq!(h.engine.frames_presented(), 12);
    }

    // --- Scale and insets -------------------------------------------------

    #[test]
    fn the_first_frame_carries_what_the_host_reported_before_it() {
        let mut h = Harness::new();
        let insets = Insets::new(0.0, 96.0, 0.0, 48.0);
        assert!(h.engine.set_scale(2.75));
        assert!(h.engine.set_insets(insets));
        h.engine.resume();
        h.engine.surface_attached(SIZE);
        assert_eq!(h.run(), 1);

        let info = h.last_info();
        assert_eq!(info.scale, 2.75);
        assert_eq!(info.insets, insets);
        assert_eq!(info.size, SIZE);
        assert_eq!(info.logical_size(), Size::new(1080.0 / 2.75, 2400.0 / 2.75));
        assert_eq!(info.dp(16.0), 44.0);
        assert_eq!(
            info.safe_area(),
            morningstar_core::Rect::new(0.0, 96.0, 1080.0, 2256.0)
        );
    }

    #[test]
    fn defaults_are_a_scale_of_one_and_no_insets() {
        let h = Harness::idle();
        let info = h.last_info();
        assert_eq!(info.scale, 1.0);
        assert_eq!(info.insets, Insets::ZERO);
        assert_eq!(info.logical_size(), Size::new(1080.0, 2400.0));
    }

    #[test]
    fn a_scale_change_produces_a_frame_and_a_repeat_does_not() {
        let mut h = Harness::idle();
        assert!(h.engine.set_scale(3.0));
        assert_eq!(h.run(), 1);
        assert_eq!(h.last_info().scale, 3.0);

        assert!(!h.engine.set_scale(3.0));
        assert!(!h.engine.needs_frame());

        // The window moved to a display of another density.
        assert!(h.engine.set_scale(1.5));
        assert_eq!(h.run(), 1);
        assert_eq!(h.last_info().scale, 1.5);
        assert_eq!(h.engine.scale(), 1.5);
    }

    #[test]
    fn a_nonsensical_scale_is_ignored() {
        let mut h = Harness::idle();
        h.engine.set_scale(2.0);
        h.run();
        for bad in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert!(!h.engine.set_scale(bad), "{bad}");
            assert!(!h.engine.needs_frame(), "{bad}");
        }
        assert_eq!(h.engine.scale(), 2.0);
    }

    #[test]
    fn an_inset_change_produces_a_frame_and_a_repeat_does_not() {
        let mut h = Harness::idle();
        let portrait = Insets::new(0.0, 96.0, 0.0, 48.0);
        assert!(h.engine.set_insets(portrait));
        assert_eq!(h.run(), 1);
        assert_eq!(h.last_info().insets, portrait);

        assert!(!h.engine.set_insets(portrait));
        assert!(!h.engine.needs_frame());

        // Rotation: the cutout is now on the left.
        let landscape = Insets::new(96.0, 0.0, 0.0, 48.0);
        h.engine.surface_resized(SurfaceSize::new(2400, 1080));
        assert!(h.engine.set_insets(landscape));
        assert_eq!(h.run(), 1, "both changes are answered by one frame");
        assert_eq!(h.last_info().insets, landscape);
        assert_eq!(h.engine.insets(), landscape);
    }

    #[test]
    fn insets_from_the_host_are_sanitised() {
        let mut h = Harness::idle();
        assert!(!h.engine.set_insets(Insets::new(-4.0, f32::NAN, -0.0, 0.0)));
        assert!(h.engine.set_insets(Insets::new(-4.0, 30.0, 0.0, 0.0)));
        h.run();
        assert_eq!(h.last_info().insets, Insets::new(0.0, 30.0, 0.0, 0.0));
    }

    #[test]
    fn frame_index_counts_presented_frames() {
        let mut h = Harness::idle();
        h.script(Ok(FrameOutcome::Skipped));
        h.engine.request_redraw();
        h.run();
        let indices: Vec<u64> = h
            .probe
            .lock()
            .unwrap()
            .frames
            .iter()
            .map(|info| info.index)
            .collect();
        assert_eq!(indices, [0, 1, 1], "the skipped frame did not count");
    }

    // --- Statistics -------------------------------------------------------

    /// Long enough to be unmistakable in the statistics, short enough not
    /// to slow the test suite.
    const IDLE_GAP: Duration = Duration::from_millis(60);

    #[test]
    fn idle_time_is_not_recorded_as_a_frame_interval() {
        let mut h = Harness::idle();
        for _ in 0..3 {
            std::thread::sleep(IDLE_GAP);
            h.touch(PointerPhase::Down);
            assert_eq!(h.run(), 1);
        }

        let report = h.engine.report().expect("frames were presented");
        assert_eq!(report.frames, 4);
        assert_eq!(report.interval, None, "no two frames were back to back");
        assert_eq!(report.cpu.samples, 4, "but each frame's cost is recorded");
        assert_eq!(report.present.samples, 4);
        assert!(report.cpu.max < IDLE_GAP);
    }

    #[test]
    fn back_to_back_frames_are_measured_and_the_gap_before_them_is_not() {
        let mut h = Harness::idle();

        // Two animations of four frames, separated by an idle period.
        for _ in 0..2 {
            std::thread::sleep(IDLE_GAP);
            h.animate(3);
            h.touch(PointerPhase::Up);
            assert_eq!(h.run(), 4);
        }

        let interval = h.engine.report().unwrap().interval.unwrap();
        assert_eq!(interval.samples, 6, "three intervals per four-frame run");
        assert!(
            interval.max < IDLE_GAP,
            "an idle gap was counted as a {:?} frame",
            interval.max
        );
    }

    #[test]
    fn time_spent_hidden_is_not_recorded_as_a_frame_interval() {
        let mut h = Harness::idle();
        h.engine.set_continuous(true);
        for _ in 0..3 {
            h.engine.frame().unwrap();
        }
        h.engine.pause();
        std::thread::sleep(IDLE_GAP);
        h.engine.resume();
        for _ in 0..3 {
            h.engine.frame().unwrap();
        }

        let interval = h.engine.report().unwrap().interval.unwrap();
        assert_eq!(interval.samples, 4, "two per run of three");
        assert!(interval.max < IDLE_GAP);
    }

    #[test]
    fn report_reads_clearly_with_and_without_intervals() {
        let mut h = Harness::idle();
        let on_demand = h.engine.report().unwrap().to_string();
        assert!(
            on_demand.starts_with("frames=1 interval[n=0] cpu[n=1 "),
            "{on_demand}"
        );

        h.animate(2);
        h.engine.request_redraw();
        h.run();
        let animated = h.engine.report().unwrap().to_string();
        assert!(
            animated.starts_with("frames=4 interval[n=2 min="),
            "{animated}"
        );
    }

    // --- Timed redraw -------------------------------------------------------

    #[test]
    fn a_timed_request_does_not_draw_until_it_is_due() {
        let mut h = Harness::idle();
        h.probe.lock().unwrap().redraw_after = Some(Duration::from_secs(60));
        h.touch(PointerPhase::Down);
        assert_eq!(h.run(), 1, "the frame that makes the request");

        assert!(!h.engine.needs_frame(), "idle while the timer runs");
        let due = h.engine.redraw_due_in().expect("a timed frame is pending");
        assert!(due > Duration::from_secs(59) && due <= Duration::from_secs(60));
        assert_eq!(h.engine.frame(), Ok(FrameOutcome::Skipped));
        assert_eq!(h.engine.timed_frames(), 0);
    }

    #[test]
    fn a_timed_request_draws_one_frame_when_due() {
        let mut h = Harness::idle();
        h.probe.lock().unwrap().redraw_after = Some(Duration::from_millis(2));
        h.touch(PointerPhase::Down);
        let _ = h.engine.frame();
        std::thread::sleep(Duration::from_millis(4));
        assert_eq!(h.engine.redraw_due_in(), Some(Duration::ZERO));
        assert!(h.engine.needs_frame());

        // The frame it produces asks again (a blinking caret): one frame
        // per period, never a continuous stream.
        h.probe.lock().unwrap().redraw_after = Some(Duration::from_secs(60));
        let frames = h.engine.frames_presented();
        assert_eq!(h.run(), 1);
        assert_eq!(h.engine.frames_presented(), frames + 1);
        assert_eq!(h.engine.timed_frames(), 1);
        assert!(!h.engine.needs_frame());
        // Back-to-back intervals are for animation; a timer is not one.
        assert_eq!(h.engine.report().unwrap().interval, None);
    }

    #[test]
    fn a_timed_request_lasts_one_frame() {
        let mut h = Harness::idle();
        h.probe.lock().unwrap().redraw_after = Some(Duration::from_secs(60));
        h.touch(PointerPhase::Down);
        h.run();
        assert!(h.engine.redraw_due_in().is_some());

        // The next frame, caused by input, does not ask again.
        h.probe.lock().unwrap().redraw_after = None;
        h.touch(PointerPhase::Down);
        assert_eq!(h.run(), 1);
        assert_eq!(h.engine.redraw_due_in(), None);
        assert_eq!(h.engine.timed_frames(), 0, "input, not the timer, drew");
    }

    #[test]
    fn a_timed_request_is_dropped_while_hidden() {
        let mut h = Harness::idle();
        h.probe.lock().unwrap().redraw_after = Some(Duration::ZERO);
        h.touch(PointerPhase::Down);
        let _ = h.engine.frame();
        assert!(h.engine.needs_frame(), "due at once");

        h.engine.pause();
        assert!(!h.engine.needs_frame());
        assert_eq!(h.engine.redraw_due_in(), None, "a hidden app has no timer");
        h.engine.resume();
        assert!(h.engine.needs_frame(), "becoming visible draws anyway");
    }

    // --- Back ---------------------------------------------------------------

    #[test]
    fn a_back_the_app_handles_produces_one_frame() {
        let mut h = Harness::idle();
        h.probe.lock().unwrap().handles_back = true;
        assert!(h.engine.back());
        assert_eq!(h.probe.lock().unwrap().backs, 1);
        assert!(h.engine.needs_frame());
        assert_eq!(h.run(), 1);
        assert!(!h.engine.needs_frame());
    }

    #[test]
    fn a_back_the_app_declines_draws_nothing() {
        let mut h = Harness::idle();
        assert!(!h.engine.back());
        assert_eq!(h.probe.lock().unwrap().backs, 1, "the app was asked");
        assert!(!h.engine.needs_frame());
        assert_eq!(h.run(), 0);
    }

    #[test]
    fn an_app_that_does_not_implement_back_declines_it() {
        struct Plain;
        impl App for Plain {
            fn frame(&mut self, _info: &FrameInfo, _frame: &mut Frame) {}
        }
        let screen = Arc::new(Mutex::new(Screen::default()));
        let mut engine = Engine::with_presenter(Box::new(Plain), Box::new(FakePresenter(screen)));
        assert!(!engine.back());
    }

    #[test]
    fn a_back_while_hidden_is_answered_and_drawn_when_visible_again() {
        let mut h = Harness::idle();
        h.probe.lock().unwrap().handles_back = true;
        h.engine.pause();
        assert!(h.engine.back());
        assert!(!h.engine.needs_frame(), "nothing is drawn while hidden");
        h.engine.resume();
        assert_eq!(h.run(), 1);
    }

    #[test]
    fn the_shortest_delay_of_a_frame_wins() {
        let mut frame = Frame::new();
        frame.request_redraw_after(Duration::from_secs(5));
        frame.request_redraw_after(Duration::from_secs(1));
        frame.request_redraw_after(Duration::from_secs(9));
        assert_eq!(frame.redraw_after(), Some(Duration::from_secs(1)));
        frame.begin();
        assert_eq!(frame.redraw_after(), None);
    }

    // --- Text input session -------------------------------------------------

    fn request(session: u64, text: &str) -> TextInputRequest {
        TextInputRequest {
            session,
            traits: morningstar_core::TextInputTraits::default(),
            text: text.to_owned(),
            caret: text.len(),
            anchor: None,
        }
    }

    impl Harness {
        fn declare(&mut self, request: Option<TextInputRequest>) {
            self.probe.lock().unwrap().text = request;
            self.touch(PointerPhase::Down);
            self.run();
        }
    }

    #[test]
    fn a_selection_the_app_makes_is_an_update_and_the_hosts_own_is_not_echoed() {
        let mut h = Harness::idle();
        h.declare(Some(request(7, "habari yako")));
        assert!(h.engine.take_text_input_command().is_some());

        // The application selects a word: the host must be told.
        let selected = TextInputRequest {
            caret: 6,
            anchor: Some(0),
            ..request(7, "habari yako")
        };
        h.declare(Some(selected.clone()));
        assert_eq!(
            h.engine.take_text_input_command(),
            Some(TextInputCommand::Update(selected))
        );

        // The input method selects another: the field adopts it and
        // declares it, and nothing goes back.
        h.engine
            .text_input(TextInputEvent::Edited(morningstar_core::TextEditState {
                text: "habari yako".to_owned(),
                caret: 11,
                anchor: Some(7),
                composing: None,
            }));
        h.declare(Some(TextInputRequest {
            caret: 11,
            anchor: Some(7),
            ..request(7, "habari yako")
        }));
        assert_eq!(h.engine.take_text_input_command(), None);

        // A collapsed selection from the host is no selection.
        h.engine
            .text_input(TextInputEvent::Edited(morningstar_core::TextEditState {
                text: "habari yako".to_owned(),
                caret: 3,
                anchor: Some(3),
                composing: None,
            }));
        h.declare(Some(TextInputRequest {
            caret: 3,
            ..request(7, "habari yako")
        }));
        assert_eq!(h.engine.take_text_input_command(), None);
    }

    // --- System bars and reduce motion --------------------------------------

    #[test]
    fn the_host_is_told_about_the_system_bars_only_when_they_change() {
        let mut h = Harness::idle();
        assert_eq!(h.engine.take_system_bars(), None, "no opinion yet");

        h.probe.lock().unwrap().bars = Some(SystemBars::DARK_CONTENT);
        h.touch(PointerPhase::Down);
        h.run();
        assert_eq!(h.engine.take_system_bars(), Some(SystemBars::DARK_CONTENT));
        assert_eq!(h.engine.take_system_bars(), None, "told once");
        h.touch(PointerPhase::Down);
        h.run();
        assert_eq!(h.engine.take_system_bars(), None, "the same again");

        h.probe.lock().unwrap().bars = Some(SystemBars::LIGHT_CONTENT);
        h.touch(PointerPhase::Down);
        h.run();
        assert_eq!(h.engine.take_system_bars(), Some(SystemBars::LIGHT_CONTENT));

        // A frame that says nothing leaves the bars as they are.
        h.probe.lock().unwrap().bars = None;
        h.touch(PointerPhase::Down);
        h.run();
        assert_eq!(h.engine.take_system_bars(), None);
    }

    #[test]
    fn reduce_motion_reaches_the_app_and_a_change_produces_one_frame() {
        let mut h = Harness::idle();
        assert!(!h.probe.lock().unwrap().frames.last().unwrap().reduce_motion);
        assert!(h.engine.set_reduce_motion(true));
        assert!(h.engine.needs_frame());
        assert_eq!(h.run(), 1);
        assert!(h.probe.lock().unwrap().frames.last().unwrap().reduce_motion);
        // Repeating the value costs nothing.
        assert!(!h.engine.set_reduce_motion(true));
        assert!(!h.engine.needs_frame());
        assert!(h.engine.set_reduce_motion(false));
        assert_eq!(h.run(), 1);
        assert!(!h.probe.lock().unwrap().frames.last().unwrap().reduce_motion);
    }

    #[test]
    fn focus_shows_the_keyboard_once_and_blur_hides_it() {
        let mut h = Harness::idle();
        assert_eq!(h.engine.take_text_input_command(), None);

        h.declare(Some(request(7, "")));
        assert_eq!(
            h.engine.take_text_input_command(),
            Some(TextInputCommand::Show(request(7, "")))
        );
        // Further frames with the same field say nothing new.
        h.touch(PointerPhase::Down);
        h.run();
        assert_eq!(h.engine.take_text_input_command(), None);

        h.declare(None);
        assert_eq!(
            h.engine.take_text_input_command(),
            Some(TextInputCommand::Hide)
        );
        assert_eq!(h.engine.take_text_input_command(), None);
    }

    #[test]
    fn another_field_or_other_traits_restart_the_keyboard() {
        let mut h = Harness::idle();
        h.declare(Some(request(1, "a")));
        h.engine.take_text_input_command();

        h.declare(Some(request(2, "b")));
        assert_eq!(
            h.engine.take_text_input_command(),
            Some(TextInputCommand::Show(request(2, "b")))
        );

        let mut secure = request(2, "b");
        secure.traits.secure = true;
        h.declare(Some(secure.clone()));
        assert_eq!(
            h.engine.take_text_input_command(),
            Some(TextInputCommand::Show(secure))
        );
    }

    #[test]
    fn the_hosts_own_edits_are_not_sent_back_to_it() {
        let mut h = Harness::idle();
        h.declare(Some(request(1, "")));
        h.engine.take_text_input_command();

        // The input method typed "hi"; the app adopts it.
        h.engine
            .text_input(TextInputEvent::Edited(morningstar_core::TextEditState {
                text: "hi".to_owned(),
                caret: 2,
                anchor: None,
                composing: Some((0, 2)),
            }));
        assert!(h.engine.needs_frame(), "text input produces a frame");
        h.probe.lock().unwrap().text = Some(request(1, "hi"));
        h.run();
        assert_eq!(h.engine.take_text_input_command(), None);
        assert_eq!(h.probe.lock().unwrap().text_events.len(), 1);
    }

    #[test]
    fn an_edit_the_app_changes_is_corrected_on_the_host() {
        let mut h = Harness::idle();
        h.declare(Some(request(1, "12")));
        h.engine.take_text_input_command();

        // The input method typed a letter; the app keeps digits only.
        h.engine
            .text_input(TextInputEvent::Edited(morningstar_core::TextEditState {
                text: "12x".to_owned(),
                caret: 3,
                anchor: None,
                composing: None,
            }));
        h.run();
        assert_eq!(
            h.engine.take_text_input_command(),
            Some(TextInputCommand::Update(request(1, "12")))
        );
        assert_eq!(h.engine.take_text_input_command(), None);
    }

    #[test]
    fn an_edit_made_in_the_app_updates_the_host() {
        let mut h = Harness::idle();
        h.declare(Some(request(1, "ab")));
        h.engine.take_text_input_command();

        // A hardware key: the app deletes, the host's copy is now stale.
        h.engine.text_input(TextInputEvent::DeleteBackward);
        h.probe.lock().unwrap().text = Some(request(1, "a"));
        h.run();
        assert_eq!(
            h.engine.take_text_input_command(),
            Some(TextInputCommand::Update(request(1, "a")))
        );
    }

    #[test]
    fn a_keyboard_the_user_dismissed_is_not_hidden_again() {
        let mut h = Harness::idle();
        h.declare(Some(request(1, "")));
        h.engine.take_text_input_command();

        h.engine.text_input(TextInputEvent::FocusLost);
        // The app gives up focus in response.
        h.probe.lock().unwrap().text = None;
        h.run();
        assert_eq!(h.engine.take_text_input_command(), None);

        // An app that keeps the field focused gets its keyboard back.
        h.declare(Some(request(1, "")));
        assert_eq!(
            h.engine.take_text_input_command(),
            Some(TextInputCommand::Show(request(1, "")))
        );
    }

    #[test]
    fn the_keyboard_inset_reaches_the_app_and_draws_once_per_change() {
        let mut h = Harness::idle();
        assert!(h.engine.set_keyboard_inset(900.0));
        assert_eq!(h.run(), 1);
        assert_eq!(h.last_info().keyboard_inset, 900.0);

        assert!(!h.engine.set_keyboard_inset(900.0), "a repeat is free");
        assert!(!h.engine.needs_frame());

        assert!(h.engine.set_keyboard_inset(f32::NAN), "NaN is zero");
        assert_eq!(h.engine.keyboard_inset(), 0.0);
        assert_eq!(h.run(), 1);
        assert_eq!(h.last_info().keyboard_inset, 0.0);
    }

    // --- Platform services (ADR 0019) --------------------------------------

    fn get(id: u64) -> ServiceCall {
        ServiceCall {
            id: ServiceId(id),
            request: ServiceRequest::Http(HttpRequest::get("https://example.com")),
        }
    }

    fn ok(status: u16) -> ServiceResult {
        ServiceResult::Http(Ok(HttpResponse::new(status)))
    }

    impl Harness {
        fn ask(&self, call: ServiceCall) {
            self.probe.lock().unwrap().outbox.push(call);
        }

        fn take(&mut self) -> Vec<ServiceCall> {
            let mut out = Vec::new();
            self.engine.take_service_requests(&mut out);
            out
        }
    }

    #[test]
    fn an_app_that_asks_for_nothing_hands_the_host_nothing() {
        let mut h = Harness::idle();
        assert!(h.take().is_empty());
        assert_eq!(h.run(), 0, "asking is not a reason to draw");
    }

    #[test]
    fn requests_reach_the_host_once_and_in_the_order_they_were_made() {
        let mut h = Harness::idle();
        h.ask(get(1));
        h.ask(ServiceCall {
            id: ServiceId(2),
            request: ServiceRequest::StartRecording,
        });
        let taken = h.take();
        assert_eq!(
            taken.iter().map(|call| call.id.0).collect::<Vec<_>>(),
            [1, 2]
        );
        assert!(h.take().is_empty(), "taken means taken");
        assert_eq!(h.run(), 0, "a request in flight draws nothing");
    }

    #[test]
    fn an_answer_the_app_is_waiting_for_produces_exactly_one_frame() {
        let mut h = Harness::idle();
        h.ask(get(7));
        h.take();
        assert!(!h.engine.needs_frame(), "idle while the request is out");
        assert!(h.engine.service_result(ServiceId(7), ok(200)));
        assert!(h.engine.needs_frame());
        assert_eq!(h.run(), 1);
        assert!(!h.engine.needs_frame(), "and idle again");
        assert_eq!(h.probe.lock().unwrap().results, [(ServiceId(7), ok(200))]);
    }

    #[test]
    fn an_answer_nobody_is_waiting_for_changes_nothing() {
        let mut h = Harness::idle();
        assert!(!h.engine.service_result(ServiceId(99), ok(200)));
        assert!(!h.engine.needs_frame());
        assert_eq!(h.run(), 0);
        assert!(h.probe.lock().unwrap().results.is_empty());
    }

    #[test]
    fn a_second_answer_to_the_same_request_is_dropped() {
        let mut h = Harness::idle();
        h.ask(get(3));
        h.take();
        assert!(h.engine.service_result(ServiceId(3), ok(200)));
        assert_eq!(h.run(), 1);
        assert!(!h.engine.service_result(ServiceId(3), ok(500)));
        assert_eq!(h.run(), 0);
        assert_eq!(h.probe.lock().unwrap().results.len(), 1);
    }

    #[test]
    fn answers_are_delivered_in_the_order_the_host_gives_them() {
        let mut h = Harness::idle();
        h.ask(get(1));
        h.ask(get(2));
        h.take();
        // The second request finished first.
        h.engine.service_result(ServiceId(2), ok(202));
        h.engine.service_result(ServiceId(1), ok(201));
        assert_eq!(h.run(), 1, "two answers before a frame share that frame");
        assert_eq!(
            h.probe.lock().unwrap().results,
            [(ServiceId(2), ok(202)), (ServiceId(1), ok(201))]
        );
    }

    #[test]
    fn an_answer_while_hidden_is_delivered_at_once_and_drawn_on_return() {
        let mut h = Harness::idle();
        h.ask(get(5));
        h.take();
        h.engine.pause();
        assert!(h.engine.service_result(ServiceId(5), ok(200)));
        assert_eq!(h.probe.lock().unwrap().results.len(), 1, "not held back");
        assert_eq!(h.run(), 0, "nothing is drawn while hidden");
        h.engine.resume();
        assert_eq!(h.run(), 1);
    }

    #[test]
    fn a_request_made_in_response_to_an_answer_is_there_for_the_host_to_take() {
        let mut h = Harness::idle();
        h.ask(get(1));
        h.take();
        h.probe.lock().unwrap().follow_up = Some(get(2));
        h.engine.service_result(ServiceId(1), ok(200));
        let taken = h.take();
        assert_eq!(taken.len(), 1);
        assert_eq!(taken[0].id, ServiceId(2));
    }

    #[test]
    fn requests_made_during_setup_are_there_before_the_first_frame() {
        let mut h = Harness::new();
        h.ask(get(1));
        assert_eq!(h.take().len(), 1);
        assert_eq!(h.app_frames(), 0);
    }

    #[test]
    fn without_a_host_setup_sees_nothing_stored_and_the_placeholder_id() {
        let h = Harness::new();
        let probe = h.probe.lock().unwrap();
        assert_eq!(probe.install_id, morningstar_core::NO_INSTALL_ID);
        assert_eq!(probe.stored_device_id, None);
    }

    #[test]
    fn setup_sees_what_the_host_stored_and_its_install_id() {
        let probe = Arc::new(Mutex::new(Probe::default()));
        let screen = Arc::new(Mutex::new(Screen::default()));
        let host = HostState::new()
            .with_install_id("00112233445566778899aabbccddeeff")
            .with_stored("device_id", "abc");
        let _engine = Engine::with_presenter_and_host(
            Box::new(ProbeApp(probe.clone())),
            Box::new(FakePresenter(screen)),
            &host,
        );
        let probe = probe.lock().unwrap();
        assert_eq!(probe.install_id, "00112233445566778899aabbccddeeff");
        assert_eq!(probe.stored_device_id.as_deref(), Some("abc"));
    }

    #[test]
    fn an_app_without_services_ignores_answers() {
        struct Plain;
        impl App for Plain {
            fn frame(&mut self, _info: &FrameInfo, _frame: &mut Frame) {}
        }
        let screen = Arc::new(Mutex::new(Screen::default()));
        let mut engine = Engine::with_presenter(Box::new(Plain), Box::new(FakePresenter(screen)));
        let mut out = Vec::new();
        engine.take_service_requests(&mut out);
        assert!(out.is_empty());
        assert!(!engine.service_result(ServiceId(1), ok(200)));
    }
}

/// Images and renderer follow-up requests, as the engine schedules them.
/// Separate from `tests` above so its fakes can record what those do not.
#[cfg(test)]
mod image_tests {
    use super::*;
    use morningstar_core::{Image, ImageError, ImageId, PointerPhase, Rect};
    use morningstar_gpu::FrameTimings;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    const SIZE: SurfaceSize = SurfaceSize::new(320, 480);
    const RED: [u8; 4] = [255, 0, 0, 255];
    const PRESENTED: FrameOutcome = FrameOutcome::Presented(FrameTimings {
        acquire: Duration::ZERO,
        encode: Duration::from_micros(300),
        present: Duration::from_micros(200),
    });

    /// What the app does on its next frames, and what it holds.
    #[derive(Default)]
    struct Script {
        frames: u32,
        /// Registered in `setup`.
        logo: Option<Image>,
        /// Registered during a frame when `load_photo` is set.
        photo: Option<Image>,
        load_photo: bool,
        release_photo: bool,
        /// What a bad PNG produced.
        bad_png: Option<ImageError>,
    }

    struct Gallery(Arc<Mutex<Script>>);

    impl App for Gallery {
        fn setup(&mut self, setup: &mut crate::Setup<'_>) {
            let mut script = self.0.lock().unwrap();
            script.logo = Some(setup.add_image_rgba8(1, 1, &RED).unwrap());
            script.bad_png = setup.add_image_png(b"not a png").err();
        }

        fn frame(&mut self, _info: &FrameInfo, frame: &mut Frame) {
            let mut script = self.0.lock().unwrap();
            script.frames += 1;
            if std::mem::take(&mut script.load_photo) {
                script.photo = Some(frame.add_image_rgba8(2, 1, &[RED, RED].concat()).unwrap());
            }
            if std::mem::take(&mut script.release_photo) {
                let photo = script.photo.expect("a photo to release");
                assert!(frame.remove_image(photo));
                assert!(!frame.remove_image(photo), "twice is harmless");
                assert!(!frame.has_image(photo));
            }
            let rect = Rect::new(0.0, 0.0, 8.0, 8.0);
            for image in [script.logo, script.photo].into_iter().flatten() {
                // Including a released photo: drawing it must be harmless.
                frame.list.draw_image(image, rect);
            }
        }
    }

    /// What the presenter was handed for one frame.
    #[derive(Clone, Debug, PartialEq)]
    struct Seen {
        image_ops: usize,
        /// Of the images the list draws, the ids that are registered.
        registered: Vec<ImageId>,
        generation: u64,
    }

    #[derive(Default)]
    struct Screen {
        seen: Vec<Seen>,
        script: VecDeque<Result<FrameOutcome, GpuError>>,
        /// Answers to `take_frame_request`, one per presented frame.
        follow_ups: VecDeque<bool>,
        /// Whether the last frame handed over was presented.
        presented_last: bool,
        without_images: u32,
    }

    struct FakePresenter(Arc<Mutex<Screen>>);

    impl Presenter for FakePresenter {
        unsafe fn attach_surface(
            &mut self,
            _native: NativeSurface,
            _size: SurfaceSize,
        ) -> Result<(), GpuError> {
            Ok(())
        }

        fn detach_surface(&mut self) {}

        fn resize(&mut self, _size: SurfaceSize) {}

        fn render(
            &mut self,
            _clear: Color,
            _list: &DisplayList,
            _atlas: AtlasView<'_>,
        ) -> Result<FrameOutcome, GpuError> {
            self.0.lock().unwrap().without_images += 1;
            Ok(PRESENTED)
        }

        fn render_with_images(
            &mut self,
            _clear: Color,
            list: &DisplayList,
            _atlas: AtlasView<'_>,
            images: ImagesView<'_>,
        ) -> Result<FrameOutcome, GpuError> {
            let drawn: Vec<ImageId> = list
                .ops()
                .iter()
                .filter_map(|op| match op {
                    morningstar_core::DrawOp::Image { image, .. } => Some(image.id()),
                    _ => None,
                })
                .collect();
            let mut screen = self.0.lock().unwrap();
            screen.seen.push(Seen {
                image_ops: drawn.len(),
                registered: drawn
                    .into_iter()
                    .filter(|&id| images.contains(id))
                    .collect(),
                generation: images.generation(),
            });
            let outcome = screen.script.pop_front().unwrap_or(Ok(PRESENTED));
            screen.presented_last = outcome == Ok(PRESENTED);
            outcome
        }

        fn take_frame_request(&mut self) -> bool {
            let mut screen = self.0.lock().unwrap();
            assert!(screen.presented_last, "asked only after a presented frame");
            screen.follow_ups.pop_front().unwrap_or(false)
        }

        fn adapter_info(&self) -> Option<&AdapterInfo> {
            None
        }
    }

    struct Harness {
        engine: Engine,
        app: Arc<Mutex<Script>>,
        screen: Arc<Mutex<Screen>>,
    }

    impl Harness {
        /// A visible engine that has drawn its first frame and gone idle.
        fn idle() -> Self {
            let app = Arc::new(Mutex::new(Script::default()));
            let screen = Arc::new(Mutex::new(Screen::default()));
            let mut engine = Engine::with_presenter(
                Box::new(Gallery(app.clone())),
                Box::new(FakePresenter(screen.clone())),
            );
            engine.resume();
            engine.surface_attached(SIZE);
            let mut harness = Self {
                engine,
                app,
                screen,
            };
            assert_eq!(harness.run(), 1, "the first frame");
            harness
        }

        /// Calls `frame` while the engine needs one, as a host does.
        /// Returns how many were presented.
        fn run(&mut self) -> u32 {
            let before = self.engine.frames_presented();
            for _ in 0..100 {
                if !self.engine.needs_frame() {
                    return u32::try_from(self.engine.frames_presented() - before).unwrap();
                }
                let _ = self.engine.frame();
            }
            panic!("the engine never went idle");
        }

        fn touch(&mut self) {
            self.engine.pointer(PointerEvent {
                id: 1,
                phase: PointerPhase::Down,
                position: morningstar_core::Point::new(1.0, 2.0),
            });
        }

        fn last_seen(&self) -> Seen {
            self.screen.lock().unwrap().seen.last().unwrap().clone()
        }

        fn app_frames(&self) -> u32 {
            self.app.lock().unwrap().frames
        }
    }

    #[test]
    fn an_image_registered_in_setup_reaches_the_first_frame() {
        let h = Harness::idle();
        let logo = h.app.lock().unwrap().logo.expect("registered in setup");
        assert_eq!((logo.width(), logo.height()), (1, 1));
        let seen = h.last_seen();
        assert_eq!(seen.image_ops, 1);
        assert_eq!(seen.registered, [logo.id()]);
        assert_eq!(
            h.screen.lock().unwrap().without_images,
            0,
            "the engine always hands the images over"
        );
    }

    #[test]
    fn a_bad_png_is_an_error_the_app_can_handle() {
        let h = Harness::idle();
        let error = h.app.lock().unwrap().bad_png.clone();
        assert!(matches!(error, Some(ImageError::Decode(_))), "{error:?}");
    }

    #[test]
    fn an_image_registered_during_a_frame_is_drawn_by_that_frame() {
        let mut h = Harness::idle();
        let before = h.last_seen().generation;
        h.app.lock().unwrap().load_photo = true;
        h.touch();

        // One frame: the registration needs no frame of its own, because
        // the frame that made it already carries the image to the renderer.
        assert_eq!(h.run(), 1);
        assert_eq!(h.app_frames(), 2);
        let photo = h.app.lock().unwrap().photo.unwrap();
        let seen = h.last_seen();
        assert_eq!(seen.image_ops, 2);
        assert!(seen.registered.contains(&photo.id()));
        assert_ne!(
            seen.generation, before,
            "the renderer can tell the set changed"
        );
        assert!(!h.engine.needs_frame(), "and the engine is idle again");
    }

    #[test]
    fn a_release_reaches_the_renderer_with_the_frame_that_made_it() {
        let mut h = Harness::idle();
        h.app.lock().unwrap().load_photo = true;
        h.touch();
        h.run();
        let photo = h.app.lock().unwrap().photo.unwrap();
        let before = h.last_seen().generation;

        h.app.lock().unwrap().release_photo = true;
        h.touch();
        assert_eq!(h.run(), 1);
        // The app still drew the stale handle; it is simply not registered
        // any more, which is how the renderer learns to free its texture.
        let seen = h.last_seen();
        assert_eq!(seen.image_ops, 2);
        assert!(!seen.registered.contains(&photo.id()));
        assert_ne!(seen.generation, before);
        assert!(!h.engine.needs_frame());

        // Nothing changes on later frames.
        h.touch();
        h.run();
        assert_eq!(h.last_seen().generation, seen.generation);
    }

    #[test]
    fn a_release_whose_frame_was_not_shown_is_delivered_again() {
        let mut h = Harness::idle();
        h.app.lock().unwrap().load_photo = true;
        h.touch();
        h.run();
        let photo = h.app.lock().unwrap().photo.unwrap();

        // The frame carrying the release is skipped, then fails. The engine
        // keeps owing a frame, so the renderer is given the smaller image
        // set again until a frame goes through and the texture can go.
        h.screen
            .lock()
            .unwrap()
            .script
            .extend([Ok(FrameOutcome::Skipped), Err(GpuError::Validation)]);
        h.app.lock().unwrap().release_photo = true;
        h.touch();
        assert_eq!(h.run(), 1);
        let screen = h.screen.lock().unwrap();
        let last_three = &screen.seen[screen.seen.len() - 3..];
        for seen in last_three {
            assert!(!seen.registered.contains(&photo.id()));
        }
        assert_eq!(last_three[0].generation, last_three[2].generation);
    }

    #[test]
    fn a_follow_up_request_from_the_renderer_produces_one_more_frame() {
        let mut h = Harness::idle();
        // The swapchain was suboptimal when the next frame was presented.
        h.screen.lock().unwrap().follow_ups.extend([true]);
        h.touch();

        assert_eq!(h.engine.frame(), Ok(PRESENTED));
        assert!(
            h.engine.needs_frame(),
            "nothing else would draw into the rebuilt swapchain"
        );
        assert_eq!(h.engine.frame(), Ok(PRESENTED));
        assert!(!h.engine.needs_frame(), "one more, not a loop");
        assert_eq!(h.app_frames(), 3);
        assert_eq!(h.engine.frames_presented(), 3);
        assert_eq!(h.engine.frame(), Ok(FrameOutcome::Skipped));

        // The two frames were back to back, so their interval is a real one.
        let interval = h.engine.report().unwrap().interval.unwrap();
        assert_eq!(interval.samples, 1);
    }

    #[test]
    fn the_renderer_is_asked_only_about_presented_frames() {
        let mut h = Harness::idle();
        {
            let mut screen = h.screen.lock().unwrap();
            screen.script.extend([Ok(FrameOutcome::Skipped)]);
            // Would be consumed, wrongly, by the skipped frame.
            screen.follow_ups.extend([true]);
        }
        h.touch();
        // Skipped (still owed), presented (asks for a follow-up), follow-up.
        // `take_frame_request` itself asserts it is not called after the
        // skipped one.
        assert_eq!(h.run(), 2);
        assert_eq!(h.app_frames(), 4);
        assert!(!h.engine.needs_frame());
    }

    #[test]
    fn without_a_request_a_frame_is_followed_by_nothing() {
        let mut h = Harness::idle();
        h.touch();
        assert_eq!(h.run(), 1);
        assert_eq!(h.engine.report().unwrap().interval, None);
    }
}
