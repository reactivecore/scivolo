"""The typer report: each file's outcome against dotty's typed trees, and their counts per corpus."""
import collections
import queue
import subprocess
import threading

from typer_corpus import (CORPORA, DUMP, DUMP_JVM, WORK, corpus, feature_stats, has_errors, load_expected, selected,
                          split, write_list)

FILE_TIMEOUT = 120
OUTCOMES = ["passed", "differ", "warnings differ", "rejected", "accepted", "syntax errors", "unsupported",
            "crashed", "skipped"]
SUBSET_EXCLUDED = ("ty_match", "tt_match_type", "quotes", "macro_calls")


def in_subset(expected):
    """Whether dotty typed the file without errors and without features outside the subset."""
    if has_errors(expected) or expected.startswith("== crash"):
        return False
    return not set(feature_stats(expected) or {}) & set(SUBSET_EXCLUDED)


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


def runnable_rows(filter_, expected):
    """The files to type as (path, options), and {path: (outcome, detail)} of those skipped."""
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
    return runnable, rows


def write_results(rows, name):
    """Write `name` and print the files whose outcome changed against its previous version."""
    results = WORK / name
    previous = {}
    if results.exists():
        for line in results.read_text().splitlines():
            n, o, _ = (line.split("\t") + ["", ""])[:3]
            previous[n] = o
    results.write_text("".join(f"{n}\t{o}\t{d}\n" for n, (o, d) in sorted(rows.items())))
    changed = [(n, previous[n], o, d) for n, (o, d) in sorted(rows.items()) if n in previous and previous[n] != o]
    for n, before, after, d in changed:
        print(f"{before} -> {after}: {n} {d}")


def dump_until_stuck(rows, command):
    """The dumps of `rows` by `command` in order, up to the first that aborts the process or takes
    longer than FILE_TIMEOUT."""
    lst = write_list("run.tsv", rows)
    proc = subprocess.Popen([*command, "--list", str(lst)], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                            text=True, encoding="utf-8", errors="replace")
    lines, done = [], queue.Queue()

    def read():
        for line in proc.stdout:
            lines.append(line)
            if line.startswith("== file "):
                done.put(True)
        done.put(False)

    reader = threading.Thread(target=read)
    reader.start()
    try:
        while done.get(timeout=FILE_TIMEOUT):
            pass
    except queue.Empty:
        proc.kill()
    reader.join()
    proc.wait()
    return split("".join(lines))


def dump(rows, command):
    """The dump of each of `rows` by `command`; a crash record for one that aborts the process or
    times out."""
    actual = {}
    while rows:
        got = dump_until_stuck(rows, command)
        actual.update(got)
        if len(got) >= len(rows):
            break
        actual[rows[len(got)][0]] = f"== crash aborted or over {FILE_TIMEOUT} s\n"
        rows = rows[len(got) + 1:]
    return actual


def outcomes(expected, runnable, rows, command):
    """`rows` with the outcome of each of `runnable` typed by `command`."""
    actual = dump(runnable, command)
    rows = dict(rows)
    for rel, _ in runnable:
        got = actual.get(rel, "== crash no output")
        rows[rel] = classify_neg(got) if corpus(rel) == "neg" else classify(expected[rel], got)
    return rows


def report(filter_):
    """Type the selected files against the JDK into results.tsv, and those of tests/programs also
    against Scala Native's javalib into results-native.tsv; count the outcomes into report.md."""
    expected = load_expected()
    runnable, rows = runnable_rows(filter_, expected)
    rows = outcomes(expected, runnable, rows, DUMP_JVM)
    write_results(rows, "results.tsv")
    programs = [r for r in runnable if corpus(r[0]) == "programs"]
    native = outcomes(expected, programs, {}, [str(DUMP)])
    write_results(native, "results-native.tsv")
    columns = [*CORPORA, "programs (Native)"]
    table = "| Outcome | " + " | ".join(columns) + " |\n|---|" + "---:|" * len(columns) + "\n"
    counts = collections.Counter((corpus(n), o) for n, (o, _) in rows.items())
    counts.update(("programs (Native)", o) for o, _ in native.values())
    for o in OUTCOMES:
        table += f"| {o} | " + " | ".join(str(counts[c, o]) for c in columns) + " |\n"
    (WORK / "report.md").write_text(table)
    print(table)
