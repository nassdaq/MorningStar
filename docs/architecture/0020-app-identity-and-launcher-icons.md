# 0020 — App identity and launcher icons

Status: Accepted (2026-11-12). The generator is verified by tests on the development host, and the Android half by building two APKs and reading them back with `aapt2`. Nothing was installed: how the icon looks on a home screen is not verified. The iOS half has never been through Xcode. Details under "Verified" and "Not verified".

Builds on the thin hosts of [ADR 0003](0003-platform-hosts.md) and the entry point of [ADR 0005](0005-application-entry-point.md), and adds to the development tool of [ADR 0008](0008-design-to-code-loop.md). No new dependency ([ADR 0007](0007-dependency-policy.md)).

## Context

Every Morningstar app installed with Android's default icon and a name made up by the build script ("Morningstar callready"). The application id was a rule in `tools/build-android.sh` with a special case for one example; the iOS script and `project.yml` were hard-coded to that example. An app had no way to say what it is called or what it looks like on a home screen, and the two platforms want that said in very different forms: Android a resource directory of fifteen pictures and three XML files, iOS an asset catalog and build settings.

## Decision

### An app declares its identity once, in its `Cargo.toml`

```toml
[package.metadata.morningstar]
name = "CallReady"                 # under the icon. Default: the package name
id = "dev.morningstar.callready"   # application id / bundle id. Default: dev.morningstar.<lib>
icon = "assets/icon.png"           # square PNG, 512 or more, best 1024, full-bleed.
                                   # Default: this path if the file exists, otherwise the built-in icon
icon_foreground = "assets/icon-foreground.png"   # optional Android adaptive foreground: square,
                                   # transparent, content in the middle 61% (66 of 108 dp)
icon_background = "#0E0E0E"        # optional colour behind the foreground.
                                   # Default: the icon's top-left pixel
```

Every key is optional, and so is the table. Paths are relative to the manifest.

Why there: the app crate is the application ([ADR 0005](0005-application-entry-point.md)); its manifest is the one file every app already has, it sits next to the code and the artwork, and `package.metadata` is the place Cargo reserves for tools. The alternatives were a second file per app (one more thing to find and keep in step), values in the app's Rust code (the hosts need them before any Rust runs, at packaging time), and arguments to the build scripts (which is what there was: the identity then lives in whoever types the command).

An unknown key is an error that names it and lists the five: a misspelt `icno` must not silently produce the default icon. So is a value that is not a string, an id that is not `name.name…`, a name that is empty or that a platform would misread, a colour that is not `#RRGGBB` or `#RGB`, and a file that is named but missing.

The iOS bundle identifier is the id with `_` written as `-`: Android allows the underscore (the default id of `services-check` is `dev.morningstar.services_check`) and iOS does not.

### The manifest is read by Cargo

`morningstar-icon` runs `cargo metadata --format-version 1 --no-deps` and takes five strings, the manifest's directory and the `cdylib` target's name out of the JSON with a small reader of its own (`tools/morningstar-devtools/src/json.rs`, about 250 lines: all of JSON, nesting limited, every error with its position).

The alternative was to read the `[package.metadata.morningstar]` table from the manifest's text with a strict reader for `key = "string"` lines. It was rejected because it would have been a second, partial definition of TOML: an inline table, dotted keys, a multi-line string, an escape or a `#` inside quotes would each have needed either support or a refusal, and the reader would still not know where a package is in the workspace or what its library is called when `[lib] name` differs from the package's. Cargo knows all of that and prints it in a format that is small to read exactly. A test writes a manifest with comments, both kinds of quote, an escape, an inline table and dotted keys, and reads it back through Cargo. The cost is one Cargo start (about 0.1 s) in scripts that go on to run Cargo anyway.

### One generator, in the development tool

`tools/morningstar-devtools` gains `Identity` (the declaration with defaults applied), `IconSet` (the pictures, checked) and the `morningstar-icon` binary:

```sh
cargo run -q -p morningstar-devtools --bin morningstar-icon -- <package> \
    [--android <res-dir>] [--ios <dir>] [--preview <file.png>] \
    [--print name|id|bundle-id|lib|icon]...
```

It uses what the crate had: its PNG reading and writing and `Image::resized`, which already averages areas in linear light with premultiplied alpha. An icon must be a square PNG of at least 512 pixels; anything else is refused with the file's name.

`--android <dir>` writes a resource directory:

| File | Size | For |
| --- | --- | --- |
| `mipmap-{mdpi,hdpi,xhdpi,xxhdpi,xxxhdpi}/ic_launcher.png` | 48, 72, 96, 144, 192 px | API 24 and 25: the icon, square and full-bleed |
| `mipmap-*/ic_launcher_round.png` | the same | API 25: the same, cut to a circle with an anti-aliased edge |
| `mipmap-*/ic_launcher_foreground.png` | 108, 162, 216, 324, 432 px | API 26+: the adaptive foreground on its 108 dp canvas |
| `mipmap-anydpi-v26/ic_launcher.xml`, `ic_launcher_round.xml` | | API 26+: `<adaptive-icon>` of background and foreground |
| `values/ic_launcher_background.xml` | | the colour `@color/ic_launcher_background` |

`--ios <dir>` writes `Assets.xcassets` with `AppIcon.appiconset` (one 1024 x 1024 PNG and the `Contents.json` of a single-size universal icon, from which Xcode 14 and later derive every size) and `Morningstar.xcconfig`. The PNG has no alpha channel at all, not merely opaque pixels, because the App Store refuses an icon that has one; a transparent icon is flattened onto the background colour first.

`--print` writes one value per line on standard output and nothing else goes there, so the build scripts take the name, the ids and the library name from the same reading of the manifest that made the icons.

`--preview` writes one picture of the icon in every form (square, round, adaptive under a circle and under a squircle, at 192, 96 and 48 pixels, the last also magnified) so it can be judged before it is on a phone.

Output is deterministic: the same input gives the same bytes. A file whose bytes would not change is not written, so its modification time stays and Gradle's resource tasks stay up to date. The set of file names is the same for every app, so switching apps leaves nothing stale.

### The single-icon rule

An app that gives only `icon` gets an adaptive icon made from it: **the foreground is the icon scaled to the 72 dp a launcher shows, centred on the transparent 108 dp canvas; the background is the colour of the icon's top-left pixel** (or `icon_background`).

The consequence: a launcher shows the icon exactly as drawn, cut by its mask. A circular mask removes the corners, as the round icon of API 25 does, so artwork that must survive every mask belongs in the middle 66 dp circle, which is 92% of the icon's width. The background colour is only seen when a launcher moves the layers against each other (the parallax of a drag), where it shows as a border in the icon's own corner colour. If the corner pixel is transparent there is no colour to take, and the tool says to set `icon_background`.

The alternative, scaling the icon to the 66 dp safe zone so that nothing is ever cut, makes every full-bleed icon a small picture inside a ring of its corner colour: wrong for the common case in order to be safe for the rare one. An app whose art needs it gives `icon_foreground`, which is used as it is (scaled to 108 dp).

Both examples with art show one route each. FundiDesk gives only `icon` (its logo on the brand green): the rule applies. CallReady gives a transparent foreground (its disc alone) over the app's black.

No `<monochrome>` layer (the themed icon of Android 13) is written for an app's own icon: a silhouette cannot be derived from a full-colour picture, and the alpha of a foreground like CallReady's is a filled circle. The built-in icon has one, because its foreground is a silhouette.

### The built-in icon

An app that declares no icon and has no `assets/icon.png` gets a four-pointed star (`|x|^0.6 + |y|^0.6 <= 1`) in pale gold on a dark field, drawn by the tool with eight-by-eight supersampling. `icon_background` alone recolours the field.

### Generated, not checked in

The template hosts contain no icon. `tools/build-android.sh` generates into `templates/android/app/build/generated/morningstar/res` and passes `-Pmorningstar.resDir=…` with the id, name and library name; `build.gradle.kts` adds that directory to the main source set's resources, and the manifest names `@mipmap/ic_launcher` and `@mipmap/ic_launcher_round`. `tools/build-ios.sh` (which now takes `MORNINGSTAR_APP`, as the Android script does) generates into `templates/ios/build/generated`, which `project.yml` includes. Both are under directories git already ignored.

Why not checked in: the hosts are templates shared by every app, and an icon in their sources would be either one app's or a second copy of each app's. What is checked in is each app's *source* icon under its own `assets/`, like its fonts.

Building the Android template without the script: `morningstar.resDir` defaults to where the script generates, so `./gradlew assembleDebug` uses the icons of the last scripted build. If there are none, the task `checkMorningstarIcons` (a dependency of `preBuild`) fails with a message that names the missing file, the script, and the command that generates the icons alone. It is a task and not a configuration-time check so that `./gradlew clean` and `tasks` still work with nothing generated.

The iOS settings travel by a generated `.xcconfig` that defines `MORNINGSTAR_BUNDLE_ID`, `MORNINGSTAR_PRODUCT_NAME` (the package name: safe in a path) and `MORNINGSTAR_DISPLAY_NAME`; `project.yml` sets `PRODUCT_BUNDLE_IDENTIFIER`, `PRODUCT_NAME` and `INFOPLIST_KEY_CFBundleDisplayName` to them. Variables of our own, rather than the settings themselves in the `.xcconfig`, because a target-level setting written by XcodeGen would silently win over the file.

### Icons drawn by the framework

`examples/callready/examples/icon.rs` and `examples/fundidesk/examples/icon.rs` draw each app's icon from its own art (the home screen's vinyl disc; the splash screen's logo) through the offscreen renderer and write `assets/icon.png`. For the transparent foreground the tool gains `screenshot_transparent`: a frame rendered over nothing instead of over the app's background, returned with straight alpha.

## Consequences

- An app's name, id and icon are in one reviewable place; the scripts have no per-app knowledge. `hello-gpu` declares `id = "dev.morningstar.hello"` and keeps the id it always had.
- **The default name changed**: an app that declares nothing is now called by its package name (`showcase`), not `Morningstar showcase`. All five examples declare a name.
- Each Android build starts the tool once (a debug build of it, about 2 s for an app with its own icon, most of it resampling 1024 x 1024 pictures unoptimised). Not measured further.
- The APK grows by the fifteen pictures: about 330 kB for CallReady, whose disc compresses poorly; flat art is far smaller.
- `morningstar-devtools` has a second, unrelated job. It stays one crate because the job needs exactly its `Image` and PNG code and nothing else, and because it is never linked into an app.

## Limitations

- One icon for all densities and both platforms; no per-density or per-platform artwork, no iOS dark or tinted variants, no monochrome layer for an app's own icon.
- The legacy square icon is full-bleed with square corners. Launchers of API 24 and 25 show it as it is.
- The display name is one string: no localisation.
- `id` is checked against Android's rule only. An id valid there and unusual for iOS (a segment that is a Swift keyword, say) is passed on.
- Version name and code are still fixed in `build.gradle.kts`.

## Verified

- 96 tests in `tools/morningstar-devtools` pass (31 of them new): every Android file at its size; the circular mask (clear corners, untouched inside, an anti-aliased edge, the area of a disc); the single-icon rule pixel for pixel at all five densities, and that under a circle it equals the round icon; a given foreground used as it is; the background from the corner pixel and from the key; the iOS icon's size, flattening in linear light and missing alpha channel; every validation error; determinism; unchanged files left alone (proved with a read-only file); the JSON reader; the manifest read through Cargo itself; the binary run on this workspace's examples.
- `MORNINGSTAR_APP=callready MORNINGSTAR_ABIS=arm64-v8a tools/build-android.sh` builds an APK whose `aapt2 dump badging` reports `package: name='dev.morningstar.callready'`, `application-label:'CallReady'` and `application: label='CallReady' icon='res/mipmap-anydpi-v26/ic_launcher.xml'`; whose resource table has `mipmap/ic_launcher`, `ic_launcher_round` and `ic_launcher_foreground` at mdpi to xxxhdpi, the two `anydpi-v26` XML files and `color/ic_launcher_background` = `#ff0e0e0e`; and whose pictures are byte for byte the generated ones.
- The same for `hello-gpu`: `dev.morningstar.hello`, `Morningstar`, the built-in icon with its `<monochrome>` element.
- Gradle run directly with an empty `morningstar.resDir` fails in `checkMorningstarIcons` with the intended message.
- The CallReady, FundiDesk and built-in icons were looked at in the preview sheet, at 1024 and down to 48 pixels, square, round and under both masks.
- `cargo test -p callready --test fidelity` passes unchanged with the disc made public (5 tests).
- `cargo fmt --all --check` is clean.

## Not verified

- **On a device**: nothing was installed. How the icon and the name look on a home screen, in the app drawer, in recents and in settings; the adaptive icon under a real launcher's mask and parallax; the themed icon of the built-in star on Android 13 and later; the legacy icons on API 24 and 25.
- **iOS, entirely**: no Xcode on this machine. Not verified: that `actool` accepts the generated catalog and the single-size `Contents.json`; that XcodeGen takes the catalog as a source path under `build/` and applies `configFiles`; that the three build settings resolve through the `.xcconfig` variables; that removing `bundleIdPrefix` has no other effect; `tools/build-ios.sh` with `MORNINGSTAR_APP` (the library name it passes to `xcodebuild -create-xcframework`); the display name on a device.
- The APK's size and build-time costs above are single observations, not benchmarks.
- **The closing checks of this change were not run**: `cargo clippy --workspace --all-targets -- -D warnings`, and the full `cargo test -p callready -p fundidesk` (only CallReady's fidelity test was). The development machine's disk fell under the agreed 600 MB of free space while the work was being finished, and building stopped there. Run them before relying on this.
