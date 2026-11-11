# 0006 — UI architecture direction

Status: Proposed. Implemented so far: the display list with two operations (filled, optionally rounded rectangle; glyph), its GPU renderer, text (ADR 0009), and raw pointer input. Everything else records intent so Phase 2 and 3 work pulls in one direction.

## Direction

```
declarative description -> retained widget tree -> layout -> display list -> GPU
        (app code)          (reconciliation)                  (renderer input)
```

- **The display list is the contract between UI and rendering**, and is the first thing to build (Phase 2). It is a flat, ordered list of draw operations (rect, rounded rect, image, glyph run, clip, transform). `Frame` (ADR 0005) grows from a clear colour into a display list. The renderer can then be developed and benchmarked with hand-written display lists, before any widget exists.
- **Retained tree, declarative API.** The developer describes UI as a function of state; Morningstar keeps a retained tree and reconciles, so a state change costs work proportional to what changed.
- **Dirty tracking at three levels**: which widgets rebuild, which subtrees re-lay-out, which screen regions redraw. This is also what makes true idle rendering possible (ADR 0004).
- **Accessibility is a parallel output of the same tree**, not an afterthought: a semantic tree that the platform hosts expose to TalkBack and VoiceOver.
- **Text is its own subsystem** with a Morningstar-owned API. Shaping, bidi, fallback and line breaking are too large to write from scratch; evaluate `cosmic-text`, or `rustybuzz` + `swash` directly, behind that API. Arabic and Swahili are first-class test cases.
- **Native stays native** where the OS does it better: text input goes through the platform IME, and pickers, permission prompts and share sheets are system UI.

## Decisions deliberately not made yet

- The state model (signals, an Elm-style update loop, or immediate-mode closures). The roadmap's illustrative `State<T>` snippet has a button callback that borrows `&self` from `build`, which a stored callback cannot do; the real design has to resolve ownership first.
- Layout algorithm: single-pass box constraints, or flexbox via an existing crate.
- Batching and text atlas strategy. These need measurements on real devices.

## Acceptance criterion for Phase 2

A hand-written display list with rectangles, an image and a line of text renders identically (by pixel comparison of offscreen output) on Metal and Vulkan, with frame statistics recorded on at least one low-end Android device.
