"""The tests of dotty's tests/run: their sources in compilation groups, check files and skip reasons."""
import re
import sys
from pathlib import Path

TESTS = Path(__file__).resolve().parent.parent / "ref/scala3/tests/run"

# Code outside comments using APIs the Scala Native javalib lacks.
JVM_ONLY = re.compile(
    r"java\.lang\.reflect|getDeclared|\.getMethods?\b|\.getFields?\b|scala\.tools\.|scala\.sys\.process"
    r"|java\.lang\.invoke|javax\.|ObjectOutputStream"
)
COMMENT = re.compile(r"//[^\n]*|/\*.*?\*/", re.S)
OLD_COMPILER = re.compile(r"_c[\d.]+\.scala$")


def group(source):
    """The separate-compilation group of a file, from the ordinal in its `_N` name suffix."""
    ordinals = [int(p) for p in source.stem.split("_")[1:] if p.isdigit()]
    return ordinals[0] if ordinals else -1


def skip_reason(files, sources):
    texts = [s.read_text(errors="replace") for s in sources]
    if any(s.suffix == ".java" for s in files):
        return "java sources"
    if any(OLD_COMPILER.search(s.name) for s in sources):
        return "older compiler"
    if any("//> using target.platform" in t for t in texts):
        return "platform-specific"
    if any(JVM_ONLY.search(COMMENT.sub("", t)) for t in texts):
        return "jvm-only api"
    return None


def tests(filter_):
    """(name, source groups in compilation order, check file or None, skip reason or None)."""
    if not TESTS.is_dir():
        sys.exit(f"{TESTS} missing: run `just dotty-fetch`")
    for path in sorted(TESTS.iterdir()):
        name = path.name if path.is_dir() else path.stem
        if not (path.is_dir() or path.suffix == ".scala") or filter_ not in name:
            continue
        files = [s for s in sorted(path.rglob("*")) if s.is_file()] if path.is_dir() else [path]
        sources = [s for s in files if s.suffix == ".scala"]
        ordinals = sorted({group(s) for s in sources})
        groups = [[s for s in sources if group(s) == g] for g in ordinals] if path.is_dir() else [sources]
        check = TESTS / (name + ".check")
        yield name, groups, check if check.exists() else None, skip_reason(files, sources)
