# Changelog

All notable changes to **SekhmetX** (formerly RDicom Viewer) are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.5.0] - 2026-11-16

### Added (3D Volume Rendering — Stage 5)
- **3D Clipping / Crop Box (`src/gpu_view.rs`, `src/ui/toolbar.rs`, `src/app/mod.rs`):** Added `✂ Clip` sliders (`X / Y / Z` min/max bounds) to intersect 3D rays against the cropped sub-volume, plus **Reset Clip**.
- **3D Background Color Controls (`src/gpu_view.rs`, `src/ui/toolbar.rs`):** Added `🎨 BG` presets (`Black`, `Dark Slate`, `Deep Navy`, `White`) and custom RGB picker.
- **3D Anatomical Orientation Cube Widget (`src/app/viewport.rs`):** Added live `L/R`, `A/P`, and `S/I` axis widget in the bottom-right corner of 3D VR/MIP viewports.

## [0.4.10] - 2026-11-16

### Added (3D Volume Rendering — Stage 4)
- **Interactive Histogram & 1D Transfer-Function Curve Editor (`src/app/modals.rs`, `src/imaging/vr_presets.rs`, `src/gpu_view.rs`):**
  - Added cached 128-bin log-scaled HU histogram behind a draggable piecewise-linear opacity/color curve (`📈 TF`).
  - Supports dragging control points, double-clicking to add stops, right-clicking (or `🗑`) to remove interior stops, RGB color editing, and dirty-hashed GPU LUT uploads.

## [0.4.9] - 2026-11-16

### Added & Fixed (Comparison Linking & Zoom Modes)
- **Two Zoom-Link Modes (`src/app/mpr_sync.rs`, `src/ui/toolbar.rs`):** Added **`= mm`** (same physical FOV size across viewports) vs **`Rel`** (preserve relative zoom at moment of link) when `🔗 Zoom` is active.
- **Fixed `⚓ Lock 3D Sync`:** Now automatically ensures a default link pair and turns on `🔗 Cut` (`link_cut = true`).
- **`⚖ Compare` / `⚖ Comparar` Mode (`src/app/mpr_sync.rs`, `src/ui/toolbar.rs`):** One-click comparison links Zoom (`= mm`), WL/WW, and Cut, matches WL/WW/pan/zoom, enables 3D MPR on linked volumes, and aligns 3D XYZ planes via `RigidTransform3D`.

## [0.4.8] - 2026-11-15

### Added (Productivity & Clipboard)
- **`Ctrl+C` / `Cmd+C` Anonymized HD Screenshot (`src/app/mod.rs`, `src/app/overlays.rs`):** Captures the active 2D/MPR/VR viewport at full display resolution with patient PHI stripped and copies it to the OS clipboard via `arboard`.
- **Right-Click Viewport Context Menu (`src/app/viewport.rs`):** Clean right-click menu with Copy Anonymized Image, Export, 3D MPR/VR/Ax-Cor-Sag toggles, WL/WW Presets, Transform, Clear ROIs, and Reset Viewport.
- **Drag-and-Drop Series / Cuts (`src/app/browser.rs`, `src/app/viewport.rs`):** Drag any series or compact thumbnail directly onto any empty or loaded viewport.

## [0.4.7] - 2026-11-15

### Added & Fixed (3D Volume Rendering — Stage 3)
- **3D Gradient Normals & Blinn-Phong Shading (`src/gpu_view.rs`, `src/ui/toolbar.rs`):** Added physical-aspect central-difference gradient estimation and studio key-light Blinn-Phong shading with **`☀ Shading`** toggle and **Ambient / Diffuse / Specular** sliders.
- **Centered Settings Cog (`src/app/mod.rs`):** Custom painted `24×18` pill in the status bar centering `⚙`.

## [0.4.6] - 2026-11-15

### Added (3D Volume Rendering — Stage 2)
- **1D RGBA Transfer Function LUTs & OsiriX-Style 3D Presets (`src/imaging/vr_presets.rs`, `src/gpu_view.rs`, `src/ui/toolbar.rs`):** Added `Default`, `Bone`, `Angio / Vascular`, `Soft Tissue`, and `Lung / Airway` presets with linearly interpolated LUT sampling and exponential opacity correction.

## [0.4.5] - 2026-11-15

### Improved (Toolbar & Settings)
- Moved Settings (`⚙`) and update indicator to the bottom-right status bar; removed the `App` toolbar group.
- Added **Updates** mode controls (`Check on startup` / `Manual only` + `Check now`) in Settings and a **Language** selector to the first-run notice modal.

## [0.4.4] - 2026-11-15

### Improved (Non-Fullscreen Layout)
- Inlined the slice scrubber & MPR/VR strip onto the second toolbar row when wrapped groups leave room, eliminating the mostly-empty third row.

## [0.4.3] - 2026-11-15

### Improved (Responsive Toolbar, Library Unload & Compact Sidebar)
- Replaced the `»` overflow popup with dynamic multi-row toolbar wrapping and mode-specific group/button hiding (Library vs 2D/MPR vs 3D VR).
- Moved highlighted **`← Library`** / **`← Biblioteca`** to the top-left toolbar (unloading decoded frames/volumes on return).
- Added **Compact Sidebar Mode** (`96 px` thumbnail strip of the current patient's acquisitions with `AX`/`COR`/`SAG` cut badges).

## [0.4.2] - 2026-11-15

### Fixed (3D VR Orientation & Rotation)
- Derived the 3D VR camera basis (`v_left`, `v_sup`, `v_ant`) from `Volume3D`'s DICOM patient-LPS vectors so Superior/head is always up and Inferior/pelvis is always down.
- Wired View-menu `Rotate 90° CCW/CW` and `Flip H/V` into `VolumeVrCallback`.

## [0.4.1] - 2026-11-15

### Added (3D Volume Rendering — Stage 1)
- Added WGSL GPU 3D ray-caster (`VR` alpha-compositing & `3D MIP`), `🧊 VR` toolbar toggle, `VR | MIP | Front | Left | Top` strip, and interactive 3D orbit/zoom/WL.

## [0.4.0] - 2026-11-15

### Added (OsiriX-Inspired UI Overhaul)
- Full-screen Database Library view, OsiriX-style grouped toolbar, multi-viewport link groups (`1–4`) with colored badges, and side-by-side DICOM Meta-Data Inspector.

---

## [0.2.7] - 2026-11-14

### Improved (Default Scroll, Auto-3D MPR & Plane Hotkeys)
- **Default Scroll Tool (`src/app.rs`):** SekhmetX now opens with the **Scroll** tool active instead of Window/Level.
- **Auto-3D on MPR-Capable Series (`src/app.rs`):** Opening an MPR-capable series automatically enables 3D MPR once its volume finishes assembling (while still allowing manual toggle-off).
- **`A` / `C` / `S` Plane Hotkeys (`src/app.rs`, `README.md`):** Bound **`A`** (Axial), **`C`** (Coronal), and **`S`** (Sagittal) to switch planes (using native sibling series in the MPR group when available).

---

## [0.2.6] - 2026-11-14

### Improved (Native-Resolution MPR Plane Switching & Version Badge)
- **Native High-Definition `Ax` / `Cor` / `Sag` Switching (`src/app.rs`, `src/ui/toolbar.rs`):**
  - Clicking `Ax`, `Cor`, or `Sag` (or opening `⊞ Ax·Cor·Sag` 3-pane Orthogonal MPR) now automatically switches to the matching native Axial / Coronal / Sagittal sibling series in the MPR group when available, preserving WL/WW, CLUT, invert, convolution, and anatomical slice position — delivering full native scanner resolution instead of reslicing a thicker axis.
- **Status Bar Version Badge (`src/app.rs`):**
  - Added the build version (`v0.2.6`) at the bottom-right corner of the status bar, and localized the empty-viewport drop hint.

---

## [0.2.5] - 2026-11-14

### Added (P2 Step 1 — Export & DICOM Anonymization)
- **Image, Series & Anonymized DICOM Export (`src/export.rs`, `src/ui/toolbar.rs`, `src/app.rs`):**
  - Added **Export Image (PNG)**, **Export Series (PNGs)**, and **Export Anonymized DICOM** under `View ▾ -> Export` (localized in English & Spanish).
  - De-identification strips private tags (odd DICOM groups), removes sensitive address/telephone/comment tags, replaces patient/institution/accession identifiers, clears study/birth dates, and remaps `StudyInstanceUID`, `SeriesInstanceUID`, `SOPInstanceUID`, and `FrameOfReferenceUID` consistently across exported series.

---

## [0.2.4] - 2026-11-14

### Added & Improved (P1 Hardening & Parity)
- **Managed Library & Persistent ZIP Studies (`src/db.rs`, `src/app.rs`):**
  - Added `CopyToLibraryPolicy` (`Ask` / `Always` / `Never`) with an inline prompt when opening `.zip` archives to copy studies into `<data dir>/SekhmetX/Studies/<StudyUID>/`.
  - Added library disk usage readout (MB), `"Clear"` library button, and managed-copy cleanup on `"Remove from library"` / `"Clear"`.
- **SQLite Schema v2 & Missing-File Handling (`src/db.rs`, `src/dicom_index.rs`):**
  - Added `PRAGMA user_version = 2` migration adding `modality`, `image_count`, `comments`, `date_added`, `last_opened`, `managed_dir` columns and the `rois` table.
  - Studies whose source files moved on disk now remain visible with a `⚠ [files missing]` badge instead of disappearing silently.
- **Persistent ROIs (`src/db.rs`, `src/app.rs`, `src/ui/toolbar.rs`):**
  - ROIs (`Length`, `Angle`, `Oval`, `Rect`, `Polygon`, `Pencil`, `Point`) are automatically saved to and restored from SQLite per `(path, frame)` key.
- **Complete Bilingual UI (`src/i18n.rs`, `src/ui/toolbar.rs`, `src/app.rs`):**
  - Routed remaining toolbar, view menu, ROI, and ZIP copy prompt strings through `Strings` (`EN` / `ES`) with zero unused-field warnings and an EN/ES non-empty parity test.
- **CLI ROI Comparison Mode & Documentation (`src/main.rs`, `DEVIATIONS.md`, `porting/map.toml`, `PORTING_MAP.md`):**
  - Added `sekhmetx --roi` to compute area, mean, SD, min, max (for `ellipse`/`rect`) and length in mm (for `length`) for direct comparison with Horos.
  - Added `DEVIATIONS.md`, ported additional IsiXROI edge-case tests, and updated `porting/map.toml` + `PORTING_MAP.md`.

---

## [0.2.3] - 2026-11-14

### Fixed & Hardened (Correctness Pass)
- **Native-Plane MPR & Sibling Blending (`src/volume.rs`, `src/app.rs`):**
  - Native-plane MPR (`SlabMode::Thin`) now returns exact bit-identical source slice voxels and source slice thickness (`Thk`), eliminating resampling blur ("3D loses detail").
  - Oblique MPR now samples only the viewport's own volume by default; multi-series sibling blending moved behind an explicit off-by-default `Blend Siblings` toggle that displays `"Blended from N series (ROI stats disabled)"` and disables ROI pixel statistics while active.
- **On-Screen Readouts & GPU Interpolation (`src/app.rs`, `src/gpu_view.rs`, `src/frame.rs`):**
  - Fixed duplicate `"HU HU"` suffix in status bar cursor readout and shifted the `AXIAL`/`CORONAL`/`SAGITTAL` plane badge below the top orientation letter.
  - Restored Horos-style post-windowing bilinear interpolation in the WGSL fragment shader with a View-menu toggle (`Smooth Interpolation (Bilinear)`).
  - Kept 2D spatial convolution presentation-only so cursor readouts and ROI statistics always read the unconvolved source pixels (matching IsiX `DCMPix+DisplaySource`).
- **3D Volume Validation & Tilted Gantry (`src/volume.rs`, `src/app.rs`):**
  - Ported IsiX `SliceIntervalRegularity` into `Volume3D::from_series_checked` (`Result<Arc<Self>, VolumeError>`) to detect missing slices, duplicates, variable intervals, dimension mismatches, and missing geometry while allowing regular tilted-gantry stacks.
  - Inverted the full 3×3 `(row·dx, col·dy, slice·dz)` basis matrix in `Volume3D::lps_to_voxel` for exact round-trip accuracy under gantry tilt.
  - Surfaced exact irregularity reasons in the Force-3D warning dialog, added `"Geometry not verified: measurements disabled"` banner when forced on unverified geometry, and kept uncalibrated (`PixelSpacing`-less) images in `px` rather than `mm`.
- **Non-Blocking Cross-Study Registration (`src/app.rs`, `src/volume.rs`):**
  - Moved `RigidTransform3D::register` onto a worker thread (`"Aligning…"` status), short-circuited matching `FrameOfReferenceUID` to identity, computed one transform + exact inverse, and displayed alignment score / position-sync fallback banners.
- **Hardened Auto-Updater & ZIP Import (`src/updater.rs`, `src/dicom_index.rs`, `.github/workflows/release.yml`):**
  - Added `SHA256SUMS` generation in CI and streaming-to-disk SHA-256 verification before `self_replace`, plus a startup-check toggle and unsigned-build notice in the Update modal.
  - Replaced the hand-written ZIP parser and `tar` fallback with streaming `zip::ZipArchive` (`enclosed_name()`, ZIP64, entry/size caps, per-process temp dir, and stale cache cleanup).

---

## [0.2.2] - 2026-11-14

### Added
- Rebranded project, binary (`sekhmetx`), and GitHub repository (`CosmicTaco22/SekhmetX`) to **SekhmetX**.
- Added crash-safe SQLite database (`src/db.rs`, WAL mode) for study/series indexing and user settings.
- Added English / Spanish (`EN` / `ES`) localization (`src/i18n.rs`).
- Added in-app GitHub Releases auto-updater (`src/updater.rs`).

---

## [0.2.1] - 2026-11-14

### Fixed
- Fixed 686-image Angio stack missing 3D MPR & jumping slices (`max_focus_len.max(512)` queue radius, native normal alignment).
- Locked native-aligned MPR to exact native dimensions and in-plane spacing.
- Separated MIP vs non-MIP stacks in Study Browser MPR grouping.

---

## [0.2.0] - 2026-11-14

### Added
- **In-Place Single-Viewport 3D MPR & 3D Trackball Rotation (`src/volume.rs`, `src/app.rs`):**
  - Load and slice a 3D volume directly inside any single viewport cell (`🧊 3D MPR`) without opening three separate orthogonal sub-windows.
  - Interactive **3D Trackball Rotation (`3D Rotate` / `O`)**: drag horizontally or vertically to orbit the 3D slicing plane through any oblique angle; hold `Shift` while dragging to roll/tilt in-plane.
  - One-click **Axial (`Ax`)**, **Coronal (`Cor`)**, **Sagittal (`Sag`)**, and **Native (`Nat`)** plane buttons.
  - **Thick-Slab Projections (`Slab: Thin / MIP / MinIP / Mean`)** with an interactive `1.0–50.0 mm` slab thickness slider and parallel ray-marched slab integration.
- **Computational Cross-Study 3D Synchronization & 6-DOF Rigid Registration (`src/volume.rs`):**
  - Automatic 3D foreground anatomical centroid detection (filtering background air) and multi-scale 6-DOF (3D translation + 3D rotation) **Normalized Cross-Correlation (NCC)** rigid registration (`RigidTransform3D`).
  - Automatically locks two comparison studies (e.g., longitudinal brain CT/MRI scans from different dates or scanners) to the same anatomical height and 3D cut angle across Axial, Coronal, Sagittal, and Oblique 3D rotations.
  - Manual **`⚓ Lock 3D Sync`** button in the toolbar to lock or fine-tune any custom 3D slice/angle alignment between comparison viewports.
- **Automatic Plane Classification & Acquisition-Time MPR Auto-Grouping (`src/dicom_index.rs`):**
  - Automatically identifies the dominant anatomical plane (`[AX]`, `[COR]`, `[SAG]`, `[OBL]`) of every series from `ImageOrientationPatient` cosines and flags 3D-capable stacks with `·MPR`.
  - Automatically detects separated Axial, Coronal, and Sagittal reconstructions acquired at the same time (`AcquisitionTime` / `ContentTime` / `SeriesTime`) and groups them under a **`🧊 MPR [AX·COR·SAG]`** entry in the Study Browser with quick `AX`, `COR`, `SAG` buttons.
  - **Hybrid Multi-Series Tri-Planar Resolution Fusion**: blends high-resolution in-plane data from sibling Axial, Coronal, and Sagittal stacks weighted by $\cos^4(\theta)$ during 3D oblique MPR slicing.
- **Full Horos / IsiX Regions of Interest (`IsiXROI`) & Interior Pixel Statistics (`src/roi/`):**
  - Ported `IsiXROI` (`ROIPlanarGeometry`, `ROIBoundingBox`, `ROIDragConstraint`, `ROISpline`, `ROIPixelStatistics`, `ROIValueStatistics`, `CalciumScoring`, `ROILengthRatio`, `ROIRasterConversion`, `ROIScanline`, `ROIPolygonClip`, `ROIPolygonFill`, `ROIMasks`) with 20 unit tests.
  - Interactive ROI tools & shortcuts:
    - **`Length` (`L` / `M`)**: click-drag or 2-click calibrated distance in `mm` / `cm`.
    - **`Angle` (`A`)**: 3-click or drag-then-click angle measurement in degrees.
    - **`◯ Oval` (`E`)** & **`▭ Rect` (`B`)**: calibrated Area ($\text{cm}^2 / \text{mm}^2$), Perimeter, Mean, SD, Min, and Max (`HU`).
    - **`⬠ Poly` (`G`)**: multi-vertex closed polygon ROI (`Enter` or click start vertex to close).
    - **`✏ Pencil` (`F`)**: freehand ROI with Numerical Recipes cubic spline smoothing.
    - **`⌖ Point` (`T`)**: 2D point probe displaying calibrated intensity (`HU`) and 3D patient `XYZ` (`mm`) coordinates.
  - Horos-style dark translucent ROI callout boxes and `🗑 Clear ROIs` toolbar action.
- **31 Horos Colour Lookup Tables (`CLUT`) & 22 Horos 2D Spatial Convolution Filters (`src/imaging/`):**
  - Ported `ColorLookupTable`, `ColourTableState`, `ColourTableBook`, and all **31 factory Horos CLUTs** (`PET`, `HotIron`, `NIH`, `Jet`, `Spectrum`, `Rainbow`, `BlackBody`, `French`, `Perfusion`, `GEcolor`, `PPU Viridis`, `PPU Inferno`, `PPU Magma`, `PPU Plasma`, `VR Bones`, `VR Muscles-Bones`, `VR Red Vessels`, etc.) on both GPU (`wgpu` WGSL shader) and CPU pipelines.
  - Ported `ImageConvolution`, `SeriesConvolution`, `ConvolutionKernelBook`, and all **22 factory Horos 2D convolution presets** (`Bone Filter 3x3`, `Sharpen 3x3`, `Sharpen 5x5`, `Basic Smooth 5x5`, `Gaussian blur`, `Blur 3x3`, `Blur 5x5`, `Lowpass`, `Highpass 5x5`, `Edge 3x3`, `Laplacian`, `Hat`, `Emboss`, etc.).
- **Multi-Series Visual Layout Picker, Navigation Controls & Horos Study Browser (`src/navigation.rs`, `src/app.rs`):**
  - Interactive **`4 × 4` visual square-grid layout selector (`⊞ Layout`)** (`1×1` up to `4×4` / 16 viewports).
  - Non-wheel slice & series navigation: `Scroll` drag tool (`S`), toolbar **`◀ Se` / `Se ▶`**, **`⏮ ◀ [Slice Slider] ▶ ⏭`**, and an on-viewport floating bottom scrubber bar.
  -Ported `IsiXGeometry` (`Vector3`, `Line3`, `Plane3`, `AffineTransform3D`, `PlanarGeometry`, `PackedVectors`, `BezierCore`) and `IsiXImaging` geometry (`ImagePlaneGeometry`, `ImagePlaneCoordinates`, `PlaneGeometry`, `LineGeometry`, `ViewTransform`, `ViewportState`, `OrientationLetters`, `SliceSynchronizer`, `ReferenceLines`) with 3D cross-reference scout cut lines and `A/P/R/L/S/I` edge orientation letters.
  - Study Browser **Search bar (`🔍`)**, **Modality filter pills (`All`, `CT`, `MR`, `US`, `DX/CR`, `PT/NM`)**, and **live rendered 38×38 DICOM series mini-thumbnails**.
- **Non-Destructive `.zip` Archive Support (`src/dicom_index.rs`):**
  - Open or drag-and-drop `.zip` patient archives directly; extracts into an isolated temporary folder (`%TEMP%\rdicom-zip-cache\zip-<hash>`) without modifying the original `.zip` and cleans up automatically on exit.

---

## [0.1.0] - 2026-11-13

### Added
- Initial release of **RDicom Viewer** for Windows (`eframe`/`egui` + `wgpu` + `dicom-rs`).
- Recursive DICOM folder and file indexing grouped by Patient, Study, and Series (splitting sub-series by geometry/orientation).
- Ported IsiX window/level pipeline (`window`, `voi_lut`, `defaults`, `photometry`, `drag`, `presets`) with GPU (`wgpu`) and CPU rendering paths.
- Support for uncompressed, JPEG baseline/lossless, JPEG 2000 (OpenJPEG), JPEG-LS (CharLS), RLE, and Deflate transfer syntaxes.
- Command-line utilities (`--list`, `--check`, `--render`) and synthetic DICOM test fixture generator (`tools/make_test_dicom.py`).
- Upstream IsiX/Horos porting map and tracker (`porting/map.toml`, `PORTING_MAP.md`, `tools/upstream.py`).
