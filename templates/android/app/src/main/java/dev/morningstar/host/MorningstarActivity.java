package dev.morningstar.host;

import android.app.Activity;
import android.animation.ValueAnimator;
import android.content.Context;
import android.content.pm.ApplicationInfo;
import android.content.pm.PackageManager;
import android.content.res.Configuration;
import android.database.ContentObserver;
import android.graphics.Color;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.provider.Settings;
import android.text.Editable;
import android.text.InputType;
import android.text.Selection;
import android.text.SpannableStringBuilder;
import android.util.Log;
import android.view.DisplayCutout;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.SurfaceHolder;
import android.view.SurfaceView;
import android.view.View;
import android.view.Window;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.view.WindowManager;
import android.view.inputmethod.BaseInputConnection;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.ExtractedText;
import android.view.inputmethod.ExtractedTextRequest;
import android.view.inputmethod.InputConnection;
import android.view.inputmethod.InputMethodManager;
import android.window.OnBackInvokedCallback;
import android.window.OnBackInvokedDispatcher;

/**
 * The entire Android host: one activity, one SurfaceView, and forwarding of
 * lifecycle, surface, display-metric and touch callbacks to Rust. It contains
 * no application logic and is the same for every Morningstar app.
 *
 * The surface covers the whole window, under the status bar, the display
 * cutout and the navigation bar. Rust is told how much of each edge they
 * cover (nativeSetInsets) and how many pixels make one dp (nativeSetScale).
 *
 * Text input: while a field of the application has focus, Rust asks for the
 * keyboard (MorningstarNative.onTextInputCommand). The view then presents
 * itself to the input method as a text editor whose text is a copy of the
 * field's. The input method edits that copy; every change is reported to
 * Rust whole (nativeTextEdited), and Rust replaces the copy when the field
 * ends up holding something else. See docs/architecture/0016-text-input.md.
 *
 * Back: a Back that no keyboard absorbs is passed to Rust (nativeBack),
 * which answers a moment later from its render thread
 * (MorningstarNative.onBackResult): the application went back a screen, or
 * it had nowhere to go and the host leaves the app. The main thread never
 * waits for the answer. See docs/architecture/0018-back-and-timers.md.
 *
 * Selection and the clipboard: the field's selection is Rust's, drawn by
 * Rust, and mirrored in the copy the input method sees (both ways). Arrow
 * keys and the Ctrl shortcuts of a hardware keyboard are passed on as they
 * are. See docs/architecture/0025-text-selection-and-clipboard.md.
 *
 * System bars and reduce motion: Rust says whether the bars' icons should
 * be light or dark (MorningstarNative.onSystemBars), and is told whether
 * the user has switched animations off (nativeSetReduceMotion). See
 * docs/architecture/0026-system-bars-and-reduce-motion.md.
 *
 * Platform services (HTTP, recording, playback, stored values) are in
 * MorningstarServices; this class only tells it when the activity comes and
 * goes and passes on the answer to the microphone permission. See
 * docs/architecture/0019-effects-and-platform-services.md.
 */
public final class MorningstarActivity extends Activity implements SurfaceHolder.Callback2 {
    private static final String LIB_NAME_KEY = "dev.morningstar.lib_name";

    /**
     * Boolean intent extra that turns on continuous rendering, for benchmarks:
     * {@code am start -n <activity> --ez dev.morningstar.continuous true}.
     */
    private static final String CONTINUOUS_EXTRA = "dev.morningstar.continuous";

    private static final String TAG = "Morningstar";

    /** The activity on screen, for commands that arrive from Rust. */
    private static MorningstarActivity current;

    static MorningstarActivity current() {
        return current;
    }

    private HostView hostView;
    private InputMethodManager inputMethodManager;
    /** The focused field's keyboard request and text, or null. */
    private TextSession textSession;
    /** Whether the last insets showed the keyboard. */
    private boolean keyboardVisible;

    /**
     * How long Rust is given to answer a Back. It normally answers within a
     * frame; if its render thread is stuck, Back must still leave the app.
     */
    private static final long BACK_ANSWER_TIMEOUT_MS = 1000;

    private final Handler mainHandler = new Handler(Looper.getMainLooper());
    /** Backs passed to Rust and not answered yet. Main thread only. */
    private int backsPending;
    /** The OnBackInvokedCallback registered on API 33 and later, or null. */
    private Object backCallback;

    private final Runnable backUnanswered =
            () -> {
                if (backsPending > 0) {
                    backsPending = 0;
                    Log.w(TAG, "back: no answer from Rust, leaving the app");
                    leave();
                }
            };

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);

        System.loadLibrary(applicationLibraryName());
        // Before Rust starts: the application is handed its installation id
        // and its stored values when it is set up.
        MorningstarServices.init(getApplicationContext());
        MorningstarNative.nativeInit(
                MorningstarServices.installId(), MorningstarServices.storedSnapshot());
        // The Rust runtime outlives the activity, so always state the mode.
        MorningstarNative.nativeSetContinuous(
                getIntent().getBooleanExtra(CONTINUOUS_EXTRA, false));
        sendScale();
        sendReduceMotion();

        layoutEdgeToEdge();
        // An activity recreated in a live process: Rust said long ago how
        // the bars should look and will not say again until it changes.
        MorningstarNative.restoreSystemBars(this);

        current = this;
        inputMethodManager = (InputMethodManager) getSystemService(Context.INPUT_METHOD_SERVICE);
        hostView = new HostView(this);
        SurfaceView surfaceView = hostView;
        surfaceView.getHolder().addCallback(this);
        // The view fills the window, so window insets are surface insets.
        // They are dispatched before the surface is created and again
        // whenever they change (rotation, bars shown or hidden).
        surfaceView.setOnApplyWindowInsetsListener(
                (view, insets) -> {
                    sendInsets(insets);
                    return insets;
                });
        // On the view, not the activity, so coordinates are relative to the surface.
        surfaceView.setOnTouchListener(
                (view, event) -> {
                    forwardTouch(event);
                    return true;
                });
        setContentView(surfaceView);

        // From API 33 Back is delivered to a registered callback; the
        // manifest turns that on (enableOnBackInvokedCallback), and with
        // targetSdk 36 on Android 16 it is the only way: onBackPressed is
        // no longer called there. Below API 33 onBackPressed still is.
        if (Build.VERSION.SDK_INT >= 33) {
            backCallback = Api33.registerBack(this);
        }
    }

    // Visibility, not focus, decides whether to render: in multi-window mode a
    // paused activity is still on screen. Hence onStart/onStop rather than
    // onResume/onPause.
    @Override
    protected void onStart() {
        super.onStart();
        MorningstarServices.activityStarted();
        // The setting may have changed while the app was off screen, and
        // is watched while it is on.
        sendReduceMotion();
        getContentResolver()
                .registerContentObserver(
                        Settings.Global.getUriFor(Settings.Global.ANIMATOR_DURATION_SCALE),
                        false,
                        animatorScaleObserver);
        MorningstarNative.nativeResume();
    }

    @Override
    protected void onStop() {
        // Leaving the screen ends any editing: Android takes the keyboard
        // down by itself, and Rust must not go on believing it is up.
        giveUpTextInput("activity stopped");
        // Sound stops with the screen: a clip ends as interrupted and a
        // recording is closed and kept (ADR 0019).
        MorningstarServices.activityStopped();
        getContentResolver().unregisterContentObserver(animatorScaleObserver);
        MorningstarNative.nativePause();
        super.onStop();
    }

    @Override
    protected void onDestroy() {
        if (current == this) {
            current = null;
            // A microphone question this activity was showing dies with it.
            MorningstarServices.activityDestroyed();
        }
        mainHandler.removeCallbacks(backUnanswered);
        backsPending = 0;
        if (Build.VERSION.SDK_INT >= 33 && backCallback != null) {
            Api33.unregisterBack(this, backCallback);
            backCallback = null;
        }
        super.onDestroy();
    }

    // The user's answer to the microphone question MorningstarServices asked
    // through this activity.
    @Override
    public void onRequestPermissionsResult(
            int requestCode, String[] permissions, int[] grantResults) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults);
        MorningstarServices.onPermissionResult(requestCode, grantResults);
    }

    // The hook on API 24 to 32. From API 33 the callback registered in
    // onCreate is called instead.
    @Override
    @SuppressWarnings("deprecation")
    public void onBackPressed() {
        handleBack();
    }

    /**
     * The system's Back gesture or key.
     *
     * With a field focused but no soft keyboard to absorb Back (a hardware
     * keyboard), it ends the editing and goes no further. With a soft
     * keyboard up the input method consumes Back itself and this is not
     * called; that is noticed through the insets (sendInsets).
     *
     * Otherwise the application is asked, without waiting: nativeBack only
     * posts to the render thread, and the answer arrives in onBackResult.
     */
    private void handleBack() {
        if (textSession != null) {
            giveUpTextInput("back");
            return;
        }
        backsPending++;
        MorningstarNative.nativeBack();
        mainHandler.removeCallbacks(backUnanswered);
        mainHandler.postDelayed(backUnanswered, BACK_ANSWER_TIMEOUT_MS);
    }

    /**
     * Rust's answer to a nativeBack, on the main thread. Not handled means
     * the application had nowhere to go back to: leave it. An answer nobody
     * is waiting for (it came after the timeout, or was meant for an
     * activity since destroyed) is dropped.
     */
    void onBackResult(boolean handled) {
        if (backsPending == 0) {
            return;
        }
        backsPending--;
        if (backsPending == 0) {
            mainHandler.removeCallbacks(backUnanswered);
        }
        if (!handled) {
            leave();
        }
    }

    /**
     * What Activity.onBackPressed does when nothing handles Back: from
     * Android 12 the root activity of a task is moved to the background
     * rather than finished, so returning to the app is a warm start; before
     * that, and for an activity that is not the root, it is finished.
     */
    private void leave() {
        if (isFinishing()) {
            return;
        }
        if (Build.VERSION.SDK_INT >= 31 && isTaskRoot()) {
            moveTaskToBack(true);
        } else {
            finish();
        }
    }

    /**
     * Code that names classes added in API 33, kept in a class of its own
     * so that MorningstarActivity loads cleanly on older releases.
     */
    private static final class Api33 {
        private Api33() {}

        static Object registerBack(MorningstarActivity activity) {
            OnBackInvokedCallback callback = activity::handleBack;
            activity.getOnBackInvokedDispatcher()
                    .registerOnBackInvokedCallback(
                            OnBackInvokedDispatcher.PRIORITY_DEFAULT, callback);
            return callback;
        }

        static void unregisterBack(MorningstarActivity activity, Object callback) {
            activity.getOnBackInvokedDispatcher()
                    .unregisterOnBackInvokedCallback((OnBackInvokedCallback) callback);
        }
    }

    // Density changes (display size setting, moving to another display) are
    // declared in configChanges, so they arrive here instead of recreating
    // the activity.
    @Override
    public void onConfigurationChanged(Configuration newConfig) {
        super.onConfigurationChanged(newConfig);
        sendScale();
    }

    @Override
    public void surfaceCreated(SurfaceHolder holder) {
        MorningstarNative.nativeSurfaceCreated(holder.getSurface());
    }

    @Override
    public void surfaceChanged(SurfaceHolder holder, int format, int width, int height) {
        // A new size can come with a new density; Rust ignores a repeat.
        sendScale();
        MorningstarNative.nativeSurfaceChanged(width, height);
    }

    // Rust only draws when something changed, so the system's request to draw
    // again must be passed on. It is not waited for: blocking the main thread
    // on the render thread is what this host avoids everywhere it can.
    @Override
    public void surfaceRedrawNeeded(SurfaceHolder holder) {
        MorningstarNative.nativeRequestRedraw();
    }

    @Override
    public void surfaceDestroyed(SurfaceHolder holder) {
        // Must not return until Rust has stopped using the surface.
        MorningstarNative.nativeSurfaceDestroyed();
    }

    private void sendScale() {
        MorningstarNative.nativeSetScale(getResources().getDisplayMetrics().density);
    }

    // -------------------------------------------------------- reduce motion

    /** Told when the user changes "animator duration scale" in the settings. */
    private final ContentObserver animatorScaleObserver =
            new ContentObserver(mainHandler) {
                @Override
                public void onChange(boolean selfChange) {
                    sendReduceMotion();
                }
            };

    /**
     * Tells Rust whether animations are switched off: "Remove animations"
     * in the accessibility settings, or an animator duration scale of zero
     * in the developer options, which is the same setting. From API 26 the
     * platform answers directly; before that the setting is read.
     */
    private void sendReduceMotion() {
        boolean enabled;
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            enabled = ValueAnimator.areAnimatorsEnabled();
        } else {
            enabled =
                    Settings.Global.getFloat(
                                    getContentResolver(),
                                    Settings.Global.ANIMATOR_DURATION_SCALE,
                                    1f)
                            != 0f;
        }
        MorningstarNative.nativeSetReduceMotion(!enabled);
    }

    // ---------------------------------------------------------- system bars

    /**
     * Makes the icons of the status bar and of the navigation bar dark
     * (over a light screen) or light, as Rust asked. The bars stay
     * transparent and the window edge to edge.
     *
     * API 30 and later: WindowInsetsController. API 23 to 29: the
     * SYSTEM_UI_FLAG_LIGHT_STATUS_BAR flag, and from API 26 the one for the
     * navigation bar; before API 26 the navigation bar's icons are always
     * light, and before API 23 the status bar's too.
     */
    @SuppressWarnings("deprecation") // The flags are the only way before API 30.
    void applySystemBars(boolean darkStatusIcons, boolean darkNavigationIcons) {
        Window window = getWindow();
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            WindowInsetsController controller = window.getInsetsController();
            if (controller == null) {
                return;
            }
            controller.setSystemBarsAppearance(
                    darkStatusIcons ? WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS : 0,
                    WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS);
            controller.setSystemBarsAppearance(
                    darkNavigationIcons
                            ? WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS
                            : 0,
                    WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS);
            return;
        }
        View decor = window.getDecorView();
        int flags = decor.getSystemUiVisibility();
        flags = withFlag(flags, View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR, darkStatusIcons);
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            flags = withFlag(flags, View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR, darkNavigationIcons);
        }
        decor.setSystemUiVisibility(flags);
    }

    private static int withFlag(int flags, int flag, boolean set) {
        return set ? flags | flag : flags & ~flag;
    }

    /**
     * Makes the window cover the whole display area it is given, with
     * transparent system bars, on every supported API level.
     *
     * API 35 and later do this by themselves for apps that target them; the
     * calls below are what earlier releases need, and are harmless later.
     */
    @SuppressWarnings("deprecation") // The pre-30 and pre-35 ways are deprecated, and needed there.
    private void layoutEdgeToEdge() {
        Window window = getWindow();

        // The window, not the system, paints behind the bars: nothing.
        window.clearFlags(
                WindowManager.LayoutParams.FLAG_TRANSLUCENT_STATUS
                        | WindowManager.LayoutParams.FLAG_TRANSLUCENT_NAVIGATION);
        window.addFlags(WindowManager.LayoutParams.FLAG_DRAWS_SYSTEM_BAR_BACKGROUNDS);
        window.setStatusBarColor(Color.TRANSPARENT);
        window.setNavigationBarColor(Color.TRANSPARENT);

        // Lay the content out under the bars instead of between them.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            window.setDecorFitsSystemWindows(false);
        } else {
            window.getDecorView()
                    .setSystemUiVisibility(
                            View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                                    | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                                    | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION);
        }

        // And under a display cutout, which otherwise letterboxes the window.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            WindowManager.LayoutParams attributes = window.getAttributes();
            attributes.layoutInDisplayCutoutMode =
                    Build.VERSION.SDK_INT >= Build.VERSION_CODES.R
                            ? WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS
                            : WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
            window.setAttributes(attributes);
        }
    }

    /**
     * Reports the area covered by system bars and the display cutout
     * (nativeSetInsets) and, separately, the height of the soft keyboard
     * (nativeSetKeyboardInset): the keyboard is not part of the safe area.
     *
     * From API 30 the keyboard has its own inset type. Before that there is
     * none: with windowSoftInputMode="adjustResize" and a window laid out
     * under the system bars, the keyboard is added to the bottom
     * system-window inset, so its height is that inset minus the "stable"
     * bottom inset (the navigation bar alone), and the stable inset is what
     * is reported as the safe area's bottom while it is up. The pre-30 path
     * is written from the platform documentation and has not been run.
     */
    @SuppressWarnings("deprecation") // getSystemWindowInset* is the only API before 30.
    private void sendInsets(WindowInsets insets) {
        int left;
        int top;
        int right;
        int bottom;
        int keyboard;
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            android.graphics.Insets safe =
                    insets.getInsets(
                            WindowInsets.Type.systemBars() | WindowInsets.Type.displayCutout());
            left = safe.left;
            top = safe.top;
            right = safe.right;
            bottom = safe.bottom;
            keyboard =
                    insets.isVisible(WindowInsets.Type.ime())
                            ? insets.getInsets(WindowInsets.Type.ime()).bottom
                            : 0;
        } else {
            left = insets.getSystemWindowInsetLeft();
            top = insets.getSystemWindowInsetTop();
            right = insets.getSystemWindowInsetRight();
            bottom = insets.getSystemWindowInsetBottom();
            int stable = insets.getStableInsetBottom();
            // Anything much taller than a navigation bar is the keyboard.
            int threshold = (int) (80 * getResources().getDisplayMetrics().density);
            if (bottom - stable > threshold) {
                keyboard = bottom;
                bottom = stable;
            } else {
                keyboard = 0;
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                // In landscape the cutout is on an edge with no system bar.
                DisplayCutout cutout = insets.getDisplayCutout();
                if (cutout != null) {
                    left = Math.max(left, cutout.getSafeInsetLeft());
                    top = Math.max(top, cutout.getSafeInsetTop());
                    right = Math.max(right, cutout.getSafeInsetRight());
                    bottom = Math.max(bottom, cutout.getSafeInsetBottom());
                }
            }
        }
        MorningstarNative.nativeSetInsets(left, top, right, bottom);
        MorningstarNative.nativeSetKeyboardInset(keyboard);

        // The keyboard went away while a field still has focus: the user
        // dismissed it (Back, or the keyboard's own hide key). Tell Rust, so
        // that the field gives up focus instead of staying focused with no
        // keyboard. Only a visible-to-hidden change counts: a keyboard that
        // is still on its way up has not been dismissed.
        boolean visible = keyboard > 0;
        if (keyboardVisible && !visible && textSession != null) {
            giveUpTextInput("keyboard dismissed");
        }
        keyboardVisible = visible;
    }

    // ---------------------------------------------------------- text input

    /** What the focused field asked for, and the host's copy of its text. */
    private static final class TextSession {
        int serial;
        int keyboard;
        int capitalization;
        int action;
        int flags;
        /** The copy the input method edits. Main thread only. */
        final Editable text = new SpannableStringBuilder();
        // What Rust was last told, or told us: only changes are reported.
        String knownText = "";
        int knownStart;
        int knownEnd;
        int knownComposingStart = -1;
        int knownComposingEnd = -1;

        boolean has(int flag) {
            return (flags & flag) != 0;
        }

        int inputType() {
            switch (keyboard) {
                case MorningstarNative.KEYBOARD_NUMBER:
                    return InputType.TYPE_CLASS_NUMBER
                            | (has(MorningstarNative.FLAG_SECURE)
                                    ? InputType.TYPE_NUMBER_VARIATION_PASSWORD
                                    : 0);
                case MorningstarNative.KEYBOARD_DECIMAL:
                    return InputType.TYPE_CLASS_NUMBER | InputType.TYPE_NUMBER_FLAG_DECIMAL;
                case MorningstarNative.KEYBOARD_PHONE:
                    return InputType.TYPE_CLASS_PHONE;
                case MorningstarNative.KEYBOARD_EMAIL:
                    return InputType.TYPE_CLASS_TEXT
                            | InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS;
                default:
                    break;
            }
            if (has(MorningstarNative.FLAG_SECURE)) {
                return InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_PASSWORD;
            }
            int type = InputType.TYPE_CLASS_TEXT;
            switch (capitalization) {
                case MorningstarNative.CAPITALIZE_SENTENCES:
                    type |= InputType.TYPE_TEXT_FLAG_CAP_SENTENCES;
                    break;
                case MorningstarNative.CAPITALIZE_WORDS:
                    type |= InputType.TYPE_TEXT_FLAG_CAP_WORDS;
                    break;
                case MorningstarNative.CAPITALIZE_CHARACTERS:
                    type |= InputType.TYPE_TEXT_FLAG_CAP_CHARACTERS;
                    break;
                default:
                    break;
            }
            if (has(MorningstarNative.FLAG_MULTILINE)) {
                type |= InputType.TYPE_TEXT_FLAG_MULTI_LINE;
            }
            type |=
                    has(MorningstarNative.FLAG_AUTOCORRECT)
                            ? InputType.TYPE_TEXT_FLAG_AUTO_CORRECT
                            : InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS;
            return type;
        }

        int imeOptions() {
            // The app draws its own field: never the full-screen editor that
            // input methods offer in landscape.
            int options = EditorInfo.IME_FLAG_NO_FULLSCREEN | EditorInfo.IME_FLAG_NO_EXTRACT_UI;
            if (has(MorningstarNative.FLAG_MULTILINE)) {
                return options | EditorInfo.IME_FLAG_NO_ENTER_ACTION;
            }
            switch (action) {
                case MorningstarNative.TEXT_KEY_NEXT:
                    return options | EditorInfo.IME_ACTION_NEXT;
                case MorningstarNative.TEXT_KEY_GO:
                    return options | EditorInfo.IME_ACTION_GO;
                case MorningstarNative.TEXT_KEY_SEARCH:
                    return options | EditorInfo.IME_ACTION_SEARCH;
                case MorningstarNative.TEXT_KEY_SEND:
                    return options | EditorInfo.IME_ACTION_SEND;
                default:
                    return options | EditorInfo.IME_ACTION_DONE;
            }
        }

        /**
         * Replaces the copy with Rust's text and selection (from start,
         * where it was begun, to end, the caret); nothing composing.
         */
        void set(String newText, int start, int end) {
            if (!text.toString().equals(newText)) {
                text.replace(0, text.length(), newText);
            }
            BaseInputConnection.removeComposingSpans(text);
            int from = Math.max(0, Math.min(start, text.length()));
            int at = Math.max(0, Math.min(end, text.length()));
            Selection.setSelection(text, from, at);
            knownText = newText;
            knownStart = from;
            knownEnd = at;
            knownComposingStart = -1;
            knownComposingEnd = -1;
        }
    }

    /** A command from Rust (see MorningstarNative.onTextInputCommand). */
    void onTextInputCommand(
            int command,
            int serial,
            int keyboard,
            int capitalization,
            int action,
            int flags,
            String text,
            int selectionStart,
            int selectionEnd) {
        switch (command) {
            case MorningstarNative.TEXT_SHOW:
                {
                    TextSession session = new TextSession();
                    session.serial = serial;
                    session.keyboard = keyboard;
                    session.capitalization = capitalization;
                    session.action = action;
                    session.flags = flags;
                    session.set(text, selectionStart, selectionEnd);
                    textSession = session;
                    hostView.requestFocus();
                    // A new field: the input method must read its type and
                    // text again (onCreateInputConnection).
                    inputMethodManager.restartInput(hostView);
                    showKeyboard();
                    Log.i(
                            TAG,
                            "host: keyboard requested serial="
                                    + serial
                                    + " inputType=0x"
                                    + Integer.toHexString(session.inputType()));
                    break;
                }
            case MorningstarNative.TEXT_UPDATE:
                {
                    TextSession session = textSession;
                    if (session == null) {
                        break;
                    }
                    boolean sameText = session.text.toString().equals(text);
                    session.set(text, selectionStart, selectionEnd);
                    if (sameText) {
                        // Only the caret or the selection moved (a handle
                        // was dragged, a word double-tapped); the
                        // composition, if any, ended. The input method
                        // follows from this, without being restarted.
                        inputMethodManager.updateSelection(
                                hostView, session.knownStart, session.knownEnd, -1, -1);
                    } else {
                        // What the input method believes is in the field is
                        // wrong: make it start again from the new text.
                        inputMethodManager.restartInput(hostView);
                    }
                    break;
                }
            case MorningstarNative.TEXT_HIDE:
                if (textSession != null) {
                    textSession = null;
                    inputMethodManager.restartInput(hostView);
                }
                inputMethodManager.hideSoftInputFromWindow(hostView.getWindowToken(), 0);
                Log.i(TAG, "host: keyboard hidden on request");
                break;
            default:
                break;
        }
    }

    private void showKeyboard() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R
                && hostView.getWindowInsetsController() != null) {
            hostView.getWindowInsetsController().show(WindowInsets.Type.ime());
        } else {
            inputMethodManager.showSoftInput(hostView, 0);
        }
    }

    /**
     * Ends the editing from the host's side and tells Rust, which clears the
     * field's focus. Does nothing when no field has focus.
     */
    private void giveUpTextInput(String why) {
        if (textSession == null) {
            return;
        }
        textSession = null;
        inputMethodManager.restartInput(hostView);
        inputMethodManager.hideSoftInputFromWindow(hostView.getWindowToken(), 0);
        MorningstarNative.nativeTextCommand(MorningstarNative.TEXT_KEY_FOCUS_LOST);
        Log.i(TAG, "host: text input ended (" + why + ")");
    }

    /**
     * Reports the copy's state to Rust if it differs from what Rust knows,
     * and tells the input method where the selection now is.
     */
    private void reportEdit(TextSession session) {
        if (session != textSession) {
            return;
        }
        Editable text = session.text;
        String now = text.toString();
        int start = Math.max(0, Selection.getSelectionStart(text));
        int end = Math.max(0, Selection.getSelectionEnd(text));
        int composingStart = BaseInputConnection.getComposingSpanStart(text);
        int composingEnd = BaseInputConnection.getComposingSpanEnd(text);
        if (composingStart > composingEnd) {
            int swap = composingStart;
            composingStart = composingEnd;
            composingEnd = swap;
        }
        if (now.equals(session.knownText)
                && start == session.knownStart
                && end == session.knownEnd
                && composingStart == session.knownComposingStart
                && composingEnd == session.knownComposingEnd) {
            return;
        }
        session.knownText = now;
        session.knownStart = start;
        session.knownEnd = end;
        session.knownComposingStart = composingStart;
        session.knownComposingEnd = composingEnd;
        MorningstarNative.nativeTextEdited(
                session.serial, now, start, end, composingStart, composingEnd);
        inputMethodManager.updateSelection(hostView, start, end, composingStart, composingEnd);
    }

    /**
     * The SurfaceView, as a text editor while a field has focus: it hands
     * the input method a connection to the session's copy of the text, and
     * turns key events (a hardware keyboard, adb) into text commands.
     */
    private final class HostView extends SurfaceView {
        HostView(Context context) {
            super(context);
            setFocusable(true);
            setFocusableInTouchMode(true);
        }

        @Override
        public boolean onCheckIsTextEditor() {
            return textSession != null;
        }

        @Override
        public InputConnection onCreateInputConnection(EditorInfo outAttrs) {
            TextSession session = textSession;
            if (session == null) {
                return null;
            }
            outAttrs.inputType = session.inputType();
            outAttrs.imeOptions = session.imeOptions();
            outAttrs.initialSelStart = Selection.getSelectionStart(session.text);
            outAttrs.initialSelEnd = Selection.getSelectionEnd(session.text);
            return new HostInputConnection(this, session);
        }

        @Override
        public boolean onKeyDown(int keyCode, KeyEvent event) {
            if (textSession == null) {
                return super.onKeyDown(keyCode, event);
            }
            switch (keyCode) {
                case KeyEvent.KEYCODE_DEL:
                    MorningstarNative.nativeTextCommand(
                            MorningstarNative.TEXT_KEY_DELETE_BACKWARD);
                    return true;
                case KeyEvent.KEYCODE_FORWARD_DEL:
                    MorningstarNative.nativeTextCommand(
                            MorningstarNative.TEXT_KEY_DELETE_FORWARD);
                    return true;
                case KeyEvent.KEYCODE_ENTER:
                case KeyEvent.KEYCODE_NUMPAD_ENTER:
                    MorningstarNative.nativeTextCommand(MorningstarNative.TEXT_KEY_ENTER);
                    return true;
                case KeyEvent.KEYCODE_TAB:
                    MorningstarNative.nativeTextCommand(MorningstarNative.TEXT_KEY_NEXT);
                    return true;
                default:
                    break;
            }
            // Arrows, Home and End: of a hardware keyboard, and of input
            // methods that move their cursor with key events.
            boolean control = event.isCtrlPressed() || event.isMetaPressed();
            int move = caretMove(keyCode, control);
            if (move >= 0) {
                MorningstarNative.nativeTextMove(move, event.isShiftPressed());
                return true;
            }
            if (control) {
                int shortcut = shortcut(keyCode, event.isShiftPressed());
                if (shortcut >= 0) {
                    MorningstarNative.nativeTextCommand(shortcut);
                    return true;
                }
                return super.onKeyDown(keyCode, event);
            }
            int character = event.getUnicodeChar();
            // Dead keys (accents waiting for a letter) are not composed.
            if (character > 0
                    && (character & android.view.KeyCharacterMap.COMBINING_ACCENT) == 0
                    && !Character.isISOControl(character)) {
                MorningstarNative.nativeTextCommit(new String(Character.toChars(character)));
                return true;
            }
            return super.onKeyDown(keyCode, event);
        }
    }

    /** The TEXT_MOVE_* of a key, or -1 if it does not move the caret. */
    private static int caretMove(int keyCode, boolean control) {
        switch (keyCode) {
            case KeyEvent.KEYCODE_DPAD_LEFT:
                return control
                        ? MorningstarNative.TEXT_MOVE_WORD_LEFT
                        : MorningstarNative.TEXT_MOVE_LEFT;
            case KeyEvent.KEYCODE_DPAD_RIGHT:
                return control
                        ? MorningstarNative.TEXT_MOVE_WORD_RIGHT
                        : MorningstarNative.TEXT_MOVE_RIGHT;
            case KeyEvent.KEYCODE_DPAD_UP:
                return MorningstarNative.TEXT_MOVE_UP;
            case KeyEvent.KEYCODE_DPAD_DOWN:
                return MorningstarNative.TEXT_MOVE_DOWN;
            case KeyEvent.KEYCODE_MOVE_HOME:
                return control
                        ? MorningstarNative.TEXT_MOVE_TEXT_START
                        : MorningstarNative.TEXT_MOVE_LINE_START;
            case KeyEvent.KEYCODE_MOVE_END:
                return control
                        ? MorningstarNative.TEXT_MOVE_TEXT_END
                        : MorningstarNative.TEXT_MOVE_LINE_END;
            default:
                return -1;
        }
    }

    /** The TEXT_KEY_* of a key pressed with Ctrl (or Meta), or -1. */
    private static int shortcut(int keyCode, boolean shift) {
        switch (keyCode) {
            case KeyEvent.KEYCODE_A:
                return MorningstarNative.TEXT_KEY_SELECT_ALL;
            case KeyEvent.KEYCODE_C:
                return MorningstarNative.TEXT_KEY_COPY;
            case KeyEvent.KEYCODE_X:
                return MorningstarNative.TEXT_KEY_CUT;
            case KeyEvent.KEYCODE_V:
                return MorningstarNative.TEXT_KEY_PASTE;
            case KeyEvent.KEYCODE_Z:
                return shift ? MorningstarNative.TEXT_KEY_REDO : MorningstarNative.TEXT_KEY_UNDO;
            case KeyEvent.KEYCODE_Y:
                return MorningstarNative.TEXT_KEY_REDO;
            default:
                return -1;
        }
    }

    /**
     * The input method's handle on the session's text. BaseInputConnection
     * does the editing (commit, composing text and region, deletion,
     * selection) on the Editable returned by getEditable; after each edit,
     * or batch of edits, the result is reported to Rust.
     */
    private final class HostInputConnection extends BaseInputConnection {
        private final TextSession session;
        private int batchDepth;

        HostInputConnection(View view, TextSession session) {
            super(view, true);
            this.session = session;
        }

        @Override
        public Editable getEditable() {
            return session.text;
        }

        private void edited() {
            if (batchDepth == 0) {
                reportEdit(session);
            }
        }

        @Override
        public boolean beginBatchEdit() {
            batchDepth++;
            return true;
        }

        @Override
        public boolean endBatchEdit() {
            if (batchDepth > 0) {
                batchDepth--;
            }
            edited();
            return batchDepth > 0;
        }

        @Override
        public boolean commitText(CharSequence text, int newCursorPosition) {
            boolean result = super.commitText(text, newCursorPosition);
            edited();
            return result;
        }

        @Override
        public boolean setComposingText(CharSequence text, int newCursorPosition) {
            boolean result = super.setComposingText(text, newCursorPosition);
            edited();
            return result;
        }

        @Override
        public boolean setComposingRegion(int start, int end) {
            boolean result = super.setComposingRegion(start, end);
            edited();
            return result;
        }

        @Override
        public boolean finishComposingText() {
            boolean result = super.finishComposingText();
            edited();
            return result;
        }

        @Override
        public boolean setSelection(int start, int end) {
            boolean result = super.setSelection(start, end);
            edited();
            return result;
        }

        @Override
        public boolean deleteSurroundingText(int beforeLength, int afterLength) {
            if (beforeLength > 0
                    && afterLength == 0
                    && Selection.getSelectionStart(session.text) <= 0
                    && Selection.getSelectionEnd(session.text) <= 0) {
                // Backspace with nothing before the caret changes nothing
                // here, but Rust wants to know: in a row of code boxes it
                // moves to the previous box.
                if (session == textSession) {
                    MorningstarNative.nativeTextCommand(
                            MorningstarNative.TEXT_KEY_DELETE_BACKWARD);
                }
                return true;
            }
            boolean result = super.deleteSurroundingText(beforeLength, afterLength);
            edited();
            return result;
        }

        @Override
        public boolean deleteSurroundingTextInCodePoints(int beforeLength, int afterLength) {
            boolean result = super.deleteSurroundingTextInCodePoints(beforeLength, afterLength);
            edited();
            return result;
        }

        @Override
        public boolean performEditorAction(int actionCode) {
            if (session != textSession) {
                return false;
            }
            int key;
            switch (actionCode) {
                case EditorInfo.IME_ACTION_NEXT:
                    key = MorningstarNative.TEXT_KEY_NEXT;
                    break;
                case EditorInfo.IME_ACTION_DONE:
                    key = MorningstarNative.TEXT_KEY_DONE;
                    break;
                case EditorInfo.IME_ACTION_GO:
                    key = MorningstarNative.TEXT_KEY_GO;
                    break;
                case EditorInfo.IME_ACTION_SEARCH:
                    key = MorningstarNative.TEXT_KEY_SEARCH;
                    break;
                case EditorInfo.IME_ACTION_SEND:
                    key = MorningstarNative.TEXT_KEY_SEND;
                    break;
                default:
                    // "Unspecified": the Return key itself.
                    key = MorningstarNative.TEXT_KEY_ENTER;
                    break;
            }
            MorningstarNative.nativeTextCommand(key);
            return true;
        }

        // An input method's own "select all", "cut", "copy" and "paste"
        // (a clipboard row, a text-editing panel). Rust does them: it owns
        // the selection, the field's rules and the clipboard request.
        @Override
        public boolean performContextMenuAction(int id) {
            if (session != textSession) {
                return false;
            }
            int key;
            if (id == android.R.id.selectAll) {
                key = MorningstarNative.TEXT_KEY_SELECT_ALL;
            } else if (id == android.R.id.cut) {
                key = MorningstarNative.TEXT_KEY_CUT;
            } else if (id == android.R.id.copy) {
                key = MorningstarNative.TEXT_KEY_COPY;
            } else if (id == android.R.id.paste || id == android.R.id.pasteAsPlainText) {
                key = MorningstarNative.TEXT_KEY_PASTE;
            } else if (id == android.R.id.undo) {
                key = MorningstarNative.TEXT_KEY_UNDO;
            } else if (id == android.R.id.redo) {
                key = MorningstarNative.TEXT_KEY_REDO;
            } else {
                return false;
            }
            MorningstarNative.nativeTextCommand(key);
            return true;
        }

        // Input methods read the field through this to decide what to
        // suggest; BaseInputConnection does not implement it.
        @Override
        public ExtractedText getExtractedText(ExtractedTextRequest request, int flags) {
            ExtractedText extracted = new ExtractedText();
            extracted.text = session.text.toString();
            extracted.startOffset = 0;
            extracted.partialStartOffset = -1;
            extracted.partialEndOffset = -1;
            extracted.selectionStart = Math.max(0, Selection.getSelectionStart(session.text));
            extracted.selectionEnd = Math.max(0, Selection.getSelectionEnd(session.text));
            return extracted;
        }
    }

    /**
     * Splits a MotionEvent into one call per pointer. Batched historical
     * positions are not forwarded; only the latest position of each pointer.
     */
    private static void forwardTouch(MotionEvent event) {
        switch (event.getActionMasked()) {
            case MotionEvent.ACTION_DOWN:
            case MotionEvent.ACTION_POINTER_DOWN:
                sendPointer(MorningstarNative.PHASE_DOWN, event, event.getActionIndex());
                break;
            case MotionEvent.ACTION_UP:
            case MotionEvent.ACTION_POINTER_UP:
                sendPointer(MorningstarNative.PHASE_UP, event, event.getActionIndex());
                break;
            case MotionEvent.ACTION_MOVE:
                for (int i = 0; i < event.getPointerCount(); i++) {
                    sendPointer(MorningstarNative.PHASE_MOVED, event, i);
                }
                break;
            case MotionEvent.ACTION_CANCEL:
                for (int i = 0; i < event.getPointerCount(); i++) {
                    sendPointer(MorningstarNative.PHASE_CANCELLED, event, i);
                }
                break;
            default:
                break;
        }
    }

    private static void sendPointer(int phase, MotionEvent event, int index) {
        MorningstarNative.nativePointer(
                phase, event.getPointerId(index), event.getX(index), event.getY(index));
    }

    private String applicationLibraryName() {
        try {
            ApplicationInfo info =
                    getPackageManager()
                            .getApplicationInfo(getPackageName(), PackageManager.GET_META_DATA);
            String name = info.metaData == null ? null : info.metaData.getString(LIB_NAME_KEY);
            if (name == null || name.isEmpty()) {
                throw new IllegalStateException(
                        "AndroidManifest.xml is missing <meta-data android:name=\""
                                + LIB_NAME_KEY
                                + "\">");
            }
            return name;
        } catch (PackageManager.NameNotFoundException e) {
            throw new IllegalStateException(e);
        }
    }
}
