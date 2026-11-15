// Ported from IsiX:
// - Modules/IsiXImaging/Sources/ImagePlaneGeometry.swift
// - Modules/IsiXImaging/Sources/Geometry/ImagePlaneCoordinates.swift
// - Modules/IsiXImaging/Sources/ViewGeometry/PlaneGeometry.swift
// - Modules/IsiXImaging/Sources/ViewGeometry/LineGeometry.swift
// - Modules/IsiXImaging/Sources/ViewGeometry/ViewTransform.swift
// - Modules/IsiXImaging/Sources/ViewGeometry/ViewTransform+Viewport.swift
// - Modules/IsiXImaging/Sources/ViewGeometry/ViewportState.swift
// - Modules/IsiXImaging/Tests/Geometry/ImagePlaneCoordinatesTests.swift
// - Modules/IsiXImaging/Tests/ViewGeometry/ViewGeometryTests.swift
// - Modules/IsiXImaging/Tests/ViewGeometry/ViewportTests.swift

use super::vector3::{adding_product, Vector3};

/// Helper matching Swift's `Float.addingProduct(a, b)` -> `self + a * b` fused in f32.
#[inline(always)]
fn adding_product_f32(acc: f32, a: f32, b: f32) -> f32 {
    a.mul_add(b, acc)
}

#[inline(always)]
fn dot3_f64(a: [f64; 3], b: [f64; 3]) -> f64 {
    let xy = adding_product(a[0] * b[0], a[1], b[1]);
    adding_product(xy, a[2], b[2])
}

#[inline(always)]
fn dot3_f32(a: [f32; 3], b: [f32; 3]) -> f32 {
    let xy = adding_product_f32(a[0] * b[0], a[1], b[1]);
    adding_product_f32(xy, a[2], b[2])
}

#[inline(always)]
fn cross3_f64(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        adding_product(-(a[2] * b[1]), a[1], b[2]),
        adding_product(-(a[0] * b[2]), a[2], b[0]),
        adding_product(-(a[1] * b[0]), a[0], b[1]),
    ]
}

#[inline(always)]
fn normalize3_f64(v: [f64; 3]) -> [f64; 3] {
    let length = dot3_f64(v, v).sqrt();
    if length == 0.0 {
        return [0.0, 0.0, 0.0];
    }
    [v[0] / length, v[1] / length, v[2] / length]
}

// ============================================================================
// ImagePlaneGeometry (Modules/IsiXImaging/Sources/ImagePlaneGeometry.swift)
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SettledPixelSpacing {
    pub x: f64,
    pub y: f64,
    pub ratio: f64,
}

pub struct ImagePlaneGeometry;

impl ImagePlaneGeometry {
    /// Identity direction cosines with slice normal `[1, 0, 0, 0, 1, 0, 0, 0, 1]`.
    pub const DEFAULT_ORIENTATION: [f64; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

    pub const IMPLAUSIBLE_PIXEL_SPACING_THRESHOLD: f64 = 1000.0;
    pub const ASPECT_RATIO_FALLBACK_MAX_DIMENSION: i32 = 1024;
    pub const ASPECT_RATIO_FALLBACK_FACTOR: i32 = 4;

    /// Settles pixel spacings read from tags and the aspect ratio `y / x`.
    pub fn settled_pixel_spacing(
        pixel_spacing_x: f64,
        pixel_spacing_y: f64,
        rows: i32,
        columns: i32,
        force_square_pixels: bool,
    ) -> SettledPixelSpacing {
        let mut x = pixel_spacing_x;
        let mut y = pixel_spacing_y;

        if x > Self::IMPLAUSIBLE_PIXEL_SPACING_THRESHOLD
            && y > Self::IMPLAUSIBLE_PIXEL_SPACING_THRESHOLD
        {
            x = 0.0;
            y = 0.0;
        }

        if rows <= Self::ASPECT_RATIO_FALLBACK_MAX_DIMENSION
            && columns <= Self::ASPECT_RATIO_FALLBACK_MAX_DIMENSION
            && (y >= (Self::ASPECT_RATIO_FALLBACK_FACTOR as f64) * x
                || x >= (Self::ASPECT_RATIO_FALLBACK_FACTOR as f64) * y)
        {
            let ratio = if columns != 0 && rows != 0 {
                (columns as f64) / (rows as f64)
            } else {
                1.0
            };
            let settled_y = x * ratio;
            return SettledPixelSpacing {
                x,
                y: settled_y,
                ratio,
            };
        }

        let mut ratio = if x != 0.0 { y / x } else { 1.0 };
        if ratio == 0.0 || force_square_pixels {
            ratio = 1.0;
        }
        SettledPixelSpacing { x, y, ratio }
    }

    /// Signed distance of the slice origin along the slice normal (`dot(origin, normal)`).
    pub fn slice_location(origin: [f64; 3], orientation: &[f64]) -> Option<f64> {
        if orientation.len() < 9 {
            return None;
        }
        let normal = [orientation[6], orientation[7], orientation[8]];
        if normal[0] == 0.0 && normal[1] == 0.0 && normal[2] == 0.0 {
            return None;
        }
        let xy = adding_product(origin[0] * normal[0], origin[1], normal[1]);
        Some(adding_product(xy, origin[2], normal[2]))
    }
}

// ============================================================================
// ImagePlaneCoordinates (Modules/IsiXImaging/Sources/Geometry/ImagePlaneCoordinates.swift)
// ============================================================================

#[derive(Clone, Copy, Debug)]
pub struct SlicePlaneParams {
    pub origin: [f32; 3],
    pub orientation: [f32; 9],
    pub spacing_x: f64,
    pub spacing_y: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct OriginDeltaSlice {
    pub origin: [f32; 3],
    pub orientation: [f32; 9],
    pub spacing_x: f64,
    pub spacing_y: f64,
}

pub struct ImagePlaneCoordinates;

impl ImagePlaneCoordinates {
    /// Normalises the row and column vectors of a 6- or 9-element orientation and
    /// sets the third vector to their normalised cross product.
    pub fn normalized_orientation(values: &[f64]) -> [f64; 9] {
        assert!(values.len() >= 6);
        let row = normalize3_f64([values[0], values[1], values[2]]);
        let column = normalize3_f64([values[3], values[4], values[5]]);
        let normal = normalize3_f64(cross3_f64(row, column));
        [
            row[0], row[1], row[2], column[0], column[1], column[2], normal[0], normal[1],
            normal[2],
        ]
    }

    pub fn normalized_orientation_from_float(values: &[f32]) -> [f64; 9] {
        assert!(values.len() >= 6);
        let doubles = [
            values[0] as f64,
            values[1] as f64,
            values[2] as f64,
            values[3] as f64,
            values[4] as f64,
            values[5] as f64,
        ];
        Self::normalized_orientation(&doubles)
    }

    /// Whether all 9 orientation elements are within `0.001` (`< 0.001`).
    pub fn has_same_orientation(lhs: &[f64; 9], rhs: &[f64; 9]) -> bool {
        for i in 0..9 {
            if (lhs[i] - rhs[i]).abs() >= 0.001 {
                return false;
            }
        }
        true
    }

    /// True when the orientation differs from `DEFAULT_ORIENTATION` (`[1,0,0, 0,1,0, 0,0,1]`).
    pub fn is_3d_plane(orientation: &[f64; 9]) -> bool {
        orientation != &ImagePlaneGeometry::DEFAULT_ORIENTATION
    }

    /// Orientation after applying in-plane rotation (degrees) and horizontal/vertical flips.
    pub fn corrected_orientation<F>(
        orientation: &[f64; 9],
        rotation_degrees: f32,
        x_flipped: bool,
        y_flipped: bool,
        mut rotate: F,
    ) -> [f32; 9]
    where
        F: FnMut(Vector3, f64, Vector3) -> Vector3,
    {
        let mut o: [f32; 9] = [
            orientation[0] as f32,
            orientation[1] as f32,
            orientation[2] as f32,
            orientation[3] as f32,
            orientation[4] as f32,
            orientation[5] as f32,
            orientation[6] as f32,
            orientation[7] as f32,
            orientation[8] as f32,
        ];
        if rotation_degrees != 0.0 {
            let n = Vector3::new(-o[6] as f64, -o[7] as f64, -o[8] as f64);
            let rad = -(rotation_degrees as f64) * std::f64::consts::PI / 180.0;
            let x = rotate(Vector3::new(o[0] as f64, o[1] as f64, o[2] as f64), rad, n);
            let y = rotate(Vector3::new(o[3] as f64, o[4] as f64, o[5] as f64), rad, n);
            o[0] = x.x as f32;
            o[1] = x.y as f32;
            o[2] = x.z as f32;
            o[3] = y.x as f32;
            o[4] = y.y as f32;
            o[5] = y.z as f32;
        }
        if x_flipped {
            o[0] = -o[0];
            o[1] = -o[1];
            o[2] = -o[2];
        }
        if y_flipped {
            o[3] = -o[3];
            o[4] = -o[4];
            o[5] = -o[5];
        }
        o
    }

    /// Convenience wrapper using `PlaneGeometry::arbitrary_rotate`.
    pub fn corrected_orientation_standard(
        orientation: &[f64; 9],
        rotation_degrees: f32,
        x_flipped: bool,
        y_flipped: bool,
    ) -> [f32; 9] {
        Self::corrected_orientation(
            orientation,
            rotation_degrees,
            x_flipped,
            y_flipped,
            PlaneGeometry::arbitrary_rotate,
        )
    }

    /// Rotates `pt` around `around` by `angle_degrees` (degrees).
    pub fn rotate_point(pt: (f64, f64), around: (f64, f64), angle_degrees: f32) -> (f64, f64) {
        if angle_degrees == 0.0 {
            return pt;
        }
        let rad = -(angle_degrees as f64) * std::f64::consts::PI / 180.0;
        let x = pt.0 - around.0;
        let y = pt.1 - around.1;
        let c = rad.cos();
        let s = rad.sin();
        (
            around.0 + adding_product(-(y * s), x, c),
            around.1 + adding_product(x * s, y, c),
        )
    }

    /// Bounding rectangle `(origin_x, origin_y, width, height)` of the rotated/scaled image in pixel space.
    pub fn useful_rect(
        pixel_width: isize,
        pixel_height: isize,
        rotation_degrees: f32,
        scale: f32,
        x_flipped: bool,
        y_flipped: bool,
    ) -> (f64, f64, f64, f64) {
        let mut r = rotation_degrees;
        if x_flipped {
            r = -r;
        }
        if y_flipped {
            r = -r;
        }

        let w = pixel_width as f64;
        let h = pixel_height as f64;
        let s = scale as f64;
        let center = (w / 2.0, h / 2.0);
        let p1 = Self::rotate_point(
            ((w - w * s) / 2.0, (h - h * s) / 2.0),
            center,
            r,
        );
        let p2 = Self::rotate_point(
            (w - (w - w * s) / 2.0, (h - h * s) / 2.0),
            center,
            r,
        );
        let p3 = Self::rotate_point(
            (w - (w - w * s) / 2.0, h - (h - h * s) / 2.0),
            center,
            r,
        );
        let p4 = Self::rotate_point(
            ((w - w * s) / 2.0, h - (h - h * s) / 2.0),
            center,
            r,
        );

        let mut min_x = p1.0;
        let mut max_x = p1.0;
        let mut min_y = p1.1;
        let mut max_y = p1.1;
        for p in [p2, p3, p4] {
            if p.0 < min_x {
                min_x = p.0;
            }
            if p.0 > max_x {
                max_x = p.0;
            }
            if p.1 < min_y {
                min_y = p.1;
            }
            if p.1 > max_y {
                max_y = p.1;
            }
        }
        (min_x, min_y, max_x - min_x, max_y - min_y)
    }

    /// Double-precision pixel `(x, y)` to 3D DICOM patient coordinates `(X, Y, Z)` in mm.
    pub fn pixel_to_patient_double(
        x: f64,
        y: f64,
        origin: [f64; 3],
        orientation: &[f64; 9],
        spacing_x: f64,
        spacing_y: f64,
        pixel_center: bool,
    ) -> [f64; 3] {
        let mut px = x;
        let mut py = y;
        if pixel_center {
            px -= 0.5;
            py -= 0.5;
        }
        [
            adding_product(
                adding_product(origin[0], py * spacing_y, orientation[3]),
                px * spacing_x,
                orientation[0],
            ),
            adding_product(
                adding_product(origin[1], py * spacing_y, orientation[4]),
                px * spacing_x,
                orientation[1],
            ),
            adding_product(
                adding_product(origin[2], py * spacing_y, orientation[5]),
                px * spacing_x,
                orientation[2],
            ),
        ]
    }

    /// Float-precision pixel `(x, y)` to 3D DICOM patient coordinates `(X, Y, Z)` in mm.
    pub fn pixel_to_patient_float(
        x: f32,
        y: f32,
        origin: [f32; 3],
        orientation: &[f32; 9],
        spacing_x: f64,
        spacing_y: f64,
        pixel_center: bool,
    ) -> [f32; 3] {
        let mut px = x;
        let mut py = y;
        if pixel_center {
            px -= 0.5;
            py -= 0.5;
        }
        let px_d = px as f64;
        let py_d = py as f64;
        [
            adding_product(
                adding_product(origin[0] as f64, py_d * spacing_y, orientation[3] as f64),
                px_d * spacing_x,
                orientation[0] as f64,
            ) as f32,
            adding_product(
                adding_product(origin[1] as f64, py_d * spacing_y, orientation[4] as f64),
                px_d * spacing_x,
                orientation[1] as f64,
            ) as f32,
            adding_product(
                adding_product(origin[2] as f64, py_d * spacing_y, orientation[5] as f64),
                px_d * spacing_x,
                orientation[2] as f64,
            ) as f32,
        ]
    }

    /// Double-precision 3D DICOM patient coordinates to slice pixel coordinates `(x, y, z_distance_mm)`.
    pub fn patient_to_slice_double(
        location: [f64; 3],
        origin: [f64; 3],
        orientation: &[f64; 9],
        spacing_x: f64,
        spacing_y: f64,
        pixel_center: bool,
    ) -> [f64; 3] {
        let d = [
            location[0] - origin[0],
            location[1] - origin[1],
            location[2] - origin[2],
        ];
        let mut x = dot3_f64(d, [orientation[0], orientation[1], orientation[2]]) / spacing_x;
        let mut y = dot3_f64(d, [orientation[3], orientation[4], orientation[5]]) / spacing_y;
        let z = dot3_f64(d, [orientation[6], orientation[7], orientation[8]]);
        if pixel_center {
            x += 0.5;
            y += 0.5;
        }
        [x, y, z]
    }

    /// Float-precision 3D DICOM patient coordinates to slice pixel coordinates `(x, y, z_distance_mm)`.
    pub fn patient_to_slice_float(
        location: [f32; 3],
        origin: [f32; 3],
        orientation: &[f32; 9],
        spacing_x: f64,
        spacing_y: f64,
        pixel_center: bool,
    ) -> [f32; 3] {
        let d = [
            location[0] - origin[0],
            location[1] - origin[1],
            location[2] - origin[2],
        ];
        let mut x = ((dot3_f32(d, [orientation[0], orientation[1], orientation[2]]) as f64)
            / spacing_x) as f32;
        let mut y = ((dot3_f32(d, [orientation[3], orientation[4], orientation[5]]) as f64)
            / spacing_y) as f32;
        let z = dot3_f32(d, [orientation[6], orientation[7], orientation[8]]);
        if pixel_center {
            x += 0.5;
            y += 0.5;
        }
        [x, y, z]
    }

    pub fn slice_center_patient(
        pixel_width: isize,
        pixel_height: isize,
        origin: [f32; 3],
        orientation: &[f32; 9],
        spacing_x: f64,
        spacing_y: f64,
    ) -> [f32; 3] {
        Self::pixel_to_patient_float(
            ((pixel_width as f64) - 1.0) as f32 / 2.0,
            ((pixel_height as f64) - 1.0) as f32 / 2.0,
            origin,
            orientation,
            spacing_x,
            spacing_y,
            true,
        )
    }

    /// Finds the slice in `slices` whose plane is closest to `location`.
    pub fn nearest_slice(
        slices: &[SlicePlaneParams],
        location: [f32; 3],
    ) -> Option<(usize, [f32; 3])> {
        let mut best_distance = f64::MAX;
        let mut best: Option<(usize, [f32; 3])> = None;
        for (index, slice) in slices.iter().enumerate() {
            let coords = Self::patient_to_slice_float(
                location,
                slice.origin,
                &slice.orientation,
                slice.spacing_x,
                slice.spacing_y,
                true,
            );
            let distance = (coords[2] as f64).abs();
            if distance < best_distance {
                best_distance = distance;
                best = Some((index, coords));
            }
        }
        best
    }

    /// Shift in pixels of `second` from `first` along `first`'s row and column vectors.
    pub fn origin_delta(
        first: &OriginDeltaSlice,
        second: &OriginDeltaSlice,
    ) -> Option<(f64, f64)> {
        if first.spacing_x == 0.0
            || first.spacing_y == 0.0
            || second.spacing_x == 0.0
            || second.spacing_y == 0.0
        {
            return None;
        }
        let d = [
            second.origin[0] - first.origin[0],
            second.origin[1] - first.origin[1],
            second.origin[2] - first.origin[2],
        ];
        let x = dot3_f32(
            d,
            [
                first.orientation[0],
                first.orientation[1],
                first.orientation[2],
            ],
        ) / (first.spacing_x as f32);
        let y = dot3_f32(
            d,
            [
                first.orientation[3],
                first.orientation[4],
                first.orientation[5],
            ],
        ) / (first.spacing_y as f32);
        Some((x as f64, y as f64))
    }

    pub fn origin_in_slice_plane(origin: [f32; 3], orientation: &[f32; 9]) -> (f64, f64) {
        let x = dot3_f32(origin, [orientation[0], orientation[1], orientation[2]]);
        let y = dot3_f32(origin, [orientation[3], orientation[4], orientation[5]]);
        (x as f64, y as f64)
    }

    pub fn slice_extent_mm(
        origin: [f32; 3],
        orientation: &[f32; 9],
        pixel_width: isize,
        pixel_height: isize,
        spacing_x: f64,
        spacing_y: f64,
    ) -> (f64, f64, f64, f64) {
        let o = Self::origin_in_slice_plane(origin, orientation);
        (
            o.0,
            o.1,
            (pixel_width as f64) * spacing_x,
            (pixel_height as f64) * spacing_y,
        )
    }
}

// ============================================================================
// PlaneGeometry (Modules/IsiXImaging/Sources/ViewGeometry/PlaneGeometry.swift)
// ============================================================================

pub struct PlaneGeometry;

impl PlaneGeometry {
    /// Rotates a 3D point `p` by `theta` radians around `axis` (`ArbitraryRotate` in DCMView).
    pub fn arbitrary_rotate(p: Vector3, theta: f64, axis: Vector3) -> Vector3 {
        let mut r = axis;
        let length = r.length();
        if length != 0.0 {
            r.x /= length;
            r.y /= length;
            r.z /= length;
        }
        let costheta = theta.cos();
        let sintheta = theta.sin();
        let one_minus_cos = 1.0 - costheta;

        let mut q = Vector3::ZERO;
        let rx0 = adding_product(costheta, r.x * r.x, one_minus_cos);
        let rx1 = adding_product(-(r.z * sintheta), r.x * r.y, one_minus_cos);
        let rx2 = adding_product(r.y * sintheta, r.x * r.z, one_minus_cos);
        q.x = adding_product(adding_product(rx0 * p.x, rx1, p.y), rx2, p.z);

        let ry0 = adding_product(r.z * sintheta, r.y * r.x, one_minus_cos);
        let ry1 = adding_product(costheta, r.y * r.y, one_minus_cos);
        let ry2 = adding_product(-(r.x * sintheta), r.y * r.z, one_minus_cos);
        q.y = adding_product(adding_product(ry0 * p.x, ry1, p.y), ry2, p.z);

        let rz0 = adding_product(-(r.y * sintheta), r.z * r.x, one_minus_cos);
        let rz1 = adding_product(r.x * sintheta, r.z * r.y, one_minus_cos);
        let rz2 = adding_product(costheta, r.z * r.z, one_minus_cos);
        q.z = adding_product(adding_product(rz0 * p.x, rz1, p.y), rz2, p.z);

        q
    }

    /// Angle in degrees between the slice normals (`elements[6..9]`) of two orientations.
    pub fn plane_normal_difference_degrees(orientation_a: &[f32], orientation_b: &[f32]) -> f32 {
        assert!(orientation_a.len() >= 9 && orientation_b.len() >= 9);
        let va0 = -(orientation_a[6] as f64);
        let va1 = -(orientation_a[7] as f64);
        let va2 = -(orientation_a[8] as f64);
        let vb0 = -(orientation_b[6] as f64);
        let vb1 = -(orientation_b[7] as f64);
        let vb2 = -(orientation_b[8] as f64);
        let rad = ((va0 - vb0).hypot((va1 - vb1).hypot(va2 - vb2)) / 2.0).asin() * 2.0;
        let deg = rad * 180.0 / std::f64::consts::PI;
        deg.abs() as f32
    }

    /// Computes the intersection segment in `self`'s pixel space where `other`'s finite
    /// slice rectangle cuts across `self`'s infinite plane.
    ///
    /// Returns `None` if either slice lacks a 3D plane (`!self_is_3d || !other_is_3d`)
    /// or if the two planes do not intersect along `other`'s four edges.
    #[allow(clippy::too_many_arguments)]
    pub fn slice_intersection(
        self_origin: [f32; 3],
        self_orientation: &[f32; 9],
        self_spacing_x: f64,
        self_spacing_y: f64,
        self_is_3d: bool,
        other_origin: [f32; 3],
        other_orientation: &[f32; 9],
        other_spacing_x: f64,
        other_spacing_y: f64,
        other_width: isize,
        other_height: isize,
        other_is_3d: bool,
    ) -> Option<([f32; 3], [f32; 3])> {
        if !other_is_3d || !self_is_3d {
            return None;
        }

        let w = other_width as f32;
        let h = other_height as f32;

        // Four corners of `other` in patient coordinates, then projected into `self` slice coords.
        let corners_pix = [(0.0f32, 0.0f32), (w, 0.0), (w, h), (0.0, h)];
        let mut c = [[0.0f32; 3]; 4];
        for (i, &(px, py)) in corners_pix.iter().enumerate() {
            let pat = ImagePlaneCoordinates::pixel_to_patient_float(
                px,
                py,
                other_origin,
                other_orientation,
                other_spacing_x,
                other_spacing_y,
                true,
            );
            c[i] = ImagePlaneCoordinates::patient_to_slice_float(
                pat,
                self_origin,
                self_orientation,
                self_spacing_x,
                self_spacing_y,
                true,
            );
        }

        // Parametric line-plane intersection along edge (a -> b) where z == 0 in `self` slice coords.
        let edge_hit = |a: [f32; 3], b: [f32; 3]| -> Option<[f32; 3]> {
            let u = (0.0 - a[2]) / (b[2] - a[2]);
            if u > 0.0 && u < 1.0 {
                Some([
                    adding_product_f32(a[0], u, b[0] - a[0]),
                    adding_product_f32(a[1], u, b[1] - a[1]),
                    0.0,
                ])
            } else {
                None
            }
        };

        let mut points: Vec<[f32; 3]> = Vec::with_capacity(2);
        for (a, b) in [
            (c[0], c[1]),
            (c[1], c[2]),
            (c[2], c[3]),
            (c[0], c[3]),
        ] {
            if let Some(p) = edge_hit(a, b) {
                points.push(p);
            }
        }

        if points.len() == 2 {
            Some((points[0], points[1]))
        } else {
            None
        }
    }
}

// ============================================================================
// LineGeometry (Modules/IsiXImaging/Sources/ViewGeometry/LineGeometry.swift)
// ============================================================================

pub struct LineGeometry;

impl LineGeometry {
    /// Whether `p3` lies within `epsilon` of the axis-aligned bounding box of `(p1, p2)`.
    pub fn between(p1: (f64, f64), p2: (f64, f64), p3: (f64, f64), epsilon: f64) -> bool {
        if p1.0 - p2.0 >= epsilon || p2.0 - p1.0 >= epsilon {
            !((p1.0 < p2.0 && (p3.0 < p1.0 || p3.0 > p2.0))
                || (p1.0 > p2.0 && (p3.0 > p1.0 || p3.0 < p2.0)))
        } else {
            !((p1.1 < p2.1 && (p3.1 < p1.1 || p3.1 > p2.1))
                || (p1.1 > p2.1 && (p3.1 > p1.1 || p3.1 < p2.1)))
        }
    }

    /// Segment-segment intersection in 2D (`detect2DLineIntersection`).
    /// Returns `(hit_count, intersection_point)` where `hit_count` is `1` if the
    /// segments intersect and `0` otherwise.
    pub fn intersection(
        p1: (f64, f64),
        p2: (f64, f64),
        p3: (f64, f64),
        p4: (f64, f64),
        epsilon: f64,
    ) -> (i32, (f64, f64)) {
        let mut x: f64;
        
        let y: f64 = if (p1.0 - p2.0).abs() < epsilon {
            if (p3.0 - p4.0).abs() < epsilon {
                return (0, (0.0, 0.0));
            }
            let m2 = (p3.1 - p4.1) / (p3.0 - p4.0);
            let b2 = adding_product(p3.1, -m2, p3.0);
            x = p1.0;
            adding_product(b2, m2, x)
        } else if (p3.0 - p4.0).abs() < epsilon {
            let m1 = (p1.1 - p2.1) / (p1.0 - p2.0);
            let b1 = adding_product(p1.1, -m1, p1.0);
            x = p3.0;
            adding_product(b1, m1, x)
        } else {
            let m1 = (p1.1 - p2.1) / (p1.0 - p2.0);
            let b1 = adding_product(p1.1, -m1, p1.0);
            let m2 = (p3.1 - p4.1) / (p3.0 - p4.0);
            let b2 = adding_product(p3.1, -m2, p3.0);
            if (m1 - m2).abs() < epsilon {
                return (0, (0.0, 0.0));
            }
            x = (b2 - b1) / (m1 - m2);
            if (x - p1.0).abs() < epsilon {
                x = p1.0;
            }
            if (x - p2.0).abs() < epsilon {
                x = p2.0;
            }
            adding_product(b1, m1, x)
        };
        let cross = (x, y);
        let hits = Self::between(p1, p2, cross, epsilon) && Self::between(p3, p4, cross, epsilon);
        (if hits { 1 } else { 0 }, cross)
    }
}

// ============================================================================
// ViewTransform & ViewportState (Modules/IsiXImaging/Sources/ViewGeometry/*)
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewTransform {
    pub view_width: f64,
    pub view_height: f64,
    pub backing_scale: f64,
    pub origin_x: f64,
    pub origin_y: f64,
    pub origin_x_offset: f64,
    pub origin_y_offset: f64,
    pub scale_value: f32,
    pub rotation_degrees: f32,
    pub pixel_ratio: f64,
    pub pixel_width: f64,
    pub pixel_height: f64,
    pub x_flipped: bool,
    pub y_flipped: bool,
}

impl ViewTransform {
    pub const MIN_SCALE_VALUE: f32 = 0.02;

    pub fn view_to_pixel(&self, pt: (f64, f64)) -> (f64, f64) {
        let mut r = Point3D {
            x: pt.0 as f32,
            y: pt.1 as f32,
            z: 0.0,
        };

        r.x *= self.backing_scale as f32;
        r.y *= self.backing_scale as f32;

        r.x -= (adding_product(
            self.view_width * 0.5,
            self.origin_x,
            self.scale_value as f64,
        ) + self.origin_x_offset * (self.scale_value as f64)) as f32;
        r.y -= (adding_product(
            self.view_height * 0.5,
            -self.origin_y,
            self.scale_value as f64,
        ) - self.origin_y_offset * (self.scale_value as f64)) as f32;

        if self.x_flipped {
            r.x = -r.x;
        }
        if self.y_flipped {
            r.y = -r.y;
        }

        if self.rotation_degrees != 0.0 {
            let mut rad = -(self.rotation_degrees as f64) * std::f64::consts::PI / 180.0;
            if self.x_flipped != self.y_flipped {
                rad = -rad;
            }
            let temp_x = r.x as f64;
            let c = rad.cos();
            let s = rad.sin();
            r.x = adding_product(-(r.y as f64) * s, temp_x, c) as f32;
            r.y = adding_product(temp_x * s, r.y as f64, c) as f32;
        }

        if self.pixel_ratio != 1.0 {
            r.y /= self.pixel_ratio as f32;
        }

        r.x = adding_product(
            (r.x / self.scale_value) as f64,
            0.5,
            self.pixel_width,
        ) as f32;
        r.y = adding_product(
            (r.y / self.scale_value) as f64,
            0.5,
            self.pixel_height,
        ) as f32;

        (r.x as f64, r.y as f64)
    }

    pub fn pixel_to_view(&self, pt: (f64, f64)) -> (f64, f64) {
        let mut r = Point3D {
            x: pt.0 as f32,
            y: pt.1 as f32,
            z: 0.0,
        };

        r.x = adding_product(r.x as f64, -0.5, self.pixel_width) as f32 * self.scale_value;
        r.y = adding_product(r.y as f64, -0.5, self.pixel_height) as f32 * self.scale_value;

        if self.pixel_ratio != 1.0 {
            r.y *= self.pixel_ratio as f32;
        }

        if self.rotation_degrees != 0.0 {
            let mut rad = (self.rotation_degrees as f64) * std::f64::consts::PI / 180.0;
            if self.x_flipped != self.y_flipped {
                rad = -rad;
            }
            let temp_x = r.x as f64;
            let c = rad.cos();
            let s = rad.sin();
            r.x = adding_product(-(r.y as f64) * s, temp_x, c) as f32;
            r.y = adding_product(temp_x * s, r.y as f64, c) as f32;
        }

        if self.x_flipped {
            r.x = -r.x;
        }
        if self.y_flipped {
            r.y = -r.y;
        }

        r.x += (adding_product(
            self.view_width * 0.5,
            self.origin_x,
            self.scale_value as f64,
        ) + self.origin_x_offset * (self.scale_value as f64)) as f32;
        r.y += (adding_product(
            self.view_height * 0.5,
            -self.origin_y,
            self.scale_value as f64,
        ) - self.origin_y_offset * (self.scale_value as f64)) as f32;

        r.x /= self.backing_scale as f32;
        r.y /= self.backing_scale as f32;

        (r.x as f64, r.y as f64)
    }

    pub fn screen_length_in_pixels(&self, screen_length: f64) -> f64 {
        let a = self.view_to_pixel((0.0, 0.0));
        let b = self.view_to_pixel((0.0, screen_length));
        (b.1 - a.1).hypot(b.0 - a.0)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn fit_scale_value(
        backing_width: f64,
        backing_height: f64,
        pixel_width: isize,
        pixel_height: isize,
        rotation_degrees: f32,
        pixel_ratio: f64,
        x_flipped: bool,
        y_flipped: bool,
    ) -> f32 {
        let (_, _, useful_w, useful_h) = ImagePlaneCoordinates::useful_rect(
            pixel_width,
            pixel_height,
            rotation_degrees,
            1.0,
            x_flipped,
            y_flipped,
        );
        let x_scale = (backing_width as f32) / (useful_w as f32);
        let y_scale = (backing_height as f32) / ((useful_h as f32) * (pixel_ratio as f32));
        x_scale.min(y_scale)
    }

    pub fn clamped_scale_value(proposed: f32) -> f32 {
        if proposed < Self::MIN_SCALE_VALUE {
            Self::MIN_SCALE_VALUE
        } else {
            proposed
        }
    }

    pub fn normalize_degrees(degrees: f32) -> f32 {
        let mut r = degrees % 360.0;
        if r < 0.0 {
            r += 360.0;
        }
        r
    }
}

#[derive(Clone, Copy, Debug)]
struct Point3D {
    x: f32,
    y: f32,
    z: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewportState {
    pub origin_x: f64,
    pub origin_y: f64,
    pub origin_x_offset: f64,
    pub origin_y_offset: f64,
    pub scale_value: f32,
    pub rotation: f32,
    pub x_flipped: bool,
    pub y_flipped: bool,
}

impl Default for ViewportState {
    fn default() -> Self {
        Self {
            origin_x: 0.0,
            origin_y: 0.0,
            origin_x_offset: 0.0,
            origin_y_offset: 0.0,
            scale_value: 1.0,
            rotation: 0.0,
            x_flipped: false,
            y_flipped: false,
        }
    }
}

impl ViewportState {
    pub fn set_origin(&mut self, x: f64, y: f64) {
        self.origin_x = x;
        self.origin_y = y;
    }

    pub fn set_origin_offset(&mut self, x: f64, y: f64) {
        self.origin_x_offset = x;
        self.origin_y_offset = y;
    }

    pub fn zero_origin(&mut self) {
        self.origin_x = 0.0;
        self.origin_y = 0.0;
        self.origin_x_offset = 0.0;
        self.origin_y_offset = 0.0;
    }

    pub fn scale_by_ratio(&mut self, old_width: f64, new_width: f64) {
        self.scale_value =
            ((self.scale_value as f64) * new_width / old_width) as f32;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn set_scale_to_fit(
        &mut self,
        backing_width: f64,
        backing_height: f64,
        pixel_width: isize,
        pixel_height: isize,
        pixel_ratio: f64,
    ) {
        self.scale_value = ViewTransform::fit_scale_value(
            backing_width,
            backing_height,
            pixel_width,
            pixel_height,
            self.rotation,
            pixel_ratio,
            self.x_flipped,
            self.y_flipped,
        );
        self.zero_origin();
    }

    pub fn apply_scale_value(&mut self, proposed: f32, has_pixels: bool) -> bool {
        if !has_pixels {
            self.scale_value = proposed;
            return false;
        }
        self.scale_value = ViewTransform::clamped_scale_value(proposed);
        true
    }

    pub fn rotate(&mut self, degrees: f32) {
        self.rotation = ViewTransform::normalize_degrees(degrees);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settled_pixel_spacing_handles_implausible_and_aspect_fallback() {
        // Normal isotropic spacing
        let s = ImagePlaneGeometry::settled_pixel_spacing(0.7, 0.7, 512, 512, false);
        assert_eq!(s.x, 0.7);
        assert_eq!(s.y, 0.7);
        assert_eq!(s.ratio, 1.0);

        // Implausible > 1000 in both
        let bad = ImagePlaneGeometry::settled_pixel_spacing(1500.0, 2000.0, 512, 512, false);
        assert_eq!(bad.x, 0.0);
        assert_eq!(bad.y, 0.0);
        assert_eq!(bad.ratio, 1.0);

        // Extreme aspect ratio (>= 4x) on matrix <= 1024 falls back to columns/rows
        let fb = ImagePlaneGeometry::settled_pixel_spacing(0.5, 2.5, 256, 512, false);
        assert_eq!(fb.x, 0.5);
        assert_eq!(fb.ratio, 2.0);
        assert_eq!(fb.y, 1.0);
    }

    #[test]
    fn orientation_normalizes_and_computes_normal() {
        let o = ImagePlaneCoordinates::normalized_orientation(&[2.0, 0.0, 0.0, 0.0, 3.0, 0.0]);
        assert_eq!(o, [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
        assert!(!ImagePlaneCoordinates::is_3d_plane(&o));

        let coronal =
            ImagePlaneCoordinates::normalized_orientation(&[1.0, 0.0, 0.0, 0.0, 0.0, -1.0]);
        assert_eq!(coronal, [1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 1.0, 0.0]);
        assert!(ImagePlaneCoordinates::is_3d_plane(&coronal));

        let degenerate =
            ImagePlaneCoordinates::normalized_orientation(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        assert_eq!(degenerate, [0.0; 9]);
    }

    #[test]
    fn same_orientation_uses_one_thousandth_tolerance() {
        let a = ImagePlaneCoordinates::normalized_orientation(&[1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
        let mut close = a;
        close[1] += 0.0009;
        let mut far = a;
        far[1] += 0.001;
        assert!(ImagePlaneCoordinates::has_same_orientation(&a, &close));
        assert!(!ImagePlaneCoordinates::has_same_orientation(&a, &far));
    }

    #[test]
    fn pixel_and_patient_round_trip() {
        let origin = [-120.25, 33.125, 17.5];
        let orientation = ImagePlaneCoordinates::normalized_orientation(&[
            0.8660254037844386,
            0.5,
            0.0,
            -0.492403876506104,
            0.8528685319524433,
            0.17364817766693033,
        ]);
        for center in [true, false] {
            let patient = ImagePlaneCoordinates::pixel_to_patient_double(
                19.25,
                42.75,
                origin,
                &orientation,
                0.7,
                0.9,
                center,
            );
            let slice = ImagePlaneCoordinates::patient_to_slice_double(
                patient,
                origin,
                &orientation,
                0.7,
                0.9,
                center,
            );
            assert!((slice[0] - 19.25).abs() < 1e-6);
            assert!((slice[1] - 42.75).abs() < 1e-6);
            assert!(slice[2].abs() < 1e-6);
        }
    }

    #[test]
    fn nearest_slice_chooses_smallest_normal_distance() {
        let o: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        let slices = vec![
            SlicePlaneParams {
                origin: [0.0, 0.0, 0.0],
                orientation: o,
                spacing_x: 1.0,
                spacing_y: 1.0,
            },
            SlicePlaneParams {
                origin: [0.0, 0.0, 2.5],
                orientation: o,
                spacing_x: 1.0,
                spacing_y: 1.0,
            },
            SlicePlaneParams {
                origin: [0.0, 0.0, 5.0],
                orientation: o,
                spacing_x: 1.0,
                spacing_y: 1.0,
            },
        ];
        let hit = ImagePlaneCoordinates::nearest_slice(&slices, [3.0, 4.0, 2.1]).unwrap();
        assert_eq!(hit.0, 1);
        assert_eq!(hit.1[0], 3.5);
        assert_eq!(hit.1[1], 4.5);
        assert!((hit.1[2] - (-0.4)).abs() < 1e-6);
    }

    #[test]
    fn origin_delta_returns_none_without_spacing() {
        let o: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        let a = OriginDeltaSlice {
            origin: [0.0, 0.0, 0.0],
            orientation: o,
            spacing_x: 0.5,
            spacing_y: 2.0,
        };
        let b = OriginDeltaSlice {
            origin: [3.0, -6.0, 9.0],
            orientation: o,
            spacing_x: 0.5,
            spacing_y: 2.0,
        };
        assert_eq!(ImagePlaneCoordinates::origin_delta(&a, &b), Some((6.0, -3.0)));

        let no_spacing = OriginDeltaSlice {
            origin: [0.0, 0.0, 0.0],
            orientation: o,
            spacing_x: 0.0,
            spacing_y: 2.0,
        };
        assert_eq!(ImagePlaneCoordinates::origin_delta(&no_spacing, &b), None);
    }

    #[test]
    fn slice_intersection_orthogonal_planes() {
        let axial_o: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        let coronal_o: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 1.0, 0.0];

        // Axial slice at z = 0, 100x100, spacing 1mm, origin (0, 0, 0).
        // Coronal slice at y = 50, 100x100, spacing 1mm, origin (0, 50, 50) (since col is -Z, z spans 50..-50).
        let seg = PlaneGeometry::slice_intersection(
            [0.0, 0.0, 0.0],
            &axial_o,
            1.0,
            1.0,
            true,
            [0.0, 50.0, 50.0],
            &coronal_o,
            1.0,
            1.0,
            100,
            100,
            true,
        );
        assert!(seg.is_some());
        let (p0, p1) = seg.unwrap();
        // Should cut across y ~= 50.5 in axial pixel coords from x=0 to x=100.
        assert!((p0[1] - 50.5).abs() < 1e-4);
        assert!((p1[1] - 50.5).abs() < 1e-4);
        assert!((p0[0] - p1[0]).abs() > 99.0);
    }

    #[test]
    fn line_geometry_intersection() {
        let (hits, pt) = LineGeometry::intersection(
            (0.0, 0.0),
            (10.0, 10.0),
            (0.0, 10.0),
            (10.0, 0.0),
            1e-5,
        );
        assert_eq!(hits, 1);
        assert!((pt.0 - 5.0).abs() < 1e-6);
        assert!((pt.1 - 5.0).abs() < 1e-6);

        let (miss, _) = LineGeometry::intersection(
            (0.0, 0.0),
            (2.0, 2.0),
            (0.0, 10.0),
            (2.0, 8.0),
            1e-5,
        );
        assert_eq!(miss, 0);
    }

    #[test]
    fn view_transform_round_trips_view_and_pixel() {
        let vt = ViewTransform {
            view_width: 800.0,
            view_height: 600.0,
            backing_scale: 2.0,
            origin_x: 12.5,
            origin_y: -7.25,
            origin_x_offset: 3.0,
            origin_y_offset: -1.5,
            scale_value: 1.75,
            rotation_degrees: 30.0,
            pixel_ratio: 1.25,
            pixel_width: 512.0,
            pixel_height: 512.0,
            x_flipped: true,
            y_flipped: false,
        };
        let pix = vt.view_to_pixel((210.0, 145.0));
        let view = vt.pixel_to_view(pix);
        assert!((view.0 - 210.0).abs() < 1e-3);
        assert!((view.1 - 145.0).abs() < 1e-3);
    }
}
