#!/usr/bin/env bash
# Downloads scala-cli 1.14.0 for macOS arm64 into tmp/tools and checks its sha256.
set -euo pipefail
dir=$(cd "$(dirname "$0")/.." && pwd)/tmp/tools
bin=$dir/scala-cli
if [ ! -x "$bin" ]; then
  mkdir -p "$dir"
  curl --proto '=https' -sSfL \
    https://github.com/VirtusLab/scala-cli/releases/download/v1.14.0/scala-cli-aarch64-apple-darwin.gz \
    -o "$bin.gz"
  echo "5b8fae8465440676a4a6eb7f4af67467585d982c8b35055a468a0e252e1fe545  $bin.gz" | shasum -a 256 -c --quiet -
  gunzip -f "$bin.gz"
  chmod +x "$bin"
fi
