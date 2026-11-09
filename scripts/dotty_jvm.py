"""dotty's tests/run through the Rust frontend and the JVM backend, run on `java -Xverify:all`.

Writes jvm-report.md and jvm-results.tsv under tmp/dotty/<versions> (another directory DIR of
dotty's tests: jvm-report-DIR.md, jvm-results-DIR.tsv). Earlier compilation groups are compiled
by scala-cli for the JVM; the last group by scivolo-jvm, which sees their TASTy and, for a macro
call, runs the macro in the VM over their Scala Native build (ADR 0112).
`scripts/DottyRun.java` calls `Test.main` through reflection, as dotty's test runner does.
"""
import collections
import concurrent.futures
import os
import re
import shutil
import sys

from dotty_sources import ROOT, SCALA, WORK, compile_groups, run, shared, tests, using_options

SCIVOLO_JVM = ROOT / "target/release/scivolo-jvm"
LIBRARY = shared(f"ref/libs/scala-library-{SCALA}.jar")
LAUNCHER = WORK / "jvm-launcher"
JOBS = max(2, (os.cpu_count() or 4) // 2)
TIMEOUT = int(os.environ.get("SCIVOLO_TIMEOUT", "60"))
OUTCOMES = ["passed", "wrong output", "verify error", "linkage error", "crashed", "timeout", "jvm refused",
            "lowering refused", "typer refused", "macro refused", "parser refused", "frontend crashed", "earlier groups failed",
            "skipped"]
GROUPED = ["jvm refused", "verify error", "linkage error", "crashed", "frontend crashed", "lowering refused",
           "typer refused", "macro refused", "earlier groups failed"]
LINKAGE = re.compile(r"java\.lang\.(NoSuch\w+Error|AbstractMethodError|IncompatibleClassChangeError"
                     r"|NoClassDefFoundError|IllegalAccessError|BootstrapMethodError|ClassCircularityError)")
TOP = 25


def first_line(s):
    return next((l for l in s.splitlines() if l.strip()), "").replace("\t", " ")[:160]


def errors(log):
    return first_line("\n".join(l for l in log.splitlines() if "error" in l) or log)


def earlier(key, groups, options):
    """The scala-cli JVM output of all groups but the last, or None and why it failed."""
    out = WORK / "jvm-earlier" / key
    if not (out / ".done").exists():
        shutil.rmtree(out, ignore_errors=True)
        out.mkdir(parents=True)
        for i, sources in enumerate(groups[:-1]):
            cp = ["--extra-jars", str(out)] if i else []
            cmd = ["scala-cli", "--power", "compile", "--quiet", "--scala", SCALA, "--workspace",
                   str(WORK / "ws-jvm" / key), *cp, *(a for o in options for a in ("-O", o)), "-d", str(out),
                   *map(str, sources)]
            code, stdout, stderr = run(cmd, TIMEOUT * 3)
            if code != 0:
                return None, errors(stdout + stderr)
        (out / ".done").touch()
    return out, ""


def native_earlier(name, groups, dir, options):
    """The Scala Native output (NIR) of all groups but the last, as `scripts/dotty-lower.py` keeps it."""
    out = WORK / "earlier" / dir / name
    if not (out / ".done").exists():
        shutil.rmtree(out, ignore_errors=True)
        out.mkdir(parents=True)
        code, log = compile_groups(f"{dir}-{name}", groups[:-1], out, TIMEOUT * 3, options)
        if code != 0:
            return None, errors(log)
        (out / ".done").touch()
    return out, ""


def compiled(name, groups, dir, options=None):
    """The class directory of the test and its classpath, or an outcome and detail; macro calls
    into earlier groups run on their Scala Native build. With `options`, the last group was split."""
    key = name if dir == "run" else f"{dir}-{name}"
    if len(groups) == 1:
        return scivolo(key, groups[-1], [], []), []
    split = "" if options is None else "-split"
    before, why = earlier(key + split, groups, options or [])
    if before is None:
        return ("earlier groups failed", why), []
    outcome = scivolo(key, groups[-1], [before], [])
    if outcome == ("typer refused", "macro call"):
        nir, why = native_earlier(name + split, groups, dir, options or [])
        if nir is None:
            return ("earlier groups failed", f"Scala Native: {why}"), []
        outcome = scivolo(key, groups[-1], [before], [nir])
    return outcome, [before]


def scivolo(key, sources, cp, macro_cp, out=None):
    """The class directory scivolo-jvm compiles `sources` to (default: one for `key` in WORK), or
    an outcome and detail."""
    out = out or WORK / "jvm" / key
    shutil.rmtree(out, ignore_errors=True)
    flags = [a for c in cp for a in ("--cp", str(c))] + [a for c in macro_cp for a in ("--macro-cp", str(c))]
    root = next(p.parent for p in sources[0].parents if p.name == "tests")
    files = [str(s.relative_to(root)) for s in sources]
    code, _, err = run([str(SCIVOLO_JVM), *flags, "-d", str(out), *files], TIMEOUT, cwd=root)
    if code is None:
        return "frontend crashed", "timeout"
    if code != 0:
        message = next((l for l in err.splitlines() if l.strip()), "").replace("\t", " ")
        if m := re.search(r"Failed to evaluate macro\..*?unsupported: no method (\S+)", err):
            return "macro refused", m[1].removeprefix("scala.quoted.runtime.impl.")
        if m := re.search(r"panicked at [^:]+:\d+:\d+:\s*(.*)", err):
            return "frontend crashed", first_line(m[1])
        if m := re.match(r"the (typer|lowering|JVM backend) does not support (.*?)( at (\S+:)?\d+:\d+)?$", message):
            stage = "jvm" if m[1] == "JVM backend" else m[1]
            return f"{stage} refused", m[2][:160]
        if "syntax error" in message:
            return "parser refused", message[:160]
        return "frontend crashed", message[:160]
    return out


def java_error(err):
    """The first `java.*` throwable line of a JVM's stderr, without the thread prefix."""
    line = next((l for l in err.splitlines() if re.search(r"\bjava\.\w+\.\w+(Error|Exception)", l)), first_line(err))
    return re.sub(r'^Exception in thread "\w+" ', "", line).strip()[:160]


def one(name, groups, check, reason, dir):
    """The outcome of test `name` and a detail; a last group whose macro definitions the typer
    refuses compiles them first, as `scripts/dotty-lower.py` does."""
    if reason:
        return "skipped", reason
    out, cp = compiled(name, groups, dir)
    quoted = [s for s in groups[-1] if "scala.quoted" in s.read_text(errors="replace")]
    if isinstance(out, tuple) and out[0] == "typer refused" and 0 < len(quoted) < len(groups[-1]):
        rest = [s for s in groups[-1] if s not in quoted]
        out, cp = compiled(name, [*groups[:-1], quoted, rest], dir, using_options(groups[-1]))
    if isinstance(out, tuple):
        return out
    classpath = os.pathsep.join(map(str, [out, *cp, LIBRARY, LAUNCHER]))
    cmd = ["java", "-Xverify:all", "-Duser.language=en", "-Duser.country=US", "-cp", classpath, "DottyRun"]
    code, stdout, err = run(cmd, TIMEOUT, cwd=out)
    (WORK / "jvm-logs" / out.name).write_text(stdout + "\n--- stderr ---\n" + err)
    return ran(code, stdout, err, check)


def ran(code, output, err, check):
    """The outcome of a JVM run that exited with `code` and printed `output` and `err`."""
    if code is None:
        return "timeout", ""
    if code != 0:
        error = java_error(err)
        if "java.lang.VerifyError" in error or "java.lang.ClassFormatError" in error:
            return "verify error", error
        if LINKAGE.search(error):
            return "linkage error", error
        return "crashed", error
    if check and output.rstrip("\n").splitlines() != check.read_text(errors="replace").rstrip("\n").splitlines():
        return "wrong output", ""
    return "passed", ""


def group(detail):
    """A refusal or error without the names of members, for counting alike failures together."""
    return re.sub(r"(Location:|for class|method|class|in|field) \S+", r"\1 _", re.sub(r"'[^']*'", "'_'", detail))


def jvm(filter_, dir="run"):
    (WORK / "jvm-logs").mkdir(parents=True, exist_ok=True)
    code, _, err = run(["javac", "-d", str(LAUNCHER), str(ROOT / "scripts/DottyRun.java")], TIMEOUT)
    if code != 0:
        sys.exit(f"compiling DottyRun.java: {err}")
    todo, all_ = [], list(tests(filter_, dir))
    files = {n for n, g, _, _ in all_ if not g or not g[0] or g[0][0].parent.name == dir}
    for name, groups, check, reason in all_:
        if name in files and groups and groups[0] and groups[0][0].parent.name != dir:
            name += "-dir"
        todo.append((name, groups, check, None if reason == "jvm-only api" else reason))
    with concurrent.futures.ThreadPoolExecutor(JOBS) as pool:
        results = list(pool.map(lambda t: one(*t, dir), todo))
    suffix = "" if dir == "run" else f"-{dir}"
    write(WORK / f"jvm-results{suffix}.tsv", WORK / f"jvm-report{suffix}.md", filter_,
          {t[0]: r for t, r in zip(todo, results)})


def write(path, report, filter_, new):
    """Writes the result rows `new` over those of the last run at `path` when `filter_` names
    some, and their counts and top groups to `report`."""
    old = [l.split("\t") for l in path.read_text().splitlines()] if filter_ and path.exists() else []
    rows = {r[0]: (r[1], r[2]) for r in old} | new
    path.write_text("".join(f"{n}\t{o}\t{d}\n" for n, (o, d) in sorted(rows.items())))
    counts = collections.Counter(o for o, _ in rows.values())
    md = "| Outcome | Tests |\n|---|---:|\n" + "".join(f"| {o} | {counts[o]} |\n" for o in OUTCOMES)
    md += f"| **total** | **{len(rows)}** |\n"
    for outcome in GROUPED:
        reasons = collections.Counter(group(d) for o, d in rows.values() if o == outcome)
        md += f"\n### {outcome.capitalize()} (top {TOP} of {len(reasons)})\n\n| Tests | Reason |\n|---:|---|\n"
        md += "".join(f"| {c} | {r.replace('|', '\\|')} |\n" for r, c in reasons.most_common(TOP))
    report.write_text(md)
    print(md)
