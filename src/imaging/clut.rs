// Ported from IsiX:
// - Modules/IsiXImaging/Sources/ViewerPipeline/ColorLookupTable.swift
// - Modules/IsiXImaging/Sources/ViewerPipeline/ColourTableBook.swift
// - Modules/IsiXImaging/Sources/ViewerPipeline/ColourTableState.swift
// - Modules/IsiXPreferences/Sources/DefaultPreferences+ImagePresets.swift
// - Modules/IsiXImaging/Tests/ViewerPipeline/ColourTableStateTests.swift

#![allow(dead_code)]

use super::clut_tables::PLIST_CLUTS;

/// A 256-entry RGB colour lookup table (CLUT) for a 2D viewer, ported from IsiX `ColorLookupTable`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColorLookupTable {
    pub red: [u8; 256],
    pub green: [u8; 256],
    pub blue: [u8; 256],
}

impl Default for ColorLookupTable {
    fn default() -> Self {
        Self::grey_levels()
    }
}

impl ColorLookupTable {
    pub const fn new(red: [u8; 256], green: [u8; 256], blue: [u8; 256]) -> Self {
        Self { red, green, blue }
    }

    /// Identity greyscale ramp (`0..=255` in every channel).
    pub fn grey_levels() -> Self {
        let mut ramp = [0u8; 256];
        let mut i = 0;
        while i < 256 {
            ramp[i] = i as u8;
            i += 1;
        }
        Self::new(ramp, ramp, ramp)
    }

    /// True when every channel equals `0..=255`.
    pub fn is_grey_levels(&self) -> bool {
        ColourTableState::is_grey_levels(&self.red, &self.green, &self.blue)
    }

    /// Index of the darkest entry (first entry with lowest `red + green + blue`),
    /// used as the viewer's black index where there is no image.
    pub fn darkest_index(&self) -> usize {
        let mut darkness: u32 = 256 * 3;
        let mut index = 0usize;
        for i in 0..256 {
            let sum = (self.red[i] as u32) + (self.green[i] as u32) + (self.blue[i] as u32);
            if sum < darkness {
                index = i;
                darkness = sum;
            }
        }
        index
    }

    /// Map an 8-bit greyscale intensity `v` to `[R, G, B, 255]`.
    #[inline]
    pub fn map_rgba(&self, v: u8) -> [u8; 4] {
        let idx = v as usize;
        [self.red[idx], self.green[idx], self.blue[idx], 255]
    }

    /// Writes the 256 entries into mutable red, green, blue buffers.
    pub fn write(&self, red: &mut [u8; 256], green: &mut [u8; 256], blue: &mut [u8; 256]) {
        *red = self.red;
        *green = self.green;
        *blue = self.blue;
    }

    /// Parses an Apple XML `.plist` colour lookup table (with `<key>Red</key>`, `<key>Green</key>`, `<key>Blue</key>`).
    pub fn from_plist_xml(xml: &str) -> Option<Self> {
        fn parse_channel(xml: &str, key: &str) -> Option<[u8; 256]> {
            let key_tag = format!("<key>{key}</key>");
            let key_pos = xml.find(&key_tag)?;
            let after_key = &xml[key_pos + key_tag.len()..];
            let arr_start = after_key.find("<array>")?;
            let arr_end = after_key[arr_start..].find("</array>")?;
            let arr_body = &after_key[arr_start..arr_start + arr_end];

            let mut values = Vec::with_capacity(256);
            let mut rest = arr_body;
            while let Some(int_start) = rest.find("<integer>") {
                let after_open = &rest[int_start + "<integer>".len()..];
                let int_end = after_open.find("</integer>")?;
                let num_str = after_open[..int_end].trim();
                let n: i64 = num_str.parse().ok()?;
                values.push((n & 0xFF) as u8);
                rest = &after_open[int_end + "</integer>".len()..];
                if values.len() == 256 {
                    break;
                }
            }
            if values.len() < 256 {
                return None;
            }
            let mut out = [0u8; 256];
            out.copy_from_slice(&values[..256]);
            Some(out)
        }

        let red = parse_channel(xml, "Red")?;
        let green = parse_channel(xml, "Green")?;
        let blue = parse_channel(xml, "Blue")?;
        Some(Self::new(red, green, blue))
    }
}

/// What `ColourTableState::set` changed on a view (`ColourTableState.swift`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColourTableChange {
    Unchanged,
    GreyLevels,
    ColourTransfer,
}

/// Tracks whether a view is currently applying a colour transfer table (`ColourTableState.swift`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ColourTableState {
    pub color_transfer: bool,
}

impl ColourTableState {
    pub const fn new(color_transfer: bool) -> Self {
        Self { color_transfer }
    }

    pub fn is_grey_levels(red: &[u8; 256], green: &[u8; 256], blue: &[u8; 256]) -> bool {
        for i in 0..256 {
            let expected = i as u8;
            if red[i] != expected || green[i] != expected || blue[i] != expected {
                return false;
            }
        }
        true
    }

    pub fn matches(
        tables: (&[u8; 256], &[u8; 256], &[u8; 256]),
        request: (&[u8; 256], &[u8; 256], &[u8; 256]),
    ) -> bool {
        tables.0 == request.0 && tables.1 == request.1 && tables.2 == request.2
    }

    pub fn set(
        &mut self,
        red: &mut [u8; 256],
        green: &mut [u8; 256],
        blue: &mut [u8; 256],
        to: Option<(&[u8; 256], &[u8; 256], &[u8; 256])>,
        holds_colour_buffer: bool,
    ) -> ColourTableChange {
        let Some(request) = to else {
            if !holds_colour_buffer && !self.color_transfer {
                return ColourTableChange::Unchanged;
            }
            self.color_transfer = false;
            ColorLookupTable::grey_levels().write(red, green, blue);
            return ColourTableChange::GreyLevels;
        };

        if Self::matches((red, green, blue), request) {
            return ColourTableChange::Unchanged;
        }

        let mut is_grey = true;
        for i in 0..256 {
            let r = request.0[i];
            let g = request.1[i];
            let b = request.2[i];
            red[i] = r;
            green[i] = g;
            blue[i] = b;
            let expected = i as u8;
            if r != expected || g != expected || b != expected {
                is_grey = false;
            }
        }
        self.color_transfer = !is_grey;
        if is_grey {
            ColourTableChange::GreyLevels
        } else {
            ColourTableChange::ColourTransfer
        }
    }
}

/// Catalog of named colour lookup tables (`ColourTableBook.swift` + `DefaultPreferences+ImagePresets.swift`).
pub struct ColourTableBook;

impl ColourTableBook {
    pub const NONE_TITLE: &'static str = "No CLUT";
    pub const LINEAR_TITLE: &'static str = "Linear";

    /// Whether `name` selects identity grey levels rather than a stored table.
    pub fn is_grey_levels_name(name: &str) -> bool {
        name == Self::NONE_TITLE || name == Self::LINEAR_TITLE || name.is_empty()
    }

    /// Look up a factory colour table by name; returns `None` for `"No CLUT"` / `"Linear"` or unknown names.
    pub fn table_named(name: &str) -> Option<ColorLookupTable> {
        if Self::is_grey_levels_name(name) {
            return None;
        }
        Self::factory_tables()
            .into_iter()
            .find(|(n, _)| *n == name)
            .map(|(_, t)| t)
    }

    /// Ordered list of all factory CLUT names for the toolbar menu, starting with `"No CLUT"`.
    pub fn menu_names() -> Vec<&'static str> {
        let mut names = vec![
            Self::NONE_TITLE,
            "B/W Inverse",
            "PET",
            "HotIron",
            "Rainbow",
            "Rainbow2",
            "Rainbow3",
            "Jet",
            "Spectrum",
            "BlackBody",
            "Flow",
            "Perfusion",
            "French",
            "Endoscopy",
            "GEcolor",
            "NIH",
            "UCLA",
            "Stern",
            "HotMetal",
            "HotGreen",
            "GrayRainbow",
            "Ratio",
            "ired",
            "Hue1",
            "Hue2",
            "PPU Viridis",
            "PPU Plasma",
            "PPU Inferno",
            "PPU Magma",
            "VR Bones",
            "VR Muscles-Bones",
            "VR Red Vessels",
        ];
        names.dedup();
        names
    }

    /// All 31 factory colour tables from `DefaultPreferences+ImagePresets.swift`.
    pub fn factory_tables() -> Vec<(&'static str, ColorLookupTable)> {
        let mut out = Vec::with_capacity(31);

        // 1. PET
        let mut pet_r = [0u8; 256];
        let mut pet_g = [0u8; 256];
        let mut pet_b = [0u8; 256];
        for i in 0..256 {
            pet_r[i] = if i < 128 { (i * 2) as u8 } else { 255 };
            pet_g[i] = if i < 128 {
                0
            } else if i < 192 {
                ((i - 128) * 4) as u8
            } else {
                255
            };
            pet_b[i] = if i < 192 { 0 } else { ((i - 192) * 4) as u8 };
        }
        out.push(("PET", ColorLookupTable::new(pet_r, pet_g, pet_b)));

        // 2. B/W Inverse
        let mut inv = [0u8; 256];
        for i in 0..256 {
            inv[i] = (255 - i) as u8;
        }
        out.push(("B/W Inverse", ColorLookupTable::new(inv, inv, inv)));

        // 3. Endoscopy
        let mut endo_r = [0u8; 256];
        let mut endo_g = [0u8; 256];
        let mut endo_b = [0u8; 256];
        for i in 0..256 {
            endo_r[i] = (240.0 + (i as f64) * 0.02) as i64 as u8;
            endo_g[i] = (187.0 - (i as f64) * 0.26) as i64 as u8;
            endo_b[i] = (195.0 - (i as f64) * 0.26) as i64 as u8;
        }
        out.push(("Endoscopy", ColorLookupTable::new(endo_r, endo_g, endo_b)));

        // 4. French (stored with red and blue exchanged, matching DefaultPreferences+ImagePresets.swift)
        out.push((
            "French",
            ColorLookupTable::new(FRENCH_BLUE, FRENCH_GREEN, FRENCH_RED),
        ));

        // 5. Perfusion (stored with red and blue exchanged, matching DefaultPreferences+ImagePresets.swift)
        out.push((
            "Perfusion",
            ColorLookupTable::new(PERFUSION_BLUE, PERFUSION_GREEN, PERFUSION_RED),
        ));

        // 6..31. The 26 plist tables
        for &(name, r, g, b) in PLIST_CLUTS {
            out.push((name, ColorLookupTable::new(r, g, b)));
        }

        out
    }
}

const FRENCH_RED: [u8; 256] = [
    0, 0, 2, 4, 4, 6, 8, 10, 12, 14, 16, 18, 20, 22, 24, 26, 28, 30, 32, 32, 34, 36, 36, 38, 40,
    40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 38, 36, 36, 34, 32, 32, 30, 28,
    28, 26, 24, 24, 22, 20, 20, 18, 16, 16, 14, 12, 12, 10, 10, 10, 8, 8, 8, 6, 6, 6, 4, 4, 4, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 2, 148, 168, 176, 184, 192, 200, 208, 214, 220, 224, 226, 248,
    250, 252, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254,
    254, 252, 250, 250, 248, 246, 246, 242, 240, 238, 236, 234, 232, 230, 228, 226, 224, 220, 218,
    216, 214, 214, 214, 214, 214, 214, 214, 214, 214, 214, 214, 214, 214, 214, 214, 214, 214, 214,
    216, 216, 216, 218, 216, 216, 214, 214, 214, 214, 212, 212, 212, 210, 210, 210, 208, 208, 208,
    206, 206, 206, 206, 206, 206, 206, 206, 206, 206, 206, 206, 206, 206, 206, 206, 206, 206, 206,
    204, 204, 204, 204, 204, 204, 204, 204, 204, 206, 208, 210, 210, 214, 216, 218, 222, 226, 230,
    232, 236, 238, 240, 240, 240, 240, 240, 240, 240, 240, 240,
];

const FRENCH_GREEN: [u8; 256] = [
    0, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 22, 24, 28, 30, 32, 36, 38, 40, 44, 46, 48, 52, 54, 56,
    60, 62, 64, 68, 70, 72, 76, 78, 80, 84, 86, 88, 92, 94, 96, 100, 102, 104, 108, 110, 112, 116,
    118, 120, 124, 126, 128, 132, 134, 136, 140, 142, 144, 146, 148, 150, 152, 154, 156, 158, 160,
    162, 162, 162, 162, 162, 162, 162, 162, 162, 162, 160, 158, 158, 156, 154, 154, 152, 150, 150,
    148, 146, 146, 144, 142, 142, 140, 140, 140, 130, 128, 128, 126, 126, 126, 126, 126, 126, 128,
    128, 130, 130, 134, 138, 142, 148, 152, 158, 164, 168, 174, 180, 184, 190, 196, 202, 206, 212,
    218, 222, 228, 234, 238, 250, 250, 250, 250, 250, 250, 250, 250, 250, 250, 250, 250, 250, 250,
    250, 250, 250, 250, 250, 250, 250, 250, 226, 218, 210, 202, 194, 186, 178, 170, 162, 154, 146,
    138, 130, 122, 114, 106, 98, 90, 82, 74, 66, 58, 50, 42, 34, 26, 18, 10, 2, 2, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 2, 4, 4, 4, 6, 6, 6, 8, 8, 8,
    10, 10, 10, 12, 14, 14, 16, 18, 18, 62, 34, 42, 50, 58, 66, 74, 82, 90, 98, 106, 114, 122, 130,
    138, 144, 152, 160, 168, 176, 184, 192, 200, 208, 216, 224, 232, 240, 248, 254,
];

const FRENCH_BLUE: [u8; 256] = [
    0, 6, 10, 16, 20, 24, 30, 34, 38, 44, 48, 54, 58, 62, 68, 72, 76, 82, 86, 92, 96, 100, 106,
    110, 114, 120, 124, 130, 134, 138, 144, 148, 152, 152, 152, 150, 148, 146, 144, 142, 140, 138,
    136, 134, 132, 130, 128, 128, 126, 126, 124, 122, 122, 120, 118, 118, 116, 114, 114, 112, 110,
    110, 108, 106, 106, 106, 104, 102, 98, 96, 94, 90, 88, 86, 84, 82, 80, 78, 76, 74, 72, 70, 68,
    66, 64, 62, 58, 56, 54, 48, 40, 38, 38, 36, 36, 36, 36, 36, 26, 24, 18, 16, 12, 10, 8, 6, 6, 4,
    4, 2, 2, 2, 4, 4, 4, 6, 6, 6, 8, 8, 8, 10, 10, 10, 12, 12, 12, 14, 14, 14, 14, 48, 60, 60, 60,
    60, 60, 60, 60, 60, 60, 60, 60, 60, 56, 52, 44, 36, 28, 20, 12, 8, 4, 4, 2, 2, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 2, 4,
    4, 6, 6, 6, 8, 8, 24, 32, 122, 124, 126, 128, 130, 132, 134, 136, 138, 140, 142, 144, 146, 148,
    150, 152, 154, 156, 158, 158, 160, 160, 162, 162, 164, 164, 164, 164, 164, 164, 164, 164, 164,
    164, 164, 164, 164, 246, 248, 248, 250, 250, 252, 252, 244, 248, 252, 254, 254, 254, 254, 254,
    254, 254, 254,
];

const PERFUSION_RED: [u8; 256] = [
    0, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 6, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
    2, 2, 2, 2, 2, 2, 2, 6, 8, 10, 14, 16, 18, 22, 24, 26, 28, 32, 34, 36, 40, 42, 44, 48, 50, 52,
    54, 58, 60, 62, 66, 68, 70, 74, 76, 78, 80, 84, 86, 88, 92, 94, 96, 98, 102, 104, 106, 110, 112,
    114, 118, 120, 122, 124, 128, 130, 132, 136, 138, 140, 144, 146, 148, 150, 154, 156, 158, 162,
    164, 166, 168, 172, 174, 176, 180, 182, 184, 188, 190, 192, 194, 198, 200, 202, 206, 208, 210,
    214, 216, 218, 220, 224, 226, 228, 232, 234, 236, 240, 242, 244, 246, 250, 252, 254, 254, 254,
    254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254,
    254, 254, 254,
];

const PERFUSION_GREEN: [u8; 256] = [
    0, 6, 8, 10, 12, 14, 14, 16, 18, 20, 22, 22, 24, 26, 28, 30, 30, 32, 34, 36, 38, 38, 40, 42, 44,
    46, 46, 48, 50, 52, 54, 54, 56, 58, 60, 62, 62, 64, 66, 68, 70, 70, 72, 74, 76, 78, 78, 76, 78,
    76, 76, 74, 74, 72, 72, 70, 68, 68, 66, 66, 64, 64, 62, 60, 60, 58, 58, 56, 56, 54, 52, 52, 50,
    50, 48, 48, 46, 46, 44, 42, 42, 40, 40, 38, 38, 36, 34, 34, 32, 32, 30, 30, 28, 26, 26, 24, 24,
    22, 22, 20, 18, 18, 16, 16, 14, 14, 12, 12, 10, 8, 8, 6, 6, 4, 4, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
    2, 2, 2, 0, 4, 6, 10, 12, 14, 18, 20, 22, 26, 28, 30, 34, 36, 38, 42, 44, 46, 50, 52, 54, 58,
    60, 62, 66, 68, 70, 74, 76, 78, 82, 84, 86, 90, 92, 94, 98, 100, 102, 106, 108, 110, 114, 116,
    118, 122, 124, 126, 130, 132, 134, 138, 140, 142, 146, 148, 150, 154, 156, 158, 162, 164, 166,
    170, 172, 174, 178, 180, 182, 186, 188, 190, 194, 196, 198, 202, 204, 206, 210, 212, 214, 218,
    220, 222, 226, 228, 230, 234, 236, 238, 242, 244, 246, 250, 252, 254,
];

const PERFUSION_BLUE: [u8; 256] = [
    0, 6, 12, 16, 22, 26, 32, 36, 42, 46, 52, 56, 62, 68, 72, 78, 82, 88, 92, 98, 102, 108, 112,
    118, 122, 128, 134, 138, 144, 148, 154, 158, 164, 168, 174, 178, 184, 188, 194, 200, 204, 210,
    214, 220, 224, 230, 234, 240, 244, 250, 252, 250, 246, 244, 240, 238, 234, 232, 230, 226, 224,
    220, 218, 214, 212, 210, 206, 204, 200, 198, 194, 192, 190, 186, 184, 180, 178, 174, 172, 170,
    166, 164, 160, 158, 154, 152, 150, 146, 144, 140, 138, 134, 132, 130, 126, 124, 120, 118, 114,
    112, 110, 106, 104, 100, 98, 94, 92, 90, 86, 84, 80, 78, 74, 72, 70, 66, 64, 60, 58, 54, 52, 50,
    46, 44, 40, 38, 34, 32, 30, 26, 24, 20, 18, 14, 12, 10, 6, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 14, 28,
    42, 56, 68, 82, 96, 110, 124, 136, 150, 164, 178, 190,
];

#[cfg(test)]
mod tests {
    use super::*;

    fn grey() -> [u8; 256] {
        let mut a = [0u8; 256];
        for i in 0..256 {
            a[i] = i as u8;
        }
        a
    }

    fn hot() -> [u8; 256] {
        let mut a = [0u8; 256];
        for i in 0..256 {
            a[i] = (i * 3).min(255) as u8;
        }
        a
    }

    fn inverse() -> [u8; 256] {
        let mut a = [0u8; 256];
        for i in 0..256 {
            a[i] = (255 - i) as u8;
        }
        a
    }

    #[test]
    fn grey_levels_are_identity_of_every_channel() {
        let g = grey();
        let h = hot();
        let inv = inverse();
        let mut almost = g;
        almost[200] = 201;

        assert!(ColorLookupTable::grey_levels().is_grey_levels());
        assert_eq!(ColorLookupTable::grey_levels().red, g);
        assert_eq!(ColorLookupTable::grey_levels().blue, g);
        assert!(!ColorLookupTable::new(g, almost, g).is_grey_levels());
        assert!(!ColorLookupTable::new(inv, inv, inv).is_grey_levels());
        assert!(ColourTableState::is_grey_levels(&g, &g, &g));
        assert!(!ColourTableState::is_grey_levels(&g, &g, &almost));
        assert!(!ColourTableState::is_grey_levels(&h, &inv, &g));
    }

    #[test]
    fn grey_request_of_grey_view_writes_nothing() {
        let mut state = ColourTableState::default();
        let h = hot();
        let mut r = h;
        let mut g = h;
        let mut b = h;
        let change = state.set(&mut r, &mut g, &mut b, None, false);
        assert_eq!(change, ColourTableChange::Unchanged);
        assert_eq!(r, h);
        assert!(!state.color_transfer);
    }

    #[test]
    fn factory_catalogue_has_all_31_horos_cluts() {
        let tables = ColourTableBook::factory_tables();
        assert_eq!(tables.len(), 31);
        for name in [
            "PET",
            "B/W Inverse",
            "Endoscopy",
            "French",
            "Perfusion",
            "HotIron",
            "Rainbow",
            "Jet",
            "PPU Viridis",
            "VR Bones",
        ] {
            let t = ColourTableBook::table_named(name).expect(name);
            assert!(!t.is_grey_levels());
        }
        assert!(ColourTableBook::table_named("No CLUT").is_none());
        assert!(ColourTableBook::table_named("Linear").is_none());
    }
}
