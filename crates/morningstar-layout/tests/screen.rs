//! A realistic screen with every rectangle computed by hand.
//!
//! 360 x 640. Text is the stand-in from `common`: 10 units per character,
//! breaking anywhere.
//!
//! ```text
//! screen  column, fill x fill
//! ├─ header  row, fill x 56, padding 16 horizontal, space-between, centred
//! │  ├─ title   "Inbox" 5 chars, line 24, hug
//! │  └─ avatar  stack 32 x 32
//! │     └─ dot  10 x 10, pinned bottom-right
//! ├─ list    column, fill x fill
//! │  └─ row (x2)  row, fill x hug, padding 16/12, gap 12, centred
//! │     ├─ icon   40 x 40
//! │     ├─ lines  column, fill x hug, gap 2
//! │     │  ├─ name     line 20, fill width
//! │     │  └─ preview  line 16, fill width
//! │     └─ badge  stack, hug, padding 6/2
//! │        └─ count  line 16, hug
//! └─ bar     column, fill x hug, padding 16
//!    └─ button  stack, fill x 48, content centred
//!       └─ label  "Compose" 7 chars, line 20, hug
//! ```

mod common;

use common::{Texts, r};
use morningstar_layout::{Alignment, CrossAlign, Edges, LayoutTree, MainAlign, NodeId, Style};

struct ListRow {
    row: NodeId,
    icon: NodeId,
    lines: NodeId,
    name: NodeId,
    preview: NodeId,
    badge: NodeId,
    count: NodeId,
}

fn list_row(
    tree: &mut LayoutTree,
    texts: &mut Texts,
    name: usize,
    preview: usize,
    count: usize,
) -> ListRow {
    let icon = tree.leaf(Style::fixed(40.0, 40.0));
    let name = texts.leaf(tree, Style::new().fill_width(), name, 20.0);
    let preview = texts.leaf(tree, Style::new().fill_width(), preview, 16.0);
    let lines = tree.column(Style::new().fill_width().gap(2.0), &[name, preview]);
    let count = texts.leaf(tree, Style::new(), count, 16.0);
    let badge = tree.stack(Style::new().padding(Edges::symmetric(6.0, 2.0)), &[count]);
    let row = tree.row(
        Style::new()
            .fill_width()
            .padding(Edges::symmetric(16.0, 12.0))
            .gap(12.0)
            .cross_align(CrossAlign::Center),
        &[icon, lines, badge],
    );
    ListRow {
        row,
        icon,
        lines,
        name,
        preview,
        badge,
        count,
    }
}

#[test]
fn inbox_screen() {
    let mut tree = LayoutTree::new();
    let mut texts = Texts::default();

    let title = texts.leaf(&mut tree, Style::new(), 5, 24.0);
    let dot = tree.leaf(Style::fixed(10.0, 10.0).place(Alignment::BOTTOM_RIGHT));
    let avatar = tree.stack(Style::fixed(32.0, 32.0), &[dot]);
    let header = tree.row(
        Style::new()
            .fill_width()
            .fixed_height(56.0)
            .padding(Edges::horizontal(16.0))
            .main_align(MainAlign::SpaceBetween)
            .cross_align(CrossAlign::Center),
        &[title, avatar],
    );

    let a = list_row(&mut tree, &mut texts, 12, 60, 1);
    let b = list_row(&mut tree, &mut texts, 9, 20, 2);
    let list = tree.column(Style::new().fill(), &[a.row, b.row]);

    let label = texts.leaf(&mut tree, Style::new(), 7, 20.0);
    let button = tree.stack(
        Style::new()
            .fill_width()
            .fixed_height(48.0)
            .content_align(Alignment::CENTER),
        &[label],
    );
    let bar = tree.column(
        Style::new().fill_width().padding(Edges::all(16.0)),
        &[button],
    );

    let screen = tree.column(Style::new().fill(), &[header, list, bar]);
    let root = texts.compute(&mut tree, screen, r(0.0, 0.0, 360.0, 640.0));
    assert_eq!(root, r(0.0, 0.0, 360.0, 640.0));

    // Header. Content box x 16..344, height 56.
    assert_eq!(tree.rect(header), r(0.0, 0.0, 360.0, 56.0));
    // 5 chars = 50 wide; y = (56 - 24) / 2
    assert_eq!(tree.rect(title), r(16.0, 16.0, 50.0, 24.0));
    // Space-between puts it at 344 - 32; y = (56 - 32) / 2
    assert_eq!(tree.rect(avatar), r(312.0, 12.0, 32.0, 32.0));
    // 312 + 32 - 10, 12 + 32 - 10
    assert_eq!(tree.rect(dot), r(334.0, 34.0, 10.0, 10.0));

    // Bar: 16 + 48 + 16 = 80 tall, at the bottom because the list fills.
    assert_eq!(tree.rect(bar), r(0.0, 560.0, 360.0, 80.0));
    assert_eq!(tree.rect(button), r(16.0, 576.0, 328.0, 48.0));
    // 7 chars = 70; x = 16 + (328 - 70) / 2, y = 576 + (48 - 20) / 2
    assert_eq!(tree.rect(label), r(145.0, 590.0, 70.0, 20.0));

    // List: 640 - 56 - 80 = 504.
    assert_eq!(tree.rect(list), r(0.0, 56.0, 360.0, 504.0));

    // Row A. Content width 328. Badge: 1 char = 10 + 12 padding = 22 wide,
    // 16 + 4 = 20 tall. Lines: 328 - 2 * 12 (gaps) - 40 - 22 = 242 wide,
    // so 24 chars per line: name (12) is 1 line = 20, preview (60) is
    // 3 lines = 48; column 20 + 2 + 48 = 70. Row: 12 + 70 + 12 = 94.
    assert_eq!(tree.rect(a.row), r(0.0, 56.0, 360.0, 94.0));
    // y = 68 + (70 - 40) / 2
    assert_eq!(tree.rect(a.icon), r(16.0, 83.0, 40.0, 40.0));
    assert_eq!(tree.rect(a.lines), r(68.0, 68.0, 242.0, 70.0));
    assert_eq!(tree.rect(a.name), r(68.0, 68.0, 242.0, 20.0));
    assert_eq!(tree.rect(a.preview), r(68.0, 90.0, 242.0, 48.0));
    // x = 68 + 242 + 12, y = 68 + (70 - 20) / 2
    assert_eq!(tree.rect(a.badge), r(322.0, 93.0, 22.0, 20.0));
    assert_eq!(tree.rect(a.count), r(328.0, 95.0, 10.0, 16.0));

    // Row B, at 56 + 94 = 150. Badge: 2 chars = 20 + 12 = 32. Lines:
    // 328 - 24 - 40 - 32 = 232 wide, 23 chars per line: name (9) and
    // preview (20) are one line each; column 20 + 2 + 16 = 38. The icon
    // (40) is now the tallest: row 12 + 40 + 12 = 64.
    assert_eq!(tree.rect(b.row), r(0.0, 150.0, 360.0, 64.0));
    assert_eq!(tree.rect(b.icon), r(16.0, 162.0, 40.0, 40.0));
    // y = 162 + (40 - 38) / 2
    assert_eq!(tree.rect(b.lines), r(68.0, 163.0, 232.0, 38.0));
    assert_eq!(tree.rect(b.name), r(68.0, 163.0, 232.0, 20.0));
    assert_eq!(tree.rect(b.preview), r(68.0, 185.0, 232.0, 16.0));
    // x = 68 + 232 + 12, y = 162 + (40 - 20) / 2
    assert_eq!(tree.rect(b.badge), r(312.0, 172.0, 32.0, 20.0));
    assert_eq!(tree.rect(b.count), r(318.0, 174.0, 20.0, 16.0));

    assert_eq!(tree.rect(screen), r(0.0, 0.0, 360.0, 640.0));
    assert_eq!(tree.overflowing().count(), 0);

    // Every value above is whole, so rounding changes nothing.
    for node in [
        header, title, avatar, dot, list, a.row, a.preview, bar, label,
    ] {
        assert_eq!(tree.rounded_rect(node), tree.rect(node));
    }

    // Eight texts, one measurement each: hugging ones at their natural
    // size, fill-width ones at their final width.
    assert_eq!(texts.calls.len(), 8);
    for key in 0..8 {
        assert_eq!(texts.calls_for(key).len(), 1, "text {key}");
    }
    assert_eq!(texts.calls_for(0), [None]); // title
    assert_eq!(texts.calls_for(1), [Some(242.0)]); // row A name
    assert_eq!(texts.calls_for(2), [Some(242.0)]); // row A preview
    assert_eq!(texts.calls_for(3), [None]); // row A count
    assert_eq!(texts.calls_for(5), [Some(232.0)]); // row B preview
    assert_eq!(texts.calls_for(7), [None]); // label
}

#[test]
fn inbox_screen_on_a_narrow_display_overflows_and_says_where() {
    // The same row on a 120-wide screen: content 88, of which gaps, icon
    // and badge need 24 + 40 + 22 = 86, leaving 2 for the text column.
    let mut tree = LayoutTree::new();
    let mut texts = Texts::default();
    let a = list_row(&mut tree, &mut texts, 12, 60, 1);
    let list = tree.column(Style::new().fill(), &[a.row]);
    texts.compute(&mut tree, list, r(0.0, 0.0, 120.0, 640.0));

    assert_eq!(tree.rect(a.lines).width, 2.0);
    // One character per line: 12 lines of 20 and 60 lines of 16.
    assert_eq!(tree.rect(a.name), r(68.0, 12.0, 2.0, 240.0));
    assert_eq!(tree.rect(a.preview), r(68.0, 254.0, 2.0, 960.0));
    // Row: 12 + (240 + 2 + 960) + 12; it is taller than the list.
    assert_eq!(tree.rect(a.row), r(0.0, 0.0, 120.0, 1226.0));
    let overflowing: Vec<NodeId> = tree.overflowing().map(|(node, _)| node).collect();
    assert_eq!(overflowing, [list]);
    assert_eq!(tree.overflow(list).height, 1226.0 - 640.0);
}
