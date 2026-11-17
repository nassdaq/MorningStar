# 0040 — A development preview: the app in a window on the Mac, reloaded when its source changes

Status: Accepted (2026-11-17). Verified on the development host, which is the only place it runs: see "What was verified". It is a development tool; nothing of it is linked into an app built for a phone.

Extends [0003](0003-platform-hosts.md) (thin native hosts that drive one `Engine`) with a third host, for the development machine, and [0008](0008-design-to-code-loop.md) (the loop from a design to code is run without a device) with a way to use the app, not only to look at one frame of it. Uses [0005](0005-application-entry-point.md) (the app is a library that registers itself), [0012](0012-scale-insets-and-demand-rendering.md) (frames on demand) and [0019](0019-effects-and-platform-services.md) (the host carries out the app's requests). Adds to the inventory of [0007](0007-dependency-policy.md).

## Context

Until now an app could be seen in two ways: as a PNG of one frame (`screenshot` examples, in seconds, without interaction), or on a phone (an APK build and an install, a minute or more, with a cable). Everything in between, tapping through screens, typing into a field, watching an animation, scrolling a list, talking to the app's server, needed the phone. The owner asked for a live preview.

## Decision

`tools/morningstar-preview` runs any Morningstar app in a window on the Mac and reloads it when its source changes:

```sh
tools/preview.sh barua-talk            # or: cargo run -q -p morningstar-preview -- barua-talk
```

It is two processes.

### The host: a third platform host

`morningstar_preview::host` is to the Mac what `MorningstarActivity` is to Android: it owns a native window (through `winit`), translates its events into calls on the same `Engine`, draws when `Engine::needs_frame` says so and sleeps otherwise, and carries out the app's service requests. The app cannot tell it from a phone except by what is answered `Unsupported`.

- **Surface.** `NativeSurface` gains one variant, `AppKitView` (an `NSView*`, `cfg(target_os = "macos")`), which `morningstar-gpu` hands to wgpu as a raw AppKit handle; wgpu makes the Metal layer. This is the only change to a framework crate's interface besides `Engine::last_frame` (below), and it stays inside the handle type, as the constitution asks. `winit` is a dependency of the preview crate alone, and only on macOS.
- **Place.** The window's place is kept in the window system's points (pixels do not mean the same on two displays of different density) and it comes back there on the next start; `--position` is for the first start only.
- **Size and scale.** The window is a phone: 390 x 844 dp by default (`--size`), a dp being a point of the Mac's display, so the engine's scale is the display's own (2 on a Retina display, which is also the scale the examples' design references are drawn at). `--scale` sets pixels per dp instead (2.8125 is the Galaxy A27). A scale that is asked for holds on any display (the window changes size in points instead). When no scale is asked for and the screen is taller than the display the window is on, everything is drawn smaller by one factor: the layout stays the phone's, which is the point; the pixels are fewer, and the terminal says so.
- **Insets.** `--insets` gives the safe area in dp; the default, 51 at the top and 21 at the bottom, is the status bar and gesture bar the examples' references are compared with. F4 turns them off and on. `--keyboard <dp>` raises an imitation of the soft keyboard's inset while a field has the focus (off by default: the Mac's keyboard covers nothing).
- **Touch.** The left mouse button is one finger. The wheel, or two fingers on the trackpad, is a second finger that drags: down under the cursor at the first wheel event, moved by what the wheel reports, lifted when the wheel has been still for 120 ms. An app scrolls by being dragged and has no notion of a wheel; this way every scroll view, sideways ones included, works unchanged, and the trackpad's own momentum (more wheel events) plays the part of the fling.
- **Keyboard.** A hardware keyboard is something the framework already takes ([0016](0016-text-input.md), [0025](0025-text-selection-and-clipboard.md)): typing becomes `TextInputEvent::Commit`, the editing keys their events, arrows `Move` with the Mac's conventions (Option by word, Command to the line's or the text's end, Shift to extend), Cmd+A/C/X/V/Z the clipboard and undo events. Text goes to the app only while it has a field focused (`take_text_input_command` said `Show`), as a phone's keyboard is only up then. Escape is the system's back, or, with a field focused, "keyboard away" (`FocusLost`), as on Android.
- **Services.** HTTP is really made, so an app with a server works against it. It is carried out by the system's `curl`, one process per request on its own thread: that brings the Mac's TLS, certificates, proxies and compression with no dependency (an HTTP client with TLS is some thirty crates), at a few milliseconds a request, which a development tool can pay. The request goes to `curl` as a configuration on its standard input, so a header such as `Authorization` never shows in the process list; a body goes through a private temporary file that is removed. Stored values are a file (below). The clipboard is the Mac's (`pbcopy`, `pbpaste`). Recording, playback and live rooms are answered `Unsupported` at once, and the terminal says so the first time each is asked for: sound and calls are tested on a phone.
- **Screenshot and script.** Cmd+S (or F2) saves what is on screen; `--screenshot-after <ms> <path>` does it from a script and quits. `--tap <x>,<y>`, `--type <text>`, `--back` and `--wait <ms>` are steps done in the window by itself, in order, half a second apart, through the code a person's input goes through (a tap is a finger of its own, so the real cursor moving over the window does not disturb it): a picture of a screen that is a few taps in is one command. The picture is the frame last presented, drawn again into memory by the same renderer (`Engine::last_frame`, added for this: the background, display list, atlas and images of the last frame). It is therefore exactly comparable with a design by `morningstar-diff`, and has no title bar or shadow in it.
- **Not imitated.** The engine has no notion of the system's light or dark appearance (an app chooses its own theme), so there is no key for it. "Reduce motion" is the engine's, and F3 toggles it. No screen reader is run, so accessibility is never turned on and `Announce` is dropped.

### The supervisor: build again, start again

`morningstar-preview <app>` (the binary) reads the app's identity from `cargo metadata` (as the build scripts do, [0020](0020-app-identity-and-launcher-icons.md)), writes a **runner** crate of two files under `target/preview/<app>/runner`, builds it, starts it, and watches the sources.

- **How the app gets in.** An app is a library; `morningstar::app!` exports `__morningstar_create_app` and there is no `main`. The runner is a binary crate that depends on the app and on the preview by path, names the app (`use app as _;`, which links it) and passes that function to `host::run`. The app is linked statically, by the same compiler, like any dependency. No app adds a line, and the tool has no list of apps. The runner is its own workspace (so it works for an app in any workspace), takes a copy of the workspace's `Cargo.lock` (so it builds the versions the workspace is locked to), and builds into the workspace's own `target` directory, where everything the app and the framework have already compiled in the dev profile is reused.
- **Loading the app's library at run time was rejected.** The app's `cdylib` could be opened with `dlopen` and replaced without restarting the process. But each library carries its own copy of the standard library and of every Morningstar crate; handing a `Box<dyn App>` across that boundary, and then calling it with an `Engine` compiled into another image, relies on two compilations laying out the same types the same way, which Rust does not promise, and state built by the old code would be read by the new. That is not sound, and the failure would be a crash in the tool people trust to show them their app.
- **Watching.** Four times a second the supervisor looks at two sets of files (a `stat` each, folded into one number; no file-event API, no dependency): every file the last build read, which Cargo lists beside the binary (`preview-<app>.d`: the sources of the app and of every path dependency, inside the workspace or not, and whatever they include, a font, a picture, the shader), and every `.rs` and `.toml` file of the workspace's packages, which catches what no build has read yet (a new module, a manifest, everything before the first build passes). Nothing else is looked at, so a server's log or database in the app's directory cannot start a build.
- **Reloading.** When a change has been quiet for 150 ms, the supervisor runs `cargo build` for the runner. Cargo decides what to rebuild. If the build passes and the binary changed, the running app is asked to quit (it saves where its window is), and the new one is started in the same place, without taking the keyboard focus from the editor. If Cargo found nothing to do, nothing is restarted. If the build fails, the old window stays up with "build failed" in its title, and the compiler's errors are in the terminal, where they can be clicked. A change saved while a build runs leads to another build after it.
- **State.** A restart is a restart: nothing in memory survives, so no stale cache and no value of a type whose fields changed. What survives is what would on a phone: the values the app stored, kept in `target/preview/<app>/state` and handed back at setup, with the installation id. An app that stores which screen it is on comes back there; one that does not starts at its first screen, as it does on a phone after being swiped away. `--fresh` (or Cmd+Shift+R in the window) forgets them: a first launch.
- `MORNINGSTAR_PREVIEW_DEBUG=1` makes the host print the displays, where the window landed, and each window and pointer event: for when the preview itself misbehaves.
- The app's exit code 75 means "start me again" (Cmd+R); the end of the supervisor's pipe means the supervisor is gone, and the window closes rather than linger as an old build.

## Dependencies (adds to 0007's inventory)

| Crate | Version | Used by | Why |
| --- | --- | --- | --- |
| `winit` | 0.30.13 (the newest stable; 0.31 is in beta) | `morningstar-preview`, macOS only | The window, its events and its handle. `default-features = false` with `rwh_06`: no X11 or Wayland. Brings the `objc2` family on macOS. Never linked into an app, a framework crate or `morningstar-devtools`. Its `raw-window-handle` (0.6.2) is the one wgpu 30 already uses. |

Deliberately absent: an HTTP client and a TLS stack (the system's `curl`), a file watcher (looking at modification times), a library loader (the app is linked, not loaded), an argument parser.

0003 said the hosts are native code and not a windowing framework. That stands for the phones, where the platform's own activity and view controller are the host. On the development machine there is no platform host to be thin over, and writing an AppKit window, its event loop and its keyboard handling by hand in Objective-C would be more code than the tool is worth.

## Costs and limits

- **Disk.** In the workspace's `target/debug`: `winit` and the crates it brings, second copies of the GPU and framework crates (with `winit` in the graph their features unify differently from a `-p` build of an app alone, so Cargo keeps both), and 45 to 50 MB for each previewed app's binary. Roughly 250 to 300 MB in all on this machine, read from the directory after the fact, not measured precisely.
- **Plain `http://` is not refused.** The phone hosts allow it only to the development machine; `curl` allows it anywhere. An app that works here against a cleartext address elsewhere will be refused on a phone.

- **macOS only.** The host is written against AppKit's handle and the Mac's `curl`, `pbcopy` and `pbpaste`. On another system the crate builds and `host::run` says so.
- **A reload is a rebuild.** It takes as long as `cargo build` takes for what changed (measured below), and the app starts over unless it stores its state.
- **Debug builds.** The runner is built in the dev profile, to share what is already compiled and to rebuild quickly. Frame times in the preview say nothing about a phone.
- **Not a phone.** The GPU is Metal on a Mac, the text input is a hardware keyboard, there is one finger and no pinch, no rotation, no system back gesture, no real soft keyboard, no notch, and the system bars are not drawn. Fonts and layout are the framework's own and are the same; what the platform decides is not.
- **No input-method composition.** Dead keys and composed input are not forwarded as `Compose`; typing is what the key produces.
- **What the watcher cannot know.** A file that no build has read and that is neither `.rs` nor `.toml` (a picture not yet included) starts nothing until the source that includes it is saved. Under the workspace's packages, directories named `target`, `build`, `out` and `node_modules`, and hidden files, are not walked.
- **The stored values are in `target/`**: `cargo clean` is a reinstall.

## What was verified

All on the development machine (an Apple M4, macOS, a Retina display at 2 pixels per point and an external one at 1), on 2026-11-17.

- **Tests**: 61 in `tools/morningstar-preview` (`cargo test -p morningstar-preview`): 53 unit tests (the command line and its round trip to the host, the fit of the window to a display, the runner's two files and the lock's copy, the workspace read from `cargo metadata`, the watcher and its debounce and Cargo's dependency information, the stored values and the installation id, the `curl` configuration and the reading of its output and exit codes, the services' answers, the mouse, the wheel and every key) and 8 that make real requests to a server on this machine written with `TcpListener` (a GET, a POST whose body is every byte value, the other methods and HEAD, error statuses, a redirect followed, no server, a hang-up, a timeout, URLs that are not HTTP, an answer over 10 MB).
- **Run**: `showcase` and `barua-talk` each built, opened a window, presented frames through Metal and saved their screen; the pictures were looked at (the showcase home, Barua Talk's welcome and home). On the Retina display the picture is 780 x 1688, on the other 390 x 844, and `--scale 2` there gives 780 x 1688.
- **Input, scripted**: in Barua Talk, a tap on the name field, typed text and a tap on Continue reached the home screen with the typed name, and the name was in the state file. While this was being tested the real cursor was moving over the window: its events arrived with coordinates inside the window (seen in the debug log), which is how the scripted tap came to need a finger of its own.
- **Services, in a running app**: a scratch app outside the repository stored a counter on each tap and showed it again on its next launch; its request to a file server on this machine came back as `200` with the file's text on screen, and the server logged it; with the server stopped the app was told `Offline`.
- **Reload**: with the scratch app running, a saved change of a string was built (2.0 s) and the app replaced (a new process; about 5 s from the save to the new window's first frame, measured in half-second steps); a syntax error left the old process running and put the compiler's error in the terminal; fixing it reloaded; killing the supervisor closed the window. A log file written into the app's directory no longer starts builds (it did, in a loop, before the watcher was narrowed).
- **Idle**: with Barua Talk's home screen up and nothing happening, the host used 0.01 s of CPU in 8 s and 125 MB of memory; the supervisor, watching 435 files, 0.32 s of CPU in 10 s.
- **The window's place** came back on both displays.
- `cargo fmt --all --check` and `cargo clippy -p morningstar-preview --all-targets` are clean.

## Not verified

- **A person's hand.** No real click, drag, wheel or key press was made: only the scripted steps, which share the mapping and the engine calls but not the window system's delivery. The keys (Cmd+S, Cmd+R, F1 to F5, Escape), the clipboard (`pbcopy`, `pbpaste`), resizing the window, dragging it to the other display and the trackpad's momentum are untried.
- **The window's own pixels.** The saved picture is the presented frame drawn again into memory, and the engine reported the frames as presented; this session may not capture the screen, so the window itself was not looked at.
- **Atlas and Barua Talk against their servers**: none was running. Barua Talk was run scripted.
- **Reload time for a real app**: only the scratch app's was measured; a change in an example could not be made without disturbing other work in the tree.
- **Any other machine**, an Intel Mac, a Mac without a second display, and every system that is not macOS.
