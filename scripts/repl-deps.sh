#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -eq 0 ]; then
  echo 'usage: just repl-deps group::artifact::version [coordinate ...]' >&2
  exit 2
fi
if ! command -v coursier >/dev/null; then
  echo 'repl-deps: coursier is not on PATH' >&2
  exit 1
fi

root=$(dirname "$0")/..
export SCALAI_LIBS=${SCALAI_LIBS:-$root/ref/libs}
export SCALAI_CACHE=${SCALAI_CACHE:-$root/tmp/cache}
deps=()
for dep do
  if [[ "$dep" =~ ^([^:]+)::([^:]+)::([^:]+)$ ]]; then
    dep="${BASH_REMATCH[1]}:${BASH_REMATCH[2]}_native0.5_3:${BASH_REMATCH[3]}"
  fi
  deps+=("$dep")
done
jars=$(coursier fetch --exclude 'org.scala-lang:*' --exclude 'org.scala-native:*' "${deps[@]}")
args=("$root/target/release/scalai")
while IFS= read -r jar; do
  if [ -n "$jar" ]; then args+=(--cp "$jar"); fi
done <<< "$jars"
exec "${args[@]}"
