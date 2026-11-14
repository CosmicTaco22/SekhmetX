//! Pixel and volume intensity statistics inside a ROI: mean, sum, sample
//! standard deviation (`n - 1`), min, max, skewness, kurtosis, and median.
//!
//! Ported from `IsiXROI` (`ROIPixelStatistics.swift`, `ROIValueStatistics.swift`,
//! `CalciumScoring.swift`).

/// Mean, sum, standard deviation, extremes, higher moments and median of the
/// intensities inside a ROI (`ROIPixelStatistics.swift`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoiPixelStatistics {
    pub count: usize,
    pub mean: f32,
    pub total: f32,
    /// Zero for zero or one finite sample; NaN when a non-finite sample made the sum invalid.
    pub deviation: f32,
    /// Zero when there are no samples.
    pub minimum: f32,
    pub maximum: f32,
    /// Zero unless `higher_moments` was requested.
    pub skewness: f32,
    pub kurtosis: f32,
}

impl RoiPixelStatistics {
    /// Takes the statistics of `samples`.
    pub fn compute(samples: &[f32], wants_deviation: bool, higher_moments: bool) -> Self {
        let count = samples.len();
        let mut total = 0.0f64;
        let mut minimum = f32::MAX;
        let mut maximum = f32::MIN;
        for &value in samples {
            total += value as f64;
            if value < minimum {
                minimum = value;
            }
            if maximum < value {
                maximum = value;
            }
        }
        let mean = if count != 0 {
            total / (count as f64)
        } else {
            0.0
        };

        let mut deviation: f32 = 0.0;
        if wants_deviation {
            let mut squares = 0.0f64;
            for &value in samples {
                let delta = mean - (value as f64);
                squares = delta.mul_add(delta, squares);
            }
            deviation = if count > 1 {
                ((squares / ((count - 1) as f64)).sqrt()) as f32
            } else if squares.is_finite() {
                0.0
            } else {
                f32::NAN
            };
        }

        let min_out = if minimum == f32::MAX { 0.0 } else { minimum };
        let max_out = if maximum == f32::MIN { 0.0 } else { maximum };
        let skewness = if higher_moments {
            Self::skewness(samples, mean) as f32
        } else {
            0.0
        };
        let kurtosis = if higher_moments {
            Self::kurtosis(samples, mean) as f32
        } else {
            0.0
        };

        Self {
            count,
            mean: mean as f32,
            total: total as f32,
            deviation,
            minimum: min_out,
            maximum: max_out,
            skewness,
            kurtosis,
        }
    }

    /// The central moment of `order`, divided by `n - 1`; 0 for order 1.
    pub fn moment(samples: &[f32], mean: f64, order: i32) -> f64 {
        if samples.len() < 2 || order == 1 {
            return 0.0;
        }
        let mut sum = 0.0;
        for &val in samples {
            sum += ((val as f64) - mean).powi(order);
        }
        sum / ((samples.len() - 1) as f64)
    }

    /// The third central moment over the cube of the deviation.
    pub fn skewness(samples: &[f32], mean: f64) -> f64 {
        if samples.len() < 2 {
            return 0.0;
        }
        let m3 = Self::moment(samples, mean, 3);
        let sm2 = Self::moment(samples, mean, 2).sqrt();
        m3 / (sm2 * sm2 * sm2)
    }

    /// The fourth central moment over the fourth power of the deviation, less 3.
    pub fn kurtosis(samples: &[f32], mean: f64) -> f64 {
        if samples.len() < 2 {
            return 0.0;
        }
        let m4 = Self::moment(samples, mean, 4);
        let sm2 = Self::moment(samples, mean, 2).sqrt();
        (m4 / (sm2 * sm2 * sm2 * sm2)) - 3.0
    }

    /// The median of the finite samples; NaN when there is none.
    pub fn median(samples: &[f32]) -> f64 {
        let mut finite: Vec<f32> = samples.iter().copied().filter(|x| x.is_finite()).collect();
        if finite.is_empty() {
            return f64::NAN;
        }
        finite.sort_by(|a, b| a.total_cmp(b));
        let middle = finite.len() / 2;
        if finite.len() % 2 == 1 {
            finite[middle] as f64
        } else {
            ((finite[middle - 1] as f64) + (finite[middle] as f64)) / 2.0
        }
    }
}

/// The statistics of the pixel values under the ROIs that make a volume
/// (`ROIValueStatistics.swift`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoiValueStatistics {
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub total: f64,
    pub deviation: f64,
}

impl RoiValueStatistics {
    /// The statistics of `values`; `None` for none.
    pub fn of(values: &[f32]) -> Option<Self> {
        let &first = values.first()?;
        let mut total = 0.0f64;
        for &value in values {
            total += value as f64;
        }
        let mean = total / (values.len() as f64);

        let mut deviation = 0.0f64;
        let mut low = first;
        let mut high = first;
        for &value in values {
            let mut distance = (mean - (value as f64)) as f32;
            distance *= distance;
            deviation += distance as f64;
            if value < low {
                low = value;
            }
            if value > high {
                high = value;
            }
        }
        deviation = (deviation / ((values.len() as f64) - 1.0)).sqrt();

        Some(Self {
            min: low as f64,
            max: high as f64,
            mean,
            total,
            deviation,
        })
    }
}

/// Agatston / Calcium Scoring scanner thresholds and vessel sums (`CalciumScoring.swift`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CalciumScoreSums {
    pub score: f32,
    pub mass: f32,
    pub volume: f32,
}

impl CalciumScoreSums {
    pub const ZERO: Self = Self {
        score: 0.0,
        mass: 0.0,
        volume: 0.0,
    };

    pub fn add(&mut self, other: Self) {
        self.score += other.score;
        self.mass += other.mass;
        self.volume += other.volume;
    }

    /// Lower threshold in HU for scanner type: 90 HU for EBCT (`0`), 130 HU for MSCT (`1`).
    pub fn lower_threshold(ct_type: i32) -> Option<i32> {
        match ct_type {
            0 => Some(90),
            1 => Some(130),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_statistics() {
        let s = RoiPixelStatistics::compute(&[9.0, 1.0, 5.0], true, true);
        assert_eq!(s.mean, 5.0);
        assert_eq!(s.total, 15.0);
        assert_eq!(s.deviation, 4.0);
        assert_eq!(s.minimum, 1.0);
        assert_eq!(s.maximum, 9.0);
        assert_eq!(s.count, 3);

        let t = RoiPixelStatistics::compute(&[9.0, 1.0, 5.0, 3.0], true, true);
        assert_eq!(t.mean, 4.5);
        assert_eq!(t.total, 18.0);
        assert!(((t.deviation as f64) - (35.0f64 / 3.0).sqrt()).abs() < 1e-6);
    }

    #[test]
    fn empty_single_and_invalid() {
        let empty = RoiPixelStatistics::compute(&[], true, true);
        assert_eq!(empty.mean, 0.0);
        assert_eq!(empty.total, 0.0);
        assert_eq!(empty.deviation, 0.0);
        assert_eq!(empty.minimum, 0.0);
        assert_eq!(empty.maximum, 0.0);
        assert!(empty.deviation.is_sign_positive());
        assert_eq!(RoiPixelStatistics::compute(&[42.0], true, true).deviation, 0.0);
        assert!(RoiPixelStatistics::compute(&[f32::NAN], true, true)
            .deviation
            .is_nan());
        assert!(RoiPixelStatistics::compute(&[f32::INFINITY], true, true)
            .deviation
            .is_nan());
    }

    #[test]
    fn higher_moments_and_median() {
        let s = RoiPixelStatistics::compute(&[1.0, 2.0, 3.0, 4.0, 5.0], true, true);
        assert!(s.skewness.abs() < 1e-7);
        assert!(((s.kurtosis as f64) - (8.5 / 6.25 - 3.0)).abs() < 1e-6);
        assert_eq!(
            RoiPixelStatistics::compute(&[1.0, 2.0, 3.0, 4.0, 5.0], true, false).kurtosis,
            0.0
        );
        let right = RoiPixelStatistics::compute(&[0.0, 0.0, 0.0, 10.0], true, true);
        assert!(right.skewness > 0.0);

        assert_eq!(RoiPixelStatistics::median(&[9.0, 1.0, 5.0]), 5.0);
        assert_eq!(RoiPixelStatistics::median(&[9.0, 1.0, 5.0, 3.0]), 4.0);
        assert!(RoiPixelStatistics::median(&[]).is_nan());
        assert_eq!(
            RoiPixelStatistics::median(&[f32::NAN, 1.0, 3.0, f32::INFINITY]),
            2.0
        );
    }

    #[test]
    fn volume_value_statistics() {
        assert_eq!(RoiValueStatistics::of(&[]), None);
        let s = RoiValueStatistics::of(&[4.0, -2.0, 10.0, 0.5]).unwrap();
        assert_eq!(s.min, -2.0);
        assert_eq!(s.max, 10.0);
        assert_eq!(s.total, 12.5);
        assert_eq!(s.mean, 3.125);
        assert_eq!(s.deviation, (81.1875f64 / 3.0).sqrt());
        assert!(RoiValueStatistics::of(&[7.0]).unwrap().deviation.is_nan());
    }
}
