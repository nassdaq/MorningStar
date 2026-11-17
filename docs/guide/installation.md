# Installation

Morningstar is not installed as a package. You work in a clone of its repository, and an app is a crate in that workspace. This page lists what the machine needs.

## Before you begin

You only need the tools for what you plan to do.

| Tool | Version | Needed for |
| --- | --- | --- |
| Rust | Stable, 1.87 or newer. CI uses 1.92.0 | Every app |
| A Mac | Any with a GPU that Metal runs on | Running the tests that draw, screenshots and the live preview |
| Android SDK and NDK | NDK 27.1 is the one in use | Android builds |
| `cargo-ndk` | CI uses 4.1.2 | Android builds |
| JDK | 17 or newer | Android builds |
| Node | 20 or newer for a generated server; 22 or newer for the example apps' servers | An app's optional server |
| Xcode and XcodeGen | Full Xcode | iOS builds, which have never been tried: see [iOS](ios.md) |

The tests that compare pixels have only been run on macOS with Metal. CI's Linux job runs the checks that need no GPU.

## Set up

{{steps}}
1. **Get the repository and check Rust.**

   Clone the repository, then confirm the toolchain from its root:

   ```sh
   rustc --version          # 1.87 or newer
   cargo test -p morningstar-layout   # a quick crate with no GPU in it
   ```

2. **Add the Android tools.**

   Install the Android SDK and an NDK (for example with Android Studio's SDK manager), then:

   ```sh
   export ANDROID_HOME="$HOME/Library/Android/sdk"
   rustup target add aarch64-linux-android x86_64-linux-android
   cargo install cargo-ndk
   ```

   `tools/build-android.sh` finds the newest complete NDK under `$ANDROID_HOME/ndk` by itself. Set `ANDROID_NDK_HOME` to choose one.

3. **Create and run an app.**

   ```sh
   cargo run -q -p morningstar-devtools --bin morningstar-new -- my-app
   cargo test -p my-app
   tools/preview.sh my-app
   ```

   [Your first app](../getting-started.md) continues from here, up to installing it on a phone.
{{/steps}}

> **Building on Linux or Windows?** Nothing in the shared crates is specific to macOS, and CI lints the whole workspace on Linux. But the pixel tests, the screenshot programs and the preview have only been run on a Mac, and the scripts under `tools/` are written for a Unix shell.

## Disk space

A full debug build of the workspace with every example takes several gigabytes under `target/`. Build one package at a time (`-p my-app`) and build Android for one architecture (`MORNINGSTAR_ABIS=arm64-v8a`) to keep it small. [Troubleshooting](../reference/troubleshooting.md#the-disk-fills-up) says what is safe to delete.
