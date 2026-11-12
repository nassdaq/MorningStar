//! Wrapping rows (`Style::wrap`): the specification. Expected rectangles
//! are computed by hand; the arithmetic is in the comments.

mod common;

use common::{CHAR, Texts, avail, no_measure, r};
use morningstar_core::Size;
use morningstar_layout::{CrossAlign, Edges, LayoutTree, MainAlign, NodeId, Sizing, Style};

fn chips(tree: &mut LayoutTree, sizes: &[(f32, f32)]) -> Vec<NodeId> {
    sizes
        .iter()
        .map(|&(width, height)| tree.leaf(Style::fixed(width, height)))
        .collect()
}

fn rects(tree: &LayoutTree, nodes: &[NodeId]) -> Vec<morningstar_core::Rect> {
    nodes.iter().map(|&node| tree.rect(node)).collect()
}

#[test]
fn children_that_fit_stay_on_one_line_exactly_as_in_a_plain_row() {
    for wrap in [false, true] {
        let mut tree = LayoutTree::new();
        let kids = chips(&mut tree, &[(30.0, 10.0), (40.0, 20.0), (20.0, 10.0)]);
        let mut style = Style::new()
            .fill_width()
            .gap(5.0)
            .line_gap(7.0)
            .cross_align(CrossAlign::Center);
        style.wrap = wrap;
        let row = tree.row(style, &kids);
        tree.compute(row, avail(100.0, 300.0), no_measure);
        // 30 + 5 + 40 + 5 + 20 = 100: exactly full. Height hugs: 20.
        assert_eq!(tree.rect(row), r(0.0, 0.0, 100.0, 20.0));
        assert_eq!(
            rects(&tree, &kids),
            [
                r(0.0, 5.0, 30.0, 10.0),
                r(35.0, 0.0, 40.0, 20.0),
                r(80.0, 5.0, 20.0, 10.0)
            ]
        );
        assert_eq!(tree.overflow(row), Size::ZERO);
    }
}

#[test]
fn a_child_that_does_not_fit_starts_the_next_line() {
    let mut tree = LayoutTree::new();
    let kids = chips(
        &mut tree,
        &[(40.0, 10.0), (40.0, 30.0), (40.0, 12.0), (90.0, 8.0)],
    );
    let row = tree.row(
        Style::new()
            .fixed_width(110.0)
            .padding(Edges::all(5.0))
            .gap(10.0)
            .line_gap(4.0)
            .wrap(),
        &kids,
    );
    tree.compute(row, avail(500.0, 500.0), no_measure);
    // Content box 100 wide from (5, 5).
    // Line 1: 40 + 10 + 40 = 90; a third 40 would make 140. Height 30.
    // Line 2: 40; then 40 + 10 + 90 = 140 does not fit. Height 12.
    // Line 3: 90. Height 8.
    // Height: 30 + 4 + 12 + 4 + 8 = 58, plus padding 10.
    assert_eq!(tree.rect(row), r(0.0, 0.0, 110.0, 68.0));
    assert_eq!(
        rects(&tree, &kids),
        [
            r(5.0, 5.0, 40.0, 10.0),
            r(55.0, 5.0, 40.0, 30.0),
            r(5.0, 39.0, 40.0, 12.0),
            r(5.0, 55.0, 90.0, 8.0)
        ]
    );
    assert_eq!(tree.overflow(row), Size::ZERO);
}

#[test]
fn alignment_applies_to_each_line() {
    let mut tree = LayoutTree::new();
    let kids = chips(&mut tree, &[(40.0, 10.0), (40.0, 30.0), (60.0, 12.0)]);
    let row = tree.row(
        Style::new()
            .fixed_width(100.0)
            .wrap()
            .main_align(MainAlign::End)
            .cross_align(CrossAlign::End),
        &kids,
    );
    tree.compute(row, avail(500.0, 500.0), no_measure);
    // Line 1: 80 used, 20 free in front; 30 tall, the short child at its
    // bottom. Line 2: 60 used, 40 free in front.
    assert_eq!(
        rects(&tree, &kids),
        [
            r(20.0, 20.0, 40.0, 10.0),
            r(60.0, 0.0, 40.0, 30.0),
            r(40.0, 30.0, 60.0, 12.0)
        ]
    );

    let mut tree = LayoutTree::new();
    let kids = chips(&mut tree, &[(30.0, 10.0), (30.0, 10.0), (80.0, 10.0)]);
    let row = tree.row(
        Style::new()
            .fixed_width(100.0)
            .wrap()
            .main_align(MainAlign::SpaceBetween),
        &kids,
    );
    tree.compute(row, avail(500.0, 500.0), no_measure);
    // Line 1: 40 spare between its two children. Line 2: one child, which
    // space-between leaves at the start.
    assert_eq!(
        rects(&tree, &kids),
        [
            r(0.0, 0.0, 30.0, 10.0),
            r(70.0, 0.0, 30.0, 10.0),
            r(0.0, 10.0, 80.0, 10.0)
        ]
    );
}

#[test]
fn a_hugging_row_takes_one_line_if_it_is_offered_the_room_and_wraps_if_not() {
    let build = |tree: &mut LayoutTree| {
        let kids = chips(tree, &[(40.0, 10.0), (40.0, 10.0), (40.0, 10.0)]);
        let row = tree.row(Style::new().gap(10.0).line_gap(2.0).wrap(), &kids);
        (tree.column(Style::new().fill(), &[row]), row, kids)
    };
    let mut tree = LayoutTree::new();
    let (root, row, _) = build(&mut tree);
    tree.compute(root, avail(300.0, 300.0), no_measure);
    // Natural width 140 fits in 300: one line.
    assert_eq!(tree.rect(row), r(0.0, 0.0, 140.0, 10.0));

    let mut tree = LayoutTree::new();
    let (root, row, kids) = build(&mut tree);
    tree.compute(root, avail(100.0, 300.0), no_measure);
    // Offered 100: the row is 100 wide (it does not shrink to its widest
    // line, 90), two lines, 10 + 2 + 10 tall.
    assert_eq!(tree.rect(row), r(0.0, 0.0, 100.0, 22.0));
    assert_eq!(tree.rect(kids[2]), r(0.0, 12.0, 40.0, 10.0));
}

#[test]
fn the_wrapped_height_pushes_what_follows_down_and_is_what_a_scroll_view_reads() {
    let mut tree = LayoutTree::new();
    let kids = chips(&mut tree, &[(60.0, 20.0); 5]);
    let row = tree.row(
        Style::new().fill_width().gap(8.0).line_gap(8.0).wrap(),
        &kids,
    );
    let after = tree.leaf(Style::fixed(10.0, 10.0));
    // A viewport 50 tall: the column of content inside it hugs its height.
    let content = tree.column(Style::new().fill_width(), &[row, after]);
    let viewport = tree.column(Style::new().fill_width().fixed_height(50.0), &[content]);
    tree.compute(viewport, avail(140.0, 500.0), no_measure);
    // Two per line (60 + 8 + 60 = 128 <= 140): three lines, 20 * 3 + 8 * 2.
    assert_eq!(tree.rect(row), r(0.0, 0.0, 140.0, 76.0));
    assert_eq!(tree.rect(kids[4]), r(0.0, 56.0, 60.0, 20.0));
    assert_eq!(tree.rect(after), r(0.0, 76.0, 10.0, 10.0));
    assert_eq!(tree.rect(content).height, 86.0);
    // The row itself does not overflow; the viewport's content does, by
    // what a scroll view would scroll.
    assert_eq!(tree.overflow(row), Size::ZERO);
    assert_eq!(tree.overflow(viewport), Size::new(0.0, 36.0));
}

#[test]
fn a_hugging_text_is_offered_the_whole_line_and_wraps_inside_itself() {
    let mut texts = Texts::default();
    let mut tree = LayoutTree::new();
    let label = tree.leaf(Style::fixed(30.0, 10.0));
    // 8 characters fit beside the label; 14 do not, but fit a line; 25 do
    // not fit a line and wrap inside themselves.
    let short = texts.leaf(&mut tree, Style::new(), 5, 10.0);
    let long = texts.leaf(&mut tree, Style::new(), 14, 10.0);
    let longer = texts.leaf(&mut tree, Style::new(), 25, 10.0);
    let row = tree.row(
        Style::new().fixed_width(150.0).gap(10.0).wrap(),
        &[label, short, long, longer],
    );
    texts.compute(&mut tree, row, avail(500.0, 500.0));
    // Line 1: 30 + 10 + 50 = 90; the 140-wide text would make 240.
    // Line 2: 140; then the 25 characters, offered 150, are 15 per line.
    // Line 3: 150 wide, two lines of text.
    assert_eq!(tree.rect(short), r(40.0, 0.0, 5.0 * CHAR, 10.0));
    assert_eq!(tree.rect(long), r(0.0, 10.0, 140.0, 10.0));
    assert_eq!(tree.rect(longer), r(0.0, 20.0, 150.0, 20.0));
    assert_eq!(tree.rect(row), r(0.0, 0.0, 150.0, 40.0));
    // At most two measurements per leaf, as everywhere else.
    assert_eq!(texts.calls_for(0), [None]);
    assert_eq!(texts.calls_for(1), [None]);
    assert_eq!(texts.calls_for(2), [None, Some(150.0)]);
}

#[test]
fn a_fill_child_takes_what_its_line_has_left() {
    let mut tree = LayoutTree::new();
    let a = tree.leaf(Style::fixed(40.0, 10.0));
    let grow = tree.leaf(Style::new().fill_width().min_width(20.0).fixed_height(10.0));
    let b = tree.leaf(Style::fixed(70.0, 10.0));
    let grow_alone = tree.leaf(Style::new().fill_width().fixed_height(10.0));
    let row = tree.row(
        Style::new().fixed_width(100.0).gap(10.0).wrap(),
        &[a, grow, b, grow_alone],
    );
    tree.compute(row, avail(500.0, 500.0), no_measure);
    // Line 1: 40 + 10 + 20 (the fill's minimum) = 70; adding 10 + 70
    // would not fit. The fill grows into the 50 that are left.
    // Line 2: 70, then the second fill (minimum 0) fits: 100 - 70 - 10.
    assert_eq!(tree.rect(grow), r(50.0, 0.0, 50.0, 10.0));
    assert_eq!(tree.rect(b), r(0.0, 10.0, 70.0, 10.0));
    assert_eq!(tree.rect(grow_alone), r(80.0, 10.0, 20.0, 10.0));
    assert_eq!(tree.overflow(row), Size::ZERO);
}

#[test]
fn lines_stretch_and_fill_within_their_line() {
    let mut tree = LayoutTree::new();
    let tall = tree.leaf(Style::fixed(60.0, 30.0));
    let stretched = tree.leaf(Style::new().fixed_width(30.0));
    let filled = tree.leaf(Style::new().fixed_width(80.0).fill_height());
    let short = tree.leaf(Style::fixed(10.0, 12.0));
    let row = tree.row(
        Style::new()
            .fixed_width(100.0)
            .wrap()
            .cross_align(CrossAlign::Stretch),
        &[tall, stretched, filled, short],
    );
    tree.compute(row, avail(500.0, 500.0), no_measure);
    // Line 1 is 30 tall: the hugging child is stretched to it. Line 2 is
    // as tall as its only sized child, 12; the fill-height child takes it.
    assert_eq!(tree.rect(stretched), r(60.0, 0.0, 30.0, 30.0));
    assert_eq!(tree.rect(filled), r(0.0, 30.0, 80.0, 12.0));
    assert_eq!(tree.rect(row).height, 42.0);
}

#[test]
fn spare_height_is_shared_equally_among_the_lines() {
    let mut tree = LayoutTree::new();
    let kids = chips(&mut tree, &[(60.0, 10.0), (60.0, 20.0)]);
    let row = tree.row(
        Style::new()
            .size(100.0, 50.0)
            .line_gap(4.0)
            .wrap()
            .cross_align(CrossAlign::End),
        &kids,
    );
    tree.compute(row, avail(500.0, 500.0), no_measure);
    // Lines need 10 + 4 + 20 = 34 of 50: 8 more each. Line 1 is 0..18,
    // line 2 is 22..50; each child at the bottom of its line.
    assert_eq!(tree.rect(kids[0]), r(0.0, 8.0, 60.0, 10.0));
    assert_eq!(tree.rect(kids[1]), r(0.0, 30.0, 60.0, 20.0));
    assert_eq!(tree.overflow(row), Size::ZERO);
}

#[test]
fn overflow_is_a_child_wider_than_a_line_or_lines_taller_than_the_row() {
    let mut tree = LayoutTree::new();
    let kids = chips(&mut tree, &[(40.0, 10.0), (130.0, 10.0), (40.0, 10.0)]);
    let row = tree.row(Style::new().size(100.0, 25.0).wrap(), &kids);
    tree.compute(row, avail(500.0, 500.0), no_measure);
    // The 130-wide child has a line to itself and sticks out by 30; three
    // lines of 10 in a height of 25 stick out by 5.
    assert_eq!(tree.rect(kids[1]), r(0.0, 10.0, 130.0, 10.0));
    assert_eq!(tree.rect(kids[2]), r(0.0, 20.0, 40.0, 10.0));
    assert_eq!(tree.overflow(row), Size::new(30.0, 5.0));
}

#[test]
fn wrap_is_ignored_by_columns_and_stacks_and_harmless_on_an_empty_row() {
    let mut tree = LayoutTree::new();
    let kids = chips(&mut tree, &[(40.0, 30.0), (40.0, 30.0)]);
    let column = tree.column(Style::new().size(50.0, 40.0).wrap(), &kids);
    tree.compute(column, avail(500.0, 500.0), no_measure);
    assert_eq!(tree.rect(kids[1]), r(0.0, 30.0, 40.0, 30.0));

    let mut tree = LayoutTree::new();
    let row = tree.row(
        Style::new().fill_width().padding(Edges::all(3.0)).wrap(),
        &[],
    );
    tree.compute(row, avail(100.0, 100.0), no_measure);
    assert_eq!(tree.rect(row), r(0.0, 0.0, 100.0, 6.0));
}

#[test]
fn a_row_given_exactly_its_natural_width_never_wraps() {
    // Thirds and sevenths do not add up exactly in binary; the row's own
    // sum must still fit the row.
    for count in 2..40_usize {
        for step in 1..30_u32 {
            let width = step as f32 / 7.0 + 0.1;
            let gap = step as f32 / 3.0;
            let mut tree = LayoutTree::new();
            let kids = chips(&mut tree, &vec![(width, 5.0); count]);
            let row = tree.row(Style::new().gap(gap).wrap(), &kids);
            let root = tree.column(Style::new().fill(), &[row]);
            tree.compute(root, avail(1.0e6, 100.0), no_measure);
            assert_eq!(tree.rect(row).height, 5.0, "{count} x {width} gap {gap}");
        }
    }
}

struct Lcg(u64);

impl Lcg {
    fn next(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) as usize) % bound.max(1)
    }

    fn value(&mut self) -> f32 {
        const VALUES: [f32; 10] = [
            0.0,
            1.0,
            7.5,
            40.0,
            250.0,
            -3.0,
            f32::NAN,
            f32::INFINITY,
            f32::MAX,
            1.0e-3,
        ];
        VALUES[self.next(VALUES.len())]
    }

    fn sizing(&mut self) -> Sizing {
        match self.next(3) {
            0 => Sizing::Hug,
            1 => Sizing::Fixed(self.value()),
            _ => Sizing::Fill(self.value()),
        }
    }
}

#[test]
fn random_wrapping_rows_with_hostile_values_stay_finite_and_keep_lines_in_order() {
    let mut rng = Lcg(0xfeed);
    let mut tree = LayoutTree::new();
    for _ in 0..3000 {
        tree.clear();
        let mut measured = 0_u64;
        let count = rng.next(12);
        let kids: Vec<NodeId> = (0..count)
            .map(|_| {
                let mut style = Style::new().width(rng.sizing()).height(rng.sizing());
                if rng.next(3) == 0 {
                    style = style.min_width(rng.value()).max_width(rng.value());
                }
                if rng.next(2) == 0 {
                    measured += 1;
                    tree.measured(style, measured - 1)
                } else {
                    tree.leaf(style)
                }
            })
            .collect();
        let mut style = Style::new()
            .width(rng.sizing())
            .height(rng.sizing())
            .gap(rng.value())
            .line_gap(rng.value())
            .wrap();
        style.main_align =
            [MainAlign::Start, MainAlign::Center, MainAlign::SpaceAround][rng.next(3)];
        let row = tree.row(style, &kids);
        let root = tree.column(Style::new().fill(), &[row]);

        let mut calls = vec![0_u32; measured as usize];
        let mut sizes = Lcg(rng.next(1000) as u64);
        tree.compute(
            root,
            r(0.0, 0.0, rng.value(), rng.value()),
            |key, max_width| {
                calls[key as usize] += 1;
                if let Some(width) = max_width {
                    assert!(width.is_finite() && width >= 0.0);
                }
                Size::new(sizes.value(), sizes.value())
            },
        );
        assert!(calls.iter().all(|&count| count <= 2), "{calls:?}");
        for &node in kids.iter().chain([&row, &root]) {
            let rect = tree.rect(node);
            for value in [rect.x, rect.y, rect.width, rect.height] {
                assert!(value.is_finite(), "{rect:?}");
            }
            assert!(rect.width >= 0.0 && rect.height >= 0.0, "{rect:?}");
        }
        let overflow = tree.overflow(row);
        assert!(overflow.width.is_finite() && overflow.height.is_finite());
    }
}
