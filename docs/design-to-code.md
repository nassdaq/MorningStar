# Design to code: the loop

How to turn a picture of a screen into Morningstar Rust code, whether you are a developer or an AI agent. The method is a loop that takes seconds per turn and needs no phone:

1. **Render** the code to a PNG on this machine.
2. **Compare** the PNG with the design.
3. **Read the report**: does it match, by how much not, and where.
4. **Fix** the worst region and go to 1.

The background is in [ADR 0008](architecture/0008-design-to-code-loop.md). The tools live in `tools/morningstar-devtools`, which is a development crate and is never linked into an app.

## 0. Prepare the design image

- **PNG.** Any bit depth, palette, greyscale or interlacing is read. Colour profiles are ignored and pixels are taken as sRGB, so export in sRGB (not Display P3).
- **Export at the size you will render, or larger.** A 2x or 3x export compared with a 1x-sized render is averaged down cleanly. The other direction (a small design against a large render) enlarges the design, which blurs its text and edges and produces differences that are not your fault.
- **Same shape as the render.** If the design is 390x844 points, render at 390x844, 780x1688 or 1170x2532. If the shapes differ the tool says so and fails unless told otherwise (see "Aspect ratio").
- Note anything in the mock-up that the app does not draw (status bar, home indicator, a device frame). You will pass these as ignore regions.

## 1. Render

`morningstar_devtools::screenshot(app, size, elapsed)` runs one frame of an `App` through the real renderer and returns PNG bytes. Put it in an example next to the app, as `examples/hello-gpu/examples/screenshot.rs` does:

```rust
use std::time::Duration;
use morningstar::prelude::*;
use morningstar_devtools::screenshot;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = MyApp::default();
    // Drive the app into the state the design shows, with pointer events.
    let png = screenshot(&mut app, SurfaceSize::new(1080, 2400), Duration::ZERO)?;
    std::fs::write("out/home.png", png)?;
    Ok(())
}
```

```sh
cargo run -p hello-gpu --example screenshot out/
```

The frame is a function of the app's state, the clock you pass and the size, so the same call gives the same picture every time. Choose `elapsed` deliberately if the screen animates.

## 2. Compare

```sh
cargo run -q -p morningstar-devtools --bin morningstar-diff -- \
    design.png out/home.png \
    --out out/home.diff.png \
    --side-by-side out/home.sbs.png \
    --ignore 0,0,1080,96
```

The first image is the reference (the design), the second the candidate (your render). The report goes to standard output. The exit status is **0** when the images match within the tolerance, **1** when they differ, **2** when the comparison could not be made (bad arguments, unreadable file).

| Option | Default | Meaning |
| --- | --- | --- |
| `--out <file>` | | Heat map PNG. |
| `--side-by-side <file>` | | Reference, candidate and heat map in one PNG. |
| `--threshold <error>` | `2` | Smallest colour error that counts as a difference. |
| `--aa-radius <pixels>` | `1` | Accept edges that moved by up to this many pixels or are anti-aliased differently. `0` is strict. |
| `--max-diff <percent>` | `0.1` | Pass when at most this percentage of compared pixels differ. |
| `--ignore <x,y,w,h>` | | Leave a rectangle out. Repeatable. Candidate pixels. |
| `--fit <width\|stretch>` | `width` | What to do on an aspect-ratio mismatch. |
| `--allow-aspect-mismatch` | | Do not fail only because the shapes differ. |
| `--max-regions <count>` | `10` | How many regions to list. |
| `--json` | | Print the report as JSON instead of text. |

All coordinates, in options and in the report, are **candidate pixels from the top-left corner**. They are the numbers your code uses when it renders at that size.

A comparison of two 1080x2400 images takes about half a second when they match and up to about six seconds when every pixel differs (unoptimised build).

## 3. Read the report

This is a real run: the example app idle (reference) against the same app with its button held (candidate).

```text
morningstar-diff report v1
result: FAIL
reason: 3.6293% of pixels differ (limit 0.1000%)
candidate: 1080x2400
reference: 1080x2400
aspect: match
threshold: 2.00 (error scale: 0 identical, 2 just visible, 100 black vs white)
aa_radius: 1 (effective 1)
max_diff_percent: 0.1000
alpha: ignored (reference is opaque)
compared_pixels: 2592000
ignored_pixels: 0
uncovered_pixels: 0
differing_pixels: 94072
diff_percent: 3.6293
mean_error: 0.673
max_error: 62.24 at 467,1259
regions: 1 (listing 1, worst first; x,y from the top-left in candidate pixels)
region 1: x=252 y=1188 w=576 h=180 pixels=94072 mean_error=18.55 max_error=62.24 reference=#9DEDED candidate=#78B4B4
map: 16x18 cells of 68x136 px; ' ' none, '.' <5%, ':' <25%, '*' <60%, '#' more, '~' not compared
map |                |
map |                |
...
map |   .:******:    |
map |   :########:   |
map |    .::::::.    |
...
```

Read it in this order.

1. **`result` and `reason`.** PASS means stop.
2. **`aspect`.** If it says `MISMATCH`, fix the render size before anything else; every other number is about a partial comparison.
3. **`region` lines.** They are ordered by severity (the sum of the errors of the differing pixels, so large areas and strong differences come first). Each gives the tight bounding box of a cluster of differing pixels, how many pixels differ, how strongly, and the average colour of the reference and of the candidate over those pixels.
   - A region with a large area and a small `mean_error` (under 10) is usually a **wrong colour**: compare `reference=` with `candidate=`. If the pixels fill the box, the two values are close to the colours to change from and to.
   - A region whose box is the whole screen means the **background** differs. Fix it first; the regions underneath only become visible once it matches.
   - A thin region along one side of an element is a **position or size** error of a few pixels. The box tells you which edge.
   - A region with `mean_error` near the maximum where the candidate colour is the background colour is something **missing**; where the reference colour is the background, something **extra**.
   - Many small regions over text mean the **text differs**: font, size, weight, or position. See the limits below before chasing these.
4. **`map`.** A coarse picture of the screen. Use it to see the overall pattern at a glance: one blob, a full column, a band across the top.
5. **The pictures**, when the numbers are not enough. In the heat map the candidate is grey, differing pixels run from yellow (just over the threshold) to red (an error of 50 or more), ignored areas are blue, areas the reference does not reach are purple, and each listed region is outlined in cyan with its number from the report. The side-by-side shows reference, candidate and heat map left to right.

### The error scale

Colour error is the distance in Oklab (a perceptually uniform colour space) times 100.

| Error | Meaning |
| --- | --- |
| 0 | identical |
| 0.3 | one step of one 8-bit channel near mid grey |
| 2 | about the smallest difference visible side by side; the default threshold |
| 10 | clearly a different colour |
| 100 | black against white |

`mean_error` is over all compared pixels, including those under the threshold. `diff_percent` counts only pixels over it.

### What "differs" means

A pixel differs when its colour error is over the threshold **and** the difference is not explained by anti-aliasing. With `--aa-radius 1`, a pixel is accepted if each image's colour at that position either appears in the other image within one pixel (the edge moved by a pixel) or is a mix of the two most different colours there (the edge is covered by a different fraction). The check runs in both directions, so a missing one-pixel line or a line of the wrong colour still counts.

The consequence to remember: **with the default radius, an element that is one pixel too wide, too narrow or shifted is accepted.** Use `--aa-radius 0` when you want to check exact geometry, and expect anti-aliased edges and text to show up.

When the reference is smaller than the candidate, the radius is multiplied by the enlargement factor (shown as `effective`), because one design pixel then covers several screen pixels.

### Aspect ratio

If the shapes differ by more than a pixel of rounding, the report says `aspect: MISMATCH` with both ratios and the height the candidate should have, and the result is FAIL.

- `--fit width` (default) scales the design uniformly to the candidate's width and aligns the tops. Nothing is distorted. Rows only one image has are not compared (`uncovered_rows`, `cropped_rows`).
- `--fit stretch` distorts the design to cover the candidate. Rarely what you want.
- `--allow-aspect-mismatch` lets the comparison pass on the pixels that were compared. Use it when the design really is for a different screen height and the content is top-aligned.

### JSON

`--json` prints the same content as one object: `passed`, `failure`, `candidate`, `reference`, `aspect_mismatch`, the totals, `regions` (each with `rank`, `x`, `y`, `width`, `height`, `differing_pixels`, `mean_error`, `max_error`, `reference_color`, `candidate_color`) and `map`. Both formats begin with the version string `morningstar-diff report v1`; key order and number formats are fixed.

## 4. Iterate

- Fix one thing per turn, the top region first, and re-run. Large errors hide small ones.
- Get structure right before detail: size and aspect, background, the big blocks, then spacing, then text, then edges.
- Read coordinates from the report, not from the picture.
- Loosen the gate rather than fight noise: once the remaining regions are text rendering or edge anti-aliasing, the screen is as close as this loop can prove. Say what remains rather than tuning the code to the tool.
- Look at the side-by-side before declaring success. A number cannot tell you that the screen looks right, only that it looks like the reference.

## Vector artwork: load the SVG, and look at it first

An illustration or icon in the design is an SVG file, and an app loads it as it is (`SvgAsset`, `Picture::svg`, `Icon::svg`; `docs/building-screens.md`, "An illustration or an icon from an SVG file"). Before wiring one in, render it alone with the same loader:

```sh
cargo run -q -p morningstar-devtools --bin morningstar-svg -- design/disc.svg out/disc.png --scale 2
```

- `--scale` is pixels per unit of the file's size (2 for a design reference at 2x); `--size 96` or `--size 200x120` fits it into that many pixels instead. `--background checker` (or `#RRGGBB`) shows white or transparent artwork; `--color #RRGGBB` sets `currentColor`.
- It prints the file's size and whether it is *single-ink* (an icon that takes any colour from one mask).
- A file outside the supported subset is refused with `file:line:column: what` and exit status 1, exactly as the app's `check()` would report it. `--lenient` draws what can be drawn and lists what was left out; use it to see what must be changed in the drawing program (text to outlines, `use` expanded, classes to inline attributes), not to ship.
- To compare one picture with the design, crop the design image to it and run `morningstar-diff` on the two PNGs like any screenshot.

## From Rust

The CLI is a thin wrapper over the library:

```rust
use morningstar_devtools::{CompareOptions, Image, Rect, compare};

let design = Image::load("design.png")?;
let render = Image::from_png(&png)?;
let comparison = compare(&design, &render, &CompareOptions {
    ignore: vec![Rect::new(0, 0, 1080, 96)],
    ..CompareOptions::default()
});
if !comparison.report.passed {
    println!("{}", comparison.report);          // the text report
    for region in &comparison.report.regions { /* region.rect, .mean_error, ... */ }
    comparison.side_by_side().save("out/home.sbs.png")?;
}
```

## Keeping it right: visual regression tests

Once a screen matches, store the render and have a test hold it there:

```rust
#[test]
fn home_screen() {
    let mut app = MyApp::default();
    let png = morningstar_devtools::screenshot(&mut app, SIZE, Duration::ZERO).unwrap();
    morningstar_devtools::assert_matches_reference(
        &png,
        "tests/references/home.png",          // relative to the package root
        morningstar_devtools::Tolerance::DEFAULT,
    );
}
```

- Create or update references by running the tests with `MORNINGSTAR_UPDATE_REFERENCES=1`, then review the changed PNGs like any other diff.
- On failure the test prints the report and writes `home.actual.png` and `home.diff.png` (side by side) next to the reference. The next passing run removes them. Add `*.actual.png` and `*.diff.png` to `.gitignore`.
- A missing reference fails the test; it is never created silently. A different size fails without resampling.
- Store the reference the app rendered, not the design export. The design is the goal; the reference is what the code was agreed to produce.
- `Tolerance::DEFAULT` absorbs differences between GPUs. `Tolerance::EXACT` demands identical pixels and suits only tests that always run on one machine.
- Screenshots are compressed (a 1080x2400 screen is about 150 KB), but keep reference sizes modest; a 360x800 render tests layout as well as a 1080x2400 one.

## Limits

- **The desktop render is not the phone.** Screenshots come from this machine's GPU (Metal on a Mac). A phone uses Vulkan or GLES with different rasterisation of edges and glyphs. The loop proves layout and colour, not device-exact pixels. Check on a device before calling a screen done.
- **Fonts must be the design's fonts.** The app draws with the font files it registers in `App::setup`. If the design uses a font the app does not bundle, text will differ in width, wrapping and shape and no amount of iteration will close the gap. Even with the same file, a design tool hints and anti-aliases glyphs differently, so text shows as scattered small regions with moderate error. Judge text by position and size, and ignore or tolerate the rest.
- **The renderer cannot draw everything a design tool can.** What exists is whatever `DisplayList` in `crates/morningstar-core` offers today (rectangles, rounded rectangles and text at the time of writing, with borders, gradients, shadows and clipping arriving; no bitmap images, blurs or blend modes). Check it rather than assuming. A photo, icon or effect the renderer lacks will stay in the report as a region: cover it with `--ignore` and say so.
- **Colour and blending are the browser's.** Translucent colours, gradients, shadows and anti-aliased edges are composited as sRGB-encoded values, and light text is weighted like Chrome's ([ADR 0023](architecture/0023-compositing-space.md)). A region whose *colour* differs is a wrong value in the code, not a rendering difference; write the design's own alpha and do not compensate.
- **Units.** A design is in points or dp; a render is in physical pixels. Render at the design's pixel size, or scale your numbers by the export factor.
- **Thin features and the radius.** With `--aa-radius 1`, a one-pixel change in the thickness or position of anything is accepted, including text weight. Use `--aa-radius 0` to see them.
- **Enlarged references.** Comparing a small design with a large render blurs the design; expect text to fail. Export larger or render smaller.
- **One whole-screen region hides the rest.** Regions are clusters of adjacent differing pixels. If the background differs, everything is one cluster.
- **Colour management.** ICC profiles and gamma chunks are ignored. A wide-gamut export will read as slightly wrong colours everywhere.
- **Transparency** is compared only when the reference has some; the two images are then judged over both black and white. Against an opaque reference the candidate's alpha is ignored.
- **A pass is relative to the tolerance.** The default lets 0.1% of pixels differ, which at 1080x2400 is about 2,600 pixels: a small icon can hide in that. Read the region list even on PASS, or use `--max-diff 0`.
- **Not a judge of design.** The tool measures similarity to a picture. It does not know about touch targets, text truncation in other languages, dark mode or accessibility.
