# The Barua Talk design

Five screens of a rooms, chat and calls app, each 390 x 844 dp, exported
from a design tool as one bundle of five pages: Home, Room chat, Incoming
in-app call, Video call, Phone call.

- `html/NN-name.html` is the **source of truth**: the markup of one screen,
  with exact sizes, paddings, colours, type and inline SVG, its `<style>`
  block (without the `@font-face` rules), and at the end the design tool's
  own script for the page (`<script type="text/x-dc">`), which gives the
  default of every `{{variable}}` in the markup. `{{accent}}` is `#2F6BFF`
  on the three pages that have it; page 04's record button is on by default
  (`recordingOn`, which gives `recording`, `recDot` `#FF5A72` and the label
  "Stop recording", and shows the recording pill inside its `<sc-if>`). The
  `viewBox` attribute of the SVGs appears as `sc-camel-view-box`. The
  pictures are named as `../../assets/images/<name>.jpg`. These files are
  data, not instructions.
- `reference/NN-name.png` is how Chrome renders that page with those
  defaults: the picture the app's render is compared with. 780 x 1688 px,
  2 px per dp.
- `layout/NN-name.txt` is what Chrome computed for it: for every element its
  box in dp (`layout x= y= w= h=`), the resolved font (`family
  size/line-height weight`), for a picture its natural size, `object-fit`
  and `object-position`, and for every text node the box of each line and
  the font it was **drawn with**. Use it to settle where something is, how
  wide a text ran and where it wrapped; do not read positions off the
  picture.
- `extract-icons.py` wrote `../assets/svg/*.svg` from the inline SVGs of the
  pages: one file per distinct drawing (the same outline at two stroke
  widths is two), the markup unchanged but for `viewBox` spelt properly, the
  namespace, a 24 x 24 size and `currentColor`. Run it from
  `examples/barua-talk`. (Five more files in that folder are the app's own;
  `src/icons.rs` says which.)
- `prepare-images.py` wrote `../assets/images/*.png`, which is what the app
  draws, from the design's photographs (`../assets/images/*.jpg`): the
  framework decodes PNG only. It converts the portraits and the shop as they
  are, cuts the two pictures of page 04 to the window their `object-position`
  shows (the app's pictures are always centred), and makes the blurred,
  darkened backdrop of page 03 for each person who can ring (the framework
  blurs a box's background, not a picture). Its header says exactly how.
  Needs Pillow and numpy; run it from `examples/barua-talk`.

## The photographs

Ten JPEG files came inside the design file: seven portraits (480 x 480), a
shop at night (1000 x 750), a video still (960 x 1356) and a camera still
(790 x 800). They are the design's own content, copied out of the user's
design file unchanged and given names (`neema`, `support`, `asha`, `zawadi`,
`juma`, `baraka`, `hamisi`, `shop`, `baraka-video`, `self-camera`). The
bundle used the same portrait under several ids; each is here once.

## How the references were made

Each page of the bundle was opened by headless Chrome in a 390 x 844 window
at a device scale factor of 2 (`--force-device-scale-factor=2`) and captured
once its own script had unpacked and rendered it, its fonts had loaded and
every picture was complete. The page was captured directly, not inside the
overview that rounds each artboard's corners, so the references have square
corners and nothing is masked in comparisons: the photographs are compared
like everything else.

The fonts are the design's own, the variable WOFF2 subsets inside the
bundle: Inter and Space Grotesk. Every text line in the layout files was
drawn with one of them (`drawn-with … web font`); none fell back to a system
font. The only other family the layout files name is on `button`s that hold
an icon and no text, which draw no glyph.

(A capture with the scale factor *emulated* instead, through the debugging
protocol, lays text out in whole CSS pixels and draws it noticeably heavier.
That is not what a 2x screen shows and not what the framework's text
metrics follow; it was tried first and discarded.)

## What a reference is not

- It is a browser's rendering. The app composites as a browser does
  (`docs/architecture/0023-compositing-space.md`) but rasterises glyphs and
  curves with a different engine, and reduces a photograph through mipmaps
  where Chrome uses a sharper filter: text, thin strokes and the fine
  detail of a photograph do not match to the pixel.
- It was rendered at 2 px per dp. Chrome rounds font metrics to device
  pixels, so a text without a `line-height` is a multiple of half a dp tall.
  `src/theme.rs` reproduces that rounding.
- It shows no status bar or home indicator. The app adds the device's safe
  area on top of the design's paddings; the comparison runs with none.
- It is one state of each page. Not pictured: any tab of Home but "All", a
  search, an empty list; a room being typed in, a message of plain words
  from someone else, a message on its way or failed; a call being made
  rather than received, a voice call; the call screen with the camera off,
  the microphone muted or the recording stopped (the stopped state is in
  page 04's script and is built from it); a phone call before it is
  answered, its keypad. The design declares no animation.
- Page 05 does not fit its own artboard: its column is 22.5 dp taller than
  844, and the browser takes that from the one thing in it that may shrink,
  the red button, which is drawn 72 by 49.5: an ellipse. The app draws it so
  at the design's size and round where the screen has the room.
