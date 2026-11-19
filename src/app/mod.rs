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
use crate::gpu_view::{self, GpuInfo, ImageCallback, QuadUvMapping, VolumeMprCallback, VolumeVrCallback};
use crate::imaging::clut::{ColorLookupTable, ColourTableBook};
use crate::imaging::convolution::FACTORY_CONVOLUTIONS;
use crate::imaging::vr_presets::{TfStop, VrPreset};
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
    CobbAngle,
    Arrow,
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
    /// Cobb angle between two line segments `(a1, b1)` and `(a2, b2)` in image pixel coordinates.
    Cobb(Pos2, Pos2, Pos2, Pos2),
    /// Arrow & text callout annotation (`tip` -> `tail`) in image pixel coordinates.
    Arrow { tip: Pos2, tail: Pos2, text: String },
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
    /// True when the shape has collapsed to nothing measurable: an oval or rectangle
    /// less than one pixel wide or high, a line shorter than one pixel, or a
    /// polygon with no area. Such a shape is never kept.
    pub(crate) fn is_degenerate(&self) -> bool {
        let tiny = |a: Pos2, b: Pos2| a.distance(b) < 1.0;
        match self {
            Measure::Length(a, b) => tiny(*a, *b),
            Measure::Arrow { tip, tail, .. } => tiny(*tip, *tail),
            Measure::Oval(a, b) | Measure::Rect(a, b) => (a.x - b.x).abs() < 1.0 || (a.y - b.y).abs() < 1.0,
            Measure::Angle(a, v, b) => tiny(*a, *v) || tiny(*v, *b),
            Measure::Cobb(a1, b1, a2, b2) => tiny(*a1, *b1) || tiny(*a2, *b2),
            Measure::Polygon(p) | Measure::Pencil(p) => {
                p.len() < 3 || {
                    let twice_area: f32 = (0..p.len())
                        .map(|i| {
                            let (u, v) = (p[i], p[(i + 1) % p.len()]);
                            u.x * v.y - v.x * u.y
                        })
                        .sum();
                    twice_area.abs() < 2.0
                }
            }
            Measure::Point(_) => false,
        }
    }

    pub(crate) fn to_persisted(&self) -> crate::db::PersistedMeasure {
        match self {
            Self::Length(a, b) => crate::db::PersistedMeasure::Length([a.x, a.y], [b.x, b.y]),
            Self::Angle(a, v, b) => {
                crate::db::PersistedMeasure::Angle([a.x, a.y], [v.x, v.y], [b.x, b.y])
            }
            Self::Cobb(a1, b1, a2, b2) => crate::db::PersistedMeasure::Cobb(
                [a1.x, a1.y],
                [b1.x, b1.y],
                [a2.x, a2.y],
                [b2.x, b2.y],
            ),
            Self::Arrow { tip, tail, text } => crate::db::PersistedMeasure::Arrow {
                tip: [tip.x, tip.y],
                tail: [tail.x, tail.y],
                text: text.clone(),
            },
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
            crate::db::PersistedMeasure::Cobb(a1, b1, a2, b2) => Self::Cobb(
                pos2(a1[0], a1[1]),
                pos2(b1[0], b1[1]),
                pos2(a2[0], a2[1]),
                pos2(b2[0], b2[1]),
            ),
            crate::db::PersistedMeasure::Arrow { tip, tail, text } => Self::Arrow {
                tip: pos2(tip[0], tip[1]),
                tail: pos2(tail[0], tail[1]),
                text,
            },
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

/// Per-viewport PET-CT / multi-modality 2D fusion overlay state.
pub(crate) struct FusionOverlayState {
    pub(crate) enabled: bool,
    /// `(study_idx, series_idx)` of the overlaid functional/secondary series.
    pub(crate) series: Option<(usize, usize)>,
    /// IsiX `FusionBlend` (alpha ramp mode `0..=5` and factor `-256..=256`).
    pub(crate) blend: crate::imaging::fusion::FusionBlend,
    /// Name of the CLUT in `ColourTableBook` (defaults to `"PET"`).
    pub(crate) clut_name: String,
    /// Optional window override for the fused series (`None` = frame default).
    pub(crate) window: Option<Window>,
    pub(crate) texture: Option<TextureHandle>,
    pub(crate) tex_key: Option<(SliceKey, i32, i32, String, u32, u32)>,
}

impl Default for FusionOverlayState {
    fn default() -> Self {
        Self {
            enabled: false,
            series: None,
            blend: crate::imaging::fusion::FusionBlend {
                mode: 0,
                factor: 0.0,
                factor_at_drag_start: 0.0,
            },
            clut_name: "PET".to_string(),
            window: None,
            texture: None,
            tex_key: None,
        }
    }
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
    pub(crate) vr_enabled: bool,
    pub(crate) vr_yaw: f32,
    pub(crate) vr_pitch: f32,
    pub(crate) vr_mip: bool,
    pub(crate) vr_preset: VrPreset,
    pub(crate) vr_custom_stops: Vec<TfStop>,
    pub(crate) vr_shading: bool,
    pub(crate) vr_ambient: f32,
    pub(crate) vr_diffuse: f32,
    pub(crate) vr_specular: f32,
    pub(crate) vr_clip_min: [f32; 3],
    pub(crate) vr_clip_max: [f32; 3],
    pub(crate) vr_bg_color: [f32; 3],
    pub(crate) link_group: u8,
    pub(crate) fusion: FusionOverlayState,
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
            vr_enabled: false,
            vr_yaw: 0.55,
            vr_pitch: -0.25,
            vr_mip: false,
            vr_preset: VrPreset::Default,
            vr_custom_stops: VrPreset::Default.default_stops(),
            vr_shading: true,
            vr_ambient: 0.28,
            vr_diffuse: 0.72,
            vr_specular: 0.35,
            vr_clip_min: [0.0, 0.0, 0.0],
            vr_clip_max: [1.0, 1.0, 1.0],
            vr_bg_color: [0.02, 0.02, 0.03],
            link_group: 0,
            fusion: FusionOverlayState::default(),
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
        self.vr_yaw = 0.55;
        self.vr_pitch = -0.25;
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
        self.fusion = FusionOverlayState::default();
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
    pub(crate) maximized_viewport: Option<usize>,
    pub(crate) viewports: Vec<ViewportCell>,
    pub(crate) active_viewport: usize,
    pub(crate) sync_mode: SliceSynchronization,
    pub(crate) link_zoom: bool,
    pub(crate) link_zoom_same_size: bool,
    pub(crate) link_wl: bool,
    pub(crate) link_cut: bool,
    pub(crate) compare_link_wl: bool,
    pub(crate) compare_vp_selected: Vec<bool>,
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
    /// Pairs aligned by hand with Lock 3D Sync; a background registration never replaces them.
    pub(crate) locked_pairs: std::collections::HashSet<SeriesPair>,
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
    pub(crate) roi_callout_offsets: HashMap<(PathBuf, u32, usize), Vec2>,
    pub(crate) callout_drag: Option<((PathBuf, u32, usize), Pos2)>,
    pub(crate) callout_hit_rects: Vec<((PathBuf, u32, usize), Rect)>,
    pub(crate) prev_callout_hit_rects: Vec<((PathBuf, u32, usize), Rect)>,
    pub(crate) key_images: std::collections::HashSet<(PathBuf, u32)>,
    pub(crate) key_images_only: bool,
    pub(crate) series_spacing_override: HashMap<(usize, usize), (f64, f64)>,
    pub(crate) show_calibrate_modal: bool,
    pub(crate) pending_remove_study: Option<(String, String)>,
    pub(crate) pending_clear_library: bool,
    pub(crate) calibrate_known_mm: String,
    pub(crate) calibrate_last_len_px: f64,
    pub(crate) drawing: Vec<Pos2>,
    pub(crate) angle_pending: Option<(Pos2, Pos2)>,
    pub(crate) cobb_pending: Option<(Pos2, Pos2)>,
    pub(crate) selected_roi: Option<(PathBuf, u32, usize)>,
    pub(crate) dragging_roi_handle: Option<(PathBuf, u32, usize, usize)>,
    /// The ROI as it was when a handle drag began, restored if the drag would leave it empty.
    pub(crate) roi_handle_original: Option<Measure>,
    pub(crate) dragging_roi_body: Option<(PathBuf, u32, usize, Pos2)>,
    pub(crate) show_roi_histogram: bool,
    pub(crate) arrow_edit_target: Option<(PathBuf, u32, usize)>,
    pub(crate) arrow_edit_text: String,
    pub(crate) hover_px: Option<Pos2>,
    pub(crate) tiling_hover: Option<(usize, usize)>,

    pub(crate) playing: bool,
    pub(crate) fps: f32,
    last_tick: f64,
    cursor_info: Option<String>,

    pub(crate) show_browser: bool,
    pub(crate) sidebar_compact: bool,
    pub(crate) show_database_view: bool,
    copy_shot: CopyShotState,
    copy_shot_tx: std::sync::mpsc::Sender<Arc<egui::ColorImage>>,
    copy_shot_rx: Receiver<Arc<egui::ColorImage>>,
    pub(crate) copy_anonymize_frame: bool,
    pub(crate) viewport_rects: Vec<Rect>,
    /// Millimetres per screen pixel of each viewport's displayed image at zoom 1,
    /// measured when it is drawn (the resliced image of a turned MPR plane differs
    /// in size from the source slices). Used by the "= mm" zoom link.
    pub(crate) viewport_mm_per_px: Vec<Option<f32>>,
    /// "= mm" zoom to match again from this viewport once every linked viewport has
    /// been drawn with its current image (a turned plane changes the image size).
    pub(crate) pending_zoom_match: Option<usize>,
    /// Ctrl (Cmd on Mac) held this frame: changes apply to the touched viewport only.
    pub(crate) solo_adjust: bool,
    /// A viewport moved or turned alone while Cut was linked; its new relation to
    /// the others is locked once Ctrl/Cmd is released.
    pub(crate) pending_relock: Option<usize>,
    /// The viewport a comparison started from (link switched on there, Compare, or
    /// Lock). Automatic alignment turns the other viewports to match it.
    pub(crate) link_master: Option<usize>,
    pub(crate) show_vr_tf_editor: bool,
    pub(crate) vr_tf_selected_stop: usize,
    pub(crate) vr_histogram_cache: Option<(u64, u32, u32, Vec<f32>)>,
    pub(crate) selected_db_study: Option<usize>,
    /// Library table sort: column index (0 = patient name … 6 = patient ID) and direction.
    pub(crate) db_sort: (usize, bool),
    pub(crate) show_metadata_inspector: bool,
    pub(crate) metadata_filter: String,
    pub(crate) metadata_cache: Option<metadata::MetadataCache>,
    pub(crate) ortho_mpr_active: bool,
    pub(crate) ortho_crosshair_lps: Option<[f64; 3]>,
    ortho_saved_tiling: Option<ViewportTiling>,
    crosshair_drag: Option<usize>,
    crosshair_tilt_drag: Option<(usize, f32)>,
    forced_mpr_series: std::collections::HashSet<(usize, usize)>,
    pending_force_mpr: Option<PendingForceMpr>,
    pub(crate) pending_mpr_activate: Option<(usize, (usize, usize), bool)>,
    pub(crate) update_status: crate::updater::UpdateStatus,
    update_rx: Option<Receiver<anyhow::Result<Option<crate::updater::ReleaseInfo>>>>,
    install_rx: Option<Receiver<anyhow::Result<String>>>,
    pub(crate) show_update_modal: bool,
    pub(crate) db: Option<crate::db::Database>,
    pub(crate) lang: crate::i18n::Lang,
    pub(crate) theme_mode: ThemeMode,
    pub(crate) blend_siblings: bool,
    pub(crate) gpu_bilinear: bool,
    check_updates_on_startup: bool,
    crash_report_opt_in: bool,
    pub(crate) copy_policy: crate::db::CopyToLibraryPolicy,
    pending_copy_prompt: bool,
    pub(crate) show_settings_modal: bool,
    study_storage_zip: bool,
    pub(crate) previous_crash_pending: bool,
    show_crash_modal: bool,
    show_first_run_notice: bool,
    confirm_delete_local_data: bool,
    patient_links: HashMap<String, String>,
    linking_source: Option<(String, Vec<String>)>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum ThemeMode {
    #[default]
    Dark,
    Oled,
    Light,
    System,
}

impl ThemeMode {
    pub(crate) fn from_str(s: &str) -> Self {
        match s {
            "oled" => Self::Oled,
            "light" => Self::Light,
            "system" => Self::System,
            _ => Self::Dark,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Oled => "oled",
            Self::Light => "light",
            Self::System => "system",
        }
    }

    pub(crate) fn apply(self, ctx: &egui::Context) {
        crate::ui::theme::set_oled(ctx, self == Self::Oled);
        let pref = match self {
            Self::Dark | Self::Oled => egui::ThemePreference::Dark,
            Self::Light => egui::ThemePreference::Light,
            Self::System => egui::ThemePreference::System,
        };
        ctx.set_theme(pref);
    }
}

#[derive(Clone, Copy)]
enum CopyShotState {
    Idle,
    Armed(Rect),
    WaitingForImage(Rect),
}

#[derive(Clone, Copy)]
struct PendingForceMpr {
    vp_idx: usize,
    key: (usize, usize),
    ortho: bool,
}

impl ViewerApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Vec<PathBuf>) -> Self {
        crate::ui::theme::install(&cc.egui_ctx);
        let ctx = cc.egui_ctx.clone();
        dicom_index::cleanup_stale_zip_caches();
        let viewports = (0..MAX_VIEWPORTS).map(ViewportCell::new).collect();
        let db = crate::db::Database::open_default().ok();
        if let Some(ref d) = db {
            crate::legacy_cleanup::remove_web_portal_data(d);
            let accent = d
                .get_setting("accent_color")
                .and_then(|h| crate::ui::theme::from_hex(&h))
                .unwrap_or(crate::ui::theme::DEFAULT_ACCENT);
            let tint = d.get_setting("tint_chrome").as_deref() == Some("1");
            crate::ui::theme::set_accent(&cc.egui_ctx, accent, tint);
        }
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
        let saved_custom_studies = settings
            .as_ref()
            .and_then(|s| s.custom_studies_dir());
        crate::db::set_custom_studies_dir(saved_custom_studies);
        let saved_storage_zip = settings
            .as_ref()
            .map(|s| s.study_storage_zip())
            .unwrap_or(false);
        // "column,asc" e.g. "5,0" = by date, newest first. Default: patient name A→Z, as Horos.
        let saved_db_sort = db
            .as_ref()
            .and_then(|d| d.get_setting("db_sort"))
            .and_then(|s| {
                let (c, a) = s.split_once(',')?;
                Some((c.parse::<usize>().ok().filter(|c| *c < 7)?, a == "1"))
            })
            .unwrap_or((0, true));
        let first_run_ack = settings
            .as_ref()
            .map(|s| s.first_run_notice_ack())
            .unwrap_or(false);
        let saved_patient_links: HashMap<String, String> = settings
            .as_ref()
            .map(|s| s.patient_links())
            .unwrap_or_default();
        let saved_theme_mode = db
            .as_ref()
            .and_then(|d| d.get_setting("theme_mode"))
            .map(|s| ThemeMode::from_str(&s))
            .unwrap_or(ThemeMode::Dark);
        saved_theme_mode.apply(&cc.egui_ctx);
        let (copy_shot_tx, copy_shot_rx) = std::sync::mpsc::channel();
        let mut app = Self {
            index: None,
            scan: None,
            scan_gen: 0,
            message: None,
            tiling: ViewportTiling::default(),
            maximized_viewport: None,
            viewports,
            active_viewport: 0,
            sync_mode: SliceSynchronization::Absolute,
            link_zoom: false,
            link_zoom_same_size: true,
            link_wl: false,
            link_cut: false,
            compare_link_wl: false,
            compare_vp_selected: vec![true; 16],
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
            locked_pairs: std::collections::HashSet::new(),
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
            roi_callout_offsets: HashMap::new(),
            callout_drag: None,
            callout_hit_rects: Vec::new(),
            prev_callout_hit_rects: Vec::new(),
            key_images: std::collections::HashSet::new(),
            key_images_only: false,
            series_spacing_override: HashMap::new(),
            show_calibrate_modal: false,
            pending_remove_study: None,
            pending_clear_library: false,
            calibrate_known_mm: "100".to_string(),
            calibrate_last_len_px: 0.0,
            drawing: Vec::new(),
            angle_pending: None,
            cobb_pending: None,
            selected_roi: None,
            dragging_roi_handle: None,
            roi_handle_original: None,
            dragging_roi_body: None,
            show_roi_histogram: false,
            arrow_edit_target: None,
            arrow_edit_text: String::new(),
            hover_px: None,
            tiling_hover: None,
            playing: false,
            fps: 15.0,
            last_tick: 0.0,
            cursor_info: None,
            show_browser: true,
            sidebar_compact: false,
            show_database_view: false,
            copy_shot: CopyShotState::Idle,
            copy_shot_tx,
            copy_shot_rx,
            copy_anonymize_frame: false,
            viewport_rects: vec![Rect::NOTHING; 16],
            viewport_mm_per_px: vec![None; 16],
            pending_zoom_match: None,
            solo_adjust: false,
            pending_relock: None,
            link_master: None,
            show_vr_tf_editor: false,
            vr_tf_selected_stop: 0,
            vr_histogram_cache: None,
            selected_db_study: None,
            db_sort: saved_db_sort,
            show_metadata_inspector: false,
            metadata_filter: String::new(),
            metadata_cache: None,
            ortho_mpr_active: false,
            ortho_crosshair_lps: None,
            ortho_saved_tiling: None,
            crosshair_drag: None,
            crosshair_tilt_drag: None,
            forced_mpr_series: std::collections::HashSet::new(),
            pending_force_mpr: None,
            pending_mpr_activate: None,
            update_status: crate::updater::UpdateStatus::Idle,
            update_rx: None,
            install_rx: None,
            show_update_modal: false,
            db,
            lang: saved_lang,
            theme_mode: saved_theme_mode,
            blend_siblings: false,
            gpu_bilinear: saved_bilinear,
            check_updates_on_startup: saved_startup_updates,
            crash_report_opt_in: saved_crash_opt_in,
            copy_policy: saved_copy_policy,
            pending_copy_prompt: false,
            show_settings_modal: false,
            study_storage_zip: saved_storage_zip,
            previous_crash_pending: false,
            show_crash_modal: false,
            show_first_run_notice: !first_run_ack,
            confirm_delete_local_data: false,
            patient_links: saved_patient_links,
            linking_source: None,
        };
        if let Some(db) = &app.db {
            if let Ok(saved_rois) = db.load_all_rois() {
                for (k, list) in saved_rois {
                    let converted: Vec<Measure> = list.into_iter().map(Measure::from_persisted).collect();
                    if !converted.is_empty() {
                        app.measures.insert(k, converted);
                    }
                }
            }
            if let Ok(saved_keys) = db.load_all_key_images() {
                app.key_images = saved_keys;
            }
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

    pub(crate) fn viewport_series(&self, vp_idx: usize) -> Option<&Series> {
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
        let mut zip_sources: Vec<(String, Vec<PathBuf>)> = Vec::new();
        for st in &mut index.studies {
            let is_zip = st.series.iter().any(|se| {
                se.slices.first().is_some_and(|sl| {
                    let p = sl.path.to_string_lossy();
                    p.contains("sekhmetx-zip-") || p.contains("rdicom-zip-cache")
                })
            });
            if is_zip {
                let zips: Vec<PathBuf> = st
                    .series
                    .iter()
                    .flat_map(|se| se.slices.iter())
                    .filter_map(|sl| crate::dicom_index::original_zip_for_extracted_path(&sl.path))
                    .collect();
                if !zips.is_empty() {
                    zip_sources.push((st.uid.clone(), zips));
                }
                let _ = crate::db::copy_study_into_library(st);
            }
        }
        if let Some(ref mut db) = self.db {
            let _ = db.save_index(index);
            for (uid, zips) in zip_sources {
                db.set_study_source_paths(&uid, &zips);
            }
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
                    self.locked_pairs.clear();
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
                    let mut zip_sources: Vec<(String, Vec<PathBuf>)> = Vec::new();
                    if self.copy_policy == crate::db::CopyToLibraryPolicy::Always {
                        for st in &mut index.studies {
                            let is_zip = st.series.iter().any(|se| {
                                se.slices.first().is_some_and(|sl| {
                                    let p = sl.path.to_string_lossy();
                                    p.contains("sekhmetx-zip-") || p.contains("rdicom-zip-cache")
                                })
                            });
                            if is_zip {
                                let zips: Vec<PathBuf> = st
                                    .series
                                    .iter()
                                    .flat_map(|se| se.slices.iter())
                                    .filter_map(|sl| {
                                        crate::dicom_index::original_zip_for_extracted_path(&sl.path)
                                    })
                                    .collect();
                                if !zips.is_empty() {
                                    zip_sources.push((st.uid.clone(), zips));
                                }
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
                        for (uid, zips) in zip_sources {
                            db.set_study_source_paths(&uid, &zips);
                        }
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
        let mut completed_pairs: Vec<SeriesPair> = Vec::new();
        while let Ok((pair, reg)) = self.reg_rx.try_recv() {
            let inv = reg.inverse();
            self.registrations_in_flight.remove(&pair);
            self.registrations_in_flight.remove(&(pair.1, pair.0));
            // A hand-made lock that arrived while this was computing wins.
            if self.locked_pairs.contains(&pair) || self.locked_pairs.contains(&(pair.1, pair.0)) {
                continue;
            }
            crate::crash_log::log_line(
                "INFO",
                &format!(
                    "Registration finished: score {:.2}, outline-based {}, trusted {}",
                    reg.correlation,
                    reg.outline_based,
                    reg.is_reliable()
                ),
            );
            self.registrations.insert(pair, reg);
            self.registrations.insert((pair.1, pair.0), inv);
            completed_pairs.push(pair);
            completed_reg = true;
        }
        // A trusted alignment just arrived: with Cut linked, turn the other viewports
        // to match the viewport the comparison started from (no manual rotation needed).
        if !completed_pairs.is_empty() && self.link_cut && !self.ortho_mpr_active {
            let count = self.tiling.cell_count();
            let master = self.link_master.filter(|&m| m < count).unwrap_or(self.active_viewport.min(count.saturating_sub(1)));
            if let Some(master_sel) = self.viewports.get(master).and_then(|v| v.selected) {
                for other_idx in 0..count {
                    let Some(other_sel) = self.viewports[other_idx].selected else {
                        continue;
                    };
                    let pair = (master_sel, other_sel);
                    let just_done = completed_pairs.iter().any(|&p| p == pair || p == (other_sel, master_sel));
                    if other_idx != master
                        && just_done
                        && self.are_viewports_linked(master, other_idx)
                        && !self.locked_pairs.contains(&pair)
                        && self.registrations.get(&pair).is_some_and(|r| r.is_reliable())
                    {
                        self.align_plane_to(master, other_idx);
                    }
                }
            }
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
        if !self.show_database_view
            && ctx.input(|i| {
                i.events.iter().any(|e| matches!(e, egui::Event::Copy))
                    || (i.modifiers.command && i.key_pressed(Key::C))
            })
        {
            self.request_copy_active_viewport(ctx);
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
            Key::K,
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
            Key::Y,
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
                    self.delete_selected_or_last_roi(vp);
                }
                Key::Escape => {
                    self.drawing.clear();
                    self.angle_pending = None;
                    self.cobb_pending = None;
                    self.selected_roi = None;
                    self.dragging_roi_handle = None;
                    self.dragging_roi_body = None;
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
                Key::K => {
                    self.toggle_viewport_key_image(vp);
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
                    self.cobb_pending = None;
                }
                Key::Y => {
                    self.tool = Tool::Arrow;
                    self.drawing.clear();
                    self.angle_pending = None;
                    self.cobb_pending = None;
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
                let cog_resp = crate::ui::icons::small_tool_button(
                    ui,
                    crate::ui::icons::Icon::Gear,
                    self.show_settings_modal,
                );
                if cog_resp.on_hover_text(tr.settings_hover).clicked() {
                    self.show_settings_modal = !self.show_settings_modal;
                }
                ui.label(
                    egui::RichText::new(format!("v{}", crate::BUILD_VERSION))
                        .small()
                        .color(crate::ui::theme::dim_text(ui)),
                );
                match &self.update_status {
                    crate::updater::UpdateStatus::Available(info) => {
                        let lbl = match self.lang {
                            crate::i18n::Lang::Es => format!("Actualizar a {}", info.tag_name),
                            crate::i18n::Lang::En => format!("Update to {}", info.tag_name),
                        };
                        if ui
                            .small_button(
                                egui::RichText::new(lbl)
                                    .strong()
                                    .color(Color32::from_rgb(120, 225, 150)),
                            )
                            .on_hover_text(match self.lang {
                                crate::i18n::Lang::Es => format!("Hay una actualización ({}): clic para instalar", info.tag_name),
                                crate::i18n::Lang::En => format!("Update {} available: click to install", info.tag_name),
                            })
                            .clicked()
                        {
                            self.show_update_modal = true;
                        }
                    }
                    crate::updater::UpdateStatus::Installed(tag) => {
                        let lbl = match self.lang {
                            crate::i18n::Lang::Es => format!("{tag} instalada: reiniciar"),
                            crate::i18n::Lang::En => format!("{tag} installed: restart"),
                        };
                        if ui
                            .small_button(
                                egui::RichText::new(lbl)
                                    .strong()
                                    .color(Color32::from_rgb(120, 225, 150)),
                            )
                            .on_hover_text(match self.lang {
                                crate::i18n::Lang::Es => "Actualización instalada: clic para reiniciar SekhmetX",
                                crate::i18n::Lang::En => "Update installed: click to restart SekhmetX",
                            })
                            .clicked()
                        {
                            self.show_update_modal = true;
                        }
                    }
                    _ => {}
                }
                if self.previous_crash_pending {
                    let lbl = match self.lang {
                        crate::i18n::Lang::Es => "Registro de fallo",
                        crate::i18n::Lang::En => "Crash log",
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
                if let Some(info) = self.cursor_info.as_ref().filter(|_| !self.show_database_view) {
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

impl ViewerApp {
    /// Returns `(path, frame)` for the current slice in `vp_idx`.
    pub(crate) fn viewport_key_image_id(&self, vp_idx: usize) -> Option<(PathBuf, u32)> {
        let cur_slice = self.viewports.get(vp_idx)?.slice;
        let key = self.viewport_slice_key(vp_idx, cur_slice)?;
        let sref = self
            .index
            .as_ref()?
            .studies
            .get(key.study)?
            .series
            .get(key.series)?
            .slices
            .get(key.slice)?;
        Some((sref.path.clone(), sref.frame))
    }

    pub(crate) fn is_viewport_key_image(&self, vp_idx: usize) -> bool {
        self.viewport_key_image_id(vp_idx)
            .is_some_and(|id| self.key_images.contains(&id))
    }

    pub(crate) fn toggle_viewport_key_image(&mut self, vp_idx: usize) {
        let Some((path, frame)) = self.viewport_key_image_id(vp_idx) else {
            return;
        };
        let now_key = if self.key_images.contains(&(path.clone(), frame)) {
            self.key_images.remove(&(path.clone(), frame));
            false
        } else {
            self.key_images.insert((path.clone(), frame));
            true
        };
        if let Some(ref db) = self.db {
            let _ = db.set_key_image(&path, frame, now_key);
        }
        if self.key_images_only && !now_key && self.series_key_image_indices(vp_idx).is_empty() {
            self.key_images_only = false;
        }
        self.message = Some(match (self.lang, now_key) {
            (crate::i18n::Lang::Es, true) => "★ Imagen marcada como clave".into(),
            (crate::i18n::Lang::Es, false) => "Imagen clave quitada".into(),
            (crate::i18n::Lang::En, true) => "★ Marked as Key Image".into(),
            (crate::i18n::Lang::En, false) => "Key Image unmarked".into(),
        });
    }

    pub(crate) fn series_key_image_indices(&self, vp_idx: usize) -> Vec<usize> {
        let Some(se) = self.viewport_series(vp_idx) else {
            return Vec::new();
        };
        se.slices
            .iter()
            .enumerate()
            .filter(|(_, s)| self.key_images.contains(&(s.path.clone(), s.frame)))
            .map(|(i, _)| i)
            .collect()
    }

    pub(crate) fn study_key_image_count(&self, si: usize) -> usize {
        let Some(st) = self.study_at(si) else {
            return 0;
        };
        st.series
            .iter()
            .flat_map(|se| se.slices.iter())
            .filter(|s| self.key_images.contains(&(s.path.clone(), s.frame)))
            .count()
    }

    pub(crate) fn step_key_image(&mut self, vp_idx: usize, dir: isize) {
        let indices = self.series_key_image_indices(vp_idx);
        if indices.is_empty() {
            return;
        }
        let cur = self.viewports[vp_idx].slice;
        let next = if dir >= 0 {
            indices
                .iter()
                .copied()
                .find(|&i| i > cur)
                .unwrap_or(indices[0])
        } else {
            indices
                .iter()
                .rev()
                .copied()
                .find(|&i| i < cur)
                .unwrap_or(*indices.last().unwrap_or(&0))
        };
        self.set_viewport_slice(vp_idx, next as isize, true);
    }

    pub(crate) fn delete_selected_or_last_roi(&mut self, vp_idx: usize) {
        let Some(k) = self.viewport_measure_key(vp_idx) else {
            return;
        };
        let removed_idx = if let Some((ref sp, sfr, idx)) = self.selected_roi
            && *sp == k.0
            && sfr == k.1
        {
            if let Some(list) = self.measures.get_mut(&k)
                && idx < list.len()
            {
                list.remove(idx);
                Some(idx)
            } else {
                None
            }
        } else if let Some(list) = self.measures.get_mut(&k)
            && !list.is_empty()
        {
            let idx = list.len() - 1;
            list.pop();
            Some(idx)
        } else {
            None
        };
        if let Some(idx) = removed_idx {
            // Reindex or clear callout offsets for this slice so remaining callouts stay aligned.
            self.roi_callout_offsets.remove(&(k.0.clone(), k.1, idx));
            let shifted: Vec<(usize, Vec2)> = self
                .roi_callout_offsets
                .iter()
                .filter_map(|((p, fr, i), v)| {
                    if *p == k.0 && *fr == k.1 && *i > idx {
                        Some((*i, *v))
                    } else {
                        None
                    }
                })
                .collect();
            for (i, v) in shifted {
                self.roi_callout_offsets.remove(&(k.0.clone(), k.1, i));
                self.roi_callout_offsets.insert((k.0.clone(), k.1, i - 1), v);
            }
            self.selected_roi = None;
            self.dragging_roi_handle = None;
            self.dragging_roi_body = None;
            self.persist_measures_for(&k);
        }
    }

    pub(crate) fn effective_spacing_for_viewport(
        &self,
        vp_idx: usize,
        f: &Frame,
    ) -> Option<(f64, f64)> {
        self.viewports
            .get(vp_idx)
            .and_then(|v| v.selected)
            .and_then(|sel| self.series_spacing_override.get(&sel).copied())
            .or(f.spacing)
    }

    pub(crate) fn export_key_images_png(&mut self) {
        let vp_idx = self.active_viewport;
        let Some((si, _)) = self.viewports[vp_idx].selected else {
            return;
        };
        let Some(st) = self.study_at(si) else {
            return;
        };
        let mut key_slices: Vec<(SliceRef, String, usize)> = Vec::new();
        for se in &st.series {
            for (slice_idx, s) in se.slices.iter().enumerate() {
                if self.key_images.contains(&(s.path.clone(), s.frame)) {
                    let se_label = if se.description.is_empty() {
                        format!("{} Se {}", se.modality, se.number)
                    } else {
                        format!("{} Se {} ({})", se.modality, se.number, se.description)
                    };
                    key_slices.push((s.clone(), se_label, slice_idx + 1));
                }
            }
        }
        if key_slices.is_empty() {
            return;
        }
        let vp = &self.viewports[vp_idx];
        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
            let count = key_slices.len();
            let refs: Vec<SliceRef> = key_slices.iter().map(|(s, _, _)| s.clone()).collect();
            let _ = crate::export::export_series_png(
                &refs,
                vp.window,
                vp.effective_invert(),
                &vp.clut_name,
                vp.convolution_idx,
                &dir,
            );
            // Write anonymized clinical summary (no patient PHI)
            let mut summary = String::from("SekhmetX — Anonymized Key Images Summary\n");
            summary.push_str("========================================\n\n");
            for (idx, (sref, se_label, slice_1based)) in key_slices.iter().enumerate() {
                summary.push_str(&format!(
                    "Key Image #{:02} | {} | Slice {}\n",
                    idx + 1,
                    se_label,
                    slice_1based
                ));
                if let Some(ms) = self.measures.get(&(sref.path.clone(), sref.frame))
                    && !ms.is_empty()
                {
                    summary.push_str(&format!("  Measurements/ROIs: {}\n", ms.len()));
                }
            }
            let _ = std::fs::write(dir.join("case_summary.txt"), summary);
            self.message = Some(match self.lang {
                crate::i18n::Lang::Es => {
                    format!("Exportadas {count} imágenes clave + resumen anonimizado")
                }
                crate::i18n::Lang::En => {
                    format!("Exported {count} Key Images + anonymized summary")
                }
            });
        }
    }

    /// Arm an anonymized HD screenshot of the currently active viewport for clipboard copy.
    pub(crate) fn request_copy_active_viewport(&mut self, ctx: &egui::Context) {
        let Some(&rect) = self.viewport_rects.get(self.active_viewport) else {
            return;
        };
        if !rect.is_positive() {
            return;
        }
        self.copy_shot = CopyShotState::Armed(rect);
        ctx.request_repaint();
    }

    /// Drive the 2-frame anonymized clipboard screenshot capture and copy cropped RGBA to clipboard.
    fn process_copy_screenshot(&mut self, ctx: &egui::Context) {
        if let Ok(image) = self.copy_shot_rx.try_recv()
            && let CopyShotState::WaitingForImage(rect) = self.copy_shot
        {
            self.copy_shot = CopyShotState::Idle;
            self.copy_anonymize_frame = false;
            let ppp = ctx.pixels_per_point();
            let iw = image.width();
            let ih = image.height();
            if iw > 0 && ih > 0 {
                let x0 = ((rect.min.x * ppp).round() as isize).clamp(0, iw as isize) as usize;
                let y0 = ((rect.min.y * ppp).round() as isize).clamp(0, ih as isize) as usize;
                let x1 = ((rect.max.x * ppp).round() as isize).clamp(0, iw as isize) as usize;
                let y1 = ((rect.max.y * ppp).round() as isize).clamp(0, ih as isize) as usize;
                let cw = x1.saturating_sub(x0);
                let ch = y1.saturating_sub(y0);
                if cw > 0 && ch > 0 {
                    let mut bytes = Vec::with_capacity(cw * ch * 4);
                    for y in y0..y1 {
                        let row_start = y * iw + x0;
                        for px in &image.pixels[row_start..row_start + cw] {
                            bytes.extend_from_slice(&px.to_array());
                        }
                    }
                    match arboard::Clipboard::new().and_then(|mut cb| {
                        cb.set_image(arboard::ImageData {
                            width: cw,
                            height: ch,
                            bytes: std::borrow::Cow::Owned(bytes),
                        })
                    }) {
                        Ok(()) => {
                            self.message = Some(match self.lang {
                                crate::i18n::Lang::Es => {
                                    format!("Imagen HD anonimizada copiada ({cw}×{ch})")
                                }
                                crate::i18n::Lang::En => {
                                    format!("Copied anonymized HD image ({cw}×{ch})")
                                }
                            });
                        }
                        Err(err) => {
                            self.message = Some(format!("Clipboard error: {err}"));
                        }
                    }
                }
            }
        }
        if let CopyShotState::Armed(rect) = self.copy_shot {
            self.copy_anonymize_frame = true;
            self.copy_shot = CopyShotState::WaitingForImage(rect);
            let tx = self.copy_shot_tx.clone();
            let ctx_clone = ctx.clone();
            ctx.request_screenshot(move |image| {
                let _ = tx.send(image);
                ctx_clone.request_repaint();
            });
        }
    }

    /// Return to the full Database Library view and unload decoded frames/volumes for the active study.
    pub(crate) fn return_to_library(&mut self) {
        self.show_database_view = true;
        self.key_images_only = false;
        self.show_calibrate_modal = false;
        self.selected_roi = None;
        self.dragging_roi_handle = None;
        self.dragging_roi_body = None;
        self.cobb_pending = None;
        self.arrow_edit_target = None;
        self.show_roi_histogram = false;
        self.maximized_viewport = None;
        self.message = None;
        self.playing = false;
        self.ortho_mpr_active = false;
        self.ortho_crosshair_lps = None;
        self.ortho_saved_tiling = None;
        self.crosshair_drag = None;
        self.crosshair_tilt_drag = None;
        self.pending_mpr_activate = None;
        self.show_metadata_inspector = false;
        self.metadata_cache = None;
        for vp in &mut self.viewports {
            vp.selected = None;
            vp.reset_view();
            vp.reset_clut();
            vp.invert = false;
            vp.mpr_enabled = false;
            vp.mpr_plane = None;
            vp.mpr_cached = None;
            vp.vr_enabled = false;
        }
        self.cache.clear();
        self.volumes.clear();
        self.volume_errors.clear();
        self.failed.clear();
        self.registrations.clear();
        self.registrations_in_flight.clear();
        self.locked_pairs.clear();
    }
}

impl eframe::App for ViewerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let (cmd_held, pointer_down) = ctx.input(|i| (i.modifiers.command, i.pointer.any_down()));
        self.solo_adjust = cmd_held;
        // A viewport moved alone with Ctrl/Cmd: keep it linked at its new offset.
        if !cmd_held
            && !pointer_down
            && let Some(vp) = self.pending_relock.take()
            && self.link_cut
            && self.tiling.cell_count() > 1
            && vp < self.tiling.cell_count()
        {
            let prev_active = self.active_viewport;
            self.active_viewport = vp;
            self.lock_current_3d_alignment();
            self.active_viewport = prev_active;
        }
        self.process_copy_screenshot(&ctx);
        self.poll_background(&ctx);
        self.handle_dropped_files(&ctx);
        self.handle_keys(&ctx);

        let chrome = crate::ui::theme::chrome_fill(&ctx);
        egui::Panel::top("toolbar")
            .frame(
                egui::Frame::new()
                    .fill(chrome)
                    .inner_margin(egui::Margin::symmetric(8, 5)),
            )
            .show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(chrome)
                    .inner_margin(egui::Margin::symmetric(8, 3)),
            )
            .show(ui, |ui| self.status_bar(ui));
        if self.show_database_view {
            egui::CentralPanel::default().show(ui, |ui| self.database_view(ui));
        } else {
            if self.show_browser {
                if self.sidebar_compact {
                    egui::Panel::left("browser_compact")
                        .resizable(false)
                        .exact_size(96.0)
                        .show(ui, |ui| self.browser(ui));
                } else {
                    egui::Panel::left("browser")
                        .resizable(true)
                        .default_size(290.0)
                        .min_size(220.0)
                        .show(ui, |ui| self.browser(ui));
                }
            }
            if self.show_metadata_inspector {
                egui::Panel::right("metadata_inspector")
                    .resizable(true)
                    .default_size(430.0)
                    .min_size(300.0)
                    .show(ui, |ui| self.metadata_inspector_panel(ui));
            }
            egui::CentralPanel::no_frame().show(ui, |ui| self.viewport_grid(ui));
            self.finish_pending_zoom_match(&ctx);
        }
        self.draw_force_mpr_modal(&ctx);
        self.draw_update_modal(&ctx);
        self.draw_settings_modal(&ctx);
        self.draw_calibrate_modal(&ctx);
        self.draw_remove_study_modal(&ctx);
        self.draw_clear_library_modal(&ctx);
        self.draw_arrow_edit_modal(&ctx);
        self.draw_roi_histogram_modal(&ctx);
        self.draw_vr_tf_editor(&ctx);
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

#[cfg(test)]
mod measure_tests {
    use super::Measure;
    use eframe::egui::pos2;

    #[test]
    fn collapsed_shapes_are_degenerate_and_normal_ones_are_not() {
        assert!(Measure::Oval(pos2(10.0, 10.0), pos2(10.4, 30.0)).is_degenerate());
        assert!(Measure::Rect(pos2(5.0, 5.0), pos2(5.0, 5.0)).is_degenerate());
        assert!(!Measure::Oval(pos2(10.0, 10.0), pos2(20.0, 30.0)).is_degenerate());
        assert!(Measure::Length(pos2(1.0, 1.0), pos2(1.5, 1.0)).is_degenerate());
        assert!(!Measure::Length(pos2(1.0, 1.0), pos2(9.0, 1.0)).is_degenerate());
        // Three points on one line enclose nothing.
        assert!(Measure::Polygon(vec![pos2(0.0, 0.0), pos2(5.0, 0.0), pos2(10.0, 0.0)]).is_degenerate());
        assert!(!Measure::Polygon(vec![pos2(0.0, 0.0), pos2(10.0, 0.0), pos2(0.0, 10.0)]).is_degenerate());
        assert!(!Measure::Point(pos2(3.0, 3.0)).is_degenerate());
    }
}
