//! What a screen reader will find on a screen, as a picture.

use morningstar_gpu::{GpuError, render_offscreen_with_images};
use morningstar_runtime::{
    Actions, App, Color, Frame, FrameInfo, Point, Rect, Role, SemanticsNode, SemanticsTree, States,
    SurfaceSize, TextStyle, TextWeight,
};

use crate::Image;

/// The colour a node is outlined in: something that is operated, something
/// that is read, or a container.
fn ink(node: &SemanticsNode) -> Color {
    let operable = Actions::ACTIVATE.0
        | Actions::FOCUS.0
        | Actions::INCREMENT.0
        | Actions::DECREMENT.0
        | Actions::SET_TEXT.0;
    if node.actions.0 & operable != 0 {
        // Magenta.
        Color::rgb(1.0, 0.0, 0.55)
    } else if matches!(node.role, Role::List | Role::Group) {
        // Amber.
        Color::rgb(1.0, 0.55, 0.0)
    } else {
        // Cyan.
        Color::rgb(0.0, 0.6, 1.0)
    }
}

/// One line of the legend: what a screen reader says and offers.
fn describe(node: &SemanticsNode) -> String {
    let mut line = format!("{:?}", node.role);
    if !node.label.is_empty() {
        line.push_str(&format!("  \u{201c}{}\u{201d}", node.label));
    }
    if !node.value.is_empty() {
        line.push_str(&format!("  = {}", node.value));
    }
    if !node.hint.is_empty() {
        line.push_str(&format!("  (hint: {})", node.hint));
    }
    let states = [
        (States::DISABLED, "disabled"),
        (States::SELECTED, "selected"),
        (States::CHECKED, "checked"),
        (States::FOCUSED, "focused"),
        (States::PASSWORD, "password"),
        (States::MULTILINE, "multi-line"),
        (States::EXPANDED, "expanded"),
        (States::LIVE_POLITE, "live"),
        (States::LIVE_ASSERTIVE, "live, assertive"),
    ];
    for (state, name) in states {
        if node.is(state) {
            line.push_str(&format!("  [{name}]"));
        }
    }
    if node.is(States::CHECKABLE) && !node.is(States::CHECKED) {
        line.push_str("  [not checked]");
    }
    let actions = [
        (Actions::ACTIVATE, "activate"),
        (Actions::LONG_PRESS, "long press"),
        (Actions::SCROLL_FORWARD, "scroll forward"),
        (Actions::SCROLL_BACKWARD, "scroll backward"),
        (Actions::SET_TEXT, "set text"),
        (Actions::INCREMENT, "increment"),
        (Actions::DECREMENT, "decrement"),
    ];
    let offered: Vec<&str> = actions
        .iter()
        .filter(|(action, _)| node.can(*action))
        .map(|(_, name)| *name)
        .collect();
    if !offered.is_empty() {
        line.push_str(&format!("  \u{2192} {}", offered.join(", ")));
    }
    line
}

/// The outline of `rect`, `width` pixels thick, inside it.
fn outline(frame: &mut Frame, rect: Rect, width: f32, color: Color) {
    let (x, y, w, h) = (rect.x, rect.y, rect.width, rect.height);
    frame.list.fill_rect(Rect::new(x, y, w, width), color);
    frame
        .list
        .fill_rect(Rect::new(x, y + h - width, w, width), color);
    frame.list.fill_rect(Rect::new(x, y, width, h), color);
    frame
        .list
        .fill_rect(Rect::new(x + w - width, y, width, h), color);
}

/// A number on a small plate, with its top-left corner at `origin`.
/// Returns the plate.
fn badge(frame: &mut Frame, number: usize, origin: Point, size: f32, color: Color) -> Rect {
    let text = number.to_string();
    let mut style = TextStyle::new(size, Color::WHITE);
    style.weight = TextWeight::BOLD;
    style.line_height = 1.15;
    let measured = frame.measure_text(&text, &style, None);
    let pad = (size * 0.25).round();
    let plate = Rect::new(
        origin.x,
        origin.y,
        measured.width.ceil() + 2.0 * pad,
        measured.height.ceil(),
    );
    frame.list.fill_rect(plate, color);
    frame.draw_text(&text, &style, Point::new(plate.x + pad, plate.y), None);
    plate
}

/// Renders one frame of `app` as [`screenshot_frame`](crate::screenshot_frame)
/// does, described for accessibility (ADR 0032), with every node a screen
/// reader can land on outlined and numbered in reading order, and beside
/// it a legend: for each number its role, what is said, its states and the
/// actions it offers. Magenta is something operated, cyan something read,
/// amber a container.
///
/// It is what a developer looks at to see a screen as TalkBack will: a
/// control with no number cannot be reached, a row with five numbers is
/// five swipes, and the numbers are the order of those swipes.
///
/// Also returns the tree. An app that describes nothing (one not built on
/// the UI layer) gives the plain picture and an empty tree.
pub fn screenshot_semantics(
    app: &mut dyn App,
    info: &FrameInfo,
) -> Result<(Image, SemanticsTree), GpuError> {
    let mut frame = Frame::for_app(app);
    frame.begin();
    let described = info.with_accessibility(true);
    app.frame(&described, &mut frame);
    let tree = app.semantics().cloned().unwrap_or_default();

    // Over the screen: outlines and numbers.
    let scale = info.scale;
    let stroke = (1.5 * scale).round().max(2.0);
    let size = (11.0 * scale).round();
    for (number, node) in tree.nodes().iter().enumerate().skip(1) {
        let color = ink(node);
        outline(&mut frame, node.bounds, stroke, color);
        // Inside the corner, so that a node at the edge keeps its number.
        badge(
            &mut frame,
            number,
            Point::new(node.bounds.x, node.bounds.y),
            size,
            color,
        );
    }
    let screen = render_offscreen_with_images(
        info.size,
        frame.clear,
        &frame.list,
        frame.atlas(),
        frame.images(),
    )?;
    let screen = Image::new(info.size.width, info.size.height, screen);

    // Beside it: the legend, drawn with the app's own fonts.
    const MAX_HEIGHT: f32 = 8000.0;
    let width = (420.0 * scale).round();
    let margin = (12.0 * scale).round();
    let text_size = (12.0 * scale).round();
    let style = TextStyle::new(text_size, Color::rgb(0.07, 0.07, 0.08));
    let gap = (6.0 * scale).round();
    let indent = (34.0 * scale).round();
    frame.begin();
    let mut y = margin;
    for (number, node) in tree.nodes().iter().enumerate().skip(1) {
        if y > MAX_HEIGHT - 4.0 * text_size {
            frame.draw_text("\u{2026}", &style, Point::new(margin, y), None);
            y += text_size * 2.0;
            break;
        }
        let plate = badge(&mut frame, number, Point::new(margin, y), size, ink(node));
        let said = frame.draw_text(
            &describe(node),
            &style,
            Point::new(margin + indent, y),
            Some(width - 2.0 * margin - indent),
        );
        y += said.height.max(plate.height) + gap;
    }
    let height = (y + margin).clamp(1.0, MAX_HEIGHT);
    let legend_size = SurfaceSize::new(width as u32, height as u32);
    let legend = render_offscreen_with_images(
        legend_size,
        Color::rgb(0.96, 0.96, 0.95),
        &frame.list,
        frame.atlas(),
        frame.images(),
    )?;
    let legend = Image::new(legend_size.width, legend_size.height, legend);

    // Side by side on one sheet.
    let mut sheet = Image::filled(
        screen.width() + legend.width(),
        screen.height().max(legend.height()),
        [245, 245, 242, 255],
    );
    for (image, left) in [(&screen, 0), (&legend, screen.width())] {
        for y in 0..image.height() {
            for x in 0..image.width() {
                sheet.set_pixel(left + x, y, image.pixel(x, y));
            }
        }
    }
    Ok((sheet, tree))
}
