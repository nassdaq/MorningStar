#!/bin/sh
# Runs a Morningstar app in a window on this Mac and reloads it when its
# source changes (ADR 0040).
#
#   tools/preview.sh barua-talk
#   tools/preview.sh showcase --size 360x800 --insets 0
#   tools/preview.sh --help
#
# The app is looked for from the directory this is called in, as Cargo
# does: inside this repository that is one of its packages; inside an app
# of another workspace it is that app (or give --manifest-path).
set -eu
root="$(cd "$(dirname "$0")/.." && pwd)"
exec cargo run -q --manifest-path "$root/Cargo.toml" -p morningstar-preview -- "$@"
