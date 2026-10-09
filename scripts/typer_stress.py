"""The typer's stress mode: errors taking precedence over unsupported constructs, and its false errors."""
import collections
import concurrent.futures
import os
import subprocess
import sys

from typer_corpus import CORPORA, DUMP, ROOT, WORK, corpus, errors, load_expected, selected, split

STRESS_TIMEOUT = 60


def dump_one(rel, opts):
    """The stress-mode record of one file, or a crash line when it does not finish in time."""
    lst = WORK / "stress" / (rel.replace("/", "_") + ".tsv")
    lst.write_text(f"{ROOT / rel}\t{opts}\n")
    try:
        out = subprocess.run([str(DUMP), "--errors-first", "--list", str(lst)], capture_output=True, text=True,
                             encoding="utf-8", errors="replace", timeout=STRESS_TIMEOUT)
    except subprocess.TimeoutExpired:
        return f"== crash timeout after {STRESS_TIMEOUT} s\n"
    return split(out.stdout).get(rel, "== crash no output\n")


def stress(filter_):
    """Type programs, pos and neg files with errors taking precedence over unsupported constructs: an
    error at an offset where dotty reports none is a false error. Without oracle output, dotty
    reports none in a programs or pos file, and in a neg file whose oracle run crashed, those on the
    lines marked `// error`."""
    expected = load_expected()
    runnable = [(rel, opts) for rel, opts, reason in selected(filter_)
                if reason is None and (rel in expected or corpus(rel) != "neg")]
    (WORK / "stress").mkdir(exist_ok=True)
    with concurrent.futures.ThreadPoolExecutor(os.cpu_count()) as pool:
        records = dict(zip(runnable, pool.map(lambda r: dump_one(*r), runnable)))
    false, crashed, rows = collections.Counter(), [], []
    for (rel, _), got in records.items():
        if got.startswith("== crash"):
            crashed.append(rel)
            rows.append(f"{rel}\tcrashed\t{got.splitlines()[0]}\n")
        oracle = expected.get(rel, "")
        if corpus(rel) == "neg" and oracle.startswith("== crash"):
            text = (ROOT / rel).read_text(errors="replace")
            marked = {i for i, l in enumerate(text.splitlines()) if "// error" in l}
            dotty = lambda at: text.count("\n", 0, at) in marked
        else:
            dotty = errors(oracle).__contains__
        for at, msg in sorted(errors(got).items()):
            if not dotty(at):
                false[corpus(rel)] += 1
                rows.append(f"{rel}\tfalse\t{at} {msg}\n")
                print(f"false error: {rel} @ {at} {msg}")
    (WORK / "stress.tsv").write_text("".join(rows))
    for rel in crashed:
        print(f"crashed: {rel}")
    print(f"{len(records)} files, {len(crashed)} crashed, false errors: "
          + ", ".join(f"{c} {false[c]}" for c in CORPORA))
    sys.exit(1 if false or crashed else 0)
