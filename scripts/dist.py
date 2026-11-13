#!/usr/bin/env python3
"""The relocatable distribution of scivolo, scivolo-aot and scivolo-jvm.

Usage: scripts/dist.py build   build tmp/dist/scivolo-<version>-<target>/ and its .tar.gz
       scripts/dist.py check   run hello.scala through each program of a copy of it

Layout: bin/ the programs; lib/ the C runtime, the checked-in TASTy jars, lib/scala/ the
library jars (from fetch-libs.sh), lib/aot/ the runtime archive and C stubs of native executables.
"""
import json
import os
import platform
import shutil
import subprocess
import sys
import tarfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DIST = ROOT / "tmp/dist"
CHECK = ROOT / "tmp/dist-check"
PROGRAMS = ["scivolo", "scivolo-aot", "scivolo-jvm"]
CHECKED_IN = ["javalib-tasty-0.5.12.jar", "jdk-declarations.jar"]
DOCS = ["LICENSE", "NOTICE", "README.md"]
SCALA_LIBRARY = "scala-library-3.9.0.jar"
# The jars crates/libs loads (its LIBS and the standard library), as fetch-libs.sh names them.
LIBRARY = [
    *(f"{lib}_native0.5_3-0.5.12.jar" for lib in ["auxlib", "javalib", "nativelib", "clib", "posixlib"]),
    "scalalib_native0.5_3-3.9.0%2B0.5.12.jar",
    SCALA_LIBRARY,
]
HELLO = "hello"
EXPECTED = "Hello, world!\n"
TIMEOUT = 30  # seconds per compile or run, as the test programs


def target():
    os_name = {"Linux": "linux", "Darwin": "macos"}[platform.system()]
    arch = {"x86_64": "x86_64", "AMD64": "x86_64", "arm64": "arm64", "aarch64": "arm64"}[platform.machine()]
    return f"{os_name}-{arch}"


def version():
    args = ["cargo", "metadata", "--no-deps", "--format-version", "1"]
    meta = json.loads(subprocess.run(args, cwd=ROOT, check=True, stdout=subprocess.PIPE, text=True).stdout)
    return next(p["version"] for p in meta["packages"] if p["name"] == "scivolo")


def name():
    return f"scivolo-{version()}-{target()}"


def build_programs():
    """Builds the programs in release mode; returns the out dir of the C runtime's build script."""
    args = ["cargo", "build", "--release", "--locked", "--message-format=json"]
    for p in PROGRAMS:
        args += ["-p", p]
    out = subprocess.run(args, cwd=ROOT, check=True, stdout=subprocess.PIPE, text=True).stdout
    for line in out.splitlines():
        msg = json.loads(line)
        if msg.get("reason") == "build-script-executed" and "scivolo-snrt" in msg["package_id"]:
            return Path(msg["out_dir"])
    sys.exit("dist: no build script output of scivolo-snrt")


def build():
    snrt = build_programs()
    subprocess.run([ROOT / "scripts/fetch-libs.sh"], check=True)
    DIST.mkdir(parents=True, exist_ok=True)
    out = DIST / name()
    shutil.rmtree(out, ignore_errors=True)
    lib = out / "lib"
    (out / "bin").mkdir(parents=True)
    (lib / "scala").mkdir(parents=True)
    (lib / "aot").mkdir()
    for p in PROGRAMS:
        shutil.copy2(ROOT / "target/release" / p, out / "bin")
    shared = "libsnrt.dylib" if platform.system() == "Darwin" else "libsnrt.so"
    shutil.copy2(snrt / shared, lib)
    shutil.copy2(snrt / "libsnrt.a", lib / "aot")
    shutil.copytree(ROOT / "crates/aot/c", lib / "aot/c")
    for jar in CHECKED_IN:
        shutil.copy2(ROOT / "libs" / jar, lib)
    for jar in LIBRARY:
        shutil.copy2(ROOT / "ref/libs" / jar, lib / "scala")
    for doc in DOCS:
        shutil.copy2(ROOT / doc, out)
    shutil.copytree(ROOT / "licenses", out / "licenses")
    shutil.copy2(ROOT / "native/LICENSE-scala-native.md", out / "licenses/scala-native.md")
    archive = DIST / f"{name()}.tar.gz"
    with tarfile.open(archive, "w:gz") as tar:
        tar.add(out, arcname=out.name)
    print(f"dist: {archive.relative_to(ROOT)}")


def run(args, env, cwd):
    """Runs `args` with a time limit; fails the check with its output if it fails."""
    try:
        r = subprocess.run(args, env=env, cwd=cwd, capture_output=True, text=True, timeout=TIMEOUT)
    except subprocess.TimeoutExpired:
        sys.exit(f"dist-check: timeout: {' '.join(map(str, args))}")
    if r.returncode != 0:
        sys.exit(f"dist-check: exit {r.returncode}: {' '.join(map(str, args))}\n{r.stdout}{r.stderr}")
    return r.stdout


def expect(what, out):
    if out != EXPECTED:
        sys.exit(f"dist-check: {what} printed {out!r}, expected {EXPECTED!r}")
    print(f"dist-check: {what}: ok")


def check():
    shutil.rmtree(CHECK, ignore_errors=True)
    dist = CHECK / "scivolo"
    shutil.copytree(DIST / name(), dist, symlinks=True)
    work = CHECK / "work"
    home = CHECK / "home"
    for d in [work, home, CHECK / "coursier"]:
        d.mkdir(parents=True)
    shutil.copy2(ROOT / f"tests/programs/{HELLO}/{HELLO}.scala", work)
    env = {k: v for k, v in os.environ.items() if not k.startswith("SCIVOLO_") and k != "XDG_CACHE_HOME"}
    env.update(
        HOME=str(home),
        COURSIER_CACHE=str(CHECK / "coursier"),
        SCIVOLO_CACHE=str(CHECK / "cache"),
        SCIVOLO_FRONTEND="strict",
    )
    bin_dir = dist / "bin"
    source = f"{HELLO}.scala"
    expect("scivolo", run([bin_dir / "scivolo", source], env, work))
    run([bin_dir / "scivolo-aot", "-o", HELLO, source], env, work)
    expect("scivolo-aot", run([work / HELLO], env, work))
    run([bin_dir / "scivolo-jvm", "-d", "classes", source], env, work)
    if shutil.which("java"):
        cp = f"classes{os.pathsep}{dist / 'lib/scala' / SCALA_LIBRARY}"
        expect("scivolo-jvm", run(["java", "-cp", cp, HELLO], env, work))
    else:
        print("dist-check: scivolo-jvm: compiled; no java to run it")


if __name__ == "__main__":
    {"build": build, "check": check}[sys.argv[1]]()
