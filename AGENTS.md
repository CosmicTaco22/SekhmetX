# RDicom Viewer: instructions for coding agents

Read `HANDOFF.md` first: it has the project's history, decisions and next steps.

## What this is

A native, open-source (LGPL-3.0-or-later) DICOM viewer in Rust for Windows,
macOS and Linux. It is rebuilt piece by piece using IsiX
(github.com/ThalesMMS/IsiX-DICOM-Viewer-macOS, a fork of Horos/OsiriX) as the
reference. The goal is the Horos interface and feel: same behaviour, same
shortcuts, lightweight and responsive. The owner is a radiologist, not a
programmer: explain results in plain language.

## Rules

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
- Don't use the IsiX/Horos/OsiriX names or icons in the app.
- Not a medical device. Never claim diagnostic use.
- **Session / Token-Limit Handoff (`CONTINUE.md`):** If you are running low on context/tokens or must stop before a task is complete, write or update `CONTINUE.md` at the repository root before ending your turn. Include: (1) current goal/milestone, (2) files modified or in progress, (3) last build/test status, and (4) the exact next steps so the next agent can resume immediately without interruption. When starting a session, read `CONTINUE.md` (if present) right after `HANDOFF.md`.
- **Bug-First & IsiX Upstream Sync Workflow:** Whenever checking for updates or updating SekhmetX:
  1. Read `BUGS.md` and check any logged errors/issues first.
  2. Run `python3 tools/upstream.py fetch` and `python3 tools/upstream.py report isix` (and inspect `.upstream/isix` git log) to see if `ThalesMMS/IsiX-DICOM-Viewer-macOS` encountered the same or similar bugs and how they fixed them.
  3. Port/adapt the IsiX fixes and UI improvements, add regression tests, update `BUGS.md`, and verify with `cargo test --release`.

## Commands

```
cargo build --release
cargo test --release
target/release/rdicom-viewer [FOLDER]            # run the viewer
target/release/rdicom-viewer --check FOLDER      # decode every image, list failures
target/release/rdicom-viewer --render FOLDER OUT.png [SERIES] [IMAGE] [WIDTH LEVEL]
RDICOM_CPU=1 target/release/rdicom-viewer ...    # force CPU windowing (compare with GPU)

python3 tools/upstream.py fetch                  # clone IsiX + Horos into .upstream/
python3 tools/upstream.py report isix            # upstream changes since last review
python3 tools/upstream.py coverage isix --strict # every IsiX file is in the map
python3 tools/test_map.py                        # map regression test
python3 tools/testdata/makect.py DIR/phantom_ct  # synthetic test series (needs pydicom, numpy)
python3 tools/testdata/makecr.py DIR             # run after makect.py: adds DIR/cr and DIR/ct_edges
```

Building needs Rust (stable), a C/C++ compiler and CMake (for OpenJPEG and
CharLS). `--no-default-features` drops JPEG-LS and the CMake requirement.

## Layout

| Path | Role |
|---|---|
| `src/main.rs` | Startup, command-line helpers |
| `src/app.rs` | The window: browser, viewport, tools, overlays, shortcuts |
| `src/gpu_view.rs` | wgpu shader that draws the slice and applies the window |
| `src/imaging/` | Windowing rules ported from IsiX, with its tests |
| `src/frame.rs` | Decoding one image, VOI LUT, value readout |
| `src/loader.rs` | Background decoding threads |
| `src/dicom_index.rs` | Folder scan, study/series grouping, slice sorting |
| `porting/` | Porting map, upstream review state, first upstream report |
| `tools/` | Upstream tracker, map test, test-data generators |
| `.github/workflows/` | Windows build; weekly IsiX/Horos tracking |

Pinned crate versions matter: eframe/egui 0.37 (wgpu 30), dicom-rs 0.10.
Their APIs change between releases; check the crate source before upgrading.
