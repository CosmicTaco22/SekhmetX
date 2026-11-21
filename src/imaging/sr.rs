//! Groundwork for DICOM Structured Reports (SR) and Grayscale Softcopy
//! Presentation States (GSPS / PR) — `annotations-sr` area in `PORTING_MAP.md`.
//!
//! Provides recursive `ContentSequence` `(0040,A730)` extraction into a readable
//! structured text tree and basic GSPS presentation state extraction (window,
//! rotation, flip, graphic annotations) from `DefaultDicomObject`.

use dicom_dictionary_std::tags;
use dicom_object::DefaultDicomObject;

use crate::imaging::Window;

/// One node in a DICOM Structured Report content tree.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrNode {
    pub relationship: String,
    pub value_type: String,
    pub concept_name: String,
    pub value: String,
    pub children: Vec<SrNode>,
}

/// Extracted Grayscale Softcopy Presentation State (GSPS / `PR`) summary.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GspsState {
    pub label: String,
    pub description: String,
    pub window: Option<Window>,
    pub rotation_deg: i32,
    pub horizontal_flip: bool,
    pub invert: bool,
    pub referenced_sop_uids: Vec<String>,
    pub text_annotations: Vec<String>,
}

#[allow(dead_code)]
impl SrNode {
    /// Extracts the SR document tree from a DICOM dataset (returns `None` if not an SR / empty).
    pub fn from_dicom(obj: &DefaultDicomObject) -> Option<Self> {
        let vt = obj
            .element_opt(tags::VALUE_TYPE)
            .ok()
            .flatten()
            .and_then(|e| e.to_str().ok().map(|s| s.trim().to_string()))
            .unwrap_or_default();
        if vt.is_empty() && obj.element_opt(tags::CONTENT_SEQUENCE).ok().flatten().is_none() {
            return None;
        }
        Some(parse_sr_item(obj, 0))
    }

    /// Formats the SR tree into indented plain text for viewing or export.
    pub fn format_plain_text(&self) -> String {
        let mut out = String::new();
        self.write_indented(&mut out, 0);
        out
    }

    fn write_indented(&self, out: &mut String, depth: usize) {
        let indent = "  ".repeat(depth);
        if !self.concept_name.is_empty() && !self.value.is_empty() {
            out.push_str(&format!("{indent}{}: {}\n", self.concept_name, self.value));
        } else if !self.concept_name.is_empty() {
            out.push_str(&format!("{indent}{}\n", self.concept_name));
        } else if !self.value.is_empty() {
            out.push_str(&format!("{indent}{}\n", self.value));
        }
        for c in &self.children {
            c.write_indented(out, depth + 1);
        }
    }
}

fn parse_sr_item(obj: &DefaultDicomObject, depth: usize) -> SrNode {
    let get_s = |tag| {
        obj.element_opt(tag)
            .ok()
            .flatten()
            .and_then(|e| e.to_str().ok().map(|s| s.trim().to_string()))
            .unwrap_or_default()
    };
    let relationship = get_s(tags::RELATIONSHIP_TYPE);
    let value_type = get_s(tags::VALUE_TYPE);
    let concept_name = obj
        .element_opt(tags::CONCEPT_NAME_CODE_SEQUENCE)
        .ok()
        .flatten()
        .and_then(|seq| seq.items()?.first())
        .and_then(|item| item.element_opt(tags::CODE_MEANING).ok().flatten())
        .and_then(|e| e.to_str().ok().map(|s| s.trim().to_string()))
        .unwrap_or_default();

    let value = match value_type.as_str() {
        "TEXT" => get_s(tags::TEXT_VALUE),
        "PNAME" => get_s(tags::PERSON_NAME),
        "DATE" => get_s(tags::DATE),
        "TIME" => get_s(tags::TIME),
        "DATETIME" => get_s(tags::DATE_TIME),
        "UIDREF" => get_s(tags::UID),
        "CODE" => obj
            .element_opt(tags::CONCEPT_CODE_SEQUENCE)
            .ok()
            .flatten()
            .and_then(|seq| seq.items()?.first())
            .and_then(|item| item.element_opt(tags::CODE_MEANING).ok().flatten())
            .and_then(|e| e.to_str().ok().map(|s| s.trim().to_string()))
            .unwrap_or_default(),
        "NUM" => obj
            .element_opt(tags::MEASURED_VALUE_SEQUENCE)
            .ok()
            .flatten()
            .and_then(|seq| seq.items()?.first())
            .map(|item| {
                let num = item
                    .element_opt(tags::NUMERIC_VALUE)
                    .ok()
                    .flatten()
                    .and_then(|e| e.to_str().ok().map(|s| s.trim().to_string()))
                    .unwrap_or_default();
                let unit = item
                    .element_opt(tags::MEASUREMENT_UNITS_CODE_SEQUENCE)
                    .ok()
                    .flatten()
                    .and_then(|u| u.items()?.first())
                    .and_then(|u| u.element_opt(tags::CODE_VALUE).ok().flatten())
                    .and_then(|e| e.to_str().ok().map(|s| s.trim().to_string()))
                    .unwrap_or_default();
                if unit.is_empty() { num } else { format!("{num} {unit}") }
            })
            .unwrap_or_default(),
        _ => String::new(),
    };

    let mut children = Vec::new();
    if depth < 16
        && let Some(seq) = obj.element_opt(tags::CONTENT_SEQUENCE).ok().flatten()
        && let Some(items) = seq.items()
    {
        for item in items {
            // InMemDicomObject items share the same element_opt API via DefaultDicomObject
            // only if we read directly; here we parse recursively on InMemDicomObject.
            children.push(parse_sr_inmem(item, depth + 1));
        }
    }

    SrNode {
        relationship,
        value_type,
        concept_name,
        value,
        children,
    }
}

fn parse_sr_inmem(obj: &dicom_object::InMemDicomObject, depth: usize) -> SrNode {
    let get_s = |tag| {
        obj.element_opt(tag)
            .ok()
            .flatten()
            .and_then(|e| e.to_str().ok().map(|s| s.trim().to_string()))
            .unwrap_or_default()
    };
    let relationship = get_s(tags::RELATIONSHIP_TYPE);
    let value_type = get_s(tags::VALUE_TYPE);
    let concept_name = obj
        .element_opt(tags::CONCEPT_NAME_CODE_SEQUENCE)
        .ok()
        .flatten()
        .and_then(|seq| seq.items()?.first())
        .and_then(|item| item.element_opt(tags::CODE_MEANING).ok().flatten())
        .and_then(|e| e.to_str().ok().map(|s| s.trim().to_string()))
        .unwrap_or_default();
    let value = match value_type.as_str() {
        "TEXT" => get_s(tags::TEXT_VALUE),
        "PNAME" => get_s(tags::PERSON_NAME),
        "DATE" => get_s(tags::DATE),
        "TIME" => get_s(tags::TIME),
        "DATETIME" => get_s(tags::DATE_TIME),
        "UIDREF" => get_s(tags::UID),
        "CODE" => obj
            .element_opt(tags::CONCEPT_CODE_SEQUENCE)
            .ok()
            .flatten()
            .and_then(|seq| seq.items()?.first())
            .and_then(|item| item.element_opt(tags::CODE_MEANING).ok().flatten())
            .and_then(|e| e.to_str().ok().map(|s| s.trim().to_string()))
            .unwrap_or_default(),
        "NUM" => obj
            .element_opt(tags::MEASURED_VALUE_SEQUENCE)
            .ok()
            .flatten()
            .and_then(|seq| seq.items()?.first())
            .map(|item| {
                let num = item
                    .element_opt(tags::NUMERIC_VALUE)
                    .ok()
                    .flatten()
                    .and_then(|e| e.to_str().ok().map(|s| s.trim().to_string()))
                    .unwrap_or_default();
                let unit = item
                    .element_opt(tags::MEASUREMENT_UNITS_CODE_SEQUENCE)
                    .ok()
                    .flatten()
                    .and_then(|u| u.items()?.first())
                    .and_then(|u| u.element_opt(tags::CODE_VALUE).ok().flatten())
                    .and_then(|e| e.to_str().ok().map(|s| s.trim().to_string()))
                    .unwrap_or_default();
                if unit.is_empty() { num } else { format!("{num} {unit}") }
            })
            .unwrap_or_default(),
        _ => String::new(),
    };
    let mut children = Vec::new();
    if depth < 16
        && let Some(seq) = obj.element_opt(tags::CONTENT_SEQUENCE).ok().flatten()
        && let Some(items) = seq.items()
    {
        for item in items {
            children.push(parse_sr_inmem(item, depth + 1));
        }
    }
    SrNode {
        relationship,
        value_type,
        concept_name,
        value,
        children,
    }
}

#[allow(dead_code)]
impl GspsState {
    /// Reads basic GSPS presentation state fields from a `PR` DICOM dataset.
    pub fn from_dicom(obj: &DefaultDicomObject) -> Option<Self> {
        let modality = obj
            .element_opt(tags::MODALITY)
            .ok()
            .flatten()
            .and_then(|e| e.to_str().ok().map(|s| s.trim().to_string()))
            .unwrap_or_default();
        if !modality.eq_ignore_ascii_case("PR") {
            return None;
        }
        let get_s = |tag| {
            obj.element_opt(tag)
                .ok()
                .flatten()
                .and_then(|e| e.to_str().ok().map(|s| s.trim().to_string()))
                .unwrap_or_default()
        };
        let label = get_s(tags::CONTENT_LABEL);
        let description = get_s(tags::CONTENT_DESCRIPTION);
        let rotation_deg = obj
            .element_opt(tags::IMAGE_ROTATION)
            .ok()
            .flatten()
            .and_then(|e| e.to_int::<i32>().ok())
            .unwrap_or(0);
        let horizontal_flip = get_s(tags::IMAGE_HORIZONTAL_FLIP).eq_ignore_ascii_case("Y");
        let invert = get_s(tags::PRESENTATION_LUT_SHAPE).eq_ignore_ascii_case("INVERSE");

        let window = obj
            .element_opt(tags::SOFTCOPY_VOILUT_SEQUENCE)
            .ok()
            .flatten()
            .and_then(|seq| seq.items()?.first())
            .and_then(|item| {
                let c = item.element_opt(tags::WINDOW_CENTER).ok().flatten()?.to_float32().ok()?;
                let w = item.element_opt(tags::WINDOW_WIDTH).ok().flatten()?.to_float32().ok()?;
                (w > 0.0).then_some(Window::new(c, w))
            });

        let mut text_annotations = Vec::new();
        if let Some(seq) = obj.element_opt(tags::GRAPHIC_ANNOTATION_SEQUENCE).ok().flatten()
            && let Some(items) = seq.items()
        {
            for item in items {
                if let Some(tseq) = item.element_opt(tags::TEXT_OBJECT_SEQUENCE).ok().flatten()
                    && let Some(titems) = tseq.items()
                {
                    for ti in titems {
                        if let Some(txt) = ti
                            .element_opt(tags::UNFORMATTED_TEXT_VALUE)
                            .ok()
                            .flatten()
                            .and_then(|e| e.to_str().ok().map(|s| s.trim().to_string()))
                            .filter(|s| !s.is_empty())
                        {
                            text_annotations.push(txt);
                        }
                    }
                }
            }
        }

        Some(Self {
            label,
            description,
            window,
            rotation_deg,
            horizontal_flip,
            invert,
            referenced_sop_uids: Vec::new(),
            text_annotations,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sr_node_formats_indented_tree() {
        let root = SrNode {
            relationship: String::new(),
            value_type: "CONTAINER".into(),
            concept_name: "CT Dose Report".into(),
            value: String::new(),
            children: vec![SrNode {
                relationship: "CONTAINS".into(),
                value_type: "NUM".into(),
                concept_name: "DLP".into(),
                value: "412.5 mGy.cm".into(),
                children: Vec::new(),
            }],
        };
        let txt = root.format_plain_text();
        assert_eq!(txt, "CT Dose Report\n  DLP: 412.5 mGy.cm\n");
    }
}
