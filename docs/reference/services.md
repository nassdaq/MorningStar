# Opt-in services

Some platform services cost every app that carries them something: megabytes, or a permission in the store listing. Those are opt-in. An app names them in its `Cargo.toml`, the build script passes the list to the host's build, and **an app that does not name a service carries none of it**.

```toml
[package.metadata.morningstar]
services = ["live", "live-video", "live-screen", "live-background"]
```

## What each name adds

| Name | Gives the app | Adds to the APK |
| --- | --- | --- |
| *(none)* | HTTP, storage, the clipboard, recording and playback | The base host: the `INTERNET` and `RECORD_AUDIO` permissions |
| `live` | [Live rooms](../guide/live-rooms.md): voice and data messages | LiveKit's client and WebRTC, about 20 MB. `ACCESS_NETWORK_STATE`, `MODIFY_AUDIO_SETTINGS`, `WAKE_LOCK`, and `BLUETOOTH` up to Android 11 |
| `live-video` | Pictures in a room: the others' cameras and shared screens, and this phone's camera | The `CAMERA` permission |
| `live-screen` | Sharing this phone's screen | `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_MEDIA_PROJECTION`, `FOREGROUND_SERVICE_MICROPHONE`, `POST_NOTIFICATIONS`, and two foreground services |
| `live-background` | A call that goes on behind another app or a locked screen | `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_MICROPHONE`, `POST_NOTIFICATIONS`, and one foreground service |

The last three need `live`; the build refuses them without it.

## Measured

The sizes of three debug APKs built for `arm64-v8a` on 2026-11-17:

| App | Services | APK |
| --- | --- | --- |
| Atlas | none | 9,952,682 bytes |
| services-check | `live` | 29,865,246 bytes |
| Barua Talk | all four | 34,307,878 bytes |

Atlas has one native library and no Kotlin or LiveKit classes. The apps differ in more than their services (Barua Talk carries photographs), so read the difference between the first two rows as the cost of `live`, about 20 MB, and not more precisely than that.

## Checking an APK

The rule in the repository is to check by listing the APK, not by reading the build file:

```sh
aapt2 dump permissions app-debug.apk
unzip -l app-debug.apk | grep '\.so$'
```

## On iOS

Nothing here applies yet. The iOS host answers every live room with `Unsupported`, and has never been compiled.

The decisions are [ADR 0038](../architecture/0038-live-rooms.md), [ADR 0039](../architecture/0039-live-video-screen-and-data.md) and [ADR 0042](../architecture/0042-live-rooms-in-the-background-and-audio-routes.md).
