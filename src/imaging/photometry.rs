//! Which way round an image is shown.
//!
//! Follows IsiX `FramePhotometry.swift`: MONOCHROME1 (lowest value white) is
//! shown inverted, except PET, which always keeps its polarity, and NM when
//! the NM opacity table is in use. Only the display is inverted; the values
//! (and every measurement) stay as they are.

pub fn inverts(photometric: &str, modality: &str, nm_opacity_table: bool) -> bool {
    let keeps_polarity = modality == "PT" || (nm_opacity_table && modality == "NM");
    photometric.trim() == "MONOCHROME1" && !keeps_polarity
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monochrome1_is_inverted_except_pet() {
        assert!(inverts("MONOCHROME1", "CR", false));
        assert!(!inverts("MONOCHROME2", "CR", false));
        assert!(!inverts("MONOCHROME1", "PT", false));
        assert!(inverts("MONOCHROME1", "NM", false));
        assert!(!inverts("MONOCHROME1", "NM", true));
    }
}
