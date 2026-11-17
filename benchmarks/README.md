# Benchmarks

How Morningstar's performance is measured. Reports live in `reports/`, one file per run, named by date.

## Rules

- Release builds only (`--release`; the workspace profile uses thin LTO and `panic = "abort"`).
- Real devices. Emulator and simulator numbers are not recorded as results.
- Every report names the device, OS version, build commit and refresh rate.
- At least three runs per metric; report the median run and the spread.
- Device on battery saver off, fixed brightness, airplane mode, cooled to idle between runs.
- A change claims an improvement only against a report produced the same way on the same device.

## Metrics

| Metric | How | Status |
| --- | --- | --- |
| Frame interval (p50/p95/p99) | Built in: `Engine` records the time between presented frames over a 480-frame window | Instrumented |
| CPU time per frame | Built in: app `frame()` + command encoding and submission | Instrumented |
| Present time | Built in: time inside the present call, which may include waiting for the display | Instrumented |
| Cold start to first frame | Android: `am start -W` (`TotalTime`). iOS: Xcode Instruments, App Launch | Android scripted |
| Memory (idle) | Android: `dumpsys meminfo` PSS. iOS: Instruments, Allocations | Android scripted |
| Binary size | Size of the stripped `.so` per ABI, and of the release APK / IPA | Manual |
| Input-to-photon latency | Needs input handling (Phase 3) and a high-speed camera or WALT-style rig | Not started |
| GPU time | Android: AGI or Perfetto GPU counters. iOS: Metal System Trace | Not started |
| Energy | Android: Perfetto battery counters / `batterystats`. iOS: Xcode energy gauge, MetricKit | Not started |

The built-in statistics are written to the device log every 600 presented frames:

```
Morningstar: stats frames=600 interval[n=480 min=… p50=… p95=… p99=… max=…] cpu[…] present[…]
```

**Read the interval figures with care.** With vsync, the interval is quantised to the display's refresh period, so p50 will sit at 16.67 ms on a 60 Hz panel regardless of how fast Morningstar is. What the interval distribution reveals is *missed* frames (p95/p99 at two periods or more). The `cpu` figures are what show how much of the budget Morningstar itself uses.

**Rendering is demand-driven** (ADR 0012): an idle app produces no frames, and the interval statistic only covers back-to-back frames. To benchmark sustained rendering, launch in continuous mode (`am start … --ez dev.morningstar.continuous true`); `measure.sh` relaunches the app several times, so do not run it on a phone someone is using.

## Host baseline (no phone)

`benchmarks/host` (`cargo run --release -p morningstar-bench -- --markdown`) measures, on the machine it runs on, what a frame of every designed example screen costs in a phone's frame: the CPU time to **build** it (view, layout, text, paint into a display list), the time to **draw** it offscreen and read the pixels back, the same for an empty frame, the first frame of each screen, and for video the copy of a 720p I420 frame and a draw with and without a new picture. It prints the instances, draw calls and passes of each screen beside the times.

It is the exception to "real devices" above, and what it is for is narrow: **noticing a regression on one machine between two commits**. It is not a device measurement: another GPU, another driver, no compositor, no display, and a read-back no phone does. Rules for a report made with it:

- Release build, the machine idle (say so if it was not), three runs, the commit and the machine named.
- Report the build figures as they are; report the draw figures with their spread between runs, and never rank two screens whose difference is inside it.
- Compare only with a report from the same machine.

| Metric | How | Status |
| --- | --- | --- |
| Frame build time per screen (CPU) | `morningstar-bench` | Measured on the host: `reports/2026-11-17-host-baseline.md` |
| Offscreen draw + read back per screen | `morningstar-bench` | Measured on the host, noisy (the machine was busy) |
| Video: copy of a 720p frame | `morningstar-bench` | Measured on the host |
| Video: upload and conversion of a frame | `morningstar-bench`, by difference | Not resolved: inside the noise |
| Any of these on a phone | `android/measure.sh` | Not done for the designed screens or for video |

## Running

```sh
tools/build-android.sh --install
benchmarks/android/measure.sh            # startup, memory, 30 s of frame stats
```

iOS measurement is manual through Instruments until there is a device to script against.

## Comparisons

The roadmap calls for comparison against native Android, native iOS, Flutter and React Native. That is meaningful only once Morningstar can draw the same UI as the others (Phase 3). Until then the comparison would be a clear-colour app against a clear-colour app, which says nothing about a UI framework.
