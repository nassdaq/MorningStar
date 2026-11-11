#!/usr/bin/env bash
# The only way anything in this repository may send input to, or capture the
# screen of, a phone connected over adb.
#
# The phone belongs to a person who may be using it. An automated tap once
# landed in a private app. So every single action here is preceded,
# immediately, by a check that the expected app is the resumed activity on
# top; if it is not, the action is refused and nothing is sent. There is no
# retry: the caller decides what to do, and should stop after a refusal or
# two rather than loop.
#
#   tools/adb-guarded.sh tap X Y            # one tap, in pixels
#   tools/adb-guarded.sh text 0712345678    # short text, as key events
#   tools/adb-guarded.sh key 67             # one key event (67 = backspace)
#   tools/adb-guarded.sh screencap out.png  # one screenshot, to a local file
#   tools/adb-guarded.sh check              # only the foreground check
#
# MORNINGSTAR_GUARD_PACKAGE: the package that must be in front
#   (default dev.morningstar.fundidesk).
# MORNINGSTAR_GUARD_LOG: where each action and refusal is recorded
#   (default: standard error only).
#
# Exit status: 0 done, 3 refused (the app is not in front), 2 bad usage.
set -euo pipefail

package="${MORNINGSTAR_GUARD_PACKAGE:-dev.morningstar.fundidesk}"
log_file="${MORNINGSTAR_GUARD_LOG:-}"

note() {
    local line
    line="$(date '+%Y-%m-%d %H:%M:%S') adb-guarded: $*"
    echo "$line" >&2
    if [[ -n "$log_file" ]]; then
        echo "$line" >>"$log_file"
    fi
}

# True only if the package's activity is the top resumed one right now. The
# line is `topResumedActivity=ActivityRecord{... u0 <package>/<activity> t123}`
# (`mResumedActivity` on older releases). A locked screen or the notification
# shade does not change it, so the device must also be awake and unlocked.
in_front() {
    local activities top
    activities="$(adb shell dumpsys activity activities 2>/dev/null)" || return 1
    top="$(grep -E 'topResumedActivity=|mResumedActivity:' <<<"$activities" | head -1)"
    [[ "$top" == *" $package/"* ]] || return 1
    # Awake and not showing the lock screen.
    local power window
    power="$(adb shell dumpsys power 2>/dev/null | grep -m1 'mWakefulness=')" || return 1
    [[ "$power" == *Awake* ]] || return 1
    window="$(adb shell dumpsys window 2>/dev/null | grep -m1 -E 'mDreamingLockscreen=|isKeyguardShowing=|mShowingLockscreen=' || true)"
    if [[ "$window" == *"mDreamingLockscreen=true"* || "$window" == *"isKeyguardShowing=true"* || "$window" == *"mShowingLockscreen=true"* ]]; then
        return 1
    fi
    return 0
}

guard() {
    if ! in_front; then
        note "REFUSED ($*): $package is not the resumed activity on an unlocked screen"
        exit 3
    fi
}

usage() {
    sed -n '2,24p' "$0" >&2
    exit 2
}

action="${1:-}"
case "$action" in
    check)
        guard check
        note "ok: $package is in front"
        ;;
    tap)
        [[ $# -eq 3 && "$2" =~ ^[0-9]+$ && "$3" =~ ^[0-9]+$ ]] || usage
        guard "tap $2 $3"
        adb shell input tap "$2" "$3"
        note "tap $2 $3"
        ;;
    text)
        # Letters, digits and a few harmless marks only: nothing the device's
        # shell could interpret, and short.
        [[ $# -eq 2 && "$2" =~ ^[A-Za-z0-9.,@_-]{1,24}$ ]] || usage
        guard "text (${#2} characters)"
        adb shell input text "$2"
        note "text (${#2} characters)"
        ;;
    key)
        [[ $# -eq 2 && "$2" =~ ^[0-9]{1,3}$ ]] || usage
        guard "key $2"
        adb shell input keyevent "$2"
        note "key $2"
        ;;
    screencap)
        [[ $# -eq 2 ]] || usage
        guard "screencap"
        adb exec-out screencap -p >"$2"
        note "screencap -> $2"
        ;;
    *)
        usage
        ;;
esac
