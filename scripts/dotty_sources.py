"""The tests of dotty's tests/run: their sources in compilation groups, check files and skip reasons;
the work directory of their builds, its result files, and running a command with a timeout."""
import os
import re
import signal
import subprocess
import sys
from pathlib import Path

SCALA, NATIVE = "3.9.0", "0.5.12"
ROOT = Path(__file__).resolve().parent.parent
# The main checkout, whose ref/ and tmp/dotty a worktree without its own uses.
MAIN = (ROOT / subprocess.run(["git", "rev-parse", "--git-common-dir"], cwd=ROOT, capture_output=True,
                              text=True).stdout.strip()).resolve().parent


def shared(rel):
    """`rel` in this checkout, else in the main checkout when only that has it."""
    return ROOT / rel if (ROOT / rel).exists() or not (MAIN / rel).exists() else MAIN / rel


WORK = shared(f"tmp/dotty/{SCALA}-{NATIVE}")


def lines(text):
    return text.rstrip("\n").splitlines()


def run(cmd, timeout, **kwargs):
    """Exit code (None on timeout), stdout and stderr of `cmd`; on timeout its whole process group
    is killed, so no child (cc, ld) outlives it."""
    p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, errors="replace",
                         start_new_session=True, **kwargs)
    try:
        out, err = p.communicate(timeout=timeout)
        return p.returncode, out, err
    except subprocess.TimeoutExpired:
        os.killpg(p.pid, signal.SIGKILL)
        out, err = p.communicate()
        return None, out, err


def load(file):
    """The rows of result file `file` in WORK: test → (outcome, detail)."""
    path = WORK / file
    rows = [l.split("\t") + [""] for l in path.read_text().splitlines()] if path.exists() else []
    return {r[0]: (r[1], r[2]) for r in rows}


def save(path, rows):
    path.write_text("".join(f"{n}\t{o}\t{d}\n" for n, (o, d) in sorted(rows.items())))

# Code outside comments using APIs the Scala Native javalib lacks.
JVM_ONLY = re.compile(
    r"java\.lang\.reflect|getDeclared|\.getMethods?\b|\.getFields?\b|scala\.tools\.|scala\.sys\.process"
    r"|java\.lang\.invoke|javax\.|ObjectOutputStream"
)
COMMENT = re.compile(r"//[^\n]*|/\*.*?\*/", re.S)
OTHER_COMPILER =re.compile(r"_c[\d.]+\.scala$")


def group(source):
    """The separate-compilation group of a file, from the ordinal in its `_N` name suffix."""
    ordinals = [int(p) for p in source.stem.split("_")[1:] if re.fullmatch(r"[0-9]+", p)]
    return ordinals[0] if ordinals else -1


def skip_reason(files, sources):
    texts = [s.read_text(errors="replace") for s in sources]
    if not sources:
        return "no sources"
    if any(s.suffix == ".java" for s in files):
        return "java sources"
    if any(OTHER_COMPILER.search(s.name) for s in sources):
        return "other compiler"
    if any("//> using target.platform" in t for t in texts):
        return "platform-specific"
    if any(JVM_ONLY.search(COMMENT.sub("", t)) for t in texts):
        return "jvm-only api"
    return None


def scala_cli(command, name, sources, *extra):
    return ["scala-cli", "--power", command, "--quiet", "--native", "--native-version", NATIVE,
            "--scala", SCALA, "--workspace", str(WORK / "ws" / name), *extra, *map(str, sources)]


def compile_groups(name, groups, out, timeout=None):
    """Compiles the groups in order into directory `out`, each seeing the output before it; the
    exit code of the last compile (None on timeout) and the compilers' output."""
    code, log = 0, ""
    for i, sources in enumerate(groups):
        extra = ["--extra-jars", str(out)] if i else []
        code, stdout, stderr = run(scala_cli("compile", name, sources, *extra, "-d", str(out)), timeout)
        log += stdout + stderr
        if code != 0:
            break
    return code, log


def tests(filter_, dir="run"):
    """(name, source groups in compilation order, check file or None, skip reason or None) of
    the tests in dotty's tests/`dir`."""
    TESTS = shared(f"ref/scala3/tests/{dir}")
    if not TESTS.is_dir():
        sys.exit(f"{TESTS} missing: run `just dotty-fetch`" + (" and `just dotty-macros`" if dir != "run" else ""))
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
