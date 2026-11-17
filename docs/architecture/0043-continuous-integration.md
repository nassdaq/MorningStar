# 0043 — Continuous integration: what runs where, and what it costs

Status: Accepted (2026-11-17). **The workflow has never run.** It was written from the actions' and runner images' own documentation as read on that day and checked to parse; everything about its behaviour on a runner is unverified until its first run, and is listed below.

## Context

Nothing checked a push. The host tests, the fidelity tests, the three servers' tests and the Android type-check were run by hand, package by package, on one machine whose disk is too small to run them together. The repository is private, so runner minutes are paid for.

## Decision

One workflow, `.github/workflows/ci.yml`, four jobs.

| Job | Runner | When | What |
| --- | --- | --- | --- |
| `rust` | Linux | every push to `main`, every pull request | `cargo fmt --check`; `cargo clippy --workspace --all-targets -- -D warnings`; the tests of `morningstar-core`, `-layout` and `-text` |
| `android` | Linux | the same | `cargo ndk -t arm64-v8a clippy` of the seven example apps, with the runner image's own NDK. The Rust half only: no Gradle, no APK |
| `servers` | Linux, one per server | the same | `npm ci && npm test` for Atlas, CallReady and Barua Talk. No `.env` exists, so the tests against a real database, model or Talk API skip themselves; the rest run on in-memory fakes |
| `gpu` | macOS (Apple silicon) | pushes to `main`, and by hand | `cargo test --workspace`: every test, the renderer's pixel tests and the examples' fidelity tests included |

**Why the tests are split.** The pixel tests are the specification of the renderer and have only ever been verified on Metal. A Linux runner has no GPU; a software Vulkan could be installed, but whether its pixels match the references is unknown, and a job that fails for the rasteriser's reasons teaches nobody anything. So Linux runs only the crates whose tests draw nothing (types, layout, text shaping and rasterising on the CPU), and everything that builds a renderer runs where it is known to pass. A macOS minute costs about ten times a Linux one, so that job does not run for pull requests.

**The toolchain is pinned in the workflow** (`RUSTUP_TOOLCHAIN: 1.92.0`, the one everything was last verified with) although `rust-toolchain.toml` says `stable`: with `-D warnings`, a new compiler's new lint would fail a push that changed nothing. Moving the pin is a change of its own.

**No secret is used**, and the workflow's token can only read the repository. A job that needed a key would be a different decision.

**Third-party actions: none.** `actions/checkout`, `actions/cache` and `actions/setup-node` are GitHub's own; the toolchain comes from `rustup`, which the runner images carry, and `cargo-ndk` from `cargo install` at a pinned version (4.1.2, the one used here).

## What it costs

Prices as published on 2026-11-17: Linux 2-core $0.006 a minute, macOS $0.062 a minute, each job rounded up to a whole minute. **The durations below are guesses** from how long the same commands take on the development machine, not measurements; the first runs will say.

| Job | Guess, cold cache | Guess, warm | At the guess |
| --- | ---: | ---: | ---: |
| `rust` | 10 to 15 min | 3 to 6 min | under $0.10 |
| `android` | 6 to 10 min | 2 to 4 min | under $0.06 |
| `servers` (three) | 1 min each | 1 min each | $0.02 |
| `gpu` | 15 to 25 min | 8 to 15 min | $0.50 to $1.55 |

So a pull-request push should cost cents, and a push to `main` most of a dollar, nearly all of it the macOS job. If that is too much, the `gpu` job's trigger is the one line to change (a schedule, or by hand only).

## Not verified

- That the workflow is accepted by GitHub and each job starts. It parses as YAML with the structure a workflow has; no workflow linter was available.
- The action versions (`checkout@v7`, `cache@v6`, `setup-node@v7`) are the ones their READMEs show; none was run.
- That the Linux image builds the whole workspace for clippy without extra system packages. `wgpu` and the text stack need none to compile; the preview tool (ADR 0040) opens a window and may.
- That `ANDROID_NDK_HOME` on the runner is an NDK `cargo-ndk` accepts (the image documents 27.3 as its default; 27.1 is what was used here).
- That the pixel and fidelity tests pass on a hosted macOS runner's GPU as they do on an M4: a different Apple GPU or a paravirtual one may differ in a pixel. If they do, that is a finding about the tolerances, not a reason to delete the job.
- Every duration and so every cost.
- Nothing here builds an APK, runs on a device or an emulator, or compiles the Swift host.
