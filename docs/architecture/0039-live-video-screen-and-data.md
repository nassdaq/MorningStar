# 0039 — What a live room carries besides sound: data, pictures, the camera and the screen

Status: Accepted (2026-11-15). Verified on the development host only, and by building three APKs and reading them back. **Nothing here has run on a device, no room has been joined, no camera has been opened and no frame has been decoded**: the conversion and the drawing are pixel-tested on the host's GPU with frames made in the tests, the Rust type-checks for Android and for iOS, and the Android host (Java and two Kotlin files) compiles and packages against LiveKit 2.30.0. The server that hands out tickets still answered 500 to creating rooms and calls while this was written. The Swift has never been compiled, and on iOS the live room remains an honest refusal. What that leaves open is under "Not verified".

Extends [0038](0038-live-rooms.md), whose decision stands: one room at a time, joined with a ticket, answered once per event, opt-in per app. This record carries out the design 0038 wrote down under "Video: the design to follow" (its second answer: decoded frames into the renderer, not a native view), and decides the three things 0038 left out: data messages, the camera, and sharing the screen. It changes one consequence of 0038, said plainly below: a room in which this device's screen is shared is **not** left when the app leaves the screen.

Uses [0014](0014-images.md) (a picture is a texture drawn by the quad pipeline), [0012](0012-scale-insets-and-demand-rendering.md) (a frame is drawn when something changed), [0023](0023-compositing-space.md) (encoded colours, blended as they are) and [0019](0019-effects-and-platform-services.md).

## Context

Barua Talk's calls carried sound and said "Video needs the next version." Its owner asked for video calls, screen sharing and chat through the room. The tickets it is given are LiveKit's and already allow all three; the framework could join a room and do nothing in it but talk.

## Decision

### 1. Data

`Effects::send_live_data(topic, payload, reliable)` sends bytes to everyone else in the room; what arrives is `LiveEvent::Data(LiveData { from, topic, payload })`. Not answered, not sent without a room, and not sent when the payload is over `MAX_LIVE_DATA_BYTES` (15,000: what LiveKit carries reliably in one piece). `LiveData` prints its size, never its contents or its sender.

A data message carries bytes, which the four values of `LiveEvent::from_wire` cannot, so it crosses each boundary by a call of its own: `nativeLiveData` on Android, `morningstar_ios_live_data` on iOS.

Chat is not a framework notion. Barua Talk keeps sending a message to its server, which stores it (the room's history is the server's) and forwards it to the room's `"chat"` topic; what the live room adds is the moment. When data arrives on that topic the app asks the server for the room's messages at once instead of at its next poll. It does not read the data and does not merge it with what the server says, so nothing can appear twice.

### 2. Pictures

**The data.** `VideoTrack(u32)` names one video, given by the host and never reused. `VideoFrame` is one decoded picture as I420: three tightly packed planes, a rotation (`VideoRotation`, the quarter turns that show it upright) and whether it is mirrored. `VideoFrame::from_i420` copies planes that have padding into the buffers of an earlier frame, so a stream allocates at its start and when it grows; `from_rgba8` makes one for tests; `to_rgba8` is the conversion written out on the CPU, the reference the GPU is tested against. All in `morningstar-core` (`video.rs`).

**Where frames live.** In the `ImageStore`, beside the images: `set_video_frame(track, frame)` replaces the video's one frame and hands the old one back. **There is no queue anywhere**: a video is its latest frame. `ImagesView::video(track)` gives a renderer the frame and a serial that changes with every new one. Keeping them in the store means the renderer's interface (`render_with_images`) did not change and every presenter, tool and test already carries them.

**Drawing.** `DrawOp::Video { track, size, rect, source, corner_radius, tint }`, recorded by `DisplayList::draw_video_fit` and `draw_video_region`, is an image operation whose pixels come from a video. In `morningstar-gpu` (`videos.rs`) each video that is drawn has three `R8` textures for its planes and one `Rgba8` texture the size of the picture as shown. When the frame in the store is not the one in that texture, the planes are uploaded and one small render pass converts them (BT.601 limited range; the turn and the mirror are undone in the same shader), submitted before the frame that draws it. The quad pipeline then samples the converted texture exactly as it samples an image, so a video is clipped, rounded, tinted, faded in a layer and turned by the code that does that for everything else, and the quad shader did not change. No `wgpu` type is public; `RenderStats` gained `video_conversions` and `video_textures`.

Chosen over converting in the quad shader (three more textures in a pipeline every instance goes through, for the sake of one kind) and over converting on the CPU (a multiply per pixel per frame on the thread that builds frames).

**Demand.** `Engine::video_frame(track, frame)` stores the frame and schedules a frame only if the display list last built draws that track; `video_ended` and `videos_ended` forget pictures. A video nobody shows costs the copy and nothing else: no upload, no conversion, no frame. A video on screen costs one frame per picture, which is a whole frame of the application (its `view` is built again), as for an animation.

**The element.** `LiveVideo::new(track)` in `morningstar-ui`: fit (`Cover` unless told), radius, tint, and it is clipped and faded with what it is in. It has no size of its own, and draws nothing while its video has no frame: what should be seen meanwhile goes underneath, in a `Stack`. That is the placeholder, and it is why Barua Talk's designed photographs and its fidelity tests are untouched.

**The service.** `LiveJoin::camera(bool)`, `Effects::set_live_camera(bool)`, `Effects::switch_live_camera()`; `LiveEvent::VideoStarted(LiveVideoStream { identity, source, local, track })` and `VideoStopped`, where `source` is `LiveVideoSource::Camera` or `Screen`; and `LiveEvent::MediaFailed { source, error }` when this device's camera or screen could not be turned on, which is **not** the end of the room. A remote shared screen is a video like any other. This device's own camera is one too (`local`), mirrored when it is the front camera; its own shared screen is announced with a track but has no pictures, since the screen is already in front of its user. A camera that is turned off is a video that stopped.

**Across the boundary (Android).** A `VideoSink` per shown track (`MorningstarLive.Picture`) receives frames on a WebRTC thread, asks for I420, and calls `nativeVideoFrame(track, width, height, rotation, mirrored, y, yStride, u, uStride, v, vStride)` with the three direct buffers, which Rust reads through `GetDirectBufferAddress` and copies before the call returns; the buffer is released after. In `morningstar-android` (`video.rs`) each video has one slot: a frame the render thread has not taken is replaced by the next (and its buffers reused for it), and the render thread is woken once per video however many frames arrived. `nativeVideoEnded(track)` ends one; the room's final event ends all.

### 3. This device's screen

`Effects::set_live_screen_share(bool)`. On Android the system asks the user every time (`MediaProjectionManager.createScreenCaptureIntent`, through the activity); a refusal is `MediaFailed { Screen, PermissionDenied }`. LiveKit 2.30.0 provides the capture and its foreground service (`ScreenCaptureService`, type `mediaProjection`, started by `setScreenShareEnabled` with the `ScreenCaptureParams` the host builds), read from its classes and its manifest rather than from memory.

**A room in which the screen is shared goes on when the app leaves the screen.** 0038 ends a room on `onStop` because a microphone must not stay open where its user cannot see it. Sharing a screen that ends when another app is opened would share nothing but the app itself, so here the rule yields, and what made the rule necessary is answered another way: the user has just agreed to a system dialog about it, and for as long as it lasts the system shows a notification in the app's name ("Sharing your screen. The call's microphone stays as you left it.") and its own indicator.

The microphone consequence: Android silences the microphone of an app that is not in front unless a foreground service of type `microphone` holds it, and LiveKit's service is of type `mediaProjection` only. So the host has a second service, `MorningstarLiveService`, which does nothing but be that, started when the user agrees to share and stopped when sharing stops; the two show one notification. The microphone stays exactly as it was: muted stays muted, live stays live. The camera does not: Android gives a background app no camera, so it is turned off (`VideoStopped`) while the app is hidden and on again when it is back.

When sharing stops while the app is hidden (the application's doing, the system's "stop sharing", or a failure), 0038's rule applies again at once: the room ends as `Hidden`. While the question is on screen nothing is decided either way, because on some releases the question covers the app completely.

The rule "nothing may leave a room connected by accident" therefore reads: a room is left when the app leaves the screen **unless this device's screen is shared in it**, on `forget_pending`, and when the engine goes. Sharing never starts by itself.

### Opt-in

Two more names beside `live`, both meaningless without it (the build says so):

| `services` | What the APK gains |
| --- | --- |
| `live` | Voice and data. As 0038: LiveKit, WebRTC, `ACCESS_NETWORK_STATE`, `MODIFY_AUDIO_SETTINGS`. Remote video is given up (unsubscribed), so nothing is received or decoded for pictures the app cannot show. |
| `live-video` | Pictures: the others' cameras and shared screens are shown, and this device's camera can be turned on. `CAMERA`, with `android.hardware.camera` and `camera.autofocus` declared not required. No more code: the classes are LiveKit's, which `live` already brought. |
| `live-screen` | Sharing this device's screen. `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_MEDIA_PROJECTION`, `FOREGROUND_SERVICE_MICROPHONE`, LiveKit's `ScreenCaptureService` and `MorningstarLiveService`. |

LiveKit's own manifest asks for the camera and the screen-capture service in every app that carries it. What an app did not ask for is taken out again by a small manifest that `app/build.gradle.kts` writes for each build from the app's services and merges over the main one. At run time the host reads the same fact back from its own manifest (`requestedPermissions`): a camera or a share asked of an app built without it is `MediaFailed { Unsupported }`, and the log says which name is missing from the `Cargo.toml`.

Read back from the three APKs built for this record (`aapt2 dump permissions`, the manifest's services, the native libraries):

| App | `services` | Permissions beyond INTERNET and RECORD_AUDIO | Services | Native libraries |
| --- | --- | --- | --- | --- |
| atlas | none | none | none | its own |
| services-check | `live` | ACCESS_NETWORK_STATE, MODIFY_AUDIO_SETTINGS | none | its own, LiveKit's three |
| barua-talk | `live`, `live-video`, `live-screen` | those two, CAMERA, FOREGROUND_SERVICE, FOREGROUND_SERVICE_MEDIA_PROJECTION, FOREGROUND_SERVICE_MICROPHONE | `MorningstarLiveService`, `ScreenCaptureService` | its own, LiveKit's three |

Barua Talk's APK is 34,291,318 bytes with all three, against 34,241,966 with `live` alone: video and screen sharing add about 50 KB, because the weight is WebRTC's and was already there.

### The two halves

- Out (Android): four more `SERVICE_LIVE_*` commands through `onServiceCommand` (14 data, 15 camera, 16 switch camera, 17 screen share), and a fourth settings byte in the join (camera).
- In (Android): `nativeLiveEvent` kinds 8 (`VideoStarted`), 9 (`VideoStopped`), 10 (`MediaFailed`), and three new natives: `nativeLiveData`, `nativeVideoFrame`, `nativeVideoEnded`. The activity passes on `onRequestPermissionsResult` (the camera question, which is the live room's own) and `onActivityResult` (the screen question). The twin in `src/nolive/java` has the same two entry points and does nothing.
- iOS: request kinds 15 to 18, event kinds 8 to 10, `morningstar_ios_live_data`, `morningstar_ios_video_frame`, `morningstar_ios_video_ended`, in `morningstar_ios.h` and `lib.rs` together. `HostServices.swift` names the new kinds beside the ones it already ignores. iOS still answers every join `Unsupported`.

## Barua Talk

`examples/barua-talk` declares all three services. A call with video joins with the camera as its button stands. On the call screen the other side's camera fills the screen over their face (which shows until the first frame and while their camera is off), a screen someone shares is shown whole in place of it, the user's own camera is in the designed tile (touching it changes camera), the Camera switch acts on the room, and "Share your screen" shares the phone's and stops. A line says what is true: "Sound and video are live.", "Sound is live. The other side's camera is off.", "Sound is live. You are seeing a shared screen.", "You are sharing your screen. Sound is live.", and a refusal in words of the app's own. In a room with several cameras the one that started last is shown: a grid of people is not designed and not built. The scripted app is unchanged.

## Consequences

- An application can show any video of a room anywhere a picture can go, with the framework's own clipping and compositing, and the fidelity tests can render a call with pictures (`screenshot_frame_with_videos`).
- A frame of video is copied twice on its way to the screen: out of WebRTC's buffer (which also converts a hardware buffer to I420 first), and into three textures. Then one conversion pass and the application's frame. **None of this has been measured**; see below.
- The quad shader, the renderer's interface and every existing draw operation are unchanged.
- A third and a fourth thing an app can declare, and a manifest that is partly generated.

## Limitations

- **Android only**; iOS refuses the room.
- **No reduced copies of a video.** A picture shown much smaller than it is (720p in a 100 dp tile) is sampled bilinearly from its full size and is coarser than an image would be.
- **A whole application frame per picture.** There is no path that redraws only a video.
- **One conversion: BT.601 limited range.** A source that says it is BT.709 or full range is drawn slightly off in colour; WebRTC's frames do not carry the fact to a sink.
- **No choice of what is received**: no simulcast layer, no pausing of a video that is off screen (it is still received and decoded; only the drawing stops).
- **No sound of the shared screen**, no choice of resolution or frame rate, no choice of camera by name (front and back only).
- **The notification** of a shared screen is not asked permission for (`POST_NOTIFICATIONS`, Android 13 and later): where notifications are off the system's own indicator is what shows.
- **A room without a shared screen still ends in the background**, as in 0038 (unless it was joined with a notice: [0042](0042-live-rooms-in-the-background-and-audio-routes.md)). A voice call that survives the background is not this record.
- **Data**: to everyone, never to chosen people; 15,000 bytes; no report of delivery.
- As 0038: one room at a time. (Bluetooth headsets: [0042](0042-live-rooms-in-the-background-and-audio-routes.md).)

## Tests

- `crates/morningstar-core/src/video.rs`: 8 (the round trip of a colour, the ends of the range, padding, refusals, reuse of buffers, the four turns and the mirror, degrees, no picture in `Debug`). `services.rs`: 2 new (the three kinds across the boundary, the four commands).
- `crates/morningstar-gpu/tests/video.rs`: 8 pixel tests on the host's GPU against the CPU reference: colours, range, every turn with and without the mirror, a turned frame that is not square, rounding, clip and tint, cover fit and paint order, replacement and release with their conversions counted, nothing converted out of view.
- `crates/morningstar-runtime/src/engine.rs`: 1 (a frame is scheduled only while its video is drawn).
- `crates/morningstar-ui/tests/live_video.rs`: 9 (data out and in, too long and no room, camera, switch and screen on the driver's room, refusals, drawn only with a picture, a shared screen, own video told apart, the pixels that reach the screen, under an engine).
- `examples/barua-talk/tests/video.rs`: 12, with fingers and the stand-in server, the last of them rendered.
- `tools/morningstar-devtools`: the three service names.

## Not verified

Everything on a device. With the log line that would prove each:

- A data message arrives: `live: id=N data received, K bytes`, then `service: id=N live room: data K bytes`; one sent: `live: id=N data K bytes sent`.
- A remote camera is shown: `live: id=N video T started (camera)`, `live: video T first frame WxH rotation R`, `service: id=N live room: someone's Camera started, track T`. That the picture is upright, the right colours and not mirrored is for eyes.
- This device's camera: the system's question the first time, then `live: id=N camera published` and `video T started (own camera)`; refused: `camera not started: permission denied`; switched: `camera is now the back one`. That the front camera's tile is a mirror is for eyes.
- An app without `live-video`: `camera refused: the app does not declare live-video`, and for a remote video `a video is not shown: the app has no live-video`.
- Sharing: `asking the user to share the screen`, the system's dialog, `live: the microphone's foreground service is running`, `live: id=N screen shared, video T`; declined: `screen not shared: the user declined`. With Home pressed: `goes on behind another app: the screen is shared`, and the other side still sees and hears. Stopped while hidden: `screen no longer shared`, `left: the app is not on screen and shares nothing`, `room released`.
- That both foreground services start on Android 14 and 15 with the types declared, that the microphone is in fact heard from the background, and that the camera's return on coming back works.
- **Cost**: the copy and the upload per frame (a 720p I420 frame is 1.4 MB), the conversion, frame time and battery with one and with several pictures, and the latency added. AGENTS.md: no performance claim without measurements; none is made. If the copies are too dear, the route 0038 names (the decoder's hardware buffer imported into `wgpu`) remains.
- Whether Barua's tickets allow what the app asks: its document says a `video: false` ticket is voice only, so the camera in a voice call should come back as "The camera could not be started."
- iOS, all of it; the Swift has never been compiled.
