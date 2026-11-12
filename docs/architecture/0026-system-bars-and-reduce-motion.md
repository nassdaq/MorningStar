# 0026 — System-bar content colour and "reduce motion" from the platform

Status: Accepted (2026-11-12). Verified on the development host only. The Rust type-checks for Android and iOS and the Java compiles against the API 36 SDK; neither the bar icons nor the setting has been looked at on a device. The Swift has been parsed and never compiled.

Closes the known defect "Status-bar icon colour" of the verification table (white icons on a light screen) and the host wiring [ADR 0021](0021-viewport-wrap-scroll-animation-navigation.md) left open ("the hosts do not report the setting"). Follows the two patterns already in use: something the application says about a frame is declared per frame and the host is told the difference ([ADR 0016](0016-text-input.md)); something the platform knows is set on the engine and arrives in `FrameInfo` ([ADR 0012](0012-scale-insets-and-demand-rendering.md)).

## Context

The surface is drawn edge to edge under transparent system bars. The system draws its icons in one colour unless told otherwise, and it cannot see what the application paints behind them: FundiDesk, a light design, had white status-bar icons on a near-white screen.

Separately, `Ui::set_reduce_motion` existed and nothing but tests called it.

## Decision

### System bars: declared per frame, light or dark content

`morningstar-core` has `SystemBars { status: BarContent, navigation: BarContent }` with `BarContent::{Light, Dark}` (the colour of the icons, not of the screen), `SystemBars::LIGHT_CONTENT`, `SystemBars::DARK_CONTENT`, and `SystemBars::for_background(color)`: dark icons when the colour's relative luminance (WCAG's, on the linear components a `Color` already holds) is above 0.179, the point where black and white text contrast equally with it.

- **Low level**: `Frame::set_system_bars(bars)`. A frame that does not call it leaves the bars as they are. `Engine::take_system_bars()` returns the value when it differs from what the host was last told, and `None` otherwise; hosts call it after every frame, as they call `take_text_input_command`.
- **UI layer**: `Application::system_bars(&self) -> Option<SystemBars>`, asked once per frame. `None`, the default, means `SystemBars::for_background(theme.colors.background)`. An application overrides it for a screen whose edges are not the theme's background (a photograph under the status bar, a dark sheet over a light app), or to treat the two bars differently.

So FundiDesk (background `#F5F6F1`-like, light) gets dark icons and CallReady (`#0E0E0E`, dark) light ones with no line of application code, and a theme switched at run time switches the bars with the frame that shows it.

A method on `Application` rather than a property of the root element: it is a fact about the screen, not about a box, and it keeps `Element` free of something that is not layout or paint.

**Android.** A fifth upcall, `MorningstarNative.onSystemBars(boolean darkStatusIcons, boolean darkNavigationIcons)` (`(ZZ)V`), posted to the main thread, where `MorningstarActivity.applySystemBars` does:

| API | Call |
| --- | --- |
| 30+ | `WindowInsetsController.setSystemBarsAppearance` with `APPEARANCE_LIGHT_STATUS_BARS` and `APPEARANCE_LIGHT_NAVIGATION_BARS` |
| 26 to 29 | `SYSTEM_UI_FLAG_LIGHT_STATUS_BAR` and `SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR` on the decor view |
| 23 to 25 | the status-bar flag only; the navigation bar's icons stay light |
| below 23 | nothing: both stay light |

The bars stay transparent and the window edge to edge, as before. The engine outlives an activity and only reports changes, so `MorningstarNative` keeps the last value in two static fields and a newly created activity applies it in `onCreate` (`restoreSystemBars`).

**iOS.** `morningstar_ios_take_system_bars(host, &out)`, polled after every call like the keyboard; the view controller keeps the answer, returns it from `preferredStatusBarStyle` (`.darkContent` or `.lightContent`) and calls `setNeedsStatusBarAppearanceUpdate`. The home indicator adapts by itself. **Uncompiled.**

### Reduce motion: from the platform into `FrameInfo`

`Engine::set_reduce_motion(bool)` (a change produces one frame, a repeat nothing) and `FrameInfo::reduce_motion`. `Ui` uses `forced || info.reduce_motion` every frame, where `forced` is what `Ui::set_reduce_motion(true)` sets: the system's setting is followed without the application doing anything, and an application's own "less motion" switch can still force it on. `Viewport::reduce_motion` tells a view, as before.

- **Android**: `nativeSetReduceMotion(boolean)`. The host reads `ValueAnimator.areAnimatorsEnabled()` (API 26+; `Settings.Global.ANIMATOR_DURATION_SCALE != 0` before) in `onCreate` and `onStart`, and while started observes `Settings.Global.getUriFor(ANIMATOR_DURATION_SCALE)` with a `ContentObserver`. "Remove animations" in the accessibility settings and a duration scale of zero in the developer options are the same setting.
- **iOS**: `morningstar_ios_set_reduce_motion`, from `UIAccessibility.isReduceMotionEnabled` at start and on `reduceMotionStatusDidChangeNotification`. **Uncompiled.**

## Alternatives considered

**The host samples the frame** under each bar and decides. Works without the application, costs a read-back per change, and is wrong while a screen slides in.

**Bars as a property of the theme** (`Theme::system_bars`). The default already comes from the theme; a field would have to be set by every theme literal, and a screen that differs from its theme would still need the override.

**A scrim**: translucent bar backgrounds so that either icon colour reads. Changes every design's top edge; rejected for the same reason the bars are transparent at all.

**Reduce motion as an `App` method** (`fn reduce_motion(&mut self, bool)`). `FrameInfo` already carries what the platform knows about the display, and a low-level app reads it there with no new trait method.

## Consequences

- JNI: `nativeSetReduceMotion` and the `onSystemBars` upcall; C: `morningstar_ios_set_reduce_motion`, `morningstar_ios_take_system_bars`, `MorningstarSystemBars`. A host older than the library is told nothing about the bars and says so once in the log; a library older than the host fails to link `nativeSetReduceMotion` (an `UnsatisfiedLinkError` at start): both halves ship together, as before.
- `Application` has one more defaulted method; `FrameInfo` one more field (it is `#[non_exhaustive]`); every `Ui` frame calls `set_system_bars`.
- An application that drew its own dark strip under the status bar of a light theme must now say so with `system_bars`, or its icons turn dark.
- `Ui::set_reduce_motion(false)` no longer forces motion on when the system asks for less.

## Limitations

- **One colour per bar**, for the whole bar. No per-region contrast.
- **Android 23 to 25 navigation bar** and everything below 23 keep light icons; an application with a light bottom edge there can pad it with a darker bar itself.
- **Three-button navigation with a translucent system scrim** (the default on some devices when contrast is enforced) is left as the platform draws it.
- **Flings are not reduced** (ADR 0021): they follow the finger.
- **The animator scale is treated as on or off.** A scale of 0.5 or 2 does not change animation durations.

## Verified

On the development host (macOS) only:

- `SystemBars::for_background`: white, black, mid grey, saturated blue, yellow, NaN (1 test in `morningstar-core`).
- Engine: the host is told once per change and a silent frame leaves the bars alone; reduce motion reaches `FrameInfo`, a change produces exactly one frame and a repeat none (2 tests in `engine.rs`).
- UI: the bars follow the light and the dark theme and the application's override, including different bars; reduce motion from `FrameInfo` makes an animated scroll instant, and the application's own switch holds when the host says otherwise (2 tests in `crates/morningstar-ui/tests/selection.rs`).
- FundiDesk's theme background gives dark content and CallReady's light, by the rule above; both examples' suites and fidelity figures are unchanged.
- Android and iOS halves: as listed in [ADR 0025](0025-text-selection-and-clipboard.md), checked in the same runs.

## Not verified

- **On an Android device**: that FundiDesk shows dark status-bar and navigation icons and CallReady light ones, at launch and after the activity is recreated (rotate with auto-rotate on, or change the display size); that a theme change at run time changes them; gesture navigation and three-button navigation; that the log line `system bars: status icons dark, navigation icons dark` appears once and not per frame.
- **Reduce motion on a device**: with "Remove animations" on, CallReady's screen changes and scroll glides are instant; switching it while the app is open takes effect (the `ContentObserver`), and `reduce motion on` appears in the log.
- **Android API 23 to 29** (the flag path) and below 26 for the setting.
- **Everything on iOS.**
