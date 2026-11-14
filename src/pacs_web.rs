use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};

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
    pub saved_password: String,
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
    last_watch_check: f64,
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
            saved_password: String::new(),
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
            last_watch_check: 0.0,
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

        let (cmd_tx, cmd_rx) = mpsc::channel::<WebViewCommand>();
        if self.remember_credentials
            && !self.saved_username.trim().is_empty()
            && !self.saved_password.is_empty()
        {
            let creds = serde_json::json!({
                "user": self.saved_username.trim(),
                "pass": self.saved_password,
            })
            .to_string();
            let _ = cmd_tx.send(WebViewCommand::AutoLogin(creds));
        }
        self.cmd_tx = Some(cmd_tx);
        self.silent_auto_connect = hidden;
        if !hidden {
            self.state = PacsHookState::WaitingLogin;
            self.status_msg = Some(tr.pacs_msg_login_prompt.into());
        }

        let event_tx = self.tx.clone();
        std::thread::spawn(move || {
            spawn_webview_subprocess(url, watch_dir, hidden, cmd_rx, event_tx, ctx);
        });
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
            self.status_msg = Some(format!("{} {label} (.zip)…", tr.pacs_msg_downloading));
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
            self.status_msg = Some(format!("{} {label} (.zip)…", tr.pacs_msg_downloading));
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
        self.status_msg = Some(format!("{} {label} (.zip)…", tr.pacs_msg_downloading));
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
    pub fn poll(&mut self, now_secs: f64) -> Vec<PathBuf> {
        let tr = crate::i18n::tr(self.lang);
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                PacsEvent::Hooked {
                    backend,
                    cookie_header,
                } => {
                    self.silent_auto_connect = false;
                    self.status_msg = Some(format!("{} {}", tr.pacs_msg_hooked, backend.label()));
                    let pending = self.pending_action.take();
                    let keep_webview = pending.is_some()
                        && matches!(
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
                    if self.download_anonymized {
                        if let PacsHookState::Hooked {
                            backend: PacsBackend::TrainedDom(ref prof),
                            ..
                        } = self.state
                        {
                            if prof.anon_steps.is_empty() {
                                let tmp = p.with_extension("anon.zip.part");
                                if anonymize_zip_archive(&p, &tmp).is_ok() {
                                    let _ = std::fs::rename(&tmp, &p);
                                } else {
                                    let _ = std::fs::remove_file(&tmp);
                                }
                            }
                        }
                    }
                    self.downloading = None;
                    self.status_msg = Some(format!(
                        "{}: {}",
                        tr.pacs_msg_saved_zip,
                        p.file_name().and_then(|n| n.to_str()).unwrap_or("study.zip")
                    ));
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

        let mut new_zips = Vec::new();
        if now_secs - self.last_watch_check >= 2.0 {
            self.last_watch_check = now_secs;
            if let Some(ref dir) = self.watch_dir {
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
            }
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

pub(crate) fn origin_of(url: &str) -> String {
    if let Some(scheme_end) = url.find("://") {
        let rest = &url[scheme_end + 3..];
        let host_end = rest.find('/').unwrap_or(rest.len());
        url[..scheme_end + 3 + host_end].to_string()
    } else {
        url.trim_end_matches('/').to_string()
    }
}

fn spawn_webview_subprocess(
    login_url: String,
    watch_dir: PathBuf,
    hidden: bool,
    cmd_rx: Receiver<WebViewCommand>,
    event_tx: Sender<PacsEvent>,
    egui_ctx: egui::Context,
) {
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};

    let Ok(exe) = std::env::current_exe() else {
        let _ = event_tx.send(PacsEvent::Incompatible(
            "Could not locate SekhmetX executable for login window.".into(),
        ));
        let _ = event_tx.send(PacsEvent::WebViewClosed);
        egui_ctx.request_repaint();
        return;
    };

    crate::crash_log::log_line("INFO", &format!("Spawning PACS WebView subprocess for {login_url}"));
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
            if let Some(rest) = line.strip_prefix("HOOK_DATA\t") {
                if let Ok(v) = serde_json::from_str::<Value>(rest) {
                    let current_url = v.get("url").and_then(|x| x.as_str()).unwrap_or(&login_url).to_string();
                    let cookie_header = v.get("cookies").and_then(|x| x.as_str()).unwrap_or("").to_string();
                    let is_vue_motion = v.get("vue_motion").and_then(|x| x.as_bool()).unwrap_or(false);
                    let base_origin = origin_of(&current_url);
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
                                    let _ = tx.send(PacsEvent::Hooked { backend, cookie_header });
                                }
                                None => {
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
            } else if let Some(rest) = line.strip_prefix("PATIENT_RESULTS\t") {
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
            } else if let Some(rest) = line.strip_prefix("PATIENT_STUDIES\t") {
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
            } else if let Some(rest) = line.strip_prefix("PICKED\t") {
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
            } else if let Some(rest) = line.strip_prefix("SEARCH_RESULTS\t") {
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
            } else if let Some(err) = line.strip_prefix("SEARCH_ERR\t") {
                let _ = event_tx.send(PacsEvent::SearchResults(Err(err.to_string())));
                egui_ctx.request_repaint();
            } else if let Some(path) = line.strip_prefix("DOWNLOADED\t") {
                let _ = event_tx.send(PacsEvent::DownloadFinished(PathBuf::from(path)));
                egui_ctx.request_repaint();
            } else if let Some(err) = line.strip_prefix("DL_FAILED\t") {
                let _ = event_tx.send(PacsEvent::DownloadFailed(err.to_string()));
                egui_ctx.request_repaint();
            } else if let Some(msg) = line.strip_prefix("INCOMPATIBLE\t") {
                let _ = event_tx.send(PacsEvent::Incompatible(msg.to_string()));
                egui_ctx.request_repaint();
            }
        }
    }

    let status = child.wait();
    crate::crash_log::log_line("INFO", &format!("PACS WebView subprocess exited: {status:?}"));
    if let Ok(st) = status {
        if !st.success() {
            let _ = event_tx.send(PacsEvent::Incompatible(format!(
                "WebView login window closed unexpectedly ({st}). See sekhmetx.log for details."
            )));
        }
    }
    let _ = event_tx.send(PacsEvent::WebViewClosed);
    egui_ctx.request_repaint();
}

pub fn run_webview_subprocess(login_url: &str, watch_dir: PathBuf, hidden: bool) {
    use std::io::{BufRead, Write};
    use tao::event::{Event, WindowEvent};
    use tao::event_loop::ControlFlow;
    use tao::window::WindowBuilder;
    use wry::WebViewBuilder;

    crate::crash_log::log_line("INFO", &format!("WebView subprocess starting for {login_url}"));
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
        println!("INCOMPATIBLE\tCould not create WebView login window on this system.");
        let _ = std::io::stdout().flush();
        return;
    };

    if hidden {
        let auto_proxy = event_loop.create_proxy();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(2600));
            let _ = auto_proxy.send_event(UserEvent::HookSession);
        });
    }

    let web_data_dir = crate::crash_log::app_data_dir().join("webview");
    let _ = std::fs::create_dir_all(&web_data_dir);
    let mut web_context = wry::WebContext::new(Some(web_data_dir));

    let dl_dir = watch_dir.to_path_buf();
    let Ok(webview) = WebViewBuilder::with_web_context(&mut web_context)
        .with_url(login_url)
        .with_ipc_handler(|req| {
            let body = req.body();
            if body.starts_with("PICKED\t")
                || body.starts_with("HOOK_DATA\t")
                || body.starts_with("PATIENT_RESULTS\t")
                || body.starts_with("PATIENT_STUDIES\t")
                || body.starts_with("SEARCH_RESULTS\t")
                || body.starts_with("SEARCH_ERR\t")
                || body.starts_with("DL_FAILED\t")
            {
                println!("{body}");
                let _ = std::io::stdout().flush();
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
                    println!("DOWNLOADED\t{}", p.display());
                }
            } else {
                println!("DL_FAILED\tBrowser download from PACS portal failed.");
            }
            let _ = std::io::stdout().flush();
        })
        .build(&window)
    else {
        crate::crash_log::log_line("ERROR", "WebViewBuilder::build failed in WebView subprocess");
        println!("INCOMPATIBLE\tThis webpage or WebView runtime does not allow hooking into its virtual PACS system.");
        let _ = std::io::stdout().flush();
        return;
    };

    let fallback_url = login_url.to_string();
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
                let js = build_auto_login_js(&payload);
                let _ = webview.evaluate_script(&js);
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
            Event::UserEvent(UserEvent::HookSession) => {
                let current_url = webview.url().unwrap_or_else(|_| fallback_url.clone());
                let cookie_header = webview
                    .cookies_for_url(&current_url)
                    .map(|list| {
                        list.into_iter()
                            .map(|c| format!("{}={}", c.name(), c.value()))
                            .collect::<Vec<_>>()
                            .join("; ")
                    })
                    .unwrap_or_default();
                let payload = serde_json::json!({
                    "url": current_url,
                    "cookies": cookie_header,
                });
                let js = format!(
                    r#"(function() {{
  const base = {payload};
  const vm = !!(document.querySelector('#sptGeneralDetailsInput') && (document.querySelector('#dataTableID') || document.querySelector('#sptSearchSubmitBtn')));
  base.vue_motion = vm;
  window.ipc.postMessage('HOOK_DATA\t' + JSON.stringify(base));
}})();"#
                );
                if webview.evaluate_script(&js).is_err() {
                    println!("HOOK_DATA\t{payload}");
                    let _ = std::io::stdout().flush();
                }
            }
            _ => {}
        }
    });
}

fn http_get_json(url: &str, cookie_header: &str, accept: &str) -> Result<Value> {
    let mut req = ureq::get(url).header("Accept", accept);
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

    let mut req = ureq::get(&url).header("Accept", "application/zip, application/octet-stream");
    if !cookie_header.is_empty() {
        req = req.header("Cookie", cookie_header);
    }
    let mut resp = req.call()?;
    let mut reader = resp.body_mut().as_reader();
    let mut out = std::fs::File::create(&raw_zip)?;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
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
    let tmp_dir = std::env::temp_dir().join(format!(
        "sekhmetx-pacs-anon-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_dir_all(&tmp_dir);
    std::fs::create_dir_all(&tmp_dir)?;

    let file = std::fs::File::open(src_zip)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let out_file = std::fs::File::create(dst_zip)?;
    let mut writer = zip::ZipWriter::new(out_file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let opts = crate::export::AnonymizeOptions::default();
    let mut uid_map = HashMap::new();
    let mut uid_counter = 0usize;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let Some(rel) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
            continue;
        };
        let in_tmp = tmp_dir.join("in.dcm");
        let out_tmp = tmp_dir.join("out.dcm");
        {
            let mut f = std::fs::File::create(&in_tmp)?;
            std::io::copy(&mut entry, &mut f)?;
        }
        writer.start_file(rel.to_string_lossy().replace('\\', "/"), options)?;
        if crate::export::anonymize_dicom_file(
            &in_tmp,
            &out_tmp,
            &opts,
            &mut uid_map,
            &mut uid_counter,
        )
        .is_ok()
        {
            let mut f = std::fs::File::open(&out_tmp)?;
            std::io::copy(&mut f, &mut writer)?;
        } else {
            let mut f = std::fs::File::open(&in_tmp)?;
            std::io::copy(&mut f, &mut writer)?;
        }
    }
    writer.finish()?;
    let _ = std::fs::remove_dir_all(&tmp_dir);
    Ok(())
}

fn build_picker_js(payload_json: &str) -> String {
    format!(
        r#"(function() {{
  const cfg = {payload_json};
  if (window.__sekhmetPickerCleanup) window.__sekhmetPickerCleanup();
  let lastEl = null;
  let lastOutline = '';
  function cssEscape(s) {{
    return (window.CSS && CSS.escape) ? CSS.escape(s) : s.replace(/([^\w-])/g, '\\$1');
  }}
  function selectorFor(el, generalizeRow) {{
    if (!el || el.nodeType !== 1) return '';
    const parts = [];
    let cur = el;
    while (cur && cur.nodeType === 1 && cur.tagName.toLowerCase() !== 'html') {{
      if (cur.id && !generalizeRow) {{
        parts.unshift('#' + cssEscape(cur.id));
        break;
      }}
      let seg = cur.tagName.toLowerCase();
      if (cur.classList && cur.classList.length > 0) {{
        const cls = Array.from(cur.classList).filter(c => !c.startsWith('sekhmet')).slice(0, 2);
        if (cls.length) seg += '.' + cls.map(cssEscape).join('.');
      }}
      if (generalizeRow && cur === el) {{
        // Generalize repeating row element (no :nth-of-type on the row itself)
      }} else if (cur.parentElement) {{
        const sibs = Array.from(cur.parentElement.children).filter(c => c.tagName === cur.tagName);
        if (sibs.length > 1) {{
          seg += ':nth-of-type(' + (sibs.indexOf(cur) + 1) + ')';
        }}
      }}
      parts.unshift(seg);
      cur = cur.parentElement;
    }}
    return parts.join(' > ');
  }}
  function relativeSelector(ancestor, desc) {{
    const parts = [];
    let cur = desc;
    while (cur && cur !== ancestor && cur.nodeType === 1) {{
      let seg = cur.tagName.toLowerCase();
      if (cur.classList && cur.classList.length > 0) {{
        const cls = Array.from(cur.classList).filter(c => !c.startsWith('sekhmet')).slice(0, 2);
        if (cls.length) seg += '.' + cls.map(cssEscape).join('.');
      }}
      if (cur.parentElement) {{
        const sibs = Array.from(cur.parentElement.children).filter(c => c.tagName === cur.tagName);
        if (sibs.length > 1) seg += ':nth-of-type(' + (sibs.indexOf(cur) + 1) + ')';
      }}
      parts.unshift(seg);
      cur = cur.parentElement;
    }}
    return parts.join(' > ');
  }}
  function onMove(e) {{
    if (lastEl && lastEl !== e.target) lastEl.style.outline = lastOutline;
    lastEl = e.target;
    lastOutline = lastEl.style.outline || '';
    lastEl.style.outline = '3px solid #22c55e';
  }}
  function onClick(e) {{
    const multiStep = (cfg.step === 'download' || cfg.step === 'anon');
    if (multiStep && !(e.ctrlKey || e.metaKey)) {{
      return; // Allow normal clicks to open dropdown menus / popups!
    }}
    e.preventDefault();
    e.stopPropagation();
    let target = e.target;
    if (cfg.step === 'row') {{
      const row = target.closest('tr[id], tbody > tr, li, [role="row"], .row');
      if (row) target = row;
    }}
    const sel = selectorFor(target, false);
    const rowSel = (cfg.step === 'row') ? selectorFor(target, true) : '';
    let inRow = false;
    let relSel = '';
    if (cfg.row_sel) {{
      try {{
        const matchedRow = target.closest(cfg.row_sel);
        if (matchedRow) {{
          inRow = true;
          relSel = relativeSelector(matchedRow, target);
        }}
      }} catch (_) {{}}
    }}
    if (!multiStep) cleanup();
    window.ipc.postMessage('PICKED\t' + JSON.stringify({{
      step: cfg.step,
      selector: sel,
      row_selector: rowSel,
      in_row: inRow,
      rel_selector: relSel
    }}));
  }}
  function cleanup() {{
    if (lastEl) lastEl.style.outline = lastOutline;
    document.removeEventListener('mouseover', onMove, true);
    document.removeEventListener('click', onClick, true);
    window.__sekhmetPickerCleanup = null;
  }}
  document.addEventListener('mouseover', onMove, true);
  document.addEventListener('click', onClick, true);
  window.__sekhmetPickerCleanup = cleanup;
}})();"#
    )
}

fn build_trained_search_js(payload_json: &str) -> String {
    format!(
        r#"(function() {{
  const cfg = {payload_json};
  const p = cfg.profile;
  const input = document.querySelector(p.search_input_sel);
  if (!input) {{
    window.ipc.postMessage('SEARCH_ERR\tSearch input not found on page.');
    return;
  }}
  input.focus();
  input.value = cfg.query || '';
  input.dispatchEvent(new Event('input', {{ bubbles: true }}));
  input.dispatchEvent(new Event('change', {{ bubbles: true }}));
  input.dispatchEvent(new KeyboardEvent('keydown', {{ key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true }}));
  input.dispatchEvent(new KeyboardEvent('keyup', {{ key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true }}));
  if (input.form) {{
    const btn = input.form.querySelector('button[type="submit"], input[type="submit"], button');
    if (btn) btn.click();
    else if (typeof input.form.requestSubmit === 'function') input.form.requestSubmit();
  }}
  let tries = 0;
  const timer = setInterval(function() {{
    tries++;
    const rows = Array.from(document.querySelectorAll(p.row_list_sel)).slice(0, 50);
    if (rows.length > 0 || tries >= 20) {{
      clearInterval(timer);
      const out = rows.map((r, i) => ({{
        index: i,
        text: (r.innerText || r.textContent || '').replace(/\s+/g, ' ').trim()
      }})).filter(x => x.text.length > 0);
      window.ipc.postMessage('SEARCH_RESULTS\t' + JSON.stringify(out));
    }}
  }}, 250);
}})();"#
    )
}

fn build_trained_download_js(payload_json: &str) -> String {
    format!(
        r#"(function() {{
  const cfg = {payload_json};
  const p = cfg.profile;
  const rows = Array.from(document.querySelectorAll(p.row_list_sel));
  const row = rows[cfg.row_index];
  if (!row) {{
    window.ipc.postMessage('DL_FAILED\tSelected study row is no longer visible on page.');
    return;
  }}
  row.click();
  const useAnon = !!(cfg.anonymize && p.anon_steps && p.anon_steps.length > 0);
  const steps = useAnon ? p.anon_steps : (p.download_steps || []);
  if (steps.length === 0) {{
    window.ipc.postMessage('DL_FAILED\tNo download steps trained for this portal.');
    return;
  }}
  function runStep(idx) {{
    if (idx >= steps.length) return;
    const st = steps[idx];
    const sel = (st.in_row && st.rel_selector) ? st.rel_selector : st.selector;
    const btn = (st.in_row && st.rel_selector) ? row.querySelector(sel) : document.querySelector(sel);
    if (!btn) {{
      window.ipc.postMessage('DL_FAILED\tDownload step #' + (idx + 1) + ' button not found on page.');
      return;
    }}
    btn.click();
    if (idx + 1 < steps.length) {{
      setTimeout(() => runStep(idx + 1), 250);
    }}
  }}
  setTimeout(() => runStep(0), 150);
}})();"#
    )
}

fn build_vuemotion_search_js(payload_json: &str) -> String {
    format!(
        r#"(function() {{
  const cfg = {payload_json};
  function runSearch() {{
    const input = document.querySelector('#sptGeneralDetailsInput');
    const btn = document.querySelector('#sptSearchSubmitBtn');
    if (!input) return false;
    const prevKey = Array.from(document.querySelectorAll('#dataTableID tr[id^="ptRecord_"]'))
      .map(r => r.id + ':' + (r.innerText || '').slice(0, 40))
      .join('|');
    input.focus();
    input.value = cfg.query || '';
    input.dispatchEvent(new Event('input', {{ bubbles: true }}));
    input.dispatchEvent(new Event('change', {{ bubbles: true }}));
    if (btn) btn.click();
    else if (input.form) input.form.submit();
    let tries = 0;
    const t = setInterval(function() {{
      tries++;
      const rows = Array.from(document.querySelectorAll('#dataTableID tr[id^="ptRecord_"]')).slice(0, 50);
      const curKey = rows.map(r => r.id + ':' + (r.innerText || '').slice(0, 40)).join('|');
      const changed = curKey !== prevKey;
      if ((rows.length > 0 && (changed || tries >= 6)) || tries >= 24) {{
        clearInterval(t);
        const out = rows.map((r, i) => {{
          const link = r.querySelector('a.dtPatientLink');
          const name = (link ? (link.innerText || link.title || '') : '').replace(/\s+/g, ' ').trim();
          const cells = Array.from(r.querySelectorAll('td'))
            .map(td => (td.innerText || '').replace(/\s+/g, ' ').trim())
            .filter(s => s.length > 0 && s !== '.' && s !== '-');
          return {{ index: i, name: name || cells[0] || ('Patient #' + (i + 1)), summary: cells.join(' · ') }};
        }});
        window.ipc.postMessage('PATIENT_RESULTS\t' + JSON.stringify(out));
      }}
    }}, 250);
    return true;
  }}
  if (!runSearch()) {{
    // User is currently in the viewer tab; click back/search tab or navigate to login_url.
    const searchTab = document.querySelector('#mainTab-0, a[href*="default.aspx"]');
    if (searchTab) searchTab.click();
    else if (cfg.login_url) window.location.href = cfg.login_url;
    let w = 0;
    const wt = setInterval(function() {{
      w++;
      if (runSearch() || w >= 20) {{
        clearInterval(wt);
        if (w >= 20) window.ipc.postMessage('SEARCH_ERR\tVue Motion search bar not found.');
      }}
    }}, 300);
  }}
}})();"#
    )
}

fn build_vuemotion_open_patient_js(payload_json: &str) -> String {
    format!(
        r#"(function() {{
  const cfg = {payload_json};
  const rows = Array.from(document.querySelectorAll('#dataTableID tr[id^="ptRecord_"]'));
  const row = rows[cfg.patient_index];
  if (!row) {{
    window.ipc.postMessage('SEARCH_ERR\tPatient row no longer visible.');
    return;
  }}
  const link = row.querySelector('a.dtPatientLink') || row.querySelector('a');
  if (!link) {{
    window.ipc.postMessage('SEARCH_ERR\tPatient hyperlink not found.');
    return;
  }}
  link.click();
  let tries = 0;
  let lipClicked = false;
  const t = setInterval(function() {{
    tries++;
    const lip = document.querySelector('#mainTab-1-lipLeft, [id$="-lipLeft"]');
    const list = document.querySelector('#mainTab-1-SV_studyList');
    if (lip && (!list || list.offsetParent === null) && !lipClicked) {{
      lipClicked = true;
      lip.click();
    }}
    const cards = Array.from(document.querySelectorAll('#mainTab-1-SV_studyList div[id$="_study"]')).slice(0, 50);
    if (cards.length > 0 || tries >= 35) {{
      clearInterval(t);
      const out = cards.map((card, i) => {{
        const infoEl = card.querySelector('.scc1') || card;
        const lines = (infoEl.innerText || '').split('\n').map(s => s.trim()).filter(Boolean);
        const date = lines[0] || '';
        const modality = lines[1] || '';
        const description = lines.slice(2).join(' ') || lines.join(' ');
        const icon = card.querySelector('span[id$="_studyIcon"]');
        const has_report = !!(icon && (icon.classList.contains('report') || (icon.title || '').toLowerCase().includes('informe')));
        return {{ index: i, date, modality, description, has_report }};
      }});
      window.ipc.postMessage('PATIENT_STUDIES\t' + JSON.stringify(out));
    }}
  }}, 300);
}})();"#
    )
}

fn build_vuemotion_download_js(payload_json: &str) -> String {
    format!(
        r#"(function() {{
  const cfg = {payload_json};
  const lip = document.querySelector('#mainTab-1-lipLeft, [id$="-lipLeft"]');
  const list = document.querySelector('#mainTab-1-SV_studyList');
  if (lip && (!list || list.offsetParent === null)) {{
    lip.click();
  }}
  setTimeout(function() {{
    const cards = Array.from(document.querySelectorAll('#mainTab-1-SV_studyList div[id$="_study"]'));
    const card = cards[cfg.study_index];
    if (!card) {{
      window.ipc.postMessage('DL_FAILED\tSelected study not found in Vue Motion list.');
      return;
    }}
    card.click();
    setTimeout(function() {{
      const saveBtn = document.querySelector('#mainTab-1-SV-saveAnchorSpan, #mainTab-1-SV-saveBtn');
      if (!saveBtn) {{
        window.ipc.postMessage('DL_FAILED\tGuardar button not found in Vue Motion toolbar.');
        return;
      }}
      saveBtn.click();
      setTimeout(function() {{
        let target = null;
        if (cfg.anonymize) {{
          const links = Array.from(document.querySelectorAll('ul.subnav.subnavpos2 a.vuitItemLink, ul.subnav a.vuitItemLink'));
          target = links.find(a => /an[oó]nimo|anonim|anonymous/i.test(a.innerText || a.textContent || ''));
        }}
        if (!target) {{
          target = document.querySelector('#mainTab-1-SV-saveStudyOption a.vuitItemLink');
        }}
        if (!target) {{
          window.ipc.postMessage('DL_FAILED\tDownload DICOM ZIP option not found in menu.');
          return;
        }}
        target.click();
      }}, 250);
    }}, 500);
  }}, 250);
}})();"#
    )
}

fn build_auto_login_js(payload_json: &str) -> String {
    format!(
        r#"(function() {{
  const cfg = {payload_json};
  let attempts = 0;
  const timer = setInterval(function() {{
    attempts++;
    if (document.querySelector('#sptGeneralDetailsInput')) {{
      clearInterval(timer);
      return;
    }}
    const pw = document.querySelector('input[type="password"]');
    if (pw && pw.offsetParent !== null) {{
      clearInterval(timer);
      const inputs = Array.from(document.querySelectorAll('input[type="text"], input[type="email"], input:not([type])'))
        .filter(el => el.offsetParent !== null && !el.disabled && !el.readOnly);
      const userEl = inputs[0];
      if (userEl && cfg.user) {{
        userEl.focus();
        userEl.value = cfg.user;
        userEl.dispatchEvent(new Event('input', {{ bubbles: true }}));
        userEl.dispatchEvent(new Event('change', {{ bubbles: true }}));
      }}
      if (cfg.pass) {{
        pw.focus();
        pw.value = cfg.pass;
        pw.dispatchEvent(new Event('input', {{ bubbles: true }}));
        pw.dispatchEvent(new Event('change', {{ bubbles: true }}));
      }}
      setTimeout(function() {{
        const btn = document.querySelector('button[type="submit"], input[type="submit"], #btnLogin, .login-btn, button');
        if (btn) btn.click();
        else if (pw.form) pw.form.submit();
      }}, 150);
    }} else if (attempts >= 20) {{
      clearInterval(timer);
    }}
  }}, 250);
}})();"#
    )
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    for i in (0..s.len()).step_by(2) {
        let byte = u8::from_str_radix(&s[i..i + 2], 16).ok()?;
        out.push(byte);
    }
    Some(out)
}

#[cfg(windows)]
pub fn protect_secret(plain: &str) -> String {
    if plain.is_empty() {
        return String::new();
    }
    #[repr(C)]
    struct DataBlob {
        cb_data: u32,
        pb_data: *mut u8,
    }
    #[link(name = "crypt32")]
    unsafe extern "system" {
        fn CryptProtectData(
            p_data_in: *const DataBlob,
            sz_data_descr: *const u16,
            p_optional_entropy: *const DataBlob,
            pv_reserved: *mut core::ffi::c_void,
            p_prompt_struct: *mut core::ffi::c_void,
            dw_flags: u32,
            p_data_out: *mut DataBlob,
        ) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LocalFree(h_mem: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
    }
    let mut bytes = plain.as_bytes().to_vec();
    let in_blob = DataBlob {
        cb_data: bytes.len() as u32,
        pb_data: bytes.as_mut_ptr(),
    };
    let mut out_blob = DataBlob {
        cb_data: 0,
        pb_data: core::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptProtectData(
            &in_blob,
            core::ptr::null(),
            core::ptr::null(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            0x1, // CRYPTPROTECT_UI_FORBIDDEN
            &mut out_blob,
        )
    };
    if ok != 0 && !out_blob.pb_data.is_null() {
        let slice = unsafe {
            core::slice::from_raw_parts(out_blob.pb_data, out_blob.cb_data as usize)
        };
        let encoded = format!("dpapi:{}", hex_encode(slice));
        unsafe { LocalFree(out_blob.pb_data.cast()) };
        return encoded;
    }
    format!("hex:{}", hex_encode(plain.as_bytes()))
}

#[cfg(windows)]
pub fn unprotect_secret(encoded: &str) -> Option<String> {
    if encoded.is_empty() {
        return Some(String::new());
    }
    if let Some(hex) = encoded.strip_prefix("hex:") {
        let bytes = hex_decode(hex)?;
        return String::from_utf8(bytes).ok();
    }
    let hex = encoded.strip_prefix("dpapi:")?;
    let mut bytes = hex_decode(hex)?;
    #[repr(C)]
    struct DataBlob {
        cb_data: u32,
        pb_data: *mut u8,
    }
    #[link(name = "crypt32")]
    unsafe extern "system" {
        fn CryptUnprotectData(
            p_data_in: *const DataBlob,
            ppsz_data_descr: *mut *mut u16,
            p_optional_entropy: *const DataBlob,
            pv_reserved: *mut core::ffi::c_void,
            p_prompt_struct: *mut core::ffi::c_void,
            dw_flags: u32,
            p_data_out: *mut DataBlob,
        ) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LocalFree(h_mem: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
    }
    let in_blob = DataBlob {
        cb_data: bytes.len() as u32,
        pb_data: bytes.as_mut_ptr(),
    };
    let mut out_blob = DataBlob {
        cb_data: 0,
        pb_data: core::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptUnprotectData(
            &in_blob,
            core::ptr::null_mut(),
            core::ptr::null(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            0x1,
            &mut out_blob,
        )
    };
    if ok != 0 && !out_blob.pb_data.is_null() {
        let slice = unsafe {
            core::slice::from_raw_parts(out_blob.pb_data, out_blob.cb_data as usize)
        };
        let plain = String::from_utf8(slice.to_vec()).ok();
        unsafe { LocalFree(out_blob.pb_data.cast()) };
        return plain;
    }
    None
}

#[cfg(not(windows))]
pub fn protect_secret(plain: &str) -> String {
    if plain.is_empty() {
        String::new()
    } else {
        format!("hex:{}", hex_encode(plain.as_bytes()))
    }
}

#[cfg(not(windows))]
pub fn unprotect_secret(encoded: &str) -> Option<String> {
    if encoded.is_empty() {
        return Some(String::new());
    }
    let hex = encoded.strip_prefix("hex:")?;
    let bytes = hex_decode(hex)?;
    String::from_utf8(bytes).ok()
}
