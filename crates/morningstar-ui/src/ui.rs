//! [`Application`] and [`Ui`], the adapter that runs one on the runtime:
//! view, layout, paint, hit-testing, gestures and scrolling.

use std::collections::HashMap;
use std::hash::Hash;
use std::time::Duration;

use morningstar_core::{
    AudioError, Border, Dash, HttpError, HttpResponse, ImeAction, LinearGradient, Point,
    PointerEvent, PointerPhase, Recording, Rect, ServiceCall, ServiceId, ServiceRequest,
    ServiceResult, Shadow, Size, TextInputEvent, TextInputRequest, TextInputTraits, Transform,
};
use morningstar_layout::{Edges, LayoutTree, NodeId, Sizing};
use morningstar_runtime::{App, Frame, FrameInfo, Setup, TextAlign, TextSpan, TextStyle};

use crate::edit::{self, EditConfig, EditState};
use crate::effects::{EffectState, Effects};
use crate::element::{
    Axis, BoxNode, DashKind, Element, Fill, IconNode, ImageNode, InputHandler, InputNode, Kind,
    LineHeight, PictureSource, SpacerKind, TextNode, key_hash,
};
use crate::theme::Theme;

/// A finger must travel this far (dp) before a press turns into a scroll.
const TOUCH_SLOP: f32 = 8.0;
/// Time constant (seconds) of the exponential decay of a fling.
const FLING_DECAY: f32 = 0.325;
/// A fling slower than this (dp per second) has stopped.
const FLING_STOP: f32 = 12.0;
/// Upper bound on fling speed (dp per second).
const FLING_MAX: f32 = 8000.0;
/// Frames further apart than this (seconds) say nothing about how fast a
/// finger is moving.
const VELOCITY_WINDOW: f32 = 0.1;
/// Children of a scroll view further than this (dp) outside the viewport
/// are not painted.
const CULL_MARGIN: f32 = 96.0;
/// Largest font size, in physical pixels, handed to the text engine. Beyond
/// this its glyph positions overflow (see ADR 0013, lower-layer gaps).
const MAX_FONT_PIXELS: f32 = 2048.0;
/// Largest magnitude of a text origin, in physical pixels, for the same
/// reason: the text engine adds glyph offsets to it as `i32`.
const MAX_TEXT_ORIGIN: f32 = 1.0e7;
/// The path of the root element.
const ROOT_PATH: u64 = 0;
/// Stands in for "not clipped" inside a transformed subtree, whose clips
/// from outside are not rectangles in its own space.
const EVERYWHERE: Rect = Rect::new(-1.0e7, -1.0e7, 2.0e7, 2.0e7);
/// How long a caret is shown, and then hidden, while it blinks.
const CARET_BLINK: Duration = Duration::from_millis(500);
/// Width of a caret, dp.
const CARET_WIDTH: f32 = 1.5;
/// Space (dp) kept between a focused field's text and the edge of what is
/// visible when it is scrolled into view: below it, enough for the field's
/// own frame; above it, a little less.
const REVEAL_BELOW: f32 = 28.0;
const REVEAL_ABOVE: f32 = 20.0;

/// A Morningstar application written with the declarative UI layer.
///
/// The application is a state struct. [`view`](Self::view) describes the
/// screen for the current state; widgets in it carry messages; a tap
/// delivers the widget's message to [`update`](Self::update), which changes
/// the state; the screen is then described again.
///
/// ```
/// use morningstar_ui::prelude::*;
///
/// #[derive(Default)]
/// struct Counter {
///     count: u32,
/// }
///
/// #[derive(Clone)]
/// enum Message {
///     Increment,
///     Reset,
/// }
///
/// impl Application for Counter {
///     type Message = Message;
///
///     fn view(&self) -> Element<Message> {
///         col![
///             text(format!("{}", self.count)).display(),
///             Button::new("Increment").on_press(Message::Increment),
///             Button::new("Reset").plain().on_press(Message::Reset),
///         ]
///         .gap(16.0)
///         .center()
///         .into()
///     }
///
///     fn update(&mut self, message: Message) {
///         match message {
///             Message::Increment => self.count += 1,
///             Message::Reset => self.count = 0,
///         }
///     }
/// }
///
/// // In the app crate: morningstar::app!(Ui::new(Counter::default()));
/// let mut ui = Ui::new(Counter::default());
/// ui.send(Message::Increment);
/// assert_eq!(ui.app().count, 1);
/// ```
pub trait Application: Send + 'static {
    /// What a widget can ask the application to do. Usually an enum, and
    /// `#[derive(Clone)]`: the framework keeps the messages of the widgets
    /// on screen and hands out a copy per tap.
    type Message: Clone + Send + 'static;

    /// Called once before the first frame. Register fonts here
    /// (`setup.add_font(include_bytes!("..."))`); without any, text is
    /// invisible.
    fn setup(&mut self, setup: &mut Setup<'_>) {
        let _ = setup;
    }

    /// The design tokens for the current state. Called once per frame.
    fn theme(&self) -> Theme {
        Theme::LIGHT
    }

    /// Describes the whole screen for the current state. Called whenever a
    /// frame is needed, which is after input and while something animates,
    /// not at every display refresh. The root element always fills the
    /// screen, edge to edge; wrap content in a
    /// [`SafeArea`](crate::SafeArea).
    fn view(&self) -> Element<Self::Message>;

    /// Changes the state in response to a message. Messages arrive in the
    /// order the taps completed, one at a time, never during `view`.
    fn update(&mut self, message: Self::Message);

    /// The system back gesture or key. Return `true` if the app handled it
    /// (for example, went to the previous screen), `false` to let the
    /// system leave the app. A frame follows a `true`, so the change shows
    /// without asking. The default handles nothing, so Back leaves the app.
    ///
    /// Back first closes an open keyboard; that never reaches here.
    ///
    /// ```ignore
    /// fn back(&mut self) -> bool {
    ///     self.stack.pop().is_some() // false on the first screen
    /// }
    /// ```
    fn back(&mut self) -> bool {
        false
    }

    /// A repeating timer. While this returns `Some((interval, message))`,
    /// the runtime delivers `message` to [`update`](Self::update) every
    /// `interval` (not faster than once per frame), and keeps frames
    /// coming only as needed for that: a one-second tick costs one frame a
    /// second. Returning `None` stops it, and the app goes fully idle
    /// again.
    ///
    /// It is asked at the start of every frame, and again after a tick was
    /// delivered, so it follows the state: a countdown returns `Some`
    /// while it is running and `None` once it has reached zero. The first
    /// message arrives one interval after the first frame that saw `Some`;
    /// a different interval starts the wait again.
    ///
    /// Time here is the frame's `elapsed`: foreground time, which stops
    /// while the app is in the background. A tick is never delivered more
    /// than once per frame, and ticks that were missed (the app was
    /// hidden, or a frame took longer than the interval) are dropped, not
    /// delivered in a burst: after a long gap one message arrives and the
    /// next follows a full interval later. So count ticks only for what
    /// may pause with the app; for a deadline on the wall clock, keep a
    /// `std::time::Instant` in the state and read it in `update`.
    ///
    /// ```ignore
    /// fn tick(&self) -> Option<(Duration, Message)> {
    ///     (self.seconds_left > 0).then_some((Duration::from_secs(1), Message::Second))
    /// }
    /// ```
    fn tick(&self) -> Option<(Duration, Self::Message)> {
        None
    }

    /// Starts work whose result comes back later as a message: an HTTP
    /// request, a recording, playing a sound, a value kept between
    /// launches. See [`Effects`] for what can be asked.
    ///
    /// `update` cannot do any of these itself: it must return at once, and
    /// the work belongs to the platform. So `update` only records, in the
    /// state, that the work is wanted; the runtime then calls this, **once
    /// after `setup` and once after every `update`** (and after
    /// [`back`](Self::back)), and the application moves what it recorded
    /// into `out`. Each result arrives as an ordinary message to `update`,
    /// between frames, and a frame follows it.
    ///
    /// ```ignore
    /// fn update(&mut self, message: Message) {
    ///     match message {
    ///         Message::Send => self.outgoing = Some(self.draft.clone()),
    ///         Message::Replied(Ok(response)) => self.reply = response.text().map(str::to_owned),
    ///         Message::Replied(Err(error)) => self.error = Some(error.to_string()),
    ///     }
    /// }
    ///
    /// fn effects(&mut self, out: &mut Effects<Message>) {
    ///     if let Some(text) = self.outgoing.take() {
    ///         out.http(HttpRequest::post(URL).json_body(text), Message::Replied);
    ///     }
    /// }
    /// ```
    ///
    /// The method takes `&mut self` so that it can `take()` what was
    /// recorded; it must not change anything `view` shows. The default
    /// starts nothing.
    fn effects(&mut self, out: &mut Effects<Self::Message>) {
        let _ = out;
    }
}

/// The repeating timer of [`Application::tick`], while it runs.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Tick {
    interval: Duration,
    /// The `FrameInfo::elapsed` at which the next message is due.
    due: Duration,
}

/// What a touch region does.
#[derive(Clone, Copy, Debug, PartialEq)]
enum RegionKind {
    /// Tappable; the index is into `Ui::messages`.
    Press(usize),
    /// A tap focuses the text input with this identity.
    Focus(u64),
    /// A scroll viewport.
    Scroll,
    /// Swallows touches.
    Block,
}

/// The space a transformed subtree's touch regions are in.
#[derive(Clone, Copy, Debug)]
struct HitSpace {
    /// From the screen to that space: undoes the transforms around it.
    inverse: Transform,
    /// What the clips outside the outermost transform leave visible, on
    /// the screen.
    clip: Rect,
}

/// A rectangle of the last frame that reacts to touch, in physical pixels,
/// already cut down to what its clips leave visible.
#[derive(Clone, Copy, Debug)]
struct Region {
    id: u64,
    rect: Rect,
    /// Set if the region is inside a transformed widget: `rect` is then in
    /// that widget's space, not on the screen.
    space: Option<HitSpace>,
    kind: RegionKind,
    /// The innermost scroll view containing this region.
    scroll: Option<u64>,
    /// The text input whose frame this region is in, if any: a tap here
    /// does not take that input's focus away.
    field: Option<u64>,
}

/// The text input being edited: transient widget state, kept while a
/// widget with this identity stays in the tree (ADR 0013's identity rule,
/// as for scroll positions).
#[derive(Clone, Debug)]
struct Focus {
    id: u64,
    edit: EditState,
    /// The value the field is expected to show: what it had, as changed by
    /// the edits made since. A different value in the next frame means the
    /// application changed or refused an edit.
    expected: String,
    /// False until `expected` and the caret have been taken from the field.
    known: bool,
    /// How far (px) a single-line field's text is scrolled to the left.
    scroll_x: f32,
    /// The frame time the caret last became solid; `None` restarts it.
    blink_from: Option<Duration>,
    /// A tap (px) whose caret position the next frame works out.
    tap: Option<Point>,
    /// Scroll the field into view at the next frame.
    reveal: bool,
}

impl Region {
    /// True if the screen position `point` is on the region.
    fn contains(&self, point: Point) -> bool {
        match self.space {
            None => self.rect.contains(point),
            Some(space) => {
                space.clip.contains(point) && self.rect.contains(space.inverse.apply(point))
            }
        }
    }
}

impl Focus {
    /// Focus on a field not yet seen with it: the caret goes to the end
    /// unless a tap says otherwise.
    fn new(id: u64, tap: Option<Point>) -> Self {
        Self {
            id,
            edit: EditState {
                caret: usize::MAX,
                composing: None,
            },
            expected: String::new(),
            known: false,
            scroll_x: 0.0,
            blink_from: None,
            tap,
            reveal: true,
        }
    }
}

/// What the last frame knew about one text input, in tree order. Kept so
/// that typing, and moving between fields, work between frames.
struct InputRecord<M> {
    id: u64,
    /// Its record in `Ui::records`.
    slot: u32,
    /// The innermost scroll view around it: identity and record.
    scroll: Option<(u64, u32)>,
    /// Its value, brought up to date by edits made since the frame.
    value: String,
    config: EditConfig,
    traits: TextInputTraits,
    auto_advance: bool,
    on_input: Option<InputHandler<M>>,
    on_submit: Option<M>,
}

#[derive(Clone, Copy, Debug)]
struct Press {
    id: u64,
    /// False once the finger has slid off; lifting then does nothing.
    inside: bool,
}

#[derive(Clone, Copy, Debug)]
struct Drag {
    anchor_y: f32,
    anchor_offset: f32,
}

/// The one touch interaction in progress.
#[derive(Clone, Copy, Debug)]
struct Gesture {
    pointer: u64,
    start: Point,
    press: Option<Press>,
    scroll: Option<u64>,
    drag: Option<Drag>,
}

/// Per-scroll-view state kept across rebuilds. Lengths in dp.
#[derive(Clone, Copy, Debug, Default)]
struct ScrollState {
    offset: f32,
    max: f32,
    /// dp per second; positive scrolls towards the end.
    velocity: f32,
    /// The offset when the previous frame was built.
    frame_offset: f32,
    /// True once `velocity` holds at least one sample of the current drag.
    tracking: bool,
    flinging: bool,
    /// True from release until the first frame of the fling.
    released: bool,
    /// Frame generation in which the widget was last seen.
    seen: u64,
}

/// What the layout pass learned about one text.
#[derive(Clone, Copy, Debug, Default)]
struct TextMeta {
    /// The wrap width (px) of the layout that will be drawn.
    constraint: Option<f32>,
    /// Size (px) of that layout, before any `max_lines` cut.
    measured: Size,
    /// The unwrapped size (px), if it was asked for.
    natural: Option<Size>,
}

/// One element in pre-order: its layout node and where its subtree ends.
#[derive(Clone, Copy, Debug)]
struct NodeRecord {
    node: Option<NodeId>,
    end_node: u32,
    end_text: u32,
}

/// A box whose children do not fit, from [`Ui::overflows`]. In dp.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Overflow {
    /// The box.
    pub rect: Rect,
    /// How far its children extend past its content box, per axis.
    pub amount: Size,
}

/// Runs an [`Application`] on the Morningstar runtime.
///
/// `Ui<A>` implements the runtime's low-level `App` trait, so it is what
/// the entry-point macro takes:
///
/// ```ignore
/// morningstar::app!(Ui::new(MyApp::default()));
/// ```
///
/// It can also be driven directly, which is how tests and screenshot tools
/// use it: `frame` builds a frame, `pointer` delivers a touch, and
/// [`send`](Self::send), [`rect_of`](Self::rect_of),
/// [`scroll_offset`](Self::scroll_offset) and
/// [`overflows`](Self::overflows) reach in from outside.
pub struct Ui<A: Application> {
    app: A,
    tree: LayoutTree,
    records: Vec<NodeRecord>,
    scratch: Vec<NodeId>,
    text_meta: Vec<TextMeta>,
    regions: Vec<Region>,
    messages: Vec<A::Message>,
    keyed: Vec<(u64, Rect)>,
    scroll_nodes: Vec<NodeId>,
    scrolls: HashMap<u64, ScrollState>,
    gesture: Option<Gesture>,
    inputs: Vec<InputRecord<A::Message>>,
    /// Boxes that are the frame of an input: (record, input identity).
    fields: Vec<(u32, u64)>,
    focus: Option<Focus>,
    keyboard_inset: f32,
    scale: f32,
    last_elapsed: Option<Duration>,
    generation: u64,
    /// The application's timer, as of the last frame.
    tick: Option<Tick>,
    /// Requests to the platform and the handlers waiting for answers.
    effects: EffectState<A::Message>,
}

impl<A: Application> Ui<A> {
    /// Wraps an application.
    pub fn new(app: A) -> Self {
        Self::with_effects(app, EffectState::new())
    }

    /// Wraps an application for a test, with nothing behind it: every
    /// request it makes of the platform ([`Application::effects`]) is held
    /// until the test answers it.
    ///
    /// * [`pending_effects`](Self::pending_effects) lists what is waiting;
    ///   [`answer_http`](Self::answer_http),
    ///   [`answer_recording_started`](Self::answer_recording_started),
    ///   [`answer_recording`](Self::answer_recording) and
    ///   [`answer_playback`](Self::answer_playback) answer the oldest
    ///   request of their kind, and [`resolve`](Self::resolve) any request
    ///   by id. The message is delivered at once, as [`send`](Self::send)
    ///   delivers one.
    /// * Stored values are kept in memory: `store` and `load` work without
    ///   being answered ([`stored`](Self::stored),
    ///   [`set_stored`](Self::set_stored)). What `load` read, and anything
    ///   else that needs no answer from the test, is delivered at the
    ///   start of the next frame, or by
    ///   [`deliver_effects`](Self::deliver_effects).
    /// * Playback behaves as on a device: `stop_playback`, or a second
    ///   `play`, ends a clip that is still pending with
    ///   [`AudioError::Interrupted`].
    ///
    /// ```
    /// use morningstar_ui::prelude::*;
    ///
    /// #[derive(Default)]
    /// struct Ping { wanted: bool, status: Option<u16> }
    ///
    /// #[derive(Clone)]
    /// enum Message { Ping, Replied(Result<HttpResponse, HttpError>) }
    ///
    /// impl Application for Ping {
    ///     type Message = Message;
    ///     fn view(&self) -> Element<Message> { col![].into() }
    ///     fn update(&mut self, message: Message) {
    ///         match message {
    ///             Message::Ping => self.wanted = true,
    ///             Message::Replied(result) => self.status = result.ok().map(|r| r.status),
    ///         }
    ///     }
    ///     fn effects(&mut self, out: &mut Effects<Message>) {
    ///         if std::mem::take(&mut self.wanted) {
    ///             out.http(HttpRequest::get("https://example.com/ping"), Message::Replied);
    ///         }
    ///     }
    /// }
    ///
    /// let mut ui = Ui::testing(Ping::default());
    /// ui.send(Message::Ping);
    /// assert_eq!(ui.pending_effects().len(), 1);
    /// assert!(ui.answer_http(Ok(HttpResponse::new(204))));
    /// assert_eq!(ui.app().status, Some(204));
    /// ```
    pub fn testing(app: A) -> Self {
        Self::with_services(app, |_| None)
    }

    /// As [`testing`](Self::testing), with a stand-in for the host: `fake`
    /// is shown every request that needs an answer (except `load`, which
    /// is answered from memory) and returns the answer, or `None` to leave
    /// that request pending for the test. Its answers are delivered at the
    /// start of the next frame or by
    /// [`deliver_effects`](Self::deliver_effects), never inside the
    /// `update` that made the request, so the state in between ("loading")
    /// can be looked at.
    ///
    /// ```ignore
    /// let mut ui = Ui::with_services(app, |request| match request {
    ///     ServiceRequest::Http(request) if request.url.ends_with("/turn") => {
    ///         Some(ServiceResult::Http(Ok(HttpResponse::new(200).with_body(REPLY))))
    ///     }
    ///     ServiceRequest::StartRecording => Some(ServiceResult::RecordingStarted(Ok(()))),
    ///     _ => None,
    /// });
    /// ```
    pub fn with_services(
        app: A,
        fake: impl FnMut(&ServiceRequest) -> Option<ServiceResult> + Send + 'static,
    ) -> Self {
        Self::with_effects(app, EffectState::with_driver(Box::new(fake)))
    }

    fn with_effects(app: A, effects: EffectState<A::Message>) -> Self {
        Self {
            app,
            tree: LayoutTree::new(),
            records: Vec::new(),
            scratch: Vec::new(),
            text_meta: Vec::new(),
            regions: Vec::new(),
            messages: Vec::new(),
            keyed: Vec::new(),
            scroll_nodes: Vec::new(),
            scrolls: HashMap::new(),
            gesture: None,
            inputs: Vec::new(),
            fields: Vec::new(),
            focus: None,
            keyboard_inset: 0.0,
            scale: 1.0,
            last_elapsed: None,
            generation: 0,
            tick: None,
            effects,
        }
    }

    /// The requests the application has made that are waiting for the test
    /// to answer them, oldest first. Empty unless the `Ui` was made with
    /// [`testing`](Self::testing) or [`with_services`](Self::with_services).
    pub fn pending_effects(&self) -> &[ServiceCall] {
        self.effects.held()
    }

    /// How many requests the application is still waiting for an answer
    /// to, whoever is carrying them out.
    pub fn effects_in_flight(&self) -> usize {
        self.effects.waiting()
    }

    /// Answers the request with this id: the application gets its message
    /// at once. Returns false, and does nothing, if the application is not
    /// waiting for that id (it was answered already, or given up on).
    /// `result` should be of the request's kind; one that is not reaches
    /// the application as a failure.
    pub fn resolve(&mut self, id: ServiceId, result: ServiceResult) -> bool {
        self.deliver_result(id, result)
    }

    /// Answers the oldest pending HTTP request: "the server replies this".
    /// Returns false if none is pending.
    pub fn answer_http(&mut self, result: Result<HttpResponse, HttpError>) -> bool {
        self.answer_oldest(
            |request| matches!(request, ServiceRequest::Http(_)),
            ServiceResult::Http(result),
        )
    }

    /// Answers the oldest pending `start_recording`. Returns false if none
    /// is pending.
    pub fn answer_recording_started(&mut self, result: Result<(), AudioError>) -> bool {
        self.answer_oldest(
            |request| matches!(request, ServiceRequest::StartRecording),
            ServiceResult::RecordingStarted(result),
        )
    }

    /// Answers the oldest pending `stop_recording`: "the recording is
    /// these bytes". Returns false if none is pending.
    pub fn answer_recording(&mut self, result: Result<Recording, AudioError>) -> bool {
        self.answer_oldest(
            |request| matches!(request, ServiceRequest::StopRecording),
            ServiceResult::Recording(result),
        )
    }

    /// Answers the oldest pending `play`: the clip finished, or failed.
    /// Returns false if none is pending.
    pub fn answer_playback(&mut self, result: Result<(), AudioError>) -> bool {
        self.answer_oldest(
            |request| matches!(request, ServiceRequest::Play(_)),
            ServiceResult::Played(result),
        )
    }

    fn answer_oldest(
        &mut self,
        wanted: impl Fn(&ServiceRequest) -> bool,
        result: ServiceResult,
    ) -> bool {
        match self.effects.oldest_held(wanted) {
            Some(id) => self.deliver_result(id, result),
            None => false,
        }
    }

    /// Delivers every answer that is known and has not reached the
    /// application yet, and those that become known as a result, until
    /// none is left: what a `load` read, what the stand-in host of
    /// [`with_services`](Self::with_services) answered, and, when there is
    /// neither a host nor a test driver, the `Unsupported` failure of each
    /// request. A frame does one round of this at its start; tests that
    /// build no frames call it. Returns how many messages were delivered.
    pub fn deliver_effects(&mut self) -> usize {
        let mut delivered = 0;
        // Bounded, so that an application answering every answer with a
        // new request cannot hang a test.
        while delivered < 10_000 {
            self.effects.fail_unattached();
            let Some((id, result)) = self.effects.next_ready() else {
                break;
            };
            delivered += usize::from(self.deliver_result(id, result));
        }
        delivered
    }

    /// The value the application has stored under `key`, in the memory of
    /// the test driver. `None` without a driver.
    pub fn stored(&self, key: &str) -> Option<&str> {
        self.effects.stored(key)
    }

    /// Puts a value into the test driver's memory, for the application to
    /// `load`. Returns false, and does nothing, without a driver. What
    /// `Setup::stored` sees at setup is separate: that comes from
    /// `Frame::for_app_with_host`.
    pub fn set_stored(&mut self, key: impl Into<String>, value: impl Into<String>) -> bool {
        self.effects.set_stored(key.into(), value.into())
    }

    /// Asks the application what it wants started and takes it in.
    fn collect_effects(&mut self) {
        self.app.effects(self.effects.scratch());
        self.effects.collect();
    }

    /// Hands the application the message an answer becomes, if it is
    /// still waiting for that answer.
    fn deliver_result(&mut self, id: ServiceId, result: ServiceResult) -> bool {
        match self.effects.message_for(id, result) {
            Some(message) => {
                self.deliver(message);
                true
            }
            None => false,
        }
    }

    /// The application state.
    pub fn app(&self) -> &A {
        &self.app
    }

    /// The application state, mutably. A change made here shows on the
    /// next frame; nothing asks for one.
    pub fn app_mut(&mut self) -> &mut A {
        &mut self.app
    }

    /// Delivers a message as if a widget had been tapped. For tests and
    /// tools; an interaction in progress is abandoned.
    pub fn send(&mut self, message: A::Message) {
        self.gesture = None;
        self.deliver(message);
    }

    /// Where the widget with this explicit `.id(..)` was in the last
    /// frame, in physical pixels, or `None` if it was not on screen. For a
    /// widget inside a scroll view this is its scrolled position, not cut
    /// down to the viewport.
    pub fn rect_of(&self, id: impl Hash) -> Option<Rect> {
        let key = key_hash(id);
        self.keyed
            .iter()
            .find(|(candidate, _)| *candidate == key)
            .map(|(_, rect)| *rect)
    }

    /// The scroll position, in dp, of the scroll view with this explicit
    /// `.id(..)`: `0` at the top. `None` if it was not in the last frame.
    pub fn scroll_offset(&self, id: impl Hash) -> Option<f32> {
        self.scrolls.get(&key_hash(id)).map(|state| state.offset)
    }

    /// The furthest the scroll view with this `.id(..)` can scroll, in dp.
    pub fn max_scroll_offset(&self, id: impl Hash) -> Option<f32> {
        self.scrolls.get(&key_hash(id)).map(|state| state.max)
    }

    /// True while a [`TextInput`](crate::TextInput) has focus (and so the
    /// keyboard is asked for).
    pub fn has_focus(&self) -> bool {
        self.focus.is_some()
    }

    /// True if the [`TextInput`](crate::TextInput) with this explicit
    /// `.id(..)` has focus.
    pub fn is_focused(&self, id: impl Hash) -> bool {
        let key = key_hash(id);
        self.focus.as_ref().is_some_and(|focus| focus.id == key)
    }

    /// Where the caret of the focused input is, as a byte offset into its
    /// value. For tests.
    pub fn caret(&self) -> Option<usize> {
        let focus = self.focus.as_ref()?;
        Some(focus.edit.caret.min(focus.expected.len()))
    }

    /// The part of the focused input's value the keyboard is still
    /// composing (shown underlined), as byte offsets. For tests.
    pub fn composing(&self) -> Option<(usize, usize)> {
        self.focus.as_ref()?.edit.composing
    }

    /// Takes the focus away from whichever input has it; the keyboard goes
    /// away with the next frame.
    pub fn clear_focus(&mut self) {
        self.focus = None;
    }

    /// Boxes of the last frame whose children do not fit inside them, in
    /// dp. An empty list means the layout holds together; an entry says
    /// which box is too small and by how much, without rendering anything.
    /// Scroll views, which overflow on purpose, are left out.
    pub fn overflows(&self) -> Vec<Overflow> {
        self.tree
            .overflowing()
            .filter(|(node, _)| !self.scroll_nodes.contains(node))
            .map(|(node, amount)| Overflow {
                rect: self.tree.rect(node),
                amount,
            })
            .collect()
    }

    /// Delivers the system back gesture or key to the application
    /// ([`Application::back`]) and returns its answer. The engine calls
    /// this for the host and draws a frame after a `true`; tests call it
    /// directly and build the next frame themselves. An interaction in
    /// progress is abandoned if the application handled it.
    pub fn back(&mut self) -> bool {
        let handled = self.app.back();
        self.collect_effects();
        if handled {
            self.gesture = None;
            // As after a message: the screen may have been replaced.
            self.regions.clear();
            self.messages.clear();
        }
        handled
    }

    /// Runs the application's timer for a frame at `elapsed`: delivers its
    /// message if one is due, at most once, and leaves in `self.tick` when
    /// the next is due, or `None` if the application wants no timer.
    fn run_tick(&mut self, elapsed: Duration) {
        let Some((interval, message)) = self.app.tick() else {
            self.tick = None;
            return;
        };
        let due = match self.tick {
            Some(tick) if tick.interval == interval => tick.due,
            // Just started, or its pace changed: a full interval from now.
            _ => elapsed.saturating_add(interval),
        };
        if elapsed < due {
            self.tick = Some(Tick { interval, due });
            return;
        }
        self.app.update(message);
        self.collect_effects();
        // The update may have stopped the timer or changed its pace.
        self.tick = self.app.tick().map(|(interval, _)| {
            // Keep the rhythm (no drift from frames arriving a little
            // late) unless a whole interval or more was missed; then what
            // was missed is dropped and the rhythm starts again from now.
            let on_beat = due.saturating_add(interval);
            let due = if on_beat > elapsed {
                on_beat
            } else {
                elapsed.saturating_add(interval)
            };
            Tick { interval, due }
        });
    }

    fn deliver(&mut self, message: A::Message) {
        self.app.update(message);
        self.collect_effects();
        // The regions describe a screen the update may have replaced. Until
        // the next frame has been laid out there is nothing to hit.
        self.regions.clear();
        self.messages.clear();
    }

    /// The press target and scroll view under `point`.
    fn hit(&self, point: Point) -> (Option<u64>, Option<u64>) {
        for region in self.regions.iter().rev() {
            if !region.contains(point) {
                continue;
            }
            return match region.kind {
                RegionKind::Press(_) | RegionKind::Focus(_) => (Some(region.id), region.scroll),
                RegionKind::Block => (None, region.scroll),
                RegionKind::Scroll => (None, Some(region.id)),
            };
        }
        (None, None)
    }

    /// What tappable `id` does, and the input frame it is in, if `point` is
    /// on it.
    fn target_at(&self, id: u64, point: Point) -> Option<(RegionKind, Option<u64>)> {
        self.regions.iter().find_map(|region| match region.kind {
            RegionKind::Press(_) | RegionKind::Focus(_)
                if region.id == id && region.contains(point) =>
            {
                Some((region.kind, region.field))
            }
            _ => None,
        })
    }

    /// Gives the focus to input `id`. `tap` is where the finger was, for
    /// placing the caret.
    fn focus_input(&mut self, id: u64, tap: Option<Point>) {
        match self.focus.as_mut() {
            Some(focus) if focus.id == id => {
                focus.tap = tap;
                focus.blink_from = None;
                focus.reveal = true;
            }
            _ => self.focus = Some(Focus::new(id, tap)),
        }
    }

    /// Moves the focus to the `index`-th input of the last frame, with the
    /// caret at the end of its value.
    fn move_focus(&mut self, index: usize) {
        let Some(record) = self.inputs.get(index) else {
            return;
        };
        self.focus = Some(Focus {
            edit: EditState {
                caret: record.value.len(),
                composing: None,
            },
            expected: record.value.clone(),
            known: true,
            ..Focus::new(record.id, None)
        });
    }

    /// The keyboard's action key.
    fn perform(&mut self, index: usize, action: ImeAction) {
        if action == ImeAction::Next && index + 1 < self.inputs.len() {
            self.move_focus(index + 1);
            return;
        }
        // The last field's Next, like Done, puts the keyboard away.
        self.focus = None;
        if action != ImeAction::Next
            && let Some(message) = self.inputs.get(index).and_then(|r| r.on_submit.clone())
        {
            self.deliver(message);
        }
    }

    /// A value produced by an edit: remembered as what the field now
    /// holds, and handed to the application.
    fn changed(&mut self, index: usize, value: String) {
        let Some(record) = self.inputs.get_mut(index) else {
            return;
        };
        record.value.clone_from(&value);
        let message = record.on_input.as_ref().map(|handler| handler(value));
        if let Some(message) = message {
            self.deliver(message);
        }
    }

    fn text_event(&mut self, event: &TextInputEvent) {
        let Some(focus) = self.focus.as_mut() else {
            return;
        };
        let Some(index) = self.inputs.iter().position(|r| r.id == focus.id) else {
            return;
        };
        let record = &self.inputs[index];
        if !focus.known {
            focus.known = true;
            focus.expected.clone_from(&record.value);
        }
        focus.edit.caret = edit::snap(&record.value, focus.edit.caret);
        focus.blink_from = None;
        focus.reveal = true;
        focus.tap = None;

        let (config, value) = (&record.config, record.value.as_str());
        let changed = match event {
            TextInputEvent::FocusLost => {
                self.focus = None;
                return;
            }
            TextInputEvent::Action(action) => return self.perform(index, *action),
            TextInputEvent::Enter if !config.multiline => {
                return self.perform(index, record.traits.action);
            }
            // A field nothing listens to cannot change.
            _ if record.on_input.is_none() => return,
            TextInputEvent::Enter => edit::insert(value, &mut focus.edit, "\n", config),
            TextInputEvent::Commit(text) => edit::insert(value, &mut focus.edit, text, config),
            TextInputEvent::Compose(text) => edit::compose(value, &mut focus.edit, text, config),
            TextInputEvent::DeleteForward => edit::delete_forward(value, &mut focus.edit),
            TextInputEvent::DeleteBackward => {
                if value.is_empty() && record.auto_advance && index > 0 {
                    // An empty code box: go back one box and delete there.
                    let previous = &self.inputs[index - 1];
                    let mut state = EditState {
                        caret: previous.value.len(),
                        composing: None,
                    };
                    let shorter = edit::delete_backward(&previous.value, &mut state);
                    self.move_focus(index - 1);
                    if let Some(shorter) = shorter {
                        if let Some(focus) = self.focus.as_mut() {
                            focus.edit = state;
                            focus.expected.clone_from(&shorter);
                        }
                        self.changed(index - 1, shorter);
                    }
                    return;
                }
                edit::delete_backward(value, &mut focus.edit)
            }
            TextInputEvent::Edited(state) => edit::adopt(value, &mut focus.edit, state, config),
        };
        let Some(value) = changed else {
            return;
        };
        focus.expected.clone_from(&value);
        let full = record.auto_advance
            && focus.edit.caret == value.len()
            && config
                .max_length
                .is_some_and(|max| edit::length(&value) >= max);
        if full {
            self.move_focus(index + 1);
        }
        self.changed(index, value);
    }

    fn pointer_down(&mut self, event: &PointerEvent) {
        if self.gesture.is_some() {
            // One interaction at a time: a second finger is ignored.
            return;
        }
        let (mut press, scroll) = self.hit(event.position);
        if let Some(state) = scroll.and_then(|id| self.scrolls.get_mut(&id)) {
            if state.flinging {
                // Touching a moving list stops it and is not a tap.
                press = None;
            }
            state.flinging = false;
            state.released = false;
            state.tracking = false;
            state.velocity = 0.0;
        }
        if press.is_none() && scroll.is_none() {
            // A touch on nothing: whatever was being edited is done.
            self.focus = None;
            return;
        }
        self.gesture = Some(Gesture {
            pointer: event.id,
            start: event.position,
            press: press.map(|id| Press { id, inside: true }),
            scroll,
            drag: None,
        });
    }

    fn pointer_moved(&mut self, event: &PointerEvent) {
        let Some(mut gesture) = self.gesture.filter(|g| g.pointer == event.id) else {
            return;
        };
        let position = event.position;
        let scroll = gesture.scroll.and_then(|id| self.scrolls.get_mut(&id));

        if let (Some(drag), Some(state)) = (gesture.drag, scroll) {
            let travelled = (position.y - drag.anchor_y) / self.scale;
            state.offset = clamp_offset(drag.anchor_offset - travelled, state.max);
            return;
        }

        let dx = position.x - gesture.start.x;
        let dy = position.y - gesture.start.y;
        if let Some(state) = gesture.scroll.and_then(|id| self.scrolls.get_mut(&id))
            && state.max > 0.0
            && dy.abs() > TOUCH_SLOP * self.scale
            && dy.abs() >= dx.abs()
        {
            // The finger is scrolling: whatever it landed on is no longer
            // being pressed. Anchoring here avoids a jump of one slop.
            gesture.press = None;
            gesture.drag = Some(Drag {
                anchor_y: position.y,
                anchor_offset: state.offset,
            });
            state.tracking = false;
            state.velocity = 0.0;
        } else if let Some(press) = gesture.press.as_mut() {
            press.inside = self.target_at(press.id, position).is_some();
        }
        self.gesture = Some(gesture);
    }

    fn pointer_up(&mut self, event: &PointerEvent) {
        let Some(gesture) = self.gesture.take_if(|g| g.pointer == event.id) else {
            return;
        };
        if gesture.drag.is_some() {
            if let Some(state) = gesture.scroll.and_then(|id| self.scrolls.get_mut(&id)) {
                state.flinging = true;
                state.released = true;
            }
        } else if let Some(press) = gesture.press {
            match self.target_at(press.id, event.position) {
                Some((RegionKind::Press(index), field)) => {
                    if let Some(message) = self.messages.get(index).cloned() {
                        // A button inside the focused field's own frame (a
                        // "show password" eye) leaves the focus alone;
                        // anything else ends the editing.
                        if self.focus.as_ref().is_some_and(|f| Some(f.id) != field) {
                            self.focus = None;
                        }
                        self.deliver(message);
                    }
                }
                Some((RegionKind::Focus(input), _)) => {
                    self.focus_input(input, Some(event.position));
                }
                // Slid off before lifting: nothing happens.
                _ => {}
            }
        } else {
            // A tap on the background of a scroll view.
            self.focus = None;
        }
    }

    fn pointer_cancelled(&mut self, event: &PointerEvent) {
        let Some(gesture) = self.gesture.take_if(|g| g.pointer == event.id) else {
            return;
        };
        if let Some(state) = gesture.scroll.and_then(|id| self.scrolls.get_mut(&id)) {
            state.velocity = 0.0;
            state.tracking = false;
        }
    }
}

impl<A: Application> App for Ui<A> {
    fn setup(&mut self, setup: &mut Setup<'_>) {
        self.app.setup(setup);
        self.collect_effects();
    }

    fn service_requests(&mut self, out: &mut Vec<ServiceCall>) {
        self.effects.take_for_host(out);
    }

    fn service_result(&mut self, id: ServiceId, result: ServiceResult) -> bool {
        self.deliver_result(id, result)
    }

    fn frame(&mut self, info: &FrameInfo, frame: &mut Frame) {
        // Answers that did not come from a host (there is none, or a test
        // driver stands in for it) are delivered here, one round per
        // frame. With a host this finds nothing: its answers arrive
        // between frames.
        self.effects.fail_unattached();
        for _ in 0..self.effects.ready_len() {
            if let Some((id, result)) = self.effects.next_ready() {
                self.deliver_result(id, result);
            }
        }
        if self.effects.has_ready() {
            // Those answers led to requests that are answered already.
            frame.request_redraw();
        }
        // The timer next, so that this frame shows what its message did.
        self.run_tick(info.elapsed);
        if let Some(tick) = self.tick {
            // One frame when the next message is due and none before it.
            frame.request_redraw_after(tick.due.saturating_sub(info.elapsed));
        }
        let theme = self.app.theme();
        let root = self.app.view();
        let scale = info.scale;
        self.scale = scale;
        self.generation += 1;
        frame.clear = theme.colors.background;
        if info.keyboard_inset != self.keyboard_inset {
            self.keyboard_inset = info.keyboard_inset;
            // The keyboard came, went or changed height: the field being
            // edited may now be under it.
            if let Some(focus) = self.focus.as_mut() {
                focus.reveal = true;
            }
        }

        // Describe the layout. The tree and every table are reused; the one
        // allocation here is the list of borrowed texts.
        self.tree.clear();
        self.records.clear();
        self.scratch.clear();
        self.fields.clear();
        let mut lowering = Lowering {
            tree: &mut self.tree,
            records: &mut self.records,
            scratch: &mut self.scratch,
            texts: Vec::with_capacity(self.text_meta.len()),
            insets: Edges::new(
                info.insets.left / scale,
                info.insets.top / scale,
                info.insets.right / scale,
                // While the keyboard is up it is what covers the bottom.
                info.insets.bottom.max(info.keyboard_inset) / scale,
            ),
            theme: &theme,
            scale,
            inputs: &mut self.inputs,
            input_count: 0,
            fields: &mut self.fields,
            open_fields: Vec::new(),
            open_scrolls: Vec::new(),
        };
        let root_node = lowering.lower(&root, None, root.key.unwrap_or(ROOT_PATH));
        let texts = lowering.texts;
        let input_count = lowering.input_count;
        self.inputs.truncate(input_count);

        // The focus lasts while its field is in the tree. If the field no
        // longer holds what the edits so far amount to, the application
        // changed or refused one: its value stands, the composition does
        // not.
        if let Some(focus) = self.focus.as_mut() {
            match self.inputs.iter().find(|record| record.id == focus.id) {
                None => self.focus = None,
                Some(record) => {
                    if !focus.known {
                        focus.known = true;
                        focus.expected.clone_from(&record.value);
                    } else if focus.expected != record.value {
                        focus.expected.clone_from(&record.value);
                        focus.edit.composing = None;
                        focus.reveal = true;
                    }
                    focus.edit.caret = edit::snap(&record.value, focus.edit.caret);
                }
            }
        }

        // Lay out in dp, measuring text in pixels through the frame.
        self.text_meta.clear();
        self.text_meta.resize(texts.len(), TextMeta::default());
        let meta = &mut self.text_meta;
        let logical = info.logical_size();
        self.tree.compute(
            root_node,
            Rect::new(0.0, 0.0, logical.width, logical.height),
            |key, max_width| {
                let index = key as usize;
                let (Some(text), Some(meta)) = (texts.get(index), meta.get_mut(index)) else {
                    return Size::ZERO;
                };
                measure_text(frame, text, meta, &theme, scale, max_width)
            },
        );

        // Keep the field being edited clear of the keyboard and in view.
        let logical_height = logical.height;
        let keyboard_top =
            (info.keyboard_inset > 0.0).then(|| logical_height - info.keyboard_inset / scale);
        // A field inside a scroll view is scrolled into view, once per
        // change (the user may scroll it away again). A field on a screen
        // that does not scroll cannot be: while the keyboard covers it, the
        // whole screen is drawn shifted up by `pan`.
        let mut pan = 0.0;
        if let Some(focus) = self.focus.as_mut()
            && let Some(record) = self.inputs.iter().find(|record| record.id == focus.id)
            && let Some(field) = node_of(&self.records, record.slot)
        {
            let reveal = std::mem::take(&mut focus.reveal);
            let field = self.tree.rect(field);
            match record.scroll {
                Some((scroll_id, scroll_slot)) => {
                    if reveal && let Some(scroll_node) = node_of(&self.records, scroll_slot) {
                        let state = self.scrolls.entry(scroll_id).or_default();
                        let offset = reveal_offset(
                            field,
                            self.tree.rect(scroll_node),
                            keyboard_top.unwrap_or(f32::INFINITY),
                            state.offset,
                            scroll_max(&self.tree, scroll_node, keyboard_top),
                        );
                        if offset != state.offset {
                            state.offset = offset;
                            state.flinging = false;
                            state.velocity = 0.0;
                        }
                    }
                }
                None => {
                    if let Some(top) = keyboard_top {
                        pan = pan_for(field, top, info.insets.top / scale);
                    }
                }
            }
        }

        // Paint, collecting the touch regions of this frame.
        self.regions.clear();
        self.messages.clear();
        self.keyed.clear();
        self.scroll_nodes.clear();
        let surface = Rect::new(0.0, 0.0, info.size.width as f32, info.size.height as f32);
        let mut painter = Painter {
            tree: &self.tree,
            records: &self.records,
            text_meta: &self.text_meta,
            frame,
            theme: &theme,
            scale,
            dt: self
                .last_elapsed
                .map(|last| info.elapsed.saturating_sub(last).as_secs_f32()),
            regions: &mut self.regions,
            messages: &mut self.messages,
            keyed: &mut self.keyed,
            scrolls: &mut self.scrolls,
            scroll_nodes: &mut self.scroll_nodes,
            generation: self.generation,
            pressed: self
                .gesture
                .and_then(|gesture| gesture.press)
                .filter(|press| press.inside)
                .map(|press| press.id),
            dragging: self
                .gesture
                .filter(|gesture| gesture.drag.is_some())
                .and_then(|gesture| gesture.scroll),
            animating: false,
            node_cursor: 0,
            text_cursor: 0,
            fields: &self.fields,
            focus: &mut self.focus,
            elapsed: info.elapsed,
            keyboard_top,
        };
        painter.paint(
            &root,
            Context {
                id: root.key.unwrap_or(ROOT_PATH),
                dx: 0.0,
                dy: -(pan * scale).round(),
                clip: surface,
                cull: None,
                scroll: None,
                field: None,
                space: None,
            },
        );
        if painter.animating {
            frame.request_redraw();
        }
        self.last_elapsed = Some(info.elapsed);

        // Ask for the keyboard for as long as a field has focus.
        if let Some(focus) = self.focus.as_ref()
            && let Some(record) = self.inputs.iter().find(|record| record.id == focus.id)
        {
            frame.set_text_input(TextInputRequest {
                session: focus.id,
                traits: record.traits,
                text: record.value.clone(),
                caret: edit::snap(&record.value, focus.edit.caret),
            });
        }

        // Widget state lives as long as the widget is on screen.
        let generation = self.generation;
        self.scrolls.retain(|_, state| state.seen == generation);
        if let Some(gesture) = self.gesture.as_mut() {
            if let Some(press) = gesture.press
                && !self.regions.iter().any(|region| {
                    region.id == press.id
                        && matches!(region.kind, RegionKind::Press(_) | RegionKind::Focus(_))
                })
            {
                gesture.press = None;
            }
            if gesture
                .scroll
                .is_some_and(|id| !self.scrolls.contains_key(&id))
            {
                gesture.scroll = None;
                gesture.drag = None;
            }
            if gesture.press.is_none() && gesture.scroll.is_none() {
                self.gesture = None;
            }
        }
    }

    fn pointer(&mut self, event: &PointerEvent) {
        match event.phase {
            PointerPhase::Down => self.pointer_down(event),
            PointerPhase::Moved => self.pointer_moved(event),
            PointerPhase::Up => self.pointer_up(event),
            PointerPhase::Cancelled => self.pointer_cancelled(event),
        }
    }

    fn text_input(&mut self, event: &TextInputEvent) {
        self.text_event(event);
    }

    fn back(&mut self) -> bool {
        Ui::back(self)
    }
}

/// The layout node of the element with record `slot`.
fn node_of(records: &[NodeRecord], slot: u32) -> Option<NodeId> {
    records.get(slot as usize).and_then(|record| record.node)
}

/// How far a scroll view can scroll, in dp. `keyboard_top` is where the
/// soft keyboard's top edge is while it is up: a viewport that reaches
/// under it can scroll that much further, so its last rows can be brought
/// clear of the keyboard.
fn scroll_max(tree: &LayoutTree, node: NodeId, keyboard_top: Option<f32>) -> f32 {
    let viewport = tree.rect(node);
    let padding_bottom = tree.style(node).map_or(0.0, |style| style.padding.bottom);
    let content_bottom = tree
        .children(node)
        .map(|child| {
            let rect = tree.rect(child);
            rect.y + rect.height
        })
        .fold(viewport.y, f32::max);
    let content = content_bottom - viewport.y + padding_bottom;
    let covered = keyboard_top.map_or(0.0, |top| {
        (viewport.y + viewport.height - top).clamp(0.0, viewport.height)
    });
    (content - viewport.height + covered).max(0.0)
}

/// How far up (dp) a screen that cannot scroll is shifted so that a focused
/// field at `field` shows above a keyboard whose top is at `keyboard_top`:
/// enough to clear it with a margin, but never so far that the field would
/// go above `safe_top` (the status bar).
fn pan_for(field: Rect, keyboard_top: f32, safe_top: f32) -> f32 {
    let covered = field.y + field.height + REVEAL_BELOW - keyboard_top;
    let room = field.y - REVEAL_ABOVE - safe_top;
    covered.min(room).max(0.0)
}

/// The scroll offset (dp) that brings a focused field into view.
///
/// `field` and `viewport` are layout rectangles (the field's as laid out,
/// before scrolling); `visible_bottom` is the top of the keyboard, or
/// infinity. The field, with a margin, is kept above the lower of the
/// viewport's bottom and the keyboard, and below the viewport's top; when
/// it cannot be both, its top wins. The result is within `0..=max`, and is
/// `offset` unchanged when the field is already in view.
fn reveal_offset(field: Rect, viewport: Rect, visible_bottom: f32, offset: f32, max: f32) -> f32 {
    let top = viewport.y;
    let bottom = (viewport.y + viewport.height).min(visible_bottom);
    let mut target = offset;
    let field_bottom = field.y + field.height + REVEAL_BELOW - target;
    if field_bottom > bottom {
        target += field_bottom - bottom;
    }
    let field_top = field.y - REVEAL_ABOVE - target;
    if field_top < top {
        target -= top - field_top;
    }
    clamp_offset(target, max)
}

fn clamp_offset(offset: f32, max: f32) -> f32 {
    if offset.is_nan() {
        0.0
    } else {
        offset.min(max).max(0.0)
    }
}

/// The text style, in physical pixels, of a text under a theme.
fn text_style(text: &TextNode, theme: &Theme, scale: f32) -> TextStyle {
    let spec = theme.text(text.role);
    let size = text.size.unwrap_or(spec.size);
    TextStyle {
        // NaN falls through `min` and is replaced by the text engine.
        size: (size * scale).min(MAX_FONT_PIXELS),
        line_height: match text.line_height {
            None => spec.line_height,
            Some(LineHeight::Multiple(multiple)) => multiple,
            // A length: the same in dp and in pixels as a multiple.
            Some(LineHeight::Dp(dp)) if size > 0.0 => dp / size,
            Some(LineHeight::Dp(_)) => spec.line_height,
        },
        color: text.color.resolve(theme),
        weight: text.weight.unwrap_or(spec.weight),
        align: text.align,
        family: text.family.unwrap_or(theme.font(text.role)),
        letter_spacing: text.letter_spacing * scale,
        italic: text.italic,
        tabular_nums: text.tabular_nums,
    }
}

/// Measures a text node: its spans if it has any, its content otherwise.
fn measure_node(
    frame: &mut Frame,
    text: &TextNode,
    style: &TextStyle,
    theme: &Theme,
    constraint: Option<f32>,
) -> Size {
    if text.spans.is_empty() {
        frame.measure_text(&text.content, style, constraint)
    } else {
        frame.measure_text_spans(&text_spans(text, theme), style, constraint)
    }
}

/// The spans of a rich text, with theme colours resolved.
fn text_spans<'a>(text: &'a TextNode, theme: &Theme) -> Vec<TextSpan<'a>> {
    text.spans
        .iter()
        .map(|span| TextSpan {
            text: &span.content,
            weight: span.weight,
            color: span.color.map(|color| color.resolve(theme)),
            family: span.family,
            italic: span.italic.then_some(true),
        })
        .collect()
}

/// The layout engine's measure callback for one text. Constraints arrive
/// and sizes leave in dp; the text engine works in pixels.
fn measure_text(
    frame: &mut Frame,
    text: &TextNode,
    meta: &mut TextMeta,
    theme: &Theme,
    scale: f32,
    max_width: Option<f32>,
) -> Size {
    let style = text_style(text, theme, scale);
    // A single-line input never wraps: it scrolls sideways instead.
    let constraint = max_width
        .filter(|_| !text.no_wrap)
        .map(|width| width * scale);

    let measured = match (constraint, meta.natural) {
        // The layout engine assumes that text reporting a natural width
        // fits any box at least that wide. Rounding in dp arithmetic can
        // offer a hair less than the natural width; wrapping there would
        // drop the last word onto a second line. Keep the unwrapped layout.
        (Some(width), Some(natural)) if width + 0.5 >= natural.width => natural,
        _ => {
            let size = measure_node(frame, text, &style, theme, constraint);
            if constraint.is_none() {
                meta.natural = Some(size);
            }
            meta.constraint = constraint;
            size
        }
    };
    meta.measured = measured;

    let mut height = measured.height;
    if let Some(lines) = text.max_lines {
        let line = style.size * style.line_height.max(0.0);
        height = height.min(line * lines as f32);
    }
    // Whole pixels wide, so the box never cuts into the last glyph.
    let mut width = measured.width.ceil() / scale;
    if text.no_wrap
        && let Some(max_width) = max_width
    {
        // What does not fit is clipped, not laid out past the box.
        width = width.min(max_width.max(0.0));
    }
    Size::new(width, height / scale)
}

/// Builds the layout tree for an element tree.
struct Lowering<'a, 't, M> {
    tree: &'a mut LayoutTree,
    records: &'a mut Vec<NodeRecord>,
    scratch: &'a mut Vec<NodeId>,
    texts: Vec<&'t TextNode>,
    /// Safe-area insets in dp.
    insets: Edges,
    theme: &'a Theme,
    scale: f32,
    /// The text inputs, in tree order. Overwritten in place so that the
    /// strings keep their allocations; `input_count` are current.
    inputs: &'a mut Vec<InputRecord<M>>,
    input_count: usize,
    fields: &'a mut Vec<(u32, u64)>,
    /// Input frames being lowered that have no input yet: their records.
    open_fields: Vec<(u32, bool)>,
    /// Scroll views being lowered: identity and record.
    open_scrolls: Vec<(u64, u32)>,
}

impl<'t, M: Clone> Lowering<'_, 't, M> {
    /// Adds `element` and its subtree; `parent` is the axis of the box it
    /// sits in, `None` for the root; `id` is the element's identity, the
    /// same one painting gives it.
    fn lower(&mut self, element: &'t Element<M>, parent: Option<Axis>, id: u64) -> NodeId {
        let slot = self.records.len();
        self.records.push(NodeRecord {
            node: None,
            end_node: 0,
            end_text: 0,
        });

        let mut style = element.layout;
        if parent.is_none() {
            // The root always covers the surface.
            style.width = Sizing::FILL;
            style.height = Sizing::FILL;
        }

        let node = match &element.kind {
            Kind::Text(text) => {
                let key = self.texts.len() as u64;
                self.texts.push(text);
                self.tree.measured(style, key)
            }
            Kind::Image(_) | Kind::Icon(_) => self.tree.leaf(style),
            Kind::Input(input) => {
                let key = self.texts.len() as u64;
                self.texts.push(&input.text);
                if input.text.content.is_empty() {
                    // Nothing to measure: an empty field is still one line
                    // tall, for its caret.
                    let text = text_style(&input.text, self.theme, self.scale);
                    let line = text.size * text.line_height.max(0.0) / self.scale;
                    if line.is_finite() {
                        style.min_height = style.min_height.max(line);
                    }
                }
                self.record_input(input, id, slot as u32);
                self.tree.measured(style, key)
            }
            Kind::Box(node) => {
                if node.field {
                    self.open_fields.push((slot as u32, false));
                }
                if node.scroll {
                    self.open_scrolls.push((id, slot as u32));
                }
                let start = self.scratch.len();
                for (index, child) in node.children.iter().enumerate() {
                    let child_id = child.key.unwrap_or_else(|| child_path(id, index));
                    let child = self.lower(child, Some(node.axis), child_id);
                    self.scratch.push(child);
                }
                if node.field {
                    self.open_fields.pop();
                }
                if node.scroll {
                    self.open_scrolls.pop();
                }

                match (node.spacer, parent) {
                    (Some(SpacerKind::Fill(weight)), Some(Axis::Row)) => {
                        style.width = Sizing::Fill(weight);
                    }
                    (Some(SpacerKind::Fill(weight)), Some(Axis::Column)) => {
                        style.height = Sizing::Fill(weight);
                    }
                    (Some(SpacerKind::Fill(_)), Some(Axis::Stack)) => {
                        style.width = Sizing::FILL;
                        style.height = Sizing::FILL;
                    }
                    (Some(SpacerKind::Fixed(length)), Some(Axis::Row)) => {
                        style.width = Sizing::Fixed(length);
                    }
                    (Some(SpacerKind::Fixed(length)), Some(Axis::Column)) => {
                        style.height = Sizing::Fixed(length);
                    }
                    (Some(SpacerKind::Fixed(length)), Some(Axis::Stack)) => {
                        style.width = Sizing::Fixed(length);
                        style.height = Sizing::Fixed(length);
                    }
                    (_, None) | (None, _) => {}
                }

                let safe = node.safe;
                let inset = |on: bool, value: f32| if on { value.max(0.0) } else { 0.0 };
                let clean = |value: f32| if value > 0.0 { value } else { 0.0 };
                style.padding = Edges::new(
                    clean(style.padding.left) + inset(safe.left, self.insets.left),
                    clean(style.padding.top) + inset(safe.top, self.insets.top),
                    clean(style.padding.right) + inset(safe.right, self.insets.right),
                    clean(style.padding.bottom) + inset(safe.bottom, self.insets.bottom),
                );

                let children = &self.scratch[start..];
                let id = match node.axis {
                    Axis::Row => self.tree.row(style, children),
                    Axis::Column => self.tree.column(style, children),
                    Axis::Stack => self.tree.stack(style, children),
                };
                self.scratch.truncate(start);
                id
            }
        };

        self.records[slot] = NodeRecord {
            node: Some(node),
            end_node: self.records.len() as u32,
            end_text: self.texts.len() as u32,
        };
        node
    }

    /// Notes a text input: for its frame, and for typing between frames.
    fn record_input(&mut self, input: &InputNode<M>, id: u64, slot: u32) {
        if let Some((frame, taken)) = self.open_fields.last_mut()
            && !*taken
        {
            *taken = true;
            self.fields.push((*frame, id));
        }
        let scroll = self.open_scrolls.last().copied();
        match self.inputs.get_mut(self.input_count) {
            Some(record) => {
                record.id = id;
                record.slot = slot;
                record.scroll = scroll;
                record.value.clone_from(&input.value);
                record.config = input.config;
                record.traits = input.traits;
                record.auto_advance = input.auto_advance;
                record.on_input.clone_from(&input.on_input);
                record.on_submit.clone_from(&input.on_submit);
            }
            None => self.inputs.push(InputRecord {
                id,
                slot,
                scroll,
                value: input.value.clone(),
                config: input.config,
                traits: input.traits,
                auto_advance: input.auto_advance,
                on_input: input.on_input.clone(),
                on_submit: input.on_submit.clone(),
            }),
        }
        self.input_count += 1;
    }
}

/// What a subtree inherits while being painted.
#[derive(Clone, Copy)]
struct Context {
    /// Identity of the element being painted.
    id: u64,
    /// Whole-pixel translation from enclosing scroll views.
    dx: f32,
    dy: f32,
    /// Intersection of the enclosing clips, in pixels.
    clip: Rect,
    /// Inside a scroll view: the viewport grown by a margin. Subtrees
    /// wholly outside it are not painted.
    cull: Option<Rect>,
    /// The innermost enclosing scroll view.
    scroll: Option<u64>,
    /// The text input whose frame encloses this element.
    field: Option<u64>,
    /// Inside a transformed widget: how to get from the screen to the
    /// space `clip` and the rectangles painted here are in.
    space: Option<HitSpace>,
}

/// Walks the element tree again, in the same order as [`Lowering`], and
/// turns each laid-out element into drawing operations and touch regions.
struct Painter<'a, M> {
    tree: &'a LayoutTree,
    records: &'a [NodeRecord],
    text_meta: &'a [TextMeta],
    frame: &'a mut Frame,
    theme: &'a Theme,
    scale: f32,
    /// Seconds since the previous frame was built.
    dt: Option<f32>,
    regions: &'a mut Vec<Region>,
    messages: &'a mut Vec<M>,
    keyed: &'a mut Vec<(u64, Rect)>,
    scrolls: &'a mut HashMap<u64, ScrollState>,
    scroll_nodes: &'a mut Vec<NodeId>,
    generation: u64,
    pressed: Option<u64>,
    dragging: Option<u64>,
    animating: bool,
    node_cursor: usize,
    text_cursor: usize,
    /// Input frames: (record, input identity).
    fields: &'a [(u32, u64)],
    focus: &'a mut Option<Focus>,
    /// The frame's time, for the caret's blinking.
    elapsed: Duration,
    /// Where the top of the soft keyboard is (dp), while it is up.
    keyboard_top: Option<f32>,
}

impl<M: Clone> Painter<'_, M> {
    /// A layout rectangle (dp) as device pixels: edges snapped to the
    /// pixel grid, so neighbours that share an edge still do, then moved by
    /// the scroll translation. Anything with a size keeps at least one
    /// pixel of it (a 1 dp hairline at a scale below 1).
    fn pixels(&self, rect: Rect, context: &Context) -> Rect {
        let snap = |value: f32| (value * self.scale).round();
        let (left, top) = (snap(rect.x), snap(rect.y));
        let mut right = snap(rect.x + rect.width);
        let mut bottom = snap(rect.y + rect.height);
        if rect.width > 0.0 && right <= left {
            right = left + 1.0;
        }
        if rect.height > 0.0 && bottom <= top {
            bottom = top + 1.0;
        }
        Rect::new(
            left + context.dx,
            top + context.dy,
            right - left,
            bottom - top,
        )
    }

    fn paint(&mut self, element: &Element<M>, mut context: Context) {
        let Some(record) = self.records.get(self.node_cursor).copied() else {
            return;
        };
        let slot = self.node_cursor as u32;
        self.node_cursor += 1;
        let Some(node) = record.node else {
            return;
        };

        // The element's transform, with its lengths in pixels. One that
        // only moves is the same as a scroll translation: whole pixels,
        // no display-list transform, and clipping, culling and hit-testing
        // carry on as they are.
        let mut transform = element
            .transform
            .map(|transform| transform.with_translation_scaled(self.scale))
            .filter(|transform| !transform.is_identity());
        if let Some(moved) = transform.filter(Transform::is_translation) {
            if moved.is_finite() {
                context.dx += moved.tx.round();
                context.dy += moved.ty.round();
            }
            transform = None;
        }

        let rect = self.pixels(self.tree.rect(node), &context);
        if element.key.is_some() {
            self.keyed.push((context.id, rect));
        }

        if let Some(transform) = transform {
            // About the element's centre, as CSS does by default.
            let transform = transform.about(rect.center());
            let Some(inverse) = transform.inverse() else {
                // Flattened to nothing (a zero scale): the subtree is
                // neither seen nor touched.
                self.node_cursor = record.end_node as usize;
                self.text_cursor = record.end_text as usize;
                return;
            };
            self.frame.list.push_transform(transform);
            // Clips and the cull rectangle from outside are not rectangles
            // in the turned space. The display list still clips what is
            // drawn; here they are kept only for hit-testing, on screen.
            context.space = Some(match context.space {
                None => HitSpace {
                    inverse,
                    clip: context.clip,
                },
                Some(outer) => HitSpace {
                    inverse: outer.inverse.then(inverse),
                    clip: outer.clip,
                },
            });
            context.clip = EVERYWHERE;
            context.cull = None;
        }

        match &element.kind {
            Kind::Text(text) => {
                let index = self.text_cursor;
                self.text_cursor += 1;
                self.paint_text(text, index, rect, &context);
            }
            Kind::Image(image) => self.paint_image(image, rect, &context),
            Kind::Icon(icon) => self.paint_icon(icon, rect, &context),
            Kind::Input(input) => {
                let index = self.text_cursor;
                self.text_cursor += 1;
                self.paint_input(input, index, rect, &context);
            }
            Kind::Box(node_data) => self.paint_box(node_data, node, slot, rect, context),
        }

        if transform.is_some() {
            self.frame.list.pop_transform();
        }
    }

    /// A text input: exactly its text while it is not being edited; with
    /// focus, also the caret, the composing underline and, for a single
    /// line wider than the field, a sideways scroll that keeps the caret
    /// in view.
    fn paint_input(&mut self, input: &InputNode<M>, index: usize, rect: Rect, context: &Context) {
        let visible = intersect(rect, context.clip);
        if !visible.is_empty() {
            self.regions.push(Region {
                id: context.id,
                rect: visible,
                kind: RegionKind::Focus(context.id),
                scroll: context.scroll,
                field: Some(context.id),
                space: context.space,
            });
        }
        let Some(meta) = self.text_meta.get(index).copied() else {
            return;
        };
        // Only a single line can be wider than its box.
        let overflow = meta.measured.width > rect.width + 0.5;
        let Some(mut focus) = self.focus.take_if(|focus| focus.id == context.id) else {
            if overflow {
                self.frame.list.push_clip(rect);
            }
            self.paint_text(&input.text, index, rect, context);
            if overflow {
                self.frame.list.pop_clip();
            }
            return;
        };

        let style = text_style(&input.text, self.theme, self.scale);
        // The placeholder is what was laid out for an empty field; the
        // caret belongs at its start.
        let shown: &str = if input.empty { "" } else { &input.text.content };
        let layout_width = meta.constraint.unwrap_or(meta.measured.width);
        let slack = rect.width - layout_width;
        let shift = match input.text.align {
            TextAlign::Start => 0.0,
            TextAlign::Center => slack / 2.0,
            TextAlign::End => slack,
        };
        let origin_x = (rect.x + shift).clamp(-MAX_TEXT_ORIGIN, MAX_TEXT_ORIGIN);
        let origin_y = rect.y.clamp(-MAX_TEXT_ORIGIN, MAX_TEXT_ORIGIN);
        let caret_width = (CARET_WIDTH * self.scale).round().max(1.0);

        if let Some(tap) = focus.tap.take() {
            // In a transformed widget the finger is where the turned text
            // is drawn; bring it back to the space the text is laid out in.
            let tap = context.space.map_or(tap, |space| space.inverse.apply(tap));
            let local = Point::new(tap.x - origin_x + focus.scroll_x, tap.y - origin_y);
            let at = self
                .frame
                .text_index_at(shown, &style, meta.constraint, local);
            focus.edit.caret = edit::value_index(&input.value, at, input.secure);
            // Moving the caret ends a composition where it stands.
            focus.edit.composing = None;
        }
        let place =
            |value_index: usize| edit::display_index(&input.value, value_index, input.secure);
        let caret = self
            .frame
            .text_caret(shown, &style, meta.constraint, place(focus.edit.caret));
        if overflow && !input.empty {
            // The caret may sit on the right edge itself: the clip below
            // is one caret wider than the box.
            if caret.x - focus.scroll_x > rect.width {
                focus.scroll_x = caret.x - rect.width;
            }
            if caret.x - focus.scroll_x < 0.0 {
                focus.scroll_x = caret.x;
            }
            let furthest = (meta.measured.width - rect.width).max(0.0);
            focus.scroll_x = focus.scroll_x.clamp(0.0, furthest).round();
        } else {
            focus.scroll_x = 0.0;
        }

        let clip = overflow || meta.measured.height > rect.height + 0.5;
        if clip {
            self.frame.list.push_clip(Rect::new(
                rect.x,
                rect.y,
                rect.width + caret_width,
                rect.height,
            ));
        }
        let origin = Point::new(origin_x - focus.scroll_x, origin_y);
        self.frame
            .draw_text(&input.text.content, &style, origin, meta.constraint);

        // Text the keyboard is still composing is underlined.
        if let Some((from, to)) = focus.edit.composing.filter(|_| !input.empty) {
            let start = self
                .frame
                .text_caret(shown, &style, meta.constraint, place(from));
            let end = self
                .frame
                .text_caret(shown, &style, meta.constraint, place(to));
            if start.y == end.y {
                let thickness = self.scale.round().max(1.0);
                let (left, right) = (start.x.min(end.x), start.x.max(end.x));
                let y = (start.y + start.height / 2.0 + style.size * 0.47)
                    .min(start.y + start.height - thickness);
                self.frame.list.fill_rounded_rect(
                    Rect::new(
                        (origin.x + left).round(),
                        (origin.y + y).round(),
                        (right - left).round(),
                        thickness,
                    ),
                    0.0,
                    style.color,
                );
            }
        }

        // The caret: solid for half a second, gone for half a second, by
        // asking for one frame at each change and none in between.
        let from = *focus.blink_from.get_or_insert(self.elapsed);
        let since = self.elapsed.saturating_sub(from);
        let phase = (since.as_millis() / CARET_BLINK.as_millis()) as u32;
        if phase.is_multiple_of(2) {
            let height = (style.size * 1.3).min(caret.height).max(1.0);
            let top = origin.y + caret.y + (caret.height - height) / 2.0;
            self.frame.list.fill_rounded_rect(
                Rect::new(
                    (origin.x + caret.x).round(),
                    top.round(),
                    caret_width,
                    height.round(),
                ),
                0.0,
                input.caret_color.resolve(self.theme),
            );
        }
        let next = CARET_BLINK
            .saturating_mul(phase.saturating_add(1))
            .saturating_sub(since)
            .max(Duration::from_millis(1));
        self.frame.request_redraw_after(next);

        if clip {
            self.frame.list.pop_clip();
        }
        *self.focus = Some(focus);
    }

    fn paint_text(&mut self, text: &TextNode, index: usize, rect: Rect, context: &Context) {
        let Some(meta) = self.text_meta.get(index) else {
            return;
        };
        if intersect(rect, context.clip).is_empty() {
            return;
        }
        let style = text_style(text, self.theme, self.scale);
        // The lines were aligned within the width the text was laid out
        // at, which is not always the width of its box.
        let layout_width = meta.constraint.unwrap_or(meta.measured.width);
        let slack = rect.width - layout_width;
        let shift = match text.align {
            TextAlign::Start => 0.0,
            TextAlign::Center => slack / 2.0,
            TextAlign::End => slack,
        };
        let origin = Point::new(
            (rect.x + shift).clamp(-MAX_TEXT_ORIGIN, MAX_TEXT_ORIGIN),
            rect.y.clamp(-MAX_TEXT_ORIGIN, MAX_TEXT_ORIGIN),
        );
        let cut = meta.measured.height > rect.height + 0.5;
        if cut {
            self.frame.list.push_clip(rect);
        }
        if text.shadow.is_none() && text.outline.is_none() {
            // The common case, with no list of spans to build for a text
            // in one style.
            if text.spans.is_empty() {
                self.frame
                    .draw_text(&text.content, &style, origin, meta.constraint);
            } else {
                let spans = text_spans(text, self.theme);
                self.frame
                    .draw_text_spans(&spans, &style, origin, meta.constraint);
            }
        } else {
            let mut spans = if text.spans.is_empty() {
                vec![TextSpan::new(&text.content)]
            } else {
                text_spans(text, self.theme)
            };
            if let Some((x, y, color)) = text.shadow {
                // The same glyphs once more, behind, all in one colour.
                let shadow = TextStyle {
                    color: color.resolve(self.theme),
                    ..style
                };
                let offset = Point::new(
                    origin.x + (x * self.scale).round(),
                    origin.y + (y * self.scale).round(),
                );
                let colors: Vec<_> = spans.iter().map(|span| span.color).collect();
                spans.iter_mut().for_each(|span| span.color = None);
                self.frame
                    .draw_text_spans(&spans, &shadow, offset, meta.constraint);
                for (span, color) in spans.iter_mut().zip(colors) {
                    span.color = color;
                }
            }
            match text.outline {
                Some((width, color)) => {
                    self.frame.draw_text_outlined(
                        &spans,
                        &style,
                        origin,
                        meta.constraint,
                        width * self.scale,
                        color.resolve(self.theme),
                    );
                }
                None => {
                    self.frame
                        .draw_text_spans(&spans, &style, origin, meta.constraint);
                }
            }
        }
        if cut {
            self.frame.list.pop_clip();
        }
    }

    fn paint_icon(&mut self, data: &IconNode, rect: Rect, context: &Context) {
        // Icons scrolled out of view cost no rasterisation.
        if intersect(rect, context.clip).is_empty() {
            return;
        }
        self.frame.draw_icon(
            &data.icon,
            rect,
            data.color.resolve(self.theme),
            data.stroke_width,
        );
    }

    fn paint_image(&mut self, data: &ImageNode, rect: Rect, context: &Context) {
        let radius = data.radius * self.scale;
        let tint = data.tint.resolve(self.theme);
        match data.source {
            PictureSource::Image(image) => {
                self.frame
                    .list
                    .draw_image_fit(image, rect, data.fit, radius, tint);
            }
            PictureSource::Vector(picture) => {
                // Pictures scrolled out of view cost no rasterisation.
                if !intersect(rect, context.clip).is_empty() {
                    self.frame.draw_picture(&picture, rect, radius, tint);
                }
            }
        }
    }

    fn paint_box(
        &mut self,
        data: &BoxNode<M>,
        node: NodeId,
        slot: u32,
        rect: Rect,
        context: Context,
    ) {
        let scale = self.scale;
        let radius = data.radius * scale;
        let pressed = data.on_press.is_some() && self.pressed == Some(context.id);
        // The input this box is the frame of, and whether it has focus.
        let field = data
            .field
            .then(|| self.fields.iter().find(|(frame, _)| *frame == slot))
            .flatten()
            .map(|(_, input)| *input);
        let focused = field.is_some() && self.focus.as_ref().map(|focus| focus.id) == field;

        if let Some(shadow) = data.shadow {
            self.frame.list.draw_shadow(
                rect,
                radius,
                Shadow {
                    offset_x: shadow.offset_x * scale,
                    offset_y: shadow.offset_y * scale,
                    blur: shadow.blur * scale,
                    spread: shadow.spread * scale,
                    color: shadow.color,
                },
            );
        }

        let border = if focused {
            data.focused_border.or(data.border)
        } else {
            data.border
        };
        let border = border.map(|(width, color)| {
            // A border thinner than a pixel would fade out; keep one.
            let width = if width > 0.0 {
                (width * scale).round().max(1.0)
            } else {
                0.0
            };
            Border::new(width, color.resolve(self.theme))
        });
        let fill = match (pressed, data.pressed_fill) {
            (true, Some(color)) => Some(Fill::Solid(color)),
            _ if focused && data.focused_fill.is_some() => data.focused_fill.map(Fill::Solid),
            _ => data.fill,
        };
        // The dash pattern in pixels. An invalid one draws a solid border.
        let dash = border.zip(data.dash).map(|(border, kind)| match kind {
            DashKind::Css => Dash::css(rect, radius, border.width),
            DashKind::Pattern(length, gap) => Dash::new(length * scale, gap * scale),
        });
        let clear = morningstar_core::Color::rgba(0.0, 0.0, 0.0, 0.0);
        match (fill, border) {
            (Some(Fill::Solid(color)), Some(border)) => {
                let fill = color.resolve(self.theme);
                match dash {
                    Some(dash) => self
                        .frame
                        .list
                        .fill_dashed_rect(rect, radius, fill, border, dash),
                    None => self
                        .frame
                        .list
                        .fill_bordered_rect(rect, radius, fill, border),
                }
            }
            (Some(Fill::Solid(color)), None) => {
                self.frame
                    .list
                    .fill_rounded_rect(rect, radius, color.resolve(self.theme));
            }
            (Some(Fill::Gradient(gradient)), border) => {
                self.frame.list.fill_gradient_rect(
                    rect,
                    radius,
                    LinearGradient::angled(
                        rect,
                        gradient.degrees,
                        gradient.from.resolve(self.theme),
                        gradient.to.resolve(self.theme),
                    ),
                );
                match (border, dash) {
                    (Some(border), Some(dash)) => self
                        .frame
                        .list
                        .fill_dashed_rect(rect, radius, clear, border, dash),
                    (Some(border), None) => {
                        self.frame.list.stroke_rounded_rect(rect, radius, border);
                    }
                    (None, _) => {}
                }
            }
            (None, Some(border)) => match dash {
                Some(dash) => self
                    .frame
                    .list
                    .fill_dashed_rect(rect, radius, clear, border, dash),
                None => self.frame.list.stroke_rounded_rect(rect, radius, border),
            },
            (None, None) => {}
        }
        if pressed && data.pressed_fill.is_none() {
            self.frame
                .list
                .fill_rounded_rect(rect, radius, self.theme.colors.pressed_overlay);
        }

        let visible = intersect(rect, context.clip);
        let mut inner = context;
        if !visible.is_empty() {
            if let Some(message) = &data.on_press {
                self.messages.push(message.clone());
                self.regions.push(Region {
                    id: context.id,
                    rect: visible,
                    kind: RegionKind::Press(self.messages.len() - 1),
                    scroll: context.scroll,
                    field: context.field,
                    space: context.space,
                });
            } else if let Some(input) = field {
                self.regions.push(Region {
                    id: context.id,
                    rect: visible,
                    kind: RegionKind::Focus(input),
                    scroll: context.scroll,
                    field,
                    space: context.space,
                });
            } else if data.block {
                self.regions.push(Region {
                    id: context.id,
                    rect: visible,
                    kind: RegionKind::Block,
                    scroll: context.scroll,
                    field: context.field,
                    space: context.space,
                });
            }
        }
        if field.is_some() {
            inner.field = field;
        }

        if data.scroll {
            let offset = self.scroll(node, context.id);
            inner.dy -= (offset * scale).round();
            inner.scroll = Some(context.id);
            if !visible.is_empty() {
                self.regions.push(Region {
                    id: context.id,
                    rect: visible,
                    kind: RegionKind::Scroll,
                    scroll: context.scroll,
                    field: context.field,
                    space: context.space,
                });
            }
        }

        let clips = data.clip || data.scroll;
        if clips {
            self.frame.list.push_rounded_clip(rect, radius);
            inner.clip = visible;
        }
        if data.scroll {
            let margin = CULL_MARGIN * scale;
            inner.cull = Some(Rect::new(
                visible.x - margin,
                visible.y - margin,
                visible.width + 2.0 * margin,
                visible.height + 2.0 * margin,
            ));
        }

        for (index, child) in data.children.iter().enumerate() {
            let child_context = Context {
                id: child.key.unwrap_or_else(|| child_path(context.id, index)),
                ..inner
            };
            if let Some(cull) = inner.cull
                && let Some(record) = self.records.get(self.node_cursor)
                && let Some(child_node) = record.node
                && intersect(self.pixels(self.tree.rect(child_node), &inner), cull).is_empty()
            {
                // Off screen: skip the subtree without painting it.
                self.node_cursor = record.end_node as usize;
                self.text_cursor = record.end_text as usize;
                continue;
            }
            self.paint(child, child_context);
        }

        if clips {
            self.frame.list.pop_clip();
        }
    }

    /// Advances the scroll view `id` to this frame and returns its offset
    /// in dp.
    fn scroll(&mut self, node: NodeId, id: u64) -> f32 {
        self.scroll_nodes.push(node);
        let max = scroll_max(self.tree, node, self.keyboard_top);

        let state = self.scrolls.entry(id).or_default();
        state.seen = self.generation;
        state.max = max;

        let step = self.dt.filter(|dt| *dt > 0.0);
        if self.dragging == Some(id) {
            // Finger speed, sampled once per frame: pointer events carry no
            // timestamps, and while a finger moves every event is followed
            // by a frame.
            match step {
                Some(dt) if dt <= VELOCITY_WINDOW => {
                    let sample = (state.offset - state.frame_offset) / dt;
                    state.velocity = if state.tracking {
                        0.4 * state.velocity + 0.6 * sample
                    } else {
                        sample
                    };
                    state.tracking = true;
                }
                _ => {
                    state.velocity = 0.0;
                    state.tracking = false;
                }
            }
        } else if state.flinging {
            match step {
                // The finger rested before lifting: no fling.
                Some(dt) if state.released && dt > VELOCITY_WINDOW => state.velocity = 0.0,
                Some(dt) => {
                    state.velocity = state.velocity.clamp(-FLING_MAX, FLING_MAX);
                    // Exponential decay, integrated exactly over the step,
                    // so the result does not depend on the frame rate.
                    let decay = (-dt / FLING_DECAY).exp();
                    state.offset += state.velocity * FLING_DECAY * (1.0 - decay);
                    state.velocity *= decay;
                }
                None => {}
            }
            state.released = false;
        }

        let clamped = clamp_offset(state.offset, max);
        if clamped != state.offset {
            // Reached an end: stop there.
            state.offset = clamped;
            state.velocity = 0.0;
        }
        if state.flinging {
            if state.velocity.abs() < FLING_STOP {
                state.flinging = false;
                state.velocity = 0.0;
            } else {
                self.animating = true;
            }
        }
        state.frame_offset = state.offset;
        state.offset
    }
}

/// Identity of the `index`-th child of the element identified by `parent`,
/// when the child has no explicit id.
fn child_path(parent: u64, index: usize) -> u64 {
    // SplitMix64's finaliser over the pair: cheap and well mixed.
    let mut value = parent ^ (index as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (value ^ (value >> 31)) & !1
}

/// The overlap of two rectangles; empty (zero-sized) if they do not meet.
fn intersect(a: Rect, b: Rect) -> Rect {
    let left = a.x.max(b.x);
    let top = a.y.max(b.y);
    let right = (a.x + a.width).min(b.x + b.width);
    let bottom = (a.y + a.height).min(b.y + b.height);
    Rect::new(left, top, (right - left).max(0.0), (bottom - top).max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEWPORT: Rect = Rect::new(0.0, 100.0, 360.0, 600.0);

    fn field(y: f32) -> Rect {
        Rect::new(20.0, y, 320.0, 22.0)
    }

    #[test]
    fn a_field_in_view_is_left_where_it_is() {
        // Viewport 100..700, nothing covering it.
        for offset in [0.0, 40.0] {
            let got = reveal_offset(field(400.0), VIEWPORT, f32::INFINITY, offset, 1000.0);
            assert_eq!(got, offset);
        }
    }

    #[test]
    fn a_field_below_the_view_is_brought_just_above_its_bottom() {
        // Field at 900..922: its bottom plus the margin goes to 700.
        let got = reveal_offset(field(900.0), VIEWPORT, f32::INFINITY, 0.0, 1000.0);
        assert_eq!(got, 922.0 + REVEAL_BELOW - 700.0);
        // Already scrolled past it: untouched.
        assert_eq!(
            reveal_offset(field(900.0), VIEWPORT, f32::INFINITY, 400.0, 1000.0),
            400.0
        );
    }

    #[test]
    fn the_keyboard_lowers_the_bottom_of_what_counts_as_visible() {
        // The keyboard's top is at 450: the field at 500 is under it.
        let got = reveal_offset(field(500.0), VIEWPORT, 450.0, 0.0, 1000.0);
        assert_eq!(got, 522.0 + REVEAL_BELOW - 450.0);
        // A keyboard below the viewport changes nothing.
        assert_eq!(
            reveal_offset(field(500.0), VIEWPORT, 900.0, 0.0, 1000.0),
            0.0
        );
    }

    #[test]
    fn a_field_above_the_view_is_brought_just_below_its_top() {
        // Scrolled 500 down, the field at 300 is at -200 on screen.
        let got = reveal_offset(field(300.0), VIEWPORT, f32::INFINITY, 500.0, 1000.0);
        assert_eq!(got, 300.0 - REVEAL_ABOVE - 100.0);
    }

    #[test]
    fn the_result_stays_within_the_scroll_range() {
        assert_eq!(
            reveal_offset(field(5000.0), VIEWPORT, f32::INFINITY, 0.0, 250.0),
            250.0
        );
        assert_eq!(
            reveal_offset(field(0.0), VIEWPORT, f32::INFINITY, 50.0, 250.0),
            0.0
        );
        // Not scrollable at all: nothing to do.
        assert_eq!(reveal_offset(field(900.0), VIEWPORT, 300.0, 0.0, 0.0), 0.0);
    }

    #[test]
    fn a_screen_that_cannot_scroll_is_panned_just_enough() {
        // Keyboard top at 500: a field at 300 is clear of it.
        assert_eq!(pan_for(field(300.0), 500.0, 30.0), 0.0);
        // A field at 520..542 is under it: up by its bottom plus margin.
        assert_eq!(
            pan_for(field(520.0), 500.0, 30.0),
            542.0 + REVEAL_BELOW - 500.0
        );
        // Never so far that the field leaves the top of the screen.
        assert_eq!(
            pan_for(field(120.0), 60.0, 30.0),
            120.0 - REVEAL_ABOVE - 30.0
        );
        assert_eq!(pan_for(field(20.0), 10.0, 30.0), 0.0);
    }

    #[test]
    fn when_the_space_is_too_small_the_top_of_the_field_wins() {
        // Only 30 dp visible between the viewport top and the keyboard.
        let tall = Rect::new(20.0, 400.0, 320.0, 200.0);
        let got = reveal_offset(tall, VIEWPORT, 130.0, 0.0, 1000.0);
        assert_eq!(got, 400.0 - REVEAL_ABOVE - 100.0);
    }
}
