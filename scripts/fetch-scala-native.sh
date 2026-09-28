#!/usr/bin/env bash
# Checks out Scala Native v0.5.12, whose unit tests scripts/sn-tests.sh compiles, into
# ref/scala-native.
set -euo pipefail
dir=$(cd "$(dirname "$0")/.." && pwd)/ref/scala-native
commit=65dd513df188300d0180514ad828308facc5ca61
if [ ! -d "$dir" ]; then
  git clone -q --depth 1 --branch v0.5.12 https://github.com/scala-native/scala-native.git "$dir"
fi
if [ ! -e "$dir/.git" ] || [ "$(git -C "$dir" rev-parse HEAD)" != "$commit" ]; then
  echo "$dir is not at v0.5.12 ($commit)" >&2
  exit 1
fi
