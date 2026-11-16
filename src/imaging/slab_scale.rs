//! The scale of a slab-thickness slider: where the knob stands for a
//! thickness and the thicknesses it stops at.
//!
//! Follows IsiX `Modules/IsiXImaging/Sources/ViewerReconstruction/ThickSlabSliderScale.swift`
//! (IsiX 6.0.4). The tick marks along the track (`tickPositions`) are not
//! ported: the slider here has none.

/// A slider scale between `minimum` and `maximum`, linear or exponential.
///
/// The knob's unit interval is separate from the thickness the renderer uses.
/// The exponential curve needs a positive minimum; incomplete bounds fall back
/// to a finite linear or constant range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlabSliderScale {
    pub minimum: f64,
    pub maximum: f64,
    pub exponential: bool,
}

impl SlabSliderScale {
    pub fn new(minimum: f64, maximum: f64, exponential: bool) -> Self {
        let minimum = if minimum.is_finite() { minimum } else { 0.0 };
        let maximum = if maximum.is_finite() { maximum.max(minimum) } else { minimum };
        Self {
            minimum,
            maximum,
            exponential: exponential && minimum > 0.0 && maximum > minimum,
        }
    }

    /// The thick end of a reconstruction's slab, in millimetres: the whole
    /// volume, as the longest side rounded up to a whole millimetre. A volume
    /// whose sides are not known keeps `fallback`.
    #[allow(dead_code)] // ported with the scale; unused while the thick end is fixed (DEVIATIONS.md)
    pub fn volume_maximum(sides: &[f64], fallback: f64) -> f64 {
        let longest = sides
            .iter()
            .copied()
            .filter(|s| s.is_finite() && *s > 0.0)
            .fold(0.0_f64, f64::max);
        if longest > 0.0 { longest.ceil() } else { fallback }
    }

    /// The thickness for a knob position in 0…1.
    pub fn thickness(&self, position: f64) -> f64 {
        if self.maximum <= self.minimum || position.is_nan() || position <= 0.0 {
            return self.minimum;
        }
        if position >= 1.0 {
            return self.maximum;
        }
        if self.exponential {
            let (lo, hi) = (self.minimum.ln(), self.maximum.ln());
            return (lo + position * (hi - lo)).exp().clamp(self.minimum, self.maximum);
        }
        self.minimum + position * (self.maximum - self.minimum)
    }

    /// The knob position in 0…1 for a thickness.
    pub fn position(&self, thickness: f64) -> f64 {
        if self.maximum <= self.minimum || thickness.is_nan() || thickness <= self.minimum {
            return 0.0;
        }
        if thickness >= self.maximum {
            return 1.0;
        }
        if self.exponential {
            let (lo, hi) = (self.minimum.ln(), self.maximum.ln());
            return (thickness.ln() - lo) / (hi - lo);
        }
        (thickness - self.minimum) / (self.maximum - self.minimum)
    }

    /// The thickness the knob stops at for `position`: a whole unit, with the
    /// scale's own ends kept as they are, so the thin end is still the slice
    /// interval. The curve alone gave 1.03, 1.57 or 2.61 mm.
    pub fn whole_thickness(&self, position: f64) -> f64 {
        let raw = self.thickness(position);
        if raw <= self.minimum || raw >= self.maximum {
            return raw;
        }
        raw.round().clamp(self.minimum, self.maximum)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // IsiX: theThickEndIsTheWholeVolume
    #[test]
    fn the_thick_end_is_the_whole_volume() {
        // 512 x 512 pixels of 0.7 mm and 695 slices every 0.5 mm.
        assert_eq!(SlabSliderScale::volume_maximum(&[358.4, 358.4, 347.5], 100.0), 359.0);
        // A long scan is longest along its slices.
        assert_eq!(SlabSliderScale::volume_maximum(&[350.0, 350.0, 1820.25], 100.0), 1821.0);
        // A small volume has a short ruler; one of unknown size keeps the ruler it had.
        assert_eq!(SlabSliderScale::volume_maximum(&[60.2, 60.2, 12.0], 100.0), 61.0);
        assert_eq!(SlabSliderScale::volume_maximum(&[f64::NAN, 0.0, -3.0, f64::INFINITY], 100.0), 100.0);
        assert_eq!(SlabSliderScale::volume_maximum(&[], 100.0), 100.0);
    }

    // IsiX: aScaleWithoutARangeHasNoRuler
    #[test]
    fn a_scale_without_a_range_is_constant() {
        for (minimum, maximum) in [(0.0, 0.0), (3.0, 2.0), (f64::NAN, f64::INFINITY)] {
            let scale = SlabSliderScale::new(minimum, maximum, true);
            assert!(scale.thickness(0.5).is_finite());
            assert_eq!(scale.position(1.0), 0.0);
        }
    }

    // IsiX: theKnobStopsAtWholeUnitsBetweenItsEnds
    #[test]
    fn the_knob_stops_at_whole_units_between_its_ends() {
        for exponential in [false, true] {
            let scale = SlabSliderScale::new(0.6, 347.3, exponential);
            assert_eq!(scale.whole_thickness(0.0), 0.6);
            assert_eq!(scale.whole_thickness(1.0), 347.3);
            let mut last = 0.6;
            for step in 1..1000 {
                let position = step as f64 / 1000.0;
                let stop = scale.whole_thickness(position);
                assert!(stop == 0.6 || stop == 347.3 || stop == stop.round());
                assert!(stop >= last && (stop - scale.thickness(position)).abs() <= 0.5 + 1e-9);
                last = stop;
            }
        }
        assert_eq!(SlabSliderScale::new(0.3, 100.0, true).whole_thickness(0.01), 0.3);
    }

    // IsiX: anExponentialRulerMarksTheStopsNearestToEqualSharesOfTheTrack (the stops themselves)
    #[test]
    fn an_exponential_scale_stops_at_equal_ratios() {
        let scale = SlabSliderScale::new(1.0, 350.0, true);
        let stops: Vec<f64> = (0..=10).map(|i| scale.whole_thickness(i as f64 / 10.0)).collect();
        assert_eq!(stops, [1.0, 2.0, 3.0, 6.0, 10.0, 19.0, 34.0, 60.0, 108.0, 195.0, 350.0]);
    }

    #[test]
    fn position_and_thickness_are_inverse() {
        for exponential in [false, true] {
            let scale = SlabSliderScale::new(0.7, 50.0, exponential);
            for t in [0.7, 1.0, 2.5, 10.0, 33.0, 50.0] {
                assert!((scale.thickness(scale.position(t)) - t).abs() < 1e-9);
            }
        }
    }
}
