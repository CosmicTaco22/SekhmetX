//! In-app GitHub Releases update checker and self-updater for SekhmetX.

use std::io::{Read, Write};
use std::sync::mpsc::{channel, Receiver};

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const REPO_LATEST_URL: &str =
    "https://api.github.com/repos/CosmicTaco22/SekhmetX/releases/latest";

#[derive(Debug, Clone)]
pub struct ReleaseInfo {
    pub tag_name: String,
    pub html_url: String,
    pub notes: String,
    pub download_url: Option<String>,
    pub asset_name: Option<String>,
    pub sha256sums_url: Option<String>,
}

#[derive(Debug, Clone)]
pub enum UpdateStatus {
    Idle,
    Checking,
    UpToDate(String),
    Available(ReleaseInfo),
    Installing,
    Installed(String),
    Error(String),
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

/// Parses `"v0.2.1"` or `"0.2.1-rc1"` into `(major, minor, patch)`.
pub fn parse_semver(tag: &str) -> Option<(u64, u64, u64)> {
    let clean = tag.trim().trim_start_matches(['v', 'V']);
    let core = clean.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let maj = parts.next()?.parse().ok()?;
    let min = parts.next().unwrap_or("0").parse().ok()?;
    let pat = parts.next().unwrap_or("0").parse().ok()?;
    Some((maj, min, pat))
}

/// Returns `true` if `remote_tag` is strictly newer than `current_version`.
pub fn is_newer(remote_tag: &str, current_version: &str) -> bool {
    match (parse_semver(remote_tag), parse_semver(current_version)) {
        (Some(r), Some(c)) => r > c,
        _ => false,
    }
}

pub fn platform_asset_suffix() -> &'static str {
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "windows-x86_64.zip"
    } else if cfg!(all(target_os = "windows", target_arch = "aarch64")) {
        "windows-arm64.zip"
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "macos-arm64.tar.gz"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "linux-x86_64.tar.gz"
    } else {
        ""
    }
}

fn target_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "sekhmetx.exe"
    } else {
        "sekhmetx"
    }
}

pub fn check_latest_release() -> Result<Option<ReleaseInfo>> {
    let agent = ureq::Agent::new_with_defaults();
    let user_agent = format!("SekhmetX/{}", env!("CARGO_PKG_VERSION"));
    let body = agent
        .get(REPO_LATEST_URL)
        .header("User-Agent", &user_agent)
        .header("Accept", "application/vnd.github+json")
        .call()
        .context("Failed to reach GitHub Releases API")?
        .body_mut()
        .read_to_string()
        .context("Failed to read GitHub Releases response")?;

    let rel: GhRelease =
        serde_json::from_str(&body).context("Failed to parse GitHub Releases JSON")?;

    if !is_newer(&rel.tag_name, env!("CARGO_PKG_VERSION")) {
        return Ok(None);
    }

    let suffix = platform_asset_suffix();
    let sha256sums_url = rel
        .assets
        .iter()
        .find(|a| a.name == "SHA256SUMS")
        .map(|a| a.browser_download_url.clone());
    let matched_asset = if suffix.is_empty() {
        None
    } else {
        rel.assets.into_iter().find(|a| a.name.ends_with(suffix))
    };
    let (asset_name, download_url) = match matched_asset {
        Some(a) => (Some(a.name), Some(a.browser_download_url)),
        None => (None, None),
    };

    Ok(Some(ReleaseInfo {
        tag_name: rel.tag_name,
        html_url: rel.html_url,
        notes: rel.body,
        download_url,
        asset_name,
        sha256sums_url,
    }))
}

pub fn spawn_check(
    on_done: impl Fn() + Send + 'static,
) -> Receiver<Result<Option<ReleaseInfo>>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let res = check_latest_release();
        let _ = tx.send(res);
        on_done();
    });
    rx
}

pub fn parse_sha256sum(contents: &str, asset_name: &str) -> Option<String> {
    for line in contents.lines() {
        let mut parts = line.split_whitespace();
        let Some(hash) = parts.next() else { continue };
        let Some(name) = parts.next() else { continue };
        let clean_name = name.trim_start_matches('*');
        if (clean_name == asset_name
            || clean_name.rsplit(['/', '\\']).next() == Some(asset_name))
            && hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
                return Some(hash.to_ascii_lowercase());
            }
    }
    None
}

fn fetch_expected_sha256(
    agent: &ureq::Agent,
    user_agent: &str,
    sha256sums_url: &str,
    asset_name: &str,
) -> Result<String> {
    let body = agent
        .get(sha256sums_url)
        .header("User-Agent", user_agent)
        .call()
        .context("Failed to download SHA256SUMS")?
        .body_mut()
        .read_to_string()
        .context("Failed to read SHA256SUMS")?;
    parse_sha256sum(&body, asset_name)
        .with_context(|| format!("No SHA-256 entry found for `{asset_name}` in SHA256SUMS"))
}

pub fn download_and_install(
    url: &str,
    asset_name: Option<&str>,
    sha256sums_url: Option<&str>,
) -> Result<()> {
    let agent = ureq::Agent::new_with_defaults();
    let user_agent = format!("SekhmetX/{}", env!("CARGO_PKG_VERSION"));

    let expected_sha256 = match (sha256sums_url, asset_name) {
        (Some(sums_url), Some(name)) => {
            Some(fetch_expected_sha256(&agent, &user_agent, sums_url, name)?)
        }
        _ => bail!("Release is missing SHA256SUMS checksum verification"),
    };

    let mut resp = agent
        .get(url)
        .header("User-Agent", &user_agent)
        .call()
        .context("Failed to download release archive")?;

    let bin_name = target_binary_name();
    let tmp_dir = std::env::temp_dir().join(format!("sekhmetx-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp_dir);
    std::fs::create_dir_all(&tmp_dir)?;
    let archive_path = tmp_dir.join("release-archive");
    let tmp_bin = tmp_dir.join(bin_name);

    const MAX_ARCHIVE_BYTES: u64 = 200 * 1024 * 1024;
    let mut reader = resp.body_mut().as_reader().take(MAX_ARCHIVE_BYTES + 1);
    let mut out_archive = std::fs::File::create(&archive_path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    let mut total: u64 = 0;
    loop {
        let n = reader.read(&mut buf).context("Failed to stream release archive")?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > MAX_ARCHIVE_BYTES {
            let _ = std::fs::remove_dir_all(&tmp_dir);
            bail!("Release archive exceeds 200 MiB safety limit");
        }
        hasher.update(&buf[..n]);
        out_archive.write_all(&buf[..n])?;
    }
    out_archive.flush()?;
    drop(out_archive);

    let actual_sha256 = format!("{:x}", hasher.finalize());
    if let Some(expected) = expected_sha256
        && actual_sha256 != expected {
            let _ = std::fs::remove_dir_all(&tmp_dir);
            bail!(
                "SHA-256 checksum mismatch for update archive (expected {expected}, got {actual_sha256})"
            );
        }

    if url.ends_with(".zip") {
        let file = std::fs::File::open(&archive_path)?;
        let mut zip = zip::ZipArchive::new(file).context("Invalid .zip release archive")?;
        let mut found = false;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            if entry.is_dir() || entry.unix_mode().is_some_and(|m| (m & 0o170000) == 0o120000) {
                continue;
            }
            let entry_name = entry.name()?.into_owned();
            let name = entry_name.rsplit('/').next().unwrap_or(&entry_name);
            if name.eq_ignore_ascii_case(bin_name) {
                let mut out = std::fs::File::create(&tmp_bin)?;
                std::io::copy(&mut entry, &mut out)?;
                found = true;
                break;
            }
        }
        if !found {
            let _ = std::fs::remove_dir_all(&tmp_dir);
            bail!("Binary `{bin_name}` not found inside release archive");
        }
    } else if url.ends_with(".tar.gz") {
        let file = std::fs::File::open(&archive_path)?;
        let gz = flate2::read::GzDecoder::new(file);
        let mut tar = tar::Archive::new(gz);
        let mut found = false;
        for entry in tar.entries()? {
            let mut entry = entry?;
            let path = entry.path()?;
            if path.file_name().and_then(|s| s.to_str()) == Some(bin_name) {
                let mut out = std::fs::File::create(&tmp_bin)?;
                std::io::copy(&mut entry, &mut out)?;
                found = true;
                break;
            }
        }
        if !found {
            let _ = std::fs::remove_dir_all(&tmp_dir);
            bail!("Binary `{bin_name}` not found inside release archive");
        }
    } else {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        bail!("Unsupported release archive format: {url}");
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&tmp_bin)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&tmp_bin, perms)?;
    }

    self_replace::self_replace(&tmp_bin).context("Failed to replace running executable")?;
    let _ = std::fs::remove_dir_all(&tmp_dir);
    Ok(())
}

pub fn spawn_install(
    url: String,
    asset_name: Option<String>,
    sha256sums_url: Option<String>,
    tag: String,
    on_done: impl Fn() + Send + 'static,
) -> Receiver<Result<String>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let res = download_and_install(
            &url,
            asset_name.as_deref(),
            sha256sums_url.as_deref(),
        )
        .map(|()| tag);
        let _ = tx.send(res);
        on_done();
    });
    rx
}

pub fn restart_application() -> Result<()> {
    let exe = std::env::current_exe().context("Cannot locate current executable")?;
    std::process::Command::new(exe)
        .spawn()
        .context("Failed to launch updated SekhmetX")?;
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_comparison_handles_v_prefix_and_prerelease() {
        assert_eq!(parse_semver("v0.2.1"), Some((0, 2, 1)));
        assert_eq!(parse_semver("0.3.0-beta.1"), Some((0, 3, 0)));
        assert!(is_newer("v0.2.2", "0.2.1"));
        assert!(is_newer("v0.3.0", "0.2.1"));
        assert!(!is_newer("v0.2.1", "0.2.1"));
        assert!(!is_newer("v0.2.0", "0.2.1"));
    }

    #[test]
    fn parse_sha256sum_extracts_matching_asset_digest() {
        let sums = "\
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  sekhmetx-v0.2.3-linux-x86_64.tar.gz\n\
BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB *sekhmetx-v0.2.3-windows-x86_64.zip\n";
        assert_eq!(
            parse_sha256sum(sums, "sekhmetx-v0.2.3-windows-x86_64.zip").as_deref(),
            Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        );
        assert_eq!(parse_sha256sum(sums, "missing.zip"), None);
    }
}
