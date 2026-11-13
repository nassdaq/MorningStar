# Fonts

The one typeface of the Atlas design, as static TrueType files.

| File | Family | Weight | `hhea` ascent / descent per em |
| --- | --- | --- | --- |
| `PlusJakartaSans-Regular.ttf` | Plus Jakarta Sans | 400 | 1038 / 222 of 1000 |
| `PlusJakartaSans-Medium.ttf` | Plus Jakarta Sans | 500 | |
| `PlusJakartaSans-SemiBold.ttf` | Plus Jakarta Sans | 600 | |
| `PlusJakartaSans-Bold.ttf` | Plus Jakarta Sans | 700 | |

The ascent and descent are what `src/theme.rs` uses to reproduce the
browser's default line height (the font sets `USE_TYPO_METRICS`; its typo
and `hhea` metrics agree). The design uses these four weights and no
others: 400 for text, 500 for the unselected tabs, 600 and 700 for labels
and headings.

**Source.** Google Fonts, downloaded 2026-11-13. The files are the
unmodified static instances.

**Licence.** SIL Open Font License 1.1: `OFL.txt`.
