#!/usr/bin/env python3
"""Keep RDicom in step with IsiX and Horos.

Uses porting/map.toml (which IsiX/Horos files belong to which area) and
porting/upstream.toml (which upstream commit we last reviewed).

  python3 tools/upstream.py fetch                 clone or update the upstream repos into .upstream/
  python3 tools/upstream.py report isix           what changed upstream since the last review, by area
  python3 tools/upstream.py mark-reviewed isix    record that everything up to upstream HEAD was reviewed
  python3 tools/upstream.py coverage isix         check every upstream file is assigned to an area
  python3 tools/upstream.py render-map            regenerate PORTING_MAP.md from the map

Needs Python 3.11+ and git. No other packages.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from collections import defaultdict
from dataclasses import dataclass, field
from datetime import date
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11
    sys.exit("This tool needs Python 3.11 or newer (for tomllib).")

ROOT = Path(__file__).resolve().parent.parent
MAP_FILE = ROOT / "porting" / "map.toml"
STATE_FILE = ROOT / "porting" / "upstream.toml"
CACHE_DIR = ROOT / ".upstream"
MAP_DOC = ROOT / "PORTING_MAP.md"

DECISION_LABEL = {
    "port": "Port",
    "rebuild": "Rebuild",
    "replace": "Replace with crate",
    "reuse": "Reuse data",
    "skip": "Skip",
    "meta": "Not app code",
}
PRIORITY_LABEL = {1: "P1 · daily core", 2: "P2 · next", 3: "P3 · later", 4: "P4 · maybe", 0: "—"}
PROGRESS_LABEL = {"todo": "not started", "partial": "started", "done": "done"}
SOURCE_EXT = re.compile(r"\.(swift|m|mm|h|hpp|c|cc|cpp|metal|py|pch)$")


# --------------------------------------------------------------------------- map


@dataclass
class Area:
    id: str
    name: str
    decision: str
    priority: int
    progress: str
    rdicom: str = ""
    crates: str = ""
    notes: str = ""
    ported_from: str = ""
    paths: list[re.Pattern] = field(default_factory=list)
    names: list[re.Pattern] = field(default_factory=list)
    fallback_paths: list[re.Pattern] = field(default_factory=list)
    fallback_names: list[re.Pattern] = field(default_factory=list)

    @property
    def active(self) -> bool:
        """Changes here may need porting now."""
        return self.planned and self.progress in ("partial", "done")

    @property
    def planned(self) -> bool:
        return self.decision in ("port", "rebuild", "replace", "reuse") and self.priority > 0


def load_map() -> list[Area]:
    with open(MAP_FILE, "rb") as f:
        data = tomllib.load(f)
    areas = []
    seen = set()
    for a in data["area"]:
        if a["id"] in seen:
            sys.exit(f"map.toml: duplicate area id {a['id']!r}")
        seen.add(a["id"])

        def comp(key):
            out = []
            for p in a.get(key, []):
                try:
                    out.append(re.compile(p))
                except re.error as e:
                    sys.exit(f"map.toml: bad pattern in area {a['id']!r} ({key}): {p!r}: {e}")
            return out

        areas.append(
            Area(
                id=a["id"],
                name=a["name"],
                decision=a["decision"],
                priority=a.get("priority", 0),
                progress=a.get("progress", "todo"),
                rdicom=a.get("rdicom", ""),
                crates=a.get("crates", ""),
                notes=a.get("notes", ""),
                ported_from=a.get("ported_from", ""),
                paths=comp("paths"),
                names=comp("names"),
                fallback_paths=comp("fallback_paths"),
                fallback_names=comp("fallback_names"),
            )
        )
    return areas


def stem(path: str) -> str:
    """'Horos/Sources/DCMView+Drawing.m' -> 'DCMView'; 'ROIEditingTests.swift' -> 'ROIEditing'."""
    name = path.rsplit("/", 1)[-1]
    name = name.split(".", 1)[0] if not name.startswith(".") else name
    name = name.split("+", 1)[0]
    name = re.sub(r"Tests?$", "", name) or name
    return name


def classify(path: str, areas: list[Area]) -> Area | None:
    s = stem(path)
    for a in areas:
        if any(p.search(path) for p in a.paths) or any(n.fullmatch(s) for n in a.names):
            return a
    for a in areas:
        if any(p.search(path) for p in a.fallback_paths) or any(n.fullmatch(s) for n in a.fallback_names):
            return a
    return None


# --------------------------------------------------------------------------- state & git


def load_state() -> dict:
    with open(STATE_FILE, "rb") as f:
        return tomllib.load(f)


def save_reviewed(upstream: str, sha: str) -> None:
    """Rewrite only the `reviewed = "..."` line of one upstream, keeping comments."""
    text = STATE_FILE.read_text()
    pattern = re.compile(
        rf'(\[upstream\.{re.escape(upstream)}\][^\[]*?\n\s*reviewed\s*=\s*")[0-9a-f]*(")', re.S
    )
    new, n = pattern.subn(rf"\g<1>{sha}\g<2>", text)
    if n != 1:
        sys.exit(f"Could not find reviewed = \"...\" for [upstream.{upstream}] in {STATE_FILE}")
    new = re.sub(
        rf'(\[upstream\.{re.escape(upstream)}\][^\[]*?\n\s*reviewed_on\s*=\s*")[^"]*(")',
        rf"\g<1>{date.today().isoformat()}\g<2>",
        new,
        flags=re.S,
    )
    STATE_FILE.write_text(new)


def git(repo: Path, *args: str) -> str:
    r = subprocess.run(["git", "-c", "core.quotepath=off", "-C", str(repo), *args], capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f"git {' '.join(args)} failed in {repo}:\n{r.stderr.strip()}")
    return r.stdout


def upstream_info(name: str) -> dict:
    state = load_state()
    ups = state.get("upstream", {})
    if name not in ups:
        sys.exit(f"Unknown upstream {name!r}. Known: {', '.join(ups)}")
    return ups[name]


def repo_path(name: str, override: str | None) -> Path:
    return Path(override) if override else CACHE_DIR / name


def ref_name(info: dict) -> str:
    return f"origin/{info['branch']}"


def cmd_fetch(args) -> None:
    state = load_state()
    names = [args.upstream] if args.upstream else list(state["upstream"])
    CACHE_DIR.mkdir(exist_ok=True)
    for name in names:
        info = state["upstream"][name]
        path = CACHE_DIR / name
        if (path / ".git").exists():
            print(f"Updating {name} ...")
            git(path, "fetch", "--quiet", "origin", info["branch"])
        else:
            # Full history, but file contents only for the latest version (fetched in one batch by
            # the checkout); older contents download on demand if ever needed.
            print(f"Cloning {name} ...")
            r = subprocess.run(
                ["git", "clone", "--quiet", "--filter=blob:none", "--no-checkout",
                 "--branch", info["branch"], info["url"], str(path)],
                capture_output=True, text=True,
            )
            if r.returncode != 0:
                sys.exit(r.stderr)
        git(path, "checkout", "--quiet", "--detach", ref_name(info))
        head = git(path, "rev-parse", ref_name(info)).strip()
        print(f"  {name}: {info['branch']} is at {head[:10]}  ({git(path, 'log', '-1', '--format=%cs %s', head).strip()})")


# --------------------------------------------------------------------------- report


@dataclass
class Change:
    sha: str
    date: str
    subject: str
    files: dict[str, tuple[int, int, str]]  # path -> (added, deleted, status)


def read_changes(repo: Path, since: str, until: str) -> list[Change]:
    fmt = "\x1e%H\x1f%cs\x1f%s"
    out = git(repo, "log", "--no-merges", "--no-renames", "--reverse", f"--format={fmt}", "--numstat",
              f"{since}..{until}")
    status_out = git(repo, "log", "--no-merges", "--no-renames", "--reverse", f"--format={fmt}", "--name-status",
                     f"{since}..{until}")
    statuses: dict[str, dict[str, str]] = {}
    for block in status_out.split("\x1e")[1:]:
        head, _, body = block.partition("\n")
        sha = head.split("\x1f")[0]
        st = {}
        for line in body.splitlines():
            if "\t" in line:
                code, p = line.split("\t", 1)
                st[unquote(p)] = code[0]
        statuses[sha] = st
    changes = []
    for block in out.split("\x1e")[1:]:
        head, _, body = block.partition("\n")
        sha, d, subject = head.split("\x1f", 2)
        files = {}
        for line in body.splitlines():
            parts = line.split("\t")
            if len(parts) == 3:
                a, dl, p = parts
                p = unquote(p)
                files[p] = (int(a) if a.isdigit() else 0, int(dl) if dl.isdigit() else 0,
                            statuses.get(sha, {}).get(p, "M"))
        changes.append(Change(sha, d, subject, files))
    return changes


def unquote(path: str) -> str:
    """git prints unusual file names as "C-style quoted" strings; turn them back into the real name."""
    if len(path) >= 2 and path[0] == path[-1] == '"':
        raw = path[1:-1].encode("latin-1", "backslashreplace").decode("unicode_escape")
        try:
            return raw.encode("latin-1").decode("utf-8")
        except (UnicodeEncodeError, UnicodeDecodeError):
            return raw
    return path


def short(sha: str) -> str:
    return sha[:8]


def commit_link(info: dict, sha: str) -> str:
    base = info["url"].removesuffix(".git")
    return f"[`{short(sha)}`]({base}/commit/{sha[:12]})"


def file_link(info: dict, sha: str, path: str, exists: bool = True) -> str:
    base = info["url"].removesuffix(".git")
    if not exists or any(c in path for c in "\r\n\"`|"):
        return f"`{path.encode('unicode_escape').decode()}`"
    return f"[`{path}`]({base}/blob/{sha[:12]}/{path.replace(' ', '%20')})"


def build_report(name: str, info: dict, repo: Path, since: str, until_sha: str, areas: list[Area],
                 files_limit: int = 40, full_note: str = "") -> tuple[str, int]:
    changes = read_changes(repo, since, until_sha)
    if not changes:
        return (f"## {info['label']}: no new changes\n\nNothing new since {commit_link(info, since)}.\n", 0)

    by_area: dict[str, dict] = defaultdict(lambda: {"commits": {}, "files": defaultdict(lambda: [0, 0, set()])})
    unmapped: dict[str, list] = defaultdict(lambda: [0, 0, set()])
    area_by_id = {a.id: a for a in areas}
    for ch in changes:
        for path, (add, dele, st) in ch.files.items():
            area = classify(path, areas)
            if area is None:
                u = unmapped[path]
                u[0] += add; u[1] += dele; u[2].add(st)
                continue
            slot = by_area[area.id]
            slot["commits"][ch.sha] = ch
            f = slot["files"][path]
            f[0] += add; f[1] += dele; f[2].add(st)

    present = {f for f in git(repo, "ls-tree", "-r", "-z", "--name-only", until_sha).split("\0") if f}
    gone_unmapped = {p: v for p, v in unmapped.items() if p not in present}
    unmapped = {p: v for p, v in unmapped.items() if p in present}

    first, last = changes[0], changes[-1]
    lines = [
        f"## {info['label']}: {len(changes)} new commit{'s' if len(changes) != 1 else ''}",
        "",
        f"From {commit_link(info, since)} (last reviewed) to {commit_link(info, until_sha)}, "
        f"{first.date} → {last.date}. Branch `{info['branch']}`.",
        "",
    ]

    def files_table(slot, limit=files_limit):
        rows = sorted(slot["files"].items(), key=lambda kv: -(kv[1][0] + kv[1][1]))
        out = ["| File | Change | Lines |", "|---|---|---|"]
        for p, (a, d, st) in rows[:limit]:
            kind = "deleted" if p not in present else "new" if "A" in st and "D" not in st and "M" not in st else "changed"
            out.append(f"| {file_link(info, until_sha, p, p in present)} | {kind} | +{a} −{d} |")
        if len(rows) > limit:
            out.append(f"| … and {len(rows) - limit} more | | |")
        return out

    def commits_list(slot):
        return [f"- {commit_link(info, c.sha)} {c.date}: {c.subject}"
                for c in sorted(slot["commits"].values(), key=lambda c: c.date)]

    active = [a for a in areas if a.id in by_area and a.active]
    waiting = [a for a in areas if a.id in by_area and not a.active and a.planned]
    ignored = [a for a in areas if a.id in by_area and not a.planned]

    lines += ["### 1. Review: areas RDicom already has", ""]
    if not active:
        lines += ["None of the changes touch areas RDicom has started. Nothing to port right now.", ""]
    for a in sorted(active, key=lambda a: (a.priority, a.id)):
        slot = by_area[a.id]
        tot_a = sum(v[0] for v in slot["files"].values()); tot_d = sum(v[1] for v in slot["files"].values())
        lines += [
            f"#### {a.name}",
            f"`{a.id}` · {PRIORITY_LABEL.get(a.priority)} · {DECISION_LABEL[a.decision]} · RDicom: {a.rdicom or '—'} · "
            f"{len(slot['files'])} files, +{tot_a} −{tot_d}",
            "",
            *[f"- [ ] {l[2:]}" for l in commits_list(slot)],
            "",
            *(["<details><summary>Files</summary>", "", *files_table(slot), "", "</details>", ""]
              if files_limit > 0 else [f"{len(slot['files'])} files changed (see the full report).", ""]),
        ]

    lines += ["### 2. Noted: planned areas RDicom hasn't started", "",
              "No action now. When an area is started, port from upstream's latest version, which includes these.", ""]
    if not waiting:
        lines += ["None.", ""]
    else:
        lines += ["| Area | Priority | Files | Lines | Commits |", "|---|---|---|---|---|"]
        for a in sorted(waiting, key=lambda a: (a.priority, a.id)):
            slot = by_area[a.id]
            lines.append(f"| {a.name} (`{a.id}`) | {PRIORITY_LABEL.get(a.priority)} | {len(slot['files'])} | "
                         f"+{sum(v[0] for v in slot['files'].values())} −{sum(v[1] for v in slot['files'].values())} | "
                         f"{len(slot['commits'])} |")
        lines.append("")

    lines += ["### 3. Ignored: not planned or not app code", ""]
    if ignored:
        lines.append(", ".join(f"{a.name} (`{a.id}`): {len(by_area[a.id]['files'])} files" for a in ignored))
    else:
        lines.append("None.")
    lines.append("")

    if unmapped:
        lines += [
            "### ⚠ New files that the porting map doesn't cover", "",
            "Add a pattern for each to `porting/map.toml` (or check `coverage`) so future changes are sorted.", "",
            "| File | Change | Lines |", "|---|---|---|",
        ]
        for p, (a, d, st) in sorted(unmapped.items()):
            kind = "new" if st == {"A"} else "deleted" if st == {"D"} else "changed"
            lines.append(f"| {file_link(info, until_sha, p)} | {kind} | +{a} −{d} |")
        lines.append("")
    if gone_unmapped:
        lines += [f"{len(gone_unmapped)} other changed file(s) outside the map no longer exist upstream "
                  "(added and removed again, or deleted); nothing to do.", ""]

    if full_note and files_limit < 40:
        lines += [f"*File lists shortened to fit. {full_note}*", ""]
    lines += [
        "### When you've reviewed this", "",
        "1. Port what matters from section 1 (and tick it here).",
        f"2. Record the review: `python3 tools/upstream.py mark-reviewed {name} {short(until_sha)}` and commit "
        "`porting/upstream.toml`.",
        "",
    ]
    return "\n".join(lines), len(changes)


def resolve_until(repo: Path, info: dict, until: str | None) -> str:
    ref = until or ref_name(info)
    return git(repo, "rev-parse", "--verify", f"{ref}^{{commit}}").strip()


def cmd_report(args) -> None:
    areas = load_map()
    info = upstream_info(args.upstream)
    repo = repo_path(args.upstream, args.repo)
    if not (repo / ".git").exists() and not (repo / "HEAD").exists():
        sys.exit(f"No clone at {repo}. Run: python3 tools/upstream.py fetch {args.upstream}")
    since = args.since or info["reviewed"]
    since = git(repo, "rev-parse", "--verify", f"{since}^{{commit}}").strip()
    until_sha = resolve_until(repo, info, args.until)
    text, n = build_report(args.upstream, info, repo, since, until_sha, areas)
    if args.full_out:
        Path(args.full_out).write_text(text)
    if args.max_chars:
        for limit in (15, 5, 0):
            if len(text) <= args.max_chars:
                break
            text, n = build_report(args.upstream, info, repo, since, until_sha, areas, limit, args.full_note or "")
        if len(text) > args.max_chars:
            cut = text.rfind("\n", 0, args.max_chars - 200)
            text = text[:cut] + "\n\n*Report cut short to fit. " + (args.full_note or "") + "*\n"
    if args.out:
        Path(args.out).write_text(text)
        print(f"{n} new commit(s). Report written to {args.out}")
    else:
        print(text)
    if args.github_output:
        with open(args.github_output, "a") as f:
            f.write(f"commits={n}\nuntil={short(until_sha)}\n")


def cmd_mark_reviewed(args) -> None:
    info = upstream_info(args.upstream)
    repo = repo_path(args.upstream, args.repo)
    sha = resolve_until(repo, info, args.sha)
    save_reviewed(args.upstream, sha)
    print(f"{args.upstream}: reviewed up to {short(sha)} ({git(repo, 'log', '-1', '--format=%cs %s', sha).strip()})")


# --------------------------------------------------------------------------- coverage & map doc


def blobs_available(repo: Path, ref: str) -> bool:
    """True if file contents for `ref` are present locally (no slow on-demand downloads)."""
    r = subprocess.run(["git", "-C", str(repo), "rev-list", "--objects", "--missing=print", ref, "--no-walk"],
                       capture_output=True, text=True)
    return r.returncode == 0 and not any(l.startswith("?") for l in r.stdout.splitlines())


def file_lines(repo: Path, ref: str) -> dict[str, int] | None:
    """Lines per text file at `ref`, or None if the contents aren't downloaded."""
    if not blobs_available(repo, ref):
        return None
    r = subprocess.run(["git", "-c", "core.quotepath=off", "-C", str(repo), "grep", "-I", "-c", "", ref, "--", "."],
                       capture_output=True, text=True)
    counts = {}
    prefix = f"{ref}:"
    for line in r.stdout.splitlines():
        if line.startswith(prefix):
            path, _, n = line[len(prefix):].rpartition(":")
            counts[path] = int(n) if n.isdigit() else 0
    return counts


def tally(repo: Path, ref: str, areas: list[Area]):
    files = [f for f in git(repo, "ls-tree", "-r", "-z", "--name-only", ref).split("\0") if f]
    lines = file_lines(repo, ref)
    have_lines = lines is not None
    lines = lines or {}
    stats = {a.id: {"files": 0, "source": 0, "lines": 0 if have_lines else None} for a in areas}
    unmapped = []
    for p in files:
        a = classify(p, areas)
        if a is None:
            unmapped.append(p)
            continue
        s = stats[a.id]
        s["files"] += 1
        if SOURCE_EXT.search(p):
            s["source"] += 1
            if have_lines:
                s["lines"] += lines.get(p, 0)
    return stats, unmapped, len(files)


def cmd_coverage(args) -> None:
    areas = load_map()
    info = upstream_info(args.upstream)
    repo = repo_path(args.upstream, args.repo)
    ref = resolve_until(repo, info, args.ref)
    stats, unmapped, total = tally(repo, ref, areas)
    print(f"{info['label']} at {short(ref)}: {total} files\n")
    print(f"{'area':28} {'decision':10} {'P':>2} {'files':>6} {'source':>6} {'lines':>8}")
    for a in sorted(areas, key=lambda a: (a.priority == 0, a.priority, a.id)):
        s = stats[a.id]
        print(f"{a.id:28} {a.decision:10} {a.priority:>2} {s['files']:>6} {s['source']:>6} {s['lines'] if s['lines'] is not None else '?':>8}")
    print()
    src_unmapped = [p for p in unmapped if SOURCE_EXT.search(p)]
    if unmapped:
        print(f"{len(unmapped)} file(s) match no area ({len(src_unmapped)} of them source code):")
        for p in unmapped[:200]:
            print("  " + p)
        if len(unmapped) > 200:
            print(f"  … {len(unmapped) - 200} more")
    else:
        print("Every file is assigned to an area.")
    if args.strict and unmapped:
        sys.exit(1)


def cmd_render_map(args) -> None:
    areas = load_map()
    state = load_state()
    stats = None
    ref_note = ""
    if args.upstream:
        info = state["upstream"][args.upstream]
        repo = repo_path(args.upstream, args.repo)
        ref = resolve_until(repo, info, None)
        stats, unmapped, total = tally(repo, ref, areas)
        ref_note = (f"Sizes counted on {info['label']} {commit_link(info, ref)} "
                    f"({git(repo, 'log', '-1', '--format=%cs', ref).strip()}): {total} files"
                    f"{f', {len(unmapped)} not yet assigned' if unmapped else ', all assigned to an area'}.")

    out = [
        "# Porting map: IsiX / Horos → RDicom",
        "",
        "Generated from `porting/map.toml` by `python3 tools/upstream.py render-map isix`. "
        "Edit the TOML, not this file.",
        "",
    ]
    if ref_note:
        out += [ref_note, ""]
    out += [
        "**Decisions:** *Port* = translate the logic to Rust, with its tests. *Rebuild* = recreate the screen or "
        "behaviour to match Horos. *Replace* = use an existing Rust library. *Reuse* = copy the data files. "
        "*Skip* = not planned.",
        "",
    ]

    def row(a: Area) -> str:
        s = stats[a.id] if stats else None
        if not s:
            size = "—"
        elif s["source"] == 0:
            size = f"{s['files']} data files"
        else:
            size = f"{s['source']} / {s['lines']:,}" if s["lines"] is not None else f"{s['source']} / ?"
        return (f"| **{a.name}**<br>`{a.id}` | {DECISION_LABEL[a.decision]} | {PROGRESS_LABEL.get(a.progress, a.progress)} | "
                f"{size} | {a.rdicom or '—'} | {a.crates or '—'} | {a.notes or ''} |")

    if stats and all(v["lines"] is not None for v in stats.values()):
        out += ["## Size by priority", "", "| Priority | Areas | Source files | Lines of IsiX code |", "|---|---|---|---|"]
        for p in (1, 2, 3, 4):
            group = [a for a in areas if a.priority == p and a.planned]
            out.append(f"| {PRIORITY_LABEL[p]} | {len(group)} | {sum(stats[a.id]['source'] for a in group):,} | "
                       f"{sum(stats[a.id]['lines'] for a in group):,} |")
        rest_ids = [a.id for a in areas if not a.planned]
        out += [f"| Not ported | {len(rest_ids)} | {sum(stats[i]['source'] for i in rest_ids):,} | "
                f"{sum(stats[i]['lines'] for i in rest_ids):,} |", "",
                "Lines count all IsiX code in the area, including headers and tests; Rust versions are usually shorter, "
                "and *Replace* areas mostly use existing libraries.", ""]

    header = ["| Area | Decision | RDicom | Source files / lines | In RDicom | Rust crates | Notes |",
              "|---|---|---|---|---|---|---|"]
    for p in (1, 2, 3, 4):
        group = [a for a in areas if a.priority == p and a.planned]
        if not group:
            continue
        out += [f"## {PRIORITY_LABEL[p]}", "", *header, *[row(a) for a in group], ""]
    rest = [a for a in areas if not a.planned]
    out += ["## Not ported", "", *header, *[row(a) for a in rest], ""]

    out += ["## Upstreams tracked", "", "| Upstream | Branch | Reviewed up to | Reviewed on |", "|---|---|---|---|"]
    for name, info in state["upstream"].items():
        out.append(f"| {info['label']} ({info['url']}) | `{info['branch']}` | {commit_link(info, info['reviewed'])} | "
                   f"{info.get('reviewed_on', '—')} |")
    out.append("")
    MAP_DOC.write_text("\n".join(out))
    print(f"Wrote {MAP_DOC.relative_to(ROOT)}")


# --------------------------------------------------------------------------- main


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("fetch", help="clone or update upstream repos into .upstream/")
    p.add_argument("upstream", nargs="?")
    p.set_defaults(func=cmd_fetch)

    p = sub.add_parser("report", help="changes since the last review, grouped by area")
    p.add_argument("upstream")
    p.add_argument("--since", help="start commit (default: last reviewed)")
    p.add_argument("--until", help="end commit (default: upstream branch head)")
    p.add_argument("--repo", help="path to an existing clone (default: .upstream/<name>)")
    p.add_argument("--out", help="write the report to this file")
    p.add_argument("--max-chars", type=int, help="shorten file lists until the report fits (GitHub issues: 65536)")
    p.add_argument("--full-out", help="also write the unshortened report to this file")
    p.add_argument("--full-note", help="text pointing to the full report, used when shortened")
    p.add_argument("--github-output", help=argparse.SUPPRESS)
    p.set_defaults(func=cmd_report)

    p = sub.add_parser("mark-reviewed", help="record the review point")
    p.add_argument("upstream")
    p.add_argument("sha", nargs="?", help="commit reviewed up to (default: upstream branch head)")
    p.add_argument("--repo")
    p.set_defaults(func=cmd_mark_reviewed)

    p = sub.add_parser("coverage", help="check every upstream file is assigned to an area")
    p.add_argument("upstream")
    p.add_argument("--ref")
    p.add_argument("--repo")
    p.add_argument("--strict", action="store_true", help="exit with an error if any file is unassigned")
    p.set_defaults(func=cmd_coverage)

    p = sub.add_parser("render-map", help="regenerate PORTING_MAP.md")
    p.add_argument("upstream", nargs="?", help="count file sizes from this upstream's clone")
    p.add_argument("--repo")
    p.set_defaults(func=cmd_render_map)

    args = ap.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
