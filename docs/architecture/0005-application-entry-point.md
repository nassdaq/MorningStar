# 0005 — Application entry point

Status: Accepted (2026-11-11)

## Context

On both platforms the operating system starts the native host, not Rust's `main`. The host must end up running the developer's application type, and the developer should write one line to make that happen, with no platform `cfg`.

## Decision

The **application crate is the native library**: a `cdylib` on Android (`libmy_app.so`) and a `staticlib` on iOS. It depends only on `morningstar`.

`morningstar::app!(expr)` expands to an exported Rust function, `__morningstar_create_app() -> Box<dyn App>`. The platform crates declare that symbol as `extern "Rust"` and call it once when the host starts. The platform crates' own entry points (`Java_…`, `morningstar_ios_*`) are `#[no_mangle]` and are carried into the application library because `morningstar` re-exports the platform crate for the current target.

```rust
use morningstar::prelude::*;

struct HelloGpu;
impl App for HelloGpu {
    fn frame(&mut self, info: &FrameInfo, frame: &mut Frame) { /* paint frame.list */ }
    fn pointer(&mut self, event: &PointerEvent) { /* optional */ }
}

morningstar::app!(HelloGpu);
```

## Consequences

- One source builds for both platforms; verified by symbol inspection of both artifacts.
- Forgetting `app!` is a link error, not a runtime failure. Using it twice is a duplicate-symbol error.
- The roadmap's `fn main() { Morningstar::new().run::<Counter>() }` is not how mobile apps start. A `main`-style entry can be offered later for a desktop development host; on mobile it would be decoration.
- `extern "Rust"` has no stable ABI, which is fine here: the app and the framework are always compiled together by the same compiler.
- **The `App` trait is a low-level interface.** The app paints a display list by hand and receives raw pointer events. The declarative UI engine (ADR 0006) will be built on top of it, as an `App` implementation supplied by Morningstar.
