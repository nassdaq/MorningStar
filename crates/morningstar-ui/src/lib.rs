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
//! | Image | [`Picture`]; a person: [`Avatar`] (image, or initials as the fallback) |
//! | Icon | [`Icon::vector`] with a [`VectorIcon`] (a small subset of SVG); the [`Icon::new`] glyph tile as a stand-in |
//! | A bold or coloured phrase inside a sentence | [`Text::rich`] with [`span`]s |
//! | Font family, letter spacing, capitals | [`Text::family`] (defaults in [`Fonts`]), [`Text::letter_spacing`], [`Text::uppercase`] |
//! | Dashed outline | [`Container::border_dashed`] |
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
//! * Children are placed at the **top-left** unless the parent says
//!   otherwise (`.center()`, `.cross_align(..)`, `.content_align(..)`).
//! * There is no right-to-left mode: `Start` is left. Arabic text is shaped
//!   and aligned correctly within its own box; give it `fill_width()`.
//!
//! [`Ui::overflows`] lists boxes whose children do not fit.
//!
//! # State and identity
//!
//! The tree returned by `view` is thrown away after each frame. Three
//! pieces of widget state outlive it: **which pressable a finger is
//! holding**, **each scroll view's position and motion**, and **which
//! [`TextInput`] has focus, with its caret**. They are kept in [`Ui`], keyed
//! by the widget's identity:
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
//! # Text input
//!
//! [`TextInput`] is an editable text. The application owns the string: the
//! widget shows the value it is given and delivers each new value as a
//! message, which `update` stores. Like [`Text`] it has no frame of its
//! own; the [`Container`] around it is the field, and
//! [`Container::focused_border`] its focused look. A tap focuses it and
//! raises the platform keyboard; tapping elsewhere, or the keyboard's Done
//! key, lets go. While the keyboard is up, boxes that pad for the bottom
//! safe area pad for the keyboard instead, and a focused field inside a
//! [`ScrollView`] is scrolled into view. There is no selection, copy or
//! paste yet. `docs/building-screens.md` has a worked form.
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
//! The layout tree, the hit regions, the message table and the display
//! list are reused and stop allocating once they have reached their
//! working size. Text shaping is cached by the text engine for as long as
//! the same text keeps being shown.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod color;
mod edit;
mod element;
mod theme;
mod ui;

pub use color::{ColorRef, ThemeColor, hex, hexa, rgb8, with_alpha};
pub use element::{
    Avatar, Button, Column, Container, Divider, Element, Gradient, Icon, Picture, Pressable, Row,
    SafeArea, SafeEdges, ScrollView, Spacer, Span, Stack, Text, TextInput, elevation, icon, span,
    text,
};
pub use theme::{Colors, Fonts, Radii, Spacing, TextRole, TextSpec, Theme, TypeScale};
pub use ui::{Application, Overflow, Ui};

pub use morningstar_core::{
    Capitalization, Color, Image, ImageError, ImageFit, ImeAction, KeyboardType, Shadow,
};
pub use morningstar_layout::{Align, Alignment, CrossAlign, MainAlign, Sizing};
pub use morningstar_runtime::{FontFamily, IconError, Setup, TextAlign, TextWeight, VectorIcon};

/// Everything a screen is written with: `use morningstar::ui::prelude::*;`.
pub mod prelude {
    pub use crate::{
        Align, Alignment, Application, Avatar, Button, Capitalization, Color, ColorRef, Colors,
        Column, Container, CrossAlign, Divider, Element, FontFamily, Fonts, Gradient, Icon, Image,
        ImageFit, ImeAction, KeyboardType, MainAlign, Picture, Pressable, Radii, Row, SafeArea,
        SafeEdges, ScrollView, Setup, Shadow, Sizing, Spacer, Spacing, Span, Stack, Text,
        TextAlign, TextInput, TextRole, TextSpec, TextWeight, Theme, ThemeColor, TypeScale, Ui,
        VectorIcon, col, elevation, hex, hexa, icon, rgb8, row, span, stack, text, with_alpha,
    };
}
