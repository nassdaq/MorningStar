//! The host benchmark: what a frame of each example screen costs on the
//! machine this runs on, with no phone (`benchmarks/README.md`, "Host
//! baseline").
//!
//!   cargo run --release -p morningstar-bench -- [--frames N] [--markdown]
//!
//! For every designed screen of FundiDesk, CallReady, Atlas and Barua Talk
//! it measures, in a Galaxy A27's frame (1080 x 2340 px at 2.8125 px per
//! dp, with its insets):
//!
//! * **build**: the application's whole share of a frame, from nothing to
//!   a display list: `view`, layout, text shaping (cached after the first
//!   frame) and painting into the list. CPU only.
//! * **draw**: the renderer's share for that list on this machine's GPU,
//!   into an offscreen target, **including reading the pixels back**,
//!   which a phone never does. The cost of an empty frame of the same
//!   size, read back the same way, is printed beside it; the difference
//!   is the nearest this method gets to what the screen itself costs.
//!
//! And for video (ADR 0039), per 1280 x 720 frame: copying the three
//! planes out of a decoder's buffers, and uploading and converting them.
//!
//! This is a baseline for noticing regressions on one machine. It says
//! nothing about a phone: another GPU, another driver, a compositor and a
//! display in the way. Device numbers are `benchmarks/android/measure.sh`.

use std::time::{Duration, Instant};

use morningstar::ui::prelude::*;
use morningstar::ui::{LiveVideo, Ui};
use morningstar::{
    App, Color, DisplayList, Frame, FrameInfo, Insets, SurfaceSize, VideoFrame, VideoRotation,
    VideoTrack,
};
use morningstar_gpu::OffscreenRenderer;

/// Frames drawn before measuring: fonts shaped, glyphs and images uploaded.
const WARM_UP: usize = 5;

/// A Galaxy A27, the one phone this project has measured on.
fn phone() -> FrameInfo {
    FrameInfo::new(SurfaceSize::new(1080, 2340))
        .with_scale(2.8125)
        .with_insets(Insets::new(0.0, 102.0, 0.0, 42.0))
}

/// Milliseconds: the median, the 95th percentile and the least of `samples`.
struct Spread {
    median: f64,
    p95: f64,
    min: f64,
}

fn spread(samples: &mut [Duration]) -> Spread {
    samples.sort_unstable();
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    let at = |fraction: f64| {
        let index = ((samples.len() - 1) as f64 * fraction).round() as usize;
        ms(samples[index])
    };
    Spread {
        median: at(0.5),
        p95: at(0.95),
        min: ms(samples[0]),
    }
}

struct Row {
    name: String,
    first: f64,
    build: Spread,
    draw: Spread,
    instances: u32,
    draw_calls: u32,
    passes: u32,
}

/// Measures one screen: `app` is already in the state to draw.
fn measure(
    name: String,
    app: &mut dyn App,
    renderer: &mut OffscreenRenderer,
    frames: usize,
) -> Row {
    let info = phone();
    let mut frame = Frame::for_app(app);
    let mut one = |frame: &mut Frame, renderer: &mut OffscreenRenderer, index: usize| {
        // The clock moves as it would at 60 frames a second.
        let info = info
            .with_elapsed(Duration::from_secs(1) + Duration::from_micros(16_667 * index as u64));
        let start = Instant::now();
        frame.begin();
        app.frame(&info, frame);
        let built = Instant::now();
        let pixels = renderer
            .render_with_images(
                info.size,
                frame.clear,
                &frame.list,
                frame.atlas(),
                frame.images(),
            )
            .expect("this machine has a GPU the offscreen renderer can use");
        let drawn = Instant::now();
        std::hint::black_box(pixels);
        (built - start, drawn - built)
    };
    let (first_build, first_draw) = one(&mut frame, renderer, 0);
    for index in 1..WARM_UP {
        one(&mut frame, renderer, index);
    }
    let (mut builds, mut draws) = (Vec::with_capacity(frames), Vec::with_capacity(frames));
    for index in 0..frames {
        let (build, draw) = one(&mut frame, renderer, WARM_UP + index);
        builds.push(build);
        draws.push(draw);
    }
    let stats = renderer.last_stats();
    Row {
        name,
        first: (first_build + first_draw).as_secs_f64() * 1000.0,
        build: spread(&mut builds),
        draw: spread(&mut draws),
        instances: stats.instances,
        draw_calls: stats.draw_calls,
        passes: stats.passes,
    }
}

/// An application that draws nothing: the cost of a frame as such.
struct Nothing;

impl App for Nothing {
    fn frame(&mut self, _info: &FrameInfo, frame: &mut Frame) {
        frame.clear = Color::BLACK;
        frame.list = DisplayList::default();
    }
}

/// One video, full screen: what a call screen is underneath.
struct Picture(VideoTrack);

impl Application for Picture {
    type Message = ();

    fn view(&self) -> Element<()> {
        LiveVideo::new(self.0).into()
    }

    fn update(&mut self, _message: ()) {}
}

/// A 1280 x 720 frame as a decoder hands it over: three planes, the luma
/// a gradient so that the bytes are not all alike.
fn planes(shift: u8) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let (w, h) = (1280_usize, 720_usize);
    let y = (0..w * h)
        .map(|i| 16 + (((i % w) + (i / w)) as u8).wrapping_add(shift) % 220)
        .collect();
    (y, vec![110; w * h / 4], vec![150; w * h / 4])
}

struct VideoCost {
    copy: Spread,
    draw_same: Spread,
    draw_new: Spread,
}

fn measure_video(renderer: &mut OffscreenRenderer, frames: usize) -> VideoCost {
    let track = VideoTrack(1);
    let info = phone();
    let sources: Vec<_> = (0..4).map(|n| planes(n * 40)).collect();
    let make = |n: usize, reuse: Option<VideoFrame>| {
        let (y, u, v) = &sources[n % sources.len()];
        VideoFrame::from_i420(
            1280,
            720,
            VideoRotation::None,
            (y, 1280),
            (u, 640),
            (v, 640),
            reuse,
        )
        .expect("a 720p frame")
    };

    // The copy out of the decoder's buffers, into a frame that is reused.
    let mut copies = Vec::with_capacity(frames);
    let mut held = Some(make(0, None));
    for n in 0..frames {
        let start = Instant::now();
        held = Some(make(n, held.take()));
        copies.push(start.elapsed());
    }

    let mut ui = Ui::new(Picture(track));
    let mut frame = Frame::for_app(&mut ui);
    let mut draw = |frame: &mut Frame, renderer: &mut OffscreenRenderer, new: Option<usize>| {
        if let Some(n) = new {
            frame.set_video_frame(track, make(n, None));
        }
        frame.begin();
        ui.frame(&info, frame);
        let start = Instant::now();
        let pixels = renderer
            .render_with_images(
                info.size,
                frame.clear,
                &frame.list,
                frame.atlas(),
                frame.images(),
            )
            .expect("this machine has a GPU the offscreen renderer can use");
        let took = start.elapsed();
        std::hint::black_box(pixels);
        took
    };
    for n in 0..WARM_UP {
        draw(&mut frame, renderer, Some(n));
    }
    assert_eq!(
        renderer.last_stats().video_conversions,
        1,
        "the benchmark's screen draws its video"
    );
    let mut same: Vec<_> = (0..frames)
        .map(|_| draw(&mut frame, renderer, None))
        .collect();
    assert_eq!(
        renderer.last_stats().video_conversions,
        0,
        "no new picture, no conversion"
    );
    let mut new: Vec<_> = (0..frames)
        .map(|n| draw(&mut frame, renderer, Some(n)))
        .collect();
    VideoCost {
        copy: spread(&mut copies),
        draw_same: spread(&mut same),
        draw_new: spread(&mut new),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let frames = args
        .iter()
        .position(|arg| arg == "--frames")
        .and_then(|at| args.get(at + 1))
        .and_then(|n| n.parse().ok())
        .unwrap_or(120_usize)
        .max(10);
    let markdown = args.iter().any(|arg| arg == "--markdown");
    if cfg!(debug_assertions) {
        eprintln!("warning: a debug build. Numbers are only recorded from `--release`.");
    }

    let mut renderer = OffscreenRenderer::new().expect("no GPU for the offscreen renderer");
    let mut rows = Vec::new();
    rows.push(measure(
        "(an empty frame)".into(),
        &mut Nothing,
        &mut renderer,
        frames,
    ));
    for screen in fundidesk::Screen::ALL {
        let mut ui = Ui::new(fundidesk::FundiDesk::on(screen));
        rows.push(measure(
            format!("fundidesk {screen:?}"),
            &mut ui,
            &mut renderer,
            frames,
        ));
    }
    for screen in callready::Screen::DESIGNED {
        let mut ui = Ui::new(callready::CallReady::on(screen));
        rows.push(measure(
            format!("callready {screen:?}"),
            &mut ui,
            &mut renderer,
            frames,
        ));
    }
    for name in atlas::DESIGNED {
        let mut ui = Ui::new(atlas::Atlas::designed(name).expect("a designed page"));
        rows.push(measure(
            format!("atlas {name}"),
            &mut ui,
            &mut renderer,
            frames,
        ));
    }
    for name in barua_talk::DESIGNED {
        let mut ui = Ui::new(barua_talk::BaruaTalk::designed(name).expect("a designed page"));
        rows.push(measure(
            format!("barua-talk {name}"),
            &mut ui,
            &mut renderer,
            frames,
        ));
    }
    let video = measure_video(&mut renderer, frames);

    let empty = rows[0].draw.median;
    println!(
        "Frame 1080 x 2340 px at 2.8125 px/dp; {frames} frames a screen after {WARM_UP} of warm-up; milliseconds.\n"
    );
    if markdown {
        println!(
            "| Screen | First frame | Build p50 | Build p95 | Draw + read back p50 | p95 | Less the empty frame | Instances | Draw calls | Passes |"
        );
        println!("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
    } else {
        println!(
            "{:<34} {:>7} {:>9} {:>9} {:>9} {:>9} {:>9} {:>6} {:>5} {:>6}",
            "screen",
            "first",
            "build50",
            "build95",
            "draw50",
            "draw95",
            "-empty",
            "inst",
            "calls",
            "passes"
        );
    }
    for row in &rows {
        let net = row.draw.median - empty;
        if markdown {
            println!(
                "| {} | {:.2} | {:.3} | {:.3} | {:.2} | {:.2} | {:.2} | {} | {} | {} |",
                row.name,
                row.first,
                row.build.median,
                row.build.p95,
                row.draw.median,
                row.draw.p95,
                net,
                row.instances,
                row.draw_calls,
                row.passes
            );
        } else {
            println!(
                "{:<34} {:>7.2} {:>9.3} {:>9.3} {:>9.2} {:>9.2} {:>9.2} {:>6} {:>5} {:>6}",
                row.name,
                row.first,
                row.build.median,
                row.build.p95,
                row.draw.median,
                row.draw.p95,
                net,
                row.instances,
                row.draw_calls,
                row.passes
            );
        }
    }
    let builds: Vec<f64> = rows[1..].iter().map(|row| row.build.median).collect();
    let worst = builds.iter().copied().fold(0.0_f64, f64::max);
    println!(
        "\n{} screens. Build p50: least {:.3}, most {:.3}. An empty frame, drawn and read back: p50 {:.2}, least {:.2}.",
        builds.len(),
        builds.iter().copied().fold(f64::INFINITY, f64::min),
        worst,
        empty,
        rows[0].draw.min,
    );
    println!("\nVideo, one 1280 x 720 I420 frame (1,382,400 bytes):");
    println!(
        "  copy out of the decoder's planes      p50 {:.3}  p95 {:.3}  least {:.3}",
        video.copy.median, video.copy.p95, video.copy.min
    );
    println!(
        "  draw + read back, same picture        p50 {:.2}  p95 {:.2}",
        video.draw_same.median, video.draw_same.p95
    );
    println!(
        "  draw + read back, a new picture       p50 {:.2}  p95 {:.2}",
        video.draw_new.median, video.draw_new.p95
    );
    println!(
        "  upload and conversion (the difference) p50 {:.2}",
        video.draw_new.median - video.draw_same.median
    );
}
