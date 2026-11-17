# morningstar-cli

The `morningstar` command, for [Morningstar](https://crates.io/crates/morningstar-framework)
apps anywhere on disk.

```sh
cargo install morningstar-cli
morningstar new my-app --name "My App" --id com.example.myapp
cd my-app
morningstar preview                             # live in a window on a Mac
morningstar build android --abis arm64-v8a --install
```

| Command | Does |
| --- | --- |
| `new <package>` | Creates an app that builds, passes its tests and installs as it is |
| `preview` | Runs the app of this directory in a window, with the mouse as a finger, and reloads it when its source changes (macOS) |
| `build android` | Builds an APK with the Android host carried inside this command; `--install` puts it on the connected phone |
| `icon`, `diff`, `svg` | The launcher icon in every form, a rendered screen against its design, an SVG as an app draws it |

`build android` needs the Android SDK and NDK (`ANDROID_HOME`), `cargo-ndk`
and a JDK 17 or newer. The command and the app's framework must be the
same release, and the build refuses a mismatch with instructions to fix it.

MIT OR Apache-2.0.
