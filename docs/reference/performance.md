# Performance

The project's rule is that nothing is claimed about performance without a measurement. This page states what has been measured and nothing else.

## On the development machine

A baseline for noticing regressions. **It is not a device measurement and says nothing about a phone.**

| | |
| --- | --- |
| Machine | Apple M4, 10 cores, 16 GB, macOS, Metal through wgpu 30 |
| Build | `--release`, rustc 1.92.0 |
| Frame | 1080 x 2340 px at 2.8125 px per dp, the size of a Galaxy A27's screen |
| Screens | 32 screens of the example apps, 120 frames each, three runs |
| Date | 2026-11-17 |

| Measure | Run 1 | Run 2 | Run 3 |
| --- | --- | --- | --- |
| Building a frame, median screen (ms) | 0.038 | 0.034 | 0.036 |
| Building a frame, slowest screen (ms) | 0.080 | 0.083 | 0.086 |
| Drawing and reading back, median screen (ms) | 4.68 | 4.89 | 6.08 |
| An empty frame, drawn and read back (ms) | 1.81 | 1.87 | 1.77 |
| First frame of a screen, median (ms) | 8.43 | 7.43 | 8.57 |

What can be read from it:

- **Building a frame** (`view`, layout, text and painting into the display list, CPU only) took under a tenth of a millisecond for every screen in every run.
- **Drawing** includes copying the pixels back from the GPU, which a phone never does, and an empty frame accounts for about 1.8 ms of it. The machine was not quiet: two other builds were running, and the same screen measured 2.0 and 3.7 ms in different runs. Do not rank screens by these numbers.
- **Every screen is one render pass.** Screens without pictures are one draw call; screens with photographs are more.
- **Video**: the cost of uploading and converting a 720p frame was not resolved by this run.

The report, with every screen, is `benchmarks/reports/2026-11-17-host-baseline.md`. Run it again with:

```sh
cargo run --release -p morningstar-bench -- --markdown
```

## On a phone

One early session on a Galaxy A27 (2026-11-11) recorded that a frame is presented at the display's 120 Hz and that an idle screen draws nothing: one frame in six seconds. It is in `benchmarks/reports/2026-11-11-galaxy-a27.md`.

**Not measured on any phone:** frame time under scrolling or animation, memory, GPU load, battery, the cost of a video frame, and start-up time. The method for measuring is `benchmarks/android/measure.sh` and `benchmarks/README.md`.

## What the design does about it

These are properties of the design, not measurements:

- Frames are produced on demand; an idle app draws nothing.
- Text shaping is cached for as long as the same text keeps being shown.
- The layout tree, hit regions and display list are reused and stop allocating once they reach their working size.
- A video keeps only its latest frame.
