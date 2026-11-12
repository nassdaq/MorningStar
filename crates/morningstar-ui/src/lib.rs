//! Morningstar's declarative UI layer.
//!
//! A screen is described as a tree of widgets, as a function of the
//! application's state, in the vocabulary designs are drawn in: columns,
//! rows and stacks with padding, gaps and alignment; sizes that are fixed,
//! hug their contents or fill their parent; boxes with a background, a
//! corner radius, a border and a shadow; text in a type scale.
//!
//! ```
//! use morningstar_ui::prelude::*;
//!
//! #[derive(Default)]
//! struct Wallet {
//!     balance_visible: bool,
//! }
//!
//! #[derive(Clone)]
//! enum Message {
//!     ToggleBalance,
//! }
//!
//! impl Application for Wallet {
//!     type Message = Message;
//!
//!     fn view(&self) -> Element<Message> {
//!         let balance = if self.balance_visible { "TSh 1,250,000" } else { "TSh ••••••" };
//!         SafeArea::new(
//!             col![
//!                 text("Salio lako").caption().muted(),
//!                 text(balance).display(),
//!                 Button::new("Onyesha / Ficha").on_press(Message::ToggleBalance).fill_width(),
//!             ]
//!             .gap(8.0)
//!             .padding(16.0),
//!         )
//!         .into()
//!     }
//!
//!     fn update(&mut self, message: Message) {
//!         match message {
//!             Message::ToggleBalance => self.balance_visible = !self.balance_visible,
//!         }
//!     }
//! }
//! ```
//!
//! # The model
//!
//! * **State** is a struct that implements [`Application`].
//! * **[`view`](Application::view)** builds an [`Element`] tree from
//!   `&self`. It is called again whenever a frame is needed; frames are
//!   produced on demand (after input, while something animates), not at
//!   every display refresh.
//! * **Messages.** Tappable widgets carry a value of the application's
//!   `Message` type. A completed tap hands it to
//!   [`update`](Application::update), which changes the state.
//! * **[`Ui`]** runs an `Application` on the runtime:
//!   `morningstar::app!(Ui::new(MyApp::default()))`.
//!
//! # Units
//!
//! Every length in this API is in **dp** (density-independent units: dp on
//! Android, points on iOS), font sizes included. The layer converts to
//! physical pixels with the display's scale and snaps box edges to whole
//! pixels. Colours given as design hex values go through [`hex`].
//!
//! # Widgets
//!
//! | Design concept | Widget |
//! | --- | --- |
//! | Vertical / horizontal auto-layout | [`col!`], [`row!`] ([`Column`], [`Row`]) |
//! | Layers, badges, overlays | [`stack!`] ([`Stack`]), children pinned with `.place(..)` |
//! | A frame with fill, radius, stroke, shadow | [`Container`] (what all of the above are) |
//! | Text | [`text`] / [`Text`] |
//! | Button | [`Button`]; anything else tappable: `.on_press(..)` on a box ([`Pressable`]) |
//! | Line between rows | [`Divider`] |
//! | Flexible or fixed gap | [`Spacer`] |
//! | Scrolling content | [`ScrollView`] |
//! | Status bar / gesture bar margins | [`SafeArea`], [`Container::safe_area`] |
//! | Image | [`Picture`]; an SVG file: [`Picture::svg`] with an [`SvgAsset`]; a person: [`Avatar`] (image, or initials as the fallback) |
//! | Icon | [`Icon::svg`] with a loaded SVG file, or [`Icon::vector`] with a [`VectorIcon`] (one-colour markup in the source); the [`Icon::new`] glyph tile as a stand-in |
//! | A bold or coloured phrase inside a sentence | [`Text::rich`] with [`span`]s |
//! | Font family, letter spacing, capitals | [`Text::family`] (defaults in [`Fonts`]), [`Text::letter_spacing`], [`Text::uppercase`] |
//! | Dashed outline | [`Container::border_dashed`] |
//! | Chips or tags that wrap (`flex-wrap`) | [`Container::wrap`] with [`Container::line_gap`] ([`Wrap`]) |
//! | A layout that depends on the screen | [`Application::view_in`] and its [`Viewport`] |
//! | A change that should be seen happening | [`Container::animate`]; appearing and disappearing: [`Container::transition`] |
//! | Screens and Back | [`Navigator`] |
//!
//! # Layout rules worth knowing
//!
//! They are the layout engine's (ADR 0010), unchanged:
//!
//! * Sizes are **hug** by default. A box is as big as its children; a text
//!   as big as its words.
//! * **Fill inside hug collapses.** A `fill_width()` child of a box that
//!   hugs its width gets no width. Give the parent a width (fixed or fill).
//! * In a row, **the text that should wrap or give way must
//!   `fill_width()`**. Two long hugging texts in one row overflow.
//! * A [`ScrollView`] needs a height from its parents (it fills by default).
//! * A row **wraps** only if told to ([`Container::wrap`]); then a child
//!   that does not fit starts the next line, and the row is as tall as its
//!   lines.
//! * Children are placed at the **top-left** unless the parent says
//!   otherwise (`.center()`, `.cross_align(..)`, `.content_align(..)`).
//! * There is no right-to-left mode: `Start` is left. Arabic text is shaped
//!   and aligned correctly within its own box; give it `fill_width()`.
//!
//! [`Ui::overflows`] lists boxes whose children do not fit.
//!
//! # State and identity
//!
//! The tree returned by `view` is thrown away after each frame. Four
//! pieces of widget state outlive it: **which pressable a finger is
//! holding**, **each scroll view's position and motion** (a
//! [`TextInput`] that scrolls inside itself counts as one), **which
//! [`TextInput`] has focus, with its caret**, and **what an animated
//! widget is on its way from**. They are kept in [`Ui`], keyed by the
//! widget's identity:
//!
//! 1. A widget's identity is its **position in the tree**: the chain of
//!    child indices from the root.
//! 2. `.id(value)` replaces that with an **explicit identity**, the hash of
//!    `value`. Ids are global to the screen, not scoped by the parent, so
//!    two widgets must not share one. The descendants of a widget with an
//!    id are positions under *that id*.
//! 3. State belongs to an identity for as long as a widget with that
//!    identity is in the tree **in consecutive frames**. A scroll view that
//!    is absent from one frame starts again at the top.
//!
//! In practice: give the root of each screen an `.id("home")`, so that two
//! screens with a scroll view in the same place do not share a scroll
//! position; give list rows an `.id(("row", item.id))` if rows can be
//! inserted or removed while a finger is on one; and give every
//! [`TextInput`] an `.id("phone")`, so that focus stays with the field when
//! something appears or disappears above it. An input that leaves the tree
//! loses focus, and the keyboard goes away.
//!
//! # The screen: size, insets, keyboard
//!
//! An application that needs to know what it is drawn on implements
//! [`Application::view_in`] instead of `view`. Its [`Viewport`] gives the
//! surface's size, the safe-area insets and the keyboard's height in dp,
//! says which field has focus, and measures text
//! ([`Viewport::measure`], [`Viewport::measure_wrapped`],
//! [`Viewport::line_count`]) exactly as layout will, so a view can choose
//! between two layouts instead of guessing from a character count. It
//! costs nothing if `view` is all an application implements.
//!
//! # Scrolling
//!
//! A [`ScrollView`]'s position belongs to the widget, by identity. The
//! rule: **while a scroll view with the same identity is in consecutive
//! frames, its position is kept**, whatever `view` rebuilds around or
//! inside it. Content that grows leaves the position where it is (there
//! is then further to scroll); content that shrinks under it brings it
//! back to the new end. So give a scroll view whose content changes an
//! `.id(..)`, and keep that id.
//!
//! The application moves a scroll view by asking, from
//! [`Application::effects`]: [`Effects::scroll`] with a [`ScrollTo`] (the
//! top, the end, an offset, or as far as brings a widget into view; at
//! once or gliding). The request is carried out in the next frame, after
//! its layout, so "the end" is the end of the content that frame has.
//!
//! [`Ui::rect_of`] is where a widget was laid out, also when it is
//! scrolled out of sight; [`Ui::visible_rect_of`] is the part that shows.
//!
//! # Animation
//!
//! `view` always describes the screen as it should be. A widget shows a
//! change gradually only if it asks to:
//!
//! * [`Container::animate`]`(duration, easing)`: when a later `view` gives
//!   the same widget another opacity, transform, offset, solid background
//!   or fixed size, it is drawn on the way from the old value to the new.
//! * [`Container::transition`]: how a widget enters in the first frame its
//!   identity is in the tree, and leaves in the first frame it is not (it
//!   is then still drawn, over the screen and outside its layout, until
//!   it is gone).
//! * [`ScrollTo::animated`]: a scroll that glides.
//!
//! Frames are produced only while something is on its way; when the last
//! animation ends the app is as idle as it was. When the user has asked the
//! system for less motion (or [`Ui::set_reduce_motion`] has) every one of
//! these takes no time. A widget that asks for none of
//! this costs one comparison per frame.
//!
//! # Navigation
//!
//! [`Navigator`] is the stack of screens as plain application state:
//! `push`, `replace`, `pop`, and `back()` for [`Application::back`].
//! `navigator.transition()` on each screen's root, beside its `.id(..)`,
//! slides screens in and out in the direction of the last change.
//!
//! # Text input
//!
//! [`TextInput`] is an editable text. The application owns the string: the
//! widget shows the value it is given and delivers each new value as a
//! message, which `update` stores. Like [`Text`] it has no frame of its
//! own; the [`Container`] around it is the field, and
//! [`Container::focused_border`] its focused look. A tap focuses it and
//! raises the platform keyboard; tapping elsewhere, or the keyboard's Done
//! key, lets go. While the keyboard is up, boxes that pad for the bottom
//! safe area pad for the keyboard instead, and the focused field is kept in
//! view: every [`ScrollView`] around it scrolls, and what scrolling cannot
//! do the screen makes up by shifting. A multi-line field can be bounded
//! ([`TextInput::max_lines`]) and then scrolls inside itself.
//!
//! A field behaves as a native one does: a double tap or a long press
//! selects a word and brings up two handles and a small menu (Cut, Copy,
//! Paste, Select all) drawn by the framework; a hardware keyboard's
//! arrows and shortcuts work; the clipboard is the platform's. None of it
//! reaches the application, which still sees only the new string;
//! [`Ui::selection`] and [`Ui::select`] reach in from outside.
//! `docs/building-screens.md` has a worked form.
//!
//! # The system's bars
//!
//! The screen is drawn under the status bar and the navigation bar. Their
//! icons are dark over a light theme and light over a dark one without
//! the application saying anything; [`Application::system_bars`] overrides
//! that for a screen whose edges are not the theme's background.
//!
//! # Talking to a server, sound, and remembering things
//!
//! `update` returns at once and never touches the platform. Work that
//! takes time is started from [`Application::effects`] by queueing
//! requests on an [`Effects`]: [`http`](Effects::http),
//! [`start_recording`](Effects::start_recording) and
//! [`stop_recording`](Effects::stop_recording), [`play`](Effects::play),
//! [`store`](Effects::store) and [`load`](Effects::load). The host does
//! the work off the render thread and the result comes back as a message.
//! In tests, [`Ui::testing`] holds each request for the test to answer
//! ("the server replies this"). ADR 0019; `docs/building-screens.md` has a
//! complete example.
//!
//! # Accessibility
//!
//! When TalkBack or VoiceOver is running (the host says so), each frame is
//! also described as a tree of semantic nodes derived from its widgets,
//! and the host mirrors it to the platform (ADR 0032). The defaults need
//! nothing: a [`Text`] is text, a [`Button`] a button with its label, a
//! [`TextInput`] an edit box with its placeholder as hint (a secure
//! field's value is bullets, never its characters), a [`ScrollView`] a
//! list, and any box with `on_press` is one stop that reads the texts
//! inside it in order. Icons, pictures and shapes say nothing.
//!
//! What the defaults cannot know is said with modifiers every widget has:
//! `label` (required for a control with no text inside it), `hint`,
//! `value`, `role`, `heading`, `selected`, `checked`, `expanded`,
//! `disabled`, `live`, `exclude_semantics`, `merge_semantics` and
//! `semantics_group`; on [`Text`] and [`TextInput`], where `label` and
//! `role` mean the text style, they are `semantic_label` and
//! `semantic_role`. [`Effects::announce`] has the reader say something
//! that is not on the screen.
//!
//! In a test, [`Ui::set_accessibility`] turns the description on,
//! [`Ui::semantics`] is the tree, [`Ui::perform`] does what a screen
//! reader's user can do, and [`Ui::accessibility_issues`] lists missing
//! labels, small touch targets, twins and low contrast. While nothing asks
//! for it, none of this is built.
//!
//! # Touch
//!
//! A tap is a finger going down and coming up on the same pressable. If
//! the finger slides off before lifting, nothing happens (sliding back on
//! re-arms it); a system cancel abandons the press. One finger at a time:
//! while a gesture is in progress, other fingers are ignored. Inside a
//! [`ScrollView`], moving more than 8 dp vertically turns the gesture into
//! a scroll and abandons the press. After a message has been delivered,
//! touches are ignored until the next frame has been laid out, so a stale
//! screen cannot be tapped.
//!
//! # What allocates
//!
//! Per rebuild: the element tree itself (one `Vec` per box that has
//! children, plus whatever `String`s the application formats; a
//! `&'static str` label is not copied) and one `Vec` of text references.
//! While a widget with a [`Container::transition`] is on screen, the
//! element tree is freed one frame later than otherwise.
//! The layout tree, the hit regions, the message table and the display
//! list are reused and stop allocating once they have reached their
//! working size. Text shaping is cached by the text engine for as long as
//! the same text keeps being shown.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod color;
mod edit;
mod effects;
mod element;
mod motion;
mod navigator;
mod scroll;
mod semantics;
mod theme;
mod ui;
mod viewport;

pub use color::{ColorRef, ThemeColor, hex, hexa, rgb8, with_alpha};
pub use effects::Effects;
pub use element::{
    Avatar, Button, Column, Container, Divider, Element, Gradient, Icon, Picture, Pressable, Row,
    SafeArea, SafeEdges, ScrollView, Spacer, Span, Stack, Text, TextInput, Wrap, elevation, icon,
    span, text,
};
pub use motion::{Animation, Easing, Transition};
pub use navigator::{Navigation, Navigator};
pub use scroll::ScrollTo;
pub use semantics::{AccessibilityIssue, IssueKind};
pub use theme::{Colors, Fonts, Radii, Spacing, TextRole, TextSpec, Theme, TypeScale};
pub use ui::{Application, Overflow, Ui};
pub use viewport::Viewport;

pub use morningstar_core::{Action, Actions, Live, Role, SemanticsNode, SemanticsTree, States};
pub use morningstar_core::{
    AudioClip, AudioError, HostState, HttpError, HttpMethod, HttpRequest, HttpResponse, Recording,
    ServiceCall, ServiceId, ServiceRequest, ServiceResult,
};
pub use morningstar_core::{
    BarContent, Capitalization, CaretMove, Color, Image, ImageError, ImageFit, ImeAction,
    KeyboardType, Shadow, SystemBars, Transform,
};
pub use morningstar_layout::{Align, Alignment, CrossAlign, MainAlign, Sizing};
pub use morningstar_runtime::{
    FontFamily, IconError, Layer, LineCap, LineJoin, Setup, Shape, SvgAsset, SvgError,
    SvgErrorKind, SvgPicture, TextAlign, TextWeight, VectorIcon, VectorPicture,
};

/// Everything a screen is written with: `use morningstar::ui::prelude::*;`.
pub mod prelude {
    pub use crate::{Action, Live, Role};
    pub use crate::{
        Align, Alignment, Application, Avatar, Button, Capitalization, Color, ColorRef, Colors,
        Column, Container, CrossAlign, Divider, Element, FontFamily, Fonts, Gradient, Icon, Image,
        ImageFit, ImeAction, KeyboardType, Layer, LineCap, LineJoin, MainAlign, Picture, Pressable,
        Radii, Row, SafeArea, SafeEdges, ScrollView, Setup, Shadow, Shape, Sizing, Spacer, Spacing,
        Span, Stack, SvgAsset, SvgPicture, Text, TextAlign, TextInput, TextRole, TextSpec,
        TextWeight, Theme, ThemeColor, Transform, TypeScale, Ui, VectorIcon, VectorPicture, col,
        elevation, hex, hexa, icon, rgb8, row, span, stack, text, with_alpha,
    };
    pub use crate::{
        AudioClip, AudioError, Effects, HttpError, HttpMethod, HttpRequest, HttpResponse,
        Recording, ServiceRequest, ServiceResult,
    };
    pub use crate::{BarContent, SystemBars};
    pub use crate::{Easing, Navigation, Navigator, ScrollTo, Transition, Viewport, Wrap};
}
