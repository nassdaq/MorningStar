package dev.morningstar.host;

import android.os.Handler;
import android.os.Looper;
import android.view.Surface;

/**
 * The complete interface between Java and Rust. The native methods are
 * implemented in crates/morningstar-android/src/jni.rs; the six methods
 * Rust calls (onTextInputCommand, onBackResult, onHttpRequest,
 * onServiceCommand, onSystemBars and onSemanticsUpdate) are looked up by
 * name and signature in crates/morningstar-android/src/java.rs. Both sides
 * must change together.
 *
 * All native methods are called on the main thread and return immediately,
 * except nativeSurfaceDestroyed, which blocks until the renderer has let go
 * of the surface, and the five results of platform services
 * (nativeHttpResult, nativeAudioResult, nativeLoadResult,
 * nativeClipboardResult, nativeLiveEvent), which are called from the thread
 * that did the work.
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
     * whole text, the selection (from where it was started to the caret;
     * equal when nothing is selected) and the composing (pre-edit) range,
     * in UTF-16 code units; composingStart is -1 when nothing is being
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
    // Ctrl+A, C, X, V, Z and Shift+Z (or Y) of a hardware keyboard, and
    // the same asked for by an input method (performContextMenuAction).
    static final int TEXT_KEY_SELECT_ALL = 9;
    static final int TEXT_KEY_COPY = 10;
    static final int TEXT_KEY_CUT = 11;
    static final int TEXT_KEY_PASTE = 12;
    static final int TEXT_KEY_UNDO = 13;
    static final int TEXT_KEY_REDO = 14;

    /** An editing key, an action key, a shortcut, or the loss of focus. */
    static native void nativeTextCommand(int command);

    // Values of nativeTextMove's first argument. Must match jni.rs.
    static final int TEXT_MOVE_LEFT = 0;
    static final int TEXT_MOVE_RIGHT = 1;
    static final int TEXT_MOVE_WORD_LEFT = 2;
    static final int TEXT_MOVE_WORD_RIGHT = 3;
    static final int TEXT_MOVE_UP = 4;
    static final int TEXT_MOVE_DOWN = 5;
    static final int TEXT_MOVE_LINE_START = 6;
    static final int TEXT_MOVE_LINE_END = 7;
    static final int TEXT_MOVE_TEXT_START = 8;
    static final int TEXT_MOVE_TEXT_END = 9;

    /**
     * A key that moves the caret (an arrow, Home, End; with Ctrl, by words
     * or to the ends of the text). extend is Shift: select from where the
     * caret was.
     */
    static native void nativeTextMove(int move, boolean extend);

    // ------------------------------------------------------- reduce motion

    /**
     * Whether the user has asked the system for less motion ("remove
     * animations": the animator duration scale is zero). Call at start and
     * whenever it may have changed; repeating the current value is free.
     */
    static native void nativeSetReduceMotion(boolean reduce);

    // -------------------------------------------------------- accessibility
    //
    // See docs/architecture/0032-accessibility.md and
    // MorningstarAccessibility.

    /**
     * Whether anything reads the screen through the accessibility
     * framework (AccessibilityManager.isEnabled()). While it does, Rust
     * describes every frame and sends what changed to onSemanticsUpdate;
     * while it does not, Rust builds nothing. Call at start and whenever
     * it may have changed; repeating the current value is free.
     */
    static native void nativeSetAccessibility(boolean enabled);

    // Values of nativeAccessibilityAction's action argument: the codes of
    // Action::from_wire in crates/morningstar-core/src/semantics.rs.
    static final int A11Y_ACTION_ACTIVATE = 0;
    static final int A11Y_ACTION_LONG_PRESS = 1;
    static final int A11Y_ACTION_SCROLL_FORWARD = 2;
    static final int A11Y_ACTION_SCROLL_BACKWARD = 3;
    static final int A11Y_ACTION_FOCUS = 4;
    /** text: the field's new text. */
    static final int A11Y_ACTION_SET_TEXT = 5;
    /** start, end: the selection, in UTF-16 units of the node's value. */
    static final int A11Y_ACTION_SET_SELECTION = 6;
    static final int A11Y_ACTION_INCREMENT = 7;
    static final int A11Y_ACTION_DECREMENT = 8;
    static final int A11Y_ACTION_DISMISS = 9;

    /**
     * An accessibility service asks for something to be done to the node
     * with this id (the ids of onSemanticsUpdate). Returns at once, like a
     * touch; what it changes arrives as the next update. text may be null.
     */
    static native void nativeAccessibilityAction(
            int node, int action, String text, int start, int end);

    /**
     * Called by Rust, on its render thread, after a frame whose meaning
     * differs from what the host was last told: the nodes that were added,
     * changed or removed, in the format MorningstarAccessibility reads.
     * The array is the host's own copy. Answering the accessibility
     * framework is main-thread work, so this only posts.
     *
     * Looked up by name through JNI: keep it out of any code shrinker.
     */
    @SuppressWarnings("unused") // Called from native code.
    static void onSemanticsUpdate(final byte[] update) {
        MAIN.post(() -> MorningstarAccessibility.applyUpdate(update));
    }

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
     * arguments, holding text with the selection from selectionStart
     * (where it was started) to selectionEnd (the caret), in UTF-16 units;
     * the two are equal when nothing is selected. TEXT_UPDATE: Rust
     * changed the same field's text, caret or selection; replace the
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
            final int selectionStart,
            final int selectionEnd) {
        MAIN.post(
                () -> {
                    MorningstarActivity activity = MorningstarActivity.current();
                    if (activity != null) {
                        activity.onTextInputCommand(
                                command, serial, keyboard, capitalization, action, flags, text,
                                selectionStart, selectionEnd);
                    }
                });
    }

    // ---------------------------------------------------------- system bars

    // What Rust last asked for, kept for activities created later: the
    // Rust runtime outlives an activity and tells the host only when the
    // answer changes. Main thread only. Null until Rust has said.
    private static Boolean darkStatusIcons;
    private static Boolean darkNavigationIcons;

    /**
     * Called by Rust, on its render thread, when the frame it has just
     * built needs the system bars' icons in another colour than the host
     * was last told: dark icons over a light screen, light over a dark
     * one. The bars themselves stay transparent. Changing a window is
     * main-thread work, so this only posts.
     *
     * Looked up by name through JNI: keep it out of any code shrinker.
     */
    @SuppressWarnings("unused") // Called from native code.
    static void onSystemBars(final boolean darkStatus, final boolean darkNavigation) {
        MAIN.post(
                () -> {
                    darkStatusIcons = darkStatus;
                    darkNavigationIcons = darkNavigation;
                    MorningstarActivity activity = MorningstarActivity.current();
                    if (activity != null) {
                        activity.applySystemBars(darkStatus, darkNavigation);
                    }
                });
    }

    /**
     * Gives a newly created activity the look Rust last asked for, if it
     * has asked. Main thread.
     */
    static void restoreSystemBars(MorningstarActivity activity) {
        if (darkStatusIcons != null && darkNavigationIcons != null) {
            activity.applySystemBars(darkStatusIcons, darkNavigationIcons);
        }
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
    /** first: plain text to put on the clipboard. */
    static final int SERVICE_SET_CLIPBOARD = 7;
    /** Answered with nativeClipboardResult. */
    static final int SERVICE_READ_CLIPBOARD = 8;
    /**
     * first: what the screen reader should say; second: "assertive" to
     * interrupt what it is saying, empty otherwise. Not answered.
     */
    static final int SERVICE_ANNOUNCE = 9;

    // The live room (docs/architecture/0038-live-rooms.md), carried out by
    // MorningstarLive. Its answers are nativeLiveEvent.
    /**
     * first: the media server's address (wss://...); second: the ticket, a
     * secret that is never logged; data: four bytes, each zero for no:
     * the microphone is live from the start, sound goes to the loudspeaker
     * rather than the earpiece, speaking reports are wanted, the camera is
     * on from the start, the room goes on while the app is not on screen
     * (a host that reads fewer takes the rest as no); and after those five,
     * for a room that goes on, its notice as UTF-8, three lines: title,
     * text, the name of the action that ends the room
     * (docs/architecture/0042). Answered with
     * a nativeLiveEvent for each event of the room, under this id, the
     * last of them LIVE_DISCONNECTED or LIVE_FAILED.
     */
    static final int SERVICE_LIVE_JOIN = 10;
    /** Leave the room. The join's events end with LIVE_END_LEFT. */
    static final int SERVICE_LIVE_LEAVE = 11;
    /** data: one byte, zero to mute the microphone. Not answered. */
    static final int SERVICE_LIVE_MICROPHONE = 12;
    /** data: one byte, zero for the earpiece. Not answered. */
    static final int SERVICE_LIVE_SPEAKER = 13;
    // What a room carries besides sound (docs/architecture/0039).
    /**
     * Send data to everyone in the room. first: the topic; second:
     * "reliable", or empty for fast and perhaps lost; data: the message.
     * Not answered.
     */
    static final int SERVICE_LIVE_DATA = 14;
    /**
     * data: one byte, zero to turn this device's camera off. Not answered:
     * LIVE_VIDEO_STARTED, LIVE_VIDEO_STOPPED or LIVE_MEDIA_FAILED follows.
     */
    static final int SERVICE_LIVE_CAMERA = 15;
    /** Change between the front and the back camera. Not answered. */
    static final int SERVICE_LIVE_SWITCH_CAMERA = 16;
    /** data: one byte, zero to stop sharing this device's screen. As for the camera. */
    static final int SERVICE_LIVE_SCREEN_SHARE = 17;
    /**
     * data: one byte, a LIVE_ROUTE_*: send the room's sound there
     * (docs/architecture/0042). Not answered: LIVE_AUDIO_ROUTES follows.
     */
    static final int SERVICE_LIVE_AUDIO_ROUTE = 18;

    // Values of nativeLiveEvent's kind argument. Must match
    // LiveEvent::from_wire in morningstar-core.
    static final int LIVE_CONNECTED = 0;
    /** first: the participant's identity; second: their name. */
    static final int LIVE_PARTICIPANT_JOINED = 1;
    /** first: the participant's identity. */
    static final int LIVE_PARTICIPANT_LEFT = 2;
    /** first: the identities of who is speaking, one per line. */
    static final int LIVE_SPEAKING = 3;
    static final int LIVE_RECONNECTING = 4;
    static final int LIVE_RECONNECTED = 5;
    /** code: a LIVE_END_* reason. The last event of a room. */
    static final int LIVE_DISCONNECTED = 6;
    /** code: a LIVE_ERROR_*; first: what failed. The last event of a room. */
    static final int LIVE_FAILED = 7;

    /**
     * A video started. code: its track number times 4, plus 2 if it is this
     * device's own, plus 1 if it is a shared screen; first: whose, by
     * identity. Its pictures follow through nativeVideoFrame under that
     * track number.
     */
    static final int LIVE_VIDEO_STARTED = 8;
    /** That video is over. code and first as for LIVE_VIDEO_STARTED. */
    static final int LIVE_VIDEO_STOPPED = 9;
    /**
     * This device's camera or screen could not be turned on. code: a
     * LIVE_ERROR_* times 2, plus 1 for the screen; first: what failed.
     * Not the end of the room.
     */
    static final int LIVE_MEDIA_FAILED = 10;
    /**
     * Where the sound can go and where it goes: once connected, and when
     * either changes. code: the LIVE_ROUTE_* in use (0 for none yet), plus
     * 16, 32, 64 and 128 for the earpiece, the loudspeaker, a wired headset
     * and a Bluetooth headset when there is one.
     */
    static final int LIVE_AUDIO_ROUTES = 11;

    // A live room's audio routes. Must match LiveAudioRoute in
    // morningstar-core.
    static final int LIVE_ROUTE_EARPIECE = 1;
    static final int LIVE_ROUTE_SPEAKER = 2;
    static final int LIVE_ROUTE_WIRED = 3;
    static final int LIVE_ROUTE_BLUETOOTH = 4;

    // nativeLiveEvent's code with LIVE_DISCONNECTED.
    static final int LIVE_END_LEFT = 0;
    static final int LIVE_END_REPLACED = 1;
    static final int LIVE_END_HIDDEN = 2;
    static final int LIVE_END_ENDED = 3;
    static final int LIVE_END_LOST = 4;
    /** The user ended it from the notification of a room in the background. */
    static final int LIVE_END_HUNG_UP = 5;

    // nativeLiveEvent's code with LIVE_FAILED.
    static final int LIVE_ERROR_PERMISSION_DENIED = 1;
    static final int LIVE_ERROR_UNSUPPORTED = 2;
    static final int LIVE_ERROR_FAILED = 3;

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
     * The answer to SERVICE_READ_CLIPBOARD: the clipboard as plain text, or
     * found false when it is empty or holds no text. Any thread.
     */
    static native void nativeClipboardResult(long id, boolean found, String text);

    /**
     * Something happened in the live room joined by SERVICE_LIVE_JOIN id:
     * kind is a LIVE_* event, code and the two strings what that event
     * says they are (unused ones 0 and null). Called as often as there are
     * events; after LIVE_DISCONNECTED or LIVE_FAILED never again for that
     * id. Any thread.
     */
    static native void nativeLiveEvent(long id, int kind, int code, String first, String second);

    /**
     * Someone sent data to the live room joined by SERVICE_LIVE_JOIN id:
     * from is their identity (empty for the server), topic what they
     * called it, payload the message. Any thread.
     */
    static native void nativeLiveData(long id, String from, String topic, byte[] payload);

    /**
     * One decoded picture of the video with this track number, as I420:
     * three direct ByteBuffers, each read from its start, with the bytes
     * from one row to the next of each; rotation is the degrees clockwise
     * that show it upright, mirrored whether it is shown as in a mirror (a
     * front camera's own preview). The planes are copied before this
     * returns: release the buffer afterwards, not before. Only the latest
     * frame of a video is kept. Any thread.
     */
    static native void nativeVideoFrame(
            int track,
            int width,
            int height,
            int rotation,
            boolean mirrored,
            java.nio.ByteBuffer y,
            int yStride,
            java.nio.ByteBuffer u,
            int uStride,
            java.nio.ByteBuffer v,
            int vStride);

    /** The video with this track number is over. Any thread. */
    static native void nativeVideoEnded(int track);

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
    static void onServiceCommand(
            final int command, long id, final String first, final String second, byte[] data) {
        if (command == SERVICE_ANNOUNCE) {
            // Not a service with work to do off the main thread: a word
            // to the accessibility framework, on the main thread.
            MAIN.post(() -> MorningstarAccessibility.announce(first, "assertive".equals(second)));
            return;
        }
        if (command >= SERVICE_LIVE_JOIN && command <= SERVICE_LIVE_AUDIO_ROUTE) {
            // The live room has its own class, which exists in two forms:
            // the real one in an app that declares the service, and one
            // that refuses in every other app (ADR 0038).
            MorningstarLive.command(command, id, first, second, data);
            return;
        }
        MorningstarServices.command(command, id, first, second, data);
    }
}
