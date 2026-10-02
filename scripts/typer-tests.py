#!/usr/bin/env python3
"""The Rust typer against dotty 3.9.0's on tests/programs, tests/pos and tests/neg
(docs/research/09-typer.md). A neg file passes when the typer does not accept it: it reports an
error or falls back.

Usage: scripts/typer-tests.py oracle|report [FILTER] | diff PATH | checks DIR | stats
"""
import collections
import difflib
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TESTS = ROOT / "ref/dotty/tests"
WORK = ROOT / "tmp/typer/3.9.0"
DUMP = ROOT / "target/release/typer-dump"
CORPORA = ("programs", "pos", "neg")
OUTCOMES = ["passed", "differ", "warnings differ", "rejected", "accepted", "syntax errors", "unsupported",
            "crashed", "skipped"]
USING = re.compile(r"^//> using options (.*)$", re.M)
SUBSET_EXCLUDED = ("ty_match", "tt_match_type", "quotes", "macro_calls")


def tests(filter_):
    """(path relative to the repository, options, skip reason or None) for each Scala file."""
    for f in sorted(ROOT.glob("tests/programs/*/*.scala")):
        yield f, "", None
    if not TESTS.is_dir():
        sys.exit(f"{TESTS} missing: run `just syntax-fetch`")
    for f in sorted(TESTS.glob("pos/*.scala")) + sorted(TESTS.glob("neg/*.scala")):
        text = f.read_text(errors="replace")
        options = " ".join(o for u in USING.findall(text) for o in u.split())
        reason = None
        if "-Ymagic-offset-header" in options:
            reason = "magic offset header"
        elif re.search(r"^package scala\b", text, re.M):
            reason = "defines package scala"
        elif re.search(r"-language:experimental|-Yexplicit-nulls|-Ycc|captureChecking", options + text):
            reason = "experimental feature"
        yield f, options, reason


def selected(filter_):
    for f, opts, reason in tests(filter_):
        rel = str(f.relative_to(ROOT))
        if filter_ in rel:
            yield rel, opts, reason


def corpus(rel):
    return "programs" if rel.startswith("tests/programs/") else rel.split("/")[3]


def write_list(name, rows):
    path = WORK / name
    path.write_text("".join(f"{ROOT / rel}\t{opts}\n" for rel, opts in rows))
    return path


def split(output):
    """{path relative to the repository: record text} from dump or oracle output."""
    records = {}
    for chunk in output.split("== file ")[1:]:
        path, _, body = chunk.partition("\n")
        records[str(Path(path).relative_to(ROOT))] = body
    return records


def load_expected():
    path = WORK / "expected.txt"
    return split(path.read_text(encoding="utf-8")) if path.exists() else {}


def oracle(filter_):
    rows = [(rel, opts) for rel, opts, reason in selected(filter_) if reason is None]
    lst = write_list("list.tsv", rows)
    cmd = ["scala-cli", "--power", "run", "--server=false", "--quiet", "--workspace", str(WORK / "oracle-ws"),
           str(ROOT / "scripts/typer-oracle"), "--", "--list", str(lst)]
    out = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8", errors="replace")
    if out.returncode != 0:
        sys.exit(out.stderr)
    expected = load_expected() | split(out.stdout)
    (WORK / "expected.txt").write_text("".join(f"== file {ROOT / p}\n{b}" for p, b in sorted(expected.items())))
    print(f"{len(split(out.stdout))} files")


def in_subset(expected):
    """Whether dotty typed the file without errors and without features outside the subset."""
    if "\n== E " in "\n" + expected or expected.startswith("== crash"):
        return False
    stats = re.search(r"^== stats (.*)$", expected, re.M)
    keys = {kv.split("=")[0] for kv in stats.group(1).split()} if stats else set()
    return not keys & set(SUBSET_EXCLUDED)


def has_errors(record):
    return "\n== E " in "\n" + record


def classify(expected, actual):
    if actual.startswith("== crash"):
        return "crashed", actual.splitlines()[0]
    if actual.startswith("== syntax errors"):
        return "syntax errors", ""
    if actual.startswith("== unsupported"):
        return "unsupported", actual.splitlines()[0][len("== unsupported "):]
    if has_errors(actual):
        return "rejected", next(l for l in actual.splitlines() if l.startswith("== E "))
    exp, act = expected.splitlines(), actual.splitlines()
    trees = lambda ls: [l for l in ls if not l.startswith("== ")]
    warnings = lambda ls: [l for l in ls if l.startswith("== W ")]
    for outcome, a, b in [("differ", trees(exp), trees(act)), ("warnings differ", warnings(exp), warnings(act))]:
        if a != b:
            at = next((i for i, (x, y) in enumerate(zip(a, b)) if x != y), min(len(a), len(b)))
            x, y = a[at] if at < len(a) else "end", b[at] if at < len(b) else "end"
            col = next((i for i, (p, q) in enumerate(zip(x, y)) if p != q), min(len(x), len(y)))
            return outcome, f"line {at + 1} col {col}: {x[max(0, col - 40):col + 60]} | {y[max(0, col - 40):col + 60]}"
    return "passed", ""


def classify_neg(actual):
    if actual.startswith("== crash"):
        return "crashed", actual.splitlines()[0]
    if actual.startswith(("== syntax errors", "== unsupported")) or has_errors(actual):
        return "passed", ""
    return "accepted", ""


def report(filter_):
    expected = load_expected()
    rows, runnable = {}, []
    for rel, opts, reason in selected(filter_):
        if reason:
            rows[rel] = ("skipped", reason)
        elif rel not in expected:
            rows[rel] = ("skipped", "no oracle output: run `just typer-oracle`")
        elif corpus(rel) == "neg":
            if has_errors(expected[rel]):
                runnable.append((rel, opts))
            else:
                rows[rel] = ("skipped", "no typer errors")
        elif not in_subset(expected[rel]):
            rows[rel] = ("skipped", "outside the subset")
        else:
            runnable.append((rel, opts))
    lst = write_list("run.tsv", runnable)
    out = subprocess.run([str(DUMP), "--list", str(lst)], capture_output=True, text=True, encoding="utf-8",
                         errors="replace")
    actual = split(out.stdout)
    for rel, _ in runnable:
        got = actual.get(rel, "== crash no output")
        rows[rel] = classify_neg(got) if corpus(rel) == "neg" else classify(expected[rel], got)
    results = WORK / "results.tsv"
    previous = {}
    if results.exists():
        for line in results.read_text().splitlines():
            n, o, _ = (line.split("\t") + ["", ""])[:3]
            previous[n] = o
    results.write_text("".join(f"{n}\t{o}\t{d}\n" for n, (o, d) in sorted(rows.items())))
    changed = [(n, previous[n], o, d) for n, (o, d) in sorted(rows.items()) if n in previous and previous[n] != o]
    for n, before, after, d in changed:
        print(f"{before} -> {after}: {n} {d}")
    table = "| Outcome | " + " | ".join(CORPORA) + " |\n|---|" + "---:|" * len(CORPORA) + "\n"
    counts = collections.Counter((corpus(n), o) for n, (o, _) in rows.items())
    for o in OUTCOMES:
        table += f"| {o} | " + " | ".join(str(counts[c, o]) for c in CORPORA) + " |\n"
    (WORK / "report.md").write_text(table)
    print(table)


def diff(path):
    rel = str(Path(path).resolve().relative_to(ROOT))
    opts = next((o for r, o, _ in selected(rel) if r == rel), "")
    lst = write_list("one.tsv", [(rel, opts)])
    out = subprocess.run([str(DUMP), "--list", str(lst)], capture_output=True, text=True, encoding="utf-8")
    actual = split(out.stdout).get(rel, "")
    expected = load_expected().get(rel, "")
    lines = lambda s: [l + "\n" for l in re.sub(r"(\((?=[A-Z]))", r"\n\1", s).splitlines() if not l.startswith("== stats")]
    sys.stdout.writelines(difflib.unified_diff(lines(expected), lines(actual), "dotty", "rust"))


def checks(directory):
    """Write dotty's typed trees for each DIR/*.scala to DIR/*.check (golden tests)."""
    files = sorted(Path(directory).resolve().glob("*.scala"))
    rows = [(str(f.relative_to(ROOT)), " ".join(o for u in USING.findall(f.read_text()) for o in u.split()))
            for f in files]
    lst = write_list("checks.tsv", rows)
    cmd = ["scala-cli", "--power", "run", "--server=false", "--quiet", "--workspace", str(WORK / "oracle-ws"),
           str(ROOT / "scripts/typer-oracle"), "--", "--list", str(lst)]
    out = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8", errors="replace")
    if out.returncode != 0:
        sys.exit(out.stderr)
    for rel, body in split(out.stdout).items():
        body = "".join(l + "\n" for l in body.splitlines() if not l.startswith("== stats"))
        (ROOT / rel).with_suffix(".check").write_text(body, encoding="utf-8")
    print(f"{len(files)} checks")


def stats(_):
    """Feature counts over the files dotty typed without errors: total, and files using each."""
    totals, files = collections.Counter(), collections.Counter()
    for body in load_expected().values():
        m = re.search(r"^== stats (.*)$", body, re.M)
        if m and "\n== E " not in "\n" + body:
            for kv in m.group(1).split():
                k, v = kv.split("=")
                totals[k] += int(v)
                files[k] += 1
    for k in sorted(totals):
        print(f"{k:24} {totals[k]:8} {files[k]:6}")


if __name__ == "__main__":
    commands = {"oracle": oracle, "report": report, "diff": diff, "checks": checks, "stats": stats}
    if len(sys.argv) < 2 or sys.argv[1] not in commands:
        sys.exit(__doc__)
    WORK.mkdir(parents=True, exist_ok=True)
    commands[sys.argv[1]](sys.argv[2] if len(sys.argv) > 2 else "")
