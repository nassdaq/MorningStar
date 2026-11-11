//! The NDK functions the Android host links against: four window functions
//! and one logging function, plus an owning wrapper for the window. The
//! other unsafe surface, the JNI calls into Java for the keyboard, is in
//! `java.rs`.

use std::ffi::{c_char, c_int, c_void};
use std::ptr::NonNull;

use morningstar_runtime::SurfaceSize;

/// `JNIEnv*`. Only ever passed through to the NDK, never dereferenced here.
pub type JniEnv = *mut c_void;
/// `jobject` / `jclass`.
pub type JObject = *mut c_void;

/// `ANDROID_LOG_INFO` / `ANDROID_LOG_ERROR` from `<android/log.h>`.
pub const LOG_INFO: c_int = 4;
pub const LOG_ERROR: c_int = 6;

#[link(name = "android")]
unsafe extern "C" {
    fn ANativeWindow_fromSurface(env: JniEnv, surface: JObject) -> *mut c_void;
    fn ANativeWindow_release(window: *mut c_void);
    fn ANativeWindow_getWidth(window: *mut c_void) -> i32;
    fn ANativeWindow_getHeight(window: *mut c_void) -> i32;
}

#[link(name = "log")]
unsafe extern "C" {
    pub fn __android_log_write(priority: c_int, tag: *const c_char, text: *const c_char) -> c_int;
}

/// An owned reference to an `ANativeWindow`. Releases it on drop.
pub struct NativeWindow(NonNull<c_void>);

// SAFETY: `ANativeWindow` is reference counted and its functions are
// thread-safe; the NDK explicitly supports using one from a render thread
// other than the thread that obtained it.
unsafe impl Send for NativeWindow {}

impl NativeWindow {
    /// Acquires the native window behind an `android.view.Surface`.
    ///
    /// # Safety
    /// `env` must be the `JNIEnv*` of the calling thread and `surface` a valid
    /// local or global reference to an `android.view.Surface`.
    pub unsafe fn from_surface(env: JniEnv, surface: JObject) -> Option<Self> {
        // SAFETY: guaranteed by the caller. Returns null on failure, and
        // otherwise a pointer carrying a reference we now own.
        NonNull::new(unsafe { ANativeWindow_fromSurface(env, surface) }).map(Self)
    }

    pub fn as_ptr(&self) -> NonNull<c_void> {
        self.0
    }

    pub fn size(&self) -> SurfaceSize {
        // SAFETY: `self.0` is a live window for as long as `self` exists.
        let (width, height) = unsafe {
            (
                ANativeWindow_getWidth(self.0.as_ptr()),
                ANativeWindow_getHeight(self.0.as_ptr()),
            )
        };
        // Both return a negative value on error; treat that as empty.
        SurfaceSize::new(
            u32::try_from(width).unwrap_or(0),
            u32::try_from(height).unwrap_or(0),
        )
    }
}

impl Drop for NativeWindow {
    fn drop(&mut self) {
        // SAFETY: releases exactly the reference acquired in `from_surface`.
        unsafe { ANativeWindow_release(self.0.as_ptr()) }
    }
}
