use super::*;
use std::io::{Read, Write};

const MAX_PACS_DOWNLOAD_BYTES: u64 = 4 * 1024 * 1024 * 1024;

fn pacs_http_agent(read_timeout_secs: u64) -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .timeout_connect(Some(std::time::Duration::from_secs(15)))
        .timeout_recv_response(Some(std::time::Duration::from_secs(read_timeout_secs)))
        .build();
    config.into()
}

fn http_get_json(url: &str, cookie_header: &str, accept: &str) -> Result<Value> {
    let agent = pacs_http_agent(30);
    let mut req = agent.get(url).header("Accept", accept);
    if !cookie_header.is_empty() {
        req = req.header("Cookie", cookie_header);
    }
    let mut resp = req.call()?;
    let body = resp.body_mut().read_to_string()?;
    let val: Value = serde_json::from_str(&body)?;
    Ok(val)
}

pub(super) fn probe_pacs_backend(base_url: &str, cookie_header: &str) -> Option<PacsBackend> {
    let base = base_url.trim_end_matches('/');

    // 1. Orthanc REST API probe
    for prefix in ["", "/orthanc"] {
        let probe_url = format!("{base}{prefix}/studies?limit=1");
        if let Ok(Value::Array(_)) = http_get_json(&probe_url, cookie_header, "application/json") {
            return Some(PacsBackend::Orthanc {
                base_url: format!("{base}{prefix}"),
            });
        }
    }

    // 2. DICOMweb (QIDO-RS / WADO-RS) probe
    for prefix in ["/dicom-web", "/qido-rs", "/rs", ""] {
        let probe_url = format!("{base}{prefix}/studies?limit=1");
        if let Ok(Value::Array(_)) =
            http_get_json(&probe_url, cookie_header, "application/dicom+json, application/json")
        {
            let wado_prefix = if prefix == "/qido-rs" {
                "/wado-rs".to_string()
            } else {
                prefix.to_string()
            };
            return Some(PacsBackend::DicomWeb {
                base_url: base.to_string(),
                qido_prefix: prefix.to_string(),
                wado_prefix,
            });
        }
    }

    None
}

fn dicom_json_str(obj: &serde_json::Map<String, Value>, tag: &str) -> String {
    let Some(val_arr) = obj.get(tag).and_then(|v| v.get("Value")).and_then(|v| v.as_array())
    else {
        return String::new();
    };
    let Some(first) = val_arr.first() else {
        return String::new();
    };
    if let Some(s) = first.as_str() {
        return s.to_string();
    }
    if let Some(alpha) = first.get("Alphabetic").and_then(|v| v.as_str()) {
        return alpha.to_string();
    }
    String::new()
}

pub fn search_pacs_studies(
    backend: &PacsBackend,
    cookie_header: &str,
    query: &str,
) -> Result<Vec<PacsStudyResult>> {
    match backend {
        PacsBackend::Orthanc { base_url } => {
            let val = if query.is_empty() {
                http_get_json(&format!("{base_url}/studies?expand&limit=50"), cookie_header, "application/json")?
            } else {
                let body = serde_json::json!({
                    "Level": "Study",
                    "Expand": true,
                    "Limit": 50,
                    "Query": {
                        "PatientName": format!("*{query}*")
                    }
                });
                let mut req = ureq::post(&format!("{base_url}/tools/find"))
                    .header("Content-Type", "application/json");
                if !cookie_header.is_empty() {
                    req = req.header("Cookie", cookie_header);
                }
                let mut resp = req.send(body.to_string())?;
                serde_json::from_str(&resp.body_mut().read_to_string()?)?
            };
            let arr = val.as_array().context("expected JSON array from Orthanc")?;
            let mut out = Vec::with_capacity(arr.len().min(50));
            for item in arr.iter().take(50) {
                let id = item.get("ID").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let pt = item.get("PatientMainDicomTags");
                let st = item.get("MainDicomTags");
                let get = |m: Option<&Value>, k: &str| {
                    m.and_then(|v| v.get(k))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string()
                };
                out.push(PacsStudyResult {
                    id,
                    study_uid: get(st, "StudyInstanceUID"),
                    patient_name: get(pt, "PatientName"),
                    patient_id: get(pt, "PatientID"),
                    study_date: get(st, "StudyDate"),
                    modality: String::new(),
                    description: get(st, "StudyDescription"),
                    has_report: false,
                });
            }
            Ok(out)
        }
        PacsBackend::DicomWeb {
            base_url,
            qido_prefix,
            ..
        } => {
            let mut url = format!("{base_url}{qido_prefix}/studies?limit=50");
            if !query.is_empty() {
                url.push_str(&format!("&PatientName=*{}*", query.replace(' ', "%20")));
            }
            let val = http_get_json(&url, cookie_header, "application/dicom+json, application/json")?;
            let arr = val.as_array().context("expected JSON array from QIDO-RS")?;
            let mut out = Vec::with_capacity(arr.len().min(50));
            for item in arr.iter().take(50) {
                let Some(obj) = item.as_object() else { continue };
                let uid = dicom_json_str(obj, "0020000D");
                out.push(PacsStudyResult {
                    id: uid.clone(),
                    study_uid: uid,
                    patient_name: dicom_json_str(obj, "00100010"),
                    patient_id: dicom_json_str(obj, "00100020"),
                    study_date: dicom_json_str(obj, "00080020"),
                    modality: dicom_json_str(obj, "00080061"),
                    description: dicom_json_str(obj, "00081030"),
                    has_report: false,
                });
            }
            Ok(out)
        }
        PacsBackend::VueMotion { .. } | PacsBackend::TrainedDom(_) => {
            anyhow::bail!("WebView searches run through the live WebView session.")
        }
    }
}

pub fn download_pacs_study_zip(
    backend: &PacsBackend,
    cookie_header: &str,
    study_id: &str,
    study_uid: &str,
    watch_dir: &Path,
    anonymize: bool,
) -> Result<PathBuf> {
    std::fs::create_dir_all(watch_dir)?;
    let safe_id: String = if study_uid.is_empty() { study_id } else { study_uid }
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' })
        .collect();
    let suffix = if anonymize { "_anon.zip" } else { ".zip" };
    let final_zip = watch_dir.join(format!("{safe_id}{suffix}"));
    let raw_zip = watch_dir.join(format!("{safe_id}.zip.part"));

    let url = match backend {
        PacsBackend::Orthanc { base_url } => format!("{base_url}/studies/{study_id}/archive"),
        PacsBackend::DicomWeb {
            base_url,
            wado_prefix,
            ..
        } => format!("{base_url}{wado_prefix}/studies/{study_uid}?accept=application/zip"),
        PacsBackend::VueMotion { .. } | PacsBackend::TrainedDom(_) => {
            anyhow::bail!("WebView downloads run through the live WebView session.")
        }
    };

    let agent = pacs_http_agent(300);
    let mut req = agent.get(&url).header("Accept", "application/zip, application/octet-stream");
    if !cookie_header.is_empty() {
        req = req.header("Cookie", cookie_header);
    }
    let mut resp = req.call()?;
    let mut reader = resp.body_mut().as_reader().take(MAX_PACS_DOWNLOAD_BYTES + 1);
    let mut out = std::fs::File::create(&raw_zip)?;
    let mut buf = [0u8; 64 * 1024];
    let mut total_bytes: u64 = 0;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        total_bytes = total_bytes.saturating_add(n as u64);
        if total_bytes > MAX_PACS_DOWNLOAD_BYTES {
            drop(out);
            let _ = std::fs::remove_file(&raw_zip);
            anyhow::bail!("Downloaded study ZIP exceeds maximum allowed size (4 GiB).");
        }
        out.write_all(&buf[..n])?;
    }
    out.flush()?;
    drop(out);

    if anonymize {
        let anon_part = watch_dir.join(format!("{safe_id}_anon.zip.part"));
        anonymize_zip_archive(&raw_zip, &anon_part)?;
        let _ = std::fs::remove_file(&raw_zip);
        if final_zip.exists() {
            let _ = std::fs::remove_file(&final_zip);
        }
        std::fs::rename(&anon_part, &final_zip)?;
    } else {
        if final_zip.exists() {
            let _ = std::fs::remove_file(&final_zip);
        }
        std::fs::rename(&raw_zip, &final_zip)?;
    }
    Ok(final_zip)
}

pub(super) fn anonymize_zip_archive(src_zip: &Path, dst_zip: &Path) -> Result<()> {
    let tmp_dir = tempfile::TempDir::new()?;

    let file = std::fs::File::open(src_zip)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let out_file = std::fs::File::create(dst_zip)?;
    let mut writer = zip::ZipWriter::new(out_file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let opts = crate::export::AnonymizeOptions::default();
    let mut uid_map = HashMap::new();
    let mut uid_counter = 0usize;
    let mut img_counter = 0usize;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        if entry.unix_mode().is_some_and(|m| (m & 0o170000) == 0o120000) {
            continue;
        }
        let Some(rel) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
            continue;
        };
        if rel
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.eq_ignore_ascii_case("DICOMDIR"))
        {
            continue;
        }
        let in_tmp = tmp_dir.path().join("in.dcm");
        let out_tmp = tmp_dir.path().join("out.dcm");
        {
            let mut f = std::fs::File::create(&in_tmp)?;
            std::io::copy(&mut entry, &mut f)?;
        }
        if crate::export::anonymize_dicom_file(
            &in_tmp,
            &out_tmp,
            &opts,
            &mut uid_map,
            &mut uid_counter,
        )
        .is_ok()
        {
            img_counter += 1;
            let neutral_name = format!("SER001/IMG{img_counter:04}.dcm");
            writer.start_file(neutral_name, options)?;
            let mut f = std::fs::File::open(&out_tmp)?;
            std::io::copy(&mut f, &mut writer)?;
        }
    }
    writer.finish()?;
    Ok(())
}

