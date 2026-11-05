#!/usr/bin/env python3
"""License gate for shipped dependencies (LEGAL-001 / LEGAL-014, docs/16 and docs/17).

Lists every Rust crate compiled into the Windows build (normal + build
dependencies, not test-only ones) and every production npm package, then checks
each license against an allowlist. Exits with status 1 if anything is not
allowed, so CI fails before a non-compliant dependency can ship.

Usage:
    python scripts/check-licenses.py            # check only
    python scripts/check-licenses.py --csv docs/third-party/dependencies.csv

This is an engineering control, not legal advice; docs/16 governs.
"""

import csv
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TARGET = "x86_64-pc-windows-msvc"

# Permissive licenses: commercial use allowed; keep the notices.
PERMISSIVE = {
    "MIT", "MIT-0", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Zlib",
    "Unicode-3.0", "Unicode-DFS-2016", "Unlicense", "CC0-1.0", "BSL-1.0", "0BSD",
}
# Weak, file-level copyleft: allowed for unmodified use; obligations are recorded
# in docs/17 (ship notices; point to the source of the MPL files).
NOTICE_REQUIRED = {"MPL-2.0"}
RANK = {"deny": 0, "notice": 1, "ok": 2}


def _leaf(identifier):
    base = identifier.split(" WITH ")[0].strip()
    if base in PERMISSIVE:
        return "ok"
    if base in NOTICE_REQUIRED:
        return "notice"
    return "deny"


def _evaluate(tokens):
    """Recursive descent over SPDX: OR (lowest) > AND > atom/parentheses.
    OR takes the best alternative (the licensee may choose), AND the worst."""
    position = 0

    def atom():
        nonlocal position
        token = tokens[position]
        position += 1
        if token == "(":
            value = either()
            position += 1  # ')'
            return value
        if position < len(tokens) and tokens[position] == "WITH":
            position += 2
        return _leaf(token)

    def both():
        nonlocal position
        value = atom()
        while position < len(tokens) and tokens[position] == "AND":
            position += 1
            right = atom()
            value = value if RANK[value] <= RANK[right] else right
        return value

    def either():
        nonlocal position
        value = both()
        while position < len(tokens) and tokens[position] == "OR":
            position += 1
            right = both()
            value = value if RANK[value] >= RANK[right] else right
        return value

    return either()


def classify(expression):
    """Return (verdict, note) for an SPDX license expression.
    `/` is treated as OR (older Cargo manifests use `MIT/Apache-2.0`)."""
    if not expression or not expression.strip():
        return "deny", "no license declared"
    tokens = re.findall(r"\(|\)|[^\s()/]+", expression.replace("/", " OR "))
    try:
        verdict = _evaluate(tokens)
    except IndexError:
        return "deny", f"unparseable license expression: {expression}"
    notes = {
        "ok": "",
        "notice": "MPL-2.0: ship notice and source location",
        "deny": "no acceptable license option; needs a legal decision (docs/16)",
    }
    return verdict, notes[verdict]


def rust_crates():
    metadata = json.loads(
        subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--filter-platform", TARGET, "--locked"],
            cwd=ROOT / "src-tauri", check=True, capture_output=True, text=True, encoding="utf-8",
        ).stdout
    )
    packages = {p["id"]: p for p in metadata["packages"]}
    nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
    root = metadata["resolve"]["root"]
    seen, stack = set(), [root]
    while stack:
        current = stack.pop()
        if current in seen:
            continue
        seen.add(current)
        for dep in nodes[current]["deps"]:
            if any(kind["kind"] in (None, "build") for kind in dep["dep_kinds"]):
                stack.append(dep["pkg"])
    seen.discard(root)
    for pid in sorted(seen, key=lambda i: (packages[i]["name"], packages[i]["version"])):
        p = packages[pid]
        yield "rust", p["name"], p["version"], p.get("license") or "", p.get("repository") or ""


def npm_packages():
    lock = json.loads((ROOT / "package-lock.json").read_text(encoding="utf-8"))
    for path, info in sorted(lock["packages"].items()):
        if not path.startswith("node_modules/") or info.get("dev") or info.get("devOptional"):
            continue
        name = path.split("node_modules/")[-1]
        yield "npm", name, info.get("version", ""), info.get("license") or "", info.get("resolved") or ""


def main():
    rows = []
    for ecosystem, name, version, license_expr, source in [*rust_crates(), *npm_packages()]:
        verdict, note = classify(license_expr)
        rows.append({"ecosystem": ecosystem, "name": name, "version": version, "license": license_expr,
                     "verdict": verdict, "note": note, "source": source})

    if "--csv" in sys.argv:
        out = ROOT / sys.argv[sys.argv.index("--csv") + 1]
        out.parent.mkdir(parents=True, exist_ok=True)
        with out.open("w", newline="", encoding="utf-8") as handle:
            writer = csv.DictWriter(handle, fieldnames=list(rows[0].keys()))
            writer.writeheader()
            writer.writerows(rows)
        print(f"Wrote {len(rows)} rows to {out.relative_to(ROOT)}")

    counts = {v: sum(r["verdict"] == v for r in rows) for v in ("ok", "notice", "deny")}
    print(f"Shipped dependencies: {len(rows)} ({counts['ok']} permissive, "
          f"{counts['notice']} notice-required, {counts['deny']} denied)")
    for row in rows:
        if row["verdict"] != "ok":
            print(f"  [{row['verdict'].upper()}] {row['ecosystem']} {row['name']} {row['version']}: "
                  f"{row['license'] or '(none)'} - {row['note']}")
    if counts["deny"]:
        print("License gate FAILED: resolve or document a legal decision (docs/16) before merging.")
        return 1
    print("License gate passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
