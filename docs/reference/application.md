# Application

The trait an app implements. Only `update` and the `Message` type are required.

```rust
pub trait Application: Send + 'static {
    type Message: Clone + Send + 'static;

    fn setup(&mut self, setup: &mut Setup<'_>) {}
    fn theme(&self) -> Theme { Theme::LIGHT }
    fn view(&self) -> Element<Self::Message> { Container::empty().into() }
    fn view_in(&self, viewport: &Viewport<'_>) -> Element<Self::Message> { self.view() }
    fn update(&mut self, message: Self::Message);
    fn back(&mut self) -> bool { false }
    fn tick(&self) -> Option<(Duration, Self::Message)> { None }
    fn effects(&mut self, out: &mut Effects<Self::Message>) {}
    fn system_bars(&self) -> Option<SystemBars> { None }
}
```

| Method | Called | Guide |
| --- | --- | --- |
| `setup` | Once, before the first frame | [Effects and services](../guide/effects.md#values-kept-on-the-phone) |
| `theme` | Every frame | [Themes and motion](../guide/theming-and-motion.md) |
| `view` | Every frame that is drawn | [The application model](../guide/application-model.md) |
| `view_in` | Instead of `view`, with the screen's size, insets and keyboard | [Layout](../guide/layout.md#a-layout-that-depends-on-the-screen) |
| `update` | For each message | [The application model](../guide/application-model.md) |
| `back` | The system's Back button or gesture | [Navigation and time](../guide/navigation.md#the-systems-back) |
| `tick` | To ask for a message after a delay | [Navigation and time](../guide/navigation.md#time) |
| `effects` | After every `update` | [Effects and services](../guide/effects.md) |
| `system_bars` | Every frame | [Themes and motion](../guide/theming-and-motion.md#the-systems-bars) |

## Registering

```rust
morningstar::app!(Ui::new(MyApp::default()));
```

`Ui` runs the application on the runtime; `app!` makes the crate the native library the host loads. One per crate.

## Setup

What `setup` is given: the place to add fonts and images, and what the phone already knows.

{{api crates/morningstar-runtime/src/app.rs Setup}}

## Element

What `view` returns. Every widget converts into one with `.into()`.

{{api crates/morningstar-ui/src/element.rs Element}}
