# Project layout

Two layouts matter: an app's own directory, and the repository around it.

## An app

This is what `morningstar-new` writes. The example apps have the same shape.

| Path | What it is |
| --- | --- |
| `Cargo.toml` | The crate, and under `[package.metadata.morningstar]` the app's name, id, icon and opt-in services. They are declared here and nowhere else |
| `src/lib.rs` | The application: state, messages, `view`, `update`, `effects`, and the `morningstar::app!` line |
| `tests/e2e.rs` | Journeys through the app: taps, typing and answers, with no device |
| `tests/accessibility.rs` | Every screen checked for missing labels, small touch targets and low contrast |
| `tests/screens.rs` | A picture of each screen compared with its reference in `tests/references/` |
| `tests/support/mod.rs` | The small "device" the tests share: a frame, a finger, a keyboard |
| `examples/screenshot.rs` | Renders every screen to a PNG on the development machine |
| `assets/` | Fonts, the launcher icon (`icon.png`), images and SVG files |
| `design/` | The design's reference pictures, if the app follows one |
| `SCREENS.md` | What every screen and control does, and what is not done |
| `server/` | Only with `--server`: a Node server with its contract in `API.md` |

The identity table looks like this:

```toml
[package.metadata.morningstar]
name = "Barua Talk"                  # under the launcher icon
id = "dev.morningstar.barua_talk"    # the application id
icon = "assets/icon.png"             # square PNG, 1024 is best
services = ["live", "live-video"]    # opt-in platform services
```

`services` is described in [Opt-in services](../reference/services.md), the icon in [Tools](../reference/tools.md#morningstar-icon).

## The repository

| Path | Owns |
| --- | --- |
| `crates/morningstar` | The public API. The only crate an app depends on |
| `crates/morningstar-ui` | Widgets, state and messages, themes, effects. Apps use it as `morningstar::ui` |
| `crates/morningstar-layout` | The layout engine |
| `crates/morningstar-text` | Fonts, shaping, the glyph atlas, vector icons, SVG |
| `crates/morningstar-gpu` | The renderer; the only crate that uses `wgpu` |
| `crates/morningstar-runtime` | The engine a host drives: lifecycle in, frames out |
| `crates/morningstar-core` | Shared types with no dependencies |
| `crates/morningstar-android`, `crates/morningstar-ios` | The Rust half of each host |
| `templates/android`, `templates/ios` | The native host projects. No application logic |
| `tools/` | The generator, the preview, the icon and diff tools, the build scripts |
| `examples/` | The framework's own demonstration apps |
| `apps/` | Where `morningstar-new` puts a new app |
| `docs/` | These pages as Markdown, and the decision records |
| `site/` | These pages as a website, built from `docs/` |
| `benchmarks/` | The benchmark method and its reports |

[Architecture](../reference/architecture.md) explains how the crates depend on each other.
