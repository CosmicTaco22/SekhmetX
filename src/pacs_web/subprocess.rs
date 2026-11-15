use super::*;

fn emit_subprocess_json(msg_type: &str, payload: &str) {
    use std::io::Write;
    let line = serde_json::json!({
        "type": msg_type,
        "payload": payload,
    });
    println!("{line}");
    let _ = std::io::stdout().flush();
}

pub(super) fn spawn_webview_subprocess(
    login_url: String,
    watch_dir: PathBuf,
    hidden: bool,
    download_requested: Arc<AtomicBool>,
    cmd_rx: Receiver<WebViewCommand>,
    event_tx: Sender<PacsEvent>,
    egui_ctx: egui::Context,
) {
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};

    let Ok(portal_origin) = parse_https_origin(&login_url) else {
        let _ = event_tx.send(PacsEvent::Incompatible(
            "Portal URL must be a valid https:// address.".into(),
        ));
        let _ = event_tx.send(PacsEvent::WebViewClosed);
        egui_ctx.request_repaint();
        return;
    };

    let _ = std::fs::create_dir_all(&watch_dir);
    let canonical_watch_dir = std::fs::canonicalize(&watch_dir).ok();

    let Ok(exe) = std::env::current_exe() else {
        let _ = event_tx.send(PacsEvent::Incompatible(
            "Could not locate SekhmetX executable for login window.".into(),
        ));
        let _ = event_tx.send(PacsEvent::WebViewClosed);
        egui_ctx.request_repaint();
        return;
    };

    crate::crash_log::log_line("INFO", &format!("Spawning PACS WebView subprocess for {portal_origin}"));
    let mut cmd = Command::new(exe);
    cmd.arg("--pacs-login").arg(&login_url).arg(&watch_dir);
    if hidden {
        cmd.arg("--hidden");
    }
    let mut child = match cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            crate::crash_log::log_line("ERROR", &format!("Failed to spawn PACS subprocess: {e}"));
            let _ = event_tx.send(PacsEvent::Incompatible(format!(
                "Could not open WebView login window: {e}"
            )));
            let _ = event_tx.send(PacsEvent::WebViewClosed);
            egui_ctx.request_repaint();
            return;
        }
    };

    let mut stdin = child.stdin.take();
    std::thread::spawn(move || {
        while let Ok(cmd) = cmd_rx.recv() {
            let Some(ref mut w) = stdin else { break };
            let is_close = matches!(cmd, WebViewCommand::Close);
            let line = match cmd {
                WebViewCommand::HookSession => "HOOK\n".to_string(),
                WebViewCommand::AutoLogin(creds) => format!("AUTO_LOGIN\t{creds}\n"),
                WebViewCommand::StartPick(payload) => format!("START_PICK\t{payload}\n"),
                WebViewCommand::HideWindow => "HIDE\n".to_string(),
                WebViewCommand::VueMotionSearch { query, login_url } => {
                    let json = serde_json::json!({ "query": query, "login_url": login_url });
                    format!("VM_SEARCH\t{json}\n")
                }
                WebViewCommand::VueMotionOpenPatient { patient_index } => {
                    let json = serde_json::json!({ "patient_index": patient_index });
                    format!("VM_OPEN_PATIENT\t{json}\n")
                }
                WebViewCommand::VueMotionDownloadStudy {
                    study_index,
                    anonymize,
                } => {
                    let json = serde_json::json!({
                        "study_index": study_index,
                        "anonymize": anonymize,
                    });
                    format!("VM_DOWNLOAD\t{json}\n")
                }
                WebViewCommand::TrainedSearch { query, profile } => {
                    let json = serde_json::json!({ "query": query, "profile": profile });
                    format!("TRAINED_SEARCH\t{json}\n")
                }
                WebViewCommand::TrainedDownload {
                    row_index,
                    anonymize,
                    profile,
                } => {
                    let json = serde_json::json!({
                        "row_index": row_index,
                        "anonymize": anonymize,
                        "profile": profile,
                    });
                    format!("TRAINED_DOWNLOAD\t{json}\n")
                }
                WebViewCommand::Close => "CLOSE\n".to_string(),
            };
            if w.write_all(line.as_bytes()).is_err() || w.flush().is_err() {
                break;
            }
            if is_close {
                break;
            }
        }
    });

    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            let Ok(msg) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let Some(msg_type) = msg.get("type").and_then(|x| x.as_str()) else {
                continue;
            };
            let rest = msg.get("payload").and_then(|x| x.as_str()).unwrap_or("");
            match msg_type {
                "HOOK_DATA" => {
                    crate::crash_log::log_line("INFO", &format!("Received HOOK_DATA from WebView subprocess ({} bytes)", rest.len()));
                    if let Ok(v) = serde_json::from_str::<Value>(rest) {
                        let cookie_header = v.get("cookies").and_then(|x| x.as_str()).unwrap_or("").to_string();
                        let is_vue_motion = v.get("vue_motion").and_then(|x| x.as_bool()).unwrap_or(false);
                        crate::crash_log::log_line("INFO", &format!("HOOK_DATA: is_vue_motion={is_vue_motion}, cookies_len={}", cookie_header.len()));
                        let base_origin = portal_origin.clone();
                        let tx = event_tx.clone();
                        let ctx = egui_ctx.clone();
                        if is_vue_motion {
                            let _ = tx.send(PacsEvent::Hooked {
                                backend: PacsBackend::VueMotion {
                                    origin: base_origin,
                                },
                                cookie_header,
                            });
                            ctx.request_repaint();
                        } else {
                            std::thread::spawn(move || {
                                match probe_pacs_backend(&base_origin, &cookie_header) {
                                    Some(backend) => {
                                        crate::crash_log::log_line("INFO", &format!("probe_pacs_backend matched: {}", backend.label()));
                                        let _ = tx.send(PacsEvent::Hooked { backend, cookie_header });
                                    }
                                    None => {
                                        crate::crash_log::log_line("WARN", &format!("probe_pacs_backend returned None for {base_origin}"));
                                        let _ = tx.send(PacsEvent::Incompatible(
                                            "This webpage does not allow hooking into its virtual PACS system."
                                                .into(),
                                        ));
                                    }
                                }
                                ctx.request_repaint();
                            });
                        }
                    }
                }
                "PATIENT_RESULTS" => {
                    if let Ok(arr) = serde_json::from_str::<Vec<Value>>(rest) {
                        let list = arr
                            .into_iter()
                            .take(50)
                            .enumerate()
                            .map(|(i, item)| PacsPatientResult {
                                index: item
                                    .get("index")
                                    .and_then(|x| x.as_u64())
                                    .map(|n| n as usize)
                                    .unwrap_or(i),
                                name: item
                                    .get("name")
                                    .and_then(|x| x.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                summary: item
                                    .get("summary")
                                    .and_then(|x| x.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                            })
                            .collect();
                        let _ = event_tx.send(PacsEvent::PatientResults(Ok(list)));
                        egui_ctx.request_repaint();
                    }
                }
                "PATIENT_STUDIES" => {
                    if let Ok(arr) = serde_json::from_str::<Vec<Value>>(rest) {
                        let list = arr
                            .into_iter()
                            .take(50)
                            .enumerate()
                            .map(|(i, item)| {
                                let idx = item
                                    .get("index")
                                    .and_then(|x| x.as_u64())
                                    .map(|n| n as usize)
                                    .unwrap_or(i);
                                PacsStudyResult {
                                    id: idx.to_string(),
                                    study_uid: format!("vm-study-{idx}"),
                                    patient_name: String::new(),
                                    patient_id: String::new(),
                                    study_date: item
                                        .get("date")
                                        .and_then(|x| x.as_str())
                                        .unwrap_or("")
                                        .to_string(),
                                    modality: item
                                        .get("modality")
                                        .and_then(|x| x.as_str())
                                        .unwrap_or("")
                                        .to_string(),
                                    description: item
                                        .get("description")
                                        .and_then(|x| x.as_str())
                                        .unwrap_or("")
                                        .to_string(),
                                    has_report: item
                                        .get("has_report")
                                        .and_then(|x| x.as_bool())
                                        .unwrap_or(false),
                                }
                            })
                            .collect();
                        let _ = event_tx.send(PacsEvent::SearchResults(Ok(list)));
                        egui_ctx.request_repaint();
                    }
                }
                "PICKED" => {
                    if let Ok(v) = serde_json::from_str::<Value>(rest) {
                        let step = v.get("step").and_then(|x| x.as_str()).unwrap_or("").to_string();
                        let selector = v.get("selector").and_then(|x| x.as_str()).unwrap_or("").to_string();
                        let row_selector = v.get("row_selector").and_then(|x| x.as_str()).unwrap_or("").to_string();
                        let in_row = v.get("in_row").and_then(|x| x.as_bool()).unwrap_or(false);
                        let rel_selector = v.get("rel_selector").and_then(|x| x.as_str()).unwrap_or("").to_string();
                        let _ = event_tx.send(PacsEvent::PickedElement {
                            step,
                            selector,
                            row_selector,
                            in_row,
                            rel_selector,
                        });
                        egui_ctx.request_repaint();
                    }
                }
                "SEARCH_RESULTS" => {
                    if let Ok(arr) = serde_json::from_str::<Vec<Value>>(rest) {
                        let list = arr
                            .into_iter()
                            .take(50)
                            .enumerate()
                            .map(|(i, item)| {
                                let idx = item
                                    .get("index")
                                    .and_then(|x| x.as_u64())
                                    .map(|n| n as usize)
                                    .unwrap_or(i);
                                let text = item
                                    .get("text")
                                    .and_then(|x| x.as_str())
                                    .unwrap_or("")
                                    .split_whitespace()
                                    .collect::<Vec<_>>()
                                    .join(" ");
                                PacsStudyResult {
                                    id: idx.to_string(),
                                    study_uid: format!("row-{idx}"),
                                    patient_name: text,
                                    patient_id: String::new(),
                                    study_date: String::new(),
                                    modality: String::new(),
                                    description: String::new(),
                                    has_report: false,
                                }
                            })
                            .collect();
                        let _ = event_tx.send(PacsEvent::SearchResults(Ok(list)));
                        egui_ctx.request_repaint();
                    }
                }
                "SEARCH_ERR" => {
                    let _ = event_tx.send(PacsEvent::SearchResults(Err(rest.to_string())));
                    egui_ctx.request_repaint();
                }
                "DOWNLOADED" => {
                    if download_requested.swap(false, Ordering::SeqCst) {
                        let candidate = PathBuf::from(rest);
                        let inside_watch = canonical_watch_dir.as_ref().is_some_and(|w| {
                            std::fs::canonicalize(&candidate)
                                .map(|c| c.starts_with(w))
                                .unwrap_or(false)
                        });
                        if inside_watch {
                            let _ = event_tx.send(PacsEvent::DownloadFinished(candidate));
                            egui_ctx.request_repaint();
                        } else {
                            let _ = event_tx.send(PacsEvent::DownloadFailed(
                                "Downloaded file path rejected: outside watch folder.".into(),
                            ));
                            egui_ctx.request_repaint();
                        }
                    }
                }
                "DL_FAILED" => {
                    download_requested.store(false, Ordering::SeqCst);
                    let _ = event_tx.send(PacsEvent::DownloadFailed(rest.to_string()));
                    egui_ctx.request_repaint();
                }
                "INCOMPATIBLE" => {
                    let _ = event_tx.send(PacsEvent::Incompatible(rest.to_string()));
                    egui_ctx.request_repaint();
                }
                _ => {}
            }
        }
    }

    let status = child.wait();
    crate::crash_log::log_line("INFO", &format!("PACS WebView subprocess exited: {status:?}"));
    if let Ok(st) = status
        && !st.success() {
            let _ = event_tx.send(PacsEvent::Incompatible(format!(
                "WebView login window closed unexpectedly ({st}). See sekhmetx.log for details."
            )));
        }
    let _ = event_tx.send(PacsEvent::WebViewClosed);
    egui_ctx.request_repaint();
}

pub fn run_webview_subprocess(login_url: &str, watch_dir: PathBuf, hidden: bool) {
    use std::io::BufRead;
    use tao::event::{Event, WindowEvent};
    use tao::event_loop::ControlFlow;
    use tao::window::WindowBuilder;
    use wry::WebViewBuilder;

    let Ok(portal_origin) = parse_https_origin(login_url) else {
        emit_subprocess_json("INCOMPATIBLE", "Portal URL must be a valid https:// address.");
        return;
    };

    crate::crash_log::log_line("INFO", &format!("WebView subprocess starting for {portal_origin}"));
    let event_loop = tao::event_loop::EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        for line in stdin.lock().lines().map_while(Result::ok) {
            let trimmed = line.trim();
            let ev = if trimmed == "HOOK" {
                UserEvent::HookSession
            } else if let Some(rest) = trimmed.strip_prefix("AUTO_LOGIN\t") {
                UserEvent::AutoLogin(rest.to_string())
            } else if trimmed == "HIDE" {
                UserEvent::HideWindow
            } else if trimmed == "CLOSE" {
                UserEvent::Close
            } else if let Some(rest) = trimmed.strip_prefix("VM_SEARCH\t") {
                UserEvent::VueMotionSearch(rest.to_string())
            } else if let Some(rest) = trimmed.strip_prefix("VM_OPEN_PATIENT\t") {
                UserEvent::VueMotionOpenPatient(rest.to_string())
            } else if let Some(rest) = trimmed.strip_prefix("VM_DOWNLOAD\t") {
                UserEvent::VueMotionDownloadStudy(rest.to_string())
            } else if let Some(rest) = trimmed.strip_prefix("START_PICK\t") {
                UserEvent::StartPick(rest.to_string())
            } else if let Some(rest) = trimmed.strip_prefix("TRAINED_SEARCH\t") {
                UserEvent::TrainedSearch(rest.to_string())
            } else if let Some(rest) = trimmed.strip_prefix("TRAINED_DOWNLOAD\t") {
                UserEvent::TrainedDownload(rest.to_string())
            } else {
                continue;
            };
            if proxy.send_event(ev).is_err() {
                break;
            }
        }
    });

    let Ok(window) = WindowBuilder::new()
        .with_title("SekhmetX — Virtual PACS Login")
        .with_inner_size(tao::dpi::LogicalSize::new(1080.0, 740.0))
        .with_visible(!hidden)
        .build(&event_loop)
    else {
        crate::crash_log::log_line("ERROR", "WindowBuilder::build failed in WebView subprocess");
        emit_subprocess_json("INCOMPATIBLE", "Could not create WebView login window on this system.");
        return;
    };

    if hidden {
        let auto_proxy = event_loop.create_proxy();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(2500));
            let _ = auto_proxy.send_event(UserEvent::HiddenHookCheck(0));
        });
    }

    let web_data_dir = crate::crash_log::app_data_dir().join("webview");
    let _ = std::fs::create_dir_all(&web_data_dir);
    let mut web_context = wry::WebContext::new(Some(web_data_dir));

    let dl_dir = watch_dir.to_path_buf();
    let nav_origin = portal_origin.clone();
    let ipc_origin = portal_origin.clone();
    let pending_hook_cookies: Arc<std::sync::Mutex<Option<String>>> =
        Arc::new(std::sync::Mutex::new(None));
    let hidden_hook_attempt: Arc<std::sync::atomic::AtomicU8> =
        Arc::new(std::sync::atomic::AtomicU8::new(u8::MAX));
    let ipc_cookies = Arc::clone(&pending_hook_cookies);
    let ipc_hidden_attempt = Arc::clone(&hidden_hook_attempt);
    let ipc_retry_proxy = event_loop.create_proxy();

    let record_requests = std::env::var("SEKHMETX_RECORD_PACS_REQUESTS").as_deref() == Ok("1");
    let mut builder = WebViewBuilder::with_web_context(&mut web_context)
        .with_url(login_url)
        .with_navigation_handler(move |url| {
            parse_https_origin(&url).as_deref() == Ok(nav_origin.as_str())
        })
        .with_new_window_req_handler(|_| false)
        .with_ipc_handler(move |req| {
            let req_uri = req.uri().to_string();
            if parse_https_origin(&req_uri).as_deref() != Ok(ipc_origin.as_str()) {
                crate::crash_log::log_line("WARN", &format!("WebView IPC dropped due to origin mismatch: uri={req_uri} expected={ipc_origin}"));
                return;
            }
            let body = req.body();
            if body.contains('\n') || body.contains('\r') {
                return;
            }
            let Some((tag, payload)) = body.split_once('\t') else {
                return;
            };
            if tag == "REQ_REC" {
                crate::crash_log::log_line("INFO", &format!("PACS request recorded: {payload}"));
                return;
            }
            if tag == "VM_CHECK" {
                crate::crash_log::log_line("INFO", &format!("WebView IPC VM_CHECK={payload}"));
                let is_vm = payload.trim() == "1";
                let attempt = ipc_hidden_attempt.swap(u8::MAX, std::sync::atomic::Ordering::SeqCst);
                if !is_vm && attempt < 8 {
                    let px = ipc_retry_proxy.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(1000));
                        let _ = px.send_event(UserEvent::HiddenHookCheck(attempt + 1));
                    });
                    return;
                }
                let cookies = ipc_cookies
                    .lock()
                    .ok()
                    .and_then(|mut g| g.take())
                    .unwrap_or_default();
                let hook_payload = serde_json::json!({
                    "cookies": cookies,
                    "vue_motion": is_vm,
                });
                emit_subprocess_json("HOOK_DATA", &hook_payload.to_string());
                return;
            }
            if matches!(
                tag,
                "PICKED"
                    | "PATIENT_RESULTS"
                    | "PATIENT_STUDIES"
                    | "SEARCH_RESULTS"
                    | "SEARCH_ERR"
                    | "DL_FAILED"
            ) {
                if matches!(tag, "PATIENT_RESULTS" | "PATIENT_STUDIES" | "SEARCH_ERR" | "DL_FAILED") {
                    crate::crash_log::log_line(
                        "INFO",
                        &format!("WebView IPC {tag}: {} bytes", payload.len()),
                    );
                }
                emit_subprocess_json(tag, payload);
            }
        })
        .with_download_started_handler(move |uri, dest| {
            let _ = std::fs::create_dir_all(&dl_dir);
            let suggested_dest = dest
                .file_name()
                .and_then(|n| n.to_str())
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.to_string());
            let raw_name = suggested_dest.unwrap_or_else(|| {
                uri.split('?')
                    .next()
                    .and_then(|s| s.rsplit('/').next())
                    .filter(|s| !s.is_empty())
                    .unwrap_or("pacs_study.zip")
                    .to_string()
            });
            let mut safe_name: String = raw_name
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            if !safe_name.to_ascii_lowercase().ends_with(".zip") {
                safe_name.push_str(".zip");
            }
            *dest = dl_dir.join(safe_name);
            true
        })
        .with_download_completed_handler(move |_uri, path, success| {
            if success {
                if let Some(p) = path {
                    emit_subprocess_json("DOWNLOADED", &p.to_string_lossy());
                }
            } else {
                emit_subprocess_json("DL_FAILED", "Browser download from PACS portal failed.");
            }
        });
    if record_requests {
        builder = builder.with_initialization_script(
            r#"(function() {
  if (window.__sekhmetReqRecorder) return;
  window.__sekhmetReqRecorder = true;
  function extractKeys(body) {
    if (!body) return [];
    if (typeof URLSearchParams !== 'undefined' && body instanceof URLSearchParams) {
      return Array.from(body.keys());
    }
    if (typeof FormData !== 'undefined' && body instanceof FormData) {
      return Array.from(body.keys());
    }
    if (typeof body === 'string') {
      try {
        const j = JSON.parse(body);
        if (j && typeof j === 'object') return Object.keys(j);
      } catch (_) {}
      return body.split('&').map(p => p.split('=')[0]).filter(Boolean);
    }
    return [];
  }
  const origFetch = window.fetch;
  if (origFetch) {
    window.fetch = function(input, init) {
      try {
        const u = new URL(typeof input === 'string' ? input : input.url, location.href);
        const method = (init && init.method) || 'GET';
        const keys = extractKeys(init && init.body);
        window.ipc.postMessage('REQ_REC\t' + JSON.stringify({ method, path: u.pathname, keys }));
      } catch (_) {}
      return origFetch.apply(this, arguments);
    };
  }
  const origOpen = XMLHttpRequest.prototype.open;
  const origSend = XMLHttpRequest.prototype.send;
  XMLHttpRequest.prototype.open = function(method, url) {
    this.__smMethod = method;
    this.__smUrl = url;
    return origOpen.apply(this, arguments);
  };
  XMLHttpRequest.prototype.send = function(body) {
    try {
      const u = new URL(this.__smUrl || '', location.href);
      const keys = extractKeys(body);
      window.ipc.postMessage('REQ_REC\t' + JSON.stringify({ method: this.__smMethod || 'GET', path: u.pathname, keys }));
    } catch (_) {}
    return origSend.apply(this, arguments);
  };
})();"#,
        );
    }
    let Ok(webview) = builder.build(&window)
    else {
        crate::crash_log::log_line("ERROR", "WebViewBuilder::build failed in WebView subprocess");
        emit_subprocess_json(
            "INCOMPATIBLE",
            "This webpage or WebView runtime does not allow hooking into its virtual PACS system.",
        );
        return;
    };

    let fallback_url = login_url.to_string();
    let retry_proxy = event_loop.create_proxy();
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            }
            | Event::UserEvent(UserEvent::Close) => {
                *control_flow = ControlFlow::Exit;
            }
            Event::UserEvent(UserEvent::HideWindow) => {
                window.set_visible(false);
            }
            Event::UserEvent(UserEvent::AutoLogin(payload)) => {
                let parsed = serde_json::from_str::<Value>(&payload).ok();
                let expected_origin = parsed
                    .as_ref()
                    .and_then(|v| v.get("expected_origin").and_then(|x| x.as_str()))
                    .map(|s| s.to_string());
                let attempt = parsed
                    .as_ref()
                    .and_then(|v| v.get("attempt").and_then(|x| x.as_u64()))
                    .unwrap_or(0);
                let cur_origin = webview
                    .url()
                    .ok()
                    .and_then(|u| parse_https_origin(&u).ok());
                if cur_origin.is_none() {
                    if attempt < 10 {
                        let mut next = parsed.unwrap_or_else(|| serde_json::json!({}));
                        if let Some(obj) = next.as_object_mut() {
                            obj.insert("attempt".to_string(), serde_json::json!(attempt + 1));
                        }
                        let next_payload = next.to_string();
                        let px = retry_proxy.clone();
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(500));
                            let _ = px.send_event(UserEvent::AutoLogin(next_payload));
                        });
                    }
                } else if expected_origin.is_some() && cur_origin == expected_origin {
                    let js = build_auto_login_js(&payload);
                    let _ = webview.evaluate_script(&js);
                } else {
                    emit_subprocess_json("SEARCH_ERR", "Login page address changed");
                }
            }
            Event::UserEvent(UserEvent::StartPick(payload)) => {
                let js = build_picker_js(&payload);
                let _ = webview.evaluate_script(&js);
            }
            Event::UserEvent(UserEvent::TrainedSearch(payload)) => {
                let js = build_trained_search_js(&payload);
                let _ = webview.evaluate_script(&js);
            }
            Event::UserEvent(UserEvent::TrainedDownload(payload)) => {
                let js = build_trained_download_js(&payload);
                let _ = webview.evaluate_script(&js);
            }
            Event::UserEvent(UserEvent::VueMotionSearch(payload)) => {
                let js = build_vuemotion_search_js(&payload);
                let _ = webview.evaluate_script(&js);
            }
            Event::UserEvent(UserEvent::VueMotionOpenPatient(payload)) => {
                let js = build_vuemotion_open_patient_js(&payload);
                let _ = webview.evaluate_script(&js);
            }
            Event::UserEvent(UserEvent::VueMotionDownloadStudy(payload)) => {
                let js = build_vuemotion_download_js(&payload);
                let _ = webview.evaluate_script(&js);
            }
            Event::UserEvent(UserEvent::HookSession)
            | Event::UserEvent(UserEvent::HiddenHookCheck(_)) => {
                if let Event::UserEvent(UserEvent::HiddenHookCheck(attempt)) = event {
                    hidden_hook_attempt.store(attempt, std::sync::atomic::Ordering::SeqCst);
                } else {
                    hidden_hook_attempt.store(u8::MAX, std::sync::atomic::Ordering::SeqCst);
                }
                let current_url = webview.url().unwrap_or_else(|_| fallback_url.clone());
                crate::crash_log::log_line("INFO", &format!("HookSession invoked on url={current_url} (expected={portal_origin})"));
                if parse_https_origin(&current_url).as_deref() != Ok(portal_origin.as_str()) {
                    emit_subprocess_json("INCOMPATIBLE", "Login page origin does not match portal URL.");
                    return;
                }
                let cookie_header = webview
                    .cookies_for_url(&current_url)
                    .map(|list| {
                        list.into_iter()
                            .map(|c| format!("{}={}", c.name(), c.value()))
                            .collect::<Vec<_>>()
                            .join("; ")
                    })
                    .unwrap_or_default();
                if let Ok(mut guard) = pending_hook_cookies.lock() {
                    *guard = Some(cookie_header.clone());
                }
                let js = r#"(function() {
  let vm = false;
  try {
    const checkDoc = (d) => !!(d && d.querySelector('#sptGeneralDetailsInput') && (d.querySelector('#dataTableID') || d.querySelector('#sptSearchSubmitBtn')));
    vm = checkDoc(document);
    if (!vm) {
      for (const fr of document.querySelectorAll('iframe')) {
        try {
          if (checkDoc(fr.contentDocument)) { vm = true; break; }
        } catch (_) {}
      }
    }
  } catch (_) {}
  window.ipc.postMessage('VM_CHECK\t' + (vm ? '1' : '0'));
})();"#;
                if let Err(e) = webview.evaluate_script(js) {
                    crate::crash_log::log_line("WARN", &format!("HookSession evaluate_script failed: {e}"));
                    let cookies = pending_hook_cookies
                        .lock()
                        .ok()
                        .and_then(|mut g| g.take())
                        .unwrap_or(cookie_header);
                    let hook_payload = serde_json::json!({
                        "cookies": cookies,
                        "vue_motion": false,
                    });
                    emit_subprocess_json("HOOK_DATA", &hook_payload.to_string());
                }
            }
            _ => {}
        }
    });
}

