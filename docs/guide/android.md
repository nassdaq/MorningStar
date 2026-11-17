# Build for Android

One script builds the app's library, generates its launcher icons, packages an APK and, if asked, installs and starts it.

```sh
MORNINGSTAR_APP=my-app MORNINGSTAR_ABIS=arm64-v8a tools/build-android.sh --install
```

| Setting | Meaning | Default |
| --- | --- | --- |
| `MORNINGSTAR_APP` | The package to build | `hello-gpu` |
| `MORNINGSTAR_ABIS` | The architectures. `arm64-v8a` alone is right for a phone and halves the time and disk | `arm64-v8a` and `x86_64` |
| `ANDROID_HOME` | The Android SDK | Required |
| `ANDROID_NDK_HOME` | The NDK to use | The newest complete one under `$ANDROID_HOME/ndk` |
| `--install` | Also install on the connected phone and start the app | Build only; the APK's path is printed |

The app's name, application id, icon and services come from its `Cargo.toml` and nowhere else. The APK is a debug build of the host with a release build of the app's library.

## What the build needs

The Android SDK and NDK, `cargo-ndk` and a JDK 17 or newer; see [Installation](installation.md). The host targets API 36 and runs on API 24 (Android 7.0) and later.

## What is in the APK

The base host carries the app's library and asks for the `INTERNET` and `RECORD_AUDIO` permissions. Everything else is opt-in through `services` in the app's `Cargo.toml`, and an app that does not name a service carries none of it. [Opt-in services](../reference/services.md) lists what each one adds.

## A server on your machine

A phone connected by USB reaches a server on the development machine through `adb reverse`:

```sh
adb reverse tcp:8790 tcp:8790
```

The link is dropped when the cable is unplugged or the phone reconnects; run it again. For a deployed server, give its address when building, so that it is never in the repository:

```sh
MY_APP_SERVER_URL=https://api.example.com MORNINGSTAR_APP=my-app tools/build-android.sh
```

## Reading what the app does

The host logs under the tag `Morningstar`:

```sh
adb logcat -s Morningstar
```

Prefer proving behaviour from the log. A phone connected over adb belongs to a person who may be using it: the repository's rule is never to send it input or capture its screen directly. The only permitted route is `tools/adb-guarded.sh`, which checks that the expected app is in front immediately before each action, and even that only when the owner has agreed.

## What has been verified on a phone

One phone so far, a Galaxy A27 on Android 16 with Vulkan: a frame is presented, text and the full shader match the desktop render by eye, touch reaches the app, the surface survives going to the background and back, an idle screen draws nothing, and typing works with the system keyboard. [Platform status](../reference/platform-status.md) has the full list, and what has not been seen.

## Type-checking the Android code without building an APK

```sh
cargo ndk -t arm64-v8a clippy -p my-app
```
