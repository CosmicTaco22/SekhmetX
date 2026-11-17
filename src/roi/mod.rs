//! Horos / IsiX Regions of Interest (`IsiXROI`):
//! - `geometry`: `RoiPoint`, `RoiSize`, `RoiRect`, `RoiBoundingBox`, `RoiDragConstraint`,
//!   `RoiPlanarGeometry` (calibrated lengths, perimeters, angles, acute/Cobb angles,
//!   polygon & ellipse areas, centroids, resampling), and `RoiSpline`.
//! - `statistics`: `RoiPixelStatistics` (mean, total, sample SD, min, max, skewness,
//!   kurtosis, median), `RoiValueStatistics`, and `CalciumScoreSums`.
//! - `raster`: `RoiRasterConversion`, `RoiPixelPoint`, `RoiScanline` (edge-table polygon
//!   scanline fill), `RoiPolygonClip` (Sutherland-Hodgman clipper), `RoiPolygonFill`
//!   (frame pixel extraction), `RoiLineRaster`, and `RoiPolygonContainment`.
//! - `RegionOfInterest`: unified ROI shape & measurement evaluation against a `Frame`,
//!   plus RFC 4180 CSV export (`RoiCsvExport`) and length ratios (`RoiLengthRatio`).

#![allow(dead_code)]
#![allow(unused_imports)]

pub mod geometry;
pub mod raster;
pub mod statistics;

pub use geometry::{
    RoiBoundingBox, RoiDragConstraint, RoiPlanarGeometry, RoiPoint, RoiRect, RoiSize, RoiSpline,
};
pub use raster::{
    RoiLineRaster, RoiPixelPoint, RoiPolygonClip, RoiPolygonContainment, RoiPolygonFill,
    RoiRasterConversion, RoiScanline,
};
pub use statistics::{CalciumScoreSums, RoiPixelStatistics, RoiValueStatistics};

use crate::frame::{Frame, Pixels};

/// Horos `ToolMode` type codes (`ROICSVExport.TypeCode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoiKind {
    Length,
    Angle,
    DynamicAngle,
    Rectangle,
    Oval,
    ClosedPolygon,
    OpenPolygon,
    Pencil,
    Point,
}

impl RoiKind {
    pub const fn type_code(self) -> i32 {
        match self {
            Self::Length => 5,
            Self::Rectangle => 6,
            Self::Oval => 9,
            Self::OpenPolygon => 10,
            Self::ClosedPolygon => 11,
            Self::Angle => 12,
            Self::Pencil => 15,
            Self::Point => 19,
            Self::DynamicAngle => 27,
        }
    }

    pub const fn type_name(self) -> &'static str {
        match self {
            Self::Length => "Length",
            Self::Rectangle => "Rectangle",
            Self::Oval => "Oval",
            Self::OpenPolygon => "Open Polygon",
            Self::ClosedPolygon => "Closed Polygon",
            Self::Angle => "Angle",
            Self::Pencil => "Pencil",
            Self::Point => "Point",
            Self::DynamicAngle => "Dynamic Angle",
        }
    }

    pub const fn has_area(self) -> bool {
        matches!(
            self,
            Self::Rectangle
                | Self::Oval
                | Self::ClosedPolygon
                | Self::OpenPolygon
                | Self::Pencil
        )
    }

    pub const fn has_statistics(self) -> bool {
        self.has_area()
    }
}

/// Complete measurement readout for a ROI evaluated on a `Frame`.
#[derive(Clone, Debug)]
pub struct RoiEvaluation {
    /// Area in $\text{cm}^2$ when calibrated, or $\text{px}^2$ when uncalibrated.
    pub area_cm2_or_px2: Option<f32>,
    /// Area in $\text{mm}^2$ when calibrated.
    pub area_mm2: Option<f32>,
    /// Perimeter or length in $\text{cm}$ when calibrated, or $\text{px}$ when uncalibrated.
    pub length_cm_or_px: Option<f32>,
    /// Length in $\text{mm}$ when calibrated.
    pub length_mm: Option<f32>,
    /// Length in pixels.
    pub length_px: Option<f32>,
    /// Angle in degrees (for `Angle` or `DynamicAngle`).
    pub angle_deg: Option<f32>,
    /// Whether the frame has valid `(row_spacing, col_spacing)` calibration.
    pub calibrated: bool,
    /// Interior pixel intensity statistics (`Mean`, `SD`, `Min`, `Max`, `Total`, `Count`).
    pub stats: Option<RoiPixelStatistics>,
    /// Single-point readout for `Point` ROIs.
    pub point_value: Option<String>,
    /// Optional 3D patient coordinates `(X, Y, Z)` in mm at the ROI anchor/centroid.
    pub patient_mm: Option<[f64; 3]>,
}

impl RoiEvaluation {
    /// Formats a multi-line Horos-style floating callout label for a viewport overlay.
    pub fn format_callout(&self, kind: RoiKind, value_unit: &str, name: &str) -> Vec<String> {
        let mut lines = Vec::new();
        if !name.is_empty() {
            lines.push(name.to_string());
        }
        match kind {
            RoiKind::Length => {
                if self.calibrated {
                    if let (Some(mm), Some(cm)) = (self.length_mm, self.length_cm_or_px) {
                        if mm >= 10.0 {
                            lines.push(format!("{cm:.2} cm ({mm:.1} mm)"));
                        } else {
                            lines.push(format!("{mm:.2} mm"));
                        }
                    }
                } else if let Some(px) = self.length_px {
                    lines.push(format!("{px:.1} px"));
                }
            }
            RoiKind::Angle | RoiKind::DynamicAngle => {
                if let Some(deg) = self.angle_deg {
                    lines.push(format!("{deg:.1}°"));
                }
            }
            RoiKind::Point => {
                if let Some(val) = &self.point_value {
                    lines.push(val.clone());
                }
                if let Some([x, y, z]) = self.patient_mm {
                    lines.push(format!("({x:.1}, {y:.1}, {z:.1}) mm"));
                }
            }
            RoiKind::Rectangle
            | RoiKind::Oval
            | RoiKind::ClosedPolygon
            | RoiKind::OpenPolygon
            | RoiKind::Pencil => {
                let unit = if value_unit.is_empty() {
                    ""
                } else {
                    value_unit
                };
                if let Some(st) = &self.stats
                    && st.count > 0
                {
                    let u_suffix = if unit.is_empty() {
                        String::new()
                    } else {
                        format!(" {unit}")
                    };
                    lines.push(format!(
                        "Mean: {:.1}{u_suffix}  SD: {:.2}",
                        st.mean, st.deviation
                    ));
                    lines.push(format!(
                        "Max: {:.0}{u_suffix}  Min: {:.0}{u_suffix}",
                        st.maximum, st.minimum
                    ));
                }
                let px_count = self.stats.as_ref().map(|s| s.count).unwrap_or(0);
                if self.calibrated {
                    if let (Some(cm2), Some(mm2)) = (self.area_cm2_or_px2, self.area_mm2) {
                        let perim_cm = self.length_mm.unwrap_or(0.0) / 10.0;
                        if cm2 >= 1.0 {
                            lines.push(format!(
                                "Area: {cm2:.3} cm² ({px_count} px)  P: {perim_cm:.2} cm"
                            ));
                        } else {
                            lines.push(format!(
                                "Area: {mm2:.1} mm² ({cm2:.4} cm²) ({px_count} px)"
                            ));
                        }
                    }
                } else if let Some(px2) = self.area_cm2_or_px2 {
                    let perim_px = self.length_cm_or_px.unwrap_or(0.0);
                    lines.push(format!("Area: {px2:.0} px² ({px_count} px)  P: {perim_px:.1} px"));
                }
            }
        }
        lines
    }
}

/// Evaluates planar geometry and interior pixel statistics of a polygon/oval/rectangle
/// outline against `frame`.
pub fn evaluate_region(
    frame: &Frame,
    kind: RoiKind,
    outline: &[RoiPoint],
    oval_half_size: Option<RoiSize>,
) -> RoiEvaluation {
    evaluate_region_with_spacing(frame, kind, outline, oval_half_size, frame.spacing)
}

/// Evaluates planar geometry and interior pixel statistics of a polygon/oval/rectangle
/// outline against `frame` using an explicit pixel spacing `(row_mm, col_mm)`.
pub fn evaluate_region_with_spacing(
    frame: &Frame,
    kind: RoiKind,
    outline: &[RoiPoint],
    oval_half_size: Option<RoiSize>,
    spacing: Option<(f64, f64)>,
) -> RoiEvaluation {
    let (sp_y, sp_x) = match spacing {
        Some((rs, cs)) if rs > 0.0 && cs > 0.0 => (rs, cs),
        _ => (0.0, 0.0),
    };
    let calibrated = sp_x > 0.0 && sp_y > 0.0;

    // 1. Area in square pixels (`ROIPlanarGeometry`), then converted to cm² and mm².
    let area_px2 = if let Some(half_size) = oval_half_size {
        RoiPlanarGeometry::area_of_ellipse_with_half_size(half_size)
    } else {
        RoiPlanarGeometry::area_of_polygon(outline)
    };
    let (area_cm2_or_px2, area_mm2) = if calibrated {
        let mm2 = (area_px2 as f64) * sp_x * sp_y;
        let cm2 = mm2 / 100.0;
        (Some(cm2 as f32), Some(mm2 as f32))
    } else {
        (Some(area_px2), None)
    };

    // 2. Perimeter (`ROIPlanarGeometry::perimeter`).
    let closed = !matches!(kind, RoiKind::OpenPolygon);
    let perim_cm_or_px = RoiPlanarGeometry::perimeter(outline, closed, sp_x, sp_y);
    let perim_mm = if calibrated {
        Some(perim_cm_or_px * 10.0)
    } else {
        None
    };

    // 3. Scanline interior pixel sampling (`ROIPolygonClip` + `ROIPolygonFill`) and statistics (`ROIPixelStatistics`).
    let stats = if outline.len() >= 3 {
        let px_pts: Vec<RoiPixelPoint> = outline
            .iter()
            .copied()
            .map(RoiPixelPoint::from_point)
            .collect();
        let clipped = RoiPolygonClip::clip(
            &px_pts,
            RoiPoint::ZERO,
            RoiPoint::new(frame.width as f64, frame.height as f64),
        );
        let samples = match &frame.pixels {
            Pixels::Gray(gray) => {
                RoiPolygonFill::sample_gray(&clipped, gray, frame.width as usize, frame.height as usize)
            }
            Pixels::Rgba(rgba) => {
                RoiPolygonFill::sample_rgba(&clipped, rgba, frame.width as usize, frame.height as usize)
            }
        };
        Some(RoiPixelStatistics::compute(&samples, true, false))
    } else {
        None
    };

    let centroid = RoiPlanarGeometry::centroid(outline);
    let patient_mm = frame.patient_coords_at(centroid.x, centroid.y);

    RoiEvaluation {
        area_cm2_or_px2,
        area_mm2,
        length_cm_or_px: Some(perim_cm_or_px),
        length_mm: perim_mm,
        length_px: None,
        angle_deg: None,
        calibrated,
        stats,
        point_value: None,
        patient_mm,
    }
}

/// Evaluates a 2-point Length ROI on `frame`.
pub fn evaluate_length(frame: &Frame, a: RoiPoint, b: RoiPoint) -> RoiEvaluation {
    evaluate_length_with_spacing(frame, a, b, frame.spacing)
}

/// Evaluates a 2-point Length ROI on `frame` using an explicit pixel spacing.
pub fn evaluate_length_with_spacing(
    frame: &Frame,
    a: RoiPoint,
    b: RoiPoint,
    spacing: Option<(f64, f64)>,
) -> RoiEvaluation {
    let (sp_y, sp_x) = match spacing {
        Some((rs, cs)) if rs > 0.0 && cs > 0.0 => (rs, cs),
        _ => (0.0, 0.0),
    };
    let calibrated = sp_x > 0.0 && sp_y > 0.0;
    let (cm_or_px, px) = RoiPlanarGeometry::measure_length(a, b, sp_x, sp_y);
    let mm = if calibrated {
        Some(cm_or_px * 10.0)
    } else {
        None
    };
    let mid = RoiPlanarGeometry::point_between(a, b, 0.5);
    RoiEvaluation {
        area_cm2_or_px2: None,
        area_mm2: None,
        length_cm_or_px: Some(cm_or_px),
        length_mm: mm,
        length_px: Some(px),
        angle_deg: None,
        calibrated,
        stats: None,
        point_value: None,
        patient_mm: frame.patient_coords_at(mid.x, mid.y),
    }
}

/// Evaluates a 3-point Angle ROI on `frame`.
pub fn evaluate_angle(frame: &Frame, a: RoiPoint, vertex: RoiPoint, b: RoiPoint) -> RoiEvaluation {
    evaluate_angle_with_spacing(frame, a, vertex, b, frame.spacing)
}

/// Evaluates a 3-point Angle ROI on `frame` using an explicit pixel spacing.
pub fn evaluate_angle_with_spacing(
    frame: &Frame,
    a: RoiPoint,
    vertex: RoiPoint,
    b: RoiPoint,
    spacing: Option<(f64, f64)>,
) -> RoiEvaluation {
    let (sp_y, sp_x) = match spacing {
        Some((rs, cs)) if rs > 0.0 && cs > 0.0 => (rs, cs),
        _ => (0.0, 0.0),
    };
    let calibrated = sp_x > 0.0 && sp_y > 0.0;
    let deg = RoiPlanarGeometry::angle_at(vertex, a, b, sp_x, sp_y);
    RoiEvaluation {
        area_cm2_or_px2: None,
        area_mm2: None,
        length_cm_or_px: None,
        length_mm: None,
        length_px: None,
        angle_deg: Some(deg),
        calibrated,
        stats: None,
        point_value: None,
        patient_mm: frame.patient_coords_at(vertex.x, vertex.y),
    }
}

/// Evaluates a 4-point Dynamic / Cobb Angle ROI between lines `a1-a2` and `b1-b2` on `frame`.
pub fn evaluate_cobb_angle(
    frame: &Frame,
    a1: RoiPoint,
    a2: RoiPoint,
    b1: RoiPoint,
    b2: RoiPoint,
) -> RoiEvaluation {
    evaluate_cobb_angle_with_spacing(frame, a1, a2, b1, b2, frame.spacing)
}

/// Evaluates a 4-point Dynamic / Cobb Angle ROI between lines `a1-a2` and `b1-b2` on `frame`
/// using an explicit pixel spacing.
pub fn evaluate_cobb_angle_with_spacing(
    frame: &Frame,
    a1: RoiPoint,
    a2: RoiPoint,
    b1: RoiPoint,
    b2: RoiPoint,
    spacing: Option<(f64, f64)>,
) -> RoiEvaluation {
    let (sp_y, sp_x) = match spacing {
        Some((rs, cs)) if rs > 0.0 && cs > 0.0 => (rs, cs),
        _ => (0.0, 0.0),
    };
    let calibrated = sp_x > 0.0 && sp_y > 0.0;
    let deg = RoiPlanarGeometry::acute_angle_between_lines(a1, a2, b1, b2, sp_x, sp_y);
    let mid = RoiPlanarGeometry::point_between(a1, b1, 0.5);
    RoiEvaluation {
        area_cm2_or_px2: None,
        area_mm2: None,
        length_cm_or_px: None,
        length_mm: None,
        length_px: None,
        angle_deg: Some(deg),
        calibrated,
        stats: None,
        point_value: None,
        patient_mm: frame.patient_coords_at(mid.x, mid.y),
    }
}

/// Samples all interior pixel values inside a closed `outline` on `frame` (for HU histograms).
pub fn sample_region_values(frame: &Frame, outline: &[RoiPoint]) -> Vec<f32> {
    if outline.len() < 3 || frame.width == 0 || frame.height == 0 {
        return Vec::new();
    }
    let px_pts: Vec<RoiPixelPoint> = outline
        .iter()
        .copied()
        .map(RoiPixelPoint::from_point)
        .collect();
    let clipped = RoiPolygonClip::clip(
        &px_pts,
        RoiPoint::ZERO,
        RoiPoint::new(frame.width as f64, frame.height as f64),
    );
    match &frame.pixels {
        Pixels::Gray(gray) => {
            RoiPolygonFill::sample_gray(&clipped, gray, frame.width as usize, frame.height as usize)
        }
        Pixels::Rgba(rgba) => {
            RoiPolygonFill::sample_rgba(&clipped, rgba, frame.width as usize, frame.height as usize)
        }
    }
}

/// Samples `steps` evenly spaced intensity values along line segment `a -> b` on `frame`.
pub fn sample_line_profile(frame: &Frame, a: RoiPoint, b: RoiPoint, steps: usize) -> Vec<f32> {
    if steps < 2 || frame.width == 0 || frame.height == 0 {
        return Vec::new();
    }
    let w = frame.width as isize;
    let h = frame.height as isize;
    let mut out = Vec::with_capacity(steps);
    for i in 0..steps {
        let t = i as f64 / (steps - 1) as f64;
        let x = (a.x + (b.x - a.x) * t).round() as isize;
        let y = (a.y + (b.y - a.y) * t).round() as isize;
        if x >= 0 && x < w && y >= 0 && y < h {
            let idx = (y as usize) * (frame.width as usize) + (x as usize);
            let val = match &frame.pixels {
                Pixels::Gray(gray) => gray.get(idx).copied().unwrap_or(0.0),
                Pixels::Rgba(rgba) => {
                    let b_idx = idx * 4;
                    if b_idx + 2 < rgba.len() {
                        0.299 * rgba[b_idx] as f32
                            + 0.587 * rgba[b_idx + 1] as f32
                            + 0.114 * rgba[b_idx + 2] as f32
                    } else {
                        0.0
                    }
                }
            };
            if val.is_finite() {
                out.push(val);
            }
        }
    }
    out
}

/// Evaluates a single Point / 2D Probe ROI on `frame`.
pub fn evaluate_point(frame: &Frame, p: RoiPoint) -> RoiEvaluation {
    let x = p.x.round() as i32;
    let y = p.y.round() as i32;
    let point_value = if x >= 0 && y >= 0 {
        frame.value_at(x as u32, y as u32)
    } else {
        None
    };
    RoiEvaluation {
        area_cm2_or_px2: None,
        area_mm2: None,
        length_cm_or_px: None,
        length_mm: None,
        length_px: None,
        angle_deg: None,
        calibrated: frame.spacing.is_some(),
        stats: None,
        point_value,
        patient_mm: frame.patient_coords_at(p.x, p.y),
    }
}

/// Ratio of two lengths (e.g., Evans index, cardiothoracic ratio, Insall-Salvati)
/// ported from `ROILengthRatio.swift`.
pub struct RoiLengthRatio;

impl RoiLengthRatio {
    pub fn value(
        numerator: f64,
        numerator_calibrated: bool,
        denominator: f64,
        denominator_calibrated: bool,
    ) -> Option<f64> {
        if numerator_calibrated != denominator_calibrated
            || !numerator.is_finite()
            || !denominator.is_finite()
            || numerator < 0.0
            || denominator <= 0.0
        {
            return None;
        }
        let r = numerator / denominator;
        r.is_finite().then_some(r)
    }

    pub fn text(ratio: Option<f64>) -> String {
        let Some(r) = ratio.filter(|v| v.is_finite()) else {
            return "\u{2013}".to_string();
        };
        if r < 0.0995 {
            format!("{r:.3}")
        } else {
            format!("{r:.2}")
        }
    }

    pub fn label_text(numerator: &str, denominator: &str, ratio: Option<f64>) -> String {
        format!("{numerator} / {denominator} = {}", Self::text(ratio))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_ratio_formatting() {
        let r = RoiLengthRatio::value(3.1, true, 10.0, true);
        assert!((r.unwrap() - 0.31).abs() < 1e-12);
        assert_eq!(
            RoiLengthRatio::label_text("3.100 cm", "10.000 cm", r),
            "3.100 cm / 10.000 cm = 0.31"
        );
        assert_eq!(RoiLengthRatio::text(Some(0.0456)), "0.046");
        assert_eq!(RoiLengthRatio::value(2.0, true, 2.0, false), None);
        assert_eq!(RoiLengthRatio::value(2.0, true, 0.0, true), None);
    }
}
