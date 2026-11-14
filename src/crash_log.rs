//! Persistent crash & diagnostic log for SekhmetX.
//! On normal exit the log is deleted automatically so no clutter is left behind.
//! If SekhmetX crashed on the previous run, the log remains available for
//! one-click retrieval & deletion on the next launch.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static LOG_LOCK: Mutex<()> = Mutex::new(());
static HAD_ERROR_OR_PANIC: AtomicBool = AtomicBool::new(false);

/// Returns the persistent SekhmetX data directory.
/// On Windows, prefers an `AppData` folder next to the portable executable
/// when writable, falling back to `%LOCALAPPDATA%\SekhmetX`.
pub fn app_data_dir() -> PathBuf {
    #[cfg(windows)]
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            let portable = exe_dir.join("AppData");
            if fs::create_dir_all(&portable).is_ok() {
                let probe = portable.join(".write_probe");
                if fs::write(&probe, b"").is_ok() {
                    let _ = fs::remove_file(&probe);
                    return portable;
                }
            }
        }
    }
    #[cfg(windows)]
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(local).join("SekhmetX");
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("SekhmetX");
    }
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(xdg).join("SekhmetX");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".local").join("share").join("SekhmetX");
    }
    std::env::temp_dir().join("SekhmetX")
}

pub fn log_path() -> PathBuf {
    app_data_dir().join("sekhmetx.log")
}

fn timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn log_line(level: &str, msg: &str) {
    if level == "FATAL" || level == "ERROR" {
        HAD_ERROR_OR_PANIC.store(true, Ordering::Relaxed);
    }
    let Ok(_guard) = LOG_LOCK.lock() else {
        return;
    };
    let path = log_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "[{}][{level}] {msg}", timestamp_secs());
        let _ = f.flush();
    }
}

/// Returns `true` if a log file from an unclean previous exit exists on disk.
/// Must be called before `init()`.
pub fn previous_run_crashed() -> bool {
    let path = log_path();
    fs::metadata(&path).map(|m| m.len() > 0).unwrap_or(false)
}

pub fn read_log() -> Option<String> {
    let _guard = LOG_LOCK.lock().ok()?;
    fs::read_to_string(log_path()).ok()
}

pub fn delete_log() {
    if let Ok(_guard) = LOG_LOCK.lock() {
        let _ = fs::remove_file(log_path());
    }
}

pub fn cleanup_on_clean_exit(keep_for_user: bool) {
    if !keep_for_user && !HAD_ERROR_OR_PANIC.load(Ordering::Relaxed) {
        delete_log();
    }
}

pub fn init() {
    let path = log_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    // Keep log bounded (~1 MB max).
    if let Ok(meta) = fs::metadata(&path) {
        if meta.len() > 1_048_576 {
            let _ = fs::remove_file(&path);
        }
    }

    log_line(
        "INFO",
        &format!(
            "=== SekhmetX v{} started (os={}, arch={}) ===",
            crate::BUILD_VERSION,
            std::env::consts::OS,
            std::env::consts::ARCH
        ),
    );

    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "non-string panic payload".to_string()
        };
        let loc = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "<unknown>".to_string());
        let bt = std::backtrace::Backtrace::force_capture();
        let msg = format!("PANIC at {loc}: {payload}\nBacktrace:\n{bt}");
        log_line("FATAL", &msg);
        eprintln!("{msg}");
        prev_hook(info);
    }));
}

fn url_encode_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push_str("%20"),
            b'\n' => out.push_str("%0A"),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

pub fn github_issue_url(log_text: &str) -> String {
    let tail: String = log_text
        .lines()
        .rev()
        .take(50)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    let clipped = if tail.len() > 3000 {
        &tail[tail.len() - 3000..]
    } else {
        &tail
    };
    let title = format!("Crash Report: SekhmetX v{} ({})", crate::BUILD_VERSION, std::env::consts::OS);
    let body = format!(
        "**Version:** v{}\n**OS:** {} ({})\n\n```text\n{}\n```",
        crate::BUILD_VERSION,
        std::env::consts::OS,
        std::env::consts::ARCH,
        clipped
    );
    format!(
        "https://github.com/CosmicTaco22/SekhmetX/issues/new?title={}&body={}",
        url_encode_component(&title),
        url_encode_component(&body)
    )
}
