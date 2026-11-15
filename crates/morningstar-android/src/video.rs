//! Video frames on their way from WebRTC's threads to the engine (ADR
//! 0039). There is no queue: each video has one slot holding its latest
//! frame, a frame the render thread has not taken yet is replaced by the
//! next, and the render thread is woken once per video however many frames
//! arrive before it looks. Buffers go round: a replaced or drawn frame's
//! planes carry the next frame, so a stream allocates only at its start and
//! when its size grows.

use std::sync::Mutex;

use morningstar_runtime::{Engine, VideoFrame, VideoTrack};

/// Spare frames kept for their buffers. More than this is only memory.
const MAX_SPARE: usize = 4;

#[derive(Default)]
struct Slots {
    /// The latest frame of each video that the render thread has not
    /// taken.
    latest: Vec<(VideoTrack, VideoFrame)>,
    /// Frames that were shown or replaced: buffers for the next ones.
    spare: Vec<VideoFrame>,
}

static SLOTS: Mutex<Slots> = Mutex::new(Slots {
    latest: Vec::new(),
    spare: Vec::new(),
});

fn slots() -> std::sync::MutexGuard<'static, Slots> {
    // A panic while holding the lock leaves plain data behind: use it.
    SLOTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Puts a new frame of `track` in its slot. `make` builds it, given a
/// spare frame to reuse the buffers of, and is called with the lock held
/// (it copies the planes; the render thread holds the lock only to take).
/// Returns true if the render thread must be woken: the slot was empty.
pub fn offer(
    track: VideoTrack,
    make: impl FnOnce(Option<VideoFrame>) -> Option<VideoFrame>,
) -> bool {
    let mut slots = slots();
    let waiting = slots.latest.iter().position(|(held, _)| *held == track);
    // The frame nobody took is the best buffer: it is this video's size.
    let reuse = match waiting {
        Some(index) => Some(slots.latest.swap_remove(index).1),
        None => slots.spare.pop(),
    };
    let Some(frame) = make(reuse) else {
        return false;
    };
    slots.latest.push((track, frame));
    waiting.is_none()
}

/// Hands every waiting frame to the engine. Render thread.
pub fn deliver(engine: &mut Engine) {
    let waiting = std::mem::take(&mut slots().latest);
    let mut used = Vec::new();
    for (track, frame) in waiting {
        used.extend(engine.video_frame(track, frame));
    }
    if !used.is_empty() {
        let mut slots = slots();
        let room = MAX_SPARE.saturating_sub(slots.spare.len());
        slots.spare.extend(used.into_iter().take(room));
    }
}

/// Video `track` is over: a frame of it still waiting is dropped.
pub fn forget(track: VideoTrack) {
    slots().latest.retain(|(held, _)| *held != track);
}

/// Every video is over: nothing waits, and the buffers are given back.
pub fn forget_all() {
    let mut slots = slots();
    slots.latest.clear();
    slots.spare.clear();
}
