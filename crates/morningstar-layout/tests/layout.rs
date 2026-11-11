//! The specification of the layout engine. Expected rectangles are computed
//! by hand; the arithmetic is in the comments.

mod common;

use common::{Texts, avail, no_measure, r};
use morningstar_core::{Rect, Size};
use morningstar_layout::{
    Alignment, CrossAlign, Edges, LayoutTree, MainAlign, NodeId, Sizing, Style, round_rect,
    snap_rect,
};

fn boxes(tree: &mut LayoutTree, count: usize, width: f32, height: f32) -> Vec<NodeId> {
    (0..count)
        .map(|_| tree.leaf(Style::fixed(width, height)))
        .collect()
}

fn xs(tree: &LayoutTree, nodes: &[NodeId]) -> Vec<f32> {
    nodes.iter().map(|&node| tree.rect(node).x).collect()
}

fn ys(tree: &LayoutTree, nodes: &[NodeId]) -> Vec<f32> {
    nodes.iter().map(|&node| tree.rect(node).y).collect()
}

fn widths(tree: &LayoutTree, nodes: &[NodeId]) -> Vec<f32> {
    nodes.iter().map(|&node| tree.rect(node).width).collect()
}

// ---------------------------------------------------------------- root

#[test]
fn fixed_root_is_placed_at_the_origin_and_ignores_available_size() {
    let mut tree = LayoutTree::new();
    let root = tree.leaf(Style::fixed(30.0, 20.0));
    let rect = tree.compute(root, r(5.0, 7.0, 100.0, 100.0), no_measure);
    assert_eq!(rect, r(5.0, 7.0, 30.0, 20.0));
    assert_eq!(tree.rect(root), rect);
}

#[test]
fn hug_leaf_has_no_size() {
    let mut tree = LayoutTree::new();
    let root = tree.leaf(Style::new());
    tree.compute(root, avail(100.0, 100.0), no_measure);
    assert_eq!(tree.rect(root), r(0.0, 0.0, 0.0, 0.0));
}

#[test]
fn fill_root_takes_the_available_size() {
    let mut tree = LayoutTree::new();
    let root = tree.leaf(Style::new().fill());
    tree.compute(root, avail(300.0, 200.0), no_measure);
    assert_eq!(tree.rect(root), r(0.0, 0.0, 300.0, 200.0));
}

#[test]
fn fill_root_hugs_when_the_available_size_is_unbounded() {
    let mut tree = LayoutTree::new();
    let child = tree.leaf(Style::fixed(30.0, 20.0));
    let root = tree.column(Style::new().fill(), &[child]);
    tree.compute(root, avail(f32::INFINITY, f32::INFINITY), no_measure);
    assert_eq!(tree.rect(root), r(0.0, 0.0, 30.0, 20.0));
}

#[test]
fn hug_root_is_capped_at_the_available_width_but_not_height() {
    let mut tree = LayoutTree::new();
    let kids = boxes(&mut tree, 2, 80.0, 150.0);
    let root = tree.row(Style::new(), &kids);
    tree.compute(root, avail(100.0, 100.0), no_measure);
    // Natural width 160 is capped at 100; height 150 is not capped at 100.
    assert_eq!(tree.rect(root), r(0.0, 0.0, 100.0, 150.0));
    assert_eq!(tree.overflow(root), Size::new(60.0, 0.0));
}

// ------------------------------------------------- rows, columns, spacing

#[test]
fn row_with_padding_and_gap_hugs_its_children() {
    let mut tree = LayoutTree::new();
    let a = tree.leaf(Style::fixed(30.0, 10.0));
    let b = tree.leaf(Style::fixed(40.0, 20.0));
    let row = tree.row(
        Style::new()
            .padding(Edges::new(10.0, 5.0, 20.0, 15.0))
            .gap(4.0),
        &[a, b],
    );
    tree.compute(row, avail(500.0, 500.0), no_measure);
    // width 10 + 30 + 4 + 40 + 20, height 5 + max(10, 20) + 15
    assert_eq!(tree.rect(row), r(0.0, 0.0, 104.0, 40.0));
    assert_eq!(tree.rect(a), r(10.0, 5.0, 30.0, 10.0));
    assert_eq!(tree.rect(b), r(44.0, 5.0, 40.0, 20.0));
}

#[test]
fn column_with_padding_and_gap_hugs_its_children() {
    let mut tree = LayoutTree::new();
    let a = tree.leaf(Style::fixed(30.0, 10.0));
    let b = tree.leaf(Style::fixed(40.0, 20.0));
    let column = tree.column(
        Style::new()
            .padding(Edges::new(10.0, 5.0, 20.0, 15.0))
            .gap(4.0),
        &[a, b],
    );
    tree.compute(column, avail(500.0, 500.0), no_measure);
    // width 10 + max(30, 40) + 20, height 5 + 10 + 4 + 20 + 15
    assert_eq!(tree.rect(column), r(0.0, 0.0, 70.0, 54.0));
    assert_eq!(tree.rect(a), r(10.0, 5.0, 30.0, 10.0));
    assert_eq!(tree.rect(b), r(10.0, 19.0, 40.0, 20.0));
}

#[test]
fn negative_gap_overlaps_children() {
    let mut tree = LayoutTree::new();
    let kids = boxes(&mut tree, 3, 40.0, 40.0);
    let row = tree.row(Style::new().gap(-10.0), &kids);
    tree.compute(row, avail(500.0, 500.0), no_measure);
    assert_eq!(tree.rect(row), r(0.0, 0.0, 100.0, 40.0));
    assert_eq!(xs(&tree, &kids), [0.0, 30.0, 60.0]);
}

#[test]
fn nested_padding_accumulates_into_absolute_coordinates() {
    let mut tree = LayoutTree::new();
    let leaf = tree.leaf(Style::fixed(20.0, 20.0));
    let inner = tree.column(
        Style::new()
            .fill_width()
            .padding(Edges::new(1.0, 2.0, 3.0, 4.0)),
        &[leaf],
    );
    let outer = tree.column(
        Style::fixed(200.0, 200.0).padding(Edges::all(10.0)),
        &[inner],
    );
    tree.compute(outer, r(5.0, 7.0, 0.0, 0.0), no_measure);
    assert_eq!(tree.rect(outer), r(5.0, 7.0, 200.0, 200.0));
    // x 5 + 10, y 7 + 10, width 200 - 20, height 2 + 20 + 4
    assert_eq!(tree.rect(inner), r(15.0, 17.0, 180.0, 26.0));
    assert_eq!(tree.rect(leaf), r(16.0, 19.0, 20.0, 20.0));
}

// ------------------------------------------------------- main alignment

/// Three 40-wide children in a 180-wide row: 60 units to spare.
fn main_aligned_row(align: MainAlign, gap: f32, width: f32) -> Vec<f32> {
    let mut tree = LayoutTree::new();
    let kids = boxes(&mut tree, 3, 40.0, 40.0);
    let row = tree.row(Style::fixed(width, 40.0).main_align(align).gap(gap), &kids);
    tree.compute(row, avail(500.0, 500.0), no_measure);
    xs(&tree, &kids)
}

#[test]
fn main_align_in_a_row() {
    use MainAlign::*;
    assert_eq!(main_aligned_row(Start, 0.0, 180.0), [0.0, 40.0, 80.0]);
    assert_eq!(main_aligned_row(Center, 0.0, 180.0), [30.0, 70.0, 110.0]);
    assert_eq!(main_aligned_row(End, 0.0, 180.0), [60.0, 100.0, 140.0]);
    // 60 / 2 between
    assert_eq!(
        main_aligned_row(SpaceBetween, 0.0, 180.0),
        [0.0, 70.0, 140.0]
    );
    // 60 / 3 = 20 per child, half of it before the first
    assert_eq!(
        main_aligned_row(SpaceAround, 0.0, 180.0),
        [10.0, 70.0, 130.0]
    );
    // 60 / 4 = 15 in each of the four slots
    assert_eq!(
        main_aligned_row(SpaceEvenly, 0.0, 180.0),
        [15.0, 70.0, 125.0]
    );
}

#[test]
fn main_align_adds_to_the_gap() {
    use MainAlign::*;
    // 200 wide, gap 10: used 140, spare 60.
    assert_eq!(main_aligned_row(Start, 10.0, 200.0), [0.0, 50.0, 100.0]);
    assert_eq!(main_aligned_row(Center, 10.0, 200.0), [30.0, 80.0, 130.0]);
    assert_eq!(
        main_aligned_row(SpaceBetween, 10.0, 200.0),
        [0.0, 80.0, 160.0]
    );
}

#[test]
fn main_align_in_a_column() {
    for (align, expected) in [
        (MainAlign::Start, [0.0, 40.0, 80.0]),
        (MainAlign::Center, [30.0, 70.0, 110.0]),
        (MainAlign::End, [60.0, 100.0, 140.0]),
        (MainAlign::SpaceBetween, [0.0, 70.0, 140.0]),
        (MainAlign::SpaceAround, [10.0, 70.0, 130.0]),
        (MainAlign::SpaceEvenly, [15.0, 70.0, 125.0]),
    ] {
        let mut tree = LayoutTree::new();
        let kids = boxes(&mut tree, 3, 40.0, 40.0);
        let column = tree.column(Style::fixed(40.0, 180.0).main_align(align), &kids);
        tree.compute(column, avail(500.0, 500.0), no_measure);
        assert_eq!(ys(&tree, &kids), expected, "{align:?}");
        assert_eq!(xs(&tree, &kids), [0.0, 0.0, 0.0]);
    }
}

#[test]
fn space_between_with_one_child_puts_it_at_the_start() {
    let mut tree = LayoutTree::new();
    let kid = tree.leaf(Style::fixed(40.0, 40.0));
    let row = tree.row(
        Style::fixed(180.0, 40.0).main_align(MainAlign::SpaceBetween),
        &[kid],
    );
    tree.compute(row, avail(500.0, 500.0), no_measure);
    assert_eq!(tree.rect(kid).x, 0.0);
}

// ------------------------------------------------------ cross alignment

fn cross_aligned_row(align: CrossAlign, child: Style) -> Rect {
    let mut tree = LayoutTree::new();
    let kid = tree.leaf(child);
    let row = tree.row(Style::fixed(200.0, 100.0).cross_align(align), &[kid]);
    tree.compute(row, avail(500.0, 500.0), no_measure);
    tree.rect(kid)
}

#[test]
fn cross_align_in_a_row() {
    let square = Style::fixed(40.0, 40.0);
    assert_eq!(
        cross_aligned_row(CrossAlign::Start, square),
        r(0.0, 0.0, 40.0, 40.0)
    );
    assert_eq!(
        cross_aligned_row(CrossAlign::Center, square),
        r(0.0, 30.0, 40.0, 40.0)
    );
    assert_eq!(
        cross_aligned_row(CrossAlign::End, square),
        r(0.0, 60.0, 40.0, 40.0)
    );
}

#[test]
fn stretch_applies_to_hugging_children_only() {
    let hugging = Style::new().fixed_width(40.0);
    assert_eq!(
        cross_aligned_row(CrossAlign::Stretch, hugging),
        r(0.0, 0.0, 40.0, 100.0)
    );
    // A fixed cross size is kept, at the start.
    assert_eq!(
        cross_aligned_row(CrossAlign::Stretch, Style::fixed(40.0, 40.0)),
        r(0.0, 0.0, 40.0, 40.0)
    );
    // Stretching respects the maximum.
    assert_eq!(
        cross_aligned_row(CrossAlign::Stretch, hugging.max_height(60.0)),
        r(0.0, 0.0, 40.0, 60.0)
    );
}

#[test]
fn align_self_overrides_the_container() {
    let square = Style::fixed(40.0, 40.0);
    assert_eq!(
        cross_aligned_row(CrossAlign::Start, square.align_self(CrossAlign::End)),
        r(0.0, 60.0, 40.0, 40.0)
    );
    // Opting out of a stretching container.
    assert_eq!(
        cross_aligned_row(
            CrossAlign::Stretch,
            Style::new()
                .fixed_width(40.0)
                .align_self(CrossAlign::Center)
        ),
        r(0.0, 50.0, 40.0, 0.0)
    );
    // Opting in to stretch in a non-stretching one.
    assert_eq!(
        cross_aligned_row(
            CrossAlign::Center,
            Style::new()
                .fixed_width(40.0)
                .align_self(CrossAlign::Stretch)
        ),
        r(0.0, 0.0, 40.0, 100.0)
    );
}

#[test]
fn cross_align_in_a_column() {
    for (align, child, expected) in [
        (
            CrossAlign::Start,
            Style::fixed(40.0, 40.0),
            r(0.0, 0.0, 40.0, 40.0),
        ),
        (
            CrossAlign::Center,
            Style::fixed(40.0, 40.0),
            r(30.0, 0.0, 40.0, 40.0),
        ),
        (
            CrossAlign::End,
            Style::fixed(40.0, 40.0),
            r(60.0, 0.0, 40.0, 40.0),
        ),
        (
            CrossAlign::Stretch,
            Style::new().fixed_height(40.0),
            r(0.0, 0.0, 100.0, 40.0),
        ),
    ] {
        let mut tree = LayoutTree::new();
        let kid = tree.leaf(child);
        let column = tree.column(Style::fixed(100.0, 200.0).cross_align(align), &[kid]);
        tree.compute(column, avail(500.0, 500.0), no_measure);
        assert_eq!(tree.rect(kid), expected, "{align:?}");
    }
}

#[test]
fn a_hugging_column_stretches_children_to_its_widest_child() {
    let mut tree = LayoutTree::new();
    let wide = tree.leaf(Style::fixed(120.0, 10.0));
    let divider = tree.leaf(Style::new().fixed_height(1.0));
    let column = tree.column(
        Style::new().cross_align(CrossAlign::Stretch),
        &[wide, divider],
    );
    tree.compute(column, avail(500.0, 500.0), no_measure);
    assert_eq!(tree.rect(column), r(0.0, 0.0, 120.0, 11.0));
    assert_eq!(tree.rect(divider), r(0.0, 10.0, 120.0, 1.0));
}

// ------------------------------------------------------------------ fill

fn filled_row(width: f32, children: &[Style]) -> (LayoutTree, NodeId, Vec<NodeId>) {
    let mut tree = LayoutTree::new();
    let kids: Vec<NodeId> = children.iter().map(|&style| tree.leaf(style)).collect();
    let row = tree.row(Style::fixed(width, 10.0), &kids);
    tree.compute(row, avail(1000.0, 1000.0), no_measure);
    (tree, row, kids)
}

fn fill(weight: f32) -> Style {
    Style::new().width(Sizing::Fill(weight)).fixed_height(10.0)
}

#[test]
fn fill_children_share_space_by_weight() {
    let (tree, _, kids) = filled_row(400.0, &[fill(1.0), fill(2.0), fill(1.0)]);
    assert_eq!(widths(&tree, &kids), [100.0, 200.0, 100.0]);
    assert_eq!(xs(&tree, &kids), [0.0, 100.0, 300.0]);
}

#[test]
fn fill_children_get_what_fixed_children_and_gaps_leave() {
    let mut tree = LayoutTree::new();
    let fixed = tree.leaf(Style::fixed(40.0, 40.0));
    let a = tree.leaf(Style::new().height(Sizing::Fill(1.0)).fixed_width(10.0));
    let b = tree.leaf(Style::new().height(Sizing::Fill(3.0)).fixed_width(10.0));
    let column = tree.column(Style::fixed(10.0, 200.0).gap(10.0), &[fixed, a, b]);
    tree.compute(column, avail(1000.0, 1000.0), no_measure);
    // 200 - 2 gaps of 10 - 40 = 140, split 1 : 3
    assert_eq!(tree.rect(a), r(0.0, 50.0, 10.0, 35.0));
    assert_eq!(tree.rect(b), r(0.0, 95.0, 10.0, 105.0));
}

#[test]
fn fill_shares_that_do_not_divide_evenly_keep_their_fractions() {
    let (tree, row, kids) = filled_row(100.0, &[fill(1.0), fill(1.0), fill(1.0)]);
    for &kid in &kids {
        assert!((tree.rect(kid).width - 100.0 / 3.0).abs() < 1e-4);
    }
    // No overflow is reported for float error in the shares.
    assert_eq!(tree.overflow(row), Size::ZERO);
    // Rounded, the remainder goes to the middle child and the edges meet.
    let rounded: Vec<Rect> = kids.iter().map(|&kid| tree.rounded_rect(kid)).collect();
    assert_eq!(rounded[0], r(0.0, 0.0, 33.0, 10.0));
    assert_eq!(rounded[1], r(33.0, 0.0, 34.0, 10.0));
    assert_eq!(rounded[2], r(67.0, 0.0, 33.0, 10.0));
}

#[test]
fn fill_respects_max_and_gives_the_excess_to_the_others() {
    let (tree, _, kids) = filled_row(300.0, &[fill(1.0).max_width(50.0), fill(1.0), fill(1.0)]);
    assert_eq!(widths(&tree, &kids), [50.0, 125.0, 125.0]);
}

#[test]
fn fill_respects_min_and_takes_the_shortfall_from_the_others() {
    let (tree, _, kids) = filled_row(300.0, &[fill(1.0).min_width(200.0), fill(1.0), fill(1.0)]);
    assert_eq!(widths(&tree, &kids), [200.0, 50.0, 50.0]);
}

#[test]
fn fill_with_min_and_max_cancelling_out() {
    // Shares of 100: the first loses 50 to its max, the second gains 50
    // from its min, the third keeps its share.
    let (tree, row, kids) = filled_row(
        300.0,
        &[
            fill(1.0).max_width(50.0),
            fill(1.0).min_width(150.0),
            fill(1.0),
        ],
    );
    assert_eq!(widths(&tree, &kids), [50.0, 150.0, 100.0]);
    assert_eq!(tree.overflow(row), Size::ZERO);
}

#[test]
fn fill_gets_nothing_when_no_space_is_left() {
    let (tree, row, kids) = filled_row(
        100.0,
        &[
            Style::fixed(80.0, 10.0),
            Style::fixed(80.0, 10.0),
            fill(1.0),
        ],
    );
    assert_eq!(widths(&tree, &kids), [80.0, 80.0, 0.0]);
    assert_eq!(tree.overflow(row), Size::new(60.0, 0.0));
}

#[test]
fn invalid_fill_weights_count_as_zero() {
    let (tree, _, kids) = filled_row(
        100.0,
        &[
            fill(0.0),
            fill(f32::NAN),
            fill(-2.0),
            fill(f32::INFINITY),
            fill(1.0),
        ],
    );
    assert_eq!(widths(&tree, &kids), [0.0, 0.0, 0.0, 0.0, 100.0]);

    let (tree, _, kids) = filled_row(100.0, &[fill(0.0).min_width(7.0), fill(0.0)]);
    assert_eq!(widths(&tree, &kids), [7.0, 0.0]);
}

#[test]
fn fill_on_the_cross_axis_takes_the_content_box() {
    let mut tree = LayoutTree::new();
    let kid = tree.leaf(Style::new().fixed_width(40.0).fill_height());
    let row = tree.row(Style::fixed(200.0, 100.0).padding(Edges::all(10.0)), &[kid]);
    tree.compute(row, avail(500.0, 500.0), no_measure);
    assert_eq!(tree.rect(kid), r(10.0, 10.0, 40.0, 80.0));
}

#[test]
fn fill_inside_hug_collapses() {
    let mut tree = LayoutTree::new();
    let filler = tree.leaf(Style::new().fill_width().fixed_height(10.0));
    let fixed = tree.leaf(Style::fixed(30.0, 10.0));
    let row = tree.row(Style::new(), &[filler, fixed]);
    tree.compute(row, avail(500.0, 500.0), no_measure);
    assert_eq!(tree.rect(row), r(0.0, 0.0, 30.0, 10.0));
    assert_eq!(tree.rect(filler).width, 0.0);
    assert_eq!(tree.rect(fixed).x, 0.0);
}

#[test]
fn a_spacer_pushes_siblings_apart() {
    let mut tree = LayoutTree::new();
    let left = tree.leaf(Style::fixed(30.0, 10.0));
    let spacer = tree.leaf(Style::new().fill_width());
    let right = tree.leaf(Style::fixed(30.0, 10.0));
    let row = tree.row(Style::new().fixed_width(200.0), &[left, spacer, right]);
    tree.compute(row, avail(500.0, 500.0), no_measure);
    assert_eq!(tree.rect(spacer), r(30.0, 0.0, 140.0, 0.0));
    assert_eq!(tree.rect(right), r(170.0, 0.0, 30.0, 10.0));
}

// --------------------------------------------------------------- min/max

#[test]
fn min_and_max_apply_to_every_sizing_mode() {
    let mut tree = LayoutTree::new();
    let fixed_over = tree.leaf(Style::fixed(100.0, 100.0).max_width(60.0).max_height(70.0));
    let fixed_under = tree.leaf(Style::fixed(10.0, 10.0).min_width(25.0).min_height(35.0));
    let hug = tree.leaf(Style::new().min_width(12.0).min_height(13.0));
    let min_beats_max = tree.leaf(Style::new().min_width(50.0).max_width(20.0));
    let column = tree.column(Style::new(), &[fixed_over, fixed_under, hug, min_beats_max]);
    tree.compute(column, avail(500.0, 500.0), no_measure);
    assert_eq!(tree.rect(fixed_over), r(0.0, 0.0, 60.0, 70.0));
    assert_eq!(tree.rect(fixed_under), r(0.0, 70.0, 25.0, 35.0));
    assert_eq!(tree.rect(hug), r(0.0, 105.0, 12.0, 13.0));
    assert_eq!(tree.rect(min_beats_max), r(0.0, 118.0, 50.0, 0.0));
}

#[test]
fn min_and_max_apply_to_hugging_containers() {
    let mut tree = LayoutTree::new();
    let small = tree.leaf(Style::fixed(30.0, 30.0));
    let padded_out = tree.column(Style::new().min_width(80.0).min_height(90.0), &[small]);
    tree.compute(padded_out, avail(500.0, 500.0), no_measure);
    assert_eq!(tree.rect(padded_out), r(0.0, 0.0, 80.0, 90.0));

    let kids = boxes(&mut tree, 2, 40.0, 40.0);
    let squeezed = tree.row(Style::new().max_width(50.0).max_height(25.0), &kids);
    tree.compute(squeezed, avail(500.0, 500.0), no_measure);
    assert_eq!(tree.rect(squeezed), r(0.0, 0.0, 50.0, 25.0));
    assert_eq!(tree.overflow(squeezed), Size::new(30.0, 15.0));
}

// ------------------------------------------------------- measured leaves

/// One 25-character text (250 wide unwrapped, 20 per line) in a column of
/// the given fixed width.
fn text_in_column(width: f32, column: Style, text: Style) -> (LayoutTree, Texts, NodeId, NodeId) {
    let mut tree = LayoutTree::new();
    let mut texts = Texts::default();
    let leaf = texts.leaf(&mut tree, text, 25, 20.0);
    let column = tree.column(column.fixed_width(width), &[leaf]);
    texts.compute(&mut tree, column, avail(1000.0, 1000.0));
    (tree, texts, column, leaf)
}

#[test]
fn text_that_fits_is_measured_once_at_its_natural_size() {
    let (tree, texts, column, leaf) = text_in_column(400.0, Style::new(), Style::new());
    assert_eq!(tree.rect(leaf), r(0.0, 0.0, 250.0, 20.0));
    assert_eq!(tree.rect(column), r(0.0, 0.0, 400.0, 20.0));
    assert_eq!(texts.calls, [(0, None)]);
}

#[test]
fn hugging_text_wraps_at_the_parent_width_and_hugs_the_wrapped_lines() {
    // 95 wide: 9 characters per line, 3 lines, longest line 90.
    let (tree, texts, column, leaf) = text_in_column(95.0, Style::new(), Style::new());
    assert_eq!(tree.rect(leaf), r(0.0, 0.0, 90.0, 60.0));
    // The height flowed back up into the hugging column.
    assert_eq!(tree.rect(column), r(0.0, 0.0, 95.0, 60.0));
    assert_eq!(texts.calls, [(0, None), (0, Some(95.0))]);
}

#[test]
fn fill_width_text_wraps_with_a_single_measurement() {
    let (tree, texts, _, leaf) = text_in_column(95.0, Style::new(), Style::new().fill_width());
    assert_eq!(tree.rect(leaf), r(0.0, 0.0, 95.0, 60.0));
    assert_eq!(texts.calls, [(0, Some(95.0))]);
}

#[test]
fn fixed_width_text_wraps_with_a_single_measurement() {
    let (tree, texts, _, leaf) =
        text_in_column(400.0, Style::new(), Style::new().fixed_width(100.0));
    assert_eq!(tree.rect(leaf), r(0.0, 0.0, 100.0, 60.0));
    assert_eq!(texts.calls, [(0, Some(100.0))]);
}

#[test]
fn stretched_text_is_remeasured_only_when_narrower_than_natural() {
    let stretch = Style::new().cross_align(CrossAlign::Stretch);
    let (tree, texts, _, leaf) = text_in_column(95.0, stretch, Style::new());
    assert_eq!(tree.rect(leaf), r(0.0, 0.0, 95.0, 60.0));
    assert_eq!(texts.calls, [(0, None), (0, Some(95.0))]);

    let (tree, texts, _, leaf) = text_in_column(300.0, stretch, Style::new());
    assert_eq!(tree.rect(leaf), r(0.0, 0.0, 300.0, 20.0));
    assert_eq!(texts.calls, [(0, None)]);
}

#[test]
fn max_width_on_text_is_the_natural_constraint() {
    // 12 characters per line: 3 lines, 120 wide.
    let (tree, texts, _, leaf) = text_in_column(500.0, Style::new(), Style::new().max_width(120.0));
    assert_eq!(tree.rect(leaf), r(0.0, 0.0, 120.0, 60.0));
    assert_eq!(texts.calls, [(0, Some(120.0))]);
}

#[test]
fn text_with_no_hugging_axis_is_never_measured() {
    let mut tree = LayoutTree::new();
    let fixed = tree.measured(Style::fixed(50.0, 20.0), 0);
    let filling = tree.measured(Style::new().fill(), 1);
    let ratio = tree.measured(Style::new().fill_width().aspect_ratio(2.0), 2);
    let column = tree.column(Style::fixed(100.0, 200.0), &[fixed, filling, ratio]);
    tree.compute(column, avail(500.0, 500.0), no_measure);
    assert_eq!(tree.rect(fixed), r(0.0, 0.0, 50.0, 20.0));
    // 200 - 20 - 50 left for the filling one
    assert_eq!(tree.rect(filling), r(0.0, 20.0, 100.0, 130.0));
    assert_eq!(tree.rect(ratio), r(0.0, 150.0, 100.0, 50.0));
}

#[test]
fn hugging_text_in_a_row_wraps_in_what_fixed_siblings_leave() {
    let mut tree = LayoutTree::new();
    let mut texts = Texts::default();
    let icon = tree.leaf(Style::fixed(40.0, 40.0));
    let text = texts.leaf(&mut tree, Style::new(), 25, 20.0);
    let trailing = tree.leaf(Style::fixed(20.0, 20.0));
    let row = tree.row(
        Style::new().fixed_width(230.0).gap(10.0),
        &[icon, text, trailing],
    );
    texts.compute(&mut tree, row, avail(1000.0, 1000.0));
    // 230 - 2 gaps - 40 - 20 = 150: 15 per line, 2 lines.
    assert_eq!(tree.rect(text), r(50.0, 0.0, 150.0, 40.0));
    assert_eq!(tree.rect(trailing), r(210.0, 0.0, 20.0, 20.0));
    assert_eq!(tree.rect(row), r(0.0, 0.0, 230.0, 40.0));
    assert_eq!(tree.overflow(row), Size::ZERO);
    assert_eq!(texts.calls, [(0, None), (0, Some(150.0))]);
}

#[test]
fn wrapped_text_height_pushes_later_siblings_down() {
    let mut tree = LayoutTree::new();
    let mut texts = Texts::default();
    let text = texts.leaf(&mut tree, Style::new().fill_width(), 25, 20.0);
    let below = tree.leaf(Style::fixed(10.0, 10.0));
    let column = tree.column(Style::new().fixed_width(95.0), &[text, below]);
    texts.compute(&mut tree, column, avail(1000.0, 1000.0));
    assert_eq!(tree.rect(below), r(0.0, 60.0, 10.0, 10.0));
    assert_eq!(tree.rect(column), r(0.0, 0.0, 95.0, 70.0));
}

#[test]
fn hugging_root_text_wraps_at_the_available_width() {
    let mut tree = LayoutTree::new();
    let mut texts = Texts::default();
    let text = texts.leaf(&mut tree, Style::new(), 25, 20.0);
    texts.compute(&mut tree, text, avail(100.0, 30.0));
    assert_eq!(tree.rect(text), r(0.0, 0.0, 100.0, 60.0));
    assert_eq!(texts.calls, [(0, None), (0, Some(100.0))]);
}

#[test]
fn text_that_cannot_wrap_narrow_enough_overflows() {
    let mut tree = LayoutTree::new();
    let word = tree.measured(Style::new(), 0);
    let column = tree.column(Style::new().fixed_width(50.0), &[word]);
    // One unbreakable 80-wide word.
    tree.compute(column, avail(500.0, 500.0), |_, _| Size::new(80.0, 20.0));
    assert_eq!(tree.rect(word), r(0.0, 0.0, 80.0, 20.0));
    assert_eq!(tree.overflow(column), Size::new(30.0, 0.0));
}

#[test]
fn measure_is_called_at_most_twice_per_leaf_in_a_nested_tree() {
    // column(200 wide, padding 10)
    //   row(fill, gap 10)
    //     icon 40x40
    //     column(fill): text A (10 chars), text B (30 chars)
    //     badge = stack(padding 4): text C (2 chars)
    let build = |text_style: Style| {
        let mut tree = LayoutTree::new();
        let mut texts = Texts::default();
        let icon = tree.leaf(Style::fixed(40.0, 40.0));
        let a = texts.leaf(&mut tree, text_style, 10, 20.0);
        let b = texts.leaf(&mut tree, text_style, 30, 20.0);
        let lines = tree.column(Style::new().fill_width(), &[a, b]);
        let c = texts.leaf(&mut tree, Style::new(), 2, 20.0);
        let badge = tree.stack(Style::new().padding(Edges::all(4.0)), &[c]);
        let row = tree.row(Style::new().fill_width().gap(10.0), &[icon, lines, badge]);
        let root = tree.column(
            Style::new().fixed_width(200.0).padding(Edges::all(10.0)),
            &[row],
        );
        texts.compute(&mut tree, root, avail(1000.0, 1000.0));
        (tree, texts, [a, b, c, lines, row, root])
    };

    // Hugging texts: badge is 20 + 8 = 28 wide, so the text column gets
    // 180 - 20 - 40 - 28 = 92. Both texts are wider than that unwrapped
    // (100 and 300) and are measured a second time at 92: 9 per line.
    let (tree, texts, [a, b, c, lines, row, root]) = build(Style::new());
    assert_eq!(texts.calls_for(0), [None, Some(92.0)]);
    assert_eq!(texts.calls_for(1), [None, Some(92.0)]);
    assert_eq!(texts.calls_for(2), [None]);
    assert_eq!(texts.calls.len(), 5);
    assert_eq!(tree.rect(a), r(60.0, 10.0, 90.0, 40.0));
    assert_eq!(tree.rect(b), r(60.0, 50.0, 90.0, 80.0));
    assert_eq!(tree.rect(c), r(166.0, 14.0, 20.0, 20.0));
    assert_eq!(tree.rect(lines), r(60.0, 10.0, 92.0, 120.0));
    assert_eq!(tree.rect(row), r(10.0, 10.0, 180.0, 120.0));
    assert_eq!(tree.rect(root), r(0.0, 0.0, 200.0, 140.0));

    // Fill-width texts: one call each, same wrapping.
    let (tree, texts, [a, b, ..]) = build(Style::new().fill_width());
    assert_eq!(texts.calls_for(0), [Some(92.0)]);
    assert_eq!(texts.calls_for(1), [Some(92.0)]);
    assert_eq!(texts.calls_for(2), [None]);
    assert_eq!(texts.calls.len(), 3);
    assert_eq!(tree.rect(a), r(60.0, 10.0, 92.0, 40.0));
    assert_eq!(tree.rect(b), r(60.0, 50.0, 92.0, 80.0));
}

#[test]
fn measure_calls_do_not_multiply_with_nesting_depth() {
    // hug column > row > hug column > row > ... > text: the shape that makes
    // naive "measure, then lay out again at the final size" exponential.
    let mut tree = LayoutTree::new();
    let mut texts = Texts::default();
    let mut node = texts.leaf(&mut tree, Style::new(), 25, 20.0);
    for _ in 0..20 {
        let row = tree.row(Style::new(), &[node]);
        node = tree.column(Style::new().cross_align(CrossAlign::Stretch), &[row]);
    }
    texts.compute(&mut tree, node, avail(95.0, 1000.0));
    assert_eq!(texts.calls, [(0, None), (0, Some(95.0))]);
    assert_eq!(tree.rect(node), r(0.0, 0.0, 95.0, 60.0));
}

// ------------------------------------------------------------------ stack

#[test]
fn stack_places_children_at_the_nine_anchors() {
    for (place, x, y) in [
        (Alignment::TOP_LEFT, 10.0, 10.0),
        (Alignment::TOP_CENTER, 40.0, 10.0),
        (Alignment::TOP_RIGHT, 70.0, 10.0),
        (Alignment::CENTER_LEFT, 10.0, 40.0),
        (Alignment::CENTER, 40.0, 40.0),
        (Alignment::CENTER_RIGHT, 70.0, 40.0),
        (Alignment::BOTTOM_LEFT, 10.0, 70.0),
        (Alignment::BOTTOM_CENTER, 40.0, 70.0),
        (Alignment::BOTTOM_RIGHT, 70.0, 70.0),
    ] {
        // Content box is 80 x 80 at (10, 10); the child is 20 x 20.
        let mut tree = LayoutTree::new();
        let by_child = tree.leaf(Style::fixed(20.0, 20.0).place(place));
        let stack = tree.stack(
            Style::fixed(100.0, 100.0).padding(Edges::all(10.0)),
            &[by_child],
        );
        tree.compute(stack, avail(500.0, 500.0), no_measure);
        assert_eq!(tree.rect(by_child), r(x, y, 20.0, 20.0), "{place:?}");

        let mut tree = LayoutTree::new();
        let by_parent = tree.leaf(Style::fixed(20.0, 20.0));
        let stack = tree.stack(
            Style::fixed(100.0, 100.0)
                .padding(Edges::all(10.0))
                .content_align(place),
            &[by_parent],
        );
        tree.compute(stack, avail(500.0, 500.0), no_measure);
        assert_eq!(tree.rect(by_parent), r(x, y, 20.0, 20.0), "{place:?}");
    }
}

#[test]
fn stack_children_overlap_and_place_overrides_content_align() {
    let mut tree = LayoutTree::new();
    let background = tree.leaf(Style::new().fill());
    let centred = tree.leaf(Style::fixed(20.0, 10.0));
    let pinned = tree.leaf(Style::fixed(8.0, 8.0).place(Alignment::BOTTOM_RIGHT));
    let stack = tree.stack(
        Style::fixed(100.0, 80.0)
            .padding(Edges::all(10.0))
            .content_align(Alignment::CENTER),
        &[background, centred, pinned],
    );
    tree.compute(stack, avail(500.0, 500.0), no_measure);
    assert_eq!(tree.rect(background), r(10.0, 10.0, 80.0, 60.0));
    assert_eq!(tree.rect(centred), r(40.0, 35.0, 20.0, 10.0));
    assert_eq!(tree.rect(pinned), r(82.0, 62.0, 8.0, 8.0));
}

#[test]
fn hugging_stack_is_as_large_as_its_largest_child() {
    let mut tree = LayoutTree::new();
    let tall = tree.leaf(Style::fixed(30.0, 50.0));
    let wide = tree.leaf(Style::fixed(60.0, 20.0));
    let filler = tree.leaf(Style::new().fill());
    let stack = tree.stack(Style::new().padding(Edges::all(5.0)), &[tall, wide, filler]);
    tree.compute(stack, avail(500.0, 500.0), no_measure);
    assert_eq!(tree.rect(stack), r(0.0, 0.0, 70.0, 60.0));
    assert_eq!(tree.rect(filler), r(5.0, 5.0, 60.0, 50.0));
}

#[test]
fn offset_lets_a_badge_overhang_its_corner() {
    let mut tree = LayoutTree::new();
    let dot = tree.leaf(Style::fixed(4.0, 4.0));
    let badge = tree.stack(
        Style::fixed(12.0, 12.0)
            .place(Alignment::TOP_RIGHT)
            .offset(4.0, -4.0)
            .content_align(Alignment::CENTER),
        &[dot],
    );
    let avatar = tree.stack(Style::fixed(40.0, 40.0), &[badge]);
    let after = tree.leaf(Style::fixed(10.0, 10.0));
    let row = tree.row(Style::new(), &[avatar, after]);
    tree.compute(row, r(100.0, 100.0, 500.0, 500.0), no_measure);
    // Pinned at x 128, y 100, then moved by (4, -4).
    assert_eq!(tree.rect(badge), r(132.0, 96.0, 12.0, 12.0));
    // The subtree moves with it.
    assert_eq!(tree.rect(dot), r(136.0, 100.0, 4.0, 4.0));
    // Siblings and overflow are unaffected.
    assert_eq!(tree.rect(after), r(140.0, 100.0, 10.0, 10.0));
    assert_eq!(tree.overflowing().count(), 0);
}

// --------------------------------------------------------------- overflow

#[test]
fn children_larger_than_the_parent_overflow_and_are_reported() {
    let mut tree = LayoutTree::new();
    let kids = boxes(&mut tree, 3, 50.0, 60.0);
    let column = tree.column(Style::fixed(100.0, 100.0), &kids);
    tree.compute(column, avail(500.0, 500.0), no_measure);
    // Sizes are kept, nothing is clipped or shrunk.
    assert_eq!(ys(&tree, &kids), [0.0, 60.0, 120.0]);
    assert_eq!(tree.rect(kids[2]), r(0.0, 120.0, 50.0, 60.0));
    assert_eq!(tree.overflow(column), Size::new(0.0, 80.0));
    assert_eq!(
        tree.overflowing().collect::<Vec<_>>(),
        [(column, Size::new(0.0, 80.0))]
    );
}

#[test]
fn cross_axis_and_stack_overflow_are_reported() {
    let mut tree = LayoutTree::new();
    let wide = tree.leaf(Style::fixed(150.0, 10.0));
    let column = tree.column(
        Style::fixed(100.0, 100.0).padding(Edges::all(10.0)),
        &[wide],
    );
    tree.compute(column, avail(500.0, 500.0), no_measure);
    assert_eq!(tree.overflow(column), Size::new(70.0, 0.0));

    let big = tree.leaf(Style::fixed(150.0, 120.0));
    let stack = tree.stack(Style::fixed(100.0, 100.0), &[big]);
    tree.compute(stack, avail(500.0, 500.0), no_measure);
    assert_eq!(tree.overflow(stack), Size::new(50.0, 20.0));
}

#[test]
fn alignment_when_children_do_not_fit() {
    let positions = |align: MainAlign| {
        let mut tree = LayoutTree::new();
        let kids = boxes(&mut tree, 3, 50.0, 60.0);
        let column = tree.column(Style::fixed(100.0, 100.0).main_align(align), &kids);
        tree.compute(column, avail(500.0, 500.0), no_measure);
        ys(&tree, &kids)
    };
    // 180 of content in 100: 80 too much.
    assert_eq!(positions(MainAlign::Start), [0.0, 60.0, 120.0]);
    assert_eq!(positions(MainAlign::Center), [-40.0, 20.0, 80.0]);
    assert_eq!(positions(MainAlign::End), [-80.0, -20.0, 40.0]);
    assert_eq!(positions(MainAlign::SpaceBetween), [0.0, 60.0, 120.0]);
    assert_eq!(positions(MainAlign::SpaceAround), [0.0, 60.0, 120.0]);
    assert_eq!(positions(MainAlign::SpaceEvenly), [0.0, 60.0, 120.0]);
}

#[test]
fn hugging_height_is_not_capped_by_the_parent() {
    let mut tree = LayoutTree::new();
    let kids = boxes(&mut tree, 3, 50.0, 60.0);
    let content = tree.column(Style::new(), &kids);
    let viewport = tree.stack(Style::fixed(100.0, 100.0), &[content]);
    tree.compute(viewport, avail(500.0, 500.0), no_measure);
    // The content reports its full height (what a scroll view would need).
    assert_eq!(tree.rect(content), r(0.0, 0.0, 50.0, 180.0));
    assert_eq!(tree.overflow(viewport), Size::new(0.0, 80.0));
    assert_eq!(tree.overflow(content), Size::ZERO);
}

#[test]
fn hugging_width_is_capped_by_the_parent() {
    let mut tree = LayoutTree::new();
    let kids = boxes(&mut tree, 2, 80.0, 10.0);
    let row = tree.row(Style::new(), &kids);
    let parent = tree.stack(Style::fixed(100.0, 100.0), &[row]);
    tree.compute(parent, avail(500.0, 500.0), no_measure);
    // The row is held to 100; the overflow is reported where it happens.
    assert_eq!(tree.rect(row), r(0.0, 0.0, 100.0, 10.0));
    assert_eq!(tree.overflow(row), Size::new(60.0, 0.0));
    assert_eq!(tree.overflow(parent), Size::ZERO);
}

#[test]
fn zero_available_space() {
    let mut tree = LayoutTree::new();
    let fixed = tree.leaf(Style::fixed(30.0, 30.0));
    let filler = tree.leaf(Style::new().fill());
    let root = tree.column(
        Style::new().fill().padding(Edges::all(8.0)),
        &[fixed, filler],
    );
    tree.compute(root, avail(0.0, 0.0), no_measure);
    assert_eq!(tree.rect(root), r(0.0, 0.0, 0.0, 0.0));
    // Padding still offsets the children; the content box is empty.
    assert_eq!(tree.rect(fixed), r(8.0, 8.0, 30.0, 30.0));
    assert_eq!(tree.rect(filler), r(8.0, 38.0, 0.0, 0.0));
    assert_eq!(tree.overflow(root), Size::new(30.0, 30.0));
}

// ----------------------------------------------------------- aspect ratio

#[test]
fn aspect_ratio_derives_height_from_width() {
    let mut tree = LayoutTree::new();
    let hero = tree.leaf(Style::new().fill_width().aspect_ratio(16.0 / 9.0));
    let below = tree.leaf(Style::fixed(10.0, 10.0));
    let column = tree.column(Style::new().fixed_width(320.0), &[hero, below]);
    tree.compute(column, avail(500.0, 500.0), no_measure);
    assert_eq!(tree.rect(hero), r(0.0, 0.0, 320.0, 180.0));
    assert_eq!(tree.rect(below).y, 180.0);
}

#[test]
fn aspect_ratio_derives_width_from_a_fixed_height() {
    let mut tree = LayoutTree::new();
    let thumb = tree.leaf(Style::new().fixed_height(50.0).aspect_ratio(2.0));
    let after = tree.leaf(Style::fixed(10.0, 10.0));
    let row = tree.row(
        Style::fixed(300.0, 80.0).cross_align(CrossAlign::Stretch),
        &[thumb, after],
    );
    tree.compute(row, avail(500.0, 500.0), no_measure);
    assert_eq!(tree.rect(thumb), r(0.0, 0.0, 100.0, 50.0));
    assert_eq!(tree.rect(after).x, 100.0);
}

#[test]
fn aspect_ratio_is_not_undone_by_stretch_and_ignores_bad_values() {
    let mut tree = LayoutTree::new();
    let kept = tree.leaf(Style::new().fixed_width(60.0).aspect_ratio(2.0));
    let nan = tree.leaf(Style::new().fixed_width(60.0).aspect_ratio(f32::NAN));
    let zero = tree.leaf(Style::new().fixed_width(60.0).aspect_ratio(0.0));
    let row = tree.row(
        Style::fixed(300.0, 100.0).cross_align(CrossAlign::Stretch),
        &[kept, nan, zero],
    );
    tree.compute(row, avail(500.0, 500.0), no_measure);
    assert_eq!(tree.rect(kept), r(0.0, 0.0, 60.0, 30.0));
    // No usable ratio: these are ordinary hugging leaves, so they stretch.
    assert_eq!(tree.rect(nan), r(60.0, 0.0, 60.0, 100.0));
    assert_eq!(tree.rect(zero), r(120.0, 0.0, 60.0, 100.0));
}

// --------------------------------------------------------------- rounding

#[test]
fn round_rect_rounds_edges_not_sizes() {
    // right edge 10.6 -> 11, bottom edge 10.8 -> 11
    assert_eq!(round_rect(r(0.4, 0.6, 10.2, 10.2)), r(0.0, 1.0, 11.0, 10.0));
    assert_eq!(round_rect(r(-0.4, -0.6, 1.0, 1.0)), r(0.0, -1.0, 1.0, 1.0));
    assert_eq!(round_rect(r(3.0, 4.0, 5.0, 6.0)), r(3.0, 4.0, 5.0, 6.0));
}

#[test]
fn snap_rect_rounds_to_device_pixels() {
    // At 2 px per unit the grid is every 0.5: edges 0.3 -> 0.5, 10.6 -> 10.5.
    assert_eq!(
        snap_rect(r(0.3, 0.3, 10.3, 10.3), 2.0),
        r(0.5, 0.5, 10.0, 10.0)
    );
    // An unusable scale is treated as 1.
    for scale in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert_eq!(
            snap_rect(r(0.4, 0.6, 10.2, 10.2), scale),
            r(0.0, 1.0, 11.0, 10.0)
        );
    }
}

#[test]
fn rounded_siblings_tile_without_gaps_or_overlaps() {
    for count in [2, 3, 7, 13] {
        for step in 0..200 {
            let width = 50.0 + step as f32 * 1.37;
            let origin = step as f32 * 0.31;
            let mut tree = LayoutTree::new();
            let kids: Vec<NodeId> = (0..count)
                .map(|index| tree.leaf(fill(1.0 + (index % 3) as f32)))
                .collect();
            let row = tree.row(Style::fixed(width, 10.0), &kids);
            tree.compute(row, r(origin, 0.0, 0.0, 0.0), no_measure);

            let outer = tree.rounded_rect(row);
            let mut edge = outer.x;
            for &kid in &kids {
                let rect = tree.rounded_rect(kid);
                assert_eq!(rect.x, edge, "width {width}, {count} children");
                assert!(rect.width >= 0.0);
                edge = rect.x + rect.width;
            }
            assert_eq!(
                edge,
                outer.x + outer.width,
                "width {width}, {count} children"
            );
        }
    }
}

// -------------------------------------------------------------- bad input

fn all_rects_are_sane(tree: &LayoutTree, ids: &[NodeId]) {
    for &id in ids {
        let rect = tree.rect(id);
        for value in [rect.x, rect.y, rect.width, rect.height] {
            assert!(value.is_finite(), "{id:?}: {rect:?}");
        }
        assert!(rect.width >= 0.0 && rect.height >= 0.0, "{id:?}: {rect:?}");
        let overflow = tree.overflow(id);
        assert!(overflow.width.is_finite() && overflow.height.is_finite());
        assert!(overflow.width >= 0.0 && overflow.height >= 0.0);
    }
}

#[test]
fn nan_and_negative_style_values_have_defined_meanings() {
    let mut tree = LayoutTree::new();
    let nan = f32::NAN;
    let a = tree.leaf(Style::fixed(nan, -5.0));
    let b = tree.leaf(
        Style::fixed(20.0, 20.0)
            .min_width(nan)
            .max_width(nan)
            .offset(nan, nan),
    );
    let c = tree.leaf(Style::fixed(20.0, 20.0).max_width(-3.0).min_height(-9.0));
    let row = tree.row(
        Style::new()
            .padding(Edges::new(nan, -4.0, 2.0, 3.0))
            .gap(nan),
        &[a, b, c],
    );
    tree.compute(row, r(nan, f32::INFINITY, nan, -10.0), no_measure);
    // NaN/negative fixed sizes are 0; NaN bounds are no bounds; a negative
    // max is 0; NaN/negative padding and NaN gap and offset are 0; a NaN or
    // infinite origin is 0.
    assert_eq!(tree.rect(a), r(0.0, 0.0, 0.0, 0.0));
    assert_eq!(tree.rect(b), r(0.0, 0.0, 20.0, 20.0));
    assert_eq!(tree.rect(c), r(20.0, 0.0, 0.0, 20.0));
    // Available width NaN -> 0, which caps the hugging row; height hugs.
    assert_eq!(tree.rect(row), r(0.0, 0.0, 0.0, 23.0));
    // The stored style is the sanitised one.
    let stored = tree.style(row).unwrap();
    assert_eq!(stored.padding, Edges::new(0.0, 0.0, 2.0, 3.0));
    assert_eq!(stored.gap, 0.0);
}

#[test]
fn infinite_and_huge_sizes_stay_finite() {
    let mut tree = LayoutTree::new();
    let huge = tree.leaf(Style::fixed(f32::INFINITY, f32::MAX));
    let filler = tree.leaf(Style::new().fill());
    let column = tree.column(
        Style::new()
            .padding(Edges::all(f32::INFINITY))
            .gap(f32::MAX),
        &[huge, filler],
    );
    tree.compute(
        column,
        r(f32::MAX, f32::MIN, f32::INFINITY, f32::INFINITY),
        no_measure,
    );
    all_rects_are_sane(&tree, &[huge, filler, column]);
    // Lengths are capped at 1e9.
    assert_eq!(tree.rect(huge).width, 1.0e9);
}

#[test]
fn bad_measure_results_are_sanitised() {
    for (result, expected) in [
        (Size::new(f32::NAN, f32::NAN), Size::new(0.0, 0.0)),
        (Size::new(-5.0, -1.0), Size::new(0.0, 0.0)),
        (Size::new(f32::INFINITY, 20.0), Size::new(100.0, 20.0)),
        (Size::new(30.0, f32::INFINITY), Size::new(30.0, 1.0e9)),
    ] {
        let mut tree = LayoutTree::new();
        let leaf = tree.measured(Style::new(), 0);
        let column = tree.column(Style::new().fixed_width(100.0), &[leaf]);
        tree.compute(column, avail(500.0, 500.0), |_, _| result);
        all_rects_are_sane(&tree, &[leaf, column]);
        let rect = tree.rect(leaf);
        // An "infinitely wide" leaf reports the same nonsense when asked
        // to fit 100, so it keeps its capped width and overflows.
        if result.width.is_infinite() {
            assert_eq!(rect.width, 1.0e9);
        } else {
            assert_eq!(Size::new(rect.width, rect.height), expected);
        }
    }
}

#[test]
fn invalid_ids_are_ignored() {
    let mut other = LayoutTree::new();
    for _ in 0..5 {
        other.leaf(Style::new());
    }
    let foreign = other.leaf(Style::fixed(1.0, 1.0));

    let mut tree = LayoutTree::new();
    let leaf = tree.leaf(Style::fixed(10.0, 10.0));
    let row = tree.row(Style::new(), &[leaf, foreign]);
    assert_eq!(tree.children(row).collect::<Vec<_>>(), [leaf]);

    assert_eq!(
        tree.compute(foreign, avail(10.0, 10.0), no_measure),
        Rect::default()
    );
    assert_eq!(tree.rect(foreign), Rect::default());
    assert_eq!(tree.rounded_rect(foreign), Rect::default());
    assert_eq!(tree.overflow(foreign), Size::ZERO);
    assert_eq!(tree.style(foreign), None);
    assert_eq!(tree.parent(foreign), None);
    assert_eq!(tree.children(foreign).count(), 0);
    assert!(!tree.set_style(foreign, Style::new()));
    assert!(!tree.add_child(foreign, leaf));
    assert!(!tree.add_child(row, foreign));
}

#[test]
fn add_child_refuses_anything_that_is_not_a_tree() {
    let mut tree = LayoutTree::new();
    let leaf = tree.leaf(Style::new());
    let other_leaf = tree.leaf(Style::new());
    let inner = tree.row(Style::new(), &[leaf]);
    let outer = tree.column(Style::new(), &[inner]);

    assert!(
        !tree.add_child(leaf, other_leaf),
        "a leaf cannot have children"
    );
    assert!(!tree.add_child(outer, leaf), "already has a parent");
    assert!(!tree.add_child(outer, outer), "itself");
    assert!(!tree.add_child(inner, outer), "its own ancestor");
    assert!(tree.add_child(inner, other_leaf));

    assert_eq!(tree.children(inner).collect::<Vec<_>>(), [leaf, other_leaf]);
    assert_eq!(tree.children(outer).collect::<Vec<_>>(), [inner]);
    assert_eq!(tree.parent(leaf), Some(inner));
    assert_eq!(tree.parent(inner), Some(outer));
    assert_eq!(tree.parent(outer), None);
    // Still lays out.
    tree.compute(outer, avail(10.0, 10.0), no_measure);
}

#[test]
fn empty_containers_are_laid_out_like_leaves() {
    let mut tree = LayoutTree::new();
    let row = tree.row(Style::new().padding(Edges::all(3.0)).gap(10.0), &[]);
    let column = tree.column(Style::new().main_align(MainAlign::SpaceEvenly), &[]);
    let stack = tree.stack(Style::fixed(5.0, 5.0), &[]);
    let root = tree.column(Style::new(), &[row, column, stack]);
    tree.compute(root, avail(100.0, 100.0), no_measure);
    assert_eq!(tree.rect(row), r(0.0, 0.0, 6.0, 6.0));
    assert_eq!(tree.rect(column), r(0.0, 6.0, 0.0, 0.0));
    assert_eq!(tree.rect(stack), r(0.0, 6.0, 5.0, 5.0));
}

/// A small deterministic generator, so the test needs no dependency.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % bound
    }

    fn value(&mut self) -> f32 {
        const VALUES: [f32; 12] = [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MAX,
            -5.0,
            0.0,
            0.5,
            7.25,
            33.0,
            120.0,
            1.0e20,
            -0.0,
        ];
        VALUES[self.next(VALUES.len())]
    }

    fn sizing(&mut self) -> Sizing {
        match self.next(3) {
            0 => Sizing::Fixed(self.value()),
            1 => Sizing::Hug,
            _ => Sizing::Fill(self.value()),
        }
    }

    fn style(&mut self) -> Style {
        let mut style = Style::new().width(self.sizing()).height(self.sizing());
        if self.next(3) == 0 {
            style = style.min_width(self.value()).max_height(self.value());
        }
        if self.next(3) == 0 {
            style = style.max_width(self.value()).min_height(self.value());
        }
        if self.next(4) == 0 {
            style = style.aspect_ratio(self.value());
        }
        if self.next(2) == 0 {
            style = style.padding(Edges::new(
                self.value(),
                self.value(),
                self.value(),
                self.value(),
            ));
        }
        if self.next(2) == 0 {
            style = style.gap(self.value());
        }
        if self.next(5) == 0 {
            style = style.offset(self.value(), self.value());
        }
        style.main_align = [
            MainAlign::Start,
            MainAlign::Center,
            MainAlign::End,
            MainAlign::SpaceBetween,
            MainAlign::SpaceAround,
            MainAlign::SpaceEvenly,
        ][self.next(6)];
        style.cross_align = [
            CrossAlign::Start,
            CrossAlign::Center,
            CrossAlign::End,
            CrossAlign::Stretch,
        ][self.next(4)];
        if self.next(3) == 0 {
            style.align_self = Some(CrossAlign::Stretch);
        }
        style.content_align = [
            Alignment::TOP_LEFT,
            Alignment::CENTER,
            Alignment::BOTTOM_RIGHT,
        ][self.next(3)];
        style
    }
}

#[test]
fn random_trees_with_hostile_values_never_panic_or_produce_nan() {
    let mut rng = Lcg(0x5eed);
    let mut tree = LayoutTree::new();
    for _ in 0..2000 {
        tree.clear();
        let mut loose: Vec<NodeId> = Vec::new();
        let mut all: Vec<NodeId> = Vec::new();
        let mut measured = 0_u64;
        let node_count = 1 + rng.next(40);
        for _ in 0..node_count {
            let style = rng.style();
            let id = match rng.next(5) {
                0 => tree.leaf(style),
                1 => {
                    measured += 1;
                    tree.measured(style, measured - 1)
                }
                kind => {
                    // Adopt some of the nodes that have no parent yet.
                    let take = rng.next(loose.len() + 1);
                    let children = loose.split_off(loose.len() - take);
                    match kind {
                        2 => tree.row(style, &children),
                        3 => tree.column(style, &children),
                        _ => tree.stack(style, &children),
                    }
                }
            };
            loose.push(id);
            all.push(id);
        }
        let root = tree.column(rng.style(), &loose);
        all.push(root);

        let mut calls = vec![0_u32; measured as usize];
        let bounds = r(rng.value(), rng.value(), rng.value(), rng.value());
        let mut measure_rng = Lcg(rng.next(1000) as u64);
        tree.compute(root, bounds, |key, max_width| {
            calls[key as usize] += 1;
            if let Some(width) = max_width {
                assert!(width.is_finite() && width >= 0.0, "max_width {width}");
            }
            Size::new(measure_rng.value(), measure_rng.value())
        });

        all_rects_are_sane(&tree, &all);
        assert!(calls.iter().all(|&count| count <= 2), "{calls:?}");
    }
}

// ------------------------------------------------------- tree management

#[test]
fn clear_empties_the_tree_and_restarts_ids() {
    let mut tree = LayoutTree::with_capacity(8);
    assert!(tree.is_empty());
    let first = tree.leaf(Style::fixed(10.0, 10.0));
    let root = tree.row(Style::new(), &[first]);
    tree.compute(root, avail(100.0, 100.0), no_measure);
    assert_eq!(tree.len(), 2);
    assert_eq!((first.index(), root.index()), (0, 1));

    tree.clear();
    assert!(tree.is_empty());
    assert_eq!(tree.rect(first), Rect::default());

    let again = tree.leaf(Style::fixed(3.0, 4.0));
    assert_eq!(again.index(), 0);
    // A fresh node carries nothing over from the node that had its index.
    assert_eq!(tree.rect(again), Rect::default());
    assert_eq!(tree.parent(again), None);
    tree.compute(again, avail(100.0, 100.0), no_measure);
    assert_eq!(tree.rect(again), r(0.0, 0.0, 3.0, 4.0));
}

#[test]
fn set_style_takes_effect_at_the_next_compute() {
    let mut tree = LayoutTree::new();
    let leaf = tree.leaf(Style::fixed(10.0, 10.0));
    let root = tree.row(Style::new(), &[leaf]);
    tree.compute(root, avail(100.0, 100.0), no_measure);
    assert_eq!(tree.rect(root), r(0.0, 0.0, 10.0, 10.0));

    assert!(tree.set_style(leaf, Style::fixed(25.0, 5.0)));
    assert_eq!(tree.rect(root), r(0.0, 0.0, 10.0, 10.0));
    tree.compute(root, avail(100.0, 100.0), no_measure);
    assert_eq!(tree.rect(root), r(0.0, 0.0, 25.0, 5.0));
    // Recomputing is idempotent.
    tree.compute(root, avail(100.0, 100.0), no_measure);
    assert_eq!(tree.rect(root), r(0.0, 0.0, 25.0, 5.0));
}

#[test]
fn several_roots_can_share_a_tree() {
    let mut tree = LayoutTree::new();
    let screen = tree.leaf(Style::new().fill());
    let dot = tree.leaf(Style::fixed(10.0, 10.0));
    let overlay = tree.stack(Style::new().fill().content_align(Alignment::CENTER), &[dot]);
    tree.compute(screen, avail(100.0, 200.0), no_measure);
    tree.compute(overlay, r(20.0, 20.0, 60.0, 60.0), no_measure);
    assert_eq!(tree.rect(screen), r(0.0, 0.0, 100.0, 200.0));
    assert_eq!(tree.rect(dot), r(45.0, 45.0, 10.0, 10.0));
}

#[test]
fn a_subtree_can_be_laid_out_on_its_own() {
    let mut tree = LayoutTree::new();
    let leaf = tree.leaf(Style::fixed(10.0, 10.0));
    let inner = tree.row(Style::new().padding(Edges::all(2.0)), &[leaf]);
    let sibling = tree.leaf(Style::fixed(50.0, 50.0));
    let _outer = tree.column(Style::new(), &[sibling, inner]);
    tree.compute(inner, r(100.0, 100.0, 500.0, 500.0), no_measure);
    assert_eq!(tree.rect(inner), r(100.0, 100.0, 14.0, 14.0));
    assert_eq!(tree.rect(leaf), r(102.0, 102.0, 10.0, 10.0));
    assert_eq!(tree.rect(sibling), Rect::default());
}

#[test]
fn very_deep_trees_do_not_overflow_the_call_stack() {
    let mut tree = LayoutTree::new();
    let leaf = tree.leaf(Style::fixed(10.0, 10.0));
    let mut node = leaf;
    for level in 0..100_000 {
        let style = Style::new().padding(Edges::new(0.0, 0.0, 0.0, 0.0));
        node = match level % 3 {
            0 => tree.row(style, &[node]),
            1 => tree.column(style, &[node]),
            _ => tree.stack(style, &[node]),
        };
    }
    tree.compute(node, avail(500.0, 500.0), no_measure);
    assert_eq!(tree.rect(node), r(0.0, 0.0, 10.0, 10.0));
    assert_eq!(tree.rect(leaf), r(0.0, 0.0, 10.0, 10.0));

    // And with padding at each of 1000 levels, positions accumulate.
    tree.clear();
    let leaf = tree.leaf(Style::fixed(10.0, 10.0));
    let mut node = leaf;
    for _ in 0..1000 {
        node = tree.column(Style::new().padding(Edges::all(1.0)), &[node]);
    }
    tree.compute(node, avail(5000.0, 5000.0), no_measure);
    assert_eq!(tree.rect(node), r(0.0, 0.0, 2010.0, 2010.0));
    assert_eq!(tree.rect(leaf), r(1000.0, 1000.0, 10.0, 10.0));
}
