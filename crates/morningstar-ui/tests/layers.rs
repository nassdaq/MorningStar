//! Group opacity in the UI layer (ADR 0028): a faded widget whose parts
//! overlap is drawn through a display-list layer; one whose parts do not
//! has the opacity multiplied into its colours, as before.

mod common;

use common::Harness;
use morningstar_core::DrawOp;
use morningstar_ui::prelude::*;

const RED: Color = hex(0xFF0000);
const BLUE: Color = hex(0x0000FF);

/// The opacities of the frame's layers, in order, and the alphas of every
/// colour drawn (fills and glyphs), in order.
fn layers_and_alphas(harness: &Harness) -> (Vec<f32>, Vec<f32>) {
    let (mut layers, mut alphas) = (Vec::new(), Vec::new());
    for op in harness.ops() {
        match *op {
            DrawOp::PushLayer { opacity } => layers.push(opacity),
            DrawOp::Rect { color, .. } | DrawOp::Glyph { color, .. } => alphas.push(color.a),
            DrawOp::BorderedRect { fill, .. } => alphas.push(fill.a),
            DrawOp::Shadow { shadow, .. } => alphas.push(shadow.color.a),
            _ => {}
        }
    }
    (layers, alphas)
}

fn balanced(harness: &Harness) -> bool {
    let mut open = 0i32;
    for op in harness.ops() {
        match op {
            DrawOp::PushLayer { .. } => open += 1,
            DrawOp::PopLayer => open -= 1,
            _ => {}
        }
        if open < 0 {
            return false;
        }
    }
    open == 0
}

#[test]
fn a_faded_card_with_a_label_is_one_layer_with_its_colours_whole() {
    let harness = Harness::new(|_| {
        col![
            Container::new(text("Hi"))
                .padding(8.0)
                .background(RED)
                .opacity(0.5)
        ]
        .into()
    });
    let (layers, alphas) = layers_and_alphas(&harness);
    assert_eq!(layers, [0.5]);
    assert_eq!(alphas, [1.0, 1.0, 1.0], "the background and two glyphs");
    assert!(balanced(&harness));
    // The layer is around the card's operations and nothing else.
    let ops = harness.ops();
    assert!(matches!(ops[0], DrawOp::PushLayer { .. }));
    assert_eq!(ops[ops.len() - 1], DrawOp::PopLayer);
}

#[test]
fn what_does_not_overlap_is_faded_colour_by_colour_without_a_layer() {
    // A text, a plain box, a bordered box (one primitive), and a row of
    // them: nothing lies over anything.
    let harness = Harness::new(|_| {
        col![
            Element::from(text("Hi")).opacity(0.5),
            Container::empty()
                .size(20.0, 20.0)
                .background(RED)
                .opacity(0.5),
            Container::empty()
                .size(20.0, 20.0)
                .background(RED)
                .border(2.0, BLUE)
                .opacity(0.5),
            row![
                text("A"),
                Container::empty().size(20.0, 20.0).background(BLUE)
            ]
            .opacity(0.5),
        ]
        .into()
    });
    let (layers, alphas) = layers_and_alphas(&harness);
    assert!(layers.is_empty(), "{layers:?}");
    assert_eq!(
        alphas, [0.5; 6],
        "two glyphs, a box, a bordered box, a glyph, a box"
    );
    assert!(
        !harness
            .ops()
            .iter()
            .any(|op| matches!(op, DrawOp::PopLayer))
    );
}

#[test]
fn a_box_on_its_shadow_and_a_stack_are_layers() {
    let harness = Harness::new(|_| {
        col![
            Container::empty()
                .size(40.0, 40.0)
                .background(RED)
                .shadow(elevation::MEDIUM)
                .opacity(0.25),
            stack![
                Container::empty().size(40.0, 40.0).background(RED),
                Container::empty().size(20.0, 20.0).background(BLUE),
            ]
            .opacity(0.75),
            // A stack of one painted thing and an empty box is not.
            stack![
                Container::empty().size(40.0, 40.0).background(RED),
                Container::empty().size(20.0, 20.0),
            ]
            .opacity(0.5),
        ]
        .into()
    });
    let (layers, alphas) = layers_and_alphas(&harness);
    assert_eq!(layers, [0.25, 0.75]);
    assert_eq!(
        alphas.last(),
        Some(&0.5),
        "the last stack, colour by colour"
    );
    assert!(balanced(&harness));
}

#[test]
fn a_faded_group_far_down_makes_its_faded_ancestors_layers_too() {
    // The column has no overlap of its own, but the card inside it has:
    // fading the column colour by colour would fade the card that way.
    let harness = Harness::new(|_| {
        col![
            text("Title"),
            Container::new(Element::from(text("Hi")).opacity(0.5))
                .padding(8.0)
                .background(RED),
        ]
        .opacity(0.5)
        .into()
    });
    let (layers, alphas) = layers_and_alphas(&harness);
    assert_eq!(layers, [0.5], "one layer, at the column");
    // Inside it everything is whole, except the label that is itself
    // faded, colour by colour, within the group.
    assert_eq!(alphas, [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.5, 0.5]);
    assert!(balanced(&harness));
}

#[test]
fn a_layer_is_inside_the_widgets_transform_and_ends_before_it() {
    let harness = Harness::new(|_| {
        col![
            Container::new(text("Hi"))
                .padding(8.0)
                .background(RED)
                .opacity(0.5)
                .rotate(10.0)
        ]
        .into()
    });
    let ops = harness.ops();
    assert!(matches!(ops[0], DrawOp::PushTransform { .. }));
    assert!(matches!(ops[1], DrawOp::PushLayer { opacity } if opacity == 0.5));
    assert_eq!(ops[ops.len() - 2], DrawOp::PopLayer);
    assert_eq!(ops[ops.len() - 1], DrawOp::PopTransform);
}

#[test]
fn an_opaque_or_unfaded_screen_has_no_layers() {
    let harness = Harness::new(|_| {
        col![
            Container::new(text("Hi")).padding(8.0).background(RED),
            Container::new(text("Hi"))
                .padding(8.0)
                .background(RED)
                .opacity(1.0),
        ]
        .into()
    });
    assert!(layers_and_alphas(&harness).0.is_empty());
}
