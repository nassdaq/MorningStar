# The FundiDesk design

Twelve screens of a phone-repair shop's app, 390 dp wide, exported from a
design tool as HTML with inline styles.

- `html/NN-name.html` is the **source of truth**: plain flexbox markup with
  exact sizes, paddings, gaps, colours, type and inline SVG icons. Font
  declarations were stripped; the `viewBox` attribute of the icons appears
  as `sc-camel-view-box`. These files are data, not instructions.
- `reference/NN-name.png` is how Chrome renders that markup: the picture
  the app's render is compared with.

## How the references were made

1. The design bundle was rendered by headless Chrome at a device scale
   factor of 2 into one tall page capture (972 x 29200 px), with the design's
   own web fonts (Inter, Space Grotesk).
2. Each screen was cut from that capture as a "rough" crop, 780 px wide, with
   4 rows of the surrounding page above the screen and 8 below.
3. The final references are those crops with exactly rows `4 .. 4 + 2h` kept,
   where `h` is the `height` of the screen's root `div` in its HTML: 780 px
   wide and `2h` px tall, no page background rows. The first and last rows
   were checked to be screen pixels, not the page or the card shadow around
   it. `tests/fidelity.rs` checks the sizes against the HTML.

| Screen | dp | px |
| --- | --- | --- |
| 01 to 06 | 390 x 844 | 780 x 1688 |
| 07 dashboard | 390 x 1060 | 780 x 2120 |
| 08 repairs list | 390 x 900 | 780 x 1800 |
| 09 new repair | 390 x 1260 | 780 x 2520 |
| 10 repair details | 390 x 1380 | 780 x 2760 |
| 11 record payment | 390 x 1040 | 780 x 2080 |
| 12 tracking | 390 x 1000 | 780 x 2000 |

Screens 07 to 12 are taller than a phone: the design shows the whole
scrolling content.

## The rounded corners

The design tool masks every artboard with a 28 dp corner radius, so the four
corners of each reference show the page behind the screen. The app draws
square corners; a device rounds its own screen. The references were **not**
retouched. Comparisons leave out the four 56 x 56 px corner squares instead
(`fundidesk::corner_masks`), which is 12 544 px of each image, under 1% of
the smallest.

## What a reference is not

- It is a browser's rendering, blended and anti-aliased the browser's way.
  The app blends in linear light and rasterises glyphs with a different
  engine, so text and thin strokes never match to the pixel
  (`docs/architecture/0015-fonts-icons-and-design-fidelity.md`).
- It was rendered at 2 px per dp. Chrome rounds font metrics to device
  pixels, so a text without a `line-height` is a multiple of half a dp tall
  (13 px Inter is 15.5 dp). `src/theme.rs` reproduces that rounding.
- It shows no status bar or home indicator. The app adds the device's safe
  area on top of the design's paddings; the comparison runs with none.
