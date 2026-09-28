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

# Compare test programs (default: all) with their `.check` files, with wall time.
check *names: release
    #!/usr/bin/env bash
    names="{{ names }}"; [ -z "$names" ] && names=$(ls tests/programs)
    failed=0
    for p in $names; do
      start=$(date +%s.%N)
      out=$(SCALAI_TIMEOUT=${SCALAI_TIMEOUT:-60} target/release/scalai --cp tests/programs/$p $p 2>&1)
      t=$(echo "$(date +%s.%N) - $start" | bc)
      if [ "$out" == "$(cat tests/programs/$p/$p.check)" ]; then echo "ok   $p ${t}s"; else echo "DIFF $p ${t}s"; failed=1; fi
    done
    exit $failed

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

# Run Scala Native test classes whose names contain the arguments (`-part` excludes).
sn-run *filters: release
    scripts/sn-run.sh {{ filters }}

# The per-group Scala Native test report, each group stopped after `timeout` seconds.
sn-report timeout="300": release
    SCALAI_TIMEOUT={{ timeout }} scripts/sn-report.sh

release:
    cargo build -q --release -p scalai

# Copy the Scala Native library jars from the Coursier cache into ref/libs.
libs:
    mkdir -p ref/libs
    for j in $(find ~/Library/Caches/Coursier/v1 ~/.cache/coursier/v1 -path '*org/scala-native/*' \
        \( -name '*_native0.5_3-0.5.12.jar' -o -name 'scalalib_native0.5_3-3.9.0%2B0.5.12.jar' \) \
        ! -name '*-sources.jar' 2>/dev/null); do cp "$j" ref/libs/; done

# Download the library jars the VM loads from Maven Central into ref/libs.
fetch-libs:
    scripts/fetch-libs.sh

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

# Run the whole test suite.
test *args:
    cargo test --workspace {{ locked }} {{ args }}
