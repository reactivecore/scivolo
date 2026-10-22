#!/usr/bin/env python3
"""Runs a test program in the release VM several times; prints min and median wall time.
With --base BIN, runs alternate between BIN and the release VM and both are printed; with
--snapshot FILE, between a cold start and one from heap snapshot FILE, written first if missing.

Usage: scripts/bench.py [--base BIN | --snapshot FILE] NAME [RUNS] [ARGS...]
"""
import os
import statistics
import subprocess
import sys
import time

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
argv = sys.argv[1:]
option = None
if argv[:1] in (["--base"], ["--snapshot"]):
    option, argv = argv[:2], argv[2:]
name = argv[0]
runs = int(argv[1]) if len(argv) > 1 else 10
args = argv[2:]
env = dict(os.environ)
env.setdefault("SCIVOLO_LIBS", os.path.join(root, "ref/libs"))
env.setdefault("SCIVOLO_CACHE", os.path.join(root, "tmp/cache"))
release = os.path.join(root, "target/release/scivolo")
binaries = {"": (release, env)}
if option and option[0] == "--base":
    binaries = {"base ": (option[1], env), "new  ": (release, env)}
if option and option[0] == "--snapshot":
    binaries = {"cold ": (release, env), "snap ": (release, dict(env, SCIVOLO_SNAPSHOT=os.path.abspath(option[1])))}
times = {label: [] for label in binaries}
for i in range(runs + 1):
    for label, (binary, run_env) in binaries.items():
        cmd = [binary, "--cp", os.path.join(root, "tests/programs", name), name, *args]
        start = time.perf_counter()
        subprocess.run(cmd, env=run_env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)
        if i > 0:
            times[label].append(time.perf_counter() - start)
for label, ts in times.items():
    print(f"{label}{name}: min {min(ts) * 1000:.1f} ms, median {statistics.median(ts) * 1000:.1f} ms ({runs} runs)")
