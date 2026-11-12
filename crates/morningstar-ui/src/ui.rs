//! [`Application`] and [`Ui`], the adapter that runs one on the runtime:
//! view, layout, paint, hit-testing, gestures and scrolling.

use std::collections::HashMap;
use std::hash::Hash;
use std::time::Duration;

use morningstar_core::{
    AudioError, Border, CaretMove, Color, Dash, HttpError, HttpResponse, ImeAction, LinearGradient,
    Point, PointerEvent, PointerPhase, Recording, Rect, ServiceCall, ServiceId, ServiceRequest,
    ServiceResult, Shadow, Size, SystemBars, TextInputEvent, TextInputRequest, TextInputTraits,
    Transform,
};
use morningstar_layout::{Edges, LayoutTree, NodeId, Sizing};
use morningstar_runtime::{
    App, Frame, FrameInfo, Setup, TextAlign, TextSpan, TextStyle, TextWeight,
};

use crate::color::{ColorRef, with_alpha};
use crate::edit::{self, EditConfig, EditKind, EditState, History};
use crate::effects::{EffectState, Effects};
use crate::element::{
    Axis, BoxNode, Container, DashKind, Element, Fill, IconNode, ImageNode, InputHandler,
    InputNode, Kind, LineHeight, PictureSource, SpacerKind, TextNode, key_hash,
};
use crate::motion::{Animation, Easing, Ghost, Motions, Props};
use crate::scroll::{ScrollTarget, ScrollTo};
use crate::theme::Theme;
use crate::viewport::Viewport;

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
/// How an animated [`ScrollTo`] moves.
const GLIDE: Animation = Animation::new(Duration::from_millis(250), Easing::EaseOut);
/// The identity given as the parent of a tree's root.
const NO_PARENT: u64 = 0;
/// A finger held still on a field for this long selects the word under it.
const LONG_PRESS: Duration = Duration::from_millis(500);
/// Two taps on a field no further apart than this, in time and in place
/// (dp), are a double tap; a third, a triple tap.
const MULTI_TAP: Duration = Duration::from_millis(400);
const MULTI_TAP_SLOP: f32 = 24.0;
/// Opacity of the selection highlight, in the theme's primary colour
/// (Material's text selection colour).
const SELECTION_ALPHA: f32 = 0.4;
/// Diameter of a selection handle, and how far around it a touch still
/// takes hold of it (dp).
const HANDLE_SIZE: f32 = 20.0;
const HANDLE_REACH: f32 = 12.0;
/// The contextual menu over a selection (dp): its height, the padding
/// either side of each label, its corner radius, the gap between it and
/// the text, the least distance from the screen's edges, and its type size.
const MENU_HEIGHT: f32 = 44.0;
const MENU_PADDING: f32 = 14.0;
const MENU_RADIUS: f32 = 8.0;
const MENU_GAP: f32 = 8.0;
const MENU_MARGIN: f32 = 8.0;
const MENU_TEXT: f32 = 14.0;
/// While a selection handle is dragged within this distance (dp) of the
/// edge of what can scroll, or past it, the text scrolls under the
/// finger: at least `AUTO_SCROLL_MIN` dp per second, faster the further
/// the finger goes, up to `AUTO_SCROLL_MAX`.
const AUTO_SCROLL_ZONE: f32 = 24.0;
const AUTO_SCROLL_MIN: f32 = 60.0;
const AUTO_SCROLL_MAX: f32 = 900.0;
/// The labels of the contextual menu.
const MENU_LABELS: [(MenuAction, &str); 4] = [
    (MenuAction::Cut, "Cut"),
    (MenuAction::Copy, "Copy"),
    (MenuAction::Paste, "Paste"),
    (MenuAction::SelectAll, "Select all"),
];

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
    ///
    /// Every application implements this or [`view_in`](Self::view_in),
    /// which is the same with the screen's measurements at hand. (The
    /// default here is an empty screen, so that an application which
    /// implements `view_in` need not write a `view` as well.)
    fn view(&self) -> Element<Self::Message> {
        Container::empty().into()
    }

    /// [`view`](Self::view), for a screen that depends on what it is shown
    /// on: `viewport` gives the surface's size in dp, the safe-area
    /// insets, how much the keyboard covers, whether a field has focus,
    /// and measures text, so that a view can choose a layout that fits
    /// instead of guessing (see [`Viewport`]). The default ignores it and
    /// calls `view`.
    ///
    /// It is called exactly when `view` would be: once per frame. A frame
    /// is produced whenever what `viewport` reports changes (rotation, the
    /// keyboard coming or going), so what is read here is never stale.
    ///
    /// ```ignore
    /// fn view_in(&self, viewport: &Viewport) -> Element<Message> {
    ///     if viewport.width() >= 600.0 { self.two_panes() } else { self.one_pane() }
    /// }
    /// ```
    fn view_in(&self, viewport: &Viewport<'_>) -> Element<Self::Message> {
        let _ = viewport;
        self.view()
    }

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

    /// Whether the icons the system draws over the application, in the
    /// status bar and the navigation bar, should be light or dark on the
    /// screen now showing. The application is drawn edge to edge under
    /// transparent bars, so the system cannot know.
    ///
    /// `None`, the default, derives it from the theme: dark icons over a
    /// light [`background`](crate::Colors::background), light icons over a
    /// dark one ([`SystemBars::for_background`]). Override it for a screen
    /// whose top or bottom is not the theme's background (a photograph
    /// under the status bar, a dark sheet over a light app):
    ///
    /// ```ignore
    /// fn system_bars(&self) -> Option<SystemBars> {
    ///     match self.screen {
    ///         Screen::Camera => Some(SystemBars::LIGHT_CONTENT),
    ///         _ => None,
    ///     }
    /// }
    /// ```
    ///
    /// Asked once per frame; the host is told only when the answer
    /// changes.
    fn system_bars(&self) -> Option<SystemBars> {
        None
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
    /// A selection handle of the focused field: dragging it moves that
    /// end of the selection.
    Handle(SelectionEnd),
    /// An item of the contextual menu over the selection.
    Menu(MenuAction),
}

/// One end of a selection, in reading order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SelectionEnd {
    Start,
    End,
}

/// What an item of the contextual menu does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MenuAction {
    Cut,
    Copy,
    Paste,
    SelectAll,
}

/// Something the focused field must do that needs its text laid out, and
/// so waits for the next frame.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Pending {
    /// Put the caret where this tap (px) was.
    Caret(Point),
    /// Select the word at this point (a double tap, a long press).
    Word(Point),
    /// Select everything (a triple tap).
    All,
    /// A key that moves by lines.
    Move(CaretMove, bool),
}

/// The last tap on a text input, for telling a double tap from two taps.
#[derive(Clone, Copy, Debug)]
struct Tap {
    input: u64,
    /// The time of the frame before the finger lifted.
    at: Duration,
    position: Point,
    count: u8,
}

/// Where the last frame drew the focused field's selection, in pixels on
/// the screen: what the handles and the menu are placed by, and what a
/// touch on a handle is measured against.
#[derive(Clone, Copy, Debug)]
struct SelectionOverlay {
    input: u64,
    /// The carets at the two ends, in reading order (the same twice for a
    /// caret alone): zero wide, one line tall.
    start: Rect,
    end: Rect,
    /// The part of the field that can be seen.
    visible: Rect,
    selected: bool,
    /// The text reads right to left: the handles lean the other way.
    rtl: bool,
    handles: bool,
    menu: bool,
    can_cut: bool,
    can_copy: bool,
    can_paste: bool,
    can_select_all: bool,
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
    /// What the next frame works out with the text laid out: a tap's caret
    /// position, the word under a long press, a move by lines.
    pending: Option<Pending>,
    /// Where a selection handle is being dragged to (px), already moved
    /// from the finger to the line it stands for.
    drag: Option<Point>,
    /// The selection shows its two handles (it was made by touch).
    handles: bool,
    /// The contextual menu is up.
    menu: bool,
    /// The edits made since the field got the focus, for undo.
    history: History,
    /// Scroll the field into view at the next frame.
    reveal: bool,
    /// How far (dp) the screen is shifted up for this field, where
    /// scrolling could not bring it clear of the keyboard.
    pan: f32,
    /// The caret may have moved: a field that scrolls inside itself
    /// brings it into view at the next frame.
    caret_moved: bool,
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
    fn new(id: u64, pending: Option<Pending>) -> Self {
        Self {
            id,
            edit: EditState::at(usize::MAX),
            expected: String::new(),
            known: false,
            scroll_x: 0.0,
            blink_from: None,
            pending,
            drag: None,
            handles: false,
            menu: false,
            history: History::default(),
            reveal: true,
            pan: 0.0,
            caret_moved: true,
        }
    }

    /// The field's text, caret or place may have changed: bring both the
    /// field and its caret into view at the next frame.
    fn touched(&mut self) {
        self.reveal = true;
        self.caret_moved = true;
    }

    /// The handles and the menu go away: the selection was replaced,
    /// typed over or given up.
    fn dismiss(&mut self) {
        self.handles = false;
        self.menu = false;
        self.drag = None;
    }
}

/// What the last frame knew about one text input, in tree order. Kept so
/// that typing, and moving between fields, work between frames.
struct InputRecord<M> {
    id: u64,
    /// Its record in `Ui::records`.
    slot: u32,
    /// The innermost scroll view around it, in `Ui::scroll_table`.
    scroll: Option<u32>,
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
    /// A selection handle is being dragged: from the finger to the middle
    /// of the line the handle hangs from (px).
    handle: Option<Point>,
    /// The frame time at which the finger was first seen down, for a long
    /// press; `None` until a frame has been built with it down.
    held_since: Option<Duration>,
    /// The finger has moved too far to still be pressing, or its long
    /// press has happened: lifting it is then not a tap.
    moved: bool,
    long_pressed: bool,
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
    /// An animated move asked for by the application.
    glide: Option<Glide>,
}

/// An animated move of a scroll view to an offset (dp).
#[derive(Clone, Copy, Debug)]
struct Glide {
    from: f32,
    to: f32,
    /// The frame time it started at; `None` until its first frame.
    start: Option<Duration>,
}

impl ScrollState {
    /// Moves to `to` (dp), at once or gliding. Whatever motion there was
    /// ends.
    fn go(&mut self, to: f32, glide: bool) {
        self.flinging = false;
        self.released = false;
        self.velocity = 0.0;
        if glide && to != self.offset {
            self.glide = Some(Glide {
                from: self.offset,
                to,
                start: None,
            });
        } else {
            self.glide = None;
            self.offset = to;
        }
    }
}

/// A scroll view of the frame, in tree order.
#[derive(Clone, Copy, Debug)]
struct ScrollRecord {
    id: u64,
    /// Its record in `Ui::records`.
    slot: u32,
    /// The scroll view around it, in the same table.
    parent: Option<u32>,
}

/// An element of the frame with an explicit `.id(..)`, in tree order.
#[derive(Clone, Copy, Debug)]
struct KeyedRecord {
    id: u64,
    slot: u32,
    /// The innermost scroll view around it, in `Ui::scroll_table`.
    scroll: Option<u32>,
}

/// Where an element with an explicit id was painted, in pixels.
#[derive(Clone, Copy, Debug)]
struct KeyedRect {
    id: u64,
    /// As laid out and scrolled.
    rect: Rect,
    /// The part of it its clips leave to be seen; empty if none.
    visible: Rect,
}

/// What the layout pass learned about one text.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TextMeta {
    /// The wrap width (px) of the layout that will be drawn.
    pub constraint: Option<f32>,
    /// Size (px) of that layout, before any `max_lines` cut.
    pub measured: Size,
    /// The unwrapped size (px), if it was asked for.
    pub natural: Option<Size>,
    /// Height of one line (px).
    pub line_height: f32,
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
/// [`visible_rect_of`](Self::visible_rect_of),
/// [`scroll_offset`](Self::scroll_offset), [`scroll_to`](Self::scroll_to)
/// and [`overflows`](Self::overflows) reach in from outside.
pub struct Ui<A: Application> {
    app: A,
    tree: LayoutTree,
    records: Vec<NodeRecord>,
    scratch: Vec<NodeId>,
    text_meta: Vec<TextMeta>,
    regions: Vec<Region>,
    messages: Vec<A::Message>,
    keyed: Vec<KeyedRect>,
    /// The frame's elements with explicit ids, and its scroll views.
    keyed_table: Vec<KeyedRecord>,
    scroll_table: Vec<ScrollRecord>,
    /// What the application asked to be scrolled, until the next frame.
    scroll_requests: Vec<ScrollTo>,
    scroll_nodes: Vec<NodeId>,
    scrolls: HashMap<u64, ScrollState>,
    gesture: Option<Gesture>,
    inputs: Vec<InputRecord<A::Message>>,
    /// Boxes that are the frame of an input: (record, input identity).
    fields: Vec<(u32, u64)>,
    focus: Option<Focus>,
    /// The last tap on an input, and what the last frame drew of the
    /// focused input's selection.
    last_tap: Option<Tap>,
    overlay: Option<SelectionOverlay>,
    /// A paste waiting for the clipboard: the request, and the input.
    paste: Option<(ServiceId, u64)>,
    /// Less motion was asked for with `set_reduce_motion`, whatever the
    /// host says.
    forced_reduce: bool,
    keyboard_inset: f32,
    scale: f32,
    last_elapsed: Option<Duration>,
    generation: u64,
    /// The application's timer, as of the last frame.
    tick: Option<Tick>,
    /// Requests to the platform and the handlers waiting for answers.
    effects: EffectState<A::Message>,
    /// Animations in progress.
    motions: Motions,
    /// Elements that have left the tree and are still seen leaving, and
    /// for each its layout node and first record and text of this frame.
    ghosts: Vec<Ghost<Element<A::Message>>>,
    ghost_roots: Vec<(NodeId, u32, u32)>,
    /// The last frame's tree, kept only while it holds a widget with a
    /// transition (which may be found to have left in the next frame).
    last_root: Option<Element<A::Message>>,
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
            keyed_table: Vec::new(),
            scroll_table: Vec::new(),
            scroll_requests: Vec::new(),
            scroll_nodes: Vec::new(),
            scrolls: HashMap::new(),
            gesture: None,
            inputs: Vec::new(),
            fields: Vec::new(),
            focus: None,
            last_tap: None,
            overlay: None,
            paste: None,
            forced_reduce: false,
            keyboard_inset: 0.0,
            scale: 1.0,
            last_elapsed: None,
            generation: 0,
            tick: None,
            effects,
            motions: Motions::new(),
            ghosts: Vec::new(),
            ghost_roots: Vec::new(),
            last_root: None,
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
        self.effects.collect(&mut self.scroll_requests);
    }

    /// Hands the application the message an answer becomes, if it is
    /// still waiting for that answer.
    fn deliver_result(&mut self, id: ServiceId, result: ServiceResult) -> bool {
        if let Some((_, input)) = self.paste.take_if(|(paste, _)| *paste == id) {
            // The clipboard, for the field that asked to paste.
            self.effects.settle(id);
            if let ServiceResult::Clipboard(Some(text)) = result {
                self.paste_into(input, &text);
            }
            return true;
        }
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

    /// Where the widget with this explicit `.id(..)` was laid out in the
    /// last frame, in physical pixels, or `None` if it was not in that
    /// frame's tree. For a widget inside a scroll view this is its
    /// scrolled position, whether or not that is inside the viewport: a
    /// row far down a list has a rectangle below the screen. Ask
    /// [`visible_rect_of`](Self::visible_rect_of) for what can be seen.
    ///
    /// Two approximations: for a widget in a subtree that was skipped
    /// because it is far outside its scroll view, a scroll view or a
    /// moving transform *between* the two is not counted; and a widget
    /// that is leaving (see [`Container::transition`]) has no rectangle.
    pub fn rect_of(&self, id: impl Hash) -> Option<Rect> {
        let key = key_hash(id);
        self.keyed
            .iter()
            .find(|keyed| keyed.id == key)
            .map(|keyed| keyed.rect)
    }

    /// The part of the widget with this explicit `.id(..)` that the last
    /// frame showed: [`rect_of`](Self::rect_of) cut down to the scroll
    /// views and clipping boxes around it and to the screen. `None` if
    /// nothing of it showed (it is scrolled out of view, or not in the
    /// tree). Something drawn over it does not count: this is about
    /// clipping, not covering. Inside a turned or scaled widget, clips
    /// outside that widget are not counted either.
    pub fn visible_rect_of(&self, id: impl Hash) -> Option<Rect> {
        let key = key_hash(id);
        self.keyed
            .iter()
            .find(|keyed| keyed.id == key)
            .map(|keyed| keyed.visible)
            .filter(|visible| !visible.is_empty())
    }

    /// Asks for a scroll as [`Effects::scroll`] does; carried out in the
    /// next frame. For tests and tools.
    pub fn scroll_to(&mut self, to: ScrollTo) {
        self.scroll_requests.push(to);
    }

    /// Tells the layer that the user has asked the system for less motion
    /// (Android's "remove animations", iOS's "reduce motion"). While set,
    /// [`animate`](Container::animate), [`transition`](Container::transition)
    /// and animated [`ScrollTo`]s take no time: every change shows at
    /// once. Flings, which follow the finger, are unaffected.
    ///
    /// The hosts report the setting themselves (`FrameInfo::reduce_motion`,
    /// ADR 0026), and the layer follows it without being told. This is for
    /// tests, and for an application that offers the choice in its own
    /// settings: `true` here reduces motion whatever the system says;
    /// `false` goes back to following the system.
    pub fn set_reduce_motion(&mut self, reduce: bool) {
        self.forced_reduce = reduce;
        self.motions.reduce = reduce;
    }

    /// True if something was still animating in the last frame (an
    /// [`animate`](Container::animate)d change, a transition, or a widget
    /// leaving). For tests.
    pub fn is_animating(&self) -> bool {
        self.motions.moving || !self.ghosts.is_empty()
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

    /// The selected part of the focused input's value, as byte offsets
    /// (start before end); `None` if no input has focus or nothing is
    /// selected.
    pub fn selection(&self) -> Option<(usize, usize)> {
        let focus = self.focus.as_ref()?;
        focus.edit.selection_in(&focus.expected)
    }

    /// Gives the focus to the [`TextInput`](crate::TextInput) with this
    /// explicit `.id(..)` and selects bytes `range` of its value (moved
    /// back to character boundaries; an empty range only places the
    /// caret, at its start). The selection shows its handles, as one made
    /// by touch does. Returns false if no such input was in the last
    /// frame. For tests, and for an application that selects a field's
    /// contents when it opens a form.
    pub fn select(&mut self, id: impl Hash, range: std::ops::Range<usize>) -> bool {
        let key = key_hash(id);
        let Some(index) = self.inputs.iter().position(|record| record.id == key) else {
            return false;
        };
        if self.focus.as_ref().is_none_or(|focus| focus.id != key) {
            self.move_focus(index);
        }
        let value = &self.inputs[index].value;
        if let Some(focus) = self.focus.as_mut() {
            let start = edit::snap(value, range.start.min(range.end));
            let end = edit::snap(value, range.end.max(range.start));
            focus.edit.select(start, end);
            focus.handles = start != end;
            focus.menu = false;
            focus.pending = None;
            focus.blink_from = None;
            focus.history.break_run();
            focus.touched();
        }
        true
    }

    /// True while the contextual menu (Cut, Copy, Paste, Select all) is
    /// shown over the focused input. For tests.
    pub fn selection_menu_shown(&self) -> bool {
        self.focus.as_ref().is_some_and(|focus| focus.menu)
    }

    /// Where the last frame put the two handles of the selection, the
    /// start's first, as the rectangles (px) a touch takes hold of them
    /// in; `None` for a handle that is not shown (nothing selected, the
    /// selection was made by key, or that end is scrolled out of sight).
    /// For tests and tools.
    pub fn selection_handles(&self) -> [Option<Rect>; 2] {
        let find = |end: SelectionEnd| {
            self.regions
                .iter()
                .find(|region| region.kind == RegionKind::Handle(end))
                .map(|region| region.rect)
        };
        [find(SelectionEnd::Start), find(SelectionEnd::End)]
    }

    /// The items of the contextual menu the last frame drew, left to
    /// right: each label and its rectangle (px). Empty while no menu is
    /// up. For tests and tools.
    pub fn selection_menu(&self) -> Vec<(&'static str, Rect)> {
        self.regions
            .iter()
            .filter_map(|region| match region.kind {
                RegionKind::Menu(action) => MENU_LABELS
                    .iter()
                    .find(|(candidate, _)| *candidate == action)
                    .map(|(_, label)| (*label, region.rect)),
                _ => None,
            })
            .collect()
    }

    /// The text on the clipboard of a `Ui` that has no platform behind it
    /// ([`testing`](Self::testing), [`with_services`](Self::with_services),
    /// or no host at all): what a field last copied or cut, or what
    /// [`set_clipboard`](Self::set_clipboard) put there. Always `None`
    /// under a real host, whose clipboard is the system's.
    pub fn clipboard(&self) -> Option<&str> {
        self.effects.clipboard()
    }

    /// Puts text on the stand-in clipboard, as if the user had copied it
    /// in another application; see [`clipboard`](Self::clipboard).
    pub fn set_clipboard(&mut self, text: impl Into<String>) {
        self.effects.set_clipboard(Some(text.into()));
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
                // Part of the selection's own controls: nothing under it
                // scrolls or is pressed.
                RegionKind::Handle(_) | RegionKind::Menu(_) => (Some(region.id), None),
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
            RegionKind::Press(_)
            | RegionKind::Focus(_)
            | RegionKind::Handle(_)
            | RegionKind::Menu(_)
                if region.id == id && region.contains(point) =>
            {
                Some((region.kind, region.field))
            }
            _ => None,
        })
    }

    /// Gives the focus to input `id`. `tap` is where the finger was, for
    /// placing the caret.
    fn focus_input(&mut self, id: u64, pending: Option<Pending>) {
        match self.focus.as_mut() {
            Some(focus) if focus.id == id => {
                focus.pending = pending;
                focus.blink_from = None;
                focus.history.break_run();
                focus.touched();
            }
            _ => self.focus = Some(Focus::new(id, pending)),
        }
    }

    /// Moves the focus to the `index`-th input of the last frame, with the
    /// caret at the end of its value.
    fn move_focus(&mut self, index: usize) {
        let Some(record) = self.inputs.get(index) else {
            return;
        };
        self.focus = Some(Focus {
            edit: EditState::at(record.value.len()),
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
        focus.touched();
        focus.pending = None;

        let (config, value) = (&record.config, record.value.as_str());
        let before = focus.edit;
        let selected = before.selection_in(value).is_some();
        let (changed, kind) = match event {
            TextInputEvent::FocusLost => {
                self.focus = None;
                return;
            }
            TextInputEvent::Action(action) => return self.perform(index, *action),
            TextInputEvent::Enter if !config.multiline => {
                return self.perform(index, record.traits.action);
            }
            TextInputEvent::Move { to, extend } => {
                // A key takes over from the finger: its selection has no
                // handles and no menu.
                focus.dismiss();
                focus.history.break_run();
                if !edit::move_caret(value, &mut focus.edit, *to, *extend) {
                    focus.pending = Some(Pending::Move(*to, *extend));
                }
                return;
            }
            TextInputEvent::SelectAll => {
                focus.edit.select(0, value.len());
                focus.history.break_run();
                return;
            }
            TextInputEvent::Copy => return self.copy_selection(index),
            TextInputEvent::Paste => return self.ask_for_paste(index),
            // A field nothing listens to cannot change.
            _ if record.on_input.is_none() => return,
            TextInputEvent::Cut => {
                // A password is never put on the clipboard.
                if record.traits.secure || !selected {
                    return;
                }
                self.copy_selection(index);
                return self.cut_selection(index);
            }
            TextInputEvent::Undo | TextInputEvent::Redo => {
                focus.dismiss();
                let restored = if matches!(event, TextInputEvent::Undo) {
                    focus.history.undo(value, &mut focus.edit)
                } else {
                    focus.history.redo(value, &mut focus.edit)
                };
                let Some(restored) = restored else {
                    return;
                };
                focus.edit.caret = edit::snap(&restored, focus.edit.caret);
                focus.edit.anchor = focus
                    .edit
                    .anchor
                    .map(|anchor| edit::snap(&restored, anchor));
                focus.expected.clone_from(&restored);
                return self.changed(index, restored);
            }
            TextInputEvent::Enter => (
                edit::insert(value, &mut focus.edit, "\n", config),
                EditKind::Other,
            ),
            TextInputEvent::Commit(text) => (
                edit::insert(value, &mut focus.edit, text, config),
                // A word and the space after it are one step; the next
                // word starts another.
                if selected || text.chars().all(char::is_whitespace) {
                    EditKind::Other
                } else {
                    EditKind::Typing
                },
            ),
            TextInputEvent::Compose(text) => (
                edit::compose(value, &mut focus.edit, text, config),
                EditKind::Typing,
            ),
            TextInputEvent::DeleteForward => (
                edit::delete_forward(value, &mut focus.edit),
                EditKind::Deleting,
            ),
            TextInputEvent::DeleteBackward => {
                if value.is_empty() && record.auto_advance && index > 0 {
                    // An empty code box: go back one box and delete there.
                    let previous = &self.inputs[index - 1];
                    let mut state = EditState::at(previous.value.len());
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
                (
                    edit::delete_backward(value, &mut focus.edit),
                    EditKind::Deleting,
                )
            }
            TextInputEvent::Edited(state) => {
                let kind = if state.text.len() >= value.len() {
                    EditKind::Typing
                } else {
                    EditKind::Deleting
                };
                (edit::adopt(value, &mut focus.edit, state, config), kind)
            }
        };
        // The input method's own selection keeps its handles only if a
        // touch selection was showing them already.
        if focus.edit.selection().is_none() || changed.is_some() {
            focus.dismiss();
        } else {
            focus.menu = false;
        }
        let Some(value) = changed else {
            if focus.edit != before {
                focus.history.break_run();
            }
            return;
        };
        let kind = if selected { EditKind::Other } else { kind };
        focus.history.record(&record.value, &before, kind);
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

    /// Puts the focused input's selected text on the clipboard. A secure
    /// field's is never copied.
    fn copy_selection(&mut self, index: usize) {
        let (Some(focus), Some(record)) = (self.focus.as_mut(), self.inputs.get(index)) else {
            return;
        };
        focus.menu = false;
        if record.traits.secure {
            return;
        }
        let Some((start, end)) = focus.edit.selection_in(&record.value) else {
            return;
        };
        let text = record.value[start..end].to_owned();
        self.effects.request(ServiceRequest::SetClipboard(text));
    }

    /// Removes the focused input's selected text, as one step of its own.
    fn cut_selection(&mut self, index: usize) {
        let (Some(focus), Some(record)) = (self.focus.as_mut(), self.inputs.get(index)) else {
            return;
        };
        let before = focus.edit;
        let Some(value) = edit::delete_selection(&record.value, &mut focus.edit) else {
            return;
        };
        focus.dismiss();
        focus
            .history
            .record(&record.value, &before, EditKind::Other);
        focus.expected.clone_from(&value);
        self.changed(index, value);
    }

    /// Asks the platform for the clipboard, for the focused input. It is
    /// read only now that the user has chosen to paste; the text arrives
    /// as the answer to the request ([`paste_into`](Self::paste_into)).
    fn ask_for_paste(&mut self, index: usize) {
        let (Some(focus), Some(record)) = (self.focus.as_mut(), self.inputs.get(index)) else {
            return;
        };
        focus.menu = false;
        if record.on_input.is_none() {
            return;
        }
        let id = self.effects.request(ServiceRequest::ReadClipboard);
        self.paste = Some((id, record.id));
    }

    /// The clipboard's text, for the input that asked for it, if it still
    /// has the focus: it replaces the selection, minus what the field does
    /// not accept and what does not fit.
    fn paste_into(&mut self, input: u64, text: &str) {
        let Some(focus) = self.focus.as_mut().filter(|focus| focus.id == input) else {
            return;
        };
        let Some(index) = self.inputs.iter().position(|record| record.id == input) else {
            return;
        };
        let record = &self.inputs[index];
        if !focus.known {
            focus.known = true;
            focus.expected.clone_from(&record.value);
        }
        focus.edit.caret = edit::snap(&record.value, focus.edit.caret);
        let before = focus.edit;
        let text = edit::pasted(text, &record.config);
        // What was being composed stays as it is: the paste goes where
        // the caret or the selection is.
        focus.edit.composing = None;
        let changed = edit::insert(&record.value, &mut focus.edit, &text, &record.config);
        focus.dismiss();
        focus.blink_from = None;
        focus.touched();
        let Some(value) = changed else {
            return;
        };
        focus
            .history
            .record(&record.value, &before, EditKind::Other);
        focus.expected.clone_from(&value);
        self.changed(index, value);
    }

    /// An item of the contextual menu was tapped.
    fn menu_action(&mut self, action: MenuAction) {
        let event = match action {
            MenuAction::Cut => TextInputEvent::Cut,
            MenuAction::Copy => TextInputEvent::Copy,
            MenuAction::Paste => TextInputEvent::Paste,
            MenuAction::SelectAll => TextInputEvent::SelectAll,
        };
        let shown = self
            .focus
            .as_ref()
            .is_some_and(|focus| focus.handles || focus.menu);
        self.text_event(&event);
        if action == MenuAction::SelectAll
            && let Some(focus) = self.focus.as_mut()
        {
            // Everything is selected: offer what can be done with it.
            focus.handles = shown && focus.edit.selection().is_some();
            focus.menu = focus.handles;
        }
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
            // A glide the application asked for stops under a finger.
            state.glide = None;
        }
        if press.is_none() && scroll.is_none() {
            // A touch on nothing: whatever was being edited is done.
            self.focus = None;
            return;
        }
        // A finger on a selection handle takes hold of that end.
        let mut handle = None;
        if let Some(id) = press
            && let Some((RegionKind::Handle(end), _)) = self.target_at(id, event.position)
        {
            press = None;
            handle = self.grab_handle(end, event.position);
        }
        self.gesture = Some(Gesture {
            pointer: event.id,
            start: event.position,
            press: press.map(|id| Press { id, inside: true }),
            scroll,
            drag: None,
            handle,
            held_since: None,
            moved: false,
            long_pressed: false,
        });
    }

    /// Starts dragging the selection handle at `end`: that end becomes the
    /// one that moves. Returns how far the finger is from the middle of
    /// the line the handle hangs from, which is kept for the whole drag
    /// so that the selection does not jump to the line under the finger.
    fn grab_handle(&mut self, end: SelectionEnd, position: Point) -> Option<Point> {
        let overlay = self.overlay?;
        let focus = self.focus.as_mut().filter(|f| f.id == overlay.input)?;
        let (start, finish) = focus.edit.selection()?;
        let (caret, anchor, line) = match end {
            SelectionEnd::Start => (start, finish, overlay.start),
            SelectionEnd::End => (finish, start, overlay.end),
        };
        focus.edit.caret = caret;
        focus.edit.anchor = Some(anchor);
        focus.edit.composing = None;
        focus.history.break_run();
        let hold = Point::new(line.x, line.y + line.height / 2.0);
        focus.drag = Some(hold);
        Some(Point::new(position.x - hold.x, position.y - hold.y))
    }

    fn pointer_moved(&mut self, event: &PointerEvent) {
        let Some(mut gesture) = self.gesture.filter(|g| g.pointer == event.id) else {
            return;
        };
        let position = event.position;
        if let Some(offset) = gesture.handle {
            // The end of the selection follows; the next frame works out
            // which character that is.
            if let Some(focus) = self.focus.as_mut() {
                focus.drag = Some(Point::new(position.x - offset.x, position.y - offset.y));
            }
            return;
        }
        let scroll = gesture.scroll.and_then(|id| self.scrolls.get_mut(&id));

        if let (Some(drag), Some(state)) = (gesture.drag, scroll) {
            let travelled = (position.y - drag.anchor_y) / self.scale;
            state.offset = clamp_offset(drag.anchor_offset - travelled, state.max);
            return;
        }

        let dx = position.x - gesture.start.x;
        let dy = position.y - gesture.start.y;
        if dx.hypot(dy) > TOUCH_SLOP * self.scale {
            // Too far to still become a long press.
            gesture.moved = true;
        }
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
            // The text moves away from under its menu.
            if let Some(focus) = self.focus.as_mut() {
                focus.menu = false;
            }
        } else if let Some(press) = gesture.press.as_mut() {
            press.inside = self.target_at(press.id, position).is_some();
        }
        self.gesture = Some(gesture);
    }

    fn pointer_up(&mut self, event: &PointerEvent) {
        let Some(gesture) = self.gesture.take_if(|g| g.pointer == event.id) else {
            return;
        };
        if gesture.handle.is_some() {
            // The handle is let go: offer what can be done with the
            // selection it leaves.
            if let Some(focus) = self.focus.as_mut() {
                focus.drag = None;
                focus.handles = true;
                focus.menu = true;
                focus.touched();
            }
        } else if gesture.drag.is_some() {
            if let Some(state) = gesture.scroll.and_then(|id| self.scrolls.get_mut(&id)) {
                state.flinging = true;
                state.released = true;
            }
        } else if gesture.long_pressed {
            // The long press already did what this touch was for.
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
                Some((RegionKind::Focus(input), _)) => self.tap_input(input, event.position),
                Some((RegionKind::Menu(action), _)) => self.menu_action(action),
                // Slid off before lifting: nothing happens.
                _ => {}
            }
        } else {
            // A tap on the background of a scroll view.
            self.focus = None;
        }
    }

    /// A tap on text input `input`: the first places the caret, a second
    /// soon after and nearby selects the word there, a third everything.
    fn tap_input(&mut self, input: u64, position: Point) {
        // Pointer events carry no time: the clock is the frames', and a
        // frame is drawn after every touch.
        let now = self.last_elapsed.unwrap_or_default();
        let count = match self.last_tap {
            Some(tap)
                if tap.input == input
                    && now.saturating_sub(tap.at) <= MULTI_TAP
                    && (position.x - tap.position.x).hypot(position.y - tap.position.y)
                        <= MULTI_TAP_SLOP * self.scale =>
            {
                tap.count % 3 + 1
            }
            _ => 1,
        };
        self.last_tap = Some(Tap {
            input,
            at: now,
            position,
            count,
        });
        let pending = match count {
            1 => Pending::Caret(position),
            2 => Pending::Word(position),
            _ => Pending::All,
        };
        self.focus_input(input, Some(pending));
        if let Some(focus) = self.focus.as_mut() {
            focus.drag = None;
            // Whether there is anything to show them for is known once
            // the frame has found the word.
            focus.handles = count > 1;
            focus.menu = count > 1;
        }
    }

    /// A finger has rested on a text input for [`LONG_PRESS`]: the word
    /// under it is selected and the menu comes up. Called while a frame is
    /// being built, which is where time passes.
    fn long_press(&mut self, elapsed: Duration, frame: &mut Frame) {
        let Some(mut gesture) = self.gesture else {
            return;
        };
        let Some(press) = gesture.press.filter(|press| press.inside) else {
            return;
        };
        if gesture.moved || gesture.long_pressed || gesture.drag.is_some() {
            return;
        }
        let Some((RegionKind::Focus(input), _)) = self.target_at(press.id, gesture.start) else {
            return;
        };
        let since = *gesture.held_since.get_or_insert(elapsed);
        let held = elapsed.saturating_sub(since);
        if held < LONG_PRESS {
            frame.request_redraw_after(LONG_PRESS - held);
            self.gesture = Some(gesture);
            return;
        }
        gesture.long_pressed = true;
        self.gesture = Some(gesture);
        self.last_tap = None;
        self.focus_input(input, Some(Pending::Word(gesture.start)));
        if let Some(focus) = self.focus.as_mut() {
            focus.drag = None;
            focus.handles = true;
            focus.menu = true;
        }
    }

    fn pointer_cancelled(&mut self, event: &PointerEvent) {
        let Some(gesture) = self.gesture.take_if(|g| g.pointer == event.id) else {
            return;
        };
        if gesture.handle.is_some()
            && let Some(focus) = self.focus.as_mut()
        {
            focus.drag = None;
        }
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
        // A finger resting on a field becomes a long press here, where
        // the time is known.
        self.long_press(info.elapsed, frame);
        // Less motion if the system says so (the host reports it) or the
        // application asked for it itself.
        self.motions.reduce = self.forced_reduce || info.reduce_motion;
        let theme = self.app.theme();
        // Light or dark icons in the system's bars: the application's
        // word, or what reads on the theme's background.
        frame.set_system_bars(
            self.app
                .system_bars()
                .unwrap_or_else(|| SystemBars::for_background(theme.colors.background)),
        );
        let scale = info.scale;
        // The tree of the last frame, if something in it may be leaving.
        let mut previous = self.last_root.take();
        let root = {
            let viewport = Viewport::new(
                info,
                frame,
                theme,
                self.focus.as_ref().map(|focus| focus.id),
                self.motions.reduce,
            );
            self.app.view_in(&viewport)
        };
        self.scale = scale;
        self.generation += 1;
        let (generation, now) = (self.generation, info.elapsed);
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
        self.keyed_table.clear();
        self.scroll_table.clear();
        self.ghost_roots.clear();
        self.motions.begin_frame();
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
            keyed: &mut self.keyed_table,
            scrolls: &mut self.scroll_table,
            motions: &mut self.motions,
            generation,
            now,
            ghost: false,
        };
        let root_node = lowering.lower(&root, None, root.key.unwrap_or(ROOT_PATH), NO_PARENT);

        // Widgets with a transition that are no longer in the tree are
        // taken out of the last frame's tree and drawn from there, on top
        // of the screen and outside its layout, until they are out of
        // sight. With no animated widget anywhere this is one test.
        if !lowering.motions.is_idle() {
            let ghosts = &mut self.ghosts;
            lowering.motions.sweep(generation, now, |id, rect| {
                match take_element(&mut previous, id) {
                    Some(element) => {
                        ghosts.push(Ghost { id, element, rect });
                        true
                    }
                    None => false,
                }
            });
            // One that has gone, or is back in the tree, is a ghost no more.
            ghosts.retain(|ghost| lowering.motions.is_leaving(ghost.id, now));
        }
        drop(previous);
        lowering.ghost = true;
        for ghost in &self.ghosts {
            let (slot, text) = (lowering.records.len(), lowering.texts.len());
            let node = lowering.lower(&ghost.element, None, ghost.id, NO_PARENT);
            self.ghost_roots.push((node, slot as u32, text as u32));
        }
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
                        // What was selected is not what is there now.
                        focus.edit.anchor = None;
                        focus.dismiss();
                        // Nor do the steps back lead to it.
                        focus.history.clear();
                        focus.touched();
                    }
                    focus.edit.caret = edit::snap(&record.value, focus.edit.caret);
                    focus.edit.anchor = focus
                        .edit
                        .anchor
                        .map(|anchor| edit::snap(&record.value, anchor));
                }
            }
        }

        // Lay out in dp, measuring text in pixels through the frame.
        self.text_meta.clear();
        self.text_meta.resize(texts.len(), TextMeta::default());
        let meta = &mut self.text_meta;
        let logical = info.logical_size();
        let mut measure = |key: u64, max_width: Option<f32>| {
            let index = key as usize;
            let (Some(text), Some(meta)) = (texts.get(index), meta.get_mut(index)) else {
                return Size::ZERO;
            };
            measure_text(frame, text, meta, &theme, scale, max_width)
        };
        self.tree.compute(
            root_node,
            Rect::new(0.0, 0.0, logical.width, logical.height),
            &mut measure,
        );
        // A leaving widget keeps the rectangle it had.
        for (ghost, (node, _, _)) in self.ghosts.iter().zip(&self.ghost_roots) {
            let rect = ghost.rect;
            self.tree.compute(
                *node,
                Rect::new(
                    rect.x / scale,
                    rect.y / scale,
                    rect.width / scale,
                    rect.height / scale,
                ),
                &mut measure,
            );
        }
        drop(texts);

        // Keep the field being edited clear of the keyboard and in view.
        let logical_height = logical.height;
        let keyboard_top =
            (info.keyboard_inset > 0.0).then(|| logical_height - info.keyboard_inset / scale);
        // What the application asked to be scrolled, now that this
        // frame's layout says where things are.
        if !self.scroll_requests.is_empty() {
            let reduce = self.motions.reduce;
            for request in self.scroll_requests.drain(..) {
                let glide = request.animated && !reduce;
                match request.target {
                    ScrollTarget::Offset(id, dp) => {
                        if let Some(record) = self.scroll_table.iter().find(|r| r.id == id)
                            && let Some(node) = node_of(&self.records, record.slot)
                        {
                            let max = scroll_max(&self.tree, node, keyboard_top);
                            let state = self.scrolls.entry(id).or_default();
                            state.go(clamp_offset(dp, max), glide);
                        }
                    }
                    ScrollTarget::Reveal(id, margin) => {
                        if let Some(keyed) = self.keyed_table.iter().find(|k| k.id == id)
                            && let Some(node) = node_of(&self.records, keyed.slot)
                            && let Some(scroll) = keyed.scroll
                        {
                            Reveal {
                                tree: &self.tree,
                                records: &self.records,
                                table: &self.scroll_table,
                                scrolls: &mut self.scrolls,
                                keyboard_top,
                            }
                            .run(
                                self.tree.rect(node),
                                scroll,
                                margin,
                                margin,
                                glide,
                            );
                        }
                    }
                }
            }
        }

        // The focused field is kept in view the same way wherever it is:
        // every scroll view around it is scrolled, the innermost first,
        // once per change (the user may scroll it away again); and if the
        // keyboard still covers it after that, or there is no scroll view
        // at all, the whole screen is drawn shifted up by `pan`.
        let mut pan = 0.0;
        if let Some(focus) = self.focus.as_mut()
            && let Some(record) = self.inputs.iter().find(|record| record.id == focus.id)
            && let Some(field) = node_of(&self.records, record.slot)
        {
            let reveal = std::mem::take(&mut focus.reveal);
            let field = self.tree.rect(field);
            let safe_top = info.insets.top / scale;
            match record.scroll {
                Some(scroll) => {
                    if reveal {
                        let shown = Reveal {
                            tree: &self.tree,
                            records: &self.records,
                            table: &self.scroll_table,
                            scrolls: &mut self.scrolls,
                            keyboard_top,
                        }
                        .run(
                            field,
                            scroll,
                            REVEAL_ABOVE,
                            REVEAL_BELOW,
                            false,
                        );
                        focus.pan = keyboard_top.map_or(0.0, |top| pan_for(shown, top, safe_top));
                    }
                    if keyboard_top.is_some() {
                        pan = focus.pan;
                    }
                }
                None => {
                    if let Some(top) = keyboard_top {
                        pan = pan_for(field, top, safe_top);
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
            keyed_table: &self.keyed_table,
            motions: &mut self.motions,
            shown_cursor: 0,
            inert: false,
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
            overlay: None,
            rects: Vec::new(),
            safe_top: info.insets.top,
            surface,
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
                alpha: 1.0,
            },
        );
        // What is leaving, over the screen, where it last was; it reacts
        // to nothing.
        painter.inert = true;
        for (ghost, &(_, slot, text)) in self.ghosts.iter().zip(&self.ghost_roots) {
            painter.node_cursor = slot as usize;
            painter.text_cursor = text as usize;
            painter.paint(
                &ghost.element,
                Context {
                    id: ghost.id,
                    dx: 0.0,
                    dy: 0.0,
                    clip: surface,
                    cull: None,
                    scroll: None,
                    field: None,
                    space: None,
                    alpha: 1.0,
                },
            );
        }
        // Over everything: the handles of the focused field's selection
        // and its menu.
        painter.inert = false;
        painter.paint_selection_controls();
        self.overlay = painter.overlay;
        let animating = painter.animating;
        self.motions.end_frame(generation);
        // Frames keep coming only while something is on its way.
        if animating || self.motions.moving {
            frame.request_redraw();
        }
        self.last_root = self.motions.transitions.then_some(root);
        self.last_elapsed = Some(info.elapsed);

        // Ask for the keyboard for as long as a field has focus.
        if let Some(focus) = self.focus.as_ref()
            && let Some(record) = self.inputs.iter().find(|record| record.id == focus.id)
        {
            let caret = edit::snap(&record.value, focus.edit.caret);
            frame.set_text_input(TextInputRequest {
                session: focus.id,
                traits: record.traits,
                text: record.value.clone(),
                caret,
                // The input method is told what is selected, so that its
                // suggestions and its own selection gestures agree.
                anchor: focus
                    .edit
                    .anchor
                    .map(|anchor| edit::snap(&record.value, anchor))
                    .filter(|anchor| *anchor != caret),
            });
        }

        // Widget state lives as long as the widget is on screen.
        let generation = self.generation;
        self.scrolls.retain(|_, state| state.seen == generation);
        if let Some(gesture) = self.gesture.as_mut() {
            if let Some(press) = gesture.press
                && !self.regions.iter().any(|region| {
                    region.id == press.id
                        && matches!(
                            region.kind,
                            RegionKind::Press(_) | RegionKind::Focus(_) | RegionKind::Menu(_)
                        )
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
            if gesture.handle.is_some() && self.focus.is_none() {
                gesture.handle = None;
            }
            if gesture.press.is_none() && gesture.scroll.is_none() && gesture.handle.is_none() {
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

/// The offset in `value` nearest a touch at `point` of its laid-out text
/// (`shown`: the value, or its bullets when `secure`).
///
/// The text engine answers with an edge of the glyph under the point, and
/// a glyph can be more than one character: an "fi" ligature is one glyph,
/// and a caret belongs between its letters too. So the characters either
/// side of the engine's answer are tried as well, and the one whose caret
/// is drawn nearest the finger, on the same line, wins.
fn index_near(
    frame: &mut Frame,
    shown: &str,
    style: &TextStyle,
    constraint: Option<f32>,
    (value, secure): (&str, bool),
    point: Point,
) -> usize {
    let found = frame.text_index_at(shown, style, constraint, point);
    let found = edit::value_index(value, found, secure);
    let mut caret_of = |index: usize| {
        frame.text_caret(
            shown,
            style,
            constraint,
            edit::display_index(value, index, secure),
        )
    };
    let at = caret_of(found);
    let mut best = (found, (at.x - point.x).abs());
    for candidate in [edit::previous(value, found), edit::next(value, found)] {
        let caret = caret_of(candidate);
        let distance = (caret.x - point.x).abs();
        if candidate != found && caret.y == at.y && distance < best.1 {
            best = (candidate, distance);
        }
    }
    best.0
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
/// infinity. The field, with a margin (`above` and `below` it), is kept
/// above the lower of the
/// viewport's bottom and the keyboard, and below the viewport's top; when
/// it cannot be both, its top wins. The result is within `0..=max`, and is
/// `offset` unchanged when the field is already in view.
fn reveal_offset(
    field: Rect,
    viewport: Rect,
    visible_bottom: f32,
    offset: f32,
    max: f32,
    above: f32,
    below: f32,
) -> f32 {
    let top = viewport.y;
    let bottom = (viewport.y + viewport.height).min(visible_bottom);
    let mut target = offset;
    let field_bottom = field.y + field.height + below - target;
    if field_bottom > bottom {
        target += field_bottom - bottom;
    }
    let field_top = field.y - above - target;
    if field_top < top {
        target -= top - field_top;
    }
    clamp_offset(target, max)
}

/// Brings a rectangle into view by scrolling the scroll views around it.
struct Reveal<'a> {
    tree: &'a LayoutTree,
    records: &'a [NodeRecord],
    table: &'a [ScrollRecord],
    scrolls: &'a mut HashMap<u64, ScrollState>,
    keyboard_top: Option<f32>,
}

impl Reveal<'_> {
    /// Scrolls every scroll view from `innermost` outwards as little as
    /// brings `rect` (a layout rectangle, dp) into its viewport with
    /// `above` and `below` to spare, and returns where `rect` then is on
    /// the screen. The keyboard, while it is up, lowers what counts as
    /// visible in the outermost scroll view, the one on the screen itself.
    fn run(self, mut rect: Rect, innermost: u32, above: f32, below: f32, glide: bool) -> Rect {
        let mut next = Some(innermost);
        while let Some(record) = next.and_then(|index| self.table.get(index as usize)) {
            next = record.parent;
            let Some(node) = node_of(self.records, record.slot) else {
                continue;
            };
            let bottom = match (next, self.keyboard_top) {
                (None, Some(top)) => top,
                _ => f32::INFINITY,
            };
            let state = self.scrolls.entry(record.id).or_default();
            let offset = reveal_offset(
                rect,
                self.tree.rect(node),
                bottom,
                state.offset,
                scroll_max(self.tree, node, self.keyboard_top),
                above,
                below,
            );
            if offset != state.offset {
                state.go(offset, glide);
            }
            rect.y -= offset;
        }
        rect
    }
}

/// Takes the element with identity `id` out of the tree at `root`,
/// leaving an empty box where it was. Identities are worked out as
/// lowering does.
fn take_element<M>(root: &mut Option<Element<M>>, id: u64) -> Option<Element<M>> {
    fn within<M>(element: &mut Element<M>, own: u64, id: u64) -> Option<Element<M>> {
        let Kind::Box(node) = &mut element.kind else {
            return None;
        };
        for (index, child) in node.children.iter_mut().enumerate() {
            let child_id = child.key.unwrap_or_else(|| child_path(own, index));
            if child_id == id {
                return Some(std::mem::replace(child, Container::empty().into()));
            }
            if let Some(found) = within(child, child_id, id) {
                return Some(found);
            }
        }
        None
    }
    let own = root.as_ref()?.key.unwrap_or(ROOT_PATH);
    if own == id {
        return root.take();
    }
    within(root.as_mut()?, own, id)
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
        frame.measure_text_spans(&text_spans(text, theme, 1.0), style, constraint)
    }
}

/// The spans of a rich text, with theme colours resolved.
fn text_spans<'a>(text: &'a TextNode, theme: &Theme, alpha: f32) -> Vec<TextSpan<'a>> {
    text.spans
        .iter()
        .map(|span| TextSpan {
            text: &span.content,
            weight: span.weight,
            color: span.color.map(|color| fade(color.resolve(theme), alpha)),
            family: span.family,
            italic: span.italic.then_some(true),
        })
        .collect()
}

/// The layout engine's measure callback for one text. Constraints arrive
/// and sizes leave in dp; the text engine works in pixels.
pub(crate) fn measure_text(
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

    let line = style.size * style.line_height.max(0.0);
    meta.line_height = line;
    let mut height = measured.height;
    if let Some(lines) = text.max_lines {
        height = height.min(line * lines as f32);
    }
    // The width the glyphs advance over, fractions included, as a browser
    // measures: rounding it up moved every later sibling (ADR 0022).
    let mut width = measured.width / scale;
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
    /// Scroll views being lowered, in `scrolls`.
    open_scrolls: Vec<u32>,
    /// Every element with an explicit id, and every scroll view.
    keyed: &'a mut Vec<KeyedRecord>,
    scrolls: &'a mut Vec<ScrollRecord>,
    motions: &'a mut Motions,
    generation: u64,
    /// The frame's time.
    now: Duration,
    /// Lowering a widget that has left the tree: it is drawn, and takes
    /// part in nothing else.
    ghost: bool,
}

impl<'t, M: Clone> Lowering<'_, 't, M> {
    /// Adds `element` and its subtree; `parent` is the axis of the box it
    /// sits in, `None` for the root; `id` is the element's identity, the
    /// same one painting gives it, and `parent_id` that of the box it is
    /// in.
    fn lower(
        &mut self,
        element: &'t Element<M>,
        parent: Option<Axis>,
        id: u64,
        parent_id: u64,
    ) -> NodeId {
        let slot = self.records.len();
        self.records.push(NodeRecord {
            node: None,
            end_node: 0,
            end_text: 0,
        });

        let mut style = element.layout;
        if element.key.is_some() && !self.ghost {
            self.keyed.push(KeyedRecord {
                id,
                slot: slot as u32,
                scroll: self.open_scrolls.last().copied(),
            });
        }
        if !element.motion.is_stateless() {
            // What the view asks for, and what is shown on the way there.
            // A size or offset on its way takes part in this layout.
            let fixed = |sizing: Sizing| match sizing {
                Sizing::Fixed(length) if length.is_finite() => Some(length),
                _ => None,
            };
            let finite = |value: f32| if value.is_finite() { value } else { 0.0 };
            let target = Props {
                opacity: element.motion.opacity,
                transform: element
                    .transform
                    .filter(Transform::is_finite)
                    .unwrap_or(Transform::IDENTITY),
                offset: Point::new(finite(style.offset.x), finite(style.offset.y)),
                fill: match &element.kind {
                    Kind::Box(node) => match node.fill {
                        Some(Fill::Solid(color)) => Some(color.resolve(self.theme)),
                        _ => None,
                    },
                    _ => None,
                },
                width: fixed(style.width),
                height: fixed(style.height),
            };
            let shown = self.motions.resolve(
                id,
                parent_id,
                slot as u32,
                &element.motion,
                target,
                self.generation,
                self.now,
                self.ghost,
            );
            if let Some(width) = shown.width {
                style.width = Sizing::Fixed(width);
            }
            if let Some(height) = shown.height {
                style.height = Sizing::Fixed(height);
            }
            style.offset = shown.offset;
        }
        if parent.is_none() {
            // The root always covers the surface.
            style.width = Sizing::FILL;
            style.height = Sizing::FILL;
            if self.ghost {
                // A leaving widget is laid out in the rectangle it had,
                // which its offset had already moved.
                style.offset = Point::new(0.0, 0.0);
            }
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
                if !self.ghost {
                    self.record_input(input, id, slot as u32);
                }
                self.tree.measured(style, key)
            }
            Kind::Box(node) => {
                let is_field = node.field && !self.ghost;
                let is_scroll = node.scroll && !self.ghost;
                if is_field {
                    self.open_fields.push((slot as u32, false));
                }
                if is_scroll {
                    self.open_scrolls.push(self.scrolls.len() as u32);
                    self.scrolls.push(ScrollRecord {
                        id,
                        slot: slot as u32,
                        parent: self.open_scrolls.iter().rev().nth(1).copied(),
                    });
                }
                let start = self.scratch.len();
                for (index, child) in node.children.iter().enumerate() {
                    let child_id = child.key.unwrap_or_else(|| child_path(id, index));
                    let child = self.lower(child, Some(node.axis), child_id, id);
                    self.scratch.push(child);
                }
                if is_field {
                    self.open_fields.pop();
                }
                if is_scroll {
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
    /// The product of the opacities of the element and the boxes around
    /// it, multiplied into every colour drawn.
    alpha: f32,
}

/// True if `element` paints anything.
fn paints<M>(element: &Element<M>) -> bool {
    match &element.kind {
        Kind::Box(node) => {
            node.fill.is_some()
                || node.border.is_some()
                || node.shadow.is_some()
                || node.children.iter().any(paints)
        }
        _ => true,
    }
}

/// True if `element` paints things that lie over one another, so that
/// fading each colour would show the lower through the upper: a box with a
/// background, border or shadow and something painted inside it; a box
/// over its own shadow; a stack of more than one painted child; or any
/// such box further down. A text, a picture, an icon, a box that is one
/// primitive, and rows and columns of those, do not: for them multiplying
/// the opacity into each colour is the same picture and costs nothing.
fn overlaps_itself<M>(element: &Element<M>) -> bool {
    let Kind::Box(node) = &element.kind else {
        return false;
    };
    let surface = node.fill.is_some() || node.border.is_some();
    let painted_children = || node.children.iter().filter(|child| paints(child)).count();
    (surface && node.shadow.is_some())
        || ((surface || node.shadow.is_some()) && painted_children() > 0)
        || (node.axis == Axis::Stack && painted_children() > 1)
        || node.children.iter().any(overlaps_itself)
}

/// `color` at `alpha` times its own opacity.
fn fade(color: Color, alpha: f32) -> Color {
    if alpha >= 1.0 {
        color
    } else {
        Color::rgba(color.r, color.g, color.b, color.a * alpha.max(0.0))
    }
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
    keyed: &'a mut Vec<KeyedRect>,
    scrolls: &'a mut HashMap<u64, ScrollState>,
    scroll_nodes: &'a mut Vec<NodeId>,
    /// The elements with explicit ids, in tree order.
    keyed_table: &'a [KeyedRecord],
    motions: &'a mut Motions,
    /// Where painting has got to in `motions.shown`.
    shown_cursor: usize,
    /// Painting a widget that has left the tree: no touch regions, no
    /// focus, and its scroll views stay where they were.
    inert: bool,
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
    /// Where the focused field's selection was drawn, for its handles and
    /// menu, which are painted last.
    overlay: Option<SelectionOverlay>,
    /// Reused for the rectangles of the selection.
    rects: Vec<Rect>,
    /// The top safe-area inset (px): the menu stays below it.
    safe_top: f32,
    /// The whole surface (px).
    surface: Rect,
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

        // What the element is shown as, if it is animated: its own
        // properties on their way, and how far it has entered or left.
        let mut own = element.transform;
        let mut fill = None;
        let mut presence = None;
        // The opacity of a layer the element is drawn through (ADR 0028).
        let mut layer = None;
        if element.motion.opacity < 1.0 || !element.motion.is_stateless() {
            let mut opacity = element.motion.opacity;
            if !element.motion.is_stateless()
                && let Some(index) = self.motions.index_at(slot, &mut self.shown_cursor)
            {
                // Where it is without its own or the transition's
                // movement: where it is drawn from if it leaves.
                let at = self.pixels(self.tree.rect(node), &context);
                let shown = &mut self.motions.shown[index];
                shown.rect = at;
                opacity = shown.props.opacity * shown.presence.opacity;
                own = Some(shown.props.transform);
                fill = shown.props.fill;
                presence = Some((shown.presence, shown.rect));
            }
            if opacity < 1.0 && overlaps_itself(element) {
                // Parts of it lie over one another: faded colour by
                // colour, the lower would show through the upper. Drawn
                // as a group and faded as one instead.
                layer = Some(opacity);
            } else {
                context.alpha *= opacity;
            }
        }

        // The element's transform, with its lengths in pixels. One that
        // only moves is the same as a scroll translation: whole pixels,
        // no display-list transform, and clipping, culling and hit-testing
        // carry on as they are.
        let mut transform = own.map(|transform| transform.with_translation_scaled(self.scale));
        if let Some((hidden, at)) = presence
            && (hidden.scale != 1.0 || hidden.shift != (0.0, 0.0))
        {
            let entering = Transform::scale(hidden.scale, hidden.scale).then(Transform::translate(
                hidden.shift.0 * at.width,
                hidden.shift.1 * at.height,
            ));
            transform = Some(transform.unwrap_or(Transform::IDENTITY).then(entering));
        }
        let mut transform = transform.filter(|transform| !transform.is_identity());
        if let Some(moved) = transform.filter(Transform::is_translation) {
            if moved.is_finite() {
                context.dx += moved.tx.round();
                context.dy += moved.ty.round();
            }
            transform = None;
        }

        let rect = self.pixels(self.tree.rect(node), &context);
        if element.key.is_some() && !self.inert {
            self.keyed.push(KeyedRect {
                id: context.id,
                rect,
                visible: intersect(rect, context.clip),
            });
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

        if let Some(opacity) = layer {
            self.frame.list.push_layer(opacity);
        }
        match &element.kind {
            Kind::Text(text) => {
                let index = self.text_cursor;
                self.text_cursor += 1;
                // Text is placed where layout put it, not at its box's
                // snapped corner: the text engine keeps the fraction
                // across and rounds each line's top itself (ADR 0022).
                let exact = self.tree.rect(node);
                let exact = Point::new(
                    exact.x * self.scale + context.dx,
                    exact.y * self.scale + context.dy,
                );
                self.paint_text(text, index, rect, exact, &context);
            }
            Kind::Image(image) => self.paint_image(image, rect, &context),
            Kind::Icon(icon) => self.paint_icon(icon, rect, &context),
            Kind::Input(input) => {
                let index = self.text_cursor;
                self.text_cursor += 1;
                self.paint_input(input, index, rect, &context);
            }
            Kind::Box(node_data) => self.paint_box(node_data, node, slot, rect, fill, context),
        }
        if layer.is_some() {
            self.frame.list.pop_layer();
        }

        if transform.is_some() {
            self.frame.list.pop_transform();
        }
    }

    /// A text input: exactly its text while it is not being edited; with
    /// focus, also the caret, the composing underline and, for a single
    /// line wider than the field, a sideways scroll that keeps the caret
    /// in view. A multi-line field whose text is taller than its box
    /// (`max_lines`, `max_height`) scrolls inside it: it is a scroll view
    /// of its own text, with the same state, drag and fling.
    fn paint_input(&mut self, input: &InputNode<M>, index: usize, rect: Rect, context: &Context) {
        let Some(meta) = self.text_meta.get(index).copied() else {
            return;
        };
        // How far (px) the text can scroll up inside the field.
        let range = meta.measured.height - rect.height;
        let tall = range > 0.5 && !input.text.no_wrap;
        let mut scroll_y = 0.0;
        if tall {
            let offset = if self.inert {
                self.rest_scroll(context.id)
            } else {
                self.advance_scroll(context.id, range / self.scale)
            };
            scroll_y = (offset * self.scale).round();
        }
        let visible = intersect(rect, context.clip);
        if !visible.is_empty() && !self.inert {
            self.regions.push(Region {
                id: context.id,
                rect: visible,
                kind: RegionKind::Focus(context.id),
                // A drag on a field that scrolls inside itself scrolls it.
                scroll: if tall {
                    Some(context.id)
                } else {
                    context.scroll
                },
                field: Some(context.id),
                space: context.space,
            });
        }
        // Only a single line can be wider than its box.
        let overflow = meta.measured.width > rect.width + 0.5;
        let focused = if self.inert {
            None
        } else {
            self.focus.take_if(|focus| focus.id == context.id)
        };
        let Some(mut focus) = focused else {
            if overflow || tall {
                self.frame.list.push_clip(rect);
            }
            // The whole text, moved up by the scroll; the clip cuts it.
            let all = if tall {
                Rect::new(rect.x, rect.y - scroll_y, rect.width, meta.measured.height)
            } else {
                rect
            };
            self.paint_text(&input.text, index, all, Point::new(all.x, all.y), context);
            if overflow || tall {
                self.frame.list.pop_clip();
            }
            return;
        };

        let mut style = text_style(&input.text, self.theme, self.scale);
        style.color = fade(style.color, context.alpha);
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
        let scale = self.scale;
        let value = input.value.as_str();
        let secure = input.secure;
        let space = context.space;
        // In a transformed widget the finger is where the turned text is
        // drawn; bring it back to the space the text is laid out in.
        let unturned = |point: Point| space.map_or(point, |space| space.inverse.apply(point));
        let place = |value_index: usize| edit::display_index(value, value_index, secure);
        let furthest_x = if overflow && !input.empty {
            (meta.measured.width - rect.width).max(0.0)
        } else {
            0.0
        };

        // A selection handle held near the edge of what can scroll, or
        // past it, scrolls the text under the finger: the field itself
        // first, then the scroll view around it.
        let dragging = focus.drag.map(unturned);
        if let Some(point) = dragging {
            let dt = self.dt.unwrap_or(1.0 / 60.0).min(0.05);
            // Pixels per second for a finger at `at` against what is
            // visible between `low` and `high`; negative towards `low`.
            let pull = |at: f32, low: f32, high: f32| {
                let zone = (AUTO_SCROLL_ZONE * scale).min((high - low) / 4.0).max(1.0);
                let depth = if at > high - zone {
                    at - (high - zone)
                } else if at < low + zone {
                    at - (low + zone)
                } else {
                    return 0.0;
                };
                let speed = (AUTO_SCROLL_MIN + depth.abs() / zone * 300.0).min(AUTO_SCROLL_MAX);
                speed * scale * depth.signum()
            };
            // True if there is further to go the way the finger pulls.
            let room = |speed: f32, offset: f32, max: f32| {
                (speed > 0.0 && offset < max) || (speed < 0.0 && offset > 0.0)
            };
            let mut scrolled = false;
            if tall && let Some(state) = self.scrolls.get_mut(&context.id) {
                let speed = pull(point.y, rect.y, rect.y + rect.height);
                let max = range / scale;
                if room(speed, state.offset, max) {
                    let to = (state.offset + speed * dt / scale).clamp(0.0, max);
                    state.go(to, false);
                    state.frame_offset = state.offset;
                    scroll_y = (to * scale).round();
                    scrolled = true;
                }
            }
            if furthest_x > 0.0 {
                let speed = pull(point.x, rect.x, rect.x + rect.width);
                if room(speed, focus.scroll_x, furthest_x) {
                    focus.scroll_x = (focus.scroll_x + speed * dt).clamp(0.0, furthest_x);
                    scrolled = true;
                }
            }
            if !scrolled
                && space.is_none()
                && let Some(state) = context.scroll.and_then(|id| self.scrolls.get_mut(&id))
            {
                let bottom = self
                    .keyboard_top
                    .map_or(f32::INFINITY, |top| top * scale)
                    .min(context.clip.y + context.clip.height);
                let speed = pull(point.y, context.clip.y, bottom);
                if room(speed, state.offset, state.max) {
                    let to = clamp_offset(state.offset + speed * dt / scale, state.max);
                    state.go(to, false);
                    scrolled = true;
                }
            }
            // Frames keep coming while the text is moving.
            self.animating |= scrolled;
        }
        let scroll_x = focus.scroll_x.round();
        // A point of the screen as a point of the laid-out text.
        let in_text = |point: Point| {
            let point = unturned(point);
            Point::new(point.x - origin_x + scroll_x, point.y - origin_y + scroll_y)
        };

        if let Some(point) = focus.drag {
            // The end being dragged goes to the character under the
            // finger; the two ends never meet.
            let at = index_near(
                self.frame,
                shown,
                &style,
                meta.constraint,
                (value, secure),
                in_text(point),
            );
            let anchor = focus.edit.anchor.unwrap_or(focus.edit.caret);
            if at != anchor {
                focus.edit.caret = at;
                focus.edit.anchor = Some(anchor);
            }
        }
        match focus.pending.take() {
            Some(Pending::Caret(tap)) => {
                let at = index_near(
                    self.frame,
                    shown,
                    &style,
                    meta.constraint,
                    (value, secure),
                    in_text(tap),
                );
                // Moving the caret ends a composition where it stands.
                focus.edit = EditState::at(at);
            }
            Some(Pending::Word(point)) => {
                let at = self
                    .frame
                    .text_index_at(shown, &style, meta.constraint, in_text(point));
                let at = edit::value_index(value, at, secure);
                // Bullets have no words: a password is selected whole.
                let (start, end) = if secure {
                    (0, value.len())
                } else {
                    edit::word_at(value, at)
                };
                if start == end {
                    focus.edit = EditState::at(at);
                } else {
                    focus.edit.select(start, end);
                }
            }
            Some(Pending::All) => focus.edit.select(0, value.len()),
            Some(Pending::Move(to, extend)) => {
                let from = edit::snap(value, focus.edit.caret);
                let at = self
                    .frame
                    .text_caret(shown, &style, meta.constraint, place(from));
                let middle = at.y + at.height / 2.0;
                let mut index_at = |x: f32, y: f32| {
                    let index =
                        self.frame
                            .text_index_at(shown, &style, meta.constraint, Point::new(x, y));
                    edit::value_index(value, index, secure)
                };
                let target = match to {
                    CaretMove::Up if at.y < at.height / 2.0 => 0,
                    CaretMove::Up => index_at(at.x, middle - at.height),
                    CaretMove::Down if middle + at.height > meta.measured.height => value.len(),
                    CaretMove::Down => index_at(at.x, middle + at.height),
                    CaretMove::LineStart | CaretMove::LineEnd => {
                        // The two edges of the line, whichever way it
                        // reads.
                        let (left, right) = (index_at(-1.0e6, middle), index_at(1.0e6, middle));
                        if to == CaretMove::LineStart {
                            left.min(right)
                        } else {
                            let end = left.max(right);
                            // Where a line wraps, its end is also the
                            // start of the next, and that is where a caret
                            // there is drawn: stay on this line.
                            let drawn =
                                self.frame
                                    .text_caret(shown, &style, meta.constraint, place(end));
                            if drawn.y > at.y + 0.5 {
                                edit::previous(value, end)
                            } else {
                                end
                            }
                        }
                    }
                    _ => from,
                };
                edit::place_caret(&mut focus.edit, from, target, extend);
                focus.caret_moved = true;
            }
            None => {}
        }
        let selection = focus.edit.selection_in(value).filter(|_| !input.empty);
        if selection.is_none() {
            // Nothing to hold on to.
            focus.handles = false;
        }

        let caret = self
            .frame
            .text_caret(shown, &style, meta.constraint, place(focus.edit.caret));
        if dragging.is_none() {
            if furthest_x > 0.0 {
                // The caret may sit on the right edge itself: the clip below
                // is one caret wider than the box.
                if caret.x - focus.scroll_x > rect.width {
                    focus.scroll_x = caret.x - rect.width;
                }
                if caret.x - focus.scroll_x < 0.0 {
                    focus.scroll_x = caret.x;
                }
                focus.scroll_x = focus.scroll_x.clamp(0.0, furthest_x).round();
            } else {
                focus.scroll_x = 0.0;
            }
        }
        let scroll_x = focus.scroll_x.round();
        if tall && dragging.is_none() && std::mem::take(&mut focus.caret_moved) {
            // Bring the caret's line into the field, with a line of
            // context above and below it where the field has the room:
            // a tap on the first or last visible line then brings the
            // next one in.
            let context_lines = caret
                .height
                .min((rect.height - caret.height) / 2.0)
                .max(0.0);
            let mut wanted = scroll_y;
            if caret.y + caret.height + context_lines > wanted + rect.height {
                wanted = caret.y + caret.height + context_lines - rect.height;
            }
            if caret.y - context_lines < wanted {
                wanted = caret.y - context_lines;
            }
            let wanted = wanted.clamp(0.0, range).round();
            if wanted != scroll_y {
                scroll_y = wanted;
                if let Some(state) = self.scrolls.get_mut(&context.id) {
                    state.go(wanted / self.scale, false);
                    state.frame_offset = state.offset;
                }
            }
        }
        focus.caret_moved = false;

        let clip = overflow || meta.measured.height > rect.height + 0.5;
        if clip {
            self.frame.list.push_clip(Rect::new(
                rect.x,
                rect.y,
                rect.width + caret_width,
                rect.height,
            ));
        }
        let origin = Point::new(origin_x - scroll_x, origin_y - scroll_y);

        // The selection, behind the text: one rectangle per line, and per
        // run of a line that mixes directions.
        if let Some((start, end)) = selection {
            let color = fade(
                with_alpha(self.theme.colors.primary, SELECTION_ALPHA),
                context.alpha,
            );
            let mut rects = std::mem::take(&mut self.rects);
            rects.clear();
            self.frame.text_selection_rects(
                shown,
                &style,
                meta.constraint,
                place(start),
                place(end),
                &mut rects,
            );
            for line in &rects {
                // Edges on whole pixels, so that lines meet without a seam.
                let (left, top) = ((origin.x + line.x).round(), (origin.y + line.y).round());
                let right = (origin.x + line.x + line.width).round();
                let bottom = (origin.y + line.y + line.height).round();
                self.frame.list.fill_rect(
                    Rect::new(left, top, (right - left).max(1.0), bottom - top),
                    color,
                );
            }
            self.rects = rects;
        }

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

        if selection.is_none() {
            // The caret: solid for half a second, gone for half a second,
            // by asking for one frame at each change and none in between.
            // A selection has no caret, and asks for no frames.
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
                    fade(input.caret_color.resolve(self.theme), context.alpha),
                );
            }
            let next = CARET_BLINK
                .saturating_mul(phase.saturating_add(1))
                .saturating_sub(since)
                .max(Duration::from_millis(1));
            self.frame.request_redraw_after(next);
        }

        if clip {
            self.frame.list.pop_clip();
        }

        // Where the handles and the menu go, once everything else has
        // been painted. Not inside a turned or scaled widget: they are
        // drawn upright on the screen.
        if space.is_none() && (focus.handles || focus.menu) {
            let (start, end) = selection.unwrap_or((focus.edit.caret, focus.edit.caret));
            let mut line = |index: usize| {
                let at = self
                    .frame
                    .text_caret(shown, &style, meta.constraint, place(index));
                Rect::new(
                    (origin.x + at.x).round(),
                    (origin.y + at.y).round(),
                    0.0,
                    at.height.round(),
                )
            };
            let (start_line, mut end_line) = (line(start), line(end));
            if selection.is_some() && end_line.y > start_line.y {
                // A selection that ends where a line wraps ends on that
                // line, not at the start of the next.
                let before = line(edit::previous(value, end));
                let mut ends = Vec::new();
                self.frame.text_selection_rects(
                    shown,
                    &style,
                    meta.constraint,
                    place(edit::previous(value, end)),
                    place(end),
                    &mut ends,
                );
                if let Some(last) = ends.last()
                    && before.y < end_line.y
                {
                    let rtl = edit::reads_right_to_left(value);
                    let x = if rtl { last.x } else { last.x + last.width };
                    end_line = Rect::new((origin.x + x).round(), before.y, 0.0, before.height);
                }
            }
            let editable = input.on_input.is_some();
            self.overlay = Some(SelectionOverlay {
                input: context.id,
                start: start_line,
                end: end_line,
                visible,
                selected: selection.is_some(),
                rtl: edit::reads_right_to_left(value),
                handles: focus.handles,
                menu: focus.menu && focus.drag.is_none(),
                can_cut: selection.is_some() && !secure && editable,
                can_copy: selection.is_some() && !secure,
                can_paste: editable,
                can_select_all: !value.is_empty() && selection != Some((0, value.len())),
            });
        }
        *self.focus = Some(focus);
    }

    // (see `index_near` below for how a touch becomes a character.)

    /// The two handles of the focused field's selection and the menu over
    /// it, painted after everything else and clipped by nothing, with the
    /// touch regions that make them work.
    fn paint_selection_controls(&mut self) {
        let Some(overlay) = self.overlay else {
            return;
        };
        let scale = self.scale;
        let colors = self.theme.colors;
        let size = (HANDLE_SIZE * scale).round();
        // Only an end that can be seen has a handle.
        let seen = |line: Rect| {
            let visible = overlay.visible;
            let foot = line.y + line.height;
            line.x >= visible.x - 1.0
                && line.x <= visible.x + visible.width + 2.0 * scale
                && foot > visible.y
                && foot <= visible.y + visible.height + 1.0
        };
        if overlay.handles && overlay.selected {
            let ends = [
                (SelectionEnd::Start, overlay.start, !overlay.rtl),
                (SelectionEnd::End, overlay.end, overlay.rtl),
            ];
            for (index, (end, line, leans_left)) in ends.into_iter().enumerate() {
                if !seen(line) {
                    continue;
                }
                // A drop hanging from the foot of the line: a disc with
                // one square corner, the corner on the text.
                let half = (size / 2.0).round();
                let (x, y) = (line.x, line.y + line.height);
                let (disc, corner) = if leans_left {
                    (
                        Rect::new(x - size, y, size, size),
                        Rect::new(x - half, y, half, half),
                    )
                } else {
                    (Rect::new(x, y, size, size), Rect::new(x, y, half, half))
                };
                self.frame
                    .list
                    .fill_rounded_rect(disc, size / 2.0, colors.primary);
                self.frame.list.fill_rect(corner, colors.primary);
                let reach = HANDLE_REACH * scale;
                self.regions.push(Region {
                    id: overlay.input ^ (0x5E1E_C7ED_0000_0001 + index as u64),
                    rect: Rect::new(
                        disc.x - reach,
                        disc.y - reach / 2.0,
                        disc.width + 2.0 * reach,
                        disc.height + 1.5 * reach,
                    ),
                    space: None,
                    kind: RegionKind::Handle(end),
                    scroll: None,
                    field: Some(overlay.input),
                });
            }
        }
        if !overlay.menu {
            return;
        }

        let style = TextStyle {
            weight: TextWeight::MEDIUM,
            line_height: 1.3,
            family: self.theme.fonts.body,
            ..TextStyle::new(MENU_TEXT * scale, colors.text)
        };
        let mut items = [(MenuAction::Copy, "", Size::ZERO); 4];
        let mut count = 0;
        let padding = (MENU_PADDING * scale).round();
        let mut width = 0.0;
        for (action, label) in MENU_LABELS {
            let offered = match action {
                MenuAction::Cut => overlay.can_cut,
                MenuAction::Copy => overlay.can_copy,
                MenuAction::Paste => overlay.can_paste,
                MenuAction::SelectAll => overlay.can_select_all,
            };
            if offered {
                let measured = self.frame.measure_text(label, &style, None);
                width += measured.width.ceil() + 2.0 * padding;
                items[count] = (action, label, measured);
                count += 1;
            }
        }
        if count == 0 {
            return;
        }
        let height = (MENU_HEIGHT * scale).round();
        let (gap, margin) = (MENU_GAP * scale, MENU_MARGIN * scale);
        let visible = overlay.visible;
        // Above the selection; below it (and its handles) when there is
        // no room above.
        let top = overlay.start.y.min(overlay.end.y).max(visible.y);
        let bottom = (overlay.start.y + overlay.start.height)
            .max(overlay.end.y + overlay.end.height)
            .min(visible.y + visible.height);
        let center = if overlay.start.y == overlay.end.y {
            (overlay.start.x + overlay.end.x) / 2.0
        } else {
            visible.x + visible.width / 2.0
        };
        let mut y = top - gap - height;
        if y < self.safe_top + margin {
            let handles = if overlay.handles && overlay.selected {
                size
            } else {
                0.0
            };
            y = bottom + handles + gap;
        }
        let floor = self
            .keyboard_top
            .map_or(self.surface.height, |top| top * scale);
        let y = y.min(floor - height - margin).max(0.0).round();
        let x = (center - width / 2.0)
            .min(self.surface.width - margin - width)
            .max(margin)
            .round();
        let menu = Rect::new(x, y, width, height);
        let radius = MENU_RADIUS * scale;
        self.frame.list.draw_shadow(
            menu,
            radius,
            Shadow::new(
                0.0,
                2.0 * scale,
                10.0 * scale,
                Color::rgba(0.0, 0.0, 0.0, 0.28),
            ),
        );
        self.frame
            .list
            .fill_rounded_rect(menu, radius, colors.surface);
        self.frame.list.stroke_rounded_rect(
            menu,
            radius,
            Border::new(scale.round().max(1.0), colors.divider),
        );
        let mut left = x;
        for (index, (action, label, measured)) in items.into_iter().take(count).enumerate() {
            let item = Rect::new(left, y, measured.width.ceil() + 2.0 * padding, height);
            let id = overlay.input ^ (0x5E1E_C7ED_0000_0010 + index as u64);
            if self.pressed == Some(id) {
                self.frame.list.push_rounded_clip(menu, radius);
                self.frame.list.fill_rect(item, colors.pressed_overlay);
                self.frame.list.pop_clip();
            }
            self.frame.draw_text(
                label,
                &style,
                Point::new(
                    item.x + padding,
                    item.y + ((height - measured.height) / 2.0).round(),
                ),
                None,
            );
            self.regions.push(Region {
                id,
                rect: item,
                space: None,
                kind: RegionKind::Menu(action),
                scroll: None,
                field: Some(overlay.input),
            });
            left += item.width;
        }
    }

    fn paint_text(
        &mut self,
        text: &TextNode,
        index: usize,
        rect: Rect,
        exact: Point,
        context: &Context,
    ) {
        let Some(meta) = self.text_meta.get(index) else {
            return;
        };
        if intersect(rect, context.clip).is_empty() {
            return;
        }
        let alpha = context.alpha;
        let mut style = text_style(text, self.theme, self.scale);
        style.color = fade(style.color, alpha);
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
            (exact.x + shift).clamp(-MAX_TEXT_ORIGIN, MAX_TEXT_ORIGIN),
            exact.y.clamp(-MAX_TEXT_ORIGIN, MAX_TEXT_ORIGIN),
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
                let spans = text_spans(text, self.theme, alpha);
                self.frame
                    .draw_text_spans(&spans, &style, origin, meta.constraint);
            }
        } else {
            let mut spans = if text.spans.is_empty() {
                vec![TextSpan::new(&text.content)]
            } else {
                text_spans(text, self.theme, alpha)
            };
            if let Some((x, y, color)) = text.shadow {
                // The same glyphs once more, behind, all in one colour.
                let shadow = TextStyle {
                    color: fade(color.resolve(self.theme), alpha),
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
                        fade(color.resolve(self.theme), alpha),
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
            fade(data.color.resolve(self.theme), context.alpha),
            data.stroke_width,
        );
    }

    fn paint_image(&mut self, data: &ImageNode, rect: Rect, context: &Context) {
        let radius = data.radius * self.scale;
        let tint = fade(data.tint.resolve(self.theme), context.alpha);
        // Pictures scrolled out of view cost no rasterisation.
        let visible = !intersect(rect, context.clip).is_empty();
        match &data.source {
            PictureSource::Image(image) => {
                self.frame
                    .list
                    .draw_image_fit(*image, rect, data.fit, radius, tint);
            }
            PictureSource::Vector(picture) if visible => {
                self.frame.draw_picture(picture, rect, radius, tint);
            }
            PictureSource::Svg(picture) if visible => {
                self.frame.draw_picture(picture, rect, radius, tint);
            }
            PictureSource::SvgIcon(picture) if visible => {
                self.frame.draw_svg_icon(picture, rect, tint);
            }
            _ => {}
        }
    }

    fn paint_box(
        &mut self,
        data: &BoxNode<M>,
        node: NodeId,
        slot: u32,
        rect: Rect,
        // A solid background on its way from one colour to another.
        animated_fill: Option<Color>,
        context: Context,
    ) {
        let scale = self.scale;
        let alpha = context.alpha;
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
                    color: fade(shadow.color, alpha),
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
            Border::new(width, fade(color.resolve(self.theme), alpha))
        });
        let fill = match (pressed, data.pressed_fill) {
            (true, Some(color)) => Some(Fill::Solid(color)),
            _ if focused && data.focused_fill.is_some() => data.focused_fill.map(Fill::Solid),
            _ => match (data.fill, animated_fill) {
                (Some(Fill::Solid(_)), Some(color)) => Some(Fill::Solid(ColorRef::Value(color))),
                (fill, _) => fill,
            },
        };
        // The dash pattern in pixels. An invalid one draws a solid border.
        let dash = border.zip(data.dash).map(|(border, kind)| match kind {
            DashKind::Css => Dash::css(rect, radius, border.width),
            DashKind::Pattern(length, gap) => Dash::new(length * scale, gap * scale),
        });
        let clear = morningstar_core::Color::rgba(0.0, 0.0, 0.0, 0.0);
        match (fill, border) {
            (Some(Fill::Solid(color)), Some(border)) => {
                let fill = fade(color.resolve(self.theme), alpha);
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
                self.frame.list.fill_rounded_rect(
                    rect,
                    radius,
                    fade(color.resolve(self.theme), alpha),
                );
            }
            (Some(Fill::Gradient(gradient)), border) => {
                self.frame.list.fill_gradient_rect(
                    rect,
                    radius,
                    LinearGradient::angled(
                        rect,
                        gradient.degrees,
                        fade(gradient.from.resolve(self.theme), alpha),
                        fade(gradient.to.resolve(self.theme), alpha),
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
            self.frame.list.fill_rounded_rect(
                rect,
                radius,
                fade(self.theme.colors.pressed_overlay, alpha),
            );
        }

        let visible = intersect(rect, context.clip);
        let mut inner = context;
        if !visible.is_empty() && !self.inert {
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
            self.scroll_nodes.push(node);
            let offset = if self.inert {
                self.rest_scroll(context.id)
            } else {
                let max = scroll_max(self.tree, node, self.keyboard_top);
                self.advance_scroll(context.id, max)
            };
            inner.dy -= (offset * scale).round();
            inner.scroll = Some(context.id);
            if !visible.is_empty() && !self.inert {
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
                // Off screen: skip the subtree without painting it. What
                // has an explicit id in it is still reported as laid out.
                let (from, to) = (self.node_cursor as u32, record.end_node);
                self.node_cursor = record.end_node as usize;
                self.text_cursor = record.end_text as usize;
                if !self.inert {
                    self.note_skipped(from, to, &inner);
                }
                continue;
            }
            self.paint(child, child_context);
        }

        if clips {
            self.frame.list.pop_clip();
        }
    }

    /// Records where the elements with explicit ids are in the records
    /// `from..to`, a subtree that is not painted because it is outside its
    /// scroll view: as laid out and moved by `context`, with nothing of
    /// them visible. This is what lets [`Ui::rect_of`] answer for a row
    /// far down a list. It costs a binary search per skipped subtree and
    /// a step per id in it.
    fn note_skipped(&mut self, from: u32, to: u32, context: &Context) {
        let first = self.keyed_table.partition_point(|keyed| keyed.slot < from);
        for keyed in &self.keyed_table[first..] {
            if keyed.slot >= to {
                break;
            }
            if let Some(node) = node_of(self.records, keyed.slot) {
                self.keyed.push(KeyedRect {
                    id: keyed.id,
                    rect: self.pixels(self.tree.rect(node), context),
                    visible: Rect::default(),
                });
            }
        }
    }

    /// The offset (dp) of the scroll view `id` inside a widget that has
    /// left the tree: where it was, kept for as long as it is drawn.
    fn rest_scroll(&mut self, id: u64) -> f32 {
        match self.scrolls.get_mut(&id) {
            Some(state) => {
                state.seen = self.generation;
                state.offset
            }
            None => 0.0,
        }
    }

    /// Advances the scroll view `id`, which can scroll `max` dp, to this
    /// frame and returns its offset in dp.
    fn advance_scroll(&mut self, id: u64, max: f32) -> f32 {
        let state = self.scrolls.entry(id).or_default();
        state.seen = self.generation;
        state.max = max;

        if let Some(mut glide) = state.glide.take()
            && self.dragging != Some(id)
        {
            // An animated move the application asked for. Its end is kept
            // within what can be scrolled now.
            let start = *glide.start.get_or_insert(self.elapsed);
            let t = GLIDE.progress(start, self.elapsed);
            let to = clamp_offset(glide.to, max);
            state.offset = glide.from + (to - glide.from) * GLIDE.easing.apply(t);
            if t < 1.0 {
                state.glide = Some(glide);
                self.animating = true;
            }
        }

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

    /// With the margins a focused field is given.
    fn reveal_offset(field: Rect, viewport: Rect, bottom: f32, offset: f32, max: f32) -> f32 {
        super::reveal_offset(
            field,
            viewport,
            bottom,
            offset,
            max,
            REVEAL_ABOVE,
            REVEAL_BELOW,
        )
    }

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
