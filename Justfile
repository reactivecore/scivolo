# Build, lint and test commands. `just --list` shows everything.

# CI must not update the lockfile; a developer shell may.
locked := if env_var_or_default("CI", "") == "" { "" } else { "--locked" }

# Scala Native's library jars, copied into the repo by `just libs`.
export SCIVOLO_LIBS := justfile_directory() + "/ref/libs"

# The library image of development runs.
export SCIVOLO_CACHE := justfile_directory() + "/tmp/cache"

default: gates

# Compile Scala Native's shared unit tests (default: the language tests in `scala`) and run
# them in the VM. Needs `ref/scala-native` (v0.5.12) and scala-cli.
sn-tests *dirs:
    scripts/sn-tests.sh {{ dirs }}
    cargo build -q --release -p scivolo
    scripts/sn-run.sh

# Run test program `name` from tests/programs in the release VM, stopped after `SCIVOLO_TIMEOUT`
# seconds (default 60).
run name *args: release
    SCIVOLO_TIMEOUT=${SCIVOLO_TIMEOUT:-60} target/release/scivolo --cp tests/programs/{{ name }} {{ name }} {{ args }}

# Run a `.scala` or `.sc` file (relative to the current directory), compiled through the own
# frontend or scala-cli unless cached.
[no-cd]
script file *args: release
    {{ justfile_directory() }}/target/release/scivolo {{ file }} {{ args }}

# Build method `entry` (an `(Int) => Int`) of test program `name` as the native executable
# tmp/aot/<name>, which prints the method's result for its first argument.
aot-build name entry:
    cargo build -q --release -p scivolo-aot
    perl -e 'alarm 20; exec @ARGV' target/release/scivolo-aot --cp tests/programs/{{ name }} --entry {{ entry }} -o tmp/aot/{{ name }}

# Build test program `name` (main class `name`) as the native executable tmp/aot/<name> and
# compare its output with its `.check` file; compile and run stop after 20 s each.
aot-check name:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build -q --release -p scivolo-aot
    perl -e 'alarm 20; exec @ARGV' target/release/scivolo-aot --cp tests/programs/{{ name }} -o tmp/aot/{{ name }} {{ name }}
    perl -e 'alarm 20; exec @ARGV' tmp/aot/{{ name }} > tmp/aot/{{ name }}.out
    diff tmp/aot/{{ name }}.out tests/programs/{{ name }}/{{ name }}.check
    echo "{{ name }}: ok"

# Build every test program as a native executable and compare each output with its `.check`
# file, compile and run stopped after 20 s each; failures with the first line of their error, or
# `timeout (compile)` / `timeout (run)`, go to tmp/aot/failures.txt.
aot-check-all:
    #!/usr/bin/env bash
    cargo build -q --release -p scivolo-aot
    mkdir -p tmp/aot
    : > tmp/aot/failures.txt
    pass=0; fail=0
    failed() { fail=$((fail + 1)); echo "$1: $2" >> tmp/aot/failures.txt; }
    for dir in tests/programs/*/; do
      name=$(basename "$dir")
      [ -f "$dir$name.check" ] || continue
      perl -e 'alarm 20; exec @ARGV' target/release/scivolo-aot --cp "$dir" -o "tmp/aot/$name" "$name" 2> "tmp/aot/$name.err"
      status=$?
      if [ $status -eq 142 ]; then failed "$name" "timeout (compile)"; continue; fi
      if [ $status -ne 0 ]; then failed "$name" "$(head -c 300 "tmp/aot/$name.err" | head -n 1)"; continue; fi
      perl -e 'alarm 20; exec @ARGV' "tmp/aot/$name" > "tmp/aot/$name.out" 2>> "tmp/aot/$name.err"
      status=$?
      if [ $status -eq 142 ]; then failed "$name" "timeout (run)"; continue; fi
      if [ $status -ne 0 ] || ! diff -q "tmp/aot/$name.out" "$dir$name.check" > /dev/null; then
        failed "$name" "exit $status: $(head -c 300 "tmp/aot/$name.err" | head -n 1)"
        continue
      fi
      pass=$((pass + 1))
    done
    echo "aot: $pass passed, $fail failed (tmp/aot/failures.txt)"

# Build the `.scala` files of directory `dir` (main class `main`) from source as the native
# executable tmp/aot/src/<main> through the own frontend, and run it with `args`; compile and run
# stop after 20 s each.
aot-src dir main *args:
    cargo build -q --release -p scivolo-aot
    mkdir -p tmp/aot/src
    perl -e 'alarm 20; exec @ARGV' target/release/scivolo-aot -o tmp/aot/src/{{ main }} --main {{ main }} {{ dir }}
    perl -e 'alarm 20; exec @ARGV' tmp/aot/src/{{ main }} {{ args }}

# Build every test program from its `.scala` sources as a native executable and compare each
# output with its `.check` file, like aot-check-all; failures go to tmp/aot/src/failures.txt,
# the frontend's refusals grouped by reason to tmp/aot/src/refusals.txt.
aot-src-all:
    #!/usr/bin/env bash
    cargo build -q --release -p scivolo-aot
    mkdir -p tmp/aot/src
    : > tmp/aot/src/failures.txt
    pass=0; fail=0
    failed() { fail=$((fail + 1)); echo "$1: $2" >> tmp/aot/src/failures.txt; }
    for dir in tests/programs/*/; do
      name=$(basename "$dir")
      [ -f "$dir$name.check" ] || continue
      perl -e 'alarm 20; exec @ARGV' target/release/scivolo-aot -o "tmp/aot/src/$name" --main "$name" "$dir" 2> "tmp/aot/src/$name.err"
      status=$?
      if [ $status -eq 142 ]; then failed "$name" "timeout (compile)"; continue; fi
      if [ $status -ne 0 ]; then failed "$name" "$(grep -m1 '^scivolo-aot:' "tmp/aot/src/$name.err" || head -n 1 "tmp/aot/src/$name.err")"; continue; fi
      perl -e 'alarm 20; exec @ARGV' "tmp/aot/src/$name" > "tmp/aot/src/$name.out" 2>> "tmp/aot/src/$name.err"
      status=$?
      if [ $status -eq 142 ]; then failed "$name" "timeout (run)"; continue; fi
      if [ $status -ne 0 ] || ! diff -q "tmp/aot/src/$name.out" "$dir$name.check" > /dev/null; then
        failed "$name" "run: exit $status: $(head -c 300 "tmp/aot/src/$name.err" | head -n 1)"
        continue
      fi
      pass=$((pass + 1))
    done
    cut -d: -f2- tmp/aot/src/failures.txt | sed -E 's/ at [^ ]+:[0-9]+:[0-9]+//' | sort | uniq -c | sort -rn > tmp/aot/src/refusals.txt
    echo "aot from source: $pass passed, $fail failed (tmp/aot/src/failures.txt, refusals.txt)"

# Build the dotty tests the VM passes (default: all; a number `n`: an evenly spaced sample of
# `n`; else the comma-separated names `n`) as native executables and compare them with their
# `.check`, 20 s per compile and run; results in tmp/aot/dotty/results.tsv.
aot-dotty n="0":
    cargo build -q --release -p scivolo-aot
    python3 scripts/aot-dotty.py '{{ n }}'

# Build as aot-build does and run the executable with `args`, stopped after 20 s.
aot-run name entry *args: (aot-build name entry)
    perl -e 'alarm 20; exec @ARGV' tmp/aot/{{ name }} {{ args }}

# Start the REPL in the release build; `--dep COORDINATE` adds a library fetched by Coursier.
[no-cd]
repl *args: release
    {{ justfile_directory() }}/target/release/scivolo {{ args }}

# Compare test programs (default: all) with their `.check` files, with wall time.
check *names: release
    python3 scripts/program_runs.py check {{ names }}

# Compile test program `name` with Scala Native into its committed NIR and `.check`.
program-update name:
    python3 scripts/programs.py update {{ name }}

# List test programs (default: all) whose committed NIR or `.check` differs from a rebuild.
programs-stale *names:
    cargo build -q --release -p scivolo-nir-dump
    python3 scripts/programs.py stale {{ names }}

# The same for the test programs changed since commit `ref` (all if their build changed).
programs-stale-since ref:
    cargo build -q --release -p scivolo-nir-dump
    python3 scripts/programs.py stale-since '{{ ref }}'

# Print NIR file, directory or jar `path` as text; `?` may stand for `$` (a glob).
nir-dump path *args:
    cargo build -q --release -p scivolo-nir-dump
    target/release/scivolo-nir-dump {{ args }} {{ path }}

# Diff the NIR of class `class` (`?` standing for `$`) of Scala file `file` as Scala Native and
# the lowering compile it, definitions sorted, with positions; Scala Native's from the test
# program of that name, else compiled with Scala 3.9.0 into tmp/sn/3.9.0/ when older than `file`.
nir-cmp file class:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build -q --release -p scivolo-nir-dump -p scivolo-lower
    base=$(basename '{{ file }}' .scala)
    sn="tests/programs/$base"
    if [ ! -d "$sn" ]; then
        sn="tmp/sn/3.9.0/$base"
        if [ ! -d "$sn" ] || [ '{{ file }}' -nt "$sn" ]; then
            rm -rf "$sn"
            scala-cli --power compile --native --native-version 0.5.12 -S 3.9.0 --server=false -d "$sn" '{{ file }}' > /dev/null
        fi
    fi
    target/release/scivolo-lower '{{ file }}' "tmp/lowered/$base" > /dev/null
    dump() { target/release/scivolo-nir-dump --debug --sorted "$1" | grep -v '^// \|^# '; }
    if diff <(dump $sn/{{ class }}.nir) <(dump tmp/lowered/$base/{{ class }}.nir); then
        echo "same: {{ class }}"
    fi

# Search the files under `paths` (default: the repository but target/ and .git/) for extended
# regular expression `pattern`, with line numbers; `.` may stand for `$`.
grep pattern *paths='.':
    grep -rnE --exclude-dir=target --exclude-dir=.git -e '{{ pattern }}' {{ paths }}

# List the files under `dir` (default: the repository but target/ and .git/) whose name matches
# glob `name`; `?` may stand for `$`.
files name dir='.':
    find {{ dir }} \( -name target -o -name .git \) -prune -o -name '{{ name }}' -print

# Compile the library overlay (overlay/src) with Scala Native into its committed NIR (overlay/nir).
overlay:
    python3 scripts/overlay.py build

# Whether the committed overlay NIR differs from a rebuild.
overlay-stale:
    cargo build -q --release -p scivolo-nir-dump
    python3 scripts/overlay.py stale

# Compile Scala Native's unit tests (`all`, or directories) and tests/junit into NIR.
sn-build *dirs:
    scripts/sn-tests.sh {{ dirs }}

# Wall time of test program `name` over `runs` runs, after one warm-up run.
bench name runs="10" *args: release
    python3 scripts/bench.py {{ name }} {{ runs }} {{ args }}

# Like bench, alternating runs between the scivolo binary `base` and the release VM.
bench-base base name runs="10" *args: release
    python3 scripts/bench.py --base {{ base }} {{ name }} {{ runs }} {{ args }}

# Like bench, alternating cold runs and runs from a fresh heap snapshot (tmp/bench-snapshot.bin).
bench-snapshot name runs="10" *args: release
    rm -f tmp/bench-snapshot.bin
    python3 scripts/bench.py --snapshot tmp/bench-snapshot.bin {{ name }} {{ runs }} {{ args }}

# The test programs started from a heap snapshot; `snapshot-diff programs` with it compares them
# with a cold baseline.
check-snapshot *names: release
    rm -f tmp/check-snapshot.bin
    SCIVOLO_SNAPSHOT=tmp/check-snapshot.bin target/release/scivolo --cp tests/programs/hello hello > /dev/null
    SCIVOLO_SNAPSHOT=$(pwd)/tmp/check-snapshot.bin python3 scripts/program_runs.py check {{ names }}

# Edit-then-run time of Scala script `file` with the strict frontend, cache miss and hit.
bench-edit file runs="10": release
    python3 scripts/bench_edit.py {{ file }} {{ runs }}

# Like bench-edit, alternating runs between the scivolo binary `base` and the release one.
bench-edit-base base file runs="10": release
    python3 scripts/bench_edit.py --base {{ base }} {{ file }} {{ runs }}

# Run test program `name` with call and instruction counts on stderr (`SCIVOLO_PROFILE`).
vm-profile name *args: release
    SCIVOLO_PROFILE=1 target/release/scivolo --cp tests/programs/{{ name }} {{ name }} {{ args }}

# Lower each Scala file of Scala Native library `module` (nativelib, clib, posixlib, windowslib,
# javalib, auxlib or all; files whose paths contain `filter`) through the own frontend and compare
# its NIR with the published jar's; results under tmp/lib/<module>.
lib-compile module filter="": fetch-scala-native
    cargo build -q --release -p scivolo-lower -p scivolo-nir-dump
    python3 scripts/lib-compile.py '{{ module }}' '{{ filter }}'

# Save lib-compile's results of `module` as the baseline whose worsened files it lists.
lib-baseline module:
    for m in $([ '{{ module }}' = all ] && echo nativelib clib posixlib windowslib javalib auxlib || echo '{{ module }}'); do cp tmp/lib/$m/results.tsv tmp/lib/$m/baseline.tsv; done

# Run a `.scala` or `.sc` file with call and instruction counts on stderr.
[no-cd]
script-profile file *args: release
    SCIVOLO_PROFILE=1 {{ justfile_directory() }}/target/release/scivolo {{ file }} {{ args }}

# Check out dotty's tests/run at the Scala version the VM runs into ref/scala3 (M4).
dotty-fetch:
    test -d ref/scala3 || (git clone -q --depth 1 --branch 3.9.0 --filter=blob:none --sparse https://github.com/scala/scala3 ref/scala3 && git -C ref/scala3 sparse-checkout set tests/run)

# Add dotty's compiler sources (typer, transforms) to ref/scala3.
dotty-sources: dotty-fetch
    git -C ref/scala3 sparse-checkout add compiler/src

# Add dotty's tests/run-macros to ref/scala3, for `just dotty-lower-report "" run-macros`.
dotty-macros: dotty-fetch
    git -C ref/scala3 sparse-checkout add tests/run-macros

# Compile dotty's tests/run (those whose names contain `filter`) to NIR under tmp/dotty.
dotty-build filter="": dotty-fetch
    python3 scripts/dotty-tests.py build {{ filter }}

# Run the compiled dotty tests; report.md and results.tsv under tmp/dotty.
dotty-report filter="": release
    python3 scripts/dotty-tests.py report {{ filter }}

# Run the dotty tests (default: all) through the Rust typer, the lowering and the VM, against
# the outcomes of dotty-report; lower-report.md and lower-results.tsv under tmp/dotty. `dir`
# another directory of dotty's tests (run-macros: `just dotty-macros`), reported in
# lower-report-<dir>.md and lower-results-<dir>.tsv.
dotty-lower-report filter="" dir="run": release
    cargo build -q --release -p scivolo-lower
    python3 scripts/dotty-lower.py '{{ filter }}' '{{ dir }}'

# Link the dotty tests failing in the VM as Scala Native binaries and run them, for dotty-report.
dotty-native filter="":
    python3 scripts/dotty-tests.py native {{ filter }}

# Check out scala/scala's test/junit and testkit at the standard library Scala 3.9 builds on (M5).
m5-fetch:
    test -d ref/scala2 || (git clone -q --depth 1 --branch v2.13.18 --filter=blob:none --sparse https://github.com/scala/scala ref/scala2 && git -C ref/scala2 sparse-checkout set test/junit src/testkit)

# Compile scala/scala's JUnit tests, minus Scala Native's denylist, to NIR under tmp/m5.
m5-build: m5-fetch fetch-scala-native
    python3 scripts/m5-tests.py build

# Run the M5 test classes (those whose names contain `filter`) in the VM; report under tmp/m5.
m5-report filter="": release
    python3 scripts/m5-tests.py report {{ filter }}

# Check out dotty 3.9.0's parser sources, tests/pos, tests/neg and the spec into ref/dotty (#15).
syntax-fetch:
    test -d ref/dotty || (git clone -q --depth 1 --branch 3.9.0 --filter=blob:none --sparse https://github.com/scala/scala3 ref/dotty && git -C ref/dotty sparse-checkout set compiler/src/dotty/tools/dotc/parsing compiler/src/dotty/tools/dotc/ast compiler/src/dotty/tools/dotc/util compiler/src/dotty/tools/dotc/config tests/pos tests/neg docs/_spec docs/_docs/reference)

# Record dotty's tokens, trees or desugared trees (`mode`) for tests/pos and tests/neg under
# tmp/syntax (JVM).
syntax-oracle mode="trees" filter="": syntax-fetch
    python3 scripts/syntax-tests.py {{ mode }} oracle {{ filter }}

# Compare the Rust scanner, parser or desugaring with the recorded output; report.md and results.tsv
# under tmp/syntax/3.9.0/<mode>.
syntax-report mode="trees" filter="":
    cargo build -q --release -p scivolo-syntax
    python3 scripts/syntax-tests.py {{ mode }} report {{ filter }}

# The difference between dotty's and the Rust output of `mode` for one file.
syntax-diff mode path:
    cargo build -q --release -p scivolo-syntax
    python3 scripts/syntax-tests.py {{ mode }} diff {{ path }}

# Record dotty's desugared trees of the golden tests in crates/syntax/tests/desugar (JVM).
desugar-checks:
    python3 scripts/syntax-tests.py desugar checks crates/syntax/tests/desugar

# Record dotty's typed trees of tests/programs and tests/pos under tmp/typer (JVM).
typer-oracle filter="": syntax-fetch
    python3 scripts/typer-tests.py oracle {{ filter }}

# Compare the Rust typer with the recorded typed trees; report.md and results.tsv under
# tmp/typer/3.9.0.
typer-report filter="":
    cargo build -q --release -p scivolo-typer
    python3 scripts/typer-tests.py report {{ filter }}

# Type tests/programs, tests/pos and tests/neg (those whose paths contain `filter`) with errors
# taking precedence over unsupported constructs, and list the false errors: those at an offset
# where dotty reports none (neg: by typer-oracle's output). Results in tmp/typer/3.9.0/stress.tsv.
# Not in `ci`, which lacks ref/dotty and the oracle output: about 1.5 min for tests/pos and
# 2.5 min for all on 12 cores.
typer-stress filter="":
    cargo build -q --release -p scivolo-typer
    python3 scripts/typer-tests.py stress {{ filter }}

# Save the typer report's results (default: the latest) as the baseline of typer-progress.
typer-baseline from="tmp/typer/3.9.0/results.tsv":
    cp {{ from }} tmp/typer/baseline.tsv

# Run the typer report and list the files that changed outcome against the baseline.
typer-progress: typer-report
    bash scripts/typer-progress.sh

# Before a refactor that must not change behaviour: record the outcomes of the oracles (default:
# all) under tmp/snapshot. `typer` (typer-report), `lower` (dotty-lower-report), `tokens`,
# `trees` and `desugar` (syntax-report), and `programs`: each test program's result in the
# release VM against its `.check` file, as `just check` runs it, with a hash of its output.
# An oracle whose recorded dotty output or checkout is missing is skipped; its old snapshot stays.
snapshot *oracles: release
    cargo build -q --release -p scivolo-typer -p scivolo-lower -p scivolo-syntax
    python3 scripts/snapshot.py record {{ oracles }}

# After the refactor: rerun the oracles, list the rows that differ from the snapshot as worsened,
# improved and other changes, and fail on any of them, on a skipped oracle that has a snapshot,
# and when no oracle was compared.
snapshot-check *oracles: release
    cargo build -q --release -p scivolo-typer -p scivolo-lower -p scivolo-syntax
    python3 scripts/snapshot.py check {{ oracles }}

# After a feature: list the rows that differ from the snapshot like `snapshot-check`, without
# failing on them.
snapshot-diff *oracles: release
    cargo build -q --release -p scivolo-typer -p scivolo-lower -p scivolo-syntax
    python3 scripts/snapshot.py diff {{ oracles }}

# In a linked worktree (e.g. an agent's): link the main checkout's ref/ and copy the oracle data
# the snapshots read from its tmp/.
worktree-setup:
    #!/usr/bin/env bash
    set -eu
    main=$(dirname "$(git rev-parse --path-format=absolute --git-common-dir)")
    if [ "$main" = "$PWD" ]; then echo "the main checkout needs no setup"; exit 0; fi
    [ -e ref ] || ln -s "$main/ref" ref
    for f in tmp/typer/3.9.0/expected.txt tmp/dotty/3.9.0-0.5.12/results.tsv; do
        if [ -f "$main/$f" ]; then mkdir -p "$(dirname "$f")"; cp "$main/$f" "$f"
        else echo "not in the main checkout: $f"; fi
    done
    [ -d ref/scala3 ] || echo "not in the main checkout: ref/scala3 (just dotty-fetch there)"

# Format, run the gates, commit every change not ignored with message `msg` and push the branch.
ship msg: fmt gates lint-msrv
    git add -A
    git status --short
    git commit -q -m {{ quote(msg) }}
    git push -q -u origin HEAD

# On a clean main: set the workspace version to `version`, commit it and tag it `v<version>`.
# Pushes nothing.
new-release version:
    #!/usr/bin/env bash
    set -euo pipefail
    v={{ quote(version) }}
    num='(0|[1-9][0-9]*)'
    pre="($num|[0-9]*[A-Za-z-][0-9A-Za-z-]*)"
    [[ "$v" =~ ^$num\.$num\.$num(-$pre(\.$pre)*)?$ ]] || { echo "not a version: $v"; exit 1; }
    [ "$(git branch --show-current)" = main ] || { echo "not on main"; exit 1; }
    [ -z "$(git status --porcelain)" ] || { echo "the working tree is not clean"; exit 1; }
    if git rev-parse -q --verify "refs/tags/v$v" > /dev/null; then echo "tag v$v exists"; exit 1; fi
    trap 'git checkout -q -- Cargo.toml Cargo.lock README.md; echo "release failed; files restored"' ERR
    perl -pi -e '$s = /^\[workspace\.package\]/ ? 1 : /^\[/ ? 0 : $s;
        s/^version = "[^"]*"/version = "'"$v"'"/ if $s' Cargo.toml
    grep -q "^version = \"$v\"$" Cargo.toml || { echo "no workspace version in Cargo.toml"; false; }
    perl -pi -e 's/^Welcome to Scivolo \S+/Welcome to Scivolo '"$v"'/' README.md
    cargo update -q --workspace
    git commit -q -am "Release $v"
    trap - ERR
    git tag -a "v$v" -m "scivolo $v"
    echo "created commit \"Release $v\" and tag v$v"

# Print REPL transcripts (default: tests/repl) as Scala 3.9's REPL answers their inputs (JVM).
repl-oracle *files:
    f="{{ files }}"; [ -z "$f" ] && f=$(ls tests/repl/*.check); \
    scala-cli --power run --server=false --quiet --workspace tmp/repl-oracle-ws scripts/repl-oracle -- $f

# Rewrite completion corpora (default: tests/repl-completion) with the labels Scala 3.9's REPL
# lists on Tab (JVM, JDK 21).
completion-oracle *files:
    f="{{ files }}"; [ -z "$f" ] && f=$(ls tests/repl-completion/*.tab); \
    scala-cli --power run --server=false --quiet --workspace tmp/completion-oracle-ws scripts/completion-oracle -- $f

# Record dotty's highlighting of tests/highlight/cases.txt in cases.check (JVM).
highlight-oracle:
    mkdir -p tmp
    scala-cli --power run --server=false --quiet --workspace tmp/highlight-oracle-ws scripts/highlight-oracle -- tests/highlight/cases.txt > tmp/highlight-cases.check
    mv tmp/highlight-cases.check tests/highlight/cases.check

# Record dotty's typed trees of the golden tests in crates/typer/tests/typed, and its `Inlined`
# trees after the `inlining` phase of those in crates/typer/tests/inlined and
# crates/macros/tests/inlined (JVM).
typer-checks:
    python3 scripts/typer-tests.py checks crates/typer/tests/typed
    python3 scripts/typer-tests.py checks crates/typer/tests/inlined
    python3 scripts/typer-tests.py checks crates/macros/tests/inlined

# The difference between dotty's and the Rust typed trees of one file.
typer-diff path:
    cargo build -q --release -p scivolo-typer
    python3 scripts/typer-tests.py diff {{ path }}

# Type Scala files with dotty's oracle and the Rust typer, and compare the typed trees.
typer-compare +files:
    cargo build -q --release -p scivolo-typer
    python3 scripts/typer-compare.py {{ files }}

# The Rust typer's typed trees of Scala files, or why they fall back.
typer-dump +files:
    cargo build -q --release -p scivolo-typer
    mkdir -p tmp
    printf '%s\n' {{ files }} > tmp/typer-dump.list
    target/release/scivolo-typer-dump --list tmp/typer-dump.list

# Lower a Scala file through the Rust frontend into NIR files in tmp/lowered/<file name>.
lower file:
    cargo build -q --release -p scivolo-lower
    target/release/scivolo-lower {{ file }} tmp/lowered/$(basename {{ file }} .scala)

# Lower a Scala file through the Rust frontend and run its `main` (default: the file name) in
# the release VM, stopped after `SCIVOLO_TIMEOUT` seconds (default 60).
lower-run file main="": release (lower file)
    m="{{ main }}"; [ -z "$m" ] && m=$(basename {{ file }} .scala); \
    SCIVOLO_TIMEOUT=${SCIVOLO_TIMEOUT:-60} target/release/scivolo --cp tmp/lowered/$(basename {{ file }} .scala) $m

# Run Scala Native test classes whose names contain the arguments (`-part` excludes).
sn-run *filters: release
    scripts/sn-run.sh {{ filters }}

# The per-group Scala Native test report, each group stopped after `timeout` seconds.
sn-report timeout="300": release
    SCIVOLO_TIMEOUT={{ timeout }} scripts/sn-report.sh

release:
    cargo build -q --release -p scivolo

# Copy the library jars (Scala Native's, the Scala standard library) from the Coursier cache
# into ref/libs.
libs:
    mkdir -p ref/libs
    for j in $(find ~/Library/Caches/Coursier/v1 ~/.cache/coursier/v1 \
        \( -path '*org/scala-native/*' \( -name '*_native0.5_3-0.5.12.jar' \
        -o -name 'scalalib_native0.5_3-3.9.0%2B0.5.12.jar' \) -o -name 'scala-library-3.9.0.jar' \) \
        ! -name '*-sources.jar' 2>/dev/null); do cp "$j" ref/libs/; done

# Download the library jars (Scala Native's, the Scala standard library) from Maven Central
# into ref/libs.
fetch-libs:
    scripts/fetch-libs.sh

# Let dotc decide the subtype pairs of crates/tasty's fixture (scala-cli).
subtype-oracle:
    scripts/subtype-oracle.py

# Compile the library of inline methods that crates/typer/tests/inlined/libbodies.scala calls.
inline-fixture:
    scala-cli --power package --library --server=false -S 3.9.0 crates/typer/tests/inlined/lib/Lib.scala -o crates/typer/tests/inlined/lib/lib.jar -f

# Compile the macro library tests/macros/lib with Scala Native into tests/macros/lib/macros.jar:
# NIR and TASTy, as a `--dep` jar holds them, and the class files dotty's oracle runs.
macro-fixture:
    #!/usr/bin/env bash
    set -euo pipefail
    rm -rf tmp/macro-fixture
    scala-cli --power compile --quiet --native --native-version 0.5.12 --scala 3.9.0 --server=false \
        --scalac-option -sourceroot --scalac-option {{ justfile_directory() }} \
        --workspace tmp/macro-fixture/ws -d tmp/macro-fixture/out tests/macros/lib/Macros.scala
    rm -f tests/macros/lib/macros.jar
    (cd tmp/macro-fixture/out && zip -q -X -r {{ justfile_directory() }}/tests/macros/lib/macros.jar . -i '*/' '*.nir' '*.tasty' '*.class')

# Rebuild the checked-in libs/javalib-tasty-<version>.jar (sbt, JDK) from ref/scala-native.
javalib-tasty:
    scripts/javalib-tasty.sh

# Check out Scala Native v0.5.12 into ref/scala-native.
fetch-scala-native:
    scripts/fetch-scala-native.sh

# Everything a pipeline job runs.
ci: fetch-libs gates check

# Scala Native's whole unit-test suite, as the pipeline's suite job runs it.
ci-suite: fetch-libs fetch-scala-native (sn-build "all") (sn-report "1800")

# The three gates that must pass before every commit.
gates: fmt-check lint test

# Apply formatting.
fmt:
    cargo fmt --all

# Check formatting without writing.
fmt-check:
    cargo fmt --all -- --check

# Lint every target, warnings denied.
lint *args:
    cargo clippy --workspace --all-targets {{ locked }} {{ args }} -- -D warnings

# Lint with the minimum Rust release (`rust-version`).
lint-msrv:
    rustup run 1.97.0 cargo clippy --workspace --all-targets -- -D warnings

# Run the whole test suite.
test *args:
    cargo test --workspace {{ locked }} {{ args }}

# Open a merge request of the current branch into `target`, described by tmp/mr.md.
mr-create title target="main" *flags:
    glab mr create --source-branch "$(git branch --show-current)" --target-branch '{{ target }}' --title '{{ title }}' --description "$(cat tmp/mr.md)" {{ flags }}

# Open a merge request of branch `source` into main, described by tmp/mr.md.
mr-create-from source title *flags:
    glab mr create --source-branch '{{ source }}' --target-branch main --title '{{ title }}' --description "$(cat tmp/mr.md)" {{ flags }}

# Create an issue with comma-separated `labels`, described by tmp/issue.md.
issue-create title labels:
    glab issue create --title '{{ title }}' --label '{{ labels }}' --description "$(cat tmp/issue.md)"

# Comment on issue `id` with the text in tmp/issue.md.
issue-note id:
    glab issue note {{ id }} --message "$(cat tmp/issue.md)"
