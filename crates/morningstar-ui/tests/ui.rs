//! Behaviour of the UI layer, driven the way the engine drives it.

mod common;

use common::{Harness, Model, Msg, close, phone, phone_with_bars};
use morningstar_core::{DrawOp, Insets, PointerPhase, Rect, SurfaceSize};
use morningstar_runtime::{App, Frame, FrameInfo, TextStyle};
use morningstar_ui::prelude::*;

// ---------------------------------------------------------------- layout

fn two_boxes(_: &Model) -> Element<Msg> {
    row![
        Container::empty().size(100.0, 50.0).id("a"),
        Container::empty().fill_width().height(50.0).id("b"),
    ]
    .padding(16.0)
    .gap(8.0)
    .id("row")
    .into()
}

#[test]
fn layout_is_in_dp_and_scales_to_pixels() {
    // 1080 px wide at 3 px per dp is 360 dp.
    let harness = Harness::new(two_boxes);
    assert_eq!(harness.dp_rect("a"), Rect::new(16.0, 16.0, 100.0, 50.0));
    assert_eq!(harness.dp_rect("b"), Rect::new(124.0, 16.0, 220.0, 50.0));
    assert_eq!(harness.px_rect("a"), Rect::new(48.0, 48.0, 300.0, 150.0));
    assert_eq!(harness.px_rect("b"), Rect::new(372.0, 48.0, 660.0, 150.0));

    // The same description on a 1 px per dp surface of the same pixel
    // size is a 1080 dp wide layout.
    let harness = Harness::with(two_boxes, phone().with_scale(1.0));
    assert_eq!(harness.px_rect("a"), Rect::new(16.0, 16.0, 100.0, 50.0));
    assert_eq!(harness.px_rect("b"), Rect::new(124.0, 16.0, 940.0, 50.0));
}

#[test]
fn edges_snap_to_whole_pixels_at_fractional_scales() {
    for scale in [1.0, 1.5, 2.0, 2.625, 2.75, 3.0, 3.5] {
        let harness = Harness::with(two_boxes, phone().with_scale(scale));
        let (a, b) = (harness.px_rect("a"), harness.px_rect("b"));
        for value in [a.x, a.y, a.width, a.height, b.x, b.width] {
            assert_eq!(value, value.round(), "scale {scale}: {a:?} {b:?}");
        }
        // Each edge is the dp edge rounded, so nothing drifts.
        assert_eq!(a.x, (16.0 * scale).round(), "scale {scale}");
        assert_eq!(a.x + a.width, (116.0 * scale).round(), "scale {scale}");
        assert_eq!(b.x, (124.0 * scale).round(), "scale {scale}");
        assert_eq!(b.x + b.width, 1080.0 - (16.0 * scale).round());
    }
}

#[test]
fn siblings_that_share_an_edge_in_dp_share_it_in_pixels() {
    fn thirds(_: &Model) -> Element<Msg> {
        row![
            Container::empty().fill().id("a"),
            Container::empty().fill().id("b"),
            Container::empty().fill().id("c"),
        ]
        .into()
    }
    for scale in [1.0, 2.625, 3.0] {
        let harness = Harness::with(
            thirds,
            FrameInfo::new(SurfaceSize::new(1000, 500)).with_scale(scale),
        );
        let (a, b, c) = (
            harness.px_rect("a"),
            harness.px_rect("b"),
            harness.px_rect("c"),
        );
        assert_eq!(a.x, 0.0);
        assert_eq!(a.x + a.width, b.x, "scale {scale}");
        assert_eq!(b.x + b.width, c.x, "scale {scale}");
        assert_eq!(c.x + c.width, 1000.0, "scale {scale}");
        assert_eq!(a.height, 500.0);
    }
}

#[test]
fn the_root_always_fills_the_screen() {
    fn hugging_root(_: &Model) -> Element<Msg> {
        col![Container::empty().size(10.0, 10.0)].id("root").into()
    }
    let harness = Harness::new(hugging_root);
    assert_eq!(harness.px_rect("root"), Rect::new(0.0, 0.0, 1080.0, 2400.0));
}

#[test]
fn safe_area_keeps_content_inside_the_insets() {
    fn screen(_: &Model) -> Element<Msg> {
        SafeArea::new(Container::empty().fill().id("content"))
            .id("screen")
            .into()
    }
    let harness = Harness::with(screen, phone_with_bars());
    // The screen itself is edge to edge; its content is inset.
    assert_eq!(
        harness.px_rect("screen"),
        Rect::new(0.0, 0.0, 1080.0, 2400.0)
    );
    assert_eq!(
        harness.px_rect("content"),
        phone_with_bars().safe_area(),
        "content must cover exactly the safe area"
    );

    fn bottom_bar(_: &Model) -> Element<Msg> {
        col![
            Spacer::new(),
            row![Container::empty().fill_width().height(56.0).id("tabs")]
                .fill_width()
                .safe_area(SafeEdges::BOTTOM)
                .id("bar"),
        ]
        .into()
    }
    let harness = Harness::with(bottom_bar, phone_with_bars());
    // The bar's background reaches the bottom edge; its content stops at
    // the gesture bar (60 px = 20 dp).
    assert_eq!(harness.dp_rect("bar"), Rect::new(0.0, 724.0, 360.0, 76.0));
    assert_eq!(harness.dp_rect("tabs"), Rect::new(0.0, 724.0, 360.0, 56.0));
}

#[test]
fn spacers_follow_the_axis_they_are_in() {
    fn bar(_: &Model) -> Element<Msg> {
        col![
            row![
                Container::empty().size(40.0, 20.0).id("left"),
                Spacer::new(),
                Container::empty().size(40.0, 20.0).id("right"),
            ]
            .fill_width()
            .id("row"),
            Spacer::fixed(30.0),
            row![
                Container::empty().size(40.0, 20.0),
                Spacer::fixed(100.0),
                Container::empty().size(40.0, 20.0).id("after"),
            ]
            .id("second"),
        ]
        .into()
    }
    let harness = Harness::new(bar);
    assert_eq!(harness.dp_rect("right").x, 320.0);
    // A fixed spacer in a row is wide, not tall: the row stays 20 high.
    assert_eq!(harness.dp_rect("row").height, 20.0);
    assert_eq!(harness.dp_rect("second"), Rect::new(0.0, 50.0, 180.0, 20.0));
    assert_eq!(harness.dp_rect("after").x, 140.0);
}

#[test]
fn stack_children_are_pinned_to_anchors() {
    fn badge(_: &Model) -> Element<Msg> {
        col![
            stack![
                Container::empty().size(48.0, 48.0),
                Container::empty()
                    .size(12.0, 12.0)
                    .place(Alignment::TOP_RIGHT)
                    .offset(4.0, -4.0)
                    .id("badge"),
                Container::empty()
                    .size(10.0, 10.0)
                    .place(Alignment::CENTER)
                    .id("dot"),
            ]
            .id("stack"),
        ]
        .padding(20.0)
        .into()
    }
    let harness = Harness::new(badge);
    assert_eq!(harness.dp_rect("stack"), Rect::new(20.0, 20.0, 48.0, 48.0));
    assert_eq!(harness.dp_rect("badge"), Rect::new(60.0, 16.0, 12.0, 12.0));
    assert_eq!(harness.dp_rect("dot"), Rect::new(39.0, 39.0, 10.0, 10.0));
}

#[test]
fn overflows_reports_boxes_that_are_too_small() {
    fn fits(_: &Model) -> Element<Msg> {
        row![Container::empty().size(100.0, 10.0)].into()
    }
    assert!(Harness::new(fits).ui.overflows().is_empty());

    fn too_wide(_: &Model) -> Element<Msg> {
        col![
            row![
                Container::empty().size(300.0, 10.0),
                Container::empty().size(300.0, 10.0),
            ]
            .width(360.0),
        ]
        .into()
    }
    let overflows = Harness::new(too_wide).ui.overflows();
    assert_eq!(overflows.len(), 1);
    assert_eq!(overflows[0].rect, Rect::new(0.0, 0.0, 360.0, 10.0));
    assert_eq!(overflows[0].amount.width, 240.0);
}

// ------------------------------------------------------------------ text

#[test]
fn text_has_the_same_dp_size_at_every_scale() {
    fn label(_: &Model) -> Element<Msg> {
        col![text("Tuma pesa kwa haraka").size(16.0).id("label")]
            .padding(16.0)
            .into()
    }
    let reference = Harness::with(label, phone().with_scale(1.0)).dp_rect("label");
    assert!(reference.width > 100.0 && reference.width < 220.0);
    // 22.4 dp, with its edges snapped to whole pixels.
    assert!(close(reference.height, 16.0 * 1.4, 0.5), "{reference:?}");
    for scale in [1.5, 2.0, 2.625, 3.0, 4.0] {
        let rect = Harness::with(label, phone().with_scale(scale)).dp_rect("label");
        // Hinting and pixel snapping move the width by at most a dp or so.
        assert!(close(rect.width, reference.width, 2.0), "{scale}: {rect:?}");
        assert!(
            close(rect.height, reference.height, 0.75),
            "{scale}: {rect:?}"
        );
        assert_eq!(rect.x, 16.0);
    }
}

/// ADR 0010 records an unchecked assumption: a text that reports natural
/// width `w` reports the same size under any constraint of at least `w`.
/// This checks it against the real text engine.
#[test]
fn text_measured_at_its_natural_width_does_not_wrap() {
    let mut probe = Ui::new(common::Probe {
        view: |_| Container::empty().into(),
        model: Model::default(),
        log: Vec::new(),
        theme: Theme::LIGHT,
    });
    let mut frame = Frame::for_app(&mut probe);
    frame.begin();

    let samples = [
        "Send money",
        "Tuma pesa kwa haraka na usalama",
        "Karibu tena, Amina! Salio lako ni TSh 1,250,000",
        "The quick brown fox jumps over the lazy dog, twice.",
        "مرحبا بكم في مورنينغ ستار",
        "Balance: TSh 12,500.00 — updated 09:41",
        "a b c d e f g h i j k l m n o p",
    ];
    let mut checked = 0;
    for text in samples {
        for tenths in 90..=400 {
            // Sizes as a UI at fractional scales produces them.
            let size = tenths as f32 / 10.0 * 1.3125;
            let style = TextStyle::new(size, Color::BLACK);
            let natural = frame.measure_text(text, &style, None);
            assert!(natural.width > 0.0, "fonts are loaded");
            for constraint in [natural.width, natural.width.ceil(), natural.width + 1.0] {
                let wrapped = frame.measure_text(text, &style, Some(constraint));
                assert_eq!(
                    wrapped.height, natural.height,
                    "{text:?} at {size} px wrapped at its own natural width \
                     ({} px, constraint {constraint})",
                    natural.width
                );
                checked += 1;
            }
        }
        // Start the next sample with an empty layout cache.
        frame.begin();
    }
    assert!(checked > 5000);
}

#[test]
fn a_padded_box_around_a_label_never_wraps_it() {
    // Padding and scale chosen so that dp arithmetic does not round-trip
    // exactly; the label must still be one line high.
    fn chip(model: &Model) -> Element<Msg> {
        let padding = 3.3 + model.count as f32 * 0.37;
        col![
            Container::new(text("Tuma pesa sasa hivi").size(15.0).id("label"))
                .padding(padding)
                .id("chip"),
        ]
        .padding(7.7)
        .into()
    }
    for scale in [1.0, 1.75, 2.625, 3.0] {
        let mut harness = Harness::with(chip, phone().with_scale(scale));
        for _ in 0..40 {
            let label = harness.dp_rect("label");
            assert!(
                close(label.height, 15.0 * 1.4, 0.75),
                "scale {scale}, count {}: label wrapped: {label:?}",
                harness.ui.app().model.count
            );
            harness.ui.send(Msg::Tap(0));
            harness.draw();
        }
    }
}

#[test]
fn fill_width_text_wraps_and_pushes_siblings_down() {
    fn paragraph(_: &Model) -> Element<Msg> {
        col![
            text("Karibu tena. Hii ni sentensi ndefu ambayo lazima ivunjwe katika mistari kadhaa ili itoshee.")
                .fill_width()
                .id("paragraph"),
            Container::empty().size(10.0, 10.0).id("below"),
        ]
        .padding(16.0)
        .width(200.0)
        .into()
    }
    fn paragraph_screen(model: &Model) -> Element<Msg> {
        col![paragraph(model)].into()
    }
    let harness = Harness::new(paragraph_screen);
    let paragraph = harness.dp_rect("paragraph");
    assert_eq!(paragraph.width, 168.0);
    let line = 15.0 * 1.4;
    let lines = (paragraph.height / line).round();
    assert!(lines >= 3.0, "{paragraph:?}");
    assert!(close(paragraph.height, lines * line, 0.5));
    assert!(close(
        harness.dp_rect("below").y,
        16.0 + paragraph.height,
        0.5
    ));
    assert!(harness.glyph_count() > 60);
}

#[test]
fn max_lines_cuts_the_box_and_clips_the_text() {
    fn clipped(_: &Model) -> Element<Msg> {
        col![
            text("Moja mbili tatu nne tano sita saba nane tisa kumi kumi na moja kumi na mbili")
                .fill_width()
                .max_lines(2)
                .id("text"),
        ]
        .width(120.0)
        .into()
    }
    fn clipped_screen(model: &Model) -> Element<Msg> {
        col![clipped(model)].into()
    }
    let harness = Harness::new(clipped_screen);
    let rect = harness.dp_rect("text");
    assert!(close(rect.height, 2.0 * 15.0 * 1.4, 0.5), "{rect:?}");
    let clip = harness.ops().iter().find_map(|op| match *op {
        DrawOp::PushClip { rect, .. } => Some(rect),
        _ => None,
    });
    assert_eq!(clip, Some(harness.px_rect("text")));
}

#[test]
fn centred_text_is_centred_in_its_box() {
    fn centred(_: &Model) -> Element<Msg> {
        col![text("Sawa").fill_width().centered().id("text")].into()
    }
    let harness = Harness::new(centred);
    let glyphs: Vec<Rect> = harness
        .ops()
        .iter()
        .filter_map(|op| match *op {
            DrawOp::Glyph { rect, .. } => Some(rect),
            _ => None,
        })
        .collect();
    let left = glyphs.iter().map(|g| g.x).fold(f32::MAX, f32::min);
    let right = glyphs.iter().map(|g| g.x + g.width).fold(0.0, f32::max);
    assert!(close((left + right) / 2.0, 540.0, 6.0), "{left} {right}");
}

// ---------------------------------------------------------------- images

#[test]
fn a_picture_is_sized_by_its_image_unless_told_otherwise() {
    fn pictures(model: &Model) -> Element<Msg> {
        let image = model.image.expect("registered in setup");
        col![
            Picture::new(image).id("natural"),
            Picture::new(image).width(100.0).id("wide"),
            Picture::new(image).height(30.0).id("tall"),
            Picture::new(image).fill_width().id("fill"),
            Picture::new(image).size(60.0, 60.0).id("square"),
        ]
        .width(200.0)
        .id("column")
        .into()
    }
    fn screen(model: &Model) -> Element<Msg> {
        col![pictures(model)].into()
    }
    let harness = Harness::new(screen);
    // One dp per image pixel; then the 2:1 shape follows a given side.
    assert_eq!(harness.dp_rect("natural"), Rect::new(0.0, 0.0, 4.0, 2.0));
    assert_eq!(harness.dp_rect("wide"), Rect::new(0.0, 2.0, 100.0, 50.0));
    assert_eq!(harness.dp_rect("tall"), Rect::new(0.0, 52.0, 60.0, 30.0));
    assert_eq!(harness.dp_rect("fill"), Rect::new(0.0, 82.0, 200.0, 100.0));
    assert_eq!(harness.dp_rect("square"), Rect::new(0.0, 182.0, 60.0, 60.0));
}

#[test]
fn a_picture_emits_one_fitted_image_operation() {
    fn picture(model: &Model) -> Element<Msg> {
        let image = model.image.expect("registered in setup");
        col![
            Picture::new(image)
                .size(60.0, 60.0)
                .radius(30.0)
                .tint(ThemeColor::Primary)
                .id("cover"),
            Picture::new(image)
                .size(60.0, 60.0)
                .fit(ImageFit::Contain)
                .id("contain"),
            Avatar::new("Amina Juma")
                .image(image)
                .size(48.0)
                .id("avatar"),
            Avatar::new("Amina Juma").image_maybe(None).id("initials"),
        ]
        .into()
    }
    let harness = Harness::new(picture);
    let image = harness.ui.app().model.image.unwrap();
    let images: Vec<_> = harness
        .ops()
        .iter()
        .filter_map(|op| match *op {
            DrawOp::Image {
                image,
                source,
                rect,
                corner_radius,
                tint,
            } => Some((image, source, rect, corner_radius, tint)),
            _ => None,
        })
        .collect();
    assert_eq!(images.len(), 3, "two pictures and one avatar");

    // Cover: the square shows the middle 2 x 2 of the 4 x 2 image.
    let (drawn, source, rect, radius, tint) = images[0];
    assert_eq!(drawn, image);
    assert_eq!(source, Rect::new(1.0, 0.0, 2.0, 2.0));
    assert_eq!(rect, harness.px_rect("cover"));
    assert_eq!(rect, Rect::new(0.0, 0.0, 180.0, 180.0));
    assert_eq!(radius, 90.0);
    assert_eq!(tint, Theme::LIGHT.colors.primary);

    // Contain: the whole image, letterboxed to 60 x 30 dp in the middle.
    let (_, source, rect, radius, tint) = images[1];
    assert_eq!(source, Rect::new(0.0, 0.0, 4.0, 2.0));
    assert_eq!(rect, Rect::new(0.0, 225.0, 180.0, 90.0));
    assert_eq!((radius, tint), (0.0, Color::WHITE));

    // An avatar with an image is that image as a circle; without, initials.
    let (_, _, rect, radius, _) = images[2];
    assert_eq!(rect, harness.px_rect("avatar"));
    assert_eq!((rect.width, radius), (144.0, 72.0));
    assert!(harness.glyph_count() >= 2, "the fallback draws initials");
    assert_eq!(harness.dp_rect("initials").width, 40.0);
}

// --------------------------------------------------------- taps and input

fn two_buttons(_: &Model) -> Element<Msg> {
    col![
        Button::new("One")
            .on_press(Msg::Tap(1))
            .size(200.0, 50.0)
            .id("one"),
        Button::new("Two")
            .on_press(Msg::Tap(2))
            .size(200.0, 50.0)
            .id("two"),
    ]
    .gap(20.0)
    .into()
}

#[test]
fn a_tap_delivers_the_widgets_message() {
    let mut harness = Harness::new(two_buttons);
    harness.tap(100.0, 25.0);
    assert_eq!(harness.log(), [Msg::Tap(1)]);
    harness.tap(100.0, 95.0);
    assert_eq!(harness.log(), [Msg::Tap(1), Msg::Tap(2)]);
    // The gap between them and the empty space around hit nothing.
    harness.tap(100.0, 60.0);
    harness.tap(300.0, 25.0);
    harness.tap(100.0, 400.0);
    assert_eq!(harness.log().len(), 2);
}

#[test]
fn hit_testing_uses_half_open_pixel_rectangles() {
    let mut harness = Harness::new(two_buttons);
    // The button covers x in 0..200 dp, y in 0..50 dp.
    harness.tap(199.9, 49.9);
    assert_eq!(harness.log().len(), 1);
    harness.tap(200.0, 25.0);
    harness.tap(100.0, 50.0);
    assert_eq!(harness.log().len(), 1);
}

#[test]
fn sliding_off_cancels_and_sliding_back_rearms() {
    let mut harness = Harness::new(two_buttons);
    harness.down(100.0, 25.0);
    harness.moved(300.0, 25.0);
    harness.up(300.0, 25.0);
    assert!(harness.log().is_empty(), "lifted off the button");

    // Sliding onto another button does not press that one.
    harness.draw();
    harness.down(100.0, 25.0);
    harness.moved(100.0, 95.0);
    harness.up(100.0, 95.0);
    assert!(harness.log().is_empty(), "lifted on a different button");

    harness.draw();
    harness.down(100.0, 25.0);
    harness.moved(300.0, 25.0);
    harness.draw();
    harness.moved(110.0, 30.0);
    harness.up(110.0, 30.0);
    assert_eq!(harness.log(), [Msg::Tap(1)], "came back before lifting");
}

#[test]
fn a_cancelled_touch_does_nothing() {
    let mut harness = Harness::new(two_buttons);
    harness.down(100.0, 25.0);
    harness.touch(1, PointerPhase::Cancelled, 100.0, 25.0);
    harness.up(100.0, 25.0);
    assert!(harness.log().is_empty());
    // And the next tap works normally.
    harness.draw();
    harness.tap(100.0, 25.0);
    assert_eq!(harness.log(), [Msg::Tap(1)]);
}

#[test]
fn a_second_finger_neither_double_fires_nor_steals_the_press() {
    let mut harness = Harness::new(two_buttons);
    harness.touch(1, PointerPhase::Down, 100.0, 25.0);
    // A second finger lands on the same button, and on the other one.
    harness.touch(2, PointerPhase::Down, 120.0, 25.0);
    harness.touch(3, PointerPhase::Down, 100.0, 95.0);
    harness.touch(2, PointerPhase::Up, 120.0, 25.0);
    harness.touch(3, PointerPhase::Up, 100.0, 95.0);
    assert!(harness.log().is_empty(), "only the first finger can tap");
    harness.touch(2, PointerPhase::Cancelled, 0.0, 0.0);
    harness.touch(1, PointerPhase::Up, 100.0, 25.0);
    assert_eq!(harness.log(), [Msg::Tap(1)]);

    // A stray up or move with no press in progress is ignored.
    harness.draw();
    harness.touch(7, PointerPhase::Up, 100.0, 25.0);
    harness.touch(7, PointerPhase::Moved, 100.0, 25.0);
    assert_eq!(harness.log().len(), 1);
}

#[test]
fn messages_arrive_in_the_order_the_taps_completed() {
    let mut harness = Harness::new(two_buttons);
    harness.tap(100.0, 95.0);
    harness.tap(100.0, 25.0);
    harness.tap(100.0, 95.0);
    assert_eq!(harness.log(), [Msg::Tap(2), Msg::Tap(1), Msg::Tap(2)]);
}

#[test]
fn a_screen_that_has_not_been_drawn_cannot_be_tapped() {
    let mut harness = Harness::new(two_buttons);
    // Two taps arrive before the engine builds the frame in between: the
    // first may have changed the screen, so the second is dropped.
    harness.down(100.0, 25.0);
    harness.up(100.0, 25.0);
    harness.down(100.0, 95.0);
    harness.up(100.0, 95.0);
    assert_eq!(harness.log(), [Msg::Tap(1)]);
    harness.draw();
    harness.tap(100.0, 95.0);
    assert_eq!(harness.log(), [Msg::Tap(1), Msg::Tap(2)]);
}

#[test]
fn a_pressed_widget_is_drawn_pressed_only_while_held() {
    let mut harness = Harness::new(two_buttons);
    let overlay = Theme::LIGHT.colors.pressed_overlay;
    assert!(harness.rects_with_color(overlay).is_empty());

    harness.down(100.0, 25.0);
    assert!(!harness.draw(), "a press is not an animation");
    assert_eq!(
        harness.rects_with_color(overlay),
        [harness.px_rect("one")],
        "the held button, and only it, has the overlay"
    );

    // Sliding off releases the visual.
    harness.moved(300.0, 300.0);
    harness.draw();
    assert!(harness.rects_with_color(overlay).is_empty());
    harness.moved(100.0, 25.0);
    harness.draw();
    assert_eq!(harness.rects_with_color(overlay).len(), 1);

    harness.up(100.0, 25.0);
    harness.draw();
    assert!(harness.rects_with_color(overlay).is_empty());
}

#[test]
fn a_custom_pressed_background_replaces_the_fill() {
    fn card(_: &Model) -> Element<Msg> {
        Pressable::new(Container::empty().size(100.0, 100.0))
            .background(hex(0x112233))
            .pressed_background(hex(0x445566))
            .on_press(Msg::Tap(9))
            .into()
    }
    let mut harness = Harness::new(card);
    assert_eq!(harness.rects_with_color(hex(0x112233)).len(), 1);
    harness.down(50.0, 50.0);
    harness.draw();
    assert!(harness.rects_with_color(hex(0x112233)).is_empty());
    assert_eq!(harness.rects_with_color(hex(0x445566)).len(), 1);
    assert!(
        harness
            .rects_with_color(Theme::LIGHT.colors.pressed_overlay)
            .is_empty()
    );
}

#[test]
fn the_innermost_pressable_wins_and_blockers_swallow_touches() {
    fn nested(model: &Model) -> Element<Msg> {
        stack![
            Pressable::new(
                col![Button::new("Inner").on_press(Msg::Tap(2)).size(100.0, 50.0)].padding(50.0),
            )
            .on_press(Msg::Tap(1)),
        ]
        .push_maybe(model.flag.then(|| {
            Container::empty()
                .size(200.0, 75.0)
                .background(hex(0x000000))
                .block_touches()
        }))
        .into()
    }
    let mut harness = Harness::new(nested);
    harness.tap(100.0, 75.0);
    harness.tap(10.0, 10.0);
    assert_eq!(harness.log(), [Msg::Tap(2), Msg::Tap(1)]);

    // With a blocking sheet over the top half, only what it leaves
    // uncovered reacts.
    harness.ui.send(Msg::Toggle);
    harness.draw();
    harness.tap(100.0, 60.0);
    harness.tap(10.0, 10.0);
    assert_eq!(harness.log().len(), 3, "covered: {:?}", harness.log());
    harness.tap(100.0, 90.0);
    assert_eq!(harness.log().last(), Some(&Msg::Tap(2)));
}

#[test]
fn a_disabled_button_ignores_touches() {
    fn disabled(_: &Model) -> Element<Msg> {
        col![Button::<Msg>::new("Later").size(200.0, 50.0)].into()
    }
    let mut harness = Harness::new(disabled);
    harness.tap(100.0, 25.0);
    assert!(harness.log().is_empty());
    // It is drawn in the quiet surface colour, not the primary.
    assert_eq!(
        harness
            .rects_with_color(Theme::LIGHT.colors.surface_variant)
            .len(),
        1
    );
    assert!(
        harness
            .rects_with_color(Theme::LIGHT.colors.primary)
            .is_empty()
    );
}

#[test]
fn a_press_ends_if_its_widget_disappears() {
    fn vanishing(model: &Model) -> Element<Msg> {
        col![]
            .push_maybe((!model.flag).then(|| {
                Button::new("Go")
                    .on_press(Msg::Tap(1))
                    .size(200.0, 50.0)
                    .id("go")
            }))
            .push(
                Button::new("Other")
                    .on_press(Msg::Tap(2))
                    .size(200.0, 50.0)
                    .id("other"),
            )
            .into()
    }
    let mut harness = Harness::new(vanishing);
    harness.down(100.0, 25.0);
    harness.draw();
    // The state changes under the finger (a timer, a network reply).
    harness.ui.app_mut().model.flag = true;
    harness.draw();
    // "Other" is now where the finger is, but it was never pressed.
    harness.up(100.0, 25.0);
    assert!(harness.log().is_empty());
}

#[test]
fn map_converts_the_messages_of_a_subtree() {
    fn child() -> Element<u32> {
        Button::new("Child").on_press(7).size(100.0, 50.0).into()
    }
    fn parent(_: &Model) -> Element<Msg> {
        col![child().map(Msg::Tap)].into()
    }
    let mut harness = Harness::new(parent);
    harness.tap(50.0, 25.0);
    assert_eq!(harness.log(), [Msg::Tap(7)]);
}

// ------------------------------------------------------------- scrolling

/// A 360 x 800 dp screen: a 100 dp header, then a list of 30 rows of 80 dp
/// (2400 dp of content in a 700 dp viewport: 1700 dp of travel).
fn list(_: &Model) -> Element<Msg> {
    col![
        Container::empty().fill_width().height(100.0).id("header"),
        ScrollView::new(Column::new().extend((0..30).map(|index| {
            Pressable::new(text(format!("Row {index}")))
                .fill_width()
                .height(80.0)
                .on_press(Msg::Tap(index))
                .id(("row", index))
        })))
        .id("list"),
    ]
    .into()
}

fn offset(harness: &Harness) -> f32 {
    harness
        .ui
        .scroll_offset("list")
        .expect("the list is on screen")
}

/// Drags from `from` to `to` (dp, at x = 180) and leaves the finger down.
/// The first 10 dp cross the touch slop and scroll nothing; the rest is
/// covered in `steps` moves with a frame after each, as the engine would.
fn drag(harness: &mut Harness, from: f32, to: f32, steps: u32, frame_time: f32) {
    harness.down(180.0, from);
    harness.step(frame_time);
    let from = from + 10.0_f32.copysign(to - from);
    harness.moved(180.0, from);
    for step in 1..=steps {
        let y = from + (to - from) * step as f32 / steps as f32;
        harness.moved(180.0, y);
        harness.step(frame_time);
    }
}

#[test]
fn dragging_scrolls_and_clamps_at_both_ends() {
    let mut harness = Harness::new(list);
    assert_eq!(offset(&harness), 0.0);
    assert_eq!(harness.ui.max_scroll_offset("list"), Some(1700.0));

    // The first 8 dp are slop; from there the content follows the finger.
    harness.down(180.0, 500.0);
    harness.moved(180.0, 490.0);
    assert_eq!(offset(&harness), 0.0);
    harness.moved(180.0, 390.0);
    assert_eq!(offset(&harness), 100.0);
    harness.draw();
    let row = harness.dp_rect_of_row(3);
    assert_eq!(row, Rect::new(0.0, 100.0 + 240.0 - 100.0, 360.0, 80.0));

    // Dragging down past the top stops at zero.
    harness.moved(180.0, 790.0);
    assert_eq!(offset(&harness), 0.0);
    harness.up(180.0, 790.0);
    harness.step(1.0);

    // Dragging far up stops at the end of the content.
    for _ in 0..4 {
        harness.down(180.0, 780.0);
        harness.moved(180.0, 700.0);
        harness.moved(180.0, 110.0);
        harness.touch(1, PointerPhase::Cancelled, 180.0, 110.0);
        harness.draw();
    }
    assert_eq!(offset(&harness), 1700.0);
    harness.draw();
    // The last row ends exactly at the bottom of the screen.
    assert_eq!(
        harness.dp_rect_of_row(29),
        Rect::new(0.0, 720.0, 360.0, 80.0)
    );
}

trait RowRect {
    fn dp_rect_of_row(&self, index: u32) -> Rect;
}

impl RowRect for Harness {
    fn dp_rect_of_row(&self, index: u32) -> Rect {
        let rect = self.ui.rect_of(("row", index)).expect("row is painted");
        Rect::new(
            rect.x / 3.0,
            rect.y / 3.0,
            rect.width / 3.0,
            rect.height / 3.0,
        )
    }
}

#[test]
fn a_scroll_view_clips_and_skips_what_is_off_screen() {
    let mut harness = Harness::new(list);
    let viewport = Rect::new(0.0, 300.0, 1080.0, 2100.0);
    let clips: Vec<Rect> = harness
        .ops()
        .iter()
        .filter_map(|op| match *op {
            DrawOp::PushClip { rect, .. } => Some(rect),
            _ => None,
        })
        .collect();
    assert_eq!(clips, [viewport]);
    let pops = harness
        .ops()
        .iter()
        .filter(|op| matches!(op, DrawOp::PopClip))
        .count();
    assert_eq!(pops, 1);

    // Rows far below the viewport are not painted at all, so a long list
    // costs what is visible.
    assert!(harness.ui.rect_of(("row", 0)).is_some());
    assert!(harness.ui.rect_of(("row", 8)).is_some());
    assert!(harness.ui.rect_of(("row", 20)).is_none());
    let glyphs_at_top = harness.glyph_count();
    assert!(glyphs_at_top < 12 * 6, "{glyphs_at_top} glyphs for 9 rows");

    harness.down(180.0, 700.0);
    harness.moved(180.0, 600.0);
    harness.moved(180.0, 0.0);
    harness.up(180.0, 0.0);
    harness.step(1.0);
    assert!(harness.ui.rect_of(("row", 0)).is_none());
    assert!(harness.ui.rect_of(("row", 10)).is_some());
}

#[test]
fn taps_follow_the_scrolled_content_and_respect_the_clip() {
    let mut harness = Harness::new(list);
    harness.tap(180.0, 140.0);
    assert_eq!(harness.log(), [Msg::Tap(0)]);

    // Scroll by 160 dp (two rows), slowly enough that nothing flings.
    drag(&mut harness, 600.0, 430.0, 4, 0.5);
    harness.up(180.0, 430.0);
    harness.step(0.5);
    assert_eq!(offset(&harness), 160.0);
    assert_eq!(harness.log().len(), 1, "a drag is not a tap");

    // Row 2 now starts at the top of the viewport (y = 100).
    harness.tap(180.0, 140.0);
    assert_eq!(harness.log().last(), Some(&Msg::Tap(2)));

    // Rows scrolled under the header cannot be tapped through it.
    harness.tap(180.0, 60.0);
    assert_eq!(harness.log().len(), 2);
}

#[test]
fn a_flick_keeps_scrolling_then_settles_and_goes_idle() {
    let mut harness = Harness::new(list);
    assert!(!harness.draw(), "an untouched screen is idle");

    // 300 dp in five 16 ms frames: about 3750 dp per second.
    drag(&mut harness, 700.0, 400.0, 5, 0.016);
    let at_release = offset(&harness);
    assert!(close(at_release, 290.0, 0.01), "{at_release}");
    harness.up(180.0, 400.0);

    let mut previous = at_release;
    let mut frames = 0;
    while harness.step(0.016) {
        let now = offset(&harness);
        assert!(now >= previous, "a fling never goes backwards");
        previous = now;
        frames += 1;
        assert!(frames < 600, "the fling must end");
    }
    let settled = offset(&harness);
    assert!(frames > 20, "only {frames} frames of fling");
    assert!(
        settled > at_release + 300.0,
        "the flick carried {} dp",
        settled - at_release
    );
    assert!(settled <= 1700.0);

    // Settled means settled: no more frames are requested, and nothing
    // moves if one is drawn anyway.
    assert!(!harness.step(0.016));
    assert!(!harness.step(5.0));
    assert_eq!(offset(&harness), settled);
}

#[test]
fn a_fling_is_the_same_distance_at_any_frame_rate() {
    let mut distances = Vec::new();
    for frame_time in [1.0 / 120.0, 1.0 / 60.0, 1.0 / 30.0] {
        let mut harness = Harness::new(list);
        drag(&mut harness, 700.0, 600.0, 4, 0.016);
        harness.up(180.0, 600.0);
        let mut frames = 0;
        while harness.step(frame_time) {
            frames += 1;
            assert!(frames < 2000);
        }
        distances.push(offset(&harness));
    }
    assert!(distances[0] > 150.0);
    // The decay is integrated exactly; only the stopping threshold is
    // reached at slightly different moments.
    assert!(close(distances[0], distances[1], 6.0), "{distances:?}");
    assert!(close(distances[1], distances[2], 6.0), "{distances:?}");
}

#[test]
fn a_fling_stops_at_the_end_of_the_content() {
    let mut harness = Harness::new(list);
    // Start near the end.
    for _ in 0..3 {
        harness.down(180.0, 780.0);
        harness.moved(180.0, 700.0);
        harness.moved(180.0, 150.0);
        harness.touch(1, PointerPhase::Cancelled, 180.0, 150.0);
        harness.step(1.0);
    }
    assert!(offset(&harness) > 1500.0);
    drag(&mut harness, 700.0, 500.0, 3, 0.016);
    harness.up(180.0, 500.0);
    let mut frames = 0;
    while harness.step(0.016) {
        frames += 1;
        assert!(frames < 600);
    }
    assert_eq!(offset(&harness), 1700.0);
    assert!(frames < 30, "hitting the end stops the animation");
}

#[test]
fn resting_before_lifting_does_not_fling() {
    let mut harness = Harness::new(list);
    drag(&mut harness, 700.0, 400.0, 5, 0.016);
    // The finger stays put for half a second; no events, so no frames.
    harness.time += 0.5;
    harness.up(180.0, 400.0);
    assert!(!harness.draw(), "no fling");
    assert_eq!(offset(&harness), 290.0);
}

#[test]
fn touching_a_flinging_list_stops_it_without_tapping() {
    let mut harness = Harness::new(list);
    drag(&mut harness, 700.0, 400.0, 5, 0.016);
    harness.up(180.0, 400.0);
    assert!(harness.step(0.016));
    assert!(harness.step(0.016));

    harness.down(180.0, 300.0);
    assert!(!harness.step(0.016), "the touch stopped the fling");
    let stopped = offset(&harness);
    harness.up(180.0, 300.0);
    assert!(!harness.step(0.016));
    assert_eq!(offset(&harness), stopped);
    assert!(harness.log().is_empty(), "stopping a fling is not a tap");

    // The next touch is an ordinary tap again.
    harness.tap(180.0, 300.0);
    assert_eq!(harness.log().len(), 1);
}

#[test]
fn content_that_fits_does_not_scroll_and_still_taps() {
    fn short(_: &Model) -> Element<Msg> {
        ScrollView::new(col![
            Button::new("Only")
                .on_press(Msg::Tap(1))
                .fill_width()
                .height(100.0),
        ])
        .id("list")
        .into()
    }
    let mut harness = Harness::new(short);
    assert_eq!(harness.ui.max_scroll_offset("list"), Some(0.0));
    // A sloppy tap that moves 30 dp but stays on the button still counts,
    // because there is nothing to scroll.
    harness.down(180.0, 30.0);
    harness.moved(180.0, 60.0);
    harness.up(180.0, 60.0);
    assert_eq!(harness.log(), [Msg::Tap(1)]);
    assert_eq!(offset(&harness), 0.0);
}

#[test]
fn horizontal_movement_does_not_start_a_scroll() {
    let mut harness = Harness::new(list);
    harness.down(100.0, 300.0);
    harness.moved(200.0, 310.0);
    harness.up(200.0, 310.0);
    assert_eq!(offset(&harness), 0.0);
    // The finger stayed on the row it started on, so it is a tap.
    assert_eq!(harness.log(), [Msg::Tap(2)]);
}

// ------------------------------------------------- state across rebuilds

/// Two screens, each a scroll view at the same place in the tree.
fn two_screens(model: &Model) -> Element<Msg> {
    let rows = |tint: u32| {
        Column::new().extend((0..20).map(move |_| {
            Container::<Msg>::empty()
                .fill_width()
                .height(100.0)
                .background(hex(tint))
        }))
    };
    let screen = if model.flag {
        ScrollView::new(rows(0x101010)).id("second")
    } else {
        ScrollView::new(rows(0x202020)).id("first")
    };
    col![
        Button::new("Switch")
            .on_press(Msg::Toggle)
            .fill_width()
            .height(50.0),
        Button::new("Noop")
            .on_press(Msg::Tap(0))
            .fill_width()
            .height(50.0),
        screen,
    ]
    .into()
}

fn scroll_by(harness: &mut Harness, dp: f32) {
    harness.down(180.0, 700.0);
    harness.moved(180.0, 690.0);
    harness.moved(180.0, 690.0 - dp);
    harness.touch(1, PointerPhase::Cancelled, 0.0, 0.0);
    harness.draw();
}

#[test]
fn scroll_position_survives_rebuilds() {
    let mut harness = Harness::new(two_screens);
    scroll_by(&mut harness, 250.0);
    assert_eq!(harness.ui.scroll_offset("first"), Some(250.0));

    // Many rebuilds, including one caused by a state change.
    harness.tap(180.0, 75.0);
    assert_eq!(harness.log(), [Msg::Tap(0)]);
    for _ in 0..5 {
        harness.step(0.1);
    }
    assert_eq!(harness.ui.scroll_offset("first"), Some(250.0));
}

#[test]
fn widget_state_belongs_to_an_identity_and_ends_when_it_leaves() {
    let mut harness = Harness::new(two_screens);
    scroll_by(&mut harness, 250.0);

    // The other screen has its own id, so it starts at the top even though
    // its scroll view sits at the same position in the tree.
    harness.tap(180.0, 25.0);
    assert_eq!(harness.ui.scroll_offset("second"), Some(0.0));
    assert_eq!(harness.ui.scroll_offset("first"), None);

    // Coming back, the first screen starts again: its state ended when it
    // left the tree.
    scroll_by(&mut harness, 100.0);
    harness.tap(180.0, 25.0);
    assert_eq!(harness.ui.scroll_offset("first"), Some(0.0));
}

#[test]
fn without_ids_identity_is_position_in_the_tree() {
    // The documented consequence of the positional rule: two different
    // scroll views that occupy the same position share their state.
    fn positional(model: &Model) -> Element<Msg> {
        let tint = if model.flag { 0x101010 } else { 0x202020 };
        col![
            Button::new("Switch")
                .on_press(Msg::Toggle)
                .fill_width()
                .height(50.0),
            ScrollView::new(Column::new().extend((0..20).map(|index| {
                Container::<Msg>::empty()
                    .fill_width()
                    .height(100.0)
                    .background(hex(tint))
                    .id(("probe", index))
            }))),
        ]
        .into()
    }
    let mut harness = Harness::new(positional);
    let before = harness.ui.rect_of(("probe", 5)).unwrap();
    harness.down(180.0, 700.0);
    harness.moved(180.0, 690.0);
    harness.moved(180.0, 440.0);
    harness.touch(1, PointerPhase::Cancelled, 0.0, 0.0);
    harness.draw();
    let scrolled = harness.ui.rect_of(("probe", 5)).unwrap();
    assert_eq!(scrolled.y, before.y - 750.0);

    harness.tap(180.0, 25.0);
    assert!(harness.ui.app().model.flag);
    assert_eq!(harness.ui.rect_of(("probe", 5)).unwrap(), scrolled);
}

// ------------------------------------------------------ redraw and theme

#[test]
fn frames_are_requested_only_while_something_moves() {
    let mut harness = Harness::new(list);
    assert!(!harness.draw());
    // Input produces a frame by itself; the UI does not ask for more.
    harness.down(180.0, 300.0);
    assert!(!harness.draw());
    harness.up(180.0, 300.0);
    assert!(!harness.draw());
    // Dragging: each move is followed by a frame without asking.
    harness.down(180.0, 700.0);
    harness.moved(180.0, 600.0);
    assert!(!harness.step(0.016));
    harness.moved(180.0, 500.0);
    assert!(!harness.step(0.016));
    // Released while moving: now the UI must keep the frames coming.
    harness.up(180.0, 500.0);
    assert!(harness.step(0.016));
}

#[test]
fn widgets_take_their_colours_and_type_from_the_theme() {
    fn themed(_: &Model) -> Element<Msg> {
        col![
            Button::new("Lipa")
                .on_press(Msg::Tap(1))
                .size(100.0, 50.0)
                .id("button"),
            Container::empty()
                .size(10.0, 10.0)
                .background(ThemeColor::Danger),
            text("Kichwa").title().id("title"),
            text("Maelezo").caption().muted().id("caption"),
            Divider::new(),
        ]
        .into()
    }
    let mut harness = Harness::new(themed);
    let light = Theme::LIGHT;
    assert_eq!(harness.frame.clear, light.colors.background);
    assert_eq!(
        harness.rects_with_color(light.colors.primary),
        [harness.px_rect("button")]
    );
    assert_eq!(harness.rects_with_color(light.colors.danger).len(), 1);
    assert_eq!(harness.rects_with_color(light.colors.divider).len(), 1);
    // Text boxes are the line height, snapped to whole pixels (1/3 dp).
    assert!(close(harness.dp_rect("title").height, 24.0 * 1.25, 0.34));
    assert!(close(harness.dp_rect("caption").height, 12.0 * 1.35, 0.34));
    let glyph_colors = |harness: &Harness| -> Vec<Color> {
        let mut colors: Vec<Color> = Vec::new();
        for op in harness.ops() {
            if let DrawOp::Glyph { color, .. } = *op
                && !colors.contains(&color)
            {
                colors.push(color);
            }
        }
        colors
    };
    assert_eq!(
        glyph_colors(&harness),
        [
            light.colors.on_primary,
            light.colors.text,
            light.colors.text_muted
        ]
    );

    // A different theme, the same view.
    let brand = hex(0x00A651);
    let mut custom = Theme::DARK;
    custom.colors.primary = brand;
    custom.type_scale.title.size = 40.0;
    harness.ui.app_mut().theme = custom;
    harness.draw();
    assert_eq!(harness.frame.clear, custom.colors.background);
    assert_eq!(harness.rects_with_color(brand), [harness.px_rect("button")]);
    assert!(harness.rects_with_color(light.colors.primary).is_empty());
    assert_eq!(
        harness.rects_with_color(custom.colors.danger).len(),
        1,
        "tokens resolve against the current theme"
    );
    assert!(close(harness.dp_rect("title").height, 40.0 * 1.25, 0.34));
    assert_eq!(
        glyph_colors(&harness),
        [
            custom.colors.on_primary,
            custom.colors.text,
            custom.colors.text_muted
        ]
    );
}

#[test]
fn decoration_is_emitted_in_paint_order_and_in_pixels() {
    fn card(_: &Model) -> Element<Msg> {
        col![
            Container::empty()
                .size(100.0, 60.0)
                .radius(8.0)
                .gradient(Gradient::vertical(hex(0xFF0000), hex(0x0000FF)))
                .border(1.0, hex(0x00FF00))
                .shadow(Shadow::new(0.0, 4.0, 12.0, Color::rgba(0.0, 0.0, 0.0, 0.3)))
                .id("card"),
        ]
        .padding(10.0)
        .into()
    }
    let harness = Harness::new(card);
    let rect = harness.px_rect("card");
    assert_eq!(rect, Rect::new(30.0, 30.0, 300.0, 180.0));
    let ops = harness.ops();
    assert_eq!(ops.len(), 3);
    match ops[0] {
        DrawOp::Shadow {
            rect: r,
            corner_radius,
            shadow,
        } => {
            assert_eq!(r, rect);
            assert_eq!(corner_radius, 24.0);
            assert_eq!((shadow.offset_y, shadow.blur), (12.0, 36.0));
        }
        other => panic!("expected a shadow first, got {other:?}"),
    }
    match ops[1] {
        DrawOp::GradientRect {
            rect: r, gradient, ..
        } => {
            assert_eq!(r, rect);
            assert_eq!(gradient.start_color, hex(0xFF0000));
            assert!(close(gradient.start.y, 30.0, 0.01));
            assert!(close(gradient.end.y, 210.0, 0.01));
            assert!(close(gradient.start.x, gradient.end.x, 0.01));
        }
        other => panic!("expected the gradient second, got {other:?}"),
    }
    match ops[2] {
        DrawOp::BorderedRect { border, .. } => assert_eq!(border.width, 3.0),
        other => panic!("expected the border last, got {other:?}"),
    }
}

// ------------------------------------------------------------ robustness

#[test]
fn odd_input_never_panics() {
    fn odd(model: &Model) -> Element<Msg> {
        let nan = f32::NAN;
        col![
            Container::empty()
                .size(nan, -5.0)
                .radius(nan)
                .border(nan, hex(0)),
            Container::new(text("").size(nan).line_height(-1.0))
                .padding(nan)
                .gap(f32::INFINITY)
                .shadow(Shadow::new(nan, nan, nan, Color::rgba(nan, nan, nan, nan))),
            text("x").size(1.0e9).max_lines(0),
            text("y").size(0.0).fill_width().max_width(-1.0),
            Spacer::weighted(nan),
            Spacer::fixed(f32::NEG_INFINITY),
            ScrollView::new(Container::<Msg>::empty().height(f32::INFINITY)),
            ScrollView::new(ScrollView::new(
                Button::new("deep").on_press(Msg::Tap(model.count))
            )),
            Avatar::new(""),
            Avatar::new("   "),
            Icon::new(""),
            Divider::new().thickness(nan).inset(nan),
            Divider::vertical().inset(4.0),
            Button::new("").on_press(Msg::Toggle).offset(nan, nan),
        ]
        .gap(nan)
        .into()
    }
    let sizes = [
        SurfaceSize::new(1080, 2400),
        SurfaceSize::new(1, 1),
        SurfaceSize::new(0, 0),
        SurfaceSize::new(4000, 10),
    ];
    for size in sizes {
        for scale in [0.01, 1.0, 3.0, 100.0] {
            let info = FrameInfo::new(size)
                .with_scale(scale)
                .with_insets(Insets::new(1.0e6, 5.0, 1.0e6, 5.0));
            let mut harness = Harness::with(odd, info);
            for (index, phase) in [
                PointerPhase::Up,
                PointerPhase::Moved,
                PointerPhase::Down,
                PointerPhase::Down,
                PointerPhase::Moved,
                PointerPhase::Cancelled,
                PointerPhase::Down,
                PointerPhase::Moved,
                PointerPhase::Up,
            ]
            .into_iter()
            .enumerate()
            {
                for (x, y) in [
                    (5.0, 5.0),
                    (f32::NAN, f32::NAN),
                    (-1.0e9, 1.0e9),
                    (f32::INFINITY, 0.0),
                ] {
                    harness.touch(index as u64 % 2, phase, x, y);
                    harness.touch(1, phase, x, y * index as f32);
                }
                harness.step(0.016);
            }
            for op in harness.ops() {
                if let DrawOp::Rect { rect, .. } | DrawOp::Glyph { rect, .. } = op {
                    assert!(rect.x.is_finite() && rect.width.is_finite(), "{op:?}");
                }
            }
            let _ = harness.ui.overflows();
        }
    }
}

#[test]
fn an_application_without_fonts_draws_boxes_and_no_text() {
    struct Bare;
    impl Application for Bare {
        type Message = ();
        fn view(&self) -> Element<()> {
            col![text("invisible"), Button::new("Still a box").on_press(())].into()
        }
        fn update(&mut self, (): ()) {}
    }
    let mut ui = Ui::new(Bare);
    let mut frame = Frame::for_app(&mut ui);
    frame.begin();
    ui.frame(&phone(), &mut frame);
    assert_eq!(frame.list.len(), 1, "the button's background");
}

#[test]
fn rebuilding_the_same_screen_gives_the_same_frame() {
    let mut harness = Harness::new(list);
    let first: Vec<DrawOp> = harness.ops().to_vec();
    for _ in 0..3 {
        harness.step(0.25);
        assert_eq!(harness.ops(), first.as_slice());
    }
}
