# Testing

An app is tested on the development machine, with no phone and no server. That is possible because the app never does I/O: it asks for work as data, and a test can look at the request and answer it.

## The kinds of test

| Test | Shows | Where |
| --- | --- | --- |
| A journey | That tapping, typing and answers lead to the right state and screen | `tests/e2e.rs` |
| An accessibility check | That no screen has a control without a label, a small target or low contrast | `tests/accessibility.rs` |
| A picture of each screen | That a screen still looks as it did when you last accepted it | `tests/screens.rs` |
| A fidelity test | That a screen matches the design's picture to within a measured limit | `tests/fidelity.rs` in the designed examples |
| Pixel tests of the renderer | That each drawing operation produces the right pixels on a real GPU | `crates/morningstar-gpu/tests` |

```sh
cargo test -p my-app          # one app
cargo test --workspace        # everything, including the tests that draw on the GPU
```

## Answering in the phone's place

`Ui::testing(app)` holds every request the app makes instead of sending it. The test reads it and answers.

{{snippet examples/docs-snippets/tests/journey.rs#http}}

Stored values and the install id are given as the phone would have them:

{{snippet examples/docs-snippets/tests/journey.rs#host}}

| To play | Use |
| --- | --- |
| The server | `ui.pending_effects()` to read the request, `ui.answer_http(..)` to reply |
| The microphone and the speaker | `ui.answer_recording_started(..)`, `ui.answer_recording(..)`, `ui.answer_playback(..)` |
| A live room | `ui.live_room()` to see what was joined, `ui.live_event(..)` to send an event |
| Storage | `HostState::new().with_stored(key, value)` before, `ui.stored(key)` after |
| The system's Back | `ui.back()` |
| The user's "reduce motion" setting | `ui.set_reduce_motion(true)` |

## A finger and a keyboard

A journey taps where a widget was laid out and types through the same interface the keyboard uses. A generated app has a small `Device` in `tests/support/mod.rs` that does both:

```rust
let mut device = Device::fresh();
device.tap("open-settings");          // the widget with .id("open-settings")
device.tap("name");
device.type_text("Ada");
device.tap("save");
assert_eq!(device.app().name, "Ada");
```

Tapping by id sends real pointer events to where the widget was laid out, so layout and hit testing are part of every journey. `ui.rect_of(id)` is where a widget was laid out, `ui.visible_rect_of(id)` the part that shows, and `ui.overflows()` lists boxes whose children do not fit.

## Pictures

`tests/screens.rs` renders each screen and compares it with a reference picture in `tests/references/`. The first run writes the references; look at them and commit them. When a visual change is one you meant, accept it:

```sh
MORNINGSTAR_UPDATE_REFERENCES=1 cargo test -p my-app
```

A passing test does not show that a screen looks right, only that it has not changed. After changing anything visual, render it and look:

```sh
cargo run -p my-app --example screenshot -- out/
```

## Holding a screen to its design

The designed examples compare every screen with the design's own picture and fail if more than a measured share of pixels differs. Barua Talk's limits, with what was measured:

| Screen | Measured difference | Limit |
| --- | --- | --- |
| Home | 0.0847% | 0.10% |
| Room | 0.6900% | 0.70% |
| Incoming call | 0.0154% | 0.05% |
| Video call | 0.2860% | 0.30% |
| Phone call | 0.0499% | 0.05% |

[Design to code](../design-to-code.md) is the method, and `morningstar-diff` the tool.

## In CI

The workflow in `.github/workflows/ci.yml` runs formatting, lints, the tests that need no GPU, the Android lints and the servers' tests on every push. The job that runs every test on a GPU is started by hand. [ADR 0043](../architecture/0043-continuous-integration.md).

The API a test uses is in the [Ui reference](../reference/ui.md).
