//! [`Viewport`]: what a view may know about the screen it is built for.

use std::cell::RefCell;
use std::hash::Hash;

use morningstar_core::{Insets, Size};
use morningstar_runtime::{Frame, FrameInfo};

use crate::element::{Text, key_hash};
use crate::theme::Theme;
use crate::ui::{TextMeta, measure_text};

/// The screen a view is being built for, in dp: its size, what the system
/// covers of it, the keyboard, and a way to measure text. Handed to
/// [`Application::view_in`](crate::Application::view_in).
///
/// ```
/// use morningstar_ui::prelude::*;
///
/// /// One column on a phone, two side by side on anything wider.
/// fn body(viewport: &Viewport, list: Element<()>, detail: Element<()>) -> Element<()> {
///     if viewport.width() >= 600.0 {
///         row![list.width(320.0), detail.fill()].fill().into()
///     } else {
///         list.fill()
///     }
/// }
///
/// /// A heading that drops a size rather than wrap.
/// fn heading(viewport: &Viewport, title: &str) -> Text {
///     let large = text(title.to_owned()).size(32.0).bold();
///     if viewport.measure(&large).width <= viewport.width() - 48.0 {
///         large
///     } else {
///         text(title.to_owned()).size(24.0).bold()
///     }
/// }
/// ```
///
/// The values are those of the frame being built, so a view that reads
/// them is right for every frame: when the size, the insets or the
/// keyboard change, a frame is produced and `view_in` is called again.
#[non_exhaustive]
pub struct Viewport<'a> {
    /// The whole surface, edge to edge, in dp.
    pub size: Size,
    /// What the status bar, cutout and gesture bar may cover on each edge,
    /// in dp: what [`SafeArea`](crate::SafeArea) pads by. The keyboard is
    /// not part of it.
    pub insets: Insets,
    /// How much of the bottom of the surface the soft keyboard covers, in
    /// dp, measured from the surface's bottom edge; `0` while it is down.
    pub keyboard_inset: f32,
    /// Physical pixels per dp.
    pub scale: f32,
    /// The user has asked the system for less motion: animations and
    /// transitions jump to their end (see
    /// [`Ui::set_reduce_motion`](crate::Ui::set_reduce_motion)).
    pub reduce_motion: bool,
    focus: Option<u64>,
    theme: Theme,
    // Measuring shapes text, which needs the frame's text engine; a view
    // holds only `&Viewport`, so that sub-views can share it.
    frame: RefCell<&'a mut Frame>,
}

impl<'a> Viewport<'a> {
    pub(crate) fn new(
        info: &FrameInfo,
        frame: &'a mut Frame,
        theme: Theme,
        focus: Option<u64>,
        reduce_motion: bool,
    ) -> Self {
        let scale = info.scale;
        Self {
            size: info.logical_size(),
            insets: Insets::new(
                info.insets.left / scale,
                info.insets.top / scale,
                info.insets.right / scale,
                info.insets.bottom / scale,
            ),
            keyboard_inset: info.keyboard_inset / scale,
            scale,
            reduce_motion,
            focus,
            theme,
            frame: RefCell::new(frame),
        }
    }

    /// Width of the surface in dp.
    pub fn width(&self) -> f32 {
        self.size.width
    }

    /// Height of the surface in dp.
    pub fn height(&self) -> f32 {
        self.size.height
    }

    /// The size, in dp, of what is neither under system bars nor under the
    /// keyboard: the room a [`SafeArea`](crate::SafeArea) at the root gives
    /// its child.
    pub fn safe_size(&self) -> Size {
        let bottom = self.insets.bottom.max(self.keyboard_inset);
        Size::new(
            (self.size.width - self.insets.left - self.insets.right).max(0.0),
            (self.size.height - self.insets.top - bottom).max(0.0),
        )
    }

    /// True while the soft keyboard covers part of the surface.
    pub fn keyboard_visible(&self) -> bool {
        self.keyboard_inset > 0.0
    }

    /// True if a [`TextInput`](crate::TextInput) had focus when this frame
    /// began.
    pub fn has_focus(&self) -> bool {
        self.focus.is_some()
    }

    /// True if the [`TextInput`](crate::TextInput) with this explicit
    /// `.id(..)` had focus when this frame began.
    pub fn is_focused(&self, id: impl Hash) -> bool {
        self.focus == Some(key_hash(id))
    }

    /// The size, in dp, `text` has on one line: its style, weight, family,
    /// letter spacing and spans taken into account, exactly as the layout
    /// will measure it (width in whole pixels). The text's own sizing
    /// (`.width(..)`, `.fill_width()`) plays no part.
    ///
    /// The measurement is cached by the text engine, and the layout that
    /// follows asks the same question, so measuring what is then shown
    /// costs nothing extra.
    pub fn measure(&self, text: &Text) -> Size {
        self.measure_node(text, None)
    }

    /// The size, in dp, `text` has when wrapped at `max_width` dp. Its
    /// `max_lines`, if any, bounds the height, as in layout.
    pub fn measure_wrapped(&self, text: &Text, max_width: f32) -> Size {
        self.measure_node(text, Some(max_width.max(0.0)))
    }

    /// How many lines `text` takes when wrapped at `max_width` dp (not
    /// bounded by its `max_lines`).
    pub fn line_count(&self, text: &Text, max_width: f32) -> u32 {
        let mut meta = TextMeta::default();
        let mut frame = self.frame.borrow_mut();
        let node = &text.node;
        measure_text(
            &mut frame,
            node,
            &mut meta,
            &self.theme,
            self.scale,
            Some(max_width.max(0.0)),
        );
        let line = meta.line_height;
        if line > 0.0 {
            (meta.measured.height / line).round().max(1.0) as u32
        } else {
            1
        }
    }

    fn measure_node(&self, text: &Text, max_width: Option<f32>) -> Size {
        let mut meta = TextMeta::default();
        let mut frame = self.frame.borrow_mut();
        measure_text(
            &mut frame,
            &text.node,
            &mut meta,
            &self.theme,
            self.scale,
            max_width,
        )
    }
}
