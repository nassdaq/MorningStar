/// Size of a drawable surface in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceSize {
    pub width: u32,
    pub height: u32,
}

impl SurfaceSize {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// A surface with a zero dimension cannot be configured or drawn to.
    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }
}

/// The lifecycle signals every platform host must translate its native
/// callbacks into. This is the whole contract between an operating system and
/// the Morningstar runtime for the purpose of deciding *whether to draw*.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifecycleEvent {
    /// The app is in the foreground and should produce frames.
    Resumed,
    /// The app is no longer in the foreground; stop producing frames.
    Paused,
    /// A native drawable now exists.
    SurfaceCreated(SurfaceSize),
    /// The native drawable changed size (rotation, split screen, ...).
    SurfaceResized(SurfaceSize),
    /// The native drawable is gone. GPU surface resources must already be
    /// released by the time the host's callback returns.
    SurfaceDestroyed,
}

/// Tracks lifecycle state and answers one question: should a frame be drawn?
///
/// Android and iOS deliver these events in different orders (Android may
/// resume before the surface exists; iOS creates the layer first), so the
/// state machine accepts them in any order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Lifecycle {
    resumed: bool,
    surface: Option<SurfaceSize>,
}

impl Lifecycle {
    pub const fn new() -> Self {
        Self {
            resumed: false,
            surface: None,
        }
    }

    pub fn apply(&mut self, event: LifecycleEvent) {
        match event {
            LifecycleEvent::Resumed => self.resumed = true,
            LifecycleEvent::Paused => self.resumed = false,
            LifecycleEvent::SurfaceCreated(size) => self.surface = Some(size),
            LifecycleEvent::SurfaceResized(size) => {
                // A resize for a surface we were never told about is ignored
                // rather than treated as creation: only `SurfaceCreated`
                // carries a native handle.
                if self.surface.is_some() {
                    self.surface = Some(size);
                }
            }
            LifecycleEvent::SurfaceDestroyed => self.surface = None,
        }
    }

    pub const fn is_resumed(&self) -> bool {
        self.resumed
    }

    pub const fn surface(&self) -> Option<SurfaceSize> {
        self.surface
    }

    /// True when the app is in the foreground and has a non-empty surface.
    pub fn should_render(&self) -> bool {
        self.resumed && self.surface.is_some_and(|size| !size.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: SurfaceSize = SurfaceSize::new(1080, 2400);

    #[test]
    fn renders_only_when_resumed_with_surface() {
        let mut lc = Lifecycle::new();
        assert!(!lc.should_render());

        lc.apply(LifecycleEvent::Resumed);
        assert!(!lc.should_render(), "resumed without a surface");

        lc.apply(LifecycleEvent::SurfaceCreated(SIZE));
        assert!(lc.should_render());

        lc.apply(LifecycleEvent::Paused);
        assert!(!lc.should_render(), "surface without foreground");
    }

    #[test]
    fn order_independent() {
        let mut android = Lifecycle::new();
        android.apply(LifecycleEvent::Resumed);
        android.apply(LifecycleEvent::SurfaceCreated(SIZE));

        let mut ios = Lifecycle::new();
        ios.apply(LifecycleEvent::SurfaceCreated(SIZE));
        ios.apply(LifecycleEvent::Resumed);

        assert_eq!(android, ios);
    }

    #[test]
    fn surface_destroyed_stops_rendering_and_can_recover() {
        let mut lc = Lifecycle::new();
        lc.apply(LifecycleEvent::Resumed);
        lc.apply(LifecycleEvent::SurfaceCreated(SIZE));
        lc.apply(LifecycleEvent::SurfaceDestroyed);
        assert!(!lc.should_render());
        assert_eq!(lc.surface(), None);

        lc.apply(LifecycleEvent::SurfaceCreated(SIZE));
        assert!(lc.should_render());
    }

    #[test]
    fn empty_surface_is_not_renderable() {
        let mut lc = Lifecycle::new();
        lc.apply(LifecycleEvent::Resumed);
        lc.apply(LifecycleEvent::SurfaceCreated(SurfaceSize::new(0, 2400)));
        assert!(!lc.should_render());

        lc.apply(LifecycleEvent::SurfaceResized(SIZE));
        assert!(lc.should_render());
    }

    #[test]
    fn resize_without_surface_is_ignored() {
        let mut lc = Lifecycle::new();
        lc.apply(LifecycleEvent::Resumed);
        lc.apply(LifecycleEvent::SurfaceResized(SIZE));
        assert_eq!(lc.surface(), None);
        assert!(!lc.should_render());
    }
}
