mod backends;
mod js_scripts;
mod secrets;
mod subprocess;
pub use backends::{download_pacs_study_zip, search_pacs_studies};
use backends::{anonymize_zip_archive, probe_pacs_backend};
use js_scripts::*;
pub use secrets::*;
pub use subprocess::run_webview_subprocess;
use subprocess::spawn_webview_subprocess;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use anyhow::{Context, Result};
use eframe::egui;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ClickStep {
    pub selector: String,
    pub in_row: bool,
    pub rel_selector: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TrainedPortalProfile {
    pub origin: String,
    pub search_input_sel: String,
    pub row_list_sel: String,
    #[serde(default)]
    pub download_steps: Vec<ClickStep>,
    #[serde(default)]
    pub anon_steps: Vec<ClickStep>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrainingStep {
    Idle,
    PickSearchInput,
    PickResultsRow,
    PickDownloadBtn,
    PickAnonBtn,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PacsBackend {
    Orthanc {
        base_url: String,
    },
    DicomWeb {
        base_url: String,
        qido_prefix: String,
        wado_prefix: String,
    },
    VueMotion {
        origin: String,
    },
    TrainedDom(TrainedPortalProfile),
}

impl PacsBackend {
    pub fn label(&self) -> String {
        match self {
            Self::Orthanc { base_url } => format!("Orthanc ({base_url})"),
            Self::DicomWeb {
                base_url,
                qido_prefix,
                ..
            } => format!("DICOMweb ({base_url}{qido_prefix})"),
            Self::VueMotion { origin } => format!("Vue Motion ({origin})"),
            Self::TrainedDom(p) => format!("Trained Portal ({})", p.origin),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PacsHookState {
    Disconnected,
    WaitingLogin,
    Hooked {
        backend: PacsBackend,
        cookie_header: String,
    },
    Incompatible(String),
}

#[derive(Clone, Debug)]
pub struct PacsPatientResult {
    pub index: usize,
    pub name: String,
    pub summary: String,
}

#[derive(Clone, Debug)]
pub struct PacsStudyResult {
    pub id: String,
    pub study_uid: String,
    pub patient_name: String,
    pub patient_id: String,
    pub study_date: String,
    pub modality: String,
    pub description: String,
    pub has_report: bool,
}

#[derive(Debug)]
pub enum PacsEvent {
    Hooked {
        backend: PacsBackend,
        cookie_header: String,
    },
    Incompatible(String),
    PickedElement {
        step: String,
        selector: String,
        row_selector: String,
        in_row: bool,
        rel_selector: String,
    },
    PatientResults(Result<Vec<PacsPatientResult>, String>),
    SearchResults(Result<Vec<PacsStudyResult>, String>),
    DownloadFinished(PathBuf),
    DownloadFailed(String),
    WebViewClosed,
}

#[derive(Debug, Clone)]
pub enum WebViewCommand {
    HookSession,
    AutoLogin(String),
    StartPick(String),
    HideWindow,
    VueMotionSearch {
        query: String,
        login_url: String,
    },
    VueMotionOpenPatient {
        patient_index: usize,
    },
    VueMotionDownloadStudy {
        study_index: usize,
        anonymize: bool,
    },
    TrainedSearch {
        query: String,
        profile: TrainedPortalProfile,
    },
    TrainedDownload {
        row_index: usize,
        anonymize: bool,
        profile: TrainedPortalProfile,
    },
    Close,
}

#[derive(Debug, Clone)]
enum UserEvent {
    HookSession,
    HiddenHookCheck(u8),
    AutoLogin(String),
    StartPick(String),
    HideWindow,
    VueMotionSearch(String),
    VueMotionOpenPatient(String),
    VueMotionDownloadStudy(String),
    TrainedSearch(String),
    TrainedDownload(String),
    Close,
}

#[derive(Debug, Clone)]
enum PendingPacsAction {
    Search(String),
    OpenPatient(PacsPatientResult),
    Download(PacsStudyResult),
}

pub struct PacsWebController {
    pub institution_name: String,
    pub login_url: String,
    pub watch_dir: Option<PathBuf>,
    pub saved_username: String,
    pub saved_password: zeroize::Zeroizing<String>,
    pub remember_credentials: bool,
    pub download_anonymized: bool,
    pub state: PacsHookState,
    pub search_query: String,
    pub searching: bool,
    pub loading_studies: bool,
    pub downloading: Option<String>,
    pub patient_results: Vec<PacsPatientResult>,
    pub selected_patient: Option<PacsPatientResult>,
    pub results: Vec<PacsStudyResult>,
    pub status_msg: Option<String>,
    pub pending_open_downloaded: bool,
    pub just_hooked: bool,
    pub silent_auto_connect: bool,
    pub lang: crate::i18n::Lang,
    pub training_step: TrainingStep,
    pub training_draft: TrainedPortalProfile,
    pub newly_trained_profile: Option<TrainedPortalProfile>,
    pub tx: Sender<PacsEvent>,
    pub rx: Receiver<PacsEvent>,
    cmd_tx: Option<Sender<WebViewCommand>>,
    pending_action: Option<PendingPacsAction>,
    known_zips: HashMap<PathBuf, (u64, u64)>,
    download_requested: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl PacsWebController {
    pub fn new(institution_name: String, login_url: String, watch_dir: Option<PathBuf>) -> Self {
        let (tx, rx) = mpsc::channel();
        let known_zips = watch_dir
            .as_deref()
            .map(load_known_zips_cache)
            .unwrap_or_default();
        Self {
            institution_name,
            login_url,
            watch_dir,
            saved_username: String::new(),
            saved_password: zeroize::Zeroizing::new(String::new()),
            remember_credentials: false,
            download_anonymized: false,
            state: PacsHookState::Disconnected,
            search_query: String::new(),
            searching: false,
            loading_studies: false,
            downloading: None,
            patient_results: Vec::new(),
            selected_patient: None,
            results: Vec::new(),
            status_msg: None,
            pending_open_downloaded: false,
            just_hooked: false,
            silent_auto_connect: false,
            lang: crate::i18n::Lang::En,
            training_step: TrainingStep::Idle,
            training_draft: TrainedPortalProfile::default(),
            newly_trained_profile: None,
            tx,
            rx,
            cmd_tx: None,
            pending_action: None,
            known_zips,
            download_requested: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    pub fn reload_watch_cache(&mut self) {
        self.known_zips = self
            .watch_dir
            .as_deref()
            .map(load_known_zips_cache)
            .unwrap_or_default();
    }

    pub fn is_webview_open(&self) -> bool {
        self.cmd_tx.is_some()
    }

    pub fn open_login_webview(&mut self, ctx: egui::Context) {
        self.open_login_webview_inner(ctx, false);
    }

    pub fn try_auto_reconnect(&mut self) {
        let url = self.login_url.trim();
        if url.is_empty() || self.watch_dir.is_none() {
            return;
        }
        let tr = crate::i18n::tr(self.lang);
        let backend = PacsBackend::VueMotion {
            origin: origin_of(url),
        };
        self.status_msg = Some(format!("{} {}", tr.pacs_msg_hooked, backend.label()));
        self.state = PacsHookState::Hooked {
            backend,
            cookie_header: String::new(),
        };
    }

    pub fn close_on_demand_webview(&mut self) {
        if self.cmd_tx.is_some()
            && self.downloading.is_none()
            && !self.searching
            && !self.loading_studies
            && self.training_step == TrainingStep::Idle
            && matches!(self.state, PacsHookState::Hooked { .. })
        {
            self.close_webview();
        }
    }

    fn open_login_webview_inner(&mut self, ctx: egui::Context, hidden: bool) {
        let tr = crate::i18n::tr(self.lang);
        let url = self.login_url.trim().to_string();
        if url.is_empty() {
            if !hidden {
                self.status_msg = Some(tr.pacs_msg_enter_url.into());
            }
            return;
        }
        let portal_origin = match parse_https_origin(&url) {
            Ok(o) => o,
            Err(err) => {
                if !hidden {
                    self.status_msg = Some(err);
                }
                return;
            }
        };
        let Some(watch_dir) = self.watch_dir.clone() else {
            if !hidden {
                self.status_msg = Some(tr.pacs_msg_select_watch.into());
            }
            return;
        };
        if let Err(e) = std::fs::create_dir_all(&watch_dir) {
            if !hidden {
                self.status_msg = Some(format!("Cannot create watch folder: {e}"));
            }
            return;
        }

        if let Some(old) = self.cmd_tx.take() {
            let _ = old.send(WebViewCommand::Close);
        }

        // F1: Try saved session cookies from the OS keyring directly via ureq before spawning a WebView.
        if let Ok(saved_cookies) = load_pacs_cookies(&portal_origin)
            && !saved_cookies.is_empty()
        {
            let probe_origin = portal_origin.clone();
            let cookie_str = saved_cookies.to_string();
            let tx = self.tx.clone();
            let ctx_probe = ctx.clone();
            if let Some(backend) = probe_pacs_backend(&probe_origin, &cookie_str) {
                let _ = tx.send(PacsEvent::Hooked {
                    backend,
                    cookie_header: cookie_str,
                });
                ctx_probe.request_repaint();
                return;
            }
        }

        let (cmd_tx, cmd_rx) = mpsc::channel::<WebViewCommand>();
        if !self.saved_username.trim().is_empty() {
            let pass_opt = if !self.saved_password.is_empty() {
                Some(self.saved_password.clone())
            } else if self.remember_credentials {
                load_pacs_password(&portal_origin).ok()
            } else {
                None
            };
            if let Some(pass) = pass_opt
                && !pass.is_empty() {
                    let creds = zeroize::Zeroizing::new(
                        serde_json::json!({
                            "user": self.saved_username.trim(),
                            "pass": &*pass,
                            "expected_origin": portal_origin,
                        })
                        .to_string(),
                    );
                    let _ = cmd_tx.send(WebViewCommand::AutoLogin((*creds).clone()));
                }
        }
        self.cmd_tx = Some(cmd_tx);
        self.silent_auto_connect = hidden;
        if !hidden {
            self.state = PacsHookState::WaitingLogin;
            self.status_msg = Some(tr.pacs_msg_login_prompt.into());
        }

        let event_tx = self.tx.clone();
        let dl_req = self.download_requested.clone();
        std::thread::spawn(move || {
            spawn_webview_subprocess(url, watch_dir, hidden, dl_req, cmd_rx, event_tx, ctx);
        });
    }

    pub fn trigger_auto_login(&mut self, ctx: egui::Context) {
        let Ok(portal_origin) = parse_https_origin(&self.login_url) else {
            self.status_msg = Some(crate::i18n::tr(self.lang).pacs_msg_enter_url.into());
            return;
        };
        if let Some(ref cmd_tx) = self.cmd_tx {
            if !self.saved_username.trim().is_empty() {
                let pass_opt = if !self.saved_password.is_empty() {
                    Some(self.saved_password.clone())
                } else if self.remember_credentials {
                    load_pacs_password(&portal_origin).ok()
                } else {
                    None
                };
                if let Some(pass) = pass_opt
                    && !pass.is_empty()
                {
                    let creds = zeroize::Zeroizing::new(
                        serde_json::json!({
                            "user": self.saved_username.trim(),
                            "pass": &*pass,
                            "expected_origin": portal_origin,
                        })
                        .to_string(),
                    );
                    let _ = cmd_tx.send(WebViewCommand::AutoLogin((*creds).clone()));
                }
            }
        } else {
            self.open_login_webview(ctx);
        }
    }

    pub fn trigger_hook_session(&mut self) {
        let tr = crate::i18n::tr(self.lang);
        self.silent_auto_connect = false;
        if let Some(ref cmd_tx) = self.cmd_tx {
            let _ = cmd_tx.send(WebViewCommand::HookSession);
            self.status_msg = Some(tr.pacs_msg_inspecting.into());
        } else {
            self.status_msg = Some(tr.pacs_msg_open_first.into());
        }
    }

    pub fn close_webview(&mut self) {
        if let Some(cmd_tx) = self.cmd_tx.take() {
            let _ = cmd_tx.send(WebViewCommand::Close);
        }
    }

    pub fn start_training(&mut self) {
        let tr = crate::i18n::tr(self.lang);
        if self.cmd_tx.is_none() {
            self.status_msg = Some(tr.pacs_msg_open_first.into());
            return;
        }
        self.training_draft = TrainedPortalProfile {
            origin: origin_of(self.login_url.trim()),
            ..Default::default()
        };
        self.training_step = TrainingStep::PickSearchInput;
        self.send_pick_command("search_input");
        self.status_msg = Some(tr.pacs_msg_step1.into());
    }

    pub fn trigger_pick_step(&mut self, step: TrainingStep) {
        self.training_step = step;
        let step_name = match step {
            TrainingStep::PickSearchInput => "search_input",
            TrainingStep::PickResultsRow => "row",
            TrainingStep::PickDownloadBtn => "download",
            TrainingStep::PickAnonBtn => "anon",
            TrainingStep::Idle => return,
        };
        self.send_pick_command(step_name);
    }

    fn send_pick_command(&self, step_name: &str) {
        if let Some(ref cmd_tx) = self.cmd_tx {
            let payload = serde_json::json!({
                "step": step_name,
                "row_sel": self.training_draft.row_list_sel,
            })
            .to_string();
            let _ = cmd_tx.send(WebViewCommand::StartPick(payload));
        }
    }

    pub fn finish_download_picking(&mut self) {
        let tr = crate::i18n::tr(self.lang);
        self.training_step = TrainingStep::PickAnonBtn;
        self.send_pick_command("anon");
        self.status_msg = Some(tr.pacs_msg_step4.into());
    }

    pub fn finish_training(&mut self) {
        let profile = self.training_draft.clone();
        self.training_step = TrainingStep::Idle;
        self.newly_trained_profile = Some(profile.clone());
        self.hook_with_trained_profile(profile);
    }

    pub fn hook_with_trained_profile(&mut self, profile: TrainedPortalProfile) {
        let is_vuemotion_profile = profile.search_input_sel.contains("sptGeneralDetailsInput")
            || profile.row_list_sel.contains("dataTableID")
            || profile.row_list_sel.contains("ptRecord");
        let backend = if is_vuemotion_profile {
            PacsBackend::VueMotion {
                origin: profile.origin,
            }
        } else {
            PacsBackend::TrainedDom(profile)
        };
        self.status_msg = Some(format!("Hooked: {}", backend.label()));
        self.state = PacsHookState::Hooked {
            backend,
            cookie_header: String::new(),
        };
        self.just_hooked = true;
        if let Some(ref cmd_tx) = self.cmd_tx {
            let _ = cmd_tx.send(WebViewCommand::HideWindow);
        }
    }

    pub fn disconnect(&mut self) {
        self.close_webview();
        self.state = PacsHookState::Disconnected;
        self.patient_results.clear();
        self.selected_patient = None;
        self.results.clear();
        self.searching = false;
        self.loading_studies = false;
        self.downloading = None;
        self.training_step = TrainingStep::Idle;
        self.status_msg = None;
    }

    pub fn start_search(&mut self, ctx: egui::Context) {
        let PacsHookState::Hooked {
            ref backend,
            ref cookie_header,
        } = self.state
        else {
            return;
        };
        let tr = crate::i18n::tr(self.lang);
        let query = self.search_query.trim().to_string();
        self.selected_patient = None;
        self.patient_results.clear();
        self.results.clear();
        if matches!(backend, PacsBackend::VueMotion { .. }) {
            self.searching = true;
            self.status_msg = Some(tr.pacs_msg_searching_patients.into());
            if let Some(ref cmd_tx) = self.cmd_tx {
                let _ = cmd_tx.send(WebViewCommand::VueMotionSearch {
                    query,
                    login_url: self.login_url.clone(),
                });
            } else {
                self.pending_action = Some(PendingPacsAction::Search(query));
                self.open_login_webview_inner(ctx, true);
            }
            return;
        }
        if let PacsBackend::TrainedDom(profile) = backend {
            let profile = profile.clone();
            self.searching = true;
            self.status_msg = Some(tr.pacs_msg_searching.into());
            if let Some(ref cmd_tx) = self.cmd_tx {
                let _ = cmd_tx.send(WebViewCommand::TrainedSearch {
                    query,
                    profile,
                });
            } else {
                self.pending_action = Some(PendingPacsAction::Search(query));
                self.open_login_webview_inner(ctx, true);
            }
            return;
        }
        let backend = backend.clone();
        let cookies = cookie_header.clone();
        let tx = self.tx.clone();
        self.searching = true;
        self.status_msg = Some(tr.pacs_msg_searching.into());
        std::thread::spawn(move || {
            let res = search_pacs_studies(&backend, &cookies, &query)
                .map_err(|e| format!("{e:#}"));
            let _ = tx.send(PacsEvent::SearchResults(res));
            ctx.request_repaint();
        });
    }

    pub fn open_patient(&mut self, patient: &PacsPatientResult, ctx: egui::Context) {
        let tr = crate::i18n::tr(self.lang);
        self.selected_patient = Some(patient.clone());
        self.results.clear();
        self.loading_studies = true;
        self.status_msg = Some(format!("{} {}…", tr.pacs_msg_loading_studies, patient.name));
        if let Some(ref cmd_tx) = self.cmd_tx {
            let _ = cmd_tx.send(WebViewCommand::VueMotionOpenPatient {
                patient_index: patient.index,
            });
        } else {
            self.pending_action = Some(PendingPacsAction::OpenPatient(patient.clone()));
            self.open_login_webview_inner(ctx, true);
        }
    }

    pub fn back_to_patients(&mut self) {
        self.selected_patient = None;
        self.results.clear();
        self.loading_studies = false;
    }

    pub fn start_download(&mut self, study: &PacsStudyResult, ctx: egui::Context) {
        let PacsHookState::Hooked {
            ref backend,
            ref cookie_header,
        } = self.state
        else {
            return;
        };
        let tr = crate::i18n::tr(self.lang);
        let Some(watch_dir) = self.watch_dir.clone() else {
            self.status_msg = Some(tr.pacs_msg_select_watch.into());
            return;
        };
        let label = if !study.description.is_empty() {
            study.description.clone()
        } else if !study.patient_name.is_empty() {
            study.patient_name.clone()
        } else {
            study.study_uid.clone()
        };
        let anonymize = self.download_anonymized;
        if matches!(backend, PacsBackend::VueMotion { .. }) {
            let study_index = study.id.parse::<usize>().unwrap_or(0);
            self.downloading = Some(label.clone());
            self.pending_open_downloaded = true;
            self.download_requested
                .store(true, std::sync::atomic::Ordering::SeqCst);
            self.status_msg = Some(format!("{} {label}…", tr.pacs_msg_starting_dl));
            if let Some(ref cmd_tx) = self.cmd_tx {
                let _ = cmd_tx.send(WebViewCommand::VueMotionDownloadStudy {
                    study_index,
                    anonymize,
                });
            } else {
                self.pending_action = Some(PendingPacsAction::Download(study.clone()));
                self.open_login_webview_inner(ctx, true);
            }
            return;
        }
        if let PacsBackend::TrainedDom(profile) = backend {
            let profile = profile.clone();
            let row_index = study.id.parse::<usize>().unwrap_or(0);
            self.downloading = Some(label.clone());
            self.pending_open_downloaded = true;
            self.download_requested
                .store(true, std::sync::atomic::Ordering::SeqCst);
            self.status_msg = Some(format!("{} {label}…", tr.pacs_msg_starting_dl));
            if let Some(ref cmd_tx) = self.cmd_tx {
                let _ = cmd_tx.send(WebViewCommand::TrainedDownload {
                    row_index,
                    anonymize,
                    profile,
                });
            } else {
                self.pending_action = Some(PendingPacsAction::Download(study.clone()));
                self.open_login_webview_inner(ctx, true);
            }
            return;
        }
        let backend = backend.clone();
        let cookies = cookie_header.clone();
        let study_id = study.id.clone();
        let study_uid = study.study_uid.clone();
        self.downloading = Some(label.clone());
        self.pending_open_downloaded = true;
        self.status_msg = Some(format!("{} {label}…", tr.pacs_msg_starting_dl));
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            match download_pacs_study_zip(
                &backend,
                &cookies,
                &study_id,
                &study_uid,
                &watch_dir,
                anonymize,
            ) {
                Ok(path) => {
                    let _ = tx.send(PacsEvent::DownloadFinished(path));
                }
                Err(e) => {
                    let _ = tx.send(PacsEvent::DownloadFailed(format!("{e:#}")));
                }
            }
            ctx.request_repaint();
        });
    }

    /// Polls background PACS events and checks the watch folder for new/changed `.zip` files.
    /// Returns any newly detected `.zip` paths that should be scanned into SekhmetX.
    pub fn poll(&mut self, _now_secs: f64) -> Vec<PathBuf> {
        let tr = crate::i18n::tr(self.lang);
        let mut new_zips = Vec::new();
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                PacsEvent::Hooked {
                    backend,
                    cookie_header,
                } => {
                    self.silent_auto_connect = false;
                    self.status_msg = Some(format!("{} {}", tr.pacs_msg_hooked, backend.label()));
                    if !cookie_header.is_empty()
                        && let Ok(origin) = parse_https_origin(&self.login_url)
                    {
                        let _ = store_pacs_cookies(&origin, &cookie_header);
                    }
                    let pending = self.pending_action.take();
                    let keep_webview = matches!(
                        backend,
                        PacsBackend::TrainedDom(_) | PacsBackend::VueMotion { .. }
                    );
                    self.state = PacsHookState::Hooked {
                        backend: backend.clone(),
                        cookie_header,
                    };
                    self.just_hooked = true;
                    if keep_webview {
                        if let Some(ref cmd_tx) = self.cmd_tx {
                            let _ = cmd_tx.send(WebViewCommand::HideWindow);
                            if let Some(act) = pending {
                                match act {
                                    PendingPacsAction::Search(q) => match backend {
                                        PacsBackend::VueMotion { .. } => {
                                            let _ = cmd_tx.send(WebViewCommand::VueMotionSearch {
                                                query: q,
                                                login_url: self.login_url.clone(),
                                            });
                                        }
                                        PacsBackend::TrainedDom(profile) => {
                                            let _ = cmd_tx.send(WebViewCommand::TrainedSearch {
                                                query: q,
                                                profile,
                                            });
                                        }
                                        _ => {}
                                    },
                                    PendingPacsAction::OpenPatient(pt) => {
                                        let _ = cmd_tx.send(WebViewCommand::VueMotionOpenPatient {
                                            patient_index: pt.index,
                                        });
                                    }
                                    PendingPacsAction::Download(st) => {
                                        let idx = st.id.parse::<usize>().unwrap_or(0);
                                        match backend {
                                            PacsBackend::VueMotion { .. } => {
                                                let _ = cmd_tx.send(WebViewCommand::VueMotionDownloadStudy {
                                                    study_index: idx,
                                                    anonymize: self.download_anonymized,
                                                });
                                            }
                                            PacsBackend::TrainedDom(profile) => {
                                                let _ = cmd_tx.send(WebViewCommand::TrainedDownload {
                                                    row_index: idx,
                                                    anonymize: self.download_anonymized,
                                                    profile,
                                                });
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        self.close_webview();
                    }
                }
                PacsEvent::PatientResults(res) => {
                    self.searching = false;
                    match res {
                        Ok(list) => {
                            self.status_msg = Some(format!("{} {}", list.len(), tr.pacs_msg_found_patients));
                            self.patient_results = list;
                        }
                        Err(err) => {
                            self.status_msg = Some(format!("{} {err}", tr.pacs_msg_search_err));
                        }
                    }
                }
                PacsEvent::PickedElement {
                    step,
                    selector,
                    row_selector,
                    in_row,
                    rel_selector,
                } => {
                    match step.as_str() {
                        "search_input" => {
                            self.training_draft.search_input_sel = selector;
                            self.training_step = TrainingStep::PickResultsRow;
                            self.status_msg = Some(tr.pacs_msg_step2.into());
                        }
                        "row" => {
                            self.training_draft.row_list_sel =
                                if row_selector.is_empty() { selector } else { row_selector };
                            self.training_step = TrainingStep::PickDownloadBtn;
                            self.status_msg = Some(tr.pacs_msg_step3.into());
                        }
                        "download" => {
                            self.training_draft.download_steps.push(ClickStep {
                                selector,
                                in_row,
                                rel_selector,
                            });
                            let n = self.training_draft.download_steps.len();
                            self.status_msg = Some(format!("#{} — {}", n, tr.pacs_msg_step3));
                        }
                        "anon" => {
                            self.training_draft.anon_steps.push(ClickStep {
                                selector,
                                in_row,
                                rel_selector,
                            });
                            let n = self.training_draft.anon_steps.len();
                            self.status_msg = Some(format!("#{} — {}", n, tr.pacs_msg_step4));
                        }
                        _ => {}
                    }
                }
                PacsEvent::Incompatible(msg) => {
                    if self.silent_auto_connect {
                        self.silent_auto_connect = false;
                        self.searching = false;
                        self.loading_studies = false;
                        self.downloading = None;
                        self.pending_action = None;
                        self.close_webview();
                        self.state = PacsHookState::Disconnected;
                        self.status_msg = Some(tr.pacs_msg_open_first.into());
                    } else {
                        self.status_msg = Some(msg.clone());
                        self.state = PacsHookState::Incompatible(msg);
                    }
                }
                PacsEvent::SearchResults(res) => {
                    self.searching = false;
                    self.loading_studies = false;
                    match res {
                        Ok(list) => {
                            self.status_msg = Some(format!("{} {}", list.len(), tr.pacs_msg_found_studies));
                            self.results = list;
                        }
                        Err(err) => {
                            self.status_msg = Some(format!("{} {err}", tr.pacs_msg_search_err));
                        }
                    }
                }
                PacsEvent::DownloadFinished(p) => {
                    if self.download_anonymized
                        && let PacsHookState::Hooked {
                            backend: PacsBackend::TrainedDom(ref prof),
                            ..
                        } = self.state
                            && prof.anon_steps.is_empty() {
                                let tmp = p.with_extension("anon.zip.part");
                                if anonymize_zip_archive(&p, &tmp).is_ok() {
                                    let _ = std::fs::rename(&tmp, &p);
                                } else {
                                    let _ = std::fs::remove_file(&tmp);
                                }
                            }
                    self.downloading = None;
                    self.status_msg = Some(format!(
                        "{}: {}",
                        tr.pacs_msg_saved_zip,
                        p.file_name().and_then(|n| n.to_str()).unwrap_or("study.zip")
                    ));
                    if let Ok(meta) = std::fs::metadata(&p) {
                        let len = meta.len();
                        let mtime = meta
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        self.known_zips.insert(p.clone(), (len, mtime));
                        if let Some(ref dir) = self.watch_dir {
                            save_known_zips_cache(dir, &self.known_zips);
                        }
                    }
                    new_zips.push(p);
                }
                PacsEvent::DownloadFailed(err) => {
                    self.downloading = None;
                    self.status_msg = Some(format!("{} {err}", tr.pacs_msg_dl_failed));
                }
                PacsEvent::WebViewClosed => {
                    self.cmd_tx = None;
                    if matches!(self.state, PacsHookState::WaitingLogin) {
                        self.state = PacsHookState::Disconnected;
                    }
                }
            }
        }
        new_zips
    }

    /// Manually checks the Watch Folder once for any new or updated `.zip` files.
    pub fn scan_watch_folder_now(&mut self) -> Vec<PathBuf> {
        let mut new_zips = Vec::new();
        let Some(ref dir) = self.watch_dir else {
            return new_zips;
        };
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let is_zip = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("zip"));
                if !is_zip {
                    continue;
                }
                if let Ok(meta) = entry.metadata() {
                    let len = meta.len();
                    if len == 0 {
                        continue;
                    }
                    let mtime = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let sig = (len, mtime);
                    if self.known_zips.get(&path) != Some(&sig) {
                        self.known_zips.insert(path.clone(), sig);
                        new_zips.push(path);
                    }
                }
            }
        }
        if !new_zips.is_empty() {
            save_known_zips_cache(dir, &self.known_zips);
        }
        new_zips
    }
}

impl Drop for PacsWebController {
    fn drop(&mut self) {
        self.close_webview();
    }
}

fn load_known_zips_cache(dir: &Path) -> HashMap<PathBuf, (u64, u64)> {
    let cache_path = dir.join(".sekhmetx_zips.json");
    let Ok(raw) = std::fs::read_to_string(cache_path) else {
        return HashMap::new();
    };
    let Ok(map) = serde_json::from_str::<HashMap<String, (u64, u64)>>(&raw) else {
        return HashMap::new();
    };
    map.into_iter()
        .map(|(name, sig)| (dir.join(name), sig))
        .collect()
}

fn save_known_zips_cache(dir: &Path, known: &HashMap<PathBuf, (u64, u64)>) {
    let cache_path = dir.join(".sekhmetx_zips.json");
    let map: HashMap<String, (u64, u64)> = known
        .iter()
        .filter_map(|(p, sig)| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| (n.to_string(), *sig))
        })
        .collect();
    if let Ok(json) = serde_json::to_string(&map) {
        let _ = std::fs::write(&cache_path, json);
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            let wide: Vec<u16> = cache_path
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            unsafe {
                // FILE_ATTRIBUTE_HIDDEN = 0x2
                windows_sys_hide(wide.as_ptr());
            }
        }
    }
}

#[cfg(windows)]
unsafe fn windows_sys_hide(path: *const u16) {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetFileAttributesW(lpFileName: *const u16, dwFileAttributes: u32) -> i32;
    }
    unsafe {
        SetFileAttributesW(path, 0x2);
    }
}

pub(crate) fn parse_https_origin(raw: &str) -> Result<String, String> {
    let parsed = url::Url::parse(raw.trim()).map_err(|e| format!("Invalid portal URL: {e}"))?;
    if parsed.scheme() != "https" {
        return Err("Portal URL must use https://".to_string());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("Portal URL must not contain embedded credentials.".to_string());
    }
    if !parsed.has_host() {
        return Err("Portal URL must include a host.".to_string());
    }
    let origin = parsed.origin().ascii_serialization();
    if origin.is_empty() || origin == "null" {
        return Err("Portal URL does not have a valid HTTPS origin.".to_string());
    }
    Ok(origin)
}

pub(crate) fn origin_of(url: &str) -> String {
    url::Url::parse(url.trim())
        .ok()
        .filter(|u| matches!(u.scheme(), "https" | "http") && u.has_host())
        .map(|u| u.origin().ascii_serialization())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::sync::Mutex;

    struct MockSecretStore {
        map: Mutex<HashMap<String, String>>,
        fail: bool,
    }

    impl SecretStore for MockSecretStore {
        fn set_password(&self, account: &str, password: &str) -> Result<(), String> {
            if self.fail {
                return Err("keyring unavailable".into());
            }
            self.map
                .lock()
                .unwrap()
                .insert(account.to_string(), password.to_string());
            Ok(())
        }

        fn get_password(&self, account: &str) -> Result<zeroize::Zeroizing<String>, String> {
            if self.fail {
                return Err("keyring unavailable".into());
            }
            Ok(zeroize::Zeroizing::new(
                self.map
                    .lock()
                    .unwrap()
                    .get(account)
                    .cloned()
                    .unwrap_or_default(),
            ))
        }

        fn delete_password(&self, account: &str) -> Result<(), String> {
            if self.fail {
                return Err("keyring unavailable".into());
            }
            self.map.lock().unwrap().remove(account);
            Ok(())
        }
    }

    #[test]
    fn keyring_store_roundtrip_and_error_without_fallback() {
        let ok_store = MockSecretStore {
            map: Mutex::new(HashMap::new()),
            fail: false,
        };
        store_pacs_password_with(&ok_store, "https://pacs.example.com", "s3cr3t!").unwrap();
        let loaded = load_pacs_password_with(&ok_store, "https://pacs.example.com").unwrap();
        assert_eq!(loaded.as_str(), "s3cr3t!");
        delete_pacs_password_with(&ok_store, "https://pacs.example.com").unwrap();
        let after_del = load_pacs_password_with(&ok_store, "https://pacs.example.com").unwrap();
        assert!(after_del.is_empty());

        let failing_store = MockSecretStore {
            map: Mutex::new(HashMap::new()),
            fail: true,
        };
        assert!(store_pacs_password_with(&failing_store, "https://pacs.example.com", "pw").is_err());
        assert!(load_pacs_password_with(&failing_store, "https://pacs.example.com").is_err());
    }

    #[test]
    fn parse_https_origin_enforces_https_and_no_userinfo() {
        assert_eq!(
            parse_https_origin("https://pacs.example.com:8443/login?next=/"),
            Ok("https://pacs.example.com:8443".to_string())
        );
        assert!(parse_https_origin("http://pacs.example.com/login").is_err());
        assert!(parse_https_origin("javascript:alert(1)").is_err());
        assert!(parse_https_origin("https://user:pass@pacs.example.com/login").is_err());
    }

    #[test]
    fn anonymize_zip_omits_corrupt_files_and_uses_neutral_paths() {
        use dicom_core::{DataElement, PrimitiveValue, VR};
        use dicom_dictionary_std::tags;
        use dicom_object::{FileMetaTableBuilder, InMemDicomObject, StandardDataDictionary};

        let dir = tempfile::TempDir::new().unwrap();
        let valid_dcm = dir.path().join("valid.dcm");
        let mut obj = InMemDicomObject::new_empty_with_dict(StandardDataDictionary);
        obj.put(DataElement::new(
            tags::SOP_CLASS_UID,
            VR::UI,
            PrimitiveValue::from("1.2.840.10008.5.1.4.1.1.2"),
        ));
        obj.put(DataElement::new(
            tags::SOP_INSTANCE_UID,
            VR::UI,
            PrimitiveValue::from("1.2.3.4.5.999"),
        ));
        obj.put(DataElement::new(
            tags::PATIENT_NAME,
            VR::PN,
            PrimitiveValue::from("Doe^John"),
        ));
        obj.with_meta(
            FileMetaTableBuilder::new()
                .transfer_syntax("1.2.840.10008.1.2.1")
                .media_storage_sop_class_uid("1.2.840.10008.5.1.4.1.1.2")
                .media_storage_sop_instance_uid("1.2.3.4.5.999"),
        )
        .unwrap()
        .write_to_file(&valid_dcm)
        .unwrap();

        let src_zip = dir.path().join("src.zip");
        let dst_zip = dir.path().join("dst.zip");
        {
            let f = std::fs::File::create(&src_zip).unwrap();
            let mut zw = zip::ZipWriter::new(f);
            let opts = zip::write::SimpleFileOptions::default();
            zw.start_file("DOE_JOHN/SECRET_NAME.dcm", opts).unwrap();
            zw.write_all(&std::fs::read(&valid_dcm).unwrap()).unwrap();
            zw.start_file("DOE_JOHN/corrupt.dcm", opts).unwrap();
            zw.write_all(b"NOT A DICOM FILE WITH PHI Doe^John").unwrap();
            zw.start_file("DICOMDIR", opts).unwrap();
            zw.write_all(b"DICOMDIR_BYTES").unwrap();
            zw.finish().unwrap();
        }

        anonymize_zip_archive(&src_zip, &dst_zip).unwrap();

        let out_f = std::fs::File::open(&dst_zip).unwrap();
        let mut zr = zip::ZipArchive::new(out_f).unwrap();
        assert_eq!(zr.len(), 1);
        let mut entry = zr.by_index(0).unwrap();
        assert_eq!(entry.name().unwrap().as_ref(), "SER001/IMG0001.dcm");
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        let out_dcm = dir.path().join("extracted.dcm");
        std::fs::write(&out_dcm, &bytes).unwrap();
        let parsed = dicom_object::open_file(&out_dcm).unwrap();
        assert_eq!(
            parsed.element(tags::PATIENT_NAME).unwrap().to_str().unwrap(),
            "ANONYMIZED"
        );
    }

    #[test]
    fn js_script_builders_inject_payload_and_vm_doc_helper() {
        let payload = r#"{"query":"test"}"#;
        let scripts = [
            js_scripts::build_picker_js(payload),
            js_scripts::build_trained_search_js(payload),
            js_scripts::build_trained_download_js(payload),
            js_scripts::build_vuemotion_search_js(payload),
            js_scripts::build_vuemotion_open_patient_js(payload),
            js_scripts::build_vuemotion_download_js(payload),
            js_scripts::build_auto_login_js(payload),
        ];
        for s in scripts {
            assert!(s.contains(payload), "script missing injected payload");
            assert!(!s.contains("__PAYLOAD__"), "unreplaced __PAYLOAD__ placeholder");
            assert!(!s.contains("__VM_DOC__"), "unreplaced __VM_DOC__ placeholder");
        }
    }

    #[test]
    fn trained_portal_profile_roundtrip_json() {
        let profile = TrainedPortalProfile {
            origin: "https://pacs.example.com".to_string(),
            search_input_sel: "#search".to_string(),
            row_list_sel: "tbody > tr".to_string(),
            download_steps: vec![ClickStep {
                selector: "#dl".to_string(),
                in_row: true,
                rel_selector: "button.dl".to_string(),
            }],
            anon_steps: vec![],
        };
        let json = serde_json::to_string(&profile).unwrap();
        let decoded: TrainedPortalProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.origin, profile.origin);
        assert_eq!(decoded.download_steps.len(), 1);
        assert!(decoded.download_steps[0].in_row);
    }

    #[test]
    fn origin_of_extracts_http_and_https_origins_and_rejects_others() {
        assert_eq!(origin_of("https://portal.example.com/a/b"), "https://portal.example.com");
        assert_eq!(origin_of("http://localhost:8042/studies"), "http://localhost:8042");
        assert_eq!(origin_of("file:///etc/passwd"), "");
        assert_eq!(origin_of("javascript:alert(1)"), "");
    }
}
