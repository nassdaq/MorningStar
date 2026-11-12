//! JNI entry points for `dev.morningstar.host.MorningstarNative`.
//!
//! Every function here returns quickly: it only posts a message to the
//! render thread. The one exception is `nativeSurfaceDestroyed`, which
//! Android requires to block until the surface is no longer in use.
//!
//! They are called on the Android main thread, except the three that carry
//! the answer to a platform service request (`nativeHttpResult`,
//! `nativeAudioResult`, `nativeLoadResult`; ADR 0019), which the host calls
//! from whichever worker thread finished the work. That is sound: each
//! copies its arguments with the calling thread's own `JNIEnv*` and sends
//! them through the channel to the render thread, whose sender may be used
//! from any thread.

use std::sync::mpsc;

use std::time::Duration;

use morningstar_runtime::{
    AudioError, HostState, HttpError, HttpResponse, ImeAction, Insets, Point, PointerEvent,
    PointerPhase, Recording, ServiceId, ServiceResult, SurfaceSize, TextEditState, TextInputEvent,
    utf16_to_byte_offset,
};

use crate::java;
use crate::log;
use crate::render_thread::{self, Message};
use crate::sys::{JObject, JniEnv, NativeWindow};

/// Starts the runtime on the first call of the process; later calls change
/// nothing. `install_id` is the host's identifier for this installation
/// (32 hexadecimal digits) and `stored` the values kept on earlier runs, as
/// keys and values in turn: both are handed to the application at setup.
///
/// # Safety
/// Called by the JVM with a valid `JNIEnv*` and the `MorningstarNative`
/// class, as for every static native method, a `String` (or null) and a
/// `String[]` (or null).
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_dev_morningstar_host_MorningstarNative_nativeInit(
    env: JniEnv,
    class: JObject,
    install_id: JObject,
    stored: JObject,
) {
    log::install_panic_hook();
    // Before the render thread starts, so that it can attach to the JVM.
    // SAFETY: the JVM passes this thread's env and the class this static
    // method belongs to.
    unsafe { java::init(env, class) };
    if render_thread::started() {
        // The activity was recreated in a live process: the application
        // was set up long ago.
        return;
    }
    // SAFETY: the JVM passes this thread's env, a live String reference
    // (or null) and a live String[] reference (or null).
    let (install_id, stored) =
        unsafe { (java::string(env, install_id), java::strings(env, stored)) };
    let mut host = HostState::new().with_install_id(&install_id);
    let mut pairs = stored.into_iter();
    while let (Some(key), Some(value)) = (pairs.next(), pairs.next()) {
        host = host.with_stored(key, value);
    }
    log::info(&format!(
        "host state: install id {}, {} stored value(s)",
        if host.install_id() == install_id {
            "given"
        } else {
            "missing or malformed"
        },
        host.stored_len(),
    ));
    render_thread::start(host);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_morningstar_host_MorningstarNative_nativeResume(
    _env: JniEnv,
    _class: JObject,
) {
    render_thread::send(Message::Resume);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_morningstar_host_MorningstarNative_nativePause(
    _env: JniEnv,
    _class: JObject,
) {
    render_thread::send(Message::Pause);
}

/// # Safety
/// Called by the JVM with a valid `JNIEnv*` and an `android.view.Surface`.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_dev_morningstar_host_MorningstarNative_nativeSurfaceCreated(
    env: JniEnv,
    _class: JObject,
    surface: JObject,
) {
    // SAFETY: the JVM passes this thread's env and a live Surface reference.
    // The native window takes its own reference, so it stays valid after the
    // local `surface` reference is gone.
    match unsafe { NativeWindow::from_surface(env, surface) } {
        Some(window) => render_thread::send(Message::SurfaceCreated(window)),
        None => log::error("ANativeWindow_fromSurface returned null"),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_morningstar_host_MorningstarNative_nativeSurfaceChanged(
    _env: JniEnv,
    _class: JObject,
    width: i32,
    height: i32,
) {
    let size = SurfaceSize::new(
        u32::try_from(width).unwrap_or(0),
        u32::try_from(height).unwrap_or(0),
    );
    render_thread::send(Message::SurfaceChanged(size));
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_morningstar_host_MorningstarNative_nativeSurfaceDestroyed(
    _env: JniEnv,
    _class: JObject,
) {
    let (done, wait) = mpsc::sync_channel(1);
    render_thread::send(Message::SurfaceDestroyed(done));
    // Blocks for at most the frame in flight. An error means the render
    // thread is gone, in which case nothing is using the surface either.
    let _ = wait.recv();
}

/// The system wants the surface's contents drawn again.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_morningstar_host_MorningstarNative_nativeRequestRedraw(
    _env: JniEnv,
    _class: JObject,
) {
    render_thread::send(Message::Redraw);
}

/// `scale` is `DisplayMetrics.density`: physical pixels per dp.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_morningstar_host_MorningstarNative_nativeSetScale(
    _env: JniEnv,
    _class: JObject,
    scale: f32,
) {
    render_thread::send(Message::Scale(scale));
}

/// Safe-area insets in pixels, relative to the edges of the surface.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_morningstar_host_MorningstarNative_nativeSetInsets(
    _env: JniEnv,
    _class: JObject,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
) {
    render_thread::send(Message::Insets(Insets::new(
        left as f32,
        top as f32,
        right as f32,
        bottom as f32,
    )));
}

/// `continuous` is a JNI `jboolean`: zero is false, anything else true.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_morningstar_host_MorningstarNative_nativeSetContinuous(
    _env: JniEnv,
    _class: JObject,
    continuous: u8,
) {
    render_thread::send(Message::Continuous(continuous != 0));
}

/// `phase` uses the `PHASE_*` constants of `MorningstarNative.java`:
/// 0 down, 1 moved, 2 up, 3 cancelled. Coordinates are in pixels relative to
/// the surface.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_morningstar_host_MorningstarNative_nativePointer(
    _env: JniEnv,
    _class: JObject,
    phase: i32,
    pointer_id: i32,
    x: f32,
    y: f32,
) {
    let phase = match phase {
        0 => PointerPhase::Down,
        1 => PointerPhase::Moved,
        2 => PointerPhase::Up,
        3 => PointerPhase::Cancelled,
        // A host newer than this library; ignoring is safer than guessing.
        _ => return,
    };
    render_thread::send(Message::Pointer(PointerEvent {
        // Android pointer ids are small non-negative integers.
        id: u64::from(pointer_id.unsigned_abs()),
        phase,
        position: Point::new(x, y),
    }));
}

/// The system's Back gesture or key, which the host did not use to close a
/// keyboard. Like every other entry point this only posts: the application
/// answers on the render thread and the answer comes back through
/// `MorningstarNative.onBackResult` (ADR 0018).
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_morningstar_host_MorningstarNative_nativeBack(
    _env: JniEnv,
    _class: JObject,
) {
    render_thread::send(Message::Back);
}

/// How much of the bottom of the surface the soft keyboard covers, in
/// pixels; zero when it is hidden.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_morningstar_host_MorningstarNative_nativeSetKeyboardInset(
    _env: JniEnv,
    _class: JObject,
    bottom: i32,
) {
    render_thread::send(Message::KeyboardInset(bottom as f32));
}

/// The input method edited the host's copy of the focused field; this is
/// the whole result. Offsets are in UTF-16 code units; a composing start of
/// -1 means nothing is being composed. `serial` is the one the host was
/// given with the keyboard request this text belongs to.
///
/// # Safety
/// Called by the JVM with a valid `JNIEnv*` and a `String` (or null).
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_dev_morningstar_host_MorningstarNative_nativeTextEdited(
    env: JniEnv,
    _class: JObject,
    serial: i32,
    text: JObject,
    _selection_start: i32,
    selection_end: i32,
    composing_start: i32,
    composing_end: i32,
) {
    // SAFETY: the JVM passes this thread's env and a live String reference.
    let text = unsafe { java::string(env, text) };
    let offset = |utf16: i32| utf16_to_byte_offset(&text, usize::try_from(utf16).unwrap_or(0));
    // There is no selection yet (ADR 0016): its end is the caret.
    let caret = offset(selection_end);
    let composing = (composing_start >= 0 && composing_end > composing_start)
        .then(|| (offset(composing_start), offset(composing_end)));
    render_thread::send(Message::Text {
        serial: Some(serial),
        event: TextInputEvent::Edited(TextEditState {
            text,
            caret,
            composing,
        }),
    });
}

/// Text from a key event (a hardware keyboard, `adb shell input text`):
/// insert it at the caret.
///
/// # Safety
/// Called by the JVM with a valid `JNIEnv*` and a `String` (or null).
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_dev_morningstar_host_MorningstarNative_nativeTextCommit(
    env: JniEnv,
    _class: JObject,
    text: JObject,
) {
    // SAFETY: the JVM passes this thread's env and a live String reference.
    let text = unsafe { java::string(env, text) };
    if !text.is_empty() {
        render_thread::send(Message::Text {
            serial: None,
            event: TextInputEvent::Commit(text),
        });
    }
}

/// `command` uses the `TEXT_KEY_*` constants of `MorningstarNative.java`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_morningstar_host_MorningstarNative_nativeTextCommand(
    _env: JniEnv,
    _class: JObject,
    command: i32,
) {
    let event = match command {
        0 => TextInputEvent::Enter,
        1 => TextInputEvent::Action(ImeAction::Next),
        2 => TextInputEvent::Action(ImeAction::Done),
        3 => TextInputEvent::Action(ImeAction::Go),
        4 => TextInputEvent::Action(ImeAction::Search),
        5 => TextInputEvent::Action(ImeAction::Send),
        6 => TextInputEvent::DeleteBackward,
        7 => TextInputEvent::DeleteForward,
        8 => TextInputEvent::FocusLost,
        // A host newer than this library; ignoring is safer than guessing.
        _ => return,
    };
    render_thread::send(Message::Text {
        serial: None,
        event,
    });
}

// ------------------------------------------------------- platform services

/// Splits the host's header block (`Name: value` lines) into pairs.
fn parse_headers(block: &str) -> Vec<(String, String)> {
    block
        .lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_owned(), value.trim().to_owned()))
        .filter(|(name, _)| !name.is_empty())
        .collect()
}

/// The answer to an HTTP request. `error` uses the `HTTP_*` constants of
/// `MorningstarNative.java`: 0 means there is a response (`status`,
/// `headers` as `Name: value` lines, `body`); anything else says why there
/// is none, with `message` describing it for the log.
///
/// # Safety
/// Called by the JVM with a valid `JNIEnv*`, two `String`s (or null) and a
/// `byte[]` (or null). May be called on any thread.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_dev_morningstar_host_MorningstarNative_nativeHttpResult(
    env: JniEnv,
    _class: JObject,
    id: i64,
    error: i32,
    status: i32,
    headers: JObject,
    body: JObject,
    message: JObject,
) {
    let result = match error {
        // SAFETY: the JVM passes this thread's env and live references.
        0 => Ok(HttpResponse {
            status: u16::try_from(status).unwrap_or(0),
            headers: parse_headers(&unsafe { java::string(env, headers) }),
            body: unsafe { java::bytes(env, body) },
        }),
        1 => Err(HttpError::Offline),
        2 => Err(HttpError::Timeout),
        3 => Err(HttpError::InvalidUrl),
        4 => Err(HttpError::TooLarge),
        5 => Err(HttpError::Unsupported),
        // 6, and whatever a newer host may add.
        // SAFETY: as above.
        _ => Err(HttpError::Failed(unsafe { java::string(env, message) })),
    };
    render_thread::send(Message::Service {
        id: ServiceId(id.cast_unsigned()),
        result: ServiceResult::Http(result),
    });
}

/// The answer to an audio request. `kind` says which request (the
/// `AUDIO_KIND_*` constants: 0 recording started, 1 recording stopped,
/// 2 playback finished) and `error` how it went (`AUDIO_*`: 0 is success).
/// For a stopped recording, `data` is the file, `mime` its type and
/// `duration_millis` its length.
///
/// # Safety
/// Called by the JVM with a valid `JNIEnv*`, two `String`s (or null) and a
/// `byte[]` (or null). May be called on any thread.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_dev_morningstar_host_MorningstarNative_nativeAudioResult(
    env: JniEnv,
    _class: JObject,
    id: i64,
    kind: i32,
    error: i32,
    message: JObject,
    data: JObject,
    mime: JObject,
    duration_millis: i64,
) {
    let outcome = match error {
        0 => Ok(()),
        1 => Err(AudioError::PermissionDenied),
        2 => Err(AudioError::Busy),
        3 => Err(AudioError::Unsupported),
        4 => Err(AudioError::NotRecording),
        5 => Err(AudioError::Interrupted),
        // 6, and whatever a newer host may add.
        // SAFETY: the JVM passes this thread's env and a live reference.
        _ => Err(AudioError::Failed(unsafe { java::string(env, message) })),
    };
    let result = match kind {
        0 => ServiceResult::RecordingStarted(outcome),
        1 => ServiceResult::Recording(outcome.map(|()| Recording {
            // SAFETY: as above.
            bytes: unsafe { java::bytes(env, data) },
            // SAFETY: as above.
            mime: unsafe { java::string(env, mime) },
            duration: Duration::from_millis(u64::try_from(duration_millis).unwrap_or(0)),
        })),
        2 => ServiceResult::Played(outcome),
        // A host newer than this library; ignoring is safer than guessing.
        _ => return,
    };
    render_thread::send(Message::Service {
        id: ServiceId(id.cast_unsigned()),
        result,
    });
}

/// The answer to a load: the stored value, or `found` false (zero) when
/// the key holds nothing.
///
/// # Safety
/// Called by the JVM with a valid `JNIEnv*` and a `String` (or null). May
/// be called on any thread.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_dev_morningstar_host_MorningstarNative_nativeLoadResult(
    env: JniEnv,
    _class: JObject,
    id: i64,
    found: u8,
    value: JObject,
) {
    // SAFETY: the JVM passes this thread's env and a live String reference.
    let value = (found != 0).then(|| unsafe { java::string(env, value) });
    render_thread::send(Message::Service {
        id: ServiceId(id.cast_unsigned()),
        result: ServiceResult::Loaded(value),
    });
}
