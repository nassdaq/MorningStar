#!/usr/bin/env bash
# Builds an example app for Android and packages it into an APK.
#
#   tools/build-android.sh            # debug APK of hello-gpu, arm64 + x86_64
#   tools/build-android.sh --install  # also install and launch on the connected device
#   MORNINGSTAR_APP=showcase tools/build-android.sh --install   # another example
#
# Requires: the Android SDK + NDK (ANDROID_HOME), cargo-ndk, a JDK 17+.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
host="$root/templates/android"
package="${MORNINGSTAR_APP:-hello-gpu}"
lib_name="${package//-/_}"
# Each example gets its own application id so they install side by side.
case "$package" in
    hello-gpu) app_id="dev.morningstar.hello"; app_name="Morningstar" ;;
    *) app_id="dev.morningstar.${lib_name}"; app_name="Morningstar ${package}" ;;
esac

: "${ANDROID_HOME:?ANDROID_HOME must point at the Android SDK}"
if [[ -z "${ANDROID_NDK_HOME:-}" ]]; then
    # Newest complete NDK: a partially downloaded one has no source.properties.
    ANDROID_NDK_HOME="$(dirname "$(ls "$ANDROID_HOME"/ndk/*/source.properties 2>/dev/null | sort -V | tail -1)")"
    if [[ ! -f "$ANDROID_NDK_HOME/source.properties" ]]; then
        echo "error: no Android NDK found under $ANDROID_HOME/ndk" >&2
        exit 1
    fi
    export ANDROID_NDK_HOME
fi

cd "$root"
# arm64 for devices, x86_64 for emulators on Intel hosts. Override with e.g.
# MORNINGSTAR_ABIS="arm64-v8a" to build one, which halves build time and disk use.
abis=()
for abi in ${MORNINGSTAR_ABIS:-arm64-v8a x86_64}; do
    abis+=(--target "$abi")
done
# Start clean so an ABI left out this time does not ship a stale library.
rm -rf "$host/app/src/main/jniLibs"
cargo ndk \
    "${abis[@]}" \
    --platform 24 \
    --output-dir "$host/app/src/main/jniLibs" \
    build --package "$package" --release

cd "$host"
./gradlew --quiet assembleDebug \
    "-Pmorningstar.appId=$app_id" \
    "-Pmorningstar.appName=$app_name" \
    "-Pmorningstar.libName=$lib_name"
apk="$host/app/build/outputs/apk/debug/app-debug.apk"
echo "APK: $apk"

if [[ "${1:-}" == "--install" ]]; then
    adb install -r "$apk"
    adb shell am start -n "$app_id/dev.morningstar.host.MorningstarActivity"
    echo "Logs: adb logcat -s Morningstar"
fi
