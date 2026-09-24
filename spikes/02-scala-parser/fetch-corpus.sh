#!/usr/bin/env bash
# Usage: ./fetch-corpus.sh <dir>
# Fills <dir>/stdlib (scala-library 3.9.0 sources) and <dir>/scala3 (sparse clone at tag 3.9.0).
set -euo pipefail
dir=${1:?usage: fetch-corpus.sh <dir>}
mkdir -p "$dir/stdlib"
jar=$(coursier fetch --sources --intransitive org.scala-lang:scala-library:3.9.0 | grep sources)
unzip -q -o "$jar" -d "$dir/stdlib"
if [ ! -d "$dir/scala3" ]; then
  git clone -q --depth 1 --branch 3.9.0 --filter=blob:none --sparse https://github.com/scala/scala3.git "$dir/scala3"
fi
git -C "$dir/scala3" sparse-checkout set tests/run tests/pos compiler/src/dotty/tools/dotc docs/_docs/reference
