#!/usr/bin/env bash
# Builds Scala Native's javalib with sbt and packs its TASTy into ref/libs/javalib-tasty-0.5.12.jar.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
native="$root/ref/scala-native"
out="$root/ref/libs/javalib-tasty-0.5.12.jar"
[ -d "$native" ] || { echo "ref/scala-native missing: run \`just fetch-scala-native\`" >&2; exit 1; }
(cd "$native" && sbt -batch javalib3/compile)
classes=$(find "$native/javalib/.3/target" -type d -name classes -path '*scala-3*' | head -1)
[ -n "$classes" ] || { echo "javalib classes not found under $native/javalib/.3/target" >&2; exit 1; }
mkdir -p "$root/ref/libs"
rm -f "$out"
(cd "$classes" && find . -name '*.tasty' | sed 's|^\./||' | sort | zip -q -X "$out" -@)
echo "$out: $(unzip -l "$out" | tail -1)"
