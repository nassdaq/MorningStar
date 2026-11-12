# 0030 — Launcher icons are flattened on sRGB-encoded values

Status: Accepted (2026-11-12). Verified in tests on the development host; no icon was installed on a device.

Amends one detail of [0020](0020-app-identity-and-launcher-icons.md) (how the icon generator flattens transparency) to agree with [0023](0023-compositing-space.md). It changes nothing else of 0020.

## Context

Where the icon generator puts a translucent pixel over an opaque colour (the built-in star on its field, an adaptive foreground over its background for the legacy and round icons, a transparent source for the opaque iOS icon, the preview sheet), it blended in linear light, "as the renderer does". Since 0023 the renderer does not: it composites sRGB-encoded values, as browsers, design tools and both platforms' launchers do. An adaptive icon composed by Android from the same foreground and background is blended on encoded values; the legacy icon generated here for API 24 and 25 was not, so the two differed along every anti-aliased edge.

## Decision

`over` in `tools/morningstar-devtools/src/icon.rs` blends the encoded bytes: `(pixel * alpha + under * (255 - alpha) + 127) / 255` per channel. Half-transparent white over black is byte 128 (it was 188).

Resizing (`Image::resized`) still filters in linear light with premultiplied alpha. That is resampling, not compositing: averaging light is what makes a reduced image look like the original from further away, and 0023 leaves it alone for the same reason.

## What changed in generated files

Compared byte for byte, before and after, for the two icons this repository has:

- **CallReady** (`examples/callready/assets/icon.png` and `icon-foreground.png`): all 22 generated Android and iOS files are identical. Its icon is opaque and its foreground is handed to Android as it is. Only the `--preview` sheet differs.
- **The built-in icon** (the star, for an app that declares none): the legacy `ic_launcher.png` and `ic_launcher_round.png` at all five densities and the iOS `AppIcon.png` differ, along the star's edges. The adaptive foreground and its colour do not.

Generated icons are written per build and never checked in (0020), so nothing in the repository was regenerated.

## Verified

`the_ios_icon_is_1024_opaque_and_flattened_onto_the_background` expects byte 128; the 81 unit tests and 4 CLI tests of `morningstar-devtools` that cover icons pass.
