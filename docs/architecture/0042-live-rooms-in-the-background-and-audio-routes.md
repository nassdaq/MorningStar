# 0042 — Live rooms that go on in the background, and where their sound goes

Status: Accepted (2026-11-17). Verified on the development host only, and by building APKs and reading their manifests back. **Nothing here has run on a device**: no foreground service has been started, no notification shown, no headset connected. The application side is tested with the test driver, the Android host compiles (Kotlin and the merged manifest), and the iOS interface is kept in step without being compiled.

Extends [0038](0038-live-rooms.md) and [0039](0039-live-video-screen-and-data.md). It supersedes one paragraph of 0038, "A room does not continue in the background" (and the limitations "Not in the background" and "No Bluetooth headset" of both records): a room now may, on the conditions below. Everything else in them stands, above all the rule that nothing leaves a room connected by accident.

## Context

A call in Barua Talk ended the moment its user looked at another app or the screen locked, and came back when they returned. That is safe and it is not what a call is: people look things up while they talk. 0038 said what going on needs (a foreground service of type `microphone` and its notification) and left it for this record. 0039 built that service, but only for a room in which the screen is shared.

Separately, 0038 routed sound to the earpiece or the loudspeaker and to a wired headset when one was plugged in, took the Bluetooth permission out of the manifest, and gave the application no way to know or choose where the sound was.

## Decision

### 1. A room goes on in the background when three things are true

1. **The application asked**, per room: `LiveJoin::background(LiveBackground::new(title, text))`. The notice is what the user will read in the system's notification, and `hang_up` names the action that ends the room from there ("Hang up" unless changed). There is no way to ask for the background without a notice: the user must be able to see that the microphone is open and to close it without finding the app again.
2. **The app was built for it**: `services = ["live", "live-background"]`. Only then does its manifest hold the service, `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_MICROPHONE` and `POST_NOTIFICATIONS`. In any other app the notice is ignored and the room ends with the screen, as before, with a log line that says why.
3. **The service is running**: `MorningstarLiveService` (type `microphone`) is started when the room starts to connect, while the app is in front, because Android does not let a microphone service be started from the background. If the system refuses it, the room falls back to ending with the screen (at once, if the app is already away).

On Android 13 and later the user is asked once per need to allow notifications, before connecting (and before "share your screen?", for 0039's service). The room is joined whatever they answer: a foreground service runs, and is listed by the system, without its notification being shown.

### 2. Exactly when such a room ends

| What happens | The room | The application hears |
| --- | --- | --- |
| The application leaves it, or joins another | Ends | `Disconnected(Left)` / `Disconnected(Replaced)` |
| The user presses the notification's action | Ends at once, in the host, without waiting for the application | `Disconnected(HungUp)`, while hidden |
| The other side closes the room, or removes this person | Ends | `Disconnected(Ended)` |
| The connection cannot be kept | Ends | `Disconnected(Lost)` |
| `forget_pending`, or the engine goes | The host is told to leave, as in 0038 | nothing (it gave up listening) |
| The app is swiped away from the recent apps | Ends: there is no screen to come back to (`onTaskRemoved`; the service is declared `stopWithTask="false"` so that it is told) | `Disconnected(Hidden)`, if anything is left to hear it |
| The service cannot start or is refused | The room no longer goes on: it ends when the app leaves the screen, or now if it has | `Disconnected(Hidden)` |
| The app leaves the screen, the screen locks | **Goes on.** The microphone stays as it was; the camera is turned off and comes back with the app (Android gives a background app no camera) | nothing |

`HungUp` is a new ending rather than `Left` because the application did not do it and has more to do: Barua Talk hangs the call up on its server. It ends the room in the host first so that the microphone closes even if the application never runs again.

The application is not told that the app left the screen: nothing about the room changed. Its timers stop while hidden as every application's do (0018), so an application that learns from a poll that a call is over must ask when the room tells it something instead; Barua Talk asks its server the moment the last other participant leaves.

A room with a notice no longer holds the screen on (a screen that times out no longer ends it) unless a picture is showing. And with the sound in the earpiece and no picture, the screen turns off at the ear (a proximity wake lock; `WAKE_LOCK` is added for apps with `live`), for every room, with or without a notice.

### 3. Audio routes

- **Told:** `LiveEvent::AudioRoutes(LiveAudioRoutes { available, selected })` once the room is connected and whenever either changes, a headset connecting or leaving in the middle of a call included. Routes are `Earpiece`, `Speaker`, `Wired`, `Bluetooth`. That event is how an application asks which routes exist: it always has the latest.
- **Chosen:** `out.set_live_audio_route(route)`. Not answered; the next `AudioRoutes` says where the sound went. A route the device does not have now is ignored. `set_live_speaker(bool)` remains: it is the preference between loudspeaker and earpiece for when no headset is connected, and a headset that connects still takes the sound.
- **How:** LiveKit's own `AudioSwitchHandler` (the `audioswitch` library), which the host already used. Nothing second was written: its device-change listener is the event, and its `selectDevice` the command. Read from the 2.30.0 classes: from Android 12 it uses the platform's communication-device API (`CommDeviceAudioSwitch`), below that `AudioSwitch` (API 23 to 30) with Bluetooth SCO.
- **Bluetooth permissions.** The library never asks for `BLUETOOTH_CONNECT` and does not mention it: on Android 12 and later the communication-device API lists and selects a Bluetooth headset without it, so no question is asked of the user. Up to Android 11 it needs `BLUETOOTH`, granted at install, which its manifest declares with `maxSdkVersion="30"`; 0038 removed that line and this record stops removing it, for apps with `live` only (others never had it). `android.hardware.bluetooth` is declared not required, so that the permission does not hide the app from devices without it. **Whether a headset's microphone and sound in fact work on both sides of Android 12 is unverified.**

### The two halves

- Join (`SERVICE_LIVE_JOIN`): a fifth flag byte (goes on in the background) and after the five flags the notice as UTF-8, three lines (`LiveBackground::to_wire`). iOS: the notice as `text3`.
- `SERVICE_LIVE_AUDIO_ROUTE` (18 on Android, 19 on iOS): one byte, the route's number (1 to 4).
- `LIVE_AUDIO_ROUTES` (event kind 11): the code is the route in use (0 for none) plus 16, 32, 64, 128 for each of the four that is available. `LIVE_END_HUNG_UP` (5).
- The notification's action is an intent to `MorningstarLiveService` (`ACTION_HANG_UP`), which calls `MorningstarLive.hangUpFromNotification()`.
- iOS: the header, `lib.rs` and the Swift host name the new command, event and ending. Joins still answer `Unsupported`; an implementation would use the `audio` background mode, CallKit for the system's own call screen, and `AVAudioSession`'s route change notification. Not compiled.

### The test driver

`Ui::live_app_hidden()` does what such a host does when the app leaves the screen: a room joined with a notice, or one sharing the screen, goes on (returns true); any other ends as `Hidden`. `Ui::live_hang_up_from_notice()` is the notification's action. `LiveRoom::audio_route` is the route last asked for. A test of an app built without `live-background` delivers `Disconnected(Hidden)` itself, as before.

## Barua Talk

Declares `live-background`. Every real call (in-app, a room's, to a phone) is joined with a notice that names it ("Call with Baraka Mushi", "Call to +255…"). `HungUp` hangs the call up as the red button does and says "You ended the call from its notification." The last other participant leaving makes the app ask its server about the call at once. The phone call's Speaker button asks for the loudspeaker route, and off again for the best of what else there is (Bluetooth, then wired, then the earpiece); it follows `AudioRoutes`, so a headset that connects un-lights it, and the status line says "Sound is live on your Bluetooth headset."

## Consequences

- An app that opts in shows a notification for every call and declares three more permissions; a store listing shows them. That is why it is opt-in, and why the notice is per room.
- A room can now outlive the activity it was joined from. The host keeps the application context for that, and the render thread, which belongs to the process, keeps delivering events.
- `LiveJoin` grew (a notice is three strings); the UI's test driver is boxed for it.
- The notification is a plain ongoing one in category `call`, with a platform icon. It is not Android's `CallStyle` and not a full-screen incoming-call notification: ringing while the app is closed needs push messages, which are a service this framework does not have.

## Limitations

- **Not incoming calls.** A call can go on in the background; it cannot arrive there.
- The camera is off while the app is hidden.
- No choice between two Bluetooth headsets, or of the microphone separately from the output: a route is a kind.
- The notification's small icon is the platform's telephone, not the app's.
- A user who refuses notifications has a call that goes on with no notification to end it from (the system's own list of active apps still shows and stops it).

## Tests

- Core: the notice's wire form, `HungUp` as a final event, routes to a code and back, unknown numbers refused.
- UI (`tests/live.rs`): a room without a notice ends on leaving the screen; one with a notice goes on and ends by each of its five endings; `forget_pending` leaves it; the notification can only end a room that has one; a shared screen keeps a room until sharing stops; routes arrive and a choice is recorded.
- Engine: a background room's events (a route change, the hang-up) reach a hidden app and its answer leaves at once.
- Barua Talk (`tests/sound.rs`): the notices, a call that goes on and is still there on return, the hang-up from the notification reaching the server for both kinds of call, the other side leaving checked with no tick, the Speaker button and the status line following the route.
- The host: built for Barua Talk with all four services and for apps with fewer, and the permissions and services of each APK read back with `aapt2`.

## Not verified

Everything on a device. In the order to try, with the log line (`adb logcat -s Morningstar`) that proves each:

1. Notifications asked on Android 13+: `live: asking the user to allow notifications`, then `live: notifications allowed`.
2. The service at connect: `live: the microphone's foreground service is running`, and the notification with the call's name.
3. Home during a call: `live: id=N goes on behind another app: it has a notice`; **the other side still hears this phone**, which is the whole point and only ears can tell; the same with the screen locked.
4. The notification's action: `live: id=N ended from the notification`, `room released`, `the microphone's foreground service stopped`, and the call shown as ended on the other side.
5. Swiping the app away: `live: id=N left: the app was closed from the recent apps`.
6. An app without `live-background`: `will end with the screen: the app does not declare live-background`.
7. Routes: `live: id=N sound on the earpiece, routes: the earpiece, the loudspeaker`; a Bluetooth headset connected mid-call adds `a Bluetooth headset` and moves the sound; the Speaker button moves it back and forth; on Android 11 or below as well as 12 or above.
8. The proximity sensor: `live: the screen turns off at the ear`, on a voice call in the earpiece only.
9. That both services of a shared screen still start (0039) now that one of them may already be running.
10. Battery and heat of a long call in the background. Not measured.
