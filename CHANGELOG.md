# Changelog

All notable changes to **SekhmetX** (formerly RDicom Viewer) are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.9.0] - 2026-11-21

### Added
- **Classic PACS Receive (`C-MOVE` SCU + local `C-STORE` SCP listener) & TLS config** (`src/net/dimse.rs`, `src/app/modals.rs`, `src/app/mod.rs`):
  - Start/stop a built-in pure-Rust DICOM `C-STORE` SCP listener on any local port (default `11112`) that accepts incoming studies, writes Part-10 `.dcm` files, and opens them automatically.
  - Added Study-Root `C-MOVE` retrieve button on `C-FIND` results, plus a `TLS` flag in the DIMSE PACS node editor.
- **2D Region Growing, ROI Interpolation & CSV Export** (`src/roi/mod.rs`, `src/app/mod.rs`, `src/ui/toolbar.rs`):
  - **ROI → Grow Region (from Point ROI)** (`Crecer Región`): 4-connected flood-fill segmentation around a placed Point/Probe ROI producing a closed polygon contour.
  - **ROI → Interpolate ROIs Between Slices** (`Interpolar ROIs entre Cortes`): linearly interpolates closed ROIs across intermediate empty slices for fast 3D volumetry.
  - **ROI → Export ROIs (.csv)…**: exports all study ROIs and statistics to a CSV spreadsheet.
- **4D Dynamic / Cardiac Phase Controls** (`src/app/mod.rs`, `src/ui/toolbar.rs`): automatic detection of multi-phase 4D stacks sharing Z positions, with **4D Phase ◀ / ▶** stepping in the View menu preserving anatomical slice position.
- **Library Smart Albums** (`src/app/browser.rs`): added **Today**, **Last 7 Days**, **Last 30 Days**, and **Key Images** smart albums in the Database Library sidebar.
- **MP4 / AVI Video Export** (`src/export.rs`, `src/app/mod.rs`, `src/ui/toolbar.rs`): added **Export Series (MP4 / AVI Video)…** using `ffmpeg` (H.264 `.mp4`) when available, with a zero-dependency pure-Rust 24-bit RGB `.avi` fallback.
- **Editable DICOM Metadata** (`src/app/metadata.rs`): inline **Edit DICOM Tag** panel inside the Metadata Inspector to update and save `StudyDescription`, `SeriesDescription`, `PatientName`, `AccessionNumber`, and `InstitutionName` directly to the `.dcm` file.
- **Multi-Monitor Detached Library Window** (`src/app/mod.rs`, `src/ui/toolbar.rs`): **View → Detach Library Window (Multi-Monitor)** opens the Study Library in an independent OS window for dual-monitor setups.
- **3-Plane Orthogonal MPR PET-CT Fusion** (`src/ui/toolbar.rs`, `src/app/viewport.rs`): toggling PET-CT Fusion while Orthogonal MPR is active now syncs the LPS-resliced PET overlay across all 3 orthogonal planes (Axial, Coronal, Sagittal) simultaneously.

## [0.8.0] - 2026-11-21

### Added
- **Classic PACS (DIMSE C-FIND & C-STORE) + DICOMweb STOW-RS** (`src/net/dimse.rs`, `src/net/dicomweb.rs`, `src/app/modals.rs`):
  - Pure-Rust Study-Root **C-FIND SCU** for searching studies on classic DICOM PACS peers.
  - Pure-Rust **C-STORE SCU** and **DICOMweb STOW-RS** for sending the active study directly to a PACS peer or DICOMweb server.
- **Digital Image Subtraction & Agatston Calcium Scoring UI** (`src/imaging/subtraction.rs`, `src/roi/statistics.rs`, `src/app/mod.rs`, `src/ui/toolbar.rs`):
  - **Subtract Series (Mask)** (`Sustraer Serie (Máscara)`) in the View menu computes a live `[SUB]` (`post - mask`) series for DSA and contrast-enhanced CT/MR.
  - **Agatston Calcium Score (≥ 130 HU)** (`Puntaje de Calcio Agatston`) in the ROI menu computes total Agatston score, calcified volume (`mm³`), and approximate mass (`mg`) across closed ROIs on the active CT series.
- **Expanded 3D Volume Rendering Presets & 3D Surface STL Export** (`src/imaging/vr_presets.rs`, `src/imaging/stl.rs`, `src/app/mod.rs`, `src/ui/toolbar.rs`):
  - Added 4 clinical 3D VR presets (**Coronary CTA**, **Muscle / Skin**, **Colon**, **Endoscopy** — 9 total).
  - Added **Export 3D Surface (.stl)…** (`Exportar Superficie 3D (.stl)…`) using Surface Nets iso-surface extraction in patient millimeter (LPS) space to write binary `.stl` meshes.
- **Customizable Report Templates & Keyboard Shortcuts** (`src/settings.rs`, `src/app/modals.rs`, `src/app/mod.rs`):
  - Configure a custom structured radiology report template in Settings (pre-filled automatically in exported `report.html` Key Images reports).
  - Customize single-key tool & action shortcuts (`wl`, `scroll`, `pan`, `zoom`, `length`, `oval`, `reset`, `key_image`, `pacs`) in Settings with one-click reset to defaults.

## [0.7.6] - 2026-11-21

### Added
- **Auto-apply Hanging Protocols on study open** (`src/app/mod.rs`, `src/app/browser.rs`): opening a study from the Library now automatically checks saved per-modality hanging protocols, sets the configured grid layout (`1×1`–`4×4`), assigns series across viewports, and applies Auto-MPR if enabled.
- **Native DICOMweb & PACS Query/Retrieve client** (`src/net/dicomweb.rs`, `src/net/dimse.rs`, `src/settings.rs`, `src/app/modals.rs`, `src/app/browser.rs`, `src/ui/toolbar.rs`):
  - **DICOMweb (QIDO-RS / WADO-RS)**: add and persist DICOMweb servers (with optional session-only Bearer/Basic token), search studies by patient/ID/modality/date via QIDO-RS, and download & open studies directly via WADO-RS `multipart/related`.
  - **DICOM PACS (DIMSE C-ECHO)**: pure-Rust PS 3.8 DICOM Upper Layer `A-ASSOCIATE-RQ` / `C-ECHO-RQ` SCU connectivity test with round-trip latency measurement, plus persistent PACS node configuration in SQLite (no hardcoded servers or credentials).
  - Accessible from the Database Library bar (**PACS / DICOMweb…**) and the View menu (**PACS / DICOMweb Query & Retrieve…** / **Consultar y Descargar PACS / DICOMweb…**).

## [0.7.5] - 2026-11-21

### Added
- **ADC & T2 Parametric Map generator** (`src/app/mod.rs`, `src/imaging/mr_maps.rs`, `src/ui/toolbar.rs`): added **Generate ADC Map** (`Generar Mapa ADC`, in $10^{-6}\text{ mm}^2/\text{s}$) and **Generate T2 Map** (`Generar Mapa T2`, in `ms`) in the View menu for MR series, building live in-memory map series from multi-b DWI or multi-echo stacks.
- **Hanging Protocols Settings UI** (`src/app/modals.rs`, `src/app/mod.rs`): configure per-modality grid layouts (`1×1`–`4×4`) and auto-MPR preferences in Settings (persisted in SQLite).
- **ROI Undo (`Ctrl+Z` / `Cmd+Z`)** (`src/app/mod.rs`, `src/ui/toolbar.rs`): undo deleting or clearing ROIs via keyboard shortcut or **Undo ROI (Ctrl+Z)** (`Deshacer ROI (Ctrl+Z)`) in the ROI menu.
- **Direct `DICOMDIR` file opening** (`src/dicom_index.rs`): opening or dropping a hospital CD/DVD `DICOMDIR` file now automatically indexes its parent folder while skipping the `DICOMDIR` record file itself.

## [0.7.4] - 2026-11-21

### Added
- **DWI b-value splitting in the viewer** (`src/imaging/mr_maps.rs`, `src/app/mod.rs`, `src/ui/toolbar.rs`): added **Split DWI by b-value** (`Separar DWI por valor b`) in the View menu for MR series, reading standard/Siemens/GE/Philips b-value tags and creating one sub-series per b-value (`b=0`, `b=500`, `b=1000`, …).
- **3D ROI Volume (`cm³`)** (`src/roi/mod.rs`, `src/app/mod.rs`, `src/ui/toolbar.rs`): added **Compute ROI Volume (cm³)** (`Calcular Volumen ROI (cm³)`) in the ROI menu, integrating closed ROI areas (Oval, Rect, Polygon, Pencil) across slices with trapezoidal integration and half-slice end caps.
- **Custom Window / Level presets & Hanging Protocol storage** (`src/settings.rs`, `src/app/modals.rs`, `src/ui/toolbar.rs`): users can now add and delete custom `(Name, WL, WW)` presets in Settings; they persist in SQLite and appear directly in the toolbar W/L preset menu.
- **Virtual Endoscopy / Fly-Through** (`src/imaging/endoscopy.rs`, `src/app/cpr_view.rs`): perspective surface ray-casting camera flying along any Curved MPR centerline with Phong headlight shading and an adjustable HU iso-surface threshold (`-900..400 HU`), toggled via **Fly-through** (`Endoscopia`) inside the Curved MPR window.

## [0.7.3] - 2026-11-21

### Added
- **Structured Report (SR) & GSPS (`PR`) viewer** (`src/app/viewport.rs`, `src/imaging/sr.rs`): selecting a DICOM SR (e.g. CT dose report) or GSPS (`PR`) series now renders a readable, scrollable report/summary inside the viewport with a **Copy text** button, and GSPS adds **Apply to other viewports** (window, rotation, horizontal flip, invert) in English and Spanish.
- **Animated GIF stack / cine export** (`src/export.rs`, `src/app/mod.rs`, `src/ui/toolbar.rs`, `src/i18n.rs`): export any series or cine loop as a looping animated GIF (`Export Series (Animated GIF)…` / `Exportar Serie (GIF Animado)…`) at the active cine FPS with current W/L, CLUT, invert, and convolution (downscaled to at most 512 px per side).
- **Compressed `.zip` library storage** (`src/db.rs`, `src/frame.rs`, `src/app/mod.rs`): wired up the *"Compressed (.zip)"* setting in Settings so studies copied into the library save into `Studies/<uid>/study.zip` and stream directly from the archive without re-extracting.
- **Curved MPR transverse cross-section view** (`src/imaging/cpr.rs`, `src/app/cpr_view.rs`): added a **Cross-section** (`Transversal`) toggle and slider in the Curved MPR window showing the live perpendicular slice along the path with a crosshair and vertical position line.
- **Advanced MR mapping** (`src/imaging/mr_maps.rs`): ported IsiX's `BValueSplit` (Siemens/GE/Philips tags + grouping) and `T2FitMapping` echo grouping, plus log-linear **T2** (ms) and **ADC** ($10^{-6}\text{ mm}^2/\text{s}$) map fitting with unit tests.

## [0.7.2] - 2026-11-21

### Improved
- **Curved MPR thick-slab and saved paths** (`src/imaging/cpr.rs`, `src/app/cpr_view.rs`, `src/db.rs`): added **Thin / MIP / MinIP / Mean** thick-slab projection (2–50 mm, parallelized with `rayon`) along the curve binormal, plus **Save / Load / Delete** named CPR paths per series in the SQLite library (`cpr_paths` table), with English and Spanish labels.
- **Sharper 3D scissors** (`src/sculpt.rs`): raised the sculpting mask resolution from 128³ to **256³** (8× more cells / 2× sharper edges on every axis) and parallelized `SculptMask::cut` across slices with `rayon`.

### Added (P2 / P3 groundwork)
- **Ultrasound region calibration (`USRegion`)** (`src/imaging/us_region.rs`, `src/frame.rs`): ported IsiX's `USRegion.swift` and its 4 unit tests, and wired `SequenceOfUltrasoundRegions` `(0018,6011)` as a spacing fallback when `PixelSpacing` is absent so ultrasound measurements calibrate in millimetres automatically.
- **Structured Report & GSPS groundwork** (`src/imaging/sr.rs`): recursive DICOM `ContentSequence` `(0040,A730)` text tree extraction (`SrNode`) and basic GSPS (`PR`) window/rotation/flip/text-annotation reader (`GspsState`) with unit tests.
- **PACS & DICOMweb groundwork** (`src/net/mod.rs`): typed, serializable `PacsNode`, `DicomWebServer` (with QIDO-RS query URL encoding), `QueryFilter`, and `StudyQueryResult` models with unit tests, ready for `dicom-ul` / `ureq` transport integration.

## [0.7.1] - 2026-11-20

### Removed
- **Code-signing steps.** The owner will not buy signing certificates, so the signing steps in the release workflow (Windows executable and installer, macOS app) and `docs/SIGNING.md` are gone. The **Windows installer stays**, unsigned like the `.zip`; the README explains how to open an unsigned build the first time.

## [0.7.0] - 2026-11-20

Milestone: PET-CT fusion (0.6.0, Antigravity) reviewed and completed, curved MPR, 3D scissors, key-images case report, and an installable, signing-ready app. Built in steps 0.6.2–0.6.6, described below.

### Added: app icon and Windows installer
- **App icon** (`assets/icon/sekhmetx.svg`, an original drawing: an axial chest CT slice inside a golden ring, the scanner's gantry and Sekhmet's sun disc). Shown on the window and taskbar, embedded in `sekhmetx.exe`, as the macOS app icon (`AppIcon.icns`, built in the release workflow) and in the Linux archive with a `sekhmetx.desktop` menu entry.
- **Windows installer** (`packaging/windows/sekhmetx.iss`, Inno Setup): `sekhmetx-vX.Y.Z-windows-x86_64-setup.exe` (and ARM64) beside the `.zip` downloads. Installs for the current user without administrator rights into `%LOCALAPPDATA%\Programs\SekhmetX`, with a Start-menu entry, an optional desktop icon, the licence page and an uninstaller, in English or Spanish; the library and settings are kept on uninstall and the in-app updater keeps working. CI builds the installer on every push to catch problems before a release.
- **Signing-ready releases:** the release workflow signs the Windows executable and installer, and signs and notarizes the macOS app, as soon as certificates are stored as repository secrets; until then builds are published unsigned, as before. `docs/SIGNING.md` explains the options and costs and how to add them.

## [0.6.6] - 2026-11-20

### Added: 3D sculpting (scissors) in volume rendering
- **Scissors** menu in the 3D view's toolbar (Spanish "Tijeras"), or `X` in a 3D view: draw a loop on the 3D image and everything inside it is cut away through the whole depth (**Cut inside the loop**), or everything outside it (**Keep only inside the loop**), to clear the table, ribs or other structures off what you want to see. **Undo last cut** (up to ten steps) and **Restore everything**; the menu shows how much is cut away, and each cut reports it in the status bar. `Esc` leaves scissors mode. Rotating, zooming, window and colour all keep working on the sculpted volume.
- How it works (`src/sculpt.rs`): a keep/cut map over the volume (up to 128 cells per side) is edited on the CPU by projecting each cell through the same camera the renderer uses (now one shared function, `sculpt::vr_camera`) and checking it against the drawn loop; the ray-caster in `src/gpu_view.rs` skips cut cells, in VR and MIP. IsiX does this in its VTK 3D window, which cannot be translated; this is a rebuild for the wgpu renderer.
- Test: a square drawn in the middle of the front view cuts the middle of the volume through its whole depth and leaves the edges; cutting outside the same square removes exactly the rest. Checked in the app on the phantom.

## [0.6.5] - 2026-11-20

### Added: curved MPR (straightened view along a path)
- **Curved MPR path tool** (`U`, ROI menu and right-click palette, own icon): click points along a structure (spine, vessel, ureter) on any 3D MPR view, then `Enter` or double-click. A **Curved MPR** window opens with the structure straightened: columns follow the path, rows step across it, the path runs along the middle row. **Turn** (−90°…90°, with 0° and 90° buttons) turns the cut about the path, 0° across the plane drawn on and 90° within it; **Width** sets how far it reaches to each side (10–120 mm). Drag on the image to measure a length in mm; hovering shows the value and the position along and across the path. The path stays drawn on the view it was made on until the window is closed.
- Ported from IsiX (`src/imaging/cpr.rs`): the path checks of `CPRStraightenedGeneration.swift` (at least three points, no degenerate segment, at least 1 mm; a path that crosses itself is reported, not refused) and the sampling of `CPRStraightenedOperation.swift` (one column per step along the curve, rows along a normal kept at right angles to it). The path is a centripetal Catmull-Rom spline through every clicked point (IsiX smooths a Bézier path), sampled at the volume's finest spacing.
- Tests: IsiX's path decisions, the smooth path passing through the points at even steps, and a path down the phantom's spine straightening into a band that is spine along the middle row and soft tissue 30 mm to the side. Checked in the app: the 30 mm spine measures 29.3 mm across the straightened band.
- Lengths and ROIs already worked on 3D MPR planes (oblique included); the straightened image adds its own length measurement.

## [0.6.4] - 2026-11-20

### Added: key-images case report
- **Snapshot when marking a key image:** pressing `K` (or ★) now also keeps an anonymized snapshot of the image exactly as shown, with measurements, colour table, fusion and zoom, and records its series, image number and the measurement texts on screen (in the app's data folder, `key_images`). Unmarking removes it.
- **"Export key images report…"** (right-click menu, Spanish "Exportar informe de imágenes clave…") writes, into a folder you choose: the key images (`key_01.png`…; rendered from the file when an image was marked before this version), **`report.html`** (opens in any browser: each image with its series, image number and measurement values, a findings box to type into, and a Print / save as PDF button, in English or Spanish), and `case_summary.txt`, now with the measurement values. No patient name, ID or dates.
- The image's slice slider is left out of anonymized captures (copied images and key-image snapshots).

## [0.6.3] - 2026-11-20

### Fixed: empty images at the ends of a turned plane
- A turned (oblique) plane scrolled over the volume's whole projected extent, which for a tilted plane includes the volume's empty corners, so the first and last images showed nothing (seen at the top of a turned chest-abdomen-pelvis study). Each volume now keeps a coarse map of where anatomy is (`foreground_points`), and a turned plane scrolls over the range in which it cuts anatomy (`Volume3D::content_span_mm`), padded by one grid cell and never wider than before, so no anatomy is cut off. On a phantom tilted 20° the range shrank from 298 to 268 mm, and the body edge appears within the first images.
- Test: every voxel holding anatomy lies within the new range, which is narrower than the old one.

## [0.6.2] - 2026-11-20

### Reviewed and fixed: PET-CT fusion (0.6.0)
Checked on a synthetic PET-CT phantom: a PET series (4 mm pixels, 3.27 mm slices, Bq/mL with decay correction to scan start, 70 kg, 370 MBq F-18 one hour before) whose body is SUV 1 and whose hot sphere (SUV 10) sits on the CT phantom's lesion.
- **SUV is right:** the cursor reads 1.00 SUV in the body and 10.00 SUV in the sphere, so the IsiX-ported SUV maths and tag reading work.
- **Fixed: fusion on coronal, sagittal, turned and 3D MPR planes.** The overlay took the stored PET slice nearest to the view and laid it flat over it, which is only right for an axial view. Once the PET series is loaded as a volume (it is now loaded whole when fusion is on), it is resampled along exactly the plane shown, so the overlay lines up on any plane; the sphere sits on the lesion in axial and coronal views, with the right SUV under the cursor. The resampled image is cached until the plane changes. Until the volume is ready, the nearest stored slice stands in as before.
- **Fixed: half a CT pixel offset** in placing the overlay (about 0.35 mm on CT).

## [0.6.1] - 2026-11-19

### Changed: a tidier series list
- **One card for the same reconstruction in several planes.** When a series exists as complete volumes in more than one plane (for example "CEREBRO PB Cabeza 1.000" axial, coronal and sagittal), it shows as a single card with the gold **MPR** badge (the axial volume); the tags in its corner read MPR-A, MPR-C, MPR-S (and AX/COR/SAG for any other reconstructions), and clicking them shows the other volumes, labelled MPR-C / MPR-S, first. Their own reconstructions move into the same card (`twin_volumes`, `twin_parent` in `src/dicom_index.rs`).
- **Extras:** the "Single images / Scouts" group is now "Extras: scouts, dose and single images" (Spanish "Extras: localizadores, dosis e imágenes sueltas"), still folded by default. It now also takes scouts of two or three images (CT/MR/PET series too short to form a stack plane) and dose reports that cannot be placed, numbered 900 or above, of up to 20 pages (a Canon dose report had six).
- **Tag letters centred:** badges and tags centre their letters on the letters' actual ink rather than on the text box, whose trailing spacing pushed them to the right.

## [0.6.0] - 2026-11-19

### Added: PET-CT Fusion and SUV (ported from IsiX)
- **PET SUV calculation** (`src/imaging/suv.rs`, `src/frame.rs`, `src/roi/mod.rs`): reads DICOM radiopharmaceutical tags (injected dose, half-life, decay correction, patient weight, injection/acquisition times, and Philips private SUV scale factor `7053,1000`) following IsiX's `SUVParameters`, `SUVComputation`, `PETSUVConversion`, `PETWindowing`, and `SUVWindowScaling` with translated unit tests. PET images display calibrated **SUV** under the cursor (2D and GPU MPR) and in ROI statistics, histograms, and line profiles.
- **2D PET-CT / multi-modality fusion overlay** (`src/imaging/fusion.rs`, `src/app/viewport.rs`, `src/ui/toolbar.rs`, `src/ui/icons.rs`): ported IsiX's `FusionBlend`, `FusionComparison`, `PETBlendingTable`, and `ViewTransform.alphaTable` (6 alpha curves: Linear, High-Low-High, Low-High-Low, Logarithmic, Inverse Log, Flat). Added a **Fusion** toolbar button and popover (EN/ES) with one-click auto-selection of PET/NM series in the same study, opacity slider (0–100%), alpha curve picker, and fusion CLUT selector (`PET`, `HotIron`, `GEcolor`, `Spectrum`, etc.). Overlays align automatically in 3D patient (LPS) space across different matrix sizes and fields of view, and show the fused PET SUV value under the cursor.

## [0.5.37] - 2026-11-19

### Housekeeping and polish
- Spanish for the last English-only texts: the library Clear button's hint, the grouped-reconstructions hint, the ROI histogram messages and the link-group badge and its hints.
- The compact thumbnail strip takes its plane colours from the theme, so AX/COR/SAG/MPR match the series list (SAG was orange there, red elsewhere).
- Project page: the comparison and series-recognition features rewritten as short points; the comparison screenshot replaced with a two-date comparison in which the later, rolled study is turned upright automatically (synthetic phantom).
- `HANDOFF.md` brought up to v0.5.37 (comparison, series recognition, open threads, the state of the owner's unpushed local build); `BUGS.md` lists the comparison and series-filing fixes.

## [0.5.36] - 2026-11-19

### Improved: series recognition from the real spine study's DICOM tags
The owner's files showed that the coronal reconstructions are Secondary Capture pictures with no position, orientation, frame of reference or slice thickness, and nothing naming their plane; they keep the acquisition time and the kernel.
- **Canon / Toshiba kernel codes** are understood: FC01-FC29 soft tissue, FC30-FC49 and FC80+ bone, FC50-FC59 lung. The recorded kernel now counts before the series name when matching reconstructions to volumes.
- **Acquisition-time cross-check** for pictures that cannot be placed: they keep the time of the acquisition they were made from, so a volume acquired at another moment is rejected and the volume of the same moment (same kernel family first) is used. Lumbar reconstructions (+112.5 s) go under the lumbar volume, thoracic ones under the thoracic volume.
- **Canon dose pages** (a few pictures with no position and no pixel size, or numbered 900 and above, described only as "4") go into Single images / Scouts.
- The plane of these pictures still comes from their block (AX and SAG present, so this one is COR); no tag names it. Reading the plane from the picture itself is left for later, as the block rule covers every case in this study.

## [0.5.35] - 2026-11-19

### Fixed
- **Lumbar coronals were filed under the thoracic volume** because thoracic and lumbar reconstructions share names ("BLANDO Columna 2.000", "OSEO Hueso 2.000") and the coronals carry no image position. A series that cannot be placed in 3D now joins only its own block: the run of neighbouring same-named series in series-number order (`name_runs`), e.g. #28 AX, #29 SAG, #30 COR. It goes under the volume that block belongs to; a block with no volume keeps it under its first series that can be placed.
- **The plane of those series:** when a block already has two planes (AX and SAG), its series without a recorded plane is the missing one (COR) and is tagged so (`infer_missing_planes`, also for studies already in the library). It still cannot be used for 3D.

### Changed
- Lettered volume badges read `MPR-A`, `MPR-C`, `MPR-S`.

## [0.5.34] - 2026-11-19

### Improved: recognising reconstructions in spine studies (from a real whole-spine CT)
- **Series without image positions** (straightened or curved reconstructions saved as pictures, which showed as "2D") take their plane from the image orientation when it is recorded (`refine_plane`, also applied to studies already in the library), and are filed with the volume whose series carry exactly the same name, e.g. "BLANDO Columna 2.000" coronal with the "BLANDO Columna 2.000" axial and sagittal reconstructions.
- **Spanish kernel names:** BLANDO / blandas count as soft tissue, OSEO / óseo / HUESO as bone, so soft-tissue and bone reconstructions go under the matching volume.
- **Dose reports, protocol pages, topograms and localizers** (up to five images with such a name, or SR/OT/PR/KO/DOC series) go into "Single images / Scouts" (`is_auxiliary`).
- **MPR A / MPR C / MPR S:** when the same reconstruction exists as complete volumes in several planes (same MPR group or same name), each badge names its plane, in the MPR gold leaning towards the plane's colour: coral for sagittal, olive-green for coronal, and a warm sky blue for axial (gold and blue mix to grey, so axial leans further towards blue). One complete volume alone keeps plain MPR (`study_cut_badge`).
- Test built on the spine study's layout (bone and soft-tissue volumes, same-named reconstructions, position-less coronals, dose report, cervical volumes in two planes).

## [0.5.33] - 2026-11-19

### Changed
- The reconstruction tags (AX, COR, SAG) sit in the bottom-right corner of the volume's card.

## [0.5.32] - 2026-11-19

### Changed
- The reconstruction tags (AX, COR, SAG) sit directly under the "CT · #4 · 300 images" line, and the volume's card keeps its normal size (its title stays on one line).
- Unfolded reconstructions are drawn in a lighter tint of the highlight colour, with a thin line in the indent tying them to their volume, so they read as derived from it.

## [0.5.31] - 2026-11-19

### Changed
- **Reconstructions are hidden inside the volume's card.** The plane tags (AX, COR, SAG) of the reconstructions made from a complete volume now sit on a third line inside its card, and the separate "▸ Includes" line is gone. The reconstructions stay hidden at every start; clicking the tags shows or hides them as smaller cards (the tags light up while shown). Hover text in English and Spanish.

## [0.5.30] - 2026-11-19

Numbered 0.5.30 because a local 0.5.29 build exists on the owner's PC that is not on GitHub yet.

### Fixed: comparison starts at different sizes and positions
- **"= mm" zoom matched against the old image size** when a comparison turned the other study: the turned image is resliced at another size (for example 512 → 667 px), but the zoom was matched before it was drawn. The match now waits until every linked viewport has been drawn with its current image (`pending_zoom_match`, `finish_pending_zoom_match` in `src/app/mpr_sync.rs`). On the test phantom the turned image now opens at 125 % instead of 100 %, the same physical size as the reference.
- **Matching anatomy is now at the same place on screen.** A turned image was centred on the middle of its own scan (for a chest-abdomen-pelvis scan compared with an abdomen scan, a different part of the body). It is now centred on the point that matches the reference image's centre, and kept there while scrolling (`recenter_in_plane`); the pan is copied too. The scroll range is unchanged.

### Added: MPR badge and folded reconstructions in the series list
- A series that is a **complete volume** (at least 30 slices, at most 2 mm apart and at most four pixels apart) shows an amber **MPR** badge instead of AX/COR/SAG: it can be viewed in any plane without losing detail. Thicker or single-plane series keep AX, COR or SAG (`Series::is_full_volume`, `cut_badge` in `src/dicom_index.rs`). Also in the compact thumbnail strip, drag preview and search.
- **Reconstructions are folded under the volume they were made from** (`find_reconstructions`): separate axial, coronal or sagittal series that lie inside a complete volume's space, with the same modality and frame of reference, and not acquired at another moment (another contrast phase stays apart). When several volumes qualify, the same kernel family wins (soft tissue; lung and bone count as one sharp family, as Siemens B70 serves both), then the closest acquisition time and the most similar name. The volume's card shows "▸ Includes AX COR SAG (n)" (Spanish "Incluye"); clicking it unfolds smaller cards for the reconstructions.
- Tests: badges for 1, 2 and 5 mm series and short stacks; filing of soft-tissue, lung and later-phase reconstructions, and nothing from another frame of reference or moment.

## [0.5.28] - 2026-11-18

### Added
- **Hidden shortcut to the data folder:** a faint "⋯" in the bottom-right corner of **Settings** opens the folder holding the settings, library database and log (`sekhmetx.log`), for troubleshooting. Hovering it says what it does (English and Spanish). The visible **Open folder** button under "Local data" stays. Both use one helper, `open_in_file_manager` in `src/app/modals.rs`.

## [0.5.27] - 2026-11-18

### Improved: alignment level by level along the body
Between two dates the patient breathes differently, lies a little twisted or with the pelvis tilted, so one alignment for the whole scan is a compromise: on a real chest-abdomen-pelvis against abdomen comparison the chest came out lower than the other date and the pelvis turned differently from the abdomen.
- **Alignment per level** (`fit_local` in `src/volume.rs`): after the whole-scan fit, the alignment is fitted again over overlapping 8 cm stretches every 3 cm, starting from the whole-scan fit. Each stretch may move up to 30 mm in height (breathing), 15 mm sideways and 6° in any direction, and only when that clearly improves the match there (each millimetre and degree has to pay for itself, so changed organs cannot pull it off). Neighbouring stretches are smoothed, and positions between them are blended, so the linked image follows smoothly while scrolling: slice position and turn change gradually from level to level.
- Finer and longer final search for the whole-scan fit (more steps, 0.3 mm / 0.2° last step).
- `sekhmetx.log` records "Local alignment: N of M stretches refined".
- Tested on a synthetic patient lying twisted (roll changing 10° along the scan) and bent (14 mm): average error 4 mm per level against 7 mm with one fit; the earlier tests (roll, part overlap, table, changed organs) still pass.

## [0.5.26] - 2026-11-18

### Added: automatic rotation between two dates
- **The other image now turns by itself.** When the automatic alignment between two studies is trusted and **Cut** is linked, the linked viewports are turned and moved to match the viewport the comparison started from (the one active when the link was switched on, or the Compare leader), not whichever image was touched last (`link_master` in `src/app/mod.rs`). Before, a good alignment only moved the slice position, and the roll had to be corrected by hand.
- **Alignment by the body outline** (`register_by_body` in `src/volume.rs`), tried alongside the earlier whole-volume method; the better one is used. It finds the body on each axial slice (above −350 HU, the CT table removed), matches the two studies along their length by the body's cross-section, air and bone, measures the patient's roll from the outline's shape, and then refines on body voxels only. This handles scans that cover different lengths (chest-abdomen-pelvis against abdomen) and a rolled patient. When the fine match stays weak but the outline evidence agrees across slices, the outline result is used and the status says "Aligned by the body outline (approximate)" / "Alineado por el contorno del cuerpo (aprox.)". Tested on synthetic two-date phantoms with a 17° roll, part overlap, a table, and changed interior.
- **Log lines for troubleshooting:** `sekhmetx.log` records each alignment ("Body registration: profile …, height shift …, outline roll …" and "Registration finished: score …, trusted …").

## [0.5.25] - 2026-11-18

### Fixed
- **"= mm" zoom link showed the two images at different scales** when one plane was turned (3D MPR): it compared the source slices' field of view, but a turned plane is shown as a resliced image of another size. It now matches the scale actually on screen (millimetres per screen pixel, measured each time a viewport is drawn). On the test phantom a 20 cm ruler measured 331 px and 370 px before, and 331 px in both after.

### Added
- **Ctrl (Cmd on Mac) = this viewport only, for everything:** while held, scrolling (Scroll tool, arrow keys, slider), zoom, pan, window/level, 2D rotation and 3D rotation change only the viewport you are working in. After moving or turning one viewport alone with **Cut** linked, its new offset is locked automatically when Ctrl/Cmd is released, so the viewports stay linked with that difference. Zooming one alone switches the zoom link from "= mm" to "Rel" so the sizes stay as set. (`Ctrl`+wheel, which zooms, now zooms only that viewport.)
- **The viewport you start from leads when you switch a link on:** turning on **Cut** makes the other linked viewports take the active viewport's plane, position, rotation and flips (through the registration or manual lock when there is one); **WL/WW** gives them its window; **Zoom "= mm"** its scale. To make the first look like the second, start from the second. Shared with **Compare** (`align_plane_to`, `align_linked_to` in `src/app/mpr_sync.rs`).

## [0.5.24] - 2026-11-18

### Fixed: rotation could not be matched between two studies of a patient lying rotated
When the automatic 3D alignment between two dates fails (score below 0.45; for example a thorax-abdomen-pelvis scan compared with an abdomen-only scan, with the patient rolled), rotation sync could not be made to work:
- Rotating either image with **3D Rotate** or **Rotate** copied its exact orientation onto the linked image, so a correction for the patient's roll could never exist between the two; **Lock 3D Sync** then recorded "no difference".
- A background alignment that finished after **Lock 3D Sync** silently replaced the lock with its own low score, so the lock was ignored.

Now:
- **Rotation follows as a change, not a copy** (`MprPlane3D::rotation_since` / `rotate_axes` in `src/volume.rs`, `sync_mpr_orientation_from` and `sync_rotation_from` in `src/app/mpr_sync.rs`): rotating one image turns the linked one by the same amount, keeping any difference set between them. Tested.
- **Ctrl+drag (Cmd on Mac) with Rotate or 3D Rotate turns only that image**, to correct one date's roll; then **Lock 3D Sync** keeps that alignment.
- **A manual lock is never replaced** by a background alignment (`locked_pairs` in `src/app/mod.rs`).
- **Clear status text**, in English and Spanish: "Alignment locked by hand", or, when automatic alignment fails, what to do ("Ctrl+rotate this image to match, then Lock 3D Sync"). The text is wrapped below the patient details instead of running into them. The bottom-left "Sync Locked (NCC …)" readout, which appeared even when nothing was locked, now says "locked" only for a real lock.
- Rotation readout shows 0° instead of 360°.

## [0.5.23] - 2026-11-18
- 2D rotation (toolbar and right-click menu) synced to linked viewports; registration rotation applied only when reliable; registration search widened for partial overlap and patient roll (Z sweep and ±24° roll).

## [0.5.18] - 2026-11-17

### Fixed
- **Windows with no Vulkan driver could not start the app (since 0.5.13).** Vulkan is now used only when the PC has a Vulkan graphics driver; otherwise Direct3D 12 is used as before. This matters for many Intel graphics, virtual machines and Windows-on-ARM laptops. The check is also exercised in the tests.
- **Dragging an ROI's corner onto its opposite corner left an empty "0 px" ROI** that was saved. The ROI now snaps back to its shape before the drag.
- **Double-clicking a viewport in a multi-viewport layout reset its zoom and pan** as well as enlarging it. It now only enlarges it; double-click still resets the view in a single viewport.
- **Wrong shortcut letters:** the Angle and Cobb angle tools were labelled `A` and `C`, but those keys switch to axial and coronal.
- Lint error (`clippy::drop_non_drop`) that newer Rust reports in `src/app/overlays.rs`, which would fail CI once GitHub's Rust is updated.

### Improved (interface)
- Right-click tool palette: drawn icons for Arrow and Cobb angle (they reused the Length and Angle icons), Pencil added, full tool descriptions as tooltips.
- Menu items no longer carry decorative emoji (★ kept for key images); the 3D item reads "3D MPR" as in the toolbar.
- The cursor readout no longer stays in the status bar while the library is shown.
- Spanish for the remaining English text: "files missing" note, update badge and tooltips, transfer-function stop removal.

### Documentation
- `CHANGELOG.md` entries for 0.5.6–0.5.17 (they were not recorded at the time), README features and shortcuts brought up to date.

## [0.5.17] - 2026-11-16
- Patient age next to the date of birth; double-click a viewport to enlarge it; 2D/3D orbit rotation kept in sync; scrubber direction fix; Ax/Cor/Sag buttons recentre the image.

## [0.5.15] - 2026-11-16
- Fixed an `unused_mut` lint on macOS/Linux.

## [0.5.14] - 2026-11-16
- **Clear Library** asks for confirmation and only empties the library list, never folders on disk. Removing a study can move its image files to the Recycle Bin / Trash (new `trash` dependency) instead of deleting them.

## [0.5.13] - 2026-11-16
- Vulkan preferred on Windows (see the 0.5.18 fix); long/short MPR sync fallback; sharper ROI handles; phases of one orthogonal MPR acquisition grouped.

## [0.5.12] - 2026-11-16
- ROI editing: move by the centre dot, reshape by handles, delete the selected ROI. Cobb angle and Arrow (with text) tools. ROI histogram and line profile. OLED (pure black) theme. Removing a study's files from disk. Separation of acquisition phases.

## [0.5.11] - 2026-11-16
- Key-image filter and anonymized case-summary export; live ratio between two lengths; calibration of pixel spacing from a known length.

## [0.5.10] - 2026-11-16
- Key images (stored in the library database; `K` key and ★ button; export). ROI mean/SD on GPU-reconstructed MPR planes. ROI callouts can be dragged.

## [0.5.6] - 2026-11-16
- Right-click quick-tool palette, maximise a viewport, "images only" toggle that hides the annotations. CI security audit switched to `cargo audit`.

## [0.5.5] - 2026-11-16

### Ported from IsiX 6.0.4 (review in `porting/reports/2026-11-16-isix-6.0.4-review.md`)
- **VOI LUT fixes (`src/imaging/voi_lut.rs`, `src/frame.rs`):** a VOI LUT is now applied to every image, including one whose pixels all fall outside the table (previously treated as a corrupt table and ignored); images with a rescale slope/intercept now use their VOI LUT, entered by the rescaled value; a one-entry table is valid. Affects mainly CR, DX and mammography. IsiX's tests translated.
- **Cine is not a volume (`src/volume.rs`):** a series whose images all sit at the same position is no longer accepted as a 3D volume.
- **Exponential slab-thickness slider (`src/imaging/slab_scale.rs`, `src/ui/toolbar.rs`):** fine steps for thin slabs and whole millimetres in between, with the thickness shown and editable beside it. The thick end stays at 50 mm (see `DEVIATIONS.md`).

### Improved (interface)
- **Library table sorts** by any column: click a header, click again to reverse. Default is patient name; the choice is remembered.
- **Settings and first-run dialogs:** section headings, button-style choices, paths in boxes; update status messages translated to Spanish.
- **Viewport slice slider** sits above the bottom ruler instead of on top of it, and follows the accent colour.
- **Metadata inspector** translated to Spanish (it was English-only).
- **Slab mode names** translated (`Fino`, `Media`).

### Project
- **IsiX tracking restored:** `porting/upstream.toml` is back (IsiX only) and marked reviewed up to IsiX 6.0.4; 11 new IsiX files assigned in the porting map.
- **`AGENTS.md` / `GEMINI.md` rewritten** for the current layout, commands and the owner's standing rules.

## [0.5.4] - 2026-11-16

### Removed
- **Web-portal PACS login** (`src/pacs_web/`, the `--pacs-login` helper, the Institution Login / Search PACS / Refresh buttons and their dialogs). It did not work reliably and showed one institution's name to every user. It will come back later in a different form. Nothing ported from IsiX is affected.
- The embedded web-browser components it needed (`wry`, `tao`, and on Linux WebKitGTK), so the app is smaller and Linux no longer needs WebKitGTK installed.

### Added
- **One-time clean-up of the old portal's data (`src/legacy_cleanup.rs`):** on first start, the saved portal password and session cookies are deleted from the system keychain, the portal settings are deleted from the local database, and the web-browser profile folder is removed.
- **Ax / Cor / Sag buttons light up** for the plane on screen in 3D MPR. A series' own acquisition plane counts even with gantry tilt; any other plane must be within about 3° of a patient axis, otherwise it is treated as oblique and none lights up.
- **Accent colour in Settings:** eight presets or a custom colour, plus an option to tint the toolbar and status bar with it. Remembered between sessions.

### Documentation and licensing
- **README rewritten** as the project's landing page: what SekhmetX is, that it is a port of IsiX DICOM Viewer by Thales Matheus M Santos (ThalesMMS) and how the port works, features, downloads, shortcuts (the `A` key is Axial, not Angle), building, and licence and credits. Screenshots from a synthetic phantom in `docs/screenshots/`, plus a social preview image.
- **Licence files added:** `LICENSE` (GNU LGPL v3), `COPYING` (GNU GPL v3) and `NOTICE` (credits to IsiX, Horos, OsiriX and third-party libraries). They are now included in every release download.

## [0.5.3] - 2026-11-16

### Improved (Interface look — pass 2, second toolbar row / status bar / library details)
- **3D VR and link controls (`src/ui/toolbar.rs`):** emoji labels replaced by plain text buttons that show their on/off state; a `Link` caption precedes the Zoom / WL/WW / Cut toggles. All of these controls, their tooltips and menus are now translated to Spanish (they were English-only).
- **View menu:** on/off items are checkboxes.
- **Status bar (`src/app/mod.rs`):** drawn gear icon for Settings; PACS, refresh and crash-log buttons without emoji.
- **Library details pane (`src/app/browser.rs`):** the selected study's patient, ID, birth date, description, date, accession and contents are shown beside the series thumbnails, with `Open study` and `Open tiled` buttons.
- **Light theme:** buttons and drop-downs have a visible body.

## [0.5.2] - 2026-11-16

### Improved (Interface look — pass 1, toolbar / sidebar / library)
- **One theme for the whole app (`src/ui/theme.rs`):** a single palette, spacing and text sizes replace the per-screen colours; the toolbar and status bar get their own darker chrome.
- **Drawn toolbar icons (`src/ui/icons.rs`, `src/ui/toolbar.rs`):** mouse tools and ROI tools are icon buttons in segmented groups; Library, Sidebar, Open, layout, Ax·Cor·Sag, 3D MPR, 3D VR, View, Meta-Data and Cine are icon tiles with their own caption. The icons are our own vector drawings and stay sharp at any display scale. Emoji and text-only buttons are gone from the toolbar and slice strip.
- **Series cards in the sidebar (`src/app/browser.rs`):** each series shows its thumbnail, description, a coloured plane badge (`AX` / `COR` / `SAG`) and modality, number and image count, instead of a monospaced text row.
- **Library table (`src/app/browser.rs`):** left-aligned columns, numeric columns right-aligned, striped rows, accent-coloured selection, row actions (open, tile, remove) as icons that no longer run off the right edge; albums list with counts; headers translated to Spanish.

### Fixed
- `cargo clippy -- -D warnings` failed on Rust 1.97 (`needless_range_loop` in `src/imaging/convolution.rs`).

## [0.5.1] - 2026-11-16

### Added
- **Dark mode by default with a Theme setting** (`Dark` / `Light` / `System`) in Settings and the first-run notice.
- **Compare popover:** choose which viewports to compare; WL/WW linking is opt-in; disabled with fewer than two loaded viewports.
- **Windows file metadata** embedded in the executable (`build.rs`) and symbols no longer fully stripped, to reduce antivirus false positives.

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
