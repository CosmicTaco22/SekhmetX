# SekhmetX

A fast, native radiological DICOM viewer for Windows, macOS, and Linux, written in Rust (`eframe`/`egui` + `wgpu` + `dicom-rs`). It is a from-scratch rewrite modeled directly on [IsiX DICOM Viewer](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS) (and Horos / OsiriX before it) for its geometry, pixel pipeline, ROI statistics, and radiological workflow.

> **Not a medical device.** For personal use, teaching, research, and second looks only; not certified for primary clinical diagnosis.

---

## Key Features (v0.2.1)

- **Direct Folder, File & `.zip` Archive Opening:**
  - Open folders recursively, individual DICOM files, or **`.zip` patient archives** (extracted non-destructively into an isolated `%TEMP%\rdicom-zip-cache` folder and cleaned up automatically when the app closes).
- **Horos Study Browser with Search & Live Mini-Thumbnails:**
  - Instant search bar (`🔍`) filtering by patient name, ID, accession, study, or series description.
  - One-click modality filter pills (`All`, `CT`, `MR`, `US`, `DX/CR`, `PT/NM`).
  - Live rendered **38×38 DICOM series mini-thumbnails** and dominant anatomical plane badges (`[AX]`, `[COR]`, `[SAG]`, `[OBL]`).
  - **Acquisition-Time MPR Auto-Grouping:** Automatically identifies separated Axial, Coronal, and Sagittal reconstructions from the same acquisition and groups them under **`🧊 MPR [AX·COR·SAG]`** with quick `AX` / `COR` / `SAG` launch buttons.
- **In-Place Single-Viewport 3D MPR, Thick-Slab MIP/MinIP/Mean & 3D Trackball Rotation:**
  - Enable **`🧊 3D MPR`** directly inside any single viewport square—no need to open a separate 3-panel window just to rotate or compare studies.
  - **`3D Rotate` (`O`)**: drag in any direction to orbit the 3D slicing plane through Axial, Coronal, Sagittal, or arbitrary 3D oblique angles; hold `Shift` while dragging to tilt/roll in-plane.
  - **Hybrid Multi-Series Tri-Planar Fusion:** Blends high-resolution in-plane data from sibling Axial, Coronal, and Sagittal stacks weighted by $\cos^4(\theta)$.
  - **Thick-Slab Projections (`Slab: Thin / MIP / MinIP / Mean`)** with a `1.0–50.0 mm` thickness slider.
- **Multi-Series Comparison Layouts & Cross-Study 6-DOF 3D Synchronization:**
  - Interactive **`4 × 4` visual square-grid layout picker (`⊞ Layout`)** (`1×1` up to `4×4` / 16 viewports).
  - **Computational 6-DOF 3D Rigid Registration (`RigidTransform3D`):** Automatically aligns two studies (e.g., longitudinal brain scans from different dates or scanners) using 3D foreground anatomical centroid matching + multi-scale Normalized Cross-Correlation (NCC), locking both viewports to the same anatomical height and 3D cut angle across Axial, Coronal, Sagittal, and Oblique 3D rotations.
  - Manual **`⚓ Lock 3D Sync`** button, 3D cross-reference scout cut lines (`Ref Lines`), and `A/P/R/L/S/I` orientation edge letters.
- **Horos / IsiX Regions of Interest (`IsiXROI`) & Pixel Statistics:**
  - **`Length` (`L` / `M`)**, **`Angle` (`A`)**, **`◯ Oval` (`E`)**, **`▭ Rect` (`B`)**, **`⬠ Closed Polygon` (`G`)**, **`✏ Freehand Pencil` (`F`, with cubic spline smoothing)**, and **`⌖ Point Probe` (`T`)**.
  - Live calibrated **Area ($\text{cm}^2 / \text{mm}^2$)**, **Perimeter**, **Mean**, **SD**, **Min**, and **Max** (`HU`) in Horos-style dark callout boxes.
- **GPU Window/Level, 31 Horos CLUTs & 22 Horos 2D Convolution Filters:**
  - Hardware-accelerated WGSL shader pipeline (`wgpu`) with CPU fallback (`RDICOM_CPU=1`).
  - All **31 factory Horos Colour Lookup Tables (`CLUT`)** and **22 factory Horos 2D Spatial Convolution Filters** (`Bone Filter 3x3`, `Sharpen 3x3`, `Sharpen 5x5`, `Basic Smooth 5x5`, `Gaussian blur`, `Edge 3x3`, `Laplacian`, etc.).

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
