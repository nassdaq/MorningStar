//! The layout cache keys on what shaping depends on, which is not colour:
//! a paragraph that fades, span colours included, is shaped once.

use morningstar_core::{Color, DisplayList, DrawOp, Point};
use morningstar_text::{TextContext, TextSpan, TextStyle, TextWeight};

const LATIN: &[u8] =
    include_bytes!("../../../examples/hello-gpu/assets/fonts/NotoSans-Regular.ttf");

fn colors(list: &DisplayList) -> Vec<Color> {
    list.ops()
        .iter()
        .filter_map(|op| match *op {
            DrawOp::Glyph { color, .. } => Some(color),
            _ => None,
        })
        .collect()
}

#[test]
fn fading_rich_text_is_shaped_once() {
    let mut text = TextContext::new();
    text.add_font(LATIN);
    let red = Color::rgb(1.0, 0.0, 0.0);
    let mut first: Option<Vec<_>> = None;
    for step in 1..=10 {
        let alpha = step as f32 / 10.0;
        let fade = |color: Color| Color::rgba(color.r, color.g, color.b, alpha);
        text.begin_frame();
        let mut list = DisplayList::new();
        let spans = [
            TextSpan::new("New here? "),
            TextSpan {
                color: Some(fade(red)),
                ..TextSpan::new("Join")
            },
        ];
        let style = TextStyle::new(24.0, fade(Color::WHITE));
        text.draw_spans(&mut list, &spans, &style, Point::new(10.0, 10.0), None);
        assert_eq!(text.shaped_count(), 1, "step {step}");

        // The glyphs are where they were, in this frame's colours.
        let drawn = colors(&list);
        assert_eq!(drawn.len(), 12, "eight letters, then four");
        assert!(drawn[..8].iter().all(|color| *color == fade(Color::WHITE)));
        assert!(drawn[8..].iter().all(|color| *color == fade(red)));
        let places: Vec<_> = list
            .ops()
            .iter()
            .filter_map(|op| match *op {
                DrawOp::Glyph { rect, .. } => Some(rect),
                _ => None,
            })
            .collect();
        match &first {
            Some(first) => assert_eq!(&places, first),
            None => first = Some(places),
        }
    }
}

#[test]
fn what_shaping_depends_on_still_reshapes() {
    let mut text = TextContext::new();
    text.add_font(LATIN);
    let style = TextStyle::new(24.0, Color::WHITE);
    let mut list = DisplayList::new();
    let mut draw = |text: &mut TextContext, spans: &[TextSpan<'_>], style: &TextStyle| {
        text.draw_spans(&mut list, spans, style, Point::new(0.0, 0.0), None);
        text.shaped_count()
    };
    let plain = [TextSpan::new("New here? "), TextSpan::new("Join")];
    assert_eq!(draw(&mut text, &plain, &style), 1);
    assert_eq!(draw(&mut text, &plain, &style), 1, "drawn again: cached");
    // A span's colour appearing or going: not shaping.
    let colored = [
        TextSpan::new("New here? "),
        TextSpan {
            color: Some(Color::BLACK),
            ..TextSpan::new("Join")
        },
    ];
    assert_eq!(draw(&mut text, &colored, &style), 1);
    // Weight, size, text and where the spans divide: shaping.
    let bold = [
        TextSpan::new("New here? "),
        TextSpan {
            weight: Some(TextWeight::BOLD),
            ..TextSpan::new("Join")
        },
    ];
    assert_eq!(draw(&mut text, &bold, &style), 2);
    assert_eq!(
        draw(&mut text, &plain, &TextStyle::new(25.0, Color::WHITE)),
        3
    );
    let divided = [TextSpan::new("New here? Jo"), TextSpan::new("in")];
    assert_eq!(draw(&mut text, &divided, &style), 4);
    let other = [TextSpan::new("New here! "), TextSpan::new("Join")];
    assert_eq!(draw(&mut text, &other, &style), 5);
}
