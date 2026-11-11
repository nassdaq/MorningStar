//! Android host integration.
//!
//! The Java side (`templates/android`, class `dev.morningstar.host.MorningstarNative`)
//! is a thin shell: an `Activity` with a `SurfaceView` that forwards lifecycle
//! and surface callbacks to the JNI functions in [`jni`]. Each callback becomes
//! a message to a dedicated render thread, which owns the [`Engine`] and is the
//! only thread that touches the GPU. The Android main thread never renders.
//!
//! The crate is empty on every other target so the workspace builds on a
//! development host.
//!
//! [`Engine`]: morningstar_runtime::Engine
#![cfg(target_os = "android")]

mod java;
mod jni;
mod log;
mod render_thread;
mod sys;
