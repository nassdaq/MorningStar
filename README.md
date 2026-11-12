# Morningstar

A Rust-native, GPU-first mobile application framework for Android and iOS.

One Rust codebase. A custom UI engine and renderer owned by Morningstar. Thin native hosts that developers never edit.

> **Status: early Phase 3.** The runtime, a renderer for rectangles and text, touch input and both platform integrations are written, tested on a desktop GPU, and running on Android (verified on a Galaxy A27: Vulkan, 120 Hz). The iOS host is written but has never been built or run. See [verification status](docs/architecture/README.md#verification-status) for exactly what has and has not been checked.

## What exists

- **A renderer** that draws rounded rectangles, borders, gradients, soft shadows, clipped regions and text in one GPU draw call, on Vulkan, Metal or GLES.
- **Text** with proper shaping, word wrapping, alignment, font families and weights, mixed styles in one sentence, letter spacing, and right-to-left scripts such as Arabic, using fonts the app bundles.
- **Vector icons** from SVG path data, crisp at any size and colour.
- **Text input** with the system keyboard on Android: focus, caret, keyboard types, Next/Done, keyboard avoidance.
- **Selection and the clipboard** in every text field, drawn by the framework: double tap and long press, two handles, a Cut · Copy · Paste · Select all menu, caret keys and shortcuts of a hardware keyboard, undo. Host-tested; not yet tried on a device.
- **System bars and motion follow the app and the user**: light or dark status and navigation icons from the theme (or per screen), and animations off when the system's "reduce motion" is on. Host-tested; not yet tried on a device.
- **Screen readers can read and operate it**: every frame can be described as a tree of semantic nodes (buttons, text, fields, lists, with labels, values, states and actions) derived from the widgets, mirrored to TalkBack as a virtual view hierarchy and to VoiceOver as accessibility elements, built only while one is running. `ui.semantics()`, `ui.perform(..)` and `ui.accessibility_issues()` make it testable. Host-tested ([ADR 0032](docs/architecture/0032-accessibility.md)); not yet tried with TalkBack, and the iOS half is uncompiled.
- **A runtime** that handles app lifecycle, surface loss and recreation, frame pacing and frame statistics.
- **Touch input** delivered from both platforms to shared Rust code.
- **Thin hosts** for Android (Java) and iOS (Swift) that contain no application logic.
- **A layout engine** (rows, columns, stacks, padding, alignment, fixed / hug / fill sizing).
- **Density-independent units, safe-area insets and demand-driven rendering**: an idle screen draws nothing.
- **A screenshot tool** that renders any app to a PNG on your development machine, with no phone, and a **diff tool** that compares it with a design image and reports where they differ.

- **Images** from PNG or raw pixels, with cropping, fit modes, rounded corners and tint.
- **A declarative UI layer**: columns, rows, stacks, text, buttons, scroll views, themes, with app state and messages.

A screen is a function of state:

```rust
use morningstar::ui::prelude::*;

#[derive(Default)]
struct Counter { count: u32 }

#[derive(Clone)]
enum Msg { Increment }

impl Application for Counter {
    type Message = Msg;

    fn setup(&mut self, setup: &mut Setup<'_>) {
        setup.add_font(include_bytes!("../assets/MyFont.ttf"));
    }

    fn update(&mut self, message: Msg) {
        match message {
            Msg::Increment => self.count += 1,
        }
    }

    fn view(&self) -> Element<Msg> {
        col![
            text(format!("Count: {}", self.count)).title(),
            Button::new("Increment").on_press(Msg::Increment),
        ]
        .gap(16.0)
        .center()
        .fill()
        .into()
    }
}

morningstar::app!(Ui::new(Counter::default()));
```

Sizes are in density-independent units. [`examples/showcase`](examples/showcase/src/lib.rs) has three realistic screens (a wallet home, settings, a login form) written this way; [`docs/building-screens.md`](docs/building-screens.md) is the guide. The snippet above is illustrative: check it against the showcase before copying.

[`examples/fundidesk`](examples/fundidesk) is the proof that this works on a real design: twelve screens of a repair-shop app reproduced from the designer's export, each within half a percent of the design's pixels and held there by a test. [`examples/callready`](examples/callready) does the same for a second, illustration-heavy design (turned cut-out letters, tilted cards, a vinyl disc drawn from layered vector shapes): five screens, each within a quarter of a percent.

The low-level interface underneath (paint a display list by hand, receive raw touches) remains available; [`examples/hello-gpu`](examples/hello-gpu/src/lib.rs) uses it.

### See it without a phone

```sh
cargo run -p showcase --example screenshot out/ 3
```

writes one PNG per showcase screen, rendered by the real renderer on your GPU. Compare one with a design using `cargo run -q -p morningstar-devtools --bin morningstar-diff -- design.png out/home.png --out heat.png`. This is also the basis for building UI from a design image ([ADR 0008](docs/architecture/0008-design-to-code-loop.md)).

## Layout

```
crates/
  morningstar/            public API; the only crate an app depends on
  morningstar-core/       colour, lifecycle state machine, frame statistics (no dependencies)
  morningstar-gpu/        device, surface and frame submission on wgpu
  morningstar-text/       fonts, shaping, layout and glyph atlas on cosmic-text
  morningstar-layout/     layout engine
  morningstar-ui/         declarative widgets, state and events
  morningstar-runtime/    Engine: lifecycle in, frames out
  morningstar-android/    JNI entry points and render thread
  morningstar-ios/        C ABI for the UIKit host
templates/
  android/                Gradle host project (one Activity, one SurfaceView)
  ios/                    XcodeGen host project (one view controller, one CAMetalLayer)
examples/hello-gpu/       low-level example (a tap counter)
examples/showcase/        three realistic screens built with the UI layer
examples/services-check/  device smoke test for HTTP, recording, playback and stored values
docs/architecture/        architecture decision records
benchmarks/               method, scripts and reports
tools/
  morningstar-devtools/    render an app to a PNG, no device needed; launcher icons
  build-android.sh, build-ios.sh
```

## Build

Requirements: Rust (stable, 1.87+).

```sh
cargo test --workspace      # includes pixel tests on this machine's GPU
```

### Name and icon

An app says what it is called and what its home-screen icon is in its own `Cargo.toml`; both builds make everything the platform needs from that ([ADR 0020](docs/architecture/0020-app-identity-and-launcher-icons.md)):

```toml
[package.metadata.morningstar]
name = "CallReady"                 # under the icon (default: the package name)
id = "dev.morningstar.callready"   # application id / bundle id (default: dev.morningstar.<lib>)
icon = "assets/icon.png"           # square PNG, 512+, best 1024, full-bleed (default: this path if it exists)
icon_foreground = "assets/icon-foreground.png"   # optional Android adaptive foreground, content in the middle 61%
icon_background = "#0E0E0E"        # optional (default: the icon's top-left pixel)
```

Every key is optional. With no icon the app gets the built-in Morningstar star. With only `icon`, Android's adaptive icon is that picture in the 72 dp a launcher shows, so a round mask cuts its corners: keep what matters in the middle. Look before installing:

```sh
cargo run -q -p morningstar-devtools --bin morningstar-icon -- callready --preview out/icon.png
```

### Android

Requirements: Android SDK with a complete NDK (`ANDROID_HOME` set), [`cargo-ndk`](https://github.com/bbqsrc/cargo-ndk), JDK 17+, and `rustup target add aarch64-linux-android x86_64-linux-android`.

```sh
tools/build-android.sh --install     # build, install and launch on the connected device
adb logcat -s Morningstar            # GPU in use, then frame statistics every 600 frames
```

### iOS

Requirements: full Xcode and [XcodeGen](https://github.com/yonaskolb/XcodeGen).

```sh
tools/build-ios.sh                   # MORNINGSTAR_APP=callready for another example
xcodegen generate --spec templates/ios/project.yml
open templates/ios/MorningstarHost.xcodeproj     # select a device, set a signing team, run
```

## Next steps

1. **Run it on iOS.** The Swift host has never been compiled.
2. **Try text selection on a phone.** Selection handles, the Cut · Copy · Paste · Select all menu, the clipboard, caret keys and undo are built and tested on the host ([ADR 0025](docs/architecture/0025-text-selection-and-clipboard.md)), as are light or dark system-bar icons and the platform's "reduce motion" setting ([ADR 0026](docs/architecture/0026-system-bars-and-reduce-motion.md)); none of it has been touched by a finger yet. Then composition on iOS, a magnifier, and translated menu labels.
3. **Match browser anti-aliasing and shadows**, the main remaining gap between a render and a design.
4. **Measure on the phone again** with the full shader and images: frame times, memory, and scrolling by hand.
5. **Accessibility tree, animation API, list virtualisation, right-to-left layout.**

## Contributing

Read [`AGENTS.md`](AGENTS.md) (the engineering constitution) and the [ADRs](docs/architecture/README.md) first.
