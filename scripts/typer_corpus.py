"""The typer test files, their lists for typer-dump and the oracle, and the records both print."""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TESTS = ROOT / "ref/dotty/tests"
WORK = ROOT / "tmp/typer/3.9.0"
DUMP = ROOT / "target/release/typer-dump"
CORPORA = ("programs", "pos", "neg")
USING = re.compile(r"^//> using options (.*)$", re.M)


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
    """{path relative to the repository: record text} from dump or oracle output, recorded in any
    checkout."""
    records = {}
    for chunk in output.split("== file ")[1:]:
        path, _, body = chunk.partition("\n")
        records[relative(path)] = body
    return records


def relative(path):
    """`path` relative to the repository, also for a path from another checkout."""
    if Path(path).is_relative_to(ROOT):
        return str(Path(path).relative_to(ROOT))
    marker = next((m for m in ("/ref/dotty/", "/tests/programs/", "/crates/") if m in path), None)
    if marker is None:
        sys.exit(f"{path}: not in the repository")
    return marker[1:] + path.split(marker, 1)[1]


def load_expected():
    path = WORK / "expected.txt"
    return split(path.read_text(encoding="utf-8")) if path.exists() else {}


def has_errors(record):
    return "\n== E " in "\n" + record


def errors(record):
    """{offset: message} of the errors of a dump or oracle record."""
    return {int(m[1]): m[2] for m in re.finditer(r"^== E (\d+) (.*)$", record, re.M)}
