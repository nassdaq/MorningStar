# Live rooms

A live room carries sound, pictures and small data messages between the people in it. On Android it is LiveKit's client, behind the same effects interface as every other service. The app's server hands out a ticket (an address and a token); the app joins with it.

Live rooms are **opt-in per app**, because LiveKit and WebRTC add about 20 MB to an APK. An app that does not ask for them carries none of the code. See [Opt-in services](../reference/services.md).

```toml
[package.metadata.morningstar]
services = ["live", "live-video"]
```

## Joining, and following the room

A join is answered more than once: one message per event until a final one. `Disconnected` and `Failed` are final.

{{snippet examples/docs-snippets/src/live.rs#events}}

{{variants crates/morningstar-core/src/services.rs LiveEvent}}

One room at a time. Joining a second room leaves the first, which ends with `Disconnected(Replaced)`.

## Pictures

A camera or a shared screen arrives as a `VideoStarted` event with a `VideoTrack`. The app draws the track with `LiveVideo`; the frames themselves never pass through the application.

{{snippet examples/docs-snippets/src/live.rs#view}}

`LiveVideo` draws nothing until a frame exists, so whatever is stacked underneath is the placeholder. Camera pictures need the `live-video` service; sharing this phone's screen needs `live-screen`.

## Controls

| Method | Effect |
| --- | --- |
| `out.set_live_microphone(bool)` | Mute and unmute |
| `out.set_live_speaker(bool)` | Loudspeaker on or off |
| `out.set_live_audio_route(route)` | Choose the earpiece, the loudspeaker, a wired headset or a Bluetooth one. Routes are reported by `LiveEvent::AudioRoutes` |
| `out.set_live_camera(bool)`, `out.switch_live_camera()` | The camera, and front or back |
| `out.set_live_screen_share(bool)` | Share this phone's screen. Android asks the user first |
| `out.send_live_data(topic, payload, reliable)` | A small message to everyone in the room, at most 15,000 bytes |
| `out.leave_live()` | Leave |

## When a room ends

A live room holds the microphone open, so nothing may leave one connected by accident.

{{variants crates/morningstar-core/src/services.rs LiveDisconnect}}

By default a room is left when the app leaves the screen: Home, another app, or the screen locking. Two things change that:

- **A shared screen.** A room in which this phone's screen is being shared continues behind other apps, since sharing is pointless otherwise. The user agreed in a system dialog and a notification shows it.
- **`live-background`.** An app that declares this service and joins with `LiveJoin::background(LiveBackground::new(title, text))` keeps the call going behind another app or with the screen locked, with an ongoing notification that has a Hang up action.

## What has been verified

Everything on this page passes its tests on the development machine, and the Android side compiles and packages. **None of it has been confirmed on a phone yet**: no sound has been heard and no picture seen through a real room. The iOS host answers every join with `Unsupported`.

The decisions, with every case a room ends in, are [ADR 0038](../architecture/0038-live-rooms.md), [ADR 0039](../architecture/0039-live-video-screen-and-data.md) and [ADR 0042](../architecture/0042-live-rooms-in-the-background-and-audio-routes.md). The [Barua Talk example](../examples/barua-talk.md) is the app that uses all of it.
