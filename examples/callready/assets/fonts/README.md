# Fonts

The five typefaces of the CallReady design, as static TrueType files.

| File | Family | Weight | `hhea` ascent / descent per em |
| --- | --- | --- | --- |
| `Outfit-Regular.ttf` | Outfit | 400 | 1000 / 260 of 1000 |
| `Outfit-Medium.ttf` | Outfit | 500 | |
| `Outfit-SemiBold.ttf` | Outfit | 600 | |
| `Outfit-Bold.ttf` | Outfit | 700 | |
| `Caveat-SemiBold.ttf` | Caveat | 600 | 960 / 300 of 1000 |
| `ArchivoBlack-Regular.ttf` | Archivo Black | 400 | 878 / 210 of 1000 |
| `AbrilFatface-Regular.ttf` | Abril Fatface | 400 | 1058 / 291 of 1000 |
| `Anton-Regular.ttf` | Anton | 400 | 2409 / 674 of 2048 |

The ascent and descent are what `src/theme.rs` uses to reproduce the
browser's default line height.

**Source.** Google Fonts, downloaded 2026-11-12 from the `truetype` URLs
that
`https://fonts.googleapis.com/css2?family=Outfit:wght@400;500;600;700&family=Caveat:wght@600&family=Archivo+Black&family=Abril+Fatface&family=Anton`
returns to a client that does not advertise WOFF2 support (Outfit v15,
Caveat v23, Archivo Black v23, Abril Fatface v25, Anton v27). The files are
unmodified. The design bundle itself only carries WOFF2 subsets of variable
fonts, which Morningstar does not load, and no Abril Fatface at all.

The design declares Caveat at 600 and 700 and uses it at the default
weight, for which a browser picks the 600 face; only that one is bundled.
Outfit 800 is declared by the design and never used.

**Licence.** All five families are under the SIL Open Font License 1.1; the
`OFL-*.txt` files are copied from `github.com/google/fonts` (`ofl/outfit`,
`ofl/caveat`, `ofl/archivoblack`, `ofl/abrilfatface`, `ofl/anton`). Abril
Fatface has the reserved font names "Abril" and "Abril Fatface", which is
why its file is the unmodified original and not a conversion.
