//! DICOMweb QIDO-RS (study search) and WADO-RS (study retrieve) client over `ureq`.
//!
//! Covers the `dicomweb` area in `PORTING_MAP.md` (PS 3.18).

use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde_json::Value;

use super::{DicomWebAuth, DicomWebServer, QueryFilter, StudyQueryResult};

fn build_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .build()
        .into()
}

fn auth_header_value(auth: DicomWebAuth, secret: Option<&str>) -> Option<String> {
    let s = secret?.trim();
    if s.is_empty() {
        return None;
    }
    match auth {
        DicomWebAuth::None => None,
        DicomWebAuth::BearerToken => Some(format!("Bearer {s}")),
        DicomWebAuth::Basic => Some(format!("Basic {s}")),
    }
}

/// Extracts the first string value of a DICOM JSON tag (supporting `Alphabetic` PN objects).
fn dicom_json_str(item: &Value, tag: &str) -> String {
    let Some(arr) = item.get(tag).and_then(|t| t.get("Value")).and_then(|v| v.as_array()) else {
        return String::new();
    };
    let mut parts = Vec::new();
    for v in arr {
        if let Some(s) = v.as_str() {
            parts.push(s.to_string());
        } else if let Some(pn) = v.get("Alphabetic").and_then(|a| a.as_str()) {
            parts.push(pn.to_string());
        } else if let Some(n) = v.as_i64() {
            parts.push(n.to_string());
        } else if let Some(n) = v.as_u64() {
            parts.push(n.to_string());
        }
    }
    parts.join("\\")
}

fn dicom_json_u32(item: &Value, tag: &str) -> u32 {
    let Some(first) = item
        .get(tag)
        .and_then(|t| t.get("Value"))
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
    else {
        return 0;
    };
    first
        .as_u64()
        .map(|n| n as u32)
        .or_else(|| first.as_str().and_then(|s| s.trim().parse::<u32>().ok()))
        .unwrap_or(0)
}

/// Parses a QIDO-RS `application/dicom+json` study array into `StudyQueryResult` items.
pub fn parse_qido_studies_json(body: &str) -> Result<Vec<StudyQueryResult>> {
    if body.trim().is_empty() {
        return Ok(Vec::new());
    }
    let root: Value = serde_json::from_str(body).context("invalid QIDO-RS JSON response")?;
    let arr = root
        .as_array()
        .ok_or_else(|| anyhow!("QIDO-RS response was not a JSON array"))?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let study_uid = dicom_json_str(item, "0020000D");
        if study_uid.is_empty() {
            continue;
        }
        out.push(StudyQueryResult {
            study_uid,
            patient_name: dicom_json_str(item, "00100010"),
            patient_id: dicom_json_str(item, "00100020"),
            study_date: dicom_json_str(item, "00080020"),
            modalities: dicom_json_str(item, "00080061"),
            description: dicom_json_str(item, "00081030"),
            accession_number: dicom_json_str(item, "00080050"),
            num_series: dicom_json_u32(item, "00201206"),
            num_instances: dicom_json_u32(item, "00201208"),
        });
    }
    Ok(out)
}

/// Queries `server` for matching studies via QIDO-RS (`GET /studies?...`).
pub fn qido_search_studies(
    server: &DicomWebServer,
    secret: Option<&str>,
    filter: &QueryFilter,
) -> Result<Vec<StudyQueryResult>> {
    let url = server.qido_studies_url(filter);
    let agent = build_agent();
    let mut req = agent.get(&url).header("Accept", "application/dicom+json");
    if let Some(auth) = auth_header_value(server.auth, secret) {
        req = req.header("Authorization", &auth);
    }
    let mut resp = req
        .call()
        .with_context(|| format!("QIDO-RS request failed for {url}"))?;
    let body = resp
        .body_mut()
        .read_to_string()
        .context("failed to read QIDO-RS response body")?;
    parse_qido_studies_json(&body)
}

/// Splits a `multipart/related` WADO-RS body by `boundary`, returning each part's raw payload.
pub fn extract_multipart_dicom(body: &[u8], boundary: &str) -> Vec<Vec<u8>> {
    let delim = format!("--{boundary}");
    let delim_bytes = delim.as_bytes();
    let mut positions = Vec::new();
    let mut i = 0;
    while i + delim_bytes.len() <= body.len() {
        if &body[i..i + delim_bytes.len()] == delim_bytes {
            positions.push(i);
            i += delim_bytes.len();
        } else {
            i += 1;
        }
    }
    let mut parts = Vec::new();
    for w in positions.windows(2) {
        let start = w[0] + delim_bytes.len();
        let end = w[1];
        if start >= end {
            continue;
        }
        let chunk = &body[start..end];
        // Find end of MIME headers (\r\n\r\n or \n\n)
        let payload = if let Some(pos) = chunk.windows(4).position(|p| p == b"\r\n\r\n") {
            &chunk[pos + 4..]
        } else if let Some(pos) = chunk.windows(2).position(|p| p == b"\n\n") {
            &chunk[pos + 2..]
        } else {
            continue;
        };
        // Trim trailing CRLF before next boundary
        let trimmed = payload
            .strip_suffix(b"\r\n")
            .or_else(|| payload.strip_suffix(b"\n"))
            .unwrap_or(payload);
        if !trimmed.is_empty() {
            parts.push(trimmed.to_vec());
        }
    }
    parts
}

fn parse_boundary(content_type: &str) -> Option<String> {
    for part in content_type.split(';') {
        let trimmed = part.trim();
        if let Some(rest) = trimmed
            .strip_prefix("boundary=")
            .or_else(|| trimmed.strip_prefix("BOUNDARY="))
        {
            return Some(rest.trim_matches('"').to_string());
        }
    }
    None
}

/// Retrieves all DICOM instances of `study_uid` from `server` via WADO-RS
/// (`GET /studies/{study_uid}`) and saves them as `.dcm` files in `dest_dir`.
pub fn wado_retrieve_study(
    server: &DicomWebServer,
    secret: Option<&str>,
    study_uid: &str,
    dest_dir: &Path,
) -> Result<usize> {
    std::fs::create_dir_all(dest_dir)?;
    let base = server.base_url.trim_end_matches('/');
    let url = format!("{base}/studies/{study_uid}");
    let agent = build_agent();
    let mut req = agent
        .get(&url)
        .header("Accept", "multipart/related; type=\"application/dicom\"");
    if let Some(auth) = auth_header_value(server.auth, secret) {
        req = req.header("Authorization", &auth);
    }
    let mut resp = req
        .call()
        .with_context(|| format!("WADO-RS retrieve failed for {url}"))?;
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let bytes = resp
        .body_mut()
        .with_config()
        .limit(512 * 1024 * 1024)
        .read_to_vec()
        .context("failed to read WADO-RS study payload")?;

    let parts = if let Some(boundary) = parse_boundary(&ct) {
        extract_multipart_dicom(&bytes, &boundary)
    } else if !bytes.is_empty() {
        vec![bytes]
    } else {
        Vec::new()
    };

    for (idx, part) in parts.iter().enumerate() {
        let out = dest_dir.join(format!("inst_{idx:05}.dcm"));
        std::fs::write(&out, part)?;
    }
    Ok(parts.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qido_json_parses_standard_dicom_study_array() {
        let sample = r#"[
            {
                "0020000D": { "vr": "UI", "Value": ["1.2.840.113619.2.55"] },
                "00100010": { "vr": "PN", "Value": [{ "Alphabetic": "DOE^JANE" }] },
                "00100020": { "vr": "LO", "Value": ["PAT001"] },
                "00080020": { "vr": "DA", "Value": ["20261121"] },
                "00080061": { "vr": "CS", "Value": ["CT", "SR"] },
                "00081030": { "vr": "LO", "Value": ["CT CHEST"] },
                "00080050": { "vr": "SH", "Value": ["ACC123"] },
                "00201206": { "vr": "IS", "Value": [4] },
                "00201208": { "vr": "IS", "Value": ["240"] }
            }
        ]"#;
        let res = parse_qido_studies_json(sample).unwrap();
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].study_uid, "1.2.840.113619.2.55");
        assert_eq!(res[0].patient_name, "DOE^JANE");
        assert_eq!(res[0].modalities, "CT\\SR");
        assert_eq!(res[0].num_series, 4);
        assert_eq!(res[0].num_instances, 240);
    }

    #[test]
    fn multipart_related_splits_dicom_parts_cleanly() {
        let raw = b"--mybound\r\nContent-Type: application/dicom\r\n\r\nPART_ONE_DATA\r\n--mybound\r\nContent-Type: application/dicom\r\n\r\nPART_TWO\r\n--mybound--\r\n";
        let parts = extract_multipart_dicom(raw, "mybound");
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], b"PART_ONE_DATA");
        assert_eq!(parts[1], b"PART_TWO");
    }
}
