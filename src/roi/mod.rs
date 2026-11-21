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
        let mut samples = match &frame.pixels {
            Pixels::Gray(gray) => {
                RoiPolygonFill::sample_gray(&clipped, gray, frame.width as usize, frame.height as usize)
            }
            Pixels::Rgba(rgba) => {
                RoiPolygonFill::sample_rgba(&clipped, rgba, frame.width as usize, frame.height as usize)
            }
        };
        if let Some(suv_factor) = frame.suv_factor {
            let f = suv_factor as f32;
            for v in &mut samples {
                *v *= f;
            }
        }
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
    let mut vals = match &frame.pixels {
        Pixels::Gray(gray) => {
            RoiPolygonFill::sample_gray(&clipped, gray, frame.width as usize, frame.height as usize)
        }
        Pixels::Rgba(rgba) => {
            RoiPolygonFill::sample_rgba(&clipped, rgba, frame.width as usize, frame.height as usize)
        }
    };
    if let Some(suv_factor) = frame.suv_factor {
        let f = suv_factor as f32;
        for v in &mut vals {
            *v *= f;
        }
    }
    vals
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
                let scaled = if let Some(suv_factor) = frame.suv_factor {
                    val * (suv_factor as f32)
                } else {
                    val
                };
                out.push(scaled);
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

/// Computes the 3D volume in `cm³` from `(slice_position_mm, area_mm2)` pairs across
/// slices using trapezoidal integration with half-slice end caps (Horos ROI Volume).
pub fn compute_roi_volume_cm3(
    slice_areas_mm2: &[(f64, f64)],
    fallback_thickness_mm: f64,
) -> Option<f64> {
    let mut valid: Vec<(f64, f64)> = slice_areas_mm2
        .iter()
        .copied()
        .filter(|&(z, a)| z.is_finite() && a.is_finite() && a > 0.0)
        .collect();
    if valid.is_empty() {
        return None;
    }
    valid.sort_by(|a, b| a.0.total_cmp(&b.0));
    if valid.len() == 1 {
        let thick = fallback_thickness_mm.max(0.0);
        if thick <= 0.0 {
            return None;
        }
        return Some((valid[0].1 * thick) / 1000.0);
    }
    let mut vol_mm3 = 0.0_f64;
    for w in valid.windows(2) {
        let dz = (w[1].0 - w[0].0).abs();
        vol_mm3 += 0.5 * (w[0].1 + w[1].1) * dz;
    }
    let avg_dz = (valid.last().unwrap().0 - valid[0].0).abs() / (valid.len() - 1) as f64;
    vol_mm3 += 0.5 * (valid[0].1 + valid.last().unwrap().1) * avg_dz;
    Some(vol_mm3 / 1000.0)
}

/// 2D Region Growing from `(seed_x, seed_y)` within `±tolerance` of the seed intensity.
/// Returns a simplified closed boundary polygon (in image pixel coordinates) suitable
/// for `Measure::Polygon`.
pub fn grow_region_polygon(
    frame: &Frame,
    seed_x: usize,
    seed_y: usize,
    tolerance: f32,
) -> Vec<(f32, f32)> {
    let w = frame.width as usize;
    let h = frame.height as usize;
    if seed_x >= w || seed_y >= h || w == 0 || h == 0 {
        return Vec::new();
    }
    let Pixels::Gray(gray) = &frame.pixels else {
        return Vec::new();
    };
    let seed_val = gray[seed_y * w + seed_x];
    if !seed_val.is_finite() {
        return Vec::new();
    }
    let tol = tolerance.abs().max(1.0);
    let lo = seed_val - tol;
    let hi = seed_val + tol;

    let mut visited = vec![false; w * h];
    let mut queue = std::collections::VecDeque::new();
    visited[seed_y * w + seed_x] = true;
    queue.push_back((seed_x, seed_y));

    let mut region = Vec::new();
    const MAX_PIXELS: usize = 150_000;
    while let Some((x, y)) = queue.pop_front() {
        region.push((x, y));
        if region.len() >= MAX_PIXELS {
            break;
        }
        let neighbors = [
            (x.wrapping_sub(1), y),
            (x + 1, y),
            (x, y.wrapping_sub(1)),
            (x, y + 1),
        ];
        for (nx, ny) in neighbors {
            if nx < w && ny < h {
                let idx = ny * w + nx;
                if !visited[idx] {
                    let v = gray[idx];
                    if v.is_finite() && v >= lo && v <= hi {
                        visited[idx] = true;
                        queue.push_back((nx, ny));
                    }
                }
            }
        }
    }
    if region.len() < 4 {
        return Vec::new();
    }

    // Collect boundary pixels and bin them into 48 angular sectors around the centroid
    // (taking the outermost boundary pixel per sector for a clean, non-self-intersecting contour).
    let (mut cx, mut cy) = (0.0f64, 0.0f64);
    for &(x, y) in &region {
        cx += x as f64;
        cy += y as f64;
    }
    cx /= region.len() as f64;
    cy /= region.len() as f64;

    const SECTORS: usize = 48;
    let mut best: [Option<(f64, f32, f32)>; SECTORS] = [None; SECTORS];
    for &(x, y) in &region {
        let is_edge = x == 0
            || y == 0
            || x + 1 == w
            || y + 1 == h
            || !visited[y * w + (x - 1)]
            || !visited[y * w + (x + 1)]
            || !visited[(y - 1) * w + x]
            || !visited[(y + 1) * w + x];
        if !is_edge {
            continue;
        }
        let dx = x as f64 - cx;
        let dy = y as f64 - cy;
        let dist2 = dx * dx + dy * dy;
        let mut ang = dy.atan2(dx);
        if ang < 0.0 {
            ang += std::f64::consts::TAU;
        }
        let bin = ((ang / std::f64::consts::TAU) * SECTORS as f64).floor() as usize % SECTORS;
        if best[bin].is_none_or(|(d, _, _)| dist2 > d) {
            best[bin] = Some((dist2, x as f32 + 0.5, y as f32 + 0.5));
        }
    }
    best.into_iter()
        .flatten()
        .map(|(_, x, y)| (x, y))
        .collect()
}

/// Resamples a closed polygon to `n` evenly spaced points along its perimeter.
pub fn resample_closed_polygon(pts: &[(f32, f32)], n: usize) -> Vec<(f32, f32)> {
    if pts.len() < 3 || n < 3 {
        return pts.to_vec();
    }
    let m = pts.len();
    let mut seg_lens = Vec::with_capacity(m);
    let mut total = 0.0f32;
    for i in 0..m {
        let a = pts[i];
        let b = pts[(i + 1) % m];
        let d = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        seg_lens.push(d);
        total += d;
    }
    if total <= 1e-5 {
        return vec![pts[0]; n];
    }
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        let target = (k as f32 / n as f32) * total;
        let mut acc = 0.0f32;
        let mut chosen = pts[0];
        for i in 0..m {
            let d = seg_lens[i];
            if acc + d >= target || i + 1 == m {
                let f = if d > 1e-6 {
                    ((target - acc) / d).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let a = pts[i];
                let b = pts[(i + 1) % m];
                chosen = (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f);
                break;
            }
            acc += d;
        }
        out.push(chosen);
    }
    out
}

/// Linearly interpolates between two closed polygons `a` (at t=0) and `b` (at t=1).
pub fn interpolate_polygon(a: &[(f32, f32)], b: &[(f32, f32)], t: f32) -> Vec<(f32, f32)> {
    const N: usize = 32;
    let ra = resample_closed_polygon(a, N);
    let rb = resample_closed_polygon(b, N);
    if ra.len() != N || rb.len() != N {
        return Vec::new();
    }
    let t = t.clamp(0.0, 1.0);
    ra.iter()
        .zip(rb.iter())
        .map(|(p, q)| (p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t))
        .collect()
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

    #[test]
    fn roi_volume_trapezoidal_cylinder_is_exact() {
        // 5 slices spaced 2 mm apart, each with 100 mm² area -> 100 * (5 * 2) = 1000 mm³ = 1.0 cm³
        let slices = [(0.0, 100.0), (2.0, 100.0), (4.0, 100.0), (6.0, 100.0), (8.0, 100.0)];
        let vol = compute_roi_volume_cm3(&slices, 2.0).unwrap();
        assert!((vol - 1.0).abs() < 1e-9, "got {vol}");
        // Single slice with 5 mm thickness: 200 mm² * 5 mm = 1.0 cm³
        assert!((compute_roi_volume_cm3(&[(10.0, 200.0)], 5.0).unwrap() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn grow_region_and_polygon_interpolation_work() {
        let mut pixels = vec![0.0f32; 20 * 20];
        for y in 6..14 {
            for x in 6..14 {
                pixels[y * 20 + x] = 200.0;
            }
        }
        let f = Frame {
            width: 20,
            height: 20,
            pixels: Pixels::Gray(pixels),
            stored_window: None,
            full_window: crate::imaging::Window::new(100.0, 200.0),
            bits_window: None,
            inverts: false,
            voi_lut_applied: false,
            slope: 1.0,
            spacing: Some((1.0, 1.0)),
            pixel_ratio: 1.0,
            origin: None,
            orientation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            is_3d_plane: false,
            frame_of_reference_uid: None,
            slice_thickness: None,
            slice_location: None,
            value_unit: "HU",
            suv_factor: None,
        };
        let poly = grow_region_polygon(&f, 10, 10, 40.0);
        assert!(poly.len() >= 12, "expected contour polygon, got {}", poly.len());
        let a = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        let b = vec![(10.0, 10.0), (20.0, 10.0), (20.0, 20.0), (10.0, 20.0)];
        let mid = interpolate_polygon(&a, &b, 0.5);
        assert_eq!(mid.len(), 32);
        assert!((mid[0].0 - 5.0).abs() < 1e-3 && (mid[0].1 - 5.0).abs() < 1e-3);
    }
}
