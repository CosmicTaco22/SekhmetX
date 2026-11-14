// Ported from IsiX:
// - Modules/IsiXImaging/Sources/Annotations/OrientationLetters.swift
// - Modules/IsiXImaging/Tests/Annotations/AnnotationOverlayLayoutTests.swift

/// Anatomical patient-axis letters (`A`/`P`, `R`/`L`, `S`/`I`) for the four edges
/// of a 2D slice viewport, ported from IsiX `OrientationLetters`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OrientationEdgeLetters {
    pub left: String,
    pub right: String,
    pub top: String,
    pub bottom: String,
}

impl OrientationEdgeLetters {
    /// Returns true if all four edges are empty (e.g. non-3D plane).
    pub fn is_empty(&self) -> bool {
        self.left.is_empty()
            && self.right.is_empty()
            && self.top.is_empty()
            && self.bottom.is_empty()
    }
}

pub struct OrientationLetters;

impl OrientationLetters {
    /// Minimum absolute cosine for an axis to contribute a letter to a slice edge.
    /// `1 / 1.44` comes from `cos(45 deg) ~= 1 / 1.4142`, slightly loosened.
    pub const COSINE_THRESHOLD: f32 = 1.0 / 1.44;

    /// Patient-axis letters along a direction vector in the LPS patient frame
    /// (`+X = Left`, `+Y = Posterior`, `+Z = Superior`), ordered from the most
    /// aligned axis to the least, for every axis whose absolute component exceeds
    /// [`Self::COSINE_THRESHOLD`].
    pub fn axis_letters(vector: [f32; 3]) -> String {
        let mut letters = String::new();
        let letter_x = if vector[0] < 0.0 { 'R' } else { 'L' };
        let letter_y = if vector[1] < 0.0 { 'A' } else { 'P' };
        let letter_z = if vector[2] < 0.0 { 'I' } else { 'S' };

        let mut abs_x = vector[0].abs();
        let mut abs_y = vector[1].abs();
        let mut abs_z = vector[2].abs();

        while letters.len() < 3 {
            if abs_x > Self::COSINE_THRESHOLD && abs_x >= abs_y && abs_x >= abs_z {
                letters.push(letter_x);
                abs_x = 0.0;
            } else if abs_y > Self::COSINE_THRESHOLD && abs_y >= abs_x && abs_y >= abs_z {
                letters.push(letter_y);
                abs_y = 0.0;
            } else if abs_z > Self::COSINE_THRESHOLD && abs_z >= abs_x && abs_z >= abs_y {
                letters.push(letter_z);
                abs_z = 0.0;
            } else {
                break;
            }
        }
        letters
    }

    /// Letters for the four borders of a slice whose corrected orientation has row
    /// direction `orientation[0..3]` and column direction `orientation[3..6]`.
    pub fn edge_letters(orientation: &[f32]) -> OrientationEdgeLetters {
        if orientation.len() < 6 {
            return OrientationEdgeLetters::default();
        }
        OrientationEdgeLetters {
            left: Self::axis_letters([-orientation[0], -orientation[1], -orientation[2]]),
            right: Self::axis_letters([orientation[0], orientation[1], orientation[2]]),
            top: Self::axis_letters([-orientation[3], -orientation[4], -orientation[5]]),
            bottom: Self::axis_letters([orientation[3], orientation[4], orientation[5]]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orientation_letters_follow_axis_strength_and_threshold() {
        assert_eq!(OrientationLetters::axis_letters([1.0, 0.0, 0.0]), "L");
        assert_eq!(OrientationLetters::axis_letters([-1.0, 0.0, 0.0]), "R");
        assert_eq!(OrientationLetters::axis_letters([0.0, 1.0, 0.0]), "P");
        assert_eq!(OrientationLetters::axis_letters([0.0, -1.0, 0.0]), "A");
        assert_eq!(OrientationLetters::axis_letters([0.0, 0.0, 1.0]), "S");
        assert_eq!(OrientationLetters::axis_letters([0.0, 0.0, -1.0]), "I");

        // Oblique: both above 1/1.44 (~0.6944), ordered strongest first.
        assert_eq!(OrientationLetters::axis_letters([0.8, -0.7, 0.0]), "LA");
        assert_eq!(OrientationLetters::axis_letters([-0.7, -0.8, 0.75]), "ASR");

        // Exactly at the threshold is excluded (comparison is strict >).
        let threshold = OrientationLetters::COSINE_THRESHOLD;
        assert_eq!(OrientationLetters::axis_letters([threshold, 0.0, 0.0]), "");
        assert_eq!(OrientationLetters::axis_letters([0.5, 0.5, 0.5]), "");
    }

    #[test]
    fn edge_letters_cover_four_slice_borders() {
        // Standard axial: row +X (L), column +Y (P).
        let edges = OrientationLetters::edge_letters(&[1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
        assert_eq!(edges.left, "R");
        assert_eq!(edges.right, "L");
        assert_eq!(edges.top, "A");
        assert_eq!(edges.bottom, "P");

        assert_eq!(
            OrientationLetters::edge_letters(&[1.0, 0.0, 0.0]),
            OrientationEdgeLetters::default()
        );
    }
}
