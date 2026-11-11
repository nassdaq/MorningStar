//! iOS host integration.
//!
//! The Swift side (`templates/ios`) is a thin shell: a `UIView` backed by a
//! `CAMetalLayer`, a `CADisplayLink`, and scene lifecycle callbacks, all
//! forwarded to the C functions below (declared in `include/morningstar_ios.h`).
//!
//! Threading: every function must be called on the main thread. Frames are
//! produced from the display link callback. Whether rendering should move to
//! a dedicated thread on iOS is an open question tracked in ADR 0004.
//!
//! Frames are produced on demand (ADR 0012). The engine can only start to
//! need a frame as a result of one of the calls below, so the host's rule is
//! simple: after every call into this interface, ask
//! [`morningstar_ios_needs_frame`] and pause or resume the display link to
//! match.
//!
//! The crate is empty on every other target so the workspace builds on a
//! development host.
#![cfg(target_os = "ios")]

use std::ffi::{CStr, c_char, c_void};
use std::ptr::NonNull;

use morningstar_runtime::{
    App, Capitalization, Engine, FrameOutcome, ImeAction, Insets, KeyboardType, NativeSurface,
    Point, PointerEvent, PointerPhase, SurfaceSize, TextInputCommand, TextInputEvent,
};

unsafe extern "Rust" {
    /// Defined in the application crate by `morningstar::app!`.
    fn __morningstar_create_app() -> Box<dyn App>;
}

/// How often frame statistics are written to the console, in presented frames.
const REPORT_EVERY_FRAMES: u64 = 600;

/// Opaque handle owned by the Swift host.
pub struct MorningstarHost {
    engine: Engine,
    last_report: u64,
    /// The most recent frame error, to log each distinct failure once rather
    /// than sixty times a second.
    last_error: Option<String>,
}

/// Creates the host. Returns a handle to pass to every other function and,
/// finally, to [`morningstar_ios_destroy`].
#[unsafe(no_mangle)]
pub extern "C" fn morningstar_ios_create() -> *mut MorningstarHost {
    // SAFETY: the symbol is provided by `morningstar::app!` in the app crate;
    // linking fails if it is absent.
    let app = unsafe { __morningstar_create_app() };
    Box::into_raw(Box::new(MorningstarHost {
        engine: Engine::new(app),
        last_report: 0,
        last_error: None,
    }))
}

/// Attaches the engine to a `CAMetalLayer`. Returns `false` on failure.
///
/// # Safety
/// `host` must come from [`morningstar_ios_create`]. `metal_layer` must be a
/// valid `CAMetalLayer*` that outlives the attachment, i.e. until
/// [`morningstar_ios_detach_layer`] or [`morningstar_ios_destroy`] returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morningstar_ios_attach_layer(
    host: *mut MorningstarHost,
    metal_layer: *mut c_void,
    width_px: u32,
    height_px: u32,
) -> bool {
    // SAFETY: caller contract.
    let Some(host) = (unsafe { host.as_mut() }) else {
        return false;
    };
    let Some(layer) = NonNull::new(metal_layer) else {
        eprintln!("morningstar: attach_layer called with a null layer");
        return false;
    };

    let size = SurfaceSize::new(width_px, height_px);
    // SAFETY: the caller guarantees the layer is valid for the attachment.
    match unsafe {
        host.engine
            .surface_created(NativeSurface::CoreAnimationLayer(layer), size)
    } {
        Ok(()) => {
            if let Some(gpu) = host.engine.adapter_info() {
                eprintln!(
                    "morningstar: surface {width_px}x{height_px} on {} ({})",
                    gpu.name, gpu.backend
                );
            }
            true
        }
        Err(error) => {
            eprintln!("morningstar: could not attach layer: {error}");
            false
        }
    }
}

/// Informs the engine of a new drawable size in physical pixels.
///
/// # Safety
/// `host` must come from [`morningstar_ios_create`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morningstar_ios_resize(
    host: *mut MorningstarHost,
    width_px: u32,
    height_px: u32,
) {
    // SAFETY: caller contract.
    if let Some(host) = unsafe { host.as_mut() } {
        host.engine
            .surface_resized(SurfaceSize::new(width_px, height_px));
    }
}

/// Releases the layer. After this returns the engine no longer references it.
///
/// # Safety
/// `host` must come from [`morningstar_ios_create`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morningstar_ios_detach_layer(host: *mut MorningstarHost) {
    // SAFETY: caller contract.
    if let Some(host) = unsafe { host.as_mut() } {
        host.engine.surface_destroyed();
    }
}

/// Tells the engine whether the scene is active. iOS forbids GPU work in the
/// background, so the host must pass `false` before the app is suspended.
///
/// # Safety
/// `host` must come from [`morningstar_ios_create`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morningstar_ios_set_active(host: *mut MorningstarHost, active: bool) {
    // SAFETY: caller contract.
    if let Some(host) = unsafe { host.as_mut() } {
        if active {
            host.engine.resume();
        } else {
            host.engine.pause();
        }
    }
}

/// Sets the display scale: physical pixels per point (the view's
/// `contentScaleFactor`, taken from the screen's `nativeScale`). Call before
/// attaching the layer and whenever the scale may have changed; repeating
/// the current value is free.
///
/// # Safety
/// `host` must come from [`morningstar_ios_create`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morningstar_ios_set_scale(host: *mut MorningstarHost, scale: f32) {
    // SAFETY: caller contract.
    if let Some(host) = unsafe { host.as_mut() }
        && host.engine.set_scale(scale)
    {
        eprintln!("morningstar: scale {scale} px/pt");
    }
}

/// Sets the safe-area insets (the view's `safeAreaInsets`), converted to
/// physical pixels. Call whenever they may have changed.
///
/// # Safety
/// `host` must come from [`morningstar_ios_create`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morningstar_ios_set_insets(
    host: *mut MorningstarHost,
    left_px: f32,
    top_px: f32,
    right_px: f32,
    bottom_px: f32,
) {
    // SAFETY: caller contract.
    if let Some(host) = unsafe { host.as_mut() }
        && host
            .engine
            .set_insets(Insets::new(left_px, top_px, right_px, bottom_px))
    {
        let now = host.engine.insets();
        eprintln!(
            "morningstar: insets left={} top={} right={} bottom={} px",
            now.left, now.top, now.right, now.bottom
        );
    }
}

/// Benchmark mode: while `continuous` is true a frame is produced at every
/// display refresh whether or not anything changed.
///
/// # Safety
/// `host` must come from [`morningstar_ios_create`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morningstar_ios_set_continuous(
    host: *mut MorningstarHost,
    continuous: bool,
) {
    // SAFETY: caller contract.
    if let Some(host) = unsafe { host.as_mut() } {
        host.engine.set_continuous(continuous);
    }
}

/// True when the next display refresh should call [`morningstar_ios_frame`]:
/// the scene is active, a layer is attached, and something changed since the
/// last frame was shown. While false the display link should be paused.
///
/// The answer changes only inside calls to this interface. Ask after each.
///
/// # Safety
/// `host` must come from [`morningstar_ios_create`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morningstar_ios_needs_frame(host: *const MorningstarHost) -> bool {
    // SAFETY: caller contract.
    unsafe { host.as_ref() }.is_some_and(|host| host.engine.needs_frame())
}

/// Delivers one touch event. `phase` is 0 down, 1 moved, 2 up, 3 cancelled
/// (the `MORNINGSTAR_PHASE_*` constants). `pointer_id` must be the same value
/// for one finger from down to up. Coordinates are in physical pixels
/// relative to the layer.
///
/// # Safety
/// `host` must come from [`morningstar_ios_create`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morningstar_ios_pointer(
    host: *mut MorningstarHost,
    phase: u32,
    pointer_id: u64,
    x_px: f32,
    y_px: f32,
) {
    let phase = match phase {
        0 => PointerPhase::Down,
        1 => PointerPhase::Moved,
        2 => PointerPhase::Up,
        3 => PointerPhase::Cancelled,
        // A host newer than this library; ignoring is safer than guessing.
        _ => return,
    };
    // SAFETY: caller contract.
    if let Some(host) = unsafe { host.as_mut() } {
        host.engine.pointer(PointerEvent {
            id: pointer_id,
            phase,
            position: Point::new(x_px, y_px),
        });
    }
}

/// Produces one frame if one is needed; otherwise does nothing. Call from the
/// `CADisplayLink` callback, then ask [`morningstar_ios_needs_frame`] whether
/// to keep the display link running.
///
/// # Safety
/// `host` must come from [`morningstar_ios_create`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morningstar_ios_frame(host: *mut MorningstarHost) {
    // SAFETY: caller contract.
    let Some(host) = (unsafe { host.as_mut() }) else {
        return;
    };

    match host.engine.frame() {
        Ok(FrameOutcome::Presented(_)) => {
            host.last_error = None;
            let frames = host.engine.frames_presented();
            if frames - host.last_report >= REPORT_EVERY_FRAMES {
                host.last_report = frames;
                if let Some(report) = host.engine.report() {
                    eprintln!("morningstar: stats {report}");
                }
            }
        }
        Ok(FrameOutcome::Skipped) => {}
        Err(error) => {
            let message = error.to_string();
            if host.last_error.as_deref() != Some(&message) {
                eprintln!("morningstar: frame failed: {message}");
                host.last_error = Some(message);
            }
        }
    }
}

/// How much of the bottom of the layer the keyboard covers, in physical
/// pixels; zero when it is hidden. Call from the keyboard frame
/// notifications.
///
/// # Safety
/// `host` must be null or a pointer from [`morningstar_ios_create`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morningstar_ios_set_keyboard_inset(
    host: *mut MorningstarHost,
    bottom_px: f32,
) {
    // SAFETY: guaranteed by the caller.
    if let Some(host) = unsafe { host.as_mut() }
        && host.engine.set_keyboard_inset(bottom_px)
    {
        eprintln!(
            "morningstar: keyboard inset {} px",
            host.engine.keyboard_inset()
        );
    }
}

/// `UIKeyInput.insertText`: text typed on the keyboard. A lone line feed is
/// the Return key.
///
/// # Safety
/// `host` must be null or a pointer from [`morningstar_ios_create`];
/// `utf8` must be null or point to a NUL-terminated string that stays valid
/// for the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morningstar_ios_text_insert(
    host: *mut MorningstarHost,
    utf8: *const c_char,
) {
    // SAFETY: guaranteed by the caller.
    let Some(host) = (unsafe { host.as_mut() }) else {
        return;
    };
    if utf8.is_null() {
        return;
    }
    // SAFETY: non-null and NUL-terminated, by the caller's contract. The
    // bytes are copied before returning.
    let text = unsafe { CStr::from_ptr(utf8) }
        .to_string_lossy()
        .into_owned();
    let event = match text.as_str() {
        "" => return,
        "\n" => TextInputEvent::Enter,
        _ => TextInputEvent::Commit(text),
    };
    host.engine.text_input(event);
}

/// `UIKeyInput.deleteBackward`: the delete key.
///
/// # Safety
/// `host` must be null or a pointer from [`morningstar_ios_create`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morningstar_ios_text_delete_backward(host: *mut MorningstarHost) {
    // SAFETY: guaranteed by the caller.
    if let Some(host) = unsafe { host.as_mut() } {
        host.engine.text_input(TextInputEvent::DeleteBackward);
    }
}

/// The view stopped being first responder without being asked to (the
/// keyboard's own dismiss key on an iPad): the focused field gives up
/// focus.
///
/// # Safety
/// `host` must be null or a pointer from [`morningstar_ios_create`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morningstar_ios_text_focus_lost(host: *mut MorningstarHost) {
    // SAFETY: guaranteed by the caller.
    if let Some(host) = unsafe { host.as_mut() } {
        host.engine.text_input(TextInputEvent::FocusLost);
    }
}

/// What the host must do to its keyboard: the `MORNINGSTAR_TEXT_*`,
/// `MORNINGSTAR_KEYBOARD_*`, `MORNINGSTAR_CAPITALIZE_*` and
/// `MORNINGSTAR_ACTION_*` constants of `morningstar_ios.h`.
#[repr(C)]
pub struct MorningstarTextInput {
    /// 0 hide, 1 show (become first responder with these traits), 2 the
    /// text changed (only `has_text` is of interest).
    pub command: u32,
    pub keyboard: u32,
    pub capitalization: u32,
    pub action: u32,
    pub secure: bool,
    pub multiline: bool,
    pub autocorrect: bool,
    /// For `UIKeyInput.hasText`.
    pub has_text: bool,
}

/// Fills `out` with the next thing to do to the keyboard and returns true,
/// or returns false when there is nothing to do, which is the usual case.
/// Call after every other call into this interface, as for
/// [`morningstar_ios_needs_frame`]: the answer changes only inside them.
///
/// # Safety
/// `host` must be null or a pointer from [`morningstar_ios_create`]; `out`
/// must be null or valid for writing one `MorningstarTextInput`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morningstar_ios_take_text_input(
    host: *mut MorningstarHost,
    out: *mut MorningstarTextInput,
) -> bool {
    // SAFETY: guaranteed by the caller.
    let (Some(host), Some(out)) = (unsafe { host.as_mut() }, unsafe { out.as_mut() }) else {
        return false;
    };
    let Some(command) = host.engine.take_text_input_command() else {
        return false;
    };
    let (code, request) = match &command {
        TextInputCommand::Hide => (0, None),
        TextInputCommand::Show(request) => (1, Some(request)),
        TextInputCommand::Update(request) => (2, Some(request)),
    };
    let traits = request.map(|request| request.traits).unwrap_or_default();
    *out = MorningstarTextInput {
        command: code,
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
        action: match traits.action {
            ImeAction::Next => 1,
            ImeAction::Done => 2,
            ImeAction::Go => 3,
            ImeAction::Search => 4,
            ImeAction::Send => 5,
        },
        secure: traits.secure,
        multiline: traits.multiline,
        autocorrect: traits.autocorrect,
        has_text: request.is_some_and(|request| !request.text.is_empty()),
    };
    if code != 2 {
        eprintln!(
            "morningstar: text input: {}",
            if code == 1 {
                "show keyboard"
            } else {
                "hide keyboard"
            }
        );
    }
    true
}

/// Seconds until the app wants a frame although nothing has happened (a
/// caret's blink), or a negative number when it wants none. While
/// [`morningstar_ios_needs_frame`] is false and this is not negative, the
/// host arms a timer for that long and asks again when it fires.
///
/// # Safety
/// `host` must be null or a pointer from [`morningstar_ios_create`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morningstar_ios_redraw_delay(host: *const MorningstarHost) -> f64 {
    // SAFETY: guaranteed by the caller.
    unsafe { host.as_ref() }
        .and_then(|host| host.engine.redraw_due_in())
        .map_or(-1.0, |delay| delay.as_secs_f64())
}

/// Destroys the host and releases every GPU resource.
///
/// # Safety
/// `host` must come from [`morningstar_ios_create`] and must not be used
/// afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morningstar_ios_destroy(host: *mut MorningstarHost) {
    if !host.is_null() {
        // SAFETY: reclaims the box leaked in `morningstar_ios_create`.
        drop(unsafe { Box::from_raw(host) });
    }
}
