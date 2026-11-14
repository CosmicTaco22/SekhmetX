# SekhmetX UI Specification (v0.3.0 — Horos / IsiX Alignment)

This specification defines the target UI layout, toolbar order, responsive overflow behavior, and visual conventions for SekhmetX v0.3.0, grounded in IsiX (`ThalesMMS/IsiX-DICOM-Viewer-macOS`) and Horos.

## 1. 2D Viewer Toolbar (`ViewerToolbarCatalog.defaultIdentifiers`)

Following `Modules/IsiXImaging/Sources/ViewerMenus/ViewerToolbarCatalog.swift`, the primary toolbar presents groups in Horos order from left to right:

1. **Database & Open**: Study Browser toggle (`◧`) + Open Folder / Files / ZIP (`📂`).
2. **Tiling**: Visual square-grid viewport layout picker (`⊞ 1×1` up to `4×4`).
3. **Mouse Tools**: Window/Level (`W`), Stack Scroll (`S`), Pan (`Middle`), Zoom (`Z`), 2D Rotate, 3D Trackball Orbit (`O`).
4. **ROI & Measurement Tools**: Length (`L`), Oval (`E`), Rectangle (`B`), plus dropdown for Angle (`A`), Polygon (`G`), Pencil (`F`), Point (`T`), and Clear ROIs.
5. **Presentation (WL / CLUT / Convolution)**: W/L Preset selector (`0`–`9`, `~` Full), Horos CLUT selector (31 tables), 2D Spatial Convolution Filter selector (22 kernels).
6. **3D & View**: 3-Pane Orthogonal MPR (`⊞ Ax·Cor·Sag`), In-Place 3D MPR (`🧊 3D`), View menu (Invert, Rotate 90°, Flip H/V, Reference Lines, Annotations, Bilinear Interpolation, Slice Sync Mode, Reset).
7. **Cine**: Play/Pause (`Space`/`P`) + FPS speed popover (`1–60 fps`).
8. **App**: Language toggle (`EN`/`ES`) + In-App Updater (`🔄`).

### Responsive Overflow (`»`)
- The toolbar never wraps into multiple cluttered rows and never hides items behind an invisible horizontal scroll bar.
- It measures `ui.available_width()` and renders as many leading groups as fit, placing any remaining trailing groups inside a **`»`** overflow menu on the right.
- Target tested widths/scales: `1024×700`, `1440×900`, `2560×1440`, and `150%` DPI scaling.

## 2. Contextual Scrubber & MPR Strip (Row 2)
- Series step (`◀ Se`, `Se ▶`), Slice scrubber (`⏮`, `◀`, slider, `cur/total`, `▶`, `⏭`).
- When active viewport has 3D MPR enabled: `Ax`, `Cor`, `Sag`, `Nat`, `Slab` (`Thin` / `MIP` / `MinIP` / `Mean` + thickness mm), and `Blend Siblings` (if sibling series exist).
- When multiple viewports are visible: `⚓ Lock 3D Sync`.

## 3. Code Organization (`src/ui/`)
- `src/ui/mod.rs`: UI module exports.
- `src/ui/toolbar.rs`: Primary Horos-ordered toolbar with `»` overflow + contextual scrubber/MPR strip.
- Future v0.3.0 splits: `ui/browser.rs`, `ui/viewport.rs`, `ui/overlays.rs`, `ui/dialogs.rs`.
