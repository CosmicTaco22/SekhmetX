//! Groundwork for PACS networking (`network-pacs`), DICOMweb (`dicomweb`),
//! and user-defined portal plugins (`PORTING_MAP.md` P2).
//!
//! Defines typed, serializable node configurations, query filters, QIDO-RS URL
//! builders, and result models with zero institution-specific defaults or
//! credentials, ready for `dicom-ul` (DIMSE) and `ureq` (DICOMweb) transports.

use serde::{Deserialize, Serialize};

/// How a DICOM DIMSE PACS node delivers requested studies.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RetrieveMethod {
    #[default]
    CMove,
    CGet,
}

/// Configuration for a standard DICOM PACS peer (C-ECHO / C-FIND / C-MOVE / C-GET / C-STORE).
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PacsNode {
    pub name: String,
    pub ae_title: String,
    pub host: String,
    pub port: u16,
    pub local_ae_title: String,
    pub use_tls: bool,
    pub retrieve_method: RetrieveMethod,
}

#[allow(dead_code)]
impl PacsNode {
    /// Validates basic DIMSE AE Title (1–16 printable ASCII chars) and host/port rules.
    pub fn is_valid(&self) -> bool {
        let valid_ae = |s: &str| {
            let t = s.trim();
            !t.is_empty() && t.len() <= 16 && t.bytes().all(|b| (0x20..=0x7E).contains(&b))
        };
        valid_ae(&self.ae_title)
            && valid_ae(&self.local_ae_title)
            && !self.host.trim().is_empty()
            && self.port > 0
    }
}

/// Authentication method for a DICOMweb server (credentials stored in OS keyring, never here).
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DicomWebAuth {
    #[default]
    None,
    Basic,
    BearerToken,
}

/// Configuration for a DICOMweb (QIDO-RS / WADO-RS / STOW-RS) endpoint.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DicomWebServer {
    pub name: String,
    pub base_url: String,
    pub auth: DicomWebAuth,
}

#[allow(dead_code)]
impl DicomWebServer {
    /// Builds a QIDO-RS `/studies` query URL from `filter`.
    pub fn qido_studies_url(&self, filter: &QueryFilter) -> String {
        let base = self.base_url.trim_end_matches('/');
        let mut params: Vec<(&str, String)> = Vec::new();
        if !filter.patient_name.trim().is_empty() {
            params.push(("PatientName", filter.patient_name.trim().to_string()));
        }
        if !filter.patient_id.trim().is_empty() {
            params.push(("PatientID", filter.patient_id.trim().to_string()));
        }
        if !filter.accession_number.trim().is_empty() {
            params.push(("AccessionNumber", filter.accession_number.trim().to_string()));
        }
        if !filter.modality.trim().is_empty() {
            params.push(("ModalitiesInStudy", filter.modality.trim().to_string()));
        }
        if !filter.study_date.trim().is_empty() {
            params.push(("StudyDate", filter.study_date.trim().to_string()));
        }
        if params.is_empty() {
            return format!("{base}/studies");
        }
        let qs: Vec<String> = params
            .into_iter()
            .map(|(k, v)| format!("{k}={}", percent_encode_query(&v)))
            .collect();
        format!("{base}/studies?{}", qs.join("&"))
    }
}

fn percent_encode_query(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'*' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Study-level search filter shared by DIMSE C-FIND and DICOMweb QIDO-RS.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct QueryFilter {
    pub patient_name: String,
    pub patient_id: String,
    pub accession_number: String,
    pub modality: String,
    /// DICOM date or range (`YYYYMMDD` or `YYYYMMDD-YYYYMMDD`).
    pub study_date: String,
}

/// One study returned by a PACS / DICOMweb query.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct StudyQueryResult {
    pub study_uid: String,
    pub patient_name: String,
    pub patient_id: String,
    pub study_date: String,
    pub modalities: String,
    pub description: String,
    pub accession_number: String,
    pub num_series: u32,
    pub num_instances: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pacs_node_validation_enforces_ae_title_and_port() {
        let valid = PacsNode {
            name: "Local Orthanc".into(),
            ae_title: "ORTHANC".into(),
            host: "127.0.0.1".into(),
            port: 4242,
            local_ae_title: "SEKHMETX".into(),
            use_tls: false,
            retrieve_method: RetrieveMethod::CMove,
        };
        assert!(valid.is_valid());

        let mut bad_ae = valid.clone();
        bad_ae.ae_title = "A_VERY_LONG_AE_TITLE_17".into();
        assert!(!bad_ae.is_valid());

        let mut bad_port = valid;
        bad_port.port = 0;
        assert!(!bad_port.is_valid());
    }

    #[test]
    fn dicomweb_qido_url_encodes_filters_cleanly() {
        let srv = DicomWebServer {
            name: "Demo".into(),
            base_url: "https://pacs.example.org/dicom-web/".into(),
            auth: DicomWebAuth::None,
        };
        let empty = QueryFilter::default();
        assert_eq!(srv.qido_studies_url(&empty), "https://pacs.example.org/dicom-web/studies");

        let f = QueryFilter {
            patient_name: "DOE^JOHN*".into(),
            modality: "CT".into(),
            study_date: "20261101-20261121".into(),
            ..Default::default()
        };
        assert_eq!(
            srv.qido_studies_url(&f),
            "https://pacs.example.org/dicom-web/studies?PatientName=DOE%5EJOHN*&ModalitiesInStudy=CT&StudyDate=20261101-20261121"
        );
    }
}
