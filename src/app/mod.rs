//! The viewer window: study/series browser on the left, Horos-style toolbar at
//! the top, and multi-series comparison viewports (`1×1` through `3×3`) with
//! synchronized scrolling, 3D cross-reference/scout lines, CLUTs, and
//! orientation edge markers in the center.

mod browser;
mod metadata;
mod modals;
mod mpr_sync;
mod overlays;
mod viewport;

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
use crate::gpu_view::{self, GpuInfo, ImageCallback, QuadUvMapping, VolumeMprCallback};
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
    pub(crate) link_group: u8,
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
            link_group: 0,
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
    pub(crate) link_zoom: bool,
    pub(crate) link_wl: bool,
    pub(crate) link_cut: bool,
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
    pub(crate) show_database_view: bool,
    pub(crate) selected_db_study: Option<usize>,
    pub(crate) show_metadata_inspector: bool,
    pub(crate) metadata_filter: String,
    pub(crate) metadata_cache: Option<metadata::MetadataCache>,
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
            link_zoom: false,
            link_wl: false,
            link_cut: false,
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
            show_database_view: false,
            selected_db_study: None,
            show_metadata_inspector: false,
            metadata_filter: String::new(),
            metadata_cache: None,
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
        if let Some(db) = &app.db
            && let Ok(saved) = db.load_index()
        {
            let has_studies = !saved.studies.is_empty();
            app.index = Some(saved);
            if initial.is_empty() && has_studies {
                app.selected_db_study = Some(0);
                app.assign_series_to_viewport(0, Some((0, 0)));
                app.populate_unassigned_viewports();
            } else if initial.is_empty() {
                app.show_database_view = true;
                let studies_dir = crate::db::managed_studies_dir();
                if studies_dir.is_dir()
                    && std::fs::read_dir(&studies_dir)
                        .ok()
                        .and_then(|mut it| it.next())
                        .is_some()
                {
                    app.start_scan(vec![studies_dir]);
                }
            }
        }
        if !initial.is_empty() {
            app.start_scan(initial);
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
        if self.ortho_mpr_active || self.link_wl {
            let count = self.tiling.cell_count();
            for i in 0..count {
                if self.are_viewports_linked(leader_idx, i) && self.viewports[i].selected.is_some() {
                    self.viewports[i].window = w;
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
                            let (tw, th, rgba) = frame.render_thumbnail(128);
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
                    format!("● {name}")
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
                        .text(format!("{} {dl}…", tr.pacs_msg_starting_dl)),
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
                    "{} {} · {} {} · {} {} · {} {}",
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
                        crate::i18n::Lang::Es => "⚠ Registro de Fallo",
                        crate::i18n::Lang::En => "⚠ Crash Log",
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


    // ------------------------------------------------------------ viewports


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
        if self.show_database_view {
            egui::CentralPanel::default().show(ui, |ui| self.database_view(ui));
        } else {
            if self.show_browser {
                egui::Panel::left("browser")
                    .resizable(true)
                    .default_size(290.0)
                    .min_size(220.0)
                    .show(ui, |ui| self.browser(ui));
            }
            if self.show_metadata_inspector {
                egui::Panel::right("metadata_inspector")
                    .resizable(true)
                    .default_size(430.0)
                    .min_size(300.0)
                    .show(ui, |ui| self.metadata_inspector_panel(ui));
            }
            egui::CentralPanel::no_frame().show(ui, |ui| self.viewport_grid(ui));
        }
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
