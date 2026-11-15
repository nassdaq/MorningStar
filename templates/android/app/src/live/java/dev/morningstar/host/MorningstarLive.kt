package dev.morningstar.host

import android.Manifest
import android.app.Activity
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.media.AudioManager
import android.media.projection.MediaProjectionManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.util.Log
import android.view.WindowManager
import com.twilio.audioswitch.AudioDevice
import io.livekit.android.AudioOptions
import io.livekit.android.LiveKit
import io.livekit.android.LiveKitOverrides
import io.livekit.android.audio.AudioSwitchHandler
import io.livekit.android.events.DisconnectReason
import io.livekit.android.events.RoomEvent
import io.livekit.android.events.collect
import io.livekit.android.room.Room
import io.livekit.android.room.participant.Participant
import io.livekit.android.room.track.CameraPosition
import io.livekit.android.room.track.DataPublishReliability
import io.livekit.android.room.track.LocalVideoTrack
import io.livekit.android.room.track.RemoteTrackPublication
import io.livekit.android.room.track.Track
import io.livekit.android.room.track.VideoTrack
import io.livekit.android.room.track.screencapture.ScreenCaptureParams
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import livekit.org.webrtc.VideoFrame
import livekit.org.webrtc.VideoSink

/**
 * The live room (docs/architecture/0038-live-rooms.md) in an app that
 * declares `services = ["live"]`: a conversation on a LiveKit server,
 * joined with the address and the ticket the application was given.
 *
 * Besides sound a room carries data, and in an app that also declares
 * `live-video` pictures (the others' cameras and shared screens, and this
 * device's camera), and in one that declares `live-screen` this device's
 * screen (docs/architecture/0039-live-video-screen-and-data.md). Which of
 * the two an app declared is read from its own manifest, where the build
 * put or did not put the permissions they need.
 *
 * This is the only Kotlin in the host (with MorningstarLiveService beside
 * it), and it is Kotlin because LiveKit's
 * client is: connecting, publishing and muting are suspending functions and
 * its events are a flow, which Java can call only through hand-written
 * continuations. Its twin in src/nolive/java, compiled into every app that
 * does not declare the service, has the same name and the same static
 * methods and refuses every join.
 *
 * Threads. Rust asks from its render thread ([command]), which only posts.
 * Everything else here runs on the main thread: the state below is touched
 * nowhere else, the coroutines are confined to it, and LiveKit does its own
 * work (the socket, WebRTC, the audio device) on threads of its own, so
 * nothing here blocks either the main thread or Rust's. The exceptions:
 * the microphone question, which belongs to MorningstarServices and its
 * service thread, and whose answer is posted back; and a video's frames,
 * which a [Picture] hands to Rust on the WebRTC thread that decoded them.
 *
 * What is never logged: the ticket, the address, who is in the room, and
 * what is said or sent in it.
 * A failure's text is the SDK's, with the ticket cut out of it ([describe]).
 */
internal object MorningstarLive {
    private const val TAG = "Morningstar"

    /** How long after a change the audio device is looked at for the log. */
    private const val AUDIO_SETTLE_MS = 1000L

    /** requestPermissions request code for the camera. */
    private const val REQUEST_CAMERA = 0x4d54

    /** startActivityForResult request code for "share your screen?". */
    private const val REQUEST_SCREEN = 0x4d55

    /** The notification shown while the screen is shared, and its channel. */
    internal const val NOTIFICATION_ID = 0x4d56
    private const val NOTIFICATION_CHANNEL = "morningstar_live"

    private const val SOURCE_CAMERA = 0
    private const val SOURCE_SCREEN = 1

    private val main = Handler(Looper.getMainLooper())
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    /** One join, from the command to the event that ends it. */
    private class Session(
        val id: Long,
        var microphone: Boolean,
        var speaker: Boolean,
        val reportSpeaking: Boolean,
        /** Whether the application wants this device's camera on. */
        var camera: Boolean,
    ) {
        var room: Room? = null
        var job: Job? = null

        /** LIVE_CONNECTED has been reported. */
        var connected = false

        /** The final event has been reported: nothing more may be. */
        var ended = false

        /** Who has been reported as in the room, so that nobody is twice. */
        val present = HashSet<String>()

        /** The videos that have been reported as started, by LiveKit's track. */
        val pictures = HashMap<VideoTrack, Picture>()

        /** This device's screen is being shared, under this track number. */
        var screen: Int? = null

        /** The camera was turned off because the app left the screen. */
        var cameraHidden = false
    }

    /**
     * One video that is being shown: the sink WebRTC hands its decoded
     * frames to, on a thread of its own, and what Rust knows it as. A
     * frame is copied by Rust inside [onFrame] and released after; nothing
     * is queued here or there.
     */
    private class Picture(
        val number: Int,
        val identity: String,
        val source: Int,
        val local: Boolean,
        @Volatile var mirrored: Boolean,
    ) : VideoSink {
        @Volatile var closed = false
        private var seen = false

        override fun onFrame(frame: VideoFrame) {
            if (closed) {
                return
            }
            val planes = frame.buffer.toI420() ?: return
            try {
                if (!seen) {
                    seen = true
                    Log.i(
                        TAG,
                        "live: video $number first frame ${planes.width}x${planes.height}" +
                            " rotation ${frame.rotation}",
                    )
                }
                MorningstarNative.nativeVideoFrame(
                    number,
                    planes.width,
                    planes.height,
                    frame.rotation,
                    mirrored,
                    planes.dataY,
                    planes.strideY,
                    planes.dataU,
                    planes.strideU,
                    planes.dataV,
                    planes.strideV,
                )
            } finally {
                planes.release()
            }
        }

        /** LIVE_VIDEO_STARTED's and LIVE_VIDEO_STOPPED's code. */
        fun code(): Int = number * 4 + (if (local) 2 else 0) + source
    }

    /** Track numbers are never used twice while the process lives. */
    private var nextPicture = 1

    /** The session whose "share your screen?" is on screen. */
    private var askingForScreen: Session? = null

    /** What to do with the answer to the camera question that is on screen. */
    private var cameraAnswer: ((Boolean) -> Unit)? = null

    /** The room that is joined or being joined: one at a time. */
    private var session: Session? = null

    /** Whether an activity is on screen. */
    private var onScreen = false

    // ------------------------------------------------------------- commands

    /** MorningstarNative.onServiceCommand, for the SERVICE_LIVE_* commands. Render thread. */
    @JvmStatic
    fun command(command: Int, id: Long, first: String?, second: String?, data: ByteArray?) {
        val flag = { index: Int -> data != null && data.size > index && data[index].toInt() != 0 }
        main.post {
            when (command) {
                MorningstarNative.SERVICE_LIVE_JOIN ->
                    join(
                        Session(id, flag(0), flag(1), flag(2), flag(3)),
                        first.orEmpty(),
                        second.orEmpty(),
                    )
                MorningstarNative.SERVICE_LIVE_LEAVE -> {
                    Log.i(TAG, "live: leave" + if (session == null) " (no room is joined)" else "")
                    session?.let { end(it, MorningstarNative.LIVE_DISCONNECTED, MorningstarNative.LIVE_END_LEFT) }
                }
                MorningstarNative.SERVICE_LIVE_MICROPHONE -> setMicrophone(flag(0))
                MorningstarNative.SERVICE_LIVE_SPEAKER -> setSpeaker(flag(0))
                MorningstarNative.SERVICE_LIVE_DATA ->
                    sendData(first.orEmpty(), second == "reliable", data ?: ByteArray(0))
                MorningstarNative.SERVICE_LIVE_CAMERA -> setCamera(flag(0))
                MorningstarNative.SERVICE_LIVE_SWITCH_CAMERA -> switchCamera()
                MorningstarNative.SERVICE_LIVE_SCREEN_SHARE -> setScreenShare(flag(0))
            }
        }
    }

    /** The activity came on screen. Main thread. */
    @JvmStatic
    fun activityStarted() {
        onScreen = true
        // A room that went on behind another app because the screen was
        // shared gets its camera back.
        session?.let { current ->
            if (current.cameraHidden) {
                current.cameraHidden = false
                if (current.camera && current.connected) {
                    startCamera(current)
                }
            }
        }
    }

    /**
     * The activity left the screen. The room is left with it: without a
     * foreground service Android would silence the microphone of a
     * background app sooner or later and may freeze the process, and until
     * then the user would be heard from an app they cannot see.
     *
     * The exception is a room in which this device's screen is shared
     * (ADR 0039): showing another app is what sharing is for. That room
     * goes on, held by two foreground services whose notification says so
     * (LiveKit's, for the screen, and MorningstarLiveService, for the
     * microphone), with the microphone as it was and the camera off until
     * the app is back. While "share your screen?" is on screen nothing is
     * decided: on some releases the question covers the app completely.
     * Main thread.
     */
    @JvmStatic
    fun activityStopped() {
        onScreen = false
        val current = session ?: return
        when {
            current.screen != null -> {
                Log.i(TAG, "live: id=${current.id} goes on behind another app: the screen is shared")
                if (current.camera && current.connected) {
                    current.cameraHidden = true
                    stopCamera(current)
                }
            }
            askingForScreen === current ->
                Log.i(TAG, "live: id=${current.id} kept while the screen question is answered")
            else -> {
                Log.i(TAG, "live: id=${current.id} left because the app left the screen")
                end(current, MorningstarNative.LIVE_DISCONNECTED, MorningstarNative.LIVE_END_HIDDEN)
            }
        }
    }

    /** Activity.onRequestPermissionsResult. Main thread. */
    @JvmStatic
    fun permissionResult(requestCode: Int, grantResults: IntArray) {
        if (requestCode != REQUEST_CAMERA) {
            return
        }
        val granted = grantResults.isNotEmpty() && grantResults[0] == PackageManager.PERMISSION_GRANTED
        cameraAnswer?.let { answer ->
            cameraAnswer = null
            answer(granted)
        }
    }

    /** Activity.onActivityResult: the answer to "share your screen?". Main thread. */
    @JvmStatic
    fun activityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        if (requestCode != REQUEST_SCREEN) {
            return
        }
        val current = askingForScreen ?: return
        askingForScreen = null
        if (current.ended) {
            return
        }
        if (resultCode != Activity.RESULT_OK || data == null) {
            Log.i(TAG, "live: id=${current.id} screen not shared: the user declined")
            mediaFailed(current, SOURCE_SCREEN, MorningstarNative.LIVE_ERROR_PERMISSION_DENIED)
            // Where the question covered the app, this answer arrives
            // before the app is back on screen: look a moment later.
            main.postDelayed({ leaveIfHidden(current) }, AUDIO_SETTLE_MS)
            return
        }
        startScreenShare(current, data)
    }

    // ----------------------------------------------------------------- join

    private fun join(joining: Session, url: String, token: String) {
        // One room at a time: the one that is joined ends as replaced.
        session?.let { end(it, MorningstarNative.LIVE_DISCONNECTED, MorningstarNative.LIVE_END_REPLACED) }
        Log.i(
            TAG,
            "live: join id=${joining.id} microphone=${if (joining.microphone) "live" else "muted"}" +
                " sound=${if (joining.speaker) "loudspeaker" else "earpiece"}",
        )
        session = joining
        val context = MorningstarActivity.current()?.applicationContext
        if (context == null || !onScreen) {
            // A join that arrives while the app is in the background (the
            // answer to a request made before it left) joins nothing.
            Log.i(TAG, "live: id=${joining.id} not joined: the app is not on screen")
            end(joining, MorningstarNative.LIVE_DISCONNECTED, MorningstarNative.LIVE_END_HIDDEN)
            return
        }
        if (token.isEmpty() || !(url.startsWith("wss://") || url.startsWith("ws://"))) {
            fail(joining, "the address is not a wss:// address, or the ticket is empty")
            return
        }
        // A room is a conversation: the microphone is asked for before
        // joining, also when joining muted, so that unmuting never has to
        // stop for a question. The question is MorningstarServices's, the
        // one recording asks.
        val asked = MorningstarServices.post {
            MorningstarServices.withMicrophone { granted, failure ->
                main.post {
                    when {
                        joining.ended -> Unit
                        granted -> connect(joining, context, url, token)
                        failure != null -> fail(joining, failure)
                        else -> {
                            Log.i(TAG, "live: id=${joining.id} not joined: microphone permission denied")
                            end(
                                joining,
                                MorningstarNative.LIVE_FAILED,
                                MorningstarNative.LIVE_ERROR_PERMISSION_DENIED,
                            )
                        }
                    }
                }
            }
        }
        if (!asked) {
            fail(joining, "the platform services were never started")
        }
    }

    private fun connect(joining: Session, context: Context, url: String, token: String) {
        if (!onScreen) {
            end(joining, MorningstarNative.LIVE_DISCONNECTED, MorningstarNative.LIVE_END_HIDDEN)
            return
        }
        // The audio of a call: LiveKit's AudioSwitchHandler puts the device
        // into communication mode and takes the audio focus when the room
        // connects, routes the sound, and puts back what it found when the
        // room is released. Which output it prefers is decided here: a
        // headset when there is one, then the loudspeaker or the earpiece
        // as the application asked.
        val audio = AudioSwitchHandler(context)
        audio.preferredDeviceList = preferredDevices(joining.speaker)
        val room: Room
        try {
            room = LiveKit.create(
                context,
                overrides = LiveKitOverrides(audioOptions = AudioOptions(audioHandler = audio)),
            )
        } catch (failure: Exception) {
            fail(joining, describe(failure, token))
            return
        }
        joining.room = room
        keepScreenOn(true)
        joining.job = scope.launch {
            // Listening starts before connecting, so that nothing that
            // happens while the room connects is missed.
            launch { room.events.collect { event -> onRoomEvent(joining, event) } }
            try {
                room.connect(url, token)
                if (joining.ended) {
                    return@launch
                }
                if (joining.microphone) {
                    room.localParticipant.setMicrophoneEnabled(true)
                }
                if (joining.ended) {
                    return@launch
                }
                // Before LIVE_CONNECTED, so that data sent to the room as
                // people join is not missed; a video that is already there
                // is reported after it, below.
                joining.connected = true
                Log.i(
                    TAG,
                    "live: id=${joining.id} connected, microphone " +
                        (if (joining.microphone) "published" else "muted") +
                        ", ${room.remoteParticipants.size} other(s) in the room",
                )
                emit(joining, MorningstarNative.LIVE_CONNECTED)
                // The audio device is switched on a thread of the SDK's:
                // what it settled on is said a moment later.
                main.postDelayed({
                    if (!joining.ended) {
                        Log.i(
                            TAG,
                            "live: id=${joining.id} audio mode ${audioMode(context)}, sound on " +
                                outputOf(audio),
                        )
                    }
                }, AUDIO_SETTLE_MS)
                for (participant in room.remoteParticipants.values) {
                    arrived(joining, participant)
                    // Videos subscribed while the room was connecting.
                    for (publication in participant.videoTrackPublications) {
                        val track = publication.second as? VideoTrack ?: continue
                        showRemote(joining, track, publication.first as? RemoteTrackPublication, participant)
                    }
                }
                if (joining.camera) {
                    startCamera(joining)
                }
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (failure: Exception) {
                fail(joining, describe(failure, token))
            }
        }
    }

    private fun onRoomEvent(current: Session, event: RoomEvent) {
        if (current.ended) {
            return
        }
        when (event) {
            is RoomEvent.ParticipantConnected ->
                // Before LIVE_CONNECTED the room's own list is reported
                // instead, in one go.
                if (current.connected) {
                    arrived(current, event.participant)
                }
            is RoomEvent.ParticipantDisconnected -> {
                val identity = identityOf(event.participant)
                if (current.present.remove(identity)) {
                    Log.i(TAG, "live: id=${current.id} someone left (${current.present.size} other(s) now)")
                    emit(current, MorningstarNative.LIVE_PARTICIPANT_LEFT, first = identity)
                }
            }
            is RoomEvent.TrackSubscribed -> {
                val track = event.track as? VideoTrack
                if (track != null && current.connected) {
                    showRemote(current, track, event.publication as? RemoteTrackPublication, event.participant)
                }
            }
            is RoomEvent.TrackUnsubscribed -> (event.track as? VideoTrack)?.let { hide(current, it) }
            // A camera turned off is a muted track: its picture is over
            // until it is turned on again.
            is RoomEvent.TrackMuted -> (event.publication.track as? VideoTrack)?.let {
                if (event.participant !== current.room?.localParticipant) {
                    hide(current, it)
                }
            }
            is RoomEvent.TrackUnmuted -> {
                val track = event.publication.track as? VideoTrack
                if (track != null && current.connected && event.participant !== current.room?.localParticipant) {
                    showRemote(current, track, event.publication as? RemoteTrackPublication, event.participant)
                }
            }
            is RoomEvent.DataReceived ->
                if (current.connected) {
                    // Never logged beyond its size: it is what people say.
                    Log.i(TAG, "live: id=${current.id} data received, ${event.data.size} bytes")
                    MorningstarNative.nativeLiveData(
                        current.id,
                        event.participant?.let { identityOf(it) }.orEmpty(),
                        event.topic.orEmpty(),
                        event.data,
                    )
                }
            is RoomEvent.ActiveSpeakersChanged ->
                if (current.reportSpeaking && current.connected) {
                    emit(
                        current,
                        MorningstarNative.LIVE_SPEAKING,
                        first = event.speakers.joinToString("\n") { identityOf(it) },
                    )
                }
            is RoomEvent.Reconnecting -> {
                Log.i(TAG, "live: id=${current.id} reconnecting")
                emit(current, MorningstarNative.LIVE_RECONNECTING)
            }
            is RoomEvent.Reconnected -> {
                Log.i(TAG, "live: id=${current.id} reconnected")
                emit(current, MorningstarNative.LIVE_RECONNECTED)
            }
            is RoomEvent.Disconnected -> {
                val reason = when (event.reason) {
                    // Ended from the other side, on purpose.
                    DisconnectReason.DUPLICATE_IDENTITY,
                    DisconnectReason.SERVER_SHUTDOWN,
                    DisconnectReason.PARTICIPANT_REMOVED,
                    DisconnectReason.ROOM_DELETED,
                    DisconnectReason.ROOM_CLOSED,
                    -> MorningstarNative.LIVE_END_ENDED
                    // Everything else is a connection that went away.
                    else -> MorningstarNative.LIVE_END_LOST
                }
                Log.i(TAG, "live: id=${current.id} disconnected by the room (${event.reason})")
                end(current, MorningstarNative.LIVE_DISCONNECTED, reason)
            }
            else -> Unit
        }
    }

    private fun arrived(current: Session, participant: Participant) {
        val identity = identityOf(participant)
        if (current.present.add(identity)) {
            Log.i(TAG, "live: id=${current.id} someone is in the room (${current.present.size} other(s) now)")
            emit(
                current,
                MorningstarNative.LIVE_PARTICIPANT_JOINED,
                first = identity,
                second = participant.name.orEmpty(),
            )
        }
    }

    /** An identity as one line: it crosses to Rust in a list of lines. */
    private fun identityOf(participant: Participant): String =
        participant.identity?.value.orEmpty().replace('\n', ' ').replace('\r', ' ')

    // ---------------------------------------------------------------- data

    private fun sendData(topic: String, reliable: Boolean, payload: ByteArray) {
        val current = session ?: return
        val room = current.room
        if (room == null || !current.connected) {
            Log.i(TAG, "live: id=${current.id} data not sent: the room is not connected")
            return
        }
        scope.launch {
            try {
                val sent = room.localParticipant.publishData(
                    payload,
                    if (reliable) DataPublishReliability.RELIABLE else DataPublishReliability.LOSSY,
                    topic,
                )
                Log.i(
                    TAG,
                    "live: id=${current.id} data ${payload.size} bytes " +
                        if (sent.isSuccess) "sent" else "not sent (${sent.exceptionOrNull()?.javaClass?.simpleName})",
                )
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (failure: Exception) {
                Log.w(TAG, "live: id=${current.id} data not sent (${failure.javaClass.simpleName})")
            }
        }
    }

    // ------------------------------------------------------------- pictures

    /** Whether the app declared `live-video`: its manifest asks for the camera. */
    private fun videoBuilt(context: Context): Boolean = declares(context, Manifest.permission.CAMERA)

    /** Whether the app declared `live-screen`. */
    private fun screenBuilt(context: Context): Boolean =
        declares(context, "android.permission.FOREGROUND_SERVICE_MEDIA_PROJECTION")

    private fun declares(context: Context, permission: String): Boolean =
        try {
            @Suppress("DEPRECATION")
            context.packageManager
                .getPackageInfo(context.packageName, PackageManager.GET_PERMISSIONS)
                .requestedPermissions
                ?.contains(permission) == true
        } catch (failure: Exception) {
            false
        }

    /**
     * Someone else's camera or shared screen is there to be shown. In an
     * app without `live-video` it is not: the track is given up, so that
     * nothing is received and decoded for pictures nobody can see.
     */
    private fun showRemote(
        current: Session,
        track: VideoTrack,
        publication: RemoteTrackPublication?,
        participant: Participant,
    ) {
        val context = MorningstarActivity.current()?.applicationContext
        if (context == null || !videoBuilt(context)) {
            Log.i(TAG, "live: id=${current.id} a video is not shown: the app has no live-video")
            publication?.setSubscribed(false)
            return
        }
        val screen = publication?.source == Track.Source.SCREEN_SHARE
        show(current, track, identityOf(participant), if (screen) SOURCE_SCREEN else SOURCE_CAMERA, false, false)
    }

    private fun show(
        current: Session,
        track: VideoTrack,
        identity: String,
        source: Int,
        local: Boolean,
        mirrored: Boolean,
    ) {
        if (current.ended || current.pictures.containsKey(track)) {
            return
        }
        val picture = Picture(nextPicture++, identity, source, local, mirrored)
        current.pictures[track] = picture
        track.addRenderer(picture)
        Log.i(
            TAG,
            "live: id=${current.id} video ${picture.number} started (" +
                (if (local) "own " else "") + (if (source == SOURCE_SCREEN) "screen" else "camera") + ")",
        )
        emit(current, MorningstarNative.LIVE_VIDEO_STARTED, first = identity, code = picture.code())
    }

    private fun hide(current: Session, track: VideoTrack) {
        val picture = current.pictures.remove(track) ?: return
        picture.closed = true
        try {
            track.removeRenderer(picture)
        } catch (failure: Exception) {
            // The track is already gone with its room.
        }
        MorningstarNative.nativeVideoEnded(picture.number)
        Log.i(TAG, "live: id=${current.id} video ${picture.number} stopped")
        emit(current, MorningstarNative.LIVE_VIDEO_STOPPED, first = picture.identity, code = picture.code())
    }

    private fun mediaFailed(current: Session, source: Int, error: Int, reason: String? = null) {
        emit(current, MorningstarNative.LIVE_MEDIA_FAILED, first = reason, code = error * 2 + source)
    }

    // ----------------------------------------------------------- the camera

    private fun setCamera(on: Boolean) {
        val current = session ?: return
        current.camera = on
        current.cameraHidden = false
        Log.i(TAG, "live: id=${current.id} camera wanted ${if (on) "on" else "off"}")
        if (!current.connected) {
            // Not connected yet: connecting starts it, or does not.
            return
        }
        if (on) startCamera(current) else stopCamera(current)
    }

    private fun startCamera(current: Session) {
        val activity = MorningstarActivity.current()
        val room = current.room
        if (activity == null || room == null) {
            return
        }
        if (!videoBuilt(activity)) {
            Log.w(
                TAG,
                "live: id=${current.id} camera refused: the app does not declare live-video" +
                    " (services = [\"live\", \"live-video\"] in its Cargo.toml)",
            )
            current.camera = false
            mediaFailed(current, SOURCE_CAMERA, MorningstarNative.LIVE_ERROR_UNSUPPORTED)
            return
        }
        val publish: (Boolean) -> Unit = { granted ->
            if (!current.ended && current.camera) {
                if (granted) {
                    publishCamera(current, room)
                } else {
                    Log.i(TAG, "live: id=${current.id} camera not started: permission denied")
                    current.camera = false
                    mediaFailed(current, SOURCE_CAMERA, MorningstarNative.LIVE_ERROR_PERMISSION_DENIED)
                }
            }
        }
        if (activity.checkSelfPermission(Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED) {
            publish(true)
        } else if (cameraAnswer != null) {
            // The question is on screen already: this answer replaces the
            // one that was waiting for it.
            cameraAnswer = publish
        } else {
            cameraAnswer = publish
            activity.requestPermissions(arrayOf(Manifest.permission.CAMERA), REQUEST_CAMERA)
        }
    }

    private fun publishCamera(current: Session, room: Room) {
        scope.launch {
            try {
                room.localParticipant.setCameraEnabled(true)
                val track = room.localParticipant
                    .getTrackPublication(Track.Source.CAMERA)?.track as? LocalVideoTrack
                if (track != null && !current.ended && current.camera) {
                    Log.i(TAG, "live: id=${current.id} camera published")
                    show(
                        current,
                        track,
                        identityOf(room.localParticipant),
                        SOURCE_CAMERA,
                        true,
                        track.options.position != CameraPosition.BACK,
                    )
                }
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (failure: Exception) {
                Log.w(TAG, "live: id=${current.id} camera not started (${failure.javaClass.simpleName})")
                current.camera = false
                mediaFailed(
                    current,
                    SOURCE_CAMERA,
                    MorningstarNative.LIVE_ERROR_FAILED,
                    describe(failure, ""),
                )
            }
        }
    }

    private fun stopCamera(current: Session) {
        val room = current.room ?: return
        val track = room.localParticipant.getTrackPublication(Track.Source.CAMERA)?.track as? VideoTrack
        track?.let { hide(current, it) }
        scope.launch {
            try {
                room.localParticipant.setCameraEnabled(false)
                Log.i(TAG, "live: id=${current.id} camera off")
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (failure: Exception) {
                // A camera that could not be turned off must not stay on
                // behind a button that says it is off: the room ends.
                fail(current, "the camera could not be turned off")
            }
        }
    }

    private fun switchCamera() {
        val current = session ?: return
        val track = current.room?.localParticipant
            ?.getTrackPublication(Track.Source.CAMERA)?.track as? LocalVideoTrack ?: return
        val picture = current.pictures[track] ?: return
        try {
            track.switchCamera()
        } catch (failure: Exception) {
            Log.w(TAG, "live: id=${current.id} camera not switched (${failure.javaClass.simpleName})")
            return
        }
        // Which camera it is now is known once the switch has happened.
        main.postDelayed({
            if (!picture.closed) {
                picture.mirrored = track.options.position != CameraPosition.BACK
                Log.i(
                    TAG,
                    "live: id=${current.id} camera is now the " +
                        if (picture.mirrored) "front one" else "back one",
                )
            }
        }, AUDIO_SETTLE_MS)
    }

    // ----------------------------------------------------------- the screen

    private fun setScreenShare(on: Boolean) {
        val current = session ?: return
        if (!on) {
            stopScreenShare(current)
            return
        }
        val activity = MorningstarActivity.current()
        if (activity == null || !current.connected || current.screen != null || askingForScreen != null) {
            if (current.screen == null && askingForScreen == null) {
                mediaFailed(
                    current,
                    SOURCE_SCREEN,
                    MorningstarNative.LIVE_ERROR_FAILED,
                    "the room is not connected",
                )
            }
            return
        }
        if (!screenBuilt(activity)) {
            Log.w(
                TAG,
                "live: id=${current.id} screen sharing refused: the app does not declare live-screen" +
                    " (services = [\"live\", \"live-screen\"] in its Cargo.toml)",
            )
            mediaFailed(current, SOURCE_SCREEN, MorningstarNative.LIVE_ERROR_UNSUPPORTED)
            return
        }
        val projection = activity.getSystemService(Context.MEDIA_PROJECTION_SERVICE) as? MediaProjectionManager
        if (projection == null) {
            mediaFailed(current, SOURCE_SCREEN, MorningstarNative.LIVE_ERROR_UNSUPPORTED)
            return
        }
        Log.i(TAG, "live: id=${current.id} asking the user to share the screen")
        askingForScreen = current
        try {
            @Suppress("DEPRECATION")
            activity.startActivityForResult(projection.createScreenCaptureIntent(), REQUEST_SCREEN)
        } catch (failure: Exception) {
            askingForScreen = null
            mediaFailed(current, SOURCE_SCREEN, MorningstarNative.LIVE_ERROR_FAILED, describe(failure, ""))
        }
    }

    /** The user said yes: [permission] is what the system gave for it. */
    private fun startScreenShare(current: Session, permission: Intent) {
        val room = current.room
        val context = MorningstarActivity.current()?.applicationContext
        if (room == null || context == null) {
            return
        }
        // The microphone's service first, while the app is in front: it is
        // what lets the room be heard once another app is.
        val notification = notification(context)
        try {
            val service = Intent(context, MorningstarLiveService::class.java)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                context.startForegroundService(service)
            } else {
                context.startService(service)
            }
        } catch (failure: Exception) {
            Log.w(TAG, "live: id=${current.id} the microphone's service did not start (${failure.javaClass.simpleName})")
        }
        scope.launch {
            try {
                room.localParticipant.setScreenShareEnabled(
                    true,
                    ScreenCaptureParams(
                        permission,
                        NOTIFICATION_ID,
                        notification,
                        // The system's own "stop sharing".
                        { main.post { stopScreenShare(current) } },
                    ),
                )
                if (current.ended) {
                    return@launch
                }
                // Reported with a track number, but no picture follows: the
                // screen is in front of its user already.
                val number = nextPicture++
                current.screen = number
                Log.i(TAG, "live: id=${current.id} screen shared, video $number")
                emit(
                    current,
                    MorningstarNative.LIVE_VIDEO_STARTED,
                    first = identityOf(room.localParticipant),
                    code = number * 4 + 2 + SOURCE_SCREEN,
                )
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (failure: Exception) {
                Log.w(TAG, "live: id=${current.id} screen not shared (${failure.javaClass.simpleName})")
                stopService(context)
                mediaFailed(current, SOURCE_SCREEN, MorningstarNative.LIVE_ERROR_FAILED, describe(failure, ""))
                main.postDelayed({ leaveIfHidden(current) }, AUDIO_SETTLE_MS)
            }
        }
    }

    private fun stopScreenShare(current: Session) {
        val number = current.screen ?: return
        current.screen = null
        val room = current.room
        Log.i(TAG, "live: id=${current.id} screen no longer shared")
        if (room != null && !current.ended) {
            emit(
                current,
                MorningstarNative.LIVE_VIDEO_STOPPED,
                first = identityOf(room.localParticipant),
                code = number * 4 + 2 + SOURCE_SCREEN,
            )
            scope.launch {
                try {
                    room.localParticipant.setScreenShareEnabled(false)
                } catch (cancelled: CancellationException) {
                    throw cancelled
                } catch (failure: Exception) {
                    // A screen that could not be un-shared must not stay
                    // shared behind a button that says it is not.
                    fail(current, "the screen could not be un-shared")
                }
            }
        }
        MorningstarActivity.current()?.applicationContext?.let { stopService(it) }
        leaveIfHidden(current)
    }

    /**
     * The rule a shared screen suspended applies again: a room whose app
     * is not on screen, and which no longer shares the screen, is left.
     */
    private fun leaveIfHidden(current: Session) {
        if (!onScreen && !current.ended && current.screen == null) {
            Log.i(TAG, "live: id=${current.id} left: the app is not on screen and shares nothing")
            end(current, MorningstarNative.LIVE_DISCONNECTED, MorningstarNative.LIVE_END_HIDDEN)
        }
    }

    private fun stopService(context: Context) {
        try {
            context.stopService(Intent(context, MorningstarLiveService::class.java))
        } catch (failure: Exception) {
            // It was not running.
        }
    }

    /**
     * What the system shows while the screen is shared, for both foreground
     * services. It says what is happening in the app's own name and opens
     * the app when touched.
     */
    internal fun notification(context: Context): Notification {
        val label = context.applicationInfo.loadLabel(context.packageManager)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val manager = context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
            manager.createNotificationChannel(
                NotificationChannel(NOTIFICATION_CHANNEL, "Calls", NotificationManager.IMPORTANCE_LOW),
            )
        }
        val open = PendingIntent.getActivity(
            context,
            0,
            Intent(context, MorningstarActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        @Suppress("DEPRECATION")
        val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            Notification.Builder(context, NOTIFICATION_CHANNEL)
        } else {
            Notification.Builder(context)
        }
        return builder
            .setContentTitle(label)
            .setContentText("Sharing your screen. The call's microphone stays as you left it.")
            .setSmallIcon(android.R.drawable.stat_sys_phone_call)
            .setContentIntent(open)
            .setOngoing(true)
            .build()
    }

    // ------------------------------------------- microphone and loudspeaker

    private fun setMicrophone(live: Boolean) {
        val current = session ?: return
        current.microphone = live
        Log.i(TAG, "live: id=${current.id} microphone ${if (live) "on" else "muted"}")
        val room = current.room
        if (room == null || !current.connected) {
            // Not connected yet: connecting publishes it, or does not.
            return
        }
        scope.launch {
            try {
                room.localParticipant.setMicrophoneEnabled(live)
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (failure: Exception) {
                // A microphone that could not be muted must not stay open
                // behind a button that says it is: the room ends.
                Log.w(TAG, "live: id=${current.id} the microphone could not be switched")
                fail(current, "the microphone could not be " + if (live) "opened" else "muted")
            }
        }
    }

    private fun setSpeaker(on: Boolean) {
        val current = session ?: return
        current.speaker = on
        val audio = current.room?.audioHandler as? AudioSwitchHandler
        if (audio == null) {
            // Not connected yet: connecting starts with this choice.
            return
        }
        audio.preferredDeviceList = preferredDevices(on)
        val wanted = if (on) AudioDevice.Speakerphone::class.java else AudioDevice.Earpiece::class.java
        val headset = audio.availableAudioDevices.any {
            it is AudioDevice.BluetoothHeadset || it is AudioDevice.WiredHeadset
        }
        val device = audio.availableAudioDevices.firstOrNull { wanted.isInstance(it) }
        when {
            // A headset keeps the sound: the choice applies once it is gone.
            headset -> Log.i(TAG, "live: id=${current.id} sound stays on the headset")
            device != null -> {
                audio.selectDevice(device)
                Log.i(TAG, "live: id=${current.id} sound to the ${if (on) "loudspeaker" else "earpiece"}")
            }
            // A tablet has no earpiece.
            else -> Log.i(TAG, "live: id=${current.id} this device has no ${if (on) "loudspeaker" else "earpiece"}")
        }
    }

    private fun preferredDevices(speaker: Boolean): List<Class<out AudioDevice>> =
        if (speaker) {
            listOf(
                AudioDevice.BluetoothHeadset::class.java,
                AudioDevice.WiredHeadset::class.java,
                AudioDevice.Speakerphone::class.java,
                AudioDevice.Earpiece::class.java,
            )
        } else {
            listOf(
                AudioDevice.BluetoothHeadset::class.java,
                AudioDevice.WiredHeadset::class.java,
                AudioDevice.Earpiece::class.java,
                AudioDevice.Speakerphone::class.java,
            )
        }

    // ------------------------------------------------------------- the end

    private fun fail(failing: Session, reason: String) {
        if (!failing.ended) {
            Log.w(TAG, "live: id=${failing.id} failed: $reason")
        }
        end(failing, MorningstarNative.LIVE_FAILED, MorningstarNative.LIVE_ERROR_FAILED, reason)
    }

    /**
     * Ends a session, once: stops listening, leaves and releases the room
     * (which closes the microphone and gives back the audio mode and the
     * audio focus), and only then reports the final event, so that "over"
     * never arrives while the microphone is still open.
     */
    private fun end(ending: Session, kind: Int, code: Int, message: String? = null) {
        if (ending.ended) {
            return
        }
        ending.ended = true
        if (session === ending) {
            session = null
            keepScreenOn(false)
        }
        if (askingForScreen === ending) {
            askingForScreen = null
        }
        // The pictures first: no frame is handed over after the end.
        for ((track, picture) in ending.pictures) {
            picture.closed = true
            try {
                track.removeRenderer(picture)
            } catch (failure: Exception) {
                // The track is gone already.
            }
            MorningstarNative.nativeVideoEnded(picture.number)
        }
        ending.pictures.clear()
        if (ending.screen != null) {
            ending.screen = null
            MorningstarActivity.current()?.applicationContext?.let { stopService(it) }
        }
        ending.job?.cancel()
        ending.room?.let { room ->
            try {
                room.disconnect()
                room.release()
            } catch (failure: Exception) {
                Log.w(TAG, "live: id=${ending.id} the room did not close cleanly (${failure.javaClass.simpleName})")
            }
            Log.i(TAG, "live: id=${ending.id} room released")
            // Giving the audio mode back happens on a thread of the SDK's;
            // this line, a moment later, is the proof that it was.
            MorningstarActivity.current()?.applicationContext?.let { context ->
                main.postDelayed({
                    if (session == null) {
                        Log.i(TAG, "live: no room is joined, audio mode ${audioMode(context)}")
                    }
                }, AUDIO_SETTLE_MS)
            }
        }
        ending.room = null
        MorningstarNative.nativeLiveEvent(ending.id, kind, code, message, null)
    }

    private fun emit(
        current: Session,
        kind: Int,
        first: String? = null,
        second: String? = null,
        code: Int = 0,
    ) {
        if (!current.ended) {
            MorningstarNative.nativeLiveEvent(current.id, kind, code, first, second)
        }
    }

    // -------------------------------------------------------------- helpers

    /**
     * While a room is joined the screen stays on (a room does not survive
     * the app leaving the screen, and a screen that times out is that) and
     * the volume keys set the call's volume.
     */
    private fun keepScreenOn(on: Boolean) {
        val activity = MorningstarActivity.current() ?: return
        if (on) {
            activity.window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
            activity.volumeControlStream = AudioManager.STREAM_VOICE_CALL
        } else {
            activity.window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
            activity.volumeControlStream = AudioManager.USE_DEFAULT_STREAM_TYPE
        }
    }

    /** Where the sound comes out, by kind (a headset's own name is its owner's). */
    private fun outputOf(audio: AudioSwitchHandler): String =
        when (audio.selectedAudioDevice) {
            is AudioDevice.Speakerphone -> "the loudspeaker"
            is AudioDevice.Earpiece -> "the earpiece"
            is AudioDevice.WiredHeadset -> "a wired headset"
            is AudioDevice.BluetoothHeadset -> "a Bluetooth headset"
            null -> "nothing yet"
        }

    /** The device's audio mode in words, for the log lines that prove it is set and restored. */
    private fun audioMode(context: Context): String {
        val manager = context.getSystemService(Context.AUDIO_SERVICE) as? AudioManager
        return when (manager?.mode) {
            AudioManager.MODE_NORMAL -> "normal"
            AudioManager.MODE_IN_COMMUNICATION -> "in communication"
            AudioManager.MODE_IN_CALL -> "in call"
            AudioManager.MODE_RINGTONE -> "ringtone"
            null -> "unknown"
            else -> manager.mode.toString()
        }
    }

    /**
     * What failed, for the application and the log, with the ticket cut
     * out: a connection error may quote the address it was connecting to,
     * and the ticket travels in that address.
     */
    private fun describe(failure: Throwable, token: String): String {
        var text = failure.javaClass.simpleName + ": " + (failure.message ?: "no detail")
        if (token.isNotEmpty()) {
            text = text.replace(token, "<ticket>")
        }
        text = text.replace(Regex("(access_token|token)=[^&\\s\"']+"), "$1=<ticket>")
        return if (text.length > 300) text.substring(0, 300) else text
    }
}
