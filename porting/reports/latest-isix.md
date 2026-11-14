## IsiX: 17 new commits

From [`a05084b6`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/a05084b61e78) (last reviewed) to [`3e248312`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/3e24831224b8), 2026-10-16 → 2026-11-13. Branch `horos`.

### 1. Review: areas RDicom already has

#### Study browser: study list, series thumbnails, previews, albums, search, comparison
`browser` · P1 · daily core · Rebuild · RDicom: src/app.rs (browser panel), src/dicom_index.rs · 118 files, +16742 −27456

- [ ] [`5a8d5161`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/5a8d516115ff) 2026-10-16: Improve imaging workflows, viewer controls, and development tooling
- [ ] [`c0b657f2`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/c0b657f29fa2) 2026-10-19: Refine viewer annotations, study comparisons, responsiveness, and packaging
- [ ] [`1c463f14`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/1c463f14f5f1) 2026-10-20: Expand imaging tools and streamline ROI management, DICOM retrieval, and viewer performance
- [ ] [`57982101`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/579821015297) 2026-10-24: Support macOS 15 and refine rendering, viewer interaction, and DICOM workflows
- [ ] [`6e02be4b`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/6e02be4b99ea) 2026-10-24: Exponential 2D slab slider; Numbers hides colors
- [ ] [`aad6c4bf`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/aad6c4bf0afc) 2026-10-26: Refine precise dragging and thickness controls, series identification, DICOM listener compatibility and stabilize retrieve inventories
- [ ] [`061d7b3a`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/061d7b3a8498) 2026-10-26: Streamline database previews and parallelize large JPEG 2000 decoding
- [ ] [`0cc3a6c2`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/0cc3a6c25063) 2026-10-31: Defer automatic queries until first-use database setup completes
- [ ] [`4d9f644c`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/4d9f644cbdce) 2026-11-04: Fix off-main-thread table draw, ROI label clicks, and label contrast cap
- [ ] [`3d4e52fd`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/3d4e52fd9631) 2026-11-12: Release IsiX DICOM Viewer 6.0.0 with modular Swift architecture and plugin compatibility
- [ ] [`cac52277`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/cac5227721c7) 2026-11-12: Update DICOM-Swift, improve archive handling and image display, and showcase supported platforms
- [ ] [`09811fc4`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/09811fc4d3e0) 2026-11-13: Improve DICOM data integrity, plugin recovery, reporting, and viewer workflows
- [ ] [`aceba6ac`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/aceba6ac9898) 2026-11-13: Add live ROI length ratios, improve contour editing and Word reports, and refine temporary preferences and SDK packaging

118 files changed (see the full report).

#### Local database and import: indexing, folders, archives, incoming, storage
`database-import` · P1 · daily core · Port · RDicom: src/dicom_index.rs; src/db/ (planned) · 229 files, +40585 −10219

- [ ] [`1c463f14`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/1c463f14f5f1) 2026-10-20: Expand imaging tools and streamline ROI management, DICOM retrieval, and viewer performance
- [ ] [`57982101`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/579821015297) 2026-10-24: Support macOS 15 and refine rendering, viewer interaction, and DICOM workflows
- [ ] [`3d4e52fd`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/3d4e52fd9631) 2026-11-12: Release IsiX DICOM Viewer 6.0.0 with modular Swift architecture and plugin compatibility
- [ ] [`09811fc4`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/09811fc4d3e0) 2026-11-13: Improve DICOM data integrity, plugin recovery, reporting, and viewer workflows

229 files changed (see the full report).

#### DICOM files: parsing, datasets, character sets, transfer syntaxes, DICOMDIR, codecs
`dicom-core` · P1 · daily core · Replace with crate · RDicom: src/dicom_index.rs, src/frame.rs · 142 files, +13410 −18682

- [ ] [`061d7b3a`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/061d7b3a8498) 2026-10-26: Streamline database previews and parallelize large JPEG 2000 decoding
- [ ] [`3d4e52fd`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/3d4e52fd9631) 2026-11-12: Release IsiX DICOM Viewer 6.0.0 with modular Swift architecture and plugin compatibility
- [ ] [`09811fc4`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/09811fc4d3e0) 2026-11-13: Improve DICOM data integrity, plugin recovery, reporting, and viewer workflows

142 files changed (see the full report).

#### Geometry: vectors, planes, patient coordinates, Bezier paths
`geometry` · P1 · daily core · Port · RDicom: src/geometry/ (vector3, line3, plane3, transform3d, bezier, orientation, image_plane) · 38 files, +5243 −873

- [ ] [`1c463f14`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/1c463f14f5f1) 2026-10-20: Expand imaging tools and streamline ROI management, DICOM retrieval, and viewer performance
- [ ] [`3d4e52fd`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/3d4e52fd9631) 2026-11-12: Release IsiX DICOM Viewer 6.0.0 with modular Swift architecture and plugin compatibility
- [ ] [`09811fc4`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/09811fc4d3e0) 2026-11-13: Improve DICOM data integrity, plugin recovery, reporting, and viewer workflows

38 files changed (see the full report).

#### Pixel pipeline: frame loading, rescale, window/level, VOI LUT, overlays, image maths
`imaging` · P1 · daily core · Port · RDicom: src/imaging/ (window, voi_lut, defaults, photometry, clut, clut_tables, convolution), src/frame.rs, src/loader.rs · 98 files, +18066 −12135

- [ ] [`5a8d5161`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/5a8d516115ff) 2026-10-16: Improve imaging workflows, viewer controls, and development tooling
- [ ] [`fd2563a4`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/fd2563a49be7) 2026-10-18: Customize ROI statistics and fix imaging memory leaks
- [ ] [`1c463f14`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/1c463f14f5f1) 2026-10-20: Expand imaging tools and streamline ROI management, DICOM retrieval, and viewer performance
- [ ] [`57982101`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/579821015297) 2026-10-24: Support macOS 15 and refine rendering, viewer interaction, and DICOM workflows
- [ ] [`3d4e52fd`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/3d4e52fd9631) 2026-11-12: Release IsiX DICOM Viewer 6.0.0 with modular Swift architecture and plugin compatibility
- [ ] [`cac52277`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/cac5227721c7) 2026-11-12: Update DICOM-Swift, improve archive handling and image display, and showcase supported platforms
- [ ] [`09811fc4`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/09811fc4d3e0) 2026-11-13: Improve DICOM data integrity, plugin recovery, reporting, and viewer workflows
- [ ] [`aceba6ac`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/aceba6ac9898) 2026-11-13: Add live ROI length ratios, improve contour editing and Word reports, and refine temporary preferences and SDK packaging

98 files changed (see the full report).

#### ROIs and measurements: length, angle, ellipse, polygon, brush, statistics, ROI manager
`roi-measurements` · P1 · daily core · Port · RDicom: src/roi/ (geometry, statistics, raster, mod), src/app.rs · 159 files, +26334 −14186

- [ ] [`5a8d5161`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/5a8d516115ff) 2026-10-16: Improve imaging workflows, viewer controls, and development tooling
- [ ] [`dc48a2f9`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/dc48a2f9d3ac) 2026-10-18: Improve imaging workflows, viewer reliability, reporting, and tooling
- [ ] [`fd2563a4`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/fd2563a49be7) 2026-10-18: Customize ROI statistics and fix imaging memory leaks
- [ ] [`c0b657f2`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/c0b657f29fa2) 2026-10-19: Refine viewer annotations, study comparisons, responsiveness, and packaging
- [ ] [`1c463f14`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/1c463f14f5f1) 2026-10-20: Expand imaging tools and streamline ROI management, DICOM retrieval, and viewer performance
- [ ] [`57982101`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/579821015297) 2026-10-24: Support macOS 15 and refine rendering, viewer interaction, and DICOM workflows
- [ ] [`4d9f644c`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/4d9f644cbdce) 2026-11-04: Fix off-main-thread table draw, ROI label clicks, and label contrast cap
- [ ] [`3d4e52fd`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/3d4e52fd9631) 2026-11-12: Release IsiX DICOM Viewer 6.0.0 with modular Swift architecture and plugin compatibility
- [ ] [`aceba6ac`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/aceba6ac9898) 2026-11-13: Add live ROI length ratios, improve contour editing and Word reports, and refine temporary preferences and SDK packaging

159 files changed (see the full report).

#### 2D viewer: viewport, tools, scrolling, cine, annotations, layouts, sync, toolbar
`viewer-2d` · P1 · daily core · Rebuild · RDicom: src/app.rs, src/gpu_view.rs, src/navigation.rs, src/imaging/drag.rs, src/imaging/presets.rs · 329 files, +58750 −45086

- [ ] [`5a8d5161`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/5a8d516115ff) 2026-10-16: Improve imaging workflows, viewer controls, and development tooling
- [ ] [`dc48a2f9`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/dc48a2f9d3ac) 2026-10-18: Improve imaging workflows, viewer reliability, reporting, and tooling
- [ ] [`fd2563a4`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/fd2563a49be7) 2026-10-18: Customize ROI statistics and fix imaging memory leaks
- [ ] [`c0b657f2`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/c0b657f29fa2) 2026-10-19: Refine viewer annotations, study comparisons, responsiveness, and packaging
- [ ] [`1c463f14`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/1c463f14f5f1) 2026-10-20: Expand imaging tools and streamline ROI management, DICOM retrieval, and viewer performance
- [ ] [`57982101`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/579821015297) 2026-10-24: Support macOS 15 and refine rendering, viewer interaction, and DICOM workflows
- [ ] [`6e02be4b`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/6e02be4b99ea) 2026-10-24: Exponential 2D slab slider; Numbers hides colors
- [ ] [`aad6c4bf`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/aad6c4bf0afc) 2026-10-26: Refine precise dragging and thickness controls, series identification, DICOM listener compatibility and stabilize retrieve inventories
- [ ] [`4d9f644c`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/4d9f644cbdce) 2026-11-04: Fix off-main-thread table draw, ROI label clicks, and label contrast cap
- [ ] [`3d4e52fd`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/3d4e52fd9631) 2026-11-12: Release IsiX DICOM Viewer 6.0.0 with modular Swift architecture and plugin compatibility
- [ ] [`cac52277`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/cac5227721c7) 2026-11-12: Update DICOM-Swift, improve archive handling and image display, and showcase supported platforms

*Report cut short to fit. *
