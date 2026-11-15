mod js_scripts;
mod secrets;
use js_scripts::*;
pub use secrets::*;

use std::collections::HashMap;
use std::io::{Read, Write};
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

fn emit_subprocess_json(msg_type: &str, payload: &str) {
    use std::io::Write;
    let line = serde_json::json!({
        "type": msg_type,
        "payload": payload,
    });
    println!("{line}");
    let _ = std::io::stdout().flush();
}

fn spawn_webview_subprocess(
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

const MAX_PACS_DOWNLOAD_BYTES: u64 = 4 * 1024 * 1024 * 1024;

fn pacs_http_agent(read_timeout_secs: u64) -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .timeout_connect(Some(std::time::Duration::from_secs(15)))
        .timeout_recv_response(Some(std::time::Duration::from_secs(read_timeout_secs)))
        .build();
    config.into()
}

fn http_get_json(url: &str, cookie_header: &str, accept: &str) -> Result<Value> {
    let agent = pacs_http_agent(30);
    let mut req = agent.get(url).header("Accept", accept);
    if !cookie_header.is_empty() {
        req = req.header("Cookie", cookie_header);
    }
    let mut resp = req.call()?;
    let body = resp.body_mut().read_to_string()?;
    let val: Value = serde_json::from_str(&body)?;
    Ok(val)
}

fn probe_pacs_backend(base_url: &str, cookie_header: &str) -> Option<PacsBackend> {
    let base = base_url.trim_end_matches('/');

    // 1. Orthanc REST API probe
    for prefix in ["", "/orthanc"] {
        let probe_url = format!("{base}{prefix}/studies?limit=1");
        if let Ok(Value::Array(_)) = http_get_json(&probe_url, cookie_header, "application/json") {
            return Some(PacsBackend::Orthanc {
                base_url: format!("{base}{prefix}"),
            });
        }
    }

    // 2. DICOMweb (QIDO-RS / WADO-RS) probe
    for prefix in ["/dicom-web", "/qido-rs", "/rs", ""] {
        let probe_url = format!("{base}{prefix}/studies?limit=1");
        if let Ok(Value::Array(_)) =
            http_get_json(&probe_url, cookie_header, "application/dicom+json, application/json")
        {
            let wado_prefix = if prefix == "/qido-rs" {
                "/wado-rs".to_string()
            } else {
                prefix.to_string()
            };
            return Some(PacsBackend::DicomWeb {
                base_url: base.to_string(),
                qido_prefix: prefix.to_string(),
                wado_prefix,
            });
        }
    }

    None
}

fn dicom_json_str(obj: &serde_json::Map<String, Value>, tag: &str) -> String {
    let Some(val_arr) = obj.get(tag).and_then(|v| v.get("Value")).and_then(|v| v.as_array())
    else {
        return String::new();
    };
    let Some(first) = val_arr.first() else {
        return String::new();
    };
    if let Some(s) = first.as_str() {
        return s.to_string();
    }
    if let Some(alpha) = first.get("Alphabetic").and_then(|v| v.as_str()) {
        return alpha.to_string();
    }
    String::new()
}

pub fn search_pacs_studies(
    backend: &PacsBackend,
    cookie_header: &str,
    query: &str,
) -> Result<Vec<PacsStudyResult>> {
    match backend {
        PacsBackend::Orthanc { base_url } => {
            let val = if query.is_empty() {
                http_get_json(&format!("{base_url}/studies?expand&limit=50"), cookie_header, "application/json")?
            } else {
                let body = serde_json::json!({
                    "Level": "Study",
                    "Expand": true,
                    "Limit": 50,
                    "Query": {
                        "PatientName": format!("*{query}*")
                    }
                });
                let mut req = ureq::post(&format!("{base_url}/tools/find"))
                    .header("Content-Type", "application/json");
                if !cookie_header.is_empty() {
                    req = req.header("Cookie", cookie_header);
                }
                let mut resp = req.send(body.to_string())?;
                serde_json::from_str(&resp.body_mut().read_to_string()?)?
            };
            let arr = val.as_array().context("expected JSON array from Orthanc")?;
            let mut out = Vec::with_capacity(arr.len().min(50));
            for item in arr.iter().take(50) {
                let id = item.get("ID").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let pt = item.get("PatientMainDicomTags");
                let st = item.get("MainDicomTags");
                let get = |m: Option<&Value>, k: &str| {
                    m.and_then(|v| v.get(k))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string()
                };
                out.push(PacsStudyResult {
                    id,
                    study_uid: get(st, "StudyInstanceUID"),
                    patient_name: get(pt, "PatientName"),
                    patient_id: get(pt, "PatientID"),
                    study_date: get(st, "StudyDate"),
                    modality: String::new(),
                    description: get(st, "StudyDescription"),
                    has_report: false,
                });
            }
            Ok(out)
        }
        PacsBackend::DicomWeb {
            base_url,
            qido_prefix,
            ..
        } => {
            let mut url = format!("{base_url}{qido_prefix}/studies?limit=50");
            if !query.is_empty() {
                url.push_str(&format!("&PatientName=*{}*", query.replace(' ', "%20")));
            }
            let val = http_get_json(&url, cookie_header, "application/dicom+json, application/json")?;
            let arr = val.as_array().context("expected JSON array from QIDO-RS")?;
            let mut out = Vec::with_capacity(arr.len().min(50));
            for item in arr.iter().take(50) {
                let Some(obj) = item.as_object() else { continue };
                let uid = dicom_json_str(obj, "0020000D");
                out.push(PacsStudyResult {
                    id: uid.clone(),
                    study_uid: uid,
                    patient_name: dicom_json_str(obj, "00100010"),
                    patient_id: dicom_json_str(obj, "00100020"),
                    study_date: dicom_json_str(obj, "00080020"),
                    modality: dicom_json_str(obj, "00080061"),
                    description: dicom_json_str(obj, "00081030"),
                    has_report: false,
                });
            }
            Ok(out)
        }
        PacsBackend::VueMotion { .. } | PacsBackend::TrainedDom(_) => {
            anyhow::bail!("WebView searches run through the live WebView session.")
        }
    }
}

pub fn download_pacs_study_zip(
    backend: &PacsBackend,
    cookie_header: &str,
    study_id: &str,
    study_uid: &str,
    watch_dir: &Path,
    anonymize: bool,
) -> Result<PathBuf> {
    std::fs::create_dir_all(watch_dir)?;
    let safe_id: String = if study_uid.is_empty() { study_id } else { study_uid }
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' })
        .collect();
    let suffix = if anonymize { "_anon.zip" } else { ".zip" };
    let final_zip = watch_dir.join(format!("{safe_id}{suffix}"));
    let raw_zip = watch_dir.join(format!("{safe_id}.zip.part"));

    let url = match backend {
        PacsBackend::Orthanc { base_url } => format!("{base_url}/studies/{study_id}/archive"),
        PacsBackend::DicomWeb {
            base_url,
            wado_prefix,
            ..
        } => format!("{base_url}{wado_prefix}/studies/{study_uid}?accept=application/zip"),
        PacsBackend::VueMotion { .. } | PacsBackend::TrainedDom(_) => {
            anyhow::bail!("WebView downloads run through the live WebView session.")
        }
    };

    let agent = pacs_http_agent(300);
    let mut req = agent.get(&url).header("Accept", "application/zip, application/octet-stream");
    if !cookie_header.is_empty() {
        req = req.header("Cookie", cookie_header);
    }
    let mut resp = req.call()?;
    let mut reader = resp.body_mut().as_reader().take(MAX_PACS_DOWNLOAD_BYTES + 1);
    let mut out = std::fs::File::create(&raw_zip)?;
    let mut buf = [0u8; 64 * 1024];
    let mut total_bytes: u64 = 0;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        total_bytes = total_bytes.saturating_add(n as u64);
        if total_bytes > MAX_PACS_DOWNLOAD_BYTES {
            drop(out);
            let _ = std::fs::remove_file(&raw_zip);
            anyhow::bail!("Downloaded study ZIP exceeds maximum allowed size (4 GiB).");
        }
        out.write_all(&buf[..n])?;
    }
    out.flush()?;
    drop(out);

    if anonymize {
        let anon_part = watch_dir.join(format!("{safe_id}_anon.zip.part"));
        anonymize_zip_archive(&raw_zip, &anon_part)?;
        let _ = std::fs::remove_file(&raw_zip);
        if final_zip.exists() {
            let _ = std::fs::remove_file(&final_zip);
        }
        std::fs::rename(&anon_part, &final_zip)?;
    } else {
        if final_zip.exists() {
            let _ = std::fs::remove_file(&final_zip);
        }
        std::fs::rename(&raw_zip, &final_zip)?;
    }
    Ok(final_zip)
}

fn anonymize_zip_archive(src_zip: &Path, dst_zip: &Path) -> Result<()> {
    let tmp_dir = tempfile::TempDir::new()?;

    let file = std::fs::File::open(src_zip)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let out_file = std::fs::File::create(dst_zip)?;
    let mut writer = zip::ZipWriter::new(out_file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let opts = crate::export::AnonymizeOptions::default();
    let mut uid_map = HashMap::new();
    let mut uid_counter = 0usize;
    let mut img_counter = 0usize;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        if entry.unix_mode().is_some_and(|m| (m & 0o170000) == 0o120000) {
            continue;
        }
        let Some(rel) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
            continue;
        };
        if rel
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.eq_ignore_ascii_case("DICOMDIR"))
        {
            continue;
        }
        let in_tmp = tmp_dir.path().join("in.dcm");
        let out_tmp = tmp_dir.path().join("out.dcm");
        {
            let mut f = std::fs::File::create(&in_tmp)?;
            std::io::copy(&mut entry, &mut f)?;
        }
        if crate::export::anonymize_dicom_file(
            &in_tmp,
            &out_tmp,
            &opts,
            &mut uid_map,
            &mut uid_counter,
        )
        .is_ok()
        {
            img_counter += 1;
            let neutral_name = format!("SER001/IMG{img_counter:04}.dcm");
            writer.start_file(neutral_name, options)?;
            let mut f = std::fs::File::open(&out_tmp)?;
            std::io::copy(&mut f, &mut writer)?;
        }
    }
    writer.finish()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
