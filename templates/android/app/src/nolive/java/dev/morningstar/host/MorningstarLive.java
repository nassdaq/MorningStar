package dev.morningstar.host;

import android.util.Log;

/**
 * The live room (docs/architecture/0038-live-rooms.md) in an app that has
 * not asked for it: nothing can be joined, and a join is told so at once.
 *
 * The live room is carried by LiveKit's client, which brings WebRTC with it:
 * tens of megabytes that an app without calls should not ship. So the
 * service is opt-in. An app that joins live rooms says so in its Cargo.toml,
 *
 *   [package.metadata.morningstar]
 *   services = ["live"]
 *
 * and tools/build-android.sh then builds the host with -Pmorningstar.services=live,
 * which compiles src/live/java in place of this directory and adds the
 * dependency (app/build.gradle.kts). The two classes have the same name and
 * the same methods; exactly one of them is in any APK.
 */
final class MorningstarLive {
    private MorningstarLive() {}

    private static final String TAG = "Morningstar";

    /** MorningstarNative.onServiceCommand, for the SERVICE_LIVE_* commands. Render thread. */
    static void command(int command, long id, String first, String second, byte[] data) {
        if (command == MorningstarNative.SERVICE_LIVE_JOIN) {
            // Neither the address nor the ticket is looked at or logged.
            Log.w(
                    TAG,
                    "live: join id="
                            + id
                            + " refused: live rooms are not built into this app (declare"
                            + " services = [\"live\"] under [package.metadata.morningstar]"
                            + " in its Cargo.toml and build again)");
            MorningstarNative.nativeLiveEvent(
                    id,
                    MorningstarNative.LIVE_FAILED,
                    MorningstarNative.LIVE_ERROR_UNSUPPORTED,
                    null,
                    null);
        }
        // Leaving, muting and the loudspeaker have no answer and no room.
    }

    /** The activity came on screen. Main thread. */
    static void activityStarted() {}

    /** The activity left the screen. Main thread. */
    static void activityStopped() {}
}
