#!/usr/bin/env bash
# Runs the JUnit tests built by scripts/sn-tests.sh in the VM. Arguments select test classes by
# name part; `-part` excludes. SCALAI_TIMEOUT (seconds, default 60) stops a hanging run.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
export SCALAI_LIBS=${SCALAI_LIBS:-$root/ref/libs}
suffix=$([ "$(uname)" = Darwin ] && echo dylib || echo so)
export SCALAI_NATIVE_LIBS=${SCALAI_NATIVE_LIBS:-$root/tmp/sn-tests/libsntest.$suffix}
export SCALAI_TIMEOUT=${SCALAI_TIMEOUT:-60}
# Tests find their resource files relative to the Scala Native repository root.
cd "$root/ref/scala-native"
# The environment Scala Native's build gives its tests (project/Settings.scala, Test / envVars).
export USER=${USER:-$(id -un)} HOME=${HOME:-$(cd ~ && pwd)}
export SCALA_NATIVE_ENV_WITH_EQUALS='1+1=2' SCALA_NATIVE_ENV_WITHOUT_VALUE=
export SCALA_NATIVE_ENV_WITH_UNICODE=$(printf '\342\206\222')
export SCALA_NATIVE_USER_DIR=$(pwd -P)
exec "$root/target/release/scalai" --cp "$root/tmp/sn-tests/nir" \
  --cp "$root/ref/libs/junit-runtime_native0.5_3-0.5.12.jar" \
  --cp "$root/ref/libs/test-interface-sbt-defs_native0.5_3-0.5.12.jar" \
  runner "$@"
