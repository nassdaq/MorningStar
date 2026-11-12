//! Animation: implicit transitions of a few properties, and enter and
//! leave transitions by identity.
//!
//! The model (ADR 0021): `view` keeps describing the screen as it should
//! be; a widget that asks for it with `.animate(..)` is *shown* on the way
//! from what it was to what it now should be. Nothing here is driven by
//! the application, and no frame is produced unless something is on its
//! way.

use std::collections::HashMap;
use std::time::Duration;

use morningstar_core::{Color, Point, Rect, Transform};

/// How an animation's progress is shaped over its duration.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Easing {
    /// Constant speed.
    Linear,
    /// Starts slowly (cubic).
    EaseIn,
    /// Ends slowly (cubic). The default: right for something that reacts
    /// to a touch.
    #[default]
    EaseOut,
    /// Starts and ends slowly (cubic): for something that travels.
    EaseInOut,
}

impl Easing {
    /// The eased progress for linear progress `t`; both are `0..=1`.
    pub fn apply(self, t: f32) -> f32 {
        let t = if t.is_nan() { 1.0 } else { t.clamp(0.0, 1.0) };
        match self {
            Self::Linear => t,
            Self::EaseIn => t * t * t,
            Self::EaseOut => 1.0 - (1.0 - t).powi(3),
            Self::EaseInOut if t < 0.5 => 4.0 * t * t * t,
            Self::EaseInOut => 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0,
        }
    }
}

/// How long a change takes to show, and with which [`Easing`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Animation {
    /// From the frame that first sees the change to the frame that shows
    /// it complete.
    pub duration: Duration,
    /// The shape of the progress.
    pub easing: Easing,
}

impl Animation {
    /// An animation of `duration` with `easing`.
    pub const fn new(duration: Duration, easing: Easing) -> Self {
        Self { duration, easing }
    }

    /// Progress `0..=1` at `now` of an animation that began at `start`,
    /// before easing. An animation with no duration is complete at once.
    pub(crate) fn progress(&self, start: Duration, now: Duration) -> f32 {
        let total = self.duration.as_secs_f32();
        if total <= 0.0 {
            return 1.0;
        }
        (now.saturating_sub(start).as_secs_f32() / total).min(1.0)
    }
}

/// What a widget looks like while it is not there: where an entering
/// widget comes from and a leaving one goes to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Hidden {
    pub opacity: f32,
    /// Displacement in multiples of the widget's own width and height.
    pub shift: (f32, f32),
    pub scale: f32,
}

impl Hidden {
    const SHOWN: Self = Self {
        opacity: 1.0,
        shift: (0.0, 0.0),
        scale: 1.0,
    };
}

/// How a widget appears when it enters the screen and disappears when it
/// leaves it: [`Container::transition`](crate::Container::transition),
/// [`Element::transition`](crate::Element::transition).
///
/// ```
/// use std::time::Duration;
/// use morningstar_ui::prelude::*;
///
/// // A banner that drops in from above and fades.
/// let banner: Container<()> = Container::new(text("Saved"))
///     .id("saved-banner")
///     .transition(Transition::slide(0.0, -1.0).with_fade());
/// // A sheet that takes 400 ms.
/// let sheet = Transition::slide(0.0, 1.0)
///     .duration(Duration::from_millis(400))
///     .easing(Easing::EaseInOut);
/// # let _ = (banner, sheet);
/// ```
///
/// A widget *enters* in the first frame its identity is in the tree and
/// *leaves* in the first frame it is not; give it an `.id(..)`. See
/// "Animation" in the crate documentation for what a leaving widget is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transition {
    pub(crate) enter: Hidden,
    pub(crate) exit: Hidden,
    pub(crate) animation: Animation,
}

impl Transition {
    const DEFAULT: Animation = Animation::new(Duration::from_millis(280), Easing::EaseOut);

    /// No transition: the widget is simply there, then simply gone.
    pub const fn none() -> Self {
        Self {
            enter: Hidden::SHOWN,
            exit: Hidden::SHOWN,
            animation: Animation::new(Duration::ZERO, Easing::Linear),
        }
    }

    /// Fades in and out.
    pub const fn fade() -> Self {
        let hidden = Hidden {
            opacity: 0.0,
            ..Hidden::SHOWN
        };
        Self {
            enter: hidden,
            exit: hidden,
            animation: Self::DEFAULT,
        }
    }

    /// Slides in from, and back out to, `x` widths to the right and `y`
    /// heights below where it belongs (its own width and height; negative
    /// for left and above). `slide(1.0, 0.0)` is a screen arriving from
    /// the right edge.
    pub const fn slide(x: f32, y: f32) -> Self {
        let hidden = Hidden {
            shift: (x, y),
            ..Hidden::SHOWN
        };
        Self {
            enter: hidden,
            exit: hidden,
            animation: Self::DEFAULT,
        }
    }

    /// Grows in from, and shrinks out to, `from` times its size, about
    /// its centre, fading as it does.
    pub const fn scale(from: f32) -> Self {
        let hidden = Hidden {
            opacity: 0.0,
            shift: (0.0, 0.0),
            scale: from,
        };
        Self {
            enter: hidden,
            exit: hidden,
            animation: Self::DEFAULT,
        }
    }

    /// Leaves towards `x` widths and `y` heights away instead of going
    /// back where it came from: a screen that came from the right and
    /// leaves to the left.
    pub const fn exit_slide(mut self, x: f32, y: f32) -> Self {
        self.exit.shift = (x, y);
        self
    }

    /// Also fades, in and out.
    pub const fn with_fade(self) -> Self {
        self.enter_fade().exit_fade()
    }

    /// Also fades in, without fading out.
    pub const fn enter_fade(mut self) -> Self {
        self.enter.opacity = 0.0;
        self
    }

    /// Also fades out, without fading in.
    pub const fn exit_fade(mut self) -> Self {
        self.exit.opacity = 0.0;
        self
    }

    /// How long entering and leaving take (280 ms by default).
    pub const fn duration(mut self, duration: Duration) -> Self {
        self.animation.duration = duration;
        self
    }

    /// The easing of both (ease-out by default).
    pub const fn easing(mut self, easing: Easing) -> Self {
        self.animation.easing = easing;
        self
    }

    /// True if entering and leaving would show nothing.
    fn is_none(&self) -> bool {
        self.enter == Hidden::SHOWN && self.exit == Hidden::SHOWN
    }
}

/// What an element asks of this module. `PLAIN` for nearly every element,
/// which then costs one comparison per frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Motion {
    /// `0..=1`, multiplied into every colour of the subtree.
    pub opacity: f32,
    pub animate: Option<Animation>,
    pub transition: Option<Transition>,
}

impl Motion {
    pub const PLAIN: Self = Self {
        opacity: 1.0,
        animate: None,
        transition: None,
    };

    /// True if the element keeps no state between frames here.
    pub fn is_stateless(&self) -> bool {
        self.animate.is_none() && self.transition.is_none()
    }
}

/// The animatable properties of one element: what `view` asked for, or a
/// point on the way there. Lengths in dp.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Props {
    pub opacity: f32,
    pub transform: Transform,
    /// The layout offset.
    pub offset: Point,
    /// A solid background, with the theme resolved.
    pub fill: Option<Color>,
    /// Fixed sizes; an axis that hugs or fills is not animated.
    pub width: Option<f32>,
    pub height: Option<f32>,
}

fn mix(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

fn mix_option(from: Option<f32>, to: Option<f32>, t: f32) -> Option<f32> {
    match (from, to) {
        (Some(from), Some(to)) => Some(mix(from, to, t)),
        _ => to,
    }
}

impl Props {
    fn mix(&self, to: &Self, t: f32) -> Self {
        if t >= 1.0 {
            return *to;
        }
        let (a, b) = (&self.transform, &to.transform);
        Self {
            opacity: mix(self.opacity, to.opacity, t),
            // Component-wise: exact for moves and scales, and close for
            // the small turns a transition makes.
            transform: Transform::matrix(
                mix(a.a, b.a, t),
                mix(a.b, b.b, t),
                mix(a.c, b.c, t),
                mix(a.d, b.d, t),
                mix(a.tx, b.tx, t),
                mix(a.ty, b.ty, t),
            ),
            offset: Point::new(
                mix(self.offset.x, to.offset.x, t),
                mix(self.offset.y, to.offset.y, t),
            ),
            fill: match (self.fill, to.fill) {
                (Some(from), Some(to)) => Some(Color::rgba(
                    mix(from.r, to.r, t),
                    mix(from.g, to.g, t),
                    mix(from.b, to.b, t),
                    mix(from.a, to.a, t),
                )),
                (_, to) => to,
            },
            width: mix_option(self.width, to.width, t),
            height: mix_option(self.height, to.height, t),
        }
    }
}

/// What one element is shown as in one frame.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Shown {
    /// The element's record in the frame's pre-order list.
    pub slot: u32,
    pub id: u64,
    pub props: Props,
    /// The enter or leave transition's part: opacity factor, shift in
    /// multiples of the element's size, scale.
    pub presence: Hidden,
    /// Written by the painter: where the element is, in pixels, before
    /// `presence` moves it. A leaving element is drawn there.
    pub rect: Rect,
}

/// State kept for one element with `.animate(..)` or `.transition(..)`,
/// by identity, for as long as it is in the tree (or leaving it).
#[derive(Clone, Copy, Debug)]
struct Track {
    seen: u64,
    /// What `view` last asked for.
    target: Props,
    /// Where the move towards `target` began, and when; `None` at rest.
    from: Option<(Props, Duration, Animation)>,
    /// Presence: 1 is there, 0 is hidden. A move from `.0` to `.1`.
    presence: (f32, f32),
    presence_start: Duration,
    presence_animation: Animation,
    hidden: Hidden,
    /// The transition the element last carried.
    transition: Option<Transition>,
    /// No longer in the tree: drawn as a ghost until hidden.
    leaving: bool,
    /// The identity of the box it was last in, and its record and
    /// rectangle (px) there.
    parent: u64,
    slot: u32,
    rect: Rect,
}

impl Track {
    /// The presence at `now`, eased, and whether it is still moving.
    fn presence_at(&self, now: Duration) -> (f32, bool) {
        let (from, to) = self.presence;
        if from == to {
            return (to, false);
        }
        let t = self.presence_animation.progress(self.presence_start, now);
        let eased = self.presence_animation.easing.apply(t);
        (mix(from, to, eased), t < 1.0)
    }

    fn hidden_at(&self, presence: f32) -> Hidden {
        let away = 1.0 - presence;
        Hidden {
            opacity: mix(self.hidden.opacity, 1.0, presence),
            shift: (self.hidden.shift.0 * away, self.hidden.shift.1 * away),
            scale: mix(self.hidden.scale, 1.0, presence),
        }
    }
}

/// An element that left the tree with a transition, kept to be drawn
/// until it is out of sight.
pub(crate) struct Ghost<E> {
    pub id: u64,
    pub element: E,
    /// Where it was when it left, in pixels.
    pub rect: Rect,
}

/// Every animation in progress, and what is needed to notice the next.
pub(crate) struct Motions {
    tracks: HashMap<u64, Track>,
    /// The frame's elements that are animated, in tree order.
    pub shown: Vec<Shown>,
    /// Elements that entered in this frame: (the box they are in, how).
    entered: Vec<(u64, Transition)>,
    /// Identities that left in this frame, in the order they were in the
    /// tree. Scratch.
    left: Vec<(u32, u64)>,
    /// True if this frame showed anything still on its way.
    pub moving: bool,
    /// True if any element of this frame carries a transition: the tree is
    /// then kept for a frame, in case one of them leaves.
    pub transitions: bool,
    /// Treat every duration as zero (the platform's "reduce motion").
    pub reduce: bool,
}

impl Motions {
    pub fn new() -> Self {
        Self {
            tracks: HashMap::new(),
            shown: Vec::new(),
            entered: Vec::new(),
            left: Vec::new(),
            moving: false,
            transitions: false,
            reduce: false,
        }
    }

    /// True if nothing is being tracked: the whole module is idle.
    pub fn is_idle(&self) -> bool {
        self.tracks.is_empty()
    }

    pub fn begin_frame(&mut self) {
        self.shown.clear();
        self.entered.clear();
        self.moving = false;
        self.transitions = false;
    }

    /// An element with `.animate` or `.transition` is in the tree being
    /// lowered: brings its track up to date and returns what to show.
    /// `ghost` is true inside a leaving element (and for it).
    #[allow(clippy::too_many_arguments)]
    pub fn resolve(
        &mut self,
        id: u64,
        parent: u64,
        slot: u32,
        motion: &Motion,
        target: Props,
        generation: u64,
        now: Duration,
        ghost: bool,
    ) -> Props {
        let reduce = self.reduce;
        // `Transition::none()` counts as one: the widget does not move as
        // it enters, but it may be told how to leave by what replaces it.
        let transition = motion.transition;
        self.transitions |= transition.is_some();
        let entry = self.tracks.entry(id);
        let fresh = matches!(entry, std::collections::hash_map::Entry::Vacant(_));
        let track = entry.or_insert(Track {
            seen: generation,
            target,
            from: None,
            presence: (1.0, 1.0),
            presence_start: now,
            presence_animation: Transition::none().animation,
            hidden: Hidden::SHOWN,
            transition,
            leaving: false,
            parent,
            slot,
            rect: Rect::default(),
        });
        track.seen = generation;
        track.slot = slot;
        if !ghost {
            track.parent = parent;
            if fresh {
                if let Some(transition) = transition.filter(|_| !reduce) {
                    if !transition.is_none() {
                        track.presence = (0.0, 1.0);
                        track.presence_animation = transition.animation;
                        track.hidden = transition.enter;
                    }
                    self.entered.push((parent, transition));
                }
            } else if track.leaving {
                // Back before it had gone: return from where it got to.
                let (presence, _) = track.presence_at(now);
                track.leaving = false;
                track.presence = (presence, 1.0);
                track.presence_start = now;
            }
            track.transition = transition;
        }

        if reduce {
            // Asked for less motion while something was on its way: it
            // is there.
            track.from = None;
            if !track.leaving {
                track.presence = (1.0, 1.0);
            }
        }

        // The implicit animation of the properties.
        if track.target != target {
            match motion.animate.filter(|_| !reduce && !fresh) {
                Some(animation) => {
                    let current = Self::props_at(track, now).0;
                    track.from = Some((current, now, animation));
                }
                None => track.from = None,
            }
            track.target = target;
        }
        let (props, moving) = Self::props_at(track, now);
        if !moving {
            track.from = None;
        }
        let (presence, arriving) = track.presence_at(now);
        if !arriving && !track.leaving {
            track.presence = (1.0, 1.0);
        }
        self.moving |= moving || arriving;
        self.shown.push(Shown {
            slot,
            id,
            props,
            presence: track.hidden_at(presence),
            rect: track.rect,
        });
        props
    }

    fn props_at(track: &Track, now: Duration) -> (Props, bool) {
        match &track.from {
            None => (track.target, false),
            Some((from, start, animation)) => {
                let t = animation.progress(*start, now);
                (from.mix(&track.target, animation.easing.apply(t)), t < 1.0)
            }
        }
    }

    /// After the screen's tree has been lowered: forgets the elements
    /// that are gone, and calls `leave` for each that should be seen
    /// leaving (it had a transition), in tree order. `leave` returns false
    /// if the element cannot be found to be drawn; its second argument is
    /// where the element last was, in pixels.
    pub fn sweep(
        &mut self,
        generation: u64,
        now: Duration,
        mut leave: impl FnMut(u64, Rect) -> bool,
    ) {
        if self.tracks.is_empty() {
            return;
        }
        let reduce = self.reduce;
        self.left.clear();
        for (id, track) in &self.tracks {
            if track.seen != generation && !track.leaving {
                self.left.push((track.slot, *id));
            }
        }
        if self.left.is_empty() {
            return;
        }
        self.left.sort_unstable();
        for index in 0..self.left.len() {
            let (_, id) = self.left[index];
            let Some(track) = self.tracks.get_mut(&id) else {
                continue;
            };
            // Whatever took its place says how it leaves; failing that,
            // the transition it carried itself.
            let how = self
                .entered
                .iter()
                .find(|(parent, _)| *parent == track.parent)
                .map(|(_, transition)| *transition)
                .or(track.transition)
                .filter(|transition| !transition.is_none());
            let (presence, _) = track.presence_at(now);
            match how.filter(|_| !reduce && presence > 0.0 && !track.rect.is_empty()) {
                Some(transition) if leave(id, track.rect) => {
                    track.leaving = true;
                    track.hidden = transition.exit;
                    track.presence = (presence, 0.0);
                    track.presence_start = now;
                    track.presence_animation = transition.animation;
                    // Inside a ghost its own animations stand still.
                    track.from = None;
                }
                _ => {
                    self.tracks.remove(&id);
                }
            }
        }
    }

    /// True while the element with this identity is leaving and has not
    /// yet gone at `now`.
    pub fn is_leaving(&self, id: u64, now: Duration) -> bool {
        self.tracks
            .get(&id)
            .is_some_and(|track| track.leaving && track.presence_at(now).1)
    }

    /// After painting: remembers where each element with a transition
    /// was, and forgets what is in neither the tree nor a ghost.
    pub fn end_frame(&mut self, generation: u64) {
        for shown in &self.shown {
            if let Some(track) = self.tracks.get_mut(&shown.id)
                && !track.leaving
            {
                track.rect = shown.rect;
            }
        }
        self.tracks.retain(|_, track| track.seen == generation);
    }

    /// Where in `shown` the element at `slot` is, if it is animated.
    /// `cursor` is the caller's position in `shown`, which is in tree
    /// order like the slots it is asked about.
    pub fn index_at(&self, slot: u32, cursor: &mut usize) -> Option<usize> {
        while self
            .shown
            .get(*cursor)
            .is_some_and(|shown| shown.slot < slot)
        {
            *cursor += 1;
        }
        self.shown
            .get(*cursor)
            .is_some_and(|shown| shown.slot == slot)
            .then_some(*cursor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easings_start_at_zero_end_at_one_and_never_go_backwards() {
        for easing in [
            Easing::Linear,
            Easing::EaseIn,
            Easing::EaseOut,
            Easing::EaseInOut,
        ] {
            assert_eq!(easing.apply(0.0), 0.0);
            assert_eq!(easing.apply(1.0), 1.0);
            let mut last = 0.0;
            for step in 0..=100 {
                let value = easing.apply(step as f32 / 100.0);
                assert!(value >= last && value <= 1.0, "{easing:?} at {step}");
                last = value;
            }
            assert_eq!(easing.apply(f32::NAN), 1.0);
            assert_eq!(easing.apply(-3.0), 0.0);
            assert_eq!(easing.apply(7.0), 1.0);
        }
        assert_eq!(Easing::EaseOut.apply(0.5), 0.875);
        assert_eq!(Easing::EaseIn.apply(0.5), 0.125);
        assert_eq!(Easing::EaseInOut.apply(0.5), 0.5);
    }

    #[test]
    fn an_animation_without_duration_is_complete_at_once() {
        let animation = Animation::new(Duration::ZERO, Easing::Linear);
        assert_eq!(animation.progress(Duration::ZERO, Duration::ZERO), 1.0);
        let animation = Animation::new(Duration::from_millis(200), Easing::Linear);
        let at = |ms| animation.progress(Duration::from_millis(100), Duration::from_millis(ms));
        assert_eq!(at(50), 0.0);
        assert_eq!(at(200), 0.5);
        assert_eq!(at(900), 1.0);
    }
}
