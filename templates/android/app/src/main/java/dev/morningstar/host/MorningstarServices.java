package dev.morningstar.host;

import android.Manifest;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.content.SharedPreferences;
import android.content.pm.PackageManager;
import android.media.AudioAttributes;
import android.media.AudioFocusRequest;
import android.media.AudioManager;
import android.media.MediaPlayer;
import android.media.MediaRecorder;
import android.os.Build;
import android.os.Handler;
import android.os.HandlerThread;
import android.os.Looper;
import android.os.SystemClock;
import android.util.Log;
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.ConnectException;
import java.net.HttpURLConnection;
import java.net.MalformedURLException;
import java.net.NoRouteToHostException;
import java.net.SocketException;
import java.net.SocketTimeoutException;
import java.net.URL;
import java.net.UnknownHostException;
import java.security.SecureRandom;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.ScheduledFuture;
import java.util.concurrent.ThreadFactory;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicInteger;

/**
 * The platform services an application can ask for: HTTP requests, recording
 * from the microphone, playing a clip, a few stored values, and the
 * clipboard. See docs/architecture/0019-effects-and-platform-services.md and
 * 0025-text-selection-and-clipboard.md.
 *
 * Rust asks from its render thread (MorningstarNative.onHttpRequest and
 * onServiceCommand), and those calls only hand the work on:
 *
 *   - HTTP runs on a small pool of worker threads, one request per thread.
 *   - Audio and storage run on one thread with a Looper ("the service
 *     thread"), in the order they were asked for. MediaRecorder and
 *     MediaPlayer are created there, so their callbacks arrive there too,
 *     and every field below marked "service thread" is touched nowhere
 *     else.
 *
 * Nothing here runs on the main thread except asking the user for the
 * microphone, which Android only allows from an activity, and the clipboard,
 * which is quick and which Android only lets the focused app read. Every request that
 * expects an answer gets exactly one, through a native method of
 * MorningstarNative, from whichever thread finished the work.
 *
 * The class holds the application context, never an activity: the Rust
 * runtime outlives activities, and so does this. An activity is looked up
 * (MorningstarActivity.current()) only at the moment a permission has to be
 * asked for.
 */
final class MorningstarServices {
    private MorningstarServices() {}

    private static final String TAG = "Morningstar";

    /** The host's own values: the installation id. Not visible to the app. */
    private static final String HOST_PREFERENCES = "morningstar_host";

    private static final String INSTALL_ID_KEY = "install_id";

    /** The application's stored values (Effects::store). */
    private static final String STORE_PREFERENCES = "morningstar_store";

    /** Must match MAX_HTTP_RESPONSE_BYTES in morningstar-core. */
    private static final int MAX_RESPONSE_BYTES = 10 * 1024 * 1024;

    /** Must match MAX_RECORDING_DURATION in morningstar-core. */
    private static final int MAX_RECORDING_MS = 60_000;

    /** requestPermissions request code for the microphone. */
    static final int REQUEST_RECORD_AUDIO = 0x4d53;

    private static final String RECORDING_FILE = "morningstar-recording.m4a";
    private static final String CLIP_PREFIX = "morningstar-clip-";

    private static volatile Context context;
    private static Handler service;
    private static ExecutorService httpPool;
    private static ScheduledExecutorService watchdog;

    /**
     * Prepares the threads and remembers the application context. Called
     * from every Activity.onCreate; only the first call of a process does
     * anything. Main thread.
     */
    static void init(Context applicationContext) {
        if (context != null) {
            return;
        }
        context = applicationContext.getApplicationContext();
        HandlerThread thread = new HandlerThread("morningstar-services");
        thread.start();
        service = new Handler(thread.getLooper());
        httpPool = Executors.newFixedThreadPool(4, new Daemons("morningstar-http"));
        watchdog = Executors.newSingleThreadScheduledExecutor(new Daemons("morningstar-http-timer"));
        // What a crash or a killed process left behind.
        service.post(MorningstarServices::deleteTemporaryFiles);
    }

    private static final class Daemons implements ThreadFactory {
        private final String name;
        private final AtomicInteger count = new AtomicInteger();

        Daemons(String name) {
            this.name = name;
        }

        @Override
        public Thread newThread(Runnable work) {
            Thread thread = new Thread(work, name + "-" + count.incrementAndGet());
            thread.setDaemon(true);
            return thread;
        }
    }

    // ------------------------------------------------- install id, snapshot

    /**
     * The identifier of this installation: 128 bits from SecureRandom as 32
     * hexadecimal digits, made on first use and kept in the app's private
     * preferences. It goes when the app's data goes. Main thread, at start.
     */
    static String installId() {
        SharedPreferences preferences =
                context.getSharedPreferences(HOST_PREFERENCES, Context.MODE_PRIVATE);
        String id = preferences.getString(INSTALL_ID_KEY, null);
        if (id == null || id.length() != 32) {
            byte[] random = new byte[16];
            new SecureRandom().nextBytes(random);
            StringBuilder hex = new StringBuilder(32);
            for (byte value : random) {
                hex.append(Character.forDigit((value >> 4) & 0xf, 16));
                hex.append(Character.forDigit(value & 0xf, 16));
            }
            id = hex.toString();
            // commit, not apply: the id must not be handed out and then lost.
            preferences.edit().putString(INSTALL_ID_KEY, id).commit();
        }
        return id;
    }

    /**
     * Everything the application has stored, as keys and values in turn,
     * for it to read at setup. Main thread, at start: this reads a small
     * file once per process, which is the price of the application knowing
     * its values before its first frame.
     */
    static String[] storedSnapshot() {
        Map<String, ?> all =
                context.getSharedPreferences(STORE_PREFERENCES, Context.MODE_PRIVATE).getAll();
        List<String> flat = new ArrayList<>(all.size() * 2);
        for (Map.Entry<String, ?> entry : all.entrySet()) {
            if (entry.getValue() instanceof String) {
                flat.add(entry.getKey());
                flat.add((String) entry.getValue());
            }
        }
        return flat.toArray(new String[0]);
    }

    // ------------------------------------------------------------- commands

    /** MorningstarNative.onServiceCommand. Render thread: only posts. */
    static void command(
            final int command,
            final long id,
            final String first,
            final String second,
            final byte[] data) {
        Handler handler = service;
        if (handler == null) {
            // Rust is running although no activity ever called init: there
            // is nothing to do the work.
            unsupported(command, id);
            return;
        }
        if (command == MorningstarNative.SERVICE_SET_CLIPBOARD
                || command == MorningstarNative.SERVICE_READ_CLIPBOARD) {
            MAIN.post(
                    () -> {
                        if (command == MorningstarNative.SERVICE_SET_CLIPBOARD) {
                            setClipboard(first);
                        } else {
                            readClipboard(id);
                        }
                    });
            return;
        }
        handler.post(
                () -> {
                    switch (command) {
                        case MorningstarNative.SERVICE_START_RECORDING:
                            startRecording(id);
                            break;
                        case MorningstarNative.SERVICE_STOP_RECORDING:
                            stopRecording(id);
                            break;
                        case MorningstarNative.SERVICE_PLAY:
                            play(id, first, data);
                            break;
                        case MorningstarNative.SERVICE_STOP_PLAYBACK:
                            endPlayback(MorningstarNative.AUDIO_INTERRUPTED, null);
                            break;
                        case MorningstarNative.SERVICE_STORE:
                            store().edit().putString(first, second).apply();
                            break;
                        case MorningstarNative.SERVICE_REMOVE:
                            store().edit().remove(first).apply();
                            break;
                        case MorningstarNative.SERVICE_LOAD:
                            String value = null;
                            try {
                                value = store().getString(first, null);
                            } catch (ClassCastException notAString) {
                                // Only strings are ever stored by this class.
                            }
                            MorningstarNative.nativeLoadResult(id, value != null, value);
                            break;
                        default:
                            // A library newer than this host.
                            unsupported(command, id);
                            break;
                    }
                });
    }

    /** Answers a command this host cannot carry out, if it has an answer. */
    private static void unsupported(int command, long id) {
        switch (command) {
            case MorningstarNative.SERVICE_START_RECORDING:
                audioResult(id, MorningstarNative.AUDIO_KIND_STARTED, MorningstarNative.AUDIO_UNSUPPORTED, null);
                break;
            case MorningstarNative.SERVICE_STOP_RECORDING:
                audioResult(id, MorningstarNative.AUDIO_KIND_RECORDING, MorningstarNative.AUDIO_UNSUPPORTED, null);
                break;
            case MorningstarNative.SERVICE_PLAY:
                audioResult(id, MorningstarNative.AUDIO_KIND_PLAYED, MorningstarNative.AUDIO_UNSUPPORTED, null);
                break;
            case MorningstarNative.SERVICE_LOAD:
                MorningstarNative.nativeLoadResult(id, false, null);
                break;
            case MorningstarNative.SERVICE_READ_CLIPBOARD:
                MorningstarNative.nativeClipboardResult(id, false, null);
                break;
            default:
                break;
        }
    }

    // ------------------------------------------------------------ clipboard

    private static final Handler MAIN = new Handler(Looper.getMainLooper());

    /** Puts plain text on the clipboard. Main thread. */
    private static void setClipboard(String text) {
        try {
            ClipboardManager clipboard =
                    (ClipboardManager) context.getSystemService(Context.CLIPBOARD_SERVICE);
            if (clipboard != null) {
                // The label is for the system's own UI and is never shown
                // with the text; nothing about the text is logged.
                clipboard.setPrimaryClip(ClipData.newPlainText("text", text));
            }
        } catch (RuntimeException refused) {
            // A clip too large for a binder transaction, or a device policy
            // that forbids copying: the text is simply not copied.
            Log.w(TAG, "clipboard: could not copy (" + refused.getClass().getSimpleName() + ")");
        }
    }

    /**
     * Reads the clipboard as plain text and answers with it. Called only
     * when the user chose Paste: from Android 12 the system shows "pasted
     * from your clipboard" each time an app reads what another app copied,
     * and from Android 10 only the focused app may read at all. Formatting
     * is dropped (coerceToText): a styled clip pastes as its characters.
     * Main thread.
     */
    private static void readClipboard(long id) {
        String text = null;
        try {
            ClipboardManager clipboard =
                    (ClipboardManager) context.getSystemService(Context.CLIPBOARD_SERVICE);
            ClipData clip = clipboard == null ? null : clipboard.getPrimaryClip();
            if (clip != null && clip.getItemCount() > 0) {
                StringBuilder all = new StringBuilder();
                for (int index = 0; index < clip.getItemCount(); index++) {
                    CharSequence item = clip.getItemAt(index).coerceToText(context);
                    if (item != null && item.length() > 0) {
                        if (all.length() > 0) {
                            all.append('\n');
                        }
                        all.append(item);
                    }
                }
                if (all.length() > 0) {
                    text = all.toString();
                }
            }
        } catch (RuntimeException refused) {
            // Not focused, or a content URI that cannot be opened.
            Log.w(TAG, "clipboard: could not read (" + refused.getClass().getSimpleName() + ")");
        }
        MorningstarNative.nativeClipboardResult(id, text != null, text);
    }

    private static SharedPreferences store() {
        return context.getSharedPreferences(STORE_PREFERENCES, Context.MODE_PRIVATE);
    }

    private static void audioResult(long id, int kind, int error, String message) {
        MorningstarNative.nativeAudioResult(id, kind, error, message, null, null, 0);
    }

    // ------------------------------------------------------------ lifecycle

    /** Whether an activity is on screen. Service thread. */
    private static boolean onScreen;

    /** The activity came on screen: sound is allowed again. */
    static void activityStarted() {
        Handler handler = service;
        if (handler != null) {
            handler.post(() -> onScreen = true);
        }
    }

    /**
     * The activity left the screen. Sound stops with it: the clip that is
     * playing ends as interrupted, and a recording is closed and kept for
     * the stop that will ask for it. (Recording in the background would
     * need a foreground service, which this host does not have.)
     */
    static void activityStopped() {
        Handler handler = service;
        if (handler != null) {
            handler.post(
                    () -> {
                        onScreen = false;
                        endPlayback(MorningstarNative.AUDIO_INTERRUPTED, null);
                        finishRecording();
                    });
        }
    }

    /**
     * The activity is gone. If the microphone question was on screen its
     * answer will never come, so the start it belonged to fails now.
     */
    static void activityDestroyed() {
        Handler handler = service;
        if (handler != null) {
            handler.post(() -> permissionAnswered(false, "the activity was destroyed"));
        }
    }

    /** Activity.onRequestPermissionsResult, main thread. */
    static void onPermissionResult(int requestCode, int[] grantResults) {
        Handler handler = service;
        if (requestCode != REQUEST_RECORD_AUDIO || handler == null) {
            return;
        }
        // An empty array means the question was cancelled: not granted.
        final boolean granted =
                grantResults.length > 0 && grantResults[0] == PackageManager.PERMISSION_GRANTED;
        handler.post(() -> permissionAnswered(granted, null));
    }

    // ------------------------------------------------------------ recording
    // Everything from here to "playback" runs on the service thread.

    /** The recorder while a recording is running. Service thread. */
    private static MediaRecorder recorder;

    private static long recordingStartedAt;

    /** A recording that was closed and not yet asked for, or null. */
    private static File finishedRecording;

    private static long finishedDurationMs;

    /** The start waiting for the user's answer about the microphone, or 0. */
    private static long startAwaitingPermission;

    private static void startRecording(long id) {
        if (recorder != null || startAwaitingPermission != 0) {
            audioResult(id, MorningstarNative.AUDIO_KIND_STARTED, MorningstarNative.AUDIO_BUSY, null);
            return;
        }
        if (!onScreen) {
            // Android gives a background app silence, not the microphone.
            audioResult(
                    id,
                    MorningstarNative.AUDIO_KIND_STARTED,
                    MorningstarNative.AUDIO_FAILED,
                    "the app is not on screen");
            return;
        }
        if (!context.getPackageManager().hasSystemFeature(PackageManager.FEATURE_MICROPHONE)) {
            audioResult(id, MorningstarNative.AUDIO_KIND_STARTED, MorningstarNative.AUDIO_UNSUPPORTED, null);
            return;
        }
        if (context.checkSelfPermission(Manifest.permission.RECORD_AUDIO)
                == PackageManager.PERMISSION_GRANTED) {
            beginRecording(id);
            return;
        }
        // Ask. Android shows its dialog, or answers "denied" at once if
        // the user has refused for good; either way the answer arrives in
        // onPermissionResult. So a later start always asks again.
        startAwaitingPermission = id;
        new Handler(Looper.getMainLooper())
                .post(
                        () -> {
                            MorningstarActivity activity = MorningstarActivity.current();
                            if (activity == null) {
                                // Nobody to ask.
                                service.post(() -> permissionAnswered(false, null));
                            } else {
                                activity.requestPermissions(
                                        new String[] {Manifest.permission.RECORD_AUDIO},
                                        REQUEST_RECORD_AUDIO);
                            }
                        });
    }

    private static void permissionAnswered(boolean granted, String failure) {
        long id = startAwaitingPermission;
        if (id == 0) {
            return;
        }
        startAwaitingPermission = 0;
        if (granted) {
            beginRecording(id);
        } else if (failure != null) {
            audioResult(id, MorningstarNative.AUDIO_KIND_STARTED, MorningstarNative.AUDIO_FAILED, failure);
        } else {
            Log.i(TAG, "services: microphone permission denied");
            audioResult(
                    id, MorningstarNative.AUDIO_KIND_STARTED, MorningstarNative.AUDIO_PERMISSION_DENIED, null);
        }
    }

    private static void beginRecording(long id) {
        // A recording nobody collected is replaced by the new one.
        discardFinishedRecording();
        File file = new File(context.getCacheDir(), RECORDING_FILE);
        // Speech: 16 kHz mono AAC is small and is what speech recognisers
        // want. If this device's encoder refuses that rate, 44.1 kHz is
        // the one every device accepts.
        int[][] settings = {{16_000, 32_000}, {44_100, 64_000}};
        RuntimeException lastFailure = null;
        for (int[] setting : settings) {
            //noinspection ResultOfMethodCallIgnored
            file.delete();
            MediaRecorder candidate = newRecorder();
            try {
                candidate.setAudioSource(MediaRecorder.AudioSource.MIC);
                candidate.setOutputFormat(MediaRecorder.OutputFormat.MPEG_4);
                candidate.setAudioEncoder(MediaRecorder.AudioEncoder.AAC);
                candidate.setAudioChannels(1);
                candidate.setAudioSamplingRate(setting[0]);
                candidate.setAudioEncodingBitRate(setting[1]);
                candidate.setMaxDuration(MAX_RECORDING_MS);
                candidate.setOutputFile(file.getAbsolutePath());
                candidate.setOnInfoListener(
                        (which, what, extra) -> {
                            if (what == MediaRecorder.MEDIA_RECORDER_INFO_MAX_DURATION_REACHED
                                    && which == recorder) {
                                Log.i(TAG, "services: recording reached its maximum length");
                                finishRecording();
                            }
                        });
                candidate.setOnErrorListener(
                        (which, what, extra) -> {
                            if (which == recorder) {
                                Log.w(TAG, "services: recorder error " + what + "/" + extra);
                                finishRecording();
                            }
                        });
                candidate.prepare();
                candidate.start();
            } catch (IOException | RuntimeException failure) {
                candidate.release();
                lastFailure =
                        failure instanceof RuntimeException
                                ? (RuntimeException) failure
                                : new RuntimeException(failure);
                continue;
            }
            recorder = candidate;
            recordingStartedAt = SystemClock.elapsedRealtime();
            Log.i(TAG, "services: recording at " + setting[0] + " Hz");
            audioResult(id, MorningstarNative.AUDIO_KIND_STARTED, MorningstarNative.AUDIO_OK, null);
            return;
        }
        //noinspection ResultOfMethodCallIgnored
        file.delete();
        // start() fails with an IllegalStateException when another app
        // holds the microphone; nothing more precise is reported.
        Log.w(TAG, "services: recording could not start", lastFailure);
        audioResult(
                id,
                MorningstarNative.AUDIO_KIND_STARTED,
                MorningstarNative.AUDIO_FAILED,
                String.valueOf(lastFailure));
    }

    @SuppressWarnings("deprecation") // The only constructor before API 31.
    private static MediaRecorder newRecorder() {
        return Build.VERSION.SDK_INT >= 31 ? Api31.newRecorder(context) : new MediaRecorder();
    }

    /**
     * Closes the running recording, if any, and keeps its file for the
     * stop that will ask for it. A recording too short to hold any sound
     * makes MediaRecorder.stop throw; it is then kept as nothing.
     */
    private static void finishRecording() {
        MediaRecorder running = recorder;
        if (running == null) {
            return;
        }
        recorder = null;
        long duration = SystemClock.elapsedRealtime() - recordingStartedAt;
        File file = new File(context.getCacheDir(), RECORDING_FILE);
        boolean valid = true;
        try {
            running.stop();
        } catch (RuntimeException nothingRecorded) {
            valid = false;
        }
        running.release();
        if (valid && file.length() > 0) {
            finishedRecording = file;
            finishedDurationMs = Math.min(duration, MAX_RECORDING_MS);
        } else {
            //noinspection ResultOfMethodCallIgnored
            file.delete();
            finishedRecording = null;
            // Remembered as "a recording was made, and it was empty".
            finishedDurationMs = -1;
        }
    }

    private static void discardFinishedRecording() {
        if (finishedRecording != null) {
            //noinspection ResultOfMethodCallIgnored
            finishedRecording.delete();
        }
        finishedRecording = null;
        finishedDurationMs = 0;
    }

    private static void stopRecording(long id) {
        finishRecording();
        File file = finishedRecording;
        if (file == null) {
            boolean empty = finishedDurationMs < 0;
            finishedDurationMs = 0;
            if (empty) {
                audioResult(
                        id,
                        MorningstarNative.AUDIO_KIND_RECORDING,
                        MorningstarNative.AUDIO_FAILED,
                        "nothing was recorded");
            } else {
                audioResult(
                        id, MorningstarNative.AUDIO_KIND_RECORDING, MorningstarNative.AUDIO_NOT_RECORDING, null);
            }
            return;
        }
        long duration = finishedDurationMs;
        byte[] bytes = null;
        String failure = null;
        try (FileInputStream in = new FileInputStream(file)) {
            bytes = readAll(in, MAX_RESPONSE_BYTES);
            if (bytes == null) {
                failure = "the recording is too large";
            }
        } catch (IOException unreadable) {
            failure = String.valueOf(unreadable);
        }
        discardFinishedRecording();
        if (bytes == null) {
            audioResult(id, MorningstarNative.AUDIO_KIND_RECORDING, MorningstarNative.AUDIO_FAILED, failure);
        } else {
            MorningstarNative.nativeAudioResult(
                    id,
                    MorningstarNative.AUDIO_KIND_RECORDING,
                    MorningstarNative.AUDIO_OK,
                    null,
                    bytes,
                    "audio/mp4",
                    duration);
        }
    }

    // ------------------------------------------------------------- playback
    // Service thread.

    private static MediaPlayer player;

    /** The request whose clip is playing or being prepared, or 0. */
    private static long playingId;

    private static File playingFile;
    private static int clipCount;

    /** The audio focus held for the clip: an Api26 request or a listener. */
    private static Object focusToken;

    private static final AudioManager.OnAudioFocusChangeListener FOCUS_LISTENER =
            change -> {
                // May be called on another thread before API 26.
                Handler handler = service;
                if (handler != null
                        && (change == AudioManager.AUDIOFOCUS_LOSS
                                || change == AudioManager.AUDIOFOCUS_LOSS_TRANSIENT)) {
                    handler.post(() -> endPlayback(MorningstarNative.AUDIO_INTERRUPTED, null));
                }
            };

    private static void play(final long id, String mime, byte[] data) {
        // One clip at a time: the one playing ends as interrupted.
        endPlayback(MorningstarNative.AUDIO_INTERRUPTED, null);
        if (data == null || data.length == 0) {
            audioResult(id, MorningstarNative.AUDIO_KIND_PLAYED, MorningstarNative.AUDIO_FAILED, "the clip is empty");
            return;
        }
        if (!onScreen) {
            // An answer that arrived while the app was in the background
            // must not start talking from there.
            audioResult(id, MorningstarNative.AUDIO_KIND_PLAYED, MorningstarNative.AUDIO_INTERRUPTED, null);
            return;
        }
        // Played from a file in the cache directory. The player recognises
        // the container from its contents; the extension is only for
        // whoever looks into the directory.
        File file =
                new File(context.getCacheDir(), CLIP_PREFIX + (++clipCount) + extensionFor(mime));
        MediaPlayer candidate = new MediaPlayer();
        try {
            try (FileOutputStream out = new FileOutputStream(file)) {
                out.write(data);
            }
            candidate.setAudioAttributes(
                    new AudioAttributes.Builder()
                            .setUsage(AudioAttributes.USAGE_MEDIA)
                            .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
                            .build());
            try (FileInputStream in = new FileInputStream(file)) {
                // The player keeps its own duplicate of the descriptor.
                candidate.setDataSource(in.getFD());
            }
        } catch (IOException | RuntimeException failure) {
            candidate.release();
            //noinspection ResultOfMethodCallIgnored
            file.delete();
            audioResult(
                    id,
                    MorningstarNative.AUDIO_KIND_PLAYED,
                    MorningstarNative.AUDIO_FAILED,
                    String.valueOf(failure));
            return;
        }
        player = candidate;
        playingId = id;
        playingFile = file;
        candidate.setOnPreparedListener(
                prepared -> {
                    if (prepared != player) {
                        return;
                    }
                    if (!requestFocus()) {
                        // A phone call, typically.
                        endPlayback(MorningstarNative.AUDIO_BUSY, null);
                        return;
                    }
                    try {
                        prepared.start();
                    } catch (RuntimeException failure) {
                        endPlayback(MorningstarNative.AUDIO_FAILED, String.valueOf(failure));
                    }
                });
        candidate.setOnCompletionListener(
                finished -> {
                    if (finished == player) {
                        endPlayback(MorningstarNative.AUDIO_OK, null);
                    }
                });
        candidate.setOnErrorListener(
                (failed, what, extra) -> {
                    if (failed == player) {
                        endPlayback(
                                MorningstarNative.AUDIO_FAILED,
                                "the clip could not be played (" + what + "/" + extra + ")");
                    }
                    // Handled: no completion callback follows.
                    return true;
                });
        try {
            candidate.prepareAsync();
        } catch (RuntimeException failure) {
            endPlayback(MorningstarNative.AUDIO_FAILED, String.valueOf(failure));
        }
    }

    /**
     * Ends the clip that is playing or being prepared, if any: releases the
     * player and the audio focus, deletes the file, and answers the play
     * request, once, with this outcome.
     */
    private static void endPlayback(int error, String message) {
        MediaPlayer ending = player;
        if (ending == null) {
            return;
        }
        long id = playingId;
        player = null;
        playingId = 0;
        // release() is allowed in every state, and stops the sound.
        ending.release();
        abandonFocus();
        if (playingFile != null) {
            //noinspection ResultOfMethodCallIgnored
            playingFile.delete();
            playingFile = null;
        }
        audioResult(id, MorningstarNative.AUDIO_KIND_PLAYED, error, message);
    }

    private static String extensionFor(String mime) {
        String type = mime == null ? "" : mime.toLowerCase(java.util.Locale.ROOT);
        int parameters = type.indexOf(';');
        if (parameters >= 0) {
            type = type.substring(0, parameters).trim();
        }
        switch (type) {
            case "audio/mpeg":
            case "audio/mp3":
                return ".mp3";
            case "audio/mp4":
            case "audio/m4a":
            case "audio/x-m4a":
            case "audio/aac":
                return ".m4a";
            case "audio/wav":
            case "audio/x-wav":
                return ".wav";
            case "audio/ogg":
                return ".ogg";
            default:
                return ".audio";
        }
    }

    @SuppressWarnings("deprecation") // The only way before API 26.
    private static boolean requestFocus() {
        AudioManager audio = (AudioManager) context.getSystemService(Context.AUDIO_SERVICE);
        if (audio == null) {
            return true;
        }
        int granted;
        if (Build.VERSION.SDK_INT >= 26) {
            Object request = Api26.focusRequest(FOCUS_LISTENER, service);
            granted = Api26.request(audio, request);
            focusToken = request;
        } else {
            granted =
                    audio.requestAudioFocus(
                            FOCUS_LISTENER,
                            AudioManager.STREAM_MUSIC,
                            AudioManager.AUDIOFOCUS_GAIN_TRANSIENT);
            focusToken = FOCUS_LISTENER;
        }
        if (granted != AudioManager.AUDIOFOCUS_REQUEST_GRANTED) {
            focusToken = null;
            return false;
        }
        return true;
    }

    @SuppressWarnings("deprecation") // The only way before API 26.
    private static void abandonFocus() {
        Object token = focusToken;
        focusToken = null;
        AudioManager audio = (AudioManager) context.getSystemService(Context.AUDIO_SERVICE);
        if (token == null || audio == null) {
            return;
        }
        if (Build.VERSION.SDK_INT >= 26) {
            Api26.abandon(audio, token);
        } else {
            audio.abandonAudioFocus(FOCUS_LISTENER);
        }
    }

    /** Classes added in API 26, kept apart so this class loads on API 24. */
    private static final class Api26 {
        private Api26() {}

        static Object focusRequest(
                AudioManager.OnAudioFocusChangeListener listener, Handler handler) {
            return new AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT)
                    .setAudioAttributes(
                            new AudioAttributes.Builder()
                                    .setUsage(AudioAttributes.USAGE_MEDIA)
                                    .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
                                    .build())
                    .setOnAudioFocusChangeListener(listener, handler)
                    .build();
        }

        static int request(AudioManager audio, Object request) {
            return audio.requestAudioFocus((AudioFocusRequest) request);
        }

        static void abandon(AudioManager audio, Object request) {
            audio.abandonAudioFocusRequest((AudioFocusRequest) request);
        }
    }

    /** The constructor added in API 31, kept apart likewise. */
    private static final class Api31 {
        private Api31() {}

        static MediaRecorder newRecorder(Context context) {
            return new MediaRecorder(context);
        }
    }

    private static void deleteTemporaryFiles() {
        File[] files = context.getCacheDir().listFiles();
        if (files == null) {
            return;
        }
        for (File file : files) {
            String name = file.getName();
            if (name.equals(RECORDING_FILE) || name.startsWith(CLIP_PREFIX)) {
                //noinspection ResultOfMethodCallIgnored
                file.delete();
            }
        }
    }

    /**
     * Reads a stream to its end. Returns null, having stopped reading, if
     * it holds more than limit bytes.
     */
    private static byte[] readAll(InputStream in, int limit) throws IOException {
        ByteArrayOutputStream out = new ByteArrayOutputStream(8192);
        byte[] buffer = new byte[16 * 1024];
        int count;
        while ((count = in.read(buffer)) != -1) {
            if (out.size() + count > limit) {
                return null;
            }
            out.write(buffer, 0, count);
        }
        return out.toByteArray();
    }

    // ----------------------------------------------------------------- HTTP

    /** MorningstarNative.onHttpRequest. Render thread: only hands on. */
    static void http(
            final long id,
            final String method,
            final String url,
            final String headers,
            final byte[] body,
            final int timeoutMs) {
        ExecutorService pool = httpPool;
        if (pool == null) {
            MorningstarNative.nativeHttpResult(
                    id, MorningstarNative.HTTP_UNSUPPORTED, 0, null, null, null);
            return;
        }
        pool.execute(() -> runHttp(id, method, url, headers, body, timeoutMs));
    }

    /** One whole exchange, on a worker thread. Answers exactly once. */
    private static void runHttp(
            long id, String method, String url, String headers, byte[] body, int timeoutMs) {
        URL target;
        try {
            target = new URL(url);
        } catch (MalformedURLException malformed) {
            MorningstarNative.nativeHttpResult(
                    id, MorningstarNative.HTTP_INVALID_URL, 0, null, null, null);
            return;
        }
        String protocol = target.getProtocol();
        if (!("https".equals(protocol) || "http".equals(protocol))
                || target.getHost() == null
                || target.getHost().isEmpty()) {
            MorningstarNative.nativeHttpResult(
                    id, MorningstarNative.HTTP_INVALID_URL, 0, null, null, null);
            return;
        }

        HttpURLConnection connection = null;
        // The timeout is for the whole exchange. The socket timeouts below
        // bound each connect and each read; this bounds their sum, by
        // closing the connection under the blocked call.
        final AtomicBoolean timedOut = new AtomicBoolean();
        ScheduledFuture<?> deadline = null;
        int error = MorningstarNative.HTTP_OK;
        String message = null;
        int status = 0;
        String responseHeaders = null;
        byte[] responseBody = null;
        try {
            // Platform TLS, trust store, proxy and redirect handling. The
            // manifest's network security config decides about cleartext.
            connection = (HttpURLConnection) target.openConnection();
            final HttpURLConnection open = connection;
            deadline =
                    watchdog.schedule(
                            () -> {
                                timedOut.set(true);
                                open.disconnect();
                            },
                            timeoutMs,
                            TimeUnit.MILLISECONDS);
            connection.setConnectTimeout(timeoutMs);
            connection.setReadTimeout(timeoutMs);
            connection.setUseCaches(false);
            connection.setRequestMethod(method);
            for (String line : headers.split("\n")) {
                int colon = line.indexOf(':');
                if (colon > 0) {
                    connection.addRequestProperty(
                            line.substring(0, colon).trim(), line.substring(colon + 1).trim());
                }
            }
            // HttpURLConnection turns a GET with a body into a POST, so a
            // body is only sent with the methods that carry one.
            boolean carriesBody =
                    "POST".equals(method) || "PUT".equals(method) || "PATCH".equals(method);
            if (carriesBody || ("DELETE".equals(method) && body.length > 0)) {
                // Not in streaming mode: the body is already in memory, and
                // a buffered body lets the platform retry on a stale pooled
                // connection and answer an authentication challenge with a
                // response instead of an exception.
                connection.setDoOutput(true);
                try (OutputStream out = connection.getOutputStream()) {
                    out.write(body);
                }
            }

            status = connection.getResponseCode();
            StringBuilder block = new StringBuilder();
            for (Map.Entry<String, List<String>> field : connection.getHeaderFields().entrySet()) {
                // The status line is listed under a null name.
                if (field.getKey() == null || field.getValue() == null) {
                    continue;
                }
                for (String value : field.getValue()) {
                    block.append(field.getKey())
                            .append(": ")
                            .append(value == null ? "" : value.replace('\n', ' ').replace('\r', ' '))
                            .append('\n');
                }
            }
            responseHeaders = block.toString();

            if (connection.getContentLengthLong() > MAX_RESPONSE_BYTES) {
                error = MorningstarNative.HTTP_TOO_LARGE;
            } else {
                // 4xx and 5xx bodies are on the error stream, which is null
                // when there is no body.
                InputStream in =
                        status >= 400 ? connection.getErrorStream() : connection.getInputStream();
                if (in == null) {
                    responseBody = new byte[0];
                } else {
                    try (InputStream closing = in) {
                        responseBody = readAll(closing, MAX_RESPONSE_BYTES);
                    }
                    if (responseBody == null) {
                        error = MorningstarNative.HTTP_TOO_LARGE;
                    }
                }
            }
        } catch (SocketTimeoutException slow) {
            error = MorningstarNative.HTTP_TIMEOUT;
        } catch (UnknownHostException | ConnectException | NoRouteToHostException unreachable) {
            error = MorningstarNative.HTTP_OFFLINE;
        } catch (javax.net.ssl.SSLException tls) {
            error = MorningstarNative.HTTP_FAILED;
            message = "TLS: " + tls.getMessage();
        } catch (SocketException dropped) {
            // Connection reset, network unreachable, socket closed.
            error = MorningstarNative.HTTP_OFFLINE;
        } catch (IOException | RuntimeException other) {
            // Includes "Cleartext HTTP traffic to ... not permitted" and a
            // method or header HttpURLConnection refuses.
            error = MorningstarNative.HTTP_FAILED;
            message = String.valueOf(other);
        } finally {
            if (deadline != null) {
                deadline.cancel(false);
            }
            if (connection != null) {
                connection.disconnect();
            }
        }
        if (error != MorningstarNative.HTTP_OK && timedOut.get()) {
            // Whatever the blocked call threw, the cause was the deadline.
            error = MorningstarNative.HTTP_TIMEOUT;
            message = null;
        }
        if (error == MorningstarNative.HTTP_OK) {
            MorningstarNative.nativeHttpResult(
                    id, error, status, responseHeaders, responseBody, null);
        } else {
            Log.i(TAG, "services: http failed (" + error + ")" + (message == null ? "" : " " + message));
            MorningstarNative.nativeHttpResult(id, error, 0, null, null, message);
        }
    }
}
