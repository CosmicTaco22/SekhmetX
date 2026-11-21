//! Advanced MR mapping: DWI b-value splitting, mono-exponential ADC fitting,
//! and multi-echo T2 fit mapping (`advanced-mr` area in `PORTING_MAP.md`).
//!
//! Ported from IsiX (`ThalesMMS/IsiX-DICOM-Viewer-macOS`):
//! - `Modules/IsiXImaging/Sources/BufferOperations/BValueSplit.swift`
//! - `Modules/IsiXImaging/Sources/BufferOperations/T2FitMapping.swift`

use std::collections::BTreeMap;

/// One b-value and the slice indices belonging to it, in series order (`BValueSplit.Group`).
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub struct BValueGroup {
    pub b_value: Option<f64>,
    pub indices: Vec<usize>,
}

/// DWI b-value extraction and grouping (`BValueSplit.swift`).
#[allow(dead_code)]
pub struct BValueSplit;

#[allow(dead_code)]
impl BValueSplit {
    /// Tag order tried for diffusion b-value: standard `(0018,9087)`, Siemens `(0019,100C)`,
    /// GE `(0043,1039)`, Philips `(2001,1003)`, and Siemens SequenceName `(0018,0024)`.
    pub const TAGS: [&'static str; 5] = [
        "0018,9087",
        "0019,100C",
        "0043,1039",
        "2001,1003",
        "0018,0024",
    ];

    /// GE adds a multiple of `10^9` to the b-value in `(0043,1039)`.
    pub fn ge_b_value(value: f64) -> f64 {
        if value >= 1e9 {
            value % 1e9
        } else {
            value
        }
    }

    /// Parses a Siemens diffusion sequence name like `"*ep_b800t"` or `"ep_b0"`.
    pub fn sequence_name_b_value(name: &str) -> Option<f64> {
        let bytes = name.as_bytes();
        for i in 0..bytes.len().saturating_sub(2) {
            if bytes[i] == b'_' && bytes[i + 1] == b'b' && bytes[i + 2].is_ascii_digit() {
                let start = i + 2;
                let mut end = start;
                while end < bytes.len() && bytes[end].is_ascii_digit() {
                    end += 1;
                }
                if let Ok(v) = name[start..end].parse::<f64>() {
                    return Some(v);
                }
            }
        }
        None
    }

    /// Parses a numeric string (taking the first `\`-separated component, trimming whitespace/NUL).
    pub fn parse_number(s: &str) -> Option<f64> {
        let first = s.split('\\').next().unwrap_or(s);
        let trimmed = first.trim_matches(|c: char| c.is_whitespace() || c == '\0');
        let v: f64 = trimmed.parse().ok()?;
        (v.is_finite() && v >= 0.0).then_some(v)
    }

    /// Groups slice indices by rounded b-value, lowest first; unknown b-values come last.
    /// Returns an empty `Vec` when fewer than 2 groups exist (nothing to split).
    pub fn groups(b_values: &[Option<f64>]) -> Vec<BValueGroup> {
        let mut by_key: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
        let mut unknown: Vec<usize> = Vec::new();
        for (idx, &val) in b_values.iter().enumerate() {
            match val {
                Some(v) if v.is_finite() && v >= 0.0 => {
                    by_key.entry(v.round() as i64).or_default().push(idx);
                }
                _ => unknown.push(idx),
            }
        }
        let mut result: Vec<BValueGroup> = by_key
            .into_iter()
            .map(|(k, indices)| BValueGroup {
                b_value: Some(k as f64),
                indices,
            })
            .collect();
        if !unknown.is_empty() {
            result.push(BValueGroup {
                b_value: None,
                indices: unknown,
            });
        }
        if result.len() >= 2 {
            result
        } else {
            Vec::new()
        }
    }
}

/// Multi-echo T2 grouping (`T2FitMapping.swift`) and log-linear T2 / ADC fitting.
#[allow(dead_code)]
pub struct MrMapFit;

#[allow(dead_code)]
impl MrMapFit {
    /// Groups slice indices of a single stack by 3D origin (equal within `1e-4` mm on each axis).
    /// When any origin has only one image, returns the whole stack as one group (matching IsiX).
    pub fn echo_groups_by_origin(origins: &[[f64; 3]]) -> Vec<Vec<usize>> {
        let same = |a: [f64; 3], b: [f64; 3]| {
            (a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4 && (a[2] - b[2]).abs() < 1e-4
        };
        let mut groups: Vec<Vec<usize>> = Vec::new();
        for (i, &orig) in origins.iter().enumerate() {
            if let Some(pos) = groups.iter().position(|g| same(origins[g[0]], orig)) {
                groups[pos].push(i);
            } else {
                groups.push(vec![i]);
            }
        }
        if groups.iter().any(|g| g.len() < 2) {
            return vec![(0..origins.len()).collect()];
        }
        groups
    }

    /// Log-linear T2 fit per pixel: `ln(S(TE)) = ln(S0) - TE / T2`.
    /// Returns T2 in milliseconds (0 for background / non-decaying pixels).
    pub fn fit_t2_ms(echo_signals: &[&[f32]], echo_times_ms: &[f64], min_signal: f32) -> Vec<f32> {
        let n_echoes = echo_signals.len().min(echo_times_ms.len());
        if n_echoes < 2 {
            return Vec::new();
        }
        let n_px = echo_signals[0].len();
        let mut out = vec![0.0_f32; n_px];
        for px in 0..n_px {
            let (mut sx, mut sy, mut sxx, mut sxy, mut cnt) = (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
            for e in 0..n_echoes {
                let s = echo_signals[e][px];
                if s > min_signal && s.is_finite() {
                    let x = echo_times_ms[e];
                    let y = (s as f64).ln();
                    sx += x;
                    sy += y;
                    sxx += x * x;
                    sxy += x * y;
                    cnt += 1.0;
                }
            }
            if cnt >= 2.0 {
                let denom = cnt * sxx - sx * sx;
                if denom.abs() > 1e-12 {
                    let slope = (cnt * sxy - sx * sy) / denom;
                    if slope < -1e-6 {
                        out[px] = (-1.0 / slope).clamp(0.0, 5000.0) as f32;
                    }
                }
            }
        }
        out
    }

    /// Mono-exponential ADC fit per pixel: `ln(S(b)) = ln(S0) - b * ADC`.
    /// `b_values` are in `s/mm²`; returns ADC in `10⁻⁶ mm²/s` (standard clinical scaling).
    pub fn fit_adc_um2_per_s(dwi_signals: &[&[f32]], b_values: &[f64], min_signal: f32) -> Vec<f32> {
        let n_b = dwi_signals.len().min(b_values.len());
        if n_b < 2 {
            return Vec::new();
        }
        let n_px = dwi_signals[0].len();
        let mut out = vec![0.0_f32; n_px];
        for px in 0..n_px {
            let (mut sx, mut sy, mut sxx, mut sxy, mut cnt) = (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
            for k in 0..n_b {
                let s = dwi_signals[k][px];
                if s > min_signal && s.is_finite() {
                    let x = b_values[k];
                    let y = (s as f64).ln();
                    sx += x;
                    sy += y;
                    sxx += x * x;
                    sxy += x * y;
                    cnt += 1.0;
                }
            }
            if cnt >= 2.0 {
                let denom = cnt * sxx - sx * sx;
                if denom.abs() > 1e-12 {
                    let slope = (cnt * sxy - sx * sy) / denom;
                    if slope < 0.0 {
                        out[px] = (-slope * 1e6).clamp(0.0, 10_000.0) as f32;
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b_value_split_parses_ge_siemens_and_groups() {
        assert_eq!(BValueSplit::ge_b_value(1_000_000_800.0), 800.0);
        assert_eq!(BValueSplit::ge_b_value(500.0), 500.0);
        assert_eq!(BValueSplit::sequence_name_b_value("*ep_b800t"), Some(800.0));
        assert_eq!(BValueSplit::sequence_name_b_value("ep_b0"), Some(0.0));
        assert_eq!(BValueSplit::sequence_name_b_value("t1_mprage"), None);
        assert_eq!(BValueSplit::parse_number("1000000800\\8\\0\\0"), Some(1_000_000_800.0));

        let grps = BValueSplit::groups(&[Some(800.0), Some(0.0), Some(800.0), None, Some(0.0)]);
        assert_eq!(grps.len(), 3);
        assert_eq!(grps[0], BValueGroup { b_value: Some(0.0), indices: vec![1, 4] });
        assert_eq!(grps[1], BValueGroup { b_value: Some(800.0), indices: vec![0, 2] });
        assert_eq!(grps[2], BValueGroup { b_value: None, indices: vec![3] });

        // Single b-value -> nothing to split
        assert!(BValueSplit::groups(&[Some(0.0), Some(0.0)]).is_empty());
    }

    #[test]
    fn t2_and_adc_log_linear_fits_recover_exact_values() {
        // Synthetic T2 = 80 ms, S0 = 1000 across TE = 20, 40, 80, 120 ms
        let tes = [20.0, 40.0, 80.0, 120.0];
        let s_echoes: Vec<Vec<f32>> = tes
            .iter()
            .map(|&te| vec![(1000.0 * (-te / 80.0_f64).exp()) as f32, 0.0])
            .collect();
        let refs: Vec<&[f32]> = s_echoes.iter().map(|v| v.as_slice()).collect();
        let t2 = MrMapFit::fit_t2_ms(&refs, &tes, 1.0);
        assert!((t2[0] - 80.0).abs() < 0.1, "got {}", t2[0]);
        assert_eq!(t2[1], 0.0);

        // Synthetic ADC = 1.2e-3 mm²/s (= 1200 × 10⁻⁶ mm²/s) across b = 0, 500, 1000 s/mm²
        let bs = [0.0, 500.0, 1000.0];
        let s_dwi: Vec<Vec<f32>> = bs
            .iter()
            .map(|&b| vec![(500.0 * (-b * 0.0012_f64).exp()) as f32])
            .collect();
        let drefs: Vec<&[f32]> = s_dwi.iter().map(|v| v.as_slice()).collect();
        let adc = MrMapFit::fit_adc_um2_per_s(&drefs, &bs, 1.0);
        assert!((adc[0] - 1200.0).abs() < 1.0, "got {}", adc[0]);
    }

    #[test]
    fn echo_groups_by_origin_matches_isix() {
        let origins = [[0.0, 0.0, 0.0], [0.0, 0.0, 5.0], [0.0, 0.0, 0.0], [0.0, 0.0, 5.0]];
        assert_eq!(MrMapFit::echo_groups_by_origin(&origins), vec![vec![0, 2], vec![1, 3]]);
        let singletons = [[0.0, 0.0, 0.0], [0.0, 0.0, 5.0], [0.0, 0.0, 10.0]];
        assert_eq!(MrMapFit::echo_groups_by_origin(&singletons), vec![vec![0, 1, 2]]);
    }
}
