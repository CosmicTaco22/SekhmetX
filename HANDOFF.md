# Handoff: where the project stands (v0.5.1, 16 Nov 2026)

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
- Virtual PACS: one institution at a time; downloads stay as `.zip` in the
  chosen folder; no browser left running in the background; credentials and
  cookies only in the OS keychain.
- Dark theme is the default.

## What exists (v0.5.1)

- **2D viewer:** scroll, zoom, pan, rotate/flip, invert, cine, Horos window
  presets, 31 CLUTs, 22 convolution filters, GPU window/level with bilinear
  interpolation and a CPU fallback (`RDICOM_CPU=1`).
- **ROIs:** length, angle, oval, rectangle, polygon, pencil, point; area, mean,
  SD, min, max from unfiltered pixels; saved per slice in SQLite.
- **Library:** SQLite (WAL, schema v2), full-window Database view, patient
  grouping, search, modality filters, thumbnails, managed study folder, opening
  of folders, files and `.zip` archives (streamed, with size caps).
- **MPR:** native-plane and oblique reslicing on the GPU (16-bit 3D texture),
  thick-slab MIP/MinIP/Mean, 3-pane Ax·Cor·Sag with crosshairs, trackball orbit
  with an anchor point, slice-regularity checks, gantry-tilt support.
- **3D volume rendering:** GPU ray-caster (VR and MIP), five presets, shading
  with ambient/diffuse/specular, histogram and transfer-function editor,
  clip box, background colour, orientation cube.
- **Comparison:** layouts up to 4×4, colour-coded link groups (zoom, WL, cut),
  two zoom-link modes, cross-study rigid registration on a worker thread,
  Compare popover.
- **Virtual PACS:** login in a WebView subprocess (`--pacs-login`); backends for
  Orthanc, DICOMweb, Vue Motion and a "trained" click-recording mode; two-stage
  patient → study finder; optional anonymized download.
- **Other:** PNG and anonymized DICOM export, anonymized `Ctrl+C` screenshot,
  right-click menu, drag-and-drop series, metadata inspector, EN/ES interface,
  Settings page, first-run notice, in-app updater with SHA-256 check, opt-in
  scrubbed crash log.

### Code layout

| Path | Role |
|---|---|
| `src/main.rs` | Startup, command-line helpers, `--pacs-login` subprocess entry |
| `src/app/mod.rs` | App state, status bar, shortcuts, background polling |
| `src/app/browser.rs` | Database view, study sidebar, compact thumbnail strip |
| `src/app/viewport.rs` | Viewport grid, 2D/MPR/VR drawing, context menu, drops |
| `src/app/overlays.rs` | HUD corners, rulers, orientation letters, ROI callouts |
| `src/app/mpr_sync.rs` | MPR state, link groups, compare, registration |
| `src/app/modals.rs` | Settings, PACS, first-run, crash-log and TF-editor windows |
| `src/app/metadata.rs` | DICOM metadata inspector |
| `src/ui/toolbar.rs` | Grouped wrapping toolbar and the scrubber/MPR/VR strip |
| `src/gpu_view.rs` | wgpu shaders: 2D window/level, 3D MPR, volume ray-caster |
| `src/volume.rs` | Volume assembly, reslicing, slab modes, rigid registration |
| `src/imaging/`, `src/geometry/`, `src/roi/` | Logic ported from IsiX, with its tests |
| `src/dicom_index.rs`, `src/frame.rs`, `src/loader.rs` | Scan and grouping, decoding, background loading |
| `src/db.rs`, `src/settings.rs` | SQLite library and typed settings |
| `src/pacs_web/` | PACS controller, backends, WebView subprocess, keychain, `scripts/*.js` |
| `src/export.rs`, `src/updater.rs`, `src/crash_log.rs`, `src/i18n.rs` | Export/anonymize, updater, crash log and data folder, strings |
| `docs/UI_SPEC.md`, `docs/ui-reference/` | UI spec and 16 OsiriX reference screenshots from the owner |
| `porting/map.toml` → `PORTING_MAP.md` | Every IsiX file assigned to an area with progress |

Local data lives in `%LOCALAPPDATA%\SekhmetX` (or beside the exe when a
`portable.txt` file is present); the equivalent user data folder on macOS/Linux.

## What was verified, and what was not

Verified on Linux, 16 Nov 2026, at v0.5.1 plus the lint fix:
- `cargo test --release`: 105 passed, 0 failed.
- `cargo clippy --release -- -D warnings`: clean after one fix in
  `src/imaging/convolution.rs` (Rust 1.97 rejects an indexed loop). That fix is
  commit `5f7fd63` on branch `claude/fervent-johnson-ekq5ps` and is **not yet on
  `master`**; CI uses stable Rust and will fail on the same lint without it.
- `cargo build --release` succeeds; `python3 tools/test_map.py` passes.
- Synthetic CT and CR series (`tools/testdata/`): 161 of 161 images decode;
  `--list`, `--check` and `--render` work.

Verified by the owner on Windows: the interface, MPR, volume rendering, compare
and the Vue Motion PACS flow, by hand, version by version.

Not verified:
- No automated test covers the window, the GPU paths or the PACS WebView.
- macOS builds in CI but the owner has not run it on a Mac.
- No side-by-side comparison of measurements against Horos/IsiX on real studies
  (`--roi` exists for that purpose).

## Known gaps and loose ends

- **Docs out of date.** `AGENTS.md`, `GEMINI.md` and `README.md` still mention a
  single `src/app.rs`, the `rdicom-viewer` binary and the `»` toolbar overflow
  (replaced by wrapping in v0.4.3). `ROADMAP.md` checkboxes are stale.
  `CHANGELOG.md` has no v0.5.1 entry.
- **Upstream tracking half-removed.** `porting/upstream.toml` and
  `.github/workflows/upstream.yml` were deleted in the v0.3.0 clean-up, but
  `AGENTS.md` still tells agents to run `tools/upstream.py fetch` / `report`.
  Whether those commands still work without the state file has not been
  checked. The 17 IsiX commits after `a05084b6` were never reviewed.
- **Antivirus false positive** (Trend Micro flagged the unsigned exe). v0.5.1
  added Windows file metadata and stopped stripping symbols; code-signing or an
  installer is the lasting fix.
- **Parked features:** "Add a Portal" submission to the owner; automatic
  crash-report sending (currently a pre-filled GitHub issue the user must
  submit); the "save library copies as `.zip`" setting is stored but copies are
  still written uncompressed.
- **Local Windows builds** in the owner's Antigravity folder use the
  `windows-gnu` toolchain, which needs `WebView2Loader.dll` beside the exe.
  GitHub's MSVC builds link it statically.
- 12-bit lossy JPEG was unsupported at v0.1; not rechecked since.
- `BUGS.md` has one open item: continued UI alignment with IsiX/Horos.

## Next steps

The owner was offered these after v0.5.1 and has not chosen yet. Ask him.

1. Curved MPR and measurements in 3D MPR.
2. PET-CT fusion with SUV (`fusion-pet`, P2, not started).
3. Key images and report/snapshot export (`annotations-sr`, P2, not started).
4. Native DICOM query/retrieve or DICOMweb client (`network-pacs`, `dicomweb`,
   P2, not started) alongside the web-portal hook.
5. 3D scissors/sculpting in volume rendering.
6. App icon, code-signing and an installer.

Housekeeping worth doing first: merge the lint fix into `master`, add the
v0.5.1 changelog entry, and bring `AGENTS.md`, `GEMINI.md` and `README.md` in
line with the current layout.

## Working across sessions

Work has moved between Antigravity (Gemini) on the owner's PC, a Claude Code
cloud session, and a local Claude Code session in the same PC folder. Only one
should change code at a time, and it should push before the owner switches, so
the copies do not diverge. Cloud sessions cannot produce a local
`sekhmetx.exe`; Windows binaries from the cloud come from GitHub Actions.
