//! VOI LUT tables (PS 3.3 C.11.2), common on CR, DX and mammography.
//!
//! Follows IsiX `VOILUTMapping.swift`, Horos `VOILookupTable.swift` and
//! `DCMPix+FrameLoading.swift` (`applyVOILUT`).

use super::Window;

#[derive(Debug, Clone, PartialEq)]
pub struct VoiLut {
    /// The first stored value the table maps; below it everything takes the first entry.
    pub first_mapped: i32,
    pub entries: Vec<u32>,
    /// Bits per entry, 8 to 16.
    pub depth: u32,
}

impl VoiLut {
    /// Builds a table from LUT Descriptor (0028,3002) and LUT Data (0028,3006),
    /// or `None` when it isn't one this can use: a descriptor that isn't three
    /// values, a count that doesn't match the data, a depth outside 8–16.
    ///
    /// `data_words` is the data read as 16-bit little-endian words (OW or US);
    /// `data_bytes` the raw bytes, for 8-bit tables written as OB.
    pub fn new(descriptor: &[i64], data_words: Option<&[u16]>, data_bytes: Option<&[u8]>, signed: bool) -> Option<Self> {
        if descriptor.len() < 3 {
            return None;
        }
        // Zero means 65536: the count is stored in 16 bits.
        let count = if descriptor[0] == 0 { 65_536 } else { descriptor[0] };
        let depth = descriptor[2];
        if count <= 1 || count > 65_536 || !(8..=16).contains(&depth) {
            return None;
        }
        let count = count as usize;
        let first = descriptor[1];
        let first_mapped = if signed && first > 32_767 { first - 65_536 } else { first } as i32;

        let entries: Vec<u32> = if let Some(words) = data_words.filter(|w| w.len() >= count) {
            words[..count].iter().map(|&w| w as u32).collect()
        } else {
            let bytes = data_bytes?;
            // 8-bit entries are one byte each when written as OB and one
            // 16-bit word each when written as OW: the length settles it.
            if bytes.len() >= count * 2 {
                (0..count).map(|i| bytes[i * 2] as u32 | (bytes[i * 2 + 1] as u32) << 8).collect()
            } else if depth <= 8 && bytes.len() >= count {
                bytes[..count].iter().map(|&b| b as u32).collect()
            } else {
                return None;
            }
        };
        Some(Self { first_mapped, entries, depth: depth as u32 })
    }

    /// Replaces each value by the table's entry for it: the first entry at or
    /// below `first_mapped`, the last past the end. Values are read as 16-bit
    /// samples (signed or not) and results truncated to 16 bits, as Horos does.
    ///
    /// Returns whether any value fell inside the table: a table that maps
    /// nothing is taken for a corrupted one, and the caller should then keep
    /// the original values.
    pub fn apply(&self, values: &mut [f32], signed: bool) -> bool {
        let number = self.entries.len() as i64;
        let mut inside = false;
        for v in values.iter_mut() {
            let raw = *v as i64;
            let sample = if signed { raw as i16 as i64 } else { raw as u16 as i64 };
            let mut index = sample - self.first_mapped as i64;
            if index <= 0 {
                index = 0;
            } else if index >= number {
                index = number - 1;
            } else {
                inside = true;
            }
            *v = (self.entries[index as usize] as u16) as f32;
        }
        inside
    }

    /// The window that shows everything the table produces. The file's own
    /// Window Center/Width describe the table's input, and mean nothing once
    /// the values are its output.
    pub fn window(&self) -> Window {
        let smallest = self.entries.iter().copied().min().unwrap_or(0) as f64;
        let largest = self.entries.iter().copied().max().unwrap_or(0) as f64;
        Window::new(((largest + smallest) / 2.0) as f32, (largest - smallest + 1.0).max(1.0) as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lut(first: i64) -> VoiLut {
        VoiLut::new(&[4, first, 16], Some(&[100, 200, 300, 400]), None, true).unwrap()
    }

    #[test]
    fn values_below_take_the_first_entry_and_above_the_last() {
        let mut values = [0.0, 10.0, 11.0, 12.0, 13.0, 99.0];
        assert!(lut(10).apply(&mut values, false));
        assert_eq!(values, [100.0, 100.0, 200.0, 300.0, 400.0, 400.0]);
    }

    #[test]
    fn a_table_that_maps_nothing_says_so() {
        let mut values = [0.0, 5.0];
        assert!(!lut(10).apply(&mut values, false));
    }

    #[test]
    fn a_signed_first_value_is_read_as_negative() {
        let lut = VoiLut::new(&[4, 65_534, 16], Some(&[1, 2, 3, 4]), None, true).unwrap();
        assert_eq!(lut.first_mapped, -2);
        let mut values = [-1.0, 0.0];
        assert!(lut.apply(&mut values, true));
        assert_eq!(values, [2.0, 3.0]);
    }

    #[test]
    fn zero_entries_means_65536_and_bad_descriptors_are_refused() {
        let words = vec![7u16; 65_536];
        assert_eq!(VoiLut::new(&[0, 0, 16], Some(&words), None, false).unwrap().entries.len(), 65_536);
        assert!(VoiLut::new(&[4, 0], Some(&[1, 2, 3, 4]), None, false).is_none());
        assert!(VoiLut::new(&[4, 0, 20], Some(&[1, 2, 3, 4]), None, false).is_none());
        assert!(VoiLut::new(&[4, 0, 16], Some(&[1, 2]), None, false).is_none());
    }

    #[test]
    fn eight_bit_tables_are_read_from_bytes_or_words() {
        let ob = VoiLut::new(&[3, 0, 8], None, Some(&[1, 2, 3]), false).unwrap();
        assert_eq!(ob.entries, vec![1, 2, 3]);
        let ow = VoiLut::new(&[3, 0, 8], None, Some(&[1, 0, 2, 0, 3, 0]), false).unwrap();
        assert_eq!(ow.entries, vec![1, 2, 3]);
    }

    #[test]
    fn the_window_covers_the_tables_output() {
        assert_eq!(lut(0).window(), Window::new(250.0, 301.0));
    }
}
