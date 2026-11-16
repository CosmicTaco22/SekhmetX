//! 1D Transfer Function LUTs & OsiriX-style 3D Volume Rendering presets.

use super::clut::ColorLookupTable;
use super::Window;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum VrPreset {
    #[default]
    Default,
    Bone,
    Vascular,
    SoftTissue,
    Lung,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TfStop {
    pub t: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

const fn stop(t: f32, r: f32, g: f32, b: f32, a: f32) -> TfStop {
    TfStop { t, r, g, b, a }
}

impl VrPreset {
    pub const ALL: [Self; 5] = [
        Self::Default,
        Self::Bone,
        Self::Vascular,
        Self::SoftTissue,
        Self::Lung,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::Bone => "Bone",
            Self::Vascular => "Angio / Vascular",
            Self::SoftTissue => "Soft Tissue",
            Self::Lung => "Lung / Airway",
        }
    }

    pub const fn default_window(self) -> Window {
        match self {
            Self::Default => Window::new(300.0, 1200.0),
            Self::Bone => Window::new(500.0, 1400.0),
            Self::Vascular => Window::new(220.0, 520.0),
            Self::SoftTissue => Window::new(60.0, 380.0),
            Self::Lung => Window::new(-550.0, 950.0),
        }
    }

    fn stops(self) -> &'static [TfStop] {
        const DEFAULT_STOPS: &[TfStop] = &[
            stop(0.00, 0.00, 0.00, 0.00, 0.00),
            stop(0.18, 0.45, 0.18, 0.10, 0.00),
            stop(0.32, 0.78, 0.38, 0.20, 0.08),
            stop(0.55, 0.92, 0.65, 0.42, 0.25),
            stop(0.78, 0.98, 0.94, 0.84, 0.72),
            stop(1.00, 1.00, 1.00, 0.97, 0.95),
        ];
        const BONE_STOPS: &[TfStop] = &[
            stop(0.00, 0.00, 0.00, 0.00, 0.00),
            stop(0.28, 0.60, 0.48, 0.36, 0.00),
            stop(0.42, 0.85, 0.75, 0.60, 0.14),
            stop(0.65, 0.96, 0.92, 0.84, 0.68),
            stop(0.88, 1.00, 0.99, 0.95, 0.94),
            stop(1.00, 1.00, 1.00, 1.00, 1.00),
        ];
        const VASCULAR_STOPS: &[TfStop] = &[
            stop(0.00, 0.00, 0.00, 0.00, 0.00),
            stop(0.22, 0.45, 0.06, 0.04, 0.00),
            stop(0.38, 0.85, 0.18, 0.08, 0.18),
            stop(0.58, 0.98, 0.48, 0.16, 0.52),
            stop(0.80, 1.00, 0.88, 0.68, 0.85),
            stop(1.00, 1.00, 0.98, 0.95, 0.98),
        ];
        const SOFT_TISSUE_STOPS: &[TfStop] = &[
            stop(0.00, 0.00, 0.00, 0.00, 0.00),
            stop(0.15, 0.55, 0.22, 0.18, 0.00),
            stop(0.35, 0.82, 0.36, 0.28, 0.16),
            stop(0.60, 0.92, 0.58, 0.48, 0.34),
            stop(0.85, 0.96, 0.86, 0.78, 0.58),
            stop(1.00, 1.00, 0.96, 0.92, 0.82),
        ];
        const LUNG_STOPS: &[TfStop] = &[
            stop(0.00, 0.05, 0.12, 0.24, 0.00),
            stop(0.20, 0.18, 0.46, 0.72, 0.05),
            stop(0.45, 0.45, 0.74, 0.88, 0.18),
            stop(0.70, 0.88, 0.82, 0.72, 0.48),
            stop(0.90, 0.98, 0.96, 0.92, 0.82),
            stop(1.00, 1.00, 1.00, 1.00, 0.95),
        ];
        match self {
            Self::Default => DEFAULT_STOPS,
            Self::Bone => BONE_STOPS,
            Self::Vascular => VASCULAR_STOPS,
            Self::SoftTissue => SOFT_TISSUE_STOPS,
            Self::Lung => LUNG_STOPS,
        }
    }

    /// Returns an editable copy of this preset's control points.
    pub fn default_stops(self) -> Vec<TfStop> {
        self.stops().to_vec()
    }
}

/// Evaluates a 256-entry RGBA8 1D Transfer Function LUT from sorted control points.
/// If `clut` is `Some`, its RGB overrides the stop colors while preserving the opacity curve.
pub fn build_lut_from_stops(stops: &[TfStop], clut: Option<&ColorLookupTable>) -> [u8; 256 * 4] {
    if stops.is_empty() {
        return [0u8; 256 * 4];
    }
    let mut out = [0u8; 256 * 4];
    for i in 0..256 {
        let t = i as f32 / 255.0;
        let mut s0 = stops[0];
        let mut s1 = stops[stops.len() - 1];
        if t <= s0.t {
            s1 = s0;
        } else if t >= s1.t {
            s0 = s1;
        } else {
            for w in stops.windows(2) {
                if t >= w[0].t && t <= w[1].t {
                    s0 = w[0];
                    s1 = w[1];
                    break;
                }
            }
        }
        let span = (s1.t - s0.t).max(1e-5);
        let f = ((t - s0.t) / span).clamp(0.0, 1.0);
        let mut r = s0.r + (s1.r - s0.r) * f;
        let mut g = s0.g + (s1.g - s0.g) * f;
        let mut b = s0.b + (s1.b - s0.b) * f;
        let a = s0.a + (s1.a - s0.a) * f;
        if let Some(c) = clut {
            r = c.red[i] as f32 / 255.0;
            g = c.green[i] as f32 / 255.0;
            b = c.blue[i] as f32 / 255.0;
        }
        let base = i * 4;
        out[base] = (r.clamp(0.0, 1.0) * 255.0).round() as u8;
        out[base + 1] = (g.clamp(0.0, 1.0) * 255.0).round() as u8;
        out[base + 2] = (b.clamp(0.0, 1.0) * 255.0).round() as u8;
        out[base + 3] = (a.clamp(0.0, 1.0) * 255.0).round() as u8;
    }
    out
}

/// Fast FNV-1a 64-bit hash of a 256-entry RGBA8 LUT for GPU dirty-checking.
pub fn lut_hash(lut: &[u8; 256 * 4]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in lut {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_presets_produce_monotonic_or_valid_alpha_luts() {
        for preset in VrPreset::ALL {
            let lut = build_lut_from_stops(&preset.default_stops(), None);
            assert_eq!(lut[3], 0, "{:?} should start transparent", preset);
            assert!(
                lut[255 * 4 + 3] > 180,
                "{:?} should end opaque at high density",
                preset
            );
            assert!(preset.default_window().width > 0.0);
        }
    }
}
