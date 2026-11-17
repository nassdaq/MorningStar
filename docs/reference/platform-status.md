# Platform status

What has actually been run, and where. "Verified in tests" means on the development machine; it is not a claim about a phone. The full table, with the evidence for every row, is kept with the [decision records](../architecture/README.md#verification-status); this page is its summary.

## Android

Run on **one phone**: a Galaxy A27, Android 16, Vulkan on an Adreno 710, 120 Hz.

| Seen on that phone | How |
| --- | --- |
| A frame is presented | First device session |
| Scale, insets, edge-to-edge and idling: one frame drawn in six idle seconds | From the log |
| The full shader (borders, gradients, shadows, clips, images, text) on Vulkan | By eye, against the desktop render |
| Text renders and matches the desktop render | By eye |
| Touch reaches the app | Taps changed the tally |
| The surface survives going to the background and back | Same session |
| Text input with the system keyboard: focus, keyboard type, Next and Done, keyboard avoidance | From the owner's own typing, by log |
| FundiDesk launches | From the log only; the screen was not inspected |

| Not yet seen on a phone | State |
| --- | --- |
| Text selection, the selection menu and the clipboard | Pass their tests on the development machine |
| System bar icon colours, and reduce motion | Pass their tests on the development machine |
| TalkBack reading and operating an app | The tree and the host's mirror are written and tested on the development machine |
| HTTP, storage, recording and playback | The Android side compiles; the two halves of the interface were compared |
| Live rooms: sound, cameras, screen sharing, data, audio routes, calls in the background | Pass their tests on the development machine; the Android side compiles and packages. Apps have been installed, but no room has been confirmed working on a phone |
| Android versions other than 16, and other GPUs | Never run. The host is built for API 24 and later |

## iOS

**Never compiled.** The shared code type-checks for `aarch64-apple-ios`. The Swift host, its services, the generated icon and build settings have not been through Xcode, so nothing has run on an iPhone or a simulator.

| Service | iOS |
| --- | --- |
| HTTP, storage, clipboard, recording, playback | Written in `HostServices.swift`; unverified |
| Live rooms | Answers `Unsupported` |

## The development machine

| What | State |
| --- | --- |
| The shared code builds and passes its tests | Verified on macOS |
| The renderer's pixel tests | Verified on macOS with Metal only |
| The live preview | Runs on macOS; driven by script, not yet by hand |
| CI on Linux | Formatting, lints, the tests that need no GPU, the Android lints and the servers' tests passed on their first run |
| CI on a hosted Mac (every test, on a GPU) | Started by hand only; has not run |

## How this page stays honest

The rule in `AGENTS.md` is to state what was and was not verified on a real device, and the verification table is updated with each change. When this summary and that table disagree, the table is right.
