# Getting started: from nothing to an app on a phone

For a developer or an AI agent starting a new Morningstar app. It takes one command to have an app that builds, passes its tests and can be installed; the rest of this page is what to do with it. The decision behind the command is [ADR 0041](architecture/0041-new-app-generator.md).

This page works inside the Morningstar repository, and everything is run from the repository root. For an app anywhere else, see [Outside this repository](#outside-this-repository) at the end: the steps are the same, but the commands change.

## What you need

| For | You need |
| --- | --- |
| Writing and testing the app | Rust (stable; the version is in the root `Cargo.toml`) and a GPU the tests can draw on. A Mac has one |
| The live preview | A Mac |
| An Android phone | The Android SDK and NDK (`ANDROID_HOME`), `cargo-ndk`, a JDK 17 or newer, and for `--install` a phone with USB debugging on |
| The optional server | Node 20 or newer |

iOS is not ready: the code is shared, but the iOS host has never been built.

## 1. Create the app

```sh
cargo run -q -p morningstar-devtools --bin morningstar-new -- \
    my-app --name "My App" --id com.example.myapp
```

| Argument | Means | If left out |
| --- | --- | --- |
| `my-app` | The Cargo package and the app's directory: lower-case letters, digits, hyphens | Required |
| `--name` | The name under the launcher icon | The package in words: "My App" |
| `--id` | The application id. It cannot be changed once people have the app | `dev.morningstar.my_app` |
| `--services live,live-video,live-screen,live-background` | Platform services the app's host is built with (voice rooms, the camera in them, sharing the screen, going on behind another app). Each adds to what the app carries; `live` is about 20 MB | None |
| `--server` | Also write a Node server in `server/`, and make the app talk to it | No server |
| `--port` | That server's port | 8790 |
| `--dir` | Where the app's directory is made | `apps` |

It writes `apps/my-app/`, adds it to the workspace, and prints the next commands. It never overwrites: a name that is taken, or a directory that exists, is refused with the reason.

What you have now is in `apps/my-app/README.md`. In short: `src/lib.rs` is the whole app (two screens, navigation, a name kept on the phone), `tests/` drives it with no device, and `Cargo.toml` is the only place its name, id, icon and services are declared.

## 2. Run its tests

```sh
cargo test -p my-app
```

They pass. The first run also writes `apps/my-app/tests/references/home.png` and `settings.png`: pictures of the two screens, which later runs compare with. Look at them and commit them.

What the tests show is how every later test is written. `tests/e2e.rs` taps, types, and reads the app's state; nothing in it needs a phone, because the app asks for work as data and the test answers (`Ui::testing`).

## 3. Look at it

```sh
cargo run -p my-app --example screenshot -- out/     # out/home.png, out/settings.png
tools/preview.sh my-app                              # the app live in a window; it reloads when you save
```

The screenshot is one frame through the real renderer. The preview is the app running, with the mouse as a finger (ADR 0040). Use both: after changing anything visual, look at it. Passing tests do not show that a screen looks right.

## 4. Change it

Open `apps/my-app/src/lib.rs`. The loop is always the same:

1. **State**: add a field to `Model`.
2. **Message**: add what can happen to `Message`.
3. **`view`**: draw the state. It changes nothing.
4. **`update`**: change the state for the message. It draws nothing and does no I/O.
5. **`effects`**: for work that takes time (a request, a stored value, sound, a live room), say what to start; the answer arrives as a message.
6. **Test**: add the journey to `tests/e2e.rs`, then look at the screen.

For a new screen: add it to `Screen`, draw it in `view`, open it with `Message::Open`, and list it in `SCREENS.md`, in the two loops of `tests/` and in `examples/screenshot.rs`.

`docs/building-screens.md` is the reference for widgets and layout. When a visual change is one you meant, accept it:

```sh
MORNINGSTAR_UPDATE_REFERENCES=1 cargo test -p my-app
```

## 5. Follow a design

Put the design's pictures in `apps/my-app/design/reference/` and hold the screens to them; `apps/my-app/design/README.md` has the commands, `docs/design-to-code.md` the method, and `examples/fundidesk` (twelve screens) and `examples/barua-talk` (five) show it carried through with a measured difference per screen.

## 6. Put it on a phone

Connect the phone, then:

```sh
MORNINGSTAR_APP=my-app MORNINGSTAR_ABIS=arm64-v8a tools/build-android.sh --install
```

That builds the app's library for the phone, generates the launcher icons from `assets/icon.png`, packages the APK and installs and starts it. Without `--install` it only builds; the APK's path is printed. `MORNINGSTAR_ABIS=arm64-v8a` builds for phones only, which halves the time and the disk space.

The icon is a placeholder in the app's colour. Replace `assets/icon.png` with a square PNG of 1024 x 1024 and look at every form it will take:

```sh
cargo run -q -p morningstar-devtools --bin morningstar-icon -- my-app --preview out/icon.png
```

A phone belongs to a person. Read `tools/adb-guarded.sh` and the rule about it in `AGENTS.md` before sending it any input.

## 7. If the app has a server

```sh
cd apps/my-app/server
npm test                 # the server through real HTTP, no dependencies to install
npm start                # 127.0.0.1:8790
```

In another terminal, let the phone reach it over USB (this is undone when the cable is unplugged):

```sh
adb reverse tcp:8790 tcp:8790
```

`server/API.md` is the contract between the app and the server: change it, `server/src/app.js` and `src/lib.rs` together. Secrets go in `server/.env` only, which git ignores; `server/.env.example` names the settings and holds no value. For a deployed server, give its address when building, so that it is never in the repository:

```sh
MY_APP_SERVER_URL=https://api.example.com MORNINGSTAR_APP=my-app tools/build-android.sh
```

## 8. Before calling it done

`AGENTS.md` defines done. For an app that means: the tests pass, the screens have been looked at, `SCREENS.md` says what every control does and what is not done, and what has and has not been seen on a real phone is written down.

## Removing an app

Delete its directory and its line under `members` in the root `Cargo.toml`.

## Outside this repository

Morningstar is released on crates.io ([ADR 0045](architecture/0045-crates-io-release-and-the-morningstar-command.md)). The `morningstar` command does there what this repository's scripts do here:

```sh
cargo install morningstar-cli
morningstar new my-app --name "My App" --id com.example.myapp     # ./my-app, a project of its own
cd my-app
cargo test                                                       # step 2
cargo run --example screenshot -- out/                           # step 3
morningstar preview                                              # step 3, on a Mac
morningstar build android --abis arm64-v8a --install             # step 6
```

`morningstar new` takes the same options as `morningstar-new`, except `--dir`, which here names the parent directory (default: the current one). The app depends on one release, `morningstar = { package = "morningstar-framework", version = "=0.1.0" }`. The crate is `morningstar-framework` on crates.io because `morningstar` belongs to another project there, but its library is still `morningstar`. The command carries the Android host of its own release and refuses to build an app on another release. Keep the two the same.

The release has not been published yet. Until it is, this section describes what the command will do, not what `cargo install` can fetch today.
