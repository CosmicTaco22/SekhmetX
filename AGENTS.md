# SekhmetX: instructions for coding agents

Read `HANDOFF.md` first: it has the project's history, the owner's decisions and
the next steps. Then read `CONTINUE.md` if it exists.

`GEMINI.md` is a copy of this file. When you change one, change the other.

## What this is

SekhmetX (formerly "RDicom Viewer"; crate and binary `sekhmetx`) is a native,
open-source (LGPL-3.0-or-later) DICOM viewer in Rust for Windows, macOS and
Linux. It is a port of IsiX DICOM Viewer
(github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS, by Thales Matheus M Santos, a
fork of Horos, itself derived from OsiriX), rebuilt piece by piece with IsiX as
the reference. The goal is the Horos interface and feel: same behaviour, same
shortcuts, lightweight and responsive. The owner is a radiologist, not a
programmer: explain results in plain language.

## Rules

**Porting**
- **Port, don't invent.** When a behaviour exists in IsiX, read the IsiX source
  and reproduce it (rounding and edge cases included). Name the IsiX file in a
  comment at the top of each Rust file that follows one.
- **Port IsiX's tests with the code.** `Modules/IsiX*/Tests` hold the expected
  results; translate them to `#[cfg(test)]` tests next to the Rust code.
- **Logic is ported, screens are rebuilt.** AppKit/Metal/CoreData code can't be
  translated: rebuild it with egui (UI) and wgpu (GPU), matching Horos.
- **Follow the map.** `porting/map.toml` assigns every IsiX file to an area with
  a decision and priority. After porting, update the area's `progress`,
  `rdicom` and `ported_from`, then run `python3 tools/upstream.py render-map isix`.
  A knowing difference from IsiX goes in `DEVIATIONS.md`.

**Product**
- Don't use the IsiX/Horos/OsiriX names or icons in the app. Toolbar icons are
  our own vector drawings (`src/ui/icons.rs`). Crediting those projects by name
  in `README.md` and `NOTICE` is required, not forbidden: keep those credits.
- Not a medical device. Never claim diagnostic use.
- Never ship a real institution's name, address or credentials, and never put
  patient data in the repository. Screenshots use the synthetic phantom only.
- Every new or changed piece of interface text gets its Spanish translation in
  the same change (`src/i18n.rs`, or an `es`/`Lang` match beside the English).
- Colours come from `src/ui/theme.rs` (use `theme::accent()` for highlights, not
  a fixed blue), so the user's accent colour and the light theme keep working.

**Every change**
- Verify with `cargo clippy --release -- -D warnings`, `cargo test --release`
  and `cargo build --release` before reporting a build as ready.
- Every new build bumps the last version number in `Cargo.toml`; a milestone
  bumps the middle one. Released versions have three parts. A release is
  published by pushing a `v*` tag; ask the owner before tagging.
- Update `CHANGELOG.md`, and `README.md` Features when what the app does
  changes (describe what exists now, without version numbers).
- Check `git status` and the staged diff before every commit: no transcripts,
  test files, patient data or credentials.
- Write source files as UTF-8. On Windows, do not edit sources through
  PowerShell's default encoding; it has corrupted symbols twice.
- Keep `LICENSE`, `COPYING` and `NOTICE`, and keep shipping them in release
  archives.

**Handoff**
- **`CONTINUE.md`:** if you are running low on context/tokens or must stop
  before a task is complete, write or update `CONTINUE.md` at the repository
  root before ending your turn: (1) current goal/milestone, (2) files modified
  or in progress, (3) last build/test status, (4) the exact next steps. Delete
  it when the task it describes is finished.
- Keep `HANDOFF.md` current when the state of the project changes.

## Bug-first and IsiX sync workflow

Whenever checking for updates or updating SekhmetX:

1. Read `BUGS.md` and check any logged errors first.
2. Run `python3 tools/upstream.py fetch isix` and
   `python3 tools/upstream.py report isix` to see what IsiX changed since the
   last review (`porting/upstream.toml`), and whether IsiX met the same bug and
   how it fixed it. The clone is in `.upstream/isix` (not committed).
3. Port or adapt the IsiX fixes with their tests, add regression tests, update
   `BUGS.md`, and verify.
4. Write what was reviewed, ported and left for later in
   `porting/reports/`, then `python3 tools/upstream.py mark-reviewed isix`.
   `python3 tools/upstream.py coverage isix --strict` must pass: assign any new
   IsiX files to an area in `porting/map.toml`.

IsiX's history has been rewritten upstream at least once, so an old commit hash
may no longer exist; compare by date and content when that happens.

## Commands

```
cargo build --release
cargo test --release
cargo clippy --release -- -D warnings
target/release/sekhmetx [FOLDER_OR_ZIP]          # run the viewer
target/release/sekhmetx --list FOLDER_OR_ZIP     # list studies and series
target/release/sekhmetx --check FOLDER_OR_ZIP    # decode every image, list failures
target/release/sekhmetx --render FOLDER OUT.png [SERIES] [IMAGE] [WIDTH LEVEL]
target/release/sekhmetx --roi FOLDER_OR_FILE [SERIES IMAGE] (ellipse|rect|length) X1 Y1 X2 Y2
RDICOM_CPU=1 target/release/sekhmetx ...         # force CPU windowing (compare with GPU)

python3 tools/upstream.py fetch isix             # clone/update IsiX into .upstream/
python3 tools/upstream.py report isix            # upstream changes since last review
python3 tools/upstream.py mark-reviewed isix     # record the review point
python3 tools/upstream.py coverage isix --strict # every IsiX file is in the map
python3 tools/upstream.py render-map isix        # regenerate PORTING_MAP.md
python3 tools/test_map.py                        # map regression test
python3 tools/testdata/makect.py DIR/phantom_ct  # synthetic test series (needs pydicom, numpy)
python3 tools/testdata/makecr.py DIR             # run after makect.py: adds DIR/cr and DIR/ct_edges
```

Building needs Rust (stable), a C/C++ compiler and CMake (for OpenJPEG and
CharLS). `--no-default-features` drops JPEG-LS and the CMake requirement. Linux
also needs the GTK 3, xkbcommon, Wayland, Vulkan/GL and D-Bus development
packages listed in `.github/workflows/ci.yml`.

To look at the interface without a desktop (cloud or CI machine): run the app
under `Xvfb` with a software Vulkan driver (`mesa-vulkan-drivers`,
`WGPU_BACKEND=vulkan`), drive it with `xdotool` mouse clicks, and capture with
ImageMagick `import`. Point `XDG_DATA_HOME` at a scratch folder so the test
profile is separate.

## Layout

| Path | Role |
|---|---|
| `src/main.rs` | Startup, command-line helpers |
| `src/app/mod.rs` | App state, status bar, shortcuts, background polling |
| `src/app/browser.rs` | Library view, study sidebar, compact thumbnail strip |
| `src/app/viewport.rs` | Viewport grid, 2D/MPR/VR drawing, context menu, drops |
| `src/app/overlays.rs` | Corner annotations, rulers, orientation letters, ROI callouts |
| `src/app/mpr_sync.rs` | MPR state, link groups, compare, registration |
| `src/app/modals.rs` | Settings, first-run, crash-log and transfer-function windows |
| `src/app/metadata.rs` | DICOM metadata inspector |
| `src/ui/theme.rs` | Palette, accent colour, spacing and text sizes |
| `src/ui/icons.rs` | Drawn icons and the button widgets that use them |
| `src/ui/toolbar.rs` | Toolbar groups and the slice / MPR / VR strip |
| `src/gpu_view.rs` | wgpu shaders: 2D window/level, 3D MPR, volume ray-caster |
| `src/volume.rs` | Volume assembly, reslicing, slab modes, rigid registration |
| `src/imaging/`, `src/geometry/`, `src/roi/` | Logic ported from IsiX, with its tests |
| `src/imaging/cpr.rs`, `src/app/cpr_view.rs` | Curved MPR: path checks and straightening (from IsiX), its window |
| `src/sculpt.rs` | 3D scissors: keep/cut map and the shared 3D camera |
| `src/frame.rs`, `src/loader.rs` | Decoding one image; background decoding threads |
| `src/dicom_index.rs` | Folder/zip scan, study/series grouping, slice sorting |
| `src/db.rs`, `src/settings.rs` | SQLite library and typed settings |
| `src/export.rs`, `src/updater.rs`, `src/crash_log.rs`, `src/i18n.rs` | Export/anonymize, updater, crash log and data folder, strings |
| `src/legacy_cleanup.rs` | One-time removal of data left by the removed web-portal login |
| `porting/` | Porting map, upstream review state, review reports |
| `assets/icon/`, `packaging/` | App icon (own drawing); Windows installer script, Linux desktop entry |
| `tools/` | Upstream tracker, map test, test-data generators |
| `docs/` | UI spec, reference screenshots, README screenshots |
| `.github/workflows/` | CI on three systems; release builds on `v*` tags |

Pinned crate versions matter: eframe/egui 0.37 (wgpu 30), dicom-rs 0.10.
Their APIs change between releases; check the crate source before upgrading.
