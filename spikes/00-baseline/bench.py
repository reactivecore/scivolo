#!/usr/bin/env python3
"""Minimal hyperfine substitute (hyperfine is not installed on the spike machine).

usage: bench.py --name NAME [--runs N] [--warmup W] [--prepare CMD] [--csv FILE] -- CMD

- CMD and --prepare are run via /bin/sh -c.
- --prepare runs before *every* timed run (and warmup) and is not timed
  (same semantics as hyperfine --prepare).
- stdout/stderr of CMD are discarded; a non-zero exit aborts.
- Prints mean / stddev / min / median / max in milliseconds and appends a CSV row.
"""
import argparse, statistics, subprocess, sys, time

ap = argparse.ArgumentParser()
ap.add_argument("--name", required=True)
ap.add_argument("--runs", type=int, default=10)
ap.add_argument("--warmup", type=int, default=1)
ap.add_argument("--prepare")
ap.add_argument("--csv")
ap.add_argument("cmd", nargs=argparse.REMAINDER)
a = ap.parse_args()
cmd = " ".join(a.cmd[1:] if a.cmd and a.cmd[0] == "--" else a.cmd)


def once():
    if a.prepare:
        subprocess.run(a.prepare, shell=True, check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    t0 = time.perf_counter()
    r = subprocess.run(cmd, shell=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    dt = (time.perf_counter() - t0) * 1000
    if r.returncode != 0:
        sys.stderr.write(r.stderr.decode(errors="replace"))
        sys.exit(f"command failed ({r.returncode}): {cmd}")
    return dt


for _ in range(a.warmup):
    once()
ts = [once() for _ in range(a.runs)]
mean = statistics.mean(ts)
sd = statistics.stdev(ts) if len(ts) > 1 else 0.0
row = (a.name, len(ts), mean, sd, min(ts), statistics.median(ts), max(ts))
print(f"{a.name:<48} n={len(ts):<3} mean={mean:9.1f} ms  sd={sd:7.1f}  "
      f"min={min(ts):9.1f}  med={statistics.median(ts):9.1f}  max={max(ts):9.1f}", flush=True)
if a.csv:
    with open(a.csv, "a") as f:
        f.write("%s,%d,%.1f,%.1f,%.1f,%.1f,%.1f\n" % row)
