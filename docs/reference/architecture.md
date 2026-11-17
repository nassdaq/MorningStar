# Architecture

Morningstar is a workspace of small crates with a one-way dependency direction. An app depends on one of them.

## The crates

| Crate | Owns |
| --- | --- |
| `morningstar` | The public API. The only crate an app depends on |
| `morningstar-core` | Types with no dependencies: colours, geometry, the display list, the data of platform services, a video's frames |
| `morningstar-layout` | The layout engine: fixed, hug and fill boxes to rectangles. Depends only on core |
| `morningstar-text` | Fonts, shaping, the layout cache, the glyph atlas, vector icons, SVG. The only crate that uses `cosmic-text` and `zeno` |
| `morningstar-gpu` | The renderer. The only crate that uses `wgpu`; no `wgpu` type appears in its public API |
| `morningstar-runtime` | The engine: lifecycle in, frames out, service requests out and answers in. The one object a host drives |
| `morningstar-ui` | Widgets, state and messages, themes, effects and their test driver. Apps use it as `morningstar::ui` |
| `morningstar-android` | JNI entry points, the Rust-to-Java upcalls and the render thread. Empty off Android |
| `morningstar-ios` | The C interface for the UIKit host. Empty off iOS |

## The direction

```text
core  <-  gpu, text, layout  <-  runtime  <-  ui  <-  android | ios  <-  morningstar  <-  your app
```

`core`, `gpu`, `text` and `runtime` never depend on a platform crate and contain no platform-specific code beyond the handle of the surface they draw on. That is why the whole of an app can be built, drawn and tested on a development machine.

## The hosts

`templates/android` and `templates/ios` are native projects with no application logic. They create the surface, forward input, and implement the platform services. The Java or Swift side and its Rust counterpart are two halves of one interface and change together; an app library and a host from different sides of such a change must never be combined.

## An app, at run time

1. The host starts and loads the app's native library.
2. `morningstar::app!` hands the host an engine holding the app's `Ui`.
3. The host forwards lifecycle, the surface, touches and the keyboard.
4. The engine produces a frame when one is needed, and nothing otherwise.
5. Requests for platform services go out to the host as data; answers come back and become messages.

[How rendering works](../guide/rendering.md) follows one frame through the crates.

## Tools around it

| Path | Owns |
| --- | --- |
| `tools/morningstar-devtools` | Screenshots without a device, the design comparison, the icon generator, the SVG renderer, the new-app generator |
| `tools/morningstar-preview` | The development host for a Mac |
| `benchmarks/host` | The host benchmark |
| `examples/docs-snippets` | The code samples of these pages |

## Decisions

Every architectural decision is recorded: what was decided, why, and what it costs. An accepted record is not edited to change its decision; a new record supersedes it. The [decision records](../architecture/README.md) page lists all of them, and tracks what has been verified and where.

The ones to read first:

- [0001: crate boundaries](../architecture/0001-crate-boundaries.md)
- [0003: thin native hosts](../architecture/0003-platform-hosts.md)
- [0013: the declarative UI layer](../architecture/0013-ui-layer.md)
- [0019: effects and platform services](../architecture/0019-effects-and-platform-services.md)
- [0012: scale, insets and demand-driven rendering](../architecture/0012-scale-insets-and-demand-rendering.md)
