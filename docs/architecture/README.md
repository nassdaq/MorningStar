# Architecture Decision Records

Each record states one decision, why it was made, and what it costs. An accepted record is not edited to change its decision; a new record supersedes it.

| # | Decision | Status |
| --- | --- | --- |
| [0001](0001-crate-boundaries.md) | Six crates with a one-way dependency direction | Accepted |
| [0002](0002-gpu-backend-wgpu.md) | wgpu behind a Morningstar-owned GPU crate | Accepted |
| [0003](0003-platform-hosts.md) | Thin native hosts, not NativeActivity or a windowing framework | Accepted |
| [0004](0004-threading-and-frame-pacing.md) | Dedicated render thread on Android; vsync pacing; idle when invisible | Accepted for Android, open for iOS |
| [0005](0005-application-entry-point.md) | The app crate is the native library; `app!` registers it | Accepted |
| [0006](0006-ui-architecture-direction.md) | Declarative API over a retained tree, emitting a display list | Proposed |
| [0007](0007-dependency-policy.md) | Dependency policy and current inventory | Accepted |
| [0008](0008-design-to-code-loop.md) | Offscreen screenshots as the basis of a design-to-code loop | Accepted |
| [0009](0009-text.md) | Text: own crate over cosmic-text, glyphs as display-list operations | Accepted |
| [0010](0010-layout.md) | Layout: own engine with fixed / hug / fill sizing, four linear sweeps | Accepted |
| [0011](0011-renderer-vocabulary.md) | Borders, gradients, shadows, clips and partial atlas upload in one draw call | Accepted |
| [0012](0012-scale-insets-and-demand-rendering.md) | Density scale, safe-area insets and demand-driven rendering | Accepted |
| [0013](0013-ui-layer.md) | Declarative UI layer: Elm-style state, widgets in dp, rebuilt per frame | Accepted |
| [0014](0014-images.md) | Images: one texture per image, draw calls split by image | Accepted |
| [0016](0016-text-input.md) | Text input: host keeps the editable copy, Rust-to-Java upcall, timed redraw for the caret | Accepted |
| [0015](0015-fonts-icons-and-design-fidelity.md) | Font families, vector icons, rich text, letter spacing, dashed borders; first design-fidelity measurements | Accepted |
| [0017](0017-transforms-and-vector-pictures.md) | Affine transforms carried per instance, multi-colour vector pictures composited on the CPU, synthetic italic, text shadow, outline and tabular numbers | Accepted |
| [0018](0018-back-and-timers.md) | System back answered by the application (asked without blocking, answered by upcall on Android); a repeating timer message built on the timed redraw | Accepted |
| [0019](0019-effects-and-platform-services.md) | Effects: HTTP, recording, playback and stored values done by the hosts with the platform's own stacks, asked for as data and answered as messages; an install id and stored values at setup; a test driver in place of a host | Accepted |
| [0020](0020-app-identity-and-launcher-icons.md) | App identity declared once in the app's `Cargo.toml` (`[package.metadata.morningstar]`: name, id, icon), read through `cargo metadata`; launcher icons and names generated per build for both hosts, never checked into the templates; a single icon becomes the Android adaptive icon by one stated rule | Accepted |
| [0021](0021-viewport-wrap-scroll-animation-navigation.md) | The viewport (size, insets, keyboard, text measurement) as an argument of the view; rows that wrap, inside the four layout sweeps; scrolling asked for through effects, positions kept by identity; a bounded multi-line field as a scroll view of its own text; implicit animation and enter/leave transitions by identity, frames only while something moves; a `Navigator` that is plain state | Accepted |
| [0022](0022-text-metrics-and-pixel-snapping.md) | Text measured and placed as a browser does: fractional widths and pen positions (a quarter of a pixel), the browser's baseline rule (half-leading rounded down, each line's top to the nearest pixel row), kerning across spaces, optional ligatures off under letter spacing, no hinting; a dashed border starts where Chrome's does | Accepted |
| [0023](0023-compositing-space.md) | Colours composited as sRGB-encoded values, like the web platform (non-sRGB target, colours encoded on the CPU, images and gradients in the same space); light text weighted to Chrome's; no linear-light mode. Supersedes the compositing space of 0002, 0011, 0014, 0015 and 0017 | Accepted |

## Verification status

What has actually been run, as of 2026-11-12 (after the first device session, the first parallel wave, transforms and vector pictures on the host, and effects and platform services on the host). Update this table when it changes.

| Claim | Status | Evidence |
| --- | --- | --- |
| Shared code builds and passes tests on the dev host | Verified | `cargo test --workspace`: 930 tests (16 more ignored: they need a device or a server) |
| Rectangles, rounded corners, anti-aliasing, blending and paint order produce correct pixels on a real GPU | Verified on macOS/Metal only | `crates/morningstar-gpu/tests/offscreen.rs` |
| Borders, gradients, shadows (against a CPU Gaussian reference), clips and partial atlas upload | Verified on macOS/Metal only | `crates/morningstar-gpu/tests/vocabulary.rs`, `atlas_upload.rs`; sample scene inspected by eye |
| Layout engine | Verified in tests | 90 tests in `crates/morningstar-layout`, including a hand-computed screen, wrapping rows (13, ADR 0021) and a zero-allocation check that includes them |
| Screenshot-versus-design comparison tool | Verified in tests | 64 tests in `tools/morningstar-devtools` |
| Demand-driven rendering, scale and inset propagation (engine logic) | Verified in tests on the host | `crates/morningstar-runtime` |
| Scale, insets, edge-to-edge and idling on Android | Verified on one device | Galaxy A27 log: `scale 2.8125 px/dp`, `insets top=102 bottom=42 px`, then `idle 6.0s: frames=1` (one frame drawn, render thread blocked) |
| The full shader (borders, gradients, shadows, clips, images, text) on Vulkan | Verified on one device, by eye | The showcase home screen on the Galaxy A27 matches the Metal screenshot |
| Widget layer and showcase screens | Verified in tests and by eye | 57 tests in `morningstar-ui`, 6 in `examples/showcase` including visual-regression references |
| Images | Verified on macOS/Metal; one image seen on device | 59 tests; the showcase avatar on the phone |
| A real design reproduced: FundiDesk, 12 of 12 screens | Verified against the design | Each screen differs from its Chrome-rendered reference in 0.09% to 0.36% of pixels (`examples/fundidesk/tests/fidelity.rs`), with nothing corrected by hand; of the twelve only onboarding was looked at by eye after ADR 0023, the rest are held by the comparison |
| A second design reproduced: CallReady, 5 designed screens | Verified against the design | 0.006% to 0.059% of pixels (`examples/callready/tests/fidelity.rs`), with the hand corrections for text placement and blending deleted; home looked at by eye beside its reference after ADR 0023 |
| Text placement (fractional widths and positions, the browser's baseline rule, kerning across spaces) and compositing in the encoded space (translucent colours, gradients, shadows, images, light-text weighting) | Verified on macOS/Metal only | `crates/morningstar-text/tests/metrics.rs` (12 tests, expected values from Chrome's layout of CallReady), `crates/morningstar-gpu/tests/compositing.rs` (6 pixel tests, expected bytes are Chrome's) |
| **The non-sRGB surface format on a device** (Vulkan and GLES; a surface that offers only sRGB formats and is rendered through a view of the plain format), **atlas use and frame time with sub-pixel positions and text tones**, and **light text on an Android phone beside Chrome on that phone** | **Not verified** | No device was used for ADR 0022 or ADR 0023; the light-text curve is fitted to Chrome on macOS |
| Font families, rich text, letter spacing, vector icons, dashed borders | Verified on macOS/Metal only | Tests in `morningstar-text`, `morningstar-ui`, `morningstar-gpu` |
| Transforms (rotation, slant, scale, move) of every operation kind, with clips outside and inside them; identity changes no byte | Verified on macOS/Metal only | `crates/morningstar-gpu/tests/transforms.rs` (18 pixel tests); FundiDesk's twelve fidelity figures unchanged to the fourth decimal |
| Multi-colour vector pictures, outlined text, synthetic italic, hard text shadow, tabular numbers | Verified on macOS/Metal only | `morningstar-text` (`picture.rs`, `tests/effects.rs`), `crates/morningstar-runtime/tests/pictures.rs` |
| Transform modifier in the UI layer: layout unaffected, touches follow the transformed picture | Verified in tests and by eye on the host | `crates/morningstar-ui/tests/transforms.rs`, `tests/rendered.rs` (one screen compared by eye with the design's rendering) |
| **Transforms and vector pictures on a device** (the 144-byte instance and the new shader paths on Vulkan and GLES; frame time; picture rasterisation time and memory) | **Not verified** | No device was used for ADR 0017; nothing was timed |
| FundiDesk launches on Android | Verified on one device, from the log only | Galaxy A27: first frame drawn, then idle. The screen was not inspected and no input was sent |
| Text input on Android | Verified on one device from the owner's own typing, by log | Galaxy A27, Samsung Keyboard: keyboard shown on focus with the right type, key-event and input-method edits reach the field, Next/Done, keyboard inset, caret blink at 10 timed frames in 6 s |
| **Text input: composition and autocorrect, swipe typing, Back-key dismissal, field staying above the keyboard, caret and focus visuals** | **Not verified** | Need the owner's eyes and fingers |
| System back (`Application::back`) and the repeating timer message (`Application::tick`): engine and UI logic | Verified in tests on the host only | 4 tests in `crates/morningstar-runtime/src/engine.rs`, 17 in `crates/morningstar-ui/tests/back_and_tick.rs`; Android and iOS halves type-check, the Java compiles against API 36 |
| **Back on a device** (the `OnBackInvokedCallback` on Android 16, the `onBackResult` upcall, leaving the app when declined, API 24 to 35), **a tick's frame rate on a device**, and any caller of `morningstar_ios_back` | **Not verified** | No device was used for ADR 0018; the Swift host does not call the function yet |
| Effects (`Application::effects`): requests out, answers in as messages, one frame per awaited answer and idle otherwise, `Unsupported` without a host, the test driver, storage round trip, host state at setup | Verified in tests on the host only | 8 tests in `crates/morningstar-core/src/services.rs`, 12 in `crates/morningstar-runtime/src/engine.rs`, 42 in `crates/morningstar-ui/tests/effects.rs`, 10 in `examples/services-check/tests` (one is the example of `docs/building-screens.md`, compiled and run) |
| Platform services, the two halves of each host interface agree | Checked by compiling and by comparison, not by running | Android: `cargo ndk clippy -p services-check` clean; the Java compiles with `javac -Xlint:all -Werror` against API 36; the upcall descriptors (`javap -s`) and native parameter lists (`javac -h`) compared with the Rust. iOS: the Rust type-checks, the header passes `cc -fsyntax-only`, the Swift passes `swiftc -parse` only |
| **Platform services on an Android device** (an HTTPS request, the cleartext rule and `adb reverse`, failure mapping, the 10 MB cap; the microphone permission flow, a recording and its format, the 60 s limit; MP3 and M4A playback, audio focus; stored values and the install id across a restart; behaviour across `onStop` and activity recreation; API 24 to 35; the APK build with the new manifest and `res/xml`) | **Not verified** | No APK was built and no device was used for ADR 0019. `examples/services-check` is the app to install for it |
| **Platform services on iOS** (`HostServices.swift`, `project.yml`'s Info.plist section) | **Not verified, never compiled** | No Xcode |
| App identity and launcher icons: the generator (sizes, masks, the single-icon rule, flattening, validation, determinism, unchanged files left alone, the manifest read through Cargo) | Verified in tests on the host; icons inspected by eye in the preview sheet down to 48 px | 31 new tests in `tools/morningstar-devtools` (`icon.rs`, `identity.rs`, `json.rs`, `tests/icon_cli.rs`) |
| Launcher icons and name in an Android APK | Verified by building and reading the APK back, not on a device | `aapt2 dump badging` of the CallReady APK: `application-label:'CallReady'`, `icon='res/mipmap-anydpi-v26/ic_launcher.xml'`; resource table has the three mipmaps at five densities and the adaptive XML; same for `hello-gpu` with the built-in icon |
| **Launcher icon and name on a device** (home screen, drawer, recents; the adaptive mask and parallax; the themed built-in icon; API 24 and 25 legacy icons) | **Not verified** | Nothing was installed for ADR 0020 |
| The viewport in `view`, wrapping rows, scroll requests and kept positions, laid-out and visible rectangles, the focused field through nested scroll views, a bounded multi-line field, `animate`, transitions with leaving widgets, reduce motion, `Navigator`; frames only while something animates and idle after (ADR 0021) | Verified in tests on the host only; three intermediate frames as pixels on macOS/Metal, one looked at | 35 tests in `crates/morningstar-ui/tests/adaptive.rs`, 5 unit tests in `motion.rs` and `navigator.rs`, 13 in `crates/morningstar-layout/tests/wrap.rs` |
| ADR 0021 in the examples: CallReady's tips as a wrapping row, its reply field bounded, its scroll asked for, its stack a `Navigator`; FundiDesk's chips as wrapping rows | Verified against the designs: no fidelity figure moved | With old and new code on the same renderer, CallReady `03-live-call` 0.0064% both ways and FundiDesk's twelve figures identical; both examples' suites pass |
| **ADR 0021 on a device** (how glides, fades and screen slides look and feel; frame times with two screens in transition; the bounded field with a real keyboard and composition), and **"reduce motion" read from the platform** | **Not verified; the hosts do not report the setting** | No device was used; `Ui::set_reduce_motion` is called by nothing but tests |
| **Launcher icon, name and bundle id on iOS** (the generated asset catalog and `.xcconfig`, `project.yml`'s use of them, `tools/build-ios.sh` with `MORNINGSTAR_APP`) | **Not verified, never compiled** | No Xcode |
| Anti-aliased edges, translucent colours and shadows versus browser rendering | Closed by ADR 0023 | Shadows within one level of Chrome's over the whole fade (FundiDesk onboarding); a 25% stroke over black is Chrome's byte |
| Text width rounding; top-bar title 1.5 px low; kerning with spaces; ligatures under letter spacing | Closed by ADR 0022 | The examples' corrections for them are deleted. "Break after hyphen" was not a defect (tested) |
| **Dashed border phase** | Mostly closed; known difference | Starts where Chrome's does and matches on the left and bottom edges; 1 to 2 px off on the right and top (ADR 0022) |
| **Turned text** | Known difference | A resampled upright bitmap, softer than Chrome's; most of what is left on CallReady's welcome and report screens (ADR 0017). Not improved |
| **Scrolling, taps and navigation in the showcase on a device** | **Not verified** | No input was sent to the phone |
| **GLES; frame timings with the new shader; memory with images** | **Not verified** | Not measured |
| **Status-bar icon colour** | Known defect | White icons on the light showcase background; the host does not set light-status-bar appearance |
| The example renders the intended picture | Verified on macOS/Metal only | `cargo run -p hello-gpu --example screenshot`, inspected by eye |
| Text shaping, wrapping, alignment, Arabic joining and right-to-left layout | Verified in tests | `crates/morningstar-text/tests/layout.rs`, with Noto Sans fonts |
| Text renders on a real phone and matches the desktop render | Verified on one device, by eye | Galaxy A27 screenshot against the Metal screenshot |
| Touch logic of the example (tap, slide-off, cancel, multi-finger) | Verified in unit tests | `examples/hello-gpu` |
| The example cross-compiles for Android (arm64, x86_64) | Verified | `cargo ndk build`, NDK 27.1 |
| The Android library exports all eleven JNI entry points | Verified when there were eleven; there are nineteen since ADR 0019, not re-checked | `llvm-nm -D libhello_gpu.so` |
| The Android host packages into an installable APK | Built, not installed | `tools/build-android.sh`, AGP 9.5.1 / Gradle 9.6.0 |
| A frame is presented on an Android device | Verified on one device | Galaxy A27, Android 16, Vulkan on Adreno 710, 120 Hz; `benchmarks/reports/2026-11-11-galaxy-a27.md` |
| Surface loss and recreation on Android (background and return) | Verified on one device | Same session: surface recreated in the same process |
| Touch reaches the app on Android | Verified on one device | Taps incremented the tally |
| **Touch reaches the app on iOS** (Swift forwarding) | **Not verified** | No Xcode |
| Vulkan output matches Metal | Checked by eye only | Device screenshot compared with the desktop render; the pixel tests have not been run on a device, and GLES is untested |
| Rotation, multi-window, device loss on Android | **Not verified** | Not exercised |
| The example cross-compiles for iOS (device, simulator) as a static library | Verified before text was added; since then only type-checked for iOS | `cargo rustc --crate-type staticlib`; all eight C symbols present |
| The Swift host and XcodeGen project build | **Not verified** | The machine had no full Xcode |
| **A frame is presented on an iOS device** | **Not verified** | Same |

Deliverable 02 (a GPU frame on both platforms) is **met for Android and not for iOS**. The iOS half needs a machine with full Xcode.
