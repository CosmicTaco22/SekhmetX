//! Window presets, their names and their hot keys.
//!
//! Follows IsiX `ViewerWindowLevel/WindowLevelPresetCatalog.swift`,
//! `ViewSeries/SeriesDisplay.swift` (`WindowLevelPreset.match`) and the factory
//! presets in `IsiXPreferences/DefaultPreferences+ImagePresets.swift`.

use std::cmp::Ordering;

use super::Window;

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub name: String,
    /// `None` when the stored entry isn't a level and a width: it names no
    /// window and applies none, but keeps its place in the hot-key order.
    pub window: Option<Window>,
}

/// What the window on screen is called.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Name {
    Preset(String),
    FullDynamic,
    ImageDefault,
    Other,
}

impl Name {
    pub fn label(&self) -> &str {
        match self {
            Name::Preset(n) => n,
            Name::FullDynamic => "Full dynamic",
            Name::ImageDefault => "Default",
            Name::Other => "Other",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Catalog {
    /// In the order the preference gave them (Horos: its dictionary's order).
    pub entries: Vec<Entry>,
}

impl Catalog {
    /// Horos's factory presets: [level, width].
    pub fn factory() -> Self {
        let p = |name: &str, level: f32, width: f32| Entry { name: name.into(), window: Some(Window::new(level, width)) };
        Self {
            entries: vec![
                p("CT - Abdomen", 40.0, 350.0),
                p("CT - Bone", 300.0, 1500.0),
                p("CT - Brain", 50.0, 100.0),
                p("CT - Pulmonary", -500.0, 1400.0),
                p("VR - Endoscopy", -300.0, 700.0),
            ],
        }
    }

    /// The name of the window on screen: the first preset with exactly this
    /// level and width; then the image's full dynamic and its default, when
    /// both are known.
    pub fn name(&self, window: Window, full_dynamic: Option<Window>, image_default: Option<Window>) -> Name {
        for entry in &self.entries {
            if let Some(w) = entry.window {
                if w.level == window.level && w.width == window.width {
                    return Name::Preset(entry.name.clone());
                }
            }
        }
        if let (Some(full), Some(default)) = (full_dynamic, image_default) {
            if window == full {
                return Name::FullDynamic;
            }
            if window == default {
                return Name::ImageDefault;
            }
        }
        Name::Other
    }

    /// The presets ordered by name, ignoring case: the order of hot keys 1–9.
    /// Names that differ only in case keep the order of their characters.
    pub fn hot_key_order(&self) -> Vec<&Entry> {
        let mut ordered: Vec<&Entry> = self.entries.iter().collect();
        ordered.sort_by(|a, b| match a.name.to_lowercase().cmp(&b.name.to_lowercase()) {
            Ordering::Equal => a.name.cmp(&b.name),
            other => other,
        });
        ordered
    }

    /// The preset of hot key `index` (0 for the key 1).
    pub fn hot_key(&self, index: usize) -> Option<&Entry> {
        self.hot_key_order().into_iter().nth(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, window: Option<(f32, f32)>) -> Entry {
        Entry { name: name.into(), window: window.map(|(l, w)| Window::new(l, w)) }
    }

    // IsiX WindowLevelPresetTests.theHotKeysCountThePresetsAsTheViewDid (the parts that don't need the plist)
    #[test]
    fn hot_keys_follow_names_ignoring_case() {
        let cased = Catalog { entries: vec![entry("lung", None), entry("Lung", None), entry("abd", None)] };
        let names: Vec<&str> = cased.hot_key_order().iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["abd", "Lung", "lung"]);
        let c = Catalog { entries: vec![entry("CT - Bone", Some((300.0, 1500.0))), entry("bone wide", Some((300.0, 3000.0))), entry("Broken", None)] };
        assert_eq!(c.hot_key(0).unwrap().name, "bone wide");
        assert_eq!(c.hot_key(1).unwrap().name, "Broken");
        assert!(c.hot_key(1).unwrap().window.is_none());
        assert!(c.hot_key(3).is_none());
    }

    #[test]
    fn horos_factory_hot_keys() {
        let c = Catalog::factory();
        let names: Vec<&str> = c.hot_key_order().iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["CT - Abdomen", "CT - Bone", "CT - Brain", "CT - Pulmonary", "VR - Endoscopy"]);
    }

    // IsiX WindowLevelPresetTests.namesTheWindowAsTheViewDid (cases with an explicit expectation)
    #[test]
    fn names_the_window_as_the_view_did() {
        let c = Catalog::factory();
        let full = Some(Window::new(100.0, 4000.0));
        let default = Some(Window::new(20.0, 30.0));
        assert_eq!(c.name(Window::new(-500.0, 1400.0), None, None), Name::Preset("CT - Pulmonary".into()));
        assert_eq!(c.name(Window::new(40.0, 350.0), full, default), Name::Preset("CT - Abdomen".into()));
        assert_eq!(c.name(Window::new(100.0, 4000.0), full, default), Name::FullDynamic);
        assert_eq!(c.name(Window::new(20.0, 30.0), full, default), Name::ImageDefault);
        // Without an image, full dynamic and default aren't known.
        assert_eq!(c.name(Window::new(100.0, 4000.0), None, None), Name::Other);
        assert_eq!(c.name(Window::new(40.5, 80.25), full, default), Name::Other);
        // The first preset of two with one window names it.
        let twice = Catalog { entries: vec![entry("first", Some((1.0, 1.0))), entry("second", Some((1.0, 1.0)))] };
        assert_eq!(twice.name(Window::new(1.0, 1.0), None, None), Name::Preset("first".into()));
    }
}
