//! JNI entry points for `dev.morningstar.host.MorningstarNative`.
//!
//! Every function here is called on the Android main thread and returns
//! quickly: it only posts a message to the render thread. The one exception
//! is `nativeSurfaceDestroyed`, which Android requires to block until the
//! surface is no longer in use.

use std::sync::mpsc;

use morningstar_runtime::{
    ImeAction, Insets, Point, PointerEvent, PointerPhase, SurfaceSize, TextEditState,
    TextInputEvent, utf16_to_byte_offset,
};

use crate::java;
use crate::log;
use crate::render_thread::{self, Message};
use crate::sys::{JObject, JniEnv, NativeWindow};

/// # Safety
/// Called by the JVM with a valid `JNIEnv*` and the `MorningstarNative`
/// class, as for every static native method.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_dev_morningstar_host_MorningstarNative_nativeInit(
    env: JniEnv,
    class: JObject,
) {
    log::install_panic_hook();
    // Before the render thread starts, so that it can attach to the JVM.
    // SAFETY: the JVM passes this thread's env and the class this static
    // method belongs to.
    unsafe { java::init(env, class) };
    render_thread::start();
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
