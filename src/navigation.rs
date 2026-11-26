// Ported from IsiX:
// - Modules/IsiXImaging/Sources/ViewerNavigation/SliceSynchronizer.swift
// - Modules/IsiXImaging/Sources/ViewerNavigation/ReferenceLines.swift
// - Modules/IsiXImaging/Sources/ViewerNavigation/SliceNavigation.swift
// - Modules/IsiXImaging/Sources/ViewerWorkspace/ViewerTilingLayout.swift
// - Modules/IsiXImaging/Tests/ViewerNavigation/SliceSynchronizerTests.swift

/// Synchronization mode for scrolling across multiple 2D viewports (`SliceSynchronizer.swift`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(i32)]
pub enum SliceSynchronization {
    #[default]
    Absolute = 0,
    Relative = 1,
    Off = 2,
    Location = 3,
}

impl SliceSynchronization {
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Sync Off",
            Self::Absolute => "Sync 3D Position",
            Self::Relative => "Sync Relative",
            Self::Location => "Sync Slice Location",
        }
    }
}

/// Geometry of a slice in a stack for synchronization (`SynchronizableSlice` in IsiX).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SynchronizableSlice {
    pub is_3d_plane: bool,
    pub slice_location: f64,
    pub origin_z: f64,
}

/// Target instruction produced by [`SliceSynchronizer::synchronize`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SynchronizationTarget {
    /// No slice change in the follower viewport.
    None,
    /// Jump to `index` in the follower's slice array.
    SliceIndex(usize),
    /// Project the leader's 3D patient point `(x, y, z)` onto the follower's slice stack.
    PatientPoint([f32; 3]),
}

pub struct SliceSynchronizer;

impl SliceSynchronizer {
    /// Orientation cosine tolerance (`< 0.01` per element) used by `SliceSynchronizer.sameOrientation`.
    pub const ORIENTATION_TOLERANCE: f32 = 0.01;

    pub fn same_orientation(lhs: &[f32], rhs: &[f32]) -> bool {
        if lhs.len() < 9 || rhs.len() < 9 {
            return false;
        }
        for i in 0..9 {
            if (lhs[i] - rhs[i]).abs() >= Self::ORIENTATION_TOLERANCE {
                return false;
            }
        }
        true
    }

    pub fn closest_by_slice_location(
        slices: &[SynchronizableSlice],
        target_slice_location: f64,
    ) -> Option<usize> {
        let mut best_distance = 999_999.0_f64;
        let mut best_index: Option<usize> = None;
        for (index, slice) in slices.iter().enumerate() {
            let distance = (slice.slice_location - target_slice_location).abs();
            if distance < best_distance {
                best_distance = distance;
                best_index = Some(index);
            }
        }
        best_index
    }

    pub fn closest_by_origin_z(slices: &[SynchronizableSlice], target_z: f64) -> Option<usize> {
        let mut best_distance = f64::INFINITY;
        let mut best_index: Option<usize> = None;
        for (index, slice) in slices.iter().enumerate() {
            let distance = (target_z - slice.origin_z).abs();
            if distance < best_distance {
                best_distance = distance;
                best_index = Some(index);
            }
        }
        best_index
    }

    /// Computes what a follower viewport should display when the active leader viewport changes slice.
    #[allow(clippy::too_many_arguments)]
    pub fn synchronize(
        mode: SliceSynchronization,
        source_slice: SynchronizableSlice,
        source_orientation: &[f32],
        source_index: usize,
        source_flipped_data: bool,
        source_center_point: Option<[f32; 3]>,
        target_current_slice: SynchronizableSlice,
        target_orientation: &[f32],
        target_slices: &[SynchronizableSlice],
        target_current_index: usize,
        target_flipped_data: bool,
        same_frame_of_reference: bool,
    ) -> SynchronizationTarget {
        if mode == SliceSynchronization::Off {
            return SynchronizationTarget::None;
        }
        if !Self::same_orientation(source_orientation, target_orientation) {
            return SynchronizationTarget::None;
        }

        if mode == SliceSynchronization::Location {
            if let Some(best) =
                Self::closest_by_slice_location(target_slices, source_slice.slice_location)
            {
                return SynchronizationTarget::SliceIndex(best);
            }
            return SynchronizationTarget::None;
        }

        if source_slice.is_3d_plane && target_current_slice.is_3d_plane && same_frame_of_reference {
            match mode {
                SliceSynchronization::Absolute => {
                    if let Some(pt) = source_center_point {
                        return SynchronizationTarget::PatientPoint(pt);
                    }
                    SynchronizationTarget::None
                }
                SliceSynchronization::Relative => {
                    let delta = source_slice.origin_z - target_current_slice.origin_z;
                    let current_z = target_slices
                        .get(target_current_index)
                        .map(|s| s.origin_z)
                        .unwrap_or(target_current_slice.origin_z);
                    if let Some(best) =
                        Self::closest_by_origin_z(target_slices, current_z + delta)
                    {
                        SynchronizationTarget::SliceIndex(best)
                    } else {
                        SynchronizationTarget::None
                    }
                }
                SliceSynchronization::Off | SliceSynchronization::Location => {
                    SynchronizationTarget::None
                }
            }
        } else if !target_slices.is_empty() {
            let count = target_slices.len();
            let raw = if source_flipped_data == target_flipped_data {
                source_index as isize
            } else {
                (count as isize) - 1 - (source_index as isize)
            };
            let clamped = raw.clamp(0, (count as isize) - 1) as usize;
            SynchronizationTarget::SliceIndex(clamped)
        } else {
            SynchronizationTarget::None
        }
    }
}

/// 3D cross-reference / scout cut lines between two 2D viewports (`ReferenceLines.swift`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ReferenceLinePair {
    pub a: [[f32; 3]; 2],
    pub b: [[f32; 3]; 2],
    pub show_a: bool,
    pub show_b: bool,
}

pub struct ReferenceLines;

impl ReferenceLines {
    /// Minimum angle in degrees (`10.0`) between two slice planes to draw a scout reference line.
    pub const MIN_PLANE_ANGLE_DEGREES: f32 = 10.0;

    /// Computes up to two reference cut lines on `self` for `other`'s current slice
    /// (and optional first/last bounding slices of `other`'s series when `other` is a scout/localizer
    /// or multi-slice stack).
    pub fn compute<FAngle, FIntersect>(
        same_frame_of_reference: bool,
        mut plane_angle_degrees: FAngle,
        mut intersect: FIntersect,
    ) -> ReferenceLinePair
    where
        FAngle: FnMut() -> f32,
        FIntersect: FnMut() -> Option<([[f32; 3]; 2], Option<[[f32; 3]; 2]>)>,
    {
        if !same_frame_of_reference {
            return ReferenceLinePair::default();
        }
        if plane_angle_degrees() <= Self::MIN_PLANE_ANGLE_DEGREES {
            return ReferenceLinePair::default();
        }
        let Some((line_a, line_b)) = intersect() else {
            return ReferenceLinePair::default();
        };
        let mut result = ReferenceLinePair {
            a: line_a,
            b: [[0.0; 3]; 2],
            show_a: true,
            show_b: false,
        };
        if let Some(b) = line_b {
            result.b = b;
            result.show_b = true;
        }
        result
    }
}

/// Tiling grid presets for multi-series 2D comparison (`ViewerTilingLayout.swift`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewportTiling {
    pub rows: usize,
    pub columns: usize,
}

impl Default for ViewportTiling {
    fn default() -> Self {
        Self {
            rows: 1,
            columns: 1,
        }
    }
}

impl ViewportTiling {
    pub const fn new(rows: usize, columns: usize) -> Self {
        Self { rows, columns }
    }

    pub const fn cell_count(self) -> usize {
        self.rows * self.columns
    }

    pub fn label(self) -> String {
        format!("{}×{}", self.rows, self.columns)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AXIAL: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    const CORONAL: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 1.0, 0.0];

    fn slice_3d(loc: f64, z: f64) -> SynchronizableSlice {
        SynchronizableSlice {
            is_3d_plane: true,
            slice_location: loc,
            origin_z: z,
        }
    }

    fn slice_2d(loc: f64) -> SynchronizableSlice {
        SynchronizableSlice {
            is_3d_plane: false,
            slice_location: loc,
            origin_z: 0.0,
        }
    }

    #[test]
    fn orientation_matching_uses_one_hundredth_tolerance() {
        assert!(SliceSynchronizer::same_orientation(&AXIAL, &AXIAL));
        let mut close = AXIAL;
        close[1] += 0.009;
        assert!(SliceSynchronizer::same_orientation(&AXIAL, &close));
        let mut far = AXIAL;
        far[1] += 0.01;
        assert!(!SliceSynchronizer::same_orientation(&AXIAL, &far));
        assert!(!SliceSynchronizer::same_orientation(&AXIAL, &CORONAL));
    }

    #[test]
    fn synchronize_modes_match_isix_behavior() {
        let target_slices = [
            slice_3d(-20.0, 10.0),
            slice_3d(-10.0, 20.0),
            slice_3d(0.0, 30.0),
            slice_3d(10.0, 40.0),
        ];

        // Off mode -> None
        assert_eq!(
            SliceSynchronizer::synchronize(
                SliceSynchronization::Off,
                slice_3d(-9.0, 21.0),
                &AXIAL,
                1,
                false,
                Some([1.0, 2.0, 3.0]),
                target_slices[0],
                &AXIAL,
                &target_slices,
                0,
                false,
                true,
            ),
            SynchronizationTarget::None
        );

        // Location mode -> closest slice_location (-9.0 -> index 1 at -10.0)
        assert_eq!(
            SliceSynchronizer::synchronize(
                SliceSynchronization::Location,
                slice_3d(-9.0, 999.0),
                &AXIAL,
                0,
                false,
                None,
                target_slices[0],
                &AXIAL,
                &target_slices,
                0,
                false,
                false,
            ),
            SynchronizationTarget::SliceIndex(1)
        );

        // Absolute mode with 3D + same FOR -> PatientPoint
        assert_eq!(
            SliceSynchronizer::synchronize(
                SliceSynchronization::Absolute,
                slice_3d(0.0, 30.0),
                &AXIAL,
                2,
                false,
                Some([12.0, -34.0, 56.0]),
                target_slices[0],
                &AXIAL,
                &target_slices,
                0,
                false,
                true,
            ),
            SynchronizationTarget::PatientPoint([12.0, -34.0, 56.0])
        );

        // Relative mode with 3D + same FOR -> shifts by origin_z delta
        assert_eq!(
            SliceSynchronizer::synchronize(
                SliceSynchronization::Relative,
                slice_3d(0.0, 25.0),
                &AXIAL,
                2,
                false,
                None,
                slice_3d(0.0, 15.0),
                &AXIAL,
                &target_slices,
                1, // current z = 20.0, delta = 25 - 15 = +10 -> target z = 30.0 -> index 2
                false,
                true,
            ),
            SynchronizationTarget::SliceIndex(2)
        );

        // Fallback to index when not same FOR or not 3D, respecting flipped_data
        let slices_2d = [slice_2d(0.0), slice_2d(1.0), slice_2d(2.0), slice_2d(3.0)];
        assert_eq!(
            SliceSynchronizer::synchronize(
                SliceSynchronization::Absolute,
                slices_2d[1],
                &AXIAL,
                1,
                false,
                None,
                slices_2d[0],
                &AXIAL,
                &slices_2d,
                0,
                true, // flipped -> 4 - 1 - 1 = 2
                true,
            ),
            SynchronizationTarget::SliceIndex(2)
        );
    }

    #[test]
    fn reference_lines_respect_for_and_plane_angle() {
        let pair = ReferenceLines::compute(
            false,
            || 90.0,
            || Some(([[0.0; 3], [10.0; 3]], None)),
        );
        assert!(!pair.show_a);

        let shallow = ReferenceLines::compute(
            true,
            || 10.0,
            || Some(([[0.0; 3], [10.0; 3]], None)),
        );
        assert!(!shallow.show_a);

        let valid = ReferenceLines::compute(
            true,
            || 90.0,
            || Some(([[1.0, 2.0, 0.0], [3.0, 4.0, 0.0]], None)),
        );
        assert!(valid.show_a);
        assert!(!valid.show_b);
        assert_eq!(valid.a, [[1.0, 2.0, 0.0], [3.0, 4.0, 0.0]]);
    }
}
