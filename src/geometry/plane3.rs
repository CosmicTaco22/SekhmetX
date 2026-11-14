// Ported from IsiX: Modules/IsiXGeometry/Sources/Plane3.swift

use super::line3::Line3;
use super::transform3d::Transform3D;
use super::vector3::{adding_product, Vector3, GEOMETRY_EPSILON};

/// Three-dimensional plane given by a point and a normal, ported from IsiX `Plane3`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Plane3 {
    pub point: Vector3,
    pub normal: Vector3,
}

impl Plane3 {
    pub const INVALID: Self = Self {
        point: Vector3::ZERO,
        normal: Vector3::ZERO,
    };

    pub const X_ZERO: Self = Self {
        point: Vector3::ZERO,
        normal: Vector3::new(1.0, 0.0, 0.0),
    };

    pub const Y_ZERO: Self = Self {
        point: Vector3::ZERO,
        normal: Vector3::new(0.0, 1.0, 0.0),
    };

    pub const Z_ZERO: Self = Self {
        point: Vector3::ZERO,
        normal: Vector3::new(0.0, 0.0, 1.0),
    };

    #[inline]
    pub fn new(point: Vector3, normal: Vector3) -> Self {
        Self {
            point,
            normal: normal.normalized(),
        }
    }

    /// Raw constructor without normalizing `normal` (used in `least_squares` and `bounding_planes` where IsiX assigns directly).
    #[inline]
    pub const fn raw(point: Vector3, normal: Vector3) -> Self {
        Self { point, normal }
    }

    #[inline]
    pub fn is_coincident(self, to: Self) -> bool {
        if !self.point.is_on_plane(to) {
            return false;
        }
        (self.normal.normalized().dot_product(to.normal.normalized()).abs() - 1.0).abs()
            < GEOMETRY_EPSILON
    }

    #[inline]
    pub fn is_valid(self) -> bool {
        !self.normal.is_zero()
    }

    #[inline]
    pub fn closest_point(self, to: Vector3) -> Vector3 {
        let plane_normal = self.normal.normalized();
        to + plane_normal * (self.point - to).dot_product(plane_normal)
    }

    #[inline]
    pub fn is_parallel(self, to: Self) -> bool {
        (1.0 - self.normal.normalized().dot_product(to.normal.normalized()).abs())
            < GEOMETRY_EPSILON
    }

    #[inline]
    pub fn is_between(self, vector1: Vector3, vector2: Vector3) -> bool {
        (vector1 - self.point).dot_product(self.normal)
            * (vector2 - self.point).dot_product(self.normal)
            < 0.0
    }

    pub fn intersection(self, with: Self) -> Line3 {
        if self.is_parallel(with) {
            return Line3::INVALID;
        }
        let plane1_normal = self.normal.normalized();
        let plane2_normal = with.normal.normalized();
        let line_direction = plane1_normal.cross_product(plane2_normal).normalized();

        let p1 = self.point.dot_product(plane1_normal);
        let p2 = with.point.dot_product(plane2_normal);
        let n1n2 = plane1_normal.dot_product(plane2_normal);

        let inv_det = 1.0 - n1n2 * n1n2;
        let c1 = adding_product(-p2 * n1n2, p1, 1.0) / inv_det;
        let c2 = adding_product(-p1 * n1n2, p2, 1.0) / inv_det;

        Line3::new(plane1_normal * c1 + plane2_normal * c2, line_direction)
    }

    pub fn applying(self, transform: Transform3D) -> Self {
        let normal_transform = transform.inverted().transposed();
        Self::new(
            self.point.applying(transform),
            self.normal.applying_to_direction(normal_transform).normalized(),
        )
    }

    /// Fits `z = a*x + b*y + c` by least squares to `points`. Needs at least 4 points.
    pub fn least_squares(points: &[Vector3]) -> Self {
        if points.len() <= 3 {
            return Self::INVALID;
        }

        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let mut sum_z = 0.0;
        let mut sum_x2 = 0.0;
        let mut sum_y2 = 0.0;
        let mut sum_xy = 0.0;
        let mut sum_xz = 0.0;
        let mut sum_yz = 0.0;

        for point in points {
            sum_x += point.x;
            sum_y += point.y;
            sum_z += point.z;
            sum_x2 = adding_product(sum_x2, point.x, point.x);
            sum_y2 = adding_product(sum_y2, point.y, point.y);
            sum_xy = adding_product(sum_xy, point.x, point.y);
            sum_xz = adding_product(sum_xz, point.x, point.z);
            sum_yz = adding_product(sum_yz, point.y, point.z);
        }

        let count = points.len() as f64;
        let mean_x = sum_x / count;
        let mean_y = sum_y / count;
        let mean_z = sum_z / count;

        let sxx = adding_product(sum_x2, -count * mean_x, mean_x);
        let syy = adding_product(sum_y2, -count * mean_y, mean_y);
        let sxy = adding_product(sum_xy, -count * mean_x, mean_y);
        let sxz = adding_product(sum_xz, -count * mean_x, mean_z);
        let syz = adding_product(sum_yz, -count * mean_y, mean_z);

        let a = adding_product(-sxy * syz, sxz, syy) / adding_product(-sxy * sxy, sxx, syy);
        let b = adding_product(-sxy * sxz, syz, sxx) / adding_product(-sxy * sxy, sxx, syy);
        let c = adding_product(adding_product(mean_z, -a, mean_x), -b, mean_y);

        let point1 = Vector3::new(0.0, 0.0, c);
        let point2 = Vector3::new(1.0, 0.0, a + c);
        let point3 = Vector3::new(0.0, 1.0, b + c);
        let mut plane = Self::new(point1, (point2 - point1).cross_product(point3 - point1));
        plane.point = Vector3::new(mean_x, mean_y, mean_z);
        plane
    }
}
