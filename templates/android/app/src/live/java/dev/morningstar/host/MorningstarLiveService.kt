package dev.morningstar.host

import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.util.Log

/**
 * What lets a live room be heard while another app is in front
 * (docs/architecture/0039-live-video-screen-and-data.md): a foreground
 * service of type microphone, running for exactly as long as this device's
 * screen is shared and for no other reason. It does nothing itself; it is
 * the system's record, with a notification the user can see, that the app
 * is using the microphone while it is not on screen. Sharing the screen
 * has a service of its own, LiveKit's, of type mediaProjection; the two
 * show one notification.
 *
 * Declared in the manifest only of an app that declares `live-screen`, and
 * started only by MorningstarLive, after the user has agreed to share.
 */
class MorningstarLiveService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
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
            // Without it the room still shares the screen; the microphone
            // may fall silent once another app is in front.
            Log.w("Morningstar", "live: the microphone's foreground service did not start (${failure.javaClass.simpleName})")
            stopSelf()
        }
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        Log.i("Morningstar", "live: the microphone's foreground service stopped")
        super.onDestroy()
    }
}
