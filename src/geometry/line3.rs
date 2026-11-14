// Ported from IsiX: Modules/IsiXGeometry/Sources/Line3.swift

use super::plane3::Plane3;
use super::transform3d::Transform3D;
use super::vector3::{adding_product, Vector3, GEOMETRY_EPSILON};

/// Closest points between two 3D lines, and the distance between them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineClosestPoints {
    pub distance: f64,
    pub on_self: Option<Vector3>,
    pub on_line: Option<Vector3>,
}

/// Three-dimensional line given by a point and a direction, ported from IsiX `Line3`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Line3 {
    pub point: Vector3,
    pub direction: Vector3,
}

impl Line3 {
    pub const INVALID: Self = Self {
        point: Vector3::ZERO,
        direction: Vector3::ZERO,
    };

    pub const X_AXIS: Self = Self {
        point: Vector3::ZERO,
        direction: Vector3::new(1.0, 0.0, 0.0),
    };

    pub const Y_AXIS: Self = Self {
        point: Vector3::ZERO,
        direction: Vector3::new(0.0, 1.0, 0.0),
    };

    pub const Z_AXIS: Self = Self {
        point: Vector3::ZERO,
        direction: Vector3::new(0.0, 0.0, 1.0),
    };

    #[inline]
    pub fn new(point: Vector3, direction: Vector3) -> Self {
        Self {
            point,
            direction: direction.normalized(),
        }
    }

    #[inline]
    pub fn from_points(from: Vector3, to: Vector3) -> Self {
        Self {
            point: from,
            direction: (to - from).normalized(),
        }
    }

    /// Raw constructor without normalizing `direction` (used where IsiX sets fields directly).
    #[inline]
    pub const fn raw(point: Vector3, direction: Vector3) -> Self {
        Self { point, direction }
    }

    #[inline]
    pub fn is_coincident(self, to: Self) -> bool {
        self.point.is_on_line(to) && self.is_parallel(to)
    }

    #[inline]
    pub fn is_on_plane(self, plane: Plane3) -> bool {
        if !self.point.is_on_plane(plane) {
            return false;
        }
        self.direction.normalized().dot_product(plane.normal.normalized()).abs() < GEOMETRY_EPSILON
    }

    #[inline]
    pub fn is_parallel(self, to: Self) -> bool {
        let self_dir = self.direction.normalized();
        let line_dir = to.direction.normalized();
        (1.0 - self_dir.dot_product(line_dir).abs()) < GEOMETRY_EPSILON
    }

    #[inline]
    pub fn is_valid(self) -> bool {
        !self.direction.is_zero()
    }

    #[inline]
    pub fn intersects(self, plane: Plane3) -> bool {
        debug_assert!(self.is_valid() && plane.is_valid());
        let line_dir = self.direction.normalized();
        let plane_normal = plane.normal.normalized();
        if self.point.is_on_plane(plane) {
            return true;
        }
        line_dir.dot_product(plane_normal).abs() > GEOMETRY_EPSILON
    }

    /// Where the line crosses the plane. When the line is parallel to the plane,
    /// returns the line's point if it lies on the plane, or a very far point otherwise.
    pub fn intersection(self, with: Plane3) -> Vector3 {
        debug_assert!(self.is_valid() && with.is_valid());
        let line_dir = self.direction.normalized();
        let plane_normal = with.normal.normalized();
        let dot = line_dir.dot_product(plane_normal);
        let p_minus_o = with.point - self.point;
        if dot.abs() <= GEOMETRY_EPSILON {
            if self.point.is_on_plane(with) {
                self.point
            } else {
                Vector3::repeating(f64::MAX)
            }
        } else {
            self.point + line_dir * (p_minus_o.dot_product(plane_normal) / dot)
        }
    }

    #[inline]
    pub fn closest_point(self, to: Vector3) -> Vector3 {
        self.point + (to - self.point).projected(self.direction)
    }

    pub fn applying(self, transform: Transform3D) -> Self {
        Self::new(
            self.point.applying(transform),
            self.direction.applying_to_direction(transform).normalized(),
        )
    }

    /// Closest points on `self` and `line`, and the distance between them.
    /// Parallel lines have no unique closest pair (`None`), only a distance.
    pub fn closest_points(self, to: Self) -> LineClosestPoints {
        if self.is_parallel(to) {
            return LineClosestPoints {
                distance: (to.point - self.closest_point(to.point)).length(),
                on_self: None,
                on_line: None,
            };
        }

        let p1 = self.point;
        let p2 = self.point + self.direction;
        let p3 = to.point;
        let p4 = to.point + to.direction;

        let p13 = p1 - p3;
        let p43 = p4 - p3;
        let p21 = p2 - p1;

        let d1343 = adding_product(
            adding_product(p13.x * p43.x, p13.y, p43.y),
            p13.z,
            p43.z,
        );
        let d4321 = adding_product(
            adding_product(p43.x * p21.x, p43.y, p21.y),
            p43.z,
            p21.z,
        );
        let d1321 = adding_product(
            adding_product(p13.x * p21.x, p13.y, p21.y),
            p13.z,
            p21.z,
        );
        let d4343 = adding_product(
            adding_product(p43.x * p43.x, p43.y, p43.y),
            p43.z,
            p43.z,
        );
        let d2121 = adding_product(
            adding_product(p21.x * p21.x, p21.y, p21.y),
            p21.z,
            p21.z,
        );

        let denom = adding_product(-d4321 * d4321, d2121, d4343);
        debug_assert!(denom.abs() >= GEOMETRY_EPSILON);
        let numer = adding_product(-d1321 * d4343, d1343, d4321);

        let mua = numer / denom;
        let mub = adding_product(d1343, d4321, mua) / d4343;

        let pa = p1.adding_product(mua, p21);
        let pb = p3.adding_product(mub, p43);
        LineClosestPoints {
            distance: pa.distance(pb),
            on_self: Some(pa),
            on_line: Some(pb),
        }
    }
}
