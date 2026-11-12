# Building screens: from a picture to code

For a developer or an AI agent handed a picture of a mobile screen. The API is `morningstar::ui`; the reasoning behind it is in [ADR 0013](architecture/0013-ui-layer.md); the compare-and-fix loop is in [design-to-code.md](design-to-code.md). `examples/showcase/src/lib.rs` is three complete screens to imitate.

## The shape of every app

```rust
use morningstar::ui::prelude::*;

#[derive(Default)]
struct Wallet { balance_hidden: bool }      // 1. state

#[derive(Clone)]
enum Message { ToggleBalance }              // 2. what widgets can ask for

impl Application for Wallet {
    type Message = Message;

    fn setup(&mut self, setup: &mut Setup<'_>) {
        setup.add_font(include_bytes!("../assets/fonts/NotoSans-Regular.ttf"));
        setup.add_font(include_bytes!("../assets/fonts/NotoSans-Bold.ttf"));
    }

    fn view(&self) -> Element<Message> {    // 3. the screen for this state
        SafeArea::new(
            col![
                text("Salio kuu").caption().muted(),
                text(if self.balance_hidden { "TSh •••••" } else { "TSh 1,250,400" }).display(),
                Button::new("Ficha").on_press(Message::ToggleBalance).fill_width(),
            ]
            .gap(8.0)
            .padding(20.0),
        )
        .into()
    }

    fn update(&mut self, message: Message) { // 4. the only place state changes
        match message {
            Message::ToggleBalance => self.balance_hidden = !self.balance_hidden,
        }
    }
}

morningstar::app!(Ui::new(Wallet::default()));
```

`Cargo.toml` depends on `morningstar` only (`morningstar-devtools` as a dev-dependency for screenshots), with `crate-type = ["cdylib", "lib"]`.

**All numbers are dp** (the units of a 1x design: a 360 or 390 wide artboard), font sizes included. Without fonts registered in `setup`, text is invisible.

## Reading a design

Work top-down. Every region of the picture is one of these:

| In the design | Write |
| --- | --- |
| Things stacked vertically | `col![a, b, c]` |
| Things side by side | `row![a, b, c]` |
| Things on top of each other (badge on avatar, text on art) | `stack![below, above]`; pin a child with `.place(Alignment::TOP_RIGHT)`, nudge with `.offset(x, y)` |
| Space between children | `.gap(12.0)` on the row or column |
| Space inside a frame | `.padding(16.0)`, `.padding_xy(h, v)`, `.padding_ltrb(l, t, r, b)` |
| Fixed size | `.width(120.0)`, `.height(48.0)`, `.size(w, h)` |
| "Hug contents" | the default |
| "Fill container" | `.fill_width()`, `.fill_height()`, `.fill()`; shares: `.fill_width_weight(2.0)` |
| Min / max | `.min_width(..)`, `.max_width(..)`, `.min_height(..)`, `.max_height(..)` |
| Children centred | `.center()` |
| Children at both ends | `row![left, Spacer::new(), right]` or `.space_between()` |
| Vertically centred items in a row | `.cross_align(CrossAlign::Center)` |
| One child aligned differently | `.align_self(CrossAlign::End)` on the child |
| Card: fill, corner radius, stroke, shadow | `.background(c).radius(16.0).border(1.0, c).shadow(elevation::LOW)` |
| Gradient fill | `.gradient(Gradient::diagonal(a, b))` (also `vertical`, `horizontal`, `angled`) |
| Content cut off by rounded corners | `.clip()` |
| Text | `text("…")` with `.display() / .title() / .headline() / .body() / .label() / .caption()`, or `.size(17.0).semibold()`; `.color(..)`, `.muted()` |
| Centred text | `text("…").centered().fill_width()` |
| Paragraph that wraps | `text("…").fill_width()` |
| One line, cut off | `.max_lines(1)` (no ellipsis) |
| Button | `Button::new("Label").on_press(msg)`; `.secondary() / .outlined() / .plain() / .destructive()`; full width: `.fill_width()` |
| Anything else tappable (row, card, chip) | `.on_press(msg)` on its box, or `Pressable::new(child).on_press(msg)` |
| Hairline | `Divider::new()`, indented: `.inset(68.0)`; in a row: `Divider::vertical()` |
| Fixed gap | `Spacer::fixed(24.0)` |
| Scrolling area | `ScrollView::new(col![..])` |
| Status bar / home indicator margins | `SafeArea::new(screen)`; for a bar whose background runs under the system bar: `.safe_area(SafeEdges::TOP)` or `BOTTOM` on the bar |
| Photo, illustration, logo | `Picture::new(image)` with `.size(..)`, `.radius(..)`, `.fit(ImageFit::Cover)`; register in `setup` with `setup.add_image_png(include_bytes!(..))` and keep the handle in the state |
| Profile picture | `Avatar::new("Amina Juma").image_maybe(self.photo)` (initials when there is no image) |
| Icon | `Icon::vector(ICON).size(20.0).color(c)`, with `const ICON: VectorIcon = VectorIcon::svg(r#"<svg viewBox=..>..</svg>"#)` pasted from the design; `Icon::new("+")` as a stand-in |
| A second typeface | `.family("Space Grotesk")` on a text; the defaults per role in `Theme { fonts: Fonts::new(body, heading), .. }` |
| Letter spacing, capitals | `.letter_spacing(0.6)` (dp), `.uppercase()` |
| Line height as a length | `.line_height_dp(22.0)` |
| A bold or coloured phrase in a sentence | `Text::rich([span("New here? "), span("Join").semibold().color(c)])` |
| Dashed outline | `.border_dashed(1.0, c)` (what a browser draws), or `.border(1.0, c).border_dash(4.0, 3.0)` |
| Colour from the design | `hex(0x2B3FE0)`, `hexa(0x2B3FE080)`; theme roles: `ThemeColor::Primary`, `Surface`, `TextMuted`, … |

Colours: put the palette in constants (`const INDIGO: Color = hex(0x2B3FE0);`) and the roles in a `Theme` returned from `theme()`:

```rust
const THEME: Theme = Theme {
    colors: Colors { primary: INDIGO, ..Theme::LIGHT.colors },
    ..Theme::LIGHT
};
```

## A worked example

The design: a white card with 20 dp corners. Inside, rows of a 40 dp round avatar, a name in semibold with a grey caption under it, and an amount on the right in green or black. Hairlines between rows start under the text.

Read it as structure first:

```
card  = column, white, radius 20, soft shadow, clipped
  row = [avatar 40] [column: name / caption — takes the remaining width] [amount]
        gap 12, padding 16 x 12, vertically centred, tappable
  divider inset by 16 + 40 + 12 = 68
```

Then write exactly that:

```rust
fn transaction_row(index: usize, name: &'static str, when: &'static str, amount: String, incoming: bool)
    -> Container<Message>
{
    row![
        Avatar::new(name),
        col![
            text(name).semibold().max_lines(1).fill_width(),
            text(when).caption().muted().max_lines(1).fill_width(),
        ]
        .gap(2.0)
        .fill_width(),                      // the part that gives way
        text(amount).label().color(if incoming {
            ColorRef::from(ThemeColor::Success)
        } else {
            ColorRef::from(ThemeColor::Text)
        }),
    ]
    .gap(12.0)
    .cross_align(CrossAlign::Center)
    .fill_width()
    .padding_xy(16.0, 12.0)
    .on_press(Message::OpenTransaction(index))
    .id(("transaction", index))
}

fn transactions(rows: Vec<Container<Message>>) -> Container<Message> {
    let mut card = Column::new()
        .fill_width()
        .radius(20.0)
        .background(ThemeColor::Surface)
        .shadow(elevation::LOW)
        .clip();
    for (index, row) in rows.into_iter().enumerate() {
        if index > 0 {
            card = card.push(Divider::new().inset(68.0));
        }
        card = card.push(row);
    }
    card
}
```

Small functions that each return one piece (`Container<Message>` or `Element<Message>`) keep `view` reading like the design's layer list. Use `Column::new().push(..)` / `.extend(iter)` / `.push_maybe(option)` when children come from data, the macros when they are written out.

## A form

A text field is two things: a `TextInput`, which is only the text (like `Text`), and the `Container` around it, which is the field's frame. The application owns the string.

```rust
#[derive(Default)]
struct Login { phone: String, password: String, show_password: bool }

#[derive(Clone)]
enum Message { Phone(String), Password(String), TogglePassword, LogIn }

fn view(&self) -> Element<Message> {
    // The design's field: 52 dp tall, 1 dp outline, 12 dp corners.
    let frame = |content: Element<Message>| {
        Row::new()
            .push(content)
            .fill_width()
            .height(52.0)
            .cross_align(CrossAlign::Center)
            .padding_xy(16.0, 0.0)
            .radius(12.0)
            .border(1.0, ThemeColor::Divider)
            .focused_border(1.5, ThemeColor::Primary) // also makes the whole box tappable
    };
    SafeArea::new(
        ScrollView::new(
            col![
                frame(
                    TextInput::new(&self.phone)
                        .placeholder("07XX XXX XXX")
                        .keyboard(KeyboardType::Phone)
                        .action(ImeAction::Next)       // the keyboard's key moves to the next field
                        .on_input(Message::Phone)
                        .id("phone")
                        .into(),
                ),
                frame(
                    row![
                        TextInput::new(&self.password)
                            .placeholder("Password")
                            .secure(!self.show_password)
                            .on_input(Message::Password)
                            .on_submit(Message::LogIn) // the keyboard's Done key
                            .id("password"),
                        Pressable::new(icon(EYE)).on_press(Message::TogglePassword),
                    ]
                    .fill_width()
                    .cross_align(CrossAlign::Center)
                    .into(),
                ),
                Button::new("Log in").on_press(Message::LogIn).fill_width(),
            ]
            .gap(16.0)
            .padding(24.0),
        ),
    )
    .id("login")
    .into()
}

fn update(&mut self, message: Message) {
    match message {
        Message::Phone(phone) => self.phone = phone,          // store what was typed...
        Message::Password(password) => self.password = password,
        Message::TogglePassword => self.show_password = !self.show_password,
        Message::LogIn => { /* ... */ }
    }
}
```

What to know:

- **`on_input` gets the whole new string**, after every change. Store it and pass it back as the value. To restrict or reformat what is typed, change it in `update` (keep only digits, upper-case it): the field then shows what you stored. For the common cases there are `.digits_only()` and `.max_length(n)`.
- **Without `on_input` a field cannot change.**
- **Style the text on the `TextInput`** (`.size()`, `.weight()`, `.color()`, `.family()`, `.placeholder_color()`, `.caret_color()`, or `.style(a_text)` to copy a `Text`'s style) **and everything else on the box around it.** Unfocused, a `TextInput` measures and draws exactly like a `Text` in the same style, so a field can be matched to a design before anyone types.
- **`focused_border` / `focused_background`** on the box are the focused look. Keep the layout the same in both states: a border is drawn over the box's edge and does not move the content, so a thicker focused border is fine.
- **Give inputs an `.id(..)`.** Focus and the caret are kept by identity (see "State and identity" in the `morningstar::ui` documentation); an id also lets a test tap the field (`ui.rect_of("phone")`).
- **Keyboard**: `.keyboard(KeyboardType::{Text, Number, Decimal, Phone, Email})`, `.capitalization(..)`, `.action(ImeAction::{Done, Next, Go, Search, Send})`, `.no_autocorrect()`, `.secure(bool)`, `.multiline()`.
- **Put a form in a `ScrollView`** if it can be taller than the space above the keyboard: the focused field is scrolled into view. A bottom bar that uses `.safe_area(SafeEdges::BOTTOM)` rides above the keyboard by itself. A screen that does not scroll is shifted up while the keyboard covers the focused field, which is a fallback, not a layout.
- **Code boxes**: one `TextInput` per box with `.max_length(1).digits_only().auto_advance().align(TextAlign::Center)`, each in a `Container::new(input).center().input_frame()`. Typing moves on, backspace moves back.
- **Focus** comes from a tap and goes when the user taps anything else, presses the keyboard's Done, or dismisses the keyboard. The application cannot set it.
- **Testing a form**: drive `Ui` as the engine does. `ui.pointer(..)` a tap on `ui.rect_of(id)`, then `ui.text_input(&TextInputEvent::Commit("a".into()))` per key, a `ui.frame(..)` after each, and assert on your state. `examples/fundidesk/tests/input.rs` is the model.
- **Not there**: selection, copy and paste, moving the caret with arrow keys, autofill, composition on iOS. See [ADR 0016](architecture/0016-text-input.md).

## Screens, Back, and time

Two optional methods of `Application` ([ADR 0018](architecture/0018-back-and-timers.md)). Navigation is state: keep a stack of screens, push in `update`, and pop in `back`.

```rust
use std::time::Duration;

#[derive(Clone, Copy, PartialEq)]
enum Screen { Home, Code, Done }

struct App { stack: Vec<Screen>, seconds_left: u32 }   // stack starts as vec![Screen::Home]

#[derive(Clone)]
enum Message { Open(Screen), ResendCode, Second }

impl Application for App {
    type Message = Message;

    fn view(&self) -> Element<Message> {
        match self.stack.last().copied().unwrap_or(Screen::Home) {
            Screen::Home => home(),
            Screen::Code => col![
                text(format!("Resend in 0:{:02}", self.seconds_left)).tabular_nums(),
                Button::new("Resend").on_press_maybe((self.seconds_left == 0).then_some(Message::ResendCode)),
            ].into(),
            Screen::Done => done(),
        }
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::Open(screen) => {
                if screen == Screen::Code { self.seconds_left = 30; }
                self.stack.push(screen);
            }
            Message::ResendCode => self.seconds_left = 30,
            Message::Second => self.seconds_left = self.seconds_left.saturating_sub(1),
        }
    }

    // The system back gesture or key. true: handled here. false: leave the app.
    fn back(&mut self) -> bool {
        if self.stack.len() > 1 { self.stack.pop(); true } else { false }
    }

    // While this is Some, `Second` arrives every second, at one frame a second.
    fn tick(&self) -> Option<(Duration, Message)> {
        let counting = self.stack.last() == Some(&Screen::Code) && self.seconds_left > 0;
        counting.then_some((Duration::from_secs(1), Message::Second))
    }
}
```

- **`back`** is also what an on-screen back arrow should do: give the arrow a message whose `update` arm pops the same stack. Back closes an open keyboard first, by itself; that Back does not reach the application. Without `back`, Back leaves the app from any screen.
- **`tick` is read from the state**, like `view`: there is nothing to start or stop. When it returns `None` (the countdown reached zero, or another screen is on top) the app draws nothing until the next touch.
- **Ticks count foreground time and never arrive in a burst.** The timer pauses while the app is in the background, and after any long gap one message arrives, not one per missed interval. For a deadline on the wall clock, store a `std::time::Instant` and compare it in `update` when a tick arrives, rather than counting ticks.
- **One timer per app.** For two rhythms, tick at the faster one and count.
- **Testing**: `ui.back()` returns what the system would be told. For a timer, build frames at chosen times, `ui.frame(&info.with_elapsed(Duration::from_secs(3)), &mut frame)`, and assert on your state; `frame.redraw_after()` is how long the host would sleep, and `None` there means idle. `crates/morningstar-ui/tests/back_and_tick.rs` is the model.

## Talking to a server, sound, and remembering things

`update` must return at once, and it cannot touch the device. Work that takes time is asked of the platform and comes back as a message ([ADR 0019](architecture/0019-effects-and-platform-services.md)). The pattern has three steps: `update` notes in the state that the work is wanted; `effects`, which the runtime calls after `setup` and after every `update`, hands it over; the result arrives in `update` like a tap would.

A complete voice note: hold nothing, tap to record, tap to send, hear the reply.

```rust
use morningstar::ui::prelude::*;

const SERVER: &str = "https://api.example.com/v1/turn";

#[derive(Clone)]
enum Message {
    Tap,
    RecordingStarted(Result<(), AudioError>),
    Recorded(Result<Recording, AudioError>),
    Replied(Result<HttpResponse, HttpError>),
    Played(Result<(), AudioError>),
}

#[derive(Clone, Copy, PartialEq)]
enum Phase { Idle, Starting, Recording, Sending, Speaking }

/// What `update` has decided to start. `effects` empties it.
enum Start { Record, Stop, Send(Recording), Play(AudioClip), Remember }

struct Note {
    device_id: String,
    phase: Phase,
    error: Option<String>,
    start: Vec<Start>,
}

impl Note {
    fn new() -> Self {
        Self { device_id: String::new(), phase: Phase::Idle, error: None, start: Vec::new() }
    }

    fn fail(&mut self, error: impl ToString) {
        self.phase = Phase::Idle;
        self.error = Some(error.to_string());
    }
}

impl Application for Note {
    type Message = Message;

    fn setup(&mut self, setup: &mut Setup<'_>) {
        setup.add_font(include_bytes!("../assets/fonts/NotoSans-Regular.ttf"));
        // Known before the first frame: what was stored last time, or else
        // the id the host made for this installation (32 hex digits).
        match setup.stored("device_id") {
            Some(id) => self.device_id = id.to_owned(),
            None => {
                self.device_id = setup.install_id().to_owned();
                self.start.push(Start::Remember);   // effects runs right after setup
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let label = match self.phase {
            Phase::Idle => "Record",
            Phase::Starting => "...",
            Phase::Recording => "Send",
            Phase::Sending => "Sending...",
            Phase::Speaking => "Listening...",
        };
        let ready = matches!(self.phase, Phase::Idle | Phase::Recording);
        SafeArea::new(
            col![
                Button::new(label).on_press_maybe(ready.then_some(Message::Tap)).fill_width(),
            ]
            .push_maybe(self.error.as_ref().map(|error| text(error.clone()).caption().fill_width()))
            .gap(12.0)
            .padding(20.0),
        )
        .into()
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::Tap if self.phase == Phase::Idle => {
                self.error = None;
                self.phase = Phase::Starting;
                self.start.push(Start::Record);
            }
            Message::Tap if self.phase == Phase::Recording => {
                self.phase = Phase::Sending;
                self.start.push(Start::Stop);
            }
            Message::Tap => {}
            Message::RecordingStarted(Ok(())) => self.phase = Phase::Recording,
            Message::RecordingStarted(Err(error)) => self.fail(error),   // PermissionDenied, ...
            Message::Recorded(Ok(recording)) => self.start.push(Start::Send(recording)),
            Message::Recorded(Err(error)) => self.fail(error),
            Message::Replied(Ok(response)) if response.is_success() => {
                self.phase = Phase::Speaking;
                let mime = response.header("content-type").unwrap_or("audio/mpeg").to_owned();
                self.start.push(Start::Play(AudioClip::new(response.body, mime)));
            }
            Message::Replied(Ok(response)) => self.fail(format!("server said {}", response.status)),
            Message::Replied(Err(error)) => self.fail(error),            // Offline, Timeout, ...
            // Stopped on purpose is not a failure worth showing.
            Message::Played(Ok(()) | Err(AudioError::Interrupted)) => self.phase = Phase::Idle,
            Message::Played(Err(error)) => self.fail(error),
        }
    }

    fn effects(&mut self, out: &mut Effects<Message>) {
        for start in self.start.drain(..) {
            match start {
                Start::Remember => out.store("device_id", self.device_id.clone()),
                Start::Record => out.start_recording(Message::RecordingStarted),
                Start::Stop => out.stop_recording(Message::Recorded),
                Start::Send(recording) => out.http(
                    HttpRequest::post(SERVER)
                        .header("X-Device-Id", self.device_id.clone())
                        .header("Content-Type", recording.mime.clone())
                        .body(recording.bytes),
                    Message::Replied,
                ),
                Start::Play(clip) => out.play(clip, Message::Played),
            }
        }
    }
}
```

What each request does, and what comes back:

| Call | Message gets | Notes |
| --- | --- | --- |
| `out.http(request, f)` | `Result<HttpResponse, HttpError>` | Any status is `Ok`, 404 and 500 included: check `response.status` or `is_success()`. Errors: `Offline`, `Timeout`, `InvalidUrl`, `TooLarge` (body over 10 MB), `Unsupported`, `Failed(text)`. |
| `out.start_recording(f)` | `Result<(), AudioError>` | Asks the user for the microphone the first time. `PermissionDenied`, `Busy` (already recording), `Unsupported`, `Failed(text)`. |
| `out.stop_recording(f)` | `Result<Recording, AudioError>` | `Recording { bytes, mime, duration }`: an M4A file (`audio/mp4`, mono AAC). `NotRecording` if none was running. |
| `out.play(clip, f)` | `Result<(), AudioError>` | Arrives when the clip **ends**. `AudioClip::new(bytes, "audio/mpeg")`; MP3 and M4A. `Interrupted` if it was stopped. |
| `out.stop_playback()` | nothing | The clip's own message says `Interrupted`. |
| `out.store(key, value)`, `out.remove(key)` | nothing | A few small strings. Not for secrets. |
| `out.load(key, f)` | `Option<String>` | Usually not needed: `setup.stored(key)` has it at start. |
| `out.forget_pending()` | nothing | Drops the answers of everything asked before it. |

- **Build a request** with `HttpRequest::get(url)`, `::post`, `::put`, `::delete`, then `.header(name, value)`, `.body(bytes)` or `.json_body(bytes)` (sets `Content-Type: application/json`; encode the JSON yourself), `.timeout(Duration)` (30 s if not set, for the whole exchange). Read a response with `.status`, `.header("content-type")`, `.body`, `.text()`.
- **HTTPS only.** Plain `http://` works only to `127.0.0.1`, `localhost` and `10.0.2.2`, for a server on your own machine (`adb reverse tcp:8787 tcp:8787`, then `http://127.0.0.1:8787`). Anything else fails as `Failed`.
- **Every request is answered once**, with an error if need be, so a "loading" state always ends. Show the error (`error.to_string()` is a sentence) and let the user try again.
- **One thing at a time, by your own state.** Nothing stops two taps from sending two requests except the state machine above (`on_press_maybe`). If a late reply could be mistaken for a newer one, number them: `out.http(request, move |result| Message::Replied(turn, result))`.
- **A recording stops by itself** after 60 seconds and when the app leaves the screen; `stop_recording` still returns what was recorded. Sound does not play, and recording does not start, while the app is off screen. Drive your own "3 seconds" with `tick` and call `stop_recording` when it runs out (`examples/services-check` does).
- **`view` never does any of this**, and `effects` must not change what `view` shows.
- **Not there**: streaming, upload or download progress, cancelling a request on the wire, cookies, recording levels, pausing or seeking a clip.

**Testing** needs no server and no phone. `Ui::testing(app)` holds each request until the test answers it:

```rust
let mut ui = Ui::testing(Note::new());
let mut frame = Frame::for_app_with_host(       // runs setup, as on a second launch
    &mut ui,
    &HostState::new().with_stored("device_id", "abc"),
);

ui.send(Message::Tap);
assert!(ui.answer_recording_started(Ok(())));                 // the microphone starts
ui.send(Message::Tap);
assert!(ui.answer_recording(Ok(Recording {                    // "the recording is these bytes"
    bytes: vec![1, 2, 3],
    mime: "audio/mp4".into(),
    duration: Duration::from_secs(2),
})));

// What is the application waiting for now?
let ServiceRequest::Http(request) = &ui.pending_effects()[0].request else { panic!() };
assert_eq!(request.header_value("X-Device-Id"), Some("abc"));
assert_eq!(request.body, [1, 2, 3]);

assert!(ui.answer_http(Ok(HttpResponse::new(200)             // "the server replies this"
    .with_header("Content-Type", "audio/mpeg")
    .with_body(b"mp3 bytes".to_vec()))));
assert!(ui.answer_playback(Ok(())));                          // the clip finishes
assert!(ui.app().phase == Phase::Idle);

// And the unhappy path:
ui.send(Message::Tap);
ui.answer_recording_started(Err(AudioError::PermissionDenied));
assert_eq!(ui.app().error.as_deref(), Some("the microphone is not allowed"));
```

- `ui.answer_http`, `answer_recording_started`, `answer_recording`, `answer_playback` answer the **oldest** pending request of their kind and return `false` if there is none. `ui.resolve(id, ServiceResult::..)` answers a particular one, with `id` from `ui.pending_effects()`. The message is delivered at once.
- **Stored values are in the driver's memory**: `out.store` shows in `ui.stored("device_id")`, `ui.set_stored(key, value)` seeds a later `out.load`. A `load`'s message is delivered at the start of the next `ui.frame(..)`, or by `ui.deliver_effects()`. What `setup.stored` sees is separate: it comes from the `HostState` given to `Frame::for_app_with_host`.
- **A pretend server for a whole test**: `Ui::with_services(app, |request| match request { ServiceRequest::Http(r) if r.url.ends_with("/turn") => Some(ServiceResult::Http(Ok(HttpResponse::new(200)))), _ => None })`. `None` leaves that request pending. Its answers arrive with the next frame or `deliver_effects()`, not inside the `update` that asked, so the "sending" state can be asserted first.
- **With plain `Ui::new`** and no device (the screenshot example), every request fails as `Unsupported` at the next frame: screenshots show your error state unless you set the state up directly.
- `examples/services-check` is a complete small application with these tests (`tests/script.rs`), and the thing to install on a phone to see each service work.

## Name and icon

What the app is called on the home screen and what its icon is go in the app's `Cargo.toml`; `tools/build-android.sh` and `tools/build-ios.sh` generate the platform's icon files from it at every build ([ADR 0020](architecture/0020-app-identity-and-launcher-icons.md)):

```toml
[package.metadata.morningstar]
name = "My App"                    # default: the package name
id = "com.example.myapp"           # default: dev.morningstar.<lib>
icon = "assets/icon.png"           # square PNG, 512+, best 1024, full-bleed; found here without being named
icon_foreground = "assets/icon-foreground.png"   # optional: Android adaptive foreground
icon_background = "#0E0E0E"        # optional: default is the icon's top-left pixel
```

- With no icon, the app gets the built-in star. A misspelt key is an error, not a default.
- With only `icon`, Android's adaptive icon shows it as drawn, cut by the launcher's mask: a circle removes the corners. Keep what matters within the middle 92% of the width, on a background that runs to the edges.
- Give `icon_foreground` (transparent, 108 dp canvas, content in the middle 66 dp, 61%) when the art should float over a plain colour.
- The icon can be the app's own drawing: `examples/callready/examples/icon.rs` renders the home screen's disc to `assets/icon.png` and, with `screenshot_transparent`, to a transparent foreground.
- **Look at it small.** `cargo run -q -p morningstar-devtools --bin morningstar-icon -- my-app --preview out/icon.png` writes the icon square, round and under two adaptive masks at 192, 96 and 48 pixels. Detail that is a smudge at 48 is not doing anything.

## The loop

Add an example next to the app (copy `examples/showcase/examples/screenshot.rs`):

```rust
use morningstar::ui::Ui;
use morningstar::{FrameInfo, Insets, SurfaceSize};

let info = FrameInfo::new(SurfaceSize::new(1080, 2400))
    .with_scale(3.0)                                  // 360 x 800 dp
    .with_insets(Insets::new(0.0, 96.0, 0.0, 60.0));  // status bar, gesture bar (px)
let mut ui = Ui::new(MyApp::default());
ui.send(Message::Open(Screen::Settings));             // reach the state the design shows
std::fs::write("out/settings.png", morningstar_devtools::screenshot_frame(&mut ui, &info)?)?;
println!("{:?}", ui.overflows());                     // boxes whose children do not fit
```

1. **Render**: `cargo run -p my-app --example screenshot out/` (seconds).
2. **Look at the PNG.** Always. Passing tests do not show that a screen looks right.
3. **Compare** with the design when there is one, rendered at the design's size times a whole scale:
   `cargo run -q -p morningstar-devtools --bin morningstar-diff -- design.png out/settings.png --out out/diff.png --ignore 0,0,1080,96`
   and fix the worst region first (how to read the report: [design-to-code.md](design-to-code.md)).
4. **Check `ui.overflows()`** is empty: an entry names a box (in dp) that is too small for its children and by how much.

To capture a state that needs touches (a pressed row, a scrolled list): build a frame first so the app knows where things are (`Frame::for_app(&mut ui)`, `frame.begin()`, `ui.frame(&info, &mut frame)`), send `PointerEvent`s in pixels to `ui.pointer`, then screenshot. `ui.rect_of("some-id")` gives the pixel rectangle of any widget with an `.id(..)`.

Lock a finished screen with a small reference (render at scale 1 to keep the PNG small):

```rust
let png = screenshot_frame(&mut ui, &FrameInfo::new(SurfaceSize::new(360, 800)))?;
assert_matches_reference(&png, "tests/references/settings.png", Tolerance::DEFAULT);
// create or update: MORNINGSTAR_UPDATE_REFERENCES=1 cargo test -p my-app
```

## Pitfalls

Each of these produces a wrong picture, not an error.

- **Fill inside hug collapses.** `fill_width()` on a child of a box that hugs gives the child zero width (text vanishes). Give the parent a width: usually `.fill_width()` up the chain to something sized by the screen.
- **In a row, exactly the element that should give way gets `.fill_width()`.** Two long hugging texts in one row overflow instead of wrapping.
- **A `ScrollView` needs a height.** It fills by default, so its parent chain must have one: `col![header, ScrollView::new(..), tab_bar]` at the root works; a scroll view inside a hugging card does not.
- **Default alignment is top-left**, for stacks too. Centring is explicit: `.center()` on the parent.
- **The root always fills the screen, edge to edge.** Content needs `SafeArea::new(..)` or it sits under the status bar.
- **`Text` has no padding or background.** Wrap it: `Container::new(text("TZS")).padding_xy(10.0, 4.0).radius(99.0).background(..)`.
- **`text(..).size(n)` is the font size**; on boxes `.size(w, h)` is width and height.
- **Text alignment needs room.** `.centered()` does nothing on a hugging text; add `.fill_width()`.
- **A stack is as large as its largest child.** Decorative shapes that should not size a card go in a layer that fills: `stack![stack![circle, blob].fill(), content]` with `.clip()` on the card.
- **Give each screen's root an `.id("name")`.** Scroll position belongs to a widget's place in the tree; two screens with a scroll view in the same place would otherwise share it. Give list rows that can come and go `.id(("row", item.id))`.
- **State changes only in `update`.** `view` takes `&self`. A value a widget should change needs a `Message`.
- **`Message` must be `Clone`** (`#[derive(Clone)]`).
- **The macro is `col!`**, not `column!` (that name belongs to the standard library).
- **Dynamic strings are `String`s**: `text(format!("{count} items"))`. A borrowed `&str` from state does not compile; literals do.
- **Icons are not in the font.** Use `Icon::vector`. Only a small subset of SVG is accepted (`path`, `circle`, `rect`, `line`; one colour; no groups, transforms or opacity) and an icon outside it draws nothing: put the icon set in one file and test every icon with `VectorIcon::check()`.
- **A font family that was not added falls back silently.** Check with `setup.has_font_family("Inter")`.
- **Arabic and other right-to-left text**: shaped and ordered correctly, but give it `.fill_width()` so it has a box to align in. Layout itself is never mirrored.
- **Gradients look slightly lighter in the middle** than in a design tool (they blend in linear light); shadows and colours match.
- **Not available yet**: text selection, copy and paste; animations other than scroll fling, horizontal scrolling, ellipsis, wrapping rows (`flex-wrap`), per-corner radii, per-side borders (use a `Divider`), a tap target inside a sentence.

## From HTML/CSS or a design tool's inspector

Learned reproducing a real twelve-screen design (`examples/fundidesk`, [ADR 0015](architecture/0015-fonts-icons-and-design-fidelity.md)). The left column is what the design says; all numbers go across unchanged as dp.

| Design property | Morningstar |
| --- | --- |
| `display: flex` (row) / `flex-direction: column` | `row![..]` / `col![..]` |
| `gap: 12px` | `.gap(12.0)` |
| `padding: 16px 20px 24px` (top, sides, bottom) | `.padding_ltrb(20.0, 16.0, 20.0, 24.0)` (note the order: left, top, right, bottom) |
| `margin-top: 28px` on a child of a column | `Spacer::fixed(28.0)` before it, or fold it into the parent's padding or gap |
| `margin-top: auto` | `Spacer::new()` before it; on a tall screen, make that block the fixed footer under a `ScrollView` |
| `align-items: center` / `flex-end` | `.cross_align(CrossAlign::Center)` / `End` |
| `justify-content: space-between` / `center` | `.space_between()` / `.main_align(MainAlign::Center)`; both axes: `.center()` |
| `flex: 1` | `.fill_width()` in a row, `.fill_height()` in a column |
| `width: 100%` | `.fill_width()` |
| `width: 70%` | no percentages: compute it from the parent's fixed width |
| `display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px` | `row![a, b].gap(8.0)` with each child `.fill_width()`; several rows in a `col!` with the same gap |
| `flex-wrap: wrap` | not available: write the rows out |
| `background: #1E8416` | `.background(hex(0x1E8416))`, from a named constant |
| `border-radius: 14px` / `999px` / `50%` | `.radius(14.0)` / `.radius(999.0)` / `.radius(side / 2.0)` |
| `border: 1px solid #D9E0DB` | `.border(1.0, c)` |
| `border: 1px dashed #AEB9B1` | `.border_dashed(1.0, c)` |
| `border-top: 1px solid ..` | a `Divider::new().color(c)` as the first child of a column |
| `box-shadow: 0 12px 28px rgba(20, 32, 26, 0.14)` | `.shadow(Shadow::new(0.0, 12.0, 28.0, with_alpha(hex(0x14201A), 0.14)))` |
| `position: absolute; left/top/..` over other content | `stack![below, above.place(Alignment::TOP_LEFT).offset(x, y)]`; decoration that must not size the stack goes in a `.fill()` layer with padding |
| `font-family`, `font-weight: 600` | `.family("Inter")` or the theme's `Fonts`; `.semibold()` |
| `font-size: 15px; line-height: 22px` | `.size(15.0).line_height_dp(22.0)` |
| no `line-height` | the browser's default for that font: see the pitfall below |
| `letter-spacing: -0.4px` | `.letter_spacing(-0.4)` |
| `text-transform: uppercase` | `.uppercase()` |
| `text-align: center` | `.centered().fill_width()` |
| `color: ..` on a parent | pass the colour to each text and icon; nothing is inherited |
| `<span style="font-weight: 600">` inside a paragraph | `Text::rich([span(".."), span("..").semibold()])` |
| `<svg viewBox=.. stroke="currentColor" ..>` | a `VectorIcon::svg(..)` constant; `Icon::vector(X).size(w).color(c)`; another `stroke-width` on the same drawing: `.stroke_width(2.4)` |
| an `<svg>` with two colours | a `VectorPicture` constant and `Picture::vector(X)` (see "Turned things, illustrations and text effects" below); for a plate with a mark on it, a box with an icon over it is still simpler |
| `transform: rotate(-8deg)` | `.rotate(-8.0)` on the widget |
| `transform: rotate(5deg) translateY(-8px)` | `.transform(Transform::rotate(5.0).and_translate(0.0, -8.0))` |
| `font-style: italic` | `.italic()` on the text or the span |
| `text-shadow: 3px 3px 0 #FFF` | `.shadow(3.0, 3.0, hex(0xFFFFFF))` |
| `-webkit-text-stroke: 1.5px #FFF` | `.outline(1.5, hex(0xFFFFFF))` |
| `font-variant-numeric: tabular-nums` | `.tabular_nums()` |
| `opacity: 0.8` on a box with one colour | the colour with that alpha: `with_alpha(c, 0.8)` |
| `<input>`, `<textarea>` | a `Container` around a `TextInput` (see "A form") |
| `<a href>`, `<button>` | `.on_press(Message::..)` on the box |

### Pitfalls of translating CSS

Each of these cost a pixel or more per element until it was understood.

- **Padding starts inside the border in CSS; here the border is drawn over the box's edge and takes no room.** `border: 1px; padding: 16px` is `.border(1.0, c).padding(17.0)`. Centred content is unaffected.
- **Check `box-sizing`.** With `box-sizing: border-box`, `height: 52px` is the outer height. Without it (the default for `div` and `a`; `button` is border-box), the border and padding are added: `height: 48px; border: 1.5px solid` is 51 tall.
- **Text with no `line-height` is not `1.2 x size`.** A browser takes the font's ascent and descent, rounds each to whole *device* pixels and adds them. For a reference rendered at 2x, 13 px Inter is 15.5 dp tall and 15 px is 18. Put the font's metrics in the theme and compute it there (`examples/fundidesk/src/theme.rs`, `normal_line_height`); every text below a wrong one is displaced.
- **A bare `<input>` has 2 px of horizontal padding of its own**, so its text starts 2 px after the gap.
- **An inline `<span>` with padding** (a pill) is as tall as its line plus the padding: 12 px text with `padding: 6px 10px` is 14.5 + 12 dp.
- **Colour is inherited in CSS and not here.** An icon with `stroke="currentColor"` takes the nearest `color:`; find it and pass it.
- **`h1`..`p` in exported markup usually carry `margin: 0`.** If one does not, the browser's default margins are part of the design.
- **Anti-aliased edges are blended differently** (ADR 0015): text and thin strokes come out lighter than in a browser's rendering. It shows in the diff as many small regions over text and icons with the right extent. Do not chase it by changing weights or colours.

### The order that worked

1. Survey every screen before building one: collect the colours, type styles, radii and icons into a theme, an icon file and shared components. Count how often each value occurs; the frequent ones are tokens.
2. Build a screen top-down from the markup, one function per block, quoting the CSS in a comment.
3. Render at the reference's exact size and compare. Before reading regions, compare the *extent of ink* row by row between the two images: a first difference of one pixel that grows down the page is a height (line height, border, padding), not a rendering difference.
4. Fix heights first, then horizontal insets, then look at text and icons.
5. Stop when every remaining region is text, an icon or a curved edge with the right extent. Record the number and hold it in a test.

## Turned things, illustrations and text effects

Added for a design full of tilted tiles and inline illustrations (`examples/callready`, [ADR 0017](architecture/0017-transforms-and-vector-pictures.md)). Everything here is in `morningstar::ui::prelude`.

### A turned tile, with its text

`.rotate(degrees)` turns a widget and everything inside it about the widget's own centre, clockwise for positive degrees, exactly as CSS `transform: rotate(..)` does. **Layout does not see it**: the widget keeps the room it was laid out in, and its turned picture overlaps whatever is beside it. So build the tile as if it were upright, check it against the design upright if you can, and add the turn last.

```rust
// <span style="background: #F6C445; border-radius: 6px; padding: 10px 16px;
//              transform: rotate(-8deg)">TURNED</span>
Container::new(text("TURNED").size(18.0).bold().color(INK))
    .padding_xy(16.0, 10.0)
    .background(GOLD)
    .radius(6.0)
    .rotate(-8.0)
```

Background, border (dashed too), radius, gradient, shadow, clip, icons, pictures and text all turn together. A design often turns only the *frame* and leaves the label upright; then the turn goes on the frame alone:

```rust
// <a style="position: relative; height: 138px">
//   <span style="position: absolute; inset: 0; border: 2px solid #9B5DE5;
//                border-radius: 4px; transform: rotate(-2deg)"></span>
//   <span style="position: absolute; left: 12px; top: 12px">Angry customer</span>
// </a>
stack![
    Container::empty().fill().border(2.0, PURPLE).radius(4.0).rotate(-2.0),
    Container::new(text("Angry customer").size(16.0).semibold().color(WHITE)).padding(12.0),
]
.fill_width()
.height(138.0)
.on_press(Message::Open(Scenario::Angry))
```

### A turned text, and transforms with more than one function

Text takes the same modifier: `text("3").size(82.0).rotate(-5.0)`. It turns about the centre of its *box*, which is the line box (font size times line height), as in CSS, so give it the design's `line-height`.

For a CSS list of functions, write them in the same order with the `and_*` methods. The order matters and this is the order CSS means: each function acts in the axes the ones before it left behind.

```rust
// transform: rotate(5deg) translateY(-8px)   (moved "up" along the turned axis)
.transform(Transform::rotate(5.0).and_translate(0.0, -8.0))
// transform: rotate(-3deg) translateY(6px)
.transform(Transform::rotate(-3.0).and_translate(0.0, 6.0))
```

Lengths are dp. `Transform::translate`, `scale`, `skew_x`, `skew_y` and `matrix(a, b, c, d, tx, ty)` exist too. `a.then(b)` is the other order (move by `a`, then by `b`, both on the fixed page); do not use it to transcribe CSS. A second `.transform(..)` or `.rotate(..)` on the same widget replaces the first; to combine a parent's and a child's transform, nest the widgets.

What to know:

- **Taps follow the picture.** A turned `on_press` box is hit where it is drawn. `Ui::rect_of` still reports the upright layout rectangle.
- **It may overlap and may be clipped.** A turned tile sticks out of its room by up to half its diagonal. Inside a `.clip()` box or a scroll view it is cut at that box's edge, as `overflow: hidden` would; leave padding if the design shows the corners.
- **A box that clips can itself be turned** (`.clip().rotate(..)`): its children are cut to the turned, rounded shape. One such clip is exact at a time; do not nest clipping boxes inside a turned clipping box.
- **Do not put a `ScrollView` inside a turned widget.** A turned widget inside a scroll view is fine.
- **Turned text and pictures are slightly softer** than upright ones (they are resampled). It does not show at the few degrees designs use.
- A transform that only moves (`Transform::translate`) is the same as `.offset(x, y)`.

### Italic

`.italic()` on a `Text`, or on one `span(..)` of a rich text. If the family has an italic font and it was added in `setup`, that font is used. If not, the upright letters are slanted by 14 degrees, which is what a browser does for `font-style: italic` on such a family; the text takes exactly the room the upright text would. There is nothing to do with `Transform` here.

```rust
// <p style="font-style: italic; text-align: right">“Sorry, could you say that slower?”</p>
text("\u{201c}Sorry, could you say that slower?\u{201d}").italic().align(TextAlign::End).fill_width()
// one word: <em>slower</em>
Text::rich([span("could you say that "), span("slower").italic(), span("?")])
```

### An illustration in several colours, from inline SVG

A `VectorIcon` is one colour. For anything else, transcribe the SVG into a `VectorPicture`: a view box and a list of `Layer`s painted in order. It is a constant; every method is `const`.

```rust
// <svg width="250" height="250" viewBox="0 0 280 280">
//   <defs><clipPath id="discClip"><circle cx="140" cy="140" r="122"/></clipPath></defs>
//   <circle cx="140" cy="140" r="138" fill="none" stroke="#F6C445" stroke-opacity="0.25" stroke-width="2"/>
//   <circle cx="140" cy="140" r="122" fill="#0E0E0E" stroke="#F6C445" stroke-width="1.5"/>
//   <g clip-path="url(#discClip)">
//     <rect x="-40" y="114" width="360" height="52" fill="#F6C445" transform="rotate(-18 140 140)"/>
//   </g>
//   <g fill="none" stroke="#F6C445" stroke-opacity="0.55" stroke-width="1">
//     <circle cx="140" cy="140" r="114"/> <circle cx="140" cy="140" r="108"/> ...
//   </g>
//   <circle cx="140" cy="140" r="48" fill="#E9E2CF" stroke="#0E0E0E" stroke-width="3"/>
//   <g fill="none" stroke="#0E0E0E" stroke-width="3" stroke-linecap="round"
//      stroke-linejoin="round" transform="translate(122 122)">
//     <path d="M4 22v-4a14 14 0 0 1 28 0v4"/>
//     <rect x="2" y="20" width="7" height="11" rx="2"/>
//   </g>
// </svg>
const RING: Color = with_alpha(GOLD, 0.55);

pub const DISC: VectorPicture = VectorPicture::new(
    280.0,
    280.0,
    &[
        Layer::circle(140.0, 140.0, 138.0).stroke(with_alpha(GOLD, 0.25), 2.0),
        Layer::circle(140.0, 140.0, 122.0).fill(INK).stroke(GOLD, 1.5),
        Layer::rect(-40.0, 114.0, 360.0, 52.0)
            .fill(GOLD)
            .rotate(-18.0, 140.0, 140.0)
            .clip_circle(140.0, 140.0, 122.0),
        Layer::circle(140.0, 140.0, 114.0).stroke(RING, 1.0),
        Layer::circle(140.0, 140.0, 108.0).stroke(RING, 1.0),
        // ...
        Layer::circle(140.0, 140.0, 48.0).fill(CREAM).stroke(INK, 3.0),
        Layer::path("M4 22v-4a14 14 0 0 1 28 0v4").stroke(INK, 3.0).round().translate(122.0, 122.0),
        Layer::rounded_rect(2.0, 20.0, 7.0, 11.0, 2.0).stroke(INK, 3.0).round().translate(122.0, 122.0),
    ],
);

// In the view: the SVG's `width` and `height` are the size in dp.
Picture::vector(DISC).size(250.0, 250.0)
```

| SVG | Layer |
| --- | --- |
| `<svg viewBox="0 0 280 280">` | `VectorPicture::new(280.0, 280.0, &[..])`; a view box not at the origin: `VectorPicture::with_view_box(x, y, w, h, &[..])` |
| `<svg width="250" height="250">` | `.size(250.0, 250.0)` on the `Picture`; nothing in the picture |
| `<path d="..">` | `Layer::path("..")` (every path command is understood) |
| `<circle cx cy r>` / `<ellipse cx cy rx ry>` | `Layer::circle(cx, cy, r)` / `Layer::ellipse(cx, cy, rx, ry)` |
| `<rect x y width height>` / with `rx="2"` | `Layer::rect(x, y, w, h)` / `Layer::rounded_rect(x, y, w, h, 2.0)` |
| `<line x1 y1 x2 y2>` | `Layer::line(x1, y1, x2, y2)` |
| `fill="#0E0E0E"` | `.fill(INK)`. **No `fill` attribute means black in SVG**: write `.fill(Color::BLACK)`. `fill="none"`: leave `.fill` out |
| `stroke="#F6C445" stroke-width="1.5"` | `.stroke(GOLD, 1.5)`; SVG's default width is 1 |
| `stroke-opacity="0.25"` / `fill-opacity` | the colour with that alpha: `.stroke(with_alpha(GOLD, 0.25), 2.0)` |
| `opacity="0.5"` | `.opacity(0.5)`: the whole layer, fill and stroke together |
| `stroke-linecap="round"` / `stroke-linejoin="round"` | `.cap(LineCap::Round)` / `.join(LineJoin::Round)`; both: `.round()` |
| `fill-rule="evenodd"` | `.even_odd()` |
| `transform="rotate(-18 140 140)"` | `.rotate(-18.0, 140.0, 140.0)` |
| `transform="translate(122 122)"` | `.translate(122.0, 122.0)` |
| any other `transform` | `.transform(Transform::..)`, written in CSS order with `and_*`; a layer has one transform, so combine a group's and an element's into it |
| `clip-path="url(#id)"` on the element or its `<g>` | `.clip(Shape::..)`, `.clip_circle(cx, cy, r)` or `.clip_rect(x, y, w, h)` with the `<clipPath>`'s shape. The clip is in view-box coordinates and is **not** moved by the layer's own transform, which is what a clip on an enclosing `<g>` means |
| attributes on a `<g>` or on the `<svg>` | repeat them on each layer inside; nothing is inherited |
| `currentColor` | the colour it stands for; a picture has no "current colour" (use `.tint(..)` on the `Picture` to multiply the whole picture by a colour) |
| gradients, `<mask>`, `<pattern>`, dashes, `<text>` | not available |

Things that are pictures although the design draws them with CSS:

- **An ellipse outline** (`border: 1.5px solid; border-radius: 50%` on a box that is not square): a box's radius only makes a pill. For a `w` by `h` dp box with a border of `b`: `VectorPicture::new(w, h, &[Layer::ellipse(w / 2.0, h / 2.0, (w - b) / 2.0, (h - b) / 2.0).stroke(c, b)])` (the numbers written out, since they are constants), under the text in a `stack!`.
- **A circle in four colours**: four `Layer::rect` quadrants, each `.clip_circle(c, c, r)`.
- **`repeating-linear-gradient(45deg, A 0 4px, B 4px 7px)`** on an `s` by `s` square: fill the square with `B`, then stroke lines in `A`, 4 wide, running down to the right at 45 degrees, 7 apart, the first centred 2 from the bottom-left corner. Line `k` is `y = x - c` with `c = (2 + 7k) * 1.4142 - s`; write each as `M{c - 4} -4 l{s + 8} {s + 8}` in one path (the view box cuts the ends off). For `s = 46` that is ten lines, `c` from -43.17 in steps of 9.90: see `STRIPES` in `crates/morningstar-ui/tests/rendered.rs`.
- **Part of a shape at the screen's edge** (rings whose centre is off-screen): give the picture the SVG's own view box; what lies outside it is cut off.

What to know:

- Run `PICTURE.check()` over every picture in a unit test. A layer whose path does not parse is skipped silently otherwise.
- A picture is rasterised once for the pixel size it is drawn at and cached; each *distinct* picture on screen is an image of `4 * width * height` bytes in pixels (2 MB for a 250 dp disc on a 2.8x phone) and usually its own draw call. That is fine for a handful of illustrations per screen. **Do not use a picture where a box will do** (a circle, a pill, a bordered tile, a one-colour icon): boxes and icons cost nothing extra.
- A picture always shows its whole view box, centred in its box; make the box the view box's shape. `.radius(..)`, `.tint(..)`, `.rotate(..)` work as on any picture. To turn a picture by a fixed angle and keep it sharp, `.rotate` its layers instead of the widget.
- Layers are `'static` data: a drawing that depends on state (a progress ring) cannot be one picture. Draw the changing part with boxes, or keep one constant per state.

### A hard text shadow

`text-shadow` with a zero blur is the text drawn twice. `.shadow(x, y, colour)` does that: the copy goes behind, moved by `(x, y)` dp, and takes no room.

```rust
// font-size: 80px; line-height: 1; color: #9B5DE5; text-shadow: 3px 3px 0 #FFFFFF; transform: rotate(3deg)
text("1").family("Anton").size(80.0).line_height(1.0).color(PURPLE).shadow(3.0, 3.0, WHITE).rotate(3.0)
```

A blurred text shadow is not available.

### An outlined digit

```rust
// font-size: 70px; line-height: 1; color: #0E0E0E; -webkit-text-stroke: 1.5px #FFFFFF; transform: rotate(-3deg)
text("2").family("Anton").size(70.0).line_height(1.0).color(INK).outline(1.5, WHITE).rotate(-3.0)
```

The stroke is centred on the letters' edges, half inside and half outside, over the fill, as in a browser, and the text takes the room it would without it. Outlined text is drawn as an image (like a picture, with the same cost), so use it for a digit or a short word, not for paragraphs. A rich text is outlined in its one base colour.

### Numbers that line up

`.tabular_nums()` is `font-variant-numeric: tabular-nums`. It matters when the font's digits are proportional by default (Outfit's and Inter's are): a right-aligned figure or a running timer is laid out differently without it. On a font without the feature it does nothing.
