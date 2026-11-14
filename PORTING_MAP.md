# Porting map: IsiX / Horos → RDicom

Generated from `porting/map.toml` by `python3 tools/upstream.py render-map isix`. Edit the TOML, not this file.

Sizes counted on IsiX [`3e248312`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/3e24831224b8) (2026-11-13): 5678 files, all assigned to an area.

**Decisions:** *Port* = translate the logic to Rust, with its tests. *Rebuild* = recreate the screen or behaviour to match Horos. *Replace* = use an existing Rust library. *Reuse* = copy the data files. *Skip* = not planned.

## Size by priority

| Priority | Areas | Source files | Lines of IsiX code |
|---|---|---|---|
| P1 · daily core | 8 | 1,139 | 234,505 |
| P2 · next | 8 | 505 | 109,817 |
| P3 · later | 5 | 215 | 69,307 |
| P4 · maybe | 1 | 21 | 5,417 |
| Not ported | 8 | 2,281 | 468,188 |

Lines count all IsiX code in the area, including headers and tests; Rust versions are usually shorter, and *Replace* areas mostly use existing libraries.

## P1 · daily core

| Area | Decision | RDicom | Source files / lines | In RDicom | Rust crates | Notes |
|---|---|---|---|---|---|---|
| **Colour tables, 3D presets, DICOM dictionaries**<br>`data-assets` | Reuse data | not started | 68 data files | assets/ | plist | CLUTs and 3D presets are plain data files: load them directly. Check licence notes for the dicom3tools tables. |
| **DICOM files: parsing, datasets, character sets, transfer syntaxes, DICOMDIR, codecs**<br>`dicom-core` | Replace with crate | started | 98 / 18,561 | src/dicom_index.rs, src/frame.rs | dicom-object, dicom-pixeldata, dicom-dictionary-std | dicom-rs covers most of this. Port IsiX's import rules and edge-case fixes (character sets, broken files) on top. |
| **Geometry: vectors, planes, patient coordinates, Bezier paths**<br>`geometry` | Port | not started | 32 / 5,657 | src/geometry.rs (planned) | glam or nalgebra |  |
| **Pixel pipeline: frame loading, rescale, window/level, VOI LUT, overlays, image maths**<br>`imaging` | Port | started | 103 / 19,396 | src/imaging/ (window, VOI LUT, defaults, photometry), src/frame.rs, src/loader.rs | — | Ported with IsiX's tests: requested/full window, linear and colour windowing, VOI LUT tables, default-window order, MONOCHROME1. Still to port: convolution filters, image arithmetic, subtraction, shutters, thumbnails. |
| **2D viewer: viewport, tools, scrolling, cine, annotations, layouts, sync, toolbar**<br>`viewer-2d` | Rebuild | started | 353 / 72,937 | src/app.rs, src/gpu_view.rs (GPU window/level), src/imaging/drag.rs, src/imaging/presets.rs | egui, wgpu | Where Horos's feel lives. Rebuild screen by screen, side by side with Horos. |
| **ROIs and measurements: length, angle, ellipse, polygon, brush, statistics, ROI manager**<br>`roi-measurements` | Port | started | 173 / 36,342 | src/app.rs (length, angle); src/roi/ (planned) | — | Statistics and geometry port directly from IsiXROI with its 25 test files; drawing tools are rebuilt. |
| **Local database and import: indexing, folders, archives, incoming, storage**<br>`database-import` | Port | started | 266 / 55,374 | src/dicom_index.rs; src/db/ (planned) | rusqlite | IsiX already moved to SQLite (IsiXStorage), which maps directly onto rusqlite. |
| **Study browser: study list, series thumbnails, previews, albums, search, comparison**<br>`browser` | Rebuild | started | 114 / 26,238 | src/app.rs (browser panel) | — | The second half of Horos's feel. Match its columns, thumbnails and preview behaviour. |

## P2 · next

| Area | Decision | RDicom | Source files / lines | In RDicom | Rust crates | Notes |
|---|---|---|---|---|---|---|
| **PACS networking: echo, query/retrieve, send, listener, TLS, worklist**<br>`network-pacs` | Port | not started | 181 / 37,529 | src/net/ (planned) | dicom-ul (dicom-rs), rustls |  |
| **DICOMweb and WADO (QIDO/WADO/STOW, OAuth)**<br>`dicomweb` | Port | not started | 18 / 6,588 | — | reqwest |  |
| **MPR: orthogonal and oblique reslicing, thick slab MIP/MinIP**<br>`mpr` | Rebuild | not started | 39 / 16,033 | src/mpr/ (planned) | wgpu, rayon |  |
| **PET-CT fusion and SUV**<br>`fusion-pet` | Port | not started | 20 / 6,105 | — | — |  |
| **Presentation states (GSPS), key images, structured reports**<br>`annotations-sr` | Port | not started | 9 / 1,612 | — | — |  |
| **Anonymization and export (DICOM, images, movies)**<br>`anonymize-export` | Port | not started | 47 / 10,593 | — | — |  |
| **Preferences, presets, shortcuts**<br>`preferences` | Rebuild | started | 114 / 18,693 | src/app.rs (PRESETS); src/settings.rs (planned) | — | Port the preference values and defaults (IsiXPreferences); rebuild only the settings screens that matter. |
| **App shell: startup, menus, activity/progress, logging, updates, keychain**<br>`app-shell` | Rebuild | started | 77 / 12,664 | src/main.rs | — |  |

## P3 · later

| Area | Decision | RDicom | Source files / lines | In RDicom | Rust crates | Notes |
|---|---|---|---|---|---|---|
| **3D: volume rendering, MIP/VR, surface rendering, 3D presets**<br>`volume-3d` | Rebuild | not started | 116 / 33,278 | — | wgpu | IsiX renders 3D with VTK and Metal. Rebuild as a wgpu ray caster; the opacity/colour preset maths ports directly. |
| **Curved MPR (CPR) and straightened views**<br>`cpr` | Rebuild | not started | 42 / 20,307 | — | — |  |
| **Segmentation, registration, subtraction, SEG objects**<br>`segmentation-registration` | Port | not started | 28 / 7,242 | — | — |  |
| **MR maps: T1/T2 fitting, synthetic DWI, b-value split**<br>`advanced-mr` | Port | not started | 11 / 2,795 | — | — |  |
| **Reports (Word/Pages/RTF templates linked to studies)**<br>`reports` | Rebuild | not started | 18 / 5,685 | — | — | Decide later: may be replaced by your own dictation workflow. |

## P4 · maybe

| Area | Decision | RDicom | Source files / lines | In RDicom | Rust crates | Notes |
|---|---|---|---|---|---|---|
| **Virtual endoscopy and fly-through**<br>`endoscopy-flythru` | Rebuild | not started | 21 / 5,417 | — | — |  |

## Not ported

| Area | Decision | RDicom | Source files / lines | In RDicom | Rust crates | Notes |
|---|---|---|---|---|---|---|
| **Build system, docs, scripts, test harnesses, release tooling**<br>`meta` | Not app code | done | 1660 / 351,902 | — | — |  |
| **Translations, icons, help pages**<br>`translations-icons` | Not app code | done | 951 data files | — | — | Icons are Horos artwork: draw our own. Spanish UI text can be added later. |
| **Bundled third-party code (OpenSSL, libarchive, VTK/ITK helpers, codecs)**<br>`third-party` | Replace with crate | done | 9 / 2,785 | — | rustls, zip/tar, dicom-pixeldata (OpenJPEG, CharLS) |  |
| **Swift/Objective-C bridges and adapters**<br>`glue` | Skip | done | 186 / 36,975 | — | — | Glue between IsiX's old and new code. Rust needs none of it, but read it to find which module does the real work. |
| **Cocoa widgets and helpers (Nitrogen, custom cells, NS categories)**<br>`ui-toolkit` | Replace with crate | done | 247 / 30,248 | — | egui | Generic AppKit building blocks. Replaced by egui widgets; only copy their look where Horos users would notice. |
| **OsiriX/Horos plugin system**<br>`plugins` | Skip | not started | 67 / 11,783 | — | — | Plugins are Objective-C bundles and cannot load in a Rust app. |
| **Web portal, HTTP/XML-RPC servers, shared databases, Bonjour sharing**<br>`web-sharing` | Skip | not started | 72 / 23,758 | — | — |  |
| **DICOM printing, CD/DVD burning, e-mail, Photos, iCloud**<br>`print-burn-mail` | Skip | not started | 40 / 10,737 | — | — |  |

## Upstreams tracked

| Upstream | Branch | Reviewed up to | Reviewed on |
|---|---|---|---|
| IsiX (https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS) | `horos` | [`a05084b6`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/a05084b61e78) | 2026-10-16 |
| Horos (https://github.com/horosproject/horos) | `horos` | [`2a130506`](https://github.com/horosproject/horos/commit/2a130506f2ae) | 2026-11-13 |
