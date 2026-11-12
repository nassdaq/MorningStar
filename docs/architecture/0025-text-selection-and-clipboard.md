# 0025 — Text selection, caret keys, the clipboard and undo

Status: Accepted (2026-11-12). Verified on the development host only (tests, and the selection looked at as pixels in both themes). Nothing here has run on a device: the Rust type-checks for Android and iOS, the Java compiles against the API 36 SDK, and no finger has long-pressed a field on a phone. The Swift has been parsed and never compiled. What that leaves open is under "Not verified".

Lifts the limitations "No selection" and "No caret movement by key" of [ADR 0016](0016-text-input.md), using the seam that record left (`TextEditState` and `TextInputRequest` carried a caret where a range would go). The clipboard is a platform service in the style of [ADR 0019](0019-effects-and-platform-services.md). Scrolling under a dragged handle uses the scroll state of [ADR 0021](0021-viewport-wrap-scroll-animation-navigation.md).

## Context

A `TextInput` had a caret that a tap placed, and nothing else a person expects of a field: no way to select a word, correct the middle of a number without retyping it, paste a code from a message, or move with the arrow keys of a keyboard. On Android an input method could move its own selection in the host's copy of the text, and Rust ignored the half of it that made it a selection.

## Decision

### The selection is the framework's; the application still sees only `on_input(String)`

Selection is transient widget state in `Ui`, next to the caret, by the identity rules of ADR 0013: `EditState` gained an `anchor` (the end of the selection that stays put; the caret is the end that moves). Nothing about it reaches the application, which keeps receiving the whole new string per change. Outside access is for tests and for an application that wants a field's contents selected: `Ui::selection() -> Option<(usize, usize)>`, `Ui::select(id, range) -> bool`, and for tests `Ui::selection_menu_shown()`, `Ui::selection_handles()` and `Ui::selection_menu()` (where the controls were drawn).

All offsets stay bytes on extended-grapheme-cluster boundaries. Every editing function of `edit.rs` now takes the selection into account: `insert` and `compose` replace it, backspace and forward delete remove it and nothing more, `adopt` takes the input method's (and drops it if a field rule changed the text under it).

### Gestures

Pointer events carry no time. The clock is the frames' (`FrameInfo::elapsed`), and a frame is drawn after every touch, so:

| Gesture | Recognised | Does |
| --- | --- | --- |
| Tap | finger up on the field | caret at the nearest character boundary; any selection collapses, handles and menu go |
| Double tap | a second tap on the same field within 400 ms of frame time and 24 dp | selects the word there (Unicode word boundaries, UAX #29, via `unicode-segmentation`); handles and menu |
| Triple tap | a third | selects everything |
| Long press | finger still inside the field and within 8 dp of where it landed after 500 ms. The frame after the touch asks for one more at that time (`request_redraw_after`), so an idle app wakes once for it | selects the word under the finger, handles and menu; lifting the finger afterwards is not a tap. On whitespace or in an empty field: the caret and a menu with Paste |
| Handle drag | finger down within 12 dp of a handle | that end follows the finger, character by character; the ends never meet; the menu is hidden while dragging and returns on release |

A password's bullets have no words: a double tap or long press there selects all of it.

A touch is turned into a character with the text engine's `index_at`, and then refined: the engine answers with an edge of the glyph under the point, and a glyph can be several characters (Noto Sans draws "fi" as one). `index_near` in `ui.rs` also tries the character either side and takes the one whose caret is drawn nearest the finger. Before this a tap could not put the caret between the letters of a ligature.

### What is drawn

- **Highlight**: the theme's primary colour at 40% (Material's text-selection colour), behind the text, one rectangle per visual line and one per run on a line that mixes directions. The rectangles come from `TextContext::selection_rects` (new, the one addition to `morningstar-text`), built from the same caret map as `caret` and `index_at`, so they follow right-to-left runs and partial ligatures. Edges are snapped to whole pixels so lines meet without a seam.
- **Handles**: two 20 dp drops in the primary colour hanging from the foot of the line at each end, a disc with one square corner on the text. The start's leans left and the end's right; in text that reads right to left (first strong letter Hebrew or Arabic) the other way. A handle whose end is scrolled out of the field is not drawn.
- **Menu**: a row of labels (Cut · Copy · Paste · Select all) on the theme's surface colour with a hairline and a shadow, 8 dp above the selection's first line, or below its last line and handles when there is no room under the status bar; kept 8 dp inside the screen and above the keyboard. Only the items that apply: Cut and Copy need a selection and a field that is not secure; Cut and Paste need `on_input`; Select all needs something not yet selected. With nothing to offer it is not drawn.
- **Caret**: not drawn while something is selected, and no blink frames are asked for then.

Handles and menu are painted after the whole tree (and after leaving widgets), outside every clip, with their own touch regions on top of everything else. They are not drawn for a field inside a turned or scaled widget (its highlight is); see Limitations.

An unfocused field is untouched: it takes the same early return as before and emits the same display list (pinned by the existing test against `Text`, and by `a_field_without_focus_draws_nothing_new`). FundiDesk's and CallReady's fidelity figures did not move.

**Dismissal.** The menu goes on a tap elsewhere in the field (which also collapses the selection), on any edit, on any key, when a scroll view starts to be dragged, and with the focus. The selection itself survives a scroll and rides along with its handles.

### Scrolling under a dragged handle

While a handle is held within 24 dp of an edge of what can scroll, or past it, the text moves under the finger at 60 dp/s at the start of that zone, rising with depth to 900 dp/s: first the field itself (a bounded multi-line field vertically, a single line wider than its box sideways), and when that has nowhere further to go, the scroll view around it (against the visible part of the viewport, the keyboard's top included). The frame asks for another only while there is further to go in the direction pulled; a handle resting in the zone at the end of the text costs nothing.

### Keys

New `TextInputEvent`s: `Move { to: CaretMove, extend }`, `SelectAll`, `Copy`, `Cut`, `Paste`, `Undo`, `Redo`. `CaretMove` is `Left`, `Right`, `WordLeft`, `WordRight`, `Up`, `Down`, `LineStart`, `LineEnd`, `TextStart`, `TextEnd`.

- Left and right are the reading direction's backward and forward; in a value that reads right to left the Left key moves on through the text. Without `extend` a selection collapses to the end the key points at.
- Moves by lines need the layout and are carried out in the next frame, like a tap: Up and Down go to the character above or below the caret's x (from the first line up to the start of the text, from the last line down to its end); Home and End go to the two edges of the caret's visual line, and End stays on its line where the line wraps.
- A selection made by key has no handles and no menu.

On Android the host maps `KEYCODE_DPAD_*`, `MOVE_HOME` and `MOVE_END` (with Ctrl: by words, and to the ends of the text; with Shift: extend) to `nativeTextMove(move, extend)`, and Ctrl/Meta + A, C, X, V, Z, Shift+Z and Y to new `nativeTextCommand` codes 9 to 14. Input methods that move their cursor with key events arrive the same way, and `performContextMenuAction` (an input method's own select all, cut, copy, paste, undo, redo) is forwarded as the same commands.

### The input method and the selection

`TextInputRequest` and `TextEditState` each gained `anchor: Option<usize>`.

- **In**: `nativeTextEdited` already carried `selectionStart` and `selectionEnd`; Rust now reads both (Android's selection end is the caret). `Engine::text_input` records the anchor in what the host is believed to hold, so the field adopting it produces no echo.
- **Out**: `onTextInputCommand` takes `selectionStart, selectionEnd` in place of `caret` (descriptor `(IIIIIILjava/lang/String;II)V`). For an `Update` whose text is what the host already holds, the host sets the selection on its copy and calls `InputMethodManager.updateSelection` and nothing else: dragging a handle does not restart the input method. A changed text still restarts it, as in ADR 0016.

### The clipboard is a platform service

Two more `ServiceRequest`s: `SetClipboard(String)` (not answered) and `ReadClipboard`, answered with `ServiceResult::Clipboard(Option<String>)`. A field's copy and paste are requests the UI layer makes for itself through the same queue as the application's (`EffectState::request`), with no handler of the application's: `Ui` remembers the id of a paste and applies the answer to the field that asked, if it still has the focus. An application can use the same service: `Effects::set_clipboard(text)` and `Effects::clipboard(Message::Pasted)`.

**The clipboard is read only when the user chooses Paste** (the menu item, Ctrl+V, an input method's paste). Showing the menu reads nothing, which is why Paste is offered whether or not there is anything to paste: from Android 12 the system shows "pasted from your clipboard" on every read of another app's clip, and iOS does the same.

What a paste does: line breaks of every kind become a space in a field of one line and `\n` in a field of several (a trailing one is dropped in a field of one line); then the text goes through `insert` like typed text, so `digits_only`, `max_length` and the refusal of control characters apply; it replaces the selection and is one undo step.

**Secure fields**: copy and cut do nothing, from the menu (not offered) and from the keyboard; paste works.

| Host | Implementation |
| --- | --- |
| Android | `onServiceCommand` commands 7 (set; `first` is the text) and 8 (read), posted to the main thread in `MorningstarServices`; `ClipboardManager.setPrimaryClip(ClipData.newPlainText(..))`, and `getPrimaryClip` with every item `coerceToText` (formatting dropped, several items joined by line breaks). Answered with `nativeClipboardResult(id, found, text)`. A `RuntimeException` (not focused, a clip too large) is "nothing" |
| iOS | Service kinds 8 and 9, `UIPasteboard.general.string`, `morningstar_ios_clipboard_result`. **Uncompiled** |
| Test driver | In memory: `Ui::clipboard()`, `Ui::set_clipboard(text)`. A read is answered at the start of the next frame, like a `load` |
| No host | The same in-memory clipboard, so that copy and paste work inside a screenshot tool |

Log lines carry the length of what was read, never the text.

### Undo and redo

A bounded history per focused field (`History` in `edit.rs`, 100 steps, oldest dropped): the value, caret and selection before each step. A run of typing is one step, and so is a run of backspaces; whitespace, a move of the caret, a paste, a cut and a replaced selection start a new one. `Undo` and `Redo` deliver the restored value through `on_input` like any edit. The history lives with the focus: leaving the field forgets it, and so does the application showing a value no step led to.

Reachable from a hardware keyboard (Ctrl+Z, Ctrl+Shift+Z, Ctrl+Y) and an input method's undo. There is no on-screen affordance.

## Alternatives considered

**The platform's own selection UI** (Android's `ActionMode` and handle drawables over the `SurfaceView`; `UITextInteraction` on iOS). Rejected: both need the platform to know where every character is, which means reporting text geometry to the host continuously (`CursorAnchorInfo`, the full `UITextInput` protocol), and the result would look native and move a frame behind the text the framework draws. The framework already has the geometry; drawing two handles and a row of labels is less interface than describing the text to someone else.

**A timestamp in `PointerEvent`.** The right long-term shape, and it would make double-tap timing independent of frames. It changes a public struct that every host and several hundred test lines construct by literal. Frame time is within one frame of the truth because a frame follows every touch; the test suites drive it deterministically.

**Reading the clipboard to decide whether to offer Paste** (or `ClipDescription` alone, which does not trigger the notice on Android). Keeps the menu honest at the cost of another host round trip per menu; left out. An empty paste does nothing.

**Selection in the application's model** (`on_select`, a controlled selection). Nothing has needed it; `Ui::select` covers "select all when the form opens".

## Consequences

- The host interfaces grew, both halves together. JNI: 22 native methods (`nativeTextMove`, `nativeSetReduceMotion` of [ADR 0026](0026-system-bars-and-reduce-motion.md), `nativeClipboardResult` are new) and 5 upcalls; **`onTextInputCommand` changed its signature**, so a library and a host from different sides of this change show no keyboard (the lookup fails and says so in the log). C: `morningstar_ios_text_command`, `morningstar_ios_text_move`, `morningstar_ios_clipboard_result`, service kinds 8 and 9.
- `TextEditState` and `TextInputRequest` have one more field; the five places in the repository that write them as literals gained `anchor: None`. `TextInputEvent` has seven more variants.
- A focused field with a selection costs one `Vec` of rectangles (reused) and up to three more caret lookups per touch.
- Every frame with a finger resting on a field asks for a timed frame 500 ms later; if the finger lifts first that frame still comes, and draws the same picture once.

## Limitations

- **No magnifier.** Skipped: it needs the framebuffer under the finger drawn again, enlarged, which the display list cannot express without a second pass or a snapshot. Not trivial, so not built.
- **No insertion handle**: a caret alone cannot be dragged. Tap again, or long-press.
- **A long press does not go on to extend the selection** if the finger then moves; lift and use the handles.
- **The menu's labels are English** and fixed (`MENU_LABELS` in `ui.rs`). There is no hook to translate them yet.
- **Triple tap selects everything**, not a paragraph.
- **Left and right follow the paragraph's reading direction**, decided by its first strong letter; inside a run of the other direction they move against the screen.
- **A caret at a soft line wrap is drawn at the start of the next line** (ADR 0016's caret map has no affinity). The end handle is corrected for it; a caret alone is not.
- **A selected line break is not highlighted**, and an empty selected line shows nothing.
- **Handles and menu are not drawn inside a transformed widget** (the highlight is, turned with the text).
- **The handles overlap the line below** the one they hang from, as Android's do; the menu covers the line above.
- **iOS**: selection, handles and menu work as drawn by the framework, but the Swift that forwards edit actions, arrow keys and the pasteboard has never been compiled, and without `UITextInput` (ADR 0016) iOS's keyboard knows nothing of the selection.
- **Undo has no visible control**, and an input method's own undo, where it keeps one, is separate.
- **Android below API 28** `coerceToText` of an HTML clip returns its text; styled spans are dropped by `toString`. Images and URIs paste as their text form or nothing.

## Verified

On the development host (macOS) only:

- Editing model: 12 new unit tests in `crates/morningstar-ui/src/edit.rs` (42 in all): ordering and snapping of a selection, typing over it in both directions, room freed in a full field, deletion (Arabic with marks), adoption of the platform's selection and its loss under a field rule, words (punctuation, apostrophes, decimals, Arabic, whitespace), word jumps, key moves and collapse, right-to-left reading, pasted line breaks and filters, history by runs and its bound.
- UI: 30 tests in `crates/morningstar-ui/tests/selection.rs` at 1080 x 2340 px and 2.8125 px/dp, driving pointer, key and input-method events: double, slow, triple tap; long press, early lift, moved finger, empty field, read-only field; typing and backspace over a selection; the caret inside a ligature; both handles, turning the selection over; a drag across wrapped lines (one rectangle per line, abutting); auto-scroll of a single line, of a bounded multi-line field and of the page, and that frames stop; menu dismissal on scroll; copy, cut and paste through the menu; that nothing is read until Paste; paste filters; secure-field rules; shortcuts; the application's own clipboard effects; arrows, Home, End, word jumps, Up and Down; undo and redo; the selection told to and taken from the input method; Arabic tapped at 41 positions landing only on its 9 character boundaries, its word selected from the right, its keys; a selection across two directions as two runs; the programmatic API; an unfocused field's display list unchanged.
- Pixels: 2 tests in `crates/morningstar-ui/tests/selection_rendered.rs` render the selection on Metal in the light and the dark theme and assert the highlight's blend (primary at 40% over the field, on encoded values), the handles' colour and their square and round corners, and the menu's body and labels. Both pictures were looked at.
- Text engine: `selection_rects` is exercised through the above (single line, wrapped lines, right-to-left, mixed directions, a partial ligature).
- Engine: a selection the application makes is an `Update`, the host's own is not echoed, a collapsed one is none (1 test in `engine.rs`).
- Android: `cargo ndk -t arm64-v8a clippy -p callready` is clean for every crate this record touches (the run reported lints in `morningstar-text`'s SVG work in progress, which is someone else's); the three Java files compile with `javac -Xlint:all -Werror` against `android-36/android.jar`; the five upcall descriptors (`javap -s`) and the new and changed native signatures (`javac -h`) were compared with the Rust, and both sides have 22 native methods.
- iOS: `cargo clippy --target aarch64-apple-ios -p morningstar-ios` clean for this crate; `morningstar_ios.h` passes `cc -fsyntax-only -Wall -Wextra`; the Swift passes `swiftc -parse` (syntax only).

## Not verified

- **Everything on an Android device.** No APK was built. To try, in a field with a few words in it:
  - double tap and long press select a word; the two handles and the menu appear where expected and are comfortable to hit; the long-press timing feels right against other apps;
  - dragging each handle, across lines, and holding one at the edge of a long single-line field, of a bounded multi-line field (CallReady's reply) and of a scrolling form;
  - the menu: Copy then Paste in the same and in another app; Cut; Select all; that **the "pasted from your clipboard" notice appears only on Paste** and never when the menu merely opens (Android 12+);
  - pasting formatted text from a browser arrives plain; pasting into a phone-number field keeps digits only;
  - a password field offers only Paste;
  - the input method stays consistent: suggestions after a selection changes, Gboard's cursor swipe on the space bar and Samsung Keyboard's cursor keys move the caret, their own select all and clipboard rows work, and typing over a selection replaces it once (not twice);
  - a hardware keyboard: arrows, Shift, Ctrl, Home, End, Ctrl+A/C/X/V/Z;
  - frame times while a handle is dragged.
- **Everything on iOS**, beyond the Rust half type-checking.
- **Android API 24 to 35.**
