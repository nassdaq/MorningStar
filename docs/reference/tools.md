# Tools and scripts

Everything is run from the root of the repository, except the `morningstar` command, which is for apps outside it. The help shown for each tool is the tool's own output, captured when the site was last rebuilt.

| Tool | Does |
| --- | --- |
| [`morningstar`](#the-morningstar-command) | The released command, for an app outside this repository: create, preview, build for Android |
| [`morningstar-new`](#morningstar-new) | Creates a new app |
| [`tools/preview.sh`](#the-live-preview) | Runs an app in a window on a Mac and reloads it when the source changes |
| An app's [`screenshot` example](#screenshots) | Renders the app's screens to PNG files with no phone |
| [`morningstar-diff`](#morningstar-diff) | Compares a screenshot with a design's picture |
| [`morningstar-svg`](#morningstar-svg) | Renders an SVG file as an app would draw it |
| [`morningstar-icon`](#morningstar-icon) | Generates launcher icons and prints an app's identity |
| [`tools/build-android.sh`](#toolsbuild-androidsh) | Builds, packages and installs an Android APK |
| [`tools/build-ios.sh`](#toolsbuild-iossh) | Builds an XCFramework for the iOS host. Never verified |
| [`tools/adb-guarded.sh`](#toolsadb-guardedsh) | The only permitted way to send input to a connected phone |
| [`tools/build-docs.py`](#the-documentation) | Builds this site |
| [`morningstar-bench`](#benchmarks) | Measures frame costs on the development machine |

## The morningstar command

For an app outside this repository: `cargo install morningstar-cli` (not yet published), then run it in the app's directory. It runs the same code as the tools below, and carries the Android host of its own release. [ADR 0045](../architecture/0045-crates-io-release-and-the-morningstar-command.md).

{{include docs/reference/help/morningstar.txt}}

`morningstar new --help`:

{{include docs/reference/help/morningstar-cli-new.txt}}

`morningstar build android --help`:

{{include docs/reference/help/morningstar-cli-build-android.txt}}

## morningstar-new

```sh
cargo run -q -p morningstar-devtools --bin morningstar-new -- my-app --name "My App" --id com.example.myapp
```

{{include docs/reference/help/morningstar-new.txt}}

[Your first app](../getting-started.md) walks through what it writes. New apps go under `apps/`; the reason is in [ADR 0041](../architecture/0041-new-app-generator.md).

## The live preview

```sh
tools/preview.sh my-app
```

A development host for a Mac. It drives the same engine the phone hosts drive, in a phone-shaped window, with the mouse as a finger and the keyboard for text fields.

{{include docs/reference/help/morningstar-preview.txt}}

| Key | Does |
| --- | --- |
| Typing, arrows, Cmd+A/C/X/V/Z | Go to the focused field |
| Esc | The system's Back, or "keyboard away" while a field is focused |
| Cmd+S or F2 | Save a screenshot |
| Cmd+R or F5 | Restart the app; with Shift, restart with stored values forgotten |
| F3 | Toggle reduce motion |
| F4 | Toggle the insets |
| F1 | List the keys |

What to know about it:

- **Reload is rebuild and restart.** On a source change the app is rebuilt and its process replaced. The old window stays up during a build and after a failed one, with the errors in the terminal. Nothing in memory survives; what the app stored does.
- **HTTP, stored values and the clipboard really work**, so an app talks to its server on the same machine. Recording, playback and live rooms answer `Unsupported`.
- **It is not a phone.** It is a debug build with one finger, no real soft keyboard and no system bars. Plain `http://` is not refused as it is on a phone. Frame times in it mean nothing.
- **macOS only.**

The scripted flags (`--tap`, `--type`, `--back`, `--screenshot-after`) are how it was tested: nobody has yet clicked or typed in the window by hand. [ADR 0040](../architecture/0040-development-preview.md).

## Screenshots

Every app has a small program that renders its screens through the real renderer on the development machine's GPU.

```sh
cargo run -p my-app --example screenshot -- out/
cargo run -p fundidesk --example screenshot -- out/ 07 --diff --phone
```

The designed examples take screen names, and options to compare with the design (`--diff`), to render in a phone's frame (`--phone`, `--small`, `--tall`), to render other states (`--states`, `--online`) and to print the semantics tree (`--semantics`). The header of each `examples/screenshot.rs` lists what that app's program takes.

## morningstar-diff

```sh
cargo run -q -p morningstar-devtools --bin morningstar-diff -- design.png out/home.png --out heat.png
```

{{include docs/reference/help/morningstar-diff.txt}}

[Design to code](../design-to-code.md) explains how to read its report.

## morningstar-svg

```sh
cargo run -q -p morningstar-devtools --bin morningstar-svg -- art.svg out/art.png --scale 3
```

{{include docs/reference/help/morningstar-svg.txt}}

## morningstar-icon

```sh
cargo run -q -p morningstar-devtools --bin morningstar-icon -- my-app --preview out/icon.png
```

{{include docs/reference/help/morningstar-icon.txt}}

The build scripts call it; you run it yourself to look at an icon in every form before shipping it. [ADR 0020](../architecture/0020-app-identity-and-launcher-icons.md).

## tools/build-android.sh

```sh
MORNINGSTAR_APP=my-app MORNINGSTAR_ABIS=arm64-v8a tools/build-android.sh --install
```

| Setting | Meaning | Default |
| --- | --- | --- |
| `MORNINGSTAR_APP` | The package to build | `hello-gpu` |
| `MORNINGSTAR_ABIS` | Architectures, separated by spaces | `arm64-v8a x86_64` |
| `ANDROID_HOME` | The Android SDK | Required |
| `ANDROID_NDK_HOME` | The NDK | The newest complete one under `$ANDROID_HOME/ndk` |
| `--install` | Install on the connected phone and start the app | Build only |

See [Build for Android](../guide/android.md).

## tools/build-ios.sh

```sh
MORNINGSTAR_APP=my-app tools/build-ios.sh
xcodegen generate --spec templates/ios/project.yml
```

Builds the app as an XCFramework and generates its icon and names for the iOS host. It was written on a machine without Xcode and has never been run to the end. See [iOS](../guide/ios.md).

## tools/adb-guarded.sh

```sh
tools/adb-guarded.sh check
tools/adb-guarded.sh tap 540 1200
tools/adb-guarded.sh screencap out.png
```

A phone connected over adb belongs to a person who may be using it. This script is the only way anything in the repository may send input to a phone or capture its screen. Before every single action it checks that the expected app (`MORNINGSTAR_GUARD_PACKAGE`) is in front, and refuses otherwise with exit status 3. Use it only when the phone's owner has agreed and is not using the app; prefer proving behaviour from `adb logcat -s Morningstar`.

## The documentation

```sh
python3 tools/build-docs.py            # docs/ -> site/
python3 tools/build-docs.py --check    # fail if site/ is out of date or a link is broken
tools/docs-screenshots.sh              # re-render every screenshot and tool help text
```

The pages are Markdown under `docs/`, listed in `docs/site.json`. The code samples are regions of `examples/docs-snippets`, which is compiled and linted with the workspace, and the method tables are read from the source's doc comments. The builder needs only Python 3. [ADR 0044](../architecture/0044-documentation-site.md).

## Benchmarks

```sh
cargo run --release -p morningstar-bench -- --markdown
```

Measures what building and drawing each example screen costs on the development machine. `benchmarks/android/measure.sh` is the method for a phone. See [Performance](performance.md).
