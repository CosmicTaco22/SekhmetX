//! One-time removal of data left by the web-portal PACS login, removed in 0.5.4.
//!
//! That feature kept a portal password and session cookies in the system
//! keychain, portal settings (address, institution name, user name, trained
//! click profiles) in the settings table, and a web-browser profile folder.
//! None of it is used any more, and it should not linger on people's computers.

use std::collections::BTreeSet;

use crate::db::Database;

const DONE_KEY: &str = "web_portal_data_removed";
const KEYCHAIN_SERVICE: &str = "SekhmetX";

/// Runs once per database: deletes the old portal's keychain entries, settings
/// and browser profile. Later calls return immediately.
pub fn remove_web_portal_data(db: &Database) {
    if db.get_setting(DONE_KEY).as_deref() == Some("1") {
        return;
    }
    let settings = db.settings_with_prefix("pacs_");
    let origins = portal_origins(&settings);
    for origin in &origins {
        for account in [format!("pacs:{origin}"), format!("pacs-cookie:{origin}")] {
            if let Ok(entry) = keyring::Entry::new(KEYCHAIN_SERVICE, &account) {
                let _ = entry.delete_credential();
            }
        }
    }
    for (key, _) in &settings {
        let _ = db.delete_setting(key);
    }
    let profile = crate::crash_log::app_data_dir().join("webview");
    if profile.is_dir() {
        let _ = std::fs::remove_dir_all(&profile);
    }
    if !settings.is_empty() {
        crate::crash_log::log_line(
            "INFO",
            &format!(
                "Removed data from the old web-portal login ({} settings, {} portal(s))",
                settings.len(),
                origins.len()
            ),
        );
    }
    let _ = db.set_setting(DONE_KEY, "1");
}

/// Every portal origin the old feature could have stored secrets under.
fn portal_origins(settings: &[(String, String)]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (key, value) in settings {
        if let Some(origin) = key.strip_prefix("pacs_trained_") {
            out.insert(origin.to_string());
        }
        if key.ends_with("_url")
            && let Some(origin) = https_origin(value)
        {
            out.insert(origin);
        }
    }
    out
}

/// `https://Host:443/path?x` → `https://host`; non-https or malformed → None.
/// Matches how the removed code named its keychain entries.
fn https_origin(raw: &str) -> Option<String> {
    let rest = raw.trim().strip_prefix("https://").or_else(|| {
        let t = raw.trim();
        t.get(..8)
            .filter(|p| p.eq_ignore_ascii_case("https://"))
            .map(|_| &t[8..])
    })?;
    let authority = rest.split(['/', '?', '#']).next()?;
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let authority = authority.to_ascii_lowercase();
    let authority = authority.strip_suffix(":443").unwrap_or(&authority);
    Some(format!("https://{authority}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_matches_the_removed_feature() {
        assert_eq!(
            https_origin("https://Portal.Example.com/portal/WebLogin.aspx?ReturnUrl=%2f").as_deref(),
            Some("https://portal.example.com")
        );
        assert_eq!(
            https_origin("https://pacs.example.com:8443/login").as_deref(),
            Some("https://pacs.example.com:8443")
        );
        assert_eq!(https_origin("https://pacs.example.com:443/").as_deref(), Some("https://pacs.example.com"));
        assert_eq!(https_origin("http://pacs.example.com/"), None);
        assert_eq!(https_origin("https://user:pw@pacs.example.com/"), None);
        assert_eq!(https_origin(""), None);
    }

    #[test]
    fn portal_settings_are_deleted_once_and_others_kept() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open_at(&dir.path().join("t.db")).unwrap();
        db.set_setting("pacs_login_url", "https://pacs.example.com/login").unwrap();
        db.set_setting("pacs_institution_name", "Example").unwrap();
        db.set_setting("pacs_trained_https://other.example.com", "{}").unwrap();
        db.set_setting("ui_lang", "es").unwrap();

        let origins = portal_origins(&db.settings_with_prefix("pacs_"));
        assert!(origins.contains("https://pacs.example.com"));
        assert!(origins.contains("https://other.example.com"));

        remove_web_portal_data(&db);
        assert!(db.settings_with_prefix("pacs_").is_empty());
        assert_eq!(db.get_setting("ui_lang").as_deref(), Some("es"));
        assert_eq!(db.get_setting(DONE_KEY).as_deref(), Some("1"));

        // A second run does nothing, even if a matching key reappears.
        db.set_setting("pacs_x", "y").unwrap();
        remove_web_portal_data(&db);
        assert_eq!(db.get_setting("pacs_x").as_deref(), Some("y"));
    }
}
