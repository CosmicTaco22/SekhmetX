//! How images are windowed and shown, ported from IsiX (Modules/IsiXImaging)
//! so RDicom behaves like Horos. Each file names the IsiX source it follows;
//! the tests are IsiX's own cases, translated.
//!
//! Ported from IsiX as of 13 Nov 2026 and reviewed against IsiX 6.0.4
//! (commit 087f2cd0f721, 16 Nov 2026).

pub mod clut;
pub mod clut_tables;
pub mod convolution;
pub mod cpr;
pub mod defaults;
pub mod drag;
pub mod endoscopy;
pub mod fusion;
pub mod mr_maps;
pub mod photometry;
pub mod presets;
pub mod slab_scale;
pub mod sr;
pub mod suv;
pub mod us_region;
pub mod voi_lut;
pub mod vr_presets;
pub mod window;

/// A window: `level` is the centre, `width` the span. level − width/2 shows
/// black and level + width/2 white.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Window {
    pub level: f32,
    pub width: f32,
}

impl Window {
    pub const fn new(level: f32, width: f32) -> Self {
        Self { level, width }
    }

    pub fn low(self) -> f32 {
        self.level - self.width / 2.0
    }

    pub fn high(self) -> f32 {
        self.level + self.width / 2.0
    }
}
