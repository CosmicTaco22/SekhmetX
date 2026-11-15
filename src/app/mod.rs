//! The viewer window: study/series browser on the left, Horos-style toolbar at
//! the top, and multi-series comparison viewports (`1Ã—1` through `3Ã—3`) with
//! synchronized scrolling, 3D cross-reference/scout lines, CLUTs, and
//! orientation edge markers in the center.

mod modals;
mod overlays;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;

use eframe::egui::{
    self, pos2, vec2, Align2, Color32, ColorImage, FontId, Key, Pos2, Rect, Sense, Stroke,
    TextureHandle, TextureOptions, Vec2,
};

use crate::dicom_index::{self, Index, Series, SliceRef, Study};
use crate::frame::Frame;
use crate::geometry::{
    ImagePlaneCoordinates, OrientationLetters, PlaneGeometry, SlicePlaneParams,
};
use crate::gpu_view::{self, GpuInfo, ImageCallback, QuadUvMapping};
use crate::imaging::clut::{ColorLookupTable, ColourTableBook};
use crate::imaging::convolution::FACTORY_CONVOLUTIONS;
use crate::imaging::{defaults, drag, presets, Window};
use crate::loader::{Job, Loader, SliceKey};
use crate::navigation::{
    ReferenceLines, SliceSynchronization, SliceSynchronizer, SynchronizableSlice,
    SynchronizationTarget, ViewportTiling,
};
use crate::roi::{
    self, RoiBoundingBox, RoiKind, RoiPlanarGeometry, RoiPoint, RoiSize, RoiSpline,
};
use crate::volume::{add3, dot3, scale3, sub3, MprPlane3D, RigidTransform3D, SlabMode, Volume3D};

const NUMBER_KEYS: [Key; 9] = [
    Key::Num1,
    Key::Num2,
    Key::Num3,
    Key::Num4,
    Key::Num5,
    Key::Num6,
    Key::Num7,
    Key::Num8,
    Key::Num9,
];

/// Memory budget for decoded slices across open series.
const CACHE_BUDGET_BYTES: usize = 3072 * 1024 * 1024;
const MAX_VIEWPORTS: usize = 16;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Tool {
    WindowLevel,
    Scroll,
    Pan,
    Zoom,
    Rotate,
    Orbit3D,
    Length,
    Angle,
    Oval,
    Rect,
    Polygon,
    Pencil,
    Point,
}

#[derive(Clone)]
pub(crate) enum Measure {
    /// End points in unrotated image pixel coordinates.
    Length(Pos2, Pos2),
    /// Vertex in the middle, in unrotated image pixel coordinates.
    Angle(Pos2, Pos2, Pos2),
    /// Oval / Ellipse ROI defined by two opposite bounding corners in image pixel coordinates.
    Oval(Pos2, Pos2),
    /// Rectangle ROI defined by two opposite corners in image pixel coordinates.
    Rect(Pos2, Pos2),
    /// Closed Polygon ROI defined by vertices in image pixel coordinates.
    Polygon(Vec<Pos2>),
    /// Closed Freehand / Pencil ROI defined by stroke points in image pixel coordinates.
    Pencil(Vec<Pos2>),
    /// Single-point 2D Probe ROI in image pixel coordinates.
    Point(Pos2),
}

impl Measure {
    pub(crate) fn to_persisted(&self) -> crate::db::PersistedMeasure {
        match self {
            Self::Length(a, b) => crate::db::PersistedMeasure::Length([a.x, a.y], [b.x, b.y]),
            Self::Angle(a, v, b) => {
                crate::db::PersistedMeasure::Angle([a.x, a.y], [v.x, v.y], [b.x, b.y])
            }
            Self::Oval(a, b) => crate::db::PersistedMeasure::Oval([a.x, a.y], [b.x, b.y]),
            Self::Rect(a, b) => crate::db::PersistedMeasure::Rect([a.x, a.y], [b.x, b.y]),
            Self::Polygon(pts) => {
                crate::db::PersistedMeasure::Polygon(pts.iter().map(|p| [p.x, p.y]).collect())
            }
            Self::Pencil(pts) => {
                crate::db::PersistedMeasure::Pencil(pts.iter().map(|p| [p.x, p.y]).collect())
            }
            Self::Point(p) => crate::db::PersistedMeasure::Point([p.x, p.y]),
        }
    }

    pub(crate) fn from_persisted(pm: crate::db::PersistedMeasure) -> Self {
        match pm {
            crate::db::PersistedMeasure::Length(a, b) => {
                Self::Length(pos2(a[0], a[1]), pos2(b[0], b[1]))
            }
            crate::db::PersistedMeasure::Angle(a, v, b) => {
                Self::Angle(pos2(a[0], a[1]), pos2(v[0], v[1]), pos2(b[0], b[1]))
            }
            crate::db::PersistedMeasure::Oval(a, b) => {
                Self::Oval(pos2(a[0], a[1]), pos2(b[0], b[1]))
            }
            crate::db::PersistedMeasure::Rect(a, b) => {
                Self::Rect(pos2(a[0], a[1]), pos2(b[0], b[1]))
            }
            crate::db::PersistedMeasure::Polygon(pts) => {
                Self::Polygon(pts.into_iter().map(|p| pos2(p[0], p[1])).collect())
            }
            crate::db::PersistedMeasure::Pencil(pts) => {
                Self::Pencil(pts.into_iter().map(|p| pos2(p[0], p[1])).collect())
            }
            crate::db::PersistedMeasure::Point(p) => Self::Point(pos2(p[0], p[1])),
        }
    }
}

struct Scan {
    rx: Receiver<Index>,
    done: Arc<AtomicUsize>,
    total: Arc<AtomicUsize>,
}

#[derive(PartialEq, Clone)]
struct RenderKey {
    key: SliceKey,
    center: u64,
    width: u64,
    invert: bool,
    clut_name: Option<String>,
    convolution_idx: Option<usize>,
}

/// State of a single 2D comparison viewport cell.
pub(crate) struct ViewportCell {
    pub(crate) selected: Option<(usize, usize)>,
    pub(crate) slice: usize,
    /// User-chosen window (`None` = series default).
    pub(crate) window: Option<Window>,
    /// Default window captured from the series' reference (middle) slice.
    pub(crate) series_default: Option<Window>,
    pub(crate) clut_name: String,
    /// Optional 2D spatial convolution filter index into `FACTORY_CONVOLUTIONS`.
    pub(crate) convolution_idx: Option<usize>,
    pub(crate) conv_cached: Option<(SliceKey, usize, Arc<Frame>, SliceKey)>,
    pub(crate) invert: bool,
    pub(crate) zoom: f32,
    pub(crate) pan: Vec2,
    pub(crate) rotation: f32,
    pub(crate) x_flipped: bool,
    pub(crate) y_flipped: bool,
    pub(crate) wheel_accum: f32,
    pub(crate) mpr_enabled: bool,
    pub(crate) mpr_auto_applied: bool,
    pub(crate) mpr_plane: Option<MprPlane3D>,
    pub(crate) mpr_cached: Option<(MprPlane3D, Arc<Frame>, SliceKey)>,
    mpr_seq: usize,
    texture: Option<TextureHandle>,
    rendered: Option<RenderKey>,
    last_gpu: Option<(Arc<Frame>, SliceKey)>,
}

impl ViewportCell {
    fn new(_id: usize) -> Self {
        Self {
            selected: None,
            slice: 0,
            window: None,
            series_default: None,
            clut_name: ColourTableBook::NONE_TITLE.to_string(),
            convolution_idx: None,
            conv_cached: None,
            invert: false,
            zoom: 1.0,
            pan: Vec2::ZERO,
            rotation: 0.0,
            x_flipped: false,
            y_flipped: false,
            wheel_accum: 0.0,
            mpr_enabled: false,
            mpr_auto_applied: false,
            mpr_plane: None,
            mpr_cached: None,
            mpr_seq: 0,
            texture: None,
            rendered: None,
            last_gpu: None,
        }
    }

    pub(crate) fn reset_view(&mut self) {
        self.zoom = 1.0;
        self.pan = Vec2::ZERO;
        self.rotation = 0.0;
        self.x_flipped = false;
        self.y_flipped = false;
    }

    fn reset_series_state(&mut self, sel: Option<(usize, usize)>) {
        self.selected = sel;
        self.slice = 0;
        self.window = None;
        self.series_default = None;
        self.clut_name = ColourTableBook::NONE_TITLE.to_string();
        self.convolution_idx = None;
        self.conv_cached = None;
        self.invert = false;
        self.reset_view();
        self.mpr_enabled = false;
        self.mpr_auto_applied = false;
        self.mpr_plane = None;
        self.mpr_cached = None;
        self.rendered = None;
        self.last_gpu = None;
        self.wheel_accum = 0.0;
    }

    fn active_clut_name(&self) -> Option<&str> {
        if ColourTableBook::is_grey_levels_name(&self.clut_name) {
            None
        } else {
            Some(self.clut_name.as_str())
        }
    }

    pub(crate) fn reset_clut(&mut self) {
        self.clut_name = ColourTableBook::NONE_TITLE.to_string();
        self.convolution_idx = None;
        self.conv_cached = None;
    }

    fn effective_invert(&self) -> bool {
        self.invert
    }
}

pub type SeriesPair = ((usize, usize), (usize, usize));
type LeaderSyncInfo = (
    (usize, usize),
    [f32; 9],
    SynchronizableSlice,
    SynchronizableSlice,
    Option<[f32; 3]>,
    Option<String>,
);

pub(crate) struct ViewerApp {
    index: Option<Index>,
    scan: Option<Scan>,
    scan_gen: u64,
    message: Option<String>,

    pub(crate) tiling: ViewportTiling,
    pub(crate) viewports: Vec<ViewportCell>,
    pub(crate) active_viewport: usize,
    pub(crate) sync_mode: SliceSynchronization,
    pub(crate) show_reference_lines: bool,
    pub(crate) show_annotations: bool,

    loader: Loader,
    cache: HashMap<SliceKey, Arc<Frame>>,
    failed: HashMap<SliceKey, String>,
    keep_radius: usize,
    volumes: HashMap<(usize, usize), Arc<Volume3D>>,
    volume_errors: HashMap<(usize, usize), crate::volume::VolumeError>,
    registrations: HashMap<SeriesPair, RigidTransform3D>,
    registrations_in_flight: std::collections::HashSet<SeriesPair>,
    reg_tx: std::sync::mpsc::Sender<(SeriesPair, RigidTransform3D)>,
    reg_rx: Receiver<(SeriesPair, RigidTransform3D)>,
    thumbnails: HashMap<(u64, usize, usize), TextureHandle>,
    thumbnails_queued: std::collections::HashSet<(u64, usize, usize)>,
    browser_search: String,
    browser_modality: String,

    pub(crate) presets: presets::Catalog,
    /// A window/level drag in progress: `(viewport_id, start_pos, drag_state)`.
    wl_drag: Option<(usize, Pos2, drag::Drag)>,
    /// A slice-scroll drag in progress: `(viewport_id, start_y, start_slice)`.
    scroll_drag: Option<(usize, f32, usize)>,
    /// An in-plane rotation drag in progress: `(viewport_id, start_angle_deg, initial_rotation_deg)`.
    rot_drag: Option<(usize, f32, f32)>,
    /// A 3D MPR orbit drag in progress: `(viewport_id, last_pos)`.
    orbit_drag: Option<(usize, Pos2)>,
    /// Visual & 3D pivot anchor for `Tool::Orbit3D`: `(viewport_id, anchor_screen_pos, anchor_lps)`.
    orbit_anchor: Option<(usize, Pos2, [f64; 3])>,
    gpu: Option<GpuInfo>,
    pub(crate) tool: Tool,

    pub(crate) measures: HashMap<(PathBuf, u32), Vec<Measure>>,
    pub(crate) drawing: Vec<Pos2>,
    pub(crate) angle_pending: Option<(Pos2, Pos2)>,
    pub(crate) hover_px: Option<Pos2>,
    pub(crate) tiling_hover: Option<(usize, usize)>,

    pub(crate) playing: bool,
    pub(crate) fps: f32,
    last_tick: f64,
    cursor_info: Option<String>,

    pub(crate) show_browser: bool,
    pub(crate) ortho_mpr_active: bool,
    pub(crate) ortho_crosshair_lps: Option<[f64; 3]>,
    ortho_saved_tiling: Option<ViewportTiling>,
    crosshair_drag: Option<usize>,
    forced_mpr_series: std::collections::HashSet<(usize, usize)>,
    pending_force_mpr: Option<PendingForceMpr>,
    pub(crate) pending_mpr_activate: Option<(usize, (usize, usize), bool)>,
    pub(crate) update_status: crate::updater::UpdateStatus,
    update_rx: Option<Receiver<anyhow::Result<Option<crate::updater::ReleaseInfo>>>>,
    install_rx: Option<Receiver<anyhow::Result<String>>>,
    pub(crate) show_update_modal: bool,
    pub(crate) db: Option<crate::db::Database>,
    pub(crate) lang: crate::i18n::Lang,
    pub(crate) blend_siblings: bool,
    pub(crate) gpu_bilinear: bool,
    check_updates_on_startup: bool,
    crash_report_opt_in: bool,
    pub(crate) copy_policy: crate::db::CopyToLibraryPolicy,
    pending_copy_prompt: bool,
    pub(crate) pacs_web: crate::pacs_web::PacsWebController,
    pub(crate) show_pacs_modal: bool,
    pub(crate) show_pacs_finder: bool,
    pub(crate) show_settings_modal: bool,
    study_storage_zip: bool,
    show_add_portal_note: bool,
    pub(crate) previous_crash_pending: bool,
    show_crash_modal: bool,
    show_first_run_notice: bool,
    confirm_delete_local_data: bool,
    patient_links: HashMap<String, String>,
    linking_source: Option<(String, Vec<String>)>,
}

#[derive(Clone, Copy)]
struct PendingForceMpr {
    vp_idx: usize,
    key: (usize, usize),
    ortho: bool,
}

impl ViewerApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Vec<PathBuf>) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = Color32::from_rgb(24, 26, 30);
        visuals.window_fill = Color32::from_rgb(28, 31, 36);
        visuals.widgets.inactive.bg_fill = Color32::from_rgb(42, 46, 54);
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(56, 62, 74);
        visuals.selection.bg_fill = Color32::from_rgb(52, 130, 225);
        cc.egui_ctx.set_visuals(visuals);
        let ctx = cc.egui_ctx.clone();
        dicom_index::cleanup_stale_zip_caches();
        let viewports = (0..MAX_VIEWPORTS).map(ViewportCell::new).collect();
        let db = crate::db::Database::open_default().ok();
        let (reg_tx, reg_rx) = std::sync::mpsc::channel();
        let settings = db.as_ref().map(crate::settings::AppSettings::new);
        let saved_lang = settings
            .as_ref()
            .map(|s| s.language())
            .unwrap_or(crate::i18n::Lang::En);
        let saved_bilinear = db
            .as_ref()
            .and_then(|d| d.get_setting("gpu_bilinear"))
            .map(|s| s != "0")
            .unwrap_or(true);
        let saved_startup_updates = settings
            .as_ref()
            .map(|s| s.check_updates_on_startup())
            .unwrap_or(true);
        let saved_crash_opt_in = settings
            .as_ref()
            .map(|s| s.crash_report_opt_in())
            .unwrap_or(true);
        let saved_copy_policy = settings
            .as_ref()
            .map(|s| s.copy_to_library_policy())
            .unwrap_or(crate::db::CopyToLibraryPolicy::Ask);
        let saved_pacs_inst = settings
            .as_ref()
            .map(|s| s.pacs_institution_name())
            .unwrap_or_default();
        let saved_pacs_url = settings
            .as_ref()
            .map(|s| s.pacs_login_url())
            .unwrap_or_default();
        let saved_custom_studies = settings
            .as_ref()
            .and_then(|s| s.custom_studies_dir());
        crate::db::set_custom_studies_dir(saved_custom_studies);
        let saved_storage_zip = settings
            .as_ref()
            .map(|s| s.study_storage_zip())
            .unwrap_or(false);
        let saved_pacs_dir = settings
            .as_ref()
            .and_then(|s| s.pacs_watch_dir());
        let mut pacs_web = crate::pacs_web::PacsWebController::new(
            saved_pacs_inst,
            saved_pacs_url,
            saved_pacs_dir,
        );
        if let Some(ref d) = db {
            let s = crate::settings::AppSettings::new(d);
            pacs_web.remember_credentials = s.pacs_remember_creds();
            if let Ok(origin) = crate::pacs_web::parse_https_origin(&pacs_web.login_url) {
                crate::pacs_web::migrate_legacy_secret_if_any(d, &origin);
                if pacs_web.remember_credentials {
                    pacs_web.saved_username = s.pacs_cred_user();
                    if let Ok(pw) = crate::pacs_web::load_pacs_password(&origin) {
                        pacs_web.saved_password = pw;
                    }
                }
            } else if pacs_web.remember_credentials {
                pacs_web.saved_username = s.pacs_cred_user();
            }
        }
        let first_run_ack = settings
            .as_ref()
            .map(|s| s.first_run_notice_ack())
            .unwrap_or(false);
        let saved_patient_links: HashMap<String, String> = settings
            .as_ref()
            .map(|s| s.patient_links())
            .unwrap_or_default();
        let mut app = Self {
            index: None,
            scan: None,
            scan_gen: 0,
            message: None,
            tiling: ViewportTiling::default(),
            viewports,
            active_viewport: 0,
            sync_mode: SliceSynchronization::Absolute,
            show_reference_lines: true,
            show_annotations: true,
            loader: Loader::new(move || ctx.request_repaint()),
            cache: HashMap::new(),
            failed: HashMap::new(),
            keep_radius: 600,
            volumes: HashMap::new(),
            volume_errors: HashMap::new(),
            registrations: HashMap::new(),
            registrations_in_flight: std::collections::HashSet::new(),
            reg_tx,
            reg_rx,
            thumbnails: HashMap::new(),
            thumbnails_queued: std::collections::HashSet::new(),
            browser_search: String::new(),
            browser_modality: "ALL".to_string(),
            presets: presets::Catalog::factory(),
            wl_drag: None,
            scroll_drag: None,
            rot_drag: None,
            orbit_drag: None,
            orbit_anchor: None,
            gpu: if std::env::var_os("RDICOM_CPU").is_some() {
                None
            } else {
                gpu_view::install(cc)
            },
            tool: Tool::Scroll,
            measures: HashMap::new(),
            drawing: Vec::new(),
            angle_pending: None,
            hover_px: None,
            tiling_hover: None,
            playing: false,
            fps: 15.0,
            last_tick: 0.0,
            cursor_info: None,
            show_browser: true,
            ortho_mpr_active: false,
            ortho_crosshair_lps: None,
            ortho_saved_tiling: None,
            crosshair_drag: None,
            forced_mpr_series: std::collections::HashSet::new(),
            pending_force_mpr: None,
            pending_mpr_activate: None,
            update_status: crate::updater::UpdateStatus::Idle,
            update_rx: None,
            install_rx: None,
            show_update_modal: false,
            db,
            lang: saved_lang,
            blend_siblings: false,
            gpu_bilinear: saved_bilinear,
            check_updates_on_startup: saved_startup_updates,
            crash_report_opt_in: saved_crash_opt_in,
            copy_policy: saved_copy_policy,
            pending_copy_prompt: false,
            pacs_web,
            show_pacs_modal: false,
            show_pacs_finder: false,
            show_settings_modal: false,
            study_storage_zip: saved_storage_zip,
            show_add_portal_note: false,
            previous_crash_pending: false,
            show_crash_modal: false,
            show_first_run_notice: !first_run_ack,
            confirm_delete_local_data: false,
            patient_links: saved_patient_links,
            linking_source: None,
        };
        if let Some(db) = &app.db
            && let Ok(saved_rois) = db.load_all_rois() {
                for (k, list) in saved_rois {
                    let converted: Vec<Measure> = list.into_iter().map(Measure::from_persisted).collect();
                    if !converted.is_empty() {
                        app.measures.insert(k, converted);
                    }
                }
            }
        app.pacs_web.lang = app.lang;
        if app.db.as_ref().and_then(|d| d.get_setting("pacs_auto_connect")).as_deref() == Some("1") {
            app.pacs_web.try_auto_reconnect();
        }
        if app.check_updates_on_startup {
            app.start_update_check(false, &cc.egui_ctx);
        }
        if !initial.is_empty() {
            app.start_scan(initial);
        } else if let Some(db) = &app.db
            && let Ok(saved) = db.load_index()
                && !saved.studies.is_empty() {
                    app.index = Some(saved);
                    app.assign_series_to_viewport(0, Some((0, 0)));
                    app.populate_unassigned_viewports();
                }
        app
    }

    // ---------------------------------------------------------------- state

    pub(crate) fn start_scan(&mut self, paths: Vec<PathBuf>) {
        let done = Arc::new(AtomicUsize::new(0));
        let total = Arc::new(AtomicUsize::new(0));
        let (tx, rx) = channel();
        let (d, t) = (done.clone(), total.clone());
        std::thread::spawn(move || {
            let index = dicom_index::scan(&paths, &d, &t);
            let _ = tx.send(index);
        });
        self.message = None;
        self.scan = Some(Scan { rx, done, total });
    }

    fn study_at(&self, study_idx: usize) -> Option<&Study> {
        self.index.as_ref()?.studies.get(study_idx)
    }

    fn series_at(&self, sel: (usize, usize)) -> Option<&Series> {
        self.index.as_ref()?.studies.get(sel.0)?.series.get(sel.1)
    }

    pub(crate) fn active_cell(&self) -> &ViewportCell {
        &self.viewports[self.active_viewport]
    }

    pub(crate) fn active_cell_mut(&mut self) -> &mut ViewportCell {
        &mut self.viewports[self.active_viewport]
    }

    fn viewport_series(&self, vp_idx: usize) -> Option<&Series> {
        let sel = self.viewports.get(vp_idx)?.selected?;
        self.series_at(sel)
    }

    fn viewport_study(&self, vp_idx: usize) -> Option<&Study> {
        let (st, _) = self.viewports.get(vp_idx)?.selected?;
        self.study_at(st)
    }

    pub(crate) fn viewport_slice_count(&self, vp_idx: usize) -> usize {
        if let Some(vp) = self.viewports.get(vp_idx)
            && vp.mpr_enabled
                && let (Some(vol), Some(plane)) = (self.viewport_volume(vp_idx), &vp.mpr_plane) {
                    if dot3(plane.normal, vol.slice_dir).abs() > 0.99 && vol.depth > 0 {
                        return vol.depth;
                    }
                    let half = vol.projected_half_span_mm(plane.normal);
                    let step = vol.spacing[0]
                        .min(vol.spacing[1])
                        .min(vol.spacing[2])
                        .clamp(0.5, 2.0);
                    return ((2.0 * half / step).round() as usize + 1).clamp(15, 2048);
                }
        self.viewport_series(vp_idx).map_or(0, |s| s.slices.len())
    }

    pub(crate) fn disable_viewport_mpr(&mut self, vp_idx: usize) {
        if let (Some(vol), Some(plane)) = (
            self.viewport_volume(vp_idx).cloned(),
            self.viewports[vp_idx].mpr_plane.clone(),
        )
            && vol.depth > 0 {
                let vox = vol.lps_to_voxel(plane.slice_center_lps());
                let mapped = (vox[2].round() as isize).clamp(0, vol.depth as isize - 1) as usize;
                self.viewports[vp_idx].slice = mapped;
            }
        self.viewports[vp_idx].mpr_enabled = false;
        self.viewports[vp_idx].mpr_plane = None;
        self.viewports[vp_idx].mpr_cached = None;
    }

    fn should_sync_viewports(&self) -> bool {
        self.ortho_mpr_active
            || (self.tiling.cell_count() > 1 && self.sync_mode != SliceSynchronization::Off)
    }

    fn sync_zoom_from(&mut self, leader_idx: usize, factor: f32) {
        if !self.should_sync_viewports() {
            return;
        }
        let count = self.tiling.cell_count();
        for (i, vp) in self.viewports.iter_mut().enumerate().take(count) {
            if i != leader_idx && vp.selected.is_some() {
                vp.zoom = (vp.zoom * factor).clamp(0.05, 50.0);
            }
        }
    }

    fn sync_pan_from(&mut self, leader_idx: usize, delta: Vec2) {
        if !self.should_sync_viewports() {
            return;
        }
        let count = self.tiling.cell_count();
        for (i, vp) in self.viewports.iter_mut().enumerate().take(count) {
            if i != leader_idx && vp.selected.is_some() {
                vp.pan += delta;
            }
        }
    }

    fn sync_window_delta(&mut self, leader_idx: usize, d_level: f32, d_width: f32) {
        if !self.should_sync_viewports() {
            return;
        }
        let count = self.tiling.cell_count();
        for i in 0..count {
            if i == leader_idx || self.viewports[i].selected.is_none() {
                continue;
            }
            let base = if let Some(w) = self.viewports[i].window {
                Some(w)
            } else {
                self.viewport_frame(i).map(|f| self.effective_window(i, f))
            };
            if let Some(w) = base {
                self.viewports[i].window = Some(Window {
                    level: w.level + d_level,
                    width: (w.width + d_width).max(1.0),
                });
            }
        }
    }

    pub(crate) fn viewport_volume(&self, vp_idx: usize) -> Option<&Arc<Volume3D>> {
        let sel = self.viewports.get(vp_idx)?.selected?;
        self.volumes.get(&sel)
    }

    pub(crate) fn viewport_sibling_volumes(&self, vp_idx: usize) -> Vec<Arc<Volume3D>> {
        let Some((st_idx, se_idx)) = self.viewports.get(vp_idx).and_then(|v| v.selected) else {
            return Vec::new();
        };
        let Some(study) = self.study_at(st_idx) else {
            return Vec::new();
        };
        let Some(gid) = study.series.get(se_idx).and_then(|s| s.mpr_group_id) else {
            return Vec::new();
        };
        let Some(group) = study.mpr_groups.get(gid) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for &other_se in &group.series_indices {
            if other_se != se_idx
                && let Some(v) = self.volumes.get(&(st_idx, other_se)) {
                    out.push(v.clone());
                }
        }
        out
    }

    fn viewport_slice_key(&self, vp_idx: usize, slice: usize) -> Option<SliceKey> {
        let (study, series) = self.viewports.get(vp_idx)?.selected?;
        Some(SliceKey {
            scan_gen: self.scan_gen,
            study,
            series,
            slice,
        })
    }

    pub(crate) fn viewport_frame(&self, vp_idx: usize) -> Option<&Arc<Frame>> {
        let vp = self.viewports.get(vp_idx)?;
        if vp.mpr_enabled
            && let Some((_, ref f, _)) = vp.mpr_cached {
                return Some(f);
            }
        let slice = vp.slice;
        let key = self.viewport_slice_key(vp_idx, slice)?;
        self.cache.get(&key)
    }

    fn ensure_mpr_frame(&mut self, vp_idx: usize) -> Option<(Arc<Frame>, SliceKey)> {
        if !self.viewports.get(vp_idx)?.mpr_enabled {
            return None;
        }
        let vol = self.viewport_volume(vp_idx)?.clone();
        if self.viewports[vp_idx].mpr_plane.is_none() {
            let mut p = MprPlane3D::from_volume_native(&vol);
            let cur_s = self.viewports[vp_idx].slice.min(vol.depth.saturating_sub(1));
            let z_lps = vol.voxel_to_lps([
                vol.width as f64 * 0.5,
                vol.height as f64 * 0.5,
                cur_s as f64,
            ]);
            p.offset_mm = dot3(sub3(z_lps, p.center_lps), p.normal);
            self.viewports[vp_idx].mpr_plane = Some(p);
        }
        let plane = self.viewports[vp_idx].mpr_plane.clone()?;
        if let Some((ref cached_p, ref cached_f, cached_k)) = self.viewports[vp_idx].mpr_cached
            && *cached_p == plane {
                return Some((cached_f.clone(), cached_k));
            }
        let siblings = if self.blend_siblings {
            self.viewport_sibling_volumes(vp_idx)
        } else {
            Vec::new()
        };
        let frame = vol.reslice_plane(&siblings, &plane);
        let vp = &mut self.viewports[vp_idx];
        vp.mpr_seq = vp.mpr_seq.wrapping_add(1);
        let key = SliceKey {
            scan_gen: self.scan_gen | (1u64 << 62),
            study: vp_idx,
            series: 0,
            slice: vp.mpr_seq,
        };
        vp.mpr_cached = Some((plane, frame.clone(), key));
        Some((frame, key))
    }

    pub(crate) fn effective_mpr_plane(&self, vp_idx: usize) -> Option<MprPlane3D> {
        let vp = self.viewports.get(vp_idx)?;
        if vp.mpr_enabled
            && let Some(ref p) = vp.mpr_plane {
                return Some(p.clone());
            }
        let vol = self.viewport_volume(vp_idx)?;
        let mut p = MprPlane3D::from_volume_native(vol);
        let cur_s = vp.slice.min(vol.depth.saturating_sub(1));
        let z_lps = vol.voxel_to_lps([
            vol.width as f64 * 0.5,
            vol.height as f64 * 0.5,
            cur_s as f64,
        ]);
        p.offset_mm = dot3(sub3(z_lps, p.center_lps), p.normal);
        Some(p)
    }

    fn sync_slice_index_from_mpr_plane(&mut self, vp_idx: usize) {
        let Some(vol) = self.viewport_volume(vp_idx).cloned() else {
            return;
        };
        let Some(plane) = self.viewports[vp_idx].mpr_plane.clone() else {
            return;
        };
        let count = self.viewport_slice_count(vp_idx);
        if count <= 1 {
            return;
        }
        let half = vol.projected_half_span_mm(plane.normal).max(1.0);
        let t = ((plane.offset_mm / half) * 0.5 + 0.5).clamp(0.0, 1.0);
        self.viewports[vp_idx].slice = (t * (count - 1) as f64).round() as usize;
    }

    pub(crate) fn set_viewport_mpr_plane(
        &mut self,
        vp_idx: usize,
        plane: MprPlane3D,
        synchronize_others: bool,
    ) {
        self.viewports[vp_idx].mpr_enabled = true;
        self.viewports[vp_idx].mpr_plane = Some(plane);
        self.sync_slice_index_from_mpr_plane(vp_idx);
        if synchronize_others && self.sync_mode != SliceSynchronization::Off {
            self.synchronize_mpr_followers(vp_idx);
        }
    }

    fn synchronize_mpr_followers(&mut self, leader_idx: usize) -> bool {
        if self.sync_mode == SliceSynchronization::Off {
            return false;
        }
        let cell_count = self.tiling.cell_count();
        if cell_count <= 1 {
            return false;
        }
        let Some(leader_sel) = self.viewports[leader_idx].selected else {
            return false;
        };
        let Some(leader_plane) = self.effective_mpr_plane(leader_idx) else {
            return false;
        };

        let mut synced_any = false;
        let mut updates: Vec<(usize, MprPlane3D)> = Vec::new();
        let mut slice_2d_updates: Vec<(usize, usize)> = Vec::new();
        let leader_mpr = self.viewports[leader_idx].mpr_enabled;
        for other_idx in 0..cell_count {
            if other_idx == leader_idx {
                continue;
            }
            let Some(other_sel) = self.viewports[other_idx].selected else {
                continue;
            };
            let other_mpr = self.viewports[other_idx].mpr_enabled;
            if other_sel == leader_sel {
                if leader_mpr || other_mpr {
                    updates.push((other_idx, leader_plane.clone()));
                }
                continue;
            }
            if let Some(reg) = self.registrations.get(&(leader_sel, other_sel)) {
                let target_plane = if reg.is_reliable() {
                    reg.transform_plane(&leader_plane)
                } else {
                    leader_plane.clone()
                };
                if leader_mpr || other_mpr {
                    updates.push((other_idx, target_plane));
                } else if let Some(vol) = self.volumes.get(&other_sel)
                    && vol.depth > 0 {
                        let vox = vol.lps_to_voxel(target_plane.slice_center_lps());
                        let mapped = (vox[2].round() as isize).clamp(0, vol.depth as isize - 1) as usize;
                        slice_2d_updates.push((other_idx, mapped));
                    }
            }
        }

        for (other_idx, target_plane) in updates {
            self.viewports[other_idx].mpr_enabled = true;
            self.viewports[other_idx].mpr_plane = Some(target_plane);
            self.sync_slice_index_from_mpr_plane(other_idx);
            synced_any = true;
        }
        for (other_idx, mapped_slice) in slice_2d_updates {
            self.viewports[other_idx].slice = mapped_slice;
            synced_any = true;
        }
        synced_any
    }

    /// Manually locks the current 3D plane & slice offset of all visible follower viewports
    /// to the active leader viewport (`âš“ Lock 3D Sync`).
    pub(crate) fn lock_current_3d_alignment(&mut self) {
        let leader_idx = self.active_viewport;
        let Some(leader_sel) = self.viewports[leader_idx].selected else {
            return;
        };
        let Some(leader_plane) = self.effective_mpr_plane(leader_idx) else {
            return;
        };
        let leader_slice_pt = leader_plane.slice_center_lps();
        let cell_count = self.tiling.cell_count();

        let mut new_regs = Vec::new();
        for other_idx in 0..cell_count {
            if other_idx == leader_idx {
                continue;
            }
            let Some(other_sel) = self.viewports[other_idx].selected else {
                continue;
            };
            if other_sel == leader_sel {
                continue;
            }
            let Some(other_plane) = self.effective_mpr_plane(other_idx) else {
                continue;
            };
            let other_slice_pt = other_plane.slice_center_lps();
            // Build the orthogonal rotation mapping `leader_plane` axes `(u, v, n)` to `other_plane` axes `(u, v, n)`.
            let u_a = leader_plane.u_axis;
            let v_a = leader_plane.v_axis;
            let n_a = leader_plane.normal;
            let u_b = other_plane.u_axis;
            let v_b = other_plane.v_axis;
            let n_b = other_plane.normal;
            // R = [u_b | v_b | n_b] * [u_a | v_a | n_a]^T
            let mut rot = [[0.0f64; 3]; 3];
            for r in 0..3 {
                for c in 0..3 {
                    rot[r][c] = u_b[r] * u_a[c] + v_b[r] * v_a[c] + n_b[r] * n_a[c];
                }
            }
            let reg_fwd = RigidTransform3D {
                pivot_source_lps: leader_slice_pt,
                pivot_target_lps: other_slice_pt,
                rotation: rot,
                correlation: 1.0,
            };
            let rot_inv = [
                [rot[0][0], rot[1][0], rot[2][0]],
                [rot[0][1], rot[1][1], rot[2][1]],
                [rot[0][2], rot[1][2], rot[2][2]],
            ];
            let reg_rev = RigidTransform3D {
                pivot_source_lps: other_slice_pt,
                pivot_target_lps: leader_slice_pt,
                rotation: rot_inv,
                correlation: 1.0,
            };
            new_regs.push(((leader_sel, other_sel), reg_fwd));
            new_regs.push(((other_sel, leader_sel), reg_rev));
        }
        for (pair, reg) in new_regs {
            self.registrations.insert(pair, reg);
        }
        self.message = Some("Locked 3D slice & angle alignment across viewports.".into());
    }

    fn assign_series_to_viewport(&mut self, vp_idx: usize, sel: Option<(usize, usize)>) {
        if vp_idx >= self.viewports.len() {
            return;
        }
        if self.viewports[vp_idx].selected == sel {
            return;
        }
        self.viewports[vp_idx].reset_series_state(sel);
        self.wl_drag = None;
        self.scroll_drag = None;
        self.rot_drag = None;
        self.drawing.clear();
        self.angle_pending = None;

        if let Some((study, series)) = sel {
            // Queue the middle slice urgently so the series default window is ready immediately.
            if let Some(se) = self.series_at((study, series))
                && let Some(idx) = defaults::reference_index(se.slices.len()) {
                    let s = &se.slices[idx];
                    self.loader.request(Job {
                        key: SliceKey {
                            scan_gen: self.scan_gen,
                            study,
                            series,
                            slice: idx,
                        },
                        path: s.path.clone(),
                        frame: s.frame,
                        urgent: true,
                        distance: 0,
                    });
                }
        }
    }

    /// Sets the multi-series comparison tiling layout and auto-populates newly exposed
    /// viewports with consecutive series from the active study.
    /// Updates the shared 3D Orthogonal MPR crosshair position (LPS mm) and reslices
    /// all visible Axial / Coronal / Sagittal panes to pass through `pt_lps`.
    fn set_ortho_crosshair_lps(&mut self, pt_lps: [f64; 3]) {
        self.ortho_crosshair_lps = Some(pt_lps);
        let count = self.tiling.cell_count().min(3);
        for vp_idx in 0..count {
            if !self.viewports[vp_idx].mpr_enabled {
                continue;
            }
            if let Some(mut plane) = self.viewports[vp_idx].mpr_plane.clone() {
                plane.offset_mm = dot3(sub3(pt_lps, plane.center_lps), plane.normal);
                self.viewports[vp_idx].mpr_plane = Some(plane);
                self.sync_slice_index_from_mpr_plane(vp_idx);
            }
        }
    }

    /// Returns `(study_idx, series_idx)` for a native `plane` sibling inside `sel`'s MPR group, if present.
    pub(crate) fn mpr_group_series_for_plane(
        &self,
        sel: (usize, usize),
        plane: crate::dicom_index::DominantPlane,
    ) -> Option<(usize, usize)> {
        let (st_idx, se_idx) = sel;
        let study = self.study_at(st_idx)?;
        let gid = study.series.get(se_idx)?.mpr_group_id?;
        let group = study.mpr_groups.get(gid)?;
        let target_se = match plane {
            crate::dicom_index::DominantPlane::Axial => group.axial_series,
            crate::dicom_index::DominantPlane::Coronal => group.coronal_series,
            crate::dicom_index::DominantPlane::Sagittal => group.sagittal_series,
            crate::dicom_index::DominantPlane::Oblique
            | crate::dicom_index::DominantPlane::Unknown => None,
        }?;
        Some((st_idx, target_se))
    }

    /// Switches `vp_idx` to `target_plane`: if the active series's MPR group already contains a
    /// native high-definition series for `target_plane`, switches to that native series at the
    /// matching anatomical position instead of reslicing a lower-resolution axis.
    pub(crate) fn switch_viewport_to_plane(
        &mut self,
        vp_idx: usize,
        target_plane: crate::dicom_index::DominantPlane,
    ) {
        let Some(cur_sel) = self.viewports.get(vp_idx).and_then(|v| v.selected) else {
            return;
        };
        let current_lps = self
            .effective_mpr_plane(vp_idx)
            .map(|p| add3(p.center_lps, scale3(p.normal, p.offset_mm)));

        if let Some(sib_sel) = self.mpr_group_series_for_plane(cur_sel, target_plane)
            && sib_sel != cur_sel {
                let win = self.viewports[vp_idx].window;
                let clut = self.viewports[vp_idx].clut_name.clone();
                let inv = self.viewports[vp_idx].invert;
                let conv = self.viewports[vp_idx].convolution_idx;
                let mpr_enabled = self.viewports[vp_idx].mpr_enabled;

                self.assign_series_to_viewport(vp_idx, Some(sib_sel));
                {
                    let vp = &mut self.viewports[vp_idx];
                    vp.window = win;
                    vp.clut_name = clut;
                    vp.invert = inv;
                    vp.convolution_idx = conv;
                    vp.conv_cached = None;
                    vp.mpr_enabled = mpr_enabled;
                }
                if let Some(sib_vol) = self.volumes.get(&sib_sel).cloned() {
                    let mut p = MprPlane3D::from_volume_native(&sib_vol);
                    if let Some(lps) = current_lps {
                        p.offset_mm = dot3(sub3(lps, p.center_lps), p.normal);
                    }
                    self.set_viewport_mpr_plane(vp_idx, p, true);
                } else if mpr_enabled {
                    self.pending_mpr_activate = Some((vp_idx, sib_sel, false));
                }
                return;
            }

        if let Some(vol) = self.viewport_volume(vp_idx).cloned() {
            let mut plane = match target_plane {
                crate::dicom_index::DominantPlane::Axial => MprPlane3D::axial(vol.center_lps),
                crate::dicom_index::DominantPlane::Coronal => MprPlane3D::coronal(vol.center_lps),
                crate::dicom_index::DominantPlane::Sagittal => MprPlane3D::sagittal(vol.center_lps),
                crate::dicom_index::DominantPlane::Oblique
                | crate::dicom_index::DominantPlane::Unknown => MprPlane3D::from_volume_native(&vol),
            };
            if let Some(lps) = current_lps {
                plane.offset_mm = dot3(sub3(lps, plane.center_lps), plane.normal);
            }
            self.set_viewport_mpr_plane(vp_idx, plane, true);
        }
    }

    /// Toggles the classic Horos 3-pane Orthogonal MPR layout (`Axial | Coronal | Sagittal`)
    /// for the active viewport's 3D volume.
    pub(crate) fn toggle_orthogonal_mpr(&mut self) {
        if self.ortho_mpr_active {
            self.ortho_mpr_active = false;
            self.ortho_crosshair_lps = None;
            self.crosshair_drag = None;
            let restore = self.ortho_saved_tiling.take().unwrap_or_default();
            self.set_tiling(restore);
            if let Some(vp) = self.viewports.get_mut(self.active_viewport) {
                vp.mpr_enabled = false;
                vp.mpr_plane = None;
                vp.mpr_cached = None;
            }
            return;
        }

        let src_idx = self.active_viewport;
        let Some(sel) = self.viewports[src_idx].selected else {
            return;
        };
        let Some(vol) = self.volumes.get(&sel).cloned() else {
            return;
        };

        self.ortho_saved_tiling = Some(self.tiling);
        let win = self.viewports[src_idx].window;
        let clut = self.viewports[src_idx].clut_name.clone();
        let inv = self.viewports[src_idx].invert;
        let conv = self.viewports[src_idx].convolution_idx;

        self.set_tiling(ViewportTiling::new(1, 3));
        self.ortho_mpr_active = true;
        let center = vol.center_lps;
        self.ortho_crosshair_lps = Some(center);

        let planes = [
            (
                crate::dicom_index::DominantPlane::Axial,
                MprPlane3D::axial(center),
            ),
            (
                crate::dicom_index::DominantPlane::Coronal,
                MprPlane3D::coronal(center),
            ),
            (
                crate::dicom_index::DominantPlane::Sagittal,
                MprPlane3D::sagittal(center),
            ),
        ];
        for (vp_idx, (anat, fallback_plane)) in planes.into_iter().enumerate() {
            let native_sel = self
                .mpr_group_series_for_plane(sel, anat)
                .filter(|&s| self.volumes.contains_key(&s));
            let pane_sel = native_sel.unwrap_or(sel);
            let pane_plane = if native_sel.is_some() {
                if let Some(native_vol) = self.volumes.get(&pane_sel) {
                    let mut p = MprPlane3D::from_volume_native(native_vol);
                    p.offset_mm = dot3(sub3(center, p.center_lps), p.normal);
                    p
                } else {
                    fallback_plane
                }
            } else {
                fallback_plane
            };
            self.assign_series_to_viewport(vp_idx, Some(pane_sel));
            let vp = &mut self.viewports[vp_idx];
            vp.window = win;
            vp.clut_name = clut.clone();
            vp.invert = inv;
            vp.convolution_idx = conv;
            vp.conv_cached = None;
            vp.mpr_enabled = true;
            vp.mpr_plane = Some(pane_plane);
            self.sync_slice_index_from_mpr_plane(vp_idx);
        }
        self.active_viewport = 0;
    }

    pub(crate) fn set_tiling(&mut self, tiling: ViewportTiling) {
        self.tiling = tiling;
        let count = tiling.cell_count();
        if self.active_viewport >= count {
            self.active_viewport = 0;
        }
        self.populate_unassigned_viewports();
    }

    fn populate_unassigned_viewports(&mut self) {
        let Some(index) = &self.index else {
            return;
        };
        if index.studies.is_empty() {
            return;
        }
        let base_sel = self
            .viewports[self.active_viewport]
            .selected
            .or_else(|| (!index.studies[0].series.is_empty()).then_some((0, 0)));
        let Some((study_idx, start_series)) = base_sel else {
            return;
        };
        let series_len = index.studies[study_idx].series.len();
        if series_len == 0 {
            return;
        }

        let cell_count = self.tiling.cell_count();
        let mut to_assign = Vec::new();
        for (vp_idx, vp) in self.viewports.iter().enumerate().take(cell_count) {
            if vp.selected.is_none() {
                let candidate = start_series + vp_idx;
                if candidate < series_len {
                    to_assign.push((vp_idx, (study_idx, candidate)));
                }
            }
        }
        for (vp_idx, sel) in to_assign {
            self.assign_series_to_viewport(vp_idx, Some(sel));
        }
    }

    pub(crate) fn set_viewport_slice(&mut self, vp_idx: usize, idx: isize, synchronize_others: bool) {
        let n = self.viewport_slice_count(vp_idx);
        if n == 0 {
            return;
        }
        let prev_slice = self.viewports[vp_idx].slice;
        let new_slice = idx.clamp(0, n as isize - 1) as usize;
        if new_slice == prev_slice && !synchronize_others {
            return;
        }
        self.viewports[vp_idx].slice = new_slice;
        if self.viewports[vp_idx].mpr_enabled
            && let (Some(vol), Some(mut plane)) = (
                self.viewport_volume(vp_idx).cloned(),
                self.viewports[vp_idx].mpr_plane.clone(),
            ) {
                let half = vol.projected_half_span_mm(plane.normal);
                let t = if n > 1 {
                    new_slice as f64 / (n - 1) as f64
                } else {
                    0.5
                };
                let delta_off = (t * 2.0 - 1.0) * half - plane.offset_mm;
                plane.offset_mm = (t * 2.0 - 1.0) * half;
                let normal = plane.normal;
                self.viewports[vp_idx].mpr_plane = Some(plane);
                if self.ortho_mpr_active && vp_idx < 3
                    && let Some(cur_pt) = self.ortho_crosshair_lps {
                        self.ortho_crosshair_lps = Some(add3(cur_pt, scale3(normal, delta_off)));
                    }
            }
        if vp_idx == self.active_viewport {
            self.drawing.clear();
            self.angle_pending = None;
        }

        if synchronize_others && self.sync_mode != SliceSynchronization::Off && !self.ortho_mpr_active {
            self.synchronize_mpr_followers(vp_idx);
            self.synchronize_from_leader(vp_idx, prev_slice, new_slice);
        }
    }

    pub(crate) fn step_viewport_slice(&mut self, vp_idx: usize, delta: isize) {
        let cur = self.viewports[vp_idx].slice as isize;
        self.set_viewport_slice(vp_idx, cur + delta, true);
    }

    pub(crate) fn all_series_pairs(&self) -> Vec<(usize, usize)> {
        let Some(index) = &self.index else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (si, st) in index.studies.iter().enumerate() {
            for ri in 0..st.series.len() {
                out.push((si, ri));
            }
        }
        out
    }

    pub(crate) fn step_active_series(&mut self, delta: isize) {
        let pairs = self.all_series_pairs();
        if pairs.is_empty() {
            return;
        }
        let vp = self.active_viewport;
        let cur_sel = self.viewports[vp].selected;
        let cur_pos = cur_sel
            .and_then(|sel| pairs.iter().position(|&p| p == sel))
            .unwrap_or(0) as isize;
        let next_pos = (cur_pos + delta).clamp(0, pairs.len() as isize - 1) as usize;
        self.assign_series_to_viewport(vp, Some(pairs[next_pos]));
    }

    /// Synchronizes all other visible viewports to the leader viewport `leader_idx`
    /// using IsiX's `SliceSynchronizer` rules.
    fn synchronize_from_leader(
        &mut self,
        leader_idx: usize,
        leader_prev_slice: usize,
        leader_new_slice: usize,
    ) {
        let mode = self.sync_mode;
        if mode == SliceSynchronization::Off {
            return;
        }
        let cell_count = self.tiling.cell_count();
        if cell_count <= 1 {
            return;
        }

        // Gather leader geometry from the index so synchronization works even before
        // adjacent frames finish decoding.
        let leader_info: Option<LeaderSyncInfo> = self.viewport_series(leader_idx).and_then(|se| {
            let sel = self.viewports[leader_idx].selected?;
            let prev_ref = se.slices.get(leader_prev_slice)?;
            let new_ref = se.slices.get(leader_new_slice)?;
            let orient = new_ref.orientation.map(|v| v as f32);
            let prev_sync = Self::sync_slice_from_ref(prev_ref);
            let new_sync = Self::sync_slice_from_ref(new_ref);
            let center_3d = if new_ref.is_3d_plane {
                let sp = new_ref.spacing.unwrap_or((1.0, 1.0));
                let origin = new_ref.position.unwrap_or([0.0; 3]).map(|v| v as f32);
                Some(ImagePlaneCoordinates::slice_center_patient(
                    se.columns as isize,
                    se.rows as isize,
                    origin,
                    &orient,
                    sp.1,
                    sp.0,
                ))
            } else {
                None
            };
            Some((
                sel,
                orient,
                prev_sync,
                new_sync,
                center_3d,
                se.frame_of_reference_uid.clone(),
            ))
        });
        let Some((leader_sel, leader_orient, leader_prev_sync, leader_new_sync, leader_center_3d, leader_for)) =
            leader_info
        else {
            return;
        };

        // In IsiX, Relative mode passes `before` as `targetCurrentSlice` and `after` as `sourceSlice`.
        let mut updates: Vec<(usize, usize)> = Vec::new();
        for other_idx in 0..cell_count {
            if other_idx == leader_idx {
                continue;
            }
            let Some(other_sel) = self.viewports[other_idx].selected else {
                continue;
            };
            if other_sel == leader_sel {
                // Same series open in two viewports (e.g. different window/CLUT): keep slices in lockstep.
                updates.push((other_idx, leader_new_slice));
                continue;
            }
            // If 3D registration is active on `other_idx`, `synchronize_mpr_followers` already updated its plane or 2D slice.
            if self.registrations.contains_key(&(leader_sel, other_sel))
                && (self.viewports[other_idx].mpr_enabled || self.volumes.contains_key(&other_sel))
            {
                continue;
            }
            let Some(other_se) = self.series_at(other_sel) else {
                continue;
            };
            if other_se.slices.is_empty() {
                continue;
            }
            let other_cur_idx = self.viewports[other_idx]
                .slice
                .min(other_se.slices.len() - 1);
            let other_cur_ref = &other_se.slices[other_cur_idx];
            let other_orient = other_cur_ref.orientation.map(|v| v as f32);
            let target_slices: Vec<SynchronizableSlice> = other_se
                .slices
                .iter()
                .map(Self::sync_slice_from_ref)
                .collect();
            let same_for = match (&leader_for, &other_se.frame_of_reference_uid) {
                (Some(a), Some(b)) => a == b,
                // If FrameOfReferenceUID was omitted by the scanner, treat series within the same study as sharing FOR.
                (None, None) => leader_sel.0 == other_sel.0,
                _ => false,
            };

            // When comparing two different studies before 3D volume registration finishes,
            // align by normalized physical stack progress rather than raw slice index.
            if !same_for && mode == SliceSynchronization::Absolute {
                let leader_len = self
                    .series_at(leader_sel)
                    .map_or(1, |s| s.slices.len())
                    .max(1);
                let frac = if leader_len > 1 {
                    leader_new_slice as f64 / (leader_len - 1) as f64
                } else {
                    0.5
                };
                let mapped =
                    (frac * (other_se.slices.len().saturating_sub(1)) as f64).round() as usize;
                updates.push((other_idx, mapped));
                continue;
            }

            let target_current_for_sync = if mode == SliceSynchronization::Relative {
                leader_prev_sync
            } else {
                target_slices[other_cur_idx]
            };

            let target = SliceSynchronizer::synchronize(
                mode,
                leader_new_sync,
                &leader_orient,
                leader_new_slice,
                false,
                leader_center_3d,
                target_current_for_sync,
                &other_orient,
                &target_slices,
                other_cur_idx,
                false,
                same_for,
            );

            match target {
                SynchronizationTarget::None => {}
                SynchronizationTarget::SliceIndex(idx) => {
                    updates.push((other_idx, idx));
                }
                SynchronizationTarget::PatientPoint(pt) => {
                    let planes: Vec<SlicePlaneParams> = other_se
                        .slices
                        .iter()
                        .map(|s| {
                            let sp = s.spacing.unwrap_or((1.0, 1.0));
                            SlicePlaneParams {
                                origin: s.position.unwrap_or([0.0; 3]).map(|v| v as f32),
                                orientation: s.orientation.map(|v| v as f32),
                                spacing_x: sp.1,
                                spacing_y: sp.0,
                            }
                        })
                        .collect();
                    if let Some((best_idx, _)) = ImagePlaneCoordinates::nearest_slice(&planes, pt) {
                        updates.push((other_idx, best_idx));
                    }
                }
            }
        }

        for (other_idx, new_idx) in updates {
            self.viewports[other_idx].slice = new_idx;
        }
    }

    fn sync_slice_from_ref(s: &SliceRef) -> SynchronizableSlice {
        SynchronizableSlice {
            is_3d_plane: s.is_3d_plane,
            slice_location: s.slice_location.unwrap_or(0.0),
            origin_z: s.position.map_or(0.0, |p| p[2]),
        }
    }

    fn effective_window(&self, vp_idx: usize, f: &Frame) -> Window {
        let vp = &self.viewports[vp_idx];
        match vp.window.or(vp.series_default) {
            Some(w) => f.requested(w).unwrap_or(f.full_window),
            None => f.default_window(),
        }
    }

    fn window_name(&self, vp_idx: usize, f: &Frame) -> presets::Name {
        let vp = &self.viewports[vp_idx];
        let default = vp
            .series_default
            .map_or_else(|| f.default_window(), |w| f.requested(w).unwrap_or(f.full_window));
        self.presets.name(
            self.effective_window(vp_idx, f),
            Some(f.full_window),
            Some(default),
        )
    }

    pub(crate) fn preset_label(&self) -> String {
        let tr = crate::i18n::tr(self.lang);
        match self.viewport_frame(self.active_viewport) {
            Some(f) => format!(
                "{} {}",
                tr.presets_prefix,
                self.window_name(self.active_viewport, f).label()
            ),
            None => tr.presets_none.into(),
        }
    }

    pub(crate) fn set_active_window(&mut self, w: Option<Window>) {
        let leader_idx = self.active_viewport;
        self.viewports[leader_idx].window = w;
        self.wl_drag = None;
        if self.should_sync_viewports() {
            let count = self.tiling.cell_count();
            for (i, vp) in self.viewports.iter_mut().enumerate().take(count) {
                if i != leader_idx && vp.selected.is_some() {
                    vp.window = w;
                }
            }
        }
    }

    fn active_clut_table(&self, vp_idx: usize) -> Option<ColorLookupTable> {
        ColourTableBook::table_named(&self.viewports[vp_idx].clut_name)
    }

    pub(crate) fn persist_measures_for(&self, key: &(PathBuf, u32)) {
        if let Some(ref db) = self.db {
            let list: Vec<crate::db::PersistedMeasure> = self
                .measures
                .get(key)
                .map(|v| v.iter().map(Measure::to_persisted).collect())
                .unwrap_or_default();
            let _ = db.save_rois_for_slice(&key.0, key.1, &list);
        }
    }

    pub(crate) fn export_active_png(&self) {
        let vp_idx = self.active_viewport;
        let Some(frame) = self.viewport_frame(vp_idx) else {
            return;
        };
        let vp = &self.viewports[vp_idx];
        let win = vp
            .window
            .and_then(|w| frame.requested(w))
            .unwrap_or_else(|| frame.default_window());
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name("sekhmetx_export.png")
            .add_filter("PNG Image", &["png"])
            .save_file()
        {
            let _ = crate::export::export_frame_png(
                frame,
                win,
                vp.effective_invert(),
                &vp.clut_name,
                vp.convolution_idx,
                &path,
            );
        }
    }

    pub(crate) fn export_active_series_png(&self) {
        let vp_idx = self.active_viewport;
        let Some((si, ri)) = self.viewports[vp_idx].selected else {
            return;
        };
        let Some(ref idx) = self.index else {
            return;
        };
        let Some(se) = idx.studies.get(si).and_then(|st| st.series.get(ri)) else {
            return;
        };
        let vp = &self.viewports[vp_idx];
        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
            let _ = crate::export::export_series_png(
                &se.slices,
                vp.window,
                vp.effective_invert(),
                &vp.clut_name,
                vp.convolution_idx,
                &dir,
            );
        }
    }

    pub(crate) fn export_active_anonymized_dicom(&self) {
        let vp_idx = self.active_viewport;
        let Some((si, ri)) = self.viewports[vp_idx].selected else {
            return;
        };
        let Some(ref idx) = self.index else {
            return;
        };
        let Some(se) = idx.studies.get(si).and_then(|st| st.series.get(ri)) else {
            return;
        };
        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
            let opts = crate::export::AnonymizeOptions::default();
            let _ = crate::export::anonymize_series(&se.slices, &dir, &opts);
        }
    }

    fn set_copy_policy(&mut self, policy: crate::db::CopyToLibraryPolicy) {
        self.copy_policy = policy;
        if let Some(ref db) = self.db {
            let _ = db.set_setting("copy_to_library_policy", policy.as_str());
        }
    }

    fn copy_ephemeral_studies_to_library(&mut self) {
        let Some(ref mut index) = self.index else {
            return;
        };
        for st in &mut index.studies {
            let is_zip = st.series.iter().any(|se| {
                se.slices.first().is_some_and(|sl| {
                    let p = sl.path.to_string_lossy();
                    p.contains("sekhmetx-zip-") || p.contains("rdicom-zip-cache")
                })
            });
            if is_zip {
                let _ = crate::db::copy_study_into_library(st);
            }
        }
        if let Some(ref mut db) = self.db {
            let _ = db.save_index(index);
        }
    }

    pub(crate) fn viewport_measure_key(&self, vp_idx: usize) -> Option<(PathBuf, u32)> {
        let slice = self.viewports.get(vp_idx)?.slice;
        let s = self.viewport_series(vp_idx)?.slices.get(slice)?;
        Some((s.path.clone(), s.frame))
    }

    // ------------------------------------------------------- background work

    fn poll_background(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.update_rx
            && let Ok(res) = rx.try_recv() {
                self.update_rx = None;
                self.update_status = match res {
                    Ok(Some(info)) => crate::updater::UpdateStatus::Available(info),
                    Ok(None) => crate::updater::UpdateStatus::UpToDate(
                        env!("CARGO_PKG_VERSION").to_string(),
                    ),
                    Err(e) => crate::updater::UpdateStatus::Error(format!("{e:#}")),
                };
            }
        if let Some(rx) = &self.install_rx
            && let Ok(res) = rx.try_recv() {
                self.install_rx = None;
                self.update_status = match res {
                    Ok(tag) => crate::updater::UpdateStatus::Installed(tag),
                    Err(e) => crate::updater::UpdateStatus::Error(format!("{e:#}")),
                };
            }

        if let Some(scan) = &self.scan {
            match scan.rx.try_recv() {
                Ok(index) => {
                    self.scan = None;
                    self.scan_gen += 1;
                    self.loader.reset();
                    self.cache.clear();
                    self.failed.clear();
                    self.volumes.clear();
                    self.registrations.clear();
                    self.registrations_in_flight.clear();
                    self.thumbnails.clear();
                    self.thumbnails_queued.clear();
                    for vp in &mut self.viewports {
                        vp.reset_series_state(None);
                    }
                    if index.studies.is_empty() {
                        self.message = Some(format!(
                            "No DICOM images found ({} files checked).",
                            index.files_seen
                        ));
                    }
                    let mut index = index;
                    let has_ephemeral_zip = index.studies.iter().any(|st| {
                        st.series.iter().any(|se| {
                            se.slices.first().is_some_and(|sl| {
                                let p = sl.path.to_string_lossy();
                                p.contains("sekhmetx-zip-") || p.contains("rdicom-zip-cache")
                            })
                        })
                    });
                    if self.copy_policy == crate::db::CopyToLibraryPolicy::Always {
                        for st in &mut index.studies {
                            let is_zip = st.series.iter().any(|se| {
                                se.slices.first().is_some_and(|sl| {
                                    let p = sl.path.to_string_lossy();
                                    p.contains("sekhmetx-zip-") || p.contains("rdicom-zip-cache")
                                })
                            });
                            if is_zip {
                                let _ = crate::db::copy_study_into_library(st);
                            }
                        }
                    } else if has_ephemeral_zip
                        && self.copy_policy == crate::db::CopyToLibraryPolicy::Ask
                    {
                        self.pending_copy_prompt = true;
                    }
                    let first = (!index.studies.is_empty()
                        && !index.studies[0].series.is_empty())
                    .then_some((0, 0));
                    let first_uid = index.studies.first().map(|s| s.uid.clone());
                    let merged = if let Some(db) = &mut self.db {
                        let _ = db.save_index(&index);
                        match db.load_index() {
                            Ok(saved) if !saved.studies.is_empty() => saved,
                            _ => index,
                        }
                    } else {
                        index
                    };
                    let target_sel = first_uid
                        .as_deref()
                        .and_then(|uid| {
                            merged
                                .studies
                                .iter()
                                .position(|s| s.uid == uid)
                                .filter(|&si| !merged.studies[si].series.is_empty())
                                .map(|si| (si, 0))
                        })
                        .or(first);
                    self.index = Some(merged);
                    self.active_viewport = 0;
                    if let Some(sel) = target_sel {
                        self.assign_series_to_viewport(0, Some(sel));
                        self.populate_unassigned_viewports();
                    }
                }
                Err(_) => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
            }
        }

        for (key, result) in self.loader.poll() {
            if key.scan_gen != self.scan_gen {
                continue;
            }
            match result {
                Ok(frame) => {
                    // Check if any viewport has this series open and needs its reference default window.
                    let slice_len = self
                        .series_at((key.study, key.series))
                        .map_or(0, |s| s.slices.len());
                    let is_ref = Some(key.slice) == defaults::reference_index(slice_len);
                    if is_ref {
                        for vp in &mut self.viewports {
                            if vp.selected == Some((key.study, key.series))
                                && vp.series_default.is_none()
                            {
                                vp.series_default = Some(frame.default_window());
                            }
                        }
                        let tkey = (key.scan_gen, key.study, key.series);
                        self.thumbnails.entry(tkey).or_insert_with(|| {
                            let (tw, th, rgba) = frame.render_thumbnail(44);
                            let cimg = ColorImage::from_rgba_unmultiplied(
                                [tw as usize, th as usize],
                                &rgba,
                            );
                            
                            ctx.load_texture(
                                format!("thumb-{}-{}-{}", key.scan_gen, key.study, key.series),
                                cimg,
                                TextureOptions::LINEAR,
                            )
                        });
                    }
                    if self.cache.is_empty() {
                        let per = frame.bytes().max(1);
                        self.keep_radius = (CACHE_BUDGET_BYTES / per / 2).clamp(15, 5000);
                    }
                    self.cache.insert(key, frame);
                }
                Err(e) => {
                    self.failed.insert(key, e);
                }
            }
        }

        // Gather active focuses across all visible viewports (plus sibling series in their MPR group).
        let cell_count = self.tiling.cell_count();
        let mut focuses: Vec<(usize, usize, usize)> = Vec::with_capacity(cell_count * 2);
        for vp_idx in 0..cell_count {
            if let Some((st, se)) = self.viewports[vp_idx].selected {
                let raw_len = self.series_at((st, se)).map_or(1, |s| s.slices.len());
                let cur_s = self.viewports[vp_idx].slice.min(raw_len.saturating_sub(1));
                focuses.push((st, se, cur_s));
                if let Some(study) = self.study_at(st)
                    && let Some(gid) = study.series.get(se).and_then(|s| s.mpr_group_id)
                        && let Some(group) = study.mpr_groups.get(gid) {
                            for &sib_se in &group.series_indices {
                                if sib_se != se && !focuses.iter().any(|&(a, b, _)| a == st && b == sib_se) {
                                    let sib_mid = study.series.get(sib_se).map_or(0, |s| s.slices.len() / 2);
                                    focuses.push((st, sib_se, sib_mid));
                                }
                            }
                        }
            }
        }

        // Keep memory bounded: retain all slices of focused series up to `keep_radius`.
        let keep = (self.keep_radius / focuses.len().max(1)).max(256);
        self.cache.retain(|k, _| {
            focuses
                .iter()
                .any(|&(st, se, sl)| k.study == st && k.series == se && k.slice.abs_diff(sl) <= keep)
        });

        // Queue visible slices and the rest of each active series so 3D volumes assemble automatically.
        let max_focus_len = self
            .index
            .as_ref()
            .map(|idx| {
                focuses
                    .iter()
                    .filter_map(|&(st, se, _)| idx.studies.get(st)?.series.get(se))
                    .map(|s| s.slices.len())
                    .max()
                    .unwrap_or(512)
            })
            .unwrap_or(512);
        let radius = max_focus_len.max(512);
        self.loader.update_focuses(&focuses, radius);

        let mut jobs = Vec::new();
        if let Some(index) = &self.index {
            for d in 0..=radius {
                for &(study, series, focus_slice) in &focuses {
                    let Some(se) = index.studies.get(study).and_then(|s| s.series.get(series))
                    else {
                        continue;
                    };
                    let n = se.slices.len();
                    for idx in [focus_slice.checked_sub(d), Some(focus_slice + d)]
                        .into_iter()
                        .flatten()
                    {
                        if idx >= n {
                            continue;
                        }
                        let key = SliceKey {
                            scan_gen: self.scan_gen,
                            study,
                            series,
                            slice: idx,
                        };
                        if self.cache.contains_key(&key)
                            || self.failed.contains_key(&key)
                            || self.loader.is_pending(key)
                            || jobs.iter().any(|j: &Job| j.key == key)
                        {
                            continue;
                        }
                        let s = &se.slices[idx];
                        jobs.push(Job {
                            key,
                            path: s.path.clone(),
                            frame: s.frame,
                            urgent: d == 0,
                            distance: d,
                        });
                    }
                }
            }
        }
        for job in jobs {
            self.loader.request(job);
        }

        // Assemble 3D `Volume3D` for any focused series whose slices have all finished decoding.
        let mut newly_built_volume = false;
        for &(study, series, _) in &focuses {
            if self.volumes.contains_key(&(study, series)) {
                continue;
            }
            let Some(se) = self.series_at((study, series)).cloned() else {
                continue;
            };
            let n = se.slices.len();
            let forced = self.forced_mpr_series.contains(&(study, series));
            if !forced && (n < 3 || se.normal.is_none()) {
                continue;
            }
            let mut all_ready = true;
            let mut frames = Vec::with_capacity(n);
            for idx in 0..n {
                let key = SliceKey {
                    scan_gen: self.scan_gen,
                    study,
                    series,
                    slice: idx,
                };
                if let Some(f) = self.cache.get(&key) {
                    frames.push(f.clone());
                } else {
                    all_ready = false;
                    break;
                }
            }
            if all_ready {
                match Volume3D::from_series_checked(&se, &frames, forced) {
                    Ok(vol) => {
                        self.volume_errors.remove(&(study, series));
                        self.volumes.insert((study, series), vol);
                        newly_built_volume = true;
                    }
                    Err(err) => {
                        self.volume_errors.insert((study, series), err);
                    }
                }
            }
        }

        // Automatically enable 3D MPR on MPR-capable studies once their 3D volume is ready.
        for vp_idx in 0..cell_count {
            let Some(key) = self.viewports[vp_idx].selected else {
                continue;
            };
            if self.viewports[vp_idx].mpr_auto_applied || self.viewports[vp_idx].mpr_enabled {
                continue;
            }
            let capable = self.series_at(key).is_some_and(|s| s.is_mpr_capable());
            if !capable {
                continue;
            }
            if let Some(vol) = self.volumes.get(&key).cloned() {
                self.viewports[vp_idx].mpr_auto_applied = true;
                let cur_s = self.viewports[vp_idx].slice.min(vol.depth.saturating_sub(1));
                let mut p = MprPlane3D::from_volume_native(&vol);
                let z_lps = vol.voxel_to_lps([
                    vol.width as f64 * 0.5,
                    vol.height as f64 * 0.5,
                    cur_s as f64,
                ]);
                p.offset_mm = dot3(sub3(z_lps, p.center_lps), p.normal);
                self.set_viewport_mpr_plane(vp_idx, p, false);
            }
        }

        if let Some((vp_idx, key, ortho)) = self.pending_mpr_activate {
            if self.viewports[vp_idx].selected != Some(key) {
                self.pending_mpr_activate = None;
            } else if self.volumes.contains_key(&key) {
                self.pending_mpr_activate = None;
                self.active_viewport = vp_idx;
                if ortho {
                    if !self.ortho_mpr_active {
                        self.toggle_orthogonal_mpr();
                    }
                } else if let Some(p) = self.effective_mpr_plane(vp_idx) {
                    self.set_viewport_mpr_plane(vp_idx, p, true);
                }
            } else if !self.forced_mpr_series.contains(&key) && self.volume_errors.contains_key(&key) {
                self.pending_mpr_activate = None;
                self.pending_force_mpr = Some(PendingForceMpr { vp_idx, key, ortho });
            }
        }

        // Drain completed background 3D registrations (one direction + exact inverse).
        let mut completed_reg = false;
        while let Ok((pair, reg)) = self.reg_rx.try_recv() {
            let inv = reg.inverse();
            self.registrations_in_flight.remove(&pair);
            self.registrations_in_flight.remove(&(pair.1, pair.0));
            self.registrations.insert(pair, reg);
            self.registrations.insert((pair.1, pair.0), inv);
            completed_reg = true;
        }

        // Automatically compute 3D Rigid Registration between the active viewport's volume
        // and any other visible viewport's volume on a worker thread so the UI never freezes.
        if !self.ortho_mpr_active && cell_count > 1 {
            let leader_idx = self.active_viewport.min(cell_count - 1);
            if let (Some(leader_sel), Some(vol_a)) = (
                self.viewports[leader_idx].selected,
                self.viewport_volume(leader_idx).cloned(),
            ) {
                let mut auto_sync_needed = newly_built_volume || completed_reg;
                for other_idx in 0..cell_count {
                    if other_idx == leader_idx {
                        continue;
                    }
                    if let (Some(other_sel), Some(vol_b)) = (
                        self.viewports[other_idx].selected,
                        self.viewport_volume(other_idx).cloned(),
                    )
                        && other_sel != leader_sel
                            && !self.registrations.contains_key(&(leader_sel, other_sel))
                            && !self.registrations_in_flight.contains(&(leader_sel, other_sel))
                        {
                            let same_for = matches!(
                                (&vol_a.frame_of_reference_uid, &vol_b.frame_of_reference_uid),
                                (Some(a), Some(b)) if !a.is_empty() && a == b
                            );
                            if same_for {
                                let id = RigidTransform3D::identity(1.0);
                                self.registrations.insert((leader_sel, other_sel), id.clone());
                                self.registrations.insert((other_sel, leader_sel), id);
                                auto_sync_needed = true;
                            } else {
                                self.registrations_in_flight.insert((leader_sel, other_sel));
                                self.registrations_in_flight.insert((other_sel, leader_sel));
                                let va = vol_a.clone();
                                let vb = vol_b.clone();
                                let tx = self.reg_tx.clone();
                                let ctx_clone = ctx.clone();
                                std::thread::spawn(move || {
                                    let reg = RigidTransform3D::register(&va, &vb);
                                    let _ = tx.send(((leader_sel, other_sel), reg));
                                    ctx_clone.request_repaint();
                                });
                            }
                        }
                }
                if auto_sync_needed && self.sync_mode == SliceSynchronization::Absolute {
                    self.synchronize_mpr_followers(leader_idx);
                }
            }
        }

        // Cine playback on the active viewport (and synchronized followers).
        let now_secs = ctx.input(|i| i.time);
        self.pacs_web.lang = self.lang;
        let new_pacs_zips = self.pacs_web.poll(now_secs);
        if let Some(profile) = self.pacs_web.newly_trained_profile.take()
            && let Some(ref db) = self.db
                && let Ok(json) = serde_json::to_string(&profile) {
                    let key = format!("pacs_trained_{}", profile.origin);
                    let _ = db.set_setting(&key, &json);
                }
        if self.pacs_web.just_hooked {
            self.pacs_web.just_hooked = false;
            self.show_pacs_modal = false;
            if let Some(ref db) = self.db {
                let _ = db.set_setting("pacs_auto_connect", "1");
                if self.pacs_web.institution_name.to_lowercase().contains("maip")
                    && !self.pacs_web.login_url.trim().is_empty()
                {
                    let _ = db.set_setting("pacs_known_maipu_url", self.pacs_web.login_url.trim());
                }
            }
        }
        if !self.show_pacs_finder {
            self.pacs_web.close_on_demand_webview();
        }
        if !new_pacs_zips.is_empty() && self.scan.is_none() {
            if self.pacs_web.pending_open_downloaded {
                self.pacs_web.pending_open_downloaded = false;
                self.show_pacs_finder = false;
            }
            self.start_scan(new_pacs_zips);
        }
        if self.pacs_web.watch_dir.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_secs(2));
        }

        let active_count = self.viewport_slice_count(self.active_viewport);
        if self.playing && active_count > 1 {
            let now = ctx.input(|i| i.time);
            if now - self.last_tick >= 1.0 / self.fps as f64 {
                let next = (self.active_cell().slice + 1) % active_count;
                if let Some(key) = self.viewport_slice_key(self.active_viewport, next)
                    && (self.cache.contains_key(&key) || self.failed.contains_key(&key)) {
                        self.set_viewport_slice(self.active_viewport, next as isize, true);
                        self.last_tick = now;
                    }
            }
            ctx.request_repaint();
        }
    }

    // ------------------------------------------------------------- input

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| !p.as_os_str().is_empty())
                .collect()
        });
        if !dropped.is_empty() {
            self.start_scan(dropped);
        }
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let keys = [
            Key::ArrowUp,
            Key::ArrowDown,
            Key::ArrowLeft,
            Key::ArrowRight,
            Key::PageUp,
            Key::PageDown,
            Key::Home,
            Key::End,
            Key::I,
            Key::R,
            Key::H,
            Key::V,
            Key::P,
            Key::Space,
            Key::Delete,
            Key::Backspace,
            Key::Escape,
            Key::Enter,
            Key::W,
            Key::S,
            Key::O,
            Key::M,
            Key::L,
            Key::A,
            Key::C,
            Key::E,
            Key::B,
            Key::G,
            Key::F,
            Key::T,
            Key::Z,
            Key::OpenBracket,
            Key::CloseBracket,
            Key::Num0,
            Key::Num1,
            Key::Num2,
            Key::Num3,
            Key::Num4,
            Key::Num5,
            Key::Num6,
            Key::Num7,
            Key::Num8,
            Key::Num9,
        ];
        let pressed: Vec<Key> =
            ctx.input(|i| keys.iter().copied().filter(|k| i.key_pressed(*k)).collect());
        let vp = self.active_viewport;
        for key in pressed {
            match key {
                Key::ArrowUp | Key::ArrowLeft => self.step_viewport_slice(vp, -1),
                Key::ArrowDown | Key::ArrowRight => self.step_viewport_slice(vp, 1),
                Key::PageUp => self.step_viewport_slice(vp, -10),
                Key::PageDown => self.step_viewport_slice(vp, 10),
                Key::Home => self.set_viewport_slice(vp, 0, true),
                Key::End => self.set_viewport_slice(vp, isize::MAX, true),
                Key::I => self.viewports[vp].invert = !self.viewports[vp].invert,
                Key::H => self.viewports[vp].x_flipped = !self.viewports[vp].x_flipped,
                Key::V => self.viewports[vp].y_flipped = !self.viewports[vp].y_flipped,
                Key::R => {
                    self.viewports[vp].reset_view();
                    self.viewports[vp].reset_clut();
                    self.viewports[vp].invert = false;
                    self.set_active_window(None);
                }
                Key::P | Key::Space => self.playing = !self.playing,
                Key::Delete | Key::Backspace => {
                    if let Some(k) = self.viewport_measure_key(vp) {
                        if let Some(list) = self.measures.get_mut(&k) {
                            list.pop();
                        }
                        self.persist_measures_for(&k);
                    }
                }
                Key::Escape => {
                    self.drawing.clear();
                    self.angle_pending = None;
                }
                Key::Enter => {
                    if self.tool == Tool::Polygon && self.drawing.len() >= 3 {
                        let pts = std::mem::take(&mut self.drawing);
                        if let Some(k) = self.viewport_measure_key(vp) {
                            self.measures
                                .entry(k.clone())
                                .or_default()
                                .push(Measure::Polygon(pts));
                            self.persist_measures_for(&k);
                        }
                    }
                }
                Key::W => {
                    self.tool = Tool::WindowLevel;
                    self.drawing.clear();
                    self.angle_pending = None;
                }
                Key::A => {
                    self.switch_viewport_to_plane(vp, crate::dicom_index::DominantPlane::Axial);
                }
                Key::C => {
                    self.switch_viewport_to_plane(vp, crate::dicom_index::DominantPlane::Coronal);
                }
                Key::S => {
                    self.switch_viewport_to_plane(vp, crate::dicom_index::DominantPlane::Sagittal);
                }
                Key::O => {
                    self.tool = Tool::Orbit3D;
                    self.drawing.clear();
                    self.angle_pending = None;
                }
                Key::M | Key::L => {
                    self.tool = Tool::Length;
                    self.drawing.clear();
                    self.angle_pending = None;
                }
                Key::E => {
                    self.tool = Tool::Oval;
                    self.drawing.clear();
                    self.angle_pending = None;
                }
                Key::B => {
                    self.tool = Tool::Rect;
                    self.drawing.clear();
                    self.angle_pending = None;
                }
                Key::G => {
                    self.tool = Tool::Polygon;
                    self.drawing.clear();
                    self.angle_pending = None;
                }
                Key::F => {
                    self.tool = Tool::Pencil;
                    self.drawing.clear();
                    self.angle_pending = None;
                }
                Key::T => {
                    self.tool = Tool::Point;
                    self.drawing.clear();
                    self.angle_pending = None;
                }
                Key::Z => {
                    self.tool = Tool::Zoom;
                    self.drawing.clear();
                    self.angle_pending = None;
                }
                Key::OpenBracket => self.step_active_series(-1),
                Key::CloseBracket => self.step_active_series(1),
                Key::Num0 => self.set_active_window(None),
                k => {
                    let preset = NUMBER_KEYS
                        .iter()
                        .position(|x| *x == k)
                        .and_then(|n| self.presets.hot_key(n))
                        .and_then(|e| e.window);
                    if let Some(w) = preset {
                        self.set_active_window(Some(w));
                    }
                }
            }
        }
    }

    // ----------------------------------------------------------------- UI

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let tr = crate::i18n::tr(self.lang);
        ui.horizontal(|ui| {
            // Bottom-left: Institution Login (turns green with institution name once hooked) + Search PACS
            let is_hooked = matches!(
                self.pacs_web.state,
                crate::pacs_web::PacsHookState::Hooked { .. }
            );
            let inst_label = if is_hooked {
                let name = self.pacs_web.institution_name.trim();
                if name.is_empty() {
                    tr.pacs_connected.to_string()
                } else {
                    format!("â— {name}")
                }
            } else {
                tr.pacs_login_btn.to_string()
            };
            let login_btn = if is_hooked {
                egui::Button::new(egui::RichText::new(inst_label).small().color(Color32::WHITE))
                    .fill(Color32::from_rgb(38, 132, 68))
            } else {
                egui::Button::new(egui::RichText::new(inst_label).small())
            };
            if ui
                .add(login_btn)
                .on_hover_text(tr.pacs_login_hover)
                .clicked()
            {
                self.show_pacs_modal = !self.show_pacs_modal;
            }
            if ui
                .add_enabled(
                    is_hooked,
                    egui::Button::new(egui::RichText::new(tr.pacs_search_btn).small()),
                )
                .on_hover_text(tr.pacs_search_hover)
                .clicked()
            {
                self.show_pacs_finder = !self.show_pacs_finder;
            }
            if ui
                .add_enabled(
                    self.pacs_web.watch_dir.is_some() && self.scan.is_none(),
                    egui::Button::new(egui::RichText::new(tr.pacs_refresh_watch_btn).small()),
                )
                .on_hover_text(tr.pacs_refresh_watch_hover)
                .clicked()
            {
                let zips = self.pacs_web.scan_watch_folder_now();
                if !zips.is_empty() {
                    self.start_scan(zips);
                } else if let Some(dir) = self.pacs_web.watch_dir.clone() {
                    let has_entries = std::fs::read_dir(&dir)
                        .ok()
                        .into_iter()
                        .flatten()
                        .flatten()
                        .any(|e| e.file_name() != ".sekhmetx_zips.json");
                    if has_entries {
                        self.start_scan(vec![dir]);
                    } else {
                        self.message = Some(tr.pacs_msg_watch_empty.into());
                    }
                }
            }
            ui.separator();

        if let Some(ref dl) = self.pacs_web.downloading {
                ui.add(
                    egui::ProgressBar::new(0.0)
                        .desired_width(220.0)
                        .animate(true)
                        .text(format!("{} {dl}â€¦", tr.pacs_msg_starting_dl)),
                );
                ui.separator();
                ui.ctx().request_repaint();
            }
        if let Some(scan) = &self.scan {
                let done = scan.done.load(Ordering::Relaxed);
                let total = scan.total.load(Ordering::Relaxed);
                ui.spinner();
                if total == 0 {
                    ui.label(tr.refreshing_db);
                } else {
                    ui.add(
                        egui::ProgressBar::new(done as f32 / total as f32)
                            .desired_width(200.0)
                            .text(format!("{} {done} / {total} {}", tr.indexing_files, tr.files_word)),
                    );
                }
            } else if let Some(index) = &self.index {
                ui.label(format!(
                    "{} {} Â· {} {} Â· {} {} Â· {} {}",
                    index.studies.len(),
                    tr.studies,
                    index.image_count(),
                    tr.images,
                    index.files_skipped,
                    tr.other_files_skipped,
                    if self.gpu.is_some() { "GPU" } else { "CPU" },
                    tr.pipeline_word,
                ));
            }
            if let Some(msg) = &self.message {
                ui.separator();
                ui.colored_label(Color32::from_rgb(255, 180, 80), msg);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("v{}", crate::BUILD_VERSION))
                        .small()
                        .color(Color32::from_gray(150)),
                );
                if self.previous_crash_pending {
                    let lbl = match self.lang {
                        crate::i18n::Lang::Es => "âš  Registro de Fallo",
                        crate::i18n::Lang::En => "âš  Crash Log",
                    };
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(lbl).small().color(Color32::WHITE),
                            )
                            .fill(Color32::from_rgb(175, 65, 50)),
                        )
                        .clicked()
                    {
                        self.show_crash_modal = true;
                    }
                }
                if let Some(info) = &self.cursor_info {
                    ui.separator();
                    ui.monospace(info);
                }
            });
        });
    }

    fn browser(&mut self, ui: &mut egui::Ui) {
        let tr = crate::i18n::tr(self.lang);
        if self.index.is_none() {
            ui.add_space(8.0);
            ui.label(tr.no_studies);
            ui.label(tr.drop_hint);
            return;
        }

        // Horos-style Search & Modality Filter Bar
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("ðŸ”");
            let edit = egui::TextEdit::singleline(&mut self.browser_search)
                .hint_text(tr.search_hint)
                .desired_width(ui.available_width() - 28.0);
            ui.add(edit);
            if !self.browser_search.is_empty() && ui.small_button("âœ•").clicked() {
                self.browser_search.clear();
            }
        });
        ui.horizontal_wrapped(|ui| {
            for (code, label) in [
                ("ALL", tr.all),
                ("CT", "CT"),
                ("MR", "MR"),
                ("US", "US"),
                ("DX", "DX/CR"),
                ("PT", "PT/NM"),
            ] {
                let selected = self.browser_modality == code;
                if ui.selectable_label(selected, label).clicked() {
                    self.browser_modality = code.to_string();
                }
            }
        });
        ui.separator();

        let query = self.browser_search.trim().to_ascii_lowercase();
        let mod_filter = self.browser_modality.clone();
        let scan_gen = self.scan_gen;
        let mut clicked: Option<(usize, (usize, usize), bool)> = None;
        let mut tile_study: Option<(usize, ViewportTiling)> = None;
        let mut remove_study_uid: Option<String> = None;
        let mut clear_library = false;
        let mut thumb_jobs: Vec<Job> = Vec::new();
        let active_sel = self.active_cell().selected;
        let cell_count = self.tiling.cell_count();

        let Some(index) = &self.index else {
            return;
        };

        ui.horizontal(|ui| {
            let disk_mb = crate::db::library_disk_usage_bytes() as f64 / (1024.0 * 1024.0);
            ui.label(
                egui::RichText::new(format!(
                    "{} Â· {} {} ({disk_mb:.1} MB)",
                    tr.library,
                    index.studies.len(),
                    tr.studies
                ))
                .small()
                .color(Color32::from_gray(165)),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button(tr.clear)
                    .on_hover_text("Clear studies from the local library (files on disk are untouched)")
                    .clicked()
                {
                    clear_library = true;
                }
            });
        });
        ui.add_space(2.0);

        // Pre-group matching studies by Patient (manual link override -> Patient ID -> normalized Patient Name).
        struct BrowserPatientGroup {
            key: String,
            display_name: String,
            display_id: String,
            studies: Vec<(usize, Vec<usize>)>,
        }
        let mut patient_groups: Vec<BrowserPatientGroup> = Vec::new();
        let mut group_idx_by_key: HashMap<String, usize> = HashMap::new();
        for (si, study) in index.studies.iter().enumerate() {
            let study_matches_query = query.is_empty()
                || study.patient_name.to_ascii_lowercase().contains(&query)
                || study.patient_id.to_ascii_lowercase().contains(&query)
                || study.description.to_ascii_lowercase().contains(&query)
                || study.accession.to_ascii_lowercase().contains(&query)
                || study.date.to_ascii_lowercase().contains(&query);

            let matching_series: Vec<usize> = study
                .series
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    let mod_ok = match mod_filter.as_str() {
                        "ALL" => true,
                        "DX" => matches!(s.modality.as_str(), "DX" | "CR" | "RG" | "MG"),
                        "PT" => matches!(s.modality.as_str(), "PT" | "NM" | "ST"),
                        m => s.modality.eq_ignore_ascii_case(m),
                    };
                    if !mod_ok {
                        return false;
                    }
                    study_matches_query
                        || s.description.to_ascii_lowercase().contains(&query)
                        || s.modality.to_ascii_lowercase().contains(&query)
                        || s.dominant_plane
                            .badge()
                            .to_ascii_lowercase()
                            .contains(&query)
                })
                .map(|(ri, _)| ri)
                .collect();

            if matching_series.is_empty() {
                continue;
            }

            let key = if let Some(over) = self.patient_links.get(&study.uid) {
                over.clone()
            } else {
                let pid = study.patient_id.trim();
                if !pid.is_empty() {
                    format!("id:{}", pid.to_ascii_uppercase())
                } else {
                    let norm_name = study
                        .patient_name
                        .replace('^', " ")
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                        .to_ascii_uppercase();
                    format!("name:{norm_name}")
                }
            };

            if let Some(&gi) = group_idx_by_key.get(&key) {
                patient_groups[gi].studies.push((si, matching_series));
            } else {
                let gi = patient_groups.len();
                group_idx_by_key.insert(key.clone(), gi);
                patient_groups.push(BrowserPatientGroup {
                    key,
                    display_name: study.patient_name.replace('^', " "),
                    display_id: study.patient_id.trim().to_string(),
                    studies: vec![(si, matching_series)],
                });
            }
        }

        let mut start_link: Option<(String, Vec<String>)> = None;
        let mut commit_link_target: Option<String> = None;
        let mut unlink_study_uid: Option<String> = None;
        let mut cancel_linking = false;

        if let Some((ref src_name, _)) = self.linking_source {
            ui.group(|ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(
                        Color32::from_rgb(255, 210, 100),
                        format!("{} {src_name}", tr.linking_banner),
                    );
                    if ui.small_button(tr.cancel_link).clicked() {
                        cancel_linking = true;
                    }
                });
            });
            ui.add_space(2.0);
        }

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                for pg in &patient_groups {
                    let mut pt_label = pg.display_name.clone();
                    if !pg.display_id.is_empty() {
                        pt_label.push_str(&format!(" Â· ID {}", pg.display_id));
                    }
                    pt_label.push_str(&format!(" ({})", pg.studies.len()));

                    let is_linking = self.linking_source.is_some();
                    let pg_resp = ui.horizontal(|ui| {
                        let resp = egui::CollapsingHeader::new(
                            egui::RichText::new(&pt_label).strong(),
                        )
                        .id_salt(format!("pt_{}", pg.key))
                        .default_open(false)
                        .show(ui, |ui| {
                            for (si, matching_series) in &pg.studies {
                                let si = *si;
                                let study = &index.studies[si];
                                let desc = if study.description.trim().is_empty() {
                                    "Study"
                                } else {
                                    study.description.trim()
                                };
                                let title = if study.files_missing {
                                    format!("âš  [files missing] {desc} â€” {}", study.date)
                                } else {
                                    format!("{desc} â€” {}", study.date)
                                };
                                let hdr_resp =
                                    egui::CollapsingHeader::new(egui::RichText::new(title))
                                        .id_salt(&study.uid)
                                        .default_open(false)
                                        .show(ui, |ui| {
                            let mut sub = Vec::new();
                            if !study.description.is_empty() {
                                sub.push(study.description.clone());
                            }
                            if !study.patient_id.is_empty() {
                                sub.push(format!("ID {}", study.patient_id));
                            }
                            if !study.accession.is_empty() {
                                sub.push(format!("Acc {}", study.accession));
                            }
                            if !sub.is_empty() {
                                ui.small(sub.join(" Â· "));
                            }
                            if study.series.len() >= 2 {
                                ui.horizontal(|ui| {
                                    ui.small(tr.compare);
                                    if ui.small_button("1Ã—2").clicked() {
                                        tile_study = Some((si, ViewportTiling::new(1, 2)));
                                    }
                                    if ui.small_button("2Ã—2").clicked() {
                                        tile_study = Some((si, ViewportTiling::new(2, 2)));
                                    }
                                    if study.series.len() >= 5 && ui.small_button("2Ã—3").clicked()
                                    {
                                        tile_study = Some((si, ViewportTiling::new(2, 3)));
                                    }
                                });
                            }

                            // Helper closure to render a single series row (with 38x38 thumbnail & click/context menu).
                            let draw_series_row = |ui: &mut egui::Ui,
                                                   ri: usize,
                                                   clicked: &mut Option<(usize, (usize, usize), bool)>,
                                                   thumb_jobs: &mut Vec<Job>| {
                                let series = &study.series[ri];
                                let desc = if series.description.is_empty() {
                                    "(no description)"
                                } else {
                                    &series.description
                                };
                                let mpr_tag = if series.is_mpr_capable() { "Â·MPR" } else { "" };
                                let is_active = active_sel == Some((si, ri));

                                // Queue low-priority background job for the middle slice thumbnail if not yet requested.
                                let tkey = (scan_gen, si, ri);
                                if !self.thumbnails.contains_key(&tkey)
                                    && !self.thumbnails_queued.contains(&tkey)
                                    && let Some(ref_idx) =
                                        defaults::reference_index(series.slices.len())
                                        && let Some(sref) = series.slices.get(ref_idx) {
                                            thumb_jobs.push(Job {
                                                key: SliceKey {
                                                    scan_gen,
                                                    study: si,
                                                    series: ri,
                                                    slice: ref_idx,
                                                },
                                                path: sref.path.clone(),
                                                frame: sref.frame,
                                                urgent: false,
                                                distance: 800 + ri,
                                            });
                                        }

                                ui.horizontal(|ui| {
                                    // Draw 38x38 Horos-style series thumbnail box
                                    let thumb_size = vec2(38.0, 38.0);
                                    let (thumb_rect, thumb_resp) =
                                        ui.allocate_exact_size(thumb_size, Sense::click());
                                    let painter = ui.painter();
                                    painter.rect_filled(
                                        thumb_rect,
                                        4.0,
                                        Color32::from_rgb(12, 14, 18),
                                    );
                                    if let Some(tex) = self.thumbnails.get(&tkey) {
                                        painter.image(
                                            tex.id(),
                                            thumb_rect.shrink(1.0),
                                            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                            Color32::WHITE,
                                        );
                                    } else {
                                        painter.text(
                                            thumb_rect.center(),
                                            Align2::CENTER_CENTER,
                                            &series.modality,
                                            FontId::monospace(10.5),
                                            Color32::from_gray(145),
                                        );
                                    }
                                    let border_col = if is_active {
                                        Color32::from_rgb(80, 175, 255)
                                    } else {
                                        Color32::from_gray(60)
                                    };
                                    painter.rect_stroke(
                                        thumb_rect,
                                        4.0,
                                        Stroke::new(if is_active { 1.5 } else { 1.0 }, border_col),
                                        egui::StrokeKind::Inside,
                                    );

                                    let text = format!(
                                        "{:<2} {:>3} [{}{}] {}\n{} images Â· {}Ã—{}",
                                        series.modality,
                                        series.number,
                                        series.dominant_plane.badge(),
                                        mpr_tag,
                                        desc,
                                        series.slices.len(),
                                        series.columns,
                                        series.rows
                                    );
                                    let resp = ui
                                        .selectable_label(
                                            is_active,
                                            egui::RichText::new(text).monospace().size(11.5),
                                        )
                                        .on_hover_text(format!(
                                            "{} {}, {}Ã—{}, plane: {}{} (click for active viewport, right-click to assign)",
                                            series.slices.len(),
                                            tr.images,
                                            series.columns,
                                            series.rows,
                                            series.dominant_plane.badge(),
                                            if series.is_mpr_capable() { " Â· 3D MPR Ready" } else { "" }
                                        ));
                                    if resp.clicked() || thumb_resp.clicked() {
                                        *clicked = Some((self.active_viewport, (si, ri), false));
                                    }
                                    resp.context_menu(|ui| {
                                        for vp_idx in 0..cell_count {
                                            if ui
                                                .button(format!("Open in Viewport {}", vp_idx + 1))
                                                .clicked()
                                            {
                                                *clicked = Some((vp_idx, (si, ri), false));
                                                ui.close();
                                            }
                                        }
                                    });
                                });
                            };

                            // Render series in list order, grouping multi-plane AX/COR/SAG reconstructions
                            // into a compact Horos-style block (no duplicate rows outside the block).
                            let mut rendered_series = std::collections::HashSet::new();
                            for &ri in matching_series {
                                if rendered_series.contains(&ri) {
                                    continue;
                                }
                                let grp = study.series[ri]
                                    .mpr_group_id
                                    .and_then(|gid| study.mpr_groups.iter().find(|g| g.id == gid))
                                    .filter(|g| {
                                        g.series_indices
                                            .iter()
                                            .filter(|idx| matching_series.contains(idx))
                                            .count()
                                            >= 2
                                    });

                                if let Some(group) = grp {
                                    let grp_active = active_sel.is_some_and(|(s_i, r_i)| {
                                        s_i == si && group.series_indices.contains(&r_i)
                                    });
                                    let mut ordered_planes = Vec::new();
                                    for opt_ri in [
                                        group.axial_series,
                                        group.coronal_series,
                                        group.sagittal_series,
                                    ] {
                                        if let Some(pri) = opt_ri
                                            && matching_series.contains(&pri)
                                                && !ordered_planes.contains(&pri)
                                            {
                                                ordered_planes.push(pri);
                                            }
                                    }
                                    for &gri in &group.series_indices {
                                        if matching_series.contains(&gri)
                                            && !ordered_planes.contains(&gri)
                                        {
                                            ordered_planes.push(gri);
                                        }
                                    }
                                    for &pri in &ordered_planes {
                                        rendered_series.insert(pri);
                                    }

                                    ui.push_id(("mpr_grp_block", si, group.id), |ui| {
                                        egui::Frame::group(ui.style())
                                            .fill(Color32::from_rgb(28, 34, 44))
                                            .stroke(Stroke::new(
                                                1.0,
                                                if grp_active {
                                                    Color32::from_rgb(70, 145, 230)
                                                } else {
                                                    Color32::from_rgb(52, 60, 74)
                                                },
                                            ))
                                            .inner_margin(egui::Margin::symmetric(6, 5))
                                            .show(ui, |ui| {
                                                let grp_text = format!(
                                                    "ðŸ§Š {}  [{}]",
                                                    group.label, group.planes_badge
                                                );
                                                let hdr = ui
                                                    .selectable_label(
                                                        grp_active,
                                                        egui::RichText::new(grp_text)
                                                            .monospace()
                                                            .strong()
                                                            .size(11.0)
                                                            .color(Color32::from_rgb(130, 210, 255)),
                                                    )
                                                    .on_hover_text(
                                                        "Grouped Axial / Coronal / Sagittal reconstructions. Click to open in 3D MPR.",
                                                    );
                                                if hdr.clicked() {
                                                    let target_ri = active_sel
                                                        .filter(|&(s_i, r_i)| {
                                                            s_i == si
                                                                && group.series_indices.contains(&r_i)
                                                        })
                                                        .map_or(group.primary_series, |(_, r_i)| r_i);
                                                    clicked = Some((
                                                        self.active_viewport,
                                                        (si, target_ri),
                                                        true,
                                                    ));
                                                }
                                                ui.add_space(2.0);
                                                for pri in ordered_planes {
                                                    draw_series_row(
                                                        ui,
                                                        pri,
                                                        &mut clicked,
                                                        &mut thumb_jobs,
                                                    );
                                                }
                                            });
                                    });
                                } else {
                                    rendered_series.insert(ri);
                                    draw_series_row(ui, ri, &mut clicked, &mut thumb_jobs);
                                }
                            }
                        });
                                    hdr_resp.header_response.context_menu(|ui| {
                                        if (pg.studies.len() > 1
                                            || self.patient_links.contains_key(&study.uid))
                                            && ui.button(tr.unlink_study).clicked() {
                                                unlink_study_uid = Some(study.uid.clone());
                                                ui.close();
                                            }
                                        if ui.button(tr.link_patient).clicked() {
                                            start_link = Some((
                                                format!("{} ({})", study.patient_name, study.date),
                                                vec![study.uid.clone()],
                                            ));
                                            ui.close();
                                        }
                                        if ui.button(tr.remove_from_library).clicked() {
                                            remove_study_uid = Some(study.uid.clone());
                                            ui.close();
                                        }
                                    });
                                }
                        });
                        if is_linking && ui.small_button(tr.link_here).clicked() {
                            commit_link_target = Some(pg.key.clone());
                        }
                        resp
                    });
                    pg_resp.inner.header_response.context_menu(|ui| {
                        if ui.button(tr.link_patient).clicked() {
                            let uids = pg
                                .studies
                                .iter()
                                .map(|(si, _)| index.studies[*si].uid.clone())
                                .collect();
                            start_link = Some((pg.display_name.clone(), uids));
                            ui.close();
                        }
                    });
                }
            });

        if cancel_linking {
            self.linking_source = None;
        }
        if let Some(src) = start_link {
            self.linking_source = Some(src);
        }
        if let Some(target_key) = commit_link_target
            && let Some((_, uids)) = self.linking_source.take() {
                for uid in uids {
                    self.patient_links.insert(uid, target_key.clone());
                }
                if let Some(ref db) = self.db
                    && let Ok(json) = serde_json::to_string(&self.patient_links) {
                        let _ = db.set_setting("patient_links", &json);
                    }
            }
        if let Some(uid) = unlink_study_uid {
            self.patient_links.insert(uid.clone(), format!("solo:{uid}"));
            if let Some(ref db) = self.db
                && let Ok(json) = serde_json::to_string(&self.patient_links) {
                    let _ = db.set_setting("patient_links", &json);
                }
        }

        if self.pending_copy_prompt {
            ui.add_space(4.0);
            ui.group(|ui| {
                ui.label(
                    egui::RichText::new(tr.copy_zip_prompt)
                        .small()
                        .strong()
                        .color(Color32::from_rgb(255, 210, 100)),
                );
                ui.horizontal(|ui| {
                    if ui.small_button(tr.copy_btn).clicked() {
                        self.copy_ephemeral_studies_to_library();
                        self.pending_copy_prompt = false;
                    }
                    if ui.small_button(tr.always_btn).clicked() {
                        self.set_copy_policy(crate::db::CopyToLibraryPolicy::Always);
                        self.copy_ephemeral_studies_to_library();
                        self.pending_copy_prompt = false;
                    }
                    if ui.small_button(tr.no_btn).clicked() {
                        self.pending_copy_prompt = false;
                    }
                    if ui.small_button(tr.never_btn).clicked() {
                        self.set_copy_policy(crate::db::CopyToLibraryPolicy::Never);
                        self.pending_copy_prompt = false;
                    }
                });
            });
        }

        if clear_library {
            if let Some(db) = &self.db {
                let _ = db.clear_all();
            }
            self.index = None;
            for vp in &mut self.viewports {
                vp.reset_series_state(None);
            }
            return;
        }
        if let Some(uid) = remove_study_uid {
            if let Some(db) = &self.db {
                let _ = db.remove_study(&uid);
            }
            if let Some(idx) = &mut self.index {
                idx.studies.retain(|s| s.uid != uid);
                if idx.studies.is_empty() {
                    self.index = None;
                }
            }
            for vp in &mut self.viewports {
                vp.reset_series_state(None);
            }
            if self.index.is_some() {
                self.assign_series_to_viewport(0, Some((0, 0)));
            }
            return;
        }

        for job in thumb_jobs {
            self.thumbnails_queued
                .insert((job.key.scan_gen, job.key.study, job.key.series));
            self.loader.request(job);
        }

        if let Some((si, tiling)) = tile_study {
            self.tiling = tiling;
            let count = tiling.cell_count();
            let series_len = self.index.as_ref().map_or(0, |idx| idx.studies[si].series.len());
            for vp_idx in 0..count {
                if vp_idx < series_len {
                    self.assign_series_to_viewport(vp_idx, Some((si, vp_idx)));
                } else {
                    self.assign_series_to_viewport(vp_idx, None);
                }
            }
            self.active_viewport = 0;
        } else if let Some((vp_idx, sel, enable_mpr)) = clicked {
            self.active_viewport = vp_idx;
            self.assign_series_to_viewport(vp_idx, Some(sel));
            if enable_mpr {
                self.viewports[vp_idx].mpr_enabled = true;
            }
        }
    }

    // ------------------------------------------------------------ viewports

    fn viewport_grid(&mut self, ui: &mut egui::Ui) {
        let full_rect = ui.available_rect_before_wrap();
        let rows = self.tiling.rows.max(1);
        let cols = self.tiling.columns.max(1);
        let cell_w = full_rect.width() / cols as f32;
        let cell_h = full_rect.height() / rows as f32;

        self.cursor_info = None;
        for r in 0..rows {
            for c in 0..cols {
                let vp_idx = r * cols + c;
                let min = pos2(
                    full_rect.min.x + c as f32 * cell_w,
                    full_rect.min.y + r as f32 * cell_h,
                );
                let max = pos2(min.x + cell_w, min.y + cell_h);
                let cell_rect = Rect::from_min_max(min, max).shrink(1.0);
                self.render_single_viewport(ui, vp_idx, cell_rect);
            }
        }
    }

    fn image_rect(&self, vp_idx: usize, view: Rect, f: &Frame) -> Rect {
        let vp = &self.viewports[vp_idx];
        let (rs, cs) = f.spacing.unwrap_or((1.0, 1.0));
        let m = rs.min(cs).max(1e-6);
        let mut w = f.width as f32 * (cs / m) as f32;
        let mut h = f.height as f32 * (rs / m) as f32;
        // Swap aspect ratio when rotated near 90Â° or 270Â°.
        let rot = vp.rotation.rem_euclid(180.0);
        if (45.0..135.0).contains(&rot) {
            std::mem::swap(&mut w, &mut h);
        }
        let fit = (view.width() / w).min(view.height() / h);
        Rect::from_center_size(view.center() + vp.pan, vec2(w, h) * fit * vp.zoom)
    }

    /// Maps a screen position inside `img` to unrotated image pixel coordinates `(0..width, 0..height)`.
    fn screen_to_pixel(&self, vp_idx: usize, img: Rect, f: &Frame, p: Pos2) -> Pos2 {
        let vp = &self.viewports[vp_idx];
        let rel_x = (p.x - img.min.x) / img.width().max(1.0);
        let rel_y = (p.y - img.min.y) / img.height().max(1.0);
        let uv_map = QuadUvMapping::from_orientation(vp.rotation, vp.x_flipped, vp.y_flipped);
        let tc_x = uv_map.origin[0] + rel_x * uv_map.u[0] + rel_y * uv_map.v[0];
        let tc_y = uv_map.origin[1] + rel_x * uv_map.u[1] + rel_y * uv_map.v[1];
        pos2(tc_x * f.width as f32, tc_y * f.height as f32)
    }

    /// Maps unrotated image pixel coordinates `(0..width, 0..height)` to screen coordinates inside `img`.
    fn pixel_to_screen(&self, vp_idx: usize, img: Rect, f: &Frame, p: Pos2) -> Pos2 {
        let vp = &self.viewports[vp_idx];
        let mut dx = (p.x / f.width.max(1) as f32) - 0.5;
        let mut dy = (p.y / f.height.max(1) as f32) - 0.5;
        if vp.x_flipped {
            dx = -dx;
        }
        if vp.y_flipped {
            dy = -dy;
        }
        let rad = (vp.rotation as f64) * std::f64::consts::PI / 180.0;
        let c = rad.cos() as f32;
        let s = rad.sin() as f32;
        let rx = dx * c - dy * s + 0.5;
        let ry = dx * s + dy * c + 0.5;
        pos2(
            img.min.x + rx * img.width(),
            img.min.y + ry * img.height(),
        )
    }

    fn viewport_scrubber_rects(view: Rect, slice_count: usize) -> Option<(Rect, Rect, Rect, Rect)> {
        if slice_count <= 1 || view.width() < 220.0 || view.height() < 140.0 {
            return None;
        }
        let bar_w = (view.width() * 0.44).clamp(160.0, 320.0);
        let bar_h = 22.0;
        let bar_rect = Rect::from_center_size(
            pos2(view.center().x, view.bottom() - 16.0),
            vec2(bar_w, bar_h),
        );
        let prev_rect = Rect::from_min_size(bar_rect.min + vec2(3.0, 2.0), vec2(20.0, bar_h - 4.0));
        let next_rect = Rect::from_min_size(
            pos2(bar_rect.max.x - 23.0, bar_rect.min.y + 2.0),
            vec2(20.0, bar_h - 4.0),
        );
        let track_rect = Rect::from_min_max(
            pos2(prev_rect.max.x + 6.0, bar_rect.min.y + 4.0),
            pos2(next_rect.min.x - 6.0, bar_rect.max.y - 4.0),
        );
        Some((bar_rect, prev_rect, track_rect, next_rect))
    }

    fn render_single_viewport(&mut self, ui: &mut egui::Ui, vp_idx: usize, view: Rect) {
        let response = ui.allocate_rect(view, Sense::click_and_drag());
        if (response.clicked() || response.drag_started() || response.secondary_clicked())
            && self.active_viewport != vp_idx {
                self.active_viewport = vp_idx;
                self.drawing.clear();
                self.angle_pending = None;
            }

        let painter = ui.painter_at(view);
        painter.rect_filled(view, 0.0, Color32::BLACK);

        if self.viewport_series(vp_idx).is_none() {
            let tr = crate::i18n::tr(self.lang);
            let text = if self.scan.is_some() {
                tr.indexing_short
            } else if vp_idx == 0 {
                tr.drop_hint
            } else {
                tr.assign_vp_hint
            };
            painter.text(
                view.center(),
                Align2::CENTER_CENTER,
                text,
                FontId::proportional(15.0),
                Color32::GRAY,
            );
            self.draw_viewport_border(&painter, vp_idx, view);
            return;
        }

        self.handle_wheel(ui, vp_idx, &response);

        let slice_count = self.viewport_slice_count(vp_idx);
        let scrubber = Self::viewport_scrubber_rects(view, slice_count);
        let press_origin = ui.input(|i| i.pointer.press_origin());
        let interact_pos = response.interact_pointer_pos();
        let on_scrubber = scrubber.is_some_and(|(bar, _, _, _)| {
            press_origin.is_some_and(|p| bar.contains(p))
                || (response.clicked() && interact_pos.is_some_and(|p| bar.contains(p)))
        });

        if on_scrubber
            && let Some((_, prev_r, track_r, next_r)) = scrubber {
                if response.clicked() {
                    if let Some(pos) = interact_pos {
                        if prev_r.contains(pos) {
                            self.step_viewport_slice(vp_idx, -1);
                        } else if next_r.contains(pos) {
                            self.step_viewport_slice(vp_idx, 1);
                        } else if track_r.expand(4.0).contains(pos) {
                            let t = ((pos.x - track_r.min.x) / track_r.width().max(1.0)).clamp(0.0, 1.0);
                            let idx = (t * (slice_count.saturating_sub(1)) as f32).round() as isize;
                            self.set_viewport_slice(vp_idx, idx, true);
                        }
                    }
                } else if (response.dragged_by(egui::PointerButton::Primary)
                    || response.drag_started())
                    && press_origin.is_some_and(|p| track_r.expand(6.0).contains(p))
                    && let Some(pos) = interact_pos {
                        let t = ((pos.x - track_r.min.x) / track_r.width().max(1.0)).clamp(0.0, 1.0);
                        let idx = (t * (slice_count.saturating_sub(1)) as f32).round() as isize;
                        self.set_viewport_slice(vp_idx, idx, true);
                    }
            }

        let mpr_pair = self.ensure_mpr_frame(vp_idx);
        let cur_slice = self.viewports[vp_idx].slice;
        let cur_key = mpr_pair
            .as_ref()
            .map(|(_, k)| *k)
            .or_else(|| self.viewport_slice_key(vp_idx, cur_slice));
        let frame_opt = mpr_pair
            .map(|(f, _)| f)
            .or_else(|| self.viewport_frame(vp_idx).cloned());
        let Some(mut frame) = frame_opt else {
            let text = match cur_key.and_then(|k| self.failed.get(&k)) {
                Some(err) => format!("Could not display this image:\n{err}"),
                None => "Loadingâ€¦".to_string(),
            };
            // Keep showing the previous image while the next slice decodes.
            if let Some((prev, key)) = self.viewports[vp_idx].last_gpu.clone() {
                let r = self.image_rect(vp_idx, view, &prev);
                let w = self.effective_window(vp_idx, &prev);
                painter.add(
                    self.gpu_callback(ui, vp_idx, prev.clone(), key, r, w)
                        .into_shape(view),
                );
            } else if let Some(tex) = &self.viewports[vp_idx].texture {
                painter.image(
                    tex.id(),
                    view,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::from_gray(160),
                );
            }
            painter.text(
                view.center(),
                Align2::CENTER_CENTER,
                text,
                FontId::proportional(15.0),
                Color32::LIGHT_GRAY,
            );
            if self.show_annotations {
                self.draw_overlay(&painter, vp_idx, view, None);
            }
            self.draw_scrubber_bar(ui, &painter, vp_idx, view);
            self.draw_viewport_border(&painter, vp_idx, view);
            return;
        };

        let mut cur_key = cur_key;
        if !on_scrubber {
            let consumed = self.handle_ortho_crosshair_drag(vp_idx, &response, view, &frame);
            if !consumed {
                self.handle_drag(vp_idx, &response, view, &frame);
            }
            if let Some((updated_f, updated_k)) = self.ensure_mpr_frame(vp_idx) {
                frame = updated_f;
                cur_key = Some(updated_k);
            }
        }

        // Preserve the unconvolved source frame for pixel value readout & ROI statistics
        // (matching IsiX DCMPix+DisplaySource: convolution is presentation-only).
        let source_frame = frame.clone();

        // Apply 2D Spatial Convolution Filter if active on this viewport (cached per slice & filter).
        if let (Some(conv_idx), Some(base_key)) = (self.viewports[vp_idx].convolution_idx, cur_key)
            && let Some(kernel_def) = FACTORY_CONVOLUTIONS.get(conv_idx) {
                let hit = self
                    .viewports[vp_idx]
                    .conv_cached
                    .as_ref()
                    .filter(|(k, c_idx, _, _)| *k == base_key && *c_idx == conv_idx)
                    .map(|(_, _, cf, ck)| (cf.clone(), *ck));
                if let Some((cf, ck)) = hit {
                    frame = cf;
                    cur_key = Some(ck);
                } else {
                    let conv = kernel_def.to_convolution();
                    let cf = Arc::new(frame.with_convolution(&conv));
                    let mut ck = base_key;
                    ck.slice ^= (conv_idx + 1) << 24;
                    self.viewports[vp_idx].conv_cached =
                        Some((base_key, conv_idx, cf.clone(), ck));
                    frame = cf;
                    cur_key = Some(ck);
                }
            }

        let window = self.effective_window(vp_idx, &frame);
        let img = self.image_rect(vp_idx, view, &frame);
        let on_gpu = self.gpu.is_some_and(|g| {
            matches!(frame.pixels, crate::frame::Pixels::Gray(_))
                && frame.width <= g.max_texture
                && frame.height <= g.max_texture
        });
        if let (true, Some(key)) = (on_gpu, cur_key) {
            painter.add(
                self.gpu_callback(ui, vp_idx, frame.clone(), key, img, window)
                    .into_shape(view),
            );
            self.viewports[vp_idx].last_gpu = Some((frame.clone(), key));
        } else if let Some(key) = cur_key {
            self.draw_on_cpu(ui, vp_idx, &painter, &frame, key, img, window);
        }

        // Live cursor readout with calibrated HU / pixel value and 3D patient coordinates.
        if let Some(p) = response.hover_pos() {
            let px = self.screen_to_pixel(vp_idx, img, &frame, p);
            if vp_idx == self.active_viewport {
                self.hover_px = Some(px);
            }
            if px.x >= 0.0 && px.y >= 0.0 {
                let (x, y) = (px.x as u32, px.y as u32);
                if let Some(val_str) = source_frame.value_at(x, y) {
                    let coords_3d = source_frame
                        .patient_coords_at(px.x as f64, px.y as f64)
                        .map(|[cx, cy, cz]| format!("   XYZ: ({cx:.1}, {cy:.1}, {cz:.1}) mm"))
                        .unwrap_or_default();
                    self.cursor_info =
                        Some(format!("X: {x}  Y: {y}   Val: {val_str}{coords_3d}"));
                }
            }
        } else if vp_idx == self.active_viewport {
            self.hover_px = None;
        }

        if self.ortho_mpr_active && vp_idx < 3 {
            self.draw_ortho_crosshairs(&painter, vp_idx, view, img, &frame);
        } else if self.show_reference_lines && self.tiling.cell_count() > 1 {
            self.draw_reference_lines(&painter, vp_idx, img, &frame);
        }
        if let Some((a_vp, anchor_pos, _)) = self.orbit_anchor
            && a_vp == vp_idx {
                let clipped = painter.with_clip_rect(view);
                let col = Color32::from_rgb(255, 200, 75);
                clipped.circle_stroke(anchor_pos, 10.0, Stroke::new(1.5, col));
                clipped.circle_filled(anchor_pos, 2.5, col);
                clipped.line_segment(
                    [pos2(anchor_pos.x - 14.0, anchor_pos.y), pos2(anchor_pos.x + 14.0, anchor_pos.y)],
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 200, 75, 160)),
                );
                clipped.line_segment(
                    [pos2(anchor_pos.x, anchor_pos.y - 14.0), pos2(anchor_pos.x, anchor_pos.y + 14.0)],
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 200, 75, 160)),
                );
                if let Some(cur) = response.interact_pointer_pos() {
                    clipped.line_segment(
                        [anchor_pos, cur],
                        Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 200, 75, 140)),
                    );
                }
            }

        let unverified_geom = self.viewports.get(vp_idx).is_some_and(|v| v.mpr_enabled)
            && self
                .viewport_volume(vp_idx)
                .is_some_and(|v| !v.geometry_verified);
        if unverified_geom {
            painter.text(
                pos2(view.center().x, view.max.y - 26.0),
                Align2::CENTER_BOTTOM,
                "Geometry not verified: measurements disabled",
                FontId::proportional(11.5),
                Color32::from_rgb(255, 195, 80),
            );
        }
        if self.tiling.cell_count() > 1 && !self.ortho_mpr_active {
            let leader_idx = self.active_viewport.min(self.tiling.cell_count() - 1);
            if let (Some(leader_sel), Some(cur_sel)) = (
                self.viewports[leader_idx].selected,
                self.viewports[vp_idx].selected,
            )
                && leader_sel != cur_sel {
                    let align_text = if self.registrations_in_flight.contains(&(leader_sel, cur_sel))
                    {
                        Some(("Aligningâ€¦".to_string(), Color32::from_rgb(130, 210, 255)))
                    } else if let Some(reg) = self.registrations.get(&(leader_sel, cur_sel)) {
                        let same_for = matches!(
                            (
                                self.volumes.get(&leader_sel).and_then(|v| v.frame_of_reference_uid.as_deref()),
                                self.volumes.get(&cur_sel).and_then(|v| v.frame_of_reference_uid.as_deref()),
                            ),
                            (Some(a), Some(b)) if !a.is_empty() && a == b
                        );
                        if same_for {
                            Some((
                                "Same Frame of Reference".to_string(),
                                Color32::from_rgb(130, 220, 155),
                            ))
                        } else if reg.is_reliable() {
                            Some((
                                format!("Auto-aligned (approximate) Â· score {:.2}", reg.correlation),
                                Color32::from_rgb(130, 220, 155),
                            ))
                        } else {
                            Some((
                                "Not aligned: synced by position".to_string(),
                                Color32::from_rgb(255, 200, 80),
                            ))
                        }
                    } else {
                        None
                    };
                    if let Some((txt, col)) = align_text {
                        painter.text(
                            pos2(view.center().x, view.min.y + 42.0),
                            Align2::CENTER_TOP,
                            txt,
                            FontId::proportional(11.0),
                            col,
                        );
                    }
                }
        }
        self.draw_measures(&painter, vp_idx, img, &source_frame);
        if self.show_annotations {
            self.draw_scale_bar(&painter, vp_idx, view, img, &frame);
            self.draw_orientation_letters(&painter, vp_idx, view, &frame);
            self.draw_overlay(&painter, vp_idx, view, Some(&frame));
        }
        self.draw_scrubber_bar(ui, &painter, vp_idx, view);
        self.draw_viewport_border(&painter, vp_idx, view);
    }

    fn ortho_crosshair_screen_pos(
        &self,
        vp_idx: usize,
        img: Rect,
        frame: &Frame,
    ) -> Option<Pos2> {
        let pt_lps = self.ortho_crosshair_lps?;
        let plane = self.viewports.get(vp_idx)?.mpr_plane.as_ref()?;
        let slice_center = plane.slice_center_lps();
        let rel = sub3(pt_lps, slice_center);
        let u_mm = dot3(rel, plane.u_axis);
        let v_mm = dot3(rel, plane.v_axis);
        let (sy, sx) = frame.spacing.unwrap_or((1.0, 1.0));
        let sx = sx.max(1e-6);
        let sy = sy.max(1e-6);
        let px = (frame.width.saturating_sub(1) as f64) * 0.5 + u_mm / sx;
        let py = (frame.height.saturating_sub(1) as f64) * 0.5 + v_mm / sy;
        Some(self.pixel_to_screen(vp_idx, img, frame, pos2(px as f32, py as f32)))
    }

    fn handle_ortho_crosshair_drag(
        &mut self,
        vp_idx: usize,
        response: &egui::Response,
        view: Rect,
        frame: &Frame,
    ) -> bool {
        if !self.ortho_mpr_active || vp_idx >= 3 {
            return false;
        }
        if response.drag_stopped() && self.crosshair_drag == Some(vp_idx) {
            self.crosshair_drag = None;
            return true;
        }
        let img = self.image_rect(vp_idx, view, frame);
        let cross = self.ortho_crosshair_screen_pos(vp_idx, img, frame);
        let pointer_pos = response.interact_pointer_pos();
        let shift_held = response.ctx.input(|i| i.modifiers.shift);

        let start_drag = response.drag_started_by(egui::PointerButton::Primary)
            && (shift_held
                || pointer_pos.is_some_and(|p| cross.is_some_and(|c| c.distance(p) <= 18.0)));
        if start_drag {
            self.crosshair_drag = Some(vp_idx);
        }

        let clicked_near = response.clicked_by(egui::PointerButton::Primary)
            && (shift_held
                || pointer_pos.is_some_and(|p| cross.is_some_and(|c| c.distance(p) <= 18.0)));

        if (self.crosshair_drag == Some(vp_idx) || clicked_near)
            && let Some(pos) = pointer_pos
                && let Some(plane) = self.viewports.get(vp_idx).and_then(|v| v.mpr_plane.clone()) {
                    let px = self.screen_to_pixel(vp_idx, img, frame, pos);
                    let (sy, sx) = frame.spacing.unwrap_or((1.0, 1.0));
                    let sx = sx.max(1e-6);
                    let sy = sy.max(1e-6);
                    let u_mm = (px.x as f64 - (frame.width.saturating_sub(1) as f64) * 0.5) * sx;
                    let v_mm = (px.y as f64 - (frame.height.saturating_sub(1) as f64) * 0.5) * sy;
                    let new_lps = add3(
                        plane.slice_center_lps(),
                        add3(scale3(plane.u_axis, u_mm), scale3(plane.v_axis, v_mm)),
                    );
                    self.set_ortho_crosshair_lps(new_lps);
                    return true;
                }
        false
    }

    fn draw_ortho_crosshairs(
        &self,
        painter: &egui::Painter,
        vp_idx: usize,
        view: Rect,
        img: Rect,
        frame: &Frame,
    ) {
        let Some(c) = self.ortho_crosshair_screen_pos(vp_idx, img, frame) else {
            return;
        };
        let clipped = painter.with_clip_rect(view);
        let (h_color, v_color, badge, badge_color) = match vp_idx {
            0 => (
                Color32::from_rgba_unmultiplied(74, 222, 128, 210), // Coronal (Green)
                Color32::from_rgba_unmultiplied(248, 113, 113, 210), // Sagittal (Red)
                "AXIAL",
                Color32::from_rgb(74, 168, 255),
            ),
            1 => (
                Color32::from_rgba_unmultiplied(74, 168, 255, 210), // Axial (Blue)
                Color32::from_rgba_unmultiplied(248, 113, 113, 210), // Sagittal (Red)
                "CORONAL",
                Color32::from_rgb(74, 222, 128),
            ),
            _ => (
                Color32::from_rgba_unmultiplied(74, 168, 255, 210), // Axial (Blue)
                Color32::from_rgba_unmultiplied(74, 222, 128, 210), // Coronal (Green)
                "SAGITTAL",
                Color32::from_rgb(248, 113, 113),
            ),
        };
        let gap = 10.0;
        // Horizontal line (with open center gap)
        if c.x - gap > view.min.x {
            clipped.line_segment(
                [pos2(view.min.x, c.y), pos2(c.x - gap, c.y)],
                Stroke::new(1.25, h_color),
            );
        }
        if c.x + gap < view.max.x {
            clipped.line_segment(
                [pos2(c.x + gap, c.y), pos2(view.max.x, c.y)],
                Stroke::new(1.25, h_color),
            );
        }
        // Vertical line (with open center gap)
        if c.y - gap > view.min.y {
            clipped.line_segment(
                [pos2(c.x, view.min.y), pos2(c.x, c.y - gap)],
                Stroke::new(1.25, v_color),
            );
        }
        if c.y + gap < view.max.y {
            clipped.line_segment(
                [pos2(c.x, c.y + gap), pos2(c.x, view.max.y)],
                Stroke::new(1.25, v_color),
            );
        }
        clipped.circle_stroke(c, 4.0, Stroke::new(1.25, badge_color));
        clipped.text(
            pos2(view.center().x, view.min.y + 26.0),
            Align2::CENTER_TOP,
            badge,
            FontId::proportional(12.0),
            badge_color,
        );
    }

    fn draw_scrubber_bar(
        &self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        vp_idx: usize,
        view: Rect,
    ) {
        let slice_count = self.viewport_slice_count(vp_idx);
        let Some((bar_rect, prev_rect, track_rect, next_rect)) =
            Self::viewport_scrubber_rects(view, slice_count)
        else {
            return;
        };
        let hover_pos = ui.input(|i| i.pointer.hover_pos());
        painter.rect_filled(
            bar_rect,
            11.0,
            Color32::from_rgba_unmultiplied(18, 22, 28, 210),
        );
        painter.rect_stroke(
            bar_rect,
            11.0,
            Stroke::new(1.0, Color32::from_gray(75)),
            egui::StrokeKind::Inside,
        );

        let prev_hover = hover_pos.is_some_and(|p| prev_rect.contains(p));
        let next_hover = hover_pos.is_some_and(|p| next_rect.contains(p));
        painter.text(
            prev_rect.center(),
            Align2::CENTER_CENTER,
            "â—€",
            FontId::proportional(11.0),
            if prev_hover {
                Color32::from_rgb(110, 190, 255)
            } else {
                Color32::from_gray(210)
            },
        );
        painter.text(
            next_rect.center(),
            Align2::CENTER_CENTER,
            "â–¶",
            FontId::proportional(11.0),
            if next_hover {
                Color32::from_rgb(110, 190, 255)
            } else {
                Color32::from_gray(210)
            },
        );

        let cy = track_rect.center().y;
        painter.line_segment(
            [pos2(track_rect.min.x, cy), pos2(track_rect.max.x, cy)],
            Stroke::new(3.0, Color32::from_gray(65)),
        );
        let t = (self.viewports[vp_idx].slice as f32
            / (slice_count.saturating_sub(1)).max(1) as f32)
            .clamp(0.0, 1.0);
        let thumb_x = track_rect.min.x + t * track_rect.width();
        painter.line_segment(
            [pos2(track_rect.min.x, cy), pos2(thumb_x, cy)],
            Stroke::new(3.0, Color32::from_rgb(70, 165, 255)),
        );
        painter.circle_filled(pos2(thumb_x, cy), 5.0, Color32::from_rgb(120, 195, 255));
        painter.circle_stroke(
            pos2(thumb_x, cy),
            5.0,
            Stroke::new(1.0, Color32::from_gray(20)),
        );
    }

    fn draw_viewport_border(&self, painter: &egui::Painter, vp_idx: usize, view: Rect) {
        if self.tiling.cell_count() > 1 {
            let is_active = vp_idx == self.active_viewport;
            let color = if is_active {
                Color32::from_rgb(70, 165, 255)
            } else {
                Color32::from_gray(55)
            };
            let width = if is_active { 1.5 } else { 1.0 };
            painter.rect_stroke(view, 0.0, Stroke::new(width, color), egui::StrokeKind::Inside);
        }
    }

    fn gpu_callback(
        &self,
        ui: &egui::Ui,
        vp_idx: usize,
        frame: Arc<Frame>,
        key: SliceKey,
        img: Rect,
        window: Window,
    ) -> ImageCallback {
        let ppp = ui.ctx().pixels_per_point();
        let vp = &self.viewports[vp_idx];
        let invert = vp.effective_invert() != frame.inverts;
        let clut = self.active_clut_table(vp_idx);
        let uv_map = QuadUvMapping::from_orientation(vp.rotation, vp.x_flipped, vp.y_flipped);
        ImageCallback {
            viewport_id: vp_idx,
            frame,
            key,
            image_rect_px: Rect::from_min_max(
                pos2(img.min.x * ppp, img.min.y * ppp),
                pos2(img.max.x * ppp, img.max.y * ppp),
            ),
            window,
            invert,
            clut,
            uv_map,
            bilinear: self.gpu_bilinear,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_on_cpu(
        &mut self,
        ui: &egui::Ui,
        vp_idx: usize,
        painter: &egui::Painter,
        frame: &Frame,
        slice_key: SliceKey,
        img: Rect,
        window: Window,
    ) {
        let invert = self.viewports[vp_idx].effective_invert();
        let clut_name = self.viewports[vp_idx]
            .active_clut_name()
            .map(str::to_owned);
        let convolution_idx = self.viewports[vp_idx].convolution_idx;
        let clut = self.active_clut_table(vp_idx);
        let key = RenderKey {
            key: slice_key,
            center: window.level.to_bits() as u64,
            width: window.width.to_bits() as u64,
            invert,
            clut_name,
            convolution_idx,
        };
        let vp = &mut self.viewports[vp_idx];
        if vp.rendered.as_ref() != Some(&key) || vp.texture.is_none() {
            let rgba = frame.render_with_clut(window, invert, clut.as_ref());
            let image = ColorImage::from_rgba_unmultiplied(
                [frame.width as usize, frame.height as usize],
                &rgba,
            );
            match &mut vp.texture {
                Some(t) => t.set(image, TextureOptions::LINEAR),
                None => {
                    let name = format!("slice-vp-{vp_idx}");
                    vp.texture = Some(ui.ctx().load_texture(name, image, TextureOptions::LINEAR));
                }
            }
            vp.rendered = Some(key);
        }
        if let Some(tex) = &vp.texture {
            painter.image(
                tex.id(),
                img,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        vp.last_gpu = None;
    }

    fn handle_wheel(&mut self, ui: &egui::Ui, vp_idx: usize, response: &egui::Response) {
        if !response.hovered() {
            return;
        }
        let events = ui.input(|i| i.events.clone());
        for event in events {
            if let egui::Event::MouseWheel {
                unit,
                delta,
                modifiers,
                ..
            } = event
            {
                let lines = match unit {
                    egui::MouseWheelUnit::Line => delta.y,
                    egui::MouseWheelUnit::Point => delta.y / 40.0,
                    egui::MouseWheelUnit::Page => delta.y * 10.0,
                };
                if modifiers.ctrl || modifiers.command {
                    let z = &mut self.viewports[vp_idx].zoom;
                    *z = (*z * (1.0 + 0.1 * lines)).clamp(0.05, 50.0);
                } else {
                    self.viewports[vp_idx].wheel_accum += lines;
                }
            }
        }
        let steps = self.viewports[vp_idx].wheel_accum.trunc();
        if steps != 0.0 {
            self.viewports[vp_idx].wheel_accum -= steps;
            self.active_viewport = vp_idx;
            self.step_viewport_slice(vp_idx, -steps as isize);
        }
    }

    fn handle_drag(
        &mut self,
        vp_idx: usize,
        response: &egui::Response,
        view: Rect,
        frame: &Frame,
    ) {
        if response.double_clicked() {
            if self.tool == Tool::Polygon && self.drawing.len() >= 3 {
                let pts = std::mem::take(&mut self.drawing);
                if let Some(k) = self.viewport_measure_key(vp_idx) {
                    self.measures
                        .entry(k.clone())
                        .or_default()
                        .push(Measure::Polygon(pts));
                    self.persist_measures_for(&k);
                }
            } else {
                self.viewports[vp_idx].reset_view();
                if self.should_sync_viewports() {
                    let count = self.tiling.cell_count();
                    for (i, vp) in self.viewports.iter_mut().enumerate().take(count) {
                        if i != vp_idx && vp.selected.is_some() {
                            vp.reset_view();
                        }
                    }
                }
            }
            return;
        }
        let delta = response.drag_delta();
        let img = self.image_rect(vp_idx, view, frame);
        let pointer_px = response
            .interact_pointer_pos()
            .map(|p| self.screen_to_pixel(vp_idx, img, frame, p));

        if response.dragged_by(egui::PointerButton::Middle) {
            self.viewports[vp_idx].pan += delta;
            self.sync_pan_from(vp_idx, delta);
            return;
        }
        if response.dragged_by(egui::PointerButton::Secondary) {
            let factor = (-delta.y / 200.0).exp();
            let z = &mut self.viewports[vp_idx].zoom;
            *z = (*z * factor).clamp(0.05, 50.0);
            self.sync_zoom_from(vp_idx, factor);
            return;
        }

        let is_drag =
            response.dragged_by(egui::PointerButton::Primary) || response.drag_stopped();
        let is_click = response.clicked();
        if !is_drag && !is_click {
            return;
        }

        match self.tool {
            Tool::WindowLevel => {
                if !is_drag {
                    return;
                }
                if response.drag_started()
                    || self.wl_drag.map(|(id, _, _)| id) != Some(vp_idx)
                {
                    let origin = response.ctx.input(|i| i.pointer.press_origin());
                    if let Some(start) = origin.or(response.interact_pointer_pos()) {
                        let from = self.effective_window(vp_idx, frame);
                        self.wl_drag =
                            Some((vp_idx, start, drag::Drag::new(drag::Variant::Image, from)));
                    }
                }
                if let (Some((id, start, d)), Some(now)) =
                    (self.wl_drag, response.interact_pointer_pos())
                    && id == vp_idx {
                        let prev_w = self.effective_window(vp_idx, frame);
                        let dx = (now.x - start.x) as f64;
                        let dy = (start.y - now.y) as f64;
                        let next_w = d.window(dx, dy, None, 0, frame.slope);
                        self.viewports[vp_idx].window = Some(next_w);
                        let d_level = next_w.level - prev_w.level;
                        let d_width = next_w.width - prev_w.width;
                        if d_level.abs() > 1e-6 || d_width.abs() > 1e-6 {
                            self.sync_window_delta(vp_idx, d_level, d_width);
                        }
                    }
                if response.drag_stopped() {
                    self.wl_drag = None;
                }
            }
            Tool::Scroll => {
                if !is_drag {
                    return;
                }
                if response.drag_started()
                    || self.scroll_drag.map(|(id, _, _)| id) != Some(vp_idx)
                {
                    let origin = response.ctx.input(|i| i.pointer.press_origin());
                    if let Some(start) = origin.or(response.interact_pointer_pos()) {
                        self.scroll_drag = Some((vp_idx, start.y, self.viewports[vp_idx].slice));
                    }
                }
                if let (Some((id, start_y, start_slice)), Some(now)) =
                    (self.scroll_drag, response.interact_pointer_pos())
                    && id == vp_idx {
                        let n = self.viewport_slice_count(vp_idx);
                        if n > 1 {
                            let px_per_slice = (view.height() * 0.75 / n as f32).clamp(2.5, 14.0);
                            let delta_slices = ((now.y - start_y) / px_per_slice).round() as isize;
                            self.set_viewport_slice(
                                vp_idx,
                                start_slice as isize + delta_slices,
                                true,
                            );
                        }
                    }
                if response.drag_stopped() {
                    self.scroll_drag = None;
                }
            }
            Tool::Pan => {
                if is_drag {
                    self.viewports[vp_idx].pan += delta;
                    self.sync_pan_from(vp_idx, delta);
                }
            }
            Tool::Zoom => {
                if is_drag {
                    let factor = (-delta.y / 200.0).exp();
                    let z = &mut self.viewports[vp_idx].zoom;
                    *z = (*z * factor).clamp(0.05, 50.0);
                    self.sync_zoom_from(vp_idx, factor);
                }
            }
            Tool::Rotate => {
                if !is_drag {
                    return;
                }
                let center = img.center();
                if response.drag_started()
                    || self.rot_drag.map(|(id, _, _)| id) != Some(vp_idx)
                {
                    let origin = response.ctx.input(|i| i.pointer.press_origin());
                    if let Some(start) = origin.or(response.interact_pointer_pos()) {
                        let a0 = (start.y - center.y).atan2(start.x - center.x).to_degrees();
                        self.rot_drag = Some((vp_idx, a0, self.viewports[vp_idx].rotation));
                    }
                }
                if let (Some((id, a0, init_rot)), Some(now)) =
                    (self.rot_drag, response.interact_pointer_pos())
                    && id == vp_idx {
                        let a1 = (now.y - center.y).atan2(now.x - center.x).to_degrees();
                        self.viewports[vp_idx].rotation =
                            (init_rot + (a1 - a0)).rem_euclid(360.0);
                    }
                if response.drag_stopped() {
                    self.rot_drag = None;
                }
            }
            Tool::Orbit3D => {
                if !is_drag {
                    return;
                }
                if response.drag_started()
                    && let Some(mut plane) = self.effective_mpr_plane(vp_idx) {
                        let press_pos = response
                            .ctx
                            .input(|i| i.pointer.press_origin())
                            .or(response.interact_pointer_pos())
                            .unwrap_or_else(|| view.center());
                        let px = self.screen_to_pixel(vp_idx, img, frame, press_pos);
                        let (sy, sx) = frame.spacing.unwrap_or((1.0, 1.0));
                        let u_mm = (px.x as f64 - (frame.width.saturating_sub(1) as f64) * 0.5)
                            * sx.max(1e-6);
                        let v_mm = (px.y as f64 - (frame.height.saturating_sub(1) as f64) * 0.5)
                            * sy.max(1e-6);
                        let anchor_lps = add3(
                            plane.slice_center_lps(),
                            add3(scale3(plane.u_axis, u_mm), scale3(plane.v_axis, v_mm)),
                        );
                        self.orbit_anchor = Some((vp_idx, press_pos, anchor_lps));
                        plane.offset_mm = dot3(sub3(anchor_lps, plane.center_lps), plane.normal);
                        self.set_viewport_mpr_plane(vp_idx, plane, true);
                    }
                if delta.length_sq() > 0.0
                    && let Some(mut plane) = self.effective_mpr_plane(vp_idx) {
                        let (shift, alt) = response
                            .ctx
                            .input(|i| (i.modifiers.shift, i.modifiers.alt));
                        let speed = if alt { 0.08 } else { 0.18 };
                        if shift {
                            plane.orbit_3d(0.0, 0.0, delta.x as f64 * speed);
                        } else {
                            plane.orbit_3d(delta.x as f64 * speed, -delta.y as f64 * speed, 0.0);
                        }
                        if let Some((a_vp, _, anchor_lps)) = self.orbit_anchor
                            && a_vp == vp_idx {
                                plane.offset_mm =
                                    dot3(sub3(anchor_lps, plane.center_lps), plane.normal);
                            }
                        self.set_viewport_mpr_plane(vp_idx, plane, true);
                    }
                if response.drag_stopped() {
                    self.orbit_drag = None;
                    self.orbit_anchor = None;
                }
            }
            Tool::Length => {
                let cur_px = pointer_px.or(self.hover_px);
                if self.drawing.is_empty() {
                    if response.drag_started() {
                        let origin = response.ctx.input(|i| i.pointer.press_origin());
                        if let Some(p) = origin.or(response.interact_pointer_pos()) {
                            let px = self.screen_to_pixel(vp_idx, img, frame, p);
                            self.drawing.push(px);
                            self.drawing.push(px);
                        }
                    } else if is_click
                        && let Some(px) = cur_px {
                            self.drawing.push(px);
                        }
                } else if self.drawing.len() == 1 {
                    if (is_click || response.drag_stopped())
                        && let Some(b) = cur_px {
                            let a = self.drawing[0];
                            if a.distance(b) > 2.0 {
                                if let Some(k) = self.viewport_measure_key(vp_idx) {
                                    self.measures
                                        .entry(k.clone())
                                        .or_default()
                                        .push(Measure::Length(a, b));
                                    self.persist_measures_for(&k);
                                }
                                self.drawing.clear();
                            }
                        }
                } else if self.drawing.len() == 2 {
                    if let Some(p) = cur_px {
                        self.drawing[1] = p;
                    }
                    if response.drag_stopped() {
                        let (a, b) = (self.drawing[0], self.drawing[1]);
                        if a.distance(b) > 3.0 {
                            if let Some(k) = self.viewport_measure_key(vp_idx) {
                                self.measures
                                    .entry(k.clone())
                                    .or_default()
                                    .push(Measure::Length(a, b));
                                self.persist_measures_for(&k);
                            }
                            self.drawing.clear();
                        } else {
                            // Tiny movement on first click: keep start point and wait for second click.
                            self.drawing.truncate(1);
                        }
                    }
                }
            }
            Tool::Angle => {
                let cur_px = pointer_px.or(self.hover_px);
                // Step 2: First arm (a -> v) is already placed in `angle_pending`.
                // Commit the angle on either click or drag-release of the third point `b`.
                if let Some((a, v)) = self.angle_pending {
                    if (is_click || response.drag_stopped())
                        && let Some(b) = cur_px
                            && v.distance(b) > 2.0 {
                                if let Some(k) = self.viewport_measure_key(vp_idx) {
                                    self.measures
                                        .entry(k.clone())
                                        .or_default()
                                        .push(Measure::Angle(a, v, b));
                                    self.persist_measures_for(&k);
                                }
                                self.angle_pending = None;
                                self.drawing.clear();
                            }
                    return;
                }

                // Step 1: Placing the first arm (a -> v) via either 2 clicks or click-and-drag.
                if self.drawing.is_empty() {
                    if response.drag_started() {
                        let origin = response.ctx.input(|i| i.pointer.press_origin());
                        if let Some(p) = origin.or(response.interact_pointer_pos()) {
                            let px = self.screen_to_pixel(vp_idx, img, frame, p);
                            self.drawing.push(px);
                            self.drawing.push(px);
                        }
                    } else if is_click
                        && let Some(px) = cur_px {
                            self.drawing.push(px);
                        }
                } else if self.drawing.len() == 1 {
                    if (is_click || response.drag_stopped())
                        && let Some(v) = cur_px {
                            let a = self.drawing[0];
                            if a.distance(v) > 2.0 {
                                self.angle_pending = Some((a, v));
                                self.drawing.clear();
                            }
                        }
                } else if self.drawing.len() == 2 {
                    if let Some(p) = cur_px {
                        self.drawing[1] = p;
                    }
                    if response.drag_stopped() {
                        let (a, v) = (self.drawing[0], self.drawing[1]);
                        if a.distance(v) > 3.0 {
                            self.angle_pending = Some((a, v));
                            self.drawing.clear();
                        } else {
                            // Tiny movement on first click: keep start point `a` and wait for click at vertex `v`.
                            self.drawing.truncate(1);
                        }
                    }
                }
            }
            Tool::Oval | Tool::Rect => {
                let is_oval = self.tool == Tool::Oval;
                let cur_px = pointer_px.or(self.hover_px);
                if self.drawing.is_empty() {
                    if response.drag_started() {
                        let origin = response.ctx.input(|i| i.pointer.press_origin());
                        if let Some(p) = origin.or(response.interact_pointer_pos()) {
                            let px = self.screen_to_pixel(vp_idx, img, frame, p);
                            self.drawing.push(px);
                            self.drawing.push(px);
                        }
                    } else if is_click
                        && let Some(px) = cur_px {
                            self.drawing.push(px);
                        }
                } else if self.drawing.len() == 1 {
                    if (is_click || response.drag_stopped())
                        && let Some(b) = cur_px {
                            let a = self.drawing[0];
                            if a.distance(b) > 2.5 {
                                if let Some(k) = self.viewport_measure_key(vp_idx) {
                                    let m = if is_oval {
                                        Measure::Oval(a, b)
                                    } else {
                                        Measure::Rect(a, b)
                                    };
                                    self.measures.entry(k.clone()).or_default().push(m);
                                    self.persist_measures_for(&k);
                                }
                                self.drawing.clear();
                            }
                        }
                } else if self.drawing.len() == 2 {
                    if let Some(p) = cur_px {
                        self.drawing[1] = p;
                    }
                    if response.drag_stopped() {
                        let (a, b) = (self.drawing[0], self.drawing[1]);
                        if a.distance(b) > 3.0 {
                            if let Some(k) = self.viewport_measure_key(vp_idx) {
                                let m = if is_oval {
                                    Measure::Oval(a, b)
                                } else {
                                    Measure::Rect(a, b)
                                };
                                self.measures.entry(k.clone()).or_default().push(m);
                                self.persist_measures_for(&k);
                            }
                            self.drawing.clear();
                        } else {
                            self.drawing.truncate(1);
                        }
                    }
                }
            }
            Tool::Polygon => {
                let cur_px = pointer_px.or(self.hover_px);
                if is_click
                    && let Some(px) = cur_px {
                        // Close polygon if clicking near the first vertex and we already have >= 3 points.
                        if self.drawing.len() >= 3 {
                            let first_scr = self.pixel_to_screen(vp_idx, img, frame, self.drawing[0]);
                            let cur_scr = self.pixel_to_screen(vp_idx, img, frame, px);
                            if first_scr.distance(cur_scr) <= 10.0 {
                                let pts = std::mem::take(&mut self.drawing);
                                if let Some(k) = self.viewport_measure_key(vp_idx) {
                                    self.measures
                                        .entry(k.clone())
                                        .or_default()
                                        .push(Measure::Polygon(pts));
                                    self.persist_measures_for(&k);
                                }
                                return;
                            }
                        }
                        self.drawing.push(px);
                    }
            }
            Tool::Pencil => {
                let cur_px = pointer_px.or(self.hover_px);
                if response.drag_started() {
                    self.drawing.clear();
                    let origin = response.ctx.input(|i| i.pointer.press_origin());
                    if let Some(p) = origin.or(response.interact_pointer_pos()) {
                        self.drawing.push(self.screen_to_pixel(vp_idx, img, frame, p));
                    }
                }
                if response.dragged_by(egui::PointerButton::Primary)
                    && let Some(px) = cur_px {
                        let add = self
                            .drawing
                            .last()
                            .is_none_or(|last| last.distance(px) >= 1.5);
                        if add {
                            self.drawing.push(px);
                        }
                    }
                if response.drag_stopped() {
                    if self.drawing.len() >= 3 {
                        let pts = std::mem::take(&mut self.drawing);
                        if let Some(k) = self.viewport_measure_key(vp_idx) {
                            self.measures
                                .entry(k.clone())
                                .or_default()
                                .push(Measure::Pencil(pts));
                            self.persist_measures_for(&k);
                        }
                    } else {
                        self.drawing.clear();
                    }
                }
            }
            Tool::Point => {
                let cur_px = pointer_px.or(self.hover_px);
                if (is_click || response.drag_stopped())
                    && let Some(px) = cur_px
                        && let Some(k) = self.viewport_measure_key(vp_idx) {
                            self.measures
                                .entry(k.clone())
                                .or_default()
                                .push(Measure::Point(px));
                            self.persist_measures_for(&k);
                        }
            }
        }
    }

    /// Requests 3D MPR (`ortho = false`) or 3-pane Orthogonal MPR (`ortho = true`) on `vp_idx`.
    /// Normally 3D-capable series activate immediately (or as soon as decoding completes).
    /// Non-3D-capable series (too few slices or missing 3D geometry) prompt a Yes/No quality warning first.
    pub(crate) fn request_mpr_mode(&mut self, vp_idx: usize, ortho: bool) {
        let Some(key) = self.viewports[vp_idx].selected else {
            return;
        };
        let is_capable = self.series_at(key).is_some_and(|s| s.is_mpr_capable())
            && !self.volume_errors.contains_key(&key);
        let already_forced = self.forced_mpr_series.contains(&key);
        let has_vol = self.viewport_volume(vp_idx).is_some();

        if is_capable || already_forced || has_vol {
            if has_vol {
                self.active_viewport = vp_idx;
                if ortho {
                    if !self.ortho_mpr_active {
                        self.toggle_orthogonal_mpr();
                    }
                } else if let Some(p) = self.effective_mpr_plane(vp_idx) {
                    self.set_viewport_mpr_plane(vp_idx, p, true);
                }
            } else {
                self.pending_mpr_activate = Some((vp_idx, key, ortho));
            }
        } else {
            self.pending_force_mpr = Some(PendingForceMpr { vp_idx, key, ortho });
        }
    }

}

impl eframe::App for ViewerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_background(&ctx);
        self.handle_dropped_files(&ctx);
        self.handle_keys(&ctx);

        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        if self.show_browser {
            egui::Panel::left("browser")
                .resizable(true)
                .default_size(300.0)
                .show(ui, |ui| self.browser(ui));
        }
        egui::CentralPanel::no_frame().show(ui, |ui| self.viewport_grid(ui));
        self.draw_force_mpr_modal(&ctx);
        self.draw_update_modal(&ctx);
        self.draw_pacs_modal(&ctx);
        self.draw_settings_modal(&ctx);
        self.draw_crash_modal(&ctx);
        self.draw_first_run_notice_modal(&ctx);
    }
}

impl Drop for ViewerApp {
    fn drop(&mut self) {
        dicom_index::cleanup_zip_cache();
        crate::crash_log::cleanup_on_clean_exit(self.previous_crash_pending);
    }
}
