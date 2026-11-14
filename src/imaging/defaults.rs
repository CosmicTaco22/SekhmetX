//! The window an image opens with, before anyone sets one.
//!
//! Follows IsiX `Display/DisplayWindowRange.swift` and `PreviewWindowing.swift`:
//! a series takes its defaults from its middle image and keeps them for every
//! slice, so an edge slice (mostly air, with its own stored window) doesn't
//! make the series flicker.

use super::Window;

/// Whether the file's Window Center/Width can be used: a width of zero is the
/// file saying nothing.
pub fn is_usable_stored_window(w: Window) -> bool {
    !(w.width == 0.0 || !w.width.is_finite() || !w.level.is_finite())
}

/// Whether the frame's own range can be used.
pub fn is_usable_frame_range(w: Window) -> bool {
    !(w.width <= 0.0 || !w.width.is_finite() || !w.level.is_finite())
}

/// What the stored bits can hold, through the rescale: the last-resort window.
pub fn stored_range(bits_stored: i32, bits_allocated: i32, signed: bool, slope: f32, intercept: f32) -> Option<Window> {
    let mut bits = bits_stored;
    if !(1..=30).contains(&bits) {
        bits = if bits_allocated > 0 && bits_allocated <= 30 { bits_allocated } else { 16 };
    }
    let (min, max) = if signed {
        let magnitude = (1i64 << (bits - 1)) as f32;
        (-magnitude, magnitude - 1.0)
    } else {
        (0.0, ((1i64 << bits) - 1) as f32)
    };
    // Horos's C fused these multiplications into the additions.
    let first = min.mul_add(slope, intercept);
    let second = max.mul_add(slope, intercept);
    let (low, high) = if first < second { (first, second) } else { (second, first) };
    let mut range = high - low;
    if range < 1.0 {
        range = 1.0;
    }
    if !range.is_finite() || !low.is_finite() {
        return None;
    }
    Some(Window::new(low + range / 2.0, range))
}

/// The image a series' default window is read from (`imageCount / 2`), or
/// for a single multi-frame file, its middle frame.
pub fn reference_index(image_count: usize) -> Option<usize> {
    (image_count > 0).then_some(image_count / 2)
}

#[allow(dead_code)] // used by the browser preview (P1, browser)
/// Whether a window is one actually on screen, rather than a request for the
/// automatic window (zero width, or not a finite number).
pub fn is_window_on_screen(w: Window) -> bool {
    w.width > 0.0 && w.width.is_finite() && w.level.is_finite()
}

/// The default window of an image, in Horos's order: the file's stored window
/// (or a VOI LUT's output range), else the frame's own minimum to maximum,
/// else what the stored bits can hold.
pub fn default_window(stored: Option<Window>, frame_range: Option<Window>, bits: Option<Window>) -> Option<Window> {
    stored
        .filter(|&w| is_usable_stored_window(w))
        .or(frame_range.filter(|&w| is_usable_frame_range(w)))
        .or(bits)
}

#[cfg(test)]
mod tests {
    use super::*;

    // IsiX DisplayWindowRangeTests.storedBitsGiveTheLastResortWindow
    #[test]
    fn stored_bits_give_the_last_resort_window() {
        assert_eq!(stored_range(12, 16, false, 1.0, -1024.0), Some(Window::new(1023.5, 4095.0)));
        assert_eq!(stored_range(1, 8, false, 0.1, 0.0).unwrap().width, 1.0);
        assert_eq!(stored_range(12, 16, true, f32::NAN, 0.0), None);
        assert!(!is_usable_stored_window(Window::new(40.0, 0.0)));
        assert!(!is_usable_frame_range(Window::new(40.0, -1.0)));
    }

    // IsiX PreviewWindowingTests.aZeroOrNonFiniteWidthAsksForTheAutomaticWindow
    #[test]
    fn a_zero_or_non_finite_width_asks_for_the_automatic_window() {
        assert!(is_window_on_screen(Window::new(40.0, 400.0)));
        assert!(is_window_on_screen(Window::new(-1000.0, 0.5)));
        for w in [(40.0, 0.0), (40.0, -10.0), (40.0, f32::INFINITY), (40.0, f32::NAN), (f32::NAN, 400.0), (f32::NEG_INFINITY, 400.0)] {
            assert!(!is_window_on_screen(Window::new(w.0, w.1)), "{w:?}");
        }
    }

    // IsiX PreviewWindowingTests.theDefaultsOfASeriesComeFromItsMiddleImage
    #[test]
    fn the_defaults_of_a_series_come_from_its_middle_image() {
        assert_eq!(reference_index(9), Some(4));
        assert_eq!(reference_index(2), Some(1));
        assert_eq!(reference_index(30), Some(15));
        assert_eq!(reference_index(0), None);
    }

    #[test]
    fn the_stored_window_wins_then_the_frame_then_the_bits() {
        let stored = Some(Window::new(40.0, 400.0));
        let frame = Some(Window::new(0.0, 100.0));
        let bits = Some(Window::new(2047.5, 4095.0));
        assert_eq!(default_window(stored, frame, bits), stored);
        assert_eq!(default_window(Some(Window::new(40.0, 0.0)), frame, bits), frame);
        assert_eq!(default_window(None, Some(Window::new(0.0, 0.0)), bits), bits);
    }
}
