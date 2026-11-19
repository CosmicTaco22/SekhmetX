//! PET Standardized Uptake Value (SUV) parameters, decay correction, conversion,
//! and PET windowing rules.
//!
//! Ported from IsiX (`ThalesMMS/IsiX-DICOM-Viewer-macOS`):
//! - `Modules/IsiXImaging/Sources/SUVParameters.swift`
//! - `Modules/IsiXImaging/Sources/BufferOperations/SUVComputation.swift`
//! - `Modules/IsiXImaging/Sources/BufferOperations/PETSUVConversion.swift`
//! - `Modules/IsiXImaging/Sources/ViewerWindowLevel/PETWindowing.swift`
//! - `Modules/IsiXImaging/Sources/SUVWindowScaling.swift`
//! - `Modules/IsiXImaging/Sources/ViewGeometry/ViewTransform.swift` (SUV & PET window helpers)

/// What a PET object says that an SUV is computed from: the patient's weight,
/// the unit of the pixels, how they were decay corrected, the dose and when it
/// was injected, and when the image was acquired.
#[derive(Debug, Clone, PartialEq)]
pub struct SuvParameters {
    pub patients_weight: f32,
    pub units: Option<String>,
    pub decay_correction: Option<String>,
    pub decay_factor: f32,
    pub radionuclide_total_dose: f32,
    pub halflife: f32,
    /// Injection as DICOM date-time (`YYYYMMDDHHMMSS.FFFFFF`).
    pub radiopharmaceutical_start_datetime: Option<String>,
    /// Acquisition as DICOM date-time (`YYYYMMDDHHMMSS.FFFFFF`).
    pub acquisition_datetime: Option<String>,
    /// Philips' own SUV scale factor `(7053,1000)`.
    pub philips_factor: f32,
}

impl Default for SuvParameters {
    fn default() -> Self {
        Self {
            patients_weight: 0.0,
            units: None,
            decay_correction: None,
            decay_factor: 1.0,
            radionuclide_total_dose: 0.0,
            halflife: 0.0,
            radiopharmaceutical_start_datetime: None,
            acquisition_datetime: None,
            philips_factor: 0.0,
        }
    }
}

impl SuvParameters {
    /// The date and time attributes, in pairs, that the decay is counted to,
    /// by preference: series, acquisition, content or study first.
    #[allow(dead_code)]
    pub fn acquisition_time_priority(field: i32) -> &'static [&'static str] {
        match field {
            0 => &[
                "SeriesDate",
                "SeriesTime",
                "AcquisitionDate",
                "AcquisitionTime",
                "ContentDate",
                "ContentTime",
                "StudyDate",
                "StudyTime",
            ],
            1 => &[
                "AcquisitionDate",
                "AcquisitionTime",
                "SeriesDate",
                "SeriesTime",
                "ContentDate",
                "ContentTime",
                "StudyDate",
                "StudyTime",
            ],
            2 => &[
                "ContentDate",
                "ContentTime",
                "SeriesDate",
                "SeriesTime",
                "AcquisitionDate",
                "AcquisitionTime",
                "StudyDate",
                "StudyTime",
            ],
            3 => &[
                "StudyDate",
                "StudyTime",
                "SeriesDate",
                "SeriesTime",
                "AcquisitionDate",
                "AcquisitionTime",
                "ContentDate",
                "ContentTime",
            ],
            _ => &[],
        }
    }

    /// Computes the PET-to-SUV multiplication factor if all required metadata is valid.
    pub fn compute_factor(&self, is_rgb: bool) -> Option<f64> {
        let inj_sec = self
            .radiopharmaceutical_start_datetime
            .as_deref()
            .and_then(parse_dicom_dt_seconds);
        let acq_sec = self
            .acquisition_datetime
            .as_deref()
            .and_then(parse_dicom_dt_seconds);
        let has_times = inj_sec.is_some() && acq_sec.is_some();

        let avail = SuvComputation::availability(
            self.units.as_deref(),
            self.decay_correction.as_deref(),
            self.philips_factor,
            self.decay_factor,
            self.halflife,
            has_times,
            self.radionuclide_total_dose,
            is_rgb,
        );
        if !avail.has_suv {
            return None;
        }

        let (corrected_dose, df_override) = SuvComputation::corrected_dose(
            self.decay_correction.as_deref(),
            self.radionuclide_total_dose,
            self.halflife,
            inj_sec,
            acq_sec,
        );
        let decay_factor = df_override.unwrap_or(self.decay_factor);

        if !PetSuvConversion::can_convert(
            is_rgb,
            self.units.as_deref(),
            self.philips_factor,
            corrected_dose,
            self.patients_weight,
            avail.has_suv,
        ) {
            return None;
        }

        let factor = PetSuvConversion::factor(
            self.units.as_deref(),
            self.philips_factor,
            self.patients_weight,
            corrected_dose,
            decay_factor,
        );
        if factor.is_finite() && factor > 0.0 {
            Some(factor)
        } else {
            None
        }
    }
}

/// Parses a DICOM DT (`YYYYMMDDHHMMSS[.FFFFFF]`) into seconds since Unix epoch (approximate
/// calendar days + exact time of day), suitable for elapsed-time differences.
pub fn parse_dicom_dt_seconds(dt: &str) -> Option<f64> {
    let s = dt.trim();
    if s.len() < 14 {
        return None;
    }
    let year: i64 = s.get(0..4)?.parse().ok()?;
    let month: i64 = s.get(4..6)?.parse().ok()?;
    let day: i64 = s.get(6..8)?.parse().ok()?;
    let hour: i64 = s.get(8..10)?.parse().ok()?;
    let min: i64 = s.get(10..12)?.parse().ok()?;
    // Seconds may include a fractional part (e.g. "30.125")
    let sec_str = s.get(12..)?;
    // Stop at timezone (+/-) if present
    let sec_end = sec_str
        .find(['+', '-'])
        .unwrap_or(sec_str.len());
    let sec: f64 = sec_str[..sec_end].parse().ok()?;

    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    // Civil days since 1970-01-01 (Howard Hinnant's algorithm).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let m = month;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;

    Some((days * 86_400 + hour * 3_600 + min * 60) as f64 + sec)
}

/// What deciding whether an image has SUV found: whether it has, and whether
/// the dose correction has to run first (`ADMIN` and `NONE`, which need no
/// acquisition time).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SuvAvailability {
    pub has_suv: bool,
    pub corrects_dose: bool,
}

/// Whether a PET image's values can be read as SUV, and the injected dose
/// decay-corrected to the acquisition (PS 3.3 C.8.9.1, C.8.9.2).
pub struct SuvComputation;

impl SuvComputation {
    /// The dose and decay factor after correction.
    ///
    /// `ADMIN` and `NONE` mean the pixel values were not decay corrected back to
    /// the injection time, so the dose applies as injected, with a factor of one;
    /// `None` `decay_factor` means it is left as it is. Otherwise the dose decays
    /// over the time from injection to acquisition; without a half-life or with
    /// an acquisition before the injection there is no usable correction, and
    /// the dose is zero, which callers read as "no SUV".
    pub fn corrected_dose(
        decay_correction: Option<&str>,
        dose: f32,
        halflife: f32,
        injection_secs: Option<f64>,
        acquisition_secs: Option<f64>,
    ) -> (f32, Option<f32>) {
        if let Some(dc) = decay_correction
            && (dc == "ADMIN" || dc == "NONE")
        {
            return (dose, Some(1.0));
        }
        let elapsed = (-Self::interval(injection_secs, acquisition_secs)) as f32;
        if halflife > 0.0 && elapsed > 0.0 {
            // Float arithmetic in the exponent, exponential and product in double (matching IsiX).
            let exp_arg = (-elapsed * 2.0_f32.ln() / halflife) as f64;
            return (((dose as f64) * exp_arg.exp()) as f32, None);
        }
        (0.0, None)
    }

    /// An image has SUV when its values are activity concentrations (`BQML`, or
    /// Philips `CNTS` with a scale factor), its decay correction is one SUV can
    /// undo, it has a dose, and it is not colour.
    #[allow(clippy::too_many_arguments)]
    pub fn availability(
        units: Option<&str>,
        decay_correction: Option<&str>,
        philips_factor: f32,
        decay_factor: f32,
        halflife: f32,
        has_injection_and_acquisition: bool,
        dose: f32,
        is_rgb: bool,
    ) -> SuvAvailability {
        let none = SuvAvailability {
            has_suv: false,
            corrects_dose: false,
        };
        if !matches!(units, Some("BQML") | Some("CNTS")) {
            return none;
        }
        if units == Some("CNTS") && philips_factor == 0.0 {
            return none;
        }
        let Some(dc) = decay_correction else {
            return none;
        };
        if !matches!(dc, "START" | "NONE" | "ADMIN") {
            return none;
        }
        let corrects = dc == "NONE" || dc == "ADMIN";
        if !corrects && (decay_factor == 0.0 || halflife <= 0.0 || !has_injection_and_acquisition) {
            return none;
        }
        SuvAvailability {
            has_suv: dose > 0.0 && !is_rgb,
            corrects_dose: corrects,
        }
    }

    /// The factor that converts the image's values to SUV, once they have been
    /// converted; one before.
    #[allow(dead_code)]
    pub fn applied_factor(converted: bool, factor: f32) -> f32 {
        if converted { factor } else { 1.0 }
    }

    /// `-[date timeIntervalSinceDate: other]` as Objective-C answered it: zero
    /// when `date` is `None`, and measured from 0 when `other` is `None`.
    pub fn interval(date: Option<f64>, since_other: Option<f64>) -> f64 {
        let Some(d) = date else {
            return 0.0;
        };
        d - since_other.unwrap_or(0.0)
    }
}

/// The conversion of PET values to SUV (standardized uptake values) of the 2D viewer.
pub struct PetSuvConversion;

impl PetSuvConversion {
    /// Whether the values of the current slice allow a conversion: grey pixels,
    /// Philips counts with their factor or a positive corrected dose and weight,
    /// and SUV values at all.
    pub fn can_convert(
        is_rgb: bool,
        units: Option<&str>,
        philips_factor: f32,
        radionuclide_total_dose_corrected: f32,
        patients_weight: f32,
        has_suv: bool,
    ) -> bool {
        if is_rgb {
            return false;
        }
        if !(units == Some("CNTS") && philips_factor != 0.0) {
            if radionuclide_total_dose_corrected <= 0.0 {
                return false;
            }
            if patients_weight <= 0.0 {
                return false;
            }
        }
        has_suv
    }

    /// The factor that turns the values of a slice into SUV: the Philips factor
    /// of counts (`"CNTS"`), otherwise `weight (kg) * 1000 / (corrected_dose * decay_factor)`,
    /// the dose times the decay in float.
    pub fn factor(
        units: Option<&str>,
        philips_factor: f32,
        patients_weight: f32,
        radionuclide_total_dose_corrected: f32,
        decay_factor: f32,
    ) -> f64 {
        if units == Some("CNTS") {
            return philips_factor as f64;
        }
        (patients_weight as f64) * 1000.0
            / ((radionuclide_total_dose_corrected * decay_factor) as f64)
    }

    /// Multiplies the `count` floats in `pixels` by `factor`.
    #[allow(dead_code)]
    pub fn scale(pixels: &mut [f32], count: isize, factor: f32) {
        if count <= 0 {
            return;
        }
        let n = (count as usize).min(pixels.len());
        for p in &mut pixels[..n] {
            *p *= factor;
        }
    }
}

/// `ViewTransform.window(from:to:)` in IsiX.
#[allow(dead_code)]
pub fn window_from_range(from: f32, to: f32) -> (f32, f32) {
    let width = to - from;
    let level = ((from as f64) + (width as f64) / 2.0) as f32;
    (width, level)
}

/// `ViewTransform.percentOfMaximum(_:_:)` in IsiX.
#[allow(dead_code)]
pub fn percent_of_maximum(maximum: f32, percent: f32) -> f32 {
    (((maximum * percent) as f64) / 100.0) as f32
}

/// `ViewTransform.suv(_:weight:dose:decay:)` in IsiX.
#[allow(dead_code)]
pub fn suv_value(value: f32, weight: f32, dose: f32, decay: f32) -> f32 {
    value * weight * 1000.0 / (dose * decay)
}

/// The window a PET-like image (`PT`, or `NM` when preferred) opens with.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PetWindowing {
    Stored,
    PercentOfMaximum { from: f32, to: f32 },
    Suv { from: f32, to: f32 },
    Unchanged,
}

#[allow(dead_code)]
impl PetWindowing {
    pub fn from_mode(mode: i32, percent_from: f32, percent_to: f32, suv_from: f32, suv_to: f32) -> Self {
        match mode {
            0 => Self::Stored,
            1 => Self::PercentOfMaximum {
                from: percent_from,
                to: percent_to,
            },
            2 => Self::Suv {
                from: suv_from,
                to: suv_to,
            },
            _ => Self::Unchanged,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn restored<F: FnOnce() -> Option<f32>>(
        self,
        stored: (f32, f32),
        current: (f32, f32),
        pet_like: bool,
        suv_converted: bool,
        series_maximum: f32,
        image_only: bool,
        suv_factor: F,
    ) -> Option<(f32, f32)> {
        let (wl, ww) = stored;
        if ww != 0.0 || wl != 0.0 {
            if ww == 0.0 {
                return None;
            }
            if !pet_like {
                return Some((wl, ww));
            }
            return match self {
                Self::Stored => {
                    if !suv_converted {
                        Some((wl, ww))
                    } else if let Some(factor) = suv_factor() {
                        Some((wl * factor, ww * factor))
                    } else {
                        Some(current)
                    }
                }
                Self::PercentOfMaximum { from, to } => {
                    let lo = percent_of_maximum(series_maximum, from);
                    let hi = percent_of_maximum(series_maximum, to);
                    if hi - lo == 0.0 {
                        Some(current)
                    } else {
                        let (w, l) = window_from_range(lo, hi);
                        Some((l, w))
                    }
                }
                Self::Suv { from, to } => {
                    if !suv_converted {
                        Some((wl, ww))
                    } else if to - from == 0.0 {
                        Some(current)
                    } else {
                        let (w, l) = window_from_range(from, to);
                        Some((l, w))
                    }
                }
                Self::Unchanged => Some(current),
            };
        }
        if image_only || !pet_like {
            return None;
        }
        match self {
            Self::PercentOfMaximum { from, to } => {
                let (w, l) = window_from_range(
                    percent_of_maximum(series_maximum, from),
                    percent_of_maximum(series_maximum, to),
                );
                Some((l, w))
            }
            Self::Suv { from, to } if suv_converted => {
                let (w, l) = window_from_range(from, to);
                Some((l, w))
            }
            Self::Stored | Self::Suv { .. } | Self::Unchanged => Some((wl, ww)),
        }
    }
}

/// What the view's window becomes after converting a PET series to SUV.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SuvViewWindow {
    Unchanged,
    FromSeries,
    Window { level: f32, width: f32 },
}

/// The choice of the SUV sheet (`SUVCONVERSION` preference).
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuvSheetConversion {
    LeaveValues = 0,
    ConvertPixels = 1,
    DisplaySuv = 2,
}

#[allow(dead_code)]
impl SuvSheetConversion {
    pub fn from_raw(raw: i32) -> Option<Self> {
        match raw {
            0 => Some(Self::LeaveValues),
            1 => Some(Self::ConvertPixels),
            2 => Some(Self::DisplaySuv),
            _ => None,
        }
    }
}

/// What converting a PET series to SUV does to its windows (`SUVWindowScaling.swift`).
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SuvWindowScaling {
    pub factor: f64,
    pub rescales_view: bool,
}

#[allow(dead_code)]
impl SuvWindowScaling {
    pub fn new(current_slice_factor: Option<f64>) -> Self {
        Self {
            factor: current_slice_factor.unwrap_or(1.0),
            rescales_view: current_slice_factor.is_some(),
        }
    }

    pub fn scaled(self, value: f32) -> f32 {
        ((value as f64) * self.factor) as f32
    }

    pub fn view_window(self, current: (f32, f32), windowing: PetWindowing) -> SuvViewWindow {
        if !self.rescales_view {
            return SuvViewWindow::Unchanged;
        }
        if windowing != PetWindowing::Stored {
            return SuvViewWindow::FromSeries;
        }
        SuvViewWindow::Window {
            level: self.scaled(current.0),
            width: self.scaled(current.1),
        }
    }

    pub fn becquerels(megabecquerels: f32) -> f32 {
        ((megabecquerels as f64) * 1_000_000.0) as f32
    }

    pub fn megabecquerels_text(becquerels: f32) -> String {
        format!("{:.3}", (becquerels as f64) / 1_000_000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pet_suv_conversion_convertible_values() {
        assert!(PetSuvConversion::can_convert(false, Some("BQML"), 0.0, 3e8, 70.0, true));
        assert!(!PetSuvConversion::can_convert(true, Some("BQML"), 0.0, 3e8, 70.0, true));
        assert!(!PetSuvConversion::can_convert(false, Some("BQML"), 0.0, 0.0, 70.0, true));
        assert!(!PetSuvConversion::can_convert(false, None, 0.0, 3e8, -1.0, true));
        assert!(!PetSuvConversion::can_convert(false, Some("BQML"), 0.0, 3e8, 70.0, false));
        // Philips counts with their factor need neither dose nor weight.
        assert!(PetSuvConversion::can_convert(false, Some("CNTS"), 0.5, 0.0, 0.0, true));
        assert!(!PetSuvConversion::can_convert(false, Some("CNTS"), 0.0, 0.0, 70.0, true));
    }

    #[test]
    fn pet_suv_conversion_factor_multiplies_dose_and_decay_in_float() {
        let dose: f32 = 3.1e8;
        let decay: f32 = 1.0625;
        assert_eq!(
            PetSuvConversion::factor(Some("BQML"), 9.0, 70.5, dose, decay),
            70.5_f64 * 1000.0 / ((dose * decay) as f64)
        );
        assert_eq!(
            PetSuvConversion::factor(Some("CNTS"), 0.00125, 70.5, dose, decay),
            0.00125_f32 as f64
        );
    }

    #[test]
    fn pet_suv_conversion_scales_every_pixel() {
        let mut pixels = vec![1.0_f32, -2.5, 3.0];
        PetSuvConversion::scale(&mut pixels, 2, 1.5);
        assert_eq!(pixels, vec![1.5, -3.75, 3.0]);
        PetSuvConversion::scale(&mut pixels, -1, 2.0);
        assert_eq!(pixels, vec![1.5, -3.75, 3.0]);
    }

    #[test]
    fn suv_window_scaling_already_in_suv_leaves_every_window() {
        let scaling = SuvWindowScaling::new(None);
        assert_eq!(scaling.factor, 1.0);
        assert!(!scaling.rescales_view);
        assert_eq!(scaling.scaled(123.456).to_bits(), 123.456_f32.to_bits());
        assert_eq!(
            scaling.view_window((40.0, 400.0), PetWindowing::Stored),
            SuvViewWindow::Unchanged
        );
        assert_eq!(
            scaling.view_window((40.0, 400.0), PetWindowing::Suv { from: 0.0, to: 6.0 }),
            SuvViewWindow::Unchanged
        );
    }

    #[test]
    fn suv_window_scaling_sheet_conversion_and_doses() {
        assert_eq!(SuvSheetConversion::from_raw(0), Some(SuvSheetConversion::LeaveValues));
        assert_eq!(SuvSheetConversion::from_raw(1), Some(SuvSheetConversion::ConvertPixels));
        assert_eq!(SuvSheetConversion::from_raw(2), Some(SuvSheetConversion::DisplaySuv));
        assert_eq!(SuvSheetConversion::from_raw(3), None);
        assert_eq!(SuvSheetConversion::from_raw(-1), None);

        assert_eq!(
            SuvWindowScaling::becquerels(370.0).to_bits(),
            ((370.0_f64 * 1_000_000.0) as f32).to_bits()
        );
        assert_eq!(SuvWindowScaling::megabecquerels_text(370_000_000.0), "370.000");
        assert_eq!(SuvWindowScaling::megabecquerels_text(0.0), "0.000");
    }

    #[test]
    fn suv_window_scaling_matches_former_viewer_conversion() {
        let factors: [f64; 8] = [0.0, 1.0, 0.000_123_4, 0.5, 1.0 / 3.0, 7.25, 1e-7, 1e6];
        for mode in -1..=4 {
            for converted in [false, true] {
                for &factor in &factors {
                    let scaling = SuvWindowScaling::new(if converted { None } else { Some(factor) });
                    let windowing = PetWindowing::from_mode(mode, 10.0, 90.0, 0.0, 6.0);
                    let cwl = 2500.0_f32;
                    let cww = 5000.0_f32;
                    let vw = scaling.view_window((cwl, cww), windowing);
                    if converted {
                        assert_eq!(vw, SuvViewWindow::Unchanged);
                    } else if mode != 0 {
                        assert_eq!(vw, SuvViewWindow::FromSeries);
                    } else {
                        assert_eq!(
                            vw,
                            SuvViewWindow::Window {
                                level: ((cwl as f64) * factor) as f32,
                                width: ((cww as f64) * factor) as f32,
                            }
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn suv_parameters_fdg_decay_and_factor() {
        // Typical F-18 FDG PET: 370 MBq injected at 09:00:00, acquired at 10:00:00 (3600 s elapsed),
        // half-life 6586.2 s, 70 kg patient, DecayCorrection = START, Units = BQML.
        let params = SuvParameters {
            patients_weight: 70.0,
            units: Some("BQML".to_string()),
            decay_correction: Some("START".to_string()),
            decay_factor: 1.0,
            radionuclide_total_dose: 370_000_000.0,
            halflife: 6586.2,
            radiopharmaceutical_start_datetime: Some("20261118090000.000000".to_string()),
            acquisition_datetime: Some("20261118100000.000000".to_string()),
            philips_factor: 0.0,
        };
        let factor = params.compute_factor(false).expect("valid FDG SUV factor");
        // Decayed dose = 370e6 * exp(-3600 * ln(2) / 6586.2) ≈ 253.40e6 Bq
        // Factor = 70 * 1000 / 253.40e6 ≈ 0.00027624
        assert!((factor - 0.00027624).abs() < 1e-6, "got {factor}");
    }

    #[test]
    fn suv_parameters_admin_decay_needs_no_times() {
        let params = SuvParameters {
            patients_weight: 80.0,
            units: Some("BQML".to_string()),
            decay_correction: Some("ADMIN".to_string()),
            decay_factor: 1.0,
            radionuclide_total_dose: 400_000_000.0,
            halflife: 6586.2,
            radiopharmaceutical_start_datetime: None,
            acquisition_datetime: None,
            philips_factor: 0.0,
        };
        let factor = params.compute_factor(false).expect("ADMIN SUV factor");
        assert!((factor - 80_000.0 / 400_000_000.0).abs() < 1e-9);
    }
}
