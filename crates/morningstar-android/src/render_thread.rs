//! The render thread: owns the engine and the native window.

use std::sync::OnceLock;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender, TryRecvError};
use std::time::{Duration, Instant};

use morningstar_runtime::{
    App, Capitalization, Engine, FrameOutcome, ImeAction, Insets, KeyboardType, NativeSurface,
    PointerEvent, SurfaceSize, TextInputCommand, TextInputEvent, TextInputRequest,
    byte_to_utf16_offset,
};

use crate::java::{Env, KeyboardCall};
use crate::log;
use crate::sys::NativeWindow;

unsafe extern "Rust" {
    /// Defined in the application crate by `morningstar::app!`.
    fn __morningstar_create_app() -> Box<dyn App>;
}

/// How often frame statistics are written to logcat, in presented frames.
const REPORT_EVERY_FRAMES: u64 = 600;
/// Back-off when a frame could not be produced, so a skipped or failing frame
/// never turns into a busy loop.
const RETRY_DELAY: Duration = Duration::from_millis(16);
/// How long the thread has had nothing to draw before it says so in the log.
/// Shorter pauses (between two touches, say) are not worth a line each.
const IDLE_NOTICE: Duration = Duration::from_secs(1);
/// How much later the idle state is logged a second and last time. Two lines
/// with the same frame count, seconds apart, are the evidence that an idle
/// app draws nothing. After it the thread blocks with no timeout at all.
const IDLE_CONFIRM: Duration = Duration::from_secs(5);
/// How many frames drawn only because a timer ran out (a blinking caret)
/// pass between two log lines about them.
const TIMED_REPORT_EVERY: u64 = 10;

pub enum Message {
    Resume,
    Pause,
    SurfaceCreated(NativeWindow),
    SurfaceChanged(SurfaceSize),
    /// The sender is signalled once the surface is fully released.
    SurfaceDestroyed(SyncSender<()>),
    Pointer(PointerEvent),
    /// Physical pixels per dp (`DisplayMetrics.density`).
    Scale(f32),
    /// Safe-area insets in physical pixels.
    Insets(Insets),
    /// The system asked for the window's contents to be drawn again.
    Redraw,
    /// Benchmark mode: draw every refresh whether or not anything changed.
    Continuous(bool),
    /// Pixels of the bottom of the surface under the soft keyboard.
    KeyboardInset(f32),
    /// Text input. `serial` is the keyboard request the host made the edit
    /// under, for events that only make sense for that request.
    Text {
        serial: Option<i32>,
        event: TextInputEvent,
    },
}

static RENDER_THREAD: OnceLock<Sender<Message>> = OnceLock::new();

/// Starts the render thread on first call. The thread lives for the rest of
/// the process: Android may destroy and recreate the `Activity` (and its
/// surface) many times within one process, and the engine survives that.
pub fn start() {
    RENDER_THREAD.get_or_init(|| {
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("morningstar-render".to_owned())
            .spawn(move || run(receiver))
            .expect("failed to spawn the render thread");
        sender
    });
}

pub fn send(message: Message) {
    match RENDER_THREAD.get() {
        // A send only fails if the render thread is gone, which means it
        // panicked and the process is already aborting.
        Some(sender) => drop(sender.send(message)),
        None => log::error("host callback received before nativeInit"),
    }
}

/// Logging of idle periods. Purely diagnostic: it decides when to write a
/// log line, never whether to draw.
#[derive(Default)]
struct IdleLog {
    /// When the thread last ran out of things to draw.
    since: Option<Instant>,
    /// How many of the two idle lines have been written for this period.
    lines: u8,
}

impl IdleLog {
    /// How long to wait for a message before the next idle line is due, or
    /// `None` to wait indefinitely.
    fn next_line_in(&mut self) -> Option<Duration> {
        let since = *self.since.get_or_insert_with(Instant::now);
        let due = match self.lines {
            0 => IDLE_NOTICE,
            1 => IDLE_NOTICE + IDLE_CONFIRM,
            _ => return None,
        };
        Some(due.saturating_sub(since.elapsed()))
    }

    /// A wait from [`next_line_in`](Self::next_line_in) ran out.
    fn line_due(&mut self, engine: &Engine) {
        let idle_for = self.since.map_or(Duration::ZERO, |since| since.elapsed());
        self.lines += 1;
        log::info(&format!(
            "idle {:.1}s: frames={} visible={} (render thread blocked, {})",
            idle_for.as_secs_f32(),
            engine.frames_presented(),
            engine.should_render(),
            if self.lines == 1 {
                "nothing to draw"
            } else {
                "last idle line until the next frame"
            },
        ));
    }

    /// A frame was presented: the idle period, if any, is over.
    fn frame_presented(&mut self, engine: &Engine) {
        if let Some(since) = self.since.take()
            && self.lines > 0
        {
            log::info(&format!(
                "woke after {:.1}s idle: frames={}",
                since.elapsed().as_secs_f32(),
                engine.frames_presented(),
            ));
        }
        self.lines = 0;
    }
}

fn run(messages: Receiver<Message>) {
    // SAFETY: the symbol is provided by `morningstar::app!` in the app crate;
    // linking fails if it is absent.
    let mut engine = Engine::new(unsafe { __morningstar_create_app() });
    // The engine's GPU surface points into this window, so every path below
    // detaches the surface before releasing the window.
    let mut window: Option<NativeWindow> = None;
    let mut last_report = 0;
    let mut retry = false;
    let mut idle = IdleLog::default();
    // The keyboard is asked for through Java; see `java.rs`.
    let java = Env::attach_current_thread();
    if java.is_none() {
        log::error("could not attach the render thread to the JVM: no keyboard");
    }
    let mut keyboard = Keyboard::default();

    loop {
        // Nothing to draw (hidden, or visible with an unchanged picture):
        // block until the host says something, using no CPU. Only a message
        // can change `needs_frame`, so nothing is missed by sleeping.
        // Something to draw: only drain what has already arrived.
        let message = if !engine.needs_frame() {
            // Sleep until a message, or the next idle log line, or the
            // moment the app asked to be drawn again (a caret's blink),
            // whichever is first.
            let idle_line = idle.next_line_in();
            let timer = engine.redraw_due_in();
            let timeout = match (idle_line, timer) {
                (Some(line), Some(timer)) => Some(line.min(timer)),
                (line, timer) => line.or(timer),
            };
            match timeout {
                Some(timeout) => match messages.recv_timeout(timeout) {
                    Ok(message) => Some(message),
                    Err(RecvTimeoutError::Timeout) => {
                        // If it was the timer, `needs_frame` is now true.
                        if !engine.needs_frame() && idle.next_line_in() == Some(Duration::ZERO) {
                            idle.line_due(&engine);
                        }
                        continue;
                    }
                    Err(RecvTimeoutError::Disconnected) => break,
                },
                None => match messages.recv() {
                    Ok(message) => Some(message),
                    Err(_) => break,
                },
            }
        } else if retry {
            match messages.recv_timeout(RETRY_DELAY) {
                Ok(message) => Some(message),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match messages.try_recv() {
                Ok(message) => Some(message),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => break,
            }
        };
        retry = false;

        if let Some(message) = message {
            handle(message, &mut engine, &mut window, &mut keyboard);
            continue;
        }

        // Vsync pacing happens inside `frame`, while acquiring the drawable.
        let outcome = engine.frame();
        // The frame just built says whether a field has focus.
        if let Some(command) = engine.take_text_input_command() {
            keyboard.send(java.as_ref(), &command);
        }
        match outcome {
            Ok(FrameOutcome::Presented(_)) => {
                idle.frame_presented(&engine);
                keyboard.timed_frame_presented(&engine);
                let frames = engine.frames_presented();
                if frames - last_report >= REPORT_EVERY_FRAMES {
                    last_report = frames;
                    if let Some(report) = engine.report() {
                        log::info(&format!("stats {report}"));
                    }
                }
            }
            Ok(FrameOutcome::Skipped) => retry = true,
            Err(error) => {
                log::error(&format!("frame failed: {error}"));
                retry = true;
            }
        }
    }

    engine.surface_destroyed();
    drop(window);
}

/// The render thread's side of the keyboard: numbering the requests it
/// makes of the host, and the log lines that let text input be followed
/// from logcat. Text itself is never logged, only its length.
#[derive(Default)]
struct Keyboard {
    /// Counts `Show` commands. The host returns it with each edit so that
    /// an edit made under an earlier request can be recognised.
    serial: i32,
    /// For the periodic line about timed frames.
    timed_logged: u64,
    timed_since: Option<Instant>,
}

impl Keyboard {
    fn send(&mut self, java: Option<&Env>, command: &TextInputCommand) {
        let (code, request) = match command {
            TextInputCommand::Hide => (0, None),
            TextInputCommand::Show(request) => {
                self.serial = self.serial.wrapping_add(1);
                (1, Some(request))
            }
            TextInputCommand::Update(request) => (2, Some(request)),
        };
        let empty = TextInputRequest {
            session: 0,
            traits: Default::default(),
            text: String::new(),
            caret: 0,
        };
        let request = request.unwrap_or(&empty);
        let traits = request.traits;
        let caret = byte_to_utf16_offset(&request.text, request.caret);
        match command {
            TextInputCommand::Hide => log::info("text input: hide keyboard"),
            TextInputCommand::Show(_) => log::info(&format!(
                "text input: show keyboard serial={} type={:?} action={:?} secure={} multiline={} \
                 chars={} caret={caret}",
                self.serial,
                traits.keyboard,
                traits.action,
                traits.secure,
                traits.multiline,
                request.text.chars().count(),
            )),
            TextInputCommand::Update(_) => log::info(&format!(
                "text input: update host text chars={} caret={caret}",
                request.text.chars().count(),
            )),
        }
        let Some(java) = java else {
            return;
        };
        java.keyboard(&KeyboardCall {
            command: code,
            serial: self.serial,
            keyboard: match traits.keyboard {
                KeyboardType::Text => 0,
                KeyboardType::Number => 1,
                KeyboardType::Decimal => 2,
                KeyboardType::Phone => 3,
                KeyboardType::Email => 4,
            },
            capitalization: match traits.capitalization {
                Capitalization::None => 0,
                Capitalization::Sentences => 1,
                Capitalization::Words => 2,
                Capitalization::Characters => 3,
            },
            // The numbers of `nativeTextCommand`'s action keys.
            action: match traits.action {
                ImeAction::Next => 1,
                ImeAction::Done => 2,
                ImeAction::Go => 3,
                ImeAction::Search => 4,
                ImeAction::Send => 5,
            },
            flags: i32::from(traits.secure)
                | i32::from(traits.multiline) << 1
                | i32::from(traits.autocorrect) << 2,
            text: &request.text,
            caret: i32::try_from(caret).unwrap_or(i32::MAX),
        });
    }

    /// Says, every few of them, how many frames a timer alone produced:
    /// the evidence that a caret blinks without continuous rendering.
    fn timed_frame_presented(&mut self, engine: &Engine) {
        let timed = engine.timed_frames();
        if timed == self.timed_logged {
            return;
        }
        let since = *self.timed_since.get_or_insert_with(Instant::now);
        if timed - self.timed_logged >= TIMED_REPORT_EVERY {
            log::info(&format!(
                "timed frames: {} in {:.1}s (caret blink; total timed={timed} frames={})",
                timed - self.timed_logged,
                since.elapsed().as_secs_f32(),
                engine.frames_presented(),
            ));
            self.timed_logged = timed;
            self.timed_since = Some(Instant::now());
        }
    }
}

fn describe(event: &TextInputEvent) -> String {
    match event {
        TextInputEvent::Commit(text) => format!("commit chars={}", text.chars().count()),
        TextInputEvent::Compose(text) => format!("compose chars={}", text.chars().count()),
        TextInputEvent::DeleteBackward => "delete backward".to_owned(),
        TextInputEvent::DeleteForward => "delete forward".to_owned(),
        TextInputEvent::Enter => "enter".to_owned(),
        TextInputEvent::Action(action) => format!("action {action:?}"),
        TextInputEvent::Edited(state) => format!(
            "edited by the input method chars={} caret={} composing={}",
            state.text.chars().count(),
            state.caret,
            state.composing.is_some(),
        ),
        TextInputEvent::FocusLost => "keyboard dismissed by the user".to_owned(),
    }
}

fn handle(
    message: Message,
    engine: &mut Engine,
    window: &mut Option<NativeWindow>,
    keyboard: &mut Keyboard,
) {
    match message {
        Message::KeyboardInset(inset) => {
            if engine.set_keyboard_inset(inset) {
                log::info(&format!(
                    "keyboard inset {} px: frames={}",
                    engine.keyboard_inset(),
                    engine.frames_presented()
                ));
            }
        }
        Message::Text { serial, event } => {
            if serial.is_some_and(|serial| serial != keyboard.serial) {
                // Made on the host's copy of a field that no longer has
                // the focus: applying it to the new one would be wrong.
                log::info(&format!("text input: dropped stale {}", describe(&event)));
            } else {
                log::info(&format!("text input: {}", describe(&event)));
                engine.text_input(event);
            }
        }
        Message::Resume => {
            engine.resume();
            log::info(&format!("resumed: frames={}", engine.frames_presented()));
        }
        Message::Pause => {
            engine.pause();
            log::info(&format!("paused: frames={}", engine.frames_presented()));
        }
        Message::Pointer(event) => engine.pointer(event),
        Message::Scale(scale) => {
            if engine.set_scale(scale) {
                log::info(&format!("scale {scale} px/dp"));
            }
        }
        Message::Insets(insets) => {
            if engine.set_insets(insets) {
                let now = engine.insets();
                log::info(&format!(
                    "insets left={} top={} right={} bottom={} px",
                    now.left, now.top, now.right, now.bottom
                ));
            }
        }
        Message::Redraw => engine.request_redraw(),
        Message::Continuous(continuous) => {
            engine.set_continuous(continuous);
            if continuous {
                log::info("continuous rendering on (benchmark mode)");
            }
        }
        Message::SurfaceCreated(new_window) => {
            // Release any previous surface before its window.
            engine.surface_destroyed();
            *window = None;

            let size = new_window.size();
            let native = NativeSurface::AndroidNativeWindow(new_window.as_ptr());
            // SAFETY: `new_window` is a live `ANativeWindow`. It is stored in
            // `window` below and only released after `surface_destroyed`.
            match unsafe { engine.surface_created(native, size) } {
                Ok(()) => {
                    if let Some(gpu) = engine.adapter_info() {
                        log::info(&format!(
                            "surface {}x{} on {} ({}, {})",
                            size.width, size.height, gpu.name, gpu.backend, gpu.driver
                        ));
                    }
                    *window = Some(new_window);
                }
                Err(error) => log::error(&format!("could not attach surface: {error}")),
            }
        }
        Message::SurfaceChanged(size) => {
            // Android also reports the size the surface was created with.
            if engine.surface_resized(size) {
                log::info(&format!(
                    "surface resized to {}x{}",
                    size.width, size.height
                ));
            }
        }
        Message::SurfaceDestroyed(done) => {
            engine.surface_destroyed();
            *window = None;
            // The host is blocked on this; it may now let Android free the surface.
            let _ = done.send(());
        }
    }
}
