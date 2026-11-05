#!/usr/bin/env python3
"""Print a Markdown table of the Rust crates compiled into PhoneMirror for
Windows, with their licences (from `cargo metadata`). Used to keep
THIRD_PARTY_LICENSES.md current: python3 scripts/rust-licenses.py"""
import json, subprocess, sys

target = sys.argv[1] if len(sys.argv) > 1 else "x86_64-pc-windows-msvc"
meta = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--format-version", "1", "--locked", "--filter-platform", target]))
pkgs = {p["id"]: p for p in meta["packages"]}
nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
root = next(p["id"] for p in meta["packages"] if p["name"] == "phonemirror")

# Normal (runtime) dependencies only; build scripts and dev-deps aren't shipped.
seen, stack = set(), [root]
while stack:
    i = stack.pop()
    if i in seen:
        continue
    seen.add(i)
    for d in nodes[i]["deps"]:
        if any(k["kind"] is None for k in d["dep_kinds"]):
            stack.append(d["pkg"])

rows = sorted((pkgs[i]["name"], pkgs[i]["version"], pkgs[i].get("license") or "see crate")
              for i in seen if pkgs[i]["source"])
print(f"| Crate | Version | Licence |\n|---|---|---|")
for n, v, l in rows:
    print(f"| {n} | {v} | {l} |")
print(f"\n{len(rows)} crates.", file=sys.stderr)
