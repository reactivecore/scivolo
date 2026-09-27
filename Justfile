# Build, lint and test commands. `just --list` shows everything.

# CI must not update the lockfile; a developer shell may.
locked := if env_var_or_default("CI", "") == "" { "" } else { "--locked" }

# Scala Native's library jars, copied into the repo by `just libs`.
export SCALAI_LIBS := justfile_directory() + "/ref/libs"

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

# Compare test programs (default: all) with their `.check` files, with wall time.
check *names: release
    #!/usr/bin/env bash
    names="{{ names }}"; [ -z "$names" ] && names=$(ls tests/programs)
    for p in $names; do
      start=$(date +%s.%N)
      out=$(SCALAI_TIMEOUT=${SCALAI_TIMEOUT:-60} target/release/scalai --cp tests/programs/$p $p 2>&1)
      t=$(echo "$(date +%s.%N) - $start" | bc)
      if [ "$out" == "$(cat tests/programs/$p/$p.check)" ]; then echo "ok   $p ${t}s"; else echo "DIFF $p ${t}s"; fi
    done

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
