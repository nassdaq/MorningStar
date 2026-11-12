# 0031 — The shaping cache is keyed on everything but colour

Status: Accepted (2026-11-12). Verified in tests on the development host. Not timed.

Removes a cost recorded in [0021](0021-viewport-wrap-scroll-animation-navigation.md) ("fading rich text with coloured spans reshapes every frame"). Extends [0009](0009-text.md) and [0015](0015-fonts-icons-and-design-fidelity.md) (the layout cache, rich text). It changes no decision of theirs.

## Context

A shaped paragraph is cached under a hash of what it was shaped from. That hash included each span's colour, and each cached glyph carried its span's colour. Opacity is multiplied into colours ([0021](0021-viewport-wrap-scroll-animation-navigation.md); still so for text, [0028](0028-group-opacity-layers.md)), so a paragraph with a coloured span had a different key in every frame of a fade and was shaped again each time, the expensive step the cache exists to avoid. A paragraph without coloured spans was not affected: the style's colour was never in the key.

## Decision

Span colours are not hashed. A cached glyph records **which span it came from**, and its colour is looked up in the spans given to the draw call. Shaping depends on the text, the division into spans, weights, families, italics, size, line height, alignment, letter spacing, tabular numbers and the width; exactly those remain in the key.

Two paragraphs that differ only in span colours now share one cache entry, which is correct: their glyphs and positions are the same.

`TextContext::shaped_count()` reports how many paragraphs have been shaped, for tests and benchmarks.

## Cost

A glyph's colour costs an index into the spans per glyph per draw instead of a field read. The cached glyph is sixteen bytes smaller (a `u32` in place of an `Option<Color>` of 20).

## Verified

`crates/morningstar-text/tests/shaping_cache.rs` (2): ten frames of a paragraph with a coloured span fading shape it once, with every glyph in that frame's colour and in the same place; a span colour appearing or going does not reshape, and weight, size, text and the division into spans do. The text crate's other tests and every fidelity figure are unchanged.
