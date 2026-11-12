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
//! Platform services (ADR 0019) follow the same rule. The application's
//! requests (an HTTP request, a recording, a clip to play, a stored value)
//! appear only inside calls into this interface, so after every call the
//! host also drains [`morningstar_ios_take_service_request`], does the work
//! off the main thread, and comes back **on the main thread** with one of
//! the `morningstar_ios_*_result` functions. The host must not call them
//! after [`morningstar_ios_destroy`]: an answer for a destroyed host is
//! dropped on the Swift side, which is the only side that can know.
//!
//! The crate is empty on every other target so the workspace builds on a
//! development host.
#![cfg(target_os = "ios")]

use std::collections::VecDeque;
use std::ffi::{CStr, CString, c_char, c_void};
use std::ptr::NonNull;
use std::time::Duration;

use morningstar_runtime::{
    App, AudioError, Capitalization, Engine, FrameOutcome, HostState, HttpError, HttpResponse,
    ImeAction, Insets, KeyboardType, NativeSurface, Point, PointerEvent, PointerPhase, Recording,
    ServiceCall, ServiceId, ServiceRequest, ServiceResult, SurfaceSize, TextInputCommand,
    TextInputEvent,
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
    /// Requests the application has made and the host has not taken yet.
    calls: VecDeque<ServiceCall>,
    /// Reused when asking the engine for requests.
    scratch: Vec<ServiceCall>,
    /// The request most recently handed out. It owns the memory the
    /// pointers in the host's `MorningstarServiceRequest` point into, and
    /// is kept until the next one is handed out.
    lent: Option<LentRequest>,
}

/// Owns what a `MorningstarServiceRequest` points to.
struct LentRequest {
    _call: ServiceCall,
    _strings: [CString; 3],
}

impl MorningstarHost {
    fn new(state: &HostState) -> *mut Self {
        // SAFETY: the symbol is provided by `morningstar::app!` in the app
        // crate; linking fails if it is absent.
        let app = unsafe { __morningstar_create_app() };
        let mut host = Box::new(Self {
            engine: Engine::with_host_state(app, state),
            last_report: 0,
            last_error: None,
            calls: VecDeque::new(),
            scratch: Vec::new(),
            lent: None,
        });
        // What the application asked for while it was being set up.
        host.collect_service_requests();
        Box::into_raw(host)
    }

    fn collect_service_requests(&mut self) {
        self.engine.take_service_requests(&mut self.scratch);
        self.calls.extend(self.scratch.drain(..));
    }
}

/// Creates the host for an application that is handed nothing at setup: no
/// stored values and a placeholder installation id. Hosts that offer
/// platform services use [`morningstar_ios_create_with_state`]. Returns a
/// handle to pass to every other function and, finally, to
/// [`morningstar_ios_destroy`].
#[unsafe(no_mangle)]
pub extern "C" fn morningstar_ios_create() -> *mut MorningstarHost {
    MorningstarHost::new(&HostState::new())
}

/// Creates the host, handing the application what it may read at setup:
/// `install_id`, this installation's identifier (32 hexadecimal digits;
/// anything else is ignored), and the values it stored on earlier runs,
/// as `stored_count` strings that are keys and values in turn.
///
/// # Safety
/// `install_id` must be null or a NUL-terminated string. `stored` must be
/// null (with a count of zero) or point to `stored_count` pointers, each
/// null or a NUL-terminated string. All of it need only stay valid for the
/// call: everything is copied.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morningstar_ios_create_with_state(
    install_id: *const c_char,
    stored: *const *const c_char,
    stored_count: usize,
) -> *mut MorningstarHost {
    // SAFETY: null or NUL-terminated, by the caller's contract.
    let mut state = HostState::new().with_install_id(&unsafe { text(install_id) });
    if !stored.is_null() {
        // SAFETY: `stored` points to `stored_count` pointers (caller), each
        // null or NUL-terminated.
        let strings = unsafe { std::slice::from_raw_parts(stored, stored_count) };
        for pair in strings.chunks_exact(2) {
            // SAFETY: as above.
            let (key, value) = unsafe { (text(pair[0]), text(pair[1])) };
            state = state.with_stored(key, value);
        }
    }
    MorningstarHost::new(&state)
}

/// Copies a C string; null is the empty string, and bytes that are not
/// UTF-8 become U+FFFD.
///
/// # Safety
/// `utf8` must be null or point to a NUL-terminated string.
unsafe fn text(utf8: *const c_char) -> String {
    if utf8.is_null() {
        return String::new();
    }
    // SAFETY: non-null and NUL-terminated (caller). Copied before returning.
    unsafe { CStr::from_ptr(utf8) }
        .to_string_lossy()
        .into_owned()
}

/// Copies `len` bytes; a null pointer or a zero length is no bytes.
///
/// # Safety
/// `data` must be null or valid for reading `len` bytes.
unsafe fn bytes(data: *const u8, len: usize) -> Vec<u8> {
    if data.is_null() || len == 0 {
        return Vec::new();
    }
    // SAFETY: non-null and valid for `len` bytes (caller). Copied before
    // returning.
    unsafe { std::slice::from_raw_parts(data, len) }.to_vec()
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

/// A "back" gesture, as the host defines one (iOS has no system Back; an
/// edge swipe is the usual choice). Returns `true` if the application
/// handled it, for example by going to its previous screen; a frame then
/// follows, so ask [`morningstar_ios_needs_frame`] as after every call.
/// Returns `false` if the application had nowhere to go back to, and for a
/// null host: the host then does nothing, since an iOS app is not left
/// with a gesture of its own.
///
/// Called on the main thread like every function here, where the engine
/// lives, so the answer is immediate: it runs the application's `back`
/// and builds no frame.
///
/// # Safety
/// `host` must be null or a pointer from [`morningstar_ios_create`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morningstar_ios_back(host: *mut MorningstarHost) -> bool {
    // SAFETY: guaranteed by the caller.
    match unsafe { host.as_mut() } {
        Some(host) => host.engine.back(),
        None => false,
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

/// One request of the application to the platform, for the host to carry
/// out. `kind` is a `MORNINGSTAR_SERVICE_*` constant of
/// `morningstar_ios.h`; which of the other fields mean something depends
/// on it and is said there. Strings are NUL-terminated UTF-8 and never
/// null (unused ones are empty); `data` is null when `data_len` is zero.
///
/// Everything pointed to stays valid until the next call to
/// [`morningstar_ios_take_service_request`] or
/// [`morningstar_ios_destroy`]: copy what outlives that.
#[repr(C)]
pub struct MorningstarServiceRequest {
    /// Give this back with the answer.
    pub id: u64,
    pub kind: u32,
    /// HTTP: whole-exchange timeout in milliseconds, at least 1.
    pub timeout_ms: u32,
    /// HTTP: the method. Play: the media type. Store, remove, load: the key.
    pub text1: *const c_char,
    /// HTTP: the URL. Store: the value.
    pub text2: *const c_char,
    /// HTTP: the headers, as `Name: value` lines each ended by `\n`.
    pub text3: *const c_char,
    /// HTTP: the body. Play: the audio file.
    pub data: *const u8,
    pub data_len: usize,
}

/// A C string holding `text`. A string cannot carry a NUL across this
/// interface, so any in `text` are removed.
fn c_string(text: &str) -> CString {
    CString::new(text)
        .unwrap_or_else(|_| CString::new(text.replace('\0', "")).expect("NULs were just removed"))
}

/// Fills `out` with the next request of the application and returns true,
/// or returns false when there is none, which is the usual case. Call it
/// in a loop after every other call into this interface, as for
/// [`morningstar_ios_take_text_input`]: requests appear only inside them.
///
/// Every request whose kind is answered (all but stop-playback, store and
/// remove) must be answered exactly once with the matching
/// `morningstar_ios_*_result` function, on the main thread, with an error
/// if it cannot be carried out; the application waits for it.
///
/// # Safety
/// `host` must be null or a pointer from [`morningstar_ios_create`]; `out`
/// must be null or valid for writing one `MorningstarServiceRequest`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morningstar_ios_take_service_request(
    host: *mut MorningstarHost,
    out: *mut MorningstarServiceRequest,
) -> bool {
    // SAFETY: guaranteed by the caller.
    let (Some(host), Some(out)) = (unsafe { host.as_mut() }, unsafe { out.as_mut() }) else {
        return false;
    };
    host.collect_service_requests();
    // The previous request's memory is released here, not before.
    host.lent = None;
    let Some(call) = host.calls.pop_front() else {
        return false;
    };
    let mut timeout_ms = 0;
    let no_data: &[u8] = &[];
    let (kind, texts, data): (u32, [String; 3], &[u8]) = match &call.request {
        ServiceRequest::Http(request) => {
            let mut headers = String::new();
            for (name, value) in &request.headers {
                headers.push_str(name);
                headers.push_str(": ");
                headers.push_str(value);
                headers.push('\n');
            }
            timeout_ms = u32::try_from(request.timeout.as_millis())
                .unwrap_or(u32::MAX)
                .max(1);
            (
                0,
                [
                    request.method.as_str().to_owned(),
                    request.url.clone(),
                    headers,
                ],
                &request.body,
            )
        }
        ServiceRequest::StartRecording => (1, Default::default(), no_data),
        ServiceRequest::StopRecording => (2, Default::default(), no_data),
        ServiceRequest::Play(clip) => (
            3,
            [clip.mime.clone(), String::new(), String::new()],
            &clip.bytes,
        ),
        ServiceRequest::StopPlayback => (4, Default::default(), no_data),
        ServiceRequest::Store {
            key,
            value: Some(value),
        } => (5, [key.clone(), value.clone(), String::new()], no_data),
        ServiceRequest::Store { key, value: None } => {
            (6, [key.clone(), String::new(), String::new()], no_data)
        }
        ServiceRequest::Load { key } => (7, [key.clone(), String::new(), String::new()], no_data),
    };
    let strings = texts.map(|text| c_string(&text));
    *out = MorningstarServiceRequest {
        id: call.id.0,
        kind,
        timeout_ms,
        text1: strings[0].as_ptr(),
        text2: strings[1].as_ptr(),
        text3: strings[2].as_ptr(),
        data: if data.is_empty() {
            std::ptr::null()
        } else {
            data.as_ptr()
        },
        data_len: data.len(),
    };
    // Moving the call and the strings into the host moves neither the
    // heap buffer of the body nor those of the C strings, so the pointers
    // just written stay valid for as long as `lent` holds them.
    host.lent = Some(LentRequest {
        _call: call,
        _strings: strings,
    });
    true
}

/// Splits the host's header block (`Name: value` lines) into pairs.
fn parse_headers(block: &str) -> Vec<(String, String)> {
    block
        .lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_owned(), value.trim().to_owned()))
        .filter(|(name, _)| !name.is_empty())
        .collect()
}

/// The answer to an HTTP request. `error` is a `MORNINGSTAR_HTTP_*`
/// constant: 0 means the server answered, with `status`, `headers`
/// (`Name: value` lines) and `body`; anything else says why there is no
/// response, `message` describing a `MORNINGSTAR_HTTP_FAILED`.
///
/// Main thread, like every function here, and not after
/// [`morningstar_ios_destroy`]. A frame may follow: ask
/// [`morningstar_ios_needs_frame`], and drain
/// [`morningstar_ios_take_service_request`] again.
///
/// # Safety
/// `host` must be null or a live pointer from [`morningstar_ios_create`].
/// `headers` and `message` must each be null or a NUL-terminated string,
/// and `body` null or valid for reading `body_len` bytes; all are copied.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morningstar_ios_http_result(
    host: *mut MorningstarHost,
    id: u64,
    error: u32,
    status: u32,
    headers: *const c_char,
    body: *const u8,
    body_len: usize,
    message: *const c_char,
) {
    // SAFETY: guaranteed by the caller.
    let Some(host) = (unsafe { host.as_mut() }) else {
        return;
    };
    let result = match error {
        0 => Ok(HttpResponse {
            status: u16::try_from(status).unwrap_or(0),
            // SAFETY: null or NUL-terminated (caller).
            headers: parse_headers(&unsafe { text(headers) }),
            // SAFETY: null or valid for `body_len` bytes (caller).
            body: unsafe { bytes(body, body_len) },
        }),
        1 => Err(HttpError::Offline),
        2 => Err(HttpError::Timeout),
        3 => Err(HttpError::InvalidUrl),
        4 => Err(HttpError::TooLarge),
        5 => Err(HttpError::Unsupported),
        // 6, and whatever a newer host may add.
        // SAFETY: null or NUL-terminated (caller).
        _ => Err(HttpError::Failed(unsafe { text(message) })),
    };
    host.engine
        .service_result(ServiceId(id), ServiceResult::Http(result));
}

/// The answer to a recording or playback request. `kind` is a
/// `MORNINGSTAR_AUDIO_KIND_*` constant (0 recording started, 1 recording
/// stopped, 2 playback finished) and `error` a `MORNINGSTAR_AUDIO_*` one
/// (0 is success), `message` describing a `MORNINGSTAR_AUDIO_FAILED`. For
/// a stopped recording, `data` is the file, `mime` its media type and
/// `duration_ms` its length.
///
/// Main thread, and not after [`morningstar_ios_destroy`].
///
/// # Safety
/// `host` must be null or a live pointer from [`morningstar_ios_create`].
/// `message` and `mime` must each be null or a NUL-terminated string, and
/// `data` null or valid for reading `data_len` bytes; all are copied.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morningstar_ios_audio_result(
    host: *mut MorningstarHost,
    id: u64,
    kind: u32,
    error: u32,
    message: *const c_char,
    data: *const u8,
    data_len: usize,
    mime: *const c_char,
    duration_ms: u64,
) {
    // SAFETY: guaranteed by the caller.
    let Some(host) = (unsafe { host.as_mut() }) else {
        return;
    };
    let outcome = match error {
        0 => Ok(()),
        1 => Err(AudioError::PermissionDenied),
        2 => Err(AudioError::Busy),
        3 => Err(AudioError::Unsupported),
        4 => Err(AudioError::NotRecording),
        5 => Err(AudioError::Interrupted),
        // 6, and whatever a newer host may add.
        // SAFETY: null or NUL-terminated (caller).
        _ => Err(AudioError::Failed(unsafe { text(message) })),
    };
    let result = match kind {
        0 => ServiceResult::RecordingStarted(outcome),
        1 => ServiceResult::Recording(outcome.map(|()| Recording {
            // SAFETY: null or valid for `data_len` bytes (caller).
            bytes: unsafe { bytes(data, data_len) },
            // SAFETY: null or NUL-terminated (caller).
            mime: unsafe { text(mime) },
            duration: Duration::from_millis(duration_ms),
        })),
        2 => ServiceResult::Played(outcome),
        // A host newer than this library; ignoring is safer than guessing.
        _ => return,
    };
    host.engine.service_result(ServiceId(id), result);
}

/// The answer to a load: the stored value, or `found` false when the key
/// holds nothing.
///
/// Main thread, and not after [`morningstar_ios_destroy`].
///
/// # Safety
/// `host` must be null or a live pointer from [`morningstar_ios_create`];
/// `value` must be null or a NUL-terminated string, which is copied.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morningstar_ios_load_result(
    host: *mut MorningstarHost,
    id: u64,
    found: bool,
    value: *const c_char,
) {
    // SAFETY: guaranteed by the caller.
    let Some(host) = (unsafe { host.as_mut() }) else {
        return;
    };
    // SAFETY: null or NUL-terminated (caller).
    let value = found.then(|| unsafe { text(value) });
    host.engine
        .service_result(ServiceId(id), ServiceResult::Loaded(value));
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
