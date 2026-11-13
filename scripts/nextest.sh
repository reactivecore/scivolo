#!/usr/bin/env bash
# Runs cargo-nextest 0.9.150 with the given arguments: the one on PATH, else one downloaded into
# tmp/tools/nextest-0.9.150 and checked against its sha256.
set -euo pipefail
version=0.9.150
if command -v cargo-nextest >/dev/null && [ "$(cargo-nextest nextest --version | head -1)" = "cargo-nextest $version" ]; then
  exec cargo-nextest nextest "$@"
fi
dir=$(cd "$(dirname "$0")/.." && pwd)/tmp/tools/nextest-$version
bin=$dir/cargo-nextest
if [ ! -x "$bin" ]; then
  case "$(uname -s)-$(uname -m)" in
    Darwin-*) target=universal-apple-darwin sha=82a759b4d218d38359424196ab244c470e8f7c7d149c814fbbeef4d67c3c301a ;;
    Linux-x86_64) target=x86_64-unknown-linux-musl sha=84f0f9ba75d0b6fa117bc418d49747ba78e20e4583057f6fa628f1b17c17234d ;;
    *) echo "no cargo-nextest $version for $(uname -sm): install it on PATH" >&2; exit 1 ;;
  esac
  mkdir -p "$dir"
  curl --proto '=https' --tlsv1.2 -sSfL -o "$dir/nextest.tar.gz" \
    "https://github.com/nextest-rs/nextest/releases/download/cargo-nextest-$version/cargo-nextest-$version-$target.tar.gz"
  echo "$sha  $dir/nextest.tar.gz" | shasum -a 256 -c --quiet -
  tar -xzf "$dir/nextest.tar.gz" -C "$dir" cargo-nextest
  rm "$dir/nextest.tar.gz"
fi
exec "$bin" nextest "$@"
