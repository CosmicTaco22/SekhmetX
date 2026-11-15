//! Windowing an image to 8-bit grey.
//!
//! Follows IsiX `BufferOperations/ImageWindow.swift` (what a requested window
//! becomes, the full window) and `BufferOperations/WindowRendering.swift`
//! (linear grey, colour table, invert).

use rayon::prelude::*;

use super::clut::ColorLookupTable;
use super::Window;

/// What a request for a level and width becomes, or `None` when it asks for
/// the full window (width 0, or a value that isn't a finite number).
///
/// On an image whose full width is over 256 the window is in whole values: a
/// width under one becomes two, and a window that doesn't start at zero is
/// truncated to integers. No width is under a thousandth of the rescale slope.
pub fn requested(level: f32, width: f32, full_width: f32, slope: f32) -> Option<Window> {
    if width == 0.0 || !width.is_finite() || !level.is_finite() {
        return None;
    }
    let (mut level, mut width) = (level, width);
    if full_width > 256.0 {
        if width < 1.0 {
            width = 2.0;
        }
        if level - width / 2.0 == 0.0 {
            level = width / 2.0;
        } else {
            // Rust's `as` truncates, saturates and maps NaN to 0, exactly the
            // arm64 conversion IsiX reproduces with `cInt`.
            width = (width as i32) as f32;
            level = (level as i32) as f32;
        }
    }
    let floor = 0.001 * slope as f64;
    if (width as f64) < floor {
        width = floor as f32;
    }
    Some(Window { level, width })
}

/// The window that shows every value: smallest to largest, twenty wide when
/// the image is flat; 0–255 for colour.
pub fn full(values: &[f32], is_rgb: bool) -> Window {
    let (low, mut high) = if is_rgb {
        (0.0, 255.0)
    } else {
        let (lo, hi) = values
            .par_chunks(65_536)
            .map(|c| c.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &v| (lo.min(v), hi.max(v))))
            .reduce(|| (f32::INFINITY, f32::NEG_INFINITY), |a, b| (a.0.min(b.0), a.1.max(b.1)));
        if lo.is_finite() { (lo, hi) } else { (0.0, 0.0) }
    };
    if !is_rgb && low == high {
        high = low + 20.0;
    }
    Window { level: low + (high - low) / 2.0, width: high - low }
}

/// Grey through a straight window, written into RGBA: level − width/2 is
/// black, level + width/2 white (Horos's `vImageConvert_PlanarFtoPlanar8`).
///
/// Apple doesn't document that routine's exact rounding; this rounds to the
/// nearest grey, which can differ from Horos by at most one level of 255.
pub fn linear_rgba(values: &[f32], out: &mut [u8], window: Window, invert: bool) {
    let low = window.low();
    let span = window.high() - low;
    let scale = if span > 0.0 { 255.0 / span } else { 0.0 };
    out.par_chunks_mut(4 * 4096)
        .zip(values.par_chunks(4096))
        .for_each(|(dst, src)| {
            for (px, &v) in dst.as_chunks_mut::<4>().0.iter_mut().zip(src) {
                let g = if span <= 0.0 {
                    if v >= window.high() { 255 } else { 0 }
                } else if v <= low {
                    0
                } else if v >= window.high() {
                    255
                } else {
                    ((v - low) * scale + 0.5) as u8
                };
                let g = if invert { 255 - g } else { g };
                px[0] = g;
                px[1] = g;
                px[2] = g;
                px[3] = 255;
            }
        });
}

/// Grey through a straight window, mapped through an optional 256-entry RGB
/// colour lookup table (`clut`), written into RGBA.
pub fn linear_rgba_with_clut(
    values: &[f32],
    out: &mut [u8],
    window: Window,
    invert: bool,
    clut: Option<&ColorLookupTable>,
) {
    let Some(clut) = clut else {
        linear_rgba(values, out, window, invert);
        return;
    };
    let low = window.low();
    let span = window.high() - low;
    let scale = if span > 0.0 { 255.0 / span } else { 0.0 };
    out.par_chunks_mut(4 * 4096)
        .zip(values.par_chunks(4096))
        .for_each(|(dst, src)| {
            for (px, &v) in dst.as_chunks_mut::<4>().0.iter_mut().zip(src) {
                let g = if span <= 0.0 {
                    if v >= window.high() { 255 } else { 0 }
                } else if v <= low {
                    0
                } else if v >= window.high() {
                    255
                } else {
                    ((v - low) * scale + 0.5) as u8
                };
                let idx = (if invert { 255 - g } else { g }) as usize;
                px[0] = clut.red[idx];
                px[1] = clut.green[idx];
                px[2] = clut.blue[idx];
                px[3] = 255;
            }
        });
}

/// The lookup table that windows each channel of a colour image (Horos
/// `colourTable` without a transfer function).
pub fn colour_table(window: Window) -> [u8; 256] {
    let low = window.low();
    let high = window.high();
    // A colour frame has no scalar intensity to window, so a zero-width
    // window shows the frame as it is rather than dividing by zero.
    let mut difference = (high - low) as i64;
    if difference == 0 {
        difference = 1;
    }
    let mut table = [0u8; 256];
    for (index, entry) in table.iter_mut().enumerate() {
        let value = ((index as f32 - low) * 255.0 / difference as f32) as i64;
        *entry = value.clamp(0, 255) as u8;
    }
    table
}

/// Applies a colour table (and optional inversion) to RGBA pixels.
pub fn colour_rgba(pixels: &[u8], out: &mut [u8], table: &[u8; 256], invert: bool) {
    out.par_chunks_mut(4 * 4096)
        .zip(pixels.par_chunks(4 * 4096))
        .for_each(|(dst, src)| {
            for (d, s) in dst.as_chunks_mut::<4>().0.iter_mut().zip(src.as_chunks::<4>().0.iter()) {
                for c in 0..3 {
                    let v = table[s[c] as usize];
                    d[c] = if invert { 255 - v } else { v };
                }
                d[3] = s[3];
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    // IsiX BufferOperationsTests.aRequestOfZeroWidthOrNotANumberIsTheFullWindow
    #[test]
    fn a_request_of_zero_width_or_not_a_number_is_the_full_window() {
        assert_eq!(requested(40.0, 0.0, 1000.0, 1.0), None);
        assert_eq!(requested(f32::NAN, 400.0, 1000.0, 1.0), None);
        assert_eq!(requested(40.0, f32::INFINITY, 1000.0, 1.0), None);
    }

    // IsiX BufferOperationsTests.aWideImageIsWindowedInWholeValues
    #[test]
    fn a_wide_image_is_windowed_in_whole_values() {
        assert_eq!(requested(100.6, 300.3, 1000.0, 1.0), Some(Window::new(100.0, 300.0)));
        assert_eq!(requested(150.0, 300.0, 1000.0, 1.0), Some(Window::new(150.0, 300.0)));
        assert_eq!(requested(3.3, 1.7, 100.0, 1.0), Some(Window::new(3.3, 1.7)));
        assert_eq!(requested(5.0, 0.0001, 100.0, 3.0).unwrap().width, (0.001f64 * 3.0) as f32);
    }

    // IsiX BufferOperationsTests.theFullWindowSpansTheValuesAndAFlatImageGetsTwenty
    #[test]
    fn the_full_window_spans_the_values_and_a_flat_image_gets_twenty() {
        let values = [-10.0, 30.0, 5.0];
        assert_eq!(full(&values, false), Window::new(10.0, 40.0));
        assert_eq!(full(&[42.0; 4], false), Window::new(52.0, 20.0));
        assert_eq!(full(&values, true), Window::new(127.5, 255.0));
    }

    // IsiX BufferOperationsTests.aColourWindowOfZeroWidthLeavesTheFrameAsItIsAboveTheLevel
    #[test]
    fn a_colour_window_of_zero_width_leaves_the_frame_as_it_is_above_the_level() {
        let table = colour_table(Window::new(128.0, 0.0));
        assert_eq!((table[127], table[128], table[129]), (0, 0, 255));
        let identity = colour_table(Window::new(127.5, 255.0));
        assert!(identity.iter().enumerate().all(|(i, &v)| v as usize == i));
    }

    // IsiX BufferOperationsTests.invertingTurnsBlackToWhite (inversion is part of the window here)
    #[test]
    fn inverting_turns_black_to_white() {
        let values = [0.0, 255.0];
        let mut out = [0u8; 8];
        linear_rgba(&values, &mut out, Window::new(127.5, 255.0), true);
        assert_eq!((out[0], out[4]), (255, 0));
    }

    #[test]
    fn the_linear_window_maps_its_ends_to_black_and_white() {
        // CT abdomen window 40/350: −135 black, 215 white, 40 mid-grey.
        let values = [-1000.0, -135.0, 40.0, 215.0, 3000.0];
        let mut out = [0u8; 20];
        linear_rgba(&values, &mut out, Window::new(40.0, 350.0), false);
        let grey: Vec<u8> = out.chunks(4).map(|p| p[0]).collect();
        assert_eq!(grey, vec![0, 0, 128, 255, 255]);
        assert!(out.chunks(4).all(|p| p[3] == 255 && p[0] == p[1] && p[1] == p[2]));
    }
}
