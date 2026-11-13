# RDicom Viewer

A small, fast DICOM viewer for Windows, written in Rust. It is a from-scratch
rewrite that uses [IsiX DICOM Viewer](https://github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS)
(and Horos/OsiriX before it) as the reference for features and behaviour.
No IsiX code runs here: IsiX is built on macOS-only frameworks (AppKit, Metal,
CoreData), so the logic is re-implemented in portable Rust.

> **Not a medical device.** For personal use, teaching and second looks only;
> not for primary diagnosis.

## What it does (v0.1)

- Open a folder (recursively), individual files, or drag them onto the window.
- Study / series browser grouped by patient, study and series. Series are split
  by image size and orientation, so localizers don't get mixed into a stack.
- Slices ordered by instance number, then by position along the slice normal.
- 2D viewing: mouse-wheel stacking, window/level, zoom, pan, invert, cine.
- CT window presets (keys 1–8), or the image's own window (key 0).
- Length (mm) and angle measurements, kept per image while the app is open.
- Live value under the cursor (HU for CT).
- Non-square pixels displayed with the correct aspect ratio.
- Background decoding of neighbouring slices, so large stacks scroll smoothly.
- Compression: uncompressed, JPEG baseline and lossless, JPEG 2000, JPEG-LS,
  RLE, deflate. Colour images (ultrasound, screen captures) and multi-frame
  files (cine loops) are supported.

**Not yet supported:** 12-bit *lossy* JPEG (old JPEG Extended, some NM and
older CT). `--check` lists any file it can't decode.

## Mouse and keyboard

| Action | Control |
|---|---|
| Previous / next image | Mouse wheel, arrow keys |
| Jump 10 images / first / last | Page Up / Page Down, Home / End |
| Active tool (W/L, pan, zoom, length, angle) | Left drag |
| Pan | Middle drag |
| Zoom | Right drag, Ctrl + wheel |
| Reset view | Double-click, R |
| Window presets | 1 Abdomen · 2 Lung · 3 Bone · 4 Brain · 5 Mediastinum · 6 Liver · 7 Stroke · 8 Angio · 0 image default |
| Invert | I |
| Tools | W window/level · M length · A angle · Z zoom |
| Angle | Drag the first arm toward the vertex, then click the end of the second arm |
| Delete last measurement on this image | Delete |
| Cine play / stop | P or Space |

## Getting the Windows .exe

### Option A — let GitHub build it (no setup on your PC)

1. Create a (private) repository on GitHub and push this folder to it.
2. Open the repository's **Actions** tab. The *Build for Windows* workflow runs on
   every push; open the latest run and download **rdicom-viewer-windows**.
3. Unzip and run `rdicom-viewer.exe`. Windows SmartScreen may warn because the
   file isn't signed: choose *More info → Run anyway*.

### Option B — build it on your PC

1. Install Rust from <https://rustup.rs> (`rustup-init.exe`). When it asks, let it
   install the Visual Studio C++ Build Tools.
2. Install CMake: `winget install Kitware.CMake` (needed for JPEG-LS; skip it and
   build with `--no-default-features` if you don't need JPEG-LS).
3. In a terminal inside this folder:

   ```
   cargo build --release
   target\release\rdicom-viewer.exe
   ```

You can pin the exe to the taskbar, or associate it with folders via
"Send to": `rdicom-viewer.exe "C:\path\to\study"` opens a folder directly.

## Command-line helpers

```
rdicom-viewer --list FOLDER        print the studies and series found
rdicom-viewer --check FOLDER       decode every image and report failures
rdicom-viewer --render FOLDER OUT.png [SERIES] [IMAGE] [WIDTH LEVEL]
```

## Code layout

| File | Role |
|---|---|
| `src/dicom_index.rs` | Folder scan, header parsing, study/series grouping, slice sorting |
| `src/frame.rs` | Pixel decoding, modality LUT, window/level rendering, value readout |
| `src/loader.rs` | Background decoding threads, nearest-slice-first |
| `src/app.rs` | The window: browser, viewport, tools, overlays, shortcuts |
| `src/main.rs` | Startup and command-line helpers |

Built on [dicom-rs](https://github.com/Enet4/dicom-rs) for DICOM and
[egui](https://github.com/emilk/egui) for the interface.

## Porting from IsiX

- `PORTING_MAP.md`: every area of IsiX/Horos, what we do with it (port, rebuild,
  replace, skip), its priority and RDicom's progress.
- `porting/README.md`: how the map works and how RDicom keeps up with new IsiX
  and Horos changes (a weekly GitHub workflow opens an issue listing them).

## Licence

LGPL-3.0-or-later, matching IsiX, since features are ported from it. The IsiX,
Horos and OsiriX names and icons belong to their owners and are not used here.
