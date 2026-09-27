#!/usr/bin/env python3
"""Runs a test program in the release VM several times; prints min and median wall time.

Usage: scripts/bench.py NAME [RUNS] [ARGS...]
"""
import os
import statistics
import subprocess
import sys
import time

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
name = sys.argv[1]
runs = int(sys.argv[2]) if len(sys.argv) > 2 else 10
args = sys.argv[3:]
env = dict(os.environ)
env.setdefault("SCALAI_LIBS", os.path.join(root, "ref/libs"))
env.setdefault("SCALAI_CACHE", os.path.join(root, "tmp/cache"))
cmd = [os.path.join(root, "target/release/scalai"), "--cp", os.path.join(root, "tests/programs", name), name, *args]
times = []
for _ in range(runs + 1):
    start = time.perf_counter()
    subprocess.run(cmd, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)
    times.append(time.perf_counter() - start)
times = times[1:]
print(f"{name}: min {min(times) * 1000:.1f} ms, median {statistics.median(times) * 1000:.1f} ms ({runs} runs)")
