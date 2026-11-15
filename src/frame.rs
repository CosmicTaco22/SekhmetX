//! Decoding one image (frame) and turning it into display pixels.
//!
//! Decoding is dicom-rs; how the image is windowed and shown follows Horos
//! (see `crate::imaging` and `crate::geometry`).

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context, Result};
use dicom_dictionary_std::tags;
use dicom_object::{DefaultDicomObject, OpenFileOptions};
use dicom_pixeldata::{PhotometricInterpretation, PixelDecoder};

use crate::dicom_index::{get_f64, get_f64s, get_int, get_str};
use crate::geometry::{ImagePlaneCoordinates, ImagePlaneGeometry};
use crate::imaging::{
    clut::ColorLookupTable, defaults, photometry, voi_lut::VoiLut, window, Window,
};

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
    /// Aspect ratio `spacing_y / spacing_x` settled by `ImagePlaneGeometry`.
    #[allow(dead_code)]
    pub pixel_ratio: f64,
    pub origin: Option<[f64; 3]>,
    pub orientation: [f64; 9],
    pub is_3d_plane: bool,
    #[allow(dead_code)]
    pub frame_of_reference_uid: Option<String>,
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
        defaults::default_window(self.stored_window, Some(self.full_window), self.bits_window)
            .unwrap_or(self.full_window)
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
        let i = (y as usize) * (self.width as usize) + (x as usize);
        match &self.pixels {
            Pixels::Gray(v) => {
                let val = *v.get(i)?;
                Some(if val.fract().abs() < 1e-3 {
                    format!("{:.0} {}", val, self.value_unit)
                } else {
                    format!("{:.2} {}", val, self.value_unit)
                })
            }
            Pixels::Rgba(v) => {
                let p = v.get(i * 4..i * 4 + 4)?;
                Some(format!("RGB {} {} {}", p[0], p[1], p[2]))
            }
        }
    }

    /// 3D patient coordinates `(X, Y, Z)` in mm at pixel `(x, y)` when this slice has a 3D plane.
    pub fn patient_coords_at(&self, x: f64, y: f64) -> Option<[f64; 3]> {
        if !self.is_3d_plane {
            return None;
        }
        let origin = self.origin?;
        let (row_sp, col_sp) = self.spacing?;
        Some(ImagePlaneCoordinates::pixel_to_patient_double(
            x,
            y,
            origin,
            &self.orientation,
            col_sp,
            row_sp,
            true,
        ))
    }

    /// RGBA8 pixels for display with the given window. `invert` is the
    /// viewer's invert toggle; a MONOCHROME1 image is already shown inverted.
    #[allow(dead_code)]
    pub fn render(&self, w: Window, invert: bool) -> Vec<u8> {
        self.render_with_clut(w, invert, None)
    }

    /// RGBA8 pixels for display with the given window and optional 256-entry RGB CLUT.
    pub fn render_with_clut(
        &self,
        w: Window,
        invert: bool,
        clut: Option<&ColorLookupTable>,
    ) -> Vec<u8> {
        let flip = invert != self.inverts;
        match &self.pixels {
            Pixels::Gray(values) => {
                let mut out = vec![0u8; values.len() * 4];
                window::linear_rgba_with_clut(values, &mut out, w, flip, clut);
                out
            }
            Pixels::Rgba(rgba) => {
                let mut out = vec![0u8; rgba.len()];
                window::colour_rgba(rgba, &mut out, &window::colour_table(w), flip);
                out
            }
        }
    }

    /// Returns a new `Frame` with 2D spatial convolution applied (`ImageConvolution`).
    pub fn with_convolution(&self, conv: &crate::imaging::convolution::ImageConvolution) -> Self {
        let w = self.width as usize;
        let h = self.height as usize;
        let pixels = match &self.pixels {
            Pixels::Gray(values) => Pixels::Gray(conv.filtered(values, w, h)),
            Pixels::Rgba(rgba) => Pixels::Rgba(conv.filtered_rgba(rgba, w, h)),
        };
        Self {
            width: self.width,
            height: self.height,
            pixels,
            stored_window: self.stored_window,
            full_window: self.full_window,
            bits_window: self.bits_window,
            inverts: self.inverts,
            voi_lut_applied: self.voi_lut_applied,
            slope: self.slope,
            spacing: self.spacing,
            pixel_ratio: self.pixel_ratio,
            origin: self.origin,
            orientation: self.orientation,
            is_3d_plane: self.is_3d_plane,
            frame_of_reference_uid: self.frame_of_reference_uid.clone(),
            slice_thickness: self.slice_thickness,
            slice_location: self.slice_location,
            value_unit: self.value_unit,
        }
    }

    /// Downsampled RGBA8 mini-thumbnail for the Horos Study Browser series list.
    pub fn render_thumbnail(&self, max_side: u32) -> (u32, u32, Vec<u8>) {
        let max_side = max_side.max(8);
        let w = self.width.max(1);
        let h = self.height.max(1);
        let scale = (max_side as f32 / w.max(h) as f32).min(1.0);
        let tw = ((w as f32 * scale).round() as u32).max(1);
        let th = ((h as f32 * scale).round() as u32).max(1);
        let win = self.default_window();
        let full_rgba = self.render_with_clut(win, false, None);
        let mut out = vec![0u8; (tw * th * 4) as usize];
        for ty in 0..th {
            let sy = ((ty * h) / th).min(h - 1) as usize;
            for tx in 0..tw {
                let sx = ((tx * w) / tw).min(w - 1) as usize;
                let src = (sy * w as usize + sx) * 4;
                let dst = (ty * tw + tx) as usize * 4;
                out[dst..dst + 4].copy_from_slice(&full_rgba[src..src + 4]);
            }
        }
        (tw, th, out)
    }
}

/// The first item of the VOI LUT Sequence as a table, if there is one usable.
fn read_voi_lut(obj: &DefaultDicomObject, signed: bool) -> Option<VoiLut> {
    let seq = obj.element_opt(tags::VOILUT_SEQUENCE).ok().flatten()?;
    let item = seq.items()?.first()?;
    let descriptor: Vec<i64> = item
        .element(tags::LUT_DESCRIPTOR)
        .ok()?
        .to_multi_int::<i64>()
        .ok()?;
    let data = item.element(tags::LUT_DATA).ok()?.to_bytes().ok()?;
    VoiLut::new(&descriptor, None, Some(&data), signed)
}

/// Small cache of recently opened multi-frame DICOM objects so stepping through
/// frames of a multi-frame cine file doesn't re-read and re-parse the whole file
/// from disk on every frame.
const MULTIFRAME_CACHE_LEN: usize = 4;

type DicomCache = VecDeque<(PathBuf, Arc<DefaultDicomObject>)>;

fn open_dicom_cached(path: &Path) -> Result<Arc<DefaultDicomObject>> {
    static CACHE: OnceLock<Mutex<DicomCache>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(VecDeque::with_capacity(MULTIFRAME_CACHE_LEN)));

    if let Ok(mut guard) = cache.lock()
        && let Some(pos) = guard.iter().position(|(p, _)| p == path) {
            let entry = guard.remove(pos).unwrap();
            let obj = entry.1.clone();
            guard.push_back(entry);
            return Ok(obj);
        }

    let obj = Arc::new(
        OpenFileOptions::new()
            .open_file(path)
            .with_context(|| format!("could not open {}", path.display()))?,
    );

    let num_frames = get_int(&obj, tags::NUMBER_OF_FRAMES).unwrap_or(1);
    if num_frames > 1
        && let Ok(mut guard) = cache.lock()
            && !guard.iter().any(|(p, _)| p == path) {
                while guard.len() >= MULTIFRAME_CACHE_LEN {
                    guard.pop_front();
                }
                guard.push_back((path.to_path_buf(), obj.clone()));
            }

    Ok(obj)
}

pub fn load_frame(path: &Path, frame: u32) -> Result<Frame> {
    let obj = open_dicom_cached(path)?;

    let decoded = obj
        .decode_pixel_data_frame(frame)
        .context("could not decode the pixel data (unsupported compression?)")?;

    let width = decoded.columns();
    let height = decoded.rows();
    let modality = get_str(&obj, tags::MODALITY).unwrap_or_default();
    let photometric = decoded.photometric_interpretation().clone();
    let mono1 = matches!(photometric, PhotometricInterpretation::Monochrome1);
    let inverts = photometry::inverts(
        if mono1 {
            "MONOCHROME1"
        } else {
            photometric.as_str()
        },
        &modality,
        false,
    );

    let raw_spacing = get_f64s(&obj, tags::PIXEL_SPACING)
        .or_else(|| get_f64s(&obj, tags::IMAGER_PIXEL_SPACING))
        .filter(|v| v.len() >= 2 && v[0] > 0.0 && v[1] > 0.0)
        .map(|v| (v[0], v[1]));
    let (spacing, pixel_ratio) = match raw_spacing {
        Some((row_sp, col_sp)) => {
            let settled = ImagePlaneGeometry::settled_pixel_spacing(
                col_sp,
                row_sp,
                height as i32,
                width as i32,
                false,
            );
            (
                (settled.x > 0.0 && settled.y > 0.0).then_some((settled.y, settled.x)),
                settled.ratio,
            )
        }
        None => (None, 1.0),
    };

    let origin = get_f64s(&obj, tags::IMAGE_POSITION_PATIENT)
        .filter(|v| v.len() >= 3)
        .map(|v| [v[0], v[1], v[2]]);
    let raw_orientation = get_f64s(&obj, tags::IMAGE_ORIENTATION_PATIENT)
        .filter(|v| v.len() >= 6)
        .map(|v| [v[0], v[1], v[2], v[3], v[4], v[5]]);
    let (orientation, is_3d_plane) = match raw_orientation {
        Some(raw) => {
            let norm = ImagePlaneCoordinates::normalized_orientation(&raw);
            let has_plane =
                origin.is_some() && (norm[6] != 0.0 || norm[7] != 0.0 || norm[8] != 0.0);
            (norm, has_plane)
        }
        None => (ImagePlaneGeometry::DEFAULT_ORIENTATION, false),
    };

    let slice_location = get_f64(&obj, tags::SLICE_LOCATION)
        .or_else(|| origin.and_then(|pos| ImagePlaneGeometry::slice_location(pos, &orientation)));
    let frame_of_reference_uid = get_str(&obj, tags::FRAME_OF_REFERENCE_UID);

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
        if slope == 1.0 && intercept == 0.0
            && let Some(lut) = read_voi_lut(&obj, signed) {
                let original = values.clone();
                if lut.apply(&mut values, signed) {
                    voi_lut_applied = true;
                    stored_window = Some(lut.window());
                } else {
                    values = original; // a table that maps nothing is corrupt
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
        (
            Pixels::Rgba(img.to_rgba8().into_raw()),
            None,
            window::full(&[], true),
            None,
        )
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
        pixel_ratio,
        origin,
        orientation,
        is_3d_plane,
        frame_of_reference_uid,
        slice_thickness: get_f64(&obj, tags::SLICE_THICKNESS),
        slice_location,
        value_unit: if modality == "CT" { "HU" } else { "" },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::imaging::convolution::FACTORY_CONVOLUTIONS;

    #[test]
    fn convolution_is_presentation_only_and_source_retains_original_values() {
        let mut pixels = vec![0.0_f32; 5 * 5];
        pixels[2 * 5 + 2] = 1000.0;
        let source = Frame {
            width: 5,
            height: 5,
            pixels: Pixels::Gray(pixels),
            stored_window: None,
            full_window: Window { level: 0.0, width: 1000.0 },
            bits_window: None,
            inverts: false,
            voi_lut_applied: false,
            slope: 1.0,
            spacing: Some((1.0, 1.0)),
            pixel_ratio: 1.0,
            origin: None,
            orientation: ImagePlaneGeometry::DEFAULT_ORIENTATION,
            is_3d_plane: false,
            frame_of_reference_uid: None,
            slice_thickness: None,
            slice_location: None,
            value_unit: "HU",
        };
        let conv = FACTORY_CONVOLUTIONS[0].to_convolution();
        let presentation = source.with_convolution(&conv);

        assert_eq!(source.value_at(2, 2).as_deref(), Some("1000 HU"));
        let Pixels::Gray(ref p_vals) = presentation.pixels else {
            panic!("expected gray");
        };
        assert_ne!(p_vals[2 * 5 + 2], 1000.0);
    }
}
