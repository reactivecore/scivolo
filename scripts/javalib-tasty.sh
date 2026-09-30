#!/usr/bin/env bash
# Builds Scala Native's javalib with sbt and packs its TASTy into the checked-in
# libs/javalib-tasty-<version>.jar, the version being ref/scala-native's tag.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
native="$root/ref/scala-native"
[ -d "$native" ] || { echo "ref/scala-native missing: run \`just fetch-scala-native\`" >&2; exit 1; }
version=$(git -C "$native" describe --tags --exact-match | sed 's/^v//')
out="$root/libs/javalib-tasty-$version.jar"
(cd "$native" && sbt -batch javalib3/clean javalib3/compile)
dirs=$(find "$native/javalib/.3/target" -type d -name classes -path '*scala-3*')
[ "$(echo "$dirs" | grep -c .)" -eq 1 ] || { echo "expected one javalib classes directory, found: $dirs" >&2; exit 1; }
mkdir -p "$root/libs"
rm -f "$out"
find "$dirs" -name '*.tasty' -exec touch -t 201001010000 {} +
(cd "$dirs" && find . -name '*.tasty' | sed 's|^\./||' | sort | zip -q -X "$out" -@)
echo "$out: $(unzip -l "$out" | tail -1)"
