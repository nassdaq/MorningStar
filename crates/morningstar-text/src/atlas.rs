//! The coverage atlas: one single-channel image holding every rasterised
//! glyph and vector icon in use.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use cosmic_text::CacheKey;
use morningstar_core::{AtlasView, DirtyRows};

const WIDTH: u32 = 1024;
const INITIAL_HEIGHT: u32 = 512;
/// Within the smallest maximum texture size Morningstar's GPU limits allow.
const MAX_HEIGHT: u32 = 2048;
/// Empty texels between glyphs so filtering never bleeds one into the next.
const PADDING: u32 = 1;

/// Source of generations. Shared by every atlas in the process, so a
/// generation identifies one state of one atlas and a renderer can never
/// mistake a view of one atlas for a newer version of another. Starts above
/// `AtlasView::EMPTY`'s generation.
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

fn next_generation() -> u64 {
    NEXT_GENERATION.fetch_add(1, Ordering::Relaxed)
}

/// What a slot of the atlas holds: a glyph of a font, or a vector icon
/// rasterised at one size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum AtlasKey {
    /// A glyph, and the tone of the colour it is weighted for (see
    /// `context::tone`): the same glyph drawn dark and light is two images.
    Glyph(CacheKey, u8),
    Icon(crate::icon::IconKey),
}

impl From<CacheKey> for AtlasKey {
    fn from(key: CacheKey) -> Self {
        Self::Glyph(key, 0)
    }
}

impl From<(CacheKey, u8)> for AtlasKey {
    fn from((key, tone): (CacheKey, u8)) -> Self {
        Self::Glyph(key, tone)
    }
}

impl From<crate::icon::IconKey> for AtlasKey {
    fn from(key: crate::icon::IconKey) -> Self {
        Self::Icon(key)
    }
}

/// Where a glyph's image sits in the atlas and how to place it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Slot {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// Offset from the pen position to the image's left edge.
    pub left: i32,
    /// Offset from the baseline up to the image's top edge.
    pub top: i32,
}

/// Result of asking the atlas for a glyph.
pub(crate) enum Lookup {
    /// The glyph is in the atlas.
    Found(Slot),
    /// The glyph has no image (a space, or a kind not supported yet).
    Blank,
    /// Not rasterised yet.
    Missing,
}

/// A shelf-packed, single-channel atlas.
///
/// Glyphs are placed left to right in rows ("shelves") as tall as their
/// tallest glyph. When full it grows downwards, which keeps existing
/// positions valid, up to `MAX_HEIGHT`.
///
/// It records which rows changed since the last
/// [`mark_clean`](Self::mark_clean), so the renderer can upload only those.
/// See [`DirtyRows`].
pub(crate) struct Atlas {
    height: u32,
    pixels: Vec<u8>,
    /// `None` records a glyph known to be blank, so it is not rasterised again.
    slots: HashMap<AtlasKey, Option<Slot>>,
    shelf_x: u32,
    shelf_y: u32,
    shelf_height: u32,
    generation: u64,
    /// The generation at the last `mark_clean`.
    clean_generation: u64,
    /// Rows written since then, as `start..end`. Empty when equal.
    dirty_rows: (u32, u32),
    /// Set when the image changed since then in a way rows cannot describe:
    /// it was resized or cleared.
    replaced: bool,
}

impl Atlas {
    pub fn new() -> Self {
        let generation = next_generation();
        Self {
            height: INITIAL_HEIGHT,
            pixels: vec![0; (WIDTH * INITIAL_HEIGHT) as usize],
            slots: HashMap::new(),
            shelf_x: PADDING,
            shelf_y: PADDING,
            shelf_height: 0,
            generation,
            clean_generation: generation,
            dirty_rows: (0, 0),
            // Nobody has seen this atlas yet.
            replaced: true,
        }
    }

    /// Starts a new record of changes. Call at a point where a renderer may
    /// have uploaded the current contents, i.e. once per frame.
    ///
    /// Correctness does not depend on when this is called: a renderer that
    /// does not hold the generation current at this moment (or a later one)
    /// ignores the record and uploads everything.
    pub fn mark_clean(&mut self) {
        self.clean_generation = self.generation;
        self.dirty_rows = (0, 0);
        self.replaced = false;
    }

    pub fn view(&self) -> AtlasView<'_> {
        AtlasView {
            width: WIDTH,
            height: self.height,
            pixels: &self.pixels,
            generation: self.generation,
            dirty: (!self.replaced).then_some(DirtyRows {
                since: self.clean_generation,
                start: self.dirty_rows.0,
                end: self.dirty_rows.1,
            }),
        }
    }

    pub fn lookup(&self, key: impl Into<AtlasKey>) -> Lookup {
        match self.slots.get(&key.into()) {
            Some(Some(slot)) => Lookup::Found(*slot),
            Some(None) => Lookup::Blank,
            None => Lookup::Missing,
        }
    }

    pub fn insert_blank(&mut self, key: impl Into<AtlasKey>) {
        self.slots.insert(key.into(), None);
    }

    /// Copies a `width * height` coverage image into the atlas. Returns
    /// `None` if the atlas is full even at its maximum size.
    pub fn insert(
        &mut self,
        key: impl Into<AtlasKey>,
        width: u32,
        height: u32,
        left: i32,
        top: i32,
        coverage: &[u8],
    ) -> Option<Slot> {
        debug_assert_eq!(coverage.len(), (width * height) as usize);
        if width + 2 * PADDING > WIDTH {
            return None;
        }

        // Start a new shelf if this one is out of room.
        if self.shelf_x + width + PADDING > WIDTH {
            self.shelf_y += self.shelf_height + PADDING;
            self.shelf_x = PADDING;
            self.shelf_height = 0;
        }
        let bottom = self.shelf_y + height + PADDING;
        if bottom > self.height {
            let mut grown = self.height;
            while grown < bottom && grown < MAX_HEIGHT {
                grown *= 2;
            }
            if grown < bottom {
                return None;
            }
            self.height = grown;
            self.pixels.resize((WIDTH * grown) as usize, 0);
            // A different size means a new texture.
            self.replaced = true;
        }

        let slot = Slot {
            x: self.shelf_x,
            y: self.shelf_y,
            width,
            height,
            left,
            top,
        };
        for row in 0..height {
            let source = (row * width) as usize;
            let target = ((slot.y + row) * WIDTH + slot.x) as usize;
            self.pixels[target..target + width as usize]
                .copy_from_slice(&coverage[source..source + width as usize]);
        }

        self.shelf_x += width + PADDING;
        self.shelf_height = self.shelf_height.max(height);
        self.dirty_rows = if self.dirty_rows.0 == self.dirty_rows.1 {
            (slot.y, slot.y + height)
        } else {
            (
                self.dirty_rows.0.min(slot.y),
                self.dirty_rows.1.max(slot.y + height),
            )
        };
        self.generation = next_generation();
        self.slots.insert(key.into(), Some(slot));
        Some(slot)
    }

    /// Forgets every glyph and shrinks back to the initial size.
    pub fn clear(&mut self) {
        self.height = INITIAL_HEIGHT;
        self.pixels.clear();
        self.pixels.resize((WIDTH * INITIAL_HEIGHT) as usize, 0);
        self.slots.clear();
        self.shelf_x = PADDING;
        self.shelf_y = PADDING;
        self.shelf_height = 0;
        self.replaced = true;
        self.generation = next_generation();
    }
}

#[cfg(test)]
mod tests {
    use cosmic_text::{CacheKeyFlags, fontdb};

    use super::*;

    fn key(glyph: u16) -> CacheKey {
        CacheKey::new(
            fontdb::ID::dummy(),
            glyph,
            16.0,
            (0.0, 0.0),
            fontdb::Weight::NORMAL,
            CacheKeyFlags::empty(),
        )
        .0
    }

    /// Inserts a `width * height` glyph filled with `value`.
    fn insert(atlas: &mut Atlas, glyph: u16, width: u32, height: u32, value: u8) -> Option<Slot> {
        let coverage = vec![value; (width * height) as usize];
        atlas.insert(key(glyph), width, height, 0, 0, &coverage)
    }

    #[test]
    fn a_new_atlas_must_be_uploaded_whole() {
        let mut atlas = Atlas::new();
        assert_eq!(atlas.view().dirty, None);
        insert(&mut atlas, 0, 4, 4, 255);
        assert_eq!(atlas.view().dirty, None, "still never marked clean");
    }

    #[test]
    fn dirty_rows_cover_exactly_the_glyphs_added_since_marked_clean() {
        let mut atlas = Atlas::new();
        insert(&mut atlas, 0, 10, 20, 255);
        atlas.mark_clean();
        let clean = atlas.view().generation;
        assert_eq!(
            atlas.view().dirty,
            Some(DirtyRows {
                since: clean,
                start: 0,
                end: 0
            }),
            "nothing changed yet"
        );

        // Same shelf, shorter: rows 1..9.
        let a = insert(&mut atlas, 1, 10, 8, 255).unwrap();
        assert_eq!((a.y, a.height), (PADDING, 8));
        let view = atlas.view();
        assert!(view.generation > clean);
        assert_eq!(
            view.dirty,
            Some(DirtyRows {
                since: clean,
                start: 1,
                end: 9
            })
        );

        // Wide enough to start a second shelf below the 20-row first one.
        let b = insert(&mut atlas, 2, WIDTH - 2 * PADDING, 5, 255).unwrap();
        assert_eq!(b.y, PADDING + 20 + PADDING);
        assert_eq!(
            atlas.view().dirty,
            Some(DirtyRows {
                since: clean,
                start: 1,
                end: b.y + 5
            }),
            "the range grows to hold both and stays relative to the same generation"
        );

        // Everything outside the dirty rows is as it was when marked clean,
        // which here means: every non-zero texel outside them belongs to the
        // glyph inserted before.
        let view = atlas.view();
        let dirty = view.dirty.unwrap();
        for (index, &texel) in view.pixels.iter().enumerate() {
            let (x, y) = (index as u32 % WIDTH, index as u32 / WIDTH);
            if texel != 0 && !(dirty.start..dirty.end).contains(&y) {
                assert!(x <= 10 && y <= 20, "unexpected texel at {x},{y}");
            }
        }

        atlas.mark_clean();
        let dirty = atlas.view().dirty.unwrap();
        assert_eq!(dirty.since, atlas.view().generation);
        assert_eq!(dirty.start, dirty.end);
    }

    #[test]
    fn blank_glyphs_change_nothing() {
        let mut atlas = Atlas::new();
        atlas.mark_clean();
        let before = atlas.view().generation;
        atlas.insert_blank(key(7));
        assert_eq!(atlas.view().generation, before);
        assert_eq!(
            atlas.view().dirty.unwrap().start,
            atlas.view().dirty.unwrap().end
        );
    }

    #[test]
    fn growing_invalidates_the_whole_image_until_marked_clean() {
        let mut atlas = Atlas::new();
        atlas.mark_clean();
        // Three shelves of 200 rows do not fit in the initial 512.
        for glyph in 0..3 {
            insert(&mut atlas, glyph, WIDTH - 2 * PADDING, 200, 255).unwrap();
        }
        let view = atlas.view();
        assert_eq!(view.height, 2 * INITIAL_HEIGHT);
        assert_eq!(view.pixels.len(), (WIDTH * view.height) as usize);
        assert_eq!(view.dirty, None, "a resized atlas is a new texture");

        // Later glyphs in the same frame do not bring row tracking back.
        insert(&mut atlas, 10, 4, 4, 255).unwrap();
        assert_eq!(atlas.view().dirty, None);

        atlas.mark_clean();
        let clean = atlas.view().generation;
        let slot = insert(&mut atlas, 11, 4, 4, 255).unwrap();
        assert_eq!(
            atlas.view().dirty,
            Some(DirtyRows {
                since: clean,
                start: slot.y,
                end: slot.y + 4
            })
        );
    }

    #[test]
    fn clearing_invalidates_the_whole_image_until_marked_clean() {
        let mut atlas = Atlas::new();
        insert(&mut atlas, 0, 8, 8, 255).unwrap();
        atlas.mark_clean();
        let before = atlas.view().generation;

        atlas.clear();
        let view = atlas.view();
        assert!(view.generation > before);
        assert_eq!(view.dirty, None);
        assert!(view.pixels.iter().all(|&texel| texel == 0));
        assert!(matches!(atlas.lookup(key(0)), Lookup::Missing));

        // Re-rasterised in the same frame as the clear: still a full upload.
        insert(&mut atlas, 0, 8, 8, 255).unwrap();
        assert_eq!(atlas.view().dirty, None);
    }

    #[test]
    fn generations_are_unique_across_atlases() {
        let mut a = Atlas::new();
        let mut b = Atlas::new();
        assert_ne!(a.view().generation, b.view().generation);
        insert(&mut a, 0, 4, 4, 255).unwrap();
        insert(&mut b, 0, 4, 4, 255).unwrap();
        assert_ne!(a.view().generation, b.view().generation);
    }
}
