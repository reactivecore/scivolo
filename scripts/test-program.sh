#!/usr/bin/env bash
# Compiles tests/programs/<name>/<name>.scala with Scala Native, keeps its NIR next to the source
# and records the native binary's output as <name>.check.
set -euo pipefail
name=$1
dir=$(cd "$(dirname "$0")/.." && pwd)/tests/programs/$name
work=$(cd "$(dirname "$0")/.." && pwd)/tmp/programs/$name
mkdir -p "$work"
cp "$dir/$name.scala" "$work/"
cd "$work"
scala-cli --power package --native --native-version 0.5.12 -f -o "$work/bin" "$name.scala" >/dev/null
classes=$(find "$work/.scala-build" -path '*/classes/main' -type d | head -1)
rm -f "$dir"/*.nir
cp "$classes"/*.nir "$dir/"
"$work/bin" > "$dir/$name.check"
