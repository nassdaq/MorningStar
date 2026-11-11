//! Tests of incremental glyph-atlas uploads (ADR 0011): when an
//! [`AtlasView`] says which rows changed, only those reach the GPU, and the
//! texture is still right afterwards.
//!
//! They use a renderer that lives across frames, as the on-screen one does,
//! and hand-built atlases, which are plain data. The text crate's own tests
//! check that its atlas reports dirty rows that cover every change.

use morningstar_core::{AtlasView, Color, DirtyRows, DisplayList, Rect, SurfaceSize};
use morningstar_gpu::{AtlasUpload, GpuError, OffscreenRenderer};

const WIDTH: u32 = 8;

fn renderer() -> Option<OffscreenRenderer> {
    match OffscreenRenderer::new() {
        Ok(renderer) => Some(renderer),
        Err(GpuError::NoAdapter(reason)) => {
            eprintln!("skipping: no GPU adapter available ({reason})");
            None
        }
        Err(other) => panic!("could not create renderer: {other}"),
    }
}

/// Draws the whole atlas one texel per pixel, white on black, and returns
/// what the GPU's copy of it holds: the red channel of each pixel.
fn atlas_on_gpu(renderer: &mut OffscreenRenderer, atlas: AtlasView<'_>) -> Vec<u8> {
    let whole = Rect::new(0.0, 0.0, atlas.width as f32, atlas.height as f32);
    let mut list = DisplayList::new();
    list.draw_glyph(whole, whole, Color::WHITE);
    let size = SurfaceSize::new(atlas.width, atlas.height);
    let pixels = renderer
        .render(size, Color::BLACK, &list, atlas)
        .expect("render");
    pixels.chunks_exact(4).map(|pixel| pixel[0]).collect()
}

fn view(pixels: &[u8], generation: u64, dirty: Option<DirtyRows>) -> AtlasView<'_> {
    AtlasView {
        width: WIDTH,
        height: pixels.len() as u32 / WIDTH,
        pixels,
        generation,
        dirty,
    }
}

fn fill_row(pixels: &mut [u8], row: u32, value: u8) {
    let start = (row * WIDTH) as usize;
    pixels[start..start + WIDTH as usize].fill(value);
}

#[test]
fn only_dirty_rows_are_uploaded_and_the_texture_stays_correct() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut cpu: Vec<u8> = (0..WIDTH * 8).map(|i| (i * 3) as u8).collect();

    // First sight of the atlas: everything, whatever the view claims.
    let claims_rows = Some(DirtyRows {
        since: 9,
        start: 0,
        end: 1,
    });
    assert_eq!(
        atlas_on_gpu(&mut renderer, view(&cpu, 10, claims_rows)),
        cpu
    );
    assert_eq!(renderer.last_atlas_upload(), AtlasUpload::Full);

    // Same generation: nothing.
    assert_eq!(atlas_on_gpu(&mut renderer, view(&cpu, 10, None)), cpu);
    assert_eq!(renderer.last_atlas_upload(), AtlasUpload::None);

    // A glyph lands in rows 5..7.
    fill_row(&mut cpu, 5, 200);
    fill_row(&mut cpu, 6, 201);
    let dirty = Some(DirtyRows {
        since: 10,
        start: 5,
        end: 7,
    });
    assert_eq!(atlas_on_gpu(&mut renderer, view(&cpu, 11, dirty)), cpu);
    assert_eq!(
        renderer.last_atlas_upload(),
        AtlasUpload::Rows { start: 5, end: 7 }
    );

    // Several additions within one frame: the record still starts at the
    // generation the renderer holds and covers all of them.
    fill_row(&mut cpu, 0, 210);
    fill_row(&mut cpu, 7, 211);
    let dirty = Some(DirtyRows {
        since: 11,
        start: 0,
        end: 8,
    });
    assert_eq!(atlas_on_gpu(&mut renderer, view(&cpu, 14, dirty)), cpu);
    assert_eq!(
        renderer.last_atlas_upload(),
        AtlasUpload::Rows { start: 0, end: 8 }
    );

    // A new generation with nothing dirty uploads nothing.
    let dirty = Some(DirtyRows {
        since: 14,
        start: 3,
        end: 3,
    });
    assert_eq!(atlas_on_gpu(&mut renderer, view(&cpu, 15, dirty)), cpu);
    assert_eq!(renderer.last_atlas_upload(), AtlasUpload::None);
}

#[test]
fn rows_outside_the_dirty_range_are_not_touched() {
    // Proof that the upload really is partial: a change the view does not
    // declare does not reach the GPU.
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut cpu = vec![50u8; (WIDTH * 8) as usize];
    atlas_on_gpu(&mut renderer, view(&cpu, 1, None));

    fill_row(&mut cpu, 2, 99); // undeclared
    fill_row(&mut cpu, 5, 200); // declared
    let dirty = Some(DirtyRows {
        since: 1,
        start: 5,
        end: 6,
    });
    let gpu = atlas_on_gpu(&mut renderer, view(&cpu, 2, dirty));
    let mut expected = cpu.clone();
    fill_row(&mut expected, 2, 50);
    assert_eq!(gpu, expected);

    // A full upload then brings the undeclared change across too.
    assert_eq!(atlas_on_gpu(&mut renderer, view(&cpu, 3, None)), cpu);
    assert_eq!(renderer.last_atlas_upload(), AtlasUpload::Full);
}

#[test]
fn a_record_the_renderer_cannot_use_falls_back_to_a_full_upload() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut cpu = vec![10u8; (WIDTH * 8) as usize];
    atlas_on_gpu(&mut renderer, view(&cpu, 20, None));

    // The renderer missed a frame: it holds 20, but the record is relative
    // to 22. Rows changed in between (row 1) are not in it.
    fill_row(&mut cpu, 1, 77);
    fill_row(&mut cpu, 6, 88);
    let dirty = Some(DirtyRows {
        since: 22,
        start: 6,
        end: 7,
    });
    assert_eq!(atlas_on_gpu(&mut renderer, view(&cpu, 23, dirty)), cpu);
    assert_eq!(renderer.last_atlas_upload(), AtlasUpload::Full);

    // A record that does not fit the image is ignored, not trusted.
    fill_row(&mut cpu, 3, 66);
    let dirty = Some(DirtyRows {
        since: 23,
        start: 3,
        end: 900,
    });
    assert_eq!(atlas_on_gpu(&mut renderer, view(&cpu, 24, dirty)), cpu);
    assert_eq!(renderer.last_atlas_upload(), AtlasUpload::Full);

    // So is one from a renderer's point of view in the future.
    fill_row(&mut cpu, 4, 55);
    let dirty = Some(DirtyRows {
        since: 10,
        start: 4,
        end: 5,
    });
    assert_eq!(atlas_on_gpu(&mut renderer, view(&cpu, 5, dirty)), cpu);
    assert_eq!(renderer.last_atlas_upload(), AtlasUpload::Full);
}

#[test]
fn growth_and_clear_replace_the_whole_texture() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut cpu: Vec<u8> = (0..WIDTH * 4).map(|i| 100 + i as u8).collect();
    atlas_on_gpu(&mut renderer, view(&cpu, 1, None));

    // The atlas grows from 4 rows to 8, keeping what it had and adding a
    // glyph in the new part. Even a view that (wrongly) claimed only rows
    // changed must be uploaded whole: the texture is a new one.
    cpu.resize((WIDTH * 8) as usize, 0);
    fill_row(&mut cpu, 6, 250);
    let dirty = Some(DirtyRows {
        since: 1,
        start: 6,
        end: 7,
    });
    assert_eq!(atlas_on_gpu(&mut renderer, view(&cpu, 2, dirty)), cpu);
    assert_eq!(renderer.last_atlas_upload(), AtlasUpload::Full);

    // Rows work again at the new size.
    fill_row(&mut cpu, 7, 251);
    let dirty = Some(DirtyRows {
        since: 2,
        start: 7,
        end: 8,
    });
    assert_eq!(atlas_on_gpu(&mut renderer, view(&cpu, 3, dirty)), cpu);
    assert_eq!(
        renderer.last_atlas_upload(),
        AtlasUpload::Rows { start: 7, end: 8 }
    );

    // The atlas is cleared and shrinks back: nothing of the old one may
    // survive, including in rows the new contents leave empty.
    let mut cleared = vec![0u8; (WIDTH * 4) as usize];
    fill_row(&mut cleared, 1, 123);
    assert_eq!(
        atlas_on_gpu(&mut renderer, view(&cleared, 4, None)),
        cleared
    );
    assert_eq!(renderer.last_atlas_upload(), AtlasUpload::Full);

    // A clear that keeps the size (same texture) must also wipe old rows.
    let wiped = vec![0u8; (WIDTH * 4) as usize];
    assert_eq!(atlas_on_gpu(&mut renderer, view(&wiped, 5, None)), wiped);
    assert_eq!(renderer.last_atlas_upload(), AtlasUpload::Full);
}

#[test]
fn a_frame_that_draws_nothing_does_not_lose_track() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut cpu = vec![40u8; (WIDTH * 4) as usize];
    atlas_on_gpu(&mut renderer, view(&cpu, 1, None));

    // An empty frame skips the atlas entirely, so the renderer still holds
    // generation 1 while the atlas moves on to 2 and then 3.
    fill_row(&mut cpu, 0, 90);
    let size = SurfaceSize::new(WIDTH, 4);
    let dirty = Some(DirtyRows {
        since: 1,
        start: 0,
        end: 1,
    });
    renderer
        .render(
            size,
            Color::BLACK,
            &DisplayList::new(),
            view(&cpu, 2, dirty),
        )
        .expect("render");
    assert_eq!(renderer.last_atlas_upload(), AtlasUpload::None);

    fill_row(&mut cpu, 3, 91);
    let dirty = Some(DirtyRows {
        since: 2,
        start: 3,
        end: 4,
    });
    assert_eq!(atlas_on_gpu(&mut renderer, view(&cpu, 3, dirty)), cpu);
    assert_eq!(renderer.last_atlas_upload(), AtlasUpload::Full);
}
