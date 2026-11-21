//! The viewer window: study/series browser on the left, Horos-style toolbar at
//! the top, and multi-series comparison viewports (`1×1` through `3×3`) with
//! synchronized scrolling, 3D cross-reference/scout lines, CLUTs, and
//! orientation edge markers in the center.

mod browser;
mod cpr_view;
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
    /// Curved MPR: click points along a structure, then Enter or double-click.
    CurvedPath,
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
    /// Fused slice, view plane (hash), blend mode, factor, CLUT, window bits.
    pub(crate) tex_key: Option<(SliceKey, u64, i32, i32, String, u32, u32)>,
    /// The fused series resampled along the viewport's plane, by plane hash.
    pub(crate) resliced: Option<(u64, Arc<Frame>)>,
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
            resliced: None,
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
    /// Scissors: what has been cut away in 3D, and earlier states for Undo.
    pub(crate) vr_sculpt: Option<Arc<crate::sculpt::SculptMask>>,
    pub(crate) vr_sculpt_undo: Vec<Arc<crate::sculpt::SculptMask>>,
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
            vr_sculpt: None,
            vr_sculpt_undo: Vec::new(),
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

pub(crate) enum PacsBgResult {
    Qido(Result<Vec<crate::net::StudyQueryResult>, String>),
    Wado(Result<(PathBuf, usize), String>),
    Echo(Result<u128, String>),
    Find(Result<Vec<crate::net::StudyQueryResult>, String>),
    Store(Result<usize, String>),
    Move(Result<u16, String>),
    ScpReceived(PathBuf, usize),
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
    sr_cache: HashMap<PathBuf, Option<(String, Option<crate::imaging::sr::GspsState>)>>,
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
    shot_purpose: ShotPurpose,
    /// The open curved MPR, if any.
    pub(crate) cpr: Option<cpr_view::CprView>,
    /// Scissors mode in the 3D view (which side of the drawn loop is cut), and the
    /// loop being drawn (screen points).
    pub(crate) sculpt_mode: Option<crate::sculpt::CutSide>,
    pub(crate) sculpt_lasso: Vec<Pos2>,
    /// The measurement callout texts drawn in each viewport on the last frame.
    pub(crate) viewport_callouts: Vec<Vec<String>>,
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
    pub(crate) custom_presets: Vec<crate::settings::CustomPreset>,
    pub(crate) new_preset_name: String,
    pub(crate) new_preset_wl: f32,
    pub(crate) new_preset_ww: f32,
    pub(crate) hanging_protocols: Vec<crate::settings::HangingProtocol>,
    pub(crate) new_hp_modality: String,
    pub(crate) new_hp_rows: usize,
    pub(crate) new_hp_cols: usize,
    pub(crate) new_hp_auto_mpr: bool,
    pub(crate) roi_undo: Vec<((PathBuf, u32), Vec<Measure>)>,
    pub(crate) show_pacs_modal: bool,
    pub(crate) pacs_tab_dimse: bool,
    pub(crate) pacs_nodes: Vec<crate::net::PacsNode>,
    pub(crate) dicomweb_servers: Vec<crate::net::DicomWebServer>,
    pub(crate) selected_dicomweb: usize,
    pub(crate) selected_pacs_node: usize,
    pub(crate) dicomweb_secret: String,
    pub(crate) pacs_filter: crate::net::QueryFilter,
    pub(crate) pacs_results: Vec<crate::net::StudyQueryResult>,
    pub(crate) pacs_status: Option<String>,
    pub(crate) pacs_busy: bool,
    pub(crate) new_dw_name: String,
    pub(crate) new_dw_url: String,
    pub(crate) new_dw_auth: crate::net::DicomWebAuth,
    pub(crate) new_pacs_name: String,
    pub(crate) new_pacs_ae: String,
    pub(crate) new_pacs_host: String,
    pub(crate) new_pacs_port: String,
    pub(crate) new_pacs_local_ae: String,
    pub(crate) new_pacs_tls: bool,
    pub(crate) pacs_tx: std::sync::mpsc::Sender<PacsBgResult>,
    pacs_rx: Receiver<PacsBgResult>,
    pub(crate) scp_port: String,
    pub(crate) scp_stop: Option<Arc<std::sync::atomic::AtomicBool>>,
    pub(crate) report_template: String,
    pub(crate) custom_shortcuts: HashMap<String, String>,
    pub(crate) show_detached_library: bool,
    pub(crate) meta_edit_field: usize,
    pub(crate) meta_edit_value: String,
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

/// What an anonymized viewport capture is for.
pub(crate) enum ShotPurpose {
    /// Copy to the clipboard (`Ctrl`/`Cmd`+`C`).
    Clipboard,
    /// Keep as the snapshot of a key image just marked in viewport `vp`, saved as
    /// `<stem>.png` with a `<stem>.txt` beside it (series, image number and the
    /// measurement texts shown), for the case report.
    KeyImage { vp: usize, stem: String, label: String, image_no: usize },
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
        let saved_custom_presets = settings
            .as_ref()
            .map(|s| s.custom_presets())
            .unwrap_or_default();
        let saved_hanging_protocols = settings
            .as_ref()
            .map(|s| s.hanging_protocols())
            .unwrap_or_default();
        let saved_pacs_nodes = settings
            .as_ref()
            .map(|s| s.pacs_nodes())
            .unwrap_or_default();
        let saved_dicomweb_servers = settings
            .as_ref()
            .map(|s| s.dicomweb_servers())
            .unwrap_or_default();
        let saved_report_template = settings
            .as_ref()
            .map(|s| s.report_template())
            .unwrap_or_default();
        let saved_custom_shortcuts = settings
            .as_ref()
            .map(|s| s.custom_shortcuts())
            .unwrap_or_default();
        let (pacs_tx, pacs_rx) = std::sync::mpsc::channel();
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
            sr_cache: HashMap::new(),
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
            shot_purpose: ShotPurpose::Clipboard,
            cpr: None,
            sculpt_mode: None,
            sculpt_lasso: Vec::new(),
            viewport_callouts: vec![Vec::new(); 16],
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
            custom_presets: saved_custom_presets,
            new_preset_name: String::new(),
            new_preset_wl: 40.0,
            new_preset_ww: 400.0,
            hanging_protocols: saved_hanging_protocols,
            new_hp_modality: "CT".to_string(),
            new_hp_rows: 1,
            new_hp_cols: 2,
            new_hp_auto_mpr: true,
            roi_undo: Vec::new(),
            show_pacs_modal: false,
            pacs_tab_dimse: false,
            pacs_nodes: saved_pacs_nodes,
            dicomweb_servers: saved_dicomweb_servers,
            selected_dicomweb: 0,
            selected_pacs_node: 0,
            dicomweb_secret: String::new(),
            pacs_filter: crate::net::QueryFilter::default(),
            pacs_results: Vec::new(),
            pacs_status: None,
            pacs_busy: false,
            new_dw_name: String::new(),
            new_dw_url: String::new(),
            new_dw_auth: crate::net::DicomWebAuth::None,
            new_pacs_name: String::new(),
            new_pacs_ae: String::new(),
            new_pacs_host: String::new(),
            new_pacs_port: "4242".to_string(),
            new_pacs_local_ae: "SEKHMETX".to_string(),
            new_pacs_tls: false,
            pacs_tx,
            pacs_rx,
            scp_port: "11112".to_string(),
            scp_stop: None,
            report_template: saved_report_template,
            custom_shortcuts: saved_custom_shortcuts,
            show_detached_library: false,
            meta_edit_field: 0,
            meta_edit_value: String::new(),
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
                    let (lo, hi) = vol.content_span_mm(plane.normal);
                    let step = vol.spacing[0]
                        .min(vol.spacing[1])
                        .min(vol.spacing[2])
                        .clamp(0.5, 2.0);
                    return (((hi - lo) / step).round() as usize + 1).clamp(15, 2048);
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

    pub(crate) fn export_active_series_gif(&mut self) {
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
        let fps = self.fps.clamp(1.0, 60.0);
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Animated GIF", &["gif"])
            .set_file_name("sekhmetx_cine.gif")
            .save_file()
        {
            match crate::export::export_series_gif(
                &se.slices,
                vp.window,
                vp.effective_invert(),
                &vp.clut_name,
                vp.convolution_idx,
                fps,
                &path,
            ) {
                Ok(n) => {
                    self.message = Some(if self.lang == crate::i18n::Lang::Es {
                        format!("GIF exportado ({n} cortes): {}", path.display())
                    } else {
                        format!("Exported GIF ({n} frames): {}", path.display())
                    });
                }
                Err(e) => {
                    self.message = Some(format!("GIF export failed: {e}"));
                }
            }
        }
    }

    /// Splits a multi-b-value MR DWI series into separate sub-series (one per b-value)
    /// using IsiX's `BValueSplit` tag priority and grouping rules.
    pub(crate) fn split_dwi_series(&mut self, si: usize, ri: usize) {
        let es = self.lang == crate::i18n::Lang::Es;
        let Some(ref mut idx) = self.index else {
            return;
        };
        let Some(study) = idx.studies.get_mut(si) else {
            return;
        };
        let Some(base) = study.series.get(ri).cloned() else {
            return;
        };
        let b_vals: Vec<Option<f64>> = base
            .slices
            .iter()
            .map(|s| crate::imaging::mr_maps::BValueSplit::read_from_file(&s.path))
            .collect();
        let groups = crate::imaging::mr_maps::BValueSplit::groups(&b_vals);
        if groups.is_empty() {
            self.message = Some(
                if es {
                    "Esta serie no tiene múltiples valores b para separar"
                } else {
                    "This series does not contain multiple b-values to split"
                }
                .to_string(),
            );
            return;
        }
        let n_groups = groups.len();
        for (gi, g) in groups.into_iter().enumerate() {
            let tag = match g.b_value {
                Some(b) => format!("b={b:.0}"),
                None => "b=?".to_string(),
            };
            let mut sub = base.clone();
            sub.uid = format!("{}.b{}", base.uid, gi + 1);
            sub.description = if base.description.is_empty() {
                format!("DWI [{tag}]")
            } else {
                format!("{} [{tag}]", base.description)
            };
            sub.slices = g.indices.into_iter().map(|i| base.slices[i].clone()).collect();
            study.series.push(sub);
        }
        self.message = Some(if es {
            format!("Serie DWI separada en {n_groups} grupos de valor b")
        } else {
            format!("Split DWI series into {n_groups} b-value groups")
        });
    }

    /// Snapshots the current ROI list for `key` onto the undo stack (capped at 32 entries).
    pub(crate) fn push_roi_undo(&mut self, key: &(PathBuf, u32)) {
        let prev = self.measures.get(key).cloned().unwrap_or_default();
        self.roi_undo.push((key.clone(), prev));
        if self.roi_undo.len() > 32 {
            self.roi_undo.remove(0);
        }
    }

    /// Restores the previous ROI state from the undo stack (`Ctrl+Z` / `Cmd+Z`).
    pub(crate) fn undo_last_roi(&mut self) {
        let Some((key, prev)) = self.roi_undo.pop() else {
            return;
        };
        if prev.is_empty() {
            self.measures.remove(&key);
        } else {
            self.measures.insert(key.clone(), prev);
        }
        self.persist_measures_for(&key);
        self.selected_roi = None;
        self.message = Some(
            if self.lang == crate::i18n::Lang::Es {
                "Último cambio de ROI deshecho"
            } else {
                "Undid last ROI change"
            }
            .to_string(),
        );
    }

    /// Generates an in-memory ADC (`is_adc = true`, in `10⁻⁶ mm²/s`) or T2 (`is_adc = false`, in `ms`)
    /// parametric map series from a multi-b DWI or multi-echo MR series and selects it.
    pub(crate) fn generate_mr_map(&mut self, si: usize, ri: usize, is_adc: bool) {
        let es = self.lang == crate::i18n::Lang::Es;
        let base = match self
            .index
            .as_ref()
            .and_then(|idx| idx.studies.get(si))
            .and_then(|st| st.series.get(ri))
        {
            Some(s) => s.clone(),
            None => return,
        };
        if base.slices.len() < 2 {
            return;
        }

        // Read per-slice parameter (b-value for ADC, EchoTime ms for T2) and 3D origin.
        let params: Vec<Option<f64>> = base
            .slices
            .iter()
            .map(|s| {
                if is_adc {
                    crate::imaging::mr_maps::BValueSplit::read_from_file(&s.path)
                } else {
                    crate::imaging::mr_maps::MrMapFit::read_echo_time_ms(&s.path)
                }
            })
            .collect();
        let origins: Vec<[f64; 3]> = base
            .slices
            .iter()
            .enumerate()
            .map(|(i, s)| s.position.unwrap_or([0.0, 0.0, s.slice_location.unwrap_or(i as f64)]))
            .collect();
        let groups = crate::imaging::mr_maps::MrMapFit::echo_groups_by_origin(&origins);

        let mut map_slices = Vec::new();
        let mut map_frames = Vec::new();
        for group in groups {
            let mut valid_items: Vec<(f64, usize)> = group
                .into_iter()
                .filter_map(|idx| Some((params[idx]?, idx)))
                .collect();
            valid_items.sort_by(|a, b| a.0.total_cmp(&b.0));
            if valid_items.len() < 2
                || (valid_items.last().unwrap().0 - valid_items[0].0).abs() < 1e-3
            {
                continue;
            }
            let mut loaded = Vec::new();
            let mut xs = Vec::new();
            for (x_val, idx) in &valid_items {
                let sl = &base.slices[*idx];
                if let Ok(fr) = crate::frame::load_frame(&sl.path, sl.frame) {
                    loaded.push(fr);
                    xs.push(*x_val);
                }
            }
            if loaded.len() < 2 {
                continue;
            }
            let (w, h) = (loaded[0].width, loaded[0].height);
            let grays: Vec<&[f32]> = loaded
                .iter()
                .filter_map(|f| match &f.pixels {
                    crate::frame::Pixels::Gray(v) if f.width == w && f.height == h => Some(v.as_slice()),
                    _ => None,
                })
                .collect();
            if grays.len() < 2 {
                continue;
            }
            let fitted = if is_adc {
                crate::imaging::mr_maps::MrMapFit::fit_adc_um2_per_s(&grays, &xs[..grays.len()], 10.0)
            } else {
                crate::imaging::mr_maps::MrMapFit::fit_t2_ms(&grays, &xs[..grays.len()], 10.0)
            };
            let full = crate::imaging::window::full(&fitted, false);
            let template = &loaded[0];
            let frame = Arc::new(Frame {
                width: w,
                height: h,
                pixels: crate::frame::Pixels::Gray(fitted),
                stored_window: Some(if is_adc {
                    Window::new(1000.0, 2000.0)
                } else {
                    Window::new(100.0, 200.0)
                }),
                full_window: full,
                bits_window: None,
                inverts: false,
                voi_lut_applied: false,
                slope: 1.0,
                spacing: template.spacing,
                pixel_ratio: template.pixel_ratio,
                origin: template.origin,
                orientation: template.orientation,
                is_3d_plane: template.is_3d_plane,
                frame_of_reference_uid: template.frame_of_reference_uid.clone(),
                slice_thickness: template.slice_thickness,
                slice_location: template.slice_location,
                value_unit: if is_adc { "×10⁻⁶ mm²/s" } else { "ms" },
                suv_factor: None,
            });
            map_slices.push(base.slices[valid_items[0].1].clone());
            map_frames.push(frame);
        }

        if map_frames.is_empty() {
            self.message = Some(
                if is_adc {
                    if es {
                        "No se encontraron cortes con múltiples valores b en la misma posición para ADC"
                    } else {
                        "No slices with multiple b-values at the same position found for ADC"
                    }
                } else if es {
                    "No se encontraron cortes con múltiples tiempos de eco (TE) para mapa T2"
                } else {
                    "No slices with multiple echo times (TE) found for T2 map"
                }
                .to_string(),
            );
            return;
        }

        let new_ri = if let Some(ref mut idx) = self.index
            && let Some(study) = idx.studies.get_mut(si)
        {
            let tag = if is_adc { "ADC Map" } else { "T2 Map" };
            let mut map_series = base.clone();
            map_series.uid = format!("{}.{}", base.uid, if is_adc { "adc" } else { "t2" });
            map_series.description = if base.description.is_empty() {
                format!("[{tag}]")
            } else {
                format!("{} [{tag}]", base.description)
            };
            map_series.slices = map_slices;
            let idx_new = study.series.len();
            study.series.push(map_series);
            idx_new
        } else {
            return;
        };

        for (sl_i, frame) in map_frames.into_iter().enumerate() {
            let key = SliceKey {
                scan_gen: self.scan_gen,
                study: si,
                series: new_ri,
                slice: sl_i,
            };
            self.cache.insert(key, frame);
        }
        let vp = self.active_viewport;
        self.assign_series_to_viewport(vp, Some((si, new_ri)));
        self.message = Some(if is_adc {
            if es { "Mapa ADC generado" } else { "Generated ADC Map" }.to_string()
        } else if es {
            "Mapa T2 generado".to_string()
        } else {
            "Generated T2 Map".to_string()
        });
    }

    /// Collects unique on-disk `.dcm` paths for the currently active study (for C-STORE / STOW-RS).
    pub(crate) fn active_study_file_paths(&self) -> Vec<PathBuf> {
        let Some((si, _)) = self.active_cell().selected else {
            return Vec::new();
        };
        let Some(study) = self.study_at(si) else {
            return Vec::new();
        };
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for se in &study.series {
            for sl in &se.slices {
                if sl.path.is_file() && seen.insert(sl.path.clone()) {
                    out.push(sl.path.clone());
                }
            }
        }
        out
    }

    /// Computes a live digital subtraction series (`post - mask`) from the active series
    /// and a mask series (either another viewport's series in the same study, or the
    /// preceding series in the study).
    pub(crate) fn subtract_active_series(&mut self) {
        let es = self.lang == crate::i18n::Lang::Es;
        let Some((si, post_ri)) = self.active_cell().selected else {
            return;
        };
        // Pick mask series: prefer another visible viewport in the same study, else post_ri - 1.
        let mut mask_ri = None;
        for (i, vp) in self.viewports.iter().take(self.tiling.cell_count()).enumerate() {
            if i != self.active_viewport
                && let Some((vsi, vri)) = vp.selected
                && vsi == si
                && vri != post_ri
            {
                mask_ri = Some(vri);
                break;
            }
        }
        let mask_ri = mask_ri.or_else(|| post_ri.checked_sub(1));
        let Some(mask_ri) = mask_ri else {
            self.message = Some(if es {
                "Abra o seleccione otra serie del mismo estudio como máscara para sustraer.".into()
            } else {
                "Open another series from the same study (or select a post-contrast series) as mask.".into()
            });
            return;
        };
        let (post_series, mask_series) = match self
            .index
            .as_ref()
            .and_then(|idx| idx.studies.get(si))
            .and_then(|st| Some((st.series.get(post_ri)?.clone(), st.series.get(mask_ri)?.clone())))
        {
            Some(pair) => pair,
            None => return,
        };
        let n = post_series.slices.len().min(mask_series.slices.len());
        if n == 0 {
            return;
        }
        let mut sub_slices = Vec::with_capacity(n);
        let mut sub_frames = Vec::with_capacity(n);
        for i in 0..n {
            let p_sl = &post_series.slices[i];
            let m_sl = &mask_series.slices[i];
            let Ok(pf) = crate::frame::load_frame(&p_sl.path, p_sl.frame) else {
                continue;
            };
            let Ok(mf) = crate::frame::load_frame(&m_sl.path, m_sl.frame) else {
                continue;
            };
            if let Some(sub) = crate::imaging::subtraction::subtract_frames(&pf, &mf) {
                let mut sl = p_sl.clone();
                sl.path = PathBuf::from(format!("memory://sub/{si}/{post_ri}-{mask_ri}/{i}"));
                sub_slices.push(sl);
                sub_frames.push(Arc::new(sub));
            }
        }
        if sub_frames.is_empty() {
            self.message = Some(if es {
                "Las series no tienen las mismas dimensiones en escala de grises.".into()
            } else {
                "Series dimensions or pixel formats do not match for subtraction.".into()
            });
            return;
        }
        let new_ri = if let Some(study) = self.index.as_mut().and_then(|i| i.studies.get_mut(si)) {
            let mut sub_se = post_series.clone();
            sub_se.uid = format!("{}.sub.{mask_ri}", post_series.uid);
            sub_se.description = if post_series.description.is_empty() {
                "[SUB]".to_string()
            } else {
                format!("{} [SUB]", post_series.description)
            };
            sub_se.slices = sub_slices;
            let idx_new = study.series.len();
            study.series.push(sub_se);
            idx_new
        } else {
            return;
        };
        for (sl_i, frame) in sub_frames.into_iter().enumerate() {
            self.cache.insert(
                SliceKey {
                    scan_gen: self.scan_gen,
                    study: si,
                    series: new_ri,
                    slice: sl_i,
                },
                frame,
            );
        }
        let vp = self.active_viewport;
        self.assign_series_to_viewport(vp, Some((si, new_ri)));
        self.message = Some(if es {
            "Serie de sustracción [SUB] generada".into()
        } else {
            "Subtraction series [SUB] generated".into()
        });
    }

    /// Computes total Agatston Calcium Score across all closed ROIs on the active CT series (≥ 130 HU).
    pub(crate) fn compute_active_calcium_score(&mut self) {
        let es = self.lang == crate::i18n::Lang::Es;
        let Some((si, ri)) = self.active_cell().selected else {
            return;
        };
        let Some(series) = self
            .index
            .as_ref()
            .and_then(|idx| idx.studies.get(si))
            .and_then(|st| st.series.get(ri))
            .cloned()
        else {
            return;
        };
        let mut total = crate::roi::statistics::CalciumScoreSums::ZERO;
        let mut roi_count = 0usize;
        for sl in &series.slices {
            let key = (sl.path.clone(), sl.frame);
            let Some(list) = self.measures.get(&key) else {
                continue;
            };
            let Ok(frame) = crate::frame::load_frame(&sl.path, sl.frame) else {
                continue;
            };
            let (sx, sy) = self
                .series_spacing_override
                .get(&(si, ri))
                .copied()
                .or(frame.spacing)
                .unwrap_or((1.0, 1.0));
            let px_area = (sx * sy) as f32;
            let thickness = frame.slice_thickness.unwrap_or(3.0).max(0.1) as f32;
            for m in list {
                let outline: Vec<crate::roi::RoiPoint> = match m {
                    Measure::Oval(a, b) => {
                        let cx = (a.x + b.x) * 0.5;
                        let cy = (a.y + b.y) * 0.5;
                        let rx = (b.x - a.x).abs() * 0.5;
                        let ry = (b.y - a.y).abs() * 0.5;
                        (0..48)
                            .map(|k| {
                                let t = (k as f32 / 48.0) * std::f32::consts::TAU;
                                crate::roi::RoiPoint::new(
                                    (cx + rx * t.cos()) as f64,
                                    (cy + ry * t.sin()) as f64,
                                )
                            })
                            .collect()
                    }
                    Measure::Rect(a, b) => vec![
                        crate::roi::RoiPoint::new(a.x as f64, a.y as f64),
                        crate::roi::RoiPoint::new(b.x as f64, a.y as f64),
                        crate::roi::RoiPoint::new(b.x as f64, b.y as f64),
                        crate::roi::RoiPoint::new(a.x as f64, b.y as f64),
                    ],
                    Measure::Polygon(pts) | Measure::Pencil(pts) if pts.len() >= 3 => pts
                        .iter()
                        .map(|p| crate::roi::RoiPoint::new(p.x as f64, p.y as f64))
                        .collect(),
                    _ => continue,
                };
                let samples = crate::roi::sample_region_values(&frame, &outline);
                let cs = crate::roi::statistics::CalciumScoreSums::from_hu_samples(
                    &samples, px_area, thickness, 130.0,
                );
                total.add(cs);
                roi_count += 1;
            }
        }
        if roi_count == 0 {
            self.message = Some(if es {
                "Dibuje una o más ROIs cerradas sobre calcificaciones (≥ 130 HU) primero.".into()
            } else {
                "Draw one or more closed ROIs over calcifications (≥ 130 HU) first.".into()
            });
        } else {
            self.message = Some(if es {
                format!(
                    "Calcio Agatston: {:.1}  ·  Volumen: {:.1} mm³  ·  Masa: {:.1} mg ({roi_count} ROIs)",
                    total.score, total.volume, total.mass
                )
            } else {
                format!(
                    "Agatston Score: {:.1}  ·  Volume: {:.1} mm³  ·  Mass: {:.1} mg ({roi_count} ROIs)",
                    total.score, total.volume, total.mass
                )
            });
        }
    }

    /// Extracts an iso-surface from the active 3D volume and saves it as a binary `.stl` mesh.
    pub(crate) fn export_active_3d_stl(&mut self) {
        let es = self.lang == crate::i18n::Lang::Es;
        let vp_idx = self.active_viewport;
        let Some(vol) = self.viewport_volume(vp_idx).cloned() else {
            self.message = Some(if es {
                "Active una serie 3D / volumen primero para exportar superficie STL.".into()
            } else {
                "Open a 3D volume series first to export a 3D STL surface.".into()
            });
            return;
        };
        let threshold = self.viewports[vp_idx]
            .window
            .map(|w| w.level)
            .unwrap_or(250.0);
        let Some(path) = rfd::FileDialog::new()
            .add_filter("STL 3D Mesh", &["stl"])
            .set_file_name("surface.stl")
            .save_file()
        else {
            return;
        };
        let tris = crate::imaging::stl::extract_isosurface_triangles(&vol, threshold, 128);
        if tris.is_empty() {
            self.message = Some(if es {
                format!("No se encontró superficie en el umbral {threshold:.0} HU.")
            } else {
                format!("No iso-surface found at threshold {threshold:.0} HU.")
            });
            return;
        }
        match crate::imaging::stl::write_binary_stl(&path, &tris) {
            Ok(()) => {
                self.message = Some(if es {
                    format!(
                        "Superficie 3D exportada: {} triángulos ({threshold:.0} HU)",
                        tris.len()
                    )
                } else {
                    format!(
                        "Exported 3D STL surface: {} triangles ({threshold:.0} HU)",
                        tris.len()
                    )
                });
            }
            Err(e) => {
                self.message = Some(format!("STL export failed: {e}"));
            }
        }
    }

    /// Runs 2D Region Growing seeded at the last Point/Probe ROI on the active slice
    /// (or image center if none), creating a closed `Measure::Polygon` ROI.
    pub(crate) fn grow_region_at_active_point(&mut self) {
        let es = self.lang == crate::i18n::Lang::Es;
        let vp = self.active_viewport;
        let Some(frame) = self.viewport_frame(vp).cloned() else {
            return;
        };
        let Some(key) = self.viewport_measure_key(vp) else {
            return;
        };
        let seed = self
            .measures
            .get(&key)
            .and_then(|list| {
                list.iter().rev().find_map(|m| match m {
                    Measure::Point(p) | Measure::Arrow { tip: p, .. } => Some(*p),
                    _ => None,
                })
            });
        let Some(seed_pt) = seed else {
            self.message = Some(if es {
                "Coloque primero un Punto/Sonda sobre la lesión y vuelva a ejecutar Crecer Región.".into()
            } else {
                "Place a Point/Probe ROI on the lesion first, then run Grow Region.".into()
            });
            return;
        };
        let sx = (seed_pt.x.round() as isize).clamp(0, frame.width as isize - 1) as usize;
        let sy = (seed_pt.y.round() as isize).clamp(0, frame.height as isize - 1) as usize;
        let win = self.effective_window(vp, &frame);
        let tol = (win.width * 0.12).clamp(20.0, 180.0);
        let poly = crate::roi::grow_region_polygon(&frame, sx, sy, tol);
        if poly.len() < 3 {
            self.message = Some(if es {
                "Región demasiado pequeña para generar contorno.".into()
            } else {
                "Grown region was too small to form a contour.".into()
            });
            return;
        }
        let pts: Vec<Pos2> = poly.into_iter().map(|(x, y)| pos2(x, y)).collect();
        self.push_roi_undo(&key);
        self.measures
            .entry(key.clone())
            .or_default()
            .push(Measure::Polygon(pts));
        self.persist_measures_for(&key);
        self.message = Some(if es {
            format!("Región segmentada (±{tol:.0} {})", frame.value_unit)
        } else {
            format!("Region grown (±{tol:.0} {})", frame.value_unit)
        });
    }

    /// Linearly interpolates closed ROIs across intermediate empty slices of the active series.
    pub(crate) fn interpolate_active_series_rois(&mut self) {
        let es = self.lang == crate::i18n::Lang::Es;
        let Some((si, ri)) = self.active_cell().selected else {
            return;
        };
        let Some(series) = self
            .index
            .as_ref()
            .and_then(|idx| idx.studies.get(si))
            .and_then(|st| st.series.get(ri))
            .cloned()
        else {
            return;
        };
        let measure_to_poly = |m: &Measure| -> Option<Vec<(f32, f32)>> {
            match m {
                Measure::Oval(a, b) => {
                    let cx = (a.x + b.x) * 0.5;
                    let cy = (a.y + b.y) * 0.5;
                    let rx = (b.x - a.x).abs() * 0.5;
                    let ry = (b.y - a.y).abs() * 0.5;
                    Some(
                        (0..32)
                            .map(|k| {
                                let t = (k as f32 / 32.0) * std::f32::consts::TAU;
                                (cx + rx * t.cos(), cy + ry * t.sin())
                            })
                            .collect(),
                    )
                }
                Measure::Rect(a, b) => Some(vec![(a.x, a.y), (b.x, a.y), (b.x, b.y), (a.x, b.y)]),
                Measure::Polygon(pts) | Measure::Pencil(pts) if pts.len() >= 3 => {
                    Some(pts.iter().map(|p| (p.x, p.y)).collect())
                }
                _ => None,
            }
        };

        let mut annotated: Vec<(usize, Vec<(f32, f32)>)> = Vec::new();
        for (idx, sl) in series.slices.iter().enumerate() {
            let key = (sl.path.clone(), sl.frame);
            if let Some(list) = self.measures.get(&key)
                && let Some(poly) = list.iter().rev().find_map(measure_to_poly)
            {
                annotated.push((idx, poly));
            }
        }
        if annotated.len() < 2 {
            self.message = Some(if es {
                "Dibuje una ROI cerrada en al menos dos cortes distintos para interpolar.".into()
            } else {
                "Draw a closed ROI on at least two separate slices to interpolate.".into()
            });
            return;
        }
        let mut added = 0usize;
        for w in annotated.windows(2) {
            let (i0, ref p0) = w[0];
            let (i1, ref p1) = w[1];
            if i1 <= i0 + 1 {
                continue;
            }
            let span = (i1 - i0) as f32;
            for (mid, sl) in series.slices.iter().enumerate().take(i1).skip(i0 + 1) {
                let t = (mid - i0) as f32 / span;
                let interp = crate::roi::interpolate_polygon(p0, p1, t);
                if interp.len() >= 3 {
                    let key = (sl.path.clone(), sl.frame);
                    let pts: Vec<Pos2> = interp.into_iter().map(|(x, y)| pos2(x, y)).collect();
                    self.measures
                        .entry(key.clone())
                        .or_default()
                        .push(Measure::Polygon(pts));
                    self.persist_measures_for(&key);
                    added += 1;
                }
            }
        }
        self.message = Some(if es {
            format!("ROIs interpoladas en {added} corte(s) intermedio(s).")
        } else {
            format!("Interpolated ROIs across {added} intermediate slice(s).")
        });
    }

    /// Exports all ROI measurements in the active study to a CSV spreadsheet.
    pub(crate) fn export_active_rois_csv(&mut self) {
        let es = self.lang == crate::i18n::Lang::Es;
        let Some((si, _)) = self.active_cell().selected else {
            return;
        };
        let Some(study) = self.study_at(si).cloned() else {
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .add_filter("CSV Spreadsheet", &["csv"])
            .set_file_name("rois.csv")
            .save_file()
        else {
            return;
        };
        let mut csv = String::from(
            "Series,Slice,Type,Length_or_Perim_cm,Area_cm2,Mean,SD,Min,Max,Unit\n",
        );
        let mut rows = 0usize;
        for (ri, se) in study.series.iter().enumerate() {
            let s_label = series_label(se).replace(',', ";");
            for (sl_idx, sl) in se.slices.iter().enumerate() {
                let key = (sl.path.clone(), sl.frame);
                let Some(list) = self.measures.get(&key) else {
                    continue;
                };
                let Ok(frame) = crate::frame::load_frame(&sl.path, sl.frame) else {
                    continue;
                };
                let sp = self
                    .series_spacing_override
                    .get(&(si, ri))
                    .copied()
                    .or(frame.spacing);
                for m in list {
                    let (kind_name, len_s, area_s, stats) = match m {
                        Measure::Length(a, b) => {
                            let ev = crate::roi::evaluate_length_with_spacing(
                                &frame,
                                crate::roi::RoiPoint::new(a.x as f64, a.y as f64),
                                crate::roi::RoiPoint::new(b.x as f64, b.y as f64),
                                sp,
                            );
                            (
                                "Length",
                                ev.length_cm_or_px.map(|v| format!("{v:.3}")).unwrap_or_default(),
                                String::new(),
                                None,
                            )
                        }
                        Measure::Oval(a, b) => {
                            let cx = (a.x + b.x) * 0.5;
                            let cy = (a.y + b.y) * 0.5;
                            let rx = (b.x - a.x).abs() * 0.5;
                            let ry = (b.y - a.y).abs() * 0.5;
                            let pts: Vec<_> = (0..48)
                                .map(|k| {
                                    let t = (k as f32 / 48.0) * std::f32::consts::TAU;
                                    crate::roi::RoiPoint::new(
                                        (cx + rx * t.cos()) as f64,
                                        (cy + ry * t.sin()) as f64,
                                    )
                                })
                                .collect();
                            let ev = crate::roi::evaluate_region_with_spacing(
                                &frame,
                                crate::roi::RoiKind::Oval,
                                &pts,
                                None,
                                sp,
                            );
                            (
                                "Oval",
                                ev.length_cm_or_px.map(|v| format!("{v:.3}")).unwrap_or_default(),
                                ev.area_cm2_or_px2.map(|v| format!("{v:.3}")).unwrap_or_default(),
                                ev.stats,
                            )
                        }
                        Measure::Rect(a, b) => {
                            let pts = vec![
                                crate::roi::RoiPoint::new(a.x as f64, a.y as f64),
                                crate::roi::RoiPoint::new(b.x as f64, a.y as f64),
                                crate::roi::RoiPoint::new(b.x as f64, b.y as f64),
                                crate::roi::RoiPoint::new(a.x as f64, b.y as f64),
                            ];
                            let ev = crate::roi::evaluate_region_with_spacing(
                                &frame,
                                crate::roi::RoiKind::Rectangle,
                                &pts,
                                None,
                                sp,
                            );
                            (
                                "Rect",
                                ev.length_cm_or_px.map(|v| format!("{v:.3}")).unwrap_or_default(),
                                ev.area_cm2_or_px2.map(|v| format!("{v:.3}")).unwrap_or_default(),
                                ev.stats,
                            )
                        }
                        Measure::Polygon(pts) | Measure::Pencil(pts) if pts.len() >= 3 => {
                            let rpts: Vec<_> = pts
                                .iter()
                                .map(|p| crate::roi::RoiPoint::new(p.x as f64, p.y as f64))
                                .collect();
                            let ev = crate::roi::evaluate_region_with_spacing(
                                &frame,
                                crate::roi::RoiKind::ClosedPolygon,
                                &rpts,
                                None,
                                sp,
                            );
                            (
                                "Polygon",
                                ev.length_cm_or_px.map(|v| format!("{v:.3}")).unwrap_or_default(),
                                ev.area_cm2_or_px2.map(|v| format!("{v:.3}")).unwrap_or_default(),
                                ev.stats,
                            )
                        }
                        _ => continue,
                    };
                    let (mean_s, sd_s, min_s, max_s) = if let Some(s) = stats {
                        (
                            format!("{:.2}", s.mean),
                            format!("{:.2}", s.deviation),
                            format!("{:.2}", s.minimum),
                            format!("{:.2}", s.maximum),
                        )
                    } else {
                        (String::new(), String::new(), String::new(), String::new())
                    };
                    let unit = if frame.suv_factor.is_some() {
                        "SUV"
                    } else {
                        frame.value_unit
                    };
                    csv.push_str(&format!(
                        "\"{s_label}\",{},{kind_name},{len_s},{area_s},{mean_s},{sd_s},{min_s},{max_s},{unit}\n",
                        sl_idx + 1
                    ));
                    rows += 1;
                }
            }
        }
        match std::fs::write(&path, csv) {
            Ok(()) => {
                self.message = Some(if es {
                    format!("Exportadas {rows} ROI(s) a CSV.")
                } else {
                    format!("Exported {rows} ROI(s) to CSV.")
                });
            }
            Err(e) => {
                self.message = Some(format!("CSV export failed: {e}"));
            }
        }
    }

    /// Exports the active series as an `.mp4` (via `ffmpeg` if installed) or pure-Rust `.avi` movie.
    pub(crate) fn export_active_series_video(&mut self) {
        let es = self.lang == crate::i18n::Lang::Es;
        let vp_idx = self.active_viewport;
        let Some(series) = self.viewport_series(vp_idx).cloned() else {
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Video (.mp4 / .avi)", &["mp4", "avi"])
            .set_file_name("series.mp4")
            .save_file()
        else {
            return;
        };
        let vp = &self.viewports[vp_idx];
        match crate::export::export_series_video(
            &series.slices,
            vp.window,
            vp.effective_invert(),
            &vp.clut_name,
            vp.convolution_idx,
            self.fps,
            &path,
        ) {
            Ok(n) => {
                self.message = Some(if es {
                    format!("Video exportado ({n} fotogramas).")
                } else {
                    format!("Exported video ({n} frames).")
                });
            }
            Err(e) => {
                self.message = Some(format!("Video export failed: {e}"));
            }
        }
    }

    /// Detects 4D temporal phases (repeated slice positions across time/cardiac phases) in the
    /// active series. Returns `Some((num_phases, current_phase_idx, slices_per_phase))`.
    pub(crate) fn active_series_4d_phases(&self) -> Option<(usize, usize, usize)> {
        let vp = self.active_viewport;
        let series = self.viewport_series(vp)?;
        let total = series.slices.len();
        if total < 4 {
            return None;
        }
        // Count how many slices share the first slice's Z/position within 0.25 mm.
        let p0 = series.slices[0].position?;
        let mut matches = 0usize;
        for sl in &series.slices {
            if let Some(p) = sl.position {
                let d2 = (p[0] - p0[0]).powi(2) + (p[1] - p0[1]).powi(2) + (p[2] - p0[2]).powi(2);
                if d2 <= 0.25 * 0.25 {
                    matches += 1;
                }
            }
        }
        if matches < 2 || !total.is_multiple_of(matches) {
            return None;
        }
        let slices_per_phase = total / matches;
        if slices_per_phase == 0 {
            return None;
        }
        let cur_slice = self.viewports[vp].slice.min(total - 1);
        let cur_phase = (cur_slice / slices_per_phase).min(matches - 1);
        Some((matches, cur_phase, slices_per_phase))
    }

    /// Steps the active viewport's 4D temporal phase by `delta` while preserving anatomical Z slice.
    pub(crate) fn step_active_4d_phase(&mut self, delta: isize) {
        let Some((num_phases, cur_phase, per_phase)) = self.active_series_4d_phases() else {
            return;
        };
        let vp = self.active_viewport;
        let cur_z = self.viewports[vp].slice % per_phase;
        let next_phase = ((cur_phase as isize + delta).rem_euclid(num_phases as isize)) as usize;
        let target_slice = next_phase * per_phase + cur_z;
        self.set_viewport_slice(vp, target_slice as isize, true);
    }

    /// Applies a matching saved `HangingProtocol` (if configured for the study's primary
    /// modality) when opening study `si`, optionally placing `lead_series` in viewport 0.
    pub(crate) fn apply_hanging_protocol_for_study(
        &mut self,
        si: usize,
        lead_series: Option<usize>,
    ) -> bool {
        let Some((modality, series_order)) = self.index.as_ref().and_then(|idx| {
            let st = idx.studies.get(si)?;
            let lead = lead_series.and_then(|r| st.series.get(r));
            let primary = lead.or_else(|| {
                st.series
                    .iter()
                    .max_by_key(|s| s.slices.len())
            })?;
            let mut order: Vec<usize> = (0..st.series.len()).collect();
            if let Some(lr) = lead_series
                && let Some(pos) = order.iter().position(|&i| i == lr)
            {
                order.remove(pos);
                order.insert(0, lr);
            }
            Some((primary.modality.clone(), order))
        }) else {
            return false;
        };

        let Some(hp) = self
            .hanging_protocols
            .iter()
            .find(|p| p.modality.eq_ignore_ascii_case(&modality))
            .cloned()
        else {
            return false;
        };

        self.tiling = ViewportTiling::new(hp.rows.clamp(1, 4), hp.cols.clamp(1, 4));
        let count = self.tiling.cell_count();
        self.active_viewport = 0;
        for vp_idx in 0..count {
            if let Some(&ri) = series_order.get(vp_idx) {
                self.assign_series_to_viewport(vp_idx, Some((si, ri)));
                if !hp.auto_mpr {
                    self.viewports[vp_idx].mpr_enabled = false;
                    self.viewports[vp_idx].mpr_auto_applied = true;
                }
            } else {
                self.assign_series_to_viewport(vp_idx, None);
            }
        }
        true
    }

    /// Computes total 3D volume (cm³) of closed ROIs drawn across slices of the active series.
    pub(crate) fn compute_active_series_roi_volume(&mut self) {
        let es = self.lang == crate::i18n::Lang::Es;
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
        let mut slice_areas: Vec<(f64, f64)> = Vec::new();
        let mut fallback_thick = 1.0_f64;
        for (sl_i, sl) in se.slices.iter().enumerate() {
            let key = (sl.path.clone(), sl.frame);
            let Some(items) = self.measures.get(&key) else {
                continue;
            };
            let Some((sp_y, sp_x)) = sl.spacing else {
                continue;
            };
            if sp_x <= 0.0 || sp_y <= 0.0 {
                continue;
            }
            let mut area_mm2 = 0.0_f64;
            for m in items {
                let area_px2: f64 = match m {
                    Measure::Oval(a, b) => {
                        let rx = ((b.x - a.x) * 0.5).abs() as f64;
                        let ry = ((b.y - a.y) * 0.5).abs() as f64;
                        std::f64::consts::PI * rx * ry
                    }
                    Measure::Rect(a, b) => ((b.x - a.x).abs() * (b.y - a.y).abs()) as f64,
                    Measure::Polygon(pts) | Measure::Pencil(pts) if pts.len() >= 3 => {
                        let outline: Vec<crate::roi::RoiPoint> = pts
                            .iter()
                            .map(|p| crate::roi::RoiPoint::new(p.x as f64, p.y as f64))
                            .collect();
                        crate::roi::RoiPlanarGeometry::area_of_polygon(&outline) as f64
                    }
                    _ => 0.0,
                };
                area_mm2 += area_px2 * sp_x * sp_y;
            }
            if area_mm2 > 0.0 {
                let z = sl.slice_location.unwrap_or(sl_i as f64);
                slice_areas.push((z, area_mm2));
            }
        }
        if se.slices.len() >= 2
            && let (Some(a), Some(b)) = (se.slices[0].slice_location, se.slices[1].slice_location)
        {
            let d = (b - a).abs();
            if d > 0.0 {
                fallback_thick = d;
            }
        }
        match crate::roi::compute_roi_volume_cm3(&slice_areas, fallback_thick) {
            Some(vol_cm3) => {
                let n = slice_areas.len();
                self.message = Some(if es {
                    format!("Volumen ROI: {vol_cm3:.2} cm³ ({n} cortes)")
                } else {
                    format!("ROI Volume: {vol_cm3:.2} cm³ ({n} slices)")
                });
            }
            None => {
                self.message = Some(
                    if es {
                        "Dibuje al menos una ROI cerrada (Óvalo, Rect, Polígono o Lápiz) para calcular el volumen"
                    } else {
                        "Draw at least one closed ROI (Oval, Rect, Polygon, or Pencil) to compute volume"
                    }
                    .to_string(),
                );
            }
        }
    }

    pub(crate) fn export_active_anonymized_dicom(&mut self) {
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
        let es = self.lang == crate::i18n::Lang::Es;
        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
            let opts = crate::export::AnonymizeOptions::default();
            // Header anonymization cannot remove text burned into the pixels (ultrasound,
            // screen captures, `BurnedInAnnotation = YES`): warn so the user checks the images.
            let burned_in = se
                .slices
                .first()
                .is_some_and(|sl| crate::export::has_likely_burned_in_text(&sl.path));
            let result = crate::export::anonymize_series(&se.slices, &dir, &opts);
            self.message = Some(match (result, es) {
                (Ok(n), true) => {
                    let mut m = format!("{n} imágenes anonimizadas exportadas a {}", dir.display());
                    if burned_in {
                        m.push_str(" · Atención: puede haber texto del paciente grabado en la imagen; revíselas antes de compartir.");
                    }
                    m
                }
                (Ok(n), false) => {
                    let mut m = format!("{n} anonymized images exported to {}", dir.display());
                    if burned_in {
                        m.push_str(" · Warning: patient text may be burned into the pixels; check the images before sharing.");
                    }
                    m
                }
                (Err(e), true) => format!("Error al exportar DICOM anonimizado: {e}"),
                (Err(e), false) => format!("Anonymized DICOM export failed: {e}"),
            });
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
                let _ = crate::db::copy_study_into_library_with_format(st, self.study_storage_zip);
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

        while let Ok(ev) = self.pacs_rx.try_recv() {
            self.pacs_busy = false;
            let es = self.lang == crate::i18n::Lang::Es;
            match ev {
                PacsBgResult::Qido(Ok(studies)) => {
                    let n = studies.len();
                    self.pacs_results = studies;
                    self.pacs_status = Some(if es {
                        format!("{n} estudio(s) encontrados.")
                    } else {
                        format!("Found {n} study(ies).")
                    });
                }
                PacsBgResult::Qido(Err(e)) => {
                    self.pacs_status = Some(if es {
                        format!("Error QIDO-RS: {e}")
                    } else {
                        format!("QIDO-RS error: {e}")
                    });
                }
                PacsBgResult::Wado(Ok((dir, count))) => {
                    self.pacs_status = Some(if es {
                        format!("{count} archivos descargados. Abriendo estudio…")
                    } else {
                        format!("Retrieved {count} file(s). Opening study…")
                    });
                    self.show_pacs_modal = false;
                    self.show_database_view = false;
                    self.start_scan(vec![dir]);
                }
                PacsBgResult::Wado(Err(e)) => {
                    self.pacs_status = Some(if es {
                        format!("Error WADO-RS: {e}")
                    } else {
                        format!("WADO-RS error: {e}")
                    });
                }
                PacsBgResult::Echo(Ok(ms)) => {
                    self.pacs_status = Some(if es {
                        format!("C-ECHO exitoso ({ms} ms).")
                    } else {
                        format!("C-ECHO succeeded ({ms} ms).")
                    });
                }
                PacsBgResult::Echo(Err(e)) => {
                    self.pacs_status = Some(if es {
                        format!("Error C-ECHO: {e}")
                    } else {
                        format!("C-ECHO failed: {e}")
                    });
                }
                PacsBgResult::Find(Ok(studies)) => {
                    let n = studies.len();
                    self.pacs_results = studies;
                    self.pacs_status = Some(if es {
                        format!("C-FIND: {n} estudio(s) encontrados.")
                    } else {
                        format!("C-FIND: found {n} study(ies).")
                    });
                }
                PacsBgResult::Find(Err(e)) => {
                    self.pacs_status = Some(if es {
                        format!("Error C-FIND: {e}")
                    } else {
                        format!("C-FIND failed: {e}")
                    });
                }
                PacsBgResult::Store(Ok(count)) => {
                    self.pacs_status = Some(if es {
                        format!("Enviados {count} archivo(s) DICOM exitosamente.")
                    } else {
                        format!("Sent {count} DICOM file(s) successfully.")
                    });
                }
                PacsBgResult::Store(Err(e)) => {
                    self.pacs_status = Some(if es {
                        format!("Error al enviar: {e}")
                    } else {
                        format!("Send failed: {e}")
                    });
                }
                PacsBgResult::Move(Ok(_)) => {
                    self.pacs_status = Some(if es {
                        "C-MOVE completado. Revise los archivos recibidos.".into()
                    } else {
                        "C-MOVE completed. Check received study.".into()
                    });
                }
                PacsBgResult::Move(Err(e)) => {
                    self.pacs_status = Some(if es {
                        format!("Error C-MOVE: {e}")
                    } else {
                        format!("C-MOVE failed: {e}")
                    });
                }
                PacsBgResult::ScpReceived(dir, count) => {
                    self.pacs_status = Some(if es {
                        format!("Recibidos {count} archivo(s) DICOM por C-STORE SCP. Abriendo…")
                    } else {
                        format!("Received {count} DICOM file(s) via C-STORE SCP. Opening…")
                    });
                    self.show_database_view = false;
                    self.start_scan(vec![dir]);
                }
            }
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
                        if let Some(zerr) = dicom_index::take_last_zip_error() {
                            self.message = Some(format!("ZIP error: {zerr}"));
                        } else {
                            self.message = Some(format!(
                                "No DICOM images found ({} files checked).",
                                index.files_seen
                            ));
                        }
                    } else {
                        let _ = dicom_index::take_last_zip_error();
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
                                let _ = crate::db::copy_study_into_library_with_format(st, self.study_storage_zip);
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
                // The fused (PET) series is loaded whole so it can be built as a volume
                // and sampled along whatever plane this viewport shows.
                if self.viewports[vp_idx].fusion.enabled
                    && let Some((fst, fse)) = self.viewports[vp_idx].fusion.series
                    && !focuses.iter().any(|&(a, b, _)| a == fst && b == fse)
                {
                    let mid = self.series_at((fst, fse)).map_or(0, |s| s.slices.len() / 2);
                    focuses.push((fst, fse, mid));
                }
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
        if !self.show_database_view
            && ctx.input(|i| i.modifiers.command && i.key_pressed(Key::Z))
        {
            self.undo_last_roi();
            return;
        }
        // Check user-customized single-letter shortcuts first.
        if !self.custom_shortcuts.is_empty() {
            let mut matched_action: Option<String> = None;
            ctx.input(|i| {
                if i.modifiers.command || i.modifiers.ctrl || i.modifiers.alt {
                    return;
                }
                for (action, key_str) in &self.custom_shortcuts {
                    if let Some(k) = Key::from_name(key_str.trim())
                        && i.key_pressed(k)
                    {
                        matched_action = Some(action.clone());
                        break;
                    }
                }
            });
            if let Some(action) = matched_action {
                let vp = self.active_viewport;
                match action.as_str() {
                    "wl" => {
                        self.tool = Tool::WindowLevel;
                        self.drawing.clear();
                    }
                    "scroll" => {
                        self.tool = Tool::Scroll;
                        self.drawing.clear();
                    }
                    "pan" => {
                        self.tool = Tool::Pan;
                        self.drawing.clear();
                    }
                    "zoom" => {
                        self.tool = Tool::Zoom;
                        self.drawing.clear();
                    }
                    "length" => {
                        self.tool = Tool::Length;
                        self.drawing.clear();
                    }
                    "oval" => {
                        self.tool = Tool::Oval;
                        self.drawing.clear();
                    }
                    "reset" => {
                        self.viewports[vp].reset_view();
                        self.viewports[vp].reset_clut();
                        self.viewports[vp].invert = false;
                        self.set_active_window(None);
                    }
                    "key_image" => self.toggle_viewport_key_image(vp),
                    "pacs" => self.show_pacs_modal = !self.show_pacs_modal,
                    _ => {}
                }
                return;
            }
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
            Key::U,
            Key::X,
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
                    self.sculpt_mode = None;
                    self.sculpt_lasso.clear();
                    self.drawing.clear();
                    self.angle_pending = None;
                    self.cobb_pending = None;
                    self.selected_roi = None;
                    self.dragging_roi_handle = None;
                    self.dragging_roi_body = None;
                }
                Key::Enter => {
                    if self.tool == Tool::CurvedPath && self.drawing.len() >= 3 {
                        if let Some(f) = self.viewport_frame(vp).cloned() {
                            self.finish_curved_path(vp, &f);
                        }
                    } else if self.tool == Tool::Polygon && self.drawing.len() >= 3 {
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
                Key::U => {
                    self.tool = Tool::CurvedPath;
                    self.drawing.clear();
                }
                Key::X if self.viewports[vp].vr_enabled => {
                    // Scissors on (cut inside the loop) / off.
                    self.sculpt_mode = match self.sculpt_mode {
                        None => Some(crate::sculpt::CutSide::Inside),
                        Some(_) => None,
                    };
                    self.sculpt_lasso.clear();
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
                ui.colored_label(crate::ui::theme::warn_color(ui), msg);
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
                                    .color(crate::ui::theme::ok_color(ui)),
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
                                    .color(crate::ui::theme::ok_color(ui)),
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
        let stem = key_image_stem(&path, frame);
        let dir = key_image_dir();
        if now_key {
            // Keep the image as shown (measurements, colours, fusion), anonymized.
            if let Some(&rect) = self.viewport_rects.get(vp_idx)
                && rect.is_positive()
                && matches!(self.copy_shot, CopyShotState::Idle)
            {
                let label = self.viewport_series_label(vp_idx);
                let image_no = self.viewports[vp_idx].slice + 1;
                if let Some(slot) = self.viewport_callouts.get_mut(vp_idx) {
                    slot.clear();
                }
                self.shot_purpose = ShotPurpose::KeyImage { vp: vp_idx, stem, label, image_no };
                self.copy_shot = CopyShotState::Armed(rect);
            }
        } else {
            let _ = std::fs::remove_file(dir.join(format!("{stem}.png")));
            let _ = std::fs::remove_file(dir.join(format!("{stem}.txt")));
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
        if self.measures.get(&k).is_some_and(|l| !l.is_empty()) {
            self.push_roi_undo(&k);
        }
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

    /// "CT Se 2 (Axial 2.5mm)" for the series shown in `vp_idx`.
    pub(crate) fn viewport_series_label(&self, vp_idx: usize) -> String {
        self.viewports
            .get(vp_idx)
            .and_then(|v| v.selected)
            .and_then(|sel| self.series_at(sel))
            .map(series_label)
            .unwrap_or_default()
    }

    /// Exports the active study's key images as a case report in a folder the user
    /// picks: each key image as shown when it was marked (measurements included,
    /// anonymized), or rendered from the file when no snapshot exists; `report.html`
    /// with the images, their series, image numbers and measurement values, and a
    /// findings box (opens in any browser, prints to PDF); and `case_summary.txt`.
    /// No patient name, ID or dates.
    pub(crate) fn export_key_images_png(&mut self) {
        let vp_idx = self.active_viewport;
        let Some((si, _)) = self.viewports[vp_idx].selected else {
            return;
        };
        let Some(st) = self.study_at(si) else {
            return;
        };
        let study_desc = st.description.clone();
        let mut entries: Vec<ReportEntry> = Vec::new();
        for se in &st.series {
            for (slice_idx, s) in se.slices.iter().enumerate() {
                if self.key_images.contains(&(s.path.clone(), s.frame)) {
                    entries.push(ReportEntry {
                        slice: s.clone(),
                        label: series_label(se),
                        image_no: slice_idx + 1,
                        measure_count: self.measures.get(&(s.path.clone(), s.frame)).map_or(0, Vec::len),
                    });
                }
            }
        }
        if entries.is_empty() {
            return;
        }
        let Some(dir) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        let es = self.lang == crate::i18n::Lang::Es;
        let vp = &self.viewports[vp_idx];
        let snap_dir = key_image_dir();
        let mut cards = Vec::new();
        let mut summary = String::from(if es {
            "SekhmetX — Resumen anonimizado de imágenes clave\n"
        } else {
            "SekhmetX — Anonymized key images summary\n"
        });
        summary.push_str("========================================\n\n");
        for (i, e) in entries.iter().enumerate() {
            let file = format!("key_{:02}.png", i + 1);
            let stem = key_image_stem(&e.slice.path, e.slice.frame);
            let snap = snap_dir.join(format!("{stem}.png"));
            // From the snapshot taken when it was marked: label, image number, then
            // the measurement texts shown.
            let meta: Vec<String> = std::fs::read_to_string(snap_dir.join(format!("{stem}.txt")))
                .map(|t| t.lines().map(str::to_string).collect())
                .unwrap_or_default();
            let wrote = if snap.exists() {
                std::fs::copy(&snap, dir.join(&file)).is_ok()
            } else {
                crate::frame::load_frame(&e.slice.path, e.slice.frame)
                    .ok()
                    .and_then(|f| {
                        let w = match vp.window {
                            Some(req) => f.requested(req).unwrap_or(f.full_window),
                            None => f.default_window(),
                        };
                        crate::export::export_frame_png(
                            &f,
                            w,
                            vp.effective_invert(),
                            &vp.clut_name,
                            vp.convolution_idx,
                            &dir.join(&file),
                        )
                        .ok()
                    })
                    .is_some()
            };
            let measurements: Vec<String> = meta.iter().skip(2).cloned().collect();
            summary.push_str(&format!(
                "{} #{:02} | {} | {} {}\n",
                if es { "Imagen clave" } else { "Key image" },
                i + 1,
                e.label,
                if es { "Imagen" } else { "Image" },
                e.image_no
            ));
            for m in &measurements {
                summary.push_str(&format!("  {m}\n"));
            }
            if measurements.is_empty() && e.measure_count > 0 {
                summary.push_str(&format!(
                    "  {}: {}\n",
                    if es { "Mediciones" } else { "Measurements" },
                    e.measure_count
                ));
            }
            cards.push(ReportCard {
                file: wrote.then_some(file),
                label: e.label.clone(),
                image_no: e.image_no,
                measurements,
                measure_count: e.measure_count,
            });
        }
        let html = report_html(&study_desc, &cards, es, &self.report_template);
        let ok = std::fs::write(dir.join("report.html"), html).is_ok()
            && std::fs::write(dir.join("case_summary.txt"), summary).is_ok();
        let count = cards.len();
        self.message = Some(match (es, ok) {
            (true, true) => format!("Informe exportado: {count} imágenes clave (report.html)"),
            (false, true) => format!("Report exported: {count} key images (report.html)"),
            (true, false) => "No se pudo escribir el informe en esa carpeta".into(),
            (false, false) => "Could not write the report to that folder".into(),
        });
    }

    /// Arm an anonymized HD screenshot of the currently active viewport for clipboard copy.
    pub(crate) fn request_copy_active_viewport(&mut self, ctx: &egui::Context) {
        let Some(&rect) = self.viewport_rects.get(self.active_viewport) else {
            return;
        };
        if !rect.is_positive() {
            return;
        }
        self.shot_purpose = ShotPurpose::Clipboard;
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
                    if let ShotPurpose::KeyImage { vp, stem, label, image_no } =
                        std::mem::replace(&mut self.shot_purpose, ShotPurpose::Clipboard)
                    {
                        let dir = key_image_dir();
                        let _ = std::fs::create_dir_all(&dir);
                        let _ = image::save_buffer(
                            dir.join(format!("{stem}.png")),
                            &bytes,
                            cw as u32,
                            ch as u32,
                            image::ColorType::Rgba8,
                        );
                        let mut meta = format!("{label}\n{image_no}\n");
                        for line in self.viewport_callouts.get(vp).map(Vec::as_slice).unwrap_or(&[]) {
                            meta.push_str(line);
                            meta.push('\n');
                        }
                        let _ = std::fs::write(dir.join(format!("{stem}.txt")), meta);
                        return;
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
        if matches!(self.copy_shot, CopyShotState::Armed(_)) {
            ctx.request_repaint();
        }

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
        self.draw_pacs_modal(&ctx);
        self.draw_cpr_window(&ctx);
        self.draw_calibrate_modal(&ctx);
        self.draw_remove_study_modal(&ctx);
        self.draw_clear_library_modal(&ctx);
        self.draw_arrow_edit_modal(&ctx);
        self.draw_roi_histogram_modal(&ctx);
        self.draw_vr_tf_editor(&ctx);
        self.draw_crash_modal(&ctx);
        self.draw_first_run_notice_modal(&ctx);

        if self.show_detached_library {
            let es = self.lang == crate::i18n::Lang::Es;
            let mut keep_open = true;
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("sekhmetx_detached_library"),
                egui::ViewportBuilder::default()
                    .with_title(if es {
                        "SekhmetX — Biblioteca (Multi-Monitor)"
                    } else {
                        "SekhmetX — Library (Multi-Monitor)"
                    })
                    .with_inner_size([1080.0, 680.0]),
                |vp_ui, _class| {
                    if vp_ui.ctx().input(|i| i.viewport().close_requested()) {
                        keep_open = false;
                    }
                    egui::CentralPanel::default().show(vp_ui, |ui| {
                        self.database_view(ui);
                    });
                },
            );
            self.show_detached_library = keep_open;
        }
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

/// "CT Se 2 (Axial 2.5mm)".
fn series_label(se: &Series) -> String {
    if se.description.is_empty() {
        format!("{} Se {}", se.modality, se.number)
    } else {
        format!("{} Se {} ({})", se.modality, se.number, se.description)
    }
}

/// Where key-image snapshots are kept (in the app's data folder).
fn key_image_dir() -> PathBuf {
    crate::crash_log::app_data_dir().join("key_images")
}

/// A file name for a key image's snapshot, from its file and frame (no patient data).
fn key_image_stem(path: &std::path::Path, frame: u32) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut h);
    frame.hash(&mut h);
    format!("{:016x}", h.finish())
}

struct ReportEntry {
    slice: SliceRef,
    label: String,
    image_no: usize,
    measure_count: usize,
}

/// One key image in the report.
pub(crate) struct ReportCard {
    /// Image file beside `report.html`, if it could be written.
    pub(crate) file: Option<String>,
    pub(crate) label: String,
    pub(crate) image_no: usize,
    pub(crate) measurements: Vec<String>,
    pub(crate) measure_count: usize,
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The case report page: the key images with their series, image numbers and
/// measurement values, a findings box to type into, and a print button. No patient
/// name, ID or dates.
pub(crate) fn report_html(study_desc: &str, cards: &[ReportCard], es: bool, template: &str) -> String {
    let t = |en: &'static str, sp: &'static str| if es { sp } else { en };
    let mut h = String::new();
    h.push_str("<!doctype html>\n<html lang=\"");
    h.push_str(if es { "es" } else { "en" });
    h.push_str("\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>");
    h.push_str(t("Key images report", "Informe de imágenes clave"));
    h.push_str("</title>\n<style>\n\
:root{color-scheme:light;--ink:#1d2330;--dim:#5b6475;--line:#d9dde5;--accent:#2f6fd6}\n\
body{margin:0;background:#f4f5f8;color:var(--ink);font:15px/1.5 system-ui,-apple-system,Segoe UI,Roboto,sans-serif}\n\
main{max-width:980px;margin:0 auto;padding:28px 16px 48px}\n\
h1{font-size:22px;margin:0 0 4px}.sub{color:var(--dim);margin:0 0 20px}\n\
.card{background:#fff;border:1px solid var(--line);border-radius:10px;padding:14px;margin:0 0 18px;break-inside:avoid}\n\
.card img{display:block;max-width:100%;height:auto;border-radius:6px;background:#000;margin:0 auto}\n\
.meta{font-weight:600;margin:10px 0 4px}.meta span{color:var(--dim);font-weight:400}\n\
ul{margin:4px 0 0;padding-left:20px}li{font-family:ui-monospace,Consolas,monospace;font-size:13.5px}\n\
.findings{min-height:120px;border:1px solid var(--line);border-radius:8px;padding:10px;background:#fff;white-space:pre-wrap}\n\
.findings:empty:before{content:attr(data-placeholder);color:var(--dim)}\n\
.bar{display:flex;gap:10px;align-items:center;justify-content:space-between;margin-bottom:18px;flex-wrap:wrap}\n\
button{font:inherit;border:1px solid var(--accent);background:var(--accent);color:#fff;border-radius:7px;padding:6px 14px;cursor:pointer}\n\
footer{color:var(--dim);font-size:12.5px;margin-top:24px}\n\
@media print{body{background:#fff}button{display:none}.card{border-color:#bbb}}\n\
</style></head><body><main>\n<div class=\"bar\"><div><h1>");
    h.push_str(t("Key images report", "Informe de imágenes clave"));
    h.push_str("</h1><p class=\"sub\">");
    if !study_desc.is_empty() {
        h.push_str(&html_escape(study_desc));
        h.push_str(" · ");
    }
    h.push_str(&format!("{} {}", cards.len(), t("key images", "imágenes clave")));
    h.push_str("</p></div><button onclick=\"window.print()\">");
    h.push_str(t("Print / save as PDF", "Imprimir / guardar como PDF"));
    h.push_str("</button></div>\n");
    for (i, c) in cards.iter().enumerate() {
        h.push_str("<section class=\"card\">");
        if let Some(f) = &c.file {
            h.push_str(&format!("<img src=\"{}\" alt=\"{} {}\">", html_escape(f), t("Key image", "Imagen clave"), i + 1));
        }
        h.push_str(&format!(
            "<p class=\"meta\">{} {} — {} <span>· {} {}</span></p>",
            t("Key image", "Imagen clave"),
            i + 1,
            html_escape(&c.label),
            t("image", "imagen"),
            c.image_no
        ));
        if !c.measurements.is_empty() {
            h.push_str("<ul>");
            for m in &c.measurements {
                h.push_str(&format!("<li>{}</li>", html_escape(m)));
            }
            h.push_str("</ul>");
        } else if c.measure_count > 0 {
            h.push_str(&format!("<p>{}: {}</p>", t("Measurements", "Mediciones"), c.measure_count));
        }
        h.push_str("</section>\n");
    }
    h.push_str("<h2 style=\"font-size:17px\">");
    h.push_str(t("Findings", "Hallazgos"));
    h.push_str(&format!(
        "</h2><div class=\"findings\" contenteditable=\"true\" data-placeholder=\"{}\">{}</div>\n",
        t("Type your findings here before printing…", "Escriba aquí sus hallazgos antes de imprimir…"),
        html_escape(template.trim())
    ));
    h.push_str("<footer>");
    h.push_str(t(
        "Made with SekhmetX. Anonymized: no patient name, ID or dates. Not a medical device; not for primary diagnosis.",
        "Hecho con SekhmetX. Anonimizado: sin nombre, ID ni fechas del paciente. No es un dispositivo médico; no apto para diagnóstico primario.",
    ));
    h.push_str("</footer>\n</main></body></html>\n");
    h
}

#[cfg(test)]
mod report_tests {
    use super::*;

    #[test]
    fn report_lists_images_and_measurements_and_escapes_text() {
        let cards = vec![
            ReportCard {
                file: Some("key_01.png".into()),
                label: "CT Se 2 (Axial <2.5mm>)".into(),
                image_no: 41,
                measurements: vec!["2.31 cm (23.1 mm)".into(), "Mean: 45.2 HU  SD: 9.80".into()],
                measure_count: 2,
            },
            ReportCard { file: None, label: "PT Se 50".into(), image_no: 7, measurements: vec![], measure_count: 1 },
        ];
        let html = report_html("CT TORAX", &cards, false, "IMPRESSION:\n- Normal");
        assert!(html.contains("<img src=\"key_01.png\""));
        assert!(html.contains("Axial &lt;2.5mm&gt;"), "names are escaped");
        assert!(html.contains("<li>2.31 cm (23.1 mm)</li>"));
        assert!(html.contains("Measurements: 1"));
        assert!(html.contains("2 key images"));
        assert!(html.contains("IMPRESSION:"));
        let es = report_html("", &cards, true, "");
        assert!(es.contains("Hallazgos") && es.contains("Imprimir"));
    }
}
