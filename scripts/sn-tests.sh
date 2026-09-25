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
if [ "${dirs[0]}" = all ]; then
  for d in scala scala-3 scala-2.13+ require-jdk9 require-jdk10 require-jdk11 require-jdk12 require-jdk14 \
    require-jdk15 require-jdk16 require-jdk17 require-scala3-jdk9 require-scala3-jdk10 \
    require-scala3-jdk12 require-scala3-jdk17 require-scala3.2 require-scala3.4; do
    cp -R "$tests/shared/src/test/$d/." "$work/src/"
  done
  cp -R "$tests/native/src/test/scala/." "$work/src/"
  for t in $(find "$work/src" -name '*.scala.template'); do
    out=${t%.template}
    cp "$t" "$out"
    for v in 9 10 11 12 13 14 15 16 17; do
      sed -i.bak -e "s|/\* >>REQUIRE-JDK-$v||g" -e "s|<<REQUIRE-JDK-$v \*/||g" "$out"
    done
    rm -f "$out.bak" "$t"
  done
else
  for d in "${dirs[@]}"; do
    mkdir -p "$work/src/$d"
    cp -R "$tests/shared/src/test/scala/$d/." "$work/src/$d/"
  done
  mkdir -p "$work/src/utils"
  cp "$tests"/shared/src/test/scala/utils/{AssertThrows,AssumesHelper}.scala "$work/src/utils/"
  cp "$tests/native/src/test/scala/org/scalanative/testsuite/utils/Platform.scala" "$work/src/utils/"
fi
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
