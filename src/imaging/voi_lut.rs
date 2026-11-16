//! VOI LUT tables (PS 3.3 C.11.2), common on CR, DX and mammography.
//!
//! Follows IsiX `VOILUTMapping.swift`, Horos `VOILookupTable.swift` and
//! `DCMPix+FrameLoading.swift` (`applyVOILUT`), as of IsiX 6.0.4: every frame
//! is mapped (one lying wholly outside the table is uniform, not a sign of a
//! corrupt table), a one-entry table is valid, and with a rescale the table is
//! entered by the rescaled value (PS 3.3 C.11.2.1.1).

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
        if !(1..=65_536).contains(&count) || !(8..=16).contains(&depth) {
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
    /// below `first_mapped`, the last past the end, truncated to 16 bits as
    /// Horos does. Every value gets an entry; a table of no entry maps nothing.
    ///
    /// The table's input is the output of the modality LUT. With `rescaled`
    /// false the values are stored samples, read as 16-bit (signed or not).
    /// With `rescaled` true they already carry the rescale slope and intercept,
    /// and each enters the table by its nearest integer.
    pub fn apply(&self, values: &mut [f32], signed: bool, rescaled: bool) {
        let Some(last) = self.entries.len().checked_sub(1) else {
            return;
        };
        for v in values.iter_mut() {
            let value = if rescaled {
                nearest_within_reach(*v as f64)
            } else {
                let raw = *v as i64;
                if signed { raw as i16 as i64 } else { raw as u16 as i64 }
            };
            let index = (value - self.first_mapped as i64).clamp(0, last as i64) as usize;
            *v = (self.entries[index] as u16) as f32;
        }
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

/// Past any first mapped value and any table: both are 16-bit quantities.
const FARTHEST: i64 = 1 << 30;

/// The integer a table is entered with for a rescaled value: the nearest one,
/// held within what no table reaches. A rescale that gives no number gives 0.
/// (IsiX `VOILUTMapping.modalityValue`.)
fn nearest_within_reach(value: f64) -> i64 {
    let value = value.round();
    if value.is_nan() {
        0
    } else {
        value.clamp(-(FARTHEST as f64), FARTHEST as f64) as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lut(first: i64) -> VoiLut {
        VoiLut::new(&[4, first, 16], Some(&[100, 200, 300, 400]), None, true).unwrap()
    }

    /// IsiX `VOILUTMappingTests.mapped`: 16-bit samples through a table, with
    /// an optional rescale applied first as the decoder does.
    fn mapped(samples: &[i64], first: i64, table: &[u16], signed: bool, slope: f64, offset: f64) -> Vec<f32> {
        let lut = VoiLut::new(&[table.len() as i64, first.rem_euclid(65_536), 16], Some(table), None, true).unwrap();
        let rescaled = slope != 1.0 || offset != 0.0;
        let mut values: Vec<f32> = samples
            .iter()
            .map(|&s| if rescaled { (s as f64 * slope + offset) as f32 } else { s as f32 })
            .collect();
        lut.apply(&mut values, signed, rescaled);
        values
    }

    #[test]
    fn values_below_take_the_first_entry_and_above_the_last() {
        let mut values = [0.0, 10.0, 11.0, 12.0, 13.0, 99.0];
        lut(10).apply(&mut values, false, false);
        assert_eq!(values, [100.0, 100.0, 200.0, 300.0, 400.0, 400.0]);
    }

    // IsiX: aRescaledSampleEntersTheTableByItsRescaledValue
    #[test]
    fn a_rescaled_sample_enters_the_table_by_its_rescaled_value() {
        let table = [1000, 1010, 1020, 1030];
        // 2 x - 1024: the stored values 562, 563 and 564 are 100, 102 and 104.
        assert_eq!(
            mapped(&[0, 562, 563, 564, 4000], 100, &table, false, 2.0, -1024.0),
            [1000.0, 1000.0, 1020.0, 1030.0, 1030.0]
        );
        // Signed samples and a negative first value: -3 and 1 are -1001 and -997.
        assert_eq!(mapped(&[-3, 0, 1], -1000, &table, true, 1.0, -998.0), [1000.0, 1020.0, 1030.0]);
        // The nearest integer: 100.4, 101.5 and 102.6.
        assert_eq!(mapped(&[1004, 1015, 1026], 100, &table, false, 0.1, 0.0), [1000.0, 1020.0, 1030.0]);
    }

    // IsiX: aRescaleThatGivesNoNumberStaysWithinTheTable
    #[test]
    fn a_rescale_that_gives_no_number_stays_within_the_table() {
        assert_eq!(nearest_within_reach(f64::NAN), 0);
        assert!(nearest_within_reach(f64::INFINITY) > 1 << 20);
        assert!(nearest_within_reach(f64::NEG_INFINITY) < -(1 << 20));
        assert!(nearest_within_reach(60000.0 * 1e30) > 1 << 20);
        assert_eq!(mapped(&[5], 0, &[1, 2, 3], false, 1e30, 0.0), [3.0]);
    }

    // IsiX: aFrameOnTheFirstEntryOrWhollyOutsideTheTableIsMapped
    #[test]
    fn a_frame_on_the_first_entry_or_wholly_outside_the_table_is_mapped() {
        let table = [1000, 1010, 1020, 1030];
        assert_eq!(mapped(&[100, 100, 100], 100, &table, false, 1.0, 0.0), [1000.0; 3]);
        assert_eq!(mapped(&[0, 50, 99], 100, &table, false, 1.0, 0.0), [1000.0; 3]);
        assert_eq!(mapped(&[104, 4095, 65535], 100, &table, false, 1.0, 0.0), [1030.0; 3]);
    }

    // IsiX: aTableOfOneEntryAnswersItForEveryValue
    #[test]
    fn a_table_of_one_entry_answers_it_for_every_value() {
        assert_eq!(mapped(&[0, 300, 301, 65535], 300, &[777], false, 1.0, 0.0), [777.0; 4]);
    }

    // IsiX: signedSamplesBelowASignedFirstValue
    #[test]
    fn signed_samples_below_a_signed_first_value() {
        assert_eq!(mapped(&[-3, -40, -2, -1, 7], -2, &[5, 6], true, 1.0, 0.0), [5.0, 5.0, 5.0, 6.0, 6.0]);
    }

    #[test]
    fn a_signed_first_value_is_read_as_negative() {
        let lut = VoiLut::new(&[4, 65_534, 16], Some(&[1, 2, 3, 4]), None, true).unwrap();
        assert_eq!(lut.first_mapped, -2);
        let mut values = [-1.0, 0.0];
        lut.apply(&mut values, true, false);
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
