// Ported from IsiX:
// - Modules/IsiXGeometry/Sources/* & Modules/IsiXGeometry/Tests/GeometryTests.swift
// - Modules/IsiXImaging/Sources/ImagePlaneGeometry.swift
// - Modules/IsiXImaging/Sources/Geometry/ImagePlaneCoordinates.swift
// - Modules/IsiXImaging/Sources/Annotations/OrientationLetters.swift
// - Modules/IsiXImaging/Sources/ViewGeometry/*

#![allow(dead_code)]
#![allow(unused_imports)]

pub mod bezier;
pub mod image_plane;
pub mod line3;
pub mod orientation;
pub mod plane3;
pub mod transform3d;
pub mod vector3;

pub use bezier::{
    BezierClosestPoint, BezierCore, BezierNodeStyle, BezierPlaneIntersection, BezierSamples,
    BezierSegment, BezierSegmentKind,
};
pub use image_plane::{
    ImagePlaneCoordinates, ImagePlaneGeometry, LineGeometry, OriginDeltaSlice, PlaneGeometry,
    SettledPixelSpacing, SlicePlaneParams, ViewTransform, ViewportState,
};
pub use line3::{Line3, LineClosestPoints};
pub use orientation::{OrientationEdgeLetters, OrientationLetters};
pub use plane3::Plane3;
pub use transform3d::Transform3D;
pub use vector3::{adding_product, PackedVectors, Vector3, GEOMETRY_EPSILON};

#[cfg(test)]
mod tests {
    // Ported from IsiX: Modules/IsiXGeometry/Tests/GeometryTests.swift
    use super::*;

    #[test]
    fn test_fused_products_round_once() {
        // (1 + e)(1 - e) - 1 is -e^2 with the product fused into the
        // subtraction, and 0 when the product is rounded first.
        let e: f64 = 2.0_f64.powi(-30);
        assert_eq!(
            Vector3::new(1.0 + e, 1.0, 0.0).dot_product(Vector3::new(1.0 - e, -1.0, 0.0)),
            -e * e
        );
        assert_eq!(
            Vector3::new(0.0, 1.0 + e, 1.0)
                .cross_product(Vector3::new(0.0, 1.0, 1.0 - e))
                .x,
            -e * e
        );
    }

    #[test]
    fn test_vector_basics() {
        assert_eq!(Vector3::new(3.0, 4.0, 0.0).length(), 5.0);
        assert_eq!(Vector3::ZERO.normalized(), Vector3::ZERO);
        assert_eq!(
            Vector3::new(0.0, 0.0, 2.0).normalized(),
            Vector3::new(0.0, 0.0, 1.0)
        );
        assert_eq!(
            Vector3::new(1.0, 0.0, 0.0).cross_product(Vector3::new(0.0, 1.0, 0.0)),
            Vector3::new(0.0, 0.0, 1.0)
        );
        assert_eq!(
            Vector3::new(1.0, 2.0, 3.0).interpolated(Vector3::new(3.0, 2.0, 1.0), 0.5),
            Vector3::new(2.0, 2.0, 2.0)
        );
        // The projection scales by the dot product over the length, not its square.
        assert_eq!(
            Vector3::new(1.0, 1.0, 0.0).projected(Vector3::new(2.0, 0.0, 0.0)),
            Vector3::new(2.0, 0.0, 0.0)
        );
        assert!(Vector3::ZERO.inverted().x.is_sign_positive());
        let normal = Vector3::new(1.0, 2.0, 3.0).any_normal();
        assert!((normal.dot_product(Vector3::new(1.0, 2.0, 3.0)) - 0.0).abs() <= 1e-12);
        assert!((normal.length() - 1.0).abs() <= 1e-12);
    }

    #[test]
    fn test_angle_and_bend() {
        let angle = Vector3::new(1.0, 0.0, 0.0).angle(
            Vector3::new(0.0, 1.0, 0.0),
            Vector3::new(0.0, 0.0, 1.0),
        );
        assert!((angle - std::f64::consts::FRAC_PI_2).abs() <= 1e-12);
        let back = Vector3::new(0.0, 1.0, 0.0).angle(
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 0.0, 1.0),
        );
        assert!((back - 3.0 * std::f64::consts::FRAC_PI_2).abs() <= 1e-12);
        let bent =
            Vector3::new(0.0, 0.0, 1.0).bent(Vector3::new(1.0, 0.0, 0.0), Vector3::new(0.0, 1.0, 0.0));
        assert!((bent.z - 1.0).abs() <= 1e-12);
        let turned =
            Vector3::new(1.0, 0.0, 0.0).bent(Vector3::new(1.0, 0.0, 0.0), Vector3::new(0.0, 1.0, 0.0));
        assert!((turned.y - 1.0).abs() <= 1e-12);
    }

    #[test]
    fn test_lines_and_planes() {
        let line = Line3::from_points(Vector3::new(0.0, 0.0, -1.0), Vector3::new(0.0, 0.0, 1.0));
        assert_eq!(line.intersection(Plane3::Z_ZERO), Vector3::ZERO);
        assert!(line.intersects(Plane3::Z_ZERO));
        assert!(!Line3::X_AXIS.intersects(Plane3::new(
            Vector3::new(0.0, 0.0, 1.0),
            Vector3::new(0.0, 0.0, 1.0)
        )));
        assert!(
            Line3::X_AXIS
                .intersection(Plane3::new(
                    Vector3::new(0.0, 0.0, 1.0),
                    Vector3::new(0.0, 0.0, 1.0)
                ))
                .length()
                > 1e290
        );
        let closest = Line3::X_AXIS
            .closest_points(Line3::new(Vector3::new(0.0, 0.0, 2.0), Vector3::new(0.0, 1.0, 0.0)));
        assert_eq!(closest.distance, 2.0);
        assert_eq!(closest.on_self, Some(Vector3::ZERO));
        assert_eq!(closest.on_line, Some(Vector3::new(0.0, 0.0, 2.0)));
        let parallel = Line3::X_AXIS
            .closest_points(Line3::new(Vector3::new(0.0, 3.0, 0.0), Vector3::new(1.0, 0.0, 0.0)));
        assert_eq!(parallel.distance, 3.0);
        assert_eq!(parallel.on_self, None);
        assert_eq!(
            Vector3::new(5.0, 3.0, 0.0).distance_to_line(Line3::X_AXIS),
            3.0
        );
        assert!(Plane3::Z_ZERO.is_between(Vector3::new(0.0, 0.0, 1.0), Vector3::new(0.0, 0.0, -1.0)));
        assert!(!Plane3::Z_ZERO.is_between(Vector3::new(0.0, 0.0, 1.0), Vector3::new(0.0, 0.0, 2.0)));
        let meet = Plane3::X_ZERO.intersection(Plane3::Y_ZERO);
        assert_eq!(meet.direction, Vector3::new(0.0, 0.0, 1.0));
        assert_eq!(meet.point, Vector3::ZERO);
        assert_eq!(
            Plane3::least_squares(&[
                Vector3::ZERO,
                Vector3::new(1.0, 0.0, 0.0),
                Vector3::new(0.0, 1.0, 0.0)
            ]),
            Plane3::INVALID
        );
        assert_eq!(
            Plane3::least_squares(&[
                Vector3::ZERO,
                Vector3::new(1.0, 0.0, 0.0),
                Vector3::new(0.0, 1.0, 0.0),
                Vector3::new(1.0, 1.0, 0.0)
            ])
            .normal,
            Vector3::new(0.0, 0.0, 1.0)
        );
    }

    #[test]
    fn test_transforms() {
        let translation = Transform3D::make_translation(1.0, 2.0, 3.0);
        assert_eq!(
            Vector3::new(1.0, 1.0, 1.0).applying(translation),
            Vector3::new(2.0, 3.0, 4.0)
        );
        assert_eq!(
            Vector3::new(1.0, 1.0, 1.0).applying_to_direction(translation),
            Vector3::new(1.0, 1.0, 1.0)
        );
        let scale = Transform3D::make_scale(2.0, 3.0, 4.0);
        assert_eq!(scale.determinant(), 24.0);
        assert!(scale.is_rectilinear());
        assert!(!Transform3D::make_rotation(0.3, 0.0, 0.0, 1.0).is_rectilinear());
        let both = scale.concatenating(translation);
        assert_eq!(
            Vector3::new(1.0, 1.0, 1.0).applying(both),
            Vector3::new(3.0, 5.0, 7.0)
        );
        assert!(
            (Vector3::new(3.0, 5.0, 7.0).applying(both.inverted()).x - 1.0).abs() <= 1e-12
        );
        assert_eq!(both.inverted().m44, 1.0);
        assert_eq!(translation.transposed().m14, 1.0);
        let plane = Plane3::Z_ZERO.applying(Transform3D::make_translation(0.0, 0.0, 5.0));
        assert_eq!(plane.point, Vector3::new(0.0, 0.0, 5.0));
        assert_eq!(plane.normal, Vector3::new(0.0, 0.0, 1.0));
    }

    #[test]
    fn test_packed_vectors() {
        let mut values = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        PackedVectors::scale(&mut values, 2, 2.0);
        assert_eq!(values, [2.0, 4.0, 6.0, 8.0, 10.0, 12.0]);
        let others = [1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
        PackedVectors::add(&others, &mut values, 2);
        assert_eq!(values, [3.0, 5.0, 7.0, 9.0, 11.0, 13.0]);
        PackedVectors::apply(Transform3D::make_translation(1.0, 0.0, -1.0), &mut values, 2);
        assert_eq!(values, [4.0, 5.0, 6.0, 10.0, 11.0, 12.0]);
    }

    #[test]
    fn test_bezier_core_operations() {
        let nodes = [
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(10.0, 0.0, 0.0),
            Vector3::new(10.0, 10.0, 0.0),
            Vector3::new(0.0, 10.0, 0.0),
        ];
        let mut curve = BezierCore::curve_through(&nodes, BezierNodeStyle::EndsMeet);
        assert!(curve.has_curve());
        curve.add(
            BezierSegmentKind::Close,
            Vector3::ZERO,
            Vector3::ZERO,
            Vector3::ZERO,
        );
        assert_eq!(curve.subpath_count(), 1);
        assert!(curve.length() > 30.0);

        let area = curve.signed_area(Vector3::new(0.0, 0.0, 1.0));
        assert!(area > 90.0);

        let rev = curve.reversed();
        let rev_area = rev.signed_area(Vector3::new(0.0, 0.0, 1.0));
        assert!((area + rev_area).abs() < 1e-6);
    }
}
