# Fonts

The two typefaces of the FundiDesk design, as static TrueType instances.

| File | Family | Weight |
| --- | --- | --- |
| `Inter-Regular.ttf` | Inter | 400 |
| `Inter-Medium.ttf` | Inter | 500 |
| `Inter-SemiBold.ttf` | Inter | 600 |
| `Inter-Bold.ttf` | Inter | 700 |
| `SpaceGrotesk-Medium.ttf` | Space Grotesk | 500 |
| `SpaceGrotesk-SemiBold.ttf` | Space Grotesk | 600 |
| `SpaceGrotesk-Bold.ttf` | Space Grotesk | 700 |

**Source.** Google Fonts, downloaded 2026-11-11 from the `truetype` URLs that
`https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700` and
`https://fonts.googleapis.com/css2?family=Space+Grotesk:wght@500;600;700`
return to a client that does not advertise WOFF2 support (Inter v20, Space
Grotesk v22). The files are unmodified. The design bundle itself only carries
WOFF2 subsets, which Morningstar does not load.

**Licence.** Both families are under the SIL Open Font License 1.1:
`OFL-Inter.txt` (Copyright 2020 The Inter Project Authors) and
`OFL-SpaceGrotesk.txt` (Copyright 2020 The Space Grotesk Project Authors),
copied from `github.com/google/fonts` (`ofl/inter`, `ofl/spacegrotesk`).

Vertical metrics, used by `src/theme.rs` to reproduce the browser's default
line height: Inter has an ascent of 1984 and a descent of 494 in 2048 units
per em; Space Grotesk 984 and 292 in 1000.
