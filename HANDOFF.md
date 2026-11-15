# Handoff: where the project stands (v0.3.0, 14 Nov 2026)

Written for whoever continues this, human or agent. It records what was
built, what was decided and why, what was verified, and what comes next.
- **v0.3.0 milestone status:** Security & privacy hardening (S1–S10: OS keyring credentials/cookies, HTTPS-only WebView origin checks, strict anonymized ZIP/DICOM export, scrubbed opt-in crash log), direct Virtual PACS login/hook/search/download (Orthanc, DICOMweb, Vue Motion, Trained DOM), internal build numbers (`build.rs`), typed settings (`src/settings.rs`), `cargo clippy --release -- -D warnings` in CI, and initial module splits (`src/app/`, `src/pacs_web/`). Horos upstream tracking has been removed.

## The owner and the goal

The owner is a radiologist who reads on both a Mac and a Windows PC (RTX 5080).
He considers the Horos/OsiriX interface the best there is, and values how
lightweight and responsive it feels. Horos and its modern fork IsiX are
macOS-only. The goal is a native, open-source viewer in Rust that looks and
behaves like Horos and runs on Windows, macOS and Linux, and that keeps up with
IsiX/Horos as they change.

## Decisions made, and why

1. **IsiX cannot be compiled for Windows.** It is about 380,000 lines of Swift
   and Objective-C on AppKit (UI), Metal (GPU), CoreData (database), Accelerate,
   PreferencePanes, Security, Carbon. It targets recent macOS only.
2. **Cocotron + Metal-to-Vulkan translators were considered and rejected.**
   - Swift's Objective-C interop exists only on Apple platforms, so IsiX's
     ~600 Swift files (almost all subclassing NSObject) can't build against
     Cocotron. Cocotron is Objective-C only and covers a ~2010-era AppKit.
   - IsiX has no `.metal` files: shaders are strings compiled at runtime, some
     using Metal 4. `steelbrain/metal2vulkan` converts compiled AIR and is a
     Rust runtime, not a drop-in Metal; `joshuaswarren/metal2vk` handles only
     compute kernels on Linux.
3. **Chosen approach: rebuild in Rust, piece by piece, with IsiX as the
   reference and the test.** Logic (maths, rules, data) is ported file by file
   along with IsiX's tests. Screens and rendering are rebuilt with egui + wgpu
   to match Horos. One codebase builds for all three systems.
4. **Licence:** LGPL-3.0-or-later, same as IsiX, since code is derived from it.
   Its names and icons are not used.
5. **Priorities (owner approved P1 and P2 as listed in `PORTING_MAP.md`).**
   P1 = daily-reading core: DICOM files, image pipeline, 2D viewer, ROIs,
   database/import, study browser, geometry, colour tables/presets.
   P2 = PACS networking, DICOMweb, MPR, PET-CT fusion, GSPS/key images,
   anonymize/export, preferences, app shell.
   Skipped: plugins, web portal, printing, CD burning, e-mail, Cocoa widgets.
6. **Upstream tracking is manual-with-automation:** a weekly workflow lists what
   changed upstream by area; nothing is merged automatically because the
   languages differ.
7. **Local model on the 5080 (idea, not done):** a local coding model could
   draft mechanical Swift→Rust translations and test skeletons for review.
   Better uses of the card: RDicom's own rendering, and local Whisper for
   Spanish dictation.

## What exists

### v0.1 (16 Oct 2026, commit 4e81b17)
2D viewer: folder scan, study/series browser, slice sorting, background
decoding, scroll, zoom, pan, invert, cine, length and angle measurement, HU
under the cursor, overlays. Codecs: uncompressed, JPEG baseline/lossless,
JPEG 2000 (OpenJPEG C library; the pure-Rust decoder crashed on valid files),
JPEG-LS (CharLS), RLE, deflate; colour and multi-frame.

### Porting map and upstream tracker (commit ed0cc77)
- `porting/map.toml`: all 5,678 IsiX files and Horos's older ones assigned to
  30 areas (decision, priority, progress). `PORTING_MAP.md` is generated from it.
- `tools/upstream.py`: `fetch`, `report`, `mark-reviewed`, `coverage`,
  `render-map`. `tools/test_map.py` guards the map.
- `.github/workflows/upstream.yml`: every Monday, one issue per upstream.
- `porting/upstream.toml`: IsiX reviewed up to `a05084b6` (15 Oct 2026). The
  17 commits since then are listed in
  `porting/reports/2026-11-13-isix-since-rdicom-v0.1.md` and **not yet reviewed**.
- Key finding: IsiX 6.0 (12 Nov 2026) moved ~113,000 lines of logic into
  `Modules/IsiX*` Swift packages with no AppKit or Metal imports and ~250
  test files. Those are the easiest code to port. In the same month IsiX
  rewrote DCMPix, ROI, ViewerController and DCMView from Objective-C to Swift.
  Horos itself has been quiet since Feb 2026; IsiX contains all of it.

### Window/level pipeline + GPU drawing (commit 9d9da68)
`src/imaging/`, ported from IsiX `3e24831224b8`:
- `window.rs`: requested window (whole values on wide images, minimum width),
  full window, linear grey and colour-table windowing.
- `voi_lut.rs`: VOI LUT tables, applied only when rescale is identity, window
  then set to the table's output range (as Horos).
- `defaults.rs`: default-window order (stored → frame range → stored bits);
  a series takes its default from its middle image.
- `photometry.rs`: MONOCHROME1 inverted for display, except PET.
- `drag.rs`: window drag, width ÷ 80 per point from the drag start; right =
  wider, up = higher level. PET modes ported, not wired in.
- `presets.rs`: Horos factory presets, hot keys 1–9 in case-insensitive
  alphabetical order, window naming (preset / Default / Full dynamic / Other).

`src/gpu_view.rs`: each slice uploaded once as an R32Float (or RGBA8) texture;
a WGSL shader windows each texel then interpolates. `RDICOM_CPU=1` uses the CPU
path, also used for images above the GPU's texture limit.

## What was verified, and what was not

Verified on Linux (software-emulated GPU):
- 22 unit tests pass, most translated from IsiX, including its drag test
  (18,900 combinations, bit-for-bit against the old Objective-C formula).
- 201 of 202 test images decode. The failure is 12-bit lossy JPEG (unsupported).
- Synthetic CR with VOI LUT + MONOCHROME1: four sampled greys equal the hand
  calculation. CT with a different stored window on edge slices opens at the
  middle image's window. A 40-point diagonal drag from the lung preset gives
  exactly the formula's result.
- GPU and CPU pictures agree within 2 grey levels of 255 (default view;
  preset + zoom + pan; inverted).
- Length measurement on a known 30 mm structure read 29.7 mm (mouse-pixel rounding).

**Not verified:**
- No Windows or macOS build has been made or run. `.github/workflows/windows.yml`
  is untested. The GPU path has never run on real hardware (DX12/Metal).
- No side-by-side comparison with real Horos/IsiX on real studies.
- Horos's grey conversion (`vImageConvert_PlanarFtoPlanar8`) has undocumented
  rounding; ours rounds to nearest and may differ by one grey level.
- `.github/workflows/upstream.yml` was only simulated step by step locally.

## Known gaps and rough edges

- 12-bit lossy JPEG (JPEG Extended) doesn't decode.
- Only the five factory window presets; user presets and settings aren't stored.
- No database: a folder is re-scanned on every open. No thumbnails.
- Measurements live only in memory. Angle tool is drag + click, not Horos's.
- Slices are cached as f32 (twice the size of the 16-bit originals).
- Wheel scrolling has none of Horos's acceleration; no hanging protocols,
  layouts, sync or reference lines yet.
- Side-panel width varied by ~18 px between two test runs; cause not investigated.

## Next steps, in order

1. Build and run on Windows (push to GitHub → Actions → download the exe);
   fix whatever the first real DX12 run shows. Compare with Horos on the Mac.
2. **ROIs** (P1): port `Modules/IsiXROI` (68 files, 25 test files): ellipse,
   rectangle, polygon with mean/SD/min/max/area; then the drawing tools.
3. **Geometry** (P1): port `Modules/IsiXGeometry` (needed by ROIs and MPR).
4. **Study browser + SQLite database** (P1): `Modules/IsiXDatabase`,
   `IsiXStorage` → rusqlite; thumbnails; Horos's columns and preview.
5. Colour tables: load `Horos/Resources/CLUTs/*.plist` directly.
6. Review the 17 pending IsiX commits for the areas already ported, then
   `python3 tools/upstream.py mark-reviewed isix`.
7. P2: MPR (wgpu), PACS query/retrieve (dicom-ul), DICOMweb, PET fusion/SUV.

The owner has been asked, and has not yet answered: which Horos features he
uses most (to reorder P1/P2), which modalities he reads most, whether he needs
PACS, and whether ROIs or the study browser should come first.

## Related work from the same conversation

`dicom-slides-backup/` (beside this project in the zip) is a separate item: an
offline/self-hosted backup of the DICOM Slides PowerPoint add-in
(github.com/ThalesMMS/dicom-slides, MIT). It has a full copy of the add-in,
manifests pointing to `https://localhost:3000` or to the owner's own GitHub
Pages, and a small Node HTTPS server with a Windows launcher. Its own README
explains it. The local server was tested; loading it inside PowerPoint was not.
A macOS launcher was offered but not made. Installing the original add-in on
the Mac failed from the agent side (macOS blocks writes into PowerPoint's
container); the owner was given a one-line Terminal command instead.
