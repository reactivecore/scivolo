#!/usr/bin/env bash
# Checks out Scala Native v0.5.12, whose unit tests scripts/sn-tests.sh compiles, into
# ref/scala-native, and the scalalib/ of the commit that adds Scala 3.9.0, whose patches the
# published scalalib 3.9.0+0.5.12 was built with, into ref/scala-native-scalalib.
set -euo pipefail
repo=https://github.com/scala-native/scala-native.git
dir=$(cd "$(dirname "$0")/.." && pwd)/ref/scala-native
commit=65dd513df188300d0180514ad828308facc5ca61
if [ ! -d "$dir" ]; then
  git clone -q --depth 1 --branch v0.5.12 "$repo" "$dir"
fi
if [ ! -e "$dir/.git" ] || [ "$(git -C "$dir" rev-parse HEAD)" != "$commit" ]; then
  echo "$dir is not at v0.5.12 ($commit)" >&2
  exit 1
fi
scalalib=$dir-scalalib
scalalib_commit=07ff189db3d2877061bc1f4925e4c2d9dca0ce3f
if [ ! -d "$scalalib" ]; then
  git clone -q --depth 1 --filter=blob:none --no-checkout "$repo" "$scalalib"
  git -C "$scalalib" fetch -q --depth 1 --filter=blob:none origin "$scalalib_commit"
  git -C "$scalalib" sparse-checkout set scalalib
  git -C "$scalalib" checkout -q "$scalalib_commit"
fi
if [ "$(git -C "$scalalib" rev-parse HEAD)" != "$scalalib_commit" ]; then
  echo "$scalalib is not at $scalalib_commit" >&2
  exit 1
fi
