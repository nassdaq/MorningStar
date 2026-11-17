# Troubleshooting

Problems that have actually come up, and what fixed them.

## The Android build cannot find a complete NDK

**Seen as:** Gradle prints `NDK at .../ndk/28.2.13676358 did not have a source.properties file`, or `cargo ndk` fails to find a compiler.

A partially downloaded NDK has no `source.properties`. `tools/build-android.sh` already picks the newest *complete* one. When calling `cargo ndk` yourself, point it at a complete NDK:

```sh
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.1.12297006"
cargo ndk -t arm64-v8a clippy -p my-app
```

## The disk fills up

A debug build of every package, the Android cross-build and Gradle's output together take many gigabytes. When a build fails with "No space left on device", or with stranger errors such as "multiple versions of crate `morningstar_ui`", free space and rebuild.

Everything under `target/` and `templates/android/app/build` can be rebuilt and is safe to delete. To stay small:

- Build one package: `cargo test -p my-app`, not `--workspace`.
- Build Android for one architecture: `MORNINGSTAR_ABIS=arm64-v8a`.
- After a failed build on a full disk: `cargo clean -p my-app -p morningstar -p morningstar-ui`.

## The app on the phone cannot reach the server

**Seen as:** the app says the server did not answer, although the server runs on the development machine.

The phone reaches `127.0.0.1` on your machine only through `adb reverse`, and the link is dropped whenever the cable is unplugged or the phone reconnects:

```sh
adb reverse tcp:8790 tcp:8790
adb reverse --list
```

Also check that the server itself is still running: `curl http://127.0.0.1:8790/v1/health`.

## A request to `http://` is refused

The hosts refuse plain `http://` except to the development machine (`localhost`, `127.0.0.1`, `10.0.2.2`). A deployed server needs `https://`. The preview on a Mac does not refuse it, so this shows up only on a phone.

## Name lookups fail on the development machine

**Seen as:** `cargo` cannot fetch crates, or a server cannot reach its database or an API, while other machines on the same network are fine.

This was the router's DNS. Checking against another resolver showed it:

```sh
dig @1.1.1.1 crates.io
```

A server that starts while the database cannot be resolved keeps running and reconnects by itself; its health endpoint says `"database": false` until then.

## A test of a designed screen fails after a harmless change

`tests/fidelity.rs` fails when more than a measured share of pixels differs from the design. Render the screen and look at the difference before touching the limit:

```sh
cargo run -p barua-talk --example screenshot -- out/ 01 --diff --phone
```

For the app's own reference pictures (`tests/screens.rs`), accept a change you meant with `MORNINGSTAR_UPDATE_REFERENCES=1 cargo test -p my-app`.

## A box has no size, or text overflows

- A `fill_width()` child of a box that hugs its width gets no width. Give the parent a width.
- In a row, the text that should wrap must `fill_width()`.
- A `ScrollView` needs a height from its parents.

`Ui::overflows()` lists boxes whose children do not fit. See [Layout](../guide/layout.md#rules-worth-knowing).

## A text field loses focus when the screen changes

Give every `TextInput` an `.id(..)`. Without one its identity is its position in the tree, which changes when something appears above it.

## The preview window shows the old app

A build failed: the old window stays up and the errors are in the terminal that started the preview. Fix the error and save again.

## Long test runs stop when the Mac sleeps

Run them under `caffeinate`:

```sh
caffeinate -i cargo test --workspace
```
