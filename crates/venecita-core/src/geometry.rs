//! Basic 2D geometry.
//!
//! All coordinates in Venecita Dock's core are **logical points** in **global desktop
//! coordinates** with the origin at the top-left of the primary display and the y axis
//! pointing down. Platform backends convert from/to their native coordinate systems.

use serde::{Deserialize, Serialize};

/// An axis-aligned rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Rect {
    /// Left edge.
    pub x: f64,
    /// Top edge.
    pub y: f64,
    /// Width (never negative for rectangles produced by this crate).
    pub w: f64,
    /// Height (never negative for rectangles produced by this crate).
    pub h: f64,
}

impl Rect {
    /// Creates a rectangle from its top-left corner and size.
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    /// Right edge (`x + w`).
    pub fn right(&self) -> f64 {
        self.x + self.w
    }

    /// Bottom edge (`y + h`).
    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }

    /// Center point `(x, y)`.
    pub fn center(&self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    /// Area of the rectangle.
    pub fn area(&self) -> f64 {
        self.w.max(0.0) * self.h.max(0.0)
    }

    /// Returns `true` if the point lies inside the rectangle (left/top inclusive,
    /// right/bottom exclusive), so adjacent rectangles never both contain a point.
    pub fn contains_point(&self, px: f64, py: f64) -> bool {
        px >= self.x && px < self.right() && py >= self.y && py < self.bottom()
    }

    /// Returns `true` if `other` lies completely inside `self` (with a small tolerance
    /// to absorb floating point noise).
    pub fn contains_rect(&self, other: &Rect) -> bool {
        const EPS: f64 = 1e-6;
        other.x >= self.x - EPS
            && other.y >= self.y - EPS
            && other.right() <= self.right() + EPS
            && other.bottom() <= self.bottom() + EPS
    }

    /// Shrinks the rectangle by the given amounts on each side. The size is clamped so
    /// it never becomes negative.
    pub fn inset(&self, left: f64, top: f64, right: f64, bottom: f64) -> Rect {
        let w = (self.w - left - right).max(0.0);
        let h = (self.h - top - bottom).max(0.0);
        Rect::new(self.x + left, self.y + top, w, h)
    }

    /// Intersection of two rectangles, or `None` if they don't overlap.
    pub fn intersection(&self, other: &Rect) -> Option<Rect> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let r = self.right().min(other.right());
        let b = self.bottom().min(other.bottom());
        (r > x && b > y).then(|| Rect::new(x, y, r - x, b - y))
    }

    /// A rectangle with the same center scaled by `factor` (clamped to `0.0..=1.0`).
    /// Used by focus mode to grow a window to e.g. 80 % of the work area.
    pub fn centered_scaled(&self, factor: f64) -> Rect {
        let f = factor.clamp(0.0, 1.0);
        let w = self.w * f;
        let h = self.h * f;
        Rect::new(
            self.x + (self.w - w) / 2.0,
            self.y + (self.h - h) / 2.0,
            w,
            h,
        )
    }

    /// Returns `true` if every edge of both rectangles is within `tolerance` points.
    /// Used to avoid re-applying a frame a window already has.
    pub fn approx_eq(&self, other: &Rect, tolerance: f64) -> bool {
        (self.x - other.x).abs() <= tolerance
            && (self.y - other.y).abs() <= tolerance
            && (self.w - other.w).abs() <= tolerance
            && (self.h - other.h).abs() <= tolerance
    }
}

/// One of the four cardinal directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Towards smaller x.
    Left,
    /// Towards larger x.
    Right,
    /// Towards smaller y.
    Up,
    /// Towards larger y.
    Down,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_and_center() {
        let r = Rect::new(10.0, 20.0, 100.0, 50.0);
        assert_eq!(r.right(), 110.0);
        assert_eq!(r.bottom(), 70.0);
        assert_eq!(r.center(), (60.0, 45.0));
        assert_eq!(r.area(), 5000.0);
    }

    #[test]
    fn contains_point_is_half_open() {
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert!(r.contains_point(0.0, 0.0));
        assert!(r.contains_point(9.99, 9.99));
        assert!(!r.contains_point(10.0, 5.0));
        assert!(!r.contains_point(-0.1, 5.0));
    }

    #[test]
    fn inset_never_goes_negative() {
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert_eq!(r.inset(1.0, 2.0, 3.0, 4.0), Rect::new(1.0, 2.0, 6.0, 4.0));
        assert_eq!(r.inset(8.0, 8.0, 8.0, 8.0).w, 0.0);
    }

    #[test]
    fn intersection() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(5.0, 5.0, 10.0, 10.0);
        assert_eq!(a.intersection(&b), Some(Rect::new(5.0, 5.0, 5.0, 5.0)));
        let c = Rect::new(10.0, 0.0, 5.0, 5.0); // touching edge only
        assert_eq!(a.intersection(&c), None);
    }

    #[test]
    fn intersection_with_negative_origins() {
        // A monitor placed to the left of / above the primary one has negative coordinates.
        let left_monitor = Rect::new(-1920.0, -200.0, 1920.0, 1080.0);
        let window = Rect::new(-100.0, 0.0, 400.0, 300.0);
        assert_eq!(
            left_monitor.intersection(&window),
            Some(Rect::new(-100.0, 0.0, 100.0, 300.0))
        );
    }

    #[test]
    fn centered_scaled_keeps_center_and_stays_inside() {
        let r = Rect::new(100.0, 50.0, 1000.0, 800.0);
        let s = r.centered_scaled(0.8);
        assert_eq!(s.center(), r.center());
        assert_eq!((s.w, s.h), (800.0, 640.0));
        assert!(r.contains_rect(&s));
        assert_eq!(r.centered_scaled(5.0), r, "factor is clamped to 1.0");
    }

    #[test]
    fn approx_eq() {
        let a = Rect::new(0.0, 0.0, 100.0, 100.0);
        assert!(a.approx_eq(&Rect::new(0.5, -0.5, 100.9, 99.1), 1.0));
        assert!(!a.approx_eq(&Rect::new(2.0, 0.0, 100.0, 100.0), 1.0));
    }
}
