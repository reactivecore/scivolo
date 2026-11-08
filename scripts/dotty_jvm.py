"""dotty's tests/run through the Rust frontend and the JVM backend, run on `java -Xverify:all`.

Writes jvm-report.md and jvm-results.tsv under tmp/dotty/<versions>. Earlier compilation groups
are compiled by scala-cli for the JVM; the last group by scivolo-jvm, which sees their TASTy.
`scripts/DottyRun.java` calls `Test.main` through reflection, as dotty's test runner does.
"""
import collections
import concurrent.futures
import os
import re
import shutil
import sys

from dotty_sources import ROOT, SCALA, WORK, run, shared, tests

SCIVOLO_JVM = ROOT / "target/release/scivolo-jvm"
LIBRARY = shared(f"ref/libs/scala-library-{SCALA}.jar")
LAUNCHER = WORK / "jvm-launcher"
JOBS = max(2, (os.cpu_count() or 4) // 2)
TIMEOUT = int(os.environ.get("SCIVOLO_TIMEOUT", "60"))
OUTCOMES = ["passed", "wrong output", "verify error", "linkage error", "crashed", "timeout", "jvm refused",
            "lowering refused", "typer refused", "parser refused", "frontend crashed", "earlier groups failed",
            "skipped"]
GROUPED = ["jvm refused", "verify error", "linkage error", "crashed", "frontend crashed", "lowering refused",
           "typer refused"]
LINKAGE = re.compile(r"java\.lang\.(NoSuch\w+Error|AbstractMethodError|IncompatibleClassChangeError"
                     r"|NoClassDefFoundError|IllegalAccessError|BootstrapMethodError|ClassCircularityError)")
TOP = 25


def first_line(s):
    return next((l for l in s.splitlines() if l.strip()), "").replace("\t", " ")[:160]


def earlier(name, groups):
    """The scala-cli output of all groups but the last, or None and why it failed."""
    out = WORK / "jvm-earlier" / name
    if not (out / ".done").exists():
        shutil.rmtree(out, ignore_errors=True)
        out.mkdir(parents=True)
        for i, sources in enumerate(groups[:-1]):
            cp = ["--extra-jars", str(out)] if i else []
            cmd = ["scala-cli", "--power", "compile", "--quiet", "--scala", SCALA, "--workspace",
                   str(WORK / "ws-jvm" / name), *cp, "-d", str(out), *map(str, sources)]
            code, stdout, stderr = run(cmd, TIMEOUT * 3)
            if code != 0:
                return None, first_line("\n".join(l for l in (stdout + stderr).splitlines() if "error" in l))
        (out / ".done").touch()
    return out, ""


def compiled(name, groups):
    """The class directory of the test and its classpath, or an outcome and detail."""
    cp = []
    if len(groups) > 1:
        before, why = earlier(name, groups)
        if before is None:
            return ("earlier groups failed", why), []
        cp = [before]
    out = WORK / "jvm" / name
    shutil.rmtree(out, ignore_errors=True)
    cmd = [str(SCIVOLO_JVM), *(a for c in cp for a in ("--cp", str(c))), "-d", str(out), *map(str, groups[-1])]
    code, _, err = run(cmd, TIMEOUT)
    if code is None:
        return ("frontend crashed", "timeout"), []
    if code != 0:
        message = first_line(err)
        if m := re.search(r"panicked at [^:]+:\d+:\d+:\s*(.*)", err):
            return ("frontend crashed", first_line(m[1])), []
        if m := re.match(r"the (typer|lowering|JVM backend) does not support (.*?)( at (\S+:)?\d+:\d+)?$", message):
            stage = "jvm" if m[1] == "JVM backend" else m[1]
            return (f"{stage} refused", m[2]), []
        if "syntax error" in message:
            return ("parser refused", message), []
        return ("frontend crashed", message), []
    return out, cp


def java_error(err):
    """The first `java.*` throwable line of a JVM's stderr, without the thread prefix."""
    line = next((l for l in err.splitlines() if re.search(r"\bjava\.\w+\.\w+(Error|Exception)", l)), first_line(err))
    return re.sub(r'^Exception in thread "\w+" ', "", line).strip()[:160]


def one(name, groups, check, reason):
    if reason:
        return "skipped", reason
    out, cp = compiled(name, groups)
    if isinstance(out, tuple):
        return out
    classpath = os.pathsep.join(map(str, [out, *cp, LIBRARY, LAUNCHER]))
    cmd = ["java", "-Xverify:all", "-Duser.language=en", "-Duser.country=US", "-cp", classpath, "DottyRun"]
    code, stdout, err = run(cmd, TIMEOUT, cwd=out)
    (WORK / "jvm-logs" / name).write_text(stdout + "\n--- stderr ---\n" + err)
    if code is None:
        return "timeout", ""
    if code != 0:
        error = java_error(err)
        if "java.lang.VerifyError" in error or "java.lang.ClassFormatError" in error:
            return "verify error", error
        if LINKAGE.search(error):
            return "linkage error", error
        return "crashed", error
    if check and stdout.rstrip("\n").splitlines() != check.read_text(errors="replace").rstrip("\n").splitlines():
        return "wrong output", ""
    return "passed", ""


def group(detail):
    """A refusal or error without the names of members, for counting alike failures together."""
    return re.sub(r"(Location:|for class|method|class|in|field) \S+", r"\1 _", re.sub(r"'[^']*'", "'_'", detail))


def jvm(filter_):
    (WORK / "jvm-logs").mkdir(parents=True, exist_ok=True)
    code, _, err = run(["javac", "-d", str(LAUNCHER), str(ROOT / "scripts/DottyRun.java")], TIMEOUT)
    if code != 0:
        sys.exit(f"compiling DottyRun.java: {err}")
    todo = []
    for name, groups, _, reason in tests(filter_):
        check = shared("ref/scala3/tests/run") / (name + ".check")
        todo.append((name, groups, check if check.exists() else None, None if reason == "jvm-only api" else reason))
    with concurrent.futures.ThreadPoolExecutor(JOBS) as pool:
        results = list(pool.map(lambda t: one(*t), todo))
    path = WORK / "jvm-results.tsv"
    old = [l.split("\t") for l in path.read_text().splitlines()] if filter_ and path.exists() else []
    rows = {r[0]: (r[1], r[2]) for r in old} | {t[0]: r for t, r in zip(todo, results)}
    path.write_text("".join(f"{n}\t{o}\t{d}\n" for n, (o, d) in sorted(rows.items())))
    counts = collections.Counter(o for o, _ in rows.values())
    md = "| Outcome | Tests |\n|---|---:|\n" + "".join(f"| {o} | {counts[o]} |\n" for o in OUTCOMES)
    md += f"| **total** | **{len(rows)}** |\n"
    for outcome in GROUPED:
        reasons = collections.Counter(group(d) for o, d in rows.values() if o == outcome)
        md += f"\n### {outcome.capitalize()} (top {TOP} of {len(reasons)})\n\n| Tests | Reason |\n|---:|---|\n"
        md += "".join(f"| {c} | {r.replace('|', '\\|')} |\n" for r, c in reasons.most_common(TOP))
    (WORK / "jvm-report.md").write_text(md)
    print(md)
