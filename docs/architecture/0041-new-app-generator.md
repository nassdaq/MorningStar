# 0041 — A new app from one command: `morningstar-new`

Status: Accepted (2026-11-17). Verified on the development host: the generator's tests, a generated app's own tests and screenshots, its server's tests, and one Android build of a generated app. **No generated app has run on a device.**

Uses [0005](0005-application-entry-point.md) (the app crate is the native library), [0013](0013-ui-layer.md) (Elm-style state), [0019](0019-effects-and-platform-services.md) (effects, and tests with no device), [0020](0020-app-identity-and-launcher-icons.md) (identity declared once in `Cargo.toml`), [0032](0032-accessibility.md) (the accessibility check), [0038](0038-live-rooms.md) (opt-in services) and [0007](0007-dependency-policy.md). The preview a generated app is told about is [0040](0040-development-preview.md)'s.

## Context

Every app so far (`examples/showcase` to `examples/barua-talk`) was started by copying another one and removing what was not wanted. That is slow, it carries the copied app's design decisions into the new one, and it is where things go missing: a test file, the icon, the workspace entry, the `crate-type` the Android build needs. The owner asked for the framework to be scaffolded: starting an app should be one command, and what it leaves should already work.

## Decision

### 1. The command

A binary in `tools/morningstar-devtools`, beside `morningstar-icon`, with the logic in the library (`scaffold.rs`: `NewApp`, `create_app`, `add_workspace_member`) so that it is tested as a library and usable by another tool:

```sh
cargo run -q -p morningstar-devtools --bin morningstar-new -- \
    my-app --name "My App" --id com.example.myapp [--services live,live-video] [--server [--port 8790]] [--dir apps]
```

Only the package name is required. `--name` defaults to the package in words ("My App"), `--id` to `dev.morningstar.<package>` (the default ADR 0020 already gives), services to none, the directory to `apps`. `--workspace <dir>` names a workspace other than the one the command is run in.

It needs no new dependency. The devtools crate already had what it uses: PNG encoding for the icon, and the identity rules.

### 2. What it writes

A crate that is complete in the sense of AGENTS.md, not a stub:

| File | What it is |
| --- | --- |
| `Cargo.toml` | The package, `[package.metadata.morningstar]` with name, id, icon and services, `crate-type = ["cdylib", "lib"]`, and a dependency on `morningstar` alone (`morningstar-devtools` for tests) |
| `src/lib.rs` | `app!`, a `Model`, a `Message`, two screens (Home, Settings) on a `Navigator` with its transition, the system's Back, a theme with the app's colour, a framed text input with an accessibility label, and one effect used the way effects are meant to be: a name stored with `Effects::store` and read at `setup` |
| `tests/e2e.rs`, `tests/support/mod.rs` | The journeys made with taps and keys through `Ui::testing`: typing and saving the name, a name stored on an earlier run, Back |
| `tests/accessibility.rs` | Both screens pass `accessibility_issues()`, and the controls are announced as what they are |
| `tests/screens.rs` | The screens against `tests/references/*.png` |
| `examples/screenshot.rs` | The screens as PNGs, as every example has |
| `assets/icon.png`, `assets/fonts/` | A launcher icon and Noto Sans with its licence |
| `README.md`, `SCREENS.md`, `design/README.md` | The commands, the screens and their controls, and where a design's references go and how to compare with them |

With `--server` it also writes `server/` (below), `src/json.rs`, and the app gets a second effect: a button that sends `POST /v1/hello` and shows the answer or, in a sentence, why there was none.

The generated app uses fixed names for its own types (`Model`, `Message`, `Screen`) rather than names made from the app's. Two reasons: the code reads the same in every app, and no line's length depends on the app's name, so the generated crate passes `cargo fmt --check` whatever it is called. A test generates the app with the longest names allowed and checks every line; another run of `rustfmt` over real output is in "Verification".

### 3. Where: `apps/`, not `examples/`

`examples/` holds the framework's own demonstrations. Each is in the table in AGENTS.md, most are the proof of an ADR (a design reproduced, a service exercised), and their fidelity tests are part of what the framework is held to. An app somebody starts is theirs: it has its own purpose, its own pace, and it may leave. Putting it among the examples would make the examples table a list of unrelated products and make "the examples pass" mean something different every week.

So a new app goes to `apps/<package>`, made on first use, and is added to the workspace's `members`. `--dir examples` (or any directory inside the workspace) is there for the case where a new framework example is what is being started.

It is a workspace member because that is the only way an app can depend on Morningstar today: by path, through `morningstar.workspace = true`. The framework is not published and has no versioned release (`publish = false`), and making it usable from another repository is a separate decision that was not asked for. The build scripts take a package name and never a path, so they need no change: `MORNINGSTAR_APP=my-app tools/build-android.sh` works for `apps/my-app` as it does for an example.

### 4. Templates kept in the binary

The templates are files under `tools/morningstar-devtools/templates/app/`, compiled in with `include_str!`. They are the generated files themselves with two additions: `{{name}}`-style values, and `@@if server` / `@@else` / `@@end` lines around what only one of the two variants has. Nothing else is interpreted, which keeps a template readable as the file it becomes.

Kept in the binary rather than read from disk at run time: the command then works from any directory and cannot find half a template set. The fonts (1.2 MB, Noto Sans Regular and Bold with `OFL.txt`) are the copies in `examples/hello-gpu`, included the same way; a development tool's size is not a cost to an app.

The alternative of generating by copying an example and rewriting it was rejected: an example changes for its own reasons, and a generator that depends on the text of `examples/services-check/src/lib.rs` breaks silently when that file is improved.

### 5. Checked before anything is written

- The package name: lower-case letters, digits and single hyphens, starting with a letter, at most 40 characters, and not the name of a crate the app is built from (`morningstar`, `morningstar-*`, `core`, `std`, `test`, ...).
- The name and the id: by the functions that read them back from `Cargo.toml` (`identity.rs`), so that what the generator accepts is exactly what the icon tool and the build scripts accept.
- The services: known names, and `live-video` and `live-screen` only with `live`.
- The directory: relative, inside the workspace, no `..`.
- The place: the directory must not exist, and the workspace must not already have a package of that name, in any directory.
- The workspace's `Cargo.toml` must have a `members` list the new entry can be added to. This is worked out before the first file is written, so a manifest that cannot take the member leaves no half-made app.

Every refusal says what to change and exits with status 2; nothing is overwritten, ever. The member is added by a textual edit that leaves every other byte of the manifest as it was (the comments in that file are worth keeping), and a list written on one line is written out one member a line.

### 6. The first test run writes the pictures

Existing examples commit their reference pictures, and a missing reference fails until the tests are run once with `MORNINGSTAR_UPDATE_REFERENCES=1`. The generator cannot commit pictures: it does not link the UI layer or a GPU, and a picture rendered on another machine's GPU would not be the reader's. A new app whose first `cargo test` fails is a bad first minute.

So the generated `tests/screens.rs` writes a reference that does not exist and says so, and compares from then on. The first run passes and leaves two PNGs to look at and commit. This is a property of the generated test, not a change to `assert_matches_reference`, whose rule (a missing reference is a failure) stays right for a repository whose references are committed.

### 7. The icon

Drawn by the generator on the CPU and written as `assets/icon.png`, 1024 x 1024: a four-pointed star in white on the app's colour, inside the middle 61% that every Android mask keeps, so ADR 0020's tool makes every launcher form from it without an adaptive foreground. The colour is one of six, chosen by a hash of the package name, and is the theme's `primary` in the generated `src/lib.rs`; each of the six carries white text at 4.5 to 1 or more, which a test computes, so the generated buttons pass the accessibility check in any of them. It is a placeholder that is not the same placeholder for every app: two generated apps on one phone can be told apart.

### 8. The server skeleton

`--server` writes a Node server with no dependencies in the shape `examples/barua-talk/server` settled on: settings from the environment and then `.env` (`config.js`), the pipeline in `app.js` (optional bearer token compared in constant time, `X-Device-Id`, a rate limit per device, one error shape, a log line that is method, path, status and time and never a header or a body), a health check, and one example resource (`POST` and `GET /v1/hello`, a count per device kept in memory). `API.md` is the contract; `test/app.test.js` runs the server through real HTTP on a free port with a clock the test owns.

No secret is generated or defaulted. `.env.example` names the settings with the token empty; `.env` is never written and is ignored by a `.gitignore` in `server/` as well as by the repository's.

The app's side is `SERVER_URL` (the development machine, or an address given at build time through `<APP>_SERVER_URL`, which keeps a deployed address out of the repository, as Atlas and Barua Talk do) and `src/json.rs`: writing one JSON string and reading one, with its limit stated and a pointer to a whole reader for when the app needs lists and numbers. The framework has no JSON type and this record does not add one.

No database: which one, and whether there is one, is the app's decision. The skeleton says where it would go.

## What this costs

- **Templates are a second copy of how an app is written.** When the UI layer's API changes, `templates/app/` must change with it. What catches it: `tests/new_app.rs` generates an app and runs its tests on every run of the devtools tests, so a template that no longer compiles fails there. That test runs `cargo` inside a test (about 10 s with a warm target directory, which it shares with the repository).
- **The first screens are not a design.** They are plain widgets in the default theme. `design/README.md` says where a design goes; nothing generated claims fidelity to anything.
- **One layout of app.** A single `src/lib.rs`. The examples that grew (`screens/`, `components.rs`, `theme.rs`) show where to go next; the generator does not offer a second shape.
- **`apps/` lives in this repository.** Until Morningstar can be depended on from outside, an app's history is the framework's history.

## Not done

- iOS: the generated app is the same crate the iOS host would link, and `tools/build-ios.sh` takes it by name, but nothing of iOS has been built on this machine, for this app or any other.
- No generator for a screen, a widget or a service inside an existing app.
- No removal command. Removing an app is deleting its directory and its line in the root `Cargo.toml`.
- The server skeleton has no deployment recipe.

## Verification

On the development host (macOS, Metal), 2026-11-17:

- `cargo test -p morningstar-devtools --lib scaffold`: 15 tests. Names and ids refused with their reasons; services; directories and ports; the file list of each variant; no directive or value left unfilled; no generated line of Rust over 100 characters with the longest names; a quoted name escaped where it is inside a string; the six colours' contrast with white; the icon's pixels and its safe zone; the member edit on a multi-line list, a one-line list, a list without a trailing comma and manifests with none.
- `cargo test -p morningstar-devtools --test new_app`: 2 tests. The command run as a program in a throwaway workspace: an app with a quoted name, an id, a service and a server is created; its identity is read back through `cargo metadata` and `Identity`; a second run and a run to another directory are refused; **the generated app's 11 tests pass on first run** and write both pictures; the pictures then hold; the server's tests pass under `node --test`. Ten refusals each exit 2 with their reason and write nothing.
- `rustfmt --check` over generated apps with the shortest (`ab`), an ordinary and the longest (40 and 60 characters) names, with and without a server: clean.
- A generated app's screens rendered with its `screenshot` example and looked at, and its icon looked at. The first look found the Settings screen blank (a screen pushed onto the navigator was still sliding in at the frame a screenshot takes), which the tests had not: `Model::on` now opens on the screen itself.
- One generated app built for Android with `tools/build-android.sh`: see `docs/architecture/README.md`.

Not verified: anything on a device.
