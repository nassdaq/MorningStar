use crate::{Rect, SurfaceSize};

/// Distances from each edge of the surface, in physical pixels.
///
/// Used for the safe area: the parts of an edge-to-edge surface covered by
/// the status bar, a display cutout, the navigation or gesture bar, or the
/// home indicator. Content drawn there is visible but may be obscured and
/// may not receive touches.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Insets {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0, 0.0);

    pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    /// The same insets with every negative or non-finite edge replaced by
    /// zero, so values from a host can be used without further checks.
    pub fn sanitized(self) -> Self {
        let clean = |edge: f32| if edge.is_finite() { edge.max(0.0) } else { 0.0 };
        Self::new(
            clean(self.left),
            clean(self.top),
            clean(self.right),
            clean(self.bottom),
        )
    }

    /// The part of a surface of `size` that lies inside these insets.
    ///
    /// Never has a negative dimension: if the insets are larger than the
    /// surface (which can happen for one frame during a rotation, when the
    /// new size and the new insets arrive separately) the result is empty.
    pub fn inner_rect(self, size: SurfaceSize) -> Rect {
        let (width, height) = (size.width as f32, size.height as f32);
        let left = self.left.min(width);
        let top = self.top.min(height);
        Rect::new(
            left,
            top,
            (width - left - self.right).max(0.0),
            (height - top - self.bottom).max(0.0),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: SurfaceSize = SurfaceSize::new(1080, 2400);

    #[test]
    fn inner_rect_removes_each_edge() {
        let safe = Insets::new(10.0, 90.0, 20.0, 60.0).inner_rect(SIZE);
        assert_eq!(safe, Rect::new(10.0, 90.0, 1050.0, 2250.0));
    }

    #[test]
    fn zero_insets_cover_the_whole_surface() {
        assert_eq!(
            Insets::ZERO.inner_rect(SIZE),
            Rect::new(0.0, 0.0, 1080.0, 2400.0)
        );
    }

    #[test]
    fn oversized_insets_give_an_empty_rect_inside_the_surface() {
        let safe = Insets::new(2000.0, 3000.0, 2000.0, 3000.0).inner_rect(SIZE);
        assert!(safe.is_empty());
        assert!(safe.width >= 0.0 && safe.height >= 0.0);
        assert!(safe.x <= 1080.0 && safe.y <= 2400.0);
    }

    #[test]
    fn sanitized_drops_negative_and_non_finite_edges() {
        let insets = Insets::new(-5.0, f32::NAN, f32::INFINITY, 12.0).sanitized();
        assert_eq!(insets, Insets::new(0.0, 0.0, 0.0, 12.0));
    }
}
