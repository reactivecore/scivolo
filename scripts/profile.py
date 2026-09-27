#!/usr/bin/env python3
"""Samples a test program running in the release VM with macOS `sample` and prints the
functions most often on top of the stack.

Usage: scripts/profile.py NAME [SECONDS] [ARGS...]; the full report lands in tmp/NAME.sample.
"""
import os
import re
import subprocess
import sys
import time

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
name = sys.argv[1]
seconds = sys.argv[2] if len(sys.argv) > 2 else "2"
args = sys.argv[3:]
env = dict(os.environ)
env.setdefault("SCALAI_LIBS", os.path.join(root, "ref/libs"))
env.setdefault("SCALAI_CACHE", os.path.join(root, "tmp/cache"))
cmd = [os.path.join(root, "target/release/scalai"), "--cp", os.path.join(root, "tests/programs", name), name, *args]
out = os.path.join(root, "tmp", f"{name}.sample")
proc = subprocess.Popen(cmd, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
time.sleep(0.05)
subprocess.run(["sample", str(proc.pid), seconds, "1", "-file", out], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
proc.wait()
text = open(out).read()
top = text.split("Sort by top of stack, same collapsed")[-1]
for line in top.splitlines()[1:30]:
    line = re.sub(r"\s+\(in [^)]*\)", "", line.strip())
    if line:
        print(line)
