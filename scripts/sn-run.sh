#!/usr/bin/env bash
# Runs the JUnit tests built by scripts/sn-tests.sh in the VM. Arguments select test classes by
# name part; `-part` excludes. SCALAI_TIMEOUT (seconds, default 600) stops a hanging run.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
export SCALAI_LIBS=${SCALAI_LIBS:-$root/ref/libs}
export SCALAI_NATIVE_LIBS=${SCALAI_NATIVE_LIBS:-$root/tmp/sn-tests/libsntest.dylib}
export SCALAI_TIMEOUT=${SCALAI_TIMEOUT:-600}
# Tests find their resource files relative to the Scala Native repository root.
cd "$root/ref/scala-native"
exec "$root/target/release/scalai" --cp "$root/tmp/sn-tests/nir" \
  --cp "$root/ref/libs/junit-runtime_native0.5_3-0.5.12.jar" \
  --cp "$root/ref/libs/test-interface-sbt-defs_native0.5_3-0.5.12.jar" \
  runner "$@"
