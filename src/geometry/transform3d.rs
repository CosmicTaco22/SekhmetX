// Ported from IsiX: Modules/IsiXGeometry/Sources/AffineTransform3D.swift

use super::vector3::adding_product;

/// 4x4 3D transform matrix matching CoreAnimation's `CATransform3D` and IsiX's
/// `AffineTransform3D.swift` extensions.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Transform3D {
    pub m11: f64,
    pub m12: f64,
    pub m13: f64,
    pub m14: f64,
    pub m21: f64,
    pub m22: f64,
    pub m23: f64,
    pub m24: f64,
    pub m31: f64,
    pub m32: f64,
    pub m33: f64,
    pub m34: f64,
    pub m41: f64,
    pub m42: f64,
    pub m43: f64,
    pub m44: f64,
}

impl Default for Transform3D {
    fn default() -> Self {
        Self::IDENTITY
    }
}

#[derive(Clone, Copy, Debug)]
struct Vector4 {
    x: f64,
    y: f64,
    z: f64,
    w: f64,
}

impl Vector4 {
    #[inline]
    const fn new(x: f64, y: f64, z: f64, w: f64) -> Self {
        Self { x, y, z, w }
    }

    #[inline]
    fn scale(self, s: f64) -> Self {
        Self::new(self.x * s, self.y * s, self.z * s, self.w * s)
    }

    #[inline]
    fn adding_product(self, scalar: f64, v: Self) -> Self {
        Self::new(
            scalar.mul_add(v.x, self.x),
            scalar.mul_add(v.y, self.y),
            scalar.mul_add(v.z, self.z),
            scalar.mul_add(v.w, self.w),
        )
    }
}

impl Transform3D {
    pub const IDENTITY: Self = Self {
        m11: 1.0,
        m12: 0.0,
        m13: 0.0,
        m14: 0.0,
        m21: 0.0,
        m22: 1.0,
        m23: 0.0,
        m24: 0.0,
        m31: 0.0,
        m32: 0.0,
        m33: 1.0,
        m34: 0.0,
        m41: 0.0,
        m42: 0.0,
        m43: 0.0,
        m44: 1.0,
    };

    #[inline]
    pub const fn make_translation(tx: f64, ty: f64, tz: f64) -> Self {
        Self {
            m11: 1.0,
            m12: 0.0,
            m13: 0.0,
            m14: 0.0,
            m21: 0.0,
            m22: 1.0,
            m23: 0.0,
            m24: 0.0,
            m31: 0.0,
            m32: 0.0,
            m33: 1.0,
            m34: 0.0,
            m41: tx,
            m42: ty,
            m43: tz,
            m44: 1.0,
        }
    }

    #[inline]
    pub const fn make_scale(sx: f64, sy: f64, sz: f64) -> Self {
        Self {
            m11: sx,
            m12: 0.0,
            m13: 0.0,
            m14: 0.0,
            m21: 0.0,
            m22: sy,
            m23: 0.0,
            m24: 0.0,
            m31: 0.0,
            m32: 0.0,
            m33: sz,
            m34: 0.0,
            m41: 0.0,
            m42: 0.0,
            m43: 0.0,
            m44: 1.0,
        }
    }

    /// Matches CoreAnimation's `CATransform3DMakeRotation(angle, x, y, z)`.
    pub fn make_rotation(angle: f64, x: f64, y: f64, z: f64) -> Self {
        let len = (x * x + y * y + z * z).sqrt();
        if len == 0.0 {
            return Self::IDENTITY;
        }
        let ux = x / len;
        let uy = y / len;
        let uz = z / len;
        let c = angle.cos();
        let s = angle.sin();
        let one_minus_c = 1.0 - c;

        Self {
            m11: c + ux * ux * one_minus_c,
            m12: uy * ux * one_minus_c + uz * s,
            m13: uz * ux * one_minus_c - uy * s,
            m14: 0.0,
            m21: ux * uy * one_minus_c - uz * s,
            m22: c + uy * uy * one_minus_c,
            m23: uz * uy * one_minus_c + ux * s,
            m24: 0.0,
            m31: ux * uz * one_minus_c + uy * s,
            m32: uy * uz * one_minus_c - ux * s,
            m33: c + uz * uz * one_minus_c,
            m34: 0.0,
            m41: 0.0,
            m42: 0.0,
            m43: 0.0,
            m44: 1.0,
        }
    }

    pub fn from_opengl_matrix_d(m: &[f64; 16]) -> Self {
        Self {
            m11: m[0],
            m12: m[1],
            m13: m[2],
            m14: m[3],
            m21: m[4],
            m22: m[5],
            m23: m[6],
            m24: m[7],
            m31: m[8],
            m32: m[9],
            m33: m[10],
            m34: m[11],
            m41: m[12],
            m42: m[13],
            m43: m[14],
            m44: m[15],
        }
    }

    pub fn from_opengl_matrix_f(m: &[f32; 16]) -> Self {
        Self {
            m11: m[0] as f64,
            m12: m[1] as f64,
            m13: m[2] as f64,
            m14: m[3] as f64,
            m21: m[4] as f64,
            m22: m[5] as f64,
            m23: m[6] as f64,
            m24: m[7] as f64,
            m31: m[8] as f64,
            m32: m[9] as f64,
            m33: m[10] as f64,
            m34: m[11] as f64,
            m41: m[12] as f64,
            m42: m[13] as f64,
            m43: m[14] as f64,
            m44: m[15] as f64,
        }
    }

    pub fn write_opengl_matrix_d(self, m: &mut [f64; 16]) {
        *m = [
            self.m11, self.m12, self.m13, self.m14, self.m21, self.m22, self.m23, self.m24,
            self.m31, self.m32, self.m33, self.m34, self.m41, self.m42, self.m43, self.m44,
        ];
    }

    pub fn write_opengl_matrix_f(self, m: &mut [f32; 16]) {
        *m = [
            self.m11 as f32,
            self.m12 as f32,
            self.m13 as f32,
            self.m14 as f32,
            self.m21 as f32,
            self.m22 as f32,
            self.m23 as f32,
            self.m24 as f32,
            self.m31 as f32,
            self.m32 as f32,
            self.m33 as f32,
            self.m34 as f32,
            self.m41 as f32,
            self.m42 as f32,
            self.m43 as f32,
            self.m44 as f32,
        ];
    }

    /// Only scaling and translation: no rotation, shear, or perspective.
    #[inline]
    pub fn is_rectilinear(self) -> bool {
        self.m12 == 0.0
            && self.m13 == 0.0
            && self.m14 == 0.0
            && self.m21 == 0.0
            && self.m23 == 0.0
            && self.m24 == 0.0
            && self.m31 == 0.0
            && self.m32 == 0.0
            && self.m34 == 0.0
            && self.m44 == 1.0
    }

    #[inline]
    pub const fn transposed(self) -> Self {
        Self {
            m11: self.m11,
            m12: self.m21,
            m13: self.m31,
            m14: self.m41,
            m21: self.m12,
            m22: self.m22,
            m23: self.m32,
            m24: self.m42,
            m31: self.m13,
            m32: self.m23,
            m33: self.m33,
            m34: self.m43,
            m41: self.m14,
            m42: self.m24,
            m43: self.m34,
            m44: self.m44,
        }
    }

    /// Determinant with products fused in the same order as `AffineTransform3D.swift`.
    pub fn determinant(self) -> f64 {
        let a0 = adding_product(-self.m12 * self.m21, self.m11, self.m22);
        let a1 = adding_product(-self.m13 * self.m21, self.m11, self.m23);
        let a2 = adding_product(-self.m14 * self.m21, self.m11, self.m24);
        let a3 = adding_product(-self.m13 * self.m22, self.m12, self.m23);
        let a4 = adding_product(-self.m14 * self.m22, self.m12, self.m24);
        let a5 = adding_product(-self.m14 * self.m23, self.m13, self.m24);
        let b0 = adding_product(-self.m32 * self.m41, self.m31, self.m42);
        let b1 = adding_product(-self.m33 * self.m41, self.m31, self.m43);
        let b2 = adding_product(-self.m34 * self.m41, self.m31, self.m44);
        let b3 = adding_product(-self.m33 * self.m42, self.m32, self.m43);
        let b4 = adding_product(-self.m34 * self.m42, self.m32, self.m44);
        let b5 = adding_product(-self.m34 * self.m43, self.m33, self.m44);

        let term1 = adding_product(-a1 * b4, a0, b5);
        let term2 = adding_product(term1, a2, b3);
        let term3 = adding_product(term2, a3, b2);
        let term4 = adding_product(term3, -a4, b1);
        adding_product(term4, a5, b0)
    }

    #[inline]
    pub fn is_affine(self) -> bool {
        self.m14 == 0.0 && self.m24 == 0.0 && self.m34 == 0.0 && self.m44 == 1.0
    }

    #[inline]
    fn column1(self) -> Vector4 {
        Vector4::new(self.m11, self.m12, self.m13, self.m14)
    }

    #[inline]
    fn column2(self) -> Vector4 {
        Vector4::new(self.m21, self.m22, self.m23, self.m24)
    }

    #[inline]
    fn column3(self) -> Vector4 {
        Vector4::new(self.m31, self.m32, self.m33, self.m34)
    }

    #[inline]
    fn column4(self) -> Vector4 {
        Vector4::new(self.m41, self.m42, self.m43, self.m44)
    }

    #[inline]
    fn from_columns(c1: Vector4, c2: Vector4, c3: Vector4, c4: Vector4) -> Self {
        Self {
            m11: c1.x,
            m12: c1.y,
            m13: c1.z,
            m14: c1.w,
            m21: c2.x,
            m22: c2.y,
            m23: c2.z,
            m24: c2.w,
            m31: c3.x,
            m32: c3.y,
            m33: c3.z,
            m34: c3.w,
            m41: c4.x,
            m42: c4.y,
            m43: c4.z,
            m44: c4.w,
        }
    }

    /// Inverse of this 4x4 matrix, matching `AffineTransform3D.inverted` in IsiX.
    pub fn inverted(self) -> Self {
        let a0 = adding_product(-self.m12 * self.m21, self.m11, self.m22);
        let a1 = adding_product(-self.m13 * self.m21, self.m11, self.m23);
        let a2 = adding_product(-self.m14 * self.m21, self.m11, self.m24);
        let a3 = adding_product(-self.m13 * self.m22, self.m12, self.m23);
        let a4 = adding_product(-self.m14 * self.m22, self.m12, self.m24);
        let a5 = adding_product(-self.m14 * self.m23, self.m13, self.m24);
        let b0 = adding_product(-self.m32 * self.m41, self.m31, self.m42);
        let b1 = adding_product(-self.m33 * self.m41, self.m31, self.m43);
        let b2 = adding_product(-self.m34 * self.m41, self.m31, self.m44);
        let b3 = adding_product(-self.m33 * self.m42, self.m32, self.m43);
        let b4 = adding_product(-self.m34 * self.m42, self.m32, self.m44);
        let b5 = adding_product(-self.m34 * self.m43, self.m33, self.m44);

        let det = adding_product(
            adding_product(
                adding_product(
                    adding_product(adding_product(-a1 * b4, a0, b5), a2, b3),
                    a3,
                    b2,
                ),
                -a4,
                b1,
            ),
            a5,
            b0,
        );
        if det == 0.0 {
            return self;
        }
        let inv_det = 1.0 / det;

        let r1 = Vector4::new(
            adding_product(adding_product(-self.m23 * b4, self.m22, b5), self.m24, b3) * inv_det,
            adding_product(adding_product(-self.m12 * b5, self.m13, b4), -self.m14, b3) * inv_det,
            adding_product(adding_product(-self.m43 * a4, self.m42, a5), self.m44, a3) * inv_det,
            adding_product(adding_product(-self.m32 * a5, self.m33, a4), -self.m34, a3) * inv_det,
        );
        let r2 = Vector4::new(
            adding_product(adding_product(-self.m21 * b5, self.m23, b2), -self.m24, b1) * inv_det,
            adding_product(adding_product(-self.m13 * b2, self.m11, b5), self.m14, b1) * inv_det,
            adding_product(adding_product(-self.m41 * a5, self.m43, a2), -self.m44, a1) * inv_det,
            adding_product(adding_product(-self.m33 * a2, self.m31, a5), self.m34, a1) * inv_det,
        );
        let r3 = Vector4::new(
            adding_product(adding_product(-self.m22 * b2, self.m21, b4), self.m24, b0) * inv_det,
            adding_product(adding_product(-self.m11 * b4, self.m12, b2), -self.m14, b0) * inv_det,
            adding_product(adding_product(-self.m42 * a2, self.m41, a4), self.m44, a0) * inv_det,
            adding_product(adding_product(-self.m31 * a4, self.m32, a2), -self.m34, a0) * inv_det,
        );
        let r4 = Vector4::new(
            adding_product(adding_product(-self.m21 * b3, self.m22, b1), -self.m23, b0) * inv_det,
            adding_product(adding_product(-self.m12 * b1, self.m11, b3), self.m13, b0) * inv_det,
            adding_product(adding_product(-self.m41 * a3, self.m42, a1), -self.m43, a0) * inv_det,
            adding_product(adding_product(-self.m32 * a1, self.m31, a3), self.m33, a0) * inv_det,
        );

        let mut inv = Self::from_columns(r1, r2, r3, r4);
        if self.is_affine() {
            inv.m14 = 0.0;
            inv.m24 = 0.0;
            inv.m34 = 0.0;
            inv.m44 = 1.0;
        }
        inv
    }

    /// Concatenates `self` followed by `t2` (`self * t2`), matching `AffineTransform3D.concatenating`.
    pub fn concatenating(self, t2: Self) -> Self {
        let mut r1 = t2
            .column1()
            .scale(self.m11)
            .adding_product(self.m12, t2.column2())
            .adding_product(self.m13, t2.column3());
        let mut r2 = t2
            .column1()
            .scale(self.m21)
            .adding_product(self.m22, t2.column2())
            .adding_product(self.m23, t2.column3());
        let mut r3 = t2
            .column1()
            .scale(self.m31)
            .adding_product(self.m32, t2.column2())
            .adding_product(self.m33, t2.column3());
        let mut r4 = t2
            .column1()
            .scale(self.m41)
            .adding_product(self.m42, t2.column2())
            .adding_product(self.m43, t2.column3());

        if self.is_affine() && t2.is_affine() {
            r4 = Vector4::new(
                r4.x + t2.m41,
                r4.y + t2.m42,
                r4.z + t2.m43,
                r4.w + t2.m44,
            );
        } else {
            r1 = r1.adding_product(self.m14, t2.column4());
            r2 = r2.adding_product(self.m24, t2.column4());
            r3 = r3.adding_product(self.m34, t2.column4());
            r4 = r4.adding_product(self.m44, t2.column4());
        }
        Self::from_columns(r1, r2, r3, r4)
    }
}
