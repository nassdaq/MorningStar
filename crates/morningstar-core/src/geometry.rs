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

/// A 2D affine transform: a linear map followed by a translation.
///
/// A point `(x, y)` is taken to
/// `(a * x + c * y + tx, b * x + d * y + ty)`, the convention of CSS
/// `matrix(a, b, c, d, tx, ty)`. With `y` growing downwards, a positive
/// rotation is clockwise on screen, as in CSS.
///
/// Two ways of combining transforms are offered, because both are wanted:
///
/// * [`then`](Self::then) is the mathematical one: `a.then(b)` moves a
///   point by `a` first and then by `b`, both in the fixed frame.
/// * The `and_*` methods append a function the way a CSS `transform` list
///   does, each one acting in the axes the earlier ones produced:
///   `Transform::rotate(5.0).and_translate(0.0, -8.0)` is
///   `transform: rotate(5deg) translate(0, -8px)`.
///
/// The translation is in the units of whatever the transform is applied to
/// (physical pixels in a display list; dp in the UI layer); the linear part
/// has no unit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    /// Leaves every point where it is.
    pub const IDENTITY: Self = Self::matrix(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);

    /// From the six numbers of CSS `matrix(a, b, c, d, tx, ty)`.
    pub const fn matrix(a: f32, b: f32, c: f32, d: f32, tx: f32, ty: f32) -> Self {
        Self { a, b, c, d, tx, ty }
    }

    /// Moves by `(x, y)`.
    pub const fn translate(x: f32, y: f32) -> Self {
        Self::matrix(1.0, 0.0, 0.0, 1.0, x, y)
    }

    /// Scales about the origin.
    pub const fn scale(x: f32, y: f32) -> Self {
        Self::matrix(x, 0.0, 0.0, y, 0.0, 0.0)
    }

    /// Rotates about the origin by `degrees`, clockwise on screen (CSS
    /// `rotate`). Multiples of 90 degrees are exact.
    pub fn rotate(degrees: f32) -> Self {
        let quarter = degrees / 90.0;
        let (sin, cos) = if quarter == quarter.round() && quarter.abs() < 1.0e6 {
            // Exact, so that a quarter turn maps pixels onto pixels.
            match (quarter as i64).rem_euclid(4) {
                0 => (0.0, 1.0),
                1 => (1.0, 0.0),
                2 => (0.0, -1.0),
                _ => (-1.0, 0.0),
            }
        } else {
            degrees.to_radians().sin_cos()
        };
        Self::matrix(cos, sin, -sin, cos, 0.0, 0.0)
    }

    /// Slants along x by `degrees` (CSS `skewX`): a point moves right by
    /// `tan(degrees)` times its `y`.
    pub fn skew_x(degrees: f32) -> Self {
        Self::shear_x(degrees.to_radians().tan())
    }

    /// Slants along y by `degrees` (CSS `skewY`).
    pub fn skew_y(degrees: f32) -> Self {
        Self::matrix(1.0, degrees.to_radians().tan(), 0.0, 1.0, 0.0, 0.0)
    }

    /// Slants along x by a factor: a point moves right by `factor` times
    /// its `y`. A synthetic italic is `shear_x(-0.25)` about the baseline.
    pub const fn shear_x(factor: f32) -> Self {
        Self::matrix(1.0, 0.0, factor, 1.0, 0.0, 0.0)
    }

    /// This transform followed by `next`: a point is moved by `self`, and
    /// the result by `next`.
    pub fn then(self, next: Self) -> Self {
        Self {
            a: next.a * self.a + next.c * self.b,
            b: next.b * self.a + next.d * self.b,
            c: next.a * self.c + next.c * self.d,
            d: next.b * self.c + next.d * self.d,
            tx: next.a * self.tx + next.c * self.ty + next.tx,
            ty: next.b * self.tx + next.d * self.ty + next.ty,
        }
    }

    /// Appends `rotate(degrees)` as a CSS transform list would.
    pub fn and_rotate(self, degrees: f32) -> Self {
        Self::rotate(degrees).then(self)
    }

    /// Appends `translate(x, y)` as a CSS transform list would: the move is
    /// along the axes as the functions before it left them.
    pub fn and_translate(self, x: f32, y: f32) -> Self {
        Self::translate(x, y).then(self)
    }

    /// Appends `scale(x, y)` as a CSS transform list would.
    pub fn and_scale(self, x: f32, y: f32) -> Self {
        Self::scale(x, y).then(self)
    }

    /// Appends `skewX(degrees)` as a CSS transform list would.
    pub fn and_skew_x(self, degrees: f32) -> Self {
        Self::skew_x(degrees).then(self)
    }

    /// The same transform carried out about `origin` instead of `(0, 0)`:
    /// CSS `transform-origin`, SVG `rotate(a cx cy)`.
    pub fn about(self, origin: Point) -> Self {
        Self::translate(-origin.x, -origin.y)
            .then(self)
            .then(Self::translate(origin.x, origin.y))
    }

    /// The same transform with its translation multiplied by `factor`: a
    /// transform given in dp, in pixels.
    pub fn with_translation_scaled(self, factor: f32) -> Self {
        Self {
            tx: self.tx * factor,
            ty: self.ty * factor,
            ..self
        }
    }

    /// Where the transform takes `point`.
    pub fn apply(&self, point: Point) -> Point {
        Point::new(
            self.a * point.x + self.c * point.y + self.tx,
            self.b * point.x + self.d * point.y + self.ty,
        )
    }

    /// The area scale factor: negative if the transform mirrors, zero if it
    /// flattens everything onto a line.
    pub fn determinant(&self) -> f32 {
        self.a * self.d - self.b * self.c
    }

    /// True if every number is finite.
    pub fn is_finite(&self) -> bool {
        [self.a, self.b, self.c, self.d, self.tx, self.ty]
            .iter()
            .all(|value| value.is_finite())
    }

    /// True if nothing moves.
    pub fn is_identity(&self) -> bool {
        *self == Self::IDENTITY
    }

    /// True if the transform only moves things, without turning, scaling or
    /// slanting them.
    pub fn is_translation(&self) -> bool {
        self.a == 1.0 && self.b == 0.0 && self.c == 0.0 && self.d == 1.0
    }

    /// The transform that undoes this one, or `None` if there is none (the
    /// determinant is zero or something is not finite).
    pub fn inverse(&self) -> Option<Self> {
        let det = self.determinant();
        if !(self.is_finite() && det.is_finite() && det != 0.0) {
            return None;
        }
        let (a, b, c, d) = (self.d / det, -self.b / det, -self.c / det, self.a / det);
        Some(Self {
            a,
            b,
            c,
            d,
            tx: -(a * self.tx + c * self.ty),
            ty: -(b * self.tx + d * self.ty),
        })
    }

    /// The smallest axis-aligned rectangle containing the image of `rect`.
    pub fn bounding_rect(&self, rect: Rect) -> Rect {
        let corners = [
            self.apply(Point::new(rect.x, rect.y)),
            self.apply(Point::new(rect.x + rect.width, rect.y)),
            self.apply(Point::new(rect.x, rect.y + rect.height)),
            self.apply(Point::new(rect.x + rect.width, rect.y + rect.height)),
        ];
        let fold = |pick: fn(&Point) -> f32, choose: fn(f32, f32) -> f32| {
            corners[1..]
                .iter()
                .map(pick)
                .fold(pick(&corners[0]), choose)
        };
        let (left, top) = (fold(|p| p.x, f32::min), fold(|p| p.y, f32::min));
        let (right, bottom) = (fold(|p| p.x, f32::max), fold(|p| p.y, f32::max));
        Rect::new(left, top, right - left, bottom - top)
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

    fn close(a: Point, b: Point) -> bool {
        (a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4
    }

    #[test]
    fn rotation_is_clockwise_on_screen_and_exact_at_quarter_turns() {
        // y grows downwards: a quarter turn takes "right" to "down".
        let quarter = Transform::rotate(90.0);
        assert_eq!(quarter.apply(Point::new(1.0, 0.0)), Point::new(0.0, 1.0));
        assert_eq!(quarter, Transform::matrix(0.0, 1.0, -1.0, 0.0, 0.0, 0.0));
        assert_eq!(
            Transform::rotate(180.0),
            Transform::matrix(-1.0, 0.0, 0.0, -1.0, 0.0, 0.0)
        );
        assert_eq!(Transform::rotate(-90.0), Transform::rotate(270.0));
        assert_eq!(Transform::rotate(360.0), Transform::IDENTITY);
        let eighth = Transform::rotate(45.0).apply(Point::new(1.0, 0.0));
        assert!(close(eighth, Point::new(0.70710677, 0.70710677)));
    }

    #[test]
    fn then_applies_left_to_right_and_css_lists_right_to_left() {
        let point = Point::new(1.0, 0.0);
        // Fixed frame: turn, then move up.
        let fixed = Transform::rotate(90.0).then(Transform::translate(0.0, -8.0));
        assert!(close(fixed.apply(point), Point::new(0.0, -7.0)));
        // CSS `rotate(90deg) translate(0, -8px)`: the move is along the
        // turned axes, so "up" has become "right".
        let css = Transform::rotate(90.0).and_translate(0.0, -8.0);
        assert!(close(css.apply(point), Point::new(8.0, 1.0)));
        assert_eq!(
            css,
            Transform::translate(0.0, -8.0).then(Transform::rotate(90.0))
        );
    }

    #[test]
    fn about_keeps_the_origin_fixed() {
        let center = Point::new(50.0, 20.0);
        let turn = Transform::rotate(37.0).about(center);
        assert!(close(turn.apply(center), center));
        let half = Transform::rotate(180.0).about(center);
        assert!(close(
            half.apply(Point::new(0.0, 0.0)),
            Point::new(100.0, 40.0)
        ));
    }

    #[test]
    fn inverse_undoes_and_degenerate_has_none() {
        let transform = Transform::rotate(-8.0)
            .and_translate(3.0, -12.0)
            .and_skew_x(10.0)
            .about(Point::new(7.0, 9.0));
        let inverse = transform.inverse().unwrap();
        let point = Point::new(31.0, -4.5);
        assert!(close(inverse.apply(transform.apply(point)), point));
        assert_eq!(Transform::scale(0.0, 1.0).inverse(), None);
        assert_eq!(Transform::translate(f32::NAN, 0.0).inverse(), None);
    }

    #[test]
    fn skew_and_shear_slant_along_x() {
        let italic = Transform::shear_x(-0.25);
        // Above the baseline (negative y) moves right.
        assert_eq!(italic.apply(Point::new(0.0, -8.0)), Point::new(2.0, -8.0));
        assert!(close(
            Transform::skew_x(45.0).apply(Point::new(0.0, 1.0)),
            Point::new(1.0, 1.0)
        ));
        assert_eq!(italic.determinant(), 1.0);
    }

    #[test]
    fn bounding_rect_of_a_turned_rectangle() {
        let rect = Rect::new(10.0, 20.0, 30.0, 10.0);
        let turned = Transform::rotate(90.0).bounding_rect(rect);
        assert_eq!(turned, Rect::new(-30.0, 10.0, 10.0, 30.0));
        assert_eq!(Transform::IDENTITY.bounding_rect(rect), rect);
        assert!(Transform::translate(3.0, 4.0).is_translation());
        assert!(!Transform::rotate(1.0).is_translation());
        assert_eq!(
            Transform::translate(3.0, 4.0).with_translation_scaled(2.0),
            Transform::translate(6.0, 8.0)
        );
    }
}
