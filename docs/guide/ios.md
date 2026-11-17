# iOS

**iOS does not work yet.** This page says exactly what exists, so that nobody is surprised.

## What exists

- **The shared code builds for iOS.** `cargo check -p morningstar-ios --target aarch64-apple-ios` passes, so the application crate, the UI layer and the renderer type-check for the platform.
- **The host is written.** `templates/ios` holds a UIKit host in Swift, the C header it shares with `crates/morningstar-ios`, and `tools/build-ios.sh`, which builds an app as an XCFramework and generates its icon and names.
- **Both halves are kept in step.** When the Android interface changes, the C header, the Rust side and the Swift are changed with it.

## What does not

- **Nothing has been compiled with Xcode.** The Swift host has never been built, so nothing has ever run on an iPhone or a simulator: no frame, no touch, no keyboard.
- **Live rooms answer `Unsupported`.** The iOS host refuses every join honestly. Recording, playback, HTTP and storage are written in `HostServices.swift` and are equally unverified.
- **The generated icon and build settings** have been tested as files; that Xcode accepts them has not.

## What it would take

A machine with full Xcode and XcodeGen:

```sh
MORNINGSTAR_APP=my-app tools/build-ios.sh
xcodegen generate --spec templates/ios/project.yml
open templates/ios/MorningstarHost.xcodeproj
```

Expect the first build to need fixes. Every row of the verification table that says "never compiled" is a thing to check; see [Platform status](../reference/platform-status.md).
