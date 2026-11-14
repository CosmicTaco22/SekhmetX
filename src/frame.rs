//! Decoding one image (frame) and turning it into display pixels.
//!
//! Decoding is dicom-rs; how the image is windowed and shown follows Horos
//! (see `crate::imaging`).

use std::path::Path;

use anyhow::{Context, Result};
use dicom_dictionary_std::tags;
use dicom_object::{DefaultDicomObject, OpenFileOptions};
use dicom_pixeldata::{PhotometricInterpretation, PixelDecoder};

use crate::dicom_index::{get_f64, get_f64s, get_int, get_str};
use crate::imaging::{defaults, photometry, voi_lut::VoiLut, window, Window};

pub enum Pixels {
    /// Grey values after the modality LUT (Hounsfield units for CT), and
    /// after a VOI LUT table when the file has one Horos applies.
    Gray(Vec<f32>),
    /// Colour images (ultrasound, screenshots, ...), RGBA8.
    Rgba(Vec<u8>),
}

pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub pixels: Pixels,
    /// The file's Window Center/Width, or a VOI LUT table's output range.
    pub stored_window: Option<Window>,
    /// Smallest to largest value: Horos's "full dynamic".
    pub full_window: Window,
    /// What the stored bits can hold: the last-resort window.
    pub bits_window: Option<Window>,
    /// Shown black-for-white (MONOCHROME1, except PET).
    pub inverts: bool,
    pub voi_lut_applied: bool,
    pub slope: f32,
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

    /// The window this image opens with in Horos: its stored window, else its
    /// full dynamic, else its stored bits.
    pub fn default_window(&self) -> Window {
        defaults::default_window(self.stored_window, Some(self.full_window), self.bits_window).unwrap_or(self.full_window)
    }

    /// What a requested window becomes on this image (whole values on wide
    /// images, a minimum width), or `None` for "the full window".
    pub fn requested(&self, w: Window) -> Option<Window> {
        window::requested(w.level, w.width, self.full_window.width, self.slope)
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

    /// RGBA8 pixels for display with the given window. `invert` is the
    /// viewer's invert toggle; a MONOCHROME1 image is already shown inverted.
    pub fn render(&self, w: Window, invert: bool) -> Vec<u8> {
        let flip = invert != self.inverts;
        match &self.pixels {
            Pixels::Gray(values) => {
                let mut out = vec![0u8; values.len() * 4];
                window::linear_rgba(values, &mut out, w, flip);
                out
            }
            Pixels::Rgba(rgba) => {
                let mut out = vec![0u8; rgba.len()];
                window::colour_rgba(rgba, &mut out, &window::colour_table(w), flip);
                out
            }
        }
    }
}

/// The first item of the VOI LUT Sequence as a table, if there is one usable.
fn read_voi_lut(obj: &DefaultDicomObject, signed: bool) -> Option<VoiLut> {
    let seq = obj.element_opt(tags::VOILUT_SEQUENCE).ok().flatten()?;
    let item = seq.items()?.first()?;
    let descriptor: Vec<i64> = item.element(tags::LUT_DESCRIPTOR).ok()?.to_multi_int::<i64>().ok()?;
    let data = item.element(tags::LUT_DATA).ok()?.to_bytes().ok()?;
    VoiLut::new(&descriptor, None, Some(&data), signed)
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
    let modality = get_str(&obj, tags::MODALITY).unwrap_or_default();
    let photometric = decoded.photometric_interpretation().clone();
    let mono1 = matches!(photometric, PhotometricInterpretation::Monochrome1);
    let inverts = photometry::inverts(if mono1 { "MONOCHROME1" } else { photometric.as_str() }, &modality, false);

    let spacing = get_f64s(&obj, tags::PIXEL_SPACING)
        .or_else(|| get_f64s(&obj, tags::IMAGER_PIXEL_SPACING))
        .filter(|v| v.len() >= 2 && v[0] > 0.0 && v[1] > 0.0)
        .map(|v| (v[0], v[1]));
    let slope = get_f64(&obj, tags::RESCALE_SLOPE).unwrap_or(1.0) as f32;
    let intercept = get_f64(&obj, tags::RESCALE_INTERCEPT).unwrap_or(0.0) as f32;
    let signed = get_int(&obj, tags::PIXEL_REPRESENTATION).unwrap_or(0) == 1;

    let mut voi_lut_applied = false;
    let (pixels, stored_window, full_window, bits_window) = if decoded.samples_per_pixel() == 1 {
        let mut values: Vec<f32> = decoded
            .to_vec_frame(0)
            .context("could not convert pixel values")?;

        let mut stored_window = decoded
            .window()
            .ok()
            .flatten()
            .and_then(|w| w.first().copied())
            .map(|w| Window::new(w.center as f32, w.width as f32));

        // Horos applies a VOI LUT table only where the rescale is the identity:
        // the table's input is the stored value, and with a rescale in the way
        // the index would be wrong.
        if slope == 1.0 && intercept == 0.0 {
            if let Some(lut) = read_voi_lut(&obj, signed) {
                let original = values.clone();
                if lut.apply(&mut values, signed) {
                    voi_lut_applied = true;
                    stored_window = Some(lut.window());
                } else {
                    values = original; // a table that maps nothing is corrupt
                }
            }
        }

        let full = window::full(&values, false);
        let bits = defaults::stored_range(
            decoded.bits_stored() as i32,
            decoded.bits_allocated() as i32,
            signed && !voi_lut_applied,
            slope,
            intercept,
        );
        (Pixels::Gray(values), stored_window, full, bits)
    } else {
        let img = decoded
            .to_dynamic_image(0)
            .context("could not convert colour image")?;
        (Pixels::Rgba(img.to_rgba8().into_raw()), None, window::full(&[], true), None)
    };

    Ok(Frame {
        width,
        height,
        pixels,
        stored_window,
        full_window,
        bits_window,
        inverts,
        voi_lut_applied,
        slope,
        spacing,
        slice_thickness: get_f64(&obj, tags::SLICE_THICKNESS),
        slice_location: get_f64(&obj, tags::SLICE_LOCATION),
        value_unit: if modality == "CT" { "HU" } else { "" },
    })
}
