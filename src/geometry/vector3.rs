// Ported from IsiX:
// - Modules/IsiXGeometry/Sources/Vector3.swift
// - Modules/IsiXGeometry/Sources/PlanarGeometry.swift
// - Modules/IsiXGeometry/Sources/PackedVectors.swift

use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

use super::line3::Line3;
use super::plane3::Plane3;
use super::transform3d::Transform3D;

/// Tolerance for geometric coincidence and orthogonality checks (`1e-6`).
pub const GEOMETRY_EPSILON: f64 = 1e-6;

/// Helper matching Swift's `Double.addingProduct(a, b)` -> `self + a * b` fused.
#[inline(always)]
pub fn adding_product(acc: f64, a: f64, b: f64) -> f64 {
    a.mul_add(b, acc)
}

/// Three-dimensional double-precision vector, ported from IsiX `Vector3`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct Vector3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vector3 {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    #[inline]
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    #[inline]
    pub const fn repeating(value: f64) -> Self {
        Self {
            x: value,
            y: value,
            z: value,
        }
    }

    /// Component-wise fused `self + a * b`.
    #[inline]
    pub fn adding_product_vec(self, a: Self, b: Self) -> Self {
        Self::new(
            a.x.mul_add(b.x, self.x),
            a.y.mul_add(b.y, self.y),
            a.z.mul_add(b.z, self.z),
        )
    }

    /// Component-wise fused `self + scalar * vector`.
    #[inline]
    pub fn adding_product(self, scalar: f64, vector: Self) -> Self {
        Self::new(
            scalar.mul_add(vector.x, self.x),
            scalar.mul_add(vector.y, self.y),
            scalar.mul_add(vector.z, self.z),
        )
    }

    #[inline]
    pub fn is_coincident(self, to: Self) -> bool {
        self.distance(to) < GEOMETRY_EPSILON
    }

    #[inline]
    pub fn is_zero(self) -> bool {
        self == Self::ZERO
    }

    #[inline]
    pub fn distance(self, to: Self) -> f64 {
        (self - to).length()
    }

    #[inline]
    pub fn dot_product(self, vector: Self) -> f64 {
        let xy = self.x.mul_add(vector.x, self.y * vector.y);
        self.z.mul_add(vector.z, xy)
    }

    #[inline]
    pub fn cross_product(self, vector: Self) -> Self {
        Self::new(
            self.y.mul_add(vector.z, -self.z * vector.y),
            self.z.mul_add(vector.x, -self.x * vector.z),
            self.x.mul_add(vector.y, -self.y * vector.x),
        )
    }

    #[inline]
    pub fn interpolated(self, to: Self, at: f64) -> Self {
        let one_minus = 1.0 - at;
        Self::new(
            one_minus.mul_add(self.x, at * to.x),
            one_minus.mul_add(self.y, at * to.y),
            one_minus.mul_add(self.z, at * to.z),
        )
    }

    #[inline]
    pub fn length(self) -> f64 {
        self.dot_product(self).sqrt()
    }

    #[inline]
    pub fn normalized(self) -> Self {
        let len = self.length();
        if len != 0.0 {
            self * (1.0 / len)
        } else {
            Self::ZERO
        }
    }

    /// The component of `self` along `direction` when `direction` is a unit vector;
    /// in general `direction * (dot(self, direction) / |direction|)`.
    #[inline]
    pub fn projected(self, onto: Self) -> Self {
        onto * (self.dot_product(onto) / onto.length())
    }

    /// `0.0 - self` so zero components stay `+0.0`.
    #[inline]
    pub fn inverted(self) -> Self {
        Self::new(0.0 - self.x, 0.0 - self.y, 0.0 - self.z)
    }

    /// A unit vector normal to this one; zero for the zero vector.
    pub fn any_normal(self) -> Self {
        if self.is_zero() {
            return Self::ZERO;
        }
        if self.x == 0.0 {
            if self.y == 0.0 {
                Self::new(0.0, 1.0, 0.0)
            } else {
                Self::new(0.0, -self.z, self.y).normalized()
            }
        } else {
            Self::new(-self.y, self.x, 0.0).normalized()
        }
    }

    /// The angle in `[0, 2*pi)` from this vector to `vector`, around `around`.
    pub fn angle(self, to: Self, around: Self) -> f64 {
        let axis = around.normalized();
        let vector1 = (self.projected(axis) - self).normalized();
        let vector2 = (to.projected(axis) - to).normalized();
        let cross_product = vector1.cross_product(vector2);
        let len = cross_product.length();
        let min_len = if len < 1.0 { len } else { 1.0 };
        let mut angle = min_len.asin();
        if vector1.dot_product(vector2) < 0.0 {
            angle = std::f64::consts::PI - angle;
        }
        if cross_product.dot_product(axis) < 0.0 {
            angle = std::f64::consts::PI * 2.0 - angle;
        }
        angle
    }

    /// `self` turned by the rotation that takes `from` (`originalDirection`) to `to` (`newDirection`).
    pub fn bent(self, from: Self, to: Self) -> Self {
        let rotation_axis = from.normalized().cross_product(to.normalized());
        let len = rotation_axis.length();
        let min_len = if len < 1.0 { len } else { 1.0 };
        let mut angle = min_len.asin();
        if from.dot_product(to) < 0.0 {
            angle = std::f64::consts::PI - angle;
        }
        let rotation = Transform3D::make_rotation(
            angle,
            rotation_axis.x,
            rotation_axis.y,
            rotation_axis.z,
        );
        self.applying(rotation)
    }

    #[inline]
    pub fn is_on_line(self, line: Line3) -> bool {
        self.distance_to_line(line) < GEOMETRY_EPSILON
    }

    #[inline]
    pub fn is_on_plane(self, plane: Plane3) -> bool {
        self.distance_to_plane(plane) < GEOMETRY_EPSILON
    }

    #[inline]
    pub fn distance_to_line(self, line: Line3) -> f64 {
        self.distance(line.closest_point(self))
    }

    #[inline]
    pub fn distance_to_plane(self, plane: Plane3) -> f64 {
        (self - plane.point)
            .dot_product(plane.normal.normalized())
            .abs()
    }

    /// Applies an affine 3D transform to this point (`w = 1`).
    #[inline]
    pub fn applying(self, t: Transform3D) -> Self {
        let col1 = Self::new(t.m11, t.m12, t.m13);
        let col2 = Self::new(t.m21, t.m22, t.m23);
        let col3 = Self::new(t.m31, t.m32, t.m33);
        let col4 = Self::new(t.m41, t.m42, t.m43);
        col4.adding_product(self.x, col1)
            .adding_product(self.y, col2)
            .adding_product(self.z, col3)
    }

    /// Applies the linear 3x3 part of an affine 3D transform to a direction (`w = 0`).
    #[inline]
    pub fn applying_to_direction(self, t: Transform3D) -> Self {
        let col1 = Self::new(t.m11, t.m12, t.m13);
        let col2 = Self::new(t.m21, t.m22, t.m23);
        let col3 = Self::new(t.m31, t.m32, t.m33);
        (col1 * self.x)
            .adding_product(self.y, col2)
            .adding_product(self.z, col3)
    }

    /// PlanarGeometry: `Vector3(point.x, point.y, 0.0)`.
    #[inline]
    pub const fn from_point_2d(x: f64, y: f64) -> Self {
        Self::new(x, y, 0.0)
    }

    /// PlanarGeometry: `(self.x, self.y)`.
    #[inline]
    pub const fn point_in_plane(self) -> (f64, f64) {
        (self.x, self.y)
    }
}

impl Add for Vector3 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl AddAssign for Vector3 {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
        self.z += rhs.z;
    }
}

impl Sub for Vector3 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl SubAssign for Vector3 {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        self.x -= rhs.x;
        self.y -= rhs.y;
        self.z -= rhs.z;
    }
}

impl Mul<f64> for Vector3 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: f64) -> Self {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

impl Mul<Vector3> for f64 {
    type Output = Vector3;
    #[inline]
    fn mul(self, rhs: Vector3) -> Vector3 {
        rhs * self
    }
}

impl Mul<Vector3> for Vector3 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: Self) -> Self {
        Self::new(self.x * rhs.x, self.y * rhs.y, self.z * rhs.z)
    }
}

impl Div<f64> for Vector3 {
    type Output = Self;
    #[inline]
    fn div(self, rhs: f64) -> Self {
        Self::new(self.x / rhs, self.y / rhs, self.z / rhs)
    }
}

impl Neg for Vector3 {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y, -self.z)
    }
}

/// Operations on packed `(x, y, z)` `f64` buffers, ported from `PackedVectors.swift`.
pub struct PackedVectors;

impl PackedVectors {
    #[inline]
    fn read_at(scalars: &[f64], index: usize) -> Vector3 {
        let base = index * 3;
        Vector3::new(scalars[base], scalars[base + 1], scalars[base + 2])
    }

    #[inline]
    fn write_at(scalars: &mut [f64], index: usize, vector: Vector3) {
        let base = index * 3;
        scalars[base] = vector.x;
        scalars[base + 1] = vector.y;
        scalars[base + 2] = vector.z;
    }

    pub fn scale(vectors: &mut [f64], count: usize, by: f64) {
        for i in 0..count {
            let v = Self::read_at(vectors, i) * by;
            Self::write_at(vectors, i, v);
        }
    }

    pub fn add(other_vectors: &[f64], to: &mut [f64], count: usize) {
        for i in 0..count {
            let v = Self::read_at(to, i) + Self::read_at(other_vectors, i);
            Self::write_at(to, i, v);
        }
    }

    pub fn apply(transform: Transform3D, to: &mut [f64], count: usize) {
        for i in 0..count {
            let v = Self::read_at(to, i).applying(transform);
            Self::write_at(to, i, v);
        }
    }

    pub fn cross_product_with_vector(vector: Vector3, vectors: &mut [f64], count: usize) {
        for i in 0..count {
            let v = Self::read_at(vectors, i).cross_product(vector);
            Self::write_at(vectors, i, v);
        }
    }

    pub fn cross_product_with_vectors(other_vectors: &[f64], vectors: &mut [f64], count: usize) {
        for i in 0..count {
            let v = Self::read_at(vectors, i).cross_product(Self::read_at(other_vectors, i));
            Self::write_at(vectors, i, v);
        }
    }

    pub fn normalize(vectors: &mut [f64], count: usize) {
        for i in 0..count {
            let v = Self::read_at(vectors, i).normalized();
            Self::write_at(vectors, i, v);
        }
    }
}
