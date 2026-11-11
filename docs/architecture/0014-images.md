# 0014 — Images

Status: Accepted (2026-11-11). Verified on macOS/Metal only. Not yet run on Vulkan or GLES, nor on a device.

Builds on [ADR 0011](0011-renderer-vocabulary.md), which listed what an image operation would need, and resolves the *suboptimal swapchain* risk recorded in [ADR 0012](0012-scale-insets-and-demand-rendering.md).

## Context

The renderer drew rounded rectangles (solid, bordered, gradient), shadows, clips and glyphs. Real designs also need photos, avatars, logos and icons. ADR 0011 named the missing pieces: texture resources with a lifetime rule, a way to bind more than one texture, an RGBA sampling kind in the shader, and filtering with correct colour.

Constraints carried over from earlier decisions: no `wgpu` type in a public API (0002); plain data crosses from the runtime to the GPU crate, as the glyph atlas does (0009, 0011); the device runs within `Limits::downlevel_defaults()` and must work on GLES 3.0 (0002); nothing is drawn while the picture is unchanged (0012); dependencies are added sparingly and recorded (0007).

## Decision

### Resources: a store of pixels, a view for the renderer, textures created on first draw

- `morningstar-core` gains `ImageStore`, which owns the pixels of every registered image, and `Image`, a 16-byte `Copy` handle carrying an `ImageId` and the image's width and height. Ids come from one process-wide counter and are never reused, so a handle that outlives its image names nothing rather than some other image.
- An app registers images through `Setup::add_image_rgba8` / `add_image_png` in `App::setup`, or the same two methods on `Frame` later, and releases them with `Frame::remove_image`. It never sees a GPU type.
- The renderer receives `ImagesView`, a borrowed view of the store, next to the display list and the `AtlasView`. It holds a `generation` that changes whenever an image is added or removed.
- `morningstar-gpu` keeps one texture per image id. A texture is created **the first time the image is actually drawn** (an image that is registered but never drawn, or drawn only where a clip hides it, costs no GPU memory). When the view's generation differs from the last one seen, textures whose id is no longer in the view are dropped. This runs at the start of every frame, including a frame that draws nothing.
- **The store keeps the pixels for as long as the image is registered.** That is what lets a renderer be given the image at any time: after the pipeline is rebuilt for a new surface format, after a device is recreated, or by a second renderer (the offscreen one used for screenshots). The cost is that each image is in memory twice, once on the CPU and once on the GPU. See Limitations.

As with the atlas's dirty rows (0011), nothing flows back from the renderer to the store. The worst outcome of a missed or repeated frame is an upload happening later or a texture being freed later, never a stale or dangling texture.

**Lifetime across surface loss.** Textures belong to the device, which outlives surfaces (0002), and live in the same object as the glyph atlas texture, so backgrounding an Android app does not discard them. If that object is rebuilt (the surface format changed), its textures are dropped with it and uploaded again from the store on next use. Nothing is leaked in either case, because the only owner of a texture is the map entry for its id.

**Use after release is harmless.** A `DrawOp::Image` whose image is not in the view is skipped. The same happens for a handle from another store.

### Drawing: one new operation

`DrawOp::Image { image, rect, source, corner_radius, tint }` draws the region `source` of an image (in the image's own pixels) into `rect`, cut to rounded corners, multiplied by `tint`. `DisplayList` has three ways to record it:

| Method | Use |
| --- | --- |
| `draw_image(image, rect)` | the whole image, stretched |
| `draw_image_fit(image, bounds, fit, corner_radius, tint)` | keep the aspect ratio: `ImageFit::Cover`, `Contain`, or `Stretch` |
| `draw_image_region(image, source, rect, corner_radius, tint)` | the general form: sprite sheets, cropping |

Fit modes are CPU-side arithmetic, `ImageFit::place(source, bounds) -> ImagePlacement { source, rect }`, not renderer state. `Cover` fills the bounds and crops the overflowing axis equally on both sides; `Contain` shows everything, centred, and leaves the rest of the bounds untouched; `Stretch` distorts. `place` takes a source region, so it also fits one sprite of a sheet.

`tint` multiplies colour and alpha. `Color::WHITE` leaves the image as it is, white with alpha 0.5 is half opacity, and a colour recolours an icon drawn in white. A circular avatar is `ImageFit::Cover` into a square with `corner_radius` of half its side.

An image is an instance of the same quad as everything else, so it is clipped by the clips in effect (rectangular and rounded) and painted in list order with no special cases.

### GPU design: one texture per image, draw calls split where the image changes

Three designs were considered.

**A. One texture per image; the instance stream is cut into runs (chosen).** All instances stay in one buffer in paint order. A run ends only where an image instance needs a *different* image texture than the one the current run has bound. Rectangles, shadows and glyphs sample no image, so they join whatever run is open and never cause a split. Each run is one draw call with its own bind group for group 1.

- Draw calls per frame = (number of changes of image along the list) + 1. A list with no images, or that draws one image any number of times, is still exactly one draw call. Sixty distinct images in a row are sixty draw calls (tested).
- No size limit below the device's, no packing, no eviction policy, no bleeding between unrelated images, a full mip chain per image, exact memory per image.
- An app that wants many icons in one draw call packs them into **one sprite sheet** and draws with source rectangles. That is the atlas benefit, under the control of whoever knows which images belong together, and it already works.

**B. Pack every image into a shared RGBA atlas (rejected).** Keeps a single draw call always. But: images larger than a fraction of the atlas (photos) do not fit or evict everything else; with a 2048 limit on the lowest tier one photo can be the whole atlas; releasing images fragments it, so it needs a packer with eviction or compaction; neighbours bleed into each other under bilinear filtering unless every image is padded, and under mipmapping unless padding grows with the mip level or mips are abandoned; every upload and every repack is CPU work on the render thread. This is a lot of machinery to save draw calls that have not been shown to cost anything.

**C. Hybrid: atlas for small images, own textures for large (rejected for now).** The plausible end state if draw calls are ever measured to matter, and it can be added behind the current API without changing it: the handle and the operation say nothing about where the texels live. It carries all of B's complexity for the small-image half. No measurement asks for it yet.

Why A is enough for "dozens of icons and a few photos": a draw call here is one bind-group change, one vertex-buffer rebind and one draw, with no pipeline change. Tens of them per frame is far inside what mobile GPUs and drivers handle; and frames are only produced when something changes (0012). This is an argument, not a measurement: see Not verified.

Runs are drawn by binding **a slice of the instance buffer per run** and drawing instances `0..n`, instead of binding the buffer once and drawing instance ranges. A non-zero first instance is the less portable of the two on GLES-class backends.

While a run has no image, a one-texel transparent placeholder is bound, because the pipeline requires group 1.

### Instance layout: unchanged, 112 bytes

An image needs nothing a rectangle's instance does not already have room for:

| Location | Field | Image meaning |
| --- | --- | --- |
| 1 | `color` | tint (premultiplied) |
| 2 | `color2` | the texture coordinates sampling is confined to: min and max |
| 3 | `params.z` | mip level |
| 4 | `extra` | source region in texture coordinates: x, y, width, height |

Kind 4 is the image. The instance is 112 bytes before and after; rectangles and glyphs carry nothing new. The vertex-to-fragment interface is still nine varyings. The fragment shader samples the image **inside the image branch**, at an explicit level, so other kinds do not pay for a second texture fetch.

### Filtering and mipmaps

- The sampler is bilinear within a level and linear between levels, clamped at the edges.
- Every image texture has a **full mip chain, generated on the CPU when it is uploaded**: each level is a box filter of the one before, averaged in linear light on premultiplied values. This is the same result on every backend, needs no render-target support for the texture format and no second pipeline, and was measured as cheap (below).
- **The mip level is computed per instance on the CPU**: `log2(max(source texels / destination pixels))` over the two axes, never below zero. For an axis-aligned, unrotated quad, which is all the renderer draws, this is what the GPU would derive from screen-space derivatives. The shader therefore still uses no derivatives and no implicit-level sampling, which keeps the per-instance branching valid on every backend (0011).
- An image drawn at its own size on whole pixels samples level 0 at texel centres, which reproduces its bytes exactly.
- Sampling is confined to the source region inset by half a texel of the level in use, so a magnified sprite does not pick up its neighbour in the sheet.

Measured on the development machine (Apple M4, optimised build of `morningstar-core`, one run each): the whole mip chain of a 2048 x 2048 image takes about 9 ms; registering plus mip chain for a 1280 x 720 photo 2.4 ms opaque and 5.2 ms translucent; for a 256 x 256 icon 0.2 to 0.4 ms. A phone will be several times slower; that has not been measured.

### Colour

A PNG holds sRGB-encoded colour with straight alpha. The pipeline blends linear colour with premultiplied alpha. Two conversions are needed, and their order matters.

1. **Premultiply in linear light, at registration.** Each colour byte is decoded to linear, multiplied by alpha, and encoded back to an sRGB byte. Opaque pixels are untouched. Fully transparent pixels become transparent black, so whatever colour they carried (exported images often carry white, black or garbage there) cannot reach their neighbours.
2. **Let the GPU decode.** The texture format is `Rgba8UnormSrgb`. Sampling decodes each texel to linear *before* filtering, so a sample is premultiplied linear colour, filtered correctly, and is used like any other premultiplied colour: multiplied by the premultiplied tint, by shape coverage and by clip coverage.

Multiplying the encoded bytes by alpha instead (the common shortcut) is wrong by a large factor: half-transparent white must store byte 188, not 128. Filtering straight-alpha texels and premultiplying afterwards is what produces dark or coloured fringes at transparent edges. Both are pinned by tests.

Storing premultiplied values sRGB-encoded keeps 8-bit precision where the eye needs it, in the darks.

Colour-space chunks in a PNG (gAMA, cHRM, iCCP) are ignored. Samples are taken to be sRGB.

### Limits

- **Stored size: longest side at most 2048 pixels** (`MAX_IMAGE_DIMENSION`), the `downlevel_defaults` texture limit and so the largest texture every target device can hold. A larger image is **scaled down at registration**, with a box filter in linear light, keeping its aspect ratio, so that its longer side is exactly 2048. The handle still reports the original width and height and source rectangles stay in original pixels, so application code is unaffected; only sharpness is lost. The rule is the same on every device, including ones that could hold more, so an image looks the same everywhere.
- **Registration size: at most 256 MiB of decoded RGBA** (`MAX_IMAGE_BYTES`, about 8192 x 8192). Beyond that registration fails with `ImageError::TooLarge`. For a PNG the header is checked before anything of that size is allocated.
- Should a device report a texture limit below the stored size, the renderer scales the image down further at upload instead of failing. With the device requested as it is today this cannot happen; it is a guard.
- Registration errors are values: `ImageError::{Empty, WrongLength, TooLarge, Decode}`. Nothing panics on bad input.

### Decoding: `png`, in the runtime, behind the Morningstar API

`Frame::add_image_png` and `Setup::add_image_png` decode with the `png` crate in `morningstar-runtime/src/image.rs`, the only place that names it. All PNG colour types and bit depths are accepted and normalised to RGBA8 (palettes and transparency chunks expanded, grey to RGB, 16-bit reduced to 8); only the first frame of an animated PNG is read. Malformed, truncated or non-PNG data is `ImageError::Decode` carrying the decoder's message.

**JPEG, WebP and every other format are out of scope.** An app that needs them decodes them itself and registers the pixels with `add_image_rgba8`.

Decoding and registration run on the calling thread, which is the render thread. See Limitations.

### Dependency

| Crate | Version | Used by | Why |
| --- | --- | --- | --- |
| `png` | 0.18 | `morningstar-runtime` | PNG decoding for `add_image_png`. Inflate and the PNG filters are not worth owning. |

This is the first image dependency **linked into apps**; until now `png` was only in the development tool. It has no optional default features to turn off. It brings `bitflags`, `crc32fast`, `fdeflate`, `flate2` and `miniz_oxide` (with `simd-adler32`, `adler2`, `cfg-if`, `zlib-rs`), all already in `Cargo.lock` through `morningstar-devtools`, so no new crate is downloaded or audited. **Its effect on the size of an app's native library has not been measured** (no release build was made for this change); it should be measured with the next Android build and recorded in ADR 0007's inventory, which this table extends.

The placement in the runtime rather than in `morningstar-core` keeps core dependency-free. The store, the handle and the colour maths are in core and need nothing.

### Demand-driven rendering

Images are registered and released only through `Setup` (before the first frame) and `Frame` (while a frame is being built). The frame being built carries the changed image set to the renderer when it is presented, so **a registration or release never needs a frame of its own**: an image registered and drawn in one `App::frame` is on screen with that frame, and a released image's texture is freed by that frame's render. If that frame is skipped or fails, the engine already owes it again (0012) and the same image set is delivered again. This is tested at the engine level. An app that changes what it *wants to draw* during `App::frame`, after having drawn, asks for the next frame with `request_redraw` as for any other state change.

### Suboptimal swapchain (ADR 0012, Risks)

A frame acquired from a suboptimal swapchain was presented and the swapchain reconfigured "for the next frame", which demand-driven rendering no longer guarantees.

- `Renderer::take_frame_request()` (new) returns true, once, after a presented frame that was drawn into a suboptimal swapchain. The request is made **once per unbroken run of suboptimal frames**: a swapchain that reports suboptimal on every frame cannot keep the app rendering. Detaching or attaching a surface clears it.
- The engine asks after every presented frame and, on true, keeps its "a frame is wanted" flag set, so exactly one more frame is drawn into the rebuilt swapchain. That frame counts as back to back with the one before it in the statistics.
- No existing signature changed. `FrameOutcome` is untouched.

On Android with Vulkan, wgpu 30 itself treats `VK_SUBOPTIMAL_KHR` as success (it does not pre-rotate), so this path is expected to matter on other backends and platforms rather than there.

## Public API added

`morningstar-core`: `Image`, `ImageId`, `ImageStore`, `ImagesView`, `ImagePixels`, `ImageError`, `ImageFit`, `ImagePlacement`, `MAX_IMAGE_DIMENSION`, `MAX_IMAGE_BYTES`; `DrawOp::Image`; `DisplayList::{draw_image, draw_image_fit, draw_image_region}`.

`morningstar-gpu`: `Renderer::{render_with_images, last_stats, take_frame_request}`, `OffscreenRenderer::{render_with_images, last_stats}`, `render_offscreen_with_images`, `RenderStats { instances, draw_calls, image_uploads, image_textures, image_texture_bytes }`. The existing `render`, `OffscreenRenderer::render` and `render_offscreen` keep their signatures and behave as a frame with no images.

`morningstar-runtime`: `Setup::{add_image_rgba8, add_image_png}`, `Frame::{add_image_rgba8, add_image_png, remove_image, has_image, images}`; re-exports of `Image`, `ImageError`, `ImageFit`, `ImageId`, `ImagePlacement`, `ImagesView`.

Anything that renders a `Frame` itself must pass `frame.images()` to one of the `*_with_images` entry points, as it passes `frame.atlas()`. The engine does. `morningstar-devtools` and the example's screenshot tool still call `render_offscreen`, so **images do not appear in their screenshots until they are switched over** (a one-line change each, outside this change's ownership).

## Verified

All on macOS/Metal, on the real GPU, in debug builds.

- **Registration and release** (`morningstar-core`, 13 tests; `morningstar-gpu/tests/images.rs`; `morningstar-runtime`): ids never reused, across stores too; double release harmless; textures created on first draw only, not for registered-but-undrawn or fully clipped images; a released image's texture freed on the next frame, including a frame that draws nothing; drawing a stale handle draws nothing; a second renderer uploads again from the same store; a renderer given a different store drops the first one's textures; the entry points without images draw everything else.
- **Oversized images**: a 4096-wide image is stored 2048 wide, reports 4096, and draws correctly with source rectangles in original pixels; texture bytes equal the reduced image plus its mip chain.
- **Exact pixels** for an unscaled opaque image, every pixel of the target checked.
- **Source rectangles**: each sprite of a 2 x 2 sheet magnified eight times is its own colour at every pixel (no bleed from neighbours).
- **Fit helpers**: the arithmetic for stretch, contain and cover, wide and tall, within a sprite region, and for degenerate rectangles; and on the GPU.
- **Rounded corners**: an image's coverage is within 1/255 of a rounded rectangle's at every pixel, for radii 0, 6 and a full circle, at a fractional position; a circular avatar shows its four quadrants and nothing at its box's corners.
- **Tint and opacity**: white icon recoloured, half opacity, colour filtered through a tint, both together over a background.
- **Premultiplied alpha**: a magnified edge between opaque red and transparent texels carrying green or black is exactly red over red at every pixel (no dark or coloured fringe); opaque black beside transparent white is exactly black over black (no light fringe); half-transparent texels come out as half the light.
- **sRGB to linear**: six grey levels within 1/255 of the sRGB transfer function; byte 128 is 22% light, half the light is byte 188.
- **Clipping**: square clip with a half-covered fractional edge, rounded clip identical to a rounded rectangle, a clipped image keeps its texels in place.
- **Paint order across draw calls**: rectangle, image, glyph, second image, rectangle, first image again, glyph, each overlapping the one before, all correct, in three draw calls. One image drawn twelve times between rectangles: one draw call.
- **Many images**: sixty distinct images in one frame, each in its place, sixty draw calls; the following frame uploads nothing and is byte-identical.
- **Mipmaps**: a striped 256-pixel image reduced sixteen times is uniform half-light grey, and a checkerboard reduced by 6.4 times likewise (both tests were confirmed to fail with mip selection disabled), while the same image at its own size keeps its stripes exactly.
- **PNG**: RGBA, RGB, grey, grey with alpha, 1-bit, palette with transparency and 16-bit all decode to the right RGBA; empty input, non-PNG text, a JPEG header, a bare signature, a truncated file, a file missing its end, a corrupted header and garbled compressed data are all `ImageError::Decode`; a header claiming 60000 x 60000 is refused as `TooLarge` without allocating.
- **Engine**: an image from `setup` reaches the first frame; one registered during a frame is in that frame's image set and the engine goes idle after it; a release reaches the renderer with its frame and again if that frame was skipped or failed; a renderer follow-up request produces exactly one more frame, and is asked for only after presented frames.
- **By eye**: `image_scene` in `morningstar-runtime/tests/images.rs`, written as an sRGB PNG when `MORNINGSTAR_DUMP_DIR` is set. A circular avatar cropped from a portrait-shaped image with a ring, and the same image reduced six times; a cover-fitted landscape in a shadowed card, cut by the card's rounded clip, with a gradient scrim and glyphs over it; one white star icon in five tints whose transparent pixels carry magenta (none shows); a four-icon sprite sheet, one icon at half opacity; a checkerboard scrolled under a rounded viewport clip and letterboxed with `Contain`; a translucent glow with no rim; and a stack of image, rectangle, image, rectangle, glyphs, translucent image in the right order.

Test counts for the three crates touched: `morningstar-core` 44; `morningstar-gpu` 9 unit, `atlas_upload` 5, `images` 23, `offscreen` 11, `vocabulary` 26; `morningstar-runtime` 36 unit, `images` 9. Every test that existed before passes unmodified.

## Not verified

- **Vulkan and GLES.** The shader change (a second bind group, a texture sampled inside a branch at an explicit level) and per-run vertex-buffer slices passed wgpu's validation on Metal only. sRGB texture formats, non-power-of-two textures with full mip chains and explicit-level sampling are all core in GLES 3.0, which is the reason for the choices above, but none of it has run there.
- **On a device.** Nothing here has run on Android or iOS.
- **Frame-time cost** of extra draw calls, and of first-draw uploads, on a phone.
- **Binary-size cost of `png`.**
- **The suboptimal-swapchain path on a real surface.** The once-per-run rule and the engine's scheduling are unit-tested; no swapchain reported suboptimal during testing.

## Known limitations

- **Pixels are held twice**, in the store and on the GPU (where mipmaps add a third more). A 2048 x 2048 image is 16 MB in the store and 21 MB of texture. Dropping the CPU copy after upload would need the renderer to tell the store, and a way to recover after texture loss; neither exists yet.
- **No texture eviction.** A texture lives until its image is released, even if it has not been drawn for a long time. Apps showing many photos must release the ones that left the screen.
- **Decoding, premultiplying, downscaling and mip generation run on the render thread.** A large photo registered or first drawn during a frame can cost more than that frame's budget. Moving this off-thread belongs with the worker-thread design ADR 0004 leaves open; `add_image_rgba8` already lets an app decode elsewhere.
- **One draw call per change of image.** Many distinct small images interleaved cost as many draw calls. Use a sprite sheet where that matters.
- **Sprite sheets and mipmaps.** Confinement to the source region is exact at level 0. When a sprite is drawn *reduced*, the coarser mip texels themselves average across sprite boundaries, so neighbours can bleed in. Leave transparent gutters between sprites that will be drawn small, or keep such icons as separate images.
- **Isotropic mip selection.** A source squashed much more on one axis than the other is blurred along the less-squashed axis. No anisotropic filtering.
- **Magnification is bilinear.** There is no nearest-neighbour mode for pixel art.
- **Downscaling uses a box filter**, which is slightly soft and can alias mildly compared with a windowed filter. It is exact for the 2:1 steps of a mip chain.
- **Images over 2048 pixels lose detail** on every device, including those that could show more.
- **PNG only**, sRGB assumed, no animation, no EXIF orientation (not a PNG concept, but the next format will need it).
- **`tint` multiplies.** No other blend modes, no colour filters, no per-corner radii, no rotation, no flips (a negative source size is rejected), no tiling.
- **`Contain` leaves the rest of its bounds unpainted**; draw a background first if one is wanted.
