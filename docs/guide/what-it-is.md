# What Morningstar is, and is not

Morningstar is a mobile application framework written in Rust. An app is one Rust crate, shared by Android and iOS. The framework owns everything between your state and the pixels: the widgets, the layout engine, text shaping, and a renderer that draws through the GPU.

## What it is

- **One codebase in Rust.** State, logic and screens are written once. The Android and iOS projects are thin hosts that an app developer never edits.
- **Its own UI engine.** A screen is a tree of Morningstar's widgets, laid out by Morningstar's layout engine. Nothing is translated into the platform's views, so a screen looks the same on both platforms and in a test.
- **Drawn on the GPU.** Rectangles, borders, gradients, shadows, text, images and video go to the GPU as one display list per frame. [How rendering works](rendering.md) follows a frame from `view` to the screen.
- **Made for building from a design.** The widgets use the vocabulary designs are drawn in: auto-layout rows and columns, sizes that are fixed, hug or fill, a radius, a stroke, a shadow. A tool compares a rendered screen with the design's picture and says where they differ. See [Design to code](../design-to-code.md).
- **Testable with no phone.** An app never does I/O itself. It asks for work as data, and the answer arrives as a message. A test can therefore drive a whole app, taps and server answers included, on the development machine. See [Testing](testing.md).
- **Idle when nothing changes.** Frames are produced on demand. A screen that is not changing draws nothing.

## What it is not

- **Not a wrapper around native widgets.** There is no `UIButton` or Android `Button` underneath. Platform look-and-feel is whatever your screens draw.
- **Not a web view**, and there is no JavaScript anywhere in an app.
- **Not on iOS yet.** The shared code type-checks for iOS and the Swift host is written, but it has never been compiled. See [iOS](ios.md).
- **Not a desktop or web framework.** The [preview](../reference/tools.md#the-live-preview) runs an app in a window on a Mac for development only.
- **No hot reload on a phone.** The preview rebuilds and restarts the app on the Mac when a source file changes. On a phone you install a new build.
- **No right-to-left layout mode.** Arabic text is shaped and aligned correctly inside its own box, but rows do not mirror.
- **One finger.** There is no multi-touch, pinch or rotate gesture.
- **Not published.** There is no crate on crates.io and no version to depend on. Apps live in this repository's workspace, under `apps/` or `examples/`.

## Who decides what

The rules of the project are in `AGENTS.md` at the root of the repository: Rust first, an independently owned UI engine, GPU rendering as the foundation, one shared app codebase, platform code behind stable interfaces, performance that is measured before it is claimed. Each decision that followed from them has a record under [decision records](../architecture/README.md).
