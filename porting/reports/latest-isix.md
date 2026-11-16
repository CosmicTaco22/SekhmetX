## IsiX: 17 new commits

From [`a05084b6`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/a05084b61e78) (last reviewed) to [`087f2cd0`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/087f2cd0f721), 2026-10-16 → 2026-11-16. Branch `horos`.

### 1. Review: areas RDicom already has

#### Study browser: study list, series thumbnails, previews, albums, search, comparison
`browser` · P1 · daily core · Rebuild · RDicom: src/app.rs (browser panel), src/dicom_index.rs · 118 files, +16815 −27473

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
- [ ] [`087f2cd0`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/087f2cd0f721) 2026-11-16: Expand vessel analysis, improve DICOM and PACS workflows, and refine plugins and localization for 6.0.4

<details><summary>Files</summary>

| File | Change | Lines |
|---|---|---|
| `Horos/Sources/BrowserController.m` | deleted | +219 −15627 |
| [`Horos/Sources/BrowserController+Sources.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/BrowserController+Sources.swift) | changed | +172 −1258 |
| [`Horos/Sources/BrowserController+DatabaseDragExport.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/BrowserController+DatabaseDragExport.swift) | changed | +35 −1342 |
| [`Horos/Sources/BrowserController+Preview.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/BrowserController+Preview.swift) | changed | +61 −1230 |
| [`Horos/Sources/BrowserController+DatabaseDragExport+Selection.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/BrowserController+DatabaseDragExport+Selection.swift) | changed | +40 −1221 |
| [`Horos/Sources/SeriesListPresenter.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/SeriesListPresenter.swift) | changed | +1214 −0 |
| [`Horos/Sources/BrowserController+Toolbar.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/BrowserController+Toolbar.swift) | changed | +33 −1079 |
| [`Horos/Sources/BrowserToolbar.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/BrowserToolbar.swift) | new | +1095 −0 |
| [`Horos/Sources/BrowserPreviewPane.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/BrowserPreviewPane.swift) | new | +1013 −0 |
| [`Horos/Sources/StudyListContent.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/StudyListContent.swift) | changed | +978 −2 |
| [`Horos/Sources/SourceList.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/SourceList.swift) | new | +899 −0 |
| `Horos/Sources/BrowserController+Reports.swift` | deleted | +0 −768 |
| `Horos/Sources/BrowserController+SwiftIvars.m` | deleted | +0 −692 |
| [`Horos/Sources/BrowserController+Window.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/BrowserController+Window.swift) | changed | +667 −14 |
| [`Horos/Sources/BrowserController+AlbumsTableView.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/BrowserController+AlbumsTableView.swift) | changed | +48 −619 |
| [`Horos/Sources/BrowserController.h`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/BrowserController.h) | changed | +53 −580 |
| [`Horos/Sources/BrowserController+DatabaseMaintenance.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/BrowserController+DatabaseMaintenance.swift) | new | +580 −0 |
| [`Horos/Sources/BrowserController+StateAccess.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/BrowserController+StateAccess.swift) | new | +569 −0 |
| [`Horos/Sources/BrowserController+DatabaseSwitching.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/BrowserController+DatabaseSwitching.swift) | new | +543 −0 |
| [`Horos/Sources/BrowserController+Plugins.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/BrowserController+Plugins.swift) | changed | +96 −415 |
| [`Horos/Sources/BrowserController.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/BrowserController.swift) | changed | +381 −3 |
| [`Horos/Sources/StudyListExport.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/StudyListExport.swift) | new | +376 −0 |
| [`Horos/Sources/BrowserController+Sources+Copy.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/BrowserController+Sources+Copy.swift) | changed | +4 −367 |
| [`Horos/Sources/BrowserController+SelectionFiles.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/BrowserController+SelectionFiles.swift) | new | +341 −0 |
| [`Horos/Sources/BrowserController+OutlineDataSource.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/BrowserController+OutlineDataSource.swift) | new | +340 −0 |
| [`Horos/Sources/ThumbnailLoader.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ThumbnailLoader.swift) | changed | +334 −1 |
| `Horos/Sources/BrowserController+SwiftIvars.h` | deleted | +0 −333 |
| [`Horos/Sources/StudyListCellStyler.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/StudyListCellStyler.swift) | new | +269 −0 |
| [`Horos/Sources/BrowserController+DistantSearch.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/BrowserController+DistantSearch.swift) | new | +252 −0 |
| `Horos/Sources/BrowserControllerDCMTKCategory.mm` | deleted | +0 −240 |
| [`Horos/Sources/BrowserController+OutlineSelection.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/BrowserController+OutlineSelection.swift) | new | +239 −0 |
| [`Horos/Sources/BrowserController+ClassState.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/BrowserController+ClassState.swift) | new | +238 −0 |
| [`Horos/Sources/BrowserState.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/BrowserState.swift) | new | +233 −0 |
| [`Horos/Sources/BrowserController+ExcludedSeries.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/BrowserController+ExcludedSeries.swift) | changed | +138 −90 |
| [`Horos/Sources/BrowserController+Files.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/BrowserController+Files.swift) | new | +214 −0 |
| [`Horos/Sources/BrowserOutlets.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/BrowserOutlets.swift) | new | +213 −0 |
| [`Horos/Sources/BrowserController+DatabaseImport.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/BrowserController+DatabaseImport.swift) | new | +201 −0 |
| [`Horos/Sources/BrowserController+Activity.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/BrowserController+Activity.swift) | changed | +148 −45 |
| [`Horos/Sources/BrowserController+DICOMExport.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/BrowserController+DICOMExport.swift) | new | +186 −0 |
| [`Horos/Sources/BrowserController+Albums.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/BrowserController+Albums.swift) | changed | +174 −6 |
| … and 78 more | | |

</details>

#### Colour tables, 3D presets, DICOM dictionaries
`data-assets` · P1 · daily core · Reuse data · RDicom: src/imaging/clut.rs, src/imaging/clut_tables.rs · 10 files, +140 −0

- [ ] [`087f2cd0`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/087f2cd0f721) 2026-11-16: Expand vessel analysis, improve DICOM and PACS workflows, and refine plugins and localization for 6.0.4

<details><summary>Files</summary>

| File | Change | Lines |
|---|---|---|
| [`Horos/Resources/3DPRESETS/Bone + Skin II.plist`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Resources/3DPRESETS/Bone%20+%20Skin%20II.plist) | changed | +14 −0 |
| [`Horos/Resources/3DPRESETS/Bone + Skin.plist`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Resources/3DPRESETS/Bone%20+%20Skin.plist) | changed | +14 −0 |
| [`Horos/Resources/3DPRESETS/Dark Bone.plist`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Resources/3DPRESETS/Dark%20Bone.plist) | changed | +14 −0 |
| [`Horos/Resources/3DPRESETS/Gold Bone.plist`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Resources/3DPRESETS/Gold%20Bone.plist) | changed | +14 −0 |
| [`Horos/Resources/3DPRESETS/Skin On Blue II.plist`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Resources/3DPRESETS/Skin%20On%20Blue%20II.plist) | changed | +14 −0 |
| [`Horos/Resources/3DPRESETS/Skin On Blue.plist`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Resources/3DPRESETS/Skin%20On%20Blue.plist) | changed | +14 −0 |
| [`Horos/Resources/3DPRESETS/Soft + Skin II.plist`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Resources/3DPRESETS/Soft%20+%20Skin%20II.plist) | changed | +14 −0 |
| [`Horos/Resources/3DPRESETS/Soft + Skin III.plist`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Resources/3DPRESETS/Soft%20+%20Skin%20III.plist) | changed | +14 −0 |
| [`Horos/Resources/3DPRESETS/Soft + Skin.plist`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Resources/3DPRESETS/Soft%20+%20Skin.plist) | changed | +14 −0 |
| [`Horos/Resources/3DPRESETS/Yellow Bone.plist`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Resources/3DPRESETS/Yellow%20Bone.plist) | changed | +14 −0 |

</details>

#### Local database and import: indexing, folders, archives, incoming, storage
`database-import` · P1 · daily core · Port · RDicom: src/dicom_index.rs, src/db.rs · 233 files, +42286 −10380

- [ ] [`1c463f14`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/1c463f14f5f1) 2026-10-20: Expand imaging tools and streamline ROI management, DICOM retrieval, and viewer performance
- [ ] [`57982101`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/579821015297) 2026-10-24: Support macOS 15 and refine rendering, viewer interaction, and DICOM workflows
- [ ] [`3d4e52fd`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/3d4e52fd9631) 2026-11-12: Release IsiX DICOM Viewer 6.0.0 with modular Swift architecture and plugin compatibility
- [ ] [`09811fc4`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/09811fc4d3e0) 2026-11-13: Improve DICOM data integrity, plugin recovery, reporting, and viewer workflows
- [ ] [`087f2cd0`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/087f2cd0f721) 2026-11-16: Expand vessel analysis, improve DICOM and PACS workflows, and refine plugins and localization for 6.0.4

<details><summary>Files</summary>

| File | Change | Lines |
|---|---|---|
| `Horos/Sources/DicomDatabase.mm` | deleted | +1 −3680 |
| `Horos/Sources/DicomDatabase+Other.swift` | deleted | +0 −1206 |
| `Horos/Sources/DicomDatabase+Scan.mm` | deleted | +1 −948 |
| [`Horos/Sources/DatabaseRebuildService.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DatabaseRebuildService.swift) | changed | +900 −1 |
| [`Horos/Sources/DicomDatabase+Clean.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DicomDatabase+Clean.swift) | changed | +19 −835 |
| [`Horos/Sources/DatabaseModelUpgrade.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DatabaseModelUpgrade.swift) | new | +789 −0 |
| [`Horos/Sources/DicomDatabase+Routing.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DicomDatabase+Routing.swift) | changed | +21 −740 |
| [`Horos/Sources/DicomDatabase.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DicomDatabase.swift) | changed | +740 −9 |
| [`Horos/Sources/IncomingAdmission.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/IncomingAdmission.swift) | changed | +745 −2 |
| [`Horos/Sources/DatabaseCleanupService.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DatabaseCleanupService.swift) | new | +736 −0 |
| [`Modules/IsiXDatabase/Sources/StudyListRowRules.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXDatabase/Sources/StudyListRowRules.swift) | new | +719 −0 |
| [`Horos/Sources/PatientHistoryList.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/PatientHistoryList.swift) | changed | +682 −13 |
| [`Horos/Sources/ArchiveImport.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ArchiveImport.swift) | changed | +594 −11 |
| [`Horos/Sources/StudyDeletionJob.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/StudyDeletionJob.swift) | new | +599 −0 |
| `Horos/Sources/DicomDatabase+DCMTK.mm` | deleted | +0 −593 |
| [`Modules/IsiXDatabase/Sources/DatabaseMaintenanceRules.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXDatabase/Sources/DatabaseMaintenanceRules.swift) | changed | +583 −0 |
| [`Modules/IsiXDatabase/Sources/StudyListRules.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXDatabase/Sources/StudyListRules.swift) | new | +570 −0 |
| [`Modules/IsiXDatabase/Tests/AutoCleanupPolicyTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXDatabase/Tests/AutoCleanupPolicyTests.swift) | new | +569 −0 |
| [`Horos/Sources/DicomStudy.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DicomStudy.swift) | changed | +235 −324 |
| [`Horos/Sources/LegacyStudiesImport.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/LegacyStudiesImport.swift) | changed | +506 −51 |
| [`Modules/IsiXDatabase/Sources/DecompressConversion.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXDatabase/Sources/DecompressConversion.swift) | changed | +543 −11 |
| [`Modules/IsiXStorage/Sources/LegacyPluginImport.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXStorage/Sources/LegacyPluginImport.swift) | changed | +463 −41 |
| [`Modules/IsiXDatabase/Sources/PACSOnDemandSearch.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXDatabase/Sources/PACSOnDemandSearch.swift) | changed | +494 −8 |
| [`Modules/IsiXDatabase/Tests/IsiXDatabaseTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXDatabase/Tests/IsiXDatabaseTests.swift) | new | +496 −0 |
| [`Modules/IsiXDatabase/Sources/AlbumCatalog.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXDatabase/Sources/AlbumCatalog.swift) | changed | +487 −0 |
| [`Modules/IsiXDatabase/Tests/PACSOnDemandSearchTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXDatabase/Tests/PACSOnDemandSearchTests.swift) | changed | +479 −3 |
| [`Modules/IsiXDatabase/Sources/BrowserWindowRules.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXDatabase/Sources/BrowserWindowRules.swift) | new | +475 −0 |
| [`Modules/IsiXDatabase/Tests/IndexRepairRulesTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXDatabase/Tests/IndexRepairRulesTests.swift) | new | +471 −0 |
| [`Horos/Sources/SourceCopyJob.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/SourceCopyJob.swift) | new | +457 −0 |
| [`Horos/Sources/DatabaseIngest.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DatabaseIngest.swift) | new | +456 −0 |
| [`Horos/Sources/MediaImportJob.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/MediaImportJob.swift) | new | +452 −0 |
| [`Horos/Sources/MediaVolumeScan.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/MediaVolumeScan.swift) | changed | +448 −1 |
| [`Horos/Sources/DatabaseOpener.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DatabaseOpener.swift) | new | +447 −0 |
| [`Modules/IsiXStorage/Sources/LegacyDatabase.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXStorage/Sources/LegacyDatabase.swift) | changed | +427 −16 |
| [`Horos/Sources/DatabaseFileConversion.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DatabaseFileConversion.swift) | changed | +439 −1 |
| [`Modules/IsiXDatabase/Sources/DatabaseImportRules.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXDatabase/Sources/DatabaseImportRules.swift) | new | +437 −0 |
| [`Horos/Sources/DatabaseFilePromiseWriter.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DatabaseFilePromiseWriter.swift) | new | +429 −0 |
| [`Horos/Sources/DicomDatabase+Instance.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DicomDatabase+Instance.swift) | changed | +150 −260 |
| [`Horos/Sources/StudyMergeJob.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/StudyMergeJob.swift) | new | +406 −0 |
| [`Horos/Sources/DicomDatabase+Albums.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DicomDatabase+Albums.swift) | changed | +41 −364 |
| … and 193 more | | |

</details>

#### DICOM files: parsing, datasets, character sets, transfer syntaxes, DICOMDIR, codecs
`dicom-core` · P1 · daily core · Replace with crate · RDicom: src/dicom_index.rs, src/frame.rs · 142 files, +13457 −18709

- [ ] [`061d7b3a`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/061d7b3a8498) 2026-10-26: Streamline database previews and parallelize large JPEG 2000 decoding
- [ ] [`3d4e52fd`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/3d4e52fd9631) 2026-11-12: Release IsiX DICOM Viewer 6.0.0 with modular Swift architecture and plugin compatibility
- [ ] [`09811fc4`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/09811fc4d3e0) 2026-11-13: Improve DICOM data integrity, plugin recovery, reporting, and viewer workflows
- [ ] [`087f2cd0`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/087f2cd0f721) 2026-11-16: Expand vessel analysis, improve DICOM and PACS workflows, and refine plugins and localization for 6.0.4

<details><summary>Files</summary>

| File | Change | Lines |
|---|---|---|
| `Horos/Sources/DicomFile.mm` | deleted | +0 −2491 |
| `DCM Framework/DCMPixelDataAttribute.mm` | deleted | +0 −1654 |
| [`Horos/Sources/DicomFileIndexer.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DicomFileIndexer.swift) | new | +1488 −0 |
| `DCM Framework/DCMObject.m` | deleted | +0 −1485 |
| `Horos/Sources/DicomFileDCMTKCategory.mm` | deleted | +0 −1464 |
| `DCM Framework/DCMAbstractSyntaxUID.m` | deleted | +0 −1063 |
| [`Horos/Sources/DicomFile+DICOMReading.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DicomFile+DICOMReading.swift) | new | +868 −0 |
| `Horos/Sources/HorosJPEG2000Codec.cpp` | deleted | +17 −707 |
| `DCM Framework/DCMCalendarDate.m` | deleted | +0 −724 |
| `Horos/Sources/HorosDICOMWriter.mm` | deleted | +0 −659 |
| [`Modules/IsiXDICOMFile/Sources/DICOMCalendarDate.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXDICOMFile/Sources/DICOMCalendarDate.swift) | new | +632 −0 |
| `Horos/Sources/HorosDCMTKObject.mm` | deleted | +0 −569 |
| `DCM Framework/DCMNetServiceDelegate.m` | deleted | +0 −564 |
| `DCM Framework/DCMValueRepresentation.m` | deleted | +0 −555 |
| [`Modules/IsiXDICOMDataset/Sources/DCMObject.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXDICOMDataset/Sources/DCMObject.swift) | new | +488 −0 |
| [`Modules/IsiXDICOMDataset/Sources/DCMPixelDataAttribute.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXDICOMDataset/Sources/DCMPixelDataAttribute.swift) | new | +461 −0 |
| `Horos/Sources/DCMWaveform.m` | deleted | +0 −432 |
| [`Modules/IsiXDICOMFile/Sources/SecondaryCaptureExport.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXDICOMFile/Sources/SecondaryCaptureExport.swift) | changed | +430 −1 |
| [`Horos/Sources/DicomdirReader.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DicomdirReader.swift) | changed | +418 −12 |
| `DCM Framework/DCMCharacterSet.m` | deleted | +0 −418 |
| [`Modules/IsiXDICOMDataset/Sources/DCMPixelDataAttribute+Colour.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXDICOMDataset/Sources/DCMPixelDataAttribute+Colour.swift) | new | +403 −0 |
| `DCM Framework/DCMAttribute.m` | deleted | +0 −402 |
| [`Horos/Sources/DicomFile+NonDICOMReaders.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DicomFile+NonDICOMReaders.swift) | new | +396 −0 |
| [`Modules/IsiXDICOMFile/Sources/DICOMSOPClass.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXDICOMFile/Sources/DICOMSOPClass.swift) | changed | +335 −0 |
| [`Horos/Sources/DicomFile+Formats.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DicomFile+Formats.swift) | new | +314 −0 |
| `DCM Framework/DCMTransferSyntax.m` | deleted | +0 −309 |
| [`Modules/IsiXDICOMFile/Sources/SelectedFieldAnonymization.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXDICOMFile/Sources/SelectedFieldAnonymization.swift) | new | +309 −0 |
| `DCM Framework/DCMObject.h` | deleted | +0 −306 |
| [`Modules/IsiXDICOMDataset/Sources/DCMAttribute.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXDICOMDataset/Sources/DCMAttribute.swift) | new | +295 −0 |
| `Horos/Sources/DCMWaveform.h` | deleted | +0 −292 |
| [`Modules/IsiXDICOMFile/Sources/StudyFileImport.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXDICOMFile/Sources/StudyFileImport.swift) | new | +292 −0 |
| [`Modules/IsiXDICOMFile/Tests/ExportRulesTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXDICOMFile/Tests/ExportRulesTests.swift) | changed | +287 −0 |
| [`Modules/IsiXDICOMFile/Sources/NonDICOMImportHeaders.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXDICOMFile/Sources/NonDICOMImportHeaders.swift) | new | +262 −0 |
| [`Modules/IsiXDICOMFile/Sources/DICOMCharacterSet.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXDICOMFile/Sources/DICOMCharacterSet.swift) | new | +259 −0 |
| [`Modules/IsiXDICOMFile/Sources/DICOMAttributeTag.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXDICOMFile/Sources/DICOMAttributeTag.swift) | new | +230 −0 |
| [`Modules/IsiXDICOMFile/Sources/KeyObjectDocumentContent.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXDICOMFile/Sources/KeyObjectDocumentContent.swift) | new | +221 −0 |
| `Horos/Sources/FVTiff.m` | deleted | +0 −215 |
| [`Modules/IsiXDICOMFile/Sources/DICOMImportRules.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXDICOMFile/Sources/DICOMImportRules.swift) | new | +201 −0 |
| `DCM Framework/DCMAttributeTag.m` | deleted | +0 −198 |
| `DCM Framework/DCMAbstractSyntaxUID.h` | deleted | +0 −196 |
| … and 102 more | | |

</details>

#### Geometry: vectors, planes, patient coordinates, Bezier paths
`geometry` · P1 · daily core · Port · RDicom: src/geometry/ (vector3, line3, plane3, transform3d, bezier, orientation, image_plane) · 38 files, +5243 −873

- [ ] [`1c463f14`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/1c463f14f5f1) 2026-10-20: Expand imaging tools and streamline ROI management, DICOM retrieval, and viewer performance
- [ ] [`3d4e52fd`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/3d4e52fd9631) 2026-11-12: Release IsiX DICOM Viewer 6.0.0 with modular Swift architecture and plugin compatibility
- [ ] [`09811fc4`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/09811fc4d3e0) 2026-11-13: Improve DICOM data integrity, plugin recovery, reporting, and viewer workflows

<details><summary>Files</summary>

| File | Change | Lines |
|---|---|---|
| [`Modules/IsiXImaging/Tests/ViewGeometry/ViewportTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Tests/ViewGeometry/ViewportTests.swift) | new | +435 −0 |
| [`Modules/IsiXGeometry/Sources/BezierCore.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXGeometry/Sources/BezierCore.swift) | new | +373 −0 |
| [`Modules/IsiXGeometry/Sources/BezierCore+Sampling.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXGeometry/Sources/BezierCore+Sampling.swift) | new | +354 −0 |
| [`Modules/IsiXImaging/Sources/ViewGeometry/ViewTransform.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/ViewGeometry/ViewTransform.swift) | new | +349 −0 |
| [`Modules/IsiXImaging/Sources/ViewGeometry/ViewTransform+Viewport.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/ViewGeometry/ViewTransform+Viewport.swift) | new | +310 −0 |
| [`Modules/IsiXGeometry/Sources/BezierCore+Measuring.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXGeometry/Sources/BezierCore+Measuring.swift) | new | +301 −0 |
| [`Modules/IsiXImaging/Sources/Geometry/ImagePlaneCoordinates.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/Geometry/ImagePlaneCoordinates.swift) | new | +279 −0 |
| [`Modules/IsiXImaging/Sources/Geometry/ImageRendering.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/Geometry/ImageRendering.swift) | changed | +236 −2 |
| [`Modules/IsiXImaging/Sources/ViewGeometry/PlaneGeometry.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/ViewGeometry/PlaneGeometry.swift) | new | +228 −0 |
| [`Modules/IsiXGeometry/Sources/Vector3.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXGeometry/Sources/Vector3.swift) | new | +206 −0 |
| [`Modules/IsiXGeometry/Sources/BezierCore+Editing.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXGeometry/Sources/BezierCore+Editing.swift) | new | +204 −0 |
| [`Modules/IsiXImaging/Tests/Geometry/ImagePlaneCoordinatesTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Tests/Geometry/ImagePlaneCoordinatesTests.swift) | changed | +194 −0 |
| [`Modules/IsiXImaging/Sources/ViewGeometry/ViewportState.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/ViewGeometry/ViewportState.swift) | new | +183 −0 |
| `Horos/Sources/Quaternion.mm` | deleted | +0 −173 |
| `Horos/Sources/OSIGeometry.m` | deleted | +0 −153 |
| [`Modules/IsiXGeometry/Sources/Line3.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXGeometry/Sources/Line3.swift) | new | +149 −0 |
| [`Modules/IsiXImaging/Tests/ViewGeometry/ViewGeometryTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Tests/ViewGeometry/ViewGeometryTests.swift) | new | +145 −0 |
| [`Horos/Sources/OSIGeometry.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/OSIGeometry.swift) | new | +139 −0 |
| [`Modules/IsiXGeometry/Sources/BezierCore+Nodes.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXGeometry/Sources/BezierCore+Nodes.swift) | new | +132 −0 |
| [`Modules/IsiXImaging/Sources/ViewGeometry/LineGeometry.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/ViewGeometry/LineGeometry.swift) | new | +129 −0 |
| [`Modules/IsiXImaging/Sources/ViewGeometry/ViewportRestoration.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/ViewGeometry/ViewportRestoration.swift) | new | +129 −0 |
| [`Horos/Sources/GantryTiltPolicy.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/GantryTiltPolicy.swift) | changed | +118 −3 |
| [`Modules/IsiXGeometry/Sources/Plane3.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXGeometry/Sources/Plane3.swift) | new | +121 −0 |
| `Horos/Sources/Quaternion.h` | deleted | +0 −115 |
| [`Horos/Sources/Spline3D.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/Spline3D.swift) | new | +105 −0 |
| [`Modules/IsiXGeometry/Sources/AffineTransform3D.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXGeometry/Sources/AffineTransform3D.swift) | new | +101 −0 |
| [`Modules/IsiXGeometry/Tests/GeometryTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXGeometry/Tests/GeometryTests.swift) | new | +101 −0 |
| `Horos/Sources/Spline3D.mm` | deleted | +0 −97 |
| `Horos/Sources/Piecewise3D.mm` | deleted | +0 −84 |
| [`Horos/Sources/Piecewise3D.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/Piecewise3D.swift) | new | +73 −0 |
| [`Modules/IsiXGeometry/Sources/PackedVectors.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXGeometry/Sources/PackedVectors.swift) | new | +72 −0 |
| [`Modules/IsiXGeometry/Sources/PlanarGeometry.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXGeometry/Sources/PlanarGeometry.swift) | new | +67 −0 |
| `Horos/Sources/Spline3D.h` | deleted | +0 −61 |
| `Horos/Sources/Piecewise3D.h` | deleted | +0 −57 |
| `Horos/Sources/Interpolation3D.m` | deleted | +0 −49 |
| [`Horos/Sources/Point3D.h`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/Point3D.h) | changed | +0 −46 |
| [`Horos/Sources/OSIGeometry.h`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/OSIGeometry.h) | changed | +6 −31 |
| [`Horos/Sources/Interpolation3D.h`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/Interpolation3D.h) | changed | +4 −2 |

</details>

#### Pixel pipeline: frame loading, rescale, window/level, VOI LUT, overlays, image maths
`imaging` · P1 · daily core · Port · RDicom: src/imaging/ (window, voi_lut, defaults, photometry, clut, clut_tables, convolution), src/frame.rs, src/loader.rs · 102 files, +18836 −12283

- [ ] [`5a8d5161`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/5a8d516115ff) 2026-10-16: Improve imaging workflows, viewer controls, and development tooling
- [ ] [`fd2563a4`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/fd2563a49be7) 2026-10-18: Customize ROI statistics and fix imaging memory leaks
- [ ] [`1c463f14`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/1c463f14f5f1) 2026-10-20: Expand imaging tools and streamline ROI management, DICOM retrieval, and viewer performance
- [ ] [`57982101`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/579821015297) 2026-10-24: Support macOS 15 and refine rendering, viewer interaction, and DICOM workflows
- [ ] [`3d4e52fd`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/3d4e52fd9631) 2026-11-12: Release IsiX DICOM Viewer 6.0.0 with modular Swift architecture and plugin compatibility
- [ ] [`cac52277`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/cac5227721c7) 2026-11-12: Update DICOM-Swift, improve archive handling and image display, and showcase supported platforms
- [ ] [`09811fc4`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/09811fc4d3e0) 2026-11-13: Improve DICOM data integrity, plugin recovery, reporting, and viewer workflows
- [ ] [`aceba6ac`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/aceba6ac9898) 2026-11-13: Add live ROI length ratios, improve contour editing and Word reports, and refine temporary preferences and SDK packaging
- [ ] [`087f2cd0`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/087f2cd0f721) 2026-11-16: Expand vessel analysis, improve DICOM and PACS workflows, and refine plugins and localization for 6.0.4

<details><summary>Files</summary>

| File | Change | Lines |
|---|---|---|
| `Horos/Sources/DCMPix.m` | deleted | +12 −11670 |
| [`Horos/Sources/DCMPix.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DCMPix.swift) | changed | +1261 −0 |
| [`Horos/Sources/DCMPix+NonDICOM.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DCMPix+NonDICOM.swift) | changed | +775 −2 |
| [`Horos/Sources/ViewerWindowLevel.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ViewerWindowLevel.swift) | changed | +747 −17 |
| [`Horos/Sources/ViewerColourTables.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ViewerColourTables.swift) | changed | +726 −17 |
| [`Modules/IsiXImaging/Sources/ImageAttributeReader.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/ImageAttributeReader.swift) | changed | +515 −5 |
| [`Horos/Sources/DCMPix+Geometry.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DCMPix+Geometry.swift) | changed | +508 −6 |
| [`Modules/IsiXImaging/Tests/ImageAttributeReaderTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Tests/ImageAttributeReaderTests.swift) | changed | +476 −1 |
| [`Horos/Sources/DCMPix+Display.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DCMPix+Display.swift) | changed | +466 −0 |
| [`Horos/Sources/DCMPix.h`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DCMPix.h) | changed | +43 −422 |
| [`Horos/Sources/DCMPix+Loading.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DCMPix+Loading.swift) | changed | +405 −1 |
| [`Modules/IsiXImaging/Sources/ImageAttributeLoad.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/ImageAttributeLoad.swift) | new | +383 −0 |
| [`Horos/Sources/DCMPix+ROIMeasure.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DCMPix+ROIMeasure.swift) | new | +352 −0 |
| [`Horos/Sources/DCMPix+ROIFill.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DCMPix+ROIFill.swift) | changed | +327 −3 |
| [`Horos/Sources/DCMPix+FrameLoading.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DCMPix+FrameLoading.swift) | changed | +300 −25 |
| [`Horos/Sources/DCMPix+RTStruct.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DCMPix+RTStruct.swift) | new | +304 −0 |
| [`Horos/Sources/DCMPix+Annotations.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DCMPix+Annotations.swift) | new | +301 −0 |
| [`Horos/Sources/DCMPix+DisplaySource.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DCMPix+DisplaySource.swift) | new | +292 −0 |
| [`Modules/IsiXImaging/Tests/BufferOperations/SeriesConvolutionTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Tests/BufferOperations/SeriesConvolutionTests.swift) | changed | +288 −0 |
| [`Modules/IsiXImaging/Sources/FrameLoader.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/FrameLoader.swift) | changed | +269 −5 |
| [`Horos/Sources/DCMPix+ImageAttributes.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DCMPix+ImageAttributes.swift) | new | +258 −0 |
| [`Modules/IsiXImaging/Sources/FrameLoad.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/FrameLoad.swift) | changed | +248 −6 |
| [`Horos/Sources/DCMPix+Database.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DCMPix+Database.swift) | changed | +252 −1 |
| [`Modules/IsiXImaging/Tests/FrameLoaderTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Tests/FrameLoaderTests.swift) | changed | +243 −0 |
| [`Modules/IsiXImaging/Sources/Display/ShutterMask.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/Display/ShutterMask.swift) | changed | +223 −0 |
| [`Modules/IsiXImaging/Sources/ImagePlaneGeometry.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/ImagePlaneGeometry.swift) | new | +221 −0 |
| [`Modules/IsiXImaging/Sources/Display/ThumbnailIcon.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/Display/ThumbnailIcon.swift) | new | +205 −0 |
| [`Horos/Sources/ViewerPixelValues.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ViewerPixelValues.swift) | new | +201 −0 |
| [`Modules/IsiXImaging/Sources/ImageAttributeReader+FunctionalGroups.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/ImageAttributeReader+FunctionalGroups.swift) | new | +199 −0 |
| [`Modules/IsiXImaging/Sources/NonDICOM/LSMImage.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/NonDICOM/LSMImage.swift) | new | +193 −0 |
| [`Modules/IsiXImaging/Tests/NonDICOMFormatsTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Tests/NonDICOMFormatsTests.swift) | changed | +188 −0 |
| [`Horos/Sources/DCMPix+KeyValueCoding.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DCMPix+KeyValueCoding.swift) | new | +187 −0 |
| [`Modules/IsiXImaging/Tests/Display/DisplayTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Tests/Display/DisplayTests.swift) | changed | +186 −0 |
| [`Horos/Sources/RawDataImport.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/RawDataImport.swift) | new | +184 −0 |
| [`Modules/IsiXImaging/Sources/OverlayPlane.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/OverlayPlane.swift) | new | +168 −0 |
| [`Modules/IsiXImaging/Sources/FrameSamples.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/FrameSamples.swift) | new | +167 −0 |
| [`Modules/IsiXImaging/Sources/ImageAttributes.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/ImageAttributes.swift) | changed | +165 −1 |
| [`Horos/Sources/DCMPix+Display.h`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DCMPix+Display.h) | new | +160 −0 |
| [`Modules/IsiXImaging/Sources/BufferOperations/BufferOperationsBridge.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/BufferOperations/BufferOperationsBridge.swift) | new | +160 −0 |
| [`Horos/Sources/DCMPix+ROI.h`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DCMPix+ROI.h) | new | +155 −0 |
| … and 62 more | | |

</details>

#### ROIs and measurements: length, angle, ellipse, polygon, brush, statistics, ROI manager
`roi-measurements` · P1 · daily core · Port · RDicom: src/roi/ (geometry, statistics, raster, mod), src/app.rs · 164 files, +31572 −14687

- [ ] [`5a8d5161`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/5a8d516115ff) 2026-10-16: Improve imaging workflows, viewer controls, and development tooling
- [ ] [`dc48a2f9`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/dc48a2f9d3ac) 2026-10-18: Improve imaging workflows, viewer reliability, reporting, and tooling
- [ ] [`fd2563a4`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/fd2563a49be7) 2026-10-18: Customize ROI statistics and fix imaging memory leaks
- [ ] [`c0b657f2`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/c0b657f29fa2) 2026-10-19: Refine viewer annotations, study comparisons, responsiveness, and packaging
- [ ] [`1c463f14`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/1c463f14f5f1) 2026-10-20: Expand imaging tools and streamline ROI management, DICOM retrieval, and viewer performance
- [ ] [`57982101`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/579821015297) 2026-10-24: Support macOS 15 and refine rendering, viewer interaction, and DICOM workflows
- [ ] [`4d9f644c`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/4d9f644cbdce) 2026-11-04: Fix off-main-thread table draw, ROI label clicks, and label contrast cap
- [ ] [`3d4e52fd`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/3d4e52fd9631) 2026-11-12: Release IsiX DICOM Viewer 6.0.0 with modular Swift architecture and plugin compatibility
- [ ] [`aceba6ac`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/aceba6ac9898) 2026-11-13: Add live ROI length ratios, improve contour editing and Word reports, and refine temporary preferences and SDK packaging
- [ ] [`087f2cd0`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/087f2cd0f721) 2026-11-16: Expand vessel analysis, improve DICOM and PACS workflows, and refine plugins and localization for 6.0.4

<details><summary>Files</summary>

| File | Change | Lines |
|---|---|---|
| `Horos/Sources/ROI.m` | deleted | +229 −7843 |
| [`Modules/IsiXImaging/Sources/ViewerReconstruction/StenosisMeasurement.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/ViewerReconstruction/StenosisMeasurement.swift) | new | +2152 −0 |
| [`Modules/IsiXImaging/Tests/ViewerReconstruction/StenosisMeasurementTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Tests/ViewerReconstruction/StenosisMeasurementTests.swift) | new | +2051 −0 |
| [`Horos/Sources/StenosisTool.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/StenosisTool.swift) | changed | +1340 −86 |
| [`Horos/Sources/ROI.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ROI.swift) | changed | +1246 −0 |
| [`Horos/Sources/ROI+Label.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ROI+Label.swift) | changed | +1112 −48 |
| [`Horos/Sources/ROIManagerController.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ROIManagerController.swift) | changed | +905 −101 |
| [`Horos/Sources/ROI+Drawing.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ROI+Drawing.swift) | new | +856 −0 |
| [`Horos/Sources/ROI+Interaction.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ROI+Interaction.swift) | new | +856 −0 |
| `Horos/Sources/ROIVolumeView.mm` | deleted | +0 −810 |
| `Horos/Sources/CalciumScoringWindowController.m` | deleted | +0 −731 |
| [`Horos/Sources/ViewerROIInterchange.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ViewerROIInterchange.swift) | new | +727 −0 |
| `Horos/Sources/StenosisMeasurement.swift` | deleted | +342 −342 |
| [`Horos/Sources/ROI+Geometry.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ROI+Geometry.swift) | changed | +565 −1 |
| [`Modules/IsiXROI/Sources/ROIBrushMask.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXROI/Sources/ROIBrushMask.swift) | new | +469 −0 |
| [`Horos/Sources/ViewerLengthRatio.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ViewerLengthRatio.swift) | changed | +435 −7 |
| [`Horos/Sources/ROI.h`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ROI.h) | changed | +18 −417 |
| [`Horos/Sources/ROIVolumeView.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ROIVolumeView.swift) | new | +429 −0 |
| [`Modules/IsiXROI/Sources/ROIPlanarGeometry.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXROI/Sources/ROIPlanarGeometry.swift) | new | +424 −0 |
| [`Modules/IsiXROI/Sources/ROIArchiveCoder.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXROI/Sources/ROIArchiveCoder.swift) | changed | +416 −2 |
| `Horos/Sources/OSIPlanarPathROI.m` | deleted | +0 −408 |
| [`Modules/IsiXROI/Tests/ROIViewGestureTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXROI/Tests/ROIViewGestureTests.swift) | new | +407 −0 |
| `Horos/Sources/ROIVolume.mm` | deleted | +0 −389 |
| `Horos/Sources/OSICoalescedPlanarROI.m` | deleted | +0 −386 |
| [`Horos/Sources/BrushReslice.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/BrushReslice.swift) | new | +367 −0 |
| [`Horos/Sources/OSIROI.h`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/OSIROI.h) | changed | +14 −330 |
| [`Horos/Sources/OSIROIVolumeGeometry.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/OSIROIVolumeGeometry.swift) | new | +337 −0 |
| [`Modules/IsiXROI/Tests/ROIMeasurementTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXROI/Tests/ROIMeasurementTests.swift) | new | +332 −0 |
| [`Horos/Sources/HorosVolumeLengthROI.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/HorosVolumeLengthROI.swift) | new | +322 −0 |
| `Horos/Sources/OSIROI.m` | deleted | +0 −320 |
| `Horos/Sources/OSIPathExtrusionROI.m` | deleted | +0 −310 |
| [`Modules/IsiXROI/Sources/Rasterization/ROIPolygonFill.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXROI/Sources/Rasterization/ROIPolygonFill.swift) | new | +306 −0 |
| [`Horos/Sources/CalciumScoringWindowController.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/CalciumScoringWindowController.swift) | new | +303 −0 |
| [`Modules/IsiXROI/Sources/ROIInterchangeDocument.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXROI/Sources/ROIInterchangeDocument.swift) | new | +296 −0 |
| [`Horos/Sources/ViewerROIVolume.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ViewerROIVolume.swift) | new | +293 −0 |
| `Horos/Sources/OSIMaskROI.m` | deleted | +0 −287 |
| [`Horos/Sources/ROIVolume.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ROIVolume.swift) | new | +286 −0 |
| [`Modules/IsiXROI/Sources/Rasterization/ROIBrushRaster.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXROI/Sources/Rasterization/ROIBrushRaster.swift) | new | +285 −0 |
| [`Modules/IsiXROI/Sources/VolumeROIGeometry.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXROI/Sources/VolumeROIGeometry.swift) | new | +284 −0 |
| [`Modules/IsiXROI/Sources/ROICSVExport.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXROI/Sources/ROICSVExport.swift) | changed | +278 −3 |
| … and 124 more | | |

</details>

#### 2D viewer: viewport, tools, scrolling, cine, annotations, layouts, sync, toolbar
`viewer-2d` · P1 · daily core · Rebuild · RDicom: src/app.rs, src/gpu_view.rs, src/navigation.rs, src/imaging/drag.rs, src/imaging/presets.rs · 335 files, +60665 −45413

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
- [ ] [`09811fc4`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/09811fc4d3e0) 2026-11-13: Improve DICOM data integrity, plugin recovery, reporting, and viewer workflows
- [ ] [`aceba6ac`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/aceba6ac9898) 2026-11-13: Add live ROI length ratios, improve contour editing and Word reports, and refine temporary preferences and SDK packaging
- [ ] [`087f2cd0`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/087f2cd0f721) 2026-11-16: Expand vessel analysis, improve DICOM and PACS workflows, and refine plugins and localization for 6.0.4

<details><summary>Files</summary>

| File | Change | Lines |
|---|---|---|
| `Horos/Sources/ViewerController.m` | deleted | +289 −15719 |
| `Horos/Sources/DCMView.m` | deleted | +302 −9719 |
| [`Horos/Sources/ViewerController+ROI.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ViewerController+ROI.swift) | changed | +792 −1530 |
| [`Horos/Sources/ViewerController+ROI+Editing.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ViewerController+ROI+Editing.swift) | changed | +891 −1399 |
| [`Horos/Sources/ViewerOpener.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ViewerOpener.swift) | changed | +2055 −96 |
| [`Horos/Sources/DCMView+WindowLevel+Coordinates.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DCMView+WindowLevel+Coordinates.swift) | changed | +506 −1121 |
| `Horos/Sources/ViewerController+SwiftIvars.m` | deleted | +74 −1488 |
| [`Horos/Sources/ViewerController+Export.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ViewerController+Export.swift) | changed | +32 −1394 |
| [`Horos/Sources/ViewerController+Export+PrintMovie.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ViewerController+Export+PrintMovie.swift) | changed | +198 −1201 |
| [`Horos/Sources/ViewerController+Toolbar.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ViewerController+Toolbar.swift) | changed | +127 −1217 |
| [`Horos/Sources/DCMView+Drawing.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DCMView+Drawing.swift) | new | +1327 −0 |
| [`Horos/Sources/ViewerController+RetrieveAndView.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ViewerController+RetrieveAndView.swift) | changed | +99 −1192 |
| [`Horos/Sources/ViewerSeriesLoad.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ViewerSeriesLoad.swift) | changed | +1077 −88 |
| [`Horos/Sources/DCMView+Geometry.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DCMView+Geometry.swift) | changed | +1070 −6 |
| `Horos/Sources/DCMView+SwiftIvars.m` | deleted | +0 −1012 |
| [`Horos/Sources/ViewerController.h`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ViewerController.h) | changed | +30 −966 |
| [`Horos/Sources/ViewerController+SeriesPopup.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ViewerController+SeriesPopup.swift) | changed | +897 −33 |
| [`Horos/Sources/DCMView+Capture.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DCMView+Capture.swift) | new | +928 −0 |
| [`Horos/Sources/ViewerToolbarItemFactory.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ViewerToolbarItemFactory.swift) | changed | +871 −3 |
| [`Horos/Sources/DCMView+ROIEditing.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DCMView+ROIEditing.swift) | changed | +791 −60 |
| [`Horos/Sources/DCMView+WindowLevel.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DCMView+WindowLevel.swift) | changed | +343 −495 |
| [`Horos/Sources/DCMView+Mouse.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DCMView+Mouse.swift) | new | +821 −0 |
| [`Horos/Sources/DCMView.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DCMView.swift) | new | +782 −0 |
| [`Horos/Sources/ViewerWindowArranger.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ViewerWindowArranger.swift) | changed | +739 −43 |
| [`Modules/IsiXImaging/Tests/Annotations/AnnotationOverlayLayoutTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Tests/Annotations/AnnotationOverlayLayoutTests.swift) | new | +780 −0 |
| [`Horos/Sources/ViewerController+PanelOutlets.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ViewerController+PanelOutlets.swift) | new | +777 −0 |
| [`Horos/Sources/DCMView+MouseDragging.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DCMView+MouseDragging.swift) | changed | +178 −548 |
| [`Modules/IsiXImaging/Sources/ViewerWorkspace/ViewerTilingLayout.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/ViewerWorkspace/ViewerTilingLayout.swift) | new | +720 −0 |
| `Horos/Sources/OSIWindowController.m` | deleted | +0 −687 |
| [`Horos/Sources/ViewerController+ROIInterchange.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ViewerController+ROIInterchange.swift) | changed | +16 −671 |
| [`Horos/Sources/ViewerController+Convolution.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ViewerController+Convolution.swift) | changed | +32 −611 |
| [`Horos/Sources/ViewerSeriesLoader.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ViewerSeriesLoader.swift) | changed | +586 −47 |
| [`Horos/Sources/ViewerController+Blending.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ViewerController+Blending.swift) | changed | +13 −614 |
| `Horos/Sources/ViewerController+SwiftIvars.h` | deleted | +0 −571 |
| [`Horos/Sources/ViewerSessionGraph+ViewerLife.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ViewerSessionGraph+ViewerLife.swift) | changed | +529 −3 |
| [`Horos/Sources/ViewerOpener+Navigation.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ViewerOpener+Navigation.swift) | changed | +512 −4 |
| [`Horos/Sources/ViewerController+SliceInterval.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ViewerController+SliceInterval.swift) | changed | +479 −12 |
| [`Modules/IsiXImaging/Sources/Annotations/AnnotationOverlayLayout.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/Annotations/AnnotationOverlayLayout.swift) | new | +488 −0 |
| [`Horos/Sources/DCMView+Keyboard.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DCMView+Keyboard.swift) | changed | +470 −16 |
| [`Modules/IsiXImaging/Sources/ViewerOpening/ViewerOpeningRules.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/ViewerOpening/ViewerOpeningRules.swift) | new | +479 −0 |
| … and 295 more | | |

</details>

#### Anonymization and export (DICOM, images, movies)
`anonymize-export` · P2 · next · Port · RDicom: src/export.rs, src/ui/toolbar.rs, src/app.rs · 33 files, +4752 −2462

- [ ] [`57982101`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/579821015297) 2026-10-24: Support macOS 15 and refine rendering, viewer interaction, and DICOM workflows
- [ ] [`3d4e52fd`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/3d4e52fd9631) 2026-11-12: Release IsiX DICOM Viewer 6.0.0 with modular Swift architecture and plugin compatibility
- [ ] [`09811fc4`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/09811fc4d3e0) 2026-11-13: Improve DICOM data integrity, plugin recovery, reporting, and viewer workflows

<details><summary>Files</summary>

| File | Change | Lines |
|---|---|---|
| `Horos/Sources/DICOMExport.mm` | deleted | +0 −1382 |
| [`Horos/Sources/ViewerImageExporter.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ViewerImageExporter.swift) | new | +1154 −0 |
| [`Horos/Sources/DICOMFolderExport.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DICOMFolderExport.swift) | new | +660 −0 |
| [`Horos/Sources/SelectionMovieExport.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/SelectionMovieExport.swift) | new | +575 −0 |
| [`Horos/Sources/DICOMExport.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DICOMExport.swift) | new | +458 −0 |
| `Horos/Sources/HorosGDCMAnonymizer.mm` | deleted | +0 −457 |
| [`Horos/Sources/ViewerMovieExport.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ViewerMovieExport.swift) | new | +454 −0 |
| [`Horos/Sources/AnonymizedImport.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/AnonymizedImport.swift) | new | +368 −0 |
| [`Horos/Sources/DICOMExport+Dataset.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DICOMExport+Dataset.swift) | changed | +236 −8 |
| [`Horos/Sources/HorosGDCMAnonymizer.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/HorosGDCMAnonymizer.swift) | new | +207 −0 |
| [`Modules/IsiXImaging/Sources/ViewerExport/ViewerExportRange.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/ViewerExport/ViewerExportRange.swift) | new | +183 −0 |
| [`Horos/Sources/AnonymizationViewController.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/AnonymizationViewController.swift) | changed | +78 −72 |
| [`Horos/Sources/Anonymization.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/Anonymization.swift) | changed | +56 −53 |
| [`Horos/Sources/QuicktimeExport.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/QuicktimeExport.swift) | changed | +48 −42 |
| [`Modules/IsiXImaging/Tests/ViewerExport/ViewerExportRangeTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Tests/ViewerExport/ViewerExportRangeTests.swift) | new | +81 −0 |
| [`Horos/Sources/BatchExportManifest.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/BatchExportManifest.swift) | changed | +15 −64 |
| [`Horos/Sources/StudyMetadataExport.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/StudyMetadataExport.swift) | changed | +11 −68 |
| `Horos/Sources/AnonymizationTagsPopUpButton+CAPI.mm` | deleted | +0 −73 |
| [`Horos/Sources/DICOMExport.h`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DICOMExport.h) | changed | +5 −59 |
| [`Horos/Sources/AnonymizationTagsPopUpButton.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/AnonymizationTagsPopUpButton.swift) | changed | +30 −30 |
| [`Modules/IsiXPreferences/Sources/ExportFolderFields.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXPreferences/Sources/ExportFolderFields.swift) | new | +54 −0 |
| `Horos/Sources/QuicktimeExport+CAPI.m` | deleted | +0 −51 |
| `Horos/Sources/AnonymizationViewController+CAPI.mm` | deleted | +0 −50 |
| [`Horos/Sources/ExportFolderOptions.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ExportFolderOptions.swift) | changed | +14 −11 |
| [`Horos/Sources/ExportFolderNaming.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ExportFolderNaming.swift) | changed | +24 −0 |
| [`Horos/Sources/QTExportHTMLSummary.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/QTExportHTMLSummary.swift) | changed | +11 −6 |
| [`Horos/Sources/AnonymizationTagsView.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/AnonymizationTagsView.swift) | changed | +8 −7 |
| [`Horos/Sources/AnonymizationTagsPopUpButton.h`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/AnonymizationTagsPopUpButton.h) | changed | +0 −14 |
| [`Horos/Sources/RegisteredGIFExport.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/RegisteredGIFExport.swift) | changed | +12 −1 |
| [`Horos/Sources/AnonymizationErrorPresenter.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/AnonymizationErrorPresenter.swift) | changed | +4 −5 |
| [`Horos/Sources/AnonymizationCustomTagPanelController.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/AnonymizationCustomTagPanelController.swift) | changed | +3 −2 |
| [`Horos/Sources/HorosGDCMAnonymizer.h`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/HorosGDCMAnonymizer.h) | changed | +3 −2 |
| [`Horos/Sources/QuicktimeExport.h`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/QuicktimeExport.h) | changed | +0 −5 |

</details>

#### App shell: startup, menus, activity/progress, logging, updates, keychain
`app-shell` · P2 · next · Rebuild · RDicom: src/main.rs · 77 files, +6915 −7768

- [ ] [`5a8d5161`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/5a8d516115ff) 2026-10-16: Improve imaging workflows, viewer controls, and development tooling
- [ ] [`dc48a2f9`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/dc48a2f9d3ac) 2026-10-18: Improve imaging workflows, viewer reliability, reporting, and tooling
- [ ] [`fd2563a4`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/fd2563a49be7) 2026-10-18: Customize ROI statistics and fix imaging memory leaks
- [ ] [`c0b657f2`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/c0b657f29fa2) 2026-10-19: Refine viewer annotations, study comparisons, responsiveness, and packaging
- [ ] [`1c463f14`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/1c463f14f5f1) 2026-10-20: Expand imaging tools and streamline ROI management, DICOM retrieval, and viewer performance
- [ ] [`57982101`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/579821015297) 2026-10-24: Support macOS 15 and refine rendering, viewer interaction, and DICOM workflows
- [ ] [`64058d4e`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/64058d4e4fb2) 2026-10-31: Fix missing portal contexts during first-use setup
- [ ] [`3d4e52fd`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/3d4e52fd9631) 2026-11-12: Release IsiX DICOM Viewer 6.0.0 with modular Swift architecture and plugin compatibility
- [ ] [`cac52277`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/cac5227721c7) 2026-11-12: Update DICOM-Swift, improve archive handling and image display, and showcase supported platforms
- [ ] [`09811fc4`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/09811fc4d3e0) 2026-11-13: Improve DICOM data integrity, plugin recovery, reporting, and viewer workflows
- [ ] [`aceba6ac`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/aceba6ac9898) 2026-11-13: Add live ROI length ratios, improve contour editing and Word reports, and refine temporary preferences and SDK packaging
- [ ] [`087f2cd0`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/087f2cd0f721) 2026-11-16: Expand vessel analysis, improve DICOM and PACS workflows, and refine plugins and localization for 6.0.4

<details><summary>Files</summary>

| File | Change | Lines |
|---|---|---|
| [`Horos/Sources/AppController.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/AppController.swift) | changed | +484 −4733 |
| `Horos/FeedbackReporter/FRFeedbackController.m` | deleted | +145 −1086 |
| [`Horos/Sources/ApplicationLifecycle.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ApplicationLifecycle.swift) | changed | +756 −5 |
| [`Horos/Sources/ApplicationMenus.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ApplicationMenus.swift) | changed | +405 −6 |
| `Horos/FeedbackReporter/FRCrashLogFinder.m` | deleted | +0 −270 |
| [`Horos/Sources/ProcessGraph.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ProcessGraph.swift) | new | +264 −0 |
| [`Modules/IsiXContracts/Sources/DICOMIngesting.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXContracts/Sources/DICOMIngesting.swift) | new | +256 −0 |
| `Horos/FeedbackReporter/FRUploader.m` | deleted | +0 −231 |
| [`Modules/IsiXCrashReporting/Sources/CrashReportDraft.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXCrashReporting/Sources/CrashReportDraft.swift) | new | +231 −0 |
| [`Horos/Sources/AppController.h`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/AppController.h) | changed | +5 −225 |
| [`Modules/IsiXContracts/Tests/PatientNameQueryFormsTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXContracts/Tests/PatientNameQueryFormsTests.swift) | new | +209 −0 |
| [`Horos/Sources/UpdateCheckScheduler.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/UpdateCheckScheduler.swift) | new | +208 −0 |
| [`Horos/Sources/CrashReportWindowController.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/CrashReportWindowController.swift) | new | +191 −0 |
| [`Horos/Sources/AppController+CAPI.m`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/AppController+CAPI.m) | changed | +6 −180 |
| [`Modules/IsiXCrashReporting/Tests/CrashReportTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXCrashReporting/Tests/CrashReportTests.swift) | new | +176 −0 |
| [`Modules/IsiXContracts/Sources/PatientNameQueryForms.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXContracts/Sources/PatientNameQueryForms.swift) | new | +176 −0 |
| [`Modules/IsiXContracts/Sources/LocalMetadataQuerying.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXContracts/Sources/LocalMetadataQuerying.swift) | new | +174 −0 |
| [`Horos/Sources/LogManager.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/LogManager.swift) | changed | +79 −91 |
| [`Modules/IsiXInstallation/Tests/ReceivingAndNotificationTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXInstallation/Tests/ReceivingAndNotificationTests.swift) | new | +152 −0 |
| [`Horos/Sources/ThreadCell.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ThreadCell.swift) | changed | +76 −73 |
| `Horos/Sources/AppControllerDCMTKCategory.mm` | deleted | +0 −142 |
| [`Modules/IsiXKeychain/Sources/WebServerIdentity.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXKeychain/Sources/WebServerIdentity.swift) | new | +139 −0 |
| [`Modules/IsiXKeychain/Sources/KeychainIdentities.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXKeychain/Sources/KeychainIdentities.swift) | new | +134 −0 |
| [`Modules/IsiXCrashReporting/Sources/CrashReportFinder.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXCrashReporting/Sources/CrashReportFinder.swift) | changed | +129 −1 |
| [`Horos/Sources/UserNotificationDelivery.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/UserNotificationDelivery.swift) | new | +126 −0 |
| [`Modules/IsiXInstallation/Sources/NotificationBurstPolicy.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXInstallation/Sources/NotificationBurstPolicy.swift) | new | +121 −0 |
| [`Modules/IsiXLogging/Sources/LogText.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXLogging/Sources/LogText.swift) | new | +120 −0 |
| [`Modules/IsiXLogging/Tests/LogTextTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXLogging/Tests/LogTextTests.swift) | new | +120 −0 |
| [`Horos/Sources/ActivityThread.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ActivityThread.swift) | new | +118 −0 |
| [`Modules/IsiXKeychain/Sources/KeychainExport.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXKeychain/Sources/KeychainExport.swift) | new | +113 −0 |
| [`Modules/IsiXInstallation/Sources/ReceivingThreadCounter.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXInstallation/Sources/ReceivingThreadCounter.swift) | new | +110 −0 |
| `Horos/Sources/SplashScreen+CAPI.m` | deleted | +0 −100 |
| [`Horos/Sources/AppController+DCMTKCodecs.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/AppController+DCMTKCodecs.swift) | new | +99 −0 |
| [`Modules/IsiXInstallation/Sources/UpdateCheckSchedule.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXInstallation/Sources/UpdateCheckSchedule.swift) | new | +90 −0 |
| [`Modules/IsiXInstallation/Sources/ApplicationLocation.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXInstallation/Sources/ApplicationLocation.swift) | new | +87 −0 |
| [`Modules/IsiXContracts/Tests/LocalMetadataQueryingTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXContracts/Tests/LocalMetadataQueryingTests.swift) | new | +85 −0 |
| [`Modules/IsiXInstallation/Tests/InstallationRulesTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXInstallation/Tests/InstallationRulesTests.swift) | new | +84 −0 |
| `Horos/Sources/main.m` | deleted | +0 −81 |
| [`Modules/IsiXInstallation/Sources/DockTile.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXInstallation/Sources/DockTile.swift) | new | +81 −0 |
| [`Horos/Sources/ApplicationNetworkNotices.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ApplicationNetworkNotices.swift) | changed | +75 −4 |
| … and 37 more | | |

</details>

#### MPR: orthogonal and oblique reslicing, thick slab MIP/MinIP
`mpr` · P2 · next · Rebuild · RDicom: src/volume.rs, src/app.rs · 51 files, +4526 −1768

- [ ] [`57982101`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/579821015297) 2026-10-24: Support macOS 15 and refine rendering, viewer interaction, and DICOM workflows
- [ ] [`6e02be4b`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/6e02be4b99ea) 2026-10-24: Exponential 2D slab slider; Numbers hides colors
- [ ] [`aad6c4bf`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/aad6c4bf0afc) 2026-10-26: Refine precise dragging and thickness controls, series identification, DICOM listener compatibility and stabilize retrieve inventories
- [ ] [`3d4e52fd`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/3d4e52fd9631) 2026-11-12: Release IsiX DICOM Viewer 6.0.0 with modular Swift architecture and plugin compatibility
- [ ] [`09811fc4`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/09811fc4d3e0) 2026-11-13: Improve DICOM data integrity, plugin recovery, reporting, and viewer workflows
- [ ] [`087f2cd0`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/087f2cd0f721) 2026-11-16: Expand vessel analysis, improve DICOM and PACS workflows, and refine plugins and localization for 6.0.4

<details><summary>Files</summary>

| File | Change | Lines |
|---|---|---|
| [`Horos/Sources/MPRController.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/MPRController.swift) | changed | +378 −321 |
| `Horos/Sources/MPRHostBridge.m` | deleted | +9 −491 |
| [`Horos/Sources/MPRDCMView+MetalPlane.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/MPRDCMView+MetalPlane.swift) | changed | +312 −0 |
| [`Horos/Sources/MPRDCMView.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/MPRDCMView.swift) | changed | +118 −176 |
| `Horos/Sources/ThickSlabSliderScale.swift` | deleted | +139 −139 |
| [`Horos/Sources/MPRController+MetalHost.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/MPRController+MetalHost.swift) | new | +272 −0 |
| [`Horos/Sources/HorosThickSlabSlider.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/HorosThickSlabSlider.swift) | changed | +246 −25 |
| [`Modules/IsiXImaging/Sources/ViewerReconstruction/VolumeEligibility.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/ViewerReconstruction/VolumeEligibility.swift) | new | +218 −0 |
| [`Horos/Sources/OrthogonalMPRViewer.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/OrthogonalMPRViewer.swift) | changed | +90 −123 |
| [`Modules/IsiXImaging/Sources/ViewerReconstruction/VolumeRenderingOpening.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/ViewerReconstruction/VolumeRenderingOpening.swift) | new | +161 −0 |
| [`Horos/Sources/OrthogonalMPRView.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/OrthogonalMPRView.swift) | changed | +55 −92 |
| [`Horos/Sources/MPRZoomSynchronizer.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/MPRZoomSynchronizer.swift) | new | +142 −0 |
| [`Modules/IsiXImaging/Sources/ViewerReconstruction/CurvedPathSet.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/ViewerReconstruction/CurvedPathSet.swift) | new | +142 −0 |
| [`Modules/IsiXImaging/Sources/ViewerReconstruction/StenosisPlaque.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/ViewerReconstruction/StenosisPlaque.swift) | new | +140 −0 |
| [`Modules/IsiXImaging/Tests/ViewerReconstruction/ThickSlabSliderScaleTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Tests/ViewerReconstruction/ThickSlabSliderScaleTests.swift) | new | +125 −0 |
| [`Modules/IsiXImaging/Sources/ViewerReconstruction/ThickSlabSliderScale.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/ViewerReconstruction/ThickSlabSliderScale.swift) | new | +122 −0 |
| [`Modules/IsiXImaging/Tests/ViewerReconstruction/VolumeEligibilityTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Tests/ViewerReconstruction/VolumeEligibilityTests.swift) | new | +121 −0 |
| [`Modules/IsiXImaging/Sources/ViewerReconstruction/ReconstructionGeometryScan.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/ViewerReconstruction/ReconstructionGeometryScan.swift) | new | +116 −0 |
| [`Modules/IsiXImaging/Tests/ViewerReconstruction/CurvedPathSetTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Tests/ViewerReconstruction/CurvedPathSetTests.swift) | new | +114 −0 |
| [`Modules/IsiXImaging/Tests/ViewerReconstruction/VolumeRenderingOpeningTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Tests/ViewerReconstruction/VolumeRenderingOpeningTests.swift) | new | +104 −0 |
| `Horos/Sources/MPRController+CAPI.m` | deleted | +0 −101 |
| [`Horos/Sources/ReconstructionPlanePreferences.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ReconstructionPlanePreferences.swift) | changed | +100 −0 |
| [`Modules/IsiXImaging/Tests/ViewerReconstruction/ViewerReconstructionTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Tests/ViewerReconstruction/ViewerReconstructionTests.swift) | new | +97 −0 |
| [`Horos/Sources/MPRMetalReslicer.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/MPRMetalReslicer.swift) | changed | +73 −22 |
| [`Horos/Sources/MPRPlaneState.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/MPRPlaneState.swift) | new | +92 −0 |
| [`Horos/Sources/MPRToolbarItemViews.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/MPRToolbarItemViews.swift) | changed | +92 −0 |
| [`Modules/IsiXImaging/Sources/ViewerReconstruction/VesselTube.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/ViewerReconstruction/VesselTube.swift) | new | +85 −0 |
| `Horos/Sources/OrthogonalMPRViewer+CAPI.m` | deleted | +0 −82 |
| [`Horos/Sources/OrthogonalMPRController.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/OrthogonalMPRController.swift) | changed | +31 −50 |
| [`Modules/IsiXImaging/Tests/ViewerReconstruction/VolumeRevertSourceTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Tests/ViewerReconstruction/VolumeRevertSourceTests.swift) | new | +73 −0 |
| [`Modules/IsiXImaging/Sources/ViewerReconstruction/ThreeDViewerPresets.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/ViewerReconstruction/ThreeDViewerPresets.swift) | new | +71 −0 |
| [`Modules/IsiXImaging/Sources/ViewerReconstruction/MPRPlaneRules.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/ViewerReconstruction/MPRPlaneRules.swift) | new | +66 −0 |
| [`Modules/IsiXImaging/Sources/ViewerReconstruction/EndoscopyNavigation.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/ViewerReconstruction/EndoscopyNavigation.swift) | new | +65 −0 |
| [`Modules/IsiXImaging/Sources/ViewerReconstruction/VolumeRevertSource.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/ViewerReconstruction/VolumeRevertSource.swift) | new | +63 −0 |
| [`Horos/Sources/OrthogonalReslice.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/OrthogonalReslice.swift) | changed | +27 −34 |
| [`Modules/IsiXImaging/Tests/ViewerReconstruction/EndoscopyNavigationTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Tests/ViewerReconstruction/EndoscopyNavigationTests.swift) | new | +58 −0 |
| [`Modules/IsiXImaging/Tests/ViewerReconstruction/MPRZoomSyncTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Tests/ViewerReconstruction/MPRZoomSyncTests.swift) | new | +55 −0 |
| [`Horos/Sources/MPRHostMessages.h`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/MPRHostMessages.h) | changed | +37 −17 |
| [`Modules/IsiXImaging/Sources/ViewerReconstruction/MPRZoomSync.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/ViewerReconstruction/MPRZoomSync.swift) | new | +54 −0 |
| [`Modules/IsiXImaging/Sources/ViewerReconstruction/ThreeDViewerScreen.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXImaging/Sources/ViewerReconstruction/ThreeDViewerScreen.swift) | new | +53 −0 |
| … and 11 more | | |

</details>

#### Preferences, presets, shortcuts
`preferences` · P2 · next · Rebuild · RDicom: src/app.rs (PRESETS); src/settings.rs (planned) · 132 files, +12082 −5195

- [ ] [`5a8d5161`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/5a8d516115ff) 2026-10-16: Improve imaging workflows, viewer controls, and development tooling
- [ ] [`dc48a2f9`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/dc48a2f9d3ac) 2026-10-18: Improve imaging workflows, viewer reliability, reporting, and tooling
- [ ] [`fd2563a4`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/fd2563a49be7) 2026-10-18: Customize ROI statistics and fix imaging memory leaks
- [ ] [`c0b657f2`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/c0b657f29fa2) 2026-10-19: Refine viewer annotations, study comparisons, responsiveness, and packaging
- [ ] [`1c463f14`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/1c463f14f5f1) 2026-10-20: Expand imaging tools and streamline ROI management, DICOM retrieval, and viewer performance
- [ ] [`57982101`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/579821015297) 2026-10-24: Support macOS 15 and refine rendering, viewer interaction, and DICOM workflows
- [ ] [`6e02be4b`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/6e02be4b99ea) 2026-10-24: Exponential 2D slab slider; Numbers hides colors
- [ ] [`3d4e52fd`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/3d4e52fd9631) 2026-11-12: Release IsiX DICOM Viewer 6.0.0 with modular Swift architecture and plugin compatibility
- [ ] [`09811fc4`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/09811fc4d3e0) 2026-11-13: Improve DICOM data integrity, plugin recovery, reporting, and viewer workflows
- [ ] [`aceba6ac`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/aceba6ac9898) 2026-11-13: Add live ROI length ratios, improve contour editing and Word reports, and refine temporary preferences and SDK packaging
- [ ] [`087f2cd0`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/087f2cd0f721) 2026-11-16: Expand vessel analysis, improve DICOM and PACS workflows, and refine plugins and localization for 6.0.4

<details><summary>Files</summary>

| File | Change | Lines |
|---|---|---|
| `Horos/Sources/DefaultsOsiriX.m` | deleted | +17 −1381 |
| [`Preference Panes/OSIViewerPreferencePane/ja-JP.lproj/OSIViewerPreferencePanePref.xib`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Preference%20Panes/OSIViewerPreferencePane/ja-JP.lproj/OSIViewerPreferencePanePref.xib) | changed | +764 −553 |
| [`Preference Panes/OSIViewerPreferencePane/Base.lproj/OSIViewerPreferencePanePref.xib`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Preference%20Panes/OSIViewerPreferencePane/Base.lproj/OSIViewerPreferencePanePref.xib) | changed | +691 −480 |
| [`Modules/IsiXPreferences/Sources/ViewerPreferences.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXPreferences/Sources/ViewerPreferences.swift) | changed | +587 −2 |
| [`Preference Panes/PaneCatalogTranslation.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Preference%20Panes/PaneCatalogTranslation.swift) | new | +502 −0 |
| [`Preference Panes/OSIViewerPreferencePane/OSIViewerPreferencePanePref.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Preference%20Panes/OSIViewerPreferencePane/OSIViewerPreferencePanePref.swift) | changed | +299 −162 |
| [`Preference Panes/OSIListenerPreferencePane/ja-JP.lproj/OSIListenerPreferencePanePref.xib`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Preference%20Panes/OSIListenerPreferencePane/ja-JP.lproj/OSIListenerPreferencePanePref.xib) | changed | +211 −225 |
| [`Modules/IsiXPreferences/Tests/TemporaryPreferenceHoldsTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXPreferences/Tests/TemporaryPreferenceHoldsTests.swift) | changed | +369 −6 |
| [`Modules/IsiXPreferences/Tests/TemporaryPreferenceTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXPreferences/Tests/TemporaryPreferenceTests.swift) | changed | +331 −31 |
| [`Preference Panes/OSIGeneralPreferencePane/ja-JP.lproj/OSIGeneralPreferencePanePref.xib`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Preference%20Panes/OSIGeneralPreferencePane/ja-JP.lproj/OSIGeneralPreferencePanePref.xib) | changed | +94 −263 |
| [`Preference Panes/OSIDatabasePreferencePane/ja-JP.lproj/OSIDatabasePreferencePanePref.xib`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Preference%20Panes/OSIDatabasePreferencePane/ja-JP.lproj/OSIDatabasePreferencePanePref.xib) | changed | +173 −183 |
| [`Modules/IsiXPreferences/Tests/LaunchPreferenceMigrationsTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXPreferences/Tests/LaunchPreferenceMigrationsTests.swift) | changed | +332 −8 |
| [`Modules/IsiXPreferences/Sources/ImagePresets.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXPreferences/Sources/ImagePresets.swift) | changed | +336 −0 |
| [`Modules/IsiXPreferences/Sources/LaunchPreferenceMigrations.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXPreferences/Sources/LaunchPreferenceMigrations.swift) | changed | +315 −3 |
| [`Preference Panes/OSIGeneralPreferencePane/Base.lproj/OSIGeneralPreferencePanePref.xib`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Preference%20Panes/OSIGeneralPreferencePane/Base.lproj/OSIGeneralPreferencePanePref.xib) | changed | +62 −231 |
| [`Horos/Sources/PreferencesWindowController.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/PreferencesWindowController.swift) | changed | +243 −43 |
| [`Preference Panes/OSIListenerPreferencePane/Base.lproj/OSIListenerPreferencePanePref.xib`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Preference%20Panes/OSIListenerPreferencePane/Base.lproj/OSIListenerPreferencePanePref.xib) | changed | +128 −142 |
| [`Preference Panes/OSIWebSharingPreferencePane/ja-JP.lproj/OSIWebSharingPreferencePanePref.xib`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Preference%20Panes/OSIWebSharingPreferencePane/ja-JP.lproj/OSIWebSharingPreferencePanePref.xib) | changed | +115 −151 |
| [`Modules/IsiXPreferences/Sources/TemporaryPreferenceHolds.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXPreferences/Sources/TemporaryPreferenceHolds.swift) | changed | +263 −3 |
| [`Modules/IsiXPreferences/Sources/VolumeRenderingPreferences.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXPreferences/Sources/VolumeRenderingPreferences.swift) | new | +237 −0 |
| [`Horos/Sources/PreferenceChangeResponder.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/PreferenceChangeResponder.swift) | changed | +233 −0 |
| [`Modules/IsiXPreferences/Sources/PreferenceChange.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXPreferences/Sources/PreferenceChange.swift) | changed | +208 −2 |
| [`Modules/IsiXPreferences/Sources/PreferenceValue.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXPreferences/Sources/PreferenceValue.swift) | new | +209 −0 |
| [`Modules/IsiXPreferences/Sources/ReconstructionPreferences.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXPreferences/Sources/ReconstructionPreferences.swift) | changed | +198 −2 |
| [`Modules/IsiXPreferences/Tests/ApplicationPreferencesTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXPreferences/Tests/ApplicationPreferencesTests.swift) | new | +198 −0 |
| [`Horos/Sources/ApplicationPreferencesSetup.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ApplicationPreferencesSetup.swift) | changed | +184 −4 |
| [`Preference Panes/OSIAutoroutingPreferencePane/ja-JP.lproj/OSIAutoroutingPreferencePanePref.xib`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Preference%20Panes/OSIAutoroutingPreferencePane/ja-JP.lproj/OSIAutoroutingPreferencePanePref.xib) | changed | +81 −102 |
| [`Modules/IsiXPreferences/Sources/PropertyListValue.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXPreferences/Sources/PropertyListValue.swift) | new | +178 −0 |
| [`Preference Panes/AYDicomPrintPreferencePane/ja-JP.lproj/AYDicomPrintPref.xib`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Preference%20Panes/AYDicomPrintPreferencePane/ja-JP.lproj/AYDicomPrintPref.xib) | changed | +89 −89 |
| [`Modules/IsiXPreferences/Tests/ReconstructionPreferencesTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXPreferences/Tests/ReconstructionPreferencesTests.swift) | new | +163 −0 |
| [`Modules/IsiXPreferences/Tests/ViewerSettingsTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXPreferences/Tests/ViewerSettingsTests.swift) | changed | +163 −0 |
| [`Modules/IsiXPreferences/Tests/ForcedPreferenceTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXPreferences/Tests/ForcedPreferenceTests.swift) | new | +157 −0 |
| [`Modules/IsiXPreferences/Tests/PreferenceCatalogTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXPreferences/Tests/PreferenceCatalogTests.swift) | new | +153 −0 |
| [`Modules/IsiXPreferences/Sources/DefaultPreferences+ImagePresets.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXPreferences/Sources/DefaultPreferences+ImagePresets.swift) | new | +151 −0 |
| [`Preference Panes/OSIPACSOnDemandPreferencePane/ja-JP.lproj/OSIPACSOnDemand.xib`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Preference%20Panes/OSIPACSOnDemandPreferencePane/ja-JP.lproj/OSIPACSOnDemand.xib) | changed | +59 −92 |
| [`Modules/IsiXPreferences/Tests/VolumeRenderingPreferencesTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXPreferences/Tests/VolumeRenderingPreferencesTests.swift) | new | +149 −0 |
| [`Modules/IsiXPreferences/Tests/ImagePresetsTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXPreferences/Tests/ImagePresetsTests.swift) | changed | +147 −0 |
| [`Modules/IsiXPreferences/Sources/DefaultPreferences+Lists.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXPreferences/Sources/DefaultPreferences+Lists.swift) | new | +143 −0 |
| [`Modules/IsiXPreferences/Sources/ApplicationPreferences.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXPreferences/Sources/ApplicationPreferences.swift) | new | +141 −0 |
| [`Modules/IsiXPreferences/Sources/TemporaryPreference.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXPreferences/Sources/TemporaryPreference.swift) | changed | +105 −36 |
| … and 92 more | | |

</details>

#### Segmentation, registration, subtraction, SEG objects
`segmentation-registration` · P3 · later · Port · RDicom: src/volume.rs (RigidTransform3D), src/app.rs · 38 files, +4465 −3930

- [ ] [`3d4e52fd`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/3d4e52fd9631) 2026-11-12: Release IsiX DICOM Viewer 6.0.0 with modular Swift architecture and plugin compatibility
- [ ] [`09811fc4`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/09811fc4d3e0) 2026-11-13: Improve DICOM data integrity, plugin recovery, reporting, and viewer workflows
- [ ] [`087f2cd0`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/commit/087f2cd0f721) 2026-11-16: Expand vessel analysis, improve DICOM and PACS workflows, and refine plugins and localization for 6.0.4

<details><summary>Files</summary>

| File | Change | Lines |
|---|---|---|
| `Horos/Sources/ITKSegmentation3D.mm` | deleted | +0 −1192 |
| `Horos/Sources/ITKSegmentation3DController.m` | deleted | +0 −632 |
| [`Horos/Sources/ViewerRegistrationSession.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ViewerRegistrationSession.swift) | new | +563 −0 |
| [`Horos/Sources/ViewerSEGSurface.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ViewerSEGSurface.swift) | changed | +467 −2 |
| `Horos/Sources/ITKTransform.mm` | deleted | +0 −385 |
| `Horos/Sources/ViewerSEGSurface.mm` | deleted | +0 −339 |
| `Horos/Sources/HornRegistration.mm` | deleted | +0 −334 |
| `Horos/Sources/ITKBrushROIFilter.mm` | deleted | +0 −310 |
| [`Horos/Sources/MaskSubtractionSession.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/MaskSubtractionSession.swift) | new | +253 −0 |
| [`Horos/Sources/ITKSegmentation3DController+RegionGrowing.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ITKSegmentation3DController+RegionGrowing.swift) | new | +244 −0 |
| [`Horos/Sources/DicomSEG.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/DicomSEG.swift) | changed | +198 −42 |
| [`Horos/Sources/ITKSegmentation3D+Results.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ITKSegmentation3D+Results.swift) | new | +238 −0 |
| [`Horos/Sources/ITKSegmentation3DController.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ITKSegmentation3DController.swift) | new | +213 −0 |
| `Horos/Sources/ITK.mm` | deleted | +0 −206 |
| [`Horos/Sources/LongitudinalRegistration.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/LongitudinalRegistration.swift) | changed | +186 −18 |
| [`Horos/Sources/ITKTransform.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ITKTransform.swift) | new | +194 −0 |
| [`Horos/Sources/ITKSegmentation3DController+Algorithms.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ITKSegmentation3DController+Algorithms.swift) | new | +191 −0 |
| [`Horos/Sources/ITKTransform+Viewer.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ITKTransform+Viewer.swift) | changed | +170 −10 |
| [`Modules/IsiXSegmentation/Sources/RegionGrowingParameters.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXSegmentation/Sources/RegionGrowingParameters.swift) | new | +172 −0 |
| [`Horos/Sources/ITKSegmentation3D+RegionGrowing.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ITKSegmentation3D+RegionGrowing.swift) | new | +145 −0 |
| [`Modules/IsiXSegmentation/Sources/RegionGrowingFilter.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXSegmentation/Sources/RegionGrowingFilter.swift) | new | +143 −0 |
| [`Modules/IsiXSegmentation/Sources/VolumeImportGeometry.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXSegmentation/Sources/VolumeImportGeometry.swift) | new | +138 −0 |
| [`Horos/Sources/ITKBrushROIFilter.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ITKBrushROIFilter.swift) | new | +127 −0 |
| [`Horos/Sources/ITKSegmentation3D.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ITKSegmentation3D.swift) | new | +122 −0 |
| [`Modules/IsiXSegmentation/Sources/ContourTracing.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXSegmentation/Sources/ContourTracing.swift) | new | +118 −0 |
| [`Modules/IsiXSegmentation/Tests/ITKRulesTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXSegmentation/Tests/ITKRulesTests.swift) | new | +117 −0 |
| [`Horos/Sources/HornRegistration.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/HornRegistration.swift) | new | +107 −0 |
| [`Horos/Sources/SegmentationSeedPoint.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/SegmentationSeedPoint.swift) | new | +102 −0 |
| [`Modules/IsiXSegmentation/Sources/MorphologyKernel.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXSegmentation/Sources/MorphologyKernel.swift) | new | +93 −0 |
| `Horos/Sources/ITKSegmentation3DController.h` | deleted | +0 −91 |
| [`Modules/IsiXSegmentation/Sources/AffineResampleGrid.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXSegmentation/Sources/AffineResampleGrid.swift) | new | +91 −0 |
| `Horos/Sources/ITKSegmentation3D.h` | deleted | +0 −87 |
| `Horos/Sources/ITK.h` | deleted | +0 −82 |
| `Horos/Sources/ITKTransform.h` | deleted | +0 −79 |
| `Horos/Sources/HornRegistration.h` | deleted | +0 −60 |
| `Horos/Sources/ITKBrushROIFilter.h` | deleted | +0 −60 |
| [`Modules/IsiXSegmentation/Tests/RegionGrowingParametersTests.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Modules/IsiXSegmentation/Tests/RegionGrowingParametersTests.swift) | new | +60 −0 |
| [`Horos/Sources/SEGViewerSession.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/SEGViewerSession.swift) | changed | +13 −1 |

</details>

### 2. Noted: planned areas RDicom hasn't started

No action now. When an area is started, port from upstream's latest version, which includes these.

| Area | Priority | Files | Lines | Commits |
|---|---|---|---|---|
| Presentation states (GSPS), key images, structured reports (`annotations-sr`) | P2 · next | 10 | +881 −1070 | 2 |
| DICOMweb and WADO (QIDO/WADO/STOW, OAuth) (`dicomweb`) | P2 · next | 15 | +598 −697 | 4 |
| PET-CT fusion and SUV (`fusion-pet`) | P2 · next | 17 | +2135 −202 | 3 |
| PACS networking: echo, query/retrieve, send, listener, TLS, worklist (`network-pacs`) | P2 · next | 189 | +30507 −21088 | 9 |
| MR maps: T1/T2 fitting, synthetic DWI, b-value split (`advanced-mr`) | P3 · later | 11 | +2533 −112 | 1 |
| Curved MPR (CPR) and straightened views (`cpr`) | P3 · later | 30 | +1684 −2135 | 5 |
| Reports (Word/Pages/RTF templates linked to studies) (`reports`) | P3 · later | 12 | +2431 −559 | 4 |
| 3D: volume rendering, MIP/VR, surface rendering, 3D presets (`volume-3d`) | P3 · later | 123 | +29565 −28743 | 6 |
| Virtual endoscopy and fly-through (`endoscopy-flythru`) | P4 · maybe | 20 | +838 −3120 | 3 |

### 3. Ignored: not planned or not app code

Build system, docs, scripts, test harnesses, release tooling (`meta`): 1415 files, Translations, icons, help pages (`translations-icons`): 266 files, Bundled third-party code (OpenSSL, libarchive, VTK/ITK helpers, codecs) (`third-party`): 13 files, Swift/Objective-C bridges and adapters (`glue`): 192 files, Cocoa widgets and helpers (Nitrogen, custom cells, NS categories) (`ui-toolkit`): 143 files, OsiriX/Horos plugin system (`plugins`): 71 files, Web portal, HTTP/XML-RPC servers, shared databases, Bonjour sharing (`web-sharing`): 102 files, DICOM printing, CD/DVD burning, e-mail, Photos, iCloud (`print-burn-mail`): 39 files

### ⚠ New files that the porting map doesn't cover

Add a pattern for each to `porting/map.toml` (or check `coverage`) so future changes are sorted.

| File | Change | Lines |
|---|---|---|
| [`Horos/Sources/AnyLanguage.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/AnyLanguage.swift) | new | +39 −0 |
| [`Horos/Sources/CurvedMPRVesselTrace.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/CurvedMPRVesselTrace.swift) | new | +256 −0 |
| [`Horos/Sources/CurvedPathLibrary.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/CurvedPathLibrary.swift) | new | +310 −0 |
| [`Horos/Sources/ReconstructionCrossDrag.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ReconstructionCrossDrag.swift) | new | +48 −0 |
| [`Horos/Sources/ReconstructionUndoEntries.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ReconstructionUndoEntries.swift) | new | +42 −0 |
| [`Horos/Sources/SeriesRestoreCopies.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/SeriesRestoreCopies.swift) | new | +51 −0 |
| [`Horos/Sources/SliderValueBubble.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/SliderValueBubble.swift) | new | +84 −0 |
| [`Horos/Sources/StenosisCapture.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/StenosisCapture.swift) | new | +53 −0 |
| [`Horos/Sources/StenosisSummary.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/StenosisSummary.swift) | new | +194 −0 |
| [`Horos/Sources/ThickSlabSliderHost.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/ThickSlabSliderHost.swift) | new | +122 −0 |
| [`Horos/Sources/TipMatrix.swift`](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS/blob/087f2cd0f721/Horos/Sources/TipMatrix.swift) | new | +27 −0 |

35 other changed file(s) outside the map no longer exist upstream (added and removed again, or deleted); nothing to do.

### When you've reviewed this

1. Port what matters from section 1 (and tick it here).
2. Record the review: `python3 tools/upstream.py mark-reviewed isix 087f2cd0` and commit `porting/upstream.toml`.
