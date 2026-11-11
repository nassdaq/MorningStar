//! Renders the app to PNG files on this machine's GPU. No device needed.
//!
//!   cargo run -p hello-gpu --example screenshot [output-dir]
//!
//! Writes the app idle, with the button held, and after seven taps, on a
//! phone in portrait, plus the idle screen in landscape with a display cutout
//! on the left. The last picture of each screen is also written with the
//! safe area outlined, to show that content stays inside it.

use std::path::PathBuf;
use std::time::Duration;

use hello_gpu::HelloGpu;
use morningstar::prelude::*;
use morningstar_devtools::encode_png;

/// A common phone: 1080 x 2400 pixels at 3 px per dp (360 x 800 dp), with a
/// status bar and a gesture bar.
const SCALE: f32 = 3.0;
const PORTRAIT: SurfaceSize = SurfaceSize::new(1080, 2400);
const PORTRAIT_INSETS: Insets = Insets::new(0.0, 96.0, 0.0, 60.0);
/// The same phone on its side: the cutout is now on the left.
const LANDSCAPE: SurfaceSize = SurfaceSize::new(2400, 1080);
const LANDSCAPE_INSETS: Insets = Insets::new(96.0, 0.0, 0.0, 60.0);

/// Long after any animation has finished.
const SETTLED: Duration = Duration::from_secs(60);

fn portrait() -> FrameInfo {
    FrameInfo::new(PORTRAIT)
        .with_scale(SCALE)
        .with_insets(PORTRAIT_INSETS)
}

fn landscape() -> FrameInfo {
    FrameInfo::new(LANDSCAPE)
        .with_scale(SCALE)
        .with_insets(LANDSCAPE_INSETS)
}

/// Draws one frame of `app` and returns the frame, as the engine would.
fn draw(app: &mut HelloGpu, info: FrameInfo) -> Frame {
    let mut frame = Frame::for_app(app);
    frame.begin();
    app.frame(&info, &mut frame);
    frame
}

/// Renders a frame to PNG bytes.
///
/// `morningstar_devtools::screenshot` does the same but always draws at a
/// scale of 1 with no insets, which is not what a phone shows. Until it can
/// be given a `FrameInfo`, this example builds the frame itself.
fn render(frame: &Frame, size: SurfaceSize) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut pixels = morningstar_gpu::render_offscreen_with_images(
        size,
        frame.clear,
        &frame.list,
        frame.atlas(),
        frame.images(),
    )?;
    // The renderer works in linear light; image files are sRGB-encoded.
    for pixel in pixels.chunks_exact_mut(4) {
        for channel in &mut pixel[..3] {
            let linear = f32::from(*channel) / 255.0;
            let encoded = if linear <= 0.003_130_8 {
                linear * 12.92
            } else {
                1.055 * linear.powf(1.0 / 2.4) - 0.055
            };
            *channel = (encoded * 255.0).round() as u8;
        }
    }
    Ok(encode_png(size.width, size.height, &pixels))
}

/// Outlines the safe area, for checking by eye that content respects it.
fn outline_safe_area(frame: &mut Frame, info: &FrameInfo) {
    let safe = info.safe_area();
    let line = info.dp(1.0);
    let color = Color::rgba(1.0, 0.2, 0.6, 0.9);
    for edge in [
        Rect::new(safe.x, safe.y, safe.width, line),
        Rect::new(safe.x, safe.y + safe.height - line, safe.width, line),
        Rect::new(safe.x, safe.y, line, safe.height),
        Rect::new(safe.x + safe.width - line, safe.y, line, safe.height),
    ] {
        frame.list.fill_rect(edge, color);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".to_owned()));
    std::fs::create_dir_all(&directory)?;

    let save = |frame: &Frame, size: SurfaceSize, name: &str| {
        let path = directory.join(name);
        std::fs::write(&path, render(frame, size)?)?;
        println!("{}", path.display());
        Ok::<_, Box<dyn std::error::Error>>(())
    };

    let mut app = HelloGpu::default();
    let idle = draw(&mut app, portrait());
    assert!(!idle.redraw_requested(), "an untouched app must be idle");
    save(&idle, PORTRAIT, "idle.png")?;

    // The app hit-tests against the frame it last drew, so find the button
    // the same way a user does: it is the one rectangle on screen.
    let button = idle
        .list
        .ops()
        .iter()
        .find_map(|op| match *op {
            morningstar::DrawOp::Rect { rect, .. } => Some(rect.center()),
            _ => None,
        })
        .expect("the app draws a button");
    let touch = |app: &mut HelloGpu, phase| {
        app.pointer(&PointerEvent {
            id: 1,
            phase,
            position: button,
        });
    };

    touch(&mut app, PointerPhase::Down);
    save(&draw(&mut app, portrait()), PORTRAIT, "pressed.png")?;
    touch(&mut app, PointerPhase::Up);

    for _ in 0..6 {
        touch(&mut app, PointerPhase::Down);
        touch(&mut app, PointerPhase::Up);
    }
    assert_eq!(app.taps(), 7);
    // The frame after a tap starts the colour animation; a later one shows
    // where it ends up.
    assert!(draw(&mut app, portrait()).redraw_requested());
    let mut settled = draw(&mut app, portrait().with_elapsed(SETTLED));
    assert!(!settled.redraw_requested(), "the animation must end");
    save(&settled, PORTRAIT, "seven-taps.png")?;
    outline_safe_area(&mut settled, &portrait());
    save(&settled, PORTRAIT, "seven-taps-safe-area.png")?;

    let info = landscape().with_elapsed(SETTLED);
    let mut rotated = draw(&mut app, info);
    outline_safe_area(&mut rotated, &info);
    save(&rotated, LANDSCAPE, "landscape-safe-area.png")?;
    Ok(())
}
