#!/usr/bin/env python3
"""Runs a test program in the release VM several times; prints min and median wall time.
With --base BIN, runs alternate between BIN and the release VM and both are printed.

Usage: scripts/bench.py [--base BIN] NAME [RUNS] [ARGS...]
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
name = argv[0]
runs = int(argv[1]) if len(argv) > 1 else 10
args = argv[2:]
env = dict(os.environ)
env.setdefault("SCIVOLO_LIBS", os.path.join(root, "ref/libs"))
env.setdefault("SCIVOLO_CACHE", os.path.join(root, "tmp/cache"))
binaries = {"": os.path.join(root, "target/release/scivolo")}
if base:
    binaries = {"base ": base, "new  ": binaries[""]}
times = {label: [] for label in binaries}
for i in range(runs + 1):
    for label, binary in binaries.items():
        cmd = [binary, "--cp", os.path.join(root, "tests/programs", name), name, *args]
        start = time.perf_counter()
        subprocess.run(cmd, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)
        if i > 0:
            times[label].append(time.perf_counter() - start)
for label, ts in times.items():
    print(f"{label}{name}: min {min(ts) * 1000:.1f} ms, median {statistics.median(ts) * 1000:.1f} ms ({runs} runs)")
