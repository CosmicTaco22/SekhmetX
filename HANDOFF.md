# Handoff: where the project stands (v0.7.5, 21 Nov 2026)

Written for whoever continues this, human or agent. It records what exists,
the owner's standing rules, what was verified, what is unfinished, and what
comes next. Read this first, then `CONTINUE.md` if it exists.

## The owner and the goal

The owner is a radiologist, not a programmer: explain results in plain
language. He reads on a Windows PC (RTX 5080) and a Mac, and his colleagues who
test the app are native Spanish speakers. He considers the Horos/OsiriX
interface the best there is and wants the same look, behaviour and shortcuts in
a native viewer that runs on Windows, macOS and Linux.

SekhmetX (formerly "RDicom Viewer"; the crate and binary are `sekhmetx`) is a
Rust rebuild that uses IsiX (github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS, the
modern Horos fork) as the reference. Logic is ported file by file with IsiX's
tests; screens are rebuilt with egui + wgpu. Licence LGPL-3.0-or-later. Not a
medical device. Do not use the IsiX/Horos/OsiriX names or icons in the app.

## Standing rules from the owner

These were set over the course of the project and are not all in `AGENTS.md`.

**Versions and releases**
- Every new build bumps the last number (`0.5.1` → `0.5.2` → …). A milestone
  bumps the middle number (`0.6.0`). Released versions always have three parts.
- `build.rs` appends a fourth number (commits since the last tag) for internal
  builds only; `release.yml` refuses to publish a four-part version.
- A release is published by pushing a `v*` tag; pushing to `master` only runs
  CI. The owner says when *not* to publish. Ask before tagging if unsure.
- Commit each version separately. v0.4.1–v0.4.10 were built locally and
  committed together as v0.5.0, so they exist only as `CHANGELOG.md` entries;
  the owner wanted a per-version log.
- Before pushing, check that the previous GitHub Actions run passed.

**Every change**
- Any new or changed UI text gets its Spanish translation immediately
  (`src/i18n.rs`; a test checks EN/ES parity).
- Update `README.md` Key Features on every push so it describes what exists
  now, without version numbers. Update `CHANGELOG.md` and `BUGS.md` too.
- Verify with `cargo clippy --release -- -D warnings`, `cargo test --release`
  and `cargo build --release` before reporting a build as ready.
- Check `git status` and the staged diff before every commit: no transcripts,
  test files, patient data or credentials on GitHub. Conversation transcripts
  are private and are listed in `.gitignore`.
- On Windows, write source files as UTF-8. Editing through PowerShell's default
  encoding twice corrupted symbols (`·`, `×`, `…`, `🔍`, `●`) into garbage.

**Behaviour the owner has decided**
- Never lose image detail. Native-plane MPR must return the source voxels
  unchanged, and Ax/Cor/Sag switch to the native sibling series when one exists.
- Nothing links between viewports unless the user turns it on. Compare asks
  which viewports to use, and WL/WW linking is an opt-in checkbox.
- MPR grouping must not mix reconstructions: MIP stays apart from source
  stacks, and different kernels (brain vs bone) stay apart.
- The app opens on the Scroll tool; `A` / `C` / `S` switch planes; 3D MPR turns
  on automatically for MPR-capable series and is always available with a
  warning for series that are not.
- `sekhmetx.db` lives only in the app's local data folder and is read first at
  startup. Folder scans happen once (first run or new folder); after that,
  refresh is manual. No background polling.
- Studies in the browser are grouped by patient and collapsed by default, with
  manual link/unlink for the same patient under different name spellings.
- The web-portal PACS login was removed in v0.5.4 at the owner's request. Do
  not bring it back, and never ship a real institution's name or address in
  the app. The owner plans to re-add portal access later as a plugin: a
  written guide people give their own AI, which records how a portal's search,
  patient list, study list and download buttons work and produces a small
  file SekhmetX reads. Wait for the owner's design before starting it.
- Dark theme is the default.

## What exists (v0.7.0)

- **2D viewer:** scroll, zoom, pan, rotate/flip, invert, cine, Horos window
  presets, 31 CLUTs, 22 convolution filters, GPU window/level with bilinear
  interpolation and a CPU fallback (`RDICOM_CPU=1`).
- **ROIs:** length, angle, Cobb angle, arrow with text, oval, rectangle,
  polygon, pencil, point; area, mean, SD, min, max from unfiltered pixels (also
  on GPU-reconstructed planes); saved per slice in SQLite. Editing by handles
  and centre dot, delete, draggable callouts, live length ratios, calibration
  from a known length, ROI histogram and line profile.
- **Key images:** `K` / ★, stored in the `key_images` table, filter and export
  with an anonymized case summary.
- **Library:** SQLite (WAL, schema v2), full-window Database view, patient
  grouping, search, modality filters, thumbnails, managed study folder, opening
  of folders, files and `.zip` archives (streamed, with size caps).
- **MPR:** native-plane and oblique reslicing on the GPU (16-bit 3D texture),
  thick-slab MIP/MinIP/Mean, 3-pane Ax·Cor·Sag with crosshairs, trackball orbit
  with an anchor point, slice-regularity checks, gantry-tilt support.
- **3D volume rendering:** GPU ray-caster (VR and MIP), five presets, shading
  with ambient/diffuse/specular, histogram and transfer-function editor,
  clip box, background colour, orientation cube.
- **Comparison of two dates** (`src/volume.rs` registration, `src/app/mpr_sync.rs`):
  layouts up to 4×4, colour-coded link groups (zoom, WL, cut), Compare popover.
  Registration runs on a worker thread: a whole-volume search and a
  body-outline method (body mask per axial slice without the table, height
  profiles, roll from the outline, refinement on body voxels), the better one
  kept, then refined per level (8 cm stretches every 3 cm, `fit_local`) for
  breathing, twist and pelvic tilt. The viewport where a link is switched on
  is the master (`link_master`); followers are turned, centred on the matching
  anatomy and zoomed "= mm" to it. `Ctrl`/`Cmd` adjusts one viewport alone and
  relocks. `sekhmetx.log` records each registration ("Body registration",
  "Local alignment", "Registration finished").
- **Series recognition** (`src/dicom_index.rs`): complete volumes (≥30 slices,
  ≤2 mm) get an MPR badge (MPR-A/C/S when one reconstruction exists as volumes
  in several planes); reconstructions are folded into their volume's card
  (`find_reconstructions`): by position for placeable series, and for
  Secondary Capture pictures (no position/orientation, typical of Canon spine
  coronals) by their run of same-named neighbouring series and acquisition
  time, with the plane inferred from the run. Canon FC kernel codes, Spanish
  kernel words, dose pages to "Single images / Scouts".
- **PET-CT fusion and SUV** (0.6.0 by Antigravity, reviewed in 0.6.2): SUV from
  the DICOM dose and timing tags (`src/imaging/suv.rs`), overlay resampled from
  the PET volume along whatever plane is shown (`draw_fusion_overlay` in
  `src/app/viewport.rs`). Checked on a synthetic PET phantom: SUV 1 and 10 read
  exactly.
- **Curved MPR** (`src/imaging/cpr.rs` ported from IsiX CPR, window in
  `src/app/cpr_view.rs`): `U`, click a path, Enter; turn, width, measure.
- **3D scissors** (`src/sculpt.rs`, mask in the VR shader): `X` or the Scissors
  menu; cut inside/outside a drawn loop, undo, restore.
- **Key-images case report:** `K` keeps an anonymized snapshot as shown; the
  export writes `report.html` (images, measurement values, findings box, print
  to PDF) and `case_summary.txt`.
- **Packaging:** app icon (`assets/icon/`), Windows installer
  (`packaging/windows/sekhmetx.iss`, built in CI and on release), Linux
  `.desktop` entry, macOS `AppIcon.icns`. Unsigned: the owner decided (0.7.1)
  not to buy signing certificates; the installer stays, unsigned.
- **Other:** PNG and anonymized DICOM export, anonymized `Ctrl+C` screenshot,
  right-click menu with a tool palette, maximise a viewport by double-click,
  drag-and-drop series, metadata inspector, EN/ES interface, OLED theme,
  study files to the Recycle Bin (`trash` crate),
  Settings page, first-run notice, in-app updater with SHA-256 check, opt-in
  scrubbed crash log.

### Code layout

| Path | Role |
|---|---|
| `src/main.rs` | Startup, command-line helpers |
| `src/app/mod.rs` | App state, status bar, shortcuts, background polling |
| `src/app/browser.rs` | Database view, study sidebar, compact thumbnail strip |
| `src/app/viewport.rs` | Viewport grid, 2D/MPR/VR drawing, context menu, drops |
| `src/app/overlays.rs` | HUD corners, rulers, orientation letters, ROI callouts |
| `src/app/mpr_sync.rs` | MPR state, link groups, compare, registration |
| `src/app/modals.rs` | Settings, first-run, crash-log and TF-editor windows |
| `src/app/metadata.rs` | DICOM metadata inspector |
| `src/gpu_view.rs` | wgpu shaders: 2D window/level, 3D MPR, volume ray-caster |
| `src/volume.rs` | Volume assembly, reslicing, slab modes, rigid registration |
| `src/imaging/`, `src/geometry/`, `src/roi/` | Logic ported from IsiX, with its tests |
| `src/dicom_index.rs`, `src/frame.rs`, `src/loader.rs` | Scan and grouping, decoding, background loading |
| `src/db.rs`, `src/settings.rs` | SQLite library and typed settings |
| `src/ui/` | Theme and accent colour, drawn icons, toolbar |
| `src/legacy_cleanup.rs` | One-time removal of the old portal login's keychain entries and settings |
| `src/export.rs`, `src/updater.rs`, `src/crash_log.rs`, `src/i18n.rs` | Export/anonymize, updater, crash log and data folder, strings |
| `docs/UI_SPEC.md`, `docs/ui-reference/` | UI spec and 16 OsiriX reference screenshots from the owner |
| `porting/map.toml` → `PORTING_MAP.md` | Every IsiX file assigned to an area with progress |

Local data lives in `%LOCALAPPDATA%\SekhmetX` (or beside the exe when a
`portable.txt` file is present); the equivalent user data folder on macOS/Linux.

## What was verified, and what was not

Verified on Linux, 20 Nov 2026, at v0.7.0: clippy (Rust 1.97) clean, 144 tests
pass, build succeeds; fusion, curved MPR, scissors, the key-image snapshot and
report, and the icon checked on synthetic phantoms. Not verified here: the
installer (it needs a Windows runner; CI builds it on every push), and the
report's folder dialog. Earlier, at
v0.5.37: comparison, series badges and folding checked by eye. The owner tested two-date comparison and series grouping
on real chest-abdomen-pelvis and whole-spine CTs (Canon) up to v0.5.29.

Earlier, at v0.5.18:
- `cargo clippy --release -- -D warnings` (Rust 1.97): clean. GitHub's CI used
  an older Rust and passed v0.5.17 even though Rust 1.97 rejected it; expect
  newer lints to appear when CI updates.
- `cargo test --release`: 115 passed, 0 failed. `cargo build --release` succeeds.
- Screens checked by eye (Xvfb + software Vulkan): viewer, right-click menu
  and palette in English and Spanish, library, Settings with OLED, ROI drawing,
  handle editing, histogram.
- Windows-only code (the Vulkan probe in `src/main.rs`) is compiled and run in
  a test on every system; the Direct3D 12 fallback itself has not been seen on
  a PC without Vulkan.

Verified by the owner on Windows: the interface, MPR, volume rendering, compare
by hand, version by version (the PACS portal flow, since removed, too).

Not verified:
- No automated test covers the window or the GPU paths. Screens can be
  checked by eye in a cloud session (Xvfb + software Vulkan); see `CHANGELOG.md`
  v0.5.2–v0.5.4 for what was looked at.
- macOS builds in CI but the owner has not run it on a Mac.
- No side-by-side comparison of measurements against Horos/IsiX on real studies
  (`--roi` exists for that purpose).

## Known gaps and loose ends

- **Keep the credits.** `README.md` and `NOTICE` credit IsiX (Thales Matheus M
  Santos / ThalesMMS), Horos and OsiriX; `LICENSE`, `COPYING` and `NOTICE` ship
  in every release archive. Keep them when changing packaging. README
  screenshots live in `docs/screenshots/` and must only show the synthetic
  phantom (`tools/testdata/`), never patient data.

- **`ROADMAP.md` is stale** (its checkboxes predate most of the work);
  `PORTING_MAP.md` and `CHANGELOG.md` are the current record.
- **IsiX tracking** was restored in v0.5.5: `porting/upstream.toml` records
  IsiX as reviewed up to 6.0.4 (`087f2cd0f721`), and the review is written up
  in `porting/reports/2026-11-16-isix-6.0.4-review.md`, including what was
  left unported and why. The weekly GitHub workflow that opened issues was not
  restored. IsiX's history was rewritten upstream once; old hashes may vanish.
- **Antivirus false positive** (Trend Micro flagged the unsigned exe). v0.5.1
  added Windows file metadata and stopped stripping symbols; code-signing or an
  installer is the lasting fix.
- **Parked features:** automatic crash-report sending (currently a pre-filled GitHub issue the user must
  submit); the "save library copies as `.zip`" setting is stored but copies are
  still written uncompressed.
- **Local Windows builds** in the owner's Antigravity folder used the
  `windows-gnu` toolchain; since v0.5.4 no `WebView2Loader.dll` is needed.
- 12-bit lossy JPEG was unsupported at v0.1; not rechecked since.
- `BUGS.md` has one open item: continued UI alignment with IsiX/Horos.

## Next steps

Done in `v0.7.2`–`v0.7.5`: Curved MPR thick-slab (MIP/MinIP/Mean), saved paths,
transverse cross-section, and Virtual Endoscopy / Fly-through; 256³ parallel 3D
scissors; `USRegion` ultrasound calibration; DICOM SR & GSPS viewport viewer;
animated GIF cine export; compressed `.zip` library storage; DWI b-value
splitting and live ADC/T2 map generation; 3D ROI volume (`cm³`) and ROI Undo
(`Ctrl+Z`); custom W/L presets & Hanging Protocols UI; direct `DICOMDIR`
opening; and `src/net/mod.rs` PACS/DICOMweb groundwork.

Still open:
1. Real-world testing & polish of `v0.7.2`–`v0.7.5` features on clinical exams
   (PET-CT, MR DWI, Ultrasound, SR dose reports, Curved MPR + Fly-through,
   two-date alignment tuning from `sekhmetx.log`).
2. Native DICOM PACS (`dicom-ul`) & DICOMweb (`ureq`) transports wired to
   `src/net/mod.rs` (parked for later testing).
3. Web-portal plugin system — dead last, waits for the owner's design.
4. No code signing: the owner will not pay for certificates (decided at 0.7.1).
   Do not add signing back unless asked.

## Working across sessions

**State on 20 Nov 2026:** the fusion work from the owner's local v0.5.29 was
redone by Antigravity on top of GitHub's master as v0.6.0; 0.6.1–0.7.0 followed
in a Claude Code cloud session on branch `claude/fervent-johnson-ekq5ps`. Cloud
sessions can push branches (and `master` when the owner asks) but not tags: the
owner tags releases from the PC.


Work has moved between Antigravity (Gemini) on the owner's PC, a Claude Code
cloud session, and a local Claude Code session in the same PC folder. Only one
should change code at a time, and it should push before the owner switches, so
the copies do not diverge. Cloud sessions cannot produce a local
`sekhmetx.exe`; Windows binaries from the cloud come from GitHub Actions.
