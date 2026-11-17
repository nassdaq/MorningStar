# Examples

The repository's own apps. Each one exists to show something, and each is tested on the development machine. The pictures are frames rendered by the app's own screenshot program, with no phone.

| App | Shows | Screens | Server |
| --- | --- | --- | --- |
| [Starter](starter.md) | What `morningstar-new` writes: the smallest complete app | 2 | Optional |
| [Showcase](showcase.md) | Realistic screens written with the UI layer alone | 3 | No |
| [FundiDesk](fundidesk.md) | A whole design reproduced: twelve screens held to their pictures | 12 | No |
| [CallReady](callready.md) | Illustrated screens: turned elements, vector pictures, text effects | 13 | Yes, port 8787 |
| [Atlas](atlas.md) | An AI assistant: glows, blur, a bottom sheet, a real chat | 10 | Yes, port 8788 |
| [Barua Talk](barua-talk.md) | Rooms, chat and calls with sound and pictures in a live room | 5 designed, more of its own | Yes, port 8789 |
| [services-check](services-check.md) | One button per platform service: a smoke test for a phone | 1 | No |
| [hello-gpu](hello-gpu.md) | The low-level API: painting and hit-testing by hand | 1 | No |

## Running any of them

```sh
cargo test -p atlas                                   # its tests
cargo run -p atlas --example screenshot -- out/       # its screens as PNG files
tools/preview.sh atlas                                # live in a window on a Mac
MORNINGSTAR_APP=atlas MORNINGSTAR_ABIS=arm64-v8a tools/build-android.sh --install
```

The apps with a server also run without it: they fall back to a scripted version of themselves, which is what the tests and the screenshots use.

## Which to read first

Read **FundiDesk** to see how a design becomes screens: its `SCREENS.md` is the method. Read **Starter** for the shape of a new app, and **Barua Talk** for an app with a server, effects and a live room.
