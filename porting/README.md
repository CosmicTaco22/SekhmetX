# Porting from IsiX and keeping up with it

RDicom is rebuilt in Rust using IsiX (and Horos, which IsiX is forked from) as
the reference. Two files here drive that:

- **`map.toml`**: the porting map. Every IsiX and Horos file belongs to one
  *area* (2D viewer, ROIs, PACS networking, ...), and each area has a decision
  (port, rebuild, replace, reuse, skip), a priority, and how far RDicom has got.
  `PORTING_MAP.md` at the top of the project is the readable version.
- **`upstream.toml`**: the last IsiX and Horos commit whose changes we reviewed.

## Keeping up with upstream

IsiX changes often. Every Monday a GitHub workflow (`.github/workflows/upstream.yml`)
checks IsiX and Horos and keeps one issue per project open, listing what changed
since the last review, sorted by area:

1. **Review**: changes in areas RDicom already has. Decide what to bring over.
2. **Noted**: changes in planned areas RDicom hasn't started. Nothing to do now:
   when we start that area we port IsiX's latest version, which already includes them.
3. **Ignored**: areas we don't port (plugins, web portal, translations, build files).
4. **⚠ New files not in the map**: add a pattern to `map.toml` so they're sorted next time.

Nothing is merged automatically. IsiX is Swift/Objective-C and RDicom is Rust,
so each change is ported by hand (by Claude, with you checking the result).

When you've gone through an issue:

```
python3 tools/upstream.py fetch
python3 tools/upstream.py mark-reviewed isix
git commit -am "Reviewed IsiX up to <commit>" && git push
```

The next run closes the issue. You can also run the workflow any time from the
GitHub **Actions** tab ("Track IsiX and Horos" → *Run workflow*).

## The same, on your own computer

Needs Python 3.11+ and git.

```
python3 tools/upstream.py fetch              # download / update IsiX and Horos into .upstream/
python3 tools/upstream.py report isix        # what changed since the last review
python3 tools/upstream.py coverage isix      # check every IsiX file is in the map
python3 tools/upstream.py render-map isix    # regenerate PORTING_MAP.md
python3 tools/test_map.py                    # check the map after editing it
```

## When an area is ported

Update its entry in `map.toml`: set `progress` to `partial` or `done`, fill in
`rdicom` (where the code lives), and set `ported_from` to the IsiX commit the
port was based on. Then run `render-map`. From then on, IsiX changes to that
area show up under **Review**.

## Checking a port

Port IsiX's own tests along with the code: the IsiX modules (`Modules/IsiX*/Tests`,
about 250 test files) describe expected results that the Rust version must also
produce. For anything visual, compare RDicom and IsiX side by side on the same study.
