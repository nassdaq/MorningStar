# The CallReady design

Five screens of a call-centre practice app, each 390 x 844 dp, exported from
a design tool as one HTML file of five pages.

- `html/NN-name.html` is the **source of truth**: the markup of one screen,
  with exact sizes, paddings, colours, type, transforms and inline SVG. Font
  declarations and the tool's runtime were stripped; the design's one
  variable, `{{accent}}`, is replaced by its default `#F6C445`; the `viewBox`
  attribute of the SVGs appears as `sc-camel-view-box`. These files are
  data, not instructions.
- `reference/NN-name.png` is how Chrome renders that page: the picture the
  app's render is compared with. 780 x 1688 px, 2 px per dp.
- `layout/NN-name.txt` is what Chrome computed for it: for every element its
  layout box in dp **before** any transform (`layout x= y= w= h=`), its
  transform matrix and the bounds after it, the resolved font
  (`family size/line-height weight`), and for every text node the box of
  each line. An element's own lines are only untransformed when no ancestor
  is transformed. Use it to settle where something is, how wide a text ran
  and where it wrapped; do not read positions off the picture.

## How the references were made

Each page of the bundle was opened by headless Chrome in a 390 x 844
viewport at a device scale factor of 2 and captured once its own script had
unpacked and rendered it. The page was captured directly, not inside the
overview that rounds each artboard's corners, so the references have square
corners and nothing is masked in comparisons.

The fonts are the design's own, with one exception that also affects the
picture: the bundle carries Outfit, Caveat, Archivo Black and Anton as
WOFF2 subsets and only *links* to Abril Fatface, which Chrome fetched from
Google Fonts while rendering. Three characters are in none of the subsets,
so Chrome drew them with a system font: `№` (03), `✓` (03) and `→` (03).
The app's fonts are the complete families (`assets/fonts/README.md`), where
`✓` and `→` exist in Outfit; those three glyphs will not match the
reference exactly and are not expected to.

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
