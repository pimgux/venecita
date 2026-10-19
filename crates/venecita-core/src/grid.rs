//! The placement grid.
//!
//! Every display's work area is divided into `cols × rows` cells (8 × 4 by default).
//! A [`Zone`] is a rectangle of contiguous cells. This module converts zones to pixel
//! rectangles (taking gaps into account), snaps arbitrary rectangles back to zones and
//! implements the keyboard operations that move or resize a zone one cell at a time.
//!
//! ## Gaps
//!
//! With a gap of `g`, there are `g` points between neighbouring zones **and** `g` points
//! between a zone and the edge of the work area. This is achieved by first shrinking the
//! work area by `g/2` on every side, dividing that into cells, and then shrinking each
//! zone by another `g/2` on every side.

use serde::{Deserialize, Serialize};

use crate::geometry::{Direction, Rect};

/// Maximum number of columns or rows a grid can have.
pub const MAX_GRID_CELLS: u8 = 32;

/// Number of columns and rows of the grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GridSize {
    /// Number of columns (1..=32).
    pub cols: u8,
    /// Number of rows (1..=32).
    pub rows: u8,
}

impl Default for GridSize {
    /// The default grid is 8 columns × 4 rows.
    fn default() -> Self {
        Self { cols: 8, rows: 4 }
    }
}

impl GridSize {
    /// Returns `true` if both dimensions are within `1..=MAX_GRID_CELLS`.
    pub fn is_valid(&self) -> bool {
        (1..=MAX_GRID_CELLS).contains(&self.cols) && (1..=MAX_GRID_CELLS).contains(&self.rows)
    }

    /// The zone covering the whole grid.
    pub fn full_zone(&self) -> Zone {
        Zone::new(0, 0, self.cols, self.rows)
    }
}

/// A rectangle of grid cells: starts at (`col`, `row`) and spans `w × h` cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Zone {
    /// First column (0-based).
    pub col: u8,
    /// First row (0-based).
    pub row: u8,
    /// Width in cells (at least 1).
    pub w: u8,
    /// Height in cells (at least 1).
    pub h: u8,
}

impl Zone {
    /// Creates a zone. Use [`Zone::fits`] to check it against a grid.
    pub const fn new(col: u8, row: u8, w: u8, h: u8) -> Self {
        Self { col, row, w, h }
    }

    /// Returns `true` if the zone is non-empty and lies completely inside `grid`.
    pub fn fits(&self, grid: GridSize) -> bool {
        self.w >= 1
            && self.h >= 1
            && u16::from(self.col) + u16::from(self.w) <= u16::from(grid.cols)
            && u16::from(self.row) + u16::from(self.h) <= u16::from(grid.rows)
    }

    /// Forces the zone into `grid`: the size is reduced if it is larger than the grid and
    /// the position is moved back inside. Used when moving a window to a display whose
    /// grid has different dimensions.
    pub fn clamped(&self, grid: GridSize) -> Zone {
        let w = self.w.clamp(1, grid.cols.max(1));
        let h = self.h.clamp(1, grid.rows.max(1));
        let col = self.col.min(grid.cols.saturating_sub(w));
        let row = self.row.min(grid.rows.saturating_sub(h));
        Zone::new(col, row, w, h)
    }

    /// The same *proportion* of the screen on a grid of another size. Used when a window
    /// moves to a display with a different grid: a quarter of an 8 × 8 grid (4 × 4) becomes
    /// a quarter of an 8 × 4 grid (4 × 2).
    pub fn rescaled(&self, from: GridSize, to: GridSize) -> Zone {
        if from == to {
            return self.clamped(to);
        }
        let scale = |v: u8, f: u8, t: u8| -> u8 {
            (f64::from(v) * f64::from(t) / f64::from(f.max(1))).round() as u8
        };
        let z = self.clamped(from);
        let c0 = scale(z.col, from.cols, to.cols).min(to.cols - 1);
        let c1 = scale(z.col + z.w, from.cols, to.cols).clamp(c0 + 1, to.cols);
        let r0 = scale(z.row, from.rows, to.rows).min(to.rows - 1);
        let r1 = scale(z.row + z.h, from.rows, to.rows).clamp(r0 + 1, to.rows);
        Zone::new(c0, r0, c1 - c0, r1 - r0)
    }

    /// The zone shifted one cell in `dir`, staying inside the grid.
    pub fn moved(&self, dir: Direction, grid: GridSize) -> Zone {
        let z = self.clamped(grid);
        match dir {
            Direction::Left => Zone {
                col: z.col.saturating_sub(1),
                ..z
            },
            Direction::Right => Zone {
                col: (z.col + 1).min(grid.cols - z.w),
                ..z
            },
            Direction::Up => Zone {
                row: z.row.saturating_sub(1),
                ..z
            },
            Direction::Down => Zone {
                row: (z.row + 1).min(grid.rows - z.h),
                ..z
            },
        }
    }

    /// The zone with its edge on the `dir` side pushed out by one cell (if there is room).
    pub fn grown(&self, dir: Direction, grid: GridSize) -> Zone {
        let z = self.clamped(grid);
        match dir {
            Direction::Left if z.col > 0 => Zone {
                col: z.col - 1,
                w: z.w + 1,
                ..z
            },
            Direction::Right if z.col + z.w < grid.cols => Zone { w: z.w + 1, ..z },
            Direction::Up if z.row > 0 => Zone {
                row: z.row - 1,
                h: z.h + 1,
                ..z
            },
            Direction::Down if z.row + z.h < grid.rows => Zone { h: z.h + 1, ..z },
            _ => z,
        }
    }

    /// The zone with its edge on the `dir` side pulled in by one cell (never smaller
    /// than one cell).
    pub fn shrunk(&self, dir: Direction, grid: GridSize) -> Zone {
        let z = self.clamped(grid);
        match dir {
            Direction::Left if z.w > 1 => Zone {
                col: z.col + 1,
                w: z.w - 1,
                ..z
            },
            Direction::Right if z.w > 1 => Zone { w: z.w - 1, ..z },
            Direction::Up if z.h > 1 => Zone {
                row: z.row + 1,
                h: z.h - 1,
                ..z
            },
            Direction::Down if z.h > 1 => Zone { h: z.h - 1, ..z },
            _ => z,
        }
    }
}

/// The area that is divided into cells: the work area shrunk by half a gap per side.
fn cell_area(area: Rect, gap: f64) -> Rect {
    let half = gap.max(0.0) / 2.0;
    area.inset(half, half, half, half)
}

/// Position of grid line `i` (of `n` cells) along an axis starting at `start` of length `len`.
///
/// Computing each line from the start (instead of accumulating cell widths) guarantees
/// neighbouring zones share exactly the same boundary and the last line equals the end.
fn grid_line(start: f64, len: f64, i: u8, n: u8) -> f64 {
    start + len * f64::from(i) / f64::from(n)
}

/// Converts a zone into a pixel rectangle inside `area` (normally a display's work area).
pub fn zone_rect(area: Rect, grid: GridSize, zone: Zone, gap: f64) -> Rect {
    let z = zone.clamped(grid);
    let cells = cell_area(area, gap);
    let x0 = grid_line(cells.x, cells.w, z.col, grid.cols);
    let x1 = grid_line(cells.x, cells.w, z.col + z.w, grid.cols);
    let y0 = grid_line(cells.y, cells.h, z.row, grid.rows);
    let y1 = grid_line(cells.y, cells.h, z.row + z.h, grid.rows);
    let half = gap.max(0.0) / 2.0;
    Rect::new(x0, y0, x1 - x0, y1 - y0).inset(half, half, half, half)
}

/// Rounds a coordinate to the nearest grid line index along one axis.
fn nearest_line(pos: f64, start: f64, len: f64, n: u8) -> u8 {
    if len <= 0.0 {
        return 0;
    }
    let idx = ((pos - start) / len * f64::from(n)).round();
    idx.clamp(0.0, f64::from(n)) as u8
}

/// Finds the zone whose edges are closest to `rect`. Used when the user places a window by
/// hand and asks Venecita to "snap" or "pin" it.
pub fn snap(area: Rect, grid: GridSize, rect: Rect, gap: f64) -> Zone {
    let cells = cell_area(area, gap);
    // Start lines are capped at `n - 1` so there is always room for one cell, and end
    // lines are forced past the start so the zone is never empty.
    let c0 = nearest_line(rect.x, cells.x, cells.w, grid.cols).min(grid.cols - 1);
    let c1 = nearest_line(rect.right(), cells.x, cells.w, grid.cols).max(c0 + 1);
    let r0 = nearest_line(rect.y, cells.y, cells.h, grid.rows).min(grid.rows - 1);
    let r1 = nearest_line(rect.bottom(), cells.y, cells.h, grid.rows).max(r0 + 1);
    Zone::new(c0, r0, c1 - c0, r1 - r0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const AREA: Rect = Rect::new(0.0, 0.0, 800.0, 400.0);
    const G84: GridSize = GridSize { cols: 8, rows: 4 };

    #[test]
    fn default_grid_is_8x4() {
        assert_eq!(GridSize::default(), G84);
        assert!(G84.is_valid());
        assert!(!GridSize { cols: 0, rows: 4 }.is_valid());
        assert!(!GridSize { cols: 33, rows: 4 }.is_valid());
    }

    #[test]
    fn zone_rect_without_gap() {
        assert_eq!(
            zone_rect(AREA, G84, Zone::new(0, 0, 1, 1), 0.0),
            Rect::new(0.0, 0.0, 100.0, 100.0)
        );
        assert_eq!(
            zone_rect(AREA, G84, Zone::new(2, 1, 4, 2), 0.0),
            Rect::new(200.0, 100.0, 400.0, 200.0)
        );
        assert_eq!(zone_rect(AREA, G84, G84.full_zone(), 0.0), AREA);
    }

    #[test]
    fn zone_rect_with_gap_has_outer_and_inner_gap() {
        let gap = 10.0;
        let a = zone_rect(AREA, G84, Zone::new(0, 0, 4, 4), gap);
        let b = zone_rect(AREA, G84, Zone::new(4, 0, 4, 4), gap);
        assert_eq!(a.x, 10.0, "outer gap on the left");
        assert_eq!(a.y, 10.0, "outer gap on the top");
        assert_eq!(b.x - a.right(), 10.0, "inner gap between zones");
        assert_eq!(AREA.right() - b.right(), 10.0, "outer gap on the right");
        assert_eq!(AREA.bottom() - b.bottom(), 10.0, "outer gap at the bottom");
        assert_eq!(a.w, b.w, "both halves have the same width");
    }

    #[test]
    fn zone_rect_on_monitor_with_negative_origin() {
        let area = Rect::new(-3840.0, -500.0, 3840.0, 2160.0);
        let r = zone_rect(area, G84, Zone::new(0, 0, 4, 4), 0.0);
        assert_eq!(r, Rect::new(-3840.0, -500.0, 1920.0, 2160.0));
    }

    #[test]
    fn one_by_one_grid() {
        let g = GridSize { cols: 1, rows: 1 };
        assert_eq!(zone_rect(AREA, g, g.full_zone(), 0.0), AREA);
        assert_eq!(
            snap(AREA, g, Rect::new(5.0, 5.0, 1.0, 1.0), 0.0),
            g.full_zone()
        );
    }

    #[test]
    fn snap_rounds_to_nearest_lines() {
        // Roughly columns 1..3, rows 0..2 drawn by hand.
        let r = Rect::new(104.0, 3.0, 190.0, 210.0);
        assert_eq!(snap(AREA, G84, r, 0.0), Zone::new(1, 0, 2, 2));
    }

    #[test]
    fn snap_tiny_window_gets_one_cell() {
        let r = Rect::new(310.0, 110.0, 5.0, 5.0);
        assert_eq!(snap(AREA, G84, r, 0.0), Zone::new(3, 1, 1, 1));
    }

    #[test]
    fn snap_window_outside_area_is_clamped() {
        let r = Rect::new(-500.0, 900.0, 50.0, 50.0);
        let z = snap(AREA, G84, r, 0.0);
        assert!(z.fits(G84), "{z:?}");
        let r = Rect::new(790.0, 0.0, 500.0, 50.0);
        let z = snap(AREA, G84, r, 0.0);
        assert!(z.fits(G84), "{z:?}");
        assert_eq!(z.col + z.w, 8);
    }

    #[test]
    fn move_grow_shrink() {
        let z = Zone::new(0, 0, 2, 2);
        assert_eq!(z.moved(Direction::Left, G84), z, "already at the left edge");
        assert_eq!(z.moved(Direction::Right, G84), Zone::new(1, 0, 2, 2));
        assert_eq!(z.moved(Direction::Down, G84), Zone::new(0, 1, 2, 2));
        assert_eq!(z.grown(Direction::Right, G84), Zone::new(0, 0, 3, 2));
        assert_eq!(z.grown(Direction::Left, G84), z, "no room on the left");
        assert_eq!(z.shrunk(Direction::Left, G84), Zone::new(1, 0, 1, 2));
        assert_eq!(z.shrunk(Direction::Down, G84), Zone::new(0, 0, 2, 1));
        let one = Zone::new(3, 3, 1, 1);
        assert_eq!(
            one.shrunk(Direction::Right, G84),
            one,
            "can't go below one cell"
        );
        assert_eq!(
            one.moved(Direction::Down, G84),
            one,
            "already at the bottom"
        );
    }

    #[test]
    fn clamped_fits_smaller_grid() {
        let z = Zone::new(6, 2, 2, 2);
        let small = GridSize { cols: 4, rows: 2 };
        assert_eq!(z.clamped(small), Zone::new(2, 0, 2, 2));
        let huge = Zone::new(0, 0, 20, 20);
        assert_eq!(huge.clamped(small), small.full_zone());
    }

    #[test]
    fn rescaled_keeps_proportions() {
        let g88 = GridSize { cols: 8, rows: 8 };
        assert_eq!(
            Zone::new(0, 4, 4, 4).rescaled(g88, G84),
            Zone::new(0, 2, 4, 2)
        );
        assert_eq!(
            Zone::new(0, 2, 4, 2).rescaled(G84, g88),
            Zone::new(0, 4, 4, 4)
        );
        assert_eq!(g88.full_zone().rescaled(g88, G84), G84.full_zone());
        // A single cell never disappears.
        assert_eq!(
            Zone::new(7, 7, 1, 1).rescaled(g88, G84),
            Zone::new(7, 3, 1, 1)
        );
        assert_eq!(
            Zone::new(2, 1, 3, 2).rescaled(G84, G84),
            Zone::new(2, 1, 3, 2)
        );
    }

    fn grid_strategy() -> impl Strategy<Value = GridSize> {
        (1u8..=16, 1u8..=16).prop_map(|(cols, rows)| GridSize { cols, rows })
    }

    fn grid_and_zone() -> impl Strategy<Value = (GridSize, Zone)> {
        grid_strategy().prop_flat_map(|g| {
            (0..g.cols, 0..g.rows).prop_flat_map(move |(col, row)| {
                (1..=g.cols - col, 1..=g.rows - row)
                    .prop_map(move |(w, h)| (g, Zone::new(col, row, w, h)))
            })
        })
    }

    fn area_strategy() -> impl Strategy<Value = Rect> {
        (
            -4000.0..4000.0f64,
            -2000.0..2000.0f64,
            800.0..6000.0f64,
            600.0..3000.0f64,
        )
            .prop_map(|(x, y, w, h)| Rect::new(x, y, w, h))
    }

    proptest! {
        #[test]
        fn snap_inverts_zone_rect((g, z) in grid_and_zone(), area in area_strategy(), gap in 0.0..40.0f64) {
            let cell_w = area.w / f64::from(g.cols);
            let cell_h = area.h / f64::from(g.rows);
            prop_assume!(gap < cell_w.min(cell_h) / 2.0);
            let r = zone_rect(area, g, z, gap);
            prop_assert_eq!(snap(area, g, r, gap), z);
        }

        #[test]
        fn zone_rect_stays_inside_area((g, z) in grid_and_zone(), area in area_strategy(), gap in 0.0..40.0f64) {
            let r = zone_rect(area, g, z, gap);
            prop_assert!(area.contains_rect(&r), "{:?} not inside {:?}", r, area);
        }

        #[test]
        fn adjacent_zones_are_separated_by_exactly_gap(g in grid_strategy(), area in area_strategy(), gap in 0.0..40.0f64) {
            prop_assume!(g.cols >= 2);
            let left = zone_rect(area, g, Zone::new(0, 0, 1, g.rows), gap);
            let right = zone_rect(area, g, Zone::new(1, 0, g.cols - 1, g.rows), gap);
            prop_assert!((right.x - left.right() - gap).abs() < 1e-6);
        }

        #[test]
        fn keyboard_ops_always_fit((g, z) in grid_and_zone(), dir in prop_oneof![
            Just(Direction::Left), Just(Direction::Right), Just(Direction::Up), Just(Direction::Down)
        ]) {
            prop_assert!(z.moved(dir, g).fits(g));
            prop_assert!(z.grown(dir, g).fits(g));
            prop_assert!(z.shrunk(dir, g).fits(g));
        }

        #[test]
        fn rescaled_always_fits((g, z) in grid_and_zone(), to in grid_strategy()) {
            prop_assert!(z.rescaled(g, to).fits(to));
        }

        #[test]
        fn snap_always_fits(g in grid_strategy(), area in area_strategy(),
                            x in -8000.0..8000.0f64, y in -4000.0..4000.0f64,
                            w in 0.0..8000.0f64, h in 0.0..4000.0f64) {
            prop_assert!(snap(area, g, Rect::new(x, y, w, h), 0.0).fits(g));
        }
    }
}
