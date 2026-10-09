"""The test programs in the release VM against their `.check` files.

Usage: scripts/program_runs.py check [NAME...]   run the programs (default: all), fail on a difference
"""
import hashlib
import os
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PROGRAMS = ROOT / "tests/programs"
SCALAI = ROOT / "target/release/scalai"
TIMEOUT = int(os.environ.get("SCALAI_TIMEOUT", "60"))


class Failed(Exception):
    pass


def all_names():
    return sorted(p.name for p in PROGRAMS.iterdir() if p.is_dir())


def run(name):
    """NAME in the release VM: ok, differs or timeout against its `.check`, and a hash of its output."""
    check = PROGRAMS / name / f"{name}.check"
    if not check.exists():
        raise Failed(f"no {check.relative_to(ROOT)}")
    env = dict(os.environ, SCALAI_TIMEOUT=str(TIMEOUT))
    try:
        r = subprocess.run([str(SCALAI), "--cp", str(PROGRAMS / name), name], cwd=ROOT, env=env, text=True,
                           errors="replace", stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=TIMEOUT + 10)
    except subprocess.TimeoutExpired:
        return "timeout", ""
    out = r.stdout.rstrip("\n")
    digest = hashlib.sha1(out.encode(errors="replace")).hexdigest()[:12]
    return "ok" if out == check.read_text(errors="replace").rstrip("\n") else "differs", digest


def check(names):
    failed = False
    for name in names or all_names():
        start = time.monotonic()
        try:
            outcome, _ = run(name)
        except Failed as e:
            outcome = str(e)
        took = f"{time.monotonic() - start:.2f}s"
        print(f"ok   {name} {took}" if outcome == "ok" else f"DIFF {name} {took} {outcome}", flush=True)
        failed |= outcome != "ok"
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    if len(sys.argv) >= 2 and sys.argv[1] == "check":
        check(sys.argv[2:])
    else:
        sys.exit(__doc__)
