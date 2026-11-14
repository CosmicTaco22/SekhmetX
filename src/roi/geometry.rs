//! The plane geometry of a ROI's points, in image pixels: lengths, angles,
//! areas, perimeters, centroids, bounding boxes, drag constraints, and cubic
//! spline smoothing.
//!
//! Ported from `IsiXROI` (`ROIPlanarGeometry.swift`, `ROIBoundingBox.swift`,
//! `ROIDragConstraint.swift`, `ROISpline.swift`).

/// A 2D point in image pixel coordinates (`f64`), matching `CGPoint`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RoiPoint {
    pub x: f64,
    pub y: f64,
}

impl RoiPoint {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    #[inline]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// A 2D size (`f64`), matching `CGSize`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RoiSize {
    pub width: f64,
    pub height: f64,
}

impl RoiSize {
    #[inline]
    pub const fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }
}

/// A 2D rectangle (`f64`), matching `CGRect`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RoiRect {
    pub origin: RoiPoint,
    pub size: RoiSize,
}

impl RoiRect {
    #[inline]
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            origin: RoiPoint::new(x, y),
            size: RoiSize::new(width, height),
        }
    }

    #[inline]
    pub fn min_x(&self) -> f64 {
        self.origin.x.min(self.origin.x + self.size.width)
    }

    #[inline]
    pub fn max_x(&self) -> f64 {
        self.origin.x.max(self.origin.x + self.size.width)
    }

    #[inline]
    pub fn min_y(&self) -> f64 {
        self.origin.y.min(self.origin.y + self.size.height)
    }

    #[inline]
    pub fn max_y(&self) -> f64 {
        self.origin.y.max(self.origin.y + self.size.height)
    }
}

/// How a rectangle and an oval ROI keep their extent in `ROI.rect`, from and
/// to the two opposite corners of their bounding box (`ROIBoundingBox.swift`).
///
/// A rectangle stores its origin and size; an oval stores its centre and its
/// two radii (`size.width` = radiusX, `size.height` = radiusY).
pub struct RoiBoundingBox;

impl RoiBoundingBox {
    /// The `ROI.rect` of a rectangle or an oval whose bounding box has these
    /// opposite corners, in any order.
    pub fn rect(corner_a: RoiPoint, corner_b: RoiPoint, oval: bool) -> RoiRect {
        let min_x = corner_a.x.min(corner_b.x);
        let min_y = corner_a.y.min(corner_b.y);
        let width = (corner_b.x - corner_a.x).abs();
        let height = (corner_b.y - corner_a.y).abs();
        if !oval {
            RoiRect::new(min_x, min_y, width, height)
        } else {
            RoiRect::new(
                min_x + width / 2.0,
                min_y + height / 2.0,
                width / 2.0,
                height / 2.0,
            )
        }
    }

    /// The bounding box's lower and upper corners of a rectangle's or an
    /// oval's `ROI.rect`.
    pub fn corners(of: RoiRect, oval: bool) -> (RoiPoint, RoiPoint) {
        if !oval {
            (
                RoiPoint::new(of.min_x(), of.min_y()),
                RoiPoint::new(of.max_x(), of.max_y()),
            )
        } else {
            let rx = of.size.width.abs();
            let ry = of.size.height.abs();
            (
                RoiPoint::new(of.origin.x - rx, of.origin.y - ry),
                RoiPoint::new(of.origin.x + rx, of.origin.y + ry),
            )
        }
    }
}

/// How a ROI being dragged is held to a shape (`ROIDragConstraint.swift`):
/// an oval to a circle, a rectangle to a square, a length to the horizontal,
/// vertical, or 45-degree diagonal in calibrated millimetres.
pub struct RoiDragConstraint;

impl RoiDragConstraint {
    /// How many of a pixel's widths make its height: 1 when either spacing is unknown.
    pub fn pixel_ratio(spacing_x: f64, spacing_y: f64) -> f64 {
        if spacing_x > 0.0 && spacing_y > 0.0 {
            spacing_y / spacing_x
        } else {
            1.0
        }
    }

    /// The width, in pixels, of a circle or a square of this height, on the
    /// side the width was dragged to.
    pub fn constrained_width(width: f64, height: f64, ratio: f64) -> f64 {
        (height.abs() * ratio).copysign(width)
    }

    /// The end of a length whose other end is `first`, on the nearest of 0, 45
    /// and 90 degrees.
    pub fn constrained_length_end(first: RoiPoint, last: RoiPoint, ratio: f64) -> RoiPoint {
        let mut out = last;
        let dx = last.x - first.x;
        let dy = last.y - first.y;
        let slope = (dy * ratio).abs() / dx.abs();
        if slope < 0.5 {
            out.y = first.y;
        } else if slope < 1.5 {
            out.y = first.y + (dx.abs() / ratio).copysign(dy);
        } else {
            out.x = first.x;
        }
        out
    }
}

/// The plane geometry of a ROI's points (`ROIPlanarGeometry.swift`).
pub struct RoiPlanarGeometry;

impl RoiPlanarGeometry {
    /// `M_PI / 180.0f`, kept in `f32` as Horos's ROI code always kept it.
    pub const DEGREES_TO_RADIANS: f32 = (std::f64::consts::PI / 180.0) as f32;

    // ------------------------------------------------------------------ Points

    /// The point at `ratio` of the way from `a` to `b`.
    pub fn point_between(a: RoiPoint, b: RoiPoint, ratio: f32) -> RoiPoint {
        let r = ratio as f64;
        RoiPoint::new(
            a.x.mul_add(1.0 - r, b.x * r),
            a.y.mul_add(1.0 - r, b.y * r),
        )
    }

    /// The distance between two points, in pixels.
    pub fn length_between(a: RoiPoint, b: RoiPoint) -> f32 {
        let dx = a.x - b.x;
        let dy = a.y - b.y;
        (dx.mul_add(dx, dy * dy) as f32).sqrt()
    }

    /// The projection of `p` on the line through `a` and `b`, not clamped to the segment.
    pub fn projection_on_segment(a: RoiPoint, b: RoiPoint, p: RoiPoint) -> RoiPoint {
        let d = RoiPoint::new(b.x - a.x, b.y - a.y);
        let f = d.x.mul_add(d.x, d.y * d.y) as f32;
        let u = ((p.x - a.x).mul_add(d.x, (p.y - a.y) * d.y) / (f as f64)) as f32;
        let x = (u as f64).mul_add(d.x, a.x) as f32;
        let y = (u as f64).mul_add(d.y, a.y) as f32;
        RoiPoint::new(x as f64, y as f64)
    }

    /// The point `distance` pixels along the closed outline through `points`.
    pub fn position_at_distance(distance: f32, points: &[RoiPoint]) -> RoiPoint {
        if points.is_empty() {
            return RoiPoint::ZERO;
        }
        if distance == 0.0 {
            return points[0];
        }
        let mut i = 0;
        let mut position: f32 = 0.0;
        while position < distance && i + 1 < points.len() {
            position += Self::length_between(points[i], points[i + 1]);
            i += 1;
        }
        if position < distance {
            position += Self::length_between(points[i], points[0]);
            i += 1;
        }
        let next = if i == points.len() {
            points[0]
        } else {
            points[i]
        };
        let seg_len = Self::length_between(points[i - 1], next);
        if seg_len == 0.0 {
            return points[i - 1];
        }
        let ratio = (position - distance) / seg_len;
        Self::point_between(points[i - 1], next, (1.0 - ratio as f64) as f32)
    }

    /// `count` points evenly spaced along the closed outline through `points`,
    /// starting at the leftmost one and turning the way the outline turns there.
    pub fn resample(points: &[RoiPoint], count: usize) -> Vec<RoiPoint> {
        if points.is_empty() || count == 0 {
            return Vec::new();
        }
        let mut total: f32 = 0.0;
        let mut i = 0;
        while i + 1 < points.len() {
            total += Self::length_between(points[i], points[i + 1]);
            i += 1;
        }
        total += Self::length_between(points[i], points[0]);

        let mut resampled = Vec::with_capacity(count);
        for n in 0..count {
            let s = (n as i32 as f32) * total / (count as i32 as f32);
            resampled.push(Self::position_at_distance(s, points));
        }

        let mut min_x = resampled[0].x as f32;
        let mut min_y = resampled[0].y as f32;
        let mut min_x_index: isize = 0;
        let mut min_y_index: isize = 0;
        for (idx, p) in resampled.iter().enumerate() {
            if min_x > p.x as f32 {
                min_x = p.x as f32;
                min_x_index = idx as isize;
            }
            if min_y > p.y as f32 {
                min_y = p.y as f32;
                min_y_index = idx as isize;
            }
        }
        let distance = min_x_index - min_y_index;
        let reverse = if distance.unsigned_abs() > resampled.len() / 2 {
            distance >= 0
        } else {
            distance < 0
        };

        let mut ordered = Vec::with_capacity(resampled.len());
        let mut index = min_x_index;
        let n_isize = resampled.len() as isize;
        for _ in 0..resampled.len() {
            ordered.push(resampled[index as usize]);
            if reverse {
                index -= 1;
                if index < 0 {
                    index = n_isize - 1;
                }
            } else {
                index += 1;
                if index == n_isize {
                    index = 0;
                }
            }
        }
        ordered
    }

    /// The outline of an oval ROI, whose frame keeps its centre and half sides:
    /// `resolution` points around it, starting on the right.
    pub fn outline_of_oval(frame: RoiRect, resolution: usize) -> Vec<RoiPoint> {
        (0..resolution)
            .map(|i| {
                let angle = (((i * 2) as f64) * std::f64::consts::PI / (resolution as f64))
                    as f32 as f64;
                RoiPoint::new(
                    frame.size.width.mul_add(angle.cos(), frame.origin.x),
                    frame.size.height.mul_add(angle.sin(), frame.origin.y),
                )
            })
            .collect()
    }

    /// The corners of a rectangle ROI, counterclockwise from its origin.
    pub fn outline_of_rectangle(frame: RoiRect) -> [RoiPoint; 4] {
        [
            RoiPoint::new(frame.min_x(), frame.min_y()),
            RoiPoint::new(frame.min_x(), frame.max_y()),
            RoiPoint::new(frame.max_x(), frame.max_y()),
            RoiPoint::new(frame.max_x(), frame.min_y()),
        ]
    }

    // ----------------------------------------------------------------- Lengths

    /// The length of a segment: in centimetres when both spacings are known
    /// (and `in_pixels` is false), in pixels otherwise.
    pub fn length(
        a: RoiPoint,
        b: RoiPoint,
        spacing_x: f64,
        spacing_y: f64,
        in_pixels: bool,
    ) -> f32 {
        let calibrated = spacing_x != 0.0 && spacing_y != 0.0 && !in_pixels;
        let mut side_a = (a.x - b.x).abs();
        let mut side_b = (a.y - b.y).abs();
        if calibrated {
            side_a *= spacing_x;
            side_b *= spacing_y;
        }
        let mut length: f32 = if side_a == 0.0 {
            side_b as f32
        } else if side_b == 0.0 {
            side_a as f32
        } else {
            (side_b / (side_b / side_a).atan().sin()) as f32
        };
        if calibrated {
            length = ((length as f64) / 10.0) as f32;
        }
        length
    }

    /// The length of a line ROI in centimetres (pixels when uncalibrated) and
    /// the same length counted in pixels of the horizontal spacing.
    pub fn measure_length(
        a: RoiPoint,
        b: RoiPoint,
        spacing_x: f64,
        spacing_y: f64,
    ) -> (f32, f32) {
        let value = Self::length(a, b, spacing_x, spacing_y, false);
        let pixels = if spacing_x != 0.0 {
            let in_mm = ((value as f64) * 10.0) as f32;
            ((in_mm as f64) / spacing_x) as f32
        } else {
            value
        };
        (value, pixels)
    }

    /// The length of the outline through `points`, closed back to the first
    /// point when `closed`, in centimetres or pixels as `length`.
    pub fn perimeter(
        points: &[RoiPoint],
        closed: bool,
        spacing_x: f64,
        spacing_y: f64,
    ) -> f32 {
        if points.len() <= 1 {
            return 0.0;
        }
        let mut total: f32 = 0.0;
        let mut i = 0;
        while i + 1 < points.len() {
            total += Self::length(points[i], points[i + 1], spacing_x, spacing_y, false);
            i += 1;
        }
        if closed {
            total += Self::length(points[i], points[0], spacing_x, spacing_y, false);
        }
        total
    }

    /// The length of a path whose points lie on several slices, in millimetres:
    /// `slices` holds each point's slice index, `slice_interval` their distance.
    pub fn path_length(
        points: &[RoiPoint],
        slices: &[i32],
        spacing_x: f64,
        spacing_y: f64,
        slice_interval: f64,
    ) -> f64 {
        let count = points.len().min(slices.len());
        if count <= 1 {
            return 0.0;
        }
        let mut total = 0.0;
        for i in 1..count {
            let dx = points[i].x * spacing_x - points[i - 1].x * spacing_x;
            let dy = points[i].y * spacing_y - points[i - 1].y * spacing_y;
            let dz = (slices[i] as f64) * slice_interval - (slices[i - 1] as f64) * slice_interval;
            total += dz.mul_add(dz, dx.mul_add(dx, dy * dy)).sqrt();
        }
        total
    }

    // ------------------------------------------------------------------ Angles

    /// The angle at `vertex` between the rays to `a` and `b`, in degrees,
    /// with the pixel spacing applied when both are known.
    pub fn angle_at(
        vertex: RoiPoint,
        a: RoiPoint,
        b: RoiPoint,
        spacing_x: f64,
        spacing_y: f64,
    ) -> f32 {
        let (px, py): (f32, f32) = if spacing_x != 0.0 && spacing_y != 0.0 {
            (spacing_x as f32, spacing_y as f32)
        } else {
            (1.0, 1.0)
        };
        let sx = px as f64;
        let sy = py as f64;
        let ax = a.x.mul_add(sx, -(vertex.x * sx)) as f32;
        let ay = a.y.mul_add(sy, -(vertex.y * sy)) as f32;
        let bx = b.x.mul_add(sx, -(vertex.x * sx)) as f32;
        let by = b.y.mul_add(sy, -(vertex.y * sy)) as f32;
        if ax == 0.0 && ay == 0.0 {
            return 0.0;
        }
        let dot = ax.mul_add(bx, ay * by);
        let lengths = (ax.mul_add(ax, ay * ay) as f64).sqrt()
            * (bx.mul_add(bx, by * by) as f64).sqrt();
        if lengths == 0.0 {
            return 0.0;
        }
        let cosine = ((dot as f64) / lengths).clamp(-1.0, 1.0) as f32;
        ((cosine as f64).acos() / (Self::DEGREES_TO_RADIANS as f64)) as f32
    }

    /// The signed angle between the line `a1`-`a2` and the line `b1`-`b2`, in degrees.
    pub fn angle_between_lines(a1: RoiPoint, a2: RoiPoint, b1: RoiPoint, b2: RoiPoint) -> f64 {
        let angle1 = (a1.y - a2.y).atan2(a1.x - a2.x);
        let angle2 = (b1.y - b2.y).atan2(b1.x - b2.x);
        (angle1 - angle2) * (180.0 / std::f64::consts::PI)
    }

    /// The acute angle between two lines, in degrees, that the dynamic angle
    /// / Cobb angle shows; pixel spacing is applied when both are known.
    pub fn acute_angle_between_lines(
        mut a1: RoiPoint,
        mut a2: RoiPoint,
        mut b1: RoiPoint,
        mut b2: RoiPoint,
        spacing_x: f64,
        spacing_y: f64,
    ) -> f32 {
        if spacing_x != 0.0 && spacing_y != 0.0 {
            a1 = RoiPoint::new(a1.x * spacing_x, a1.y * spacing_y);
            a2 = RoiPoint::new(a2.x * spacing_x, a2.y * spacing_y);
            b1 = RoiPoint::new(b1.x * spacing_x, b1.y * spacing_y);
            b2 = RoiPoint::new(b2.x * spacing_x, b2.y * spacing_y);
        }
        let mut angle = Self::angle_between_lines(a1, a2, b1, b2) as f32;
        if angle < -180.0 {
            angle = -180.0 - angle;
        } else if angle < -90.0 {
            angle += 180.0;
        } else if angle < 0.0 {
            angle *= -1.0;
        }

        if angle > 270.0 {
            angle = 360.0 - angle;
        } else if angle > 180.0 {
            angle -= 180.0;
        } else if angle > 90.0 {
            angle = 180.0 - angle;
        }
        angle
    }

    // ----------------------------------------------------- Distances to a line

    /// The distance between two points, as a `float`.
    pub fn magnitude(a: RoiPoint, b: RoiPoint) -> f32 {
        let vx = b.x - a.x;
        let vy = b.y - a.y;
        vx.mul_add(vx, vy * vy).sqrt() as f32
    }

    fn parameter(point: RoiPoint, start: RoiPoint, end: RoiPoint) -> f32 {
        let line_mag = Self::magnitude(end, start);
        if line_mag == 0.0 {
            return 0.0;
        }
        let u = (point.x - start.x).mul_add(end.x - start.x, (point.y - start.y) * (end.y - start.y))
            as f32;
        u / (line_mag * line_mag)
    }

    /// The projection of `point` on the line through `start` and `end`.
    pub fn projection(point: RoiPoint, start: RoiPoint, end: RoiPoint) -> RoiPoint {
        let u = Self::parameter(point, start, end);
        RoiPoint::new(
            (u as f64).mul_add(end.x - start.x, start.x),
            (u as f64).mul_add(end.y - start.y, start.y),
        )
    }

    /// The distance from `point` to the segment `start`-`end`, or `None` when its
    /// projection falls outside the segment (with a 1 % margin).
    pub fn distance_to_segment(point: RoiPoint, start: RoiPoint, end: RoiPoint) -> Option<f32> {
        let u = Self::parameter(point, start, end);
        if !(-0.01..=1.01).contains(&u) {
            return None;
        }
        let intersection = RoiPoint::new(
            (u as f64).mul_add(end.x - start.x, start.x),
            (u as f64).mul_add(end.y - start.y, start.y),
        );
        Some(Self::magnitude(point, intersection))
    }

    // ----------------------------------------------------- Areas and centroids

    /// The area of the polygon through `points`, in square pixels.
    pub fn area_of_polygon(points: &[RoiPoint]) -> f32 {
        if points.len() < 3 {
            return 0.0;
        }
        let mut area: f32 = 0.0;
        let count = points.len();
        for i in 0..count {
            let j = (i + 1) % count;
            let xi = points[i].x as f32;
            let yi = points[i].y as f32;
            let xj = points[j].x as f32;
            let yj = points[j].y as f32;
            area = xi.mul_add(yj, area);
            area = (-yi).mul_add(xj, area);
        }
        area *= 0.5;
        area.abs()
    }

    /// The area of the ellipse inscribed in a rectangle whose half sides are
    /// `size`, as an oval ROI keeps its frame, in square pixels.
    pub fn area_of_ellipse_with_half_size(size: RoiSize) -> f32 {
        ((std::f64::consts::PI * size.width * 2.0 * size.height * 2.0).abs() / 4.0) as f32
    }

    /// The number of painted pixels of a brush mask, in square pixels.
    pub fn area_of_mask(mask: &[u8]) -> f32 {
        Self::painted_pixels(mask) as f32
    }

    /// The painted pixels of a brush mask.
    pub fn painted_pixels(mask: &[u8]) -> usize {
        mask.iter().filter(|&&b| b != 0).count()
    }

    /// The mean of the points, accumulated as the ROI always did it: each
    /// coordinate divided in `f32`, summed in `f64`.
    pub fn centroid(points: &[RoiPoint]) -> RoiPoint {
        if points.is_empty() {
            return RoiPoint::ZERO;
        }
        let n = points.len() as f32;
        let mut c = RoiPoint::ZERO;
        for p in points {
            c.x += ((p.x as f32) / n) as f64;
            c.y += ((p.y as f32) / n) as f64;
        }
        c
    }
}

/// A cubic piece of the spline (`ROISpline.swift`).
#[derive(Clone, Copy, Debug)]
pub struct SplinePiece {
    pub x: (f64, f64, f64, f64),
    pub y: (f64, f64, f64, f64),
    pub length: f64,
    pub segment: usize,
}

/// The cubic spline that smooths a ROI's outline (`ROISpline.swift`).
pub struct RoiSpline;

impl RoiSpline {
    /// The smoothed points of the outline through `points` (the closing point
    /// repeated last for a closed one), sampled about every `scale`-th of a
    /// pixel, with the index of the segment each comes from.
    pub fn smooth(points: &[RoiPoint], scale: f64) -> (Vec<RoiPoint>, Vec<usize>) {
        let Some(pieces) = Self::pieces(points, scale) else {
            return (Vec::new(), Vec::new());
        };
        let mut count = 1;
        for p in &pieces {
            if p.length >= 1.0 && p.length < 1e6 {
                count += 1 + (p.length as usize);
            } else {
                count += 1;
            }
        }
        let mut smoothed = Vec::with_capacity(count);
        let mut segments = Vec::with_capacity(count);
        for piece in &pieces {
            smoothed.push(RoiPoint::new(piece.x.0, piece.y.0));
            segments.push(piece.segment);
            let mut j: i64 = 1;
            while (j as f64) <= piece.length {
                let jf = j as f64;
                let j2 = (j.wrapping_mul(j)) as f64;
                let j3 = (j.wrapping_mul(j).wrapping_mul(j)) as f64;
                let x = piece
                    .x
                    .3
                    .mul_add(j3, piece.x.2.mul_add(j2, piece.x.1.mul_add(jf, piece.x.0)));
                let y = piece
                    .y
                    .3
                    .mul_add(j3, piece.y.2.mul_add(j2, piece.y.1.mul_add(jf, piece.y.0)));
                smoothed.push(RoiPoint::new(x, y));
                segments.push(piece.segment);
                j += 1;
            }
        }
        smoothed.push(points[points.len() - 1]);
        segments.push(points.len() - 2);
        (smoothed, segments)
    }

    pub fn pieces(points: &[RoiPoint], mut scale: f64) -> Option<Vec<SplinePiece>> {
        let tot = points.len();
        if scale > 5.0 {
            scale = 5.0;
        }
        let nb = tot + 2;
        if nb < 4 || tot < 2 {
            return None;
        }
        let mut a = vec![0.0; nb];
        let mut c = vec![0.0; nb];
        let mut cx = vec![0.0; nb];
        let mut cy = vec![0.0; nb];
        let mut d = vec![0.0; nb];
        let mut g = vec![0.0; nb];
        let mut gam = vec![0.0; nb];
        let mut h = vec![0.0; nb];
        let mut px = vec![0.0; nb];
        let mut py = vec![0.0; nb];

        for (i, p) in points.iter().enumerate() {
            px[i + 1] = p.x;
            py[i + 1] = p.y;
        }
        px[0] = px[nb - 3];
        px[nb - 1] = px[2];
        py[0] = py[nb - 3];
        py[nb - 1] = py[2];

        for i in 1..nb {
            if px[i] == px[i - 1] && py[i] == py[i - 1] {
                return None;
            }
        }

        for i in 0..(nb - 1) {
            let xi = px[i + 1] - px[i];
            let yi = py[i + 1] - py[i];
            h[i] = xi.mul_add(xi, yi * yi).sqrt() * scale;
            d[i + 1] = d[i] + h[i];
        }

        for i in 2..(nb - 1) {
            a[i] = 2.0 * h[i - 1] / (h[i] + h[i - 1]);
        }
        for i in 1..(nb - 2) {
            c[i] = 2.0 * h[i] / (h[i] + h[i - 1]);
        }

        let mut solve = |p: &[f64], u: &mut [f64]| {
            for i in 1..(nb - 1) {
                g[i] = 6.0
                    * (((p[i - 1] - p[i]) / (d[i - 1] - d[i]))
                        - ((p[i] - p[i + 1]) / (d[i] - d[i + 1])))
                    / (d[i - 1] - d[i + 1]);
            }
            let b = 4.0;
            let mut bet = 4.0;
            u[1] = g[1] / b;
            for j in 2..(nb - 1) {
                gam[j] = c[j - 1] / bet;
                bet = (-a[j]).mul_add(gam[j], b);
                u[j] = (-a[j]).mul_add(u[j - 1], g[j]) / bet;
            }
            for j in (1..=(nb - 2)).rev() {
                u[j] = (-gam[j + 1]).mul_add(u[j + 1], u[j]);
            }
        };

        solve(&px, &mut cx);
        solve(&py, &mut cy);

        let mut out = Vec::with_capacity(nb.saturating_sub(3));
        for i in 1..(nb - 2) {
            let ddx = (cx[i + 1] - cx[i]) / (3.0 * h[i]);
            let bbx = (-(h[i] / 3.0))
                .mul_add(2.0f64.mul_add(cx[i], cx[i + 1]), (px[i + 1] - px[i]) / h[i]);
            let ddy = (cy[i + 1] - cy[i]) / (3.0 * h[i]);
            let bby = (-(h[i] / 3.0))
                .mul_add(2.0f64.mul_add(cy[i], cy[i + 1]), (py[i + 1] - py[i]) / h[i]);
            out.push(SplinePiece {
                x: (px[i], bbx, cx[i], ddx),
                y: (py[i], bby, cy[i], ddy),
                length: h[i],
                segment: i - 1,
            });
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polygon_area_is_unsigned_and_closed() {
        let triangle = [
            RoiPoint::new(10.0, 10.0),
            RoiPoint::new(40.0, 10.0),
            RoiPoint::new(10.0, 50.0),
        ];
        assert_eq!(RoiPlanarGeometry::area_of_polygon(&triangle), 600.0);
        let rev: Vec<_> = triangle.iter().copied().rev().collect();
        assert_eq!(RoiPlanarGeometry::area_of_polygon(&rev), 600.0);
        assert_eq!(RoiPlanarGeometry::area_of_polygon(&[]), 0.0);
        assert_eq!(
            RoiPlanarGeometry::area_of_polygon(&[RoiPoint::new(1.0, 1.0)]),
            0.0
        );
    }

    #[test]
    fn ellipse_and_mask_areas() {
        assert_eq!(
            RoiPlanarGeometry::area_of_ellipse_with_half_size(RoiSize::new(2.0, 3.0)),
            (std::f64::consts::PI * 6.0) as f32
        );
        let mask: [u8; 6] = [0, 255, 1, 0, 0, 9];
        assert_eq!(RoiPlanarGeometry::area_of_mask(&mask), 3.0);
    }

    #[test]
    fn lengths_in_centimetres_or_pixels() {
        let a = RoiPoint::new(0.0, 0.0);
        let b = RoiPoint::new(30.0, 40.0);
        assert_eq!(RoiPlanarGeometry::length(a, b, 0.0, 0.0, false), 50.0);
        assert_eq!(RoiPlanarGeometry::length(a, b, 0.5, 0.5, true), 50.0);
        // 15 mm by 20 mm: 25 mm, 2.5 cm.
        assert!((RoiPlanarGeometry::length(a, b, 0.5, 0.5, false) - 2.5).abs() < 1e-6);
        assert_eq!(
            RoiPlanarGeometry::length(a, RoiPoint::new(0.0, 7.0), 1.0, 2.0, false),
            1.4
        );
        let (cm, pixels) = RoiPlanarGeometry::measure_length(a, b, 0.5, 0.5);
        assert!((cm - 2.5).abs() < 1e-6 && (pixels - 50.0).abs() < 1e-4);
        let (cm0, px0) = RoiPlanarGeometry::measure_length(a, b, 0.0, 0.0);
        assert_eq!((cm0, px0), (50.0, 50.0));
    }

    #[test]
    fn perimeter_open_and_closed() {
        let triangle = [
            RoiPoint::new(0.0, 0.0),
            RoiPoint::new(30.0, 0.0),
            RoiPoint::new(0.0, 40.0),
        ];
        assert_eq!(
            RoiPlanarGeometry::perimeter(&triangle, true, 0.0, 0.0),
            120.0
        );
        assert_eq!(
            RoiPlanarGeometry::perimeter(&triangle, false, 0.0, 0.0),
            80.0
        );
        assert_eq!(
            RoiPlanarGeometry::perimeter(&triangle[..1], true, 0.0, 0.0),
            0.0
        );
    }

    #[test]
    fn path_across_slices() {
        let points = [
            RoiPoint::new(0.0, 0.0),
            RoiPoint::new(3.0, 0.0),
            RoiPoint::new(3.0, 4.0),
        ];
        let slices = [0, 0, 2];
        let length = RoiPlanarGeometry::path_length(&points, &slices, 1.0, 1.0, 1.5);
        assert_eq!(length, 8.0);
    }

    #[test]
    fn angles() {
        let vertex = RoiPoint::new(0.0, 0.0);
        assert!(
            (RoiPlanarGeometry::angle_at(
                vertex,
                RoiPoint::new(1.0, 0.0),
                RoiPoint::new(0.0, 1.0),
                0.0,
                0.0
            ) - 90.0)
                .abs()
                < 1e-4
        );
        assert_eq!(
            RoiPlanarGeometry::angle_at(vertex, vertex, RoiPoint::new(0.0, 1.0), 0.0, 0.0),
            0.0
        );
        let skewed = RoiPlanarGeometry::angle_at(
            vertex,
            RoiPoint::new(1.0, 0.0),
            RoiPoint::new(1.0, 1.0),
            1.0,
            2.0,
        );
        assert!(((skewed as f64) - 2.0f64.atan() * 180.0 / std::f64::consts::PI).abs() < 1e-4);
        let between = RoiPlanarGeometry::angle_between_lines(
            RoiPoint::new(1.0, 1.0),
            vertex,
            RoiPoint::new(1.0, 0.0),
            vertex,
        );
        assert!((between - 45.0).abs() < 1e-12);
        let acute = RoiPlanarGeometry::acute_angle_between_lines(
            vertex,
            RoiPoint::new(10.0, 0.0),
            vertex,
            RoiPoint::new(-10.0, 3.0),
            0.0,
            0.0,
        );
        assert!(((acute as f64) - 0.3f64.atan() * 180.0 / std::f64::consts::PI).abs() < 1e-4);
    }

    #[test]
    fn distances_to_a_line() {
        let start = RoiPoint::new(0.0, 0.0);
        let end = RoiPoint::new(10.0, 0.0);
        let projected = RoiPlanarGeometry::projection(RoiPoint::new(4.0, 7.0), start, end);
        assert_eq!(projected.x, (0.4f32 as f64) * 10.0);
        assert_eq!(projected.y, 0.0);
        assert_eq!(
            RoiPlanarGeometry::distance_to_segment(RoiPoint::new(4.0, 7.0), start, end),
            Some(7.0)
        );
        assert_eq!(
            RoiPlanarGeometry::distance_to_segment(RoiPoint::new(20.0, 7.0), start, end),
            None
        );
        assert_eq!(
            RoiPlanarGeometry::magnitude(start, RoiPoint::new(3.0, 4.0)),
            5.0
        );
        assert_eq!(
            RoiPlanarGeometry::projection_on_segment(start, end, RoiPoint::new(15.0, -2.0)),
            RoiPoint::new(15.0, 0.0)
        );
    }

    #[test]
    fn centroid_and_outlines() {
        let square = [
            RoiPoint::new(0.0, 0.0),
            RoiPoint::new(4.0, 0.0),
            RoiPoint::new(4.0, 2.0),
            RoiPoint::new(0.0, 2.0),
        ];
        assert_eq!(RoiPlanarGeometry::centroid(&square), RoiPoint::new(2.0, 1.0));
        assert_eq!(
            RoiPlanarGeometry::outline_of_rectangle(RoiRect::new(1.0, 2.0, 3.0, 4.0)),
            [
                RoiPoint::new(1.0, 2.0),
                RoiPoint::new(1.0, 6.0),
                RoiPoint::new(4.0, 6.0),
                RoiPoint::new(4.0, 2.0)
            ]
        );
        let oval = RoiPlanarGeometry::outline_of_oval(RoiRect::new(10.0, 20.0, 5.0, 3.0), 4);
        assert_eq!(oval.len(), 4);
        assert_eq!(oval[0], RoiPoint::new(15.0, 20.0));
        assert!((oval[1].y - 23.0).abs() < 1e-6);
        assert!((oval[2].x - 5.0).abs() < 1e-6);
    }

    #[test]
    fn bounding_box_rectangle_and_oval() {
        let rect = RoiBoundingBox::rect(
            RoiPoint::new(48.0, 40.0),
            RoiPoint::new(16.0, 8.0),
            false,
        );
        assert_eq!(rect, RoiRect::new(16.0, 8.0, 32.0, 32.0));
        let (lo, hi) = RoiBoundingBox::corners(rect, false);
        assert_eq!(lo, RoiPoint::new(16.0, 8.0));
        assert_eq!(hi, RoiPoint::new(48.0, 40.0));

        let oval_rect = RoiBoundingBox::rect(
            RoiPoint::new(16.0, 20.0),
            RoiPoint::new(48.0, 44.0),
            true,
        );
        assert_eq!(oval_rect, RoiRect::new(32.0, 32.0, 16.0, 12.0));
        let (olo, ohi) = RoiBoundingBox::corners(oval_rect, true);
        assert_eq!(olo, RoiPoint::new(16.0, 20.0));
        assert_eq!(ohi, RoiPoint::new(48.0, 44.0));
    }

    #[test]
    fn point_between_and_resampling_edges() {
        assert_eq!(
            RoiPlanarGeometry::point_between(
                RoiPoint::new(0.0, 0.0),
                RoiPoint::new(10.0, 20.0),
                0.25
            ),
            RoiPoint::new(2.5, 5.0)
        );
        assert_eq!(
            RoiPlanarGeometry::magnitude(RoiPoint::new(0.0, 0.0), RoiPoint::new(3.0, 4.0)),
            5.0
        );
    }
}
