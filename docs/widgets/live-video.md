# LiveVideo

The picture of a camera or a shared screen from a [live room](../guide/live-rooms.md).

{{snippet examples/docs-snippets/src/live.rs#view}}

{{api crates/morningstar-ui/src/element.rs LiveVideo}}

- The app draws a **track**, which it gets from `LiveEvent::VideoStarted`. The frames never pass through the application.
- It **draws nothing until a frame exists**, so whatever is stacked underneath is the placeholder.
- A frame is requested only while a `LiveVideo` for that track is on screen and a new picture has arrived.
- For a camera, `ImageFit::Cover` fills the box; for a shared screen, `ImageFit::Contain` shows it whole.

Pictures need the `live-video` service in the app's `Cargo.toml`; see [Opt-in services](../reference/services.md). In a test or a screenshot, frames are supplied with `Frame::set_video_frame` or the devtools' `screenshot_frame_with_videos`.

**Not yet seen on a phone.** The conversion and the widget pass their tests on the development machine. [ADR 0039](../architecture/0039-live-video-screen-and-data.md).
