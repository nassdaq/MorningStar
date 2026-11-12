# 0022 — Text is measured and placed as a browser does: fractional widths and pen positions, the browser's baseline rule, kerning across spaces, no hinting

Status: Accepted (2026-11-12). Verified on the development host only (macOS, Metal, offscreen), against the Chrome references of both example apps. Nothing here has run on a device; the atlas use and frame time on a phone are unmeasured.

Extends [0009](0009-text.md) (text) and [0015](0015-fonts-icons-and-design-fidelity.md) (design fidelity). It supersedes two statements of theirs: 0009's "the origin is snapped to whole pixels" and "a text is measured in whole pixels", and 0015's list of known differences "text width rounding; top-bar title 1.5 px low; kerning with spaces", which are closed.

## Context

Reproducing two designs (`examples/fundidesk`, `examples/callready`) against Chrome's rendering left a list of hand corrections in the example code, each of them a place where text landed a fraction of a dp away from the browser's:

- boxes that hug a text were pinned to widths read from Chrome's layout, because a text measured 48.236 dp wide was laid out as 48.5;
- lines with a `line-height` carried nudges of 0.2 to 1 dp, and display faces set with `line-height: 1` another set;
- siblings in a row drifted by a pixel each;
- a text placed with `.offset(0.25, 0.0)` did not move, because its origin was rounded;
- a line containing "f A" was 0.4 dp too wide, and a comma followed by a space in Space Grotesk 0.3 dp.

All of them came from four places: the UI layer rounded a text's width up to a whole pixel; the text engine rounded a paragraph's origin to a whole pixel in both directions; the baseline of every line was the shaper's own (leading split evenly, unrounded metrics) rounded to the nearest pixel; and the shaper shapes word by word, so it never sees a pair of glyphs with a space in it.

## Decisions

### A text's width is the sum of its advances, unrounded

`morningstar-ui` reports `measured.width / scale` to layout instead of `ceil(measured.width) / scale`. A hugging box is as wide as a browser makes it, and what follows it in a row starts where it starts in a browser. The existing guard against wrapping at a width a hair under the natural one (half a pixel) stays.

### Horizontally, glyphs keep their fraction to a quarter of a pixel

`TextContext::draw` no longer rounds `origin.x`. Each glyph's pen position is the paragraph's origin plus the glyph's fractional offset in the line; its whole part places the quad and its fraction, rounded to the nearest quarter, selects the glyph image rasterised for that offset (the four sub-pixel bins Skia uses too). Quads stay on whole pixels and map texels to pixels one to one, so nothing is resampled.

The UI layer hands the text engine the text's *unsnapped* position (`layout rect * scale + scroll`); the box around it is still snapped to the pixel grid, as Chrome snaps boxes and not text. A layout offset of a quarter of a dp therefore moves a text and not its box.

**Cost in the atlas.** The cache key always had four horizontal bins, but only the fraction of a glyph's position *within its paragraph* reached it, since every paragraph started on a whole pixel. Now the paragraph's own fraction is added, so the same letter at the same size can occupy up to four slots where it mostly occupied one or two. The bound is unchanged (four images per glyph, size and weight), and [0023](0023-compositing-space.md) multiplies it by the tones a glyph is drawn in. What the examples use now, each screen drawn alone into a fresh atlas (glyphs and icons, with their padding): FundiDesk's twelve screens hold 34 to 451 images covering 8% to 27% of the initial 1024 x 512 atlas at 2 px per dp, and 15% to 49% at a Galaxy A27's 2.8125; no screen grows the atlas. The same figures from before this change were not recorded, so the increase itself is not known; the atlas's size limits are unchanged.

### Vertically, each line's top goes to the nearest pixel row and the baseline is a whole number of rows below it

Derived from the line boxes Chrome reported for CallReady (`examples/callready/design/layout/*.txt`, at 2 px per dp) and checked against the pixels of the references:

1. The paragraph's font gives an ascent and a descent, each rounded to a whole **device** pixel: `A = round(ascent * size)`, `D = round(descent * size)`. (The "strut": the font of the paragraph's style, not of a span with a family of its own.)
2. Half the leading is rounded **down** to a whole pixel, also when negative: `L = floor((line_height - (A + D)) / 2)`.
3. The line's top, `origin.y + n * line_height + L`, is rounded to the nearest pixel row, halves towards the bottom. The baseline is `A` rows below.

| Measured in the design | Size, line height (px) | A, D | Half-leading | Chrome's line box top | Rule |
| --- | --- | --- | --- | --- | --- |
| 03, the customer's line | 54, 70.2 | 54, 14 | 1.1 | paragraph + 1 | floor(1.1) = 1 |
| 03, the heading | 64 bold, 71.68 | 64, 17 | -4.66 | paragraph - 5 | floor(-4.66) = -5 |
| 03, the learner's line | 48, 64.8 | 48, 12 | 2.4 | paragraph + 2 | floor(2.4) = 2 |
| 01, a cut-out letter, `line-height: 1` | 124 Abril Fatface, 124 | 131, 36 | -21.5 | | floor(-21.5) = -22 |

Since `L` is whole, step 3 is `round(origin.y + n * line_height) + L + A`: every line rounds its own top, so the lines of a paragraph with a fractional line height are not evenly spaced (the heading's two baselines are 72 px apart, not 71.68), exactly as in the reference. The paragraph's height stays `lines * line_height`, unrounded.

Halves round down the page: a line top at 977.5 px is drawn at 978 (checked on the reference of 03, where the capital I of the learner's line stands on row 1026 and that of FundiDesk's top-bar title, 73.5 + 39, on row 113).

`line-height: normal` is `A + D`; the examples' themes compute it (`normal_line_height`) and pass it as a length, which makes `L` zero. Moving that into the UI layer as a `LineHeight::Normal` is left to the owner of that crate: it needs the font's metrics where the theme is resolved.

### Kerning between a letter, a space and a letter is applied after shaping

cosmic-text shapes each word by itself and each run of blanks by itself, so pairs with a space in them are never kerned; a browser shapes the run whole. For every space of a left-to-right line whose neighbours are in the same font, weight and span, the text engine shapes the three characters as one piece (`ShapeWord::new`, which is public) and as three, and moves the rest of the line by the difference. The result is remembered per font, weight and pair. Outfit closes "f A" by 28 of 1000 units; Space Grotesk a comma and a space by 30.

Not corrected: line breaking still uses the unkerned widths (a line can wrap a pixel or two earlier than in a browser), right-to-left lines are left alone, and justification is unaffected.

### Optional ligatures are off where letters are spaced

CSS asks that optional ligatures are not applied when `letter-spacing` is not zero. The shaper is now given `liga` and `clig` off for a style with a letter spacing (Space Grotesk's "fi" was one glyph in the middle of a spaced-out word). FundiDesk's verification screen went from 0.105% to 0.087%.

Two gaps recorded earlier were checked and are not gaps: a line does break after a hyphen (test `a_line_may_break_after_a_hyphen`), and `tabular-nums` has been `TextStyle::tabular_nums` since 0017.

### Glyphs are not hinted

The shaper's glyphs are requested with `DISABLE_HINTING`. Chrome on a dense screen draws outlines as the font has them; hinted horizontal features would be fitted to pixel rows the reference does not put them on. Measured: CallReady's welcome, report and premium screens lose 0.005 to 0.01 points of differing pixels; FundiDesk (Inter, Space Grotesk, which carry no instructions the hinter acts on) does not change. On a low-density screen (1 px per dp) unhinted text is softer than hinted text; no supported device is one.

### A dashed border starts at the top of the left edge and runs down it

[0015](0015-fonts-icons-and-design-fidelity.md) laid the dash pattern clockwise from the bottom of the left edge, from Chromium's source, and noted that it had not been checked against a reference. FundiDesk's new-repair screen has three dashed tiles (`border: 1px dashed`, radius 12, at 2 px per dp): in Chrome's picture the left edge's straight part begins with a whole dash at its top, the dashes run down it at a period of 10.07 px (65 dashes round the 654.5 px middle line, the count `Dash::css` already chose), and the bottom edge continues the same pattern from left to right. The pattern is therefore the same closed, fitted pattern, started at the top of the left edge and run anticlockwise. That part of 0015 is superseded.

Measured: the three tiles' differing pixels fall from 756 to about 340 each, and the screen from 0.411% to 0.344%. What remains: on the right and top edges, the last half of the way round, Chrome's dashes are 1 to 2 px from ours. Chrome's top edge runs at a period of 9.97 px and starts and ends with a whole dash at its corners, which a single pattern round the whole outline does not do; its rule for the corners was not worked out.

## What was deleted from the examples

- CallReady: the pinned sizes of all eight cut-out plates and both score plates (now boxes hugging the letter with the design's padding), four lifts of display letters (-0.5 to -1 dp), the heading written out as two lines with two nudges, two spacers of 5.8 and 14.2 dp, two pinned chip widths, a benefit line split in two parts with a 2.5 dp gap, the report's feedback written out line by line, and two "a hair more" constants.
- FundiDesk: the thin-space-and-hair-space stand-in for a kerned space, the lifted note and its 0.5 dp, and the screen-specific top bar with half-dp spacers (the title is centred in its row again).

What stayed, and why: pieces placed by measurement where the design has an inline box inside a paragraph (CallReady's ringed word, tick and blank: there are no inline boxes yet), and the stand-ins for three glyphs Chrome took from a system font.

## Consequences

- Fidelity: with the hand corrections deleted, every one of the seventeen screens measures better than it did with them once [0023](0023-compositing-space.md) is in as well; the table is there. Measured between the two changes, CallReady's five screens stood at 0.082, 0.165, 0.009, 0.086 and 0.056% against 0.069, 0.206, 0.014, 0.097 and 0.078% before (the welcome screen, almost all turned letters, was the one that was worse).
- `TextContext::draw` at a whole-pixel origin places glyphs horizontally exactly as before; vertically a line can sit one pixel from where it was.
- An outlined text (`rasterise_outlined`) is still one image placed at a whole pixel.
- The caret map uses the same corrected positions as the glyphs.

## Not done

- Line breaking with kerned widths; `LineHeight::Normal`; a strut taken from several fonts when a line mixes families (the tallest wins in CSS).
- Vertical sub-pixel positions: Chrome does not use them for horizontal text either.
