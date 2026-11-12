package dev.morningstar.host;

import android.os.Handler;
import android.os.Looper;
import android.view.Surface;

/**
 * The complete interface between Java and Rust. The native methods are
 * implemented in crates/morningstar-android/src/jni.rs; the four methods
 * Rust calls (onTextInputCommand, onBackResult, onHttpRequest and
 * onServiceCommand) are looked up by name and signature in
 * crates/morningstar-android/src/java.rs. Both sides must change together.
 *
 * All native methods are called on the main thread and return immediately,
 * except nativeSurfaceDestroyed, which blocks until the renderer has let go
 * of the surface, and the three results of platform services
 * (nativeHttpResult, nativeAudioResult, nativeLoadResult), which are called
 * from the worker thread that did the work.
 *
 * Rust draws on demand: after any of these calls that can change the picture
 * it produces a frame, and between them it draws nothing.
 */
final class MorningstarNative {
    private MorningstarNative() {}

    /**
     * Starts the Rust runtime. Safe to call more than once per process:
     * only the first call does anything. installId is this installation's
     * identifier (32 hexadecimal digits) and stored the application's
     * stored values as keys and values in turn; the application is handed
     * both when it is set up.
     */
    static native void nativeInit(String installId, String[] stored);

    /** The activity is visible; frames may be produced. */
    static native void nativeResume();

    /** The activity is no longer visible; frame production stops. */
    static native void nativePause();

    static native void nativeSurfaceCreated(Surface surface);

    static native void nativeSurfaceChanged(int width, int height);

    static native void nativeSurfaceDestroyed();

    /** The system asked for the surface's contents to be drawn again. */
    static native void nativeRequestRedraw();

    /**
     * Physical pixels per dp (DisplayMetrics.density). Call at start and
     * whenever it may have changed; repeating the current value is free.
     */
    static native void nativeSetScale(float scale);

    /**
     * The parts of the surface covered by system bars or a display cutout,
     * in pixels from each edge. Call whenever they may have changed.
     */
    static native void nativeSetInsets(int left, int top, int right, int bottom);

    /** Benchmark mode: draw every display refresh even if nothing changed. */
    static native void nativeSetContinuous(boolean continuous);

    // Values of nativePointer's phase argument. Must match jni.rs.
    static final int PHASE_DOWN = 0;
    static final int PHASE_MOVED = 1;
    static final int PHASE_UP = 2;
    static final int PHASE_CANCELLED = 3;

    /** One touch event for one pointer, in pixels relative to the surface. */
    static native void nativePointer(int phase, int pointerId, float x, float y);

    // ---------------------------------------------------------------- back

    /**
     * The system's Back gesture or key, not absorbed by a keyboard. Returns
     * at once; the application's answer arrives later in onBackResult.
     */
    static native void nativeBack();

    // ---------------------------------------------------------- text input

    /**
     * The soft keyboard's height over the bottom of the surface, in pixels
     * (the navigation bar under it included); 0 while it is hidden.
     */
    static native void nativeSetKeyboardInset(int bottom);

    /**
     * The input method changed the host's copy of the focused field: its
     * whole text, the selection and the composing (pre-edit) range, in
     * UTF-16 code units; composingStart is -1 when nothing is being
     * composed. serial is the one given with the SHOW command the edit was
     * made under.
     */
    static native void nativeTextEdited(
            int serial,
            String text,
            int selectionStart,
            int selectionEnd,
            int composingStart,
            int composingEnd);

    /** Text typed as key events (hardware keyboard, adb): insert it. */
    static native void nativeTextCommit(String text);

    // Values of nativeTextCommand's argument. Must match jni.rs. 1 to 5 are
    // also the values of onTextInputCommand's action argument.
    static final int TEXT_KEY_ENTER = 0;
    static final int TEXT_KEY_NEXT = 1;
    static final int TEXT_KEY_DONE = 2;
    static final int TEXT_KEY_GO = 3;
    static final int TEXT_KEY_SEARCH = 4;
    static final int TEXT_KEY_SEND = 5;
    static final int TEXT_KEY_DELETE_BACKWARD = 6;
    static final int TEXT_KEY_DELETE_FORWARD = 7;
    /** The user put the keyboard away: the field should give up focus. */
    static final int TEXT_KEY_FOCUS_LOST = 8;

    /** An editing key, an action key, or the loss of focus. */
    static native void nativeTextCommand(int command);

    // Values of onTextInputCommand's arguments. Must match render_thread.rs.
    static final int TEXT_HIDE = 0;
    static final int TEXT_SHOW = 1;
    static final int TEXT_UPDATE = 2;
    static final int KEYBOARD_TEXT = 0;
    static final int KEYBOARD_NUMBER = 1;
    static final int KEYBOARD_DECIMAL = 2;
    static final int KEYBOARD_PHONE = 3;
    static final int KEYBOARD_EMAIL = 4;
    static final int CAPITALIZE_NONE = 0;
    static final int CAPITALIZE_SENTENCES = 1;
    static final int CAPITALIZE_WORDS = 2;
    static final int CAPITALIZE_CHARACTERS = 3;
    static final int FLAG_SECURE = 1;
    static final int FLAG_MULTILINE = 2;
    static final int FLAG_AUTOCORRECT = 4;

    private static final Handler MAIN = new Handler(Looper.getMainLooper());

    /**
     * Called by Rust, on its render thread, when the frame it has just built
     * gained, changed or lost a focused text field. Showing a keyboard is
     * main-thread work, so this only posts.
     *
     * TEXT_SHOW: a field has focus; show the keyboard described by the
     * arguments, holding text with the caret at caret (UTF-16 units).
     * TEXT_UPDATE: Rust changed the same field's text or caret; replace the
     * host's copy. TEXT_HIDE: no field has focus.
     *
     * Looked up by name through JNI: keep it out of any code shrinker.
     */
    @SuppressWarnings("unused") // Called from native code.
    static void onTextInputCommand(
            final int command,
            final int serial,
            final int keyboard,
            final int capitalization,
            final int action,
            final int flags,
            final String text,
            final int caret) {
        MAIN.post(
                () -> {
                    MorningstarActivity activity = MorningstarActivity.current();
                    if (activity != null) {
                        activity.onTextInputCommand(
                                command, serial, keyboard, capitalization, action, flags, text,
                                caret);
                    }
                });
    }

    /**
     * Called by Rust, on its render thread, once per nativeBack: handled is
     * true if the application dealt with Back itself (it went to its
     * previous screen), false if it had nowhere to go and the host should
     * leave the app. Leaving is main-thread work, so this only posts.
     *
     * Looked up by name through JNI: keep it out of any code shrinker.
     */
    @SuppressWarnings("unused") // Called from native code.
    static void onBackResult(final boolean handled) {
        MAIN.post(
                () -> {
                    MorningstarActivity activity = MorningstarActivity.current();
                    if (activity != null) {
                        activity.onBackResult(handled);
                    }
                });
    }

    // ---------------------------------------------------- platform services
    //
    // The application asks for work that takes time (an HTTP request, a
    // recording, playing a clip, a stored value); MorningstarServices does
    // it off the main thread and answers with one of the native methods
    // below. id is Rust's name for the request and comes back unchanged.
    // See docs/architecture/0019-effects-and-platform-services.md.

    // Values of onServiceCommand's command argument. Must match
    // render_thread.rs.
    static final int SERVICE_START_RECORDING = 0;
    static final int SERVICE_STOP_RECORDING = 1;
    /** first: the clip's media type; data: the file. */
    static final int SERVICE_PLAY = 2;
    static final int SERVICE_STOP_PLAYBACK = 3;
    /** first: the key; second: the value. */
    static final int SERVICE_STORE = 4;
    /** first: the key. */
    static final int SERVICE_REMOVE = 5;
    /** first: the key. Answered with nativeLoadResult. */
    static final int SERVICE_LOAD = 6;

    // Values of nativeHttpResult's error argument. Must match jni.rs.
    static final int HTTP_OK = 0;
    static final int HTTP_OFFLINE = 1;
    static final int HTTP_TIMEOUT = 2;
    static final int HTTP_INVALID_URL = 3;
    static final int HTTP_TOO_LARGE = 4;
    static final int HTTP_UNSUPPORTED = 5;
    static final int HTTP_FAILED = 6;

    // Values of nativeAudioResult's kind argument. Must match jni.rs.
    static final int AUDIO_KIND_STARTED = 0;
    static final int AUDIO_KIND_RECORDING = 1;
    static final int AUDIO_KIND_PLAYED = 2;

    // Values of nativeAudioResult's error argument. Must match jni.rs.
    static final int AUDIO_OK = 0;
    static final int AUDIO_PERMISSION_DENIED = 1;
    static final int AUDIO_BUSY = 2;
    static final int AUDIO_UNSUPPORTED = 3;
    static final int AUDIO_NOT_RECORDING = 4;
    static final int AUDIO_INTERRUPTED = 5;
    static final int AUDIO_FAILED = 6;

    /**
     * The answer to onHttpRequest. error HTTP_OK: the server answered with
     * status, headers ("Name: value" lines) and body. Otherwise there is no
     * response, and message describes an HTTP_FAILED for the log. Any
     * thread.
     */
    static native void nativeHttpResult(
            long id, int error, int status, String headers, byte[] body, String message);

    /**
     * The answer to a recording or playback command: kind says which
     * (AUDIO_KIND_*), error how it went (AUDIO_*), message describes an
     * AUDIO_FAILED. For a recording that was stopped, data is the file,
     * mime its media type and durationMs its length. Any thread.
     */
    static native void nativeAudioResult(
            long id,
            int kind,
            int error,
            String message,
            byte[] data,
            String mime,
            long durationMs);

    /** The answer to SERVICE_LOAD: the value, or found false. Any thread. */
    static native void nativeLoadResult(long id, boolean found, String value);

    /**
     * Called by Rust, on its render thread, to make an HTTP request: method
     * is "GET", "POST", ...; headers is "Name: value" lines; body is empty
     * for none; timeoutMs bounds the whole exchange. Only hands the request
     * to a worker thread; the answer is nativeHttpResult.
     *
     * Looked up by name through JNI: keep it out of any code shrinker.
     */
    @SuppressWarnings("unused") // Called from native code.
    static void onHttpRequest(
            long id, String method, String url, String headers, byte[] body, int timeoutMs) {
        MorningstarServices.http(id, method, url, headers, body, timeoutMs);
    }

    /**
     * Called by Rust, on its render thread, for every other service: a
     * SERVICE_* command with whichever of first, second and data it uses
     * (the others empty). Only posts to the service thread.
     *
     * Looked up by name through JNI: keep it out of any code shrinker.
     */
    @SuppressWarnings("unused") // Called from native code.
    static void onServiceCommand(int command, long id, String first, String second, byte[] data) {
        MorningstarServices.command(command, id, first, second, data);
    }
}
