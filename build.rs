use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/tags");
    println!("cargo:rerun-if-changed=assets/icon/sekhmetx.ico");

    let pkg_version = env!("CARGO_PKG_VERSION");
    let commits_since_tag = Command::new("git")
        .args(["describe", "--tags", "--long", "--match", "v*"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| {
            // Format: v0.2.8-14-gabc1234 -> split from right by '-'
            let mut parts = s.trim().rsplit('-');
            let _hash = parts.next()?;
            let count_str = parts.next()?;
            count_str.parse::<u64>().ok()
        });

    let internal_version = match commits_since_tag {
        Some(n) if n > 0 => format!("{pkg_version}.{n}"),
        _ => pkg_version.to_string(),
    };

    println!("cargo:rustc-env=SEKHMETX_INTERNAL_VERSION={internal_version}");

    #[cfg(windows)]
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        let mut res = winres::WindowsResource::new();
        res.set("ProductName", "SekhmetX");
        res.set("FileDescription", "SekhmetX DICOM Medical Image Viewer");
        res.set("CompanyName", "SekhmetX Open Source");
        res.set("LegalCopyright", "Copyright (C) 2026 SekhmetX Contributors (LGPL-3.0-or-later)");
        res.set("OriginalFilename", "sekhmetx.exe");
        res.set("InternalName", "sekhmetx");
        res.set("FileVersion", pkg_version);
        res.set("ProductVersion", pkg_version);
        res.set_icon("assets/icon/sekhmetx.ico");
        let _ = res.compile();
    }
}
