# 0016 — Text input

Status: Accepted (2026-11-11). Android: built, installed and typed into on a Galaxy A27 (Android 16, Samsung Keyboard); what that run did and did not cover is under "Verified". iOS: the Rust half type-checks; the Swift has never been compiled.

Lifts the limitations "No text input, focus or keyboard" and "No `request_redraw` from … timers" of [ADR 0013](0013-ui-layer.md), and supplies the two things [ADR 0012](0012-scale-insets-and-demand-rendering.md) left out on purpose: the keyboard inset ("reported separately when text input exists") and a frame requested for a future time ("a blinking cursor"). The reason [ADR 0003](0003-platform-hosts.md) gave for thin native hosts, that an input method needs a real `View`, is what this builds on. Cross-thread wake-ups are still not designed.

## Context

An application could draw a box that looked like a field, and nothing could be typed into it. Typing needs four things the framework did not have: a host view the platform's input method will talk to; a way for Rust, which decides what has focus, to raise and lower the keyboard; the keyboard's size, so content can get out from under it; and a frame at a future time, so a caret can blink in an app that otherwise draws nothing while idle.

## Decision

### Text input is declared per frame

Whether the keyboard is up is part of what a frame says, like its display list. `Frame::set_text_input(TextInputRequest { session, traits, text, caret })` in a frame means "this field has focus and holds this"; a frame that does not call it means "no field has focus". There is no `show_keyboard()` to forget to pair with a `hide_keyboard()`: a field that disappears with its screen takes the keyboard with it.

The engine keeps two values, what the last frame *declared* and what the host is *believed to hold*, and `Engine::take_text_input_command()` returns their difference, at most one command:

- `Show(request)`: no keyboard before, or another field (`session`), or other traits. The host restarts its input method with the new traits and text.
- `Update(request)`: same field and traits, different text or caret, and the difference did not come from the host. The host replaces what it holds.
- `Hide`.

Hosts call it after every frame. In the usual frame it returns `None`.

Input comes back as `TextInputEvent`s through `Engine::text_input`, which hands them to `App::text_input` (a new method with an empty default) and schedules a frame:

| Event | From |
| --- | --- |
| `Commit(text)` | iOS `insertText`; Android key events (hardware keyboard, `adb shell input text`, keypads that send keys) |
| `Compose(text)` | no host today; kept for hosts that report pre-edit text as text |
| `DeleteBackward`, `DeleteForward` | iOS `deleteBackward`; Android key events, and backspace in an empty field |
| `Enter` | the Return key as a key |
| `Action(ImeAction)` | the keyboard's action key |
| `Edited(TextEditState)` | Android's input method: the whole text, caret and composing range after its edit |
| `FocusLost` | the user put the keyboard away |

Two of these also update the engine's belief about the host: `Edited` (the host holds exactly that text and caret, so adopting it produces no `Update`), and `FocusLost` (the keyboard is already down, so no `Hide` follows).

All offsets in these types are byte offsets into UTF-8 text. The platform crates convert to and from the UTF-16 units Java and UIKit count in (`utf16_to_byte_offset`, `byte_to_utf16_offset` in `morningstar-core`).

### Android: the host keeps a copy and the input method edits it

Android's input methods do not send keystrokes. They read the text around the cursor, mark a region as "composing", replace it, move the selection, and expect the editor to hold all of that. An editor that only forwards "insert this" works until the first autocorrection, which is expressed as "make characters 4 to 9 the composing region, then set the composing text", and then duplicates words.

So the host keeps an `Editable` copy of the focused field (`TextSession.text`), and `HostInputConnection extends BaseInputConnection` in full-editor mode lets the platform's own code apply every operation to it: `commitText`, `setComposingText`, `setComposingRegion`, `finishComposingText`, `deleteSurroundingText`, `setSelection`. After each operation (or batch: `beginBatchEdit`/`endBatchEdit` are counted) the copy's state is compared with what Rust was last told and, if different, sent whole: `nativeTextEdited(serial, text, selStart, selEnd, composingStart, composingEnd)`, followed by `InputMethodManager.updateSelection`. `getExtractedText`, which `BaseInputConnection` does not implement and input methods use to read the field, is answered from the copy.

Rust is still the authority. The widget adopts an `Edited` state after applying its own rules (digits only, maximum length, no line breaks), the application may change or refuse the value in `update`, and whatever the field ends up holding is declared in the next frame. If that is not what the host holds, the engine's diff produces `Update`, and the host replaces its copy and calls `restartInput` so the input method forgets what it believed. This is the same division Flutter uses (`TextInput.setEditingState` one way, `updateEditingState` the other); the alternative of forwarding semantic operations and re-implementing composing regions in Rust was rejected as a second, subtly different, implementation of `BaseInputConnection`.

Details that matter:

- **The view.** `HostView extends SurfaceView` is focusable, answers `onCheckIsTextEditor()` with "a field has focus", and builds the `EditorInfo` in `onCreateInputConnection`: `inputType` from keyboard type, capitalisation, secure, multi-line and autocorrect; `imeOptions` from the action, always with `IME_FLAG_NO_FULLSCREEN | IME_FLAG_NO_EXTRACT_UI` (the app draws its own field, also in landscape) and `IME_FLAG_NO_ENTER_ACTION` for multi-line.
- **Key events** (`HostView.onKeyDown`, only while a field has focus): Delete, Forward Delete, Enter and Tab become commands; a key with a printable character becomes `nativeTextCommit`. These are applied in Rust, so each is followed by an `Update` and a `restartInput`. That is one restart per key for keyboards that type with key events (Samsung's phone keypad does); it works, and is more than it should cost.
- **Backspace in an empty field** changes nothing in the copy, so it would never be reported; `deleteSurroundingText` forwards it as a `DeleteBackward` command. A row of one-character boxes needs it to step back.
- **Stale edits.** Each `Show` carries a serial number; the host returns it with every `nativeTextEdited`, and the render thread drops an edit made under an earlier serial (the input method was still editing the copy of the field that has just lost focus).
- **Dismissal.** With a keyboard up, the input method consumes Back itself and the app is not told. The host notices the keyboard inset going from visible to hidden while a field has focus, ends the session and sends `FocusLost`. Without a soft keyboard (hardware keyboard), `onBackPressed` does the same instead of leaving the app. `onStop` also ends the session.
- **Passwords** are held in the copy as plain text, as in any `EditText`; `TYPE_TEXT_VARIATION_PASSWORD` tells the input method not to learn or suggest. Log lines carry lengths, never text.

### Rust asks the host through a JNI upcall

The application decides what has focus while building a frame, on the render thread; Android shows a keyboard only from its main thread. Rust must reach Java.

**Chosen: an upcall.** `MorningstarNative.onTextInputCommand(command, serial, keyboard, capitalization, action, flags, text, caret)` is a static Java method that posts to the main thread and returns. The render thread calls it after a frame when `take_text_input_command` has something.

Rejected: *a pull*, the main thread asking Rust "anything for me?" The main thread has no moment to ask at: frames are produced on another thread, on demand, and a focus change caused by a timer or by the application's own logic is preceded by no host callback. It would need a poll (waking an idle app) or a wake-up path from Rust to the main thread, which is the upcall again.

The mechanism, in `crates/morningstar-android/src/java.rs`, hand-declared like the rest of the FFI (ADR 0007):

1. `nativeInit` runs on the main thread with a `JNIEnv*` and the `MorningstarNative` class. `java::init` calls `GetJavaVM`, makes a global reference to the class, and looks up the method id. This is done there rather than in `JNI_OnLoad` because the class arrives as an argument (no `FindClass`, which from a native thread would search the wrong class loader) and because the library has no other need for an `OnLoad`.
2. The render thread calls `AttachCurrentThreadAsDaemon` once, at start, and keeps its `JNIEnv*` for good. It is never detached: the thread lives as long as the process, and a daemon attachment does not hold up JVM shutdown.
3. A command is one `NewString` (from UTF-16), one `CallStaticVoidMethodA` with an eight-element `jvalue` array, `DeleteLocalRef`, and `ExceptionCheck` (described and cleared if set).

No JNI crate: twelve functions are used (the ten used above, counting `ExceptionDescribe` and `ExceptionClear`, plus `GetStringLength` and `GetStringRegion` to read incoming strings), found by their fixed positions in the JNI function tables. The positions are constants in `java.rs` and were checked against `jni.h` of NDK 27 by script.

The unsafe boundaries, each documented where it is:

- A `JNIEnv*` belongs to one thread. `java::Env` wraps the render thread's and is `!Send`; the JNI entry points use the one they are handed.
- `JavaVM*`, the global class reference and the method id are process-wide and immutable; they sit in a `OnceLock` behind `unsafe impl Send + Sync`.
- A thread that never returns to Java never has its local references freed for it: the one string per call is deleted explicitly.
- A pending Java exception would abort the process at the next JNI call: it is cleared, and logged.
- `entry::<F>(table, index)` reads a function pointer from a table slot. Its soundness is exactly "the index and the signature are the specification's"; a compile-time assertion checks only that `F` is pointer-sized.

A code shrinker would remove or rename `onTextInputCommand`, which nothing in Java calls. The template does not shrink; a host that does must keep it.

### iOS: `UIKeyInput`, and a pull

On iOS the engine runs on the main thread and every change happens inside a call the host makes, so the existing pattern holds: after each call the host asks. `morningstar_ios_take_text_input(host, &out)` fills a small struct (command, keyboard, capitalisation, action, secure, multiline, autocorrect, has_text) when there is something to do.

`MetalView` adopts `UIKeyInput` (which includes `UITextInputTraits`): `insertText` → `morningstar_ios_text_insert` (a lone `"\n"` is the Return key), `deleteBackward` → `morningstar_ios_text_delete_backward`. `canBecomeFirstResponder` is true only while a field has focus. `Show` sets the trait properties and calls `becomeFirstResponder()` or, if already first responder, `reloadInputViews()`; `Hide` resigns. A resignation UIKit performs by itself (an iPad keyboard's dismiss key) is reported with `morningstar_ios_text_focus_lost`. The keyboard inset comes from `keyboardWillChangeFrameNotification`.

There is no host-side copy on iOS, so every edit is made in Rust and every edit produces an `Update`, of which the host uses only `has_text`.

**Not built: marked text.** Composition on iOS needs the full `UITextInput` protocol (text positions, ranges, a tokenizer, geometry queries), which is several hundred lines of Swift that cannot be compiled here. Without it, Chinese, Japanese and Korean input and dictation's live text do not work, and autocorrection is switched off (it could only append). Latin, Swahili and Arabic typing do not need it.

### The keyboard inset

`FrameInfo::keyboard_inset` is the height, in physical pixels, of the part of the surface the keyboard covers, measured from the surface's bottom edge (so it includes the gesture bar beneath the keyboard). It is separate from `insets`, as ADR 0012 said it would be. `Engine::set_keyboard_inset` produces a frame when it changes.

Android: `android:windowSoftInputMode="adjustResize|stateHidden"`. The window is edge to edge (`setDecorFitsSystemWindows(false)`), so nothing is resized; `adjustResize` is what makes the keyboard appear in the window insets on every API level. From API 30 it is `WindowInsets.Type.ime()`. Before that there is no keyboard inset type: the keyboard is included in the bottom system-window inset, so the host reports that as the keyboard when it exceeds the stable (navigation bar) inset by more than 80 dp, and reports the stable inset as the safe area's bottom meanwhile. The pre-30 path is written from documentation and has not been run. The inset arrives once per change, not animated; the app jumps while the keyboard slides (`WindowInsetsAnimation` is not used).

### Timed redraw

`Frame::request_redraw_after(delay)` asks for one frame after `delay`. Like `request_redraw` it lasts one frame: whatever frame comes next, the timer or an earlier one caused by input, clears it, and the app asks again if it still depends on time. The shortest delay asked in a frame wins.

The engine stores a deadline. `needs_frame()` is true once it has passed; `Engine::redraw_due_in()` is how long a host may sleep. A hidden app has no deadline (becoming visible draws anyway). `Engine::timed_frames()` counts frames that only the deadline caused.

- Android: the render thread, which blocked in `recv()` when idle, now waits with `recv_timeout(min(next idle log line, redraw_due_in))`.
- iOS: when the display link is paused and `morningstar_ios_redraw_delay()` is not negative, a one-shot `Timer` is armed; when it fires the host asks `needs_frame` again and the display link runs for one frame.

A blinking caret costs two frames a second and no continuous rendering.

### The editing model

`crates/morningstar-ui/src/edit.rs`: pure functions from (value, caret, composing range, event, field rules) to a new value. No widget or platform types.

- Offsets are bytes and always on **extended grapheme cluster** boundaries (`unicode-segmentation`, a direct dependency of `morningstar-ui` at the version cosmic-text already brings in). Backspace removes one cluster: an Arabic letter with its shadda and vowel, `e` + combining acute, a surrogate pair's code point. This applies to `DeleteBackward` events. Text an Android input method deletes itself is deleted the way that input method chooses (usually one code point); the field adopts the result.
- `insert` replaces the composing range if there is one, else inserts at the caret. `compose` does the same and marks what it inserted. `adopt` takes a host's whole state.
- Field rules, applied by all three: `digits_only` (ASCII 0-9), line breaks only when `multiline`, other control characters never, `max_length` in clusters (what does not fit is dropped), and `overwrite_when_full` for code boxes.
- A composition survives adoption only if the field kept the host's text unchanged. If a rule removed something, the composing range is dropped; the resulting `Update` restarts the input method.
- A secure field's display is one `•` per cluster; caret positions are mapped between value and display.

### The widget

`TextInput<M>` is a leaf, like `Text`: only the text. Padding, border and background belong to the `Container` around it, which keeps ADR 0013's "one box type" and lets a design's field be built from the same pieces as everything else. Three `Container` methods connect the box to the input inside it: `input_frame()` (a tap anywhere on the box focuses its first `TextInput`), `focused_border(width, color)` and `focused_background(color)`.

Unfocused, a `TextInput` lowers to the same measured text node and emits the same display-list operations as a `Text` in the same style (pinned by a test that compares the two lists). This is why FundiDesk's twelve fidelity numbers did not change in the fourth decimal when its look-alike fields became inputs.

Elm-style: the application owns the string. `TextInput::new(value).on_input(Message::Name)` shows `value` and delivers the *whole new string* per change; `on_submit(message)` is delivered by the action key. The handler is stored as `Arc<dyn Fn(String) -> M + Send + Sync>`, because a message has to be built per keystroke between frames. Consequence: **`Element::map` now requires its conversion to be `Send + Sync + 'static`** (and `M`, `N: 'static`). Enum variant constructors and functions, which is every use in the repository, satisfy it; a closure borrowing local state no longer does.

Behaviour:

- A tap on the input or its frame focuses it. The tap position is kept and turned into a caret position during the next frame, where the text engine is available: `TextContext::index_at` (new) maps a point to a byte offset on a cluster boundary, following right-to-left runs; `TextContext::caret` maps an offset to a position. Both are built from a caret map made on first use for a layout (one extra shaping of the focused field's text per edit; other texts pay nothing).
- A tap anywhere else, or pressing any other pressable, clears focus, except a pressable inside the focused field's own frame (a "show password" eye). A drag that scrolls does not.
- `Action(Next)` moves to the next `TextInput` in tree order, or clears focus on the last. Other actions clear focus and then deliver `on_submit`. `Enter` inserts a line break in a multi-line field and otherwise performs the field's action.
- `auto_advance()` with `max_length(n)`: filling the field moves to the next input; backspace in an empty one moves to the previous and deletes there.
- A single-line field never wraps; when the text is wider than the field it is clipped and scrolled sideways so the caret stays inside. A multi-line field wraps and grows downwards; it does not scroll internally.
- The caret is 1.5 dp wide, solid for 500 ms and absent for 500 ms, restarted by every edit and tap, driven by `request_redraw_after`. Composing text is underlined.
- Several events can arrive before a frame. Each input's value, rules and handlers are kept from the last frame and updated optimistically as events are applied, so three digits typed between two frames land in three code boxes.
- A value the application changes or refuses in `update` wins: at the next frame the field shows it, the caret is clamped, any composition is dropped, and the host is corrected.

**Identity and state.** Focus, caret, composing range, sideways scroll and blink phase are transient widget state in `Ui`, keyed by the input's identity under ADR 0013's rules, exactly as scroll offsets are: position in the tree, or `.id(..)`; kept while a widget with that identity is present in consecutive frames. An input that leaves the tree loses focus, and the keyboard goes with the next frame. Give inputs ids when fields can appear or disappear around them. The identity is also the `session` of the keyboard request.

### Keeping the field in view

Three mechanisms, chosen by where the field is:

1. **Bottom safe area includes the keyboard.** A box that pads for the bottom safe edge (`SafeArea`, `safe_area(SafeEdges::BOTTOM)`) pads for `max(bottom inset, keyboard inset)`. A screen built as "header, scroll view, bottom bar" therefore resizes like a native one: the bar's content rides above the keyboard and the scroll view shrinks. This is the behaviour for a fixed bottom button.
2. **A focused field inside a scroll view is scrolled into view** when it gains focus, when the keyboard inset changes and after each edit: its bottom plus 28 dp above the lower of the viewport's bottom and the keyboard's top, its top minus 20 dp below the viewport's top, top winning. A viewport that itself reaches under the keyboard (no safe-area box resized it) may scroll further by the overlap. Only the innermost scroll view is adjusted.
3. **A focused field not inside any scroll view** cannot be scrolled: while the keyboard would cover it, the whole screen is painted shifted up just far enough (never so far that the field passes the top inset). Touch regions come from what is painted, so taps still land.

## Consequences

- The host interfaces grew. JNI: `nativeSetKeyboardInset`, `nativeTextEdited`, `nativeTextCommit`, `nativeTextCommand` (15 native methods in all) and the first Rust-to-Java call, `onTextInputCommand`. C: `morningstar_ios_set_keyboard_inset`, `_text_insert`, `_text_delete_backward`, `_text_focus_lost`, `_take_text_input`, `_redraw_delay`.
- `morningstar-android` now contains JNI function-table access, where before it only passed `JNIEnv*` through.
- `Element::map` has stricter bounds (above).
- While a field has focus, each frame clones its value once for the request (compared before cloning in the engine, so an unchanged field costs one `String` per frame in the widget layer and none in the engine). A blinking caret makes that two per second.
- `App` has one more defaulted method; `FrameInfo` one more field (it is `#[non_exhaustive]`).

## Limitations

- **No selection**: no range selection, drag handles, long-press menu, copy, cut or paste (an input method's own paste arrives as text and works). The seam: `TextEditState` and `TextInputRequest` carry a caret where a range would go, the Android host already tracks `selectionStart`/`selectionEnd` and sends both, and `EditState` is where an anchor would be added.
- **No caret movement by key**: arrow keys, Home and End do nothing. The caret moves by tap and by the input method.
- **iOS has no marked text** (above), so no CJK input, and autocorrection is off there.
- **Autofill** (SMS codes, passwords) is not wired on either platform. A six-digit code pasted or autofilled into one code box keeps its first digit.
- **Multi-line fields do not scroll internally** and have no maximum height; they grow.
- **Digits-only means ASCII digits.** Eastern Arabic numerals are refused.
- **One restart of the input method per key event** on Android for keyboards that type with key events, and per hardware backspace.
- **The keyboard inset is not animated**: content jumps to its final place while the keyboard slides.
- **Scroll-into-view adjusts the innermost scroll view only**, and the pan for non-scrolling screens stops at the top inset: a field taller than the space above the keyboard is not fully visible.
- **A non-scrolling form squeezes** while the keyboard is up: its bottom safe padding grows, flexible spacers collapse, and what no longer fits overflows downwards under the keyboard. It is not visible there, but `Ui::overflows()` would report it for that frame.
- **Focus cannot be set or read by the application** (no `autofocus`, no `on_focus`/`on_blur` messages). `Ui::clear_focus` exists for hosts and tests.
- **Landscape and rotation with the keyboard up, split screen, and a hardware keyboard** are untested.
- **Android below API 30**: the inset fallback and `showSoftInput` path are unrun.
- **A pressable under a moving finger inside a focused field** and other gesture subtleties follow ADR 0013's minimal arbitration.

## Verified

On the development host (macOS):

- Engine: session diffing (show once, restart on another field or traits, no echo of the host's own edits, correction of a filtered edit, update after an app-side edit, no hide after a user dismissal), keyboard inset, and timed redraw (not before due, one frame when due, lasts one frame, dropped while hidden, shortest wins): 12 tests in `crates/morningstar-runtime/src/engine.rs`.
- UTF-16 conversion: 2 tests in `morningstar-core`.
- Caret map: left-to-right, right-to-left (Arabic), cluster boundaries for Arabic with marks, multiple and wrapped lines, empty text, no fonts: 8 tests in `crates/morningstar-text/tests/caret.rs`.
- Editing model: 16 tests in `edit.rs`, including grapheme deletion over Arabic with marks and Swahili.
- Scroll and pan arithmetic: 7 unit tests in `ui.rs`.
- Widget: 29 tests in `crates/morningstar-ui/tests/input.rs`.
- FundiDesk: 7 flow tests in `examples/fundidesk/tests/input.rs`; fidelity unchanged for all twelve screens.
- `cargo ndk -t arm64-v8a clippy -p morningstar-android` and `cargo clippy --target aarch64-apple-ios -p morningstar-ios` are clean.

On the Galaxy A27 (1080 × 2340, 2.8125 px/dp, Samsung Keyboard), from `adb logcat -s Morningstar`. The owner of the phone picked the app up and typed into it; that, not automated input, is what produced these lines:

- The upcall works: `text input: show keyboard serial=1 type=Phone action=Next …` on the render thread is followed 17 ms later by `host: keyboard requested serial=1 inputType=0x3` on the main thread, and 450 ms later by `keyboard inset 1008 px`.
- Key-event typing reaches the field: seven `text input: commit chars=1` lines, each followed by `update host text chars=N caret=N` with N from 1 to 7.
- The input method's own editing reaches the field: for a name field (`inputType=0xa001`: text, capitalise words, autocorrect) holding 10 characters, five `edited by the input method chars=11…15 caret=11…15 composing=false` lines, with no `update host text` in between (nothing was echoed back).
- A secure field (`inputType=0x81`) and a number field (`inputType=0x2`) were given their keyboards.
- The action key: `text input: action Done`, then `hide keyboard`, `host: keyboard hidden on request`, `keyboard inset 0 px`.
- The caret blinks without continuous rendering: `timed frames: 10 in 6.0s (caret blink; total timed=10 frames=55)` while a field had focus, 55 frames in the app's first 16 seconds including the keyboard appearing and seven keystrokes; and the `idle` lines resume when focus is lost.
- No crash or JNI abort in a session of about 660 frames across several pauses and resumes.

## Not verified

- **Composition on a device.** Every `edited` line observed had `composing=false`: Samsung Keyboard committed each character. Composing text, `setComposingRegion` (autocorrect of an earlier word) and swipe typing have not been seen to work on hardware. Gboard, which composes by default, has not been tried.
- **Dismissal by the user**: the Back gesture with the keyboard up, the keyboard's hide key, and `onBackPressed` without a soft keyboard. No `host: text input ended` line has been observed.
- **Scroll-into-view and the pan** on a device, by eye. The log shows the inset arriving; whether the focused field ends up comfortably above the keyboard was not looked at (no screenshot was taken).
- **The caret, the underline and the focused border as pixels** on a device.
- `adb shell input text`, backspace by key event, a hardware keyboard, the code boxes' auto-advance, the multi-line field, and the "show password" eye, on a device. All are covered by host tests only.
- **Everything on iOS**: the Swift is uncompiled.
- **Android API 24 to 35.**
- Power: two frames a second is argued from frame counts, not measured.
