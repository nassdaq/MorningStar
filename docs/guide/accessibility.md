# Accessibility

When TalkBack or VoiceOver is running, each frame is also described as a tree of semantic nodes derived from its widgets, and the host mirrors that tree to the platform. While no assistive technology asks for it, none of this is built.

## What you get without doing anything

- A `Text` is text.
- A `Button` is a button with its label.
- A `TextInput` is an edit box with its placeholder as the hint. A secure field's value is bullets, never its characters.
- A `ScrollView` is a list.
- Any box with `on_press` is one stop that reads the texts inside it in order.
- Icons, pictures and shapes say nothing.

## What only you know

The defaults cannot know what an icon means. Every widget has modifiers to say it.

| Modifier | Use it for |
| --- | --- |
| `.label(text)` | What a control is called. **Required for a control with no text inside it**, such as an icon button |
| `.hint(text)` | What the control is for, spoken after its label |
| `.value(text)` | What it shows now: a progress, a count |
| `.role(Role::..)` | What it is, when the default is wrong |
| `.heading()` | A heading, which a screen reader's user can jump between |
| `.selected(bool)`, `.checked(bool)`, `.expanded(bool)`, `.disabled(bool)` | Its state |
| `.live(Live::..)` | Announce the text whenever it changes: a status line, an error |
| `.exclude_semantics()` | Hide a decoration and everything inside it |
| `.merge_semantics()` | Make a group of texts one stop |
| `.semantics_group()` | Keep the children separate inside a named container |

On `Text` and `TextInput`, where `label` and `role` already mean the text style, the two are `semantic_label` and `semantic_role`.

`out.announce(text)` in `effects` has the reader say something that is not on the screen, such as "Message sent".

## Checking a screen

A test turns the description on and asks what is wrong with it.

```rust
let mut ui = Ui::testing(MyApp::default());
ui.set_accessibility(true);
// ... draw a frame ...
assert!(ui.accessibility_issues().is_empty());
```

{{variants crates/morningstar-ui/src/semantics.rs IssueKind}}

A generated app has this test for every screen from the start (`tests/accessibility.rs`), and the example apps hold all their screens to it. `Ui::semantics()` is the tree itself, and `Ui::perform(id, action)` does what a screen reader's user can do, so a journey can be tested by ear as well as by finger.

## What has been verified

The tree, its defaults and the checks pass their tests on the development machine. The Android host mirrors the tree to TalkBack as a virtual view hierarchy; **it has not yet been tried with TalkBack on a phone**. The iOS side is written and has never been compiled.

The design is [ADR 0032](../architecture/0032-accessibility.md); [Building screens](../building-screens.md#accessibility) has examples of each modifier.
