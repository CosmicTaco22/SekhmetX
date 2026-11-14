# SekhmetX

A fast, native radiological DICOM viewer for Windows, macOS, and Linux, written in Rust (`eframe`/`egui` + `wgpu` + `dicom-rs`). It is a from-scratch rewrite modeled directly on [IsiX DICOM Viewer](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS) (and Horos / OsiriX before it) for its geometry, pixel pipeline, ROI statistics, and radiological workflow.

> **Not a medical device.** For personal use, teaching, research, and second looks only; not certified for primary clinical diagnosis.

---

## Key Features

- **Direct Folder, File & Streaming `.zip` Archive Opening:**
  - Open folders recursively, individual DICOM files, or **`.zip` / ZIP64 patient archives** (streamed safely into a per-process `%TEMP%\sekhmetx-zip-<pid>` folder with path-traversal protection, size/entry caps, and automatic cleanup).
- **Horos-Style Responsive Toolbar (`»` Overflow) & Bilingual UI (`EN` / `ES`):**
  - Toolbar ordered to match IsiX / Horos (`ViewerToolbarCatalog`), automatically collapsing trailing groups into a **`»`** overflow menu on narrower windows so controls never wrap or clip.
  - One-click **English / Spanish (`🌐 EN` / `🌐 ES`)** language toggle persisted in the local SQLite database.
- **Horos Study Browser with Search, Local SQLite Library & Live Mini-Thumbnails:**
  - Crash-safe local SQLite database (WAL mode, schema v2 migrations) storing study/series metadata, persistent ROIs, and preferences. *(Note: patient metadata in the local SQLite database is stored unencrypted on disk.)*
  - Managed library storage (`SekhmetX/Studies/<StudyUID>/`) with configurable **`Ask` / `Always` / `Never`** copy policy when opening `.zip` archives, live library disk-usage reporting (MB), **`⚠ [files missing]`** badges if source files move, per-study **`Remove from library`**, and one-click **`Clear`** library reset.
  - Instant search bar (`🔍`) filtering by patient name, ID, accession, study, or series description, plus one-click modality filter pills (`All`, `CT`, `MR`, `US`, `DX/CR`, `PT/NM`).
  - Live rendered **38×38 DICOM series mini-thumbnails**, dominant anatomical plane badges (`[AX]`, `[COR]`, `[SAG]`, `[OBL]`), and **Acquisition-Time MPR Auto-Grouping** (`🧊 MPR [AX·COR·SAG]`).
- **Exact Native-Plane & In-Place Single-Viewport 3D MPR, Thick-Slab MIP/MinIP/Mean & Trackball Rotation:**
  - Native-plane `Thin` MPR returns bit-identical source slice voxels and source slice thickness (`Thk`).
  - **`3D Rotate` (`O`)** trackball orbit inside any single viewport, **`⊞ Ax·Cor·Sag`** 3-pane orthogonal MPR with interactive crosshairs, **Thick-Slab Projections (`Thin / MIP / MinIP / Mean`, `1.0–50.0 mm`)**, and opt-in **Blend Siblings** mode.
  - **3D Geometry Validation & Gantry Tilt Support:** Ported `SliceIntervalRegularity` validation, full 3×3 matrix inversion in `lps_to_voxel` for tilted gantries, unverified-geometry measurement guards, and pixel-unit (`px`) fallback when `PixelSpacing` is absent.
- **Multi-Series Comparison Layouts & Non-Blocking Cross-Study 3D Synchronization:**
  - Interactive **`4 × 4` visual square-grid layout picker (`⊞ Layout`)** (`1×1` up to `4×4` / 16 viewports).
  - Worker-thread 6-DOF **Normalized Cross-Correlation (NCC)** rigid registration with `FrameOfReferenceUID` identity fast path, alignment score / position-sync fallback banners, and manual **`⚓ Lock 3D Sync`** override.
- **Horos / IsiX Regions of Interest (`IsiXROI`) & Pixel Statistics:**
  - **`Length` (`L` / `M`)**, **`Angle` (`A`)**, **`◯ Oval` (`E`)**, **`▭ Rect` (`B`)**, **`⬠ Closed Polygon` (`G`)**, **`✏ Freehand Pencil` (`F`, with cubic spline smoothing)**, and **`⌖ Point Probe` (`T`)**.
  - Live calibrated **Area ($\text{cm}^2 / \text{mm}^2$)**, **Perimeter**, **Mean**, **SD**, **Min**, and **Max** (`HU`) reading unconvolved source pixels, with automatic per-slice persistence in SQLite (`rois` table).
- **GPU Window/Level with Bilinear Interpolation, 31 Horos CLUTs & 22 Horos 2D Convolution Filters:**
  - Hardware-accelerated WGSL shader pipeline (`wgpu`) with post-windowing **Bilinear Interpolation** (toggleable in View), CPU fallback (`RDICOM_CPU=1`), **31 Horos CLUTs**, and **22 Horos 2D Convolution Filters**.
- **In-App Auto-Updater:**
  - Checks GitHub Releases (`CosmicTaco22/SekhmetX`), displays release notes, verifies `SHA256SUMS` before replacing the binary in-place, and supports a startup-check toggle.

---

## Mouse and Keyboard Controls

| Action | Control |
|---|---|
| Previous / next slice | Mouse wheel, `↑` / `↓` / `←` / `→`, `Scroll` tool (`S`), toolbar slider, or bottom viewport scrubber |
| Jump 10 slices / first / last | `Page Up` / `Page Down`, `Home` / `End`, toolbar `⏮` / `⏭` |
| Previous / next series | `[` / `]`, or toolbar `◀ Se` / `Se ▶` |
| Active tool | Left click / drag |
| Pan | Middle drag, or `Pan` tool |
| Zoom | Right drag, `Ctrl` + wheel, or `Zoom` tool (`Z`) |
| In-plane 2D rotate / flip | `Rotate` tool, `↺ 90°`, `↻ 90°`, `H` (Flip H), `V` (Flip V) |
| In-place 3D MPR orbit | `3D Rotate` tool (`O`) — drag to orbit 3D plane, `Shift` + drag to roll |
| Reset view | Double-click, `R`, or toolbar `Reset` |
| Window presets | `1` CT Abdomen · `2` CT Bone · `3` CT Brain · `4` CT Pulmonary · `5` VR Endoscopy · `0` Default |
| Invert greyscale | `I` |
| ROI & measurement tools | `L`/`M` Length · `A` Angle · `E` Oval · `B` Rect · `G` Polygon (`Enter` to close) · `F` Pencil · `T` Point |
| Delete last ROI on slice | `Delete` or `Backspace` (`🗑 Clear ROIs` clears slice) |
| Cine play / pause | `P` or `Space` |

---

## Releases & Building

### Download from GitHub Releases
Every tagged release (`v*`) automatically builds **SekhmetX** (`sekhmetx`) for **Windows (`x86_64` & `arm64`)**, **macOS (`arm64` `.app` & `.tar.gz`)**, and **Linux (`x86_64`)** via GitHub Actions and attaches them to the [GitHub Releases](../../releases) page alongside the changelog notes from [CHANGELOG.md](CHANGELOG.md). You can also update directly inside the app via the toolbar **`🔄`** update button.

### Build from Source on Windows
1. Install Rust (<https://rustup.rs>) with the Visual Studio C++ Build Tools, plus CMake (`winget install Kitware.CMake` or `scoop install cmake`).
2. Build and run:
   ```powershell
   cargo test --release
   cargo build --release
   .\target\release\sekhmetx.exe
   ```

---

## Command-Line Helpers

```powershell
sekhmetx --list FOLDER_OR_ZIP             # print studies and series found
sekhmetx --check FOLDER_OR_ZIP            # decode every image and report failures
sekhmetx --render FOLDER OUT.png [SERIES] [IMAGE] [WIDTH LEVEL]
sekhmetx --roi FOLDER_OR_FILE [SERIES IMAGE] (ellipse|rect|length) X1 Y1 X2 Y2
```

---

## Code Layout

| Path | Role |
|---|---|
| `src/dicom_index.rs` | Folder & `.zip` scan, DICOM tag parsing, plane classification (`AX`/`COR`/`SAG`), MPR acquisition grouping |
| `src/frame.rs` | Pixel decoding, modality rescale, VOI LUT, 2D convolution application, series mini-thumbnails |
| `src/geometry/` | Ported `IsiXGeometry` & `IsiXImaging` 3D vectors, lines, planes, transforms, Bezier splines, orientation letters |
| `src/imaging/` | Ported `IsiXImaging` window/level, VOI LUT, defaults, photometry, 31 Horos CLUTs, 22 convolution kernels |
| `src/roi/` | Ported `IsiXROI` planar geometry, cubic splines, scanline rasterizer, Sutherland-Hodgman clipper, pixel statistics |
| `src/volume.rs` | 3D volume assembly, in-place oblique 3D MPR reslicing, thick-slab MIP/MinIP/Mean, 6-DOF 3D rigid registration |
| `src/navigation.rs` | Multi-series `ViewportTiling`, `SliceSynchronizer`, and `ReferenceLines` |
| `src/gpu_view.rs` | `wgpu` hardware-accelerated window/level, CLUT, and quad UV orientation renderer |
| `src/loader.rs` | Multi-threaded priority background slice decoder |
| `src/app.rs` | Horos-style dark UI: Study Browser, Toolbar, Multi-Viewport Grid, 3D MPR, ROIs, and HUD overlays |
| `CHANGELOG.md` | Release history and changelog |
| `PORTING_MAP.md` | Full IsiX / Horos module porting map and progress tracker |

## Licence

LGPL-3.0-or-later, matching IsiX and Horos.
