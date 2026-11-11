//! Minimal logcat output under the `Morningstar` tag.

use std::ffi::CString;

use crate::sys;

const TAG: &std::ffi::CStr = c"Morningstar";

pub fn info(message: &str) {
    write(sys::LOG_INFO, message);
}

pub fn error(message: &str) {
    write(sys::LOG_ERROR, message);
}

fn write(priority: std::ffi::c_int, message: &str) {
    // Interior NULs would truncate the message; replace them instead.
    let Ok(text) = CString::new(message.replace('\0', " ")) else {
        return;
    };
    // SAFETY: both pointers are valid NUL-terminated strings for the call.
    unsafe {
        sys::__android_log_write(priority, TAG.as_ptr(), text.as_ptr());
    }
}

/// Routes Rust panics to logcat. Without this a panic is invisible on a device.
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|panic| error(&format!("panic: {panic}"))));
}
