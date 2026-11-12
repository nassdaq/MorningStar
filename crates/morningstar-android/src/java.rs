//! Calls from Rust into the Java host, and reading Java strings.
//!
//! Almost everything crosses the boundary the other way (Java calls the
//! functions in `jni.rs`). Text input is one exception: whether the soft
//! keyboard should be up is decided by the application, on the render
//! thread, when it builds a frame, and Android will only show a keyboard
//! from its main thread. So the render thread calls a static Java method,
//! `MorningstarNative.onTextInputCommand`, which posts to the main thread.
//! The system's Back is the other (ADR 0018): the application answers it on
//! the render thread, and the answer goes to `MorningstarNative.onBackResult`
//! the same way, so the main thread never waits for it. Platform services
//! are the third (ADR 0019): the application's requests (an HTTP request,
//! a recording, a clip to play, a value to store) are handed to
//! `MorningstarNative.onHttpRequest` and `onServiceCommand`, which pass
//! them to worker threads; the answers come back through native methods
//! like everything else. The clipboard is one of those services. The
//! last is the look of the system's bars (ADR 0026): whether their icons
//! should be light or dark over the frame just built goes to
//! `MorningstarNative.onSystemBars`. And what a frame means to assistive
//! technology (ADR 0032): when a screen reader is on, what changed in the
//! frame's semantics goes to `MorningstarNative.onSemanticsUpdate` as one
//! byte array, which the main thread keeps and answers TalkBack from.
//!
//! This is done with the JNI function tables directly rather than a binding
//! crate (ADR 0007): seventeen functions are needed. Their positions in the
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
//!   created here (the strings and the byte array of one call) is deleted
//!   before returning.
//! * Arrays are copied, never borrowed: `GetByteArrayRegion` and
//!   `SetByteArrayRegion` copy a stated number of elements into or out of
//!   a buffer of exactly that length, so no pointer into the Java heap is
//!   ever held and nothing needs releasing.
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
const GET_ARRAY_LENGTH: usize = 171;
const GET_OBJECT_ARRAY_ELEMENT: usize = 173;
const NEW_BYTE_ARRAY: usize = 176;
const GET_BYTE_ARRAY_REGION: usize = 200;
const SET_BYTE_ARRAY_REGION: usize = 208;
const GET_JAVA_VM: usize = 219;
const GET_STRING_REGION: usize = 220;
const EXCEPTION_CHECK: usize = 228;
// Position in `struct JNIInvokeInterface`, after its three reserved slots.
const ATTACH_CURRENT_THREAD_AS_DAEMON: usize = 7;

/// `JNI_OK`.
const OK: i32 = 0;

const METHOD_NAME: &CStr = c"onTextInputCommand";
/// `(int command, int serial, int keyboard, int capitalization, int action,
/// int flags, String text, int selectionStart, int selectionEnd)`,
/// returning void.
const METHOD_SIGNATURE: &CStr = c"(IIIIIILjava/lang/String;II)V";

const BACK_METHOD_NAME: &CStr = c"onBackResult";
/// `(boolean handled)`, returning void.
const BACK_METHOD_SIGNATURE: &CStr = c"(Z)V";

const HTTP_METHOD_NAME: &CStr = c"onHttpRequest";
/// `(long id, String method, String url, String headers, byte[] body,
/// int timeoutMillis)`, returning void.
const HTTP_METHOD_SIGNATURE: &CStr =
    c"(JLjava/lang/String;Ljava/lang/String;Ljava/lang/String;[BI)V";

const SERVICE_METHOD_NAME: &CStr = c"onServiceCommand";
/// `(int command, long id, String first, String second, byte[] data)`,
/// returning void.
const SERVICE_METHOD_SIGNATURE: &CStr = c"(IJLjava/lang/String;Ljava/lang/String;[B)V";

const BARS_METHOD_NAME: &CStr = c"onSystemBars";
/// `(boolean darkStatusIcons, boolean darkNavigationIcons)`, returning void.
const BARS_METHOD_SIGNATURE: &CStr = c"(ZZ)V";

const SEMANTICS_METHOD_NAME: &CStr = c"onSemanticsUpdate";
/// `(byte[] update)`, returning void.
const SEMANTICS_METHOD_SIGNATURE: &CStr = c"([B)V";

/// `jvalue`: one argument of a `Call...MethodA` function.
#[repr(C)]
#[derive(Clone, Copy)]
union JValue {
    int: i32,
    /// `jboolean`: an unsigned byte, zero for false.
    boolean: u8,
    object: JObject,
    /// `jlong`. Also the widest member: makes the union eight bytes, as
    /// in C.
    long: i64,
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
type GetArrayLength = unsafe extern "system" fn(JniEnv, JObject) -> i32;
type GetObjectArrayElement = unsafe extern "system" fn(JniEnv, JObject, i32) -> JObject;
type NewByteArray = unsafe extern "system" fn(JniEnv, i32) -> JObject;
type GetByteArrayRegion = unsafe extern "system" fn(JniEnv, JObject, i32, i32, *mut i8);
type SetByteArrayRegion = unsafe extern "system" fn(JniEnv, JObject, i32, i32, *const i8);
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
    /// `onBackResult`, or null if the host has none (one older than this
    /// library): a null method id is never called.
    back_method: MethodId,
    /// `onHttpRequest` and `onServiceCommand`, or null likewise: the
    /// application is then told its requests are unsupported.
    http_method: MethodId,
    service_method: MethodId,
    /// `onSystemBars`, or null likewise: the bars keep the host's look.
    bars_method: MethodId,
    /// `onSemanticsUpdate`, or null likewise: the application stays one
    /// opaque view to a screen reader.
    semantics_method: MethodId,
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
        let back_method = entry::<GetStaticMethodId>(env, GET_STATIC_METHOD_ID)(
            env,
            class,
            BACK_METHOD_NAME.as_ptr(),
            BACK_METHOD_SIGNATURE.as_ptr(),
        );
        if back_method.is_null() {
            // As above. The keyboard still works; Back cannot be answered.
            entry::<ExceptionVoid>(env, EXCEPTION_CLEAR)(env);
            log::error("MorningstarNative.onBackResult is missing: Back is not answered");
        }
        let optional = |name: &CStr, signature: &CStr, without: &str| {
            let method = entry::<GetStaticMethodId>(env, GET_STATIC_METHOD_ID)(
                env,
                class,
                name.as_ptr(),
                signature.as_ptr(),
            );
            if method.is_null() {
                // As above: an older host, and a pending NoSuchMethodError.
                entry::<ExceptionVoid>(env, EXCEPTION_CLEAR)(env);
                log::error(&format!(
                    "MorningstarNative.{} is missing: {without}",
                    name.to_string_lossy()
                ));
            }
            method
        };
        let http_method = optional(
            HTTP_METHOD_NAME,
            HTTP_METHOD_SIGNATURE,
            "HTTP requests are unsupported",
        );
        let service_method = optional(
            SERVICE_METHOD_NAME,
            SERVICE_METHOD_SIGNATURE,
            "audio, storage and the clipboard are unsupported",
        );
        let bars_method = optional(
            BARS_METHOD_NAME,
            BARS_METHOD_SIGNATURE,
            "the system bars keep their icons' colour",
        );
        let semantics_method = optional(
            SEMANTICS_METHOD_NAME,
            SEMANTICS_METHOD_SIGNATURE,
            "screen readers see one opaque view",
        );
        Host {
            vm,
            class,
            method,
            back_method,
            http_method,
            service_method,
            bars_method,
            semantics_method,
        }
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
    /// The selection in UTF-16 code units, which is what Java counts in:
    /// where it was started from, and the caret. Equal when nothing is
    /// selected.
    pub selection_start: i32,
    pub selection_end: i32,
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
                    JValue {
                        int: call.selection_start,
                    },
                    JValue {
                        int: call.selection_end,
                    },
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

    /// Calls `MorningstarNative.onBackResult` on this thread: the
    /// application's answer to the Back the host passed on with
    /// `nativeBack`. The Java method only posts to the main thread, so this
    /// returns at once. Returns false if the host could not be told (it has
    /// no such method).
    pub fn back_result(&self, handled: bool) -> bool {
        let Some(host) = HOST.get().filter(|host| !host.back_method.is_null()) else {
            return false;
        };
        let env = self.env;
        // SAFETY: `env` belongs to this thread (`Env` is not `Send`). The
        // class and the method id are valid for the life of the process,
        // and the method id was checked not to be null. The argument array
        // matches the method's signature: one `jboolean`. No local
        // reference is created, and an exception is cleared before
        // returning.
        unsafe {
            let arguments = [JValue {
                boolean: u8::from(handled),
            }];
            entry::<CallStaticVoidMethodA>(env, CALL_STATIC_VOID_METHOD_A)(
                env,
                host.class,
                host.back_method,
                arguments.as_ptr(),
            );
            if entry::<ExceptionCheck>(env, EXCEPTION_CHECK)(env) != 0 {
                entry::<ExceptionVoid>(env, EXCEPTION_DESCRIBE)(env);
                entry::<ExceptionVoid>(env, EXCEPTION_CLEAR)(env);
                log::error("the host threw from onBackResult");
            }
        }
        true
    }
}

impl Env {
    /// Calls `MorningstarNative.onSystemBars` on this thread: whether the
    /// status bar's and the navigation bar's icons should be dark (over a
    /// light screen). The Java method only posts to the main thread, so
    /// this returns at once. Returns false if the host could not be told
    /// (it has no such method).
    pub fn system_bars(&self, dark_status_icons: bool, dark_navigation_icons: bool) -> bool {
        let Some(host) = HOST.get().filter(|host| !host.bars_method.is_null()) else {
            return false;
        };
        let env = self.env;
        // SAFETY: `env` belongs to this thread (`Env` is not `Send`). The
        // class and the method id are valid for the life of the process,
        // and the method id was checked not to be null. The argument array
        // matches the method's signature: two `jboolean`s. No local
        // reference is created, and an exception is cleared before
        // returning.
        unsafe {
            let arguments = [
                JValue {
                    boolean: u8::from(dark_status_icons),
                },
                JValue {
                    boolean: u8::from(dark_navigation_icons),
                },
            ];
            entry::<CallStaticVoidMethodA>(env, CALL_STATIC_VOID_METHOD_A)(
                env,
                host.class,
                host.bars_method,
                arguments.as_ptr(),
            );
            if entry::<ExceptionCheck>(env, EXCEPTION_CHECK)(env) != 0 {
                entry::<ExceptionVoid>(env, EXCEPTION_DESCRIBE)(env);
                entry::<ExceptionVoid>(env, EXCEPTION_CLEAR)(env);
                log::error("the host threw from onSystemBars");
            }
        }
        true
    }
}

/// One HTTP request for `MorningstarNative.onHttpRequest`.
pub struct HttpCall<'a> {
    pub id: i64,
    pub method: &'a str,
    pub url: &'a str,
    /// `Name: value` lines, each ended by a line feed.
    pub headers: &'a str,
    pub body: &'a [u8],
    pub timeout_millis: i32,
}

/// One request for `MorningstarNative.onServiceCommand`: a `SERVICE_*`
/// command, the id its answer must carry, and whichever of two strings and
/// one byte array the command uses (the rest empty).
pub struct ServiceCommand<'a> {
    pub command: i32,
    pub id: i64,
    pub first: &'a str,
    pub second: &'a str,
    pub data: &'a [u8],
}

impl Env {
    /// Calls `MorningstarNative.onHttpRequest` on this thread. The Java
    /// method only hands the request to a worker thread, so this returns
    /// at once. Returns false if the host could not be given the request
    /// (it has no such method, or the arguments could not be made): the
    /// caller then answers the application itself.
    pub fn http_request(&self, call: &HttpCall<'_>) -> bool {
        let Some(host) = HOST.get().filter(|host| !host.http_method.is_null()) else {
            return false;
        };
        // SAFETY: `self.env` belongs to this thread (`Env` is not `Send`).
        // The class and the method id are valid for the life of the process
        // and the id is not null. The argument array matches the method's
        // signature in order and in type: long, three strings, a byte
        // array, int. `call_with` creates the references, deletes them and
        // clears any exception.
        unsafe {
            self.call_with(
                host,
                host.http_method,
                [call.method, call.url, call.headers],
                call.body,
                |[method, url, headers], body| {
                    [
                        JValue { long: call.id },
                        JValue { object: method },
                        JValue { object: url },
                        JValue { object: headers },
                        JValue { object: body },
                        JValue {
                            int: call.timeout_millis,
                        },
                    ]
                },
                "onHttpRequest",
            )
        }
    }

    /// Calls `MorningstarNative.onServiceCommand` on this thread; as
    /// [`http_request`](Self::http_request) in every other respect.
    pub fn service_command(&self, call: &ServiceCommand<'_>) -> bool {
        let Some(host) = HOST.get().filter(|host| !host.service_method.is_null()) else {
            return false;
        };
        // SAFETY: as in `http_request`. The argument array matches the
        // signature: int, long, two strings, a byte array.
        unsafe {
            self.call_with(
                host,
                host.service_method,
                [call.first, call.second],
                call.data,
                |[first, second], data| {
                    [
                        JValue { int: call.command },
                        JValue { long: call.id },
                        JValue { object: first },
                        JValue { object: second },
                        JValue { object: data },
                    ]
                },
                "onServiceCommand",
            )
        }
    }

    /// Calls `MorningstarNative.onSemanticsUpdate` on this thread with one
    /// update of the semantics tree, in the format of
    /// `SemanticsTree::encode_update`. The Java method copies nothing and
    /// only posts the array to the main thread, so this returns at once.
    /// Returns false if the host could not be given it.
    pub fn semantics_update(&self, update: &[u8]) -> bool {
        let Some(host) = HOST.get().filter(|host| !host.semantics_method.is_null()) else {
            return false;
        };
        // SAFETY: as in `http_request`. The argument array matches the
        // signature: one byte array. `call_with` makes that array as a
        // copy of `update` (a new `byte[]` of exactly its length), so the
        // Java side owns what it is given and nothing of `update` is
        // referred to after this returns.
        unsafe {
            self.call_with(
                host,
                host.semantics_method,
                [],
                update,
                |[], update| [JValue { object: update }],
                "onSemanticsUpdate",
            )
        }
    }

    /// Makes `S` Java strings and one Java byte array, calls a static void
    /// method of the host class with the arguments `arguments` builds from
    /// them, and deletes the references again. Returns false, having
    /// called nothing, if any of them could not be made (out of memory, or
    /// longer than a Java array can be).
    ///
    /// # Safety
    /// `method` must be a valid, non-null id of a static void method of
    /// `host.class`, and the array `arguments` returns must match that
    /// method's signature in number, order and type.
    unsafe fn call_with<const S: usize, const N: usize>(
        &self,
        host: &Host,
        method: MethodId,
        strings: [&str; S],
        bytes: &[u8],
        arguments: impl FnOnce([JObject; S], JObject) -> [JValue; N],
        name: &str,
    ) -> bool {
        let env = self.env;
        let Ok(byte_count) = i32::try_from(bytes.len()) else {
            log::error(&format!(
                "{name}: {} bytes do not fit a Java array",
                bytes.len()
            ));
            return false;
        };
        let mut locals = [std::ptr::null_mut(); S];
        let mut complete = true;
        // SAFETY: `env` belongs to this thread. Each string is created from
        // a live UTF-16 buffer of the stated length. The byte array is
        // created with `byte_count` elements and exactly that many are
        // copied into it from a slice of that length (`u8` and `jbyte`
        // have the same size and alignment). Once a creation has failed an
        // exception is pending, and no JNI function other than the
        // exception ones and `DeleteLocalRef` is called after that. The
        // method contract is the caller's. Every local reference made is
        // deleted, and an exception is cleared, before returning.
        unsafe {
            for (local, text) in locals.iter_mut().zip(strings) {
                if !complete {
                    break;
                }
                let units: Vec<u16> = text.encode_utf16().collect();
                match i32::try_from(units.len()) {
                    Ok(length) => {
                        *local = entry::<NewString>(env, NEW_STRING)(env, units.as_ptr(), length);
                    }
                    Err(_) => log::error(&format!("{name}: a string is too long for Java")),
                }
                complete = !local.is_null();
            }
            let mut array = std::ptr::null_mut();
            if complete {
                array = entry::<NewByteArray>(env, NEW_BYTE_ARRAY)(env, byte_count);
                complete = !array.is_null();
            }
            if complete && byte_count > 0 {
                entry::<SetByteArrayRegion>(env, SET_BYTE_ARRAY_REGION)(
                    env,
                    array,
                    0,
                    byte_count,
                    bytes.as_ptr().cast::<i8>(),
                );
                complete = entry::<ExceptionCheck>(env, EXCEPTION_CHECK)(env) == 0;
            }
            if complete {
                let arguments = arguments(locals, array);
                entry::<CallStaticVoidMethodA>(env, CALL_STATIC_VOID_METHOD_A)(
                    env,
                    host.class,
                    method,
                    arguments.as_ptr(),
                );
            }
            // Out of memory while making the arguments, or anything thrown
            // by the method.
            let threw = entry::<ExceptionCheck>(env, EXCEPTION_CHECK)(env) != 0;
            if threw {
                entry::<ExceptionVoid>(env, EXCEPTION_DESCRIBE)(env);
                entry::<ExceptionVoid>(env, EXCEPTION_CLEAR)(env);
                log::error(&format!("the host threw from {name}"));
            }
            for local in locals.into_iter().chain([array]) {
                if !local.is_null() {
                    entry::<DeleteLocalRef>(env, DELETE_LOCAL_REF)(env, local);
                }
            }
            // A method that threw did not take the request on.
            complete && !threw
        }
    }
}

/// Copies a Java `byte[]`. A null reference is the empty array.
///
/// # Safety
/// `env` must be the calling thread's `JNIEnv*` and `array` null or a
/// valid reference to a `byte[]`.
pub unsafe fn bytes(env: JniEnv, array: JObject) -> Vec<u8> {
    if array.is_null() {
        return Vec::new();
    }
    // SAFETY: `env` and `array` are valid (caller). `GetByteArrayRegion`
    // copies exactly `length` elements into a buffer of exactly that
    // length; `u8` and `jbyte` have the same size and alignment.
    unsafe {
        let length = entry::<GetArrayLength>(env, GET_ARRAY_LENGTH)(env, array);
        let mut bytes = vec![0u8; usize::try_from(length).unwrap_or(0)];
        if !bytes.is_empty() {
            entry::<GetByteArrayRegion>(env, GET_BYTE_ARRAY_REGION)(
                env,
                array,
                0,
                length,
                bytes.as_mut_ptr().cast::<i8>(),
            );
        }
        bytes
    }
}

/// Copies a Java `String[]`. A null reference is the empty array; a null
/// element is the empty string.
///
/// # Safety
/// `env` must be the calling thread's `JNIEnv*` and `array` null or a
/// valid reference to a `String[]`.
pub unsafe fn strings(env: JniEnv, array: JObject) -> Vec<String> {
    if array.is_null() {
        return Vec::new();
    }
    // SAFETY: `env` and `array` are valid (caller). Every index is below
    // the array's length, so each element is null or a `String`, which is
    // what `string` requires. Each element's local reference is deleted
    // once copied, so a long array cannot exhaust the local reference
    // table of the native frame.
    unsafe {
        let length = entry::<GetArrayLength>(env, GET_ARRAY_LENGTH)(env, array);
        (0..length.max(0))
            .map(|index| {
                let element = entry::<GetObjectArrayElement>(env, GET_OBJECT_ARRAY_ELEMENT)(
                    env, array, index,
                );
                let text = string(env, element);
                if !element.is_null() {
                    entry::<DeleteLocalRef>(env, DELETE_LOCAL_REF)(env, element);
                }
                text
            })
            .collect()
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
