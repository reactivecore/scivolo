#!/usr/bin/env bash
# Compiles Scala Native's shared unit tests (ref/scala-native) with its JUnit plugin plus
# tests/junit/*.scala into NIR under tmp/sn-tests/nir, and copies the JUnit runtime jars
# into ref/libs. Usage: scripts/sn-tests.sh [test dirs relative to unit-tests/shared/src/test/scala]
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
tests=$root/ref/scala-native/unit-tests
work=$root/tmp/sn-tests
dirs=("${@:-scala}")

rm -rf "$work/src" "$work/nir"
mkdir -p "$work/src" "$work/nir"
for d in "${dirs[@]}"; do
  mkdir -p "$work/src/$d"
  cp -R "$tests/shared/src/test/scala/$d/." "$work/src/$d/"
done
mkdir -p "$work/src/utils"
cp "$tests"/shared/src/test/scala/utils/{AssertThrows,AssumesHelper}.scala "$work/src/utils/"
cp "$tests/native/src/test/scala/org/scalanative/testsuite/utils/Platform.scala" "$work/src/utils/"
cp "$root"/tests/junit/*.scala "$work/src/"

cd "$work"
opts=(--power --native --native-version 0.5.12 -S 3.9.0
  --dep org.scala-native::junit-runtime::0.5.12
  --compiler-plugin org.scala-native:::junit-plugin:0.5.12)
scala-cli compile "${opts[@]}" src
classes=$(find . -path '*/.scala-build/*/classes/main' -type d | head -1)
(cd "$classes" && find . -name '*.nir' | cpio -pdm --quiet "$work/nir")
for jar in $(scala-cli compile "${opts[@]}" --print-class-path src | tr ':' '\n' | grep -E 'junit-runtime|test-interface'); do
  cp "$jar" "$root/ref/libs/"
done
echo "$(find "$work/nir" -name '*bootstrapper*.nir' | wc -l | tr -d ' ') test classes in $work/nir"
