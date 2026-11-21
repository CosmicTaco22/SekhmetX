use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::db::{CopyToLibraryPolicy, Database};
use crate::i18n::Lang;

/// User-defined Window / Level preset saved in SQLite settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomPreset {
    pub name: String,
    pub level: f32,
    pub width: f32,
}

/// Hanging protocol rule: preferred layout & auto-MPR for a given modality.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HangingProtocol {
    pub modality: String,
    pub rows: usize,
    pub cols: usize,
    pub auto_mpr: bool,
}

/// Typed wrapper around `Database` settings so setting keys are not scattered across UI code.
pub struct AppSettings<'a> {
    db: &'a Database,
}

impl<'a> AppSettings<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    pub fn language(&self) -> Lang {
        self.db
            .get_setting("lang")
            .map(|s| Lang::from_code(&s))
            .unwrap_or(Lang::En)
    }

    pub fn set_language(&self, lang: Lang) {
        let code = match lang {
            Lang::En => "en",
            Lang::Es => "es",
        };
        let _ = self.db.set_setting("lang", code);
    }

    pub fn custom_studies_dir(&self) -> Option<PathBuf> {
        self.db
            .get_setting("custom_studies_dir")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
    }

    pub fn set_custom_studies_dir(&self, dir: Option<&std::path::Path>) {
        let val = dir
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let _ = self.db.set_setting("custom_studies_dir", &val);
    }

    pub fn copy_to_library_policy(&self) -> CopyToLibraryPolicy {
        self.db
            .get_setting("copy_to_library_policy")
            .map(|s| CopyToLibraryPolicy::from_str(&s))
            .unwrap_or(CopyToLibraryPolicy::Ask)
    }

    pub fn study_storage_zip(&self) -> bool {
        self.db.get_setting("study_storage_format").as_deref() == Some("zip")
    }

    pub fn set_study_storage_zip(&self, zip: bool) {
        let _ = self
            .db
            .set_setting("study_storage_format", if zip { "zip" } else { "unzip" });
    }

    pub fn check_updates_on_startup(&self) -> bool {
        self.db
            .get_setting("check_updates_on_startup")
            .as_deref()
            != Some("0")
    }

    pub fn set_check_updates_on_startup(&self, enabled: bool) {
        let _ = self.db.set_setting(
            "check_updates_on_startup",
            if enabled { "1" } else { "0" },
        );
    }

    pub fn first_run_notice_ack(&self) -> bool {
        self.db.get_setting("first_run_notice_ack").as_deref() == Some("1")
    }

    pub fn set_first_run_notice_ack(&self, ack: bool) {
        let _ = self
            .db
            .set_setting("first_run_notice_ack", if ack { "1" } else { "0" });
    }

    pub fn crash_report_opt_in(&self) -> bool {
        self.db
            .get_setting("crash_report_opt_in")
            .as_deref()
            != Some("0")
    }

    pub fn set_crash_report_opt_in(&self, enabled: bool) {
        let _ = self
            .db
            .set_setting("crash_report_opt_in", if enabled { "1" } else { "0" });
    }

    pub fn patient_links(&self) -> HashMap<String, String> {
        self.db
            .get_setting("patient_links")
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    pub fn custom_presets(&self) -> Vec<CustomPreset> {
        self.db
            .get_setting("custom_presets")
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    pub fn set_custom_presets(&self, presets: &[CustomPreset]) {
        if let Ok(json) = serde_json::to_string(presets) {
            let _ = self.db.set_setting("custom_presets", &json);
        }
    }

    pub fn hanging_protocols(&self) -> Vec<HangingProtocol> {
        self.db
            .get_setting("hanging_protocols")
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    pub fn set_hanging_protocols(&self, protocols: &[HangingProtocol]) {
        if let Ok(json) = serde_json::to_string(protocols) {
            let _ = self.db.set_setting("hanging_protocols", &json);
        }
    }

    pub fn pacs_nodes(&self) -> Vec<crate::net::PacsNode> {
        self.db
            .get_setting("pacs_nodes")
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    pub fn set_pacs_nodes(&self, nodes: &[crate::net::PacsNode]) {
        if let Ok(json) = serde_json::to_string(nodes) {
            let _ = self.db.set_setting("pacs_nodes", &json);
        }
    }

    pub fn dicomweb_servers(&self) -> Vec<crate::net::DicomWebServer> {
        self.db
            .get_setting("dicomweb_servers")
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    pub fn set_dicomweb_servers(&self, servers: &[crate::net::DicomWebServer]) {
        if let Ok(json) = serde_json::to_string(servers) {
            let _ = self.db.set_setting("dicomweb_servers", &json);
        }
    }

    pub fn report_template(&self) -> String {
        self.db.get_setting("report_template").unwrap_or_default()
    }

    pub fn set_report_template(&self, template: &str) {
        let _ = self.db.set_setting("report_template", template);
    }

    pub fn custom_shortcuts(&self) -> HashMap<String, String> {
        self.db
            .get_setting("custom_shortcuts")
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    pub fn set_custom_shortcuts(&self, shortcuts: &HashMap<String, String>) {
        if let Ok(json) = serde_json::to_string(shortcuts) {
            let _ = self.db.set_setting("custom_shortcuts", &json);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_settings_roundtrip() {
        let dir = tempfile::TempDir::new().unwrap();
        let db = Database::open_at(&dir.path().join("test.db")).unwrap();
        let settings = AppSettings::new(&db);

        assert_eq!(settings.language(), Lang::En);
        settings.set_language(Lang::Es);
        assert_eq!(settings.language(), Lang::Es);

        assert!(!settings.study_storage_zip());
        settings.set_study_storage_zip(true);
        assert!(settings.study_storage_zip());

        assert!(settings.crash_report_opt_in());
        settings.set_crash_report_opt_in(false);
        assert!(!settings.crash_report_opt_in());

        let presets = vec![CustomPreset {
            name: "Liver".into(),
            level: 60.0,
            width: 150.0,
        }];
        settings.set_custom_presets(&presets);
        assert_eq!(settings.custom_presets(), presets);

        let protos = vec![HangingProtocol {
            modality: "CT".into(),
            rows: 1,
            cols: 2,
            auto_mpr: true,
        }];
        settings.set_hanging_protocols(&protos);
        assert_eq!(settings.hanging_protocols(), protos);

        let nodes = vec![crate::net::PacsNode {
            name: "Local PACS".into(),
            ae_title: "ORTHANC".into(),
            host: "127.0.0.1".into(),
            port: 4242,
            local_ae_title: "SEKHMETX".into(),
            use_tls: false,
            retrieve_method: crate::net::RetrieveMethod::CMove,
        }];
        settings.set_pacs_nodes(&nodes);
        assert_eq!(settings.pacs_nodes(), nodes);

        let servers = vec![crate::net::DicomWebServer {
            name: "Demo DICOMweb".into(),
            base_url: "https://example.org/dicom-web".into(),
            auth: crate::net::DicomWebAuth::BearerToken,
        }];
        settings.set_dicomweb_servers(&servers);
        assert_eq!(settings.dicomweb_servers(), servers);

        settings.set_report_template("FINDINGS:\n- Normal\n\nIMPRESSION:\n- Unremarkable");
        assert!(settings.report_template().contains("IMPRESSION:"));

        let mut sc = HashMap::new();
        sc.insert("wl".to_string(), "W".to_string());
        sc.insert("pacs".to_string(), "P".to_string());
        settings.set_custom_shortcuts(&sc);
        assert_eq!(settings.custom_shortcuts(), sc);

        assert!(!settings.first_run_notice_ack());
        settings.set_first_run_notice_ack(true);
        assert!(settings.first_run_notice_ack());
    }
}
