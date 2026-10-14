#!/usr/bin/env python3
"""Edit-then-run time of a Scala script with the strict frontend: each run edits a copy under
tmp/bench-edit (a cache miss) and runs it twice, the second a cache hit; prints min and median
wall time of both. With --base BIN, runs alternate between BIN and the release scalai.

Usage: scripts/bench_edit.py [--base BIN] FILE [RUNS]
"""
import os
import statistics
import subprocess
import sys
import time

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
argv = sys.argv[1:]
base = None
if argv[:1] == ["--base"]:
    base, argv = argv[1], argv[2:]
source = argv[0]
runs = int(argv[1]) if len(argv) > 1 else 10
env = dict(os.environ, SCALAI_FRONTEND="strict")
if os.path.isdir(os.path.join(root, "ref/libs")):
    env.setdefault("SCALAI_LIBS", os.path.join(root, "ref/libs"))
env.setdefault("SCALAI_CACHE", os.path.join(root, "tmp/cache"))
binaries = {"": os.path.join(root, "target/release/scalai")}
if base:
    binaries = {"base ": base, "new  ": binaries[""]}
with open(source) as f:
    text = f.read()
work = os.path.join(root, "tmp/bench-edit")
os.makedirs(work, exist_ok=True)
copy = os.path.join(work, os.path.basename(source))
times = {(label, kind): [] for label in binaries for kind in ("miss", "hit ")}
stamp = time.time_ns()
for i in range(runs + 1):
    for label, binary in binaries.items():
        with open(copy, "w") as f:
            f.write(f"{text}\n// edit {stamp} {label.strip()} {i}\n")
        for kind in ("miss", "hit "):
            start = time.perf_counter()
            run = subprocess.run([binary, copy], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
            if run.returncode != 0:
                sys.exit(f"{binary} {copy} failed ({run.returncode}):\n{run.stderr}")
            if i > 0:
                times[(label, kind)].append(time.perf_counter() - start)
name = os.path.basename(source)
for (label, kind), ts in times.items():
    print(f"{label}{name} {kind}: min {min(ts) * 1000:.1f} ms, median {statistics.median(ts) * 1000:.1f} ms ({runs} runs)")
