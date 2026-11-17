# Text input

`TextInput` is an editable text. The application owns the string: the widget shows the value it is given and delivers each new value as a message, which `update` stores.

## A field

Like `Text`, an input has no frame of its own. The box around it is the field.

{{snippet examples/docs-snippets/src/form.rs#field}}

- `.input_frame()` makes a tap anywhere in the box focus the input.
- `.focused_border(..)` and `.focused_background(..)` are the box's look while the input has focus.
- `.id(..)` keeps focus with the field when the screen changes around it. An input that leaves the tree loses focus, and the keyboard goes away.

## The keyboard

{{snippet examples/docs-snippets/src/form.rs#secure}}

| Method | Effect |
| --- | --- |
| `.keyboard(KeyboardType::..)` | Which keyboard is shown: `Text`, `Number`, `Decimal`, `Phone` or `Email` |
| `.action(ImeAction::..)` | The action key: `Done`, `Next`, `Go`, `Search` or `Send` |
| `.on_submit(message)` | What the action key sends |
| `.capitalization(..)` | What the keyboard capitalises by itself: nothing, sentences, words or every character |
| `.no_autocorrect()` | Turns correction and suggestions off |
| `.secure(true)` | A password: characters are shown as bullets |
| `.max_length(n)`, `.digits_only()` | What the value may hold |
| `.multiline()`, `.max_lines(n)` | Several lines; bounded, the field scrolls inside itself |
| `.auto_advance()` | For a row of one-character code boxes |

While the keyboard is up, boxes that pad for the bottom safe area pad for the keyboard instead, and the focused field is kept in view.

## Selection and the clipboard

A field behaves as a native one does. A double tap or a long press selects a word and brings up two handles and a small menu (Cut, Copy, Paste, Select all), drawn by the framework. A hardware keyboard's arrows and shortcuts work, and the clipboard is the platform's. None of this reaches the application, which still sees only the new string. [ADR 0025](../architecture/0025-text-selection-and-clipboard.md).

The app can also use the clipboard itself, as an effect: `out.set_clipboard(text)` and `out.clipboard(on_text)`.

## What has been seen on a phone

Typing on Android has been verified on one phone, with the system keyboard: focus, the keyboard's type, Next and Done, and keyboard avoidance. Selection, the menu and the clipboard pass their tests on the development machine and have not yet been tried on a device.

## More

Every method is under [TextInput](../widgets/text-input.md). [Building screens](../building-screens.md#a-form) has a complete form with validation.
