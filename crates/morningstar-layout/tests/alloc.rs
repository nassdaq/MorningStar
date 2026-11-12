//! Evidence for the claim that a warmed-up tree does not allocate: neither
//! rebuilding it after `clear` nor laying it out.
//!
//! This file holds a single test so that nothing else runs in the process
//! while allocations are being counted, and only allocations made by the
//! test's own thread are counted.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

use morningstar_core::{Rect, Size};
use morningstar_layout::{CrossAlign, Edges, LayoutTree, NodeId, Sizing, Style};

struct Counting;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    // Const-initialised and without a destructor, so reading it from inside
    // the allocator cannot itself allocate.
    static COUNTING: Cell<bool> = const { Cell::new(false) };
}

// SAFETY: every method forwards to `System` with its arguments unchanged, so
// `System`'s guarantees are this allocator's guarantees. The only addition
// is bumping an atomic counter, which neither allocates nor unwinds.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNTING.with(Cell::get) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: the caller upholds `GlobalAlloc::alloc`'s contract.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` came from `System.alloc` with this `layout`.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if COUNTING.with(Cell::get) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: the caller upholds `GlobalAlloc::realloc`'s contract.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// A list of `rows` rows, each an icon, two wrapped texts and a badge, with
/// a wrapping row of chips after every eighth.
fn build(tree: &mut LayoutTree, rows: usize) -> NodeId {
    let list = tree.column(Style::new().fill(), &[]);
    for index in 0..rows {
        let key = index as u64;
        let icon = tree.leaf(Style::fixed(40.0, 40.0));
        let title = tree.measured(Style::new().fill_width(), key);
        let body = tree.measured(Style::new(), key);
        let lines = tree.column(
            Style::new()
                .fill_width()
                .gap(2.0)
                .cross_align(CrossAlign::Stretch),
            &[title, body],
        );
        let badge = tree.leaf(Style::fixed(20.0, 20.0));
        let row = tree.row(
            Style::new()
                .width(Sizing::FILL)
                .padding(Edges::all(12.0))
                .gap(12.0),
            &[icon, lines, badge],
        );
        tree.add_child(list, row);
        if index % 8 == 0 {
            // A wrapping row of five chips, three to a line.
            let chips = tree.row(Style::new().fill_width().gap(8.0).line_gap(8.0).wrap(), &[]);
            for _ in 0..5 {
                let chip = tree.leaf(Style::fixed(100.0, 24.0));
                tree.add_child(chips, chip);
            }
            tree.add_child(list, chips);
        }
    }
    list
}

fn measure(_key: u64, max_width: Option<f32>) -> Size {
    match max_width {
        Some(width) => Size::new(width.min(400.0), 40.0),
        None => Size::new(400.0, 20.0),
    }
}

#[test]
fn a_warmed_up_tree_does_not_allocate() {
    let mut tree = LayoutTree::new();
    let bounds = Rect::new(0.0, 0.0, 360.0, 640.0);

    // Warm up: the first build and layout size the two vectors.
    let root = build(&mut tree, 200);
    let first = tree.compute(root, bounds, measure);
    let nodes = tree.len();

    ALLOCATIONS.store(0, Ordering::Relaxed);
    COUNTING.with(|counting| counting.set(true));
    let mut last = Rect::default();
    for _ in 0..10 {
        tree.clear();
        let root = build(&mut tree, 200);
        last = tree.compute(root, bounds, measure);
    }
    COUNTING.with(|counting| counting.set(false));

    assert_eq!(ALLOCATIONS.load(Ordering::Relaxed), 0);
    assert_eq!(tree.len(), nodes);
    assert_eq!(last, first);
}
