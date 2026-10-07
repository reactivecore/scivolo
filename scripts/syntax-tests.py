#!/usr/bin/env python3
"""The Rust scanner, parser or desugaring against dotty 3.9.0 on tests/pos and tests/neg
(docs/research/06-parser-port.md, 07-desugar-port.md).

Usage: scripts/syntax-tests.py MODE oracle|report [FILTER] | MODE diff PATH | MODE checks DIR,
MODE = tokens|trees|desugar
"""
import collections
import difflib
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TESTS = ROOT / "ref/dotty/tests"
CORPORA = ("pos", "neg")
WORK = ROOT / "tmp/syntax/3.9.0"
MODE = "tokens"
DUMP = ROOT / "target/release/syntax-dump"
OUTCOMES = ["passed", "differ", "errors differ", "messages differ", "warnings differ", "crashed", "skipped"]
USING = re.compile(r"^//> using options (.*)$", re.M)


def tests(filter_):
    """(path relative to tests/, options, skip reason or None) for each Scala file."""
    if not TESTS.is_dir():
        sys.exit(f"{TESTS} missing: run `just syntax-fetch`")
    for corpus in CORPORA:
        for top in sorted((TESTS / corpus).iterdir()):
            files = sorted(top.rglob("*.scala")) if top.is_dir() else [top] if top.suffix == ".scala" else []
            options = [o for f in files for u in USING.findall(f.read_text(errors="replace")) for o in u.split()]
            reason = "magic offset header" if any(o.startswith("-Ymagic-offset-header") for o in options) else None
            for f in files:
                rel = str(f.relative_to(TESTS))
                if filter_ in rel:
                    yield rel, " ".join(options), reason


def write_list(filter_):
    rows = [(rel, opts) for rel, opts, reason in tests(filter_) if reason is None]
    path = WORK / "list.tsv"
    path.write_text("".join(f"{TESTS / rel}\t{opts}\n" for rel, opts in rows))
    return path


def split(output):
    """{path relative to tests/: record text} from dump or oracle output, recorded in any checkout."""
    records = {}
    for chunk in output.split("== file ")[1:]:
        path, _, body = chunk.partition("\n")
        records[path.split("ref/dotty/tests/", 1)[-1]] = body
    return records


def oracle(filter_):
    lst = write_list(filter_)
    cmd = ["scala-cli", "--power", "run", "--server=false", "--quiet", "--workspace", str(WORK / "oracle-ws"),
           str(ROOT / "scripts/syntax-oracle"), "--", MODE, "--list", str(lst)]
    out = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8", errors="replace")
    if out.returncode != 0:
        sys.exit(out.stderr)
    expected = load_expected() | split(out.stdout)
    (WORK / "expected.txt").write_text("".join(f"== file {TESTS / p}\n{b}" for p, b in sorted(expected.items())))
    print(f"{len(split(out.stdout))} files")


def load_expected():
    path = WORK / "expected.txt"
    return split(path.read_text(encoding="utf-8")) if path.exists() else {}


def classify(expected, actual):
    if actual.startswith("== crash"):
        return "crashed", actual.splitlines()[0]
    exp, act = expected.splitlines(), actual.splitlines()
    tokens = lambda ls: [l for l in ls if not l.startswith("== ")]
    errors = lambda ls: [l for l in ls if l.startswith("== E ")]
    warnings = lambda ls: [l for l in ls if l.startswith("== W ")]
    for outcome, a, b in [("differ", tokens(exp), tokens(act)), ("messages differ", errors(exp), errors(act)),
                          ("warnings differ", warnings(exp), warnings(act))]:
        if a != b:
            at = next((i for i, (x, y) in enumerate(zip(a, b)) if x != y), min(len(a), len(b)))
            if outcome == "messages differ" and [l.split()[2] for l in a] != [l.split()[2] for l in b]:
                outcome = "errors differ"
            detail = f"line {at + 1}: {a[at] if at < len(a) else 'end'} | {b[at] if at < len(b) else 'end'}"
            return outcome, detail
    return "passed", ""


def report(filter_):
    lst = write_list(filter_)
    expected = load_expected()
    out = subprocess.run([str(DUMP), MODE, "--list", str(lst)], capture_output=True, text=True,
                         encoding="utf-8", errors="replace")
    actual = split(out.stdout)
    rows = {}
    for rel, _, reason in tests(filter_):
        if reason:
            rows[rel] = ("skipped", reason)
        elif rel not in expected:
            rows[rel] = ("skipped", "no oracle output: run `just syntax-oracle`")
        else:
            rows[rel] = classify(expected[rel], actual.get(rel, "== crash no output"))
    (WORK / "results.tsv").write_text("".join(f"{n}\t{o}\t{d}\n" for n, (o, d) in sorted(rows.items())))
    table = "| Outcome | " + " | ".join(CORPORA) + " |\n|---|" + "---:|" * len(CORPORA) + "\n"
    counts = collections.Counter((n.split("/")[0], o) for n, (o, _) in rows.items())
    for o in OUTCOMES:
        table += f"| {o} | " + " | ".join(str(counts[c, o]) for c in CORPORA) + " |\n"
    (WORK / "report.md").write_text(table)
    print(table)


def checks(directory):
    """Write dotty's output for each DIR/*.scala to DIR/*.check (golden tests)."""
    files = sorted(Path(directory).resolve().glob("*.scala"))
    lst = WORK / "checks.tsv"
    lst.write_text("".join(f"{f}\t{' '.join(o for u in USING.findall(f.read_text()) for o in u.split())}\n"
                           for f in files))
    cmd = ["scala-cli", "--power", "run", "--server=false", "--quiet", "--workspace", str(WORK / "oracle-ws"),
           str(ROOT / "scripts/syntax-oracle"), "--", MODE, "--list", str(lst)]
    out = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8", errors="replace")
    if out.returncode != 0:
        sys.exit(out.stderr)
    for chunk in out.stdout.split("== file ")[1:]:
        path, _, body = chunk.partition("\n")
        Path(path).with_suffix(".check").write_text(body, encoding="utf-8")
    print(f"{len(files)} checks")


def diff(path):
    rel = str(Path(path).resolve().relative_to(TESTS)) if Path(path).exists() else path
    lst = WORK / "one.tsv"
    opts = next((o for r, o, _ in tests(rel) if r == rel), "")
    lst.write_text(f"{TESTS / rel}\t{opts}\n")
    out = subprocess.run([str(DUMP), MODE, "--list", str(lst)], capture_output=True, text=True)
    actual = split(out.stdout).get(rel, "")
    sys.stdout.writelines(difflib.unified_diff(load_expected().get(rel, "").splitlines(True),
                                               actual.splitlines(True), "dotty", "rust"))


if __name__ == "__main__":
    commands = {"oracle": oracle, "report": report, "diff": diff, "checks": checks}
    if len(sys.argv) < 3 or sys.argv[1] not in ("tokens", "trees", "desugar") or sys.argv[2] not in commands:
        sys.exit(__doc__)
    MODE = sys.argv[1]
    WORK = WORK / MODE
    WORK.mkdir(parents=True, exist_ok=True)
    commands[sys.argv[2]](sys.argv[3] if len(sys.argv) > 3 else "")
