# The application model

A Morningstar app is state, messages, and three functions. `view` draws the state, `update` changes it, and `effects` asks the phone for work that takes time. Nothing else touches the state, and nothing in the app does I/O.

## State, messages, view, update

{{snippet examples/docs-snippets/src/counter.rs#app}}

- **State** is a struct that implements `Application`.
- **`view`** builds a tree of widgets from `&self`. It changes nothing. It is called again whenever a frame is needed, so it must be cheap and must not remember anything between calls.
- **Messages** are values of your own type. A tappable widget carries one; a text field makes one from each new value.
- **`update`** receives a message and changes the state. It returns at once: no sockets, files, threads or sleeps.

The loop is the same for every feature: add a field to the state, add a message, draw the field in `view`, handle the message in `update`, and write the test.

## Registering the app

{{snippet examples/docs-snippets/src/counter.rs#register}}

`Ui` runs an `Application` on the runtime. `morningstar::app!` makes it the app that the Android or iOS host starts. The crate is built as a native library; there is no `main`.

## Everything an application can implement

Only `update` is required. Each other method has a default.

| Method | Called | Default |
| --- | --- | --- |
| `setup(&mut self, setup)` | Once, before the first frame. Add fonts and images, read the install id and stored values | Nothing |
| `theme(&self) -> Theme` | Every frame | `Theme::LIGHT` |
| `view(&self) -> Element<Message>` | Every frame that is drawn | An empty box |
| `view_in(&self, viewport) -> Element<Message>` | Instead of `view`, for a layout that depends on the screen | Calls `view` |
| `update(&mut self, message)` | For each message | Required |
| `back(&mut self) -> bool` | The system's Back button or gesture. `true` means the app dealt with it | `false`: the system leaves the app |
| `tick(&self) -> Option<(Duration, Message)>` | To ask for a message after a delay, again and again while it returns `Some` | `None`: no timer |
| `effects(&mut self, out)` | After every `update`. Queue work on `out` | Nothing |
| `system_bars(&self) -> Option<SystemBars>` | Every frame. Light or dark status and navigation icons | `None`: follow the theme |

[Application](../reference/application.md) in the reference has each one in full.

## Work that takes time

`update` cannot wait for a server. It records what should start, `effects` starts it, and the answer arrives later as a message. The host does the work off the render thread.

{{snippet examples/docs-snippets/src/effects.rs#update}}

Because a request is data and its answer is a message, a test can play the phone and the server. [Effects and services](effects.md) covers every service; [Testing](testing.md) shows the other side.

## Identity

The tree returned by `view` is thrown away after each frame. Four pieces of widget state outlive it, kept by the framework:

- which pressable a finger is holding,
- each scroll view's position and motion,
- which text input has focus, with its caret,
- what an animated widget is on its way from.

Each belongs to a widget's **identity**. By default that is its position in the tree: the chain of child indices from the root. `.id(value)` replaces it with an explicit one. Ids are global to the screen, so two widgets must not share one.

In practice:

- Give the root of each screen an id, so that two screens with a scroll view in the same place do not share a scroll position.
- Give list rows an id if rows can be inserted or removed while a finger is on one.
- Give every `TextInput` an id, so that focus stays with the field when something appears above it.

## Units

Every length is in **dp**: density-independent units, which are dp on Android and points on iOS. Font sizes too. The framework converts to physical pixels with the display's scale and snaps edges to whole pixels.

## Touch

A tap is a finger going down and coming up on the same pressable. Sliding off before lifting cancels it. Inside a scroll view, moving more than 8 dp turns the gesture into a scroll. One finger is followed at a time. After a message has been delivered, touches are ignored until the next frame is laid out, so a stale screen cannot be tapped.

## Next

- [Layout](layout.md): how sizes and positions are decided.
- [Navigation and time](navigation.md): several screens, Back, and timers.
- [Building screens](../building-screens.md): the long worked guide, from a picture to code.
