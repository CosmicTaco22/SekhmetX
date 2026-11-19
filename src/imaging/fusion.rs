//! PET-CT / multi-modality 2D fusion alpha ramps, blending factors, and
//! PET blending CLUT selection.
//!
//! Ported from IsiX (`ThalesMMS/IsiX-DICOM-Viewer-macOS`):
//! - `Modules/IsiXImaging/Sources/ViewGeometry/ViewTransform.swift` (`alphaTable`)
//! - `Modules/IsiXImaging/Sources/ViewerFusion/FusionBlend.swift`
//! - `Modules/IsiXImaging/Sources/ViewerFusion/FusionComparison.swift`
//! - `Modules/IsiXImaging/Sources/ViewerFusion/PETBlendingTable.swift`

use crate::imaging::clut::ColorLookupTable;

/// `value` limited to `0..=255` and truncated, matching IsiX `ViewTransform.clampedByte`.
fn clamped_byte(value: f32) -> u8 {
    if value.is_nan() || value <= 0.0 {
        return 0;
    }
    if value >= 255.0 {
        return 255;
    }
    value as u8
}

/// The 256 alpha values of a fused image for blending factor `alpha`
/// (`-256..=256`) and blending mode `mode`:
/// - `0`: Linear ramp
/// - `1`: High-Low-High (V shape)
/// - `2`: Low-High-Low (inverted V shape)
/// - `3`: Logarithmic
/// - `4`: Inverse logarithmic
/// - `5`: Flat (constant 128 base)
pub fn alpha_table(mode: i32, alpha: f32) -> [u8; 256] {
    let mut src = [0.0_f32; 256];
    match mode {
        0 => {
            for (i, s) in src.iter_mut().enumerate() {
                *s = i as f32;
            }
        }
        1 => {
            for (i, s) in src.iter_mut().enumerate().take(128) {
                *s = ((127 - i) * 2) as f32;
            }
            for (i, s) in src.iter_mut().enumerate().skip(128) {
                *s = ((i - 127) * 2) as f32;
            }
        }
        2 => {
            for (i, s) in src.iter_mut().enumerate().take(128) {
                *s = (i * 2) as f32;
            }
            for (i, s) in src.iter_mut().enumerate().skip(128) {
                *s = (256 - (i - 127) * 2) as f32;
            }
        }
        3 => {
            for (i, s) in src.iter_mut().enumerate() {
                *s = (255.0 * ((i as f64) / 255.0 * 9.0 + 1.0).log10()) as f32;
            }
        }
        4 => {
            for (i, s) in src.iter_mut().enumerate() {
                *s = (255.0 * (1.0 - (((255 - i) as f64) / 255.0 * 9.0 + 1.0).log10())) as f32;
            }
        }
        5 => {
            src.fill(128.0);
        }
        _ => {}
    }

    let mut table = [0_u8; 256];
    let mut a = alpha;
    if a <= 0.0 {
        a += 256.0;
        for i in 0..256 {
            table[i] = clamped_byte(((a * src[i]) as f64 / 256.0) as f32);
        }
    } else if a == 256.0 {
        table.fill(255);
    } else {
        for i in 0..256 {
            table[i] = clamped_byte(((256.0 * (src[i] as f64)) / ((256.0 - a) as f64)) as f32);
        }
    }
    table
}

/// The blend of a view's fused series (`FusionBlend.swift`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FusionBlend {
    /// The shape of the alpha ramp (`0..=5`):
    /// 0 linear, 1/2 V-shapes, 3/4 log/inverse-log, 5 flat.
    pub mode: i32,
    /// The blending factor (`-256.0..=256.0`, where `0.0` is 50%).
    pub factor: f32,
    /// The factor when a drag that changes it started.
    pub factor_at_drag_start: f32,
}

impl Default for FusionBlend {
    fn default() -> Self {
        Self {
            mode: 0,
            factor: 0.5,
            factor_at_drag_start: 0.0,
        }
    }
}

impl FusionBlend {
    /// The factor's bound on either side: `-setBlendingFactor:` keeps it within ±256.
    pub const FACTOR_LIMIT: f32 = 256.0;

    /// `factor` within `±256`; a value that is `NaN` stays as it is.
    pub fn bounded(factor: f32) -> f32 {
        factor.clamp(-Self::FACTOR_LIMIT, Self::FACTOR_LIMIT)
    }

    /// Sets the factor within `±256` and returns it.
    pub fn set_bounded_factor(&mut self, value: f32) -> f32 {
        self.factor = Self::bounded(value);
        self.factor
    }

    /// Marks where a drag of the factor starts.
    #[allow(dead_code)]
    pub fn begin_drag(&mut self) {
        self.factor_at_drag_start = self.factor;
    }

    /// The alpha table of the fused series for `alpha`, in this mode.
    pub fn alpha_table(&self, alpha: f32) -> [u8; 256] {
        alpha_table(self.mode, alpha)
    }

    /// Convenience: alpha table using the current bounded `self.factor`.
    pub fn current_alpha_table(&self) -> [u8; 256] {
        self.alpha_table(Self::bounded(self.factor))
    }

    /// Blending percentage `0.0..=1.0` corresponding to `self.factor` in `-256..=256`.
    pub fn blend_fraction(&self) -> f64 {
        FusionComparison::blend(
            self.factor as f64,
            -Self::FACTOR_LIMIT as f64,
            Self::FACTOR_LIMIT as f64,
        )
    }

    /// Sets `self.factor` from a `0.0..=1.0` blend fraction.
    pub fn set_blend_fraction(&mut self, frac: f64) {
        let v = FusionComparison::slider_value(
            frac.clamp(0.0, 1.0),
            -Self::FACTOR_LIMIT as f64,
            Self::FACTOR_LIMIT as f64,
        );
        self.set_bounded_factor(v as f32);
    }
}

/// Slider-to-blend mapping and patient-space box helpers (`FusionComparison.swift`).
pub struct FusionComparison;

impl FusionComparison {
    /// The fusion blend, `0..=1`, of `value` between `minimum` and `maximum`
    /// (`-256..=256` shown as `0..=100%`); `0.5` when the slider has no range.
    pub fn blend(value: f64, minimum: f64, maximum: f64) -> f64 {
        if maximum > minimum {
            (value - minimum) / (maximum - minimum)
        } else {
            0.5
        }
    }

    /// The slider value of a blend `stop` (`0..=1`) between `minimum` and `maximum`.
    pub fn slider_value(stop: f64, minimum: f64, maximum: f64) -> f64 {
        minimum + stop * (maximum - minimum)
    }

    /// Manual offset in millimetres: non-finite numbers count as `0`.
    #[allow(dead_code)]
    pub fn offset(values: &[f64]) -> Vec<f64> {
        values.iter().map(|&v| if v.is_finite() { v } else { 0.0 }).collect()
    }

    /// Axis-aligned box in patient millimetres around `points`.
    #[allow(dead_code)]
    pub fn bounds(points: &[[f32; 3]]) -> ([f64; 3], [f64; 3]) {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for p in points {
            for axis in 0..3 {
                let v = p[axis] as f64;
                lo[axis] = if lo[axis] < v { lo[axis] } else { v };
                hi[axis] = if hi[axis] < v { v } else { hi[axis] };
            }
        }
        (lo, hi)
    }
}

/// The PET blending table of a fused series (`PETBlendingTable.swift`).
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PetBlendingTable {
    pub colours: ColorLookupTable,
}

#[allow(dead_code)]
impl PetBlendingTable {
    /// The PET CLUT mode (`"PET Clut Mode"`) in which a fused series is drawn
    /// with this table rather than with its own.
    pub const FUSED_SERIES_MODE: &'static str = "B/W Inverse";

    pub fn new(colours: ColorLookupTable) -> Self {
        Self { colours }
    }

    /// Whether a fused series is drawn with the PET blending table in `mode`.
    pub fn draws_fused_series(pet_clut_mode: Option<&str>) -> bool {
        pet_clut_mode == Some(Self::FUSED_SERIES_MODE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fusion_blend_factor_bounds() {
        for value in [
            -1000.0_f32,
            -256.5,
            -256.0,
            -12.25,
            0.0,
            0.5,
            255.9,
            256.0,
            256.1,
            1e9,
            f32::NEG_INFINITY,
            f32::INFINITY,
        ] {
            let expected = value.clamp(-256.0, 256.0);
            assert_eq!(FusionBlend::bounded(value), expected);
            let mut blend = FusionBlend::default();
            assert_eq!(blend.set_bounded_factor(value), expected);
            assert_eq!(blend.factor, expected);
        }
        assert!(FusionBlend::bounded(f32::NAN).is_nan());
    }

    #[test]
    fn fusion_blend_starts_and_drags_as_view_did() {
        let blend = FusionBlend::default();
        assert_eq!(blend.mode, 0);
        assert_eq!(blend.factor, 0.5);
        assert_eq!(blend.factor_at_drag_start, 0.0);

        let mut b = FusionBlend {
            mode: 2,
            factor: 40.0,
            factor_at_drag_start: 0.0,
        };
        b.begin_drag();
        b.factor = 90.0;
        assert_eq!(b.factor_at_drag_start, 40.0);
        assert_eq!(b.factor, 90.0);
    }

    #[test]
    fn fusion_blend_alpha_tables_match_modes() {
        // Mode 0 (linear) at alpha = 0 -> identity ramp 0..=255
        let t0 = alpha_table(0, 0.0);
        assert_eq!(t0[0], 0);
        assert_eq!(t0[128], 128);
        assert_eq!(t0[255], 255);

        // At alpha = -256 -> all zeros
        let t_min = alpha_table(0, -256.0);
        assert!(t_min.iter().all(|&a| a == 0));

        // At alpha = 256 -> all 255
        let t_max = alpha_table(0, 256.0);
        assert!(t_max.iter().all(|&a| a == 255));

        // Mode 5 (flat) at alpha = 0 -> constant 128
        let t_flat = alpha_table(5, 0.0);
        assert!(t_flat.iter().all(|&a| a == 128));

        // Mode 3 (log) rises faster than linear at midpoint
        let t_log = alpha_table(3, 0.0);
        assert!(t_log[64] > 64);
        assert_eq!(t_log[0], 0);
        assert_eq!(t_log[255], 255);
    }

    #[test]
    fn fusion_comparison_slider_and_bounds() {
        assert_eq!(FusionComparison::blend(0.0, -256.0, 256.0), 0.5);
        assert_eq!(FusionComparison::blend(256.0, -256.0, 256.0), 1.0);
        assert_eq!(FusionComparison::blend(7.0, 3.0, 3.0), 0.5);
        assert_eq!(FusionComparison::slider_value(0.0, -256.0, 256.0), -256.0);
        assert_eq!(FusionComparison::slider_value(1.0, -256.0, 256.0), 256.0);
        for stop in [0.0, 0.25, 0.5, 1.0] {
            let v = FusionComparison::slider_value(stop, -256.0, 256.0);
            assert_eq!(FusionComparison::blend(v, -256.0, 256.0), stop);
        }

        assert_eq!(
            FusionComparison::offset(&[1.5, f64::NAN, f64::NEG_INFINITY]),
            vec![1.5, 0.0, 0.0]
        );

        let (lo, hi) = FusionComparison::bounds(&[[1.0, -2.0, 3.0], [-4.0, 5.0, 0.5]]);
        assert_eq!(lo, [-4.0, -2.0, 0.5]);
        assert_eq!(hi, [1.0, 5.0, 3.0]);

        let (elo, ehi) = FusionComparison::bounds(&[]);
        assert!(elo.iter().all(|&v| v == f64::INFINITY));
        assert!(ehi.iter().all(|&v| v == f64::NEG_INFINITY));
    }

    #[test]
    fn pet_blending_table_bw_inverse_only() {
        assert!(PetBlendingTable::draws_fused_series(Some("B/W Inverse")));
        for mode in [None, Some(""), Some("PET"), Some("b/w inverse"), Some("B/W Inverse ")] {
            assert!(!PetBlendingTable::draws_fused_series(mode));
        }
    }
}
