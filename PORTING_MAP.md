# Porting map: IsiX / Horos → RDicom

Generated from `porting/map.toml` by `python3 tools/upstream.py render-map isix`. Edit the TOML, not this file.

**Decisions:** *Port* = translate the logic to Rust, with its tests. *Rebuild* = recreate the screen or behaviour to match Horos. *Replace* = use an existing Rust library. *Reuse* = copy the data files. *Skip* = not planned.

## P1 · daily core

| Area | Decision | RDicom | Source files / lines | In RDicom | Rust crates | Notes |
|---|---|---|---|---|---|---|
| **Colour tables, 3D presets, DICOM dictionaries**<br>`data-assets` | Reuse data | started | — | src/imaging/clut.rs, src/imaging/clut_tables.rs | plist | All 31 Horos 2D colour lookup tables (26 .plist CLUTs + 5 procedural tables) ported and wired into GPU/CPU pipelines. 3D presets still to port. |
| **DICOM files: parsing, datasets, character sets, transfer syntaxes, DICOMDIR, codecs**<br>`dicom-core` | Replace with crate | started | — | src/dicom_index.rs, src/frame.rs | dicom-object, dicom-pixeldata, dicom-dictionary-std | dicom-rs covers most of this. Port IsiX's import rules and edge-case fixes (character sets, broken files) on top. |
| **Geometry: vectors, planes, patient coordinates, Bezier paths**<br>`geometry` | Port | started | — | src/geometry/ (vector3, line3, plane3, transform3d, bezier, orientation, image_plane) | — | Ported IsiXGeometry (Vector3, Line3, Plane3, AffineTransform3D, PlanarGeometry, PackedVectors, BezierCore) and IsiXImaging geometry (ImagePlaneGeometry, ImagePlaneCoordinates, PlaneGeometry, LineGeometry, ViewTransform, ViewportState, OrientationLetters) with unit tests. |
| **Pixel pipeline: frame loading, rescale, window/level, VOI LUT, overlays, image maths**<br>`imaging` | Port | started | — | src/imaging/ (window, voi_lut, defaults, photometry, clut, clut_tables, convolution), src/frame.rs, src/loader.rs | — | Ported with IsiX's tests: requested/full window, linear and colour windowing, VOI LUT tables, default-window order, MONOCHROME1, ColorLookupTable, ColourTableState, ColourTableBook, 31 factory CLUTs, ImageConvolution, SeriesConvolution, ConvolutionKernelBook, 22 factory 2D convolution presets, and series mini-thumbnails. |
| **2D viewer: viewport, tools, scrolling, cine, annotations, layouts, sync, toolbar**<br>`viewer-2d` | Rebuild | started | — | src/app.rs, src/gpu_view.rs, src/navigation.rs, src/imaging/drag.rs, src/imaging/presets.rs | egui, wgpu | Multi-series visual square-grid tiling picker (1x1..4x4), SliceSynchronizer (3D position, slice location, relative) + cross-study 6-DOF 3D rigid sync, ReferenceLines (3D scout/cut lines), OrientationLetters (A/P/R/L/S/I), GPU/CPU CLUTs, 22 Horos convolution filters, rotation/flip, calibrated scale bar, scrubber bar, and Horos presets. |
| **ROIs and measurements: length, angle, ellipse, polygon, brush, statistics, ROI manager**<br>`roi-measurements` | Port | started | — | src/roi/ (geometry, statistics, raster, mod), src/app.rs | — | Ported IsiXROI (ROIPlanarGeometry, ROIBoundingBox, ROIDragConstraint, ROISpline, ROIPixelStatistics, ROIValueStatistics, CalciumScoring, ROILengthRatio, ROIRasterConversion, ROIScanline, ROIPolygonClip, ROIPolygonFill, ROIMasks) with 22 unit tests, SQLite ROI persistence (`rois` table), `--roi` CLI comparison mode, and interactive Length, Angle, Oval/Ellipse, Rectangle, Closed Polygon, Freehand Pencil (with spline smoothing), and 2D Point/Probe ROIs. |
| **Local database and import: indexing, folders, archives, incoming, storage**<br>`database-import` | Port | started | — | src/dicom_index.rs, src/db.rs | rusqlite, zip | SQLite WAL v2 database with schema migrations, persistent managed library (`SekhmetX/Studies/<uid>/`) with Ask/Always/Never ZIP copy policy, disk usage reporting, missing-file flagging (`⚠ [files missing]`), Clear/Remove study, temporary non-destructive .zip archive extraction, dominant anatomical plane classification (AX/COR/SAG/OBL), and acquisition-time auto-grouping of multi-plane reconstructions. |
| **Study browser: study list, series thumbnails, previews, albums, search, comparison**<br>`browser` | Rebuild | started | — | src/app.rs (browser panel), src/dicom_index.rs | — | Horos-style study browser with search filter bar, modality pills (All/CT/MR/US/DX/PT), live rendered 38x38 DICOM series mini-thumbnails, auto-detected MPR acquisition groups (AX/COR/SAG), and quick multi-series comparison tiling buttons. |

## P2 · next

| Area | Decision | RDicom | Source files / lines | In RDicom | Rust crates | Notes |
|---|---|---|---|---|---|---|
| **PACS networking: echo, query/retrieve, send, listener, TLS, worklist**<br>`network-pacs` | Port | not started | — | src/net/ (planned) | dicom-ul (dicom-rs), rustls |  |
| **DICOMweb and WADO (QIDO/WADO/STOW, OAuth)**<br>`dicomweb` | Port | not started | — | — | reqwest |  |
| **MPR: orthogonal and oblique reslicing, thick slab MIP/MinIP**<br>`mpr` | Rebuild | started | — | src/volume.rs, src/app.rs | wgpu, rayon | In-place single-viewport 3D MPR (Axial/Coronal/Sagittal/Oblique 3D trackball rotation), hybrid multi-series (AX/COR/SAG) tri-planar resolution fusion, and Thick-Slab MIP/MinIP/Mean ray-marched projections. |
| **PET-CT fusion and SUV**<br>`fusion-pet` | Port | not started | — | — | — |  |
| **Presentation states (GSPS), key images, structured reports**<br>`annotations-sr` | Port | not started | — | — | — |  |
| **Anonymization and export (DICOM, images, movies)**<br>`anonymize-export` | Port | started | — | src/export.rs, src/ui/toolbar.rs, src/app.rs | — | PNG slice/series export with active presentation (WL/CLUT/invert/convolution) and Horos-aligned DICOM de-identification (private tag stripping, identifier scrubbing, consistent UID remapping). |
| **Preferences, presets, shortcuts**<br>`preferences` | Rebuild | started | — | src/app.rs (PRESETS); src/settings.rs (planned) | — | Port the preference values and defaults (IsiXPreferences); rebuild only the settings screens that matter. |
| **App shell: startup, menus, activity/progress, logging, updates, keychain**<br>`app-shell` | Rebuild | started | — | src/main.rs | — |  |

## P3 · later

| Area | Decision | RDicom | Source files / lines | In RDicom | Rust crates | Notes |
|---|---|---|---|---|---|---|
| **3D: volume rendering, MIP/VR, surface rendering, 3D presets**<br>`volume-3d` | Rebuild | not started | — | — | wgpu | IsiX renders 3D with VTK and Metal. Rebuild as a wgpu ray caster; the opacity/colour preset maths ports directly. |
| **Curved MPR (CPR) and straightened views**<br>`cpr` | Rebuild | not started | — | — | — |  |
| **Segmentation, registration, subtraction, SEG objects**<br>`segmentation-registration` | Port | started | — | src/volume.rs (RigidTransform3D), src/app.rs | — | Automatic 6-DOF 3D rigid registration (foreground centroid + multi-scale Normalized Cross-Correlation) and manual 3D lock for cross-study comparison. |
| **MR maps: T1/T2 fitting, synthetic DWI, b-value split**<br>`advanced-mr` | Port | not started | — | — | — |  |
| **Reports (Word/Pages/RTF templates linked to studies)**<br>`reports` | Rebuild | not started | — | — | — | Decide later: may be replaced by your own dictation workflow. |

## P4 · maybe

| Area | Decision | RDicom | Source files / lines | In RDicom | Rust crates | Notes |
|---|---|---|---|---|---|---|
| **Virtual endoscopy and fly-through**<br>`endoscopy-flythru` | Rebuild | not started | — | — | — |  |

## Not ported

| Area | Decision | RDicom | Source files / lines | In RDicom | Rust crates | Notes |
|---|---|---|---|---|---|---|
| **Build system, docs, scripts, test harnesses, release tooling**<br>`meta` | Not app code | done | — | — | — |  |
| **Translations, icons, help pages**<br>`translations-icons` | Not app code | done | — | — | — | Icons are Horos artwork: draw our own. Spanish UI text can be added later. |
| **Bundled third-party code (OpenSSL, libarchive, VTK/ITK helpers, codecs)**<br>`third-party` | Replace with crate | done | — | — | rustls, zip/tar, dicom-pixeldata (OpenJPEG, CharLS) |  |
| **Swift/Objective-C bridges and adapters**<br>`glue` | Skip | done | — | — | — | Glue between IsiX's old and new code. Rust needs none of it, but read it to find which module does the real work. |
| **Cocoa widgets and helpers (Nitrogen, custom cells, NS categories)**<br>`ui-toolkit` | Replace with crate | done | — | — | egui | Generic AppKit building blocks. Replaced by egui widgets; only copy their look where Horos users would notice. |
| **OsiriX/Horos plugin system**<br>`plugins` | Skip | not started | — | — | — | Plugins are Objective-C bundles and cannot load in a Rust app. |
| **Web portal, HTTP/XML-RPC servers, shared databases, Bonjour sharing**<br>`web-sharing` | Skip | not started | — | — | — |  |
| **DICOM printing, CD/DVD burning, e-mail, Photos, iCloud**<br>`print-burn-mail` | Skip | not started | — | — | — |  |

## Upstreams tracked

| Upstream | Branch | Reviewed up to | Reviewed on |
|---|---|---|---|
| IsiX (https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS) | `horos` | [`a05084b6`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/a05084b61e78) | 2026-10-16 |
| Horos (https://github.com/horosproject/horos) | `horos` | [`2a130506`](https://github.com/horosproject/horos/commit/2a130506f2ae) | 2026-11-13 |
