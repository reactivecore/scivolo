#!/usr/bin/env bash
# Compiles Scala Native's library modules with its build's Scala 3 version (sbt, JDK; 3.8.3 for
# 0.5.12) and copies their NIR into tmp/lib/sn/<module>: the oracle of `just lib-compile` in
# place of the published jars, which Scala Native compiles with Scala 3.1.3.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
native="$root/ref/scala-native"
[ -d "$native" ] || { echo "ref/scala-native missing: run \`just fetch-scala-native\`" >&2; exit 1; }
modules=(nativelib clib posixlib windowslib javalib auxlib)
tasks=()
for m in "${modules[@]}"; do tasks+=("${m}3/compile"); done
(cd "$native" && sbt -batch "${tasks[@]}")
for m in "${modules[@]}"; do
  dirs=$(find "$native/$m/.3/target" -type d -name classes -path '*scala-3*')
  [ "$(echo "$dirs" | grep -c .)" -eq 1 ] || { echo "expected one $m classes directory, found: $dirs" >&2; exit 1; }
  out="$root/tmp/lib/sn/$m"
  rm -rf "$out"
  mkdir -p "$out"
  (cd "$dirs" && find . -name '*.nir' | while read -r f; do mkdir -p "$out/$(dirname "$f")"; cp "$f" "$out/$f"; done)
  echo "$m: $(find "$out" -name '*.nir' | wc -l | tr -d ' ') NIR files in $out"
done
