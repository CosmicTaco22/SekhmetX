pub trait SecretStore {
    fn set_password(&self, account: &str, password: &str) -> Result<(), String>;
    fn get_password(&self, account: &str) -> Result<zeroize::Zeroizing<String>, String>;
    fn delete_password(&self, account: &str) -> Result<(), String>;
}

pub struct KeyringSecretStore;

impl SecretStore for KeyringSecretStore {
    fn set_password(&self, account: &str, password: &str) -> Result<(), String> {
        let entry = keyring::Entry::new("SekhmetX", account)
            .map_err(|e| format!("Keyring entry error: {e}"))?;
        entry
            .set_password(password)
            .map_err(|e| format!("Could not store password in OS keyring: {e}"))
    }

    fn get_password(&self, account: &str) -> Result<zeroize::Zeroizing<String>, String> {
        let entry = keyring::Entry::new("SekhmetX", account)
            .map_err(|e| format!("Keyring entry error: {e}"))?;
        match entry.get_password() {
            Ok(pw) => Ok(zeroize::Zeroizing::new(pw)),
            Err(keyring::Error::NoEntry) => Ok(zeroize::Zeroizing::new(String::new())),
            Err(e) => Err(format!("Could not read password from OS keyring: {e}")),
        }
    }

    fn delete_password(&self, account: &str) -> Result<(), String> {
        let entry = keyring::Entry::new("SekhmetX", account)
            .map_err(|e| format!("Keyring entry error: {e}"))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("Could not delete password from OS keyring: {e}")),
        }
    }
}

fn secret_account(portal_origin: &str) -> String {
    format!("pacs:{}", portal_origin.trim())
}

pub fn store_pacs_password_with(
    store: &dyn SecretStore,
    portal_origin: &str,
    password: &str,
) -> Result<(), String> {
    let account = secret_account(portal_origin);
    if password.is_empty() {
        store.delete_password(&account)
    } else {
        store.set_password(&account, password)
    }
}

pub fn load_pacs_password_with(
    store: &dyn SecretStore,
    portal_origin: &str,
) -> Result<zeroize::Zeroizing<String>, String> {
    store.get_password(&secret_account(portal_origin))
}

pub fn delete_pacs_password_with(
    store: &dyn SecretStore,
    portal_origin: &str,
) -> Result<(), String> {
    store.delete_password(&secret_account(portal_origin))
}

pub fn store_pacs_password(portal_origin: &str, password: &str) -> Result<(), String> {
    store_pacs_password_with(&KeyringSecretStore, portal_origin, password)
}

pub fn load_pacs_password(portal_origin: &str) -> Result<zeroize::Zeroizing<String>, String> {
    load_pacs_password_with(&KeyringSecretStore, portal_origin)
}

pub fn delete_pacs_password(portal_origin: &str) -> Result<(), String> {
    delete_pacs_password_with(&KeyringSecretStore, portal_origin)
}

fn cookie_account(portal_origin: &str) -> String {
    format!("pacs-cookie:{}", portal_origin.trim())
}

pub fn store_pacs_cookies(portal_origin: &str, cookie_header: &str) -> Result<(), String> {
    let account = cookie_account(portal_origin);
    if cookie_header.is_empty() {
        KeyringSecretStore.delete_password(&account)
    } else {
        KeyringSecretStore.set_password(&account, cookie_header)
    }
}

pub fn load_pacs_cookies(portal_origin: &str) -> Result<zeroize::Zeroizing<String>, String> {
    KeyringSecretStore.get_password(&cookie_account(portal_origin))
}

pub fn delete_pacs_cookies(portal_origin: &str) -> Result<(), String> {
    KeyringSecretStore.delete_password(&cookie_account(portal_origin))
}

/// Clears any legacy `pacs_cred_pass` row from SQLite so plaintext/hex secrets are never retained.
pub fn migrate_legacy_secret_if_any(db: &crate::db::Database, _portal_origin: &str) {
    if let Some(legacy) = db.get_setting("pacs_cred_pass")
        && !legacy.is_empty() {
            let _ = db.set_setting("pacs_cred_pass", "");
        }
}

