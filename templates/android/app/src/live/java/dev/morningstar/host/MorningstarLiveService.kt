package dev.morningstar.host

import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.util.Log

/**
 * What lets a live room be heard while another app is in front, or the
 * screen is locked: a foreground service of type microphone. It runs for
 * exactly as long as MorningstarLive has a reason for it, and there are
 * two: this device's screen is shared in the room
 * (docs/architecture/0039-live-video-screen-and-data.md), or the room was
 * joined with a notice to go on in the background under
 * (docs/architecture/0042-live-rooms-in-the-background-and-audio-routes.md).
 * It does nothing itself; it is the system's record, with a notification
 * the user can see and end the room from, that the app is using the
 * microphone while it is not on screen. Sharing the screen has a service
 * of its own besides, LiveKit's, of type mediaProjection; the two show one
 * notification.
 *
 * Declared in the manifest only of an app that declares `live-screen` or
 * `live-background`, and started only by MorningstarLive, while the app is
 * in front.
 */
class MorningstarLiveService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == MorningstarLive.ACTION_HANG_UP) {
            // The notification's action. Ending the room stops this service.
            MorningstarLive.hangUpFromNotification()
            if (!MorningstarLive.wantsService()) {
                stopSelf()
            }
            return START_NOT_STICKY
        }
        if (!MorningstarLive.wantsService()) {
            // Started for a room that is already over.
            stopSelf()
            return START_NOT_STICKY
        }
        try {
            val notification = MorningstarLive.notification(this)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                startForeground(
                    MorningstarLive.NOTIFICATION_ID,
                    notification,
                    ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE,
                )
            } else {
                startForeground(MorningstarLive.NOTIFICATION_ID, notification)
            }
            Log.i("Morningstar", "live: the microphone's foreground service is running")
        } catch (failure: Exception) {
            // Without it the microphone falls silent once another app is
            // in front: a room that was to go on in the background does
            // not (MorningstarLive ends it with the screen instead).
            Log.w("Morningstar", "live: the microphone's foreground service did not start (${failure.javaClass.simpleName})")
            MorningstarLive.serviceFailed()
            stopSelf()
        }
        return START_NOT_STICKY
    }

    /**
     * The app was swiped away from the recent apps. The room is not kept by
     * a notification alone: it is left, and this service goes with it.
     */
    override fun onTaskRemoved(rootIntent: Intent?) {
        MorningstarLive.taskRemoved()
        stopSelf()
        super.onTaskRemoved(rootIntent)
    }

    override fun onDestroy() {
        Log.i("Morningstar", "live: the microphone's foreground service stopped")
        super.onDestroy()
    }
}
