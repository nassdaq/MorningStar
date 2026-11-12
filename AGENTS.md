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
| `crates/morningstar` | Public developer API. The only crate an app depends on. |
| `crates/morningstar-core` | Dependency-free types: `Color`, `Lifecycle`, `FrameStats`, and the data of platform services (`HttpRequest`, `Recording`, `ServiceRequest`, ...). |
| `crates/morningstar-gpu` | The only crate that uses `wgpu`. No `wgpu` type appears in its public API. |
| `crates/morningstar-text` | Fonts and families, shaping, layout cache, glyph atlas, vector icons and multi-colour vector pictures. The only crate that uses `cosmic-text` and `zeno`. |
| `crates/morningstar-layout` | Layout engine: fixed / hug / fill boxes to rectangles. Depends only on core. |
| `crates/morningstar-ui` | Declarative widgets, Elm-style state and events, themes, and `Effects` (HTTP, audio, stored values as messages) with its test driver. Apps use it as `morningstar::ui`. |
| `crates/morningstar-runtime` | `Engine`: lifecycle in, frames out, and platform service requests out and answers in. The one object a host drives. |
| `crates/morningstar-android` | JNI entry points, the Rust-to-Java upcalls (`java.rs`) and the render thread. Empty off Android. |
| `crates/morningstar-ios` | C ABI for the UIKit host. Empty off iOS. |
| `templates/android`, `templates/ios` | Thin native host projects. No application logic. They implement the platform services (`MorningstarServices.java`, `HostServices.swift`). |
| `tools/morningstar-devtools` | Renders an app to a PNG with no device, and compares it with a design image (`morningstar-diff`). Generates an app's launcher icons and reads its name and id from its `Cargo.toml` (`morningstar-icon`, ADR 0020). |
| `examples/hello-gpu` | The low-level example: paints and hit-tests by hand. |
| `examples/showcase` | Three realistic screens written with the UI layer. |
| `examples/services-check` | Device smoke test for platform services: one button per service. Also the smallest app that uses every effect, with scripted tests. |
| `examples/fundidesk` | Twelve screens reproduced from a real design, each tested against its reference image. The model to imitate; start with its `SCREENS.md`. |
| `examples/callready` | Five illustrated screens from a second design: turned elements, multi-colour vector pictures, text effects. Tested against its references. |
| `docs/architecture` | Architecture Decision Records. Read before changing a subsystem. |
| `benchmarks` | Benchmark method and reports. |

Dependency direction is one-way: `core` ← `gpu`, `text`, `layout` ← `runtime` ← `ui` ← platform crates ← `morningstar` ← apps. `core`, `gpu`, `text` and `runtime` must never depend on a platform crate or contain `cfg(target_os)` beyond the `NativeSurface` handle type.

### Commands

```sh
cargo test --workspace                 # host tests, including pixel tests on the real GPU
cargo run -p hello-gpu --example screenshot out/   # render the app to PNGs; look at them
cargo run -q -p morningstar-devtools --bin morningstar-diff -- design.png out/idle.png --out heat.png
cargo clippy --workspace --all-targets
cargo ndk -t arm64-v8a clippy -p hello-gpu   # type-check and lint the Android code
tools/build-android.sh [--install]     # APK (MORNINGSTAR_APP=showcase, MORNINGSTAR_ABIS=arm64-v8a)
cargo run -p showcase --example screenshot out/ 3   # the showcase screens as PNGs at scale 3
cargo run -p fundidesk --example screenshot -- out/ 07 --diff --phone   # one FundiDesk screen, compared with its design
tools/build-ios.sh                     # XCFramework, icon and names for the example (MORNINGSTAR_APP; needs full Xcode)
cargo run -q -p morningstar-devtools --bin morningstar-icon -- callready --preview out/icon.png   # the app's launcher icon in every form; look at it
cargo run -p callready --example icon  # redraw an app's source icon (assets/icon.png) from its own art
```

### Building a screen from a design

Read `docs/building-screens.md` and `docs/design-to-code.md`, imitate `examples/showcase`, render with the screenshot example, compare with `morningstar-diff`, and iterate.

### Rules specific to this codebase

- The Java/Swift hosts and their Rust counterparts are two halves of one interface. `MorningstarNative.java` ↔ `morningstar-android/src/jni.rs` and `java.rs`, and `morningstar_ios.h` ↔ `morningstar-ios/src/lib.rs`, must change together. An app library and a host from different sides of such a change must never be combined: rebuild both.
- An application never does I/O itself: no sockets, files, threads or sleeps in `update` or `view`. Work that takes time goes through `Application::effects` (ADR 0019), and tests answer it with `Ui::testing`.
- Record a new ADR in `docs/architecture` when a decision there is changed or a new cross-cutting one is made. Do not edit an accepted ADR's decision; supersede it.
- Dependency versions move quickly (wgpu's API changed materially between major versions). Check the version in `Cargo.lock` and its docs rather than relying on memory.
- An app's name, id and launcher icon are declared in its `Cargo.toml` under `[package.metadata.morningstar]` and nowhere else. The hosts' icon resources are generated by the build scripts into `build/` directories: never add an icon to `templates/`, and never special-case an app in a script.
- After changing anything visual, render a screenshot and look at it. Passing tests do not show that a screen looks right.
- The offscreen pixel tests are the specification of the renderer. A new drawing operation comes with pixel tests.
- A phone connected over adb belongs to a person who may be using it. Never call `adb shell input`, monkey or `screencap` directly. The only permitted route is `tools/adb-guarded.sh`, which checks the foreground app immediately before each command, and even then only when the owner has agreed and is not using the app: a guarded tap has still landed while the owner was mid-typing. Prefer proving behaviour from logs.
- Disk space on the main development machine is tight: prefer `-p` builds, avoid release and cross builds you do not need.
- State what was and was not verified on a real device. `docs/architecture/README.md` tracks verification status.
