# SekhmetX Bug & Error Log

This file tracks known bugs, user-reported issues, and runtime errors.
Per `AGENTS.md`, before updating SekhmetX or fixing any bug:
1. Check this log (`BUGS.md`) for open or recurring issues.
2. Check `.upstream/isix` (`git log` / `python3 tools/upstream.py report isix`) to see if IsiX (`ThalesMMS/IsiX-DICOM-Viewer-macOS`) encountered and fixed the same or a similar issue.
3. Port/adapt the fix, add a regression test, mark the item `[x]` here, and release.

## Resolved
- [x] **686-image Angio stack missing 3D MPR & jumping slices** (`v0.2.1`): Fixed dynamic background queue radius (`max_focus_len.max(512)`), native normal alignment with `vol.slice_dir`, and `disable_viewport_mpr` slice mapping.
- [x] **3D axial resampling blur (`547×547` vs `512×512`)** (`v0.2.1`): Locked `reslice_plane` to exact native `width × height` and in-plane spacing when aligned with the native acquisition plane.
- [x] **Study Browser AX/COR/SAG grouping & MIP contamination** (`v0.2.0`–`v0.2.1`): Separated MIP vs non-MIP stacks and grouped multi-plane reconstructions into inline Horos-style blocks.

## Open / Backlog
- [ ] **UI alignment with IsiX / Horos** (`v0.3.0`): Continue refining toolbar, study browser, and viewport chrome to match IsiX/Horos.
- [ ] **In-app Auto-Update** (`v0.2.2`): Check GitHub Releases (`CosmicTaco22/rdicom-viewer`), show release notes, and download/replace binary in-place.
