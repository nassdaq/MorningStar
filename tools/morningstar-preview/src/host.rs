//! The host: one window on the Mac, driving one [`Engine`].
//!
//! It is to the Mac what `MorningstarActivity` is to Android and the UIKit
//! host to iOS (ADR 0003): it owns the native window, translates its
//! events into engine calls, draws when the engine says a frame is needed
//! and sleeps otherwise, and carries out the app's service requests. The
//! app cannot tell it from a phone, except by what it answers
//! `Unsupported`.
//!
//! The generated runner calls [`run`] with the app's constructor.

use std::process::ExitCode;

use morningstar_runtime::App;

/// The exit code with which the host asks the supervisor to start it
/// again (the restart keys).
pub const RESTART_EXIT_CODE: u8 = 75;

/// Runs the app in a window until the window is closed. Reads its options
/// from the command line (see [`crate::options`]).
#[cfg(target_os = "macos")]
pub fn run(create: fn() -> Box<dyn App>) -> ExitCode {
    mac::run(create)
}

/// The preview's window is the Mac's; there is no host here.
#[cfg(not(target_os = "macos"))]
pub fn run(_create: fn() -> Box<dyn App>) -> ExitCode {
    eprintln!("preview: the preview's window host is written for macOS only");
    ExitCode::FAILURE
}

#[cfg(target_os = "macos")]
mod mac {
    use std::io::BufRead;
    use std::path::{Path, PathBuf};
    use std::process::ExitCode;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use morningstar_gpu::render_offscreen_with_images;
    use morningstar_runtime::{
        App, Engine, FrameOutcome, Insets, NativeSurface, Point, PointerEvent, PointerPhase,
        ServiceCall, ServiceId, ServiceResult, SurfaceSize, TextInputCommand, TextInputEvent,
    };
    use winit::application::ApplicationHandler;
    use winit::dpi::{LogicalPosition, LogicalSize};
    use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
    use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
    use winit::keyboard::{Key as WinitKey, ModifiersState, NamedKey};
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winit::window::{Window, WindowId};

    use super::RESTART_EXIT_CODE;
    use crate::input::{self, Key, KeyAction, Modifiers, Mouse, WHEEL_LINE_DP, Wheel};
    use crate::options::{Edges, Fit, Options, STEP_PAUSE_MS, Step, USAGE, fit};
    use crate::services::{Answer, Services};
    use crate::store::Store;

    /// What reaches the window's thread from the others.
    enum Wake {
        /// The answer to a service request.
        Answer(ServiceId, ServiceResult),
        /// The supervisor says how the build stands; empty for "fine".
        Status(String),
        /// The supervisor is replacing this app, or is gone.
        Quit,
    }

    /// How long to wait before trying a frame that could not be shown.
    const RETRY_AFTER: Duration = Duration::from_millis(16);
    /// How long after a frame failed with an error.
    const RETRY_AFTER_ERROR: Duration = Duration::from_millis(250);
    /// The display's menu bar, the window's title bar and some air: what
    /// of the display's height a window's content cannot have, in points.
    const DISPLAY_MARGIN: f64 = 110.0;

    pub fn run(create: fn() -> Box<dyn App>) -> ExitCode {
        let options = match Options::parse(std::env::args().skip(1)) {
            Ok(options) => options,
            Err(error) => {
                eprintln!("preview: {error}");
                return ExitCode::from(2);
            }
        };
        if options.help {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        let store = match &options.state_dir {
            Some(dir) => match Store::open(dir) {
                Ok(store) => store,
                Err(error) => {
                    eprintln!("preview: {} cannot be used: {error}", dir.display());
                    return ExitCode::FAILURE;
                }
            },
            None => Store::in_memory(),
        };

        let event_loop = match EventLoop::<Wake>::with_user_event().build() {
            Ok(event_loop) => event_loop,
            Err(error) => {
                eprintln!("preview: no window system: {error}");
                return ExitCode::FAILURE;
            }
        };

        let proxy = event_loop.create_proxy();
        let answer: Answer = {
            let proxy = proxy.clone();
            Arc::new(move |id, result| {
                // Refused only when the window is gone, and with it whoever
                // was waiting for the answer.
                let _ = proxy.send_event(Wake::Answer(id, result));
            })
        };
        if options.supervised {
            listen_to_supervisor(proxy);
        }

        let mut host = Host::new(options, create, Services::new(store, answer));
        if let Err(error) = event_loop.run_app(&mut host) {
            eprintln!("preview: the window system stopped: {error}");
            return ExitCode::FAILURE;
        }
        ExitCode::from(host.exit_code)
    }

    /// Reads the supervisor's lines from standard input: `status <text>`
    /// and `quit`. The end of the input means the supervisor is gone, and
    /// a preview nobody supervises closes rather than linger as a window
    /// of an old build.
    fn listen_to_supervisor(proxy: EventLoopProxy<Wake>) {
        let spawned = std::thread::Builder::new()
            .name("preview supervisor".to_owned())
            .spawn(move || {
                for line in std::io::stdin().lock().lines() {
                    let Ok(line) = line else { break };
                    let wake = match line.split_once(' ') {
                        Some(("status", text)) => Wake::Status(text.trim().to_owned()),
                        _ if line.trim() == "status" => Wake::Status(String::new()),
                        _ if line.trim() == "quit" => Wake::Quit,
                        _ => continue,
                    };
                    if proxy.send_event(wake).is_err() {
                        return;
                    }
                }
                let _ = proxy.send_event(Wake::Quit);
            });
        if let Err(error) = spawned {
            eprintln!("preview: cannot listen to the supervisor: {error}");
        }
    }

    struct Host {
        options: Options,
        create: fn() -> Box<dyn App>,
        services: Services,
        calls: Vec<ServiceCall>,

        // The engine is declared before the window and taken apart before
        // it in `shutdown`: its surface draws into the window's view.
        engine: Option<Engine>,
        window: Option<Window>,

        /// Window points per dp.
        zoom: f64,
        /// Physical pixels per dp: what the engine was told.
        scale: f32,
        insets_on: bool,
        reduce_motion: bool,

        mouse: Mouse,
        wheel: Wheel,
        modifiers: Modifiers,
        /// A text field has the keyboard: on a phone, the keyboard is up.
        field_focused: bool,

        /// Not before this: a frame could not be shown.
        retry_at: Option<Instant>,
        frame_error_reported: bool,
        announced: bool,
        screenshot_at: Option<Instant>,
        /// The scripted steps still to do, and when the next one is due
        /// (set when the first frame has been drawn).
        script: std::collections::VecDeque<Step>,
        step_at: Option<Instant>,
        /// A scripted tap's finger is down; it lifts then, there.
        lift_at: Option<(Instant, Point)>,
        shots: u32,
        status: String,
        position: Option<(i32, i32)>,
        position_saved_at: Option<Instant>,
        exit_code: u8,
        /// `MORNINGSTAR_PREVIEW_DEBUG` is set: say what the window system
        /// says.
        debug: bool,
    }

    impl Host {
        fn new(options: Options, create: fn() -> Box<dyn App>, services: Services) -> Self {
            let screenshot_at = options
                .screenshot_after
                .as_ref()
                .map(|(after, _)| Instant::now() + Duration::from_millis(*after));
            let options_script = options.script.iter().cloned().collect();
            Self {
                insets_on: true,
                reduce_motion: options.reduce_motion,
                options,
                create,
                services,
                calls: Vec::new(),
                engine: None,
                window: None,
                zoom: 1.0,
                scale: 1.0,
                mouse: Mouse::default(),
                wheel: Wheel::default(),
                modifiers: Modifiers::default(),
                field_focused: false,
                retry_at: None,
                frame_error_reported: false,
                announced: false,
                screenshot_at,
                script: options_script,
                step_at: None,
                lift_at: None,
                shots: 0,
                status: String::new(),
                position: None,
                position_saved_at: None,
                exit_code: 0,
                debug: std::env::var_os("MORNINGSTAR_PREVIEW_DEBUG").is_some(),
            }
        }

        fn title(&self) -> String {
            let name = self.options.title.as_deref().unwrap_or("Morningstar app");
            if self.status.is_empty() {
                format!("{name} \u{2014} preview")
            } else {
                format!("{name} \u{2014} {}", self.status)
            }
        }

        fn insets(&self) -> Insets {
            let edges = if self.insets_on {
                self.options.insets
            } else {
                Edges::ZERO
            };
            Insets::new(
                edges.left * self.scale,
                edges.top * self.scale,
                edges.right * self.scale,
                edges.bottom * self.scale,
            )
        }

        /// Makes the window and starts the app in it.
        fn open(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
            let monitor = event_loop
                .primary_monitor()
                .or_else(|| event_loop.available_monitors().next());
            let (factor, available) = match &monitor {
                Some(monitor) => {
                    let factor = monitor.scale_factor();
                    let height = f64::from(monitor.size().height) / factor;
                    (factor, Some(height - DISPLAY_MARGIN))
                }
                None => (1.0, None),
            };
            // A first guess, from the main display; corrected below for the
            // display the window lands on.
            let Fit { points, .. } = fit(self.options.size, self.options.scale, factor, available);

            let position = self
                .options
                .position
                .or_else(|| self.services.store().window_position());
            let attributes = Window::default_attributes()
                .with_title(self.title())
                .with_inner_size(LogicalSize::new(points.0, points.1))
                .with_active(!self.options.background)
                .with_visible(false);
            let window = event_loop
                .create_window(attributes)
                .map_err(|error| format!("the window could not be made: {error}"))?;
            // Placed after it is made and shown only then. Places are kept
            // in the window system's points, which mean the same on every
            // display; pixels do not when displays differ in density.
            if let Some((x, y)) = position {
                window.set_outer_position(LogicalPosition::new(f64::from(x), f64::from(y)));
            }

            // The window is where it last was, or where the system put it,
            // which need not be the main display: the display it is on is
            // the one whose pixels and height count.
            let available = window.current_monitor().map_or(available, |monitor| {
                Some(f64::from(monitor.size().height) / monitor.scale_factor() - DISPLAY_MARGIN)
            });
            if self.debug {
                for monitor in event_loop.available_monitors() {
                    eprintln!(
                        "preview: display {:?} {:?} at {:?} factor {}",
                        monitor.name(),
                        monitor.size(),
                        monitor.position(),
                        monitor.scale_factor()
                    );
                }
                eprintln!(
                    "preview: window on {:?}, factor {}, outer {:?}, inner {:?}, available {available:?}",
                    window.current_monitor().and_then(|monitor| monitor.name()),
                    window.scale_factor(),
                    window.outer_position(),
                    window.inner_size(),
                );
            }
            let fitted = fit(
                self.options.size,
                self.options.scale,
                window.scale_factor(),
                available,
            );
            if fitted.points != points {
                let _ =
                    window.request_inner_size(LogicalSize::new(fitted.points.0, fitted.points.1));
            }
            self.zoom = fitted.points.0 / f64::from(self.options.size.0);
            self.scale = fitted.scale;
            if fitted.shrunk {
                eprintln!(
                    "preview: {}x{} dp is taller than this display; drawn at {:.2} px per dp \
                     (the layout is the phone's, the pixels are fewer). --scale sets it.",
                    self.options.size.0, self.options.size.1, self.scale
                );
            }

            let host_state = self.services.store().host_state();
            let mut engine = Engine::with_host_state((self.create)(), &host_state);
            engine.set_scale(self.scale);
            engine.set_insets(self.insets());
            engine.set_reduce_motion(self.reduce_motion);

            let size = window.inner_size();
            let view = match window
                .window_handle()
                .map_err(|error| format!("the window has no handle: {error}"))?
                .as_raw()
            {
                RawWindowHandle::AppKit(handle) => handle.ns_view,
                other => return Err(format!("not a Mac window: {other:?}")),
            };
            // SAFETY: `view` is the view of `window`, which is alive. The
            // engine is taken apart in `shutdown` before the window is
            // dropped, and this runs on the main thread, as windows do.
            unsafe {
                engine.surface_created(
                    NativeSurface::AppKitView(view),
                    SurfaceSize::new(size.width, size.height),
                )
            }
            .map_err(|error| format!("the GPU surface could not be made: {error}"))?;
            engine.resume();

            window.set_visible(true);
            self.engine = Some(engine);
            self.window = Some(window);
            self.after_engine_call();
            Ok(())
        }

        /// Makes the window as many points as give a screen of `size` dp
        /// exactly `scale` pixels per dp on a display of `factor` pixels
        /// per point: a scale that was asked for holds on any display.
        fn keep_scale(&mut self, window: &Window, scale: f32, factor: f64, size: (f32, f32)) {
            self.zoom = f64::from(scale) / factor;
            self.scale = scale;
            let _ = window.request_inner_size(LogicalSize::new(
                f64::from(size.0) * self.zoom,
                f64::from(size.1) * self.zoom,
            ));
        }

        /// What a host does after every call into the engine: carry out
        /// what the app asked of the platform, and follow its keyboard.
        fn after_engine_call(&mut self) {
            let Some(engine) = self.engine.as_mut() else {
                return;
            };
            while let Some(command) = engine.take_text_input_command() {
                self.field_focused = !matches!(command, TextInputCommand::Hide);
                let keyboard = if self.field_focused {
                    self.options.keyboard * self.scale
                } else {
                    0.0
                };
                engine.set_keyboard_inset(keyboard);
            }
            // The Mac's menu bar is not the app's status bar: nothing to do
            // with how the app wants its bars, but it must be taken.
            let _ = engine.take_system_bars();

            engine.take_service_requests(&mut self.calls);
            for call in self.calls.drain(..) {
                self.services.handle(call);
            }
        }

        fn pointer(&mut self, event: PointerEvent) {
            if self.debug {
                eprintln!("preview: {:?} {:?}", event.phase, event.position);
            }
            if let Some(engine) = self.engine.as_mut() {
                engine.pointer(event);
            }
            self.after_engine_call();
        }

        fn text(&mut self, event: TextInputEvent) {
            if let Some(engine) = self.engine.as_mut() {
                engine.text_input(event);
            }
            self.after_engine_call();
        }

        fn draw(&mut self) {
            let Some(engine) = self.engine.as_mut() else {
                return;
            };
            let now = Instant::now();
            match engine.frame() {
                Ok(FrameOutcome::Presented(_)) => {
                    self.retry_at = None;
                    self.frame_error_reported = false;
                    if !self.announced {
                        self.announced = true;
                        self.announce();
                        if !self.script.is_empty() {
                            self.step_at = Some(now + Duration::from_millis(STEP_PAUSE_MS));
                        }
                    }
                }
                Ok(FrameOutcome::Skipped) => {
                    // Still owed: try again soon, without spinning.
                    self.retry_at = engine.needs_frame().then_some(now + RETRY_AFTER);
                }
                Err(error) => {
                    if !self.frame_error_reported {
                        self.frame_error_reported = true;
                        eprintln!("preview: a frame could not be drawn: {error}");
                    }
                    self.retry_at = Some(now + RETRY_AFTER_ERROR);
                }
            }
            self.after_engine_call();
        }

        fn announce(&self) {
            let (Some(engine), Some(window)) = (&self.engine, &self.window) else {
                return;
            };
            let size = window.inner_size();
            let gpu = engine
                .adapter_info()
                .map(|info| format!("{} on {}", info.backend, info.name))
                .unwrap_or_else(|| "no GPU".to_owned());
            let edges = self.options.insets;
            eprintln!(
                "preview: {:.0}x{:.0} dp at {} px per dp ({}x{} px), insets top {} bottom {} dp, \
                 {gpu}. F1 lists the keys.",
                f64::from(size.width) / f64::from(self.scale),
                f64::from(size.height) / f64::from(self.scale),
                self.scale,
                size.width,
                size.height,
                edges.top,
                edges.bottom,
            );
        }

        /// Saves what is on screen: the frame last presented, drawn again
        /// into memory by the same renderer.
        fn screenshot(&mut self, path: &Path) -> Result<(), String> {
            let (Some(engine), Some(window)) = (&self.engine, &self.window) else {
                return Err("there is no window yet".to_owned());
            };
            if engine.frames_presented() == 0 {
                return Err("nothing has been drawn yet".to_owned());
            }
            let size = window.inner_size();
            let frame = engine.last_frame();
            let pixels = render_offscreen_with_images(
                SurfaceSize::new(size.width, size.height),
                frame.clear,
                &frame.list,
                frame.atlas(),
                frame.images(),
            )
            .map_err(|error| format!("the screen could not be drawn again: {error}"))?;
            let png = morningstar_devtools::encode_png(size.width, size.height, &pixels);
            if let Some(parent) = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("{}: {error}", parent.display()))?;
            }
            std::fs::write(path, png).map_err(|error| format!("{}: {error}", path.display()))
        }

        fn next_shot_path(&mut self) -> PathBuf {
            let dir = match &self.options.state_dir {
                // Beside the state directory: target/preview/<app>/.
                Some(state) => state.parent().unwrap_or(state).to_owned(),
                None => PathBuf::from("."),
            };
            loop {
                self.shots += 1;
                let path = dir.join(format!("screen-{:03}.png", self.shots));
                if !path.exists() {
                    return path;
                }
            }
        }

        fn key(&mut self, event_loop: &ActiveEventLoop, action: KeyAction) {
            match action {
                KeyAction::Text(event) => self.text(event),
                KeyAction::Back => {
                    let handled = self.engine.as_mut().is_some_and(Engine::back);
                    if !handled {
                        eprintln!(
                            "preview: back, and the app has nowhere to go back to: a phone \
                             would leave the app here."
                        );
                    }
                    self.after_engine_call();
                }
                KeyAction::Screenshot => {
                    let path = self.next_shot_path();
                    match self.screenshot(&path) {
                        Ok(()) => eprintln!("preview: saved {}", path.display()),
                        Err(error) => eprintln!("preview: no screenshot: {error}"),
                    }
                }
                KeyAction::Restart { fresh } => {
                    if fresh && let Err(error) = self.services.store_mut().clear() {
                        eprintln!("preview: the stored values could not be forgotten: {error}");
                    }
                    if self.options.supervised {
                        self.exit_code = RESTART_EXIT_CODE;
                        self.shutdown(event_loop);
                    } else {
                        eprintln!("preview: nothing supervises this window to start it again");
                    }
                }
                KeyAction::ToggleReduceMotion => {
                    self.reduce_motion = !self.reduce_motion;
                    if let Some(engine) = self.engine.as_mut() {
                        engine.set_reduce_motion(self.reduce_motion);
                    }
                    eprintln!(
                        "preview: reduce motion is {}",
                        if self.reduce_motion { "on" } else { "off" }
                    );
                }
                KeyAction::ToggleInsets => {
                    self.insets_on = !self.insets_on;
                    let insets = self.insets();
                    if let Some(engine) = self.engine.as_mut() {
                        engine.set_insets(insets);
                    }
                    eprintln!(
                        "preview: the safe-area insets are {}",
                        if self.insets_on { "on" } else { "off" }
                    );
                }
                KeyAction::Help => {
                    if let Some(keys) = USAGE.split("Keys, in the window:").nth(1) {
                        eprintln!("preview: keys, in the window:{keys}");
                    }
                }
                KeyAction::None => {}
            }
        }

        /// Does the next scripted step, through the same code a person's
        /// mouse and keyboard go through.
        fn step(&mut self, event_loop: &ActiveEventLoop, now: Instant) {
            let Some(step) = self.script.pop_front() else {
                self.step_at = None;
                return;
            };
            let mut pause = STEP_PAUSE_MS;
            match step {
                Step::Tap(x, y) => {
                    let at = Point::new(x * self.scale, y * self.scale);
                    eprintln!("preview: tap at {x},{y} dp");
                    // A finger of its own: the real cursor may be moving
                    // over the window, and must not turn the tap into a drag.
                    self.pointer(PointerEvent {
                        id: input::SCRIPT_POINTER,
                        phase: PointerPhase::Down,
                        position: at,
                    });
                    self.lift_at = Some((now + Duration::from_millis(80), at));
                }
                Step::Type(text) => {
                    if self.field_focused {
                        eprintln!("preview: type {} characters", text.chars().count());
                        let action = input::key_action(
                            Key::Other,
                            Some(&text),
                            Modifiers::default(),
                            self.field_focused,
                        );
                        self.key(event_loop, action);
                    } else {
                        eprintln!("preview: nothing typed: no text field has the focus");
                    }
                }
                Step::Back => {
                    eprintln!("preview: back");
                    let action = input::key_action(
                        Key::Escape,
                        None,
                        Modifiers::default(),
                        self.field_focused,
                    );
                    self.key(event_loop, action);
                }
                Step::Wait(wait) => pause = wait,
            }
            self.step_at = (!self.script.is_empty()).then(|| now + Duration::from_millis(pause));
        }

        fn save_position(&mut self) {
            if let Some(position) = self.position.take() {
                self.services.store().set_window_position(position);
                self.position_saved_at = Some(Instant::now());
            }
        }

        /// Takes the engine apart before its window, and stops the loop.
        fn shutdown(&mut self, event_loop: &ActiveEventLoop) {
            self.save_position();
            if let Some(mut engine) = self.engine.take() {
                engine.pause();
                engine.surface_destroyed();
            }
            self.window = None;
            event_loop.exit();
        }
    }

    impl ApplicationHandler<Wake> for Host {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.window.is_some() {
                return;
            }
            if let Err(error) = self.open(event_loop) {
                eprintln!("preview: {error}");
                self.exit_code = 1;
                self.shutdown(event_loop);
            }
        }

        fn user_event(&mut self, event_loop: &ActiveEventLoop, wake: Wake) {
            match wake {
                Wake::Answer(id, result) => {
                    if let Some(engine) = self.engine.as_mut() {
                        engine.service_result(id, result);
                    }
                    self.after_engine_call();
                }
                Wake::Status(status) => {
                    self.status = status;
                    if let Some(window) = &self.window {
                        window.set_title(&self.title());
                    }
                }
                Wake::Quit => self.shutdown(event_loop),
            }
        }

        fn window_event(
            &mut self,
            event_loop: &ActiveEventLoop,
            _id: WindowId,
            event: WindowEvent,
        ) {
            if self.debug
                && matches!(
                    event,
                    WindowEvent::Focused(_)
                        | WindowEvent::Occluded(_)
                        | WindowEvent::Resized(_)
                        | WindowEvent::ScaleFactorChanged { .. }
                        | WindowEvent::Moved(_)
                )
            {
                eprintln!("preview: {event:?}");
            }
            match event {
                WindowEvent::CloseRequested => self.shutdown(event_loop),
                WindowEvent::RedrawRequested => self.draw(),
                WindowEvent::Resized(size) => {
                    if let Some(engine) = self.engine.as_mut() {
                        engine.surface_resized(SurfaceSize::new(size.width, size.height));
                    }
                }
                WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                    // Dragged to a display with other pixels. A scale that
                    // was asked for is kept, and the window changes size in
                    // points; otherwise a dp stays as many points, and is
                    // now another number of pixels.
                    match (self.options.scale, self.window.take()) {
                        (Some(scale), Some(window)) => {
                            let size = window.inner_size();
                            let dp = (
                                size.width as f32 / self.scale,
                                size.height as f32 / self.scale,
                            );
                            self.keep_scale(&window, scale, scale_factor, dp);
                            self.window = Some(window);
                        }
                        (_, window) => {
                            self.window = window;
                            self.scale = (self.zoom * scale_factor) as f32;
                        }
                    }
                    let insets = self.insets();
                    if let Some(engine) = self.engine.as_mut() {
                        engine.set_scale(self.scale);
                        engine.set_insets(insets);
                    }
                }
                WindowEvent::Occluded(hidden) => {
                    if let Some(engine) = self.engine.as_mut() {
                        if hidden {
                            engine.pause();
                        } else {
                            engine.resume();
                        }
                    }
                    self.after_engine_call();
                }
                WindowEvent::Moved(position) => {
                    let factor = self.window.as_ref().map_or(1.0, Window::scale_factor);
                    let points: LogicalPosition<f64> = position.to_logical(factor);
                    self.position = Some((points.x.round() as i32, points.y.round() as i32));
                }
                WindowEvent::Focused(false) => {
                    if let Some(event) = self.mouse.cancel() {
                        self.pointer(event);
                    }
                    if let Some(event) = self.wheel.lift() {
                        self.pointer(event);
                    }
                }
                WindowEvent::ModifiersChanged(modifiers) => {
                    let state: ModifiersState = modifiers.state();
                    self.modifiers = Modifiers {
                        shift: state.shift_key(),
                        alt: state.alt_key(),
                        command: state.super_key(),
                        control: state.control_key(),
                    };
                }
                WindowEvent::CursorMoved { position, .. } => {
                    let position = Point::new(position.x as f32, position.y as f32);
                    if let Some(event) = self.mouse.moved(position) {
                        self.pointer(event);
                    }
                }
                WindowEvent::MouseInput {
                    state,
                    button: MouseButton::Left,
                    ..
                } => {
                    let pressed = state == ElementState::Pressed;
                    if pressed && let Some(event) = self.wheel.lift() {
                        self.pointer(event);
                    }
                    if let Some(event) = self.mouse.button(pressed) {
                        self.pointer(event);
                    }
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    // A press is one gesture; the wheel does not join it.
                    if self.mouse.is_down() {
                        return;
                    }
                    let delta = match delta {
                        MouseScrollDelta::LineDelta(x, y) => {
                            let line = WHEEL_LINE_DP * self.scale;
                            (x * line, y * line)
                        }
                        MouseScrollDelta::PixelDelta(delta) => (delta.x as f32, delta.y as f32),
                    };
                    let touches = self
                        .wheel
                        .scrolled(self.mouse.position(), delta, Instant::now());
                    for event in touches.into_iter().flatten() {
                        self.pointer(event);
                    }
                }
                WindowEvent::KeyboardInput { event, .. }
                    if event.state == ElementState::Pressed =>
                {
                    let key = match &event.logical_key {
                        WinitKey::Named(named) => match named {
                            NamedKey::Backspace => Key::Backspace,
                            NamedKey::Delete => Key::Delete,
                            NamedKey::Enter => Key::Enter,
                            NamedKey::Escape => Key::Escape,
                            NamedKey::ArrowLeft => Key::Left,
                            NamedKey::ArrowRight => Key::Right,
                            NamedKey::ArrowUp => Key::Up,
                            NamedKey::ArrowDown => Key::Down,
                            NamedKey::Home => Key::Home,
                            NamedKey::End => Key::End,
                            NamedKey::F1 => Key::Function(1),
                            NamedKey::F2 => Key::Function(2),
                            NamedKey::F3 => Key::Function(3),
                            NamedKey::F4 => Key::Function(4),
                            NamedKey::F5 => Key::Function(5),
                            _ => Key::Other,
                        },
                        WinitKey::Character(character) => Key::Character(character.as_str()),
                        _ => Key::Other,
                    };
                    let action = input::key_action(
                        key,
                        event.text.as_ref().map(|text| text.as_str()),
                        self.modifiers,
                        self.field_focused,
                    );
                    self.key(event_loop, action);
                }
                _ => {}
            }
        }

        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            let now = Instant::now();
            if let Some(event) = self.wheel.lift_if_still(now) {
                self.pointer(event);
            }
            if self.position.is_some()
                && self
                    .position_saved_at
                    .is_none_or(|at| now.duration_since(at) >= Duration::from_millis(500))
            {
                self.save_position();
            }

            if let Some((_, position)) = self.lift_at.take_if(|(at, _)| now >= *at) {
                self.pointer(PointerEvent {
                    id: input::SCRIPT_POINTER,
                    phase: PointerPhase::Up,
                    position,
                });
            }
            if self.lift_at.is_none() && self.step_at.is_some_and(|at| now >= at) {
                self.step(event_loop, now);
            }

            let drawn = self
                .engine
                .as_ref()
                .is_some_and(|engine| engine.frames_presented() > 0);
            if self.screenshot_at.is_some_and(|at| now >= at) && drawn {
                self.screenshot_at = None;
                if let Some((_, path)) = self.options.screenshot_after.clone() {
                    match self.screenshot(&path) {
                        Ok(()) => eprintln!("preview: saved {}", path.display()),
                        Err(error) => {
                            eprintln!("preview: no screenshot: {error}");
                            self.exit_code = 1;
                        }
                    }
                }
                self.shutdown(event_loop);
                return;
            }

            let (Some(engine), Some(window)) = (&self.engine, &self.window) else {
                return;
            };
            // The earliest moment something is due without an event: a
            // frame the app timed, a retry, the scrolling finger's lift,
            // the scripted screenshot. With none, sleep until an event.
            let mut wake: Option<Instant> = None;
            let mut sooner = |at: Instant| wake = Some(wake.map_or(at, |wake| wake.min(at)));
            if engine.needs_frame() {
                match self.retry_at {
                    Some(at) if at > now => sooner(at),
                    _ => window.request_redraw(),
                }
            } else if let Some(due) = engine.redraw_due_in() {
                sooner(now + due);
            }
            if let Some(at) = self.wheel.lift_at() {
                sooner(at);
            }
            if let Some(at) = self.screenshot_at {
                // Past due but nothing drawn yet: look again shortly.
                sooner(at.max(now + RETRY_AFTER));
            }
            if self.position.is_some() {
                sooner(now + Duration::from_millis(500));
            }
            if let Some(at) = self.lift_at.map(|(at, _)| at).or(self.step_at) {
                sooner(at);
            }
            event_loop.set_control_flow(match wake {
                Some(at) => ControlFlow::WaitUntil(at),
                None => ControlFlow::Wait,
            });
        }
    }
}
