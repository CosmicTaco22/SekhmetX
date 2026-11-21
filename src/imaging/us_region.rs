//! Ultrasound Region Calibration (`SequenceOfUltrasoundRegions`, tag `0018,6011`).
//!
//! Ported from IsiX (`ThalesMMS/IsiX-DICOM-Viewer-macOS`):
//! - `Modules/IsiXImaging/Sources/USRegion.swift`
//! - `Modules/IsiXImaging/Tests/USRegionTests.swift`

/// One region of the Sequence of Ultrasound Regions `(0018,6011)`: where it sits
/// in the image and how its pixels map to physical units.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UsRegion {
    /// `(0018,6012)` — 1 = 2D, 2 = M-mode, 3 = Spectral, 4 = Waveform, 5 = Graphics
    pub region_spatial_format: i32,
    /// `(0018,6014)`
    pub region_data_type: i32,
    /// `(0018,6016)`
    pub region_flags: i32,

    /// `(0018,6018)`
    pub region_location_min_x0: i32,
    /// `(0018,601A)`
    pub region_location_min_y0: i32,
    /// `(0018,601C)`
    pub region_location_max_x1: i32,
    /// `(0018,601E)`
    pub region_location_max_y1: i32,

    /// `(0018,6020)` (optional)
    pub reference_pixel_x0: i32,
    /// `(0018,6022)` (optional)
    pub reference_pixel_y0: i32,

    /// `(0018,6024)` — 3 = cm
    pub physical_units_x_direction: i32,
    /// `(0018,6026)` — 3 = cm
    pub physical_units_y_direction: i32,

    /// `(0018,6028)`
    pub ref_pixel_physical_value_x: f64,
    /// `(0018,602A)`
    pub ref_pixel_physical_value_y: f64,

    /// `(0018,602C)` — physical delta per pixel along X
    pub physical_delta_x: f64,
    /// `(0018,602E)` — physical delta per pixel along Y (`<0`: up, `>0`: down)
    pub physical_delta_y: f64,

    /// `(0018,6034)`
    pub doppler_correction_angle: f64,

    pub is_reference_pixel_x0_present: bool,
    pub is_reference_pixel_y0_present: bool,
}

impl UsRegion {
    /// Whether the pixel `(x, y)` lies in the region, its edges included.
    pub fn holds(&self, x: i32, y: i32) -> bool {
        x >= self.region_location_min_x0
            && x <= self.region_location_max_x1
            && y >= self.region_location_min_y0
            && y <= self.region_location_max_y1
    }

    /// A region to measure in: one with a spatial format or physical units.
    pub fn measures(&self) -> bool {
        !(self.region_spatial_format == 0
            && self.physical_units_x_direction == 0
            && self.physical_units_y_direction == 0)
    }

    /// A 2D region (`format == 1`) in centimetres both ways (`units == 3`),
    /// where a length has a single isotropic/anisotropic mm scale.
    pub fn is_two_dimensional_in_centimetres(&self) -> bool {
        self.region_spatial_format == 1
            && self.physical_units_x_direction == 3
            && self.physical_units_y_direction == 3
    }

    /// Pixel spacing `(x_mm, y_mm)` in millimetres for a 2D centimetre region
    /// with non-zero deltas.
    pub fn pixel_spacing_mm(&self) -> Option<(f64, f64)> {
        if !self.is_two_dimensional_in_centimetres()
            || self.physical_delta_x == 0.0
            || self.physical_delta_y == 0.0
        {
            return None;
        }
        Some((
            self.physical_delta_x.abs() * 10.0,
            self.physical_delta_y.abs() * 10.0,
        ))
    }

    /// First region in `regions` that holds both endpoints `a` and `b` and can measure.
    #[allow(dead_code)]
    pub fn region_holding(a: (i32, i32), b: (i32, i32), regions: &[UsRegion]) -> Option<&UsRegion> {
        regions
            .iter()
            .find(|r| r.measures() && r.holds(a.0, a.1) && r.holds(b.0, b.1))
    }

    /// Primary 2D centimetre pixel spacing `(row_mm, col_mm)` from the largest
    /// (or first) calibrated 2D region in `regions`, for frame-level fallback.
    pub fn primary_spacing_row_col_mm(regions: &[UsRegion]) -> Option<(f64, f64)> {
        regions
            .iter()
            .filter_map(|r| {
                let (x_mm, y_mm) = r.pixel_spacing_mm()?;
                let area = ((r.region_location_max_x1 - r.region_location_min_x0).max(0) as i64)
                    * ((r.region_location_max_y1 - r.region_location_min_y0).max(0) as i64);
                Some((area, (y_mm, x_mm)))
            })
            .max_by_key(|(area, _)| *area)
            .map(|(_, sp)| sp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_region(
        min_x: i32,
        min_y: i32,
        max_x: i32,
        max_y: i32,
        format: i32,
        units: (i32, i32),
        deltas: (f64, f64),
    ) -> UsRegion {
        UsRegion {
            region_location_min_x0: min_x,
            region_location_min_y0: min_y,
            region_location_max_x1: max_x,
            region_location_max_y1: max_y,
            region_spatial_format: format,
            physical_units_x_direction: units.0,
            physical_units_y_direction: units.1,
            physical_delta_x: deltas.0,
            physical_delta_y: deltas.1,
            ..Default::default()
        }
    }

    #[test]
    fn line_is_measured_with_scale_of_region_that_holds_it() {
        let fine = make_region(0, 0, 99, 99, 1, (3, 3), (0.1, 0.1));
        let coarse = make_region(100, 0, 199, 99, 1, (3, 3), (0.2, -0.2));
        for regions in [vec![fine.clone(), coarse.clone()], vec![coarse.clone(), fine.clone()]] {
            let in_fine = UsRegion::region_holding((10, 50), (20, 50), &regions).unwrap();
            assert_eq!(in_fine, &fine);
            let (fx, fy) = in_fine.pixel_spacing_mm().unwrap();
            assert_eq!((fx, fy), (1.0, 1.0));

            let in_coarse = UsRegion::region_holding((110, 50), (120, 50), &regions).unwrap();
            assert_eq!(in_coarse, &coarse);
            let (cx, cy) = in_coarse.pixel_spacing_mm().unwrap();
            assert_eq!((cx, cy), (2.0, 2.0));
        }
    }

    #[test]
    fn line_from_one_region_to_another_is_in_none() {
        let regions = [
            make_region(0, 0, 99, 99, 1, (3, 3), (0.1, 0.1)),
            make_region(100, 0, 199, 99, 1, (3, 3), (0.1, 0.1)),
        ];
        assert!(UsRegion::region_holding((90, 50), (110, 50), &regions).is_none());
        assert!(UsRegion::region_holding((250, 50), (260, 50), &regions).is_none());
        assert_eq!(UsRegion::region_holding((0, 0), (99, 99), &regions), Some(&regions[0]));
        assert_eq!(UsRegion::region_holding((100, 0), (199, 99), &regions), Some(&regions[1]));
    }

    #[test]
    fn region_with_no_format_and_no_units_is_passed_over() {
        let blank = make_region(0, 0, 199, 99, 0, (0, 0), (0.1, 0.1));
        let real = make_region(0, 0, 99, 99, 1, (3, 3), (0.1, 0.1));
        assert_eq!(
            UsRegion::region_holding((10, 10), (20, 20), &[blank.clone(), real.clone()]),
            Some(&real)
        );
        assert!(UsRegion::region_holding((10, 10), (20, 20), std::slice::from_ref(&blank)).is_none());
        assert!(!blank.measures() && real.measures());
    }

    #[test]
    fn only_2d_region_in_centimetres_gives_pixel_spacing() {
        let doppler = make_region(0, 100, 199, 199, 3, (4, 7), (0.01, 0.5));
        assert!(doppler.measures() && !doppler.is_two_dimensional_in_centimetres() && doppler.pixel_spacing_mm().is_none());
        assert!(make_region(0, 0, 9, 9, 2, (4, 3), (0.1, 0.1)).pixel_spacing_mm().is_none());
        assert!(make_region(0, 0, 9, 9, 1, (3, 4), (0.1, 0.1)).pixel_spacing_mm().is_none());
        assert!(make_region(0, 0, 9, 9, 1, (3, 3), (0.0, 0.1)).pixel_spacing_mm().is_none());
        assert_eq!(
            make_region(0, 0, 9, 9, 1, (3, 3), (0.05, 0.025)).pixel_spacing_mm(),
            Some((0.5, 0.25))
        );
    }
}
