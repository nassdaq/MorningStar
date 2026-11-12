//! [`ScrollTo`]: asking a scroll view to move.

use std::hash::Hash;

use crate::element::key_hash;

/// Where a [`ScrollTo`] goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ScrollTarget {
    /// An offset in dp from the top of the scroll view with this identity;
    /// infinity is the end.
    Offset(u64, f32),
    /// The widget with this identity, brought into view with this much
    /// room (dp) around it.
    Reveal(u64, f32),
}

/// A request to scroll, made by the application through
/// [`Effects::scroll`](crate::Effects::scroll).
///
/// Scroll positions are widget state, not application state (the user
/// moves them by dragging), so an application does not set a position in
/// `view`. It asks, once, the way it asks for anything else that is not a
/// change of its own state: `update` records that the newest message
/// should be seen, and [`effects`](crate::Application::effects) says so.
///
/// ```
/// use morningstar_ui::prelude::*;
/// # #[derive(Default)] struct Chat { lines: Vec<String>, show_latest: bool }
/// # #[derive(Clone)] enum Message { Received(String) }
/// # impl Application for Chat {
/// #     type Message = Message;
/// #     fn view(&self) -> Element<Message> {
/// #         ScrollView::new(Column::new().extend(self.lines.iter().cloned().map(text))).id("chat").into()
/// #     }
/// fn update(&mut self, message: Message) {
///     match message {
///         Message::Received(line) => {
///             self.lines.push(line);
///             self.show_latest = true;
///         }
///     }
/// }
///
/// fn effects(&mut self, out: &mut Effects<Message>) {
///     if std::mem::take(&mut self.show_latest) {
///         out.scroll(ScrollTo::end("chat").animated());
///     }
/// }
/// # }
/// ```
///
/// The request is carried out in the next frame, after that frame's
/// layout: `ScrollTo::end` goes to the end of the content *as it is
/// then*, with the new line in it. A request whose scroll view or widget
/// is not in that frame is dropped.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollTo {
    pub(crate) target: ScrollTarget,
    pub(crate) animated: bool,
}

impl ScrollTo {
    const fn new(target: ScrollTarget) -> Self {
        Self {
            target,
            animated: false,
        }
    }

    /// The top of the scroll view with this `.id(..)`.
    pub fn top(scroll_view: impl Hash) -> Self {
        Self::new(ScrollTarget::Offset(key_hash(scroll_view), 0.0))
    }

    /// The end of the scroll view with this `.id(..)`: its last content at
    /// the bottom of the viewport.
    pub fn end(scroll_view: impl Hash) -> Self {
        Self::new(ScrollTarget::Offset(key_hash(scroll_view), f32::INFINITY))
    }

    /// `dp` from the top of the scroll view with this `.id(..)`, kept
    /// within what it can scroll.
    pub fn offset(scroll_view: impl Hash, dp: f32) -> Self {
        Self::new(ScrollTarget::Offset(key_hash(scroll_view), dp))
    }

    /// Brings the widget with this `.id(..)` into view by scrolling the
    /// scroll views it is in, the innermost first, each as little as
    /// needed; nothing moves if it is already in view. A widget taller
    /// than the viewport shows its top. While the keyboard is up, "in
    /// view" means above it.
    pub fn reveal(widget: impl Hash) -> Self {
        Self::new(ScrollTarget::Reveal(key_hash(widget), 0.0))
    }

    /// For [`reveal`](Self::reveal): keeps this many dp between the widget
    /// and the edges of the viewport.
    pub fn margin(mut self, dp: f32) -> Self {
        if let ScrollTarget::Reveal(_, margin) = &mut self.target {
            *margin = if dp.is_finite() { dp.max(0.0) } else { 0.0 };
        }
        self
    }

    /// Glides there (250 ms, easing out) instead of jumping. A touch on
    /// the scroll view stops the glide where it is. With "reduce motion"
    /// it jumps.
    pub fn animated(mut self) -> Self {
        self.animated = true;
        self
    }
}
