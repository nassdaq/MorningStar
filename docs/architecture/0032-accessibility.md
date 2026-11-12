# ADR 0032: Accessibility: a semantics tree derived from the widgets, mirrored by the hosts

Status: Accepted. Tested on the development host; nothing in it has been tried with TalkBack or VoiceOver, and the iOS host is not compiled.

## Context

Morningstar draws every pixel itself on one surface (ADR 0003, 0013). To the platform's assistive technology an app is therefore one opaque view: TalkBack says nothing, a blind person cannot use the app at all, and it fails any accessibility review. Native toolkits give each widget a node the screen reader can land on; a framework that owns its pixels has to describe them.

What was wanted: that a Morningstar screen reads and operates like a native one with no work for the usual widget, a small vocabulary for what the defaults cannot know (an icon-only button has no words), no cost for the great majority of sessions in which no assistive technology runs, a way to test it, and the crate boundaries of ADR 0001 kept.

## Decision

### 1. A semantics tree, plain data in core

`morningstar-core::SemanticsTree` is the nodes a screen reader can land on, in reading order, which is also depth-first order (a node is followed by everything inside it; the first node is the root and stands for the surface). A `SemanticsNode` has:

- `id`: the widget's identity (ADR 0013: its explicit `.id(..)` or its place in the tree), so a node is the same node from frame to frame and the reader's focus stays on it;
- `bounds`: where it can be seen, in physical pixels of the surface;
- `role`: `Group`, `Text`, `Heading`, `Button`, `Link`, `Image`, `TextField`, `Checkbox`, `Switch`, `Radio`, `Tab`, `List`, `Progress`, `Slider`;
- `label`, `value`, `hint`;
- `states` (a bit set): disabled, selected, checkable and checked, focused, password, multi-line, expandable and expanded, live (polite or assertive);
- `actions` (a bit set): activate, long press, scroll forward and backward, focus, set text, set selection, increment, decrement, dismiss;
- for a focused text field its selection (in characters of `value`), for a list its scroll position and range.

`Action` is what is asked for on a node. `SetSelection` counts in characters, not bytes, so that a password field, whose node shows one bullet per character, can be addressed without its text.

### 2. Derived from the frame that was painted, in the UI layer

The painter already visits every element with its rectangle, its scroll translation, its clip, its transform and its opacity. When a frame is to be described it also notes, per element, where it ended up on the screen and what it is (`crates/morningstar-ui/src/semantics.rs`); the tree is built from those notes after painting. Nothing is laid out or measured twice, and the bounds are by construction where the pixels are: through scroll offsets, cut by clips, and the bounding box of the rectangle through the transforms around it.

**What is left out**: anything the painter skipped or that cannot be seen. A row far outside its scroll view (culled), a widget whose clips leave nothing of it, one drawn at an opacity of nothing, one scaled to nothing, and a widget that is leaving (the "ghost" of a transition) have no node. A row partly in view has its visible part as bounds.

**The defaults**, with no work from the application:

| Widget | Node |
| --- | --- |
| `Text` | static text; its content is its label |
| `Button` | a button labelled by its text; without `on_press`, disabled and with no actions |
| any box with `on_press` (`Pressable`, a tappable row or card) | **one** button: its label is the texts inside it, in order, joined with ", "; the nodes inside are absorbed |
| `TextInput` | a text field: its placeholder is its hint, its text its value; focused, multi-line and password are states; its bounds are its input frame's if it has one |
| `ScrollView` | a list: a container whose children stay separate, with scroll actions while it can scroll that way |
| `Icon`, `Picture`, `Avatar`, shapes, `Divider` | nothing: decoration unless labelled |
| any other box | nothing itself; what is inside it stands on its own |

**The merge rules.** A box becomes one stop, absorbing the texts inside it, when it can be pressed, when it has `on_increment`/`on_decrement`, or when it was given a label, a value, a role, a state or `.merge_semantics()`. Inside a merged node, other controls (pressable boxes, text inputs) stay stops of their own, as its children, and their text is not part of its label. A box given `.semantics_group()`, and every scroll view, is a container instead: its children stay separate. The frame of a text input (`input_frame`) is never a node: it lends the input its size and, if it was labelled, its label.

**A password is never in the tree.** A secure field's `value` is its bullets; the characters are not copied into a note, a node or the buffer that crosses to the host, and a contrast finding about a secure field carries no text. Tests assert that the characters appear nowhere in the tree or on the wire.

**Reading order** is the tree's: a row or column is read in the order it is laid out in (layout is never mirrored, so that is start to end as written). The children of a `Stack` lie on one another in paint order, and are read top to bottom, then left to right, each child with all inside it placed by where its first stop is; children that start at the same place keep paint order. What lies wholly under a box that swallows touches (`block_touches`: the scrim of a sheet or dialog) is left out, as it is out of a finger's reach. While the selection menu of a text field is up its items (Cut, Copy, Paste, Select all) are buttons at the end of the tree.

**The modifiers**, on every widget:

| Modifier | Effect |
| --- | --- |
| `.label("Back")` | what it is called; required for a control with no text inside. On `Text` and `TextInput`, where `label` and `role` mean the text style, the same thing is `.semantic_label(..)` and `.semantic_role(..)`, which exist everywhere |
| `.hint("Opens the report")` | what it is for |
| `.value("72 percent")` | what it shows now |
| `.role(Role::Tab)`, `.heading()` | what it is |
| `.selected(bool)`, `.checked(bool)`, `.expanded(bool)`, `.disabled(bool)` | its state |
| `.live(Live::Polite)` / `Live::Assertive` | changes of what it says are announced |
| `.exclude_semantics()` | it, and all inside it, say nothing |
| `.merge_semantics()` | one stop reading the texts inside |
| `.semantics_group()` | a container, labelled or not |
| `Container::on_increment(msg)`, `on_decrement(msg)` | the value can be raised and lowered |

and one effect: `out.announce("Report ready")` (`announce_with(.., Live::Assertive)` to interrupt), a `ServiceRequest::Announce` that the hosts hand to the screen reader. It is not answered.

Roles and states are bits in the element and cost nothing to set. Labels, hints and values are strings in one box per widget that has any.

### 3. Built only when asked for

`FrameInfo::accessibility` says that assistive technology is on; hosts set it through `Engine::set_accessibility` from the platform's own setting (below). The UI layer describes a frame when it is set, or when a test called `Ui::set_accessibility(true)`. Otherwise the cost of accessibility to a frame is one branch per element painted, and nothing is collected, built or kept: `crates/morningstar-ui/tests/semantics_alloc.rs` counts allocations and finds a frame of a screen that uses every flag modifier to allocate exactly as often as the same screen without them (57 and 57), plus one allocation per widget given a label or value (the string's box, made in `view`), against 314 for the same frame described. Switched off again, a `Ui` drops its tree and is back to the first number.

### 4. To the host: only what changed, in one buffer

`App::semantics()` returns the tree of the last frame, if it was described; `Ui` implements it. After each frame the engine (`Engine::take_semantics_update`) compares that tree with the copy of what the host holds and, if they differ, appends an update to a byte buffer: the ids of nodes that went, and every node that is new or changed, each with a byte saying what changed (bounds, label, value, state, children, scroll, selection). The first update after accessibility comes on carries everything and tells the host to forget what it had. An unchanged frame (a blinking caret) sends nothing and costs one comparison of two trees.

The format is `SemanticsTree::encode_update`'s, documented there field by field: little-endian, strings as a length and UTF-8, selections in UTF-16 code units, which is what both platforms count in. Hosts name nodes by a positive `i32` (Android's virtual view ids are `int`s): the 64-bit identity folded, the root 0; two identities that fold alike are told apart by taking the next free number.

Actions come back through `Engine::accessibility_action(node, code, text, start, end)`, which maps the host's number to the identity, turns UTF-16 offsets into characters, and calls `App::accessibility_action`; `Ui` performs it exactly as `Ui::perform` does for a test. An action is input: a frame follows.

### 5. Android: a virtual view hierarchy

`MorningstarAccessibility` (new, beside `MorningstarActivity`) is the `AccessibilityNodeProvider` of the host view.

- **The tree crosses once per change**: the render thread calls `MorningstarNative.onSemanticsUpdate(byte[])`, which posts the array to the main thread; `applyUpdate` decodes it into a static table of nodes. Every query of the accessibility framework (`createAccessibilityNodeInfo`, `findFocus`) is answered from that table on the main thread and never waits for the render thread. The table is static, like the Rust runtime it mirrors, so a recreated activity attaches to it.
- **Nodes**: class names from roles so that TalkBack says the right thing (`Button`, `TextView`, `EditText`, `CheckBox`, `Switch`, `RadioButton`, `ImageView`, `ScrollView`, `ProgressBar`, `SeekBar`; a tab and a link are buttons with a role description), bounds in screen coordinates (the node's pixels plus the view's position), text or content description, state description for a value (API 30; appended to the text before that), hint text for a field, and enabled, selected, checkable, checked, heading (API 28), password, multi-line, editable, focused, scrollable, live region.
- **Actions**: click, long click, scroll forward and backward (increment and decrement for a slider, which is how TalkBack adjusts a `SeekBar`), focus, set text, set selection, expand and collapse (a click), and next/previous at movement granularity for text fields by character and by word, done in Java with a `BreakIterator` over the node's value and sent to Rust as a selection. Accessibility focus is kept in Java, the nodes not being views. Everything else is posted to Rust (`nativeAccessibilityAction`) like a touch.
- **Events**: `TYPE_WINDOW_CONTENT_CHANGED` (subtree, once, when children came or went or more than eight nodes changed; otherwise per changed node, and always for a live region, which is what makes TalkBack speak it), `TYPE_VIEW_TEXT_CHANGED` and `TYPE_VIEW_TEXT_SELECTION_CHANGED` for fields, `TYPE_VIEW_SCROLLED`, `TYPE_VIEW_FOCUSED` when the input focus moves, `TYPE_VIEW_CLICKED` and `TYPE_VIEW_LONG_CLICKED` after those actions, `TYPE_VIEW_ACCESSIBILITY_FOCUSED` and its clearing, hover enter and exit for explore by touch (`dispatchHoverEvent` hit-tests the table: the deepest node under the finger, the one painted last among siblings).
- **On and off**: `AccessibilityManager.isEnabled()`, sent with `nativeSetAccessibility` when the view attaches and from the accessibility-state and touch-exploration listeners. Enabled without touch exploration still counts: Switch Access, Voice Access and UI tests read the tree too.
- **The focus rectangle** is TalkBack's own, drawn from the node's bounds.
- An announcement is `SERVICE_ANNOUNCE` through `onServiceCommand`: `announceForAccessibility`, after `AccessibilityManager.interrupt()` when assertive.

### 6. iOS: accessibility elements (not compiled)

The same buffer through `morningstar_ios_take_semantics`, actions through `morningstar_ios_accessibility_action`, on and off through `morningstar_ios_set_accessibility` (`isVoiceOverRunning` or `isSwitchControlRunning`, and their notifications). `HostAccessibility.swift` keeps the tree and vends one `UIAccessibilityElement` per node that says or does something as the Metal view's `accessibilityElements`, in the tree's order: label, value, hint, traits from roles and states, frame converted to screen points, `accessibilityActivate`, `accessibilityScroll` (sent to the nearest list above the element), increment and decrement, the escape gesture as Back. Changes post `.screenChanged`, `.layoutChanged` or `.pageScrolled`; iOS has no live regions, so a live node's new text is posted as an `.announcement` (queued when polite). `MORNINGSTAR_SERVICE_ANNOUNCE` does the same for the effect.

### 7. Testing, a checker and a picture

- `ui.set_accessibility(true)`, then `ui.semantics()` is the tree of the last frame, with `find_by_label`, `find(role, label)`, `find_containing`, `with_role`, `children`, `spoken()`. `ui.perform(id, Action::Activate)` does what TalkBack's double tap does, through the same code; `ui.activate("Save")` is the short form. Asking for the tree of a frame that was not described panics, so that a test asserting "no issues" cannot pass without having looked.
- `ui.accessibility_issues()` reports: a control with nothing to call it; a touch target smaller than 48 x 48 dp (the laid-out size, or the frame's for a field); two controls beside each other with the same role, label and value; and text whose contrast with the colour behind it is under 4.5:1, or 3:1 for text of 24 dp (18.66 dp bold) and larger. Contrast is judged only where the answer is known: the nearest filled ancestor is one solid colour, or the screen's background, with nothing else painted in between; text over a gradient, a picture, or inside a group drawn through an opacity layer is not reported. A see-through colour is composited as the renderer does it (ADR 0023) first. Disabled controls and excluded text are not judged.
- `morningstar_devtools::screenshot_semantics` renders a screen with every node outlined and numbered in reading order beside a legend of role, label, value, hint, states and actions; `--semantics` on the `callready` and `fundidesk` screenshot examples writes it per screen and prints the checker's findings.

## Consequences

- An app built from the stock widgets is readable without change; what it must add is a label for each control that is only a picture, and the checker says which.
- The tree is rebuilt whole on every described frame and compared whole. For a few hundred nodes that is small beside painting; during a fling every row's bounds change and every frame sends them. No throttling was added without a measurement to justify it.
- `Element` grew by a role, a bit set and a pointer (16 bytes); every widget has the modifiers.
- `ServiceRequest` has a tenth variant, `Announce`, and the Java and Swift hosts a tenth command.
- `Live` is a new name in `morningstar::ui::prelude`; an application with a `Live` of its own imports its own explicitly (CallReady does).
- The two example apps pass the checker on every screen except for what their designs themselves get wrong, which is listed in each `SCREENS.md` and allow-listed in each test with its reason. Their fidelity numbers are unchanged to four decimals: describing a frame draws nothing.

## Known gaps

- **Font scale.** The `scale` the hosts report is density only; Android's "font size" and iOS's Dynamic Type are ignored, so text does not grow for a user who asked for larger text. It would take a second number from the hosts (`Configuration.fontScale`, `UIFontMetrics`) in `FrameInfo`, multiplied into text sizes (not box sizes) in the UI layer, and screens that hold when text is 1.3 to 2 times larger, which the designed examples would not without rework.
- **Granular text navigation** exists only for text fields (character and word) on Android. Reading a static text by character, word or line relies on TalkBack doing it from the node's text; lines and paragraphs are not offered, and nothing of the kind exists on iOS, where a text field is also not a `UITextInput` (ADR 0016): VoiceOver does not call it a text field and cannot move its caret.
- **Switch Access and keyboard navigation**: the tree and its actions are there, which is what Switch Access scans; there is no input-focus traversal (Tab, D-pad) and no visible focus ring of Morningstar's own, and none of it was tried.
- **Braille displays**: nothing specific; they read what TalkBack reads. Set-text and set-selection are implemented for them; not tried.
- **Collections**: a list has no `CollectionInfo` ("item 3 of 12"), because rows out of view have no nodes and the count would be wrong.
- **Scrolling to what is off screen**: reached by TalkBack's own scroll action when it runs out of nodes, or two fingers. `ACTION_SHOW_ON_SCREEN` is not implemented.
- **Role descriptions and the words "expanded"/"collapsed"** are English strings in the hosts.
- **Custom actions** (swipe actions on a row) and **links inside a sentence** have no vocabulary yet.
- **Reading order in a `Stack`** is geometric by each layer's first stop; a design that needs another order has no way to say so yet.

## Not verified

- **Everything on an Android device.** No APK was built and TalkBack has never read a Morningstar screen. The Rust type-checks for Android, the Java compiles against API 36 with every lint as an error, and the JNI descriptors match; that is all. `docs/architecture/README.md` has the checklist for the owner.
- **Whether TalkBack speaks a hint** given as part of a content description the way intended, announces a live region's change from a virtual node, and draws its focus rectangle on the surface at the right place.
- **API levels below 30** (state description appended to the text), below 28 (no heading), below 26 (no hint text).
- **Everything on iOS**: `HostAccessibility.swift` and the changes to `HostViewController.swift` and `HostServices.swift` were parsed (`swiftc -parse`) and nothing more. The Rust C ABI compiles and lints for `aarch64-apple-ios`.
- **Cost on a device** of describing frames while TalkBack is on (CPU per frame, the size and rate of updates during a fling).
