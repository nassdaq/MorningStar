package dev.morningstar.host

import android.content.Context
import android.media.AudioManager
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
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch

/**
 * The live room (docs/architecture/0038-live-rooms.md) in an app that
 * declares `services = ["live"]`: a conversation on a LiveKit server,
 * joined with the address and the ticket the application was given.
 *
 * This is the only Kotlin in the host, and it is Kotlin because LiveKit's
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
 * nothing here blocks either the main thread or Rust's. The one exception
 * is the microphone question, which belongs to MorningstarServices and its
 * service thread; its answer is posted back.
 *
 * What is never logged: the ticket, the address, and who is in the room.
 * A failure's text is the SDK's, with the ticket cut out of it ([describe]).
 */
internal object MorningstarLive {
    private const val TAG = "Morningstar"

    /** How long after a change the audio device is looked at for the log. */
    private const val AUDIO_SETTLE_MS = 1000L

    private val main = Handler(Looper.getMainLooper())
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    /** One join, from the command to the event that ends it. */
    private class Session(
        val id: Long,
        var microphone: Boolean,
        var speaker: Boolean,
        val reportSpeaking: Boolean,
    ) {
        var room: Room? = null
        var job: Job? = null

        /** LIVE_CONNECTED has been reported. */
        var connected = false

        /** The final event has been reported: nothing more may be. */
        var ended = false

        /** Who has been reported as in the room, so that nobody is twice. */
        val present = HashSet<String>()
    }

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
                    join(Session(id, flag(0), flag(1), flag(2)), first.orEmpty(), second.orEmpty())
                MorningstarNative.SERVICE_LIVE_LEAVE -> {
                    Log.i(TAG, "live: leave" + if (session == null) " (no room is joined)" else "")
                    session?.let { end(it, MorningstarNative.LIVE_DISCONNECTED, MorningstarNative.LIVE_END_LEFT) }
                }
                MorningstarNative.SERVICE_LIVE_MICROPHONE -> setMicrophone(flag(0))
                MorningstarNative.SERVICE_LIVE_SPEAKER -> setSpeaker(flag(0))
            }
        }
    }

    /** The activity came on screen. Main thread. */
    @JvmStatic
    fun activityStarted() {
        onScreen = true
    }

    /**
     * The activity left the screen. The room is left with it: without a
     * foreground service Android would silence the microphone of a
     * background app sooner or later and may freeze the process, and until
     * then the user would be heard from an app they cannot see. Main
     * thread.
     */
    @JvmStatic
    fun activityStopped() {
        onScreen = false
        session?.let {
            Log.i(TAG, "live: id=${it.id} left because the app left the screen")
            end(it, MorningstarNative.LIVE_DISCONNECTED, MorningstarNative.LIVE_END_HIDDEN)
        }
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

    private fun emit(current: Session, kind: Int, first: String? = null, second: String? = null) {
        if (!current.ended) {
            MorningstarNative.nativeLiveEvent(current.id, kind, 0, first, second)
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
