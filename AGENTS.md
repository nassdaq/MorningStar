# Morningstar — Engineering Constitution

Instructions for every engineer and AI agent working in this repository.

## Project Identity

Morningstar is a Rust-native, GPU-first, cross-platform mobile application framework targeting Android and iOS.

## Non-negotiable principles

1. Rust is the primary implementation language.
2. The UI engine must be independently developed and owned by Morningstar.
3. GPU rendering must be a foundational architectural capability.
4. Application developers must maintain one shared Rust application codebase.
5. Platform-specific integration must remain isolated behind stable interfaces.
6. Performance must be measurable and continuously evaluated.
7. Architecture must prioritize long-term maintainability.

## Engineering Rules

- Never introduce unnecessary dependencies.
- Never duplicate platform implementations when a shared abstraction is possible.
- Avoid unnecessary memory allocations and data copying.
- Minimize unsafe Rust and document every necessary unsafe boundary.
- Avoid blocking the UI thread.
- Avoid unnecessary rendering and layout recalculation.
- Keep public APIs independent of internal implementation details.
- Every subsystem must have clear ownership and responsibilities.
- All architectural decisions must be documented.

## Development Workflow

Before implementing a subsystem:

1. Understand its responsibilities.
2. Identify its dependencies.
3. Define its public interfaces.
4. Identify platform-specific requirements.
5. Define measurable acceptance criteria.
6. Implement the smallest working version.
7. Test on actual target platforms.
8. Document architectural decisions.

## Performance Rules

- Every performance optimization must have measurable evidence.
- Avoid premature optimization.
- Benchmark changes against a reproducible baseline.
- Measure CPU, GPU, memory, frame time and battery consumption.
- Never claim performance improvements without measurements.

## Architectural Discipline

Do not replace Morningstar's architecture with an existing framework.

Existing Rust libraries may be used as implementation dependencies, but the core Morningstar architecture, public APIs, UI model and rendering orchestration must remain independently controlled.

## Definition of Completion

A feature is not complete merely because it compiles.

It must have:

- Working implementation.
- Appropriate tests.
- Android compatibility where applicable.
- iOS compatibility where applicable.
- Documented interfaces.
- Performance considerations.
- Clear failure handling.

---

## Working in this repository

### Where things are

| Path | Owns |
| --- | --- |
| `crates/morningstar` | Public developer API. The only crate an app depends on. Published as `morningstar-framework` (the name `morningstar` is another project's on crates.io); its library is still `morningstar` (ADR 0045). |
| `crates/morningstar-core` | Dependency-free types: `Color`, `Lifecycle`, `FrameStats`, the data of platform services (`HttpRequest`, `Recording`, `LiveJoin`, `LiveEvent`, `ServiceRequest`, ...), and a video's frames (`VideoFrame`, kept one per video in the `ImageStore`). |
| `crates/morningstar-gpu` | The only crate that uses `wgpu`. No `wgpu` type appears in its public API. |
| `crates/morningstar-text` | Fonts and families, shaping, layout cache, glyph atlas, vector icons and multi-colour vector pictures. The only crate that uses `cosmic-text` and `zeno`. |
| `crates/morningstar-layout` | Layout engine: fixed / hug / fill boxes to rectangles. Depends only on core. |
| `crates/morningstar-ui` | Declarative widgets, Elm-style state and events, themes, and `Effects` (HTTP, audio, stored values as messages) with its test driver. Apps use it as `morningstar::ui`. |
| `crates/morningstar-runtime` | `Engine`: lifecycle in, frames out, and platform service requests out and answers in. The one object a host drives. |
| `crates/morningstar-android` | JNI entry points, the Rust-to-Java upcalls (`java.rs`) and the render thread. Empty off Android. |
| `crates/morningstar-ios` | C ABI for the UIKit host. Empty off iOS. |
| `templates/android`, `templates/ios` | Thin native host projects. No application logic. They implement the platform services (`MorningstarServices.java`, `HostServices.swift`). The live room is `MorningstarLive`, which exists twice on Android: `app/src/live/java` (Kotlin, LiveKit, with `MorningstarLiveService` beside it) for an app that declares `services = ["live"]`, `app/src/nolive/java` (refuses) for every other. What of LiveKit's manifest an app keeps (the camera for `live-video`, the foreground services for `live-screen`, the microphone's service and its notification for `live-background`) is written per build by `app/build.gradle.kts`. |
| `tools/morningstar-devtools` | Renders an app to a PNG with no device, and compares it with a design image (`morningstar-diff`). Generates an app's launcher icons and reads its name and id from its `Cargo.toml` (`morningstar-icon`, ADR 0020). Creates a new app from the templates in `templates/app/` (`morningstar-new`, ADR 0041). |
| `tools/morningstar-preview` | The development preview (ADR 0040): a third host, for the Mac. Runs any app in a window through the same `Engine`, with the mouse as a finger, the keyboard as text input, HTTP, stored values and the clipboard really carried out, and reloads it when its source changes. The only crate that uses `winit`; nothing of it is linked into an app. |
| `tools/morningstar-cli` | The released `morningstar` command (ADR 0045): `new` (a standalone app on crates.io's framework), `preview`, `build android`, `icon`, `diff`, `svg`, for an app outside this repository. Runs the devtools' and the preview's code; carries the Android host (`android/` is a link to `templates/android`, embedded file by file in `src/android.rs`). |
| `apps` | Applications started with `morningstar-new` (ADR 0041): somebody's app, not a framework example. Made on first use. |
| `examples/hello-gpu` | The low-level example: paints and hit-tests by hand. |
| `examples/showcase` | Three realistic screens written with the UI layer. |
| `examples/services-check` | Device smoke test for platform services: one button per service, the live room included. Also the smallest app that uses every effect but a room's data, camera and screen (it declares `live` alone: the voice-only build), with scripted tests. |
| `examples/fundidesk` | Twelve screens reproduced from a real design, each tested against its reference image. The model to imitate; start with its `SCREENS.md`. |
| `examples/callready` | Five illustrated screens from a second design: turned elements, multi-colour vector pictures, text effects. Tested against its references. |
| `examples/atlas` | Ten screens from a third design, an AI assistant: radial gradients, blur, inner shadows, a bottom sheet, a sideways chip row. With its server (`server/`, DeepSeek and optional voice) the chat is real; without it, scripted. Tested against its references. Start with its `SCREENS.md`. |
| `examples/barua-talk` | Five screens from a fourth design, rooms, chat and calls: photographs beside the drawing, a blurred backdrop, an arched bar. With its server (`server/`, the Barua Talk API) rooms, messages, in-app calls and phone calls are real and carry sound, both cameras and a shared screen through a live room that goes on behind another app (ADRs 0038, 0039 and 0042); without it, scripted from the design's own people. Tested against its references. Start with its `SCREENS.md`. |
| `docs/architecture` | Architecture Decision Records. Read before changing a subsystem. |
| `docs`, `site` | The documentation: Markdown in `docs/` (pages listed in `docs/site.json`), built into the static site `site/` by `tools/build-docs.py` (ADR 0044). `site/**/*.html` and `site/assets/search-index.js` are output: never edit them. `site/assets` (stylesheet, script, logo, fonts) and `site/shots` (screenshots, written by `tools/docs-screenshots.sh`) are sources. |
| `examples/docs-snippets` | The documentation's code samples, as a crate, so that they compile. Not an app. A page includes a region marked `// docs:begin name` ... `// docs:end name`. |
| `benchmarks` | Benchmark method and reports. `benchmarks/host` (`morningstar-bench`) times every designed screen on this machine with no phone: a regression baseline, not a device measurement. |
| `.github/workflows` | Continuous integration (ADR 0043): lint and GPU-free tests on Linux, every test on macOS for `main`. No secrets. |

Dependency direction is one-way: `core` ← `gpu`, `text`, `layout` ← `runtime` ← `ui` ← platform crates ← `morningstar` ← apps. `core`, `gpu`, `text` and `runtime` must never depend on a platform crate or contain `cfg(target_os)` beyond the `NativeSurface` handle type.

### Commands

```sh
cargo run -q -p morningstar-cli -- --help   # the released command, for apps outside this repository (ADR 0045)
cargo run -q -p morningstar-devtools --bin morningstar-new -- my-app --name "My App" --id com.example.myapp   # a new app in apps/my-app (add --server for a Node server skeleton); docs/getting-started.md
cargo test --workspace                 # host tests, including pixel tests on the real GPU
cargo run -p hello-gpu --example screenshot out/   # render the app to PNGs; look at them
cargo run -q -p morningstar-devtools --bin morningstar-diff -- design.png out/idle.png --out heat.png
cargo run -q -p morningstar-devtools --bin morningstar-svg -- art.svg out/art.png --scale 3   # an SVG file as an app would draw it
cargo clippy --workspace --all-targets
cargo ndk -t arm64-v8a clippy -p hello-gpu   # type-check and lint the Android code
cargo run --release -p morningstar-bench -- --markdown   # host baseline: build and draw time of every designed screen (benchmarks/README.md)
tools/preview.sh barua-talk            # the app live in a window on the Mac; reloads when a source changes (--help lists options and keys)
tools/build-android.sh [--install]     # APK (MORNINGSTAR_APP=showcase, MORNINGSTAR_ABIS=arm64-v8a; the app's `services` decide what the host is built with)
cargo run -p showcase --example screenshot out/ 3   # the showcase screens as PNGs at scale 3
cargo run -p fundidesk --example screenshot -- out/ 07 --diff --phone   # one FundiDesk screen, compared with its design
tools/build-ios.sh                     # XCFramework, icon and names for the example (MORNINGSTAR_APP; needs full Xcode)
cargo run -q -p morningstar-devtools --bin morningstar-icon -- callready --preview out/icon.png   # the app's launcher icon in every form; look at it
cargo run -p callready --example icon  # redraw an app's source icon (assets/icon.png) from its own art
python3 tools/build-docs.py            # rebuild the documentation site (site/) from docs/; --check fails if it is stale or a link is broken
tools/docs-screenshots.sh [app ...]    # re-render the site's screenshots (site/shots) and the tools' help texts; then rebuild the site
```

### Building a screen from a design

Read `docs/building-screens.md` and `docs/design-to-code.md`, imitate `examples/showcase`, render with the screenshot example, compare with `morningstar-diff`, and iterate.

### Rules specific to this codebase

- The Java/Swift hosts and their Rust counterparts are two halves of one interface. `MorningstarNative.java` ↔ `morningstar-android/src/jni.rs` and `java.rs`, and `morningstar_ios.h` ↔ `morningstar-ios/src/lib.rs`, must change together. An app library and a host from different sides of such a change must never be combined: rebuild both.
- An application never does I/O itself: no sockets, files, threads or sleeps in `update` or `view`. Work that takes time goes through `Application::effects` (ADR 0019; a live room: ADRs 0038 and 0039), and tests answer it with `Ui::testing`. A video's pictures never pass through the application: it draws a `LiveVideo` for a track, and a test or a tool supplies frames with `Frame::set_video_frame` or `screenshot_frame_with_videos`.
- Record a new ADR in `docs/architecture` when a decision there is changed or a new cross-cutting one is made. Do not edit an accepted ADR's decision; supersede it.
- Dependency versions move quickly (wgpu's API changed materially between major versions). Check the version in `Cargo.lock` and its docs rather than relying on memory.
- A platform service that costs every app that carries it something (the live room: LiveKit and WebRTC, about 20 MB; `live-video`: the camera permission; `live-screen`: three foreground-service permissions and two services; `live-background`: the microphone's service and the notification permission) is opt-in: the app names it in `services = [...]` under `[package.metadata.morningstar]` (ADRs 0038, 0039 and 0042), the build script passes that to the host's build, and an app that does not name it must not carry any of it. Check by listing the APK and its permissions (`aapt2 dump permissions`), not by reading the build file.
- A live room holds the microphone open. Nothing may leave one connected by accident: it is left when the app leaves the screen (unless this device's screen is shared in it, which the user agreed to in a system dialog and a notification shows: ADR 0039; or the application joined it with a notice in an app built with `live-background`, where a notification names the call and can end it: ADR 0042), on `forget_pending`, and when the engine goes, and a change there comes with a test of what happens to the room.
- A new app is started with `morningstar-new`, not by copying an example, and lives in `apps/`. Its templates (`tools/morningstar-devtools/templates/app/`) are a second copy of how an app is written: when the UI layer's API changes, change them too. `cargo test -p morningstar-devtools --test new_app` generates an app and runs its tests, so it is what tells you.
- The framework's crates, the devtools, the preview and `morningstar-cli` are one release on crates.io (ADR 0045): one `version` in `[workspace.package]`, and every internal dependency in `[workspace.dependencies]` has a path and the same exact `version = "=x.y.z"`. Change them together. A new published crate sets `publish = true` and links `LICENSE-MIT` and `LICENSE-APACHE`; anything else stays unpublished. Publishing is public and permanent: only the owner runs `cargo publish`, never an agent.
- A file added to or removed from `templates/android` must also be added to or removed from `HOST` in `tools/morningstar-cli/src/android.rs` and from that crate's `include` list. A test compares them with git.
- An app's name, id and launcher icon are declared in its `Cargo.toml` under `[package.metadata.morningstar]` and nowhere else. The hosts' icon resources are generated by the build scripts into `build/` directories: never add an icon to `templates/`, and never special-case an app in a script.
- After changing anything visual, render a screenshot and look at it. Passing tests do not show that a screen looks right.
- The preview (`tools/preview.sh`) is for using an app while writing it; it is not a phone. What only a phone decides (the soft keyboard, the system bars, sound, a live room, frame times) is still stated as unverified until it has run on one.
- The offscreen pixel tests are the specification of the renderer. A new drawing operation comes with pixel tests.
- A phone connected over adb belongs to a person who may be using it. Never call `adb shell input`, monkey or `screencap` directly. The only permitted route is `tools/adb-guarded.sh`, which checks the foreground app immediately before each command, and even then only when the owner has agreed and is not using the app: a guarded tap has still landed while the owner was mid-typing. Prefer proving behaviour from logs.
- Disk space on the main development machine is tight: prefer `-p` builds, avoid release and cross builds you do not need.
- State what was and was not verified on a real device. `docs/architecture/README.md` tracks verification status.
- The documentation says only what the repository does. After changing a guide, a doc comment of a public method, a tool's help or a sample in `examples/docs-snippets`, run `python3 tools/build-docs.py` and commit `site/` with the change. A new code sample in a page goes into `examples/docs-snippets` and is included from there, never typed into the Markdown. After changing how an example's screen looks, run `tools/docs-screenshots.sh <app>`.
