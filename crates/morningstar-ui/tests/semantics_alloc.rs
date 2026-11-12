//! Evidence for the claim that accessibility costs nothing while it is off
//! (ADR 0032): a frame of a screen that uses the semantics modifiers
//! allocates exactly as often as the same screen without them, plus one
//! small allocation per label, hint or value given (the string's box, made
//! by `view` like any other text of the screen). Nothing is collected,
//! built or kept.
//!
//! This file holds a single test so that nothing else runs in the process
//! while allocations are being counted, and only allocations made by the
//! test's own thread are counted.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

use morningstar_core::SurfaceSize;
use morningstar_runtime::{App, Frame, FrameInfo};
use morningstar_ui::prelude::*;

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

const REGULAR: &[u8] =
    include_bytes!("../../../examples/hello-gpu/assets/fonts/NotoSans-Regular.ttf");
const BACK: VectorIcon = VectorIcon::stroked("M15 6l-6 6 6 6");

#[derive(Clone, Copy, PartialEq)]
enum Variant {
    /// No semantics modifier anywhere.
    Plain,
    /// Roles, states, merging and exclusion: everything that is a flag.
    Flags,
    /// The flags, and labels and values on `LABELLED_WIDGETS` widgets.
    Labels,
}

/// How many widgets the `Labels` variant gives strings: the back button,
/// and in each row the picture (a label) and the bar (a label and a value,
/// which share one box).
const LABELLED_WIDGETS: usize = 2 * ROWS + 1;
const ROWS: usize = 12;

struct Screen(Variant);

impl Application for Screen {
    type Message = ();

    fn setup(&mut self, setup: &mut Setup<'_>) {
        setup.add_font(REGULAR);
    }

    fn view(&self) -> Element<()> {
        let variant = self.0;
        let flags = variant != Variant::Plain;
        let labels = variant == Variant::Labels;
        let mut title = text("Repairs").title();
        if flags {
            title = title.heading();
        }
        let mut back = Pressable::new(Icon::vector(BACK).size(24.0))
            .size(48.0, 48.0)
            .on_press(());
        if labels {
            back = back.label("Back");
        }
        let mut list = Column::new().fill_width();
        for index in 0..ROWS {
            let mut picture = Icon::vector(BACK).size(24.0);
            let mut amount = text("TSh 45,000");
            let mut bar = Container::empty()
                .size(120.0, 4.0)
                .background(hex(0x2C2C2E));
            let mut row_flags = Container::new(text("Screen repair")).padding(4.0);
            if flags {
                amount = amount.exclude_semantics();
                bar = bar.semantic_role(Role::Progress);
                row_flags = row_flags
                    .merge_semantics()
                    .selected(index == 0)
                    .live(Live::Polite);
            }
            if labels {
                picture = picture.label("Phone");
                bar = bar.label("Progress").value("40 percent");
            }
            list = list.push(
                row![
                    picture,
                    row_flags,
                    bar,
                    amount,
                    Button::new("Open").on_press(())
                ]
                .gap(8.0)
                .fill_width(),
            );
        }
        col![row![back, title], ScrollView::new(list)].fill().into()
    }

    fn update(&mut self, (): ()) {}
}

/// Allocations of one warmed-up frame, `view` included.
fn frame_allocations(ui: &mut Ui<Screen>, frame: &mut Frame, info: &FrameInfo) -> usize {
    // Warm up: tables reach their working size.
    for _ in 0..3 {
        frame.begin();
        ui.frame(info, frame);
    }
    frame.begin();
    ALLOCATIONS.store(0, Ordering::Relaxed);
    COUNTING.with(|counting| counting.set(true));
    ui.frame(info, frame);
    COUNTING.with(|counting| counting.set(false));
    ALLOCATIONS.load(Ordering::Relaxed)
}

fn measure(variant: Variant, accessibility: bool) -> usize {
    let mut ui = Ui::new(Screen(variant));
    let mut frame = Frame::for_app(&mut ui);
    let info = FrameInfo::new(SurfaceSize::new(1080, 2400))
        .with_scale(3.0)
        .with_accessibility(accessibility);
    frame_allocations(&mut ui, &mut frame, &info)
}

#[test]
fn accessibility_that_is_off_allocates_nothing() {
    let plain = measure(Variant::Plain, false);
    let flags = measure(Variant::Flags, false);
    let labels = measure(Variant::Labels, false);
    println!("off: plain {plain}, flags {flags}, labels {labels} allocations per frame");
    // Roles, states, merging, exclusion and live regions are bits in the
    // element: free.
    assert_eq!(flags, plain, "flag modifiers allocate while off");
    // A widget given strings boxes them once, in `view`. (Static strings
    // are borrowed, so the box is all there is.)
    assert_eq!(
        labels,
        plain + LABELLED_WIDGETS,
        "labels cost more than their one box each while off"
    );

    // The count can tell: described, the same frame allocates more.
    let on = measure(Variant::Labels, true);
    println!("on: labels {on} allocations per frame");
    assert!(on > labels + 20, "on {on}, off {labels}");

    // Switched off again, a `Ui` is back to exactly what it cost before,
    // and holds no tree.
    let mut ui = Ui::new(Screen(Variant::Labels));
    let mut frame = Frame::for_app(&mut ui);
    let off = FrameInfo::new(SurfaceSize::new(1080, 2400)).with_scale(3.0);
    frame_allocations(&mut ui, &mut frame, &off.with_accessibility(true));
    assert!(App::semantics(&ui).is_some());
    assert_eq!(frame_allocations(&mut ui, &mut frame, &off), labels);
    assert!(App::semantics(&ui).is_none());
}
