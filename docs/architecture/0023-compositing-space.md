# 0023 — Colours are composited as sRGB-encoded values, like the web platform; light text is weighted

Status: Accepted (2026-11-12). Verified on the development host only (macOS, Metal, offscreen): pixel tests in `crates/morningstar-gpu/tests/compositing.rs` and the seventeen reference screens of the two example apps. **Not run on a device**: the choice of a non-sRGB surface format on Vulkan and GLES, and the frame time, are unverified and unmeasured.

Supersedes the compositing space of earlier records, and nothing else in them:

- [0002](0002-gpu-backend-wgpu.md): the surface format "prefers sRGB, so `Color` is linear and encoding is done by the GPU";
- [0011](0011-renderer-vocabulary.md): gradients interpolated in linear RGB;
- [0014](0014-images.md), "Colour": premultiplying in linear light at registration, the `Rgba8UnormSrgb` texture decoded by the GPU before filtering, and mip levels averaged in linear light;
- [0015](0015-fonts-icons-and-design-fidelity.md): the known difference that anti-aliased edges are blended differently from a browser's, which is closed;
- [0017](0017-transforms-and-vector-pictures.md): a picture's layers composited in linear light.

## Context

The renderer blended in linear light: colours were converted from sRGB to linear, blended, and encoded again by an sRGB render target. That is the physically right way to mix light, and it is not what a design is made with. Browsers and design tools composite the *encoded* values: `rgba(246, 196, 69, 0.25)` over black is a quarter of each byte. Every design this framework is meant to reproduce was looked at, approved and measured in that space. The differences this caused were recorded while reproducing two designs:

- a 25% yellow stroke over black came out as `#83692A` where the design shows `#3E3111`, about twice as bright; CallReady painted such strokes opaque in a hand-computed colour (`on_black`) and reordered layers to hide the consequences;
- a shadow at 14% was half as dark as in the design; FundiDesk doubled its shadows' alphas;
- anti-aliased edges were lighter than Chrome's on light backgrounds and heavier on dark ones, so thin strokes and small text never matched;
- the middle of a gradient was lighter than in a design tool;
- the offscreen target held linear values in 8 bits, which cannot represent the dark end of sRGB: a background of `#0E0E0E` was not exact in a screenshot.

## Decision

**Everything is composited in the encoded space.** A `Color` remains what it was, linear RGB with straight alpha, so no application code changes; the renderer encodes it (`Color::srgb`) when it builds an instance, multiplies by alpha, and from there on blending, gradient interpolation, edge anti-aliasing, image filtering and picture compositing all operate on encoded values. The render target stores what the shader writes, unchanged.

- **Surface**: a format without the sRGB suffix is chosen when the surface offers one. Where it offers only sRGB formats, the surface is configured with that format and its plain twin as a view format, and frames are rendered through a view of the plain format. (wgpu requires `SURFACE_VIEW_FORMATS` for that; a backend without it and without a plain format would fail to configure. None is known among the targets; it is unverified on GLES.)
- **Offscreen target**: unchanged format (`Rgba8Unorm`), new meaning: its bytes are the bytes of an image file. `morningstar-devtools` no longer converts them.
- **Images**: stored as sRGB-encoded bytes multiplied by alpha *as bytes*; the texture is `Rgba8Unorm`, sampled and filtered as it is. Mipmaps and the reduction of oversized images average the stored bytes, as a browser does when it shrinks an image.
- **Gradients**: interpolated between the encoded, premultiplied end colours, which is CSS's `linear-gradient`.
- **Vector pictures and outlined text** (composited on the CPU): the same, in `Canvas`.
- **Shadows**: unchanged in shape (CSS's blur radius is two standard deviations; spread and offset as before). With the blend fixed their darkness is CSS's: measured against FundiDesk's onboarding card (`0 12px 28px rgba(20, 32, 26, 0.14)`), the render is within one level of the reference over the whole fade.

### Linear light is not kept as an option

No consumer needs it: there is no lighting, no HDR, and no content authored for it. A second mode would double the pixel tests that are the renderer's specification and leave every application to discover which one its design assumes. If physically based blending is ever needed (photographic effects), it belongs in an explicit off-screen pass, not in a mode of the one pipeline.

### Light text is weighted

With plain coverage composited in the encoded space, text is still not Chrome's. Measured on the references (ink summed over text regions): Chrome's text carries 8 to 18% more ink than the plain coverage of the same outlines, in both polarities, and far more at the edges of light text on dark (where plain coverage is 0.1 the reference has about 0.6; 0.3 gives 0.8; for dark text on white, 0.3 and 0.5). That is the browser's text rendering (font smoothing and Skia's luminance-dependent text gamma), which blending in linear light used to imitate by accident for light text.

Three corrections were tried against all seventeen references, with the share of differing pixels summed over the screens as the measure (3.38 with none):

| Correction | Sum of differing % | Sum of mean error |
| --- | --- | --- |
| none | 3.38 | 0.795 |
| every glyph image emboldened (coverage plus 0.1 / 0.2 / 0.3 of the heaviest neighbouring pixel) | 3.40 / 4.01 / 4.63 | 0.744 / 0.738 / 0.843 |
| coverage `c^(1/g)`, `g` from 1 for black text to G for white: G = 1.4 / 1.8 / 2.2 / 2.6 / 3.0 / 3.6 | 3.32 / 3.25 / 3.12 / 2.97 / 2.89 / 2.88 | 0.712 / 0.660 / 0.629 / 0.603 / 0.595 / 0.593 |
| the same with a stronger curve for black text (1.2, 1.4, 1.7 instead of 1) at G = 3.0 | 2.89 / 2.90 / 2.90 | 0.593 / 0.592 / 0.597 |

So: no emboldening (it does not help where it matters), nothing for dark text, and a curve for light text. Implemented in `morningstar-text`, not in the shader: a glyph's image is stored once per **tone** of the colour it is drawn in (four tones by the luma of the encoded colour; the exponent's divisor runs 1, 1.67, 2.33, 3), with the curve applied to the coverage when the image is made. Icons and pictures are vector paths, which a browser does not weight, and are exempt; with them exempt and four tones the sum is **2.78**.

The curve is fitted to Chrome on macOS, which is what the references are. Other browsers and platforms weight text differently (Android's Skia less). It is one constant, `LIGHT_TEXT_GAMMA` in `crates/morningstar-text/src/context.rs`; setting it to 1 turns the weighting off.

**Cost in the atlas**: a glyph drawn in dark and in light colours is stored once per tone used, up to four times. An app with one text polarity pays nothing; FundiDesk's fullest screen holds 451 images in 27% of the initial atlas at 2 px per dp, tones and sub-pixel positions included. The atlas's dimensions and limits are unchanged, and so is the instance layout (144 bytes).

## What changed for application authors

- Nothing in the API. `Color`, `hex`, gradients, shadows and images are written as before.
- Pictures change: translucent colours, shadows, gradients and anti-aliased edges now look as they do in the design. Write the design's own values: `box-shadow` alphas as given, `stroke-opacity` as an alpha, layers in the design's order. Workarounds for the old blend must be removed (both examples' were).
- Visual regression references made before this change must be regenerated and looked at (the showcase's were).
- Text in light colours is a little heavier than before on light backgrounds and lighter than before on dark ones, and matches Chrome on both.

## Measured

Share of pixels differing from the Chrome reference, default comparison, before this record and [0022](0022-text-metrics-and-pixel-snapping.md) (with every hand correction in place) and after both (with the corrections deleted):

| Screen | Before | After |
| --- | --- | --- |
| CallReady 01 welcome | 0.0685% | 0.0274% |
| CallReady 02 home | 0.2064% | 0.0209% |
| CallReady 03 live call | 0.0140% | 0.0064% |
| CallReady 04 call report | 0.0965% | 0.0591% |
| CallReady 05 premium | 0.0775% | 0.0147% |
| FundiDesk 01 splash | 0.2868% | 0.2250% |
| FundiDesk 02 onboarding | 0.3615% | 0.2113% |
| FundiDesk 03 login | 0.1671% | 0.1575% |
| FundiDesk 04 create account | 0.0953% | 0.0880% |
| FundiDesk 05 verification | 0.1398% | 0.0870% |
| FundiDesk 06 shop setup | 0.1772% | 0.1359% |
| FundiDesk 07 dashboard | 0.4091% | 0.3607% |
| FundiDesk 08 repairs list | 0.2576% | 0.1781% |
| FundiDesk 09 new repair | 0.4870% | 0.3441% |
| FundiDesk 10 repair details | 0.3425% | 0.2724% |
| FundiDesk 11 record payment | 0.2672% | 0.2460% |
| FundiDesk 12 tracking | 0.2666% | 0.2593% |

(Verification and new repair include two later changes recorded in 0022: optional ligatures off under letter spacing, and the dash pattern's start.) The mean error of CallReady's screens fell from about 0.45 to about 0.03: its near-black background is now exact.

Pixel tests whose expected values changed, each reviewed: the clear colour test (`offscreen.rs`: half the light is byte 188 on screen), the image colour test (`images.rs`: an image's bytes now reach the target unchanged), three image-store tests in `morningstar-core` (premultiplication and box filtering on the encoded bytes), two picture tests in `morningstar-text`, two assertions in `crates/morningstar-runtime/tests/images.rs` and one threshold in `examples/hello-gpu`. Every other pixel test uses colours whose components are 0 or 1, which encode to themselves, and passed unchanged.

## Not done

- The launcher-icon generator in `morningstar-devtools` (`icon.rs`) still flattens artwork over a background in linear light. It is a separate compositor with its own tests and does not go through the renderer.
- A device run, and a look at light text on an Android phone next to Chrome on that phone.
