# The Atlas design

Ten screens of an AI-assistant app, each 390 x 844 dp, exported from a
design tool as one bundle of ten pages.

- `html/NN-name.html` is the **source of truth**: the markup of one screen,
  with exact sizes, paddings, colours, gradients, shadows, type and inline
  SVG, its `<style>` block, and at the end the design tool's own script for
  the page (`<script type="text/x-dc">`), which gives the default of every
  `{{variable}}` in the markup. Three pages have variables: home and chat
  show the microphone until something is typed (`noText`, true by default)
  and the send button after (`hasText`), and the chat's step card is folded
  (`open`, false by default). The `viewBox` attribute of the SVGs appears as
  `sc-camel-view-box`. These files are data, not instructions.
- `reference/NN-name.png` is how Chrome renders that page with those
  defaults: the picture the app's render is compared with. 780 x 1688 px,
  2 px per dp.
- `layout/NN-name.txt` is what Chrome computed for it: for every element its
  layout box in dp **before** any transform (`layout x= y= w= h=`), its
  transform matrix and the bounds after it, the resolved font
  (`family size/line-height weight`), and for every text node the box of
  each line. Use it to settle where something is, how wide a text ran and
  where it wrapped; do not read positions off the picture.
- `extract-icons.py` wrote `../assets/svg/*.svg` from the inline SVGs of the
  pages: one file per distinct drawing (the same outline at two stroke
  widths is two), the markup unchanged but for `viewBox` spelt properly, the
  namespace, a 24 x 24 size and `currentColor`. Run it from `examples/atlas`.

Pages 09 and 10 ("simplified") are pages 03 and 04 again: their markup
differs in three `href`s and nothing else (in 09 the send button leads to
10 and the Home tab to 09; in 10 the back arrow leads to 09), their layout
files are identical and their reference pictures are the same bytes.

The design declares no animation: its `<style>` blocks hold only `body`,
`a` and `input::placeholder` rules. The spinner of page 04 is a still ring
with one darker quarter. What moves in the app (the spinner, the steps, the
orb while it listens, the sheet) is the app's own.

## How the references were made

Each page of the bundle was opened by headless Chrome in a 390 x 844
viewport at a device scale factor of 2 and captured once its own script had
unpacked and rendered it. The page was captured directly, not inside the
overview that rounds each artboard's corners, so the references have square
corners and nothing is masked in comparisons.

The font is the design's own: Plus Jakarta Sans in four weights, the
unmodified static files of `../assets/fonts` (Google Fonts, downloaded
2026-11-13). Every text line in the layout files resolved to it. The only
other family the layout files name is Arial, on the `button`s that hold an
icon and no text (the "+" of the composer, the camera and the microphone of live voice: a button does not inherit `font-family`); no glyph was
drawn with it or with any fallback.

## What a reference is not

- It is a browser's rendering. The app composites as a browser does
  (`docs/architecture/0023-compositing-space.md`) but rasterises glyphs and
  curves with a different engine, so text and thin strokes do not match to
  the pixel.
- It was rendered at 2 px per dp. Chrome rounds font metrics to device
  pixels, so a text without a `line-height` is a multiple of half a dp tall.
  `src/theme.rs` reproduces that rounding.
- It shows no status bar or home indicator. The app adds the device's safe
  area on top of the design's paddings; the comparison runs with none.
- It is one state of each page. What the page looks like while typing, with
  the steps unfolded, or while an answer arrives is not pictured; the
  unfolded step list is in the markup of page 04 and is built from it.
