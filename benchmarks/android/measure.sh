#!/usr/bin/env bash
# Collects the scripted Android metrics for the installed hello-gpu example
# from the single connected device. See benchmarks/README.md.
#
# The app draws on demand (ADR 0012), so this measures two things separately:
# that an untouched app draws nothing, and frame statistics in continuous
# mode. It sends no input to the device, but it does stop and relaunch the app
# several times, so do not run it on a phone someone is using.
#
#   benchmarks/android/measure.sh [runs]     # default: 3 cold starts
set -euo pipefail

app_id="dev.morningstar.hello"
activity="$app_id/dev.morningstar.host.MorningstarActivity"
runs="${1:-3}"
# 600 frames are needed for one stats line: 10 s at 60 Hz. Leave headroom.
settle_seconds=30
# The second idle line is written six seconds after the last frame.
idle_seconds=8

if [[ "$(adb devices | sed 1d | grep -c 'device$')" -ne 1 ]]; then
    echo "error: exactly one connected device is required" >&2
    exit 1
fi

echo "device:  $(adb shell getprop ro.product.manufacturer) $(adb shell getprop ro.product.model)"
echo "android: $(adb shell getprop ro.build.version.release) (API $(adb shell getprop ro.build.version.sdk))"
echo "soc:     $(adb shell getprop ro.soc.model)"
echo

echo "cold start (am start -W, TotalTime ms):"
for run in $(seq 1 "$runs"); do
    adb shell am force-stop "$app_id"
    sleep 2
    adb logcat -c # So the idle check below sees only the last run.
    total="$(adb shell am start -W -n "$activity" | awk -F': ' '/TotalTime/ {print $2}' | tr -d '\r')"
    echo "  run $run: $total"
done
echo

# Idle behaviour. The app has been left alone since the last cold start, so
# it must have stopped drawing: the render thread logs its frame count once
# when it has been idle for a second and again five seconds later. Equal
# counts are the evidence that nothing was drawn in between.
echo "idle (untouched app; the two frame counts must be equal):"
sleep "$idle_seconds"
adb logcat -d -s Morningstar | grep -E 'scale |insets |idle ' | sed 's/^/  /' || echo "  (no idle lines: the app is still drawing)"
echo "  cpu while idle (dumpsys cpuinfo):"
adb shell dumpsys cpuinfo | grep -F "$app_id" | sed 's/^ */    /' || echo "    (not listed: no measurable CPU use)"
echo

# Frame statistics need a steady stream of frames, which an idle app does not
# produce. Restart in continuous mode, where every refresh draws a frame.
adb shell am force-stop "$app_id"
sleep 2
adb logcat -c
adb shell am start -W -n "$activity" --ez dev.morningstar.continuous true >/dev/null
echo "rendering continuously for ${settle_seconds}s..."
sleep "$settle_seconds"

echo "memory (dumpsys meminfo, kB):"
adb shell dumpsys meminfo "$app_id" | grep -E 'TOTAL PSS|TOTAL RSS|Graphics:|Native Heap:' | sed 's/^ */  /'
echo

echo "renderer and frame statistics (logcat, continuous mode):"
adb logcat -d -s Morningstar | grep -E 'surface |continuous |stats ' | sed 's/^/  /' || echo "  (no statistics logged)"

# Leave the app as a user would find it: drawing on demand.
adb shell am force-stop "$app_id"
