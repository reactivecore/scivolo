#!/usr/bin/env python3
"""The Scala Native unit tests passing in one sn-report but not in another (#168).

Usage: scripts/sn-report-compare.py BASE OURS

BASE and OURS are report directories of scripts/sn-report.sh (e.g. tmp/sn-report and
tmp/sn-report-ours); prints each test with a PASS line in a group output of BASE but none in that of
OURS and one summary line, and exits with 1 if there are any or BASE has no passed test.
"""
import sys
from pathlib import Path


def passed(report):
    """The tests with a PASS line in the group outputs of directory `report`."""
    return {l.split()[1] for f in report.glob("*.txt") for l in f.read_text(errors="replace").splitlines()
            if l.startswith("PASS ")}


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    base, ours = passed(Path(sys.argv[1])), passed(Path(sys.argv[2]))
    if not base:
        sys.exit(f"{sys.argv[1]}: no passed test")
    lost = sorted(base - ours)
    for t in lost:
        print(f"no longer passes: {t}")
    print(f"{len(ours)} tests pass in {sys.argv[2]}, {len(base)} in {sys.argv[1]}; {len(lost)} of these not in {sys.argv[2]}")
    sys.exit(1 if lost else 0)
