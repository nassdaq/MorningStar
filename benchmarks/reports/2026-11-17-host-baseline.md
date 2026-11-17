# Host baseline, 2026-11-17

What building and drawing each example screen costs on the development machine, with no phone. The method is `benchmarks/README.md`, "Host baseline"; the program is `benchmarks/host`.

**This is not a device measurement and says nothing about a phone.** It is a baseline for noticing regressions on this machine. Nothing here is a claim of performance.

## Machine and build

| | |
| --- | --- |
| Machine | Apple M4, 10 cores, 16 GB |
| OS | macOS 26.5.2 (25F84) |
| GPU, API | Apple M4 (integrated), Metal, through wgpu 30.0.1 |
| Toolchain | rustc 1.92.0, `--release` (thin LTO, `panic = "abort"`) |
| Commit | the working tree on top of `7dc0518` (uncommitted: ADRs 0040 to 0043 in progress) |
| Frame | 1080 x 2340 px at 2.8125 px/dp, insets 102 px top and 42 px bottom (a Galaxy A27's) |
| Samples | 120 frames a screen after 5 of warm-up; three runs |

**The machine was not quiet.** Two other builds were compiling on it while this ran (load average between 6 and 17 on 10 cores). The CPU figures (build) agree between the three runs to a few microseconds; the GPU figures (draw) do not, and are given with their spread for that reason. A run on an idle machine should replace this report before anything is compared with it.

## What was measured

- **Build**: the application's whole share of a frame, from nothing to a display list: `view`, layout, text (shaped once, then cached) and painting into the list. CPU only.
- **Draw + read back**: the renderer's share for that list, into an offscreen target, **including copying the 10 MB of pixels back from the GPU**, which a phone never does. An empty frame of the same size costs most of it; "less the empty frame" subtracts that row's median, and is the nearest this method gets to what the screen itself costs. It is a difference of two noisy numbers.
- **First frame**: build and draw of the first frame of a screen, which shapes its text, fills the glyph atlas and uploads its images. The first row's also holds the renderer's own first use.

## Results

Across the three runs:

| | Run 1 | Run 2 | Run 3 |
| --- | ---: | ---: | ---: |
| Build p50, median of the 32 screens (ms) | 0.038 | 0.034 | 0.036 |
| Build p50, the slowest screen (ms) | 0.080 | 0.083 | 0.086 |
| Draw + read back p50, median of the screens (ms) | 4.68 | 4.89 | 6.08 |
| Draw + read back p50, the slowest screen (ms) | 11.21 | 7.78 | 11.08 |
| An empty frame, drawn and read back, p50 (ms) | 1.81 | 1.87 | 1.77 |
| Less the empty frame, median of the screens (ms) | 2.87 | 3.02 | 4.31 |
| First frame, median of the screens (ms) | 8.43 | 7.43 | 8.57 |
| First frame, the slowest screen (ms) | 27.54 | 33.69 | 39.09 |

What can be read from that, and no more:

- Building a frame of any of these screens took under a tenth of a millisecond of CPU on this machine, in every run. The slowest was FundiDesk's repair details (403 instances).
- Drawing and reading back took between 2 and 11 ms depending on the screen and the run, of which about 1.8 ms is the empty frame. The spread between runs (the same screen at 2.0 and 3.7 ms) is larger than the difference between many screens, so screens are not ranked here.
- Every screen is one render pass. FundiDesk and Atlas screens are one draw call; screens with photographs or pictures are more (Barua Talk's home: 12), as ADR 0014 says they would be.

The second run, in full (milliseconds):

| Screen | First frame | Build p50 | Build p95 | Draw + read back p50 | p95 | Less the empty frame | Instances | Draw calls | Passes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| (an empty frame) | 7.88 | 0.000 | 0.000 | 1.87 | 2.60 | 0.00 | 0 | 0 | 1 |
| fundidesk Splash | 2.62 | 0.004 | 0.016 | 1.97 | 3.95 | 0.10 | 40 | 1 | 1 |
| fundidesk Onboarding | 6.42 | 0.030 | 0.044 | 7.03 | 7.66 | 5.17 | 204 | 1 | 1 |
| fundidesk Login | 8.60 | 0.029 | 0.053 | 5.29 | 6.50 | 3.42 | 173 | 1 | 1 |
| fundidesk CreateAccount | 5.15 | 0.029 | 0.041 | 4.35 | 5.06 | 2.48 | 217 | 1 | 1 |
| fundidesk Verification | 6.13 | 0.023 | 0.034 | 4.62 | 5.89 | 2.75 | 114 | 1 | 1 |
| fundidesk ShopSetup | 7.98 | 0.035 | 0.055 | 5.84 | 6.35 | 3.97 | 235 | 1 | 1 |
| fundidesk Dashboard | 9.67 | 0.064 | 0.083 | 7.07 | 7.39 | 5.20 | 365 | 1 | 1 |
| fundidesk RepairsList | 10.33 | 0.071 | 0.095 | 7.15 | 7.60 | 5.28 | 477 | 1 | 1 |
| fundidesk NewRepair | 8.91 | 0.063 | 0.083 | 6.91 | 7.33 | 5.04 | 311 | 1 | 1 |
| fundidesk RepairDetails | 10.45 | 0.083 | 0.121 | 7.78 | 8.17 | 5.91 | 403 | 1 | 1 |
| fundidesk RecordPayment | 7.96 | 0.056 | 0.067 | 7.08 | 7.40 | 5.21 | 329 | 1 | 1 |
| fundidesk Tracking | 9.49 | 0.057 | 0.074 | 7.48 | 7.97 | 5.61 | 416 | 1 | 1 |
| callready Welcome | 9.69 | 0.034 | 0.052 | 5.97 | 6.33 | 4.10 | 228 | 3 | 1 |
| callready Home | 33.69 | 0.036 | 0.056 | 3.99 | 5.42 | 2.12 | 142 | 5 | 1 |
| callready LiveCall | 6.43 | 0.046 | 0.070 | 5.84 | 6.58 | 3.97 | 284 | 1 | 1 |
| callready CallReport | 7.90 | 0.052 | 0.076 | 5.50 | 6.35 | 3.63 | 239 | 1 | 1 |
| callready Premium | 7.25 | 0.048 | 0.069 | 4.89 | 6.48 | 3.02 | 288 | 1 | 1 |
| atlas 01-onboarding | 5.20 | 0.017 | 0.034 | 3.19 | 5.83 | 1.32 | 138 | 1 | 1 |
| atlas 02-sign-in | 2.87 | 0.027 | 0.079 | 2.87 | 5.29 | 1.00 | 171 | 1 | 1 |
| atlas 03-home | 11.10 | 0.022 | 0.064 | 2.81 | 9.04 | 0.94 | 151 | 1 | 1 |
| atlas 04-chat | 2.97 | 0.037 | 0.097 | 5.13 | 7.10 | 3.26 | 209 | 1 | 1 |
| atlas 05-model-picker | 9.22 | 0.041 | 0.090 | 7.49 | 13.32 | 5.62 | 195 | 1 | 1 |
| atlas 06-live-voice | 4.16 | 0.011 | 0.017 | 3.35 | 4.14 | 1.48 | 93 | 1 | 1 |
| atlas 07-explore | 3.73 | 0.032 | 0.041 | 2.23 | 2.71 | 0.36 | 349 | 1 | 1 |
| atlas 08-plans | 3.61 | 0.028 | 0.074 | 2.21 | 3.42 | 0.34 | 347 | 1 | 1 |
| atlas 09-home-simplified | 7.07 | 0.038 | 0.115 | 5.18 | 10.71 | 3.31 | 151 | 1 | 1 |
| atlas 10-chat-simplified | 2.90 | 0.028 | 0.045 | 4.47 | 6.22 | 2.60 | 209 | 1 | 1 |
| barua-talk 01-home | 8.94 | 0.044 | 0.071 | 2.24 | 5.01 | 0.37 | 311 | 12 | 1 |
| barua-talk 02-room | 7.43 | 0.020 | 0.025 | 2.16 | 2.90 | 0.29 | 191 | 4 | 1 |
| barua-talk 03-incoming | 4.27 | 0.009 | 0.013 | 2.43 | 3.18 | 0.56 | 87 | 2 | 1 |
| barua-talk 04-video-call | 8.50 | 0.010 | 0.016 | 2.60 | 2.82 | 0.73 | 65 | 4 | 1 |
| barua-talk 05-phone-call | 4.34 | 0.016 | 0.020 | 2.00 | 2.52 | 0.13 | 133 | 1 | 1 |


## Video (ADR 0039), one 1280 x 720 I420 frame

| | Run 1 | Run 2 | Run 3 |
| --- | ---: | ---: | ---: |
| Copy out of the decoder's three planes into a reused frame, p50 (ms) | 0.022 | 0.020 | 0.027 |
| Draw + read back with the same picture, p50 (ms) | 4.52 | 2.34 | 5.43 |
| Draw + read back with a new picture each frame, p50 (ms) | 5.79 | 5.96 | 5.43 |
| The difference: upload and conversion (ms) | 1.27 | 3.63 | 0.00 |

- The copy of the 1,382,400 bytes took about 0.02 ms in every run: at 30 pictures a second, well under a millisecond a second of CPU on this machine.
- **The cost of uploading and converting a picture was not resolved.** It is the difference between two draws, and the three runs give 1.3, 3.6 and 0.0 ms: the noise of a busy machine is larger than the thing measured. It needs an idle machine, or a timer inside the renderer around the upload and the conversion pass, which does not exist yet.

## Not measured

Everything on a device: frame intervals, CPU per frame, GPU time, memory, battery and heat, for these screens and for a call with video; cold start; and anything on iOS. `benchmarks/android/measure.sh` is the device method, and its last report is `2026-11-11-galaxy-a27.md`, from before these screens existed.

## Reproduce

```sh
cargo run --release -p morningstar-bench -- --markdown      # about 20 s
```
