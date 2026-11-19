#!/usr/bin/env python3
"""The TASTy scivolo-lower pickles for a module of Scala Native's library, compared with that of
Scala Native's build (`just lib-oracle`, tmp/lib/sn/jars).

Usage: scripts/lib-tasty.py MODULE [FILTER]

Lowers the units of MODULE (those of `just lib-compile`, their files' paths containing FILTER) as
lib-compile does, with `--pickle`, into tmp/lib/MODULE/tasty-out/UNIT/ (UNIT its first file's path
with `%` for `/`), and compares each TASTy
file with the oracle's as scripts/jvm-tasty.py does. Each NIR file written is compared with the
one lib-compile wrote for the unit, if there, to show that pickling leaves the NIR alone.

Writes under tmp/lib/MODULE: tasty.tsv (TASTy file, same|differ|ours-only|oracle-only),
tasty-diffs/ (a unified diff per differing file), tasty-groups.txt (differing files by their first
differing line, numbers replaced by N), unpickled.txt (why the classes of a file have no TASTy);
prints one summary line.
"""
import collections
import concurrent.futures
import difflib
import importlib
import re
import shutil
import subprocess
import sys
import zipfile

lib = importlib.import_module("lib-compile")
jvm_tasty = importlib.import_module("jvm-tasty")

PICKLE = "--pickle"
PRINTED_PER_RUN = 400


def unit_dir(module, shown):
    """The output directory of unit `shown`, beside those of the others, not inside one."""
    return lib.WORK / module / "tasty-out" / shown.replace("/", "%")


def lower(module, unit, env):
    """(unit directory, state, stderr lines) of lowering `unit` with PICKLE."""
    shown = f"{unit[0][0].name}/{unit[0][1].relative_to(unit[0][0])}".removesuffix(".scala")
    out = unit_dir(module, shown)
    cmd = lib.lower_cmd(module, unit, out)
    cmd.insert(1, PICKLE)
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, errors="replace",
                           timeout=lib.TIMEOUT * len(unit), env=env)
    except subprocess.TimeoutExpired:
        return shown, "timeout", []
    return shown, "ok" if r.returncode == 0 else "refused", r.stderr.splitlines()


def oracle(module):
    """The directory of the oracle jar's TASTy files of `module`, unpacked once."""
    whole = lib.PART_OF[module][0] if module in lib.PART_OF else module
    out = lib.WORK / "sn" / "tasty" / whole
    if not (out / ".done").exists():
        with zipfile.ZipFile(lib.ORACLE_JARS / lib.jar_name(whole)) as jar:
            jar.extractall(out, [n for n in jar.namelist() if n.endswith(".tasty")])
        (out / ".done").touch()
    return out


def nir_changed(module, shown):
    """The NIR files of unit `shown` that differ from lib-compile's, where it wrote them."""
    ours, plain = unit_dir(module, shown), lib.WORK / module / "out" / shown
    if not plain.is_dir():
        return []
    return [str(p.relative_to(ours)) for p in ours.rglob("*.nir")
            if not (plain / p.relative_to(ours)).exists() or (plain / p.relative_to(ours)).read_bytes() != p.read_bytes()]


def without_addresses(line):
    """Printed TASTy line `line` without its address, lengths and the addresses it refers to."""
    line = re.sub(r"^\s*\d+:", "", line)
    line = re.sub(r"\(\d+( bytes)?\)", "", line)
    return re.sub(r"\b(SHARED\w+|\w+symbol|\w+direct|\w+in) \d+", r"\1", line).strip()


def first_change(left, right):
    """The first line that differs between printouts `left` and `right` besides addresses and the
    order of names, its numbers replaced by N, or `addresses only`."""
    def trees(lines):
        start = next((i for i, l in enumerate(lines) if l.startswith("Trees")), 0)
        return [without_addresses(l) for l in lines[start:]]
    left, right = trees(left), trees(right)
    line = next((l for l in difflib.unified_diff(left, right, lineterm="", n=0) if l[:1] in "+-" and l[1:3] not in ("--", "++")), None)
    return "addresses only" if line is None else re.sub(r"\d+", "N", line)


def compare(module, ours):
    """The rows of tasty.tsv for the TASTy files under `ours` (TASTy path to unit directory) and
    the oracle's, writing the diffs."""
    theirs, diffs = oracle(module), lib.WORK / module / "tasty-diffs"
    prefix = lib.PART_OF[module][1] if module in lib.PART_OF else ""
    own = {str(p.relative_to(theirs)) for p in theirs.rglob("*.tasty")}
    own = {p for p in own if p.startswith(prefix) and (module != "scalalib" or not lib.scalalib_sources.specialized(p))}
    rows = [[p, "oracle-only", ""] for p in sorted(own - ours.keys())]
    rows += [[p, "ours-only", ""] for p in sorted(ours.keys() - own)]
    both = sorted(own & ours.keys())
    for start in range(0, len(both), PRINTED_PER_RUN):
        names = both[start:start + PRINTED_PER_RUN]
        shutil.rmtree(jvm_tasty.COPIES, ignore_errors=True)
        cuts = [[] for _ in range(2 * len(names))]
        copies = [jvm_tasty.printable(theirs, n, "scalac", cuts[i]) for i, n in enumerate(names)]
        copies += [jvm_tasty.printable(ours[n], n, "ours", cuts[i + len(names)]) for i, n in enumerate(names)]
        printed = jvm_tasty.shown(copies)
        for i, name in enumerate(names):
            left, right = printed[i], printed[i + len(names)]
            diff = list(difflib.unified_diff(left, right, f"oracle/{name}", f"ours/{name}", lineterm=""))
            if cuts[i] != cuts[i + len(names)]:
                diff += ["cut constants differ"]
            if not diff:
                rows.append([name, "same", ""])
                continue
            path = diffs / f"{name}.diff"
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("\n".join(diff) + "\n")
            rows.append([name, "differ", first_change(left, right)])
    return sorted(rows)


def main(module, filter_):
    work = lib.WORK / module
    shutil.rmtree(work / "tasty-out", ignore_errors=True)
    shutil.rmtree(work / "tasty-diffs", ignore_errors=True)
    env = lib.ENV
    if module == "scalalib":
        env = lib.ENV | {"SCIVOLO_LIBS": str(lib.scalalib_sources.libs(lib.ORACLE_JARS))}
        lib.link_scalalib_root(lib.scalalib_sources.sources(lib.LIBS))
    with concurrent.futures.ThreadPoolExecutor(lib.JOBS) as pool:
        done = list(pool.map(lambda u: lower(module, u, env), lib.units(module, filter_)))
    ours, unpickled, changed = {}, [], []
    for shown, _, stderr in done:
        unit = unit_dir(module, shown)
        ours |= {str(p.relative_to(unit)): unit for p in unit.rglob("*.tasty")}
        unpickled += [f"{shown}: {l.removeprefix('no TASTy: ')}" for l in stderr if l.startswith("no TASTy: ")]
        changed += [f"{shown}: {p}" for p in nir_changed(module, shown)]
    rows = compare(module, ours)
    lib.write_tsv(work / "tasty.tsv", rows)
    groups = collections.Counter(r[2] for r in rows if r[1] == "differ")
    (work / "tasty-groups.txt").write_text("".join(f"{c:5} {g}\n" for g, c in groups.most_common()))
    (work / "unpickled.txt").write_text("".join(f"{l}\n" for l in unpickled))
    states = collections.Counter(r[1] for r in rows)
    units = collections.Counter(s for _, s, _ in done)
    print(f"{module}: tasty {states['same']} same, {states['differ']} differ, {states['ours-only']} ours-only, "
          f"{states['oracle-only']} oracle-only; {len(unpickled)} files unpickled; units {units['ok']} ok, "
          f"{units['refused']} refused, {units['timeout']} timeout; {len(changed)} NIR files changed by pickling")
    for c in changed:
        print(f"  NIR changed: {c}")


if __name__ == "__main__":
    if len(sys.argv) not in (2, 3) or sys.argv[1] not in lib.MODULES:
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2] if len(sys.argv) == 3 else "")
