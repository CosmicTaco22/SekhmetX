# SekhmetX Deviations from IsiX / Horos

This document records behaviors where SekhmetX intentionally extends, adapts, or differs from **IsiX** (`ThalesMMS/IsiX-DICOM-Viewer-macOS`) and **Horos**, who requested each behavior, and how it is surfaced on screen.

| Behavior | IsiX / Horos Baseline | SekhmetX Behavior | Requested By | On-Screen Indicator |
|---|---|---|---|---|
| **Single-Viewport In-Place 3D MPR (`3D Rotate`)** | 3D MPR opens in a dedicated 3-pane or MPR window | Slice any 3D volume in-place inside any comparison viewport tile via trackball orbit (`O`) + `Ax`/`Cor`/`Sag`/`Nat` buttons | Owner (cross-study comparison workflow) | `🧊 3D` toggle, `AXIAL`/`CORONAL`/`SAGITTAL`/`OBLIQUE` badge |
| **Acquisition-Time AX/COR/SAG Auto-Grouping** | Separate series stay listed independently | Orthogonal series from the same acquisition & kernel group under `🧊 MPR [AX·COR·SAG]` with quick `AX`/`COR`/`SAG` buttons | Owner | `🧊 MPR [AX·COR·SAG]` block in Study Browser |
| **Optional Multi-Series Sibling Blending** | Reslicing samples a single volume | Off by default; explicit `Blend Siblings` checkbox samples neighboring orthogonal series during oblique MPR | Owner / Review v0.2.2 (A1) | `"Blended from N series (ROI stats disabled)"` |
| **Non-Blocking Cross-Study 3D Registration** | Manual slice/position sync or manual registration | Worker-thread 6-DOF NCC alignment across comparison viewports, `FrameOfReferenceUID` identity fast path, and `⚓ Lock 3D Sync` manual override | Owner | `"Aligning…"`, `"Same Frame of Reference"`, `"Auto-aligned (approximate) · score 0.xx"`, or `"Not aligned: synced by position"` |
| **Force 3D on Non-Standard / RGB Series** | Refuses 3D MPR on <3 slices, RGB, or irregular stacks | Optional Force-3D confirmation dialog explaining the exact reason; RGB converted to luminance | Owner / Review v0.2.2 (A2) | `"Geometry not verified: measurements disabled"` banner + disabled measurements |
| **Study Browser Modality Filter Pills** | Search & smart albums | One-click `All`, `CT`, `MR`, `US`, `DX`, `PT` filter pills | Owner | Active pill highlighted in Study Browser |
| **Streaming `.zip` / ZIP64 Archive Opening** | Folders & DICOM files / DICOMDIR | Direct `.zip` / ZIP64 extraction into per-process temp dir with optional **"Copy into library"** prompt | Owner | `"Copy opened ZIP study into library?"` prompt in Study Browser |
| **In-App GitHub Auto-Updater** | Separate Sparkle / manual download | Built-in GitHub Releases checker + `SHA256SUMS` verifier + `self_replace` updater | Owner | `🔄` / `⬆ vX.Y.Z` toolbar button & Updates dialog |
| **Bilingual EN / ES Toggle** | macOS `.lproj` system locale | Language chosen in Settings and on first run, remembered | Owner (Spanish-speaking colleagues) | Settings → Language |
| **Slab thickness slider thick end** | IsiX 6.0.4: exponential slider from the slice interval to the whole volume, with tick marks | Same exponential scale and whole-millimetre stops, but the thick end stays at 50 mm and there are no tick marks, because slab sampling is capped at 65 steps and thicker slabs would skip voxels | Review of IsiX 6.0.4 | Slab slider with the thickness in mm beside it |
