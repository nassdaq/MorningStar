//! Calls from Rust into the Java host, and reading Java strings.
//!
//! Almost everything crosses the boundary the other way (Java calls the
//! functions in `jni.rs`). Text input is the exception: whether the soft
//! keyboard should be up is decided by the application, on the render
//! thread, when it builds a frame, and Android will only show a keyboard
//! from its main thread. So the render thread calls one static Java method,
//! `MorningstarNative.onTextInputCommand`, which posts to the main thread.
//!
//! This is done with the JNI function tables directly rather than a binding
//! crate (ADR 0007): twelve functions are needed. Their positions in the
//! tables are fixed by the JNI specification and never change; they were
//! checked against `jni.h` of NDK 27 (see the constants below).
//!
//! # The unsafe boundary
//!
//! * A `JNIEnv*` is valid only on the thread it was given to. `Env` wraps
//!   one and is neither `Send` nor `Sync`; it is created from the pointer
//!   the JVM passes to a native method, or by attaching the current thread.
//! * `JavaVM*` is process-wide and usable from any thread; so are global
//!   references and method ids (a method id stays valid while its class is
//!   loaded, which the global reference to the class guarantees). They are
//!   stored once in `HOST` and never released: the host class lives as long
//!   as the process.
//! * A native thread attached to the JVM never returns to Java, so local
//!   references it creates are not freed for it. Every local reference
//!   created here (one string per call) is deleted before returning.
//! * A Java exception thrown by the upcall would otherwise stay pending on
//!   the render thread and make the next JNI call abort the process. It is
//!   described to the log and cleared.

use std::ffi::{CStr, c_char, c_void};
use std::marker::PhantomData;
use std::sync::OnceLock;

use crate::log;
use crate::sys::{JObject, JniEnv};

// Positions in `struct JNINativeInterface` (jni.h), counting the four
// reserved slots at its start.
const EXCEPTION_DESCRIBE: usize = 16;
const EXCEPTION_CLEAR: usize = 17;
const NEW_GLOBAL_REF: usize = 21;
const DELETE_LOCAL_REF: usize = 23;
const GET_STATIC_METHOD_ID: usize = 113;
const CALL_STATIC_VOID_METHOD_A: usize = 143;
const NEW_STRING: usize = 163;
const GET_STRING_LENGTH: usize = 164;
const GET_JAVA_VM: usize = 219;
const GET_STRING_REGION: usize = 220;
const EXCEPTION_CHECK: usize = 228;
// Position in `struct JNIInvokeInterface`, after its three reserved slots.
const ATTACH_CURRENT_THREAD_AS_DAEMON: usize = 7;

/// `JNI_OK`.
const OK: i32 = 0;

const METHOD_NAME: &CStr = c"onTextInputCommand";
/// `(int command, int serial, int keyboard, int capitalization, int action,
/// int flags, String text, int caret)`, returning void.
const METHOD_SIGNATURE: &CStr = c"(IIIIIILjava/lang/String;I)V";

/// `jvalue`: one argument of a `Call...MethodA` function.
#[repr(C)]
#[derive(Clone, Copy)]
union JValue {
    int: i32,
    object: JObject,
    /// The widest member: makes the union eight bytes, as in C.
    _long: i64,
}

type JavaVm = *mut c_void;
type MethodId = *mut c_void;

type GetJavaVm = unsafe extern "system" fn(JniEnv, *mut JavaVm) -> i32;
type NewGlobalRef = unsafe extern "system" fn(JniEnv, JObject) -> JObject;
type DeleteLocalRef = unsafe extern "system" fn(JniEnv, JObject);
type GetStaticMethodId =
    unsafe extern "system" fn(JniEnv, JObject, *const c_char, *const c_char) -> MethodId;
type CallStaticVoidMethodA = unsafe extern "system" fn(JniEnv, JObject, MethodId, *const JValue);
type NewString = unsafe extern "system" fn(JniEnv, *const u16, i32) -> JObject;
type GetStringLength = unsafe extern "system" fn(JniEnv, JObject) -> i32;
type GetStringRegion = unsafe extern "system" fn(JniEnv, JObject, i32, i32, *mut u16);
type ExceptionCheck = unsafe extern "system" fn(JniEnv) -> u8;
type ExceptionVoid = unsafe extern "system" fn(JniEnv);
type AttachCurrentThread = unsafe extern "system" fn(JavaVm, *mut JniEnv, *mut c_void) -> i32;

/// Reads entry `index` of the function table an interface pointer (a
/// `JNIEnv*` or a `JavaVM*`) points to, as a function pointer of type `F`.
///
/// # Safety
/// `interface` must be a valid `JNIEnv*` or `JavaVM*`, and `F` must be the
/// `extern "system"` function-pointer type the JNI specification gives the
/// entry at `index` of that table.
unsafe fn entry<F: Copy>(interface: *mut c_void, index: usize) -> F {
    const { assert!(size_of::<F>() == size_of::<*const c_void>()) };
    // SAFETY: an interface pointer points to a pointer to an array of
    // function pointers that has more than `index` entries (caller).
    unsafe {
        let table = *interface.cast::<*const *const c_void>();
        std::mem::transmute_copy::<*const c_void, F>(&*table.add(index))
    }
}

/// What is needed to call the host from any thread.
struct Host {
    vm: JavaVm,
    /// Global reference to `dev.morningstar.host.MorningstarNative`.
    class: JObject,
    method: MethodId,
}

// SAFETY: a `JavaVM*`, a global reference and a method id are valid on
// every thread for the life of the process (see the module documentation),
// and nothing is ever written through them here.
unsafe impl Send for Host {}
// SAFETY: as above; the struct is immutable after creation.
unsafe impl Sync for Host {}

static HOST: OnceLock<Host> = OnceLock::new();

/// Remembers the JVM and the host class. Called from `nativeInit`; later
/// calls change nothing.
///
/// # Safety
/// `env` must be the calling thread's `JNIEnv*` and `class` a valid
/// reference to the `MorningstarNative` class: exactly what the JVM passes
/// to a static native method of that class.
pub unsafe fn init(env: JniEnv, class: JObject) {
    if HOST.get().is_some() {
        return;
    }
    // SAFETY: `env` is valid on this thread (caller); each function is
    // read from its specified slot with its specified type. The class
    // reference is valid for this call, and `NewGlobalRef` makes one that
    // outlives it. The name and signature are NUL-terminated.
    let host = unsafe {
        let mut vm: JavaVm = std::ptr::null_mut();
        if entry::<GetJavaVm>(env, GET_JAVA_VM)(env, &mut vm) != OK || vm.is_null() {
            log::error("GetJavaVM failed: the keyboard cannot be shown");
            return;
        }
        let class = entry::<NewGlobalRef>(env, NEW_GLOBAL_REF)(env, class);
        if class.is_null() {
            log::error("NewGlobalRef failed: the keyboard cannot be shown");
            return;
        }
        let method = entry::<GetStaticMethodId>(env, GET_STATIC_METHOD_ID)(
            env,
            class,
            METHOD_NAME.as_ptr(),
            METHOD_SIGNATURE.as_ptr(),
        );
        if method.is_null() {
            // A host older than this library. The lookup left a
            // NoSuchMethodError pending; clear it so Java carries on.
            entry::<ExceptionVoid>(env, EXCEPTION_CLEAR)(env);
            log::error("MorningstarNative.onTextInputCommand is missing: no keyboard");
            return;
        }
        Host { vm, class, method }
    };
    let _ = HOST.set(host);
}

/// The JNI environment of one thread. Not `Send`: it must stay there.
pub struct Env {
    env: JniEnv,
    _not_send: PhantomData<*mut c_void>,
}

/// One request to the host's keyboard, as the integers the Java side
/// decodes (`MorningstarNative.TEXT_*`).
pub struct KeyboardCall<'a> {
    pub command: i32,
    pub serial: i32,
    pub keyboard: i32,
    pub capitalization: i32,
    pub action: i32,
    pub flags: i32,
    pub text: &'a str,
    /// The caret in UTF-16 code units, which is what Java counts in.
    pub caret: i32,
}

impl Env {
    /// Attaches the calling thread to the JVM, for good, and returns its
    /// environment. `None` if the host was never initialised or the JVM
    /// refuses. The thread is attached as a daemon so that it never keeps
    /// the JVM from shutting down; it is never detached because it lives as
    /// long as the process.
    pub fn attach_current_thread() -> Option<Self> {
        let host = HOST.get()?;
        let mut env: JniEnv = std::ptr::null_mut();
        // SAFETY: `host.vm` is the process's `JavaVM*`; the function is
        // read from its specified slot with its specified type; `env` is a
        // valid place for the result and a null argument block is allowed.
        let status = unsafe {
            entry::<AttachCurrentThread>(host.vm, ATTACH_CURRENT_THREAD_AS_DAEMON)(
                host.vm,
                &mut env,
                std::ptr::null_mut(),
            )
        };
        (status == OK && !env.is_null()).then_some(Self {
            env,
            _not_send: PhantomData,
        })
    }

    /// Calls `MorningstarNative.onTextInputCommand` on this thread. The
    /// Java method only posts to the main thread, so this returns at once.
    pub fn keyboard(&self, call: &KeyboardCall<'_>) {
        let Some(host) = HOST.get() else {
            return;
        };
        let text: Vec<u16> = call.text.encode_utf16().collect();
        let Ok(length) = i32::try_from(text.len()) else {
            return;
        };
        let env = self.env;
        // SAFETY: `env` belongs to this thread (`Env` is not `Send`). The
        // string is created from a live buffer of `length` units. The
        // argument array matches the method's signature, in order and in
        // type. The one local reference made is deleted, and an exception
        // is cleared, before returning.
        unsafe {
            let string = entry::<NewString>(env, NEW_STRING)(env, text.as_ptr(), length);
            if !string.is_null() {
                let arguments = [
                    JValue { int: call.command },
                    JValue { int: call.serial },
                    JValue { int: call.keyboard },
                    JValue {
                        int: call.capitalization,
                    },
                    JValue { int: call.action },
                    JValue { int: call.flags },
                    JValue { object: string },
                    JValue { int: call.caret },
                ];
                entry::<CallStaticVoidMethodA>(env, CALL_STATIC_VOID_METHOD_A)(
                    env,
                    host.class,
                    host.method,
                    arguments.as_ptr(),
                );
                entry::<DeleteLocalRef>(env, DELETE_LOCAL_REF)(env, string);
            }
            // Out of memory in NewString, or anything thrown by the method.
            if entry::<ExceptionCheck>(env, EXCEPTION_CHECK)(env) != 0 {
                entry::<ExceptionVoid>(env, EXCEPTION_DESCRIBE)(env);
                entry::<ExceptionVoid>(env, EXCEPTION_CLEAR)(env);
                log::error("the host threw from onTextInputCommand");
            }
        }
    }
}

/// Copies a Java string. Java strings are UTF-16; an unpaired surrogate
/// becomes U+FFFD. A null reference is the empty string.
///
/// # Safety
/// `env` must be the calling thread's `JNIEnv*` and `string` null or a
/// valid reference to a `java.lang.String`.
pub unsafe fn string(env: JniEnv, string: JObject) -> String {
    if string.is_null() {
        return String::new();
    }
    // SAFETY: `env` and `string` are valid (caller). `GetStringRegion`
    // copies exactly `length` units into a buffer of exactly that length.
    unsafe {
        let length = entry::<GetStringLength>(env, GET_STRING_LENGTH)(env, string);
        let mut units = vec![0u16; usize::try_from(length).unwrap_or(0)];
        if !units.is_empty() {
            entry::<GetStringRegion>(env, GET_STRING_REGION)(
                env,
                string,
                0,
                length,
                units.as_mut_ptr(),
            );
        }
        String::from_utf16_lossy(&units)
    }
}
