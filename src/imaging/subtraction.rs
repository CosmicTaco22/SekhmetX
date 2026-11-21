//! Digital image subtraction (`post - mask`) for DSA and dynamic contrast-enhanced MR/CT.

use crate::frame::{Frame, Pixels};
use crate::imaging::Window;

/// Subtracts `mask` from `post` pixel-by-pixel for matching grayscale frames.
/// Returns `None` if dimensions differ or either frame is RGB.
pub fn subtract_frames(post: &Frame, mask: &Frame) -> Option<Frame> {
    if post.width != mask.width || post.height != mask.height {
        return None;
    }
    let (Pixels::Gray(p), Pixels::Gray(m)) = (&post.pixels, &mask.pixels) else {
        return None;
    };
    let mut diff = Vec::with_capacity(p.len());
    let mut lo = f32::INFINITY;
    let mut hi = f32::NEG_INFINITY;
    for (&pv, &mv) in p.iter().zip(m.iter()) {
        let d = pv - mv;
        if d.is_finite() {
            lo = lo.min(d);
            hi = hi.max(d);
        }
        diff.push(d);
    }
    if !lo.is_finite() || !hi.is_finite() {
        lo = 0.0;
        hi = 1.0;
    }
    let width = (hi - lo).max(1.0);
    let level = lo + width * 0.5;
    let win = Window::new(level, width);
    Some(Frame {
        width: post.width,
        height: post.height,
        pixels: Pixels::Gray(diff),
        stored_window: Some(win),
        full_window: win,
        bits_window: None,
        inverts: false,
        voi_lut_applied: false,
        slope: 1.0,
        spacing: post.spacing,
        pixel_ratio: post.pixel_ratio,
        origin: post.origin,
        orientation: post.orientation,
        is_3d_plane: post.is_3d_plane,
        frame_of_reference_uid: post.frame_of_reference_uid.clone(),
        slice_thickness: post.slice_thickness,
        slice_location: post.slice_location,
        value_unit: "Δ",
        suv_factor: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_frame(vals: Vec<f32>) -> Frame {
        Frame {
            width: 2,
            height: 2,
            pixels: Pixels::Gray(vals),
            stored_window: None,
            full_window: Window::new(50.0, 100.0),
            bits_window: None,
            inverts: false,
            voi_lut_applied: false,
            slope: 1.0,
            spacing: Some((1.0, 1.0)),
            pixel_ratio: 1.0,
            origin: Some([0.0, 0.0, 0.0]),
            orientation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            is_3d_plane: true,
            frame_of_reference_uid: None,
            slice_thickness: Some(1.0),
            slice_location: Some(0.0),
            value_unit: "HU",
            suv_factor: None,
        }
    }

    #[test]
    fn subtract_frames_computes_exact_difference_and_window() {
        let post = make_frame(vec![120.0, 200.0, 50.0, 80.0]);
        let mask = make_frame(vec![100.0, 100.0, 50.0, 90.0]);
        let sub = subtract_frames(&post, &mask).unwrap();
        let Pixels::Gray(vals) = &sub.pixels else {
            panic!("expected gray");
        };
        assert_eq!(vals, &[20.0, 100.0, 0.0, -10.0]);
        assert!((sub.full_window.width - 110.0).abs() < 1e-3);
        assert!((sub.full_window.level - 45.0).abs() < 1e-3);
    }
}
