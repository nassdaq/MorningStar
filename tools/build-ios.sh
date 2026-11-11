#!/usr/bin/env bash
# Builds the hello-gpu example as an XCFramework for the iOS host project.
#
#   tools/build-ios.sh
#   xcodegen generate --spec templates/ios/project.yml
#   open templates/ios/MorningstarHost.xcodeproj
#
# Requires: full Xcode (not just the command line tools) and XcodeGen.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/templates/ios/build"
targets=(aarch64-apple-ios aarch64-apple-ios-sim)

if ! xcodebuild -version >/dev/null 2>&1; then
    echo "error: full Xcode is required (xcode-select points at the command line tools)." >&2
    exit 1
fi

cd "$root"
rustup target add "${targets[@]}"

libraries=()
for target in "${targets[@]}"; do
    # --crate-type staticlib: skip the cdylib, which only Android needs.
    cargo rustc --package hello-gpu --release --target "$target" --crate-type staticlib
    libraries+=(-library "$root/target/$target/release/libhello_gpu.a"
                -headers "$root/crates/morningstar-ios/include")
done

rm -rf "$out/Morningstar.xcframework"
mkdir -p "$out"
xcodebuild -create-xcframework "${libraries[@]}" -output "$out/Morningstar.xcframework"
echo "XCFramework: $out/Morningstar.xcframework"
