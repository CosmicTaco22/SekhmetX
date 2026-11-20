//! Curved MPR (CPR): a straightened view along a path drawn through a volume.
//!
//! Follows IsiX `Horos/Sources/CPRStraightenedGeneration.swift` (the checks a
//! path must pass before a straightened image is made: at least three points,
//! no degenerate segment, at least 1 mm long; a path that crosses itself is
//! reported, not refused) and `CPRStraightenedOperation.swift` (the sampling:
//! one image column per step along the curve, `length / width` apart; each row
//! steps across the curve along a normal kept at right angles to it, rows centred
//! on the curve). The path is smoothed through the clicked points (IsiX smooths a
//! Bézier path); here a centripetal Catmull-Rom spline is used, which also passes
//! through every point the reader placed.

use std::sync::Arc;

use crate::frame::{Frame, Pixels};
use crate::volume::{add3, cross3, dot3, len3, normalize3, scale3, sub3, Volume3D};

const MINIMUM_NODES: usize = 3;
const MINIMUM_LENGTH_MM: f64 = 1.0;
const MINIMUM_SAMPLE_SPACING: f64 = 1e-4;
const DEGENERATE_SEGMENT: f64 = 1e-8;
const LOOP_PROXIMITY_MM: f64 = 0.5;

/// Whether a path can be straightened (IsiX `CPRStraightenedDecision`).
#[derive(Debug, Clone, PartialEq)]
pub struct CprCheck {
    pub accepted: bool,
    /// IsiX's wording: "valid centerline", "need at least 3 nodes", "curve too
    /// short", "degenerate spacing", "non-finite patient node", "self-intersecting
    /// loop" (accepted), "pixelsWide is zero".
    pub diagnosis: &'static str,
    pub length_mm: f64,
    pub sample_spacing: f64,
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    len3(sub3(a, b))
}

fn polyline_length(nodes: &[[f64; 3]]) -> f64 {
    nodes.windows(2).map(|w| dist(w[0], w[1])).sum()
}

/// IsiX `CPRStraightenedSession.evaluate(pixelsWide:)`.
pub fn check_path(nodes: &[[f64; 3]], pixels_wide: usize) -> CprCheck {
    let length = polyline_length(nodes);
    let decide = |accepted: bool, diagnosis: &'static str| CprCheck {
        accepted,
        diagnosis,
        length_mm: length,
        sample_spacing: if pixels_wide > 0 { length / pixels_wide as f64 } else { 0.0 },
    };
    if nodes.iter().any(|n| n.iter().any(|v| !v.is_finite())) {
        return decide(false, "non-finite patient node");
    }
    if nodes.len() < MINIMUM_NODES {
        return decide(false, "need at least 3 nodes");
    }
    if pixels_wide == 0 {
        return decide(false, "pixelsWide is zero");
    }
    if nodes.windows(2).any(|w| dist(w[0], w[1]) < DEGENERATE_SEGMENT) {
        return decide(false, "degenerate spacing");
    }
    if length < MINIMUM_LENGTH_MM {
        return decide(false, "curve too short");
    }
    if length / (pixels_wide as f64) < MINIMUM_SAMPLE_SPACING {
        return decide(false, "degenerate spacing");
    }
    // A crossing is reported, not refused (IsiX: refusing left the panel blank).
    if self_intersects(nodes) {
        return decide(true, "self-intersecting loop");
    }
    decide(true, "valid centerline")
}

fn self_intersects(nodes: &[[f64; 3]]) -> bool {
    if nodes.len() < 4 {
        return false;
    }
    if polyline_length(nodes) >= 10.0 && dist(nodes[0], nodes[nodes.len() - 1]) < LOOP_PROXIMITY_MM {
        return true;
    }
    let last = nodes.len() - 2;
    for i in 0..=last {
        for j in (i + 2)..=last {
            if interior_segment_distance(nodes[i], nodes[i + 1], nodes[j], nodes[j + 1]) < LOOP_PROXIMITY_MM {
                return true;
            }
        }
    }
    false
}

/// Distance between the interiors of two segments (IsiX `interiorSegmentDistance`):
/// infinite when the closest points lie at or near an end.
fn interior_segment_distance(a0: [f64; 3], a1: [f64; 3], b0: [f64; 3], b1: [f64; 3]) -> f64 {
    let u = sub3(a1, a0);
    let v = sub3(b1, b0);
    let w = sub3(a0, b0);
    let (uu, vv, uv, uw, vw) = (dot3(u, u), dot3(v, v), dot3(u, v), dot3(u, w), dot3(v, w));
    let denom = uu * vv - uv * uv;
    let (s, t) = if denom < 1e-18 {
        (0.0, if vv > 1e-18 { vw / vv } else { 0.0 })
    } else {
        ((uv * vw - vv * uw) / denom, (uu * vw - uv * uw) / denom)
    };
    if !(s > 0.02 && s < 0.98 && t > 0.02 && t < 0.98) {
        return f64::MAX;
    }
    dist(add3(a0, scale3(u, s)), add3(b0, scale3(v, t)))
}

/// A smooth path through every node (centripetal Catmull-Rom), as points about
/// `step_mm` apart along it.
pub fn smooth_path(nodes: &[[f64; 3]], step_mm: f64) -> Vec<[f64; 3]> {
    if nodes.len() < 2 {
        return nodes.to_vec();
    }
    let n = nodes.len();
    let at = |i: isize| -> [f64; 3] {
        if i < 0 {
            // Mirror the first segment so the curve starts heading at the 2nd node.
            sub3(scale3(nodes[0], 2.0), nodes[1])
        } else if i as usize >= n {
            sub3(scale3(nodes[n - 1], 2.0), nodes[n - 2])
        } else {
            nodes[i as usize]
        }
    };
    let mut out = vec![nodes[0]];
    for seg in 0..n - 1 {
        let (p0, p1, p2, p3) = (at(seg as isize - 1), at(seg as isize), at(seg as isize + 1), at(seg as isize + 2));
        // Centripetal parametrisation (alpha = 0.5): no cusps or self-loops.
        let knot = |a: [f64; 3], b: [f64; 3]| dist(a, b).sqrt().max(1e-6);
        let (t0, t1) = (0.0, knot(p0, p1));
        let t2 = t1 + knot(p1, p2);
        let t3 = t2 + knot(p2, p3);
        let lerp = |a: [f64; 3], b: [f64; 3], ta: f64, tb: f64, t: f64| {
            add3(scale3(a, (tb - t) / (tb - ta)), scale3(b, (t - ta) / (tb - ta)))
        };
        let pieces = ((dist(p1, p2) / step_mm).ceil() as usize * 4).clamp(4, 4000);
        for k in 1..=pieces {
            let t = t1 + (t2 - t1) * k as f64 / pieces as f64;
            let a1 = lerp(p0, p1, t0, t1, t);
            let a2 = lerp(p1, p2, t1, t2, t);
            let a3 = lerp(p2, p3, t2, t3, t);
            let b1 = lerp(a1, a2, t0, t2, t);
            let b2 = lerp(a2, a3, t1, t3, t);
            out.push(lerp(b1, b2, t1, t2, t));
        }
    }
    resample(&out, step_mm)
}

/// Points `step_mm` apart along the polyline `pts`, first and last included.
fn resample(pts: &[[f64; 3]], step_mm: f64) -> Vec<[f64; 3]> {
    let total = polyline_length(pts);
    let count = ((total / step_mm).round() as usize).max(1);
    let step = total / count as f64;
    let mut out = Vec::with_capacity(count + 1);
    out.push(pts[0]);
    let (mut seg, mut seg_start) = (0usize, 0.0f64);
    for k in 1..count {
        let target = k as f64 * step;
        while seg + 1 < pts.len() - 1 && seg_start + dist(pts[seg], pts[seg + 1]) < target {
            seg_start += dist(pts[seg], pts[seg + 1]);
            seg += 1;
        }
        let len = dist(pts[seg], pts[seg + 1]).max(1e-12);
        let f = ((target - seg_start) / len).clamp(0.0, 1.0);
        out.push(add3(pts[seg], scale3(sub3(pts[seg + 1], pts[seg]), f)));
    }
    out.push(pts[pts.len() - 1]);
    out
}

/// Turns `v` about the unit axis `axis` by `deg` degrees (Rodrigues).
fn rotate_about(v: [f64; 3], axis: [f64; 3], deg: f64) -> [f64; 3] {
    let (s, c) = deg.to_radians().sin_cos();
    add3(
        add3(scale3(v, c), scale3(cross3(axis, v), s)),
        scale3(axis, dot3(axis, v) * (1.0 - c)),
    )
}

/// A straightened image and how it maps back to the patient.
pub struct Straightened {
    /// Columns follow the curve; rows step across it; `spacing` mm per pixel both ways.
    pub frame: Arc<Frame>,
    pub length_mm: f64,
    pub spacing: f64,
    pub check: CprCheck,
}

/// Straightens `vol` along the path through `nodes` (patient LPS, mm), IsiX
/// `CPRStraightenedOperation`: one column per `spacing` step along the smoothed
/// curve; rows go across it along `initial_normal` (e.g. the normal of the plane
/// the path was drawn on) turned by `angle_deg` about the curve and kept at right
/// angles to it, `half_width_mm` to each side. Returns `None` when the path fails
/// the checks.
pub fn straighten(
    vol: &Volume3D,
    nodes: &[[f64; 3]],
    initial_normal: [f64; 3],
    angle_deg: f64,
    half_width_mm: f64,
) -> Option<Straightened> {
    let spacing = vol.spacing[0].min(vol.spacing[1]).min(vol.spacing[2]).clamp(0.3, 1.5);
    let length = polyline_length(nodes);
    let columns = ((length / spacing).round() as usize).max(1);
    let check = check_path(nodes, columns);
    if !check.accepted {
        return None;
    }
    let path = smooth_path(nodes, spacing);
    let width = path.len();
    let half_rows = (half_width_mm / spacing).round() as usize;
    let height = 2 * half_rows + 1;
    // Tangents by central differences; normals carried along the curve from the
    // initial direction, each made perpendicular to its tangent (no sudden flips).
    let tangent = |i: usize| {
        let a = path[i.saturating_sub(1)];
        let b = path[(i + 1).min(width - 1)];
        normalize3(sub3(b, a))
    };
    let mut normals = Vec::with_capacity(width);
    let mut prev = normalize3(initial_normal);
    for i in 0..width {
        let t = tangent(i);
        let mut n = sub3(prev, scale3(t, dot3(prev, t)));
        if len3(n) < 1e-6 {
            // The direction ran along the curve: take any perpendicular one.
            n = cross3(t, if t[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] });
        }
        n = normalize3(n);
        normals.push(normalize3(rotate_about(n, t, angle_deg)));
        prev = n;
    }
    let bg = vol.background_value();
    let mut pixels = vec![bg; width * height];
    for (col, (&p, &n)) in path.iter().zip(&normals).enumerate() {
        for row in 0..height {
            let offset = (row as f64 - half_rows as f64) * spacing;
            if let Some(v) = vol.sample_lps(add3(p, scale3(n, offset))) {
                pixels[row * width + col] = v;
            }
        }
    }
    let frame = Frame {
        width: width as u32,
        height: height as u32,
        pixels: Pixels::Gray(pixels),
        stored_window: vol.stored_window,
        full_window: vol.full_window,
        bits_window: vol.bits_window,
        inverts: vol.inverts,
        voi_lut_applied: false,
        slope: vol.slope,
        spacing: Some((spacing, spacing)),
        pixel_ratio: 1.0,
        origin: None,
        orientation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        is_3d_plane: false,
        frame_of_reference_uid: vol.frame_of_reference_uid.clone(),
        slice_thickness: None,
        slice_location: None,
        value_unit: vol.value_unit,
        suv_factor: vol.suv_factor,
    };
    Some(Straightened { frame: Arc::new(frame), length_mm: polyline_length(&path), spacing, check })
}

#[cfg(test)]
mod tests {
    use super::*;

    // IsiX CPRStraightenedSession gate.
    #[test]
    fn path_checks_match_isix() {
        let ok = [[0.0, 0.0, 0.0], [0.0, 0.0, 10.0], [0.0, 5.0, 20.0]];
        let c = check_path(&ok, 100);
        assert!(c.accepted);
        assert_eq!(c.diagnosis, "valid centerline");
        assert!((c.sample_spacing - c.length_mm / 100.0).abs() < 1e-12);
        assert_eq!(check_path(&ok[..2], 100).diagnosis, "need at least 3 nodes");
        assert_eq!(check_path(&ok, 0).diagnosis, "pixelsWide is zero");
        let short = [[0.0, 0.0, 0.0], [0.0, 0.0, 0.3], [0.0, 0.0, 0.6]];
        assert_eq!(check_path(&short, 10).diagnosis, "curve too short");
        let repeated = [[0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 10.0]];
        assert_eq!(check_path(&repeated, 10).diagnosis, "degenerate spacing");
        let nan = [[0.0, 0.0, f64::NAN], [0.0, 0.0, 1.0], [0.0, 0.0, 2.0]];
        assert_eq!(check_path(&nan, 10).diagnosis, "non-finite patient node");
        // A path crossing itself is reported but accepted.
        let crossing = [[0.0, 0.0, 0.0], [10.0, 10.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0]];
        let c = check_path(&crossing, 50);
        assert!(c.accepted);
        assert_eq!(c.diagnosis, "self-intersecting loop");
    }

    #[test]
    fn smooth_path_passes_through_the_nodes_at_even_steps() {
        let nodes = [[0.0, 0.0, 0.0], [20.0, 10.0, 0.0], [40.0, 0.0, 0.0], [60.0, -10.0, 5.0]];
        let path = smooth_path(&nodes, 0.5);
        for n in nodes {
            let nearest = path.iter().map(|p| dist(*p, n)).fold(f64::MAX, f64::min);
            assert!(nearest < 0.5, "node {n:?} is {nearest:.2} mm from the path");
        }
        let steps: Vec<f64> = path.windows(2).map(|w| dist(w[0], w[1])).collect();
        let (lo, hi) = steps.iter().fold((f64::MAX, 0.0f64), |(a, b), &s| (a.min(s), b.max(s)));
        assert!(lo > 0.4 && hi < 0.6, "steps {lo:.3}..{hi:.3}");
    }
}
