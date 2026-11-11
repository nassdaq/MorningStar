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

## Verification status

What has actually been run, as of 2026-11-11 (after the first device session and the first parallel wave). Update this table when it changes.

| Claim | Status | Evidence |
| --- | --- | --- |
| Shared code builds and passes tests on the dev host | Verified | `cargo test --workspace`: 563 tests |
| Rectangles, rounded corners, anti-aliasing, blending and paint order produce correct pixels on a real GPU | Verified on macOS/Metal only | `crates/morningstar-gpu/tests/offscreen.rs` |
| Borders, gradients, shadows (against a CPU Gaussian reference), clips and partial atlas upload | Verified on macOS/Metal only | `crates/morningstar-gpu/tests/vocabulary.rs`, `atlas_upload.rs`; sample scene inspected by eye |
| Layout engine | Verified in tests | 77 tests in `crates/morningstar-layout`, including a hand-computed screen and a zero-allocation check |
| Screenshot-versus-design comparison tool | Verified in tests | 64 tests in `tools/morningstar-devtools` |
| Demand-driven rendering, scale and inset propagation (engine logic) | Verified in tests on the host | `crates/morningstar-runtime` |
| Scale, insets, edge-to-edge and idling on Android | Verified on one device | Galaxy A27 log: `scale 2.8125 px/dp`, `insets top=102 bottom=42 px`, then `idle 6.0s: frames=1` (one frame drawn, render thread blocked) |
| The full shader (borders, gradients, shadows, clips, images, text) on Vulkan | Verified on one device, by eye | The showcase home screen on the Galaxy A27 matches the Metal screenshot |
| Widget layer and showcase screens | Verified in tests and by eye | 57 tests in `morningstar-ui`, 6 in `examples/showcase` including visual-regression references |
| Images | Verified on macOS/Metal; one image seen on device | 59 tests; the showcase avatar on the phone |
| A real design reproduced: FundiDesk, 12 of 12 screens | Verified against the design | Each screen differs from its Chrome-rendered reference in 0.10% to 0.49% of pixels (`examples/fundidesk/tests/fidelity.rs`); all twelve inspected by eye |
| Font families, rich text, letter spacing, vector icons, dashed borders | Verified on macOS/Metal only | Tests in `morningstar-text`, `morningstar-ui`, `morningstar-gpu` |
| FundiDesk launches on Android | Verified on one device, from the log only | Galaxy A27: first frame drawn, then idle. The screen was not inspected and no input was sent |
| Text input on Android | Verified on one device from the owner's own typing, by log | Galaxy A27, Samsung Keyboard: keyboard shown on focus with the right type, key-event and input-method edits reach the field, Next/Done, keyboard inset, caret blink at 10 timed frames in 6 s |
| **Text input: composition and autocorrect, swipe typing, Back-key dismissal, field staying above the keyboard, caret and focus visuals** | **Not verified** | Need the owner's eyes and fingers |
| **Anti-aliased edges versus browser rendering** | Known difference | Text and thin strokes are lighter than Chrome's (linear-light blending); shadows render about half as dark for the same alpha |
| **Dashed border phase; text width rounding; top-bar title 1.5 px low; break after hyphen; kerning with spaces; ligatures under letter spacing** | Known differences | Found by the FundiDesk agents; each costs a fraction of a percent |
| **Scrolling, taps and navigation in the showcase on a device** | **Not verified** | No input was sent to the phone |
| **GLES; frame timings with the new shader; memory with images** | **Not verified** | Not measured |
| **Status-bar icon colour** | Known defect | White icons on the light showcase background; the host does not set light-status-bar appearance |
| The example renders the intended picture | Verified on macOS/Metal only | `cargo run -p hello-gpu --example screenshot`, inspected by eye |
| Text shaping, wrapping, alignment, Arabic joining and right-to-left layout | Verified in tests | `crates/morningstar-text/tests/layout.rs`, with Noto Sans fonts |
| Text renders on a real phone and matches the desktop render | Verified on one device, by eye | Galaxy A27 screenshot against the Metal screenshot |
| Touch logic of the example (tap, slide-off, cancel, multi-finger) | Verified in unit tests | `examples/hello-gpu` |
| The example cross-compiles for Android (arm64, x86_64) | Verified | `cargo ndk build`, NDK 27.1 |
| The Android library exports all eleven JNI entry points | Verified | `llvm-nm -D libhello_gpu.so` |
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
