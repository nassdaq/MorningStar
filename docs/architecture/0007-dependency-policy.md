# 0007 — Dependency policy

Status: Accepted (2026-11-11)

## Policy

1. A dependency is added only when writing the equivalent ourselves would be a significant, ongoing cost (a GPU abstraction, text shaping) and not for convenience (logging macros, error derive, a handful of FFI declarations).
2. A dependency that defines a subsystem's behaviour is confined to one crate and kept out of every public API.
3. Default features are off unless needed.
4. Each addition is recorded here with its reason.

## Inventory

| Crate | Version | Used by | Why |
| --- | --- | --- | --- |
| `wgpu` | 30 | `morningstar-gpu` | Vulkan, Metal and GLES behind one API (ADR 0002) |
| `pollster` | 1 | `morningstar-gpu` | Blocks on wgpu's two async setup calls; avoids an async runtime |
| `cosmic-text` | 0.17 | `morningstar-text` | Shaping, bidi, line breaking, font fallback, glyph rasterisation (ADR 0009). `fontconfig` feature off. 0.17 is what resolves on Rust 1.92; newer releases need a newer compiler. |

| `zeno` | 0.3 | `morningstar-text` | Rasterises vector icons. Already in the tree through the text stack; now a direct dependency. |
| `png` | 0.18 | `morningstar-runtime` (image decoding, linked into apps; binary-size cost not yet measured) and `morningstar-devtools` | Decoding real-world design exports (palette, 16-bit, interlaced) and compressed screenshots. Adds 8 packages to the lock file. Replaced a hand-written uncompressed encoder once decoding was needed. |

Build tooling, not linked into apps: `cargo-ndk`, Android Gradle Plugin 9.5.1 with Gradle 9.6.0 (wrapper), XcodeGen.

## Deliberately absent

- **`winit`, `android-activity`**: would own the application (ADR 0003).
- **`jni`, `ndk`, `objc2`**: the hosts need four C functions on Android and none on iOS.
- **`log`**: wgpu logs through it, so its warnings are currently invisible on devices. Adding a `log` backend that writes to logcat / the Xcode console is the most likely next dependency, and is worth it the first time a device shows a driver problem.
- **`tokio` or another async runtime**: no demonstrated need.

## Toolchain note

`rust-toolchain.toml` pins the channel but does not list targets. Listing targets makes rustup synchronise the whole channel on any cargo invocation, which silently upgrades a developer's compiler. Targets are added explicitly (`rustup target add …`; `tools/build-ios.sh` does this).
