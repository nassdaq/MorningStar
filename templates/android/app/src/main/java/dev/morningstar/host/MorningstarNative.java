package dev.morningstar.host;

import android.os.Handler;
import android.os.Looper;
import android.view.Surface;

/**
 * The complete interface between Java and Rust. The native methods are
 * implemented in crates/morningstar-android/src/jni.rs; the one method Rust
 * calls, onTextInputCommand, is looked up by name and signature in
 * crates/morningstar-android/src/java.rs. Both sides must change together.
 *
 * All native methods are called on the main thread and return immediately,
 * except nativeSurfaceDestroyed, which blocks until the renderer has let go
 * of the surface.
 *
 * Rust draws on demand: after any of these calls that can change the picture
 * it produces a frame, and between them it draws nothing.
 */
final class MorningstarNative {
    private MorningstarNative() {}

    /** Starts the Rust runtime. Safe to call more than once per process. */
    static native void nativeInit();

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
}
