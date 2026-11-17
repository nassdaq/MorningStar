# Morningstar

A Rust-native, GPU-first framework for mobile apps. You write one Rust
codebase. Morningstar lays it out, draws it on the GPU with its own
renderer, and runs it on Android. iOS shares the same code, but its host
has not been built yet.

On crates.io the crate is `morningstar-framework`, because the name
`morningstar` belongs to another project there. Its library is still
called `morningstar`, so app code writes `use morningstar::ui::prelude::*;`.

## Start an app

```sh
cargo install morningstar-cli
morningstar new my-app --name "My App" --id com.example.myapp
cd my-app
cargo test                                     # the app driven with no device
cargo run --example screenshot -- out/         # its screens as PNGs
morningstar preview                            # live in a window on a Mac; reloads on save
morningstar build android --abis arm64-v8a --install   # onto a phone
```

The generated app depends on one release of the framework:

```toml
[dependencies]
morningstar = { package = "morningstar-framework", version = "=0.1.0" }
```

The framework's crates, the `morningstar` command and the Android host it
builds into are released together under one version. Keep the app's
version and the command's version the same.

## What an app is

- A state struct implements `ui::Application`. `view` describes the screen
  as widgets in dp, and `update` handles one message at a time.
- Work that takes time is requested as data in `effects` and comes back as
  a message: HTTP, stored values, sound, recording, live rooms with voice,
  video, screen sharing and data. Tests answer those requests with
  `Ui::testing`, so they need no phone.
- The app's name, id, icon and opt-in platform services are declared once,
  in its `Cargo.toml` under `[package.metadata.morningstar]`.

## Status

0.1.0 is the first release. On Android it runs on real phones, and the live
room's video, screen sharing and background calls have not yet been
verified on one. iOS has never been built.

## Licence

MIT OR Apache-2.0, at your option.
