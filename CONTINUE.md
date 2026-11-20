# Continue: road to 0.7.0 (started 20 Nov 2026)

The owner asked for all of these together as **0.7.0**. Each step is committed as
its own 0.6.x build on branch `claude/fervent-johnson-ekq5ps` (master gets it when
the owner says so; this cloud session cannot push tags, the owner tags from the PC).

## Plan and state

| Step | Build | State |
|---|---|---|
| Review PET-CT fusion from 0.6.0 (synthetic PET phantom) | 0.6.2 | done: SUV correct; overlay now resampled along any plane |
| Empty images at the ends of a turned plane | 0.6.3 | done: `Volume3D::content_span_mm` |
| Key images case report | 0.6.4 | done: snapshot on `K`, `report.html` export |
| Curved MPR (straightened view along a drawn path) and measurements on MPR planes | 0.6.5 | done: `U` tool, `src/imaging/cpr.rs`, `src/app/cpr_view.rs` |
| 3D sculpting (scissors) in volume rendering | 0.6.6 | next |
| App icon (own drawing), Windows installer, signing-ready release workflow; then 0.7.0 | 0.7.0 | pending |

Not in 0.7.0: PACS access waits for the owner's plugin design; code signing needs a
certificate the owner must buy (prepare the workflow step so it activates when a
certificate secret exists).

## Last check

v0.6.5: clippy (also `--tests`) clean, 142 tests pass, release build OK.

## Test data (scratchpad, regenerate if lost)

`scripts/make_pet.py CT_DIR OUT` makes a PET series for the CT phantom
(`tools/testdata/makect.py`): body SUV 1, hot sphere SUV 10 on the CT lesion.
`scripts/make_recon.py OUT` makes a 1 mm volume with reconstructions, scout and
dose pages.

## Next steps

1. Curved MPR: look at IsiX for a curved-reformation port first (`PORTING_MAP.md`,
   `porting/map.toml`); otherwise build: a polyline tool on an MPR plane, sample
   the volume along it (columns follow the path, rows along the plane normal or
   view-up), show it in a viewport; measurements on the straightened image use its
   mm spacing.
2. Then sculpting, packaging, 0.7.0 (CHANGELOG, README, HANDOFF, delete this file).
