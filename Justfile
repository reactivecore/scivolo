# Build, lint and test commands. `just --list` shows everything.

# CI must not update the lockfile; a developer shell may.
locked := if env_var_or_default("CI", "") == "" { "" } else { "--locked" }

# Scala Native's library jars, copied into the repo by `just libs`.
export SCALAI_LIBS := justfile_directory() + "/ref/libs"

# The library image of development runs.
export SCALAI_CACHE := justfile_directory() + "/tmp/cache"

default: gates

# Compile Scala Native's shared unit tests (default: the language tests in `scala`) and run
# them in the VM. Needs `ref/scala-native` (v0.5.12) and scala-cli.
sn-tests *dirs:
    scripts/sn-tests.sh {{ dirs }}
    cargo build -q --release -p scalai
    scripts/sn-run.sh

# Run test program `name` from tests/programs in the release VM, stopped after `SCALAI_TIMEOUT`
# seconds (default 60).
run name *args: release
    SCALAI_TIMEOUT=${SCALAI_TIMEOUT:-60} target/release/scalai --cp tests/programs/{{ name }} {{ name }} {{ args }}

# Run a `.scala` or `.sc` file (relative to the current directory), compiled through scala-cli
# unless cached.
[no-cd]
script file *args: release
    {{ justfile_directory() }}/target/release/scalai {{ file }} {{ args }}

# Start the REPL in the release build.
[no-cd]
repl *args: release
    {{ justfile_directory() }}/target/release/scalai {{ args }}

# Compare test programs (default: all) with their `.check` files, with wall time.
check *names: release
    #!/usr/bin/env bash
    names="{{ names }}"; [ -z "$names" ] && names=$(find tests/programs -mindepth 1 -maxdepth 1 -type d -exec basename {} \; | sort)
    failed=0
    for p in $names; do
      start=$(date +%s.%N)
      out=$(SCALAI_TIMEOUT=${SCALAI_TIMEOUT:-60} target/release/scalai --cp tests/programs/$p $p 2>&1)
      t=$(echo "$(date +%s.%N) - $start" | bc)
      if [ "$out" == "$(cat tests/programs/$p/$p.check)" ]; then echo "ok   $p ${t}s"; else echo "DIFF $p ${t}s"; failed=1; fi
    done
    exit $failed

# Compile test program `name` with Scala Native into its committed NIR and `.check`.
program-update name:
    python3 scripts/programs.py update {{ name }}

# List test programs (default: all) whose committed NIR or `.check` differs from a rebuild.
programs-stale *names:
    cargo build -q --release -p nir-dump
    python3 scripts/programs.py stale {{ names }}

# Print NIR file, directory or jar `path` as text.
nir-dump path *args:
    cargo build -q --release -p nir-dump
    target/release/nir-dump {{ args }} '{{ path }}'

# Diff the NIR of class `class` (`?` standing for `$`) of Scala file `file` as Scala Native and
# the lowering compile it, definitions sorted, with positions; Scala Native's from the test
# program of that name, else compiled into tmp/sn/ when older than `file`.
nir-cmp file class:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build -q --release -p nir-dump -p lower
    base=$(basename '{{ file }}' .scala)
    target/release/lower '{{ file }}' "tmp/lowered/$base" > /dev/null
    sn="tests/programs/$base"
    if [ ! -d "$sn" ]; then
        sn="tmp/sn/$base"
        if [ ! -d "$sn" ] || [ '{{ file }}' -nt "$sn" ]; then
            rm -rf "$sn"
            scala-cli --power compile --native --native-version 0.5.12 --server=false -d "$sn" '{{ file }}' > /dev/null
        fi
    fi
    dump() { target/release/nir-dump --debug --sorted "$1" | grep -v '^// \|^# '; }
    if diff <(dump $sn/{{ class }}.nir) <(dump tmp/lowered/$base/{{ class }}.nir); then
        echo "same: {{ class }}"
    fi

# Compile the library overlay (overlay/src) with Scala Native into its committed NIR (overlay/nir).
overlay:
    python3 scripts/overlay.py build

# Whether the committed overlay NIR differs from a rebuild.
overlay-stale:
    cargo build -q --release -p nir-dump
    python3 scripts/overlay.py stale

# Compile Scala Native's unit tests (`all`, or directories) and tests/junit into NIR.
sn-build *dirs:
    scripts/sn-tests.sh {{ dirs }}

# Wall time of test program `name` over `runs` runs, after one warm-up run.
bench name runs="10" *args: release
    python3 scripts/bench.py {{ name }} {{ runs }} {{ args }}

# Like bench, alternating runs between the scalai binary `base` and the release VM.
bench-base base name runs="10" *args: release
    python3 scripts/bench.py --base {{ base }} {{ name }} {{ runs }} {{ args }}

# Run test program `name` with call and instruction counts on stderr (`SCALAI_PROFILE`).
vm-profile name *args: release
    SCALAI_PROFILE=1 target/release/scalai --cp tests/programs/{{ name }} {{ name }} {{ args }}

# Run a `.scala` or `.sc` file with call and instruction counts on stderr.
[no-cd]
script-profile file *args: release
    SCALAI_PROFILE=1 {{ justfile_directory() }}/target/release/scalai {{ file }} {{ args }}

# Check out dotty's tests/run at the Scala version the VM runs into ref/scala3 (M4).
dotty-fetch:
    test -d ref/scala3 || (git clone -q --depth 1 --branch 3.9.0 --filter=blob:none --sparse https://github.com/scala/scala3 ref/scala3 && git -C ref/scala3 sparse-checkout set tests/run)

# Compile dotty's tests/run (those whose names contain `filter`) to NIR under tmp/dotty.
dotty-build filter="": dotty-fetch
    python3 scripts/dotty-tests.py build {{ filter }}

# Run the compiled dotty tests; report.md and results.tsv under tmp/dotty.
dotty-report filter="": release
    python3 scripts/dotty-tests.py report {{ filter }}

# Run the dotty tests (default: all) through the Rust typer, the lowering and the VM, against
# the outcomes of dotty-report; lower-report.md and lower-results.tsv under tmp/dotty.
dotty-lower-report filter="": release
    cargo build -q --release -p lower
    python3 scripts/dotty-lower.py {{ filter }}

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
    cargo build -q --release -p syntax
    python3 scripts/syntax-tests.py {{ mode }} report {{ filter }}

# The difference between dotty's and the Rust output of `mode` for one file.
syntax-diff mode path:
    cargo build -q --release -p syntax
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
    cargo build -q --release -p typer
    python3 scripts/typer-tests.py report {{ filter }}

# Save the typer report's results (default: the latest) as the baseline of typer-progress.
typer-baseline from="tmp/typer/3.9.0/results.tsv":
    cp {{ from }} tmp/typer/baseline.tsv

# Run the typer report and list the files that changed outcome against the baseline.
typer-progress: typer-report
    bash scripts/typer-progress.sh

# Format, run the gates, commit every change not ignored with message `msg` and push the branch.
ship msg: fmt gates lint-msrv
    git add -A
    git status --short
    git commit -q -m {{ quote(msg) }}
    git push -q -u origin HEAD

# Print REPL transcripts (default: tests/repl) as Scala 3.9's REPL answers their inputs (JVM).
repl-oracle *files:
    f="{{ files }}"; [ -z "$f" ] && f=$(ls tests/repl/*.check); \
    scala-cli --power run --server=false --quiet --workspace tmp/repl-oracle-ws scripts/repl-oracle -- $f

# Record dotty's typed trees of the golden tests in crates/typer/tests/typed (JVM).
typer-checks:
    python3 scripts/typer-tests.py checks crates/typer/tests/typed

# The difference between dotty's and the Rust typed trees of one file.
typer-diff path:
    cargo build -q --release -p typer
    python3 scripts/typer-tests.py diff {{ path }}

# Type Scala files with dotty's oracle and the Rust typer, and compare the typed trees.
typer-compare +files:
    cargo build -q --release -p typer
    python3 scripts/typer-compare.py {{ files }}

# The Rust typer's typed trees of Scala files, or why they fall back.
typer-dump +files:
    cargo build -q --release -p typer
    mkdir -p tmp
    printf '%s\n' {{ files }} > tmp/typer-dump.list
    target/release/typer-dump --list tmp/typer-dump.list

# Lower a Scala file through the Rust frontend into NIR files in tmp/lowered/<file name>.
lower file:
    cargo build -q --release -p lower
    target/release/lower {{ file }} tmp/lowered/$(basename {{ file }} .scala)

# Lower a Scala file through the Rust frontend and run its `main` (default: the file name) in
# the release VM, stopped after `SCALAI_TIMEOUT` seconds (default 60).
lower-run file main="": release (lower file)
    m="{{ main }}"; [ -z "$m" ] && m=$(basename {{ file }} .scala); \
    SCALAI_TIMEOUT=${SCALAI_TIMEOUT:-60} target/release/scalai --cp tmp/lowered/$(basename {{ file }} .scala) $m

# Run Scala Native test classes whose names contain the arguments (`-part` excludes).
sn-run *filters: release
    scripts/sn-run.sh {{ filters }}

# The per-group Scala Native test report, each group stopped after `timeout` seconds.
sn-report timeout="300": release
    SCALAI_TIMEOUT={{ timeout }} scripts/sn-report.sh

release:
    cargo build -q --release -p scalai

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

# Lint with the Rust release of the macOS pipeline runner.
lint-msrv:
    cargo +1.93.0 clippy --workspace --all-targets -- -D warnings

# Run the whole test suite.
test *args:
    cargo test --workspace {{ locked }} {{ args }}
