//! Decoding one image (frame) and turning it into display pixels.

use std::path::Path;

use anyhow::{Context, Result};
use dicom_dictionary_std::tags;
use dicom_object::OpenFileOptions;
use dicom_pixeldata::{PhotometricInterpretation, PixelDecoder};
use rayon::prelude::*;

use crate::dicom_index::{get_f64, get_f64s, get_str};

pub enum Pixels {
    /// Grayscale values after the modality LUT (Hounsfield units for CT).
    Gray(Vec<f32>),
    /// Colour images (ultrasound, screenshots, ...), already RGBA8.
    Rgba(Vec<u8>),
}

pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub pixels: Pixels,
    /// Window (center, width) suggested by the file, if any.
    pub file_window: Option<(f64, f64)>,
    /// Robust value range (0.5–99.5 percentile), used when no window is stored.
    pub auto_window: (f64, f64),
    pub monochrome1: bool,
    /// (row spacing, column spacing) in mm.
    pub spacing: Option<(f64, f64)>,
    pub slice_thickness: Option<f64>,
    pub slice_location: Option<f64>,
    pub value_unit: &'static str,
}

impl Frame {
    pub fn bytes(&self) -> usize {
        match &self.pixels {
            Pixels::Gray(v) => v.len() * 4,
            Pixels::Rgba(v) => v.len(),
        }
    }

    pub fn default_window(&self) -> (f64, f64) {
        self.file_window.unwrap_or(self.auto_window)
    }

    pub fn is_gray(&self) -> bool {
        matches!(self.pixels, Pixels::Gray(_))
    }

    /// Value at pixel (x, y), for the cursor readout.
    pub fn value_at(&self, x: u32, y: u32) -> Option<String> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = (y * self.width + x) as usize;
        Some(match &self.pixels {
            Pixels::Gray(v) => {
                let val = v[i];
                if val.fract().abs() < 1e-3 {
                    format!("{:.0} {}", val, self.value_unit)
                } else {
                    format!("{:.2} {}", val, self.value_unit)
                }
            }
            Pixels::Rgba(v) => {
                let p = &v[i * 4..i * 4 + 4];
                format!("RGB {} {} {}", p[0], p[1], p[2])
            }
        })
    }

    /// Produces RGBA8 pixels for display with the given window.
    pub fn render(&self, window: (f64, f64), invert: bool) -> Vec<u8> {
        match &self.pixels {
            Pixels::Rgba(v) => {
                if invert {
                    v.chunks_exact(4)
                        .flat_map(|p| [255 - p[0], 255 - p[1], 255 - p[2], p[3]])
                        .collect()
                } else {
                    v.clone()
                }
            }
            Pixels::Gray(values) => {
                let mut out = vec![0u8; values.len() * 4];
                let flip = invert != self.monochrome1;
                apply_window(values, &mut out, window, flip);
                out
            }
        }
    }
}

/// DICOM linear VOI function (PS3.3 C.11.2.1.2), written straight into RGBA.
pub fn apply_window(values: &[f32], out: &mut [u8], (center, width): (f64, f64), flip: bool) {
    let c = center as f32 - 0.5;
    let w = (width.max(1.0) as f32 - 1.0).max(1e-3);
    let lo = c - w / 2.0;
    let hi = c + w / 2.0;
    out.par_chunks_mut(4 * 1024)
        .zip(values.par_chunks(1024))
        .for_each(|(dst, src)| {
            for (px, &v) in dst.chunks_exact_mut(4).zip(src) {
                let g = if v <= lo {
                    0.0
                } else if v > hi {
                    255.0
                } else {
                    ((v - c) / w + 0.5) * 255.0
                };
                let mut g = g as u8;
                if flip {
                    g = 255 - g;
                }
                px[0] = g;
                px[1] = g;
                px[2] = g;
                px[3] = 255;
            }
        });
}

fn percentile_range(values: &[f32]) -> (f32, f32) {
    // Sample up to ~64k values so this stays fast on large images.
    let step = (values.len() / 65_536).max(1);
    let mut s: Vec<f32> = values
        .iter()
        .step_by(step)
        .copied()
        .filter(|v| v.is_finite())
        .collect();
    if s.is_empty() {
        return (0.0, 1.0);
    }
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let lo = s[(s.len() as f64 * 0.005) as usize];
    let hi = s[((s.len() as f64 * 0.995) as usize).min(s.len() - 1)];
    if hi > lo {
        (lo, hi)
    } else {
        (s[0], s[s.len() - 1].max(s[0] + 1.0))
    }
}

pub fn load_frame(path: &Path, frame: u32) -> Result<Frame> {
    let obj = OpenFileOptions::new()
        .open_file(path)
        .with_context(|| format!("could not open {}", path.display()))?;

    let decoded = obj
        .decode_pixel_data_frame(frame)
        .context("could not decode the pixel data (unsupported compression?)")?;

    let width = decoded.columns();
    let height = decoded.rows();
    let photometric = decoded.photometric_interpretation().clone();
    let monochrome1 = matches!(photometric, PhotometricInterpretation::Monochrome1);

    let spacing = get_f64s(&obj, tags::PIXEL_SPACING)
        .or_else(|| get_f64s(&obj, tags::IMAGER_PIXEL_SPACING))
        .filter(|v| v.len() >= 2 && v[0] > 0.0 && v[1] > 0.0)
        .map(|v| (v[0], v[1]));
    let modality = get_str(&obj, tags::MODALITY).unwrap_or_default();

    let (pixels, file_window, auto_window) = if decoded.samples_per_pixel() == 1 {
        let values: Vec<f32> = decoded
            .to_vec_frame(0)
            .context("could not convert pixel values")?;
        let file_window = decoded
            .window()
            .ok()
            .flatten()
            .and_then(|w| w.first().copied())
            .filter(|w| w.width > 0.0)
            .map(|w| (w.center, w.width));
        let (lo, hi) = percentile_range(&values);
        let auto = (((lo + hi) / 2.0) as f64, ((hi - lo) as f64).max(1.0));
        (Pixels::Gray(values), file_window, auto)
    } else {
        let img = decoded
            .to_dynamic_image(0)
            .context("could not convert colour image")?;
        (Pixels::Rgba(img.to_rgba8().into_raw()), None, (128.0, 256.0))
    };

    Ok(Frame {
        width,
        height,
        pixels,
        file_window,
        auto_window,
        monochrome1,
        spacing,
        slice_thickness: get_f64(&obj, tags::SLICE_THICKNESS),
        slice_location: get_f64(&obj, tags::SLICE_LOCATION),
        value_unit: if modality == "CT" { "HU" } else { "" },
    })
}
