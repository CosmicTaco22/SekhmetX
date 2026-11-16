use std::collections::HashMap;
use std::path::PathBuf;

use crate::db::{CopyToLibraryPolicy, Database};
use crate::i18n::Lang;

/// Typed wrapper around `Database` settings so setting keys are not scattered across UI code.
pub struct AppSettings<'a> {
    db: &'a Database,
}

#[allow(dead_code)]
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

        assert!(!settings.first_run_notice_ack());
        settings.set_first_run_notice_ack(true);
        assert!(settings.first_run_notice_ack());
    }
}
