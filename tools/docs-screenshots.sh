#!/usr/bin/env bash
# Renders the example apps' screens for the documentation site, and the help
# text of every tool, so that neither can go stale.
#
#   tools/docs-screenshots.sh              every app
#   tools/docs-screenshots.sh atlas tools  only these (apps by package name;
#                                          `starter` is the generated app,
#                                          `tools` the help texts)
#   python3 tools/build-docs.py            then rebuild the pages
#
# Every picture is one frame through the real renderer on this machine's GPU
# (each app's own `examples/screenshot.rs`), scaled to 540 px wide and saved
# as a JPEG in site/shots/. Nothing is captured from a screen or a phone.
#
# Requires: a Mac (a GPU the renderer can draw on, and `sips` to scale).
set -euo pipefail
cd "$(dirname "$0")/.."

OUT=site/shots
WORK=target/docs-shots
WIDTH=540
QUALITY=78
mkdir -p "$OUT" "$WORK"

# shot <png> <name>: one picture into site/shots/<name>.jpg.
shot() {
    [ -f "$1" ] || { echo "docs-screenshots: $1 was not rendered" >&2; exit 1; }
    sips -s format jpeg -s formatOptions "$QUALITY" --resampleWidth "$WIDTH" "$1" --out "$OUT/$2.jpg" >/dev/null
}

# designed <package> <screen>...: screens of an app whose screenshot example
# takes their names, in the phone's frame.
designed() {
    local app=$1; shift
    rm -rf "$WORK/$app"
    cargo run -q -p "$app" --example screenshot -- "$WORK/$app" "$@" --phone >/dev/null
    for screen in "$@"; do
        # FundiDesk writes the design's tall artboard as <screen>.png and the
        # phone's frame beside it.
        if [ -f "$WORK/$app/$screen.phone.png" ]; then
            shot "$WORK/$app/$screen.phone.png" "$app-$screen"
        else
            shot "$WORK/$app/$screen.png" "$app-$screen"
        fi
    done
}

app_showcase() {
    rm -rf "$WORK/showcase"
    cargo run -q -p showcase --example screenshot "$WORK/showcase" 2 >/dev/null
    for screen in home login settings; do shot "$WORK/showcase/$screen.png" "showcase-$screen"; done
}
app_fundidesk() { designed fundidesk 03-login 07-dashboard 08-repairs-list 10-repair-details; }
app_callready() { designed callready 01-welcome 02-home 03-live-call 04-call-report; }
app_atlas() { designed atlas 01-onboarding 03-home 04-chat 06-live-voice; }
app_barua-talk() { designed barua-talk 01-home 02-room 03-incoming 04-video-call; }
app_services-check() {
    rm -rf "$WORK/services-check"
    cargo run -q -p services-check --example screenshot "$WORK/services-check" >/dev/null
    shot "$(ls "$WORK"/services-check/*.png | head -1)" services-check
}
app_hello-gpu() {
    rm -rf "$WORK/hello-gpu"
    cargo run -q -p hello-gpu --example screenshot "$WORK/hello-gpu" >/dev/null
    shot "$WORK/hello-gpu/idle.png" hello-gpu-idle
    shot "$WORK/hello-gpu/seven-taps.png" hello-gpu-seven-taps
}

# The app `morningstar-new` writes, generated into a throwaway workspace
# under target/ (so this repository's own manifest is never touched) and
# built into this repository's target directory.
app_starter() {
    local root=$PWD ws=$PWD/target/docs-starter
    rm -rf "$ws"; mkdir -p "$ws"
    cat > "$ws/Cargo.toml" <<MANIFEST
[workspace]
resolver = "3"
members = []

[workspace.package]
version = "0.0.1"
edition = "2024"
rust-version = "1.87"
license = "MIT OR Apache-2.0"
publish = false

[workspace.dependencies]
morningstar = { package = "morningstar-framework", path = "$root/crates/morningstar" }
morningstar-devtools = { path = "$root/tools/morningstar-devtools" }
MANIFEST
    cargo run -q -p morningstar-devtools --bin morningstar-new -- starter --name "Starter" --workspace "$ws" >/dev/null
    (cd "$ws" && CARGO_TARGET_DIR="$root/target" cargo run -q -p starter --example screenshot -- "$root/$WORK/starter" >/dev/null)
    shot "$WORK/starter/home.png" starter-home
    shot "$WORK/starter/settings.png" starter-settings
    rm -rf "$ws"
}

# The help of every tool, as the tool prints it, for docs/reference/tools.md.
app_tools() {
    local dir=docs/reference/help
    mkdir -p "$dir"
    for tool in morningstar-new morningstar-icon morningstar-diff morningstar-svg; do
        cargo run -q -p morningstar-devtools --bin "$tool" -- --help > "$dir/$tool.txt"
    done
    cargo run -q -p morningstar-preview -- --help > "$dir/morningstar-preview.txt"
    cargo run -q -p morningstar-cli -- --help > "$dir/morningstar.txt"
    cargo run -q -p morningstar-cli -- new --help > "$dir/morningstar-cli-new.txt"
    cargo run -q -p morningstar-cli -- build android --help > "$dir/morningstar-cli-build-android.txt"
}

ALL="showcase fundidesk callready atlas barua-talk services-check hello-gpu starter tools"
for what in ${*:-$ALL}; do
    declare -F "app_$what" >/dev/null || { echo "docs-screenshots: unknown: $what (one of: $ALL)" >&2; exit 2; }
    echo "docs-screenshots: $what"
    "app_$what"
done
du -sh "$OUT" | awk '{print "docs-screenshots: site/shots is " $1}'
