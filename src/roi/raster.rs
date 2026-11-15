//! ROI rasterization: edge-table scanline polygon fill, Sutherland-Hodgman
//! polygon clipping, even-odd point-in-polygon containment, Bresenham line
//! rasterization, and frame pixel sampling.
//!
//! Ported from `IsiXROI/Sources/Rasterization/` (`ROIRasterConversion.swift`,
//! `ROIScanline.swift`, `ROIPolygonClip.swift`, `ROIPolygonFill.swift`,
//! `ROIMasks.swift`).

use super::geometry::RoiPoint;

/// The C conversions the ROI rasterization of `DCMPix` made (`ROIRasterConversion.swift`):
/// a float to an integer truncates toward zero and saturates (`NaN` gives `0`).
pub struct RoiRasterConversion;

impl RoiRasterConversion {
    #[inline]
    pub fn long_f64(d: f64) -> isize {
        if d.is_nan() {
            return 0;
        }
        if d >= 9_223_372_036_854_775_807.0 {
            return isize::MAX;
        }
        if d <= -9_223_372_036_854_775_808.0 {
            return isize::MIN;
        }
        d as isize
    }

    #[inline]
    pub fn long_f32(f: f32) -> isize {
        Self::long_f64(f as f64)
    }

    #[inline]
    pub fn int32(d: f64) -> i32 {
        if d.is_nan() {
            return 0;
        }
        if d >= 2_147_483_647.0 {
            return i32::MAX;
        }
        if d <= -2_147_483_648.0 {
            return i32::MIN;
        }
        d as i32
    }

    #[inline]
    pub fn byte(f: f32) -> u8 {
        Self::int32(f as f64) as u8
    }
}

/// A vertex of a ROI on the pixel grid (`ROIPixelPoint` in `ROIRasterConversion.swift`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoiPixelPoint {
    pub x: isize,
    pub y: isize,
}

impl RoiPixelPoint {
    #[inline]
    pub const fn new(x: isize, y: isize) -> Self {
        Self { x, y }
    }

    #[inline]
    pub fn from_point(point: RoiPoint) -> Self {
        Self {
            x: RoiRasterConversion::long_f64(point.x),
            y: RoiRasterConversion::long_f64(point.y),
        }
    }
}

/// The rows a polygon covers and, on each, the columns where it enters and
/// leaves: the edge-table scanline fill of `DCMPix` (`ras_FillPolygon` / `ROIScanline.swift`).
pub struct RoiScanline;

#[derive(Clone, Copy)]
struct Edge {
    y_top: isize,
    y_bottom: isize,
    x: isize,
    numerator: isize,
    denominator: isize,
    direction: isize,
    step: isize,
}

impl Edge {
    #[inline]
    fn advance(&mut self) {
        self.numerator = self.numerator.wrapping_add(self.step);
        while self.numerator >= self.denominator {
            self.x = self.x.wrapping_add(self.direction);
            self.numerator = self.numerator.wrapping_sub(self.denominator);
        }
    }
}

impl RoiScanline {
    pub const MAXIMUM_ROWS: isize = 100_000;

    #[inline]
    fn sign(x: isize) -> isize {
        if x > 0 {
            1
        } else if x < 0 {
            -1
        } else {
            0
        }
    }

    /// Calls `row_fn` with each row below `rows` the polygon covers and the
    /// sorted active edge columns there (an even count, ascending).
    pub fn for_each_row<F>(polygon: &[RoiPixelPoint], rows: isize, mut row_fn: F)
    where
        F: FnMut(isize, &[isize]),
    {
        let n = polygon.len();
        if n == 0 {
            return;
        }
        let mut pending = Vec::with_capacity(n);
        let mut first_row: isize = 0;
        let mut last_row: isize = 0;

        for i in 0..n {
            let p1 = polygon[i];
            let p2 = polygon[if i + 1 == n { 0 } else { i + 1 }];
            if p1.y == p2.y {
                continue;
            }
            if p1.y < 0
                || p2.y < 0
                || p1.y >= Self::MAXIMUM_ROWS
                || p2.y >= Self::MAXIMUM_ROWS
            {
                continue;
            }
            let mut j = (i + 2) % n;
            let mut guard_steps = 0;
            while polygon[j].y == p2.y && guard_steps < n {
                j = (j + 1) % n;
                guard_steps += 1;
            }
            let p3 = polygon[j];

            let step = (p1.x.wrapping_sub(p2.x)).abs();
            let e = if p2.y > p1.y {
                let denom = p2.y - p1.y;
                let mut e = Edge {
                    y_top: p1.y,
                    y_bottom: p2.y,
                    x: p1.x,
                    numerator: denom >> 1,
                    denominator: denom,
                    direction: Self::sign(p2.x.wrapping_sub(p1.x)),
                    step,
                };
                if p3.y > p2.y {
                    e.y_bottom -= 1;
                }
                e
            } else {
                let denom = p1.y - p2.y;
                let mut e = Edge {
                    y_top: p2.y,
                    y_bottom: p1.y,
                    x: p2.x,
                    numerator: denom >> 1,
                    denominator: denom,
                    direction: Self::sign(p1.x.wrapping_sub(p2.x)),
                    step,
                };
                if p3.y < p2.y {
                    e.y_top += 1;
                    e.advance();
                }
                e
            };
            if pending.is_empty() || e.y_top < first_row {
                first_row = e.y_top;
            }
            if pending.is_empty() || e.y_top > last_row {
                last_row = e.y_top;
            }
            pending.push(e);
        }

        let edges = pending.len();
        if edges == 0 {
            return;
        }

        // Counting sort by `y_top`.
        let span = (last_row - first_row + 2) as usize;
        let mut starts = vec![0usize; span];
        for e in &pending {
            starts[(e.y_top - first_row + 1) as usize] += 1;
        }
        for k in 1..=((last_row - first_row) as usize) {
            starts[k] += starts[k - 1];
        }
        let mut table = vec![pending[0]; edges];
        for e in &pending {
            let slot = (e.y_top - first_row) as usize;
            table[starts[slot]] = *e;
            starts[slot] += 1;
        }

        let mut active: Vec<Edge> = Vec::with_capacity(n);
        let mut columns: Vec<isize> = Vec::with_capacity(n + 1);
        let mut y = table[0].y_top;
        let mut next = 0usize;

        loop {
            active.retain(|e| e.y_bottom >= y);
            while next < edges && table[next].y_top == y {
                active.push(table[next]);
                next += 1;
            }
            if active.is_empty() {
                return;
            }
            if y < rows {
                columns.clear();
                for e in &mut active {
                    let x = e.x;
                    let pos = columns.partition_point(|&c| c <= x);
                    columns.insert(pos, x);
                    e.advance();
                }
                if columns.len() % 2 == 1
                    && let Some(&last) = columns.last() {
                        columns.push(last);
                    }
                row_fn(y, &columns);
            }
            y += 1;
        }
    }
}

/// A polygon cut to a rectangle of the image, one side after the other (`ROIPolygonClip.swift`).
pub struct RoiPolygonClip;

impl RoiPolygonClip {
    /// `polygon` clipped to the pixels from `min` to `max` (exclusive).
    pub fn clip(polygon: &[RoiPixelPoint], min: RoiPoint, max: RoiPoint) -> Vec<RoiPixelPoint> {
        let up_left = RoiPixelPoint::new(
            RoiRasterConversion::long_f64(min.x),
            RoiRasterConversion::long_f64(min.y),
        );
        let down_right = RoiPixelPoint::new(
            RoiRasterConversion::long_f64(max.x - 1.0),
            RoiRasterConversion::long_f64(max.y - 1.0),
        );

        let mut left = Vec::with_capacity(polygon.len() * 2);
        for v in 0..polygon.len() {
            Self::clip_left(
                &mut left,
                polygon[v],
                polygon[if v + 1 == polygon.len() { 0 } else { v + 1 }],
                up_left,
            );
        }
        let mut right = Vec::with_capacity(left.len() * 2);
        for v in 0..left.len() {
            Self::clip_right(
                &mut right,
                left[v],
                left[if v + 1 == left.len() { 0 } else { v + 1 }],
                down_right,
            );
        }
        let mut top = Vec::with_capacity(right.len() * 2);
        for v in 0..right.len() {
            Self::clip_top(
                &mut top,
                right[v],
                right[if v + 1 == right.len() { 0 } else { v + 1 }],
                up_left,
            );
        }
        let mut bottom = Vec::with_capacity(top.len() * 2);
        for v in 0..top.len() {
            Self::clip_bottom(
                &mut bottom,
                top[v],
                top[if v + 1 == top.len() { 0 } else { v + 1 }],
                down_right,
            );
        }
        bottom
    }

    #[inline]
    fn slope(v1: RoiPixelPoint, v2: RoiPixelPoint) -> (f32, f32) {
        let dx = v2.x.wrapping_sub(v1.x) as f32;
        let dy = v2.y.wrapping_sub(v1.y) as f32;
        (dx, if dx != 0.0 { dy / dx } else { 1.0 })
    }

    #[inline]
    fn y_at(x: isize, v1: RoiPixelPoint, m: f32) -> isize {
        RoiRasterConversion::long_f32(m.mul_add(x.wrapping_sub(v1.x) as f32, v1.y as f32))
    }

    #[inline]
    fn x_at(y: isize, v1: RoiPixelPoint, dx: f32, m: f32) -> isize {
        if dx != 0.0 {
            RoiRasterConversion::long_f32((v1.x as f32) + (y.wrapping_sub(v1.y) as f32) / m)
        } else {
            v1.x
        }
    }

    fn clip_left(
        out: &mut Vec<RoiPixelPoint>,
        v1: RoiPixelPoint,
        v2: RoiPixelPoint,
        up_left: RoiPixelPoint,
    ) {
        let (_, m) = Self::slope(v1, v2);
        if v1.x >= up_left.x && v2.x >= up_left.x {
            out.push(v2);
        }
        if v1.x >= up_left.x && v2.x < up_left.x {
            out.push(RoiPixelPoint::new(up_left.x, Self::y_at(up_left.x, v1, m)));
        }
        if v1.x < up_left.x && v2.x >= up_left.x {
            out.push(RoiPixelPoint::new(up_left.x, Self::y_at(up_left.x, v1, m)));
            out.push(v2);
        }
    }

    fn clip_right(
        out: &mut Vec<RoiPixelPoint>,
        v1: RoiPixelPoint,
        v2: RoiPixelPoint,
        down_right: RoiPixelPoint,
    ) {
        let (_, m) = Self::slope(v1, v2);
        if v1.x <= down_right.x && v2.x <= down_right.x {
            out.push(v2);
        }
        if v1.x <= down_right.x && v2.x > down_right.x {
            out.push(RoiPixelPoint::new(
                down_right.x,
                Self::y_at(down_right.x, v1, m),
            ));
        }
        if v1.x > down_right.x && v2.x <= down_right.x {
            out.push(RoiPixelPoint::new(
                down_right.x,
                Self::y_at(down_right.x, v1, m),
            ));
            out.push(v2);
        }
    }

    fn clip_top(
        out: &mut Vec<RoiPixelPoint>,
        v1: RoiPixelPoint,
        v2: RoiPixelPoint,
        up_left: RoiPixelPoint,
    ) {
        let (dx, m) = Self::slope(v1, v2);
        if v1.y >= up_left.y && v2.y >= up_left.y {
            out.push(v2);
        }
        if v1.y >= up_left.y && v2.y < up_left.y {
            out.push(RoiPixelPoint::new(
                Self::x_at(up_left.y, v1, dx, m),
                up_left.y,
            ));
        }
        if v1.y < up_left.y && v2.y >= up_left.y {
            out.push(RoiPixelPoint::new(
                Self::x_at(up_left.y, v1, dx, m),
                up_left.y,
            ));
            out.push(v2);
        }
    }

    fn clip_bottom(
        out: &mut Vec<RoiPixelPoint>,
        v1: RoiPixelPoint,
        v2: RoiPixelPoint,
        down_right: RoiPixelPoint,
    ) {
        let (dx, m) = Self::slope(v1, v2);
        if v1.y <= down_right.y && v2.y <= down_right.y {
            out.push(v2);
        }
        if v1.y <= down_right.y && v2.y > down_right.y {
            out.push(RoiPixelPoint::new(
                Self::x_at(down_right.y, v1, dx, m),
                down_right.y,
            ));
        }
        if v1.y > down_right.y && v2.y <= down_right.y {
            out.push(RoiPixelPoint::new(
                Self::x_at(down_right.y, v1, dx, m),
                down_right.y,
            ));
            out.push(v2);
        }
    }
}

#[inline]
fn clamped(value: isize, low: isize, high: isize) -> isize {
    let mut r = value;
    if r > high {
        r = high;
    }
    if r < low {
        r = low;
    }
    r
}

/// Sampling and filling pixels inside a polygon on an image (`ROIPolygonFill.swift`).
pub struct RoiPolygonFill;

impl RoiPolygonFill {
    /// Extracts the grey values of the pixels inside `polygon` on an image of
    /// `width` × `height`, row by row, matching `ROIPolygonFill.sample`.
    pub fn sample_gray(
        polygon: &[RoiPixelPoint],
        image: &[f32],
        width: usize,
        height: usize,
    ) -> Vec<f32> {
        if width == 0 || height == 0 || image.len() < width * height {
            return Vec::new();
        }
        let w = width as isize;
        let h = height as isize;
        let mut out = Vec::new();
        RoiScanline::for_each_row(polygon, h, |y, row| {
            let mut i = 0;
            while i + 1 < row.len() {
                let start = clamped(row[i], 0, w);
                let end = clamped(row[i + 1], 0, w - 1);
                i += 2;
                if end < start {
                    continue;
                }
                let run = (end - start + 1) as usize;
                let first = (y as usize) * width + (start as usize);
                out.extend_from_slice(&image[first..first + run]);
            }
        });
        out
    }

    /// Extracts the mean RGB intensity `(R + G + B) / 3.0` of the pixels inside
    /// `polygon` on an RGBA8 image of `width` × `height`.
    pub fn sample_rgba(
        polygon: &[RoiPixelPoint],
        rgba: &[u8],
        width: usize,
        height: usize,
    ) -> Vec<f32> {
        if width == 0 || height == 0 || rgba.len() < width * height * 4 {
            return Vec::new();
        }
        let w = width as isize;
        let h = height as isize;
        let mut out = Vec::new();
        RoiScanline::for_each_row(polygon, h, |y, row| {
            let mut i = 0;
            while i + 1 < row.len() {
                let start = clamped(row[i], 0, w);
                let end = clamped(row[i + 1], 0, w - 1);
                i += 2;
                if end < start {
                    continue;
                }
                let run = (end - start + 1) as usize;
                let first = (y as usize) * width + (start as usize);
                for k in 0..run {
                    let p = (first + k) * 4;
                    let mean =
                        ((rgba[p] as u32 + rgba[p + 1] as u32 + rgba[p + 2] as u32) as f64 / 3.0)
                            as f32;
                    out.push(mean);
                }
            }
        });
        out
    }
}

/// The pixels of a line ROI, from its first point to its second: Bresenham's
/// line, both ends included (`ROILineRaster` in `ROIMasks.swift`).
pub struct RoiLineRaster;

impl RoiLineRaster {
    pub fn pixels(mut ax: i32, mut ay: i32, bx: i32, by: i32) -> (Vec<isize>, Vec<isize>) {
        let mut d_x = bx.wrapping_sub(ax).abs();
        let mut d_y = by.wrapping_sub(ay).abs();
        let cap = (d_x as usize) + (d_y as usize) + 2;
        let mut xs = Vec::with_capacity(cap);
        let mut ys = Vec::with_capacity(cap);
        let x_inc: i32 = if ax > bx { -1 } else { 1 };
        let y_inc: i32 = if ay > by { -1 } else { 1 };
        if d_x >= d_y {
            let right = d_y << 1;
            let right_up = right.wrapping_sub(d_x << 1);
            let mut decision = right.wrapping_sub(d_x);
            while d_x >= 0 {
                xs.push(ax as isize);
                ys.push(ay as isize);
                if decision > 0 {
                    ax = ax.wrapping_add(x_inc);
                    ay = ay.wrapping_add(y_inc);
                    decision = decision.wrapping_add(right_up);
                } else {
                    ax = ax.wrapping_add(x_inc);
                    decision = decision.wrapping_add(right);
                }
                d_x -= 1;
            }
        } else {
            let right = d_x << 1;
            let right_up = right.wrapping_sub(d_y << 1);
            let mut decision = right.wrapping_sub(d_y);
            while d_y >= 0 {
                xs.push(ax as isize);
                ys.push(ay as isize);
                if decision > 0 {
                    ax = ax.wrapping_add(x_inc);
                    ay = ay.wrapping_add(y_inc);
                    decision = decision.wrapping_add(right_up);
                } else {
                    ay = ay.wrapping_add(y_inc);
                    decision = decision.wrapping_add(right);
                }
                d_y -= 1;
            }
        }
        (xs, ys)
    }
}

/// Whether a point lies inside a polygon, by the even-odd rule (`pnpoly` / `ROIPolygonContainment` in `ROIMasks.swift`).
pub struct RoiPolygonContainment;

impl RoiPolygonContainment {
    pub fn contains(polygon: &[RoiPoint], x: f32, y: f32) -> bool {
        if polygon.is_empty() {
            return false;
        }
        let mut inside = false;
        let px = x as f64;
        let py = y as f64;
        let mut j = polygon.len() - 1;
        for i in 0..polygon.len() {
            let pi = polygon[i];
            let pj = polygon[j];
            if ((pi.y <= py && py < pj.y) || (pj.y <= py && py < pi.y))
                && px < (pj.x - pi.x) * (py - pi.y) / (pj.y - pi.y) + pi.x
            {
                inside = !inside;
            }
            j = i;
        }
        inside
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x0: isize, y0: isize, x1: isize, y1: isize) -> [RoiPixelPoint; 4] {
        [
            RoiPixelPoint::new(x0, y0),
            RoiPixelPoint::new(x1, y0),
            RoiPixelPoint::new(x1, y1),
            RoiPixelPoint::new(x0, y1),
        ]
    }

    #[test]
    fn square_covers_its_rows_and_columns() {
        let mut rows = Vec::new();
        RoiScanline::for_each_row(&square(2, 3, 6, 7), 100, |y, cols| {
            rows.push((y, cols.to_vec()));
        });
        let ys: Vec<isize> = rows.iter().map(|r| r.0).collect();
        assert_eq!(ys, vec![3, 4, 5, 6, 7]);
        assert!(rows.iter().all(|r| r.1 == vec![2, 6]));
    }

    #[test]
    fn rows_past_the_limit_are_not_given() {
        let mut rows = Vec::new();
        RoiScanline::for_each_row(&square(0, 0, 4, 10), 5, |y, _| rows.push(y));
        assert_eq!(rows, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn samples_come_row_by_row() {
        let image: Vec<f32> = (0..36).map(|i| i as f32).collect();
        let values = RoiPolygonFill::sample_gray(&square(1, 1, 2, 2), &image, 6, 6);
        assert_eq!(values, vec![7.0, 8.0, 13.0, 14.0]);
    }

    #[test]
    fn clip_keeps_the_rectangle() {
        let clipped = RoiPolygonClip::clip(
            &square(-5, -5, 20, 20),
            RoiPoint::ZERO,
            RoiPoint::new(10.0, 8.0),
        );
        assert!(clipped
            .iter()
            .all(|p| p.x >= 0 && p.x <= 9 && p.y >= 0 && p.y <= 7));
    }

    #[test]
    fn line_includes_both_ends() {
        let (xs, ys) = RoiLineRaster::pixels(0, 0, 4, 2);
        assert_eq!(xs, vec![0, 1, 2, 3, 4]);
        assert_eq!(ys, vec![0, 0, 1, 1, 2]);
        let (sxs, sys) = RoiLineRaster::pixels(3, 5, 3, 1);
        assert_eq!(sxs, vec![3, 3, 3, 3, 3]);
        assert_eq!(sys, vec![5, 4, 3, 2, 1]);
    }

    #[test]
    fn containment_is_even_odd() {
        let bowtie = [
            RoiPoint::new(0.0, 0.0),
            RoiPoint::new(10.0, 10.0),
            RoiPoint::new(10.0, 0.0),
            RoiPoint::new(0.0, 10.0),
        ];
        assert!(RoiPolygonContainment::contains(&bowtie, 8.0, 5.0));
        assert!(!RoiPolygonContainment::contains(&bowtie, 5.0, 2.0));
    }
}
