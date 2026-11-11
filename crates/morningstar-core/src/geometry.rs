/// A position in physical pixels. The origin is the top-left corner of the
/// surface; `y` grows downwards.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// A width and height in physical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const ZERO: Self = Self::new(0.0, 0.0);

    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

/// An axis-aligned rectangle in physical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// A rectangle of the given size centred on `center`.
    pub fn centered(center: Point, width: f32, height: f32) -> Self {
        Self::new(
            center.x - width / 2.0,
            center.y - height / 2.0,
            width,
            height,
        )
    }

    pub fn center(&self) -> Point {
        Point::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    /// True if the rectangle covers no area (or has a non-finite dimension).
    pub fn is_empty(&self) -> bool {
        // Written so that NaN counts as empty.
        !(self.width > 0.0 && self.height > 0.0)
    }

    /// True if `point` lies inside. The left and top edges are inside; the
    /// right and bottom edges are not, so adjacent rectangles never both
    /// claim a point.
    pub fn contains(&self, point: Point) -> bool {
        point.x >= self.x
            && point.x < self.x + self.width
            && point.y >= self.y
            && point.y < self.y + self.height
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_is_half_open() {
        let rect = Rect::new(10.0, 20.0, 30.0, 40.0);
        assert!(rect.contains(Point::new(10.0, 20.0)));
        assert!(rect.contains(Point::new(39.9, 59.9)));
        assert!(!rect.contains(Point::new(40.0, 30.0)));
        assert!(!rect.contains(Point::new(20.0, 60.0)));
        assert!(!rect.contains(Point::new(9.9, 30.0)));
    }

    #[test]
    fn centered_round_trips() {
        let rect = Rect::centered(Point::new(100.0, 50.0), 40.0, 20.0);
        assert_eq!(rect, Rect::new(80.0, 40.0, 40.0, 20.0));
        assert_eq!(rect.center(), Point::new(100.0, 50.0));
    }

    #[test]
    fn degenerate_rects_are_empty() {
        assert!(Rect::new(0.0, 0.0, 0.0, 10.0).is_empty());
        assert!(Rect::new(0.0, 0.0, 10.0, -1.0).is_empty());
        assert!(Rect::new(0.0, 0.0, f32::NAN, 10.0).is_empty());
        assert!(!Rect::new(0.0, 0.0, 0.5, 0.5).is_empty());
    }
}
