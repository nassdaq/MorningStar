#!/usr/bin/env bash
# Builds an example app as an XCFramework for the iOS host project, and
# generates its icon and names.
#
#   tools/build-ios.sh                          # hello-gpu
#   MORNINGSTAR_APP=callready tools/build-ios.sh   # another example
#   xcodegen generate --spec templates/ios/project.yml
#   open templates/ios/MorningstarHost.xcodeproj
#
# The app's name, bundle id and icon come from the
# [package.metadata.morningstar] table of its Cargo.toml (ADR 0020), and are
# written to templates/ios/build/generated, which project.yml refers to:
# run xcodegen again after building a different app.
#
# NOT VERIFIED: MORNINGSTAR_APP, the generated asset catalog and the
# generated build settings were written on a machine without Xcode. The
# generator's output is tested; that Xcode accepts it is not.
#
# Requires: full Xcode (not just the command line tools) and XcodeGen.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/templates/ios/build"
package="${MORNINGSTAR_APP:-hello-gpu}"
targets=(aarch64-apple-ios aarch64-apple-ios-sim)

if ! xcodebuild -version >/dev/null 2>&1; then
    echo "error: full Xcode is required (xcode-select points at the command line tools)." >&2
    exit 1
fi

cd "$root"
# Assets.xcassets (the AppIcon) and Morningstar.xcconfig (bundle id and
# names); prints the name of the Rust library. Files that have not changed
# are left alone.
lib_name="$(cargo run --quiet --package morningstar-devtools --bin morningstar-icon -- \
    "$package" --ios "$out/generated" --print lib)"

rustup target add "${targets[@]}"

libraries=()
for target in "${targets[@]}"; do
    # --crate-type staticlib: skip the cdylib, which only Android needs.
    cargo rustc --package "$package" --release --target "$target" --crate-type staticlib
    libraries+=(-library "$root/target/$target/release/lib$lib_name.a"
                -headers "$root/crates/morningstar-ios/include")
done

rm -rf "$out/Morningstar.xcframework"
mkdir -p "$out"
xcodebuild -create-xcframework "${libraries[@]}" -output "$out/Morningstar.xcframework"
echo "XCFramework: $out/Morningstar.xcframework"
