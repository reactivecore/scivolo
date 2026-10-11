"""dotty's typed trees: recording them for the corpora and the golden tests, and their feature counts."""
import collections
import subprocess
import sys
from pathlib import Path

from typer_corpus import ROOT, USING, WORK, feature_stats, has_errors, load_expected, selected, split, write_list


def run_oracle(lst, mode=()):
    """The oracle's output for the files of the list `lst`, with the oracle's options `mode`."""
    cmd = ["scala-cli", "--power", "run", "--server=false", "--quiet", "--workspace", str(WORK / "oracle-ws"),
           str(ROOT / "scripts/typer-oracle"), "--", *mode, "--list", str(lst)]
    out = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8", errors="replace")
    if out.returncode != 0:
        sys.exit(out.stderr)
    return out.stdout


def oracle(filter_):
    rows = [(rel, opts) for rel, opts, reason in selected(filter_) if reason is None]
    stdout = run_oracle(write_list("list.tsv", rows))
    expected = load_expected() | split(stdout)
    (WORK / "expected.txt").write_text("".join(f"== file {ROOT / p}\n{b}" for p, b in sorted(expected.items())))
    print(f"{len(split(stdout))} files")


def checks(directory):
    """Write dotty's typed trees for each DIR/*.scala to DIR/*.check (golden tests); for a DIR named
    `inlined`, its `Inlined` trees after the `inlining` phase."""
    files = sorted(Path(directory).resolve().glob("*.scala"))
    rows = [(str(f.relative_to(ROOT)), " ".join(o for u in USING.findall(f.read_text()) for o in u.split()))
            for f in files]
    mode = ["--inlined"] if Path(directory).name == "inlined" else []
    for rel, body in split(run_oracle(write_list("checks.tsv", rows), mode)).items():
        body = "".join(l + "\n" for l in body.splitlines() if not l.startswith("== stats"))
        (ROOT / rel).with_suffix(".check").write_text(body, encoding="utf-8")
    print(f"{len(files)} checks")


def stats(_):
    """Feature counts over the files dotty typed without errors: total, and files using each."""
    totals, files = collections.Counter(), collections.Counter()
    for body in load_expected().values():
        counts = feature_stats(body)
        if counts is not None and not has_errors(body):
            for k, v in counts.items():
                totals[k] += v
                files[k] += 1
    for k in sorted(totals):
        print(f"{k:24} {totals[k]:8} {files[k]:6}")
