# Roadmap: porting IsiX features to Rust

> **Superseded by [`PORTING_MAP.md`](PORTING_MAP.md)**, which covers every IsiX file and
> is generated from `porting/map.toml`. This file is kept as the original overview.

IsiX's Swift code is mostly macOS interface glue. What carries over is the
logic and data: measurement maths, slice handling, reslicing, colour tables,
presets and rules. Each item lists where to look in the IsiX source
(`Horos/Sources`, `Horos/Resources`) and the Rust building blocks.

## Done (v0.1)

- [x] Folder import, study/series grouping, slice sorting
- [x] 2D viewer: stack, window/level, zoom, pan, invert, cine, CT presets
- [x] Length and angle measurements; value under cursor
- [x] JPEG, JPEG 2000, JPEG-LS, RLE, deflate, colour and multi-frame images

## Tier 1 — weeks; makes it a daily-use viewer

- [ ] **Local database** of imported studies, so folders don't need re-scanning.
      IsiX: `Database*`, `BrowserController*`. Rust: `rusqlite` (SQLite).
- [ ] **Series thumbnails** in the browser. IsiX: `Thumbnail*`.
- [ ] **ROIs**: ellipse, rectangle, polygon, freehand with mean / SD / min / max /
      area. IsiX: `ROI*` (≈70 files; the statistics code is the useful part).
- [ ] **Colour lookup tables** (Hot Iron, Rainbow, PET colours). IsiX ships them
      as data: `Resources/CLUTs/*.plist`. Rust: `plist` crate, load at startup.
- [ ] **Layouts and comparison**: 1×2, 2×2 viewports; synced scrolling by slice
      position; reference lines between orthogonal series.
      IsiX: `Orthogonal*`, `Comparative*`, `Sync*`.
- [ ] **Saved measurements** and key images per study.
- [ ] **Anonymize and export** (DICOM, PNG/JPEG, cine to MP4).
      IsiX: `Anonymization*` (the tag rules), `Export*`. Rust: dicom-rs can write files.
- [ ] **Open DICOMDIR** (CDs from other hospitals).
- [ ] **Patient orientation markers** (A/P/R/L/S/I) from Image Orientation.

## Tier 2 — months

- [ ] **MPR**: build a volume from a series, reslice axial / coronal / sagittal,
      oblique planes, thick-slab MIP / MinIP / average. IsiX: `MPR*`.
      Rust: CPU reslicing first (`rayon`), then GPU via `wgpu` (Direct3D 12 / Vulkan on Windows).
- [ ] **PACS query / retrieve / send** (C-ECHO, C-FIND, C-MOVE / C-GET, C-STORE).
      IsiX: `Query*`, `Retrieve*`, `Store*`, `AssociationContract*`.
      Rust: `dicom-ul` (the dicom-rs networking crate); dicom-rs also publishes
      working `dicom-findscu`, `dicom-movescu` and `dicom-storescu` tools to start from.
- [ ] **DICOMweb** (QIDO / WADO / STOW). IsiX: `DICOMWeb*`, `Wado*`. Rust: `reqwest`.
- [ ] **PET-CT fusion** with SUV (body weight from injected dose, decay
      correction). IsiX: `PET*`, `Fusion*`.
- [ ] **Hanging protocols**: automatic layout per modality and body part.
- [ ] **GSPS** display (saved annotations and window from other workstations).
      IsiX: `*GSPS*`.
- [ ] 12-bit lossy JPEG (via the `gdcm` feature of dicom-pixeldata, or `libjpeg-turbo`).

## Tier 3 — large or research-grade; only if really needed

- [ ] 3D volume rendering with transfer-function presets
      (`Resources/3DPRESETS` are reusable data). `wgpu` ray-casting.
- [ ] Curved MPR (`CPR*`), virtual endoscopy, surface rendering.
- [ ] Segmentation (region growing), registration, subtraction.
- [ ] Structured reports (`SR*`).

## Not planned

- OsiriX/Horos plugin compatibility (Objective-C plugin API)
- DICOM film printing, CD/DVD burning, built-in web portal, Bonjour sharing
- Reading the IsiX/Horos CoreData database
