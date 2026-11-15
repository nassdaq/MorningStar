# Fonts

The two typefaces of the Barua Talk design, as static TrueType files.

| File | Family | Weight | `hhea` ascent / descent per em |
| --- | --- | --- | --- |
| `Inter-Regular.ttf` | Inter | 400 | 1984 / 494 of 2048 |
| `Inter-Medium.ttf` | Inter | 500 | |
| `Inter-SemiBold.ttf` | Inter | 600 | |
| `SpaceGrotesk-Medium.ttf` | Space Grotesk | 500 | 984 / 292 of 1000 |
| `SpaceGrotesk-SemiBold.ttf` | Space Grotesk | 600 | |

The ascent and descent are what `src/theme.rs` uses to reproduce the
browser's default line height (both fonts set `USE_TYPO_METRICS`; their typo
and `hhea` metrics agree). The design uses these five and no others: Inter
400 for text, 500 for the two tabs that are not chosen, 600 for names in
lists and counts; Space Grotesk 600 for the names at the top of a screen and
500 for the phone call's clock. (Its style sheet also declares Space Grotesk
700, which no element uses.)

The design file itself carries the two families as variable WOFF2 subsets
(Latin, weight axis only), which is what the references were rendered with.
These files are the same outlines at the weights used.

**Source.** Google Fonts (`fonts.googleapis.com/css2?family=Inter:wght@400;500;600&family=Space+Grotesk:wght@500;600`,
asked for as TrueType), downloaded 2026-11-15. The files are the unmodified
static instances.

**Licence.** SIL Open Font License 1.1: `OFL-Inter.txt`, `OFL-SpaceGrotesk.txt`.
